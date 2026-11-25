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
    jitter: vec4<f32>,    // body paint's coat: x clear coat intensity, y clear coat roughness, z flake orientation (0 = no flakes); w vehicle wear (0 to 1)
    frame: vec4<f32>,     // x strand dither index, y, z the target's width and height in pixels (1e9 in the sun shadow pass, x then the strand widening factor)
    light_vp: mat4x4<f32>, // the sun shadow map's view-projection (all zero: none)
};

struct Mat {
    base: vec4<f32>,
    flags: vec4<u32>,     // x has-bits (1 base 2 normal 4 packed 8 emissive), y cls, z alpha (0 opaque 1 mask 2 blend), w gap in bits 0-1 (0 ok 1 low 2 flat), bit 8 = stand-in surface with metallic in bits 16-23 and roughness in bits 24-31, bit 9 = base texture is a tint mask, bit 12 = an emissive shader of another class (lit by its emissive map), bit 13 = the packed map is a vehicle material map, bit 14 = the emissive slot holds the part ramp image with the parts' surfaces
    tile: vec4<f32>,      // fabric detail tile: x UV scale, y rotation, z mean grey value, w bound
    emis: vec4<f32>,      // x emissive scale of the material (from the GLB's emissive strength)
};

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
// the game's hair lighting tables (hairlight): lobes along the strand,
// lobes around it, reflectance and path length, and the colour factor
@group(0) @binding(6) var t_hair_m: texture_3d<f32>;
@group(0) @binding(7) var t_hair_n: texture_3d<f32>;
@group(0) @binding(8) var t_hair_a: texture_2d<f32>;
@group(0) @binding(9) var t_hair_p: texture_3d<f32>;
@group(0) @binding(10) var t_shadow: texture_depth_2d;
@group(0) @binding(11) var samp_shadow: sampler_comparison;
@group(0) @binding(12) var t_cam_depth: texture_depth_2d;

@group(1) @binding(0) var<uniform> M: Mat;
@group(1) @binding(1) var t_base: texture_2d<f32>;
@group(1) @binding(2) var t_nrm: texture_2d<f32>;
@group(1) @binding(3) var t_pk: texture_2d<f32>;
@group(1) @binding(4) var t_em: texture_2d<f32>;
@group(1) @binding(5) var t_tile: texture_2d<f32>;

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
    var wp = w.xyz;
    var edge = v.tan.w;
    var uv1 = v.uv1;
    // a strand ribbon stored closed (|tan.w| = 2 + its half width in mm): opened towards the eye as the
    // game does, to at least its minimum drawn width, with the share of
    // that width the strand really covers kept for the pixel shader (uv1.x) when it is lit as a standard surface
    if (abs(v.tan.w) >= 2.0) {
        let code = abs(v.tan.w) - 2.0;
        let ti = floor(code / 16.0);
        let r = (code - 16.0 * ti) * 0.001;
        // the game's minimum drawn width: 2 pixels at the strand's distance, capped at 10 mm (read from its
        // strand vertex program by the shader archive work); ti, the piece type, is kept for the strand count rule
        let npx = 2.0;
        edge = sign(v.tan.w);
        let td = normalize((m * vec4(v.tan.xyz, 0.0)).xyz);
        var side = cross(td, normalize(F.cam_pos.xyz - w.xyz));
        side = side / max(length(side), 1e-5);
        let c0 = F.view_proj * w;
        let c1 = F.view_proj * vec4(w.xyz + side, 1.0);
        let ppm = length((c1.xy / c1.w - c0.xy / c0.w) * 0.5 * F.frame.yz);
        let rmin = min(0.5 * npx / max(ppm, 1e-3), 0.005);
        // (a strand lit by the hair lighting keeps its profile in uv1.x: there the share rides on uv1.y, 1 .. 1.4)
        if (uv1.y > 1.5) { uv1.x = clamp(r / rmin, 0.0, 1.0); } else if (uv1.y > 0.5) { uv1.y = 1.0 + 0.4 * clamp(r / rmin, 0.0, 1.0); }
        wp = w.xyz + side * edge * max(r, rmin);
        // the sun shadow pass (no target size) with a widening factor: the ribbon of the game's ray-traced passes,
        // flat, turned by a random direction per strand (here a hash of the strand's root normal and the
        // accumulation frame in F.frame.w, so a many-frame picture averages the turns the way the game's frames
        // do; the game's own hash is not read) and the factor times its own width
        if (F.frame.y > 1.0e8 && F.frame.x >= 1.0) {
            let hn = v.nrm + vec3(F.frame.w * 0.618034, F.frame.w * 0.414214, F.frame.w * 0.732051);
            let hsh = fract(sin(vec3(dot(hn, vec3(12.9898, 78.233, 37.719)), dot(hn, vec3(39.346, 11.135, 83.155)), dot(hn, vec3(73.156, 52.235, 9.151)))) * 43758.5453) * 2.0 - 1.0;
            var rs = cross(td, normalize(hsh + vec3(1.0e-4, 0.0, 0.0)));
            rs = rs / max(length(rs), 1e-5);
            wp = w.xyz + rs * edge * r * F.frame.x;
        }
    }
    o.clip = F.view_proj * vec4(wp, 1.0);
    o.world = wp;
    o.nrm = (m * vec4(v.nrm, 0.0)).xyz;
    o.tan = vec4((m * vec4(v.tan.xyz, 0.0)).xyz, edge);
    o.uv0 = v.uv0;
    o.uv1 = uv1;
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
    let s = max(dot(d, normalize(F.sun_dir.xyz)), 0.0);
    let disc = smoothstep(0.99972, 0.99984, s) * smoothstep(-0.006, 0.006, d.y);
    return environment(d, 0.0) + F.sun_col.rgb * (disc * 40.0 * F.sun_dir.w);
}

// Diffuse light from the surroundings on a surface facing n: the sky,
// mostly desaturated (its full colour tinted every shaded surface
// blue-white), and on surfaces that face down the light the ground sends
// back.
fn ambient(n: vec3<f32>) -> vec3<f32> {
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


// Hair and fur strands (material class 12): a three-lobe fibre model.
// R, TT and TRT lobes with a Gaussian along the strand and the published
// fits around it (Karis, "Physically Based Hair Shading in Unreal"), plus
// its multiple-scattering term. What is taken from the game's own hair
// lighting program: the index of refraction (F0 = 0.046461) and the
// absorption of the base colour, sigma = 0.0478501 ln(colour)^2 per
// channel. The game weights its lobes with four lookup tables; those are
// not used here yet, so the lobe shapes are the published fits.
fn hair_g(b: f32, theta: f32) -> f32 {
    return exp(-0.5 * theta * theta / (b * b)) / (2.5066283 * b);
}

fn hair_f(c: f32) -> f32 {
    let f0 = 0.046461;
    return f0 + (1.0 - f0) * pow(1.0 - c, 5.0);
}

// Transmittance of the fibre for a path of `p` fibre widths.
fn hair_t(colour: vec3<f32>, p: f32) -> vec3<f32> {
    let lc = log(max(colour, vec3(0.0001)));
    return exp(-0.0478501 * lc * lc * p);
}

fn hair_light(colour: vec3<f32>, t: vec3<f32>, v: vec3<f32>, l: vec3<f32>, rough: f32, scatter: f32) -> vec3<f32> {
    let vol = dot(v, l);
    let sin_l = clamp(dot(t, l), -1.0, 1.0);
    let sin_v = clamp(dot(t, v), -1.0, 1.0);
    let cos_d = cos(0.5 * abs(asin(sin_v) - asin(sin_l)));
    let lp = l - sin_l * t;
    let vp = v - sin_v * t;
    let cos_phi = dot(lp, vp) * inverseSqrt(dot(lp, lp) * dot(vp, vp) + 0.0001);
    let cos_half = sqrt(clamp(0.5 + 0.5 * cos_phi, 0.0, 1.0));
    let n_p = 1.19 / cos_d + 0.36 * cos_d;
    let shift = 0.035;
    let r2 = rough * rough;
    var s = vec3(0.0);
    // R: light reflected off the surface of the fibre
    let m_r = hair_g(r2 * 1.4142135 * cos_half + 0.02, sin_l + sin_v + 2.0 * shift);
    s += vec3(m_r * 0.25 * cos_half * hair_f(sqrt(clamp(0.5 + 0.5 * vol, 0.0, 1.0))));
    // TT: light passing through the fibre
    let a = 1.0 / n_p;
    let h = cos_half * (1.0 + a * (0.6 - 0.8 * cos_phi));
    let f_tt = hair_f(cos_d * sqrt(clamp(1.0 - h * h, 0.0, 1.0)));
    let m_tt = hair_g(r2 * 0.5 + 0.02, sin_l + sin_v - shift);
    let t_tt = hair_t(colour, 0.5 * sqrt(max(1.0 - h * h * a * a, 0.0)) / cos_d * 2.0);
    s += m_tt * exp(-3.65 * cos_phi - 3.98) * (1.0 - f_tt) * (1.0 - f_tt) * t_tt;
    // TRT: light reflected once inside the fibre
    let f_trt = hair_f(cos_d * 0.5);
    let m_trt = hair_g(r2 * 2.0 + 0.02, sin_l + sin_v - 4.0 * shift);
    let t_trt = hair_t(colour, 0.8 / cos_d * 2.0);
    s += m_trt * exp(17.0 * cos_phi - 16.78) * (1.0 - f_trt) * (1.0 - f_trt) * f_trt * t_trt;
    // many fibres: light scattered between strands
    let kajiya = 1.0 - abs(sin_l);
    let fake_n = normalize(v - t * sin_v);
    let ndl = clamp((dot(fake_n, l) + 1.0) * 0.25, 0.0, 1.0);
    // (in full light the published model adds no tint to this term)
    s += sqrt(colour) * (mix(ndl, kajiya, 0.33) * scatter / 3.14159);
    return max(s, vec3(0.0));
}

// The light sum of the game's hair lighting program for one light,
// transcribed from the compiled code (block 02564-027a8 of
// TL_PS_PBR_MATERIAL_HAIR; read once by hand, medium confidence), with its
// four lookup tables. Returns the light leaving towards the viewer per unit
// of light colour. `t` strand direction, `n` the tube normal (perpendicular
// to the strand), `v` to the eye, `l` to the light, `g0` and `y` the two
// per-material values the game bakes (not decoded: fixed here).
fn hair_game(colour: vec3<f32>, t: vec3<f32>, n: vec3<f32>, root: vec3<f32>, v: vec3<f32>, l: vec3<f32>, g0: f32, y: f32, e: f32) -> vec3<f32> {
    let c = normalize(cross(n, t));
    // (the view vector here points to the eye; the tables put the surface
    // reflection where light and eye are on the same side of the strand)
    let sin_o = clamp(dot(v, t), -1.0, 1.0);
    let sin_i = clamp(dot(l, t), -1.0, 1.0);
    let ln = dot(l, n);
    let lc = dot(l, c);
    let vn = dot(v, n);
    let vc = dot(v, c);
    let cos_phi = clamp((ln * vn + lc * vc) / sqrt(max((vn * vn + vc * vc) * (ln * ln + lc * lc), 0.000001)), -1.0, 1.0);
    let h = clamp(vc / sqrt(max(0.000001, 1.0 - sin_o * sin_o)), -1.0, 1.0);
    // reflectance and path length; absorption of the colour
    let a = textureSampleLevel(t_hair_a, samp_clamp, vec2(clamp(sin_o * sin_o, 0.0, 1.0), 0.5), 0.0); // between its two rows: their mean
    let f = a.r;
    let lcol = log(max(colour, vec3(0.0000001)));
    let tt0 = exp(-0.0478501 * lcol * lcol * a.a);
    let tt = (1.0 - f) * (1.0 - f) * tt0;
    let trt = (f * tt0) * tt;
    let res = (f * tt0) * trt / max(vec3(0.0001), vec3(1.0) - f * tt0);
    let pz = 1.0 - g0;
    let pc = vec3(
        textureSampleLevel(t_hair_p, samp_clamp, vec3(colour.r, y, pz), 0.0).r,
        textureSampleLevel(t_hair_p, samp_clamp, vec3(colour.g, y, pz), 0.0).r,
        textureSampleLevel(t_hair_p, samp_clamp, vec3(colour.b, y, pz), 0.0).r);
    // roughness slice: z0 = k g0 + (1 - g0) with k the light's source term;
    // the table is flat at z = 1, so a distant light takes k = 0
    let z0 = 1.0 - g0;
    let zb = 0.375 + 0.25 * z0;
    let zc = 0.25 + 0.5 * z0;
    let muv = vec2(0.5 + 0.5 * sin_i, 0.5 + 0.5 * sin_o);
    let m0 = textureSampleLevel(t_hair_m, samp_clamp, vec3(muv, z0), 0.0);
    let mb = textureSampleLevel(t_hair_m, samp_clamp, vec3(muv, zb), 0.0);
    let mc = textureSampleLevel(t_hair_m, samp_clamp, vec3(muv, zc), 0.0);
    let tphi = 0.5 + 0.5 * cos_phi;
    let nx = clamp(abs(sin_o) * abs(sin_o) * abs(sin_o), 0.0, 1.0);
    let n1 = textureSampleLevel(t_hair_n, samp_clamp, vec3(nx, 0.515873 + 0.484127 * tphi, 0.5 + 0.5 * h), 0.0).rgb;
    let n2 = textureSampleLevel(t_hair_n, samp_clamp, vec3(nx, 0.484127 * tphi, 0.5 + 0.5 * h), 0.0).rgb;
    let cos_i = sqrt(clamp(1.0 - sin_i * sin_i, 0.0, 1.0));
    let nr = mix(n1.r, cos_i / 3.14159265, 0.333);
    let ntrt = mix(n1.b, cos_i, 0.25);
    let nr2 = mix(n2.r, nr, 0.333);
    let ng2 = mix(n2.g, n1.g, 0.333);
    let nb2 = mix(n2.b, ntrt, 0.25);
    let q = tt * y;
    let inv2pi = 0.15915494;
    let s0 = tt * ng2 * m0.g + trt * nb2 * m0.b + res * m0.a * inv2pi;
    let sc = vec3(f * nr * mc.r) + tt * n1.g * mc.g + trt * ntrt * mc.b + res * mc.a * inv2pi;
    let sb = q * q * cos_i * inv2pi + (vec3(f * mb.r) + tt * mb.g + trt * mb.b + res * mb.a) * inv2pi;
    // the program divides by the colour here and multiplies it back later
    let coloured = pc * (s0 + q * sc + q * q * sb);
    // the light's size factor a / (a + 0.25 s (3 (0.1 + m) + s)) scales both
    // parts; s is the source size from the light's record, near 0 for the
    // sun, so the factor is 1
    let area = 1.0;
    // visibility: the side of the head the light is on (the root normal)
    // times the azimuth term sat(1 + cos phi), and a factor 1.5 - 0.5 e:
    // W = ((1 - e) D A + e)(1.5 - 0.5 e) (read once from the program, medium
    // confidence). The fill e of the unlit side is a stored alpha times a
    // fade by depth and light (run-time values, not read); here the
    // parameter ramp's blue value with the fade taken as 1
    let w = (clamp(dot(root, l), 0.0, 1.0) * clamp(1.0 + cos_phi, 0.0, 1.0) * (1.0 - e) + e) * (1.5 - 0.5 * e);
    return max(w * (coloured * colour / max(colour, vec3(0.000001)) + vec3(f * m0.r * nr2 * area)), vec3(0.0));
}

// The ambient part of the game's hair lighting (read once from the same
// program, medium confidence): ambient light x base colour x this factor.
// `o` (0..1) comes from the strand's profile value and a depth the program
// reads; 1 is the outer hair. The environment reflection it also subtracts
// and adds is left out (small).
fn hair_ambient(t: vec3<f32>, v: vec3<f32>, g0: f32, o: f32) -> f32 {
    let sin_o = clamp(dot(v, t), -1.0, 1.0);
    let cos_o = sqrt(max(1.0 - sin_o * sin_o, 0.0));
    let f = textureSampleLevel(t_hair_a, samp_clamp, vec2(clamp(sin_o * sin_o, 0.0, 1.0), 0.5), 0.0).r;
    return (0.31831 + 0.68169 * exp(-7.21348 * g0 * (1.0 - 0.5 * ((1.0 - f) + cos_o)))) * (0.195 + 0.91 * o);
}

// The game's environment table (generated by the game at start-up, as read
// by the shader archive work; recomputed here from its definition at 16 x 16
// texel centres): x = N.V, y = perceptual roughness; r = the part weighted
// by (F90 - F0), g = the total reflectance (scale + bias).
const ENV_N: i32 = 16;
const ENV_TAB: array<vec2<f32>, 256> = array<vec2<f32>, 256>(
    vec2(0.8523, 0.9995), vec2(0.6112, 1.0000), vec2(0.4276, 1.0000), vec2(0.2910, 1.0000), vec2(0.1918, 1.0000), vec2(0.1217, 1.0000), vec2(0.0738, 1.0000), vec2(0.0423, 1.0000), vec2(0.0226, 1.0000), vec2(0.0111, 1.0000), vec2(0.0048, 1.0000), vec2(0.0018, 1.0000), vec2(0.0005, 1.0000), vec2(0.0001, 1.0000), vec2(0.0000, 1.0000), vec2(0.0000, 1.0000),
    vec2(0.7874, 0.9554), vec2(0.6006, 0.9946), vec2(0.4239, 0.9981), vec2(0.2898, 0.9995), vec2(0.1913, 0.9995), vec2(0.1215, 0.9995), vec2(0.0739, 1.0000), vec2(0.0425, 1.0000), vec2(0.0228, 1.0000), vec2(0.0112, 1.0000), vec2(0.0049, 1.0000), vec2(0.0018, 1.0000), vec2(0.0005, 1.0000), vec2(0.0001, 1.0000), vec2(0.0000, 1.0000), vec2(0.0000, 1.0000),
    vec2(0.6385, 0.8877), vec2(0.5464, 0.9605), vec2(0.4035, 0.9840), vec2(0.2820, 0.9932), vec2(0.1885, 0.9961), vec2(0.1206, 0.9967), vec2(0.0739, 0.9977), vec2(0.0430, 0.9992), vec2(0.0233, 0.9995), vec2(0.0115, 0.9995), vec2(0.0051, 0.9995), vec2(0.0019, 0.9995), vec2(0.0006, 0.9995), vec2(0.0001, 0.9994), vec2(0.0000, 1.0000), vec2(0.0000, 1.0000),
    vec2(0.5192, 0.8964), vec2(0.4482, 0.9074), vec2(0.3549, 0.9482), vec2(0.2589, 0.9704), vec2(0.1784, 0.9815), vec2(0.1169, 0.9870), vec2(0.0734, 0.9916), vec2(0.0434, 0.9938), vec2(0.0241, 0.9959), vec2(0.0123, 0.9964), vec2(0.0056, 0.9970), vec2(0.0023, 0.9975), vec2(0.0008, 0.9981), vec2(0.0002, 0.9991), vec2(0.0000, 0.9987), vec2(0.0000, 0.9979),
    vec2(0.4148, 0.9194), vec2(0.3520, 0.8836), vec2(0.2877, 0.9062), vec2(0.2201, 0.9328), vec2(0.1584, 0.9524), vec2(0.1081, 0.9660), vec2(0.0697, 0.9741), vec2(0.0426, 0.9802), vec2(0.0244, 0.9839), vec2(0.0130, 0.9862), vec2(0.0063, 0.9879), vec2(0.0027, 0.9873), vec2(0.0010, 0.9897), vec2(0.0003, 0.9911), vec2(0.0001, 0.9923), vec2(0.0000, 0.9929),
    vec2(0.3255, 0.9367), vec2(0.2733, 0.8865), vec2(0.2245, 0.8835), vec2(0.1766, 0.8992), vec2(0.1321, 0.9181), vec2(0.0937, 0.9344), vec2(0.0630, 0.9462), vec2(0.0403, 0.9541), vec2(0.0242, 0.9599), vec2(0.0135, 0.9663), vec2(0.0070, 0.9718), vec2(0.0033, 0.9757), vec2(0.0013, 0.9774), vec2(0.0005, 0.9802), vec2(0.0001, 0.9818), vec2(0.0000, 0.9830),
    vec2(0.2532, 0.9474), vec2(0.2112, 0.8970), vec2(0.1727, 0.8770), vec2(0.1371, 0.8760), vec2(0.1052, 0.8845), vec2(0.0771, 0.8927), vec2(0.0539, 0.9042), vec2(0.0359, 0.9175), vec2(0.0226, 0.9281), vec2(0.0132, 0.9350), vec2(0.0073, 0.9425), vec2(0.0037, 0.9479), vec2(0.0016, 0.9533), vec2(0.0006, 0.9575), vec2(0.0002, 0.9624), vec2(0.0000, 0.9643),
    vec2(0.1969, 0.9531), vec2(0.1627, 0.9039), vec2(0.1323, 0.8734), vec2(0.1052, 0.8564), vec2(0.0814, 0.8468), vec2(0.0608, 0.8539), vec2(0.0438, 0.8628), vec2(0.0301, 0.8718), vec2(0.0197, 0.8831), vec2(0.0123, 0.8936), vec2(0.0071, 0.9036), vec2(0.0038, 0.9119), vec2(0.0019, 0.9192), vec2(0.0008, 0.9251), vec2(0.0003, 0.9303), vec2(0.0001, 0.9350),
    vec2(0.1538, 0.9548), vec2(0.1257, 0.9040), vec2(0.1013, 0.8636), vec2(0.0802, 0.8312), vec2(0.0621, 0.8227), vec2(0.0470, 0.8192), vec2(0.0344, 0.8234), vec2(0.0243, 0.8300), vec2(0.0165, 0.8382), vec2(0.0107, 0.8461), vec2(0.0065, 0.8546), vec2(0.0037, 0.8614), vec2(0.0019, 0.8686), vec2(0.0009, 0.8766), vec2(0.0003, 0.8830), vec2(0.0001, 0.8895),
    vec2(0.1212, 0.9533), vec2(0.0977, 0.8966), vec2(0.0777, 0.8456), vec2(0.0610, 0.8151), vec2(0.0472, 0.7979), vec2(0.0360, 0.7907), vec2(0.0266, 0.7875), vec2(0.0192, 0.7864), vec2(0.0133, 0.7891), vec2(0.0088, 0.7921), vec2(0.0056, 0.7965), vec2(0.0033, 0.8032), vec2(0.0018, 0.8096), vec2(0.0009, 0.8161), vec2(0.0004, 0.8223), vec2(0.0001, 0.8291),
    vec2(0.0968, 0.9495), vec2(0.0766, 0.8830), vec2(0.0600, 0.8253), vec2(0.0468, 0.7947), vec2(0.0362, 0.7746), vec2(0.0275, 0.7598), vec2(0.0205, 0.7486), vec2(0.0148, 0.7417), vec2(0.0105, 0.7363), vec2(0.0071, 0.7353), vec2(0.0046, 0.7371), vec2(0.0028, 0.7383), vec2(0.0016, 0.7417), vec2(0.0008, 0.7456), vec2(0.0004, 0.7497), vec2(0.0001, 0.7547),
    vec2(0.0784, 0.9437), vec2(0.0605, 0.8640), vec2(0.0468, 0.8066), vec2(0.0363, 0.7724), vec2(0.0278, 0.7460), vec2(0.0211, 0.7244), vec2(0.0157, 0.7076), vec2(0.0114, 0.6930), vec2(0.0082, 0.6854), vec2(0.0056, 0.6782), vec2(0.0037, 0.6729), vec2(0.0023, 0.6696), vec2(0.0014, 0.6687), vec2(0.0007, 0.6685), vec2(0.0003, 0.6695), vec2(0.0001, 0.6708),
    vec2(0.0645, 0.9361), vec2(0.0485, 0.8407), vec2(0.0370, 0.7836), vec2(0.0283, 0.7447), vec2(0.0215, 0.7119), vec2(0.0162, 0.6853), vec2(0.0120, 0.6617), vec2(0.0088, 0.6459), vec2(0.0063, 0.6301), vec2(0.0043, 0.6179), vec2(0.0029, 0.6084), vec2(0.0019, 0.6008), vec2(0.0011, 0.5945), vec2(0.0006, 0.5895), vec2(0.0003, 0.5861), vec2(0.0001, 0.5835),
    vec2(0.0539, 0.9270), vec2(0.0394, 0.8180), vec2(0.0296, 0.7591), vec2(0.0223, 0.7134), vec2(0.0168, 0.6754), vec2(0.0125, 0.6421), vec2(0.0093, 0.6175), vec2(0.0068, 0.5937), vec2(0.0048, 0.5746), vec2(0.0034, 0.5590), vec2(0.0023, 0.5442), vec2(0.0015, 0.5317), vec2(0.0009, 0.5215), vec2(0.0005, 0.5121), vec2(0.0003, 0.5044), vec2(0.0001, 0.4975),
    vec2(0.0457, 0.9166), vec2(0.0324, 0.7979), vec2(0.0239, 0.7313), vec2(0.0178, 0.6790), vec2(0.0132, 0.6347), vec2(0.0098, 0.5997), vec2(0.0071, 0.5679), vec2(0.0052, 0.5417), vec2(0.0037, 0.5199), vec2(0.0026, 0.4989), vec2(0.0018, 0.4812), vec2(0.0012, 0.4658), vec2(0.0007, 0.4514), vec2(0.0004, 0.4391), vec2(0.0002, 0.4276), vec2(0.0001, 0.4174),
    vec2(0.0393, 0.9050), vec2(0.0269, 0.7754), vec2(0.0194, 0.7023), vec2(0.0142, 0.6430), vec2(0.0104, 0.5938), vec2(0.0076, 0.5540), vec2(0.0056, 0.5196), vec2(0.0040, 0.4909), vec2(0.0029, 0.4641), vec2(0.0020, 0.4416), vec2(0.0014, 0.4215), vec2(0.0009, 0.4031), vec2(0.0006, 0.3867), vec2(0.0003, 0.3719), vec2(0.0002, 0.3580), vec2(0.0001, 0.3458)
);

fn env_tab(nv: f32, y: f32) -> vec2<f32> {
    let n = f32(ENV_N);
    let px = clamp(nv * n - 0.5, 0.0, n - 1.0);
    let py = clamp(y * n - 0.5, 0.0, n - 1.0);
    let x0 = i32(floor(px));
    let y0 = i32(floor(py));
    let x1 = min(x0 + 1, ENV_N - 1);
    let y1 = min(y0 + 1, ENV_N - 1);
    let fx = px - f32(x0);
    let fy = py - f32(y0);
    let a = mix(ENV_TAB[y0 * ENV_N + x0], ENV_TAB[y0 * ENV_N + x1], fx);
    let b = mix(ENV_TAB[y1 * ENV_N + x0], ENV_TAB[y1 * ENV_N + x1], fx);
    return mix(a, b, fy);
}

struct MeshOut {
    @location(0) col: vec4<f32>,
    @builtin(sample_mask) mask: u32,
};

// interleaved gradient noise, as the game's strand visibility pass uses it
fn ign(p: vec2<f32>) -> f32 {
    return fract(52.9829 * fract(0.0671106 * p.x + 0.00583715 * p.y));
}

// Share of the sun's light that reaches a point, from the sun shadow map (1 where there is none): 3 x 3 taps
// of a linear comparison. A stand-in for the game's shadow filtering, which is not read.
// The camera depth pre-pass at a pixel (reversed: nearer is larger; 0: nothing drawn).
fn eye_depth(p: vec2<i32>) -> f32 {
    let lim = vec2<i32>(F.frame.yz) - 1;
    return textureLoad(t_cam_depth, clamp(p, vec2(0), lim), 0);
}

// The game's openness of a strand pixel (read from its hair lighting program
// by the shader archive work): over 16 neighbours at (dx, dy), dx and dy in
// -2, -1, 1, 2, the weighted share that is not in front of the pixel, by
// depth. 1: the nearest thing in its block of pixels; towards 0: seen
// through a gap.
fn strand_openness(clip: vec4<f32>) -> f32 {
    if (F.light_vp[3][3] == 0.0) { return 1.0; }
    let p = vec2<i32>(clip.xy);
    let z = clip.z * 1.0001 + 1.0e-7;
    var s = 0.0;
    for (var y = -2; y <= 2; y++) {
        for (var x = -2; x <= 2; x++) {
            if (x == 0 || y == 0) { continue; }
            let far = abs(x) + abs(y);
            let w = select(select(0.376603, 0.209611, far == 4), 0.676634, far == 2);
            if (eye_depth(p + vec2(x, y)) <= z) { s += w; }
        }
    }
    return clamp(0.15249 * s, 0.0, 1.0);
}

// Screen-space occlusion of the ambient light on an opaque surface, from the
// camera depth pre-pass: the game's horizon occlusion pass (read in full from
// its shader archive by the shader archive work). Two perpendicular slices
// through the pixel at a random turn; on each side of a slice two depth taps
// at (j/2)^2 R + 1.3 and ((1 + j)/2)^2 R + 1.3 pixels (j random, 0..1); R in pixels,
// 0.625 x the target height, held to 64 near the eye. A tap's horizon is
// blended towards the surface's own tangent plane by its depth difference in
// pixel units ((0.15 dz)^2, so a tap 6.7 pixels or more away in depth counts
// for nothing): the pass only sees fine contact occlusion. The larger
// horizon per side is clamped to the normal's hemisphere and integrated the
// usual way. The game's noise comes from a texture and temporal passes
// follow; here the noise is a hash and the viewer's frame accumulation
// averages it. The game's standard lighting programs (not its hair programs)
// use the result on the specular ambient only, through a cone test against
// the reflection vector; the viewer keeps no bent direction and uses the
// plain visibility in place of that test's result (an approximation), with
// the game's strength by roughness.
// The game's diffuse ambient is darkened by its traced probe lighting, which
// the viewer does not have.
fn screen_occlusion(clip: vec4<f32>, wp: vec3<f32>, n: vec3<f32>) -> f32 {
    if (F.light_vp[3][3] == 0.0) { return 1.0; }
    let size = F.frame.yz;
    let toeye = F.cam_pos.xyz - wp;
    let v = toeye / max(length(toeye), 1.0e-6);
    let c0 = F.view_proj * vec4(wp, 1.0);
    let z = max(c0.w, 1.0e-4);
    // pixels a metre covers at the pixel's depth: 0.5 H / tan(fov_y / 2) / z
    var up = cross(v, cross(vec3(0.0, 1.0, 0.0), v));
    up = up / max(length(up), 1.0e-5);
    let c1 = F.view_proj * vec4(wp + up * 0.01, 1.0);
    let ppm = length((c1.xy / c1.w - c0.xy / c0.w) * 0.5 * size) * 100.0;
    let r0 = 0.625 * size.y;
    let th = tanh(min(0.02 * z, 10.0));
    let rad = mix(clamp(r0, 1.0, 64.0), r0, th * th);
    let phi = ign(clip.xy + vec2(F.frame.x * 7.31, F.frame.x * 3.17)) * 1.5708;
    let jit = ign(clip.xy * 1.37 + vec2(F.frame.x * 2.11, 17.0));
    let ndc0 = c0.xy / c0.w;
    var sum = 0.0;
    for (var sl = 0; sl < 2; sl++) {
        let ang = phi + f32(sl) * 1.5708;
        let dir = vec2(cos(ang), sin(ang));
        // the slice's direction in the world, at right angles to the view
        let w1 = F.inv_view_proj * vec4(ndc0 + vec2(dir.x, -dir.y) * 0.01, c0.z / c0.w, 1.0);
        var t = w1.xyz / w1.w - wp;
        t = t - v * dot(t, v);
        t = t / max(length(t), 1.0e-9);
        let axis = cross(t, v);
        let pn = n - axis * dot(n, axis);
        let pl = length(pn);
        let na = select(-1.0, 1.0, dot(pn, t) >= 0.0) * acos(clamp(dot(pn, v) / max(pl, 1.0e-5), -1.0, 1.0));
        var hc = vec2(-1.0, -1.0);
        for (var sd = 0; sd < 2; sd++) {
            let sg = f32(sd) * 2.0 - 1.0;
            // the tangent plane's own horizon on this side
            let flat = -sg * sin(na);
            var best = -1.0;
            for (var k = 0; k < 2; k++) {
                let d = 0.25 * (f32(k) + jit) * (f32(k) + jit) * rad + 1.3;
                let q = floor(clip.xy) + round(dir * sg * d) + 0.5;
                var h = flat;
                if (q.x >= 0.0 && q.y >= 0.0 && q.x < size.x && q.y < size.y) {
                    let dep = eye_depth(vec2<i32>(q));
                    if (dep > 0.0) {
                        let w4 = F.inv_view_proj * vec4(q.x / size.x * 2.0 - 1.0, 1.0 - q.y / size.y * 2.0, dep, 1.0);
                        let pt = w4.xyz / w4.w;
                        let dv = pt - wp;
                        let zt = (F.view_proj * vec4(pt, 1.0)).w;
                        let dz = abs(zt - z) * ppm;
                        let w = clamp(0.0225 * dz * dz, 0.0, 1.0);
                        h = mix(dot(dv, v) / max(length(dv), 1.0e-6), flat, w);
                    }
                }
                best = max(best, h);
            }
            if (sd == 0) { hc.x = best; } else { hc.y = best; }
        }
        let h0 = na + max(-acos(clamp(hc.x, -1.0, 1.0)) - na, -1.5708);
        let h1 = na + min(acos(clamp(hc.y, -1.0, 1.0)) - na, 1.5708);
        let lw = pl + 0.15 * (1.0 - pl);
        sum += lw * ((cos(na) + 2.0 * h0 * sin(na) - cos(2.0 * h0 - na)) + (cos(na) + 2.0 * h1 * sin(na) - cos(2.0 * h1 - na)));
    }
    return clamp(max(0.01, 0.125 * sum), 0.0, 1.0);
}

// One tap of the sun shadow map at a world position (1 outside the map).
fn sun_shadow_tap(wp: vec3<f32>, r: i32) -> f32 {
    let c = F.light_vp * vec4(wp, 1.0);
    let uv = c.xy * vec2(0.5, -0.5) + 0.5;
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || c.z > 1.0) { return 1.0; }
    let px = 1.0 / f32(textureDimensions(t_shadow).x);
    var s = 0.0;
    for (var y = -r; y <= r; y++) {
        for (var x = -r; x <= r; x++) {
            s += textureSampleCompareLevel(t_shadow, samp_shadow, uv + vec2(f32(x), f32(y)) * px, c.z - 0.0012);
        }
    }
    let n = f32(2 * r + 1);
    return s / (n * n);
}

// The sun's visibility at a pixel. The game's lighting programs average its
// screen-space shadow buffer over 13 pixels (read from its hair and standard
// lighting programs by the shader archive work): the pixel, its four
// diagonal neighbours, and the pixels 2 and 4 to each side and above and
// below, a neighbour counting only when its depth is within 0.275 % of the
// pixel's. The viewer has no such buffer: each neighbour is taken on the
// pixel's own surface (the world position moved by its screen derivatives
// `dx`, `dy`), which is where a neighbour of nearly the same depth lies, and
// read from the shadow map. The 3 x 3 map filter of the centre stands in for
// whatever filters the game's buffer before this step (not read).
fn sun_shadow(wp: vec3<f32>, dx: vec3<f32>, dy: vec3<f32>) -> f32 {
    if (F.light_vp[3][3] == 0.0) { return 1.0; }
    var s = sun_shadow_tap(wp, 1);
    for (var k = 0; k < 4; k++) {
        let a = f32(k & 1) * 2.0 - 1.0;
        let b = f32(k >> 1u) * 2.0 - 1.0;
        s += sun_shadow_tap(wp + dx * a + dy * b, 0);
        // the cross: 2 and 4 pixels along x (k = 0, 1) and along y (k = 2, 3)
        let d = select(dy, dx, k < 2) * a;
        s += sun_shadow_tap(wp + d * 2.0, 0) + sun_shadow_tap(wp + d * 4.0, 0);
    }
    return s / 13.0;
}

@fragment
fn fs_mesh(i: VOut, @builtin(front_facing) ff: bool) -> MeshOut {
    var o: MeshOut;
    o.col = mesh_colour(i, ff);
    o.mask = 0xffffffffu;
    // a strand thinner than its drawn width: the game keeps a pixel with a probability equal to the
    // covered share (a dither it averages over frames); here each sample of the pixel is kept or not
    let cov = select(clamp((i.uv1.y - 1.0) * 2.5, 0.0, 1.0), i.uv1.x, i.uv1.y > 1.5);
    if (M.flags.y == 12u && i.uv1.y > 0.5 && cov < 0.999) {
        let j = fract(dot(i.tint.rgb, vec3(12.9898, 78.233, 37.719))) * 64.0 + F.frame.x * 5.588238;
        var mk = 0u;
        for (var s = 0u; s < 4u; s++) {
            if (cov >= ign(i.clip.xy + vec2(j + f32(s) * 5.588, j + f32(s) * 9.123))) { mk |= 1u << s; }
        }
        if (mk == 0u) { discard; }
        o.mask = mk;
    }
    return o;
}

fn mesh_colour(i: VOut, ff: bool) -> vec4<f32> {
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
        albedo = base.rgb * textureSampleLevel(t_em, samp, vec2(clamp(g, 0.004, 0.996), (id + 0.5) / 768.0), 0.0).rgb;
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
    // wear mask), by the amount the user sets.
    if (M.tile.w > 1.5 && F.jitter.w > 0.0) {
        let d = textureSample(t_tile, samp, i.uv0);
        let id = floor(i.col.a * 255.0 + 0.5);
        let u = clamp(d.g, 0.004, 0.996);
        let w1 = textureSampleLevel(t_em, samp, vec2(u, (id + 256.5) / 768.0), 0.0);
        let m1 = textureSampleLevel(t_em, samp, vec2(u, (id + 0.5) / 768.0), 0.0).a;
        let k = clamp(d.b * F.jitter.w * 2.0, 0.0, 1.0);
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
        emissive = e * F.misc.y * M.emis.x;
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
        let p = i.world / (0.0006 + 0.02 * F.sky_top.w);
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
    let D = a2 / (3.14159 * dd * dd);
    let k = (rough + 1.0) * (rough + 1.0) / 8.0;
    let G = (ndl / (ndl * (1.0 - k) + k)) * (ndv / (ndv * (1.0 - k) + k));
    let f0 = mix(vec3(0.04), albedo, metal);
    let Fr = f0 + (1.0 - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    let spec = D * G * Fr / max(4.0 * ndl * ndv, 0.001);
    let kd = (1.0 - Fr) * (1.0 - metal);
    let direct = (kd * albedo / 3.14159 + spec) * F.sun_col.rgb * F.sun_dir.w * ndl * 3.0;
    let amb_d = ambient(n);
    let fr_v = f0 + (max(vec3(1.0 - rough), f0) - f0) * pow(1.0 - ndv, 5.0);
    let r = reflect(-v, n);
    let env = environment(r, rough) * F.misc.w;
    let sh = sun_shadow(i.world, dpdx(i.world), dpdy(i.world));
    var ao_s = ao;
    // strength = max(0, I - 2 x roughness), I = 1 (the game's value in every weather and hour): no effect from
    // roughness 0.5 upward, so next to none on skin
    let occ_k = max(0.0, 1.0 - 2.0 * rough);
    if (cls != 12u && occ_k > 0.0) { ao_s = ao * (1.0 + occ_k * (screen_occlusion(i.clip, i.world, n) - 1.0)); }
    // the game applies its horizon occlusion to the specular ambient only (never to diffuse ambient, never to hair)
    var col = direct * sh * ao_direct + kd * albedo * amb_d * ao + env * fr_v * ao_s + emissive;
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
            + coat.x * (cspec * F.sun_col.rgb * F.sun_dir.w * ndl * 3.0 * sh * ao_direct + environment(r, cr) * F.misc.w * cfv * ao_s);
    }
    col += pearl * pow(1.0 - ndv, 2.0) * 0.6;
    if (cls == 12u) {
        // strands: the fibre model on the strand's direction (the tangent)
        let tdir = normalize(i.tan.xyz);
        let sun = F.sun_col.rgb * F.sun_dir.w * 3.0;
        let amb_h = sqrt(albedo) * 0.5 * (neutral(F.sky_top.rgb, 0.8) + neutral(F.sky_hor.rgb, 0.8) * 0.6) * 0.5 * F.misc.w;
        col = hair_light(albedo, tdir, v, l, 0.33, 0.5) * sun * sh * ao_direct + amb_h * ao;
        // with the game's tables: its own light sum
        let dims = textureDimensions(t_hair_m);
        if (dims.x > 1u) {
            // the tube normal as the game builds it: a side vector S across the
            // strand as seen from the eye, and the normal rounded across the
            // ribbon by the position u (-1 .. 1, the tangent's w) between its edges
            let side = normalize(cross(tdir, v));
            let u_h = clamp(i.tan.w, -1.0, 1.0);
            let tube = normalize(side * u_h + cross(side, tdir) * sqrt(max(1.0 - u_h * u_h, 0.0)));
            // large-scale light and dark side from the head: the root normal
            let root = normalize(i.nrm);
            // the parameter ramp's values at this point of the strand, when the
            // ribbon carries them (uv1.y = 1); else fixed stand-ins
            let ramped = i.uv1.y > 0.5;
            let g0h = select(0.33, clamp(i.uv0.x, 0.0, 1.0), ramped);
            let eh = select(0.5, clamp(i.uv0.y, 0.0, 1.0), ramped);
            // profile4 = 15 x (ramp profile x the strand's own profile, taken as 1); y = 0.5 + 0.0266667 profile4
            let yh = select(0.6067, 0.5 + 0.4 * clamp(i.uv1.x, 0.0, 1.0), ramped);
            let direct_h = hair_game(albedo, tdir, tube, root, v, l, g0h, yh, eh) * sun;
            // ambient: the viewer's sky light at the root normal, with the game's factor
            let up_h = root.y * 0.5 + 0.5;
            let sky_h = mix(neutral(F.sky_hor.rgb, 0.8) * 0.6, neutral(F.sky_top.rgb, 0.8), up_h) * F.misc.w;
            // o = sqrt(profile4 / 15 x openness) (profile4: the ramp's profile value in 15 steps)
            let o_h = sqrt(select(1.0, round(15.0 * clamp(i.uv1.x, 0.0, 1.0)) / 15.0, ramped) * strand_openness(i.clip));
            // the reflection part of the game's hair ambient (read from its hair program, medium confidence):
            // its reflection buffer x Fenv x an energy term, Fenv = (0.3 + 1.4 o) x a roughness factor x the
            // environment table's Fresnel; the diffuse part keeps 1 - Fenv. Stand-ins here: the viewer's sky along
            // the reflection about the tube normal for the game's traced reflection buffer, and a reflectance
            // scale of 1.
            let m_h = max(0.001, (1.0 - g0h) * (1.0 - g0h));
            let nv_h = max(abs(dot(v, tube)), 0.0001);
            let rg_h = 1.0 - g0h;
            // the game reads its table at roughness 0.5 + 0.5 (1 - g0): the upper half of the axis
            let e_h = env_tab(nv_h, 0.5 + 0.5 * rg_h);
            let e_r = e_h.x;
            let e_g = max(e_h.y, 0.001);
            let f0_h = 0.046461;
            let fenv = clamp((0.3 + 1.4 * o_h) * mix(0.55 + 0.65 * m_h, 1.4 + 0.16 * m_h, m_h) * (e_r * (1.0 - f0_h) + e_g * f0_h), 0.0, 1.0);
            let refl_h = environment(reflect(-v, tube), clamp(rg_h, 0.0, 1.0)) * F.misc.w;
            let spec_g = refl_h * fenv * (1.0 + (1.0 / e_g - 1.0) * f0_h);
            let amb_g = sky_h * albedo * hair_ambient(tdir, v, g0h, o_h) * (1.0 - fenv) + spec_g;
            // the game multiplies its light sum by the light's shadow (1 here without the sun shadow map)
            col = direct_h * sh * ao_direct + amb_g * ao;
            // a set with UseStandardBsdf (uv1.y = 2 + reflectance scale; nearly all hair and fur): the game
            // lights the strand by its standard lighting, with the atlas colour as base colour, the ramp's
            // roughness, the tube normal and F0 = 0.0465 x the scale
            if (i.uv1.y > 1.5) {
                let rs = clamp(g0h, 0.05, 1.0);
                let f0s = vec3(0.0465 * clamp(i.uv1.y - 2.0, 0.5, 1.0));
                let nls = max(dot(tube, l), 0.0);
                let nvs = max(dot(tube, v), 0.001);
                let hs = normalize(v + l);
                let nhs = max(dot(tube, hs), 0.0);
                let a2s = rs * rs * rs * rs;
                let dds = nhs * nhs * (a2s - 1.0) + 1.0;
                let ks = (rs + 1.0) * (rs + 1.0) / 8.0;
                let gs = (nls / (nls * (1.0 - ks) + ks)) * (nvs / (nvs * (1.0 - ks) + ks));
                let frs = f0s + (1.0 - f0s) * pow(1.0 - max(dot(hs, v), 0.0), 5.0);
                let specs = a2s / (3.14159 * dds * dds) * gs * frs / max(4.0 * nls * nvs, 0.001);
                // stand-in for the shadow the rest of the hair casts: the side of the head turned from the light
                // (with the sun shadow map: its shadow instead)
                let shade = select(smoothstep(-0.35, 0.35, dot(root, l)), sh, F.light_vp[3][3] != 0.0);
                col = ((1.0 - frs) * albedo / 3.14159 + specs) * sun * nls * shade * ao_direct + sky_h * (albedo + f0s) * ao;
            }
        }
    }
    if (cls == 11u) {
        // baked map tiles (already hill-shaded): soft diffuse, no sky reflection
        col = albedo * (0.55 * F.sun_col.rgb * F.sun_dir.w * (0.35 + 0.65 * ndl) + 0.7 * amb_d);
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
        col = w / 3.14159 * F.sun_col.rgb * F.sun_dir.w * ndl * 3.0 + w * amb_d;
    }
    // map tint (instance extra.z): 1 = selected, 2.. = gap heat
    if (i.extra.y > 0.5) { col = mix(col, vec3(1.0, 0.55, 0.1), 0.45); }
    if (i.extra.z > 0.5) { col = mix(col, gap_tint(u32(i.extra.z + 0.5) - 1u), 0.6); }
    // livery editor: the chosen sticker's zones and its border
    if (st_hl > 0.0) { col = mix(col, vec3(0.1, 0.55, 1.0), st_hl); }
    if (st_edge > 0.0) { col = mix(col, vec3(1.0, 0.85, 0.1), 0.9); }
    // distance fog
    let dist = length(F.cam_pos.xyz - i.world);
    let fog = 1.0 - exp(-dist * F.misc2.x);
    col = mix(col, neutral(sky(-v), 0.5) * 0.9, fog);
    if (mode != 1u && mode != 3u && mode != 6u) {
        col = aces(col * F.sky_hor.w);
    }
    if (M.flags.z != 2u && cls != 5u && cls != 6u && cls != 8u) { alpha = 1.0; }
    return vec4(col, alpha);
}

@vertex
fn vs_mesh(v: VIn) -> VOut {
    return mesh_vertex(v);
}

// wireframe: same vertex path, flat colour, pulled towards the camera
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
    let direct = (kd * albedo / 3.14159 + spec) * F.sun_col.rgb * F.sun_dir.w * ndl * 3.0;
    let amb_d = ambient(n);
    let fr_v = f0 + (max(vec3(1.0 - rough), f0) - f0) * pow(1.0 - ndv, 5.0);
    let env = environment(reflect(-v, n), rough) * F.misc.w;
    var col = direct + kd * albedo * amb_d + env * fr_v + emissive;
    let mode = F.mode.x;
    if (mode == 1u) { col = albedo; }
    else if (mode == 2u) { return vec4(n * 0.5 + 0.5, 1.0); }
    let dist = length(F.cam_pos.xyz - i.world);
    let fog = 1.0 - exp(-dist * F.misc2.x);
    col = mix(col, neutral(sky(-v), 0.5) * 0.9, fog);
    if (mode != 1u) {
        col = aces(col * F.sky_hor.w);
    }
    if (amode != 2u && cls != 5u && cls != 6u && cls != 8u) { alpha = 1.0; }
    return vec4(col, alpha);
}
