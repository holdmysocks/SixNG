//! Head skins: face textures for ped heads.
//!
//! Head drawables bind no textures; the game picks the face from material
//! presets in the packs.rpf meta resources (0x7a). A preset (`f795e8ba`) has
//! a tag map (`12f85293`, key -> u32 list). The tag `4bfd27dd` lists the
//! drawables (joaat of the drawable name) the preset applies to, so a head
//! listed there uses that preset. Its "base" parameter (`3eaddcd7` =
//! joaat("base")) holds the face albedo in slot `ead733fc` and a second
//! texture (normal map) in slot `d689b17f`.
//!
//! Heads that no preset lists (most generic `a_head_base_*`, crowd `hc_*`
//! heads whose presets have empty slots) get an automatic skin: a face from
//! the linked heads of the same sex, body type and ethnicity code (`fr1_lat`
//! in `head_fr1_lat_018_m_full`), preferring the same age letter.
//!
//! A preset's asset also has a keyed texture list (`b70a22c1`: key ->
//! `AssetName`), and a preset inherits from the preset its `TemplateName`
//! names (the male and female base heads, `c17f02fe` and `a3bfa046`, for
//! nearly all). Three keys are in use ([`EXTRA_KEYS`]); the table keeps
//! them per face (`extras`), the preset's own entry before its template's.
//!
//! The table is built once from the packs.rpf index (a few seconds) and
//! cached as `<work>/index/head_skins.json`.

use crate::meta7a::{joaat, Meta};
use crate::resource::Resource;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::OnceLock;

const PRESET: u32 = 0xf795_e8ba;
const TAG_DRAWABLE: u32 = 0x4bfd_27dd;
const PARAM_BASE: u32 = 0x44e2_1c90;
const CACHE_VERSION: u32 = 4;

/// Keys of the keyed texture list of a face preset, with what the texture
/// is. The key names are not recovered; the descriptions are from the
/// decoded textures.
pub const EXTRA_KEYS: &[(u32, &str)] = &[
    (0xe125_85d1, "wrinkle region masks: a 3-slice array, each colour channel of each slice one face region (9 regions: brow, eye, cheek and mouth areas, left and right)"),
    (0x8585_8f5e, "face region mask (red: brows, eyelids, nose bridge, chin; green: cheeks and neck; blue: mouth area)"),
    (0x2633_887d, "one more face map per sex or per named character; none of these is in the game files"),
];

/// What the texture under a keyed-list key is, when the key is known.
pub fn extra_role(key: u32) -> Option<&'static str> {
    EXTRA_KEYS.iter().find(|k| k.0 == key).map(|k| k.1)
}

/// Textures of one face preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Skin {
    pub albedo: u64,
    /// Second texture of the "base" parameter (a normal map), 0 if none.
    pub normal: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HeadSkins {
    version: u32,
    /// joaat(head drawable name) -> face presets that list it.
    pub by_head: BTreeMap<u32, Vec<Skin>>,
    /// Linked faces by head code (`fr1_lat`), with the age letter.
    pub library: BTreeMap<String, Vec<(char, Skin)>>,
    /// Face albedo -> the keyed textures of its preset (key, texture), own
    /// entries before the template's.
    #[serde(default)]
    pub extras: BTreeMap<u64, Vec<(u32, u64)>>,
    /// joaat(head drawable name) -> the head whose faces it shares: a
    /// `cs_<x>` head no preset lists takes the face of the head of the one
    /// model the ped model table gives character name `<x>`
    /// (`cs_lucia_head_fr1_lat_000_y_full` -> Lucia's player head), when both
    /// heads have the same head code ([`same_person_codes`]).
    #[serde(default)]
    pub character: BTreeMap<u32, String>,
}

/// Whether two heads of one character can share a face: the same sex, body
/// type and ethnicity code, and no step into or out of the old (`o`) or kid
/// (`k`, `i`, `b`) ages. `y` and `m` mix: Lucia's player head is `m`, her
/// cutscene head `y`.
pub fn same_person_codes(a: &str, b: &str) -> bool {
    let (Some((ca, aa)), Some((cb, ab))) = (head_code(a), head_code(b)) else { return false };
    let adult = |c: char| matches!(c, 'y' | 'm');
    ca == cb && (aa == ab || (adult(aa) && adult(ab)))
}

/// The `character` links: `cs_` heads with no preset of their own -> the
/// head of the same character, by the ped model table's character names
/// (`aliases`: joaat(character name) -> models). `heads` are the head
/// drawable names; `has_preset` says whether a preset lists a head.
fn character_links(heads: &[&str], aliases: &BTreeMap<u32, Vec<u32>>, has_preset: impl Fn(&str) -> bool) -> BTreeMap<u32, String> {
    let mut out = BTreeMap::new();
    for &h in heads.iter().filter(|h| h.starts_with("cs_") && !has_preset(h)) {
        let Some((prefix, _)) = h.split_once("_head_") else { continue };
        let short = &prefix[3..];
        // exactly one model carries the character name, and it is another model
        let Some(&[model]) = aliases.get(&joaat(short)).map(Vec::as_slice) else { continue };
        if model == joaat(prefix) {
            continue;
        }
        let donors: Vec<&str> = heads.iter().copied().filter(|d| d.split_once("_head_").is_some_and(|(p, _)| joaat(p) == model) && has_preset(d) && same_person_codes(h, d)).collect();
        if let [d] = donors[..] {
            out.insert(joaat(h), d.to_string());
        }
    }
    out
}


/// A preset's own keyed textures with those of its template chain; a key
/// keeps its first texture.
fn resolve_extras(own: &[(u32, u64)], template: u32, defs: &HashMap<u32, (u32, Vec<(u32, u64)>)>) -> Vec<(u32, u64)> {
    let mut out = own.to_vec();
    let mut t = template;
    for _ in 0..4 {
        let Some((parent, list)) = defs.get(&t) else { break };
        for x in list {
            if !out.iter().any(|o| o.0 == x.0) {
                out.push(*x);
            }
        }
        t = *parent;
    }
    out.sort();
    out
}

/// Choice made for one head.
#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub skin: Skin,
    /// "linked" (a preset lists this head), "character" (the face of the
    /// same character's other head, named in `group`) or "auto".
    pub source: &'static str,
    /// Code the automatic choice matched (`fr1_lat`, `f`, or `any`).
    pub group: String,
    pub index: usize,
    pub count: usize,
}

/// Head code of a drawable name: (sex + body type + ethnicity, age letter),
/// e.g. `cs_jada_watts_head_fr1_afr_000_y_full` -> ("fr1_afr", 'y').
pub fn head_code(name: &str) -> Option<(String, char)> {
    let p: Vec<&str> = name.split('_').collect();
    let i = p.iter().position(|s| *s == "head")?;
    let rest = &p[i + 1..];
    // head_<sex/body>_<eth>_<nnn>_<age> or a_head_base_<sex/body>_<eth>_...
    let rest = if rest.first() == Some(&"base") { &rest[1..] } else { rest };
    let body = rest.first()?;
    if body.len() != 3 || !matches!(body.as_bytes()[0], b'f' | b'm') {
        return None;
    }
    let eth = rest.get(1).filter(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_lowercase())).copied();
    let age = rest.iter().skip(2).find(|s| s.len() == 1).and_then(|s| s.chars().next()).unwrap_or('m');
    Some((match eth {
        Some(e) => format!("{body}_{e}"),
        None => body.to_string(),
    }, age))
}

/// True for a head drawable (not eyes, lashes or other head parts).
pub fn is_head(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    (n.starts_with("head_") || n.contains("_head_")) && !["eye", "lash", "teeth", "tongue", "hair", "brow", "beard", "tattoo"].iter().any(|k| n.contains(k)) && head_code(&n).is_some()
}

fn build(packs: &Path, ix: &crate::scan::ArchiveIndex) -> HeadSkins {
    let names: HashMap<u32, &str> = ix.entries.iter().filter_map(|e| e.name.as_deref()).map(|n| (joaat(n), n)).collect();
    // per preset: the heads it lists and its face, and its keyed textures under its own name
    type Found = (Vec<(u32, Skin, u32, Vec<(u32, u64)>)>, Vec<(u32, u32, Vec<(u32, u64)>)>);
    let found: Vec<Found> = ix
        .entries
        .par_iter()
        .filter(|e| e.sub == "7a")
        .map(|e| {
            let (mut v, mut defs) = (Vec::new(), Vec::new());
            let Ok(r) = Resource::load_meta(packs, e) else { return (v, defs) };
            let Some(m) = Meta::parse(&r) else { return (v, defs) };
            // Field keys as the meta decoder names them (hex unless known).
            let k = |h: u32| crate::meta7a::field_name(h);
            for o in m.objects(PRESET) {
                let Some(p) = m.object(PRESET, o, 3) else { continue };
                let heads: Vec<u32> = p[k(0x12f8_5293).as_str()]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|t| t[k(0x6098_a50e).as_str()].as_u64() == Some(TAG_DRAWABLE as u64))
                    .flat_map(|t| t[k(0x063f_a3f2).as_str()].as_array().cloned().unwrap_or_default())
                    .filter_map(|x| x.as_u64().map(|x| x as u32))
                    .collect();
                let hx = |v: &serde_json::Value| v.as_str().and_then(|s| u64::from_str_radix(s, 16).ok()).unwrap_or(0);
                let (kit, template) = (p["KitName"].as_u64().unwrap_or(0) as u32, p["TemplateName"].as_u64().unwrap_or(0) as u32);
                for prm in p[k(0xebac_f18c).as_str()].as_array().into_iter().flatten() {
                    if prm[k(0x3ead_dcd7).as_str()].as_u64() != Some(PARAM_BASE as u64) {
                        continue;
                    }
                    let t = &prm[k(0xe7cb_9e1b).as_str()];
                    let own: Vec<(u32, u64)> = t[k(0xb70a_22c1).as_str()]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|x| Some((x["Key"].as_u64()? as u32, hx(&x["Item"]["AssetName"]))))
                        .filter(|x| x.1 != 0)
                        .collect();
                    if !own.is_empty() {
                        defs.push((kit, template, own.clone()));
                    }
                    let s = Skin { albedo: hx(&t[k(0xead7_33fc).as_str()]), normal: hx(&t[k(0xd689_b17f).as_str()]) };
                    if s.albedo != 0 {
                        v.extend(heads.iter().map(|&h| (h, s, template, own.clone())));
                    }
                }
            }
            (v, defs)
        })
        .collect();
    let mut defs: HashMap<u32, (u32, Vec<(u32, u64)>)> = HashMap::new();
    for (kit, template, own) in found.iter().flat_map(|f| f.1.iter().cloned()) {
        defs.entry(kit).or_insert((template, own));
    }
    let mut extras: BTreeMap<u64, Vec<(u32, u64)>> = BTreeMap::new();
    for (_, s, template, own) in found.iter().flat_map(|f| &f.0) {
        let x = resolve_extras(own, *template, &defs);
        if !x.is_empty() {
            extras.entry(s.albedo).or_insert(x);
        }
    }
    let found = found.into_iter().flat_map(|f| f.0).map(|(h, s, _, _)| (h, s));
    let mut by: BTreeMap<u32, BTreeSet<Skin>> = BTreeMap::new();
    for (h, s) in found {
        by.entry(h).or_default().insert(s);
    }
    let mut library: BTreeMap<String, BTreeSet<(char, Skin)>> = BTreeMap::new();
    for (h, ss) in &by {
        let Some(n) = names.get(h) else { continue };
        if !is_head(n) {
            continue;
        }
        if let Some((code, age)) = head_code(n) {
            library.entry(code).or_default().extend(ss.iter().map(|s| (age, *s)));
        }
    }
    let heads: Vec<&str> = {
        let mut v: Vec<&str> = names.values().copied().filter(|n| is_head(n)).collect();
        v.sort();
        v.dedup();
        v
    };
    let character = match packs.parent().map(crate::pedlook::models) {
        Some(Ok(m)) => character_links(&heads, &m.aliases, |n| by.contains_key(&joaat(n))),
        _ => BTreeMap::new(),
    };
    HeadSkins {
        version: CACHE_VERSION,
        character,
        by_head: by.into_iter().map(|(h, s)| (h, s.into_iter().collect())).collect(),
        library: library.into_iter().map(|(c, s)| (c, s.into_iter().collect())).collect(),
        extras,
    }
}

/// The head-skin table (built from the packs.rpf index on first use, then
/// cached in the work directory).
pub fn get(game_root: &Path) -> Result<&'static HeadSkins, String> {
    static T: OnceLock<Result<HeadSkins, String>> = OnceLock::new();
    T.get_or_init(|| {
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let cache = work.join("index").join("head_skins.json");
        let ixp = crate::scan::index_path(&work.join("index"), "packs.rpf");
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let Ok(b) = std::fs::read(&cache) {
            if let Ok(t) = serde_json::from_slice::<HeadSkins>(&b) {
                if t.version == CACHE_VERSION && stamp(&cache) >= stamp(&ixp) {
                    return Ok(t);
                }
            }
        }
        let ix = crate::scan::load_index(&ixp).map_err(|e| format!("packs.rpf index ({}): {e}; index packs.rpf first", ixp.display()))?;
        let t = build(&game_root.join("packs.rpf"), &ix);
        if t.by_head.is_empty() {
            return Err("no face presets found in packs.rpf".into());
        }
        if let Ok(b) = serde_json::to_vec(&t) {
            let _ = crate::atomic::write(&cache, b);
        }
        Ok(t)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

impl HeadSkins {
    /// The keyed textures of the face with this albedo (key, texture).
    pub fn extras_of(&self, albedo: u64) -> &[(u32, u64)] {
        self.extras.get(&albedo).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Candidate skins for a head (linked ones, else the automatic group),
    /// filtered by `exists` (the texture is in a store), and the choice at
    /// `index` (wrapping).
    pub fn choose(&self, name: &str, index: usize, exists: impl Fn(u64) -> bool) -> Option<Choice> {
        let n = name.to_ascii_lowercase();
        let linked: Vec<Skin> = self.by_head.get(&joaat(&n)).into_iter().flatten().copied().filter(|s| exists(s.albedo)).collect();
        if !linked.is_empty() {
            let i = index % linked.len();
            return Some(Choice { skin: linked[i], source: "linked", group: String::new(), index: i, count: linked.len() });
        }
        // the same character's other head
        if let Some(d) = self.character.get(&joaat(&n)) {
            let v: Vec<Skin> = self.by_head.get(&joaat(d)).into_iter().flatten().copied().filter(|s| exists(s.albedo)).collect();
            if !v.is_empty() {
                let i = index % v.len();
                return Some(Choice { skin: v[i], source: "character", group: d.clone(), index: i, count: v.len() });
            }
        }
        let (code, age) = head_code(&n)?;
        let sex = &code[..1];
        let pick = |pred: &dyn Fn(&str, char) -> bool| -> Vec<Skin> {
            let mut v: Vec<Skin> = self.library.iter().flat_map(|(c, l)| l.iter().filter(move |(a, _)| pred(c, *a)).map(|(_, s)| *s)).filter(|s| exists(s.albedo)).collect();
            v.sort();
            v.dedup();
            v
        };
        let tries: [(&dyn Fn(&str, char) -> bool, String); 4] = [
            (&|c, a| c == code && a == age, code.clone()),
            (&|c, _| c == code, code.clone()),
            (&|c, _| c.starts_with(sex), sex.to_string()),
            (&|_, _| true, "any".into()),
        ];
        for (pred, group) in tries.iter() {
            let v = pick(*pred);
            if !v.is_empty() {
                let i = index % v.len();
                return Some(Choice { skin: v[i], source: "auto", group: group.clone(), index: i, count: v.len() });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_heads() {
        assert!(same_person_codes("cs_lucia_head_fr1_lat_000_y_full", "player_story000_00_head_fr1_lat_000_m"));
        assert!(!same_person_codes("cs_marv_carpenter_head_mr1_afr_000_o_full", "cs_marv_carpenter_head_mr1_afr_000_m_full"));
        assert!(!same_person_codes("cs_kordell_head_mr1_afr_000_m_full", "cs_kordell_head_mb1_afr_000_m_full"));
        let heads = ["cs_lucia_head_fr1_lat_000_y_full", "player_story000_00_head_fr1_lat_000_m", "cs_blair_head_fr1_cau_000_m_full", "cs_marv_head_mr1_afr_000_o_full", "marv_x_head_mr1_afr_000_m"];
        let mut aliases = BTreeMap::new();
        aliases.insert(joaat("lucia"), vec![joaat("player_story000_00")]);
        aliases.insert(joaat("marv"), vec![joaat("marv_x")]);
        let preset = |n: &str| n.starts_with("player_") || n.starts_with("marv_x");
        let l = character_links(&heads, &aliases, preset);
        assert_eq!(l.len(), 1, "old head and middle-aged face do not mix; blair has no character model");
        assert_eq!(l[&joaat("cs_lucia_head_fr1_lat_000_y_full")], "player_story000_00_head_fr1_lat_000_m");
    }

    #[test]
    fn codes() {
        assert_eq!(head_code("cs_jada_watts_head_fr1_afr_000_y_full"), Some(("fr1_afr".into(), 'y')));
        assert_eq!(head_code("a_head_base_fo1_000_m_full"), Some(("fo1".into(), 'm')));
        assert_eq!(head_code("head_mr1_lat_118_m_full"), Some(("mr1_lat".into(), 'm')));
        assert!(is_head("hc_colombians_head_mr1_lat_004_m"));
        assert!(!is_head("cs_jada_watts_head_fr1_afr_000_y_eyes_full"));
        assert!(!is_head("ambient_femalebody_head"));
    }

    #[test]
    fn keyed_textures_come_from_the_preset_then_its_template_chain() {
        let mut defs = HashMap::new();
        defs.insert(0xa1, (0u32, vec![(0x2633_887d, 1u64), (0x8585_8f5e, 2), (0xe125_85d1, 3)]));
        defs.insert(0xb1, (0xa1u32, vec![(0x2633_887d, 9u64)]));
        // nothing of its own: the template's three
        assert_eq!(resolve_extras(&[], 0xa1, &defs).len(), 3);
        // its own entry wins; the rest comes through two levels
        assert_eq!(resolve_extras(&[(0xe125_85d1, 7)], 0xb1, &defs), vec![(0x2633_887d, 9), (0x8585_8f5e, 2), (0xe125_85d1, 7)]);
        assert!(resolve_extras(&[], 0xc1, &defs).is_empty());
        assert!(extra_role(0xe125_85d1).unwrap().starts_with("wrinkle") && extra_role(1).is_none());
    }
}
