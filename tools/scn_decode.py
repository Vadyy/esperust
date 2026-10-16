from pathlib import Path
import math
import struct
import zlib


class SceneDecodeError(ValueError):
    pass


class Cursor:
    def __init__(self, data, trace=None):
        self.data = bytes(data)
        self.pos = 0
        self.trace = trace
        self.scope = "scene"

    def fail(self, message):
        raise SceneDecodeError(f"{self.scope} at byte 0x{self.pos:x}: {message}")

    def take(self, size, field):
        start = self.pos
        if size < 0 or size > len(self.data) - start:
            self.fail(f"{field} needs {size} bytes; {len(self.data) - start} remain")
        self.pos += size
        if self.trace is not None:
            self.trace.append({"record": self.scope, "field": field,
                               "offset": start, "bytes": size})
        return self.data[start:self.pos]

    def values(self, layout, field):
        codec = struct.Struct("<" + layout)
        return list(codec.unpack(self.take(codec.size, field)))

    def word(self, field):
        return self.values("I", field)[0]

    def scalar(self, field):
        value = self.values("f", field)[0]
        if not math.isfinite(value):
            self.fail(f"non-finite {field}")
        return value

    def text(self, field, width=None):
        if width is None:
            end = self.data.find(b"\0", self.pos)
            if end < 0:
                self.fail(f"unterminated {field}")
            raw = self.take(end + 1 - self.pos, field)[:-1]
        else:
            slot = self.take(width, field)
            end = slot.find(b"\0")
            if end < 0:
                self.fail(f"{field} has no terminator in its {width}-byte slot")
            raw = slot[:end]
        return raw.decode("cp1252", errors="surrogateescape")

    def count(self, field, minimum_size):
        value = self.word(field + ".count")
        if value > (len(self.data) - self.pos) // minimum_size:
            self.fail(f"{field} count {value} cannot fit in remaining bytes")
        return value

    def rows(self, field, layout, count=None):
        codec = struct.Struct("<" + layout)
        if count is None:
            count = self.count(field, codec.size)
        raw = self.take(count * codec.size, field)
        return [list(row) for row in codec.iter_unpack(raw)]


def class_tag(name):
    return zlib.crc32(name.encode("ascii"))


VERSION_02 = 0.20000000298023224
VERSION_01 = 0.10000000149011612


def _revision(stream, field):
    raw = stream.word(field)
    value = struct.unpack("<f", struct.pack("<I", raw))[0]
    if not math.isfinite(value) or value < 0:
        stream.fail(f"unsupported {field}: {value}")
    return raw, value


def _node_header(stream):
    start = stream.pos
    tag = stream.word("class_tag")
    name = stream.text("name")
    parent = stream.text("parent")
    stream.scope = name or "unnamed node"
    revision, _ = _revision(stream, "transform_revision")
    matrix = stream.values("16f", "local_matrix")
    subtype_revision, numeric_revision = _revision(stream, "node_revision")
    return {"offset": start, "type": tag, "name": name, "parent": parent,
            "version": revision, "matrix": matrix, "version2": subtype_revision}, numeric_revision


def _motion(stream):
    key = {"duration_ms": stream.word("duration_ticks")}
    present = stream.values("B", "transform_present")[0]
    if present not in (0, 1):
        stream.fail(f"invalid transform presence byte {present}")
    if present:
        for field, layout in (("translation", "3f"), ("rotation", "4f"), ("scale", "3f")):
            key[field] = stream.values(layout, field)
        for field, layout in (("translations", "I3f"), ("rotations", "I4f"), ("scales", "I3f")):
            key[field] = stream.rows(field, layout)
    key["alpha"] = stream.rows("alpha", "If")
    return key


def _clips(stream, copy_names=False, morph=False):
    clips = []
    for _ in range(stream.count("clips", 2 if copy_names else 1)):
        clip = {"name": stream.text("clip_name")}
        if copy_names:
            clip["copy"] = stream.text("copy_name")
        elif not morph:
            clip["copy"] = ""
        if not clip.get("copy"):
            clip["key"] = _motion(stream)
            if morph:
                frames = []
                for _ in range(stream.count("morph_frames", 12)):
                    frames.append({"time_ms": stream.word("morph_timestamp_ticks"),
                                   "rotations": stream.rows("morph_vectors4", "4f"),
                                   "positions": stream.rows("morph_positions", "3f")})
                clip["morph_keys"] = frames
        clips.append(clip)
    return clips


def _mesh(stream, node, revision):
    node["shader_flags"] = stream.word("render_flags")
    mesh_revision = stream.scalar("mesh_revision")
    node["texture_version"] = mesh_revision
    uv_layout = stream.word("uv_layout") if mesh_revision >= VERSION_02 else 0
    if uv_layout not in (0, 1, 2):
        stream.fail(f"unsupported UV layout {uv_layout}")
    materials = []
    material_size = 2056 if mesh_revision >= VERSION_02 else 1032
    for _ in range(stream.count("materials", material_size)):
        primary = stream.text("texture_primary", 1024)
        secondary = stream.text("texture_secondary", 1024) if mesh_revision >= VERSION_02 else ""
        face_start, face_length = stream.values("II", "material_faces")
        materials.append({"main": primary, "side": secondary,
                          "face_offset": face_start, "face_count": face_length})
    node["textures"] = materials
    for field, layout in (("positions", "3f"), ("faces", "3H"), ("normals", "3f"), ("uvs", "2f")):
        node[field] = stream.rows(field, layout)
    node["uvs2"] = stream.rows("uvs2", "2f", len(node["uvs"])) if uv_layout == 1 else []
    node["tangents"] = stream.rows("tangents", "3f")
    bindings = []
    for _ in range(stream.count("skin_bindings", 69)):
        bindings.append({"name": stream.text("binding_name"),
                         "matrix": stream.values("16f", "binding_matrix"),
                         "weights": stream.rows("vertex_weights", "If")})
    node["weights"] = bindings
    node["animations"] = _clips(stream, morph=True)
    vertices = len(node["positions"])
    if any(index >= vertices for face in node["faces"] for index in face):
        stream.fail("triangle references a vertex outside positions")
    for field in ("normals", "uvs"):
        if len(node[field]) not in (0, vertices):
            stream.fail(f"{field} count does not match positions")
    if any(item["face_offset"] + item["face_count"] > len(node["faces"]) for item in materials):
        stream.fail("material range exceeds triangles")
    if any(index >= vertices for binding in bindings for index, _ in binding["weights"]):
        stream.fail("skin weight references a vertex outside positions")


def _bone(stream, node, revision):
    node["animations"] = _clips(stream, copy_names=revision >= VERSION_02)


def _box(stream, node, revision):
    node["box_version"], box_revision = _revision(stream, "box_revision")
    if box_revision < VERSION_01:
        stream.fail("box revision below the original client's supported minimum")
    center = stream.values("3f", "box_center")
    node["center"] = center
    node["basis"] = stream.values("9f", "box_basis")
    node["size"] = stream.values("3f", "box_half_extents")


def _shape(stream, node, revision):
    if revision < VERSION_01:
        stream.fail("shape revision below the original client's supported minimum")
    node["shapes"] = stream.rows("shape_endpoints", "6f")


def _light(stream, node, revision):
    if revision < VERSION_01:
        stream.fail("light revision below the original client's supported minimum")
    node["light_type"] = stream.word("light_type")
    node["light_colors"] = stream.rows("light_colors_rgba", "4f", 3)
    node["direction"] = stream.values("3f", "light_direction")
    node["light_parameter_words"] = stream.values("7I", "light_parameters")


def _skeleton(stream, node, revision):
    pass


LOADERS = {class_tag(name): loader for name, loader in (
    ("CGeometry", _mesh), ("CBone", _bone), ("CBoxNode", _box),
    ("CShapeNode", _shape), ("CLightNode", _light), ("CBoneSystem", _skeleton))}


def decode_scene(data, *, trace=None):
    stream = Cursor(data, trace)
    if stream.word("container_count") != 1:
        stream.fail("only a single scene group is supported")
    root, revision = _node_header(stream)
    if root["type"] != class_tag("CSceneGroup"):
        stream.fail("outer record is not CSceneGroup")
    count = stream.count("nodes", 78)
    if revision >= VERSION_02:
        stream.text("group_annotation")
    nodes = []
    for ordinal in range(count):
        node, node_revision = _node_header(stream)
        loader = LOADERS.get(node["type"])
        if loader is None:
            stream.fail(f"unsupported node class CRC32 0x{node['type']:08x}")
        loader(stream, node, node_revision)
        node["ordinal"] = ordinal
        node["end"] = stream.pos
        nodes.append(node)
    if stream.pos != len(stream.data):
        stream.fail(f"{len(stream.data) - stream.pos} unconsumed bytes")
    return {"name": root["name"], "matrix": root["matrix"], "chunks": nodes}


def parse(path):
    return decode_scene(Path(path).read_bytes())


class Reader(Cursor):
    def read(self, fmt):
        values = self.values(fmt, "consumer_field")
        return values[0] if len(values) == 1 else values

    def skip(self, size):
        self.take(size, "consumer_skip")

    def string(self, length=None):
        return self.text("consumer_string", length)

    def array(self, fmt, count=None):
        rows = self.rows("consumer_array", fmt, count)
        return [row[0] if len(row) == 1 else row for row in rows]
