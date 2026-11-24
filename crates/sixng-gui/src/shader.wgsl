// sixng viewer shaders: PBR-lite meshes (skinned, instanced), wireframe,
// debug lines and a procedural sky.

struct Frame {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    sun_dir: vec4<f32>,   // xyz towards the sun, w intensity
    sun_col: vec4<f32>,
    sky_top: vec4<f32>,
    sky_hor: vec4<f32>,   // w exposure
    mode: vec4<u32>,      // x view mode, y packed channel order, z paint finish, w slot view
    paint1: vec4<f32>,    // primary paint colour (linear), w the vehicle's own paint roughness
    paint2: vec4<f32>,    // pearl colour (linear), w pearl amount
    misc: vec4<f32>,      // x wetness, y emissive boost, z time, w ambient
    misc2: vec4<f32>,     // x fog density, yzw secondary paint colour
    misc3: vec4<f32>,     // x tint palette on (1) / off (0), w the vehicle's own paint metalness
    prev_view_proj: mat4x4<f32>,
    jitter: vec4<f32>,    // w vehicle wear (0 to 1)
};

struct Mat {
    base: vec4<f32>,
    flags: vec4<u32>,     // x has-bits (1 base 2 normal 4 packed 8 emissive), y cls, z alpha (0 opaque 1 mask 2 blend), w gap in bits 0-1 (0 ok 1 low 2 flat), bit 8 = stand-in surface with metallic in bits 16-23 and roughness in bits 24-31, bit 9 = base texture is a tint mask
    tile: vec4<f32>,      // fabric detail tile: x UV scale, y rotation, z mean grey value, w bound
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

fn sky(d: vec3<f32>) -> vec3<f32> {
    let t = clamp(d.y, -1.0, 1.0);
    var c = mix(F.sky_hor.rgb, F.sky_top.rgb, pow(max(t, 0.0), 0.6));
    if (t < 0.0) {
        c = mix(F.sky_hor.rgb * 0.55, F.sky_hor.rgb * 0.25, min(-t * 3.0, 1.0));
    }
    let s = max(dot(normalize(d), normalize(F.sun_dir.xyz)), 0.0);
    c += F.sun_col.rgb * (pow(s, 600.0) * 30.0 + pow(s, 8.0) * 0.15) * F.sun_dir.w;
    return c;
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
fn hair_game(colour: vec3<f32>, t: vec3<f32>, n: vec3<f32>, root: vec3<f32>, v: vec3<f32>, l: vec3<f32>, g0: f32, y: f32) -> vec3<f32> {
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
    // times the azimuth term, which reaches 2 for light from the eye's side;
    // the program also fades this towards 1 by a depth value (not decoded)
    // (fade 0.5 is a stand-in: without it the unlit side is black)
    let fade = 0.5;
    let w = clamp(dot(root, l), 0.0, 1.0) * clamp(1.0 + cos_phi, 0.0, 2.0) * (1.0 - fade) + fade;
    return max(w * (coloured * colour / max(colour, vec3(0.000001)) + vec3(f * m0.r * nr2 * area)), vec3(0.0));
}

@fragment
fn fs_mesh(i: VOut, @builtin(front_facing) ff: bool) -> @location(0) vec4<f32> {
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
    if ((has & 4u) != 0u && (M.flags.w & 8192u) != 0u) {
        ao = textureSample(t_pk, samp, i.uv0).b;
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
        albedo = base.rgb * textureSampleLevel(t_em, samp, vec2(clamp(g, 0.004, 0.996), (id + 0.5) / 512.0), 0.0).rgb;
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
        }
    }
    if (painted) {
        albedo = F.paint1.rgb;
        if ((has & 16u) != 0u) { albedo = F.misc2.yzw; }
        if (F.mode.z == 0u) { metal = 0.6; rough = 0.28; }
        else if (F.mode.z == 1u) { metal = 0.0; rough = 0.8; }
        else if (F.mode.z == 2u) { metal = 1.0; rough = 0.04; albedo = mix(vec3(0.95), albedo, 0.25); }
        else if (F.mode.z == 4u) { metal = F.misc3.w; rough = clamp(F.paint1.w, 0.04, 1.0); }
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
    // under bare metal): rows 256.. of the part ramp image hold its colour
    // and roughness (alpha); its metalness is the alpha of the first rows.
    // Shown where the layer detail texture's blue is high (taken as the
    // wear mask), by the amount the user sets.
    if (M.tile.w > 1.5 && F.jitter.w > 0.0) {
        let d = textureSample(t_tile, samp, i.uv0);
        let id = floor(i.col.a * 255.0 + 0.5);
        let u = clamp(d.g, 0.004, 0.996);
        let w1 = textureSampleLevel(t_em, samp, vec2(u, (id + 256.5) / 512.0), 0.0);
        let m1 = textureSampleLevel(t_em, samp, vec2(u, (id + 0.5) / 512.0), 0.0).a;
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
    } else if (cls == 7u) {
        var e = albedo;
        if ((has & 8u) != 0u) { e = textureSample(t_em, samp, i.uv0).rgb; }
        emissive = e * F.misc.y;
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
    let ndh = max(dot(n, h), 0.0);
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
    // diffuse ambient from a mostly desaturated sky: the full sky colour tinted
    // every shaded surface blue-white
    let amb_d = mix(neutral(F.sky_hor.rgb, 0.8) * 0.6, neutral(F.sky_top.rgb, 0.8), n.y * 0.5 + 0.5) * F.misc.w;
    let fr_v = f0 + (max(vec3(1.0 - rough), f0) - f0) * pow(1.0 - ndv, 5.0);
    let r = reflect(-v, n);
    let env = mix(sky(r), (F.sky_top.rgb + F.sky_hor.rgb) * 0.5, rough) * F.misc.w;
    var col = direct * ao_direct + kd * albedo * amb_d * ao + env * fr_v * ao + emissive;
    col += pearl * pow(1.0 - ndv, 2.0) * 0.6;
    if (cls == 12u) {
        // strands: the fibre model on the strand's direction (the tangent)
        let tdir = normalize(i.tan.xyz);
        let sun = F.sun_col.rgb * F.sun_dir.w * 3.0;
        let amb_h = sqrt(albedo) * 0.5 * (neutral(F.sky_top.rgb, 0.8) + neutral(F.sky_hor.rgb, 0.8) * 0.6) * 0.5 * F.misc.w;
        col = hair_light(albedo, tdir, v, l, 0.33, 0.5) * sun * ao_direct + amb_h * ao;
        // with the game's tables: its own light sum
        let dims = textureDimensions(t_hair_m);
        if (dims.x > 1u) {
            // the tube normal facing the eye (the game rounds it across the
            // ribbon; the ribbons here carry no position across)
            let tube = normalize(cross(cross(tdir, v), tdir));
            // large-scale light and dark side from the head: the root normal
            let root = normalize(i.nrm);
            let direct_h = hair_game(albedo, tdir, tube, root, v, l, 0.33, 0.6067) * sun;
            // ambient is the viewer's own: the same sum lit from the root normal's side by the sky
            let sky_h = (neutral(F.sky_top.rgb, 0.8) + neutral(F.sky_hor.rgb, 0.8) * 0.6) * 0.5 * F.misc.w;
            let amb_g = hair_game(albedo, tdir, tube, root, v, normalize(root + vec3(0.0, 0.6, 0.0)), 0.33, 0.6067) * sky_h * 3.14159;
            col = direct_h * ao_direct + amb_g * ao;
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
    let amb_d = mix(neutral(F.sky_hor.rgb, 0.8) * 0.6, neutral(F.sky_top.rgb, 0.8), n.y * 0.5 + 0.5) * F.misc.w;
    let fr_v = f0 + (max(vec3(1.0 - rough), f0) - f0) * pow(1.0 - ndv, 5.0);
    let env = mix(sky(reflect(-v, n)), (F.sky_top.rgb + F.sky_hor.rgb) * 0.5, rough) * F.misc.w;
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
