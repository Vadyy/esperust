#!/usr/bin/env python3
import hashlib
import json
from pathlib import Path
from asset_melee import OUTPUT, RESEARCH, extract, lua_functions, symbolic_calls, sequence, mesh_animation
from asset_hud import gui_nodes
from asset_extract import extract as extract_shared
from PIL import Image

def weapon_switch():
    nodes = [node for node in gui_nodes(extract_shared('resources/gui/new/maingame_ingame.gui').read_bytes()) if 0xcac9 <= int(node['offset'], 16) < 0xe307]
    result = {'source': 'resources/gui/new/maingame_ingame.gui', 'fade_in_ms': 200, 'hold_ms': 800, 'fade_out_ms': 200, 'slots': []}
    for slot in [1, 2, 3]:
        group = next(n for n in nodes if n['name'] == f'slot{slot}_GroupBox')
        start = nodes.index(group)
        children = []
        for node in nodes[start + 1:]:
            if node['name'].startswith('slot') and node['name'].endswith('_GroupBox'): break
            children.append(node)
        record = {'number': slot, 'rect': group['canvas_rect'], 'gui_offset': group['offset']}
        for state in ['on', 'off']:
            node = next(n for n in children if n['name'] == f'icon_{state}_Image')
            source = 'resources/gui/new/' + node['textures'][0]['file'].lower().replace('.tga', '.dds')
            path = OUTPUT / f'ui/weapon-slot{slot}-{state}.png'
            Image.open(extract_shared(source)).convert('RGBA').save(path)
            record[state] = str(path.relative_to(OUTPUT))
        record['icon_rect'] = next(n['canvas_rect'] for n in children if n['name'] == 'icon_image_GroupBox')
        record['key_rect'] = next(n['canvas_rect'] for n in children if n['name'] == 'key_Label')
        result['slots'].append(record)
    (OUTPUT / 'ui/weapon-switch.json').write_text(json.dumps(result, indent=2) + '\n')

def main():
    weapon_switch()
    functions = lua_functions(RESEARCH / 'resources/script/actorstates_gameplay.lua')
    builders = {name: symbolic_calls(functions[name]) for name in ['ActorState_Stun', 'ActorState_Death']}
    sources = {value.lower() for calls in builders.values() for call in calls
               for value in call['arguments'] if isinstance(value, str) and value.lower().endswith(('.seq', '.ogg'))}
    effects = {'sequences': {}, 'models': {}}
    audio = {}
    for source in sorted(sources):
        if source.endswith('.seq'):
            definition, models = sequence(source)
            effects['sequences'][Path(source).stem] = definition
            for model in models:
                effects['models'][Path(model).stem] = mesh_animation(model)
        else:
            path = extract(source)
            audio[Path(source).stem] = {'source': source, 'path': str(path.relative_to(OUTPUT)), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
    result = {'builders': builders, 'effects': effects, 'audio': audio}
    path = OUTPUT / 'melee/reactions.json'
    path.write_text(json.dumps(result, separators=(',', ':')) + '\n')
    print(path, len(effects['sequences']), 'sequences', len(effects['models']), 'models')

if __name__ == '__main__':
    main()
