#![allow(dead_code)]
//! GPU renderer shared by the model viewer and the 3D map: offscreen MSAA
//! colour + depth target shown as an egui texture, PBR-lite mesh pipeline
//! (skinned, instanced), wireframe, debug lines, procedural sky and
//! screenshot readback.
use crate::glb;
use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

pub const COLOR_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub const DEPTH_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Depth with stencil (needs the DEPTH32FLOAT_STENCIL8 feature): the stencil
/// marks pixels covered by a real surface, so coarse fallback LODs fill only
/// the pixels nothing else covers.
pub const DEPTH_STENCIL_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32FloatStencil8;
pub const MSAA: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameU {
    pub view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 4],
    pub sun_dir: [f32; 4],
    pub sun_col: [f32; 4],
    pub sky_top: [f32; 4],
    pub sky_hor: [f32; 4],
    pub mode: [u32; 4],
    pub paint1: [f32; 4],
    pub paint2: [f32; 4],
    pub misc: [f32; 4],
    pub misc2: [f32; 4],
    /// x: tint palette on (1) / off (0)
    pub misc3: [f32; 4],
    pub prev_view_proj: [[f32; 4]; 4],
    pub jitter: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MatU {
    base: [f32; 4],
    flags: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance {
    pub m: [[f32; 4]; 4],
    pub extra: [f32; 4],
}

impl Instance {
    pub fn new(m: Mat4, skinned: bool) -> Self {
        Instance {
            m: m.to_cols_array_2d(),
            extra: [if skinned { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        }
    }
    /// A coverage fallback: a coarse LOD drawn near the camera only where no
    /// real surface covers the pixel (extra.w = 1).
    pub fn fallback(mut self) -> Self {
        self.extra[3] = 1.0;
        self
    }
    pub fn is_fallback(&self) -> bool {
        self.extra[3] > 0.5
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineV {
    pub pos: [f32; 3],
    pub col: [u8; 4],
}

/// Lighting and view settings for one frame.
#[derive(Clone)]
pub struct Look {
    pub mode: u32,
    pub rma_order: u32,
    pub finish: u32,
    pub slot: u32,
    pub paint: [f32; 3],
    pub paint2nd: [f32; 3],
    pub pearl: [f32; 3],
    pub pearl_amount: f32,
    pub sun_dir: Vec3,
    pub sun_intensity: f32,
    pub sun_col: [f32; 3],
    pub sky_top: [f32; 3],
    pub sky_hor: [f32; 3],
    pub exposure: f32,
    pub ambient: f32,
    pub wet: f32,
    pub emissive: f32,
    pub fog: f32,
    pub wire: bool,
    /// Multiply the model's tint palette colour (GLB COLOR_0) into the base colour.
    pub tint: bool,
    pub clear: [f32; 3],
}

impl Default for Look {
    fn default() -> Self {
        Look {
            mode: 0,
            // "ignore packed": the packed-data map's channel order is unverified and
            // typical maps (R=G=0, B~0.5) read as R-roughness made every surface a
            // mirror of the sky (washed-out, bluish look). Selectable in the UI.
            rma_order: 3,
            finish: 0,
            slot: 0,
            paint: [0.55, 0.02, 0.02],
            paint2nd: [0.6, 0.6, 0.62],
            pearl: [0.9, 0.8, 0.3],
            pearl_amount: 0.0,
            sun_dir: Vec3::new(0.45, 0.75, 0.35).normalize(),
            sun_intensity: 1.0,
            sun_col: [1.0, 0.95, 0.88],
            sky_top: [0.25, 0.45, 0.85],
            sky_hor: [0.75, 0.82, 0.9],
            exposure: 1.0,
            ambient: 0.9,
            wet: 0.0,
            emissive: 2.0,
            fog: 0.0,
            wire: false,
            tint: true,
            clear: [0.6, 0.65, 0.7],
        }
    }
}

impl Look {
    /// Time of day 0..24 h: sun direction, colours and sky.
    pub fn set_time_of_day(&mut self, hours: f32) {
        let a = (hours - 6.0) / 12.0 * std::f32::consts::PI; // 0 at 6h, pi at 18h
        let el = a.sin();
        self.sun_dir = Vec3::new(a.cos() * 0.8, el.max(-0.3), 0.35).normalize();
        let day = el.clamp(0.0, 1.0);
        let dusk = (1.0 - (el.abs() * 4.0).min(1.0)).max(0.0);
        self.sun_intensity = (el * 3.0).clamp(0.0, 1.0);
        self.sun_col = [1.0, 0.75 + 0.2 * day, 0.55 + 0.4 * day];
        let night = [0.01, 0.015, 0.04];
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]
        };
        self.sky_top = lerp(night, [0.25, 0.45, 0.85], day.sqrt());
        self.sky_hor = lerp(
            lerp(night, [0.75, 0.82, 0.9], day.sqrt()),
            [0.95, 0.55, 0.3],
            dusk * 0.7,
        );
        self.ambient = 0.25 + 0.5 * day.sqrt();
        self.emissive = 1.5 + 6.0 * (1.0 - day);
        self.exposure = 1.0 + 1.5 * (1.0 - day.sqrt());
        if el < 0.0 {
            // moonlight from the opposite side
            self.sun_dir = Vec3::new(-a.cos() * 0.6, (-el).max(0.35), -0.3).normalize();
            self.sun_intensity = 0.35;
            self.sun_col = [0.45, 0.55, 0.85];
            self.ambient = 0.6;
        }
    }
}

pub struct Camera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub fly: bool,
    pub eye: Vec3,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: Vec3::ZERO,
            yaw: 0.6,
            pitch: 0.35,
            dist: 10.0,
            fly: false,
            eye: Vec3::new(0.0, 2.0, 10.0),
            fov: 45f32.to_radians(),
            near: 0.05,
            far: 5000.0,
        }
    }
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.pitch.cos() * self.yaw.sin(),
            -self.pitch.sin(),
            -self.pitch.cos() * self.yaw.cos(),
        )
    }
    pub fn eye(&self) -> Vec3 {
        if self.fly {
            self.eye
        } else {
            self.target - self.forward() * self.dist
        }
    }
    pub fn view(&self) -> Mat4 {
        let e = self.eye();
        Mat4::look_at_rh(e, e + self.forward(), Vec3::Y)
    }
    pub fn proj(&self, aspect: f32) -> Mat4 {
        // Reversed Z (near -> 1, far -> 0) with a 32-bit float depth buffer:
        // near-uniform precision out to the far plane, so coplanar and distant
        // surfaces do not fight.
        Mat4::perspective_rh(self.fov, aspect, self.far, self.near)
    }
    /// Orbit / fly controls from an egui response.
    pub fn handle(&mut self, ui: &egui::Ui, r: &egui::Response) {
        let d = r.drag_delta();
        if r.dragged_by(egui::PointerButton::Primary) {
            self.yaw += d.x * 0.008;
            self.pitch = (self.pitch + d.y * 0.008).clamp(-1.55, 1.55);
        }
        if r.dragged_by(egui::PointerButton::Secondary) || r.dragged_by(egui::PointerButton::Middle)
        {
            let right = self.forward().cross(Vec3::Y).normalize_or_zero();
            let up = right.cross(self.forward());
            let s = if self.fly { 0.05 } else { self.dist * 0.0015 };
            let m = -right * d.x * s + up * d.y * s;
            self.target += m;
            self.eye += m;
        }
        if r.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                if self.fly {
                    self.eye += self.forward() * scroll * 0.05;
                } else {
                    self.dist = (self.dist * (1.0 - scroll * 0.0015)).clamp(0.05, 50000.0);
                }
            }
            if self.fly {
                let dt = ui.input(|i| i.stable_dt).min(0.1);
                let fast = if ui.input(|i| i.modifiers.shift) {
                    6.0
                } else {
                    1.0
                };
                let sp = self.near.max(0.05) * 400.0 * dt * fast;
                let f = self.forward();
                let right = f.cross(Vec3::Y).normalize_or_zero();
                ui.input(|i| {
                    if i.key_down(egui::Key::W) {
                        self.eye += f * sp;
                    }
                    if i.key_down(egui::Key::S) {
                        self.eye -= f * sp;
                    }
                    if i.key_down(egui::Key::A) {
                        self.eye -= right * sp;
                    }
                    if i.key_down(egui::Key::D) {
                        self.eye += right * sp;
                    }
                    if i.key_down(egui::Key::E) {
                        self.eye += Vec3::Y * sp;
                    }
                    if i.key_down(egui::Key::Q) {
                        self.eye -= Vec3::Y * sp;
                    }
                });
            }
        }
    }
    pub fn frame_bbox(&mut self, lo: Vec3, hi: Vec3) {
        self.target = (lo + hi) * 0.5;
        let r = ((hi - lo).length() * 0.5).max(0.1);
        self.dist = r / (self.fov * 0.5).tan() * 1.1;
        self.near = (r * 0.002).max(0.01);
        self.far = (r * 50.0).max(1000.0);
        self.eye = self.eye();
    }
}

struct Target {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolve: wgpu::Texture,
    resolve_view: wgpu::TextureView,
    egui_id: egui::TextureId,
}

pub struct GpuMesh {
    /// Vertex and index data: byte ranges of buffers shared by the model's meshes.
    pub vb: wgpu::Buffer,
    pub ib: wgpu::Buffer,
    pub vb_range: std::ops::Range<u64>,
    pub ib_range: std::ops::Range<u64>,
    pub n_idx: u32,
    pub eb: Option<wgpu::Buffer>,
    pub n_edge: u32,
    pub material: usize,
    pub skinned: bool,
    pub centre: Vec3,
    pub visible: bool,
}

pub struct GpuModel {
    pub meshes: Vec<GpuMesh>,
    pub mat_bg: Vec<wgpu::BindGroup>,
    pub mat_alpha: Vec<u32>,
    pub mat_class: Vec<u32>,
    /// Decal layer order per material (None = not a decal).
    pub mat_decal: Vec<Option<u8>>,
    /// All material uniforms in one buffer, MAT_STRIDE bytes apart.
    mat_ub: wgpu::Buffer,
    mat_cpu: Vec<MatU>,
    _textures: Vec<wgpu::Texture>,
    pub joints_buf: wgpu::Buffer,
    pub frame_bg: wgpu::BindGroup,
    pub instances: wgpu::Buffer,
    pub n_instances: u32,
    /// The last `n_fb` instances are coverage fallbacks (see `Instance::fallback`).
    pub n_fb: u32,
    pub instance_cap: u32,
    pub bbox: [Vec3; 2],
    /// Stencil value its opaque surfaces write: 1 = real surface, 0 = far
    /// field (terrain, ocean), which coverage fallbacks may draw over.
    pub stencil: u32,
    /// Material names and shaders (inspection).
    pub mat_names: Vec<String>,
}

impl GpuModel {
    /// One line per material: name, class, alpha mode, texture bits, base factor.
    pub fn mat_summary(&self) -> Vec<String> {
        self.mat_cpu
            .iter()
            .enumerate()
            .map(|(k, u)| {
                format!(
                    "{} class {} alpha {} has {:#x} base {:?}",
                    self.mat_names.get(k).map(|s| s.as_str()).unwrap_or("?"),
                    u.flags[1],
                    u.flags[2],
                    u.flags[0],
                    u.base
                )
            })
            .collect()
    }

    /// Instances drawn normally (the fallbacks come after them).
    pub fn n_normal(&self) -> u32 {
        self.n_instances - self.n_fb.min(self.n_instances)
    }
}

/// Spacing of the material uniforms in a model's buffer (the uniform offset alignment).
const MAT_STRIDE: u64 = 256;

impl GpuMesh {
    pub fn vslice(&self) -> wgpu::BufferSlice<'_> {
        self.vb.slice(self.vb_range.clone())
    }
    pub fn islice(&self) -> wgpu::BufferSlice<'_> {
        self.ib.slice(self.ib_range.clone())
    }
    /// GPU bytes of this mesh's vertices and indices.
    pub fn bytes(&self) -> u64 {
        (self.vb_range.end - self.vb_range.start) + (self.ib_range.end - self.ib_range.start)
    }
}

pub fn class_id(c: &str) -> u32 {
    match c {
        "opaque" => 1,
        "alpha" => 2,
        "cutout" => 3,
        "decal" => 4,
        "glass" => 5,
        "window" => 6,
        "emissive" => 7,
        "water" => 8,
        "terrain_multilayer" => 9,
        "car_paint" => 10,
        "map_tile" => 11,
        _ => 1,
    }
}

pub struct Renderer3D {
    pipe_opaque: wgpu::RenderPipeline,
    pipe_blend: wgpu::RenderPipeline,
    /// Decal-bucket materials: depth-biased toward the camera so coplanar
    /// overlays win against the surface under them.
    pipe_decal: wgpu::RenderPipeline,
    pipe_decal_blend: wgpu::RenderPipeline,
    pub decal_bias: bool,
    pipe_wire: wgpu::RenderPipeline,
    pipe_lines: wgpu::RenderPipeline,
    pipe_lines_top: wgpu::RenderPipeline,
    pipe_sky: wgpu::RenderPipeline,
    bgl_frame: wgpu::BindGroupLayout,
    bgl_mat: wgpu::BindGroupLayout,
    frame_ub: wgpu::Buffer,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    dummy_joints: wgpu::Buffer,
    pub default_frame_bg: wgpu::BindGroup,
    target: Option<Target>,
    /// Draw calls in the last frame.
    pub draws: u32,
    /// Map cells: opaque, decal, decal blend, blend pipelines.
    pipe_cell: [wgpu::RenderPipeline; 4],
    /// Coverage fallbacks (models, cells): depth-tested, drawn only where
    /// the stencil is still 0. Without stencil support they draw plainly.
    pipe_fb: wgpu::RenderPipeline,
    pipe_cell_fb: wgpu::RenderPipeline,
    depth_fmt: wgpu::TextureFormat,
    bgl_cell: wgpu::BindGroupLayout,
    /// Cells to draw this frame, each with the batches that are in range.
    pub cells: Vec<(std::sync::Arc<CellGpu>, Vec<u16>)>,
    /// LOD distance scale of the cell ranges (detail setting).
    pub lod_k: f32,
    /// Map view: building glass drawn dark and mostly opaque (misc3.z).
    pub map_glass: bool,
    /// Draw calls of the coverage fallbacks in the last frame: models, cells.
    pub fb_draws: [u32; 2],
}

/// A map cell on the GPU (see cells.rs).
pub struct CellGpu {
    pub vb: wgpu::Buffer,
    pub ib: wgpu::Buffer,
    /// One instance: the cell origin.
    pub inst: wgpu::Buffer,
    pub chunk_bg: Vec<wgpu::BindGroup>,
    _tex: Vec<wgpu::Texture>,
    pub batches: Vec<crate::cells::Batch>,
    pub groups: Vec<crate::cells::Group>,
    pub bytes: u64,
    /// Any placement with a coverage-fallback range (table slot 5).
    pub has_fb: bool,
}

pub fn tex_rgba(device: &wgpu::Device, queue: &wgpu::Queue, img: &glb::Image) -> wgpu::Texture {
    if let Some(bc) = &img.bc {
        return tex_bc(device, queue, img, bc);
    }
    let (w, h, _) = &img.mips[0];
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("model tex"),
        size: wgpu::Extent3d {
            width: *w,
            height: *h,
            depth_or_array_layers: 1,
        },
        mip_level_count: img.mips.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if img.srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (l, (mw, mh, px)) in img.mips.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &t,
                mip_level: l as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            px,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * mw),
                rows_per_image: Some(*mh),
            },
            wgpu::Extent3d {
                width: *mw,
                height: *mh,
                depth_or_array_layers: 1,
            },
        );
    }
    t
}

/// Upload a BC1 / BC3 chain (level 0 is a multiple of 4; smaller levels are
/// written at their 4x4-block size).
fn tex_bc(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    img: &glb::Image,
    bc: &crate::bc::Bc,
) -> wgpu::Texture {
    let (w, h, _) = &img.mips[0];
    let format = match (bc.alpha, img.srgb) {
        (false, false) => wgpu::TextureFormat::Bc1RgbaUnorm,
        (false, true) => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
        (true, false) => wgpu::TextureFormat::Bc3RgbaUnorm,
        (true, true) => wgpu::TextureFormat::Bc3RgbaUnormSrgb,
    };
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("model tex bc"),
        size: wgpu::Extent3d {
            width: *w,
            height: *h,
            depth_or_array_layers: 1,
        },
        mip_level_count: bc.levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bb = crate::bc::block_bytes(bc.alpha) as u32;
    for (l, data) in bc.levels.iter().enumerate() {
        let (mw, mh) = ((w >> l).max(1), (h >> l).max(1));
        let (bw, bh) = (mw.div_ceil(4), mh.div_ceil(4));
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &t,
                mip_level: l as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bw * bb),
                rows_per_image: Some(bh),
            },
            wgpu::Extent3d {
                width: bw * 4,
                height: bh * 4,
                depth_or_array_layers: 1,
            },
        );
    }
    t
}

impl Renderer3D {
    pub fn new(rs: &egui_wgpu::RenderState) -> Self {
        let device = &rs.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sixng shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let vis = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let bgl_frame = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: vis,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let tex_entry = |b: u32| wgpu::BindGroupLayoutEntry {
            binding: b,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let bgl_mat = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex_entry(1),
                tex_entry(2),
                tex_entry(3),
                tex_entry(4),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[Some(&bgl_frame), Some(&bgl_mat)],
            immediate_size: 0,
        });
        let layout_frame = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("frame only"),
            bind_group_layouts: &[Some(&bgl_frame)],
            immediate_size: 0,
        });
        let vattrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x2, 4 => Float32x2, 5 => Unorm8x4, 6 => Uint16x4, 7 => Unorm8x4, 13 => Unorm8x4];
        let iattrs = wgpu::vertex_attr_array![8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4, 12 => Float32x4];
        let vbufs = [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<glb::Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &vattrs,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Instance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &iattrs,
            }),
        ];
        let lattrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4];
        let lbufs = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineV>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &lattrs,
        })];
        let ms = wgpu::MultisampleState {
            count: MSAA,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };
        let has_stencil = device
            .features()
            .contains(wgpu::Features::DEPTH32FLOAT_STENCIL8);
        let depth_fmt = if has_stencil {
            DEPTH_STENCIL_FMT
        } else {
            DEPTH_FMT
        };
        let face = |compare: wgpu::CompareFunction, pass_op: wgpu::StencilOperation| {
            wgpu::StencilFaceState {
                compare,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op,
            }
        };
        let st_write = if has_stencil {
            let f = face(
                wgpu::CompareFunction::Always,
                wgpu::StencilOperation::Replace,
            );
            wgpu::StencilState {
                front: f,
                back: f,
                read_mask: 0xff,
                write_mask: 0xff,
            }
        } else {
            Default::default()
        };
        let st_fb = if has_stencil {
            let f = face(wgpu::CompareFunction::Equal, wgpu::StencilOperation::Keep);
            wgpu::StencilState {
                front: f,
                back: f,
                read_mask: 0xff,
                write_mask: 0,
            }
        } else {
            Default::default()
        };
        let depth = |write: bool, cmp: wgpu::CompareFunction| {
            Some(wgpu::DepthStencilState {
                format: depth_fmt,
                depth_write_enabled: Some(write),
                depth_compare: Some(cmp),
                stencil: Default::default(),
                bias: Default::default(),
            })
        };
        let depth_st = |st: &wgpu::StencilState| {
            Some(wgpu::DepthStencilState {
                format: depth_fmt,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Greater),
                stencil: st.clone(),
                bias: Default::default(),
            })
        };
        let mkp = |label: &str,
                   layout: &wgpu::PipelineLayout,
                   vs: &str,
                   fs: &str,
                   bufs: &[Option<wgpu::VertexBufferLayout>],
                   topo: wgpu::PrimitiveTopology,
                   blend: Option<wgpu::BlendState>,
                   ds: Option<wgpu::DepthStencilState>,
                   cull: Option<wgpu::Face>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: bufs,
                },
                primitive: wgpu::PrimitiveState {
                    topology: topo,
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: ds,
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FMT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let mk = |label: &str,
                  layout: &wgpu::PipelineLayout,
                  vs: &str,
                  fs: &str,
                  bufs: &[Option<wgpu::VertexBufferLayout>],
                  topo: wgpu::PrimitiveTopology,
                  blend: Option<wgpu::BlendState>,
                  ds: Option<wgpu::DepthStencilState>| {
            mkp(label, layout, vs, fs, bufs, topo, blend, ds, None)
        };
        let pipe_opaque = mk(
            "opaque",
            &layout,
            "vs_mesh",
            "fs_mesh",
            &vbufs,
            wgpu::PrimitiveTopology::TriangleList,
            None,
            depth_st(&st_write),
        );
        let pipe_fb = mkp(
            "fallback",
            &layout,
            "vs_mesh",
            "fs_mesh",
            &vbufs,
            wgpu::PrimitiveTopology::TriangleList,
            None,
            depth_st(&st_fb),
            Some(wgpu::Face::Back),
        );
        let biased = |write: bool| {
            Some(wgpu::DepthStencilState {
                format: depth_fmt,
                depth_write_enabled: Some(write),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                // reversed Z: positive = toward the camera
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 0.25,
                    clamp: 6.0e-5,
                },
            })
        };
        let pipe_decal = mk(
            "decal",
            &layout,
            "vs_mesh",
            "fs_mesh",
            &vbufs,
            wgpu::PrimitiveTopology::TriangleList,
            None,
            biased(true),
        );
        let pipe_decal_blend = mk(
            "decal blend",
            &layout,
            "vs_mesh",
            "fs_mesh",
            &vbufs,
            wgpu::PrimitiveTopology::TriangleList,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            biased(false),
        );
        let pipe_blend = mk(
            "blend",
            &layout,
            "vs_mesh",
            "fs_mesh",
            &vbufs,
            wgpu::PrimitiveTopology::TriangleList,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            depth(false, wgpu::CompareFunction::Greater),
        );
        let pipe_wire = mk(
            "wire",
            &layout,
            "vs_wire",
            "fs_wire",
            &vbufs,
            wgpu::PrimitiveTopology::LineList,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            depth(false, wgpu::CompareFunction::GreaterEqual),
        );
        let pipe_lines = mk(
            "lines",
            &layout_frame,
            "vs_line",
            "fs_line",
            &lbufs,
            wgpu::PrimitiveTopology::LineList,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            depth(false, wgpu::CompareFunction::GreaterEqual),
        );
        let pipe_lines_top = mk(
            "lines top",
            &layout_frame,
            "vs_line",
            "fs_line",
            &lbufs,
            wgpu::PrimitiveTopology::LineList,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            depth(false, wgpu::CompareFunction::Always),
        );
        let pipe_sky = mk(
            "sky",
            &layout_frame,
            "vs_sky",
            "fs_sky",
            &[],
            wgpu::PrimitiveTopology::TriangleList,
            None,
            depth(false, wgpu::CompareFunction::Always),
        );
        let frame_ub = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame ub"),
            size: std::mem::size_of::<FrameU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tex"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let wt = tex_rgba(
            device,
            &rs.queue,
            &glb::Image {
                key: "white".into(),
                mips: vec![(1, 1, vec![255; 4])],
                srgb: false,
                bc: None,
            },
        );
        let white = wt.create_view(&Default::default());
        let ident = Mat4::IDENTITY.to_cols_array();
        let dummy_joints = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("joints"),
            contents: bytemuck::cast_slice(&ident),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let default_frame_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame bg"),
            layout: &bgl_frame,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame_ub.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dummy_joints.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        std::mem::forget(wt); // the 1x1 white texture lives for the process
                              // map cells
        let bgl_cell = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cell"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layout_cell = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cell"),
            bind_group_layouts: &[Some(&bgl_frame), Some(&bgl_cell)],
            immediate_size: 0,
        });
        let cattrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float16x2, 2 => Snorm8x4, 3 => Unorm8x4, 4 => Unorm8x4, 5 => Uint16x2];
        let ciattrs = wgpu::vertex_attr_array![8 => Float32x4];
        let cbufs = [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<crate::cells::CellV>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &cattrs,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: 16,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &ciattrs,
            }),
        ];
        let pipe_cell = [
            mk(
                "cell opaque",
                &layout_cell,
                "vs_cell",
                "fs_cell",
                &cbufs,
                wgpu::PrimitiveTopology::TriangleList,
                None,
                depth_st(&st_write),
            ),
            mk(
                "cell decal",
                &layout_cell,
                "vs_cell",
                "fs_cell",
                &cbufs,
                wgpu::PrimitiveTopology::TriangleList,
                None,
                biased(true),
            ),
            mk(
                "cell decal blend",
                &layout_cell,
                "vs_cell",
                "fs_cell",
                &cbufs,
                wgpu::PrimitiveTopology::TriangleList,
                Some(wgpu::BlendState::ALPHA_BLENDING),
                biased(false),
            ),
            mk(
                "cell blend",
                &layout_cell,
                "vs_cell",
                "fs_cell",
                &cbufs,
                wgpu::PrimitiveTopology::TriangleList,
                Some(wgpu::BlendState::ALPHA_BLENDING),
                depth(false, wgpu::CompareFunction::Greater),
            ),
        ];
        let pipe_cell_fb = mkp(
            "cell fallback",
            &layout_cell,
            "vs_cell_fb",
            "fs_cell",
            &cbufs,
            wgpu::PrimitiveTopology::TriangleList,
            None,
            depth_st(&st_fb),
            Some(wgpu::Face::Back),
        );
        Renderer3D {
            pipe_fb,
            pipe_cell_fb,
            depth_fmt,
            pipe_opaque,
            pipe_blend,
            pipe_decal,
            pipe_decal_blend,
            decal_bias: true,
            pipe_wire,
            pipe_lines,
            pipe_lines_top,
            pipe_sky,
            bgl_frame,
            bgl_mat,
            frame_ub,
            sampler,
            white,
            dummy_joints,
            default_frame_bg,
            target: None,
            draws: 0,
            pipe_cell,
            bgl_cell,
            cells: Vec::new(),
            lod_k: 1.0,
            map_glass: false,
            fb_draws: [0, 0],
        }
    }

    /// Upload a CPU model (meshes, materials, textures, skeleton).
    pub fn upload(
        &self,
        rs: &egui_wgpu::RenderState,
        m: &glb::Model,
        overrides: &[Option<[f32; 4]>],
    ) -> GpuModel {
        self.upload_shared(rs, m, overrides, None)
    }

    /// Upload with a texture cache shared between models (the 3D map): an
    /// image whose key is cached reuses that texture; images that arrive
    /// without pixels (skipped by the loader) must be in the cache.
    pub fn upload_shared(
        &self,
        rs: &egui_wgpu::RenderState,
        m: &glb::Model,
        overrides: &[Option<[f32; 4]>],
        mut cache: Option<&mut std::collections::HashMap<String, wgpu::Texture>>,
    ) -> GpuModel {
        let (device, queue) = (&rs.device, &rs.queue);
        self.uploader().upload_with(
            device,
            queue,
            m,
            overrides,
            &mut |i| match cache.as_deref_mut() {
                Some(c) => match c.get(&i.key) {
                    Some(t) => Some(t.clone()),
                    None if has_pixels(i) => {
                        let t = tex_rgba(device, queue, i);
                        c.insert(i.key.clone(), t.clone());
                        Some(t)
                    }
                    None => None,
                },
                None => has_pixels(i).then(|| tex_rgba(device, queue, i)),
            },
        )
    }

    /// What uploading needs, for loader threads (wgpu objects can be made on
    /// any thread).
    pub fn uploader(&self) -> Uploader {
        Uploader {
            bgl_mat: self.bgl_mat.clone(),
            bgl_frame: self.bgl_frame.clone(),
            frame_ub: self.frame_ub.clone(),
            sampler: self.sampler.clone(),
            white: self.white.clone(),
            bgl_cell: self.bgl_cell.clone(),
        }
    }
}

/// An image with pixels to upload (not one skipped as already on the GPU).
pub fn has_pixels(i: &glb::Image) -> bool {
    !i.mips.is_empty() && (i.bc.is_some() || !i.mips[0].2.is_empty())
}

/// Render timing totals (µs): pass recording (incl. pass end), encoder
/// finish, submit; then frames.
pub static RTIME: [std::sync::atomic::AtomicU64; 4] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 4];

#[derive(Clone)]
pub struct Uploader {
    bgl_mat: wgpu::BindGroupLayout,
    bgl_frame: wgpu::BindGroupLayout,
    frame_ub: wgpu::Buffer,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    bgl_cell: wgpu::BindGroupLayout,
}

impl Uploader {
    /// Upload a map cell: geometry, origin and its BC3 texture arrays.
    pub fn upload_cell(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        c: &crate::cells::CellCpu,
    ) -> CellGpu {
        use crate::cells::{mip_bytes, LAYER_MIPS, LAYER_PX};
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cell vb"),
            contents: if c.verts.is_empty() {
                &[0u8; 32]
            } else {
                bytemuck::cast_slice(&c.verts)
            },
            usage: wgpu::BufferUsages::VERTEX,
        });
        let eb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cell ents"),
            contents: if c.ents.is_empty() {
                &[0u8; 32]
            } else {
                bytemuck::cast_slice(&c.ents)
            },
            usage: wgpu::BufferUsages::STORAGE,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cell ib"),
            contents: if c.indices.is_empty() {
                &[0u8; 4]
            } else {
                bytemuck::cast_slice(&c.indices)
            },
            usage: wgpu::BufferUsages::INDEX,
        });
        let o = [c.origin[0], c.origin[1], c.origin[2], 0.0f32];
        let inst = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cell origin"),
            contents: bytemuck::cast_slice(&o),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut tex = Vec::new();
        let mut chunk_bg = Vec::new();
        let mut bytes = (c.verts.len() * std::mem::size_of::<crate::cells::CellV>()
            + c.indices.len() * 4
            + c.ents.len() * 32) as u64;
        for ch in &c.chunks {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("cell tex"),
                size: wgpu::Extent3d {
                    width: LAYER_PX,
                    height: LAYER_PX,
                    depth_or_array_layers: ch.layers,
                },
                mip_level_count: LAYER_MIPS,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bc3RgbaUnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let mut off = 0usize;
            for l in 0..LAYER_MIPS {
                let n = mip_bytes(l) * ch.layers as usize;
                let bw = ((LAYER_PX >> l).max(1)).div_ceil(4);
                if let Some(data) = ch.data.get(off..off + n) {
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &t,
                            mip_level: l,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(bw * 16),
                            rows_per_image: Some(bw),
                        },
                        wgpu::Extent3d {
                            width: bw * 4,
                            height: bw * 4,
                            depth_or_array_layers: ch.layers,
                        },
                    );
                }
                off += n;
            }
            bytes += ch.data.len() as u64;
            let view = t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
            chunk_bg.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cell bg"),
                layout: &self.bgl_cell,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: eb.as_entire_binding(),
                    },
                ],
            }));
            tex.push(t);
        }
        CellGpu {
            vb,
            ib,
            inst,
            chunk_bg,
            _tex: tex,
            batches: c.batches.clone(),
            groups: c.groups.clone(),
            bytes,
            has_fb: c.ents.iter().any(|e| e[5] > 0.5),
        }
    }

    /// Upload a model; `get_tex` gives each image's texture (shared or new).
    pub fn upload_with(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        m: &glb::Model,
        overrides: &[Option<[f32; 4]>],
        get_tex: &mut dyn FnMut(&glb::Image) -> Option<wgpu::Texture>,
    ) -> GpuModel {
        let mut textures: Vec<wgpu::Texture> = Vec::new();
        let mut views: Vec<Option<wgpu::TextureView>> = Vec::new();
        for i in &m.images {
            let t = get_tex(i);
            views.push(t.as_ref().map(|t| t.create_view(&Default::default())));
            if let Some(t) = t {
                textures.push(t);
            }
        }
        let view = |i: Option<usize>| {
            i.and_then(|i| views.get(i))
                .and_then(|v| v.as_ref())
                .unwrap_or(&self.white)
        };
        let mut mat_bg = Vec::new();
        let mut mat_cpu = Vec::new();
        let mut mat_alpha = Vec::new();
        let mut mat_class = Vec::new();
        let mut mat_decal = Vec::new();
        let default = glb::Material {
            name: "default".into(),
            class: "opaque".into(),
            alpha_mode: "OPAQUE".into(),
            base_factor: [0.7, 0.7, 0.7, 1.0],
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
        let mats: Vec<&glb::Material> = if m.materials.is_empty() {
            vec![&default]
        } else {
            m.materials.iter().collect()
        };
        let mat_ub = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mat ub"),
            size: MAT_STRIDE * mats.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut mat_bytes = vec![0u8; (MAT_STRIDE as usize) * mats.len()];
        for (k, mt) in mats.iter().enumerate() {
            let has = (mt.base.is_some() as u32)
                | ((mt.normal.is_some() as u32) << 1)
                | ((mt.packed.is_some() as u32) << 2)
                | ((mt.emissive.is_some() as u32) << 3);
            let alpha = match mt.alpha_mode.as_str() {
                "MASK" => 1,
                "BLEND" => 2,
                _ => 0,
            };
            let gap = match mt.gap.as_str() {
                "flat" => 2,
                "lowmip" => 1,
                _ => 0,
            };
            let base = overrides
                .get(k)
                .copied()
                .flatten()
                .unwrap_or(mt.base_factor);
            // w: gap in bits 0-1; a stand-in surface adds bit 8 and its
            // metallic / roughness factors in bits 16-23 / 24-31; bit 9 =
            // the base texture is a tint mask
            let mr = mt.mr.map_or(0, |[m, r]| {
                let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
                0x100 | (q(m) << 16) | (q(r) << 24)
            }) | if mt.tint_mask { 0x200 } else { 0 } | if mt.parts { 0x400 } else { 0 };
            let u = MatU {
                base,
                flags: [has, class_id(&mt.class), alpha, gap | mr],
            };
            let o = k * MAT_STRIDE as usize;
            mat_bytes[o..o + std::mem::size_of::<MatU>()].copy_from_slice(bytemuck::bytes_of(&u));
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mat bg"),
                layout: &self.bgl_mat,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &mat_ub,
                            offset: o as u64,
                            size: wgpu::BufferSize::new(std::mem::size_of::<MatU>() as u64),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(view(mt.base)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(view(mt.normal)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(view(mt.packed)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(view(mt.emissive)),
                    },
                ],
            });
            mat_bg.push(bg);
            mat_cpu.push(u);
            mat_alpha.push(alpha);
            mat_class.push(class_id(&mt.class));
            mat_decal.push(mt.decal);
        }
        queue.write_buffer(&mat_ub, 0, &mat_bytes);
        // one vertex and one index buffer for all meshes (fewer GPU objects:
        // creating them is what costs when a model has many parts)
        let vsz = std::mem::size_of::<glb::Vertex>() as u64;
        let nv: u64 = m.meshes.iter().map(|me| me.verts.len() as u64).sum();
        let ni: u64 = m.meshes.iter().map(|me| me.indices.len() as u64).sum();
        // sizes rounded up to 4 bytes, as mapped buffers need
        let vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vb"),
            size: (nv * vsz).max(vsz).next_multiple_of(4),
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: true,
        });
        let ib = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ib"),
            size: (ni * 4).max(4),
            usage: wgpu::BufferUsages::INDEX,
            mapped_at_creation: true,
        });
        let mut meshes = Vec::with_capacity(m.meshes.len());
        {
            let mut vmap = vb
                .slice(..)
                .get_mapped_range_mut()
                .expect("vertex buffer was mapped at creation");
            let mut imap = ib
                .slice(..)
                .get_mapped_range_mut()
                .expect("index buffer was mapped at creation");
            let (mut vo, mut io) = (0u64, 0u64);
            for me in &m.meshes {
                let vbytes: &[u8] = bytemuck::cast_slice(&me.verts);
                let ibytes: &[u8] = bytemuck::cast_slice(&me.indices);
                vmap.slice(vo as usize..vo as usize + vbytes.len())
                    .copy_from_slice(vbytes);
                imap.slice(io as usize..io as usize + ibytes.len())
                    .copy_from_slice(ibytes);
                let (vr, ir) = (vo..vo + vbytes.len() as u64, io..io + ibytes.len() as u64);
                vo += vbytes.len() as u64;
                io += ibytes.len() as u64;
                // an empty range is not a valid slice: point it at the start
                let vr = if vr.is_empty() { 0..vsz } else { vr };
                let ir = if ir.is_empty() { 0..4 } else { ir };
                meshes.push(GpuMesh {
                    vb: vb.clone(),
                    ib: ib.clone(),
                    vb_range: vr,
                    ib_range: ir,
                    n_idx: me.indices.len() as u32,
                    eb: None,
                    n_edge: 0,
                    material: me.material.unwrap_or(0).min(mats.len() - 1),
                    skinned: me.skinned,
                    centre: Vec3::from(me.centre),
                    visible: true,
                });
            }
        }
        vb.unmap();
        ib.unmap();
        let nj = m.joints.len().max(1);
        let joints_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("joints"),
            size: (nj * 64) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ident: Vec<f32> = (0..nj)
            .flat_map(|_| Mat4::IDENTITY.to_cols_array())
            .collect();
        queue.write_buffer(&joints_buf, 0, bytemuck::cast_slice(&ident));
        let frame_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame bg model"),
            layout: &self.bgl_frame,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.frame_ub.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: joints_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let inst = [Instance::new(Mat4::IDENTITY, !m.joints.is_empty())];
        let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("inst"),
            contents: bytemuck::cast_slice(&inst),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        GpuModel {
            meshes,
            mat_bg,
            mat_alpha,
            mat_class,
            mat_decal,
            mat_ub,
            mat_cpu,
            _textures: textures,
            joints_buf,
            frame_bg,
            instances,
            n_instances: 1,
            n_fb: 0,
            instance_cap: 1,
            bbox: [Vec3::from(m.bbox[0]), Vec3::from(m.bbox[1])],
            stencil: 1,
            mat_names: mats
                .iter()
                .map(|mt| format!("{} [{}] {}", mt.name, mt.shader_hash, mt.class))
                .collect(),
        }
    }
}

impl Renderer3D {
    /// Build wireframe edge buffers (on first use).
    pub fn ensure_edges(&self, rs: &egui_wgpu::RenderState, gm: &mut GpuModel, m: &glb::Model) {
        for (g, me) in gm.meshes.iter_mut().zip(&m.meshes) {
            if g.eb.is_some() {
                continue;
            }
            let mut e: Vec<u32> = Vec::with_capacity(me.indices.len() * 2);
            for t in me.indices.chunks_exact(3) {
                e.extend_from_slice(&[t[0], t[1], t[1], t[2], t[2], t[0]]);
            }
            g.n_edge = e.len() as u32;
            g.eb = Some(
                rs.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("edges"),
                        contents: bytemuck::cast_slice(&e),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
            );
        }
    }

    pub fn set_instances(&self, rs: &egui_wgpu::RenderState, gm: &mut GpuModel, inst: &[Instance]) {
        if inst.len() as u32 > gm.instance_cap {
            let cap = inst.len();
            gm.instances = rs.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("inst"),
                size: (cap * std::mem::size_of::<Instance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gm.instance_cap = cap as u32;
        }
        let n_fb = inst.iter().filter(|i| i.is_fallback()).count();
        if n_fb == 0 || n_fb == inst.len() {
            rs.queue
                .write_buffer(&gm.instances, 0, bytemuck::cast_slice(inst));
        } else {
            // normal instances first, fallbacks after them
            let v: Vec<Instance> = inst
                .iter()
                .filter(|i| !i.is_fallback())
                .chain(inst.iter().filter(|i| i.is_fallback()))
                .copied()
                .collect();
            rs.queue
                .write_buffer(&gm.instances, 0, bytemuck::cast_slice(&v));
        }
        gm.n_instances = inst.len() as u32;
        gm.n_fb = n_fb as u32;
    }

    pub fn set_joints(&self, rs: &egui_wgpu::RenderState, gm: &GpuModel, mats: &[Mat4]) {
        let v: Vec<f32> = mats.iter().flat_map(|m| m.to_cols_array()).collect();
        rs.queue
            .write_buffer(&gm.joints_buf, 0, bytemuck::cast_slice(&v));
    }

    /// Change a material's base factor (e.g. after a paint override).
    pub fn set_material_base(
        &self,
        rs: &egui_wgpu::RenderState,
        gm: &mut GpuModel,
        k: usize,
        base: [f32; 4],
    ) {
        if let Some(u) = gm.mat_cpu.get_mut(k) {
            u.base = base;
            rs.queue
                .write_buffer(&gm.mat_ub, k as u64 * MAT_STRIDE, bytemuck::bytes_of(u));
        }
    }

    /// Use the secondary paint colour on a car-paint material.
    pub fn set_material_secondary(
        &self,
        rs: &egui_wgpu::RenderState,
        gm: &mut GpuModel,
        k: usize,
        on: bool,
    ) {
        if let Some(u) = gm.mat_cpu.get_mut(k) {
            u.flags[0] = if on {
                u.flags[0] | 16
            } else {
                u.flags[0] & !16
            };
            rs.queue
                .write_buffer(&gm.mat_ub, k as u64 * MAT_STRIDE, bytemuck::bytes_of(u));
        }
    }

    pub fn is_secondary(gm: &GpuModel, k: usize) -> bool {
        gm.mat_cpu
            .get(k)
            .map(|u| u.flags[0] & 16 != 0)
            .unwrap_or(false)
    }

    fn ensure_target(&mut self, rs: &egui_wgpu::RenderState, size: [u32; 2]) {
        if self
            .target
            .as_ref()
            .map(|t| t.size == size)
            .unwrap_or(false)
        {
            return;
        }
        let device = &rs.device;
        let mk = |samples: u32, fmt: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("viewer target"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: fmt,
                usage,
                view_formats: &[],
            })
        };
        let msaa = mk(MSAA, COLOR_FMT, wgpu::TextureUsages::RENDER_ATTACHMENT)
            .create_view(&Default::default());
        let depth = mk(MSAA, self.depth_fmt, wgpu::TextureUsages::RENDER_ATTACHMENT)
            .create_view(&Default::default());
        let resolve = mk(
            1,
            COLOR_FMT,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        );
        let resolve_view = resolve.create_view(&Default::default());
        let mut r = rs.renderer.write();
        let egui_id = match &self.target {
            Some(t) => {
                r.update_egui_texture_from_wgpu_texture(
                    device,
                    &resolve_view,
                    wgpu::FilterMode::Linear,
                    t.egui_id,
                );
                t.egui_id
            }
            None => r.register_native_texture(device, &resolve_view, wgpu::FilterMode::Linear),
        };
        self.target = Some(Target {
            size,
            msaa,
            depth,
            resolve,
            resolve_view,
            egui_id,
        });
    }

    pub fn frame_uniform(&self, cam: &Camera, look: &Look, aspect: f32, time: f32) -> FrameU {
        let vp = cam.proj(aspect) * cam.view();
        let lin = |c: [f32; 3]| [c[0], c[1], c[2], 1.0];
        FrameU {
            view_proj: vp.to_cols_array_2d(),
            inv_view_proj: vp.inverse().to_cols_array_2d(),
            cam_pos: cam.eye().extend(1.0).to_array(),
            sun_dir: look.sun_dir.extend(look.sun_intensity).to_array(),
            sun_col: lin(look.sun_col),
            sky_top: lin(look.sky_top),
            sky_hor: [
                look.sky_hor[0],
                look.sky_hor[1],
                look.sky_hor[2],
                look.exposure,
            ],
            mode: [look.mode, look.rma_order, look.finish, look.slot],
            paint1: lin(look.paint),
            paint2: [
                look.pearl[0],
                look.pearl[1],
                look.pearl[2],
                look.pearl_amount,
            ],
            misc: [look.wet, look.emissive, time, look.ambient],
            misc2: [
                look.fog,
                look.paint2nd[0],
                look.paint2nd[1],
                look.paint2nd[2],
            ],
            misc3: [
                look.tint as u32 as f32,
                self.lod_k,
                self.map_glass as u32 as f32,
                0.0,
            ],
            prev_view_proj: vp.to_cols_array_2d(),
            jitter: [0.0, 0.0, 0.0, 0.0],
        }
    }

    /// Render models (each with its own instances) and lines into the
    /// offscreen target; returns the egui texture to show.
    /// Opaque and cutout draws of `models` into a render pass.
    /// `fb`: draw the coverage-fallback instances instead of the normal ones.
    fn record_opaque(
        pipe: &wgpu::RenderPipeline,
        default_frame_bg: &wgpu::BindGroup,
        decal_bias: bool,
        be: &mut wgpu::RenderPass<'_>,
        models: &[&GpuModel],
        mode: u32,
        fb: bool,
    ) -> u32 {
        let mut n = 0u32;
        be.set_pipeline(pipe);
        let mut sref = u32::MAX;
        // static models share the default frame bind group (their joints
        // buffer is never read), so it is bound once
        let mut shared_bound = false;
        for gm in models {
            let range = if fb {
                gm.n_normal()..gm.n_instances
            } else {
                0..gm.n_normal()
            };
            if range.is_empty() {
                continue;
            }
            let want = if fb { 0 } else { gm.stencil };
            if sref != want {
                be.set_stencil_reference(want);
                sref = want;
            }
            if gm.meshes.iter().any(|m| m.skinned) {
                be.set_bind_group(0, Some(&gm.frame_bg), &[]);
                shared_bound = false;
            } else if !shared_bound {
                be.set_bind_group(0, Some(default_frame_bg), &[]);
                shared_bound = true;
            }
            be.set_vertex_buffer(1, Some(gm.instances.slice(..)));
            // a model's meshes share one vertex and one index buffer: bind
            // them once and address each mesh by first index / base vertex,
            // and bind a material only when it changes (fewer commands to
            // validate: that is most of the CPU cost of a frame)
            let mut bound: Option<&wgpu::Buffer> = None;
            let mut mat = usize::MAX;
            for me in gm.meshes.iter().filter(|m| m.visible && m.n_idx > 0) {
                let blend = gm.mat_alpha[me.material] == 2
                    || matches!(gm.mat_class[me.material], 5 | 6 | 8);
                if blend && mode == 0 {
                    continue;
                }
                if decal_bias && gm.mat_decal[me.material].is_some() {
                    continue;
                }
                if mat != me.material {
                    be.set_bind_group(1, Some(&gm.mat_bg[me.material]), &[]);
                    mat = me.material;
                }
                if bound != Some(&me.vb) {
                    be.set_vertex_buffer(0, Some(me.vb.slice(..)));
                    be.set_index_buffer(me.ib.slice(..), wgpu::IndexFormat::Uint32);
                    bound = Some(&me.vb);
                }
                let first = (me.ib_range.start / 4) as u32;
                let base = (me.vb_range.start / std::mem::size_of::<glb::Vertex>() as u64) as i32;
                be.draw_indexed(first..first + me.n_idx, base, range.clone());
                n += 1;
            }
        }
        n
    }

    /// Draw the cell batches of one pass (see `cells::PASS_*`).
    fn draw_cells(
        pass: &mut wgpu::RenderPass<'_>,
        pipe: &wgpu::RenderPipeline,
        fbg: &wgpu::BindGroup,
        cells: &[(std::sync::Arc<CellGpu>, Vec<u16>)],
        which: u8,
        fb: bool,
    ) -> u32 {
        let mut n = 0;
        let mut set = false;
        for (c, active) in cells {
            if fb && !c.has_fb {
                continue;
            }
            let mut bound = false;
            for &bi in active {
                let Some(b) = c.batches.get(bi as usize) else {
                    continue;
                };
                if b.pass != which || b.count == 0 {
                    continue;
                }
                if !set {
                    pass.set_pipeline(pipe);
                    pass.set_bind_group(0, fbg, &[]);
                    set = true;
                }
                if !bound {
                    pass.set_vertex_buffer(0, c.vb.slice(..));
                    pass.set_vertex_buffer(1, c.inst.slice(..));
                    pass.set_index_buffer(c.ib.slice(..), wgpu::IndexFormat::Uint32);
                    bound = true;
                }
                pass.set_bind_group(1, &c.chunk_bg[b.chunk as usize], &[]);
                pass.draw_indexed(b.first..b.first + b.count, 0, 0..1);
                n += 1;
            }
        }
        n
    }

    pub fn render(
        &mut self,
        rs: &egui_wgpu::RenderState,
        size: [u32; 2],
        cam: &Camera,
        look: &Look,
        models: &[&GpuModel],
        lines: &[LineV],
        lines_on_top: &[LineV],
    ) -> egui::TextureId {
        let t_r = std::time::Instant::now();
        let size = [size[0].max(16), size[1].max(16)];
        self.ensure_target(rs, size);
        let device = &rs.device;
        let queue = &rs.queue;
        let aspect = size[0] as f32 / size[1] as f32;
        let fu = self.frame_uniform(cam, look, aspect, 0.0);
        queue.write_buffer(&self.frame_ub, 0, bytemuck::bytes_of(&fu));
        let lb = (!lines.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("lines"),
                contents: bytemuck::cast_slice(lines),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let lt = (!lines_on_top.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("lines top"),
                contents: bytemuck::cast_slice(lines_on_top),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let t = self.target.as_ref().unwrap();
        let mut draws = 0u32;
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("viewer"),
        });
        {
            let c = look.clear;
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewer pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.msaa,
                    resolve_target: Some(&t.resolve_view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: c[0] as f64,
                            g: c[1] as f64,
                            b: c[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: (self.depth_fmt == DEPTH_STENCIL_FMT).then_some(
                        wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0),
                            store: wgpu::StoreOp::Discard,
                        },
                    ),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if look.mode == 0 || look.mode == 7 {
                pass.set_pipeline(&self.pipe_sky);
                pass.set_bind_group(0, &self.default_frame_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            // opaque + mask
            // no render bundles: wgpu 24 never frees a bundle, and with it
            // everything it draws (evicted models stayed in GPU memory)
            draws += Self::record_opaque(
                &self.pipe_opaque,
                &self.default_frame_bg,
                self.decal_bias,
                &mut pass,
                models,
                look.mode,
                false,
            );
            pass.set_stencil_reference(1);
            draws += Self::draw_cells(
                &mut pass,
                &self.pipe_cell[0],
                &self.default_frame_bg,
                &self.cells,
                crate::cells::PASS_OPAQUE,
                false,
            );
            // coverage fallbacks: after every real opaque surface, only on
            // pixels still showing far field or sky
            let fm = Self::record_opaque(
                &self.pipe_fb,
                &self.default_frame_bg,
                self.decal_bias,
                &mut pass,
                models,
                0,
                true,
            );
            pass.set_stencil_reference(0);
            let fc = Self::draw_cells(
                &mut pass,
                &self.pipe_cell_fb,
                &self.default_frame_bg,
                &self.cells,
                crate::cells::PASS_OPAQUE,
                true,
            );
            self.fb_draws = [fm, fc];
            draws += fm + fc;
            if self.decal_bias {
                draws += Self::draw_cells(
                    &mut pass,
                    &self.pipe_cell[1],
                    &self.default_frame_bg,
                    &self.cells,
                    crate::cells::PASS_DECAL,
                    false,
                );
                draws += Self::draw_cells(
                    &mut pass,
                    &self.pipe_cell[2],
                    &self.default_frame_bg,
                    &self.cells,
                    crate::cells::PASS_DECAL_BLEND,
                    false,
                );
            }
            if look.mode == 0 {
                draws += Self::draw_cells(
                    &mut pass,
                    &self.pipe_cell[3],
                    &self.default_frame_bg,
                    &self.cells,
                    crate::cells::PASS_BLEND,
                    false,
                );
            }
            // decals: after all opaque surfaces, by layer order; opaque-mode
            // decals (terrain blend layers) write depth, blended ones do not
            if self.decal_bias {
                let eye = cam.eye();
                let mut dl: Vec<(u8, bool, f32, &GpuModel, &GpuMesh)> = Vec::new();
                for gm in models {
                    for me in gm.meshes.iter().filter(|m| m.visible) {
                        if let Some(o) = gm.mat_decal[me.material] {
                            let blend = look.mode == 0
                                && (gm.mat_alpha[me.material] == 2
                                    || matches!(gm.mat_class[me.material], 5 | 6 | 8));
                            dl.push((o, blend, (me.centre - eye).length(), gm, me));
                        }
                    }
                }
                // by layer order (terrain blend layers 0, other decals 1), then
                // opaque before blended, then far to near
                dl.sort_by(|a, b| {
                    a.0.cmp(&b.0)
                        .then(a.1.cmp(&b.1))
                        .then(b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
                });
                let mut cur: Option<bool> = None;
                for (_, blend, _, gm, me) in dl {
                    if cur != Some(blend) {
                        pass.set_pipeline(if blend {
                            &self.pipe_decal_blend
                        } else {
                            &self.pipe_decal
                        });
                        cur = Some(blend);
                    }
                    pass.set_bind_group(0, &gm.frame_bg, &[]);
                    pass.set_vertex_buffer(1, gm.instances.slice(..));
                    pass.set_bind_group(1, &gm.mat_bg[me.material], &[]);
                    pass.set_vertex_buffer(0, me.vslice());
                    pass.set_index_buffer(me.islice(), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..me.n_idx, 0, 0..gm.n_normal());
                    draws += 1;
                }
            }
            // blended, back to front by mesh centre
            if look.mode == 0 {
                let eye = cam.eye();
                let mut bl: Vec<(f32, &GpuModel, &GpuMesh)> = Vec::new();
                for gm in models {
                    for me in gm.meshes.iter().filter(|m| m.visible) {
                        if self.decal_bias && gm.mat_decal[me.material].is_some() {
                            continue;
                        }
                        if gm.mat_alpha[me.material] == 2
                            || matches!(gm.mat_class[me.material], 5 | 6 | 8)
                        {
                            bl.push(((me.centre - eye).length(), gm, me));
                        }
                    }
                }
                bl.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                pass.set_pipeline(&self.pipe_blend);
                for (_, gm, me) in bl {
                    pass.set_bind_group(0, &gm.frame_bg, &[]);
                    pass.set_vertex_buffer(1, gm.instances.slice(..));
                    pass.set_bind_group(1, &gm.mat_bg[me.material], &[]);
                    pass.set_vertex_buffer(0, me.vslice());
                    pass.set_index_buffer(me.islice(), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..me.n_idx, 0, 0..gm.n_normal());
                    draws += 1;
                }
            }
            if look.wire {
                pass.set_pipeline(&self.pipe_wire);
                for gm in models {
                    pass.set_bind_group(0, &gm.frame_bg, &[]);
                    pass.set_vertex_buffer(1, gm.instances.slice(..));
                    for me in gm.meshes.iter().filter(|m| m.visible) {
                        if let Some(eb) = &me.eb {
                            pass.set_bind_group(1, &gm.mat_bg[me.material], &[]);
                            pass.set_vertex_buffer(0, me.vslice());
                            pass.set_index_buffer(eb.slice(..), wgpu::IndexFormat::Uint32);
                            pass.draw_indexed(0..me.n_edge, 0, 0..gm.n_normal());
                        }
                    }
                }
            }
            if let Some(lb) = &lb {
                pass.set_pipeline(&self.pipe_lines);
                pass.set_bind_group(0, &self.default_frame_bg, &[]);
                pass.set_vertex_buffer(0, lb.slice(..));
                pass.draw(0..lines.len() as u32, 0..1);
            }
            if let Some(lt) = &lt {
                pass.set_pipeline(&self.pipe_lines_top);
                pass.set_bind_group(0, &self.default_frame_bg, &[]);
                pass.set_vertex_buffer(0, lt.slice(..));
                pass.draw(0..lines_on_top.len() as u32, 0..1);
            }
            RTIME[0].fetch_add(
                t_r.elapsed().as_micros() as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
        }
        let t_e = std::time::Instant::now();
        let cb = enc.finish();
        RTIME[1].fetch_add(
            t_e.elapsed().as_micros() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        let t_s = std::time::Instant::now();
        queue.submit([cb]);
        RTIME[2].fetch_add(
            t_s.elapsed().as_micros() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        RTIME[3].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.draws = draws;
        let _ = &self.dummy_joints;
        t.egui_id
    }

    /// Read the last rendered frame back as RGBA8 (sRGB-encoded).
    pub fn read_back(&self, rs: &egui_wgpu::RenderState) -> Option<(u32, u32, Vec<u8>)> {
        let t = self.target.as_ref()?;
        let [w, h] = t.size;
        let row = (4 * w + 255) / 256 * 256;
        let buf = rs.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = rs.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &t.resolve,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        rs.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        if let Err(e) = rs.device.poll(wgpu::PollType::wait_indefinitely()) {
            eprintln!("GPU readback poll failed: {e}");
            return None;
        }
        match rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                eprintln!("GPU readback mapping failed: {e}");
                return None;
            }
            Err(e) => {
                eprintln!("GPU readback callback failed: {e}");
                return None;
            }
        }
        let data = match slice.get_mapped_range() {
            Ok(data) => data,
            Err(e) => {
                eprintln!("GPU readback buffer access failed: {e}");
                return None;
            }
        };
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            out.extend_from_slice(&data[(y * row) as usize..(y * row + 4 * w) as usize]);
        }
        Some((w, h, out))
    }

    /// Screen position (in pixels within the view rect) of a world point.
    pub fn project(cam: &Camera, size: egui::Vec2, p: Vec3) -> Option<egui::Pos2> {
        let vp = cam.proj(size.x / size.y) * cam.view();
        let c = vp * Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w <= 0.0 {
            return None;
        }
        let n = c.truncate() / c.w;
        Some(egui::pos2(
            (n.x * 0.5 + 0.5) * size.x,
            (0.5 - n.y * 0.5) * size.y,
        ))
    }
}

pub fn save_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) -> anyhow::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    image::save_buffer(path, rgba, w, h, image::ExtendedColorType::Rgba8)?;
    Ok(())
}
