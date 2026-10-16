#!/usr/bin/env python3
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import tarfile
import zipfile


def assemble(python_archive, pillow_wheel, native_dir, licenses, output, platform):
    if output.exists():
        raise ValueError(f'Output already exists: {output}')
    output.mkdir(parents=True)
    try:
        with tarfile.open(python_archive) as archive:
            archive.extractall(output, filter='data')
        site = output / ('python/Lib/site-packages' if platform.startswith('windows') else 'python/lib/python3.12/site-packages')
        site.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(pillow_wheel) as wheel:
            for member in wheel.infolist():
                path = Path(member.filename)
                if path.is_absolute() or '..' in path.parts:
                    raise ValueError('Unsafe wheel path')
            wheel.extractall(site)
        for path in site.glob('pip*'):
            if path.is_dir():
                shutil.rmtree(path)
        shutil.copytree(native_dir, output / 'native')
        shutil.copytree(licenses, output / 'licenses')
        crypto = output / 'python/DLLs/libcrypto-3-x64.dll'
        if crypto.is_file():
            shutil.copy2(crypto, output / 'native' / crypto.name)
        executable = output / ('python/python.exe' if platform.startswith('windows') else 'python/bin/python3')
        if not executable.is_file():
            raise ValueError('Missing portable Python interpreter')
        from package_release import check_binary
        check_binary(executable, platform)
        metadata = {'platform': platform, 'python': '3.12',
                    'inputs': [{'file': p.name, 'sha256': hashlib.file_digest(p.open('rb'), 'sha256').hexdigest()}
                               for p in [python_archive, pillow_wheel]]}
        (output / 'runtime.json').write_text(json.dumps(metadata, indent=2) + '\n')
    except BaseException:
        shutil.rmtree(output)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for argument in ['python-archive', 'pillow-wheel', 'native-dir', 'licenses', 'output']:
        parser.add_argument('--' + argument, type=Path, required=True)
    parser.add_argument('--platform', required=True)
    args = parser.parse_args()
    assemble(args.python_archive, args.pillow_wheel, args.native_dir, args.licenses, args.output, args.platform)


if __name__ == '__main__':
    main()
