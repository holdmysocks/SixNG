//! Instanced batches in map files (GTA V `fwInstancedMapData` equivalent):
//! the map data struct `d3593fa6` has, at +0x140, struct `f14a15be` with two
//! arrays:
//!
//! ```text
//! +0x08 ba049770 (368 B) prop batch   (GTA V PropInstanceList)
//! +0x18 7c475b5d (416 B) grass batch  (GTA V GrassInstanceList)
//! ```
//! Both start with the entity prefix (archetype joaat at +0x44, batch
//! centre at +0x38, zero rotation and scale). Their instance arrays live in
//! the resource's graphics page, which is stored uncompressed after the
//! zlib stream: load with `Resource::load_meta`.
//!
//! Prop batch: +0x60 f32 LOD distance, +0x80 u16 array, +0x90 instances
//! `ea6b3a96` (64 B): +0x00 3 x vec4 rows of a 3x4 matrix (rotation and
//! scale in xyz, world translation in w), +0x30 f32, +0x34 u32 tint
//! (RGBA bytes), +0x38 f32, +0x3C u16, +0x3E u8 x2; +0xA0 bbox.
//!
//! Grass batch: +0x80 bbox (min vec4, max vec4), +0xA0 vec3 scale range,
//! +0xB8 f32 LOD distance, +0x178 instances `ed86f522` (16 B): +0 u16 x3
//! position quantised in the bbox, +6 / +7 u8 normal x / y, +8 u8 x3
//! colour, +0xB u8 scale, +0xC u8 ambient occlusion, +0xD u8, +0xE u16.
use crate::meta7a::Meta;
use crate::resource::Resource;
use serde::Serialize;

pub const PROP_BATCH: u32 = 0xba04_9770;
pub const PROP_INSTANCE: u32 = 0xea6b_3a96;
pub const GRASS_BATCH: u32 = 0x7c47_5b5d;
pub const GRASS_INSTANCE: u32 = 0xed86_f522;

#[derive(Debug, Clone, Serialize)]
pub struct PropInstance {
    /// World position (RAGE, Z-up, metres): the matrix translation.
    pub p: [f32; 3],
    /// World rotation quaternion (x, y, z, w), from the matrix.
    pub q: [f32; 4],
    /// Scale per local axis (length of the matrix columns).
    pub s: [f32; 3],
    /// Tint, RGBA bytes (u32 at +0x34).
    pub tint: [u8; 4],
    /// f32 at +0x30 and +0x38, u16 at +0x3C, u8 x2 at +0x3E (unknown).
    pub raw: (f32, f32, u16, [u8; 2]),
}

#[derive(Debug, Clone, Serialize)]
pub struct PropBatch {
    pub archetype: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub bbox: [[f32; 3]; 2],
    pub lod_distance: f32,
    pub instances: Vec<PropInstance>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GrassInstance {
    /// World position (RAGE, Z-up, metres), dequantised in the batch bbox.
    pub p: [f32; 3],
    /// Ground normal from the two stored components (z = +sqrt(1 - x² - y²)).
    pub n: [f32; 3],
    pub colour: [u8; 3],
    pub scale: u8,
    pub ao: u8,
    /// u8 at +0xD and u16 at +0xE (unknown).
    pub raw: (u8, u16),
}

#[derive(Debug, Clone, Serialize)]
pub struct GrassBatch {
    pub archetype: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub bbox: [[f32; 3]; 2],
    /// vec3 at +0xA0 (GTA V ScaleRange: min, max, random).
    pub scale_range: [f32; 3],
    pub lod_distance: f32,
    pub instances: Vec<GrassInstance>,
}

fn arr(m: &Meta, o: usize) -> Option<(usize, usize)> {
    let (_, p) = m.ptr(m.r.u64(o))?;
    Some((p, m.r.u16(o + 8) as usize))
}

fn v3(r: &Resource, o: usize) -> [f32; 3] {
    [r.f32(o), r.f32(o + 4), r.f32(o + 8)]
}

/// Rotation matrix (columns normalised) to quaternion.
fn mat_to_quat(m: [[f32; 3]; 3]) -> [f32; 4] {
    let t = m[0][0] + m[1][1] + m[2][2];
    let (x, y, z, w) = if t > 0.0 {
        let s = (t + 1.0).sqrt() * 2.0;
        ((m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s, 0.25 * s)
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        (0.25 * s, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s, (m[2][1] - m[1][2]) / s)
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        ((m[0][1] + m[1][0]) / s, 0.25 * s, (m[1][2] + m[2][1]) / s, (m[0][2] - m[2][0]) / s)
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        ((m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, 0.25 * s, (m[1][0] - m[0][1]) / s)
    };
    let l = (x * x + y * y + z * z + w * w).sqrt().max(1e-9);
    [x / l, y / l, z / l, w / l]
}

pub fn prop_batches(r: &Resource, m: &Meta) -> Vec<PropBatch> {
    m.objects(PROP_BATCH)
        .into_iter()
        .map(|o| {
            let mut instances = Vec::new();
            if let Some((p, n)) = arr(m, o + 0x90) {
                for k in 0..n {
                    let q = p + 64 * k;
                    if q + 64 > r.sys.len() {
                        break;
                    }
                    // rows[i] = (m[i][0], m[i][1], m[i][2], t[i])
                    let row = |i: usize| [r.f32(q + 16 * i), r.f32(q + 16 * i + 4), r.f32(q + 16 * i + 8), r.f32(q + 16 * i + 12)];
                    let rows = [row(0), row(1), row(2)];
                    let mut mm = [[0f32; 3]; 3];
                    let mut s = [0f32; 3];
                    for c in 0..3 {
                        s[c] = (rows[0][c] * rows[0][c] + rows[1][c] * rows[1][c] + rows[2][c] * rows[2][c]).sqrt();
                        for i in 0..3 {
                            mm[i][c] = rows[i][c] / s[c].max(1e-9);
                        }
                    }
                    let t = r.u32(q + 0x34).to_le_bytes();
                    instances.push(PropInstance {
                        p: [rows[0][3], rows[1][3], rows[2][3]],
                        q: mat_to_quat(mm),
                        s,
                        tint: t,
                        raw: (r.f32(q + 0x30), r.f32(q + 0x38), r.u16(q + 0x3C), [r.sys.get(q + 0x3E).copied().unwrap_or(0), r.sys.get(q + 0x3F).copied().unwrap_or(0)]),
                    });
                }
            }
            PropBatch { archetype: r.u32(o + 0x44), name: None, model_id: None, bbox: [v3(r, o + 0xA0), v3(r, o + 0xB0)], lod_distance: r.f32(o + 0x60), instances }
        })
        .collect()
}

pub fn grass_batches(r: &Resource, m: &Meta) -> Vec<GrassBatch> {
    m.objects(GRASS_BATCH)
        .into_iter()
        .map(|o| {
            let (lo, hi) = (v3(r, o + 0x80), v3(r, o + 0x90));
            let mut instances = Vec::new();
            if let Some((p, n)) = arr(m, o + 0x178) {
                for k in 0..n {
                    let q = p + 16 * k;
                    if q + 16 > r.sys.len() {
                        break;
                    }
                    let b = &r.sys[q..q + 16];
                    let pos: [f32; 3] = std::array::from_fn(|i| lo[i] + (hi[i] - lo[i]) * u16::from_le_bytes([b[2 * i], b[2 * i + 1]]) as f32 / 65535.0);
                    let nx = b[6] as f32 / 255.0 * 2.0 - 1.0;
                    let ny = b[7] as f32 / 255.0 * 2.0 - 1.0;
                    let nz = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
                    instances.push(GrassInstance { p: pos, n: [nx, ny, nz], colour: [b[8], b[9], b[10]], scale: b[11], ao: b[12], raw: (b[13], u16::from_le_bytes([b[14], b[15]])) });
                }
            }
            GrassBatch { archetype: r.u32(o + 0x44), name: None, model_id: None, bbox: [lo, hi], scale_range: v3(r, o + 0xA0), lod_distance: r.f32(o + 0xB8), instances }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta7a::StructDef;
    use std::collections::HashMap;

    fn put_f32(b: &mut [u8], o: usize, v: f32) {
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// One prop batch whose instance array is the pool reference
    /// `0x5000_0003` (block 3, byte offset 0x500): it also reads as a real
    /// system pointer to sys offset 3, which must not win.
    fn synthetic(n: usize) -> (Resource, Vec<(u32, u32, usize)>) {
        let mut sys = vec![0u8; 0x2000];
        // Decoy bytes at sys offset 3 (what the real-pointer reading hits).
        for b in &mut sys[3..3 + 64 * n] {
            *b = 0x7f;
        }
        let batch = 0x200;
        sys[batch + 0x44..batch + 0x48].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        sys[batch + 0x90..batch + 0x98].copy_from_slice(&0x5000_0003u64.to_le_bytes());
        sys[batch + 0x98..batch + 0x9A].copy_from_slice(&(n as u16).to_le_bytes());
        // Instance block 3 at 0x800; the array starts 0x500 into it.
        let inst = 0x800 + 0x500;
        for k in 0..n {
            let q = inst + 64 * k;
            // rotation identity scaled by 2, translation (100 + k, 200, 30)
            put_f32(&mut sys, q, 2.0);
            put_f32(&mut sys, q + 12, 100.0 + k as f32);
            put_f32(&mut sys, q + 16 + 4, 2.0);
            put_f32(&mut sys, q + 16 + 12, 200.0);
            put_f32(&mut sys, q + 32 + 8, 2.0);
            put_f32(&mut sys, q + 32 + 12, 30.0);
        }
        let blocks = vec![(0x1111, 0x10, 0x100), (PROP_BATCH, 368, batch), (PROP_INSTANCE, 0x700, 0x800)];
        (Resource { sys, gfx: Vec::new(), gfx_base: 0 }, blocks)
    }

    fn meta<'a>(r: &'a Resource, blocks: Vec<(u32, u32, usize)>) -> Meta<'a> {
        let structs = HashMap::from([(PROP_BATCH, StructDef { hash: PROP_BATCH, size: 368, members: vec![] }), (PROP_INSTANCE, StructDef { hash: PROP_INSTANCE, size: 64, members: vec![] })]);
        Meta { r, structs, blocks, pso: false }
    }

    #[test]
    fn pool_reference_wins_over_real_pointer() {
        let (r, blocks) = synthetic(4);
        let m = meta(&r, blocks);
        assert_eq!(m.ptr(0x5000_0003), Some((PROP_INSTANCE, 0xD00)));
        // Same for the graphics-page look-alike (offset 0x600).
        assert_eq!(m.ptr(0x6000_0003), Some((PROP_INSTANCE, 0xE00)));
        // Not a valid pool reference (no block 0x10): the real pointer is used.
        assert_eq!(m.ptr(0x5000_0010), Some((0, 0x10)));
    }

    #[test]
    fn prop_batch_at_ambiguous_pool_reference() {
        let (r, blocks) = synthetic(5);
        let m = meta(&r, blocks);
        let b = prop_batches(&r, &m);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].archetype, 0x1234_5678);
        assert_eq!(b[0].instances.len(), 5);
        for (k, i) in b[0].instances.iter().enumerate() {
            assert_eq!(i.p, [100.0 + k as f32, 200.0, 30.0]);
            assert_eq!(i.s, [2.0, 2.0, 2.0]);
            assert!((i.q[3] - 1.0).abs() < 1e-6);
        }
    }
}
