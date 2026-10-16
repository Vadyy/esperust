#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
from import_context import INDEX_PATH
from resource_reader import (ROOT, CLIENT, CLIENT_SHA256, INDEX_SHA256,
    index_by_name, safe_name, decode_resource, decode_inner, sha256, stored_bytes)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('names', nargs='+')
    args = parser.parse_args()
    entries = index_by_name()
    names = list(dict.fromkeys(safe_name(name) for name in args.names))
    for name in names:
        if name not in entries or not name.endswith(('.lua', '.x7')):
            raise ValueError(f'Unsupported text resource: {name}')
    args.output.mkdir(parents=True, exist_ok=False)
    results = []
    for name in names:
        entry = entries[name]
        stored = stored_bytes(entry)
        outer, compressed = decode_resource(entry, stored)
        plain = decode_inner(name, outer)
        dest = args.output / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(plain)
        results.append(dict(entry, stored_sha256=sha256(stored), outer_sha256=sha256(outer),
            outer_crc_pass=True, outer_lzo_compressed=compressed,
            inner_crc_pass=True if name.endswith('.x7') else None,
            decoded_size=len(plain), decoded_sha256=sha256(plain)))
    report = dict(schema='esper-resource-inner-extraction-v2',
        decoder_reference_executable_sha256=CLIENT_SHA256, index_sha256=sha256(INDEX_PATH.read_bytes()) if INDEX_PATH else None,
        producer_sha256=sha256(Path(__file__).read_bytes()),
        reader_sha256=sha256(Path(__file__).with_name('resource_reader.py').read_bytes()),
        resources=results)
    (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    (args.output / 'manifest.json').write_text(json.dumps({'files':[
        dict(path=str(p.relative_to(args.output)), size=p.stat().st_size, sha256=sha256(p.read_bytes()))
        for p in sorted(args.output.rglob('*')) if p.is_file()]}, indent=2) + '\n')
    print(json.dumps(results))


if __name__ == '__main__':
    main()
