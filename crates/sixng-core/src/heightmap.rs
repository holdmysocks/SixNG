//! Whole-world terrain height map: the `MHIU` blobs of `common.rpf`.
//!
//! The archive holds two of them, same grid and world box, about 1.2 MB
//! each. Layout (little endian):
//!
//! | offset | type      | meaning                                          |
//! |--------|-----------|--------------------------------------------------|
//! | 0x00   | 4 bytes   | magic `MHIU`                                     |
//! | 0x04   | u16       | version (1)                                      |
//! | 0x06   | u16       | width in samples (2048)                          |
//! | 0x08   | u32       | height in samples / rows (2048)                  |
//! | 0x0C   | 6 × f32   | world box: min x, y, z, max x, y, z (metres)     |
//! | 0x24   | u32       | bytes after the header (row table + samples)     |
//! | 0x28   | u8        | fill value for samples outside every run (0x17)  |
//! | 0x29   | 3 bytes   | unknown, differs between the two copies          |
//! | 0x2C   | rows × 8  | per row: u16 first column, u16 count, i32 offset |
//! | …      | u8 × size | samples                                          |
//!
//! Row `r` holds columns `first .. first + count`; the sample of column `x`
//! is `data[offset + x]` (the offset is pre-biased by `-first`, so it is
//! often negative). The counts add up to the sample bytes exactly. Row 0 is the
//! south edge (min y), column 0 the west edge (min x); a sample covers a
//! `(max - min) / size` square (9.77 m). A sample `v` is the height
//! `min_z + v * (max_z - min_z) / 255` (2.16 m per step); the fill value
//! 0x17 is sea level (-0.4 m). Rows with no run are all sea.
//!
//! It is a surface map: city blocks stand out. Checked against the collision
//! top-surface grid of the map bake, the heights sit a median of about 2 m
//! below the collision top. The second copy covers more land and agrees
//! better; what tells the two apart in game is not known.
use anyhow::{bail, Result};

pub const MAGIC: &[u8; 4] = b"MHIU";
const HEADER: usize = 0x2C;

#[derive(Debug, Clone)]
pub struct HeightMap {
    pub version: u16,
    pub width: usize,
    pub height: usize,
    /// min x, y, z
    pub min: [f32; 3],
    /// max x, y, z
    pub max: [f32; 3],
    pub fill: u8,
    /// Header bytes 0x29..0x2C (meaning unknown).
    pub unknown: [u8; 3],
    /// Samples, row-major, row 0 = south. Cells outside every run hold `fill`.
    pub samples: Vec<u8>,
    /// Number of samples stored in runs (the rest are fill).
    pub stored: usize,
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32_at(b, o))
}

pub fn is_heightmap(b: &[u8]) -> bool {
    b.len() >= HEADER && &b[..4] == MAGIC
}

pub fn decode(b: &[u8]) -> Result<HeightMap> {
    if !is_heightmap(b) {
        bail!("not an MHIU height map");
    }
    let version = u16_at(b, 4);
    let width = u16_at(b, 6) as usize;
    let height = u32_at(b, 8) as usize;
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        bail!("height map size {width}x{height} out of range");
    }
    let min = [f32_at(b, 0x0C), f32_at(b, 0x10), f32_at(b, 0x14)];
    let max = [f32_at(b, 0x18), f32_at(b, 0x1C), f32_at(b, 0x20)];
    let size = u32_at(b, 0x24) as usize;
    let fill = b[0x28];
    let unknown = [b[0x29], b[0x2A], b[0x2B]];
    let data_at = HEADER + height * 8;
    if size < height * 8 || b.len() < HEADER + size {
        bail!("height map truncated: {} bytes, header says {}", b.len(), HEADER + size);
    }
    let data = &b[data_at..HEADER + size];
    let mut samples = vec![fill; width * height];
    let mut stored = 0;
    for r in 0..height {
        let o = HEADER + r * 8;
        let first = u16_at(b, o) as usize;
        let count = u16_at(b, o + 2) as usize;
        let off = u32_at(b, o + 4) as i32 as i64;
        if count == 0 {
            continue;
        }
        let start = off + first as i64;
        if first + count > width || start < 0 || start as usize + count > data.len() {
            bail!("height map row {r} out of range");
        }
        let s = start as usize;
        samples[r * width + first..r * width + first + count].copy_from_slice(&data[s..s + count]);
        stored += count;
    }
    Ok(HeightMap { version, width, height, min, max, fill, unknown, samples, stored })
}

impl HeightMap {
    /// Metres per sample along x and y.
    pub fn cell(&self) -> [f32; 2] {
        [(self.max[0] - self.min[0]) / self.width as f32, (self.max[1] - self.min[1]) / self.height as f32]
    }
    /// Metres per sample step.
    pub fn step(&self) -> f32 {
        (self.max[2] - self.min[2]) / 255.0
    }
    pub fn z(&self, v: u8) -> f32 {
        self.min[2] + v as f32 * self.step()
    }
    /// Height at world (x, y), nearest sample; `None` outside the box.
    pub fn at(&self, x: f32, y: f32) -> Option<f32> {
        let [cx, cy] = self.cell();
        let c = ((x - self.min[0]) / cx).floor();
        let r = ((y - self.min[1]) / cy).floor();
        if c < 0.0 || r < 0.0 || c as usize >= self.width || r as usize >= self.height {
            return None;
        }
        Some(self.z(self.samples[r as usize * self.width + c as usize]))
    }
    /// Bilinear height at world (x, y) and whether any of the four samples
    /// is land (above the fill value); `None` outside the box.
    pub fn sample(&self, x: f32, y: f32) -> Option<(f32, bool)> {
        let [cx, cy] = self.cell();
        let fx = (x - self.min[0]) / cx - 0.5;
        let fy = (y - self.min[1]) / cy - 0.5;
        if fx < -0.5 || fy < -0.5 || fx > self.width as f32 - 0.5 || fy > self.height as f32 - 0.5 {
            return None;
        }
        let (c0, r0) = (fx.floor().max(0.0) as usize, fy.floor().max(0.0) as usize);
        let (c1, r1) = ((c0 + 1).min(self.width - 1), (r0 + 1).min(self.height - 1));
        let (tx, ty) = ((fx - c0 as f32).clamp(0.0, 1.0), (fy - r0 as f32).clamp(0.0, 1.0));
        let s = |c: usize, r: usize| self.samples[r * self.width + c];
        let q = [s(c0, r0), s(c1, r0), s(c0, r1), s(c1, r1)];
        let z = |v: u8| self.z(v);
        let h = (z(q[0]) * (1.0 - tx) + z(q[1]) * tx) * (1.0 - ty) + (z(q[2]) * (1.0 - tx) + z(q[3]) * tx) * ty;
        Some((h, q.iter().any(|&v| v > self.fill)))
    }
    /// Samples flipped to image order (row 0 = north), widened to 16 bits
    /// (`v * 257`, so 0..65535 spans min_z..max_z).
    pub fn gray16_north_up(&self) -> Vec<u16> {
        let mut out = Vec::with_capacity(self.samples.len());
        for r in (0..self.height).rev() {
            out.extend(self.samples[r * self.width..(r + 1) * self.width].iter().map(|&v| v as u16 * 257));
        }
        out
    }
    /// Hill-shaded RGB preview, north up: light from the north-west, tinted
    /// by height, sea blue.
    pub fn hillshade_rgb(&self) -> Vec<u8> {
        let (w, h) = (self.width, self.height);
        let [cx, cy] = self.cell();
        let z = |c: usize, r: usize| self.z(self.samples[r * w + c]);
        let (lx, ly, lz) = { let l = (-1.0f32, 1.0f32, 1.4f32); let n = (l.0 * l.0 + l.1 * l.1 + l.2 * l.2).sqrt(); (l.0 / n, l.1 / n, l.2 / n) };
        let mut out = vec![0u8; w * h * 3];
        for r in 0..h {
            for c in 0..w {
                let v = self.samples[r * w + c];
                let dzdx = (z((c + 1).min(w - 1), r) - z(c.saturating_sub(1), r)) / (2.0 * cx);
                let dzdy = (z(c, (r + 1).min(h - 1)) - z(c, r.saturating_sub(1))) / (2.0 * cy);
                let (nx, ny, nz) = (-dzdx * 2.0, -dzdy * 2.0, 1.0);
                let nl = (nx * nx + ny * ny + nz * nz).sqrt();
                let shade = ((nx * lx + ny * ly + nz * lz) / nl).clamp(0.0, 1.0);
                let rgb = if v <= self.fill {
                    [40.0, 70.0, 110.0]
                } else {
                    let t = ((self.z(v) / 300.0).clamp(0.0, 1.0)).sqrt();
                    [110.0 + 110.0 * t, 140.0 + 60.0 * t, 90.0 + 100.0 * t]
                };
                let k = if v <= self.fill { 1.0 } else { 0.35 + 0.85 * shade };
                let o = ((h - 1 - r) * w + c) * 3;
                for i in 0..3 {
                    out[o + i] = (rgb[i] * k).clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    }
}

/// Every height map in an indexed archive (normally `common`), in archive
/// order, with its entry id.
pub fn load_all(ws: &crate::export::Workspace, archive: &str) -> Result<Vec<(String, HeightMap)>> {
    let a = ws.resolve(archive)?;
    let ix = ws.load_index(&a)?;
    let mut maps = Vec::new();
    for e in ix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Bin && e.usize >= HEADER as u64) {
        let b = crate::scan::read_entry(&a.path, e)?;
        if is_heightmap(&b) {
            maps.push((e.id.clone(), decode(&b)?));
        }
    }
    Ok(maps)
}

/// Index of the copy with the most stored samples (the one that covers the
/// most land and agrees best with the collision).
pub fn best(maps: &[(String, HeightMap)]) -> Option<usize> {
    (0..maps.len()).max_by_key(|&i| maps[i].1.stored)
}

/// Writes the height map as a 16-bit greyscale PNG, north up.
pub fn write_png16(m: &HeightMap, path: &std::path::Path) -> Result<()> {
    let px = m.gray16_north_up();
    let img = image::ImageBuffer::<image::Luma<u16>, Vec<u16>>::from_raw(m.width as u32, m.height as u32, px)
        .ok_or_else(|| anyhow::anyhow!("image size"))?;
    img.save(path)?;
    Ok(())
}

/// Writes the hill-shaded preview as an RGB PNG, north up.
pub fn write_hillshade(m: &HeightMap, path: &std::path::Path) -> Result<()> {
    image::save_buffer(path, &m.hillshade_rgb(), m.width as u32, m.height as u32, image::ColorType::Rgb8)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build() -> Vec<u8> {
        // 4 x 2 map, box 0..40 x 0..20 x 0..255
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes());
        for f in [0.0f32, 0.0, 0.0, 40.0, 20.0, 255.0] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&(2 * 8 + 3u32).to_le_bytes());
        b.extend_from_slice(&[0x17, 0, 0, 0]);
        // row 0: columns 1..3 at data[0..2] -> offset -1
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&(-1i32).to_le_bytes());
        // row 1: column 3 at data[2] -> offset -1
        b.extend_from_slice(&3u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&(-1i32).to_le_bytes());
        b.extend_from_slice(&[100, 101, 200]);
        b
    }

    #[test]
    fn decodes_runs() {
        let m = decode(&build()).unwrap();
        assert_eq!(m.samples, vec![0x17, 100, 101, 0x17, 0x17, 0x17, 0x17, 200]);
        assert_eq!(m.stored, 3);
        assert_eq!(m.at(15.0, 5.0), Some(100.0));
        assert_eq!(m.at(35.0, 15.0), Some(200.0));
        assert_eq!(m.gray16_north_up()[3], 200 * 257);
        // centre of sample (1, 0) and halfway to (2, 0)
        assert_eq!(m.sample(15.0, 5.0), Some((100.0, true)));
        assert_eq!(m.sample(20.0, 5.0), Some((100.5, true)));
        assert_eq!(m.sample(5.0, 15.0), Some((23.0, false)));
        assert_eq!(m.sample(-1.0, 5.0), None);
    }

    #[test]
    fn rejects_bad_rows() {
        let mut b = build();
        b[HEADER + 4..HEADER + 8].copy_from_slice(&50i32.to_le_bytes());
        assert!(decode(&b).is_err());
    }
}
