//! Runtime side of the map cells (see cells.rs): which cells the 3D map
//! draws, loading and building them on worker threads, and the per-frame
//! list of cell batches in range.
//!
//! The map is split in two rings. Within `NEAR` x the HD band of the camera,
//! placements are drawn one by one (sharp textures, exact LOD hand-off).
//! A cell lying wholly outside that ring is drawn as a cell once it has
//! loaded, and its placements are skipped by the per-placement pass. A
//! cell the camera comes close to keeps drawing until the models of its
//! placements have loaded (no hole while they stream in).
use crate::cells::{self, CellCpu, CellEnt, CellKey};
use crate::glb;
use crate::map2d::MapData;
use crate::map3d::{self, Detail};
use crate::mapbake::{RF_BBOX_UNKNOWN, RF_HIDE};
use crate::mem;
use crate::render3d::CellGpu;
use eframe::egui_wgpu::wgpu;
use glam::Vec3;
use sixng_core::export::Workspace;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Instant;

/// Near ring (fraction of the HD band) drawn per placement.
pub const NEAR: f32 = 0.6;
/// GPU bytes of loaded cells kept before the least recently drawn go.
const GPU_CAP: u64 = 2560 << 20;
/// Longest a cell keeps drawing after the camera came close (s).
const HANDOVER_S: f32 = 6.0;

/// What a cell means for one of its placements in a visibility pass.
pub const USE_NONE: u8 = 0;
/// Drawn by its cell: skip it.
pub const USE_CELL: u8 = 1;
/// Its cell is handing over to the per-placement pass: load, do not draw.
pub const USE_HANDOVER: u8 = 2;

struct Info {
    key: CellKey,
    ents: Arc<Vec<CellEnt>>,
    /// Range covered by its placements (m, default LOD scale).
    lo: f32,
    hi: f32,
}

/// The cell index of one detail setting.
struct Index {
    tag: String,
    cells: Vec<Info>,
    /// Entity -> cell (u32::MAX: drawn per placement only).
    ent_cell: Vec<u32>,
}

enum Slot {
    Loading,
    Ready { gpu: Arc<CellGpu>, used: Instant },
    Failed,
}

struct Job {
    cell: u32,
    gen: u64,
    key: CellKey,
    ents: Arc<Vec<CellEnt>>,
    path: PathBuf,
    prio: f32,
}

struct Shared {
    queue: Mutex<Vec<Job>>,
    cv: Condvar,
    stop: AtomicBool,
    /// Cells a worker has taken and not delivered yet.
    taken: Mutex<HashSet<u32>>,
    /// Cell files on disk (None until listed).
    present: Mutex<Option<HashSet<PathBuf>>>,
    /// Built / read from the cache, and build time (µs).
    built: AtomicU64,
    read: AtomicU64,
    build_us: AtomicU64,
}

/// (generation, cell, GPU cell or None when it failed)
type Done = (u64, u32, Option<CellGpu>);

pub struct Cells {
    /// Device takes BC textures and the view uses default settings.
    pub on: bool,
    index: Option<Arc<Index>>,
    building: Option<(String, mpsc::Receiver<Index>)>,
    gen: u64,
    slots: HashMap<u32, Slot>,
    /// Per cell for this pass: USE_*.
    state: Vec<u8>,
    /// Cells drawn (far, loaded) and handing over (since when).
    drawn: HashSet<u32>,
    handover: HashMap<u32, Instant>,
    /// Handover cells whose placements have all loaded (from the last pass).
    pub done_handover: HashSet<u32>,
    shared: Arc<Shared>,
    rx: Option<mpsc::Receiver<Done>>,
    started: bool,
    pub bytes: u64,
    /// For the stats: cells in the index, loaded, drawn, batches drawn.
    pub n_index: usize,
    pub n_drawn: usize,
    pub n_batches: usize,
}

/// Static [near, far) range of a placement (m, default LOD scale): the
/// per-placement LOD rules of map3d with every model loaded. None when it
/// is never drawn with default settings.
/// Range [lo, hi) of placement `i` at the default LOD scale, and, for a
/// coverage fallback (unlinked LOD, or LOD whose children all lack a model),
/// the nearest distance it fills in at (it draws as a fallback from there to
/// `lo`).
fn static_range(
    data: &MapData,
    cs: &[u32],
    cl: &[u32],
    d: &Detail,
    i: usize,
) -> Option<(f32, f32, Option<f32>)> {
    let n = data.ents.len();
    let k = d.radii[0] / 330.0;
    let rn: Vec<f32> = d.radii.iter().map(|r| r / k).collect();
    let drawable =
        |e: &crate::mapbake::EntRec| !data.name(e.model).is_empty() && e.rflags & RF_HIDE == 0;
    let rad_of = |e: &crate::mapbake::EntRec| -> f32 {
        if e.rflags & RF_BBOX_UNKNOWN != 0 && e.lod > 0 {
            1.0e4
        } else {
            0.5 * ((e.hi[0] - e.lo[0]).powi(2)
                + (e.hi[1] - e.lo[1]).powi(2)
                + (e.hi[2] - e.lo[2]).powi(2))
            .sqrt()
        }
    };
    let band = |e: &crate::mapbake::EntRec, rad: f32| -> (f32, f32) {
        let l = e.lod.min(5) as usize;
        let lo = if l == 0 { 0.0 } else { rn[l - 1] * 0.92 };
        let hi = if l == 0 && d.hd_fallback && rad >= 10.0 {
            rn[1].min(rn[0] * 1.75)
        } else {
            rn[l]
        };
        (lo, hi)
    };
    let outer = |e: &crate::mapbake::EntRec, rad: f32| -> f32 {
        if e.lod_dist > 0.0 {
            e.lod_dist.min(rn[5])
        } else {
            band(e, rad).1
        }
    };
    let e = &data.ents[i];
    if !drawable(e) {
        return None;
    }
    let rad = rad_of(e);
    let has_parent = (e.parent as usize) < n;
    let has_children = cs[i] < cs[i + 1];
    let mut fb: Option<f32> = None;
    let (mut lo, mut hi) = if has_parent || has_children {
        let mut lim = outer(e, rad);
        if has_parent {
            let p = &data.ents[e.parent as usize];
            if drawable(p) {
                if p.child_lod_dist <= 0.0 {
                    return None;
                }
                lim = lim.min(p.child_lod_dist);
            } else if d.hd_fallback {
                lim = lim.max(outer(p, rad_of(p)));
            }
        }
        let lo = if has_children && e.child_lod_dist > 0.0 {
            e.child_lod_dist
        } else {
            0.0
        };
        if lo > 0.0
            && !cl[cs[i] as usize..cs[i + 1] as usize]
                .iter()
                .any(|&c| drawable(&data.ents[c as usize]))
        {
            fb = Some(0.0);
        }
        (lo, lim)
    } else {
        let (lo, hi) = band(e, rad);
        if d.hd_fallback && e.lod == 1 {
            // the large HD placements stand in for this level
            return None;
        }
        if e.lod >= 1 {
            fb = Some(0.0);
        }
        (lo, hi)
    };
    // screen-size cut
    if d.min_size > 0.0 {
        hi = hi.min(rad / d.min_size / k);
    }
    let dens = match e.kind.min(5) {
        2 => d.prop_density,
        3 => d.veg_density,
        _ => 1.0,
    };
    if dens < 1.0 && (map3d::hash32(i as u32) as f32 / u32::MAX as f32) > dens {
        return None;
    }
    if let Some(f) = fb {
        lo = lo.max(0.0);
        return (hi > f).then_some((lo, hi, fb));
    }
    (lo < hi).then_some((lo, hi, None))
}

/// Cell format prefix of the cache directories (older ones are deleted).
const TAG_FORMAT: &str = "v7_";

fn tag_of(d: &Detail) -> String {
    format!(
        "{TAG_FORMAT}{:.0}_{:.4}_{:.2}_{:.2}_{}",
        d.radii[0], d.min_size, d.prop_density, d.veg_density, d.hd_fallback as u8
    )
}

fn build_index(data: &MapData, cs: &[u32], cl: &[u32], d: &Detail, tag: String) -> Index {
    let t0 = Instant::now();
    let k = d.radii[0] / 330.0;
    // placements that can never be outside the near ring when drawn stay per placement
    let near_n = NEAR * 0.85 * 330.0;
    let mut by_cell: HashMap<CellKey, (Vec<u32>, f32, f32)> = HashMap::new();
    for i in 0..data.ents.len() {
        let Some((lo, hi, fb)) = static_range(data, cs, cl, d, i) else {
            continue;
        };
        let lo = fb.unwrap_or(lo);
        if hi <= near_n {
            continue;
        }
        let e = &data.ents[i];
        let key = CellKey::of(e.lod.min(5), e.pos[0], e.pos[1]);
        let c = by_cell.entry(key).or_insert((Vec::new(), f32::MAX, 0.0));
        c.0.push(i as u32);
        c.1 = c.1.min(lo);
        c.2 = c.2.max(hi);
    }
    let mut ent_cell = vec![u32::MAX; data.ents.len()];
    let mut keys: Vec<CellKey> = by_cell.keys().copied().collect();
    keys.sort();
    let mut cells = Vec::with_capacity(keys.len());
    for key in keys {
        let (list, lo, hi) = by_cell.remove(&key).unwrap();
        let ents: Vec<CellEnt> = list
            .iter()
            .map(|&i| {
                ent_cell[i as usize] = cells.len() as u32;
                let e = &data.ents[i as usize];
                let (lo, hi, fb) = static_range(data, cs, cl, d, i as usize).unwrap();
                let (s, c) = e.yaw.sin_cos();
                let lc = [
                    (e.lo[0] + e.hi[0]) * 0.5,
                    (e.lo[1] + e.hi[1]) * 0.5,
                    (e.lo[2] + e.hi[2]) * 0.5,
                ];
                let ctr = [
                    e.pos[0] + lc[0] * c - lc[1] * s,
                    e.pos[1] + lc[0] * s + lc[1] * c,
                    e.pos[2] + lc[2],
                ];
                // about two pixels (1/500 of the distance) where the cell first draws it
                let grid = (fb.unwrap_or(lo) * k).max(NEAR * 0.85 * d.radii[0]) * 0.002;
                CellEnt {
                    model: data.name(e.model).to_string(),
                    pos: e.pos,
                    yaw: e.yaw,
                    ctr,
                    lo,
                    hi,
                    grid,
                    fb,
                }
            })
            .collect();
        cells.push(Info {
            key,
            ents: Arc::new(ents),
            lo,
            hi,
        });
    }
    let n: usize = cells.iter().map(|c| c.ents.len()).sum();
    let mut per = [(0usize, 0usize); 6];
    for c in &cells {
        per[c.key.level as usize].0 += 1;
        per[c.key.level as usize].1 += c.ents.len();
    }
    eprintln!("[map3d] cell index per level (cells, placements): {per:?}");
    eprintln!(
        "[map3d] cell index: {} cells, {} placements in {:.0} ms (lod scale {:.2})",
        cells.len(),
        n,
        t0.elapsed().as_secs_f32() * 1000.0,
        k
    );
    Index {
        tag,
        cells,
        ent_cell,
    }
}

impl Cells {
    pub fn new() -> Self {
        Cells {
            on: false,
            index: None,
            building: None,
            gen: 0,
            slots: HashMap::new(),
            state: Vec::new(),
            drawn: HashSet::new(),
            handover: HashMap::new(),
            done_handover: HashSet::new(),
            shared: Arc::new(Shared {
                queue: Mutex::new(Vec::new()),
                cv: Condvar::new(),
                stop: AtomicBool::new(false),
                taken: Mutex::new(HashSet::new()),
                present: Mutex::new(None),
                built: AtomicU64::new(0),
                read: AtomicU64::new(0),
                build_us: AtomicU64::new(0),
            }),
            rx: None,
            started: false,
            bytes: 0,
            n_index: 0,
            n_drawn: 0,
            n_batches: 0,
        }
    }

    fn dir(work: &str) -> PathBuf {
        Path::new(work).join(crate::GUI_CACHE).join("cells")
    }

    /// Start the cell workers (three at low priority) and their upload thread.
    pub fn start(
        &mut self,
        device: wgpu::Device,
        queue: wgpu::Queue,
        up: crate::render3d::Uploader,
        game: &str,
        work: &str,
    ) {
        if self.started {
            return;
        }
        self.started = true;
        {
            // size of the cell cache (kept under its cap) and its files
            let dir = Self::dir(work);
            cells::CELL_BYTES.store(u64::MAX / 2, Ordering::Relaxed);
            let sh = self.shared.clone();
            std::thread::spawn(move || {
                fn walk(p: &Path, files: &mut HashSet<PathBuf>) -> u64 {
                    let Ok(rd) = std::fs::read_dir(p) else {
                        return 0;
                    };
                    let mut n = 0;
                    for e in rd.filter_map(|e| e.ok()) {
                        let p = e.path();
                        if p.is_dir() {
                            n += walk(&p, files);
                        } else {
                            n += e.metadata().map(|m| m.len()).unwrap_or(0);
                            files.insert(p);
                        }
                    }
                    n
                }
                // cells of an older format are never read again: drop them so
                // they do not hold the cap
                if let Ok(rd) = std::fs::read_dir(&dir) {
                    for e in rd.filter_map(|e| e.ok()) {
                        let name = e.file_name().to_string_lossy().to_string();
                        if e.path().is_dir() && !name.starts_with(TAG_FORMAT) {
                            let _ = std::fs::remove_dir_all(e.path());
                        }
                    }
                }
                let mut files = HashSet::new();
                cells::CELL_BYTES.store(walk(&dir, &mut files), Ordering::Relaxed);
                let mut p = sh.present.lock().unwrap();
                match p.as_mut() {
                    Some(s) => s.extend(files),
                    None => *p = Some(files),
                }
            });
        }
        let (main_tx, rx) = mpsc::channel::<Done>();
        self.rx = Some(rx);
        let (tx, up_rx) = mpsc::channel::<(u64, u32, Option<CellCpu>)>();
        let sh = self.shared.clone();
        std::thread::spawn(move || {
            for (gen, cell, c) in up_rx {
                let g = c.map(|c| up.upload_cell(&device, &queue, &c));
                let r = main_tx.send((gen, cell, g));
                sh.taken.lock().unwrap().remove(&cell);
                if r.is_err() {
                    return;
                }
            }
        });
        let idx = Arc::new(Mutex::new(HashMap::new()));
        let offs: map3d::OffMaps = Arc::new(Mutex::new(HashMap::new()));
        let arcs: map3d::ArcCache = Arc::new(Mutex::new(HashMap::new()));
        for _ in 0..3 {
            let sh = self.shared.clone();
            let tx = tx.clone();
            let (game, work) = (game.to_string(), work.to_string());
            let (idx, offs, arcs) = (idx.clone(), offs.clone(), arcs.clone());
            std::thread::spawn(move || {
                mem::lower_thread_priority();
                let ws = Workspace::new(&game, &work);
                let cache = Path::new(&work).join(crate::GUI_CACHE);
                let opts = glb::LoadOpts {
                    max_tex: 64,
                    skip_keys: None,
                    no_textures: false,
                    hide_placeholders: true,
                    small_cache: true,
                    base_only: true,
                    bc: true,
                };
                // recently loaded models (cells next to each other share many)
                let mut models: HashMap<String, Option<Arc<glb::Model>>> = HashMap::new();
                let mut model_bytes = 0usize;
                loop {
                    let job = {
                        let mut q = sh.queue.lock().unwrap();
                        loop {
                            if sh.stop.load(Ordering::Relaxed) {
                                return;
                            }
                            if !q.is_empty() {
                                let (bi, _) =
                                    q.iter().enumerate().fold((0, f32::MAX), |a, (i, j)| {
                                        if j.prio < a.1 {
                                            (i, j.prio)
                                        } else {
                                            a
                                        }
                                    });
                                let j = q.swap_remove(bi);
                                sh.taken.lock().unwrap().insert(j.cell);
                                break j;
                            }
                            q = sh.cv.wait(q).unwrap();
                        }
                    };
                    let c = match cells::read(&job.path) {
                        Some(c) => {
                            sh.read.fetch_add(1, Ordering::Relaxed);
                            Some(c)
                        }
                        None => {
                            let t = Instant::now();
                            if model_bytes > 384 << 20 {
                                models.clear();
                                model_bytes = 0;
                            }
                            let c = cells::build(job.key, &job.ents, 256, &mut |id, lod| {
                                models
                                    .entry(format!("{id}|{lod}"))
                                    .or_insert_with(|| {
                                        let m = map3d::load_model(
                                            &ws, &cache, id, lod, &opts, &idx, &offs, &arcs,
                                        )
                                        .ok()?;
                                        model_bytes += m
                                            .meshes
                                            .iter()
                                            .map(|me| me.verts.len() * 72 + me.indices.len() * 4)
                                            .sum::<usize>()
                                            + m.images.iter().map(|i| i.bytes()).sum::<usize>();
                                        Some(Arc::new(m))
                                    })
                                    .clone()
                            });
                            sh.build_us
                                .fetch_add(t.elapsed().as_micros() as u64, Ordering::Relaxed);
                            sh.built.fetch_add(1, Ordering::Relaxed);
                            if cells::CELL_BYTES.load(Ordering::Relaxed) < cells::CELL_CAP {
                                if cells::write(&job.path, &c).is_ok() {
                                    sh.present
                                        .lock()
                                        .unwrap()
                                        .get_or_insert_with(HashSet::new)
                                        .insert(job.path.clone());
                                }
                            }
                            Some(c)
                        }
                    };
                    if tx.send((job.gen, job.cell, c)).is_err() {
                        return;
                    }
                }
            });
        }
    }

    /// Make sure the index matches the detail setting (built on a thread).
    fn ensure_index(
        &mut self,
        data: &Arc<MapData>,
        cs: &Arc<(Vec<u32>, Vec<u32>)>,
        d: &Detail,
    ) -> bool {
        let tag = tag_of(d);
        if let Some((t, rx)) = &self.building {
            if let Ok(ix) = rx.try_recv() {
                let ok = *t == tag;
                self.building = None;
                if ok {
                    self.reset();
                    self.n_index = ix.cells.len();
                    self.index = Some(Arc::new(ix));
                }
            }
        }
        if self.index.as_ref().is_some_and(|ix| ix.tag == tag) {
            return true;
        }
        if self.building.as_ref().is_none_or(|(t, _)| *t != tag) {
            let (tx, rx) = mpsc::channel();
            let (data, cs, d, t) = (data.clone(), cs.clone(), d.clone(), tag.clone());
            std::thread::spawn(move || {
                let _ = tx.send(build_index(&data, &cs.0, &cs.1, &d, t));
            });
            self.building = Some((tag, rx));
        }
        // the old index stays out of use until the new one is in
        if self.index.is_some() {
            self.reset();
            self.index = None;
        }
        false
    }

    fn reset(&mut self) {
        self.gen += 1;
        self.slots.clear();
        self.drawn.clear();
        self.handover.clear();
        self.done_handover.clear();
        self.state.clear();
        self.bytes = 0;
        self.shared.queue.lock().unwrap().clear();
    }

    /// Before a visibility pass: decide which cells draw, ask for the ones
    /// missing. `eye` RAGE; `seen(x0, y0, x1, y1)`: rect in the view.
    /// While the camera `moving`, HD cells not on disk yet are not built
    /// (they are the heavy ones: building them would slow the streaming of
    /// the models the camera is flying into).
    pub fn update(
        &mut self,
        data: &Arc<MapData>,
        cs: &Arc<(Vec<u32>, Vec<u32>)>,
        d: &Detail,
        work: &str,
        eye: [f32; 3],
        moving: bool,
        seen: &dyn Fn([f32; 4]) -> bool,
    ) {
        if !self.on || !self.ensure_index(data, cs, d) {
            self.state.clear();
            self.drawn.clear();
            self.handover.clear();
            return;
        }
        let ix = self.index.clone().unwrap();
        let k = d.radii[0] / 330.0;
        let near = NEAR * d.radii[0];
        let dir = Self::dir(work).join(&ix.tag);
        let now = Instant::now();
        // handovers whose placements have all loaded end now
        for c in self.done_handover.drain() {
            self.handover.remove(&c);
        }
        self.handover
            .retain(|_, t| t.elapsed().as_secs_f32() < HANDOVER_S);
        let prev = std::mem::take(&mut self.drawn);
        self.state.clear();
        self.state.resize(ix.cells.len(), USE_NONE);
        let mut jobs = Vec::new();
        // loading marks are set again below for what is still wanted
        self.slots.retain(|_, s| !matches!(s, Slot::Loading));
        let taken = self.shared.taken.lock().unwrap().clone();
        let present = self.shared.present.lock().unwrap();
        let gz = eye[2].abs() + 300.0;
        for (ci, c) in ix.cells.iter().enumerate() {
            let ci = ci as u32;
            let dmin = c.key.dist(eye[0], eye[1]);
            let r = c.key.rect();
            let dx = (eye[0] - r[0]).abs().max((eye[0] - r[2]).abs());
            let dy = (eye[1] - r[1]).abs().max((eye[1] - r[3]).abs());
            let dmax = (dx * dx + dy * dy + gz * gz).sqrt();
            if dmin >= c.hi * k || dmax < c.lo * k {
                // nothing of it in range
                if self.handover.remove(&ci).is_some() {
                    // fall through to per placement
                }
                continue;
            }
            let was = prev.contains(&ci) || self.handover.contains_key(&ci);
            let far = dmin > if was { near * 0.85 } else { near };
            let ready = matches!(self.slots.get(&ci), Some(Slot::Ready { .. }));
            if far {
                self.handover.remove(&ci);
                if ready {
                    self.drawn.insert(ci);
                    self.state[ci as usize] = USE_CELL;
                    if let Some(Slot::Ready { used, .. }) = self.slots.get_mut(&ci) {
                        *used = now;
                    }
                } else {
                    // not loaded yet: the per-placement pass draws (and loads) its placements meanwhile
                    if !self.slots.contains_key(&ci) {
                        self.slots.insert(ci, Slot::Loading);
                    }
                    if matches!(self.slots.get(&ci), Some(Slot::Loading)) && !taken.contains(&ci) {
                        let lvl = c.key.level as f32;
                        let prio =
                            (dmin + 1.0) / (1.0 + 0.5 * lvl) * if seen(r) { 1.0 } else { 3.0 };
                        let path = dir.join(c.key.file_name());
                        let on_disk = present.as_ref().is_some_and(|p| p.contains(&path));
                        if c.key.level == 0 && moving && !on_disk {
                            self.slots.remove(&ci);
                            continue;
                        }
                        jobs.push(Job {
                            cell: ci,
                            gen: self.gen,
                            key: c.key,
                            ents: c.ents.clone(),
                            path,
                            prio,
                        });
                    }
                }
            } else if ready && was {
                self.handover.entry(ci).or_insert(now);
                self.state[ci as usize] = USE_HANDOVER;
                if let Some(Slot::Ready { used, .. }) = self.slots.get_mut(&ci) {
                    *used = now;
                }
            }
        }
        drop(present);
        // the queue holds what is wanted now, nearest first
        {
            let mut q = self.shared.queue.lock().unwrap();
            q.clear();
            jobs.sort_by(|a, b| {
                a.prio
                    .partial_cmp(&b.prio)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            jobs.truncate(512);
            q.extend(jobs);
        }
        self.shared.cv.notify_all();
        self.evict();
    }

    /// What a visibility pass needs (shareable between its threads).
    pub fn view(&self) -> CellView<'_> {
        CellView {
            ent_cell: self.index.as_ref().map_or(&[][..], |ix| &ix.ent_cell[..]),
            state: &self.state,
        }
    }

    /// After the pass: `pending` = per handover cell, placements in range
    /// whose model has not loaded. Returns true when a handover finished
    /// (the next pass draws those placements).
    pub fn after_pass(&mut self, pending: &HashMap<u32, u32>) -> bool {
        let mut any = false;
        for &c in self.handover.keys() {
            if pending.get(&c).copied().unwrap_or(0) == 0 {
                self.done_handover.insert(c);
                any = true;
            }
        }
        any
    }

    /// Book in loaded cells. Returns true if any arrived.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else { return false };
        let mut any = false;
        let t0 = Instant::now();
        while t0.elapsed().as_millis() < 3 {
            let Ok((gen, cell, g)) = rx.try_recv() else {
                break;
            };
            if gen != self.gen {
                continue;
            }
            any = true;
            match g {
                Some(g) => {
                    self.bytes += g.bytes;
                    self.slots.insert(
                        cell,
                        Slot::Ready {
                            gpu: Arc::new(g),
                            used: Instant::now(),
                        },
                    );
                }
                None => {
                    self.slots.insert(cell, Slot::Failed);
                }
            }
        }
        any
    }

    /// Cells queued or being built.
    pub fn busy(&self) -> usize {
        self.shared.queue.lock().unwrap().len() + self.shared.taken.lock().unwrap().len()
    }

    fn evict(&mut self) {
        if self.bytes <= GPU_CAP {
            return;
        }
        let mut ready: Vec<(Instant, u32, u64)> = self
            .slots
            .iter()
            .filter_map(|(&c, s)| match s {
                Slot::Ready { gpu, used }
                    if !self.drawn.contains(&c) && !self.handover.contains_key(&c) =>
                {
                    Some((*used, c, gpu.bytes))
                }
                _ => None,
            })
            .collect();
        ready.sort_by_key(|r| r.0);
        for (_, c, b) in ready {
            if self.bytes <= GPU_CAP * 4 / 5 {
                break;
            }
            self.slots.remove(&c);
            self.bytes -= b;
        }
    }

    /// Cells and their batches in range this frame. `eye` Y-up world.
    pub fn draw_list(
        &mut self,
        eye: Vec3,
        k: f32,
        in_view: &dyn Fn(Vec3, Vec3) -> bool,
    ) -> Vec<(Arc<CellGpu>, Vec<u16>)> {
        let mut out = Vec::new();
        let mut nb = 0;
        for &c in self.drawn.iter().chain(self.handover.keys()) {
            let Some(Slot::Ready { gpu, .. }) = self.slots.get(&c) else {
                continue;
            };
            let gact: Vec<bool> = gpu
                .groups
                .iter()
                .map(|g| {
                    let (lo, hi) = (Vec3::from(g.bb[0]), Vec3::from(g.bb[1]));
                    if lo.x > hi.x {
                        return false;
                    }
                    let near = eye.clamp(lo, hi).distance(eye);
                    let far = (eye - lo).abs().max((eye - hi).abs()).length();
                    near < g.hi * k && far >= g.lo * k && in_view(lo, hi)
                })
                .collect();
            let act: Vec<u16> = gpu
                .batches
                .iter()
                .enumerate()
                .filter(|(_, b)| gact.get(b.group as usize).copied().unwrap_or(false))
                .map(|(i, _)| i as u16)
                .collect();
            if !act.is_empty() {
                nb += act.len();
                out.push((gpu.clone(), act));
            }
        }
        self.n_drawn = out.len();
        self.n_batches = nb;
        out
    }

    /// Stats line: index, loaded, drawn, building, built / read, MB.
    pub fn summary(&self) -> String {
        let built = self.shared.built.load(Ordering::Relaxed);
        format!(
            "cells {} indexed, {} loaded, {} drawn ({} batches), {} loading, {} built ({:.0} ms avg), {} read, {:.0} MB GPU",
            self.n_index,
            self.slots.values().filter(|s| matches!(s, Slot::Ready { .. })).count(),
            self.n_drawn,
            self.n_batches,
            self.busy(),
            built,
            self.shared.build_us.load(Ordering::Relaxed) as f32 / 1000.0 / built.max(1) as f32,
            self.shared.read.load(Ordering::Relaxed),
            self.bytes as f32 / (1 << 20) as f32
        )
    }
}

/// Entity -> cell and cell -> USE_* for one visibility pass.
pub struct CellView<'a> {
    ent_cell: &'a [u32],
    state: &'a [u8],
}

impl CellView<'_> {
    #[inline]
    pub fn use_of(&self, i: u32) -> u8 {
        match self.ent_cell.get(i as usize) {
            Some(&c) if c != u32::MAX => self.state.get(c as usize).copied().unwrap_or(USE_NONE),
            _ => USE_NONE,
        }
    }
    #[inline]
    pub fn cell_of(&self, i: u32) -> u32 {
        self.ent_cell.get(i as usize).copied().unwrap_or(u32::MAX)
    }
}

impl Drop for Cells {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.cv.notify_all();
    }
}
