#!/usr/bin/env python3
from array import array
from dataclasses import dataclass
from functools import lru_cache
import hashlib
import json
import math
from pathlib import Path
import re
import shutil
import struct
import subprocess
from import_process import run as run_import_process
import sys
import tempfile
import xml.etree.ElementTree as ET

from PIL import Image
from resource_reader import CLIENT_SHA256
from asset_extract import INDEX, RECREATION, OUTPUT, decode_resource, stored_bytes, TOOLS
from asset_scn import parse, mul, point, vector, convert, matrix_convert, static_matrix, trs, Reader
from asset_player import key_convert
from asset_animations import sample, slerp, HZ, TICKS_PER_MS

DEST = OUTPUT / "melee"
RESEARCH = RECREATION / "research/melee-assets-decoded"
IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
TEXT = ["xml/weapon.x7", "xml/weapon_lua.x7", "xml/item.x7"] + [
    "resources/script/" + name + ".lua" for name in [
        "actorstates_normalstates", "actorstates_runstates", "actorstates_plasmasword",
        "actorstates_spydagger", "actorstates_countersword", "actorstates_damagestates", "actorstates_gameplay", "weapon_plasmasword",
        "weapon_spydagger", "weapon_countersword", "attackattribindexer"]]
WEAPONS = {"plasma": "2000001", "dagger": "2000006", "counter": "2000002"}
RESOURCE_REPORT = {}
RESOURCE_ALIASES = {}


@lru_cache(maxsize=None)
def extract(name):
    name = name.lower().replace("\\", "/")
    entry = INDEX[name]
    stored = stored_bytes(entry)
    data, compressed = decode_resource(entry, stored)
    RESOURCE_REPORT[name] = {**entry, "stored_sha256": hashlib.sha256(stored).hexdigest(),
                             "decoded_sha256": hashlib.sha256(data).hexdigest(),
                             "outer_crc_pass": True, "outer_lzo_compressed": compressed,
                             "decoded_size": len(data), "inner_crc_pass": None}
    path = DEST / "original" / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return path


def texture(name, resource_directory="resources/model/weapon/"):
    if not name:
        return None
    filename = str(Path(name.lower().replace("\\", "/").rsplit("/", 1)[-1]).with_suffix(".dds"))
    if resource_directory == "resources/uieffect/" and filename == "_rc_critical_dam.dds":
        resolved = "_eu_critical_dam.dds"
        assert resource_directory + resolved in INDEX
        RESOURCE_ALIASES[resource_directory + filename] = {"source": resource_directory + resolved,
            "status": "native locale region rewrite c0e320/c0e0a0, EU catalog1051b50"}
        filename = resolved
    candidates = INDEX.texture_candidates(filename, resource_directory)
    if not candidates:
        raise ValueError("Missing original texture " + filename)
    source = next((n for n in candidates if n.startswith(resource_directory)), candidates[0])
    dest = DEST / "textures" / (resource_directory.replace("/", "_") + Path(filename).with_suffix(".png").name)
    dest.parent.mkdir(parents=True, exist_ok=True)
    Image.open(extract(source)).convert("RGBA").save(dest)
    return str(dest.relative_to(OUTPUT))


class LuaReader:
    def __init__(self, raw):
        assert raw[:12] == bytes.fromhex("1b4c75615100010404040800")
        self.raw, self.pos = raw, 12

    def read(self, fmt):
        out = struct.unpack_from("<" + fmt, self.raw, self.pos)
        self.pos += struct.calcsize("<" + fmt)
        return out[0] if len(out) == 1 else out

    def string(self):
        n = self.read("I")
        raw = self.raw[self.pos:self.pos + n]
        self.pos += n
        return raw[:-1].decode("utf-8", "replace") if n else None

    def proto(self):
        p = {"source": self.string(), "first_line": self.read("I"), "last_line": self.read("I")}
        p["nups"], p["params"], _, p["stack"] = self.read("4B")
        p["code"] = [self.read("I") for _ in range(self.read("I"))]
        p["constants"] = []
        for _ in range(self.read("I")):
            kind = self.read("B")
            value = {0: lambda: None, 1: lambda: bool(self.read("B")), 3: lambda: self.read("d"), 4: self.string}[kind]()
            p["constants"].append(value)
        p["protos"] = [self.proto() for _ in range(self.read("I"))]
        p["lines"] = [self.read("I") for _ in range(self.read("I"))]
        p["locals"] = [(self.string(), self.read("I"), self.read("I")) for _ in range(self.read("I"))]
        p["upvalues"] = [self.string() for _ in range(self.read("I"))]
        return p


def instruction(i):
    return i & 63, i >> 6 & 255, i >> 23 & 511, i >> 14 & 511, i >> 14


def lua_functions(path):
    raw = path.read_bytes()
    r = LuaReader(raw)
    p = r.proto()
    assert r.pos == len(raw), (path, r.pos, len(raw))
    result, registers = {}, {}
    for code in p["code"]:
        op, a, b, c, bx = instruction(code)
        if op == 36:
            registers[a] = p["protos"][bx]
        elif op == 7 and a in registers:
            result[p["constants"][bx]] = registers[a]
            del registers[a]
    return result


@dataclass
class Ref:
    path: str


@dataclass
class Method:
    target: Ref
    name: str


def symbolic_calls(p):
    regs = {i: Ref("argument_" + str(i)) for i in range(p["params"])}
    if p["params"] == 1: regs[0] = Ref("state")
    calls = []
    constants = p["constants"]
    globals_ = {}
    open_call_end = None
    def rk(n): return constants[n & 255] if n & 256 else regs[n]
    def serial(v): return v.path if isinstance(v, Ref) else v
    for pc, code in enumerate(p["code"]):
        op, a, b, c, bx = instruction(code)
        if op == 0: regs[a] = regs[b]
        elif op == 1: regs[a] = constants[bx]
        elif op == 2: regs[a] = bool(b)
        elif op == 5:
            n = constants[bx]
            refs = ("AttackAttribIndexer", "STATE", "WEAPON", "ATTACKS", "CreateCAttack", "MAKEVECTOR2", "MAKEVECTOR3", "MAKERAY", "MAKEBOX", "MAKEFAN", "MAKECYLINDER", "AddAttack")
            regs[a] = globals_.get(n, {"LOOP_TRUE": True, "LOOP_FALSE": False, "RESET_TRUE": True, "RESET_FALSE": False}.get(n, Ref(n) if n in refs else n))
        elif op == 7: globals_[constants[bx]] = regs[a]
        elif op == 11:
            regs[a], regs[a + 1] = Method(regs[b], rk(c)), regs[b]
        elif op in (12, 13, 14, 15):
            left, right = rk(b), rk(c)
            if isinstance(left, (int, float)) and isinstance(right, (int, float)):
                regs[a] = {12: lambda: left + right, 13: lambda: left - right, 14: lambda: left * right, 15: lambda: left / right}[op]()
            else: regs[a] = Ref("(" + str(serial(left)) + ["+", "-", "*", "/"][op - 12] + str(serial(right)) + ")")
        elif op == 28:
            f = regs[a]
            assert isinstance(f, (Method, Ref)), (p["source"], pc, f)
            end = a + b if b else open_call_end
            assert end is not None, (p["source"], pc, "open CALL without known result")
            args = [serial(regs[n]) for n in range(a + (2 if isinstance(f, Method) else 1), end)]
            call = {"method": f.name if isinstance(f, Method) else "__call", "target": f.target.path if isinstance(f, Method) else f.path, "arguments": args, "source_line": p["lines"][pc]}
            calls.append(call)
            if c == 0:
                assert isinstance(f, Ref) and f.path in ("MAKEVECTOR2", "MAKEVECTOR3", "MAKERAY", "MAKEBOX", "MAKEFAN", "MAKECYLINDER"), (p["source"], pc, "unsupported open-return function", f)
            if c != 1:
                regs[a] = Ref(call["target"] + ("." + f.name if isinstance(f, Method) else "") + "(" + ",".join(str(v) for v in args) + ")")
            open_call_end = a + 1 if c == 0 else None
        elif op == 30: break
        else: raise ValueError((p["source"], p["first_line"], pc, "Unsupported registration opcode", op))
    return calls


def collect_builders():
    out = {}
    for name in ["actorstates_normalstates", "actorstates_runstates", "actorstates_plasmasword", "actorstates_spydagger", "actorstates_countersword", "actorstates_damagestates"]:
        funcs = lua_functions(RESEARCH / ("resources/script/" + name + ".lua"))
        for fn, p in funcs.items():
            if name == "actorstates_damagestates" and fn != "DamageState_CounterSword": continue
            if fn.endswith(("_PlasmaSword", "_SpyDagger", "_CounterSword", "_CounterSwordDash1", "_CounterSwordGuard")):
                p["source"] = p["source"] or ("resources/script/" + name + ".lua")
                out[fn] = {"source": p["source"], "first_line": p["first_line"], "last_line": p["last_line"], "calls": symbolic_calls(p)}
    return out


def geometry(source):
    scene = parse(extract(source))
    lookup = {c["name"]: c for c in scene["chunks"]}
    def world(c):
        if isinstance(c, str): c = lookup[c]
        m = static_matrix(c)
        return mul(world(lookup[c["parent"]]), m) if c["parent"] in lookup else mul(scene["matrix"], m)
    meshes, nodes = [], []
    for c in scene["chunks"]:
        nodes.append({"name": c["name"], "parent": c["parent"], "type": hex(c["type"]), "matrix": matrix_convert(static_matrix(c)), "animations": c.get("animations", []), "shapes": c.get("shapes", [])})
        if c["type"] != 0x081098F8: continue
        m = world(c)
        positions, normals = mesh_pose(c, world)
        for j, t in enumerate(c["textures"]):
            meshes.append({"name": source + "/" + c["name"] + "/" + str(j),
                           "positions": positions, "normals": normals,
                           "source_skin": c["weights"],
                           "uvs": c["uvs"], "uvs2": c["uvs2"],
                           "indices": [i for f in c["faces"][t["face_offset"]:t["face_offset"] + t["face_count"]] for i in (f[0], f[2], f[1])],
                           "texture": texture(t["main"], source.rsplit("/", 1)[0] + "/"), "side_texture": texture(t["side"]),
                           "shader_flags": c["shader_flags"], "visible": True, "collision": False, "sky": False})
    return {"meshes": meshes, "nodes": nodes, "matrix": matrix_convert(scene["matrix"])}


def mesh_pose(node, world):
    if not node["weights"]:
        matrix = world(node)
        return ([convert(point(matrix, p)) for p in node["positions"]],
                [convert(vector(matrix, n), 1) for n in node["normals"]])
    matrices = [(mul(world(weight["name"]), weight["matrix"]), weight["weights"])
                for weight in node["weights"]]
    sums = [0.] * len(node["positions"])
    positions = [[0., 0., 0.] for _ in node["positions"]]
    normals = [[0., 0., 0.] for _ in node["normals"]]
    for matrix, weights in matrices:
        for index, weight in weights:
            assert 0 <= index < len(positions) and math.isfinite(weight) and weight >= 0
            p = point(matrix, node["positions"][index])
            n = vector(matrix, node["normals"][index])
            sums[index] += weight
            for axis in range(3):
                positions[index][axis] += p[axis] * weight
                normals[index][axis] += n[axis] * weight
    assert all(total > 0 for total in sums), (node["name"], "unweighted weapon vertex")
    positions = [convert([v / total for v in p]) for p, total in zip(positions, sums)]
    normals = [convert([v / math.sqrt(sum(q*q for q in n)) for v in n], 1) for n in normals]
    return positions, normals


def mesh_animation(source, clip=None, ui=False):
    scene = parse(extract(source))
    nodes = {c["name"]: c for c in scene["chunks"]}
    def track(c):
        choices = c.get("animations", [])
        a = next((a for a in choices if a["name"] == clip), None)
        if not a:
            a = next((a for a in choices if a["name"].upper() == "BASE"), choices[0] if choices else {})
        return a.get("key", {})
    duration = max((track(c).get("duration_ms", 0) for c in scene["chunks"]), default=0)
    count = max(2, math.ceil(duration / TICKS_PER_MS * HZ / 1000) + 1)
    meshes = []
    for c in scene["chunks"]:
        if c["type"] != 0x081098F8: continue
        frames = []
        for index in range(count):
            tick = index / (count - 1) * duration
            def world(node):
                if isinstance(node, str): node = nodes[node]
                k = track(node)
                m = trs({"translation": sample(k["translation"], k.get("translations", []), tick),
                         "rotation": sample(k["rotation"], k.get("rotations", []), tick, True),
                         "scale": sample(k["scale"], k.get("scales", []), tick)}) if "translation" in k else node["matrix"]
                return mul(world(nodes[node["parent"]]), m) if node["parent"] in nodes else mul(IDENTITY if ui else scene["matrix"], m)
            positions, normals = mesh_pose(c, world)
            if ui:
                positions = [[x * 100, y * 100, -z * 100] for x, y, z in positions]
            alpha = sample([1.], track(c).get("alpha", []), tick)[0]
            frame = {"tick": round(tick, 6), "alpha": alpha, "positions": positions}
            if c["weights"]: frame["normals"] = normals
            frames.append(frame)
        for j, t in enumerate(c["textures"]):
            meshes.append({"name": source + "/" + c["name"] + "/" + str(j), "shader_flags": c["shader_flags"],
                           "texture": texture(t["main"], source.rsplit("/", 1)[0] + "/"),
                           "indices": [i for f in c["faces"][t["face_offset"]:t["face_offset"] + t["face_count"]] for i in (f[0], f[2], f[1])],
                           "uvs": c["uvs"] if ui or source == "resources/effects/spy_dagger_effect.scn" or "counter" in source
                                  else [[u, 1-v] for u, v in c["uvs"]], "frames": frames})
    trail_frames = []
    if "Line01" in nodes and meshes:
        shape = nodes["Line01"]
        pair = shape["shapes"][0]
        endpoints = [pair[:3], [pair[i] + pair[i + 3] for i in range(3)]]
        for index in range(count):
            tick = index / (count - 1) * duration
            m = world(shape)
            trail_frames.append({"tick": round(tick, 6), "points": [convert(point(m, p)) for p in endpoints]})
    node_frames = {}
    for node in scene["chunks"]:
        frames = []
        for index in range(count):
            tick = index / (count - 1) * duration
            frames.append({"tick": round(tick, 6), "matrix": world(node) if ui else matrix_convert(world(node))})
        node_frames[node["name"]] = frames
    return {"source": source, "source_clip": clip, "duration_ticks": duration, "ticks_per_ms": TICKS_PER_MS, "meshes": meshes, "trail_frames": trail_frames, "node_frames": node_frames,
            "units": "source_screen_units" if ui else "meters",
            "coordinate_mapping": "source XY screen plane, Z depth; child hierarchy baked, scene root omitted because native feedback overrides it" if ui else "[x,y,-z]*0.01",
            **({"original_scene_matrix": scene["matrix"], "scene_root_status": "native0x50baa0 virtual34 overrides scene root with screen offset/depth"} if ui else {})}


def sequence(source):
    import asset_effects
    asset_effects.texture = texture
    raw = extract(source).read_bytes()
    r = Reader(raw)
    assert abs(r.read("f") - .1) < 1e-6
    matches = list(re.finditer(rb'CAct\w+\x00', raw))
    assert matches
    first = matches[0].start()
    header = raw[4:first - 10]
    r.pos = first - 10
    result = {"source": source, "sha256": hashlib.sha256(raw).hexdigest(),
              "title_hex": header.hex(), "flags": r.read("2B"), "duration_ms": r.read("I"), "components": []}
    count = r.read("I")
    assert len(matches) == count and r.pos == first
    models = []
    for ordinal, match in enumerate(matches):
        end = matches[ordinal + 1].start() if ordinal + 1 < count else len(raw)
        part = Reader(raw[match.start():end])
        item = {"class": part.string(), "offset": match.start()}
        assert abs(part.read("f") - .1) < 1e-6
        item.update({"flags": part.read("2I"), "name": part.string(), "start_ms": part.read("I"), "end_ms": part.read("I")})
        if item["class"] == "CActParticle":
            assert abs(part.read("f") - .1) < 1e-6
            if Path(source).name == "blank_1sec.seq":
                item["particle"] = None
                item["empty_particle_payload_hex"] = part.data[part.pos:].hex()
                item["payload_status"] = "original blank attack sequence; no visible particle"
                part.pos = len(part.data)
            else:
                item["particle"] = asset_effects.particle(part)
        elif item["class"] in ("CActNodeLoader", "CActNodeLoader_IF"):
            item["version"] = part.read("f")
            item["model"] = part.string()
            item["node_payload_hex"] = part.data[part.pos:].hex()
            payload = Reader(part.data[part.pos:])
            item["animation_name"] = payload.string()
            item["animation_loop"] = bool(payload.read("B"))
            item["fade_in_ms"] = payload.read("I")
            item["fade_out_ms"] = payload.read("I")
            item["payload_status"] = "source animation/fade fields decoded; optional renderer flags retained"
            directory = "resources/uieffect/" if item["class"] == "CActNodeLoader_IF" else "resources/effects/"
            model = directory + item["model"].lower().replace("\\", "/").rsplit("/", 1)[-1]
            if item["class"] == "CActNodeLoader_IF":
                assert source == "resources/uieffect/inter_critical_damage.seq" and abs(item["version"] - .3) < 1e-6, (source, item["version"])
                item["node_optional_payload_hex"] = payload.data[payload.pos:-16].hex()
                payload.pos = len(payload.data) - 16
                assert abs(payload.read("f") - .1) < 1e-6
                item["screen_position"] = payload.read("3f")
                item["screen_position_fields"] = ["ScreenPosX", "ScreenPosY", "ScreenDepth"]
                item["payload_status"] = "source loader/fade and UI screen fields decoded; optional loader payload retained"
            models.append(model)
            item["asset_model"] = model
            part.pos = len(part.data)
        elif item["class"] == "CActNodeControl":
            item["node_payload_hex"] = part.data[part.pos:].hex()
            item["payload_status"] = "original control payload retained; target/control semantics unresolved"
            part.pos = len(part.data)
        else:
            raise ValueError((source, item["class"]))
        assert part.pos == len(part.data), (source, ordinal, part.pos, len(part.data))
        item["end_offset"] = end
        result["components"].append(item)
    for control in result["components"]:
        if control["class"] != "CActNodeControl" or control["flags"][0] != 1:
            continue
        raw_control = bytes.fromhex(control["node_payload_hex"])
        if (len(raw_control) != 88 or raw_control[44:56] != bytes(12)
                or raw_control[60:64] != bytes(4) or raw_control[75] != 0
                or control["flags"][1] >= len(result["components"])):
            continue
        payload = Reader(raw_control)
        payload.pos = 64
        enables = payload.read("3B")
        destination_type, destination_id = payload.read("2I")
        destination_name = payload.string()
        sources = payload.read("3I")
        target = result["components"][control["flags"][1]]
        if enables == [1, 0, 0] and (destination_type, destination_id, destination_name) == (0, 0, ""):
            target["actor_transform_sources"] = sources
        elif enables == [1, 0, 0] and (destination_type, destination_id, destination_name) == (1, control["flags"][1], ""):
            target["self_transform_sources"] = sources
    return result, models


def effects_audio_ui(builders, weapons, gameplay_builders):
    attribute_p = lua_functions(RESEARCH / "resources/script/attackattribindexer.lua")["AttackAttribIndexer_Run"]
    attrs = [c for c in symbolic_calls(attribute_p) if any(w in c["target"] for w in ("PLASMA_SWORD", "SPYDAGGER", "COUNTER_SWORD"))]
    calls = attrs + [c for b in [*builders.values(), *gameplay_builders.values()] for c in b["calls"]]
    sources = sorted({s.lower() for c in calls for s in c["arguments"] if isinstance(s, str) and s.lower().endswith((".seq", ".ogg"))})
    sources += ["resources/effects/sword_jumpatt_d100.seq", "resources/sound/sword_power.ogg"]
    native_dependencies = []
    for source in ("resources/effects/guard_crush.seq", "resources/sound/guard_crush.ogg"):
        native_dependencies.append({"source": source, "native_consumer": "0x455de0, guard-crush incoming CAttackInfo+24 hit position",
            "status": "selected native dependency present" if source in INDEX else "selected native dependency missing from checked EU1162 resource index; no replacement"})
        if source in INDEX: sources.append(source)
    sources.append("resources/sound/interface/critical.ogg")
    for weapon in weapons.values():
        for value in weapon.get("other_effects", {}).values():
            if value.lower().endswith(".seq"): sources.append("resources/effects/" + value.lower())
            elif value.lower().endswith(".ogg"): sources.append("resources/sound/" + value.lower())
        weapon["missing_dependencies"] = []
        for field, value in weapon.get("resources", {}).items():
            if not value.lower().endswith(".ogg"): continue
            source = "resources/sound/" + value.lower()
            if source in INDEX: sources.append(source)
            else:
                weapon["missing_dependencies"].append({"source": source, "registry_field": field,
                    "status": "missing from checked resource index; unselected reload registry reference retained, no replacement"})
    seqs, models, audio = {}, set(), {}
    for source in sorted(set(sources)):
        if source.endswith(".seq"):
            seq, refs = sequence(source)
            seqs[Path(source).stem] = seq
            models.update(refs)
        elif source.endswith(".ogg"):
            path = extract(source)
            data = path.read_bytes()
            assert data.startswith(b"OggS") and b"\x01vorbis" in data[:128]
            audio[Path(source).stem] = {"source": source, "path": str(path.relative_to(OUTPUT)), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    effects = {"sequences": seqs, "models": {Path(source).stem: mesh_animation(source) for source in sorted(models)}, "attribute_calls": attrs}
    effects["native_dependencies"] = native_dependencies
    effects["guard_block_status"] = "native0x5af450 consumes attribute guard-sequence list; selected AttackAttribIndexer has zero SetGuardSequence registrations, list empty"
    critical_sequence, critical_models = sequence("resources/uieffect/inter_critical_damage.seq")
    ui_feedback = {"critical": {"sequence": critical_sequence,
        "models": {Path(source).stem: mesh_animation(source, ui=True) for source in critical_models},
        "audio_key": "critical", "native_consumer": "0x50baa0, CAttackInfo+68 feedback, global250ms debounce",
        "debounce_ms": 250, "native_root_translation": {"x": "(viewport_width-1024)/2", "y": 0, "z": 1000},
        "placement_status": "source UI geometry and ScreenPosX/ScreenPosY/ScreenDepth retained; native UI projection consumed separately from world effects"}}
    ui = {}
    items = ET.parse(RESEARCH / "xml/item.x7").getroot()
    for slug, weapon in weapons.items():
        item = next(n for n in items if n.get("item_key") == str(weapon["item_key"]))
        filename = item.find("graphic").get("icon_image").lower()
        source = "resources/image/weapon/" + filename
        if source not in INDEX:
            source = str(Path(source).with_suffix(".dds"))
        image = Image.open(extract(source)).convert("RGBA")
        dest = DEST / "ui" / (slug + "-slot.png")
        dest.parent.mkdir(parents=True, exist_ok=True)
        image.save(dest)
        ui[slug] = {"path": str(dest.relative_to(OUTPUT)), "source": source, "registry": "xml/item.x7", "size": list(image.size), "canvas_rect": None,
                    "placement_status": "dynamic slot image placement not established by static gameplay GUI"}
    return effects, audio, ui, ui_feedback


def weapon_builders():
    out = {}
    for name in ["weapon_plasmasword", "weapon_spydagger", "weapon_countersword"]:
        for fn, p in lua_functions(RESEARCH / ("resources/script/" + name + ".lua")).items():
            out[fn] = {"source": "resources/script/" + name + ".lua", "first_line": p["first_line"], "last_line": p["last_line"], "calls": symbolic_calls(p)}
    out["StandardCounterSwordCounterAttack"]["calls"].append({"target": "STATE", "method": "AddSequence",
        "arguments": [0, "Resources/Effects/weapon_counter_upper02.seq", "Resources/Sound/bsword_counter.ogg", 250, 5000],
        "source_line": 0, "evidence_kind": "static native trace", "native_callback": "0x82bc23->0x82bfd0->0x82c010; 0x518dc0/0x518ea0",
        "selection_status": "manual retaliation state44 selected equipment other_effects; independent of incoming guard-success feedback"})
    return out


def inverse_rigid(matrix):
    for i in range(3):
        for j in range(3):
            assert abs(sum(matrix[i * 4 + k] * matrix[j * 4 + k] for k in range(3)) - (i == j)) < 1e-5
    result = IDENTITY.copy()
    for i in range(3):
        for j in range(3):
            result[i * 4 + j] = matrix[j * 4 + i]
    result[12:15] = [-sum(result[k * 4 + i] * matrix[12 + k] for k in range(3)) for i in range(3)]
    return result


def native_metadata(weapons):
    sword = parse(extract(weapons["plasma"]["source"]))
    dummy = next(c for c in sword["chunks"] if "Wea_Dummy" in c["name"])
    equipped = inverse_rigid(matrix_convert(mul(sword["matrix"], static_matrix(dummy))))
    weapons["plasma"].update({"attachment_status": "native0x7176a0/0x7160c0: dummy child attaches directly to hand; dummy retained for holster", "attach_matrix": equipped, "holster_bone": "Spine_Dummy", "holster_matrix": IDENTITY})
    weapons["dagger"]["attachment_status"] = "native0x715b84->0x717a90; R_Hand_Dummy only, no separate holster attachment"
    counter_scene = parse(extract(weapons["counter"]["source"]))
    counter_dummy = next(c for c in counter_scene["chunks"] if c["name"] == "Cs_Wea_Dummy")
    counter_equipped = inverse_rigid(matrix_convert(mul(counter_scene["matrix"], static_matrix(counter_dummy))))
    right, left = weapons["counter"]["parts"]
    right.update({"attach_bone": "R_Hand_Dummy", "attach_matrix": counter_equipped,
                  "holster_bone": "Gun_Dummy", "holster_matrix": IDENTITY,
                  "attachment_status": "native0x71554f->0x7176a0: model0 dummy child attaches hand; dummy retained under Gun_Dummy holster"})
    left.update({"attach_bone": "Bip01 L Forearm", "attach_matrix": IDENTITY,
                 "attachment_status": "native0x71556b..0x715582->0x717a90: model1 top-level children attach left forearm, dummy retained"})
    weapons["counter"].update({"attach_bone": right["attach_bone"], "attach_matrix": right["attach_matrix"],
                                "holster_bone": right["holster_bone"], "holster_matrix": IDENTITY,
                                "attachment_status": "two independently attached parts; see parts metadata"})
    trails = {}
    for slug, name in [("plasma", "GhostTrail_two_hand_sword_01"), ("dagger", "GhostTrail_SpyDagger_01"), ("counter", "GhostTrail_counter_sword_01")]:
        scene = parse(extract(weapons[slug]["source"]))
        nodes = {c["name"]: c for c in scene["chunks"]}
        shape = nodes["Line01"]
        def world(c):
            return mul(world(nodes[c["parent"]]), static_matrix(c)) if c["parent"] in nodes else mul(scene["matrix"], static_matrix(c))
        pair = shape["shapes"][0]
        endpoints = [pair[:3], [pair[i] + pair[i + 3] for i in range(3)]]
        trails[name] = {"shape": "Line01", "shape_parent": shape["parent"], "shape_matrix": matrix_convert(world(shape)),
                        "points": [convert(point(world(shape), p)) for p in endpoints],
                        "raw_shape_pairs": shape["shapes"], "texture": texture("gi.dds", "resources/image/"),
                        "source_texture": "resources/image/gi.dds", "requested_texture": "Resources/Image/gi.tga",
                        "native_constructor_arguments": [200, 50.0, 50, 100, 0, 0],
                        "native_constructor": "0x4e2490", "source_setup": {"plasma": "0x715254", "dagger": "0x715b84", "counter": "0x7154cf"}[slug],
                        "lifetime_ms": 200, "unit_length_m": .5, "fade_in_ms": 50, "fade_out_ms": 100,
                        "blend_type": 1, "glow": False, "haze": True,
                        "shape_pair_status": "native0xd1984a reads origin;0xd19861->0x43c070 adds delta to obtain other endpoint"}
        if slug == "counter": trails[name]["part"] = "right"
        second = name[:-2] + "02"
        trails[second] = {**trails[name], "texture": texture("gi2.dds", "resources/image/"),
                          "source_texture": "resources/image/gi2.dds", "requested_texture": "Resources/Image/gi2.tga"}
    for trail in trails.values():
        gradient = Path(trail["texture"]).with_stem(Path(trail["texture"]).stem + "-haze")
        shutil.copyfile(OUTPUT / trail["texture"], OUTPUT / gradient)
        trail["haze_texture"] = str(gradient)
    return {"executable_sha256": CLIENT_SHA256, "equipment_switch": "0x715110, table0x715ec0", "trails": trails}


def validate(result):
    raw = (OUTPUT / result["animations"]["data_file"]).read_bytes()
    animation = result["animations"]
    native_lower = native_counter_lower_actions()
    assert all(animation["actions"].get(name) == action for name, action in native_lower.items())
    assert len(raw) == animation["byte_length"]
    frames = 0
    for name, clip in animation["clips"].items():
        start = clip["byte_offset"]
        size = clip["frame_count"] * animation["floats_per_frame"] * 4
        assert 0 <= start <= start + size <= len(raw)
        values = array("f")
        values.frombytes(raw[start:start + size])
        if sys.byteorder != "little": values.byteswap()
        assert all(math.isfinite(v) for v in values)
        for frame in range(clip["frame_count"]):
            for bone in range(animation["bone_count"]):
                offset = frame * animation["floats_per_frame"] + bone * 10 + 3
                assert abs(sum(v * v for v in values[offset:offset + 4]) - 1) < 2e-6, (name, frame, bone)
        frames += clip["frame_count"]
    local_frames = 0
    skinned_meshes = 0
    for weapon in result["weapons"].values():
        for part in weapon.get("parts", [weapon]):
            for mesh in part["meshes"]:
                assert len(mesh["positions"]) == len(mesh["normals"]) == len(mesh["uvs"])
                assert all(0 <= i < len(mesh["positions"]) for i in mesh["indices"])
                assert all(math.isfinite(v) for p in mesh["positions"] + mesh["normals"] for v in p)
                assert (OUTPUT / mesh["texture"]).is_file()
                skinned_meshes += bool(mesh.get("source_skin"))
            for clip in part.get("mesh_animations", {}).values():
                counts = {mesh["name"]: len(mesh["positions"]) for mesh in part["meshes"]}
                for mesh in clip["meshes"]:
                    for frame in mesh["frames"]:
                        assert len(frame["positions"]) == counts[mesh["name"]]
                        assert all(math.isfinite(v) for p in frame["positions"] for v in p)
                        if "normals" in frame:
                            assert len(frame["normals"]) == len(frame["positions"])
                            assert all(abs(sum(v*v for v in n) - 1) < 1e-5 for n in frame["normals"])
                        local_frames += 1
    counter_calls = result["weapon_builders"]["StandardCounterSwordJumpDash"]["calls"]
    assert next(c["arguments"] for c in counter_calls if c["method"] == "SetDrop") == [400., "MAKEVECTOR3(0.0,-4000.0,-6000.0)"]
    assert any(c["target"] == "MAKECYLINDER" for c in counter_calls)
    assert any(c["method"] == "AddSequence" and c["arguments"] == [0, "Resources/Effects/weapon_counter_upper02.seq", "Resources/Sound/bsword_counter.ogg", 250, 5000]
               for c in result["weapon_builders"]["StandardCounterSwordCounterAttack"]["calls"])
    critical = result["ui_feedback"]["critical"]
    component, = critical["sequence"]["components"]
    assert component["class"] == "CActNodeLoader_IF" and component["screen_position"] == [0., 0., 1000.]
    assert component["fade_in_ms"] == 0 and component["fade_out_ms"] == 500
    assert "inter_critical_damage" not in result["effects"]["models"]
    for model in critical["models"].values():
        assert model["units"] == "source_screen_units"
        for mesh in model["meshes"]:
            assert (OUTPUT / mesh["texture"]).is_file()
            assert mesh["uvs"] == [[0., 1.], [1., 1.], [0., 0.], [1., 0.]]
            assert all(math.isfinite(v) for f in mesh["frames"] for p in f["positions"] for v in p)
    for entry in result["audio"].values():
        assert hashlib.sha256((OUTPUT / entry["path"]).read_bytes()).hexdigest() == entry["sha256"]
    report = {"status": "passed", "skeletal_clips": len(animation["clips"]), "actions": len(animation["actions"]),
              "frames": frames, "bone_count": animation["bone_count"], "binary_bytes": len(raw),
              "binary_sha256": hashlib.sha256(raw).hexdigest(), "weapons": list(result["weapons"]),
              "sequences": len(result["effects"]["sequences"]), "effect_models": len(result["effects"]["models"]),
              "audio_files": len(result["audio"]), "ghosttrails": len(result["ghosttrails"]),
              "local_mesh_frames": local_frames, "skinned_weapon_meshes": skinned_meshes,
              "missing_registry_dependencies": {slug: weapon["missing_dependencies"] for slug, weapon in result["weapons"].items() if weapon["missing_dependencies"]},
              "native_dependencies": result["effects"]["native_dependencies"], "ui_feedback_graphs": len(result["ui_feedback"]),
              "validations": ["all skeletal frame samples finite", "all quaternions normalized", "all binary clip ranges in bounds",
                              "mesh attributes and indices valid", "local weapon poses finite and normals normalized", "source weapon skin retained and baked", "Counter SetDrop open-call arguments retained", "UI critical graph separated, fields and unmodified UV checked", "all source audio hashes match", "all weapon texture paths exist"]}
    report["native_counter_lower_actions"] = list(native_lower)
    report["validations"].append("native Counter guard lower jump args and existing base clip references checked")
    (RECREATION / "research/melee-assets-validation.json").write_text(json.dumps(report, indent=2) + "\n")


def actions(builders):
    result = {}
    dirs = {"FRONT": "forward", "LEFT_FRONT": "left_forward", "LEFT": "left", "RIGHT_FRONT": "right_forward", "RIGHT": "right", "BACK": "backward", "LEFT_BACK": "left_backward", "RIGHT_BACK": "right_backward", "NORMAL": "idle"}
    phases = {"SwordAtkWeakState_PlasmaSword": ["weak_1", "weak_2", "weak_3"],
              "SwordAtkStrongState_PlasmaSword": ["strong_1", "strong_2", "strong_3"],
              "SwordJumpAtkWeakState_PlasmaSword": ["air_weak", "air_weak_recovery"],
              "SwordJumpAtkState_PlasmaSword": ["jump", "jump_recovery"],
              "SwordAtkStrong1State_PlasmaSword": ["charged", "charged_recovery"],
              "UseWeapon2State_PlasmaSword": ["dash", "dash_recovery"],
              "SwordJumpAtkState_SpyDagger": ["air_cut_1", "air_cut_2", "air_cut_recovery"],
              "UseWeapon1State_SpyDagger": ["cut_1", "cut_2"],
              "UseWeapon2State_SpyDagger": ["poke_1", "poke_2", "poke_recovery"],
              **{"SwordAtkAfterDash" + str(i) + "State_CounterSword": ["combo_" + str(i), "combo_" + str(i) + "_recovery"] for i in range(1, 5)},
              "SwordJumpAtkState_CounterSword": ["jump", "jump_recovery"],
              "SwordAtkCounterAttack_CounterSword": ["retaliation", "retaliation_recovery"],
              "UseWeapon1State_CounterSwordDash1": ["dash", "dash_active", "dash_recovery"],
              "UseWeapon2State_CounterSwordGuard": ["guard", "guard_recovery"],
              "DamageState_CounterSword": ["damage_" + str(i) for i in range(8)]}
    for fn, builder in builders.items():
        weapon = "counter" if "CounterSword" in fn else "plasma" if fn.endswith("PlasmaSword") else "dagger"
        phase = 0
        for call in builder["calls"]:
            if call["method"] != "SetAnim": continue
            target = call["target"]
            if fn.startswith("NormalState_"): key = weapon + "_idle_upper"
            elif fn.startswith("RunState_"):
                index = int(float(target.rsplit("(", 1)[-1][:-1]))
                kind = "upper" if index < 8 else "lower"
                direction = ["forward", "left_forward", "left", "left_backward", "right_forward", "right", "right_backward", "backward"][index % 8]
                key = weapon + "_run_" + direction + "_" + kind
            elif "GetWalkAnimParam" in target:
                direction = target.rsplit("(", 1)[-1][:-1].split("_ANIM_")[-1]
                stem = fn.split("State_")[0]
                key = weapon + "_" + stem + "_walk_" + dirs[direction] + "_lower"
            else:
                key = weapon + "_" + phases[fn][phase]
                phase += 1
            args = call["arguments"]
            args = [int(v) if isinstance(v, float) and v.is_integer() else v for v in args]
            result[key] = {"clip": args[0], "arguments": args[1:], "source_function": fn,
                           "source_line": call["source_line"], "target": target}
    return result


def native_counter_lower_actions():
    base = json.loads((OUTPUT / "player-animations.json").read_text())
    result = {}
    for phase, alias, clip, blend, loop, requested in [
        (0, "start", "00000", 500, False, 0),
        (1, "rise", "00001", 200, False, 700),
        (2, "fall", "00002", 1000, True, 700),
        (2, "fall_natural", "00002", 1000, True, 0),
        (3, "land", "00003", 200, False, 300),
    ]:
        base_key = {"00000": "jump_start", "00001": "jump_rise", "00002": "jump_fall", "00003": "jump_land"}[clip]
        assert base["clips"][base_key]["source_clip"] == clip
        result["counter_guard_jump_" + alias] = {
            "clip": base_key, "arguments": [0, blend, 0, loop, 1, True],
            "source_function": "native CLowerPartJumpState constructor", "source_line": 0,
            "target": "CLowerPartJumpState.GetAnimParam(" + str(phase) + ")",
            "source_native": {"constructor_range": "0x5bc1a3..0x5bc296", "source_clip": clip,
                "base_manifest": "player-animations.json", "base_clip": base_key,
                "phase": phase, "requested_duration_ms": requested,
                "assignment": "0x5bce84..0x5bcec7 timer expiry" if alias == "fall_natural" else "0x5bcf00 phase setter",
                "evidence": "research/counter-motion-native.txt"},
        }
        result["lower_jump_" + alias] = result["counter_guard_jump_" + alias]
    return result


def animations(action_table):
    scene = parse(extract("resources/model/character/male_bip.scn"))
    bones = [c for c in scene["chunks"] if c["type"] == 0x6D411AD1]
    player = json.loads((OUTPUT / "player.json").read_text())
    assert [b["name"] for b in bones] == [b["name"] for b in player["bones"]]
    def key(c, clip, seen=None):
        seen = set() if seen is None else seen
        assert clip not in seen
        a = next((a for a in c["animations"] if a["name"] == clip), None)
        if a and a.get("copy"): return key(c, a["copy"], seen | {clip})
        if a and "translation" in a.get("key", {}): return a["key"]
        if clip != "BASE": return key(c, "BASE", seen | {clip})
        raise ValueError((c["name"], clip))
    result = {"format_version": 3, "sample_hz": HZ, "source_time_unit": "ticks", "source_ticks_per_ms": TICKS_PER_MS,
              "data_file": "melee/melee-animations.bin", "pose_layout": "local_translation3_quaternion_xyzw4_scale3",
              "bone_count": len(bones), "floats_per_frame": len(bones) * 10, "clips": {}, "actions": action_table}
    required = sorted({v["clip"] for v in action_table.values()} - {""})
    present = {a["name"] for b in bones for a in b["animations"]}
    assert set(required) <= present, set(required) - present
    with (OUTPUT / result["data_file"]).open("wb") as out:
        for clip in required:
            tracks = [key_convert(key(b, clip)) for b in bones]
            duration = max(k["duration_ms"] for k in tracks)
            count = math.ceil(duration / TICKS_PER_MS * HZ / 1000) + 1
            result["clips"][clip] = {"source_clip": clip, "duration_ticks": duration, "frame_count": count, "byte_offset": out.tell()}
            for frame in range(count):
                tick = frame / (count - 1) * duration if count > 1 else 0
                values = array("f")
                for k in tracks:
                    values.extend(sample(k["translation"], k["translations"], tick))
                    values.extend(slerp(k["rotation"], sample(k["rotation"], k["rotations"], tick, True), 1.))
                    values.extend(sample(k["scale"], k["scales"], tick))
                assert len(values) == result["floats_per_frame"] and all(math.isfinite(v) for v in values)
                if sys.byteorder != "little": values.byteswap()
                values.tofile(out)
        result["byte_length"] = out.tell()
    result["actions"] = {**action_table, **native_counter_lower_actions()}
    return result


def update_counter_lower_actions_only():
    path = DEST / "melee.json"
    raw = path.read_bytes()
    result = json.loads(raw)
    native = native_counter_lower_actions()
    existing = result["animations"]["actions"]
    assert all(existing[name] == action for name, action in native.items() if name in existing), "Existing native aliases differ; regenerate explicitly"
    missing = {name: action for name, action in native.items() if name not in existing}
    if missing:
        marker = b'"actions":{'
        assert raw.count(marker) == 1
        offset = raw.index(marker) + len(marker)
        addition = json.dumps(missing, separators=(",", ":"), allow_nan=False).encode()[1:-1] + b","
        patched = raw[:offset] + addition + raw[offset:]
        assert patched[:offset] == raw[:offset] and patched[offset + len(addition):] == raw[offset:]
        path.write_bytes(patched)
        existing.update(missing)
    validate(result)
    print("Updated", len(missing), "shared native lower action aliases; skeletal bytes unchanged")


def root_motion(action_table):
    scene = parse(extract("resources/model/character/male_bip.scn"))
    node = next(c for c in scene["chunks"] if c["name"] == "MoveBox")
    tracks = {}
    for clip in sorted({v["clip"] for v in action_table.values()} - {""}):
        a = next((a for a in node["animations"] if a["name"] == clip), None)
        seen = set()
        while a and a.get("copy"):
            assert a["name"] not in seen
            seen.add(a["name"])
            a = next(a2 for a2 in node["animations"] if a2["name"] == a["copy"])
        if not a or "translation" not in a.get("key", {}):
            tracks[clip] = {"has_source_track": False, "keys": [], "status": "MoveBox has no named track; no authored translation"}
            continue
        k = a["key"]
        keys = k["translations"] or [[0, *k["translation"]], [k["duration_ms"], *k["translation"]]]
        tracks[clip] = {"has_source_track": True, "duration_ticks": k["duration_ms"],
                        "keys": [[time / TICKS_PER_MS / 1000, *convert(position)] for time, *position in keys],
                        "raw_keys": keys, "raw_translation": k["translation"]}
    return {"source": "resources/model/character/male_bip.scn", "node": "MoveBox", "ticks_per_ms": TICKS_PER_MS,
            "key_layout": "seconds,x_m,y_m,z_m", "coordinate_mapping": "[x,y,-z]*0.01; absolute local translation, subtract adjacent samples", "clips": tracks}


def write_hit_rules(result):
    attribute_methods = {"SetCounterAttack", "SetGuardData", "SetDamageData"}
    shake_methods = {"SetCameraShakeEnable", "SetCameraShakeWhenSuccess",
                     "SetCameraShakeSetFactor1", "SetCameraShakeSetFactor2"}
    rules = {
        "format_version": 1,
        "source": "melee.json: effects.attribute_calls and selected weapon camera-shake builders",
        "effects": {"attribute_calls": [call for call in result["effects"]["attribute_calls"]
                                        if call["method"] in attribute_methods]},
        "weapon_builders": {name: {
            "calls": [call for call in result["weapon_builders"][name]["calls"]
                      if call["method"] in shake_methods]}
            for name in ["StandardCounterSwordCounterAttack", "StandardSpyDaggerAttack1"]},
    }
    assert len(rules["weapon_builders"]["StandardCounterSwordCounterAttack"]["calls"]) == 4
    (DEST / "hit-rules.json").write_text(json.dumps(rules, separators=(",", ":"), allow_nan=False) + "\n")


def main():
    DEST.mkdir(parents=True, exist_ok=True)
    if not all((RESEARCH / name).exists() for name in TEXT):
        with tempfile.TemporaryDirectory(prefix="esper-melee-") as staging:
            decoded = Path(staging) / "decoded"
            run_import_process([sys.executable, str(TOOLS / "extract_inner_resources.py"), str(decoded), *TEXT], check=True, stdout=subprocess.DEVNULL)
            shutil.copytree(decoded, RESEARCH, dirs_exist_ok=True)
    source_report = json.loads((RESEARCH / "report.json").read_text())
    for source in source_report["resources"]:
        assert hashlib.sha256((RESEARCH / source["name"]).read_bytes()).hexdigest() == source["decoded_sha256"], source["name"]
    builders = collect_builders()
    action_table = actions(builders)
    registry = ET.parse(RESEARCH / "xml/weapon.x7").getroot()
    bindings = ET.parse(RESEARCH / "xml/weapon_lua.x7").getroot()
    weapons = {}
    for slug, item in WEAPONS.items():
        node = next(n for n in registry if n.get("item_key") == item)
        source = "resources/model/weapon/" + node.find("scene").get("value1").lower()
        weapons[slug] = {"item_key": int(item), "source": source, "ability": node.find("ability").attrib,
                         "resources": node.find("resources").attrib,
                         "lua_bindings": [n.attrib for n in bindings if n.get("item_key") == item],
                         "attach_bone": "R_Hand_Dummy", "attach_matrix": IDENTITY,
                         "attachment_status": "R_Hand_Dummy provisional pending original native attachment table",
                         **geometry(source)}
        weapons[slug]["other_effects"] = node.find("other_effects").attrib if node.find("other_effects") is not None else {}
        clips = sorted({a["name"] for c in parse(extract(source))["chunks"] for a in c.get("animations", []) if a["name"] not in ("SWORD", "base", "BASE")})
        weapons[slug]["mesh_animations"] = {clip: mesh_animation(source, clip) for clip in clips}
        if slug == "counter":
            parts = []
            for field, key, bone in [("value1", "right", "R_Hand_Dummy"), ("value2", "left", "L_Hand_Dummy")]:
                part_source = "resources/model/weapon/" + node.find("scene").get(field).lower()
                part_clips = sorted({a["name"] for c in parse(extract(part_source))["chunks"] for a in c.get("animations", []) if a["name"].lower() != "base"})
                parts.append({"key": key, "source": part_source, "attach_bone": bone, "attach_matrix": IDENTITY,
                              "attachment_status": "pending selected native attachment trace", **geometry(part_source),
                              "mesh_animations": {clip: mesh_animation(part_source, clip) for clip in part_clips},
                              "local_animation_inventory": {"authored_clips": part_clips,
                                  "authored_selected_clips": sorted(set(part_clips) & {a["clip"] for name, a in action_table.items() if name.startswith("counter_")}),
                                  "missing_named_selected_clips": sorted({a["clip"] for name, a in action_table.items() if name.startswith("counter_")} - set(part_clips)),
                                  "missing_clip_status": "no named local track exported; runtime keeps static source base, native missing-track fallback unresolved"}})
            weapons[slug]["parts"] = parts
    gameplay_builders = weapon_builders()
    effects, audio, ui, ui_feedback = effects_audio_ui(builders, weapons, gameplay_builders)
    result = {"format_version": 1, "weapons": weapons, "animations": animations(action_table), "builders": builders,
              "effects": effects, "audio": audio, "ui": ui, "ui_feedback": ui_feedback, "root_motion": root_motion(action_table),
              "weapon_builders": gameplay_builders, "native": native_metadata(weapons)}
    result["ghosttrails"] = result["native"]["trails"]
    result["source_report"] = "research/melee-assets-decoded/report.json"
    result["resource_report"] = "research/melee-assets-resource-report.json"
    (RECREATION / "research/melee-assets-resource-report.json").write_text(json.dumps({
        "status": "passed", "resources": [RESOURCE_REPORT[name] for name in sorted(RESOURCE_REPORT)],
        "locale_aliases": RESOURCE_ALIASES, "native_dependencies": effects["native_dependencies"]}, indent=2) + "\n")
    (DEST / "melee.json").write_text(json.dumps(result, separators=(",", ":"), allow_nan=False) + "\n")
    (DEST / "root-motion.json").write_text(json.dumps(result["root_motion"], separators=(",", ":"), allow_nan=False) + "\n")
    (RECREATION / "research/melee-assets-builders.json").write_text(json.dumps(builders, indent=2) + "\n")
    (RECREATION / "research/melee-assets-weapon-builders.json").write_text(json.dumps(result["weapon_builders"], indent=2) + "\n")
    write_hit_rules(result)
    validate(result)
    print("Exported", len(weapons), "weapons", len(result["animations"]["actions"]), "actions", len(result["animations"]["clips"]), "clips")


if __name__ == "__main__":
    if sys.argv[1:] == ["--hit-rules-only"]:
        write_hit_rules(json.loads((DEST / "melee.json").read_bytes()))
        print("Exported compact impact rules without changing geometry or animations")
    elif sys.argv[1:] == ["--counter-lower-actions-only"]:
        update_counter_lower_actions_only()
    else:
        assert not sys.argv[1:], "Unknown arguments"
        main()
