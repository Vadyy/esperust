#!/usr/bin/env python3
from functools import lru_cache
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from import_context import RECREATION, OUTPUT, RESEARCH, TOOLS
from resource_reader import index_by_name, extract_bytes, safe_name, decode_resource, stored_bytes

INDEX = index_by_name()


@lru_cache(maxsize=None)
def extract(name):
    name = safe_name(name.lower().replace('\\', '/'))
    data = extract_bytes(name)
    path = OUTPUT / 'original' / name
    if not path.resolve().is_relative_to((OUTPUT / 'original').resolve()):
        raise ValueError('Asset destination escapes original cache')
    if not path.exists() or path.read_bytes() != data:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    return path


if __name__ == '__main__':
    for name in sys.argv[1:]:
        print(extract(name))
