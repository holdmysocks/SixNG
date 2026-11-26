"""Make Blender hair curves of a strand GLB written by `sixng outfit --hair`
(or `sixng grooms --full`).

    blender --python scripts/blender_hair_curves.py -- <hair.glb> [--model <model.glb>] [--save <file.blend>]

Each strand piece of the GLB becomes one hair curves object with these
attributes:

- `radius` (point): half the strand's width,
- `root_uv` (curve): the root UV the groom stores, a coordinate in one of
  the head mesh's UV sets (the GLB names it per piece: `head_uv_set`),
- `surface_uv_coordinate` (curve, with --model): the same root UV, with
  that UV set as the object's surface UV map, for Blender's hair tools
  (pieces whose set was not found, or where it overlaps itself under the
  roots, get the head's first UV map under each root instead),
- `clump` (curve, int): the strand's clump (1-based; 0 = not in a clump),
- `variant` (curve, int): the strand's colour variant (0-3).

Guide strands (`<piece>_guides`) come in hidden, in a collection of their
own, for restyling. With --model the model GLB is imported first. Each hair
object is then parented to the armature bone the groom names, and to the
head mesh as its surface (so Blender's hair tools and the "Deform Curves on
Surface" node can use it). Run it from Blender's Scripting tab as well:
set ARGS below.
"""

import json
import struct
import sys

import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree
from mathutils.geometry import barycentric_transform, intersect_point_tri_2d

# when run from the Scripting tab: ["<hair.glb>", "--model", "<model.glb>"]
ARGS = []


def read_glb(path):
    with open(path, "rb") as f:
        data = f.read()
    if data[:4] != b"glTF":
        raise SystemExit(f"{path}: not a GLB")
    jlen = struct.unpack_from("<I", data, 12)[0]
    doc = json.loads(data[20:20 + jlen])
    bin_at = 20 + jlen
    blen = struct.unpack_from("<I", data, bin_at)[0]
    return doc, data[bin_at + 8:bin_at + 8 + blen]


def accessor(doc, buf, i):
    a = doc["accessors"][i]
    v = doc["bufferViews"][a["bufferView"]]
    width = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[a["type"]]
    fmt = {5126: "f", 5125: "I"}[a["componentType"]]
    off = v.get("byteOffset", 0) + a.get("byteOffset", 0)
    n = a["count"] * width
    return struct.unpack_from(f"<{n}{fmt}", buf, off), width


def add_attr(curves, name, kind, domain, values):
    at = curves.attributes.get(name) or curves.attributes.new(name, kind, domain)
    field = {"FLOAT": "value", "INT": "value", "FLOAT2": "vector"}[kind]
    at.data.foreach_set(field, values)


def hair_material():
    m = bpy.data.materials.get("strand_hair")
    if m is None:
        m = bpy.data.materials.new("strand_hair")
        nt = m.node_tree
        nt.nodes.clear()
        out = nt.nodes.new("ShaderNodeOutputMaterial")
        bsdf = nt.nodes.new("ShaderNodeBsdfHairPrincipled")
        bsdf.parametrization = "MELANIN"
        # a neutral brown: the export carries no hair colour
        bsdf.inputs["Melanin Redness"].default_value = 0.3
        nt.links.new(bsdf.outputs[0], out.inputs[0])
    return m


def build(path):
    doc, buf = read_glb(path)
    made = []
    for mesh in doc["meshes"]:
        ex = mesh.get("extras", {})
        k = ex.get("points_per_strand")
        prim = mesh["primitives"][0]
        attrs = prim["attributes"]
        if not k or "POSITION" not in attrs:
            continue
        pos, _ = accessor(doc, buf, attrs["POSITION"])
        n = len(pos) // 3 // k
        if n == 0:
            continue
        cv = bpy.data.hair_curves.new(mesh["name"])
        cv.add_curves([k] * n)
        # glTF Y-up to Blender Z-up: (x, y, z) -> (x, -z, y)
        flat = [0.0] * (n * k * 3)
        for i in range(n * k):
            x, y, z = pos[3 * i:3 * i + 3]
            flat[3 * i:3 * i + 3] = (x, -z, y)
        cv.attributes["position"].data.foreach_set("vector", flat)
        if "_WIDTH" in attrs:
            w, _ = accessor(doc, buf, attrs["_WIDTH"])
            add_attr(cv, "radius", "FLOAT", "POINT", [x * 0.5 for x in w])
        if "TEXCOORD_0" in attrs:
            uv, _ = accessor(doc, buf, attrs["TEXCOORD_0"])
            # one per strand (its root point); glTF V runs down, Blender's up
            root = []
            for s in range(n):
                u, v = uv[2 * s * k], uv[2 * s * k + 1]
                root += (u, 1.0 - v)
            add_attr(cv, "root_uv", "FLOAT2", "CURVE", root)
        for key, name in (("_CLUMP", "clump"), ("_VARIANT", "variant")):
            if key in attrs:
                vals, _ = accessor(doc, buf, attrs[key])
                add_attr(cv, name, "INT", "CURVE", [int(vals[s * k]) for s in range(n)])
        ob = bpy.data.objects.new(mesh["name"], cv)
        for key in ("slot", "placeholder", "groom", "grown", "share"):
            if key in ex:
                ob[key] = ex[key] if not isinstance(ex[key], (list, dict)) else json.dumps(ex[key])
        cv.materials.append(hair_material())
        made.append((ob, ex))
    return made


def head_mesh():
    best = None
    for ob in bpy.data.objects:
        if ob.type == "MESH" and ("_head_" in ob.name or ob.name.startswith("head_")) and "eyes" not in ob.name:
            if best is None or len(ob.data.vertices) > len(best.data.vertices):
                best = ob
    return best


def surface_uvs(ob, head):
    """The head mesh's UV under each strand root (nearest point on it)."""
    me = head.data
    me.calc_loop_triangles()
    mw = head.matrix_world
    tris = me.loop_triangles
    bvh = BVHTree.FromPolygons([mw @ v.co for v in me.vertices], [t.vertices[:] for t in tris])
    uvl = me.uv_layers[0].data
    cv = ob.data
    pos = cv.attributes["position"].data
    out = []
    for c in cv.curves:
        loc, _, idx, _ = bvh.find_nearest(ob.matrix_world @ pos[c.first_point_index].vector)
        if idx is None:
            out += (0.0, 0.0)
            continue
        t = tris[idx]
        vs = [mw @ me.vertices[v].co for v in t.vertices]
        us = [Vector((uvl[lp].uv.x, uvl[lp].uv.y, 0.0)) for lp in t.loops]
        uv = barycentric_transform(loc, *vs, *us)
        out += (uv.x, uv.y)
    return out


def uv_misplacement(ob, head, layer, uv, samples=100):
    """Median distance (metres) from sample strand roots to the head point
    their UV names in `layer`; large where the UV set overlaps itself."""
    me = head.data
    me.calc_loop_triangles()
    uvl = me.uv_layers[layer].data
    mw = head.matrix_world
    tris = []
    for t in me.loop_triangles:
        q = [uvl[lp].uv.copy() for lp in t.loops]
        tris.append((q, t.vertices, min(u.x for u in q), max(u.x for u in q), min(u.y for u in q), max(u.y for u in q)))
    cv = ob.data
    pos = cv.attributes["position"].data
    d = []
    for i in range(0, len(cv.curves), max(1, len(cv.curves) // samples)):
        u = Vector((uv[2 * i], uv[2 * i + 1]))
        root = ob.matrix_world @ pos[cv.curves[i].first_point_index].vector
        for q, vs, x0, x1, y0, y1 in tris:
            if x0 <= u.x <= x1 and y0 <= u.y <= y1 and intersect_point_tri_2d(u, q[0], q[1], q[2]):
                at = barycentric_transform(u.to_3d(), *[w.to_3d() for w in q], *[mw @ me.vertices[v].co for v in vs])
                d.append((at - root).length)
                break
        else:
            d.append(float("inf"))
    d.sort()
    return d[len(d) // 2] if d else float("inf")


def attach(made):
    arm = next((o for o in bpy.data.objects if o.type == "ARMATURE"), None)
    for ob, ex in made:
        head = bpy.data.objects.get(ex.get("head_mesh", "")) or head_mesh()
        world = ob.matrix_world.copy()
        if arm is not None:
            # the groom's head bone where it names one, else its first bone the armature has
            named = [b for b in ex.get("bones", []) if b in arm.data.bones]
            bone = next((b for b in named if b.lower().endswith("_head")), named[0] if named else None)
            ob.parent = arm
            if bone:
                ob.parent_type = "BONE"
                ob.parent_bone = bone
            ob.matrix_world = world
        if head is not None and head.data.uv_layers:
            ob.data.surface = head
            layers = head.data.uv_layers
            n = ex.get("head_uv_set")
            uv = None
            if n is not None and n < len(layers) and "root_uv" in ob.data.attributes:
                # the game's own root UVs, in the head UV set they belong to
                # (glTF TEXCOORD_n comes in as the n-th UV map), unless that
                # set overlaps itself where the strands grow (some lashes):
                # then a UV does not name one place on the head
                uv = [0.0] * (2 * len(ob.data.curves))
                ob.data.attributes["root_uv"].data.foreach_get("vector", uv)
                if uv_misplacement(ob, head, layers[n].name, uv) > 0.005:
                    print(f"{ob.name}: its UV set overlaps itself under the roots; using the first UV map")
                    uv = None
            if uv is not None:
                ob.data.surface_uv_map = layers[n].name
                add_attr(ob.data, "surface_uv_coordinate", "FLOAT2", "CURVE", uv)
            else:
                ob.data.surface_uv_map = layers[0].name
                add_attr(ob.data, "surface_uv_coordinate", "FLOAT2", "CURVE", surface_uvs(ob, head))


def main(argv):
    if not argv:
        raise SystemExit("usage: blender --python blender_hair_curves.py -- <hair.glb> [--model <model.glb>] [--save <file.blend>]")
    hair, model, save = argv[0], None, None
    if "--model" in argv:
        model = argv[argv.index("--model") + 1]
    if "--save" in argv:
        save = argv[argv.index("--save") + 1]
    if model:
        bpy.ops.import_scene.gltf(filepath=model)
    made = build(hair)
    coll = bpy.data.collections.new("hair")
    guides = bpy.data.collections.new("hair guides")
    bpy.context.scene.collection.children.link(coll)
    bpy.context.scene.collection.children.link(guides)
    for ob, ex in made:
        (guides if ex.get("guides") else coll).objects.link(ob)
        ob.hide_render = ob.hide_viewport = bool(ex.get("guides"))
    attach(made)
    strands = sum(len(ob.data.curves) for ob, ex in made if not ex.get("guides"))
    print(f"hair curves: {len(made)} objects, {strands} strands")
    if save:
        bpy.ops.wm.save_as_mainfile(filepath=save)


if __name__ == "__main__":
    main(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else ARGS)
