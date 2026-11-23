//! Whole-map far field for the 3D map.
//!
//! A quadtree of heightfield tiles over the baked map box, so the whole map
//! is always on screen out to the horizon while the placed models stream in
//! near the camera.
//!
//! * Heights come from the baked top collision surface at 8 m
//!   (`heights_8m.bin`, already in memory with the map data), as two
//!   pyramids: **roof** (max per cell: buildings and bridges stay as blocks,
//!   used in the distance) and **ground** (a morphological opening removes
//!   buildings; used under the streamed models near the camera).
//! * Node (l, x, y) spans 32768 m / 2^l from the map's north-west corner.
//!   Levels 0-6 have 64 x 64 cells (8 m cells at level 6); levels 7 and 8
//!   split the 8 m data into smaller nodes so the roof/ground switch follows
//!   the model range closely.
//! * The colour is the baked 2D map tile of the same footprint
//!   (`tiles/terrain/<z>/<x>_<y>.png`, z = min(l, 6)), so coarse nodes come
//!   with coarse tiles and refining a node sharpens its texture.
//! * Each frame nodes are refined by screen-space error (cell size over
//!   distance, in pixels) and frustum, children replace a parent only when
//!   all four are loaded, so nothing pops out. Worker threads build meshes
//!   and decode tiles; the least recently used nodes are evicted past a
//!   budget.
use crate::glb;
use crate::mapbake::{CELL, COARSE, H, W, X0, Y1};
use crate::render3d::{Camera, GpuModel, Instance, Renderer3D};
#[cfg(all(target_os = "macos", feature = "metal4-poc"))]
use crate::render3d_metal4::{GpuModel4, Renderer3D4};
use eframe::egui_wgpu::{self, wgpu};
use glam::{Mat4, Vec3, Vec4};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock};

/// Root node size (m): level 0 covers the whole map.
pub const ROOT: f32 = 32768.0;
pub const MAX_LEVEL: u8 = 8;
/// Level whose cells are the 8 m source cells.
const DATA_LEVEL: u8 = 6;
const N: usize = 64;
/// Heights sit this far under the surface so placed models win the depth test.
const SINK: f32 = 1.5;
/// Ground nodes (coverage pass only) sit this far above the ground estimate.
const GROUND_LIFT: f32 = 2.0;
const WATER_Z: f32 = -2.0;
/// The map tiles' water colour (40, 80, 140 sRGB), linear.
const SEA: [f32; 4] = [0.021, 0.080, 0.262, 1.0];

#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug, PartialOrd, Ord)]
pub struct Key {
    pub l: u8,
    pub x: u32,
    pub y: u32,
    pub ground: bool,
}

impl Key {
    fn node(l: u8, x: u32, y: u32) -> Key {
        Key {
            l,
            x,
            y,
            ground: false,
        }
    }
    fn span(&self) -> f32 {
        ROOT / (1u32 << self.l) as f32
    }
    /// (x min, y min, x max, y max), RAGE metres.
    pub fn rect(&self) -> [f32; 4] {
        let s = self.span();
        let x0 = X0 + self.x as f32 * s;
        let y1 = Y1 - self.y as f32 * s;
        [x0, y1 - s, x0 + s, y1]
    }
    /// Mesh cell size (m).
    fn cell(&self) -> f32 {
        self.span() / self.cells() as f32
    }
    fn cells(&self) -> usize {
        if self.l <= DATA_LEVEL {
            N
        } else {
            N >> (self.l - DATA_LEVEL)
        }
    }
    fn children(&self) -> [Key; 4] {
        let (l, x, y) = (self.l + 1, self.x * 2, self.y * 2);
        [
            Key {
                l,
                x,
                y,
                ground: self.ground,
            },
            Key {
                l,
                x: x + 1,
                y,
                ground: self.ground,
            },
            Key {
                l,
                x,
                y: y + 1,
                ground: self.ground,
            },
            Key {
                l,
                x: x + 1,
                y: y + 1,
                ground: self.ground,
            },
        ]
    }
    fn with(&self, ground: bool) -> Key {
        Key { ground, ..*self }
    }
    /// Map tile (z, x, y) and the node's sub-rectangle in it (u0, v0, size).
    fn tile(&self) -> ((u8, u32, u32), [f32; 3]) {
        if self.l <= DATA_LEVEL {
            ((self.l, self.x, self.y), [0.0, 0.0, 1.0])
        } else {
            let sh = self.l - DATA_LEVEL;
            let n = 1u32 << sh;
            let f = 1.0 / n as f32;
            (
                (DATA_LEVEL, self.x >> sh, self.y >> sh),
                [(self.x % n) as f32 * f, (self.y % n) as f32 * f, f],
            )
        }
    }
}

/// One pyramid level: heights per cell (row 0 = north). NaN = water / no
/// data, +inf = outside the baked box.
struct Level {
    nx: usize,
    ny: usize,
    h: Vec<f32>,
}

impl Level {
    fn get(&self, c: i64, r: i64) -> f32 {
        if c < 0 || r < 0 || c as usize >= self.nx || r as usize >= self.ny {
            f32::INFINITY
        } else {
            self.h[r as usize * self.nx + c as usize]
        }
    }
    /// Clamped to the grid (for normals at edges).
    fn get_clamped(&self, c: i64, r: i64) -> f32 {
        self.h[(r.clamp(0, self.ny as i64 - 1) as usize) * self.nx
            + c.clamp(0, self.nx as i64 - 1) as usize]
    }
    fn down(&self, max: bool) -> Level {
        let (nx, ny) = (self.nx.div_ceil(2), self.ny.div_ceil(2));
        let mut h = vec![f32::INFINITY; nx * ny];
        for r in 0..ny {
            for c in 0..nx {
                let (mut acc, mut n, mut any_in) = (if max { f32::MIN } else { 0.0 }, 0, false);
                for (dc, dr) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let v = self.get((c * 2 + dc) as i64, (r * 2 + dr) as i64);
                    if v.is_infinite() {
                        continue;
                    }
                    any_in = true;
                    if v.is_nan() {
                        continue;
                    }
                    acc = if max { acc.max(v) } else { acc + v };
                    n += 1;
                }
                h[r * nx + c] = if !any_in {
                    f32::INFINITY
                } else if n == 0 {
                    f32::NAN
                } else if max {
                    // between max and mean: towers stay, blocks do not swell
                    let mut s = 0.0;
                    for (dc, dr) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let v = self.get((c * 2 + dc) as i64, (r * 2 + dr) as i64);
                        if v.is_finite() {
                            s += v;
                        }
                    }
                    0.5 * (acc + s / n as f32)
                } else {
                    acc / n as f32
                };
            }
        }
        Level { nx, ny, h }
    }
}

/// Height pyramids, index = node level 0..=6 (cell = 8 m at level 6).
pub struct Pyramids {
    roof: Vec<Level>,
    ground: Vec<Level>,
}

impl Pyramids {
    pub fn build(h8: &[f32]) -> Pyramids {
        let (nx, ny) = (W / COARSE, H / COARSE);
        let roof6 = Level {
            nx,
            ny,
            h: h8.to_vec(),
        };
        // Opening (min then max over ~100 m) removes buildings and keeps
        // hills; a 3x3 mean smooths the steps. Water stays water.
        const R: usize = 6;
        let filt = |src: &[f32], take_min: bool| -> Vec<f32> {
            let val = |v: f32| if v.is_nan() { WATER_Z } else { v };
            let mut tmp = vec![0.0f32; nx * ny];
            for r in 0..ny {
                for c in 0..nx {
                    let mut v = if take_min { f32::MAX } else { f32::MIN };
                    for k in c.saturating_sub(R)..=(c + R).min(nx - 1) {
                        let s = val(src[r * nx + k]);
                        v = if take_min { v.min(s) } else { v.max(s) };
                    }
                    tmp[r * nx + c] = v;
                }
            }
            let mut out = vec![0.0f32; nx * ny];
            for r in 0..ny {
                for c in 0..nx {
                    let mut v = if take_min { f32::MAX } else { f32::MIN };
                    for k in r.saturating_sub(R)..=(r + R).min(ny - 1) {
                        let s = tmp[k * nx + c];
                        v = if take_min { v.min(s) } else { v.max(s) };
                    }
                    out[r * nx + c] = v;
                }
            }
            out
        };
        let opened = filt(&filt(h8, true), false);
        let mut g = opened.clone();
        for r in 1..ny - 1 {
            for c in 1..nx - 1 {
                let mut s = 0.0;
                for dr in 0..3 {
                    for dc in 0..3 {
                        s += opened[(r + dr - 1) * nx + c + dc - 1];
                    }
                }
                g[r * nx + c] = s / 9.0;
            }
        }
        for (v, src) in g.iter_mut().zip(h8) {
            if src.is_nan() {
                *v = f32::NAN;
            }
        }
        let ground6 = Level { nx, ny, h: g };
        let mut roof = vec![roof6];
        let mut ground = vec![ground6];
        for _ in 0..DATA_LEVEL {
            let r = roof.last().unwrap().down(true);
            let gl = ground.last().unwrap().down(false);
            roof.push(r);
            ground.push(gl);
        }
        roof.reverse();
        ground.reverse();
        Pyramids { roof, ground }
    }

    /// Highest roof height inside a node (for culling and distances).
    fn node_zmax(&self, k: Key) -> f32 {
        let lvl = &self.roof[k.l.min(DATA_LEVEL) as usize];
        let n = k.cells() as i64;
        let (c0, r0) = (k.x as i64 * n, k.y as i64 * n);
        let mut m = WATER_Z;
        for r in r0..r0 + n {
            for c in c0..c0 + n {
                let v = lvl.get(c, r);
                if v.is_finite() {
                    m = m.max(v);
                }
            }
        }
        m
    }

    /// Whether any cell of the node is inside the baked box.
    fn node_inside(&self, k: Key) -> bool {
        let lvl = &self.roof[k.l.min(DATA_LEVEL) as usize];
        let n = k.cells();
        (k.x as usize * n) < lvl.nx && (k.y as usize * n) < lvl.ny
    }
}

/// RAGE (Z-up) -> renderer (Y-up).
fn gl(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[2], -p[1]]
}

fn node_model(
    py: &Pyramids,
    k: Key,
    tiles: &PathBuf,
    tex_keys: &Mutex<HashSet<String>>,
) -> glb::Model {
    let lvl = if k.ground {
        &py.ground[k.l.min(DATA_LEVEL) as usize]
    } else {
        &py.roof[k.l.min(DATA_LEVEL) as usize]
    };
    let n = k.cells();
    let cs = k.cell();
    let (c0, r0) = (k.x as i64 * n as i64, k.y as i64 * n as i64);
    let ((tz, tx, ty), [u0, v0, us]) = k.tile();
    let mut verts = Vec::with_capacity((n + 1) * (n + 1) + 4 * (n + 1));
    let mut inside = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..=n {
        for i in 0..=n {
            let (c, r) = (c0 + i as i64, r0 + j as i64);
            let raw = lvl.get(c, r);
            inside.push(raw.is_finite() || raw.is_nan());
            // ground nodes are lifted by GROUND_LIFT when drawn; water stays under the sea plane
            let z = if raw.is_finite() {
                raw - SINK
            } else {
                WATER_Z - SINK - if k.ground { GROUND_LIFT } else { 0.0 }
            };
            let hz = |dc: i64, dr: i64| {
                let v = lvl.get_clamped(c + dc, r + dr);
                if v.is_finite() {
                    v
                } else {
                    WATER_Z
                }
            };
            let zx = hz(1, 0) - hz(-1, 0);
            let zy = hz(0, -1) - hz(0, 1); // rows go south
            let nr = Vec3::new(-zx, -zy, 2.0 * cs).normalize();
            let x = X0 + (c as f32 + 0.5) * cs;
            let y = Y1 - (r as f32 + 0.5) * cs;
            let u = u0 + (i as f32 + 0.5) / n as f32 * us;
            let v = v0 + (j as f32 + 0.5) / n as f32 * us;
            verts.push(glb::Vertex {
                pos: gl([x, y, z]),
                nrm: gl([nr.x, nr.y, nr.z]),
                tan: [1.0, 0.0, 0.0, 1.0],
                uv0: [u, v],
                uv1: [0.0, 0.0],
                col: [255, 255, 255, 255],
                joints: [0; 4],
                weights: [0; 4],
                tint: [255; 4],
            });
        }
    }
    let w = n + 1;
    let mut idx: Vec<u32> = Vec::with_capacity(n * n * 6);
    for j in 0..n {
        for i in 0..n {
            let a = j * w + i;
            let (b, c, d) = (a + 1, a + w, a + w + 1);
            if inside[a] && inside[b] && inside[c] {
                idx.extend_from_slice(&[a as u32, b as u32, c as u32]);
            }
            if inside[b] && inside[d] && inside[c] {
                idx.extend_from_slice(&[b as u32, d as u32, c as u32]);
            }
        }
    }
    // Skirts hide cracks against coarser neighbours.
    let depth = cs * 1.5 + 8.0;
    let skirt = |edge: Vec<usize>, flip: bool, verts: &mut Vec<glb::Vertex>, idx: &mut Vec<u32>| {
        for p in edge.windows(2) {
            let (a, b) = (p[0], p[1]);
            if !(inside[a] && inside[b]) {
                continue;
            }
            let base = verts.len() as u32;
            for &s in &[a, b] {
                let mut v = verts[s];
                v.pos[1] -= depth;
                verts.push(v);
            }
            let (a, b) = (a as u32, b as u32);
            if flip {
                idx.extend_from_slice(&[a, base, b, b, base, base + 1]);
            } else {
                idx.extend_from_slice(&[a, b, base, b, base + 1, base]);
            }
        }
    };
    skirt((0..w).collect(), false, &mut verts, &mut idx);
    skirt(
        (0..w).map(|i| n * w + i).collect(),
        true,
        &mut verts,
        &mut idx,
    );
    skirt((0..w).map(|j| j * w).collect(), true, &mut verts, &mut idx);
    skirt(
        (0..w).map(|j| j * w + n).collect(),
        false,
        &mut verts,
        &mut idx,
    );

    // tile texture (shared by key; decoded only when not yet on the GPU)
    let key = format!("world/{tz}/{tx}_{ty}");
    let path = tiles.join(format!("{tz}/{tx}_{ty}.png"));
    let mut images = Vec::new();
    let mut base = None;
    if tex_keys.lock().unwrap().contains(&key) {
        images.push(glb::Image {
            key: key.clone(),
            mips: Vec::new(),
            srgb: true,
            bc: None,
        });
        base = Some(0);
    } else if let Ok(img) = image::open(&path) {
        let img = img.to_rgba8();
        let (iw, ih) = img.dimensions();
        images.push(glb::Image {
            key: key.clone(),
            mips: glb::mip_chain(iw, ih, img.into_raw()),
            srgb: true,
            bc: None,
        });
        base = Some(0);
    }
    let base_factor = if base.is_some() {
        [1.0, 1.0, 1.0, 1.0]
    } else {
        SEA
    };
    let rect = k.rect();
    let centre = gl([(rect[0] + rect[2]) * 0.5, (rect[1] + rect[3]) * 0.5, 0.0]);
    // counter-clockwise seen from above: the coverage pass culls back faces
    for t in idx.chunks_exact_mut(3) {
        t.swap(1, 2);
    }
    let tris = idx.len() / 3;
    let nv = verts.len();
    let mesh = glb::Mesh {
        name: format!("world {}/{}/{}", k.l, k.x, k.y),
        verts,
        indices: idx,
        material: Some(0),
        skinned: false,
        has_tangents: false,
        uv_sets: 1,
        centre,
    };
    let mat = glb::Material {
        name: "world".into(),
        class: "map_tile".into(),
        alpha_mode: "OPAQUE".into(),
        base_factor,
        base,
        normal: None,
        packed: None,
        emissive: None,
        mr: None,
        tint_mask: false,
        parts: false,
        slots: vec![],
        gap: "ok".into(),
        shader_hash: String::new(),
        decal: None,
    };
    glb::Model {
        path: PathBuf::new(),
        meshes: vec![mesh],
        materials: vec![mat],
        joints: vec![],
        images,
        bbox: [[rect[0], -10.0, -rect[3]], [rect[2], 500.0, -rect[1]]],
        triangles: tris,
        vertices: nv,
        extras: Default::default(),
    }
}

struct Shared {
    queue: Mutex<Vec<(Key, f32)>>,
    cv: Condvar,
    inflight: Mutex<HashSet<Key>>,
    stop: AtomicBool,
    py: OnceLock<Arc<Pyramids>>,
}

struct Node {
    gm: GpuModel,
    used: u64,
    tex: String,
    bytes: u64,
}

#[cfg(all(target_os = "macos", feature = "metal4-poc"))]
struct Node4 {
    gm: GpuModel4,
    used: u64,
    tex: String,
    bytes: u64,
}

/// Inputs for one selection pass.
pub struct View<'a> {
    pub cam: &'a Camera,
    pub aspect: f32,
    pub viewport_h: f32,
    /// Screen-space error target (px): smaller = finer.
    pub tau: f32,
    /// Nodes entirely inside this distance (m) use the ground heights (the
    /// streamed models stand on them) once `ground_ok` says their models
    /// are ready; everything else uses the roof heights.
    pub ground_r: f32,
    pub ground_ok: &'a dyn Fn([f32; 4]) -> bool,
}

#[derive(Default, Clone)]
pub struct Stats {
    pub drawn: usize,
    pub resident: usize,
    pub queued: usize,
    pub tris: usize,
    pub gpu_bytes: u64,
    pub max_level: u8,
    pub ground: usize,
}

pub struct World {
    shared: Arc<Shared>,
    rx: Option<mpsc::Receiver<(Key, glb::Model)>>,
    nodes: HashMap<Key, Node>,
    tex_cache: HashMap<String, wgpu::Texture>,
    tex_refs: HashMap<String, u32>,
    #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
    nodes4: HashMap<Key, Node4>,
    #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
    tex_cache4: HashMap<String, metal_rust::metal::Texture>,
    #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
    tex_refs4: HashMap<String, u32>,
    tex_keys: Arc<Mutex<HashSet<String>>>,
    zmax: HashMap<(u8, u32, u32), f32>,
    selected: Vec<Key>,
    ocean: Option<GpuModel>,
    #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
    ocean4: Option<GpuModel4>,
    frame: u64,
    /// Node budget (resident), from the quality setting.
    pub max_nodes: usize,
    pub stats: Stats,
    started: bool,
    tiles: PathBuf,
}

impl World {
    pub fn new(map_dir: PathBuf) -> World {
        World {
            shared: Arc::new(Shared {
                queue: Mutex::new(Vec::new()),
                cv: Condvar::new(),
                inflight: Mutex::new(HashSet::new()),
                stop: AtomicBool::new(false),
                py: OnceLock::new(),
            }),
            rx: None,
            nodes: HashMap::new(),
            tex_cache: HashMap::new(),
            tex_refs: HashMap::new(),
            #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
            nodes4: HashMap::new(),
            #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
            tex_cache4: HashMap::new(),
            #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
            tex_refs4: HashMap::new(),
            tex_keys: Arc::new(Mutex::new(HashSet::new())),
            zmax: HashMap::new(),
            selected: Vec::new(),
            ocean: None,
            #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
            ocean4: None,
            frame: 0,
            max_nodes: 700,
            stats: Stats::default(),
            started: false,
            tiles: map_dir.join("tiles").join("terrain"),
        }
    }

    /// Pyramids built and nothing queued or loading.
    pub fn idle(&self) -> bool {
        self.ready()
            && self.stats.drawn > 0
            && self.shared.queue.lock().unwrap().is_empty()
            && self.shared.inflight.lock().unwrap().is_empty()
    }

    pub fn ready(&self) -> bool {
        self.shared.py.get().is_some()
    }

    /// Start the pyramid build and the workers (once).
    pub fn start(&mut self, h8: Arc<Vec<f32>>) {
        if self.started {
            return;
        }
        self.started = true;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        {
            let sh = self.shared.clone();
            std::thread::spawn(move || {
                let t0 = std::time::Instant::now();
                let py = Pyramids::build(&h8);
                let _ = sh.py.set(Arc::new(py));
                eprintln!(
                    "[world] height pyramids in {:.0} ms",
                    t0.elapsed().as_secs_f32() * 1000.0
                );
                sh.cv.notify_all();
            });
        }
        for _ in 0..2 {
            let sh = self.shared.clone();
            let tx = tx.clone();
            let tiles = self.tiles.clone();
            let keys = self.tex_keys.clone();
            std::thread::spawn(move || loop {
                crate::mem::lower_thread_priority();
                let key = {
                    let mut q = sh.queue.lock().unwrap();
                    loop {
                        if sh.stop.load(Ordering::Relaxed) {
                            return;
                        }
                        if !q.is_empty() && sh.py.get().is_some() {
                            let (bi, _) = q.iter().enumerate().fold((0, f32::MAX), |a, (i, r)| {
                                if r.1 < a.1 {
                                    (i, r.1)
                                } else {
                                    a
                                }
                            });
                            let k = q.swap_remove(bi).0;
                            sh.inflight.lock().unwrap().insert(k);
                            break k;
                        }
                        q = sh.cv.wait(q).unwrap();
                    }
                };
                let py = sh.py.get().unwrap().clone();
                let m = node_model(&py, key, &tiles, &keys);
                if tx.send((key, m)).is_err() {
                    return;
                }
            });
        }
    }

    fn zmax(&mut self, k: Key) -> f32 {
        let id = (k.l, k.x, k.y);
        if let Some(v) = self.zmax.get(&id) {
            return *v;
        }
        let v = self.shared.py.get().map_or(400.0, |p| p.node_zmax(k));
        self.zmax.insert(id, v);
        v
    }

    /// Upload finished nodes (time-boxed); returns true if any arrived.
    pub fn poll(&mut self, rs: &egui_wgpu::RenderState, ren: &Renderer3D, budget_ms: f32) -> bool {
        let Some(rx) = self.rx.take() else {
            return false;
        };
        let t0 = std::time::Instant::now();
        let mut any = false;
        while t0.elapsed().as_secs_f32() * 1000.0 < budget_ms {
            let Ok((k, m)) = rx.try_recv() else { break };
            self.shared.inflight.lock().unwrap().remove(&k);
            any = true;
            let tex = m.images.first().map(|i| i.key.clone()).unwrap_or_default();
            let mut gm = ren.upload_shared(rs, &m, &[], Some(&mut self.tex_cache));
            gm.stencil = 0; // far field: coverage fallbacks may draw over it
            if k.ground {
                // Near the camera the ground only fills what the streamed
                // models leave open: it draws in the coverage pass (pixels no
                // real surface covers), lifted a little above its estimate
                // so it also covers ground-level coarse slabs there.
                ren.set_instances(
                    rs,
                    &mut gm,
                    &[Instance::new(
                        glam::Mat4::from_translation(Vec3::Y * (SINK + GROUND_LIFT)),
                        false,
                    )
                    .fallback()],
                );
            }
            if !tex.is_empty() && self.tex_cache.contains_key(&tex) {
                *self.tex_refs.entry(tex.clone()).or_insert(0) += 1;
                self.tex_keys.lock().unwrap().insert(tex.clone());
            }
            let bytes = gm.meshes.iter().map(|g| g.bytes()).sum::<u64>();
            if let Some(old) = self.nodes.insert(
                k,
                Node {
                    gm,
                    used: self.frame,
                    tex,
                    bytes,
                },
            ) {
                self.release_tex(&old.tex);
            }
        }
        self.rx = Some(rx);
        any
    }

    #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
    pub fn poll_mtl4(&mut self, ren: &mut Renderer3D4, budget_ms: f32) -> anyhow::Result<bool> {
        let Some(rx) = self.rx.take() else {
            return Ok(false);
        };
        let t0 = std::time::Instant::now();
        let mut any = false;
        while t0.elapsed().as_secs_f32() * 1000.0 < budget_ms {
            let Ok((k, model)) = rx.try_recv() else {
                break;
            };
            self.shared.inflight.lock().unwrap().remove(&k);
            any = true;
            let tex = model
                .images
                .first()
                .map(|image| image.key.clone())
                .unwrap_or_default();
            let gm = match ren.upload_shared(&model, &mut self.tex_cache4) {
                Ok(gm) => gm,
                Err(error) => {
                    self.rx = Some(rx);
                    return Err(error);
                }
            };
            if !tex.is_empty() && self.tex_cache4.contains_key(&tex) {
                *self.tex_refs4.entry(tex.clone()).or_insert(0) += 1;
                self.tex_keys.lock().unwrap().insert(tex.clone());
            }
            let bytes = gm.buffer_bytes();
            if let Some(old) = self.nodes4.insert(
                k,
                Node4 {
                    gm,
                    used: self.frame,
                    tex,
                    bytes,
                },
            ) {
                if let Err(error) = ren.release_model(&old.gm) {
                    self.rx = Some(rx);
                    return Err(error);
                }
                if let Err(error) = self.release_tex4(&old.tex, ren) {
                    self.rx = Some(rx);
                    return Err(error);
                }
            }
        }
        self.rx = Some(rx);
        Ok(any)
    }

    fn release_tex(&mut self, tex: &str) {
        if tex.is_empty() {
            return;
        }
        if let Some(r) = self.tex_refs.get_mut(tex) {
            *r = r.saturating_sub(1);
            if *r == 0 {
                self.tex_refs.remove(tex);
                self.tex_cache.remove(tex);
                #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
                let cached_by_mtl4 = self.tex_cache4.contains_key(tex);
                #[cfg(not(all(target_os = "macos", feature = "metal4-poc")))]
                let cached_by_mtl4 = false;
                if !cached_by_mtl4 {
                    self.tex_keys.lock().unwrap().remove(tex);
                }
            }
        }
    }

    #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
    fn release_tex4(&mut self, tex: &str, ren: &mut Renderer3D4) -> anyhow::Result<()> {
        if tex.is_empty() {
            return Ok(());
        }
        if let Some(refs) = self.tex_refs4.get_mut(tex) {
            *refs = refs.saturating_sub(1);
            if *refs == 0 {
                self.tex_refs4.remove(tex);
                if let Some(texture) = self.tex_cache4.remove(tex) {
                    ren.release_texture(&texture)?;
                }
                if !self.tex_cache.contains_key(tex) {
                    self.tex_keys.lock().unwrap().remove(tex);
                }
            }
        }
        Ok(())
    }

    /// Pick the nodes to draw this frame and queue the missing ones.
    pub fn select(&mut self, v: &View) {
        self.select_inner(v, false);
    }

    #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
    pub fn select_mtl4(&mut self, v: &View, ren: &mut Renderer3D4) -> anyhow::Result<()> {
        self.select_inner(v, true);
        self.evict_mtl4(ren)
    }

    fn select_inner(&mut self, v: &View, mtl4: bool) {
        self.frame += 1;
        self.selected.clear();
        if !self.ready() {
            return;
        }
        let eye = v.cam.eye();
        let er = [eye.x, -eye.z, eye.y];
        let vp = v.cam.proj(v.aspect) * v.cam.view();
        let k_px = v.viewport_h / (2.0 * (v.cam.fov * 0.5).tan());
        let mut want: Vec<(Key, f32)> = Vec::new();
        let mut out: Vec<Key> = Vec::new();
        let mut ground_n = 0usize;
        let mut max_level = 0u8;
        let mut stack = vec![Key::node(0, 0, 0)];
        while let Some(k) = stack.pop() {
            let py = self.shared.py.get().unwrap().clone();
            if !py.node_inside(k) {
                continue;
            }
            let r = k.rect();
            let zmax = self.zmax(k);
            let zmin = WATER_Z - SINK - k.cell() * 1.5 - 8.0;
            if !in_frustum(&vp, r, zmin, zmax) {
                continue;
            }
            let dx = (r[0] - er[0]).max(er[0] - r[2]).max(0.0);
            let dy = (r[1] - er[1]).max(er[1] - r[3]).max(0.0);
            let dz = (zmin - er[2]).max(er[2] - zmax).max(0.0);
            let near = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0);
            let fx = (r[0] - er[0]).abs().max((r[2] - er[0]).abs());
            let fy = (r[1] - er[1]).abs().max((r[3] - er[1]).abs());
            let far = (fx * fx + fy * fy).sqrt();
            let err_px = k.cell() / near * k_px;
            // the ground/roof boundary is followed closely only where a node
            // is still large on screen (a long boundary far away would need
            // more nodes than the budget)
            let straddles = near < v.ground_r && far > v.ground_r && k.span() / near * k_px > 48.0;
            // below the data level only the ground/roof boundary splits further
            let split =
                (k.l < DATA_LEVEL && err_px > v.tau) || (k.l < MAX_LEVEL && straddles && k.l >= 4);
            let prio = near / k.span();
            let ground = far <= v.ground_r && (v.ground_ok)(r);
            let me = k.with(ground);
            if split {
                let kids: Vec<Key> = k
                    .children()
                    .into_iter()
                    .filter(|c| py.node_inside(*c))
                    .collect();
                // children replace this node only when all are loaded (either variant)
                let all = kids.iter().all(|c| {
                    self.node_loaded(c.with(false), mtl4) || self.node_loaded(c.with(true), mtl4)
                });
                if all {
                    stack.extend(kids);
                    continue;
                }
                for c in &kids {
                    if !self.node_loaded(c.with(false), mtl4)
                        && !self.node_loaded(c.with(true), mtl4)
                    {
                        want.push((c.with(false), prio * 0.5));
                    }
                }
            }
            // draw this node: the wanted variant if loaded, else the other
            let pick = if self.node_loaded(me, mtl4) {
                Some(me)
            } else {
                want.push((me, prio));
                let other = me.with(!ground);
                self.node_loaded(other, mtl4).then_some(other)
            };
            if let Some(p) = pick {
                if p.ground {
                    ground_n += 1;
                }
                max_level = max_level.max(p.l);
                out.push(p);
            }
        }
        for k in &out {
            if mtl4 {
                #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
                if let Some(n) = self.nodes4.get_mut(k) {
                    n.used = self.frame;
                }
            } else if let Some(n) = self.nodes.get_mut(k) {
                n.used = self.frame;
            }
        }
        let inflight = self.shared.inflight.lock().unwrap().clone();
        want.retain(|(k, _)| !inflight.contains(k) && !self.nodes.contains_key(k));
        want.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        want.dedup_by_key(|w| w.0);
        want.truncate(256);
        self.stats.queued = want.len();
        *self.shared.queue.lock().unwrap() = want;
        self.shared.cv.notify_all();
        self.selected = out;
        self.stats.drawn = self.selected.len();
        self.stats.ground = ground_n;
        self.stats.max_level = max_level;
        if !mtl4 {
            self.evict();
        }
        #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
        {
            self.stats.resident = if mtl4 {
                self.nodes4.len()
            } else {
                self.nodes.len()
            };
            self.stats.tris = if mtl4 {
                self.selected
                    .iter()
                    .filter_map(|k| self.nodes4.get(k))
                    .map(|n| {
                        n.gm.meshes
                            .iter()
                            .map(|m| m.n_idx as usize / 3)
                            .sum::<usize>()
                    })
                    .sum()
            } else {
                self.selected
                    .iter()
                    .filter_map(|k| self.nodes.get(k))
                    .map(|n| {
                        n.gm.meshes
                            .iter()
                            .map(|m| m.n_idx as usize / 3)
                            .sum::<usize>()
                    })
                    .sum()
            };
            let (node_bytes, texture_count) = if mtl4 {
                (
                    self.nodes4.values().map(|n| n.bytes).sum::<u64>(),
                    self.tex_cache4.len(),
                )
            } else {
                (
                    self.nodes.values().map(|n| n.bytes).sum::<u64>(),
                    self.tex_cache.len(),
                )
            };
            self.stats.gpu_bytes = node_bytes + texture_count as u64 * 350_000;
        }
        #[cfg(not(all(target_os = "macos", feature = "metal4-poc")))]
        {
            self.stats.resident = self.nodes.len();
            self.stats.tris = self
                .selected
                .iter()
                .filter_map(|k| self.nodes.get(k))
                .map(|n| {
                    n.gm.meshes
                        .iter()
                        .map(|m| m.n_idx as usize / 3)
                        .sum::<usize>()
                })
                .sum();
            let tex_bytes: u64 = self.tex_cache.len() as u64 * 350_000;
            self.stats.gpu_bytes = self.nodes.values().map(|n| n.bytes).sum::<u64>() + tex_bytes;
        }
    }

    fn node_loaded(&self, key: Key, mtl4: bool) -> bool {
        #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
        if mtl4 {
            return self.nodes4.contains_key(&key);
        }
        let _ = mtl4;
        self.nodes.contains_key(&key)
    }

    fn evict(&mut self) {
        if self.nodes.len() <= self.max_nodes {
            return;
        }
        let keep: HashSet<Key> = self.selected.iter().copied().collect();
        let mut old: Vec<(u64, Key)> = self
            .nodes
            .iter()
            .filter(|(k, _)| !keep.contains(k))
            .map(|(k, n)| (n.used, *k))
            .collect();
        old.sort();
        let n = (self.nodes.len() - self.max_nodes).min(old.len());
        for (_, k) in old.into_iter().take(n) {
            if let Some(node) = self.nodes.remove(&k) {
                self.release_tex(&node.tex);
            }
        }
    }

    #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
    fn evict_mtl4(&mut self, ren: &mut Renderer3D4) -> anyhow::Result<()> {
        if self.nodes4.len() > self.max_nodes {
            let keep: HashSet<Key> = self.selected.iter().copied().collect();
            let mut old: Vec<(u64, Key)> = self
                .nodes4
                .iter()
                .filter(|(key, _)| !keep.contains(key))
                .map(|(key, node)| (node.used, *key))
                .collect();
            old.sort();
            let count = (self.nodes4.len() - self.max_nodes).min(old.len());
            for (_, key) in old.into_iter().take(count) {
                if let Some(node) = self.nodes4.remove(&key) {
                    ren.release_model(&node.gm)?;
                    self.release_tex4(&node.tex, ren)?;
                }
            }
        }
        self.stats.resident = self.nodes4.len();
        self.stats.gpu_bytes = self.nodes4.values().map(|node| node.bytes).sum::<u64>()
            + self.tex_cache4.len() as u64 * 350_000;
        Ok(())
    }

    /// The selected nodes plus a sea plane to the horizon.
    pub fn models(&mut self, rs: &egui_wgpu::RenderState, ren: &Renderer3D) -> Vec<&GpuModel> {
        if self.ocean.is_none() {
            let mut o = ren.upload(rs, &ocean_model(), &[]);
            o.stencil = 0;
            self.ocean = Some(o);
        }
        let mut v: Vec<&GpuModel> = self.ocean.iter().collect();
        for k in &self.selected {
            if let Some(n) = self.nodes.get(k) {
                v.push(&n.gm);
            }
        }
        v
    }

    #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
    pub fn models_mtl4(&mut self, ren: &mut Renderer3D4) -> anyhow::Result<Vec<&GpuModel4>> {
        if self.ocean4.is_none() {
            self.ocean4 = Some(ren.upload_shared(&ocean_model(), &mut self.tex_cache4)?);
        }
        let mut models: Vec<&GpuModel4> = self.ocean4.iter().collect();
        for key in &self.selected {
            if let Some(node) = self.nodes4.get(key) {
                models.push(&node.gm);
            }
        }
        Ok(models)
    }
}

impl Drop for World {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.cv.notify_all();
    }
}

/// Conservative test of a node box (RAGE rect, z range) against the view frustum.
fn in_frustum(vp: &Mat4, r: [f32; 4], zmin: f32, zmax: f32) -> bool {
    let mut out = [true; 5];
    for k in 0..8 {
        let x = if k & 1 == 0 { r[0] } else { r[2] };
        let y = if k & 2 == 0 { r[1] } else { r[3] };
        let z = if k & 4 == 0 { zmin } else { zmax };
        let p = *vp * Vec4::new(x, z, -y, 1.0);
        let t = [p.x < -p.w, p.x > p.w, p.y < -p.w, p.y > p.w, p.w < 0.0];
        for i in 0..5 {
            out[i] &= t[i];
        }
    }
    !out.iter().any(|&o| o)
}

fn ocean_model() -> glb::Model {
    let (cx, cy) = (X0 + W as f32 * CELL * 0.5, Y1 - H as f32 * CELL * 0.5);
    let e = 120_000.0;
    let z = WATER_Z - SINK - 0.5;
    let v = |x: f32, y: f32| glb::Vertex {
        pos: gl([x, y, z]),
        nrm: [0.0, 1.0, 0.0],
        tan: [1.0, 0.0, 0.0, 1.0],
        uv0: [0.0, 0.0],
        uv1: [0.0, 0.0],
        col: [255; 4],
        joints: [0; 4],
        weights: [0; 4],
        tint: [255; 4],
    };
    let verts = vec![
        v(cx - e, cy - e),
        v(cx + e, cy - e),
        v(cx - e, cy + e),
        v(cx + e, cy + e),
    ];
    let mesh = glb::Mesh {
        name: "sea".into(),
        verts,
        indices: vec![0, 2, 1, 1, 2, 3],
        material: Some(0),
        skinned: false,
        has_tangents: false,
        uv_sets: 1,
        centre: gl([cx, cy, z]),
    };
    let mat = glb::Material {
        name: "sea".into(),
        class: "map_tile".into(),
        alpha_mode: "OPAQUE".into(),
        base_factor: SEA,
        base: None,
        normal: None,
        packed: None,
        emissive: None,
        mr: None,
        tint_mask: false,
        parts: false,
        slots: vec![],
        gap: "ok".into(),
        shader_hash: String::new(),
        decal: None,
    };
    glb::Model {
        path: PathBuf::new(),
        meshes: vec![mesh],
        materials: vec![mat],
        joints: vec![],
        images: vec![],
        bbox: [[cx - e, z, -(cy + e)], [cx + e, z, -(cy - e)]],
        triangles: 2,
        vertices: 4,
        extras: Default::default(),
    }
}
