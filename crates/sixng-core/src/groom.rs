//! Strand hair grooms: unnamed `packs.rpf` resources (hair, beards, brows,
//! lashes, stubble, animal fur) holding guide strands.
//!
//! System page: byte 0x1A = 3 (type), u32 0x1C = 0x7d8f3db3; vec4 0x30 /
//! 0x40 = root bounding box (min / max); vec4 0x50 / 0x60 = a second box
//! (shared by the LODs of one groom); ptr 0x70 = buffer descriptor (+0x18
//! graphics pointer); u32 0x98 = bone count, ptr 0xA0 = u32 bone joaat hashes.
//!
//! Graphics buffer: u32[8] section offsets A0-A7 at 0x00 and B0-B7 at 0x30
//! (from the buffer start); u16 S (strands) at 0x58, u16 T (root triangles)
//! at 0x5A, u32 K (points per strand) at 0x5C. B5 = S x K points of 4 x f16
//! (x, y, z, w), strand-major, model space, Z-up metres; B4 = T x 3 u16
//! strand indices (triangles between strand roots); B0 = S x f16 strand
//! length; A2 = S x 2 f16, the strand's root UV (inferred: in 0..1 and
//! following the root position). The point w is about 0.5 mm along the
//! strand and larger at the root (about 3 mm): a width (inferred). The
//! other sections (A0/A1/A3-A7, B1-B3, B6, B7, Q0-Q3, the tail weights) are
//! not decoded.
//!
//! Which groom belongs to which ped is not stored in any resource found so
//! far (the 3-vertex hair / beard / brows placeholder drawables carry only
//! their own name hash). Grooms come in LOD chains (consecutive resources
//! with the same bones and fewer strands each time).
use crate::resource::Resource;
use anyhow::{anyhow, bail, Result};

pub const TYPE_GROOM: u8 = 3;
pub const GROOM_TAG: u32 = 0x7d8f_3db3;

pub struct Groom {
    pub strands: usize,
    pub points_per_strand: usize,
    /// S x K points, strand-major (RAGE space, Z-up metres).
    pub points: Vec<[f32; 3]>,
    /// Per-point w (non-zero at the root; thickness or mass, unknown).
    pub w: Vec<f32>,
    /// Root triangles (strand indices).
    pub roots: Vec<[u32; 3]>,
    /// Stored strand lengths.
    pub lengths: Vec<f32>,
    /// Root UV per strand (A2; inferred), empty when absent.
    pub root_uv: Vec<[f32; 2]>,
    pub bones: Vec<u32>,
    pub root_box: [[f32; 3]; 2],
    /// Second box (None when empty); equal across one groom's LODs.
    pub group_box: Option<[[f32; 3]; 2]>,
    /// B6, one entry per root triangle (same order as `roots`): c0 = strands
    /// the game grows in the triangle, c1 = strands in clumps (inferred),
    /// c2 = clumps, c3 unknown. Empty when the section is absent. The sum
    /// of c0 equals system +0xD0 on most densest LODs; lower LODs saturate
    /// at 255.
    pub tri_counts: Vec<[u8; 4]>,
    /// System +0xF0 (f16): strand width in metres (0.2-0.5 mm; inferred),
    /// 0 when not stored.
    pub width: f32,
    /// System +0x110: 16 width steps from root to tip (/ 255; inferred).
    pub taper: [u8; 16],
}

/// Strands interpolated between the guides (`Groom::coat`).
pub struct Coat {
    /// Points per strand.
    pub k: usize,
    /// n x k points, strand-major (RAGE space).
    pub points: Vec<[f32; 3]>,
    /// Per strand: its three guide strands and their weights.
    pub guides: Vec<([u32; 3], [f32; 3])>,
    /// Per strand: unit normal of its root triangle.
    pub normals: Vec<[f32; 3]>,
    /// Strands the groom stores for the whole piece (sum of c0 + c1).
    pub stored: usize,
}

/// Cheap test on the system page alone.
pub fn is_groom(sys: &[u8]) -> bool {
    sys.len() >= 0xE0 && sys[0x1A] == TYPE_GROOM && sys.get(0x1C..0x20) == Some(&GROOM_TAG.to_le_bytes()[..])
}

pub fn f16(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((h >> 10) & 0x1F) as i32;
    let m = (h & 0x3FF) as f32;
    s * match e {
        0 => m / 1024.0 * 2f32.powi(-14),
        31 => f32::INFINITY,
        _ => (1.0 + m / 1024.0) * 2f32.powi(e - 15),
    }
}

fn v3(r: &Resource, o: usize) -> [f32; 3] {
    [r.f32(o), r.f32(o + 4), r.f32(o + 8)]
}

/// Decode a groom from a loaded resource (system page and graphics pages).
pub fn parse(r: &Resource) -> Result<Groom> {
    if !is_groom(&r.sys) {
        bail!("not a groom resource");
    }
    let desc = r.sys_off(r.u64(0x70)).ok_or_else(|| anyhow!("no buffer descriptor"))?;
    let base = r.gfx_off(r.u64(desc + 0x18)).ok_or_else(|| anyhow!("no graphics buffer"))?;
    let g = r.gfx.get(base..).ok_or_else(|| anyhow!("graphics buffer outside the pages"))?;
    let gu16 = |o: usize| g.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).unwrap_or(0);
    let gu32 = |o: usize| g.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0);
    let s = gu16(0x58) as usize;
    let t = gu16(0x5A) as usize;
    let k = gu32(0x5C) as usize;
    if s == 0 || k == 0 || k > 1024 {
        bail!("unexpected groom header (S {s}, K {k})");
    }
    let b = |i: usize| gu32(0x30 + 4 * i) as usize;
    let (b0, b4, b5) = (b(0), b(4), b(5));
    let pts = g.get(b5..b5 + s * k * 8).ok_or_else(|| anyhow!("point section outside the buffer"))?;
    let mut points = Vec::with_capacity(s * k);
    let mut w = Vec::with_capacity(s * k);
    for p in pts.chunks_exact(8) {
        let h = |i: usize| f16(u16::from_le_bytes([p[2 * i], p[2 * i + 1]]));
        points.push([h(0), h(1), h(2)]);
        w.push(h(3));
    }
    let mut roots = Vec::with_capacity(t);
    let mut tri_counts = Vec::new();
    if t > 0 && b4 > 0 {
        let tri = g.get(b4..b4 + t * 6).ok_or_else(|| anyhow!("root triangles outside the buffer"))?;
        let b6 = b(6);
        let counts = if b6 > 0 { g.get(b6..b6 + t * 4) } else { None };
        for (n, c) in tri.chunks_exact(6).enumerate() {
            let i = |j: usize| u16::from_le_bytes([c[2 * j], c[2 * j + 1]]) as u32;
            let v = [i(0), i(1), i(2)];
            if v.iter().all(|&x| (x as usize) < s) {
                roots.push(v);
                if let Some(q) = counts {
                    tri_counts.push([q[4 * n], q[4 * n + 1], q[4 * n + 2], q[4 * n + 3]]);
                }
            }
        }
    }
    let a2 = gu32(0x08) as usize;
    let root_uv = match g.get(a2..a2 + s * 4) {
        Some(u) if a2 > 0 => u.chunks_exact(4).map(|c| [f16(u16::from_le_bytes([c[0], c[1]])), f16(u16::from_le_bytes([c[2], c[3]]))]).collect(),
        _ => Vec::new(),
    };
    let lengths = match g.get(b0..b0 + s * 2) {
        Some(l) if b0 > 0 => l.chunks_exact(2).map(|c| f16(u16::from_le_bytes([c[0], c[1]]))).collect(),
        _ => Vec::new(),
    };
    let nb = r.u32(0x98) as usize;
    let bones = match r.sys_off(r.u64(0xA0)) {
        Some(o) if nb <= 4096 => (0..nb).map(|i| r.u32(o + 4 * i)).collect(),
        _ => Vec::new(),
    };
    let root_box = [v3(r, 0x30), v3(r, 0x40)];
    let gb = [v3(r, 0x50), v3(r, 0x60)];
    let group_box = (gb[0][0] <= gb[1][0] && gb[0][0].is_finite() && gb[1][0].is_finite() && gb[0][0].abs() < 1e30).then_some(gb);
    let width = Some(f16(r.u16(0xF0))).filter(|w| w.is_finite() && *w > 0.0 && *w < 0.01).unwrap_or(0.0);
    let taper: [u8; 16] = r.sys.get(0x110..0x120).and_then(|b| b.try_into().ok()).unwrap_or([255; 16]);
    Ok(Groom { strands: s, points_per_strand: k, points, w, roots, lengths, root_uv, bones, root_box, group_box, tri_counts, width, taper })
}

impl Groom {
    /// Line-segment indices (pairs) along every strand.
    pub fn segments(&self) -> Vec<u32> {
        let k = self.points_per_strand as u32;
        let mut v = Vec::with_capacity(self.strands * (k as usize).saturating_sub(1) * 2);
        for s in 0..self.strands as u32 {
            for j in 0..k.saturating_sub(1) {
                v.push(s * k + j);
                v.push(s * k + j + 1);
            }
        }
        v
    }
    /// Root triangles as point indices (the first point of each strand).
    pub fn root_indices(&self) -> Vec<u32> {
        let k = self.points_per_strand as u32;
        self.roots.iter().flat_map(|t| t.map(|i| i * k)).collect()
    }
    /// Root UV of each point's strand (zeros when absent).
    pub fn point_uvs(&self) -> Vec<[f32; 2]> {
        let k = self.points_per_strand;
        (0..self.strands * k).map(|i| self.root_uv.get(i / k).copied().unwrap_or([0.0, 0.0])).collect()
    }

    /// Display mesh: two crossed ribbons along every strand (RAGE space).
    /// `half_width` (metres) at the root, tapering to 40 % at the tip; the
    /// taper is a display choice, not from the data. Returns positions,
    /// normals, root UVs and triangle indices.
    pub fn ribbons(&self, half_width: f32) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>) {
        let k = self.points_per_strand;
        let n = self.strands * k * 4;
        let (mut pos, mut nrm, mut uv, mut idx) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::new());
        let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let norm = |a: [f32; 3]| {
            let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            if l > 1e-9 { [a[0] / l, a[1] / l, a[2] / l] } else { [0.0, 0.0, 1.0] }
        };
        for s in 0..self.strands {
            let uvs = self.root_uv.get(s).copied().unwrap_or([0.0, 0.0]);
            let base = pos.len() as u32;
            for j in 0..k {
                let p = self.points[s * k + j];
                let t = if k < 2 { [0.0, 0.0, 1.0] } else { norm(sub(self.points[s * k + (j + 1).min(k - 1)], self.points[s * k + j.saturating_sub(1)])) };
                let up = if t[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
                let a = norm(cross(t, up));
                let b = norm(cross(t, a));
                let r = half_width * (1.0 - 0.6 * j as f32 / (k.max(2) - 1) as f32);
                for (side, nv) in [(a, b), (b, a)] {
                    for sg in [-1.0f32, 1.0] {
                        pos.push([p[0] + side[0] * r * sg, p[1] + side[1] * r * sg, p[2] + side[2] * r * sg]);
                        nrm.push(nv);
                        uv.push(uvs);
                    }
                }
            }
            for j in 0..k.saturating_sub(1) as u32 {
                for q in 0..2u32 {
                    let v0 = base + j * 4 + q * 2;
                    let v1 = v0 + 4;
                    idx.extend_from_slice(&[v0, v0 + 1, v1 + 1, v0, v1 + 1, v1]);
                }
            }
        }
        (pos, nrm, uv, idx)
    }

    /// Ribbon GLB with one plain material of colour `rgb` (linear).
    pub fn ribbon_glb(&self, name: &str, half_width: f32, rgb: [f32; 3], extras: serde_json::Value) -> Vec<u8> {
        let (positions, normals, uv, indices) = self.ribbons(half_width);
        let prim = crate::gltf::TriPrim { name: name.to_string(), positions, normals: Some(normals), tangents: None, uvs: vec![uv], colors: None, tint: None, indices, material: Some(0), joints: None, weights: None, joints1: None, weights1: None };
        let mat = crate::gltf::Material {
            name: "hair".into(),
            base_color_uri: None,
            normal_uri: None,
            gltf: serde_json::json!({"doubleSided": true, "pbrMetallicRoughness": {"baseColorFactor": [rgb[0], rgb[1], rgb[2], 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.55}}),
            extras: serde_json::json!({"note": "display ribbons; colour chosen, not from the data"}),
        };
        crate::gltf::mesh_glb(name, &[crate::gltf::NodeGroup { name: name.to_string(), prims: vec![prim] }], &[mat], &[], extras)
    }

    /// Strands the groom stores for the piece: the sum of c0 + c1 over the
    /// root triangles (0 without B6).
    pub fn stored_strands(&self) -> usize {
        self.tri_counts.iter().map(|c| c[0] as usize + c[1] as usize).sum()
    }

    /// The coat the game grows from the guides, as far as it is decoded:
    /// in every root triangle, `share` of its stored strand count (c0 + c1)
    /// at random points, each strand blended from the triangle's three
    /// guides by its barycentric weights. The placement rule, clumping,
    /// curl and noise of the game's own interpolation are not decoded, so
    /// this coat is smoother and more even. Same strands for the same
    /// groom and share (fixed seed). Empty without B6.
    pub fn coat(&self, share: f32) -> Coat {
        let k = self.points_per_strand;
        let mut c = Coat { k, points: Vec::new(), guides: Vec::new(), normals: Vec::new(), stored: self.stored_strands() };
        // xorshift, seeded from the groom
        let mut state = 0x9E37_79B9_7F4A_7C15u64 ^ ((self.strands as u64) << 32) ^ self.roots.len() as u64;
        let mut rnd = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 40) as f32 / (1u64 << 24) as f32
        };
        let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        for (t, n) in self.roots.iter().zip(&self.tri_counts) {
            let want = (n[0] as f32 + n[1] as f32) * share.clamp(0.0, 1.0);
            // stochastic rounding keeps triangles with a small count
            let count = want.floor() as usize + (rnd() < want.fract()) as usize;
            if count == 0 {
                continue;
            }
            let g = t.map(|i| i as usize * k);
            let (p0, p1, p2) = (self.points[g[0]], self.points[g[1]], self.points[g[2]]);
            let (e1, e2) = (sub(p1, p0), sub(p2, p0));
            let nr = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            let l = (nr[0] * nr[0] + nr[1] * nr[1] + nr[2] * nr[2]).sqrt();
            let nr = if l > 1e-12 { [nr[0] / l, nr[1] / l, nr[2] / l] } else { [0.0, 0.0, 1.0] };
            for _ in 0..count {
                let (mut u, mut v) = (rnd(), rnd());
                if u + v > 1.0 {
                    u = 1.0 - u;
                    v = 1.0 - v;
                }
                let w = [1.0 - u - v, u, v];
                for j in 0..k {
                    let (a, b, d) = (self.points[g[0] + j], self.points[g[1] + j], self.points[g[2] + j]);
                    c.points.push([a[0] * w[0] + b[0] * w[1] + d[0] * w[2], a[1] * w[0] + b[1] * w[1] + d[1] * w[2], a[2] * w[0] + b[2] * w[1] + d[2] * w[2]]);
                }
                c.guides.push((*t, w));
                c.normals.push(nr);
            }
        }
        c
    }

    /// GLB of a coat: one flat ribbon per strand, lying in the skin's
    /// tangent plane, `width` metres wide at the root and tapered by the
    /// stored 16 steps. `guide_rgb` (linear, one per guide strand) colours
    /// each strand by its barycentric weights (COLOR_0 on a white
    /// material); strands are white where it is short.
    pub fn coat_glb(&self, name: &str, coat: &Coat, width: f32, guide_rgb: &[[u8; 3]], extras: serde_json::Value) -> Vec<u8> {
        let k = coat.k;
        let n = coat.guides.len();
        let (mut pos, mut nrm, mut tint, mut idx) = (Vec::with_capacity(n * k * 2), Vec::with_capacity(n * k * 2), Vec::with_capacity(n * k * 2), Vec::with_capacity(n * k.saturating_sub(1) * 6));
        let flat_taper = self.taper.iter().all(|&b| b == 0);
        for s in 0..n {
            let (g, w) = coat.guides[s];
            let mut c = [255u8; 4];
            if let (Some(a), Some(b), Some(d)) = (guide_rgb.get(g[0] as usize), guide_rgb.get(g[1] as usize), guide_rgb.get(g[2] as usize)) {
                for q in 0..3 {
                    c[q] = (a[q] as f32 * w[0] + b[q] as f32 * w[1] + d[q] as f32 * w[2] + 0.5) as u8;
                }
            }
            let nr = coat.normals[s];
            let base = pos.len() as u32;
            for j in 0..k {
                let p = coat.points[s * k + j];
                let (a, b) = (coat.points[s * k + j.saturating_sub(1)], coat.points[s * k + (j + 1).min(k - 1)]);
                let t = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                // across the strand, in the skin's plane
                let mut side = [t[1] * nr[2] - t[2] * nr[1], t[2] * nr[0] - t[0] * nr[2], t[0] * nr[1] - t[1] * nr[0]];
                let l = (side[0] * side[0] + side[1] * side[1] + side[2] * side[2]).sqrt();
                side = if l > 1e-12 { [side[0] / l, side[1] / l, side[2] / l] } else { [1.0, 0.0, 0.0] };
                let f = j as f32 / (k.max(2) - 1) as f32;
                let tap = if flat_taper { 1.0 - 0.6 * f } else { (self.taper[((f * 15.0) as usize).min(15)] as f32 / 255.0).max(0.15) };
                let r = width * 0.5 * tap;
                for sg in [-1.0f32, 1.0] {
                    pos.push([p[0] + side[0] * r * sg, p[1] + side[1] * r * sg, p[2] + side[2] * r * sg]);
                    nrm.push(nr);
                    tint.push(c);
                }
            }
            for j in 0..k.saturating_sub(1) as u32 {
                let v0 = base + j * 2;
                idx.extend_from_slice(&[v0, v0 + 1, v0 + 3, v0, v0 + 3, v0 + 2]);
            }
        }
        let prim = crate::gltf::TriPrim { name: name.to_string(), positions: pos, normals: Some(nrm), tangents: None, uvs: vec![], colors: None, tint: Some(tint), indices: idx, material: Some(0), joints: None, weights: None, joints1: None, weights1: None };
        let mat = crate::gltf::Material {
            name: "fur".into(),
            base_color_uri: None,
            normal_uri: None,
            gltf: serde_json::json!({"doubleSided": true, "pbrMetallicRoughness": {"baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.85}}),
            extras: serde_json::json!({"note": "interpolated coat: the stored per-triangle strand counts, strands blended from the three guides; placement, clumping and curl are not the game's. COLOR_0 = skin colour under the root (a display choice)"}),
        };
        crate::gltf::mesh_glb(name, &[crate::gltf::NodeGroup { name: name.to_string(), prims: vec![prim] }], &[mat], &[], extras)
    }

    /// Ribbon GLB with one colour per strand (`strand_rgb`, linear RGB,
    /// written as COLOR_0 on a white material): fur coloured from the skin
    /// under each root. Strands past the list are white.
    pub fn ribbon_glb_coloured(&self, name: &str, half_width: f32, strand_rgb: &[[u8; 3]], extras: serde_json::Value) -> Vec<u8> {
        let (positions, normals, uv, indices) = self.ribbons(half_width);
        let per = self.points_per_strand * 4;
        let tint: Vec<[u8; 4]> = (0..positions.len()).map(|i| strand_rgb.get(i / per.max(1)).map_or([255; 4], |c| [c[0], c[1], c[2], 255])).collect();
        let prim = crate::gltf::TriPrim { name: name.to_string(), positions, normals: Some(normals), tangents: None, uvs: vec![uv], colors: None, tint: Some(tint), indices, material: Some(0), joints: None, weights: None, joints1: None, weights1: None };
        let mat = crate::gltf::Material {
            name: "fur".into(),
            base_color_uri: None,
            normal_uri: None,
            gltf: serde_json::json!({"doubleSided": true, "pbrMetallicRoughness": {"baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.7}}),
            extras: serde_json::json!({"note": "display ribbons; COLOR_0 = the skin colour under each strand's root (a display choice, not from the groom)"}),
        };
        crate::gltf::mesh_glb(name, &[crate::gltf::NodeGroup { name: name.to_string(), prims: vec![prim] }], &[mat], &[], extras)
    }

    /// Share of strand roots inside the stored root box (a layout check).
    pub fn roots_in_box(&self) -> f32 {
        let k = self.points_per_strand;
        let eps = 1e-3;
        let n = (0..self.strands)
            .filter(|&s| {
                let p = self.points[s * k];
                (0..3).all(|a| p[a] >= self.root_box[0][a] - eps && p[a] <= self.root_box[1][a] + eps)
            })
            .count();
        n as f32 / self.strands.max(1) as f32
    }
    /// Strands with no segment longer than 4 x the strand's median segment.
    pub fn continuous(&self) -> f32 {
        let k = self.points_per_strand;
        if k < 3 {
            return 1.0;
        }
        let d = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        let n = (0..self.strands)
            .filter(|&s| {
                let mut seg: Vec<f32> = (0..k - 1).map(|j| d(self.points[s * k + j], self.points[s * k + j + 1])).collect();
                let mx = seg.iter().cloned().fold(0.0, f32::max);
                seg.sort_by(|a, b| a.partial_cmp(b).unwrap());
                mx <= 4.0 * seg[seg.len() / 2] + 1e-4
            })
            .count();
        n as f32 / self.strands.max(1) as f32
    }
}

/// Bone names (joaat of the lower-case name -> name) from the skeletons
/// of the archive's named fragments (people, animals) and a sample of head
/// drawables (facial bones).
pub fn bone_names(a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> std::collections::HashMap<u32, String> {
    use rayon::prelude::*;
    let mut heads = 0;
    let src: Vec<&crate::scan::Entry> = all
        .iter()
        .filter(|e| match (e.rtype.as_deref(), e.name.as_deref()) {
            (Some("ft"), Some(_)) => true,
            (Some("dr"), Some(n)) if n.starts_with("head_") && heads < 64 => {
                heads += 1;
                true
            }
            _ => false,
        })
        .collect();
    let lists: Vec<Vec<String>> = src
        .par_iter()
        .filter_map(|e| Resource::load_sys(&a.path, e).ok())
        .map(|r| crate::drawable::find_skeleton(&r).into_iter().map(|b| b.name).collect())
        .collect();
    let mut m = std::collections::HashMap::new();
    for n in lists.into_iter().flatten() {
        m.entry(crate::meta7a::joaat(&n)).or_insert(n);
    }
    m
}

/// One groom as listed by the catalog (system page only).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GroomInfo {
    pub off: u64,
    pub id: String,
    pub strands: u32,
    pub root_triangles: u32,
    pub bones: Vec<String>,
    pub root_box: [[f32; 3]; 2],
    /// scalp / face / brows / lashes / body / animal / other, from the bones (a guess).
    pub category: String,
    /// LOD chain: consecutive grooms with the same bones and fewer strands
    /// each time (inferred); `lod` 0 is the densest.
    pub chain: u32,
    pub lod: u32,
}

/// Body region from the bone names (a guess for browsing).
/// Body region of a groom from its bones; head-only grooms whose roots all
/// sit low on the front of the head (beards, moustaches) count as face.
pub fn category(bones: &[String], root_box: &[[f32; 3]; 2]) -> &'static str {
    let c = category_by_bones(bones);
    if c == "scalp" && root_box[1][2] < 0.74 && root_box[1][1] < 0.0 {
        "face"
    } else {
        c
    }
}

fn category_by_bones(bones: &[String]) -> &'static str {
    let has = |f: &dyn Fn(&str) -> bool| bones.iter().any(|b| f(b));
    if has(&|b| b.starts_with("SKEL_Tail") || b.starts_with("MH_Tail") || b.contains("_Fin") || b.contains("Paw") || b.contains("Wing")) {
        "animal"
    } else if has(&|b| b.contains("Brow") || b.contains("Lid") || b.contains("Lash")) {
        "brows / lashes"
    } else if has(&|b| b.contains("Ear") || b.contains("Cheek") || b.contains("Throat") || b.contains("Chin") || b.contains("Jaw") || b.contains("Lip")) {
        "face"
    } else if has(&|b| b == "BM_Head" || b == "SKEL_Head" || b == "SKEL_HEAD" || b.contains("Occipitalis")) {
        "scalp"
    } else if has(&|b| b.contains("Pectoralis") || b.contains("Scapula") || b.contains("Spine") || b.contains("Pelvis") || b.contains("Thigh") || b.contains("Arm")) {
        "body"
    } else {
        "other"
    }
}

/// List every groom among the archive's unnamed resources from their
/// system pages (no graphics pages read), with LOD chains.
pub fn catalog(a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> Result<Vec<GroomInfo>> {
    use rayon::prelude::*;
    let names = bone_names(a, all);
    let bone_name = |h: u32| names.get(&h).cloned().or_else(|| crate::names::lookup(h, crate::names::Ctx::Field)).unwrap_or_else(|| format!("{h:08x}"));
    let cands: Vec<&crate::scan::Entry> = all.iter().filter(|e| e.name.is_none() && e.kind == crate::classify::Kind::RageObj).collect();
    let mut v: Vec<GroomInfo> = cands
        .par_iter()
        .filter_map(|e| {
            let r = Resource::load_sys(&a.path, e).ok()?;
            if !is_groom(&r.sys) {
                return None;
            }
            let nb = r.u32(0x98) as usize;
            let bones = match r.sys_off(r.u64(0xA0)) {
                Some(o) if nb <= 4096 => (0..nb).map(|i| bone_name(r.u32(o + 4 * i))).collect(),
                _ => Vec::new(),
            };
            let root_box = [v3(&r, 0x30), v3(&r, 0x40)];
            let cat = category(&bones, &root_box).to_string();
            Some(GroomInfo { off: e.off, id: e.id.clone(), strands: r.u16(0xDE) as u32, root_triangles: r.u16(0xDC) as u32, bones, root_box, category: cat, chain: 0, lod: 0 })
        })
        .collect();
    v.sort_by_key(|g| g.off);
    let mut chain = 0;
    for i in 0..v.len() {
        let cont = i > 0 && v[i].bones == v[i - 1].bones && v[i].strands < v[i - 1].strands;
        if cont {
            v[i].lod = v[i - 1].lod + 1;
        } else {
            chain += 1;
        }
        v[i].chain = chain;
    }
    Ok(v)
}

/// The catalog, cached in `<work>/index/grooms_<archive>.json`.
pub fn catalog_cached(work: &std::path::Path, a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> Result<Vec<GroomInfo>> {
    let p = work.join("index").join(format!("grooms_{}.json", a.id.trim_end_matches(".rpf").replace(['/', '\\'], "_")));
    if let Ok(b) = std::fs::read(&p) {
        if let Ok(mut v) = serde_json::from_slice::<Vec<GroomInfo>>(&b) {
            // regions may be refined after the cache was written
            for g in &mut v {
                g.category = category(&g.bones, &g.root_box).to_string();
            }
            return Ok(v);
        }
    }
    let v = catalog(a, all)?;
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(&p, serde_json::to_vec(&v)?)?;
    Ok(v)
}

/// Load and decode one groom by archive offset.
pub fn load(a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry], off: u64) -> Result<Groom> {
    let e = all.iter().find(|e| e.off == off).ok_or_else(|| anyhow!("no entry at {off:x}"))?;
    parse(&Resource::load(&a.path, e)?)
}

/// Export grooms to `<out>/grooms/<archive>/<offset>.glb` plus
/// `index.jsonl`: line strips (with `_WIDTH` and the root UV as
/// TEXCOORD_0), or crossed ribbons of `ribbons` half-width (metres) when
/// given. `only` limits the export to these offsets (all grooms when
/// empty). Returns (grooms written, errors).
pub fn export_grooms(a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry], only: &[u64], out: &std::path::Path, roots: bool, ribbons: Option<f32>, progress: crate::export::Progress) -> Result<(usize, usize)> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    let cat = catalog(a, all)?;
    let sel: Vec<&GroomInfo> = cat.iter().filter(|g| only.is_empty() || only.contains(&g.off)).collect();
    let by_off: std::collections::HashMap<u64, &crate::scan::Entry> = all.iter().map(|e| (e.off, e)).collect();
    let dir = out.join("grooms").join(a.id.trim_end_matches(".rpf"));
    std::fs::create_dir_all(&dir)?;
    let total = sel.len() as u64;
    let done = AtomicU64::new(0);
    let errors = AtomicUsize::new(0);
    let mut rows: Vec<(u64, String, bool)> = sel
        .par_iter()
        .filter_map(|info| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&info.id, d, total);
            let e = by_off.get(&info.off)?;
            let g = match Resource::load(&a.path, e).map_err(|x| x.to_string()).and_then(|r| parse(&r).map_err(|x| x.to_string())) {
                Ok(g) => g,
                Err(msg) => {
                    errors.fetch_add(1, Ordering::Relaxed);
                    return Some((e.off, serde_json::json!({"id": info.id, "error": msg}).to_string(), false));
                }
            };
            let name = format!("groom_{:x}", e.off);
            let extras = serde_json::json!({"source": e.id, "strands": g.strands, "points_per_strand": g.points_per_strand, "bones": info.bones, "category": info.category, "chain": info.chain, "lod": info.lod,
                "coordinate_system": "model space (Z-up metres) converted to glTF Y-up (x, z, -y)"});
            let bytes = match ribbons {
                Some(hw) => g.ribbon_glb(&name, hw, [0.08, 0.05, 0.03], extras),
                None => {
                    let tris = if roots { g.root_indices() } else { Vec::new() };
                    crate::gltf::lines_glb(&name, &g.points, &g.w, &g.point_uvs(), &g.segments(), &tris, extras)
                }
            };
            let file = dir.join(format!("{:x}.glb", e.off));
            if std::fs::write(&file, bytes).is_err() {
                errors.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            let len: f32 = g.lengths.iter().sum();
            Some((e.off, serde_json::json!({"id": e.id, "file": format!("{:x}.glb", e.off), "strands": g.strands, "points_per_strand": g.points_per_strand,
                "root_triangles": g.roots.len(), "total_length_m": (len * 1000.0).round() / 1000.0, "bones": info.bones, "category": info.category, "chain": info.chain, "lod": info.lod,
                "root_box": g.root_box, "roots_in_box": g.roots_in_box(), "continuous": g.continuous()}).to_string(), true))
        })
        .collect();
    rows.sort_by_key(|r| r.0);
    let n = rows.iter().filter(|r| r.2).count();
    let mut text = String::new();
    for r in &rows {
        text.push_str(&r.1);
        text.push('\n');
    }
    std::fs::write(dir.join("index.jsonl"), text)?;
    Ok((n, errors.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn half() {
        assert_eq!(f16(0x3C00), 1.0);
        assert_eq!(f16(0xC000), -2.0);
        assert_eq!(f16(0x3555) - 0.33325195 < 1e-6, true);
    }
    #[test]
    fn segments() {
        let g = Groom { strands: 2, points_per_strand: 3, points: vec![[0.0; 3]; 6], w: vec![0.0; 6], roots: vec![[0, 1, 1]], lengths: vec![], root_uv: vec![], bones: vec![], root_box: [[0.0; 3]; 2], group_box: None, tri_counts: vec![], width: 0.0, taper: [255; 16] };
        assert_eq!(g.segments(), vec![0, 1, 1, 2, 3, 4, 4, 5]);
        assert_eq!(g.root_indices(), vec![0, 3, 3]);
    }
    #[test]
    fn coat_from_strand_counts() {
        // one root triangle of three 2-point guides, 40 + 10 stored strands
        let pts = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.1], [1.0, 0.0, 0.0], [1.0, 0.0, 0.1], [0.0, 1.0, 0.0], [0.0, 1.0, 0.1]];
        let g = Groom { strands: 3, points_per_strand: 2, points: pts, w: vec![0.0; 6], roots: vec![[0, 1, 2]], lengths: vec![], root_uv: vec![], bones: vec![], root_box: [[0.0; 3]; 2], group_box: None, tri_counts: vec![[40, 10, 1, 0]], width: 0.0003, taper: [255; 16] };
        assert_eq!(g.stored_strands(), 50);
        let c = g.coat(1.0);
        assert_eq!((c.guides.len(), c.points.len(), c.stored), (50, 100, 50));
        // every root lies inside the triangle and every strand keeps the guides' shape
        for s in 0..50 {
            let (r, tip) = (c.points[2 * s], c.points[2 * s + 1]);
            assert!(r[0] >= 0.0 && r[1] >= 0.0 && r[0] + r[1] <= 1.0 + 1e-5 && r[2].abs() < 1e-6);
            assert!((tip[2] - 0.1).abs() < 1e-6 && (tip[0] - r[0]).abs() < 1e-6);
            assert_eq!(c.normals[s], [0.0, 0.0, 1.0]);
        }
        // same strands again; a quarter of them at share 0.25
        assert_eq!(g.coat(1.0).points, c.points);
        let q = g.coat(0.25).guides.len();
        assert!((12..=13).contains(&q), "{q}");
        // 50 strands x 2 points x 2 vertices; coloured from the guides
        let glb = g.coat_glb("t", &c, 0.001, &[[255, 0, 0], [255, 0, 0], [255, 0, 0]], serde_json::json!({}));
        assert_eq!(&glb[0..4], b"glTF");
    }

}
