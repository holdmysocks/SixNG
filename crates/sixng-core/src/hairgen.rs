//! How the game grows the full strand set of a groom from its guides: the
//! arithmetic of its strand generation compute program (`smh_gen_1`, the
//! variant without extra features), read from the compiled program and
//! checked against it with an interpreter on random inputs (triangle, seed
//! and barycentric weights bit for bit; length and positions to the
//! output's quantisation).
//!
//! Per generated strand, by its index:
//!
//! 1. `pick`: up to four hashed attempts at a root triangle; an attempt is
//!    taken with a probability proportional to the triangle's density byte
//!    (the first of its four count bytes). So that byte is a density weight
//!    under rejection sampling, not a strand count; the number of strands is
//!    a run-time value (it is not in the groom).
//! 2. `seed_bary`: a seed from the triangle, the groom's seed and the pick,
//!    and from it the barycentric weights of the root (uniform over the
//!    triangle: `1 - sqrt(r1)`, `(1 - r2) sqrt(r1)`, `r2 sqrt(r1)`).
//! 3. `Draws`: three more random values from the seed: the length factor,
//!    and two picks against per-guide threshold bytes (one gives the 2-bit
//!    variant the strand shader uses to choose among four colour ramps).
//! 4. `grow`: the strand is the barycentric blend of its three guides, cut
//!    at the strand's own length (the blend of the guides' lengths, varied
//!    by the length factor); points past the cut repeat the last position.
//!
//! 5. `style` (the variants with feature bit 128, `smh_gen_129` ...): noise
//!    and curl. Per point, at arc length `s` from the root, a 2D offset
//!    across the strand: noise is a lerp between two directions of a
//!    64-entry table (`TABLE`, stored in the program), hashed from the seed
//!    and `floor(4 s Fn)`; curl is a circle of radius `Ac`, phase
//!    `fract(Fc s + r)` with `r` the length draw. Both fade in from half the
//!    fade length to the fade length. The offset goes on the output point
//!    only, so it does not accumulate. Checked against the program with an
//!    interpreter on 400 random inputs (to 2.5e-7). The six values are the
//!    groom's first style group (system page +0xF8), which the game
//!    copies into the per-instance record the program reads.
//!
//! Not covered: the cull groups (hat and clothing masks), the length
//! texture, skinning, the twist value written per vertex (the style
//! offset's frame uses it: here the root triangle's normal stands in)
//! and clumping (bits 2 / 4 / 8 / 16, only partly
//! read). Both amplitudes are scaled by `1 - m`, `m` a per-guide value not yet
//! found: taken as 0.
//! The acceptance scale of step 1 is a run-time number; here the densest
//! triangle is always taken.

/// Root triangle of strand `strand` and the pick value that seeds the rest.
/// `density` is the first count byte of every root triangle, `scale` what a
/// byte is multiplied by to give the acceptance probability.
pub fn pick(strand: u32, density: &[u8], scale: f32) -> (usize, u32) {
    let t = density.len().max(1) as u32;
    let v3 = 0xef3e_fb11u32.wrapping_add(strand.wrapping_mul(1_664_525));
    let (mut tri, mut v20) = (0u32, 0u32);
    for i in 0..4u32 {
        let s0 = i.wrapping_mul(1_664_525).wrapping_add(0x8694_5bed);
        let v11 = s0.wrapping_mul(0x153e_39ce).wrapping_add(v3);
        let mut v5 = 0x31cd_17d1u32.wrapping_mul(v11).wrapping_add(0x153e_39ce);
        let mut v6 = v5.wrapping_mul(v11).wrapping_add(0x31cd_17d1);
        let mut v7 = v6.wrapping_mul(v5).wrapping_add(s0);
        v5 ^= v5 >> 16;
        v6 ^= v6 >> 16;
        v7 ^= v7 >> 16;
        let v8 = v7.wrapping_mul(v5).wrapping_add((v11 >> 16) ^ v11);
        tri = (v8 >> 16) % t;
        let v10 = v8.wrapping_mul(v6).wrapping_add(v5);
        let a = v10.wrapping_mul(v8).wrapping_add(v6);
        let b = a.wrapping_mul(v10).wrapping_add(v7);
        v20 = a >> 8;
        let r = (b & 0xff_ffff) as f32 * (1.0 / 16_777_216.0);
        let p = (density.get(tri as usize).copied().unwrap_or(0) as f32 * scale).clamp(0.0, 1.0);
        if r < p {
            break;
        }
    }
    (tri as usize, v20)
}

/// The strand's seed and the barycentric weights of its root on the
/// triangle's three guides.
pub fn seed_bary(tri: u32, groom_seed: u32, v20: u32) -> (u32, [f32; 3]) {
    let seed = tri.wrapping_mul(961).wrapping_add(groom_seed.wrapping_mul(566_029)).wrapping_add(v20.wrapping_mul(31));
    let mad = |x: u32, c: u32| 0x00c0_83c5u32.wrapping_mul(x & 0xff_ffff).wrapping_add(c);
    let x1 = mad(seed, 0x3c6e_f35f);
    let x2 = mad(x1, 0x3c6e_f35f);
    let x3 = mad(x2, x1);
    let x4 = mad(x3, x2);
    let (y1, y2) = (x3 ^ (x3 >> 12), x4 ^ (x4 >> 12));
    let x5 = mad(y2, y1);
    let x6 = mad(x5, y2);
    let f = |x: u32| ((x & 0xff_ffff) ^ (x >> 12)) as f32 * (1.0 / 16_777_216.0);
    let (r1, r2) = (f(x5), f(x6));
    let s = r1.sqrt();
    (seed, [1.0 - s, (1.0 - r2) * s, r2 * s])
}

/// The strand's further random values, in the order the program draws
/// them: length factor, second pick, variant pick.
pub struct Draws(pub u32);

impl Draws {
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        let st = self.0;
        let w = ((st >> ((st >> 28) + 4)) ^ st).wrapping_mul(277_803_737);
        (((w >> 22) ^ w) & 0xff_ffff) as f32 * (1.0 / 16_777_216.0)
    }
}

/// How many of the three thresholds (bytes / 255, blended over the guides)
/// the draw does not fall under: the 2-bit variant of a strand.
pub fn variant(draw: f32, steps: [f32; 3]) -> u8 {
    steps.iter().filter(|&&s| !(draw < s)).count() as u8
}

/// The strand's length: the blend of its guides' lengths, varied by the
/// draw. `var` is the groom's (amount, lowest factor) pair.
pub fn length(blend: f32, draw: f32, var: [f32; 2]) -> f32 {
    let k = var[1] + draw * (1.0 - var[1]);
    blend + (blend * k - blend) * var[0]
}

/// The points of a generated strand: `blend(j)` is the barycentric blend of
/// the guides' point `j`; the strand follows them and stops at `len`.
pub fn grow(k: usize, len: f32, blend: impl Fn(usize) -> [f32; 3]) -> Vec<[f32; 3]> {
    const EPS: f32 = 1.192_092_9e-7;
    const TINY: f32 = 0.000_976_562_5;
    let mut out = Vec::with_capacity(k);
    if k == 0 {
        return out;
    }
    let mut p = blend(0);
    out.push(p);
    let mut used = 0.0f32;
    for j in 1..k {
        let q = blend(j);
        let e = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
        let le = (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt().max(EPS);
        let step = le.min((len - used).max(0.0));
        let step = if step > TINY { step } else { 0.0 };
        used += step;
        let f = step / le;
        p = [p[0] + e[0] * f, p[1] + e[1] * f, p[2] + e[2] * f];
        out.push(p);
    }
    out
}

/// Direction table of the strand programs (64 x 2D, in [-1, 1]), as stored
/// after the code of `smh_gen_129` (the same in every variant that has it).
pub const TABLE: [[f32; 2]; 64] = [
    [-0.610765994, 0.449018002], [-0.343364, -0.941099226], [0.897336006, -0.923520625], [-0.0722979978, 0.386516005],
    [0.27097401, -0.138035998], [0.267190009, -0.87481302], [-0.757622004, -0.492110014], [-0.995300114, 0.0967739969],
    [-0.121434003, -0.450300008], [-0.429242015, -0.0210880004], [-0.934080005, 0.929441988], [-0.998046815, -0.960753202],
    [-0.540026009, -0.274147987], [-0.677601993, -0.859920025], [0.350566, 0.179662004], [-0.735037982, 0.196873993],
    [-0.659474015, 0.73082602], [0.501875997, -0.756280005], [-0.00613400014, -0.972716391], [0.704397976, -0.226659998],
    [-0.292275995, 0.55497998], [-0.0329900011, 0.132054001], [0.0993380025, -0.63438803], [-0.429610014, -0.673879981],
    [0.158178002, 0.595264018], [-0.817743421, -0.222205997], [-0.976195574, 0.511582017], [-0.939023972, -0.705435991],
    [-0.186194003, -0.164159998], [0.572618008, -0.514634013], [-0.367655993, 0.264503986], [-0.0195620004, -0.741752028],
    [0.218482003, -0.00705000013], [-0.286233991, -0.646899998], [0.816096008, -0.675284028], [-0.516282022, 0.701834023],
    [0.209266007, 0.264564008], [0.700673997, -0.958738983], [-0.520982027, 0.87023598], [0.691457987, -0.724478006],
    [0.464094013, 0.00302199996], [-0.075534001, -0.326151997], [0.448042005, -0.444808006], [-0.0362260006, 0.616566002],
    [-0.936216414, 0.363628], [-0.889950216, 0.216711998], [-0.553758025, 0.0368960015], [-0.785700023, 0.835322022],
    [-0.582934022, -0.546249986], [0.725149989, -0.515428007], [0.383648008, -0.271768004], [-0.0707120001, -0.0163880009],
    [0.166843995, -0.234533995], [0.42869401, 0.321512014], [0.228676006, -0.364360005], [-0.969359398, -0.383525997],
    [0.974120021, -0.76079601], [-0.314859986, -0.215430006], [0.0630199984, -0.384319991], [-0.782648027, 0.53263998],
    [0.502488017, -0.933713794], [-0.608142018, 0.220679998], [0.569872022, -0.0992759988], [0.128330007, 0.132175997],
];

/// The hash the program draws its random values from (the raw bits of
/// `Draws::next`).
pub fn pcg_bits(st: u32) -> u32 {
    let w = ((st >> ((st >> 28) + 4)) ^ st).wrapping_mul(277_803_737);
    (w >> 22) ^ w
}

/// Noise and curl settings of one style group, in the order the groom
/// stores them (six f16): a word the generator does not read, fade-in
/// length (m), noise amplitude
/// (m), curl radius (m), noise frequency and curl frequency (per metre of
/// strand).
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct Style {
    pub bend: f32,
    pub fade: f32,
    pub noise: f32,
    pub curl: f32,
    pub noise_freq: f32,
    pub curl_freq: f32,
}

impl Style {
    pub fn is_flat(&self) -> bool {
        (self.noise == 0.0 || self.noise_freq == 0.0) && (self.curl == 0.0 || self.curl_freq == 0.0)
    }
}

/// The 2D style offset at arc length `s` of a strand with `seed` and length
/// draw `r` (x along `cross(T, N)`, y along `N`), fade-in included.
pub fn style_offset(st: &Style, seed: u32, r: f32, s: f32) -> [f32; 2] {
    let mut n = [0.0f32; 2];
    if st.noise_freq != 0.0 && st.noise != 0.0 {
        let x = 4.0 * s * st.noise_freq;
        let i = x as u32;
        let f = x - x.floor();
        let st0 = seed.wrapping_add(i).wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        let st1 = st0.wrapping_add(747_796_405);
        let (a, c) = (TABLE[(pcg_bits(st0) & 63) as usize], TABLE[(pcg_bits(st1) & 63) as usize]);
        n = [(a[0] + f * (c[0] - a[0])) * st.noise, (a[1] + f * (c[1] - a[1])) * st.noise];
    }
    if st.curl_freq != 0.0 && st.curl != 0.0 {
        let x = st.curl_freq * s + r;
        let ph = std::f32::consts::TAU * (x - x.trunc());
        n[0] += ph.sin() * st.curl;
        n[1] += ph.cos() * st.curl;
    }
    let k = if st.fade == 0.0 { 1.0 } else { (2.0 * s / st.fade - 1.0).clamp(0.0, 1.0) };
    [n[0] * k, n[1] * k]
}

/// Add the style offsets to a grown strand (`grow`'s points). `reference`
/// is the vector the frame is built from (`N = normalize(cross(reference,
/// T))`): the game's per-vertex twist vector, not decoded; callers pass
/// the root triangle's normal.
pub fn apply_style(points: &mut [[f32; 3]], st: &Style, seed: u32, r: f32, reference: [f32; 3]) {
    if st.is_flat() || points.len() < 2 {
        return;
    }
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let len = |a: [f32; 3]| (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let walk = points.to_vec();
    let first = (1..walk.len()).map(|j| sub(walk[j], walk[j - 1])).find(|e| len(*e) > 1e-9);
    let Some(mut t) = first else { return };
    let mut s = 0.0f32;
    for j in 0..walk.len() {
        if j > 0 {
            let e = sub(walk[j], walk[j - 1]);
            let l = len(e);
            if l > 1e-9 {
                t = e;
                s += l;
            }
        }
        let tl = len(t);
        let tn = [t[0] / tl, t[1] / tl, t[2] / tl];
        let n = cross(reference, tn);
        let nl = len(n);
        if nl < 1e-9 {
            continue;
        }
        let n = [n[0] / nl, n[1] / nl, n[2] / nl];
        let u = cross(tn, n);
        let o = style_offset(st, seed, r, s);
        for c in 0..3 {
            points[j][c] = walk[j][c] + o[0] * u[c] + o[1] * n[c];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // expected values are from the reference implementation that was
    // checked against the compiled program
    #[test]
    fn triangle_pick_matches_the_program() {
        let density = [10u8, 200, 0, 255, 90, 30, 60];
        let want = [(0u32, 5usize, 10_186_703u32), (1, 3, 5_667_830), (2, 3, 2_705_055), (77, 1, 8_628_155), (1000, 3, 2_750_504), (123_456, 6, 6_949_998)];
        for (strand, tri, v20) in want {
            assert_eq!(pick(strand, &density, 1.0 / 255.0), (tri, v20), "strand {strand}");
        }
    }

    #[test]
    fn seed_weights_and_draws_match_the_program() {
        let cases: [(u32, u32, u32, u32, [f32; 3], [f32; 3]); 3] = [
            (0, 0, 0, 0, [0.228777503, 0.304961914, 0.466260584], [0.731199384, 0.713725805, 0.964366019]),
            (5, 305_419_896, 11_259_375, 138_727_118, [0.316025671, 0.624082725, 0.0598916039], [0.862997115, 0.89719373, 0.238004327]),
            (1000, 7, 99, 4_926_272, [0.431437525, 0.428319536, 0.140242938], [0.816430628, 0.526370287, 0.491015673]),
        ];
        for (tri, gs, v20, seed, w, d) in cases {
            let (s, b) = seed_bary(tri, gs, v20);
            assert_eq!(s, seed);
            assert!((0..3).all(|i| (b[i] - w[i]).abs() < 1e-6), "{b:?}");
            assert!((b[0] + b[1] + b[2] - 1.0).abs() < 1e-6);
            let mut dr = Draws(s);
            let got = [dr.next(), dr.next(), dr.next()];
            assert!((0..3).all(|i| (got[i] - d[i]).abs() < 1e-7), "{got:?}");
        }
    }

    #[test]
    fn a_strand_follows_the_guide_blend_and_stops_at_its_length() {
        let g: [[[f32; 3]; 6]; 3] = [
            [[-0.0105, 0.0018, 0.0015], [0.0042, 0.005, 0.0303], [-0.0195, 0.0135, 0.061], [-0.0106, 0.0198, 0.0919], [0.0135, -0.0009, 0.1226], [-0.014, 0.0054, 0.1535]],
            [[0.0109, 0.0097, 0.0027], [-0.0074, 0.0103, 0.0324], [0.0021, -0.0188, 0.0635], [0.0089, 0.0088, 0.0935], [0.0186, 0.0168, 0.1216], [0.022, -0.0022, 0.1537]],
            [[0.0352, -0.0161, 0.0005], [0.0087, 0.0186, 0.0317], [0.0251, -0.008, 0.062], [0.0154, -0.006, 0.0923], [0.0234, 0.0162, 0.1227], [0.0372, 0.0143, 0.154]],
        ];
        let w = [0.2f32, 0.5, 0.3];
        let blend = |j: usize| [0, 1, 2].map(|c| g[0][j][c] * w[0] + g[1][j][c] * w[1] + g[2][j][c] * w[2]);
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 2e-6);
        // longer than the strand: the blend itself
        let full = grow(6, 1.0, blend);
        assert!(near(full[0], [0.013910, 0.000380, 0.001800]) && near(full[2], [0.004680, -0.009100, 0.062550]) && near(full[5], [0.019360, 0.004270, 0.153750]), "{full:?}");
        // cut inside the second segment: later points repeat the cut
        let cut = grow(6, 0.07, blend);
        assert!(near(cut[1], [-0.000250, 0.011730, 0.031770]) && near(cut[2], [0.004348, -0.007696, 0.060475]) && cut[3] == cut[2] && cut[5] == cut[2], "{cut:?}");
        assert_eq!((variant(0.1, [0.25, 0.5, 0.75]), variant(0.3, [0.25, 0.5, 0.75]), variant(0.9, [0.25, 0.5, 0.75])), (0, 1, 3));
        assert!((length(0.2, 0.0, [0.5, 0.6]) - 0.16).abs() < 1e-6 && (length(0.2, 1.0, [0.5, 0.6]) - 0.2).abs() < 1e-6 && length(0.2, 0.3, [0.0, 0.6]) == 0.2);
    }

    #[test]
    fn style_noise_curl_and_fade() {
        // curl alone: a circle of the radius, phase from the length draw
        let curl = Style { curl: 0.002, curl_freq: 10.0, ..Default::default() };
        let o = style_offset(&curl, 7, 0.25, 0.1);
        let x = 10.0f32 * 0.1 + 0.25;
        let ph = std::f32::consts::TAU * (x - x.trunc());
        assert!((o[0] - ph.sin() * 0.002).abs() < 1e-7 && (o[1] - ph.cos() * 0.002).abs() < 1e-7, "{o:?}");
        assert!(((o[0] * o[0] + o[1] * o[1]).sqrt() - 0.002).abs() < 1e-7);
        // fade: nothing before half the fade length, all of it from the fade length on
        let faded = Style { fade: 0.04, ..curl };
        assert_eq!(style_offset(&faded, 7, 0.25, 0.019), [0.0, 0.0]);
        assert_eq!(style_offset(&faded, 7, 0.25, 0.05), style_offset(&curl, 7, 0.25, 0.05));
        // noise alone: a table entry at whole steps, scaled by the amplitude
        let noise = Style { noise: 0.01, noise_freq: 2.5, ..Default::default() };
        let o = style_offset(&noise, 3, 0.0, 0.2);
        let x = 4.0f32 * 0.2 * 2.5;
        let st0 = 3u32.wrapping_add(x as u32).wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        let (a, c) = (TABLE[(pcg_bits(st0) & 63) as usize], TABLE[(pcg_bits(st0.wrapping_add(747_796_405)) & 63) as usize]);
        let f = x - x.floor();
        assert!((o[0] - (a[0] + f * (c[0] - a[0])) * 0.01).abs() < 1e-7 && (o[1] - (a[1] + f * (c[1] - a[1])) * 0.01).abs() < 1e-7, "{o:?}");
        assert!(Style::default().is_flat() && !curl.is_flat());
        // a straight strand gets curled across itself; the length axis stays
        let mut pts: Vec<[f32; 3]> = (0..8).map(|j| [0.0, 0.0, j as f32 * 0.01]).collect();
        apply_style(&mut pts, &curl, 7, 0.25, [1.0, 0.0, 0.0]);
        assert!(pts.iter().enumerate().all(|(j, p)| (p[2] - j as f32 * 0.01).abs() < 1e-7));
        assert!(pts[3][0].abs() + pts[3][1].abs() > 1e-4);
        // the table is the one in the program
        assert_eq!(TABLE[0], [-0.610765994, 0.449018002]);
        assert_eq!(TABLE[3], [-0.0722979978, 0.386516005]);
    }
}
