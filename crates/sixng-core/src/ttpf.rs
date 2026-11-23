//! `ttpf` texture stores (`data/ttdb/**`, magic `RDS5`).
//!
//! ```text
//! +0x00 "RDS5"
//! +0x04 u32 chunk-table entries (the last one is a terminator)
//! +0x08 u32 size of the virtual (uncompressed) space, including a 0x1000 header page
//! +0x0C u32 0x16000000
//! +0x10 u32 ?, +0x14 u32 0
//! +0x18 chunk table, 8 bytes per entry:
//!       u32 a: (a & 0xFFFFFF) << 8 = file offset of a zlib stream; bits 24-29 = flags
//!              (codec: 0x20 zlib; 0x04 BC1, 0x0C/0x10 BC3/BC4?, 0x14 BC5, 0x18 BC6H,
//!              0x1C BC7 = per-format BCPack, the Xbox Series hardware texture codec;
//!              BC1 decodes with `bcpack-rs`, BC3 with `bc3pack`, BC4/BC5 with `bc45pack`,
//!              BC6H with `bc6pack`, BC7 with `bc7pack`; 0x00 stored),
//!              bits 30-31 = 4 KiB pages added to the virtual offset
//!       u32 b: virtual offset = 0x1000 + (b & 0xFFFFF) * 0x4000 + (a >> 30) * 0x1000
//!              (the 0x1000 is the header page); b >> 20 = bytes of padding after
//!              the chunk's data (stored length = next offset - offset - pad)
//!       A chunk runs to the next entry, both in the file and in the virtual space.
//!       The last entry is a terminator (file end, end of the virtual space). The
//!       virtual space can exceed 4 GiB (the header's size field is truncated).
//! align_up(0x10 + 8 * entries, 0x100): "ETPF" texture index
//!       +4 u32 version (0x27A), +8 u32 texture count n, +0xC u32 length of the next part
//!       +0x10            n x 16-byte descriptors
//!       +0x10 + 16n      n x u64 texture hashes, sorted (descriptor i <-> hash i)
//!       ...              other tables (u32 list, n x 32-byte digests)
//! descriptor:
//!       u32 d0: bits 0-13 width-1, bits 14-27 height-1, bits 28-31 mip count-1
//!              (see `TtpfTex::mips` for the exceptions)
//!       u32 d1: bits 0-2 flags, bits 3-9 DXGI format, bits 10-19 array size-1
//!              (depth-1 on 3D textures), bits 20-23 resource kind (2 = 2D,
//!              4 = cube, 6 = 3D, 10 = 2D array), bits 24-27 tile mode
//!              (0 = SW_64KB_D, 1/2/3 = SW_4KB_S, see `gfx10`; 9-12 = SW_4KB_S
//!              cube maps stored as tile mode - 8 copies, see
//!              `gfx10::Layout::with_cube_copies`)
//!       u64 loc: bits 0-20 first 4 KiB page of the texture, counted after the
//!              0x1000 header page; higher bits ?
//! ```
//! A texture's pixel data is its full mip chain in the AMD Gfx10 layout
//! (`crate::gfx10`): mips smallest first, small mips packed in a mip-tail
//! block, mip 0 last. The hashes are the 64-bit texture hashes referenced by
//! drawable shaders.
use anyhow::{anyhow, Context, Result};
use flate2::read::ZlibDecoder;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct TtpfTex {
    pub hash: u64,
    pub width: u32,
    pub height: u32,
    pub dxgi_format: u32,
    /// Byte offset in the virtual space.
    pub virt: u64,
    /// Bytes of the texture's mip chain (from the Gfx10 layout; textures
    /// can overlap others' padding, so this is not the distance to the next one).
    pub size: u64,
    pub d0: u32,
    pub d1: u32,
    pub loc: u64,
    /// Cube maps (tile modes 9-12): offset of the full-resolution copy,
    /// from the chunk table (each copy is its own chunk).
    #[serde(skip)]
    pub cube_base: Option<u64>,
    /// Mip count recovered from the store layout where the descriptor's
    /// field does not fit (see `Ttpf::open`); `mips()` prefers it.
    #[serde(skip)]
    pub mips_fixed: Option<u32>,
}

pub struct Ttpf {
    pub path: PathBuf,
    /// (file offset, virtual offset, flags byte, trailing pad bytes) per chunk, plus a terminator.
    pub chunks: Vec<(u64, u64, u8, u64)>,
    pub virt_size: u64,
    pub textures: Vec<TtpfTex>,
    by_hash: HashMap<u64, usize>,
}

/// Flags of a BCPack chunk (any BC format).
fn is_bcpack(fl: u8) -> bool {
    fl & 0x1C != 0
}

/// BCPack BC3 codec id.
const CODEC_BC3: u8 = 0x0C;
/// BCPack BC4 / BC5 codec ids.
const CODEC_BC4: u8 = 0x10;
const CODEC_BC5: u8 = 0x14;
/// BCPack BC6H codec id.
const CODEC_BC6H: u8 = 0x18;
/// BCPack BC7 codec id.
const CODEC_BC7: u8 = 0x1C;

/// BCPack chunks that SixNG cannot decode (a codec id outside the six BC formats).
fn is_undecoded(fl: u8) -> bool {
    is_bcpack(fl) && ![bcpack::ttpf::CODEC_BC1, CODEC_BC3, CODEC_BC4, CODEC_BC5, CODEC_BC6H, CODEC_BC7].contains(&(fl & 0x3F))
}

/// Decode a BCPack BC1 chunk to plain BC1 blocks (`blocks` x 8 bytes).
/// `None` if the stream does not decode.
fn decode_bc1(comp: &[u8], blocks: usize) -> Option<Vec<u8>> {
    use bcpack::bc1::{QuadTable, Window};
    static TABLES: std::sync::OnceLock<(Window, QuadTable)> = std::sync::OnceLock::new();
    let (w, q) = TABLES.get_or_init(|| (Window::default_table(), QuadTable::default_table()));
    bcpack::bc1::decode(comp, blocks, w, q).ok().map(|o| o.blocks)
}

/// The descriptor's mip field (d0 bits 28-31) undercounts on many textures:
/// e.g. every tile-mode-1/2 texture with field 0 and a second location in
/// `loc` holds 3 mips, not 1 (41.5 K textures). Mips are stored smallest
/// first with mip 0 last, so a wrong count puts mip 0 at the wrong offset
/// (a scrambled band of the smaller mips at the top). The store packs each
/// texture's chain right up to the next texture, so where the field's size
/// does not reach the next texture and exactly one mip count does, that
/// count is used. 2D textures and arrays (tile modes 0-3); checked as mip
/// n+1 = mip n downsampled.
fn fix_mip_counts(textures: &mut [TtpfTex], end: Option<u64>) {
    let mut order: Vec<usize> = (0..textures.len()).collect();
    order.sort_by_key(|&i| textures[i].virt);
    // (texture, bytes up to the next one, whether that is the store end)
    let mut spans: Vec<(usize, u64, bool)> = order.windows(2).map(|w| (w[0], textures[w[1]].virt - textures[w[0]].virt, false)).collect();
    if let (Some(&last), Some(end)) = (order.last(), end) {
        spans.push((last, end.saturating_sub(textures[last].virt), true));
    }
    for (i, gap, at_end) in spans {
        let t = &textures[i];
        if gap == t.size || t.tile_mode() > 3 || t.is_3d() {
            continue;
        }
        let fmt = crate::texture::Fmt::from_dxgi(t.dxgi_format);
        let (Some(sw), false) = (t.swizzle(), fmt == crate::texture::Fmt::Unknown) else { continue };
        let bd = fmt.block_dim();
        let full = 32 - t.width.max(t.height).leading_zeros();
        let fits: Vec<u32> = (1..=full)
            .filter(|&m| crate::gfx10::Layout::new(sw, t.width.div_ceil(bd), t.height.div_ceil(bd), m, fmt.bytes_per_elem()).is_some_and(|l| l.with_slices(t.slices()).total == gap))
            .collect();
        if let ([m], false) = (&fits[..], at_end) {
            let m = *m;
            let t = &mut textures[i];
            t.mips_fixed = Some(m);
            t.size = gap;
        }
    }
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64le(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

impl Ttpf {
    pub fn open(path: &Path) -> Result<Ttpf> {
        let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
        let mut h = [0u8; 24];
        f.read_exact(&mut h)?;
        if &h[..4] != b"RDS5" {
            return Err(anyhow!("not an RDS5 store"));
        }
        let n = u32le(&h, 4) as usize;
        let virt_size = u32le(&h, 8) as u64;
        let mut t = vec![0u8; 8 * n];
        f.read_exact(&mut t)?;
        let mut chunks = Vec::with_capacity(n);
        for i in 0..n {
            let a = u32le(&t, 8 * i);
            let b = u32le(&t, 8 * i + 4);
            if a == 0 && b == 0 {
                break;
            }
            chunks.push((((a & 0xFF_FFFF) as u64) << 8, 0x1000 + (b & 0xF_FFFF) as u64 * 0x4000 + (a >> 30) as u64 * 0x1000, (a >> 24) as u8, (b >> 20) as u64));
        }
        let eoff = ((0x10 + 8 * n + 0xFF) & !0xFF) as u64;
        f.seek(SeekFrom::Start(eoff))?;
        let mut e = [0u8; 16];
        f.read_exact(&mut e)?;
        if &e[..4] != b"ETPF" {
            return Err(anyhow!("no ETPF index at {eoff:#x}"));
        }
        let nt = u32le(&e, 8) as usize;
        let mut body = vec![0u8; 24 * nt];
        f.read_exact(&mut body)?;
        let mut textures = Vec::with_capacity(nt);
        for i in 0..nt {
            let d0 = u32le(&body, 16 * i);
            let d1 = u32le(&body, 16 * i + 4);
            let loc = u64le(&body, 16 * i + 8);
            textures.push(TtpfTex {
                hash: u64le(&body, 16 * nt + 8 * i),
                width: (d0 & 0x3FFF) + 1,
                height: ((d0 >> 14) & 0x3FFF) + 1,
                dxgi_format: (d1 >> 3) & 0x7F,
                virt: (loc & 0x1F_FFFF) * 0x1000 + 0x1000,
                size: 0,
                d0,
                d1,
                loc,
                cube_base: None,
                mips_fixed: None,
            });
        }
        for t in textures.iter_mut() {
            if (9..=12).contains(&t.tile_mode()) {
                let n = (t.tile_mode() - 8) as usize;
                if let Ok(i) = chunks.binary_search_by_key(&t.virt, |c| c.1) {
                    t.cube_base = chunks.get(i + n - 1).map(|c| c.1 - t.virt);
                }
            }
            t.size = t.layout().map(|l| l.total).unwrap_or(0);
        }
        fix_mip_counts(&mut textures, Some(virt_size));
        let by_hash = textures.iter().enumerate().map(|(i, t)| (t.hash, i)).collect();
        Ok(Ttpf { path: path.to_path_buf(), chunks, virt_size, textures, by_hash })
    }

    /// Short name: the file name, prefixed with the pack for stores under
    /// `pack/<name>/` (`story000/ttdb_maps.ttpf`).
    pub fn label(&self) -> String {
        let file = self.path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        let parts: Vec<String> = self.path.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
        match parts.iter().rposition(|c| c.eq_ignore_ascii_case("pack")).and_then(|i| parts.get(i + 1)) {
            Some(pk) if pk != &file => format!("{pk}/{file}"),
            _ => file,
        }
    }
    pub fn get(&self, hash: u64) -> Option<&TtpfTex> {
        self.by_hash.get(&hash).map(|&i| &self.textures[i])
    }

    /// Read `len` bytes of the virtual space from `virt`, inflating only the
    /// chunks that overlap it.
    pub fn read(&self, virt: u64, len: u64) -> Result<Vec<u8>> {
        Ok(self.read_ex(virt, len)?.0)
    }

    /// Like `read`; also returns how many bytes could not be decoded (BCPack
    /// chunks other than BC1, or a BC1 chunk that fails to decode; left as zeros).
    pub fn read_ex(&self, virt: u64, len: u64) -> Result<(Vec<u8>, u64)> {
        let mut out = vec![0u8; len as usize];
        let mut skipped = 0u64;
        let end = virt + len;
        let mut f = File::open(&self.path)?;
        for i in 0..self.chunks.len().saturating_sub(1) {
            let (foff, v0, fl, pad) = self.chunks[i];
            let (fnext, v1, _, _) = self.chunks[i + 1];
            if v1 <= virt || v0 >= end {
                continue;
            }
            if is_undecoded(fl) {
                skipped += end.min(v1) - virt.max(v0);
                continue;
            }
            f.seek(SeekFrom::Start(foff))?;
            let mut comp = vec![0u8; (fnext - foff).saturating_sub(pad) as usize];
            f.read_exact(&mut comp)?;
            let raw = if is_bcpack(fl) {
                let decoded = if fl & 0x3F == CODEC_BC3 {
                    crate::bc3pack::decode(&comp, ((v1 - v0) / 16) as usize).map(|o| o.blocks)
                } else if fl & 0x3F == CODEC_BC7 {
                    crate::bc7pack::decode(&comp, ((v1 - v0) / 16) as usize).map(|o| o.blocks)
                } else if fl & 0x3F == CODEC_BC4 {
                    crate::bc45pack::decode_bc4(&comp, ((v1 - v0) / 8) as usize).map(|o| o.blocks)
                } else if fl & 0x3F == CODEC_BC5 {
                    crate::bc45pack::decode_bc5(&comp, ((v1 - v0) / 16) as usize).map(|o| o.blocks)
                } else if fl & 0x3F == CODEC_BC6H {
                    crate::bc6pack::decode_bc6h(&comp, ((v1 - v0) / 16) as usize).map(|o| o.blocks)
                } else {
                    decode_bc1(&comp, ((v1 - v0) / 8) as usize)
                };
                match decoded {
                    Some(b) => b,
                    None => {
                        skipped += end.min(v1) - virt.max(v0);
                        continue;
                    }
                }
            } else if fl & 0x3F == 0 {
                comp // stored (5 chunks in all stores; sizes match)
            } else {
                let mut raw = Vec::with_capacity((v1 - v0) as usize);
                ZlibDecoder::new(&comp[..]).read_to_end(&mut raw).with_context(|| format!("chunk {i} at {foff:#x}"))?;
                raw
            };
            let s = virt.max(v0);
            let e = end.min(v1).min(v0 + raw.len() as u64);
            if e > s {
                out[(s - virt) as usize..(e - virt) as usize].copy_from_slice(&raw[(s - v0) as usize..(e - v0) as usize]);
            }
        }
        Ok((out, skipped))
    }

    /// Bytes of [virt, virt + len) that fall in BCPack chunks SixNG cannot
    /// decode (a codec outside the six BC formats; no read).
    pub fn bcpack_bytes(&self, virt: u64, len: u64) -> u64 {
        self.bcpack_bytes_where(virt, len, is_undecoded)
    }

    fn bcpack_bytes_where(&self, virt: u64, len: u64, pick: fn(u8) -> bool) -> u64 {
        let end = virt + len;
        let mut n = 0;
        for w in self.chunks.windows(2) {
            let ((_, v0, fl, _), (_, v1, _, _)) = (w[0], w[1]);
            if v1 > virt && v0 < end && pick(fl) {
                n += end.min(v1) - virt.max(v0);
            }
        }
        n
    }

    /// Virtual ranges [start, end) of the BCPack chunks SixNG cannot decode
    /// (all but BC1), sorted and merged (for fast per-texture checks over a
    /// whole store).
    pub fn bcpack_ranges(&self) -> Vec<(u64, u64)> {
        let mut v: Vec<(u64, u64)> = self.chunks.windows(2).filter(|w| is_undecoded(w[0].2)).map(|w| (w[0].1, w[1].1)).filter(|r| r.1 > r.0).collect();
        v.sort();
        let mut m: Vec<(u64, u64)> = Vec::with_capacity(v.len());
        for r in v {
            match m.last_mut() {
                Some(l) if r.0 <= l.1 => l.1 = l.1.max(r.1),
                _ => m.push(r),
            }
        }
        m
    }

    /// Largest mip of `t` that lies entirely outside undecodable BCPack chunks.
    pub fn best_mip(&self, t: &TtpfTex) -> Option<u32> {
        self.best_mip_where(t, is_undecoded)
    }

    fn best_mip_where(&self, t: &TtpfTex, pick: fn(u8) -> bool) -> Option<u32> {
        let l = t.layout()?;
        (0..l.mips.len()).find(|&m| {
            let (a, b) = l.mip_span(m);
            self.bcpack_bytes_where(t.virt + a, b - a, pick) == 0
        }).map(|m| m as u32)
    }

    /// Pixel data of a texture (full mip chain, tiled).
    pub fn data(&self, t: &TtpfTex) -> Result<Vec<u8>> {
        self.read(t.virt, t.size.min(64 << 20))
    }

    /// Pixel data plus the number of bytes that could not be decoded.
    pub fn data_ex(&self, t: &TtpfTex) -> Result<(Vec<u8>, u64)> {
        self.read_ex(t.virt, t.size.min(64 << 20))
    }
}

/// All stores under `<game>/data/ttdb`, opened lazily.
pub struct TtpfSet {
    pub stores: Vec<Ttpf>,
}

impl TtpfSet {
    pub fn open_dir(dir: &Path) -> TtpfSet {
        let mut paths = Vec::new();
        fn walk(d: &Path, out: &mut Vec<PathBuf>) {
            if let Ok(rd) = std::fs::read_dir(d) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        walk(&p, out);
                    } else if p.extension().map(|x| x == "ttpf").unwrap_or(false) {
                        out.push(p);
                    }
                }
            }
        }
        walk(dir, &mut paths);
        paths.sort();
        TtpfSet { stores: paths.iter().filter_map(|p| Ttpf::open(p).ok()).collect() }
    }

    pub fn get(&self, hash: u64) -> Option<(&Ttpf, &TtpfTex)> {
        self.stores.iter().find_map(|s| s.get(hash).map(|t| (s, t)))
    }
}

impl TtpfTex {
    /// Mip count: d0 bits 28-31 + 1 (UI textures have 1-2, a 1024 map
    /// texture 7, i.e. down to 16 px), with two exceptions measured on the
    /// install:
    /// * cube maps (tile modes 9-12) need the tile mode too: the count is
    ///   field + copies - 1 (copies = tile mode - 8). On every cube the data
    ///   runs to exactly that mip and the tail slot after it is empty; e.g. a
    ///   64x64 BC7 cube with field 6 and 2 copies holds 7 mips, down to 1x1.
    /// * tile mode 1 with field 1 is one mip: the store spacing says 1 on all
    ///   4071 of these where it can tell (never 2), and the small ones whose
    ///   whole chain fits one block (16x8 RGBA8, ...) hold their data at
    ///   offset 0, where a single mip starts (see `gfx10`).
    pub fn mips(&self) -> u32 {
        if let Some(m) = self.mips_fixed {
            return m;
        }
        let field = self.d0 >> 28;
        match self.tile_mode() {
            9..=12 => {
                let full = 32 - self.width.max(self.height).leading_zeros();
                (field + self.tile_mode() - 9).clamp(1, full)
            }
            1 if field == 1 => 1,
            _ => field + 1,
        }
    }
    /// Resource kind (d1 bits 20-23): 2 = 2D, 4 = cube, 6 = 3D, 10 = 2D array.
    pub fn kind(&self) -> u32 {
        (self.d1 >> 20) & 0xF
    }
    /// A 3D (volume) texture; `slices()` is its depth.
    pub fn is_3d(&self) -> bool {
        self.kind() == 6
    }
    /// A `TexInfo` describing this texture, with `data_off` = 0 relative to
    /// the bytes returned by `Ttpf::data`.
    pub fn tex_info(&self, name: &str) -> crate::texture::TexInfo {
        crate::texture::TexInfo {
            name: name.to_string(),
            width: self.width,
            height: self.height,
            depth: if self.is_3d() { self.slices() } else { 1 },
            mips: self.mips(),
            dxgi_format: self.dxgi_format,
            format: crate::texture::dxgi_name(self.dxgi_format),
            struct_off: 0,
            data_off: 0,
            size_field: 0,
            swizzle: crate::texture::Fmt::from_dxgi(self.dxgi_format).swizzle_status().into(),
        }
    }
}

/// Decode mip `m` of a ttpf texture to RGBA8 (slice 0 of an array).
/// Returns (rgba, width, height).
pub fn decode_rgba(t: &TtpfTex, data: &[u8], m: u32) -> Result<(Vec<u8>, u32, u32)> {
    decode_rgba_slice(t, data, m, 0)
}

/// Decode slice `sl` of mip `m` of a ttpf texture to RGBA8.
pub fn decode_rgba_slice(t: &TtpfTex, data: &[u8], m: u32, sl: u32) -> Result<(Vec<u8>, u32, u32)> {
    use crate::texture as tx;
    let ti = t.tex_info("");
    let fmt = tx::Fmt::from_dxgi(t.dxgi_format);
    if fmt == tx::Fmt::Unknown {
        return Err(anyhow!("unsupported format {}", ti.format));
    }
    let (mw, mh) = ((t.width >> m).max(1), (t.height >> m).max(1));
    let l = t.layout().ok_or_else(|| anyhow!("tile mode {} / {} not supported", t.tile_mode(), ti.format))?;
    if m as usize >= l.mips.len() {
        return Err(anyhow!("mip {m} out of range"));
    }
    if sl >= l.slices {
        return Err(anyhow!("slice {sl} out of range ({} slices)", l.slices));
    }
    let lin = l.untile_slice(data, m as usize, sl);
    Ok((tx::to_rgba(fmt, &lin, mw, mh)?, mw, mh))
}

impl TtpfTex {
    /// Tiling mode (d1 bits 24-27): 0 = SW_64KB_D, 1/2/3 = SW_4KB_S
    /// (Gfx10 swizzle modes; all verified). Other values occur on a few
    /// special formats and are not decoded.
    pub fn tile_mode(&self) -> u32 {
        (self.d1 >> 24) & 0xF
    }
    pub fn swizzle(&self) -> Option<crate::gfx10::Swizzle> {
        match self.tile_mode() {
            0 => Some(crate::gfx10::Swizzle::D64K),
            1..=3 | 9..=12 => Some(crate::gfx10::Swizzle::S4K),
            _ => None,
        }
    }
    /// Array size (d1 bits 10-19, plus 1). 1 for almost all textures; 6 on
    /// cube maps; 2-64 on a few texture arrays; the depth of a 3D texture.
    pub fn slices(&self) -> u32 {
        ((self.d1 >> 10) & 0x3FF) + 1
    }
    /// Mip-chain layout (`None` for unsupported tile modes or element sizes:
    /// 1-, 2-, 4-, 8- and 16-byte elements are mapped for 2D textures; 3D
    /// textures only with one mip, `SW_4KB_S` and 2- or 8-byte elements, the
    /// only kinds measured on the install).
    pub fn layout(&self) -> Option<crate::gfx10::Layout> {
        let fmt = crate::texture::Fmt::from_dxgi(self.dxgi_format);
        if fmt == crate::texture::Fmt::Unknown {
            return None;
        }
        let bd = fmt.block_dim();
        if self.is_3d() {
            if self.mips() != 1 || bd != 1 {
                return None;
            }
            return crate::gfx10::Layout::new_3d(self.swizzle()?, self.width, self.height, self.slices(), fmt.bytes_per_elem());
        }
        let l = crate::gfx10::Layout::new(self.swizzle()?, self.width.div_ceil(bd), self.height.div_ceil(bd), self.mips(), fmt.bytes_per_elem())?;
        match self.tile_mode() {
            9..=12 => l.with_cube_copies(self.slices(), self.tile_mode() - 8, self.cube_base),
            _ => Some(l.with_slices(self.slices())),
        }
    }
    pub fn decodable(&self) -> bool {
        self.layout().is_some()
    }
}

/// The stores under `<game root>/data/ttdb` and `<game root>/pack/*/data/ttdb`,
/// opened once per process.
pub fn global(game_root: &Path) -> &'static TtpfSet {
    static SET: std::sync::OnceLock<TtpfSet> = std::sync::OnceLock::new();
    SET.get_or_init(|| {
        let mut set = TtpfSet::open_dir(&game_root.join("data").join("ttdb"));
        // Content packs keep their own stores (`pack/<name>/data/ttdb`), used
        // by the models in that pack's archive (story000 map models, ...).
        // Only 4 hashes also occur in data/ttdb, with the same texture.
        let mut packs: Vec<PathBuf> = std::fs::read_dir(game_root.join("pack")).into_iter().flatten().flatten().map(|e| e.path().join("data").join("ttdb")).filter(|p| p.is_dir()).collect();
        packs.sort();
        for p in packs {
            set.stores.extend(TtpfSet::open_dir(&p).stores);
        }
        set
    })
}

/// Mean colour of a decoded texture and whether it looks like a tangent-space
/// normal map (mean close to (128, 128, 255)).
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct PngStats {
    pub width: u32,
    pub height: u32,
    pub mean_rgba: [f32; 4],
    pub normal_like: bool,
    /// Mip level written (0 = full size). Higher when the top mips are in
    /// BCPack chunks and only lower mips could be decoded.
    #[serde(default)]
    pub mip: u32,
    /// Lowest and highest alpha value. A narrow range on a colour texture
    /// means the alpha channel holds a constant, not opacity.
    #[serde(default)]
    pub alpha_range: Option<[u8; 2]>,
    /// Mip count the PNG was decoded with. PNGs written before the mip
    /// count fix (`fix_mip_counts`) have none and are re-decoded when the
    /// count changed.
    #[serde(default)]
    pub mips: Option<u32>,
}

fn stats(rgba: &[u8], w: u32, h: u32) -> PngStats {
    let mut s = [0f64; 4];
    let (mut amin, mut amax) = (255u8, 0u8);
    for px in rgba.chunks_exact(4) {
        for k in 0..4 {
            s[k] += px[k] as f64;
        }
        amin = amin.min(px[3]);
        amax = amax.max(px[3]);
    }
    let n = (rgba.len() / 4).max(1) as f64;
    let m = s.map(|x| (x / n) as f32);
    let normal_like = m[2] > 200.0 && (100.0..160.0).contains(&m[0]) && (100.0..160.0).contains(&m[1]);
    PngStats { width: w, height: h, mean_rgba: m, normal_like, mip: 0, alpha_range: Some([amin, amax]), mips: None }
}

/// Decode mip 0 of a texture to `<dir>/<hash>.png` (plus a `.json` with
/// its stats) unless it exists. Returns the file path and the stats.
pub fn write_png(store: &Ttpf, t: &TtpfTex, dir: &Path) -> Result<(PathBuf, PngStats)> {
    let p = dir.join(format!("{:016x}.png", t.hash));
    let js = dir.join(format!("{:016x}.json", t.hash));
    if p.exists() {
        // stats written before alpha_range existed are refreshed below, and so
        // are lower-mip PNGs whose top mips now decode (BC1 BCPack)
        if let Some(st) = std::fs::read(&js).ok().and_then(|b| serde_json::from_slice::<PngStats>(&b).ok()).filter(|st| st.alpha_range.is_some() && !matches!(store.best_mip(t), Some(m) if m < st.mip) && (t.mips_fixed.is_none() || st.mips == Some(t.mips()))) {
            return Ok((p, st));
        }
    }
    if !t.decodable() {
        return Err(anyhow!("tile mode {} / format {} not supported", t.tile_mode(), t.dxgi_format));
    }
    let (data, skipped) = store.data_ex(t)?;
    // Top mips in BCPack chunks that did not decode: fall back to the largest
    // mip outside all BCPack chunks (a BC1 chunk that fails counts too).
    let mip = if skipped == 0 {
        0
    } else {
        match store.best_mip_where(t, is_bcpack) {
            Some(m) => m,
            None => return Err(anyhow!("{skipped} bytes in BCPack chunks, no mip outside them (Xbox hardware texture codec; only BC1 decodes so far)")),
        }
    };
    let (rgba, w, h) = decode_rgba(t, &data, mip)?;
    let mut st = stats(&rgba, w, h);
    st.mip = mip;
    st.mips = Some(t.mips());
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{:016x}.{:?}.tmp.png", t.hash, std::thread::current().id()).replace(['(', ')'], ""));
    crate::texture::save_png(&tmp, &rgba, w, h)?;
    std::fs::rename(&tmp, &p)?;
    std::fs::write(&js, serde_json::to_vec(&st)?)?;
    Ok((p, st))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tex(d0: u32, d1: u32, virt: u64) -> TtpfTex {
        let mut t = TtpfTex { hash: virt, width: (d0 & 0x3FFF) + 1, height: ((d0 >> 14) & 0x3FFF) + 1, dxgi_format: (d1 >> 3) & 0x7F, virt, size: 0, d0, d1, loc: 0, cube_base: None, mips_fixed: None };
        t.size = t.layout().map(|l| l.total).unwrap_or(0);
        t
    }

    #[test]
    fn mip_count_from_store_spacing() {
        // 1024x1024 BC1, tile mode 1, mip field 0 (descriptor says 1 mip),
        // followed by the next texture 688128 bytes later = 3 mips.
        let mut v = vec![tex(0x00ffc3ff, 0x0120023b, 0x1000), tex(0x00ffc3ff, 0x0120023b, 0x1000 + 688128)];
        assert_eq!(v[0].mips(), 1);
        fix_mip_counts(&mut v, None);
        assert_eq!((v[0].mips(), v[0].size), (3, 688128));
        // the last texture has no neighbour and keeps the descriptor count
        assert_eq!(v[1].mips_fixed, None);
    }

    #[test]
    fn every_bcpack_codec_decodes() {
        // flags byte = codec | (4 KiB page bits << 6)
        assert!(!is_bcpack(0x20) && !is_bcpack(0x00) && !is_bcpack(0xA0));
        assert!(is_bcpack(0x04) && !is_undecoded(0x04) && !is_undecoded(0xC4));
        assert!(is_bcpack(0x0C) && !is_undecoded(0x0C) && !is_undecoded(0x4C));
        assert!(!is_undecoded(0x1C) && !is_undecoded(0x5C));
        assert!(!is_undecoded(0x10) && !is_undecoded(0x14) && !is_undecoded(0x54));
        assert!(!is_undecoded(0x18) && !is_undecoded(0x58));
    }

    #[test]
    fn bc1_chunk_decodes_to_blocks() {
        let (chunk, _) = bcpack::encode::synthetic_bc1(7, 512, 200);
        assert_eq!(decode_bc1(&chunk, 512).map(|b| b.len()), Some(512 * 8));
        // a wrong block count does not converge
        assert_eq!(decode_bc1(&chunk, 1024), None);
    }

    /// d0 / d1 of a synthetic descriptor.
    fn desc(w: u32, h: u32, field: u32, fmt: u32, slices: u32, kind: u32, tile: u32) -> (u32, u32) {
        ((w - 1) | (h - 1) << 14 | field << 28, fmt << 3 | (slices - 1) << 10 | kind << 20 | tile << 24)
    }

    #[test]
    fn cube_mip_count_uses_copies() {
        // 64x64 BC7 cube, field 6, 2 copies: 7 mips, down to 1x1.
        let (d0, d1) = desc(64, 64, 6, 98, 6, 4, 10);
        let t = tex(d0, d1, 0x1000);
        assert_eq!(t.mips(), 7);
        let l = t.layout().unwrap();
        assert_eq!((l.mips.len(), l.mips[6].w, l.mips[6].h), (7, 1, 1));
        // one copy: the field is the count
        let (d0, d1) = desc(32, 32, 6, 98, 6, 4, 9);
        assert_eq!(tex(d0, d1, 0x1000).mips(), 6);
        // 128x128, field 2, 3 copies: 4 mips
        let (d0, d1) = desc(128, 128, 2, 98, 6, 4, 11);
        assert_eq!(tex(d0, d1, 0x1000).mips(), 4);
    }

    #[test]
    fn small_single_mip_decodes_from_offset_0() {
        // 16x8 RGBA8, tile mode 1, mip field 1: one mip at offset 0.
        let (d0, d1) = desc(16, 8, 1, 28, 1, 2, 1);
        let t = tex(d0, d1, 0x1000);
        assert_eq!(t.mips(), 1);
        let l = t.layout().unwrap();
        let mut data = vec![0u8; l.total as usize];
        for y in 0..8 {
            for x in 0..16 {
                let a = l.addr(0, 0, x, y) as usize;
                data[a..a + 4].copy_from_slice(&[x as u8 * 16, y as u8 * 32, 7, 255]);
            }
        }
        // the mip covers 256-byte blocks 0 and 2 of the 4 KiB block, as stored
        assert!(data[256..512].iter().chain(&data[768..]).all(|&b| b == 0));
        let (rgba, w, h) = decode_rgba(&t, &data, 0).unwrap();
        assert_eq!((w, h), (16, 8));
        assert_eq!(&rgba[(5 * 16 + 9) * 4..][..4], &[144, 160, 7, 255]);
        // other tile modes keep field + 1
        let (d0, d1) = desc(16, 8, 1, 28, 1, 2, 2);
        assert_eq!(tex(d0, d1, 0x1000).mips(), 2);
    }

    #[test]
    fn volumes_use_the_3d_layout() {
        // 32x64x32 R16G16B16A16_FLOAT, one mip: 3D, 524288 bytes
        let (d0, d1) = desc(32, 64, 1, 10, 32, 6, 1);
        let t = tex(d0, d1, 0x1000);
        assert!(t.is_3d());
        let l = t.layout().unwrap();
        assert_eq!((l.block_d, l.slices, t.size), (8, 32, 524288));
        assert_eq!(t.tex_info("").depth, 32);
        // element sizes not measured on a volume, and multi-mip volumes, are rejected
        let (d0, d1) = desc(32, 32, 1, 28, 32, 6, 1);
        assert!(tex(d0, d1, 0x1000).layout().is_none());
        let (d0, d1) = desc(32, 32, 2, 10, 32, 6, 1);
        assert!(tex(d0, d1, 0x1000).layout().is_none());
    }
}
