//! Navigation meshes (`navmeshes.rpf`), the GTA V `.ynv` (`CNavMesh`)
//! equivalent: one 128 x 128 m tile per resource.
//!
//! ```text
//! +0x10 u32 content flags, +0x14 u32 version 0x00010015
//! +0x20 4x4 transform (identity), +0x60 vec3 AABB size (128, 128, height)
//! +0x70 list: vertices (u16 x3, 6 bytes)
//! +0x80 list: indices (u16)
//! +0x88 list: edges (12 bytes)
//! +0x98 list: polygons (44 bytes)
//! +0xA0 ptr sector tree: vec3 min at +0, vec3 max at +0x10 (tile bbox)
//! +0xC0 u32 vertex count, u32 polygon count, u32 area id, u32 total bytes
//! list: +0x08 u32 item count, +0x10 ptr parts, +0x18 ptr part offsets,
//!       +0x20 u32 part count; part = { ptr, u32 count, u32 pad } (16 KiB pages)
//! polygon (44 bytes):
//!       +0x00 u32 area id, +0x04 u32 (first index << 4 | index count),
//!       +0x08 u32 ?, +0x0C..+0x18 cell AABB (u16 pairs), +0x18 u32 flags
//!       (bit 0x2000 = road surface, checked visually), +0x1C u32 ?
//! vertex position = sector min + u16 / 65535 * AABB size
//! ```
use crate::resource::Resource;
use std::collections::BTreeMap;

pub const ROAD: u32 = 0x2000;

pub struct NavMesh {
    pub bbox: [[f32; 3]; 2],
    pub area_id: u32,
    pub vertices: Vec<[f32; 3]>,
    /// (flags, vertex indices) per polygon.
    pub polys: Vec<(u32, Vec<u32>)>,
}

pub fn is_navmesh(r: &Resource) -> bool {
    r.sys.len() > 0x100 && r.u32(0x14) == 0x0001_0015 && r.sys_off(r.u64(0x70)).is_some() && r.sys_off(r.u64(0x98)).is_some()
}

/// Items of a paged list as byte slices of `size` bytes.
fn list<'a>(r: &'a Resource, at: usize, size: usize) -> Option<Vec<&'a [u8]>> {
    let l = r.sys_off(r.u64(at))?;
    let n = r.u32(l + 8) as usize;
    let parts = r.sys_off(r.u64(l + 0x10))?;
    let np = r.u32(l + 0x20) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..np.min(100_000) {
        let p = r.sys_off(r.u64(parts + 16 * i))?;
        let c = r.u32(parts + 16 * i + 8) as usize;
        let bytes = r.sys.get(p..p + c * size)?;
        out.extend(bytes.chunks_exact(size));
    }
    if out.len() != n {
        return None;
    }
    Some(out)
}

pub fn decode(r: &Resource) -> Option<NavMesh> {
    if !is_navmesh(r) {
        return None;
    }
    let st = r.sys_off(r.u64(0xA0))?;
    let mn = [r.f32(st), r.f32(st + 4), r.f32(st + 8)];
    let mx = [r.f32(st + 16), r.f32(st + 20), r.f32(st + 24)];
    let size = [r.f32(0x60), r.f32(0x64), r.f32(0x68)];
    let vertices = list(r, 0x70, 6)?
        .iter()
        .map(|v| {
            let u = |k: usize| u16::from_le_bytes([v[2 * k], v[2 * k + 1]]) as f32 / 65535.0;
            [mn[0] + u(0) * size[0], mn[1] + u(1) * size[1], mn[2] + u(2) * size[2]]
        })
        .collect::<Vec<_>>();
    let idx: Vec<u32> = list(r, 0x80, 2)?.iter().map(|x| u16::from_le_bytes([x[0], x[1]]) as u32).collect();
    let polys = list(r, 0x98, 44)?
        .iter()
        .filter_map(|p| {
            let w = u32::from_le_bytes(p[4..8].try_into().unwrap()) as usize;
            let (first, cnt) = (w >> 4, w & 15);
            let ids = idx.get(first..first + cnt)?.to_vec();
            if cnt < 3 || ids.iter().any(|&i| i as usize >= vertices.len()) {
                return None;
            }
            Some((u32::from_le_bytes(p[24..28].try_into().unwrap()), ids))
        })
        .collect();
    Some(NavMesh { bbox: [mn, mx], area_id: r.u32(0xC8), vertices, polys })
}

impl NavMesh {
    /// GLB with one primitive per polygon flag value (fans triangulated).
    /// With `roads_only`, only polygons with the road bit.
    pub fn glb(&self, name: &str, roads_only: bool, extras: serde_json::Value) -> Option<Vec<u8>> {
        use crate::gltf::{mesh_glb, Material, NodeGroup, TriPrim};
        let mut by: BTreeMap<u32, (Vec<[f32; 3]>, Vec<u32>, BTreeMap<u32, u32>)> = BTreeMap::new();
        for (fl, ids) in &self.polys {
            if roads_only && fl & ROAD == 0 {
                continue;
            }
            let e = by.entry(*fl).or_default();
            let local = |i: u32, e: &mut (Vec<[f32; 3]>, Vec<u32>, BTreeMap<u32, u32>)| -> u32 {
                *e.2.entry(i).or_insert_with(|| {
                    e.0.push(self.vertices[i as usize]);
                    (e.0.len() - 1) as u32
                })
            };
            for k in 1..ids.len() - 1 {
                let a = local(ids[0], e);
                let b = local(ids[k], e);
                let c = local(ids[k + 1], e);
                e.1.extend([a, b, c]);
            }
        }
        if by.is_empty() {
            return None;
        }
        let mut mats = Vec::new();
        let mut prims = Vec::new();
        for (fl, (pos, idx, _)) in by {
            let road = fl & ROAD != 0;
            let col = if road { [0.35, 0.35, 0.38] } else {
                let h = fl.wrapping_mul(2654435761);
                [0, 8, 16].map(|s| ((h >> s) & 255) as f32 / 255.0 * 0.6 + 0.3)
            };
            mats.push(Material {
                name: format!("nav_{fl:08x}{}", if road { "_road" } else { "" }),
                base_color_uri: None,
                normal_uri: None,
                gltf: serde_json::json!({"pbrMetallicRoughness": {"baseColorFactor": [col[0], col[1], col[2], 1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0}}),
                extras: serde_json::json!({"flags": format!("{fl:08x}"), "road": road}),
            });
            prims.push(TriPrim {
                name: format!("nav_{fl:08x}"),
                positions: pos,
                normals: None,
                tangents: None,
                uvs: vec![],
                colors: None, colors1: None, regions: None,
                tint: None,
                indices: idx,
                material: Some(mats.len() - 1),
                joints: None,
                weights: None,
                joints1: None,
                weights1: None,
            });
        }
        Some(mesh_glb(name, &[NodeGroup { name: name.to_string(), prims }], &mats, &[], extras))
    }

    pub fn flag_counts(&self) -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for (fl, _) in &self.polys {
            *m.entry(format!("{fl:08x}")).or_default() += 1;
        }
        m
    }
}
