#![allow(dead_code)]
//! 3D map: a streamed fly-through of the placed models.
//!
//! Entities come from the baked map (`work/map/entities.bin`, see mapbake),
//! bucketed by LOD level, LOD distance and size so a visibility update only
//! visits entities that can be in range. The camera picks entities by LOD
//! band (HD near, then LOD and SLOD1-4 further out), drops props/vegetation
//! by the density setting, anything smaller than the screen-size cut (raised
//! automatically to stay in the triangle and draw-call budget) and anything
//! outside the (widened) view frustum, and draws one GPU model per placed
//! model id with all its instances; the opaque draws are recorded once into
//! a render bundle and replayed while the set stays the same.
//!
//! Models are exported on demand into `work/gui_cache` (same cache as the
//! model viewer) by worker threads, on-screen, big and near first, and
//! evicted least-recently-used. Textures stream: a model first loads with
//! 64 px textures (from reduced copies in `<cache>/lowtex`), then reloads
//! sharper as the camera comes closer. The whole map stays visible through
//! the far field (`world3d`): a quadtree of the baked heights with the 2D
//! map tiles as colour. Overlays: roads and lights as lines, and draggable
//! minimap / stats / inspect windows.
use crate::glb;
use crate::map2d::{self, MapData};
use crate::mapbake::{rflag_names, EntRec, KIND_NAMES, RF_BBOX_UNKNOWN, RF_HIDE};
use crate::mem;
use crate::model_view;
use crate::render3d::{self, Camera, GpuModel, Instance, LineV, Look, Renderer3D};
use crate::world3d::{self, World};
use eframe::egui::{self, Color32, Pos2, Stroke};
use eframe::egui_wgpu::{self, wgpu};
use glam::{Mat4, Vec3};
use sixng_core::export::{ExportOptions, Workspace};
use sixng_core::scan::ArchiveIndex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Instant;

pub const LOD_NAMES: [&str; 6] = ["HD", "LOD", "SLOD1", "SLOD2", "SLOD3", "SLOD4"];

/// RAGE (Z-up) -> glTF/renderer (Y-up).
pub fn gl(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[2], -p[1])
}
/// Renderer (Y-up) -> RAGE (Z-up).
pub fn rage(v: Vec3) -> [f32; 3] {
    [v.x, -v.z, v.y]
}

enum Slot {
    /// `tex`: texture size cap it was loaded with (u32::MAX = untextured).
    Ready { gm: GpuModel, used: u64, tris: usize, tex: u32 },
    Failed(String),
}

struct Req {
    model: u32,
    id: String,
    prio: f32,
}

struct Shared {
    queue: Mutex<Vec<Req>>,
    cv: Condvar,
    inflight: Mutex<HashSet<u32>>,
    stop: AtomicBool,
    /// Results sent by workers but not yet uploaded.
    unread: AtomicUsize,
    /// Texture budget used up or RAM over the soft cap: workers load 64 px.
    tex_over: AtomicBool,
    /// RAM over the hard cap: workers skip loads entirely.
    hard: AtomicBool,
}

/// (model, texture size cap used, result)
type LoadResult = (u32, u32, Result<glb::Model, String>);

/// Detail settings (the "detail" slider sets them all; each can be tuned).
#[derive(Clone, PartialEq)]
pub struct Detail {
    /// Upper distance of each LOD band: HD, LOD, SLOD1..4 (m).
    pub radii: [f32; 6],
    /// Fraction of props / vegetation kept (0..1).
    pub prop_density: f32,
    pub veg_density: f32,
    /// Skip entities whose bbox radius / distance is below this (screen-size cull).
    pub min_size: f32,
    /// Drawable LOD file used for every model (0 = model.glb).
    pub drawable_lod: usize,
    pub max_tex: u32,
    pub textures: bool,
    /// Most LOD-level placements have no linked model yet (unresolved
    /// archetype hashes), so keep large HD models out to the LOD band and
    /// skip the LOD level instead.
    pub hd_fallback: bool,
}

impl Detail {
    pub fn from_level(d: f32) -> Self {
        let d = d.clamp(0.0, 1.0);
        let hd = 150.0 + 450.0 * d;
        Detail {
            radii: [hd, hd * 3.5, hd * 8.0, hd * 14.0, hd * 20.0, hd * 28.0],
            prop_density: 0.25 + 0.75 * d,
            veg_density: 0.15 + 0.85 * d,
            min_size: 0.012 - 0.009 * d,
            drawable_lod: 0,
            max_tex: if d > 0.7 { 512 } else { 256 },
            textures: true,
            hd_fallback: true,
        }
    }
}

pub struct Auto {
    pub screenshot: Option<PathBuf>,
    pub exit: bool,
    pub frames: u32,
    pub quiet_frames: u32,
    pub ui_shot: Option<PathBuf>,
    pub started: Option<Instant>,
    /// Automation: viewport points (fractions 0..1) to pick and print when the shot is taken.
    pub picks: Vec<[f32; 2]>,
    /// Automation: once the first view has loaded, fly to (x, y, height over
    /// ground) RAGE over `secs` seconds, then wait for loads and take the shot.
    pub fly: Option<([f32; 3], f32)>,
    fly_from: Option<(Vec3, Instant)>,
    fly_done: bool,
    fly_ms: Vec<f32>,
}

/// Timings for the perf log (open time, frame times).
#[derive(Default)]
struct Perf {
    t_open: Option<Instant>,
    data_s: Option<f32>,
    first_idle_s: Option<f32>,
    /// Every in-range model loaded at least with small textures.
    coarse_s: Option<f32>,
    last_frame: Option<Instant>,
    frame_ms: std::collections::VecDeque<f32>,
}

/// Mean, 99th percentile and worst frame time (ms) of a sample.
fn frame_summary(v: &[f32]) -> (f32, f32, f32) {
    if v.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = s.iter().sum::<f32>() / s.len() as f32;
    (mean, s[((s.len() as f32 * 0.99) as usize).min(s.len() - 1)], *s.last().unwrap())
}

pub struct Map3D {
    pub game: String,
    pub work: String,
    pub data: Option<Arc<MapData>>,
    pub cam: Camera,
    pub look: Look,
    pub tod: f32,
    pub detail_level: f32,
    pub detail: Detail,
    pub show_terrain: bool,
    pub show_roads: bool,
    pub show_lights: bool,
    pub show_minimap: bool,
    pub show_stats: bool,
    pub decal_bias: bool,
    /// Debug: also draw proxies, placeholders and degenerate models.
    pub show_proxies: bool,
    pub kinds: [bool; 6],
    pub lods: [bool; 6],
    pub speed: f32,
    pub auto: Auto,
    pub open_model: Option<String>,
    pub status: String,
    renderer: Option<Renderer3D>,
    cache: HashMap<u32, Slot>,
    tex_cache: HashMap<String, wgpu::Texture>,
    /// Estimated GPU bytes of the cached textures.
    tex_bytes: usize,
    children_start: Vec<u32>,
    children_list: Vec<u32>,
    buckets: Vec<Bucket>,
    /// Texture key -> (models using it, estimated bytes).
    tex_refs: HashMap<String, (u32, usize)>,
    /// Model -> texture keys it uses.
    model_texs: HashMap<u32, Vec<String>>,
    /// Nearest distance of each wanted model (m).
    near_dist: HashMap<u32, f32>,
    pub budget: mem::Budget,
    /// User RAM limit in GB (None = automatic, from the installed RAM).
    pub ram_cap_gb: Option<f32>,
    mem: mem::MemInfo,
    /// Process RAM when the map opened, and the map's own memory estimate.
    base_ram: Option<u64>,
    mem_est: u64,
    pressure: u8,
    mem_checked: Option<Instant>,
    tex_keys: Arc<Mutex<HashSet<String>>>,
    shared: Arc<Shared>,
    rx: Option<mpsc::Receiver<LoadResult>>,
    workers_started: bool,
    want: HashMap<u32, Vec<Instance>>,
    visible: Vec<(u32, u32)>, // (entity, model)
    last_eye: Vec3,
    last_detail: Option<Detail>,
    last_kinds: [bool; 6],
    last_lods: [bool; 6],
    last_proxies: bool,
    frame: u64,
    dirty: bool,
    stats: Stats,
    picked: Option<u32>,
    lines_cache: Option<(Vec3, f32, bool, bool, Vec<LineV>, Vec<LineV>)>,
    size: [u32; 2],
    load_opts_lod: usize,
    perf: Perf,
    world: World,
    /// Screen-space error target of the far field (px).
    pub world_tau: f32,
    /// Visible building placements per 128 m cell: (wanted, loaded).
    bld_ready: HashMap<(i32, i32), (u32, u32)>,
    /// Models in range (on screen or not): kept loaded, not evicted first.
    keep: HashSet<u32>,
    last_fwd: Vec3,
    /// Triangle budget; over it the screen-size cut is raised (`size_boost`).
    pub tri_budget: usize,
    pub draw_budget: usize,
    size_boost: f32,
    /// Quality preset index (see QUALITY).
    pub quality: usize,
}

/// Quality presets: (name, detail, triangle budget (M), terrain error px, max texture px, draw-call budget).
pub const QUALITY: [(&str, f32, f32, f32, u32, usize); 4] = [("low", 0.15, 6.0, 5.0, 128, 2500), ("medium", 0.4, 12.0, 3.0, 256, 5000), ("high", 0.6, 20.0, 2.0, 512, 8000), ("ultra", 0.9, 40.0, 1.5, 1024, 16000)];

#[derive(Default, Clone)]
struct Stats {
    entities: usize,
    models_wanted: usize,
    models_ready: usize,
    queued: usize,
    inflight: usize,
    failed: usize,
    instances: usize,
    tris: usize,
    per_band: [usize; 6],
    textures: usize,
    upload_ms: f32,
    handoffs: usize,
    hidden: usize,
    tex_gb: f32,
    update_ms: f32,
    /// Entities tested in the last visibility update.
    visited: usize,
    draws: u32,
    render_ms: f32,
    /// Models queued to reload with sharper textures.
    upgrades: usize,
    /// ms: loads + visibility, terrain + overlays.
    phase: [f32; 4],
}

impl Map3D {
    pub fn new(game: &str, work: &str) -> Self {
        let mut look = Look::default();
        look.set_time_of_day(14.0);
        look.fog = 0.00022;
        let mut cam = Camera::default();
        cam.fly = true;
        cam.near = 0.5;
        cam.far = 12000.0;
        cam.fov = 55f32.to_radians();
        cam.eye = gl([-600.0, -60.0, 120.0]);
        cam.yaw = 0.0;
        cam.pitch = 0.35;
        let detail_level = 0.5;
        let mut m = Map3D {
            game: game.into(),
            work: work.into(),
            data: None,
            cam,
            look,
            tod: 14.0,
            detail_level,
            detail: Detail::from_level(detail_level),
            show_terrain: true,
            show_roads: false,
            show_lights: false,
            show_minimap: true,
            show_stats: true,
            decal_bias: true,
            show_proxies: false,
            children_start: Vec::new(),
            children_list: Vec::new(),
            buckets: Vec::new(),
            kinds: [true; 6],
            lods: [true; 6],
            speed: 1.0,
            auto: Auto { screenshot: None, exit: false, frames: 0, quiet_frames: 0, ui_shot: None, started: None, picks: Vec::new(), fly: None, fly_from: None, fly_done: false, fly_ms: Vec::new() },
            open_model: None,
            status: String::new(),
            renderer: None,
            cache: HashMap::new(),
            tex_cache: HashMap::new(),
            tex_bytes: 0,
            tex_refs: HashMap::new(),
            model_texs: HashMap::new(),
            near_dist: HashMap::new(),
            budget: mem::Budget::for_total(mem::query().total),
            ram_cap_gb: None,
            mem: mem::query(),
            base_ram: None,
            mem_est: 0,
            pressure: 0,
            mem_checked: None,
            tex_keys: Arc::new(Mutex::new(HashSet::new())),
            shared: Arc::new(Shared { queue: Mutex::new(Vec::new()), cv: Condvar::new(), inflight: Mutex::new(HashSet::new()), stop: AtomicBool::new(false), unread: AtomicUsize::new(0), tex_over: AtomicBool::new(false), hard: AtomicBool::new(false) }),
            rx: None,
            workers_started: false,
            want: HashMap::new(),
            visible: Vec::new(),
            last_eye: Vec3::splat(f32::MAX),
            last_detail: None,
            last_kinds: [true; 6],
            last_lods: [true; 6],
            last_proxies: false,
            frame: 0,
            dirty: true,
            stats: Stats::default(),
            picked: None,
            lines_cache: None,
            size: [16, 16],
            load_opts_lod: 0,
            perf: Perf::default(),
            world: World::new(Path::new(work).join("map")),
            world_tau: 2.5,
            bld_ready: HashMap::new(),
            keep: HashSet::new(),
            last_fwd: Vec3::Z,
            tri_budget: 20_000_000,
            draw_budget: 8000,
            size_boost: 1.0,
            quality: 2,
        };
        m.set_quality(2);
        m
    }

    pub fn cache_dir(&self) -> PathBuf {
        Path::new(&self.work).join(crate::GUI_CACHE)
    }

    /// Put the camera above (x, y) RAGE, `h` metres over the ground.
    /// Forget the map data (after a re-bake); loaded models stay cached.
    pub fn reset_data(&mut self) {
        self.data = None;
        self.children_start.clear();
        self.children_list.clear();
        self.buckets.clear();
        self.world = World::new(Path::new(&self.work).join("map"));
        self.last_eye = Vec3::splat(f32::MAX);
    }

    pub fn place(&mut self, x: f32, y: f32, h: f32) {
        let g = self.data.as_ref().and_then(|d| d.height(x, y)).unwrap_or(0.0);
        self.cam.eye = gl([x, y, g + h]);
        self.dirty = true;
    }

    fn start_workers(&mut self) {
        if self.workers_started {
            return;
        }
        self.workers_started = true;
        let (tx, rx) = mpsc::channel::<LoadResult>();
        self.rx = Some(rx);
        {
            // size of the reduced-texture cache, so it stays under its cap
            let dir = self.cache_dir().join("lowtex");
            glb::LOWTEX_BYTES.store(u64::MAX / 2, Ordering::Relaxed);
            std::thread::spawn(move || {
                let n: u64 = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()?.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0);
                glb::LOWTEX_BYTES.store(n, Ordering::Relaxed);
            });
        }
        let idx: Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>> = Arc::new(Mutex::new(HashMap::new()));
        let offs: OffMaps = Arc::new(Mutex::new(HashMap::new()));
        let arcs: ArcCache = Arc::new(Mutex::new(HashMap::new()));
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 6);
        for _ in 0..n {
            let sh = self.shared.clone();
            let tx = tx.clone();
            let idx = idx.clone();
            let game = self.game.clone();
            let work = self.work.clone();
            let keys = self.tex_keys.clone();
            let offs = offs.clone();
            let arcs = arcs.clone();
            std::thread::spawn(move || {
                let ws = Workspace::new(&game, &work);
                loop {
                    // backpressure: decoded models wait in the channel until the
                    // UI uploads them; keep that backlog small.
                    while sh.unread.load(Ordering::SeqCst) > 24 && !sh.stop.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    let req = {
                        let mut q = sh.queue.lock().unwrap();
                        loop {
                            if sh.stop.load(Ordering::Relaxed) {
                                return;
                            }
                            if !q.is_empty() {
                                // nearest first
                                let (bi, _) = q.iter().enumerate().fold((0, f32::MAX), |a, (i, r)| if r.prio < a.1 { (i, r.prio) } else { a });
                                let r = q.swap_remove(bi);
                                sh.inflight.lock().unwrap().insert(r.model);
                                break r;
                            }
                            q = sh.cv.wait(q).unwrap();
                        }
                    };
                    let (lod, max_tex, textures) = {
                        // encoded in prio's companion fields would be cleaner; read from the id suffix
                        let mut it = req.id.split('|');
                        let _ = it.next();
                        let lod = it.next().and_then(|s| s.parse().ok()).unwrap_or(0usize);
                        let mt = it.next().and_then(|s| s.parse().ok()).unwrap_or(256u32);
                        let tx_on = it.next().map(|s| s == "1").unwrap_or(true);
                        (lod, mt, tx_on)
                    };
                    let id = req.id.split('|').next().unwrap_or("").to_string();
                    if sh.hard.load(Ordering::Relaxed) {
                        sh.inflight.lock().unwrap().remove(&req.model);
                        continue;
                    }
                    let max_tex = if sh.tex_over.load(Ordering::Relaxed) { max_tex.min(64) } else { max_tex };
                    let opts = glb::LoadOpts { max_tex, skip_keys: Some(keys.clone()), no_textures: !textures, hide_placeholders: true, small_cache: true };
                    let res = load_model(&ws, &Path::new(&work).join(crate::GUI_CACHE), &id, lod, &opts, &idx, &offs, &arcs).map(merge_meshes);
                    sh.unread.fetch_add(1, Ordering::SeqCst);
                    sh.inflight.lock().unwrap().remove(&req.model);
                    let used = if textures { max_tex } else { u32::MAX };
                    if tx.send((req.model, used, res.map_err(|e| format!("{e:#}")))).is_err() {
                        return;
                    }
                }
            });
        }
    }

    /// Parent -> children lists (CSR) from the baked LOD parent links.
    fn build_children(&mut self, data: &MapData) {
        let n = data.ents.len();
        let mut count = vec![0u32; n + 1];
        for e in data.ents.iter() {
            if (e.parent as usize) < n {
                count[e.parent as usize + 1] += 1;
            }
        }
        for i in 0..n {
            count[i + 1] += count[i];
        }
        let mut fill = count.clone();
        let mut list = vec![0u32; count[n] as usize];
        for (i, e) in data.ents.iter().enumerate() {
            if (e.parent as usize) < n {
                let p = e.parent as usize;
                list[fill[p] as usize] = i as u32;
                fill[p] += 1;
            }
        }
        // buckets by level, LOD distance and size
        let t0 = Instant::now();
        let mut groups: HashMap<(usize, usize, usize), Vec<(f32, f32, u32)>> = HashMap::new();
        for (i, e) in data.ents.iter().enumerate() {
            if data.name(e.model).is_empty() {
                continue;
            }
            let linked = (e.parent as usize) < n || count[i] < count[i + 1];
            let level = e.lod.min(5) as usize;
            let rad = 0.5 * ((e.hi[0] - e.lo[0]).powi(2) + (e.hi[1] - e.lo[1]).powi(2) + (e.hi[2] - e.lo[2]).powi(2)).sqrt();
            let size = if e.rflags & RF_BBOX_UNKNOWN != 0 && e.lod > 0 { SIZE_CLASSES.len() - 1 } else { SIZE_CLASSES.iter().position(|&u| rad <= u).unwrap_or(SIZE_CLASSES.len() - 1) };
            let dist = if linked {
                usize::MAX
            } else if e.lod_dist > 0.0 {
                DIST_CLASSES.iter().position(|&u| e.lod_dist <= u).unwrap_or(DIST_CLASSES.len() - 1)
            } else {
                DIST_CLASSES.len()
            };
            groups.entry((level, dist, size)).or_default().push((e.pos[0], e.pos[1], i as u32));
        }
        let mut buckets: Vec<Bucket> = groups
            .into_iter()
            .map(|((level, dist, size), pts)| {
                let span = if dist < DIST_CLASSES.len() { DIST_CLASSES[dist] } else { 1000.0 };
                let cell = (span.min(SIZE_CLASSES[size] / 0.003) * 0.5).clamp(64.0, 1024.0);
                Bucket { level, dist, size, count: pts.len(), grid: CsrGrid::build(cell, &pts) }
            })
            .collect();
        buckets.sort_by_key(|b| (b.level, b.dist, b.size));
        eprintln!("[map3d] {} entity buckets in {:.0} ms", buckets.len(), t0.elapsed().as_secs_f32() * 1000.0);
        self.buckets = buckets;
        self.children_start = count;
        self.children_list = list;
    }

    /// Recompute the visible entity set and the per-model instance lists.
    fn update_visible(&mut self, data: &MapData) {
        let t0 = Instant::now();
        if self.children_start.is_empty() {
            self.build_children(data);
        }
        let eye = rage(self.cam.eye());
        let d = &self.detail;
        let rmax = d.radii[5];
        let mut want: HashMap<u32, Vec<Instance>> = HashMap::new();
        let mut prio: HashMap<u32, f32> = HashMap::new();
        let mut near: HashMap<u32, f32> = HashMap::new();
        let mut visible = Vec::new();
        let mut per_band = [0usize; 6];
        let mut n_ent = 0usize;
        // Game LOD distances scaled by the detail setting (1.0 at detail ~0.4).
        let k = d.radii[0] / 330.0;
        let show_proxies = self.show_proxies;
        let kinds = self.kinds;
        let lods = self.lods;
        let centre_dist = |e: &EntRec| -> f32 {
            let (s, c) = e.yaw.sin_cos();
            let lc = [(e.lo[0] + e.hi[0]) * 0.5, (e.lo[1] + e.hi[1]) * 0.5, (e.lo[2] + e.hi[2]) * 0.5];
            let wc = [e.pos[0] + lc[0] * c - lc[1] * s, e.pos[1] + lc[0] * s + lc[1] * c, e.pos[2] + lc[2]];
            ((wc[0] - eye[0]).powi(2) + (wc[1] - eye[1]).powi(2) + (wc[2] - eye[2]).powi(2)).sqrt()
        };
        let drawable = |e: &EntRec| -> bool {
            !data.name(e.model).is_empty() && (show_proxies || e.rflags & RF_HIDE == 0) && kinds[e.kind.min(5) as usize] && lods[e.lod.min(5) as usize]
        };
        let rad_of = |e: &EntRec| -> f32 {
            if e.rflags & RF_BBOX_UNKNOWN != 0 && e.lod > 0 {
                1.0e4 // size unknown: never size-cull
            } else {
                0.5 * ((e.hi[0] - e.lo[0]).powi(2) + (e.hi[1] - e.lo[1]).powi(2) + (e.hi[2] - e.lo[2]).powi(2)).sqrt()
            }
        };
        // band limits for placements without usable LOD links
        let band = |e: &EntRec, rad: f32| -> (f32, f32) {
            let l = e.lod.min(5) as usize;
            let lo_r = if l == 0 { 0.0 } else { d.radii[l - 1] * 0.92 };
            let hi_r = if l == 0 && d.hd_fallback && rad >= 10.0 { d.radii[1].min(d.radii[0] * 1.75) } else { d.radii[l] };
            (lo_r, hi_r)
        };
        let outer = |e: &EntRec, rad: f32| -> f32 { if e.lod_dist > 0.0 { (e.lod_dist * k).min(rmax) } else { band(e, rad).1 } };
        // children of parent p are active (they take over) when the camera
        // is within p's child_lod_dist
        let children_active = |p: &EntRec| -> bool { p.child_lod_dist > 0.0 && centre_dist(p) < p.child_lod_dist * k };
        let mut handoffs = 0usize;
        let mut hidden = 0usize;
        let cache = &self.cache;
        let (cs, cl) = (&self.children_start, &self.children_list);
        // frustum (widened so turning a little shows loaded models at once)
        let aspect = self.size[0] as f32 / self.size[1].max(1) as f32;
        let vp = Mat4::perspective_rh((self.cam.fov * 1.3).min(3.0), aspect.max(0.1), 0.5, 1.0e5) * self.cam.view();
        let planes: Vec<glam::Vec4> = [vp.row(3) + vp.row(0), vp.row(3) - vp.row(0), vp.row(3) + vp.row(1), vp.row(3) - vp.row(1)].iter().map(|p| *p / p.truncate().length().max(1e-6)).collect();
        let cam_eye = self.cam.eye();
        let cam_fwd = self.cam.forward();
        let in_view = |c: Vec3, r: f32| -> bool { (c - cam_eye).dot(cam_fwd) > -r && planes.iter().all(|p| p.truncate().dot(c) + p.w >= -r) };
        let min_size = d.min_size * self.size_boost;
        let mut keep: HashSet<u32> = HashSet::new();
        let mut visited = 0usize;
        let mut visit = |i: u32| {
            visited += 1;
            let e: &EntRec = &data.ents[i as usize];
            let kind = e.kind.min(5) as usize;
            if !drawable(e) {
                if e.rflags & RF_HIDE != 0 && !data.name(e.model).is_empty() {
                    hidden += 1;
                }
                return;
            }
            let l = e.lod.min(5) as usize;
            let dist = centre_dist(e);
            let rad = rad_of(e);
            let has_parent = e.parent != u32::MAX;
            let has_children = cs[i as usize] < cs[i as usize + 1];
            if has_parent || has_children {
                // linked placements: game LOD hand-off
                let mut lim = outer(e, rad);
                if has_parent {
                    let p = &data.ents[e.parent as usize];
                    if drawable(p) {
                        if !children_active(p) {
                            return; // the parent is drawn instead
                        }
                    } else if d.hd_fallback {
                        // parent has no model: stay out to the parent's distance
                        lim = lim.max(outer(p, rad_of(p)));
                    }
                }
                if dist >= lim {
                    return;
                }
                if has_children && children_active(e) {
                    // hand over only when every drawable child is loaded
                    let ready = cl[cs[i as usize] as usize..cs[i as usize + 1] as usize].iter().all(|&c| {
                        let ce = &data.ents[c as usize];
                        !drawable(ce) || cache.contains_key(&ce.model)
                    });
                    if ready {
                        handoffs += 1;
                        return;
                    }
                }
            } else {
                let (lo_r, hi_r) = band(e, rad);
                if d.hd_fallback && l == 1 {
                    return;
                }
                if dist >= hi_r || dist < lo_r {
                    return;
                }
            }
            if dist > 1.0 && rad / dist < min_size {
                return;
            }
            let dens = match kind {
                2 => d.prop_density,
                3 => d.veg_density,
                _ => 1.0,
            };
            if dens < 1.0 && (hash32(i) as f32 / u32::MAX as f32) > dens {
                return;
            }
            let (s, c) = e.yaw.sin_cos();
            let lc = [(e.lo[0] + e.hi[0]) * 0.5, (e.lo[1] + e.hi[1]) * 0.5, (e.lo[2] + e.hi[2]) * 0.5];
            let wc = gl([e.pos[0] + lc[0] * c - lc[1] * s, e.pos[1] + lc[0] * s + lc[1] * c, e.pos[2] + lc[2]]);
            let seen = in_view(wc, rad.min(2000.0));
            keep.insert(e.model);
            let nd = near.entry(e.model).or_insert(f32::MAX);
            *nd = nd.min(dist);
            // load order: big and near first, on screen before off screen
            let p = prio.entry(e.model).or_insert(f32::MAX);
            *p = p.min(dist / rad.clamp(1.0, 400.0).sqrt() * if seen { 1.0 } else { 3.0 });
            if !seen {
                return;
            }
            n_ent += 1;
            per_band[l] += 1;
            let m = Mat4::from_translation(gl(e.pos)) * Mat4::from_rotation_y(e.yaw);
            want.entry(e.model).or_default().push(Instance::new(m, false));
            visible.push((i, e.model));
        };
        for b in &self.buckets {
            if !lods[b.level] {
                continue;
            }
            let r = b.radius(d);
            b.grid.query(eye[0] - r, eye[1] - r, eye[0] + r, eye[1] + r, &mut visit);
        }
        self.stats.visited = visited;
        self.stats.handoffs = handoffs;
        self.stats.hidden = hidden;
        // requests: wanted models that are not cached or in flight, nearest first
        let inflight = self.shared.inflight.lock().unwrap().clone();
        // Texture size by distance (full size within the HD band, then 1/2,
        // 1/4, 1/8) and a hard 64 px once the texture budget is used up.
        let over = self.tex_bytes as u64 > self.budget.textures || self.pressure > 0;
        let tex_for = |dist: f32| -> u32 {
            if over {
                return 64;
            }
            let r0 = d.radii[0].max(50.0);
            let div = if dist < r0 { 1 } else if dist < 2.5 * r0 { 2 } else if dist < 6.0 * r0 { 4 } else { 8 };
            (d.max_tex / div).max(64)
        };
        let tx_on = if d.textures { 1 } else { 0 };
        // Texture streaming: a model first loads with small textures (fast,
        // from the reduced copies), then reloads sharper when the camera is
        // close enough to need twice the size.
        let mut upgrades: Vec<Req> = Vec::new();
        if d.textures && !over {
            for (&m, &p) in &prio {
                if let Some(Slot::Ready { tex, .. }) = self.cache.get(&m) {
                    let want_t = tex_for(near.get(&m).copied().unwrap_or(0.0));
                    if *tex != u32::MAX && want_t >= tex.saturating_mul(2) && !inflight.contains(&m) && self.pressure == 0 {
                        upgrades.push(Req { model: m, id: format!("{}|{}|{}|{}", data.name(m), d.drawable_lod, want_t, tx_on), prio: p * 3.0 + 1.0 });
                    }
                }
            }
        }
        self.stats.upgrades = upgrades.len();
        let mut reqs: Vec<Req> = prio
            .iter()
            .filter(|(m, _)| !self.cache.contains_key(m) && !inflight.contains(m))
            // Under memory pressure only the nearest models still load, and
            // over the hard cap nothing does.
            .filter(|(m, _)| match self.pressure {
                0 => true,
                1 => near.get(m).map_or(false, |&dd| dd < d.radii[0]),
                _ => false,
            })
            .map(|(&m, &p)| Req { model: m, id: format!("{}|{}|{}|{}", data.name(m), d.drawable_lod, tex_for(near.get(&m).copied().unwrap_or(0.0)).min(FIRST_TEX), tx_on), prio: p })
            .collect();
        reqs.extend(upgrades);
        reqs.sort_by(|a, b| a.prio.partial_cmp(&b.prio).unwrap_or(std::cmp::Ordering::Equal));
        reqs.truncate(4000);
        let nq = reqs.len();
        *self.shared.queue.lock().unwrap() = reqs;
        self.shared.cv.notify_all();
        self.stats.entities = n_ent;
        self.stats.models_wanted = want.len();
        self.stats.queued = nq;
        self.stats.per_band = per_band;
        self.want = want;
        self.keep = keep;
        self.near_dist = near;
        self.visible = visible;
        self.stats.update_ms = t0.elapsed().as_secs_f32() * 1000.0;
    }

    fn apply_instances(&mut self, rs: &egui_wgpu::RenderState) {
        let ren = self.renderer.as_ref().unwrap();
        let mut inst_total = 0;
        let mut tris = 0;
        for (m, slot) in self.cache.iter_mut() {
            if let Slot::Ready { gm, used, tris: t, .. } = slot {
                match self.want.get(m) {
                    Some(v) => {
                        ren.set_instances(rs, gm, v);
                        *used = self.frame;
                        inst_total += v.len();
                        tris += *t * v.len();
                    }
                    None => gm.n_instances = 0,
                }
            }
        }
        self.stats.instances = inst_total;
        self.stats.tris = tris;
    }

    /// Over the hard cap: discard finished loads instead of uploading them.
    fn drain_loads(&mut self) -> bool {
        let Some(rx) = &self.rx else { return false };
        let mut any = false;
        while rx.try_recv().is_ok() {
            self.shared.unread.fetch_sub(1, Ordering::SeqCst);
            any = true;
        }
        any
    }

    /// Upload finished loads within a time budget.
    fn poll_loads(&mut self, rs: &egui_wgpu::RenderState) -> bool {
        let Some(rx) = &self.rx else { return false };
        let t0 = Instant::now();
        let mut any = false;
        let ren = self.renderer.as_ref().unwrap();
        while t0.elapsed().as_millis() < 12 {
            let Ok((m, tex, res)) = rx.try_recv() else { break };
            self.shared.unread.fetch_sub(1, Ordering::SeqCst);
            any = true;
            match res {
                Ok(model) => {
                    let mut gm = ren.upload_shared(rs, &model, &[], Some(&mut self.tex_cache));
                    {
                        let mut k = self.tex_keys.lock().unwrap();
                        let mut keys = Vec::new();
                        for i in &model.images {
                            let bytes: usize = i.mips.iter().map(|m| m.2.len()).sum();
                            let r = self.tex_refs.entry(i.key.clone()).or_insert((0, 0));
                            r.0 += 1;
                            if bytes > 0 && r.1 == 0 {
                                r.1 = bytes;
                                self.tex_bytes += bytes;
                                k.insert(i.key.clone());
                            }
                            keys.push(i.key.clone());
                        }
                        if let Some(old) = self.model_texs.insert(m, keys) {
                            Self::release_keys(&mut self.tex_refs, &mut self.tex_cache, &mut k, &mut self.tex_bytes, old);
                        }
                    }
                    if let Some(v) = self.want.get(&m) {
                        ren.set_instances(rs, &mut gm, v);
                    } else {
                        gm.n_instances = 0;
                    }
                    self.cache.insert(m, Slot::Ready { gm, used: self.frame, tris: model.triangles, tex });
                }
                Err(e) => {
                    self.cache.insert(m, Slot::Failed(e));
                }
            }
        }
        self.stats.upload_ms = t0.elapsed().as_secs_f32() * 1000.0;
        self.shared.tex_over.store(self.tex_bytes as u64 > self.budget.textures || self.pressure > 0, Ordering::Relaxed);
        any
    }

    /// Drop one reference to each texture key; free textures nobody uses.
    fn release_keys(refs: &mut HashMap<String, (u32, usize)>, cache: &mut HashMap<String, wgpu::Texture>, keys_set: &mut HashSet<String>, bytes: &mut usize, keys: Vec<String>) {
        for k in keys {
            if let Some(r) = refs.get_mut(&k) {
                r.0 = r.0.saturating_sub(1);
                if r.0 == 0 {
                    *bytes = bytes.saturating_sub(r.1);
                    refs.remove(&k);
                    cache.remove(&k);
                    keys_set.remove(&k);
                }
            }
        }
    }

    fn remove_model(&mut self, m: u32) {
        self.cache.remove(&m);
        if let Some(keys) = self.model_texs.remove(&m) {
            let mut k = self.tex_keys.lock().unwrap();
            Self::release_keys(&mut self.tex_refs, &mut self.tex_cache, &mut k, &mut self.tex_bytes, keys);
        }
    }

    fn ready_by_distance(&self) -> Vec<(f32, u32)> {
        let mut v: Vec<(f32, u32)> = self
            .cache
            .iter()
            .filter(|(_, s)| matches!(s, Slot::Ready { .. }))
            .map(|(m, _)| (self.near_dist.get(m).copied().unwrap_or(f32::MAX), *m))
            .collect();
        v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    /// Keep the resident set inside the RAM-derived budget: a model-count
    /// cap (off-screen models first, then the farthest), everything off
    /// screen dropped under pressure, and over the hard cap the farthest
    /// fifth of the visible models on every check.
    fn evict(&mut self) {
        let ready = self.cache.values().filter(|s| matches!(s, Slot::Ready { .. })).count();
        let cap = if self.pressure > 0 { 0 } else { self.budget.models };
        if ready > cap {
            let mut off: Vec<(u64, u32)> = self
                .cache
                .iter()
                .filter_map(|(m, s)| match s {
                    Slot::Ready { used, .. } if !self.keep.contains(m) => Some((*used, *m)),
                    _ => None,
                })
                .collect();
            off.sort();
            let n = (ready - cap).min(off.len());
            for (_, m) in off.into_iter().take(n) {
                self.remove_model(m);
            }
        }
        if self.pressure >= 2 {
            let vis = self.ready_by_distance();
            let n = vis.len() / 5;
            for (_, m) in vis.into_iter().take(n) {
                self.remove_model(m);
            }
        }
        let ready_now = self.cache.values().filter(|s| matches!(s, Slot::Ready { .. })).count();
        if ready_now > self.budget.models {
            let vis = self.ready_by_distance();
            for (_, m) in vis.into_iter().take(ready_now - self.budget.models) {
                self.remove_model(m);
            }
        }
    }

    /// Sample memory twice a second; refresh the stream when the pressure
    /// level changes (and keep shedding while over the hard cap).
    fn check_memory(&mut self) {
        if self.mem_checked.map_or(false, |t| t.elapsed().as_millis() < 500) {
            return;
        }
        self.mem_checked = Some(Instant::now());
        self.mem = mem::query();
        // The process figure keeps memory the allocator and the graphics
        // driver hold on to after eviction, so on its own it never comes
        // back down. Judge pressure by the map's own estimate (RAM at open
        // plus GPU data, x1.6 as measured downtown), capped by the process
        // figure; the low-free-RAM guard still uses the real numbers.
        let base = *self.base_ram.get_or_insert(self.mem.process);
        let est = base + (self.gpu_bytes() as f64 * 1.6) as u64;
        self.mem_est = est.min(self.mem.process);
        let p = self.budget.pressure(&mem::MemInfo { process: self.mem_est, ..self.mem });
        self.shared.hard.store(p >= 2, Ordering::Relaxed);
        self.shared.tex_over.store(self.tex_bytes as u64 > self.budget.textures || p > 0, Ordering::Relaxed);
        if p != self.pressure || p >= 2 {
            self.pressure = p;
            self.dirty = true;
        }
    }

    fn overlay_lines(&mut self, data: &MapData) -> (Vec<LineV>, Vec<LineV>) {
        let eye = self.cam.eye();
        if let Some((e, h, r, l, a, b)) = &self.lines_cache {
            if (*e - eye).length() < 40.0 && *h == self.tod && *r == self.show_roads && *l == self.show_lights {
                return (a.clone(), b.clone());
            }
        }
        let er = rage(eye);
        let mut under = Vec::new();
        let top = Vec::new();
        if self.show_roads {
            let rr = 1500.0;
            data.road_grid.query(er[0] - rr, er[1] - rr, er[0] + rr, er[1] + rr, |i| {
                let r = &data.roads[i as usize];
                let c = map2d::ramp((r.speed / 35.0).clamp(0.0, 1.0));
                let col = [c.r(), c.g(), c.b(), 255];
                under.push(LineV { pos: (gl(r.a) + Vec3::Y * 0.4).into(), col });
                under.push(LineV { pos: (gl(r.b) + Vec3::Y * 0.4).into(), col });
            });
        }
        if self.show_lights {
            let lr = 700.0;
            let hour = (self.tod.floor() as u32).min(23);
            data.light_grid.query(er[0] - lr, er[1] - lr, er[0] + lr, er[1] + lr, |i| {
                let l = &data.lights[i as usize];
                if l.flags & 1 != 0 || l.mask & (1 << hour) == 0 || (l.flags & 2 != 0 && (7..19).contains(&hour)) {
                    return;
                }
                let col = [l.rgb_type[0], l.rgb_type[1], l.rgb_type[2], 255];
                let p = gl(l.pos);
                let s = 0.25 + l.range.min(40.0) * 0.02;
                for d in [Vec3::X, Vec3::Y, Vec3::Z] {
                    under.push(LineV { pos: (p - d * s).into(), col });
                    under.push(LineV { pos: (p + d * s).into(), col });
                }
            });
        }
        self.lines_cache = Some((eye, self.tod, self.show_roads, self.show_lights, under.clone(), top.clone()));
        (under, top)
    }

    /// Ray pick against the visible entities' bboxes.
    fn pick(&self, data: &MapData, ray_o: Vec3, ray_d: Vec3) -> Option<u32> {
        let o = rage(ray_o);
        let dd = rage(ray_d);
        let mut best: Option<(f32, u32)> = None;
        for &(i, _) in &self.visible {
            let e = &data.ents[i as usize];
            // to entity-local (rotate by -yaw)
            let (s, c) = (-e.yaw).sin_cos();
            let px = o[0] - e.pos[0];
            let py = o[1] - e.pos[1];
            let lo = [px * c - py * s, px * s + py * c, o[2] - e.pos[2]];
            let ld = [dd[0] * c - dd[1] * s, dd[0] * s + dd[1] * c, dd[2]];
            let (mut t0, mut t1) = (0.0f32, f32::MAX);
            let mut hit = true;
            for k in 0..3 {
                if ld[k].abs() < 1e-9 {
                    if lo[k] < e.lo[k] || lo[k] > e.hi[k] {
                        hit = false;
                        break;
                    }
                } else {
                    let a = (e.lo[k] - lo[k]) / ld[k];
                    let b = (e.hi[k] - lo[k]) / ld[k];
                    t0 = t0.max(a.min(b));
                    t1 = t1.min(a.max(b));
                }
            }
            if hit && t0 <= t1 {
                // prefer small boxes on ties (a prop on a ground tile)
                let vol = (e.hi[0] - e.lo[0]) * (e.hi[1] - e.lo[1]) * (e.hi[2] - e.lo[2]);
                let key = t0 + vol.cbrt() * 0.05;
                if best.map_or(true, |b| key < b.0) {
                    best = Some((key, i));
                }
            }
        }
        best.map(|b| b.1)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        let t_open = *self.perf.t_open.get_or_insert(now);
        if let Some(l) = self.perf.last_frame.replace(now) {
            let ms = (now - l).as_secs_f32() * 1000.0;
            self.perf.frame_ms.push_back(ms);
            if self.perf.frame_ms.len() > 240 {
                self.perf.frame_ms.pop_front();
            }
            if self.auto.fly_from.is_some() && !self.auto.fly_done {
                self.auto.fly_ms.push(ms);
            }
        }
        let Some(rs) = frame.wgpu_render_state() else {
            ui.label("The 3D map needs the wgpu renderer.");
            return;
        };
        let Some(data) = self.data.clone() else {
            ui.label("Loading the baked map (work/map) …  Bake it first if this stays: sixng-gui --bake-map");
            ctx.request_repaint();
            return;
        };
        if self.perf.data_s.is_none() {
            self.perf.data_s = Some(t_open.elapsed().as_secs_f32());
        }
        self.frame += 1;
        self.start_workers();
        if self.renderer.is_none() {
            let ai = rs.adapter.get_info();
            eprintln!("[map3d] adapter {} ({:?}, {:?}) present {:?}", ai.name, ai.backend, ai.device_type, rs.target_format);
            self.renderer = Some(Renderer3D::new(rs));
        }
        // side panel
        egui::SidePanel::right("map3d_side").resizable(true).default_width(300.0).show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.side_ui(ui));
        });
        if self.detail_level_changed() {
            self.dirty = true;
        }
        // uploads first, then visibility
        let tph = Instant::now();
        self.check_memory();
        let got = if self.pressure >= 2 { self.drain_loads() } else { self.poll_loads(rs) };
        let eye = self.cam.eye();
        let moved = (eye - self.last_eye).length();
        let turned = self.cam.forward().dot(self.last_fwd) < 0.995;
        if self.dirty || moved > 15.0 || turned || self.last_kinds != self.kinds || self.last_lods != self.lods || self.last_proxies != self.show_proxies || self.last_detail.as_ref() != Some(&self.detail) {
            let lod_changed = self.last_detail.as_ref().map_or(false, |d| d.drawable_lod != self.detail.drawable_lod || d.textures != self.detail.textures || d.max_tex != self.detail.max_tex);
            if lod_changed {
                self.cache.clear();
                self.tex_cache.clear();
                self.tex_bytes = 0;
                self.tex_refs.clear();
                self.model_texs.clear();
                self.tex_keys.lock().unwrap().clear();
            }
            self.update_visible(&data);
            self.last_eye = eye;
            self.last_fwd = self.cam.forward();
            self.last_detail = Some(self.detail.clone());
            self.last_kinds = self.kinds;
            self.last_lods = self.lods;
            self.last_proxies = self.show_proxies;
            self.dirty = false;
            self.apply_instances(rs);
            self.evict();
            self.govern();
        } else if got {
            self.apply_instances(rs);
            self.govern();
        }
        self.stats.phase[0] = tph.elapsed().as_secs_f32() * 1000.0;
        let tph = Instant::now();
        let er = rage(eye);
        if self.show_terrain {
            self.world.start(Arc::new(data.h8.clone()));
            let ren = self.renderer.as_ref().unwrap();
            if self.world.poll(rs, ren, 4.0) {
                ctx.request_repaint();
            }
            self.update_building_readiness(&data);
        }
        {
            let inflight = self.shared.inflight.lock().unwrap().len();
            let queued = self.shared.queue.lock().unwrap().len();
            self.stats.inflight = inflight + self.shared.unread.load(Ordering::SeqCst);
            self.stats.queued = queued;
            self.stats.models_ready = self.cache.values().filter(|s| matches!(s, Slot::Ready { .. })).count();
            self.stats.failed = self.cache.values().filter(|s| matches!(s, Slot::Failed(_))).count();
            self.stats.textures = self.tex_cache.len();
            self.stats.tex_gb = self.tex_bytes as f32 / (1u64 << 30) as f32;
        }

        // viewport
        let avail = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(avail, egui::Sense::click_and_drag());
        self.cam.near = 0.5;
        self.cam.far = (self.detail.radii[5] * 1.3).max(3000.0);
        self.cam.handle(ui, &resp);
        // fly speed: the camera handler moves near*400 m/s; scale with height
        if resp.hovered() {
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            let ground = data.height(er[0], er[1]).unwrap_or(0.0);
            let alt = (er[2] - ground).max(2.0);
            let sp = (alt * 0.8).clamp(8.0, 600.0) * self.speed * dt * if ui.input(|i| i.modifiers.shift) { 4.0 } else { 1.0 };
            let f = self.cam.forward();
            let right = f.cross(Vec3::Y).normalize_or_zero();
            ui.input(|i| {
                if i.key_down(egui::Key::W) || i.key_down(egui::Key::ArrowUp) { self.cam.eye += f * sp; }
                if i.key_down(egui::Key::S) || i.key_down(egui::Key::ArrowDown) { self.cam.eye -= f * sp; }
                if i.key_down(egui::Key::A) || i.key_down(egui::Key::ArrowLeft) { self.cam.eye -= right * sp; }
                if i.key_down(egui::Key::D) || i.key_down(egui::Key::ArrowRight) { self.cam.eye += right * sp; }
                if i.key_down(egui::Key::E) || i.key_down(egui::Key::PageUp) { self.cam.eye += Vec3::Y * sp; }
                if i.key_down(egui::Key::Q) || i.key_down(egui::Key::PageDown) { self.cam.eye -= Vec3::Y * sp; }
            });
        }
        let ppp = ctx.pixels_per_point();
        let px = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
        self.size = px;
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let ndc_x = (p.x - rect.left()) / rect.width() * 2.0 - 1.0;
                let ndc_y = 1.0 - (p.y - rect.top()) / rect.height() * 2.0;
                let aspect = rect.width() / rect.height();
                let inv = (self.cam.proj(aspect) * self.cam.view()).inverse();
                // reversed Z: 1 = near plane
                let a = inv.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
                let b = inv.project_point3(Vec3::new(ndc_x, ndc_y, 0.5));
                self.picked = self.pick(&data, self.cam.eye(), (b - a).normalize());
            }
        }
        let (under, top) = self.overlay_lines(&data);
        if self.show_terrain {
            self.cam.far = 150_000.0;
            let ready = &self.bld_ready;
            let ok = |r: [f32; 4]| -> bool {
                let c0 = ((r[0] - crate::mapbake::X0) / 128.0).floor() as i32;
                let c1 = ((r[2] - crate::mapbake::X0) / 128.0).ceil() as i32;
                let r0 = ((crate::mapbake::Y1 - r[3]) / 128.0).floor() as i32;
                let r1 = ((crate::mapbake::Y1 - r[1]) / 128.0).ceil() as i32;
                for cy in r0..r1 {
                    for cx in c0..c1 {
                        if let Some(&(w, l)) = ready.get(&(cx, cy)) {
                            if (l as f32) < 0.9 * w as f32 {
                                return false;
                            }
                        }
                    }
                }
                true
            };
            let view = world3d::View { cam: &self.cam, aspect: rect.width() / rect.height().max(1.0), viewport_h: px[1] as f32, tau: self.world_tau, ground_r: self.ground_radius(), ground_ok: &ok };
            self.world.select(&view);
        }
        self.stats.phase[1] = tph.elapsed().as_secs_f32() * 1000.0;
        let mut models: Vec<&GpuModel> = Vec::new();
        if self.show_terrain {
            let ren = self.renderer.as_ref().unwrap();
            models.extend(self.world.models(rs, ren));
        }
        for s in self.cache.values() {
            if let Slot::Ready { gm, .. } = s {
                if gm.n_instances > 0 {
                    models.push(gm);
                }
            }
        }
        // thinner fog higher up, so the whole map shows when zoomed out
        let mut look = self.look.clone();
        let alt = (er[2] - data.height(er[0], er[1]).unwrap_or(0.0)).max(1.0);
        look.fog *= (400.0 / (alt + 400.0)).min(1.0);
        let ren = self.renderer.as_mut().unwrap();
        ren.decal_bias = self.decal_bias;
        let tr = Instant::now();
        let tid = ren.render(rs, px, &self.cam, &look, &models, &under, &top);
        self.stats.render_ms = tr.elapsed().as_secs_f32() * 1000.0;
        self.stats.draws = ren.draws;
        ui.painter().image(tid, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        // picked entity outline (bbox corners projected)
        if let Some(i) = self.picked {
            let e = &data.ents[i as usize];
            let (s, c) = e.yaw.sin_cos();
            let corner = |k: usize| {
                let lx = if k & 1 == 0 { e.lo[0] } else { e.hi[0] };
                let ly = if k & 2 == 0 { e.lo[1] } else { e.hi[1] };
                let lz = if k & 4 == 0 { e.lo[2] } else { e.hi[2] };
                gl([e.pos[0] + lx * c - ly * s, e.pos[1] + lx * s + ly * c, e.pos[2] + lz])
            };
            let pts: Vec<Option<Pos2>> = (0..8).map(|k| Renderer3D::project(&self.cam, rect.size(), corner(k)).map(|p| p + rect.min.to_vec2())).collect();
            for (a, b) in [(0, 1), (1, 3), (3, 2), (2, 0), (4, 5), (5, 7), (7, 6), (6, 4), (0, 4), (1, 5), (2, 6), (3, 7)] {
                if let (Some(pa), Some(pb)) = (pts[a], pts[b]) {
                    ui.painter().line_segment([pa, pb], Stroke::new(1.5_f32, Color32::from_rgb(255, 220, 40)));
                }
            }
        }
        // hint
        ui.painter().text(rect.left_bottom() + egui::vec2(8.0, -8.0), egui::Align2::LEFT_BOTTOM, "drag: look · WASD/arrows: move · Q/E: down/up · Shift: fast · right-drag: pan · click: inspect", egui::FontId::proportional(12.0), Color32::from_white_alpha(200));
        let tov = Instant::now();
        self.overlay_windows(&ctx, rect, &data);
        self.stats.phase[2] = tov.elapsed().as_secs_f32() * 1000.0;
        self.stats.phase[3] = now.elapsed().as_secs_f32() * 1000.0;
        self.automation(&ctx, rs, rect);
        let busy = self.stats.queued > 0 || self.stats.inflight > 0;
        if busy || resp.hovered() || resp.dragged() || self.auto.screenshot.is_some() || self.auto.ui_shot.is_some() {
            ctx.request_repaint();
        }
    }

    /// Visible building placements per 128 m cell and how many have their
    /// model loaded: the far field switches a node to ground heights only
    /// when its buildings are on screen.
    /// Keep the drawn triangles near the budget by raising or lowering the
    /// screen-size cut.
    fn govern(&mut self) {
        // triangles and draw calls, whichever is further over its budget
        let t = (self.stats.tris as f32 / self.tri_budget.max(1) as f32).max(self.stats.draws as f32 / self.draw_budget.max(1) as f32);
        let b = 1.0;
        if t > b && self.size_boost < 16.0 {
            self.size_boost = (self.size_boost * 1.25).min(16.0);
            self.dirty = true;
        } else if t < 0.6 * b && self.size_boost > 1.0 {
            self.size_boost = (self.size_boost / 1.15).max(1.0);
            self.dirty = true;
        }
    }

    /// Apply a quality preset: detail, triangle budget, terrain error, texture size.
    pub fn set_quality(&mut self, q: usize) {
        let q = q.min(QUALITY.len() - 1);
        let (_, det, tris, tau, tex, draws) = QUALITY[q];
        self.draw_budget = draws;
        self.quality = q;
        self.detail_level = det;
        let keep = (self.detail.drawable_lod, self.detail.textures, self.detail.hd_fallback);
        self.detail = Detail::from_level(det);
        self.detail.drawable_lod = keep.0;
        self.detail.textures = keep.1;
        self.detail.hd_fallback = keep.2;
        self.detail.max_tex = tex;
        self.tri_budget = (tris * 1.0e6) as usize;
        self.world_tau = tau;
        self.size_boost = 1.0;
        self.dirty = true;
    }

    fn ground_radius(&self) -> f32 {
        // out to here the far field lies flat under the streamed buildings
        // (the HD band of large buildings); beyond it shows them as blocks
        let d = &self.detail;
        if d.hd_fallback { d.radii[1].min(d.radii[0] * 1.75).max(d.radii[0]) } else { d.radii[0] }
    }

    fn update_building_readiness(&mut self, data: &MapData) {
        let mut m: HashMap<(i32, i32), (u32, u32)> = HashMap::new();
        for &(i, model) in &self.visible {
            let e = &data.ents[i as usize];
            if e.kind != 1 {
                continue;
            }
            let k = (((e.pos[0] - crate::mapbake::X0) / 128.0).floor() as i32, ((crate::mapbake::Y1 - e.pos[1]) / 128.0).floor() as i32);
            let v = m.entry(k).or_insert((0, 0));
            v.0 += 1;
            if self.cache.contains_key(&model) {
                v.1 += 1;
            }
        }
        self.bld_ready = m;
    }

    fn detail_level_changed(&mut self) -> bool {
        false
    }

    /// Apply a RAM limit (None = automatic) and save it for next time.
    pub fn set_ram_cap(&mut self, cap: Option<f32>, save: bool) {
        self.ram_cap_gb = cap;
        let auto = mem::Budget::for_total(mem::query().total);
        self.budget = match cap {
            Some(gb) => auto.with_cap(gb),
            None => auto,
        };
        self.dirty = true;
        if save {
            let p = Path::new(&self.work).join("gui_settings.json");
            let mut j: serde_json::Value = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| serde_json::json!({}));
            j["map3d_ram_cap_gb"] = match cap {
                Some(v) => serde_json::json!(v),
                None => serde_json::Value::Null,
            };
            let _ = std::fs::write(&p, serde_json::to_vec_pretty(&j).unwrap_or_default());
        }
    }

    /// Load the saved RAM limit and quality preset, if any.
    pub fn load_settings(&mut self) {
        let p = Path::new(&self.work).join("gui_settings.json");
        let j = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()).unwrap_or_default();
        let cap = j["map3d_ram_cap_gb"].as_f64().map(|v| v as f32);
        if cap.is_some() {
            self.set_ram_cap(cap, false);
        }
        if let Some(q) = j["map3d_quality"].as_str().and_then(|q| QUALITY.iter().position(|x| x.0 == q)) {
            self.set_quality(q);
        }
    }

    fn save_setting(&self, key: &str, v: serde_json::Value) {
        let p = Path::new(&self.work).join("gui_settings.json");
        let mut j: serde_json::Value = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| serde_json::json!({}));
        j[key] = v;
        let _ = std::fs::write(&p, serde_json::to_vec_pretty(&j).unwrap_or_default());
    }

    fn ram_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Memory");
        let total = mem::gb(self.mem.total).max(4.0);
        let mut auto = self.ram_cap_gb.is_none();
        let mut gb = self.ram_cap_gb.unwrap_or(mem::gb(self.budget.soft));
        let mut changed = false;
        ui.horizontal(|ui| {
            if ui.checkbox(&mut auto, "automatic").on_hover_text("20 % of installed RAM before streaming slows down, 30 % before distant models are dropped").changed() {
                changed = true;
            }
            ui.add_enabled_ui(!auto, |ui| {
                let r = ui.add(egui::Slider::new(&mut gb, 1.0..=(total * 0.9).floor()).step_by(0.5).suffix(" GB").text("RAM limit"));
                // only real input saves (the slider also reports a change
                // when it snaps a value set from the command line)
                if r.drag_stopped() || (r.changed() && r.has_focus()) {
                    changed = true;
                }
            });
        });
        if changed {
            self.set_ram_cap(if auto { None } else { Some(gb) }, true);
        }
        let (mi, bu) = (self.mem, self.budget);
        let frac = (self.mem_est as f32 / bu.hard.max(1) as f32).clamp(0.0, 1.0);
        let col = match self.pressure {
            0 => Color32::from_rgb(90, 170, 90),
            1 => Color32::from_rgb(220, 180, 40),
            _ => Color32::from_rgb(220, 80, 60),
        };
        ui.add(egui::ProgressBar::new(frac).fill(col).text(format!("{:.1} GB map · slows at {:.1} · drops at {:.1} GB", mem::gb(self.mem_est), mem::gb(bu.soft), mem::gb(bu.hard)))).on_hover_text(format!("Map estimate: RAM at open plus GPU data. Process total {:.1} GB (includes memory the allocator and driver keep after eviction).", mem::gb(mi.process)));
        ui.label(egui::RichText::new(format!("{:.0} GB installed · {:.1} GB free · textures ≤ {:.1} GB · up to {} models", mem::gb(mi.total), mem::gb(mi.avail), mem::gb(bu.textures), bu.models)).small().weak());
        ui.separator();
    }

    fn side_ui(&mut self, ui: &mut egui::Ui) {
        self.ram_ui(ui);
        ui.heading("Quality");
        ui.horizontal(|ui| {
            for k in 0..QUALITY.len() {
                if ui.selectable_label(self.quality == k, QUALITY[k].0).on_hover_text("Detail, triangle and draw-call budget, far-field error and texture size together").clicked() {
                    self.set_quality(k);
                    self.save_setting("map3d_quality", serde_json::json!(QUALITY[k].0));
                }
            }
        });
        let mut tb = self.tri_budget as f32 / 1e6;
        if ui.add(egui::Slider::new(&mut tb, 1.0..=80.0).logarithmic(true).suffix(" M").text("triangle budget")).on_hover_text("Over this the smallest objects on screen are dropped first").changed() {
            self.tri_budget = (tb * 1e6) as usize;
            self.dirty = true;
        }
        let mut db = self.draw_budget as f32;
        if ui.add(egui::Slider::new(&mut db, 500.0..=40000.0).logarithmic(true).text("draw-call budget")).on_hover_text("GPU cost grows with the number of draw calls (one per model material)").changed() {
            self.draw_budget = db as usize;
            self.dirty = true;
        }
        ui.label(egui::RichText::new(format!("small objects cut ×{:.1} to stay in budget", self.size_boost)).small().weak());
        ui.separator();
        ui.heading("Detail");
        let mut lvl = self.detail_level;
        if ui.add(egui::Slider::new(&mut lvl, 0.0..=1.0).text("detail")).changed() {
            self.detail_level = lvl;
            let keep = (self.detail.drawable_lod, self.detail.textures, self.detail.hd_fallback);
            self.detail = Detail::from_level(lvl);
            self.detail.drawable_lod = keep.0;
            self.detail.textures = keep.1;
            self.detail.hd_fallback = keep.2;
        }
        egui::CollapsingHeader::new("LOD distances (m)").default_open(false).show(ui, |ui| {
            for k in 0..6 {
                let lo = if k == 0 { 10.0 } else { self.detail.radii[k - 1] };
                ui.add(egui::Slider::new(&mut self.detail.radii[k], lo..=12000.0).logarithmic(true).text(LOD_NAMES[k]));
            }
        });
        ui.checkbox(&mut self.detail.hd_fallback, "HD fallback for unlinked LODs").on_hover_text("Most LOD placements have no linked model yet; keep large HD models out to the LOD distance instead and skip the LOD level.");
        ui.add(egui::Slider::new(&mut self.detail.prop_density, 0.0..=1.0).text("prop density"));
        ui.add(egui::Slider::new(&mut self.detail.veg_density, 0.0..=1.0).text("vegetation density"));
        ui.add(egui::Slider::new(&mut self.detail.min_size, 0.0..=0.03).text("min screen size"));
        ui.horizontal(|ui| {
            ui.label("drawable LOD");
            for k in 0..4 {
                ui.selectable_value(&mut self.detail.drawable_lod, k, format!("{k}"));
            }
        });
        ui.label(egui::RichText::new("Building drawables keep trim strips in LOD 1-3; the distance bands use the separate LOD/SLOD placements instead.").small().weak());
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.detail.textures, "textures");
            egui::ComboBox::from_id_salt("maxtex").selected_text(format!("max {} px", self.detail.max_tex)).show_ui(ui, |ui| {
                for v in [128u32, 256, 512, 1024] {
                    ui.selectable_value(&mut self.detail.max_tex, v, format!("{v} px"));
                }
            });
        });
        ui.separator();
        ui.heading("Show");
        ui.horizontal_wrapped(|ui| {
            for k in 0..6 {
                ui.checkbox(&mut self.kinds[k], KIND_NAMES[k]);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("levels");
            for k in 0..6 {
                ui.checkbox(&mut self.lods[k], LOD_NAMES[k]);
            }
        });
        ui.checkbox(&mut self.show_terrain, "whole-map terrain").on_hover_text("Far field: the baked 8 m top surface as a streamed quadtree, with the 2D map tiles as colour. Buildings show as blocks until their models load.");
        ui.add(egui::Slider::new(&mut self.world_tau, 1.0..=12.0).logarithmic(true).text("terrain error (px)")).on_hover_text("Screen-space error of the far field: smaller is finer and loads more tiles");
        ui.checkbox(&mut self.show_roads, "roads (by speed)");
        ui.checkbox(&mut self.show_lights, "lights (exterior, by hour)");
        ui.checkbox(&mut self.show_proxies, "show proxies / placeholders").on_hover_text("Debug: also draw interior-shell, shadow, light and VFX proxies, placements without a model and degenerate models");
        ui.checkbox(&mut self.decal_bias, "decal depth bias").on_hover_text("Markings, grime, signage and terrain blend layers drawn after opaque surfaces with a depth bias, by layer order");
        ui.checkbox(&mut self.show_minimap, "minimap window");
        ui.checkbox(&mut self.show_stats, "stats window");
        ui.separator();
        ui.heading("Light");
        if ui.add(egui::Slider::new(&mut self.tod, 0.0..=24.0).text("time of day")).changed() {
            self.look.set_time_of_day(self.tod);
        }
        ui.add(egui::Slider::new(&mut self.look.fog, 0.0..=0.002).logarithmic(true).text("fog"));
        ui.add(egui::Slider::new(&mut self.look.exposure, 0.2..=4.0).text("exposure"));
        egui::ComboBox::from_label("packed data").selected_text(crate::model_view::RMA_ORDERS[self.look.rma_order.min(3) as usize]).show_ui(ui, |ui| {
            for (k, m) in crate::model_view::RMA_ORDERS.iter().enumerate() {
                ui.selectable_value(&mut self.look.rma_order, k as u32, *m);
            }
        }).response.on_hover_text("How the packed material-data texture is read. Its channel order is unverified; \"ignore packed\" (default) avoids mirror-like surfaces.");
        ui.add(egui::Slider::new(&mut self.speed, 0.1..=10.0).logarithmic(true).text("fly speed"));
        ui.separator();
        ui.heading("Go to");
        let er = rage(self.cam.eye());
        ui.label(format!("eye RAGE ({:.0}, {:.0}, {:.0})", er[0], er[1], er[2]));
        ui.horizontal(|ui| {
            if ui.button("downtown").clicked() {
                self.place(-640.0, -60.0, 120.0);
            }
            if ui.button("street level").clicked() {
                self.place(er[0], er[1], 2.0);
                self.cam.pitch = 0.05;
            }
            if ui.button("overview").clicked() {
                self.place(er[0], er[1], 900.0);
                self.cam.pitch = 0.9;
            }
        });
    }

    fn overlay_windows(&mut self, ctx: &egui::Context, rect: egui::Rect, data: &MapData) {
        if self.show_stats {
            let st = self.stats.clone();
            let picked = self.picked;
            let mut open_model = None;
            egui::Window::new("Stream")
                .default_pos(rect.left_top() + egui::vec2(10.0, 10.0))
                .constrain_to(rect)
                .resizable(false)
                .collapsible(true)
                .show(ctx, |ui| {
                    ui.label(format!("entities {} · models {} wanted / {} loaded", st.entities, st.models_wanted, st.models_ready));
                    ui.label(format!("queue {} (sharpen {}) · loading {} · failed {}", st.queued, st.upgrades, st.inflight, st.failed));
                    ui.label(format!("instances {} · {:.1} M tris · textures {} ({:.2} GB)", st.instances, st.tris as f32 / 1e6, st.textures, st.tex_gb));
                    ui.label(format!("bands: {}", (0..6).map(|k| format!("{} {}", LOD_NAMES[k], st.per_band[k])).collect::<Vec<_>>().join(" · ")));
                    ui.label(format!("LOD parents handed to children {} · proxies hidden {}", st.handoffs, st.hidden));
                    let ws = self.world.stats.clone();
                    ui.label(egui::RichText::new(format!("draw calls {} · render {:.1} ms · size cut ×{:.2} (budget {:.0} M tris, {} draws)", st.draws, st.render_ms, self.size_boost, self.tri_budget as f32 / 1e6, self.draw_budget)).small().weak());
                    ui.label(format!("terrain: {} tiles drawn ({} ground) · level ≤ {} · {} resident · {} queued · {:.1} M tris", ws.drawn, ws.ground, ws.max_level, ws.resident, ws.queued, ws.tris as f32 / 1e6));
                    ui.label(egui::RichText::new(format!("update {:.1} ms ({} tested) · upload {:.1} ms", st.update_ms, st.visited, st.upload_ms)).small().weak());
                    let (mi, bu, pr) = (self.mem, self.budget, self.pressure);
                    let txt = format!("map {:.1} GB, process {:.1} GB (caps {:.1} / {:.1} of {:.0} GB) · {:.1} GB free · textures ≤ {:.1} GB · models ≤ {}", mem::gb(self.mem_est), mem::gb(mi.process), mem::gb(bu.soft), mem::gb(bu.hard), mem::gb(mi.total), mem::gb(mi.avail), mem::gb(bu.textures), bu.models);
                    match pr {
                        0 => ui.label(egui::RichText::new(txt).small().weak()),
                        1 => ui.colored_label(Color32::YELLOW, format!("{txt}\nmemory cap reached: loading only nearby models, 64 px textures")),
                        _ => ui.colored_label(Color32::LIGHT_RED, format!("{txt}\nover the hard cap: dropping distant models")),
                    };
                    if let Some(i) = picked {
                        ui.separator();
                        let e = &data.ents[i as usize];
                        ui.strong(data.name(e.name));
                        ui.label(format!("{} · {} · model {}", KIND_NAMES[e.kind.min(5) as usize], LOD_NAMES[e.lod.min(5) as usize], data.name(e.model)));
                        ui.label(format!("pos ({:.1}, {:.1}, {:.1}) · yaw {:.0}°", e.pos[0], e.pos[1], e.pos[2], e.yaw.to_degrees()));
                        ui.label(format!("bbox {:.1} × {:.1} × {:.1} m", e.hi[0] - e.lo[0], e.hi[1] - e.lo[1], e.hi[2] - e.lo[2]));
                        ui.label(format!("lod_dist {:.0} · child_lod_dist {:.0} · parent {}", e.lod_dist, e.child_lod_dist, if e.parent == u32::MAX { "none".to_string() } else { data.name(data.ents[e.parent as usize].name).to_string() }));
                        let fl = rflag_names(e.rflags);
                        if !fl.is_empty() {
                            ui.label(format!("flags: {}", fl.join(", ")));
                        }
                        if let Some(Slot::Failed(err)) = self.cache.get(&e.model) {
                            ui.colored_label(Color32::LIGHT_RED, err);
                        }
                        if ui.button("Open in model viewer").clicked() {
                            open_model = Some(data.name(e.model).to_string());
                        }
                    }
                });
            if open_model.is_some() {
                self.open_model = open_model;
            }
        }
        if self.show_minimap {
            let eye = rage(self.cam.eye());
            let fwd = rage(self.cam.forward());
            let mut teleport = None;
            egui::Window::new("Minimap")
                .default_pos(rect.right_top() + egui::vec2(-250.0, 10.0))
                .constrain_to(rect)
                .resizable(false)
                .collapsible(true)
                .show(ctx, |ui| {
                    let size = egui::vec2(230.0, 230.0);
                    let (r, resp) = ui.allocate_exact_size(size, egui::Sense::click());
                    let p = ui.painter_at(r);
                    p.rect_filled(r, 4.0, Color32::from_rgb(28, 32, 36));
                    let range = 900.0f32; // metres from centre to edge
                    let to_px = |x: f32, y: f32| r.center() + egui::vec2((x - eye[0]) / range * size.x * 0.5, -(y - eye[1]) / range * size.y * 0.5);
                    data.road_grid.query(eye[0] - range, eye[1] - range, eye[0] + range, eye[1] + range, |i| {
                        let rd = &data.roads[i as usize];
                        let c = map2d::ramp((rd.speed / 35.0).clamp(0.0, 1.0));
                        p.line_segment([to_px(rd.a[0], rd.a[1]), to_px(rd.b[0], rd.b[1])], Stroke::new(1.0_f32, c));
                    });
                    // HD radius and view wedge
                    p.circle_stroke(r.center(), self.detail.radii[0] / range * size.x * 0.5, Stroke::new(1.0_f32, Color32::from_white_alpha(60)));
                    let yaw = fwd[1].atan2(fwd[0]);
                    let half = self.cam.fov * 0.5 * 1.3;
                    let len = 60.0;
                    let a = r.center() + egui::vec2((yaw - half).cos(), -(yaw - half).sin()) * len;
                    let b = r.center() + egui::vec2((yaw + half).cos(), -(yaw + half).sin()) * len;
                    p.add(egui::Shape::convex_polygon(vec![r.center(), a, b], Color32::from_rgba_unmultiplied(255, 220, 60, 50), Stroke::new(1.0_f32, Color32::from_rgb(255, 220, 60))));
                    p.text(r.left_bottom() + egui::vec2(4.0, -4.0), egui::Align2::LEFT_BOTTOM, format!("±{range:.0} m · click to move"), egui::FontId::proportional(10.0), Color32::from_white_alpha(160));
                    if resp.clicked() {
                        if let Some(q) = resp.interact_pointer_pos() {
                            let d = q - r.center();
                            teleport = Some([eye[0] + d.x / (size.x * 0.5) * range, eye[1] - d.y / (size.y * 0.5) * range]);
                        }
                    }
                });
            if let Some([x, y]) = teleport {
                let g = data.height(eye[0], eye[1]).unwrap_or(0.0);
                let alt = (eye[2] - g).max(2.0);
                self.place(x, y, alt);
            }
        }
    }

    fn automation(&mut self, ctx: &egui::Context, rs: &egui_wgpu::RenderState, rect: egui::Rect) {
        if self.perf.coarse_s.is_none() && self.frame > 5 && self.stats.queued <= self.stats.upgrades && !self.keep.is_empty() && self.keep.iter().all(|m| self.cache.contains_key(m)) {
            self.perf.coarse_s = self.perf.t_open.map(|t| t.elapsed().as_secs_f32());
        }
        if let Some(path) = self.auto.screenshot.clone() {
            self.auto.frames += 1;
            let started = *self.auto.started.get_or_insert_with(Instant::now);
            let idle = self.stats.queued == 0 && self.stats.inflight == 0 && self.rx.as_ref().map_or(true, |_| true);
            if idle && self.frame > 5 {
                self.auto.quiet_frames += 1;
            } else {
                self.auto.quiet_frames = 0;
            }
            let mut ready = (self.auto.quiet_frames > 20 && self.frames_ok()) || started.elapsed().as_secs() > 900;
            if ready && self.perf.first_idle_s.is_none() {
                self.perf.first_idle_s = self.perf.t_open.map(|t| t.elapsed().as_secs_f32());
                self.perf_log("first view loaded");
            }
            if let Some((to, secs)) = self.auto.fly {
                if !self.auto.fly_done {
                    if self.auto.fly_from.is_none() && ready {
                        self.auto.fly_from = Some((self.cam.eye(), Instant::now()));
                    }
                    if let Some((from, t0)) = self.auto.fly_from {
                        let f = (t0.elapsed().as_secs_f32() / secs.max(0.1)).min(1.0);
                        let g = self.data.as_ref().and_then(|d| d.height(to[0], to[1])).unwrap_or(0.0);
                        self.cam.eye = from.lerp(gl([to[0], to[1], g + to[2]]), f);
                        self.dirty = true;
                        if f >= 1.0 {
                            self.auto.fly_done = true;
                            let (mean, p99, worst) = frame_summary(&self.auto.fly_ms);
                            self.perf_log(&format!("fly {:.0} s: {} frames, mean {:.1} ms ({:.0} fps), p99 {:.1} ms, worst {:.1} ms", secs, self.auto.fly_ms.len(), mean, 1000.0 / mean.max(0.01), p99, worst));
                            self.auto.quiet_frames = 0;
                        }
                    }
                    ready = false;
                }
            }
            if ready {
                let v: Vec<f32> = self.perf.frame_ms.iter().copied().collect();
                let (mean, p99, worst) = frame_summary(&v);
                self.perf_log(&format!("shot: last {} frames mean {:.1} ms ({:.0} fps), p99 {:.1}, worst {:.1}", v.len(), mean, 1000.0 / mean.max(0.01), p99, worst));
                if let Some(data) = self.data.clone() {
                    for f in std::mem::take(&mut self.auto.picks) {
                        let (ndc_x, ndc_y) = (f[0] * 2.0 - 1.0, 1.0 - f[1] * 2.0);
                        let inv = (self.cam.proj(rect.width() / rect.height()) * self.cam.view()).inverse();
                        let a = inv.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
                        let b = inv.project_point3(Vec3::new(ndc_x, ndc_y, 0.5));
                        match self.pick(&data, self.cam.eye(), (b - a).normalize()) {
                            Some(i) => {
                                let e = &data.ents[i as usize];
                                eprintln!("pick {:.2},{:.2}: {} model {} at {:.0},{:.0},{:.0}", f[0], f[1], data.name(e.name), data.name(e.model), e.pos[0], e.pos[1], e.pos[2]);
                            }
                            None => eprintln!("pick {:.2},{:.2}: nothing", f[0], f[1]),
                        }
                    }
                }
                if let Some((w, h, px)) = self.renderer.as_ref().and_then(|r| r.read_back(rs)) {
                    let _ = render3d::save_png(&path, w, h, &px);
                }
                self.auto.screenshot = None;
                let ui_path = path.with_file_name(format!("{}_ui.png", path.file_stem().unwrap_or_default().to_string_lossy()));
                self.auto.ui_shot = Some(ui_path);
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            ctx.request_repaint();
        }
        if let Some(p) = self.auto.ui_shot.clone() {
            let shot = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                let _ = render3d::save_png(&p, img.size[0] as u32, img.size[1] as u32, &rgba);
                self.auto.ui_shot = None;
                if self.auto.exit {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            ctx.request_repaint();
        }
    }

    /// Estimated GPU bytes: vertex, index and instance buffers plus textures.
    fn gpu_bytes(&self) -> u64 {
        let mut b = self.tex_bytes as u64;
        for s in self.cache.values() {
            if let Slot::Ready { gm, .. } = s {
                b += gm.instances.size();
                for m in &gm.meshes {
                    b += m.vb.size() + m.ib.size();
                }
            }
        }
        b + self.world.stats.gpu_bytes
    }

    /// One line to stderr and `<work>/map3d_perf.log`.
    fn perf_log(&mut self, what: &str) {
        let m = mem::query();
        let st = &self.stats;
        let er = rage(self.cam.eye());
        let ws = self.world.stats.clone();
        let line = format!(
            "[map3d] {what} | map est {:.2} GB | coarse {} | terrain {} tiles ({} ground, level <= {}, {} resident, {} queued, {:.1} M tris) | t {:.1} s (data {:.1} s, first view {}) | eye ({:.0}, {:.0}, {:.0}) | RAM {:.2} GB | GPU est {:.2} GB | models {} | inst {} | {:.1} M tris | update {:.1} ms ({} tested) | draws {} | render cpu {:.1} ms | stream {:.1} ms | terrain {:.1} ms | windows {:.1} ms | ui total {:.1} ms | size cut x{:.2} | loads {}: lookup {:.0} ms, export {} x {:.0} ms, glb {:.0} ms avg",
            mem::gb(self.mem_est),
            self.perf.coarse_s.map_or("-".to_string(), |v| format!("{v:.1} s")),
            ws.drawn, ws.ground, ws.max_level, ws.resident, ws.queued, ws.tris as f32 / 1e6,
            self.perf.t_open.map_or(0.0, |t| t.elapsed().as_secs_f32()),
            self.perf.data_s.unwrap_or(0.0),
            self.perf.first_idle_s.map_or("-".to_string(), |v| format!("{v:.1} s")),
            er[0], er[1], er[2],
            mem::gb(m.process),
            self.gpu_bytes() as f32 / (1u64 << 30) as f32,
            st.models_ready, st.instances, st.tris as f32 / 1e6, st.update_ms, st.visited, st.draws, st.render_ms, st.phase[0], st.phase[1], st.phase[2], st.phase[3], self.size_boost,
            LOAD_T[3].load(Ordering::Relaxed),
            LOAD_T[0].load(Ordering::Relaxed) as f32 / 1000.0 / LOAD_T[3].load(Ordering::Relaxed).max(1) as f32,
            LOAD_T[4].load(Ordering::Relaxed),
            LOAD_T[1].load(Ordering::Relaxed) as f32 / 1000.0 / LOAD_T[4].load(Ordering::Relaxed).max(1) as f32,
            LOAD_T[2].load(Ordering::Relaxed) as f32 / 1000.0 / LOAD_T[3].load(Ordering::Relaxed).max(1) as f32
        );
        eprintln!("{line}");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(Path::new(&self.work).join("map3d_perf.log")) {
            use std::io::Write;
            let _ = writeln!(f, "{line}");
        }
    }

    fn frames_ok(&self) -> bool {
        // under memory pressure not every wanted model will load: an empty
        // queue is as good as it gets
        (!self.show_terrain || self.world.idle()) && (self.pressure > 0 || self.keep.iter().all(|m| self.cache.contains_key(m)))
    }
}

impl Drop for Map3D {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.cv.notify_all();
    }
}

trait NanOr {
    fn nan_or(self, v: f32) -> f32;
}
impl NanOr for f32 {
    fn nan_or(self, v: f32) -> f32 {
        if self.is_nan() { v } else { self }
    }
}

/// Compact spatial grid (cell lists in one array) over the baked map box.
struct CsrGrid {
    cell: f32,
    nx: usize,
    ny: usize,
    start: Vec<u32>,
    items: Vec<u32>,
}

impl CsrGrid {
    fn build(cell: f32, pts: &[(f32, f32, u32)]) -> Self {
        let nx = (crate::mapbake::W as f32 * crate::mapbake::CELL / cell).ceil() as usize + 1;
        let ny = (crate::mapbake::H as f32 * crate::mapbake::CELL / cell).ceil() as usize + 1;
        let key = |x: f32, y: f32| -> Option<usize> {
            let cx = ((x - crate::mapbake::X0) / cell).floor();
            let cy = ((crate::mapbake::Y1 - y) / cell).floor();
            (cx >= 0.0 && cy >= 0.0 && (cx as usize) < nx && (cy as usize) < ny).then(|| cy as usize * nx + cx as usize)
        };
        let mut start = vec![0u32; nx * ny + 1];
        for &(x, y, _) in pts {
            if let Some(k) = key(x, y) {
                start[k + 1] += 1;
            }
        }
        for k in 0..nx * ny {
            start[k + 1] += start[k];
        }
        let mut fill = start.clone();
        let mut items = vec![0u32; start[nx * ny] as usize];
        for &(x, y, i) in pts {
            if let Some(k) = key(x, y) {
                items[fill[k] as usize] = i;
                fill[k] += 1;
            }
        }
        CsrGrid { cell, nx, ny, start, items }
    }

    fn query(&self, x0: f32, y0: f32, x1: f32, y1: f32, mut f: impl FnMut(u32)) {
        let cl = |v: f32, n: usize| ((v.floor().max(0.0)) as usize).min(n - 1);
        let cx0 = cl((x0 - crate::mapbake::X0) / self.cell, self.nx);
        let cx1 = cl((x1 - crate::mapbake::X0) / self.cell, self.nx);
        let cy0 = cl((crate::mapbake::Y1 - y1) / self.cell, self.ny);
        let cy1 = cl((crate::mapbake::Y1 - y0) / self.cell, self.ny);
        for cy in cy0..=cy1 {
            let row = cy * self.nx;
            for &i in &self.items[self.start[row + cx0] as usize..self.start[row + cx1 + 1] as usize] {
                f(i);
            }
        }
    }
}

/// Texture size cap of a model's first load (sharper copies follow).
const FIRST_TEX: u32 = 64;

/// Upper bounds of the entity LOD-distance classes (m, at game scale).
const DIST_CLASSES: [f32; 7] = [50.0, 100.0, 200.0, 400.0, 800.0, 1600.0, f32::INFINITY];
/// Upper bounds of the bbox-radius classes (m).
const SIZE_CLASSES: [f32; 4] = [2.0, 8.0, 32.0, f32::INFINITY];

/// Entities of one LOD level, LOD-distance class and size class, so that a
/// visibility update only visits entities that can be in range.
struct Bucket {
    level: usize,
    /// Index into DIST_CLASSES, DIST_CLASSES.len() = no own distance (LOD band),
    /// usize::MAX = linked into a LOD hierarchy (always queried to the far limit).
    dist: usize,
    size: usize,
    count: usize,
    grid: CsrGrid,
}

impl Bucket {
    /// Query radius around the eye for the current detail settings.
    fn radius(&self, d: &Detail) -> f32 {
        let rmax = d.radii[5];
        let k = d.radii[0] / 330.0;
        let lim = if self.dist == usize::MAX {
            rmax
        } else if self.dist < DIST_CLASSES.len() {
            DIST_CLASSES[self.dist] * k
        } else if self.level == 0 {
            d.radii[0].max(if d.hd_fallback { d.radii[1].min(d.radii[0] * 1.75) } else { 0.0 })
        } else {
            d.radii[self.level]
        };
        let size_lim = if self.dist == usize::MAX || d.min_size <= 0.0 { f32::INFINITY } else { SIZE_CLASSES[self.size] / d.min_size };
        // bbox centres sit up to one radius from the placement origin
        let margin = if self.size + 1 < SIZE_CLASSES.len() { SIZE_CLASSES[self.size] } else { 600.0 };
        lim.min(size_lim).min(rmax) + margin
    }
}

fn hash32(i: u32) -> u32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}

/// Export (if needed) and load one model by entry id ("levels.rpf@<hex>").
/// Loader timing totals (µs): index lookup, export, GLB + texture load; then counts.
static LOAD_T: [std::sync::atomic::AtomicU64; 5] = [const { std::sync::atomic::AtomicU64::new(0) }; 5];

/// Archive id -> (entry offset -> entry index), built once per archive.
/// Also caches resolved archives (name -> info): resolving lists the game
/// folder each time.
type OffMaps = Arc<Mutex<HashMap<String, Arc<HashMap<u64, usize>>>>>;
type ArcCache = Arc<Mutex<HashMap<String, (sixng_core::game::ArchiveInfo, Arc<ArchiveIndex>)>>>;

/// Merge the meshes of a model that share a material (and are not skinned)
/// into one, so each material is one draw call.
fn merge_meshes(mut m: glb::Model) -> glb::Model {
    if m.meshes.len() < 2 {
        return m;
    }
    let mut out: Vec<glb::Mesh> = Vec::new();
    let mut slot: HashMap<(Option<usize>, bool, u32), usize> = HashMap::new();
    for me in std::mem::take(&mut m.meshes) {
        if me.skinned {
            out.push(me);
            continue;
        }
        let key = (me.material, me.has_tangents, me.uv_sets as u32);
        match slot.get(&key) {
            Some(&k) if out[k].verts.len() + me.verts.len() < (1 << 24) => {
                let o = &mut out[k];
                let base = o.verts.len() as u32;
                let (n0, n1) = (o.verts.len() as f32, me.verts.len() as f32);
                let c0 = Vec3::from(o.centre);
                let c1 = Vec3::from(me.centre);
                o.centre = ((c0 * n0 + c1 * n1) / (n0 + n1).max(1.0)).into();
                o.verts.extend_from_slice(&me.verts);
                o.indices.extend(me.indices.iter().map(|i| i + base));
            }
            _ => {
                slot.insert(key, out.len());
                out.push(me);
            }
        }
    }
    m.meshes = out;
    m
}

fn load_model(ws: &Workspace, cache: &Path, id: &str, lod: usize, opts: &glb::LoadOpts, idx: &Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>, offs: &OffMaps, arcs: &ArcCache) -> anyhow::Result<glb::Model> {
    let t0 = Instant::now();
    let add = |k: usize, t: Instant| LOAD_T[k].fetch_add(t.elapsed().as_micros() as u64, Ordering::Relaxed);
    let (archive, off) = id.split_once('@').ok_or_else(|| anyhow::anyhow!("bad model id {id}"))?;
    let got = arcs.lock().unwrap().get(archive).cloned();
    let (a, ix) = match got {
        Some(v) => v,
        None => {
            let v = model_view::index_for(ws, archive, idx)?;
            arcs.lock().unwrap().insert(archive.to_string(), v.clone());
            v
        }
    };
    let off = u64::from_str_radix(off.trim_start_matches("0x"), 16)?;
    let map = {
        let got = offs.lock().unwrap().get(&a.id).cloned();
        match got {
            Some(m) => m,
            None => {
                let m = Arc::new(ix.entries.iter().enumerate().map(|(i, e)| (e.off, i)).collect::<HashMap<u64, usize>>());
                offs.lock().unwrap().insert(a.id.clone(), m.clone());
                m
            }
        }
    };
    let e = map.get(&off).map(|&i| &ix.entries[i]).ok_or_else(|| anyhow::anyhow!("no entry {id}"))?;
    let dir = cache.join("models").join(sixng_core::game::index_stem(&a.id)).join(format!("{}_{:x}", model_view::export_sanitize(e.name.as_deref().unwrap_or("unnamed")), e.off));
    add(0, t0);
    if !dir.join("model.glb").exists() {
        let t1 = Instant::now();
        let eo = ExportOptions { out: cache.to_path_buf(), png: !opts.no_textures, dds: false, raw: false, force: false, skip_embedded_textures: false };
        let _ = sixng_core::export::export_entry(&a, e, &eo)?;
        add(1, t1);
        LOAD_T[4].fetch_add(1, Ordering::Relaxed);
    }
    let lods = model_view::lods_in(&dir);
    if lods.is_empty() {
        anyhow::bail!("{id} has no model.glb");
    }
    let t2 = Instant::now();
    let r = glb::load_with(&lods[lod.min(lods.len() - 1)], cache, opts);
    add(2, t2);
    LOAD_T[3].fetch_add(1, Ordering::Relaxed);
    r
}
