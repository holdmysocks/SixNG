//! Strand hair grooms: unnamed `packs.rpf` resources (hair, beards, brows,
//! lashes, stubble, animal fur) holding guide strands.
//!
//! System page: byte 0x1A = 3 (type), u32 0x1C = 0x7d8f3db3; vec4 0x30 /
//! 0x40 = root bounding box (min / max); vec4 0x50 / 0x60 = a second box
//! (shared by the LODs of one groom); ptr 0x70 = buffer descriptor (+0x18
//! graphics pointer); u32 0x98 = bone count, ptr 0xA0 = u32 bone joaat hashes.
//!
//! Graphics buffer: u32[8] section offsets A0-A7 at 0x00 and B0-B7 at 0x30
//! (from the buffer start); u16 S (strands) at 0x58, u16 T (root triangles)
//! at 0x5A, u32 K (points per strand) at 0x5C. B5 = S x K points of 4 x f16
//! (x, y, z, w), strand-major, model space, Z-up metres; B4 = T x 3 u16
//! strand indices (triangles between strand roots); B0 = S x f16 strand
//! length; A2 = S x 2 f16, the strand's root UV (inferred: in 0..1 and
//! following the root position). The point w is about 0.5 mm along the
//! strand and larger at the root (about 3 mm): a width (inferred). The
//! other sections (A0/A1/A3-A7, B1-B3, B6, B7, Q0-Q3, the tail weights) are
//! not decoded.
//!
//! Which grooms belong to a hair asset is stored through name ids (the
//! asset's placeholder name, the piece table, the file ids of the table of
//! contents): see `hairlink`. Grooms come in LOD chains (consecutive
//! resources with the same bones and fewer strands each time).
use crate::resource::Resource;
use anyhow::{anyhow, bail, Result};

pub const TYPE_GROOM: u8 = 3;
pub const GROOM_TAG: u32 = 0x7d8f_3db3;

#[derive(Clone)]
pub struct Groom {
    pub strands: usize,
    pub points_per_strand: usize,
    /// S x K points, strand-major (RAGE space, Z-up metres).
    pub points: Vec<[f32; 3]>,
    /// Per-point w (non-zero at the root; thickness or mass, unknown).
    pub w: Vec<f32>,
    /// Root triangles (strand indices).
    pub roots: Vec<[u32; 3]>,
    /// Stored strand lengths.
    pub lengths: Vec<f32>,
    /// Root UV per strand (A2; inferred), empty when absent.
    pub root_uv: Vec<[f32; 2]>,
    pub bones: Vec<u32>,
    pub root_box: [[f32; 3]; 2],
    /// Second box (None when empty); equal across one groom's LODs.
    pub group_box: Option<[[f32; 3]; 2]>,
    /// B6, one entry per root triangle (same order as `roots`): c0 = strands
    /// the game grows in the triangle, c1 = strands in clumps (inferred),
    /// c2 = clumps, c3 unknown. Empty when the section is absent. The sum
    /// of c0 equals system +0xD0 on most densest LODs; lower LODs saturate
    /// at 255.
    pub tri_counts: Vec<[u8; 4]>,
    /// System +0xF0 (f16): strand width in metres (0.2-0.5 mm; inferred),
    /// 0 when not stored.
    pub width: f32,
    /// System +0x110: 16 width steps from root to tip (/ 255; inferred).
    pub taper: [u8; 16],
    /// Three threshold bytes per guide strand (graphics header +0x0C; a
    /// fourth byte is not read): the game draws one random value per
    /// generated strand against them (blended over its guides) to get the
    /// strand's 2-bit variant, which its strand shader uses to choose among
    /// four colour ramps (`hairgen`). Most grooms store 64, 128, 191, so
    /// the four variants are equally likely. Empty when not stored.
    /// System +0x12C (byte / 255): how far the strand colour is multiplied
    /// by the tint read at the strand's root (the body's colour under it):
    /// colour = mix(C, C x T, weight x the tint's alpha). Read from the
    /// game's code by the shader archive work; 1 on most grooms.
    pub tint_weight: f32,
    pub pick_steps: Vec<[u8; 4]>,
    /// Graphics header +0x7C: the groom's seed for strand generation.
    pub seed: u32,
    /// Graphics header +0x60 (two half floats): how much a generated
    /// strand's length varies, and the lowest factor it can get.
    pub length_var: [f32; 2],
    /// System +0xF8 and +0x104: two groups of six half floats each (an
    /// unread word, fade-in length, noise amplitude, curl radius, noise and
    /// curl frequency; `hairgen::Style`). The game copies the first group
    /// into the per-instance record its strand generator reads, so it
    /// applies to every strand; no program read so far reads the second.
    pub curl: [crate::hairgen::Style; 2],
    /// Graphics header bytes 0x00-0xBF as stored (clump settings live here).
    pub head: Vec<u8>,
    /// A0, one dword per guide strand: byte 0 density, byte 1 clump radius
    /// in mm (shader archive reading). Empty when not stored.
    pub corners: Vec<[u8; 4]>,
    /// System +0x12A / +0x12B (byte / 255): how far a clump member's offset
    /// reaches along the clump's two axes (heading, sideways). The game
    /// copies them into its per-draw record; hair modifiers can change
    /// them at run time.
    pub clump_scale: [f32; 2],
    /// Strand styles (graphics header +0x20, a byte offset; 0 = none): the
    /// shape the game's style pass gives every generated strand. Empty for
    /// most pieces.
    pub styles: Vec<Style>,
    /// System +0x12D: how the game places its clumped strands (shader
    /// archive reading): 0 = hashed clump centres in the root triangle
    /// (mode 2), 1 = one strand per thread around a guide strand's root
    /// (mode 16), 2 = clump centre at the triangle's centroid (mode 4, not
    /// grown here), other = no clumping.
    pub clump_type: u8,
    /// A1, per guide strand (low 16 bits): a root triangle (index into
    /// `roots`) with the guide as a corner. Empty when absent.
    pub guide_tri: Vec<u32>,
    /// Per guide: four angles (byte x 2 pi / 255 - pi) at 0, 1/3, 2/3 and 1
    /// of the strand, by which the style pass's frame is turned about the
    /// strand (graphics header +0x24, a byte offset; 0 = none). Read from
    /// the game's strand generator.
    pub twist: Vec<[u8; 4]>,
}

/// One strand style record: 2D offsets across the strand along its length.
/// Stored as two half-float pairs (the x, y range) and samples of
/// x (10 bits), y (10 bits) and the arc-length position z (12 bits), all
/// unsigned fractions. Read from the game's style pass by the shader
/// archive work (medium confidence).
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub lo: [f32; 2],
    pub hi: [f32; 2],
    pub samples: Vec<u32>,
}

impl Style {
    /// (x, y, z) at `u` (0 root .. 1 tip): x, y in metres, z a fraction of
    /// the strand's length.
    pub fn at(&self, u: f32) -> [f32; 3] {
        let n = self.samples.len();
        if n == 0 {
            return [0.0, 0.0, u];
        }
        let d = |v: u32| [(v & 1023) as f32 / 1023.0, ((v >> 10) & 1023) as f32 / 1023.0, (v >> 20) as f32 / 4095.0];
        let x = u.clamp(0.0, 1.0) * (n - 1) as f32;
        let i = (x as usize).min(n.saturating_sub(2));
        let (a, b) = (d(self.samples[i]), d(self.samples[(i + 1).min(n - 1)]));
        let f = (x - i as f32).clamp(0.0, 1.0);
        let m = |q: usize| a[q] + (b[q] - a[q]) * f;
        [self.lo[0] + (self.hi[0] - self.lo[0]) * m(0), self.lo[1] + (self.hi[1] - self.lo[1]) * m(1), m(2)]
    }

    /// Re-shape one strand (its points in order, root first): point j goes
    /// to the strand's point at arc length z(u) L, u = j / (K - 1), plus the
    /// style offset x along the frame axis `up(z)` (made square to the
    /// strand) and y along axis x tangent (`swap` exchanges them). The
    /// offset's length is 0.8 mm + k (|d| - 0.8 mm), capped at |d|: the
    /// game's k is a per-point weight from the collider damping, capped by
    /// the per-instance style strength (`k` of the point's place on the
    /// strand before the re-shape).
    pub fn apply(&self, pts: &mut [[f32; 3]], up: &dyn Fn(f32) -> [f32; 3], swap: bool, k: &dyn Fn([f32; 3]) -> f32) {
        let k_strength = k;
        let k = pts.len();
        if k < 2 || self.samples.is_empty() {
            return;
        }
        let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let mut arc = vec![0.0f32; k];
        for j in 1..k {
            let e = sub(pts[j], pts[j - 1]);
            arc[j] = arc[j - 1] + dot(e, e).sqrt();
        }
        let total = arc[k - 1];
        if total <= 1e-6 {
            return;
        }
        let src = pts.to_vec();
        for j in 0..k {
            let [x, y, z] = self.at(j as f32 / (k - 1) as f32);
            let s = z.clamp(0.0, 1.0) * total;
            let i = arc.partition_point(|&a| a <= s).clamp(1, k - 1);
            let seg = (arc[i] - arc[i - 1]).max(1e-9);
            let f = ((s - arc[i - 1]) / seg).clamp(0.0, 1.0);
            let e = sub(src[i], src[i - 1]);
            let base = [src[i - 1][0] + e[0] * f, src[i - 1][1] + e[1] * f, src[i - 1][2] + e[2] * f];
            let len = (x * x + y * y).sqrt();
            let f = if len > 1e-9 { ((0.0008 + k_strength(base) * (len - 0.0008)) / len).clamp(0.0, 1.0) } else { 0.0 };
            let (x, y) = (x * f, y * f);
            let l = dot(e, e).sqrt().max(1e-9);
            let t = [e[0] / l, e[1] / l, e[2] / l];
            let up = up(z.clamp(0.0, 1.0));
            let d = dot(up, t);
            let nn = [up[0] - t[0] * d, up[1] - t[1] * d, up[2] - t[2] * d];
            let nl = dot(nn, nn).sqrt();
            if nl < 1e-6 {
                pts[j] = base;
                continue;
            }
            let n = [nn[0] / nl, nn[1] / nl, nn[2] / nl];
            let bv = [n[1] * t[2] - n[2] * t[1], n[2] * t[0] - n[0] * t[2], n[0] * t[1] - n[1] * t[0]];
            let (a, b) = if swap { (y, x) } else { (x, y) };
            pts[j] = [base[0] + n[0] * a + bv[0] * b, base[1] + n[1] * a + bv[1] * b, base[2] + n[2] * a + bv[2] * b];
        }
    }
}

/// What the style pass damps against: the game's colliders are capsules,
/// spheres and boxes built on the CPU from the character's body bound (its
/// ragdoll pieces on the head, neck, shoulders and chest) and hats, not
/// stored with the groom. `Colliders::shapes` takes the body bound's round
/// pieces (`Taper`), and `with_boxes` its boxes (`Cuboid`); without a body bound, `Colliders::new` stands in with
/// the character's own surface points (head, body and garments). `damp` is
/// the game's d = sat(1 - dist / falloff), dist to the nearest surface.
pub struct Colliders {
    cell: f32,
    grid: std::collections::HashMap<(i32, i32, i32), Vec<[f32; 3]>>,
    shapes: Vec<Taper>,
    boxes: Vec<Cuboid>,
    /// Changes with the points or shapes (cache key).
    pub id: u64,
}

/// A tapered capsule (a sphere when `a == b`): radius `ra` at `a`, `rb`
/// at `b`, model space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Taper {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub ra: f32,
    pub rb: f32,
}

impl Taper {
    /// Distance from `p` to the surface (negative inside): to the nearest
    /// point of the axis, less the radius there.
    pub fn dist(&self, p: [f32; 3]) -> f32 {
        let ab = [self.b[0] - self.a[0], self.b[1] - self.a[1], self.b[2] - self.a[2]];
        let ap = [p[0] - self.a[0], p[1] - self.a[1], p[2] - self.a[2]];
        let ll = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
        let t = if ll > 0.0 { ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / ll).clamp(0.0, 1.0) } else { 0.0 };
        let d = [ap[0] - t * ab[0], ap[1] - t * ab[1], ap[2] - t * ab[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - (self.ra + t * (self.rb - self.ra))
    }
}

/// A box: centre, unit axes and half extents along them, model space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cuboid {
    pub centre: [f32; 3],
    pub axes: [[f32; 3]; 3],
    pub half: [f32; 3],
}

impl Cuboid {
    /// Distance from `p` to the surface (negative inside).
    pub fn dist(&self, p: [f32; 3]) -> f32 {
        let d = [p[0] - self.centre[0], p[1] - self.centre[1], p[2] - self.centre[2]];
        let q: [f32; 3] = std::array::from_fn(|i| (d[0] * self.axes[i][0] + d[1] * self.axes[i][1] + d[2] * self.axes[i][2]).abs() - self.half[i]);
        let out = q.iter().map(|v| v.max(0.0).powi(2)).sum::<f32>().sqrt();
        out + q[0].max(q[1]).max(q[2]).min(0.0)
    }
}

impl Colliders {
    /// Distance at which a collider stops damping (metres).
    pub const FALLOFF: f32 = 0.03;

    pub fn new(points: &[[f32; 3]]) -> Colliders {
        let cell = Self::FALLOFF;
        let mut grid: std::collections::HashMap<(i32, i32, i32), Vec<[f32; 3]>> = Default::default();
        let mut id = 0xcbf2_9ce4_8422_2325u64;
        for p in points {
            grid.entry(Self::key(cell, *p)).or_default().push(*p);
            id = (id ^ (p[0].to_bits() as u64) << 1 ^ p[2].to_bits() as u64).wrapping_mul(0x100_0000_01b3);
        }
        Colliders { cell, grid, shapes: Vec::new(), boxes: Vec::new(), id }
    }

    /// Colliders from the body bound's round pieces.
    pub fn shapes(shapes: Vec<Taper>) -> Colliders {
        let mut id = 0x84222325_cbf29ce4u64;
        for t in &shapes {
            for v in [t.a[0], t.a[1], t.a[2], t.b[0], t.b[1], t.b[2], t.ra, t.rb] {
                id = (id ^ v.to_bits() as u64).wrapping_mul(0x100_0000_01b3);
            }
        }
        Colliders { cell: Self::FALLOFF, grid: Default::default(), shapes, boxes: Vec::new(), id }
    }

    /// Adds the bound's boxes to the shapes.
    pub fn with_boxes(mut self, boxes: Vec<Cuboid>) -> Colliders {
        for b in &boxes {
            for v in b.centre.iter().chain(b.axes.iter().flatten()).chain(&b.half) {
                self.id = (self.id ^ v.to_bits() as u64).wrapping_mul(0x100_0000_01b3);
            }
        }
        self.boxes = boxes;
        self
    }

    fn key(cell: f32, p: [f32; 3]) -> (i32, i32, i32) {
        ((p[0] / cell).floor() as i32, (p[1] / cell).floor() as i32, (p[2] / cell).floor() as i32)
    }

    /// 0 at `FALLOFF` or more from every point (shape surface), 1 on a
    /// point (on or inside a shape).
    pub fn damp(&self, p: [f32; 3]) -> f32 {
        if !self.shapes.is_empty() || !self.boxes.is_empty() {
            let d = self.shapes.iter().map(|t| t.dist(p)).chain(self.boxes.iter().map(|b| b.dist(p))).fold(f32::MAX, f32::min);
            return (1.0 - d / Self::FALLOFF).clamp(0.0, 1.0);
        }
        let (x, y, z) = Self::key(self.cell, p);
        let mut best = Self::FALLOFF * Self::FALLOFF;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for q in self.grid.get(&(x + dx, y + dy, z + dz)).into_iter().flatten() {
                        best = best.min((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2));
                    }
                }
            }
        }
        (1.0 - best.sqrt() / Self::FALLOFF).clamp(0.0, 1.0)
    }

    /// The style weight at `p` for the instance strength `strength`: the
    /// game rounds the damping to thirds, k = min(1 - round(3 d) / 3, strength).
    pub fn weight(&self, p: [f32; 3], strength: f32) -> f32 {
        (1.0 - (3.0 * self.damp(p)).round() / 3.0).min(strength)
    }
}

/// A hat seen from a point inside the head: per direction, the distance to
/// the nearest point of the hat (its lining). Strand points that lie
/// farther out are pulled in under it (`Shell::press`), so hair does not
/// come through a hat. A display rule: the game's own handling of hair
/// under a hat is not in the data read so far (no hair asset has a hat
/// variant, and a hat item names no hair region).
pub struct Shell {
    pub centre: [f32; 3],
    /// `Shell::W` x `Shell::H` distances by longitude and latitude,
    /// infinite where no hat lies in that direction.
    r: Vec<f32>,
    /// Changes with the hat (cache key).
    pub id: u64,
    /// The hat's own triangles, when known (`Shell::with_lining`).
    lining: Option<Lining>,
}

/// A hat's triangles, binned by their direction from the shell's centre
/// (`Lining::W` x `Lining::H` cells of one degree), for crossing tests.
struct Lining {
    tris: Vec<[[f32; 3]; 3]>,
    cells: Vec<Vec<u32>>,
}

impl Lining {
    const W: usize = 360;
    const H: usize = 180;

    /// Cell coordinates (fractional) of direction `d` from the centre.
    fn at(d: [f32; 3]) -> (f32, f32) {
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-9);
        let x = (d[1].atan2(d[0]) / std::f32::consts::TAU + 0.5) * Self::W as f32;
        let y = ((d[2] / l).clamp(-1.0, 1.0).asin() / std::f32::consts::PI + 0.5) * Self::H as f32;
        (x, y)
    }

    /// Cell ranges (x may run past `W`: it wraps) covering the directions
    /// `c`, with a margin of one cell; every longitude near the poles.
    /// `c` should hold points inside the shape too (a segment or triangle
    /// near a pole reaches higher than its corners).
    fn span(c: &[(f32, f32)]) -> ((i64, i64), (usize, usize)) {
        let w = Self::W as f32;
        let (y0, y1) = c.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.1), b.max(q.1)));
        let (mut x0, mut x1) = c.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.0), b.max(q.0)));
        if x1 - x0 > w / 2.0 {
            // across the longitude seam: count the western ones a turn on
            let xs: Vec<f32> = c.iter().map(|q| if q.0 < w / 2.0 { q.0 + w } else { q.0 }).collect();
            x0 = xs.iter().copied().fold(f32::MAX, f32::min);
            x1 = xs.iter().copied().fold(f32::MIN, f32::max);
        }
        let ya = (y0.floor() as i64 - 1).max(0) as usize;
        let yb = (y1.floor() as i64 + 1).min(Self::H as i64 - 1) as usize;
        let all = ya < 6 || yb + 6 >= Self::H || x1 - x0 > w / 2.0;
        let xs = if all { (0, Self::W as i64 - 1) } else { (x0.floor() as i64 - 1, x1.floor() as i64 + 1) };
        (xs, (ya, yb))
    }

    fn new(centre: [f32; 3], tris: Vec<[[f32; 3]; 3]>) -> Lining {
        let mut cells = vec![Vec::new(); Self::W * Self::H];
        for (i, t) in tris.iter().enumerate() {
            // corners, edge midpoints and centre
            let mid = |p: [f32; 3], q: [f32; 3]| [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0, (p[2] + q[2]) / 2.0];
            let g = [(t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][1] + t[1][1] + t[2][1]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0];
            let pts = [t[0], t[1], t[2], mid(t[0], t[1]), mid(t[1], t[2]), mid(t[2], t[0]), g];
            let c: Vec<(f32, f32)> = pts.iter().map(|p| Self::at([p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]])).collect();
            let ((xa, xb), (ya, yb)) = Self::span(&c);
            for y in ya..=yb {
                for x in xa..=xb {
                    cells[y * Self::W + x.rem_euclid(Self::W as i64) as usize].push(i as u32);
                }
            }
        }
        Lining { tris, cells }
    }

    /// The first crossing of segment `a`-`b` with the hat: its share of
    /// the way from `a`. Only triangles binned in the cells the segment
    /// spans (seen from `centre`) are tested (Moller-Trumbore).
    fn first_hit(&self, centre: [f32; 3], a: [f32; 3], b: [f32; 3]) -> Option<f32> {
        let sub = |p: [f32; 3], q: [f32; 3]| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
        let cross = |p: [f32; 3], q: [f32; 3]| [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]];
        let dot = |p: [f32; 3], q: [f32; 3]| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
        // the directions along the segment (from the centre itself every
        // direction is possible: those of the rest)
        let ends: Vec<(f32, f32)> = (0..=8)
            .map(|i| {
                let f = i as f32 / 8.0;
                [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f]
            })
            .filter(|p| dot(sub(*p, centre), sub(*p, centre)) > 1e-10)
            .map(|p| Self::at(sub(p, centre)))
            .collect();
        let ((xa, xb), (ya, yb)) = Self::span(&ends);
        let mut ids = Vec::new();
        for y in ya..=yb {
            for x in xa..=xb {
                ids.extend_from_slice(&self.cells[y * Self::W + x.rem_euclid(Self::W as i64) as usize]);
            }
        }
        ids.sort_unstable();
        ids.dedup();
        let d = sub(b, a);
        let mut best: Option<f32> = None;
        for i in ids {
            let [v0, v1, v2] = self.tris[i as usize];
            let (e1, e2) = (sub(v1, v0), sub(v2, v0));
            let p = cross(d, e2);
            let det = dot(e1, p);
            if det.abs() < 1e-14 {
                continue;
            }
            let s = sub(a, v0);
            let u = dot(s, p) / det;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = cross(s, e1);
            let v = dot(d, q) / det;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let t = dot(e2, q) / det;
            if (0.0..=1.0).contains(&t) && best.is_none_or(|x| t < x) {
                best = Some(t);
            }
        }
        best
    }
}

impl Shell {
    const W: usize = 72;
    const H: usize = 36;

    fn cell(d: [f32; 3], r: f32) -> usize {
        let lon = d[1].atan2(d[0]);
        let lat = (d[2] / r).clamp(-1.0, 1.0).asin();
        let x = (((lon / std::f32::consts::TAU + 0.5) * Self::W as f32) as usize).min(Self::W - 1);
        let y = (((lat / std::f32::consts::PI + 0.5) * Self::H as f32) as usize).min(Self::H - 1);
        y * Self::W + x
    }

    /// From surface points of the hat and of the head (RAGE space). Cells
    /// between two covered neighbours are filled, so a coarse mesh leaves
    /// no holes.
    pub fn new(centre: [f32; 3], hat: &[[f32; 3]], head: &[[f32; 3]]) -> Shell {
        let mut r = vec![f32::INFINITY; Self::W * Self::H];
        let mut id = 0u64;
        for p in hat {
            let d = [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if l < 1e-4 {
                continue;
            }
            let c = Self::cell(d, l);
            r[c] = r[c].min(l);
            id = id.wrapping_mul(0x100_0000_01b3).wrapping_add(l.to_bits() as u64 ^ c as u64);
        }
        let seen = r.clone();
        for y in 0..Self::H {
            for x in 0..Self::W {
                if seen[y * Self::W + x].is_finite() {
                    continue;
                }
                let at = |dx: usize, dy: usize| seen[((y + dy).clamp(1, Self::H) - 1) * Self::W + (x + dx + Self::W - 1) % Self::W];
                // opposite neighbours both covered: a gap in the sampling
                for (a, b) in [(at(0, 1), at(2, 1)), (at(1, 0), at(1, 2)), (at(0, 0), at(2, 2)), (at(2, 0), at(0, 2))] {
                    if a.is_finite() && b.is_finite() {
                        r[y * Self::W + x] = r[y * Self::W + x].min((a + b) / 2.0);
                    }
                }
            }
        }
        // An opening the hat encloses (the gap over a cap's strap) is
        // closed: open is what connects to straight down, where the head is.
        let mut scalp = vec![0.0f32; r.len()];
        for p in head {
            let d = [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if l > 1e-4 {
                let c = Self::cell(d, l);
                scalp[c] = scalp[c].max(l);
            }
        }
        let mut open = vec![false; r.len()];
        let mut todo: Vec<usize> = (0..Self::W).filter(|&x| !r[x].is_finite()).collect();
        todo.iter().for_each(|&c| open[c] = true);
        while let Some(c) = todo.pop() {
            let (x, y) = (c % Self::W, c / Self::W);
            let mut next = vec![y * Self::W + (x + 1) % Self::W, y * Self::W + (x + Self::W - 1) % Self::W];
            if y > 0 {
                next.push(c - Self::W);
            }
            if y + 1 < Self::H {
                next.push(c + Self::W);
            }
            for n in next {
                if !r[n].is_finite() && !open[n] {
                    open[n] = true;
                    todo.push(n);
                }
            }
        }
        loop {
            let seen = r.clone();
            let mut left = false;
            for c in (0..r.len()).filter(|&c| !seen[c].is_finite() && !open[c]) {
                let (x, y) = (c % Self::W, c / Self::W);
                let near = [(y > 0).then(|| c - Self::W), (y + 1 < Self::H).then(|| c + Self::W), Some(y * Self::W + (x + 1) % Self::W), Some(y * Self::W + (x + Self::W - 1) % Self::W)];
                let v: Vec<f32> = near.iter().flatten().map(|&n| seen[n]).filter(|v| v.is_finite()).collect();
                if v.is_empty() {
                    left = true;
                } else {
                    // no cloth here: the hair may lie on the scalp
                    r[c] = (v.iter().sum::<f32>() / v.len() as f32).max(scalp[c] + 0.006);
                }
            }
            if !left {
                break;
            }
        }
        Shell { centre, r, id, lining: None }
    }

    /// The same shell with the hat's own triangles (RAGE space), for
    /// `press_strands`.
    pub fn with_lining(mut self, tris: Vec<[[f32; 3]; 3]>) -> Shell {
        self.lining = (!tris.is_empty()).then(|| Lining::new(self.centre, tris));
        self
    }

    /// `press` for strands of `k` points each (strand-major), then, with
    /// the hat's triangles (`with_lining`), a check of every strand from
    /// its root: where a segment crosses the hat, its far point goes back
    /// under the hat, `gap` inside it on the line from the centre (or,
    /// where the segment would still cross, `gap` short of the crossing
    /// along the segment). The shell alone is coarse (5 degree cells,
    /// blended out toward the hat's edges) and lets a few points through.
    /// Returns how many points moved.
    pub fn press_strands(&self, points: &mut [[f32; 3]], k: usize, gap: f32) -> usize {
        let before = points.to_vec();
        self.press(points, gap);
        if let (Some(l), true) = (&self.lining, k >= 2) {
            let c = self.centre;
            for s in points.chunks_exact_mut(k) {
                for j in 1..k {
                    let (a, b) = (s[j - 1], s[j]);
                    let Some(t) = l.first_hit(c, a, b) else { continue };
                    let under = l.first_hit(c, c, b).and_then(|h| {
                        let len = ((b[0] - c[0]).powi(2) + (b[1] - c[1]).powi(2) + (b[2] - c[2]).powi(2)).sqrt().max(1e-9);
                        let f = (h - gap / len).max(0.0);
                        let q = [c[0] + (b[0] - c[0]) * f, c[1] + (b[1] - c[1]) * f, c[2] + (b[2] - c[2]) * f];
                        l.first_hit(c, a, q).is_none().then_some(q)
                    });
                    s[j] = under.unwrap_or_else(|| {
                        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt().max(1e-9);
                        let f = (t - gap / len).max(0.0);
                        [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f]
                    });
                }
            }
        }
        before.iter().zip(points.iter()).filter(|(a, b)| a != b).count()
    }

    /// Pull the points that lie beyond the hat to `gap` under it. Returns
    /// how many moved.
    pub fn press(&self, points: &mut [[f32; 3]], gap: f32) -> usize {
        let mut n = 0;
        for p in points {
            let d = [p[0] - self.centre[0], p[1] - self.centre[1], p[2] - self.centre[2]];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if l < 1e-4 {
                continue;
            }
            // blend the four cells around the direction; a cell with no
            // hat leaves the point where it is, so the pull fades out at
            // the hat's edge instead of stepping
            let fx = (d[1].atan2(d[0]) / std::f32::consts::TAU + 0.5) * Self::W as f32 - 0.5;
            let fy = ((d[2] / l).clamp(-1.0, 1.0).asin() / std::f32::consts::PI + 0.5) * Self::H as f32 - 0.5;
            let (x0, y0) = (fx.floor(), fy.floor());
            let (tx, ty) = (fx - x0, fy - y0);
            let mut to = 0.0;
            for (dx, dy, w) in [(0, 0, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)] {
                let x = (x0 as i64 + dx).rem_euclid(Self::W as i64) as usize;
                let y = (y0 as i64 + dy).clamp(0, Self::H as i64 - 1) as usize;
                to += w * l.min((self.r[y * Self::W + x] - gap).max(0.0));
            }
            if to < l - 1e-5 {
                let k = to / l;
                *p = [self.centre[0] + d[0] * k, self.centre[1] + d[1] * k, self.centre[2] + d[2] * k];
                n += 1;
            }
        }
        n
    }
}

/// Strands interpolated between the guides (`Groom::coat`).
pub struct Coat {
    /// Points per strand.
    pub k: usize,
    /// n x k points, strand-major (RAGE space).
    pub points: Vec<[f32; 3]>,
    /// Per strand: its three guide strands and their weights.
    pub guides: Vec<([u32; 3], [f32; 3])>,
    /// Per strand: unit normal of its root triangle.
    pub normals: Vec<[f32; 3]>,
    /// Per strand: its 2-bit variant (which of four colour ramps the game's
    /// strand shader takes for it).
    pub variants: Vec<u8>,
    /// Per strand: its clump (1-based, numbered in the order grown; 0 for a
    /// scattered strand). In mode 16 the clump is the guide strand + 1.
    pub clumps: Vec<u32>,
    /// Strands the groom stores for the whole piece (sum of c0 + c1).
    pub stored: usize,
}

/// Cheap test on the system page alone.
pub fn is_groom(sys: &[u8]) -> bool {
    sys.len() >= 0xE0 && sys[0x1A] == TYPE_GROOM && sys.get(0x1C..0x20) == Some(&GROOM_TAG.to_le_bytes()[..])
}

pub fn f16(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((h >> 10) & 0x1F) as i32;
    let m = (h & 0x3FF) as f32;
    s * match e {
        0 => m / 1024.0 * 2f32.powi(-14),
        31 => f32::INFINITY,
        _ => (1.0 + m / 1024.0) * 2f32.powi(e - 15),
    }
}

fn v3(r: &Resource, o: usize) -> [f32; 3] {
    [r.f32(o), r.f32(o + 4), r.f32(o + 8)]
}

/// Decode a groom from a loaded resource (system page and graphics pages).
pub fn parse(r: &Resource) -> Result<Groom> {
    if !is_groom(&r.sys) {
        bail!("not a groom resource");
    }
    let desc = r.sys_off(r.u64(0x70)).ok_or_else(|| anyhow!("no buffer descriptor"))?;
    let base = r.gfx_off(r.u64(desc + 0x18)).ok_or_else(|| anyhow!("no graphics buffer"))?;
    let g = r.gfx.get(base..).ok_or_else(|| anyhow!("graphics buffer outside the pages"))?;
    let gu16 = |o: usize| g.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).unwrap_or(0);
    let gu32 = |o: usize| g.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0);
    let s = gu16(0x58) as usize;
    let t = gu16(0x5A) as usize;
    let k = gu32(0x5C) as usize;
    if s == 0 || k == 0 || k > 1024 {
        bail!("unexpected groom header (S {s}, K {k})");
    }
    let b = |i: usize| gu32(0x30 + 4 * i) as usize;
    let (b0, b4, b5) = (b(0), b(4), b(5));
    let pts = g.get(b5..b5 + s * k * 8).ok_or_else(|| anyhow!("point section outside the buffer"))?;
    let mut points = Vec::with_capacity(s * k);
    let mut w = Vec::with_capacity(s * k);
    for p in pts.chunks_exact(8) {
        let h = |i: usize| f16(u16::from_le_bytes([p[2 * i], p[2 * i + 1]]));
        points.push([h(0), h(1), h(2)]);
        w.push(h(3));
    }
    let mut roots = Vec::with_capacity(t);
    let mut tri_counts = Vec::new();
    if t > 0 && b4 > 0 {
        let tri = g.get(b4..b4 + t * 6).ok_or_else(|| anyhow!("root triangles outside the buffer"))?;
        let b6 = b(6);
        let counts = if b6 > 0 { g.get(b6..b6 + t * 4) } else { None };
        for (n, c) in tri.chunks_exact(6).enumerate() {
            let i = |j: usize| u16::from_le_bytes([c[2 * j], c[2 * j + 1]]) as u32;
            let v = [i(0), i(1), i(2)];
            if v.iter().all(|&x| (x as usize) < s) {
                roots.push(v);
                if let Some(q) = counts {
                    tri_counts.push([q[4 * n], q[4 * n + 1], q[4 * n + 2], q[4 * n + 3]]);
                }
            }
        }
    }
    let a2 = gu32(0x08) as usize;
    let root_uv = match g.get(a2..a2 + s * 4) {
        Some(u) if a2 > 0 => u.chunks_exact(4).map(|c| [f16(u16::from_le_bytes([c[0], c[1]])), f16(u16::from_le_bytes([c[2], c[3]]))]).collect(),
        _ => Vec::new(),
    };
    let lengths = match g.get(b0..b0 + s * 2) {
        Some(l) if b0 > 0 => l.chunks_exact(2).map(|c| f16(u16::from_le_bytes([c[0], c[1]]))).collect(),
        _ => Vec::new(),
    };
    let nb = r.u32(0x98) as usize;
    let bones = match r.sys_off(r.u64(0xA0)) {
        Some(o) if nb <= 4096 => (0..nb).map(|i| r.u32(o + 4 * i)).collect(),
        _ => Vec::new(),
    };
    let root_box = [v3(r, 0x30), v3(r, 0x40)];
    let gb = [v3(r, 0x50), v3(r, 0x60)];
    let group_box = (gb[0][0] <= gb[1][0] && gb[0][0].is_finite() && gb[1][0].is_finite() && gb[0][0].abs() < 1e30).then_some(gb);
    let width = Some(f16(r.u16(0xF0))).filter(|w| w.is_finite() && *w > 0.0 && *w < 0.01).unwrap_or(0.0);
    let taper: [u8; 16] = r.sys.get(0x110..0x120).and_then(|b| b.try_into().ok()).unwrap_or([255; 16]);
    // strand styles: records of 8 + 4N bytes from header +0x20 up to the
    // next section; a record ends where the arc position starts again at 0
    let st = gu32(0x20) as usize;
    let mut styles = Vec::new();
    if st > 0 {
        let end = (0..0x50).step_by(4).map(|o| gu32(o) as usize).filter(|&o| o > st).min().unwrap_or(g.len()).min(g.len());
        let mut o = st;
        while o + 12 <= end && styles.len() < 4 {
            let lo = [f16(gu16(o)), f16(gu16(o + 2))];
            let hi = [f16(gu16(o + 4)), f16(gu16(o + 6))];
            let mut samples = Vec::new();
            let mut q = o + 8;
            while q + 4 <= end {
                let v = gu32(q);
                if !samples.is_empty() && (v >> 20) < (samples[samples.len() - 1] >> 20) {
                    break;
                }
                samples.push(v);
                q += 4;
            }
            if !(lo.iter().chain(&hi).all(|x| x.is_finite())) || samples.len() < 2 {
                break;
            }
            // every record of a piece has the same sample count
            if styles.first().is_some_and(|f: &Style| f.samples.len() != samples.len()) {
                break;
            }
            styles.push(Style { lo, hi, samples });
            o = q;
        }
    }
    let a0 = gu32(0x00) as usize;
    let corners = match g.get(a0..a0 + s * 4) {
        Some(p) if a0 > 0 => p.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect(),
        _ => Vec::new(),
    };
    let a1 = gu32(0x04) as usize;
    let guide_tri = match g.get(a1..a1 + s * 4) {
        Some(p) if a1 > 0 => p.chunks_exact(4).map(|c| u16::from_le_bytes([c[0], c[1]]) as u32).collect(),
        _ => Vec::new(),
    };
    let a3 = gu32(0x0C) as usize;
    let pick_steps = match g.get(a3..a3 + s * 4) {
        Some(p) if a3 > 0 => p.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect(),
        _ => Vec::new(),
    };
    // style frame angles: 4 bytes per guide
    let tw = gu32(0x24) as usize;
    let twist: Vec<[u8; 4]> = if tw > 0 { g.get(tw..tw + s * 4).map_or(Vec::new(), |b| b.chunks_exact(4).map(|q| [q[0], q[1], q[2], q[3]]).collect()) } else { Vec::new() };
    let style_at = |o: usize| {
        let v = |i: usize| Some(f16(r.u16(o + 2 * i))).filter(|x| x.is_finite() && x.abs() < 1e4).unwrap_or(0.0);
        crate::hairgen::Style { bend: v(0), fade: v(1), noise: v(2), curl: v(3), noise_freq: v(4), curl_freq: v(5) }
    };
    let style = if r.sys.len() >= 0x110 { [style_at(0xF8), style_at(0x104)] } else { Default::default() };
    Ok(Groom { tint_weight: r.sys.get(0x12C).map_or(1.0, |b| *b as f32 / 255.0), strands: s, points_per_strand: k, points, w, roots, lengths, root_uv, bones, root_box, group_box, tri_counts, width, taper, pick_steps, seed: gu32(0x7C), length_var: [f16(gu16(0x60)), f16(gu16(0x62))], head: g.get(..0xC0).map_or(Vec::new(), |h| h.to_vec()), corners, clump_scale: [0x12A, 0x12B].map(|o| r.sys.get(o).map_or(1.0, |b| *b as f32 / 255.0)), styles, clump_type: r.sys.get(0x12D).copied().unwrap_or(0), guide_tri, twist, curl: style })
}

impl Groom {
    /// Line-segment indices (pairs) along every strand.
    pub fn segments(&self) -> Vec<u32> {
        let k = self.points_per_strand as u32;
        let mut v = Vec::with_capacity(self.strands * (k as usize).saturating_sub(1) * 2);
        for s in 0..self.strands as u32 {
            for j in 0..k.saturating_sub(1) {
                v.push(s * k + j);
                v.push(s * k + j + 1);
            }
        }
        v
    }
    /// Root triangles as point indices (the first point of each strand).
    pub fn root_indices(&self) -> Vec<u32> {
        let k = self.points_per_strand as u32;
        self.roots.iter().flat_map(|t| t.map(|i| i * k)).collect()
    }
    /// Root UV of each point's strand (zeros when absent).
    pub fn point_uvs(&self) -> Vec<[f32; 2]> {
        let k = self.points_per_strand;
        (0..self.strands * k).map(|i| self.root_uv.get(i / k).copied().unwrap_or([0.0, 0.0])).collect()
    }

    /// Display mesh: two crossed ribbons along every strand (RAGE space).
    /// `half_width` (metres) at the root, tapering to 40 % at the tip; the
    /// taper is a display choice, not from the data. Returns positions,
    /// normals, root UVs and triangle indices.
    pub fn ribbons(&self, half_width: f32) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>) {
        let k = self.points_per_strand;
        let n = self.strands * k * 4;
        let (mut pos, mut nrm, mut uv, mut idx) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::new());
        let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let norm = |a: [f32; 3]| {
            let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            if l > 1e-9 { [a[0] / l, a[1] / l, a[2] / l] } else { [0.0, 0.0, 1.0] }
        };
        for s in 0..self.strands {
            let uvs = self.root_uv.get(s).copied().unwrap_or([0.0, 0.0]);
            let base = pos.len() as u32;
            for j in 0..k {
                let p = self.points[s * k + j];
                let t = if k < 2 { [0.0, 0.0, 1.0] } else { norm(sub(self.points[s * k + (j + 1).min(k - 1)], self.points[s * k + j.saturating_sub(1)])) };
                let up = if t[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
                let a = norm(cross(t, up));
                let b = norm(cross(t, a));
                let r = half_width * (1.0 - 0.6 * j as f32 / (k.max(2) - 1) as f32);
                for (side, nv) in [(a, b), (b, a)] {
                    for sg in [-1.0f32, 1.0] {
                        pos.push([p[0] + side[0] * r * sg, p[1] + side[1] * r * sg, p[2] + side[2] * r * sg]);
                        nrm.push(nv);
                        uv.push(uvs);
                    }
                }
            }
            for j in 0..k.saturating_sub(1) as u32 {
                for q in 0..2u32 {
                    let v0 = base + j * 4 + q * 2;
                    let v1 = v0 + 4;
                    idx.extend_from_slice(&[v0, v0 + 1, v1 + 1, v0, v1 + 1, v1]);
                }
            }
        }
        (pos, nrm, uv, idx)
    }

    /// Ribbon GLB with one plain material of colour `rgb` (linear).
    pub fn ribbon_glb(&self, name: &str, half_width: f32, rgb: [f32; 3], extras: serde_json::Value) -> Vec<u8> {
        let (positions, normals, uv, indices) = self.ribbons(half_width);
        let prim = crate::gltf::TriPrim { name: name.to_string(), positions, normals: Some(normals), tangents: None, uvs: vec![uv], colors: None, colors1: None, regions: None, tint: None, indices, material: Some(0), joints: None, weights: None, joints1: None, weights1: None, morphs: Vec::new() };
        let mat = crate::gltf::Material {
            name: "hair".into(),
            base_color_uri: None,
            normal_uri: None,
            gltf: serde_json::json!({"doubleSided": true, "pbrMetallicRoughness": {"baseColorFactor": [rgb[0], rgb[1], rgb[2], 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.55}}),
            extras: serde_json::json!({"note": "display ribbons; colour chosen, not from the data"}),
        };
        crate::gltf::mesh_glb(name, &[crate::gltf::NodeGroup { name: name.to_string(), prims: vec![prim] }], &[mat], &[], extras)
    }

    /// Strands the groom stores for the piece: the sum of c0 + c1 over the
    /// root triangles (0 without B6).
    pub fn stored_strands(&self) -> usize {
        self.tri_counts.iter().map(|c| c[0] as usize + c[1] as usize).sum()
    }

    /// The coat the game grows from the guides, by the arithmetic of its
    /// strand generation program (`hairgen`). Two populations:
    ///
    /// - Scattered strands (c0 per triangle): strand `i` picks its root
    ///   triangle by hash, weighted by c0, takes a hashed point in it, and
    ///   is the blend of the triangle's three guides cut at its own length.
    /// - Clumped strands (c1 per triangle, in c2 clumps): each clump has a
    ///   hashed centre point in the triangle whose guide blend G(t) is the
    ///   clump's centre line. A member starts at a random offset T (uniform
    ///   in the square [-1, 1]^2) of the clump radius R (per-guide byte 1,
    ///   mm) in the root triangle's plane (axes: the centre line's heading
    ///   and its cross with the normal, fixed for the whole strand), and is
    ///   walked toward G(t) + (1 - p(t)) R T with the centre line's segment
    ///   lengths, where
    ///   p is the 16-step pull profile at graphics header +0x90 (255 = on
    ///   the centre line). Read from the program's clump variant by the
    ///   shader archive work (medium confidence); the hashes and the offset
    ///   table are the viewer's own, with the same spread. The two axes
    ///   are scaled by `clump_scale` (hair modifiers that change it at run
    ///   time are not applied). With graphics header +0xA8 = 3
    ///   the game takes the root normal from a per-vertex table at +0xBC
    ///   when that offset is set; it does not change the clump shape, and
    ///   the viewer keeps the triangle's normal.
    ///
    /// A piece without the clump bytes grows its clumped strands on their
    /// centre lines. `share` of each population is grown: the game's own
    /// count is a run-time number, and so is the scale of the density byte
    /// (here the densest triangle always takes a strand). Same strands for
    /// the same groom and share. Empty without B6.
    ///
    /// Every strand then gets the game's noise and curl
    /// (`hairgen::apply_style`, the groom's first curl group) on its grown
    /// or walked points, with its own seed and length draw; the root
    /// triangle's normal stands in for the game's twist vector. The style
    /// record pass comes after it.
    pub fn coat(&self, share: f32) -> Coat {
        self.coat_with(share, None)
    }

    /// `coat`, with the style pass damped against `colliders`.
    pub fn coat_with(&self, share: f32, colliders: Option<&Colliders>) -> Coat {
        use crate::hairgen;
        let k = self.points_per_strand;
        let mut c = Coat { k, points: Vec::new(), guides: Vec::new(), normals: Vec::new(), variants: Vec::new(), clumps: Vec::new(), stored: self.stored_strands() };
        let share = share.clamp(0.0, 1.0);
        if share <= 0.0 || k == 0 || self.tri_counts.len() != self.roots.len() || self.roots.is_empty() {
            return c;
        }
        let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let scale = |a: [f32; 3], f: f32| [a[0] * f, a[1] * f, a[2] * f];
        let unit = |a: [f32; 3], or: [f32; 3]| {
            let l = dot(a, a).sqrt();
            if l > 1e-12 { scale(a, 1.0 / l) } else { or }
        };
        // the triangle's normal and the blended guide curve at weights w
        let normal = |t: [u32; 3]| {
            let g = t.map(|x| x as usize * k);
            unit(cross(sub(self.points[g[1]], self.points[g[0]]), sub(self.points[g[2]], self.points[g[0]])), [0.0, 0.0, 1.0])
        };
        let blend = |t: [u32; 3], w: [f32; 3], j: usize| {
            let g = t.map(|x| x as usize * k);
            let (a, b, d) = (self.points[g[0] + j], self.points[g[1] + j], self.points[g[2] + j]);
            [a[0] * w[0] + b[0] * w[1] + d[0] * w[2], a[1] * w[0] + b[1] * w[1] + d[1] * w[2], a[2] * w[0] + b[2] * w[1] + d[2] * w[2]]
        };
        let mix = |t: [u32; 3], w: [f32; 3], f: &dyn Fn(usize) -> f32| f(t[0] as usize) * w[0] + f(t[1] as usize) * w[1] + f(t[2] as usize) * w[2];
        // without stored lengths the strand is not cut
        let length = |t: [u32; 3], w: [f32; 3], d: f32| if self.lengths.len() == self.strands { hairgen::length(mix(t, w, &|s| self.lengths[s]), d, self.length_var) } else { f32::MAX };
        let steps = |t: [u32; 3], w: [f32; 3]| [0usize, 1, 2].map(|q| mix(t, w, &|s| self.pick_steps.get(s).map_or([64u8, 128, 191][q], |b| b[q]) as f32) / 255.0);

        // the style pass: the stored style shape laid over every generated
        // strand in a frame that turns along it. The strand's style is picked by the generator (not read); here by
        // its variant draw among the stored records. The collider damping
        // (`Colliders`) keeps the offsets small near the skin.
        let swap = std::env::var_os("SIXNG_STYLE_SWAP").is_some();
        // On by default where the piece stores its frame angles (header
        // +0x24), at the game's normal strength 1. SIXNG_STYLE=<k> sets the
        // strength (0 = off) and also applies it to pieces without the
        // table (frame: the root normal, not the game's).
        let k_style = std::env::var("SIXNG_STYLE").ok().map(|v| v.parse::<f32>().unwrap_or(1.0).clamp(0.0, 1.0));
        let strength = k_style.unwrap_or(1.0);
        let off = strength <= 0.0 || (k_style.is_none() && self.twist.len() != self.strands);
        // per point: damped near the colliders (none: the strength alone)
        let weight = |p: [f32; 3]| colliders.map_or(strength, |c| c.weight(p, strength));
        // the generator's noise and curl (feature 128), on the output points
        let curl = self.curl[0];
        // The frame axis, as the game's generator writes it into every
        // strand point: a direction in the root triangle's plane (the game
        // takes one of its 64 clump offsets by a strand hash; here an angle
        // from the strand's draw), turned about the strand by an angle
        // interpolated at 3u between four knots, each the root guides'
        // blend of their +0x24 bytes. Without the table: the root normal.
        // SIXNG_STYLE_TWIST=-1 turns the other way.
        let twist_sign = std::env::var("SIXNG_STYLE_TWIST").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
        let style = |pts: &mut [[f32; 3]], nr: [f32; 3], d: f32, guides: &[(usize, f32)], frame: Option<[f32; 3]>| {
            if off || self.styles.is_empty() {
                return;
            }
            let i = ((d * self.styles.len() as f32) as usize).min(self.styles.len() - 1);
            if self.twist.len() != self.strands {
                self.styles[i].apply(pts, &|_| nr, swap, &weight);
                return;
            }
            let knots = [0usize, 1, 2, 3].map(|q| guides.iter().map(|&(g, w)| w * (self.twist[g][q] as f32 * (std::f32::consts::TAU / 255.0) - std::f32::consts::PI)).sum::<f32>());
            let e1 = unit(cross(nr, if nr[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] }), [1.0, 0.0, 0.0]);
            let e2 = cross(nr, e1);
            let phi = (d * 7919.0).fract() * std::f32::consts::TAU;
            let r0 = frame.unwrap_or([e1[0] * phi.cos() + e2[0] * phi.sin(), e1[1] * phi.cos() + e2[1] * phi.sin(), e1[2] * phi.cos() + e2[2] * phi.sin()]);
            let tangents: Vec<[f32; 3]> = {
                let n = pts.len();
                (0..n).map(|j| unit(sub(pts[(j + 1).min(n - 1)], pts[j.saturating_sub(1)]), nr)).collect()
            };
            let axis = |z: f32| {
                let x = z * 3.0;
                let s = (x as usize).min(2);
                let th = twist_sign * (knots[s] + (knots[s + 1] - knots[s]) * (x - s as f32));
                let t = tangents[((z * (tangents.len() - 1) as f32).round() as usize).min(tangents.len() - 1)];
                let n = unit(sub(r0, scale(t, dot(r0, t))), nr);
                let b = cross(t, n);
                [n[0] * th.cos() + b[0] * th.sin(), n[1] * th.cos() + b[1] * th.sin(), n[2] * th.cos() + b[2] * th.sin()]
            };
            self.styles[i].apply(pts, &axis, swap, &weight);
        };
        // scattered strands
        let density: Vec<u8> = self.tri_counts.iter().map(|q| q[0]).collect();
        let top = density.iter().copied().max().unwrap_or(0);
        let n = (density.iter().map(|&d| d as f32).sum::<f32>() * share).round() as usize;
        if top > 0 {
            for i in 0..n as u32 {
                let (ti, v20) = hairgen::pick(i, &density, 1.0 / top as f32);
                let t = self.roots[ti];
                let (seed, w) = hairgen::seed_bary(ti as u32, self.seed, v20);
                let mut draws = hairgen::Draws(seed);
                let (d_len, _, d_var) = (draws.next(), draws.next(), draws.next());
                let at = c.points.len();
                c.points.extend(hairgen::grow(k, length(t, w, d_len), |j| blend(t, w, j)));
                hairgen::apply_style(&mut c.points[at..], &curl, seed, d_len, normal(t));
                style(&mut c.points[at..], normal(t), d_var, &[(t[0] as usize, w[0]), (t[1] as usize, w[1]), (t[2] as usize, w[2])], None);
                c.normals.push(normal(t));
                c.guides.push((t, w));
                c.variants.push(hairgen::variant(d_var, steps(t, w)));
                c.clumps.push(0);
            }
        }

        // clumped strands
        let profile: [f32; 16] = match self.head.get(0x90..0xA0) {
            Some(b) => std::array::from_fn(|i| b[i] as f32 / 255.0),
            None => [1.0; 16],
        };
        let pull = |f: f32| {
            let x = f.clamp(0.0, 1.0) * 15.0;
            let i = (x as usize).min(14);
            profile[i] + (profile[i + 1] - profile[i]) * (x - i as f32)
        };
        // a member: starts at the centre line's root + (1 - p(0)) R T and is walked toward
        // centre(s) + (1 - p(s)) R T with the centre line's segment lengths, cut at `len`
        #[allow(clippy::too_many_arguments)]
        let walk = |out: &mut Vec<[f32; 3]>, centre: &[[f32; 3]], fa: [f32; 3], fb: [f32; 3], radius: f32, tx: f32, ty: f32, len: f32, nr: [f32; 3]| {
            let total: f32 = centre.windows(2).map(|p| { let e = sub(p[1], p[0]); dot(e, e).sqrt() }).sum::<f32>().max(1e-9);
            // one world offset per member, scaled down the strand by the pull profile
            let off = |f: f32| {
                let r = (1.0 - pull(f)) * radius;
                [r * (tx * fa[0] + ty * fb[0]), r * (tx * fa[1] + ty * fb[1]), r * (tx * fa[2] + ty * fb[2])]
            };
            let o = off(0.0);
            let mut p = [centre[0][0] + o[0], centre[0][1] + o[1], centre[0][2] + o[2]];
            out.push(p);
            let (mut arc, mut used) = (0.0f32, 0.0f32);
            for j in 1..centre.len() {
                let e = sub(centre[j], centre[j - 1]);
                let seg = dot(e, e).sqrt();
                arc += seg;
                let o = off(arc / total);
                let target = [centre[j][0] + o[0], centre[j][1] + o[1], centre[j][2] + o[2]];
                let step = seg.min((len - used).max(0.0));
                let step = if step > 0.000_976_562_5 { step } else { 0.0 };
                used += step;
                let dir = unit(sub(target, p), unit(e, nr));
                p = [p[0] + dir[0] * step, p[1] + dir[1] * step, p[2] + dir[2] * step];
                out.push(p);
            }
        };
        let clump_seed = self.head.get(0x80..0x84).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])) ^ self.seed;
        let mode16 = self.clump_type == 1 && self.guide_tri.len() == self.strands && self.corners.len() == self.strands && self.strands > 0;
        if mode16 {
            // Mode 16: one strand per thread. Thread m picks guide idx by hash; its clump centre line is
            // guide idx itself, so the strand starts at C = root(idx) + (1 - p(0)) R T and follows the guide's
            // shape, pulled back toward it by the profile. The offset axes: A = the surface direction of the
            // root triangle's u axis (from the corner guides' root UVs), B = A x N. R = the guide's byte 1 (mm).
            let total: u32 = self.tri_counts.iter().map(|q| q[1] as u32).sum();
            let n16 = (total as f32 * share).round() as u32;
            for m in 0..n16 {
                let mseed = clump_seed ^ m.wrapping_mul(0x9e37_79b9).wrapping_add(0x7f4a_7c15);
                let mut draws = hairgen::Draws(mseed);
                let idx = ((draws.next() * self.strands as f32) as usize).min(self.strands - 1);
                let Some(&t) = self.roots.get(self.guide_tri[idx] as usize) else { continue };
                let nr = normal(t);
                let centre: Vec<[f32; 3]> = self.points[idx * k..idx * k + k].to_vec();
                let head = sub(centre[1.min(k - 1)], centre[0]);
                let in_plane = |a: [f32; 3]| sub(a, scale(nr, dot(a, nr)));
                let u_axis = (self.root_uv.len() == self.strands)
                    .then(|| {
                        let (p0, p1, p2) = (self.points[t[0] as usize * k], self.points[t[1] as usize * k], self.points[t[2] as usize * k]);
                        let (u0, u1, u2) = (self.root_uv[t[0] as usize], self.root_uv[t[1] as usize], self.root_uv[t[2] as usize]);
                        let (e1, e2) = (sub(p1, p0), sub(p2, p0));
                        let (du1, dv1, du2, dv2) = (u1[0] - u0[0], u1[1] - u0[1], u2[0] - u0[0], u2[1] - u0[1]);
                        let det = du1 * dv2 - du2 * dv1;
                        (det.abs() > 1e-12).then(|| in_plane(scale(sub(scale(e1, dv2), scale(e2, dv1)), 1.0 / det)))
                    })
                    .flatten();
                let fa = unit(u_axis.unwrap_or_else(|| in_plane(head)), unit(in_plane(head), [1.0, 0.0, 0.0]));
                let fb = unit(cross(fa, nr), [0.0, 1.0, 0.0]);
                let radius = 0.001 * self.corners[idx][1] as f32;
                let (tx, ty) = ((draws.next() * 2.0 - 1.0) * self.clump_scale[0], (draws.next() * 2.0 - 1.0) * self.clump_scale[1]);
                let (d_len, d_var) = (draws.next(), draws.next());
                let len = if self.lengths.len() == self.strands { hairgen::length(self.lengths[idx], d_len, self.length_var) } else { f32::MAX };
                walk(&mut c.points, &centre, fa, fb, radius, tx, ty, len, nr);
                let at = c.points.len() - k;
                hairgen::apply_style(&mut c.points[at..], &curl, mseed, d_len, nr);
                // the style frame starts on the root frame's axis, the same for every strand of guide idx (the
                // generator carries it along from the root frame): the guide's strands curl together into one ringlet
                style(&mut c.points[at..], nr, d_var, &[(idx, 1.0)], Some(fa));
                // the strand's guide weights: all on guide idx (its colour, UV and bones)
                let w = match t.iter().position(|&x| x as usize == idx) {
                    Some(0) => [1.0, 0.0, 0.0],
                    Some(1) => [0.0, 1.0, 0.0],
                    Some(_) => [0.0, 0.0, 1.0],
                    None => [1.0 / 3.0; 3],
                };
                c.normals.push(nr);
                c.guides.push((t, w));
                c.variants.push(hairgen::variant(d_var, steps(t, w)));
                c.clumps.push(idx as u32 + 1);
            }
            return c;
        }
        let mut acc = 0.0f32;
        let mut clump_id = 0u32;
        for (ti, (&t, q)) in self.roots.iter().zip(&self.tri_counts).enumerate() {
            let (members, clumps) = (q[1] as u32, q[2].max(1) as u32);
            if members == 0 {
                continue;
            }
            let nr = normal(t);
            for ck in 0..clumps {
                clump_id += 1;
                let (cseed, b) = hairgen::seed_bary(ti as u32, clump_seed, ck.wrapping_mul(0x9e37_79b9) ^ 0x5bd1_e995);
                let radius = 0.001 * mix(t, b, &|s| self.corners.get(s).map_or(0.0, |c| c[1] as f32));
                let centre: Vec<[f32; 3]> = (0..k).map(|j| blend(t, b, j)).collect();
                // the member frame, fixed for the whole strand: A = the centre line's
                // first segment carried into the root triangle's plane, B = A x N
                let head = sub(centre[1.min(k - 1)], centre[0]);
                let fa = unit(sub(head, scale(nr, dot(head, nr))), { let g0 = self.points[t[0] as usize * k]; unit(sub(self.points[t[1] as usize * k], g0), [1.0, 0.0, 0.0]) });
                let fb = unit(cross(fa, nr), [0.0, 1.0, 0.0]);
                let m = members / clumps + u32::from(ck < members % clumps);
                for mi in 0..m {
                    acc += share;
                    if acc < 1.0 {
                        continue;
                    }
                    acc -= 1.0;
                    let mseed = cseed.wrapping_add(mi.wrapping_mul(0x2545_f491));
                    let mut draws = hairgen::Draws(mseed);
                    let (tx, ty) = ((draws.next() * 2.0 - 1.0) * self.clump_scale[0], (draws.next() * 2.0 - 1.0) * self.clump_scale[1]);
                    let (d_len, d_var) = (draws.next(), draws.next());
                    let len = length(t, b, d_len);
                    walk(&mut c.points, &centre, fa, fb, radius, tx, ty, len, nr);
                    let at = c.points.len() - k;
                    hairgen::apply_style(&mut c.points[at..], &curl, mseed, d_len, nr);
                    style(&mut c.points[at..], nr, d_var, &[(t[0] as usize, b[0]), (t[1] as usize, b[1]), (t[2] as usize, b[2])], None);
                    c.normals.push(nr);
                    c.guides.push((t, b));
                    c.variants.push(hairgen::variant(d_var, steps(t, b)));
                    c.clumps.push(clump_id);
                }
            }
        }
        c
    }

    /// Width (metres) of a strand at point `j` of `k`: the stored width
    /// (0.3 mm when the groom stores none) times the stored 16-step taper
    /// from root to tip (full width when the taper is all zero).
    pub fn width_at(&self, j: usize, k: usize) -> f32 {
        let base = if self.width > 0.0 { self.width } else { 0.0003 };
        if self.taper.iter().all(|&b| b == 0) {
            return base;
        }
        let f = j as f32 / (k.max(2) - 1) as f32;
        base * self.taper[((f * 15.0) as usize).min(15)] as f32 / 255.0
    }

    /// The full strand set as line strips, for export: the coat's points,
    /// each point's width, each strand's root UV (blended from its three
    /// guides) and the segment index pairs.
    pub fn coat_lines(&self, coat: &Coat) -> (Vec<f32>, Vec<[f32; 2]>, Vec<u32>) {
        let (k, n) = (coat.k, coat.guides.len());
        let (mut width, mut uv, mut seg) = (Vec::with_capacity(n * k), Vec::with_capacity(n * k), Vec::with_capacity(n * k.saturating_sub(1) * 2));
        for (s, (g, w)) in coat.guides.iter().enumerate() {
            let at = |i: u32| self.root_uv.get(i as usize).copied().unwrap_or([0.0, 0.0]);
            let (a, b, d) = (at(g[0]), at(g[1]), at(g[2]));
            let u = [a[0] * w[0] + b[0] * w[1] + d[0] * w[2], a[1] * w[0] + b[1] * w[1] + d[1] * w[2]];
            for j in 0..k {
                width.push(self.width_at(j, k));
                uv.push(u);
                if j + 1 < k {
                    seg.extend_from_slice(&[(s * k + j) as u32, (s * k + j + 1) as u32]);
                }
            }
        }
        (width, uv, seg)
    }

    /// What a rebuild of the full strand set needs beside the guides: the
    /// root triangles (guide strand indices) with the strands the game
    /// grows in each, the strand width and its taper.
    pub fn growth_json(&self) -> serde_json::Value {
        serde_json::json!({
            "guide_strands": self.strands,
            "points_per_strand": self.points_per_strand,
            "root_triangles": self.roots,
            "triangle_counts": self.tri_counts,
            "triangle_counts_note": "per root triangle [c0, c1, c2, c3]: c0 = strands grown in the triangle, c1 = strands in clumps (inferred), c2 = clumps (inferred), c3 unknown. Lower LODs saturate at 255",
            "stored_strands": self.stored_strands(),
            "strand_width_m": self.width,
            "taper": self.taper,
            "curl": self.curl,
            "curl_note": "system +0xF8 and +0x104: unread word, fade-in length, noise amplitude, curl radius, noise and curl frequency; the game's generator reads the first group for every strand, the second is not read by any known program",
            "taper_note": "16 width steps from root to tip, each / 255 of the strand width (inferred)",
            "method": "per root triangle, c0 + c1 strands at random barycentric points; every point of a strand is the same barycentric blend of the three guide strands' points. Noise and curl from the groom's first style group (the root triangle normal stands in for the game's twist vector); clumping is not decoded",
        })
    }

    /// GLB of a coat: one flat ribbon per strand, lying in the skin's
    /// tangent plane, `width` metres wide at the root and tapered by the
    /// stored 16 steps. `guide_rgb` (linear, one per guide strand) colours
    /// each strand by its barycentric weights (COLOR_0 on a white
    /// material); strands are white where it is short.
    pub fn coat_glb(&self, name: &str, coat: &Coat, width: f32, guide_rgb: &[[u8; 3]], extras: serde_json::Value) -> Vec<u8> {
        self.coat_glb_with(name, coat, width, &|s, _| {
            let (g, w) = coat.guides[s];
            let mut c = [255u8; 4];
            if let (Some(a), Some(b), Some(d)) = (guide_rgb.get(g[0] as usize), guide_rgb.get(g[1] as usize), guide_rgb.get(g[2] as usize)) {
                for q in 0..3 {
                    c[q] = (a[q] as f32 * w[0] + b[q] as f32 * w[1] + d[q] as f32 * w[2] + 0.5) as u8;
                }
            }
            c
        }, extras)
    }

    /// `coat_glb` with the colour (linear) of strand `s` at `t` along it
    /// (0 root, 1 tip) given by `colour`: for hair whose colour changes
    /// from root to tip.
    pub fn coat_glb_with(&self, name: &str, coat: &Coat, width: f32, colour: &dyn Fn(usize, f32) -> [u8; 4], extras: serde_json::Value) -> Vec<u8> {
        self.coat_glb_ramped(name, coat, width, colour, None, extras)
    }

    /// `coat_glb_with`, and with `params` each vertex carries what the
    /// game's strand shader reads from its parameter ramp at that point of
    /// the strand: TEXCOORD_0 = (g0, fill of the unlit side), TEXCOORD_1 =
    /// (profile scale, 1). Without it both are left out.
    pub fn coat_glb_ramped(&self, name: &str, coat: &Coat, width: f32, colour: &dyn Fn(usize, f32) -> [u8; 4], params: Option<&dyn Fn(usize, f32) -> [f32; 6]>, extras: serde_json::Value) -> Vec<u8> {
        let k = coat.k;
        let n = coat.guides.len();
        let (mut uv0, mut uv1): (Vec<[f32; 2]>, Vec<[f32; 2]>) = (Vec::new(), Vec::new());
        let mut tan: Vec<[f32; 4]> = Vec::with_capacity(coat.guides.len() * coat.k * 2);
        let (mut pos, mut nrm, mut tint, mut idx) = (Vec::with_capacity(n * k * 2), Vec::with_capacity(n * k * 2), Vec::with_capacity(n * k * 2), Vec::with_capacity(n * k.saturating_sub(1) * 6));
        let flat_taper = self.taper.iter().all(|&b| b == 0);
        for s in 0..n {
            let nr = coat.normals[s];
            let base = pos.len() as u32;
            for j in 0..k {
                let p = coat.points[s * k + j];
                let (a, b) = (coat.points[s * k + j.saturating_sub(1)], coat.points[s * k + (j + 1).min(k - 1)]);
                let t = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                // the strand's direction, for the hair lighting
                let tl = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
                let dir = if tl > 1e-12 { [t[0] / tl, t[1] / tl, t[2] / tl, 1.0] } else { [0.0, 0.0, 1.0, 1.0] };
                // across the strand, in the skin's plane
                let mut side = [t[1] * nr[2] - t[2] * nr[1], t[2] * nr[0] - t[0] * nr[2], t[0] * nr[1] - t[1] * nr[0]];
                let l = (side[0] * side[0] + side[1] * side[1] + side[2] * side[2]).sqrt();
                side = if l > 1e-12 { [side[0] / l, side[1] / l, side[2] / l] } else { [1.0, 0.0, 0.0] };
                let f = j as f32 / (k.max(2) - 1) as f32;
                let tap = if flat_taper { 1.0 - 0.6 * f } else { (self.taper[((f * 15.0) as usize).min(15)] as f32 / 255.0).max(0.15) };
                let c = colour(s, f);
                let pv = params.map(|p| p(s, f));
                // with parameters: their own width at this point (metres), in place of the groom's width and taper
                let r = pv.map_or(width * 0.5 * tap, |v| v[3] * 0.5);
                for sg in [-1.0f32, 1.0] {
                    // with parameters the ribbon is left closed: the viewer opens it towards the eye, at least a pixel wide
                    let rr = if pv.is_some() { 0.0 } else { r };
                    pos.push([p[0] + side[0] * rr * sg, p[1] + side[1] * rr * sg, p[2] + side[2] * rr * sg]);
                    nrm.push(nr);
                    // w: which edge of the ribbon (-1 / +1), so the shader can round the normal across it
                    // with parameters: the magnitude carries the half width, 2 + millimetres (under 16), plus 16 x the
                    // index of the piece's minimum drawn width (the viewer's list of the LOD presets' TargetThickness)
                    tan.push([dir[0], dir[1], dir[2], match pv { Some(v) => sg * (2.0 + (r * 1000.0).min(15.9) + 16.0 * v[5]), None => sg }]);
                    if let Some(pv) = pv {
                        uv0.push([pv[0], pv[1]]);
                        // y: 1 = lit by the hair lighting; 2 + reflectance scale = by the standard lighting
                        uv1.push([pv[2], pv[4]]);
                    }
                    tint.push(c);
                }
            }
            for j in 0..k.saturating_sub(1) as u32 {
                let v0 = base + j * 2;
                idx.extend_from_slice(&[v0, v0 + 1, v0 + 3, v0, v0 + 3, v0 + 2]);
            }
        }
        let prim = crate::gltf::TriPrim { name: name.to_string(), positions: pos, normals: Some(nrm), tangents: Some(tan), uvs: if uv0.is_empty() { vec![] } else { vec![uv0, uv1] }, colors: None, colors1: None, regions: None, tint: Some(tint), indices: idx, material: Some(0), joints: None, weights: None, joints1: None, weights1: None, morphs: Vec::new() };
        let mat = crate::gltf::Material {
            name: "fur".into(),
            base_color_uri: None,
            normal_uri: None,
            gltf: serde_json::json!({"doubleSided": true, "pbrMetallicRoughness": {"baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.85}}),
            extras: serde_json::json!({"classification": {"class": "hair"}, "note": "interpolated coat: the stored per-triangle strand counts, strands blended from the three guides; noise and curl from the first curl group; placement and clumping follow the game's program with the viewer's own hashes. COLOR_0 = skin colour under the root (a display choice)"}),
        };
        crate::gltf::mesh_glb(name, &[crate::gltf::NodeGroup { name: name.to_string(), prims: vec![prim] }], &[mat], &[], extras)
    }

    /// Ribbon GLB with one colour per strand (`strand_rgb`, linear RGB,
    /// written as COLOR_0 on a white material): fur coloured from the skin
    /// under each root. Strands past the list are white.
    pub fn ribbon_glb_coloured(&self, name: &str, half_width: f32, strand_rgb: &[[u8; 3]], extras: serde_json::Value) -> Vec<u8> {
        let (positions, normals, uv, indices) = self.ribbons(half_width);
        let per = self.points_per_strand * 4;
        let tint: Vec<[u8; 4]> = (0..positions.len()).map(|i| strand_rgb.get(i / per.max(1)).map_or([255; 4], |c| [c[0], c[1], c[2], 255])).collect();
        let prim = crate::gltf::TriPrim { name: name.to_string(), positions, normals: Some(normals), tangents: None, uvs: vec![uv], colors: None, colors1: None, regions: None, tint: Some(tint), indices, material: Some(0), joints: None, weights: None, joints1: None, weights1: None, morphs: Vec::new() };
        let mat = crate::gltf::Material {
            name: "fur".into(),
            base_color_uri: None,
            normal_uri: None,
            gltf: serde_json::json!({"doubleSided": true, "pbrMetallicRoughness": {"baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.7}}),
            extras: serde_json::json!({"note": "display ribbons; COLOR_0 = the skin colour under each strand's root (a display choice, not from the groom)"}),
        };
        crate::gltf::mesh_glb(name, &[crate::gltf::NodeGroup { name: name.to_string(), prims: vec![prim] }], &[mat], &[], extras)
    }

    /// Share of strand roots inside the stored root box (a layout check).
    pub fn roots_in_box(&self) -> f32 {
        let k = self.points_per_strand;
        let eps = 1e-3;
        let n = (0..self.strands)
            .filter(|&s| {
                let p = self.points[s * k];
                (0..3).all(|a| p[a] >= self.root_box[0][a] - eps && p[a] <= self.root_box[1][a] + eps)
            })
            .count();
        n as f32 / self.strands.max(1) as f32
    }
    /// Strands with no segment longer than 4 x the strand's median segment.
    pub fn continuous(&self) -> f32 {
        let k = self.points_per_strand;
        if k < 3 {
            return 1.0;
        }
        let d = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        let n = (0..self.strands)
            .filter(|&s| {
                let mut seg: Vec<f32> = (0..k - 1).map(|j| d(self.points[s * k + j], self.points[s * k + j + 1])).collect();
                let mx = seg.iter().cloned().fold(0.0, f32::max);
                seg.sort_by(|a, b| a.partial_cmp(b).unwrap());
                mx <= 4.0 * seg[seg.len() / 2] + 1e-4
            })
            .count();
        n as f32 / self.strands.max(1) as f32
    }
}

/// Bone names (joaat of the lower-case name -> name) from the skeletons
/// of the archive's named fragments (people, animals) and a sample of head
/// drawables (facial bones).
pub fn bone_names(a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> std::collections::HashMap<u32, String> {
    use rayon::prelude::*;
    let mut heads = 0;
    let src: Vec<&crate::scan::Entry> = all
        .iter()
        .filter(|e| match (e.rtype.as_deref(), e.name.as_deref()) {
            (Some("ft"), Some(_)) => true,
            (Some("dr"), Some(n)) if n.starts_with("head_") && heads < 64 => {
                heads += 1;
                true
            }
            _ => false,
        })
        .collect();
    let lists: Vec<Vec<String>> = src
        .par_iter()
        .filter_map(|e| Resource::load_sys(&a.path, e).ok())
        .map(|r| crate::drawable::find_skeleton(&r).into_iter().map(|b| b.name).collect())
        .collect();
    let mut m = std::collections::HashMap::new();
    for n in lists.into_iter().flatten() {
        m.entry(crate::meta7a::joaat(&n)).or_insert(n);
    }
    m
}

/// One groom as listed by the catalog (system page only).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GroomInfo {
    pub off: u64,
    pub id: String,
    pub strands: u32,
    pub root_triangles: u32,
    pub bones: Vec<String>,
    pub root_box: [[f32; 3]; 2],
    /// scalp / face / brows / lashes / body / animal / other, from the bones (a guess).
    pub category: String,
    /// LOD chain: consecutive grooms with the same bones and fewer strands
    /// each time (inferred); `lod` 0 is the densest. `hairlink::HairLinks::
    /// relabel` sets both from the files' names where they can be read.
    pub chain: u32,
    pub lod: u32,
}

/// Body region from the bone names (a guess for browsing).
/// Body region of a groom from its bones; head-only grooms whose roots all
/// sit low on the front of the head (beards, moustaches) count as face.
pub fn category(bones: &[String], root_box: &[[f32; 3]; 2]) -> &'static str {
    let c = category_by_bones(bones);
    if c == "scalp" && root_box[1][2] < 0.74 && root_box[1][1] < 0.0 {
        "face"
    } else {
        c
    }
}

fn category_by_bones(bones: &[String]) -> &'static str {
    let has = |f: &dyn Fn(&str) -> bool| bones.iter().any(|b| f(b));
    if has(&|b| b.starts_with("SKEL_Tail") || b.starts_with("MH_Tail") || b.contains("_Fin") || b.contains("Paw") || b.contains("Wing")) {
        "animal"
    } else if has(&|b| b.contains("Brow") || b.contains("Lid") || b.contains("Lash")) {
        "brows / lashes"
    } else if has(&|b| b.contains("Ear") || b.contains("Cheek") || b.contains("Throat") || b.contains("Chin") || b.contains("Jaw") || b.contains("Lip")) {
        "face"
    } else if has(&|b| b == "BM_Head" || b == "SKEL_Head" || b == "SKEL_HEAD" || b.contains("Occipitalis")) {
        "scalp"
    } else if has(&|b| b.contains("Pectoralis") || b.contains("Scapula") || b.contains("Spine") || b.contains("Pelvis") || b.contains("Thigh") || b.contains("Arm")) {
        "body"
    } else {
        "other"
    }
}

/// List every groom among the archive's unnamed resources from their
/// system pages (no graphics pages read), with LOD chains.
pub fn catalog(a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> Result<Vec<GroomInfo>> {
    use rayon::prelude::*;
    let names = bone_names(a, all);
    let bone_name = |h: u32| names.get(&h).cloned().or_else(|| crate::names::lookup(h, crate::names::Ctx::Field)).unwrap_or_else(|| format!("{h:08x}"));
    let cands: Vec<&crate::scan::Entry> = all.iter().filter(|e| e.name.is_none() && e.kind == crate::classify::Kind::RageObj).collect();
    let mut v: Vec<GroomInfo> = cands
        .par_iter()
        .filter_map(|e| {
            let r = Resource::load_sys(&a.path, e).ok()?;
            if !is_groom(&r.sys) {
                return None;
            }
            let nb = r.u32(0x98) as usize;
            let bones = match r.sys_off(r.u64(0xA0)) {
                Some(o) if nb <= 4096 => (0..nb).map(|i| bone_name(r.u32(o + 4 * i))).collect(),
                _ => Vec::new(),
            };
            let root_box = [v3(&r, 0x30), v3(&r, 0x40)];
            let cat = category(&bones, &root_box).to_string();
            Some(GroomInfo { off: e.off, id: e.id.clone(), strands: r.u16(0xDE) as u32, root_triangles: r.u16(0xDC) as u32, bones, root_box, category: cat, chain: 0, lod: 0 })
        })
        .collect();
    v.sort_by_key(|g| g.off);
    let mut chain = 0;
    for i in 0..v.len() {
        let cont = i > 0 && v[i].bones == v[i - 1].bones && v[i].strands < v[i - 1].strands;
        if cont {
            v[i].lod = v[i - 1].lod + 1;
        } else {
            chain += 1;
        }
        v[i].chain = chain;
    }
    Ok(v)
}

/// The catalog, cached in `<work>/index/grooms_<archive>.json`.
pub fn catalog_cached(work: &std::path::Path, a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> Result<Vec<GroomInfo>> {
    let p = work.join("index").join(format!("grooms_{}.json", a.id.trim_end_matches(".rpf").replace(['/', '\\'], "_")));
    if let Ok(b) = std::fs::read(&p) {
        if let Ok(mut v) = serde_json::from_slice::<Vec<GroomInfo>>(&b) {
            // regions may be refined after the cache was written
            for g in &mut v {
                g.category = category(&g.bones, &g.root_box).to_string();
            }
            return Ok(v);
        }
    }
    let v = catalog(a, all)?;
    std::fs::create_dir_all(p.parent().unwrap())?;
    crate::atomic::write(&p, serde_json::to_vec(&v)?)?;
    Ok(v)
}

/// Load and decode one groom by archive offset.
pub fn load(a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry], off: u64) -> Result<Groom> {
    let e = all.iter().find(|e| e.off == off).ok_or_else(|| anyhow!("no entry at {off:x}"))?;
    parse(&Resource::load(&a.path, e)?)
}

/// Export grooms to `<out>/grooms/<archive>/<offset>.glb` plus
/// `index.jsonl`: line strips (with `_WIDTH` and the root UV as
/// TEXCOORD_0), or crossed ribbons of `ribbons` half-width (metres) when
/// given. `only` limits the export to these offsets (all grooms when
/// empty). With `full` (a share of the stored strand count, 0..1) a groom
/// that stores per-triangle counts is written as the full strand set
/// grown from its guides (`Groom::coat`) instead of the guides alone, and
/// every groom gets `<offset>.growth.json` (`Groom::growth_json`). With
/// `head` (the head the grooms sit on, `hairexport::head_from_glb`) each
/// grown groom names the head UV set its root UVs are in
/// (`hairexport::tag_uv_set`).
/// Returns (grooms written, errors).
pub fn export_grooms(a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry], only: &[u64], out: &std::path::Path, roots: bool, ribbons: Option<f32>, full: Option<f32>, head: Option<&crate::hairexport::HeadUvs>, progress: crate::export::Progress) -> Result<(usize, usize)> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    // the cached list when there is a work folder (reading it anew takes minutes)
    let cat = match crate::names::work_dir() {
        Some(w) => catalog_cached(&w, a, all)?,
        None => catalog(a, all)?,
    };
    let sel: Vec<&GroomInfo> = cat.iter().filter(|g| only.is_empty() || only.contains(&g.off)).collect();
    let by_off: std::collections::HashMap<u64, &crate::scan::Entry> = all.iter().map(|e| (e.off, e)).collect();
    let dir = out.join("grooms").join(a.id.trim_end_matches(".rpf"));
    std::fs::create_dir_all(&dir)?;
    let total = sel.len() as u64;
    let done = AtomicU64::new(0);
    let errors = AtomicUsize::new(0);
    let mut rows: Vec<(u64, String, bool)> = sel
        .par_iter()
        .filter_map(|info| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&info.id, d, total);
            let e = by_off.get(&info.off)?;
            let g = match Resource::load(&a.path, e).map_err(|x| x.to_string()).and_then(|r| parse(&r).map_err(|x| x.to_string())) {
                Ok(g) => g,
                Err(msg) => {
                    errors.fetch_add(1, Ordering::Relaxed);
                    return Some((e.off, serde_json::json!({"id": info.id, "error": msg}).to_string(), false));
                }
            };
            let name = format!("groom_{:x}", e.off);
            let extras = serde_json::json!({"source": e.id, "strands": g.strands, "points_per_strand": g.points_per_strand, "bones": info.bones, "category": info.category, "chain": info.chain, "lod": info.lod,
                "coordinate_system": "model space (Z-up metres) converted to glTF Y-up (x, z, -y)"});
            // the full strand set, where the groom stores what to grow
            let coat = full.filter(|_| g.stored_strands() > 0).map(|share| g.coat(share));
            let mut extras = extras;
            if let Some(c) = &coat {
                extras["full_strands"] = serde_json::json!({"written": c.guides.len(), "stored": c.stored,
                    "note": "strands grown between the guides from the stored per-triangle counts (barycentric blend of three guides); noise and curl follow the first style group; clumping is not decoded"});
            }
            if full.is_some() && crate::atomic::write(dir.join(format!("{:x}.growth.json", e.off)), g.growth_json().to_string()).is_err() {
                errors.fetch_add(1, Ordering::Relaxed);
            }
            let bytes = match (ribbons, &coat) {
                (Some(hw), Some(c)) => g.coat_glb(&name, c, hw * 2.0, &[], extras),
                // the grown strands with their root UV, width, clump and variant (`hairexport`)
                (None, Some(_)) => {
                    let o = crate::hairexport::Options { density: full.unwrap_or(1.0), guides: false, ..Default::default() };
                    let mut ex = serde_json::json!({"source": e.id, "groom": format!("{:x}", e.off), "bones": info.bones});
                    crate::hairexport::tag_uv_set(&g, head, &mut ex);
                    let (prims, _) = crate::hairexport::piece(&g, &name, &o, None, None, ex);
                    crate::gltf::strands_glb(&name, &prims, extras)
                }
                (Some(hw), None) => g.ribbon_glb(&name, hw, [0.08, 0.05, 0.03], extras),
                (None, None) => {
                    let tris = if roots { g.root_indices() } else { Vec::new() };
                    crate::gltf::lines_glb(&name, &g.points, &g.w, &g.point_uvs(), &g.segments(), &tris, extras)
                }
            };
            let file = dir.join(format!("{:x}.glb", e.off));
            if crate::atomic::write(&file, bytes).is_err() {
                errors.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            let len: f32 = g.lengths.iter().sum();
            Some((e.off, serde_json::json!({"id": e.id, "file": format!("{:x}.glb", e.off), "strands": g.strands, "points_per_strand": g.points_per_strand,
                "stored_strands": g.stored_strands(), "written_strands": coat.as_ref().map_or(g.strands, |c| c.guides.len()), "strand_width_m": g.width,
                "root_triangles": g.roots.len(), "total_length_m": (len * 1000.0).round() / 1000.0, "bones": info.bones, "category": info.category, "chain": info.chain, "lod": info.lod,
                "root_box": g.root_box, "roots_in_box": g.roots_in_box(), "continuous": g.continuous(), "head_uv_set": head.and_then(|h| crate::hairexport::root_uv_set(&g, h)).map(|u| u.0)}).to_string(), true))
        })
        .collect();
    rows.sort_by_key(|r| r.0);
    let n = rows.iter().filter(|r| r.2).count();
    let mut text = String::new();
    for r in &rows {
        text.push_str(&r.1);
        text.push('\n');
    }
    crate::atomic::write(dir.join("index.jsonl"), text)?;
    Ok((n, errors.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_strand_set_follows_the_stored_counts() {
        // three guides of two points each, one root triangle that stores 5 + 2 strands
        let g = Groom { tint_weight: 1.0,
            strands: 3,
            points_per_strand: 2,
            points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 1.0, 0.0], [0.0, 1.0, 1.0]],
            w: vec![0.0; 6],
            roots: vec![[0, 1, 2]],
            lengths: vec![1.0; 3],
            root_uv: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            bones: vec![],
            root_box: [[0.0; 3]; 2],
            group_box: None,
            tri_counts: vec![[5, 2, 1, 0]],
            width: 0.0004,
            taper: [255, 255, 255, 255, 255, 255, 255, 255, 128, 128, 128, 128, 128, 128, 128, 128],
            pick_steps: vec![],
            seed: 0,
            length_var: [0.0, 1.0],
            curl: Default::default(),
            head: vec![],
            corners: vec![],
            clump_scale: [1.0; 2],
            styles: vec![],
            clump_type: 0,
            guide_tri: vec![],
            twist: vec![],
        };
        let c = g.coat(1.0);
        assert_eq!(c.guides.len(), 7);
        // every strand stays inside the triangle and keeps the guides' height
        for s in 0..7 {
            let (r, t) = (c.points[s * 2], c.points[s * 2 + 1]);
            assert!(r[0] >= 0.0 && r[1] >= 0.0 && r[0] + r[1] <= 1.0 + 1e-6 && r[2] == 0.0 && (t[2] - 1.0).abs() < 1e-6);
        }
        let (w, uv, seg) = g.coat_lines(&c);
        assert_eq!((w.len(), uv.len(), seg.len()), (14, 14, 14));
        assert!((w[0] - 0.0004).abs() < 1e-9 && (w[1] - 0.0004 * 128.0 / 255.0).abs() < 1e-9);
        // the root UV is the same blend as the root position
        assert!((uv[0][0] - c.points[0][0]).abs() < 1e-6 && (uv[0][1] - c.points[0][1]).abs() < 1e-6);
        assert_eq!(g.growth_json()["stored_strands"], 7);
        assert_eq!(g.coat(0.0).guides.len(), 0);
    }

    #[test]
    fn hair_is_pressed_under_a_hat() {
        // a hat: the upper half of a 10 cm sphere, sampled every 3 degrees
        let mut hat = Vec::new();
        for i in 0..30 {
            for j in 0..120 {
                let (lat, lon) = ((i as f32 * 3.0).to_radians(), (j as f32 * 3.0).to_radians());
                hat.push([0.1 * lat.cos() * lon.cos(), 0.1 * lat.cos() * lon.sin(), 1.7 + 0.1 * lat.sin()]);
            }
        }
        let s = Shell::new([0.0, 0.0, 1.7], &hat, &[]);
        // above the hat, under it already, and below its edge
        let mut pts = [[0.02, 0.01, 1.85], [0.0, 0.05, 1.74], [0.12, 0.0, 1.6]];
        assert_eq!(s.press(&mut pts, 0.003), 1);
        let r = ((pts[0][0]).powi(2) + (pts[0][1]).powi(2) + (pts[0][2] - 1.7).powi(2)).sqrt();
        assert!((r - 0.097).abs() < 1e-3, "{r}");
        assert_eq!(pts[1], [0.0, 0.05, 1.74]);
        assert_eq!(pts[2], [0.12, 0.0, 1.6]);
        // an opening in the hat that it encloses is closed
        let holed: Vec<[f32; 3]> = hat.iter().copied().filter(|p| !(p[0] > 0.05 && p[1].abs() < 0.03 && p[2] > 1.72 && p[2] < 1.76)).collect();
        let s = Shell::new([0.0, 0.0, 1.7], &holed, &[]);
        let mut through = [[0.14, 0.0, 1.75]];
        assert_eq!(s.press(&mut through, 0.003), 1);
    }

    #[test]
    fn lining_keeps_strands_from_crossing_the_hat() {
        // a dome of triangles, radius 10 cm, up to its pole
        let at = |i: usize, j: usize| {
            let (lat, lon) = ((i as f32 * 10.0).min(90.0).to_radians(), (j as f32 * 15.0).to_radians());
            [0.1 * lat.cos() * lon.cos(), 0.1 * lat.cos() * lon.sin(), 1.7 + 0.1 * lat.sin()]
        };
        let mut tris = Vec::new();
        for i in 0..9 {
            for j in 0..24 {
                tris.push([at(i, j), at(i, j + 1), at(i + 1, j + 1)]);
                tris.push([at(i, j), at(i + 1, j + 1), at(i + 1, j)]);
            }
        }
        let pts: Vec<[f32; 3]> = tris.iter().flatten().copied().collect();
        let s = Shell::new([0.0, 0.0, 1.7], &pts, &[]).with_lining(tris.clone());
        // strands of 4 points: out through the pole, through the side, and
        // one that stays inside
        let mut strands = vec![
            [0.0, 0.0, 1.75], [0.003, 0.0, 1.79], [0.006, 0.001, 1.83], [0.01, 0.002, 1.9],
            [0.05, 0.0, 1.72], [0.08, 0.01, 1.73], [0.12, 0.02, 1.74], [0.2, 0.03, 1.75],
            [0.0, 0.03, 1.72], [0.0, 0.04, 1.73], [0.0, 0.05, 1.74], [0.0, 0.06, 1.75],
        ];
        let inside = strands[8..].to_vec();
        assert!(s.press_strands(&mut strands, 4, 0.003) > 0);
        assert_eq!(&strands[8..], &inside[..]);
        // no segment crosses any triangle (brute force)
        let sub = |p: [f32; 3], q: [f32; 3]| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
        let cross = |p: [f32; 3], q: [f32; 3]| [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]];
        let dot = |p: [f32; 3], q: [f32; 3]| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
        for st in strands.chunks_exact(4) {
            for w in st.windows(2) {
                let (a, d) = (w[0], sub(w[1], w[0]));
                for [v0, v1, v2] in &tris {
                    let (e1, e2) = (sub(*v1, *v0), sub(*v2, *v0));
                    let p = cross(d, e2);
                    let det = dot(e1, p);
                    if det.abs() < 1e-14 {
                        continue;
                    }
                    let q = cross(sub(a, *v0), e1);
                    let (u, v, t) = (dot(sub(a, *v0), p) / det, dot(d, q) / det, dot(e2, q) / det);
                    assert!(!(u >= 0.0 && v >= 0.0 && u + v <= 1.0 && (0.0..=1.0).contains(&t)), "{w:?} crosses");
                }
            }
        }
    }

    #[test]
    fn style_samples_and_reshape() {
        // x 0 -> 1023 over the range -0.01 .. 0.01, y fixed at the middle, z 0 -> 4095
        let st = Style { lo: [-0.01, -0.01], hi: [0.01, 0.01], samples: vec![512 << 10, 1023 | (512 << 10) | (4095 << 20)] };
        let a = st.at(0.5);
        assert!((a[0] - 0.0).abs() < 1e-4 && a[1].abs() < 1e-4 && (a[2] - 0.5).abs() < 1e-3, "{a:?}");
        // a straight strand along -z with up = +x: the tip moves 1 cm along +x
        let mut pts = [[0.0, 0.0, 0.0], [0.0, 0.0, -0.5], [0.0, 0.0, -1.0]];
        let src = pts;
        st.apply(&mut pts, &|_| [1.0, 0.0, 0.0], false, &|_| 1.0);
        assert!((pts[2][0] - 0.01).abs() < 1e-4 && (pts[2][2] + 1.0).abs() < 1e-4 && (pts[0][0] + 0.01).abs() < 1e-4, "{pts:?}");
        // a collider on the root: the root's offset is damped to 0.8 mm, the tip's is not
        let c = Colliders::new(&[[0.0, 0.0, 0.0]]);
        assert_eq!(c.weight([0.0, 0.0, 0.0], 1.0), 0.0);
        assert_eq!(c.weight([0.0, 0.0, -0.5], 0.7), 0.7);
        let mut pts = src;
        st.apply(&mut pts, &|_| [1.0, 0.0, 0.0], false, &|p| c.weight(p, 1.0));
        assert!((pts[0][0] + 0.0008).abs() < 1e-5 && (pts[2][0] - 0.01).abs() < 1e-4, "{pts:?}");
    }

    #[test]
    fn taper_colliders() {
        // radius 0.1 at the origin, 0.05 at z = 1
        let t = Taper { a: [0.0; 3], b: [0.0, 0.0, 1.0], ra: 0.1, rb: 0.05 };
        assert!((t.dist([0.2, 0.0, 0.0]) - 0.1).abs() < 1e-6);
        assert!((t.dist([0.2, 0.0, 0.5]) - 0.125).abs() < 1e-6);
        assert!((t.dist([0.0, 0.0, 1.1]) - 0.05).abs() < 1e-6);
        assert!(t.dist([0.0, 0.0, 0.5]) < 0.0);
        let c = Colliders::shapes(vec![t]);
        // on the surface: damped fully; 1.2 cm out: d = 0.6, rounded to 2/3 -> k = 1/3
        assert_eq!(c.weight([0.1, 0.0, 0.0], 1.0), 0.0);
        assert!((c.weight([0.112, 0.0, 0.0], 1.0) - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(c.weight([0.2, 0.0, 0.0], 0.7), 0.7);
        // a box turned 90 degrees about z: half extents 0.1 along +y, 0.2 along -x, 0.3 along z
        let b = Cuboid { centre: [1.0, 0.0, 0.0], axes: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]], half: [0.1, 0.2, 0.3] };
        assert!((b.dist([1.0, 0.15, 0.0]) - 0.05).abs() < 1e-6);
        assert!((b.dist([1.25, 0.0, 0.0]) - 0.05).abs() < 1e-6);
        assert!((b.dist([1.0, 0.0, 0.0]) + 0.1).abs() < 1e-6);
        // past a corner: the distance to the corner
        assert!((b.dist([1.23, 0.14, 0.0]) - 0.05).abs() < 1e-6);
        let c = Colliders::shapes(vec![t]).with_boxes(vec![b]);
        assert_eq!(c.weight([1.0, 0.1, 0.0], 1.0), 0.0);
        assert_eq!(c.weight([0.112, 0.0, 0.0], 1.0), Colliders::shapes(vec![t]).weight([0.112, 0.0, 0.0], 1.0));
    }

    #[test]
    fn half() {
        assert_eq!(f16(0x3C00), 1.0);
        assert_eq!(f16(0xC000), -2.0);
        assert_eq!(f16(0x3555) - 0.33325195 < 1e-6, true);
    }
    #[test]
    fn segments() {
        let g = Groom { tint_weight: 1.0, strands: 2, points_per_strand: 3, points: vec![[0.0; 3]; 6], w: vec![0.0; 6], roots: vec![[0, 1, 1]], lengths: vec![], root_uv: vec![], bones: vec![], root_box: [[0.0; 3]; 2], group_box: None, tri_counts: vec![], width: 0.0, taper: [255; 16], pick_steps: vec![], seed: 0, length_var: [0.0, 1.0], head: vec![], corners: vec![], clump_scale: [1.0; 2], styles: vec![], clump_type: 0, guide_tri: vec![], twist: vec![], curl: Default::default() };
        assert_eq!(g.segments(), vec![0, 1, 1, 2, 3, 4, 4, 5]);
        assert_eq!(g.root_indices(), vec![0, 3, 3]);
    }
    #[test]
    fn coat_from_strand_counts() {
        // one root triangle of three 2-point guides, 40 + 10 stored strands
        let pts = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.1], [1.0, 0.0, 0.0], [1.0, 0.0, 0.1], [0.0, 1.0, 0.0], [0.0, 1.0, 0.1]];
        let g = Groom { tint_weight: 1.0, strands: 3, points_per_strand: 2, points: pts, w: vec![0.0; 6], roots: vec![[0, 1, 2]], lengths: vec![], root_uv: vec![], bones: vec![], root_box: [[0.0; 3]; 2], group_box: None, tri_counts: vec![[40, 10, 1, 0]], width: 0.0003, taper: [255; 16], pick_steps: vec![], seed: 0, length_var: [0.0, 1.0], head: vec![], corners: vec![], clump_scale: [1.0; 2], styles: vec![], clump_type: 0, guide_tri: vec![], twist: vec![], curl: Default::default() };
        assert_eq!(g.stored_strands(), 50);
        let c = g.coat(1.0);
        assert_eq!((c.guides.len(), c.points.len(), c.stored), (50, 100, 50));
        // every root lies inside the triangle and every strand keeps the guides' shape
        for s in 0..50 {
            let (r, tip) = (c.points[2 * s], c.points[2 * s + 1]);
            assert!(r[0] >= 0.0 && r[1] >= 0.0 && r[0] + r[1] <= 1.0 + 1e-5 && r[2].abs() < 1e-6);
            assert!((tip[2] - 0.1).abs() < 1e-6 && (tip[0] - r[0]).abs() < 1e-6);
            assert_eq!(c.normals[s], [0.0, 0.0, 1.0]);
        }
        // same strands again; a quarter of them at share 0.25
        assert_eq!(g.coat(1.0).points, c.points);
        let q = g.coat(0.25).guides.len();
        assert!((12..=13).contains(&q), "{q}");
        // a piece of clump strands only still grows
        let clumps = Groom { tri_counts: vec![[0, 30, 4, 0]], ..g.clone() };
        assert_eq!(clumps.coat(1.0).guides.len(), 30);
        // 50 strands x 2 points x 2 vertices; coloured from the guides
        let glb = g.coat_glb("t", &c, 0.001, &[[255, 0, 0], [255, 0, 0], [255, 0, 0]], serde_json::json!({}));
        assert_eq!(&glb[0..4], b"glTF");
    }

}
