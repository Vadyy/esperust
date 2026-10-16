#!/usr/bin/env python3
from pathlib import Path
import json
import math
import re
from functools import lru_cache
from asset_extract import extract, INDEX, OUTPUT

from scn_decode import Reader, parse

def mul(a,b):
    return [sum(a[k*4+row]*b[col*4+k] for k in range(4)) for col in range(4) for row in range(4)]

def point(m,p):
    return [sum(m[k*4+i]*p[k] for k in range(3))+m[12+i] for i in range(3)]

def vector(m,p):
    q=[sum(m[k*4+i]*p[k] for k in range(3)) for i in range(3)]
    n=math.sqrt(sum(x*x for x in q))
    return [v/n for v in q] if n else [0,1,0]

def convert(p,scale=0.01): return [p[0]*scale,p[1]*scale,-p[2]*scale]

def trs(key):
    x,y,z,w=key["rotation"]
    n=math.sqrt(x*x+y*y+z*z+w*w)
    x,y,z,w=x/n,y/n,z/n,w/n
    sx,sy,sz=key["scale"]
    tx,ty,tz=key["translation"]
    return [(1-2*(y*y+z*z))*sx,2*(x*y+z*w)*sx,2*(x*z-y*w)*sx,0,
            2*(x*y-z*w)*sy,(1-2*(x*x+z*z))*sy,2*(y*z+x*w)*sy,0,
            2*(x*z+y*w)*sz,2*(y*z-x*w)*sz,(1-2*(x*x+y*y))*sz,0,
            tx,ty,tz,1]

def static_matrix(c):
    animations=c.get("animations",[])
    a=next((a for a in animations if a["name"]=="BASE"),animations[0] if animations else {})
    if "translation" in a.get("key",{}): return trs(a["key"])
    return c["matrix"]

def matrix_convert(m):
    signs=[1,1,-1,1]
    out=[m[j*4+i]*signs[i]*signs[j] for j in range(4) for i in range(4)]
    out[12:15]=convert(m[12:15])
    return out

@lru_cache(maxsize=None)
def texture(name,resource_directory="resources/model/background/",legacy=False):
    if not name: return None
    name=name.lower().replace("\\","/").rsplit("/",1)[-1]
    name=str(Path(name).with_suffix(".dds"))
    candidates=INDEX.texture_candidates(name, resource_directory)
    if not candidates:
        print("MISSING TEXTURE", name)
        return None
    preferred=[n for n in candidates if n.startswith(resource_directory)]
    path=extract((preferred or candidates)[0])
    from PIL import Image
    dest=OUTPUT / "textures" / (("legacy-" if legacy else "") + str(Path(name).with_suffix(".png")))
    dest.parent.mkdir(parents=True,exist_ok=True)
    Image.open(path).save(dest)
    return str(dest.relative_to(OUTPUT))

MAPS = {
    "circle-2": {"name":"Circle 2", "output":"circle2.json", "main":"ds_circle2.scn",
        "resources":[("STATIC","ds_circle2.scn"),("SKY","sky_temple-m.scn"),("STATIC","standbyroom.scn")]},
    "station-2": {"name":"Station 2", "output":"station2.json", "main":"ds5_station.scn",
        "profile":"bginfo-sstation02.ini"},
    "neden-1": {"name":"Neden 1", "output":"neden1.json", "main":"ds1_neden01.scn",
        "profile":"bginfo-neden01.ini"},
}

def profile_resources(name):
    path=extract("resources/mapinfo/"+name)
    section=""
    resources=[]
    renderer={}
    for line in path.read_text(encoding="cp1252").splitlines():
        line=line.split(";",1)[0].strip()
        if line.startswith("[") and line.endswith("]"):
            section=line[1:-1].upper()
        elif "=" in line:
            key,value=(v.strip() for v in line.split("=",1))
            if section in ("SKY","STATIC","DYNAMIC","GAMERULE") and value.lower().endswith(".scn"):
                resources.append((section,value.lower()))
            elif section=="RENDERER":
                try: renderer[key]=float(value)
                except ValueError: renderer[key]=value
    return resources,renderer

def export_map(map_id="circle-2"):
    profile=MAPS[map_id]
    resources,renderer=profile_resources(profile["profile"]) if "profile" in profile else (profile["resources"],{})
    result={"format_version":1,"id":map_id,"name":profile["name"],"units":"meters",
        "coordinate_mapping":"[x,y,-z] * 0.01 from source Y-up left-handed",
        "profile":profile.get("profile"),"renderer":renderer,
        "meshes":[],"boxes":[],"spawns":[],"sources":[]}
    for category,resource in resources:
        path=extract("resources/model/background/"+resource)
        scene=parse(path)
        result["sources"].append({"resource":resource,"category":category,"chunks":len(scene["chunks"]),"bytes":path.stat().st_size})
        nodes={c["name"]:c for c in scene["chunks"]}
        def world(c,seen=None):
            seen=set() if seen is None else seen
            assert c["name"] not in seen
            if c["parent"] in nodes:
                return mul(world(nodes[c["parent"]],seen|{c["name"]}),static_matrix(c))
            return mul(scene["matrix"],static_matrix(c))
        for c in scene["chunks"]:
            m=world(c)
            if c["type"] == 0x081098F8:
                lower=c["name"].lower()
                visible=not (lower.startswith(("oct_","shadowsurface")) or "occlusion" in resource)
                ancestor=nodes.get(c["parent"])
                while ancestor:
                    root=nodes.get(ancestor["parent"])
                    if root and root["name"].lower().startswith("blast2") and ancestor["type"]==0x081098F8:
                        visible=False
                        break
                    ancestor=root
                collision=lower.startswith("oct_") and not lower.startswith("oct_weapon")
                positions=[convert(point(m,p)) for p in c["positions"]]
                normals=[convert(vector(m,n),1) for n in c["normals"]]
                for j,t in enumerate(c["textures"] or [{"main":"","side":"","face_offset":0,"face_count":len(c["faces"])}]):
                    key=c.get("animations",[{}])[0].get("key",{})
                    alpha=key.get("alpha",[[0,1]])[0][1] if key.get("alpha") else 1
                    legacy_lightmap=bool(t["side"] and c["uvs2"])
                    main_texture = texture(t["main"], legacy=legacy_lightmap)
                    enemy_texture = None
                    team = 1 if "alphateam" in lower else 2 if "betateam" in lower else 0
                    if team and "_atex" in t["main"].lower():
                        enemy_texture = texture(t["main"].lower().replace("_atex", "_etex"))
                    animation = None
                    sector = re.match(r"(sector\d+)_(ally|enemy)_", lower)
                    if resource == "ds7_safeline.scn" and key.get("alpha"):
                        animation = {"duration_ticks": key["duration_ms"], "alpha": key["alpha"]}
                    result["meshes"].append({"name":resource+"/"+c["name"]+"/"+str(j),
                        "positions":positions,"normals":normals,"uvs":c["uvs"],"uvs2":c["uvs2"],
                        "indices":[i for f in c["faces"][t["face_offset"]:t["face_offset"]+t["face_count"]] for i in (f[0],f[2],f[1])],
                        "texture":main_texture,"side_texture":texture(t["side"], legacy=legacy_lightmap),
                        "enemy_texture":enemy_texture,"team":team,"animation":animation,
                        "sector":sector[1] if sector else None,"sector_enemy":bool(sector and sector[2]=="enemy"),
                        "shader_flags":c["shader_flags"],"visible":visible,"collision":collision,
                        "sky":category=="SKY","dynamic":category=="DYNAMIC","alpha":alpha})
            elif c["type"] == 0x25ADF0D1:
                box={"name":c["name"],"resource":resource,"position":convert(m[12:15]),"matrix":m,"size":[abs(v)*.01 for v in c["size"]]}
                result["boxes"].append(box)
                if "_spawn_pos" in c["name"].lower() and not c["name"].lower().startswith("ball_"):
                    result["spawns"].append({"name":c["name"],"position":box["position"]})
    visible=[p for m in result["meshes"] if m["visible"] and m["name"].startswith(profile["main"]+"/") for p in m["positions"]]
    result["bounds"]={"min":[min(p[i] for p in visible) for i in range(3)],"max":[max(p[i] for p in visible) for i in range(3)]}
    (OUTPUT/profile["output"]).write_text(json.dumps(result,separators=(",",":"),allow_nan=False))
    missing=[m["name"] for m in result["meshes"] if m["visible"] and m["side_texture"] and not m["texture"]]
    assert not missing, ("Lightmapped mesh lacks a base texture",missing)
    print("RESULT",map_id,len(result["sources"]),"SCNs",len(result["meshes"]),"mesh parts",len(result["spawns"]),"player spawns",result["bounds"],flush=True)
    return result

if __name__ == "__main__":
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--map",choices=MAPS,action="append",help="Export only selected maps; the default exports every supported map")
    args=parser.parse_args()
    for map_id in args.map or MAPS:
        export_map(map_id)
