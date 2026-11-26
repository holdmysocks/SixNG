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
//!   ramps of its colour set the game's strand shader takes),
//! - `COLOR_0` (when the piece's colours are known, `HairColour`): the
//!   strand's colour at that point, linear RGB.
//!
//! Each mesh's extras give `points_per_strand`, `strands`, the piece's slot,
//! placeholder, bone and groom offset, and in a character export
//! `head_mesh` and `head_uv_set`: the head mesh and which of its UV sets
//! (`TEXCOORD_n`) the root UVs are coordinates in (`root_uv_set`).
use crate::gltf::StrandPrim;
use crate::groom::{Coat, Groom, Shell};
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

/// The colours of a piece's strands, as the viewer draws them: per colour
/// variant, the colour along the strand.
pub enum HairColour {
    /// The game's colour set (`hairtone::StrandData`): four colour ramps,
    /// the atlas colour (sRGB-encoded) along the strand.
    Set(Box<[crate::hairtone::ColourRamp; 4]>),
    /// Without the strand tables: the item's colour preset per variant, its
    /// two colours from root to tip (`hairtone::ramp`, taken as linear).
    Presets(Box<[crate::hairtone::Preset; 4]>),
}

impl HairColour {
    /// Linear RGB of variant `variant` at `t` along the strand (0 root, 1 tip).
    pub fn at(&self, variant: u8, t: f32) -> [f32; 3] {
        let v = (variant & 3) as usize;
        match self {
            HairColour::Set(r) => r[v].at(t).map(|x| {
                let x = x.clamp(0.0, 1.0);
                if x < 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
            }),
            HairColour::Presets(p) => crate::hairtone::ramp(p[v].a, p[v].b, t),
        }
    }
}

/// Wardrobe item category of each strand slot, as the outfit's strand
/// colour items name it (`pedlook::Outfit::strand_colours`).
pub const SLOT_CATEGORY: [(&str, &str); 5] = [("hair", "864b03ae"), ("brows", "7c50ad43"), ("lashes", "83a664e7"), ("beard", "44d2709f"), ("body hair", "a228cc57")];

/// The colours of a strand slot of an outfit, as the viewer picks them:
/// the colour set the outfit's colour item gives the slot's category, else
/// the one the slot's item names (its hair preset), looked up in the game's
/// strand tables; without a set there, the item's own colour presets
/// (unless their flag marks them as not a hair colour). Returns the
/// colours and the set's name hash.
pub fn slot_colour(outfit: &crate::pedlook::Outfit, slot: &str, tables: Option<&crate::hairtone::StrandData>, presets: Option<&crate::hairtone::Table>) -> Option<(HairColour, u32)> {
    let hex = |h: &str| u32::from_str_radix(h, 16).ok();
    let preset = outfit.parts.iter().find(|p| p.strands && p.slot == slot).and_then(|p| p.hair_preset.as_deref()).and_then(hex);
    let cat = SLOT_CATEGORY.iter().find(|c| c.0 == slot).map(|c| c.1);
    let set = cat.and_then(|c| outfit.strand_colours.iter().find(|x| x.0 == c)).and_then(|x| hex(&x.1)).or(preset)?;
    if let Some(cs) = tables.and_then(|t| t.colour_set(set)) {
        return Some((HairColour::Set(Box::new(cs.clone())), set));
    }
    let p = preset?;
    let v = presets?.variants(p)?;
    (!v[0].flag).then(|| (HairColour::Presets(Box::new(v)), p))
}

/// Strands of one piece as a `StrandPrim`: `points` are n strands of `k`
/// points; `uv`, `clump` and `variant` one value per strand.
#[allow(clippy::too_many_arguments)]
fn prim(g: &Groom, name: String, k: usize, points: &[[f32; 3]], uv: &[[f32; 2]], clump: &[u32], variant: &[u8], colour: Option<&HairColour>, o: &Options, extras: Value) -> StrandPrim {
    let n = if k == 0 { 0 } else { points.len() / k };
    let kk = if o.points >= 2 { o.points } else { k };
    let mut pos = Vec::with_capacity(n * kk);
    let (mut width, mut uvs, mut cl, mut var) = (Vec::with_capacity(n * kk), Vec::with_capacity(n * kk), Vec::with_capacity(n * kk), Vec::with_capacity(n * kk));
    let mut col = Vec::with_capacity(if colour.is_some() { n * kk } else { 0 });
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
            if let Some(c) = colour {
                let v = c.at(variant.get(s).copied().unwrap_or(0), j as f32 / (kk.max(2) - 1) as f32);
                col.push([v[0], v[1], v[2], 1.0]);
            }
        }
    }
    let mut extras = extras;
    extras["points_per_strand"] = json!(kk);
    extras["strands"] = json!(n);
    StrandPrim { name, k: kk, positions: pos, uv: uvs, colour: col, attrs: vec![("_WIDTH".into(), width), ("_CLUMP".into(), cl), ("_VARIANT".into(), var)], extras }
}

/// `extras` with `head_mesh`, `head_uv_set` and `head_uv_error` when the
/// groom's root UVs match one of the head's UV sets (`root_uv_set`).
/// Returns the set.
pub fn tag_uv_set(g: &Groom, head: Option<&HeadUvs>, extras: &mut Value) -> Option<usize> {
    let h = head?;
    let (set, err) = root_uv_set(g, h)?;
    if !h.name.is_empty() {
        extras["head_mesh"] = json!(h.name);
    }
    extras["head_uv_set"] = json!(set);
    extras["head_uv_error"] = json!(err);
    Some(set)
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
/// `o.guides`. `extras` goes on every mesh; `colour` colours the grown
/// strands (COLOR_0). With `hat` the strands are pressed under it as the
/// viewer does (`Shell::press`, 3 mm under its lining): the guides before
/// the strands are grown, the grown strands again after (noise, curl and
/// clumps move them off the guides). Returns the pieces and a summary
/// (strands written, stored, guides, points pressed).
pub fn piece(g: &Groom, name: &str, o: &Options, colour: Option<&HairColour>, hat: Option<&Shell>, extras: Value) -> (Vec<StrandPrim>, Value) {
    let k = g.points_per_strand;
    let mut out = Vec::new();
    let mut pressed = 0;
    let under;
    let g = match hat {
        Some(h) => {
            let mut c = g.clone();
            pressed += h.press_strands(&mut c.points, k, 0.003);
            under = c;
            &under
        }
        None => g,
    };
    let sh = share(g, o);
    let mut coat = g.coat(sh);
    if let Some(h) = hat {
        pressed += h.press_strands(&mut coat.points, k, 0.003);
    }
    let grown = !coat.guides.is_empty();
    let mut ex = extras.clone();
    ex["grown"] = json!(grown);
    if grown {
        ex["share"] = json!(sh);
        out.push(prim(g, name.to_string(), k, &coat.points, &coat_uvs(g, &coat), &coat.clumps, &coat.variants, colour, o, ex));
    } else {
        // nothing stored to grow from: the guides are the hair
        out.push(prim(g, name.to_string(), k, &g.points, &g.root_uv, &[], &[], colour, o, ex));
    }
    if o.guides && grown {
        let mut ex = extras;
        ex["guides"] = json!(true);
        out.push(prim(g, format!("{name}_guides"), k, &g.points, &g.root_uv, &[], &[], None, o, ex));
    }
    let summary = json!({"name": name, "written": if grown { coat.guides.len() } else { g.strands }, "stored": g.stored_strands(), "guides": g.strands,
        "points_per_strand": if o.points >= 2 { o.points } else { k }, "grown": grown, "hat_pressed": hat.map(|_| pressed)});
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

/// A character's head mesh read back from its model GLB: the vertex
/// positions (glTF space) and every UV set (`TEXCOORD_n`).
pub struct HeadUvs {
    /// The head's node name (the Blender object name after import); empty
    /// when the file has no head-named mesh.
    pub name: String,
    pub pos: Vec<[f32; 3]>,
    pub sets: Vec<Vec<[f32; 2]>>,
}

/// The head mesh of a model GLB: the largest mesh named like a head
/// drawable (`…_head_…` or `head_…`, not its eyes), or in a file with no
/// such name (a head drawable exported on its own) its largest mesh. None
/// when there is none.
pub fn head_from_glb(b: &[u8]) -> Option<HeadUvs> {
    if b.len() < 20 || &b[0..4] != b"glTF" {
        return None;
    }
    let jl = u32::from_le_bytes(b[12..16].try_into().ok()?) as usize;
    let doc: Value = serde_json::from_slice(b.get(20..20 + jl)?).ok()?;
    let bo = 20 + jl;
    let bl = u32::from_le_bytes(b.get(bo..bo + 4)?.try_into().ok()?) as usize;
    let bin = b.get(bo + 8..bo + 8 + bl)?;
    // float accessor `i` as rows of `w` values
    let floats = |i: &Value, w: usize| -> Option<Vec<f32>> {
        let a = &doc["accessors"][i.as_u64()? as usize];
        if a["componentType"].as_u64()? != 5126 {
            return None;
        }
        let v = &doc["bufferViews"][a["bufferView"].as_u64()? as usize];
        let off = (v["byteOffset"].as_u64().unwrap_or(0) + a["byteOffset"].as_u64().unwrap_or(0)) as usize;
        let n = a["count"].as_u64()? as usize * w;
        let raw = bin.get(off..off + n * 4)?;
        Some(raw.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
    };
    let is_head = |n: &str| (n.contains("_head_") || n.starts_with("head_")) && !n.contains("eyes");
    let named = doc["nodes"].as_array()?.iter().any(|n| n["mesh"].is_u64() && n["name"].as_str().is_some_and(is_head));
    let mut best: Option<HeadUvs> = None;
    for node in doc["nodes"].as_array()? {
        let (Some(name), Some(mi)) = (node["name"].as_str(), node["mesh"].as_u64()) else { continue };
        if named && !is_head(name) {
            continue;
        }
        let Some(prim) = doc["meshes"][mi as usize]["primitives"].get(0) else { continue };
        let at = &prim["attributes"];
        let Some(p) = floats(&at["POSITION"], 3) else { continue };
        let pos: Vec<[f32; 3]> = p.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        let mut sets = Vec::new();
        while let Some(u) = at.get(format!("TEXCOORD_{}", sets.len())).and_then(|i| floats(i, 2)) {
            sets.push(u.chunks_exact(2).map(|c| [c[0], c[1]]).collect::<Vec<[f32; 2]>>());
        }
        if best.as_ref().is_none_or(|h| pos.len() > h.pos.len()) {
            // a head drawable on its own has generic mesh names: no name to give
            best = Some(HeadUvs { name: if named { name.to_string() } else { String::new() }, pos, sets });
        }
    }
    best
}

/// The hat of a model GLB the exporters wrote, as the viewer builds it for
/// pressing the scalp hair (`groom::Shell`, with the hat's triangles as its
/// lining): the meshes of the drawables `hats` (lower case; nodes are named
/// `<drawable>/<geometry>`), seen from 10 cm under the crown, in the middle
/// of the top 12 cm of the head. None without a hat or a head in the file.
pub fn hat_shell(b: &[u8], hats: &[String]) -> Option<Shell> {
    if hats.is_empty() || b.len() < 20 || &b[0..4] != b"glTF" {
        return None;
    }
    let jl = u32::from_le_bytes(b[12..16].try_into().ok()?) as usize;
    let doc: Value = serde_json::from_slice(b.get(20..20 + jl)?).ok()?;
    let bo = 20 + jl;
    let bl = u32::from_le_bytes(b.get(bo..bo + 4)?.try_into().ok()?) as usize;
    let bin = b.get(bo + 8..bo + 8 + bl)?;
    let is_head = |n: &str| n.contains("_head_") || n.starts_with("head_");
    let (mut hat, mut head, mut tris) = (Vec::new(), Vec::new(), Vec::new());
    for node in doc["nodes"].as_array()? {
        let (Some(name), Some(mi)) = (node["name"].as_str(), node["mesh"].as_u64()) else { continue };
        let drawable = name.split('/').next().unwrap_or(name).to_ascii_lowercase();
        let is_hat = hats.contains(&drawable);
        if !is_hat && !is_head(&drawable) {
            continue;
        }
        let Some(prim) = doc["meshes"][mi as usize]["primitives"].get(0) else { continue };
        let Some(i) = prim["attributes"]["POSITION"].as_u64() else { continue };
        let a = &doc["accessors"][i as usize];
        if a["componentType"].as_u64() != Some(5126) {
            continue;
        }
        let v = &doc["bufferViews"][a["bufferView"].as_u64().unwrap_or(0) as usize];
        let off = (v["byteOffset"].as_u64().unwrap_or(0) + a["byteOffset"].as_u64().unwrap_or(0)) as usize;
        let n = a["count"].as_u64().unwrap_or(0) as usize;
        let Some(raw) = bin.get(off..off + n * 12) else { continue };
        // glTF Y-up back to RAGE Z-up: (x, y, z) -> (x, -z, y)
        let pts: Vec<[f32; 3]> = raw
            .chunks_exact(12)
            .map(|c| {
                let f = |j: usize| f32::from_le_bytes([c[j], c[j + 1], c[j + 2], c[j + 3]]);
                [f(0), -f(8), f(4)]
            })
            .collect();
        if !is_hat {
            head.extend(pts);
            continue;
        }
        // the hat's triangles (u16 or u32 indices)
        if let Some(ia) = prim["indices"].as_u64().map(|i| &doc["accessors"][i as usize]) {
            let iv = &doc["bufferViews"][ia["bufferView"].as_u64().unwrap_or(0) as usize];
            let io = (iv["byteOffset"].as_u64().unwrap_or(0) + ia["byteOffset"].as_u64().unwrap_or(0)) as usize;
            let n = ia["count"].as_u64().unwrap_or(0) as usize;
            let idx: Vec<usize> = match ia["componentType"].as_u64() {
                Some(5125) => bin.get(io..io + n * 4).map(|r| r.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize).collect()),
                Some(5123) => bin.get(io..io + n * 2).map(|r| r.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as usize).collect()),
                _ => None,
            }
            .unwrap_or_default();
            tris.extend(idx.chunks_exact(3).filter_map(|t| Some([*pts.get(t[0])?, *pts.get(t[1])?, *pts.get(t[2])?])));
        }
        hat.extend(pts);
    }
    if hat.is_empty() || head.is_empty() {
        return None;
    }
    let top = head.iter().map(|p| p[2]).fold(f32::MIN, f32::max);
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in head.iter().filter(|p| p[2] > top - 0.12) {
        for a in 0..2 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    Some(Shell::new([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, top - 0.10], &hat, &head).with_lining(tris))
}

/// Which of the head's UV sets a groom's root UVs belong to: at every guide
/// root, the UV of the nearest head vertex in each set against the root's
/// stored UV; the set with the smallest median distance wins when that is
/// under 0.05 (UV units) and under half the next set's. Returns (set,
/// median distance). On the player
/// heads checked, scalp hair matched set 3 and brows, lashes and beards set
/// 2, to within 0.01; which set a groom uses is not found in the groom
/// itself, hence the match.
pub fn root_uv_set(g: &Groom, head: &HeadUvs) -> Option<(usize, f32)> {
    let k = g.points_per_strand.max(1);
    if g.root_uv.len() != g.strands || head.pos.is_empty() || head.sets.is_empty() {
        return None;
    }
    // the nearest head vertex to each guide root (glTF space, as the head)
    let near: Vec<usize> = (0..g.strands)
        .map(|s| {
            let r = g.points[s * k];
            let q = [r[0], r[2], -r[1]];
            let d = |p: &[f32; 3]| (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
            (0..head.pos.len()).min_by(|&a, &b| d(&head.pos[a]).total_cmp(&d(&head.pos[b]))).unwrap_or(0)
        })
        .collect();
    let mut med: Vec<(usize, f32)> = head
        .sets
        .iter()
        .enumerate()
        .map(|(si, set)| {
            let mut e: Vec<f32> = near.iter().zip(&g.root_uv).map(|(&v, uv)| ((set[v][0] - uv[0]).powi(2) + (set[v][1] - uv[1]).powi(2)).sqrt()).collect();
            e.sort_by(f32::total_cmp);
            (si, e[e.len() / 2])
        })
        .collect();
    med.sort_by(|a, b| a.1.total_cmp(&b.1));
    let next = med.get(1).map_or(f32::MAX, |m| m.1);
    med.first().copied().filter(|b| b.1 < 0.05 && b.1 < next * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_uvs_find_their_head_uv_set() {
        // a head of three vertices whose UV set 1 is the groom's root UV
        let head = HeadUvs { name: "h".into(), pos: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], sets: vec![vec![[0.9, 0.9], [0.1, 0.5], [0.5, 0.1]], vec![[0.2, 0.3], [0.4, 0.3], [0.2, 0.6]]] };
        // guide roots in RAGE space: glTF (x, y, z) is RAGE (x, -z, y)
        let pts = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.1], [1.0, 0.0, 0.0], [1.0, 0.0, 0.1], [0.0, 0.0, 1.0], [0.0, 0.0, 1.1]];
        let g = Groom { tint_weight: 1.0, strands: 3, points_per_strand: 2, points: pts, w: vec![0.0; 6], roots: vec![[0, 1, 2]], lengths: vec![], root_uv: vec![[0.2, 0.3], [0.4, 0.3], [0.2, 0.6]], bones: vec![], root_box: [[0.0; 3]; 2], group_box: None, tri_counts: vec![], width: 0.0003, taper: [255; 16], pick_steps: vec![], seed: 0, length_var: [0.0, 1.0], head: vec![], corners: vec![], clump_scale: [1.0; 2], styles: vec![], clump_type: 0, guide_tri: vec![], twist: vec![], curl: Default::default() };
        assert_eq!(root_uv_set(&g, &head), Some((1, 0.0)));
        // no set close enough
        let far = Groom { root_uv: vec![[0.7, 0.7]; 3], ..g.clone() };
        assert_eq!(root_uv_set(&far, &head), None);
    }

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
        let (p, s) = piece(&g, "t", &Options { points: 3, ..Default::default() }, None, None, json!({}));
        assert_eq!(p.len(), 2);
        assert_eq!((s["written"].as_u64(), s["guides"].as_u64()), (Some(50), Some(3)));
        let c = &p[0];
        assert_eq!((c.k, c.positions.len(), c.uv.len()), (3, 150, 150));
        let clump = &c.attrs.iter().find(|a| a.0 == "_CLUMP").unwrap().1;
        // 40 scattered strands, then 10 in two clumps
        assert!(clump[..120].iter().all(|&x| x == 0.0) && clump[120..].iter().all(|&x| x == 1.0 || x == 2.0));
        assert!(clump[120..].contains(&2.0));
        // the cap
        let (p, s) = piece(&g, "t", &Options { max_strands: 10, guides: false, ..Default::default() }, None, None, json!({}));
        assert!(p[0].colour.is_empty());
        // with colours: one per point, root to tip of the strand's variant
        let pr = |a: [u8; 3], b: [u8; 3]| crate::hairtone::Preset { a, b, texture: 0, flag: false, values: Default::default() };
        let hc = HairColour::Presets(Box::new([pr([255, 0, 0], [0, 0, 255]), pr([0, 255, 0], [0, 255, 0]), pr([0, 255, 0], [0, 255, 0]), pr([0, 255, 0], [0, 255, 0])]));
        let (p, _) = piece(&g, "t", &Options { points: 3, guides: false, ..Default::default() }, Some(&hc), None, json!({}));
        let c = &p[0];
        assert_eq!(c.colour.len(), c.positions.len());
        let var = &c.attrs.iter().find(|a| a.0 == "_VARIANT").unwrap().1;
        let s0 = (0..c.positions.len() / 3).find(|&s| var[s * 3] == 0.0).unwrap();
        assert_eq!(c.colour[s0 * 3], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(c.colour[s0 * 3 + 2], [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(p.len(), 1);
        assert!(s["written"].as_u64().unwrap() <= 11);
        let glb = crate::gltf::strands_glb("t", &p, json!({}));
        assert_eq!(&glb[0..4], b"glTF");
        // under a hat (a dome of 5 cm): every point above its rim, guide or
        // grown, ends up under its lining
        let mut dome = Vec::new();
        for i in 0..30 {
            for j in 0..120 {
                let (lat, lon) = ((i as f32 * 3.0).to_radians(), (j as f32 * 3.0).to_radians());
                dome.push([0.5 + 0.05 * lat.cos() * lon.cos(), 0.5 + 0.05 * lat.cos() * lon.sin(), 0.05 * lat.sin()]);
            }
        }
        let hat = Shell::new([0.5, 0.5, 0.0], &dome, &[]);
        let (p, s) = piece(&g, "t", &Options { points: 3, ..Default::default() }, None, Some(&hat), json!({}));
        assert!(s["hat_pressed"].as_u64().unwrap() > 0);
        for c in &p {
            for q in c.positions.iter().filter(|q| q[2] > 0.01) {
                let r = ((q[0] - 0.5).powi(2) + (q[1] - 0.5).powi(2) + q[2].powi(2)).sqrt();
                assert!(r < 0.05, "{} {q:?} {r}", c.name);
            }
        }
    }
}
