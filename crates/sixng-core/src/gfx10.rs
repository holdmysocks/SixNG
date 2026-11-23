//! AMD Gfx10 (RDNA2, Xbox Series) 2D texture layout, as used by the `ttpf`
//! stores. Port of the parts of addrlib (`Gfx10Lib::ComputeSurfaceInfoMacroTiled`)
//! that place mips, for thin 2D surfaces without pipe/bank XOR.
//!
//! * Each swizzle mode is a block (4 KiB or 64 KiB) with a fixed bit order of
//!   x/y element-coordinate bits inside it; blocks are stored row-major.
//! * Mips are stored **smallest first**; mip 0 is last.
//! * Mips that fit in half a block (`w <= block_w / 2`, `h <= block_h`) and are
//!   among the last `max_mips_in_tail` share one "mip tail" block at offset 0,
//!   each at a fixed 2D origin derived from its tail offset.
//!
//! * A single-mip surface has no mip tail: its one mip starts at offset 0,
//!   whatever its size (AMD addrlib only builds a tail for more than one mip).
//! * 3D (volume) textures in `SW_4KB_S` use the 3D "S3" pattern: each 4 KiB
//!   block is a small x*y*z brick with Z interleaved into the address bits;
//!   bricks are stored x first, then y, then z (`Layout::new_3d`).
//!
//! Verified on the stores: tile mode 0 = `SW_64KB_D`, modes 1, 2, 3 =
//! `SW_4KB_S` (mip n+1 equals mip n downsampled, every format checked;
//! 1-byte elements (A8 / R8) added 2026-11-21, checked the same way; 4-byte
//! elements checked by mip consistency (mode 0) and by eye (mode 1), 2-byte
//! elements by image smoothness on R16F textures).

/// Coordinate axes in the equations.
const X: u8 = 0;
const Y: u8 = 1;
const Z: u8 = 2;

/// Element-coordinate bit order inside a block, low bit first:
/// (axis, coordinate bit).
fn equation(sw: Swizzle, bpe: u32) -> Option<&'static [(u8, u8)]> {
    Some(match (sw, bpe) {
        (Swizzle::S4K, 16) => &[(Y, 0), (Y, 1), (X, 0), (X, 1), (Y, 2), (X, 2), (Y, 3), (X, 3)],
        (Swizzle::S4K, 8) => &[(X, 0), (Y, 0), (Y, 1), (X, 1), (X, 2), (Y, 2), (X, 3), (Y, 3), (X, 4)],
        (Swizzle::D64K, 16) => &[(X, 0), (Y, 0), (X, 1), (Y, 1), (Y, 2), (X, 2), (Y, 3), (X, 3), (Y, 4), (X, 4), (Y, 5), (X, 5)],
        (Swizzle::D64K, 8) => &[(X, 0), (Y, 0), (X, 1), (X, 2), (Y, 1), (Y, 2), (X, 3), (Y, 3), (X, 4), (Y, 4), (X, 5), (Y, 5), (X, 6)],
        // 4-byte elements (RGBA8 / BGRA8)
        (Swizzle::S4K, 4) => &[(X, 0), (X, 1), (Y, 0), (Y, 1), (Y, 2), (X, 2), (Y, 3), (X, 3), (Y, 4), (X, 4)],
        (Swizzle::D64K, 4) => &[(X, 0), (X, 1), (Y, 0), (Y, 1), (X, 2), (Y, 2), (Y, 3), (X, 3), (Y, 4), (X, 4), (Y, 5), (X, 5), (Y, 6), (X, 6)],
        // 2-byte elements (R16F / RG8)
        (Swizzle::S4K, 2) => &[(X, 0), (X, 1), (X, 2), (Y, 0), (Y, 1), (Y, 2), (X, 3), (Y, 3), (X, 4), (Y, 4), (X, 5)],
        (Swizzle::D64K, 2) => &[(X, 0), (X, 1), (X, 2), (Y, 0), (Y, 1), (Y, 2), (X, 3), (Y, 3), (X, 4), (Y, 4), (X, 5), (Y, 5), (X, 6), (Y, 6), (X, 7)],
        // 1-byte elements (R8 / A8)
        (Swizzle::S4K, 1) => &[(X, 0), (X, 1), (X, 2), (X, 3), (Y, 0), (Y, 1), (Y, 2), (Y, 3), (Y, 4), (X, 4), (Y, 5), (X, 5)],
        (Swizzle::D64K, 1) => &[(X, 0), (X, 1), (X, 2), (Y, 1), (Y, 0), (Y, 2), (X, 3), (Y, 3), (Y, 4), (X, 4), (Y, 5), (X, 5), (Y, 6), (X, 6), (Y, 7), (X, 7)],
        _ => return None,
    })
}

/// Element-coordinate bit order inside a 4 KiB block of a 3D `SW_4KB_S`
/// texture, low bit first. From AMD addrlib (Gfx10 swizzle patterns, MIT):
/// the `SW_4K_S3` pattern (non-XOR, so no pipe/bank bits), with the
/// byte-within-element bits dropped. Only the element sizes measured on the
/// install are mapped: 2 bytes (R16F volume, 8x16x16 elements per block) and
/// 8 bytes (R16G16B16A16F volumes, 8x8x8). The pattern exists for 1, 4 and
/// 16 bytes too, but no volume in the stores uses them, so they are rejected.
fn equation_3d(sw: Swizzle, bpe: u32) -> Option<&'static [(u8, u8)]> {
    Some(match (sw, bpe) {
        (Swizzle::S4K, 2) => &[(X, 0), (Z, 0), (Y, 0), (Z, 1), (Y, 1), (X, 1), (Z, 2), (Y, 2), (X, 2), (Z, 3), (Y, 3)],
        (Swizzle::S4K, 8) => &[(X, 0), (Z, 0), (Y, 0), (X, 1), (Z, 1), (Y, 1), (X, 2), (Z, 2), (Y, 2)],
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swizzle {
    /// SW_4KB_S
    S4K,
    /// SW_64KB_D
    D64K,
}

#[derive(Debug, Clone)]
pub struct MipPlace {
    /// Size in elements.
    pub w: u32,
    pub h: u32,
    /// Byte offset of the mip's first block (non-tail) or of the tail block.
    pub offset: u64,
    /// Origin inside the tail block, in elements (tail mips only).
    pub tail: Option<(u32, u32)>,
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub eq: &'static [(u8, u8)],
    pub bpe: u32,
    pub block_w: u32,
    pub block_h: u32,
    /// Block depth in elements (1 for 2D; > 1 for 3D textures, whose
    /// `slices` are the depth).
    pub block_d: u32,
    pub block_bytes: u64,
    pub mips: Vec<MipPlace>,
    /// Array slices (1 for a plain 2D texture); the depth of a 3D texture.
    pub slices: u32,
    /// Slices stored one after another, each with its whole mip chain
    /// (`slice_bytes` apart), instead of level by level.
    pub slice_major: bool,
    /// Bytes of one slice's mip chain.
    pub slice_bytes: u64,
    /// Byte offset of slice 0 (slice-major layouts that follow lower-resolution
    /// copies; see `with_cube_copies`).
    pub base: u64,
    /// Bytes of the whole texture (all mips, all slices).
    pub total: u64,
}

impl Layout {
    /// `w`, `h` in elements (BCn blocks), `bpe` bytes per element.
    pub fn new(sw: Swizzle, w: u32, h: u32, mips: u32, bpe: u32) -> Option<Layout> {
        let eq = equation(sw, bpe)?;
        let nx = eq.iter().filter(|e| e.0 == X).count() as u32;
        let (bw, bh) = (1u32 << nx, 1u32 << (eq.len() as u32 - nx));
        let bs = (bw * bh * bpe) as u64;
        let log2 = 63 - bs.leading_zeros();
        let max_tail = if log2 <= 11 { 1 + (1 << (log2 - 9)) } else { log2 - 4 };
        let (tw, th) = (bw / 2, bh);
        let nm = mips.max(1);
        let dims: Vec<(u32, u32)> = (0..nm).map(|i| ((w >> i).max(1), (h >> i).max(1))).collect();
        // AMD addrlib (Gfx10 swizzle patterns, MIT): the mip tail exists only
        // for more than one mip; a single mip starts at offset 0 even when it
        // would fit in the tail (measured on the install: 16x8 RGBA8 and other
        // small single-mip textures hold their data at the start of the block).
        let first = if nm == 1 { 1 } else { dims.iter().enumerate().position(|(i, &(mw, mh))| mw <= tw && mh <= th && nm - i as u32 <= max_tail).unwrap_or(nm as usize) };
        let mut place: Vec<MipPlace> = dims.iter().map(|&(mw, mh)| MipPlace { w: mw, h: mh, offset: 0, tail: None }).collect();
        let mut off = if first < nm as usize { bs } else { 0 };
        for i in (0..first).rev() {
            let (mw, mh) = dims[i];
            place[i].offset = off;
            off += (mw.div_ceil(bw) * bw) as u64 * (mh.div_ceil(bh) * bh) as u64 * bpe as u64;
        }
        // 256-byte block in elements (the unit of mip-tail origins)
        let b256 = match bpe {
            16 => (4, 4),
            8 => (8, 4),
            4 => (8, 8),
            2 => (16, 8),
            _ => (16, 16),
        };
        for (k, p) in place.iter_mut().enumerate().skip(first) {
            let m = max_tail - 1 - (k - first) as u32;
            let mo: u32 = if m > 6 { 16 << m } else { m << 8 };
            let (mut mx, mut my) = (0u32, 0u32);
            for b in 0..6 {
                mx |= ((mo >> (9 + 2 * b)) & 1) << b;
                my |= ((mo >> (8 + 2 * b)) & 1) << b;
            }
            p.offset = 0;
            p.tail = Some((mx * b256.0, my * b256.1));
        }
        Some(Layout { eq, bpe, block_w: bw, block_h: bh, block_d: 1, block_bytes: bs, mips: place, slices: 1, slice_major: false, slice_bytes: off, base: 0, total: off })
    }

    /// Layout of a single-mip 3D texture of `w` x `h` x `d` elements.
    /// `None` for element sizes without a measured 3D equation, and for
    /// `SW_64KB_D` (no volume in the stores uses it).
    pub fn new_3d(sw: Swizzle, w: u32, h: u32, d: u32, bpe: u32) -> Option<Layout> {
        let eq = equation_3d(sw, bpe)?;
        let n = |a: u8| eq.iter().filter(|e| e.0 == a).count() as u32;
        let (bw, bh, bd) = (1u32 << n(X), 1u32 << n(Y), 1u32 << n(Z));
        let bs = (bw * bh * bd * bpe) as u64;
        let total = w.div_ceil(bw) as u64 * h.div_ceil(bh) as u64 * d.max(1).div_ceil(bd) as u64 * bs;
        let mips = vec![MipPlace { w, h, offset: 0, tail: None }];
        Some(Layout { eq, bpe, block_w: bw, block_h: bh, block_d: bd, block_bytes: bs, mips, slices: d.max(1), slice_major: false, slice_bytes: total, base: 0, total })
    }

    /// The same layout for an array of `n` slices. Arrays are stored level by
    /// level, smallest first, with each level's slices back to back: the tail
    /// block of every slice, then mip k of every slice, ... then mip 0 of every
    /// slice. (Decoding slice 0 with the single-slice layout gives a mosaic of
    /// other slices' mip tails.)
    pub fn with_slices(mut self, n: u32) -> Layout {
        let n = n.max(1);
        self.total = self.slice_bytes * n as u64;
        self.slices = n;
        self
    }

    /// Layout of the ttpf cube maps (tile modes 9-12): `copies` complete
    /// copies of the array, smallest first, each stored slice by slice with
    /// every slice's mip chain back to back. Copy `c` holds the smallest
    /// `units - copies + 1 + c` "units" of the chain, where a unit is the
    /// mip-tail block or one mip above it; the last copy is the full texture,
    /// and that is the one this layout addresses. Each copy is its own
    /// chunk in the store; one larger than 128 KiB is padded to a multiple of
    /// 64 KiB. `base` overrides the computed offset of the last copy.
    /// `None` if `copies` is more than the chain has units.
    pub fn with_cube_copies(mut self, slices: u32, copies: u32, base: Option<u64>) -> Option<Layout> {
        let first = self.mips.iter().position(|p| p.tail.is_some()).unwrap_or(self.mips.len());
        let tail = first < self.mips.len();
        let units = first + tail as usize;
        let n = copies as usize;
        if n == 0 || n > units {
            return None;
        }
        // bytes of one slice's chain up to and including unit j
        let chain = |j: usize| -> u64 {
            if tail && j == 0 {
                self.block_bytes
            } else {
                let m = first - (j + 1 - tail as usize);
                self.mips[m].offset + self.level_bytes(m)
            }
        };
        let s = slices.max(1) as u64;
        let pad = |b: u64| if b > 128 << 10 { b.div_ceil(64 << 10) * (64 << 10) } else { b };
        let base = base.unwrap_or_else(|| (0..n - 1).map(|c| pad(s * chain(units - n + c))).sum());
        self.slices = slices.max(1);
        self.slice_major = true;
        self.base = base;
        self.total = base + s * self.slice_bytes;
        Some(self)
    }

    /// Bytes of one slice of mip `m` (the tail block for tail mips).
    fn level_bytes(&self, m: usize) -> u64 {
        let p = &self.mips[m];
        match p.tail {
            Some(_) => self.block_bytes,
            None => (p.w.div_ceil(self.block_w) * self.block_w) as u64 * (p.h.div_ceil(self.block_h) * self.block_h) as u64 * self.bpe as u64,
        }
    }

    /// Start of slice `s` of mip `m`.
    fn level_start(&self, m: usize, s: u32) -> u64 {
        if self.slice_major {
            let off = if self.mips[m].tail.is_some() { 0 } else { self.mips[m].offset };
            return self.base + s as u64 * self.slice_bytes + off;
        }
        let base = match self.mips[m].tail {
            Some(_) => 0,
            None => self.mips[m].offset * self.slices as u64,
        };
        base + s as u64 * self.level_bytes(m)
    }

    fn inblock(&self, x: u32, y: u32, z: u32) -> u64 {
        let mut a = 0u64;
        for (k, &(axis, b)) in self.eq.iter().enumerate() {
            let v = match axis {
                X => x,
                Y => y,
                _ => z,
            };
            a |= (((v >> b) & 1) as u64) << k;
        }
        a
    }

    /// Byte range [start, end) that slice 0 of mip `m` occupies (the whole
    /// tail block for tail mips).
    pub fn mip_span(&self, m: usize) -> (u64, u64) {
        let a = self.level_start(m, 0);
        (a, a + self.level_bytes(m))
    }

    /// Byte address of element (x, y) of slice `s` of mip `m` (for a 3D
    /// texture, `s` is the depth coordinate z).
    pub fn addr(&self, m: usize, s: u32, x: u32, y: u32) -> u64 {
        let p = &self.mips[m];
        if self.block_d > 1 {
            let (pitch, rows) = (p.w.div_ceil(self.block_w) as u64, p.h.div_ceil(self.block_h) as u64);
            let blk = (s / self.block_d) as u64 * pitch * rows + (y / self.block_h) as u64 * pitch + (x / self.block_w) as u64;
            return blk * self.block_bytes + self.inblock(x % self.block_w, y % self.block_h, s % self.block_d) * self.bpe as u64;
        }
        let base = self.level_start(m, s);
        match p.tail {
            Some((cx, cy)) => base + self.inblock(x + cx, y + cy, 0) * self.bpe as u64,
            None => {
                let pitch = p.w.div_ceil(self.block_w) as u64;
                let blk = (y / self.block_h) as u64 * pitch + (x / self.block_w) as u64;
                base + blk * self.block_bytes + self.inblock(x % self.block_w, y % self.block_h, 0) * self.bpe as u64
            }
        }
    }

    /// Linear (row-major) elements of slice 0 of mip `m`. Missing source
    /// bytes stay zero.
    pub fn untile(&self, src: &[u8], m: usize) -> Vec<u8> {
        self.untile_slice(src, m, 0)
    }

    /// Linear elements of slice `s` of mip `m`.
    pub fn untile_slice(&self, src: &[u8], m: usize, sl: u32) -> Vec<u8> {
        let p = &self.mips[m];
        let b = self.bpe as usize;
        let mut out = vec![0u8; (p.w * p.h) as usize * b];
        for y in 0..p.h {
            for x in 0..p.w {
                let s = self.addr(m, sl, x, y) as usize;
                if let Some(e) = src.get(s..s + b) {
                    let d = (y * p.w + x) as usize * b;
                    out[d..d + b].copy_from_slice(e);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizes_match_store_spacing() {
        // 64x64 BC1 (16x16 elements), 6 mips: whole chain in one 4 KiB tail.
        assert_eq!(Layout::new(Swizzle::S4K, 16, 16, 6, 8).unwrap().total, 4096);
        // 128x128 BC1, 7 mips: tail block + mip 0 (2 blocks).
        let l = Layout::new(Swizzle::S4K, 32, 32, 7, 8).unwrap();
        assert_eq!(l.total, 12288);
        assert_eq!(l.mips[0].offset, 4096);
        assert_eq!(l.mips[1].tail, Some((16, 0)));
        // 128x64 BC7, 6 mips.
        assert_eq!(Layout::new(Swizzle::S4K, 32, 16, 6, 16).unwrap().total, 16384);
    }

    #[test]
    fn arrays_and_cube_copies_match_store_spacing() {
        // 256x256 BC5, 9 mips, 8 slices (SW_64KB_D): level by level.
        let l = Layout::new(Swizzle::D64K, 64, 64, 9, 16).unwrap().with_slices(8);
        assert_eq!(l.total, 1 << 20);
        assert_eq!(l.mip_span(0), (8 * 65536, 9 * 65536));
        assert_eq!(l.addr(0, 3, 0, 0), 11 * 65536);
        // 128x128 BC7 cube, 6 mips, 3 copies: 6 + 12 pages, then the full copy.
        let l = Layout::new(Swizzle::S4K, 32, 32, 6, 16).unwrap().with_cube_copies(6, 3, None).unwrap();
        assert_eq!(l.base, 18 * 4096);
        assert_eq!(l.total, (18 + 36) * 4096);
        // 512x512 BC1 cube, 7 mips, 4 copies: 6 + 18 + 66 (padded to 80) pages.
        let l = Layout::new(Swizzle::S4K, 128, 128, 7, 8).unwrap().with_cube_copies(6, 4, None).unwrap();
        assert_eq!(l.base, 104 * 4096);
        assert_eq!(l.total, (104 + 258) * 4096);
    }

    #[test]
    fn one_byte_elements_match_store_spacing() {
        // A8 512x512, 6 mips, tile 2: 4 full mips + one 4 KiB tail block
        assert_eq!(Layout::new(Swizzle::S4K, 512, 512, 6, 1).unwrap().total, 352256);
        // A8 1024x1024, 7 mips, tile 0: 3 full mips + one 64 KiB tail block
        assert_eq!(Layout::new(Swizzle::D64K, 1024, 1024, 7, 1).unwrap().total, 1441792);
    }

    #[test]
    fn two_and_four_byte_elements_match_store_spacing() {
        // RGBA8 512x512, 6 mips, mode 0; R16F 256x256, 1 mip, mode 0
        assert_eq!(Layout::new(Swizzle::D64K, 512, 512, 6, 4).unwrap().total, 1441792);
        assert_eq!(Layout::new(Swizzle::D64K, 256, 256, 1, 2).unwrap().total, 131072);
        // RGBA8 256x256, 5 mips, mode 2
        assert_eq!(Layout::new(Swizzle::S4K, 256, 256, 5, 4).unwrap().total, 352256);
    }

    /// Fill a tiled buffer so every element holds its own coordinates.
    fn tiled(l: &Layout, m: usize, slices: u32) -> Vec<u8> {
        let p = &l.mips[m];
        let mut buf = vec![0u8; l.total as usize];
        for s in 0..slices {
            for y in 0..p.h {
                for x in 0..p.w {
                    let a = l.addr(m, s, x, y) as usize;
                    let v = [x as u8, y as u8, s as u8, 0xA5];
                    for k in 0..l.bpe as usize {
                        buf[a + k] = v[k % 4];
                    }
                }
            }
        }
        buf
    }

    #[test]
    fn single_mip_has_no_tail() {
        // 16x8 RGBA8, one mip: fits the tail block, but starts at offset 0.
        let l = Layout::new(Swizzle::S4K, 16, 8, 1, 4).unwrap();
        assert_eq!((l.mips[0].offset, l.mips[0].tail, l.total), (0, None, 4096));
        assert_eq!((l.addr(0, 0, 0, 0), l.addr(0, 0, 1, 0), l.addr(0, 0, 0, 1)), (0, 4, 16));
        let lin = l.untile(&tiled(&l, 0, 1), 0);
        assert_eq!(&lin[(3 * 16 + 5) * 4..][..4], &[5, 3, 0, 0xA5]);
        // two mips: both in the tail, mip 0 at a non-zero origin
        let l = Layout::new(Swizzle::S4K, 16, 8, 2, 4).unwrap();
        assert!(l.mips[0].tail.is_some_and(|o| o != (0, 0)));
    }

    #[test]
    fn volumes_interleave_z_in_blocks() {
        // 32x64x32, 8-byte elements: 8x8x8 bricks of 4 KiB.
        let l = Layout::new_3d(Swizzle::S4K, 32, 64, 32, 8).unwrap();
        assert_eq!((l.block_w, l.block_h, l.block_d, l.total), (8, 8, 8, 524288));
        // x0, z0, y0 are the lowest element bits
        assert_eq!((l.addr(0, 0, 1, 0), l.addr(0, 1, 0, 0), l.addr(0, 0, 0, 1)), (8, 16, 32));
        // next brick along x, then y, then z
        assert_eq!((l.addr(0, 0, 8, 0), l.addr(0, 0, 0, 8), l.addr(0, 8, 0, 0)), (4096, 4 * 4096, 32 * 4096));
        // 256x32x8, 2-byte elements: 8x16x16 bricks; depth 8 leaves z3 = 0,
        // so every other KiB of each brick is unused
        let l = Layout::new_3d(Swizzle::S4K, 256, 32, 8, 2).unwrap();
        assert_eq!((l.block_w, l.block_h, l.block_d, l.total), (8, 16, 16, 262144));
        let mut seen = std::collections::HashSet::new();
        for z in 0..8 {
            for y in 0..32 {
                for x in 0..256 {
                    let a = l.addr(0, z, x, y);
                    assert!(a + 2 <= l.total && (a >> 10) & 1 == 0 && seen.insert(a));
                }
            }
        }
        // round trip
        let l = Layout::new_3d(Swizzle::S4K, 32, 64, 32, 8).unwrap();
        let src = tiled(&l, 0, 32);
        let lin = l.untile_slice(&src, 0, 21);
        assert_eq!(&lin[(40 * 32 + 17) * 8..][..4], &[17, 40, 21, 0xA5]);
    }

    #[test]
    fn unmeasured_volume_element_sizes_are_rejected() {
        for bpe in [1, 4, 16] {
            assert!(Layout::new_3d(Swizzle::S4K, 32, 32, 32, bpe).is_none());
        }
        assert!(Layout::new_3d(Swizzle::D64K, 32, 32, 32, 8).is_none());
    }
}
