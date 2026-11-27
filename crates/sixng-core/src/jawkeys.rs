//! Teeth and tongue follow the head's jaw shape keys.
//!
//! A ped head's face network (`facegraph`) moves the skin only. The lower
//! teeth, the tongue and the head's own inner-mouth vertices are skinned to
//! the bones under `FB_Jaw`, which the game poses in code, so on their own
//! they stay shut while a jaw-open key drops the chin.
//!
//! The bony chin, the head vertices below and in front of the lower teeth,
//! moves rigidly in these keys. Each head key is fitted on the chin as a turn
//! about the `FB_Jaw` joint (the jaw is a hinge), or, when no turn fits, as a
//! shift (the jaw pushed forward or back). A key the fit explains (mean
//! residual under 30% of the motion, at least 1 mm of motion) gives every
//! vertex under `FB_Jaw` the same motion, scaled by its skin weight there.
//! The teeth and tongue get a shape key of the same name, and the head's
//! inner-mouth vertices get the motion added to the head's own key.

use crate::gltf::{Joint, NodeGroup};

/// Below this mean chin motion (metres) a key is left alone.
const MIN_MOTION: f32 = 0.001;
/// A fit must leave less than this share of the chin's motion unexplained.
const MAX_RESIDUAL: f32 = 0.3;
/// How far below the lower teeth the chin region reaches (metres).
const CHIN_DEPTH: f32 = 0.06;

/// The rigid motion fitted to one key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JawMotion {
    /// A rotation vector (radians) about the jaw joint.
    Turn([f32; 3]),
    /// A translation (metres).
    Shift([f32; 3]),
}

impl JawMotion {
    /// Where the motion takes a rest position.
    pub fn apply(&self, pivot: [f32; 3], p: [f32; 3]) -> [f32; 3] {
        match *self {
            JawMotion::Shift(t) => [p[0] + t[0], p[1] + t[1], p[2] + t[2]],
            JawMotion::Turn(w) => {
                let r = sub(p, pivot);
                let q = rotate(w, r);
                [pivot[0] + q[0], pivot[1] + q[1], pivot[2] + q[2]]
            }
        }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn len(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// Rodrigues: `v` turned by the rotation vector `w`.
fn rotate(w: [f32; 3], v: [f32; 3]) -> [f32; 3] {
    let a = len(w);
    if a < 1e-9 {
        return v;
    }
    let k = [w[0] / a, w[1] / a, w[2] / a];
    let (s, c) = a.sin_cos();
    let kv = cross(k, v);
    let kd = k[0] * v[0] + k[1] * v[1] + k[2] * v[2];
    std::array::from_fn(|i| v[i] * c + kv[i] * s + k[i] * kd * (1.0 - c))
}

/// Solve a 3x3 system (Cramer's rule); `None` when singular.
fn solve3(m: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let d = det(m);
    if d.abs() < 1e-18 {
        return None;
    }
    Some(std::array::from_fn(|k| {
        let mut mk = m;
        for r in 0..3 {
            mk[r][k] = b[r];
        }
        det(mk) / d
    }))
}

/// Fit a turn about `pivot` and a shift to the displacements `d` of the rest
/// positions `p`. Returns both with their residuals (mean error over mean
/// motion), or `None` when the motion is too small.
pub fn fit_both(pivot: [f32; 3], p: &[[f32; 3]], d: &[[f32; 3]]) -> Option<[(JawMotion, f32); 2]> {
    let n = p.len().min(d.len());
    if n == 0 {
        return None;
    }
    let mean_motion = d[..n].iter().map(|&x| len(x)).sum::<f32>() / n as f32;
    if mean_motion < MIN_MOTION {
        return None;
    }
    // turn: d = w x r, i.e. d = -[r]x w; normal equations
    let mut ata = [[0f64; 3]; 3];
    let mut atb = [0f64; 3];
    for i in 0..n {
        let r = sub(p[i], pivot).map(f64::from);
        let a = [[0.0, r[2], -r[1]], [-r[2], 0.0, r[0]], [r[1], -r[0], 0.0]];
        for row in 0..3 {
            for c in 0..3 {
                for k in 0..3 {
                    ata[c][k] += a[row][c] * a[row][k];
                }
                atb[c] += a[row][c] * d[i][row] as f64;
            }
        }
    }
    let turn = solve3(ata, atb).map(|w| JawMotion::Turn(w.map(|x| x as f32)));
    let mut t = [0f32; 3];
    for x in &d[..n] {
        for k in 0..3 {
            t[k] += x[k] / n as f32;
        }
    }
    let shift = JawMotion::Shift(t);
    let residual = |m: JawMotion| {
        (0..n).map(|i| {
            let q = m.apply(pivot, p[i]);
            len(sub(sub(q, p[i]), d[i]))
        }).sum::<f32>() / n as f32 / mean_motion
    };
    let turn = turn.map_or((JawMotion::Turn([0.0; 3]), f32::INFINITY), |m| (m, residual(m)));
    Some([turn, (shift, residual(shift))])
}

/// The motion used for a key: the turn when it fits, else the shift when it
/// fits; with its residual.
pub fn fit(pivot: [f32; 3], p: &[[f32; 3]], d: &[[f32; 3]]) -> Option<(JawMotion, f32)> {
    let [turn, shift] = fit_both(pivot, p, d)?;
    [turn, shift].into_iter().find(|x| x.1 < MAX_RESIDUAL)
}

/// Each vertex's skin weight on bones under the jaw joint (0..1), or `None`
/// when the primitive is not skinned there.
fn jaw_weights(p: &crate::gltf::TriPrim, under: &[bool]) -> Option<Vec<f32>> {
    let (js, ws) = (p.joints.as_ref()?, p.weights.as_ref()?);
    let mut any = false;
    let out = (0..p.positions.len()).map(|i| {
        let mut sets = vec![(js[i], ws[i])];
        if let (Some(j1), Some(w1)) = (&p.joints1, &p.weights1) {
            sets.push((j1[i], w1[i]));
        }
        let (mut on, mut all) = (0f32, 0f32);
        for (j, w) in sets {
            for k in 0..4 {
                let w = w[k] as f32;
                all += w;
                if under.get(j[k] as usize).copied().unwrap_or(false) {
                    on += w;
                }
            }
        }
        let x = if all > 0.0 { on / all } else { 0.0 };
        any |= x > 0.0;
        x
    }).collect();
    any.then_some(out)
}

/// A head key's fitted jaw motion.
#[derive(Clone, Debug, PartialEq)]
pub struct JawKey {
    pub name: String,
    pub motion: JawMotion,
    /// The share of the chin's motion each fit leaves unexplained.
    pub turn_residual: f32,
    pub shift_residual: f32,
}

/// Fit a head's jaw keys. Positions are in the model's space, Z up and -Y
/// forward. `head_jaw` is each head vertex's skin weight under the jaw (its
/// inner mouth, left out of the chin); `lo` and `hi` bound the lower teeth.
/// Returns the number of chin vertices and the keys, or why none were fitted.
pub fn fit_keys(pivot: [f32; 3], head: &[[f32; 3]], head_jaw: &[f32], morphs: &[(String, Vec<[f32; 3]>)], lo: [f32; 3], hi: [f32; 3]) -> Result<(usize, Vec<JawKey>), String> {
    // the chin: below the lower teeth, in front of their middle, within
    // their width
    let half = lo[0].abs().max(hi[0].abs());
    let mid_y = (lo[1] + hi[1]) / 2.0;
    let chin: Vec<usize> = head.iter().enumerate().filter(|(i, v)| v[2] < lo[2] && v[2] > lo[2] - CHIN_DEPTH && v[0].abs() < half && v[1] < mid_y && head_jaw.get(*i).copied().unwrap_or(0.0) < 0.01).map(|(i, _)| i).collect();
    if chin.len() < 50 {
        return Err(format!("only {} chin vertices", chin.len()));
    }
    let rest: Vec<[f32; 3]> = chin.iter().map(|&i| head[i]).collect();
    let mut keys = Vec::new();
    for m in morphs.iter().filter(|m| !m.0.starts_with("pose_") && m.1.len() == head.len()) {
        let d: Vec<[f32; 3]> = chin.iter().map(|&i| m.1[i]).collect();
        if let Some([turn, shift]) = fit_both(pivot, &rest, &d) {
            if let Some((motion, _)) = [turn, shift].into_iter().find(|x| x.1 < MAX_RESIDUAL) {
                keys.push(JawKey { name: m.0.clone(), motion, turn_residual: turn.1, shift_residual: shift.1 });
            }
        }
    }
    Ok((chin.len(), keys))
}

/// Add the keys to a mesh's morph targets: each vertex moves with the jaw by
/// its weight under it (`jaw`). A key the mesh already has gets the motion
/// added; a new one is appended.
pub fn add_keys(morphs: &mut Vec<(String, Vec<[f32; 3]>)>, keys: &[JawKey], pivot: [f32; 3], positions: &[[f32; 3]], jaw: &[f32]) {
    for k in keys {
        let d: Vec<[f32; 3]> = positions.iter().zip(jaw).map(|(&v, &x)| if x > 0.0 { sub(k.motion.apply(pivot, v), v).map(|c| c * x) } else { [0.0; 3] }).collect();
        match morphs.iter_mut().find(|m| m.0 == k.name && m.1.len() == d.len()) {
            Some(m) => m.1.iter_mut().zip(&d).for_each(|(a, b)| (0..3).for_each(|c| a[c] += b[c])),
            None => morphs.push((k.name.clone(), d)),
        }
    }
}

/// Give the teeth, tongue and inner mouth the head's jaw keys (see the module
/// notes). `joints` is the merged skeleton the primitives are skinned to.
/// Returns the report: each key fitted, and the parts that took them.
pub fn follow_jaw(groups: &mut [NodeGroup], joints: &[Joint]) -> serde_json::Value {
    let Some(jaw) = joints.iter().position(|j| j.name.eq_ignore_ascii_case("FB_Jaw")) else {
        return serde_json::json!({"applied": false, "note": "no FB_Jaw joint"});
    };
    let under: Vec<bool> = (0..joints.len()).map(|mut i| loop {
        if i == jaw {
            break true;
        }
        match joints[i].parent {
            Some(p) => i = p,
            None => break false,
        }
    }).collect();
    let pivot = [joints[jaw].bind[12], joints[jaw].bind[13], joints[jaw].bind[14]];
    let is_face = |m: &(String, Vec<[f32; 3]>)| !m.0.starts_with("pose_");
    let Some((hg, hp)) = groups.iter().enumerate().find_map(|(g, gr)| gr.prims.iter().position(|p| p.morphs.iter().any(is_face)).map(|p| (g, p))) else {
        return serde_json::json!({"applied": false, "note": "no head with face shape keys"});
    };
    let weights: Vec<Vec<Option<Vec<f32>>>> = groups.iter().map(|g| g.prims.iter().map(|p| jaw_weights(p, &under)).collect()).collect();
    // the lower teeth (and tongue): vertices of other parts mostly under the jaw
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for (g, gr) in groups.iter().enumerate() {
        for (k, p) in gr.prims.iter().enumerate() {
            if (g, k) == (hg, hp) {
                continue;
            }
            let Some(w) = &weights[g][k] else { continue };
            for (v, &x) in p.positions.iter().zip(w) {
                if x > 0.5 {
                    for a in 0..3 {
                        lo[a] = lo[a].min(v[a]);
                        hi[a] = hi[a].max(v[a]);
                    }
                }
            }
        }
    }
    if lo[0] > hi[0] {
        return serde_json::json!({"applied": false, "note": "no teeth or tongue part skinned under FB_Jaw"});
    }
    let head = &groups[hg].prims[hp];
    let hw = weights[hg][hp].clone().unwrap_or_else(|| vec![0.0; head.positions.len()]);
    let (chin, keys) = match fit_keys(pivot, &head.positions, &hw, &head.morphs, lo, hi) {
        Ok(x) => x,
        Err(note) => return serde_json::json!({"applied": false, "note": note}),
    };
    let mut parts = Vec::new();
    if !keys.is_empty() {
        for (g, gr) in groups.iter_mut().enumerate() {
            for (k, p) in gr.prims.iter_mut().enumerate() {
                let Some(w) = &weights[g][k] else { continue };
                add_keys(&mut p.morphs, &keys, pivot, &p.positions, w);
                parts.push(format!("{}/{}", gr.name, p.name));
            }
        }
    }
    serde_json::json!({
        "applied": !keys.is_empty(),
        "chin_vertices": chin,
        // residuals: the share of the chin's motion each fit leaves unexplained
        "keys": keys.iter().map(|k| match k.motion {
            JawMotion::Turn(w) => serde_json::json!({"key": k.name, "turn_degrees": len(w).to_degrees(), "axis": w.map(|c| c / len(w).max(1e-9)), "turn_residual": k.turn_residual, "shift_residual": k.shift_residual}),
            JawMotion::Shift(t) => serde_json::json!({"key": k.name, "shift_mm": t.map(|c| c * 1000.0), "turn_residual": k.turn_residual, "shift_residual": k.shift_residual}),
        }).collect::<Vec<_>>(),
        "parts": parts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Vec<[f32; 3]> {
        (0..10).flat_map(|i| (0..10).map(move |k| [i as f32 * 0.005 - 0.02, -0.08 - k as f32 * 0.003, -0.05 - (i * k % 7) as f32 * 0.004])).collect()
    }

    #[test]
    fn fits_a_turn_about_the_pivot() {
        let pivot = [0.0, 0.0, 0.0];
        let w = [0.25, 0.0, 0.0];
        let p = grid();
        let d: Vec<[f32; 3]> = p.iter().map(|&v| sub(JawMotion::Turn(w).apply(pivot, v), v)).collect();
        let (m, res) = fit(pivot, &p, &d).unwrap();
        let JawMotion::Turn(got) = m else { panic!("{m:?}") };
        assert!(res < 0.05, "{res}");
        assert!((got[0] - 0.25).abs() < 0.02 && got[1].abs() < 0.02 && got[2].abs() < 0.02, "{got:?}");
    }

    #[test]
    fn fits_a_shift() {
        let p = grid();
        let d = vec![[0.0, -0.007, 0.0]; p.len()];
        let (m, res) = fit([0.0; 3], &p, &d).unwrap();
        let JawMotion::Shift(t) = m else { panic!("{m:?}") };
        assert!((t[1] + 0.007).abs() < 1e-6 && t[0].abs() < 1e-6 && t[2].abs() < 1e-6, "{t:?}");
        assert!(res < 1e-4);
    }

    #[test]
    fn small_motion_is_left_alone() {
        let p = grid();
        assert!(fit([0.0; 3], &p, &vec![[0.0, 0.0002, 0.0]; p.len()]).is_none());
    }
}
