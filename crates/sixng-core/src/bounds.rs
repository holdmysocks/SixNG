//! Physics bounds (collision), the GTA V `.ybn` / `phBound` equivalent.
//!
//! Unnamed resources in `levels.rpf` whose root is a bound. Layout (system
//! page, little-endian), close to GTA V's:
//!
//! ```text
//! common header (0x70)
//!   +0x10 u8  type: 5 = geometry (BVH), 6 = composite
//!   +0x14 f32 bounding-sphere radius
//!   +0x20 vec3 bbox max, +0x2C f32 margin (0.005)
//!   +0x30 vec3 bbox min, +0x3C u32 ref count (1)
//!   +0x40 vec3 bbox centre, +0x50 vec3 centre of gravity
//! composite (type 6)
//!   +0x70 ptr children (u64 pointers)
//!   +0x78 ptr child transforms (4x4 f32, 64 bytes each, column-major)
//!   +0x80 ptr second transform array (same values)
//!   +0x88 ptr child flags (16 bytes each)
//!   +0x98 u16 child count, u16 child count
//! geometry (type 5)
//!   +0x84 u32 vertex count (shrunk copy)
//!   +0x88 ptr polygons (16 bytes each)
//!   +0x90 vec3 quantum, +0xA0 vec3 centre: vertex = s16[3] * quantum + centre
//!   +0xB0 ptr vertices (s16 x3)
//!   +0xD0 u32 vertex count, u32 polygon count
//!   +0xF0 ptr materials (8 bytes each)
//!   +0x118 ptr per-polygon material index (u8)
//!   +0x120 u8 material count
//! polygon (16 bytes; type in byte 3)
//!   0 triangle: u16 v0, v1, v2 at +4 (top bit = flag), i16 neighbours at +10
//!   1 sphere:   u16 v at +0, f32 radius at +4
//!   2 capsule:  u16 v0 at +0, f32 radius at +4, u16 v1 at +8
//!   3 box:      u16 x4 at +4: four alternate corners of the box
//!   4 cylinder: as capsule
//! material (8 bytes)
//!   +0 u16 material id (15-bit hash of the surface-type name; the hash
//!      function is unknown), +3 u8 flags, +4..+8 flags
//! ```
use crate::resource::Resource;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MAX_DEPTH: u32 = 8;

#[derive(Debug, Default, Serialize)]
pub struct Stats {
    pub children: usize,
    pub triangles: usize,
    pub boxes: usize,
    pub spheres: usize,
    pub capsules: usize,
    pub cylinders: usize,
    pub other_polygons: usize,
    pub other_bound_types: BTreeMap<u8, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Mat {
    /// Material id (u16 at +0), hex.
    pub id: String,
    /// The whole 8-byte record, hex.
    pub raw: String,
    /// Triangles (after converting boxes/primitives) using it.
    pub triangles: usize,
}

/// Decoded collision of one bounds resource, in RAGE world space (Z-up).
#[derive(Default)]
pub struct Collision {
    /// Triangles per material id: (positions, indices).
    pub by_mat: BTreeMap<u16, (Vec<[f32; 3]>, Vec<u32>)>,
    /// Material records by id (first seen).
    pub mats: BTreeMap<u16, [u8; 8]>,
    /// Spheres, capsules and cylinders as stored (also meshed into `by_mat`).
    pub primitives: Vec<Value>,
    pub stats: Stats,
    pub bbox: [[f32; 3]; 2],
    /// Vertex dedupe per material (position bits -> index).
    lookup: std::collections::HashMap<u16, std::collections::HashMap<[u32; 3], u32>>,
}

fn f3(r: &Resource, o: usize) -> [f32; 3] {
    [r.f32(o), r.f32(o + 4), r.f32(o + 8)]
}

/// A wheel's collision disc inside a vehicle fragment.
#[derive(Debug, Clone, Serialize)]
pub struct WheelDisc {
    /// Centre in model space (RAGE Z-up): the wheel bone's pivot.
    pub centre: [f32; 3],
    /// Outer radius of the tyre.
    pub radius: f32,
    /// Half the tyre's width (the disc's margin).
    pub half_width: f32,
}

/// Wheel collision discs of a vehicle fragment. Its physics is an embedded
/// composite bound (type 6, same layout as a bounds resource but not at the
/// root); each wheel is a child of type 8, a disc on the X axis: +0x14
/// radius, +0x2C margin = half the width, bbox max = (margin, radius,
/// radius). The child's transform (translation at +48 of its 4x4) puts it
/// on the wheel bone's pivot. A second, thinner disc per wheel is the brake
/// disc. The composite is found by scanning the system page for its header.
pub fn wheel_discs(r: &Resource) -> Vec<WheelDisc> {
    let mut out: Vec<WheelDisc> = Vec::new();
    let n = r.sys.len();
    let mut o = 0;
    while o + 0xA0 <= n {
        let at = o;
        o += 16;
        if r.sys[at + 0x10] != 6 {
            continue;
        }
        let count = r.u16(at + 0x98) as usize;
        if !(1..=1024).contains(&count) || r.u16(at + 0x9A) as usize != count {
            continue;
        }
        let (Some(kp), Some(tp)) = (r.sys_off(r.u64(at + 0x70)), r.sys_off(r.u64(at + 0x78))) else { continue };
        if kp + 8 * count > n || tp + 64 * count > n {
            continue;
        }
        for i in 0..count {
            let Some(k) = r.sys_off(r.u64(kp + 8 * i)) else { continue };
            if k + 0x50 > n || r.sys[k + 0x10] != 8 {
                continue;
            }
            let (radius, margin) = (r.f32(k + 0x14), r.f32(k + 0x2C));
            let mx = f3(r, k + 0x20);
            let disc = radius > 0.03 && radius < 5.0 && margin > 0.0 && (mx[0] - margin).abs() < 2e-3 && (mx[1] - radius).abs() < 2e-3 && (mx[2] - radius).abs() < 2e-3;
            if !disc {
                continue;
            }
            let centre = f3(r, tp + 64 * i + 48);
            if centre.iter().all(|c| c.is_finite() && c.abs() < 100.0) {
                out.push(WheelDisc { centre, radius, half_width: margin });
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

/// True if the resource's root looks like a bound (composite or geometry).
pub fn is_bound(r: &Resource) -> bool {
    if r.sys.len() < 0x140 {
        return false;
    }
    let t = r.sys[0x10];
    let margin = r.f32(0x2C);
    (t == 5 || t == 6) && r.u32(0x3C) >= 1 && (0.0..0.5).contains(&margin) && r.sys_off(r.u64(if t == 6 { 0x70 } else { 0x88 })).is_some()
}

/// Bytes of system page the bound's geometry reaches (polygon, vertex,
/// material and material-index arrays of every child). Can exceed the
/// loaded system page when the vertices follow the last stream uncompressed.
pub fn sys_extent(r: &Resource) -> usize {
    fn go(r: &Resource, o: usize, depth: u32, best: &mut usize) {
        if depth > MAX_DEPTH || o + 0x140 > r.sys.len() {
            return;
        }
        let off = |p: u64| (p >> 28 == 5).then(|| (p & 0x0FFF_FFFF) as usize);
        match r.sys[o + 0x10] {
            6 => {
                let n = r.u16(o + 0x98) as usize;
                let Some(kp) = r.sys_off(r.u64(o + 0x70)) else { return };
                for i in 0..n.min(4096) {
                    if let Some(k) = r.sys_off(r.u64(kp + 8 * i)) {
                        go(r, k, depth + 1, best);
                    }
                }
            }
            5 => {
                let (nv, np) = (r.u32(o + 0xD0) as usize, r.u32(o + 0xD4) as usize);
                let nm = r.sys[o + 0x120] as usize;
                for (p, len) in [(r.u64(o + 0xB0), 6 * nv), (r.u64(o + 0x88), 16 * np), (r.u64(o + 0xF0), 8 * nm), (r.u64(o + 0x118), np)] {
                    if let Some(s) = off(p) {
                        *best = (*best).max(s + len);
                    }
                }
            }
            _ => {}
        }
    }
    let mut best = 0;
    if is_bound(r) {
        go(r, 0, 0, &mut best);
    }
    best
}

type M4 = [f32; 16];
const ID: M4 = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];

fn xf(m: &M4, p: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

fn mul(a: &M4, b: &M4) -> M4 {
    let mut o = [0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

pub fn decode(r: &Resource) -> Option<Collision> {
    if !is_bound(r) {
        return None;
    }
    let mut c = Collision { bbox: [f3(r, 0x30), f3(r, 0x20)], ..Default::default() };
    walk(r, 0, &ID, 0, &mut c);
    Some(c)
}

fn walk(r: &Resource, o: usize, m: &M4, depth: u32, c: &mut Collision) {
    if depth > MAX_DEPTH || o + 0x140 > r.sys.len() {
        return;
    }
    match r.sys[o + 0x10] {
        6 => {
            let n = r.u16(o + 0x98) as usize;
            let (Some(kp), tp) = (r.sys_off(r.u64(o + 0x70)), r.sys_off(r.u64(o + 0x78))) else { return };
            for i in 0..n.min(4096) {
                let Some(k) = r.sys_off(r.u64(kp + 8 * i)) else { continue };
                let local: M4 = match tp {
                    Some(t) if t + 64 * (i + 1) <= r.sys.len() => {
                        let mut x = [0f32; 16];
                        for (j, v) in x.iter_mut().enumerate() {
                            *v = r.f32(t + 64 * i + 4 * j);
                        }
                        // stored with w = 0 in the last row
                        x[3] = 0.0;
                        x[7] = 0.0;
                        x[11] = 0.0;
                        x[15] = 1.0;
                        x
                    }
                    _ => ID,
                };
                c.stats.children += 1;
                walk(r, k, &mul(m, &local), depth + 1, c);
            }
        }
        5 => geometry(r, o, m, c),
        t => *c.stats.other_bound_types.entry(t).or_default() += 1,
    }
}

fn geometry(r: &Resource, o: usize, m: &M4, c: &mut Collision) {
    let nv = r.u32(o + 0xD0) as usize;
    let np = r.u32(o + 0xD4) as usize;
    let q = f3(r, o + 0x90);
    let cen = f3(r, o + 0xA0);
    let (Some(vp), Some(pp)) = (r.sys_off(r.u64(o + 0xB0)), r.sys_off(r.u64(o + 0x88))) else { return };
    if vp + 6 * nv > r.sys.len() || pp + 16 * np > r.sys.len() {
        return;
    }
    let nm = r.sys[o + 0x120] as usize;
    let mats: Vec<[u8; 8]> = match r.sys_off(r.u64(o + 0xF0)) {
        Some(mp) if mp + 8 * nm <= r.sys.len() => (0..nm).map(|j| r.sys[mp + 8 * j..mp + 8 * j + 8].try_into().unwrap()).collect(),
        _ => Vec::new(),
    };
    let pmi = r.sys_off(r.u64(o + 0x118)).filter(|&p| p + np <= r.sys.len());
    let vert = |i: u16| -> [f32; 3] {
        let i = (i & 0x7FFF) as usize;
        if i >= nv {
            return cen;
        }
        let s = |k: usize| i16::from_le_bytes([r.sys[vp + 6 * i + 2 * k], r.sys[vp + 6 * i + 2 * k + 1]]) as f32;
        xf(m, [s(0) * q[0] + cen[0], s(1) * q[1] + cen[1], s(2) * q[2] + cen[2]])
    };
    for i in 0..np {
        let p = &r.sys[pp + 16 * i..pp + 16 * i + 16];
        let u = |k: usize| u16::from_le_bytes([p[k], p[k + 1]]);
        let mi = pmi.map(|x| r.sys[x + i] as usize).unwrap_or(0);
        let rec = mats.get(mi).or(mats.first()).copied().unwrap_or([0; 8]);
        let id = u16::from_le_bytes([rec[0], rec[1]]);
        c.mats.entry(id).or_insert(rec);
        let tris: Vec<[[f32; 3]; 3]> = match p[3] {
            0 => {
                c.stats.triangles += 1;
                vec![[vert(u(4)), vert(u(6)), vert(u(8))]]
            }
            3 => {
                c.stats.boxes += 1;
                box_tris([vert(u(4)), vert(u(6)), vert(u(8)), vert(u(10))])
            }
            t @ (1 | 2 | 4) => {
                let rad = f32::from_le_bytes(p[4..8].try_into().unwrap());
                let a = vert(u(0));
                let kind = match t {
                    1 => "sphere",
                    2 => "capsule",
                    _ => "cylinder",
                };
                match t {
                    1 => c.stats.spheres += 1,
                    2 => c.stats.capsules += 1,
                    _ => c.stats.cylinders += 1,
                }
                if t == 1 {
                    c.primitives.push(json!({"type": kind, "centre": a, "radius": rad, "material": format!("{id:04x}")}));
                    sphere_tris(a, rad)
                } else {
                    let b = vert(u(8));
                    c.primitives.push(json!({"type": kind, "a": a, "b": b, "radius": rad, "material": format!("{id:04x}")}));
                    prism_tris(a, b, rad, t == 2)
                }
            }
            _ => {
                c.stats.other_polygons += 1;
                vec![]
            }
        };
        let e = c.by_mat.entry(id).or_default();
        let lk = c.lookup.entry(id).or_default();
        for t in tris {
            for v in t {
                let key = v.map(f32::to_bits);
                let ix = *lk.entry(key).or_insert_with(|| {
                    e.0.push(v);
                    (e.0.len() - 1) as u32
                });
                e.1.push(ix);
            }
        }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// A box from four alternate corners v0..v3. Their mirror images through
/// the centre, w0..w3, are the other four corners; w_i is opposite v_i.
/// Each face holds two v's (one diagonal) and the two w's of the other
/// indices (the other diagonal), so the 6 faces are the 6 pairs (i, j).
fn box_tris(v: [[f32; 3]; 4]) -> Vec<[[f32; 3]; 3]> {
    let cen = [0, 1, 2].map(|k| (v[0][k] + v[1][k] + v[2][k] + v[3][k]) / 4.0);
    let w = v.map(|x| [2.0 * cen[0] - x[0], 2.0 * cen[1] - x[1], 2.0 * cen[2] - x[2]]);
    let mut out = Vec::new();
    for i in 0..4 {
        for j in i + 1..4 {
            let rest: Vec<usize> = (0..4).filter(|&x| x != i && x != j).collect();
            let (k, l) = (rest[0], rest[1]);
            for mut t in [[v[i], w[k], v[j]], [v[i], v[j], w[l]]] {
                let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
                if dot(n, sub(t[0], cen)) < 0.0 {
                    t.swap(1, 2);
                }
                out.push(t);
            }
        }
    }
    out
}

/// Low-poly sphere (octahedron).
fn sphere_tris(c: [f32; 3], r: f32) -> Vec<[[f32; 3]; 3]> {
    let p = |x: f32, y: f32, z: f32| [c[0] + x * r, c[1] + y * r, c[2] + z * r];
    let (px, nx, py, ny, pz, nz) = (p(1., 0., 0.), p(-1., 0., 0.), p(0., 1., 0.), p(0., -1., 0.), p(0., 0., 1.), p(0., 0., -1.));
    vec![[px, py, pz], [py, nx, pz], [nx, ny, pz], [ny, px, pz], [py, px, nz], [nx, py, nz], [ny, nx, nz], [px, ny, nz]]
}

/// 8-sided prism between a and b (capsules get pyramid caps).
fn prism_tris(a: [f32; 3], b: [f32; 3], r: f32, caps: bool) -> Vec<[[f32; 3]; 3]> {
    let ax = norm(sub(b, a));
    let helper = if ax[2].abs() < 0.9 { [0., 0., 1.] } else { [1., 0., 0.] };
    let u = norm(cross(ax, helper));
    let v = cross(ax, u);
    let ring = |c: [f32; 3]| -> Vec<[f32; 3]> {
        (0..8)
            .map(|i| {
                let t = i as f32 * std::f32::consts::TAU / 8.0;
                let (s, co) = t.sin_cos();
                [0, 1, 2].map(|k| c[k] + r * (co * u[k] + s * v[k]))
            })
            .collect()
    };
    let (ra, rb) = (ring(a), ring(b));
    let mut out = Vec::new();
    for i in 0..8 {
        let j = (i + 1) % 8;
        out.push([ra[i], ra[j], rb[j]]);
        out.push([ra[i], rb[j], rb[i]]);
    }
    let (ta, tb) = if caps { ([0, 1, 2].map(|k| a[k] - r * ax[k]), [0, 1, 2].map(|k| b[k] + r * ax[k])) } else { (a, b) };
    for i in 0..8 {
        let j = (i + 1) % 8;
        out.push([ta, ra[j], ra[i]]);
        out.push([tb, rb[i], rb[j]]);
    }
    out
}

impl Collision {
    pub fn materials(&self) -> Vec<Mat> {
        self.mats
            .iter()
            .map(|(id, raw)| Mat {
                id: format!("{id:04x}"),
                raw: raw.iter().map(|b| format!("{b:02x}")).collect(),
                triangles: self.by_mat.get(id).map(|x| x.1.len() / 3).unwrap_or(0),
            })
            .collect()
    }

    /// GLB: one primitive per material id (shared vertices, no normals:
    /// viewers flat-shade), a stable colour per id.
    pub fn glb(&self, name: &str, extras: Value) -> Vec<u8> {
        use crate::gltf::{mesh_glb, Material, NodeGroup, TriPrim};
        let mut mats = Vec::new();
        let mut prims = Vec::new();
        for (id, (pos, idx)) in &self.by_mat {
            if idx.is_empty() {
                continue;
            }
            let h = (*id as u32).wrapping_mul(2654435761);
            let col = [0, 8, 16].map(|s| ((h >> s) & 255) as f32 / 255.0 * 0.75 + 0.2);
            mats.push(Material {
                name: format!("surface_{id:04x}"),
                base_color_uri: None,
                normal_uri: None,
                gltf: json!({"pbrMetallicRoughness": {"baseColorFactor": [col[0], col[1], col[2], 1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0}}),
                extras: json!({"surface_id": format!("{id:04x}"), "record": self.mats.get(id).map(|r| r.iter().map(|b| format!("{b:02x}")).collect::<String>())}),
            });
            prims.push(TriPrim {
                name: format!("surface_{id:04x}"),
                positions: pos.clone(),
                normals: None,
                tangents: None,
                uvs: vec![],
                colors: None,
                tint: None,
                indices: idx.clone(),
                material: Some(mats.len() - 1),
                joints: None,
                weights: None,
                joints1: None,
                weights1: None,
            });
        }
        mesh_glb(name, &[NodeGroup { name: name.to_string(), prims }], &mats, &[], extras)
    }
}
