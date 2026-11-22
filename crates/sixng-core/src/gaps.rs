//! BCPack texture-gap census (`sixng texture-gaps`):
//! which materials lost texture detail to BCPack, how badly, and how much
//! of the map uses them.
//!
//! * Per texture: `flat` (every mip has bytes in BCPack chunks that do not
//!   decode, i.e. any format but BC1; the export writes a flat fallback
//!   colour), `lowmip` (top mips in such chunks; exported at mip N), `ok`. Same rule as the exporter (`ttpf::write_png`), checked
//!   from the chunk table only, without decoding.
//! * Per material (drawable shader): class (terrain / road / building /
//!   prop / foliage / vehicle / water / interior ...), the slot role of each
//!   affected texture, and the worst status.
//! * Usage weight from placement JSON (`sixng placements`) and
//!   `batches.jsonl`: placement count and footprint area (local bbox x/y
//!   extent times scale², overlaps not removed).
//! * Terrain and layered materials: texture slots grouped into layers
//!   (a colour slot starts a layer); each layer's surface type is guessed
//!   from its mean colour (when any mip decodes), the model name, and the
//!   collision surface categories under the model's placements.
use crate::export::Progress;
use crate::game::ArchiveInfo;
use crate::resource::Resource;
use crate::scan::Entry;
use crate::ttpf::{Ttpf, TtpfSet, TtpfTex};
use anyhow::Result;
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub struct GapInputs {
    /// Placement directories (`<out>/placements/<archive>`: `*.json` entity
    /// files and/or `batches.jsonl`) or single `batches.jsonl` files.
    pub placements: Vec<PathBuf>,
    /// Output dir of the other exports (for `collision/<archive>/index.jsonl`,
    /// `surface_categories.json` and cached `textures/ttpf/*.json` stats).
    pub out: PathBuf,
}

#[derive(Clone)]
struct Tex {
    store: String,
    w: u32,
    h: u32,
    fmt: u32,
    status: &'static str,
    mip: u32,
}

/// Is any byte of [a, b) inside the sorted, merged ranges?
fn overlaps(r: &[(u64, u64)], a: u64, b: u64) -> bool {
    let i = r.partition_point(|x| x.1 <= a);
    i < r.len() && r[i].0 < b
}

fn tex_status(ranges: &[(u64, u64)], t: &TtpfTex) -> (&'static str, u32) {
    let Some(l) = t.layout() else { return ("undecodable", 0) };
    let best = (0..l.mips.len()).find(|&m| {
        let (a, b) = l.mip_span(m);
        !overlaps(ranges, t.virt + a, t.virt + b)
    });
    match best {
        Some(0) => ("ok", 0),
        Some(m) => ("lowmip", m as u32),
        None => ("flat", 0),
    }
}

/// Mean colour of a texture from a small decodable mip (>= 16 px where
/// possible), or from the exporter's cached stats.
fn mean_colour(store: &Ttpf, ranges: &[(u64, u64)], t: &TtpfTex, m0: u32, cache: &Path) -> Option<[f32; 4]> {
    #[derive(Deserialize)]
    struct S {
        mean_rgba: [f32; 4],
    }
    if let Some(s) = std::fs::read(cache.join(format!("{:016x}.json", t.hash))).ok().and_then(|b| serde_json::from_slice::<S>(&b).ok()) {
        return Some(s.mean_rgba);
    }
    let l = t.layout()?;
    let ok = |m: usize| {
        let (a, b) = l.mip_span(m);
        !overlaps(ranges, t.virt + a, t.virt + b)
    };
    let mut m = m0 as usize;
    while m + 1 < l.mips.len() && (t.width >> (m + 1)) >= 16 && (t.height >> (m + 1)) >= 16 && ok(m + 1) {
        m += 1;
    }
    let (a, b) = l.mip_span(m);
    if b > (64 << 20) {
        return None;
    }
    let part = store.read(t.virt + a, b - a).ok()?;
    let mut buf = vec![0u8; b as usize];
    buf[a as usize..].copy_from_slice(&part);
    let (rgba, _, _) = crate::ttpf::decode_rgba(t, &buf, m as u32).ok()?;
    let mut s = [0f64; 4];
    for px in rgba.chunks_exact(4) {
        for k in 0..4 {
            s[k] += px[k] as f64;
        }
    }
    let n = (rgba.len() / 4).max(1) as f64;
    Some(s.map(|x| (x / n).round() as f32))
}

fn has(n: &str, keys: &[&str]) -> bool {
    keys.iter().any(|k| n.contains(k))
}

/// Material class from the shader class and the model name (name cues are
/// guesses; `class_source` says which one decided).
fn material_class(name: &str, shader_class: &str, archive: &str) -> (&'static str, &'static str) {
    let n = name.to_ascii_lowercase();
    let interior = n.starts_with("int_") || n.contains("_int_") || n.contains("interior");
    if shader_class == "water" {
        return ("water", "shader");
    }
    if archive.contains("vehicle") || matches!(shader_class, "car_paint" | "window") {
        return ("vehicle", "shader / archive");
    }
    if archive.contains("ped") {
        return ("ped", "archive");
    }
    if interior {
        return ("interior", "name");
    }
    if shader_class == "terrain_multilayer" {
        return ("terrain", "shader (multilayer blend)");
    }
    if has(&n, &["water", "river", "_lake", "ocean", "_pond", "_sea_"]) {
        return ("water", "name");
    }
    if has(&n, &["terrain", "_trn", "seabed", "beach", "cliff", "_grnd", "ground"]) {
        return ("terrain", "name");
    }
    if n.starts_with("veg_") || has(&n, &["tree", "bush", "hedge", "grass", "plant", "foliage", "leaf", "leaves", "palm", "shrub", "flower", "_ivy", "weed", "cactus", "reed"]) || shader_class == "cutout" {
        return ("foliage", if shader_class == "cutout" { "shader (cutout) / name" } else { "name" });
    }
    if has(&n, &["road", "_rd_", "_rd0", "_rds", "street", "hwy", "freeway", "highway", "junc", "_jn_", "pavement", "sidewalk", "kerb", "curb", "crossing", "parking", "carpark", "rail", "track", "bridge", "tunnel", "lane"]) {
        return ("road", "name");
    }
    if has(&n, &["verge", "embank", "dirt", "sand", "_hill", "rock", "slope", "_mud", "gravel", "dune", "_land", "field", "meadow", "mound"]) {
        return ("terrain", "name");
    }
    if has(&n, &["bld", "build", "house", "shell", "facade", "roof", "wall", "apart", "tower", "store", "shop", "garage", "hotel", "motel", "office", "church", "station", "warehouse", "_hse", "_hs_", "mall", "factory", "hangar", "barn", "shed", "block", "condo", "_res_", "_dtl"]) {
        return ("building", "name");
    }
    match shader_class {
        "emissive" => ("prop", "default (emissive shader)"),
        "glass" => ("building", "shader (glass)"),
        "decal" => ("decal", "shader"),
        _ => ("prop", "default"),
    }
}

/// Slot role from the parameter hash and texture format.
fn slot_role(param_hash: &str, dxgi: u32, shader_class: &str) -> &'static str {
    match param_hash {
        "c8e8c282" => "base_colour",
        "1e77bf4c" | "84c69350" | "58b6e707" => "normal",
        "4d56d7fe" | "2bb2ede8" => "normal_atlas",
        "739e3d0d" => "packed_roughness_metal_ao",
        "c6f46e79" => "detail_normal_or_mask",
        "0df47048" => "detail_colour",
        "7485dafb" => "colour_with_alpha",
        "f151c063" => "decal",
        "d37026cb" | "640111c6" => "dirt_mask",
        "b9f4bdac" => "grime",
        "84d22f74" => "scratches",
        "fb03ae36" => "mask_pack",
        // checked visually on decoded low mips
        "de84c011" | "4c759bf1" | "27dbd2be" | "7e1f7f44" | "1a016c7e" => "normal (layer / detail, inferred)",
        "c075f9b6" => "layer_blend_mask (RGB weights, inferred)",
        _ => {
            let srgb = matches!(dxgi, 29 | 72 | 75 | 78 | 91 | 99);
            match (srgb, dxgi) {
                (true, _) if shader_class == "emissive" => "emissive",
                (true, _) => "colour (sRGB, unnamed slot)",
                (false, 83) => "normal",
                _ => "data (linear, unnamed slot)",
            }
        }
    }
}

fn is_colour_role(r: &str) -> bool {
    matches!(r, "base_colour" | "detail_colour" | "colour_with_alpha" | "emissive" | "colour (sRGB, unnamed slot)")
}

/// Surface type from a mean colour (0..255).
fn colour_surface(c: [f32; 4]) -> &'static str {
    let (r, g, b) = (c[0] / 255.0, c[1] / 255.0, c[2] / 255.0);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let v = mx;
    let s = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
    if s < 0.13 || mx - mn < 0.04 {
        return if v < 0.30 { "asphalt (dark grey)" } else if v < 0.62 { "concrete / rock (grey)" } else { "concrete / pale stone (light grey)" };
    }
    let d = mx - mn;
    let mut hue = if mx == r { 60.0 * ((g - b) / d) } else if mx == g { 60.0 * ((b - r) / d + 2.0) } else { 60.0 * ((r - g) / d + 4.0) };
    if hue < 0.0 {
        hue += 360.0;
    }
    match hue {
        h if (70.0..170.0).contains(&h) => "grass / vegetation",
        h if (45.0..70.0).contains(&h) => {
            if v > 0.55 {
                "dry grass / sand"
            } else {
                "dirt / dry grass"
            }
        }
        h if (15.0..45.0).contains(&h) => {
            if v > 0.62 && s < 0.45 {
                "sand"
            } else {
                "dirt / soil"
            }
        }
        h if !(15.0..330.0).contains(&h) => "red soil / brick",
        h if (170.0..260.0).contains(&h) => "water / blue-grey",
        _ => "other",
    }
}

fn name_surface(n: &str) -> Option<&'static str> {
    let n = n.to_ascii_lowercase();
    let t: &[(&[&str], &str)] = &[
        (&["sand", "beach", "dune"], "sand"),
        (&["grass", "lawn", "meadow", "field", "verge"], "grass"),
        (&["mud", "swamp", "marsh"], "mud"),
        (&["dirt", "soil", "track"], "dirt"),
        (&["gravel", "ballast", "rail"], "gravel"),
        (&["rock", "cliff", "stone", "boulder"], "rock"),
        (&["asphalt", "tarmac", "road", "_rd_", "hwy", "freeway", "street", "parking", "carpark"], "asphalt"),
        (&["concrete", "pavement", "sidewalk", "kerb", "curb", "plaza"], "concrete"),
        (&["snow"], "snow"),
        (&["water", "river", "lake"], "water"),
    ];
    t.iter().find(|(k, _)| has(&n, k)).map(|x| x.1)
}

#[derive(Default, Clone)]
struct Use {
    count: u64,
    scale2: f64,
    samples: Vec<[f32; 3]>,
}

impl Use {
    fn add(&mut self, n: u64, s2: f64, p: Option<[f32; 3]>) {
        self.count += n;
        self.scale2 += s2;
        if let Some(p) = p {
            if self.samples.len() < 24 && (self.count < 8 || self.count % 64 == 0 || self.samples.is_empty()) {
                self.samples.push(p);
            }
        }
    }
    fn merge(&mut self, o: Use) {
        self.count += o.count;
        self.scale2 += o.scale2;
        for p in o.samples {
            if self.samples.len() < 24 {
                self.samples.push(p);
            }
        }
    }
}

#[derive(Deserialize)]
struct PEnt {
    model_id: Option<String>,
    position: Option<[f32; 3]>,
    scale_xy: Option<f32>,
}
#[derive(Deserialize)]
struct PFile {
    #[serde(default)]
    entities: Vec<PEnt>,
}
#[derive(Deserialize)]
struct PBatch {
    model_id: Option<String>,
    count: Option<u64>,
    bbox: Option<[[f32; 3]; 2]>,
}

fn read_usage(paths: &[PathBuf]) -> (HashMap<String, Use>, Value) {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut jsonl: Vec<PathBuf> = Vec::new();
    for p in paths {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    let q = e.path();
                    match q.extension().and_then(|x| x.to_str()) {
                        Some("json") => files.push(q),
                        Some("jsonl") if q.file_name().map(|f| f == "batches.jsonl").unwrap_or(false) => jsonl.push(q),
                        _ => {}
                    }
                }
            }
        } else if p.is_file() {
            jsonl.push(p.clone());
        }
    }
    let ents = AtomicU64::new(0);
    let mut u: HashMap<String, Use> = files
        .par_iter()
        .fold(HashMap::new, |mut m: HashMap<String, Use>, f| {
            if let Some(pf) = std::fs::read(f).ok().and_then(|b| serde_json::from_slice::<PFile>(&b).ok()) {
                ents.fetch_add(pf.entities.len() as u64, Ordering::Relaxed);
                for e in pf.entities {
                    if let Some(id) = e.model_id {
                        let s = e.scale_xy.unwrap_or(1.0) as f64;
                        m.entry(id).or_default().add(1, s * s, e.position);
                    }
                }
            }
            m
        })
        .reduce(HashMap::new, |mut a, b| {
            for (k, v) in b {
                a.entry(k).or_default().merge(v);
            }
            a
        });
    let mut batched = 0u64;
    for j in &jsonl {
        let Ok(f) = std::fs::File::open(j) else { continue };
        use std::io::BufRead;
        for l in std::io::BufReader::new(f).lines().map_while(|l| l.ok()) {
            if let Ok(b) = serde_json::from_str::<PBatch>(&l) {
                if let Some(id) = b.model_id {
                    let n = b.count.unwrap_or(1);
                    batched += n;
                    let c = b.bbox.map(|bb| [(bb[0][0] + bb[1][0]) / 2.0, (bb[0][1] + bb[1][1]) / 2.0, (bb[0][2] + bb[1][2]) / 2.0]);
                    u.entry(id).or_default().add(n, n as f64, c);
                }
            }
        }
    }
    let info = json!({"entity_files": files.len(), "entities": ents.load(Ordering::Relaxed), "batch_files": jsonl.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(), "batched_instances": batched, "models_placed": u.len()});
    (u, info)
}

/// World-space collision bounds with their surface ids, on a 200 m grid.
struct CollGrid {
    items: Vec<([[f32; 3]; 2], Vec<(String, f64)>)>,
    grid: HashMap<(i32, i32), Vec<usize>>,
    cats: HashMap<String, String>,
}

impl CollGrid {
    fn load(out: &Path, archive: &str) -> Option<CollGrid> {
        let dir = out.join("collision").join(archive.trim_end_matches(".rpf"));
        let t = std::fs::read_to_string(dir.join("index.jsonl")).ok()?;
        let mut items = Vec::new();
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for l in t.lines() {
            let Ok(x) = serde_json::from_str::<Value>(l) else { continue };
            let Ok(bb) = serde_json::from_value::<[[f32; 3]; 2]>(x["bbox"].clone()) else { continue };
            if crate::surface::is_local_xy(&bb) {
                continue;
            }
            let mats: Vec<(String, f64)> = x["materials"].as_array().map(|a| a.iter().filter_map(|m| Some((m["id"].as_str()?.to_string(), m["triangles"].as_f64()?))).collect()).unwrap_or_default();
            if mats.is_empty() {
                continue;
            }
            let i = items.len();
            for gx in (bb[0][0] / 200.0).floor() as i32..=(bb[1][0] / 200.0).floor() as i32 {
                for gy in (bb[0][1] / 200.0).floor() as i32..=(bb[1][1] / 200.0).floor() as i32 {
                    grid.entry((gx, gy)).or_default().push(i);
                }
            }
            items.push((bb, mats));
        }
        let mut cats = HashMap::new();
        if let Some(v) = std::fs::read(dir.join("surface_categories.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
            if let Some(o) = v["surfaces"].as_object() {
                for (k, s) in o {
                    if let Some(c) = s["category"].as_str() {
                        cats.insert(k.clone(), format!("{} ({})", c, s["confidence"].as_str().unwrap_or("?")));
                    }
                }
            }
        }
        Some(CollGrid { items, grid, cats })
    }

    /// Collision surface categories inside the footprints (centre +- r),
    /// weighted by triangles x overlap share; top 3 with shares.
    fn under(&self, pts: &[[f32; 3]], r: f32) -> Vec<Value> {
        let mut w: HashMap<String, f64> = HashMap::new();
        for p in pts {
            let (x0, y0, x1, y1) = (p[0] - r, p[1] - r, p[0] + r, p[1] + r);
            let mut seen = std::collections::HashSet::new();
            for gx in (x0 / 200.0).floor() as i32..=(x1 / 200.0).floor() as i32 {
                for gy in (y0 / 200.0).floor() as i32..=(y1 / 200.0).floor() as i32 {
                    for &i in self.grid.get(&(gx, gy)).into_iter().flatten() {
                        if !seen.insert(i) {
                            continue;
                        }
                        let (bb, mats) = &self.items[i];
                        let ox = (x1.min(bb[1][0]) - x0.max(bb[0][0])).max(0.0);
                        let oy = (y1.min(bb[1][1]) - y0.max(bb[0][1])).max(0.0);
                        let a = ((bb[1][0] - bb[0][0]) * (bb[1][1] - bb[0][1])).max(1.0);
                        let share = (ox * oy / a) as f64;
                        if share <= 0.0 || (p[2] < bb[0][2] - 30.0 || p[2] > bb[1][2] + 30.0) {
                            continue;
                        }
                        for (id, t) in mats {
                            *w.entry(id.clone()).or_default() += t * share;
                        }
                    }
                }
            }
        }
        // merge by category
        let mut byc: HashMap<String, f64> = HashMap::new();
        for (id, v) in w {
            let c = self.cats.get(&id).cloned().unwrap_or_else(|| format!("surface {id} (uncategorised)"));
            *byc.entry(c).or_default() += v;
        }
        let tot: f64 = byc.values().sum();
        let mut v: Vec<(String, f64)> = byc.into_iter().collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        v.into_iter().take(3).filter(|_| tot > 0.0).map(|(c, x)| json!({"category": c, "share": (x / tot * 100.0).round() / 100.0})).collect()
    }
}

struct Slot {
    param: usize,
    param_hash: String,
    role: &'static str,
    hash: u64,
    status: &'static str,
    layer: Option<usize>,
}

struct Mat {
    shader: usize,
    shader_hash: String,
    shader_class: String,
    class: &'static str,
    class_source: &'static str,
    slots: Vec<Slot>,
}

struct Model {
    id: String,
    name: String,
    archive: String,
    /// local bbox x/y extent (m)
    ext: [f32; 2],
    mats: Vec<Mat>,
}

fn worst(slots: &[Slot]) -> &'static str {
    if slots.iter().any(|s| s.status == "flat") {
        "flat"
    } else if slots.iter().any(|s| s.status == "lowmip") {
        "lowmip"
    } else {
        "ok"
    }
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Run the census over drawable entries of one or more archives.
pub fn texture_gaps(archives: &[(ArchiveInfo, Vec<Entry>)], inp: &GapInputs, progress: Progress) -> Result<(Value, Vec<Value>)> {
    let t0 = std::time::Instant::now();
    let Some(first) = archives.first() else { anyhow::bail!("no archives") };
    let set: &TtpfSet = crate::ttpf::global(&crate::export::game_root(&first.0));
    let ranges: Vec<Vec<(u64, u64)>> = set.stores.iter().map(|s| s.bcpack_ranges()).collect();
    let store_of: HashMap<usize, usize> = set.stores.iter().enumerate().map(|(i, s)| (s as *const Ttpf as usize, i)).collect();
    let store_name = |i: usize| set.stores[i].label();
    let tex_cache: Mutex<HashMap<u64, Option<Tex>>> = Mutex::new(HashMap::new());
    let look = |h: u64| -> Option<Tex> {
        if let Some(t) = tex_cache.lock().unwrap().get(&h) {
            return t.clone();
        }
        let r = set.get(h).map(|(s, t)| {
            let si = store_of[&(s as *const Ttpf as usize)];
            let (st, m) = tex_status(&ranges[si], t);
            Tex { store: store_name(si), w: t.width, h: t.height, fmt: t.dxgi_format, status: st, mip: m }
        });
        tex_cache.lock().unwrap().insert(h, r.clone());
        r
    };

    // 1. usage
    progress("reading placements", 0, 1);
    let (usage, usage_info) = read_usage(&inp.placements);

    // 2. drawables
    let total: u64 = archives.iter().flat_map(|a| &a.1).map(|e| if e.id.contains('#') { 1 } else { e.members.len().max(1) as u64 }).sum();
    let done = AtomicU64::new(0);
    let mut models: Vec<Model> = Vec::new();
    for (a, entries) in archives {
        let entries: Vec<_> = entries.iter().flat_map(|e| {
            if e.members.is_empty() || e.id.contains('#') { vec![e.clone()] } else {
                e.members.iter().filter_map(|n| crate::export::member_entry(e, n).ok()).collect()
            }
        }).collect();
        let resolver = crate::export::ModelResolver::new(&entries);
        let m: Vec<Model> = entries
            .par_iter()
            .filter_map(|e| {
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                if d % 256 == 0 {
                    progress(&e.id, d, total);
                }
                let (source, member) = resolver.resolve(&entries, &e.id).ok()?;
                let r = if member.is_some() { Resource::load(&a.path, source) } else { Resource::load_sys(&a.path, source) }.ok()?;
                let shaders = if let Some(name) = member {
                    let mut info = crate::drawable::describe(&r);
                    crate::drawable::select_member(&r, &mut info, name).ok()?;
                    info.shaders
                } else { crate::drawable::find_shaders(&r) };
                let name = e.name.clone().unwrap_or_else(|| e.id.clone());
                let ext = if let Some(name) = member {
                    crate::drawable::model_bounds(&r, Some(name)).map(|b| [b[1][0] - b[0][0], b[1][1] - b[0][1]]).unwrap_or([0.0; 2])
                } else if r.sys.len() >= 0x4c { [(r.f32(0x2c + 20) - r.f32(0x2c + 4)).abs(), (r.f32(0x2c + 24) - r.f32(0x2c + 8)).abs()] } else { [0.0, 0.0] };
                let ext = if ext.iter().all(|v| v.is_finite() && *v < 20000.0) { ext } else { [0.0, 0.0] };
                let mats = shaders
                    .iter()
                    .map(|s| {
                        let sc = crate::material::classify(&s.name_hash, &s.flags);
                        let shader_class = sc["class"].as_str().unwrap_or("opaque").to_string();
                        let (class, class_source) = material_class(&name, &shader_class, &a.id);
                        let layered = class == "terrain" || shader_class == "terrain_multilayer";
                        let mut layer: Option<usize> = None;
                        let slots = s
                            .textures
                            .iter()
                            .map(|t| {
                                let h = u64::from_str_radix(&t.hash64, 16).unwrap_or(0);
                                let tx = look(h);
                                let role = slot_role(&t.param_hash, tx.as_ref().map(|x| x.fmt).unwrap_or(0), &shader_class);
                                if layered && is_colour_role(role) {
                                    layer = Some(layer.map_or(0, |l| l + 1));
                                }
                                Slot {
                                    param: t.param,
                                    param_hash: t.param_hash.clone(),
                                    role,
                                    hash: h,
                                    status: tx.as_ref().map(|x| x.status).unwrap_or("missing"),
                                    layer: if layered { Some(layer.unwrap_or(0)) } else { None },
                                }
                            })
                            .collect();
                        Mat { shader: s.index, shader_hash: s.name_hash.clone(), shader_class, class, class_source, slots }
                    })
                    .collect();
                Some(Model { id: e.id.clone(), name, archive: a.id.clone(), ext, mats })
            })
            .collect();
        models.extend(m);
    }
    progress("summarising", total, total);

    // 3. mean colours for colour slots of terrain/road/layered materials
    let cache_dir = inp.out.join("textures").join("ttpf");
    let mut want: Vec<u64> = models
        .iter()
        .flat_map(|m| m.mats.iter())
        .filter(|mt| mt.slots.iter().any(|s| s.layer.is_some()) || matches!(mt.class, "terrain" | "road"))
        .flat_map(|mt| mt.slots.iter())
        .filter(|s| is_colour_role(s.role) && s.status != "flat" && s.status != "missing")
        .map(|s| s.hash)
        .collect();
    want.sort();
    want.dedup();
    let means: HashMap<u64, [f32; 4]> = want
        .par_iter()
        .filter_map(|&h| {
            let (s, t) = set.get(h)?;
            let si = store_of[&(s as *const Ttpf as usize)];
            let (_, m) = tex_status(&ranges[si], t);
            mean_colour(s, &ranges[si], t, m, &cache_dir).map(|c| (h, c))
        })
        .collect();

    let coll = CollGrid::load(&inp.out, &first.0.id);

    // 4. per-material records and summaries
    #[derive(Default)]
    struct ClassAgg {
        materials: u64,
        flat: u64,
        lowmip: u64,
        placed_materials: u64,
        placements: u64,
        placements_flat: u64,
        placements_lowmip: u64,
        area: f64,
        area_flat: f64,
        area_lowmip: f64,
    }
    let mut by_class: BTreeMap<&str, ClassAgg> = BTreeMap::new();
    let mut by_role: BTreeMap<&str, [u64; 3]> = BTreeMap::new();
    let mut tex_used: HashMap<u64, &'static str> = HashMap::new();
    // one record per model with affected materials
    let mut recs: Vec<(u64, f64, Value)> = Vec::new();
    let mut flat_rows: Vec<(u64, f64, Value)> = Vec::new();
    // (surface, status, affected kind) -> [layers, placements, area]
    let mut terrain_layers: BTreeMap<(String, &str, &str), [f64; 3]> = BTreeMap::new();
    let mut terrain_roles: BTreeMap<&str, [u64; 2]> = BTreeMap::new();
    let mut by_format: BTreeMap<String, [u64; 3]> = BTreeMap::new();
    let mut colour_flat = 0u64;
    let mut colour_lowmip = 0u64;
    for m in &models {
        let u = usage.get(&m.id);
        let (cnt, s2) = u.map(|u| (u.count, u.scale2)).unwrap_or((0, 0.0));
        let lod_proxy = m.name.ends_with("_lod") || m.name.contains("slod") || m.name.contains("imposter") || m.name.contains("_lod_");
        // LOD proxies cover the same ground as their HD models: no area.
        let area = if lod_proxy { 0.0 } else { (m.ext[0] * m.ext[1]) as f64 * s2 };
        let nsurf = name_surface(&m.name);
        let mut coll_cue: Option<Vec<Value>> = None;
        let mut mrecs: Vec<Value> = Vec::new();
        // area share per material (a model with several materials covers its footprint once)
        let ma = area / m.mats.len().max(1) as f64;
        for mt in &m.mats {
            let st = worst(&mt.slots);
            let c = by_class.entry(mt.class).or_default();
            c.materials += 1;
            if cnt > 0 {
                c.placed_materials += 1;
            }
            c.placements += cnt;
            c.area += ma;
            match st {
                "flat" => {
                    c.flat += 1;
                    c.placements_flat += cnt;
                    c.area_flat += ma;
                }
                "lowmip" => {
                    c.lowmip += 1;
                    c.placements_lowmip += cnt;
                    c.area_lowmip += ma;
                }
                _ => {}
            }
            for s in &mt.slots {
                let k = by_role.entry(s.role).or_default();
                k[match s.status {
                    "flat" => 0,
                    "lowmip" => 1,
                    _ => 2,
                }] += 1;
                tex_used.insert(s.hash, s.status);
            }
            if st == "ok" {
                continue;
            }
            let cs: Vec<&Slot> = mt.slots.iter().filter(|s| is_colour_role(s.role)).collect();
            let colour_st = if cs.iter().any(|s| s.status == "flat") { "flat" } else if cs.iter().any(|s| s.status == "lowmip") { "lowmip" } else { "ok" };
            match colour_st {
                "flat" => colour_flat += 1,
                "lowmip" => colour_lowmip += 1,
                _ => {}
            }
            let layered = mt.slots.iter().any(|s| s.layer.is_some());
            // affected slots only: [param, param_hash, role, texture, status, mip, layer]
            let slots: Vec<Value> = mt
                .slots
                .iter()
                .filter(|s| s.status == "flat" || s.status == "lowmip")
                .map(|s| json!([s.param, s.param_hash, s.role, format!("{:016x}", s.hash), s.status, if s.status == "lowmip" { look(s.hash).map(|t| t.mip) } else { None }, s.layer]))
                .collect();
            let mut rec = json!({"shader": mt.shader, "shader_hash": mt.shader_hash, "shader_class": mt.shader_class, "class": mt.class, "class_source": mt.class_source,
                "status": st, "colour_status": colour_st, "slots": slots});
            if st == "flat" {
                let roles: Vec<&str> = mt.slots.iter().filter(|s| s.status == "flat").map(|s| s.role).collect();
                flat_rows.push((cnt, area, json!({"model": m.name, "model_id": m.id, "shader": mt.shader, "class": mt.class, "placements": cnt, "area_m2": r1(area), "lod_proxy": lod_proxy, "flat_roles": roles, "colour_status": colour_st})));
            }
            if layered || matches!(mt.class, "terrain" | "road") {
                if coll_cue.is_none() {
                    coll_cue = Some(match (&coll, u) {
                        (Some(g), Some(u)) if !u.samples.is_empty() => g.under(&u.samples, (m.ext[0].max(m.ext[1]) / 2.0).clamp(2.0, 300.0)),
                        _ => vec![],
                    });
                }
                let nl = mt.slots.iter().filter_map(|s| s.layer).max().map(|x| x + 1).unwrap_or(1);
                let layers: Vec<Value> = (0..nl)
                    .filter_map(|l| {
                        let ls: Vec<&Slot> = mt.slots.iter().filter(|s| s.layer.unwrap_or(0) == l).collect();
                        let lst = if ls.iter().any(|s| s.status == "flat") { "flat" } else if ls.iter().any(|s| s.status == "lowmip") { "lowmip" } else { "ok" };
                        if lst == "ok" {
                            return None;
                        }
                        let hit: Vec<&'static str> = ls.iter().filter(|s| s.status == lst).map(|s| s.role).collect();
                        let kind = if hit.iter().any(|r| is_colour_role(r)) { "colour" } else { "normal/data only" };
                        for r in &hit {
                            terrain_roles.entry(*r).or_default()[if lst == "flat" { 0 } else { 1 }] += 1;
                        }
                        let colour = ls.iter().filter(|s| is_colour_role(s.role)).find_map(|s| means.get(&s.hash).copied());
                        let ccue = colour.map(colour_surface);
                        let cc = coll_cue.as_ref().and_then(|v| v.first()).and_then(|x| x["category"].as_str()).map(|s| s.to_string());
                        let (guess, src) = match (ccue, nsurf, &cc) {
                            (Some(c), _, _) => (c.to_string(), "texture colour"),
                            (None, Some(n), _) => (n.to_string(), "model name"),
                            (None, None, Some(c)) => (c.clone(), "collision"),
                            _ => ("unknown".to_string(), "none"),
                        };
                        let k = terrain_layers.entry((guess.clone(), lst, kind)).or_default();
                        k[0] += 1.0;
                        k[1] += cnt as f64;
                        k[2] += ma / nl as f64;
                        Some(json!({"layer": l, "status": lst, "affected": kind, "affected_roles": hit, "surface_type": guess, "surface_source": src, "colour_rgba": colour}))
                    })
                    .collect();
                rec["layers"] = json!(layers);
            }
            mrecs.push(rec);
        }
        if mrecs.is_empty() {
            continue;
        }
        let worst_m = if mrecs.iter().any(|r| r["status"] == "flat") { "flat" } else { "lowmip" };
        let mut rec = json!({"model_id": m.id, "model": m.name, "archive": m.archive, "status": worst_m, "placements": cnt, "area_m2": r1(area),
            "footprint_m": [r1(m.ext[0] as f64), r1(m.ext[1] as f64)], "lod_proxy": lod_proxy, "materials": mrecs});
        if let Some(c) = coll_cue.filter(|c| !c.is_empty()) {
            rec["collision_under"] = json!(c);
        }
        if let Some(n) = nsurf {
            rec["name_surface"] = json!(n);
        }
        recs.push((cnt, area, rec));
    }
    recs.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.partial_cmp(&a.1).unwrap()));
    flat_rows.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.partial_cmp(&a.1).unwrap()));


    let pct = |a: u64, b: u64| if b == 0 { 0.0 } else { (a as f64 / b as f64 * 1000.0).round() / 10.0 };
    let classes: BTreeMap<&str, Value> = by_class
        .iter()
        .map(|(k, c)| {
            (*k, json!({"materials": c.materials, "flat": c.flat, "lowmip": c.lowmip, "pct_flat": pct(c.flat, c.materials), "pct_lowmip": pct(c.lowmip, c.materials),
                "placements": c.placements, "placements_flat": c.placements_flat, "placements_lowmip": c.placements_lowmip,
                "pct_placements_flat": pct(c.placements_flat, c.placements), "pct_placements_lowmip": pct(c.placements_lowmip, c.placements),
                "area_km2": (c.area / 1e4).round() / 100.0, "area_km2_flat": (c.area_flat / 1e4).round() / 100.0, "area_km2_lowmip": (c.area_lowmip / 1e4).round() / 100.0}))
        })
        .collect();
    let tot = |f: fn(&ClassAgg) -> u64| by_class.values().map(f).sum::<u64>();
    let (tm, tf, tl) = (tot(|c| c.materials), tot(|c| c.flat), tot(|c| c.lowmip));
    let (tp, tpf, tpl) = (tot(|c| c.placements), tot(|c| c.placements_flat), tot(|c| c.placements_lowmip));
    let mut tcount = [0u64; 3];
    for s in tex_used.values() {
        tcount[match *s {
            "flat" => 0,
            "lowmip" => 1,
            _ => 2,
        }] += 1;
    }
    let mut lowmip_res: BTreeMap<String, u64> = BTreeMap::new();
    let mut textures: BTreeMap<String, Value> = BTreeMap::new();
    for (h, s) in &tex_used {
        let Some(t) = look(*h) else { continue };
        by_format.entry(crate::texture::dxgi_name(t.fmt).to_string()).or_default()[match *s {
            "flat" => 0,
            "lowmip" => 1,
            _ => 2,
        }] += 1;
        if *s != "flat" && *s != "lowmip" {
            continue;
        }
        let (ew, eh) = ((t.w >> t.mip).max(1), (t.h >> t.mip).max(1));
        if *s == "lowmip" {
            *lowmip_res.entry(format!("{}x{} -> {}x{}", t.w, t.h, ew, eh)).or_default() += 1;
        }
        let mut v = json!({"store": t.store, "size": [t.w, t.h], "format": crate::texture::dxgi_name(t.fmt), "status": t.status});
        if *s == "lowmip" {
            v["mip"] = json!(t.mip);
            v["export_size"] = json!([ew, eh]);
        }
        if let Some(c) = means.get(h) {
            v["mean_rgba"] = json!(c);
        }
        textures.insert(format!("{h:016x}"), v);
    }
    let mut lr: Vec<(String, u64)> = lowmip_res.into_iter().collect();
    lr.sort_by(|a, b| b.1.cmp(&a.1));
    let top_flat: Vec<Value> = flat_rows.iter().take(50).map(|r| r.2.clone()).collect();
    let top_flat_colour: Vec<Value> = flat_rows.iter().filter(|r| r.2["lod_proxy"] == false && r.2["colour_status"] == "flat").take(50).map(|r| r.2.clone()).collect();
    let mut tl_rows: Vec<Value> = terrain_layers
        .iter()
        .map(|((s, st, k), v)| json!({"surface_type": s, "status": st, "affected": k, "layers": v[0], "placements": v[1], "area_km2": (v[2] / 1e4).round() / 100.0}))
        .collect();
    tl_rows.sort_by(|a, b| b["area_km2"].as_f64().partial_cmp(&a["area_km2"].as_f64()).unwrap());
    let roles: BTreeMap<&str, Value> = by_role.iter().map(|(k, v)| (*k, json!({"flat": v[0], "lowmip": v[1], "ok_or_other": v[2]}))).collect();

    Ok((json!({
        "note": "BCPack texture gaps (sixng texture-gaps). status: flat = every mip has bytes in BCPack chunks that do not decode (Xbox hardware texture codec; BC1, BC3 and BC7 decode, the other formats not yet); the export writes a flat fallback colour (model's decoded colour average, flat normal, mid grey for data maps). lowmip = top mips in such chunks, exported at mip N (resolution in `textures`). One record per affected material (drawable shader); `status` is its worst slot. placements = entity placements + batched instances using the model; area_m2 = local bbox x*y extent x scale^2 summed over placements (overlaps counted twice; LOD proxies cover the same ground as their HD models). Terrain / layered materials: a colour slot starts a layer (guess from slot order); surface_type from the layer's mean colour (when any mip decodes), else the model name, else the collision surface categories under the placements (inferred, see surface_categories.json). Classes come from shader families and model names (class_source).",
        "generated_by": "sixng texture-gaps",
        "seconds": t0.elapsed().as_secs_f64().round(),
        "inputs": {"archives": archives.iter().map(|a| json!({"archive": a.0.id, "drawables": a.1.len()})).collect::<Vec<_>>(), "usage": usage_info, "collision_cue": coll.is_some()},
        "summary": {
            "materials": {"total": tm, "flat": tf, "lowmip": tl, "pct_flat": pct(tf, tm), "pct_lowmip": pct(tl, tm),
                          "placements": tp, "placements_flat": tpf, "placements_lowmip": tpl, "pct_placements_flat": pct(tpf, tp), "pct_placements_lowmip": pct(tpl, tp)},
            "textures_referenced": {"flat": tcount[0], "lowmip": tcount[1], "ok_or_other": tcount[2]},
            "by_class": classes,
            "slots_by_role": roles,
            "lowmip_resolutions": lr.iter().take(20).map(|(k, v)| json!({"resolution": k, "textures": v})).collect::<Vec<_>>(),
            "materials_colour_affected": {"flat": colour_flat, "lowmip": colour_lowmip, "note": "materials whose colour slot (base / detail / sRGB colour) itself is flat or low-mip; the rest lost only normal / packed / mask data"},
            "textures_by_format": by_format.iter().map(|(k, v)| (k.clone(), json!({"flat": v[0], "lowmip": v[1], "ok_or_other": v[2]}))).collect::<BTreeMap<_, _>>(),
            "top_flat_materials": top_flat,
            "top_flat_colour_materials_hd": top_flat_colour,
            "terrain_layer_roles": terrain_roles.iter().map(|(k, v)| (*k, json!({"flat": v[0], "lowmip": v[1]}))).collect::<BTreeMap<_, _>>(),
            "terrain_layers": tl_rows,
        },
        "textures": textures,
    }), recs.into_iter().map(|r| r.2).collect()))
}

/// Write the per-model records as gzip-compressed JSON lines.
pub fn write_models_gz(path: &Path, models: &[Value]) -> Result<()> {
    use std::io::Write;
    let f = std::fs::File::create(path)?;
    let mut w = flate2::write::GzEncoder::new(std::io::BufWriter::new(f), flate2::Compression::default());
    for m in models {
        serde_json::to_writer(&mut w, m)?;
        w.write_all(b"
")?;
    }
    w.finish()?.flush()?;
    Ok(())
}
