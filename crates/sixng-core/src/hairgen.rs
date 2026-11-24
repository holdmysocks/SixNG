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
//! Not covered: the cull groups (hat and clothing masks), the length
//! texture, skinning, the twist value written per vertex, and the other
//! variants of the program, which are where clumping and curl would be.
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
}
