#!/usr/bin/env python3
from array import array
import hashlib
import json
import math
from pathlib import Path
import subprocess
from import_process import run as run_import_process
import sys

from asset_extract import INDEX, OUTPUT, RECREATION, decode_resource, stored_bytes, TOOLS
from asset_scn import parse
from asset_player import key_convert
from asset_animations import HZ, TICKS_PER_MS, sample, slerp
from asset_melee import lua_functions, symbolic_calls

DEST = OUTPUT / "practice"
RESEARCH = RECREATION / "research/practice-assets-decoded"


def extract(name):
    name = name.lower().replace("\\", "/")
    entry = INDEX[name]
    raw = stored_bytes(entry)
    decoded, _ = decode_resource(entry, raw)
    target = DEST / "original" / name
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(decoded)
    return target


def main():
    DEST.mkdir(parents=True, exist_ok=True)
    names = ["actorstates_damagestates", "actorstates_gameplay", "actorstates_normalstates"]
    if not (RESEARCH / "report.json").exists():
        run_import_process([sys.executable, str(TOOLS / "extract_inner_resources.py"), str(RESEARCH),
                        *["resources/script/" + name + ".lua" for name in names]], check=True,
                       stdout=subprocess.DEVNULL)
    builders = {}
    for name in names:
        for function, proto in lua_functions(RESEARCH / ("resources/script/" + name + ".lua")).items():
            if function in ["DamageState_WeaponUnused", "NormalState_WeaponUnused", "ActorState_Stun",
                            "ActorState_Blow", "ActorState_BoundBlow", "ActorState_Down", "ActorState_Standup", "ActorState_DownDamage", "ActorState_Death"]:
                builders[function] = {"source": "resources/script/" + name + ".lua",
                                      "first_line": proto["first_line"], "last_line": proto["last_line"],
                                      "calls": symbolic_calls(proto)}
    actions = {}
    mapping = {
        "NormalState_WeaponUnused": ["idle"],
        "DamageState_WeaponUnused": ["flinch_0", "flinch_1", "flinch_2", "flinch_3"],
        "ActorState_Stun": ["stun_start", "stun_loop"],
        "ActorState_Blow": ["blow_start", "blow_air", "blow_fall", "blow_ground", "get_up", "blow_end"],
        "ActorState_BoundBlow": ["bound_start", "bound_down", "bound_get_up"],
        "ActorState_Standup": ["stand_up"],
        "ActorState_Down": ["down"],
        "ActorState_DownDamage": ["down_hit"],
        "ActorState_Death": ["death", "death_hold", "death_ground", "death_bound", "death_bound_end"],
    }
    for function, labels in mapping.items():
        calls = [c for c in builders[function]["calls"] if c["method"] == "SetAnim"
                 and ".GetAnimParam(" in c["target"]]
        for label, call in zip(labels, calls):
            arguments = [int(v) if isinstance(v, float) and v.is_integer() else v for v in call["arguments"][1:]]
            actions[label] = {"clip": call["arguments"][0], "arguments": arguments,
                              "source_function": function, "source_line": call["source_line"]}
    source = extract("resources/model/character/male_bip.scn")
    scene = parse(source)
    bones = [c for c in scene["chunks"] if c["type"] == 0x6D411AD1]
    player = json.loads((OUTPUT / "player.json").read_text())
    assert [b["name"] for b in bones] == [b["name"] for b in player["bones"]]

    def key(bone, clip, seen=None):
        seen = set() if seen is None else seen
        assert clip not in seen
        animation = next((a for a in bone["animations"] if a["name"] == clip), None)
        if animation and animation.get("copy"):
            return key(bone, animation["copy"], seen | {clip})
        if animation and "translation" in animation.get("key", {}):
            return key_convert(animation["key"])
        return key(bone, "BASE", seen | {clip}) if clip != "BASE" else None

    manifest = {"format_version": 1, "source_ticks_per_ms": TICKS_PER_MS, "bone_count": len(bones),
                "floats_per_frame": len(bones) * 10, "sample_hz": HZ, "data_file": "practice/poses.bin",
                "source_skeleton_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                "clips": {}, "actions": actions, "builders": builders, "audio": {}}
    with (OUTPUT / manifest["data_file"]).open("wb") as output:
        for clip in sorted({action["clip"] for action in actions.values()}):
            tracks = [key(bone, clip) for bone in bones]
            assert all(tracks)
            duration = max(track["duration_ms"] for track in tracks)
            assert duration > 0
            count = max(2, math.ceil(duration / TICKS_PER_MS * HZ / 1000) + 1)
            manifest["clips"][clip] = {"duration_ticks": duration, "frame_count": count,
                                       "byte_offset": output.tell()}
            for frame in range(count):
                tick = frame / (count - 1) * duration
                values = array("f")
                for track in tracks:
                    values.extend(sample(track["translation"], track["translations"], tick))
                    values.extend(slerp(track["rotation"], sample(track["rotation"], track["rotations"], tick, True), 1.))
                    values.extend(sample(track["scale"], track["scales"], tick))
                assert len(values) == manifest["floats_per_frame"] and all(math.isfinite(v) for v in values)
                if sys.byteorder != "little":
                    values.byteswap()
                values.tofile(output)
        manifest["byte_length"] = output.tell()
    sounds = {c["arguments"][0].lower() for b in builders.values() for c in b["calls"]
              if c["method"] in ("AddSoundBySex", "AddSound")}
    sounds.update("resources/sound/" + name + ".ogg" for name in
                  ["_eu_voice_man_dead_normal", "_eu_voice_man_dead_blow", "_eu_voice_man_dead_shock", "stun"])
    for name in sorted(sounds):
        path = extract(name)
        raw = path.read_bytes()
        assert raw.startswith(b"OggS") and b"\x01vorbis" in raw[:128]
        manifest["audio"][Path(name).stem] = {"path": str(path.relative_to(OUTPUT)), "source": name,
                                               "sha256": hashlib.sha256(raw).hexdigest()}
    raw = (OUTPUT / manifest["data_file"]).read_bytes()
    manifest["data_sha256"] = hashlib.sha256(raw).hexdigest()
    (DEST / "practice.json").write_text(json.dumps(manifest, separators=(",", ":")) + "\n")
    report = {"status": "passed", "clips": len(manifest["clips"]), "actions": len(actions),
              "bones": len(bones), "pose_bytes": len(raw), "audio_files": len(manifest["audio"]),
              "data_sha256": manifest["data_sha256"], "source_skeleton_sha256": manifest["source_skeleton_sha256"]}
    (RECREATION / "research/practice-assets-validation.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
