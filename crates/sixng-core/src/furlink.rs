//! Fur: which strand grooms belong to an animal's fur variants.
//!
//! Nothing readable names a groom, but four facts together place them:
//!
//! 1. A fur preset (`partskin`, slot fur body / fur head, named after the
//!    3-vertex placeholder drawable `<root>_fur_<slot>_<variant>`) carries
//!    the 64-bit GUID of a fur asset (`41acf8eb`).
//! 2. One unnamed resource after the groom block is a hash map from asset
//!    GUID to the asset's groom piece ids (`piece_table`). The ids are
//!    file-name hashes and occur nowhere else, but they say how many pieces
//!    a variant has and which pieces variants of one animal share.
//! 3. packs.rpf stores resources in name order, and a groom file is named
//!    after its fur asset: an animal's grooms are one run of the archive,
//!    its variants follow each other in name order, and a piece shared by
//!    several variants is stored once, with the first of them.
//! 4. The strand roots of an animal's grooms lie on its skin, which finds
//!    the run (`fit`).
//!
//! Placeholders without a preset (12 % of them) have no piece count, a few
//! counts are off by one, and some variants' files are not in the archive;
//! the walk (`align`) allows for all three and settles them by how close
//! each piece's roots are to the skin and to the right part (body / head).
//! All of this is inferred from order and geometry, not read from a stored
//! link.

use crate::groom::GroomInfo;
use crate::resource::Resource;
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

const CACHE_VERSION: u32 = 1;
/// Most pieces a placeholder without a preset is given.
const MAX_UNKNOWN: usize = 14;
/// Roots this close to the skin (median, metres) mark an animal's run.
const FIT: f32 = 0.006;
/// Closeness scale of the walk: a piece at this distance scores 0 (long
/// fur sits up to 15 mm off the skin mesh, other animals' fur 25 mm and more).
const NEAR: f32 = 0.018;
/// Pieces looked at on either side of the run.
const WINDOW: usize = 40;

/// One groom piece: a groom with its LODs (archive offsets, densest first).
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub lods: Vec<u64>,
    pub strands: u32,
    pub root_box: [[f32; 3]; 2],
}

/// Group the catalog's grooms (archive order) into pieces. A groom joins
/// one of the last three pieces when it follows the same bones with fewer
/// strands than that piece's last LOD; for a piece further back than the
/// previous one the root boxes must also agree. That covers LODs whose
/// names sort apart (`x`, `x_fluff`, `x_fluff_lod1` …, `x_lod1` …).
pub fn pieces(cat: &[GroomInfo]) -> Vec<Piece> {
    let close = |a: &[[f32; 3]; 2], b: &[[f32; 3]; 2]| (0..3).all(|k| (a[0][k] - b[0][k]).abs() < 0.002 && (a[1][k] - b[1][k]).abs() < 0.002);
    let mut out: Vec<(Piece, BTreeSet<&str>, u32)> = Vec::new();
    for g in cat {
        let bones: BTreeSet<&str> = g.bones.iter().map(|s| s.as_str()).collect();
        let n = out.len();
        let hit = (1..=3.min(n)).find(|&back| {
            let (p, b, last) = &out[n - back];
            *b == bones && g.strands < *last && (back == 1 || close(&p.root_box, &g.root_box))
        });
        match hit {
            Some(back) => {
                let e = &mut out[n - back];
                e.0.lods.push(g.off);
                e.2 = g.strands;
            }
            None => out.push((Piece { lods: vec![g.off], strands: g.strands, root_box: g.root_box }, bones, g.strands)),
        }
    }
    out.into_iter().map(|e| e.0).collect()
}

/// The GUID -> piece ids map: the unnamed resource whose header has 0x100
/// groups at 0x10 and the map pointer 0x50000060 at 0x18. Groups are 16
/// control bytes and 16 slots of {u64 GUID, 16 zero bytes, pointer}; the
/// pointer leads to {u32 byte length, u64 ids}. Records of other shapes
/// (other asset types) are skipped.
pub fn piece_table(archive: &Path, all: &[crate::scan::Entry]) -> HashMap<u64, Vec<u64>> {
    let mut out = HashMap::new();
    let cands: Vec<&crate::scan::Entry> = all.iter().filter(|e| e.name.is_none() && e.kind == crate::classify::Kind::RageObj && e.usize >= 0x8000).collect();
    let found: Vec<HashMap<u64, Vec<u64>>> = cands
        .par_iter()
        .filter_map(|e| {
            let s = Resource::load_sys(archive, e).ok()?;
            if s.sys.len() < 0x70 || s.u32(0x10) != 0x100 || s.u64(0x18) != 0x5000_0060 || crate::groom::is_groom(&s.sys) {
                return None;
            }
            let r = Resource::load(archive, e).ok()?;
            let mut b = r.sys.clone();
            b.extend_from_slice(&r.gfx);
            Some(parse_table(&b))
        })
        .collect();
    for m in found {
        out.extend(m);
    }
    out
}

fn parse_table(b: &[u8]) -> HashMap<u64, Vec<u64>> {
    let u64at = |o: usize| b.get(o..o + 8).map(|x| u64::from_le_bytes(x.try_into().unwrap()));
    let u32at = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()));
    let groups = u32at(0x10).unwrap_or(0) as usize;
    let mut out = HashMap::new();
    for g in 0..groups {
        for s in 0..16 {
            let o = 0x60 + g * 528 + 16 + s * 32;
            let (Some(guid), Some(z1), Some(z2), Some(p)) = (u64at(o), u64at(o + 8), u64at(o + 16), u64at(o + 24)) else { return out };
            if guid == 0 || z1 != 0 || z2 != 0 || p >> 28 != 5 {
                continue;
            }
            let q = (p & 0x0fff_ffff) as usize;
            let Some(n) = u32at(q).map(|n| n as usize) else { continue };
            if n == 0 || n % 8 != 0 || n > 2048 {
                continue;
            }
            let ids: Option<Vec<u64>> = (0..n / 8).map(|i| u64at(q + 4 + 8 * i)).collect();
            if let Some(ids) = ids {
                out.insert(guid, ids);
            }
        }
    }
    out
}

/// One fur asset of the walk.
#[derive(Debug, Clone)]
pub struct Asset {
    pub name: String,
    pub head: bool,
    /// Animal root (pieces are shared within one root only).
    pub root: usize,
    /// Piece ids from the table; None for a placeholder without a preset.
    pub ids: Option<Vec<u64>>,
}

/// A piece as the walk sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seen {
    /// Median distance of its strand roots from the skin (metres); None
    /// when its root box lies outside the skin's box.
    pub dist: Option<f32>,
    /// Nearer the head part than the body part.
    pub head: bool,
}

/// How well a piece suits an asset of the given slot: 1 on the skin, 0 at
/// `NEAR`, down to -1; a piece on the other part scores at most -0.5.
fn gain(head: bool, s: &Seen) -> f32 {
    let Some(d) = s.dist else { return -1.0 };
    let c = (1.0 - d / NEAR).clamp(-1.0, 1.0);
    if s.head == head { c } else { c.min(0.0) - 0.5 }
}

/// Where each asset's own (new) pieces sit in a window of pieces.
#[derive(Debug, Clone, PartialEq)]
pub struct Alignment {
    /// Per asset: range of window positions it introduces.
    pub ranges: Vec<std::ops::Range<usize>>,
    pub score: f32,
}

/// Walk `assets` (name order) over a window of pieces (archive order).
/// The walk may start and end anywhere in the window. A known asset takes
/// its new ids (those no earlier asset of its root listed): exactly that
/// many, or one more or fewer (penalty 1), or none when its files are not
/// in the archive (penalty 1.5). An unknown one takes 0..=MAX_UNKNOWN. The
/// best walk is the one whose pieces lie closest to the skin on the part
/// their asset belongs to (`gain`; unknown assets count for 0.3).
pub fn align(assets: &[Asset], seen: &[Seen]) -> Alignment {
    let b = seen.len();
    let mut ids_seen: HashMap<usize, BTreeSet<u64>> = HashMap::new();
    let counts: Vec<Option<usize>> = assets
        .iter()
        .map(|a| {
            a.ids.as_ref().map(|ids| {
                let s = ids_seen.entry(a.root).or_default();
                ids.iter().filter(|id| s.insert(**id)).count()
            })
        })
        .collect();
    // prefix sums of the gain for either slot
    let pre = |head: bool| {
        let mut v = vec![0.0f32; b + 1];
        for q in 0..b {
            v[q + 1] = v[q] + gain(head, &seen[q]);
        }
        v
    };
    let (pb, ph) = (pre(false), pre(true));
    const NEG: f32 = f32::MIN;
    let n = assets.len();
    // dp[i][p]: best score with assets[..i] placed, the last ending at p
    let mut dp = vec![vec![NEG; b + 1]; n + 1];
    let mut from = vec![vec![0usize; b + 1]; n + 1];
    dp[0].fill(0.0);
    for (i, a) in assets.iter().enumerate() {
        let sum = |p: usize, k: usize| if a.head { ph[p + k] - ph[p] } else { pb[p + k] - pb[p] };
        let options: Vec<(usize, f32, f32)> = match counts[i] {
            // (pieces, penalty, weight)
            Some(0) => vec![(0, 0.0, 1.0)],
            Some(c) => {
                let mut v = vec![(c, 0.0, 1.0), (c + 1, 1.0, 1.0), (0, 1.5, 1.0)];
                if c >= 2 {
                    v.push((c - 1, 1.0, 1.0));
                }
                v
            }
            None => (0..=MAX_UNKNOWN).map(|k| (k, 0.0, 0.3)).collect(),
        };
        for p in 0..=b {
            let base = dp[i][p];
            if base == NEG {
                continue;
            }
            for &(k, pen, w) in &options {
                if p + k > b {
                    continue;
                }
                let s = base + w * sum(p, k) - pen;
                if s > dp[i + 1][p + k] {
                    dp[i + 1][p + k] = s;
                    from[i + 1][p + k] = p;
                }
            }
        }
    }
    let mut end = 0;
    for p in 0..=b {
        if dp[n][p] > dp[n][end] {
            end = p;
        }
    }
    let mut ranges = vec![0..0; n];
    let mut p = end;
    for i in (0..n).rev() {
        let q = from[i + 1][p];
        ranges[i] = q..p;
        p = q;
    }
    Alignment { ranges, score: dp[n][end] }
}

/// One fur variant of an animal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Variant {
    /// Placeholder name (`crm_cat_feral_m_00_fur_body_short_00`).
    pub name: String,
    /// "body" or "head".
    pub slot: String,
    /// Densest groom of each piece (archive offsets).
    pub grooms: Vec<u64>,
    /// The piece count came from the piece table (false: a placeholder
    /// without a preset, sized by what the walk left).
    pub known: bool,
    /// The walk gave it exactly the table's count (false: one more or
    /// fewer, or none because its files are not in the archive).
    pub exact: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FurLink {
    pub root: String,
    pub variants: Vec<Variant>,
    /// Animal roots walked together (their grooms fit the same skin).
    pub roots: Vec<String>,
    /// Pieces the walk placed for these roots, and how many of them lie
    /// near the skin on their own part.
    pub block: usize,
    pub fitting: usize,
    /// `fitting` over `block` for the variants with a known count (0..1).
    pub agreement: f32,
    pub note: String,
}

impl FurLink {
    /// The variant to show first for a slot: medium, else short, else the
    /// first with grooms.
    pub fn default_variant(&self, slot: &str) -> Option<&Variant> {
        let v: Vec<&Variant> = self.variants.iter().filter(|v| v.slot == slot && !v.grooms.is_empty()).collect();
        ["_medium_00", "_short_00"].iter().find_map(|s| v.iter().find(|x| x.name.ends_with(s)).copied()).or_else(|| v.first().copied())
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    version: u32,
    links: BTreeMap<String, FurLink>,
}

/// Skin points denser than the vertices: nearest-point distances to a
/// coarse mesh would otherwise be as large as its edges.
pub fn skin_points(verts: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut out = verts.to_vec();
    let mid = |a: [f32; 3], b: [f32; 3]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0];
    for t in indices.chunks_exact(3) {
        let (Some(&a), Some(&b), Some(&c)) = (verts.get(t[0] as usize), verts.get(t[1] as usize), verts.get(t[2] as usize)) else { continue };
        out.push([(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0]);
        out.extend([mid(a, b), mid(b, c), mid(c, a)]);
    }
    out
}

/// Median distance from `roots` to the nearest of `pts`.
fn median_dist(roots: &[[f32; 3]], pts: &[[f32; 3]]) -> f32 {
    if pts.is_empty() || roots.is_empty() {
        return f32::MAX;
    }
    let mut d: Vec<f32> = roots
        .iter()
        .map(|r| pts.iter().map(|p| (r[0] - p[0]).powi(2) + (r[1] - p[1]).powi(2) + (r[2] - p[2]).powi(2)).fold(f32::MAX, f32::min).sqrt())
        .collect();
    d.sort_by(|a, b| a.total_cmp(b));
    d[d.len() / 2]
}

/// Fur assets of the animal roots, in name order: every `<root>_fur_…`
/// placeholder drawable or preset, with its piece ids where a preset names
/// a fur asset.
fn assets_of(roots: &[String], all: &[crate::scan::Entry], skins: &crate::partskin::PartSkins, table: &HashMap<u64, Vec<u64>>) -> Vec<Asset> {
    let mut by: BTreeMap<String, (usize, Option<Vec<u64>>)> = BTreeMap::new();
    for (ri, root) in roots.iter().enumerate() {
        let pre = format!("{root}_fur_");
        for n in all.iter().filter_map(|e| e.name.as_deref()).filter(|n| n.starts_with(&pre)) {
            by.entry(n.to_string()).or_insert((ri, None));
        }
        for p in skins.sets.iter().flat_map(|s| &s.presets) {
            let Some(own) = p.own.as_deref().filter(|o| o.starts_with(&pre)) else { continue };
            let e = by.entry(own.to_string()).or_insert((ri, None));
            if p.guid != 0 {
                if let Some(ids) = table.get(&p.guid) {
                    e.1 = Some(ids.clone());
                }
            }
        }
    }
    by.into_iter()
        .map(|(name, (root, ids))| {
            let head = name.split("_fur_").nth(1).is_some_and(|s| s.starts_with("head"));
            Asset { name, head, root, ids }
        })
        .collect()
}

/// Root of a fur placeholder name.
fn fur_root(name: &str) -> Option<&str> {
    name.split_once("_fur_").map(|x| x.0)
}

/// Link the fur of animal `root`. `skin`: its parts as (part name, points
/// in RAGE model space, see `skin_points`). Results are cached in
/// `<work>/index/fur_links.json`.
pub fn for_animal(work: &Path, a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry], root: &str, skin: &[(String, Vec<[f32; 3]>)]) -> Result<FurLink> {
    let root = root.to_ascii_lowercase();
    let cache_path = work.join("index").join("fur_links.json");
    let mut cache: Cache = std::fs::read(&cache_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(|c: &Cache| c.version == CACHE_VERSION).unwrap_or_default();
    if let Some(l) = cache.links.get(&root) {
        return Ok(l.clone());
    }
    let link = link(work, a, all, &root, skin)?;
    cache.version = CACHE_VERSION;
    cache.links.insert(root, link.clone());
    if let Ok(b) = serde_json::to_vec(&cache) {
        let _ = std::fs::write(&cache_path, b);
    }
    Ok(link)
}

fn link(work: &Path, a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry], root: &str, skin: &[(String, Vec<[f32; 3]>)]) -> Result<FurLink> {
    let pts: Vec<[f32; 3]> = skin.iter().flat_map(|s| s.1.iter().copied()).collect();
    if pts.is_empty() {
        return Err(anyhow!("no skin mesh to fit the fur to"));
    }
    let cat = crate::groom::catalog_cached(work, a, all)?;
    let pcs = pieces(&cat);
    let by_off: HashMap<u64, &crate::scan::Entry> = all.iter().map(|e| (e.off, e)).collect();
    // candidates: root box inside the skin's box
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &pts {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let m = 0.04;
    let inside = |b: &[[f32; 3]; 2]| (0..3).all(|k| b[0][k] >= lo[k] - m && b[1][k] <= hi[k] + m);
    let head_pts: Vec<[f32; 3]> = skin.iter().filter(|s| s.0.contains("_head_")).flat_map(|s| s.1.iter().copied()).collect();
    let body_pts: Vec<[f32; 3]> = skin.iter().filter(|s| s.0.contains("_body_")).flat_map(|s| s.1.iter().copied()).collect();
    // per piece: distance of its roots from the skin, and the nearer part
    let seen_all: Vec<Seen> = pcs
        .par_iter()
        .map(|p| {
            let none = Seen { dist: None, head: false };
            if !inside(&p.root_box) {
                return none;
            }
            let Some(g) = by_off.get(&p.lods[0]).and_then(|e| Resource::load(&a.path, e).ok()).and_then(|r| crate::groom::parse(&r).ok()) else { return none };
            let k = g.points_per_strand.max(1);
            let step = (g.strands / 40).max(1);
            let roots: Vec<[f32; 3]> = (0..g.strands).step_by(step).filter_map(|s| g.points.get(s * k).copied()).collect();
            let head = !head_pts.is_empty() && (body_pts.is_empty() || median_dist(&roots, &head_pts) < median_dist(&roots, &body_pts));
            Seen { dist: Some(median_dist(&roots, &pts)), head }
        })
        .collect();
    // the run: the largest group of pieces on the skin (gaps of up to 3)
    let hits: Vec<usize> = seen_all.iter().enumerate().filter(|x| x.1.dist.is_some_and(|d| d < FIT)).map(|x| x.0).collect();
    let mut run: Option<(usize, usize, usize)> = None; // (first, last, fits)
    let mut i = 0;
    while i < hits.len() {
        let mut j = i;
        while j + 1 < hits.len() && hits[j + 1] - hits[j] <= 4 {
            j += 1;
        }
        if run.is_none_or(|b| j - i + 1 > b.2) {
            run = Some((hits[i], hits[j], j - i + 1));
        }
        i = j + 1;
    }
    let (first, last, _) = run.ok_or_else(|| anyhow!("no groom fits this animal's skin"))?;
    let w0 = first.saturating_sub(WINDOW);
    let seen = &seen_all[w0..(last + 1 + WINDOW).min(pcs.len())];

    // roots with fur, in name order; this one with up to three neighbours
    // of the same species on either side (animals that share a skin share
    // the run, and walking them together keeps their variants apart)
    let skins = crate::partskin::get(&crate::export::game_root(a)).map_err(|e| anyhow!(e))?;
    let table = piece_table(&a.path, all);
    if table.is_empty() {
        return Err(anyhow!("fur piece table not found in {}", a.id));
    }
    let mut roots: BTreeSet<String> = all.iter().filter_map(|e| e.name.as_deref()).filter(|n| crate::outfit::is_animal(n)).filter_map(fur_root).map(str::to_string).collect();
    roots.extend(skins.sets.iter().flat_map(|s| &s.presets).filter_map(|p| p.own.as_deref()).filter_map(fur_root).map(str::to_string));
    let roots: Vec<String> = roots.into_iter().collect();
    let Some(at) = roots.iter().position(|r| *r == root) else {
        return Err(anyhow!("{root} has no fur variants"));
    };
    if std::env::var_os("SIXNG_DEBUG_FUR").is_some() {
        let w: Vec<String> = seen.iter().map(|s| s.dist.map_or("-".into(), |d| format!("{}{}", if s.head { 'h' } else { 'b' }, ((d * 1000.0) as i32).min(99)))).collect();
        eprintln!("[fur] {root}: run {first}..={last}; mm from skin from piece {w0}: {}", w.join(" "));
        for r in &roots[at.saturating_sub(2)..(at + 3).min(roots.len())] {
            let mut ids = BTreeSet::new();
            let v: Vec<String> = assets_of(std::slice::from_ref(r), all, skins, &table).iter().map(|a| format!("{}:{}", a.name.split("_fur_").nth(1).unwrap_or(""), a.ids.as_ref().map_or("?".to_string(), |x| x.iter().filter(|i| ids.insert(**i)).count().to_string()))).collect();
            eprintln!("[fur]   {r}: {}", v.join(" "));
        }
    }
    let species = |r: &str| r.split('_').take(2).collect::<Vec<_>>().join("_");
    let mut best: Option<(usize, usize, Vec<Asset>, Alignment)> = None;
    for l in 0..=3.min(at) {
        for r in 0..=3.min(roots.len() - 1 - at) {
            let set = &roots[at - l..=at + r];
            if set.iter().any(|x| species(x) != species(root)) {
                continue;
            }
            let assets = assets_of(set, all, skins, &table);
            let al = align(&assets, seen);
            // more roots only when they explain more
            if best.as_ref().is_none_or(|b| al.score > b.3.score + 0.01) {
                best = Some((l, r, assets, al));
            }
        }
    }
    let (l, r, assets, al) = best.ok_or_else(|| anyhow!("{root} has no fur variants"))?;
    let set = &roots[at - l..=at + r];
    // id -> piece, per root: the j-th new id of an asset is the j-th piece of its range
    let mut id_piece: HashMap<(usize, u64), usize> = HashMap::new();
    let mut variants = Vec::new();
    let (mut block, mut fitting, mut known, mut known_fit) = (0, 0, 0, 0);
    for (asset, range) in assets.iter().zip(&al.ranges) {
        let mut grooms: Vec<u64> = Vec::new();
        let mut exact = true;
        match &asset.ids {
            Some(ids) => {
                let mut next = range.clone();
                let mut new = 0;
                for id in ids {
                    let pi = match id_piece.get(&(asset.root, *id)) {
                        Some(pi) => Some(*pi),
                        None => {
                            new += 1;
                            let q = next.next();
                            if let Some(q) = q {
                                id_piece.insert((asset.root, *id), w0 + q);
                            }
                            q.map(|q| w0 + q)
                        }
                    };
                    if let Some(pi) = pi {
                        grooms.push(pcs[pi].lods[0]);
                    }
                }
                exact = new == range.len();
                // one piece more than the table lists
                grooms.extend(next.map(|q| pcs[w0 + q].lods[0]));
            }
            None => grooms.extend(range.clone().map(|q| pcs[w0 + q].lods[0])),
        }
        for q in range.clone() {
            let good = gain(asset.head, &seen[q]) > 0.0;
            block += 1;
            fitting += good as usize;
            if asset.ids.is_some() {
                known += 1;
                known_fit += good as usize;
            }
        }
        if asset.root == l {
            variants.push(Variant { name: asset.name.clone(), slot: if asset.head { "head" } else { "body" }.into(), grooms, known: asset.ids.is_some(), exact });
        }
    }
    if variants.iter().all(|v| v.grooms.is_empty()) {
        return Err(anyhow!("none of the fur variants of {root} could be placed on its skin"));
    }
    Ok(FurLink {
        root: root.to_string(),
        variants,
        roots: set.to_vec(),
        block,
        fitting,
        agreement: if known == 0 { 0.0 } else { known_fit as f32 / known as f32 },
        note: "Inferred from archive order, the fur piece table and a fit of strand roots to the skin; not a stored link. known: false = a placeholder without a preset, given the pieces the others leave; exact: false = the walk used one piece more or fewer than the table lists, or none.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(off: u64, strands: u32, bones: &[&str], x: f32) -> GroomInfo {
        GroomInfo { off, id: String::new(), strands, root_triangles: 0, bones: bones.iter().map(|s| s.to_string()).collect(), root_box: [[x, 0.0, 0.0], [x + 0.1, 0.1, 0.1]], category: String::new(), chain: 0, lod: 0 }
    }

    #[test]
    fn lods_into_pieces() {
        // a, then b with its LODs, then a's LODs (names sort apart); bone order differs
        let cat = [g(1, 571, &["x", "y"], 0.0), g(2, 827, &["x", "y"], 0.001), g(3, 413, &["y", "x"], 0.001), g(4, 124, &["x", "y"], 0.001), g(5, 285, &["x", "y"], 0.0), g(6, 196, &["x", "y"], 0.0), g(7, 500, &["z"], 0.0)];
        let p = pieces(&cat);
        assert_eq!(p.iter().map(|p| p.lods.clone()).collect::<Vec<_>>(), [vec![1, 5, 6], vec![2, 3, 4], vec![7]]);
    }

    #[test]
    fn table() {
        // one group, slot 3: GUID 7 -> two ids; slot 5: a record of another shape
        let mut b = vec![0u8; 0x60 + 528 + 64];
        b[0x10..0x14].copy_from_slice(&1u32.to_le_bytes());
        let rec = 0x60 + 528;
        let slot = |b: &mut Vec<u8>, s: usize, guid: u64, ptr: u64| {
            let o = 0x60 + 16 + s * 32;
            b[o..o + 8].copy_from_slice(&guid.to_le_bytes());
            b[o + 24..o + 32].copy_from_slice(&ptr.to_le_bytes());
        };
        slot(&mut b, 3, 7, 0x5000_0000 + rec as u64);
        slot(&mut b, 5, 9, 0x5000_0000 + rec as u64 + 32);
        b[rec..rec + 4].copy_from_slice(&16u32.to_le_bytes());
        b[rec + 4..rec + 12].copy_from_slice(&11u64.to_le_bytes());
        b[rec + 12..rec + 20].copy_from_slice(&12u64.to_le_bytes());
        b[rec + 32..rec + 36].copy_from_slice(&1u32.to_le_bytes());
        let t = parse_table(&b);
        assert_eq!(t.len(), 1);
        assert_eq!(t[&7], [11, 12]);
    }

    fn asset(name: &str, head: bool, ids: Option<&[u64]>) -> Asset {
        Asset { name: name.into(), head, root: 0, ids: ids.map(|x| x.to_vec()) }
    }

    #[test]
    fn walk() {
        // body_long (no preset), two body variants sharing a piece, head_long
        // (no preset), one head variant
        let assets = [
            asset("r_fur_body_long_00", false, None),
            asset("r_fur_body_medium_00", false, Some(&[1, 2, 3])),
            asset("r_fur_body_short_00", false, Some(&[3, 4])),
            asset("r_fur_head_long_00", true, None),
            asset("r_fur_head_medium_00", true, Some(&[5, 6])),
        ];
        let far = Seen { dist: Some(0.08), head: false };
        let b = Seen { dist: Some(0.001), head: false };
        let h = Seen { dist: Some(0.001), head: true };
        // another animal's fur either side; 2 long body pieces, 3 + 1 new,
        // 1 long head piece, 2 head
        let seen = [far, far, b, b, b, b, b, b, h, h, h, far];
        let a = align(&assets, &seen);
        assert_eq!(a.ranges, [2..4, 4..7, 7..8, 8..9, 9..11]);
        // a variant whose files are not in the archive takes nothing
        let seen = [far, b, b, b, h, h, far];
        let a = align(&[assets[1].clone(), asset("r_fur_head_long_00", true, Some(&[7, 8, 9])), assets[4].clone()], &seen);
        assert_eq!(a.ranges, [1..4, 4..4, 4..6]);
        // loose long fur (12 mm) still beats the neighbour's (40 mm)
        assert!(gain(false, &Seen { dist: Some(0.012), head: false }) > 0.0);
        assert!(gain(false, &Seen { dist: Some(0.04), head: false }) < 0.0);
        assert!(gain(true, &b) < 0.0);
    }

    #[test]
    fn denser_skin() {
        let p = skin_points(&[[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], &[0, 1, 2]);
        assert_eq!(p.len(), 7);
        assert!((median_dist(&[[0.5, 0.5, 0.0]], &p) - 0.0).abs() < 1e-6);
    }
}
