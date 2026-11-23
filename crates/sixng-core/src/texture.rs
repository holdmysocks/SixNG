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
    Unknown,
}

impl Fmt {
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
            _ => Fmt::Unknown,
        }
    }
    pub fn block_dim(&self) -> u32 {
        match self {
            Fmt::Rgba8 | Fmt::Bgra8 | Fmt::R8 | Fmt::A8 | Fmt::Rg8 | Fmt::R16f | Fmt::R16 | Fmt::Rgba16 | Fmt::Rgba16f | Fmt::Unknown => 1,
            _ => 4,
        }
    }
    pub fn bytes_per_elem(&self) -> u32 {
        match self {
            Fmt::Bc1 | Fmt::Bc4 => 8,
            Fmt::Bc2 | Fmt::Bc3 | Fmt::Bc5 | Fmt::Bc6u | Fmt::Bc6s | Fmt::Bc7 => 16,
            Fmt::Rgba8 | Fmt::Bgra8 => 4,
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
/// packed tail whose layout is not solved yet.
pub fn exportable_mips(t: &TexInfo) -> u32 {
    let fmt = Fmt::from_dxgi(t.dxgi_format);
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

/// Linear (untiled) data of one mip of slice 0. Mips whose element size is a
/// multiple of the 16x16 tile are untiled; smaller ones are taken as linear
///.
pub fn mip_linear(r: &Resource, t: &TexInfo, m: u32) -> Result<(Fmt, Vec<u8>, u32, u32)> {
    let fmt = Fmt::from_dxgi(t.dxgi_format);
    if fmt == Fmt::Unknown {
        return Err(anyhow!("unsupported format {}", t.format));
    }
    if m == 0 {
        let (f, d) = mip0_linear(r, t, false)?;
        return Ok((f, d, t.width, t.height));
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

/// Decode linear mip0 to RGBA8.
pub fn to_rgba(fmt: Fmt, data: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
    use texture2ddecoder as td;
    let (wu, hu) = (w as usize, h as usize);
    let mut img = vec![0u32; wu * hu];
    let need_blocks = (w.div_ceil(4) * h.div_ceil(4)) as usize;
    let mut buf;
    let data = if fmt.block_dim() == 4 && data.len() < need_blocks * fmt.bytes_per_elem() as usize {
        buf = data.to_vec();
        buf.resize(need_blocks * fmt.bytes_per_elem() as usize, 0);
        &buf[..]
    } else {
        data
    };
    let r = match fmt {
        Fmt::Bc1 => td::decode_bc1(data, wu, hu, &mut img),
        Fmt::Bc2 => td::decode_bc2(data, wu, hu, &mut img),
        Fmt::Bc3 => td::decode_bc3(data, wu, hu, &mut img),
        Fmt::Bc4 => td::decode_bc4(data, wu, hu, &mut img),
        Fmt::Bc5 => td::decode_bc5(data, wu, hu, &mut img),
        Fmt::Bc6u => crate::bc6h::decode_image(data, wu, hu, &mut img, false),
        Fmt::Bc6s => crate::bc6h::decode_image(data, wu, hu, &mut img, true),
        Fmt::Bc7 => td::decode_bc7(data, wu, hu, &mut img),
        Fmt::Rgba8 | Fmt::Bgra8 | Fmt::R8 | Fmt::A8 | Fmt::Rg8 | Fmt::R16f | Fmt::R16 | Fmt::Rgba16 | Fmt::Rgba16f => {
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
                    _ => {
                        o[0] = s[0];
                        o[1] = s[1];
                        o[2] = 0;
                    }
                }
            }
            return Ok(out);
        }
        Fmt::Unknown => Err("unsupported"),
    };
    r.map_err(|e| anyhow!("decode: {e}"))?;
    let mut out = Vec::with_capacity(wu * hu * 4);
    for px in img {
        // texture2ddecoder packs as 0xAARRGGBB
        out.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, px as u8, (px >> 24) as u8]);
    }
    Ok(out)
}

pub fn save_png(path: &Path, rgba: &[u8], w: u32, h: u32) -> Result<()> {
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec()).ok_or_else(|| anyhow!("bad image size"))?;
    img.save(path)?;
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
    std::fs::write(path, h)?;
    Ok(())
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
    fn a8_preview_preserves_alpha() {
        let rgba = to_rgba(Fmt::from_dxgi(65), &[0, 128, 255], 3, 1).unwrap();
        assert_eq!(rgba, [255, 255, 255, 0, 255, 255, 255, 128, 255, 255, 255, 255]);
    }

    #[test]
    fn r8_preview_stays_opaque_grey() {
        let rgba = to_rgba(Fmt::from_dxgi(61), &[0, 128, 255], 3, 1).unwrap();
        assert_eq!(rgba, [0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255]);
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
