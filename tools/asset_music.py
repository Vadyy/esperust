#!/usr/bin/env python3
from pathlib import Path
import configparser
import hashlib
import json
import shutil
import subprocess
from import_process import run as run_import_process
import sys
import tempfile
from ogg_metadata import vorbis_metadata
import xml.etree.ElementTree as ET

from asset_extract import extract, OUTPUT, RECREATION, TOOLS

ROOT = RECREATION
OPTIONS = ROOT / "research/original-music/language/default_option.x7"
MAPS = ROOT / "research/original-music/xml/map.x7"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    if not OPTIONS.is_file() or not MAPS.is_file():
        with tempfile.TemporaryDirectory(prefix="music-text-", dir=ROOT / "assets") as temporary:
            decoded = Path(temporary) / "decoded"
            run_import_process([sys.executable, str(TOOLS / "extract_inner_resources.py"),
                            str(decoded), "xml/map.x7", "language/default_option.x7"], check=True,
                           stdout=subprocess.DEVNULL)
            shutil.copytree(decoded, ROOT / "research/original-music", dirs_exist_ok=True)
    option = ET.parse(OPTIONS).getroot()
    map_nodes = ET.parse(MAPS).getroot().findall("map")
    tracks = {n.attrib["name_key"]: n.attrib["file"].lower()
              for n in option.findall("bgm/bgmlist/data")}
    playlists = {int(n.attrib["map_id"]): [tracks[n.attrib[key]]
                 for key in ("bgm1_key", "bgm2_key", "bgm3_key") if n.attrib[key]]
                 for n in option.findall("bgm/maplist/data")}
    definitions = []
    for slug, profile_name in (("station-2", "bginfo-sstation02.ini"),
                               ("neden-1", "bginfo-neden01.ini")):
        node = next(n for n in map_nodes if n.find("resourse").attrib["bginfo_path"].lower()
                    == "resources/mapinfo/" + profile_name)
        source_id = int(node.attrib["id"])
        definitions.append({"id": slug, "source_map_id": source_id,
                            "tracks": playlists[source_id],
                            "selection": "original default_option.x7 maplist"})

    circle_profile = extract("resources/mapinfo/bginfo-19dayssl.ini")
    ini = configparser.ConfigParser()
    ini.read_string(circle_profile.read_bytes().decode("cp949", errors="replace"))
    assert ini.get("STATIC", "object_2") == "ds_circle2.scn"
    assert not any(ini.has_option("BGM", f"bgm_{i}") for i in range(1, 4))
    circle_node = next(n for n in map_nodes if n.find("resourse").attrib["bginfo_path"].lower()
                       == "resources/mapinfo/bginfo-19dayssl.ini")
    definitions.insert(0, {"id": "circle-2", "source_map_id": int(circle_node.attrib["id"]),
                          "tracks": ["resources/bgm/nanorisk.ogg"],
                          "selection": "native non-chaser role; local solo defaults to runner",
                          "chaser_track": "resources/bgm/chaseyourself.ogg"})
    names = sorted({name for d in definitions for name in d["tracks"]}
                   | {"resources/bgm/chaseyourself.ogg"})
    resources = []
    for name in names:
        path = extract(name)
        assert path.read_bytes()[:4] == b"OggS", name
        stream = vorbis_metadata(path)
        assert stream["codec_name"] == "vorbis", name
        resources.append({"source": name, "asset_path": str(path.relative_to(OUTPUT)),
                          "sha256": digest(path), "bytes": path.stat().st_size,
                          "codec": stream["codec_name"], "sample_rate": int(stream["sample_rate"]),
                          "channels": stream["channels"], "duration_seconds": float(stream["duration"])})
    for definition in definitions:
        definition["tracks"] = ["original/" + name for name in definition["tracks"]]
        if "chaser_track" in definition:
            definition["chaser_track"] = "original/" + definition["chaser_track"]
    catalog = {"format_version": 1, "volume": int(option.find("sound/bgmvolume").attrib["value"]) / 100,
               "volume_status": "original 80/100 default; native mixer response curve not yet matched",
               "maps": definitions,
               "sources": [{"path": str(p.relative_to(ROOT)), "sha256": digest(p)}
                           for p in (OPTIONS, MAPS, circle_profile)],
               "resources": resources,
               "limits": ["Circle 2 role changes require a match/chaser system",
                          "Native loading, waiting and round-transition music is outside this gameplay slice",
                          "No original-client audible comparison has been performed"]}
    dest = ROOT / "config/music.json"
    dest.write_text(json.dumps(catalog, indent=2) + "\n")
    print(f"Imported {len(resources)} original Vorbis tracks; configured all {len(definitions)} maps")


if __name__ == "__main__":
    main()
