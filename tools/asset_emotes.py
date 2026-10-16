#!/usr/bin/env python3
from array import array
import hashlib
import json
import math
import subprocess
from import_process import run as run_import_process
import sys
import html
import re
import struct
import xml.etree.ElementTree as ET

from asset_extract import extract, OUTPUT, RECREATION, TOOLS
from asset_scn import parse
from asset_player import key_convert
from asset_animations import HZ, TICKS_PER_MS, sample, slerp


def main():
    evidence = RECREATION / "research/emotes-decoded"
    if not (evidence / "report.json").exists():
        run_import_process([sys.executable, str(TOOLS / "extract_inner_resources.py"),
                        str(evidence), "language/default_option.x7",
                        "language/xml/default_option_string_table.x7",
                        "language/develop_social.x7", "resources/script/actorstates_gameplay.lua"],
                       check=True, stdout=subprocess.DEVNULL)
    labels = {e.get("key"): e.get("eng") for e in
              ET.parse(evidence / "language/xml/default_option_string_table.x7").getroot()}
    options = ET.parse(evidence / "language/default_option.x7").getroot()
    ui_evidence = RECREATION / "research/emotes-ui-source"
    if not (ui_evidence / "report.json").exists():
        run_import_process([sys.executable, str(TOOLS / "extract_inner_resources.py"),
                        str(ui_evidence), "language/script/quick_chatting_string_table.x7",
                        "language/font_config.x7"], check=True, stdout=subprocess.DEVNULL)
    quick = ET.parse(ui_evidence / "language/script/quick_chatting_string_table.x7").getroot()
    commands = [quick.find(f"string[@key='DEFENSE_MESAGE{i}']/eng").get("value") for i in range(1, 10)]
    socials = {labels[e.get("key_key")].lower(): e for e in options.find("socialaction")}
    ordered = [socials[command.lower()] for command in commands] + [socials["/hug"]]
    markup = quick.find("string[@key='DEFENSE_MESSAGE_LIST']/eng").get("value")
    menu_text = re.sub(r"\{[^}]+\}", "", markup.replace("{N}", "\n"))
    menu_labels = [line.split(".", 1)[1] for line in menu_text.splitlines() if re.match(r"[1-9]\.", line)]
    color = [int(n) for n in re.search(r"\{CB-([\d,]+)\}", markup)[1].split(",")]
    font_key = re.search(r"\{F-([^}]+)\}", markup)[1]
    font = ET.parse(ui_evidence / "language/font_config.x7").find(f"font[@name='{font_key}']/eng")
    gui = extract("resources/gui/new/maingame_ingame.gui").read_bytes()
    start = gui.index(b"GroupBox_quickchat\0") + len(b"GroupBox_quickchat\0")
    x, y, right, bottom = struct.unpack_from("<4i", gui, start + 16)
    skeleton = extract("resources/model/character/male_bip.scn")
    bones = [b for b in parse(skeleton)["chunks"] if b["type"] == 0x6D411AD1]
    player = json.loads((OUTPUT / "player.json").read_text())
    assert [b["name"] for b in bones] == [b["name"] for b in player["bones"]]

    def track(bone, clip, seen=None):
        seen = set() if seen is None else seen
        assert clip not in seen, (bone["name"], clip)
        a = next((a for a in bone["animations"] if a["name"] == clip), None)
        if a and a.get("copy"):
            return track(bone, a["copy"], seen | {clip})
        if a and "translation" in a.get("key", {}):
            return key_convert(a["key"])
        return track(bone, "BASE", seen | {clip}) if clip != "BASE" else None

    dest = OUTPUT / "emotes"
    dest.mkdir(parents=True, exist_ok=True)
    manifest = {"format_version": 3, "source_ticks_per_ms": TICKS_PER_MS,
                "floats_per_frame": len(bones) * 10, "bone_count": len(bones),
                "data_file": "emotes/poses.bin", "clips": {}, "actions": {}, "entries": [],
                "menu": {"text": menu_text, "rect": [x, y, right-x, bottom-y],
                         "font_size": int(font.get("size")), "color": color,
                         "line_height": 20, "source_markup": markup,
                         "gui_sha256": hashlib.sha256(gui).hexdigest()}}
    with (OUTPUT / manifest["data_file"]).open("wb") as out:
        for index, e in enumerate(ordered):
            clip = e.get("animation")
            if not clip:
                continue
            assert any(a["name"] == clip for b in bones for a in b["animations"]), clip
            tracks = [track(b, clip) for b in bones]
            assert all(tracks), clip
            duration = max(t["duration_ms"] for t in tracks)
            count = max(2, math.ceil(duration / TICKS_PER_MS * HZ / 1000) + 1)
            assert duration > 0
            manifest["clips"][clip] = {"duration_ticks": duration, "frame_count": count,
                                        "byte_offset": out.tell()}
            for frame in range(count):
                tick = frame / (count - 1) * duration
                values = array("f")
                for t in tracks:
                    values.extend(sample(t["translation"], t["translations"], tick))
                    values.extend(slerp(t["rotation"], sample(t["rotation"], t["rotations"], tick, True), 1.))
                    values.extend(sample(t["scale"], t["scales"], tick))
                assert len(values) == len(bones) * 10 and all(math.isfinite(v) for v in values)
                if sys.byteorder != "little":
                    values.byteswap()
                values.tofile(out)
            action = "emote_" + e.get("id")
            manifest["actions"][action] = {"clip": clip, "arguments": [0, 500, 0, False, 1, True]}
            sounds = {}
            for sex, field in [("male", "malesound"), ("female", "femalesound")]:
                name = e.get(field).lower().replace("_rc_", "_eu_")
                if name:
                    path = extract(name)
                    raw = path.read_bytes()
                    assert raw.startswith(b"OggS") and b"\x01vorbis" in raw[:128]
                    sounds[sex] = {"path": str(path.relative_to(OUTPUT)),
                                   "sha256": hashlib.sha256(raw).hexdigest()}
            manifest["entries"].append({"source_id": int(e.get("id")),
                "label": menu_labels[index] if index < 9 else html.unescape(labels[e.get("text_key")]),
                "command": labels[e.get("key_key")],
                "action": action, "duration": duration / TICKS_PER_MS / 1000,
                "use_weapon": e.get("useweapon") == "true", "require_level": int(e.get("require_level")),
                "sounds": sounds})
    manifest["data_sha256"] = hashlib.sha256((OUTPUT / manifest["data_file"]).read_bytes()).hexdigest()
    manifest["source_skeleton_sha256"] = hashlib.sha256(skeleton.read_bytes()).hexdigest()
    (dest / "emotes.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Imported {len(manifest['entries'])} original social animations and their EU voice files.")


if __name__ == "__main__":
    main()
