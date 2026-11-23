#![allow(dead_code)]
//! CPU side of the model viewer: read an exported `model*.glb` (the layout
//! `sixng_core::gltf::mesh_glb` writes) plus the material extras, and decode
//! the referenced PNGs with a mip chain. Runs on a worker thread.
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Default)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub tan: [f32; 4],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub col: [u8; 4],
    pub joints: [u16; 4],
    pub weights: [u8; 4],
    /// Tint palette colour (GLB COLOR_0, linear); white = no tint.
    pub tint: [u8; 4],
}

pub struct Mesh {
    pub name: String,
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material: Option<usize>,
    pub skinned: bool,
    pub has_tangents: bool,
    pub uv_sets: usize,
    pub centre: [f32; 3],
}

/// One texture slot of a material, as written in the material extras.
#[derive(Clone)]
pub struct Slot {
    pub role: String,
    pub param_hash: String,
    pub hash: String,
    pub png: Option<String>,
    /// ok | lowmip | flat | missing | other
    pub status: String,
    pub mip: u32,
    pub size: [u32; 2],
    pub format: String,
}

#[derive(Clone)]
pub struct Material {
    pub name: String,
    pub class: String,
    pub alpha_mode: String,
    pub base_factor: [f32; 4],
    pub base: Option<usize>,
    pub normal: Option<usize>,
    pub packed: Option<usize>,
    pub emissive: Option<usize>,
    /// Metallic and roughness factors of a stand-in surface (tyre rubber,
    /// lamp-reflector chrome); None = the viewer's defaults.
    pub mr: Option<[f32; 2]>,
    /// The base texture is a tint mask (animal coats): its green channel
    /// (shading) is shown as grey.
    pub tint_mask: bool,
    pub slots: Vec<Slot>,
    /// worst slot status: ok / lowmip / flat
    pub gap: String,
    pub shader_hash: String,
    /// Decal layer order when the material is in the decal bucket.
    pub decal: Option<u8>,
}

pub struct Joint {
    pub name: String,
    pub parent: Option<usize>,
    /// local transform (column-major)
    pub local: [f32; 16],
    pub ibm: [f32; 16],
}

pub struct Image {
    pub key: String,
    /// mip chain RGBA8, level 0 first
    pub mips: Vec<(u32, u32, Vec<u8>)>,
    pub srgb: bool,
    /// Block-compressed copy of the chain (the 3D map); when set, `mips`
    /// keeps only the level sizes.
    pub bc: Option<crate::bc::Bc>,
}

impl Image {
    /// Pixel bytes held (block-compressed if compressed).
    pub fn bytes(&self) -> usize {
        match &self.bc {
            Some(b) => b.levels.iter().map(|l| l.len()).sum(),
            None => self.mips.iter().map(|m| m.2.len()).sum(),
        }
    }

    /// Replace the RGBA chain by BC1 / BC3 blocks when the size allows.
    pub fn compress(&mut self) {
        if self.bc.is_some() || self.mips.is_empty() {
            return;
        }
        if let Some(b) = crate::bc::compress(&self.mips) {
            self.bc = Some(b);
            for m in &mut self.mips {
                m.2 = Vec::new();
            }
        }
    }
}

pub struct Model {
    pub path: PathBuf,
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    pub joints: Vec<Joint>,
    pub images: Vec<Image>,
    pub bbox: [[f32; 3]; 2],
    pub triangles: usize,
    pub vertices: usize,
    pub extras: Value,
}

fn rd_f32(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

struct Doc<'a> {
    j: Value,
    bin: &'a [u8],
}

impl<'a> Doc<'a> {
    /// Raw elements of an accessor as f32 (normalised for u8/u16).
    fn floats(&self, acc: usize) -> Result<(Vec<f32>, usize)> {
        let a = &self.j["accessors"][acc];
        let n = a["count"]
            .as_u64()
            .ok_or_else(|| anyhow!("accessor count"))? as usize;
        let comps = match a["type"].as_str().unwrap_or("") {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            t => return Err(anyhow!("accessor type {t}")),
        };
        let ct = a["componentType"].as_u64().unwrap_or(0);
        let norm = a["normalized"].as_bool().unwrap_or(false);
        let bv = &self.j["bufferViews"][a["bufferView"].as_u64().unwrap_or(0) as usize];
        let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize
            + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let csz = match ct {
            5126 | 5125 => 4,
            5123 | 5122 => 2,
            5121 | 5120 => 1,
            _ => return Err(anyhow!("component type {ct}")),
        };
        let stride = bv["byteStride"]
            .as_u64()
            .map(|s| s as usize)
            .unwrap_or(comps * csz);
        let mut out = Vec::with_capacity(n * comps);
        for i in 0..n {
            for c in 0..comps {
                let o = off + i * stride + c * csz;
                let v = match ct {
                    5126 => rd_f32(self.bin, o),
                    5125 => u32::from_le_bytes(self.bin[o..o + 4].try_into()?) as f32,
                    5123 => {
                        let x = u16::from_le_bytes(self.bin[o..o + 2].try_into()?) as f32;
                        if norm {
                            x / 65535.0
                        } else {
                            x
                        }
                    }
                    5121 => {
                        let x = self.bin[o] as f32;
                        if norm {
                            x / 255.0
                        } else {
                            x
                        }
                    }
                    _ => 0.0,
                };
                out.push(v);
            }
        }
        Ok((out, comps))
    }
    fn u32s(&self, acc: usize) -> Result<Vec<u32>> {
        let a = &self.j["accessors"][acc];
        let n = a["count"].as_u64().unwrap_or(0) as usize;
        let ct = a["componentType"].as_u64().unwrap_or(0);
        let bv = &self.j["bufferViews"][a["bufferView"].as_u64().unwrap_or(0) as usize];
        let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize
            + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        Ok((0..n)
            .map(|i| match ct {
                5125 => {
                    u32::from_le_bytes(self.bin[off + 4 * i..off + 4 * i + 4].try_into().unwrap())
                }
                5123 => {
                    u16::from_le_bytes(self.bin[off + 2 * i..off + 2 * i + 2].try_into().unwrap())
                        as u32
                }
                _ => self.bin[off + i] as u32,
            })
            .collect())
    }
    /// JOINTS_0 as u16 x 4 (componentType 5121 or 5123).
    fn joints4(&self, acc: usize) -> Result<Vec<[u16; 4]>> {
        let a = &self.j["accessors"][acc];
        if a["componentType"].as_u64() != Some(5123) {
            return Ok(self
                .u8x4(acc)?
                .into_iter()
                .map(|c| c.map(|x| x as u16))
                .collect());
        }
        let n = a["count"].as_u64().unwrap_or(0) as usize;
        let bv = &self.j["bufferViews"][a["bufferView"].as_u64().unwrap_or(0) as usize];
        let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize
            + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let r = |o: usize| u16::from_le_bytes([self.bin[o], self.bin[o + 1]]);
        Ok((0..n)
            .map(|i| {
                let o = off + 8 * i;
                [r(o), r(o + 2), r(o + 4), r(o + 6)]
            })
            .collect())
    }
    fn u8x4(&self, acc: usize) -> Result<Vec<[u8; 4]>> {
        let a = &self.j["accessors"][acc];
        let n = a["count"].as_u64().unwrap_or(0) as usize;
        let bv = &self.j["bufferViews"][a["bufferView"].as_u64().unwrap_or(0) as usize];
        let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize
            + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        Ok((0..n)
            .map(|i| self.bin[off + 4 * i..off + 4 * i + 4].try_into().unwrap())
            .collect())
    }
}

/// Split a GLB file into (json, bin).
pub fn split_glb(b: &[u8]) -> Result<(Value, &[u8])> {
    if b.len() < 20 || &b[0..4] != b"glTF" {
        return Err(anyhow!("not a GLB"));
    }
    let jl = u32::from_le_bytes(b[12..16].try_into()?) as usize;
    let j: Value = serde_json::from_slice(&b[20..20 + jl])?;
    let bo = 20 + jl;
    let bin = if b.len() >= bo + 8 {
        let bl = u32::from_le_bytes(b[bo..bo + 4].try_into()?) as usize;
        &b[bo + 8..(bo + 8 + bl).min(b.len())]
    } else {
        &b[0..0]
    };
    Ok((j, bin))
}

/// Slot role for the viewer, from the material extras' texture entry.
fn slot_role(t: &Value) -> String {
    let ph = t["param_hash"].as_str().unwrap_or("");
    match ph {
        "c8e8c282" => "base_colour".into(),
        "1e77bf4c" | "84c69350" | "58b6e707" => "normal".into(),
        "739e3d0d" => "packed_rma".into(),
        "0df47048" => "detail_colour".into(),
        "c6f46e79" => "detail_normal_or_mask".into(),
        "4d56d7fe" | "2bb2ede8" => "normal_atlas".into(),
        "de84c011" | "4c759bf1" | "27dbd2be" | "7e1f7f44" | "1a016c7e" => "layer_normal".into(),
        "c075f9b6" => "layer_blend_mask".into(),
        _ => {
            if t["looks_like_normal_map"].as_bool().unwrap_or(false) {
                "normal".into()
            } else {
                match t["role_guess"].as_str().unwrap_or("") {
                    "colour (sRGB)" => "colour".into(),
                    "base_color" => "base_colour".into(),
                    "normal" => "normal".into(),
                    "data (linear)" => "data".into(),
                    _ => "other".into(),
                }
            }
        }
    }
}

fn slot_status(t: &Value) -> (String, u32) {
    let st = t["status"].as_str().unwrap_or("");
    let mip = t["mip_used"].as_u64().unwrap_or(0) as u32;
    if st.starts_with("ok") {
        (
            if mip > 0 {
                "lowmip".into()
            } else {
                "ok".into()
            },
            mip,
        )
    } else if st.contains("BCPack") {
        ("flat".into(), 0)
    } else if st.contains("not in any") {
        ("missing".into(), 0)
    } else {
        ("other".into(), 0)
    }
}

/// Half-size box filter of an RGBA8 image (odd edges clamp).
fn half(pw: u32, ph: u32, p: &[u8]) -> (u32, u32, Vec<u8>) {
    let (nw, nh) = ((pw / 2).max(1) as usize, (ph / 2).max(1) as usize);
    let (pw, ph) = (pw as usize, ph as usize);
    let mut o = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        let (y0, y1) = ((y * 2).min(ph - 1), (y * 2 + 1).min(ph - 1));
        let r0 = &p[y0 * pw * 4..(y0 + 1) * pw * 4];
        let r1 = &p[y1 * pw * 4..(y1 + 1) * pw * 4];
        let orow = &mut o[y * nw * 4..(y + 1) * nw * 4];
        for x in 0..nw {
            let (x0, x1) = ((x * 2).min(pw - 1) * 4, (x * 2 + 1).min(pw - 1) * 4);
            for c in 0..4 {
                orow[x * 4 + c] = ((r0[x0 + c] as u32
                    + r0[x1 + c] as u32
                    + r1[x0 + c] as u32
                    + r1[x1 + c] as u32
                    + 2)
                    >> 2) as u8;
            }
        }
    }
    (nw as u32, nh as u32, o)
}

/// Box-filter mip chain of an RGBA8 image.
pub fn mip_chain(w: u32, h: u32, px: Vec<u8>) -> Vec<(u32, u32, Vec<u8>)> {
    let mut v = vec![(w, h, px)];
    loop {
        let (pw, ph, p) = v.last().unwrap();
        if *pw == 1 && *ph == 1 {
            break;
        }
        let n = half(*pw, *ph, p);
        v.push(n);
    }
    v
}

/// Size of the reduced texture copies kept for distant models.
pub const SMALL_TEX: u32 = 256;
/// Bytes in `<cache>/lowtex` (set by the 3D map at start, then counted up)
/// and the cap above which no further copies are written.
pub static LOWTEX_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub const LOWTEX_CAP: u64 = 4 << 30;
/// Texture loader counters: reduced-copy hits, full decodes, full-decode µs, full-decode megapixels.
/// [4]: µs in the whole image step (reads, reductions, mip chains, cache writes).
pub static TEX_T: [std::sync::atomic::AtomicU64; 5] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 5];

/// Smaller reduced copies (<= TINY_TEX px) for the first, distant loads of
/// the 3D map, in `<cache>/lowtex128`, with their own byte count and cap.
pub const TINY_TEX: u32 = 128;
pub static LOWTEX128_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub const LOWTEX128_CAP: u64 = 2 << 30;

/// Block-compressed 64 px copies (with their mips) for the 3D map's first
/// loads, in `<cache>/bc64`: a few KB each, read straight into GPU format
/// (no PNG decode, no compression on load). Own byte count and cap.
pub const BC_TEX: u32 = 64;
pub static BC64_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub const BC64_CAP: u64 = 1 << 30;

/// Read a `bc64` copy: "SXB1", alpha u8, level count u8, then per level
/// width u32, height u32, byte length u32 and the blocks.
fn read_bc(sp: &Path) -> Option<(Vec<(u32, u32, Vec<u8>)>, crate::bc::Bc)> {
    let b = std::fs::read(sp).ok()?;
    if b.len() < 6 || &b[..4] != b"SXB1" {
        return None;
    }
    let alpha = b[4] != 0;
    let n = b[5] as usize;
    let (mut o, mut dims, mut levels) = (6usize, Vec::with_capacity(n), Vec::with_capacity(n));
    let u =
        |o: usize| -> Option<u32> { Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?)) };
    for _ in 0..n {
        let (w, h, len) = (u(o)?, u(o + 4)?, u(o + 8)? as usize);
        o += 12;
        levels.push(b.get(o..o + len)?.to_vec());
        o += len;
        dims.push((w, h, Vec::new()));
    }
    (n > 0).then_some((dims, crate::bc::Bc { alpha, levels }))
}

fn write_bc(sp: &Path, mips: &[(u32, u32, Vec<u8>)], bc: &crate::bc::Bc) {
    let mut out = b"SXB1".to_vec();
    out.push(bc.alpha as u8);
    out.push(bc.levels.len() as u8);
    for (m, l) in mips.iter().zip(&bc.levels) {
        for v in [m.0, m.1, l.len() as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(l);
    }
    let _ = std::fs::create_dir_all(sp.parent().unwrap());
    let tmp = sp.with_extension("tmp");
    if std::fs::write(&tmp, &out).is_ok() {
        BC64_BYTES.fetch_add(out.len() as u64, std::sync::atomic::Ordering::Relaxed);
        let _ = std::fs::rename(&tmp, sp);
    }
}

/// Where the reduced copy of a texture lives (`<cache>/<dir>/<hash>.png`).
fn small_path_in(out_root: &Path, dir: &str, p: &Path) -> PathBuf {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in p.to_string_lossy().as_bytes() {
        h = (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    out_root.join(dir).join(format!("{h:016x}.png"))
}

/// Halve an RGBA8 image until its longer side is at most `size`.
fn reduce_to(mut img: (u32, u32, Vec<u8>), size: u32) -> (u32, u32, Vec<u8>) {
    while img.0.max(img.1) > size.max(1) {
        img = half(img.0, img.1, &img.2);
    }
    img
}

/// Write a reduced copy (best effort, via a temporary file) and count its bytes.
fn write_small(sp: &Path, img: &(u32, u32, Vec<u8>), counter: &std::sync::atomic::AtomicU64) {
    let _ = std::fs::create_dir_all(sp.parent().unwrap());
    let tmp = sp.with_extension("tmp");
    if image::save_buffer_with_format(
        &tmp,
        &img.2,
        img.0,
        img.1,
        image::ExtendedColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .is_ok()
    {
        let n = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        counter.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
        let _ = std::fs::rename(&tmp, sp);
    }
}

fn load_png(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some((w, h, img.into_raw()))
}

/// Load an exported GLB with its textures. `out_root` is the export root
/// (texture `png` paths in the extras are relative to it).
/// Loader options for the 3D map: cap texture size and skip images that are
/// already on the GPU (by key; they come back with empty `mips`).
#[derive(Default, Clone)]
pub struct LoadOpts {
    pub max_tex: u32,
    pub skip_keys: Option<std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>>,
    pub no_textures: bool,
    /// Read and write reduced copies (<= SMALL_TEX px) under `<cache>/lowtex`
    /// when `max_tex` is at most that size: far faster than decoding the
    /// full-size PNG every time.
    pub small_cache: bool,
    /// Drop (discard) materials whose every decoded texture is a tiny (<= 8 px)
    /// placeholder; used by the 3D map (see `placeholder_only`).
    pub hide_placeholders: bool,
    /// Base colour (and emissive) only: no normal or packed-data maps. The
    /// 3D map's first, distant loads use this; sharper reloads get all maps.
    pub base_only: bool,
    /// Block-compress textures here and, at `BC_TEX` px, read and write the
    /// `bc64` copies.
    pub bc: bool,
}

pub fn load(path: &Path, out_root: &Path) -> Result<Model> {
    load_with(path, out_root, &LoadOpts::default())
}

pub fn load_with(path: &Path, out_root: &Path, opts: &LoadOpts) -> Result<Model> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let (j, bin) = split_glb(&bytes)?;
    let d = Doc { j, bin };
    let dir = path.parent().unwrap_or(Path::new("."));
    // images
    let mut images: Vec<Image> = Vec::new();
    let mut img_by_key: HashMap<String, usize> = HashMap::new();
    let mut add_img = |p: PathBuf, srgb: bool, images: &mut Vec<Image>| -> Option<usize> {
        // the size cap is part of the key: a sharper copy is a separate texture
        let key = format!("{}|{}|{}", p.display(), srgb, opts.max_tex);
        if let Some(&i) = img_by_key.get(&key) {
            return Some(i);
        }
        if opts.no_textures {
            return None;
        }
        if opts
            .skip_keys
            .as_ref()
            .is_some_and(|s| s.lock().unwrap().contains(&key))
        {
            images.push(Image {
                key: key.clone(),
                mips: Vec::new(),
                srgb,
                bc: None,
            });
            img_by_key.insert(key, images.len() - 1);
            return Some(images.len() - 1);
        }
        use std::sync::atomic::Ordering::Relaxed;
        let t_img = std::time::Instant::now();
        let spbc = (opts.bc && opts.small_cache && opts.max_tex == BC_TEX)
            .then(|| small_path_in(out_root, "bc64", &p).with_extension("bc"));
        if let Some((mips, bc)) = spbc.as_ref().and_then(|sp| read_bc(sp)) {
            TEX_T[0].fetch_add(1, Relaxed);
            TEX_T[4].fetch_add(t_img.elapsed().as_micros() as u64, Relaxed);
            images.push(Image {
                key: key.clone(),
                mips,
                srgb,
                bc: Some(bc),
            });
            img_by_key.insert(key, images.len() - 1);
            return Some(images.len() - 1);
        }
        // reduced copies: <= 128 px (lowtex128) for small requests, <= 256 px
        // (lowtex) otherwise; a 256 px copy also feeds the 128 px one
        let small = opts.small_cache && opts.max_tex > 0 && opts.max_tex <= SMALL_TEX;
        let tiny = small && opts.max_tex <= TINY_TEX;
        let sp128 = tiny.then(|| small_path_in(out_root, "lowtex128", &p));
        let sp256 = small.then(|| small_path_in(out_root, "lowtex", &p));
        let mut img = None;
        let mut tier = 0u32; // size cap of the copy read (0 = original file)
        if let Some(v) = sp128.as_ref().and_then(|sp| load_png(sp)) {
            img = Some(v);
            tier = TINY_TEX;
        } else if let Some(v) = sp256.as_ref().and_then(|sp| load_png(sp)) {
            img = Some(v);
            tier = SMALL_TEX;
        }
        let mut cur = match img {
            Some(v) => {
                TEX_T[0].fetch_add(1, Relaxed);
                v
            }
            None => {
                let t = std::time::Instant::now();
                let v = load_png(&p)?;
                TEX_T[1].fetch_add(1, Relaxed);
                TEX_T[2].fetch_add(t.elapsed().as_micros() as u64, Relaxed);
                TEX_T[3].fetch_add((v.0 as u64 * v.1 as u64) >> 10, Relaxed);
                v
            }
        };
        let orig = cur.0.max(cur.1);
        if tier == 0 && small && orig > SMALL_TEX {
            cur = reduce_to(cur, SMALL_TEX);
            if LOWTEX_BYTES.load(Relaxed) < LOWTEX_CAP {
                write_small(sp256.as_ref().unwrap(), &cur, &LOWTEX_BYTES);
            }
        }
        if tier != TINY_TEX && tiny && cur.0.max(cur.1) > TINY_TEX {
            cur = reduce_to(cur, TINY_TEX);
            if LOWTEX128_BYTES.load(Relaxed) < LOWTEX128_CAP {
                write_small(sp128.as_ref().unwrap(), &cur, &LOWTEX128_BYTES);
            }
        }
        // drop the levels above the size cap before building the chain
        if opts.max_tex > 0 {
            cur = reduce_to(cur, opts.max_tex);
        }
        let mips = mip_chain(cur.0, cur.1, cur.2);
        let mut im = Image {
            key: key.clone(),
            mips,
            srgb,
            bc: None,
        };
        if opts.bc {
            im.compress();
            if let (Some(sp), Some(bc)) = (spbc.as_ref(), im.bc.as_ref()) {
                if BC64_BYTES.load(Relaxed) < BC64_CAP {
                    write_bc(sp, &im.mips, bc);
                }
            }
        }
        TEX_T[4].fetch_add(t_img.elapsed().as_micros() as u64, Relaxed);
        images.push(im);
        img_by_key.insert(key, images.len() - 1);
        Some(images.len() - 1)
    };
    let uri_of = |tex: &Value| -> Option<PathBuf> {
        let t = tex["index"].as_u64()? as usize;
        let src = d.j["textures"][t]["source"].as_u64()? as usize;
        let uri = d.j["images"][src]["uri"].as_str()?;
        Some(dir.join(uri))
    };
    // materials
    let mut materials = Vec::new();
    for m in d.j["materials"].as_array().cloned().unwrap_or_default() {
        let ex = &m["extras"];
        let class = ex["classification"]["class"]
            .as_str()
            .unwrap_or("opaque")
            .to_string();
        let alpha_mode = m["alphaMode"].as_str().unwrap_or("OPAQUE").to_string();
        let base_factor: [f32; 4] =
            serde_json::from_value(m["pbrMetallicRoughness"]["baseColorFactor"].clone())
                .unwrap_or([1.0; 4]);
        // Tiny (<= 8 px) textures are engine defaults/placeholders (e.g. a 4x4
        // all-zero "packed data" map). Read as data they mean AO 0 (black walls)
        // or a flat-black normal, so the packed and normal slots ignore them.
        let tex_list = ex["textures"].as_array().cloned().unwrap_or_default();
        let tiny_png = |t: &Value| -> Option<String> {
            let (w, h) = (t["width"].as_u64()?, t["height"].as_u64()?);
            (w <= 8 && h <= 8)
                .then(|| {
                    t["png"]
                        .as_str()
                        .map(|s| s.rsplit('/').next().unwrap_or(s).to_string())
                })
                .flatten()
        };
        let tiny: std::collections::HashSet<String> =
            tex_list.iter().filter_map(|t| tiny_png(t)).collect();
        let is_tiny = |p: &Path| {
            p.file_name()
                .map_or(false, |f| tiny.contains(&f.to_string_lossy().to_string()))
        };
        let with_png: Vec<&Value> = tex_list.iter().filter(|t| t["png"].is_string()).collect();
        let placeholder_only =
            !with_png.is_empty() && with_png.iter().all(|t| tiny_png(t).is_some());
        let base = uri_of(&m["pbrMetallicRoughness"]["baseColorTexture"])
            .and_then(|p| add_img(p, true, &mut images));
        let normal = uri_of(&m["normalTexture"])
            .filter(|p| !opts.base_only && !is_tiny(p))
            .and_then(|p| add_img(p, false, &mut images));
        let mut slots = Vec::new();
        let mut packed = None;
        // The export names the emissive map when the shader has its own.
        let mut emissive = if class == "emissive" {
            uri_of(&m["emissiveTexture"]).and_then(|p| add_img(p, true, &mut images))
        } else {
            None
        };
        let pbr = &m["pbrMetallicRoughness"];
        let mr = ex["classification"]["stand_in"].as_str().and_then(|_| {
            Some([pbr["metallicFactor"].as_f64()? as f32, pbr["roughnessFactor"].as_f64()? as f32])
        });
        for t in ex["textures"].as_array().cloned().unwrap_or_default() {
            let role = slot_role(&t);
            let (status, mip) = slot_status(&t);
            let png = t["png"].as_str().map(|s| s.to_string());
            if role == "packed_rma" && packed.is_none() && !opts.base_only && tiny_png(&t).is_none()
            {
                if let Some(p) = &png {
                    packed = add_img(out_root.join(p), false, &mut images);
                }
            }
            if class == "emissive"
                && emissive.is_none()
                && (role == "colour" || role == "base_colour")
            {
                if let Some(p) = &png {
                    emissive = add_img(out_root.join(p), true, &mut images);
                }
            }
            slots.push(Slot {
                role,
                param_hash: t["param_hash"].as_str().unwrap_or("").into(),
                hash: t["hash64"].as_str().unwrap_or("").into(),
                png,
                status,
                mip,
                size: [
                    t["width"].as_u64().unwrap_or(0) as u32,
                    t["height"].as_u64().unwrap_or(0) as u32,
                ],
                format: t["format"].as_str().unwrap_or("").into(),
            });
        }
        let gap = if slots.iter().any(|s| s.status == "flat") {
            "flat"
        } else if slots.iter().any(|s| s.status == "lowmip") {
            "lowmip"
        } else {
            "ok"
        };
        let decal = if ex["classification"]["render_bucket"].as_str() == Some("decal") {
            // exports before 2026-11-20 lack the decal block: same rule by class
            Some(
                ex["classification"]["decal"]["order"]
                    .as_u64()
                    .unwrap_or(if class == "terrain_multilayer" { 0 } else { 1 })
                    .min(7) as u8,
            )
        } else {
            None
        };
        let (alpha_mode, base_factor) =
            if opts.hide_placeholders && placeholder_only && class == "opaque" {
                // cutout with alpha 0: discarded by the shader
                (
                    "MASK".to_string(),
                    [base_factor[0], base_factor[1], base_factor[2], 0.0],
                )
            } else {
                (alpha_mode, base_factor)
            };
        materials.push(Material {
            name: m["name"].as_str().unwrap_or("").into(),
            class,
            alpha_mode,
            base_factor,
            base,
            normal,
            packed,
            emissive,
            mr,
            tint_mask: ex["classification"]["tint_mask"] == true,
            slots,
            gap: gap.into(),
            shader_hash: ex["shader_name_hash"].as_str().unwrap_or("").into(),
            decal,
        });
    }
    // skeleton
    let mut joints = Vec::new();
    if let Some(skin) = d.j["skins"].get(0) {
        let jn: Vec<usize> = skin["joints"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_u64().map(|v| v as usize))
                    .collect()
            })
            .unwrap_or_default();
        let ibm = skin["inverseBindMatrices"]
            .as_u64()
            .and_then(|a| d.floats(a as usize).ok())
            .map(|x| x.0)
            .unwrap_or_default();
        let mut parent_of: HashMap<usize, usize> = HashMap::new();
        for (i, n) in d.j["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            for c in n["children"].as_array().cloned().unwrap_or_default() {
                if let Some(c) = c.as_u64() {
                    parent_of.insert(c as usize, i);
                }
            }
        }
        let pos_of: HashMap<usize, usize> = jn.iter().enumerate().map(|(k, &n)| (n, k)).collect();
        for (k, &n) in jn.iter().enumerate() {
            let nd = &d.j["nodes"][n];
            // a matrix, or translation / rotation / scale (a GLB with an
            // animation writes its joints that way)
            let local: [f32; 16] = serde_json::from_value(nd["matrix"].clone()).unwrap_or_else(|_| {
                let v3 = |k: &str, d: [f32; 3]| serde_json::from_value::<[f32; 3]>(nd[k].clone()).unwrap_or(d);
                let q: [f32; 4] = serde_json::from_value(nd["rotation"].clone()).unwrap_or([0.0, 0.0, 0.0, 1.0]);
                glam::Mat4::from_scale_rotation_translation(
                    glam::Vec3::from(v3("scale", [1.0; 3])),
                    glam::Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize(),
                    glam::Vec3::from(v3("translation", [0.0; 3])),
                )
                .to_cols_array()
            });
            let mut ib = [0f32; 16];
            if ibm.len() >= (k + 1) * 16 {
                ib.copy_from_slice(&ibm[k * 16..k * 16 + 16]);
            } else {
                ib = [
                    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
                ];
            }
            joints.push(Joint {
                name: nd["name"].as_str().unwrap_or("").into(),
                parent: parent_of.get(&n).and_then(|p| pos_of.get(p).copied()),
                local,
                ibm: ib,
            });
        }
    }
    // meshes
    let mut meshes = Vec::new();
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    let mut tris = 0;
    let mut nverts = 0;
    for (mi, mesh) in d.j["meshes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let skinned = d.j["nodes"]
            .as_array()
            .map(|ns| {
                ns.iter()
                    .any(|n| n["mesh"].as_u64() == Some(mi as u64) && n.get("skin").is_some())
            })
            .unwrap_or(false);
        for p in mesh["primitives"].as_array().cloned().unwrap_or_default() {
            if p["mode"].as_u64().unwrap_or(4) != 4 {
                continue;
            }
            let at = &p["attributes"];
            let Some(pa) = at["POSITION"].as_u64() else {
                continue;
            };
            let (pos, _) = d.floats(pa as usize)?;
            let n = pos.len() / 3;
            let mut v = vec![Vertex::default(); n];
            for i in 0..n {
                v[i].pos = [pos[3 * i], pos[3 * i + 1], pos[3 * i + 2]];
                v[i].col = [255; 4];
                v[i].tint = [255; 4];
                v[i].weights = [255, 0, 0, 0];
                for k in 0..3 {
                    lo[k] = lo[k].min(v[i].pos[k]);
                    hi[k] = hi[k].max(v[i].pos[k]);
                }
            }
            if let Some(a) = at["NORMAL"].as_u64() {
                let (x, _) = d.floats(a as usize)?;
                for i in 0..n.min(x.len() / 3) {
                    v[i].nrm = [x[3 * i], x[3 * i + 1], x[3 * i + 2]];
                }
            }
            let has_tangents = at["TANGENT"].as_u64().is_some();
            if let Some(a) = at["TANGENT"].as_u64() {
                let (x, _) = d.floats(a as usize)?;
                for i in 0..n.min(x.len() / 4) {
                    v[i].tan = [x[4 * i], x[4 * i + 1], x[4 * i + 2], x[4 * i + 3]];
                }
            }
            let mut uv_sets = 0;
            for (k, key) in ["TEXCOORD_0", "TEXCOORD_1"].iter().enumerate() {
                if let Some(a) = at[*key].as_u64() {
                    uv_sets += 1;
                    let (x, _) = d.floats(a as usize)?;
                    for i in 0..n.min(x.len() / 2) {
                        if k == 0 {
                            v[i].uv0 = [x[2 * i], x[2 * i + 1]];
                        } else {
                            v[i].uv1 = [x[2 * i], x[2 * i + 1]];
                        }
                    }
                }
            }
            if at
                .as_object()
                .map(|o| o.keys().any(|k| k == "TEXCOORD_2"))
                .unwrap_or(false)
            {
                uv_sets += at
                    .as_object()
                    .unwrap()
                    .keys()
                    .filter(|k| {
                        k.starts_with("TEXCOORD_") && *k != "TEXCOORD_0" && *k != "TEXCOORD_1"
                    })
                    .count();
            }
            if let Some(a) = at["_COLOR0"].as_u64() {
                let c = d.u8x4(a as usize)?;
                for i in 0..n.min(c.len()) {
                    v[i].col = c[i];
                }
            }
            if let Some(a) = at["COLOR_0"].as_u64() {
                let c = d.u8x4(a as usize)?;
                for i in 0..n.min(c.len()) {
                    v[i].tint = c[i];
                }
            }
            if let (true, Some(a), Some(w)) =
                (skinned, at["JOINTS_0"].as_u64(), at["WEIGHTS_0"].as_u64())
            {
                let js = d.joints4(a as usize)?;
                let ws = d.u8x4(w as usize)?;
                for i in 0..n.min(js.len()) {
                    v[i].joints = js[i];
                    v[i].weights = ws[i];
                }
            }
            let indices = match p["indices"].as_u64() {
                Some(a) => d.u32s(a as usize)?,
                None => (0..n as u32).collect(),
            };
            let indices: Vec<u32> = indices.into_iter().filter(|&i| (i as usize) < n).collect();
            if at["NORMAL"].as_u64().is_none() {
                // Some meshes (e.g. SLOD ground tiles) ship without normals:
                // derive area-weighted smooth normals from the faces.
                let mut acc = vec![[0f32; 3]; n];
                for t in indices.chunks_exact(3) {
                    let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
                    let (pa, pb, pc) = (v[a].pos, v[b].pos, v[c].pos);
                    let e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
                    let e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
                    let fnrm = [
                        e1[1] * e2[2] - e1[2] * e2[1],
                        e1[2] * e2[0] - e1[0] * e2[2],
                        e1[0] * e2[1] - e1[1] * e2[0],
                    ];
                    for i in [a, b, c] {
                        for k in 0..3 {
                            acc[i][k] += fnrm[k];
                        }
                    }
                }
                for (x, s) in v.iter_mut().zip(acc) {
                    let l = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
                    x.nrm = if l > 0.0 {
                        [s[0] / l, s[1] / l, s[2] / l]
                    } else {
                        [0.0, 1.0, 0.0]
                    };
                }
            }
            tris += indices.len() / 3;
            nverts += n;
            let mut c = [0f32; 3];
            for x in &v {
                for k in 0..3 {
                    c[k] += x.pos[k] / n.max(1) as f32;
                }
            }
            meshes.push(Mesh {
                name: mesh["name"].as_str().unwrap_or("").into(),
                verts: v,
                indices,
                material: p["material"].as_u64().map(|m| m as usize),
                skinned: skinned && !joints.is_empty(),
                has_tangents,
                uv_sets,
                centre: c,
            });
        }
    }
    if meshes.is_empty() {
        lo = [-1.0; 3];
        hi = [1.0; 3];
    }
    Ok(Model {
        path: path.to_path_buf(),
        meshes,
        materials,
        joints,
        images,
        bbox: [lo, hi],
        triangles: tris,
        vertices: nverts,
        extras: d.j["scenes"][0]["extras"].clone(),
    })
}

/// Append another model's meshes, materials and images (for animals and
/// characters assembled from parts). The appended meshes are drawn in their
/// bind pose (not skinned to this model's joints).
pub fn merge(into: &mut Model, other: Model) {
    let empty = into.meshes.is_empty();
    let img0 = into.images.len();
    let mat0 = into.materials.len();
    into.images.extend(other.images);
    for mut m in other.materials {
        for x in [&mut m.base, &mut m.normal, &mut m.packed, &mut m.emissive] {
            if let Some(i) = x {
                *i += img0;
            }
        }
        into.materials.push(m);
    }
    for mut me in other.meshes {
        me.material = me.material.map(|i| i + mat0);
        me.skinned = false;
        into.meshes.push(me);
    }
    if empty {
        into.bbox = other.bbox;
    } else {
        for k in 0..3 {
            into.bbox[0][k] = into.bbox[0][k].min(other.bbox[0][k]);
            into.bbox[1][k] = into.bbox[1][k].max(other.bbox[1][k]);
        }
    }
    into.triangles += other.triangles;
    into.vertices += other.vertices;
}

/// One animated joint property of a GLB animation: path 0 = translation,
/// 1 = rotation (x, y, z, w), 2 = scale; linear keys.
pub struct AnimChannel {
    pub joint: usize,
    pub path: u8,
    pub times: Vec<f32>,
    pub values: Vec<[f32; 4]>,
}

pub struct Anim {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<AnimChannel>,
}

impl AnimChannel {
    /// Value at time `t` (clamped to the keys; rotations take the shorter arc).
    pub fn sample(&self, t: f32) -> [f32; 4] {
        let n = self.times.len().min(self.values.len());
        if n == 0 {
            return [0.0, 0.0, 0.0, 1.0];
        }
        let i = self.times[..n].partition_point(|&x| x <= t);
        if i == 0 {
            return self.values[0];
        }
        if i >= n {
            return self.values[n - 1];
        }
        let (a, b) = (self.values[i - 1], self.values[i]);
        let f = ((t - self.times[i - 1]) / (self.times[i] - self.times[i - 1]).max(1e-9)).clamp(0.0, 1.0);
        if self.path == 1 {
            let s = if a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3] < 0.0 { -1.0 } else { 1.0 };
            let q = [0, 1, 2, 3].map(|k| a[k] + (s * b[k] - a[k]) * f);
            let l = q.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
            q.map(|x| x / l)
        } else {
            [0, 1, 2, 3].map(|k| a[k] + (b[k] - a[k]) * f)
        }
    }
}

/// The animations of a GLB whose channels drive joints of its first skin
/// (the roof clip of a convertible). Joint indices are skin joint indices,
/// as in `Model::joints`.
pub fn load_anims(path: &Path) -> Result<Vec<Anim>> {
    let data = std::fs::read(path)?;
    if data.len() < 20 || &data[0..4] != b"glTF" {
        return Err(anyhow!("not a GLB"));
    }
    let jl = u32::from_le_bytes(data[12..16].try_into()?) as usize;
    let j: Value = serde_json::from_slice(&data[20..20 + jl])?;
    let bin = data.get(20 + jl + 8..).unwrap_or(&[]);
    let d = Doc { j, bin };
    let skin: Vec<u64> = d.j["skins"][0]["joints"].as_array().map(|a| a.iter().filter_map(|x| x.as_u64()).collect()).unwrap_or_default();
    let mut out = Vec::new();
    for an in d.j["animations"].as_array().cloned().unwrap_or_default() {
        let mut channels = Vec::new();
        for ch in an["channels"].as_array().cloned().unwrap_or_default() {
            let Some(joint) = ch["target"]["node"].as_u64().and_then(|n| skin.iter().position(|&x| x == n)) else { continue };
            let path = match ch["target"]["path"].as_str() {
                Some("translation") => 0,
                Some("rotation") => 1,
                Some("scale") => 2,
                _ => continue,
            };
            let s = &an["samplers"][ch["sampler"].as_u64().unwrap_or(0) as usize];
            let (Some(i), Some(o)) = (s["input"].as_u64(), s["output"].as_u64()) else { continue };
            let (times, _) = d.floats(i as usize)?;
            let (vals, comps) = d.floats(o as usize)?;
            let values = vals.chunks_exact(comps.max(1)).map(|c| [c[0], c.get(1).copied().unwrap_or(0.0), c.get(2).copied().unwrap_or(0.0), c.get(3).copied().unwrap_or(0.0)]).collect();
            channels.push(AnimChannel { joint, path, times, values });
        }
        let duration = channels.iter().filter_map(|c| c.times.last().copied()).fold(0.0f32, f32::max);
        out.push(Anim { name: an["name"].as_str().unwrap_or("").to_string(), duration, channels });
    }
    Ok(out)
}

#[cfg(test)]
mod bench {
    use super::*;
    /// Per-texture cost of a first (64 px) load: 128 px PNG copy vs `bc64`.
    /// `SIXNG_WORK=<work> cargo test --release bench_first_tex -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_first_tex() {
        let Some(work) = std::env::var_os("SIXNG_WORK") else {
            return;
        };
        let root = Path::new(&work).join(crate::GUI_CACHE);
        let take = |d: &str| -> Vec<PathBuf> {
            std::fs::read_dir(root.join(d))
                .map(|r| r.filter_map(|e| Some(e.ok()?.path())).take(2000).collect())
                .unwrap_or_default()
        };
        let t = std::time::Instant::now();
        let pngs = take("lowtex128");
        let mut n = 0;
        for p in &pngs {
            if let Some(v) = load_png(p) {
                let v = reduce_to(v, BC_TEX);
                let mut im = Image {
                    key: String::new(),
                    mips: mip_chain(v.0, v.1, v.2),
                    srgb: true,
                    bc: None,
                };
                im.compress();
                n += 1;
            }
        }
        let a = t.elapsed().as_secs_f64() * 1000.0 / n.max(1) as f64;
        let t = std::time::Instant::now();
        let bcs = take("bc64");
        let m = bcs.iter().filter(|p| read_bc(p).is_some()).count();
        let b = t.elapsed().as_secs_f64() * 1000.0 / m.max(1) as f64;
        println!("png128 -> 64 px BC: {a:.3} ms per texture ({n}); bc64 read: {b:.3} ms per texture ({m})");
    }
}
