#![allow(dead_code)]
//! Map baker (`sixng-gui --bake-map`): turns the world exports in
//! `<work>/out` (collision, navmesh, lane graph, lights, batches) and the
//! full placements into `<work>/map`:
//!
//! * `meta.json`: world box, cell size, tile pyramid, layer list;
//! * `tiles/<layer>/<z>/<x>_<y>.png`: 256 px raster tiles (north up). At
//!   z = zmax one pixel is one 2 m cell; each lower level halves.
//!   Fully transparent tiles are not written;
//! * `heights_8m.bin`: f32 top-surface height per 8 m cell (row 0 = north);
//! * `roads.json`, `junctions.json`: lane graph for vector drawing;
//! * `lights.bin` (32 B per light), `entities.bin` (48 B per placement),
//!   `strings.json` (names, model ids, map files), `interiors.json`.
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};

pub const CELL: f32 = 2.0;
pub const COARSE: usize = 4; // coarse grid = 8 m
pub const TILE: usize = 256;
pub const X0: f32 = -10240.0;
pub const Y1: f32 = 10496.0;
pub const W: usize = 7936; // 15.9 km
pub const H: usize = 9600; // 19.2 km

pub const LAYERS: &[(&str, &str)] = &[
    ("terrain", "Terrain: top collision surface, hill-shaded and tinted by height; water/no data blue"),
    ("surfaces", "Collision surface categories (inferred, see surface_categories.json)"),
    ("buildings", "Building footprints (HD placements of building-sized models)"),
    ("navmesh", "Navmesh: road polygons (orange) and the other navmesh polygons the export kept (blue)"),
    ("density", "Vegetation (green) and prop (magenta) instance density from batches and placements"),
    ("gaps", "Texture-gap heatmap: share of placed model area whose textures are flat (red) or low-mip (amber) in BCPack"),
    ("lod", "LOD regions: footprint of LOD / SLOD1-4 models (darker = coarser)"),
    ("water", "Water / no-collision cells"),
];

pub fn log(work: &Path, msg: &str) {
    let p = work.join("map").join("bake.log");
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
    {
        let _ = writeln!(f, "{} {msg}", chrono_like());
    }
    eprintln!("{msg}");
}

fn chrono_like() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!(
        "[{:02}:{:02}:{:02}]",
        (t / 3600) % 24,
        (t / 60) % 60,
        t % 60
    )
}

#[inline]
pub fn cell_of(x: f32, y: f32) -> (f32, f32) {
    ((x - X0) / CELL, (Y1 - y) / CELL)
}

fn hq(z: f32) -> u64 {
    (((z + 1000.0).clamp(0.0, 8000.0)) * 16.0) as u64 + 1
}
pub fn unhq(v: u64) -> Option<f32> {
    let h = v >> 16;
    if h == 0 {
        None
    } else {
        Some((h - 1) as f32 / 16.0 - 1000.0)
    }
}

/// Rasterise a triangle (cell-space x/y, world z) into cells, calling f(c, r, z).
fn raster_tri(p: [[f32; 3]; 3], mut f: impl FnMut(usize, usize, f32)) {
    let minx = p
        .iter()
        .map(|v| v[0])
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0) as i64;
    let maxx = p
        .iter()
        .map(|v| v[0])
        .fold(f32::MIN, f32::max)
        .ceil()
        .min(W as f32 - 1.0) as i64;
    let miny = p
        .iter()
        .map(|v| v[1])
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0) as i64;
    let maxy = p
        .iter()
        .map(|v| v[1])
        .fold(f32::MIN, f32::max)
        .ceil()
        .min(H as f32 - 1.0) as i64;
    if minx > maxx || miny > maxy {
        return;
    }
    let d = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
    if d.abs() < 1e-9 {
        // degenerate in plan: mark the vertices' cells
        for v in &p {
            if v[0] >= 0.0 && v[1] >= 0.0 && (v[0] as usize) < W && (v[1] as usize) < H {
                f(v[0] as usize, v[1] as usize, v[2]);
            }
        }
        return;
    }
    let small = (maxx - minx) <= 1 && (maxy - miny) <= 1;
    for cy in miny..=maxy {
        for cx in minx..=maxx {
            let (x, y) = (cx as f32 + 0.5, cy as f32 + 0.5);
            let w1 =
                ((x - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (y - p[0][1])) / d;
            let w2 =
                ((p[1][0] - p[0][0]) * (y - p[0][1]) - (x - p[0][0]) * (p[1][1] - p[0][1])) / d;
            let w0 = 1.0 - w1 - w2;
            let inside = w0 >= -1e-4 && w1 >= -1e-4 && w2 >= -1e-4;
            if inside || small {
                let z = if inside {
                    w0 * p[0][2] + w1 * p[1][2] + w2 * p[2][2]
                } else {
                    p.iter().map(|v| v[2]).fold(f32::MIN, f32::max)
                };
                f(cx as usize, cy as usize, z);
            }
        }
    }
}

/// Fill an oriented rectangle (world corners) into a grid with cell size `cs` (in 2 m cells).
fn raster_quad(c: [[f32; 2]; 4], cs: usize, w: usize, h: usize, mut f: impl FnMut(usize, usize)) {
    let cc: Vec<[f32; 3]> = c
        .iter()
        .map(|v| {
            let (x, y) = cell_of(v[0], v[1]);
            [x / cs as f32, y / cs as f32, 0.0]
        })
        .collect();
    let minx = cc
        .iter()
        .map(|v| v[0])
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0) as i64;
    let maxx = cc
        .iter()
        .map(|v| v[0])
        .fold(f32::MIN, f32::max)
        .ceil()
        .min(w as f32 - 1.0) as i64;
    let miny = cc
        .iter()
        .map(|v| v[1])
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0) as i64;
    let maxy = cc
        .iter()
        .map(|v| v[1])
        .fold(f32::MIN, f32::max)
        .ceil()
        .min(h as f32 - 1.0) as i64;
    if minx > maxx || miny > maxy {
        return;
    }
    let inside = |x: f32, y: f32| {
        let mut s = 0i32;
        for k in 0..4 {
            let a = cc[k];
            let b = cc[(k + 1) % 4];
            let cr = (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
            s += if cr >= 0.0 { 1 } else { -1 };
        }
        s.abs() == 4
    };
    let mut any = false;
    for cy in miny..=maxy {
        for cx in minx..=maxx {
            if inside(cx as f32 + 0.5, cy as f32 + 0.5) {
                f(cx as usize, cy as usize);
                any = true;
            }
        }
    }
    if !any {
        let m = cc
            .iter()
            .fold([0.0f32; 2], |a, v| [a[0] + v[0] / 4.0, a[1] + v[1] / 4.0]);
        if m[0] >= 0.0 && m[1] >= 0.0 && (m[0] as usize) < w && (m[1] as usize) < h {
            f(m[0] as usize, m[1] as usize);
        }
    }
}

fn yaw_of(q: [f32; 4]) -> f32 {
    let [x, y, z, w] = q;
    (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z))
}

pub fn corners(pos: [f32; 3], yaw: f32, lo: [f32; 2], hi: [f32; 2], s: f32) -> [[f32; 2]; 4] {
    let (sn, cs) = yaw.sin_cos();
    let t = |x: f32, y: f32| {
        [
            pos[0] + (x * cs - y * sn) * s,
            pos[1] + (x * sn + y * cs) * s,
        ]
    };
    [
        t(lo[0], lo[1]),
        t(hi[0], lo[1]),
        t(hi[0], hi[1]),
        t(lo[0], hi[1]),
    ]
}

// ------------------------------------------------------------------ inputs

#[derive(Deserialize)]
struct PIndex {
    id: Option<String>,
    file: String,
    parent_id: Option<String>,
    bbox: Option<[[f32; 3]; 2]>,
}

#[derive(Deserialize)]
struct PLod {
    level: Option<String>,
    parent_index: Option<i64>,
    lod_dist: Option<f32>,
    child_lod_dist: Option<f32>,
}

#[derive(Deserialize)]
struct PEnt {
    name: Option<String>,
    model_id: Option<String>,
    position: [f32; 3],
    rotation_world: Option<[f32; 4]>,
    scale_xy: Option<f32>,
    scale_z: Option<f32>,
    archetype: Option<u64>,
    lod: Option<PLod>,
}

#[derive(Deserialize)]
struct PFile {
    id: Option<String>,
    entities: Vec<PEnt>,
}

/// One placement, compact.
#[derive(Clone)]
pub struct Ent {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub s: f32,
    pub sz: f32,
    pub name: u32,
    pub model: u32,
    pub file: u32,
    pub lod: u8,
    pub arch: u32,
    /// Global index of the LOD parent placement (u32::MAX = none).
    pub parent: u32,
    pub lod_dist: f32,
    pub child_lod_dist: f32,
    /// RF_* bits.
    pub rflags: u8,
}

/// EntRec.rflags: why a placement is not drawn by default
/// (sixng_core::meta7a::render_hint), plus bbox provenance.
pub const RF_PLACEHOLDER: u8 = 1;
pub const RF_VFX_PROXY: u8 = 2;
pub const RF_INTERIOR_PROXY: u8 = 4;
pub const RF_SHADOW_PROXY: u8 = 8;
pub const RF_LIGHT_PROXY: u8 = 16;
/// Model bounds collapse to under 1 cm in two axes.
pub const RF_DEGENERATE: u8 = 32;
/// Model bounds unknown: lo/hi are a placeholder unit cube.
pub const RF_BBOX_UNKNOWN: u8 = 64;
/// Bits that hide a placement by default.
pub const RF_HIDE: u8 = RF_PLACEHOLDER
    | RF_VFX_PROXY
    | RF_INTERIOR_PROXY
    | RF_SHADOW_PROXY
    | RF_LIGHT_PROXY
    | RF_DEGENERATE;

pub fn rflag_names(f: u8) -> Vec<&'static str> {
    [
        (RF_PLACEHOLDER, "placeholder"),
        (RF_VFX_PROXY, "vfx_proxy"),
        (RF_INTERIOR_PROXY, "interior_proxy"),
        (RF_SHADOW_PROXY, "shadow_proxy"),
        (RF_LIGHT_PROXY, "light_proxy"),
        (RF_DEGENERATE, "degenerate"),
        (RF_BBOX_UNKNOWN, "bbox_unknown"),
    ]
    .iter()
    .filter(|(b, _)| f & b != 0)
    .map(|(_, n)| *n)
    .collect()
}

fn reason_bit(r: Option<&str>) -> u8 {
    match r {
        Some("placeholder") => RF_PLACEHOLDER,
        Some("vfx_proxy") => RF_VFX_PROXY,
        Some("interior_proxy") => RF_INTERIOR_PROXY,
        Some("shadow_proxy") => RF_SHADOW_PROXY,
        Some("light_proxy") => RF_LIGHT_PROXY,
        _ => 0,
    }
}

pub const KIND_NAMES: [&str; 6] = [
    "other",
    "building",
    "prop",
    "vegetation",
    "lod",
    "interior shell",
];

fn lod_level(s: &str) -> u8 {
    match s {
        "HD" => 0,
        "LOD" => 1,
        "SLOD1" => 2,
        "SLOD2" => 3,
        "SLOD3" => 4,
        "ORPHAN_HD" => 0,
        "SLOD4" => 5,
        _ => 0,
    }
}

pub fn kind_of(name: &str, lod: u8, b: Option<&[f32; 6]>) -> u8 {
    let n = name;
    if lod > 0 || n.contains("_lod") || n.contains("slod") || n.contains("_imposter") {
        return 4;
    }
    if n.starts_with("int_") || n.contains("_shell") {
        return 5;
    }
    if n.starts_with("veg_")
        || n.contains("_tree")
        || n.contains("grass")
        || n.contains("bush")
        || n.contains("hedge")
    {
        return 3;
    }
    if n.starts_with("prop_") || n.starts_with("p_") {
        return 2;
    }
    if let Some(b) = b {
        let ex = (b[3] - b[0]).abs();
        let ey = (b[4] - b[1]).abs();
        let ez = (b[5] - b[2]).abs();
        let groundish = [
            "ground",
            "terrain",
            "_land",
            "rock",
            "cliff",
            "bank",
            "culvert",
            "stormdrain",
            "_road",
            "pavement",
            "_dtl",
            "details",
            "decal",
            "wall",
            "fence",
            "tunnel",
        ]
        .iter()
        .any(|k| n.contains(k));
        if ex * ey >= 36.0 && ez >= 3.0 && ex.max(ey) <= 250.0 && !groundish {
            return 1;
        }
        if ex * ey < 9.0 {
            return 2;
        }
    }
    0
}

struct Strings {
    list: Vec<String>,
    map: HashMap<String, u32>,
}
impl Strings {
    fn new() -> Self {
        Strings {
            list: vec![String::new()],
            map: HashMap::new(),
        }
    }
    fn id(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.map.get(s) {
            return i;
        }
        self.list.push(s.to_string());
        let i = (self.list.len() - 1) as u32;
        self.map.insert(s.to_string(), i);
        i
    }
}

fn read_glb(path: &Path) -> Result<(Value, Vec<u8>)> {
    let b = std::fs::read(path)?;
    let (j, bin) = crate::glb::split_glb(&b)?;
    Ok((j, bin.to_vec()))
}

/// (positions as rage xyz, indices) per primitive with its material index.
fn glb_tris(j: &Value, bin: &[u8]) -> Vec<(Option<usize>, String, Vec<[f32; 3]>, Vec<u32>)> {
    let mut out = Vec::new();
    let acc = |i: usize| -> Option<(usize, usize, u64, usize)> {
        let a = &j["accessors"][i];
        let bv = &j["bufferViews"][a["bufferView"].as_u64()? as usize];
        let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize
            + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        Some((
            off,
            a["count"].as_u64()? as usize,
            a["componentType"].as_u64()?,
            bv["byteStride"].as_u64().unwrap_or(0) as usize,
        ))
    };
    for m in j["meshes"].as_array().into_iter().flatten() {
        let name = m["name"].as_str().unwrap_or("").to_string();
        for p in m["primitives"].as_array().into_iter().flatten() {
            let Some((po, pn, _, ps)) = p["attributes"]["POSITION"]
                .as_u64()
                .and_then(|i| acc(i as usize))
            else {
                continue;
            };
            let stride = if ps == 0 { 12 } else { ps };
            if po + pn * stride > bin.len() {
                continue;
            }
            let pos: Vec<[f32; 3]> = (0..pn)
                .map(|k| {
                    let o = po + k * stride;
                    let g = |q: usize| {
                        f32::from_le_bytes(bin[o + 4 * q..o + 4 * q + 4].try_into().unwrap())
                    };
                    [g(0), -g(2), g(1)]
                })
                .collect();
            let idx: Vec<u32> = match p["indices"].as_u64().and_then(|i| acc(i as usize)) {
                Some((io, n, ct, _)) => match ct {
                    5125 if io + 4 * n <= bin.len() => (0..n)
                        .map(|k| {
                            u32::from_le_bytes(bin[io + 4 * k..io + 4 * k + 4].try_into().unwrap())
                        })
                        .collect(),
                    5123 if io + 2 * n <= bin.len() => (0..n)
                        .map(|k| {
                            u16::from_le_bytes(bin[io + 2 * k..io + 2 * k + 2].try_into().unwrap())
                                as u32
                        })
                        .collect(),
                    _ => continue,
                },
                None => (0..pn as u32).collect(),
            };
            out.push((
                p["material"].as_u64().map(|m| m as usize),
                name.clone(),
                pos,
                idx,
            ));
        }
    }
    out
}

pub struct Baker {
    pub work: PathBuf,
    pub out: PathBuf,
    pub placements: PathBuf,
    pub game: String,
}

impl Baker {
    pub fn new(game: &str, work: &str) -> Self {
        let w = PathBuf::from(work);
        let pf = w.join("placements_full").join("placements").join("levels");
        Baker {
            out: w.join("out"),
            placements: if pf.exists() {
                pf
            } else {
                w.join("out").join("placements").join("levels")
            },
            work: w,
            game: game.to_string(),
        }
    }
    fn mapdir(&self) -> PathBuf {
        self.work.join("map")
    }

    pub fn run(&self, only: &[String]) -> Result<()> {
        let t0 = std::time::Instant::now();
        let md = self.mapdir();
        std::fs::create_dir_all(&md)?;
        let want = |s: &str| only.is_empty() || only.iter().any(|o| o == s);
        log(
            &self.work,
            &format!(
                "bake start: placements {} out {}",
                self.placements.display(),
                self.out.display()
            ),
        );

        // ---------------- placements
        let (ents, strings, files, interiors) = self.read_placements()?;
        log(
            &self.work,
            &format!(
                "placements: {} entities, {} interiors ({:.0}s)",
                ents.len(),
                interiors.len(),
                t0.elapsed().as_secs_f32()
            ),
        );
        let bounds = self.model_bounds(&strings, &ents)?;
        log(
            &self.work,
            &format!(
                "model bounds: {} ({:.0}s)",
                bounds.len(),
                t0.elapsed().as_secs_f32()
            ),
        );

        // ---------------- collision top surface
        let top: Vec<AtomicU64> = (0..W * H).map(|_| AtomicU64::new(0)).collect();
        if want("collision") || !md.join("top.bin").exists() {
            self.collision_pass(&top)?;
            // cache the top surface for re-runs of the raster step
            let mut f = std::io::BufWriter::new(std::fs::File::create(md.join("top.bin"))?);
            for v in &top {
                f.write_all(&v.load(Ordering::Relaxed).to_le_bytes())?;
            }
        } else {
            let b = std::fs::read(md.join("top.bin"))?;
            for (k, v) in top.iter().enumerate() {
                v.store(
                    u64::from_le_bytes(b[8 * k..8 * k + 8].try_into()?),
                    Ordering::Relaxed,
                );
            }
        }
        log(
            &self.work,
            &format!("collision done ({:.0}s)", t0.elapsed().as_secs_f32()),
        );

        // ---------------- navmesh
        let nav: Vec<AtomicU8> = (0..W * H).map(|_| AtomicU8::new(0)).collect();
        self.navmesh_pass(&nav)?;
        log(
            &self.work,
            &format!("navmesh done ({:.0}s)", t0.elapsed().as_secs_f32()),
        );

        // ---------------- entity rasters
        let (cw, ch) = (W / COARSE, H / COARSE);
        let mut bld = vec![0u8; W * H];
        let mut veg = vec![0f32; cw * ch];
        let mut prop = vec![0f32; cw * ch];
        let mut gap_flat = vec![0f32; cw * ch];
        let mut gap_low = vec![0f32; cw * ch];
        let mut gap_w = vec![0f32; cw * ch];
        let mut lod = vec![0u8; cw * ch];
        let gaps = self.read_gaps();
        log(&self.work, &format!("gap models: {}", gaps.len()));
        let mut kinds = vec![0u8; ents.len()];
        for (i, e) in ents.iter().enumerate() {
            let b = bounds.get(&e.model);
            let k = kind_of(&strings.list[e.name as usize], e.lod, b);
            kinds[i] = k;
            let (lo, hi) = match b {
                Some(b) => ([b[0], b[1]], [b[3], b[4]]),
                None => ([-0.5, -0.5], [0.5, 0.5]),
            };
            let c = corners(e.pos, e.yaw, lo, hi, e.s);
            let area = ((hi[0] - lo[0]) * (hi[1] - lo[1]) * e.s * e.s).max(0.25);
            match k {
                1 => raster_quad(c, 1, W, H, |x, y| bld[y * W + x] = 1),
                5 => raster_quad(c, 1, W, H, |x, y| {
                    if bld[y * W + x] == 0 {
                        bld[y * W + x] = 2
                    }
                }),
                2 | 3 => {
                    let (x, y) = cell_of(e.pos[0], e.pos[1]);
                    let (x, y) = (x as usize / COARSE, y as usize / COARSE);
                    if x < cw && y < ch {
                        if k == 3 {
                            veg[y * cw + x] += 1.0;
                        } else {
                            prop[y * cw + x] += 1.0;
                        }
                    }
                }
                4 => {
                    let lv = e.lod.max(1);
                    raster_quad(c, COARSE, cw, ch, |x, y| {
                        lod[y * cw + x] = lod[y * cw + x].max(lv)
                    });
                }
                _ => {}
            }
            if k != 4 {
                if let Some(&(f, l)) = gaps.get(strings.list[e.model as usize].as_str()) {
                    let mut n = 0usize;
                    raster_quad(c, COARSE, cw, ch, |_, _| n += 1);
                    let per = area / n.max(1) as f32;
                    raster_quad(c, COARSE, cw, ch, |x, y| {
                        gap_flat[y * cw + x] += f * per;
                        gap_low[y * cw + x] += l * per;
                        gap_w[y * cw + x] += per;
                    });
                } else if k == 1 || k == 2 || k == 0 {
                    // models without gap records are fully OK: weight them too
                    let mut n = 0usize;
                    raster_quad(c, COARSE, cw, ch, |_, _| n += 1);
                    let per = area / n.max(1) as f32;
                    raster_quad(c, COARSE, cw, ch, |x, y| gap_w[y * cw + x] += per);
                }
            }
        }
        // batches (instanced vegetation / props)
        self.batches_pass(&mut veg, &mut prop, cw, ch);
        log(
            &self.work,
            &format!("entity rasters done ({:.0}s)", t0.elapsed().as_secs_f32()),
        );

        // ---------------- heights 8 m + vector data
        let mut h8 = vec![f32::NAN; cw * ch];
        for y in 0..ch {
            for x in 0..cw {
                let mut m = f32::NAN;
                for dy in 0..COARSE {
                    for dx in 0..COARSE {
                        if let Some(z) = unhq(
                            top[(y * COARSE + dy) * W + x * COARSE + dx].load(Ordering::Relaxed),
                        ) {
                            m = if m.is_nan() { z } else { m.max(z) };
                        }
                    }
                }
                h8[y * cw + x] = m;
            }
        }
        std::fs::write(md.join("heights_8m.bin"), bytemuck::cast_slice(&h8))?;
        self.write_entities(&md, &ents, &kinds, &bounds, &strings, &files)?;
        std::fs::write(md.join("interiors.json"), serde_json::to_vec(&interiors)?)?;
        self.write_roads(&md)?;
        let nl = self.write_lights(&md, &interiors)?;
        log(
            &self.work,
            &format!(
                "vectors done: {nl} lights ({:.0}s)",
                t0.elapsed().as_secs_f32()
            ),
        );

        // ---------------- local ground (min filter) for the building test
        let mut gmin = vec![f32::NAN; cw * ch];
        for y in 0..ch {
            for x in 0..cw {
                let mut m = f32::NAN;
                for dy in 0..COARSE {
                    for dx in 0..COARSE {
                        if let Some(z) = unhq(
                            top[(y * COARSE + dy) * W + x * COARSE + dx].load(Ordering::Relaxed),
                        ) {
                            m = if m.is_nan() { z } else { m.min(z) };
                        }
                    }
                }
                gmin[y * cw + x] = m;
            }
        }
        let rad = 3i64; // 7 x 7 coarse cells = 56 m
        let ground: Vec<f32> = (0..cw * ch)
            .into_par_iter()
            .map(|k| {
                let (x, y) = ((k % cw) as i64, (k / cw) as i64);
                let mut m = f32::NAN;
                for yy in (y - rad).max(0)..=(y + rad).min(ch as i64 - 1) {
                    for xx in (x - rad).max(0)..=(x + rad).min(cw as i64 - 1) {
                        let v = gmin[yy as usize * cw + xx as usize];
                        if !v.is_nan() {
                            m = if m.is_nan() { v } else { m.min(v) };
                        }
                    }
                }
                m
            })
            .collect();

        // ---------------- surface palette
        let surf_cat = self.surface_categories();

        // ---------------- tiles
        let zmax = 6u32;
        let src = Src {
            ground: &ground,
            top: &top,
            nav: &nav,
            bld: &bld,
            veg: &veg,
            prop: &prop,
            gap_flat: &gap_flat,
            gap_low: &gap_low,
            gap_w: &gap_w,
            lod: &lod,
            surf: &surf_cat,
            cw,
            ch,
        };
        let mut tile_counts = serde_json::Map::new();
        for (layer, _) in LAYERS {
            let n = self.write_pyramid(&md, layer, zmax, &src)?;
            tile_counts.insert(layer.to_string(), json!(n));
            log(
                &self.work,
                &format!("tiles {layer}: {n} ({:.0}s)", t0.elapsed().as_secs_f32()),
            );
        }
        let meta = json!({
            "x0": X0, "y1": Y1, "cell_m": CELL, "w": W, "h": H, "tile": TILE, "zmax": zmax, "coarse_cells": COARSE,
            "coordinate_system": "RAGE world space, Z-up, metres. Image rows go north to south: cell (c, r) centre = (x0 + (c + 0.5) * cell_m, y1 - (r + 0.5) * cell_m). glTF / BeamNG-style Y-up: (x, z, -y).",
            "layers": LAYERS.iter().map(|(n, d)| json!({"name": n, "desc": d})).collect::<Vec<_>>(),
            "tiles": tile_counts,
            "kinds": KIND_NAMES,
            "surface_categories": surf_cat.legend(),
            "entities": ents.len(),
            "seconds": t0.elapsed().as_secs_f64(),
        });
        std::fs::write(md.join("meta.json"), serde_json::to_vec_pretty(&meta)?)?;
        log(
            &self.work,
            &format!("bake done in {:.0}s", t0.elapsed().as_secs_f32()),
        );
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    fn read_placements(&self) -> Result<(Vec<Ent>, Strings, Vec<String>, Vec<Value>)> {
        let idx = self.placements.join("index.jsonl");
        let f = BufReader::new(
            std::fs::File::open(&idx).map_err(|e| anyhow!("{}: {e}", idx.display()))?,
        );
        let mut files = Vec::new();
        let mut parent_of: HashMap<String, String> = HashMap::new();
        for l in f.lines() {
            let l = l?;
            let Ok(p) = serde_json::from_str::<PIndex>(&l) else {
                continue;
            };
            let Some(bb) = p.bbox else { continue };
            if sixng_core::surface::is_local_xy(&bb) {
                continue; // interior-space entity lists (placed via interiors.jsonl)
            }
            if let (Some(id), Some(pid)) = (&p.id, &p.parent_id) {
                parent_of.insert(id.clone(), pid.clone());
            }
            let name = Path::new(&p.file)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            files.push(self.placements.join(name));
        }
        let done = AtomicUsize::new(0);
        let parsed: Vec<(String, Vec<PEnt>)> = files
            .par_iter()
            .filter_map(|p| {
                let b = std::fs::read(p).ok()?;
                let pf: PFile = serde_json::from_slice(&b).ok()?;
                let n = done.fetch_add(1, Ordering::Relaxed);
                if n % 2000 == 0 {
                    eprintln!("placements {n}/{}", files.len());
                }
                Some((
                    pf.id.unwrap_or_else(|| {
                        p.file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default()
                    }),
                    pf.entities,
                ))
            })
            .collect();
        let mut strings = Strings::new();
        let mut file_ids = Vec::new();
        let mut ents = Vec::new();
        // (file start, parent_index) per entity, resolved to global indices below
        let mut starts: HashMap<String, usize> = HashMap::new();
        let mut pending: Vec<(usize, String, i64)> = Vec::new();
        for (fi, (id, es)) in parsed.into_iter().enumerate() {
            starts.insert(id.clone(), ents.len());
            let parent_file = parent_of.get(&id).cloned();
            file_ids.push(id);
            for e in es {
                let reason =
                    sixng_core::meta7a::render_hint(e.name.as_deref(), e.model_id.is_some());
                let (pi, ld, cld) = e
                    .lod
                    .as_ref()
                    .map(|l| {
                        (
                            l.parent_index,
                            l.lod_dist.unwrap_or(-1.0),
                            l.child_lod_dist.unwrap_or(0.0),
                        )
                    })
                    .unwrap_or((None, -1.0, 0.0));
                if let (Some(pi), Some(pf)) = (pi, &parent_file) {
                    pending.push((ents.len(), pf.clone(), pi));
                }
                let name = e
                    .name
                    .unwrap_or_else(|| format!("{:08x}", e.archetype.unwrap_or(0)));
                let model = e.model_id.unwrap_or_default();
                ents.push(Ent {
                    parent: u32::MAX,
                    lod_dist: ld,
                    child_lod_dist: cld,
                    rflags: reason_bit(reason),
                    pos: e.position,
                    yaw: e.rotation_world.map(yaw_of).unwrap_or(0.0),
                    s: e.scale_xy.unwrap_or(1.0),
                    sz: e.scale_z.unwrap_or(1.0),
                    name: strings.id(&name),
                    model: strings.id(&model),
                    file: fi as u32,
                    lod: e
                        .lod
                        .and_then(|l| l.level)
                        .map(|s| lod_level(&s))
                        .unwrap_or(0),
                    arch: e.archetype.unwrap_or(0) as u32,
                });
            }
        }
        let mut linked = 0usize;
        for (i, pf, pi) in pending {
            if let Some(&st) = starts.get(&pf) {
                let g = st + pi as usize;
                if pi >= 0 && g < ents.len() {
                    ents[i].parent = g as u32;
                    linked += 1;
                }
            }
        }
        log(&self.work, &format!("LOD parents linked: {linked}"));
        // interiors
        let mut interiors = Vec::new();
        let ip = self
            .out
            .join("placements")
            .join("levels")
            .join("interiors.jsonl");
        let ip = if ip.exists() {
            ip
        } else {
            self.placements.join("interiors.jsonl")
        };
        if let Ok(f) = std::fs::File::open(&ip) {
            for l in BufReader::new(f).lines().map_while(|l| l.ok()) {
                let Ok(v) = serde_json::from_str::<Value>(&l) else {
                    continue;
                };
                interiors.push(json!({
                    "archetype": v["archetype"], "interior_file": v["interior_file"], "map": v["map"],
                    "position": v["position"], "rotation_world": v["rotation_world"], "bbox": v["bbox"],
                    "entities": v["entities"].as_array().map(|a| a.len()).unwrap_or(0),
                    "collision": v["collision_linked"]["id"],
                }));
            }
        }
        Ok((ents, strings, file_ids, interiors))
    }

    /// Local bounds (min xyz, max xyz) per model string id, from the
    /// drawable root (+0x30 min, +0x40 max). Cached in map/model_bounds.json.
    fn model_bounds(&self, strings: &Strings, ents: &[Ent]) -> Result<HashMap<u32, [f32; 6]>> {
        let cache = self.mapdir().join("model_bounds.json");
        let mut known: HashMap<String, [f32; 6]> = std::fs::read(&cache)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut need: Vec<u32> = ents.iter().map(|e| e.model).collect();
        need.sort();
        need.dedup();
        let missing: Vec<&str> = need
            .iter()
            .map(|&m| strings.list[m as usize].as_str())
            .filter(|s| !s.is_empty() && !known.contains_key(*s))
            .collect();
        if !missing.is_empty() {
            let ws = sixng_core::export::Workspace::new(
                &self.game,
                self.work.to_str().unwrap_or("work"),
            );
            let mut by_arch: HashMap<&str, Vec<&str>> = HashMap::new();
            for m in &missing {
                if let Some((a, _)) = m.split_once('@') {
                    by_arch.entry(a).or_default().push(m);
                }
            }
            for (arch, ids) in by_arch {
                let Ok(a) = ws.resolve(arch) else { continue };
                let Ok(ix) = ws.load_index(&a) else { continue };
                let resolver = sixng_core::export::ModelResolver::new(&ix.entries);
                let res: Vec<(String, [f32; 6])> = ids
                    .par_iter()
                    .filter_map(|id| {
                        let (e, member) = resolver.resolve(&ix.entries, id).ok()?;
                        let r = if member.is_some() {
                            sixng_core::resource::Resource::load(&a.path, e)
                        } else {
                            sixng_core::resource::Resource::load_sys(&a.path, e)
                        }
                        .ok()?;
                        if r.sys.len() < 0x50 {
                            return None;
                        }
                        let d = sixng_core::drawable::model_bounds(&r, member)?;
                        let b = [d[0][0], d[0][1], d[0][2], d[1][0], d[1][1], d[1][2]];
                        let ok = b.iter().all(|v| v.is_finite() && v.abs() < 20000.0)
                            && b[3] >= b[0]
                            && b[4] >= b[1];
                        ok.then(|| (id.to_string(), b))
                    })
                    .collect();
                log(
                    &self.work,
                    &format!("bounds {arch}: {} of {}", res.len(), ids.len()),
                );
                known.extend(res);
            }
            std::fs::write(&cache, serde_json::to_vec(&known)?)?;
        }
        Ok(need
            .into_iter()
            .filter_map(|m| {
                known
                    .get(strings.list[m as usize].as_str())
                    .map(|b| (m, *b))
            })
            .collect())
    }

    fn collision_pass(&self, top: &[AtomicU64]) -> Result<()> {
        let dir = self.out.join("collision").join("levels");
        let mut files = Vec::new();
        let f = BufReader::with_capacity(1 << 20, std::fs::File::open(dir.join("index.jsonl"))?);
        #[derive(Deserialize)]
        struct CI {
            file: String,
            bbox: Option<[[f32; 3]; 2]>,
        }
        for l in f.lines() {
            let l = l?;
            let Ok(c) = serde_json::from_str::<CI>(&l) else {
                continue;
            };
            let Some(bb) = c.bbox else { continue };
            if sixng_core::surface::is_local_xy(&bb) {
                continue;
            }
            let name = Path::new(&c.file)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            files.push(dir.join(name));
        }
        log(
            &self.work,
            &format!("collision files (world space): {}", files.len()),
        );
        let done = AtomicUsize::new(0);
        files.par_iter().for_each(|p| {
            let Ok((j, bin)) = read_glb(p) else { return };
            for (_, name, pos, idx) in glb_tris(&j, &bin) {
                let surf = name
                    .strip_prefix("surface_")
                    .and_then(|s| u16::from_str_radix(s, 16).ok())
                    .unwrap_or(0xffff) as u64;
                for t in idx.chunks_exact(3) {
                    let (Some(a), Some(b), Some(c)) = (
                        pos.get(t[0] as usize),
                        pos.get(t[1] as usize),
                        pos.get(t[2] as usize),
                    ) else {
                        continue;
                    };
                    // skip vertical faces
                    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                    let n = [
                        u[1] * v[2] - u[2] * v[1],
                        u[2] * v[0] - u[0] * v[2],
                        u[0] * v[1] - u[1] * v[0],
                    ];
                    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    if l < 1e-9 || (n[2] / l).abs() < 0.2 {
                        continue;
                    }
                    let q = |w: &[f32; 3]| {
                        let (x, y) = cell_of(w[0], w[1]);
                        [x, y, w[2]]
                    };
                    raster_tri([q(a), q(b), q(c)], |cx, cy, z| {
                        top[cy * W + cx].fetch_max(hq(z) << 16 | surf, Ordering::Relaxed);
                    });
                }
            }
            let n = done.fetch_add(1, Ordering::Relaxed);
            if n % 5000 == 0 {
                log(&self.work, &format!("collision {n}/{}", files.len()));
            }
        });
        Ok(())
    }

    fn navmesh_pass(&self, nav: &[AtomicU8]) -> Result<()> {
        let dir = self.out.join("navmesh").join("navmeshes");
        let files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().map(|x| x == "glb").unwrap_or(false))
                    .collect()
            })
            .unwrap_or_default();
        files.par_iter().for_each(|p| {
            let Ok((j, bin)) = read_glb(p) else { return };
            for (mat, _, pos, idx) in glb_tris(&j, &bin) {
                let road = mat
                    .map(|m| {
                        j["materials"][m]["extras"]["road"]
                            .as_bool()
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                let bit = if road { 1 } else { 2 };
                for t in idx.chunks_exact(3) {
                    let (Some(a), Some(b), Some(c)) = (
                        pos.get(t[0] as usize),
                        pos.get(t[1] as usize),
                        pos.get(t[2] as usize),
                    ) else {
                        continue;
                    };
                    let q = |w: &[f32; 3]| {
                        let (x, y) = cell_of(w[0], w[1]);
                        [x, y, w[2]]
                    };
                    raster_tri([q(a), q(b), q(c)], |cx, cy, _| {
                        nav[cy * W + cx].fetch_or(bit, Ordering::Relaxed);
                    });
                }
            }
        });
        Ok(())
    }

    fn batches_pass(&self, veg: &mut [f32], prop: &mut [f32], cw: usize, ch: usize) {
        let p = self
            .out
            .join("placements")
            .join("levels")
            .join("batches.jsonl");
        let Ok(f) = std::fs::File::open(&p) else {
            return;
        };
        for l in BufReader::new(f).lines().map_while(|l| l.ok()) {
            let Ok(v) = serde_json::from_str::<Value>(&l) else {
                continue;
            };
            let Ok(bb) = serde_json::from_value::<[[f32; 3]; 2]>(v["bbox"].clone()) else {
                continue;
            };
            let n = v["count"].as_f64().unwrap_or(0.0) as f32;
            let grass = v["kind"].as_str().map(|k| k == "grass").unwrap_or(false)
                || v["name"]
                    .as_str()
                    .map(|s| s.starts_with("veg_"))
                    .unwrap_or(false);
            let (x0, y0) = cell_of(bb[0][0], bb[1][1]);
            let (x1, y1) = cell_of(bb[1][0], bb[0][1]);
            let (x0, y0, x1, y1) = (
                (x0 as i64 / COARSE as i64).max(0),
                (y0 as i64 / COARSE as i64).max(0),
                (x1 as i64 / COARSE as i64).min(cw as i64 - 1),
                (y1 as i64 / COARSE as i64).min(ch as i64 - 1),
            );
            if x0 > x1 || y0 > y1 {
                continue;
            }
            let per = n / (((x1 - x0 + 1) * (y1 - y0 + 1)) as f32);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let k = y as usize * cw + x as usize;
                    if grass {
                        veg[k] += per;
                    } else {
                        prop[k] += per;
                    }
                }
            }
        }
    }

    /// model id -> (flat share, lowmip share) of its materials.
    fn read_gaps(&self) -> HashMap<String, (f32, f32)> {
        let mut m = HashMap::new();
        let Ok(f) = std::fs::File::open(self.work.join("texture_gaps_models.jsonl.gz")) else {
            return m;
        };
        let r = BufReader::new(flate2::read::GzDecoder::new(f));
        for l in r.lines().map_while(|l| l.ok()) {
            let Ok(v) = serde_json::from_str::<Value>(&l) else {
                continue;
            };
            let Some(id) = v["model_id"].as_str() else {
                continue;
            };
            let mats = v["materials"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0)
                .max(1) as f32;
            let flat = v["materials"]
                .as_array()
                .map(|a| a.iter().filter(|x| x["status"] == "flat").count())
                .unwrap_or(0) as f32;
            let low = v["materials"]
                .as_array()
                .map(|a| a.iter().filter(|x| x["status"] == "lowmip").count())
                .unwrap_or(0) as f32;
            // materials not listed are fine; the record lists affected ones only
            let total = v["material_count"]
                .as_f64()
                .map(|x| x as f32)
                .unwrap_or(mats);
            m.insert(id.to_string(), (flat / total, low / total));
        }
        m
    }

    fn write_entities(
        &self,
        md: &Path,
        ents: &[Ent],
        kinds: &[u8],
        bounds: &HashMap<u32, [f32; 6]>,
        strings: &Strings,
        files: &[String],
    ) -> Result<()> {
        let mut f = std::io::BufWriter::new(std::fs::File::create(md.join("entities.bin"))?);
        for (e, &k) in ents.iter().zip(kinds) {
            let known = bounds.get(&e.model).copied();
            let b = known.unwrap_or([-0.5, -0.5, 0.0, 0.5, 0.5, 1.0]);
            let mut rf = e.rflags;
            match known {
                None => rf |= RF_BBOX_UNKNOWN,
                Some(b) => {
                    let ext = [b[3] - b[0], b[4] - b[1], b[5] - b[2]];
                    if ext.iter().filter(|v| **v < 0.01).count() >= 2 {
                        rf |= RF_DEGENERATE;
                    }
                }
            }
            let rec = EntRec {
                pos: e.pos,
                yaw: e.yaw,
                lo: [b[0] * e.s, b[1] * e.s, b[2] * e.sz],
                hi: [b[3] * e.s, b[4] * e.s, b[5] * e.sz],
                name: e.name,
                model: e.model,
                file: e.file,
                arch: e.arch,
                lod: e.lod,
                kind: k,
                rflags: rf,
                _pad: 0,
                parent: e.parent,
                lod_dist: e.lod_dist,
                child_lod_dist: e.child_lod_dist,
            };
            f.write_all(bytemuck::bytes_of(&rec))?;
        }
        std::fs::write(
            md.join("strings.json"),
            serde_json::to_vec(&json!({"strings": strings.list, "files": files}))?,
        )?;
        Ok(())
    }

    fn write_roads(&self, md: &Path) -> Result<()> {
        let p = self.out.join("paths").join("levels").join("graph.json");
        let Ok(b) = std::fs::read(&p) else {
            return Ok(());
        };
        let g: Value = serde_json::from_slice(&b)?;
        let nodes = g["nodes"].as_array().cloned().unwrap_or_default();
        let np = |i: usize| -> Option<[f32; 3]> {
            serde_json::from_value(nodes.get(i)?["p"].clone()).ok()
        };
        let mut links = Vec::new();
        for l in g["links"].as_array().into_iter().flatten() {
            let (Some(a), Some(b)) = (l["from"].as_u64(), l["to"].as_u64()) else {
                continue;
            };
            let (Some(pa), Some(pb)) = (np(a as usize), np(b as usize)) else {
                continue;
            };
            let na = &nodes[a as usize];
            let flags = l["flags"].as_u64().unwrap_or(0);
            links.push(json!([
                pa[0],
                pa[1],
                pa[2],
                pb[0],
                pb[1],
                pb[2],
                na["speed"].as_f64().unwrap_or(0.0),
                l["lanes_from"],
                l["lanes_to"],
                flags,
                na["road_class"],
                na["kind"],
                a,
                b
            ]));
        }
        let mut nflags = Vec::new();
        for n in &nodes {
            nflags.push(json!([
                n["p"],
                n["flag_names"],
                n["speed"],
                n["density"],
                n["street"]
            ]));
        }
        std::fs::write(
            md.join("roads.json"),
            serde_json::to_vec(
                &json!({"links": links, "nodes": nflags, "flags_note": g["flags_note"]}),
            )?,
        )?;
        let mut juncs = Vec::new();
        for jn in g["junctions"].as_array().into_iter().flatten() {
            let Some(n) = jn["node"].as_u64() else {
                continue;
            };
            let Some(p) = np(n as usize) else { continue };
            let ents = jn["entrances"].as_array().cloned().unwrap_or_default();
            let stop = ents
                .iter()
                .filter(|e| e["stop"].as_bool() == Some(true))
                .count();
            let yld = ents
                .iter()
                .filter(|e| e["yield"].as_bool() == Some(true))
                .count();
            let phases: Vec<Value> = jn["phases"].as_array().cloned().unwrap_or_default();
            let entr: Vec<Value> = ents
                .iter()
                .filter_map(|e| {
                    np(e["node"].as_u64()? as usize)
                        .map(|q| json!([q[0], q[1], e["stop"], e["yield"]]))
                })
                .collect();
            juncs.push(
                json!({"p": p, "entrances": entr, "stop": stop, "yield": yld, "phases": phases}),
            );
        }
        std::fs::write(md.join("junctions.json"), serde_json::to_vec(&juncs)?)?;
        Ok(())
    }

    fn write_lights(&self, md: &Path, interiors: &[Value]) -> Result<usize> {
        let dir = self.out.join("lights").join("levels");
        // (archetype, light) -> time mask, falloff exponent
        let mut sets: HashMap<(String, u64), (u32, f32)> = HashMap::new();
        if let Ok(f) = std::fs::File::open(dir.join("sets.jsonl")) {
            for l in BufReader::with_capacity(1 << 20, f)
                .lines()
                .map_while(|l| l.ok())
            {
                #[derive(Deserialize)]
                struct L {
                    time_flags: Option<String>,
                    falloff_exponent: Option<f32>,
                }
                #[derive(Deserialize)]
                struct S {
                    archetype: String,
                    lights: Vec<L>,
                }
                let Ok(s) = serde_json::from_str::<S>(&l) else {
                    continue;
                };
                for (k, li) in s.lights.iter().enumerate() {
                    let m = li
                        .time_flags
                        .as_deref()
                        .and_then(|t| u32::from_str_radix(t, 16).ok())
                        .unwrap_or(0xff_ffff)
                        & 0xff_ffff;
                    sets.insert(
                        (s.archetype.clone(), k as u64),
                        (m, li.falloff_exponent.unwrap_or(8.0)),
                    );
                }
            }
        }
        // interior instances by interior file
        let mut inst: HashMap<String, Vec<([f32; 3], [f32; 4])>> = HashMap::new();
        for v in interiors {
            let (Some(f), Ok(p), Ok(q)) = (
                v["interior_file"].as_str(),
                serde_json::from_value::<[f32; 3]>(v["position"].clone()),
                serde_json::from_value::<[f32; 4]>(v["rotation_world"].clone()),
            ) else {
                continue;
            };
            inst.entry(f.to_string()).or_default().push((p, q));
        }
        let rot = |q: [f32; 4], v: [f32; 3]| -> [f32; 3] {
            let g = glam::Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize();
            (g * glam::Vec3::from(v)).into()
        };
        let mut out = std::io::BufWriter::new(std::fs::File::create(md.join("lights.bin"))?);
        let mut n = 0usize;
        let f = std::fs::File::open(dir.join("world.jsonl"))?;
        #[derive(Deserialize)]
        struct WL {
            archetype: String,
            colour: String,
            intensity: Option<f32>,
            light: u64,
            map: String,
            night_only: Option<bool>,
            p: [f32; 3],
            range: Option<f32>,
            space: String,
            #[serde(rename = "type")]
            ty: Option<u8>,
        }
        for l in BufReader::with_capacity(1 << 20, f)
            .lines()
            .map_while(|l| l.ok())
        {
            let Ok(w) = serde_json::from_str::<WL>(&l) else {
                continue;
            };
            let c = u32::from_str_radix(&w.colour, 16).unwrap_or(0xffffff);
            let (mask, fexp) = sets
                .get(&(w.archetype.clone(), w.light))
                .copied()
                .unwrap_or((0xff_ffff, 8.0));
            let night = w.night_only.unwrap_or(false);
            let interior_map = inst.contains_key(&w.map);
            let positions: Vec<[f32; 3]> = if w.space == "world" {
                vec![w.p]
            } else {
                inst.get(&w.map)
                    .map(|v| {
                        v.iter()
                            .map(|(p, q)| {
                                let r = rot(*q, w.p);
                                [p[0] + r[0], p[1] + r[1], p[2] + r[2]]
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };
            for p in positions {
                let rec = LightRec {
                    pos: p,
                    rgb_type: [(c >> 16) as u8, (c >> 8) as u8, c as u8, w.ty.unwrap_or(0)],
                    mask: if mask == 0 {
                        if night {
                            0xe0_007f
                        } else {
                            0xff_ffff
                        }
                    } else {
                        mask
                    },
                    intensity: w.intensity.unwrap_or(1.0),
                    range: w.range.unwrap_or(1.0),
                    flags: (interior_map || w.space != "world") as u8 | ((night as u8) << 1),
                    _pad: [0; 3],
                    falloff_exp: fexp,
                };
                let _ = rec.falloff_exp;
                out.write_all(bytemuck::bytes_of(&rec))?;
                n += 1;
            }
        }
        Ok(n)
    }

    fn surface_categories(&self) -> SurfPal {
        let mut cat: HashMap<u16, String> = HashMap::new();
        if let Some(v) = std::fs::read(
            self.out
                .join("collision")
                .join("levels")
                .join("surface_categories.json"),
        )
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        {
            if let Some(o) = v["surfaces"].as_object() {
                for (k, s) in o {
                    if let (Ok(id), Some(c)) = (u16::from_str_radix(k, 16), s["category"].as_str())
                    {
                        cat.insert(id, c.to_string());
                    }
                }
            }
        }
        let mut names: Vec<String> = cat.values().cloned().collect();
        names.sort();
        names.dedup();
        SurfPal { cat, names }
    }

    fn write_pyramid(&self, md: &Path, layer: &str, zmax: u32, src: &Src) -> Result<usize> {
        let tx = W.div_ceil(TILE);
        let ty = H.div_ceil(TILE);
        let base = md.join("tiles").join(layer);
        let _ = std::fs::remove_dir_all(&base);
        // level zmax
        let mut cur: HashMap<(usize, usize), Vec<u8>> = (0..tx * ty)
            .into_par_iter()
            .filter_map(|k| {
                let (x, y) = (k % tx, k / tx);
                let mut px = vec![0u8; TILE * TILE * 4];
                let mut any = false;
                for j in 0..TILE {
                    for i in 0..TILE {
                        let (c, r) = (x * TILE + i, y * TILE + j);
                        if c >= W || r >= H {
                            continue;
                        }
                        let v = src.pixel(layer, c, r);
                        if v[3] != 0 {
                            any = true;
                        }
                        px[(j * TILE + i) * 4..(j * TILE + i) * 4 + 4].copy_from_slice(&v);
                    }
                }
                any.then_some(((x, y), px))
            })
            .collect();
        let mut count = 0;
        let mut z = zmax;
        let (mut w, mut h) = (tx, ty);
        loop {
            let dir = base.join(z.to_string());
            std::fs::create_dir_all(&dir)?;
            count += cur
                .par_iter()
                .filter(|(&(x, y), px)| {
                    image::save_buffer(
                        dir.join(format!("{x}_{y}.png")),
                        px,
                        TILE as u32,
                        TILE as u32,
                        image::ExtendedColorType::Rgba8,
                    )
                    .is_ok()
                })
                .count();
            if z == 0 {
                break;
            }
            let (nw, nh) = (w.div_ceil(2), h.div_ceil(2));
            let next: HashMap<(usize, usize), Vec<u8>> = (0..nw * nh)
                .into_par_iter()
                .filter_map(|k| {
                    let (x, y) = (k % nw, k / nw);
                    let kids: Vec<Option<&Vec<u8>>> = [(0, 0), (1, 0), (0, 1), (1, 1)]
                        .iter()
                        .map(|(dx, dy)| cur.get(&(2 * x + dx, 2 * y + dy)))
                        .collect();
                    if kids.iter().all(|k| k.is_none()) {
                        return None;
                    }
                    let mut px = vec![0u8; TILE * TILE * 4];
                    for (q, kid) in kids.iter().enumerate() {
                        let Some(kid) = kid else { continue };
                        let (ox, oy) = ((q % 2) * TILE / 2, (q / 2) * TILE / 2);
                        for j in 0..TILE / 2 {
                            for i in 0..TILE / 2 {
                                let mut acc = [0u32; 4];
                                let mut wsum = 0u32;
                                for (a, b) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                                    let o = ((2 * j + b) * TILE + 2 * i + a) * 4;
                                    let al = kid[o + 3] as u32;
                                    for c in 0..3 {
                                        acc[c] += kid[o + c] as u32 * al;
                                    }
                                    acc[3] += al;
                                    wsum += al;
                                }
                                let o = ((oy + j) * TILE + ox + i) * 4;
                                if wsum > 0 {
                                    for c in 0..3 {
                                        px[o + c] = (acc[c] / wsum) as u8;
                                    }
                                    px[o + 3] = (acc[3] / 4) as u8;
                                }
                            }
                        }
                    }
                    Some(((x, y), px))
                })
                .collect();
            cur = next;
            w = nw;
            h = nh;
            z -= 1;
        }
        Ok(count)
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EntRec {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub lo: [f32; 3],
    pub hi: [f32; 3],
    pub name: u32,
    pub model: u32,
    pub file: u32,
    pub arch: u32,
    pub lod: u8,
    pub kind: u8,
    /// RF_* bits (proxy / placeholder / degenerate / bbox unknown).
    pub rflags: u8,
    pub _pad: u8,
    /// Global index of the LOD parent placement (u32::MAX = none).
    pub parent: u32,
    /// Entity LOD distance (m, -1 = archetype default) and the distance
    /// below which its children take over (0 = none).
    pub lod_dist: f32,
    pub child_lod_dist: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightRec {
    pub pos: [f32; 3],
    pub rgb_type: [u8; 4],
    pub mask: u32,
    pub intensity: f32,
    pub range: f32,
    pub flags: u8,
    pub _pad: [u8; 3],
    pub falloff_exp: f32,
}

pub struct SurfPal {
    cat: HashMap<u16, String>,
    names: Vec<String>,
}

pub fn category_colour(c: &str) -> [u8; 3] {
    let c = c.to_lowercase();
    let has = |k: &str| c.contains(k);
    if has("water") || has("sea") || has("river") {
        [60, 110, 200]
    } else if has("road") || has("asphalt") || has("tarmac") {
        [70, 70, 78]
    } else if has("pavement") || has("sidewalk") || has("concrete") || has("kerb") {
        [165, 165, 160]
    } else if has("grass") || has("lawn") || has("vegetat") {
        [90, 160, 70]
    } else if has("sand") || has("beach") {
        [225, 205, 140]
    } else if has("dirt") || has("soil") || has("mud") || has("track") {
        [150, 115, 75]
    } else if has("rock") || has("cliff") || has("stone") {
        [125, 115, 105]
    } else if has("gravel") || has("ballast") {
        [175, 160, 135]
    } else if has("roof") {
        [175, 95, 80]
    } else if has("wall") || has("vertical") {
        [150, 130, 140]
    } else if has("floor") || has("hard") || has("platform") {
        [195, 190, 175]
    } else if has("wood") {
        [160, 120, 70]
    } else if has("metal") {
        [120, 140, 160]
    } else {
        let h = c
            .bytes()
            .fold(2166136261u32, |a, b| (a ^ b as u32).wrapping_mul(16777619));
        [
            (80 + (h & 0x7f)) as u8,
            (80 + ((h >> 8) & 0x7f)) as u8,
            (80 + ((h >> 16) & 0x7f)) as u8,
        ]
    }
}

impl SurfPal {
    fn colour(&self, id: u16) -> [u8; 4] {
        match self.cat.get(&id) {
            Some(c) => {
                let k = category_colour(c);
                [k[0], k[1], k[2], 255]
            }
            None => [110, 100, 120, 255],
        }
    }
    fn legend(&self) -> Value {
        Value::Array(
            self.names
                .iter()
                .map(|n| json!([n, category_colour(n)]))
                .collect(),
        )
    }
}

struct Src<'a> {
    ground: &'a [f32],
    top: &'a [AtomicU64],
    nav: &'a [AtomicU8],
    bld: &'a [u8],
    veg: &'a [f32],
    prop: &'a [f32],
    gap_flat: &'a [f32],
    gap_low: &'a [f32],
    gap_w: &'a [f32],
    lod: &'a [u8],
    surf: &'a SurfPal,
    cw: usize,
    ch: usize,
}

fn hypso(z: f32) -> [f32; 3] {
    let stops: [(f32, [f32; 3]); 7] = [
        (-50.0, [0.20, 0.35, 0.55]),
        (0.0, [0.55, 0.60, 0.45]),
        (15.0, [0.50, 0.62, 0.38]),
        (60.0, [0.62, 0.66, 0.42]),
        (150.0, [0.70, 0.62, 0.45]),
        (350.0, [0.62, 0.55, 0.48]),
        (800.0, [0.95, 0.95, 0.95]),
    ];
    for k in 0..stops.len() - 1 {
        let (a, ca) = stops[k];
        let (b, cb) = stops[k + 1];
        if z <= b {
            let t = ((z - a) / (b - a)).clamp(0.0, 1.0);
            return [
                ca[0] + (cb[0] - ca[0]) * t,
                ca[1] + (cb[1] - ca[1]) * t,
                ca[2] + (cb[2] - ca[2]) * t,
            ];
        }
    }
    stops[stops.len() - 1].1
}

impl Src<'_> {
    fn h(&self, c: usize, r: usize) -> Option<f32> {
        unhq(self.top[r.min(H - 1) * W + c.min(W - 1)].load(Ordering::Relaxed))
    }
    fn pixel(&self, layer: &str, c: usize, r: usize) -> [u8; 4] {
        let k = r * W + c;
        let ck = (r / COARSE).min(self.ch - 1) * self.cw + (c / COARSE).min(self.cw - 1);
        match layer {
            "terrain" => {
                let Some(z) = self.h(c, r) else {
                    return [40, 80, 140, 255];
                };
                let zx =
                    self.h(c + 1, r).unwrap_or(z) - self.h(c.saturating_sub(1), r).unwrap_or(z);
                let zy =
                    self.h(c, r + 1).unwrap_or(z) - self.h(c, r.saturating_sub(1)).unwrap_or(z);
                // light from the north-west
                let n = glam::Vec3::new(-zx / (2.0 * CELL), zy / (2.0 * CELL), 1.0).normalize();
                let l = glam::Vec3::new(-0.5, 0.5, 0.7).normalize();
                let shade = (n.dot(l)).clamp(0.0, 1.0) * 0.75 + 0.35;
                let t = hypso(z);
                [
                    (t[0] * shade * 255.0).min(255.0) as u8,
                    (t[1] * shade * 255.0).min(255.0) as u8,
                    (t[2] * shade * 255.0).min(255.0) as u8,
                    255,
                ]
            }
            "surfaces" => {
                let v = self.top[k].load(Ordering::Relaxed);
                if v >> 16 == 0 {
                    return [0, 0, 0, 0];
                }
                self.surf.colour((v & 0xffff) as u16)
            }
            "water" => {
                if self.top[k].load(Ordering::Relaxed) >> 16 == 0 {
                    [40, 90, 190, 200]
                } else {
                    [0, 0, 0, 0]
                }
            }
            "buildings" => match self.bld[k] {
                1 => {
                    // keep footprint cells that stand clear of the local ground
                    let g = self.ground[ck];
                    match self.h(c, r) {
                        Some(z)
                            if !g.is_nan()
                                && z - g > 3.0
                                && self.nav[k].load(Ordering::Relaxed) & 1 == 0 =>
                        {
                            let t = ((z - g) / 60.0).clamp(0.0, 1.0);
                            [
                                (235.0 - 60.0 * t) as u8,
                                (120.0 - 40.0 * t) as u8,
                                (60.0 + 120.0 * t) as u8,
                                230,
                            ]
                        }
                        _ => [0, 0, 0, 0],
                    }
                }
                2 => [150, 90, 220, 200],
                _ => [0, 0, 0, 0],
            },
            "navmesh" => match self.nav[k].load(Ordering::Relaxed) {
                0 => [0, 0, 0, 0],
                b if b & 1 != 0 => [255, 150, 30, 200],
                _ => [60, 140, 255, 150],
            },
            "density" => {
                let a = self.cw as f32; // keep the compiler quiet about unused
                let _ = a;
                let v = (self.veg[ck] / 900.0).sqrt().min(1.0);
                let p = (self.prop[ck] / 40.0).sqrt().min(1.0);
                if v + p < 0.01 {
                    return [0, 0, 0, 0];
                }
                [
                    (p * 255.0) as u8,
                    (v * 220.0) as u8,
                    (p * 200.0) as u8,
                    (((v + p).min(1.0)) * 200.0 + 40.0) as u8,
                ]
            }
            "gaps" => {
                let w = self.gap_w[ck];
                if w <= 0.0 {
                    return [0, 0, 0, 0];
                }
                let f = (self.gap_flat[ck] / w).clamp(0.0, 1.0);
                let l = (self.gap_low[ck] / w).clamp(0.0, 1.0);
                let s = (f + 0.5 * l).clamp(0.0, 1.0);
                let col = if f >= l {
                    [230, 40, 40]
                } else {
                    [240, 180, 30]
                };
                let g = [50.0, 170.0, 80.0];
                let mix = |a: f32, b: f32| (a + (b - a) * s) as u8;
                [
                    mix(g[0], col[0] as f32),
                    mix(g[1], col[1] as f32),
                    mix(g[2], col[2] as f32),
                    (90.0 + 140.0 * s) as u8,
                ]
            }
            "lod" => match self.lod[ck] {
                0 => [0, 0, 0, 0],
                v => {
                    let t = (v as f32 / 5.0).min(1.0);
                    [
                        (80.0 + 120.0 * (1.0 - t)) as u8,
                        (60.0 + 80.0 * (1.0 - t)) as u8,
                        (180.0 - 60.0 * t) as u8,
                        (40.0 + 15.0 * v as f32) as u8,
                    ]
                }
            },
            _ => [0, 0, 0, 0],
        }
    }
}
