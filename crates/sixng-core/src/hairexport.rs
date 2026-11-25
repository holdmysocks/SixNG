//! Strand hair in model exports: the full strand set the game grows from a
//! groom's guides (`Groom::coat`, by the game's generation arithmetic in
//! `hairgen`), written as line strips with the per-strand values Blender's
//! hair curves use: root UV, width along the strand, clump and colour
//! variant. `scripts/blender_hair_curves.py` turns such a GLB into hair
//! curves objects.
//!
//! Every point of a piece carries:
//! - `TEXCOORD_0`: the strand's root UV (the barycentric blend of its
//!   three guides' root UVs; the same for every point of the strand),
//! - `_WIDTH`: the strand's width (metres) at that point,
//! - `_CLUMP`: the strand's clump (1-based; 0 = a scattered strand),
//! - `_VARIANT`: the strand's colour variant 0-3 (which of the four colour
//!   ramps of its colour set the game's strand shader takes).
//!
//! Each mesh's extras give `points_per_strand`, `strands`, the piece's slot,
//! placeholder, bone and groom offset.
use crate::gltf::StrandPrim;
use crate::groom::{Coat, Groom};
use serde_json::{json, Value};

/// How much of the hair to write.
#[derive(Debug, Clone)]
pub struct Options {
    /// Share of the strands the groom stores, 0..1.
    pub density: f32,
    /// At most this many strands per piece (0 = no cap).
    pub max_strands: usize,
    /// Points per strand, resampled along its length (0 = as stored).
    pub points: usize,
    /// Also write each piece's guide strands, as `<piece>_guides`.
    pub guides: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { density: 1.0, max_strands: 0, points: 0, guides: true }
    }
}

/// The share of `g`'s stored strands that `o` asks for.
pub fn share(g: &Groom, o: &Options) -> f32 {
    let mut s = o.density.clamp(0.0, 1.0);
    let stored = g.stored_strands();
    if o.max_strands > 0 && stored > 0 {
        s = s.min(o.max_strands as f32 / stored as f32);
    }
    s
}

/// `pts` (one strand) resampled to `k` points evenly spaced along its
/// length. The points past a strand's cut repeat its last one, so they
/// are dropped first.
pub fn resample(pts: &[[f32; 3]], k: usize) -> Vec<[f32; 3]> {
    if k < 2 || pts.len() < 2 {
        return pts.to_vec();
    }
    let d = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    let mut acc = vec![0.0f32];
    for w in pts.windows(2) {
        acc.push(acc.last().unwrap() + d(w[0], w[1]));
    }
    let total = *acc.last().unwrap();
    if total <= 0.0 {
        return vec![pts[0]; k];
    }
    let mut out = Vec::with_capacity(k);
    let mut seg = 0;
    for i in 0..k {
        let t = total * i as f32 / (k - 1) as f32;
        while seg + 2 < acc.len() && acc[seg + 1] < t {
            seg += 1;
        }
        let span = (acc[seg + 1] - acc[seg]).max(1e-12);
        let f = ((t - acc[seg]) / span).clamp(0.0, 1.0);
        let (a, b) = (pts[seg], pts[seg + 1]);
        out.push([a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f]);
    }
    out
}

/// Strands of one piece as a `StrandPrim`: `points` are n strands of `k`
/// points; `uv`, `clump` and `variant` one value per strand.
fn prim(g: &Groom, name: String, k: usize, points: &[[f32; 3]], uv: &[[f32; 2]], clump: &[u32], variant: &[u8], o: &Options, extras: Value) -> StrandPrim {
    let n = if k == 0 { 0 } else { points.len() / k };
    let kk = if o.points >= 2 { o.points } else { k };
    let mut pos = Vec::with_capacity(n * kk);
    let (mut width, mut uvs, mut cl, mut var) = (Vec::with_capacity(n * kk), Vec::with_capacity(n * kk), Vec::with_capacity(n * kk), Vec::with_capacity(n * kk));
    for s in 0..n {
        let src = &points[s * k..s * k + k];
        if kk == k {
            pos.extend_from_slice(src);
        } else {
            pos.extend(resample(src, kk));
        }
        for j in 0..kk {
            width.push(g.width_at(j, kk));
            uvs.push(uv.get(s).copied().unwrap_or([0.0, 0.0]));
            cl.push(clump.get(s).copied().unwrap_or(0) as f32);
            var.push(variant.get(s).copied().unwrap_or(0) as f32);
        }
    }
    let mut extras = extras;
    extras["points_per_strand"] = json!(kk);
    extras["strands"] = json!(n);
    StrandPrim { name, k: kk, positions: pos, uv: uvs, attrs: vec![("_WIDTH".into(), width), ("_CLUMP".into(), cl), ("_VARIANT".into(), var)], extras }
}

/// Root UV of every coat strand (blended from its guides).
pub fn coat_uvs(g: &Groom, c: &Coat) -> Vec<[f32; 2]> {
    c.guides
        .iter()
        .map(|(t, w)| {
            let at = |i: u32| g.root_uv.get(i as usize).copied().unwrap_or([0.0, 0.0]);
            let (a, b, d) = (at(t[0]), at(t[1]), at(t[2]));
            [a[0] * w[0] + b[0] * w[1] + d[0] * w[2], a[1] * w[0] + b[1] * w[1] + d[1] * w[2]]
        })
        .collect()
}

/// The strands of one groom for `strands_glb`: the grown coat (or the
/// guides when the groom stores nothing to grow), then the guides when
/// `o.guides`. `extras` goes on every mesh. Returns the pieces and a
/// summary (strands written, stored, guides).
pub fn piece(g: &Groom, name: &str, o: &Options, extras: Value) -> (Vec<StrandPrim>, Value) {
    let k = g.points_per_strand;
    let mut out = Vec::new();
    let sh = share(g, o);
    let coat = g.coat(sh);
    let grown = !coat.guides.is_empty();
    let mut ex = extras.clone();
    ex["grown"] = json!(grown);
    if grown {
        ex["share"] = json!(sh);
        out.push(prim(g, name.to_string(), k, &coat.points, &coat_uvs(g, &coat), &coat.clumps, &coat.variants, o, ex));
    } else {
        // nothing stored to grow from: the guides are the hair
        out.push(prim(g, name.to_string(), k, &g.points, &g.root_uv, &[], &[], o, ex));
    }
    if o.guides && grown {
        let mut ex = extras;
        ex["guides"] = json!(true);
        out.push(prim(g, format!("{name}_guides"), k, &g.points, &g.root_uv, &[], &[], o, ex));
    }
    let summary = json!({"name": name, "written": if grown { coat.guides.len() } else { g.strands }, "stored": g.stored_strands(), "guides": g.strands,
        "points_per_strand": if o.points >= 2 { o.points } else { k }, "grown": grown});
    (out, summary)
}

/// The groom pieces of a character's outfit: per strand part of outfit
/// `outfit` of `look` whose slot is in `slots` (all when empty), its slot,
/// the placeholder the grooms are placed under, and the densest groom of
/// every piece. Resolved as the viewer does: by the item's hair asset (for
/// the head's fit), else by the placeholder name.
pub fn character_grooms(work: &std::path::Path, a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry], look: &crate::pedlook::Look, outfit: usize, slots: &[String]) -> Result<Vec<(String, String, Vec<u64>, f32)>, String> {
    let t = crate::hairlink::get(work, a, all)?;
    let o = look.outfits.get(outfit).ok_or_else(|| format!("{} has {} outfits", look.model, look.outfits.len()))?;
    let mut v = Vec::new();
    for p in o.parts.iter().filter(|p| p.strands && (slots.is_empty() || slots.iter().any(|s| s.eq_ignore_ascii_case(&p.slot)))) {
        let guid = p.guid.as_deref().unwrap_or("");
        let l = u64::from_str_radix(guid, 16).ok().and_then(|g| t.resolve(g, look.fit.as_deref())).or_else(|| t.by_placeholder(&p.drawable));
        if let Some(l) = l {
            v.push((p.slot.clone(), l.placeholder.clone(), t.uncontested(l), l.confidence));
        }
    }
    Ok(v)
}

/// The grooms placed under placeholders matching `re` (other hair styles
/// and beards of a character), the most probable placement of each.
pub fn placeholder_grooms(work: &std::path::Path, a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry], re: &regex::Regex) -> Result<Vec<(String, String, Vec<u64>, f32)>, String> {
    let t = crate::hairlink::get(work, a, all)?;
    let mut seen = std::collections::BTreeMap::new();
    for l in t.links.values().flatten().filter(|l| !l.grooms.is_empty() && re.is_match(&l.placeholder)) {
        let e = seen.entry(l.placeholder.clone()).or_insert(l);
        if l.confidence > e.confidence {
            *e = l;
        }
    }
    Ok(seen.into_values().map(|l| (crate::pedlook::slot_of(&l.placeholder), l.placeholder.clone(), t.uncontested(l), l.confidence)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampled_strand_keeps_its_ends_and_spacing() {
        let pts = [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 3.0], [0.0, 0.0, 3.0]];
        let r = resample(&pts, 4);
        assert_eq!(r.len(), 4);
        assert!((r[1][2] - 1.0).abs() < 1e-6 && (r[2][2] - 2.0).abs() < 1e-6 && (r[3][2] - 3.0).abs() < 1e-6, "{r:?}");
        assert_eq!(resample(&pts, 0), pts.to_vec());
    }

    #[test]
    fn piece_writes_grown_strands_with_their_attributes() {
        let pts = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.1], [1.0, 0.0, 0.0], [1.0, 0.0, 0.1], [0.0, 1.0, 0.0], [0.0, 1.0, 0.1]];
        let g = Groom { tint_weight: 1.0, strands: 3, points_per_strand: 2, points: pts, w: vec![0.0; 6], roots: vec![[0, 1, 2]], lengths: vec![], root_uv: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], bones: vec![], root_box: [[0.0; 3]; 2], group_box: None, tri_counts: vec![[40, 10, 2, 0]], width: 0.0003, taper: [255; 16], pick_steps: vec![], seed: 0, length_var: [0.0, 1.0], head: vec![], corners: vec![], clump_scale: [1.0; 2], styles: vec![], clump_type: 0, guide_tri: vec![], twist: vec![], curl: Default::default() };
        let (p, s) = piece(&g, "t", &Options { points: 3, ..Default::default() }, json!({}));
        assert_eq!(p.len(), 2);
        assert_eq!((s["written"].as_u64(), s["guides"].as_u64()), (Some(50), Some(3)));
        let c = &p[0];
        assert_eq!((c.k, c.positions.len(), c.uv.len()), (3, 150, 150));
        let clump = &c.attrs.iter().find(|a| a.0 == "_CLUMP").unwrap().1;
        // 40 scattered strands, then 10 in two clumps
        assert!(clump[..120].iter().all(|&x| x == 0.0) && clump[120..].iter().all(|&x| x == 1.0 || x == 2.0));
        assert!(clump[120..].contains(&2.0));
        // the cap
        let (p, s) = piece(&g, "t", &Options { max_strands: 10, guides: false, ..Default::default() }, json!({}));
        assert_eq!(p.len(), 1);
        assert!(s["written"].as_u64().unwrap() <= 11);
        let glb = crate::gltf::strands_glb("t", &p, json!({}));
        assert_eq!(&glb[0..4], b"glTF");
    }
}
