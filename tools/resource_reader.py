#!/usr/bin/env python3
from __future__ import annotations

import ctypes
import ctypes.util
import json
import os
import re
from functools import lru_cache
import hashlib
from pathlib import Path
import struct
import zlib

from import_context import RECREATION, RESOURCES, INDEX_PATH, TOOLS
ROOT = RECREATION.parent
CLIENT = RESOURCES.parent
_METADATA = json.loads((TOOLS / "data/client-tables.json").read_text())
CLIENT_SHA256 = _METADATA['source_executable_sha256']
INDEX_SHA256 = None
MAX_RESOURCE_BYTES = 256 * 1024 * 1024


def sha256(data):
    return hashlib.sha256(data).hexdigest()


class FormatError(ValueError):
    pass


@lru_cache(maxsize=1)
def original_tables():
    prefix, inner = bytes.fromhex(_METADATA['prefix_table']), bytes.fromhex(_METADATA['inner_tables'])
    if len(prefix) != 32 or len(inner) != 800:
        raise FormatError('Malformed bundled decoder tables')
    return prefix, inner


_DLL_DIRECTORIES = []


def load_library(name, alternatives):
    installation = TOOLS.parent
    try:
        configuration = json.loads((installation / 'config/importer.json').read_text())
    except (OSError, ValueError):
        configuration = {}
    configured = configuration.get('libraries', {}).get(name)
    candidates = []
    bundled = installation / "runtime/native"
    directories = [Path(p) for p in os.environ.get('ESPER_NATIVE_LIBRARY_DIRS', '').split(os.pathsep) if p]
    if bundled.is_dir():
        directories.insert(0, bundled)
    for directory in directories:
        candidates.extend(str(directory / filename) for filename in alternatives if (directory / filename).is_file())
    if configured:
        path = Path(configured)
        if not path.is_absolute() and len(path.parts) > 1:
            path = installation / path
        candidates.append(str(path))
        if path.is_absolute():
            directories.append(path.parent)
    if hasattr(os, 'add_dll_directory'):
        for directory in directories:
            if directory.is_dir():
                _DLL_DIRECTORIES.append(os.add_dll_directory(str(directory)))
    candidates.extend([ctypes.util.find_library(name), *alternatives])
    for candidate in candidates:
        if candidate:
            try:
                return ctypes.CDLL(candidate)
            except OSError:
                pass
    raise FormatError(f'Missing {name} library. Run the build script or install the importer dependencies (see README).')


def safe_name(name):
    if not isinstance(name, str) or not name or '\\' in name or ':' in name or '\0' in name:
        raise FormatError('Invalid resource path')
    if name.startswith('/') or any(p in ('', '.', '..') for p in name.split('/')):
        raise FormatError('Unsafe resource path')
    name.encode('ascii')
    return name


def decode_prefix(payload):
    key, _ = original_tables()
    result = bytearray(payload)
    for offset, value in enumerate(payload[:256]):
        result[offset] = ((value >> 1) | ((value & 1) << 7)) ^ key[offset & 31]
    return bytes(result)


def encode_checksum(name, payload):
    safe_name(name)
    key, _ = original_tables()
    fields = struct.pack('<II', zlib.crc32(payload), zlib.crc32(name.encode('ascii')))
    encoded = bytearray(8)
    for i, value in enumerate(fields):
        value ^= key[i]
        encoded[i] = ((value << 1) | (value >> 7)) & 255
    return format(int.from_bytes(encoded, 'little'), 'x')


def transpose_blocks(payload):
    result = bytearray(payload)
    boundary = len(payload) // 16 * 16
    for row in range(4):
        for column in range(4):
            result[row * 4 + column:boundary:16] = payload[column * 4 + row:boundary:16]
    return bytes(result)


def _rotate_table(payload, bank, row):
    _, table = original_tables()
    key = table[bank * 400 + row * 40:bank * 400 + row * 40 + 40]
    return bytes(((value ^ key[i % 40]) >> 1) | (((value ^ key[i % 40]) & 1) << 7)
                 for i, value in enumerate(payload))


@lru_cache(maxsize=1)
def _crypto():
    lib = load_library('crypto', ['libcrypto.so.3', 'libcrypto-3-x64.dll', 'libcrypto.3.dylib', 'libcrypto.dylib'])
    lib.SEED_set_key.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    lib.SEED_set_key.restype = None
    lib.SEED_encrypt.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
    lib.SEED_encrypt.restype = None
    return lib


def unwrap(payload):
    if not 40 <= len(payload) <= MAX_RESOURCE_BYTES:
        raise FormatError('Unsupported encrypted resource size')
    wire = transpose_blocks(payload)
    size = len(wire) - 32
    split = size // 3
    key = wire[split:split + 16]
    iv_offset = split * 2 + 16
    iv = wire[iv_offset:iv_offset + 16]
    ciphertext = wire[:split] + wire[split + 16:iv_offset] + wire[iv_offset + 16:]
    selector = ((size - 8) // 4) ^ 0xcd4802ef
    ciphertext = _rotate_table(ciphertext, selector % 2, selector % 10)
    schedule = ctypes.create_string_buffer(128)
    block = ctypes.create_string_buffer(16)
    crypto = _crypto()
    crypto.SEED_set_key(key, schedule)
    counter = int.from_bytes(iv, 'big')
    output = bytearray(size)
    for start in range(0, size, 16):
        crypto.SEED_encrypt(counter.to_bytes(16, 'big'), block, schedule)
        chunk = ciphertext[start:start + 16]
        output[start:start + len(chunk)] = bytes(a ^ b for a, b in zip(chunk, block.raw))
        counter = (counter + 1) & ((1 << 128) - 1)
    return transpose_blocks(output)


@lru_cache(maxsize=1)
def _lzo():
    lib = load_library('lzo2', ['liblzo2.so.2', 'liblzo2-2.dll', 'lzo2.dll', 'lzo2-2.dll', 'liblzo2.dll', 'liblzo2.2.dylib', 'liblzo2.dylib'])
    lib.lzo1x_decompress_safe.argtypes = [ctypes.c_void_p, ctypes.c_size_t,
        ctypes.c_void_p, ctypes.POINTER(ctypes.c_size_t), ctypes.c_void_p]
    lib.lzo1x_decompress_safe.restype = ctypes.c_int
    return lib


def decompress(payload, size):
    if not 0 < size <= MAX_RESOURCE_BYTES:
        raise FormatError('Decoded resource exceeds allocation limit')
    output = ctypes.create_string_buffer(size)
    actual = ctypes.c_size_t(size)
    status = _lzo().lzo1x_decompress_safe(payload, len(payload), output, ctypes.byref(actual), None)
    if status != 0 or actual.value != size:
        raise FormatError(f'LZO size/status mismatch: {actual.value}/{size}, {status}')
    return output.raw


def decode_resource(entry, stored):
    safe_name(entry['name'])
    expected = entry.get('size')
    if (expected is not None and not 0 < expected <= MAX_RESOURCE_BYTES) or not 0 < len(stored) <= MAX_RESOURCE_BYTES:
        raise FormatError('Unsupported resource size')
    buffer = bytearray(stored)
    pairs = min(len(buffer), 128) // 2
    if pairs:
        buffer[:pairs], buffer[-pairs:] = buffer[-pairs:][::-1], buffer[:pairs][::-1]
    compressed = 0 < len(buffer) < 0xfa000
    decoded = (decompress(bytes(buffer), expected) if expected else decompress_unknown(bytes(buffer))) if compressed else bytes(buffer)
    decoded = decode_prefix(decoded)
    if (expected is not None and len(decoded) != expected) or encode_checksum(entry['name'], decoded) != entry['checksum']:
        raise FormatError(f'Resource integrity mismatch: {entry["name"]}')
    return decoded, compressed


def decode_inner(name, outer):
    if not name.endswith(('.lua', '.x7')):
        raise FormatError('Inner decoding is only defined for Lua/X7')
    decoded = unwrap(outer)
    if name.endswith('.lua'):
        return decoded
    if len(decoded) < 12 or (len(decoded) - 8) % 4:
        raise FormatError('Malformed X7 interleaved carrier')
    size, checksum = struct.unpack_from('<II', decoded)
    size ^= 0xfe292513
    selector = ((len(decoded) - 8) // 4) ^ 0xcd4802ef
    lane = decoded[8::4]
    if lane != _rotate_table(decoded[9::4], 0, selector % 10):
        raise FormatError('X7 first redundant lane mismatch')
    if lane != _rotate_table(decoded[11::4], 1, selector % 10):
        raise FormatError('X7 second redundant lane mismatch')
    plain = decompress(lane, size)
    if zlib.crc32(plain) != checksum ^ 0xbad0a4b3:
        raise FormatError('X7 payload CRC mismatch')
    return plain


def parse_index(decoded):
    if len(decoded) < 8:
        raise FormatError('Truncated resource index')
    version, count = struct.unpack_from('<II', decoded)
    if version != 1 or not 0 < count <= 100000 or len(decoded) != 8 + count * 276:
        raise FormatError('Unsupported index version/count/length')
    entries = []
    names = set()
    for ordinal in range(count):
        start = 8 + ordinal * 276
        if struct.unpack_from('<I', decoded, start)[0] != 272:
            raise FormatError('Unsupported resource record size')
        record = decode_prefix(decoded[start + 4:start + 276])
        path_end = record.find(b'\0', 0, 256)
        if path_end < 1:
            raise FormatError('Unterminated resource name')
        name = safe_name(record[:path_end].decode('ascii'))
        checksum, size, unknown = struct.unpack_from('<QII', record, 256)
        if name in names or not 0 < size <= MAX_RESOURCE_BYTES:
            raise FormatError('Duplicate path or unsupported resource size')
        names.add(name)
        path_crc = struct.unpack_from('<I', decode_prefix(struct.pack('<Q', checksum)), 4)[0]
        if path_crc != zlib.crc32(name.encode('ascii')):
            raise FormatError(f'Index path CRC mismatch: {name}')
        entries.append(dict(ordinal=ordinal, record_offset=start + 4, name=name,
                            checksum=format(checksum, 'x'), size=size, unknown=unknown))
    return entries


def read_index(path=INDEX_PATH):
    if path is None:
        raise FormatError('No resource.s4hd selected')
    path = Path(path)
    if path.stat().st_size > MAX_RESOURCE_BYTES:
        raise FormatError('Resource index exceeds allocation limit')
    return parse_index(unwrap(path.read_bytes()))


def decompress_unknown(payload):
    size = max(4096, len(payload) * 2)
    while size <= MAX_RESOURCE_BYTES:
        output = ctypes.create_string_buffer(size)
        actual = ctypes.c_size_t(size)
        status = _lzo().lzo1x_decompress_safe(payload, len(payload), output, ctypes.byref(actual), None)
        if status == 0:
            return output.raw[:actual.value]
        if status != -5:
            raise FormatError(f'Malformed LZO resource (status {status})')
        if size == MAX_RESOURCE_BYTES:
            break
        size = min(size * 2, MAX_RESOURCE_BYTES)
    raise FormatError('Decoded resource exceeds allocation limit')


class ResourceCatalog:
    def __init__(self, resources=RESOURCES, index_path=INDEX_PATH):
        self.resources = Path(resources)
        self.entries = {e['name']: e for e in read_index(index_path)} if index_path else None
        self.by_crc = {}
        if self.entries is None:
            for path in self.resources.iterdir():
                if not path.is_file() or path.is_symlink() or not re.fullmatch(r'[0-9a-fA-F]{1,16}', path.name):
                    continue
                fields = decode_prefix(int(path.name, 16).to_bytes(8, 'little'))
                crc = int.from_bytes(fields[4:], 'little')
                self.by_crc.setdefault(crc, []).append(path.name)

    def __contains__(self, name):
        name = safe_name(name)
        return name in self.entries if self.entries is not None else zlib.crc32(name.encode('ascii')) in self.by_crc

    def __getitem__(self, name):
        name = safe_name(name)
        if self.entries is not None:
            try:
                return self.entries[name]
            except KeyError:
                raise FormatError(f'Required asset missing from this client: {name}') from None
        candidates = self.by_crc.get(zlib.crc32(name.encode('ascii')), [])
        if not candidates:
            raise FormatError(f'Required asset missing from this client: {name}')
        if len(candidates) != 1:
            raise FormatError(f'{name}: {len(candidates)} revisions found. Supply the matching resource.s4hd with --index.')
        return dict(name=name, checksum=candidates[0], size=None)

    def __iter__(self):
        if self.entries is None:
            raise FormatError('Enumerating unknown resource paths requires resource.s4hd')
        return iter(self.entries)

    def texture_candidates(self, filename, directory):
        if self.entries is not None:
            return [n for n in self.entries if n.endswith('/' + filename)]
        directories = [directory.rstrip('/'), 'resources/model/background', 'resources/model/weapon',
                       'resources/image', 'resources/uieffect', 'resources/gui/new', 'resources/model/interface']
        directories += ['resources/model/character/' + part for part in ['body', 'leg', 'hand', 'foot', 'face', 'hair']]
        return [n for n in dict.fromkeys(d + '/' + filename for d in directories) if n in self]


@lru_cache(maxsize=1)
def index_by_name():
    return ResourceCatalog()


def stored_bytes(entry):
    path = RESOURCES / entry['checksum']
    if path.is_symlink() or not path.is_file():
        raise FormatError(f'Missing resource backing file for {entry["name"]}: {path.name}')
    if path.stat().st_size > MAX_RESOURCE_BYTES:
        raise FormatError(f'Resource too large: {entry["name"]}')
    return path.read_bytes()


def extract_bytes(name, *, inner=False):
    entry = index_by_name()[safe_name(name)]
    outer, _ = decode_resource(entry, stored_bytes(entry))
    return decode_inner(name, outer) if inner else outer
