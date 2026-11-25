//! Sun and moon shadows for the 3D map: cascaded shadow maps.
//!
//! Four cascades (the timecycle's `dir_shadow_num_cascades`) split the
//! distance from the camera to the shadow range with the usual blend of
//! even and logarithmic spacing. Each cascade is an orthographic view from
//! the light over the bounding sphere of its slice of the view, snapped to
//! whole texels so shadows do not shimmer while the camera moves. The two
//! near cascades are drawn every frame, the two far ones on alternate
//! frames; a far cascade skips meshes smaller than about two of its texels.
use eframe::egui_wgpu::wgpu;
use glam::{Mat4, Vec3};

pub const CASCADES: usize = 4;
pub const RES: u32 = 2048;
pub const FMT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Distance (m) the shadows reach.
pub const RANGE: f32 = 900.0;
/// How much of the split spacing is logarithmic (rest even).
const LAMBDA: f32 = 0.8;
/// Room behind a cascade's sphere towards the light for casters (m).
const BACK: f32 = 1500.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Default)]
pub struct ShadowU {
    /// World to the cascade's clip space (reversed depth: 1 nearest the light).
    pub mats: [[[f32; 4]; 4]; CASCADES],
    /// Far distance of each cascade (m from the camera).
    pub splits: [f32; 4],
    /// World size of one texel of each cascade (m).
    pub texel: [f32; 4],
    /// Depth bias of each cascade (depth units).
    pub bias: [f32; 4],
    /// x 1 = shadows on, y filter radius in texels, z light size (softness), w unused.
    pub params: [f32; 4],
}

/// One cascade's view.
#[derive(Clone, Copy)]
pub struct Cascade {
    pub view_proj: Mat4,
    pub far: f32,
    pub texel: f32,
    pub bias: f32,
    /// Meshes smaller than this (bounding radius, m) are not drawn into it.
    pub min_radius: f32,
}

/// Split distances: from `near` to `range`.
pub fn splits(near: f32, range: f32) -> [f32; CASCADES] {
    let mut out = [0.0; CASCADES];
    for (i, o) in out.iter_mut().enumerate() {
        let f = (i + 1) as f32 / CASCADES as f32;
        let log = near * (range / near).powf(f);
        let lin = near + (range - near) * f;
        *o = LAMBDA * log + (1.0 - LAMBDA) * lin;
    }
    out
}

/// The cascades for a camera (eye, forward, vertical field of view,
/// aspect) and a light direction (towards the light).
pub fn cascades(eye: Vec3, fwd: Vec3, fov_y: f32, aspect: f32, light: Vec3, near: f32) -> [Cascade; CASCADES] {
    let sp = splits(near.max(0.3), RANGE);
    let l = light.normalize();
    let up = if l.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let fwd = fwd.normalize();
    let ty = (fov_y * 0.5).tan();
    let tx = ty * aspect;
    // distance from the axis to a corner of the view, per unit of depth
    let corner = (1.0 + tx * tx + ty * ty).sqrt();
    let mut out = [Cascade { view_proj: Mat4::IDENTITY, far: 0.0, texel: 1.0, bias: 0.0, min_radius: 0.0 }; CASCADES];
    let mut prev = near;
    for (i, &far) in sp.iter().enumerate() {
        // bounding sphere of the slice between prev and far: the centre on
        // the axis where the near and far corner distances are equal
        let (a, b) = (prev, far);
        let k2 = corner * corner - 1.0;
        let z = ((b * b - a * a) * (1.0 + k2) / (2.0 * (b - a))).clamp(a, b);
        let r = ((b - z).powi(2) + (b * b) * k2).sqrt().max(((z - a).powi(2) + a * a * k2).sqrt());
        let centre = eye + fwd * z;
        // light view, centre snapped to whole texels in the light's plane
        let view0 = Mat4::look_at_rh(Vec3::ZERO, -l, up);
        let texel = 2.0 * r / RES as f32;
        let c_ls = view0.transform_point3(centre);
        let snapped = Vec3::new((c_ls.x / texel).floor() * texel, (c_ls.y / texel).floor() * texel, c_ls.z);
        let centre_ws = view0.inverse().transform_point3(snapped);
        let eye_ls = centre_ws + l * (r + BACK);
        let view = Mat4::look_at_rh(eye_ls, centre_ws, up);
        let depth = 2.0 * r + BACK;
        // reversed depth: near plane (at the light) 1, far plane 0
        let proj = Mat4::orthographic_rh(-r, r, -r, r, depth, 0.0);
        out[i] = Cascade {
            view_proj: proj * view,
            far,
            texel,
            bias: 1.5 * texel / depth,
            min_radius: if i >= 2 { 2.0 * texel } else { 0.0 },
        };
        prev = far;
    }
    out
}

impl ShadowU {
    pub fn new(cs: &[Cascade; CASCADES], on: bool, softness: f32) -> Self {
        let mut u = ShadowU::default();
        for (i, c) in cs.iter().enumerate() {
            u.mats[i] = c.view_proj.to_cols_array_2d();
            u.splits[i] = c.far;
            u.texel[i] = c.texel;
            u.bias[i] = c.bias;
        }
        u.params = [on as u32 as f32, 1.5, softness, 0.0];
        u
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_grow_to_the_range() {
        let s = splits(0.5, RANGE);
        assert!(s.windows(2).all(|w| w[0] < w[1]));
        assert!((s[CASCADES - 1] - RANGE).abs() < 1e-3);
        assert!(s[0] < 20.0);
    }

    #[test]
    fn cascade_contains_its_slice() {
        let eye = Vec3::new(10.0, 50.0, -20.0);
        let fwd = Vec3::new(0.3, -0.2, 1.0).normalize();
        let light = Vec3::new(0.4, 0.8, 0.2);
        let cs = cascades(eye, fwd, 1.0, 16.0 / 9.0, light, 0.5);
        for c in &cs {
            // a point on the view axis at the cascade's far distance is inside
            let p = c.view_proj * (eye + fwd * (c.far * 0.99)).extend(1.0);
            assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0, "{p:?}");
            assert!(p.z > 0.0 && p.z < 1.0, "{p:?}");
        }
        // a point between the eye and the light is nearer the light (larger depth)
        let c = &cs[0];
        let a = c.view_proj * (eye + fwd * 2.0).extend(1.0);
        let b = c.view_proj * (eye + fwd * 2.0 + light.normalize() * 3.0).extend(1.0);
        assert!(b.z > a.z);
    }
}

/// The cascades on the GPU: one depth array (a 1-texel stand-in for the
/// model viewer), its views, the comparison sampler, the uniform the mesh
/// shaders read and a frame uniform per cascade for drawing into it.
pub struct ShadowGpu {
    pub layers: Vec<wgpu::TextureView>,
    pub array: wgpu::TextureView,
    /// Bound in place of `array` while drawing the cascades.
    pub stand_in: wgpu::TextureView,
    pub cmp: wgpu::Sampler,
    pub ub: wgpu::Buffer,
    pub frame_ubs: Vec<wgpu::Buffer>,
    /// What each cascade was last drawn with.
    pub drawn: [Option<Cascade>; CASCADES],
    pub frame: u64,
}

fn depth_array(device: &wgpu::Device, res: u32, label: &str) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: res, height: res, depth_or_array_layers: CASCADES as u32 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FMT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

impl ShadowGpu {
    pub fn new(device: &wgpu::Device, on: bool, frame_size: u64) -> Self {
        let tex = depth_array(device, if on { RES } else { 1 }, "shadow cascades");
        let stand = depth_array(device, 1, "shadow stand-in");
        let arr = |t: &wgpu::Texture| {
            t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let layers = (0..CASCADES as u32)
            .map(|i| {
                tex.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: i,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let cmp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow compare"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            // reversed depth: lit where the point is at least as near the light as the map
            compare: Some(wgpu::CompareFunction::GreaterEqual),
            ..Default::default()
        });
        let ub = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow ub"),
            size: std::mem::size_of::<ShadowU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_ubs = (0..CASCADES)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("cascade frame ub"),
                    size: frame_size,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        ShadowGpu {
            layers,
            array: arr(&tex),
            stand_in: arr(&stand),
            cmp,
            ub,
            frame_ubs,
            drawn: [None; CASCADES],
            frame: 0,
        }
    }

    /// Which cascades to draw this frame: the near two always, the far two
    /// in turn (and any not drawn yet).
    pub fn due(&self) -> [bool; CASCADES] {
        let mut d = [true; CASCADES];
        let odd = self.frame % 2 == 1;
        d[2] = !odd || self.drawn[2].is_none();
        d[3] = odd || self.drawn[3].is_none();
        d
    }
}
