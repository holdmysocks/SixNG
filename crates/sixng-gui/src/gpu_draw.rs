//! GPU-driven drawing of the 3D map's models.
//!
//! A city view draws thousands of models, most with several materials, and
//! each material used to be its own bind group and draw call. Here the map's
//! models keep their geometry in a few large shared buffers (the pool),
//! their materials in one storage buffer and their textures in pages of a
//! texture binding array. The opaque, coverage-fallback and shadow passes
//! then draw everything that shares a pool chunk and a texture page with one
//! indirect call: each sub-draw is one entry of an indirect buffer, and its
//! instances (copied per sub-draw for the frame) carry its material's index
//! in `extra.w`, which the vertex program hands to the pixel program.
//!
//! Models that do not fit (skinned, strand hair, too many instances), or
//! whose texture page has not been rebuilt since they arrived, draw the old
//! way. `SIXNG_MAP_GPU_DRAWS=0` turns the whole path off.

use std::ops::Range;
use std::sync::{Arc, Mutex};

/// Textures in one page (one bind group); slot 0 is always the white texture.
pub const PAGE: u32 = 4096;
const MAX_PAGES: usize = 64;
/// Materials in the shared table.
const MAT_CAP: u64 = 1 << 17;
/// Frames a freed pool or material range waits before it is handed out again
/// (a frame recorded before the free may not be submitted yet).
const HOLD_FRAMES: u64 = 3;
/// Frames between two rebuilds of one texture page.
const PAGE_EVERY: u64 = 10;

/// One material in the shared table: the material uniform (as the shader's
/// `Mat`) and the page slots of its eight textures.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MatB {
    pub m: [u32; 20],
    pub t: [u32; 8],
}
const MATB: u64 = std::mem::size_of::<MatB>() as u64;

/// One entry of an indexed indirect draw (wgpu's `DrawIndexedIndirectArgs`).
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Args {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub base_vertex: i32,
    pub first_instance: u32,
}
pub const ARGS: u64 = std::mem::size_of::<Args>() as u64;

/// Free ranges of a buffer or table, sorted, neighbours merged.
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
        let k = self.list.iter().position(|r| r.end - r.start >= n)?;
        let r = &mut self.list[k];
        let out = r.start..r.start + n;
        r.start += n;
        if r.start == r.end {
            self.list.remove(k);
        }
        Some(out)
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
}

struct Chunk {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    v: Free,
    i: Free,
}

struct Page {
    views: Vec<Option<wgpu::TextureView>>,
    free: Vec<u32>,
    /// Bumped by every change of a slot; `built` is the value the bind group shows.
    gen: u64,
    built: u64,
    built_frame: u64,
    bg: Option<wgpu::BindGroup>,
    /// One past the highest slot handed out (what a partly bound page binds).
    hi: u32,
}

enum Held {
    Geo { chunk: usize, v: Range<u64>, i: Range<u64> },
    Mats(Range<u64>),
}

struct Inner {
    chunks: Vec<Option<Chunk>>,
    pages: Vec<Page>,
    mats: Free,
    frame: u64,
    held: Vec<(u64, Held)>,
    /// Page rebuilds and their time (µs) since the stats were last taken.
    rebuilds: (u32, u64),
}

/// The shared tables. Created with the map's renderer when the device can
/// index texture arrays per pixel and draw indirect with a first instance.
pub struct Shared {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub bgl: wgpu::BindGroupLayout,
    /// The material table alone (solid shadow casters read no texture).
    pub bgl_mats: wgpu::BindGroupLayout,
    mats_bg: wgpu::BindGroup,
    mats: wgpu::Buffer,
    white: wgpu::TextureView,
    /// Pages bind only their used slots (PARTIALLY_BOUND_BINDING_ARRAY): wgpu
    /// tracks every bound view each time a page is set in a pass.
    partial: bool,
    vcap: u64,
    icap: u64,
    inner: Mutex<Inner>,
}

/// What a frame draws with: per page its bind group and the generation it
/// shows, per chunk its buffers.
pub struct Snapshot {
    pub built: Vec<u64>,
    pub bgs: Vec<Option<wgpu::BindGroup>>,
    pub chunks: Vec<Option<(wgpu::Buffer, wgpu::Buffer)>>,
    pub mats_bg: wgpu::BindGroup,
}

impl Snapshot {
    pub fn ready(&self, h: &Handle) -> bool {
        self.built.get(h.page).is_some_and(|&b| b >= h.gen)
            && self.bgs.get(h.page).is_some_and(|b| b.is_some())
            && self.chunks.get(h.chunk).is_some_and(|c| c.is_some())
    }
}

/// The features the path needs.
pub fn features() -> wgpu::Features {
    wgpu::Features::TEXTURE_BINDING_ARRAY
        | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
        | wgpu::Features::INDIRECT_FIRST_INSTANCE
}

/// Features the path uses when the device has them.
pub fn optional_features() -> wgpu::Features {
    wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY
}

/// A model's place in the shared tables; freed when the model is dropped.
pub struct Handle {
    shared: Arc<Shared>,
    pub chunk: usize,
    v: Range<u64>,
    i: Range<u64>,
    pub page: usize,
    slots: Vec<u32>,
    pub mid0: u32,
    n_mats: u32,
    pub gen: u64,
    pub vb: wgpu::Buffer,
    pub ib: wgpu::Buffer,
}

impl Handle {
    /// Byte offset of the model's vertices in the chunk's vertex buffer.
    pub fn v_start(&self) -> u64 {
        self.v.start
    }
    pub fn i_start(&self) -> u64 {
        self.i.start
    }
    /// Rewrite the uniform part of material `k` (paint and other changes).
    pub fn write_mat(&self, k: usize, m: [u32; 20]) {
        if (k as u32) < self.n_mats {
            let at = (self.mid0 as u64 + k as u64) * MATB;
            self.shared.queue.write_buffer(&self.shared.mats, at, bytemuck::cast_slice(&m));
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let mut g = self.shared.inner.lock().unwrap();
        let f = g.frame;
        if let Some(p) = g.pages.get_mut(self.page) {
            for &s in &self.slots {
                p.views[s as usize] = None;
                p.free.push(s);
            }
            p.gen += 1;
        }
        let (v, i) = (std::mem::take(&mut self.v), std::mem::take(&mut self.i));
        g.held.push((f, Held::Geo { chunk: self.chunk, v, i }));
        let m = self.mid0 as u64..self.mid0 as u64 + self.n_mats as u64;
        g.held.push((f, Held::Mats(m)));
    }
}

/// A model's texture slots, before its materials and geometry are placed.
pub struct Textures {
    pub page: usize,
    /// Per image of the model, its slot (0: white).
    pub of_image: Vec<u32>,
    owned: Vec<u32>,
    gen: u64,
}

impl Shared {
    /// None when the device lacks a feature, or `SIXNG_MAP_GPU_DRAWS=0`.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, white: &wgpu::TextureView) -> Option<Arc<Shared>> {
        if std::env::var("SIXNG_MAP_GPU_DRAWS").is_ok_and(|v| v == "0") {
            eprintln!("[map3d] GPU-driven draws: off (SIXNG_MAP_GPU_DRAWS=0)");
            return None;
        }
        if !device.features().contains(features()) || device.limits().max_binding_array_elements_per_shader_stage < PAGE {
            eprintln!("[map3d] GPU-driven draws: off (the device has no texture arrays or indirect first instance)");
            return None;
        }
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu draw page"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: std::num::NonZeroU32::new(PAGE),
                },
            ],
        });
        let mats = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu draw materials"),
            size: MAT_CAP * MATB,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl_mats = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu draw materials"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let mats_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu draw materials"),
            layout: &bgl_mats,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: mats.as_entire_binding() }],
        });
        let partial = device.features().contains(optional_features());
        // chunk sizes: as large as the device allows, up to 256 MB of
        // vertices and 128 MB of indices
        let max = device.limits().max_buffer_size;
        let vsz = std::mem::size_of::<crate::glb::Vertex>() as u64;
        let vcap = (max.min(256 << 20) / vsz) * vsz;
        let icap = (max.min(128 << 20) / 4) * 4;
        eprintln!(
            "[map3d] GPU-driven draws: on (pool chunks {} MB + {} MB, {} textures per page{})",
            vcap >> 20,
            icap >> 20,
            PAGE,
            if partial { ", partly bound" } else { "" }
        );
        Some(Arc::new(Shared {
            device: device.clone(),
            queue: queue.clone(),
            bgl,
            bgl_mats,
            mats_bg,
            mats,
            white: white.clone(),
            partial,
            vcap,
            icap,
            inner: Mutex::new(Inner {
                chunks: Vec::new(),
                pages: Vec::new(),
                mats: Free::new(MAT_CAP),
                frame: 0,
                held: Vec::new(),
                rebuilds: (0, 0),
            }),
        }))
    }

    /// Slots for a model's textures (`views`, one per image; None: no
    /// pixels), all in one page. None when no page has room.
    pub fn textures(&self, views: &[Option<wgpu::TextureView>]) -> Option<Textures> {
        let k = views.iter().filter(|v| v.is_some()).count();
        if k as u32 >= PAGE {
            return None;
        }
        let mut g = self.inner.lock().unwrap();
        let page = match g.pages.iter().rposition(|p| p.free.len() >= k) {
            Some(p) => p,
            None if g.pages.len() < MAX_PAGES => {
                g.pages.push(Page {
                    views: vec![None; PAGE as usize],
                    free: (1..PAGE).rev().collect(),
                    gen: 1,
                    built: 0,
                    built_frame: 0,
                    bg: None,
                    hi: 1,
                });
                g.pages.len() - 1
            }
            None => return None,
        };
        let p = &mut g.pages[page];
        let mut of_image = vec![0u32; views.len()];
        let mut owned = Vec::with_capacity(k);
        for (n, v) in views.iter().enumerate() {
            if let Some(v) = v {
                let s = p.free.pop().unwrap();
                p.views[s as usize] = Some(v.clone());
                p.hi = p.hi.max(s + 1);
                of_image[n] = s;
                owned.push(s);
            }
        }
        p.gen += 1;
        Some(Textures { page, of_image, owned, gen: p.gen })
    }

    /// Place a model's materials and room for its geometry (`vbytes`,
    /// `ibytes`); the caller writes the geometry at the handle's offsets.
    /// None (and the texture slots freed) when a table or chunk is full.
    pub fn place(self: &Arc<Self>, tex: Textures, mats: &[MatB], vbytes: u64, ibytes: u64) -> Option<Handle> {
        let mut g = self.inner.lock().unwrap();
        let undo = |g: &mut Inner, tex: &Textures| {
            if let Some(p) = g.pages.get_mut(tex.page) {
                for &s in &tex.owned {
                    p.views[s as usize] = None;
                    p.free.push(s);
                }
                p.gen += 1;
            }
        };
        if vbytes == 0 || ibytes == 0 || vbytes > self.vcap || ibytes > self.icap || mats.is_empty() {
            undo(&mut *g, &tex);
            return None;
        }
        let Some(m) = g.mats.alloc(mats.len() as u64) else {
            undo(&mut *g, &tex);
            return None;
        };
        // first chunk with room for both, else a new one
        let mut found = None;
        for (c, ch) in g.chunks.iter_mut().enumerate() {
            let Some(ch) = ch else { continue };
            let Some(v) = ch.v.alloc(vbytes) else { continue };
            match ch.i.alloc(ibytes) {
                Some(i) => {
                    found = Some((c, v, i));
                    break;
                }
                None => ch.v.free(v),
            }
        }
        let (chunk, v, i) = match found {
            Some(f) => f,
            None => {
                let mk = |label, size, usage| {
                    self.device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false })
                };
                let mut ch = Chunk {
                    vb: mk("pool vb", self.vcap, wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST),
                    ib: mk("pool ib", self.icap, wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST),
                    v: Free::new(self.vcap),
                    i: Free::new(self.icap),
                };
                let v = ch.v.alloc(vbytes).unwrap();
                let i = ch.i.alloc(ibytes).unwrap();
                let c = match g.chunks.iter().position(|c| c.is_none()) {
                    Some(c) => {
                        g.chunks[c] = Some(ch);
                        c
                    }
                    None => {
                        g.chunks.push(Some(ch));
                        g.chunks.len() - 1
                    }
                };
                (c, v, i)
            }
        };
        let ch = g.chunks[chunk].as_ref().unwrap();
        let (vb, ib) = (ch.vb.clone(), ch.ib.clone());
        drop(g);
        self.queue.write_buffer(&self.mats, m.start * MATB, bytemuck::cast_slice(mats));
        Some(Handle {
            shared: self.clone(),
            chunk,
            v,
            i,
            page: tex.page,
            slots: tex.owned,
            mid0: m.start as u32,
            n_mats: mats.len() as u32,
            gen: tex.gen,
            vb,
            ib,
        })
    }

    /// Once per frame before drawing: hand out ranges freed long enough ago,
    /// drop empty chunks, rebuild one changed texture page, and return what
    /// the frame draws with.
    pub fn prepare(&self) -> Snapshot {
        let mut g = self.inner.lock().unwrap();
        g.frame += 1;
        let f = g.frame;
        let held = std::mem::take(&mut g.held);
        for (at, h) in held {
            if f < at + HOLD_FRAMES {
                g.held.push((at, h));
                continue;
            }
            match h {
                Held::Geo { chunk, v, i } => {
                    if let Some(Some(ch)) = g.chunks.get_mut(chunk) {
                        ch.v.free(v);
                        ch.i.free(i);
                    }
                }
                Held::Mats(m) => g.mats.free(m),
            }
        }
        // an empty chunk goes back to the device (the first one stays)
        let in_held: Vec<usize> = g.held.iter().filter_map(|(_, h)| if let Held::Geo { chunk, .. } = h { Some(*chunk) } else { None }).collect();
        for c in 1..g.chunks.len() {
            let empty = g.chunks[c].as_ref().is_some_and(|ch| ch.v.whole() && ch.i.whole());
            if empty && !in_held.contains(&c) {
                g.chunks[c] = None;
            }
        }
        // one page per frame: the stalest changed one not rebuilt in the
        // last few frames (a page with models waiting on it goes first)
        let pick = g
            .pages
            .iter()
            .enumerate()
            .filter(|(_, p)| p.gen > p.built && (p.bg.is_none() || f >= p.built_frame + PAGE_EVERY))
            .min_by_key(|(_, p)| p.built_frame)
            .map(|(k, _)| k);
        if let Some(k) = pick {
            let t = std::time::Instant::now();
            let p = &g.pages[k];
            let n = if self.partial { p.hi as usize } else { PAGE as usize };
            let views: Vec<&wgpu::TextureView> = p.views[..n].iter().map(|v| v.as_ref().unwrap_or(&self.white)).collect();
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gpu draw page"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.mats.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureViewArray(&views) },
                ],
            });
            let gen = p.gen;
            let p = &mut g.pages[k];
            p.bg = Some(bg);
            p.built = gen;
            p.built_frame = f;
            g.rebuilds.0 += 1;
            g.rebuilds.1 += t.elapsed().as_micros() as u64;
        }
        Snapshot {
            built: g.pages.iter().map(|p| p.built).collect(),
            bgs: g.pages.iter().map(|p| p.bg.clone()).collect(),
            chunks: g.chunks.iter().map(|c| c.as_ref().map(|c| (c.vb.clone(), c.ib.clone()))).collect(),
            mats_bg: self.mats_bg.clone(),
        }
    }

    /// Pool and table use for the draw log: chunks, pages, materials in
    /// use, page rebuilds and their time (µs) since the last call.
    pub fn stats(&self) -> (usize, usize, u64, (u32, u64)) {
        let mut g = self.inner.lock().unwrap();
        let used = MAT_CAP - g.mats.list.iter().map(|r| r.end - r.start).sum::<u64>();
        let r = std::mem::take(&mut g.rebuilds);
        (g.chunks.iter().filter(|c| c.is_some()).count(), g.pages.len(), used, r)
    }
}

/// The map shader for GPU-driven draws: the material group becomes the
/// shared material table and a texture page, read through the material
/// index the instance carries. None (and the path off) if the source has
/// changed shape and a piece is not found.
pub fn shader(common: &str, src: &str) -> Option<String> {
    let src = src.replace("\r\n", "\n");
    let mut s = src.clone();
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
         @group(1) @binding(1) var TEX: binding_array<texture_2d<f32>, {PAGE}>;\n\
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
    let ok = rep("fn skin_tile(t: texture_2d<f32>,", "fn skin_tile(ti: u32,")
        && rep("return textureSampleGrad(t, samp, q,", "return textureSampleGrad(TEX[ti], samp, q,")
        && rep("skin_tile(t_primer,", "skin_tile(TI1.z,")
        && rep("skin_tile(t_dust,", "skin_tile(TI1.w,")
        // the material index: instance extra.w (the vertex program sets its own w)
        && rep(
            "    @location(11) uv23: vec4<f32>,\n};",
            "    @location(11) uv23: vec4<f32>,\n    @location(12) @interpolate(flat) mid: u32,\n};",
        )
        && rep(
            "fn vs_mesh(v: VIn) -> VOut {\n    return mesh_vertex(v);",
            "fn vs_mesh(v: VIn) -> VOut {\n    let mid = u32(v.extra.w + 0.5);\n    bl_set(mid);\n    var o = mesh_vertex(v);\n    o.mid = mid;\n    return o;",
        )
        && rep(
            "fn fs_mesh(i: VOut, @builtin(front_facing) ff: bool) -> MeshOut {",
            "fn fs_mesh(i: VOut, @builtin(front_facing) ff: bool) -> MeshOut {\n    bl_set(i.mid);",
        )
        && rep("fn fs_shadow(i: VOut) {", "fn fs_shadow(i: VOut) {\n    bl_set(i.mid);");
    if !ok {
        return None;
    }
    // the textures by name: whole identifiers only
    for (name, to) in [
        ("t_base", "TEX[TI0.x]"),
        ("t_nrm", "TEX[TI0.y]"),
        ("t_pk", "TEX[TI0.z]"),
        ("t_em", "TEX[TI0.w]"),
        ("t_tile", "TEX[TI1.x]"),
        ("t_wear", "TEX[TI1.y]"),
        ("t_primer", "TEX[TI1.z]"),
        ("t_dust", "TEX[TI1.w]"),
    ] {
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
        f.free(c);
        assert!(f.whole());
        assert!(f.alloc(101).is_none());
    }

    #[test]
    fn ident_replace_is_whole_word() {
        let s = "textureSample(t_em, s, uv) + t_emx + my_t_em + textureDimensions(t_em)";
        assert_eq!(
            replace_ident(s, "t_em", "TEX[TI0.w]"),
            "textureSample(TEX[TI0.w], s, uv) + t_emx + my_t_em + textureDimensions(TEX[TI0.w])"
        );
    }

    #[test]
    fn map_shader_parses_and_validates() {
        let src = shader(include_str!("atmo_common.wgsl"), include_str!("shader.wgsl")).expect("transform");
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        // every texture of the material group now reads the page
        for t in ["t_base", "t_nrm", "t_pk", "t_em", "t_tile", "t_wear", "t_primer", "t_dust"] {
            assert!(!src.contains(&format!("{t},")) && !src.contains(&format!("{t})")), "{t} left");
        }
    }
}
