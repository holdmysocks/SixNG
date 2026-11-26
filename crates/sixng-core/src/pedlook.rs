//! Ped looks: the outfits of a character model, from the wardrobe data.
//!
//! A named character owns only a few drawables (`cs_ali`: head, eyes, a hair
//! placeholder, a piercing). What it wears is data:
//!
//! * One packs.rpf PSO (struct `a5ea6310`) maps joaat(model name) to the
//!   model's outfits, in order: variation id, outfit type, body scale. 2,717
//!   models are listed (characters and animals).
//! * Each model has one wardrobe meta resource (`wardrobe`) with those
//!   variations. An outfit variation lists its garments and includes other
//!   variations by id: the head set (head, eyes, teeth), the body set (the
//!   skin meshes `hand_*` = upper body and `feet_*` = lower body), brows and
//!   lashes, and a body-shape preset.
//!
//! `look` flattens that into parts per outfit. The first outfit of the list
//! is used as the default look (nothing marks a default; it is the first one
//! the game lists).
//!
//! Which drawable an item uses: the one its asset GUID names (the GUID is
//! the name id, `meta7a::asset_id`, of the item's drawable with `_full`;
//! not for heads, whose GUID names the base head template); else the name
//! the item's tags give (heads, hair, brows), else the drawable that shares
//! most of the item's textures, where ties go to the model's own fit
//! (`fr1`) and to `_full` meshes. Items found by none of these get no part.
//!
//! The model table is built once from the packs.rpf index and cached as
//! `<work>/index/ped_models.json`.
use crate::game::ArchiveInfo;
use crate::meta7a::{joaat, Meta};
use crate::resource::Resource;
use crate::scan::ArchiveIndex;
use crate::wardrobe::{Part, PartTint, Variation};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const CACHE_VERSION: u32 = 5;
const VARIATION: u32 = 0xa9d5_7751;
const LIST_ITEM: u32 = 0xa5ea_6310;

/// One model of the outfit list: its wardrobe resource and its outfits in
/// the stored order (variation id, outfit type, body scale).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRow {
    pub resource: u64,
    pub outfits: Vec<(u32, u32, f32)>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Models {
    version: u32,
    /// joaat(model name) -> row
    pub by_model: BTreeMap<u32, ModelRow>,
    /// joaat(drawable name or name prefix) -> wardrobe resources with an
    /// item that names that head (tag `4bfd27dd`) or that strand-hair
    /// placeholder (tag `023e5a2f`, the name without `_full`). Many characters are listed under
    /// a model name that is not their drawable prefix (`cs_lucia_*`); their
    /// head finds their wardrobe.
    #[serde(default)]
    pub heads: BTreeMap<u32, Vec<u64>>,
    /// Character name of the ped model table (a packs PSO, record struct
    /// `eb66d086`, field `dc3dfe39`) -> the models (`Name`) that carry it.
    /// `player_story000_00` carries joaat("lucia"), `player_story000_01`
    /// joaat("jason"); for 47 of 114 `cs_` models it is the model name
    /// without `cs_`.
    #[serde(default)]
    pub aliases: BTreeMap<u32, Vec<u32>>,
    /// Name id (`meta7a::asset_id`) of a drawable of a character prefix the
    /// list does not have (`cs_roxy_jacketfur_…`) -> the wardrobe resources
    /// with an item naming it. Such a prefix can be a clothing set that
    /// another model wears (`cs_nastydimez_roxyrashad`).
    #[serde(default)]
    pub parts: BTreeMap<u64, Vec<u64>>,
}

fn build(packs: &Path, ix: &ArchiveIndex) -> Models {
    use crate::classify::Kind;
    // outfit lists, in order
    let mut lists: BTreeMap<u32, Vec<(u32, u32, f32)>> = BTreeMap::new();
    let mut aliases: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for e in ix.entries.iter().filter(|e| e.kind == Kind::Pso) {
        let Ok(b) = crate::scan::read_entry(packs, e) else { continue };
        // the ped model table: character name -> models
        const MODEL_INFO: u32 = 0xeb66_d086;
        if b.windows(4).any(|w| w == MODEL_INFO.to_le_bytes() || w == MODEL_INFO.to_be_bytes()) {
            if let Some(p) = crate::pso::parse(&b) {
                let j = p.to_json();
                let f = crate::meta7a::field_name;
                for (k, objs) in j["objects"].as_object().into_iter().flatten() {
                    if !k.ends_with("eb66d086") {
                        continue;
                    }
                    for o in objs.as_array().into_iter().flatten() {
                        if let (Some(name), Some(alias)) = (o["Name"].as_u64(), o[f(0xdc3d_fe39)].as_u64().filter(|&x| x != 0)) {
                            let v = aliases.entry(alias as u32).or_default();
                            if !v.contains(&(name as u32)) {
                                v.push(name as u32);
                            }
                        }
                    }
                }
            }
        }
        if !b.windows(4).any(|w| w == LIST_ITEM.to_le_bytes() || w == LIST_ITEM.to_be_bytes()) {
            continue;
        }
        let Some(p) = crate::pso::parse(&b) else { continue };
        let j = p.to_json();
        let f = crate::meta7a::field_name;
        for (k, objs) in j["objects"].as_object().into_iter().flatten() {
            if !k.starts_with("78262a67") {
                continue;
            }
            for o in objs.as_array().into_iter().flatten() {
                let Some(model) = o["Key"].as_u64() else { continue };
                let v: Vec<(u32, u32, f32)> = o["Item"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|it| Some((it[f(0xfc7e_aa76)].as_u64()? as u32, it[f(0xfee1_82a6)].as_u64()? as u32, it[f(0x3cba_3fe6)].as_f64().unwrap_or(0.0) as f32)))
                    .collect();
                if !v.is_empty() {
                    lists.insert(model as u32, v);
                }
            }
        }
    }
    // the drawables of character prefixes the list does not have, by name id
    let targets: HashSet<u64> = ix
        .entries
        .iter()
        .filter(|e| e.rtype.as_deref() == Some("dr"))
        .filter_map(|e| e.name.as_ref().map(|n| n.to_ascii_lowercase()))
        .filter(|n| crate::outfit::outfit_prefix(n).is_some_and(|p| !lists.contains_key(&joaat(&p))))
        .map(|n| crate::meta7a::asset_id(&n))
        .collect();
    let parts_found: std::sync::Mutex<Vec<(u64, u64)>> = Default::default();
    // variation id -> wardrobe resources that define it (false), and head
    // drawable hash -> resources whose items name it (true)
    let found: Vec<(bool, u32, u64)> = ix
        .entries
        .par_iter()
        .filter(|e| e.sub == "7a")
        .flat_map_iter(|e| {
            let mut v = Vec::new();
            let Ok(r) = Resource::load_meta(packs, e) else { return v };
            let Some(m) = Meta::parse(&r) else { return v };
            let f = crate::meta7a::field_name;
            let k = f(0x6512_1749);
            let vars = m.objects(VARIATION);
            if !vars.is_empty() && !targets.is_empty() {
                // item names: the 64-bit name ids anywhere in the resource
                let mut hit: Vec<(u64, u64)> = r.sys.chunks_exact(8).chain(r.gfx.chunks_exact(8)).map(|c| u64::from_le_bytes(c.try_into().unwrap())).filter(|h| targets.contains(h)).map(|h| (h, e.off)).collect();
                hit.sort();
                hit.dedup();
                if !hit.is_empty() {
                    parts_found.lock().unwrap().extend(hit);
                }
            }
            for &o in &vars {
                if let Some(id) = m.scalars(VARIATION, o, &[]).get(&k).and_then(|x| x.as_u64()) {
                    v.push((false, id as u32, e.off));
                }
            }
            if !vars.is_empty() {
                for o in m.objects(0xf795_e8ba) {
                    let Some(p) = m.object(0xf795_e8ba, o, 1) else { continue };
                    for t in p[f(0x12f8_5293)].as_array().into_iter().flatten().filter(|t| matches!(t["Key"].as_u64(), Some(0x4bfd_27dd) | Some(0x023e_5a2f) | Some(0x2cd3_1b46))) {
                        v.extend(t["Item"].as_array().into_iter().flatten().filter_map(|x| x.as_u64()).map(|h| (true, h as u32, e.off)));
                    }
                }
            }
            v
        })
        .collect();
    let mut by_var: HashMap<u32, Vec<u64>> = HashMap::new();
    let mut heads: BTreeMap<u32, Vec<u64>> = BTreeMap::new();
    for (head, id, off) in found {
        if head {
            let v = heads.entry(id).or_default();
            if !v.contains(&off) {
                v.push(off);
            }
        } else {
            by_var.entry(id).or_default().push(off);
        }
    }
    let mut by_model = BTreeMap::new();
    for (model, outfits) in lists {
        // the resource that holds most of the model's variations
        let mut n: BTreeMap<u64, usize> = BTreeMap::new();
        for (v, _, _) in &outfits {
            for off in by_var.get(v).into_iter().flatten() {
                *n.entry(*off).or_insert(0) += 1;
            }
        }
        if let Some((&resource, _)) = n.iter().max_by_key(|(off, c)| (**c, std::cmp::Reverse(**off))) {
            by_model.insert(model, ModelRow { resource, outfits });
        }
    }
    let mut parts: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for (h, off) in parts_found.into_inner().unwrap() {
        parts.entry(h).or_default().push(off);
    }
    Models { version: CACHE_VERSION, by_model, heads, aliases, parts }
}

/// The model table (built from the packs.rpf index on first use, about ten
/// seconds, then cached in the work directory).
pub fn models(game_root: &Path) -> Result<&'static Models, String> {
    static T: OnceLock<Result<Models, String>> = OnceLock::new();
    T.get_or_init(|| {
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let cache = work.join("index").join("ped_models.json");
        let ixp = crate::scan::index_path(&work.join("index"), "packs.rpf");
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let Ok(b) = std::fs::read(&cache) {
            if let Ok(t) = serde_json::from_slice::<Models>(&b) {
                if t.version == CACHE_VERSION && stamp(&cache) >= stamp(&ixp) {
                    return Ok(t);
                }
            }
        }
        let ix = crate::scan::load_index(&ixp).map_err(|e| format!("packs.rpf index ({}): {e}; index packs.rpf first", ixp.display()))?;
        let t = build(&game_root.join("packs.rpf"), &ix);
        if t.by_model.is_empty() {
            return Err("no outfit lists found in packs.rpf".into());
        }
        if let Ok(b) = serde_json::to_vec(&t) {
            let _ = crate::atomic::write(&cache, b);
        }
        Ok(t)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Names for the models of the outfit list. The list stores only
/// joaat(model name), so a name is known only when a candidate hashes to a
/// listed model. Candidates come from the packs.rpf index (every name and
/// every `_` prefix of one: `cs_ali_head_…` gives `cs_ali`), from the name
/// dictionary, and from the game's text tables (capitalised words and runs
/// of two or three, lower-cased and joined with `_`, behind the ped model
/// prefixes: "Phil Cassidy" gives `cs_phil_cassidy`). A text or dictionary
/// candidate counts only with a ped model prefix; with about 13 million
/// candidates against 2,713 models, around eight names are expected to be
/// chance hits.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ModelNames {
    version: u32,
    /// joaat(model name) -> name
    pub names: BTreeMap<u32, String>,
    /// Names that may be chance hash hits: an ambient-style name
    /// (`a_f_y_<word>_NN`) made from a text word, with no second model on
    /// the same word. A control run with prefixes no ped model has finds as
    /// many such single hits as the real prefixes do.
    #[serde(default)]
    pub uncertain: std::collections::BTreeSet<u32>,
    /// joaat(model name) -> the character's name as the game shows it
    /// ("Mandy"). A character table of data.rpf (a PSO, record struct
    /// `691f01d2`) gives a text label (`f249cb70`) per character name
    /// (`Name`); a model's character name is the one the ped model table
    /// gives it (`Models::aliases`), else its own name without `cs_`. The
    /// string is taken from the text table that reads most like English
    /// (which table is which language is not stored).
    #[serde(default)]
    pub display: BTreeMap<u32, String>,
}

const NAMES_VERSION: u32 = 5;

/// The cached story cutscene catalog (`sixng cutscenes` or the Scenes tab
/// builds it), when there is one.
fn cutscene_catalog(work: &Path) -> Option<crate::cutscene::Catalog> {
    let b = std::fs::read(catalog_path(work)).ok()?;
    serde_json::from_slice::<crate::cutscene::Catalog>(&b).ok().filter(|c| c.version == crate::cutscene::CATALOG_VERSION)
}

fn catalog_path(work: &Path) -> std::path::PathBuf {
    crate::cutscene::cache_path(&work.join("index"), crate::cutscene::STORY_ARCHIVE)
}

/// Model names from the handles of cutscene clips (`arnold_rubio`,
/// `chart_bachelor2`): the handle, or the handle without its number, with or
/// without `cs_`, names a listed model (`listed(joaat)`) that has no name yet.
/// A name whose 64-bit id is also a scene file's ped model is certain; the
/// others go to `uncertain`.
/// The 64-bit model ids of a catalog's scene-file peds.
fn scene_peds(c: &crate::cutscene::Catalog) -> HashSet<u64> {
    c.files.iter().flat_map(|f| &f.objects).filter(|o| o.kind == "ped").filter_map(|o| o.model).collect()
}

/// The words of a name for candidate model names: lower case, at least three
/// letters, digits cut off the end (`afinal_goon3` -> `afinal`, `goon`).
fn name_words(n: &str) -> impl Iterator<Item = String> + '_ {
    n.split(|c: char| !c.is_ascii_alphanumeric())
        .map(|w| w.trim_end_matches(|c: char| c.is_ascii_digit()).to_ascii_lowercase())
        .filter(|w| w.len() >= 3 && w.len() <= 20 && w.bytes().all(|c| c.is_ascii_lowercase()))
}

fn names_from_scenes(c: &crate::cutscene::Catalog, listed: impl Fn(u32) -> bool, names: &mut BTreeMap<u32, String>, uncertain: &mut std::collections::BTreeSet<u32>) {
    let peds = scene_peds(c);
    let mut handles: Vec<String> = c.dicts.iter().flat_map(|d| &d.clips).map(|cl| crate::cutscene::split_clip_name(cl).0.to_ascii_lowercase()).collect();
    handles.sort();
    handles.dedup();
    for h in &handles {
        let base = h.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end_matches('_');
        for stem in [h.as_str(), base] {
            for cand in [stem.to_string(), format!("cs_{stem}")] {
                let j = joaat(&cand);
                if !cand.contains('_') || !listed(j) || names.contains_key(&j) {
                    continue;
                }
                if !peds.contains(&crate::meta7a::asset_id(&cand)) {
                    uncertain.insert(j);
                }
                names.insert(j, cand);
            }
        }
    }
}
const CHARACTER: u32 = 0x691f_01d2;
const DISPLAY_LABEL: u32 = 0xf249_cb70;

/// A name shaped like a ped model's: `cs_…`, `ig_…`, `player_…`,
/// `a_f_y_…` (kind, sex, age) or an animal root.
pub fn is_model_name(n: &str) -> bool {
    let b = n.as_bytes();
    ["cs_", "csb_", "hc_", "ig_", "mp_", "player_"].iter().any(|p| n.starts_with(p))
        || (b.len() > 6 && b[1] == b'_' && b[3] == b'_' && b[5] == b'_' && b"agsu".contains(&b[0]) && b"fm".contains(&b[2]) && b"ymo".contains(&b[4]))
        || crate::outfit::is_animal(n)
}

/// Name stems a line of text may hold: its capitalised words and runs of
/// two or three of them, lower-cased and joined with `_` ("Phil Cassidy" ->
/// `phil`, `cassidy`, `phil_cassidy`).
pub fn text_name_stems(s: &str, out: &mut HashSet<String>) {
    let mut run: Vec<String> = Vec::new();
    let mut flush = |run: &mut Vec<String>| {
        for i in 0..run.len() {
            for k in 1..=3.min(run.len() - i) {
                out.insert(run[i..i + k].join("_"));
            }
        }
        run.clear();
    };
    for piece in s.split(' ') {
        let w: String = piece.chars().filter(|c| *c != '\'').collect();
        let core = w.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        let ok = core.len() > 1 && core.len() < 24 && core.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && core.chars().all(|c| c.is_ascii_alphanumeric());
        if ok {
            run.push(core.to_ascii_lowercase());
        }
        // punctuation after the word, or a word that is no name, ends the run
        if !ok || w.ends_with(|c: char| !c.is_ascii_alphanumeric()) {
            flush(&mut run);
        }
    }
    flush(&mut run);
}

fn build_names(game_root: &Path, work: &Path, t: &Models, ix: &ArchiveIndex) -> ModelNames {
    let mut names: BTreeMap<u32, String> = BTreeMap::new();
    let keys = &t.by_model;
    // names and name prefixes of the index
    let mut seen: HashSet<&str> = HashSet::new();
    let mut words: HashSet<String> = HashSet::new();
    for e in &ix.entries {
        let Some(n) = e.name.as_deref() else { continue };
        if !seen.insert(n) {
            continue;
        }
        let n = n.to_ascii_lowercase();
        let cuts = n.match_indices('_').map(|(i, _)| i).skip(1).chain(std::iter::once(n.len()));
        // single words of the names, for the ambient forms below
        for w in n.split('_').filter(|w| w.len() >= 3 && w.bytes().all(|c| c.is_ascii_alphabetic())) {
            if !words.contains(w) {
                words.insert(w.to_string());
            }
        }
        for c in cuts {
            let p = &n[..c];
            // `hc_cs_x` and `hc_x` parts belong to the models `cs_x` and `x`
            for cand in [Some(p), p.strip_prefix("hc_"), p.strip_prefix("hc_cs_")].into_iter().flatten() {
                for cand in [cand.to_string(), format!("cs_{cand}")] {
                    let h = joaat(&cand);
                    if keys.contains_key(&h) && cand.contains('_') {
                        names.entry(h).or_insert(cand);
                    }
                }
            }
        }
    }
    let mut uncertain = std::collections::BTreeSet::new();
    // the 64-bit model ids of the scene files' peds, and the words of the
    // scenes' clip handles (`afinal_goon3` -> `afinal`, `goon`)
    let mut peds: HashSet<u64> = HashSet::new();
    let mut more_words: HashSet<String> = HashSet::new();
    if let Some(c) = cutscene_catalog(work) {
        names_from_scenes(&c, |h| keys.contains_key(&h), &mut names, &mut uncertain);
        peds = scene_peds(&c);
        for d in &c.dicts {
            for cl in &d.clips {
                more_words.extend(name_words(crate::cutscene::split_clip_name(cl).0));
            }
        }
    }
    // the words of the name dictionary's names
    if let Ok(d) = std::fs::read(crate::names::dict_path(work)).map_err(|_| ()).and_then(|b| serde_json::from_slice::<crate::names::Dict>(&b).map_err(|_| ())) {
        for (n, _) in d.names.values().flatten() {
            more_words.extend(name_words(n));
        }
    }
    more_words.retain(|w| !words.contains(w));
    // the name dictionary
    for &h in keys.keys() {
        if !names.contains_key(&h) {
            if let Some((n, _)) = crate::names::all(h).into_iter().find(|(n, _)| is_model_name(&n.to_ascii_lowercase())) {
                names.insert(h, n.to_ascii_lowercase());
            }
        }
    }
    // Ambient forms (`g_m_m_<word>_NN`) and numbered story models
    // (`cs_<word>_NN`): a hit counts when the same word also hits behind
    // another prefix (`g_f_m_` beside `g_m_m_`), which chance does not
    // produce. Several numbers behind one prefix prove nothing: models come
    // in numbered families, and one chance match of the hash state before
    // the number hits the whole family. A single hit from a text word is
    // kept, with the narrow number range only, and marked uncertain.
    let mut ambient: Vec<String> = Vec::new();
    for k in "agsu".chars() {
        for s in "fm".chars() {
            for age in "ymo".chars() {
                ambient.push(format!("{k}_{s}_{age}_"));
            }
        }
    }
    let numbered: Vec<String> = (0..10).map(|n| format!("_{n:02}")).collect();
    let mut display: BTreeMap<u32, String> = BTreeMap::new();
    let mut grouped = |stems: &HashSet<String>, names: &mut BTreeMap<u32, String>, with_cs: bool, keep_single: bool| {
        let found: Vec<(String, u32, String, bool)> = stems
            .par_iter()
            .flat_map_iter(|stem| {
                let mut v = Vec::new();
                for p in ambient.iter().map(String::as_str).chain(with_cs.then_some("cs_")) {
                    for suffix in std::iter::once("").chain(numbered.iter().map(String::as_str)) {
                        // `cs_<word>` alone is covered by the other sources
                        if p == "cs_" && suffix.is_empty() {
                            continue;
                        }
                        let cand = format!("{p}{stem}{suffix}");
                        let h = joaat(&cand);
                        if keys.contains_key(&h) {
                            v.push((stem.clone(), h, cand, matches!(suffix, "" | "_00" | "_01" | "_02")));
                        }
                    }
                }
                v
            })
            .collect();
        let mut per_stem: HashMap<&str, HashSet<&str>> = HashMap::new();
        for (stem, _, n, _) in &found {
            per_stem.entry(stem.as_str()).or_default().insert(n.find(stem.as_str()).map_or("", |i| &n[..i]));
        }
        let mut count: HashMap<&str, usize> = HashMap::new();
        for (stem, _, _, _) in &found {
            *count.entry(stem.as_str()).or_insert(0) += 1;
        }
        let mut found: Vec<&(String, u32, String, bool)> = found.iter().collect();
        found.sort();
        for (stem, h, n, narrow) in found {
            // the few index words make a chance family hit unlikely (about 0.05 expected)
            let several = per_stem[stem.as_str()].len() > 1 || (with_cs && count[stem.as_str()] > 1);
            if names.contains_key(h) || !(several || keep_single && *narrow) {
                continue;
            }
            names.insert(*h, n.clone());
            if !several {
                uncertain.insert(*h);
            }
        }
    };
    grouped(&words, &mut names, true, false);
    grouped(&more_words, &mut names, true, false);
    // the text tables of data.rpf, when it is indexed
    let ixp = crate::scan::index_path(&work.join("index"), "data.rpf");
    if let Ok(dix) = crate::scan::load_index(&ixp) {
        let path = game_root.join("data.rpf");
        let a = ArchiveInfo { id: "data.rpf".into(), size: std::fs::metadata(&path).map_or(0, |m| m.len()), container: "rpf9".into(), path };
        if let Ok((tables, _)) = crate::text::read_all(&a, &dix.entries, &[], crate::export::no_progress()) {
            let stems: HashSet<String> = tables
                .par_iter()
                .map(|t| {
                    let mut s = HashSet::new();
                    t.strings.values().for_each(|x| text_name_stems(&crate::text::plain(x), &mut s));
                    s
                })
                .reduce(HashSet::new, |mut a, b| {
                    a.extend(b);
                    a
                });
            let prefixes: Vec<String> = ["cs_", "hc_cs_", "hc_", "ig_", "csb_"].iter().map(|s| s.to_string()).collect();
            grouped(&stems, &mut names, false, true);
            let found: Vec<(u32, String)> = stems
                .par_iter()
                .flat_map_iter(|stem| {
                    let mut v = Vec::new();
                    for p in &prefixes {
                        for suffix in ["", "_00", "_01", "_02"] {
                            let cand = format!("{p}{stem}{suffix}");
                            let h = joaat(&cand);
                            if keys.contains_key(&h) {
                                v.push((h, cand));
                            }
                        }
                    }
                    v
                })
                .collect();
            let mut found = found;
            found.sort();
            for (h, n) in found {
                names.entry(h).or_insert(n);
            }
            // display names: character name -> label, label -> text
            let f = crate::meta7a::field_name;
            let labels: HashMap<u32, u32> = dix
                .entries
                .par_iter()
                .filter(|e| e.kind == crate::classify::Kind::Pso && e.usize >= 1 << 16)
                .flat_map_iter(|e| {
                    let mut v = Vec::new();
                    let Some(p) = crate::scan::read_entry(&a.path, e).ok().and_then(|b| crate::pso::parse(&b)) else { return v };
                    let m = p.meta();
                    for o in m.objects(CHARACTER) {
                        let Some(r) = m.object(CHARACTER, o, 1) else { continue };
                        if let (Some(name), Some(label)) = (r["Name"].as_u64(), r[f(DISPLAY_LABEL)].as_u64().filter(|l| *l != 0)) {
                            v.push((name as u32, label as u32));
                        }
                    }
                    v
                })
                .collect();
            let wanted: HashSet<u32> = labels.values().copied().collect();
            // per label the string of the most English-looking table that has it
            let mut text: HashMap<u32, (usize, String)> = HashMap::new();
            for tb in &tables {
                if !tb.strings.keys().any(|h| wanted.contains(h)) {
                    continue;
                }
                let score = tb.strings.values().filter(|s| s.contains(" the ")).count() + 1;
                for (h, s) in tb.strings.iter().filter(|(h, _)| wanted.contains(h)) {
                    let s = crate::text::plain(s);
                    if !s.is_empty() && text.get(h).is_none_or(|x| x.0 < score) {
                        text.insert(*h, (score, s));
                    }
                }
            }
            let shown = |character: u32| labels.get(&character).and_then(|l| text.get(l)).map(|x| x.1.clone());
            for (character, models) in &t.aliases {
                if let Some(s) = shown(*character) {
                    for m in models.iter().filter(|m| keys.contains_key(m)) {
                        display.entry(*m).or_insert_with(|| s.clone());
                    }
                }
            }
            for &m in keys.keys() {
                let own = names.get(&m).and_then(|n| n.strip_prefix("cs_")).map(joaat);
                if let Some(s) = own.and_then(&shown).or_else(|| shown(m)) {
                    display.entry(m).or_insert(s);
                }
            }
        }
    }
    // A candidate whose 64-bit id is also a scene file's ped model is that
    // model, whatever its form (`cs_felixtheten`, `g_m_m_cashcity_00`).
    if !peds.is_empty() {
        let prefixes: Vec<String> = ambient.iter().cloned().chain(["cs_", "csb_", "ig_", "hc_cs_", "hc_"].map(String::from)).collect();
        let suffixes: Vec<String> = std::iter::once(String::new()).chain((0..13).map(|n| format!("_{n:02}"))).collect();
        let found: Vec<(u32, String)> = words
            .par_iter()
            .chain(more_words.par_iter())
            .flat_map_iter(|stem| {
                let mut v = Vec::new();
                for p in &prefixes {
                    for x in &suffixes {
                        let cand = format!("{p}{stem}{x}");
                        let h = joaat(&cand);
                        if keys.contains_key(&h) && peds.contains(&crate::meta7a::asset_id(&cand)) {
                            v.push((h, cand));
                        }
                    }
                }
                v
            })
            .collect();
        for (h, n) in found {
            if !names.contains_key(&h) || uncertain.contains(&h) {
                uncertain.remove(&h);
                names.insert(h, n);
            }
        }
    }
    uncertain.retain(|h| names.contains_key(h));
    ModelNames { version: NAMES_VERSION, names, uncertain, display }
}

/// The model name table (built on first use, about twenty seconds when
/// data.rpf is indexed, then cached as `<work>/index/ped_model_names.json`).
pub fn model_names(game_root: &Path) -> Result<&'static ModelNames, String> {
    static T: OnceLock<Result<ModelNames, String>> = OnceLock::new();
    T.get_or_init(|| {
        let t = models(game_root)?;
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let cache = work.join("index").join("ped_model_names.json");
        let ixp = crate::scan::index_path(&work.join("index"), "packs.rpf");
        let dxp = crate::scan::index_path(&work.join("index"), "data.rpf");
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let Ok(b) = std::fs::read(&cache) {
            if let Ok(n) = serde_json::from_slice::<ModelNames>(&b) {
                if n.version == NAMES_VERSION && stamp(&cache) >= stamp(&ixp) && stamp(&cache) >= stamp(&dxp) && stamp(&cache) >= stamp(&catalog_path(&work)) {
                    return Ok(n);
                }
            }
        }
        let ix = crate::scan::load_index(&ixp).map_err(|e| format!("packs.rpf index ({}): {e}; index packs.rpf first", ixp.display()))?;
        let n = build_names(game_root, &work, t, &ix);
        if let Ok(b) = serde_json::to_vec(&n) {
            let _ = crate::atomic::write(&cache, b);
        }
        Ok(n)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// One model of the outfit list, for listing.
#[derive(Clone, Debug, Serialize)]
pub struct ListedModel {
    /// The model's name, or `ped_0x<hash>` when no name is known; `look`
    /// and `outfit:` take either.
    pub name: String,
    pub hash: String,
    pub named: bool,
    /// The character's name as the game shows it, where the data gives one.
    pub display: Option<String>,
    /// The name may be a chance hash hit (`ModelNames::uncertain`).
    pub uncertain: bool,
    pub outfits: usize,
}

/// Every model of the outfit list, named where a name is known, sorted by
/// name (the unnamed ones last).
pub fn listed_models(game_root: &Path) -> Result<Vec<ListedModel>, String> {
    let t = models(game_root)?;
    let n = model_names(game_root)?;
    let mut v: Vec<ListedModel> = t
        .by_model
        .iter()
        .map(|(h, row)| {
            let name = n.names.get(h);
            ListedModel { name: name.cloned().unwrap_or_else(|| format!("ped_0x{h:08x}")), hash: format!("{h:08x}"), named: name.is_some(), display: n.display.get(h).cloned(), uncertain: n.uncertain.contains(h), outfits: row.outfits.len() }
        })
        .collect();
    v.sort_by(|a, b| (!a.named, &a.name).cmp(&(!b.named, &b.name)));
    Ok(v)
}

/// The model hash a name stands for: `ped_0x<hash>` / `0x<hash>` as
/// written, else joaat of the name.
pub fn model_hash(model: &str) -> u32 {
    let m = model.strip_prefix("ped_").unwrap_or(model);
    m.strip_prefix("0x").filter(|x| x.len() <= 8).and_then(|x| u32::from_str_radix(x, 16).ok()).unwrap_or_else(|| joaat(model))
}

/// Textures the wardrobe data names: store hash -> (wardrobe resource, head
/// drawable hash or 0). Ped skin has no texture bound in its drawables: a
/// head's face, the body's normal, detail and mask maps and the eyes'
/// colour are named only by wardrobe items (`States` -> `Asset`), so the
/// texture-users index, built from model materials, does not know them.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WardrobeTextures {
    version: u32,
    pub by_texture: HashMap<u64, Vec<(u64, u32)>>,
}

/// The wardrobe texture table (built from the packs.rpf index on first use,
/// about a minute, then cached as `<work>/index/wardrobe_textures.json`).
pub fn wardrobe_textures(game_root: &Path) -> Result<&'static WardrobeTextures, String> {
    static T: OnceLock<Result<WardrobeTextures, String>> = OnceLock::new();
    T.get_or_init(|| {
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let cache = work.join("index").join("wardrobe_textures.json");
        let ixp = crate::scan::index_path(&work.join("index"), "packs.rpf");
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let Ok(b) = std::fs::read(&cache) {
            if let Ok(t) = serde_json::from_slice::<WardrobeTextures>(&b) {
                if t.version == 2 && stamp(&cache) >= stamp(&ixp) {
                    return Ok(t);
                }
            }
        }
        let ix = crate::scan::load_index(&ixp).map_err(|e| format!("packs.rpf index ({}): {e}; index packs.rpf first", ixp.display()))?;
        let packs = game_root.join("packs.rpf");
        fn hashes(v: &serde_json::Value, out: &mut Vec<u64>) {
            match v {
                serde_json::Value::Array(a) => a.iter().for_each(|x| hashes(x, out)),
                serde_json::Value::Object(m) => m.values().for_each(|x| hashes(x, out)),
                serde_json::Value::String(s) if s.len() == 16 => out.extend(u64::from_str_radix(s, 16).ok().filter(|&h| h != 0)),
                _ => {}
            }
        }
        let found: Vec<(u64, u64, u32)> = ix
            .entries
            .par_iter()
            .filter(|e| e.sub == "7a")
            .flat_map_iter(|e| {
                let mut v = Vec::new();
                let Ok(r) = Resource::load_meta(&packs, e) else { return v };
                let Some(m) = Meta::parse(&r) else { return v };
                if m.objects(VARIATION).is_empty() {
                    return v;
                }
                let f = crate::meta7a::field_name;
                for o in m.objects(0xf795_e8ba) {
                    let Some(p) = m.object(0xf795_e8ba, o, 4) else { continue };
                    let head = p[f(0x12f8_5293)].as_array().into_iter().flatten().filter(|t| t["Key"].as_u64() == Some(0x4bfd_27dd)).find_map(|t| t["Item"].as_array()?.first()?.as_u64()).unwrap_or(0) as u32;
                    let mut hs = Vec::new();
                    hashes(&p[f(0xebac_f18c)], &mut hs);
                    hs.sort();
                    hs.dedup();
                    v.extend(hs.into_iter().map(|h| (h, e.off, head)));
                }
                v
            })
            .collect();
        let mut by_texture: HashMap<u64, Vec<(u64, u32)>> = HashMap::new();
        for (h, off, head) in found {
            let v = by_texture.entry(h).or_default();
            if !v.contains(&(off, head)) {
                v.push((off, head));
            }
        }
        if by_texture.is_empty() {
            return Err("no wardrobe textures found in packs.rpf".into());
        }
        let t = WardrobeTextures { version: 2, by_texture };
        if let Ok(b) = serde_json::to_vec(&t) {
            let _ = crate::atomic::write(&cache, b);
        }
        Ok(t)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Drawables by asset GUID, over every wardrobe resource: an item names
/// its drawable by GUID only (`States` -> `Asset` -> `Drawable`), and the
/// drawable is found through the item's textures. Where an item's own
/// textures lead to no model, another wardrobe's item with the same GUID
/// often does resolve; this table carries that answer over.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AssetDrawables {
    version: u32,
    /// GUID (hex) -> the drawables its items resolved to, with how often.
    pub by_guid: HashMap<String, Vec<(String, u32)>>,
}

impl AssetDrawables {
    /// The drawable of a GUID: one of the character's fit before the
    /// others, then the one most items agree on.
    pub fn pick(&self, guid: &str, fit: Option<&str>, exists: &dyn Fn(&str) -> bool) -> Option<String> {
        let has_fit = |n: &str| fit.is_some_and(|f| n.split('_').any(|t| t == f));
        self.by_guid.get(guid)?.iter().filter(|(n, _)| exists(n)).max_by(|a, b| has_fit(&a.0).cmp(&has_fit(&b.0)).then(a.1.cmp(&b.1)).then(b.0.cmp(&a.0))).map(|x| x.0.clone())
    }
}

/// The asset table (built from every wardrobe resource on first use, a
/// few minutes, then cached as `<work>/index/asset_drawables.json`).
pub fn asset_drawables(a: &ArchiveInfo, ix: &ArchiveIndex) -> Result<&'static AssetDrawables, String> {
    static T: OnceLock<Result<AssetDrawables, String>> = OnceLock::new();
    T.get_or_init(|| {
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let cache = work.join("index").join("asset_drawables.json");
        let ixp = crate::scan::index_path(&work.join("index"), "packs.rpf");
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let Ok(b) = std::fs::read(&cache) {
            if let Ok(t) = serde_json::from_slice::<AssetDrawables>(&b) {
                if t.version == 1 && stamp(&cache) >= stamp(&ixp) {
                    return Ok(t);
                }
            }
        }
        let tu = tex_users(&work)?;
        let pre = crate::wardrobe::prefixes(ix.entries.iter().filter(|e| e.kind == crate::classify::Kind::RageObj).filter_map(|e| e.name.as_deref()));
        let names: HashSet<String> = ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| e.name.as_ref().map(|n| n.to_ascii_lowercase())).collect();
        let exists = |n: &str| names.contains(n);
        let found: Vec<(String, String)> = ix
            .entries
            .par_iter()
            .filter(|e| e.sub == "7a")
            .flat_map_iter(|e| {
                let mut v = Vec::new();
                let Ok(r) = Resource::load_meta(&a.path, e) else { return v };
                let Some(m) = Meta::parse(&r) else { return v };
                if m.objects(VARIATION).is_empty() {
                    return v;
                }
                // each item once per resource, whatever the number of outfits that wear it
                let mut seen = HashSet::new();
                for var in crate::wardrobe::variations(&m, &pre, tu, &a.id) {
                    for p in &var.parts {
                        let Some(g) = p.guid.as_ref().filter(|_| seen.insert(p.item.clone())) else { continue };
                        if let Some((d, _)) = choose(p, None, &exists) {
                            v.push((g.clone(), d));
                        }
                    }
                }
                v
            })
            .collect();
        let mut by_guid: HashMap<String, Vec<(String, u32)>> = HashMap::new();
        for (g, d) in found {
            let v = by_guid.entry(g).or_default();
            match v.iter_mut().find(|x| x.0 == d) {
                Some(x) => x.1 += 1,
                None => v.push((d, 1)),
            }
        }
        if by_guid.is_empty() {
            return Err("no wardrobe item of packs.rpf resolved to a drawable".into());
        }
        let t = AssetDrawables { version: 1, by_guid };
        if let Ok(b) = serde_json::to_vec(&t) {
            let _ = crate::atomic::write(&cache, b);
        }
        Ok(t)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Whether the outfit list has this model (a character prefix such as
/// `cs_ali`); false when the table is not available.
pub fn has_model(game_root: &Path, model: &str) -> bool {
    models(game_root).is_ok_and(|t| t.by_model.contains_key(&joaat(&model.to_ascii_lowercase())))
}

/// The models whose wardrobe names a head of this character prefix
/// (`cs_lucia` → the heads `cs_lucia_head_*`), for characters the outfit
/// list has under another model name.
fn rows_by_head<'a>(t: &'a Models, ix: &ArchiveIndex, prefix: &str) -> Vec<&'a ModelRow> {
    let p = format!("{prefix}_head_");
    let mut res: Vec<u64> = ix
        .entries
        .iter()
        .filter_map(|e| e.name.as_ref())
        .map(|n| n.to_ascii_lowercase())
        .filter(|n| n.starts_with(&p))
        .flat_map(|n| t.heads.get(&joaat(&n)).cloned().unwrap_or_default())
        .collect();
    res.sort();
    res.dedup();
    t.by_model.values().filter(|r| res.contains(&r.resource)).collect()
}

/// One part of an outfit.
#[derive(Debug, Clone, Serialize)]
pub struct LookPart {
    /// Outfit slot: head, eyes, teeth, body upper, body lower, hair, brows,
    /// lashes, beard, top, outer, bottom, feet, full, or an accessory word.
    pub slot: String,
    pub drawable: String,
    /// Wardrobe item id (hex).
    pub item: String,
    /// The drawable is a strand-hair placeholder (3 vertices): the hair is
    /// the groom asset `guid`.
    pub strands: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tint: Option<PartTint>,
    /// Flat colour of the item's state (ARGB), when it stores one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colour: Option<[u8; 4]>,
    /// Colour word of the item's colour tag (black, blue, khaki …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colour_tag: Option<&'static str>,
    /// Hair colour preset of a hair, brow, lash or beard item (hex): the key
    /// of `hairtone`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hair_preset: Option<String>,
    /// Base colour (sRGB) per sub-material from the item's garment preset
    /// (`matlib`); empty when the item names none or it is not in the
    /// library.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub preset_colours: Vec<Option<[u8; 3]>>,
    /// The item's garment preset (hex): the key of `matlib` for its tiles
    /// and tint-mask layers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tex_remap: Vec<(String, String)>,
    /// Colour and normal textures the item names (hex), item first: used for
    /// heads, whose drawables bind no face texture.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skins: Vec<(String, String)>,
    /// How the drawable was found: "name" (the item's tags name it),
    /// "textures" (it shares most of the item's textures), "head" (the eyes
    /// named after the head) or "default" (the whole skin mesh of the fit,
    /// added when the outfit resolves no body).
    pub found_by: &'static str,
    /// The item's category (plural word or hex), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Layer of the item's state (hex): the layer its own vertex regions
    /// belong to (`wardrobe::Part::layer`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// Hide masks of the item's state: layer id (hex) -> 256-bit set over
    /// that layer's regions (`wardrobe::Part::hides`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hides: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hides_sub: Vec<(String, u32, String)>,
    /// Hair only: the item state used because of the outfit's headwear
    /// (`hathairalt`, `helmethairalt` …; `guid` is that state's asset), and
    /// the headwear that asked for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hair_state: Option<String>,
    /// Hair only: the asset of each state the item has (state name hex ->
    /// GUID).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_guids: Vec<(String, String)>,
}

/// The hair states to try, in order, under a piece of headwear, from the
/// item word of its drawable (`hatbeanie…`, `helmetmotor`, `masksdiving`,
/// `wrapbandana`). The state names are the game's (joaat of each is a state
/// of hair items); that this headwear picks that state is read from the
/// names, not from a rule in the data.
pub fn hair_states_under(word: &str) -> &'static [&'static str] {
    if word.starts_with("helmet") {
        &["helmethairalt", "hairhidden"]
    } else if word.contains("beanie") {
        &["beaniehairalt", "hathairalt"]
    } else if word.contains("bandana") {
        &["bandanahairalt"]
    } else if word.starts_with("hat") {
        &["hathairalt"]
    } else if word.starts_with("mask") {
        &["maskhairalt"]
    } else {
        &[]
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Outfit {
    /// Variation id (hex).
    pub id: String,
    pub outfit_type: String,
    pub scale: f32,
    /// Made from the main garments ("jacket track, pants track").
    pub label: String,
    /// Body-shape preset name (`BodyShapePreset` variation), when included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_shape: Option<String>,
    pub parts: Vec<LookPart>,
    /// Strand colour sets the outfit's colour items give per item category (category hash, set name hash; hex).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub strand_colours: Vec<(String, String)>,
    /// Items of the outfit that resolved to no drawable (item id, slot hash).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
    /// Items left out on purpose (item id, why): first-person-only meshes
    /// and the anatomical body layer, which clothing always covers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub not_shown: Vec<String>,
    /// The wardrobe variations this outfit is made of (hex): the outfit's
    /// own first, then those it includes (head set, body set, brows …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variations: Vec<String>,
}

/// Why an item that resolved to no shown drawable is left out on purpose:
/// the first-person arms (its GUID or every texture match is an `_fp`
/// drawable) or the anatomical body layer (category `2a7a13a4`).
fn not_shown_reason(p: &Part, by_guid: &HashMap<u64, &String>) -> Option<&'static str> {
    let fp = |n: &str| n.ends_with("_fp") || n.contains("_fp_");
    let named = p.guid.as_deref().and_then(|g| u64::from_str_radix(g, 16).ok()).and_then(|g| by_guid.get(&g));
    if named.is_some_and(|n| fp(n)) || (named.is_none() && !p.candidates.is_empty() && p.candidates.keys().all(|n| fp(n))) {
        return Some("first-person view only");
    }
    // The four items of this category in the named characters' wardrobes
    // (2026-11-25) name by GUID `bodygenitalsgen_mr1_000_regular_ws00_full`,
    // its `_alt_00` and a hidden placeholder: drawables of a content pack
    // (core_restofworld), not of packs.rpf. Clothing covers them.
    // The same layer is also stored under another category (`c8bc4e01`,
    // `cs_raina`'s nude outfit: `bodygenitalsgen_fb1_008_regular_ws00_full`),
    // so an item whose GUID names one of these drawables counts too.
    let layer = p.guid.as_deref().and_then(|g| u64::from_str_radix(g, 16).ok()).is_some_and(|g| body_layer_ids().contains(&g));
    if p.category.as_deref() == Some("2a7a13a4") || layer {
        return Some("anatomical body layer, always under clothing");
    }
    None
}

/// Asset ids of the anatomical body layer drawables
/// (`bodygenitalsgen_<fit>_<nnn>_regular_ws00_full`, `_alt_00` to `_alt_02`;
/// ten of them are in core_restofworld, of fits `fr1`, `fb1`, `fo1` and
/// `mr1`), over every fit-shaped tag and numbers 000 to 031.
fn body_layer_ids() -> &'static HashSet<u64> {
    static T: OnceLock<HashSet<u64>> = OnceLock::new();
    T.get_or_init(|| {
        let mut s = HashSet::new();
        for sex in ['m', 'f'] {
            for kind in 'a'..='z' {
                for d in '0'..='9' {
                    for n in 0..32 {
                        let base = format!("bodygenitalsgen_{sex}{kind}{d}_{n:03}_regular_ws00_full");
                        for suffix in ["", "_alt_00", "_alt_01", "_alt_02"] {
                            s.insert(crate::meta7a::asset_id(&format!("{base}{suffix}")));
                        }
                    }
                }
            }
        }
        s
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Look {
    pub model: String,
    /// Wardrobe resource (`packs.rpf@<hex>`).
    pub resource: String,
    /// Fit tag of the model's head (`fr1`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
    pub outfits: Vec<Outfit>,
    /// Variations of the model's wardrobe that no listed outfit reaches,
    /// with their resolved parts (head, eyes and the default skin are not
    /// added to them). Empty when the wardrobe is another character's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_variations: Vec<Outfit>,
    /// How the outfits were found when no stored link names this model: a
    /// head-shape match or a wardrobe that names its hair.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Strand-hair item words (3-vertex placeholder drawables).
pub fn is_strand_word(w: &str) -> bool {
    ["hair", "beard", "brows", "lashes", "bodyhair", "chesthair", "armpithair", "leghair", "genitalhair", "chin", "chops", "wigs", "stubble"].iter().any(|p| w.starts_with(p))
}

/// Item word of a ped drawable name: the token before the fit tag
/// (`cs_ali_hairdreadlocks_fr1_…` → `hairdreadlocks`, `hand_fr1_…` → `hand`).
pub fn item_word(name: &str) -> &str {
    let t: Vec<&str> = name.split('_').collect();
    match t.iter().position(|x| crate::outfit::is_fit(x)) {
        Some(i) if i > 0 => t[i - 1],
        _ => t[0],
    }
}

/// Outfit slot of a ped drawable name.
pub fn slot_of(name: &str) -> String {
    let w = item_word(name);
    if w == "head" {
        return if name.contains("_eyes") { "eyes" } else { "head" }.to_string();
    }
    match w {
        "hand" => return "body upper".into(),
        "feet" => return "body lower".into(),
        _ => {}
    }
    for (p, s) in [("bracelet", "bracelet"), ("teethaccs", "teeth accessory"), ("teeth", "teeth"), ("eyecaps", "eyecaps"), ("bodyhair", "body hair"), ("hair", "hair"), ("wigs", "hair"), ("beard", "beard"), ("brows", "brows"), ("lashes", "lashes")] {
        if w.starts_with(p) {
            return s.to_string();
        }
    }
    // one-piece swimwear and bodysuits cover like a one-piece garment, the
    // sports base layers like a top or a bottom
    for (p, s) in [("swimsuit", "full"), ("bodysuit", "full"), ("lingeriebodysuit", "full"), ("basebottoms", "bottom"), ("basetops", "top")] {
        if w.starts_with(p) {
            return s.to_string();
        }
    }
    crate::outfit::slot_word(w)
}

/// The garment word of a wardrobe item category (the category is the
/// joaat of a plural word; given as that word or as hex), for the
/// categories whose missing model leaves a character visibly undressed.
pub fn garment_category(category: &str) -> Option<&'static str> {
    ["shirts", "tshirts", "tanktops", "hoodies", "jackets", "pants", "shorts", "skirts", "dresses", "shoes", "boots"].into_iter().find(|w| category == *w || category == format!("{:08x}", joaat(w)))
}

fn tex_users(work: &Path) -> Result<&'static crate::texusers::TexUsers, String> {
    static T: OnceLock<Result<crate::texusers::TexUsers, String>> = OnceLock::new();
    T.get_or_init(|| {
        let p = work.join("index").join("texture_users.jsonl.gz");
        crate::texusers::TexUsers::load(&p).map_err(|e| format!("texture-users index ({}): {e:#}; build it with `sixng texture-users` (Textures tab, Build index)", p.display()))
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// The drawable for a wardrobe part (see the module notes).
fn choose(p: &Part, fit: Option<&str>, exists: &dyn Fn(&str) -> bool) -> Option<(String, &'static str)> {
    if let Some(n) = p.named.as_ref().filter(|n| exists(n)) {
        return Some((n.clone(), "name"));
    }
    let usable = |n: &str| exists(n) && !n.contains("_cull") && !n.ends_with("_fp") && !n.contains("_fp_");
    let top = p.candidates.iter().filter(|(n, _)| usable(n)).map(|(_, c)| *c).max()?;
    let has_fit = |n: &str| fit.is_some_and(|f| n.split('_').any(|t| t == f));
    let best = p.candidates.iter().filter(|(n, c)| **c == top && usable(n)).max_by(|a, b| has_fit(a.0).cmp(&has_fit(b.0)).then(a.0.ends_with("_full").cmp(&b.0.ends_with("_full"))).then(b.0.cmp(a.0)))?;
    // strand placeholders all share two marker textures: only a name finds one
    if is_strand_word(item_word(best.0)) {
        return None;
    }
    // Body skin: the `nude_cull` mesh of the same fit carries the region of
    // every vertex (mesh stream 13), which the hide masks of the clothes
    // address; the `_full` mesh has none.
    if matches!(item_word(best.0), "hand" | "feet") {
        if let Some(cull) = best.0.strip_suffix("_full").map(|b| format!("{b}_nude_cull")).filter(|n| exists(n)) {
            return Some((cull, "textures"));
        }
    }
    Some((best.0.clone(), "textures"))
}

/// The whole skin mesh of a fit (`hand` = upper body, `feet` = lower body):
/// the `nude_cull` one, whose vertices carry the regions that clothes hide,
/// else `_full`.
fn whole_body(word: &str, fit: &str, exists: &dyn Fn(&str) -> bool) -> Option<String> {
    ["nude_cull", "full", "nude_full"].iter().map(|s| format!("{word}_{fit}_000_{s}")).find(|n| exists(n))
}

fn pretty(word: &str) -> String {
    // "jackettrack" -> "jacket track" for the known garment stems
    const STEMS: &[&str] = &[
        "tshirt", "tanktop", "sweatshirt", "shirt", "hoodie", "jacket", "coat", "blazer", "pants", "shorts", "skirt", "dress", "shoes", "boots", "fullbody", "swimsuit", "undies", "bra", "basetops", "basebottoms", "vest", "gilet", "bodysuit", "blouse", "leggings",
    ];
    match STEMS.iter().filter(|s| word.starts_with(**s) && word.len() > s.len() && !word.starts_with("bracelet")).max_by_key(|s| s.len()) {
        Some(s) => format!("{} {}", s, &word[s.len()..]),
        None => word.to_string(),
    }
}

/// The outfits of a model (`cs_ali`, `player_story000_00` …).
pub fn look(game_root: &Path, a: &ArchiveInfo, ix: &ArchiveIndex, model: &str) -> Result<Look, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Result<Look, String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = model.to_ascii_lowercase();
    if let Some(r) = cache.lock().unwrap().get(&key) {
        return r.clone();
    }
    let r = look_uncached(game_root, a, ix, &key).map(dressed_first);
    cache.lock().unwrap().insert(key, r.clone());
    r
}

/// The outfit list's order is kept, except that the first outfit (the one
/// shown when a character is opened) is a dressed one when there is one:
/// for about twenty characters the first listed outfit resolves to
/// accessories and skin only while later ones have clothes.
fn dressed_first(mut l: Look) -> Look {
    let has = |o: &Outfit, s: &str| o.parts.iter().any(|p| p.slot == s);
    // underwear alone is not a bottom
    let bottom = |o: &Outfit| o.parts.iter().any(|p| p.slot == "bottom" && !item_word(&p.drawable).starts_with("undies"));
    let dressed = |o: &Outfit| has(o, "full") || (has(o, "top") || has(o, "outer")) && bottom(o);
    if l.outfits.first().is_some_and(|o| !dressed(o)) {
        // of the dressed ones, the first with shoes when one has them
        let pick = l.outfits.iter().position(|o| dressed(o) && has(o, "feet")).or_else(|| l.outfits.iter().position(dressed));
        if let Some(i) = pick {
            let o = l.outfits.remove(i);
            l.outfits.insert(0, o);
            let say = "The first listed outfit has no clothes in the data read here, so the first dressed one is shown first.";
            l.note = Some(match l.note.take() {
                Some(n) => format!("{n} {say}"),
                None => say.to_string(),
            });
        }
    }
    l
}

fn look_uncached(game_root: &Path, a: &ArchiveInfo, ix: &ArchiveIndex, model: &str) -> Result<Look, String> {
    let t = models(game_root)?;
    if let Some(row) = t.by_model.get(&model_hash(model)) {
        return look_row(t, a, ix, model, row, false);
    }
    // listed under another model name: the models whose wardrobe names this
    // character's head, and of their outfits those that wear it
    let rows = rows_by_head(t, ix, model);
    let mut out: Option<Look> = None;
    let mut last_err = None;
    for row in rows {
        match look_row(t, a, ix, model, row, true) {
            Ok(l) => match &mut out {
                Some(o) => o.outfits.extend(l.outfits.into_iter().filter(|x| !o.outfits.iter().any(|y| y.id == x.id)).collect::<Vec<_>>()),
                None => out = Some(l),
            },
            Err(e) => last_err = Some(e),
        }
    }
    if let Some(l) = out {
        return Ok(l);
    }
    // A clothing set that another model's wardrobe wears (`cs_roxy` ->
    // `cs_nastydimez_roxyrashad`)
    if let Some(l) = look_by_parts(t, a, ix, model) {
        return Ok(l);
    }
    // The ped model table names a model after this character: `cs_lucia`
    // -> the one model whose character name is joaat("lucia"). That model's
    // outfits, with this character's own head when it has one.
    if let Some(l) = look_by_alias(t, a, ix, model) {
        return Ok(l);
    }
    // A head no wardrobe names: the character whose head has the nearest shape
    if let Some(l) = look_by_head_shape(t, a, ix, model) {
        return Ok(l);
    }
    // no outfit data at all (cutscene-only models): its own parts, or for a
    // head with nothing to wear, stand-in clothes
    own_look(a, ix, model)
        .or_else(|| look_stand_in(t, a, ix, model))
        .ok_or_else(|| last_err.unwrap_or_else(|| format!("{model} is not in the game's outfit list, and no wardrobe names its head")))
}

/// Stand-in clothes for a character that is only a head (`cs_blair`,
/// `cs_vern`): nothing in the files says what it wears, so it is shown in
/// the outfits of the listed model whose head is nearest in shape, among
/// the heads of the same fit and ethnicity code (and the same age letter,
/// `_m` / `_o` / `_y` / `_k`, when there are enough), crowd heads included.
/// The character keeps its own head, eyes, hair and other parts of its own
/// (Connie's bandana, Mischa's piercing), each in place of the stand-in's
/// part of the same slot. The note says it is a stand-in.
fn look_stand_in(t: &Models, a: &ArchiveInfo, ix: &ArchiveIndex, model: &str) -> Option<Look> {
    let drs = crate::outfit::dedupe(ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| {
        let n = e.name.as_ref()?.to_ascii_lowercase();
        (item_word(&n) == "head" && !n.contains("_eye") && !n.contains("_earring")).then_some((n, e))
    }));
    let own_p = format!("{model}_head_");
    let Some((own_name, own_e)) = drs.iter().find(|(n, _)| n.starts_with(&own_p)) else {
        return look_stand_in_hair(t, a, ix, model);
    };
    // "head_fr1_lat_" and the age letter after the number
    let tail = &own_name[own_name.rfind("head_")?..];
    let seg: Vec<&str> = tail.split('_').collect();
    let code = format!("{}_{}_{}_", seg.first()?, seg.get(1)?, seg.get(2)?);
    let age = seg.get(4).copied().unwrap_or("");
    let age_of = |n: &str| n.rfind("head_").and_then(|i| n[i..].split('_').nth(4).map(str::to_string)).unwrap_or_default();
    let listed = |n: &String| t.heads.contains_key(&joaat(n)) || t.heads.contains_key(&joaat(n.trim_end_matches("_full")));
    let mut cands: Vec<(&String, &&crate::scan::Entry)> = drs.iter().filter(|(n, _)| n.contains(&code) && !n.starts_with(&own_p) && listed(n)).collect();
    if cands.iter().filter(|(n, _)| age_of(n) == age).count() >= 5 {
        cands.retain(|(n, _)| age_of(n) == age);
    }
    let own = mesh_points(a, own_e)?;
    let own: Vec<[f32; 3]> = own.into_iter().step_by(5).collect();
    let mut dist: Vec<(f32, &String)> = cands
        .par_iter()
        .filter_map(|(n, e)| {
            let p: Vec<[f32; 3]> = mesh_points(a, e)?.into_iter().step_by(2).collect();
            if p.len() < 1000 {
                return None;
            }
            let sum: f32 = own.iter().map(|q| p.iter().map(|r| (q[0] - r[0]).powi(2) + (q[1] - r[1]).powi(2) + (q[2] - r[2]).powi(2)).fold(f32::MAX, f32::min).sqrt()).sum();
            Some((sum / own.len() as f32, *n))
        })
        .collect();
    dist.sort_by(|x, y| x.0.total_cmp(&y.0));
    // the character's own parts besides the head: eyes, hair, accessories
    let own_parts: Vec<String> = crate::outfit::outfit_options(ix, model).into_iter().map(|o| o.name).filter(|n| n != own_name && !n.contains("_eyecap")).collect();
    let own_eyes = own_parts.iter().find(|n| n.starts_with(&own_p) && n.contains("_eyes")).cloned();
    // one per slot, the first in name order (a hair has `_m` / `_p` forms)
    let mut extras: Vec<&String> = Vec::new();
    for n in own_parts.iter().filter(|n| !n.starts_with(&own_p)) {
        if !extras.iter().any(|x| slot_of(x) == slot_of(n)) {
            extras.push(n);
        }
    }
    for (d, head) in &dist {
        // the listed models whose wardrobe names this head
        let res: Vec<u64> = [head.as_str(), head.trim_end_matches("_full")].iter().flat_map(|n| t.heads.get(&joaat(n)).cloned().unwrap_or_default()).collect();
        // a model with everyday clothes (not only swimwear, diving gear or nothing)
        let Some(mut l) = t.by_model.values().filter(|r| res.contains(&r.resource)).find_map(|row| look_row(t, a, ix, model, row, false).ok().filter(|l| l.outfits.iter().any(everyday))) else { continue };
        // everyday outfits first, of those the ones that wear this head
        l.outfits.sort_by_key(|o| (!everyday(o), !o.parts.iter().any(|p| &p.drawable == *head)));
        for o in &mut l.outfits {
            for p in &mut o.parts {
                if p.slot == "head" {
                    p.drawable = own_name.clone();
                    p.skins.clear();
                    p.found_by = "own";
                } else if p.slot == "eyes" {
                    if let Some(e) = &own_eyes {
                        p.drawable = e.clone();
                        p.skins.clear();
                        p.found_by = "own";
                    }
                }
            }
            for x in &extras {
                let slot = slot_of(x);
                let strands = is_strand_word(item_word(x));
                o.parts.retain(|p| p.slot != slot);
                o.parts.push(LookPart { strands, slot, drawable: (*x).clone(), item: String::new(), guid: strands.then(|| format!("{:016x}", crate::meta7a::asset_id(x))), tint: None, colour: None, colour_tag: None, hair_preset: None, preset_colours: vec![], preset: None, tex_remap: vec![], skins: vec![], found_by: "own", category: None, layer: None, hides: vec![], hides_sub: vec![], hair_state: None, state_guids: vec![] });
            }
        }
        l.other_variations.clear();
        let other = crate::outfit::outfit_prefix(head).unwrap_or_else(|| head.to_string());
        let who = t.by_model.iter().find(|(_, r)| r.resource == l_resource(&l)).map(|(h, _)| model_names(&crate::export::game_root(a)).ok().and_then(|m| m.names.get(h).cloned()).unwrap_or_else(|| format!("ped_0x{h:08x}")));
        l.note = Some(format!(
            "Stand-in clothes: the game files give {model} only a head, and nothing links it to an outfit. It is shown in the outfits of {} (whose head {other} is nearest in shape, {:.1} mm mean distance), with its own head{}. Not the character's real clothes.",
            who.unwrap_or_else(|| "the model".into()),
            d * 1000.0,
            if extras.is_empty() { String::new() } else { format!(" and parts ({})", extras.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")) }
        ));
        l.model = model.to_string();
        return Some(l);
    }
    None
}

/// Stand-in for a character that is only a hair (`cs_byron_dugal`: a
/// mohawk): the first listed model of the hair's fit (in model hash order)
/// with an everyday outfit that wears a hair of the same item word
/// (`hairmohawk`), or failing that the first with an everyday outfit. That
/// model's outfits and head, with this character's own hair in place of
/// the model's.
fn look_stand_in_hair(t: &Models, a: &ArchiveInfo, ix: &ArchiveIndex, model: &str) -> Option<Look> {
    let own: Vec<String> = crate::outfit::outfit_options(ix, model).into_iter().map(|o| o.name).collect();
    let hair = own.iter().find(|n| slot_of(n) == "hair")?.clone();
    let fit = hair.split('_').find(|x| crate::outfit::is_fit(x))?.to_string();
    let word = item_word(&hair).to_string();
    let wears = |o: &Outfit| o.parts.iter().any(|p| p.slot == "hair" && item_word(&p.drawable) == word);
    let mut rows: Vec<(&u32, &ModelRow)> = t.by_model.iter().collect();
    rows.sort_by_key(|(h, _)| **h);
    let mut first: Option<(u32, Look)> = None;
    let mut best: Option<(u32, Look)> = None;
    for (h, row) in rows {
        let Ok(l) = look_row(t, a, ix, model, row, false) else { continue };
        if l.fit.as_deref() != Some(fit.as_str()) || !l.outfits.iter().any(everyday) {
            continue;
        }
        if l.outfits.iter().any(|o| everyday(o) && wears(o)) {
            best = Some((*h, l));
            break;
        }
        if first.is_none() {
            first = Some((*h, l));
        }
    }
    let matched = best.is_some();
    let (h, mut l) = best.or(first)?;
    l.outfits.sort_by_key(|o| (!everyday(o), !wears(o)));
    for o in &mut l.outfits {
        o.parts.retain(|p| p.slot != "hair");
        o.parts.push(LookPart { strands: true, slot: "hair".into(), drawable: hair.clone(), item: String::new(), guid: Some(format!("{:016x}", crate::meta7a::asset_id(&hair))), tint: None, colour: None, colour_tag: None, hair_preset: None, preset_colours: vec![], preset: None, tex_remap: vec![], skins: vec![], found_by: "own", category: None, layer: None, hides: vec![], hides_sub: vec![], hair_state: None, state_guids: vec![] });
    }
    l.other_variations.clear();
    let who = model_names(&crate::export::game_root(a)).ok().and_then(|m| m.names.get(&h).cloned()).unwrap_or_else(|| format!("ped_0x{h:08x}"));
    l.note = Some(format!(
        "Stand-in: the game files give {model} only a hair ({hair}), no head, and nothing links it to an outfit. It is shown as {who}{}, with its own hair. Not the character's real head or clothes.",
        if matched { format!(", a model that wears a {word} hair too") } else { String::new() }
    ));
    l.model = model.to_string();
    Some(l)
}

/// An outfit of everyday clothes: a top (or outer layer) and a bottom, no
/// mask, no swimwear (`undies swimming`, `bra bikini`).
fn everyday(o: &Outfit) -> bool {
    let has = |s: &str| o.parts.iter().any(|p| p.slot == s);
    let swim = o.parts.iter().any(|p| ["undiesswimming", "brabikini", "undiesgstring", "shortsswim", "swimsuit"].contains(&item_word(&p.drawable)));
    (has("top") || has("outer")) && has("bottom") && !has("mask") && !swim
}

/// The wardrobe resource offset of a look (`packs.rpf@<hex>`).
fn l_resource(l: &Look) -> u64 {
    l.resource.rsplit('@').next().and_then(|h| u64::from_str_radix(h.split_whitespace().next().unwrap_or(""), 16).ok()).unwrap_or(0)
}

/// Vertex positions of a drawable's largest mesh.
/// Vertex positions of a drawable's largest mesh (RAGE model space).
pub fn mesh_points(a: &ArchiveInfo, e: &crate::scan::Entry) -> Option<Vec<[f32; 3]>> {
    let r = Resource::load(&a.path, e).ok()?;
    crate::headdelta::meshes(&r).into_iter().map(|m| m.0).max_by_key(|p| p.len())
}

/// For a character the outfit list has under another model: the model
/// whose character name in the ped model table is this prefix without its
/// first word (`cs_lucia` -> "lucia" -> `player_story000_00`), when exactly
/// one model carries that name. The character's own head and eyes replace
/// the model's when it has them (`cs_jason` has none).
fn look_by_alias(t: &Models, a: &ArchiveInfo, ix: &ArchiveIndex, model: &str) -> Option<Look> {
    let short = model.split_once('_')?.1;
    // the one model the table names after the character; else the listed
    // model this one is a variant of: a second head of the same person
    // (`hc_cs_chuck` -> `cs_chuck`) or the same person under a crowd-style
    // name (`u_f_o_rhonda` -> `cs_rhonda`)
    let by_name = match t.aliases.get(&joaat(short)).map(Vec::as_slice) {
        Some([target]) => Some((*target, short.to_string())),
        _ => None,
    };
    let variant = || {
        let words: Vec<&str> = model.split('_').collect();
        let mut c = Vec::new();
        if let Some(rest) = model.strip_prefix("hc_") {
            c.push(rest.to_string());
        }
        if words.len() >= 4 && words[0].len() == 1 && words[1].len() == 1 && words[2].len() == 1 {
            c.push(format!("cs_{}", words[3..].join("_")));
        }
        c.into_iter().find(|n| t.by_model.contains_key(&joaat(n))).map(|n| (joaat(&n), n))
    };
    let (target, short) = by_name.or_else(variant)?;
    let short = short.as_str();
    let row = t.by_model.get(&target)?;
    let mut l = look_row(t, a, ix, model, row, false).ok()?;
    let names: Vec<String> = ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| e.name.as_ref().map(|n| n.to_ascii_lowercase())).filter(|n| n.starts_with(&format!("{model}_head_"))).collect();
    let head = names.iter().filter(|n| !n.contains("_eye") && !n.contains("_earring")).min().cloned();
    let eyes = head.as_ref().and_then(|h| [format!("{}_eyes_full", h.trim_end_matches("_full")), format!("{}_eyes", h.trim_end_matches("_full"))].into_iter().find(|n| names.contains(n)));
    let was = l.outfits.first().and_then(|o| o.parts.iter().find(|p| p.slot == "head")).map(|p| crate::outfit::outfit_prefix(&p.drawable).unwrap_or_default()).unwrap_or_default();
    for o in &mut l.outfits {
        for p in &mut o.parts {
            match (p.slot.as_str(), &head, &eyes) {
                ("head", Some(h), _) => {
                    p.drawable = h.clone();
                    p.skins.clear();
                    p.found_by = "own";
                }
                ("eyes", Some(_), Some(e)) => {
                    p.drawable = e.clone();
                    p.found_by = "own";
                }
                _ => {}
            }
        }
    }
    l.note = Some(match head {
        Some(_) => format!("{model} is not in the outfit list under its own name. The outfits of the model named after \"{short}\" ({was}) are shown with this character's own head."),
        None => format!("{model} has no head of its own and is not in the outfit list. The outfits and head of the model named after \"{short}\" ({was}) are shown."),
    });
    Some(l)
}

/// For a character prefix the outfit list does not have, whose own parts
/// a listed model's wardrobe names (`Models::parts`): that model's outfits,
/// those that wear this prefix's parts first. `cs_roxy` is a head-less set
/// of clothes; `cs_nastydimez_roxyrashad` wears them.
fn look_by_parts(t: &Models, a: &ArchiveInfo, ix: &ArchiveIndex, model: &str) -> Option<Look> {
    let p = format!("{model}_");
    let own: HashSet<String> = ix
        .entries
        .iter()
        .filter(|e| e.rtype.as_deref() == Some("dr"))
        .filter_map(|e| e.name.as_ref().map(|n| n.to_ascii_lowercase()))
        .filter(|n| n.starts_with(&p) && crate::outfit::outfit_prefix(n).as_deref() == Some(model))
        .collect();
    let mut res: Vec<u64> = own.iter().flat_map(|n| t.parts.get(&crate::meta7a::asset_id(n)).cloned().unwrap_or_default()).collect();
    res.sort();
    res.dedup();
    // of the models in those resources, the one with most outfits wearing these parts
    let wears = |o: &Outfit| o.parts.iter().filter(|x| own.contains(&x.drawable)).count();
    // A model counts as wearing this set when an outfit wears the prefix's
    // own head, or (for a head-less set such as cs_roxy) two of its parts.
    // One shared accessory is not enough: a crowd ped wears
    // cs_mischa_piercing with a generic head.
    let heads: HashSet<&String> = own.iter().filter(|n| n.contains("_head_")).collect();
    let is_set = |o: &Outfit| if heads.is_empty() { wears(o) >= 2 } else { o.parts.iter().any(|x| heads.contains(&x.drawable)) };
    let names = model_names(&crate::export::game_root(a)).ok();
    let mut best: Option<(usize, Look, String)> = None;
    for (h, row) in t.by_model.iter().filter(|(_, r)| res.contains(&r.resource)) {
        let Ok(l) = look_row(t, a, ix, model, row, false) else { continue };
        let n = l.outfits.iter().filter(|o| is_set(o)).count();
        if n > 0 && best.as_ref().is_none_or(|b| n > b.0) {
            let name = names.and_then(|m| m.names.get(h).cloned()).unwrap_or_else(|| format!("ped_0x{h:08x}"));
            best = Some((n, l, name));
        }
    }
    let (_, mut l, other) = best?;
    // stable: the outfits that wear most of this prefix's parts first
    l.outfits.sort_by_key(|o| std::cmp::Reverse(wears(o)));
    l.model = model.to_string();
    l.note = Some(format!("{model} is not in the outfit list. Its parts are worn by the outfits of {other}, so that model's outfits are shown, those with these parts first."));
    Some(l)
}

/// For a character whose head no wardrobe names (`cs_lucia`): the outfits
/// of the character whose head has the nearest shape, worn with this
/// character's own head. Candidates are the heads of the same fit and
/// ethnicity code (`_head_fr1_lat_`) that a wardrobe names. The match
/// counts only when it stands out: the nearest head is at most 0.6 of the
/// candidates' median distance, among at least five. At 0.8 it linked
/// unrelated people (`cs_vern` to `cs_kai` at 0.69, `hc_jasonsmom` to a
/// hotel guest at 0.75), so the bar is where only `u_m_m_deal_dook` ->
/// `cs_dook` (0.52) passes. A shape match is evidence, not a stored link;
/// the look says so.
fn look_by_head_shape(t: &Models, a: &ArchiveInfo, ix: &ArchiveIndex, model: &str) -> Option<Look> {
    let drs = crate::outfit::dedupe(ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| {
        let n = e.name.as_ref()?.to_ascii_lowercase();
        (item_word(&n) == "head" && !n.contains("_eye") && !n.contains("_earring")).then_some((n, e))
    }));
    let own_p = format!("{model}_head_");
    let (own_name, own_e) = drs.iter().find(|(n, _)| n.starts_with(&own_p))?;
    // "_head_fr1_lat_"
    let i = own_name.find("_head_")?;
    let code: String = own_name[i..].split('_').take(4).collect::<Vec<_>>().join("_") + "_";
    let own = mesh_points(a, own_e)?;
    let own: Vec<[f32; 3]> = own.into_iter().step_by(5).collect();
    let cands: Vec<(&String, &&crate::scan::Entry)> = drs.iter().filter(|(n, _)| n.contains(&code) && !n.starts_with(&own_p) && (t.heads.contains_key(&joaat(n)) || t.heads.contains_key(&joaat(n.trim_end_matches("_full"))))).collect();
    let mut dist: Vec<(f32, &String)> = cands
        .par_iter()
        .filter_map(|(n, e)| {
            let p: Vec<[f32; 3]> = mesh_points(a, e)?.into_iter().step_by(2).collect();
            if p.len() < 1000 {
                return None;
            }
            let sum: f32 = own.iter().map(|q| p.iter().map(|r| (q[0] - r[0]).powi(2) + (q[1] - r[1]).powi(2) + (q[2] - r[2]).powi(2)).fold(f32::MAX, f32::min).sqrt()).sum();
            Some((sum / own.len() as f32, *n))
        })
        .collect();
    dist.sort_by(|x, y| x.0.total_cmp(&y.0));
    if dist.len() < 5 {
        return None;
    }
    let median = dist[dist.len() / 2].0;
    // the nearest head that stands out and whose character has outfits
    for (d, name) in dist.iter().take(1).filter(|(d, _)| *d <= 0.6 * median) {
        let Some(other) = crate::outfit::outfit_prefix(name) else { continue };
        let Ok(mut l) = look_uncached_listed(t, a, ix, &other) else { continue };
        let own_eyes = [format!("{}_eyes_full", own_name.trim_end_matches("_full")), format!("{}_eyes", own_name.trim_end_matches("_full"))].into_iter().find(|n| ix.entries.iter().any(|e| e.name.as_ref().is_some_and(|x| x.eq_ignore_ascii_case(n))));
        for o in &mut l.outfits {
            for p in &mut o.parts {
                if p.slot == "head" {
                    p.drawable = own_name.clone();
                    p.skins.clear();
                    p.found_by = "own";
                } else if p.slot == "eyes" {
                    if let Some(e) = &own_eyes {
                        p.drawable = e.clone();
                        p.found_by = "own";
                    }
                }
            }
        }
        l.other_variations.clear();
        l.note = Some(format!("{model} is not in the outfit data. Its head is nearest in shape to the head of {other} ({:.1} mm mean distance; the median over {} heads of that kind is {:.1} mm), so that character's outfits are shown with this head. A shape match, not a stored link.", d * 1000.0, dist.len(), median * 1000.0));
        l.model = model.to_string();
        return Some(l);
    }
    None
}

/// The outfits of a character by its stored links only (model name, or a
/// wardrobe that names its head).
fn look_uncached_listed(t: &Models, a: &ArchiveInfo, ix: &ArchiveIndex, model: &str) -> Result<Look, String> {
    if let Some(row) = t.by_model.get(&joaat(model)) {
        return look_row(t, a, ix, model, row, false);
    }
    rows_by_head(t, ix, model).into_iter().find_map(|row| look_row(t, a, ix, model, row, true).ok()).ok_or_else(|| "no outfits".to_string())
}

/// A look made of a character's own drawables, for models the outfit data
/// does not cover (`cs_lucia`, `cs_ghost`): one part per slot as
/// `outfit::outfit_options` picks them, its own hair placeholders, and the
/// whole skin meshes of the head's fit when it has clothes of its own but no
/// body. None for a head on its own (crowd heads): no data says what goes
/// under it.
fn own_look(a: &ArchiveInfo, ix: &ArchiveIndex, model: &str) -> Option<Look> {
    let opts = crate::outfit::outfit_options(ix, model);
    let names: HashSet<String> = ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| e.name.as_ref().map(|n| n.to_ascii_lowercase())).collect();
    let exists = |n: &str| names.contains(n);
    let mut parts: Vec<LookPart> = Vec::new();
    let plain = |slot: String, drawable: String, found_by: &'static str| LookPart { strands: is_strand_word(item_word(&drawable)), slot, drawable, item: String::new(), guid: None, tint: None, colour: None, colour_tag: None, hair_preset: None, preset_colours: vec![], preset: None, tex_remap: vec![], skins: vec![], found_by, category: None, layer: None, hides: vec![], hides_sub: vec![], hair_state: None, state_guids: vec![] };
    for o in opts.iter().filter(|o| o.default || is_strand_word(item_word(&o.name))) {
        let slot = slot_of(&o.name);
        // one hair, one beard …: the first of each
        if slot == "eyecaps" || parts.iter().any(|p| p.slot == slot && p.strands) {
            continue;
        }
        parts.push(plain(slot, o.name.clone(), "own"));
    }
    let head = parts.iter().find(|p| p.slot == "head")?;
    let fit = head.drawable.split('_').find(|t| crate::outfit::is_fit(t)).map(str::to_string);
    let has = |s: &str| parts.iter().any(|p| p.slot == s);
    // enough clothes of its own to stand on a bare body
    let clothed = has("full") || ((has("top") || has("outer")) && has("bottom"));
    if !clothed {
        return None;
    }
    for (slot, w) in [("body upper", "hand"), ("body lower", "feet")] {
        if !parts.iter().any(|p| p.slot == slot) {
            if let Some(n) = fit.as_deref().and_then(|f| whole_body(w, f, &exists)) {
                parts.push(plain(slot.to_string(), n, "default"));
            }
        }
    }
    let main: Vec<String> = parts.iter().filter(|p| ["full", "outer", "top", "bottom"].contains(&p.slot.as_str())).map(|p| pretty(item_word(&p.drawable))).collect();
    Some(Look { model: model.to_string(), resource: format!("{} (own parts: the outfit data does not cover this model)", a.id), fit, outfits: vec![Outfit { id: String::new(), outfit_type: String::new(), scale: 0.0, label: format!("own parts: {}", main.join(", ")), body_shape: None, parts, strand_colours: vec![], unresolved: vec![], not_shown: vec![], variations: vec![] }], other_variations: vec![], note: Some("The outfit data does not cover this model: it is shown in its own parts.".into()) })
}

/// The outfits of one model row. `own_head`: keep only the outfits whose
/// head is a drawable of `model` (the row was found through that head).
fn look_row(t: &Models, a: &ArchiveInfo, ix: &ArchiveIndex, model: &str, row: &ModelRow, own_head: bool) -> Result<Look, String> {
    static PRE: OnceLock<HashMap<u32, (String, String)>> = OnceLock::new();
    let work = crate::names::work_dir().ok_or("no work directory")?;
    let tu = tex_users(&work)?;
    let pre = PRE.get_or_init(|| crate::wardrobe::prefixes(ix.entries.iter().filter(|e| e.kind == crate::classify::Kind::RageObj).filter_map(|e| e.name.as_deref())));
    let e = ix.entries.iter().find(|e| e.off == row.resource).ok_or("wardrobe resource not in the index")?;
    let r = Resource::load_meta(&a.path, e).map_err(|e| format!("{e:#}"))?;
    let m = Meta::parse(&r).ok_or("wardrobe resource is not a meta resource")?;
    let vars = crate::wardrobe::variations(&m, pre, tu, &a.id);
    let by_id: HashMap<&str, &Variation> = vars.iter().map(|v| (v.id.as_str(), v)).collect();
    let names: HashSet<String> = ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| e.name.as_ref().map(|n| n.to_ascii_lowercase())).collect();
    let exists = |n: &str| names.contains(n);
    // An item's asset GUID is the name id (`meta7a::asset_id`) of its own
    // drawable, `_full` included (`shortswork_mr1_001_regular_mw_long_full`),
    // as for hair assets (`hairlink`).
    let by_guid: HashMap<u64, &String> = names.iter().map(|n| (crate::meta7a::asset_id(n), n)).collect();
    // an outfit with everything it includes (head set, body set, brows …)
    fn collect<'a>(v: &'a Variation, by_id: &HashMap<&str, &'a Variation>, seen: &mut HashSet<String>, out: &mut Vec<&'a Variation>) {
        if !seen.insert(v.id.clone()) {
            return;
        }
        out.push(v);
        for i in &v.includes {
            if let Some(n) = by_id.get(i.as_str()) {
                collect(n, by_id, seen, out);
            }
        }
    }
    let mut fit: Option<String> = None;
    let mut flat: Vec<(&(u32, u32, f32), Vec<&Variation>)> = Vec::new();
    for o in &row.outfits {
        let Some(v) = by_id.get(format!("{:08x}", o.0).as_str()) else { continue };
        let mut vs = Vec::new();
        collect(v, &by_id, &mut HashSet::new(), &mut vs);
        if fit.is_none() {
            fit = vs.iter().flat_map(|v| &v.parts).filter_map(|p| p.named.as_deref()).filter(|n| item_word(n) == "head").find_map(|n| n.split('_').find(|t| crate::outfit::is_fit(t)).map(str::to_string));
        }
        flat.push((o, vs));
    }
    if flat.is_empty() {
        return Err(format!("none of the {} outfits of {model} is in its wardrobe resource", row.outfits.len()));
    }
    // The model's own head, for outfits whose head item does not resolve
    // (`cs_maleek_rivers`: the item names nothing found, and the texture
    // link then picked another head's eyes).
    let self_head = names.iter().filter(|n| n.starts_with(&format!("{model}_head_")) && !n.contains("_eye") && !n.contains("_earring")).min().cloned();
    if fit.is_none() {
        fit = self_head.as_deref().and_then(|n| n.split('_').find(|t| crate::outfit::is_fit(t)).map(str::to_string));
    }
    // garment preset colours (the clothing material library of data.rpf)
    static PRESETS: OnceLock<Option<crate::matlib::Index>> = OnceLock::new();
    let presets = PRESETS.get_or_init(|| crate::matlib::Index::load_or_build(&crate::export::Workspace::new(crate::export::game_root(a), &work)).ok()).as_ref();
    // Items whose own textures find no model: the answer of another
    // wardrobe's item with the same asset GUID. Built only when needed.
    // The wardrobe's variations that no listed outfit reaches (alternative
    // garments, unused sets), each on its own. Not when the wardrobe was
    // found through this character's head: it is another model's.
    let reached: HashSet<&str> = flat.iter().flat_map(|(_, vs)| vs.iter()).map(|v| v.id.as_str()).collect();
    // Nor when another listed model keeps its outfits in the same resource.
    let shared = t.by_model.values().filter(|r| r.resource == row.resource).count() > 1;
    let extra: Vec<&Variation> = if own_head || shared { vec![] } else { vars.iter().filter(|v| !reached.contains(v.id.as_str())).collect() };
    let n_listed = flat.len();
    let mut flat: Vec<(Option<&(u32, u32, f32)>, Vec<&Variation>)> = flat.into_iter().map(|(o, vs)| (Some(o), vs)).collect();
    flat.extend(extra.into_iter().map(|v| (None, vec![v])));
    let needs_assets = flat.iter().flat_map(|(_, vs)| vs.iter()).flat_map(|v| &v.parts).any(|p| p.guid.is_some() && choose(p, fit.as_deref(), &exists).is_none());
    let assets = if needs_assets { asset_drawables(a, ix).ok() } else { None };
    let mut outfits = Vec::new();
    let mut other_variations = Vec::new();
    for (o, vs) in flat {
        let listed = o.is_some();
        let mut parts: Vec<LookPart> = Vec::new();
        let mut unresolved = Vec::new();
        let mut not_shown = Vec::new();
        let mut body_shape = None;
        let mut strand_colours: Vec<(String, String)> = Vec::new();
        for v in &vs {
            for c in v.parts.iter().filter_map(|p| p.strand_colour.clone()) {
                if !strand_colours.iter().any(|x| x.0 == c.0) {
                    strand_colours.push(c);
                }
            }
            if v.tags.values().flatten().any(|w| w == "BodyShapePreset") {
                body_shape = v.tags.iter().filter(|(_, w)| !w.iter().any(|x| x == "BodyShapePreset")).flat_map(|(_, w)| w.first()).next().cloned();
            }
            for p in &v.parts {
                let by_asset = || Some((assets?.pick(p.guid.as_deref()?, fit.as_deref(), &exists)?, "asset id"));
                // The drawable the GUID names, first: the texture and name
                // matches below pick a sibling variant (open / closed
                // jacket, hood up / down, another piercing) for about one
                // item in eight. Not for a head: its GUID names the base
                // head template (`head_mr1_base_000_m_full`); the face is
                // the head the item names.
                let own = || {
                    let n = by_guid.get(&u64::from_str_radix(p.guid.as_deref()?, 16).ok()?)?;
                    (slot_of(n) != "head" && !n.ends_with("_fp") && !n.contains("_fp_")).then(|| ((*n).clone(), "asset id (name)"))
                };
                let Some((drawable, found_by)) = own().or_else(|| choose(p, fit.as_deref(), &exists)).or_else(by_asset) else {
                    // items without textures or names are settings, not parts;
                    // a garment that names a model by GUID is a part whose
                    // model was not found
                    let garment = p.guid.as_ref().and(p.category.as_deref()).and_then(garment_category);
                    if let Some(why) = not_shown_reason(p, &by_guid) {
                        not_shown.push(format!("{} ({why})", p.item));
                    } else if let Some(g) = garment {
                        unresolved.push(format!("{} ({g}: its model is not found)", p.item));
                    } else if p.named.is_some() || !p.candidates.is_empty() {
                        unresolved.push(format!("{} ({})", p.item, p.category.clone().unwrap_or_default()));
                    }
                    continue;
                };
                let mut drawable = drawable;
                let slot = slot_of(&drawable);
                // The skin meshes come cut to the garment (`hand_fr1_000_tshirt_a02`);
                // which cut goes with which garment is not decoded, so the whole
                // one is used and the clothes cover it.
                if slot.starts_with("body ") && !slot.contains("hair") {
                    let w = item_word(&drawable);
                    if let Some(whole) = fit.as_deref().and_then(|f| whole_body(w, f, &exists)) {
                        drawable = whole;
                    }
                }
                // eye caps and hidden-polygon helpers are not shown
                if slot == "eyecaps" || slot.contains("hiddenpoly") || parts.iter().any(|x| x.drawable == drawable) {
                    continue;
                }
                let slot_is_hair = slot == "hair";
                parts.push(LookPart { strands: is_strand_word(item_word(&drawable)), slot, drawable, item: p.item.clone(), guid: p.guid.clone(), tint: p.tint.clone(), colour: p.colour, colour_tag: p.colour_tag, hair_preset: p.hair_preset.clone(), preset: p.preset.clone(), preset_colours: p.preset.as_deref().and_then(|h| presets?.colours(h)).map(|v| v.into_iter().map(|c| c.map(|c| [c[0], c[1], c[2]])).collect()).unwrap_or_default(), tex_remap: p.tex_remap.clone(), skins: p.skins.clone(), found_by, category: p.category.clone(), layer: p.layer.clone(), hides: p.hides.clone(), hides_sub: p.hides_sub.clone(), hair_state: None, state_guids: if slot_is_hair { p.state_guids.clone() } else { vec![] } });
            }
        }
        let plain = |slot: &str, drawable: String, found_by: &'static str| LookPart { slot: slot.into(), drawable, item: String::new(), strands: false, guid: None, tint: None, colour: None, colour_tag: None, hair_preset: None, preset_colours: vec![], preset: None, tex_remap: vec![], skins: vec![], found_by, category: None, layer: None, hides: vec![], hides_sub: vec![], hair_state: None, state_guids: vec![] };
        // another variation of the wardrobe: only what it holds itself
        if !listed {
            let main: Vec<String> = parts.iter().filter(|p| ["full", "outer", "top", "bottom"].contains(&p.slot.as_str())).map(|p| pretty(item_word(&p.drawable))).collect();
            let v = vs[0];
            if !parts.is_empty() || !unresolved.is_empty() || !not_shown.is_empty() {
                other_variations.push(Outfit { id: v.id.clone(), outfit_type: String::new(), scale: 0.0, label: if main.is_empty() { format!("variation {}", v.id) } else { main.join(", ") }, body_shape, parts, strand_colours, unresolved, not_shown, variations: vec![v.id.clone()] });
            }
            continue;
        }
        let o = o.expect("listed");
        if let (false, Some(h)) = (parts.iter().any(|p| p.slot == "head"), &self_head) {
            parts.push(plain("head", h.clone(), "own"));
        }
        // The eyes that go with the head (`<head>_eyes_full`): the item's
        // textures are shared by every eye mesh, so the texture link can
        // pick another head's.
        let head_eyes = parts.iter().find(|p| p.slot == "head").and_then(|h| {
            let stem = h.drawable.strip_suffix("_full").unwrap_or(&h.drawable);
            [format!("{stem}_eyes_full"), format!("{stem}_eyes")].into_iter().find(|n| exists(n))
        });
        if let Some(n) = head_eyes {
            match parts.iter_mut().find(|p| p.slot == "eyes") {
                Some(e) if e.drawable != n => {
                    e.drawable = n;
                    e.found_by = "head";
                }
                Some(_) => {}
                None => parts.push(plain("eyes", n, "head")),
            }
        }
        // Headwear: the hair item's state for it names the hair asset worn
        // under it (a flattened or shortened one, or the hidden one).
        let wanted: Vec<(&'static str, String)> = parts.iter().flat_map(|p| hair_states_under(item_word(&p.drawable)).iter().map(|s| (*s, p.drawable.clone()))).collect();
        for h in parts.iter_mut().filter(|p| p.slot == "hair") {
            let found = wanted.iter().find_map(|(s, by)| {
                let key = format!("{:08x}", joaat(s));
                h.state_guids.iter().find(|g| g.0 == key).map(|g| (*s, by.clone(), g.1.clone()))
            });
            if let Some((s, by, g)) = found {
                h.hair_state = Some(format!("{s} (under {by})"));
                h.guid = Some(g);
            }
        }
        // No body set resolved (it is in a shared resource for many
        // characters): the whole skin meshes of the fit.
        if parts.iter().any(|p| p.slot == "head") {
            for (slot, w) in [("body upper", "hand"), ("body lower", "feet")] {
                if !parts.iter().any(|p| p.slot == slot) {
                    if let Some(n) = fit.as_deref().and_then(|f| whole_body(w, f, &exists)) {
                        parts.push(plain(slot, n, "default"));
                    }
                }
            }
        }
        let main: Vec<String> = parts.iter().filter(|p| ["full", "outer", "top", "bottom"].contains(&p.slot.as_str())).map(|p| pretty(item_word(&p.drawable))).collect();
        outfits.push(Outfit {
            id: format!("{:08x}", o.0),
            outfit_type: format!("{:08x}", o.1),
            scale: o.2,
            label: if main.is_empty() { format!("outfit {}", outfits.len() + 1) } else { main.join(", ") },
            body_shape,
            parts,
            strand_colours,
            unresolved,
            not_shown,
            variations: vs.iter().map(|v| v.id.clone()).collect(),
        });
    }
    debug_assert_eq!(outfits.len(), n_listed);
    if own_head {
        let p = format!("{model}_");
        outfits.retain(|o| o.parts.iter().any(|x| x.slot == "head" && x.drawable.starts_with(&p)));
        if outfits.is_empty() {
            return Err(format!("{model} is not in the game's outfit list, and no listed outfit of the wardrobe that names its head wears that head"));
        }
    }
    Ok(Look { model: model.to_string(), resource: format!("{}@{:x}", a.id, row.resource), fit, outfits, other_variations, note: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_names_from_scene_handles() {
        use crate::cutscene::{Catalog, FileObject, SceneDict, SceneFile, CATALOG_VERSION};
        use crate::meta7a::asset_id;
        let d = SceneDict { name: "cin@x_p1_t01".into(), id: "a.rpf@10".into(), clips: ["boss_face", "guard2", "lucia"].map(String::from).to_vec(), sections: 0, section_ids: vec![], adopted_sections: 0, streamed: vec![], unjoined: vec![], windows_streamed: 0, windows_missing: 0 };
        // the scene file records the boss's model; the guard's is not recorded
        let f = SceneFile { id: "a.rpf@20".into(), states: vec![], objects: vec![FileObject { key: joaat("boss"), kind: "ped".into(), model: Some(asset_id("cs_boss")) }] };
        let c = Catalog { version: CATALOG_VERSION, archive: "a.rpf".into(), dicts: vec![d], files: vec![f] };
        let listed: HashSet<u32> = ["cs_boss", "cs_guard", "player_story000_00"].iter().map(|n| joaat(n)).collect();
        let (mut names, mut uncertain) = (BTreeMap::new(), std::collections::BTreeSet::new());
        names.insert(joaat("player_story000_00"), "player_story000_00".to_string());
        names_from_scenes(&c, |h| listed.contains(&h), &mut names, &mut uncertain);
        assert_eq!(names.get(&joaat("cs_boss")).map(String::as_str), Some("cs_boss"));
        // `guard2` without its number
        assert_eq!(names.get(&joaat("cs_guard")).map(String::as_str), Some("cs_guard"));
        assert_eq!(uncertain.iter().copied().collect::<Vec<_>>(), vec![joaat("cs_guard")]);
        // `lucia` names no listed model
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn model_names_from_text_and_hashes() {
        let mut s = HashSet::new();
        text_name_stems("Talk to Phil Cassidy. Then meet Mandy at the bar", &mut s);
        for want in ["phil", "cassidy", "phil_cassidy", "mandy", "talk", "then"] {
            assert!(s.contains(want), "{want}");
        }
        // a sentence end and a lower-case word both end a run
        assert!(!s.contains("cassidy_then") && !s.contains("mandy_at"));
        assert!(is_model_name("cs_mandy") && is_model_name("a_f_o_beach_00") && is_model_name("crb_eagle_bald_m_00"));
        assert!(!is_model_name("contact") && !is_model_name("a_x_o_beach"));
        assert_eq!(model_hash("ped_0x7fd5440d"), 0x7fd5_440d);
        assert_eq!(model_hash("0x0007f78b"), 0x7f78b);
        assert_eq!(model_hash("cs_mandy"), joaat("cs_mandy"));
    }

    #[test]
    fn body_layer_by_guid() {
        // the GUID of `cs_raina`'s nude-outfit item, stored under a category
        // other than the body layer's
        assert!(body_layer_ids().contains(&0xf25c_e576_de83_479d));
        assert!(body_layer_ids().contains(&crate::meta7a::asset_id("bodygenitalsgen_mr1_000_regular_ws00_full_alt_01")));
        assert!(!body_layer_ids().contains(&crate::meta7a::asset_id("shortswork_mr1_001_regular_mw_long_full")));
    }

    #[test]
    fn swimwear_and_garment_categories() {
        assert_eq!(slot_of("swimsuitplunge_fr1_000_slim_full"), "full");
        assert_eq!(slot_of("bodysuitbaby_mi1_000_regular_full"), "full");
        assert_eq!(slot_of("basebottomssports_fr1_000_slim_full"), "bottom");
        assert_eq!(garment_category("d7c6badd"), Some("shoes"));
        assert_eq!(garment_category("skirts"), Some("skirts"));
        assert_eq!(garment_category("864b03ae"), None);
    }

    #[test]
    fn asset_table_prefers_the_fit_then_the_count() {
        let mut t = AssetDrawables::default();
        t.by_guid.insert("ab".into(), vec![("shoes_mr1_000_low_fh_full".into(), 9), ("shoes_fr1_000_low_fh_full".into(), 2), ("shoes_fr1_001_low_fh_full".into(), 5), ("gone_fr1_000_full".into(), 50)]);
        let exists = |n: &str| !n.starts_with("gone");
        assert_eq!(t.pick("ab", Some("fr1"), &exists).as_deref(), Some("shoes_fr1_001_low_fh_full"));
        assert_eq!(t.pick("ab", None, &exists).as_deref(), Some("shoes_mr1_000_low_fh_full"));
        assert_eq!(t.pick("cd", Some("fr1"), &exists), None);
    }

    #[test]
    fn slots_and_words() {
        assert_eq!(item_word("cs_ali_hairdreadlocks_fr1_000_curly_l05_f02_sb_n_full"), "hairdreadlocks");
        assert_eq!(slot_of("cs_ali_head_fr1_afr_000_m_eyes_full"), "eyes");
        assert_eq!(slot_of("cs_ali_head_fr1_afr_000_m_full"), "head");
        assert_eq!(slot_of("hand_fr1_000_full"), "body upper");
        assert_eq!(slot_of("feet_fr1_000_full"), "body lower");
        assert_eq!(slot_of("jackettrack_fr1_001_regular_h_hl_long_full"), "outer");
        assert_eq!(slot_of("shoessneakers_fr1_031_mid_fh_full"), "feet");
        assert_eq!(slot_of("teethaccsgrills_mr1_004_regular_ws00_full"), "teeth accessory");
        assert_eq!(slot_of("teethmodular_mr1_000_regular_ws00_full"), "teeth");
        assert_eq!(slot_of("braceletchain_fr1_004_small_ws00_full"), "bracelet");
        assert_eq!(slot_of("brabalcony_fr1_000_regular_ws00_full"), "top");
        assert_eq!(slot_of("browsangle_fr1_000_medium_n_full"), "brows");
        assert!(is_strand_word("hairafro") && !is_strand_word("hand"));
        assert_eq!(pretty("jackettrack"), "jacket track");
        assert_eq!(pretty("tshirtcrewneck"), "tshirt crewneck");
    }

    #[test]
    fn choose_prefers_name_then_fit() {
        let mut p = Part { item: "1".into(), category: None, name: None, drawable: None, tex_remap: vec![], tags: Default::default(), tint: None, named: None, guid: None, colour: None, colour_tag: None, hair_preset: None, strand_colour: None, preset: None, state: String::new(), skins: vec![], candidates: Default::default(), layer: None, hides: vec![], hides_sub: vec![], state_guids: vec![] };
        p.candidates.insert("hand_fc1_000_full".into(), 3);
        p.candidates.insert("hand_fr1_000_full".into(), 3);
        p.candidates.insert("hand_fr1_000_nude_cull".into(), 3);
        p.candidates.insert("hand_fr1_000_tshirt_a02".into(), 3);
        let all = |_: &str| true;
        assert_eq!(choose(&p, Some("fr1"), &all).unwrap().0, "hand_fr1_000_nude_cull");
        let no_cull = |n: &str| !n.contains("_cull");
        assert_eq!(choose(&p, Some("fr1"), &no_cull).unwrap().0, "hand_fr1_000_full");
        p.named = Some("x_head_fr1_000_full".into());
        assert_eq!(choose(&p, Some("fr1"), &all).unwrap(), ("x_head_fr1_000_full".to_string(), "name"));
        // strand placeholders are found by name only
        let mut h = p.clone();
        h.named = None;
        h.candidates = [("browsflat_mi1_000_short_n_full".to_string(), 2)].into_iter().collect();
        assert!(choose(&h, Some("fr1"), &all).is_none());
    }
}
