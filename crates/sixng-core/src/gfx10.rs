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
//! Verified on the stores: tile mode 0 = `SW_64KB_D`, modes 1, 2, 3 =
//! `SW_4KB_S` (mip n+1 equals mip n downsampled, every format checked;
//! 1-byte elements (A8 / R8) added 2026-11-21, checked the same way; 4-byte
//! elements checked by mip consistency (mode 0) and by eye (mode 1), 2-byte
//! elements by image smoothness on R16F textures). One R16F array uses
//! its own order with every other KiB unused (`untile_split_2b`).

/// Element-coordinate bit order inside a block, low bit first:
/// (is_x, coordinate bit).
fn equation(sw: Swizzle, bpe: u32) -> Option<&'static [(bool, u8)]> {
    const X: bool = true;
    const Y: bool = false;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swizzle {
    /// SW_4KB_S
    S4K,
    /// SW_64KB_D
    D64K,
}

/// Element order of the 2-byte arrays stored at twice their size (one R16F
/// array, `fd954c597b21d41c`): address bits 1-9 and 11 of a 4 KiB block of
/// 32x32 elements; bit 10 is always 0, so every other KiB is unused. Found
/// by brute force over monotone interleavings, scored by smoothness (half
/// the error of the regular 2-byte order, which splits the image at x=128).
/// (is_x, coordinate bit, address bit).
const SPLIT_2B: [(bool, u32, u32); 10] = [(true, 0, 1), (false, 0, 2), (false, 1, 3), (true, 1, 4), (false, 2, 5), (true, 2, 6), (true, 3, 7), (false, 3, 8), (true, 4, 9), (false, 4, 11)];

/// Bytes per slice of a split 2-byte array.
pub fn split_2b_slice_size(w: u32, h: u32) -> u32 {
    w.div_ceil(32) * h.div_ceil(32) * 4096
}

/// Linear elements of slice `sl` of a split 2-byte array (one mip).
pub fn untile_split_2b(src: &[u8], w: u32, h: u32, sl: u32) -> Vec<u8> {
    let base = (sl * split_2b_slice_size(w, h)) as usize;
    let bx = w.div_ceil(32);
    let mut out = vec![0u8; (w * h * 2) as usize];
    for y in 0..h {
        for x in 0..w {
            let mut a = ((y / 32) * bx + x / 32) as usize * 4096;
            for &(ax, k, bit) in &SPLIT_2B {
                let v = if ax { x } else { y };
                a |= (((v >> k) & 1) as usize) << bit;
            }
            if let Some(e) = src.get(base + a..base + a + 2) {
                let d = (y * w + x) as usize * 2;
                out[d..d + 2].copy_from_slice(e);
            }
        }
    }
    out
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

/// AMD Gfx10 3D `SW_4KB_S` intra-tile order: 0 < s < 100 is an X bit
/// (s-1), s < 0 is a Y bit (-s-1), s >= 101 is a Z bit (s-101). One 4 KiB
/// micro-tile holds `bw * bh * bd` elements; micro-tiles are row-major in X/Y
/// within a Z tile slab, and slabs are `z_tile_stride` apart. The Z axis is
/// interleaved into the tile (graphics #200), not stored as separate slices.
///
/// The table and the micro-tile extents below are derived from the
/// GPL-3.0-or-later Leonida texture tools (`tools/tex_headers.py` and
/// `tools/ttpf_mips.py`, pin `aa2988`) and from the combined SDK's own
/// `src/layout.rs`, which is not published separately. No game data is part
/// of the derivation. The 2- and 8-byte extents are also checked against real
/// allocation spans and decoded volumes; the other sizes are table values.
const VOLUME_EQ_4K: [[i32; 12]; 5] = [
    [1, 2, 101, -1, 102, -2, 3, 103, -3, 4, 104, -4],
    [0, 1, 2, 101, -1, 102, 3, -2, 103, 4, -3, 104],
    [0, 0, 1, 2, 101, -1, 102, -2, 3, 103, -3, 104],
    [0, 0, 0, 1, 101, -1, 2, 102, -2, 3, 103, -3],
    [0, 0, 0, 0, 1, -1, 101, 2, -2, 102, 3, 103],
];

/// Geometry of a 3D volume: one mip, all Z slices interleaved into the
/// micro-tiles. `untile_slice` selects one Z slice out of it.
#[derive(Debug, Clone, Copy)]
pub struct VolumeInfo {
    pub bpe: u32,
    /// Micro-tile extents in elements.
    pub bw: u32,
    pub bh: u32,
    pub bd: u32,
    /// Padded extents, a whole number of micro-tiles.
    pub padded_w: u32,
    pub padded_h: u32,
    pub padded_d: u32,
    /// Micro-tiles per row.
    pub tiles_x: u32,
    /// Bytes of one Z tile slab (`tiles_x * tiles_y` micro-tiles).
    pub z_tile_stride: u64,
    pub total: u64,
}

impl VolumeInfo {
    /// Byte address of element (x, y) of Z slice `z`, using the measured
    /// Gfx10 3D intra-tile bit order.
    fn addr(&self, x: u32, y: u32, z: u32) -> u64 {
        let eq = &VOLUME_EQ_4K[(self.bpe.trailing_zeros()) as usize];
        let z_base = (z / self.bd) as u64 * self.z_tile_stride;
        let mut z_in_tile = 0u64;
        for (bit, &s) in eq.iter().enumerate() {
            if s >= 101 {
                z_in_tile |= (((z >> (s - 101)) & 1) as u64) << bit;
            }
        }
        let mut y_in_tile = z_in_tile;
        for (bit, &s) in eq.iter().enumerate() {
            if s < 0 {
                y_in_tile |= (((y >> (-s - 1)) & 1) as u64) << bit;
            }
        }
        let mut off = z_base + ((y / self.bh) as u64 * self.tiles_x as u64 + (x / self.bw) as u64) * 4096 + y_in_tile;
        for (bit, &s) in eq.iter().enumerate() {
            if s > 0 && s < 100 {
                off |= (((x >> (s - 1)) & 1) as u64) << bit;
            }
        }
        off
    }
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub eq: &'static [(bool, u8)],
    pub bpe: u32,
    pub block_w: u32,
    pub block_h: u32,
    pub block_bytes: u64,
    pub mips: Vec<MipPlace>,
    /// Set for a 3D volume; Z is interleaved into every micro-tile.
    pub volume: Option<VolumeInfo>,
    /// Array slices (1 for a plain 2D texture).
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
        let nx = eq.iter().filter(|e| e.0).count() as u32;
        let (bw, bh) = (1u32 << nx, 1u32 << (eq.len() as u32 - nx));
        let bs = (bw * bh * bpe) as u64;
        let log2 = 63 - bs.leading_zeros();
        let max_tail = if log2 <= 11 { 1 + (1 << (log2 - 9)) } else { log2 - 4 };
        let (tw, th) = (bw / 2, bh);
        let nm = mips.max(1);
        let dims: Vec<(u32, u32)> = (0..nm).map(|i| ((w >> i).max(1), (h >> i).max(1))).collect();
        // A one-mip surface has no smaller mip to share a tail with. Placing
        // it at the fixed max-tail origin reads the unused part of the tile
        // instead of offset 0 (graphics #151: 16x8 RGBA8 decodes as zeros).
        let first = if nm == 1 {
            nm as usize
        } else {
            dims.iter()
                .enumerate()
                .position(|(i, &(mw, mh))| mw <= tw && mh <= th && nm - i as u32 <= max_tail)
                .unwrap_or(nm as usize)
        };
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
        Some(Layout { eq, bpe, block_w: bw, block_h: bh, block_bytes: bs, mips: place, volume: None, slices: 1, slice_major: false, slice_bytes: off, base: 0, total: off })
    }

    /// Layout of one mip of a 3D volume (`SW_4KB_S`, uncompressed). The Z axis
    /// is part of the micro-tile address, so the whole volume is one region and
    /// `mip_span` covers all of it. `None` for another swizzle or element size.
    pub fn new_volume(sw: Swizzle, w: u32, h: u32, d: u32, bpe: u32) -> Option<Layout> {
        if sw != Swizzle::S4K {
            return None;
        }
        let eq = equation(sw, bpe)?;
        // (bw, bh, bd) measured for 1/2/4/8/16-byte elements; every tile is
        // 4096 bytes and its Z extent covers the full padded depth when small.
        let (bw, bh, bd) = match bpe {
            1 => (16, 16, 16),
            2 => (16, 8, 16),
            4 => (8, 8, 16),
            8 => (8, 8, 8),
            16 => (8, 4, 8),
            _ => return None,
        };
        let (w, h, d) = (w.max(1), h.max(1), d.max(1));
        let padded_w = w.div_ceil(bw) * bw;
        let padded_h = h.div_ceil(bh) * bh;
        let padded_d = d.div_ceil(bd) * bd;
        let tiles_x = padded_w / bw;
        let z_tile_stride = tiles_x as u64 * (padded_h / bh) as u64 * 4096;
        let total = (padded_d / bd) as u64 * z_tile_stride;
        let volume = VolumeInfo { bpe, bw, bh, bd, padded_w, padded_h, padded_d, tiles_x, z_tile_stride, total };
        Some(Layout {
            eq,
            bpe,
            block_w: bw,
            block_h: bh,
            block_bytes: 4096,
            mips: vec![MipPlace { w, h, offset: 0, tail: None }],
            volume: Some(volume),
            slices: d,
            slice_major: false,
            slice_bytes: total,
            base: 0,
            total,
        })
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

    fn inblock(&self, x: u32, y: u32) -> u64 {
        let mut a = 0u64;
        for (k, &(isx, b)) in self.eq.iter().enumerate() {
            let v = if isx { x } else { y };
            a |= (((v >> b) & 1) as u64) << k;
        }
        a
    }

    /// Byte range [start, end) that slice 0 of mip `m` occupies (the whole
    /// tail block for tail mips). A volume is Z-interleaved, so its only mip
    /// spans the entire allocation.
    pub fn mip_span(&self, m: usize) -> (u64, u64) {
        if let Some(v) = &self.volume {
            return (0, v.total);
        }
        let a = self.level_start(m, 0);
        (a, a + self.level_bytes(m))
    }

    /// Byte address of element (x, y) of slice `s` of mip `m`.
    pub fn addr(&self, m: usize, s: u32, x: u32, y: u32) -> u64 {
        if let Some(v) = &self.volume {
            return v.addr(x, y, s);
        }
        let p = &self.mips[m];
        let base = self.level_start(m, s);
        match p.tail {
            Some((cx, cy)) => base + self.inblock(x + cx, y + cy) * self.bpe as u64,
            None => {
                let pitch = p.w.div_ceil(self.block_w) as u64;
                let blk = (y / self.block_h) as u64 * pitch + (x / self.block_w) as u64;
                base + blk * self.block_bytes + self.inblock(x % self.block_w, y % self.block_h) * self.bpe as u64
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
        if m >= self.mips.len() || sl >= self.slices {
            return Vec::new();
        }
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

    #[test]
    fn single_mip_is_standalone_not_tail_origin() {
        // graphics #151: 16x8 RGBA8, one mip. The only mip starts at offset 0
        // instead of the fixed max-tail origin (16, 0), which holds zeros.
        let l = Layout::new(Swizzle::S4K, 16, 8, 1, 4).unwrap();
        assert_eq!(l.total, 4096);
        assert_eq!(l.mips.len(), 1);
        assert_eq!(l.mips[0].offset, 0);
        assert_eq!(l.mips[0].tail, None);
    }

    #[test]
    fn volume_is_one_z_interleaved_region() {
        // graphics #200: 32x64x32 R16G16B16A16_FLOAT (8-byte elements).
        let l = Layout::new_volume(Swizzle::S4K, 32, 64, 32, 8).unwrap();
        assert_eq!(l.total, 524288);
        assert_eq!(l.slices, 32);
        assert!(l.volume.is_some());
        assert_eq!(l.mip_span(0), (0, 524288));
        // Measured intra-tile order (bpe 8): x bit 0 at address bit 3,
        // Z bit 0 at bit 4, Y bit 0 at bit 5. Z is part of the tile.
        assert_eq!(l.addr(0, 0, 0, 0), 0);
        assert_eq!(l.addr(0, 0, 1, 0), 8);
        assert_eq!(l.addr(0, 1, 0, 0), 16);
        assert_eq!(l.addr(0, 0, 0, 1), 32);
        // Every eight Z slices start a new tile slab.
        assert_eq!(l.addr(0, 8, 0, 0), 131072);
        assert_eq!(l.addr(0, 31, 31, 31), 458744);
    }

    #[test]
    fn volume_two_byte_layout_matches_measurable_store_spacing() {
        // 256x32x8 R16_FLOAT volume: 16x8x16 micro-tiles, 16 tiles per row,
        // four rows and one 16-deep Z slab = 262144 bytes.
        let l = Layout::new_volume(Swizzle::S4K, 256, 32, 8, 2).unwrap();
        assert_eq!(l.total, 262144);
        assert_eq!(l.slices, 8);
        let v = l.volume.unwrap();
        assert_eq!((v.bw, v.bh, v.bd), (16, 8, 16));
        assert_eq!((v.padded_w, v.padded_h, v.padded_d), (256, 32, 16));
        assert_eq!(v.z_tile_stride, 262144);
    }

    #[test]
    fn volume_rejects_unsupported_element_sizes() {
        assert!(Layout::new_volume(Swizzle::S4K, 32, 32, 32, 3).is_none());
        assert!(Layout::new_volume(Swizzle::D64K, 32, 32, 32, 8).is_none());
    }

    #[test]
    fn volume_untile_guards_mip_and_slice_range() {
        let l = Layout::new_volume(Swizzle::S4K, 32, 32, 8, 8).unwrap();
        let bytes = vec![0u8; l.total as usize];
        assert!(l.untile_slice(&bytes, 0, 8).is_empty());
        assert!(l.untile_slice(&bytes, 1, 0).is_empty());
        // A truncated source stays zero-filled instead of panicking.
        assert_eq!(l.untile_slice(&bytes[..16], 0, 0), vec![0u8; 32 * 32 * 8]);
    }
}
