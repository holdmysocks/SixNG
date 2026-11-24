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
//! Which drawable an item uses: the name the item's tags give (heads, hair,
//! brows), else the drawable that shares most of the item's textures, where
//! ties go to the model's own fit (`fr1`) and to `_full` meshes. Items whose
//! textures are in no drawable get no part.
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

const CACHE_VERSION: u32 = 4;
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
            for &o in &vars {
                if let Some(id) = m.scalars(VARIATION, o, &[]).get(&k).and_then(|x| x.as_u64()) {
                    v.push((false, id as u32, e.off));
                }
            }
            if !vars.is_empty() {
                for o in m.objects(0xf795_e8ba) {
                    let Some(p) = m.object(0xf795_e8ba, o, 1) else { continue };
                    for t in p[f(0x12f8_5293)].as_array().into_iter().flatten().filter(|t| matches!(t["Key"].as_u64(), Some(0x4bfd_27dd) | Some(0x023e_5a2f))) {
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
    Models { version: CACHE_VERSION, by_model, heads, aliases }
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
            let _ = std::fs::write(&cache, b);
        }
        Ok(t)
    })
    .as_ref()
    .map_err(|e| e.clone())
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
            let _ = std::fs::write(&cache, b);
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
    /// Items of the outfit that resolved to no drawable (item id, slot hash).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
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
    crate::outfit::slot_word(w)
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
    Some((best.0.clone(), "textures"))
}

/// The whole skin mesh of a fit (`hand` = upper body, `feet` = lower body):
/// `_full`, else the nude one (male fits only have `nude_cull`).
fn whole_body(word: &str, fit: &str, exists: &dyn Fn(&str) -> bool) -> Option<String> {
    ["full", "nude_full", "nude_cull"].iter().map(|s| format!("{word}_{fit}_000_{s}")).find(|n| exists(n))
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
    let r = look_uncached(game_root, a, ix, &key);
    cache.lock().unwrap().insert(key, r.clone());
    r
}

fn look_uncached(game_root: &Path, a: &ArchiveInfo, ix: &ArchiveIndex, model: &str) -> Result<Look, String> {
    let t = models(game_root)?;
    if let Some(row) = t.by_model.get(&joaat(model)) {
        return look_row(a, ix, model, row, false);
    }
    // listed under another model name: the models whose wardrobe names this
    // character's head, and of their outfits those that wear it
    let rows = rows_by_head(t, ix, model);
    let mut out: Option<Look> = None;
    let mut last_err = None;
    for row in rows {
        match look_row(a, ix, model, row, true) {
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
    // no outfit data at all (cutscene-only models): its own parts
    own_look(a, ix, model).ok_or_else(|| last_err.unwrap_or_else(|| format!("{model} is not in the game's outfit list, and no wardrobe names its head")))
}

/// Vertex positions of a drawable's largest mesh.
fn mesh_points(a: &ArchiveInfo, e: &crate::scan::Entry) -> Option<Vec<[f32; 3]>> {
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
    let [target] = t.aliases.get(&joaat(short))?.as_slice() else { return None };
    let row = t.by_model.get(target)?;
    let mut l = look_row(a, ix, model, row, false).ok()?;
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
        Some(_) => format!("{model} is not in the outfit list under its own name. The ped model table names one model after \"{short}\" ({was}); its outfits are shown with this character's own head."),
        None => format!("{model} has no head of its own and is not in the outfit list. The ped model table names one model after \"{short}\" ({was}); that model's outfits and head are shown."),
    });
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
        return look_row(a, ix, model, row, false);
    }
    rows_by_head(t, ix, model).into_iter().find_map(|row| look_row(a, ix, model, row, true).ok()).ok_or_else(|| "no outfits".to_string())
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
    let plain = |slot: String, drawable: String, found_by: &'static str| LookPart { strands: is_strand_word(item_word(&drawable)), slot, drawable, item: String::new(), guid: None, tint: None, colour: None, colour_tag: None, preset_colours: vec![], preset: None, tex_remap: vec![], skins: vec![], found_by };
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
    Some(Look { model: model.to_string(), resource: format!("{} (own parts: the outfit data does not cover this model)", a.id), fit, outfits: vec![Outfit { id: String::new(), outfit_type: String::new(), scale: 0.0, label: format!("own parts: {}", main.join(", ")), body_shape: None, parts, unresolved: vec![] }], note: Some("The outfit data does not cover this model: it is shown in its own parts.".into()) })
}

/// The outfits of one model row. `own_head`: keep only the outfits whose
/// head is a drawable of `model` (the row was found through that head).
fn look_row(a: &ArchiveInfo, ix: &ArchiveIndex, model: &str, row: &ModelRow, own_head: bool) -> Result<Look, String> {
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
    // garment preset colours (the clothing material library of data.rpf)
    static PRESETS: OnceLock<Option<crate::matlib::Index>> = OnceLock::new();
    let presets = PRESETS.get_or_init(|| crate::matlib::Index::load_or_build(&crate::export::Workspace::new(crate::export::game_root(a), &work)).ok()).as_ref();
    let mut outfits = Vec::new();
    for (o, vs) in flat {
        let mut parts: Vec<LookPart> = Vec::new();
        let mut unresolved = Vec::new();
        let mut body_shape = None;
        for v in &vs {
            if v.tags.values().flatten().any(|w| w == "BodyShapePreset") {
                body_shape = v.tags.iter().filter(|(_, w)| !w.iter().any(|x| x == "BodyShapePreset")).flat_map(|(_, w)| w.first()).next().cloned();
            }
            for p in &v.parts {
                let Some((drawable, found_by)) = choose(p, fit.as_deref(), &exists) else {
                    // items without textures or names are settings, not parts
                    if p.named.is_some() || !p.candidates.is_empty() {
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
                parts.push(LookPart { strands: is_strand_word(item_word(&drawable)), slot, drawable, item: p.item.clone(), guid: p.guid.clone(), tint: p.tint.clone(), colour: p.colour, colour_tag: p.colour_tag, preset: p.preset.clone(), preset_colours: p.preset.as_deref().and_then(|h| presets?.colours(h)).map(|v| v.into_iter().map(|c| c.map(|c| [c[0], c[1], c[2]])).collect()).unwrap_or_default(), tex_remap: p.tex_remap.clone(), skins: p.skins.clone(), found_by });
            }
        }
        let plain = |slot: &str, drawable: String, found_by: &'static str| LookPart { slot: slot.into(), drawable, item: String::new(), strands: false, guid: None, tint: None, colour: None, colour_tag: None, preset_colours: vec![], preset: None, tex_remap: vec![], skins: vec![], found_by };
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
            unresolved,
        });
    }
    if own_head {
        let p = format!("{model}_");
        outfits.retain(|o| o.parts.iter().any(|x| x.slot == "head" && x.drawable.starts_with(&p)));
        if outfits.is_empty() {
            return Err(format!("{model} is not in the game's outfit list, and no listed outfit of the wardrobe that names its head wears that head"));
        }
    }
    Ok(Look { model: model.to_string(), resource: format!("{}@{:x}", a.id, row.resource), fit, outfits, note: None })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut p = Part { item: "1".into(), category: None, name: None, drawable: None, tex_remap: vec![], tags: Default::default(), tint: None, named: None, guid: None, colour: None, colour_tag: None, preset: None, state: String::new(), skins: vec![], candidates: Default::default() };
        p.candidates.insert("hand_fc1_000_full".into(), 3);
        p.candidates.insert("hand_fr1_000_full".into(), 3);
        p.candidates.insert("hand_fr1_000_nude_cull".into(), 3);
        p.candidates.insert("hand_fr1_000_tshirt_a02".into(), 3);
        let all = |_: &str| true;
        assert_eq!(choose(&p, Some("fr1"), &all).unwrap().0, "hand_fr1_000_full");
        p.named = Some("x_head_fr1_000_full".into());
        assert_eq!(choose(&p, Some("fr1"), &all).unwrap(), ("x_head_fr1_000_full".to_string(), "name"));
        // strand placeholders are found by name only
        let mut h = p.clone();
        h.named = None;
        h.candidates = [("browsflat_mi1_000_short_n_full".to_string(), 2)].into_iter().collect();
        assert!(choose(&h, Some("fr1"), &all).is_none());
    }
}
