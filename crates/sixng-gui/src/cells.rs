//! Map cells for the 3D map: the placements beyond the near ring, baked per
//! area into a few merged batches.
//!
//! Drawing every placement on its own costs thousands of draw calls (each
//! model and material is one), which made the frame CPU-bound. A cell takes
//! the placements of one LOD level in one square of the map, puts their
//! geometry in place (cell-local coordinates) and merges it per visibility
//! class and pass. Its base-colour textures go, at 64 px, into BC3 texture
//! arrays, so one draw covers many materials. Near the camera the map keeps
//! drawing models one by one (sharp textures, exact LOD hand-off); beyond the
//! near ring it draws cells.
//!
//! A visibility class is a quantised [near, far) distance range: the static
//! version of the per-placement LOD rules, worked out when the cell index is
//! built. A batch is drawn while the camera's distance to its group lies in
//! that range (scaled by the detail setting).
//!
//! Built cells are cached in `<cache>/cells/<detail>/`, one file each, so a
//! revisited area loads with one read.

use crate::glb;
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Cell size (m) per LOD level: HD, LOD, SLOD1..4.
pub const CELL_SIZE: [f32; 6] = [128.0, 256.0, 512.0, 1024.0, 2048.0, 2048.0];

/// Class boundaries (m, at the default LOD scale); the last is "no limit".
pub const BOUNDS: [f32; 20] = [
    0.0, 16.0, 24.0, 36.0, 54.0, 81.0, 121.0, 182.0, 273.0, 410.0, 615.0, 922.0, 1384.0, 2076.0,
    3114.0, 4671.0, 7006.0, 10509.0, 15764.0, 1.0e9,
];

/// Texture array layer size (px) and bytes of one BC3 layer with its mips.
pub const LAYER_PX: u32 = 64;
pub const LAYER_MIPS: u32 = 7;
pub fn mip_bytes(l: u32) -> usize {
    let s = (LAYER_PX >> l).max(1);
    (s.div_ceil(4) * s.div_ceil(4) * 16) as usize
}

/// Bytes written under `<cache>/cells` (set at start, then counted up) and
/// the cap above which built cells are no longer saved.
pub static CELL_BYTES: AtomicU64 = AtomicU64::new(0);
pub const CELL_CAP: u64 = 4 << 30;

/// Passes a batch is drawn in.
pub const PASS_OPAQUE: u8 = 0;
pub const PASS_DECAL: u8 = 1;
pub const PASS_DECAL_BLEND: u8 = 2;
pub const PASS_BLEND: u8 = 3;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct CellKey {
    pub level: u8,
    pub cx: i32,
    pub cy: i32,
}

impl CellKey {
    /// Cell of a placement position (RAGE coordinates), on the map grid.
    pub fn of(level: u8, x: f32, y: f32) -> Self {
        let s = CELL_SIZE[level.min(5) as usize];
        CellKey {
            level: level.min(5),
            cx: ((x - crate::mapbake::X0) / s).floor() as i32,
            cy: ((crate::mapbake::Y1 - y) / s).floor() as i32,
        }
    }
    /// x0, y0, x1, y1 (RAGE coordinates).
    pub fn rect(&self) -> [f32; 4] {
        let s = CELL_SIZE[self.level as usize];
        let x0 = crate::mapbake::X0 + self.cx as f32 * s;
        let y1 = crate::mapbake::Y1 - self.cy as f32 * s;
        [x0, y1 - s, x0 + s, y1]
    }
    /// Horizontal distance from a point to the cell (0 inside).
    pub fn dist(&self, x: f32, y: f32) -> f32 {
        let r = self.rect();
        let dx = (r[0] - x).max(0.0).max(x - r[2]);
        let dy = (r[1] - y).max(0.0).max(y - r[3]);
        (dx * dx + dy * dy).sqrt()
    }
    pub fn file_name(&self) -> String {
        format!("{}_{}_{}.cell", self.level, self.cx, self.cy)
    }
}

/// Class index of a distance: the largest boundary at or below it.
pub fn class_of(v: f32) -> u8 {
    BOUNDS.iter().rposition(|&b| b <= v).unwrap_or(0) as u8
}

/// Cell vertex (32 bytes): position (cell-local, Y-up), UV (half floats),
/// normal with the flags in w (alpha mode | class << 2), base-colour
/// factor, tint, texture layer and placement index. A cell's placement
/// table holds each placement's centre (cell-local) and [near, far)
/// distance range (m at the default LOD scale); the vertex shader drops the
/// triangles of a placement whose centre is outside its range, so each
/// placement hands over to its LOD parent or children at its own distance.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CellV {
    pub pos: [f32; 3],
    pub uv: [u16; 2],
    pub nrm: [i8; 4],
    pub base: [u8; 4],
    pub tint: [u8; 4],
    pub layer: u16,
    pub ent: u16,
}

/// f32 -> IEEE half (round to nearest; tiny values flush to zero).
pub fn f16(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let man = b & 0x7f_ffff;
    if exp >= 31 {
        return sign | 0x7bff; // clamp to the largest finite half
    }
    if exp <= 0 {
        return sign;
    }
    let h = ((exp as u32) << 10) | (man >> 13);
    let h = h + ((man >> 12) & 1); // round
    sign | (h.min(0x7bff) as u16)
}

/// Placements of one visibility class: their [lo, hi) range (m, default
/// LOD scale) and bounds (Y-up, world).
#[derive(Clone, Copy)]
pub struct Group {
    pub lo: f32,
    pub hi: f32,
    pub bb: [[f32; 3]; 2],
}

#[derive(Clone, Copy)]
pub struct Batch {
    pub group: u16,
    pub chunk: u16,
    pub pass: u8,
    pub first: u32,
    pub count: u32,
}

/// One texture array: its layer count and BC3 data, mip-major (every
/// layer's level 0, then every layer's level 1, ...).
pub struct Chunk {
    pub layers: u32,
    pub data: Vec<u8>,
}

pub struct CellCpu {
    /// World position (Y-up) of the cell-local origin.
    pub origin: [f32; 3],
    pub groups: Vec<Group>,
    pub batches: Vec<Batch>,
    pub chunks: Vec<Chunk>,
    pub verts: Vec<CellV>,
    pub indices: Vec<u32>,
    /// Placement table: centre (cell-local), near, far, then padding.
    pub ents: Vec<[f32; 8]>,
}

impl CellCpu {
    pub fn bytes(&self) -> usize {
        self.verts.len() * std::mem::size_of::<CellV>()
            + self.indices.len() * 4
            + self.ents.len() * 32
            + self.chunks.iter().map(|c| c.data.len()).sum::<usize>()
    }
}

/// A placement going into a cell: model, position, yaw, bbox centre (all
/// RAGE coordinates) and drawn distance range (m, default LOD scale).
#[derive(Clone)]
pub struct CellEnt {
    pub model: String,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub ctr: [f32; 3],
    pub lo: f32,
    pub hi: f32,
    /// Simplification grid (m): about one pixel at the nearest distance
    /// the cell draws it; 0 keeps every vertex.
    pub grid: f32,
    /// Coverage fallback: also drawn from this distance to `lo` (nothing
    /// finer replaces it there), only on pixels no real surface covers.
    pub fb: Option<f32>,
}

fn gl(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[2], -p[1])
}

/// RGBA of an image's level 0 (decoded if block-compressed), resized to
/// the layer size (nearest texel).
fn layer_rgba(img: &glb::Image) -> Option<Vec<u8>> {
    let (w, h, px) = img.mips.first()?;
    let src = match &img.bc {
        Some(bc) => crate::bc::decode(bc, *w, *h),
        None if !px.is_empty() => px.clone(),
        None => return None,
    };
    let (w, h, n) = (*w as usize, *h as usize, LAYER_PX as usize);
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        let sy = (y * h / n).min(h - 1);
        for x in 0..n {
            let sx = (x * w / n).min(w - 1);
            out[(y * n + x) * 4..(y * n + x) * 4 + 4]
                .copy_from_slice(&src[(sy * w + sx) * 4..(sy * w + sx) * 4 + 4]);
        }
    }
    Some(out)
}

fn layer_bc3(rgba: Vec<u8>) -> Vec<Vec<u8>> {
    crate::bc::compress_bc3(&glb::mip_chain(LAYER_PX, LAYER_PX, rgba))
}

/// Drawable LOD file used for the placements of a cell level: HD
/// placements in a cell are at least the near ring away (the low LOD,
/// where a model has one); the other levels have one drawable.
pub fn drawable_lod(level: u8) -> usize {
    if level == 0 {
        2
    } else {
        0
    }
}

/// Vertex clustering: vertices in the same `grid` cube merge (mean
/// position and normal, first UV); triangles that collapse are dropped.
fn simplify(verts: &mut Vec<CellV>, idx: &mut Vec<u32>, grid: f32) {
    if grid <= 0.0 || verts.len() < 64 {
        return;
    }
    let inv = 1.0 / grid;
    let mut at: HashMap<(i32, i32, i32), u32> = HashMap::with_capacity(verts.len() / 2);
    let mut out: Vec<CellV> = Vec::with_capacity(verts.len() / 2);
    let mut acc: Vec<([f32; 3], [f32; 3], f32)> = Vec::with_capacity(verts.len() / 2);
    let mut remap = Vec::with_capacity(verts.len());
    for v in verts.iter() {
        let q = (
            (v.pos[0] * inv).floor() as i32,
            (v.pos[1] * inv).floor() as i32,
            (v.pos[2] * inv).floor() as i32,
        );
        let n = [v.nrm[0] as f32, v.nrm[1] as f32, v.nrm[2] as f32];
        let k = *at.entry(q).or_insert_with(|| {
            out.push(*v);
            acc.push(([0.0; 3], [0.0; 3], 0.0));
            (out.len() - 1) as u32
        });
        let a = &mut acc[k as usize];
        for c in 0..3 {
            a.0[c] += v.pos[c];
            a.1[c] += n[c];
        }
        a.2 += 1.0;
        remap.push(k);
    }
    for (v, a) in out.iter_mut().zip(&acc) {
        v.pos = [a.0[0] / a.2, a.0[1] / a.2, a.0[2] / a.2];
        let n = Vec3::from(a.1).normalize_or_zero();
        let s = |c: f32| (c.clamp(-1.0, 1.0) * 127.0).round() as i8;
        v.nrm = [s(n.x), s(n.y), s(n.z), v.nrm[3]];
    }
    let mut ni = Vec::with_capacity(idx.len() / 2);
    for t in idx.chunks_exact(3) {
        let (a, b, c) = (
            remap[t[0] as usize],
            remap[t[1] as usize],
            remap[t[2] as usize],
        );
        if a != b && b != c && a != c {
            ni.extend_from_slice(&[a, b, c]);
        }
    }
    *verts = out;
    *idx = ni;
}

/// Merge the placements of a cell. `load` gives a model (64 px textures)
/// at a drawable LOD, or None when it cannot be loaded; `max_layers` is the
/// device's array layer limit.
pub fn build(
    key: CellKey,
    ents: &[CellEnt],
    max_layers: u32,
    load: &mut dyn FnMut(&str, usize) -> Option<Arc<glb::Model>>,
) -> CellCpu {
    let r = key.rect();
    let origin = gl([(r[0] + r[2]) * 0.5, (r[1] + r[3]) * 0.5, 0.0]);
    let max_layers = max_layers.clamp(2, 2048) as usize;
    let mut groups: Vec<Group> = Vec::new();
    let mut group_of: HashMap<(u8, u8), usize> = HashMap::new();
    // texture layers: chunk -> per-layer BC3 mips; layer 0 of each chunk is white
    let white = layer_bc3(vec![255u8; (LAYER_PX * LAYER_PX * 4) as usize]);
    let mut chunks: Vec<Vec<Vec<Vec<u8>>>> = Vec::new();
    let mut tex_at: HashMap<String, (usize, u16)> = HashMap::new();
    // batch (group, chunk, pass) -> vertices and indices
    let mut parts: HashMap<(u16, u16, u8), (Vec<CellV>, Vec<u32>)> = HashMap::new();
    let mut models: HashMap<&str, Option<Arc<glb::Model>>> = HashMap::new();
    let mut table: Vec<[f32; 8]> = Vec::new();
    for e in ents {
        let Some(m) = models
            .entry(e.model.as_str())
            .or_insert_with(|| load(&e.model, drawable_lod(key.level)))
            .clone()
        else {
            continue;
        };
        if table.len() > u16::MAX as usize {
            break;
        }
        let ent = table.len() as u16;
        let c = gl(e.ctr) - origin;
        table.push([
            c.x,
            c.y,
            c.z,
            e.lo,
            e.hi,
            if e.fb.is_some() { 1.0 } else { 0.0 },
            e.fb.unwrap_or(0.0),
            0.0,
        ]);
        // group by quantised range (rounded outwards: the shader does the exact test)
        let gk = (
            class_of(e.fb.unwrap_or(e.lo)),
            (class_of(e.hi) + 1).min(BOUNDS.len() as u8 - 1),
        );
        let gi = *group_of.entry(gk).or_insert_with(|| {
            groups.push(Group {
                lo: BOUNDS[gk.0 as usize],
                hi: BOUNDS[gk.1 as usize],
                bb: [[f32::MAX; 3], [f32::MIN; 3]],
            });
            groups.len() - 1
        });
        let rot = Mat4::from_rotation_y(e.yaw);
        let xf = Mat4::from_translation(gl(e.pos) - origin) * rot;
        for me in &m.meshes {
            if me.skinned || me.indices.is_empty() {
                continue;
            }
            let mat = me.material.and_then(|k| m.materials.get(k));
            let (amode, cls, decal, factor, base_img) = match mat {
                Some(mt) => (
                    match mt.alpha_mode.as_str() {
                        "MASK" => 1u16,
                        "BLEND" => 2,
                        _ => 0,
                    },
                    crate::render3d::class_id(&mt.class) as u16,
                    mt.decal.is_some(),
                    mt.base_factor,
                    mt.base.and_then(|i| m.images.get(i)),
                ),
                None => (0, 1, false, [0.7, 0.7, 0.7, 1.0], None),
            };
            let blend = amode == 2 || matches!(cls, 5 | 6 | 8);
            let pass = match (decal, blend) {
                (true, false) => PASS_DECAL,
                (true, true) => PASS_DECAL_BLEND,
                (false, true) => PASS_BLEND,
                (false, false) => PASS_OPAQUE,
            };
            // texture layer (shared by every material using the image)
            let (chunk, layer) = match base_img {
                Some(img) => match tex_at.get(&img.key) {
                    Some(&v) => v,
                    None => {
                        let v = match layer_rgba(img) {
                            Some(rgba) => {
                                if chunks.last().is_none_or(|c| c.len() >= max_layers) {
                                    chunks.push(vec![white.clone()]);
                                }
                                let c = chunks.len() - 1;
                                chunks[c].push(layer_bc3(rgba));
                                (c, (chunks[c].len() - 1) as u16)
                            }
                            None => (usize::MAX, 0),
                        };
                        tex_at.insert(img.key.clone(), v);
                        v
                    }
                },
                None => (usize::MAX, 0),
            };
            // untextured: white layer of the current chunk
            let chunk = if chunk == usize::MAX {
                if chunks.is_empty() {
                    chunks.push(vec![white.clone()]);
                }
                chunks.len() - 1
            } else {
                chunk
            };
            let part = parts.entry((gi as u16, chunk as u16, pass)).or_default();
            let base = part.0.len() as u32;
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            let bf = [q(factor[0]), q(factor[1]), q(factor[2]), q(factor[3])];
            let g = &mut groups[gi];
            let mut mv: Vec<CellV> = Vec::with_capacity(me.verts.len());
            for v in &me.verts {
                let p = xf.transform_point3(Vec3::from(v.pos));
                let n = rot.transform_vector3(Vec3::from(v.nrm));
                let w = p + origin;
                for a in 0..3 {
                    g.bb[0][a] = g.bb[0][a].min(w[a]);
                    g.bb[1][a] = g.bb[1][a].max(w[a]);
                }
                let s = |c: f32| (c.clamp(-1.0, 1.0) * 127.0).round() as i8;
                mv.push(CellV {
                    pos: p.into(),
                    uv: [f16(v.uv0[0]), f16(v.uv0[1])],
                    nrm: [s(n.x), s(n.y), s(n.z), (amode | (cls << 2)) as i8],
                    base: bf,
                    tint: v.tint,
                    layer,
                    ent,
                });
            }
            let mut mi = me.indices.clone();
            simplify(&mut mv, &mut mi, e.grid);
            part.0.extend(mv);
            part.1.extend(mi.iter().map(|i| i + base));
        }
    }
    // batches in a stable order: by group, then pass, then chunk
    let mut keys: Vec<(u16, u16, u8)> = parts.keys().copied().collect();
    keys.sort_by_key(|k| (k.0, k.2, k.1));
    let mut verts = Vec::new();
    let mut indices = Vec::new();
    let mut batches = Vec::new();
    for k in keys {
        let (v, ix) = parts.remove(&k).unwrap();
        let vb = verts.len() as u32;
        let first = indices.len() as u32;
        verts.extend(v);
        indices.extend(ix.into_iter().map(|i| i + vb));
        batches.push(Batch {
            group: k.0,
            chunk: k.1,
            pass: k.2,
            first,
            count: indices.len() as u32 - first,
        });
    }
    let chunks = chunks
        .into_iter()
        .map(|layers| {
            let mut data =
                Vec::with_capacity(layers.len() * (0..LAYER_MIPS).map(mip_bytes).sum::<usize>());
            for l in 0..LAYER_MIPS as usize {
                for ly in &layers {
                    data.extend_from_slice(&ly[l]);
                }
            }
            Chunk {
                layers: layers.len() as u32,
                data,
            }
        })
        .collect();
    CellCpu {
        origin: origin.into(),
        groups,
        batches,
        chunks,
        verts,
        indices,
        ents: table,
    }
}

const MAGIC: &[u8; 4] = b"SXC3";

pub fn write(path: &Path, c: &CellCpu) -> std::io::Result<u64> {
    let mut b: Vec<u8> = Vec::with_capacity(c.bytes() + 1024);
    b.extend_from_slice(MAGIC);
    for v in [
        c.groups.len(),
        c.batches.len(),
        c.chunks.len(),
        c.verts.len(),
        c.indices.len(),
        c.ents.len(),
    ] {
        b.extend_from_slice(&(v as u32).to_le_bytes());
    }
    for f in c.origin {
        b.extend_from_slice(&f.to_le_bytes());
    }
    for g in &c.groups {
        for f in [
            g.lo, g.hi, g.bb[0][0], g.bb[0][1], g.bb[0][2], g.bb[1][0], g.bb[1][1], g.bb[1][2],
        ] {
            b.extend_from_slice(&f.to_le_bytes());
        }
    }
    for bt in &c.batches {
        b.extend_from_slice(&bt.group.to_le_bytes());
        b.extend_from_slice(&bt.chunk.to_le_bytes());
        b.extend_from_slice(&[bt.pass, 0, 0, 0]);
        b.extend_from_slice(&bt.first.to_le_bytes());
        b.extend_from_slice(&bt.count.to_le_bytes());
    }
    for ch in &c.chunks {
        b.extend_from_slice(&ch.layers.to_le_bytes());
        b.extend_from_slice(&(ch.data.len() as u32).to_le_bytes());
    }
    b.extend_from_slice(bytemuck::cast_slice(&c.verts));
    b.extend_from_slice(bytemuck::cast_slice(&c.indices));
    b.extend_from_slice(bytemuck::cast_slice(&c.ents));
    for ch in &c.chunks {
        b.extend_from_slice(&ch.data);
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::File::create(&tmp)?.write_all(&b)?;
    std::fs::rename(&tmp, path)?;
    CELL_BYTES.fetch_add(b.len() as u64, Ordering::Relaxed);
    Ok(b.len() as u64)
}

pub fn read(path: &Path) -> Option<CellCpu> {
    let b = std::fs::read(path).ok()?;
    if b.len() < 40 || &b[..4] != MAGIC {
        return None;
    }
    let mut o = 4usize;
    let u = |o: &mut usize| -> Option<u32> {
        let v = u32::from_le_bytes(b.get(*o..*o + 4)?.try_into().ok()?);
        *o += 4;
        Some(v)
    };
    let f = |o: &mut usize| -> Option<f32> {
        let v = f32::from_le_bytes(b.get(*o..*o + 4)?.try_into().ok()?);
        *o += 4;
        Some(v)
    };
    let (ng, nb, nc, nv, ni, ne) = (
        u(&mut o)? as usize,
        u(&mut o)? as usize,
        u(&mut o)? as usize,
        u(&mut o)? as usize,
        u(&mut o)? as usize,
        u(&mut o)? as usize,
    );
    let origin = [f(&mut o)?, f(&mut o)?, f(&mut o)?];
    let mut groups = Vec::with_capacity(ng);
    for _ in 0..ng {
        let v: Vec<f32> = (0..8).map(|_| f(&mut o)).collect::<Option<_>>()?;
        groups.push(Group {
            lo: v[0],
            hi: v[1],
            bb: [[v[2], v[3], v[4]], [v[5], v[6], v[7]]],
        });
    }
    let mut batches = Vec::with_capacity(nb);
    for _ in 0..nb {
        let gc = b.get(o..o + 8)?;
        let (group, chunk, pass) = (
            u16::from_le_bytes([gc[0], gc[1]]),
            u16::from_le_bytes([gc[2], gc[3]]),
            gc[4],
        );
        o += 8;
        batches.push(Batch {
            group,
            chunk,
            pass,
            first: u(&mut o)?,
            count: u(&mut o)?,
        });
    }
    let mut sizes = Vec::with_capacity(nc);
    for _ in 0..nc {
        sizes.push((u(&mut o)?, u(&mut o)? as usize));
    }
    let vbytes = nv * std::mem::size_of::<CellV>();
    let verts: Vec<CellV> = bytemuck::pod_collect_to_vec(b.get(o..o + vbytes)?);
    o += vbytes;
    let indices: Vec<u32> = bytemuck::pod_collect_to_vec(b.get(o..o + ni * 4)?);
    o += ni * 4;
    let ents: Vec<[f32; 8]> = bytemuck::pod_collect_to_vec(b.get(o..o + ne * 32)?);
    o += ne * 32;
    let mut chunks = Vec::with_capacity(nc);
    for (layers, len) in sizes {
        chunks.push(Chunk {
            layers,
            data: b.get(o..o + len)?.to_vec(),
        });
        o += len;
    }
    Some(CellCpu {
        origin,
        groups,
        batches,
        chunks,
        verts,
        indices,
        ents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_and_keys() {
        assert_eq!(class_of(0.0), 0);
        assert_eq!(class_of(20.0), 1);
        assert_eq!(class_of(5.0e9), 19);
        let k = CellKey::of(0, crate::mapbake::X0 + 130.0, crate::mapbake::Y1 - 10.0);
        assert_eq!((k.cx, k.cy), (1, 0));
        assert_eq!(
            k.dist(crate::mapbake::X0 + 200.0, crate::mapbake::Y1 - 50.0),
            0.0
        );
        assert!((0..LAYER_MIPS).map(mip_bytes).sum::<usize>() == 5488);
    }

    #[test]
    fn write_read_round_trip() {
        let img = glb::Image {
            key: "a".into(),
            mips: vec![(8, 8, vec![200; 256])],
            srgb: true,
            bc: None,
        };
        let mesh = glb::Mesh {
            name: String::new(),
            verts: vec![
                glb::Vertex {
                    pos: [0.0, 0.0, 0.0],
                    nrm: [0.0, 1.0, 0.0],
                    tan: [0.0; 4],
                    uv0: [0.0; 2],
                    uv1: [0.0; 2],
                    col: [255; 4],
                    joints: [0; 4],
                    weights: [0; 4],
                    tint: [255; 4]
                };
                3
            ],
            indices: vec![0, 1, 2],
            material: Some(0),
            skinned: false,
            has_tangents: false,
            uv_sets: 1,
            centre: [0.0; 3],
        };
        let mat = glb::Material {
            name: String::new(),
            class: "opaque".into(),
            alpha_mode: "OPAQUE".into(),
            base_factor: [1.0; 4],
            base: Some(0),
            normal: None,
            packed: None,
            emissive: None,
            mr: None,
            slots: vec![],
            gap: "ok".into(),
            shader_hash: String::new(),
            decal: None,
        };
        let model = Arc::new(glb::Model {
            path: Default::default(),
            meshes: vec![mesh],
            materials: vec![mat],
            joints: vec![],
            images: vec![img],
            bbox: [[0.0; 3]; 2],
            triangles: 1,
            vertices: 3,
            extras: serde_json::Value::Null,
        });
        let p = [crate::mapbake::X0 + 10.0, crate::mapbake::Y1 - 10.0, 5.0];
        let ents = vec![
            CellEnt {
                model: "m".into(),
                pos: p,
                yaw: 0.5,
                ctr: p,
                lo: 0.0,
                hi: 300.0,
                grid: 0.0,
                fb: None
            };
            2
        ];
        let key = CellKey::of(0, ents[0].pos[0], ents[0].pos[1]);
        let c = build(key, &ents, 256, &mut |_, _| Some(model.clone()));
        assert_eq!(c.verts.len(), 6);
        assert_eq!(c.batches.len(), 1);
        assert_eq!(c.chunks[0].layers, 2);
        let p = std::env::temp_dir().join(format!("sixng_cell_test_{}.cell", std::process::id()));
        write(&p, &c).unwrap();
        let r = read(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(r.indices, c.indices);
        assert_eq!(r.chunks[0].data, c.chunks[0].data);
        assert_eq!(r.groups[0].hi, BOUNDS[9]);
        assert_eq!((r.ents[0][3], r.ents[0][4]), (0.0, 300.0));
        assert_eq!(r.verts[3].ent, 1);
        assert_eq!(std::mem::size_of::<CellV>(), 32);
        assert_eq!(f16(1.0), 0x3c00);
        assert_eq!(f16(-2.5), 0xc100);
        assert_eq!(f16(0.0), 0);
    }

    #[test]
    fn simplify_merges_close_vertices() {
        let v = |x: f32| CellV {
            pos: [x, 0.0, (x * 7.0).fract()],
            uv: [0; 2],
            nrm: [0, 127, 0, 4],
            base: [255; 4],
            tint: [255; 4],
            layer: 0,
            ent: 0,
        };
        // a strip of 200 thin triangles 1 cm apart
        let mut verts: Vec<CellV> = (0..202).map(|i| v(i as f32 * 0.01)).collect();
        let mut idx: Vec<u32> = (0..200).flat_map(|i| [i, i + 1, i + 2]).collect();
        simplify(&mut verts, &mut idx, 0.5);
        assert!(verts.len() < 30, "{}", verts.len());
        assert!(idx.len() % 3 == 0 && idx.iter().all(|&i| (i as usize) < verts.len()));
    }
}
