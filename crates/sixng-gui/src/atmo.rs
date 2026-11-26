//! The 3D map's sky, light and frame from the game's timecycle.
//!
//! The timecycle describes a physical atmosphere (`sky_new_*`): planet
//! radius, atmosphere top, Rayleigh and Mie scattering with scale heights,
//! Mie absorption and phase eccentricity, and a two-piece ozone layer. It
//! is rendered with look-up tables (atmo.wgsl): transmittance, multiple
//! scattering, the sky seen from the camera and a froxel volume of aerial
//! perspective. Surfaces are lit in physical units (lux, cd/m2): the sun's
//! illuminance (`sky_scattering_sun_illuminance`) through the air, the sky's
//! irradiance from the sky-view table, and the timecycle's height fog. The
//! frame is HDR; an exposure meter (histogram between the timecycle's low
//! and high percentiles, its key value and adaptation rate) sets the
//! exposure, then bloom (`postfx_intensity_bloom`), contrast and
//! saturation and a filmic curve make the shown image.
//!
//! Own choices, not decoded: the scale of the colour values to scattering
//! coefficients (so the game's defaults give the usual Earth atmosphere),
//! the moon's illuminance, star and air-glow brightness, the darker
//! exposure of night scenes and the filmic curve.
use eframe::egui_wgpu::wgpu;
use glam::{Mat4, Vec3};

pub const HDR_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DISPLAY_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

const TRANS: [u32; 2] = [256, 64];
const MS: u32 = 32;
const SKY: [u32; 2] = [192, 108];
const AP: u32 = 32;
const BLOOM_LEVELS: usize = 6;

/// Scattering per km of a colour value 1 (Rayleigh, Mie scattering, Mie
/// absorption, ozone): chosen so the game's default colours give the usual
/// Earth atmosphere (Rayleigh 5.8, 13.6, 33.1 e-3; Mie 4.0 e-3 and 0.44 e-3;
/// ozone 0.65, 1.88, 0.085 e-3 per km).
const K_RAY: f32 = 0.0331 / 0.9134;
const K_MIE_S: f32 = 0.003996 / 0.5767;
const K_MIE_A: f32 = 0.000444 / 0.5767;
const K_OZO: f32 = 0.001881 / 0.9434;
/// Moon illuminance (lux) at `sky_scattering_moon_illuminance` 1.
const MOON_LUX: f32 = 0.25;
/// Star and air-glow luminance (cd/m2) at a timecycle intensity of 1.
const K_STAR: f32 = 100.0;
const K_GLOW: f32 = 0.002;
/// Share of the direct light that full cloud cover takes away besides the
/// drawn clouds' shadows (the haze under and between them).
const CLOUD_DIM: f32 = 0.35;
/// Cumulus extinction per m at full density (our choice: a 1 km deep core
/// is opaque, its thin edges are not).
const CLOUD_SIGMA: f32 = 0.004;
/// Feature size of the cloud cover (m).
const CLOUD_SCALE: f32 = 2500.0;
/// Wind speed that moves the clouds at a wind velocity scale of 1 (m/s).
const CLOUD_WIND: f32 = 12.0;
/// Depth of the aerial-perspective volume (km) near the ground; from
/// higher up it reaches the horizon (`ap_depth`).
const AP_DEPTH: f32 = 40.0;

/// Depth of the aerial-perspective volume (km) for a camera `h_km` above
/// the ground of a planet of radius `rb`: at least `AP_DEPTH`, else a
/// little past the distance to the horizon (at most 400 km).
fn ap_depth(rb: f32, h_km: f32) -> f32 {
    let horizon = (2.0 * rb * h_km.max(0.0) + h_km * h_km).sqrt();
    (horizon * 1.2).clamp(AP_DEPTH, 400.0)
}
/// Ground albedo for the sky's light bounced off the ground.
const GROUND_ALBEDO: f32 = 0.15;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Default, Debug)]
pub struct SceneU {
    pub ray: [f32; 4],
    pub mie_s: [f32; 4],
    pub mie_a: [f32; 4],
    pub ozo: [f32; 4],
    pub ozo_lin: [f32; 4],
    pub radii: [f32; 4],
    pub sun: [f32; 4],
    pub sun_sky: [f32; 4],
    pub sun_dir_e: [f32; 4],
    pub moon: [f32; 4],
    pub moon_sky: [f32; 4],
    pub moon_dir_e: [f32; 4],
    pub intens: [f32; 4],
    pub fog0: [f32; 4],
    pub fog1: [f32; 4],
    pub air_glow: [f32; 4],
    pub expo: [f32; 4],
    pub expo2: [f32; 4],
    pub post: [f32; 4],
    pub post2: [f32; 4],
    pub flags: [u32; 4],
    pub inv_vp: [[f32; 4]; 4],
    pub cam: [f32; 4],
    /// Water (`water_*` timecycle values, see `Water`): extinction per m
    /// (rgb), w peak wavelength (m).
    pub water0: [f32; 4],
    /// Scattering per m (rgb), w slope variance of the sea surface.
    pub water1: [f32; 4],
    /// Phase (rgb, the game's underwater fog; not used yet), w wave speed scale.
    pub water2: [f32; 4],
    /// Body colour tint times brightness (rgb), w added sun-glint roughness.
    pub water3: [f32; 4],
    /// Fresnel rescale, its min and max; w time (s) for the waves.
    pub water4: [f32; 4],
    /// Clouds: cover (0 none to 1 overcast), base and top of the cumulus
    /// layer (world y, m), extinction per m at full density.
    pub cloud0: [f32; 4],
    /// Clouds: drift (m/s, map x and world z), w feature size (m).
    pub cloud1: [f32; 4],
}

/// Scale from the timecycle's `water_clip_*` scattering and extinction
/// values to coefficients per metre (our choice: open-sea red extinction
/// 0.34 / m, blue 0.2 / m).
const K_WATER: f32 = 0.03;

/// Waves of a sea surface for a Beaufort number and a fetch (km): the
/// peak wavelength (m) and the slope variance.
///
/// Wind speed from the Beaufort scale, `U = 0.836 B^1.5` m/s. The peak
/// frequency is the fetch-limited JONSWAP one, `22 (g² / (U F))^⅓`, but
/// no lower than a fully developed sea's (Pierson-Moskowitz, `0.877 g / U`).
/// The slope variance is the Cox-Munk clean-surface fit, `0.003 + 0.00512 U`.
pub fn sea_waves(beaufort: f32, fetch_km: f32) -> (f32, f32) {
    const G: f32 = 9.81;
    let u = (0.836 * beaufort.max(0.0).powf(1.5)).max(0.5);
    let f = fetch_km.max(1.0) * 1000.0;
    let w_fetch = 22.0 * (G * G / (u * f)).powf(1.0 / 3.0);
    let w_pm = 0.877 * G / u;
    let w = w_fetch.max(w_pm);
    let lambda = (2.0 * std::f32::consts::PI * G / (w * w)).clamp(2.0, 400.0);
    (lambda, 0.003 + 0.00512 * u)
}

/// The sky and light of one weather and hour, from the timecycle.
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    pub u: SceneU,
    /// The frame's directional light (sun or moon), y up.
    pub key_dir: Vec3,
    pub key_is_moon: bool,
}

fn v4(a: [f32; 3], w: f32) -> [f32; 4] {
    [a[0], a[1], a[2], w]
}

fn mul3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

fn scale3(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

impl Sky {
    /// From the timecycle table of the game (`sixng_core::daylight`).
    pub fn from_timecycle(t: &sixng_core::daylight::Table, w: &sixng_core::daylight::Weather, hour: f32) -> Sky {
        let g = |k: &str, d: f32| t.get(w, k, hour, d);
        let rgb = |k: &str| t.rgb(w, k, hour);
        let sd = sixng_core::daylight::sun_dir(hour);
        let sun = Vec3::from(sd).normalize();
        let moon = -sun;
        let cloud = w.cloud_cover.clamp(0.0, 1.0);
        let dim = 1.0 - CLOUD_DIM * cloud;
        let mut u = SceneU::default();
        u.ray = v4(scale3(rgb("sky_new_ray_scattering_col"), K_RAY * g("sky_new_ray_scattering_intensity", 1.0)), g("sky_new_ray_density_scale", 8.0));
        let mie_k = g("sky_new_mie_scattering_intensity", 1.0);
        u.mie_s = v4(scale3(rgb("sky_new_mie_scattering_col"), K_MIE_S * mie_k), g("sky_new_mie_density_scale", 1.2));
        u.mie_a = v4(
            scale3(rgb("sky_new_mie_absorption_col"), K_MIE_A * g("sky_new_mie_absorption_intensity", 1.0)),
            g("sky_new_mie_phase_eccentricity", 0.8).clamp(0.0, 0.98),
        );
        u.ozo = v4(scale3(rgb("sky_new_ozo_extinction_col"), K_OZO * g("sky_new_ozo_extinction_intensity", 1.0)), g("sky_new_ozo_0_width", 25.0));
        u.ozo_lin = [
            g("sky_new_ozo_0_linear", 1.0 / 15.0),
            g("sky_new_ozo_0_constant", -2.0 / 3.0),
            g("sky_new_ozo_1_linear", -1.0 / 15.0),
            g("sky_new_ozo_1_constant", 8.0 / 3.0),
        ];
        let rb = g("sky_new_planet_radius", 6360.0);
        u.radii = [rb, rb + g("sky_new_karman_line", 100.0), 0.0, AP_DEPTH];
        let sun_lux = g("sky_scattering_sun_illuminance", 100000.0);
        let sun_half = 0.004650 * g("sky_sun_disc_size", 2.0) * 0.5;
        u.sun = v4(sun.to_array(), sun_half);
        let sun_col = rgb("sky_scattering_sun_col");
        u.sun_sky = v4(scale3(sun_col, sun_lux * g("sky_scattering_sun_intensity_multiplier", 1.0)), 0.0);
        let dir_col = rgb("light_dir_col");
        u.sun_dir_e = v4(scale3(mul3(dir_col, sun_col), sun_lux * g("light_dir_sun_mult", 1.0) * dim), 0.0);
        let moon_lux = MOON_LUX * g("sky_scattering_moon_illuminance", 1.0);
        u.moon = v4(moon.to_array(), 0.0045 * g("sky_moon_disc_size", 2.0) * 0.5);
        let stars = K_STAR * g("sky_stars_iten", 0.01) * g("sky_stars_sparse_intensity_mult", 1.0) * (1.0 - cloud);
        u.moon_sky = v4(
            scale3(rgb("sky_scattering_moon_col"), moon_lux * g("sky_scattering_moon_intensity_multiplier", 1.0)),
            stars,
        );
        u.moon_dir_e = v4(scale3(mul3(dir_col, rgb("sky_moon_col")), moon_lux * g("light_dir_moon_mult", 1.0) * dim), 1.0);
        let amb = g("indirect_light_sky_multiplier", 1.0) * g("ambient_sky_intensity", 0.2) / 0.2;
        u.intens = [
            g("sky_new_intensity_sky", 1.0),
            g("sky_new_intensity_aerial", 1.0),
            g("sky_new_intensity_multiscattering", 1.0),
            amb.clamp(0.0, 20.0),
        ];
        u.fog0 = [
            g("local_atmosphere_fog_density", 0.0).max(0.0),
            g("local_atmosphere_fog_falloff", 50.0),
            g("local_atmosphere_fog_height", 0.0),
            g("local_atmosphere_haze_phase_anisotropy", 0.6).clamp(-0.95, 0.95),
        ];
        let base = g("local_atmosphere_cloud_height", 200.0).max(0.0);
        let top = g("local_atmosphere_cloud_height_top", 1500.0).max(base + 50.0);
        u.cloud0 = [cloud, base, top, CLOUD_SIGMA];
        let wind = CLOUD_WIND * g("local_atmosphere_cloud_wind_velocity_scale", 0.5);
        u.cloud1 = [wind * 0.8, wind * 0.6, 0.0, CLOUD_SCALE];
        u.fog1 = [
            g("local_atmosphere_fog_density_layer_1", 0.0).max(0.0),
            g("local_atmosphere_fog_falloff_layer_1", 50.0),
            g("local_atmosphere_fog_height_layer_1", 0.0),
            0.0,
        ];
        let glow = rgb("sky_air_glow_color");
        let glow_base = rgb("sky_air_glow_base_color");
        let gi = g("sky_air_glow_intensity", 0.0);
        let gb = g("sky_air_glow_base_intensity", 0.0);
        u.air_glow = v4(
            [0, 1, 2].map(|c| K_GLOW * (glow[c] * gi + glow_base[c] * gb)),
            GROUND_ALBEDO,
        );
        u.expo = [g("postfx_exposure", 0.0), -6.0, 17.0, 2.0 * g("postfx_exposure_adaption_step_mult", 0.65)];
        u.expo2 = [
            g("postfx_exposure_lowlights_percent", 0.3).clamp(0.0, 0.45),
            g("postfx_exposure_highlights_percent", 0.3).clamp(0.0, 0.45),
            g("postfx_exposure_key_value", 0.18),
            0.016,
        ];
        u.post = [
            0.25 * g("postfx_intensity_bloom", 0.15),
            g("postfx_contrast_amount", 1.0),
            g("postfx_contrast_pivot", 0.18),
            g("postfx_desaturation", 1.0).clamp(0.0, 2.0),
        ];
        u.post2 = [
            0.0,
            (g("sky_scattering_sun_disk_intensity_multiplier", 256.0) / 256.0).clamp(0.01, 1.0),
            g("ambient_bounce_intensity", 0.5).clamp(0.0, 4.0),
            0.0,
        ];
        let (lambda, slope) = sea_waves(g("water_ocean_beaufort_scale", 4.0), g("water_ocean_fetch", 400.0));
        let ext = g("water_clip_extinction_scale", 1.0) * K_WATER;
        let sca = g("water_clip_scattering_scale", 1.0) * K_WATER;
        u.water0 = v4(scale3(rgb("water_clip_extinction"), ext), lambda);
        u.water1 = v4(scale3(rgb("water_clip_scattering"), sca), slope);
        u.water2 = v4(rgb("water_clip_phase"), g("water_ripple_speed_scale", 1.0).clamp(0.0, 4.0));
        u.water3 = v4(
            scale3(rgb("water_clip_fog_color_tint"), g("water_clip_fog_brightness", 1.0)),
            g("water_direct_light_roughness_mod", 0.0).clamp(0.0, 1.0),
        );
        u.water4 = [
            g("water_fresnel_rescale", 0.0),
            g("water_fresnel_rescale_min", -0.2),
            g("water_fresnel_rescale_max", 0.1),
            0.0,
        ];
        u.flags = [1, 0, 0, 0];
        // the light that shades surfaces: the sun until it is a little under
        // the horizon (the twilight sky still lights), then the moon
        let key_is_moon = sun.y < -0.02;
        Sky {
            u,
            key_dir: if key_is_moon { moon } else { sun },
            key_is_moon,
        }
    }

    /// Without the timecycle (no index): the game's Sunny values, written out.
    pub fn fallback(hour: f32) -> Sky {
        let sun = Vec3::from(sixng_core::daylight::sun_dir(hour)).normalize();
        let day = (sun.y * 4.0).clamp(0.0, 1.0);
        let mut u = SceneU::default();
        u.ray = v4(scale3([0.1601, 0.3741, 0.9134], K_RAY), 8.0);
        u.mie_s = v4(scale3([0.5767; 3], K_MIE_S), 1.1973);
        u.mie_a = v4(scale3([0.5767; 3], K_MIE_A), 0.8);
        u.ozo = v4(scale3([0.3262, 0.9434, 0.0427], K_OZO), 25.0);
        u.ozo_lin = [0.0666, -0.6646, -0.0666, 2.6641];
        u.radii = [6360.0, 6460.0, 0.0, AP_DEPTH];
        u.sun = v4(sun.to_array(), 0.00465);
        u.sun_sky = v4([99968.0 * day; 3], 0.0);
        u.sun_dir_e = v4([99968.0; 3], 0.0);
        u.moon = v4((-sun).to_array(), 0.0045);
        u.moon_sky = v4([MOON_LUX * 0.02; 3], K_STAR * 0.01);
        u.moon_dir_e = v4([MOON_LUX * 2.5; 3], 1.0);
        u.intens = [1.0, 1.0, 1.0, 1.0 + 14.0 * (1.0 - day)];
        u.fog0 = [0.0, 50.0, 0.0, 0.65];
        u.fog1 = [0.0, 50.0, 0.0, 0.0];
        u.air_glow = v4([K_GLOW * 0.2 * 0.184, K_GLOW * 0.2, K_GLOW * 0.2 * 0.494], GROUND_ALBEDO);
        u.expo = [0.0, -6.0, 17.0, 1.3];
        u.expo2 = [0.3, 0.3, 0.1797, 0.016];
        u.post = [0.25 * 0.1498, 1.0, 0.1797, 1.0];
        u.post2 = [0.0, 1.0, 0.5, 0.0];
        // the Sunny water
        let (lambda, slope) = sea_waves(4.0, 400.0);
        u.water0 = v4(scale3([11.1875, 6.5977, 6.5977], K_WATER), lambda);
        u.water1 = v4(scale3([0.06, 0.12, 0.3997], K_WATER), slope);
        u.water2 = v4([0.7188, 0.8184, 0.9185], 1.0);
        u.water3 = v4([1.0; 3], 0.0);
        u.water4 = [0.0, -0.2, 0.1, 0.0];
        u.flags = [1, 0, 0, 0];
        let key_is_moon = sun.y < -0.02;
        Sky {
            u,
            key_dir: if key_is_moon { -sun } else { sun },
            key_is_moon,
        }
    }

    /// Values that change the transmittance and multiple-scattering tables.
    fn medium_key(&self) -> [f32; 23] {
        let u = &self.u;
        let mut k = [0.0; 23];
        let parts = [u.ray, u.mie_s, u.mie_a, u.ozo, u.ozo_lin];
        for (i, p) in parts.iter().enumerate() {
            k[i * 4..i * 4 + 4].copy_from_slice(p);
        }
        k[20] = u.radii[0];
        k[21] = u.radii[1];
        k[22] = u.air_glow[3];
        k
    }
}

/// What the mesh shaders bind of the scene (group 0, bindings 6 to 11).
#[derive(Clone)]
pub struct SceneBinds {
    pub scene_ub: wgpu::Buffer,
    pub trans: wgpu::TextureView,
    pub sky: wgpu::TextureView,
    pub ap: wgpu::TextureView,
    pub state: wgpu::Buffer,
    pub sampler: wgpu::Sampler,
    /// Sun and moon shadows (shadow.rs): the cascades' uniform, depth array
    /// and comparison sampler.
    pub shadow_ub: wgpu::Buffer,
    pub shadow: wgpu::TextureView,
    pub shadow_cmp: wgpu::Sampler,
    /// The placed lights near the camera and their grid (lights.rs).
    pub lights: wgpu::Buffer,
    pub light_grid: wgpu::Buffer,
}

/// Bind group layout entries 6 to 14, 22 and 23 of the mesh shaders' group 0.
pub fn frame_layout_entries() -> Vec<wgpu::BindGroupLayoutEntry> {
    let vis = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
    let tex = |b: u32, dim: wgpu::TextureViewDimension| wgpu::BindGroupLayoutEntry {
        binding: b,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: dim,
            multisampled: false,
        },
        count: None,
    };
    vec![
        wgpu::BindGroupLayoutEntry {
            binding: 6,
            visibility: vis,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        tex(7, wgpu::TextureViewDimension::D2),
        tex(8, wgpu::TextureViewDimension::D2),
        tex(9, wgpu::TextureViewDimension::D3),
        wgpu::BindGroupLayoutEntry {
            binding: 10,
            visibility: vis,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 11,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 12,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 13,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 14,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
            count: None,
        },
        storage_fs(22),
        storage_fs(23),
    ]
}

fn storage_fs(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

impl SceneBinds {
    pub fn entries(&self) -> [wgpu::BindGroupEntry<'_>; 11] {
        [
            wgpu::BindGroupEntry { binding: 6, resource: self.scene_ub.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&self.trans) },
            wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(&self.sky) },
            wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(&self.ap) },
            wgpu::BindGroupEntry { binding: 10, resource: self.state.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 11, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            wgpu::BindGroupEntry { binding: 12, resource: self.shadow_ub.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 13, resource: wgpu::BindingResource::TextureView(&self.shadow) },
            wgpu::BindGroupEntry { binding: 14, resource: wgpu::BindingResource::Sampler(&self.shadow_cmp) },
            wgpu::BindGroupEntry { binding: 22, resource: self.lights.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 23, resource: self.light_grid.as_entire_binding() },
        ]
    }
}

struct Post {
    size: [u32; 2],
    /// Bloom chain: downsampled levels (half size first) and the upsampled sums.
    down: Vec<wgpu::TextureView>,
    up: Vec<wgpu::TextureView>,
    /// Per pass: group 1 (source, added).
    bg_down: Vec<wgpu::BindGroup>,
    bg_up: Vec<wgpu::BindGroup>,
    bg_tonemap: wgpu::BindGroup,
    bg_hist: wgpu::BindGroup,
}

/// The GPU side: tables, state, passes.
pub struct Atmosphere {
    pub binds: SceneBinds,
    _ms: wgpu::TextureView,
    hist: wgpu::Buffer,
    bgl_post: wgpu::BindGroupLayout,
    bgl_hist: wgpu::BindGroupLayout,
    g0_trans: wgpu::BindGroup,
    g0_ms: wgpu::BindGroup,
    g0_sky: wgpu::BindGroup,
    g0_all: wgpu::BindGroup,
    g1_trans: wgpu::BindGroup,
    g1_ms: wgpu::BindGroup,
    g1_sky: wgpu::BindGroup,
    g1_ap: wgpu::BindGroup,
    cs_trans: wgpu::ComputePipeline,
    cs_ms: wgpu::ComputePipeline,
    cs_sky: wgpu::ComputePipeline,
    cs_ap: wgpu::ComputePipeline,
    cs_ambient: wgpu::ComputePipeline,
    cs_hist: wgpu::ComputePipeline,
    cs_expo: wgpu::ComputePipeline,
    fs_down: wgpu::RenderPipeline,
    fs_up: wgpu::RenderPipeline,
    fs_tonemap: wgpu::RenderPipeline,
    black: wgpu::TextureView,
    post: Option<Post>,
    medium_key: Option<[f32; 23]>,
    frame: u32,
    /// Frames left that snap the exposure to the metered value.
    pub reset: u32,
    /// Start of the scene's clock (the waves).
    t0: std::time::Instant,
}

fn storage_tex_entry(b: u32, dim: wgpu::TextureViewDimension) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: b,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: HDR_FMT,
            view_dimension: dim,
        },
        count: None,
    }
}

fn sampled_entry(b: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: b,
        visibility: vis,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn tex(device: &wgpu::Device, label: &str, w: u32, h: u32, d: u32, dim: wgpu::TextureDimension, usage: wgpu::TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: d },
        mip_level_count: 1,
        sample_count: 1,
        dimension: dim,
        format: HDR_FMT,
        usage,
        view_formats: &[],
    })
}

impl Atmosphere {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, shadows: &crate::shadow::ShadowGpu) -> Self {
        let src = format!("{}\n{}", include_str!("atmo_common.wgsl"), include_str!("atmo.wgsl"));
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("atmosphere"),
            source: wgpu::ShaderSource::Wgsl(src.into()),
        });
        let lut = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        let d2 = wgpu::TextureDimension::D2;
        let trans = tex(device, "atmo transmittance", TRANS[0], TRANS[1], 1, d2, lut).create_view(&Default::default());
        let ms = tex(device, "atmo multiple scattering", MS, MS, 1, d2, lut).create_view(&Default::default());
        let sky = tex(device, "atmo sky view", SKY[0], SKY[1], 1, d2, lut).create_view(&Default::default());
        let ap = tex(device, "atmo aerial perspective", AP, AP, AP, wgpu::TextureDimension::D3, lut).create_view(&Default::default());
        let black = tex(device, "atmo black", 1, 1, 1, d2, wgpu::TextureUsages::TEXTURE_BINDING).create_view(&Default::default());
        let scene_ub = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene ub"),
            size: std::mem::size_of::<SceneU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let state = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene state"),
            size: 16 * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // a daylight exposure to start from (EV100 12), white ambient off
        let mut init = [0f32; 64];
        init[0] = 0.1797 / (2f32.powf(12.0) * 0.125);
        init[1] = 12.0;
        init[2] = 12.0;
        for k in 7..9 {
            init[k * 4..k * 4 + 3].copy_from_slice(&[1.0; 3]);
        }
        queue.write_buffer(&state, 0, bytemuck::cast_slice(&init));
        let hist = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("exposure histogram"),
            size: 128 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atmo clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let cf = wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT;
        let bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atmo scene"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: cf,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: cf,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                sampled_entry(2, cf),
                sampled_entry(3, cf),
                sampled_entry(4, cf),
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: cf,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let g0 = |t: &wgpu::TextureView, m: &wgpu::TextureView, s: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("atmo g0"),
                layout: &bgl0,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: scene_ub.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(t) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(m) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(s) },
                    wgpu::BindGroupEntry { binding: 5, resource: state.as_entire_binding() },
                ],
            })
        };
        // a table is never read while it is written: each pass binds a
        // stand-in for the tables it writes or that are not ready yet
        let g0_trans = g0(&black, &black, &black);
        let g0_ms = g0(&trans, &black, &black);
        let g0_sky = g0(&trans, &ms, &black);
        let g0_all = g0(&trans, &ms, &sky);
        let one = |b: u32, dim: wgpu::TextureViewDimension, label: &str| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[storage_tex_entry(b, dim)],
            })
        };
        let bgl_trans = one(0, wgpu::TextureViewDimension::D2, "atmo out trans");
        let bgl_ms = one(1, wgpu::TextureViewDimension::D2, "atmo out ms");
        let bgl_sky = one(2, wgpu::TextureViewDimension::D2, "atmo out sky");
        let bgl_ap = one(3, wgpu::TextureViewDimension::D3, "atmo out ap");
        let bgl_hist = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atmo hist"),
            entries: &[
                sampled_entry(4, wgpu::ShaderStages::COMPUTE),
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bgl_post = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atmo post"),
            entries: &[sampled_entry(6, wgpu::ShaderStages::FRAGMENT), sampled_entry(7, wgpu::ShaderStages::FRAGMENT)],
        });
        let g1 = |l: &wgpu::BindGroupLayout, b: u32, v: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("atmo out"),
                layout: l,
                entries: &[wgpu::BindGroupEntry { binding: b, resource: wgpu::BindingResource::TextureView(v) }],
            })
        };
        let g1_trans = g1(&bgl_trans, 0, &trans);
        let g1_ms = g1(&bgl_ms, 1, &ms);
        let g1_sky = g1(&bgl_sky, 2, &sky);
        let g1_ap = g1(&bgl_ap, 3, &ap);
        let layout = |l1: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("atmo"),
                bind_group_layouts: &[Some(&bgl0), Some(l1)],
                immediate_size: 0,
            })
        };
        let cs = |l1: &wgpu::BindGroupLayout, entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout(l1)),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let cs_trans = cs(&bgl_trans, "cs_transmittance");
        let cs_ms = cs(&bgl_ms, "cs_multiscatter");
        let cs_sky = cs(&bgl_sky, "cs_skyview");
        let cs_ap = cs(&bgl_ap, "cs_aerial");
        let cs_ambient = cs(&bgl_ap, "cs_ambient");
        let cs_hist = cs(&bgl_hist, "cs_histogram");
        let cs_expo = cs(&bgl_hist, "cs_exposure");
        let post_layout = layout(&bgl_post);
        let rp = |entry: &str, fmt: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&post_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: fmt,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let fs_down = rp("fs_down", HDR_FMT);
        let fs_up = rp("fs_up", HDR_FMT);
        let fs_tonemap = rp("fs_tonemap", DISPLAY_FMT);
        Atmosphere {
            t0: std::time::Instant::now(),
            binds: SceneBinds {
                scene_ub,
                trans,
                sky,
                ap,
                state,
                sampler,
                shadow_ub: shadows.ub.clone(),
                shadow: shadows.array.clone(),
                shadow_cmp: shadows.cmp.clone(),
                // zeroed: a grid of 0 cells, no lights
                lights: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("placed lights"),
                    size: (crate::lights::MAX_LIGHTS * std::mem::size_of::<crate::lights::GpuLight>()) as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                light_grid: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("light grid"),
                    size: (crate::lights::GRID_WORDS * 4) as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            },
            _ms: ms,
            hist,
            bgl_post,
            bgl_hist,
            g0_trans,
            g0_ms,
            g0_sky,
            g0_all,
            g1_trans,
            g1_ms,
            g1_sky,
            g1_ap,
            cs_trans,
            cs_ms,
            cs_sky,
            cs_ap,
            cs_ambient,
            cs_hist,
            cs_expo,
            fs_down,
            fs_up,
            fs_tonemap,
            black,
            post: None,
            medium_key: None,
            frame: 0,
            reset: 3,
        }
    }

    /// Write the frame's scene values; `None` keeps the model viewer's own light.
    /// Uploads the placed lights near the camera (from `lights::pack`); an
    /// empty grid turns them off.
    pub fn set_lights(&self, queue: &wgpu::Queue, lights: &[crate::lights::GpuLight], grid: &[u32]) {
        if !lights.is_empty() {
            queue.write_buffer(&self.binds.lights, 0, bytemuck::cast_slice(&lights[..lights.len().min(crate::lights::MAX_LIGHTS)]));
        }
        let off = [0u32; crate::lights::HEADER];
        let grid = if grid.len() < crate::lights::HEADER { &off[..] } else { &grid[..grid.len().min(crate::lights::GRID_WORDS)] };
        queue.write_buffer(&self.binds.light_grid, 0, bytemuck::cast_slice(grid));
    }

    pub fn update(&mut self, queue: &wgpu::Queue, sky: Option<&Sky>, view_proj: Mat4, cam: Vec3, size: [u32; 2], dt: f32) {
        let Some(sky) = sky else {
            queue.write_buffer(&self.binds.scene_ub, 0, bytemuck::bytes_of(&SceneU::default()));
            return;
        };
        let mut u = sky.u;
        u.radii[2] = (cam.y * 0.001).max(0.0);
        u.water4[3] = self.t0.elapsed().as_secs_f32();
        u.radii[3] = ap_depth(u.radii[0], u.radii[2]);
        u.expo2[3] = dt.clamp(0.0, 0.25);
        u.post2[0] = if self.reset > 0 { 1.0 } else { 0.0 };
        u.flags = [1, self.frame, size[0], size[1]];
        u.inv_vp = view_proj.inverse().to_cols_array_2d();
        u.cam = cam.extend(1.0).to_array();
        queue.write_buffer(&self.binds.scene_ub, 0, bytemuck::bytes_of(&u));
        self.frame = self.frame.wrapping_add(1);
    }

    /// Before the main pass: the tables (transmittance and multiple
    /// scattering only when the medium changed), sky view, aerial
    /// perspective and ambient.
    pub fn compute(&mut self, enc: &mut wgpu::CommandEncoder, sky: &Sky) {
        let key = sky.medium_key();
        let medium = self.medium_key != Some(key);
        self.medium_key = Some(key);
        let mut p = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("atmosphere"), timestamp_writes: None });
        if medium {
            p.set_pipeline(&self.cs_trans);
            p.set_bind_group(0, &self.g0_trans, &[]);
            p.set_bind_group(1, &self.g1_trans, &[]);
            p.dispatch_workgroups(TRANS[0].div_ceil(8), TRANS[1].div_ceil(8), 1);
            p.set_pipeline(&self.cs_ms);
            p.set_bind_group(0, &self.g0_ms, &[]);
            p.set_bind_group(1, &self.g1_ms, &[]);
            p.dispatch_workgroups(MS.div_ceil(8), MS.div_ceil(8), 1);
        }
        p.set_pipeline(&self.cs_sky);
        p.set_bind_group(0, &self.g0_sky, &[]);
        p.set_bind_group(1, &self.g1_sky, &[]);
        p.dispatch_workgroups(SKY[0].div_ceil(8), SKY[1].div_ceil(8), 1);
        p.set_pipeline(&self.cs_ap);
        p.set_bind_group(1, &self.g1_ap, &[]);
        p.dispatch_workgroups(AP / 4, AP / 4, AP / 4);
        // ambient reads the finished sky view (its pipeline layout takes
        // the aerial-perspective group, which it does not use)
        p.set_pipeline(&self.cs_ambient);
        p.set_bind_group(0, &self.g0_all, &[]);
        p.dispatch_workgroups(1, 1, 1);
    }

    fn ensure_post(&mut self, device: &wgpu::Device, hdr: &wgpu::TextureView, size: [u32; 2]) {
        if self.post.as_ref().map(|p| p.size == size).unwrap_or(false) {
            return;
        }
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let mut down = Vec::new();
        let mut up = Vec::new();
        let (mut w, mut h) = (size[0], size[1]);
        for _ in 0..BLOOM_LEVELS {
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            down.push(tex(device, "bloom down", w, h, 1, wgpu::TextureDimension::D2, usage).create_view(&Default::default()));
            up.push(tex(device, "bloom up", w, h, 1, wgpu::TextureDimension::D2, usage).create_view(&Default::default()));
        }
        let pair = |a: &wgpu::TextureView, b: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom"),
                layout: &self.bgl_post,
                entries: &[
                    wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(a) },
                    wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(b) },
                ],
            })
        };
        let mut bg_down = Vec::new();
        for i in 0..BLOOM_LEVELS {
            let src = if i == 0 { hdr } else { &down[i - 1] };
            bg_down.push(pair(src, &self.black));
        }
        // up[i] = tent(up[i + 1] or down[last]) + down[i]
        let mut bg_up = Vec::new();
        for i in 0..BLOOM_LEVELS - 1 {
            let src = if i == BLOOM_LEVELS - 2 { &down[BLOOM_LEVELS - 1] } else { &up[i + 1] };
            bg_up.push(pair(src, &down[i]));
        }
        let bg_tonemap = pair(hdr, &up[0]);
        let bg_hist = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("exposure"),
            layout: &self.bgl_hist,
            entries: &[
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(hdr) },
                wgpu::BindGroupEntry { binding: 5, resource: self.hist.as_entire_binding() },
            ],
        });
        self.post = Some(Post { size, down, up, bg_down, bg_up, bg_tonemap, bg_hist });
    }

    /// After the main pass: meter the exposure for the next frame, bloom,
    /// and tone map `hdr` into `out` (the shown, sRGB image).
    pub fn post(&mut self, device: &wgpu::Device, enc: &mut wgpu::CommandEncoder, hdr: &wgpu::TextureView, out: &wgpu::TextureView, size: [u32; 2]) {
        self.ensure_post(device, hdr, size);
        let post = self.post.as_ref().unwrap();
        {
            let mut p = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("exposure"), timestamp_writes: None });
            p.set_pipeline(&self.cs_hist);
            p.set_bind_group(0, &self.g0_all, &[]);
            p.set_bind_group(1, &post.bg_hist, &[]);
            p.dispatch_workgroups(size[0].div_ceil(32), size[1].div_ceil(32), 1);
            p.set_pipeline(&self.cs_expo);
            p.dispatch_workgroups(1, 1, 1);
        }
        let pass = |enc: &mut wgpu::CommandEncoder, pipe: &wgpu::RenderPipeline, bg: &wgpu::BindGroup, target: &wgpu::TextureView| {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_pipeline(pipe);
            rp.set_bind_group(0, &self.g0_all, &[]);
            rp.set_bind_group(1, bg, &[]);
            rp.draw(0..3, 0..1);
        };
        for i in 0..BLOOM_LEVELS {
            pass(enc, &self.fs_down, &post.bg_down[i], &post.down[i]);
        }
        for i in (0..BLOOM_LEVELS - 1).rev() {
            pass(enc, &self.fs_up, &post.bg_up[i], &post.up[i]);
        }
        pass(enc, &self.fs_tonemap, &post.bg_tonemap, out);
        if self.reset > 0 {
            self.reset -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    fn validate(src: &str) {
        let m = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
            .validate(&m)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
    }

    #[test]
    fn shaders_validate() {
        validate(&format!("{}
{}", include_str!("atmo_common.wgsl"), include_str!("atmo.wgsl")));
        validate(&format!("{}
{}", include_str!("atmo_common.wgsl"), include_str!("shader.wgsl")));
    }

    #[test]
    fn scene_layout() {
        // 20 vec4, flags, a matrix, the camera
        assert_eq!(std::mem::size_of::<super::SceneU>(), 21 * 16 + 64 + 16 + 7 * 16);
    }

    #[test]
    fn ap_depth_reaches_the_horizon() {
        assert_eq!(super::ap_depth(6360.0, 0.002), super::AP_DEPTH);
        // from 3 km up the horizon is about 195 km away
        let d = super::ap_depth(6360.0, 3.0);
        assert!(d > 195.0 && d < 400.0, "{d}");
    }

    #[test]
    fn sea_waves_grow_with_wind() {
        let (l4, s4) = super::sea_waves(4.0, 400.0);
        let (l5, s5) = super::sea_waves(5.0, 100.0);
        // Beaufort 4: about 6.7 m/s, a fully developed sea of ~37 m waves
        assert!(l4 > 30.0 && l4 < 45.0, "{l4}");
        assert!(l5 > l4 && s5 > s4);
        assert!((s4 - (0.003 + 0.00512 * 6.7)).abs() < 0.002, "{s4}");
    }

    #[test]
    fn fallback_sky() {
        let noon = super::Sky::fallback(13.0);
        assert!(!noon.key_is_moon && noon.key_dir.y > 0.8);
        let night = super::Sky::fallback(1.0);
        assert!(night.key_is_moon && night.key_dir.y > 0.0);
    }
}
