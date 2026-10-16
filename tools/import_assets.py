#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
from import_process import run as run_import_process
import sys
import tempfile

TOOLS = Path(__file__).resolve().parent
STAGES = ['scn', 'player', 'animations', 'emotes', 'chat', 'hud', 'loading',
          'minimap', 'music', 'movement_audio', 'effects', 'melee', 'practice', 'parity']
REQUIRED = ['circle2.json', 'station2.json', 'neden1.json', 'player.json',
            'player-animations.bin', 'ui/loading/selection.json', 'ui/verdana.ttf',
            'ui/hud/hud-source.json', 'melee/melee.json', 'melee/melee-animations.bin',
            'melee/reactions.json', 'practice/practice.json', 'practice/poses.bin',
            'emotes/emotes.json', 'emotes/poses.bin', 'ui/weapon-switch.json',
            'ui/minimap/minimap.json', 'movement-audio.json', 'movement-effects.json']


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def tool_digest():
    inputs = sorted(TOOLS.glob('*.py')) + sorted((TOOLS / 'data').glob('*.json'))
    return hashlib.sha256(''.join(p.name + digest(p) for p in inputs).encode()).hexdigest()


def source_paths(value, index=None):
    path = Path(value).expanduser().resolve()
    resources = path if path.name == '_resources' else path / '_resources'
    if not resources.is_dir():
        raise ValueError(f'Cannot find _resources in {path}')
    if index:
        index = Path(index).expanduser().resolve()
        if not index.is_file():
            raise ValueError(f'Cannot find resource index: {index}')
    else:
        index = next((p for p in [resources.parent / 'resource.s4hd', resources.parent / 'resources.s4hd'] if p.is_file()), None)
    return resources, index


def ready(destination, signature=None):
    assets = destination / 'assets'
    try:
        manifest = json.loads((assets / 'import-manifest.json').read_text())
        if manifest['importer_sha256'] != tool_digest():
            return False
        if signature is not None and manifest['source'] != signature:
            return False
        if not all((assets / name).is_file() for name in REQUIRED):
            return False
        for record in manifest['files']:
            path = assets / record['path']
            if not path.resolve().is_relative_to(assets.resolve()) or not path.is_file() or digest(path) != record['sha256']:
                return False
        return True
    except (OSError, ValueError, KeyError, TypeError):
        return False


def choose_source():
    try:
        import tkinter as tk
        from tkinter import filedialog
        root = tk.Tk()
        root.withdraw()
        selected = filedialog.askdirectory(title='Select S4 client folder (containing _resources and resource.s4hd)')
        root.destroy()
        if selected:
            return selected
    except (ImportError, RuntimeError):
        pass
    except Exception as cause:
        print(f'Folder picker unavailable: {cause}', file=sys.stderr)
    if sys.stdin.isatty():
        return input('S4 client folder (or _resources path): ').strip().strip('"')
    raise ValueError('Select your client with --resources /path/to/client. Include its matching resource.s4hd if revisions are ambiguous.')


def run_stage(command, environment, destination):
    log = destination / 'logs/asset-import.log'
    log.parent.mkdir(exist_ok=True)
    with log.open('a') as output:
        result = run_import_process(command, cwd=TOOLS, env=environment, stdout=output, stderr=subprocess.STDOUT)
    if result.returncode:
        print('\n'.join(log.read_text(errors='replace').splitlines()[-14:]), file=sys.stderr)
        raise ValueError(f'This client could not be imported. Details: {log}')


def prepare(resources, index, destination):
    assets = destination / 'assets'
    if resources.is_relative_to(assets) or assets.is_relative_to(resources):
        raise ValueError('The input _resources folder must be separate from the generated assets folder')
    source = {'resources': str(resources), 'index_sha256': digest(index) if index else None}
    if ready(destination, source):
        print('Imported assets are up to date.', flush=True)
        return
    environment = dict(os.environ, ESPER_RESOURCES=str(resources), ESPER_RESOURCE_INDEX=str(index) if index else '', PYTHONDONTWRITEBYTECODE='1')
    if index is None:
        environment.pop('ESPER_RESOURCE_INDEX', None)
    with tempfile.TemporaryDirectory(prefix='.esper-import-', dir=destination) as temporary:
        staging = Path(temporary)
        for name in ('assets', 'config', 'research'):
            (staging / name).mkdir()
        environment['ESPER_IMPORT_ROOT'] = str(staging)
        run_stage([sys.executable, '-c', 'from PIL import Image; from resource_reader import _crypto, _lzo, index_by_name; _crypto(); _lzo(); index_by_name()'], environment, destination)
        for number, stage in enumerate(STAGES, 1):
            print(f'Importing {stage.replace("_", " ")} ({number}/{len(STAGES)})…', flush=True)
            run_stage([sys.executable, str(TOOLS / f'asset_{stage}.py')], environment, destination)
        generated = staging / 'assets'
        shutil.copyfile(staging / 'config/music.json', generated / 'music.json')
        shutil.move(str(staging / 'research'), str(generated / 'import-evidence'))
        missing = [name for name in REQUIRED if not (generated / name).is_file()]
        if missing:
            raise ValueError('Import incomplete: ' + ', '.join(missing))
        manifest = {'schema': 1, 'source': source, 'importer_sha256': tool_digest(),
                    'files': [{'path': p.relative_to(generated).as_posix(), 'sha256': digest(p)}
                              for p in sorted(generated.rglob('*')) if p.is_file()]}
        (generated / 'import-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        backup = staging / 'previous-assets'
        if assets.exists():
            assets.rename(backup)
        try:
            generated.rename(assets)
        except BaseException:
            if backup.exists():
                backup.rename(assets)
            raise
    print('Assets imported successfully. Starting ESPERUST.', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--resources', type=Path, help='Client folder or its _resources folder')
    parser.add_argument('--index', type=Path, help='Matching resource.s4hd, if stored elsewhere')
    parser.add_argument('--destination', type=Path, default=TOOLS.parent)
    parser.add_argument('--ensure', action='store_true', help='Reuse a verified cache or prompt on first run')
    args = parser.parse_args()
    destination = args.destination.expanduser().resolve()
    destination.mkdir(parents=True, exist_ok=True)
    if args.ensure and not args.resources and not args.index and ready(destination):
        return
    value = args.resources or os.environ.get('ESPER_RESOURCES')
    if not value and (destination / '_resources').is_dir():
        value = destination
    if not value:
        value = choose_source()
    resources, index = source_paths(value, args.index)
    prepare(resources, index, destination)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as cause:
        print(f'Asset import failed: {cause}\nExisting imported assets were preserved.', file=sys.stderr)
        sys.exit(1)
