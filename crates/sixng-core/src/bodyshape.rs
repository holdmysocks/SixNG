//! Ped body shape: the game's body-shape and garment rigs, applied to the
//! shared ped skeleton and to clothing helper bones.
//!
//! Body shape (the `gcweight*` / `gcmuscle*` controls of the body-shape
//! presets) moves helper bones (`BM_*`, `MH_*`, `SEC_*`, `RB_*`) of the
//! shared ped skeleton; the clothing is skinned to them. The moves come from
//! RIG0 programs in the rig dictionary of `anim.rpf`
//! (`anim/rig/generated/source/ped/human/<female|male>/body/LOD0_Body_Shapes_*`).
//! Clothing with its own helper bones (`GM_U01_*`) is then posed by the
//! garment surface rig (`.../garment/GarmentSurface<Female|Male>`), which
//! reads the shaped body bones and writes the helper bones' local
//! translation and rotation.
//!
//! A rig program runs over a float register file. The input list loads
//! float controls and bone local (translation, rotation, scale) channels
//! into registers, the blocks compute (vector and quaternion arithmetic,
//! curves, cone readers, blend skinning, additive pose passes), and the
//! output list writes bone channels back. Rotations and translations are in
//! the game's Z-up bone frames, i.e. the frames of the bind matrices here.
//! The block semantics are those of the game's evaluator (rva 0x5141100);
//! see FORMATS.md §3.20.
use crate::drawable::{affine_inverse, Bone};
use crate::meta7a::joaat;
use crate::resource::Resource;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// A bone channel of a rig's input or output list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channel {
    /// 0 translation (3 floats), 1 rotation (x, y, z, w), 2 scale (3 floats),
    /// 3 a scalar channel.
    pub t: u8,
    /// First register.
    pub reg: u16,
    /// joaat(bone name).
    pub hash: u32,
}

#[derive(Clone, Debug)]
pub struct Rig {
    pub registers: usize,
    /// Float input name hash -> register.
    pub inputs: HashMap<u32, u16>,
    /// Bone channels loaded into registers before the program runs.
    pub bone_in: Vec<Channel>,
    /// Bone channels written from registers after it.
    pub bone_out: Vec<Channel>,
    /// Program bytes and its blocks: (type, start, end) in `prog`.
    prog: Vec<u8>,
    blocks: Vec<(u8, usize, usize)>,
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn f32_at(b: &[u8], o: usize) -> Option<f32> {
    u32_at(b, o).map(f32::from_bits)
}

const SEC_BLOCKS: u32 = 0xe636c3e9;
const SEC_INPUTS: u32 = 0x2316833f;
const SEC_OUTPUTS: u32 = 0x7224f987;
const SEC_PROGRAM: u32 = 0xb00f0959;

/// Block types the evaluator here runs. Others (24, 25, 57, 80) occur only
/// in cull-volume and accessory rigs and are not decoded.
const KNOWN_BLOCKS: &[u8] = &[
    0x20, 0x21, 0x22, 0x40, 0x41, 0x42, 0x43, 0x45, 0x46, 0x48, 0x4b, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x60, 0x61, 0x62, 0x70, 0x71, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86,
];

/// Parse a RIG0 blob (starts with "RIG0").
pub fn parse_rig(b: &[u8]) -> Option<Rig> {
    if b.get(0..4)? != b"RIG0" {
        return None;
    }
    let n = u32_at(b, 0x10)? as usize;
    let offs: Vec<usize> = (0..n).map(|i| u32_at(b, 0x14 + 4 * i).map(|x| x as usize)).collect::<Option<_>>()?;
    let sec = |t: u32| offs.iter().copied().find(|&o| u32_at(b, o) == Some(t));
    let (bt, inp, prog) = (sec(SEC_BLOCKS)?, sec(SEC_INPUTS)?, sec(SEC_PROGRAM)?);
    // lists: (u8 channel, u8 kind, u16 register, u32 hash); kind 1 = float,
    // 3 = bone channel
    let list = |at: usize| -> Option<Vec<(u8, u8, u16, u32)>> { (0..u32_at(b, at + 8)? as usize).map(|i| { let o = at + 12 + 8 * i; Some((*b.get(o)?, *b.get(o + 1)?, u16_at(b, o + 2)?, u32_at(b, o + 4)?)) }).collect() };
    let mut inputs = HashMap::new();
    let mut bone_in = Vec::new();
    for (t, kind, reg, hash) in list(inp)? {
        match kind {
            1 => {
                inputs.insert(hash, reg);
            }
            3 => bone_in.push(Channel { t, reg, hash }),
            _ => {}
        }
    }
    let bone_out = match sec(SEC_OUTPUTS) {
        Some(o) => list(o)?.into_iter().filter(|e| e.1 == 3).map(|(t, _, reg, hash)| Channel { t, reg, hash }).collect(),
        None => Vec::new(),
    };
    // block table: count, register count, then u32 (type << 24 | offset), 0xff ends
    let count = u32_at(b, bt + 8)? as usize;
    let registers = u32_at(b, bt + 12)? as usize;
    let body = prog + 8;
    let body_end = (prog + u32_at(b, prog + 4)? as usize).min(b.len());
    let code = b.get(body..body_end)?.to_vec();
    let mut starts: Vec<(u8, usize)> = Vec::new();
    for i in 0..count {
        let e = u32_at(b, bt + 16 + 4 * i)?;
        if e >> 24 == 0xff {
            break;
        }
        let t = (e >> 24) as u8;
        if !KNOWN_BLOCKS.contains(&t) {
            return None;
        }
        starts.push((t, (e & 0xff_ffff) as usize));
    }
    let blocks = starts.iter().enumerate().map(|(k, &(t, o))| (t, o, starts.get(k + 1).map_or(code.len(), |x| x.1).max(o))).collect();
    Some(Rig { registers, inputs, bone_in, bone_out, prog: code, blocks })
}

type Trs = ([f32; 3], Quat, [f32; 3]);

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn normed<const N: usize>(v: [f32; N]) -> [f32; N] {
    let l = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if l > 0.0 {
        v.map(|x| x / l)
    } else {
        v
    }
}
fn conj(q: Quat) -> Quat {
    [-q[0], -q[1], -q[2], q[3]]
}
fn dot4(a: Quat, b: Quat) -> f32 {
    a.iter().zip(&b).map(|(x, y)| x * y).sum()
}

struct Regs(Vec<f32>);
impl Regs {
    fn s(&self, r: u16) -> f32 {
        self.0.get(r as usize).copied().unwrap_or(0.0)
    }
    fn g<const N: usize>(&self, r: u16) -> [f32; N] {
        std::array::from_fn(|i| self.0.get(r as usize + i).copied().unwrap_or(0.0))
    }
    fn p(&mut self, r: u16, v: &[f32]) {
        for (i, x) in v.iter().enumerate() {
            if let Some(d) = self.0.get_mut(r as usize + i) {
                *d = *x;
            }
        }
    }
}

impl Rig {
    /// Run the program. Bone channels of the input list are loaded from
    /// `locals` (bone hash -> local translation, rotation, scale; missing
    /// bones read zero), float inputs from `floats` (missing ones read zero).
    /// Returns the output list's channels with their values (rotation x, y,
    /// z, w; translation and scale in the first three).
    pub fn run(&self, locals: &HashMap<u32, Trs>, floats: &HashMap<u32, f32>) -> Vec<(Channel, [f32; 4])> {
        let mut r = Regs(vec![0f32; self.registers + 8]);
        for (h, &reg) in &self.inputs {
            r.p(reg, &[floats.get(h).copied().unwrap_or(0.0)]);
        }
        for c in &self.bone_in {
            if let Some((t, q, s)) = locals.get(&c.hash) {
                match c.t {
                    0 => r.p(c.reg, t),
                    1 => r.p(c.reg, q),
                    2 => r.p(c.reg, s),
                    _ => {}
                }
            }
        }
        let p = &self.prog;
        let h = |o: usize| u16_at(p, o).unwrap_or(0);
        let w = |o: usize| u32_at(p, o).unwrap_or(0) as usize;
        let f = |o: usize| f32_at(p, o).unwrap_or(0.0);
        for &(t, o, _end) in &self.blocks {
            match t {
                0x20 => r.p(h(o), &[f(o + 2)]),
                0x21 => r.p(h(o), &[f(o + 2), f(o + 6), f(o + 10)]),
                0x22 => r.p(h(o), &[f(o + 2), f(o + 6), f(o + 10), f(o + 14)]),
                0x85 => {
                    let (first, n) = (h(o), h(o + 2) as usize);
                    let v: Vec<f32> = (0..n).map(|i| f(o + 4 + 4 * i)).collect();
                    r.p(first, &v);
                }
                0x40 => r.p(h(o + 4), &[r.s(h(o)) + r.s(h(o + 2))]),
                0x46 => r.p(h(o + 4), &[r.s(h(o)) * r.s(h(o + 2))]),
                0x42 => r.p(h(o + 6), &[r.s(h(o)) + r.s(h(o + 2)) * r.s(h(o + 4))]),
                0x41 | 0x45 | 0x48 | 0x4b => {
                    let (a, b): ([f32; 3], [f32; 3]) = (r.g(h(o)), r.g(h(o + 2)));
                    let v: [f32; 3] = std::array::from_fn(|i| match t {
                        0x41 => a[i] + b[i],
                        0x45 => a[i] - b[i],
                        0x48 => a[i] * b[i],
                        _ if b[i] != 0.0 => a[i] / b[i],
                        _ => 0.0,
                    });
                    r.p(h(o + 4), &v);
                }
                0x43 => {
                    let (a, b, k): ([f32; 3], [f32; 3], f32) = (r.g(h(o)), r.g(h(o + 2)), r.s(h(o + 4)));
                    r.p(h(o + 6), &std::array::from_fn::<f32, 3, _>(|i| a[i] + b[i] * k));
                }
                0x55 => {
                    let (a, b, k): ([f32; 3], [f32; 3], f32) = (r.g(h(o)), r.g(h(o + 2)), r.s(h(o + 4)));
                    r.p(h(o + 6), &std::array::from_fn::<f32, 3, _>(|i| a[i] + (b[i] - a[i]) * k));
                }
                0x50 | 0x53 => {
                    let (q, v): (Quat, [f32; 3]) = (r.g(h(o)), r.g(h(o + 2)));
                    r.p(h(o + 4), &qrot(if t == 0x53 { conj(q) } else { q }, v));
                }
                0x51 | 0x54 => {
                    let (a, b): (Quat, Quat) = (r.g(h(o)), r.g(h(o + 2)));
                    r.p(h(o + 4), &qmul(if t == 0x54 { conj(a) } else { a }, b));
                }
                0x52 => {
                    // a * nlerp(identity, b, k), b in the w >= 0 hemisphere
                    let (a, mut b, k): (Quat, Quat, f32) = (r.g(h(o)), r.g(h(o + 2)), r.s(h(o + 4)));
                    if b[3] < 0.0 {
                        b = b.map(|x| -x);
                    }
                    r.p(h(o + 6), &qmul(a, normed([k * b[0], k * b[1], k * b[2], k * (b[3] - 1.0) + 1.0])));
                }
                0x56 => {
                    let (a, mut b, k): (Quat, Quat, f32) = (r.g(h(o)), r.g(h(o + 2)), r.s(h(o + 4)));
                    if dot4(a, b) < 0.0 {
                        b = b.map(|x| -x);
                    }
                    r.p(h(o + 6), &normed(std::array::from_fn::<f32, 4, _>(|i| a[i] + k * (b[i] - a[i]))));
                }
                0x60 => {
                    let (a, b): ([f32; 3], [f32; 3]) = (r.g(h(o)), r.g(h(o + 2)));
                    let v = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    r.p(h(o + 4), &if dot3(v, v) > 1e-12 { normed(v) } else { [1.0, 0.0, 0.0] });
                }
                0x61 => {
                    // shortest arc from a to b
                    let (a, b): ([f32; 3], [f32; 3]) = (r.g(h(o)), r.g(h(o + 2)));
                    let (a, b) = (normed(a), normed(b));
                    let d = dot3(a, b);
                    let q = if (d + 1.0).abs() < 1e-6 {
                        let ax = if a[0].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] };
                        let c = normed(cross(a, ax));
                        [c[0], c[1], c[2], 0.0]
                    } else {
                        let c = cross(a, b);
                        normed([c[0], c[1], c[2], 1.0 + d])
                    };
                    r.p(h(o + 4), &q);
                }
                0x62 => {
                    // frame from three points: x along b - a, y towards c
                    let (a, b, c): ([f32; 3], [f32; 3], [f32; 3]) = (r.g(h(o)), r.g(h(o + 2)), r.g(h(o + 4)));
                    let x = normed([b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
                    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                    let d = dot3(x, v);
                    let y = normed([v[0] - d * x[0], v[1] - d * x[1], v[2] - d * x[2]]);
                    let z = cross(x, y);
                    r.p(h(o + 6), &normed(m2q([[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]])));
                }
                0x70 | 0x71 => {
                    // cone readers: u16 input direction, entries of
                    // (u16 dst, direction, k) from +0x16, 0x12 apart
                    let v: [f32; 3] = r.g(h(o));
                    for i in 0..w(o + 0x12) {
                        let e = o + 0x16 + 0x12 * i;
                        let d = dot3(v, [f(e + 2), f(e + 6), f(e + 10)]);
                        let k = f(e + 14);
                        let x = if t == 0x70 {
                            (1.0 - d) * k + 1.0
                        } else {
                            let s = (2.0 * d + 2.0).abs();
                            let s = if s > f32::MIN_POSITIVE { 2.0 - s.sqrt() } else { 2.0 };
                            let y = s.abs();
                            1.0 - k * if y > f32::MIN_POSITIVE { y.sqrt() } else { 0.0 }
                        };
                        r.p(h(e), &[x.max(0.0)]);
                    }
                }
                0x81..=0x83 => {
                    // piecewise-linear curve: u16 dst, u16 input, u8 flags
                    // (bit 0: wrap to the fraction), u32 n at +8, n knots,
                    // n value registers
                    let n = w(o + 8);
                    if n == 0 {
                        continue;
                    }
                    let mut x = r.s(h(o + 2));
                    if p.get(o + 4).is_some_and(|fl| fl & 1 != 0) {
                        x -= x.floor();
                    }
                    let kn = |i: usize| f(o + 12 + 4 * i);
                    let vr = |i: usize| h(o + 12 + 4 * n + 2 * i);
                    let (j, s) = if n == 1 {
                        (0, 0.0)
                    } else {
                        let mut j = 0;
                        while j + 2 < n && kn(j + 1) < x {
                            j += 1;
                        }
                        (j, ((x - kn(j)) / (kn(j + 1) - kn(j)).max(1e-10)).clamp(0.0, 1.0))
                    };
                    let j1 = (j + 1).min(n - 1);
                    match t {
                        0x81 => r.p(h(o), &[r.s(vr(j)) + s * (r.s(vr(j1)) - r.s(vr(j)))]),
                        0x82 => {
                            let (a, b): ([f32; 3], [f32; 3]) = (r.g(vr(j)), r.g(vr(j1)));
                            r.p(h(o), &std::array::from_fn::<f32, 3, _>(|i| a[i] + s * (b[i] - a[i])));
                        }
                        _ => {
                            let (a, mut b): (Quat, Quat) = (r.g(vr(j)), r.g(vr(j1)));
                            if dot4(a, b) < 0.0 {
                                b = b.map(|x| -x);
                            }
                            r.p(h(o), &normed(std::array::from_fn::<f32, 4, _>(|i| a[i] + s * (b[i] - a[i]))));
                        }
                    }
                }
                0x84 => {
                    // blend skinning: u32 n; register triples (T, Q, S) for 8
                    // influences per item; weights; offsets and rotations
                    // (structure of arrays); destination (position, rotation)
                    let n = w(o);
                    let (ix, wt) = (o + 4, o + 4 + 48 * n);
                    let (of, qq) = (wt + 32 * n, wt + 32 * n + 96 * n);
                    let ds = qq + 128 * n;
                    for i in 0..n {
                        let mut pos = [0f32; 3];
                        let mut acc: Option<Quat> = None;
                        for k in 0..8 {
                            let reg = |j: usize| h(ix + 2 * (24 * i + 3 * k + j));
                            let wk = f(wt + 4 * (8 * i + k));
                            let off: [f32; 3] = std::array::from_fn(|c| f(of + 4 * (24 * i + 8 * c + k)));
                            let lq: Quat = std::array::from_fn(|c| f(qq + 4 * (32 * i + 8 * c + k)));
                            let (tt, q, sc): ([f32; 3], Quat, [f32; 3]) = (r.g(reg(0)), r.g(reg(1)), r.g(reg(2)));
                            let rv = qrot(q, [sc[0] * off[0], sc[1] * off[1], sc[2] * off[2]]);
                            (0..3).for_each(|c| pos[c] += wk * (tt[c] + rv[c]));
                            let rk = qmul(q, lq).map(|x| wk * x);
                            acc = Some(match acc {
                                None => rk,
                                Some(a) => {
                                    let sg = if dot4(a, rk) < 0.0 { -1.0 } else { 1.0 };
                                    std::array::from_fn(|c| a[c] + sg * rk[c])
                                }
                            });
                        }
                        r.p(h(ds + 4 * i), &pos);
                        let a = acc.unwrap_or([0.0; 4]);
                        let l2 = dot4(a, a);
                        r.p(h(ds + 4 * i + 2), &if l2 > 1e-5 { a.map(|x| x / l2.sqrt()) } else { [0.0; 4] });
                    }
                }
                0x86 => {
                    // additive pass: (track register, weight register) entries,
                    // translations, then scales (track += weight * delta), then
                    // rotations (q = q * delta^weight)
                    let (nt, ns, nq) = (h(o) as usize, h(o + 2) as usize, h(o + 4) as usize);
                    let n = nt + ns + nq;
                    let mut d = o + 8 + 6 * n;
                    for i in 0..n {
                        let (tr, wr) = (h(o + 8 + 6 * i), h(o + 8 + 6 * i + 2));
                        let rot = i >= nt + ns;
                        let wv = r.s(wr);
                        if wv != 0.0 {
                            if rot {
                                let q: Quat = r.g(tr);
                                r.p(tr, &qmul(q, qpow([f(d), f(d + 4), f(d + 8), f(d + 12)], wv)));
                            } else {
                                let v: [f32; 3] = r.g(tr);
                                r.p(tr, &std::array::from_fn::<f32, 3, _>(|j| v[j] + wv * f(d + 4 * j)));
                            }
                        }
                        d += if rot { 16 } else { 12 };
                    }
                }
                _ => {}
            }
        }
        self.bone_out
            .iter()
            .map(|c| {
                let v: [f32; 4] = r.g(c.reg);
                (*c, if c.t == 1 { v } else { [v[0], v[1], v[2], 0.0] })
            })
            .collect()
    }
}

/// A body control and the rig inputs it drives: (name, input for positive
/// values, input for negative values driven by |v|). From the game's
/// body-shape preset PSO, where each preset item pairs a control value
/// with rig input values.
pub const CONTROLS: &[(&str, u32, Option<u32>)] = &[
    ("gcweightback", 0x722b3991, Some(0xe79422cf)),
    ("gcweightbelly", 0xefc858c0, Some(0x74a5940e)),
    ("gcweightbutt", 0xb65f7951, Some(0x1d70bfc2)),
    ("gcweightcalf", 0x4c92b04a, Some(0xb3925c52)),
    ("gcweightchest", 0xfbc18785, Some(0x47164eb7)),
    ("gcweightforearm", 0x7e5f130b, Some(0x49c5ac98)),
    ("gcweighthip", 0x0978bf99, Some(0x596b89f1)),
    ("gcweightmuffin", 0xb0c81e7f, Some(0x9ffec641)),
    ("gcweightneck", 0x334633c1, Some(0xefc4dbff)),
    ("gcweightshoulder", 0x6d8264ca, Some(0xfb039566)),
    ("gcweightthigh", 0x525a6395, Some(0x7636ca63)),
    ("gcweightupperarm", 0xc8a41369, Some(0x5a7029e5)),
    ("gcmuscleback", 0x083e8443, None),
    ("gcmusclebelly", 0x860a629a, None),
    ("gcmusclebutt", 0x2267feeb, None),
    ("gcmusclecalf", 0x798aa11f, None),
    ("gcmusclechest", 0xf8ca1901, None),
    ("gcmuscleforearm", 0x33c4e1ea, None),
    ("gcmusclehip", 0xfc1aaed3, None),
    ("gcmusclemuffin", 0x3f088cd7, None),
    ("gcmuscleneck", 0x97d111cf, None),
    ("gcmuscleshoulder", 0xab82da7b, None),
    ("gcmusclethigh", 0x9e37dc2e, None),
    ("gcmuscleupperarm", 0x37657a6b, None),
    ("gcbutt", 0x5a502ae2, None),
    ("gcbreasts", 0x377b0101, Some(0x48f90678)),
];

/// Parse "gcweightbelly=1,gcmusclechest=0.5" into control values. Names are
/// case-insensitive; `allweight=v` / `allmuscle=v` set every weight / muscle
/// control. Values are clamped to -1..1 (muscle and butt to 0..1).
pub fn parse_spec(spec: &str) -> Result<Vec<(String, f32)>, String> {
    let mut out: Vec<(String, f32)> = Vec::new();
    for kv in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (k, v) = kv.split_once('=').ok_or_else(|| format!("body shape: expected name=value, got `{kv}`"))?;
        let k = k.trim().to_ascii_lowercase();
        let v: f32 = v.trim().parse().map_err(|_| format!("body shape: bad value in `{kv}`"))?;
        let names: Vec<&str> = match k.as_str() {
            "allweight" => CONTROLS.iter().map(|c| c.0).filter(|n| n.starts_with("gcweight")).collect(),
            "allmuscle" => CONTROLS.iter().map(|c| c.0).filter(|n| n.starts_with("gcmuscle")).collect(),
            _ => vec![CONTROLS.iter().find(|c| c.0 == k).map(|c| c.0).ok_or_else(|| format!("body shape: unknown control `{k}` (known: {}, allweight, allmuscle)", CONTROLS.iter().map(|c| c.0).collect::<Vec<_>>().join(", ")))?],
        };
        for n in names {
            let lo = if n.starts_with("gcmuscle") || n == "gcbutt" { 0.0 } else { -1.0 };
            out.retain(|x| x.0 != n);
            out.push((n.to_string(), v.clamp(lo, 1.0)));
        }
    }
    Ok(out)
}

/// Control values of an outfit's body-shape preset tag (the variation's
/// `BodyShapePreset` word, e.g. `Average`), from `common.rpf`'s body-type
/// XML (`CPedVariationBodyTypeManager`): the profile `BODYSHAPE_<TAG>` lists
/// presets the game picks from (the first is taken), and a preset
/// `PED_BODYSHAPE_*` lists control values (the first of each value list).
/// Controls without a known rig input (`GCCleavage`) are left out. Needs
/// `common.rpf` indexed.
pub fn preset_controls(game_root: &Path, tag: &str) -> Result<Vec<(String, f32)>, String> {
    preset_from_xml(body_type_xml(game_root)?, tag)
}

/// `preset_controls` on the XML text.
pub fn preset_from_xml(xml: &str, tag: &str) -> Result<Vec<(String, f32)>, String> {
    let tag = tag.trim().to_ascii_uppercase();
    let key = |k: &str| format!("<Item key=\"{k}\">");
    // the item's text up to the next item key of the same kind
    let block = |k: &str, next: &str| -> Option<&str> {
        let at = xml.find(&key(k))? + key(k).len();
        let rest = &xml[at..];
        Some(&rest[..rest.find(next).unwrap_or(rest.len())])
    };
    // an unnamed profile word comes as its joaat (8 hex digits): name it
    // from the profile keys of the XML
    let tag = match u32::from_str_radix(&tag, 16) {
        Ok(h) if tag.len() == 8 => xml
            .split("<Item key=\"BODYSHAPE_")
            .skip(1)
            .filter_map(|r| r.split('"').next())
            .find(|n| crate::meta7a::joaat(n) == h)
            .map(str::to_string)
            .ok_or_else(|| format!("body-shape profile word {tag} is not a profile of the body-type XML"))?,
        _ => tag,
    };
    let preset = if tag.starts_with("PED_BODYSHAPE_") {
        tag.clone()
    } else {
        let p = block(&format!("BODYSHAPE_{tag}"), "<Item key=\"BODYSHAPE_").ok_or_else(|| format!("body-shape profile BODYSHAPE_{tag} is not in the body-type XML"))?;
        let a = p.find("<BodyShapePreset>").ok_or_else(|| format!("body-shape profile BODYSHAPE_{tag} names no preset"))? + "<BodyShapePreset>".len();
        p[a..p[a..].find('<').map_or(p.len(), |e| a + e)].trim().to_string()
    };
    let b = block(&preset, "</Expressions>").ok_or_else(|| format!("body-shape preset {preset} is not in the body-type XML"))?;
    let mut spec = Vec::new();
    for item in b.split("<Id>").skip(1) {
        let Some((id, rest)) = item.split_once("</Id>") else { continue };
        let id = id.trim().to_ascii_lowercase();
        let Some(v) = rest.split_once('>').and_then(|(_, r)| r.split('<').next()).and_then(|r| r.split_whitespace().next()).and_then(|x| x.parse::<f32>().ok()) else { continue };
        if CONTROLS.iter().any(|c| c.0 == id) {
            spec.push(format!("{id}={v}"));
        }
    }
    parse_spec(&spec.join(","))
}

/// Tag keys of a body-shape kit (wardrobe kits tagged `bodyshape`, included
/// by an outfit's `BodyShapePreset` variation) whose values name preset
/// items of the body-shape preset PSO in `anim.rpf`. The game reads these
/// five keys together (static list at .rdata 0x651eab4, gathered by fn
/// 0x3e47f90).
pub const PRESET_ITEM_KEYS: [u32; 5] = [0x3631_3cc7, 0x0248_4ea6, 0x6b3f_b5a6, 0x179c_8d5c, 0x63d3_733d];

/// Control values of an outfit's body-shape preset items (hex hashes from
/// its body-shape kits' `PRESET_ITEM_KEYS` tags, by key and then by kit, as
/// the game gathers them in fn 0x3e47f90). Each item of the preset PSO
/// (struct 890b5009) lists control values and the rig input values they
/// give; the game stores each input's value over the old one (fn 0x4f95310),
/// so a later item's value for a control replaces an earlier one (Mandy's
/// preset sets every muscle region to 0.2, then thigh and calf back to 0).
/// Controls without a slot in `CONTROLS` are left out. Needs `anim.rpf`
/// indexed.
pub fn preset_items(game_root: &Path, items: &[String]) -> Result<Vec<(String, f32)>, String> {
    preset_from_items(preset_item_table(game_root)?, items)
}

/// An outfit's body-shape preset: its preset items (`Outfit::body_preset_items`)
/// when it has any, else its profile word through the body-type XML
/// (`preset_controls`); `None` when it has neither.
pub fn outfit_preset(game_root: &Path, word: Option<&str>, items: &[String]) -> Option<Result<Vec<(String, f32)>, String>> {
    if !items.is_empty() {
        Some(preset_items(game_root, items))
    } else {
        word.map(|w| preset_controls(game_root, w))
    }
}

/// `preset_items` on a table of item hash -> (control hash, value).
pub fn preset_from_items(table: &HashMap<u32, Vec<(u32, f32)>>, items: &[String]) -> Result<Vec<(String, f32)>, String> {
    let mut vals: Vec<(&str, f32)> = Vec::new();
    let mut found = false;
    for it in items {
        let Some(list) = u32::from_str_radix(it, 16).ok().and_then(|h| table.get(&h)) else { continue };
        found = true;
        for (n, v) in list {
            if let Some(c) = CONTROLS.iter().find(|c| crate::meta7a::joaat(c.0) == *n) {
                vals.retain(|x| x.0 != c.0);
                vals.push((c.0, *v));
            }
        }
    }
    if !found && !items.is_empty() {
        return Err(format!("body-shape preset items {} are not in the preset PSO", items.join(", ")));
    }
    parse_spec(&vals.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join(","))
}

/// Preset items of the body-shape preset PSO in `anim.rpf` (the PSO holding
/// struct 890b5009; read once): item hash -> (control hash, value).
fn preset_item_table(game_root: &Path) -> Result<&'static HashMap<u32, Vec<(u32, f32)>>, String> {
    static T: OnceLock<Result<HashMap<u32, Vec<(u32, f32)>>, String>> = OnceLock::new();
    T.get_or_init(|| {
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let ixp = crate::scan::index_path(&work.join("index"), "anim.rpf");
        let ix = crate::scan::load_index(&ixp).map_err(|e| format!("anim.rpf index ({}): {e}; index anim.rpf first", ixp.display()))?;
        let arch = game_root.join("anim.rpf");
        // more than one PSO holds the struct (a small one comes first in the
        // index): items of all of them
        let tag = 0x890b_5009u32;
        let mut t = HashMap::new();
        for e in ix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Pso) {
            let Ok(b) = crate::scan::read_entry(&arch, e) else { continue };
            if !b.windows(4).any(|w| w == tag.to_le_bytes() || w == tag.to_be_bytes()) {
                continue;
            }
            if let Some(p) = crate::pso::parse(&b) {
                preset_items_of(&p.to_json(), &mut t);
            }
        }
        if t.is_empty() {
            return Err("body-shape preset PSO (struct 890b5009) not found in anim.rpf".into());
        }
        Ok(t)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Map entries (struct 00000100) whose item is a preset (struct 890b5009).
fn preset_items_of(v: &serde_json::Value, out: &mut HashMap<u32, Vec<(u32, f32)>>) {
    match v {
        serde_json::Value::Object(o) => {
            let it = &v["Item"];
            if v["_struct"] == "00000100" && it["_struct"] == "890b5009" {
                if let Some(k) = v["Key"].as_u64() {
                    let vals = it["Values"].as_array().into_iter().flatten().filter_map(|x| Some((x["Name"].as_u64()? as u32, x["Value"].as_f64()? as f32))).collect();
                    out.entry(k as u32).or_insert(vals);
                }
            }
            o.values().for_each(|x| preset_items_of(x, out));
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| preset_items_of(x, out)),
        _ => {}
    }
}

/// The body-type XML of `common.rpf` (read once).
fn body_type_xml(game_root: &Path) -> Result<&'static str, String> {
    static X: OnceLock<Result<String, String>> = OnceLock::new();
    X.get_or_init(|| {
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let ixp = crate::scan::index_path(&work.join("index"), "common.rpf");
        let ix = crate::scan::load_index(&ixp).map_err(|e| format!("common.rpf index ({}): {e}; index common.rpf first", ixp.display()))?;
        let arch = game_root.join("common.rpf");
        for e in ix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Xml) {
            if let Ok(b) = crate::scan::read_entry(&arch, e) {
                let s = String::from_utf8_lossy(&b);
                if s.contains("<CPedVariationBodyTypeManager") {
                    return Ok(s.into_owned());
                }
            }
        }
        Err("body-type XML (CPedVariationBodyTypeManager) not found in common.rpf".into())
    })
    .as_deref()
    .map_err(|e| e.clone())
}

/// Rig input values (hash -> value) for control values.
pub fn input_values(controls: &[(String, f32)]) -> HashMap<u32, f32> {
    let mut m = HashMap::new();
    for (name, v) in controls {
        if let Some(&(_, pos, neg)) = CONTROLS.iter().find(|c| c.0 == name) {
            match (*v >= 0.0, neg) {
                (true, _) => *m.entry(pos).or_insert(0.0) += v,
                (false, Some(n)) => *m.entry(n).or_insert(0.0) += -v,
                (false, None) => {}
            }
        }
    }
    m
}

type Quat = [f32; 4];
fn qmul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx, aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz]
}
fn qpow(q: Quat, w: f32) -> Quat {
    let s = q[3].clamp(-1.0, 1.0);
    let a = s.acos();
    let sn = a.sin();
    if sn.abs() < 1e-8 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let k = (a * w).sin() / sn;
    [q[0] * k, q[1] * k, q[2] * k, (a * w).cos()]
}
fn m2q(r: [[f32; 3]; 3]) -> Quat {
    let t = r[0][0] + r[1][1] + r[2][2];
    if t > 0.0 {
        let k = 0.5 / (t + 1.0).sqrt();
        return [(r[2][1] - r[1][2]) * k, (r[0][2] - r[2][0]) * k, (r[1][0] - r[0][1]) * k, 0.25 / k];
    }
    let i = if r[0][0] >= r[1][1] && r[0][0] >= r[2][2] { 0 } else if r[1][1] >= r[2][2] { 1 } else { 2 };
    let (j, k) = ((i + 1) % 3, (i + 2) % 3);
    let s = 2.0 * (1.0 + r[i][i] - r[j][j] - r[k][k]).sqrt();
    let mut q = [0f32; 4];
    q[i] = 0.25 * s;
    q[j] = (r[j][i] + r[i][j]) / s;
    q[k] = (r[k][i] + r[i][k]) / s;
    q[3] = (r[k][j] - r[j][k]) / s;
    q
}

/// Local transform as translation, rotation, scale (column-major 4x4 in).
fn to_trs(m: &[f32; 16]) -> ([f32; 3], Quat, [f32; 3]) {
    let a = |r: usize, c: usize| m[c * 4 + r];
    let mut s = [0f32; 3];
    for (c, sc) in s.iter_mut().enumerate() {
        *sc = (a(0, c).powi(2) + a(1, c).powi(2) + a(2, c).powi(2)).sqrt().max(1e-12);
    }
    let mut r = [[0f32; 3]; 3];
    for (i, row) in r.iter_mut().enumerate() {
        for (c, x) in row.iter_mut().enumerate() {
            *x = a(i, c) / s[c];
        }
    }
    let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1]) - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0]) + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
    if det < 0.0 {
        s[0] = -s[0];
        for row in r.iter_mut() {
            row[0] = -row[0];
        }
    }
    ([a(0, 3), a(1, 3), a(2, 3)], m2q(r), s)
}
/// Rotate v by the unit quaternion q.
fn qrot(q: Quat, v: [f32; 3]) -> [f32; 3] {
    let [x, y, z, w] = q;
    // t = 2 (q.xyz x v); v' = v + w t + q.xyz x t
    let t = [2.0 * (y * v[2] - z * v[1]), 2.0 * (z * v[0] - x * v[2]), 2.0 * (x * v[1] - y * v[0])];
    [v[0] + w * t[0] + (y * t[2] - z * t[1]), v[1] + w * t[1] + (z * t[0] - x * t[2]), v[2] + w * t[2] + (x * t[1] - y * t[0])]
}
fn from_trs(t: [f32; 3], q: Quat, s: [f32; 3]) -> [f32; 16] {
    let n = (q.iter().map(|x| x * x).sum::<f32>()).sqrt().max(1e-12);
    let [x, y, z, w] = q.map(|v| v / n);
    let r = [[1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)], [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)], [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)]];
    let mut m = [0f32; 16];
    for c in 0..3 {
        for i in 0..3 {
            m[c * 4 + i] = r[i][c] * s[c];
        }
    }
    m[12] = t[0];
    m[13] = t[1];
    m[14] = t[2];
    m[15] = 1.0;
    m
}
fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut o = [0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

/// A shaped skeleton.
#[derive(Clone, Debug)]
pub struct Posed {
    /// Bone -> model matrices (same space as `Bone::bind_matrix`). Bones the
    /// rig does not move keep their bind pose relative to their parent.
    pub matrices: Vec<[f32; 16]>,
    /// Bone channels the rig changed.
    pub moved: usize,
    /// Each bone's local (translation, rotation, scale) after the rig.
    locals: Vec<Trs>,
    /// Each bone's model (translation, rotation, scale).
    model: Vec<Trs>,
    by_hash: HashMap<u32, usize>,
}

fn changed(a: &[f32], b: &[f32]) -> bool {
    a.iter().zip(b).any(|(x, y)| (x - y).abs() > 1e-6)
}

/// Compose a model transform from the parent's and a local one, as the
/// game's skeleton update does: rotation = parent rotation * local rotation,
/// translation = parent translation + parent rotation applied to (parent
/// scale * local translation, per axis), scale = parent scale * local scale
/// (per axis). So a helper bone's scale reaches the bones under it, in each
/// bone's own axes, without the shear a matrix product would add. A vertex
/// is then rotation * (scale * (inverse bind * vertex)) + translation.
fn compose((pt, pq, ps): Trs, (lt, lq, ls): Trs) -> Trs {
    let r = qrot(pq, [ps[0] * lt[0], ps[1] * lt[1], ps[2] * lt[2]]);
    ([pt[0] + r[0], pt[1] + r[1], pt[2] + r[2]], qmul(pq, lq), [ps[0] * ls[0], ps[1] * ls[1], ps[2] * ls[2]])
}

fn apply(trs: &mut Trs, c: &Channel, v: &[f32; 4]) -> bool {
    let (old, new): (&mut [f32], &[f32]) = match c.t {
        0 => (&mut trs.0, &v[..3]),
        1 => (&mut trs.1, &v[..4]),
        2 => (&mut trs.2, &v[..3]),
        _ => return false,
    };
    let moved = changed(old, new);
    old.copy_from_slice(new);
    moved
}

/// Shaped pose of a skeleton by a body-shape rig. `None` if a bone has no
/// bind matrix.
pub fn pose(bones: &[Bone], rig: &Rig, inputs: &HashMap<u32, f32>) -> Option<Posed> {
    let binds: Vec<[f32; 16]> = bones.iter().map(|b| b.bind_matrix).collect::<Option<_>>()?;
    let by_hash: HashMap<u32, usize> = bones.iter().enumerate().map(|(i, b)| (joaat(&b.name), i)).collect();
    let mut locals: Vec<Trs> = bones
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let local = match b.parent {
                Some(p) => mul(&affine_inverse(&binds[p as usize]), &binds[i]),
                None => binds[i],
            };
            to_trs(&local)
        })
        .collect();
    let input: HashMap<u32, Trs> = by_hash.iter().map(|(&h, &i)| (h, locals[i])).collect();
    let mut touched = vec![false; bones.len()];
    let mut moved = 0;
    for (c, v) in rig.run(&input, inputs) {
        let Some(&bi) = by_hash.get(&c.hash) else { continue };
        if apply(&mut locals[bi], &c, &v) {
            touched[bi] = true;
            moved += 1;
        }
    }
    let mut out = binds.clone();
    let mut model: Vec<Trs> = binds.iter().map(to_trs).collect();
    for i in 0..bones.len() {
        model[i] = match bones[i].parent {
            // parents come before children in RAGE skeletons; fall back to bind
            Some(p) if (p as usize) < i => compose(model[p as usize], locals[i]),
            Some(_) => continue,
            None => locals[i],
        };
        if touched[i] || bones[i].parent.is_some_and(|p| (p as usize) < i && out[p as usize] != binds[p as usize]) {
            out[i] = from_trs(model[i].0, model[i].1, model[i].2);
        }
    }
    Some(Posed { matrices: out, moved, locals, model, by_hash })
}

impl Posed {
    /// Pose clothing helper bones with a garment surface rig: the rig reads
    /// the shaped body bones' locals and the helper bones' stored locals and
    /// writes the helper bones' new locals. Returns, per bone of `own` (the
    /// clothing drawable's own skeleton), its model matrix when the rig
    /// moved it; `None` for bones it leaves (they follow their parent
    /// rigidly). Only root bones that name their parent in the shared
    /// skeleton (`Bone::parent_hash`) are posed, which is how helper bones
    /// are stored.
    pub fn garment(&self, rig: &Rig, own: &[Bone]) -> Vec<Option<[f32; 16]>> {
        let stored = |b: &Bone| -> Trs { (b.translation, b.rotation, b.scale) };
        let mut input: HashMap<u32, Trs> = self.by_hash.iter().map(|(&h, &i)| (h, self.locals[i])).collect();
        for b in own {
            input.entry(joaat(&b.name)).or_insert_with(|| stored(b));
        }
        let mut outs: HashMap<u32, Vec<(Channel, [f32; 4])>> = HashMap::new();
        for (c, v) in rig.run(&input, &HashMap::new()) {
            outs.entry(c.hash).or_default().push((c, v));
        }
        own.iter()
            .map(|b| {
                let pi = *b.parent_hash.filter(|_| b.parent.is_none()).and_then(|h| self.by_hash.get(&h))?;
                let o = outs.get(&joaat(&b.name))?;
                let mut local = stored(b);
                for (c, v) in o {
                    apply(&mut local, c, v);
                }
                let (t, q, s) = compose(self.model[pi], local);
                Some(from_trs(t, q, s))
            })
            .collect()
    }
}

/// Rig path of the LOD0 body-shape rig.
pub fn rig_path(female: bool) -> &'static str {
    if female {
        "anim/rig/generated/source/ped/human/female/body/LOD0_Body_Shapes_Female"
    } else {
        "anim/rig/generated/source/ped/human/male/body/LOD0_Body_Shapes_Male"
    }
}

/// Look up a rig by path in a rig dictionary resource: header +0x48 pointer
/// to hash buckets, +0x50 bucket count; entry {u32 joaat(path), u64 wrapper,
/// u64 next}; wrapper +0x20 pointer to the rig ("RIG" + byte 2 + u32 size,
/// then the "RIG0" blob of that size at +0x100).
pub fn dict_rig<'a>(r: &'a Resource, path: &str) -> Option<&'a [u8]> {
    let target = joaat(path);
    let buckets = r.sys_off(r.u64(0x48))?;
    let n = r.u32(0x50) as usize;
    if n == 0 || n > 1 << 20 || buckets + 8 * n > r.sys.len() {
        return None;
    }
    for i in 0..n {
        let mut e = r.u64(buckets + 8 * i);
        let mut guard = 0;
        while e != 0 && guard < 4096 {
            let Some(o) = r.sys_off(e).filter(|o| o + 24 <= r.sys.len()) else { break };
            if r.u32(o) == target {
                let w = r.sys_off(r.u64(o + 8))?;
                let rig = r.sys_off(r.u64(w + 0x20))?;
                let blob = r.sys.get(rig..)?;
                if blob.get(0..4)? != b"RIG" {
                    return None;
                }
                // the size is the inner RIG0 blob's
                let size = u32::from_le_bytes(blob.get(4..8)?.try_into().ok()?) as usize;
                return blob.get(0x100..(0x100 + size).min(blob.len()));
            }
            e = r.u64(o + 16);
            guard += 1;
        }
    }
    None
}

/// The rig dictionary resource of `anim.rpf` (found once by probing the
/// large resources in the anim.rpf index; the id is cached in the work
/// directory as `index/anim.rigdict`).
fn rig_dict(game_root: &Path) -> Result<&'static Resource, String> {
    static D: OnceLock<Result<Resource, String>> = OnceLock::new();
    D.get_or_init(|| {
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let ixp = crate::scan::index_path(&work.join("index"), "anim.rpf");
        let ix = crate::scan::load_index(&ixp).map_err(|e| format!("anim.rpf index ({}): {e}; index anim.rpf first", ixp.display()))?;
        let arch = game_root.join("anim.rpf");
        let cache = work.join("index").join("anim.rigdict");
        let ok = |r: &Resource| dict_rig(r, rig_path(true)).is_some();
        if let Some(id) = std::fs::read_to_string(&cache).ok() {
            if let Some(e) = ix.entries.iter().find(|e| e.id == id.trim()) {
                if let Ok(r) = Resource::load_sys(&arch, e) {
                    if ok(&r) {
                        return Ok(r);
                    }
                }
            }
        }
        let mut cand: Vec<_> = ix.entries.iter().filter(|e| e.kind == crate::classify::Kind::RageObj && e.usize >= 1 << 20 && !e.streams.is_empty()).collect();
        cand.sort_by_key(|e| std::cmp::Reverse(e.usize));
        for e in cand {
            if let Ok(r) = Resource::load_sys(&arch, e) {
                if ok(&r) {
                    let _ = crate::atomic::write(&cache, &e.id);
                    return Ok(r);
                }
            }
        }
        Err("body-shape rig dictionary not found in anim.rpf (index it again with this version)".into())
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Rig path of the garment surface rig, which poses clothing helper bones
/// (`GM_U01_*`) from the body bones.
pub fn garment_rig_path(female: bool) -> &'static str {
    if female {
        "anim/rig/generated/source/ped/human/female/garment/GarmentSurfaceFemale"
    } else {
        "anim/rig/generated/source/ped/human/male/garment/GarmentSurfaceMale"
    }
}

fn load(game_root: &Path, path: &str) -> Result<Rig, String> {
    let d = rig_dict(game_root)?;
    let blob = dict_rig(d, path).ok_or_else(|| format!("{path} not in the rig dictionary"))?;
    parse_rig(blob).ok_or_else(|| format!("{path}: unexpected RIG0 layout"))
}

/// The LOD0 body-shape rig for a body (cached).
pub fn get(game_root: &Path, female: bool) -> Result<&'static Rig, String> {
    static F: OnceLock<Result<Rig, String>> = OnceLock::new();
    static M: OnceLock<Result<Rig, String>> = OnceLock::new();
    let cell = if female { &F } else { &M };
    cell.get_or_init(|| load(game_root, rig_path(female))).as_ref().map_err(|e| e.clone())
}

/// The garment surface rig for a body (cached).
pub fn get_garment(game_root: &Path, female: bool) -> Result<&'static Rig, String> {
    static F: OnceLock<Result<Rig, String>> = OnceLock::new();
    static M: OnceLock<Result<Rig, String>> = OnceLock::new();
    let cell = if female { &F } else { &M };
    cell.get_or_init(|| load(game_root, garment_rig_path(female))).as_ref().map_err(|e| e.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_and_inputs() {
        let c = parse_spec("GCWeightBelly=-0.5, gcmusclechest=2").unwrap();
        assert_eq!(c, vec![("gcweightbelly".to_string(), -0.5), ("gcmusclechest".to_string(), 1.0)]);
        let m = input_values(&c);
        assert_eq!(m.get(&0x74a5940e), Some(&0.5));
        assert_eq!(m.get(&0xf8ca1901), Some(&1.0));
        assert_eq!(parse_spec("allweight=1").unwrap().len(), 12);
        assert!(parse_spec("nope=1").is_err());
    }

    #[test]
    fn preset_items_later_replace_earlier() {
        let j = crate::meta7a::joaat;
        let mut t = HashMap::new();
        // every muscle region 0.2, then thigh back to 0, then an unknown control
        t.insert(0x1d61_da02, CONTROLS.iter().filter(|c| c.0.starts_with("gcmuscle")).map(|c| (j(c.0), 0.2)).collect());
        t.insert(0xa693_fe91, vec![(j("gcmusclethigh"), 0.0)]);
        t.insert(0x5cd3_731a, vec![(0x1234_5678, 0.5), (j("GCBreasts"), 0.4)]);
        let c = preset_from_items(&t, &["1d61da02".into(), "a693fe91".into(), "5cd3731a".into(), "00000001".into()]).unwrap();
        let get = |n: &str| c.iter().find(|x| x.0 == n).map(|x| x.1);
        assert_eq!(get("gcmuscleback"), Some(0.2));
        assert_eq!(get("gcmusclethigh"), Some(0.0));
        assert_eq!(get("gcbreasts"), Some(0.4));
        assert!(preset_from_items(&t, &["00000001".into()]).is_err());
        assert!(preset_from_items(&t, &[]).unwrap().is_empty());
    }

    #[test]
    fn preset_from_body_type_xml() {
        let xml = r#"<BodyShapePresets>
    <Item key="PED_BODYSHAPE_REGULAR"><Expressions><Item><Id>gcweightback</Id><Values content="float_array">0.0</Values></Item></Expressions></Item>
    <Item key="PED_BODYSHAPE_BELLY">
      <Expressions>
        <Item><Id>GCCleavage</Id><Values content="float_array">0.5</Values></Item>
        <Item><Id>GCButt</Id><Values content="float_array">0.75</Values></Item>
        <Item><Id>gcweightbelly</Id><Values content="float_array">0.5 0.75 1.0</Values></Item>
      </Expressions>
    </Item>
  </BodyShapePresets>
  <BodyShapeVariationProfiles>
    <Item key="BODYSHAPE_AVERAGE"><BodyShapeVariations><Item><BodyShapePreset>PED_BODYSHAPE_REGULAR</BodyShapePreset></Item></BodyShapeVariations></Item>
    <Item key="BODYSHAPE_BEACHBELLIES"><BodyShapeVariations><Item><BodyShapePreset>PED_BODYSHAPE_BELLY</BodyShapePreset></Item></BodyShapeVariations></Item>
  </BodyShapeVariationProfiles>"#;
        assert_eq!(preset_from_xml(xml, "Average").unwrap(), vec![("gcweightback".to_string(), 0.0)]);
        assert_eq!(preset_from_xml(xml, "BeachBellies").unwrap(), vec![("gcbutt".to_string(), 0.75), ("gcweightbelly".to_string(), 0.5)]);
        assert!(preset_from_xml(xml, "Nope").is_err());
        // an unnamed profile word, as its joaat
        let h = format!("{:08x}", crate::meta7a::joaat("BeachBellies"));
        assert_eq!(preset_from_xml(xml, &h).unwrap(), preset_from_xml(xml, "BeachBellies").unwrap());
        assert!(preset_from_xml(xml, &format!("{:08x}", crate::meta7a::joaat("Skinny"))).is_err());
    }

    #[test]
    fn trs_round_trip() {
        let m = from_trs([0.1, -0.2, 0.3], [0.1, 0.2, 0.3, 0.927_362], [1.0, 1.5, 0.5]);
        let (t, q, s) = to_trs(&m);
        let m2 = from_trs(t, q, s);
        assert!(m.iter().zip(&m2).all(|(a, b)| (a - b).abs() < 1e-4));
    }

    #[test]
    fn scale_carries_to_children_per_axis() {
        // child rotated by `cq` relative to its parent
        let bone = |i: u16, name: &str, parent: Option<u16>, z: f32, q: Quat| {
            let mut b: Bone = serde_json::from_value(serde_json::json!({"index": i, "name": name, "parent": parent, "next_sibling": null, "flags": 0, "hash16": 0, "rotation": q, "translation": [0.0, 0.0, z], "scale": [1.0, 1.0, 1.0]})).unwrap();
            b.bind_matrix = Some(from_trs([0.0, 0.0, z], q, [1.0; 3]));
            b
        };
        let id = [0.0, 0.0, 0.0, 1.0];
        let h = std::f32::consts::FRAC_1_SQRT_2;
        // root at 0, parent at 1 m, child 0.1 m above the parent and turned 90 degrees about x
        let bones = vec![bone(0, "root", None, 0.0, id), bone(1, "parent", Some(0), 1.0, id), bone(2, "child", Some(1), 1.1, [h, 0.0, 0.0, h])];
        // one additive pass: scale of "parent" (registers 1..3) += weight (register 0) * (0, 0, 2)
        let mut prog = Vec::new();
        for x in [0u16, 1, 0, 0, 1, 0, 0] {
            prog.extend(x.to_le_bytes());
        }
        for x in [0f32, 0.0, 2.0] {
            prog.extend(x.to_le_bytes());
        }
        let ch = Channel { t: 2, reg: 1, hash: joaat("parent") };
        let rig = Rig { registers: 4, inputs: HashMap::from([(7, 0)]), bone_in: vec![ch], bone_out: vec![ch], blocks: vec![(0x86, 0, prog.len())], prog };
        let posed = pose(&bones, &rig, &HashMap::from([(7, 1.0)])).unwrap();
        assert_eq!(posed.moved, 1);
        let m = &posed.matrices;
        // the parent is scaled 3x along z
        assert!((m[1][10] - 3.0).abs() < 1e-5);
        // the child's offset is scaled with it: 1 + 3 * 0.1
        assert!((m[2][14] - 1.3).abs() < 1e-5);
        // and its own z axis (pointing along -y in the model) is 3 long, its
        // y axis (along z in the model) stays 1 long: no shear
        let len = |c: usize| (m[2][c * 4].powi(2) + m[2][c * 4 + 1].powi(2) + m[2][c * 4 + 2].powi(2)).sqrt();
        assert!((len(2) - 3.0).abs() < 1e-5);
        assert!((len(1) - 1.0).abs() < 1e-5);
        assert!((m[2][9] + 3.0).abs() < 1e-5);
    }
}
