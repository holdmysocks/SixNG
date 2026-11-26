// Shared by atmo.wgsl and shader.wgsl (prepended to both): the Scene
// uniform, the atmosphere's medium and its look-up table mappings. Each
// module declares the globals S, lin_clamp, t_trans and t_sky itself.

// Map 3D atmosphere and post chain (see atmo.rs).
//
// The sky is the game's own physical atmosphere: the timecycle's
// `sky_new_*` variables describe a planet radius, an atmosphere top
// (Karman line), Rayleigh and Mie scattering with their scale heights,
// Mie absorption and phase eccentricity, and a two-piece ozone layer. They
// are rendered here with look-up tables, as is usual for this model:
// transmittance (to the top of the atmosphere), multiple scattering, the
// sky as seen from the camera, and a froxel volume of aerial perspective
// (in-scattering and transmittance between the camera and each point).
// Lengths are km in the atmosphere code and m elsewhere.

struct Scene {
    ray: vec4<f32>,       // Rayleigh scattering per km (rgb), scale height km
    mie_s: vec4<f32>,     // Mie scattering per km (rgb), scale height km
    mie_a: vec4<f32>,     // Mie absorption per km (rgb), phase eccentricity
    ozo: vec4<f32>,       // ozone absorption per km (rgb), width of the lower layer km
    ozo_lin: vec4<f32>,   // ozone density: lower linear, lower constant, upper linear, upper constant
    radii: vec4<f32>,     // planet radius km, atmosphere top km, camera height km, aerial perspective depth km
    sun: vec4<f32>,       // towards the sun (unit), w disc half angle (rad)
    sun_sky: vec4<f32>,   // sun illuminance the sky scatters (lux, rgb), w unused
    sun_dir_e: vec4<f32>, // sun illuminance on surfaces before the air (lux, rgb), w unused
    moon: vec4<f32>,      // towards the moon (unit), w disc half angle (rad)
    moon_sky: vec4<f32>,  // moon illuminance the sky scatters (lux, rgb), w star brightness (cd/m2)
    moon_dir_e: vec4<f32>,// moon illuminance on surfaces before the air (lux, rgb), w disc luminance scale
    intens: vec4<f32>,    // sky, aerial perspective, multiple scattering, ambient scale
    fog0: vec4<f32>,      // height fog layer 0: density per m, falloff m, base height m, haze anisotropy
    fog1: vec4<f32>,      // height fog layer 1: density per m, falloff m, base height m, unused
    air_glow: vec4<f32>,  // night air glow luminance at the horizon (cd/m2, rgb), w ground albedo
    expo: vec4<f32>,      // exposure offset EV, lowest EV100, highest EV100, adaptation rate per s
    expo2: vec4<f32>,     // low percentile, high percentile, key value, frame time s
    post: vec4<f32>,      // bloom amount, contrast, contrast pivot, saturation
    post2: vec4<f32>,     // 1 = exposure reset this frame, sun disc boost, bounce intensity, unused
    flags: vec4<u32>,     // x 1 = map HDR frame, y frame index, z width, w height
    inv_vp: mat4x4<f32>,  // camera clip to world
    cam: vec4<f32>,       // camera position (world m), w unused
    // water: extinction / m (rgb), peak wavelength (m); scattering / m,
    // slope variance; phase, wave speed scale; body tint, glint roughness;
    // Fresnel rescale, min, max, time (s)
    water0: vec4<f32>,
    water1: vec4<f32>,
    water2: vec4<f32>,
    water3: vec4<f32>,
    water4: vec4<f32>,
    // clouds: cover, base and top (world y, m), extinction per m
    cloud0: vec4<f32>,
    // clouds: drift (m/s, x and z), w feature size (m)
    cloud1: vec4<f32>,
};

// State the GPU keeps between frames: [0] x exposure (multiplier on
// radiance), y current EV100, z metered EV100, w unused; [1..7] ambient
// irradiance / pi from the sky for normals +x -x +y -y +z -z; [7] sun
// transmittance at the camera (rgb); [8] moon transmittance at the camera.
struct State {
    v: array<vec4<f32>, 16>,
};

const PI: f32 = 3.14159265;
const TRANS_W: f32 = 256.0;
const TRANS_H: f32 = 64.0;
const MS_N: f32 = 32.0;
const SKY_W: f32 = 192.0;
const SKY_H: f32 = 108.0;
const AP_N: f32 = 32.0;

// ---- medium ----------------------------------------------------------

struct Medium {
    scat_r: vec3<f32>,
    scat_m: vec3<f32>,
    ext: vec3<f32>,
};

fn medium(h: f32) -> Medium {
    var m: Medium;
    let hh = max(h, 0.0);
    let dr = exp(-hh / max(S.ray.w, 0.01));
    let dm = exp(-hh / max(S.mie_s.w, 0.01));
    var dz = S.ozo_lin.x * hh + S.ozo_lin.y;
    if (hh >= S.ozo.w) { dz = S.ozo_lin.z * hh + S.ozo_lin.w; }
    dz = clamp(dz, 0.0, 1.0);
    m.scat_r = S.ray.rgb * dr;
    m.scat_m = S.mie_s.rgb * dm;
    m.ext = m.scat_r + m.scat_m + S.mie_a.rgb * dm + S.ozo.rgb * dz;
    return m;
}

fn phase_r(c: f32) -> f32 {
    return 3.0 / (16.0 * PI) * (1.0 + c * c);
}

// Cornette-Shanks
fn phase_m(c: f32, g: f32) -> f32 {
    let g2 = g * g;
    let k = 3.0 / (8.0 * PI) * (1.0 - g2) / (2.0 + g2);
    return k * (1.0 + c * c) / pow(max(1.0 + g2 - 2.0 * g * c, 1e-4), 1.5);
}

// distance along a ray from radius r (direction cosine mu to the zenith)
// to a sphere of radius R; negative when it misses
fn ray_sphere(r: f32, mu: f32, R: f32) -> f32 {
    let d = r * r * (mu * mu - 1.0) + R * R;
    if (d < 0.0) { return -1.0; }
    let s = sqrt(d);
    let t1 = -r * mu - s;
    if (t1 > 0.0) { return t1; }
    return -r * mu + s;
}

fn hits_ground(r: f32, mu: f32) -> bool {
    let Rb = S.radii.x;
    return mu < 0.0 && r * r * (mu * mu - 1.0) + Rb * Rb >= 0.0;
}

// ---- transmittance LUT ----------------------------------------------

fn trans_uv(r: f32, mu: f32) -> vec2<f32> {
    let Rb = S.radii.x;
    let Rt = S.radii.y;
    let H = sqrt(max(Rt * Rt - Rb * Rb, 0.0));
    let rho = sqrt(max(r * r - Rb * Rb, 0.0));
    let disc = r * r * (mu * mu - 1.0) + Rt * Rt;
    let d = max(0.0, -r * mu + sqrt(max(disc, 0.0)));
    let dmin = Rt - r;
    let dmax = rho + H;
    let x = (d - dmin) / max(dmax - dmin, 1e-4);
    let y = rho / H;
    // keep the samples on texel centres
    return vec2(0.5 / TRANS_W + x * (1.0 - 1.0 / TRANS_W), 0.5 / TRANS_H + y * (1.0 - 1.0 / TRANS_H));
}

fn trans_rmu(uv0: vec2<f32>) -> vec2<f32> {
    let uv = vec2((uv0.x - 0.5 / TRANS_W) / (1.0 - 1.0 / TRANS_W), (uv0.y - 0.5 / TRANS_H) / (1.0 - 1.0 / TRANS_H));
    let Rb = S.radii.x;
    let Rt = S.radii.y;
    let H = sqrt(max(Rt * Rt - Rb * Rb, 0.0));
    let rho = H * uv.y;
    let r = sqrt(rho * rho + Rb * Rb);
    let dmin = Rt - r;
    let dmax = rho + H;
    let d = dmin + uv.x * (dmax - dmin);
    var mu = 1.0;
    if (d > 0.0) { mu = (H * H - rho * rho - d * d) / (2.0 * r * d); }
    return vec2(r, clamp(mu, -1.0, 1.0));
}

fn transmittance(r: f32, mu: f32) -> vec3<f32> {
    if (hits_ground(r, mu)) { return vec3(0.0); }
    return textureSampleLevel(t_trans, lin_clamp, trans_uv(r, mu), 0.0).rgb;
}

fn cam_r() -> f32 {
    return S.radii.x + max(S.radii.z, 0.002);
}

// ---- sky-view LUT mapping ---------------------------------------------
// u: azimuth from the sun, sqrt-spaced (finer near the sun); v: view
// zenith angle, split at the horizon and squared towards it (finer there).

fn sun_frame() -> mat3x3<f32> {
    // x: horizontal towards the sun, y: up, z: the third axis
    var f = vec3(S.sun.x, 0.0, S.sun.z);
    if (dot(f, f) < 1e-8) { f = vec3(1.0, 0.0, 0.0); }
    f = normalize(f);
    let z = normalize(cross(f, vec3(0.0, 1.0, 0.0)));
    return mat3x3<f32>(f, vec3(0.0, 1.0, 0.0), z);
}

fn sky_dir(uv: vec2<f32>, r: f32) -> vec3<f32> {
    let Rb = S.radii.x;
    let vh = sqrt(max(r * r - Rb * Rb, 0.0));
    let beta = acos(clamp(vh / r, -1.0, 1.0));
    let zh = PI - beta;
    var vz = 0.0;
    if (uv.y < 0.5) {
        var c = 1.0 - 2.0 * uv.y;
        c = 1.0 - c * c;
        vz = zh * c;
    } else {
        var c = uv.y * 2.0 - 1.0;
        c = c * c;
        vz = zh + beta * c;
    }
    let az = uv.x * uv.x * PI;
    let s = sin(vz);
    let local = vec3(s * cos(az), cos(vz), s * sin(az));
    return sun_frame() * local;
}

fn sky_uv(d: vec3<f32>, r: f32) -> vec2<f32> {
    let Rb = S.radii.x;
    let vh = sqrt(max(r * r - Rb * Rb, 0.0));
    let beta = acos(clamp(vh / r, -1.0, 1.0));
    let zh = PI - beta;
    let vz = acos(clamp(d.y, -1.0, 1.0));
    var v = 0.0;
    if (vz < zh) {
        let c = sqrt(max(1.0 - vz / zh, 0.0));
        v = (1.0 - c) * 0.5;
    } else {
        let c = sqrt(max((vz - zh) / max(beta, 1e-4), 0.0));
        v = 0.5 + c * 0.5;
    }
    let f = sun_frame();
    let lx = dot(d, f[0]);
    let lz = dot(d, f[2]);
    let az = atan2(abs(lz), lx);
    let u = sqrt(clamp(az / PI, 0.0, 1.0));
    return vec2((u * (SKY_W - 1.0) + 0.5) / SKY_W, (v * (SKY_H - 1.0) + 0.5) / SKY_H);
}

// Distance (km) of slice z (0..AP_N-1): squared spacing, finer near the camera.
fn ap_depth(z: f32) -> f32 {
    let s = (z + 1.0) / AP_N;
    return s * s * S.radii.w;
}

