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
/// Side of the sun shadow map in texels.
const SHADOW_SIZE: u32 = 4096;
/// Side of the camera depth pre-pass texture (the view is drawn into its top-left corner).
const CAM_DEPTH_SIZE: u32 = 4096;
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
    /// x: strand dither index, y, z: the target's width and height in pixels
    /// (1e9 in the sun shadow pass, with x the strand widening factor)
    pub frame: [f32; 4],
    /// The sun shadow map's view-projection (all zero: no shadow map).
    pub light_view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MatU {
    base: [f32; 4],
    flags: [u32; 4],
    /// Fabric detail tile: UV scale, rotation, the tile's mean grey value,
    /// 1 = bound.
    tile: [f32; 4],
    /// x: how bright an emissive material lights, times the frame's
    /// emissive level (`glb::emissive_scale`); y, z, w: the light's colour
    /// (`glb::emissive_tint`).
    emis: [f32; 4],
    /// x: strand width factor of a strand piece this frame (the stored strands over the drawn ones, as the
    /// game's count rule draws fewer strands at its target thickness); 1 otherwise.
    strand: [f32; 4],
    /// Ped glass (`glb::Material::glass`, the material library's transmission
    /// record): [σA rgb, wA], [σB rgb, wB], [d_base, d_max, p, h0],
    /// [k, t, d_mask, 1 = glass]; all zero otherwise.
    glass: [[f32; 4]; 4],
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

/// Vertex of the ground shadow (shader `GIn`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GroundV {
    pos: [f32; 3],
    /// place in the blob (m from its centre), its half size
    rect: [f32; 4],
    /// softness (m), darkness
    k: [f32; 2],
}

/// A soft shadow on the ground under a model. Not a shadow map: two blurred
/// rounded boxes from the model's bounds, a dark one right under it (what
/// the model hides of the sky) and a wider, lighter one pushed away from
/// the sun.
#[derive(Clone, Copy)]
pub struct GroundShadow {
    /// Centre of the model's footprint, on the ground.
    pub centre: Vec3,
    /// Half size of the footprint along x and z.
    pub half: [f32; 2],
    /// Height of the model over the ground.
    pub height: f32,
}

impl GroundShadow {
    fn verts(&self, sun: Vec3, sun_intensity: f32) -> Vec<GroundV> {
        let m = self.half[0].min(self.half[1]);
        // away from the sun, by a part of the model's height (most of a
        // model's bulk is low; a low sun must not throw the blob far off)
        let off = -Vec3::new(sun.x, 0.0, sun.z) / sun.y.max(0.4) * self.height * 0.22;
        let mut out = Vec::with_capacity(12);
        let mut quad = |c: Vec3, half: [f32; 2], soft: f32, dark: f32| {
            let (ex, ez) = (half[0] + soft, half[1] + soft);
            let v = |sx: f32, sz: f32| GroundV {
                pos: [c.x + sx * ex, c.y, c.z + sz * ez],
                rect: [sx * ex, sz * ez, half[0], half[1]],
                k: [soft, dark],
            };
            out.extend([v(-1.0, -1.0), v(1.0, -1.0), v(1.0, 1.0), v(-1.0, -1.0), v(1.0, 1.0), v(-1.0, 1.0)]);
        };
        quad(
            self.centre + off,
            [self.half[0] + off.x.abs() * 0.5, self.half[1] + off.z.abs() * 0.5],
            0.5 * m + 0.15 * self.height,
            0.45 * sun_intensity.clamp(0.0, 1.0),
        );
        quad(self.centre, self.half, 0.35 * m, 0.65);
        out
    }
}

/// Lighting and view settings for one frame.
#[derive(Clone)]
pub struct Look {
    pub mode: u32,
    /// Ped skin's engine detail (pores, wrinkles) on. Off for checking
    /// whether a pattern on the skin comes from it; sent as bit 8 of mode.y.
    pub skin_detail: bool,
    pub rma_order: u32,
    pub finish: u32,
    /// The vehicle's own paint finish (metalness, roughness), for finish 4.
    pub stored_finish: [f32; 2],
    /// The coat of the vehicle's body paint, for finish 4: clear coat
    /// intensity (0 = none) and roughness, flake orientation (0 = no
    /// flakes) and flake size.
    pub coat: [f32; 4],
    /// The coat's base coat roughness, when the coat overrides it.
    pub coat_base: Option<f32>,
    /// How much of the second layer of vehicle part surfaces shows (0 to 1).
    pub wear: f32,
    /// Show each part's wear as the vehicle's configuration sets it (its
    /// wear level), instead of `wear`, when the model has wear levels.
    pub wear_from_config: bool,
    /// The shown vehicle has wear levels (set by the model view).
    pub wear_levels: bool,
    /// With `wear_from_config`: the amount of the second wear layer (the
    /// primer), 0 (off, the default) to 1. The game takes it per vehicle at
    /// run time (spawn data, or a record of a file that is not shipped), so
    /// it is the user's to set; it scales the primer's summed mask level
    /// before its window.
    pub primer_amount: f32,
    /// The primer set a part takes first when its `primerSets` mask allows
    /// several (0 to 2): the game takes it from a per-vehicle random order.
    pub primer_set: u8,
    pub slot: u32,
    pub paint: [f32; 3],
    pub paint2nd: [f32; 3],
    pub pearl: [f32; 3],
    pub pearl_amount: f32,
    pub sun_dir: Vec3,
    /// Degrees the light is turned about the vertical from where the time of day puts it.
    pub sun_turn: f32,
    pub sun_intensity: f32,
    pub sun_col: [f32; 3],
    pub sky_top: [f32; 3],
    pub sky_hor: [f32; 3],
    pub exposure: f32,
    pub ambient: f32,
    /// Share of the sun light the lit ground and surroundings send back to
    /// the model (the timecycle's `ambient_bounce_intensity`; sun_col.w).
    pub bounce: f32,
    pub wet: f32,
    pub emissive: f32,
    pub fog: f32,
    pub wire: bool,
    /// Multiply the model's tint palette colour (GLB COLOR_0) into the base colour.
    pub tint: bool,
    pub clear: [f32; 3],
}

/// The timecycle's `ambient_bounce_intensity` where no weather is picked
/// (its default in the map's scene too).
pub const BOUNCE: f32 = 0.5;

impl Default for Look {
    fn default() -> Self {
        Look {
            mode: 0,
            // "ignore packed": the packed-data map's channel order is unverified and
            // typical maps (R=G=0, B~0.5) read as R-roughness made every surface a
            // mirror of the sky (washed-out, bluish look). Selectable in the UI.
            rma_order: 3,
            finish: 0,
            stored_finish: [0.0, 0.12],
            coat: [0.0; 4],
            coat_base: None,
            wear: 0.0,
            wear_from_config: true,
            wear_levels: false,
            primer_amount: 0.0,
            primer_set: 0,
            slot: 0,
            paint: [0.55, 0.02, 0.02],
            paint2nd: [0.6, 0.6, 0.62],
            pearl: [0.9, 0.8, 0.3],
            pearl_amount: 0.0,
            sun_dir: Vec3::new(0.45, 0.75, 0.35).normalize(),
            sun_turn: std::env::var("SIXNG_SUN_TURN").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0),
            sun_intensity: 1.0,
            sun_col: [1.0, 0.95, 0.88],
            sky_top: [0.25, 0.45, 0.85],
            sky_hor: [0.75, 0.82, 0.9],
            exposure: 1.0,
            ambient: 0.9,
            bounce: BOUNCE,
            wet: 0.0,
            emissive: 2.0,
            fog: 0.0,
            wire: false,
            skin_detail: std::env::var("SIXNG_SKIN_DETAIL").map_or(true, |v| v != "0" && !v.eq_ignore_ascii_case("off")),
            tint: true,
            clear: [0.6, 0.65, 0.7],
        }
    }
}

impl Look {
    /// The game's lighting for a weather at an hour (from the timecycle,
    /// see `sixng_core::daylight::light`): sun or moon direction, its colour
    /// and strength, sky tint and ambient, and exposure. Sunny at 12:00
    /// gives the default look's sun strength and ambient.
    pub fn set_game_light(&mut self, l: &sixng_core::daylight::Light) {
        let def = Look::default();
        self.sun_dir = Vec3::from_array(l.dir).try_normalize().unwrap_or(def.sun_dir);
        self.sun_col = l.dir_col;
        self.sun_intensity = l.dir_strength * def.sun_intensity;
        let mul = |a: [f32; 3], b: [f32; 3]| [a[0] * b[0], a[1] * b[1], a[2] * b[2]];
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        let grey = |c: [f32; 3]| {
            let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            [y; 3]
        };
        // daytime sky, tinted by the timecycle and greyed by cloud cover
        let cloud = l.cloud_cover * 0.8;
        let top = lerp(mul(def.sky_top, l.sky_col), grey(def.sky_top), cloud);
        let hor = lerp(mul(def.sky_hor, l.sky_col), grey(def.sky_hor), cloud);
        let hor = lerp(hor, [0.95, 0.55, 0.3], l.dusk * 0.7 * (1.0 - cloud));
        // the sky's brightness carries part of the ambient (the shaders
        // multiply the sky colours by `ambient`), the rest goes to `ambient`
        let night = [0.01, 0.015, 0.04];
        let b = l.day.sqrt();
        self.sky_top = lerp(night, top, b);
        self.sky_hor = lerp(lerp(night, [0.03, 0.04, 0.08], 0.5), hor, b);
        let sky_l = (0.2126 * self.sky_hor[0] + 0.7152 * self.sky_hor[1] + 0.0722 * self.sky_hor[2])
            / (0.2126 * def.sky_hor[0] + 0.7152 * def.sky_hor[1] + 0.0722 * def.sky_hor[2]);
        self.ambient = (l.ambient * def.ambient / sky_l.max(0.05)).min(2.0);
        self.bounce = l.bounce;
        self.emissive = 1.5 + 6.0 * (1.0 - l.day);
        // a viewer's stand-in for eye adaptation: brighten dark scenes by
        // part of their darkness, plus the timecycle's exposure offset
        let key = (l.dir_strength * l.dir[1].max(0.0) + 0.5 * l.ambient) / (0.9 + 0.5);
        self.exposure = ((1.0 / key.max(1e-3)).powf(0.5).clamp(1.0, 4.0) * 2f32.powf(l.exposure_ev)).clamp(0.2, 4.0);
        // keep the viewer's sun turn (the sun direction slider) across weather and hour changes
        let t = std::mem::take(&mut self.sun_turn);
        self.turn_sun(t);
    }

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
        self.bounce = BOUNCE;
        self.emissive = 1.5 + 6.0 * (1.0 - day);
        self.exposure = 1.0 + 1.5 * (1.0 - day.sqrt());
        if el < 0.0 {
            // moonlight from the opposite side
            self.sun_dir = Vec3::new(-a.cos() * 0.6, (-el).max(0.35), -0.3).normalize();
            self.sun_intensity = 0.35;
            self.sun_col = [0.45, 0.55, 0.85];
            self.ambient = 0.6;
        }
        // the light turned about the vertical (the model views' sun direction slider)
        let t = std::mem::take(&mut self.sun_turn);
        self.turn_sun(t);
    }

    /// Turns the light about the vertical to `deg` degrees from where the time of day puts it.
    pub fn turn_sun(&mut self, deg: f32) {
        let (s, c) = (deg - self.sun_turn).to_radians().sin_cos();
        let d = self.sun_dir;
        self.sun_dir = Vec3::new(d.x * c + d.z * s, d.y, d.z * c - d.x * s);
        self.sun_turn = deg;
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
    /// The depth as the blended draws read it (between the opaque and the
    /// blended pass).
    scene_bg: wgpu::BindGroup,
    resolve: wgpu::Texture,
    resolve_view: wgpu::TextureView,
    /// HDR frame (the 3D map): the tone-mapped image that is shown and read back.
    display: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// The running mean of still frames (when the renderer accumulates).
    accum: Option<crate::accum::Targets>,
    egui_id: egui::TextureId,
}

pub struct GpuMesh {
    /// Vertex and index data: byte ranges of buffers shared by the model's
    /// meshes. The indices address the whole vertex buffer (draw with base
    /// vertex 0 and the whole buffer bound).
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
    /// Bounding radius around `centre` (model space).
    pub radius: f32,
    pub visible: bool,
    /// Strand hair that follows the game's strand count rule, and the
    /// indices of it to draw this frame.
    pub strand_lod: Option<glb::StrandLod>,
    pub n_draw: std::sync::atomic::AtomicU32,
    /// The same for the sun shadow map.
    pub n_shadow: std::sync::atomic::AtomicU32,
    /// Which of the shadow pass's frame groups the mesh is drawn with (0: the plain one).
    pub shadow_slot: std::sync::atomic::AtomicU32,
}

pub struct GpuModel {
    pub meshes: Vec<GpuMesh>,
    /// Mesh indices in the order their indices sit in the shared buffer
    /// (`draw_order`): drawing in this order puts meshes of one material
    /// next to each other, so they join into one draw.
    pub order: Vec<usize>,
    pub mat_bg: Vec<wgpu::BindGroup>,
    pub mat_alpha: Vec<u32>,
    pub mat_class: Vec<u32>,
    /// Decal layer order per material (None = not a decal).
    pub mat_decal: Vec<Option<u8>>,
    /// Ped glass per material: drawn with the tinted-glass blend when the
    /// device has dual-source blending.
    pub mat_glass: Vec<bool>,
    /// All material uniforms in one buffer, MAT_STRIDE bytes apart.
    mat_ub: wgpu::Buffer,
    mat_cpu: Vec<MatU>,
    _textures: Vec<Tex>,
    /// The map's material table (gpu_draw.rs): the model's place in the
    /// shared pools and table; None: it draws the old way only.
    pub gd: Option<crate::gpu_draw::Handle>,
    /// Base vertex of every draw of its meshes: a model in the shared pool
    /// addresses its own vertices from its first one there (0: own buffers).
    pub base_vertex: i32,
    pub joints_buf: wgpu::Buffer,
    pub frame_bg: wgpu::BindGroup,
    /// The model's own sticker records, once it has stickers (`set_stickers`).
    pub sticker_buf: Option<wgpu::Buffer>,
    pub instances: wgpu::Buffer,
    /// The instances' transforms and largest axis scale, in buffer order,
    /// when the model has at most `CULL_MAX_INST` of them (empty otherwise):
    /// its meshes are then culled one by one (`MeshCull`).
    pub inst_cpu: Vec<(Mat4, f32)>,
    pub n_instances: u32,
    /// The last `n_fb` instances are coverage fallbacks (see `Instance::fallback`).
    pub n_fb: u32,
    pub instance_cap: u32,
    pub bbox: [Vec3; 2],
    /// World bounding sphere of the normally drawn instances (centre,
    /// radius), set with the instances; None: unknown (always drawn).
    pub world: Option<(Vec3, f32)>,
    /// Stencil value its opaque surfaces write: 1 = real surface, 0 = far
    /// field (terrain, ocean), which coverage fallbacks may draw over.
    pub stencil: u32,
    /// Material names and shaders (inspection).
    pub mat_names: Vec<String>,
    /// Draw calls recorded for this model (main, shadow), summed until the
    /// map's draw log takes them.
    pub drawn: [std::sync::atomic::AtomicU32; 2],
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

    /// The meshes in buffer order (see `order`).
    pub fn ordered(&self) -> impl Iterator<Item = &GpuMesh> {
        self.order.iter().filter_map(|&i| self.meshes.get(i))
    }
}

/// Models with at most this many instances have their meshes culled one by
/// one (big single placements: SLOD tiles, buildings).
pub const CULL_MAX_INST: usize = 8;

/// Meshes skipped by the per-mesh cull in the last frame: main pass,
/// coverage fallbacks, shadow cascades, decal and see-through.
pub static MESH_CULLED: [std::sync::atomic::AtomicU32; 4] = [
    std::sync::atomic::AtomicU32::new(0),
    std::sync::atomic::AtomicU32::new(0),
    std::sync::atomic::AtomicU32::new(0),
    std::sync::atomic::AtomicU32::new(0),
];

/// A per-mesh visibility test: the camera's frustum sides, or a shadow cascade.
#[derive(Clone, Copy)]
pub enum MeshCull<'a> {
    View([Vec4; 4]),
    Cascade(&'a crate::shadow::Cascade),
}

impl MeshCull<'_> {
    /// The four side planes of `view_proj` (normalised, inside positive).
    /// Near and far are left out: a sphere behind the camera fails the
    /// sides, and the far plane is at infinity.
    pub fn view(view_proj: Mat4) -> MeshCull<'static> {
        let r = |i: usize| view_proj.row(i);
        let n = |p: Vec4| p / p.truncate().length().max(1e-12);
        MeshCull::View([n(r(3) + r(0)), n(r(3) - r(0)), n(r(3) + r(1)), n(r(3) - r(1))])
    }

    /// Whether a world sphere may be seen.
    pub fn sees(&self, c: Vec3, r: f32) -> bool {
        match self {
            MeshCull::View(p) => p.iter().all(|p| p.truncate().dot(c) + p.w >= -r),
            MeshCull::Cascade(k) => crate::shadow::in_cascade(k, c, r),
        }
    }

    /// Whether any of the given instances shows `me`.
    fn mesh(&self, inst: &[(Mat4, f32)], me: &GpuMesh) -> bool {
        inst.iter().any(|(m, s)| self.sees(m.transform_point3(me.centre), me.radius * s))
    }
}

/// The instances a model's meshes are culled against, when it has its
/// transforms on the CPU and no skeleton (None: draw every mesh).
fn cull_inst(gm: &GpuModel, range: &std::ops::Range<u32>) -> Option<std::ops::Range<usize>> {
    (gm.inst_cpu.len() == gm.n_instances as usize && !gm.meshes.iter().any(|m| m.skinned))
        .then(|| range.start as usize..range.end as usize)
}

/// Which pass draws a mesh of this material, as the buffer layout groups
/// them: 0 solid, 1 cut-out, 2 decal, 3 see-through.
fn mesh_kind(alpha: u32, class: u32, decal: bool) -> u8 {
    if decal {
        2
    } else if alpha == 2 || matches!(class, 5 | 6 | 8) {
        3
    } else if alpha == 1 {
        1
    } else {
        0
    }
}

/// The order a model's meshes go into its shared buffers: by kind
/// (`mesh_kind`), then material, then their own order. A model's solid
/// meshes then follow on for the shadow passes, and the meshes of one
/// material for the main pass.
pub fn draw_order(mats: &[usize], kind: impl Fn(usize) -> u8) -> Vec<usize> {
    let mut o: Vec<usize> = (0..mats.len()).collect();
    o.sort_by_key(|&i| (kind(mats[i]), mats[i], i));
    o
}

/// World bounding sphere of a model's instances from its local box.
fn world_sphere<'a>(bbox: [Vec3; 2], inst: impl Iterator<Item = &'a Instance>) -> Option<(Vec3, f32)> {
    let c = (bbox[0] + bbox[1]) * 0.5;
    let r = (bbox[1] - bbox[0]).length() * 0.5;
    if !c.is_finite() || !r.is_finite() {
        return None;
    }
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for i in inst {
        let m = Mat4::from_cols_array_2d(&i.m);
        let wc = m.transform_point3(c);
        let s = m.x_axis.truncate().length().max(m.y_axis.truncate().length()).max(m.z_axis.truncate().length());
        let wr = r * s;
        lo = lo.min(wc - Vec3::splat(wr));
        hi = hi.max(wc + Vec3::splat(wr));
    }
    if lo.x > hi.x {
        return None;
    }
    Some(((lo + hi) * 0.5, (hi - lo).length() * 0.5))
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
    /// First index of this mesh in the shared index buffer.
    pub fn first(&self) -> u32 {
        (self.ib_range.start / 4) as u32
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
        "hair" => 12,
        _ => 1,
    }
}

pub struct Renderer3D {
    pipe_opaque: wgpu::RenderPipeline,
    pipe_blend: wgpu::RenderPipeline,
    /// Ped glass's dual-source blend (None: the device has no dual-source
    /// blending, or a map renderer); glass is then drawn with `pipe_blend`.
    pipe_glass: Option<wgpu::RenderPipeline>,
    /// The blended draws' view of the scene depth (group 2).
    bgl_scene: wgpu::BindGroupLayout,
    /// Decal-bucket materials: depth-biased toward the camera so coplanar
    /// overlays win against the surface under them.
    pipe_decal: wgpu::RenderPipeline,
    pipe_decal_blend: wgpu::RenderPipeline,
    pub decal_bias: bool,
    pipe_wire: wgpu::RenderPipeline,
    pipe_lines: wgpu::RenderPipeline,
    pipe_lines_top: wgpu::RenderPipeline,
    /// Coronas of the map's placed lights (lights.rs), added in HDR.
    pipe_glow: wgpu::RenderPipeline,
    /// The coronas to draw this frame (Map 3D; empty elsewhere).
    pub glows: Vec<crate::lights::Glow>,
    pipe_sky: wgpu::RenderPipeline,
    pipe_ground: wgpu::RenderPipeline,
    /// Shadow to draw on the ground under the model (shaded mode only).
    pub ground: Option<GroundShadow>,
    bgl_frame: wgpu::BindGroupLayout,
    bgl_mat: wgpu::BindGroupLayout,
    frame_ub: wgpu::Buffer,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    dummy_joints: wgpu::Buffer,
    stickers: StickerDefaults,
    /// The game's four hair lighting tables (1-texel stand-ins until loaded).
    hair_luts: Vec<wgpu::TextureView>,
    /// Whether the game's tables were tried (once per renderer).
    hair_tried: bool,
    pub default_frame_bg: wgpu::BindGroup,
    target: Option<Target>,
    /// Draw calls in the last frame.
    pub draws: u32,
    /// The sun shadow map and its sampler (a 1-texel map when shadows are off).
    sun_map: (wgpu::TextureView, wgpu::Sampler, wgpu::TextureView),
    pub sun_maps: bool,
    shadow_ub: wgpu::Buffer,
    shadow_frame_bg: wgpu::BindGroup,
    /// More frame groups for the shadow pass: one per strand piece, each with the piece's widening factor.
    shadow_extra: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    /// Strands enter the shadow map by the rule of the game's ray-traced sun shadows (fewer, wider ribbons);
    /// off: by the rule of its cascade shadow pass (SIXNG_SHADOWS=map).
    pub shadow_traced: bool,
    pipe_sun_map: wgpu::RenderPipeline,
    depth_ub: wgpu::Buffer,
    depth_frame_bg: wgpu::BindGroup,
    pipe_cam_depth: wgpu::RenderPipeline,
    /// The game's screen-space hair blur (hairblur.rs) and the pipeline of its strand mask: strands drawn again
    /// over the main pass's depth.
    hair_blur: Option<crate::hairblur::HairBlur>,
    pipe_hair_mask: Option<wgpu::RenderPipeline>,
    light_vp: [[f32; 4]; 4],
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
    /// Index of the strand dither pattern (0 in the live view; screenshots
    /// that average several frames step it, as the game's dither changes
    /// every frame and its temporal filter averages it).
    pub dither_frame: f32,
    /// Still-camera accumulation (the model viewer): while the view is
    /// unchanged each frame steps the dither pattern and joins a running mean
    /// (`crate::accum`), which is the image shown and read back.
    pub accumulate: bool,
    accum: Option<crate::accum::Accum>,
    /// Frames in the running mean, and the key of the view they show.
    accum_n: u32,
    /// GPU time by part of the frame (automation tick lines)
    gpu_timer: Option<crate::gpu_time::GpuTimer>,
    accum_key: u64,
    /// Changes made outside `render` (joints, instances) since the last frame.
    accum_bump: std::sync::atomic::AtomicU64,
    /// Map view: building glass drawn dark and mostly opaque (misc3.z).
    pub map_glass: bool,
    /// Draw calls of the coverage fallbacks in the last frame: models, cells.
    pub fb_draws: [u32; 2],
    /// The scene's atmosphere, exposure and post chain (atmo.rs); the model
    /// viewer only binds its resources.
    atmo: crate::atmo::Atmosphere,
    /// HDR frame with the game's sky and light (the 3D map).
    hdr: bool,
    /// The sky of this frame (HDR frame only).
    pub sky: Option<crate::atmo::Sky>,
    last_frame: Option<std::time::Instant>,
    /// Sun and moon shadow cascades (HDR frame only; shadow.rs).
    shadow: crate::shadow::ShadowGpu,
    /// One frame bind group per cascade: its view, and a stand-in in place
    /// of the cascades it draws into.
    shadow_bgs: Vec<wgpu::BindGroup>,
    pipe_shadow: wgpu::RenderPipeline,
    pipe_shadow_cell: wgpu::RenderPipeline,
    /// Draw sun and moon shadows (3D map).
    pub shadows: bool,
    /// Draw calls into the shadow cascades in the last frame.
    pub shadow_draws: u32,
    /// Decal and see-through model draw calls in the last frame (part of `draws`).
    pub late_draws: u32,
    /// The map's material table (gpu_draw.rs): the shared pools, table and
    /// texture arrays, the opaque, fallback, cut-out shadow and solid
    /// shadow pipelines that read them, and the frame's indirect buffer.
    gd: Option<std::sync::Arc<crate::gpu_draw::Shared>>,
    gd_pipes: Option<[wgpu::RenderPipeline; 4]>,
    gd_args: Option<wgpu::Buffer>,
    /// Last frame: sub-draws, indirect calls, models drawn the old way, models on the table.
    pub gd_stats: [u32; 4],
}

/// Indirect calls of one pass: the sub-draws of one set of pool chunks
/// (and stencil value) one after another in the frame's indirect buffer.
struct GdList {
    stencil: u32,
    g: usize,
    n: usize,
    args: std::ops::Range<u32>,
}

/// What the material table draws this frame.
struct GdFrame<'a> {
    snap: crate::gpu_draw::Snapshot,
    main: Vec<GdList>,
    fb: Vec<GdList>,
    /// Per cascade: cut-out casters, solid ones.
    shadow: [Vec<GdList>; crate::shadow::CASCADES],
    shadow_solid: [Vec<GdList>; crate::shadow::CASCADES],
    /// Models it does not draw (the old path draws them).
    rest: Vec<&'a GpuModel>,
    /// Models it draws, whose decals the old path still draws.
    on: Vec<&'a GpuModel>,
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

/// What a model without stickers binds in their place.
#[derive(Clone)]
struct StickerDefaults {
    none: wgpu::Buffer,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

/// Layout entry of one hair lighting table.
fn hair_entry(binding: u32, view_dimension: wgpu::TextureViewDimension) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension, multisampled: false },
        count: None,
    }
}

/// A float table as a texture (RGBA half floats): 3D when `d3`, else 2D.
fn hair_texture(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32, d: u32, d3: bool, rgba16: &[u8]) -> wgpu::TextureView {
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: if d3 { d } else { 1 } };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hair table"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: if d3 { wgpu::TextureDimension::D3 } else { wgpu::TextureDimension::D2 },
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        rgba16,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 8), rows_per_image: Some(h) },
        size,
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// 1-texel stand-ins for the hair tables, until the game's are loaded.
fn hair_blank(device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<wgpu::TextureView> {
    [true, true, false, true].iter().map(|&d3| hair_texture(device, queue, 1, 1, 1, d3, &[0u8; 8])).collect()
}

#[allow(clippy::too_many_arguments)]
fn frame_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    frame_ub: &wgpu::Buffer,
    joints: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    stickers: &wgpu::Buffer,
    sticker_view: &wgpu::TextureView,
    sticker_sampler: &wgpu::Sampler,
    scene: &crate::atmo::SceneBinds,
    hair: &[wgpu::TextureView],
    shadow: &(wgpu::TextureView, wgpu::Sampler, wgpu::TextureView),
) -> wgpu::BindGroup {
    let mut entries = vec![
        wgpu::BindGroupEntry { binding: 0, resource: frame_ub.as_entire_binding() },
        wgpu::BindGroupEntry { binding: 1, resource: joints.as_entire_binding() },
        wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
        wgpu::BindGroupEntry { binding: 3, resource: stickers.as_entire_binding() },
        wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(sticker_view) },
        wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(sticker_sampler) },
    ];
    entries.extend(scene.entries());
    for (k, v) in hair.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry { binding: 15 + k as u32, resource: wgpu::BindingResource::TextureView(v) });
    }
    entries.push(wgpu::BindGroupEntry { binding: 19, resource: wgpu::BindingResource::TextureView(&shadow.0) });
    entries.push(wgpu::BindGroupEntry { binding: 20, resource: wgpu::BindingResource::Sampler(&shadow.1) });
    entries.push(wgpu::BindGroupEntry { binding: 21, resource: wgpu::BindingResource::TextureView(&shadow.2) });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("frame bg"),
        layout,
        entries: &entries,
    })
}

/// Sticker pictures as the layers of one square texture array: per layer
/// its mip chain (RGBA8, `size` down to 1). Stored as plain values, not
/// sRGB: a colour mask's channels are amounts, and the shader converts the
/// pictures that are colours.
pub fn sticker_array(device: &wgpu::Device, queue: &wgpu::Queue, size: u32, layers: &[Vec<Vec<u8>>]) -> wgpu::TextureView {
    let mips = layers.iter().map(|l| l.len()).min().unwrap_or(1).max(1) as u32;
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("stickers"),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: layers.len().max(1) as u32 },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (li, l) in layers.iter().enumerate() {
        for (mi, px) in l.iter().take(mips as usize).enumerate() {
            let w = (size >> mi).max(1);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: mi as u32, origin: wgpu::Origin3d { x: 0, y: 0, z: li as u32 }, aspect: wgpu::TextureAspect::All },
                px,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(w) },
                wgpu::Extent3d { width: w, height: w, depth_or_array_layers: 1 },
            );
        }
    }
    tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() })
}

/// A model texture: a texture of its own, or a layer of one of the map's
/// shared texture arrays (gpu_draw.rs) with a 2D view of that layer.
#[derive(Clone)]
pub struct Tex {
    pub tex: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub layer: Option<std::sync::Arc<crate::gpu_draw::Layer>>,
}

impl Tex {
    pub fn own(tex: wgpu::Texture) -> Tex {
        let view = tex.create_view(&Default::default());
        Tex { tex, view, layer: None }
    }
}

/// The texture class an image uploads as: format, size, mip count.
fn tex_class(img: &glb::Image) -> crate::gpu_draw::Class {
    let (w, h, _) = &img.mips[0];
    let (format, mips) = match &img.bc {
        Some(bc) => (
            match (bc.alpha, img.srgb) {
                (false, false) => wgpu::TextureFormat::Bc1RgbaUnorm,
                (false, true) => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
                (true, false) => wgpu::TextureFormat::Bc3RgbaUnorm,
                (true, true) => wgpu::TextureFormat::Bc3RgbaUnormSrgb,
            },
            bc.levels.len() as u32,
        ),
        None => (if img.srgb { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm }, img.mips.len() as u32),
    };
    crate::gpu_draw::Class { format, w: *w, h: *h, mips }
}

/// The copies that upload an image's mip chain: level, bytes, layout,
/// extent (BC levels at their 4x4-block size).
fn tex_copies(img: &glb::Image) -> Vec<(u32, &[u8], wgpu::TexelCopyBufferLayout, wgpu::Extent3d)> {
    if let Some(bc) = &img.bc {
        let (w, h, _) = &img.mips[0];
        let bb = crate::bc::block_bytes(bc.alpha) as u32;
        return bc
            .levels
            .iter()
            .enumerate()
            .map(|(l, data)| {
                let (mw, mh) = ((w >> l).max(1), (h >> l).max(1));
                let (bw, bh) = (mw.div_ceil(4), mh.div_ceil(4));
                (
                    l as u32,
                    &data[..],
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bw * bb), rows_per_image: Some(bh) },
                    wgpu::Extent3d { width: bw * 4, height: bh * 4, depth_or_array_layers: 1 },
                )
            })
            .collect();
    }
    img.mips
        .iter()
        .enumerate()
        .map(|(l, (mw, mh, px))| {
            (
                l as u32,
                &px[..],
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * mw), rows_per_image: Some(*mh) },
                wgpu::Extent3d { width: *mw, height: *mh, depth_or_array_layers: 1 },
            )
        })
        .collect()
}

fn tex_origin(t: &wgpu::Texture, l: u32, layer: u32) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo { texture: t, mip_level: l, origin: wgpu::Origin3d { x: 0, y: 0, z: layer }, aspect: wgpu::TextureAspect::All }
}

/// Write an image's mip chain into `layer` of `t`.
fn tex_write(queue: &wgpu::Queue, t: &wgpu::Texture, layer: u32, img: &glb::Image) {
    for (l, data, lay, ext) in tex_copies(img) {
        queue.write_texture(tex_origin(t, l, layer), data, lay, ext);
    }
}

/// Upload an image as a texture of its own (RGBA8, or BC1 / BC3 when it has blocks).
pub fn tex_rgba(device: &wgpu::Device, queue: &wgpu::Queue, img: &glb::Image) -> wgpu::Texture {
    let c = tex_class(img);
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(if img.bc.is_some() { "model tex bc" } else { "model tex" }),
        size: wgpu::Extent3d { width: c.w, height: c.h, depth_or_array_layers: 1 },
        mip_level_count: c.mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: c.format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    tex_write(queue, &t, 0, img);
    t
}

/// Upload an image into a layer of the map's shared texture arrays, or as a
/// texture of its own when there is no table or it does not fit one.
pub fn tex_upload(device: &wgpu::Device, queue: &wgpu::Queue, img: &glb::Image, gd: Option<&std::sync::Arc<crate::gpu_draw::Shared>>) -> Tex {
    crate::gpu_draw::watch::at(1, 10);
    if let Some((layer, t, l)) = gd.and_then(|g| g.layer(tex_class(img))) {
        crate::gpu_draw::watch::at(1, 11);
        // written by the render thread before its next frame (see `Shared::defer`)
        let copies: Vec<(u32, Vec<u8>, wgpu::TexelCopyBufferLayout, wgpu::Extent3d)> = tex_copies(img).into_iter().map(|(m, d, lay, ext)| (m, d.to_vec(), lay, ext)).collect();
        let tw = t.clone();
        gd.unwrap().defer(Box::new(move |q: &wgpu::Queue| {
            for (m, d, lay, ext) in &copies {
                q.write_texture(tex_origin(&tw, *m, l), d, *lay, *ext);
            }
        }));
        let view = t.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: l,
            array_layer_count: Some(1),
            ..Default::default()
        });
        return Tex { tex: t, view, layer: Some(layer) };
    }
    crate::gpu_draw::watch::at(1, 12);
    Tex::own(tex_rgba(device, queue, img))
}

impl Renderer3D {
    pub fn new(rs: &egui_wgpu::RenderState) -> Self {
        Self::new_with(rs, false, false)
    }

    /// The model viewer's renderer: as `new`, with the sun shadow map on
    /// (SIXNG_SHADOWS=0 turns it off).
    pub fn new_viewer(rs: &egui_wgpu::RenderState) -> Self {
        Self::new_with(rs, false, true)
    }

    /// The 3D map's renderer: HDR frame with the game's sky, light and
    /// exposure (atmo.rs), tone mapped for display.
    pub fn new_map(rs: &egui_wgpu::RenderState) -> Self {
        Self::new_with(rs, true, false)
    }

    /// `sun_default`: the sun shadow map is on unless SIXNG_SHADOWS says otherwise.
    fn new_with(rs: &egui_wgpu::RenderState, hdr: bool, sun_default: bool) -> Self {
        let device = &rs.device;
        let color_fmt = if hdr { crate::atmo::HDR_FMT } else { COLOR_FMT };
        let shadow = crate::shadow::ShadowGpu::new(device, hdr, std::mem::size_of::<FrameU>() as u64);
        let atmo = crate::atmo::Atmosphere::new(device, &rs.queue, &shadow);
        let shader_src = format!("{}\n{}", include_str!("atmo_common.wgsl"), include_str!("shader.wgsl"));
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sixng shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
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
                // stickers placed on a vehicle (custom liveries): their
                // records, their pictures as layers of one array, and a
                // clamping sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // the four lookup tables of the game's hair lighting (M, N, A, P)
                hair_entry(15, wgpu::TextureViewDimension::D3),
                hair_entry(16, wgpu::TextureViewDimension::D3),
                hair_entry(17, wgpu::TextureViewDimension::D2),
                hair_entry(18, wgpu::TextureViewDimension::D3),
                // the sun shadow map and its comparison sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 19,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 20,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                // the camera depth pre-pass (opaque surfaces and strands as the eye sees them)
                wgpu::BindGroupLayoutEntry {
                    binding: 21,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ]
            .into_iter()
            .chain(crate::atmo::frame_layout_entries())
            .collect::<Vec<_>>(),
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
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
                tex_entry(5),
                tex_entry(6),
                tex_entry(7),
                tex_entry(8),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[Some(&bgl_frame), Some(&bgl_mat)],
            immediate_size: 0,
        });
        // the blended draws also read the scene's depth (the water's depth
        // over what lies under it)
        let bgl_scene = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene depth"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: true,
                },
                count: None,
            }],
        });
        let layout_blend = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh blend"),
            bind_group_layouts: &[Some(&bgl_frame), Some(&bgl_mat), Some(&bgl_scene)],
            immediate_size: 0,
        });
        let layout_frame = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("frame only"),
            bind_group_layouts: &[Some(&bgl_frame)],
            immediate_size: 0,
        });
        let vattrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x2, 4 => Float32x2, 5 => Unorm8x4, 6 => Uint16x4, 7 => Unorm8x4, 13 => Unorm8x4, 14 => Uint8x4, 15 => Float16x4];
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
                        format: color_fmt,
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
            &layout_blend,
            "vs_mesh",
            "fs_mesh_blend",
            &vbufs,
            wgpu::PrimitiveTopology::TriangleList,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            depth(false, wgpu::CompareFunction::Greater),
        );
        // ped glass: the game's dual-source blend, dst' = src0 + dst x src1
        // (glass.wgsl), in a shader module of its own so that devices without
        // dual-source blending still build the rest; only the model viewer
        // sets glass records, so only its renderer builds it
        let pipe_glass = (!hdr && device.features().contains(wgpu::Features::DUAL_SOURCE_BLENDING)).then(|| {
            let src = format!("enable dual_source_blending;\n{}\n{}\n{}", include_str!("atmo_common.wgsl"), include_str!("shader.wgsl"), include_str!("glass.wgsl"));
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sixng glass shader"),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            });
            let blend = wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::Src1,
                operation: wgpu::BlendOperation::Add,
            };
            let blend_a = wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::Src1Alpha,
                operation: wgpu::BlendOperation::Add,
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("glass"),
                layout: Some(&layout_blend),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_mesh"),
                    compilation_options: Default::default(),
                    buffers: &vbufs,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: depth(false, wgpu::CompareFunction::Greater),
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_mesh_glass"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_fmt,
                        blend: Some(wgpu::BlendState { color: blend, alpha: blend_a }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
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
        let gattrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
        let gbufs = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<crate::lights::Glow>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &gattrs,
        })];
        let pipe_glow = mk(
            "glow",
            &layout_frame,
            "vs_glow",
            "fs_glow",
            &gbufs,
            wgpu::PrimitiveTopology::TriangleStrip,
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
                alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            }),
            depth(false, wgpu::CompareFunction::GreaterEqual),
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
        let gattrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x2];
        let pipe_ground = mk(
            "ground shadow",
            &layout_frame,
            "vs_ground",
            "fs_ground",
            &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<GroundV>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &gattrs,
            })],
            wgpu::PrimitiveTopology::TriangleList,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            depth(false, wgpu::CompareFunction::Greater),
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
        let stickers = StickerDefaults {
            // one record: the header, which says "no stickers"
            none: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("no stickers"),
                contents: bytemuck::bytes_of(&crate::livery::Record::default()),
                usage: wgpu::BufferUsages::STORAGE,
            }),
            view: sticker_array(device, &rs.queue, 1, &[vec![vec![255u8; 4]]]),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("stickers"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                anisotropy_clamp: 8,
                ..Default::default()
            }),
        };
        let hair_luts = hair_blank(device, &rs.queue);
        // The sun shadow map (on in the model viewer, elsewhere with SIXNG_SHADOWS=1; SIXNG_SHADOWS=0 turns it off; one map over the models, strands drawn into it as the
        // game draws its strands into its shadow maps; the game's own cascades, bias and filter are not read).
        let shadow_tex = |size: u32| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("sun shadow"),
                    size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Depth32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sun shadow"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let sun_maps = std::env::var("SIXNG_SHADOWS").map_or(sun_default, |v| v != "0");
        let sun_map = (shadow_tex(if sun_maps { SHADOW_SIZE } else { 1 }), shadow_sampler.clone(), shadow_tex(if sun_maps { CAM_DEPTH_SIZE } else { 1 }));
        let shadow_ub = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow frame ub"),
            size: std::mem::size_of::<FrameU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // the shadow pass's own frame group: the light's matrices, and a 1-texel map in place of the one it draws
        let shadow_frame_bg = frame_bind_group(device, &bgl_frame, &shadow_ub, &dummy_joints, &sampler, &stickers.none, &stickers.view, &stickers.sampler, &atmo.binds, &hair_luts, &(shadow_tex(1), shadow_sampler.clone(), shadow_tex(1)));
        let shadow_extra: Vec<(wgpu::Buffer, wgpu::BindGroup)> = (0..if sun_maps { 15 } else { 0 })
            .map(|_| {
                let ub = device.create_buffer(&wgpu::BufferDescriptor { label: Some("shadow frame ub"), size: std::mem::size_of::<FrameU>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
                let bg = frame_bind_group(device, &bgl_frame, &ub, &dummy_joints, &sampler, &stickers.none, &stickers.view, &stickers.sampler, &atmo.binds, &hair_luts, &(shadow_tex(1), shadow_sampler.clone(), shadow_tex(1)));
                (ub, bg)
            })
            .collect();
        let pipe_sun_map = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sun shadow"),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_mesh"), compilation_options: Default::default(), buffers: &vbufs },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        // The camera depth pre-pass (with the sun shadow map): the same depth-only draw from the eye, read by the
        // lighting for the game's strand openness test and a screen-space occlusion of the ambient light.
        let depth_ub = device.create_buffer(&wgpu::BufferDescriptor { label: Some("depth pre-pass frame ub"), size: std::mem::size_of::<FrameU>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let depth_frame_bg = frame_bind_group(device, &bgl_frame, &depth_ub, &dummy_joints, &sampler, &stickers.none, &stickers.view, &stickers.sampler, &atmo.binds, &hair_luts, &(shadow_tex(1), shadow_sampler.clone(), shadow_tex(1)));
        let pipe_cam_depth = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("camera depth pre-pass"),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_mesh"), compilation_options: Default::default(), buffers: &vbufs },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                // the viewer's depth is reversed: nearer is larger
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let hair_blur = Some(crate::hairblur::HairBlur::new(device));
        let pipe_hair_mask = Some({
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("hair blur mask"),
                layout: Some(&layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_mesh"), compilation_options: Default::default(), buffers: &vbufs },
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth_fmt,
                    depth_write_enabled: Some(false),
                    // the main pass's own strand draw: a sample passes where the strand is the surface kept there
                    // (not where it was dithered out or lies behind something)
                    depth_compare: Some(wgpu::CompareFunction::Equal),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: MSAA, ..Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_hair_mask"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: crate::hairblur::MASK_FMT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
        let default_frame_bg = frame_bind_group(device, &bgl_frame, &frame_ub, &dummy_joints, &sampler, &stickers.none, &stickers.view, &stickers.sampler, &atmo.binds, &hair_luts, &sun_map);
        let stand_in = crate::atmo::SceneBinds { shadow: shadow.stand_in.clone(), ..atmo.binds.clone() };
        let shadow_bgs = shadow
            .frame_ubs
            .iter()
            .map(|ub| frame_bind_group(device, &bgl_frame, ub, &dummy_joints, &sampler, &stickers.none, &stickers.view, &stickers.sampler, &stand_in, &hair_luts, &(shadow_tex(1), shadow_sampler.clone(), shadow_tex(1))))
            .collect();
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
        // shadow casters: depth only, both faces, pushed away from the light
        // by the slope (reversed depth: towards 0)
        let shadow_pipe = |label: &str, layout: &wgpu::PipelineLayout, vs: &str, fs: &str, bufs: &[Option<wgpu::VertexBufferLayout>]| {
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
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::shadow::FMT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState { constant: 0, slope_scale: -2.0, clamp: 0.0 },
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipe_shadow = shadow_pipe("shadow", &layout, "vs_mesh", "fs_shadow", &vbufs);
        let pipe_shadow_cell = shadow_pipe("shadow cell", &layout_cell, "vs_cell", "fs_shadow_cell", &cbufs);
        // the map's material table: the same programs reading the shared
        // table and texture arrays (gpu_draw.rs)
        let gd = if hdr { crate::gpu_draw::Shared::new(device, &rs.queue, std::mem::size_of::<MatU>() as u64) } else { None };
        let gd_src = gd.as_ref().and_then(|_| crate::gpu_draw::shader(include_str!("atmo_common.wgsl"), include_str!("shader.wgsl")));
        let gd = gd.filter(|_| gd_src.is_some());
        let gd_pipes = gd.as_ref().zip(gd_src).map(|(g, src)| {
            let m = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sixng shader (material table)"),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            });
            let lay = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mesh (material table)"),
                bind_group_layouts: &[Some(&bgl_frame), Some(&g.bgl)],
                immediate_size: 0,
            });
            let main = |label: &str, ds: Option<wgpu::DepthStencilState>, cull: Option<wgpu::Face>| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&lay),
                    vertex: wgpu::VertexState { module: &m, entry_point: Some("vs_mesh"), compilation_options: Default::default(), buffers: &vbufs },
                    primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: cull, ..Default::default() },
                    depth_stencil: ds,
                    multisample: ms,
                    fragment: Some(wgpu::FragmentState {
                        module: &m,
                        entry_point: Some("fs_mesh"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState { format: color_fmt, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            };
            let opaque = main("opaque (material table)", depth_st(&st_write), None);
            let fb = main("fallback (material table)", depth_st(&st_fb), Some(wgpu::Face::Back));
            let shadow_ds = || {
                Some(wgpu::DepthStencilState {
                    format: crate::shadow::FMT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState { constant: 0, slope_scale: -2.0, clamp: 0.0 },
                })
            };
            let shadow = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("shadow (material table)"),
                layout: Some(&lay),
                vertex: wgpu::VertexState { module: &m, entry_point: Some("vs_mesh"), compilation_options: Default::default(), buffers: &vbufs },
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
                depth_stencil: shadow_ds(),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: &m, entry_point: Some("fs_shadow"), compilation_options: Default::default(), targets: &[] }),
                multiview_mask: None,
                cache: None,
            });
            // solid shadow casters: depth only, with the material table alone bound
            let lay_solid = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("solid shadow (material table)"),
                bind_group_layouts: &[Some(&bgl_frame), Some(&g.bgl_mats)],
                immediate_size: 0,
            });
            let solid = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("solid shadow (material table)"),
                layout: Some(&lay_solid),
                vertex: wgpu::VertexState { module: &m, entry_point: Some("vs_mesh"), compilation_options: Default::default(), buffers: &vbufs },
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
                depth_stencil: shadow_ds(),
                multisample: Default::default(),
                fragment: None,
                multiview_mask: None,
                cache: None,
            });
            [opaque, fb, shadow, solid]
        });
        let gd = gd.filter(|_| gd_pipes.is_some());
        Renderer3D {
            gd,
            gd_pipes,
            gd_args: None,
            gd_stats: [0; 4],
            shadow,
            shadow_bgs,
            pipe_shadow,
            pipe_shadow_cell,
            shadows: true,
            shadow_draws: 0,
            late_draws: 0,
            pipe_fb,
            pipe_cell_fb,
            depth_fmt,
            pipe_opaque,
            pipe_blend,
            pipe_glass,
            bgl_scene,
            pipe_decal,
            pipe_decal_blend,
            decal_bias: true,
            pipe_wire,
            pipe_lines,
            pipe_lines_top,
            pipe_glow,
            glows: Vec::new(),
            pipe_sky,
            pipe_ground,
            ground: None,
            bgl_frame,
            bgl_mat,
            frame_ub,
            sampler,
            white,
            dummy_joints,
            stickers,
            hair_luts,
            sun_map,
            sun_maps,
            shadow_ub,
            shadow_frame_bg,
            shadow_extra,
            shadow_traced: std::env::var("SIXNG_SHADOWS").map_or(true, |v| v != "map"),
            pipe_sun_map,
            depth_ub,
            depth_frame_bg,
            pipe_cam_depth,
            hair_blur,
            pipe_hair_mask,
            light_vp: [[0.0; 4]; 4],
            hair_tried: false,
            default_frame_bg,
            target: None,
            draws: 0,
            pipe_cell,
            bgl_cell,
            cells: Vec::new(),
            lod_k: 1.0,
            dither_frame: 0.0,
            accumulate: false,
            accum: None,
            accum_n: 0,
            gpu_timer: crate::gpu_time::GpuTimer::new(&rs.device, &rs.queue),
            accum_key: 0,
            accum_bump: std::sync::atomic::AtomicU64::new(0),
            map_glass: false,
            fb_draws: [0, 0],
            atmo,
            hdr,
            sky: None,
            last_frame: None,
        }
    }

    /// Upload a CPU model (meshes, materials, textures, skeleton).
    /// Load the game's hair lighting tables, once. Models uploaded after
    /// this are lit with them; without the tables (not a float texture
    /// store, or not found) hair keeps the stand-in fibre model. Returns
    /// what happened, for the status line.
    pub fn load_hair_luts(&mut self, rs: &egui_wgpu::RenderState, game: &std::path::Path) -> Option<String> {
        if self.hair_tried {
            return None;
        }
        self.hair_tried = true;
        match sixng_core::hairlight::load(game) {
            Ok(l) if l.len() == 4 => {
                self.hair_luts = l.iter().enumerate().map(|(i, t)| hair_texture(&rs.device, &rs.queue, t.w, t.h, t.d, i != 2, &t.rgba16())).collect();
                // models without joints of their own are drawn with the shared frame group
                self.default_frame_bg = frame_bind_group(&rs.device, &self.bgl_frame, &self.frame_ub, &self.dummy_joints, &self.sampler, &self.stickers.none, &self.stickers.view, &self.stickers.sampler, &self.atmo.binds, &self.hair_luts, &self.sun_map);
                if std::env::var_os("SIXNG_DEBUG_HAIR").is_some() {
                    eprintln!("[hair] lighting tables: {:?}", l.iter().map(|t| (t.w, t.h, t.d, t.ch, t.v.iter().cloned().fold(0.0f32, f32::max))).collect::<Vec<_>>());
                }
                None
            }
            Ok(_) => Some("hair lighting tables: not all four found".into()),
            Err(e) => Some(format!("hair lighting tables: {e}")),
        }
    }

    /// Whether the game's hair lighting tables are loaded.
    pub fn hair_luts_loaded(&self) -> bool {
        self.hair_tried && self.hair_luts.len() == 4
    }

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
                    Some(t) => Some(Tex::own(t.clone())),
                    None if has_pixels(i) => {
                        let t = tex_rgba(device, queue, i);
                        c.insert(i.key.clone(), t.clone());
                        Some(Tex::own(t))
                    }
                    None => None,
                },
                None => has_pixels(i).then(|| Tex::own(tex_rgba(device, queue, i))),
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
            stickers: self.stickers.clone(),
            hair_luts: self.hair_luts.clone(),
            sun_map: self.sun_map.clone(),
            scene: self.atmo.binds.clone(),
            gd: self.gd.clone(),
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
    stickers: StickerDefaults,
    hair_luts: Vec<wgpu::TextureView>,
    sun_map: (wgpu::TextureView, wgpu::Sampler, wgpu::TextureView),
    scene: crate::atmo::SceneBinds,
    /// The map's material table (gpu_draw.rs), when the path is on.
    gd: Option<std::sync::Arc<crate::gpu_draw::Shared>>,
}

impl Uploader {
    /// Upload an image: into the map's shared texture arrays when it has
    /// them, else as a texture of its own.
    pub fn tex(&self, device: &wgpu::Device, queue: &wgpu::Queue, img: &glb::Image) -> Tex {
        tex_upload(device, queue, img, self.gd.as_ref().filter(|_| !crate::gpu_draw::opt("noarray")))
    }

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
        // batches that draw at the same distance next to each other (see draw_cells)
        let (indices, batches) = c.draw_order();
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cell ib"),
            contents: if indices.is_empty() {
                &[0u8; 4]
            } else {
                bytemuck::cast_slice(&indices)
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
            batches,
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
        get_tex: &mut dyn FnMut(&glb::Image) -> Option<Tex>,
    ) -> GpuModel {
        let mut textures: Vec<Tex> = Vec::new();
        let mut views: Vec<Option<wgpu::TextureView>> = Vec::new();
        // per image its slot in the map's texture arrays and the table
        // generation that shows it ((0, 0): white; None: a texture of its
        // own, which the table cannot read)
        let mut slots_of: Vec<Option<(u32, u64)>> = Vec::new();
        for i in &m.images {
            let t = get_tex(i);
            views.push(t.as_ref().map(|t| t.view.clone()));
            slots_of.push(match &t {
                None => Some((0, 0)),
                Some(t) => t.layer.as_ref().map(|l| (l.slot, l.need)),
            });
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
        let mut mat_glass = Vec::new();
        let mut mat_class = Vec::new();
        let mut mat_decal = Vec::new();
        let mut mat_src: Vec<[Option<usize>; 8]> = Vec::new();
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
            ped_shade: false,
            detail: None,
            ramp_u: None,
            wear_mask: None,
            primer_mask: None,
            dust_mask: None,
            base_uv: 0,
            ramp: None,
            parts: false,
            occlusion_only: false,
            skin_spec: None,
            skin_detail: None,
            glass: None,
            emissive_scale: 1.0,
            lit: false,
            emissive_tint: [1.0; 3],
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
            // bit 6: the base colour is laid out in the fourth UV set
            let has = (mt.base.is_some() as u32)
                | ((mt.normal.is_some() as u32) << 1)
                | ((mt.packed.is_some() as u32) << 2)
                | ((mt.emissive.is_some() as u32) << 3)
                | (((mt.base_uv == 3) as u32) << 6);
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
            }) | if mt.tint_mask { 0x200 } else { 0 }
                // bit 10: the vertex tint's alpha carries metalness and roughness
                // (vehicle parts; weapon parts, by the `weapon` shader's hash)
                | if mt.parts || mt.shader_hash == "e68a4a98" { 0x400 } else { 0 }
                // bit 11: a lamp's emissive mesh (standard_vehicle_emissive_animated_bones)
                | if mt.name.ends_with("29648098") { 0x800 } else { 0 }
                // bit 12: an emissive shader outside the emissive class
                // (a bulb's emissive layer, a neon tube): lit like class 7
                | if mt.lit && mt.class != "emissive" { 0x1000 } else { 0 }
                // bit 13: the packed map is a vehicle material map, blue = occlusion
                | if mt.occlusion_only { 0x2000 } else { 0 }
                // bit 5: the packed map is ped skin's spec map (green =
                // roughness), with the specular factor x 100 in bits 16-23
                | match (mt.skin_spec, mt.packed, mt.mr) {
                    (Some(f), Some(_), None) => 0x20 | (((f.clamp(0.0, 2.55) * 100.0 + 0.5) as u32) << 16),
                    _ => 0,
                }
                // bit 6: ped skin's engine detail is bound (bindings 6-8:
                // speculartex2, the normal and control sheets); bit 7: skin
                // occlusion (strength in tile.x)
                | match (mt.skin_spec, mt.packed, mt.mr, mt.skin_detail) {
                    (Some(_), Some(_), None, Some(d)) if d.spec2.is_some() && d.normal.is_some() && d.control.is_some() => 0x40,
                    _ => 0,
                }
                | match (mt.skin_spec, mt.packed, mt.mr, mt.skin_detail) {
                    (Some(_), Some(_), None, Some(d)) if d.occlusion > 0.0 => 0x80,
                    _ => 0,
                }
                // bit 14: the emissive slot holds the part ramp image, whose
                // rows 512.. are the parts' surfaces (low and high metalness
                // and roughness)
                | if mt.ramp.is_some() && mt.emissive.is_none() { 0x4000 } else { 0 }
                // bit 15: the game's wear mask is bound (binding 6)
                | if mt.wear_mask.is_some() && mt.ramp.is_some() && mt.emissive.is_none() { 0x8000 } else { 0 }
                // bit 4: the game's dust and scratch mask is bound (binding 8)
                | if mt.dust_mask.is_some() && mt.wear_mask.is_some() && mt.ramp.is_some() && mt.emissive.is_none() { 0x10 } else { 0 };
            let u = MatU {
                base,
                flags: [has, class_id(&mt.class), alpha, gap | mr],
                // w: 1 = fabric detail tile, 2 = vehicle part ramps (the
                // palette column map in the tile slot, the rows in the
                // emissive slot)
                tile: match (mt.detail, mt.ramp_u.zip(mt.ramp).filter(|_| mt.emissive.is_none()), mt.skin_detail.filter(|_| mt.skin_spec.is_some())) {
                    (Some(d), _, _) => [d.scale, d.rot, d.mean, 1.0],
                    (None, Some(_), _) => [0.0, 0.0, 0.0, 2.0],
                    // ped skin (w stays 0): x occlusion strength, y detail normal
                    // strength, z 1 + translucency bias (0 = no translucency)
                    (None, None, Some(s)) => [s.occlusion, s.strength[1], s.translucency.map_or(0.0, |b| 1.0 + b.clamp(0.0, 1.0)), 0.0],
                    _ => [0.0; 4],
                },
                emis: [mt.emissive_scale, mt.emissive_tint[0], mt.emissive_tint[1], mt.emissive_tint[2]],
                // ped skin: yz detail UV tiling, w detail colour strength
                strand: match mt.skin_detail.filter(|_| mt.skin_spec.is_some()) {
                    Some(s) => [1.0, s.uv_scale[0], s.uv_scale[1], s.strength[0]],
                    None => [1.0, 0.0, 0.0, 0.0],
                },
                glass: mt.glass.map_or([[0.0; 4]; 4], |g| {
                    [
                        [g.absorption[0], g.absorption[1], g.absorption[2], g.absorption_albedo],
                        [g.scattering[0], g.scattering[1], g.scattering[2], g.scattering_albedo],
                        [g.thickness, g.path_cap, g.anisotropy, g.back_front],
                        [g.alpha_divisor, g.opaque_from, g.thickness_mask, 1.0],
                    ]
                }),
            };
            let o = k * MAT_STRIDE as usize;
            mat_bytes[o..o + std::mem::size_of::<MatU>()].copy_from_slice(bytemuck::bytes_of(&u));
            let src = mat_sources(mt);
            let bg =device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view(src[0])) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(view(src[1])) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(view(src[2])) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(view(src[3])) },
                    wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(view(src[4])) },
                    wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(view(src[5])) },
                    wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(view(src[6])) },
                    wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(view(src[7])) },
                ],
            });
            mat_src.push(src);
            mat_bg.push(bg);
            mat_cpu.push(u);
            mat_alpha.push(alpha);
            mat_class.push(class_id(&mt.class));
            mat_decal.push(mt.decal);
            mat_glass.push(mt.glass.is_some());
        }
        queue.write_buffer(&mat_ub, 0, &mat_bytes);
        let vsz = std::mem::size_of::<glb::Vertex>() as u64;
        let nv: u64 = m.meshes.iter().map(|me| me.verts.len() as u64).sum();
        let ni: u64 = m.meshes.iter().map(|me| me.indices.len() as u64).sum();
        let mesh_mat: Vec<usize> = m.meshes.iter().map(|me| me.material.unwrap_or(0).min(mats.len() - 1)).collect();
        let order = draw_order(&mesh_mat, |k| mesh_kind(mat_alpha[k], mat_class[k], mat_decal[k].is_some()));
        // the map's material table: a static model without strand hair whose
        // textures are all in the shared arrays goes into the shared pools
        let pooled = m.joints.is_empty() && m.meshes.iter().all(|me| !me.skinned && me.strand_lod.is_none());
        let gd = self.gd.as_ref().filter(|_| pooled && nv > 0 && ni > 0 && !crate::gpu_draw::opt("nopool")).and_then(|g| {
            let mut need = 0u64;
            let mut rows: Vec<u8> = Vec::with_capacity(mat_cpu.len() * g.row() as usize);
            for (u, src) in mat_cpu.iter().zip(&mat_src) {
                rows.extend_from_slice(bytemuck::bytes_of(u));
                for i in src {
                    let (s, n) = match i.and_then(|i| slots_of.get(i)) {
                        None => (0, 0),
                        Some(None) => return None,
                        Some(Some(x)) => *x,
                    };
                    need = need.max(n);
                    rows.extend_from_slice(&s.to_le_bytes());
                }
            }
            crate::gpu_draw::watch::at(1, 13);
            g.place(&rows, nv, ni, need)
        });
        // one vertex and one index buffer for all meshes (fewer GPU objects:
        // creating them is what costs when a model has many parts); sizes
        // rounded up to 4 bytes, as mapped buffers need. A pooled model's
        // are its ranges of the pool's chunks.
        let (vb, ib) = match &gd {
            Some(h) => (h.buf.clone(), h.buf.clone()),
            None => (
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("vb"),
                    size: (nv * vsz).max(vsz).next_multiple_of(4),
                    // COPY_DST: morph targets (face expressions) rewrite a mesh's vertices
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: true,
                }),
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ib"),
                    size: (ni * 4).max(4),
                    usage: wgpu::BufferUsages::INDEX,
                    mapped_at_creation: true,
                }),
            ),
        };
        // where the model starts in its buffers (bytes); its indices count
        // from its first vertex, which its draws pass as base vertex
        let (v_at, i_at) = gd.as_ref().map_or((0, 0), |h| (h.v_bytes(), h.i_bytes()));
        let base_vertex = gd.as_ref().map_or(0, |h| h.v_first() as i32);
        let mut slots: Vec<Option<GpuMesh>> = (0..m.meshes.len()).map(|_| None).collect();
        {
            let mut maps = gd.is_none().then(|| {
                (
                    vb.slice(..).get_mapped_range_mut().expect("vertex buffer was mapped at creation"),
                    ib.slice(..).get_mapped_range_mut().expect("index buffer was mapped at creation"),
                )
            });
            // a pooled model's data, written to the pool when laid out
            let mut pv: Vec<glb::Vertex> = Vec::new();
            let mut pi: Vec<u32> = Vec::new();
            let (mut vo, mut io) = (0u64, 0u64);
            for &k in &order {
                let me = &m.meshes[k];
                let vbytes: &[u8] = bytemuck::cast_slice(&me.verts);
                // indices count from the model's first vertex (not the
                // mesh's), so neighbouring meshes can be drawn as one range
                let v0 = (vo / vsz) as u32;
                let abs: Vec<u32> = me.indices.iter().map(|&i| i + v0).collect();
                let ibytes: &[u8] = bytemuck::cast_slice(&abs);
                match &mut maps {
                    Some((vmap, imap)) => {
                        vmap.slice(vo as usize..vo as usize + vbytes.len()).copy_from_slice(vbytes);
                        imap.slice(io as usize..io as usize + ibytes.len()).copy_from_slice(ibytes);
                    }
                    None => {
                        // the vertex carries its material's index in the
                        // model (joints are free without a skeleton)
                        pv.extend(me.verts.iter().map(|v| {
                            let mut v = *v;
                            v.joints[3] = mesh_mat[k] as u16;
                            v
                        }));
                        pi.extend_from_slice(&abs);
                    }
                }
                let (vr, ir) = (v_at + vo..v_at + vo + vbytes.len() as u64, i_at + io..i_at + io + ibytes.len() as u64);
                vo += vbytes.len() as u64;
                io += ibytes.len() as u64;
                // an empty range is not a valid slice: point it at the start
                let vr = if vr.is_empty() { 0..vsz } else { vr };
                let ir = if ir.is_empty() { 0..4 } else { ir };
                slots[k] = Some(GpuMesh {
                    vb: vb.clone(),
                    ib: ib.clone(),
                    vb_range: vr,
                    ib_range: ir,
                    n_idx: me.indices.len() as u32,
                    eb: None,
                    n_edge: 0,
                    material: mesh_mat[k],
                    skinned: me.skinned,
                    centre: Vec3::from(me.centre),
                    radius: me
                        .verts
                        .iter()
                        .map(|v| Vec3::from(v.pos).distance_squared(Vec3::from(me.centre)))
                        .fold(0.0f32, f32::max)
                        .sqrt(),
                    visible: true,
                    strand_lod: me.strand_lod,
                    n_draw: std::sync::atomic::AtomicU32::new(me.indices.len() as u32),
                    n_shadow: std::sync::atomic::AtomicU32::new(me.indices.len() as u32),
                    shadow_slot: std::sync::atomic::AtomicU32::new(0),
                });
            }
            if gd.is_some() {
                crate::gpu_draw::watch::at(1, 14);
                queue.write_buffer(&vb, v_at, bytemuck::cast_slice(&pv));
                queue.write_buffer(&ib, i_at, bytemuck::cast_slice(&pi));
                crate::gpu_draw::watch::at(1, 15);
            }
        }
        if gd.is_none() {
            vb.unmap();
            ib.unmap();
        }
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
        let frame_bg = frame_bind_group(device, &self.bgl_frame, &self.frame_ub, &joints_buf, &self.sampler, &self.stickers.none, &self.stickers.view, &self.stickers.sampler, &self.scene, &self.hair_luts, &self.sun_map);
        let inst = [Instance::new(Mat4::IDENTITY, !m.joints.is_empty())];
        let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("inst"),
            contents: bytemuck::cast_slice(&inst),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let meshes: Vec<GpuMesh> = slots.into_iter().map(|s| s.expect("every mesh is laid out once")).collect();
        GpuModel {
            meshes,
            order,
            mat_bg,
            mat_alpha,
            mat_glass,
            mat_class,
            mat_decal,
            mat_ub,
            mat_cpu,
            _textures: textures,
            gd,
            base_vertex,
            joints_buf,
            frame_bg,
            sticker_buf: None,
            instances,
            inst_cpu: Vec::new(),
            n_instances: 1,
            n_fb: 0,
            instance_cap: 1,
            bbox: [Vec3::from(m.bbox[0]), Vec3::from(m.bbox[1])],
            world: None,
            stencil: 1,
            mat_names: mats
                .iter()
                .map(|mt| format!("{} [{}] {}", mt.name, mt.shader_hash, mt.class))
                .collect(),
            drawn: Default::default(),
        }
    }
}

/// The images a material binds at bindings 1-8 of its group (None: white).
fn mat_sources(mt: &glb::Material) -> [Option<usize>; 8] {
    [
        mt.base,
        mt.normal,
        mt.packed,
        mt.emissive.or(mt.ramp),
        mt.detail.map(|d| d.image).or(mt.ramp_u.filter(|_| mt.ramp.is_some() && mt.emissive.is_none())),
        mt.wear_mask.or(mt.skin_detail.and_then(|d| d.spec2)),
        mt.primer_mask.or(mt.wear_mask).or(mt.skin_detail.and_then(|d| d.normal)),
        mt.dust_mask.or(mt.wear_mask).or(mt.skin_detail.and_then(|d| d.control)),
    ]
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
            if let Some(h) = &mut gm.gd {
                h.set_instances(inst);
            }
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
            if let Some(h) = &mut gm.gd {
                h.set_instances(&v);
            }
        }
        gm.inst_cpu.clear();
        if inst.len() <= CULL_MAX_INST {
            let ordered = inst.iter().filter(|i| !i.is_fallback()).chain(inst.iter().filter(|i| i.is_fallback()));
            gm.inst_cpu.extend(ordered.map(|i| {
                let m = Mat4::from_cols_array_2d(&i.m);
                let s = m.x_axis.truncate().length().max(m.y_axis.truncate().length()).max(m.z_axis.truncate().length());
                (m, s)
            }));
        }
        gm.n_instances = inst.len() as u32;
        gm.n_fb = n_fb as u32;
        gm.world = world_sphere(gm.bbox, inst.iter().filter(|i| !i.is_fallback()));
        self.bump_accum(bytemuck::cast_slice(inst));
    }

    /// Give a model its stickers: the shader records (header first, see
    /// `livery::records`) and the pictures' texture array (None while it has
    /// none). The model keeps a buffer of its own from then on.
    pub fn set_stickers(&self, rs: &egui_wgpu::RenderState, gm: &mut GpuModel, recs: &[crate::livery::Record], pictures: Option<&wgpu::TextureView>) {
        let cap = crate::livery::MAX_STICKERS + 2;
        let buf = gm.sticker_buf.get_or_insert_with(|| {
            rs.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("stickers"),
                size: (cap * std::mem::size_of::<crate::livery::Record>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let n = recs.len().min(cap);
        if n > 0 {
            rs.queue.write_buffer(buf, 0, bytemuck::cast_slice(&recs[..n]));
        }
        self.bump_accum(bytemuck::cast_slice(&recs[..n]));
        gm.frame_bg = frame_bind_group(&rs.device, &self.bgl_frame, &self.frame_ub, &gm.joints_buf, &self.sampler, buf, pictures.unwrap_or(&self.stickers.view), &self.stickers.sampler, &self.atmo.binds, &self.hair_luts, &self.sun_map);
    }

    /// Write a skeleton's joint matrices into a model's joints buffer. A
    /// model uploaded for another skeleton (fewer joints) is left alone:
    /// the write would overrun its buffer (a wgpu validation panic).
    pub fn set_joints(&self, rs: &egui_wgpu::RenderState, gm: &GpuModel, mats: &[Mat4]) {
        if !joints_fit(gm.joints_buf.size(), mats.len()) {
            return;
        }
        let v: Vec<f32> = mats.iter().flat_map(|m| m.to_cols_array()).collect();
        rs.queue
            .write_buffer(&gm.joints_buf, 0, bytemuck::cast_slice(&v));
        self.bump_accum(bytemuck::cast_slice(&v));
    }

    /// Replace one mesh's vertices (same count as uploaded), e.g. after
    /// morph targets changed its shape.
    pub fn write_vertices(&self, rs: &egui_wgpu::RenderState, gm: &GpuModel, mesh: usize, verts: &[glb::Vertex]) {
        let Some(me) = gm.meshes.get(mesh) else { return };
        let bytes: &[u8] = bytemuck::cast_slice(verts);
        if bytes.len() as u64 != me.vb_range.end - me.vb_range.start || bytes.len() % 4 != 0 {
            return;
        }
        rs.queue.write_buffer(&me.vb, me.vb_range.start, bytes);
        self.bump_accum(&bytes[..bytes.len().min(4096)]);
    }

    /// Restart the still-camera mean when data it shows changed outside `render`.
    fn bump_accum(&self, bytes: &[u8]) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        self.accum_bump.fetch_add(h.finish() | 1, std::sync::atomic::Ordering::Relaxed);
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
            if let Some(h) = &gm.gd {
                h.write_mat(k, bytemuck::bytes_of(u));
            }
            self.bump_accum(bytemuck::bytes_of(u));
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
            if let Some(h) = &gm.gd {
                h.write_mat(k, bytemuck::bytes_of(u));
            }
            self.bump_accum(bytemuck::bytes_of(u));
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
            .map(|t| t.size == size && t.accum.is_some() == self.accumulate)
            .unwrap_or(false)
        {
            return;
        }
        self.accum_n = 0;
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
        let color_fmt = if self.hdr { crate::atmo::HDR_FMT } else { COLOR_FMT };
        let msaa = mk(MSAA, color_fmt, wgpu::TextureUsages::RENDER_ATTACHMENT)
            .create_view(&Default::default());
        let depth_tex = mk(
            MSAA,
            self.depth_fmt,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth = depth_tex.create_view(&Default::default());
        let depth_read = depth_tex.create_view(&wgpu::TextureViewDescriptor {
            aspect: wgpu::TextureAspect::DepthOnly,
            ..Default::default()
        });
        let scene_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene depth"),
            layout: &self.bgl_scene,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&depth_read),
            }],
        });
        let resolve = mk(
            1,
            color_fmt,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        );
        let resolve_view = resolve.create_view(&Default::default());
        let display = self.hdr.then(|| {
            let t = mk(
                1,
                crate::atmo::DISPLAY_FMT,
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
            );
            let v = t.create_view(&Default::default());
            (t, v)
        });
        let frame = display.as_ref().map(|d| &d.1).unwrap_or(&resolve_view);
        let accum = self.accumulate.then(|| {
            let out_fmt = if self.hdr { crate::atmo::DISPLAY_FMT } else { color_fmt };
            if self.accum.as_ref().is_some_and(|a| a.out_fmt != out_fmt) {
                self.accum = None;
            }
            self.accum.get_or_insert_with(|| crate::accum::Accum::new(device, out_fmt)).targets(device, frame, size)
        });
        let shown = accum.as_ref().map(|a| &a.out_view).unwrap_or(frame);
        let mut r = rs.renderer.write();
        let egui_id = match &self.target {
            Some(t) => {
                r.update_egui_texture_from_wgpu_texture(
                    device,
                    shown,
                    wgpu::FilterMode::Linear,
                    t.egui_id,
                );
                t.egui_id
            }
            None => r.register_native_texture(device, shown, wgpu::FilterMode::Linear),
        };
        self.target = Some(Target {
            size,
            msaa,
            depth,
            scene_bg,
            resolve,
            resolve_view,
            display,
            accum,
            egui_id,
        });
    }

    pub fn frame_uniform(&self, cam: &Camera, look: &Look, aspect: f32, time: f32) -> FrameU {
        let vp = cam.proj(aspect) * cam.view();
        let lin = |c: [f32; 3]| [c[0], c[1], c[2], 1.0];
        let mut paint1 = lin(look.paint);
        paint1[3] = look.coat_base.unwrap_or(look.stored_finish[1]);
        let mut sky_top = lin(look.sky_top);
        sky_top[3] = look.coat[3];
        FrameU {
            view_proj: vp.to_cols_array_2d(),
            inv_view_proj: vp.inverse().to_cols_array_2d(),
            cam_pos: cam.eye().extend(1.0).to_array(),
            sun_dir: look.sun_dir.extend(look.sun_intensity).to_array(),
            sun_col: [look.sun_col[0], look.sun_col[1], look.sun_col[2], look.bounce],
            sky_top,
            sky_hor: [
                look.sky_hor[0],
                look.sky_hor[1],
                look.sky_hor[2],
                look.exposure,
            ],
            mode: [look.mode, look.rma_order | if look.skin_detail { 0 } else { 256 }, look.finish, look.slot],
            paint1,
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
                look.stored_finish[0],
            ],
            prev_view_proj: vp.to_cols_array_2d(),
            // xyz: the body paint's coat; w: vehicle wear (0 to 1)
            // w < 0: each part's own wear amount from the part ramp image
            // -1 - a (a > 0): the same, and the primer layer at amount a
            jitter: [look.coat[0], look.coat[1], look.coat[2], if look.wear_from_config && look.wear_levels { -1.0 - look.primer_amount.clamp(0.0, 1.0) } else { look.wear }],
            frame: [self.dither_frame, self.target.as_ref().map_or(aspect * 1080.0, |t| t.size[0] as f32), self.target.as_ref().map_or(1080.0, |t| t.size[1] as f32), 0.0],
            light_view_proj: self.light_vp,
        }
    }

    /// Render models (each with its own instances) and lines into the
    /// offscreen target; returns the egui texture to show.
    /// Depth of the opaque surfaces as the sun sees them (the sun shadow
    /// map). Strand pieces with the count rule draw ShadowLodScale of their
    /// main share, as the game's sun shadow pass does. Models skinned on the GPU are left out (their joints are bound
    /// with the main pass's frame group); the character viewer poses its
    /// parts on the CPU.
    /// `eye`: the camera depth pre-pass instead (the strands of the main pass, one frame group).
    fn record_sun_map(pipe: &wgpu::RenderPipeline, frame_bg: &wgpu::BindGroup, extra: &[(wgpu::Buffer, wgpu::BindGroup)], be: &mut wgpu::RenderPass<'_>, models: &[&GpuModel], eye: bool) {
        be.set_pipeline(pipe);
        be.set_bind_group(0, Some(frame_bg), &[]);
        let mut slot = 0usize;
        for gm in models {
            if gm.n_normal() == 0 || gm.meshes.iter().any(|m| m.skinned) {
                continue;
            }
            be.set_vertex_buffer(1, Some(gm.instances.slice(..)));
            let mut bound: Option<&wgpu::Buffer> = None;
            for me in gm.ordered().filter(|m| m.visible && m.n_idx > 0) {
                if gm.mat_alpha[me.material] == 2 || matches!(gm.mat_class[me.material], 5 | 6 | 8) || gm.mat_decal[me.material].is_some() {
                    continue;
                }
                let want = if eye { 0 } else { (me.shadow_slot.load(std::sync::atomic::Ordering::Relaxed) as usize).min(extra.len()) };
                if want != slot {
                    be.set_bind_group(0, Some(if want == 0 { frame_bg } else { &extra[want - 1].1 }), &[]);
                    slot = want;
                }
                be.set_bind_group(1, Some(&gm.mat_bg[me.material]), &[]);
                if bound != Some(&me.vb) {
                    be.set_vertex_buffer(0, Some(me.vb.slice(..)));
                    be.set_index_buffer(me.ib.slice(..), wgpu::IndexFormat::Uint32);
                    bound = Some(&me.vb);
                }
                let first = me.first();
                let cnt = if me.strand_lod.is_some() { if eye { &me.n_draw } else { &me.n_shadow }.load(std::sync::atomic::Ordering::Relaxed).min(me.n_idx) } else { me.n_idx };
                if cnt > 0 {
                    be.draw_indexed(first..first + cnt, gm.base_vertex, 0..gm.n_normal());
                }
            }
        }
    }

    /// The strand pieces of `models` as the main pass draws them (same frame groups, instances and strand
    /// counts), for the hair blur's mask. Returns the number of draws.
    fn record_strand_mask(pipe: &wgpu::RenderPipeline, default_frame_bg: &wgpu::BindGroup, be: &mut wgpu::RenderPass<'_>, models: &[&GpuModel]) -> u32 {
        let mut n = 0u32;
        be.set_pipeline(pipe);
        for gm in models {
            let range = 0..gm.n_normal();
            if range.is_empty() || !gm.meshes.iter().any(|m| m.visible && m.strand_lod.is_some()) {
                continue;
            }
            let skinned = gm.meshes.iter().any(|m| m.skinned);
            be.set_bind_group(0, Some(if skinned { &gm.frame_bg } else { default_frame_bg }), &[]);
            be.set_vertex_buffer(1, Some(gm.instances.slice(..)));
            for me in gm.meshes.iter().filter(|m| m.visible && m.n_idx > 0 && m.strand_lod.is_some()) {
                if gm.mat_alpha[me.material] == 2 || matches!(gm.mat_class[me.material], 5 | 6 | 8) {
                    continue;
                }
                be.set_bind_group(1, Some(&gm.mat_bg[me.material]), &[]);
                be.set_vertex_buffer(0, Some(me.vb.slice(..)));
                be.set_index_buffer(me.ib.slice(..), wgpu::IndexFormat::Uint32);
                let first = me.first();
                let cnt = me.n_draw.load(std::sync::atomic::Ordering::Relaxed).min(me.n_idx);
                if cnt > 0 {
                    // the indices address the whole vertex buffer: no base vertex
                    be.draw_indexed(first..first + cnt, gm.base_vertex, range.clone());
                    n += 1;
                }
            }
        }
        n
    }

    /// Draw sorted decal or see-through meshes (pipeline, model number,
    /// model, mesh) in the given order. Groups and buffers are bound only
    /// when they change, and a mesh that follows straight on from the
    /// pending draw (same pipeline, model and material) joins it. A mesh
    /// the per-mesh cull hides (`shown` false) is not drawn, but when it
    /// follows on from the pending draw a later mesh can still join across
    /// it, so the cull never splits a draw. Returns the number of draws.
    fn record_joined<'a>(
        be: &mut wgpu::RenderPass<'_>,
        list: impl Iterator<Item = (&'a wgpu::RenderPipeline, usize, &'a GpuModel, &'a GpuMesh, bool)>,
    ) -> u32 {
        let mut n = 0u32;
        let mut pipe: Option<&wgpu::RenderPipeline> = None;
        let mut model = usize::MAX;
        let mut mat = usize::MAX;
        let mut inst = 0..0u32;
        let mut base = 0i32;
        let mut run: Option<std::ops::Range<u32>> = None;
        // end of the hidden meshes that follow on from the run
        let mut reach = u32::MAX;
        for (p, mi, gm, me, shown) in list {
            let first = me.first();
            let same_pipe = pipe.is_some_and(|q| std::ptr::eq(q, p));
            let joins = same_pipe && model == mi && mat == me.material;
            if !shown {
                if run.is_some() && joins && reach == first {
                    reach = first + me.n_idx;
                }
                continue;
            }
            if let Some(r) = &mut run {
                if joins && (r.end == first || reach == first) {
                    r.end = first + me.n_idx;
                    reach = r.end;
                    continue;
                }
                be.draw_indexed(r.clone(), base, inst.clone());
                n += 1;
            }
            if !same_pipe {
                be.set_pipeline(p);
                pipe = Some(p);
            }
            if model != mi {
                be.set_bind_group(0, Some(&gm.frame_bg), &[]);
                be.set_vertex_buffer(1, Some(gm.instances.slice(..)));
                be.set_vertex_buffer(0, Some(me.vb.slice(..)));
                be.set_index_buffer(me.ib.slice(..), wgpu::IndexFormat::Uint32);
                model = mi;
                mat = usize::MAX;
                inst = 0..gm.n_normal();
                base = gm.base_vertex;
            }
            if mat != me.material {
                be.set_bind_group(1, Some(&gm.mat_bg[me.material]), &[]);
                mat = me.material;
            }
            run = Some(first..first + me.n_idx);
            reach = first + me.n_idx;
        }
        if let Some(r) = run {
            be.draw_indexed(r, base, inst);
            n += 1;
        }
        n
    }

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
        cull: Option<&MeshCull>,
        only_decals: bool,
    ) -> u32 {
        let mut n = 0u32;
        let mut culled = 0u32;
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
            // pending run of indices: neighbouring meshes with the same
            // material and buffers are one draw
            let mut run: Option<std::ops::Range<u32>> = None;
            // end of the culled meshes of the run's material that follow
            // straight on from it: a later mesh starting there still joins
            // (drawing them costs only vertex work), so the cull never
            // splits one draw into two
            let mut reach = u32::MAX;
            let ci = cull.zip(cull_inst(gm, &range));
            for me in gm.ordered().filter(|m| m.visible && m.n_idx > 0) {
                let blend = gm.mat_alpha[me.material] == 2
                    || matches!(gm.mat_class[me.material], 5 | 6 | 8);
                if blend && mode == 0 {
                    continue;
                }
                if decal_bias && gm.mat_decal[me.material].is_some() {
                    continue;
                }
                // the material table draws the model's other meshes
                if only_decals && gm.mat_decal[me.material].is_none() {
                    continue;
                }
                // strand hair: the first strands the game's count rule keeps at this distance
                let cnt = if me.strand_lod.is_some() { me.n_draw.load(std::sync::atomic::Ordering::Relaxed).min(me.n_idx) } else { me.n_idx };
                if cnt == 0 {
                    continue;
                }
                let first = me.first();
                if let Some((c, ir)) = &ci {
                    if me.strand_lod.is_none() && !c.mesh(&gm.inst_cpu[ir.clone()], me) {
                        if run.is_some() && mat == me.material && bound == Some(&me.vb) && reach == first {
                            reach = first + cnt;
                        }
                        culled += 1;
                        continue;
                    }
                }
                if let Some(r) = &mut run {
                    if mat == me.material && bound == Some(&me.vb) && (r.end == first || reach == first) && me.strand_lod.is_none() {
                        r.end = first + cnt;
                        reach = r.end;
                        continue;
                    }
                    be.draw_indexed(r.clone(), gm.base_vertex, range.clone());
                    n += 1;
                    gm.drawn[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    run = None;
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
                if me.strand_lod.is_some() {
                    be.draw_indexed(first..first + cnt, gm.base_vertex, range.clone());
                    n += 1;
                    gm.drawn[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                } else {
                    run = Some(first..first + cnt);
                    reach = first + cnt;
                }
            }
            if let Some(r) = run {
                be.draw_indexed(r, gm.base_vertex, range.clone());
                n += 1;
                gm.drawn[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        MESH_CULLED[fb as usize].fetch_add(culled, std::sync::atomic::Ordering::Relaxed);
        n
    }

    /// Draw the models' opaque and cut-out meshes into a shadow cascade:
    /// no skinned models, see-through or decal surfaces or coverage
    /// fallbacks, and no model smaller than `min_radius` (m).
    fn record_shadow(
        pipe: &wgpu::RenderPipeline,
        bg: &wgpu::BindGroup,
        be: &mut wgpu::RenderPass<'_>,
        models: &[&GpuModel],
        c: &crate::shadow::Cascade,
    ) -> u32 {
        let mut n = 0u32;
        let mut culled = 0u32;
        be.set_pipeline(pipe);
        be.set_bind_group(0, Some(bg), &[]);
        for gm in models {
            let range = 0..gm.n_normal();
            if range.is_empty() || gm.meshes.iter().any(|m| m.skinned) {
                continue;
            }
            if c.min_radius > 0.0 && (gm.bbox[1] - gm.bbox[0]).length() * 0.5 < c.min_radius {
                continue;
            }
            if let Some((wc, wr)) = gm.world {
                if !crate::shadow::in_cascade(c, wc, wr) {
                    continue;
                }
            }
            be.set_vertex_buffer(1, Some(gm.instances.slice(..)));
            let mut bound: Option<&wgpu::Buffer> = None;
            // the material bound now, and whether it is cut out
            let mut mat: Option<(usize, bool)> = None;
            // solid surfaces read the material only for its cut-out flag, so
            // neighbouring solid meshes are one draw whatever their material
            let mut run: Option<std::ops::Range<u32>> = None;
            // culled solid meshes that follow on from the run (see `record_opaque`)
            let mut reach = u32::MAX;
            let ci = cull_inst(gm, &range);
            let cc = MeshCull::Cascade(c);
            for me in gm.ordered().filter(|m| m.visible && m.n_idx > 0) {
                let see_through = gm.mat_alpha[me.material] == 2
                    || matches!(gm.mat_class[me.material], 5 | 6 | 8);
                if see_through || gm.mat_decal[me.material].is_some() {
                    continue;
                }
                let cut = gm.mat_alpha[me.material] == 1;
                let first = me.first();
                if let Some(ir) = &ci {
                    if !cc.mesh(&gm.inst_cpu[ir.clone()], me) {
                        if !cut && run.is_some() && bound == Some(&me.vb) && reach == first {
                            reach = first + me.n_idx;
                        }
                        culled += 1;
                        continue;
                    }
                }
                if let Some(r) = &mut run {
                    if !cut && bound == Some(&me.vb) && (r.end == first || reach == first) {
                        r.end = first + me.n_idx;
                        reach = r.end;
                        continue;
                    }
                    be.draw_indexed(r.clone(), gm.base_vertex, range.clone());
                    n += 1;
                    gm.drawn[1].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    run = None;
                }
                // a cut-out mesh needs its own material, a solid one any
                // solid material
                let rebind = match mat {
                    None => true,
                    Some((m, c)) => {
                        if cut {
                            m != me.material
                        } else {
                            c
                        }
                    }
                };
                if rebind {
                    be.set_bind_group(1, Some(&gm.mat_bg[me.material]), &[]);
                    mat = Some((me.material, cut));
                }
                if bound != Some(&me.vb) {
                    be.set_vertex_buffer(0, Some(me.vb.slice(..)));
                    be.set_index_buffer(me.ib.slice(..), wgpu::IndexFormat::Uint32);
                    bound = Some(&me.vb);
                }
                if cut {
                    be.draw_indexed(first..first + me.n_idx, gm.base_vertex, range.clone());
                    n += 1;
                    gm.drawn[1].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                } else {
                    run = Some(first..first + me.n_idx);
                    reach = first + me.n_idx;
                }
            }
            if let Some(r) = run {
                be.draw_indexed(r, gm.base_vertex, range.clone());
                n += 1;
                gm.drawn[1].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        MESH_CULLED[2].fetch_add(culled, std::sync::atomic::Ordering::Relaxed);
        n
    }

    /// Move models out of the emptiest chunk of the material table's
    /// geometry pool into fuller ones, a few MB per frame, so the chunk
    /// empties and goes back to the device: models come and go as the map
    /// streams, and the holes they leave would otherwise keep every chunk
    /// alive. Runs on the thread that renders, before the frame.
    pub fn compact<'a>(&self, rs: &egui_wgpu::RenderState, models: impl Iterator<Item = &'a mut GpuModel>) {
        const BUDGET: u64 = 8 << 20;
        let Some(g) = &self.gd else {
            return;
        };
        let Some(from) = g.compact_from() else {
            return;
        };
        let mut enc = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pool compact") });
        let mut moved = 0u64;
        for gm in models {
            if moved >= BUDGET {
                break;
            }
            let Some(h) = gm.gd.as_mut().filter(|h| h.gc == from) else {
                continue;
            };
            let Some((dv, di)) = h.relocate(&mut enc) else {
                break;
            };
            let buf = h.buf.clone();
            gm.base_vertex = h.v_first() as i32;
            let shift = |r: &std::ops::Range<u64>, d: i64| (r.start as i64 + d) as u64..(r.end as i64 + d) as u64;
            for me in &mut gm.meshes {
                me.vb = buf.clone();
                me.ib = buf.clone();
                // an empty mesh keeps its stand-in range at the start
                if me.n_idx == 0 {
                    continue;
                }
                me.vb_range = shift(&me.vb_range, dv);
                me.ib_range = shift(&me.ib_range, di);
                moved += me.vb_range.end - me.vb_range.start + me.ib_range.end - me.ib_range.start;
            }
        }
        if moved > 0 {
            rs.queue.submit([enc.finish()]);
        }
    }

    /// The material table's part of the tick line (empty when it is off).
    pub fn gd_summary(&self) -> String {
        let Some(g) = &self.gd else {
            return String::new();
        };
        let [sub, calls, old, on] = self.gd_stats;
        format!(" | table: {on} models in {sub} sub-draws, {calls} calls, {old} models the old way; {}", g.summary())
    }

    /// The frame's shadow cascades and which of them are drawn this frame
    /// (None: no shadows).
    fn shadow_plan(&self, cam: &Camera, look: &Look, aspect: f32) -> Option<([crate::shadow::Cascade; crate::shadow::CASCADES], [bool; crate::shadow::CASCADES])> {
        let light = look.sun_dir.normalize_or_zero();
        let on = self.hdr && self.shadows && self.sky.is_some() && look.mode == 0 && light.y > 0.02;
        if !on {
            return None;
        }
        let fwd = -cam.view().row(2).truncate();
        Some((crate::shadow::cascades(cam.eye(), fwd, cam.fov, aspect, light, cam.near), self.shadow.due()))
    }

    /// The material table's part of the frame (gpu_draw.rs): per pass the
    /// sub-draws of the models in the shared pools, written to the frame's
    /// indirect buffer. A model's solid and cut-out meshes follow on in its
    /// buffers and read their material from the vertex, so they are one
    /// sub-draw, split only where the per-mesh cull leaves meshes out. The
    /// models it cannot draw are left in `rest` for the old path.
    #[allow(clippy::too_many_arguments)]
    fn gd_frame<'a>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cam: &Camera,
        look: &Look,
        aspect: f32,
        models: &[&'a GpuModel],
        view_cull: &MeshCull,
    ) -> Option<GdFrame<'a>> {
        use crate::gpu_draw::Args;
        use crate::shadow::CASCADES;
        self.gd_stats = [0; 4];
        let g = self.gd.clone()?;
        self.gd_pipes.as_ref()?;
        // every frame: freed ranges come back, the bind group follows new arrays
        crate::gpu_draw::watch::at(0, 2);
        let snap = g.prepare();
        crate::gpu_draw::watch::at(0, 3);
        if look.mode != 0 || crate::gpu_draw::opt("nodraw") {
            return None;
        }
        let plan = self.shadow_plan(cam, look, aspect).filter(|_| !crate::gpu_draw::opt("noshadow"));
        // (stencil, geometry chunk, instance chunk)
        type Key = (u32, usize, usize);
        let mut main: Vec<(Key, Args)> = Vec::new();
        let mut fb: Vec<(Key, Args)> = Vec::new();
        let mut shadow: [Vec<(Key, Args)>; CASCADES] = Default::default();
        let mut solid: [Vec<(Key, Args)>; CASCADES] = Default::default();
        let (mut rest, mut on) = (Vec::new(), Vec::new());
        let mut culled = [0u32; 3];
        // runs of index ranges: (first index, count)
        let push_run = |runs: &mut Vec<(u32, u32)>, first: u32, n: u32| match runs.last_mut() {
            Some(r) if r.0 + r.1 == first => r.1 += n,
            _ => runs.push((first, n)),
        };
        let mut runs: Vec<(u32, u32)> = Vec::new();
        let mut cuts: Vec<(u32, u32)> = Vec::new();
        for &gm in models {
            let Some(h) = gm.gd.as_ref().filter(|h| snap.ready(h)) else {
                rest.push(gm);
                continue;
            };
            let (nn, nall) = (gm.n_normal(), gm.n_instances);
            if nall == 0 {
                continue;
            }
            let Some((nc, _)) = h.n.clone() else {
                rest.push(gm);
                continue;
            };
            on.push(gm);
            let n0 = h.n_first();
            let kind = |me: &GpuMesh| mesh_kind(gm.mat_alpha[me.material], gm.mat_class[me.material], gm.mat_decal[me.material].is_some());
            // the meshes the table draws: solid and cut-out ones (buffer order: solid first)
            let drawn = || gm.ordered().filter(|m| m.visible && m.n_idx > 0 && kind(m) <= 1);
            let args = |r: &(u32, u32), first_instance: u32, n: u32| Args {
                index_count: r.1,
                instance_count: n,
                first_index: r.0,
                base_vertex: gm.base_vertex,
                first_instance,
            };
            // main pass and fallbacks: every drawn mesh the view may show
            for (pass, range) in [(0usize, 0..nn), (1, nn..nall)] {
                if range.is_empty() {
                    continue;
                }
                runs.clear();
                let ci = cull_inst(gm, &range);
                for me in drawn() {
                    if let Some(ir) = &ci {
                        if !view_cull.mesh(&gm.inst_cpu[ir.clone()], me) {
                            culled[pass] += 1;
                            continue;
                        }
                    }
                    push_run(&mut runs, me.first(), me.n_idx);
                }
                let (list, stencil) = if pass == 0 { (&mut main, gm.stencil) } else { (&mut fb, 0) };
                for r in &runs {
                    list.push(((stencil, h.gc, nc), args(r, n0 + range.start, range.end - range.start)));
                }
            }
            // shadow casters: solid and cut-out meshes apart (a solid one
            // needs no texture), normal instances only
            if let (Some((cs, due)), true) = (&plan, nn > 0) {
                let ci = cull_inst(gm, &(0..nn));
                for i in 0..CASCADES {
                    let c = &cs[i];
                    if !due[i] || (c.min_radius > 0.0 && (gm.bbox[1] - gm.bbox[0]).length() * 0.5 < c.min_radius) {
                        continue;
                    }
                    if let Some((wc, wr)) = gm.world {
                        if !crate::shadow::in_cascade(c, wc, wr) {
                            continue;
                        }
                    }
                    let cc = MeshCull::Cascade(c);
                    runs.clear();
                    cuts.clear();
                    for me in drawn() {
                        if let Some(ir) = &ci {
                            if !cc.mesh(&gm.inst_cpu[ir.clone()], me) {
                                culled[2] += 1;
                                continue;
                            }
                        }
                        let to = if kind(me) == 1 { &mut cuts } else { &mut runs };
                        push_run(to, me.first(), me.n_idx);
                    }
                    for r in &runs {
                        solid[i].push(((0, h.gc, nc), args(r, n0, nn)));
                    }
                    for r in &cuts {
                        shadow[i].push(((0, h.gc, nc), args(r, n0, nn)));
                    }
                }
            }
        }
        for (k, n) in culled.iter().enumerate() {
            MESH_CULLED[k].fetch_add(*n, std::sync::atomic::Ordering::Relaxed);
        }
        // per pass: the sub-draws of one set of chunks one after another (a
        // stable sort keeps the models' order inside each)
        let mut all: Vec<Args> = Vec::new();
        let mut lists = |v: &mut Vec<(Key, Args)>| -> Vec<GdList> {
            v.sort_by_key(|(k, _)| *k);
            let mut out: Vec<GdList> = Vec::new();
            for (k, a) in v.drain(..) {
                let at = all.len() as u32;
                all.push(a);
                match out.last_mut() {
                    Some(l) if (l.stencil, l.g, l.n) == k => l.args.end = at + 1,
                    _ => out.push(GdList { stencil: k.0, g: k.1, n: k.2, args: at..at + 1 }),
                }
            }
            out
        };
        let main = lists(&mut main);
        let fb = lists(&mut fb);
        let shadow: [Vec<GdList>; CASCADES] = std::array::from_fn(|i| lists(&mut shadow[i]));
        let shadow_solid: [Vec<GdList>; CASCADES] = std::array::from_fn(|i| lists(&mut solid[i]));
        drop(lists);
        let calls = main.len() + fb.len() + shadow.iter().chain(&shadow_solid).map(|s| s.len()).sum::<usize>();
        self.gd_stats = [all.len() as u32, calls as u32, rest.len() as u32, on.len() as u32];
        if !all.is_empty() {
            let ab = all.len() as u64 * crate::gpu_draw::ARGS;
            if self.gd_args.as_ref().is_none_or(|a| a.size() < ab) {
                self.gd_args = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("material table args"),
                    size: ab.next_power_of_two().max(1 << 16),
                    usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            }
            queue.write_buffer(self.gd_args.as_ref().unwrap(), 0, bytemuck::cast_slice(&all));
        }
        Some(GdFrame { snap, main, fb, shadow, shadow_solid, rest, on })
    }

    /// Record one pass's material-table lists: one indirect call per set of
    /// chunks. `stencil`: set each list's stencil value. `group1`: a fixed
    /// group 1 (the material table alone) in place of the table with its arrays.
    #[allow(clippy::too_many_arguments)]
    fn record_gd(
        be: &mut wgpu::RenderPass<'_>,
        pipe: &wgpu::RenderPipeline,
        frame_bg: &wgpu::BindGroup,
        f: &GdFrame,
        lists: &[GdList],
        args: &wgpu::Buffer,
        stencil: bool,
        group1: Option<&wgpu::BindGroup>,
    ) -> u32 {
        let Some(bg) = group1.or(f.snap.bg.as_ref()) else {
            return 0;
        };
        if lists.is_empty() {
            return 0;
        }
        be.set_pipeline(pipe);
        be.set_bind_group(0, Some(frame_bg), &[]);
        be.set_bind_group(1, Some(bg), &[]);
        let mut n = 0;
        let (mut sref, mut gc, mut nb) = (u32::MAX, usize::MAX, usize::MAX);
        for l in lists {
            let (Some(Some(gb)), Some(Some(inb))) = (f.snap.g.get(l.g), f.snap.n.get(l.n)) else {
                continue;
            };
            if stencil && l.stencil != sref {
                be.set_stencil_reference(l.stencil);
                sref = l.stencil;
            }
            // one buffer holds the chunk's vertices and indices
            if l.g != gc {
                be.set_vertex_buffer(0, Some(gb.slice(..)));
                be.set_index_buffer(gb.slice(..), wgpu::IndexFormat::Uint32);
                gc = l.g;
            }
            if l.n != nb {
                be.set_vertex_buffer(1, Some(inb.slice(..)));
                nb = l.n;
            }

            if crate::gpu_draw::opt("nomulti") {
                for k in l.args.clone() {
                    be.draw_indexed_indirect(args, k as u64 * crate::gpu_draw::ARGS);
                }
            } else {
                be.multi_draw_indexed_indirect(args, l.args.start as u64 * crate::gpu_draw::ARGS, l.args.end - l.args.start);
            }
            n += 1;
        }
        n
    }

    /// Draw the shadow cascades of the frame's light (sun or moon) and set
    /// the uniform the mesh shaders read them with.
    #[allow(clippy::too_many_arguments)]
    fn render_shadows(
        &mut self,
        queue: &wgpu::Queue,
        enc: &mut wgpu::CommandEncoder,
        cam: &Camera,
        look: &Look,
        fu: &FrameU,
        aspect: f32,
        models: &[&GpuModel],
        gd: Option<&GdFrame>,
    ) {
        use crate::shadow::{ShadowU, CASCADES};
        self.shadow_draws = 0;
        let Some((cs, due)) = self.shadow_plan(cam, look, aspect) else {
            self.shadow.drawn = [None; CASCADES];
            queue.write_buffer(&self.shadow.ub, 0, bytemuck::bytes_of(&ShadowU::default()));
            return;
        };
        // the models the material table does not draw
        let models = gd.map_or(models, |f| &f.rest[..]);
        for i in 0..CASCADES {
            if !due[i] {
                continue;
            }
            let c = cs[i];
            let mut sfu = *fu;
            sfu.view_proj = c.view_proj.to_cols_array_2d();
            queue.write_buffer(&self.shadow.frame_ubs[i], 0, bytemuck::bytes_of(&sfu));
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow cascade"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow.layers[i],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let bg = &self.shadow_bgs[i];
            if let (Some(f), Some(p), Some(a)) = (gd, &self.gd_pipes, &self.gd_args) {
                self.shadow_draws += Self::record_gd(&mut pass, &p[3], bg, f, &f.shadow_solid[i], a, false, Some(&f.snap.mats_bg));
                self.shadow_draws += Self::record_gd(&mut pass, &p[2], bg, f, &f.shadow[i], a, false, None);
            }
            self.shadow_draws += Self::record_shadow(&self.pipe_shadow, bg, &mut pass, models, &c);
            // map cells stand in for placements far from the camera: none
            // in the nearest cascade
            if i > 0 {
                self.shadow_draws += Self::draw_cells(
                    &mut pass,
                    &self.pipe_shadow_cell,
                    bg,
                    &self.cells,
                    crate::cells::PASS_OPAQUE,
                    false,
                );
            }
            drop(pass);
            self.shadow.drawn[i] = Some(c);
        }
        let used: [crate::shadow::Cascade; CASCADES] = std::array::from_fn(|i| self.shadow.drawn[i].unwrap_or(cs[i]));
        queue.write_buffer(&self.shadow.ub, 0, bytemuck::bytes_of(&ShadowU::new(&used, true, 1.0)));
        self.shadow.frame += 1;
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
            let mut chunk = u16::MAX;
            // pending range: batches of one chunk in drawing order (cells.rs
            // `draw_order`) join into one draw, also across batches out of
            // range that cost less than half the draw's indices (the vertex
            // program drops their placements)
            let mut run: Option<(u16, std::ops::Range<u32>)> = None;
            let mut flush = |pass: &mut wgpu::RenderPass<'_>, run: Option<(u16, std::ops::Range<u32>)>, chunk: &mut u16| {
                if let Some((ch, r)) = run {
                    if *chunk != ch {
                        pass.set_bind_group(1, &c.chunk_bg[ch as usize], &[]);
                        *chunk = ch;
                    }
                    pass.draw_indexed(r, 0, 0..1);
                    n += 1;
                }
            };
            for &bi in active {
                let Some(b) = c.batches.get(bi as usize) else {
                    continue;
                };
                if b.pass != which || b.count == 0 || b.chunk as usize >= c.chunk_bg.len() {
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
                if let Some((ch, r)) = &mut run {
                    let joins = *ch == b.chunk && b.first >= r.end && (b.first - r.end) * 2 <= (r.end - r.start) + b.count;
                    if joins {
                        r.end = b.first + b.count;
                        continue;
                    }
                }
                flush(pass, run.take(), &mut chunk);
                run = Some((b.chunk, b.first..b.first + b.count));
            }
            flush(pass, run.take(), &mut chunk);
        }
        n
    }

    /// Snap the exposure to the metered value over the next frames (a jump
    /// in weather or place).
    pub fn reset_exposure(&mut self) {
        self.atmo.reset = 3;
    }

    /// Uploads the water maps (fog atlas and water height map) for the
    /// map's water.
    pub fn set_water_maps(&mut self, queue: &wgpu::Queue, m: &sixng_core::water::ViewMaps, ground: Option<&dyn Fn(f32, f32) -> Option<f32>>) {
        self.atmo.set_water_maps(queue, m, ground);
    }

    /// Uploads the placed lights near the camera for the HDR shading
    /// (lights.rs `pack`; an empty grid turns them off).
    pub fn set_lights(&self, queue: &wgpu::Queue, lights: &[crate::lights::GpuLight], grid: &[u32]) {
        self.atmo.set_lights(queue, lights, grid);
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
        // still-camera accumulation: the mean starts over when anything in the view changes, and each frame of it
        // takes the next dither pattern
        let base_dither = self.dither_frame;
        if self.accumulate {
            use std::hash::{Hash, Hasher};
            let mut k = self.frame_uniform(cam, look, aspect, 0.0);
            k.frame = [0.0; 4];
            k.light_view_proj = [[0.0; 4]; 4];
            let mut h = std::collections::hash_map::DefaultHasher::new();
            bytemuck::bytes_of(&k).hash(&mut h);
            size.hash(&mut h);
            base_dither.to_bits().hash(&mut h);
            self.decal_bias.hash(&mut h);
            if let Some(g) = &self.ground {
                bytemuck::bytes_of(&[g.centre.x, g.centre.y, g.centre.z, g.half[0], g.half[1], g.height]).hash(&mut h);
            }
            self.accum_bump.swap(0, std::sync::atomic::Ordering::Relaxed).hash(&mut h);
            for gm in models {
                (*gm as *const GpuModel as usize, gm.meshes.len(), gm.n_instances).hash(&mut h);
                for me in &gm.meshes {
                    (me.visible, me.n_idx).hash(&mut h);
                }
            }
            bytemuck::cast_slice::<LineV, u8>(lines).hash(&mut h);
            bytemuck::cast_slice::<LineV, u8>(lines_on_top).hash(&mut h);
            let key = h.finish();
            if key != self.accum_key {
                self.accum_key = key;
                self.accum_n = 0;
            }
            self.dither_frame = base_dither + self.accum_n as f32;
        }
        // the sun shadow map: one orthographic view from the sun over the models
        let mut shadow_fu = None;
        self.light_vp = [[0.0; 4]; 4];
        if self.sun_maps && !models.is_empty() && models.len() < 64 {
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for me in models.iter().flat_map(|gm| gm.meshes.iter()).filter(|m| m.visible) {
                lo = lo.min(me.centre);
                hi = hi.max(me.centre);
            }
            if lo.x <= hi.x {
                let (c, r) = ((lo + hi) * 0.5, (hi - lo).length() * 0.5 + 0.7);
                let l = look.sun_dir.normalize();
                let up = if l.y.abs() > 0.95 { Vec3::X } else { Vec3::Y };
                let vp = Mat4::orthographic_rh(-r, r, -r, r, 0.1, 2.0 * r + 2.0) * Mat4::look_at_rh(c + l * (r + 1.0), c, up);
                self.light_vp = vp.to_cols_array_2d();
                let mut f = self.frame_uniform(cam, look, aspect, 0.0);
                f.view_proj = vp.to_cols_array_2d();
                f.inv_view_proj = vp.inverse().to_cols_array_2d();
                // strand ribbons open towards the light at their own width: the game's sun shadow pass draws strands
                // solid and has no minimum width (a target size this large makes the minimum vanish)
                f.cam_pos = (c + l * 1000.0).extend(1.0).to_array();
                // w: the accumulation frame, which turns the shadow ribbons (see the vertex program)
                f.frame = [0.0, 1.0e9, 1.0e9, self.dither_frame];
                shadow_fu = Some(f);
            }
        }
        let fu = self.frame_uniform(cam, look, aspect, 0.0);
        queue.write_buffer(&self.frame_ub, 0, bytemuck::bytes_of(&fu));
        if let Some(f) = &shadow_fu {
            queue.write_buffer(&self.shadow_ub, 0, bytemuck::bytes_of(f));
        }
        // strand count by distance (from the eye to the piece's centre; the model's own placement is taken as none)
        let eye = cam.eye();
        // the drawn strands of a piece are as much wider as there are fewer of them (the game's count rule draws
        // a share of the strands at its target thickness, so the piece keeps its cover)
        for gm in models {
            for me in &gm.meshes {
                if let Some(l) = me.strand_lod {
                    let f = l.share((eye - me.centre).length(), crate::glb::STRAND_RULE_LINES, (cam.fov * 0.5).tan());
                    let n = l.indices(me.n_idx, f).max(1);
                    let k = (me.n_idx as f32 / n as f32).clamp(1.0, 50.0);
                    let at = me.material as u64 * MAT_STRIDE + std::mem::offset_of!(MatU, strand) as u64;
                    queue.write_buffer(&gm.mat_ub, at, bytemuck::bytes_of(&[k, 0.0f32, 0.0, 0.0]));
                }
            }
        }
        let mut slots = 0u32;
        for me in models.iter().flat_map(|gm| gm.meshes.iter()) {
            if let Some(l) = me.strand_lod {
                let d = (eye - me.centre).length();
                // the game's rule counts in a fixed 1440-line reference view (an integer global, read from its LOD
                // function), not in the target's own lines: a small window draws as many strands as a large one
                let lines = crate::glb::STRAND_RULE_LINES;
                let f = l.share(d, lines, (cam.fov * 0.5).tan());
                me.n_draw.store(l.indices(me.n_idx, f), std::sync::atomic::Ordering::Relaxed);
                me.n_shadow.store(l.indices(me.n_idx, l.shadow_share(d, lines, (cam.fov * 0.5).tan())), std::sync::atomic::Ordering::Relaxed);
                me.shadow_slot.store(0, std::sync::atomic::Ordering::Relaxed);
                // the ray-traced rule: the first n of the m drawn strands, each m / n times as wide (a piece of its own
                // frame group carries the factor; pieces beyond the groups share the last one)
                if let (true, Some(sf)) = (self.shadow_traced && !self.shadow_extra.is_empty(), shadow_fu.as_ref()) {
                    let m = (l.stored as f32 * f).round();
                    let n = l.traced_strands(m, d, (cam.fov * 0.5).tan());
                    me.n_shadow.store(l.indices(me.n_idx, n / (l.stored as f32).max(1.0)), std::sync::atomic::Ordering::Relaxed);
                    if n > 0.0 {
                        slots = (slots + 1).min(self.shadow_extra.len() as u32);
                        me.shadow_slot.store(slots, std::sync::atomic::Ordering::Relaxed);
                        let mut g = *sf;
                        g.frame[0] = m / n;
                        queue.write_buffer(&self.shadow_extra[slots as usize - 1].0, 0, bytemuck::bytes_of(&g));
                    }
                }
                if std::env::var("SIXNG_DEBUG_HAIR").is_ok() {
                    static ONCE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                    if ONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 12 {
                        eprintln!("strand piece (rule version {}) at {d:.2} m: share {f:.3}", l.version);
                    }
                }
            }
        }
        let lb = (!lines.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("lines"),
                contents: bytemuck::cast_slice(lines),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let glow_b = (self.hdr && !self.glows.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("glows"),
                contents: bytemuck::cast_slice(&self.glows),
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
        // the ground shadow: shaded view, seen from above the ground
        let gv = self
            .ground
            .filter(|g| look.mode == 0 && cam.eye().y > g.centre.y)
            .map(|g| g.verts(look.sun_dir, look.sun_intensity));
        let gb = gv.as_ref().map(|v| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ground shadow"),
                contents: bytemuck::cast_slice(v),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("viewer"),
        });
        // the game's sky: scene values, then its tables for this frame
        let now = std::time::Instant::now();
        let dt = self.last_frame.map(|t| (now - t).as_secs_f32()).unwrap_or(0.016);
        self.last_frame = Some(now);
        let sky = if self.hdr { self.sky } else { None };
        let gslot = self.gpu_timer.as_mut().and_then(|g| g.begin(device));
        let gmark = |g: &Option<crate::gpu_time::GpuTimer>, enc: &mut wgpu::CommandEncoder, i: u32| {
            if let (Some(g), Some(_)) = (g, gslot) {
                g.mark(enc, i);
            }
        };
        gmark(&self.gpu_timer, &mut enc, 0);
        self.atmo.update(queue, sky.as_ref(), cam.proj(aspect) * cam.view(), cam.eye(), size, dt);
        if let Some(sky) = &sky {
            self.atmo.compute(&mut enc, sky);
        }
        gmark(&self.gpu_timer, &mut enc, 1);
        for c in &MESH_CULLED {
            c.store(0, std::sync::atomic::Ordering::Relaxed);
        }
        let view_cull = MeshCull::view(Mat4::from_cols_array_2d(&fu.view_proj));
        // the material table's part (map models in the shared pools),
        // gathered and written before any pass is recorded
        crate::gpu_draw::watch::at(0, 3);
        let gdf = self.gd_frame(device, queue, cam, look, aspect, models, &view_cull);
        crate::gpu_draw::watch::at(0, 4);
        self.render_shadows(queue, &mut enc, cam, look, &fu, aspect, models, gdf.as_ref());
        gmark(&self.gpu_timer, &mut enc, 2);
        let t = self.target.as_ref().unwrap();
        let mut draws = 0u32;
        let late_n: u32;
        // the models the old path draws whole; the table's models with
        // decals in the opaque pass (no decal bias) give it those
        let rest = gdf.as_ref().map_or(models, |f| &f.rest[..]);
        let decals_of: &[&GpuModel] = match &gdf {
            Some(f) if !self.decal_bias => &f.on,
            _ => &[],
        };
        if shadow_fu.is_some() {
            let mut sp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.sun_map.0,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            Self::record_sun_map(&self.pipe_sun_map, &self.shadow_frame_bg, &self.shadow_extra, &mut sp, models, false);
        }
        if shadow_fu.is_some() {
            // the camera depth pre-pass, into the corner of its texture (cleared to far; a view larger than the texture
            // leaves it so, which reads as nothing in front anywhere)
            queue.write_buffer(&self.depth_ub, 0, bytemuck::bytes_of(&fu));
            let mut dp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("camera depth pre-pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.sun_map.2,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if size[0] <= CAM_DEPTH_SIZE && size[1] <= CAM_DEPTH_SIZE {
                dp.set_viewport(0.0, 0.0, size[0] as f32, size[1] as f32, 0.0, 1.0);
                Self::record_sun_map(&self.pipe_cam_depth, &self.depth_frame_bg, &[], &mut dp, models, true);
            }
        }
        {
            let c = if self.hdr { [0.0; 3] } else { look.clear };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewer pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.msaa,
                    resolve_target: None,
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
                    // kept for the blended pass, which reads it
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: (self.depth_fmt == DEPTH_STENCIL_FMT).then_some(
                        wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0),
                            store: wgpu::StoreOp::Store,
                        },
                    ),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            crate::gpu_draw::watch::at(0, 5);
            if look.mode == 0 || look.mode == 7 {
                pass.set_pipeline(&self.pipe_sky);
                pass.set_bind_group(0, &self.default_frame_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            // opaque + mask
            // no render bundles: wgpu 24 never frees a bundle, and with it
            // everything it draws (evicted models stayed in GPU memory)
            if let (Some(f), Some(p), Some(a)) = (&gdf, &self.gd_pipes, &self.gd_args) {
                draws += Self::record_gd(&mut pass, &p[0], &self.default_frame_bg, f, &f.main, a, true, None);
            }
            draws += Self::record_opaque(
                &self.pipe_opaque,
                &self.default_frame_bg,
                self.decal_bias,
                &mut pass,
                rest,
                look.mode,
                false,
                Some(&view_cull),
                false,
            );
            draws += Self::record_opaque(&self.pipe_opaque, &self.default_frame_bg, false, &mut pass, decals_of, look.mode, false, Some(&view_cull), true);
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
            let mut fm = 0;
            if let (Some(f), Some(p), Some(a)) = (&gdf, &self.gd_pipes, &self.gd_args) {
                fm += Self::record_gd(&mut pass, &p[1], &self.default_frame_bg, f, &f.fb, a, true, None);
            }
            fm += Self::record_opaque(
                &self.pipe_fb,
                &self.default_frame_bg,
                self.decal_bias,
                &mut pass,
                rest,
                0,
                true,
                Some(&view_cull),
                false,
            );
            fm += Self::record_opaque(&self.pipe_fb, &self.default_frame_bg, false, &mut pass, decals_of, 0, true, Some(&view_cull), true);
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
            // on the far ground behind the model, under everything see-through
            if let (Some(gb), Some(gv)) = (&gb, &gv) {
                pass.set_pipeline(&self.pipe_ground);
                pass.set_bind_group(0, &self.default_frame_bg, &[]);
                pass.set_vertex_buffer(0, gb.slice(..));
                pass.draw(0..gv.len() as u32, 0..1);
                draws += 1;
            }
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
            let mut late = 0u32;
            let mut late_culled = 0u32;
            if self.decal_bias {
                let eye = cam.eye();
                let mut dl: Vec<(u8, bool, f32, usize, usize, &GpuModel, &GpuMesh, bool)> = Vec::new();
                for (mi, gm) in models.iter().enumerate() {
                    if gm.n_normal() == 0 {
                        continue;
                    }
                    let ci = cull_inst(gm, &(0..gm.n_normal()));
                    let model_d = gm.world.map(|(c, _)| (c - eye).length());
                    for (pos, me) in gm.ordered().enumerate().filter(|(_, m)| m.visible && m.n_idx > 0) {
                        if let Some(o) = gm.mat_decal[me.material] {
                            let blend = look.mode == 0
                                && (gm.mat_alpha[me.material] == 2
                                    || matches!(gm.mat_class[me.material], 5 | 6 | 8));
                            // opaque-mode decals write depth and do not blend, so
                            // their order matters only between coplanar ones: keep
                            // a model's together (far to near by model) so its
                            // meshes of one material join; blended ones go far to
                            // near by mesh
                            let d = if blend { (me.centre - eye).length() } else { model_d.unwrap_or((me.centre - eye).length()) };
                            let shown = ci.as_ref().is_none_or(|ir| view_cull.mesh(&gm.inst_cpu[ir.clone()], me));
                            dl.push((o, blend, d, mi, pos, gm, me, shown));
                        }
                    }
                }
                // by layer order (terrain blend layers 0, other decals 1), then
                // opaque before blended, then far to near
                dl.sort_by(|a, b| {
                    a.0.cmp(&b.0)
                        .then(a.1.cmp(&b.1))
                        .then(b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
                        .then(a.3.cmp(&b.3))
                        .then(a.4.cmp(&b.4))
                });
                late_culled += dl.iter().filter(|e| !e.7).count() as u32;
                late += Self::record_joined(
                    &mut pass,
                    dl.iter().map(|e| (if e.1 { &self.pipe_decal_blend } else { &self.pipe_decal }, e.3, e.5, e.6, e.7)),
                );
            }
            drop(pass);
            crate::gpu_draw::watch::at(0, 6);
            // the blended pass: the depth is read only, so the blended draws
            // can read it as a texture too (the water's depth over the scene
            // under it); nothing from here on writes depth
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewer blended pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.msaa,
                    resolve_target: Some(&t.resolve_view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // blended, back to front by mesh centre
            if look.mode == 0 {
                let eye = cam.eye();
                let mut bl: Vec<(f32, usize, usize, &GpuModel, &GpuMesh, bool)> = Vec::new();
                for (mi, gm) in models.iter().enumerate() {
                    if gm.n_normal() == 0 {
                        continue;
                    }
                    let ci = cull_inst(gm, &(0..gm.n_normal()));
                    for (pos, me) in gm.ordered().enumerate().filter(|(_, m)| m.visible && m.n_idx > 0) {
                        if self.decal_bias && gm.mat_decal[me.material].is_some() {
                            continue;
                        }
                        if gm.mat_alpha[me.material] == 2
                            || matches!(gm.mat_class[me.material], 5 | 6 | 8)
                        {
                            let shown = ci.as_ref().is_none_or(|ir| view_cull.mesh(&gm.inst_cpu[ir.clone()], me));
                            bl.push(((me.centre - eye).length(), mi, pos, gm, me, shown));
                        }
                    }
                }
                bl.sort_by(|a, b| {
                    b.0.partial_cmp(&a.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.1.cmp(&b.1))
                        .then(a.2.cmp(&b.2))
                });
                pass.set_bind_group(2, &t.scene_bg, &[]);
                late_culled += bl.iter().filter(|e| !e.5).count() as u32;
                late += Self::record_joined(&mut pass, bl.iter().map(|e| (if e.3.mat_glass[e.4.material] { self.pipe_glass.as_ref().unwrap_or(&self.pipe_blend) } else { &self.pipe_blend }, e.1, e.3, e.4, e.5)));
            }
            MESH_CULLED[3].fetch_add(late_culled, std::sync::atomic::Ordering::Relaxed);
            draws += late;
            late_n = late;
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
            if let Some(gb) = &glow_b {
                pass.set_pipeline(&self.pipe_glow);
                pass.set_bind_group(0, &self.default_frame_bg, &[]);
                pass.set_vertex_buffer(0, gb.slice(..));
                pass.draw(0..4, 0..self.glows.len() as u32);
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
        // the game's hair blur, on the lit colour before tone mapping: the strand mask first (the strands again
        // over the main pass's depth), then the blur where it found strands
        if let (Some(hb), Some(pm)) = (self.hair_blur.as_mut().filter(|h| h.enabled), &self.pipe_hair_mask) {
            if models.iter().any(|gm| gm.meshes.iter().any(|m| m.visible && m.strand_lod.is_some())) {
                hb.ensure(device, size);
                let (ms, one) = hb.mask_views();
                let mut mp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("hair blur mask"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: ms,
                        resolve_target: Some(one),
                        depth_slice: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Discard },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &t.depth, depth_ops: None, stencil_ops: None }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                let n = Self::record_strand_mask(pm, &self.default_frame_bg, &mut mp, models);
                drop(mp);
                if n > 0 {
                    hb.run(device, queue, &mut enc, &t.resolve, &t.resolve_view, size, cam.near, cam.far);
                }
            }
        }
        gmark(&self.gpu_timer, &mut enc, 3);
        if let (Some(_), Some(d)) = (&sky, &t.display) {
            self.atmo.post(device, &mut enc, &t.resolve_view, &d.1, size);
        }
        if let (Some(a), Some(at)) = (&self.accum, &t.accum) {
            a.run(&mut enc, at, self.accum_n);
        }
        gmark(&self.gpu_timer, &mut enc, 4);
        if let (Some(g), Some(slot)) = (&self.gpu_timer, gslot) {
            g.end(&mut enc, slot);
        }
        crate::gpu_draw::watch::at(0, 7);
        let t_e = std::time::Instant::now();
        let cb = enc.finish();
        RTIME[1].fetch_add(
            t_e.elapsed().as_micros() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        let t_s = std::time::Instant::now();
        crate::gpu_draw::watch::at(0, 8);
        queue.submit([cb]);
        crate::gpu_draw::watch::at(0, 9);
        if let (Some(g), Some(slot)) = (&self.gpu_timer, gslot) {
            g.after_submit(slot);
        }
        RTIME[2].fetch_add(
            t_s.elapsed().as_micros() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        RTIME[3].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.draws = draws;
        self.late_draws = late_n;
        let _ = &self.dummy_joints;
        let id = t.egui_id;
        if self.accumulate {
            self.accum_n = self.accum_n.saturating_add(1);
            self.dither_frame = base_dither;
        }
        id
    }

    /// Frames in the still-camera mean so far (0 when not accumulating).
    pub fn accum_frames(&self) -> u32 {
        if self.accumulate { self.accum_n } else { 0 }
    }

    /// The still-camera mean is still settling: the caller keeps drawing.
    pub fn accumulating(&self) -> bool {
        self.accumulate && self.accum_n < crate::accum::CAP
    }

    /// Read the last rendered frame back as RGBA8 (sRGB-encoded).
    pub fn read_back(&self, rs: &egui_wgpu::RenderState) -> Option<(u32, u32, Vec<u8>)> {
        let t = self.target.as_ref()?;
        let [w, h] = t.size;
        let src = t.accum.as_ref().map(|a| &a.out).or(t.display.as_ref().map(|d| &d.0)).unwrap_or(&t.resolve);
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
                texture: src,
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

/// Does a joints buffer of `bytes` hold `n` joint matrices?
pub fn joints_fit(bytes: u64, n: usize) -> bool {
    (n as u64) * 64 <= bytes
}

#[cfg(test)]
mod joints_tests {
    use super::joints_fit;

    #[test]
    fn joints_fit_the_buffer() {
        // a model without joints gets a one-matrix buffer
        assert!(joints_fit(64, 1));
        assert!(joints_fit(64, 0));
        // an animal's 129-joint skin into a character hair model's buffer
        assert!(!joints_fit(64, 129));
        assert!(joints_fit(129 * 64, 129));
        assert!(joints_fit(135 * 64, 129));
    }
}

#[cfg(test)]
mod order_tests {
    use super::{draw_order, mesh_kind};

    #[test]
    fn draw_order_groups_kinds_then_materials() {
        // materials: 0 solid, 1 see-through, 2 solid, 3 decal, 4 cut-out
        let kinds = [mesh_kind(0, 0, false), mesh_kind(2, 0, false), mesh_kind(0, 0, false), mesh_kind(0, 0, true), mesh_kind(1, 0, false)];
        assert_eq!(kinds, [0, 3, 0, 2, 1]);
        let mats = [2, 1, 0, 3, 2, 4, 0, 1];
        let o = draw_order(&mats, |m| kinds[m]);
        // solid by material (0, 0, 2, 2), then cut-out, decal, see-through;
        // meshes of one material keep their own order
        assert_eq!(o, vec![2, 6, 0, 4, 5, 3, 1, 7]);
    }
}
#[cfg(test)]
mod cull_tests {
    use super::MeshCull;
    use glam::{Mat4, Vec3};

    #[test]
    fn view_cull_keeps_what_is_in_front() {
        // camera at the origin looking down -Z, 90 degrees, reversed infinite depth
        let vp = Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1) * Mat4::look_at_rh(Vec3::ZERO, -Vec3::Z, Vec3::Y);
        let c = MeshCull::view(vp);
        assert!(c.sees(Vec3::new(0.0, 0.0, -50.0), 1.0));
        // behind the camera
        assert!(!c.sees(Vec3::new(0.0, 0.0, 50.0), 1.0));
        // well off to the side, and one just touching the edge
        assert!(!c.sees(Vec3::new(200.0, 0.0, -50.0), 1.0));
        assert!(c.sees(Vec3::new(52.0, 0.0, -50.0), 2.0));
        // a big sphere around the camera
        assert!(c.sees(Vec3::new(0.0, 0.0, 30.0), 40.0));
    }
}
