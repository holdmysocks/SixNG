//! Hair on people: which strand grooms are a human hair asset (GUID).
//!
//! The link is stored, in three steps, and read exactly when the archive's
//! table of contents can be read (`gamecipher`):
//!
//! 1. A hair asset's GUID is the name id (`meta7a::asset_id`, the first
//!    half of MurmurHash3 x64 128) of its 3-vertex placeholder drawable,
//!    `_full` included (`hairlob_fr1_018_straight_l08_non_sm_n_full`).
//!    1,854 of the 1,879 assets of the piece table are named this way.
//! 2. The piece table (`furlink::piece_table`) lists, per asset GUID, the
//!    ids of its groom pieces. A piece id is the name id of the piece's
//!    file: the placeholder's name without `_full`, then a piece word
//!    (`…_sb_n_base`, `_hairline`, `_babyhair`, `_tail`, `_fringe` …).
//! 3. The table of contents gives every file its name id (column
//!    `rpf9toc::COL_FILE_ID`), so a piece id leads to one groom file.
//!    6,232 of the 6,238 ids of the table are files of the archive; 5,711
//!    of them are grooms (the rest are other files of the same resource
//!    type, listed for hats, shoes and animals).
//!
//! Only a piece's densest groom has an id in the table. Its LODs are
//! files of their own, `<piece>_lod1` …, so they are read by name too
//! once the piece's name is known: the placeholder's name without `_full`
//! and one of the piece words (`PIECE_WORDS`) whose name id is the
//! table's id. 2,859 of the 3,419 distinct groom pieces of the named
//! human assets are named so; a search over two- and three-word compounds
//! found no word for the rest. The LODs of the rest are attached by bones and falling strand
//! counts (`chains`), the one inferred part of an exact table; on the
//! named pieces that rule agrees with the names on 4,200 of 4,253 and
//! gave too few or too many LODs on the others.
//!
//! Without the table of contents (another platform, another game build)
//! the table is inferred instead, as before; all of that is a guess from
//! names, order and geometry:
//!
//! 1. Over all hair / fur assets the piece table has about as many
//!    distinct ids as the archive has groom pieces (5,591 against 5,599).
//! 2. The asset registries (PSO files, record struct `20e0f6ed`, type
//!    `2d4958cf` = 30 for hair / fur) carry a tag list (`e3da9c8b`) per
//!    asset. The tags are joaat hashes of the words of the asset's name:
//!    `hair`, `lob`, `wavy`, `l08`, `non`, `sf`, `n`, or the whole stem for
//!    a character's own asset (`cs_andrea_hairlob`). The body fit (`fr1`,
//!    `mr1`) and the number are not among them.
//! 3. An asset is matched to the placeholders made of the same words. A
//!    wardrobe item (tag `023e5a2f`, the placeholder prefix) settles some
//!    of the ties; many items name a stand-in placeholder of another
//!    style, which is ignored.
//! 4. packs.rpf stores resources in name order and a groom file is named
//!    after its asset: the pieces of the placeholders follow each other in
//!    placeholder order, each asset taking as many pieces as it has ids
//!    not listed by an earlier asset.
//! 5. Geometry pins the walk: strand roots of women's hair end at z 0.72,
//!    men's at 0.82; beards, brows and body hair sit elsewhere; the length
//!    word (`l01`..`l09`) follows the longest strand (20 mm .. 490 mm); the
//!    side word (`sl` / `sr`) follows the way the strands lean.
//!
//! The walk is a hidden Markov chain over the placeholders (name order)
//! and the pieces (archive order). An asset takes its count of pieces, one
//! more or fewer, or none; pieces of unknown assets may lie between. The
//! confidence of a link is the posterior probability of exactly that asset
//! on exactly that run of pieces. It is a model probability, not a
//! measured error rate, and against the exact table it is often wrong:
//! files are missing and unlisted pieces lie between, and one of either
//! shifts every asset after it.
//!
//! The table is built once (seconds when exact, about a minute when
//! inferred) and cached as `<work>/index/hair_links.json`.

use crate::classify::Kind;
use crate::groom::GroomInfo;
use crate::meta7a::{joaat, Meta};
use crate::resource::Resource;
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

const CACHE_VERSION: u32 = 5;
const ITEM: u32 = 0xf795_e8ba;
const TAG_PLACEHOLDER: u32 = 0x023e_5a2f;
const STATE_BASE: u32 = 0x44e2_1c90;
const REGISTRY_RECORD: u32 = 0x20e0_f6ed;
const TYPE_HAIR: i64 = 30;
/// Registry tag list holding the words of the asset's name.
const PROP_WORDS: u32 = 0xe3da_9c8b;
/// Fur slots (body, head): tags of animal assets.
const FUR_SLOTS: [u32; 2] = [0xbf48_3c65, 0x3438_22c2];
/// Placeholder words that make a hair placeholder (the word before the fit).
const STRAND: [&str; 13] = ["hair", "beard", "brows", "lashes", "bodyhair", "chesthair", "armpithair", "leghair", "genitalhair", "chin", "chops", "wigs", "stubble"];
/// Part words, longest first.
const CATS: [&str; 12] = ["armpithair", "bodyhair", "chesthair", "leghair", "genitalhair", "hair", "beard", "brows", "lashes", "chin", "chops", "wigs"];
/// Walk penalties (log): one piece more or fewer than listed, none at all,
/// one piece of no known asset.
const OFF_BY_ONE: f64 = -3.5;
const ABSENT: f64 = -4.0;
const GAP: f64 = -3.0;
/// Longest strand (metres) by length word and its spread (log): measured on
/// the characters' own assets, whose names alternate beard / hair and
/// women / men and so place themselves.
const HAIR_LEN: [(f64, f64); 9] = [(0.020, 0.65), (0.046, 0.65), (0.103, 0.55), (0.147, 0.55), (0.234, 0.33), (0.282, 0.30), (0.328, 0.30), (0.409, 0.33), (0.492, 0.30)];
const BEARD_LEN: [(f64, f64); 8] = [(0.020, 0.4), (0.023, 0.3), (0.024, 0.3), (0.027, 0.4), (0.033, 0.4), (0.045, 0.5), (0.08, 0.6), (0.2, 0.6)];
/// Lean of the strands sideways (tip minus root, metres) by side word.
const LEAN_X: [(&str, f64, f64); 5] = [("sl", 0.016, 0.010), ("sr", -0.017, 0.012), ("sm", 0.0, 0.006), ("sb", 0.0, 0.006), ("sf", 0.0, 0.006)];
/// Lean backwards for swept back / forward.
const LEAN_Y: [(&str, f64, f64); 2] = [("sb", 0.043, 0.032), ("sf", 0.005, 0.026)];

/// One placement of an asset: its placeholder and its grooms. In an exact
/// table every asset has one placement of confidence 1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Link {
    /// Placeholder drawable (`cs_ali_hairdreadlocks_fr1_000_curly_l05_f02_sb_n_full`).
    pub placeholder: String,
    /// Body fit of the placeholder (`fr1`, `mr1`, …).
    pub fit: String,
    /// hair / beard / brows / lashes / body; other for strands on clothing.
    pub part: String,
    /// 1 in an exact table. In an inferred one, the posterior probability
    /// of this asset on exactly these pieces (0..1).
    pub confidence: f32,
    /// Densest groom of each piece (archive offsets). Empty when the walk
    /// found the asset's files missing.
    pub grooms: Vec<u64>,
    /// The LOD chain of each piece, densest first (same order as `grooms`).
    pub chains: Vec<Vec<u64>>,
    /// Pieces the table lists for the asset that an earlier asset owns and
    /// the walk placed there with confidence 0.5 or more.
    pub shared: Vec<u64>,
    /// Pieces the table lists for this asset alone (the walk may have
    /// taken one more or fewer).
    pub listed: usize,
    /// Listed pieces that are neither in `grooms` nor in `shared`.
    pub missing: usize,
    /// Assets made of the same words that compete for this placeholder.
    pub rivals: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HairLinks {
    version: u32,
    pub note: String,
    /// The links were read (table of contents), not inferred.
    #[serde(default)]
    pub exact: bool,
    /// Asset GUID (16 hex digits) -> placements, most probable first.
    pub links: BTreeMap<String, Vec<Link>>,
    /// Exact table: every groom of the archive in its LOD chain (densest
    /// first, chains in archive order), read by name where the piece is
    /// named (`chains`). Empty when inferred.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lods: Vec<Vec<u64>>,
}

impl HairLinks {
    /// Set the catalog's `chain` and `lod` from `lods` (exact table); the
    /// catalog's own are inferred from neighbours. Grooms of no chain, and
    /// every groom when the table is inferred, keep theirs.
    pub fn relabel(&self, cat: &mut [GroomInfo]) {
        if self.lods.is_empty() {
            return;
        }
        let at: HashMap<u64, (u32, u32)> = self.lods.iter().enumerate().flat_map(|(c, ch)| ch.iter().enumerate().map(move |(l, o)| (*o, (c as u32 + 1, l as u32)))).collect();
        for g in cat {
            if let Some(&(c, l)) = at.get(&g.off) {
                g.chain = c;
                g.lod = l;
            }
        }
    }

    /// The grooms of hair asset `guid`: the most probable placement that
    /// has grooms, among those of body fit `fit` when given (`fr1` for a
    /// woman's head, `mr1` for a man's) and any otherwise. Check
    /// `confidence` before use: under 0.5 the pieces are a guess.
    pub fn resolve(&self, guid: u64, fit: Option<&str>) -> Option<&Link> {
        let v = self.links.get(&format!("{guid:016x}"))?;
        let has = |l: &&Link| !l.grooms.is_empty();
        fit.and_then(|f| v.iter().filter(has).find(|l| l.fit == f)).or_else(|| v.iter().find(has))
    }

    /// The placement made under a placeholder drawable (`cs_lucia_hairbob_…
    /// _full`), for a character whose hair is known only by its own
    /// placeholder: the most probable one that has grooms.
    pub fn by_placeholder(&self, name: &str) -> Option<&Link> {
        let n = name.trim_end_matches("_full");
        self.links.values().flatten().filter(|l| !l.grooms.is_empty() && l.placeholder.trim_end_matches("_full") == n).max_by(|a, b| a.confidence.total_cmp(&b.confidence))
    }

    /// The grooms of a link that no other asset claims with a higher
    /// confidence. Placements are each asset's own best, so two neighbours
    /// in the archive can both claim the piece between them; it goes to the
    /// stronger claim. Pieces an asset lists as shared with others stay.
    pub fn uncontested(&self, link: &Link) -> Vec<u64> {
        link.grooms
            .iter()
            .copied()
            .filter(|g| link.shared.contains(g) || !self.links.values().flatten().any(|o| o.confidence > link.confidence && o.placeholder != link.placeholder && o.grooms.contains(g) && !o.shared.contains(g)))
            .collect()
    }
}

/// The table (built on first use, then cached in the work directory).
pub fn get(work: &Path, a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> Result<&'static HairLinks, String> {
    static T: OnceLock<Result<HairLinks, String>> = OnceLock::new();
    T.get_or_init(|| {
        let cache = work.join("index").join("hair_links.json");
        let ixp = crate::scan::index_path(&work.join("index"), &a.id);
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let Ok(b) = std::fs::read(&cache) {
            if let Ok(t) = serde_json::from_slice::<HairLinks>(&b) {
                if t.version == CACHE_VERSION && stamp(&cache) >= stamp(&ixp) {
                    return Ok(t);
                }
            }
        }
        let t = build(work, a, all).map_err(|e| e.to_string())?;
        if let Ok(b) = serde_json::to_vec(&t) {
            let _ = crate::atomic::write(&cache, b);
        }
        Ok(t)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// A hair placeholder name taken apart.
#[derive(Debug, Clone, PartialEq)]
pub struct Placeholder {
    pub name: String,
    /// Everything before the fit (`hairlob`, `cs_andrea_hairlob`).
    pub stem: String,
    pub fit: String,
    /// The stem belongs to a character or model (more than one word).
    pub own: bool,
    /// Words of the name: part, style and what follows the number.
    pub words: BTreeSet<String>,
    pub part: &'static str,
}

fn is_fit(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 3 && (b[0] == b'm' || b[0] == b'f') && b[1].is_ascii_lowercase() && b[2].is_ascii_digit()
}

/// Take a placeholder name apart; None when it is not a hair placeholder
/// (no fit word, an animal, or another kind of part before the fit).
pub fn placeholder(name: &str) -> Option<Placeholder> {
    let t: Vec<&str> = name.split('_').collect();
    let i = t.iter().position(|x| is_fit(x)).filter(|&i| i >= 1 && i + 1 < t.len())?;
    let last = t[i - 1];
    if name.starts_with("cr") || !STRAND.iter().any(|s| last.starts_with(s)) {
        return None;
    }
    let stem = t[..i].join("_");
    let mut rest: Vec<&str> = t[i + 2..].to_vec();
    if rest.last() == Some(&"full") {
        rest.pop();
    }
    let cat = CATS.iter().find(|c| last.starts_with(**c)).copied();
    let mut words: BTreeSet<String> = rest.iter().filter(|w| !w.is_empty()).map(|w| w.to_string()).collect();
    match cat {
        Some(c) => {
            words.insert(c.to_string());
            if last.len() > c.len() {
                words.insert(last[c.len()..].to_string());
            }
        }
        // a part word the registries do not use: never equal to an asset's words
        None => {
            words.insert(format!("?{last}"));
        }
    }
    let part = part_of(&words, &stem);
    Some(Placeholder { name: name.to_string(), stem, fit: t[i].to_string(), own: i > 1, words, part })
}

fn part_of(words: &BTreeSet<String>, stem: &str) -> &'static str {
    let has = |k: &str| words.contains(k) || stem.contains(k);
    if ["armpithair", "bodyhair", "chesthair", "leghair", "genitalhair"].iter().any(|k| has(k)) {
        "body"
    } else if has("lashes") {
        "lashes"
    } else if has("brows") {
        "brows"
    } else if has("beard") || stem.contains("chin") || stem.contains("chops") || stem.contains("mustache") {
        "beard"
    } else {
        "hair"
    }
}

/// What the walk looks at in one groom piece (its densest LOD).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Shape {
    pub root_box: [[f32; 3]; 2],
    /// Mean root y.
    pub root_y: f32,
    /// Longest and median strand (metres along the strand).
    pub len_max: f32,
    pub len_med: f32,
    pub strands: u32,
    /// Mean tip minus mean root (x, y).
    pub lean: [f32; 2],
}

impl Shape {
    pub fn of(g: &crate::groom::Groom, root_box: [[f32; 3]; 2]) -> Shape {
        let k = g.points_per_strand.max(1);
        let d = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        let mut lens = Vec::with_capacity(g.strands);
        let (mut root, mut tip) = ([0f64; 3], [0f64; 3]);
        for s in 0..g.strands {
            let Some(p) = g.points.get(s * k..(s + 1) * k) else { break };
            lens.push(p.windows(2).map(|w| d(w[0], w[1])).sum::<f32>());
            for q in 0..3 {
                root[q] += p[0][q] as f64;
                tip[q] += p[k - 1][q] as f64;
            }
        }
        let n = lens.len().max(1) as f64;
        lens.sort_by(|a, b| a.total_cmp(b));
        Shape {
            root_box,
            root_y: (root[1] / n) as f32,
            len_max: lens.last().copied().unwrap_or(0.0),
            len_med: lens.get(lens.len() / 2).copied().unwrap_or(0.0),
            strands: g.strands as u32,
            lean: [((tip[0] - root[0]) / n) as f32, ((tip[1] - root[1]) / n) as f32],
        }
    }
    fn on_head(&self) -> bool {
        self.root_box[0][2] > 0.40 && self.root_box[1][2] < 0.90 && self.root_box[1][0] - self.root_box[0][0] < 0.30
    }
}

/// How well a piece suits a part of a body fit: about 0.5 for a fit, down
/// to -4 for a piece that cannot be it (body hair on a head, a man's scalp
/// for a woman's style).
pub fn suits(part: &str, fit: &str, f: &Shape) -> f64 {
    if part == "body" {
        return if f.on_head() { -4.0 } else { 0.5 };
    }
    if !f.on_head() {
        return -4.0;
    }
    let sex = if fit == "fr1" { 'f' } else if fit == "mr1" { 'm' } else { '?' };
    let (top, low) = (f.root_box[1][2], f.root_box[0][2]);
    match part {
        "hair" => match sex {
            'm' if top >= 0.745 => 0.5,
            'm' if f.root_y < -0.05 => -2.0,
            'm' => -0.3,
            'f' if top > 0.745 => -2.5,
            'f' if top >= 0.66 => 0.5,
            'f' => -0.3,
            _ => 0.0,
        },
        "beard" => {
            if top < 0.78 && f.root_y < -0.04 && low < 0.66 { 0.5 } else { -2.0 }
        }
        _ => {
            // brows, lashes: a short strip on the face
            let strip = f.len_max < 0.04 && f.root_y < -0.05 && f.root_box[1][0] - f.root_box[0][0] < 0.2 && top - low < 0.08;
            match sex {
                _ if !strip => -2.5,
                'f' => if top < 0.67 { 0.6 } else { -2.5 },
                'm' => if top > 0.68 && top < 0.78 { 0.6 } else { -2.5 },
                _ => 0.0,
            }
        }
    }
}

/// An asset that may sit at a placeholder.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub guid: u64,
    /// Ids no asset of an earlier placeholder lists, in table order.
    pub new: Vec<u64>,
    /// Length word (1..=9).
    pub length: Option<u8>,
    /// Side word (`sl`, `sr`, `sm`, `sb`, `sf`).
    pub side: Option<&'static str>,
}

/// One step of the walk: a placeholder and the assets that may be it.
/// `free` marks the step after which any number of pieces may be skipped
/// (the animals between `chops…` and `cs_…`).
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub name: String,
    pub part: &'static str,
    pub fit: String,
    pub cands: Vec<Candidate>,
    pub free: bool,
}

/// Where the walk puts one candidate of a slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub post: f64,
    pub start: usize,
    pub count: usize,
}

fn lae(a: f64, b: f64) -> f64 {
    if a == f64::NEG_INFINITY {
        b
    } else if b == f64::NEG_INFINITY {
        a
    } else {
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        hi + (lo - hi).exp().ln_1p()
    }
}

/// Walk the slots (name order) over the pieces (archive order): for every
/// slot and candidate the most probable run of pieces and its posterior.
/// The walk starts at the first piece and may end anywhere.
pub fn walk(slots: &[Slot], shapes: &[Shape]) -> Vec<Vec<Placed>> {
    let b = shapes.len();
    let n = slots.len();
    // prefix sums: suitability per (part, fit), lean weights
    let mut pre: HashMap<(&str, &str), Vec<f64>> = HashMap::new();
    for s in slots {
        pre.entry((s.part, s.fit.as_str())).or_insert_with(|| {
            let mut v = vec![0.0; b + 1];
            for q in 0..b {
                v[q + 1] = v[q] + suits(s.part, &s.fit, &shapes[q]);
            }
            v
        });
    }
    let (mut w0, mut wx, mut wy) = (vec![0.0f64; b + 1], vec![0.0f64; b + 1], vec![0.0f64; b + 1]);
    for q in 0..b {
        let w = shapes[q].strands as f64 * shapes[q].len_med as f64 + 1e-9;
        w0[q + 1] = w0[q] + w;
        wx[q + 1] = wx[q] + w * shapes[q].lean[0] as f64;
        wy[q + 1] = wy[q] + w * shapes[q].lean[1] as f64;
    }
    let bell = |x: f64, mu: f64, sd: f64| (-0.5 * ((x - mu) / sd).powi(2)).max(-3.0);
    // options of a slot: (candidate, pieces, penalty)
    let options = |s: &Slot| -> Vec<(usize, usize, f64)> {
        let mut o = Vec::new();
        for (ci, c) in s.cands.iter().enumerate() {
            let k = c.new.len();
            if k == 0 {
                o.push((ci, 0, 0.0));
                continue;
            }
            o.extend([(ci, k, 0.0), (ci, k + 1, OFF_BY_ONE), (ci, 0, ABSENT)]);
            if k >= 2 {
                o.push((ci, k - 1, OFF_BY_ONE));
            }
        }
        if s.cands.is_empty() {
            o.push((usize::MAX, 0, 0.0));
        }
        o
    };
    // emission of an option at every start
    let emit = |s: &Slot, ci: usize, k: usize, pen: f64| -> Vec<f64> {
        if k > b {
            return Vec::new();
        }
        let pr = &pre[&(s.part, s.fit.as_str())];
        let c = s.cands.get(ci);
        let len = c.and_then(|c| c.length).and_then(|l| match s.part {
            "hair" => HAIR_LEN.get(l as usize - 1),
            "beard" => BEARD_LEN.get(l as usize - 1),
            _ => None,
        });
        let side = c.and_then(|c| c.side).filter(|_| s.part == "hair");
        (0..=b - k)
            .map(|p| {
                let mut e = pen + pr[p + k] - pr[p];
                if k > 0 {
                    if let Some(&(mu, sd)) = len {
                        let m = shapes[p..p + k].iter().map(|f| f.len_max).fold(1e-3f32, f32::max) as f64;
                        e += bell(m.ln(), mu.ln(), sd);
                    }
                    if let Some(sw) = side {
                        let den = w0[p + k] - w0[p];
                        if let Some(&(_, mu, sd)) = LEAN_X.iter().find(|x| x.0 == sw) {
                            e += bell((wx[p + k] - wx[p]) / den, mu, sd);
                        }
                        if let Some(&(_, mu, sd)) = LEAN_Y.iter().find(|x| x.0 == sw) {
                            e += bell((wy[p + k] - wy[p]) / den, mu, sd);
                        }
                    }
                }
                e
            })
            .collect()
    };
    let em: Vec<Vec<(usize, usize, Vec<f64>)>> = slots.par_iter().map(|s| options(s).into_iter().map(|(ci, k, pen)| (ci, k, emit(s, ci, k, pen))).filter(|x| x.1 <= b).collect()).collect();
    // forward: fw[i][p] = slots[..i] placed, the last ending at p; fg = after gap pieces
    let mut start = vec![f64::NEG_INFINITY; b + 1];
    start[0] = 0.0;
    let mut fw = vec![start];
    let mut fg: Vec<Vec<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut g = fw[i].clone();
        let step = if i > 0 && slots[i - 1].free { 0.0 } else { GAP };
        for p in 1..=b {
            g[p] = lae(g[p], g[p - 1] + step);
        }
        let mut nx = vec![f64::NEG_INFINITY; b + 1];
        for (_, k, e) in &em[i] {
            for p in 0..=b - k {
                nx[p + k] = lae(nx[p + k], g[p] + e[p]);
            }
        }
        fg.push(g);
        fw.push(nx);
    }
    // backward: bw[i][p] = slots[i..] placed from p on (gap pieces first)
    let mut bw = vec![Vec::new(); n + 1];
    bw[n] = vec![0.0f64; b + 1];
    for i in (0..n).rev() {
        let mut cur = vec![f64::NEG_INFINITY; b + 1];
        for (_, k, e) in &em[i] {
            for p in 0..=b - k {
                cur[p] = lae(cur[p], e[p] + bw[i + 1][p + k]);
            }
        }
        // gap pieces before slot i's run
        let step = if i > 0 && slots[i - 1].free { 0.0 } else { GAP };
        for p in (0..b).rev() {
            cur[p] = lae(cur[p], cur[p + 1] + step);
        }
        bw[i] = cur;
    }
    let z = fw[n].iter().fold(f64::NEG_INFINITY, |a, &x| lae(a, x));
    (0..n)
        .map(|i| {
            let nc = slots[i].cands.len();
            let mut best = vec![Placed { post: 0.0, start: 0, count: 0 }; nc];
            for (ci, k, e) in &em[i] {
                if *ci == usize::MAX {
                    continue;
                }
                for p in 0..=b - k {
                    let v = (fg[i][p] + e[p] + bw[i + 1][p + k] - z).exp();
                    if v > best[*ci].post {
                        best[*ci] = Placed { post: v, start: p, count: *k };
                    }
                }
            }
            best
        })
        .collect()
}

/// A field of a decoded meta object: under its name when the name
/// dictionary is loaded, under its hash otherwise.
fn fld<'a>(v: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    v.get(name).or_else(|| v.get(format!("{:08x}", crate::names::joaat_exact(name))))
}

/// Asset registries: GUID -> (type, name-word hashes), from every PSO file
/// with registry records.
fn registry(archive: &Path, all: &[crate::scan::Entry]) -> HashMap<u64, Vec<u32>> {
    let found: Vec<Vec<(u64, Vec<u32>)>> = all
        .par_iter()
        .filter(|e| e.kind == Kind::Pso)
        .filter_map(|e| {
            let b = crate::scan::read_entry(archive, e).ok()?;
            if !b.windows(4).any(|x| x == REGISTRY_RECORD.to_le_bytes() || x == REGISTRY_RECORD.to_be_bytes()) {
                return None;
            }
            let j = crate::pso::parse(&b)?.to_json();
            let mut out = Vec::new();
            for objs in j.get("objects")?.as_object()?.values() {
                for o in objs.as_array().into_iter().flatten() {
                    for rec in o.as_object().into_iter().flat_map(|m| m.values()).filter_map(|v| v.as_array()).flatten() {
                        let (Some(key), Some(item)) = (fld(rec, "Key").and_then(|k| k.as_str()), fld(rec, "Item")) else { continue };
                        if item.get("_struct").and_then(|s| s.as_str()) != Some("20e0f6ed") || item.get("2d4958cf").and_then(|t| t.as_i64()) != Some(TYPE_HAIR) {
                            continue;
                        }
                        let Ok(guid) = u64::from_str_radix(key, 16) else { continue };
                        let words = fld(item, "properties")
                            .and_then(|p| p.as_array())
                            .into_iter()
                            .flatten()
                            .filter(|p| fld(p, "Key").and_then(|k| k.as_u64()) == Some(PROP_WORDS as u64))
                            .flat_map(|p| fld(p, "Item").and_then(|i| i.as_array()).into_iter().flatten())
                            .filter_map(|x| x.as_u64().map(|x| x as u32))
                            .collect();
                        out.push((guid, words));
                    }
                }
            }
            Some(out)
        })
        .collect();
    found.into_iter().flatten().collect()
}

/// Wardrobe items: hair asset GUID (the item's "base" state) -> hashes of
/// the placeholder prefixes its items name.
fn wardrobe_tags(archive: &Path, all: &[crate::scan::Entry]) -> HashMap<u64, BTreeSet<u32>> {
    let found: Vec<Vec<(u64, u32)>> = all
        .par_iter()
        .filter(|e| e.kind == Kind::Resource && e.sub == "7a")
        .filter_map(|e| {
            let r = Resource::load_meta(archive, e).ok()?;
            if !r.sys.windows(4).any(|x| x == TAG_PLACEHOLDER.to_le_bytes()) {
                return None;
            }
            let m = Meta::parse(&r)?;
            let mut out = Vec::new();
            for o in m.objects(ITEM) {
                let Some(it) = m.object(ITEM, o, 4) else { continue };
                let tags: Vec<u32> = it
                    .get("12f85293")
                    .and_then(|t| t.as_array())
                    .into_iter()
                    .flatten()
                    .filter(|t| fld(t, "Key").and_then(|k| k.as_u64()) == Some(TAG_PLACEHOLDER as u64))
                    .flat_map(|t| fld(t, "Item").and_then(|i| i.as_array()).into_iter().flatten())
                    .filter_map(|x| x.as_u64().map(|x| x as u32))
                    .collect();
                if tags.is_empty() {
                    continue;
                }
                for st in fld(&it, "States").and_then(|s| s.as_array()).into_iter().flatten() {
                    if fld(st, "StateName").and_then(|n| n.as_u64()) != Some(STATE_BASE as u64) {
                        continue;
                    }
                    let Some(g) = st.get("e7cb9e1b").and_then(|x| x.get("41acf8eb")).and_then(|g| g.as_str()).and_then(|g| u64::from_str_radix(g, 16).ok()).filter(|&g| g != 0) else { continue };
                    out.extend(tags.iter().map(|&t| (g, t)));
                }
            }
            Some(out)
        })
        .collect();
    let mut out: HashMap<u64, BTreeSet<u32>> = HashMap::new();
    for (g, t) in found.into_iter().flatten() {
        out.entry(g).or_default().insert(t);
    }
    out
}

/// The words of an asset: (own stem, words). None for animal fur and
/// assets without words.
fn asset_words(tags: &[u32], stems: &HashMap<u32, String>, words: &HashMap<u32, String>) -> Option<(String, BTreeSet<String>)> {
    if tags.is_empty() || tags.iter().any(|t| FUR_SLOTS.contains(t)) {
        return None;
    }
    let own: Vec<&String> = tags.iter().filter_map(|t| stems.get(t)).collect();
    if own.iter().any(|s| s.starts_with("cr")) {
        return None;
    }
    let w: BTreeSet<String> = tags.iter().filter(|t| !stems.contains_key(t)).filter_map(|t| words.get(t)).cloned().collect();
    Some((own.first().map(|s| s.to_string()).unwrap_or_default(), w))
}

/// Placeholders an asset may be: those made of the same words (under the
/// asset's own stem when it has one); for a character's asset whose words
/// differ, the placeholders of exactly its stem.
fn candidates<'a>(own: &str, words: &BTreeSet<String>, phs: &'a [Placeholder]) -> Vec<&'a Placeholder> {
    if own.is_empty() {
        return phs.iter().filter(|p| !p.own && p.words == *words).collect();
    }
    let pre = format!("{own}_");
    let under: Vec<&Placeholder> = phs.iter().filter(|p| p.stem == own || p.stem.starts_with(&pre)).collect();
    let exact: Vec<&Placeholder> = under.iter().copied().filter(|p| p.words == *words).collect();
    if !exact.is_empty() {
        exact
    } else if under.iter().all(|p| p.stem == own) {
        under
    } else {
        Vec::new()
    }
}

/// The game folder of an archive: its path without its id.
fn game_root(a: &crate::game::ArchiveInfo) -> std::path::PathBuf {
    let mut p = a.path.clone();
    for _ in Path::new(&a.id).components() {
        p.pop();
    }
    p
}

/// Piece words of groom files: a piece is `<placeholder without _full>_<word>`,
/// the word also with a number (`WORD_NUMBERS`: `base2`, `top_02`). Read
/// off the piece files the table of contents names, and for fur from a
/// search over two-word compounds (`leg_front`, `earsflyaways`).
const PIECE_WORDS: &[&str] = &[
    "accessory", "accs", "arms", "baby", "babyhair", "babyhair_back", "babyhair_front", "babyhairback", "back", "back_base", "back_crown", "back_filler",
    "backbase", "backlegflyaways", "bangs", "base", "base_babyhair", "base_back", "base_bottom", "base_braids", "base_clumps", "base_filler",
    "base_fillerfront", "base_flyaways", "base_fringe", "base_front", "base_full", "base_hairline", "base_nape", "base_pony", "base_ponytail", "base_top",
    "baseback", "basebottom", "basecap", "baseclumps", "basefill", "basefiller", "baseflyaways", "basefront", "basehair", "baselower", "basetop", "beard",
    "beard_hairline", "beardbase", "beardline", "beardstache", "belly", "bellyflyaway", "bellyflyaways", "body", "body_front", "body_lower", "body_rear",
    "bodyback", "bodybacklower", "bodyclump", "bodyclumps", "bodyfiller", "bodyflyaways", "bodyfront", "bodyfrontlower", "bodyfuzz", "bodymiddle",
    "bodypom", "bodyupper", "bot", "bottom", "bottom_base", "bottom_braids", "bottom_clumps", "bottom_filler", "bottom_hairline", "bottom_transition",
    "bottombase", "braid", "braid_base", "braid_filler", "braidbase", "braidbun", "braidbunloose", "braidleft", "braidloose", "braidright", "braidroots",
    "braids", "braids_bottom", "braids_filler", "braids_fringe", "braids_loose", "braids_tie", "braids_top", "braidsback", "braidsbase",
    "braidsbaseshort", "braidsbody", "braidsbun", "braidsfront", "braidshort", "braidsides", "braidsloose", "braidsshort", "braidtip", "brow", "brows",
    "btm", "bun", "bun_braid", "bun_clumps", "bun_flyaways", "bun_loose", "bunbase", "buncurls", "bunfiller", "bunloose", "buntips", "buzz", "buzz_base",
    "buzz_cap", "buzz_filler", "buzzfiller", "cap", "cap_front", "chest", "chestpom", "chin", "chop", "clump", "clumps", "clumps_right", "crown", "curls",
    "curls_base", "curly", "curly_base", "detail", "ear", "ear_flyaways", "earflyaway", "earflyaways", "ears", "earsflyaway", "earsflyaways", "earspom",
    "eyeflyaways", "eyelashes", "faceflyaways", "fade", "fade_filler", "fade_top", "fadelower", "fadetop", "feet", "feetflyaways", "fill", "filler",
    "filler_back", "filler_braids", "filler_cap", "filler_front", "filler_pony", "fillerfront", "flyaway", "flyawaybody", "flyaways", "flyaways_top",
    "flyawaytail", "fringe", "fringe_braids", "fringe_filler", "fringe_hairline", "fringefiller", "front", "front_clumps", "front_filler", "full",
    "full_beard", "fullbody", "fuzz", "hairline", "hairtie", "hands", "head", "head_clump", "head_ears", "head_flyaways", "head_long", "head_neck",
    "head_short", "head_top", "head_whiskers", "headclump", "headclumps", "headfiller", "headflyaway", "headflyaways", "headflyawayshairline", "headlong",
    "headneck", "headpom", "headshort", "heart", "hooves", "left", "leg", "leg_back", "leg_front", "legback", "legflyaways", "legfront", "legs",
    "legs_back", "legs_front", "legsback", "legsbacklower", "legsflyaways", "legsfront", "legsfrontlower", "legshort", "legsmoss", "line", "long",
    "loose", "loosefront", "loosetop", "lower", "lowerbodyflyaways", "lowerflyaways", "main", "mainleft", "mane", "mid", "middle", "moss", "moustache",
    "mustache", "mustache_base", "nape", "nape_filler", "neck", "neck_head", "neckfiller", "neckflyaways", "necklower", "paws", "peach", "pony",
    "pony_base", "pony_flyaways", "pony_tie", "ponybase", "ponytail", "ponytail_base", "ponytail_braids", "ponytail_clumps", "ponytail_filler",
    "ponytail_flyaways", "ponytail_tie", "ponytailbase", "ponytailfront", "ponytailtie", "ponytie", "rear", "rearlegflyaways", "rearpom", "right",
    "roots", "shoe", "short", "short_base", "shoulder", "side", "sideburn", "sideburns", "sidecurls", "sidehair", "sides", "strands", "strip", "tail",
    "tailflyaway", "tailflyaways", "tailheart", "tailpom", "tie", "top", "top_back", "top_base", "top_braids", "top_clumps", "top_filler", "top_flyaways",
    "top_front", "top_full", "topbase", "topclumps", "topfill", "topfiller", "topflyaways", "tophair", "transition", "tube", "upper", "whisker",
    "whiskers"
];
const WORD_NUMBERS: [&str; 7] = ["", "1", "2", "3", "4", "_2", "_02"];

/// Name the listed piece ids (`ids`) that are `<base>_<word>` for one of
/// the placeholder bases (names without `_full`): id -> piece name.
pub fn piece_names<'a>(bases: impl IntoIterator<Item = &'a str>, ids: &HashSet<u64>) -> HashMap<u64, String> {
    let mut out = HashMap::new();
    for b in bases {
        for w in PIECE_WORDS {
            for n in WORD_NUMBERS {
                let name = format!("{b}_{w}{n}");
                let id = crate::meta7a::asset_id(&name);
                if ids.contains(&id) {
                    out.entry(id).or_insert(name);
                }
            }
        }
    }
    out
}

/// The LOD chain of a named piece: its densest groom `start`, then the
/// grooms `<name>_lod1`, `_lod2` … by name id (`file`), up to the first
/// that is none or is another piece's start.
pub fn lod_files(name: &str, start: u64, file: impl Fn(&u64) -> Option<u64>, starts: &HashSet<u64>) -> Vec<u64> {
    let mut c = vec![start];
    for k in 1..=9 {
        match file(&crate::meta7a::asset_id(&format!("{name}_lod{k}"))) {
            Some(o) if !c.contains(&o) && !starts.contains(&o) => c.push(o),
            _ => break,
        }
    }
    c
}

/// Group the catalog's grooms (archive order) into pieces when the densest
/// groom of every listed piece is known (`starts`): densest groom -> its
/// LOD chain. A piece whose chain was read by name (`known`, densest ->
/// chain) keeps it, and its LODs join no other piece. Any other groom that
/// is no start joins one of the last three other pieces by
/// `furlink::pieces`' rule (same bones, fewer strands than the piece's
/// last LOD, and for a piece further back the same root box) or stands
/// alone.
pub fn chains(cat: &[GroomInfo], starts: &HashSet<u64>, known: &HashMap<u64, Vec<u64>>) -> HashMap<u64, Vec<u64>> {
    let close = |a: &[[f32; 3]; 2], b: &[[f32; 3]; 2]| (0..3).all(|k| (a[0][k] - b[0][k]).abs() < 0.002 && (a[1][k] - b[1][k]).abs() < 0.002);
    let read: HashSet<u64> = known.values().flatten().copied().collect();
    let mut done: HashMap<u64, Vec<u64>> = HashMap::new();
    // (LODs, index of the densest in `cat`, strands of the last LOD)
    let mut out: Vec<(Vec<u64>, usize, u32)> = Vec::new();
    for (i, g) in cat.iter().enumerate() {
        if let Some(c) = known.get(&g.off) {
            done.insert(g.off, c.clone());
            continue;
        }
        if read.contains(&g.off) {
            continue;
        }
        let n = out.len();
        let hit = if starts.contains(&g.off) {
            None
        } else {
            (1..=3.min(n)).find(|&back| {
                let (_, first, last) = &out[n - back];
                cat[*first].bones == g.bones && g.strands < *last && (back == 1 || close(&cat[*first].root_box, &g.root_box))
            })
        };
        match hit {
            Some(back) => {
                let e = &mut out[n - back];
                e.0.push(g.off);
                e.2 = g.strands;
            }
            None => out.push((vec![g.off], i, g.strands)),
        }
    }
    out.into_iter().map(|e| (e.0[0], e.0)).chain(done).collect()
}

/// Build the table: exactly when the table of contents can be read,
/// inferred otherwise.
pub fn build(work: &Path, a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> Result<HairLinks> {
    match build_exact(work, a, all) {
        Ok(t) => Ok(t),
        Err(e) => {
            let mut t = build_inferred(work, a, all)?;
            t.note = format!("{} The stored link could not be read: {e:#}.", t.note);
            Ok(t)
        }
    }
}

/// The stored link: placeholder name -> GUID, piece table, file ids of the
/// table of contents.
pub fn build_exact(work: &Path, a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> Result<HairLinks> {
    let cat: Vec<GroomInfo> = crate::groom::catalog_cached(work, a, all)?;
    if cat.is_empty() {
        return Err(anyhow!("no grooms in {}", a.id));
    }
    let table = crate::furlink::piece_table(&a.path, all);
    if table.is_empty() {
        return Err(anyhow!("groom piece table not found in {}", a.id));
    }
    let mut cipher = crate::gamecipher::open(&game_root(a))?;
    let mut f = std::fs::File::open(&a.path)?;
    let ids = crate::rpf9toc::file_ids(cipher.as_mut(), &mut f)?;
    let grooms: HashSet<u64> = cat.iter().map(|g| g.off).collect();
    let file = |id: &u64| ids.get(id).into_iter().flatten().copied().find(|o| grooms.contains(o));
    let starts: HashSet<u64> = table.values().flatten().filter_map(file).collect();
    if starts.is_empty() {
        return Err(anyhow!("no groom of {} has an id of the piece table", a.id));
    }
    let names: HashMap<u64, String> = all.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| e.name.as_deref()).map(|n| n.to_ascii_lowercase()).map(|n| (crate::meta7a::asset_id(&n), n)).collect();
    let listed: HashSet<u64> = table.values().flatten().copied().collect();
    let pieces = piece_names(table.keys().filter_map(|g| names.get(g)).map(|n| n.trim_end_matches("_full")), &listed);
    let known: HashMap<u64, Vec<u64>> = pieces.iter().filter_map(|(id, n)| file(id).map(|s| (s, lod_files(n, s, file, &starts)))).collect();
    let lods = chains(&cat, &starts, &known);
    let mut every: Vec<Vec<u64>> = lods.values().cloned().collect();
    every.sort_unstable_by_key(|c| c[0]);
    let mut links: BTreeMap<String, Vec<Link>> = BTreeMap::new();
    for (guid, list) in &table {
        let name = names.get(guid).map(|s| s.as_str()).unwrap_or("");
        // animals: their fur is `furlink`'s
        if name.starts_with("cr") {
            continue;
        }
        let mut own: Vec<u64> = list.iter().filter_map(file).collect();
        own.sort_unstable();
        own.dedup();
        if own.is_empty() {
            continue;
        }
        let ph = placeholder(name);
        links.insert(
            format!("{guid:016x}"),
            vec![Link {
                placeholder: name.to_string(),
                fit: ph.as_ref().map(|p| p.fit.clone()).or_else(|| name.split('_').find(|t| is_fit(t)).map(|t| t.to_string())).unwrap_or_default(),
                part: match &ph {
                    Some(p) => p.part,
                    // a name that is no hair placeholder: strands on a hat, a mask, a costume
                    None if !name.is_empty() && !["hair", "beard", "mustache", "brows", "lashes"].iter().any(|k| name.contains(k)) => "other",
                    None => part_of(&BTreeSet::new(), name),
                }
                .to_string(),
                confidence: 1.0,
                chains: own.iter().map(|o| lods.get(o).cloned().unwrap_or_else(|| vec![*o])).collect(),
                shared: Vec::new(),
                listed: list.len(),
                missing: list.len() - list.iter().filter(|id| file(id).is_some()).count(),
                rivals: 0,
                grooms: own,
            }],
        );
    }
    Ok(HairLinks {
        version: CACHE_VERSION,
        note: "Read from the game: an asset's GUID is the name id of its placeholder, the piece table lists its pieces by the name ids of their files, and the table of contents gives each file its id. missing = listed pieces that are no groom file of the archive. A piece's LODs are read by name (`<piece>_lodN`) when its piece word is known, else attached by bones and strand counts.".into(),
        exact: true,
        links,
        lods: every,
    })
}

/// The inferred table: registries, wardrobe items, piece table, groom
/// shapes, then the walk.
pub fn build_inferred(work: &Path, a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> Result<HairLinks> {
    let cat: Vec<GroomInfo> = crate::groom::catalog_cached(work, a, all)?;
    let pcs = crate::furlink::pieces(&cat);
    if pcs.is_empty() {
        return Err(anyhow!("no grooms in {}", a.id));
    }
    let table = crate::furlink::piece_table(&a.path, all);
    if table.is_empty() {
        return Err(anyhow!("groom piece table not found in {}", a.id));
    }
    // names: placeholders, word and stem dictionaries
    let names: BTreeSet<String> = all.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| e.name.as_deref()).map(|n| n.to_ascii_lowercase()).collect();
    let phs: Vec<Placeholder> = names.iter().filter_map(|n| placeholder(n)).collect();
    let mut words: HashMap<u32, String> = HashMap::new();
    let mut stems: HashMap<u32, String> = HashMap::new();
    for n in all.iter().filter_map(|e| e.name.as_deref()) {
        let n = n.to_ascii_lowercase();
        let t: Vec<&str> = n.split('_').collect();
        for (i, w) in t.iter().enumerate() {
            words.entry(joaat(w)).or_insert_with(|| w.to_string());
            if i >= 1 {
                let s = t[..=i].join("_");
                stems.entry(joaat(&s)).or_insert(s);
            }
        }
    }
    for p in &phs {
        for w in &p.words {
            words.entry(joaat(w)).or_insert_with(|| w.clone());
        }
    }
    // assets and the placeholders each may be
    let reg = registry(&a.path, all);
    if reg.is_empty() {
        return Err(anyhow!("no hair assets in the registries of {}", a.id));
    }
    let ward = wardrobe_tags(&a.path, all);
    let index: HashMap<&str, usize> = phs.iter().enumerate().map(|(i, p)| (p.name.as_str(), i)).collect();
    let mut at: BTreeMap<u64, (Vec<usize>, BTreeSet<String>)> = BTreeMap::new();
    for (guid, tags) in &reg {
        let Some((own, w)) = asset_words(tags, &stems, &words) else { continue };
        let mut c: Vec<usize> = candidates(&own, &w, &phs).iter().map(|p| index[p.name.as_str()]).collect();
        // a wardrobe item naming exactly one of them settles it
        if let Some(t) = ward.get(guid) {
            let named: Vec<usize> = c.iter().copied().filter(|&i| t.iter().any(|h| *h == joaat(&phs[i].name) || *h == joaat(phs[i].name.trim_end_matches("_full")))).collect();
            if named.len() == 1 {
                c = named;
            }
        }
        if !c.is_empty() {
            at.insert(*guid, (c, w));
        }
    }
    if std::env::var_os("SIXNG_DEBUG_HAIR").is_some() {
        let worded = reg.iter().filter(|(_, t)| asset_words(t, &stems, &words).is_some()).count();
        eprintln!("[hair] {} placeholders, {} hair assets ({} human with words, {} matched to a placeholder), {} assets in wardrobe items, {} table assets, {} pieces", phs.len(), reg.len(), worded, at.len(), ward.len(), table.len(), pcs.len());
        if let Some((g, t)) = reg.iter().find(|(_, t)| t.len() > 6) {
            eprintln!("[hair] e.g. {g:016x}: {:?} -> {:?}", t.iter().map(|x| format!("{x:08x}")).collect::<Vec<_>>(), asset_words(t, &stems, &words));
        }
    }
    // an id belongs to the asset with the earliest placeholder that lists it
    let first: HashMap<u64, usize> = at.iter().map(|(g, (c, _))| (*g, *c.iter().min().unwrap())).collect();
    let mut owner: HashMap<u64, usize> = HashMap::new();
    for (g, &m) in &first {
        for id in table.get(g).into_iter().flatten() {
            let e = owner.entry(*id).or_insert(m);
            *e = (*e).min(m);
        }
    }
    let mut slots: Vec<Slot> = phs.iter().map(|p| Slot { name: p.name.clone(), part: p.part, fit: p.fit.clone(), cands: Vec::new(), free: false }).collect();
    for (g, (c, w)) in &at {
        let new: Vec<u64> = table.get(g).into_iter().flatten().copied().filter(|id| owner[id] == first[g]).collect();
        let length = w.iter().find_map(|t| t.strip_prefix("l0").and_then(|d| d.parse::<u8>().ok()).filter(|d| (1..=9).contains(d) && t.len() == 3));
        let side = LEAN_X.iter().map(|x| x.0).find(|s| w.contains(*s));
        for &i in c {
            slots[i].cands.push(Candidate { guid: *g, new: new.clone(), length, side });
        }
    }
    // the animals lie between the last name before "cr" and the first after
    if let Some(i) = phs.iter().rposition(|p| p.name.as_str() < "cr") {
        slots[i].free = true;
    }
    // shapes of the pieces
    let by_off: HashMap<u64, &crate::scan::Entry> = all.iter().map(|e| (e.off, e)).collect();
    let shapes: Vec<Shape> = pcs
        .par_iter()
        .map(|p| {
            by_off
                .get(&p.lods[0])
                .and_then(|e| Resource::load(&a.path, e).ok())
                .and_then(|r| crate::groom::parse(&r).ok())
                .map(|g| Shape::of(&g, p.root_box))
                .unwrap_or(Shape { root_box: p.root_box, strands: p.strands, ..Default::default() })
        })
        .collect();
    let placed = walk(&slots, &shapes);
    if let Ok(pat) = std::env::var("SIXNG_DEBUG_HAIR") {
        for g in pat.split(',').filter_map(|x| u64::from_str_radix(x, 16).ok()) {
            let hit = reg.iter().find(|(k, _)| **k == g || **k >> 32 == g);
            eprintln!("[hair] asset {g:x}: reg {:?} words {:?} at {:?} table {:?}", hit.map(|(k, t)| (format!("{k:016x}"), t.iter().map(|x| format!("{x:08x}")).collect::<Vec<_>>())), hit.and_then(|(_, t)| asset_words(t, &stems, &words)), hit.and_then(|(k, _)| at.get(k)).map(|(c, _)| c.iter().map(|&i| phs[i].name.clone()).collect::<Vec<_>>()), hit.and_then(|(k, _)| table.get(k)).map(|v| v.len()));
        }
        let distinct: BTreeSet<u64> = table.values().flatten().copied().collect();
        let hairids: BTreeSet<u64> = reg.iter().filter_map(|(k, _)| table.get(k)).flatten().copied().collect();
        let atids: BTreeSet<u64> = at.keys().filter_map(|k| table.get(k)).flatten().copied().collect();
        eprintln!("[hair] ids: {} distinct in table, {} of hair assets, {} of matched assets; pieces {}", distinct.len(), hairids.len(), atids.len(), pcs.len());
        for g in pat.split(',').filter_map(|x| u64::from_str_radix(x, 16).ok()) {
            if let Some((k, _)) = reg.iter().find(|(k, _)| **k >> 32 == g) {
                eprintln!("[hair] ids {k:016x}: {:x?}", table.get(k));
            }
        }
        let _ = std::fs::write("hair_reg.json", serde_json::to_vec(&reg.iter().map(|(k, t)| (format!("{k:016x}"), t.clone())).collect::<BTreeMap<_, _>>()).unwrap());
        let key = pat.split(',').next().unwrap_or("");
        for (i, (s, pl)) in slots.iter().zip(&placed).enumerate() {
            if key.is_empty() || !s.name.contains(key) {
                continue;
            }
            eprintln!("[hair] slot {i} {}", s.name);
            for (c, p) in s.cands.iter().zip(pl) {
                let ids = table.get(&c.guid).map(|v| v.as_slice()).unwrap_or(&[]);
                let other: Vec<String> = ids.iter().filter(|id| !c.new.contains(id)).map(|id| phs[owner[id]].name.clone()).collect::<BTreeSet<_>>().into_iter().collect();
                eprintln!("[hair]   {:016x} ids {} new {} post {:.3} start {} count {} off {:x?} others {:?}", c.guid, ids.len(), c.new.len(), p.post, p.start, p.count, pcs.get(p.start).map(|q| q.lods[0]), other);
            }
        }
        if let Ok(r) = std::env::var("SIXNG_DEBUG_HAIR_RANGE") {
            let v: Vec<usize> = r.split(',').filter_map(|x| x.parse().ok()).collect();
            for q in v[0]..v[1].min(pcs.len()) {
                let sh = &shapes[q];
                eprintln!("[hair] piece {q} off {:x} strands {} len {:.3}/{:.3} top {:.3} y {:.3} lean {:.2?}", pcs[q].lods[0], sh.strands, sh.len_med, sh.len_max, sh.root_box[1][2], sh.root_y, sh.lean);
            }
        }
    }
    // ids placed with their owner
    let mut id_piece: HashMap<u64, usize> = HashMap::new();
    for (s, pl) in slots.iter().zip(&placed) {
        let Some((c, p)) = s.cands.iter().zip(pl).max_by(|x, y| x.1.post.total_cmp(&y.1.post)) else { continue };
        if p.post >= 0.5 && p.count == c.new.len() {
            for (j, id) in c.new.iter().enumerate() {
                id_piece.entry(*id).or_insert(p.start + j);
            }
        }
    }
    let mut links: BTreeMap<String, Vec<Link>> = BTreeMap::new();
    for (s, pl) in slots.iter().zip(&placed) {
        for (c, p) in s.cands.iter().zip(pl) {
            let run = p.start..p.start + p.count;
            let ids = table.get(&c.guid).map(|v| v.as_slice()).unwrap_or(&[]);
            let shared: Vec<u64> = ids.iter().filter(|id| !c.new.contains(id)).filter_map(|id| id_piece.get(id)).filter(|q| !run.contains(q)).map(|&q| pcs[q].lods[0]).collect();
            let missing = (ids.len()).saturating_sub(p.count + shared.len());
            links.entry(format!("{:016x}", c.guid)).or_default().push(Link {
                placeholder: s.name.clone(),
                fit: s.fit.clone(),
                part: s.part.to_string(),
                confidence: p.post as f32,
                grooms: run.clone().map(|q| pcs[q].lods[0]).collect(),
                chains: run.map(|q| pcs[q].lods.clone()).collect(),
                shared,
                listed: c.new.len(),
                missing,
                rivals: s.cands.len() - 1,
            });
        }
    }
    for v in links.values_mut() {
        v.sort_by(|x, y| y.confidence.total_cmp(&x.confidence));
    }
    Ok(HairLinks {
        version: CACHE_VERSION,
        exact: false,
        lods: Vec::new(),
        note: "Inferred from name words, archive order, piece counts and geometry; not the stored link. confidence = model posterior of this asset on exactly these pieces. Under 0.5 treat the grooms as a guess; rivals > 0 = assets of the same words that could not be told apart.".into(),
        links,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let p = placeholder("cs_andrea_hairlob_fr1_000_wavy_l08_non_sf_n_full").unwrap();
        assert_eq!((p.stem.as_str(), p.fit.as_str(), p.own, p.part), ("cs_andrea_hairlob", "fr1", true, "hair"));
        assert_eq!(p.words.iter().map(|s| s.as_str()).collect::<Vec<_>>(), ["hair", "l08", "lob", "n", "non", "sf", "wavy"]);
        let p = placeholder("beardgoatee_mr1_001_straight_l02_n_full").unwrap();
        assert_eq!((p.own, p.part), (false, "beard"));
        assert!(p.words.contains("goatee") && p.words.contains("beard"));
        assert_eq!(placeholder("armpithairfull_mr1_003_curly_regular_full").unwrap().part, "body");
        assert!(placeholder("crm_cat_feral_m_00_fur_body_short_00").is_none());
        assert!(placeholder("head_fr1_lat_018_m_full").is_none());
    }

    fn shape(top: f32, y: f32, len: f32) -> Shape {
        Shape { root_box: [[-0.08, -0.1, top - 0.15], [0.08, 0.08, top]], root_y: y, len_max: len, len_med: len / 2.0, strands: 100, lean: [0.0, 0.0] }
    }

    #[test]
    fn chains_by_starts() {
        let g = |off: u64, strands: u32, bones: &[&str]| GroomInfo { off, id: String::new(), strands, root_triangles: 0, bones: bones.iter().map(|b| b.to_string()).collect(), root_box: [[0.0; 3], [0.1; 3]], category: String::new(), chain: 0, lod: 0 };
        // x, x_fluff (fewer strands, same bones: a piece of its own), x_fluff_lod1, x_lod1, y
        let cat = [g(1, 100, &["a"]), g(2, 60, &["a"]), g(3, 30, &["a"]), g(4, 50, &["a"]), g(5, 40, &["b"])];
        let none = HashMap::new();
        let c = chains(&cat, &HashSet::from([1, 2, 5]), &none);
        assert_eq!(c[&1], [1, 4]);
        assert_eq!(c[&2], [2, 3]);
        assert_eq!(c[&5], [5]);
        // without the ids x_fluff reads as a LOD of x
        assert_eq!(chains(&cat, &HashSet::new(), &none)[&1], [1, 2, 3]);
        // x's LODs read by name: 3 and 4, though 3 follows x_fluff; x_fluff keeps none
        let c = chains(&cat, &HashSet::from([1, 2, 5]), &HashMap::from([(1, vec![1, 3, 4])]));
        assert_eq!(c[&1], [1, 3, 4]);
        assert_eq!(c[&2], [2]);
        assert_eq!(c[&5], [5]);
    }

    #[test]
    fn pieces_and_lods_by_name() {
        let base = "hairlob_fr1_018_straight_l08_non_sm_n";
        let id = |n: &str| crate::meta7a::asset_id(n);
        let ids = HashSet::from([id(&format!("{base}_base")), id(&format!("{base}_top_02")), 7]);
        let p = piece_names([base], &ids);
        assert_eq!(p.len(), 2);
        assert_eq!(p[&id(&format!("{base}_top_02"))], format!("{base}_top_02"));
        // files by id: the piece 100, lod1 101, lod2 102; lod3 is another piece's start
        let name = format!("{base}_base");
        let files: HashMap<u64, u64> = HashMap::from([(id(&name), 100), (id(&format!("{name}_lod1")), 101), (id(&format!("{name}_lod2")), 102), (id(&format!("{name}_lod3")), 200)]);
        let file = |i: &u64| files.get(i).copied();
        assert_eq!(lod_files(&name, 100, file, &HashSet::from([100, 200])), [100, 101, 102]);
    }

    #[test]
    fn relabel_by_lods() {
        let g = |off: u64| GroomInfo { off, id: String::new(), strands: 1, root_triangles: 0, bones: Vec::new(), root_box: [[0.0; 3], [0.1; 3]], category: String::new(), chain: 9, lod: 9 };
        let mut cat = [g(1), g(2), g(3), g(4)];
        let t = HairLinks { lods: vec![vec![1, 3], vec![2]], ..Default::default() };
        t.relabel(&mut cat);
        assert_eq!(cat.iter().map(|g| (g.chain, g.lod)).collect::<Vec<_>>(), [(1, 0), (2, 0), (1, 1), (9, 9)]);
    }

    #[test]
    fn walk_by_fit_and_length() {
        // a woman's long style (3 pieces), a man's beard (1), a man's short
        // style (2); a piece of no asset in front
        let cand = |g: u64, n: usize, l: u8| Candidate { guid: g, new: (0..n as u64).map(|i| g * 100 + i).collect(), length: Some(l), side: None };
        let slot = |name: &str, part: &'static str, fit: &str, c: Vec<Candidate>| Slot { name: name.into(), part, fit: fit.into(), cands: c, free: false };
        let slots = [slot("a", "hair", "fr1", vec![cand(1, 3, 8)]), slot("b", "beard", "mr1", vec![cand(2, 1, 2)]), slot("c", "hair", "mr1", vec![cand(3, 2, 3)])];
        let body = Shape { root_box: [[-0.2, 0.0, 0.2], [0.2, 0.1, 0.3]], ..shape(0.3, 0.0, 0.02) };
        let beard = Shape { root_box: [[-0.07, -0.16, 0.54], [0.07, -0.04, 0.66]], ..shape(0.66, -0.1, 0.02) };
        let shapes = [body, shape(0.71, 0.0, 0.4), shape(0.71, 0.0, 0.1), shape(0.70, 0.0, 0.3), beard, shape(0.82, 0.0, 0.1), shape(0.81, 0.0, 0.05)];
        let p = walk(&slots, &shapes);
        assert_eq!((p[0][0].start, p[0][0].count), (1, 3));
        assert_eq!((p[1][0].start, p[1][0].count), (4, 1));
        assert_eq!((p[2][0].start, p[2][0].count), (5, 2));
        assert!(p.iter().all(|x| x[0].post > 0.8), "{p:?}");
    }
}
