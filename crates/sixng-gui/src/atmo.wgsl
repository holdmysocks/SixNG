// Map 3D atmosphere compute passes and post chain (see atmo.rs); atmo_common.wgsl is prepended.

@group(0) @binding(0) var<uniform> S: Scene;
@group(0) @binding(1) var lin_clamp: sampler;
@group(0) @binding(2) var t_trans: texture_2d<f32>;
@group(0) @binding(3) var t_ms: texture_2d<f32>;
@group(0) @binding(4) var t_sky: texture_2d<f32>;
@group(0) @binding(5) var<storage, read_write> ST: State;

@group(1) @binding(0) var o_trans: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn cs_transmittance(@builtin(global_invocation_id) id: vec3<u32>) {
    if (f32(id.x) >= TRANS_W || f32(id.y) >= TRANS_H) { return; }
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2(TRANS_W, TRANS_H);
    let rm = trans_rmu(uv);
    let r = rm.x;
    let mu = rm.y;
    let Rb = S.radii.x;
    let len = ray_sphere(r, mu, S.radii.y);
    var od = vec3(0.0);
    let n = 40;
    let dt = max(len, 0.0) / f32(n);
    for (var i = 0; i < n; i++) {
        let t = (f32(i) + 0.5) * dt;
        let rr = sqrt(r * r + t * t + 2.0 * r * mu * t);
        od += medium(rr - Rb).ext * dt;
    }
    textureStore(o_trans, vec2<i32>(id.xy), vec4(exp(-od), 1.0));
}

// ---- multiple scattering LUT ------------------------------------------

fn ms_lookup(r: f32, mu_s: f32) -> vec3<f32> {
    let Rb = S.radii.x;
    let Rt = S.radii.y;
    let uv = vec2(clamp(mu_s * 0.5 + 0.5, 0.0, 1.0), clamp((r - Rb) / (Rt - Rb), 0.0, 1.0));
    let st = (uv * (MS_N - 1.0) + 0.5) / MS_N;
    return textureSampleLevel(t_ms, lin_clamp, st, 0.0).rgb;
}

@group(1) @binding(1) var o_ms: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn cs_multiscatter(@builtin(global_invocation_id) id: vec3<u32>) {
    if (f32(id.x) >= MS_N || f32(id.y) >= MS_N) { return; }
    let uv = vec2<f32>(id.xy) / (MS_N - 1.0);
    let Rb = S.radii.x;
    let Rt = S.radii.y;
    let mu_s = uv.x * 2.0 - 1.0;
    let r = Rb + max(uv.y * (Rt - Rb), 0.01);
    let sun = vec3(sqrt(max(1.0 - mu_s * mu_s, 0.0)), mu_s, 0.0);
    let p0 = vec3(0.0, r, 0.0);
    let albedo = S.air_glow.w;
    var l2 = vec3(0.0);
    var fms = vec3(0.0);
    let nd = 8;
    for (var a = 0; a < nd; a++) {
        for (var b = 0; b < nd; b++) {
            // directions spread evenly over the sphere
            let cz = 1.0 - 2.0 * (f32(a) + 0.5) / f32(nd);
            let ph = 2.0 * PI * (f32(b) + 0.5) / f32(nd);
            let sz = sqrt(max(1.0 - cz * cz, 0.0));
            let dir = vec3(sz * cos(ph), cz, sz * sin(ph));
            let mu = dir.y;
            var len = ray_sphere(r, mu, Rt);
            let ground = hits_ground(r, mu);
            if (ground) { len = ray_sphere(r, mu, Rb); }
            let n = 20;
            let dt = max(len, 0.0) / f32(n);
            var tr = vec3(1.0);
            var lsum = vec3(0.0);
            var fsum = vec3(0.0);
            for (var i = 0; i < n; i++) {
                let t = (f32(i) + 0.5) * dt;
                let p = p0 + dir * t;
                let pr = length(p);
                let m = medium(pr - Rb);
                let scat = m.scat_r + m.scat_m;
                let st = exp(-m.ext * dt);
                let integ = (vec3(1.0) - st) / max(m.ext, vec3(1e-7));
                let ts = transmittance(pr, dot(p, sun) / pr);
                lsum += tr * integ * scat * ts / (4.0 * PI);
                fsum += tr * integ * scat;
                tr *= st;
            }
            if (ground) {
                let pg = p0 + dir * len;
                let pr = length(pg);
                let ng = pg / pr;
                lsum += tr * transmittance(pr, dot(ng, sun)) * max(dot(ng, sun), 0.0) * albedo / PI;
            }
            l2 += lsum;
            fms += fsum / (4.0 * PI);
        }
    }
    let k = 4.0 * PI / f32(nd * nd);
    l2 = l2 * k / (4.0 * PI);
    fms = fms * k;
    let psi = l2 / max(vec3(1.0) - fms, vec3(1e-3));
    textureStore(o_ms, vec2<i32>(id.xy), vec4(psi, 1.0));
}

// ---- in-scattering along a view ray ------------------------------------

struct March {
    l: vec3<f32>,
    t: vec3<f32>,
};

// Light scattered towards the camera along dir from p0 (km, from the
// planet centre) over len km, with the sun and the moon.
fn march(p0: vec3<f32>, dir: vec3<f32>, len: f32, n: i32) -> March {
    let Rb = S.radii.x;
    let g = S.mie_a.w;
    let cs = dot(dir, S.sun.xyz);
    let cm = dot(dir, S.moon.xyz);
    let pr_s = phase_r(cs);
    let pm_s = phase_m(cs, g);
    let pr_m = phase_r(cm);
    let pm_m = phase_m(cm, g);
    var tr = vec3(1.0);
    var l = vec3(0.0);
    let dt = max(len, 0.0) / f32(n);
    for (var i = 0; i < n; i++) {
        let t = (f32(i) + 0.5) * dt;
        let p = p0 + dir * t;
        let pr = length(p);
        let up = p / pr;
        let m = medium(pr - Rb);
        let mu_s = dot(up, S.sun.xyz);
        let mu_m = dot(up, S.moon.xyz);
        let ms_s = ms_lookup(pr, mu_s) * S.intens.z;
        let ms_m = ms_lookup(pr, mu_m) * S.intens.z;
        let ls = transmittance(pr, mu_s) * (m.scat_r * pr_s + m.scat_m * pm_s) + ms_s * (m.scat_r + m.scat_m);
        let lm = transmittance(pr, mu_m) * (m.scat_r * pr_m + m.scat_m * pm_m) + ms_m * (m.scat_r + m.scat_m);
        let src = ls * S.sun_sky.rgb + lm * S.moon_sky.rgb;
        let st = exp(-m.ext * dt);
        let integ = (vec3(1.0) - st) / max(m.ext, vec3(1e-7));
        l += tr * src * integ;
        tr *= st;
    }
    var o: March;
    o.l = l;
    o.t = tr;
    return o;
}

// ---- sky-view LUT --------------------------------------------------------

@group(1) @binding(2) var o_sky: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn cs_skyview(@builtin(global_invocation_id) id: vec3<u32>) {
    if (f32(id.x) >= SKY_W || f32(id.y) >= SKY_H) { return; }
    let uv = vec2<f32>(id.xy) / vec2(SKY_W - 1.0, SKY_H - 1.0);
    let r = cam_r();
    let d = sky_dir(uv, r);
    let p0 = vec3(0.0, r, 0.0);
    var len = ray_sphere(r, d.y, S.radii.y);
    if (hits_ground(r, d.y)) { len = ray_sphere(r, d.y, S.radii.x); }
    let m = march(p0, d, max(len, 0.0), 32);
    // night air glow: brightest towards the horizon
    let glow = S.air_glow.rgb * (0.35 + 0.65 * pow(1.0 - abs(d.y), 4.0)) * select(1.0, 0.0, hits_ground(r, d.y));
    textureStore(o_sky, vec2<i32>(id.xy), vec4(m.l * S.intens.x + glow, 1.0));
}

// ---- aerial perspective volume -------------------------------------------

@group(1) @binding(3) var o_ap: texture_storage_3d<rgba16float, write>;

@compute @workgroup_size(4, 4, 4)
fn cs_aerial(@builtin(global_invocation_id) id: vec3<u32>) {
    if (f32(id.x) >= AP_N || f32(id.y) >= AP_N || f32(id.z) >= AP_N) { return; }
    let uv = (vec2<f32>(id.xy) + 0.5) / AP_N;
    let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    // reversed Z: 1 is the near plane, 0.5 any point further along
    let a = S.inv_vp * vec4(ndc, 1.0, 1.0);
    let b = S.inv_vp * vec4(ndc, 0.5, 1.0);
    let d = normalize(b.xyz / b.w - a.xyz / a.w);
    let r = cam_r();
    let p0 = vec3(0.0, r, 0.0);
    var len = ap_depth(f32(id.z));
    // stop at the ground
    if (hits_ground(r, d.y)) { len = min(len, ray_sphere(r, d.y, S.radii.x)); }
    let n = max(4, i32(id.z) / 2 + 4);
    let m = march(p0, d, len, n);
    let t = dot(m.t, vec3(1.0 / 3.0));
    textureStore(o_ap, vec3<i32>(id), vec4(m.l * S.intens.y, t));
}

// ---- ambient and camera transmittance ----------------------------------

var<workgroup> acc: array<vec3<f32>, 448>;

fn sky_radiance(d: vec3<f32>) -> vec3<f32> {
    return textureSampleLevel(t_sky, lin_clamp, sky_uv(d, cam_r()), 0.0).rgb;
}

// Irradiance of the sky on surfaces facing +x -x +y -y +z -z, from the
// sky-view LUT (cosine weighted) and a ground of the scene's albedo lit by
// the sun, moon and sky; one workgroup of 64.
@compute @workgroup_size(64)
fn cs_ambient(@builtin(local_invocation_index) li: u32) {
    let axes = array<vec3<f32>, 6>(vec3(1.0, 0.0, 0.0), vec3(-1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, -1.0, 0.0), vec3(0.0, 0.0, 1.0), vec3(0.0, 0.0, -1.0));
    let r = cam_r();
    let ts = transmittance(r, S.sun.y);
    let tm = transmittance(r, S.moon.y);
    // light on the ground: sun and moon, and the sky above it (cosine-weighted average of the zenith band)
    let e_dir = S.sun_dir_e.rgb * ts * max(S.sun.y, 0.0) + S.moon_dir_e.rgb * tm * max(S.moon.y, 0.0);
    var e = array<vec3<f32>, 7>(vec3(0.0), vec3(0.0), vec3(0.0), vec3(0.0), vec3(0.0), vec3(0.0), vec3(0.0));
    // 64 threads x 8 directions: 512 over the sphere
    for (var k = 0u; k < 8u; k++) {
        let s = li * 8u + k;
        let a = s / 32u;
        let b = s % 32u;
        let cz = 1.0 - 2.0 * (f32(a) + 0.5) / 16.0;
        let ph = 2.0 * PI * (f32(b) + 0.5) / 32.0;
        let sz = sqrt(max(1.0 - cz * cz, 0.0));
        let d = vec3(sz * cos(ph), cz, sz * sin(ph));
        var l = sky_radiance(d);
        if (d.y < 0.0) {
            // the scene's ground (sky irradiance on it added below, once known)
            l = e_dir * S.air_glow.w / PI;
        }
        for (var j = 0u; j < 6u; j++) {
            e[j] += l * max(dot(d, axes[j]), 0.0);
        }
        if (d.y >= 0.0) { e[6] += l * d.y; }
    }
    for (var j = 0u; j < 7u; j++) { acc[li * 7u + j] = e[j]; }
    workgroupBarrier();
    if (li == 0u) {
        var t = array<vec3<f32>, 7>(vec3(0.0), vec3(0.0), vec3(0.0), vec3(0.0), vec3(0.0), vec3(0.0), vec3(0.0));
        for (var i = 0u; i < 64u; i++) {
            for (var j = 0u; j < 7u; j++) { t[j] += acc[i * 7u + j]; }
        }
        // each direction stands for 4 pi / 512 sr
        let w = 4.0 * PI / 512.0;
        let sky_on_ground = t[6] * w * S.air_glow.w / PI;
        for (var j = 0u; j < 6u; j++) {
            // the ground's share from the sky: the lower hemisphere seen by the axis
            let below = select(0.5, select(0.0, 1.0, j == 3u), j == 2u || j == 3u);
            let g = sky_on_ground * PI * below;
            ST.v[1u + j] = vec4((t[j] * w + g) / PI * S.intens.w, 1.0);
        }
        ST.v[7] = vec4(ts, 1.0);
        ST.v[8] = vec4(tm, 1.0);
    }
}

// ---- exposure -----------------------------------------------------------
// A histogram of log2 luminance of the resolved frame (pre-exposed by the
// current exposure), then one workgroup averages between the timecycle's
// low and high percentiles and moves the exposure towards the key value.

const BINS: u32 = 128u;
const EV_LO: f32 = -16.0;
const EV_HI: f32 = 20.0;

@group(1) @binding(4) var t_hdr: texture_2d<f32>;
@group(1) @binding(5) var<storage, read_write> HIST: array<atomic<u32>, 128>;

@compute @workgroup_size(16, 16)
fn cs_histogram(@builtin(global_invocation_id) id: vec3<u32>) {
    let sz = textureDimensions(t_hdr);
    // every second pixel each way is plenty
    let p = id.xy * 2u;
    if (p.x >= sz.x || p.y >= sz.y) { return; }
    let c = textureLoad(t_hdr, vec2<i32>(p), 0).rgb;
    let exposure = max(ST.v[0].x, 1e-12);
    let lum = dot(c, vec3(0.2126, 0.7152, 0.0722)) / exposure;
    let ev = log2(max(lum, 1e-9));
    let b = u32(clamp((ev - EV_LO) / (EV_HI - EV_LO) * f32(BINS), 0.0, f32(BINS - 1u)));
    atomicAdd(&HIST[b], 1u);
}

var<workgroup> wh: array<u32, 128>;

// least sine of elevation the incident meter counts a light above the
// horizon at (about 20 degrees)
const SLANT_MIN: f32 = 0.35;

fn slant(y: f32) -> f32 {
    return select(0.0, max(y, SLANT_MIN) * smoothstep(-0.02, 0.05, y), y > -0.02);
}

// share of the incident meter in the exposure (rest: the histogram)
const INCIDENT: f32 = 0.75;

@compute @workgroup_size(128)
fn cs_exposure(@builtin(local_invocation_index) li: u32) {
    wh[li] = atomicLoad(&HIST[li]);
    atomicStore(&HIST[li], 0u);
    workgroupBarrier();
    if (li != 0u) { return; }
    var total = 0.0;
    for (var i = 0u; i < BINS; i++) { total += f32(wh[i]); }
    let lo = total * S.expo2.x;
    let hi = total * (1.0 - S.expo2.y);
    var run = 0.0;
    var sum = 0.0;
    var wsum = 0.0;
    for (var i = 0u; i < BINS; i++) {
        let c = f32(wh[i]);
        let a = clamp(run, lo, hi);
        let b = clamp(run + c, lo, hi);
        let w = b - a;
        let ev = EV_LO + (f32(i) + 0.5) / f32(BINS) * (EV_HI - EV_LO);
        sum += ev * w;
        wsum += w;
        run += c;
    }
    var avg = 0.0;
    if (wsum > 0.0) { avg = sum / wsum; } else { avg = ST.v[0].z; }
    // EV100 of the metered luminance (calibration constant 12.5)
    let ev_seen = avg + log2(100.0 / 12.5);
    // incident meter: the light falling on a level surface at the camera
    // (sun or moon through the air plus the sky), read as the luminance of
    // an 18 % grey card there. It keeps a large dark or bright area (water,
    // a white wall) from swinging the exposure; the histogram trims it.
    let w3 = vec3(0.2126, 0.7152, 0.0722);
    // a low sun or moon still lights what faces it (slopes, walls), so
    // its light counts as if at least SLANT_MIN up
    let e_dir = dot(S.sun_dir_e.rgb * ST.v[7].rgb, w3) * slant(S.sun.y)
        + dot(S.moon_dir_e.rgb * ST.v[8].rgb, w3) * slant(S.moon.y);
    let e_sky = PI * dot(ST.v[3].rgb, w3);
    let l_card = max(0.18 * (e_dir + e_sky) / PI, 1e-6);
    let ev_inc = log2(l_card * 100.0 / 12.5);
    var ev100 = ev_inc;
    if (wsum > 0.0) { ev100 = mix(ev_seen, ev_inc, INCIDENT); }
    // darker scenes are kept darker than mid grey (own rule, not decoded):
    // up to 3 stops under EV100 9
    let night = clamp((ev100 - 9.0) / 2.5, -3.0, 0.0);
    let key = max(S.expo2.z, 0.01);
    // exposure so the metered luminance lands on the key value
    var target_ev = clamp(ev100 - night - S.expo.x, S.expo.y, S.expo.z);
    var cur = ST.v[0].y;
    if (S.post2.x > 0.5 || ST.v[0].x <= 0.0 || cur != cur) {
        cur = target_ev;
    } else {
        let k = 1.0 - exp(-S.expo2.w * S.expo.w);
        cur = cur + (target_ev - cur) * k;
    }
    // L * exposure = key at the metered EV100
    let exposure = key / (exp2(cur) * 12.5 / 100.0);
    ST.v[0] = vec4(exposure, cur, ev100, 1.0);
}

// ---- bloom and tone mapping ------------------------------------------------

struct FsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> FsOut {
    let p = vec2(f32((i << 1u) & 2u), f32(i & 2u));
    var o: FsOut;
    o.clip = vec4(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2(p.x, 1.0 - p.y);
    return o;
}

@group(1) @binding(6) var t_src: texture_2d<f32>;
@group(1) @binding(7) var t_add: texture_2d<f32>;

fn tap(uv: vec2<f32>, o: vec2<f32>, px: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(t_src, lin_clamp, uv + o * px, 0.0).rgb;
}

// 13-tap downsample; the first level also caps fireflies by luminance
@fragment
fn fs_down(i: FsOut) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(t_src));
    let uv = i.uv;
    var c = tap(uv, vec2(0.0), px) * 0.125;
    c += (tap(uv, vec2(-1.0, -1.0), px) + tap(uv, vec2(1.0, -1.0), px) + tap(uv, vec2(-1.0, 1.0), px) + tap(uv, vec2(1.0, 1.0), px)) * 0.125;
    c += (tap(uv, vec2(-2.0, 0.0), px) + tap(uv, vec2(2.0, 0.0), px) + tap(uv, vec2(0.0, -2.0), px) + tap(uv, vec2(0.0, 2.0), px)) * 0.0625;
    c += (tap(uv, vec2(-2.0, -2.0), px) + tap(uv, vec2(2.0, -2.0), px) + tap(uv, vec2(-2.0, 2.0), px) + tap(uv, vec2(2.0, 2.0), px)) * 0.03125;
    let l = dot(c, vec3(0.2126, 0.7152, 0.0722));
    c = c / (1.0 + max(l - 64.0, 0.0) / 64.0);
    return vec4(c, 1.0);
}

// 3x3 tent upsample added to the next finer level
@fragment
fn fs_up(i: FsOut) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(t_src));
    let uv = i.uv;
    var c = tap(uv, vec2(0.0), px) * 4.0;
    c += (tap(uv, vec2(-1.0, 0.0), px) + tap(uv, vec2(1.0, 0.0), px) + tap(uv, vec2(0.0, -1.0), px) + tap(uv, vec2(0.0, 1.0), px)) * 2.0;
    c += tap(uv, vec2(-1.0, -1.0), px) + tap(uv, vec2(1.0, -1.0), px) + tap(uv, vec2(-1.0, 1.0), px) + tap(uv, vec2(1.0, 1.0), px);
    c = c / 16.0 + textureSampleLevel(t_add, lin_clamp, uv, 0.0).rgb;
    return vec4(c, 1.0);
}

fn aces_fit(x: vec3<f32>) -> vec3<f32> {
    // ACES RRT + ODT fit (sRGB/Rec.709 primaries in and out)
    let m1 = mat3x3<f32>(vec3(0.59719, 0.07600, 0.02840), vec3(0.35458, 0.90834, 0.13383), vec3(0.04823, 0.01566, 0.83777));
    let m2 = mat3x3<f32>(vec3(1.60475, -0.10208, -0.00327), vec3(-0.53108, 1.10813, -0.07276), vec3(-0.07367, -0.00605, 1.07602));
    let v = m1 * x;
    let a = v * (v + 0.0245786) - 0.000090537;
    let b = v * (0.983729 * v + 0.4329510) + 0.238081;
    return clamp(m2 * (a / b), vec3(0.0), vec3(1.0));
}

@fragment
fn fs_tonemap(i: FsOut) -> @location(0) vec4<f32> {
    var c = textureSampleLevel(t_src, lin_clamp, i.uv, 0.0).rgb;
    let bloom = textureSampleLevel(t_add, lin_clamp, i.uv, 0.0).rgb;
    c = mix(c, bloom, clamp(S.post.x, 0.0, 1.0));
    // contrast about the pivot, then saturation
    let pivot = max(S.post.z, 1e-3);
    c = pivot * pow(max(c, vec3(0.0)) / pivot, vec3(S.post.y));
    let l = dot(c, vec3(0.2126, 0.7152, 0.0722));
    c = max(mix(vec3(l), c, S.post.w), vec3(0.0));
    return vec4(aces_fit(c * 1.6), 1.0);
}
