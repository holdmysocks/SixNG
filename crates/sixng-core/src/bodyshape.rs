//! Ped body shape: the game's body-shape rigs, applied to the shared ped
//! skeleton.
//!
//! Body shape (the `gcweight*` / `gcmuscle*` controls of the body-shape
//! presets) moves helper bones (`BM_*`, `MH_*`, `SEC_*`, `RB_*`) of the
//! shared ped skeleton; the clothing is skinned to them. The moves come from
//! a RIG0 program in the rig dictionary of `anim.rpf`
//! (`anim/rig/generated/source/ped/human/<female|male>/body/LOD0_Body_Shapes_*`).
//! The program is a sparse additive pose rig over a float register file:
//! passes of (track register, weight register, delta) entries, each doing
//! track += weight × delta (translation, scale) or q = q · delta^weight
//! (rotation). Deltas are in the game's Z-up bone frames, i.e. the frames of
//! the bind matrices here. See FORMATS.md §3.20.
use crate::drawable::{affine_inverse, Bone};
use crate::meta7a::joaat;
use crate::resource::Resource;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Translation,
    Rotation,
    Scale,
}

#[derive(Clone, Debug)]
pub struct Op {
    /// Track register (first of 3 or 4).
    pub track: u16,
    /// Weight register.
    pub weight: u16,
    pub kind: Kind,
    /// xyz (translation, scale) or xyzw (rotation).
    pub delta: [f32; 4],
}

#[derive(Clone, Debug)]
enum Block {
    /// Constants: first register and values.
    Consts(u16, Vec<f32>),
    /// dst = a × b (assumed; only drives a minor corrective).
    Product(u16, u16, u16),
    /// dst = clamp(src, 0, 1) (assumed).
    Clamp(u16, u16),
    Pass(Vec<Op>),
}

#[derive(Clone, Debug)]
pub struct Rig {
    pub registers: usize,
    /// Float input name hash -> register.
    pub inputs: HashMap<u32, u16>,
    /// Track register -> joaat(bone name).
    pub tracks: HashMap<u16, u32>,
    blocks: Vec<Block>,
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
const SEC_PROGRAM: u32 = 0xb00f0959;

/// Parse a RIG0 blob (starts with "RIG0").
pub fn parse_rig(b: &[u8]) -> Option<Rig> {
    if b.get(0..4)? != b"RIG0" {
        return None;
    }
    let n = u32_at(b, 0x10)? as usize;
    let offs: Vec<usize> = (0..n).map(|i| u32_at(b, 0x14 + 4 * i).map(|x| x as usize)).collect::<Option<_>>()?;
    let sec = |t: u32| offs.iter().copied().find(|&o| u32_at(b, o) == Some(t));
    let (bt, inp, prog) = (sec(SEC_BLOCKS)?, sec(SEC_INPUTS)?, sec(SEC_PROGRAM)?);
    // inputs: (u8 type, u8 kind, u16 register, u32 hash); kind 1 = float, 3 = bone track
    let mut inputs = HashMap::new();
    let mut tracks = HashMap::new();
    for i in 0..u32_at(b, inp + 8)? as usize {
        let o = inp + 12 + 8 * i;
        let (kind, reg, h) = (*b.get(o + 1)?, u16_at(b, o + 2)?, u32_at(b, o + 4)?);
        match kind {
            1 => {
                inputs.insert(h, reg);
            }
            3 => {
                tracks.insert(reg, h);
            }
            _ => {}
        }
    }
    // block table: count, register count, then u32 (type << 24 | offset), 0xff ends
    let count = u32_at(b, bt + 8)? as usize;
    let registers = u32_at(b, bt + 12)? as usize;
    let body = prog + 8;
    let body_end = prog + u32_at(b, prog + 4)? as usize;
    let mut ents: Vec<(u8, usize)> = Vec::new();
    for i in 0..count {
        let e = u32_at(b, bt + 16 + 4 * i)?;
        if e >> 24 == 0xff {
            break;
        }
        ents.push(((e >> 24) as u8, body + (e & 0xff_ffff) as usize));
    }
    let mut blocks = Vec::new();
    for (k, &(t, o)) in ents.iter().enumerate() {
        let end = ents.get(k + 1).map_or(body_end, |x| x.1).min(b.len());
        let blk = b.get(o..end)?;
        match t {
            0x85 => {
                let (first, n) = (u16_at(blk, 0)?, u16_at(blk, 2)? as usize);
                blocks.push(Block::Consts(first, (0..n).map(|i| f32_at(blk, 4 + 4 * i)).collect::<Option<_>>()?));
            }
            0x46 => blocks.push(Block::Product(u16_at(blk, 0)?, u16_at(blk, 2)?, u16_at(blk, 4)?)),
            0x81 => blocks.push(Block::Clamp(u16_at(blk, 0)?, u16_at(blk, 2)?)),
            0x86 => {
                let (nt, ns, nq) = (u16_at(blk, 0)? as usize, u16_at(blk, 2)? as usize, u16_at(blk, 4)? as usize);
                let n = nt + ns + nq;
                let mut f = 8 + 6 * n;
                let mut ops = Vec::with_capacity(n);
                for i in 0..n {
                    let (track, weight) = (u16_at(blk, 8 + 6 * i)?, u16_at(blk, 8 + 6 * i + 2)?);
                    let kind = if i < nt { Kind::Translation } else if i < nt + ns { Kind::Scale } else { Kind::Rotation };
                    let m = if kind == Kind::Rotation { 4 } else { 3 };
                    let mut delta = [0f32; 4];
                    for (j, d) in delta.iter_mut().enumerate().take(m) {
                        *d = f32_at(blk, f + 4 * j)?;
                    }
                    f += 4 * m;
                    ops.push(Op { track, weight, kind, delta });
                }
                blocks.push(Block::Pass(ops));
            }
            _ => return None,
        }
    }
    Some(Rig { registers, inputs, tracks, blocks })
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

/// Shaped pose of a skeleton: bone -> model matrices (same space as
/// `Bone::bind_matrix`), plus how many rig entries moved a bone. Bones the rig
/// does not touch keep their bind pose relative to their parent. `None` if a
/// bone has no bind matrix.
pub fn pose(bones: &[Bone], rig: &Rig, inputs: &HashMap<u32, f32>) -> Option<(Vec<[f32; 16]>, usize)> {
    let binds: Vec<[f32; 16]> = bones.iter().map(|b| b.bind_matrix).collect::<Option<_>>()?;
    let mut regs = vec![0f32; rig.registers.max(1)];
    for (h, &r) in &rig.inputs {
        if let (Some(v), Some(x)) = (inputs.get(h), regs.get_mut(r as usize)) {
            *x = *v;
        }
    }
    let by_hash: HashMap<u32, usize> = bones.iter().enumerate().map(|(i, b)| (joaat(&b.name), i)).collect();
    let mut trs: Vec<_> = bones
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
    let mut touched = vec![false; bones.len()];
    let mut applied = 0;
    for blk in &rig.blocks {
        match blk {
            Block::Consts(first, v) => {
                for (k, x) in v.iter().enumerate() {
                    if let Some(r) = regs.get_mut(*first as usize + k) {
                        *r = *x;
                    }
                }
            }
            Block::Product(a, b, d) => {
                let v = regs.get(*a as usize).copied().unwrap_or(0.0) * regs.get(*b as usize).copied().unwrap_or(0.0);
                if let Some(r) = regs.get_mut(*d as usize) {
                    *r = v;
                }
            }
            Block::Clamp(d, s) => {
                let v = regs.get(*s as usize).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                if let Some(r) = regs.get_mut(*d as usize) {
                    *r = v;
                }
            }
            Block::Pass(ops) => {
                for op in ops {
                    let w = regs.get(op.weight as usize).copied().unwrap_or(0.0);
                    if w == 0.0 {
                        continue;
                    }
                    let Some(&bi) = rig.tracks.get(&op.track).and_then(|h| by_hash.get(h)) else { continue };
                    let (t, q, s) = &mut trs[bi];
                    match op.kind {
                        Kind::Translation => (0..3).for_each(|k| t[k] += w * op.delta[k]),
                        Kind::Scale => (0..3).for_each(|k| s[k] += w * op.delta[k]),
                        Kind::Rotation => *q = qmul(*q, qpow(op.delta, w)),
                    }
                    touched[bi] = true;
                    applied += 1;
                }
            }
        }
    }
    // Model transforms as the game's skeleton update builds them: each bone
    // keeps (rotation, translation, scale) in model space, composed from its
    // parent's as rotation = parent rotation * local rotation, translation =
    // parent translation + parent rotation applied to (parent scale * local
    // translation, per axis), scale = parent scale * local scale (per axis).
    // So a helper bone's scale reaches the bones under it, in each bone's own
    // axes, without the shear a matrix product would add. A vertex is then
    // rotation * (scale * (inverse bind * vertex)) + translation.
    let mut out = binds.clone();
    let mut model: Vec<([f32; 3], Quat, [f32; 3])> = binds.iter().map(to_trs).collect();
    for i in 0..bones.len() {
        let (lt, lq, ls) = trs[i];
        model[i] = match bones[i].parent {
            // parents come before children in RAGE skeletons; fall back to bind
            Some(p) if (p as usize) < i => {
                let (pt, pq, ps) = model[p as usize];
                let r = qrot(pq, [ps[0] * lt[0], ps[1] * lt[1], ps[2] * lt[2]]);
                ([pt[0] + r[0], pt[1] + r[1], pt[2] + r[2]], qmul(pq, lq), [ps[0] * ls[0], ps[1] * ls[1], ps[2] * ls[2]])
            }
            Some(_) => continue,
            None => (lt, lq, ls),
        };
        if touched[i] || bones[i].parent.is_some_and(|p| (p as usize) < i && out[p as usize] != binds[p as usize]) {
            out[i] = from_trs(model[i].0, model[i].1, model[i].2);
        }
    }
    Some((out, applied))
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

/// The LOD0 body-shape rig for a body (cached).
pub fn get(game_root: &Path, female: bool) -> Result<&'static Rig, String> {
    static F: OnceLock<Result<Rig, String>> = OnceLock::new();
    static M: OnceLock<Result<Rig, String>> = OnceLock::new();
    let cell = if female { &F } else { &M };
    cell.get_or_init(|| {
        let d = rig_dict(game_root)?;
        let blob = dict_rig(d, rig_path(female)).ok_or_else(|| format!("{} not in the rig dictionary", rig_path(female)))?;
        parse_rig(blob).ok_or_else(|| format!("{}: unexpected RIG0 layout", rig_path(female)))
    })
    .as_ref()
    .map_err(|e| e.clone())
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
        let rig = Rig {
            registers: 4,
            inputs: HashMap::from([(7, 0)]),
            tracks: HashMap::from([(1, joaat("parent"))]),
            blocks: vec![Block::Pass(vec![Op { track: 1, weight: 0, kind: Kind::Scale, delta: [0.0, 0.0, 2.0, 0.0] }])],
        };
        let (m, applied) = pose(&bones, &rig, &HashMap::from([(7, 1.0)])).unwrap();
        assert_eq!(applied, 1);
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
