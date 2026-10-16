#!/usr/bin/env python3
from io import BytesIO
from pathlib import Path
import hashlib
import json
import sys

from PIL import Image

sys.dont_write_bytecode = True
from import_context import OUTPUT
from resource_reader import decode_resource, index_by_name, stored_bytes

OUT = OUTPUT / "ui/loading"
INDEX = index_by_name()
SOURCES = {}
MAPS = {
    "circle-2": "resources/image/loading_chaser_circle2.dds",
    "station-2": "resources/image/loading_touch_station2.dds",
    "neden-1": "resources/image/loading_death_neden1.dds",
}


def resource(name):
    entry = INDEX[name]
    stored = stored_bytes(entry)
    data, _ = decode_resource(entry, stored)
    SOURCES[name] = {"checksum": entry["checksum"], "checksum_pass": True,
                     "sha256": hashlib.sha256(data).hexdigest(),
                     "decoded_size": len(data)}
    return data


def image(name):
    result = Image.open(BytesIO(resource(name))).convert("RGBA")
    SOURCES[name]["size"] = list(result.size)
    return result


def save(result, name):
    result.save(OUT / name)
    return "ui/loading/" + name


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    selection = {"wallpapers": [], "maps": {}, "artwork": {},
                 "title_canvas": [1368, 768],
                 "title_center_rect": [172, 0, 1024, 768],
                 "map_canvas": [1024, 576],
                 "selection": "All nine title variants; only three supported maps; no login backgrounds."}
    for region in ("eu", "cn", "rc"):
        for number in range(1, 4):
            stem = f"resources/image/loading/_{region}_s4_title_{number:02}"
            center_source, side_source = stem + "_c.dds", stem + "_s.dds"
            center, side = image(center_source), image(side_source)
            center.putalpha(255)
            side.putalpha(255)
            result = Image.new("RGBA", (1368, 768))
            result.paste(side.crop((0, 0, 256, 1024)).resize(
                (172, 768), Image.Resampling.LANCZOS), (0, 0))
            result.paste(center.resize((1024, 768), Image.Resampling.LANCZOS), (172, 0))
            result.paste(side.crop((256, 0, 512, 1024)).resize(
                (172, 768), Image.Resampling.LANCZOS), (1196, 0))
            name = f"title-{region}-{number}.png"
            path = save(result, name)
            selection["wallpapers"].append(path)
            selection["artwork"][path] = {
                "sources": [center_source, side_source], "size": list(result.size),
                "id": f"{region.upper()}{number}",
                "composition": "1024x768 center with 172x768 left/right source side extensions",
                "source_side_crops": [[0, 0, 256, 1024], [256, 0, 512, 1024]],
                "opaque_background": True,
            }
    for map_id, source in MAPS.items():
        result = image(source).crop((0, 0, 1024, 576))
        path = save(result, f"map-{map_id}.png")
        selection["maps"][map_id] = path
        selection["artwork"][path] = {
            "sources": [source], "size": list(result.size),
            "crop": [0, 0, 1024, 576],
            "source_gui": "resources/image/loading_tip.gui",
            "source_uv": [0, 0, 1, 0.5625],
            "composition": "Original map/mode title, colored divider and Pentavision footer are baked into this source crop.",
        }
    title_fill = save(Image.new("RGBA", (1, 1), (255, 255, 255, 255)), "bar-title.png")
    map_fill = save(Image.new("RGBA", (1, 1), (255, 255, 255, 255)), "bar-map.png")
    selection["bars"] = {"title": {
        "rect": [-0.5, 695.5, 1369, 13], "canvas": [1368, 768], "fill": title_fill,
        "rect_normalized": [-0.5 / 1368, 695.5 / 768, 1369 / 1368, 13 / 768],
        "native_procedure": "0x6f01b0", "rgba": [1, 1, 1, 1],
    }}
    for map_id in MAPS:
        selection["bars"][map_id] = {
            "rect": [-0.5, 695.5, 1369, 21.5], "canvas": [1368, 768], "fill": map_fill,
            "rect_normalized": [-0.5 / 1368, 695.5 / 768, 1369 / 1368, 21.5 / 768],
            "native_procedure": "0x5f7890", "rgba": [1, 1, 1, 1],
        }
    selection["map_layout"] = {
        "source": "resources/image/loading_tip.gui", "base_canvas": [1024, 768],
        "background_rect": [-171, 0, 1368, 768],
        "footer": "Baked into the bottom of each exported wide map card.",
        "tip_note": "The legacy GUI's overlaid illustrated tips are license/tutorial-specific. Do not combine those with unrelated map cards. The newer mode_loading.xui uses a separate full layout.",
    }
    selection["ui"] = {
        "spinner": False, "text": False, "footer_overlay": False,
        "progress": "White untextured strip fills left to right over original baked track; no replacement empty track.",
        "native_x": "right = (logicalWidth + 1) * min(loaded / total, 1) - 0.5",
        "bar_evidence": "recreation/research/loading-bar-native.json",
        "source_executable_sha256": "359a43f403b6b6f918322d637e40276fadfe8de405196d338718dfa19d002029",
    }
    for path in selection["wallpapers"] + list(selection["maps"].values()):
        selection["artwork"][path]["sha256"] = hashlib.sha256(
            (OUT / Path(path).name).read_bytes()).hexdigest()
    selection["sources"] = SOURCES
    (OUT / "selection.json").write_text(json.dumps(selection, indent=2) + "\n")
    (OUT / "source.json").write_text(json.dumps({
        "source": "EU1162 supplied resource archive", "base_canvas": [1024, 768],
        "selection_manifest": "selection.json", "wallpapers_included": selection["wallpapers"],
        "maps_included": selection["maps"], "sources": SOURCES,
        "bars": selection["bars"], "ui": selection["ui"],
    }, indent=2) + "\n")
    for obsolete in ("loading1.png", "loading2.png", "loading3.png", "footer.png", "progress-track.png"):
        (OUT / obsolete).unlink(missing_ok=True)
    print(f"Imported {len(selection['wallpapers'])} selected titles, {len(selection['maps'])} map cards and original loading UI into {OUT}")


if __name__ == "__main__":
    main()
