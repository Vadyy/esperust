#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import sys


def portable_path(value, root):
    path = Path(value)
    if path.is_absolute():
        try:
            return path.relative_to(root).as_posix()
        except ValueError:
            pass
    return str(value)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--library-dir', action='append', default=[])
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    if sys.version_info < (3, 11):
        parser.error('Python 3.11+ is required')
    if args.library_dir:
        directories = [str(Path(p).resolve(strict=True)) for p in args.library_dir]
        os.environ['ESPER_NATIVE_LIBRARY_DIRS'] = os.pathsep.join(directories)
    from PIL import Image, DdsImagePlugin
    from resource_reader import _crypto, _lzo
    crypto, lzo = _crypto(), _lzo()
    try:
        import tkinter
    except ImportError:
        print('Tk is unavailable; supply --resources when starting the game.')
    configuration = {'python': portable_path(sys.executable, root),
                     'libraries': {'crypto': portable_path(crypto._name, root),
                                   'lzo2': portable_path(lzo._name, root)}}
    target = root / 'config/importer.json'
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_suffix('.json.tmp')
    temporary.write_text(json.dumps(configuration, indent=2) + '\n')
    temporary.replace(target)
    print(f'Importer ready: Python {sys.version.split()[0]}, Pillow {Image.__version__}, OpenSSL/SEED and LZO.')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, AttributeError, ImportError) as cause:
        print(f'Importer setup failed: {cause}', file=sys.stderr)
        sys.exit(1)
