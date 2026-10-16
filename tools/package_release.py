#!/usr/bin/env python3
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def check_binary(path, platform):
    with path.open('rb') as stream:
        header = stream.read(64)
        if platform == 'linux-x86_64':
            valid = header[:6] == b'\x7fELF\x02\x01' and struct.unpack_from('<H', header, 18)[0] == 62
        elif platform == 'windows-x86_64':
            valid = header[:2] == b'MZ'
            if valid:
                stream.seek(struct.unpack_from('<I', header, 60)[0])
                pe = stream.read(6)
                valid = pe == b'PE\0\0\x64\x86'
        else:
            cpu = 0x100000c if platform == 'macos-arm64' else 0x1000007
            valid = header[:4] == b'\xcf\xfa\xed\xfe' and struct.unpack_from('<I', header, 4)[0] == cpu
    if not valid:
        raise ValueError(f'Executable does not match {platform}: {path}')


def package(binary, platform, output, runtime_dlls=(), runtime_licenses=(), importer_runtime=None):
    binary = binary.resolve(strict=True)
    check_binary(binary, platform)
    if importer_runtime is None:
        raise ValueError('A portable importer runtime is required; use --importer-runtime')
    metadata = json.loads((importer_runtime / 'runtime.json').read_text())
    if metadata['platform'] != platform:
        raise ValueError('Importer runtime does not match the release platform')
    interpreter = importer_runtime / ('python/python.exe' if platform.startswith('windows') else 'python/bin/python3')
    check_binary(interpreter, platform)
    native = importer_runtime / 'native'
    required = ['libcrypto-3-x64.dll', 'liblzo2-2.dll'] if platform.startswith('windows') else ['libcrypto.so.3', 'liblzo2.so.2'] if platform.startswith('linux') else ['libcrypto.3.dylib', 'liblzo2.2.dylib']
    if any(not (native / name).is_file() for name in required):
        raise ValueError('Portable importer is missing native decoding libraries')
    site = importer_runtime / ('python/Lib/site-packages' if platform.startswith('windows') else 'python/lib/python3.12/site-packages')
    if not (site / 'PIL/Image.py').is_file() or not list((site / 'PIL').glob('_imaging.*')):
        raise ValueError('Portable importer is missing Pillow')
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    name = f'esperust-{version}-{platform}'
    output.mkdir(parents=True, exist_ok=True)
    final = output / name
    extension = '.zip' if platform.startswith('windows') else '.tar.gz'
    archive = output / (name + extension)
    if final.exists() or archive.exists():
        raise ValueError(f'Release already exists: {name}; choose another output folder or move the previous build first')
    with tempfile.TemporaryDirectory(prefix='.esperust-release-', dir=output) as temporary:
        staging = Path(temporary) / name
        staging.mkdir()
        executable = 'esperust.exe' if platform.startswith('windows') else 'esperust'
        shutil.copy2(binary, staging / executable)
        shutil.copy2(ROOT / 'README.md', staging / 'README.md')
        shutil.copytree(importer_runtime, staging / 'runtime', symlinks=True,
                        ignore=shutil.ignore_patterns('__pycache__', '*.pyc'))
        shutil.copytree(ROOT / 'branding', staging / 'branding')
        for config in ['controller.json', 'music.json']:
            destination = staging / 'config' / config
            destination.parent.mkdir(exist_ok=True)
            shutil.copy2(ROOT / 'config' / config, destination)
        tools = list((ROOT / 'tools').glob('*.py')) + list((ROOT / 'tools/data').glob('*.json')) + [ROOT / 'tools/requirements.txt']
        for tool in tools:
            destination = staging / tool.relative_to(ROOT)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(tool, destination)
        for dll in runtime_dlls:
            shutil.copy2(dll, staging / dll.name)
        for license_file in runtime_licenses:
            (staging / 'licenses').mkdir(exist_ok=True)
            shutil.copy2(license_file, staging / 'licenses' / license_file.name)
        manifest = {'name': 'esperust', 'version': version, 'platform': platform,
                    'profile': 'release', 'debug_symbols': False, 'bundled_importer': True,
                    'files': [{'path': p.relative_to(staging).as_posix(),
                               'sha256': hashlib.file_digest(p.open('rb'), 'sha256').hexdigest()}
                              for p in sorted(staging.rglob('*')) if p.is_file()]}
        (staging / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        temporary_archive = Path(temporary) / archive.name
        if extension == '.zip':
            with zipfile.ZipFile(temporary_archive, 'w', zipfile.ZIP_DEFLATED) as bundle:
                for path in sorted(staging.rglob('*')):
                    if path.is_file():
                        bundle.write(path, path.relative_to(staging.parent))
        else:
            with tarfile.open(temporary_archive, 'w:gz') as bundle:
                bundle.add(staging, arcname=name)
        staging.rename(final)
        temporary_archive.rename(archive)
    return final, archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--platform', choices=['linux-x86_64','windows-x86_64','macos-arm64','macos-x86_64'], required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--importer-runtime', type=Path, required=True)
    parser.add_argument('--runtime-dll', type=Path, action='append', default=[])
    parser.add_argument('--runtime-license', type=Path, action='append', default=[])
    args = parser.parse_args()
    directory, archive = package(args.binary, args.platform, args.output.resolve(), args.runtime_dll, args.runtime_license, args.importer_runtime.resolve())
    print(directory)
    print(archive)


if __name__ == '__main__':
    main()
