//! grcTexture parsing, Xbox Series (RDNA2) deswizzling and PNG / DDS export.
//!
//! Texture struct (0x70 bytes, found in TXD dictionaries and drawables):
//! ```text
//! +0x08 u32  size field (bytes / 16 for most textures)
//! +0x0C u16  bytes per element hint (0x08 = BC1/BC4, 0x10 = BC2/3/5/6/7)
//! +0x0E u16  8
//! +0x10 u32  flags (0x980)
//! +0x18 ptr  pixel data (graphics page)
//! +0x30 ptr  name block (C string at +8)
//! +0x38 u16  width, +0x3A u16 height, +0x3C u16 depth/array size
//! +0x3E u8   dimension: 1 = 2D (or array), 2 = cube, 3 = 3D (volume)
//! +0x3F u8   DXGI_FORMAT
//! +0x42 u8   mip count
//! ```
//! Pixel layout: mips stored smallest-first; mip0 starts at
//! align_up(sum(size of mips 1..n), 4096). Each mip is tiled in 16x16-element
//! tiles (row-major), with a format-size dependent bit pattern inside a tile.
use crate::resource::Resource;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TexInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    /// Header dimension byte: 1 = 2D or 2D array, 2 = cube, 3 = volume
    /// (0 when unknown).
    #[serde(default)]
    pub dimension: u8,
    pub mips: u32,
    pub dxgi_format: u32,
    pub format: String,
    /// Offset of the texture struct in the system page.
    pub struct_off: u64,
    /// Offset of the pixel data in the graphics pages.
    pub data_off: u64,
    pub size_field: u32,
    /// "verified", "probable" or "unverified" for the deswizzle of this format.
    pub swizzle: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fmt {
    Bc1,
    Bc2,
    Bc3,
    Bc4,
    Bc5,
    Bc6u,
    Bc6s,
    Bc7,
    Rgba8,
    Bgra8,
    R8,
    /// A8_UNORM: white RGB for previewing the stored alpha mask.
    A8,
    Rg8,
    /// R16_FLOAT, shown as grey clamped to 0..1.
    R16f,
    /// R16_UNORM (water and sea-level height maps), shown as grey, high byte.
    R16,
    /// R16G16B16A16_UNORM.
    Rgba16,
    /// R16G16B16A16_FLOAT, clamped to 0..1.
    Rgba16f,
    /// R32_FLOAT, shown as grey clamped to 0..1.
    R32f,
    /// R16G16_FLOAT: red and green clamped to 0..1, blue 0.
    Rg16f,
    /// R10G10B10A2_UNORM: the top 8 of each 10-bit channel; 2-bit alpha.
    Rgb10a2,
    Unknown,
}

impl Fmt {
    /// Formats that store floats (see `to_floats`).
    pub fn is_float(self) -> bool {
        matches!(self, Fmt::R16f | Fmt::Rg16f | Fmt::Rgba16f | Fmt::R32f)
    }

    pub fn from_dxgi(f: u32) -> Fmt {
        match f {
            70..=72 => Fmt::Bc1,
            73..=75 => Fmt::Bc2,
            76..=78 => Fmt::Bc3,
            79..=81 => Fmt::Bc4,
            82..=84 => Fmt::Bc5,
            94 | 95 => Fmt::Bc6u,
            96 => Fmt::Bc6s,
            97..=99 => Fmt::Bc7,
            27..=29 | 30 | 31 | 32 => Fmt::Rgba8,
            87 | 88 | 90 | 91 => Fmt::Bgra8,
            60 | 61 | 62 | 63 | 64 => Fmt::R8,
            65 => Fmt::A8,
            48..=52 => Fmt::Rg8,
            54 => Fmt::R16f,
            53 | 56 | 57 => Fmt::R16,
            11 => Fmt::Rgba16,
            10 => Fmt::Rgba16f,
            39..=41 => Fmt::R32f,
            33 | 34 => Fmt::Rg16f,
            23..=25 => Fmt::Rgb10a2,
            _ => Fmt::Unknown,
        }
    }
    pub fn block_dim(&self) -> u32 {
        match self {
            Fmt::Rgba8 | Fmt::Bgra8 | Fmt::R8 | Fmt::A8 | Fmt::Rg8 | Fmt::R16f | Fmt::R16 | Fmt::Rgba16 | Fmt::Rgba16f | Fmt::R32f | Fmt::Rg16f | Fmt::Rgb10a2 | Fmt::Unknown => 1,
            _ => 4,
        }
    }
    pub fn bytes_per_elem(&self) -> u32 {
        match self {
            Fmt::Bc1 | Fmt::Bc4 => 8,
            Fmt::Bc2 | Fmt::Bc3 | Fmt::Bc5 | Fmt::Bc6u | Fmt::Bc6s | Fmt::Bc7 => 16,
            Fmt::Rgba8 | Fmt::Bgra8 | Fmt::R32f | Fmt::Rg16f | Fmt::Rgb10a2 => 4,
            Fmt::Rg8 | Fmt::R16f | Fmt::R16 => 2,
            Fmt::Rgba16 | Fmt::Rgba16f => 8,
            Fmt::R8 | Fmt::A8 => 1,
            Fmt::Unknown => 0,
        }
    }
    pub fn swizzle_status(&self) -> &'static str {
        match self {
            Fmt::Bc1 | Fmt::Bc4 => "verified",
            Fmt::Bc2 | Fmt::Bc3 | Fmt::Bc5 | Fmt::Bc6u | Fmt::Bc6s | Fmt::Bc7 => "probable",
            _ => "unverified",
        }
    }
}

pub fn dxgi_name(f: u32) -> String {
    match f {
        2 => "R32G32B32A32_FLOAT".into(),
        10 => "R16G16B16A16_FLOAT".into(),
        24 => "R10G10B10A2_UNORM".into(),
        26 => "R11G11B10_FLOAT".into(),
        28 => "R8G8B8A8_UNORM".into(),
        29 => "R8G8B8A8_UNORM_SRGB".into(),
        34 => "R16G16_FLOAT".into(),
        41 => "R32_FLOAT".into(),
        49 => "R8G8_UNORM".into(),
        54 => "R16_FLOAT".into(),
        56 => "R16_UNORM".into(),
        61 => "R8_UNORM".into(),
        71 => "BC1_UNORM".into(),
        72 => "BC1_UNORM_SRGB".into(),
        74 => "BC2_UNORM".into(),
        75 => "BC2_UNORM_SRGB".into(),
        77 => "BC3_UNORM".into(),
        78 => "BC3_UNORM_SRGB".into(),
        80 => "BC4_UNORM".into(),
        81 => "BC4_SNORM".into(),
        83 => "BC5_UNORM".into(),
        84 => "BC5_SNORM".into(),
        87 => "B8G8R8A8_UNORM".into(),
        91 => "B8G8R8A8_UNORM_SRGB".into(),
        95 => "BC6H_UF16".into(),
        96 => "BC6H_SF16".into(),
        98 => "BC7_UNORM".into(),
        99 => "BC7_UNORM_SRGB".into(),
        _ => format!("DXGI_{f}"),
    }
}

fn plausible(r: &Resource, p: usize) -> Option<TexInfo> {
    if p + 0x48 > r.sys.len() {
        return None;
    }
    if r.u16(p + 0x0E) != 8 {
        return None;
    }
    let data = r.u64(p + 0x18);
    if data >> 28 != 6 {
        return None;
    }
    let w = r.u16(p + 0x38) as u32;
    let h = r.u16(p + 0x3A) as u32;
    let depth = r.u16(p + 0x3C) as u32;
    let fmt = r.sys[p + 0x3F] as u32;
    let mips = r.sys[p + 0x42] as u32;
    if w == 0 || h == 0 || w > 16384 || h > 16384 || depth == 0 || depth > 256 || mips == 0 || mips > 15 || fmt == 0 || fmt > 132 {
        return None;
    }
    let name = r.sys_off(r.u64(p + 0x30)).and_then(|o| r.cstr(o + 8)).unwrap_or_default();
    Some(TexInfo {
        name,
        width: w,
        height: h,
        depth,
        dimension: r.sys[p + 0x3E],
        mips,
        dxgi_format: fmt,
        format: dxgi_name(fmt),
        struct_off: p as u64,
        data_off: data & 0x0FFF_FFFF,
        size_field: r.u32(p + 8),
        swizzle: Fmt::from_dxgi(fmt).swizzle_status().into(),
    })
}

/// Find all texture structs in a resource's system page.
pub fn find_textures(r: &Resource) -> Vec<TexInfo> {
    let mut v = Vec::new();
    let mut p = 0;
    while p + 0x48 <= r.sys.len() {
        if let Some(t) = plausible(r, p) {
            v.push(t);
            p += 0x70;
        } else {
            p += 8;
        }
    }
    v
}

/// Store key (hash64) of a texture embedded in a resource: the struct's
/// +0x30 pointer leads to a record whose +8 is the hash that shader texture
/// references use (the same key the ttpf stores use).
pub fn embedded_hash(r: &Resource, t: &TexInfo) -> Option<u64> {
    let o = r.sys_off(r.u64(t.struct_off as usize + 0x30))?;
    (o + 16 <= r.sys.len()).then(|| r.u64(o + 8)).filter(|&h| h != 0)
}

/// Row layout of a tint palette `width` texels wide: (entries per row, rows).
/// The width is a power of two `p` plus `k` extra texels; a row holds
/// `256 >> k` entries (at most `p`). Placements pick the row with their
/// tint value (`Entity::tint_row`). Derived from our own data: the largest
/// tint value placed with each palette width is rows - 1, and later rows
/// start on these boundaries.
pub fn tint_layout(width: u32) -> (u32, u32) {
    if width == 0 {
        return (0, 0);
    }
    let p = 1u32 << (31 - width.leading_zeros());
    let k = width - p;
    let len = if k >= 8 { 1 } else { (256u32 >> k).min(p) };
    (len, p / len)
}

/// RGBA8 pixels of mip 0 of an embedded texture. One-row textures (the
/// per-model tint palettes) are stored linear.
pub fn embedded_rgba(r: &Resource, t: &TexInfo) -> Result<Vec<u8>> {
    let (fmt, lin) = mip0_linear(r, t, t.height == 1)?;
    to_rgba(fmt, &lin, t.width, t.height)
}

/// Dictionary name of a TXD (string following the page map), if present.
pub fn dict_name(r: &Resource) -> Option<String> {
    let pm = r.sys_off(r.u64(8))?;
    // page map is followed by NUL padding then strings
    let mut o = pm + 16;
    while o < r.sys.len() && o < pm + 0x200 {
        if r.sys[o] != 0 {
            return r.cstr(o);
        }
        o += 1;
    }
    None
}

pub fn mip_bytes(fmt: Fmt, w: u32, h: u32, m: u32) -> u64 {
    let bd = fmt.block_dim();
    let mw = (w >> m).max(1);
    let mh = (h >> m).max(1);
    let bw = mw.div_ceil(bd) as u64;
    let bh = mh.div_ceil(bd) as u64;
    bw * bh * fmt.bytes_per_elem() as u64
}

/// In-tile bit pattern (element index bit k -> coordinate bit), 16x16 tiles.
pub fn pattern(bpe: u32) -> [(bool, u32); 8] {
    // (is_x, bit)
    match bpe {
        8 => [(true, 0), (false, 0), (false, 1), (true, 1), (true, 2), (false, 2), (true, 3), (false, 3)],
        16 => [(false, 0), (false, 1), (true, 0), (true, 1), (false, 2), (true, 2), (false, 3), (true, 3)],
        _ => [(true, 0), (false, 0), (true, 1), (false, 1), (true, 2), (false, 2), (true, 3), (false, 3)],
    }
}

/// Convert a tiled mip (16x16-element tiles, row-major, padded pitch) to linear.
pub fn deswizzle(src: &[u8], ew: u32, eh: u32, bpe: u32) -> Vec<u8> {
    let bpe_us = bpe as usize;
    let tiles_x = ew.div_ceil(16);
    let pat = pattern(bpe);
    let mut lut = [(0u32, 0u32); 256];
    for (e, slot) in lut.iter_mut().enumerate() {
        let (mut x, mut y) = (0u32, 0u32);
        for (k, &(isx, bit)) in pat.iter().enumerate() {
            if e >> k & 1 == 1 {
                if isx {
                    x |= 1 << bit;
                } else {
                    y |= 1 << bit;
                }
            }
        }
        *slot = (x, y);
    }
    let mut out = vec![0u8; (ew * eh) as usize * bpe_us];
    let tiles_y = eh.div_ceil(16);
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            let tile_base = ((ty * tiles_x + tx) * 256) as usize * bpe_us;
            for (e, &(lx, ly)) in lut.iter().enumerate() {
                let x = tx * 16 + lx;
                let y = ty * 16 + ly;
                if x >= ew || y >= eh {
                    continue;
                }
                let s = tile_base + e * bpe_us;
                if s + bpe_us > src.len() {
                    continue;
                }
                let d = ((y * ew + x) as usize) * bpe_us;
                out[d..d + bpe_us].copy_from_slice(&src[s..s + bpe_us]);
            }
        }
    }
    out
}

/// Gfx10 SW_4KB_S layout of an uncompressed texture: every uncompressed
/// texture in a texture dictionary is stored this way (2D, arrays and, with
/// the 3D pattern, single-mip volumes), not in the 16 x 16-element tiles of
/// [`deswizzle`]. `None` for block-compressed formats and for volumes with a
/// mip chain.
///
/// * Single-mip 2/4-byte textures (R16 height maps, RGBA8 palettes and
///   profiles, BGRA8 tiles): by neighbour smoothness over every such texture
///   of 32 x 32 or more in textures.rpf, levels.rpf, common.rpf and the
///   packs, SW_4KB_S is smoothest on 1,051 of 1,064; the rest are noise,
///   volumes (read as 2D before the dimension byte was used) and flat tiles.
///   Water height and shore tiles placed with it continue across 1,396 of
///   1,400 and 268 of 271 tile edges.
/// * Single-mip 1-byte textures: 4 x 4 A8 blanks read as one value and the
///   4 x 4 R8 dither offsets as 16 distinct levels 16 apart (rows 16 bytes
///   apart, the rest padding); a 512 x 512 R8 light-animation table reads as
///   clean per-row curves (block-scrambled with 16 x 16 tiles).
/// * Single-mip 8-byte textures: identical to 16 x 16 tiles up to 16 rows;
///   the 256 x 16 RGBA16 float identity LUT decodes exactly.
/// * Mip chains (RGBA8, R8, A8, R32 float): mip k+1 matches mip k averaged
///   2 x 2 at 41 to 67 dB on mips 0 to 1 (11 to 25 dB with 16 x 16 tiles),
///   and on down through the packed tail (40 to 68 dB on R8 and A8; exact on
///   the R32 float glow).
/// * Volumes (dimension byte 3): the 3D pattern of [`crate::gfx10::Layout::new_3d`].
///   The 33 x 33 x 33 RGBA16 float identity LUT decodes exactly.
fn uncompressed_layout(fmt: Fmt, t: &TexInfo) -> Option<crate::gfx10::Layout> {
    use crate::gfx10::{Layout, Swizzle};
    let bpe = fmt.bytes_per_elem();
    if fmt.block_dim() != 1 || bpe == 0 {
        return None;
    }
    if t.dimension == 3 && t.depth > 1 {
        if t.mips != 1 {
            return None;
        }
        if let Some(l) = Layout::new_3d(Swizzle::S4K, t.width, t.height, t.depth, bpe) {
            return Some(l);
        }
    }
    Layout::new(Swizzle::S4K, t.width, t.height, t.mips, bpe).map(|l| l.with_slices(t.depth))
}

fn untile_mip(r: &Resource, t: &TexInfo, l: &crate::gfx10::Layout, m: usize) -> Result<Vec<u8>> {
    let start = t.data_off as usize;
    if start >= r.gfx.len() {
        return Err(anyhow!("texture data out of range"));
    }
    let end = (start + l.total.max(l.block_bytes) as usize).min(r.gfx.len());
    Ok(l.untile(&r.gfx[start..end], m))
}

/// Linear (deswizzled) mip-0 bytes of slice 0, in the texture's own format.
pub fn mip0_linear(r: &Resource, t: &TexInfo, linear_input: bool) -> Result<(Fmt, Vec<u8>)> {
    let fmt = Fmt::from_dxgi(t.dxgi_format);
    if fmt == Fmt::Unknown {
        return Err(anyhow!("unsupported format {}", t.format));
    }
    let mut tail = 0u64;
    for m in 1..t.mips {
        tail += mip_bytes(fmt, t.width, t.height, m);
    }
    let mip0_off = if t.mips > 1 { (tail + 4095) & !4095 } else { 0 };
    let bd = fmt.block_dim();
    let ew = t.width.div_ceil(bd);
    let eh = t.height.div_ceil(bd);
    let bpe = fmt.bytes_per_elem();
    // Small textures with mips (e.g. 32x32 BC3 HUD icons) keep their whole
    // chain in one 4 KiB mip-tail block, mip 0 included: there is no
    // separate mip-0 page after the tail. Untile mip 0 from the tail block
    // (Gfx10 SW_4KB_S tail layout).
    if !linear_input && t.mips > 1 {
        if let Some(l) = crate::gfx10::Layout::new(crate::gfx10::Swizzle::S4K, ew, eh, t.mips, bpe) {
            if l.mips[0].tail.is_some() {
                let start = t.data_off as usize;
                if start >= r.gfx.len() {
                    return Err(anyhow!("texture data out of range"));
                }
                let end = (start + l.block_bytes as usize).min(r.gfx.len());
                return Ok((fmt, l.untile(&r.gfx[start..end], 0)));
            }
        }
    }
    if !linear_input {
        if let Some(l) = uncompressed_layout(fmt, t) {
            return untile_mip(r, t, &l, 0).map(|d| (fmt, d));
        }
    }
    let padded = (ew.div_ceil(16) * 16 * eh.div_ceil(16) * 16 * bpe) as u64;
    let start = (t.data_off + mip0_off) as usize;
    let len = if linear_input { (ew * eh * bpe) as usize } else { padded as usize };
    let end = (start + len).min(r.gfx.len());
    if start >= r.gfx.len() {
        return Err(anyhow!("texture data out of range"));
    }
    let src = &r.gfx[start..end];
    let lin = if linear_input { src.to_vec() } else { deswizzle(src, ew, eh, bpe) };
    Ok((fmt, lin))
}

/// Byte offset (from the texture data pointer) of mip `m`.
/// Mips are stored smallest first; each starts at the 4 KiB-aligned
/// sum of the exact sizes of all smaller mips (verified on mips 0 and 1
/// by comparing against a downscaled mip 0).
pub fn mip_offset(fmt: Fmt, t: &TexInfo, m: u32) -> u64 {
    let tail: u64 = (m + 1..t.mips).map(|k| mip_bytes(fmt, t.width, t.height, k)).sum();
    if m + 1 >= t.mips { 0 } else { (tail + 4095) & !4095 }
}

/// Number of leading mips that are safe to export: levels whose element
/// grid is at least 32x32 (128 px for BC formats). Smaller mips sit in a
/// packed tail whose layout is not solved yet for block-compressed formats;
/// uncompressed mip chains are untiled whole (see `uncompressed_layout`).
pub fn exportable_mips(t: &TexInfo) -> u32 {
    let fmt = Fmt::from_dxgi(t.dxgi_format);
    if t.mips > 1 && uncompressed_layout(fmt, t).is_some() {
        return t.mips;
    }
    let bd = fmt.block_dim().max(1);
    let mut n = 1;
    while n < t.mips {
        let (w, h) = ((t.width >> n).max(1), (t.height >> n).max(1));
        if w.div_ceil(bd) < 32 || h.div_ceil(bd) < 32 {
            break;
        }
        n += 1;
    }
    n
}

/// Linear (untiled) data of one mip of slice 0. Uncompressed mip chains use
/// the Gfx10 SW_4KB_S layout, packed tail included; other mips are untiled
/// as 16x16-element tiles.
pub fn mip_linear(r: &Resource, t: &TexInfo, m: u32) -> Result<(Fmt, Vec<u8>, u32, u32)> {
    let fmt = Fmt::from_dxgi(t.dxgi_format);
    if fmt == Fmt::Unknown {
        return Err(anyhow!("unsupported format {}", t.format));
    }
    if m == 0 {
        let (f, d) = mip0_linear(r, t, false)?;
        return Ok((f, d, t.width, t.height));
    }
    if let Some(l) = uncompressed_layout(fmt, t) {
        let p = l.mips.get(m as usize).ok_or_else(|| anyhow!("mip {m} out of range"))?;
        let (w, h) = (p.w, p.h);
        return Ok((fmt, untile_mip(r, t, &l, m as usize)?, w, h));
    }
    let (mw, mh) = ((t.width >> m).max(1), (t.height >> m).max(1));
    let bd = fmt.block_dim();
    let (ew, eh) = (mw.div_ceil(bd), mh.div_ceil(bd));
    let bpe = fmt.bytes_per_elem();
    let start = (t.data_off + mip_offset(fmt, t, m)) as usize;
    let padded = (ew.div_ceil(16) * 16 * eh.div_ceil(16) * 16 * bpe) as usize;
    let end = (start + padded).min(r.gfx.len());
    let src = r.gfx.get(start..end).ok_or_else(|| anyhow!("mip {m} out of range"))?;
    let lin = deswizzle(src, ew, eh, bpe);
    Ok((fmt, lin, mw, mh))
}

/// Expand a little-endian RGB565 endpoint, replicating the high bits into the
/// low ones as the BC1 specification describes.
fn rgb565(v: u16) -> (u8, u8, u8) {
    (
        ((v >> 8) & 0xF8) as u8 | (v >> 13) as u8,
        ((v >> 3) & 0xFC) as u8 | ((v >> 9) & 0x03) as u8,
        (v << 3) as u8 | ((v >> 2) & 0x07) as u8,
    )
}

/// BC2 and BC3 always use four colours. texture2ddecoder decodes their colour
/// half with the BC1 block decoder, which switches to three colours when
/// `c0 <= c1`; replace only those blocks' index-2 and index-3 RGB. Alpha is
/// already correct and is preserved.
fn repair_bc23_colour(data: &[u8], w: usize, h: usize, img: &mut [u32]) {
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    let mix = |a: u8, b: u8, wa: u16, wb: u16| ((a as u16 * wa + b as u16 * wb) / 3) as u8;
    for by in 0..bh {
        for bx in 0..bw {
            let off = (by * bw + bx) * 16;
            let block = &data[off + 8..off + 16];
            let c0 = u16::from_le_bytes([block[0], block[1]]);
            let c1 = u16::from_le_bytes([block[2], block[3]]);
            if c0 > c1 {
                continue;
            }
            let (r0, g0, b0) = rgb565(c0);
            let (r1, g1, b1) = rgb565(c1);
            let indices = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
            for y in 0..4 {
                let py = by * 4 + y;
                if py >= h {
                    break;
                }
                for x in 0..4 {
                    let px = bx * 4 + x;
                    if px >= w {
                        break;
                    }
                    let (wa, wb) = match (indices >> (2 * (y * 4 + x))) & 3 {
                        2 => (2, 1),
                        3 => (1, 2),
                        _ => continue,
                    };
                    let rgb = (mix(r0, r1, wa, wb) as u32) << 16
                        | (mix(g0, g1, wa, wb) as u32) << 8
                        | mix(b0, b1, wa, wb) as u32;
                    let pixel = &mut img[py * w + px];
                    *pixel = (*pixel & 0xFF00_0000) | rgb;
                }
            }
        }
    }
}

/// Block rows per band for a `bw` x `bh` block image, or all of them when
/// the image is too small to gain: handing a few milliseconds of work to
/// other cores costs more than it saves when they are busy. BC6H and BC7
/// decode about five times slower than the others, so they split sooner
/// (from 512 x 512 pixels; the others from 2048 x 2048). About four bands
/// per thread, so that slow and fast bands even out.
fn band_rows(fmt: Fmt, bw: usize, bh: usize) -> usize {
    let (min_blocks, band_blocks): (usize, usize) = match fmt {
        Fmt::Bc6u | Fmt::Bc6s | Fmt::Bc7 => (1 << 14, 1 << 12),
        _ => (1 << 18, 1 << 14),
    };
    if bw * bh < min_blocks {
        return bh.max(1);
    }
    let bands = rayon::current_num_threads() * 4;
    bh.div_ceil(bands).max(band_blocks.div_ceil(bw))
}

/// Decode a block-compressed image (BC1 to BC7, blocks in row order) to
/// RGBA8 in bands of `rows` block rows, each on its own core. Blocks do not
/// depend on each other, so the bands give the same bytes as one pass.
fn bc_to_rgba(fmt: Fmt, data: &[u8], w: usize, h: usize, rows: usize) -> Result<Vec<u8>, &'static str> {
    use rayon::prelude::*;
    use texture2ddecoder as td;
    let bw = w.div_ceil(4);
    let bpe = fmt.bytes_per_elem() as usize;
    // One band: block rows starting at `data`, `bh` pixel rows into `out`.
    let band = |data: &[u8], bh: usize, out: &mut [u8]| -> Result<(), &'static str> {
        let mut img = vec![0u32; w * bh];
        match fmt {
            Fmt::Bc1 => td::decode_bc1a(data, w, bh, &mut img)?,
            Fmt::Bc2 => {
                td::decode_bc2(data, w, bh, &mut img)?;
                repair_bc23_colour(data, w, bh, &mut img);
            }
            Fmt::Bc3 => {
                td::decode_bc3(data, w, bh, &mut img)?;
                repair_bc23_colour(data, w, bh, &mut img);
            }
            Fmt::Bc4 => td::decode_bc4(data, w, bh, &mut img)?,
            Fmt::Bc5 => {
                td::decode_bc5(data, w, bh, &mut img)?;
                reconstruct_bc5_z(&mut img);
            }
            Fmt::Bc6u => crate::bc6h::decode_image(data, w, bh, &mut img, false)?,
            Fmt::Bc6s => crate::bc6h::decode_image(data, w, bh, &mut img, true)?,
            Fmt::Bc7 => td::decode_bc7(data, w, bh, &mut img)?,
            _ => return Err("unsupported"),
        }
        for (o, px) in out.chunks_exact_mut(4).zip(&img) {
            // texture2ddecoder packs as 0xAARRGGBB
            o.copy_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, *px as u8, (px >> 24) as u8]);
        }
        Ok(())
    };
    let mut out = vec![0u8; w * h * 4];
    if rows * 4 >= h || w == 0 {
        band(data, h, &mut out)?;
    } else {
        out.par_chunks_mut(rows * 4 * w * 4)
            .enumerate()
            .try_for_each(|(i, o)| band(&data[i * rows * bw * bpe..], o.len() / (w * 4), o))?;
    }
    Ok(out)
}

/// Decode linear mip0 to RGBA8.
pub fn to_rgba(fmt: Fmt, data: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
    let (wu, hu) = (w as usize, h as usize);
    let need_blocks = (w.div_ceil(4) * h.div_ceil(4)) as usize;
    let mut buf;
    let data = if fmt.block_dim() == 4 && data.len() < need_blocks * fmt.bytes_per_elem() as usize {
        buf = data.to_vec();
        buf.resize(need_blocks * fmt.bytes_per_elem() as usize, 0);
        &buf[..]
    } else {
        data
    };
    match fmt {
        Fmt::Bc1 | Fmt::Bc2 | Fmt::Bc3 | Fmt::Bc4 | Fmt::Bc5 | Fmt::Bc6u | Fmt::Bc6s | Fmt::Bc7 => {
            let rows = band_rows(fmt, wu.div_ceil(4), hu.div_ceil(4));
            bc_to_rgba(fmt, data, wu, hu, rows).map_err(|e| anyhow!("decode: {e}"))
        }
        Fmt::Rgba8 | Fmt::Bgra8 | Fmt::R8 | Fmt::A8 | Fmt::Rg8 | Fmt::R16f | Fmt::R16 | Fmt::Rgba16 | Fmt::Rgba16f | Fmt::R32f | Fmt::Rg16f | Fmt::Rgb10a2 => {
            let bpe = fmt.bytes_per_elem() as usize;
            let mut out = vec![255u8; wu * hu * 4];
            for i in 0..wu * hu {
                let s = &data.get(i * bpe..i * bpe + bpe).unwrap_or(&[0; 8][..bpe]);
                let u16at = |k: usize| u16::from_le_bytes([s[2 * k], s[2 * k + 1]]);
                let unit = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                let o = &mut out[i * 4..i * 4 + 4];
                match fmt {
                    Fmt::Rgba8 => o.copy_from_slice(s),
                    Fmt::Bgra8 => {
                        o[0] = s[2];
                        o[1] = s[1];
                        o[2] = s[0];
                        o[3] = s[3];
                    }
                    Fmt::A8 => o[3] = s[0],
                    Fmt::R8 => {
                        o[0] = s[0];
                        o[1] = s[0];
                        o[2] = s[0];
                    }
                    Fmt::R16f => {
                        let v = unit(f16(u16at(0)));
                        o[..3].fill(v);
                    }
                    Fmt::R16 => o[..3].fill((u16at(0) >> 8) as u8),
                    Fmt::Rgba16 => (0..4).for_each(|k| o[k] = (u16at(k) >> 8) as u8),
                    Fmt::Rgba16f => (0..4).for_each(|k| o[k] = unit(f16(u16at(k)))),
                    Fmt::R32f => o[..3].fill(unit(f32_or_zero(u32at(s)))),
                    Fmt::Rg16f => {
                        o[0] = unit(f16(u16at(0)));
                        o[1] = unit(f16(u16at(1)));
                        o[2] = 0;
                    }
                    Fmt::Rgb10a2 => {
                        let v = u32at(s);
                        (0..3).for_each(|k| o[k] = (v >> (10 * k + 2)) as u8);
                        o[3] = (v >> 30) as u8 * 85;
                    }
                    _ => {
                        o[0] = s[0];
                        o[1] = s[1];
                        o[2] = 0;
                    }
                }
            }
            Ok(out)
        }
        Fmt::Unknown => Err(anyhow!("decode: unsupported")),
    }
}

/// Reconstruct the tangent-space Z component into the blue channel of a decoded
/// BC5 image (`0xAARRGGBB`). `texture2ddecoder::decode_bc5` leaves B = 0, which
/// unpacks as z = -1 in glTF viewers and prevents `normal_like` detection.
/// R and G (DirectX green) are kept untouched so shaders that flip G stay consistent.
fn reconstruct_bc5_z(img: &mut [u32]) {
    for px in img.iter_mut() {
        let r = ((*px >> 16) & 0xFF) as f32 * (2.0 / 255.0) - 1.0;
        let g = ((*px >> 8) & 0xFF) as f32 * (2.0 / 255.0) - 1.0;
        let z = (1.0 - r * r - g * g).max(0.0).sqrt();
        let b = ((z * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
        *px = (*px & 0xFFFF_FF00) | b;
    }
}

pub fn save_png(path: &Path, rgba: &[u8], w: u32, h: u32) -> Result<()> {
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec()).ok_or_else(|| anyhow!("bad image size"))?;
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::from_path(path)?)?;
    crate::atomic::write(path, buf.into_inner())?;
    Ok(())
}

/// Write a DX10-header DDS with one mip (linear layout).
/// Write a DDS with a full mip chain (`mips` = linear data per level, mip 0 first).
pub fn save_dds_mips(path: &Path, t: &TexInfo, fmt: Fmt, mips: &[Vec<u8>]) -> Result<()> {
    let data: Vec<u8> = mips.concat();
    save_dds_n(path, t, fmt, &data, mips.len() as u32, mips.first().map(|m| m.len()).unwrap_or(0))
}

pub fn save_dds(path: &Path, t: &TexInfo, fmt: Fmt, data: &[u8]) -> Result<()> {
    save_dds_n(path, t, fmt, data, 1, data.len())
}

fn save_dds_n(path: &Path, t: &TexInfo, fmt: Fmt, data: &[u8], nmips: u32, mip0_len: usize) -> Result<()> {
    let mut h = Vec::with_capacity(148 + data.len());
    let w32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    h.extend_from_slice(b"DDS ");
    w32(&mut h, 124);
    let mipflag = if nmips > 1 { 0x20000 } else { 0 };
    w32(&mut h, 0x1 | 0x2 | 0x4 | 0x1000 | 0x80000 | mipflag); // caps|height|width|pixelformat|linearsize|mipcount
    w32(&mut h, t.height);
    w32(&mut h, t.width);
    w32(&mut h, mip0_len as u32);
    w32(&mut h, 0);
    w32(&mut h, nmips);
    for _ in 0..11 {
        w32(&mut h, 0);
    }
    // pixel format
    w32(&mut h, 32);
    w32(&mut h, 0x4); // FOURCC
    h.extend_from_slice(b"DX10");
    for _ in 0..5 {
        w32(&mut h, 0);
    }
    w32(&mut h, if nmips > 1 { 0x1000 | 0x400000 | 0x8 } else { 0x1000 }); // caps: texture (+mipmap|complex)
    for _ in 0..4 {
        w32(&mut h, 0);
    }
    // DX10 header
    let _ = fmt;
    w32(&mut h, t.dxgi_format);
    w32(&mut h, 3); // TEXTURE2D
    w32(&mut h, 0);
    w32(&mut h, 1);
    w32(&mut h, 0);
    h.extend_from_slice(data);
    crate::atomic::write(path, h)?;
    Ok(())
}

fn u32at(s: &[u8]) -> u32 {
    u32::from_le_bytes([s[0], s[1], s[2], s[3]])
}

/// The stored values of a float-format image, unclamped: channels
/// interleaved, row by row. Returns the values and the channel count
/// (1, 2 or 4). None for formats that do not store floats.
pub fn to_floats(fmt: Fmt, data: &[u8], w: u32, h: u32) -> Option<(Vec<f32>, u32)> {
    let (ch, half) = match fmt {
        Fmt::R16f => (1, true),
        Fmt::Rg16f => (2, true),
        Fmt::Rgba16f => (4, true),
        Fmt::R32f => (1, false),
        _ => return None,
    };
    let n = (w as usize) * (h as usize) * ch;
    let size = if half { 2 } else { 4 };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(match data.get(i * size..i * size + size) {
            Some(s) if half => f16(u16::from_le_bytes([s[0], s[1]])),
            Some(s) => f32_or_zero(u32::from_le_bytes([s[0], s[1], s[2], s[3]])),
            None => 0.0,
        });
    }
    Some((out, ch as u32))
}

/// IEEE single float from its bits (NaN becomes 0, as in `f16`).
fn f32_or_zero(bits: u32) -> f32 {
    let v = f32::from_bits(bits);
    if v.is_nan() { 0.0 } else { v }
}

/// IEEE half float to f32 (NaN becomes 0).
pub(crate) fn f16(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((h >> 10) & 0x1F) as i32;
    let m = (h & 0x3FF) as f32;
    s * match e {
        0 => m / 1024.0 * 2f32.powi(-14),
        31 if m == 0.0 => f32::INFINITY,
        31 => 0.0,
        _ => (1.0 + m / 1024.0) * 2f32.powi(e - 15),
    }
}

#[cfg(test)]
mod alpha_tests {
    use super::*;

    #[test]
    fn floats_are_not_clamped() {
        // half floats 2.0, -0.5, 0.25, 1.0 as one RGBA texel
        let px: Vec<u8> = [0x4000u16, 0xb800, 0x3400, 0x3c00].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(to_floats(Fmt::Rgba16f, &px, 1, 1), Some((vec![2.0, -0.5, 0.25, 1.0], 4)));
        assert_eq!(to_floats(Fmt::R16f, &px, 2, 2), Some((vec![2.0, -0.5, 0.25, 1.0], 1)));
        assert_eq!(to_floats(Fmt::R32f, &3.5f32.to_le_bytes(), 1, 1), Some((vec![3.5], 1)));
        // short data reads as 0; other formats store no floats
        assert_eq!(to_floats(Fmt::Rg16f, &px[..4], 2, 1), Some((vec![2.0, -0.5, 0.0, 0.0], 2)));
        assert_eq!(to_floats(Fmt::Rgba8, &px, 1, 1), None);
    }

    #[test]
    fn a8_preview_preserves_alpha() {
        let rgba = to_rgba(Fmt::from_dxgi(65), &[0, 128, 255], 3, 1).unwrap();
        assert_eq!(rgba, [255, 255, 255, 0, 255, 255, 255, 128, 255, 255, 255, 255]);
    }

    #[test]
    fn r32_float_preview_is_clamped_grey() {
        let data: Vec<u8> = [0.0f32, 0.5, 2.0, -1.0, f32::NAN].iter().flat_map(|v| v.to_le_bytes()).collect();
        let rgba = to_rgba(Fmt::from_dxgi(41), &data, 5, 1).unwrap();
        assert_eq!(rgba, [0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255, 0, 0, 0, 255, 0, 0, 0, 255]);
    }

    #[test]
    fn r16g16_float_preview_has_red_green() {
        // 1.0 = 0x3C00, 0.5 = 0x3800
        let rgba = to_rgba(Fmt::from_dxgi(34), &[0x00, 0x3C, 0x00, 0x38], 1, 1).unwrap();
        assert_eq!(rgba, [255, 128, 0, 255]);
    }

    #[test]
    fn r10g10b10a2_preview_keeps_high_bits() {
        let v: u32 = 0x3FF | 0x200 << 10 | 0x004 << 20 | 2 << 30;
        let rgba = to_rgba(Fmt::from_dxgi(24), &v.to_le_bytes(), 1, 1).unwrap();
        assert_eq!(rgba, [255, 128, 1, 170]);
    }

    #[test]
    fn r8_preview_stays_opaque_grey() {
        let rgba = to_rgba(Fmt::from_dxgi(61), &[0, 128, 255], 3, 1).unwrap();
        assert_eq!(rgba, [0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255]);
    }
}

#[cfg(test)]
mod bc_colour_tests {
    use super::{Fmt, to_rgba};

    /// BC1 colour block: two RGB565 endpoints and sixteen 2-bit indices.
    fn bc1_block(c0: u16, c1: u16, indices: u32) -> [u8; 8] {
        let mut block = [0u8; 8];
        block[0..2].copy_from_slice(&c0.to_le_bytes());
        block[2..4].copy_from_slice(&c1.to_le_bytes());
        block[4..8].copy_from_slice(&indices.to_le_bytes());
        block
    }

    fn bc2_block(alpha: [u8; 16], colour: [u8; 8]) -> [u8; 16] {
        let mut block = [0u8; 16];
        for (i, nibble) in alpha.iter().enumerate() {
            block[i / 2] |= nibble << (4 * (i % 2));
        }
        block[8..16].copy_from_slice(&colour);
        block
    }

    /// BC3 alpha block: two endpoints and sixteen 3-bit indices.
    fn bc3_alpha(a0: u8, a1: u8, indices: [u8; 16]) -> [u8; 8] {
        let mut block = [0u8; 8];
        block[0] = a0;
        block[1] = a1;
        let mut packed = 0u64;
        for (i, index) in indices.iter().enumerate() {
            packed |= (*index as u64) << (3 * i);
        }
        block[2..8].copy_from_slice(&packed.to_le_bytes()[..6]);
        block
    }

    fn bc3_block(alpha: [u8; 8], colour: [u8; 8]) -> [u8; 16] {
        let mut block = [0u8; 16];
        block[..8].copy_from_slice(&alpha);
        block[8..16].copy_from_slice(&colour);
        block
    }

    fn px(rgba: &[u8], i: usize) -> [u8; 4] {
        rgba[i * 4..i * 4 + 4].try_into().unwrap()
    }

    #[test]
    fn bc1_three_colour_is_transparent() {
        // c0 <= c1: index 3 is transparent black, index 2 is the midpoint.
        let rgba = to_rgba(Fmt::Bc1, &bc1_block(0x0000, 0xFFFF, 0xFFFF_FFFF), 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [0, 0, 0, 0]);
        let rgba = to_rgba(Fmt::Bc1, &bc1_block(0x0000, 0xFFFF, 0xAAAA_AAAA), 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [127, 127, 127, 255]);
    }

    #[test]
    fn bc1_four_colour_is_opaque() {
        let rgba = to_rgba(Fmt::Bc1, &bc1_block(0xFFFF, 0x0000, 0xAAAA_AAAA), 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [170, 170, 170, 255]);
        let rgba = to_rgba(Fmt::Bc1, &bc1_block(0xFFFF, 0x0000, 0xFFFF_FFFF), 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [85, 85, 85, 255]);
    }

    #[test]
    fn bc2_always_uses_four_colours() {
        // Ascending palette: index 3 and index 2 are the 1/3 and 2/3 mixes.
        let block = bc2_block([0; 16], bc1_block(0x0000, 0xFFFF, 0xFFFF_FFFF));
        let rgba = to_rgba(Fmt::Bc2, &block, 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [170, 170, 170, 0]);
        let block = bc2_block([0; 16], bc1_block(0x0000, 0xFFFF, 0xAAAA_AAAA));
        let rgba = to_rgba(Fmt::Bc2, &block, 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [85, 85, 85, 0]);

        // Equal palette: every entry is c0.
        let block = bc2_block([0xF; 16], bc1_block(0x1234, 0x1234, 0xFFFF_FFFF));
        let rgba = to_rgba(Fmt::Bc2, &block, 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [16, 69, 165, 255]);

        // Alpha nibbles are replicated and preserved.
        let block = bc2_block(std::array::from_fn(|i| i as u8), bc1_block(0x0000, 0xFFFF, 0));
        let rgba = to_rgba(Fmt::Bc2, &block, 4, 4).unwrap();
        let alpha = [px(&rgba, 0)[3], px(&rgba, 1)[3], px(&rgba, 8)[3], px(&rgba, 15)[3]];
        assert_eq!(alpha, [0, 17, 136, 255]);
    }

    #[test]
    fn bc3_always_uses_four_colours() {
        let block = bc3_block(
            bc3_alpha(0, 255, [0; 16]),
            bc1_block(0x0000, 0xFFFF, 0xFFFF_FFFF),
        );
        let rgba = to_rgba(Fmt::Bc3, &block, 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [170, 170, 170, 0]);
        let block = bc3_block(
            bc3_alpha(0, 255, [0; 16]),
            bc1_block(0x0000, 0xFFFF, 0xAAAA_AAAA),
        );
        let rgba = to_rgba(Fmt::Bc3, &block, 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [85, 85, 85, 0]);

        let block = bc3_block(
            bc3_alpha(255, 255, [1; 16]),
            bc1_block(0x1234, 0x1234, 0xFFFF_FFFF),
        );
        let rgba = to_rgba(Fmt::Bc3, &block, 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [16, 69, 165, 255]);
    }

    #[test]
    fn bc3_alpha_interpolation_is_preserved() {
        // a0 < a1: six interpolated values, then 0 and 255.
        let block = bc3_block(
            bc3_alpha(0, 255, std::array::from_fn(|i| (i % 8) as u8)),
            bc1_block(0x0000, 0xFFFF, 0),
        );
        let rgba = to_rgba(Fmt::Bc3, &block, 4, 4).unwrap();
        let alpha: Vec<u8> = (0..8).map(|i| px(&rgba, i)[3]).collect();
        assert_eq!(alpha, [0, 255, 51, 102, 153, 204, 0, 255]);

        // a0 > a1: seven interpolated values between the endpoints.
        let block = bc3_block(
            bc3_alpha(255, 0, std::array::from_fn(|i| (i % 8) as u8)),
            bc1_block(0x0000, 0xFFFF, 0),
        );
        let rgba = to_rgba(Fmt::Bc3, &block, 4, 4).unwrap();
        let alpha: Vec<u8> = (0..8).map(|i| px(&rgba, i)[3]).collect();
        assert_eq!(alpha, [255, 0, 218, 182, 145, 109, 72, 36]);
    }

    #[test]
    fn bc_colour_clips_partial_blocks() {
        let rgba = to_rgba(Fmt::Bc1, &bc1_block(0xFFFF, 0x0000, 0), 2, 3).unwrap();
        assert_eq!(rgba.len(), 2 * 3 * 4);
        for i in 0..6 {
            assert_eq!(px(&rgba, i), [255, 255, 255, 255]);
        }
        let ascending = bc1_block(0, 0xFFFF, 0xAAAA_AAAA);
        let other = bc1_block(0, 0xFFFF, 0xFFFF_FFFF);
        for (fmt, first, second, a0, a1) in [
            (Fmt::Bc2, bc2_block([1; 16], ascending), bc2_block([14; 16], other), 17, 238),
            (Fmt::Bc3, bc3_block(bc3_alpha(37, 0, [0; 16]), ascending),
                bc3_block(bc3_alpha(211, 0, [0; 16]), other), 37, 211),
        ] {
            let mut data = first.to_vec();
            data.extend_from_slice(&second);
            let rgba = to_rgba(fmt, &data, 5, 3).unwrap();
            assert_eq!(rgba.len(), 5 * 3 * 4);
            for y in 0..3 {
                for x in 0..5 {
                    let expected = if x < 4 { [85, 85, 85, a0] } else { [170, 170, 170, a1] };
                    assert_eq!(px(&rgba, y * 5 + x), expected);
                }
            }
        }
    }

    #[test]
    fn bc5_reconstructs_normal_z() {
        // One 4x4 BC5 block: R = 128, G = 128 everywhere (flat normal -> Z ≈ +1, B = 255).
        let mut flat = [0u8; 16];
        flat[0] = 128;
        flat[1] = 128;
        flat[8] = 128;
        flat[9] = 128;
        let rgba = to_rgba(Fmt::Bc5, &flat, 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [128, 128, 255, 255]);

        // Tilted normal: R = 255 (x = +1), G = 128 (y ≈ 0) -> z = 0 -> B = 128.
        let mut tilted = [0u8; 16];
        tilted[0] = 255;
        tilted[1] = 255;
        tilted[8] = 128;
        tilted[9] = 128;
        let rgba = to_rgba(Fmt::Bc5, &tilted, 4, 4).unwrap();
        assert_eq!(px(&rgba, 0), [255, 128, 128, 255]);
    }
}

#[cfg(test)]
mod band_tests {
    use super::{bc_to_rgba, Fmt};

    /// Every band split gives the bytes of a single pass, on random blocks
    /// and on sizes that are not whole blocks.
    #[test]
    fn bands_equal_one_pass() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for fmt in [Fmt::Bc1, Fmt::Bc2, Fmt::Bc3, Fmt::Bc4, Fmt::Bc5, Fmt::Bc6u, Fmt::Bc6s, Fmt::Bc7] {
            for (w, h) in [(64usize, 64usize), (37, 50), (4, 128), (130, 9)] {
                let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
                let data: Vec<u8> = (0..bw * bh * fmt.bytes_per_elem() as usize).map(|_| (next() >> 32) as u8).collect();
                let whole = bc_to_rgba(fmt, &data, w, h, bh).unwrap();
                assert_eq!(whole.len(), w * h * 4);
                for rows in [1, 2, 3, 5, bh - 1] {
                    assert!(bc_to_rgba(fmt, &data, w, h, rows.max(1)).unwrap() == whole, "{fmt:?} {w}x{h} in bands of {rows}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tint_tests {
    #[test]
    fn tint_layouts() {
        for (w, l) in [(13, (8, 1)), (37, (8, 4)), (131, (32, 4)), (256, (256, 1)), (513, (128, 4)), (1024, (256, 4)), (1028, (16, 64)), (8194, (64, 128))] {
            assert_eq!(super::tint_layout(w), l, "width {w}");
        }
    }
}
