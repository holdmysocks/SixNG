//! GPU-driven drawing of the 3D map's models: the material table.
//!
//! A city view draws thousands of models, most with several materials, and
//! each material is its own bind group and draw call on the old path. Here
//! the map's static models keep:
//!
//! - their geometry in shared pools of vertex and index buffers;
//! - their instances in a shared instance pool, each carrying the index of
//!   the model's first material in the shared table (`extra.w`);
//! - their materials in one storage buffer (the table): the material
//!   uniform and the slots of its eight textures;
//! - their textures as layers of texture arrays, one set of arrays per
//!   class of texture (format, size, mip count), all bound at once.
//!
//! A vertex carries its material's index within the model (`joints.w`, free
//! on a model without a skeleton), so a model's solid and cut-out meshes are
//! one sub-draw whatever their materials, and the main, coverage-fallback
//! and shadow passes draw every sub-draw of one set of pool buffers with one
//! indirect call. The texture arrays replace the pages of views an earlier
//! try bound: wgpu tracks each bound view every time a bind group is set,
//! and a few hundred arrays cost little where thousands of views cost much.
//!
//! The old path keeps working for the same models: each texture layer also
//! has a 2D view of its own, which the model's own material bind groups
//! take, and the meshes address the pool buffers. Decals and see-through
//! meshes, models with skeletons or strand hair, and models whose textures
//! did not fit an array draw the old way. `SIXNG_MAP_GPU_DRAWS=0` turns the
//! whole path off.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use crate::render3d::Instance;

/// Texture arrays one bind group may hold (array 0 is a white layer).
pub const ARRAYS: u32 = 2048;
/// Materials in the shared table.
const MAT_CAP: u64 = 1 << 16;
/// Frames a freed range or layer waits before it is handed out again (a
/// frame recorded before the free may not be submitted yet).
const HOLD_FRAMES: u64 = 3;
/// Pool chunk sizes (bytes): vertices, indices, instances.
const G_CHUNK: u64 = 128 << 20;
/// Bytes of a vertex.
const VSZ: u64 = std::mem::size_of::<crate::glb::Vertex>() as u64;
const N_CHUNK: u64 = 4 << 20;
/// Layers of one texture array at most, and its bytes at most.
const MAX_LAYERS: u32 = 256;
const ARRAY_BYTES: u64 = 64 << 20;
/// Larger textures stay textures of their own (and their models draw the old way).
const LAYER_MAX: u64 = 16 << 20;
/// Layers of a class's first array; each later one doubles, up to the caps.
const FIRST_LAYERS: u32 = 4;

/// Where the threads that touch the table are (hang diagnosis): one slot
/// per thread kind, each holding an index into `SITES`.
pub mod watch {
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    pub const SITES: [&str; 32] = [
        "-",
        "render: start",
        "render: table prepare",
        "render: table frame",
        "render: shadows",
        "render: main pass",
        "render: blended pass",
        "render: finish",
        "render: submit",
        "render: done",
        "loader: texture layer",
        "loader: texture write",
        "loader: texture own",
        "loader: place",
        "loader: pool write",
        "loader: model done",
        "lock: layer drop",
        "lock: v_bytes",
        "lock: set_instances",
        "lock: handle drop",
        "lock: layer",
        "lock: place",
        "lock: prepare",
        "lock: summary",
        "set_instances: write",
        "set_instances: done",
        "map: take loaded",
        "map: loaded done",
        "map: remove model",
        "map: remove done",
        "lock: compact_from",
        "lock: relocate",
    ];
    /// Thread kinds: render, loader, other (drops, instance writes).
    pub static AT: [AtomicUsize; 3] = [AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0)];
    /// The site that holds the table lock (0: none).
    pub static HOLDER: AtomicUsize = AtomicUsize::new(0);
    pub static FRAMES: AtomicU64 = AtomicU64::new(0);

    pub fn at(kind: usize, site: usize) {
        AT[kind].store(site, Ordering::Relaxed);
    }

    pub fn line() -> String {
        let n = |a: &AtomicUsize| SITES.get(a.load(Ordering::Relaxed)).copied().unwrap_or("?");
        format!("render [{}], loader [{}], other [{}], table lock held by [{}]", n(&AT[0]), n(&AT[1]), n(&AT[2]), n(&HOLDER))
    }

    /// A thread that prints where everyone is once frames stop for 3 s.
    pub fn start() {
        std::thread::spawn(|| {
            let mut last = (0u64, std::time::Instant::now());
            let mut told = 0u32;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let f = FRAMES.load(Ordering::Relaxed);
                if f != last.0 {
                    last = (f, std::time::Instant::now());
                    told = 0;
                } else if f > 0 && last.1.elapsed().as_secs() >= 3 * (told as u64 + 1) && told < 10 {
                    told += 1;
                    eprintln!("[map3d] table watchdog: no frame for {:.0} s after frame {f}: {}", last.1.elapsed().as_secs_f32(), line());
                }
            }
        });
    }
}

/// `SIXNG_MAP_GPU_TABLE`: a comma list of `nodraw` (pools and arrays
/// filled, but every model drawn the old way), `noshadow` (the table skips
/// the shadow cascades), `nomulti` (one indirect call per sub-draw),
/// `noarray` (textures stay in their own images, so nothing joins the
/// pools) and `nopool` (textures go to the arrays, geometry stays in its
/// own buffers and is drawn the old way), `shadowold` (the table's models
/// cast their shadows the old way), `vsfull` (each program loads the
/// whole material row, as before), and for timing only `shadownocut`
/// (cut-out casters without their alpha test) and `shadowmodel` (shadow
/// sub-draws in model order, not grouped by chunk).
pub fn opt(name: &str) -> bool {
    static V: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    V.get_or_init(|| std::env::var("SIXNG_MAP_GPU_TABLE").unwrap_or_default()).split(',').any(|s| s.trim() == name)
}

/// A queued texture write (see Shared::defer).
pub type Deferred = Box<dyn FnOnce(&wgpu::Queue) + Send>;

/// The features the path needs.
pub fn features() -> wgpu::Features {
    wgpu::Features::TEXTURE_BINDING_ARRAY
        | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
        | wgpu::Features::INDIRECT_FIRST_INSTANCE
        | wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY
}

/// One entry of an indexed indirect draw (wgpu's `DrawIndexedIndirectArgs`).
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Args {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub base_vertex: i32,
    pub first_instance: u32,
}
pub const ARGS: u64 = std::mem::size_of::<Args>() as u64;

/// A texture slot in the table: array index and layer.
pub fn slot(array: u32, layer: u32) -> u32 {
    (array << 8) | layer
}

/// Free ranges of a buffer or table (in its own units), sorted, neighbours merged.
struct Free {
    list: Vec<Range<u64>>,
    cap: u64,
}

impl Free {
    fn new(cap: u64) -> Free {
        Free { list: vec![0..cap], cap }
    }
    /// First fit.
    fn alloc(&mut self, n: u64) -> Option<Range<u64>> {
        self.alloc_aligned(n, 1)
    }
    /// First fit, starting at a multiple of `align`.
    fn alloc_aligned(&mut self, n: u64, align: u64) -> Option<Range<u64>> {
        let fits = |r: &Range<u64>| r.start.next_multiple_of(align) + n <= r.end;
        let k = self.list.iter().position(fits)?;
        let r = self.list[k].clone();
        let s = r.start.next_multiple_of(align);
        let out = s..s + n;
        // what is left before and after it
        let mut rest = Vec::with_capacity(2);
        if s > r.start {
            rest.push(r.start..s);
        }
        if out.end < r.end {
            rest.push(out.end..r.end);
        }
        self.list.splice(k..k + 1, rest);
        Some(out)
    }
    /// Whether a range of `n` at a multiple of `align` fits.
    fn fits(&self, n: u64, align: u64) -> bool {
        self.list.iter().any(|r| r.start.next_multiple_of(align) + n <= r.end)
    }
    fn free(&mut self, r: Range<u64>) {
        if r.is_empty() {
            return;
        }
        let k = self.list.partition_point(|x| x.start < r.start);
        let mut r = r;
        if k < self.list.len() && self.list[k].start == r.end {
            r.end = self.list[k].end;
            self.list.remove(k);
        }
        if k > 0 && self.list[k - 1].end == r.start {
            self.list[k - 1].end = r.end;
        } else {
            self.list.insert(k, r);
        }
    }
    fn whole(&self) -> bool {
        self.list.len() == 1 && self.list[0] == (0..self.cap)
    }
    fn used(&self) -> u64 {
        self.cap - self.list.iter().map(|r| r.end - r.start).sum::<u64>()
    }
}

/// Buffers of one kind in chunks, handed out in elements of `unit` bytes.
struct Pool {
    label: &'static str,
    usage: wgpu::BufferUsages,
    unit: u64,
    /// Elements per chunk.
    cap: u64,
    chunks: Vec<Option<(wgpu::Buffer, Free)>>,
}

impl Pool {
    fn new(label: &'static str, usage: wgpu::BufferUsages, unit: u64, bytes: u64, max: u64) -> Pool {
        Pool { label, usage, unit, cap: bytes.min(max) / unit, chunks: Vec::new() }
    }
    fn alloc(&mut self, device: &wgpu::Device, n: u64) -> Option<(usize, Range<u64>)> {
        self.alloc_in(device, n, 1, None, true)
    }
    /// `n` elements at a multiple of `align`, not in chunk `not`; a new
    /// chunk when none has room and `grow`.
    fn alloc_in(&mut self, device: &wgpu::Device, n: u64, align: u64, not: Option<usize>, grow: bool) -> Option<(usize, Range<u64>)> {
        if n == 0 || n + align > self.cap {
            return None;
        }
        // the fullest chunk with room: the emptier ones then drain as models
        // leave (and as `compact` moves them), and go back to the device
        let pick = self
            .chunks
            .iter()
            .enumerate()
            .filter(|(c, _)| Some(*c) != not)
            .filter_map(|(c, ch)| ch.as_ref().filter(|(_, f)| f.fits(n, align)).map(|(_, f)| (f.used(), c)))
            .max();
        if let Some((_, c)) = pick {
            if let Some(Some((_, f))) = self.chunks.get_mut(c) {
                if let Some(r) = f.alloc_aligned(n, align) {
                    return Some((c, r));
                }
            }
        }
        if !grow {
            return None;
        }
        let c = self.new_chunk(device);
        let r = self.chunks[c].as_mut()?.1.alloc_aligned(n, align)?;
        Some((c, r))
    }
    /// A new empty chunk.
    fn new_chunk(&mut self, device: &wgpu::Device) -> usize {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(self.label),
            size: self.cap * self.unit,
            usage: self.usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let f = Free::new(self.cap);
        match self.chunks.iter().position(|c| c.is_none()) {
            Some(c) => {
                self.chunks[c] = Some((buf, f));
                c
            }
            None => {
                self.chunks.push(Some((buf, f)));
                self.chunks.len() - 1
            }
        }
    }
    fn free(&mut self, c: usize, r: Range<u64>) {
        if let Some(Some((_, f))) = self.chunks.get_mut(c) {
            f.free(r);
        }
    }
    fn buf(&self, c: usize) -> Option<wgpu::Buffer> {
        self.chunks.get(c).and_then(|c| c.as_ref()).map(|c| c.0.clone())
    }
    /// Give empty chunks back to the device (the first one stays), except
    /// those with ranges still held.
    fn trim(&mut self, held: &[usize]) {
        for c in 1..self.chunks.len() {
            let empty = self.chunks[c].as_ref().is_some_and(|(_, f)| f.whole());
            if empty && !held.contains(&c) {
                self.chunks[c] = None;
            }
        }
    }
    fn snapshot(&self) -> Vec<Option<wgpu::Buffer>> {
        self.chunks.iter().map(|c| c.as_ref().map(|c| c.0.clone())).collect()
    }
    /// Chunks, bytes allocated, bytes in use.
    fn stats(&self) -> (usize, u64, u64) {
        let live = self.chunks.iter().flatten();
        let n = live.clone().count();
        (n, n as u64 * self.cap * self.unit, live.map(|c| c.1.used() * self.unit).sum())
    }
}

/// Room for a model's geometry in one chunk of the geometry pool: `nv`
/// vertices (at a vertex boundary, so the chunk's first vertex index is
/// exact) and `ni` indices. Fullest chunk first, not chunk `not`, a new one
/// when none has room and `grow`.
fn geo_alloc(p: &mut Pool, device: &wgpu::Device, nv: u64, ni: u64, not: Option<usize>, grow: bool) -> Option<(usize, Range<u64>, Range<u64>)> {
    let (vu, a) = (nv * VSZ / 4, VSZ / 4);
    if vu + a + ni > p.cap {
        return None;
    }
    let mut order: Vec<(u64, usize)> = p
        .chunks
        .iter()
        .enumerate()
        .filter(|(c, _)| Some(*c) != not)
        .filter_map(|(c, ch)| ch.as_ref().filter(|(_, f)| f.fits(vu, a) && f.fits(ni, 1)).map(|(_, f)| (f.used(), c)))
        .collect();
    order.sort_by(|x, y| y.cmp(x));
    for (_, c) in order {
        let f = &mut p.chunks[c].as_mut()?.1;
        if let Some(v) = f.alloc_aligned(vu, a) {
            if let Some(i) = f.alloc(ni) {
                return Some((c, v, i));
            }
            f.free(v);
        }
    }
    if !grow {
        return None;
    }
    let c = p.new_chunk(device);
    let f = &mut p.chunks[c].as_mut()?.1;
    let v = f.alloc_aligned(vu, a)?;
    let i = f.alloc(ni)?;
    Some((c, v, i))
}

/// A class of textures that can share an array.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Class {
    pub format: wgpu::TextureFormat,
    pub w: u32,
    pub h: u32,
    pub mips: u32,
}

impl Class {
    /// Bytes of one layer with its mips.
    pub fn layer_bytes(&self) -> u64 {
        let (bw, bh) = self.format.block_dimensions();
        let bb = self.format.block_copy_size(None).unwrap_or(4) as u64;
        (0..self.mips)
            .map(|l| {
                let (w, h) = ((self.w >> l).max(1), (self.h >> l).max(1));
                w.div_ceil(bw) as u64 * h.div_ceil(bh) as u64 * bb
            })
            .sum()
    }
}

struct Arr {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    class: Class,
    cap: u32,
    free: Vec<u32>,
    /// Bind group generation that first holds this array.
    gen: u64,
}

enum Held {
    G(usize, Range<u64>),
    N(usize, Range<u64>),
    Mats(Range<u64>),
    Layer(u32),
}

struct Inner {
    /// Vertices and indices (one buffer serves as both), in 4-byte units.
    g: Pool,
    n: Pool,
    mats: Free,
    arrays: Vec<Option<Arr>>,
    classes: HashMap<Class, Vec<usize>>,
    frame: u64,
    held: Vec<(u64, Held)>,
    /// Bumped by every new array; `built` is the one the bind group shows.
    gen: u64,
    built: u64,
    bg: Option<wgpu::BindGroup>,
    /// Bind group rebuilds and their time (µs) since the stats were last taken.
    rebuilds: (u32, u64),
    /// Models compact moved and their bytes, since the stats were last taken.
    moved: (u32, u64),
}

/// The shared tables. Created with the map's renderer when the device can
/// index texture arrays per pixel, bind them partly and draw indirect with a
/// first instance.
pub struct Shared {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub bgl: wgpu::BindGroupLayout,
    /// The material table alone (solid shadow casters read no texture).
    pub bgl_mats: wgpu::BindGroupLayout,
    mats_bg: wgpu::BindGroup,
    mats: wgpu::Buffer,
    /// Bytes of one material uniform, and of one table row (uniform + slots).
    mat_bytes: u64,
    row: u64,
    max_layers: u32,
    inner: Mutex<Inner>,
    /// Texture layer writes waiting for the render thread (see defer).
    writes: Mutex<Vec<Deferred>>,
}

/// What a frame draws with.
pub struct Snapshot {
    pub built: u64,
    pub bg: Option<wgpu::BindGroup>,
    pub mats_bg: wgpu::BindGroup,
    pub g: Vec<Option<wgpu::Buffer>>,
    pub n: Vec<Option<wgpu::Buffer>>,
}

impl Snapshot {
    pub fn ready(&self, h: &Handle) -> bool {
        self.bg.is_some()
            && self.built >= h.need
            && h.n.is_some()
            && self.g.get(h.gc).is_some_and(|b| b.is_some())
    }
}

/// A texture's layer in an array; given back when the last holder drops it.
pub struct Layer {
    shared: Arc<Shared>,
    pub slot: u32,
    /// Bind group generation that first shows it.
    pub need: u64,
}

impl Drop for Layer {
    fn drop(&mut self) {
        let mut g = self.shared.lock(16);
        let f = g.frame;
        g.held.push((f, Held::Layer(self.slot)));
    }
}

/// A model's place in the shared pools and table; freed when the model is dropped.
pub struct Handle {
    shared: Arc<Shared>,
    /// Geometry chunk; vertex and index ranges in it (4-byte units).
    pub gc: usize,
    v: Range<u64>,
    i: Range<u64>,
    /// Instance chunk and range (None until the model has instances).
    pub n: Option<(usize, Range<u64>)>,
    pub mid0: u32,
    n_mats: u32,
    /// Bind group generation its textures need.
    pub need: u64,
    /// The geometry chunk's buffer.
    pub buf: wgpu::Buffer,
}

impl Handle {
    /// First vertex of the model in its chunk (its draws' base vertex).
    pub fn v_first(&self) -> u64 {
        self.v.start * 4 / VSZ
    }
    /// Byte offsets of its vertices and indices in their chunk.
    pub fn v_bytes(&self) -> u64 {
        self.v.start * 4
    }
    pub fn i_bytes(&self) -> u64 {
        self.i.start * 4
    }
    /// First instance of the model in its instance chunk.
    pub fn n_first(&self) -> u32 {
        self.n.as_ref().map_or(0, |n| n.1.start as u32)
    }
    /// Move the model's geometry out of its chunk into another one with
    /// room (`compact`): the copies go into `enc`, and the old ranges are
    /// held until frames recorded before the move are done. Returns the
    /// byte shifts of its vertices and indices (new minus old); None when
    /// no other chunk has room.
    pub fn relocate(&mut self, enc: &mut wgpu::CommandEncoder) -> Option<(i64, i64)> {
        let (nv, ni) = ((self.v.end - self.v.start) * 4 / VSZ, self.i.end - self.i.start);
        let mut g = self.shared.lock(31);
        let (gc, v, i) = geo_alloc(&mut g.g, &self.shared.device, nv, ni, Some(self.gc), false)?;
        let buf = g.g.buf(gc)?;
        let f = g.frame;
        g.held.push((f, Held::G(self.gc, self.v.clone())));
        g.held.push((f, Held::G(self.gc, self.i.clone())));
        g.moved.0 += 1;
        g.moved.1 += (nv * VSZ) + ni * 4;
        drop(g);
        enc.copy_buffer_to_buffer(&self.buf, self.v.start * 4, &buf, v.start * 4, (self.v.end - self.v.start) * 4);
        enc.copy_buffer_to_buffer(&self.buf, self.i.start * 4, &buf, i.start * 4, ni * 4);
        let shift = ((v.start as i64 - self.v.start as i64) * 4, (i.start as i64 - self.i.start as i64) * 4);
        (self.gc, self.v, self.i, self.buf) = (gc, v, i, buf);
        Some(shift)
    }

    /// Rewrite the uniform part of material `k` (paint and other changes).
    pub fn write_mat(&self, k: usize, m: &[u8]) {
        if (k as u32) < self.n_mats && m.len() as u64 == self.shared.mat_bytes {
            let at = (self.mid0 as u64 + k as u64) * self.shared.row;
            self.shared.queue.write_buffer(&self.shared.mats, at, m);
        }
    }
    /// Write the model's instances (normal ones first, then fallbacks), each
    /// carrying the model's first material. False when the pool is full (the
    /// model then draws the old way).
    pub fn set_instances(&mut self, inst: &[Instance]) -> bool {
        let n = inst.len() as u64;
        let mut g = self.shared.lock(18);
        let fits = self.n.as_ref().is_some_and(|(_, r)| r.end - r.start >= n && n > 0);
        if !fits {
            if let Some((c, r)) = self.n.take() {
                let f = g.frame;
                g.held.push((f, Held::N(c, r)));
            }
            if n == 0 {
                return false;
            }
            self.n = g.n.alloc(&self.shared.device, n);
        }
        let Some((c, r)) = self.n.clone() else {
            return false;
        };
        let Some(buf) = g.n.buf(c) else {
            return false;
        };
        let unit = g.n.unit;
        drop(g);
        let mid = self.mid0 as f32;
        let v: Vec<Instance> = inst
            .iter()
            .map(|i| {
                let mut i = *i;
                i.extra[3] = mid;
                i
            })
            .collect();
        watch::at(2, 24);
        self.shared.queue.write_buffer(&buf, r.start * unit, bytemuck::cast_slice(&v));
        watch::at(2, 25);
        true
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let mut g = self.shared.lock(19);
        let f = g.frame;
        g.held.push((f, Held::G(self.gc, std::mem::take(&mut self.v))));
        g.held.push((f, Held::G(self.gc, std::mem::take(&mut self.i))));
        if let Some((c, r)) = self.n.take() {
            g.held.push((f, Held::N(c, r)));
        }
        let m = self.mid0 as u64..self.mid0 as u64 + self.n_mats as u64;
        g.held.push((f, Held::Mats(m)));
    }
}

impl Shared {
    /// The table lock, from `site` (see `watch::SITES`): a wait over 2 s
    /// says who waits and who took the lock last.
    fn lock(&self, site: usize) -> std::sync::MutexGuard<'_, Inner> {
        let t = std::time::Instant::now();
        let mut warned = false;
        loop {
            match self.inner.try_lock() {
                Ok(g) => {
                    watch::HOLDER.store(site, std::sync::atomic::Ordering::Relaxed);
                    return g;
                }
                Err(std::sync::TryLockError::Poisoned(e)) => return e.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => {
                    if !warned && t.elapsed().as_secs() >= 2 {
                        warned = true;
                        eprintln!("[map3d] table lock: [{}] waits over 2 s; {}", watch::SITES[site], watch::line());
                    }
                    if t.elapsed().as_micros() < 500 {
                        std::thread::yield_now();
                    } else {
                        std::thread::sleep(std::time::Duration::from_micros(200));
                    }
                }
            }
        }
    }

    /// None when the device lacks a feature, or `SIXNG_MAP_GPU_DRAWS=0`.
    /// `mat_bytes`: size of the material uniform (a multiple of 16).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, mat_bytes: u64) -> Option<Arc<Shared>> {
        if std::env::var("SIXNG_MAP_GPU_DRAWS").is_ok_and(|v| v == "0") {
            eprintln!("[map3d] GPU-driven draws: off (SIXNG_MAP_GPU_DRAWS=0)");
            return None;
        }
        if !device.features().contains(features()) || device.limits().max_binding_array_elements_per_shader_stage < ARRAYS {
            eprintln!("[map3d] GPU-driven draws: off (the device has no partly bound texture arrays or indirect first instance)");
            return None;
        }
        assert!(mat_bytes % 16 == 0, "material uniform is not a multiple of 16 bytes");
        let row = mat_bytes + 32;
        let storage = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu draw table"),
            entries: &[
                storage,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: std::num::NonZeroU32::new(ARRAYS),
                },
            ],
        });
        let bgl_mats = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu draw materials"),
            entries: &[storage],
        });
        let mats = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu draw materials"),
            size: MAT_CAP * row,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mats_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu draw materials"),
            layout: &bgl_mats,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: mats.as_entire_binding() }],
        });
        let max = device.limits().max_buffer_size;
        let nsz = std::mem::size_of::<Instance>() as u64;
        // COPY_SRC: `compact` moves models between chunks
        let g = Pool::new("pool geometry", wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_SRC, 4, G_CHUNK, max);
        let n = Pool::new("pool instances", wgpu::BufferUsages::VERTEX, nsz, N_CHUNK, max);
        // array 0: one white layer, the slot of every missing texture
        let white = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu draw white"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &white, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &[255u8; 4],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let wview = white.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let wclass = Class { format: wgpu::TextureFormat::Rgba8Unorm, w: 1, h: 1, mips: 1 };
        let max_layers = device.limits().max_texture_array_layers.min(MAX_LAYERS);
        watch::start();
        eprintln!(
            "[map3d] GPU-driven draws: on (material table; pool chunks {} + {} MB; texture arrays up to {} layers, {} arrays)",
            g.cap * g.unit >> 20,
            n.cap * n.unit >> 20,
            max_layers,
            ARRAYS
        );
        Some(Arc::new(Shared {
            device: device.clone(),
            queue: queue.clone(),
            bgl,
            bgl_mats,
            mats_bg,
            mats,
            mat_bytes,
            row,
            max_layers,
            writes: Mutex::new(Vec::new()),
            inner: Mutex::new(Inner {
                g,
                n,
                mats: Free::new(MAT_CAP),
                arrays: vec![Some(Arr { tex: white, view: wview, class: wclass, cap: 1, free: Vec::new(), gen: 1 })],
                classes: HashMap::new(),
                frame: 0,
                held: Vec::new(),
                gen: 1,
                built: 0,
                bg: None,
                rebuilds: (0, 0),
                moved: (0, 0),
            }),
        }))
    }

    /// A layer for a texture of `class`: the layer, the array texture and
    /// the layer index to write. None when the class is too large or no
    /// array is left (the texture is then a texture of its own).
    pub fn layer(self: &Arc<Self>, class: Class) -> Option<(Arc<Layer>, wgpu::Texture, u32)> {
        let bytes = class.layer_bytes();
        if bytes == 0 || bytes > LAYER_MAX || class.mips == 0 {
            return None;
        }
        let mut g = self.lock(20);
        let g = &mut *g;
        let list = g.classes.entry(class).or_default();
        // the lowest free layer of the class's first array with room
        for &a in list.iter() {
            if let Some(arr) = g.arrays[a].as_mut() {
                if let Some(l) = arr.free.pop() {
                    let out = (Arc::new(Layer { shared: self.clone(), slot: slot(a as u32, l), need: arr.gen }), arr.tex.clone(), l);
                    return Some(out);
                }
            }
        }
        // a new array: twice the class's layers so far, up to the caps
        let had: u32 = list.iter().filter_map(|&a| g.arrays[a].as_ref()).map(|a| a.cap).sum();
        let cap_bytes = (ARRAY_BYTES / bytes).clamp(1, self.max_layers as u64) as u32;
        let cap = had.max(FIRST_LAYERS).min(cap_bytes).min(self.max_layers);
        let a = match g.arrays.iter().position(|a| a.is_none()) {
            Some(a) => a,
            None if (g.arrays.len() as u32) < ARRAYS => {
                g.arrays.push(None);
                g.arrays.len() - 1
            }
            None => return None,
        };
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu draw texture array"),
            size: wgpu::Extent3d { width: class.w, height: class.h, depth_or_array_layers: cap },
            mip_level_count: class.mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: class.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        g.gen += 1;
        let gen = g.gen;
        // handed out from the top of the list: layer 0 first
        let mut free: Vec<u32> = (0..cap).rev().collect();
        let l = free.pop().unwrap();
        g.arrays[a] = Some(Arr { tex: tex.clone(), view, class, cap, free, gen });
        g.classes.get_mut(&class).unwrap().push(a);
        Some((Arc::new(Layer { shared: self.clone(), slot: slot(a as u32, l), need: gen }), tex, l))
    }

    /// Place a model's materials (`rows`: per material its uniform, then
    /// its eight texture slots as u32) and room for its geometry (`nv`
    /// vertices, `ni` indices); the caller writes the geometry at the
    /// handle's offsets. `need`: the bind group generation its textures need.
    pub fn place(self: &Arc<Self>, rows: &[u8], nv: u64, ni: u64, need: u64) -> Option<Handle> {
        let n_mats = rows.len() as u64 / self.row;
        if n_mats == 0 || rows.len() as u64 != n_mats * self.row || nv == 0 || ni == 0 {
            return None;
        }
        let mut g = self.lock(21);
        let m = g.mats.alloc(n_mats)?;
        let Some((gc, v, i)) = geo_alloc(&mut g.g, &self.device, nv, ni, None, true) else {
            g.mats.free(m);
            return None;
        };
        let buf = g.g.buf(gc).unwrap();
        drop(g);
        self.queue.write_buffer(&self.mats, m.start * self.row, rows);
        Some(Handle { shared: self.clone(), gc, v, i, n: None, mid0: m.start as u32, n_mats: n_mats as u32, need, buf })
    }

    /// The geometry chunk `compact` should empty: the least used one, when
    /// the chunks hold more than one and a half chunks of free room and the
    /// others have room for what it holds.
    pub fn compact_from(&self) -> Option<usize> {
        let g = self.lock(30);
        let live: Vec<(usize, u64)> = g.g.chunks.iter().enumerate().filter_map(|(c, ch)| ch.as_ref().map(|(_, f)| (c, f.used()))).collect();
        if live.len() < 2 {
            return None;
        }
        let cap = g.g.cap;
        let free: u64 = live.iter().map(|(_, u)| cap - u).sum();
        let (c, u) = *live.iter().min_by_key(|(_, u)| *u)?;
        (free * 2 > cap * 3 && free - (cap - u) > u + cap / 8).then_some(c)
    }

    /// Queue a write into an array layer for the render thread, which runs
    /// it before the next frame's passes: a model uploaded meanwhile is first
    /// drawn in that frame, so it never shows an unwritten layer.
    pub fn defer(&self, w: Deferred) {
        self.writes.lock().unwrap_or_else(|e| e.into_inner()).push(w);
    }

    /// Bytes of one table row (material uniform, then eight slots).
    pub fn row(&self) -> u64 {
        self.row
    }

    /// Once per frame before drawing: hand out ranges and layers freed long
    /// enough ago, give empty chunks back, rebuild the bind group when an
    /// array came, and return what the frame draws with.
    pub fn prepare(&self) -> Snapshot {
        // the layer writes first, on this thread: writing an array while
        // another thread submits a frame that binds it hangs both inside wgpu
        let writes = std::mem::take(&mut *self.writes.lock().unwrap_or_else(|e| e.into_inner()));
        for w in writes {
            w(&self.queue);
        }
        let mut g = self.lock(22);
        let g = &mut *g;
        g.frame += 1;
        watch::FRAMES.store(g.frame, std::sync::atomic::Ordering::Relaxed);
        let f = g.frame;
        for (at, h) in std::mem::take(&mut g.held) {
            if f < at + HOLD_FRAMES {
                g.held.push((at, h));
                continue;
            }
            match h {
                Held::G(c, r) => g.g.free(c, r),
                Held::N(c, r) => g.n.free(c, r),
                Held::Mats(r) => g.mats.free(r),
                Held::Layer(s) => {
                    if let Some(Some(a)) = g.arrays.get_mut((s >> 8) as usize) {
                        a.free.push(s & 255);
                    }
                }
            }
        }
        let held = |k: u8| -> Vec<usize> {
            g.held
                .iter()
                .filter_map(|(_, h)| match (k, h) {
                    (0, Held::G(c, _)) | (2, Held::N(c, _)) => Some(*c),
                    _ => None,
                })
                .collect()
        };
        let (hg, hn) = (held(0), held(2));
        g.g.trim(&hg);
        g.n.trim(&hn);
        if g.built != g.gen || g.bg.is_none() {
            let t = std::time::Instant::now();
            let n = g.arrays.len();
            let views: Vec<&wgpu::TextureView> = (0..n).map(|k| g.arrays[k].as_ref().map_or(&g.arrays[0].as_ref().unwrap().view, |a| &a.view)).collect();
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gpu draw table"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.mats.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureViewArray(&views) },
                ],
            });
            g.bg = Some(bg);
            g.built = g.gen;
            g.rebuilds.0 += 1;
            g.rebuilds.1 += t.elapsed().as_micros() as u64;
        }
        Snapshot { built: g.built, bg: g.bg.clone(), mats_bg: self.mats_bg.clone(), g: g.g.snapshot(), n: g.n.snapshot() }
    }

    /// Pool and table use for the tick line.
    pub fn summary(&self) -> String {
        let mut g = self.lock(23);
        let (gn, gb, gu) = g.g.stats();
        let (nn, nb, nu) = g.n.stats();
        let moved = std::mem::take(&mut g.moved);
        let live: Vec<&Arr> = g.arrays.iter().skip(1).flatten().collect();
        let layers: u32 = live.iter().map(|a| a.cap).sum();
        let used: u32 = live.iter().map(|a| a.cap - a.free.len() as u32).sum();
        let tex_bytes: u64 = live.iter().map(|a| a.class.layer_bytes() * a.cap as u64).sum();
        let tex_used: u64 = live.iter().map(|a| a.class.layer_bytes() * (a.cap - a.free.len() as u32) as u64).sum();
        let mb = |b: u64| b as f64 / (1 << 20) as f64;
        let (n_live, n_classes, n_mats) = (live.len(), g.classes.values().filter(|l| !l.is_empty()).count(), g.mats.used());
        drop(live);
        let r = std::mem::take(&mut g.rebuilds);
        format!(
            "pool {gn}+{nn} chunks {:.0}/{:.0} MB used ({} models moved, {:.0} MB), {} arrays ({} classes) {used}/{layers} layers {:.0}/{:.0} MB, {} materials, {} table rebuilds {:.1} ms",
            mb(gu + nu),
            mb(gb + nb),
            moved.0,
            mb(moved.1),
            n_live,
            n_classes,
            mb(tex_used),
            mb(tex_bytes),
            n_mats,
            r.0,
            r.1 as f32 / 1000.0
        )
    }
}

/// Texture builtins whose first texture argument the transform rewrites,
/// and where the array layer goes (arguments after the texture; None: no layer).
fn layer_after(f: &str) -> Option<Option<usize>> {
    match f {
        "textureSample" | "textureSampleLevel" | "textureSampleGrad" | "textureSampleBias" | "textureGather" => Some(Some(2)),
        "textureLoad" => Some(Some(1)),
        "textureDimensions" | "textureNumLevels" => Some(None),
        _ => None,
    }
}

fn ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Split the arguments of a call whose `(` is just before `at`: the
/// arguments (as written) and the index of the closing `)`.
fn split_args(s: &str, at: usize) -> Option<(Vec<String>, usize)> {
    let b = s.as_bytes();
    let (mut depth, mut start) = (0i32, at);
    let mut out = Vec::new();
    for k in at..b.len() {
        match b[k] {
            b'(' | b'[' => depth += 1,
            b')' | b']' if depth > 0 => depth -= 1,
            b')' => {
                out.push(s[start..k].to_string());
                return Some((out, k));
            }
            b',' if depth == 0 => {
                out.push(s[start..k].to_string());
                start = k + 1;
            }
            _ => {}
        }
    }
    None
}

/// Rewrite the texture builtins that read one of `names` (texture name,
/// slot expression) to read its array and layer.
fn rewrite_tex_calls(s: &str, names: &[(&str, &str)]) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 4);
    let mut i = 0;
    while i < s.len() {
        let Some(k) = s[i..].find("texture") else {
            out.push_str(&s[i..]);
            break;
        };
        let p = i + k;
        let mut e = p;
        while e < b.len() && ident_byte(b[e]) {
            e += 1;
        }
        let f = &s[p..e];
        let boundary = p == 0 || !ident_byte(b[p - 1]);
        if let (true, true, Some(after)) = (boundary, e < b.len() && b[e] == b'(', layer_after(f)) {
            if let Some((args, close)) = split_args(s, e + 1) {
                let mut args: Vec<String> = args.iter().map(|a| rewrite_tex_calls(a, names)).collect();
                let hit = args.iter().enumerate().find_map(|(n, a)| names.iter().find(|(t, _)| a.trim() == *t).map(|(_, x)| (n, *x)));
                if let Some((ti, x)) = hit {
                    let lead = if args[ti].starts_with(' ') { " " } else { "" };
                    args[ti] = format!("{lead}TEX[({x}) >> 8u]");
                    if let Some(a) = after {
                        let at = ti + 1 + a;
                        if at <= args.len() {
                            args.insert(at, format!(" i32(({x}) & 255u)"));
                        }
                    }
                }
                out.push_str(&s[i..e]);
                out.push('(');
                out.push_str(&args.join(","));
                out.push(')');
                i = close + 1;
                continue;
            }
        }
        out.push_str(&s[i..e.max(p + 1)]);
        i = e.max(p + 1);
    }
    out
}

/// The map shader for GPU-driven draws: the material group becomes the
/// shared table and the texture arrays, read through the material index
/// (the instance's first material plus the vertex's own). None (and the
/// path off) if the source has changed shape and a piece is not found.
pub fn shader(common: &str, src: &str) -> Option<String> {
    shader_with(common, src, opt("vsfull"))
}

/// `full`: every program loads the material's whole row (the earlier form,
/// kept for A/B runs).
fn shader_with(common: &str, src: &str, full: bool) -> Option<String> {
    let mut s = src.replace("\r\n", "\n");
    let fail = |what: &str| {
        eprintln!("[map3d] GPU-driven draws: off (shader piece not found: {what})");
    };
    // the material group: from its uniform to its last texture
    let a = "@group(1) @binding(0) var<uniform> M: Mat;";
    let b = "@group(1) @binding(8) var t_dust: texture_2d<f32>;";
    let (Some(i0), Some(i1)) = (s.find(a), s.find(b)) else {
        fail("material group");
        return None;
    };
    let table = format!(
        "struct MatB {{ m: Mat, t0: vec4<u32>, t1: vec4<u32> }};\n\
         @group(1) @binding(0) var<storage, read> MATS: array<MatB>;\n\
         @group(1) @binding(1) var TEX: binding_array<texture_2d_array<f32>, {ARRAYS}>;\n\
         var<private> M: Mat;\n\
         var<private> TI0: vec4<u32>;\n\
         var<private> TI1: vec4<u32>;\n\
         fn bl_set(id: u32) {{ let b = MATS[id]; M = b.m; TI0 = b.t0; TI1 = b.t1; }}\n"
    );
    s.replace_range(i0..i1 + b.len(), &table);
    let mut rep = |from: &str, to: &str| -> bool {
        if !s.contains(from) {
            fail(from);
            return false;
        }
        s = s.replace(from, to);
        true
    };
    // skin_tile takes a texture: it takes the slot instead
    let ok = rep("fn skin_tile(t: texture_2d<f32>,", "fn skin_tile(t_skin: u32,")
        && rep("return textureSampleLevel(t, samp, q, lod);", "return textureSampleLevel(t_skin, samp, q, lod);")
        // the material index: the instance's first material (extra.w) plus
        // the vertex's own (joints.w); the vertex program sets its own w
        && rep(
            "    @location(13) rest_n: vec3<f32>,\n};",
            "    @location(13) rest_n: vec3<f32>,\n    @location(14) @interpolate(flat) mid: u32,\n};",
        )
        // The vertex program reads only the strand width factor (the
        // shadow passes are vertex bound, and a whole row is 176 bytes per
        // vertex); a shadow cut-out only its alpha inputs and base texture.
        && rep(
            "fn vs_mesh(v: VIn) -> VOut {\n    return mesh_vertex(v);",
            if full {
                "fn vs_mesh(v: VIn) -> VOut {\n    let mid = u32(v.extra.w + 0.5) + v.joints.w;\n    bl_set(mid);\n    var o = mesh_vertex(v);\n    o.mid = mid;\n    return o;"
            } else {
                "fn vs_mesh(v: VIn) -> VOut {\n    let mid = u32(v.extra.w + 0.5) + v.joints.w;\n    M.strand = MATS[mid].m.strand;\n    var o = mesh_vertex(v);\n    o.mid = mid;\n    return o;"
            },
        )
        && rep(
            "fn fs_mesh(i: VOut, @builtin(front_facing) ff: bool) -> MeshOut {",
            "fn fs_mesh(i: VOut, @builtin(front_facing) ff: bool) -> MeshOut {\n    bl_set(i.mid);",
        )
        && rep(
            "fn fs_shadow(i: VOut) {",
            if full {
                "fn fs_shadow(i: VOut) {\n    bl_set(i.mid);"
            } else {
                "fn fs_shadow(i: VOut) {\n    M.base = MATS[i.mid].m.base;\n    M.flags = MATS[i.mid].m.flags;\n    TI0.x = MATS[i.mid].t0.x;"
            },
        )
        // no stickers on the map: the pixel program without their part.
        // With it the main pass took about a tenth longer although the
        // list is empty there and the part never runs (measured: the
        // sticker laid on flat alone added 0.5 ms to 4.4 ms).
        && rep("let st_n = min(ST[0].flags.x, arrayLength(&ST) - 1u);", "let st_n = 0u;");
    if !ok {
        return None;
    }
    let slots = [
        ("t_base", "TI0.x"),
        ("t_nrm", "TI0.y"),
        ("t_pk", "TI0.z"),
        ("t_em", "TI0.w"),
        ("t_tile", "TI1.x"),
        ("t_wear", "TI1.y"),
        ("t_primer", "TI1.z"),
        ("t_dust", "TI1.w"),
    ];
    let mut calls = slots.to_vec();
    calls.push(("t_skin", "t_skin"));
    s = rewrite_tex_calls(&s, &calls);
    // what is left of the names: slots handed to skin_tile
    for (name, to) in slots {
        s = replace_ident(&s, name, to);
    }
    // the material now comes from a table indexed per pixel, so naga's
    // uniformity check would reject the texture reads under its branches;
    // the index is the same over a draw, which is what the check guards
    Some(format!(
        "enable wgpu_binding_array;\ndiagnostic(off, derivative_uniformity);\n{}\n{}",
        common.replace("\r\n", "\n"),
        s
    ))
}

/// Replace whole-identifier occurrences of `name`.
fn replace_ident(s: &str, name: &str, to: &str) -> String {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(k) = rest.find(name) {
        let before = rest[..k].chars().next_back();
        let after = rest[k + name.len()..].chars().next();
        out.push_str(&rest[..k]);
        if before.is_some_and(ident) || after.is_some_and(ident) {
            out.push_str(name);
        } else {
            out.push_str(to);
        }
        rest = &rest[k + name.len()..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_list_merges() {
        let mut f = Free::new(100);
        let a = f.alloc(10).unwrap();
        let b = f.alloc(20).unwrap();
        let c = f.alloc(30).unwrap();
        assert_eq!((a.clone(), b.clone(), c.clone()), (0..10, 10..30, 30..60));
        f.free(b);
        f.free(a);
        assert_eq!(f.list, vec![0..30, 60..100]);
        assert_eq!(f.used(), 30);
        f.free(c);
        assert!(f.whole());
        assert!(f.alloc(101).is_none());
    }

    #[test]
    fn tex_calls_take_array_and_layer() {
        let names = [("t_em", "TI0.w"), ("t_wear", "TI1.y")];
        assert_eq!(
            rewrite_tex_calls("textureSampleLevel(t_em, samp, vec2(clamp(g, 0.0, 1.0), 0.5), 0.0)", &names),
            "textureSampleLevel(TEX[(TI0.w) >> 8u], samp, vec2(clamp(g, 0.0, 1.0), 0.5), i32((TI0.w) & 255u), 0.0)"
        );
        assert_eq!(
            rewrite_tex_calls("textureLoad(t_em, vec2(16 + c, row), 0).a", &names),
            "textureLoad(TEX[(TI0.w) >> 8u], vec2(16 + c, row), i32((TI0.w) & 255u), 0).a"
        );
        assert_eq!(
            rewrite_tex_calls("textureGather(1, t_wear, samp, uv)", &names),
            "textureGather(1, TEX[(TI1.y) >> 8u], samp, uv, i32((TI1.y) & 255u))"
        );
        // nested: a texture read inside another call's argument
        assert_eq!(
            rewrite_tex_calls("f(textureSample(t_em, s, uv).r, mytexture)", &names),
            "f(textureSample(TEX[(TI0.w) >> 8u], s, uv, i32((TI0.w) & 255u)).r, mytexture)"
        );
        // other textures stay as they are
        assert_eq!(rewrite_tex_calls("textureSample(t_other, s, uv)", &names), "textureSample(t_other, s, uv)");
    }

    #[test]
    fn class_bytes() {
        let c = Class { format: wgpu::TextureFormat::Bc1RgbaUnorm, w: 8, h: 8, mips: 2 };
        // 2x2 blocks of 8 bytes, then one block
        assert_eq!(c.layer_bytes(), 32 + 8);
        let c = Class { format: wgpu::TextureFormat::Rgba8Unorm, w: 4, h: 2, mips: 3 };
        assert_eq!(c.layer_bytes(), 32 + 8 + 4);
    }

    #[test]
    fn map_shader_parses_and_validates() {
        for full in [false, true] {
            let src = shader_with(include_str!("atmo_common.wgsl"), include_str!("shader.wgsl"), full).expect("transform");
            let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
            for t in ["t_base", "t_nrm", "t_pk", "t_em", "t_tile", "t_wear", "t_primer", "t_dust"] {
                assert!(!src.contains(&format!("{t},")) && !src.contains(&format!("{t})")), "{t} left");
            }
            // the vertex program loads one field, not the row
            let vs = &src[src.find("fn vs_mesh(").unwrap()..];
            let vs = &vs[..vs.find("\n}").unwrap()];
            assert_eq!(vs.contains("bl_set("), full);
        }
    }
}
