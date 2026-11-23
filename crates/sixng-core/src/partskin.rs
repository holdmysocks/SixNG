//! Part skins: colour textures for animal parts from material presets.
//!
//! Each animal has one packs.rpf meta resource (0x7a) of material presets
//! (`f795e8ba`, the struct `headskin` reads for ped heads). A preset has a
//! name hash (`65121749`), a slot hash (`b53667be`: body, head, eye, teeth,
//! horn, fur …), three tint bytes (`c4d75d79`, `f4fb3dec`, `ab5d2a85`) with
//! a tint palette hash (`5ecf9331`), and in its "base" parameter the albedo
//! (`ead733fc`) and normal map (`d689b17f`). The first preset of a part is
//! named after its drawable; head presets also list the drawable under tag
//! `1c114e61`; the others are the part's variations (coats, eye colours).
//!
//! Two cases need a preset to look right:
//! * the drawable binds a colour texture that is in no store (cat eyes): the
//!   preset named after the drawable has the real one;
//! * the drawable binds a tint mask (cattle: R and B are patch masks, G the
//!   shading; a linear format), coloured in game by the tint bytes through
//!   the palette. The palette lookup is not decoded, so the part takes the
//!   slot's first preset with a ready-painted (sRGB) albedo instead.
//!
//! The table is built once from the packs.rpf index and cached as
//! `<work>/index/part_skins.json`.

use crate::meta7a::{joaat, Meta};
use crate::resource::Resource;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

const PRESET: u32 = 0xf795_e8ba;
const TAG_DRAWABLE: u32 = 0x1c11_4e61;
const PARAM_BASE: u32 = 0x44e2_1c90;
const CACHE_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: u32,
    /// The drawable this preset is named after, when its name is one.
    pub own: Option<String>,
    pub slot: u32,
    pub albedo: u64,
    /// Normal map of the "base" parameter, 0 if none.
    pub normal: u64,
    /// `41acf8eb` of the "base" parameter: on fur presets the GUID of the
    /// fur asset (see `furlink`), 0 if none.
    #[serde(default)]
    pub guid: u64,
    pub tint: [u8; 3],
    pub palette: u32,
    /// Drawables listed under the drawable tag (joaat of the name).
    pub drawables: Vec<u32>,
}

impl Preset {
    /// No tint bytes or no palette: the albedo is used as it is.
    pub fn untinted(&self) -> bool {
        self.palette == 0 || self.tint == [255; 3] || self.tint == [0; 3]
    }
}

/// The presets of one meta resource and the animal roots they belong to.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Set {
    pub roots: Vec<String>,
    pub presets: Vec<Preset>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PartSkins {
    version: u32,
    pub sets: Vec<Set>,
}

/// Choice made for one part.
#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub albedo: u64,
    pub normal: u64,
    /// Name hash of the chosen preset.
    pub preset: u32,
    pub tint: [u8; 3],
    pub palette: u32,
    /// How the part's slot was found: "name", "drawable tag" or "bound texture".
    pub slot_source: &'static str,
    /// Presets of the slot (variations of this part).
    pub count: usize,
}

/// Animal root of a part name: up to the sex letter and number
/// (`crm_cat_feral_m_00_eye_00` -> `crm_cat_feral_m_00`).
pub fn animal_root(name: &str) -> Option<&str> {
    if !crate::outfit::is_animal(name) {
        return None;
    }
    let mut at = 0;
    let toks: Vec<&str> = name.split('_').collect();
    for (i, t) in toks.iter().enumerate() {
        let num = toks.get(i + 1).is_some_and(|n| n.len() == 2 && n.bytes().all(|c| c.is_ascii_digit()));
        if (*t == "m" || *t == "f") && num && i > 0 {
            return Some(&name[..at + t.len() + 3]);
        }
        at += t.len() + 1;
    }
    None
}

fn presets_of(m: &Meta, names: &HashMap<u32, &str>) -> Vec<Preset> {
    let k = |h: u32| crate::meta7a::field_name(h);
    let hx = |v: &serde_json::Value| v.as_str().and_then(|s| u64::from_str_radix(s, 16).ok()).unwrap_or(0);
    let mut v = Vec::new();
    for o in m.objects(PRESET) {
        let Some(p) = m.object(PRESET, o, 1) else { continue };
        let u = |h: u32| p[k(h).as_str()].as_u64().unwrap_or(0);
        let base = p[k(0xebac_f18c).as_str()].as_array().into_iter().flatten().find(|prm| prm[k(0x3ead_dcd7).as_str()].as_u64() == Some(PARAM_BASE as u64));
        let Some(t) = base.map(|prm| &prm[k(0xe7cb_9e1b).as_str()]) else { continue };
        let albedo = hx(&t[k(0xead7_33fc).as_str()]);
        if albedo == 0 {
            continue;
        }
        let name = u(0x6512_1749) as u32;
        v.push(Preset {
            name,
            own: names.get(&name).map(|n| n.to_string()),
            slot: u(0xb536_67be) as u32,
            albedo,
            normal: hx(&t[k(0xd689_b17f).as_str()]),
            guid: hx(&t[k(0x41ac_f8eb).as_str()]),
            tint: [u(0xc4d7_5d79) as u8, u(0xf4fb_3dec) as u8, u(0xab5d_2a85) as u8],
            palette: u(0x5ecf_9331) as u32,
            drawables: p[k(0x12f8_5293).as_str()]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|t| t[k(0x6098_a50e).as_str()].as_u64() == Some(TAG_DRAWABLE as u64))
                .flat_map(|t| t[k(0x063f_a3f2).as_str()].as_array().cloned().unwrap_or_default())
                .filter_map(|x| x.as_u64().map(|x| x as u32))
                .collect(),
        });
    }
    v
}

fn build(packs: &Path, ix: &crate::scan::ArchiveIndex) -> PartSkins {
    let names: HashMap<u32, &str> = ix.entries.iter().filter_map(|e| e.name.as_deref()).filter(|n| crate::outfit::is_animal(n)).map(|n| (joaat(n), n)).collect();
    let mut sets: Vec<(u64, Set)> = ix
        .entries
        .par_iter()
        .filter(|e| e.sub == "7a")
        .filter_map(|e| {
            let r = Resource::load_meta(packs, e).ok()?;
            let presets = presets_of(&Meta::parse(&r)?, &names);
            let mut roots: Vec<String> = presets
                .iter()
                .flat_map(|p| p.own.as_deref().into_iter().chain(p.drawables.iter().filter_map(|d| names.get(d).copied())))
                .filter_map(animal_root)
                .map(str::to_string)
                .collect();
            roots.sort();
            roots.dedup();
            (!roots.is_empty()).then_some((e.off, Set { roots, presets }))
        })
        .collect();
    sets.sort_by_key(|s| s.0);
    PartSkins { version: CACHE_VERSION, sets: sets.into_iter().map(|s| s.1).collect() }
}

/// The part-skin table (built from the packs.rpf index on first use, then
/// cached in the work directory).
pub fn get(game_root: &Path) -> Result<&'static PartSkins, String> {
    static T: OnceLock<Result<PartSkins, String>> = OnceLock::new();
    T.get_or_init(|| {
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let cache = work.join("index").join("part_skins.json");
        let ixp = crate::scan::index_path(&work.join("index"), "packs.rpf");
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let Ok(b) = std::fs::read(&cache) {
            if let Ok(t) = serde_json::from_slice::<PartSkins>(&b) {
                if t.version == CACHE_VERSION && stamp(&cache) >= stamp(&ixp) {
                    return Ok(t);
                }
            }
        }
        let ix = crate::scan::load_index(&ixp).map_err(|e| format!("packs.rpf index ({}): {e}; index packs.rpf first", ixp.display()))?;
        let t = build(&game_root.join("packs.rpf"), &ix);
        if t.sets.is_empty() {
            return Err("no animal material presets found in packs.rpf".into());
        }
        if let Ok(b) = serde_json::to_vec(&t) {
            let _ = std::fs::write(&cache, b);
        }
        Ok(t)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

impl PartSkins {
    /// A ready-painted skin for the part `name`, whose shader binds the
    /// colour texture `bound`. `painted`: the texture is in a store in an
    /// sRGB format. The preset named after the part comes first, then
    /// untinted presets, then the rest.
    pub fn choose(&self, name: &str, bound: u64, painted: impl Fn(u64) -> bool) -> Option<Choice> {
        let name = name.to_ascii_lowercase();
        let root = animal_root(&name)?;
        let set = self.sets.iter().find(|s| s.roots.iter().any(|r| r == root))?;
        let h = joaat(&name);
        let (slot, slot_source) = None
            .or_else(|| set.presets.iter().find(|p| p.name == h).map(|p| (p.slot, "name")))
            .or_else(|| set.presets.iter().find(|p| p.drawables.contains(&h)).map(|p| (p.slot, "drawable tag")))
            .or_else(|| set.presets.iter().find(|p| bound != 0 && p.albedo == bound).map(|p| (p.slot, "bound texture")))?;
        // Presets named after another drawable belong to that drawable.
        let mut c: Vec<&Preset> = set.presets.iter().filter(|p| p.slot == slot && p.own.as_deref().map_or(true, |o| o == name)).collect();
        let count = c.len();
        c.sort_by_key(|p| (p.name != h, !p.untinted()));
        let p = c.into_iter().find(|p| painted(p.albedo))?;
        Some(Choice { albedo: p.albedo, normal: p.normal, preset: p.name, tint: p.tint, palette: p.palette, slot_source, count })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(name: u32, own: Option<&str>, slot: u32, albedo: u64, tint: [u8; 3], palette: u32) -> Preset {
        Preset { name, own: own.map(str::to_string), slot, albedo, normal: 0, guid: 0, tint, palette, drawables: vec![] }
    }

    #[test]
    fn roots() {
        assert_eq!(animal_root("crm_cat_feral_m_00_eye_00"), Some("crm_cat_feral_m_00"));
        assert_eq!(animal_root("crm_cattle_holstein_f_00_body_07_secrecmm1"), Some("crm_cattle_holstein_f_00"));
        assert_eq!(animal_root("crm_cat_feral_m_00"), Some("crm_cat_feral_m_00"));
        assert_eq!(animal_root("cs_ali_head_fr1_000"), None);
    }

    #[test]
    fn picks() {
        let eye = "crm_cat_feral_m_00_eye_00";
        let body = "crm_cattle_holstein_f_00_body_00";
        let t = PartSkins {
            version: CACHE_VERSION,
            sets: vec![
                Set { roots: vec!["crm_cat_feral_m_00".into()], presets: vec![preset(joaat(eye), Some(eye), 1, 10, [0; 3], 0), preset(5, None, 1, 11, [0; 3], 0)] },
                Set {
                    roots: vec!["crm_cattle_holstein_f_00".into()],
                    presets: vec![
                        preset(1, None, 2, 20, [2, 45, 255], 9),
                        preset(2, None, 2, 21, [255; 3], 0),
                        preset(3, Some("crm_cattle_holstein_f_00_body_07_x"), 2, 22, [255; 3], 0),
                        preset(4, None, 3, 23, [255; 3], 0),
                    ],
                },
            ],
        };
        // the eye's bound texture is in no store: the preset named after it
        let c = t.choose(eye, 99, |_| true).unwrap();
        assert_eq!((c.albedo, c.slot_source, c.count), (10, "name", 2));
        // the body binds mask 20: the slot's first painted, untinted preset,
        // not another drawable's preset (22) nor another slot's (23)
        let c = t.choose(body, 20, |h| h != 20).unwrap();
        assert_eq!((c.albedo, c.slot_source, c.count), (21, "bound texture", 2));
        assert!(t.choose(body, 77, |_| true).is_none());
        assert!(t.choose("crm_dog_x_m_00_body_00", 20, |_| true).is_none());
    }
}
