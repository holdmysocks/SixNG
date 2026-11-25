// sixng viewer shaders: PBR-lite meshes (skinned, instanced), wireframe,
// debug lines, procedural surroundings (sky, sun, ground) and a ground shadow.

struct Frame {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    sun_dir: vec4<f32>,   // xyz towards the sun, w intensity
    sun_col: vec4<f32>,
    sky_top: vec4<f32>,   // w flake size of the body paint's coat
    sky_hor: vec4<f32>,   // w exposure
    mode: vec4<u32>,      // x view mode, y packed channel order, z paint finish, w slot view
    paint1: vec4<f32>,    // primary paint colour (linear), w the vehicle's own paint roughness
    paint2: vec4<f32>,    // pearl colour (linear), w pearl amount
    misc: vec4<f32>,      // x wetness, y emissive boost, z time, w ambient
    misc2: vec4<f32>,     // x fog density, yzw secondary paint colour
    misc3: vec4<f32>,     // x tint palette on (1) / off (0), w the vehicle's own paint metalness
    prev_view_proj: mat4x4<f32>,
    jitter: vec4<f32>,    // body paint's coat: x clear coat intensity, y clear coat roughness, z flake orientation (0 = no flakes); w vehicle wear (0 to 1; < 0: each part's own amount, column 11 of its coat row)
};

struct Mat {
    base: vec4<f32>,
    flags: vec4<u32>,     // x has-bits (1 base 2 normal 4 packed 8 emissive), y cls, z alpha (0 opaque 1 mask 2 blend), w gap in bits 0-1 (0 ok 1 low 2 flat), bit 8 = stand-in surface with metallic in bits 16-23 and roughness in bits 24-31, bit 9 = base texture is a tint mask, bit 12 = an emissive shader of another class (lit by its emissive map), bit 13 = the packed map is a vehicle material map, bit 14 = the emissive slot holds the part ramp image with the parts' surfaces
    tile: vec4<f32>,      // fabric detail tile: x UV scale, y rotation, z mean grey value, w bound
    emis: vec4<f32>,      // x emissive scale of the material (from the GLB's emissive strength), yzw the light's colour
};

// rows of the part ramp image: 256 each of the first layer's palette rows,
// the second layer's, the parts' surfaces and the parts' coats, and one row
// with the body paint's coat
const RAMP_ROWS: f32 = 1025.0;

@group(0) @binding(0) var<uniform> F: Frame;
@group(0) @binding(1) var<storage, read> J: array<mat4x4<f32>>;
@group(0) @binding(2) var samp: sampler;

// A sticker placed on a vehicle (custom liveries; `livery::Record`). The
// first record of the list is a header: its flags.x is the number of
// records after it.
struct Sticker {
    zones: array<u32, 8>, // one bit per sticker zone it lies on
    m: vec4<f32>,         // a, b, c, d: u = U*a + V*b + ou, v = U*c + V*d + ov
    o: vec4<f32>,         // ou, ov, opacity, picture layer (-1 = none)
    flags: vec4<u32>,     // x: 1 drawn, 2 colour mask, 4 highlight its zones, 8 outline it; y: model of the vehicle; z: UV set; w: 1 = zone under the pointer
    c0: vec4<f32>,        // colour of the mask's red (or the picture's tint)
    c1: vec4<f32>,
    c2: vec4<f32>,
};

@group(0) @binding(3) var<storage, read> ST: array<Sticker>;
@group(0) @binding(4) var t_stick: texture_2d_array<f32>;
@group(0) @binding(5) var samp_clamp: sampler;

// The 3D map's scene (atmo.rs, atmo_common.wgsl): the game's atmosphere
// as look-up tables, the exposure and the sky's ambient light. The model
// viewer binds a scene with flags.x = 0 and keeps its own light.
@group(0) @binding(6) var<uniform> S: Scene;
@group(0) @binding(7) var t_trans: texture_2d<f32>;
@group(0) @binding(8) var t_sky: texture_2d<f32>;
@group(0) @binding(9) var t_ap: texture_3d<f32>;
@group(0) @binding(10) var<storage, read> SCN: State;
@group(0) @binding(11) var lin_clamp: sampler;

// Sun and moon shadows (shadow.rs): four cascades from the light.
struct Shadow {
    mats: array<mat4x4<f32>, 4>,
    splits: vec4<f32>,
    texel: vec4<f32>,
    bias: vec4<f32>,
    // x on, y filter radius (texels), z softness
    params: vec4<f32>,
};
@group(0) @binding(12) var<uniform> SH: Shadow;
@group(0) @binding(13) var t_shadow: texture_depth_2d_array;
@group(0) @binding(14) var samp_shadow: sampler_comparison;

fn hdr() -> bool { return S.flags.x == 1u; }

// How lit p is in cascade c (1 lit, 0 shadowed; -1 outside the cascade).
fn shadow_in(p: vec3<f32>, c: u32) -> f32 {
    let q = SH.mats[c] * vec4(p, 1.0);
    let uv = vec2(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
    if (any(uv < vec2(0.0)) || any(uv > vec2(1.0)) || q.z > 1.0) { return -1.0; }
    let refd = q.z + SH.bias[c];
    let px = 1.0 / f32(textureDimensions(t_shadow).x);
    let rad = SH.params.y;
    // 4x4 filter of bilinear compares, weights fall off from the centre
    var lit = 0.0;
    var wsum = 0.0;
    for (var y = -1.5; y <= 1.5; y += 1.0) {
        for (var x = -1.5; x <= 1.5; x += 1.0) {
            let w = 1.0 - 0.15 * (abs(x) + abs(y));
            let o = vec2(x, y) * px * rad * 0.67;
            lit += w * textureSampleCompareLevel(t_shadow, samp_shadow, uv + o, i32(c), refd);
            wsum += w;
        }
    }
    return lit / wsum;
}

// How much of the sun (or moon) reaches world point p past the casters
// in the shadow cascades; 1 beyond their range or with shadows off.
fn sun_shadow(p: vec3<f32>) -> f32 {
    if (!hdr() || SH.params.x < 0.5) { return 1.0; }
    // view depth: the cascades split the distance along the view axis
    let depth = (F.view_proj * vec4(p, 1.0)).w;
    for (var c = 0u; c < 4u; c++) {
        let far = SH.splits[c];
        if (depth < far) {
            var s = shadow_in(p, c);
            if (s < 0.0) { continue; }
            // last tenth of a cascade: blend into the next (or into lit)
            let fade = clamp((depth - far * 0.9) / (far * 0.1), 0.0, 1.0);
            if (fade > 0.0) {
                var nxt = 1.0;
                if (c < 3u) {
                    let s2 = shadow_in(p, c + 1u);
                    if (s2 >= 0.0) { nxt = s2; }
                }
                s = mix(s, nxt, fade);
            }
            return s;
        }
    }
    return 1.0;
}

// share of a baked map tile's colour taken as the ground's reflectance
const TILE_ALBEDO: f32 = 0.6;

// radius (km from the planet centre) of a world point (m, y up)
fn point_r(p: vec3<f32>) -> f32 {
    return S.radii.x + max(p.y * 0.001, 0.002);
}

// Light of the sun (or the moon, whichever is the frame's light) that
// reaches world point p through the air, in lux times pi (the callers
// divide the Lambert term by pi); the model viewer's own light otherwise.
fn sun_e(p: vec3<f32>) -> vec3<f32> {
    if (!hdr()) { return F.sun_col.rgb * F.sun_dir.w * 3.0; }
    let r = point_r(p);
    let l = normalize(F.sun_dir.xyz);
    let sh = sun_shadow(p);
    if (dot(l, S.moon.xyz) > 0.99) {
        return S.moon_dir_e.rgb * transmittance(r, S.moon.y) * sh;
    }
    return S.sun_dir_e.rgb * transmittance(r, S.sun.y) * sh;
}

// Sky irradiance / pi on a surface facing n (ambient cube from the sky-view LUT).
fn sky_ambient(n: vec3<f32>) -> vec3<f32> {
    let n2 = n * n;
    let x = select(SCN.v[2].rgb, SCN.v[1].rgb, n.x >= 0.0);
    let y = select(SCN.v[4].rgb, SCN.v[3].rgb, n.y >= 0.0);
    let z = select(SCN.v[6].rgb, SCN.v[5].rgb, n.z >= 0.0);
    return n2.x * x + n2.y * y + n2.z * z;
}

// Radiance of the sky along d as seen from the camera (no sun or moon
// disc); under the horizon a ground of the scene's albedo.
fn sky_radiance(d: vec3<f32>) -> vec3<f32> {
    let r = S.radii.x + max(S.radii.z, 0.002);
    var l = textureSampleLevel(t_sky, lin_clamp, sky_uv(d, r), 0.0).rgb;
    if (hits_ground(r, d.y)) {
        let up = sky_ambient(vec3(0.0, 1.0, 0.0));
        let sun = S.sun_dir_e.rgb * SCN.v[7].rgb * max(S.sun.y, 0.0) + S.moon_dir_e.rgb * SCN.v[8].rgb * max(S.moon.y, 0.0);
        l += S.air_glow.w * (up + sun / PI) * 0.7;
    }
    return l;
}

fn hash3(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3(127.1, 311.7, 74.7))) * 43758.5453);
}

// The sun's and the moon's discs and the stars along d (unit), as luminance.
fn sky_discs(d: vec3<f32>) -> vec3<f32> {
    var c = vec3(0.0);
    let r = S.radii.x + max(S.radii.z, 0.002);
    let cs = dot(d, S.sun.xyz);
    let ws = S.sun.w;
    if (cs > cos(ws)) {
        let x = acos(clamp(cs, -1.0, 1.0)) / ws;
        let limb = 1.0 - 0.6 * (1.0 - sqrt(max(1.0 - x * x, 0.0)));
        c += S.sun_dir_e.rgb * SCN.v[7].rgb / (PI * ws * ws) * limb * S.post2.y;
    }
    let cm = dot(d, S.moon.xyz);
    let wm = S.moon.w;
    if (cm > cos(wm)) {
        c += S.moon_dir_e.rgb * SCN.v[8].rgb / (PI * wm * wm) * S.moon_dir_e.w;
    }
    if (d.y > 0.0 && S.moon_sky.w > 0.0) {
        // stars: one in some cells of a fine grid of directions
        let g = d * 420.0;
        let cell = floor(g);
        let h = hash3(cell);
        if (h > 0.985) {
            let o = vec3(hash3(cell + 1.7), hash3(cell + 3.1), hash3(cell + 5.3));
            let dd = length(fract(g) - o);
            let b = pow((h - 0.985) / 0.015, 6.0);
            c += vec3(0.85, 0.9, 1.0) * S.moon_sky.w * b * smoothstep(0.35, 0.0, dd) * transmittance(r, d.y);
        }
    }
    return c;
}

// Transmittance of the timecycle's two exponential height fog layers
// between the camera and p.
fn height_fog(p: vec3<f32>) -> f32 {
    let o = F.cam_pos.xyz;
    let len = length(p - o);
    var tau = 0.0;
    let n = 8;
    for (var i = 0; i < n; i++) {
        let y = mix(o.y, p.y, (f32(i) + 0.5) / f32(n));
        tau += S.fog0.x * exp(-max(y - S.fog0.z, 0.0) / max(S.fog0.y, 1.0));
        tau += S.fog1.x * exp(-max(y - S.fog1.z, 0.0) / max(S.fog1.y, 1.0));
    }
    return exp(-tau * len / f32(n));
}

// ---- water ------------------------------------------------------------
// The sea and the game's water meshes on the map: a sum of wind waves for
// the normal (wavelengths from the timecycle's Beaufort number and fetch,
// atmo.rs `sea_waves`), the sky mirrored by Fresnel, the sun's glint, and
// the water body's colour from its scattering and extinction.
const WAVES: u32 = 10u;
// least roughness of the sea's glint (keeps single-pixel sparkles out)
const SEA_MIN_ROUGH: f32 = 0.06;
// the baked map tiles' water colour (mapbake, 40 80 140 sRGB), linear
const TILE_SEA: vec3<f32> = vec3(0.021, 0.080, 0.262);
// highest terrain (map y, m) whose painted water is drawn as water: the
// terrain is sunk 1.5 m, so this is about 0.5 m above sea level
const TILE_WET_TOP: f32 = -1.0;
// share of the sea's slope variance drawn as waves (the rest is roughness)
const WAVE_SHARE: f32 = 0.6;

fn wave_rand(i: u32, k: f32) -> f32 {
    return fract(sin(f32(i) * 12.9898 + k * 78.233) * 43758.5453);
}

// Surface normal of the sea at p (world), and the slope variance of the
// waves too small to draw at this pixel (added to the roughness); `fp` is
// the pixel's size on the surface (m, from the screen derivatives).
fn sea_normal(p: vec3<f32>, fp: f32) -> vec4<f32> {
    let lp = max(S.water0.w, 2.0);
    let var_all = max(S.water1.w, 0.003);
    let s = sqrt(2.0 * WAVE_SHARE * var_all / f32(WAVES));
    let t = S.water4.w * S.water2.w;
    var g = vec2(0.0);
    var lost = 0.0;
    var lam = lp;
    for (var i = 0u; i < WAVES; i++) {
        let a = 0.6 + (wave_rand(i, 1.0) - 0.5) * 2.2;
        let d = vec2(cos(a), sin(a));
        // waves shorter than about eight pixels fade into roughness
        let keep = smoothstep(4.0 * fp, 12.0 * fp, lam);
        lost += (1.0 - keep * keep) * s * s * 0.5;
        if (keep > 0.0) {
            let k = 2.0 * PI / lam;
            let w = sqrt(9.81 * k);
            let ph = k * dot(d, p.xz) - w * t + wave_rand(i, 2.0) * 2.0 * PI;
            g += keep * s * d * cos(ph);
        }
        lam *= 0.62;
    }
    return vec4(normalize(vec3(-g.x, 1.0, -g.y)), lost);
}

// Size of a pixel on the surface at p (m), from the screen derivatives
// (grazing views give long footprints).
fn footprint(p: vec3<f32>) -> f32 {
    return max(length(dpdx(p)), length(dpdy(p)));
}

// Radiance leaving the water surface at p towards the camera; `fp` from
// `footprint` (taken in uniform control flow by the caller).
fn water_shade(p: vec3<f32>, fp: f32) -> vec3<f32> {
    let o = F.cam_pos.xyz;
    let dist = length(o - p);
    let v = (o - p) / max(dist, 1e-3);
    let sn = sea_normal(p, fp);
    var n = sn.xyz;
    if (dot(n, v) < 0.02) { n = normalize(n + v * (0.02 - dot(n, v))); }
    let var_micro = (1.0 - WAVE_SHARE) * max(S.water1.w, 0.003) + sn.w;
    let a = clamp(sqrt(2.0 * var_micro) + S.water3.w, SEA_MIN_ROUGH, 0.8);
    let ndv = max(dot(n, v), 1e-3);
    var fr = 0.02 + 0.98 * pow(1.0 - ndv, 5.0);
    fr = clamp(fr + clamp(S.water4.x, S.water4.y, S.water4.z), 0.0, 1.0);
    // the sky in the mirror direction (kept above the horizon)
    var r = reflect(-v, n);
    r.y = abs(r.y);
    let refl = environment(r, sqrt(a));
    // the sun's (or moon's) glint
    let l = normalize(F.sun_dir.xyz);
    let ndl = max(dot(n, l), 0.0);
    let h = normalize(l + v);
    let ndh = max(dot(n, h), 0.0);
    let a2 = a * a;
    let dd = ndh * ndh * (a2 - 1.0) + 1.0;
    let D = a2 / (PI * dd * dd);
    let kk = a2 * 0.5;
    let G = (ndl / (ndl * (1.0 - kk) + kk)) * (ndv / (ndv * (1.0 - kk) + kk));
    let fh = 0.02 + 0.98 * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    let sun = sun_e(p);
    let glint = D * G * fh / max(4.0 * ndl * ndv, 1e-3) * sun * ndl;
    // the water body: light going down, scattered back up (a third of
    // the single-scattering albedo, the usual ocean-colour rule)
    let ssa = S.water1.rgb / max(S.water0.rgb, vec3(1e-5));
    let e_down = sun * max(l.y, 0.0) * 0.98 + PI * sky_ambient(vec3(0.0, 1.0, 0.0));
    let body = 0.33 * ssa * e_down / PI * S.water3.rgb;
    return fr * refl + glint + (1.0 - fr) * body;
}

// The map's frame: aerial perspective (froxel volume), height fog, then
// the exposure; `col0` is the radiance leaving the surface at `p`.
fn map_finish(col0: vec3<f32>, p: vec3<f32>) -> vec3<f32> {
    var col = col0;
    let o = F.cam_pos.xyz;
    let v = p - o;
    let d_km = length(v) * 0.001;
    let c = F.view_proj * vec4(p, 1.0);
    let ndc = c.xy / max(c.w, 1e-6);
    let uv = vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let z = sqrt(clamp(d_km / max(S.radii.w, 0.001), 0.0, 1.0));
    var ap = textureSampleLevel(t_ap, lin_clamp, vec3(uv, max(z - 0.5 / AP_N, 0.5 / AP_N)), 0.0);
    // nearer than the first slice: fade in from nothing
    let k = clamp(d_km / ap_depth(0.0), 0.0, 1.0);
    ap = mix(vec4(0.0, 0.0, 0.0, 1.0), ap, k);
    col = col * ap.a + ap.rgb;
    let tf = height_fog(p);
    if (tf < 0.9999) {
        let dir = normalize(v);
        let g = S.fog0.w;
        let ph = (1.0 - g * g) / (4.0 * PI * pow(max(1.0 + g * g - 2.0 * g * dot(dir, S.sun.xyz), 1e-4), 1.5));
        let lit = S.sun_dir_e.rgb * SCN.v[7].rgb * ph + S.moon_dir_e.rgb * SCN.v[8].rgb / (4.0 * PI)
            + 0.5 * (sky_ambient(vec3(0.0, 1.0, 0.0)) + sky_ambient(vec3(0.0, -1.0, 0.0)));
        col = mix(lit, col, tf);
    }
    return min(col * SCN.v[0].x, vec3(30000.0));
}

@group(1) @binding(0) var<uniform> M: Mat;
@group(1) @binding(1) var t_base: texture_2d<f32>;
@group(1) @binding(2) var t_nrm: texture_2d<f32>;
@group(1) @binding(3) var t_pk: texture_2d<f32>;
@group(1) @binding(4) var t_em: texture_2d<f32>;
@group(1) @binding(5) var t_tile: texture_2d<f32>;
// vehicle parts: the game's tiling wear mask (flags.w bit 15)
@group(1) @binding(6) var t_wear: texture_2d<f32>;

// How often the wear mask repeats across a part's UV0 (the age records'
// minTiling / maxTiling are about 4 to 6.5).
const WEAR_TILING: f32 = 5.0;

// The wear mask's green value above which a part with wear amount `a` (0 to
// 1) shows its second layer: the share of the mask shown is 0.45 a^2 (none
// when new, 40 % at the most worn levels), read off the mask's green
// histogram (10 % of its texels are above 0.81, 20 % above 0.58, 30 % above
// 0.43, 40 % above 0.32, 50 % above 0.27). A stand-in: the game's windows
// on the three channels are not decoded to fields.
fn wear_threshold(a: f32) -> f32 {
    let c = 0.45 * a * a;
    if (c < 0.05) { return mix(1.01, 0.98, c / 0.05); }
    if (c < 0.1) { return mix(0.98, 0.81, (c - 0.05) / 0.05); }
    if (c < 0.2) { return mix(0.81, 0.58, (c - 0.1) / 0.1); }
    if (c < 0.3) { return mix(0.58, 0.43, (c - 0.2) / 0.1); }
    if (c < 0.4) { return mix(0.43, 0.32, (c - 0.3) / 0.1); }
    return mix(0.32, 0.27, min((c - 0.4) / 0.1, 1.0));
}

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) tan: vec4<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
    @location(5) col: vec4<f32>,
    @location(6) joints: vec4<u32>,
    @location(7) weights: vec4<f32>,
    @location(8) m0: vec4<f32>,
    @location(9) m1: vec4<f32>,
    @location(10) m2: vec4<f32>,
    @location(11) m3: vec4<f32>,
    @location(12) extra: vec4<f32>, // x skinned, y highlight, z tint code, w unused
    @location(13) tint: vec4<f32>,  // model tint palette colour (linear; white = none)
    @location(14) zone: vec4<u32>,  // x sticker zone, y model of an assembled vehicle
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) tan: vec4<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
    @location(5) col: vec4<f32>,
    @location(6) extra: vec4<f32>,
    @location(7) tint: vec4<f32>,
    // light a lit vehicle lamp adds (linear colour x intensity)
    @location(8) lamp: vec3<f32>,
    // sticker zone, model of an assembled vehicle
    @location(9) @interpolate(flat) zone: vec2<u32>,
};

fn skin_matrix(v: VIn) -> mat4x4<f32> {
    if (v.extra.x < 0.5) {
        return mat4x4<f32>(vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.0, 0.0, 0.0, 1.0));
    }
    let n = arrayLength(&J);
    var s = mat4x4<f32>(vec4(0.0), vec4(0.0), vec4(0.0), vec4(0.0));
    var tw = 0.0;
    for (var k = 0u; k < 4u; k++) {
        let w = v.weights[k];
        let j = v.joints[k];
        if (w > 0.0 && j < n) {
            s = s + J[j] * w;
            tw += w;
        }
    }
    if (tw < 0.001) {
        return mat4x4<f32>(vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.0, 0.0, 0.0, 1.0));
    }
    return s * (1.0 / tw);
}

fn mesh_vertex(v: VIn) -> VOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    // The bottom row of a joint matrix is (0, 0, 0, 1). The viewer puts the
    // light of a lit lamp in its first three entries (lamps hang on their
    // own bones); it is taken out here before the matrix is used.
    var sk = skin_matrix(v);
    let lamp = vec3(sk[0].w, sk[1].w, sk[2].w);
    sk[0].w = 0.0;
    sk[1].w = 0.0;
    sk[2].w = 0.0;
    let m = model * sk;
    let w = m * vec4(v.pos, 1.0);
    var o: VOut;
    o.lamp = lamp;
    o.clip = F.view_proj * w;
    o.world = w.xyz;
    o.nrm = (m * vec4(v.nrm, 0.0)).xyz;
    o.tan = vec4((m * vec4(v.tan.xyz, 0.0)).xyz, v.tan.w);
    o.uv0 = v.uv0;
    o.uv1 = v.uv1;
    o.col = v.col;
    o.extra = v.extra;
    // w: -1 on a mirrored instance (right-hand wheels): its triangles wind
    // the other way, so front_facing is reversed there
    let det = determinant(mat3x3<f32>(m[0].xyz, m[1].xyz, m[2].xyz));
    o.extra.w = select(1.0, -1.0, det < 0.0);
    o.tint = v.tint;
    o.zone = v.zone.xy;
    return o;
}

// The ground the surroundings show under the horizon: a plain of asphalt,
// lit by the sun and the sky like a diffuse surface facing up. Kept darker
// than the sky at the horizon (moonlight on it would outshine a night sky),
// so the horizon always reads.
fn ground_col() -> vec3<f32> {
    let sun = F.sun_col.rgb * (F.sun_dir.w * max(normalize(F.sun_dir.xyz).y, 0.0) * 3.0 / 3.14159);
    let g = vec3(0.185, 0.19, 0.20) * (sun + neutral(F.sky_top.rgb, 0.8) * F.misc.w);
    let w = vec3(0.2126, 0.7152, 0.0722);
    return g * min(1.0, 0.6 * dot(F.sky_hor.rgb, w) / max(dot(g, w), 0.00001));
}

// A glow around the sun, pow(s, n) * amp, widened by a reflection lobe of
// width a (roughness squared): the two widths add in quadrature and the
// peak drops by the same factor, so the glow keeps its energy.
fn sun_glow(s: f32, n: f32, amp: f32, a: f32) -> f32 {
    let k = 1.0 / (1.0 + 0.5 * n * a * a);
    return amp * k * pow(s, n * k);
}

// The surroundings along d (unit length): sky from horizon to zenith with
// the glow of the sun, and the ground under the horizon. `blur` is the
// roughness of the surface that reflects them (0 = as seen directly): the
// horizon line, the sky gradient and the glow widen with it. The sun's disc
// is not part of it: the direct specular term is its reflection.
fn environment(d: vec3<f32>, blur: f32) -> vec3<f32> {
    let a = blur * blur;
    if (hdr()) {
        return mix(sky_radiance(d), sky_ambient(d), smoothstep(0.15, 0.85, blur));
    }
    let y = clamp(d.y, -1.0, 1.0);
    let t = mix(max(y, 0.0), 0.35, min(a * 1.5, 1.0));
    var c = mix(F.sky_hor.rgb, F.sky_top.rgb, pow(t, 0.6));
    let s = max(dot(d, normalize(F.sun_dir.xyz)), 0.0);
    c += F.sun_col.rgb * ((sun_glow(s, 8.0, 0.15, a) + sun_glow(s, 160.0, 0.45, a)) * F.sun_dir.w);
    // the ground pales a little into the haze towards the horizon
    let g = mix(ground_col(), F.sky_hor.rgb, 0.1 * exp(min(y, 0.0) * 12.0) * (1.0 - min(a * 1.5, 1.0)));
    let w = 0.006 + 1.5 * a;
    return mix(g, c, smoothstep(-w, w, y));
}

// The surroundings as the camera sees them: with the sun's disc.
fn sky(d: vec3<f32>) -> vec3<f32> {
    if (hdr()) { return sky_radiance(d) + sky_discs(d); }
    let s = max(dot(d, normalize(F.sun_dir.xyz)), 0.0);
    let disc = smoothstep(0.99972, 0.99984, s) * smoothstep(-0.006, 0.006, d.y);
    return environment(d, 0.0) + F.sun_col.rgb * (disc * 40.0 * F.sun_dir.w);
}

// Diffuse light from the surroundings on a surface facing n: the sky,
// mostly desaturated (its full colour tinted every shaded surface
// blue-white), and on surfaces that face down the light the ground sends
// back.
fn ambient(n: vec3<f32>) -> vec3<f32> {
    if (hdr()) { return sky_ambient(n); }
    let sky_d = mix(neutral(F.sky_hor.rgb, 0.8) * 0.6, neutral(F.sky_top.rgb, 0.8), n.y * 0.5 + 0.5);
    return mix(sky_d, ground_col(), 0.8 * smoothstep(0.0, 1.0, -n.y)) * F.misc.w;
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3(0.0), vec3(1.0));
}

// pull a colour towards its own luminance by k (0 = unchanged, 1 = grey)
fn neutral(c: vec3<f32>, k: f32) -> vec3<f32> {
    return mix(c, vec3(dot(c, vec3(0.2126, 0.7152, 0.0722))), k);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

fn checker(uv: vec2<f32>) -> f32 {
    let c = floor(uv * 8.0);
    return select(0.35, 0.9, (i32(c.x) + i32(c.y)) % 2 == 0);
}

fn gap_tint(g: u32) -> vec3<f32> {
    if (g == 2u) { return vec3(0.95, 0.08, 0.05); }
    if (g == 1u) { return vec3(0.95, 0.75, 0.05); }
    return vec3(0.15, 0.75, 0.25);
}

// Brightness the clothing gradient map gives a colour at the tile's grey
// value g, relative to the colour itself: the dark half is exact (0.1 x the
// colour at g = 0, the colour at 0.5); the light half, which also
// desaturates in the game, is kept as a gentle brightening.
fn weave_gain(g: f32) -> f32 {
    if (g < 0.5) { return 0.1 + 1.8 * g; }
    return 1.0 + 0.5 * (g - 0.5);
}

@fragment
fn fs_mesh(i: VOut, @builtin(front_facing) ff: bool) -> @location(0) vec4<f32> {
    let fp = footprint(i.world);
    let has = M.flags.x;
    let cls = M.flags.y;
    var base = M.base;
    // clothing with a fabric detail tile: the base texture's alpha marks the
    // texels of the sub-material the tile belongs to
    var weave_w = 0.0;
    if ((has & 1u) != 0u) {
        var bt = textureSample(t_base, samp, i.uv0);
        // tint mask (animal coats): only the green channel (shading) is a picture
        if ((M.flags.w & 512u) != 0u) { bt = vec4(vec3(bt.g), 1.0); }
        if (M.tile.w > 0.5 && M.tile.w < 1.5) { weave_w = bt.a; bt.a = 1.0; }
        base = base * bt;
    }
    if (M.flags.z == 1u && base.a < 0.5) {
        discard;
    }
    var n = normalize(i.nrm);
    if (ff == (i.extra.w < 0.0)) { n = -n; }
    let tl = length(i.tan.xyz);
    if ((has & 2u) != 0u && tl > 0.1) {
        let t = normalize(i.tan.xyz - n * dot(n, i.tan.xyz) / tl * tl);
        let b = cross(n, t) * select(1.0, -1.0, i.tan.w < 0.0);
        var tn = textureSample(t_nrm, samp, i.uv0).xyz * 2.0 - 1.0;
        tn.z = sqrt(max(1.0 - tn.x * tn.x - tn.y * tn.y, 0.0));
        n = normalize(t * tn.x + b * tn.y + n * tn.z);
    }
    var rough = 0.75;
    var metal = 0.0;
    var ao = 1.0;
    if ((M.flags.w & 256u) != 0u) {
        // stand-in surface (tyre rubber, lamp-reflector chrome)
        metal = f32((M.flags.w >> 16u) & 255u) / 255.0;
        rough = clamp(f32(M.flags.w >> 24u) / 255.0, 0.04, 1.0);
    }
    // a vehicle layer's material map: blue is occlusion, on the direct
    // light as well (the game multiplies its diffuse and specular sides)
    var ao_direct = 1.0;
    // its red and green: where a part's metalness and roughness lie between
    // the low and the high of its surface
    var mat_blend = vec2(0.5);
    if ((has & 4u) != 0u && (M.flags.w & 8192u) != 0u) {
        let mm = textureSample(t_pk, samp, i.uv0);
        mat_blend = mm.rg;
        ao = mm.b;
        ao_direct = ao;
    } else if ((has & 4u) != 0u && F.mode.y < 3u) {
        let p = textureSample(t_pk, samp, i.uv0);
        if (F.mode.y == 0u) { rough = p.r; metal = p.g; ao = p.b; }
        else if (F.mode.y == 1u) { ao = p.r; rough = p.g; metal = p.b; }
        else { metal = p.r; rough = p.g; ao = p.b; }
        rough = clamp(rough, 0.04, 1.0);
    }
    var albedo = base.rgb;
    // the model's tint palette entry multiplies the base colour
    if (F.misc3.x > 0.5) { albedo = albedo * i.tint.rgb; }
    // close-up fabric weave: the baked colour was made at the tile's mean
    // grey value; the tile itself, sampled per pixel on the second UV set,
    // moves it to the value at this pixel (its mips bring it back to the
    // mean with distance)
    // vehicle parts: the colour of a part is a row of the material palette,
    // read at the column the layer detail texture's green gives at this
    // pixel (the vertex tint holds the row at that texture's mean). The
    // rows come as an image with one row per part id (vertex colour alpha).
    if (M.tile.w > 1.5) {
        let g = textureSample(t_tile, samp, i.uv0).g;
        let id = floor(i.col.a * 255.0 + 0.5);
        albedo = base.rgb * textureSampleLevel(t_em, samp, vec2(clamp(g, 0.004, 0.996), (id + 0.5) / RAMP_ROWS), 0.0).rgb;
    }
    if (M.tile.w > 0.5 && M.tile.w < 1.5) {
        let c = cos(M.tile.y);
        let s = sin(M.tile.y);
        let p = i.uv1 * M.tile.x;
        let g = textureSample(t_tile, samp, vec2(p.x * c - p.y * s, p.x * s + p.y * c)).g;
        albedo = albedo * mix(1.0, weave_gain(g) / weave_gain(M.tile.z), weave_w);
    }
    var alpha = base.a;
    var emissive = vec3(0.0);
    var pearl = vec3(0.0);
    // classes: 1 opaque 2 alpha 3 cutout 4 decal 5 glass 6 window 7 emissive 8 water 9 terrain 10 car_paint
    // vehicle parts coloured per vertex: tint alpha 254 marks the primary
    // body paint, other values carry the part's metalness and roughness
    var painted = cls == 10u;
    if ((M.flags.w & 1024u) != 0u) {
        let ta = i.tint.a * 255.0;
        if (ta > 253.5 && ta < 254.5) { painted = true; }
        else if (ta < 199.5) {
            // metalness in four steps x 50 + roughness in 50 steps
            let code = floor(ta + 0.5);
            metal = floor(code / 50.0) / 3.0;
            rough = clamp((code - floor(code / 50.0) * 50.0) / 49.0, 0.04, 1.0);
            // With the material map and the parts' surfaces at hand
            // (rows 512.. of the part ramp image: metalness low and high,
            // roughness low and high in the alpha of columns 0 to 3 of the
            // part id's row), the blend is done here per pixel, as the
            // game's paint shader does; the code above is the same blend
            // at the map's mean. The body paint keeps its own finish.
            if ((M.flags.w & 24576u) == 24576u && (has & 4u) != 0u) {
                let row = 512 + i32(floor(i.col.a * 255.0 + 0.5));
                let m_lo = textureLoad(t_em, vec2(0, row), 0).a;
                let m_hi = textureLoad(t_em, vec2(1, row), 0).a;
                let r_lo = textureLoad(t_em, vec2(2, row), 0).a;
                let r_hi = textureLoad(t_em, vec2(3, row), 0).a;
                metal = clamp(mix(m_lo, m_hi, mat_blend.x), 0.0, 1.0);
                rough = clamp(mix(r_lo, r_hi, mat_blend.y), 0.04, 1.0);
            }
        }
    }
    // the body paint's coat record (the vehicle's own finish only): clear
    // coat intensity and roughness, flake orientation
    var coat = vec3(0.0);
    if (painted) {
        albedo = F.paint1.rgb;
        if ((has & 16u) != 0u) { albedo = F.misc2.yzw; }
        if (F.mode.z == 0u) { metal = 0.6; rough = 0.28; }
        else if (F.mode.z == 1u) { metal = 0.0; rough = 0.8; }
        else if (F.mode.z == 2u) { metal = 1.0; rough = 0.04; albedo = mix(vec3(0.95), albedo, 0.25); }
        else if (F.mode.z == 4u) { metal = F.misc3.w; rough = clamp(F.paint1.w, 0.04, 1.0); coat = F.jitter.xyz; }
        else { metal = 0.0; rough = 0.12; }
        pearl = F.paint2.rgb * F.paint2.w;
    }
    // The coat record of a part (rows 768.. of the part ramp image, by part
    // id) or of the body paint (row 1024, with the vehicle's own finish):
    // its values are in the alpha of the row's first columns. fin: x
    // anisotropy, y sheen intensity, z film thickness; fin2: x sheen fade,
    // y sheen reflectance, z film IOR.
    var flake_size = F.sky_top.w;
    var fin = vec3(0.0);
    var fin2 = vec3(0.0);
    var coat_row = -1;
    if ((M.flags.w & 16384u) != 0u) {
        if (painted) {
            if (F.mode.z == 4u) { coat_row = 1024; }
        } else if ((M.flags.w & 1024u) != 0u) {
            coat_row = 768 + i32(floor(i.col.a * 255.0 + 0.5));
        }
    }
    if (coat_row >= 0) {
        coat = vec3(textureLoad(t_em, vec2(0, coat_row), 0).a, textureLoad(t_em, vec2(1, coat_row), 0).a, textureLoad(t_em, vec2(2, coat_row), 0).a * 0.25);
        flake_size = textureLoad(t_em, vec2(3, coat_row), 0).a * 0.25;
        // the record's base coat roughness: on the body paint only; other
        // parts keep the roughness of their own surface (whether the game
        // lets the coat override it there is not known)
        let own = textureLoad(t_em, vec2(4, coat_row), 0).a;
        if (own > 0.0 && painted) { rough = clamp(own, 0.04, 1.0); }
        fin = vec3(textureLoad(t_em, vec2(5, coat_row), 0).a, textureLoad(t_em, vec2(6, coat_row), 0).a, textureLoad(t_em, vec2(10, coat_row), 0).a);
        fin2 = vec3(textureLoad(t_em, vec2(7, coat_row), 0).a, textureLoad(t_em, vec2(8, coat_row), 0).a * 0.25, textureLoad(t_em, vec2(9, coat_row), 0).a * 4.0);
    }
    // Stickers placed on the vehicle (custom liveries). Like the game's
    // two-layer vehicle shader: a sticker lies on the sticker zones it
    // lists and is drawn where the mesh UV, taken through its 2x2 matrix
    // and offset, is between 0 and 1. A colour mask's red, green and blue
    // are the cover of three colours; other pictures bring their own
    // colours and alpha. Later stickers lie on top. The list is short
    // (`livery::MAX_STICKERS`), so every pixel walks it.
    var st_hl = 0.0;
    var st_edge = 0.0;
    let st_n = min(ST[0].flags.x, arrayLength(&ST) - 1u);
    if (st_n > 0u) {
        let dx0 = dpdx(i.uv0);
        let dy0 = dpdy(i.uv0);
        let dx1 = dpdx(i.uv1);
        let dy1 = dpdy(i.uv1);
        let zb = i.zone.x & 255u;
        for (var k = 1u; k <= st_n; k++) {
            let fl = ST[k].flags;
            if ((fl.x & 1u) == 0u || fl.y != i.zone.y) { continue; }
            if (((ST[k].zones[zb >> 5u] >> (zb & 31u)) & 1u) == 0u) { continue; }
            if ((fl.x & 4u) != 0u) { st_hl = max(st_hl, select(0.22, 0.5, fl.w == 1u)); }
            let o = ST[k].o;
            if (o.w < 0.0 && (fl.x & 8u) == 0u) { continue; }
            let m = ST[k].m;
            var uv = i.uv0;
            var dx = dx0;
            var dy = dy0;
            if (fl.z == 1u) { uv = i.uv1; dx = dx1; dy = dy1; }
            let p = vec2(dot(m.xy, uv), dot(m.zw, uv)) + o.xy;
            if (p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0) { continue; }
            let gx = vec2(dot(m.xy, dx), dot(m.zw, dx));
            let gy = vec2(dot(m.xy, dy), dot(m.zw, dy));
            if (o.w >= 0.0) {
                let t = textureSampleGrad(t_stick, samp_clamp, p, i32(o.w + 0.5), gx, gy);
                var c = srgb_to_linear(t.rgb) * ST[k].c0.rgb;
                var a = t.a;
                if ((fl.x & 2u) != 0u) {
                    let sum = t.r + t.g + t.b;
                    a = clamp(sum, 0.0, 1.0);
                    c = (ST[k].c0.rgb * t.r + ST[k].c1.rgb * t.g + ST[k].c2.rgb * t.b) / max(sum, 0.0001);
                }
                a = a * o.z;
                albedo = mix(albedo, c, a);
                metal = metal * (1.0 - a);
                rough = mix(rough, 0.45, a);
                pearl = pearl * (1.0 - a);
            }
            if ((fl.x & 8u) != 0u) {
                // the selected sticker's border, about two pixels wide
                let w = abs(gx) + abs(gy);
                let e = min(min(p.x, 1.0 - p.x) / max(w.x, 0.000001), min(p.y, 1.0 - p.y) / max(w.y, 0.000001));
                if (e < 2.0) { st_edge = 1.0; }
            }
        }
    }
    // vehicle parts, second layer of the surface (rust under paint, grey
    // under bare metal): rows 256..512 of the part ramp image hold its colour
    // and roughness (alpha); its metalness is the alpha of the first rows.
    // Shown where the layer detail texture's blue is high (taken as the
    // wear mask), by the amount the user sets, or by each part's own amount
    // from the vehicle's configuration (its wear level).
    // With the game's wear mask bound, the second layer shows where the
    // mask's green is above the part's threshold (`wear_threshold`), in
    // patches; the detail texture then only gives the palette column.
    var wear_amt = F.jitter.w;
    let wear_mask = (M.flags.w & 0x8000u) != 0u;
    let wear_on = M.tile.w > 1.5 || wear_mask;
    if (wear_on && wear_amt < 0.0) {
        wear_amt = textureLoad(t_em, vec2(11, 768 + i32(floor(i.col.a * 255.0 + 0.5))), 0).a;
    }
    if (wear_on && wear_amt > 0.0) {
        var u = 0.5;
        var k = 0.0;
        if (M.tile.w > 1.5) {
            let d = textureSample(t_tile, samp, i.uv0);
            u = clamp(d.g, 0.004, 0.996);
            k = clamp(d.b * wear_amt * 2.0, 0.0, 1.0);
        }
        if (wear_mask) {
            let t = wear_threshold(wear_amt);
            k = smoothstep(t, t + 0.08, textureSample(t_wear, samp, i.uv0 * WEAR_TILING).g);
        }
        let id = floor(i.col.a * 255.0 + 0.5);
        let w1 = textureSampleLevel(t_em, samp, vec2(u, (id + 256.5) / RAMP_ROWS), 0.0);
        let m1 = textureSampleLevel(t_em, samp, vec2(u, (id + 0.5) / RAMP_ROWS), 0.0).a;
        albedo = mix(albedo, w1.rgb, k);
        rough = clamp(mix(rough, w1.a, k), 0.04, 1.0);
        metal = mix(metal, m1, k);
        pearl = pearl * (1.0 - k);
    }
    if (cls == 5u || cls == 6u) {
        rough = 0.04; metal = 0.0;
        alpha = min(alpha, 0.45);
        if (F.misc3.z > 0.5) {
            // map: building glass reads as dark reflective glass (the
            // game's reflections and interiors behind it are not drawn; the
            // texture is only a light dirt layer, so pale and see-through
            // otherwise)
            albedo = albedo * 0.12 + vec3(0.025, 0.03, 0.04);
            alpha = 0.9;
        }
    } else if (cls == 8u) {
        albedo = vec3(0.04, 0.12, 0.16); rough = 0.05; alpha = 0.7;
    }
    // class 7, or an emissive shader of another class (flags.w bit 12:
    // a bulb's emissive layer, a neon tube), which lights by its emissive map
    if (cls == 7u || ((M.flags.w & 4096u) != 0u && (has & 8u) != 0u)) {
        var e = albedo;
        if ((has & 8u) != 0u) { e = textureSample(t_em, samp, i.uv0).rgb; }
        emissive = e * M.emis.yzw * F.misc.y * M.emis.x;
        // A coloured light (neon) far above white washes out to white in
        // the tone map; its brightest channel is capped at 1.2 after
        // exposure, all channels in proportion, so it keeps its colour.
        // White lights are not capped.
        let sat = 1.0 - min(M.emis.y, min(M.emis.z, M.emis.w)) / max(max(M.emis.y, max(M.emis.z, M.emis.w)), 1.0e-4);
        let peak = max(emissive.r, max(emissive.g, emissive.b)) * F.sky_hor.w;
        let cap = mix(1.0e4, 1.2, smoothstep(0.05, 0.3, sat));
        if (peak > cap) { emissive = emissive * (cap / peak); }
    }
    // vehicle lamps: a lamp's own emissive mesh (bit 11) is lit only while
    // its bone is; reflectors and lenses on a lit lamp bone glow in its colour
    let lamp_i = max(i.lamp.r, max(i.lamp.g, i.lamp.b));
    if ((M.flags.w & 2048u) != 0u) {
        emissive = emissive * min(lamp_i, 1.5);
    } else if (lamp_i > 0.001) {
        emissive = emissive + i.lamp * F.misc.y;
        if (cls == 5u || cls == 6u) { alpha = max(alpha, min(lamp_i, 1.0) * 0.85); }
    }
    // brake heat (instance extra.z < 0, set on brake discs): a hot disc
    // glows from dull red to orange
    if (i.extra.z < -0.001) {
        let heat = min(-i.extra.z, 1.0);
        emissive = emissive + mix(vec3(0.5, 0.015, 0.0), vec3(1.5, 0.32, 0.03), heat) * heat * heat * F.misc.y;
        albedo = albedo * (1.0 - 0.6 * heat);
    }
    // wet look: darker albedo, glossy up-facing surfaces
    let wet = F.misc.x * smoothstep(0.3, 0.8, n.y);
    albedo = albedo * (1.0 - 0.45 * F.misc.x);
    rough = mix(rough, 0.06, wet);

    let v = normalize(F.cam_pos.xyz - i.world);
    let l = normalize(F.sun_dir.xyz);
    let h = normalize(v + l);
    let ndl = max(dot(n, l), 0.0);
    let ndv = max(dot(n, v), 0.001);
    // Metal flakes in the base coat: each flake (a cell of a grid in space,
    // its size from the coat record) tilts the base coat's highlight by the
    // record's orientation spread. Flakes smaller than a pixel widen the
    // highlight instead of sparkling.
    var nb = n;
    if (coat.z > 0.0 && cls != 11u) {
        let p = i.world / (0.0006 + 0.02 * flake_size);
        let seen = clamp(1.5 - max(length(dpdx(p)), length(dpdy(p))), 0.0, 1.0);
        let c = floor(p);
        let q = fract(sin(vec3(dot(c, vec3(127.1, 311.7, 74.7)), dot(c, vec3(269.5, 183.3, 246.1)), dot(c, vec3(113.5, 271.9, 124.6)))) * 43758.5453) - 0.5;
        let spread = coat.z * 6.0;
        nb = normalize(n + q * (2.0 * spread * seen));
        rough = clamp(sqrt(rough * rough + spread * spread * (1.0 - seen)), 0.04, 1.0);
    }
    let ndh = max(dot(nb, h), 0.0);
    let a2 = rough * rough * rough * rough;
    let dd = ndh * ndh * (a2 - 1.0) + 1.0;
    var D = a2 / (3.14159 * dd * dd);
    // Anisotropy (brushed metal, carbon weave): the highlight is stretched
    // across the surface's tangent and narrowed along it, and the mirror
    // image of the surroundings is bent the same way.
    var n_env = n;
    let tan_len = length(i.tan.xyz);
    if (fin.x > 0.0 && tan_len > 0.001) {
        let t = normalize(i.tan.xyz - nb * dot(nb, i.tan.xyz));
        let b = cross(nb, t);
        let a = rough * rough;
        let at = max(a * (1.0 + fin.x), 0.002);
        let ab = max(a * (1.0 - fin.x), 0.002);
        let th = dot(t, h) / at;
        let bh = dot(b, h) / ab;
        let ad = th * th + bh * bh + ndh * ndh;
        D = 1.0 / (3.14159 * at * ab * ad * ad);
        n_env = normalize(mix(n, cross(cross(b, v), b), fin.x));
    }
    let k = (rough + 1.0) * (rough + 1.0) / 8.0;
    let G = (ndl / (ndl * (1.0 - k) + k)) * (ndv / (ndv * (1.0 - k) + k));
    var f0 = mix(vec3(0.04), albedo, metal);
    // Thin film (pearlescent paint): light mirrored at the top and at the
    // bottom of a film interferes, so the colour of the mirror image turns
    // with the viewing angle. The thickness is taken as micrometres. Only
    // the colour of the reflection changes, not how much is reflected, so
    // the paint under it keeps its colour.
    if (fin.z > 0.0) {
        let ior = max(fin2.z, 1.0);
        let ct = sqrt(1.0 - (1.0 - ndv * ndv) / (ior * ior));
        let path = 2.0 * ior * fin.z * 1000.0 * ct;
        let wave = 0.5 + 0.5 * cos(6.28318 * path / vec3(650.0, 532.0, 450.0));
        let tint = wave / max(dot(wave, vec3(1.0 / 3.0)), 0.05);
        let r0 = (ior - 1.0) / (ior + 1.0);
        f0 = f0 * mix(vec3(1.0), tint, clamp(2.0 * r0, 0.0, 1.0));
    }
    let Fr = f0 + (1.0 - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    let spec = D * G * Fr / max(4.0 * ndl * ndv, 0.001);
    let kd = (1.0 - Fr) * (1.0 - metal);
    let direct = (kd * albedo / 3.14159 + spec) * sun_e(i.world) * ndl;
    let amb_d = ambient(n);
    let fr_v = f0 + (max(vec3(1.0 - rough), f0) - f0) * pow(1.0 - ndv, 5.0);
    let r = reflect(-v, n);
    let env = environment(reflect(-v, n_env), rough) * F.misc.w;
    var col = direct * ao_direct + kd * albedo * amb_d * ao + env * fr_v * ao + emissive;
    // Sheen (velvet, suede): fibres standing on the surface catch light
    // towards the silhouette. The reflectance is the share seen head-on;
    // the fade sets how fast it grows towards the edge.
    if (fin.y > 0.0) {
        let edge = pow(1.0 - ndv, 1.0 + 4.0 * fin2.x);
        let sheen = fin.y * (fin2.y + (1.0 - fin2.y) * edge);
        col += sheen * (sun_e(i.world) * ndl / 3.14159 * ao_direct + amb_d * ao);
    }
    // Clear coat: a second, glossy layer over the base coat (a dielectric,
    // 4 % at normal incidence). It mirrors the sun and the surroundings at
    // its own roughness, and what it reflects does not reach the base coat.
    if (coat.x > 0.0) {
        let cr = clamp(coat.y, 0.03, 1.0);
        let ca2 = cr * cr * cr * cr;
        let cnh = max(dot(n, h), 0.0);
        let cdd = cnh * cnh * (ca2 - 1.0) + 1.0;
        let ck = (cr + 1.0) * (cr + 1.0) / 8.0;
        let cG = (ndl / (ndl * (1.0 - ck) + ck)) * (ndv / (ndv * (1.0 - ck) + ck));
        let cF = 0.04 + 0.96 * pow(1.0 - max(dot(h, v), 0.0), 5.0);
        let cspec = ca2 / (3.14159 * cdd * cdd) * cG * cF / max(4.0 * ndl * ndv, 0.001);
        let cfv = 0.04 + (max(1.0 - cr, 0.04) - 0.04) * pow(1.0 - ndv, 5.0);
        col = (col - emissive) * (1.0 - coat.x * cfv) + emissive
            + coat.x * (cspec * sun_e(i.world) * ndl * ao_direct + environment(r, cr) * F.misc.w * cfv * ao);
    }
    col += pearl * pow(1.0 - ndv, 2.0) * 0.6;
    if (cls == 11u) {
        // baked map tiles (already hill-shaded): soft diffuse, no sky reflection
        if (hdr()) {
            // the tiles carry the map's own light and hill shading: taken
            // down to a ground's reflectance, half of the slope shading kept
            col = albedo * TILE_ALBEDO * (sun_e(i.world) * (0.5 + 0.5 * ndl) / 3.14159 + amb_d);
            // the tiles' painted water (the map's water and no-data colour)
            // is shaded as the game's water where the terrain lies at sea
            // level (its no-data cells and low shores); higher up the same
            // paint marks wet sand and land, which stay ground
            let painted = 1.0 - smoothstep(0.02, 0.07, length(base.rgb - TILE_SEA));
            let low = 1.0 - smoothstep(TILE_WET_TOP - 1.0, TILE_WET_TOP, i.world.y);
            let wet = painted * low;
            if (wet > 0.0) {
                col = mix(col, water_shade(i.world, fp), wet);
            }
        } else {
            col = albedo * (0.55 * F.sun_col.rgb * F.sun_dir.w * (0.35 + 0.65 * ndl) + 0.7 * amb_d);
        }
    }

    if (hdr() && cls == 8u && F.mode.x == 0u) {
        // the map's water (the sea plane is opaque)
        col = water_shade(i.world, fp);
        alpha = select(0.92, 1.0, M.flags.z == 0u);
    }
    let mode = F.mode.x;
    if (mode == 1u) { col = albedo; }
    else if (mode == 2u) { return vec4(n * 0.5 + 0.5, 1.0); }
    else if (mode == 3u) { col = vec3(checker(i.uv0)) * vec3(fract(i.uv0.x), fract(i.uv0.y), 0.6) * 1.3; }
    else if (mode == 4u) {
        let s = F.mode.w;
        var t = vec4(0.5, 0.5, 0.5, 1.0);
        var bit = 1u;
        if (s == 0u) { t = textureSample(t_base, samp, i.uv0); bit = 1u; }
        else if (s == 1u) { t = textureSample(t_nrm, samp, i.uv0); bit = 2u; }
        else if (s == 2u) { t = textureSample(t_pk, samp, i.uv0); bit = 4u; }
        else { t = textureSample(t_em, samp, i.uv0); bit = 8u; }
        if ((has & bit) == 0u) { t = vec4(vec3(checker(i.uv0 * 0.25)) * vec3(0.6, 0.2, 0.6), 1.0); }
        return vec4(t.rgb, 1.0);
    }
    else if (mode == 5u) {
        let shade = 0.35 + 0.65 * ndl + 0.25 * (n.y * 0.5 + 0.5);
        col = gap_tint(M.flags.w & 3u) * shade * 0.6;
    }
    else if (mode == 6u) { col = i.col.rgb; }
    else if (mode == 7u) {
        let w = vec3(0.8);
        col = w / 3.14159 * sun_e(i.world) * ndl + w * amb_d;
    }
    // map tint (instance extra.z): 1 = selected, 2.. = gap heat
    if (i.extra.y > 0.5) { col = mix(col, vec3(1.0, 0.55, 0.1), 0.45); }
    if (i.extra.z > 0.5) { col = mix(col, gap_tint(u32(i.extra.z + 0.5) - 1u), 0.6); }
    // livery editor: the chosen sticker's zones and its border
    if (st_hl > 0.0) { col = mix(col, vec3(0.1, 0.55, 1.0), st_hl); }
    if (st_edge > 0.0) { col = mix(col, vec3(1.0, 0.85, 0.1), 0.9); }
    if (hdr() && (mode == 0u || mode == 7u)) {
        col = map_finish(col, i.world);
    } else if (!hdr()) {
        // distance fog
        let dist = length(F.cam_pos.xyz - i.world);
        let fog = 1.0 - exp(-dist * F.misc2.x);
        col = mix(col, neutral(sky(-v), 0.5) * 0.9, fog);
        if (mode != 1u && mode != 3u && mode != 6u) {
            col = aces(col * F.sky_hor.w);
        }
    }
    if (M.flags.z != 2u && cls != 5u && cls != 6u && cls != 8u) { alpha = 1.0; }
    return vec4(col, alpha);
}

@vertex
fn vs_mesh(v: VIn) -> VOut {
    return mesh_vertex(v);
}

// wireframe: same vertex path, flat colour, pulled towards the camera
// Shadow casters: depth only, alpha-tested cut-outs (leaves, fences).
@fragment
fn fs_shadow(i: VOut) {
    let a = M.base.a * select(1.0, textureSample(t_base, samp, i.uv0).a, (M.flags.x & 1u) != 0u);
    if (M.flags.z == 1u && a < 0.5) {
        discard;
    }
}

@vertex
fn vs_wire(v: VIn) -> VOut {
    var o = mesh_vertex(v);
    // reversed Z: larger depth is nearer; a relative nudge works at any distance
    o.clip.z = o.clip.z * 1.002;
    return o;
}

@fragment
fn fs_wire(i: VOut) -> @location(0) vec4<f32> {
    return vec4(0.02, 0.02, 0.05, 0.45);
}

// debug lines (skeleton, bboxes, overlays)
struct LIn {
    @location(0) pos: vec3<f32>,
    @location(1) col: vec4<f32>,
};
struct LOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) col: vec4<f32>,
};

@vertex
fn vs_line(v: LIn) -> LOut {
    var o: LOut;
    o.clip = F.view_proj * vec4(v.pos, 1.0);
    o.col = v.col;
    return o;
}

@fragment
fn fs_line(i: LOut) -> @location(0) vec4<f32> {
    return i.col;
}

// sky: full-screen triangle
struct SOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> SOut {
    let p = vec2(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var o: SOut;
    o.clip = vec4(p, 1.0, 1.0);
    o.ndc = p;
    return o;
}

@fragment
fn fs_sky(i: SOut) -> @location(0) vec4<f32> {
    // reversed Z: ndc z 1 is the near plane, 0 the far plane
    let a = F.inv_view_proj * vec4(i.ndc, 1.0, 1.0);
    let b = F.inv_view_proj * vec4(i.ndc, 0.0, 1.0);
    let d = normalize(b.xyz / b.w - a.xyz / a.w);
    if (hdr()) {
        // the sun disc can pass the HDR target's range
        return vec4(min(sky(d) * SCN.v[0].x, vec3(30000.0)), 1.0);
    }
    return vec4(aces(sky(d) * F.sky_hor.w), 1.0);
}

// ground shadow: soft dark blobs on the ground under a model (no shadow map;
// rounded boxes from the model's bounds, see `GroundShadow`)
struct GIn {
    @location(0) pos: vec3<f32>,
    @location(1) box: vec4<f32>,  // xy place in the blob (m from its centre), zw its half size
    @location(2) k: vec2<f32>,    // softness (m), darkness
};
struct GOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) box: vec4<f32>,
    @location(1) k: vec2<f32>,
};

@vertex
fn vs_ground(v: GIn) -> GOut {
    var o: GOut;
    o.clip = F.view_proj * vec4(v.pos, 1.0);
    o.box = v.box;
    o.k = v.k;
    return o;
}

@fragment
fn fs_ground(i: GOut) -> @location(0) vec4<f32> {
    // signed distance to the rounded box, then a smooth step across its edge
    let r = min(i.box.z, i.box.w) * 0.6;
    let q = abs(i.box.xy) - (i.box.zw - vec2(r));
    let d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
    let t = clamp(0.5 - d / (2.0 * i.k.x), 0.0, 1.0);
    return vec4(0.0, 0.0, 0.0, i.k.y * t * t * (3.0 - 2.0 * t));
}

// map cells: many placements merged per area (see cells.rs). Base colour
// only, from a 64 px texture array; each vertex carries its placement's
// centre and distance range, outside which the placement is dropped.
@group(1) @binding(0) var t_cell: texture_2d_array<f32>;
// per placement: centre (cell-local), near; far, unused
@group(1) @binding(1) var<storage, read> CE: array<vec4<f32>>;

struct CIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) nrm: vec4<f32>,
    @location(3) base: vec4<f32>,
    @location(4) tint: vec4<f32>,
    @location(5) le: vec2<u32>,     // texture layer, placement
    @location(8) origin: vec4<f32>,
};

struct COut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) base: vec4<f32>,
    @location(4) tint: vec4<f32>,
    @location(5) @interpolate(flat) lf: vec2<u32>,     // texture layer, flags (alpha mode | class << 2)
};

@vertex
fn vs_cell(v: CIn) -> COut {
    return cell_vert(v, false);
}

// Coverage fallbacks: placements flagged in table slot 5 also draw nearer
// than their own range (no finer LOD replaces them); the pipeline's stencil
// test keeps them to pixels no real surface covers.
@vertex
fn vs_cell_fb(v: CIn) -> COut {
    return cell_vert(v, true);
}

fn cell_vert(v: CIn, fb: bool) -> COut {
    var o: COut;
    let w = v.pos + v.origin.xyz;
    let e0 = CE[v.le.y * 2u];
    let e1 = CE[v.le.y * 2u + 1u];
    let d = distance(e0.xyz + v.origin.xyz, F.cam_pos.xyz);
    let k = F.misc3.y;
    var cull = d < e0.w * k || d >= e1.x * k;
    if (fb) {
        cull = e1.y < 0.5 || d >= min(e0.w, e1.x) * k || d < e1.z * k;
    }
    if (cull) {
        // outside the placement's range: every vertex of it lands outside
        // the clip volume, so its triangles are culled
        o.clip = vec4(2.0, 2.0, 2.0, 1.0);
        return o;
    }
    o.clip = F.view_proj * vec4(w, 1.0);
    o.world = w;
    o.nrm = v.nrm.xyz;
    o.uv = v.uv;
    o.base = v.base;
    o.tint = v.tint;
    o.lf = vec2(v.le.x, u32(round(v.nrm.w * 127.0)));
    return o;
}

@fragment
fn fs_cell(i: COut, @builtin(front_facing) ff: bool) -> @location(0) vec4<f32> {
    let fp = footprint(i.world);
    let amode = i.lf.y & 3u;
    let cls = i.lf.y >> 2u;
    let base = i.base * textureSample(t_cell, samp, i.uv, i.lf.x);
    if (amode == 1u && base.a < 0.5) {
        discard;
    }
    var n = normalize(i.nrm + vec3(0.0, 1.0e-4, 0.0));
    if (!ff) { n = -n; }
    var albedo = base.rgb;
    if (F.misc3.x > 0.5) { albedo = albedo * i.tint.rgb; }
    var alpha = base.a;
    var rough = 0.75;
    var emissive = vec3(0.0);
    if (cls == 5u || cls == 6u) {
        rough = 0.04;
        alpha = min(alpha, 0.45);
        if (F.misc3.z > 0.5) {
            // dark reflective building glass (see fs_mesh)
            albedo = albedo * 0.12 + vec3(0.025, 0.03, 0.04);
            alpha = 0.9;
        }
    } else if (cls == 8u) {
        albedo = vec3(0.04, 0.12, 0.16); rough = 0.05; alpha = 0.7;
    } else if (cls == 7u) {
        emissive = albedo * F.misc.y;
    }
    albedo = albedo * (1.0 - 0.45 * F.misc.x);
    let v = normalize(F.cam_pos.xyz - i.world);
    let l = normalize(F.sun_dir.xyz);
    let h = normalize(v + l);
    let ndl = max(dot(n, l), 0.0);
    let ndv = max(dot(n, v), 0.001);
    let ndh = max(dot(n, h), 0.0);
    let a2 = rough * rough * rough * rough;
    let dd = ndh * ndh * (a2 - 1.0) + 1.0;
    let D = a2 / (3.14159 * dd * dd);
    let kk = (rough + 1.0) * (rough + 1.0) / 8.0;
    let G = (ndl / (ndl * (1.0 - kk) + kk)) * (ndv / (ndv * (1.0 - kk) + kk));
    let f0 = vec3(0.04);
    let Fr = f0 + (1.0 - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    let spec = D * G * Fr / max(4.0 * ndl * ndv, 0.001);
    let kd = 1.0 - Fr;
    let direct = (kd * albedo / 3.14159 + spec) * sun_e(i.world) * ndl;
    let amb_d = ambient(n);
    let fr_v = f0 + (max(vec3(1.0 - rough), f0) - f0) * pow(1.0 - ndv, 5.0);
    let env = environment(reflect(-v, n), rough) * F.misc.w;
    var col = direct + kd * albedo * amb_d + env * fr_v + emissive;
    if (hdr() && cls == 8u && F.mode.x == 0u) {
        col = water_shade(i.world, fp);
        alpha = select(0.92, 1.0, amode != 2u);
    }
    let mode = F.mode.x;
    if (mode == 1u) { col = albedo; }
    else if (mode == 2u) { return vec4(n * 0.5 + 0.5, 1.0); }
    if (hdr() && mode == 0u) {
        col = map_finish(col, i.world);
    } else if (!hdr()) {
        let dist = length(F.cam_pos.xyz - i.world);
        let fog = 1.0 - exp(-dist * F.misc2.x);
        col = mix(col, neutral(sky(-v), 0.5) * 0.9, fog);
        if (mode != 1u) {
            col = aces(col * F.sky_hor.w);
        }
    }
    if (amode != 2u && cls != 5u && cls != 6u && cls != 8u) { alpha = 1.0; }
    return vec4(col, alpha);
}

@fragment
fn fs_shadow_cell(i: COut) {
    let a = i.base.a * textureSample(t_cell, samp, i.uv, i.lf.x).a;
    if ((i.lf.y & 3u) == 1u && a < 0.5) {
        discard;
    }
}
