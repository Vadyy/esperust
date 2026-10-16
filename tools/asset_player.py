#!/usr/bin/env python3
import json
import math
from asset_scn import parse, extract, OUTPUT, trs, point, vector, mul, convert, matrix_convert, static_matrix, texture

IDENTITY=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]
CLIPS={"idle":"00074","run_forward":"00008","run_left_forward":"00009","run_left":"00010","run_right_forward":"00011","run_right":"00012","run_backward":"00013","run_left_backward":"00014","run_right_backward":"00015","jump_start":"00000","jump_rise":"00001","jump_fall":"00002","jump_land":"00003","jump_moving":"00004"}
CLIPS.update({"dodge_left":"00005","dodge_right":"00056","wall_jump_start":"00066","wall_jump_rise":"00067","wall_jump_rise_b":"00067_2","sprint":"00068"})

def qconvert(q): return [-q[0],-q[1],q[2],q[3]]

def key_convert(k):
    return {"duration_ms":k["duration_ms"],"translation":convert(k["translation"]),"rotation":qconvert(k["rotation"]),"scale":k["scale"],"translations":[[a[0],*convert(a[1:])] for a in k["translations"]],"rotations":[[a[0],*qconvert(a[1:])] for a in k["rotations"]],"scales":k["scales"]}

def export_player():
    skeleton=parse(extract("resources/model/character/male_bip.scn"))
    bones=[c for c in skeleton["chunks"] if c["type"] == 0x6D411AD1]
    lookup={c["name"]:i for i,c in enumerate(bones)}
    cache={}
    def anim_key(c,clip,seen=None):
        seen=set() if seen is None else seen
        assert clip not in seen, (c["name"],clip)
        a=next((a for a in c["animations"] if a["name"]==clip),None)
        if a and a.get("copy"): return anim_key(c,a["copy"],seen|{clip})
        if a and "translation" in a.get("key",{}): return a["key"]
        if clip != "BASE": return anim_key(c,"BASE",seen)
        return None
    def world(i,clip="00074"):
        if (i,clip) in cache:return cache[(i,clip)]
        c=bones[i]
        k=anim_key(c,clip)
        if k:
            k=dict(k)
            for field,keys in [("translation","translations"),("rotation","rotations"),("scale","scales")]:
                if k[keys]: k[field]=k[keys][0][1:]
            m=trs(k)
        else:m=c["matrix"]
        parent=lookup.get(c["parent"])
        if parent is not None:m=mul(world(parent,clip),m)
        cache[(i,clip)]=m
        return m
    result={"format_version":1,"units":"meters","coordinate_mapping":"[x,y,-z] * 0.01 from source Y-up left-handed","meshes":[],"bones":[],"clips":{},"initial_clip":"idle","sources":["resources/model/character/male_bip.scn"]}
    for c in bones:
        key=anim_key(c,"BASE")
        result["bones"].append({"name":c["name"],"parent":lookup.get(c["parent"]),"base":key_convert(key) if key else None,"matrix":matrix_convert(c["matrix"])})
    for name,clip in CLIPS.items():
        keys=[anim_key(c,clip) for c in bones]
        result["clips"][name]={"source_clip":clip,"duration_ms":max((k["duration_ms"] for k in keys if k),default=0),"tracks":[key_convert(k) if k else None for k in keys]}
    for typ in ["body","leg","hand","foot","face","hair"]:
        name="resources/model/character/"+typ+"/00_male_"+typ+".scn"
        scene=parse(extract(name))
        result["sources"].append(name)
        nodes={c["name"]:c for c in scene["chunks"]}
        def localworld(c):
            m=static_matrix(c)
            if c["parent"] in nodes: return mul(localworld(nodes[c["parent"]]),m)
            return m
        for c in scene["chunks"]:
            if c["type"] != 0x081098F8:continue
            skin=[[] for _ in c["positions"]]
            poses=[]
            joint_matrices=[]
            for w in c["weights"]:
                i=lookup[w["name"]]
                poses.append({"bone":i,"inverse_bind":matrix_convert(w["matrix"])})
                joint_matrices.append(mul(world(i),w["matrix"]))
                for vi,weight in w["weights"]:
                    skin[vi].append({"joint":len(poses)-1,"weight":weight})
            positions=[]
            normals=[]
            attach=lookup.get("Bip01 Head") if typ in ("face","hair") else None
            attach_matrix=localworld(c)
            if attach is not None:attach_matrix=mul(world(attach),attach_matrix)
            for vi,p in enumerate(c["positions"]):
                normal=c["normals"][vi] if c["normals"] else [0,1,0]
                if skin[vi]:
                    v=[0,0,0];n=[0,0,0];total=sum(w["weight"] for w in skin[vi])
                    for weight in skin[vi]:
                        m=joint_matrices[weight["joint"]]
                        p1=point(m,p);n1=vector(m,normal)
                        for j in range(3):v[j]+=p1[j]*weight["weight"]/total;n[j]+=n1[j]*weight["weight"]/total
                    length=math.sqrt(sum(q*q for q in n))
                    if length:n=[q/length for q in n]
                else:v=point(attach_matrix,p);n=vector(attach_matrix,normal)
                positions.append(convert(v));normals.append(convert(n,1))
            for j,t in enumerate(c["textures"]):
                result["meshes"].append({"name":typ+"/"+c["name"]+"/"+str(j),"positions":positions,"normals":normals,"bind_positions":[convert(p) for p in c["positions"]],"bind_normals":[convert(n,1) for n in c["normals"]],"uvs":c["uvs"],"indices":[i for f in c["faces"][t["face_offset"]:t["face_offset"]+t["face_count"]] for i in (f[0],f[2],f[1])],"texture":texture(t["main"],"resources/model/character/"+typ+"/"),"side_texture":texture(t["side"],"resources/model/character/"+typ+"/"),"shader_flags":c["shader_flags"],"visible":True,"collision":False,"sky":False,"skin":skin,"joints":poses,"attach_bone":attach,"attach_matrix":matrix_convert(localworld(c)) if attach is not None else None})
    positions=[p for m in result["meshes"] for p in m["positions"]]
    result["bounds"]={"min":[min(p[i] for p in positions) for i in range(3)],"max":[max(p[i] for p in positions) for i in range(3)]}
    (OUTPUT/"player.json").write_text(json.dumps(result,separators=(",",":"),allow_nan=False))
    print(len(result["bones"]),"bones",len(result["meshes"]),"meshes",len(positions),"vertices",result["bounds"])
    print("clips",[(k,v["source_clip"],v["duration_ms"]) for k,v in result["clips"].items()])

if __name__=="__main__":export_player()
