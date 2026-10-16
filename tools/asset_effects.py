#!/usr/bin/env python3
import hashlib
import json
import math
import struct
import shutil
from pathlib import Path

from asset_extract import OUTPUT, extract
from asset_scn import Reader, parse, texture, convert

EFFECTS = {"dodge_left": "dodge_left.seq", "dodge_right": "dodge_right.seq",
           "jump": "jump_up.seq", "sprint": "skill_fastrun1.seq",
           "wall_jump": "basic_wall_jump.seq"}


def particle(r):
    version = r.read("f")
    out = {"version": version}
    for name, fmt in [("render_type", "I"), ("tex_anim_type", "I"),
                      ("blend_type", "I"), ("lifetime_var", "I"),
                      ("speed_var", "f"), ("size_x_var", "f"),
                      ("size_y_var", "f"), ("color_r_var", "f"),
                      ("color_g_var", "f"), ("color_b_var", "f"),
                      ("alpha_var", "f"), ("rotate_var", "f"),
                      ("attach_target", "B"), ("max_particles", "I"),
                      ("scale_emitter", "B"), ("tex_anim_delay_ms", "I"),
                      ("unknown_64", "B"), ("cull_radius", "f")]:
        out[name] = r.read(fmt)
    if version >= struct.unpack("<f", struct.pack("<f", .2))[0]:
        out["size_var"] = r.read("f")
        out["locks"] = r.read("4B")
    if version >= struct.unpack("<f", struct.pack("<f", .3))[0]:
        out["minimum_time_delta_ms"] = r.read("I")
    if version >= struct.unpack("<f", struct.pack("<f", .4))[0]:
        out["glow"] = r.read("B")
    if version >= .5:
        out["haze"] = r.read("B")
    assert abs(r.read("f") - .1) < 1e-6
    out["emitter_duration_ms"] = r.read("I")
    out["emitter_key_lerp"] = r.read("B")
    out["emitter_keys"] = []
    for _ in range(r.read("I")):
        out["emitter_keys"].append({
            "time_ms": r.read("f"), "continuance": r.read("B"),
            "position": r.read("3f"), "cone_direction": r.read("3f"),
            "cone_theta_degrees": r.read("f"), "speed": r.read("f"),
            "emitter_type": r.read("I"), "min_width": r.read("f"),
            "max_width": r.read("f"), "min_height": r.read("f"),
            "max_height": r.read("f"), "gravity": r.read("3f"),
            "attractive": r.read("f"), "lifetime_ms": r.read("I"),
            "emit_per_second": r.read("I"), "texture_key": r.read("I")})
    assert abs(r.read("f") - .1) < 1e-6
    out["particle_keys"] = [r.read("8f") for _ in range(r.read("I"))]
    assert abs(r.read("f") - .2) < 1e-6
    out["textures"] = []
    for _ in range(r.read("I")):
        source = r.string()
        out["textures"].append({"source": source,
                                "path": texture(source, "resources/effects/"),
                                "uv": r.read("8f")})
    assert out["textures"] and out["particle_keys"] and out["emitter_keys"]
    return out


def sequence(path):
    r = Reader(path.read_bytes())
    assert abs(r.read("f") - .1) < 1e-6
    out = {"source": str(path.relative_to(OUTPUT)),
           "sha256": hashlib.sha256(r.data).hexdigest(),
           "flags": r.read("2B"), "duration_ms": r.read("I"), "components": []}
    for _ in range(r.read("I")):
        item = {"class": r.string(), "offset": r.pos}
        assert abs(r.read("f") - .1) < 1e-6
        item.update({"flags": r.read("2I"), "name": r.string(),
                     "start_ms": r.read("I"), "end_ms": r.read("I")})
        if item["class"] == "CActParticle":
            assert abs(r.read("f") - .1) < 1e-6
            item["particle"] = particle(r)
        elif item["class"] == "CActNodeLoader":
            item["version"] = r.read("f")
            item["model"] = r.string()
            item["node_payload_hex"] = r.data[r.pos:].hex()
            assert item["node_payload_hex"] == "0000f4010000f40100000000000000c8000000cdcccc3d00000000"
            r.pos = len(r.data)
        else:
            raise ValueError(item["class"])
        item["end_offset"] = r.pos
        out["components"].append(item)
    assert r.pos == len(r.data), (path, r.pos, len(r.data))
    return out


def sample(track, base, ticks):
    if not track:
        return base
    if ticks <= track[0][0]:
        return track[0][1:]
    for a, b in zip(track, track[1:]):
        if ticks <= b[0]:
            t = (ticks - a[0]) / max(1, b[0] - a[0])
            return [x + (y - x) * t for x, y in zip(a[1:], b[1:])]
    return track[-1][1:]


def wall_mesh():
    from asset_scn import trs, point
    data = parse(extract("resources/effects/basic_walljump.scn"))
    meshes = []
    for c in data["chunks"]:
        if c["type"] != 0x081098F8:
            continue
        key = c["animations"][0]["key"]
        frames = []
        for index in range(75):
            ticks = min(2960, index * 40)
            transform = {name: sample(key.get(name + "s", []), key[name], ticks)
                         for name in ["translation", "rotation", "scale"]}
            alpha = sample(key.get("alpha", []), [1.], ticks)[0]
            frames.append({"tick": ticks, "alpha": alpha,
                           "positions": [convert(point(trs(transform), p)) for p in c["positions"]]})
        for t in c["textures"]:
            faces = c["faces"][t["face_offset"]:t["face_offset"] + t["face_count"]]
            color_texture = texture(t["main"], "resources/effects/")
            gradient = None
            if c["shader_flags"] & 0x8000:
                gradient = str(Path(color_texture).with_stem(Path(color_texture).stem + "-haze"))
                shutil.copyfile(OUTPUT / color_texture, OUTPUT / gradient)
            meshes.append({"name": c["name"], "shader_flags": c["shader_flags"],
                           "texture": color_texture, "haze_texture": gradient,
                           "indices": [i for f in faces for i in [f[0], f[2], f[1]]],
                           "uvs": [[u, 1-v] for u, v in c["uvs"]], "frames": frames})
    return {"duration_ticks": 2960, "ticks_per_ms": 4.8, "meshes": meshes}


def main():
    output = {"format_version": 1,
              "sequences": {key: sequence(extract("resources/effects/" + name))
                            for key, name in EFFECTS.items()}, "wall_jump": wall_mesh()}
    dest = OUTPUT / "movement-effects.json"
    dest.write_text(json.dumps(output, separators=(",", ":")))
    print(dest, dest.stat().st_size, "bytes")
    for name, seq in output["sequences"].items():
        for c in seq["components"]:
            p = c.get("particle")
            if p:
                print(name, c["start_ms"], c["end_ms"], "render", p["render_type"], "blend", p["blend_type"],
                      "attach", p["attach_target"], "keys", p["emitter_keys"], "particle", p["particle_keys"])


if __name__ == "__main__":
    main()
