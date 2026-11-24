//! The four lookup tables of the game's hair and fur lighting.
//!
//! The deferred hair lighting program (`TL_PS_PBR_MATERIAL_HAIR`) weights
//! the lobes of its fibre model with four float textures of the graphics
//! texture store. The game's code loads them by hash and binds them in this
//! order (read from the executable and the compiled program by the shader
//! archive work):
//!
//! | table | size | holds |
//! |---|---|---|
//! | `M` | 32 x 32 x 32, 4 channels | along the strand: the R, TT, TRT and residual lobes by (sin of the light angle, sin of the view angle, roughness slice) |
//! | `N` | 32 x 64 x 32, 4 channels | around the strand: two tables stacked in Y, by (view term, azimuth, where the view ray crosses the strand) |
//! | `A` | 256 x 2, 4 channels | reflectance (r, g) and path length (a) by the squared sin of the view angle |
//! | `P` | 256 x 32 x 8, 1 channel | a per-channel factor by (colour, strand-to-strand transmission, 1 - g0) |
//!
//! How the program combines them is in the viewer's mesh shader
//! (`hair_game`), transcribed from the compiled code.

use std::path::Path;

/// Texture hashes of the tables, in the order M, N, A, P.
pub const TABLES: [u64; 4] = [0xa089_90fd_0ae2_b38e, 0x325a_463e_5738_bef9, 0xe928_838e_4c2c_b253, 0xfd95_4c59_7b21_d41c];

/// One table: `d` slices of `w` x `h` texels with `ch` interleaved channels.
pub struct Lut {
    pub w: u32,
    pub h: u32,
    pub d: u32,
    pub ch: u32,
    pub v: Vec<f32>,
}

/// The four tables from the game's texture stores, with their real values.
pub fn load(game: &Path) -> Result<Vec<Lut>, String> {
    let set = crate::ttpf::global(game);
    TABLES
        .iter()
        .map(|&h| {
            let (store, t) = set.get(h).ok_or_else(|| format!("hair lighting table {h:016x} is in no texture store"))?;
            let data = store.data(t).map_err(|e| format!("{e:#}"))?;
            let d = t.slices().max(1);
            let mut lut = Lut { w: 0, h: 0, d, ch: 0, v: Vec::new() };
            for sl in 0..d {
                let (v, ch, w, hh) = crate::ttpf::decode_float_slice(t, &data, 0, sl).map_err(|e| format!("table {h:016x}: {e:#}"))?;
                (lut.w, lut.h, lut.ch) = (w, hh, ch);
                lut.v.extend(v);
            }
            Ok(lut)
        })
        .collect()
}

/// A float as the bits of a half float (round to nearest, clamped).
pub fn half_bits(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let e = ((b >> 23) & 0xff) as i32 - 127;
    let m = b & 0x7f_ffff;
    if x.is_nan() {
        return sign | 0x7e00;
    }
    if e > 15 {
        return sign | 0x7bff; // largest finite
    }
    if e >= -14 {
        let (mut he, mut hm) = ((e + 15) as u32, (m + 0x1000) >> 13);
        if hm == 0x400 {
            hm = 0;
            he += 1;
        }
        return if he >= 31 { sign | 0x7bff } else { sign | ((he << 10) | hm) as u16 };
    }
    if e >= -24 {
        let full = m | 0x80_0000;
        let shift = (-14 - e) as u32 + 13;
        return sign | ((full + (1 << (shift - 1))) >> shift) as u16;
    }
    sign
}

impl Lut {
    /// The table as RGBA half floats (missing channels: 0, alpha 1).
    pub fn rgba16(&self) -> Vec<u8> {
        let n = (self.w * self.h * self.d) as usize;
        let ch = self.ch.max(1) as usize;
        let mut out = Vec::with_capacity(n * 8);
        for i in 0..n {
            for c in 0..4 {
                let x = if c < ch { self.v.get(i * ch + c).copied().unwrap_or(0.0) } else if c == 3 { 1.0 } else { 0.0 };
                out.extend_from_slice(&half_bits(x).to_le_bytes());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats_round_trip_the_usual_values() {
        let back = |x: f32| crate::groom::f16(half_bits(x));
        for x in [0.0f32, 1.0, -1.0, 0.5, 0.25, 45.75, 2.5, 0.159_154_94, 1e-3, 65504.0] {
            assert!((back(x) - x).abs() <= x.abs() * 1e-3 + 1e-6, "{x} -> {}", back(x));
        }
        assert_eq!((half_bits(0.0), half_bits(1.0), half_bits(-2.0)), (0, 0x3c00, 0xc000));
        // too large: the largest half, not infinity
        assert_eq!(half_bits(1e9), 0x7bff);
        let l = Lut { w: 2, h: 1, d: 1, ch: 1, v: vec![1.0, 0.5] };
        let b = l.rgba16();
        assert_eq!(b.len(), 16);
        assert_eq!(&b[0..8], &[0x00, 0x3c, 0, 0, 0, 0, 0x00, 0x3c]);
    }
}
