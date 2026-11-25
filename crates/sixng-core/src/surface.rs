//! Collision-surface helpers used after `sixng collision`:
//!
//! * [`Locator`]: which collision surface id lies under a world point
//!   (bounds decoded on demand from the archive, 200 m grid over the
//!   bounds bboxes from `collision/<archive>/index.jsonl`).
//! * [`floor_fit`]: share of points standing on upward-facing triangles
//!   (interior second-pass linking, see `export::export_interiors`).
//! * [`surface_categories`]: inferred categories per surface id (rounds
//!   1-3), `sixng surfaces`.
use crate::bounds;
use crate::game::ArchiveInfo;
use crate::resource::Resource;
use crate::scan::Entry;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

pub type Tri = [[f32; 3]; 3];

/// Bounds list from `<out>/collision/<archive>/index.jsonl`: (id, bbox).
pub fn read_collision_index(out: &Path, archive_id: &str) -> Vec<(String, [[f32; 3]; 2])> {
    let mut v = Vec::new();
    if let Ok(t) = std::fs::read_to_string(out.join("collision").join(archive_id.trim_end_matches(".rpf")).join("index.jsonl")) {
        for l in t.lines() {
            if let Ok(x) = serde_json::from_str::<Value>(l) {
                if let (Some(id), Ok(bb)) = (x["id"].as_str(), serde_json::from_value::<[[f32; 3]; 2]>(x["bbox"].clone())) {
                    v.push((id.to_string(), bb));
                }
            }
        }
    }
    v
}

/// Interior-space bounds (props, interiors) sit near the origin; world
/// bounds don't. Same test as the research scripts: all of x/y min/max < 400.
pub fn is_local_xy(bb: &[[f32; 3]; 2]) -> bool {
    [bb[0][0], bb[0][1], bb[1][0], bb[1][1]].iter().all(|v| v.abs() < 400.0)
}

/// All six bbox values < 400 (interior-space bounds for interior linking).
pub fn is_local_xyz(bb: &[[f32; 3]; 2]) -> bool {
    bb.iter().flatten().all(|v| v.abs() < 400.0)
}

/// Decode one bounds file by id.
pub fn decode_bounds(archive: &Path, e: &Entry) -> Option<bounds::Collision> {
    let r = Resource::load_sys(archive, e).ok()?;
    bounds::decode(&r)
}

/// Triangles per surface id; `up_only` keeps those whose normal z /
/// |normal| > 0.5.
pub fn triangles(c: &bounds::Collision, up_only: bool) -> Vec<(u16, Tri)> {
    let mut out = Vec::new();
    for (id, (pos, idx)) in &c.by_mat {
        for t in idx.chunks_exact(3) {
            let tri = [pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]];
            if up_only {
                let n = normal(&tri);
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                if !(n[2] / (l + 1e-9) > 0.5) {
                    continue;
                }
            }
            out.push((*id, tri));
        }
    }
    out
}

fn normal(t: &Tri) -> [f64; 3] {
    let u = [0, 1, 2].map(|k| t[1][k] as f64 - t[0][k] as f64);
    let v = [0, 1, 2].map(|k| t[2][k] as f64 - t[0][k] as f64);
    [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
}

/// Barycentric height of `t` at (x, y), if (x, y) lies inside (tolerance `eps`).
fn height_at(t: &Tri, x: f64, y: f64, eps: f64) -> Option<f64> {
    let (x0, y0) = (t[0][0] as f64, t[0][1] as f64);
    let (x1, y1) = (t[1][0] as f64, t[1][1] as f64);
    let (x2, y2) = (t[2][0] as f64, t[2][1] as f64);
    let d = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2);
    if d.abs() < 1e-12 {
        return None;
    }
    let l0 = ((y1 - y2) * (x - x2) + (x2 - x1) * (y - y2)) / d;
    let l1 = ((y2 - y0) * (x - x2) + (x0 - x2) * (y - y2)) / d;
    let l2 = 1.0 - l0 - l1;
    if l0 < -eps || l1 < -eps || l2 < -eps {
        return None;
    }
    Some(l0 * t[0][2] as f64 + l1 * t[1][2] as f64 + l2 * t[2][2] as f64)
}

/// Share of `pts` that stand on an upward-facing triangle of `tris`
/// (surface 0.3 m above to 1.5 m below the point).
pub fn floor_fit(pts: &[[f32; 3]], tris: &[(u16, Tri)]) -> f64 {
    if pts.is_empty() || tris.is_empty() {
        return 0.0;
    }
    let boxes: Vec<[f32; 4]> = tris
        .iter()
        .map(|(_, t)| [t[0][0].min(t[1][0]).min(t[2][0]), t[0][1].min(t[1][1]).min(t[2][1]), t[0][0].max(t[1][0]).max(t[2][0]), t[0][1].max(t[1][1]).max(t[2][1])])
        .collect();
    let mut ok = 0;
    for p in pts {
        let hit = tris.iter().zip(&boxes).any(|((_, t), b)| {
            if p[0] < b[0] || p[0] > b[2] || p[1] < b[1] || p[1] > b[3] {
                return false;
            }
            match height_at(t, p[0] as f64, p[1] as f64, 1e-4) {
                Some(z) => {
                    let dz = p[2] as f64 - z;
                    dz > -0.3 && dz < 1.5
                }
                None => false,
            }
        });
        if hit {
            ok += 1;
        }
    }
    ok as f64 / pts.len() as f64
}

/// Bbox IoU in f64.
pub fn iou(a: &[[f32; 3]; 2], b: &[[f32; 3]; 2]) -> f64 {
    let vol = |lo: [f64; 3], hi: [f64; 3]| (0..3).map(|k| (hi[k] - lo[k]).max(0.0)).product::<f64>();
    let f = |v: [f32; 3]| v.map(|x| x as f64);
    let lo = [0, 1, 2].map(|k| (a[0][k] as f64).max(b[0][k] as f64));
    let hi = [0, 1, 2].map(|k| (a[1][k] as f64).min(b[1][k] as f64));
    let inter = vol(lo, hi);
    let u = vol(f(a[0]), f(a[1])) + vol(f(b[0]), f(b[1])) - inter;
    inter / u.max(1e-9)
}

/// Evenly spaced subsample of at most `n` items (deterministic).
pub fn spread<T: Clone>(v: &[T], n: usize) -> Vec<T> {
    if v.len() <= n {
        return v.to_vec();
    }
    (0..n).map(|k| v[k * v.len() / n].clone()).collect()
}

/// Surface id under world points; decodes bounds on demand.
pub struct Locator<'a> {
    archive: &'a Path,
    items: Vec<(&'a Entry, [[f32; 3]; 2])>,
    grid: HashMap<(i32, i32), Vec<usize>>,
    cache: HashMap<usize, Arc<Vec<(u16, Tri)>>>,
}

const CELL: f32 = 200.0;

impl<'a> Locator<'a> {
    /// World-space bounds only (interior-space ones are skipped).
    pub fn new(archive: &'a Path, coll: &[(String, [[f32; 3]; 2])], by_id: &HashMap<&str, &'a Entry>) -> Self {
        let mut items = Vec::new();
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (id, bb) in coll {
            if is_local_xy(bb) {
                continue;
            }
            let Some(e) = by_id.get(id.as_str()) else { continue };
            let i = items.len();
            items.push((*e, *bb));
            for gx in (bb[0][0] / CELL).floor() as i32..=(bb[1][0] / CELL).floor() as i32 {
                for gy in (bb[0][1] / CELL).floor() as i32..=(bb[1][1] / CELL).floor() as i32 {
                    grid.entry((gx, gy)).or_default().push(i);
                }
            }
        }
        Locator { archive, items, grid, cache: HashMap::new() }
    }

    fn tris(&mut self, i: usize) -> Arc<Vec<(u16, Tri)>> {
        if let Some(t) = self.cache.get(&i) {
            return t.clone();
        }
        let t = Arc::new(decode_bounds(self.archive, self.items[i].0).map(|c| triangles(&c, true)).unwrap_or_default());
        if self.cache.len() > 3000 {
            self.cache.clear();
        }
        self.cache.insert(i, t.clone());
        t
    }

    /// Surface id of the upward-facing triangle closest in height (< 1 m)
    /// directly under or over `p`.
    pub fn under(&mut self, p: [f32; 3]) -> Option<u16> {
        if !p.iter().all(|v| v.is_finite() && v.abs() < 1e5) {
            return None;
        }
        let cand = self.grid.get(&((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32))?.clone();
        let mut best: Option<(f64, u16)> = None;
        for i in cand {
            let bb = self.items[i].1;
            if !(bb[0][0] <= p[0] && p[0] <= bb[1][0] && bb[0][1] <= p[1] && p[1] <= bb[1][1] && bb[0][2] - 2.0 <= p[2] && p[2] <= bb[1][2] + 2.0) {
                continue;
            }
            for (id, t) in self.tris(i).iter() {
                if let Some(z) = height_at(t, p[0] as f64, p[1] as f64, 0.0) {
                    let dz = (z - p[2] as f64).abs();
                    if dz < 1.0 && best.map_or(true, |b| dz < b.0) {
                        best = Some((dz, *id));
                    }
                }
            }
        }
        best.map(|b| b.1)
    }
}

// ---------------------------------------------------------------- categories

#[derive(Default, Clone)]
struct Stat {
    area: f64,
    up: f64,
    vert: f64,
    zsum: f64,
    low: f64,
    files: usize,
    road_on: f64,
    road_tot: f64,
}

fn plant_fam_r2(n: &str) -> &'static str {
    for (k, v) in [
        ("ocean", "seabed"), ("seagrass", "seabed"), ("barnacle", "seabed"), ("algae", "seabed"), ("_uw", "seabed"), ("coral", "seabed"),
        ("beach", "sand"), ("sand", "sand"), ("dune", "sand"), ("crack", "paved"), ("litter", "paved"), ("mulch", "forest"), ("leaves", "forest"),
        ("fern", "forest"), ("needle", "forest"), ("debris", "debris"), ("grass", "grass"), ("weed", "grass"), ("flower", "grass"), ("reed", "wetland"),
        ("marsh", "wetland"), ("rock", "rock"), ("pebble", "rock"), ("stone", "rock"), ("bush", "shrub"), ("shrub", "shrub"),
    ] {
        if n.contains(k) {
            return v;
        }
    }
    "other"
}

fn plant_fam_r3(n: &str) -> &'static str {
    if ["_water", "duckweed", "reed", "marsh", "mangrove"].iter().any(|k| n.contains(k)) {
        return "wet";
    }
    for (k, v) in [
        ("ocean", "seabed"), ("seagrass", "seabed"), ("_uw", "seabed"), ("algae", "seabed"), ("coral", "seabed"), ("beach", "sand"), ("sand", "sand"),
        ("dune", "sand"), ("crack", "paved"), ("litter", "paved"), ("asphalt", "paved"), ("mulch", "forest"), ("leaves", "forest"), ("fern", "forest"),
        ("needle", "forest"), ("twig", "forest"), ("stick", "forest"), ("debris", "debris"), ("grass", "grass"), ("weed", "grass"), ("flower", "grass"),
        ("rock", "rock"), ("pebble", "rock"), ("stone", "rock"), ("bush", "shrub"), ("shrub", "shrub"),
    ] {
        if n.contains(k) {
            return v;
        }
    }
    "other"
}

fn prop_fam(n: &str) -> &'static str {
    for (k, v) in [
        ("reflector", "road-edge"), ("bollard", "sidewalk"), ("barrier", "barrier"), ("fence", "fence"), ("inwater", "water"), ("rail", "rail"),
        ("rock", "rock"), ("tree", "tree"), ("palm", "tree"), ("veg_", "veg"), ("bush", "veg"),
    ] {
        if n.contains(k) {
            return v;
        }
    }
    "other"
}

fn shares(c: &HashMap<String, usize>, fam: fn(&str) -> &'static str) -> (usize, BTreeMap<&'static str, f64>) {
    let tot: usize = c.values().sum();
    let mut f: BTreeMap<&'static str, f64> = BTreeMap::new();
    for (n, k) in c {
        *f.entry(fam(n)).or_default() += *k as f64;
    }
    if tot > 0 {
        for v in f.values_mut() {
            *v /= tot as f64;
        }
    }
    (tot, f)
}

fn top(f: &BTreeMap<&'static str, f64>, n: usize) -> Value {
    let mut v: Vec<_> = f.iter().collect();
    v.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap().then(a.0.cmp(b.0)));
    let mut m = serde_json::Map::new();
    for (k, x) in v.into_iter().take(n) {
        m.insert(k.to_string(), json!(r2(*x)));
    }
    Value::Object(m)
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}
fn r4(x: f64) -> f64 {
    (x * 10000.0).round() / 10000.0
}
fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Road raster: navmesh road polygons at 0.5 m in 128 m tiles.
struct RoadRaster {
    tiles: HashMap<(i32, i32), Vec<u64>>,
}

const RT: f32 = 128.0;
const RPX: usize = 256;

impl RoadRaster {
    fn new() -> Self {
        RoadRaster { tiles: HashMap::new() }
    }
    fn set(&mut self, x: f32, y: f32) {
        let (tx, ty) = ((x / RT).floor() as i32, (y / RT).floor() as i32);
        let u = (((x - tx as f32 * RT) / RT) * RPX as f32) as usize;
        let v = (((y - ty as f32 * RT) / RT) * RPX as f32) as usize;
        let t = self.tiles.entry((tx, ty)).or_insert_with(|| vec![0u64; RPX * RPX / 64]);
        let i = v.min(RPX - 1) * RPX + u.min(RPX - 1);
        t[i / 64] |= 1 << (i % 64);
    }
    fn get(&self, x: f32, y: f32) -> bool {
        let (tx, ty) = ((x / RT).floor() as i32, (y / RT).floor() as i32);
        let Some(t) = self.tiles.get(&(tx, ty)) else { return false };
        let u = ((((x - tx as f32 * RT) / RT) * RPX as f32) as usize).min(RPX - 1);
        let v = ((((y - ty as f32 * RT) / RT) * RPX as f32) as usize).min(RPX - 1);
        let i = v * RPX + u;
        t[i / 64] >> (i % 64) & 1 == 1
    }
    /// Fill a polygon (fan-triangulated) by sampling pixel centres.
    fn fill_poly(&mut self, pts: &[[f32; 3]]) {
        for k in 1..pts.len().saturating_sub(1) {
            let t: Tri = [pts[0], pts[k], pts[k + 1]];
            let (x0, x1) = (t.iter().map(|p| p[0]).fold(f32::MAX, f32::min), t.iter().map(|p| p[0]).fold(f32::MIN, f32::max));
            let (y0, y1) = (t.iter().map(|p| p[1]).fold(f32::MAX, f32::min), t.iter().map(|p| p[1]).fold(f32::MIN, f32::max));
            if x1 - x0 > 500.0 || y1 - y0 > 500.0 {
                continue;
            }
            let s = RT / RPX as f32;
            let mut y = (y0 / s).floor() * s + s / 2.0;
            while y <= y1 {
                let mut x = (x0 / s).floor() * s + s / 2.0;
                while x <= x1 {
                    if height_at(&t, x as f64, y as f64, 1e-6).is_some() {
                        self.set(x, y);
                    }
                    x += s;
                }
                y += s;
            }
        }
    }
}

pub struct SurfaceInputs<'a> {
    pub levels: &'a ArchiveInfo,
    pub level_entries: &'a [Entry],
    pub navmeshes: Option<(&'a ArchiveInfo, &'a [Entry])>,
}

pub struct SurfaceSummary {
    pub ids: usize,
    pub area_covered: f64,
    pub by_confidence: BTreeMap<String, f64>,
    pub samples: Value,
}

/// Railway surface ids: (id, category, confidence, evidence).
const RAIL_IDS: &[(&str, &str, &str, &str)] = &[
    ("2287", "gravel / ballast (dirt tracks, verges, railway beds)", "medium",
     "under 19-29 % of rail sleepers, signal boxes, rail wheels and rail-track pieces vs 0.4 % of collision area (~60x), but present in 12,158 bounds files map-wide, so not railway-only; path links on it are mostly unpaved-flagged"),
    ("4180", "gravel / ballast variant (railway verges, yards)", "low",
     "under 9-21 % of rail sleepers, signal boxes and rail stacks vs 0.13 % of collision area (~100x); in 5,535 bounds files, clustered"),
];

/// Inferred surface categories (rounds 1-4), written to
/// `<out>/collision/<archive>/surface_categories.json`. Needs the outputs
/// of `sixng collision`, `sixng paths` and `sixng placements --batches`
/// (missing ones only drop their cue). Sampling is deterministic.
pub fn surface_categories(inp: &SurfaceInputs, out: &Path, progress: &(dyn Fn(&str, u64, u64) + Sync)) -> Result<SurfaceSummary> {
    use rayon::prelude::*;
    let a = inp.levels;
    let short = a.id.trim_end_matches(".rpf").to_string();
    let coll = read_collision_index(out, &a.id);
    if coll.is_empty() {
        return Err(anyhow!("no collision index: run `sixng collision {}` first", short));
    }
    let by_id: HashMap<&str, &Entry> = inp.level_entries.iter().map(|e| (e.id.as_str(), e)).collect();

    // --- navmesh road raster
    let mut road = RoadRaster::new();
    if let Some((na, ne)) = inp.navmeshes {
        let polys: Vec<Vec<Vec<[f32; 3]>>> = ne
            .par_iter()
            .filter_map(|e| {
                let r = Resource::load_sys(&na.path, e).ok()?;
                let nm = crate::navmesh::decode(&r)?;
                let v: Vec<Vec<[f32; 3]>> = nm
                    .polys
                    .iter()
                    .filter(|p| p.0 & crate::navmesh::ROAD != 0)
                    .map(|p| p.1.iter().filter_map(|&i| nm.vertices.get(i as usize).copied()).collect())
                    .collect();
                if v.is_empty() {
                    None
                } else {
                    Some(v)
                }
            })
            .collect();
        for t in polys {
            for p in t {
                road.fill_poly(&p);
            }
        }
    }

    // --- round 1: geometry over every 20th index line (world-space only)
    let sample: Vec<(usize, &Entry)> = coll
        .iter()
        .enumerate()
        .filter(|(i, (_, bb))| i % 20 == 0 && !is_local_xy(bb))
        .filter_map(|(i, (id, _))| by_id.get(id.as_str()).map(|e| (i, *e)))
        .collect();
    let total = sample.len() as u64;
    let done = std::sync::atomic::AtomicU64::new(0);
    let parts: Vec<HashMap<u16, Stat>> = sample
        .par_iter()
        .filter_map(|(_, e)| {
            let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            progress(&e.id, d, total);
            let c = decode_bounds(&a.path, e)?;
            let mut m: HashMap<u16, Stat> = HashMap::new();
            for (id, t) in triangles(&c, false) {
                let n = normal(&t);
                let ar = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() / 2.0;
                if ar < 1e-4 {
                    continue;
                }
                let s = m.entry(id).or_default();
                let nz = n[2].abs() / (2.0 * ar);
                let zc = (t[0][2] as f64 + t[1][2] as f64 + t[2][2] as f64) / 3.0;
                s.area += ar;
                s.zsum += ar * zc;
                if nz > 0.8 {
                    s.up += ar;
                }
                if nz < 0.3 {
                    s.vert += ar;
                }
                if zc < 4.0 {
                    s.low += ar;
                }
                // road overlap: upward-ish triangles, centroid in the road raster
                let ah = n[2].abs() / 2.0;
                if ah >= 1e-3 {
                    let cx = (t[0][0] + t[1][0] + t[2][0]) / 3.0;
                    let cy = (t[0][1] + t[1][1] + t[2][1]) / 3.0;
                    s.road_tot += ah;
                    if road.get(cx, cy) {
                        s.road_on += ah;
                    }
                }
            }
            for s in m.values_mut() {
                s.files = 1;
            }
            Some(m)
        })
        .collect();
    let mut st: BTreeMap<String, Stat> = BTreeMap::new();
    for m in parts {
        for (id, s) in m {
            let x = st.entry(format!("{id:04x}")).or_default();
            x.area += s.area;
            x.up += s.up;
            x.vert += s.vert;
            x.zsum += s.zsum;
            x.low += s.low;
            x.files += s.files;
            x.road_on += s.road_on;
            x.road_tot += s.road_tot;
        }
    }
    let tot_area: f64 = st.values().map(|s| s.area).sum();
    let share = |k: &str| st.get(k).map(|s| s.area / tot_area).unwrap_or(0.0);
    let geo = |s: &Stat| (s.up / s.area, s.vert / s.area, s.zsum / s.area, s.low / s.area);

    // --- point samples: plants, props, path links
    let pdir = out.join("placements").join(&short);
    let mut grass_pts: Vec<([f32; 3], String)> = Vec::new();
    let mut prop_pts: Vec<([f32; 3], String)> = Vec::new();
    if let Ok(t) = std::fs::read_to_string(pdir.join("batches.jsonl")) {
        let mut grass = Vec::new();
        let mut props = Vec::new();
        for l in t.lines() {
            let Ok(b) = serde_json::from_str::<Value>(l) else { continue };
            let cnt = b["count"].as_u64().unwrap_or(0);
            if cnt == 0 {
                continue;
            }
            let name = b["name"].as_str().map(|s| s.to_string()).unwrap_or_else(|| b["archetype"].to_string());
            let first = b["first"].as_u64().unwrap_or(0);
            match b["kind"].as_str() {
                Some("grass") => grass.push((first, cnt, name)),
                Some("prop") => props.push((first, cnt, name)),
                _ => {}
            }
        }
        let read = |file: &str, rec: u64, list: Vec<(u64, u64, String)>, per: u64, out: &mut Vec<([f32; 3], String)>| {
            use std::io::{Read, Seek, SeekFrom};
            let Ok(mut f) = std::fs::File::open(pdir.join(file)) else { return };
            for (first, cnt, name) in list {
                let k = cnt.min(per);
                for j in 0..k {
                    let idx = first + j * cnt / k;
                    let mut b = [0u8; 12];
                    if f.seek(SeekFrom::Start(idx * rec)).is_err() || f.read_exact(&mut b).is_err() {
                        continue;
                    }
                    let p = [0, 1, 2].map(|i| f32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]));
                    out.push((p, name.clone()));
                }
            }
        };
        read("batch_grass.bin", 20, spread(&grass, 2500), 8, &mut grass_pts);
        read("batch_props.bin", 44, spread(&props, 3500), 6, &mut prop_pts);
    }
    let mut link_pts: Vec<([f32; 3], bool)> = Vec::new();
    if let Ok(t) = std::fs::read_to_string(out.join("paths").join(&short).join("graph.json")) {
        if let Ok(g) = serde_json::from_str::<Value>(&t) {
            let nodes = g["nodes"].as_array().cloned().unwrap_or_default();
            let pos = |i: usize| -> Option<[f32; 3]> {
                let p = nodes.get(i)?["p"].as_array()?;
                Some([p[0].as_f64()? as f32, p[1].as_f64()? as f32, p[2].as_f64()? as f32])
            };
            let water = |i: usize| nodes.get(i).and_then(|n| n["flag_names"].as_array()).map_or(false, |a| a.iter().any(|x| x == "WaterNode"));
            let links: Vec<(usize, usize, bool)> = g["links"]
                .as_array()
                .map(|v| {
                    v.iter()
                        .filter_map(|l| {
                            let (f, to) = (l["from"].as_u64()? as usize, l["to"].as_u64()? as usize);
                            (f < to && !water(f) && pos(f).is_some() && pos(to).is_some()).then(|| (f, to, l["flags"].as_u64().unwrap_or(0) >> 5 & 1 == 1))
                        })
                        .collect()
                })
                .unwrap_or_default();
            for (f, to, track) in spread(&links, 20000) {
                let (pa, pb) = (pos(f).unwrap(), pos(to).unwrap());
                for t in [0.25f32, 0.5, 0.75] {
                    link_pts.push(([0, 1, 2].map(|k| pa[k] + (pb[k] - pa[k]) * t), track));
                }
            }
        }
    }
    // locate (parallel chunks, one cache per chunk)
    let locate = |pts: Vec<[f32; 3]>| -> Vec<Option<u16>> {
        let chunks: Vec<Vec<[f32; 3]>> = pts.chunks(2000).map(|c| c.to_vec()).collect();
        chunks
            .par_iter()
            .flat_map_iter(|c| {
                let mut loc = Locator::new(&a.path, &coll, &by_id);
                c.iter().map(|p| loc.under(*p)).collect::<Vec<_>>()
            })
            .collect()
    };
    let mut plants: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let gs = locate(grass_pts.iter().map(|x| x.0).collect());
    for ((_, n), s) in grass_pts.iter().zip(&gs) {
        if let Some(s) = s {
            *plants.entry(format!("{s:04x}")).or_default().entry(n.clone()).or_default() += 1;
        }
    }
    let mut propson: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let ps = locate(prop_pts.iter().map(|x| x.0).collect());
    for ((_, n), s) in prop_pts.iter().zip(&ps) {
        if let Some(s) = s {
            *propson.entry(format!("{s:04x}")).or_default().entry(n.clone()).or_default() += 1;
        }
    }
    let mut lk: HashMap<String, (usize, usize)> = HashMap::new();
    let ls = locate(link_pts.iter().map(|x| x.0).collect());
    for ((_, track), s) in link_pts.iter().zip(&ls) {
        if let Some(s) = s {
            let e = lk.entry(format!("{s:04x}")).or_default();
            e.0 += 1;
            e.1 += *track as usize;
        }
    }
    let tot_links: usize = lk.values().map(|x| x.0).sum();

    // --- round 1 rules
    let mut s_out: BTreeMap<String, Value> = BTreeMap::new();
    let mut order: Vec<&String> = st.keys().collect();
    order.sort_by(|x, y| st[*y].area.partial_cmp(&st[*x].area).unwrap());
    for k in order {
        let o = &st[k];
        let (up, vert, z, b4) = geo(o);
        let road_share = if o.road_tot > 3000.0 { Some(o.road_on / o.road_tot) } else { None };
        let cat = if k == "317b" {
            "generic / default surface (open ground, flat roofs, seabed)"
        } else if road_share.map_or(false, |r| r >= 0.45) && up >= 0.9 {
            "road surface (carriageway)"
        } else if road_share.map_or(false, |r| r >= 0.3) {
            "hard surface: road edges, kerbs and walls (concrete/tarmac family)"
        } else if z < -20.0 && b4 > 0.95 {
            "seabed / underwater ground"
        } else if vert >= 0.6 {
            "walls / vertical building surface"
        } else if b4 > 0.9 && z.abs() < 6.0 && up > 0.9 {
            "shore level ground (beach, shallows, quays)"
        } else {
            continue;
        };
        s_out.insert(
            k.clone(),
            json!({"category": cat, "round": 1, "evidence": {"area_share": r4(share(k)), "road_share": road_share.map(r2), "up": r2(up),
                "vertical": r2(vert), "mean_z": r1(z), "below_4m": r2(b4), "files_sampled": o.files}}),
        );
    }

    // --- round 2: plants (as surfcat2.py)
    let mut keys: Vec<&String> = plants.keys().collect();
    keys.sort();
    for k in keys {
        let (tot, sh) = shares(&plants[k], plant_fam_r2);
        if tot < 30 {
            continue;
        }
        let g = |f: &str| sh.get(f).copied().unwrap_or(0.0);
        let mean_z = st.get(k).map(|s| s.zsum / s.area).unwrap_or(0.0);
        let (veg, forest, sea, hard, rock, sand) = (g("grass") + g("shrub"), g("forest") + g("shrub"), g("seabed"), g("paved") + g("debris"), g("rock"), g("sand"));
        let cat = if sea >= 0.45 && mean_z < 0.0 {
            "seabed / underwater ground"
        } else if sand >= 0.4 {
            "sand / beach"
        } else if g("grass") >= 0.55 {
            "grass / vegetated soil"
        } else if forest >= 0.5 {
            "forest floor / undergrowth soil"
        } else if veg >= 0.5 {
            "grass / vegetated soil"
        } else if hard >= 0.5 {
            "paved or hard ground (litter, crack weeds)"
        } else if rock >= 0.35 {
            "rocky ground"
        } else {
            continue;
        };
        let ev = json!({"plants_sampled": tot, "plant_families": top(&sh, 4)});
        if let Some(x) = s_out.get_mut(k) {
            for (a, b) in ev.as_object().unwrap() {
                x["evidence"][a] = b.clone();
            }
            if x["category"].as_str().unwrap_or("").starts_with("generic") {
                x["category"] = json!(format!("generic / default surface (open ground, flat roofs, seabed); the most common plant cover on it is {}", cat.split(" /").next().unwrap()));
            }
            continue;
        }
        let o = st.get(k).cloned().unwrap_or_default();
        let (up, vert, z, _) = if o.area > 0.0 { geo(&o) } else { (0.0, 0.0, 0.0, 0.0) };
        s_out.insert(k.clone(), json!({"category": cat, "round": 2, "evidence": {"area_share": r4(share(k)), "up": r2(up), "vertical": r2(vert), "mean_z": r1(z),
            "plants_sampled": tot, "plant_families": top(&sh, 4)}}));
    }

    // confidence for rounds 1-2
    for v in s_out.values_mut() {
        let e = &v["evidence"];
        let pf_max = e["plant_families"].as_object().map(|m| m.values().filter_map(|x| x.as_f64()).fold(0.0, f64::max)).unwrap_or(0.0);
        let strong = e["road_share"].as_f64().unwrap_or(0.0) >= 0.5 || (pf_max >= 0.55 && e["plants_sampled"].as_u64().unwrap_or(0) >= 30);
        v["confidence"] = json!(if strong { "high" } else { "medium" });
    }

    // --- round 3
    for (k, o) in &st {
        if s_out.contains_key(k) {
            continue;
        }
        let sh = share(k);
        let (nl, ntr) = lk.get(k).copied().unwrap_or((0, 0));
        let ratio = if sh > 0.0 && tot_links > 0 { (nl as f64 / tot_links as f64) / sh } else { 0.0 };
        let track = if nl > 0 { ntr as f64 / nl as f64 } else { 0.0 };
        let empty = HashMap::new();
        let (pt, pf) = shares(propson.get(k).unwrap_or(&empty), prop_fam);
        let (gt, gf) = shares(plants.get(k).unwrap_or(&empty), plant_fam_r3);
        let p = |m: &BTreeMap<&str, f64>, f: &str| m.get(f).copied().unwrap_or(0.0);
        let (up, vert, z, b4) = geo(o);
        let gmax = gf.values().cloned().fold(0.0, f64::max);
        let (cat, conf): (String, &str) = if nl >= 20 && ratio >= 3.0 {
            if track >= 0.4 {
                ("dirt track / unpaved road (path links, mostly flagged unpaved)".into(), "medium")
            } else {
                ("road / drivable surface (path links run on it)".into(), if ratio >= 6.0 || p(&pf, "road-edge") >= 0.2 { "medium" } else { "low" })
            }
        } else if vert >= 0.5 && ratio < 0.3 {
            ("walls / vertical structure".into(), if vert >= 0.55 { "medium" } else { "low" })
        } else if z < -3.0 && b4 >= 0.85 && up >= 0.85 {
            let rock = p(&gf, "rock").max(p(&pf, "rock"));
            let c = if rock >= 0.3 { "rocky seabed / coastal shallows" } else { "seabed / underwater ground" };
            (c.into(), if p(&gf, "seabed") + p(&gf, "rock") >= 0.5 && gt >= 30 { "medium" } else { "low" })
        } else if gt >= 10 && p(&gf, "wet") >= 0.3 && z < 8.0 {
            ("wetland / swamp ground".into(), "low")
        } else if gt >= 15 && gmax >= 0.35 {
            let mut v: Vec<_> = gf.iter().collect();
            v.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap().then(a.0.cmp(b.0)));
            let c = match *v[0].0 {
                "grass" | "shrub" => "grass / vegetated soil",
                "forest" | "debris" => "forest floor / leaf litter",
                "rock" => "rocky ground",
                "paved" => "paved or hard ground",
                "seabed" => "seabed / underwater ground",
                "sand" => "sand / beach",
                "wet" => "wetland / swamp ground",
                _ => continue,
            };
            (c.into(), "low")
        } else if pt >= 15 && p(&pf, "rock") >= 0.5 {
            ("rocky ground".into(), "low")
        } else if pt >= 15 && p(&pf, "tree") + p(&pf, "veg") >= 0.6 {
            ("vegetated ground (trees and plants stand on it)".into(), "low")
        } else if (-2.0..=3.0).contains(&z) && b4 >= 0.8 && up >= 0.9 {
            ("shoreline / water-edge ground".into(), "low")
        } else if (0.25..0.5).contains(&vert) {
            ("mixed structure (steps, ledges, walls)".into(), "low")
        } else if up >= 0.95 && z > 2.0 && gt + pt + nl < 10 {
            ("flat hard surface (floors, roofs, platforms)".into(), "low")
        } else {
            continue;
        };
        s_out.insert(
            k.clone(),
            json!({"category": cat, "confidence": conf, "round": 3, "evidence": {"area_share": r4(sh), "up": r2(up), "vertical": r2(vert), "mean_z": r1(z),
                "below_4m": r2(b4), "path_link_ratio": r2(ratio), "path_links_sampled": nl, "track_share": r2(track), "props_sampled": pt,
                "prop_families": top(&pf, 4), "plants_sampled": gt, "plant_families": top(&gf, 4)}}),
        );
    }

    // --- round 4: gravel / railway ids, found offline by
    // sampling the surface under named railway map entities (rail sleepers,
    // signal boxes, crossing signs, rail-track pieces) from the full
    // placements. Fixed here because the cue needs the per-entity names.
    for (k, cat, conf, ev) in RAIL_IDS {
        if let Some(x) = s_out.get_mut(*k) {
            let prev = x["category"].clone();
            x["category"] = json!(cat);
            x["confidence"] = json!(conf);
            x["round"] = json!(4);
            x["evidence"]["railway_props"] = json!(ev);
            x["evidence"]["earlier_category"] = prev;
        }
    }

    let cov: f64 = s_out.keys().map(|k| share(k)).sum();
    let mut by: BTreeMap<String, f64> = BTreeMap::new();
    for (k, v) in &s_out {
        *by.entry(v["confidence"].as_str().unwrap_or("").to_string()).or_default() += share(k);
    }
    let samples = json!({"collision_files": sample.len(), "plants": grass_pts.len(), "plants_located": gs.iter().filter(|x| x.is_some()).count(),
        "props": prop_pts.len(), "props_located": ps.iter().filter(|x| x.is_some()).count(),
        "link_points": link_pts.len(), "link_points_located": ls.iter().filter(|x| x.is_some()).count(), "road_tiles": road.tiles.len()});
    let doc = json!({
        "note": "INFERRED categories: the surface id is 18 + elf(material name) % 32749; `sixng annotate` adds the material name where it is known. Round 1: geometry statistics over every 20th world-space collision file and overlap with navmesh road polygons. Round 2: plants (grass batches) standing on each id. Round 3: path-link density on each id, props standing on it (reflectors, bollards, barriers, rocks, trees), lower plant thresholds, slope and height relative to water. Round 4: gravel/ballast ids from the surface under named railway entities (no railway-only id was found). Every id has a confidence (high / medium / low); low = a plausible guess from weak cues. Written by `sixng surfaces`; sampling is deterministic.",
        "area_covered": r4(cov),
        "area_by_confidence": by.iter().map(|(k, v)| (k.clone(), json!(r4(*v)))).collect::<serde_json::Map<_, _>>(),
        "samples": samples,
        "surfaces": s_out,
    });
    let dir = out.join("collision").join(&short);
    crate::atomic::write(dir.join("surface_categories.json"), serde_json::to_string_pretty(&doc)?)?;
    Ok(SurfaceSummary { ids: s_out.len(), area_covered: cov, by_confidence: by, samples })
}
