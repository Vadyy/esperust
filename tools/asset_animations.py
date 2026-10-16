#!/usr/bin/env python3
import bisect
import json
import math
from array import array
from asset_scn import OUTPUT, mul, trs

HZ=60
TICKS_PER_MS=4.800000190734863

def actions():
    result={"idle":{"clip":"idle","arguments":[0,1000,0,True,1,False]}}
    for name in ("run_forward","run_left_forward","run_left","run_right_forward","run_right","run_backward","run_left_backward","run_right_backward"):
        result[name]={"clip":name,"arguments":[0,1000,0,True,1,False],"upper_arguments":[0,2000,0,True,1,False]}
    for name,clip,args in [
        ("jump_start","jump_start",[0,500,1000,False,1,True]),
        ("jump_rise","jump_rise",[0,500,0,False,1,True]),
        ("jump_moving","jump_moving",[600,100,300,False,1,True]),
        ("jump_land","jump_land",[500,200,500,False,0.8,True]),
        ("jump_land_moving","jump_start",[0,100,0,False,0.8,True]),
        ("jump_fall","jump_fall",[0,1000,0,True,1,True]),
        ("fall_land","jump_land",[380,200,300,False,1,True]),
        ("dodge_left","dodge_left",[0,300,700,False,1,True]),
        ("dodge_right","dodge_right",[0,300,700,False,1,True]),
        ("wall_jump_start","wall_jump_start",[500,0,500,False,1,True]),
        ("wall_jump_rise","wall_jump_rise",[500,100,0,False,1,True]),
        ("wall_jump_rise_b","wall_jump_rise_b",[500,200,1000,False,0.8,True]),
        ("wall_jump_fall","jump_fall",[0,500,0,True,1,True]),
        ("wall_jump_land","jump_land",[0,500,0,False,1,True]),
        ("sprint","sprint",[0,200,0,True,1,True]),
    ]:result[name]={"clip":clip,"arguments":args}
    return result

def lerp(a,b,t):return [x+(y-x)*t for x,y in zip(a,b)]

def slerp(a,b,t):
    dot=sum(x*y for x,y in zip(a,b))
    if dot<0:b=[-x for x in b];dot=-dot
    dot=min(1,max(-1,dot))
    if dot>0.9995:q=lerp(a,b,t)
    else:
        angle=math.acos(dot);s=math.sin(angle)
        a0=math.sin((1-t)*angle)/s;b0=math.sin(t*angle)/s
        q=[a0*x+b0*y for x,y in zip(a,b)]
    n=math.sqrt(sum(x*x for x in q))
    return [x/n for x in q]

def sample(default,keys,t,rotation=False):
    if not keys:return default
    i=bisect.bisect_right(keys,t,key=lambda k:k[0])-1
    if i<0:return keys[0][1:]
    if i>=len(keys)-1:return keys[-1][1:]
    a,b=keys[i],keys[i+1]
    f=(t-a[0])/(b[0]-a[0])
    return slerp(a[1:],b[1:],f) if rotation else lerp(a[1:],b[1:],f)

def point(m,p):
    x,y,z=p
    return [m[0]*x+m[4]*y+m[8]*z+m[12],m[1]*x+m[5]*y+m[9]*z+m[13],m[2]*x+m[6]*y+m[10]*z+m[14]]

def vector(m,p):
    x,y,z=p
    return [m[0]*x+m[4]*y+m[8]*z,m[1]*x+m[5]*y+m[9]*z,m[2]*x+m[6]*y+m[10]*z]

def bake_geometry():
    source=json.loads((OUTPUT/"player.json").read_text())
    result={"format_version":2,"sample_hz":HZ,"source_time_unit":"ticks","source_ticks_per_ms":TICKS_PER_MS,"data_file":"player-animations.bin","floats_per_frame":0,"mesh_layout":[],"clips":{},"actions":actions()}
    for m in source["meshes"]:
        n=len(m["positions"])
        result["mesh_layout"].append({"name":m["name"],"vertex_count":n,"positions_float_offset":result["floats_per_frame"],"normals_float_offset":result["floats_per_frame"]+3*n})
        result["floats_per_frame"]+=6*n
    with (OUTPUT/result["data_file"]).open("wb") as out:
        for name,clip in source["clips"].items():
            duration_ticks=clip["duration_ms"]
            count=math.ceil(duration_ticks/TICKS_PER_MS*HZ/1000)+1
            result["clips"][name]={"source_clip":clip["source_clip"],"duration_ticks":duration_ticks,"frame_count":count,"byte_offset":out.tell()}
            for frame in range(count):
                time=frame/(count-1)*clip["duration_ms"]
                local=[]
                for b,track in zip(source["bones"],clip["tracks"]):
                    if not track:local.append(b["matrix"]);continue
                    k={"translation":sample(track["translation"],track["translations"],time),"rotation":sample(track["rotation"],track["rotations"],time,True),"scale":sample(track["scale"],track["scales"],time)}
                    local.append(trs(k))
                cache={}
                def world(i):
                    if i not in cache:
                        parent=source["bones"][i]["parent"]
                        cache[i]=mul(world(parent),local[i]) if parent is not None else local[i]
                    return cache[i]
                values=array("f")
                for mesh in source["meshes"]:
                    joints=[mul(world(j["bone"]),j["inverse_bind"]) for j in mesh["joints"]]
                    attach=mul(world(mesh["attach_bone"]),mesh["attach_matrix"]) if mesh["attach_bone"] is not None else None
                    positions=[];normals=[]
                    for vi,p in enumerate(mesh["bind_positions"]):
                        normal=mesh["bind_normals"][vi]
                        skin=mesh["skin"][vi]
                        if skin:
                            v=[0,0,0];n=[0,0,0];total=sum(w["weight"] for w in skin)
                            for weight in skin:
                                m=joints[weight["joint"]];w=weight["weight"]/total
                                q=point(m,p);r=vector(m,normal)
                                v[0]+=q[0]*w;v[1]+=q[1]*w;v[2]+=q[2]*w
                                n[0]+=r[0]*w;n[1]+=r[1]*w;n[2]+=r[2]*w
                        else:v=point(attach,p);n=vector(attach,normal)
                        length=math.sqrt(n[0]*n[0]+n[1]*n[1]+n[2]*n[2])
                        if length:n=[n[0]/length,n[1]/length,n[2]/length]
                        positions.extend(v);normals.extend(n)
                    values.extend(positions);values.extend(normals)
                assert len(values)==result["floats_per_frame"]
                if __import__("sys").byteorder!="little":values.byteswap()
                values.tofile(out)
            print(name,count,"frames",out.tell(),"bytes",flush=True)
        result["byte_length"]=out.tell()
    (OUTPUT/"player-animations.json").write_text(json.dumps(result,indent=2)+"\n")

def retain_geometry_reference():
    path=OUTPUT/"player-animations.json"
    if not path.exists():return
    old=json.loads(path.read_text())
    if old.get("format_version")!=2:return
    raw=(OUTPUT/old["data_file"]).read_bytes()
    size=old["floats_per_frame"]*4
    reference={"source":"original matrix skinning exporter, before local TRS migration",
               "data_file":"player-animation-reference.bin","floats_per_frame":old["floats_per_frame"],
               "mesh_layout":old["mesh_layout"],"poses":[]}
    with (OUTPUT/reference["data_file"]).open("wb") as out:
        for name,clip in old["clips"].items():
            for frame in sorted({0,(clip["frame_count"]-1)//2,clip["frame_count"]-1}):
                start=clip["byte_offset"]+frame*size
                reference["poses"].append({"clip":name,"ticks":frame/(clip["frame_count"]-1)*clip["duration_ticks"],"byte_offset":out.tell()})
                out.write(raw[start:start+size])
    (OUTPUT/"player-animation-reference.json").write_text(json.dumps(reference,indent=2)+"\n")

def bake():
    if not (OUTPUT/"player-animation-reference.json").exists() or not (OUTPUT/"player-animation-reference.bin").exists():
        path=OUTPUT/"player-animations.json"
        if not path.exists() or json.loads(path.read_text()).get("format_version")!=2:
            bake_geometry()
    retain_geometry_reference()
    source=json.loads((OUTPUT/"player.json").read_text())
    count_bones=len(source["bones"])
    for i,b in enumerate(source["bones"]):
        assert b["parent"] is None or 0<=b["parent"]<i, (i,b["parent"])
    result={"format_version":3,"sample_hz":HZ,"source_time_unit":"ticks","source_ticks_per_ms":TICKS_PER_MS,
            "data_file":"player-animations.bin","pose_layout":"local_translation3_quaternion_xyzw4_scale3",
            "bone_count":count_bones,"floats_per_frame":count_bones*10,"clips":{},"actions":actions()}
    with (OUTPUT/result["data_file"]).open("wb") as out:
        for name,clip in source["clips"].items():
            duration_ticks=clip["duration_ms"]
            count=math.ceil(duration_ticks/TICKS_PER_MS*HZ/1000)+1
            result["clips"][name]={"source_clip":clip["source_clip"],"duration_ticks":duration_ticks,"frame_count":count,"byte_offset":out.tell()}
            for frame in range(count):
                time=frame/(count-1)*duration_ticks
                values=array("f")
                for bone,track in zip(source["bones"],clip["tracks"]):
                    assert track is not None, (name,bone["name"])
                    values.extend(sample(track["translation"],track["translations"],time))
                    values.extend(slerp(track["rotation"],sample(track["rotation"],track["rotations"],time,True),1.))
                    values.extend(sample(track["scale"],track["scales"],time))
                assert len(values)==result["floats_per_frame"]
                assert all(math.isfinite(v) for v in values)
                if __import__("sys").byteorder!="little":values.byteswap()
                values.tofile(out)
            print(name,count,"local bone frames",out.tell(),"bytes",flush=True)
        result["byte_length"]=out.tell()
    (OUTPUT/"player-animations.json").write_text(json.dumps(result,indent=2)+"\n")

if __name__=="__main__":bake()
