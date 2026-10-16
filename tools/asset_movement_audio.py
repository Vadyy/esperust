#!/usr/bin/env python3
from pathlib import Path
import hashlib
import json
import struct
import sys
import xml.etree.ElementTree as ET

from asset_extract import extract, INDEX, OUTPUT

from import_context import RECREATION as ROOT
from resource_reader import extract_bytes, CLIENT_SHA256 as EXPECTED
SURFACES = ["ground", "rock", "steel", "wooden", "water", "glass"]


def main():
    assets = []
    for filename in ["dodge.ogg", "jump_up.ogg", "dash.ogg", "warning.ogg", "voice_man_fastrun.ogg"] + [
        f"{prefix}_{surface}{suffix}.ogg" for surface in SURFACES
        for prefix, suffix in [("land", "")] + [("run", f"_{i}") for i in range(1, 5)]
    ]:
        name = "resources/sound/" + filename
        dest = extract(name)
        data = dest.read_bytes()
        assert data.startswith(b"OggS") and b"\x01vorbis" in data[:128], filename
        assets.append({"source": name, "resource_checksum": INDEX[name]["checksum"],
                       "path": str(dest.relative_to(OUTPUT)), "bytes": len(data),
                       "sha256": hashlib.sha256(data).hexdigest()})
    tables = {}
    for surface, address in zip(SURFACES, [0x1198DB4, 0x1198DA4, 0x1198D94,
                                         0x1198D84, 0x1198D74, 0x1198D64]):
        tables[surface] = {"id": SURFACES.index(surface) + 1, "address": hex(address),
                           "files": [f"resources/sound/run_{surface}_{i}.ogg" for i in range(1, 5)]}
    option = ET.fromstring(extract_bytes("language/default_option.x7", inner=True))
    sfx_volume = int(option.find("sound/sfxvolume").get("value"))
    manifest = {"executable_sha256": EXPECTED, "surface_tables": tables,
                "sprint_voice_registration": {
                    "source": "ActorStates_GamePlay.lua:1312",
                    "function": "ActorState_FastRun",
                    "method": "AddSoundBySex",
                    "male": "resources/sound/voice_man_fastrun.ogg",
                    "female": "resources/sound/voice_girl_fastrun.ogg",
                    "attachment": "Bip01", "arguments": [0, 2500],
                    "runtime": "male starter; once per accepted Sprint entry"},
                "footstep_phase_offset_ms": 200, "footstep_phase_period_ms": 333,
                "foot_volume": 0.5, "sfx_volume_percent": sfx_volume, "assets": assets}
    (OUTPUT / "movement-audio.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Imported {len(assets)} original Vorbis files at original SFX option {sfx_volume}/100.")


if __name__ == "__main__":
    main()
