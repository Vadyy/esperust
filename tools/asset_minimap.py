#!/usr/bin/env python3
import configparser
import hashlib
import json
from pathlib import Path
from PIL import Image
from asset_extract import extract, OUTPUT

MAPS = {
    'circle-2': 'bginfo-19dayssl.ini',
    'station-2': 'bginfo-sstation02.ini',
    'neden-1': 'bginfo-neden01.ini',
}

def main():
    out = OUTPUT / 'ui/minimap'
    out.mkdir(parents=True, exist_ok=True)
    sources = {}
    def record(name):
        path = extract(name)
        sources[name] = hashlib.sha256(path.read_bytes()).hexdigest()
        return path
    def image(name, source):
        source = source.lower().replace('\\', '/').replace('.tga', '.dds')
        decoded = Image.open(record(source)).convert('RGBA')
        decoded.save(out / f'{name}.png')
        return {'file': f'ui/minimap/{name}.png', 'source': source, 'size': list(decoded.size)}
    sprites = {name: image(name, f'resources/gui/new/{source}.dds') for name, source in {
        'mask': 'image_maingame_minamap_alpha',
        'rim': 'image_maingame_minamap_skin',
        'player': 'image_minamap_me',
        'ally': 'image_minamap_white',
        'enemy': 'image_minamap_red',
    }.items()}
    maps = {}
    for map_id, profile in MAPS.items():
        name = f'resources/mapinfo/{profile}'
        ini = configparser.ConfigParser(strict=False)
        ini.read_string(record(name).read_bytes().decode('cp949', errors='replace'))
        radar = ini['RADAR']
        spec = image(map_id, radar['Image'])
        enemy = radar.get('EnemyImage')
        maps[map_id] = {
            'profile': name, 'image': spec['file'],
            'enemy_image': image(map_id + '-enemy', enemy)['file'] if enemy else spec['file'],
            'bounds': [float(radar[key]) * .01 for key in ['Left', 'Right', 'Top', 'Bottom']],
            'range_radius': float(radar['RangeRadius']) * .01,
        }
    hud = json.loads((OUTPUT / 'ui/hud/hud-source.json').read_text())
    node = next(n for n in hud['nodes'] if n['name'] == 'GroupBox_radar')
    manifest = {
        'source': 'EU1162 unchanged resource archive and native radar renderer',
        'rect': node['canvas_rect'], 'gui_node': node['offset'],
        'player_size': [10, 10], 'actor_size': [8, 8],
        'sprites': sprites, 'maps': maps, 'sources_sha256': sources,
        'native_evidence': 'research/minimap-native.json',
        'coordinate_mapping': 'Source [x,y,-z]*0.01; radar centered on actor, radius in metres, camera-forward at top.',
    }
    (out / 'minimap.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(f'Exported {len(maps)} original radar maps and {len(sprites)} HUD sprites.')

if __name__ == '__main__':
    main()
