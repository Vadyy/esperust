#!/usr/bin/env python3
from pathlib import Path
import hashlib
import json
import re
import struct
import xml.etree.ElementTree as ET
from PIL import Image
from asset_extract import extract, OUTPUT, RECREATION
from resource_reader import extract_bytes
from asset_scn import parse as parse_scene


GUI = 'resources/gui/new/maingame_ingame.gui'
ATLAS = 'resources/gui/new/ingame2.dds'


def gui_nodes(data):
    nodes = []
    for match in re.finditer(rb'(CGUI_[\w]+)\x00\x00\x00\x80\x3f([^\0]+)\x00', data):
        start = match.end()
        end = data.find(b'CGUI_', start)
        end = len(data) if end < 0 else end
        textures = []
        for texture in re.finditer(rb'([a-zA-Z0-9_]+\.(?:tga|dds|bmp))\x00', data[start:end]):
            blend, flip, *uv = struct.unpack_from('<2i4f', data, start + texture.end())
            textures.append({'file': texture[1].decode(), 'blend': blend,
                             'flip': flip, 'uv': uv})
        global_rect = list(struct.unpack_from('<4i', data, start + 16))
        scene_files = [scene[1].decode() for scene in re.finditer(
            rb'([a-zA-Z0-9_/.\\:\-]+\.scn)\x00', data[start:end])]
        nodes.append({'offset': hex(match.start()), 'type': match[1].decode(),
                      'name': match[2].decode(),
                      'local': list(struct.unpack_from('<4i', data, start)),
                      'global': global_rect,
                      'canvas_rect': [global_rect[0] - 1, global_rect[1] - 1,
                                      global_rect[2] - global_rect[0],
                                      global_rect[3] - global_rect[1]],
                      'opacity': struct.unpack_from('<f', data, start + 52)[0],
                      'textures': textures, 'scene_files': scene_files})
        if match[1] == b'CGUI_Label':
            segment = data[start:end]
            font = re.search(rb'(\d{4}_\d+)\x00', segment)
            if font:
                tail = font.end()
                alignment = struct.unpack_from('<i', segment, font.start() - 4)[0]
                font_style, *colors = struct.unpack_from('<i8f', segment, tail)
                texts = {}
                language_count = tail + 44
                if struct.unpack_from('<I', segment, language_count)[0] == 10:
                    cursor = language_count + 4
                    for _ in range(10):
                        language = struct.unpack_from('<I', segment, cursor)[0]
                        cursor += 4
                        terminator = segment.index(b'\0', cursor)
                        texts[str(language)] = segment[cursor:terminator].decode('utf-8')
                        cursor = terminator + 1
                nodes[-1]['label'] = {'font_key': font[1].decode(),
                                     'alignment_enum': alignment,
                                     'alignment_offset': hex(start + font.start() - 4),
                                     'font_style_enum': font_style,
                                     'font_style_offset': hex(start + tail),
                                     'color': colors[:4], 'shadow_color': colors[4:],
                                     'language_text': texts,
                                     'english_text': texts.get('2')}
    return nodes


def main():
    out = OUTPUT / 'ui/hud'
    out.mkdir(parents=True, exist_ok=True)
    gui_path = extract(GUI)
    nodes = gui_nodes(gui_path.read_bytes())
    by_offset = {node['offset']: node for node in nodes}
    sources = {GUI: gui_path}
    sprites = {}
    scenes = {}

    def save(name, source, crop=None, node=None, texture=0, flip=False, rect=None):
        path = extract(source)
        sources[source] = path
        image = Image.open(path).convert('RGBA')
        original_size = list(image.size)
        if node:
            record = by_offset[node]
            material = record['textures'][texture]
            crop = [round(material['uv'][i] * image.size[i % 2]) for i in range(4)]
            flip = bool(material['flip'])
            rect = record['canvas_rect']
        if crop:
            image = image.crop(crop)
        if flip:
            image = image.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
        image.save(out / f'{name}.png')
        sprites[name] = {'file': f'ui/hud/{name}.png', 'size': list(image.size),
                         'source': source, 'source_size': original_size,
                         'source_crop': crop, 'flip_x': flip,
                         'canvas_rect': rect, 'gui_node_offset': node}

    save('hp-background', ATLAS, node='0xee72', texture=0)
    save('hp-fill', ATLAS, node='0xee72', texture=1)
    save('hp-damage', ATLAS, node='0xee72', texture=2)
    save('sp-background', ATLAS, node='0xe61c', texture=0)
    save('sp-fill', ATLAS, node='0xe61c', texture=1)
    save('skill-frame', ATLAS, node='0xf42d')
    save('skill-frame-red', ATLAS, node='0xf52f')
    save('ammo-frame', ATLAS, node='0xfdc5')
    save('ammo-frame-red', ATLAS, node='0xfec8')
    save('infinity-main', ATLAS, node='0xfab5')
    save('infinity-reserve', ATLAS, node='0xf8a0')
    save('hp-slash', ATLAS, node='0xebe7')
    save('ammo-slash', ATLAS, node='0xfcca')
    save('score-frame', ATLAS, node='0x2f1b8')
    save('chaser-score-marker', ATLAS, node='0x2355a')
    save('chaser-score-bonus', ATLAS, node='0x21abc')
    save('chaser-score-back', ATLAS, node='0x24f81')
    save('skill-hp30', 'resources/image/action/passive1.dds', rect=[83, 627, 64, 64])
    save('skill-sp40', 'resources/image/action/passive2.dds', rect=[83, 627, 64, 64])
    save('crosshair', 'resources/image/weapon/crosshair_sword_plasma_1.dds', rect=[480, 352, 64, 64])
    for digit in range(10):
        for color in ('white', 'green', 'red'):
            save(f'digit-{color}-{digit}',
                 f'resources/gui/new/ingame_score_number_{color}_{digit}.dds')
        save(f'timer-{digit}', f'resources/gui/new/ingame_score_time_{digit}.dds')
        save(f'chaser-timer-{digit}', f'resources/gui/new/time_{digit}.dds')
        save(f'chaser-digit-{digit}', f'resources/model/interface/time_num{digit}.dds')
        for suffix in ('', '_remove'):
            source = f'resources/model/interface/time_num{digit}{suffix}.scn'
            path = extract(source)
            sources[source] = path
            scene = parse_scene(path)
            scene_file = f'chaser-digit-{digit}{suffix}.json'
            (out / scene_file).write_text(json.dumps(scene, indent=2) + '\n')
            scenes[f'chaser-digit-{digit}{suffix}'] = {'file': f'ui/hud/{scene_file}', 'source': source}
    save('timer-colon', 'resources/gui/new/ingame_score_time_colon.dds')
    for slot in range(1, 4):
        for state in ('on', 'off'):
            save(f'weapon-slot-{slot}-{state}',
                 f'resources/gui/new/weapon_slot{slot}_{state}.dds')
    font_source = OUTPUT / 'original/language/font_config.x7'
    font_source.parent.mkdir(parents=True, exist_ok=True)
    font_source.write_bytes(extract_bytes('language/font_config.x7', inner=True))
    font_tree = ET.parse(font_source)
    fonts = {}
    for font in font_tree.getroot().iter('font'):
        english = font.find('eng')
        if english is not None:
            fonts[font.attrib['name']] = english.attrib
    label_offsets = {'team_state': '0x2e44f', 'chaser_name': '0x2110b',
                     'chaser_first_place': '0x27850', 'chaser_my_score': '0x2767b',
                     'chaser_total_score': '0x279e1'}
    labels = {}
    for name, offset in label_offsets.items():
        node = by_offset[offset]
        label = dict(node['label'])
        label.update({'canvas_rect': node['canvas_rect'], 'gui_node_offset': offset,
                      'english_font': fonts[label['font_key']]})
        labels[name] = label
    item_sources = OUTPUT / 'original/xml'
    item_path = item_sources / 'item.x7'
    iteminfo_path = item_sources / 'iteminfo.x7'
    weapon_path = item_sources / 'weapon.x7'
    item_sources.mkdir(parents=True, exist_ok=True)
    for source in (item_path, iteminfo_path, weapon_path):
        source.write_bytes(extract_bytes('xml/' + source.name, inner=True))
    hp_item = ET.parse(item_path).getroot().find("item[@item_key='3000001']")
    sp_item = ET.parse(item_path).getroot().find("item[@item_key='3000002']")
    hp_legacy = next(item for item in ET.parse(iteminfo_path).getroot().iter('item')
                     if item.attrib.get('number') == '1' and item.find('client/icon') is not None
                     and item.find('client/icon').attrib.get('image') == 'passive1.tga')
    plasma_weapons = [weapon for weapon in ET.parse(weapon_path).getroot().iter('weapon')
                      if weapon.find('resources') is not None and
                      weapon.find('resources').attrib.get('crosshair_file') == 'crosshair_sword_plasma']
    provenance = {
        'skill-sp40': {'item_key': sp_item.attrib['item_key'],
                       'name_key': sp_item.find('base').attrib['name_key'],
                       'icon_image': sp_item.find('graphic').attrib['icon_image'],
                       'sp_bonus': 40,
                       'bonus_source': 'research/sp-skill-source/xml/item_effect.x7',
                       'bonus_effect_id': '02040'},
        'skill-hp30': {'item_key': hp_item.attrib['item_key'], 'name_key': hp_item.find('base').attrib['name_key'],
                       'icon_image': hp_item.find('graphic').attrib['icon_image'],
                       'legacy_item_number': hp_legacy.attrib['number'],
                       'legacy_icon_image': hp_legacy.find('client/icon').attrib['image']},
        'crosshair': {'weapon_keys': [weapon.attrib['item_key'] for weapon in plasma_weapons],
                      'crosshair_file': 'crosshair_sword_plasma'},
        'decoded_sources': [{'path': str(path.relative_to(RECREATION)),
                             'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                            for path in (item_path, iteminfo_path, weapon_path)],
    }
    gauges = {}
    for name, node_offset, fields_offset, reverse_offset in (
            ('hp', '0xee72', 0xf02b, 0xf0a3), ('sp', '0xe61c', 0xe7df, 0xe857)):
        data = gui_path.read_bytes()
        values = struct.unpack_from('<7i', data, fields_offset)
        fields = dict(zip(('fill_direction', 'fill_margin_x', 'fill_margin_y', 'fill_time',
                           'after_fill_time', 'after_fill_wait_time', 'after_fill_visible_anim_time'), values))
        fields.update({'reverse_uv': bool(data[reverse_offset]), 'gui_node_offset': node_offset,
                       'fields_byte_offset': hex(fields_offset), 'reverse_uv_byte_offset': hex(reverse_offset)})
        by_offset[node_offset]['progress'] = fields
        gauges[name] = fields
    manifest = {
        'source': 'EU1162 supplied client resource archive',
        'base_canvas': [1024, 768],
        'rect_convention': 'canvas_rect = [x,y,width,height]; serialized global is [left,top,right,bottom]. Root Window_MainGame at (1,1) is removed.',
        'sprites': sprites,
        'scenes': scenes,
        'labels': labels,
        'gauges': gauges,
        'item_provenance': provenance,
        'font_mapping': {'source': 'language/font_config.x7',
                         'decoded_source': 'assets/original/language/font_config.x7',
                         'decoded_sha256': hashlib.sha256(font_source.read_bytes()).hexdigest()},
        'layouts': {
            'hp_digits': {'current': [435, 714, 66, 22], 'maximum': [525, 714, 66, 22], 'digit_size': [22, 22], 'source_nodes': ['0xea3b', '0xeb11']},
            'team_score': {'alpha_one_digit': [411, 20, 37, 28], 'beta_one_digit': [576, 20, 37, 28],
                           'timer': [[478, 22, 16, 32], [491, 22, 16, 32], [503, 22, 16, 32], [515, 22, 16, 32], [527, 22, 16, 32]],
                           'state_label': [256, 5, 512, 15], 'source_nodes': ['0x2f0e3', '0x2ee64', '0x2e88d', '0x2e78a', '0x2e990', '0x2e687', '0x2e584', '0x2e44f']},
            'chaser': {'timer_group': [410, 0, 205, 80], 'timer_digits': [[410, 0, 65, 76], [480, 0, 65, 76], [550, 0, 65, 76]],
                       'chaser_name_label': [256, 80, 512, 15],
                       'first_place_score': [10, 50, 37, 28], 'my_score': [10, 110, 37, 28],
                       'first_place_label': [10, 30, 50, 15], 'my_score_label': [10, 90, 50, 15],
                       'score_proceed_label': [40, 5, 50, 15],
                       'source_nodes': ['0x21242', '0x21511', '0x21402', '0x212f3', '0x2110b', '0x2742e', '0x2712e', '0x27850', '0x2767b', '0x279e1']},
            'quick_slots': {'group': by_offset['0xcac9']['canvas_rect'],
                            'slots': [by_offset[offset]['canvas_rect'] for offset in ('0xdb17', '0xd34a', '0xcb7d')],
                            'source_nodes': ['0xcac9', '0xdb17', '0xd34a', '0xcb7d']},
        },
        'sources': [{'path': source, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                    for source, path in sorted(sources.items())],
        'nodes': nodes,
    }
    (out / 'hud-source.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(f'Imported {len(sprites)} original HUD image regions and {len(nodes)} serialized GUI nodes into {out}')


if __name__ == '__main__':
    main()
