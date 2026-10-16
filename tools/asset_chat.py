#!/usr/bin/env python3
from pathlib import Path
import hashlib
import json
import re
import shutil
import struct
from PIL import Image
from asset_extract import extract, OUTPUT


def main():
    gui_path = extract('resources/gui/new/chatting_window.gui')
    gui = gui_path.read_bytes()
    atlas_path = extract('resources/gui/new/ingame2.dds')
    language_path = extract('resources/gui/new/_eng_ingame3.dds')
    font_path = extract('resources/gui/verdana.ttf')
    out = OUTPUT / 'ui'
    out.mkdir(exist_ok=True)
    shutil.copyfile(font_path, out / 'verdana.ttf')
    atlas = Image.open(atlas_path)
    language = Image.open(language_path)
    atlas.crop((410, 233, 442, 265)).save(out / 'chat-edge.png')
    atlas.crop((457, 233, 473, 265)).save(out / 'chat-center.png')
    language.crop((246, 157, 288, 175)).save(out / 'chat-language.png')
    nodes = []
    for match in re.finditer(rb'(CGUI_[\w]+)\x00\x00\x00\x80\x3f([^\0]+)\x00', gui):
        start = match.end()
        local, global_ = struct.unpack_from('<4i', gui, start), struct.unpack_from('<4i', gui, start + 16)
        end = gui.find(b'CGUI_', start)
        end = len(gui) if end < 0 else end
        textures = []
        for texture in re.finditer(rb'([a-zA-Z0-9_]+\.tga)\x00', gui[start:end]):
            offset = start + texture.end()
            blend, flip, *uv = struct.unpack_from('<2i4f', gui, offset)
            textures.append({'file': texture[1].decode(), 'blend': blend, 'flip': flip, 'uv': uv})
        nodes.append({'offset': hex(match.start()), 'type': match[1].decode(), 'name': match[2].decode(),
                      'local': local, 'global': global_, 'opacity': struct.unpack_from('<f', gui, start + 52)[0],
                      'textures': textures})
    sources = [gui_path, atlas_path, language_path, font_path]
    manifest = {'source': 'EU1162 supplied resource archive', 'base_canvas': [1024, 768],
                'input': {'max_length': struct.unpack_from('<I', gui, 0x6ff)[0], 'max_byte_size': struct.unpack_from('<I', gui, 0x70c)[0], 'cursor_width': struct.unpack_from('<I', gui, 0x703)[0], 'cursor_visible_anim_time_ms': struct.unpack_from('<I', gui, 0x707)[0]},
                'font': {'gui_key': '2002_12', 'english_file': 'resources/gui/verdana.ttf', 'size': 11},
                'sources': [{'path': str(p.relative_to(OUTPUT)), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in sources],
                'nodes': nodes}
    (out / 'chat-source.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(f'Imported original chat font, 3 texture regions and {len(nodes)} GUI nodes into {out}')


if __name__ == '__main__':
    main()
