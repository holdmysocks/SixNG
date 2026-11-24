//! Layered-material libraries: the data behind the per-material parameter
//! block of the clothing and weapon shaders (`ped_standard_muv`, `weapon`).
//!
//! Those shaders bind no colour texture. A material has up to 8
//! sub-materials (picked per vertex or by the `materialid` texture) and the
//! game builds one 0xF00-byte block per material from a **layered preset**.
//! The presets live in two PSO files of data.rpf with root struct
//! `8419ab01`: a large one for clothing (7,994 base materials, 13,071
//! presets) and a small one for weapons (556 / 2,635). The root holds four
//! hash maps keyed by a name hash; two are decoded here:
//!
//! ```text
//! 42fa4d39 -> 6cc4c278 base material (a fabric, a leather, a metal)
//! fd540657 -> 756aef9c layered preset: 4a1ebc36 = 8 base-material names
//!                      (one per sub-material) plus overrides
//! ```
//!
//! Every record is four parameter lists: floats `1652fce8`, textures
//! `0f208402` (64-bit texture hash), colours `fc24d9c5` (u32 0xAABBGGRR: red in the low byte) and
//! ints `9f6b5b03`. A parameter is `Name` (hash; the names are not
//! recovered), `Index` (element of an array parameter, 255 = scalar),
//! `Value` and `aadeb53e`, the sub-material 0..7 (255 = the whole record).
//!
//! What points at a preset:
//! - a wardrobe item's `GarmentPreset` (`e13ebaf8`): clothing library;
//! - a weapon family file's component record `85492dc3`, field `72f5e33e`,
//!   and its variant record `6724161c`, field `14b064c2`: weapon library.
//!
//! Colour (read from the game's block builder): parameter `f574261b` has
//! four entries per sub-material. Entry 0 is the base colour; the game
//! blends it towards entry 1 by a per-part byte whose source is not known
//! (0 = entry 0; entry 1 is usually a greyer shade). The blended sRGB
//! colour is expanded to the three stops of the shader's gradient map
//! ([`stops`]). Entries 2 and 3 give a second colour set the same way. A
//! preset's own entry wins; without one the sub-material's base material
//! supplies it.
//!
//! Not decoded: the parameter names, the defaults of parameters a record
//! leaves out (a run-time table), the other two maps, and what the item's
//! `Tint0..2` bytes do on items that have a preset.

use crate::meta7a::{field_name, Meta};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

/// Root struct of a library file.
pub const ROOT: u32 = 0x8419_ab01;
pub const BASE: u32 = 0x6cc4_c278;
pub const PRESET: u32 = 0x756a_ef9c;
/// The colour parameter: 4 entries per sub-material.
pub const COLOUR: u32 = 0xf574_261b;
/// Index format version of [`Index`].
pub const VERSION: u32 = 1;

/// One parameter: (name hash, index, sub-material, value).
pub type Param<T> = (u32, u8, u8, T);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    pub floats: Vec<Param<f32>>,
    /// 64-bit texture hashes.
    pub textures: Vec<Param<u64>>,
    /// 0xAABBGGRR.
    pub colours: Vec<Param<u32>>,
    pub ints: Vec<Param<i64>>,
    /// Base-material names per sub-material (presets only).
    pub bases: [u32; 8],
}

#[derive(Debug, Clone, Default)]
pub struct Library {
    pub base: HashMap<u32, Record>,
    pub presets: HashMap<u32, Record>,
}

fn params<T>(v: &Value, list: u32, val: impl Fn(&Value) -> Option<T>) -> Vec<Param<T>> {
    let sub = field_name(0xaade_b53e);
    v[field_name(list)]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p["Name"].as_u64()? as u32, p["Index"].as_u64()? as u8, p[&sub].as_u64()? as u8, val(&p["Value"])?)))
        .collect()
}

/// A record from its decoded JSON object (`Meta::object`).
pub fn record(v: &Value) -> Record {
    let mut bases = [0u32; 8];
    for (b, x) in bases.iter_mut().zip(v[field_name(0x4a1e_bc36)].as_array().into_iter().flatten()) {
        *b = x.as_u64().unwrap_or(0) as u32;
    }
    Record {
        floats: params(v, 0x1652_fce8, |x| x.as_f64().map(|f| f as f32)),
        textures: params(v, 0x0f20_8402, |x| u64::from_str_radix(x.as_str()?, 16).ok()),
        colours: params(v, 0xfc24_d9c5, |x| x.as_u64().map(|c| c as u32)),
        ints: params(v, 0x9f6b_5b03, |x| x.as_i64()),
        bases,
    }
}

impl Library {
    /// Read the base-material and preset maps of a parsed library file.
    /// The maps are stored as bucket objects with an entry list `93c495af`
    /// of `{Key, Item}`; entries are taken from every bucket type, by the
    /// struct of their item.
    pub fn from_meta(m: &Meta) -> Library {
        let mut lib = Library::default();
        let (entries, st) = (field_name(0x93c4_95af), |h: u32| format!("{h:08x}"));
        let mut hs: Vec<u32> = m.structs.values().filter(|s| s.members.iter().any(|x| x.hash == 0x93c4_95af)).map(|s| s.hash).collect();
        hs.sort();
        for h in hs {
            for o in m.objects(h) {
                let Some(b) = m.object(h, o, 0) else { continue };
                for e in b[&entries].as_array().into_iter().flatten() {
                    let (Some(k), it) = (e["Key"].as_u64(), &e["Item"]) else { continue };
                    match it["_struct"].as_str() {
                        Some(s) if s == st(BASE) => drop(lib.base.insert(k as u32, record(it))),
                        Some(s) if s == st(PRESET) => drop(lib.presets.insert(k as u32, record(it))),
                        _ => {}
                    }
                }
            }
        }
        lib
    }

    /// Colour entry `index` (0..=3) of sub-material `sub` of a preset, as
    /// sRGB RGBA: the preset's own, else its base material's.
    pub fn colour(&self, preset: u32, sub: usize, index: u8) -> Option<[u8; 4]> {
        let p = self.presets.get(&preset)?;
        let own = p.colours.iter().find(|c| c.0 == COLOUR && c.1 == index && c.2 as usize == sub);
        let c = own.or_else(|| self.base.get(p.bases.get(sub)?)?.colours.iter().find(|c| c.0 == COLOUR && c.1 == index))?;
        Some(rgba(c.3))
    }
}

/// 0xAABBGGRR -> [r, g, b, a]. The byte order was settled with the items'
/// colour tags: presets of items tagged blue hold values such as
/// 0xff684a26, those tagged brown 0xff142e49.
pub fn rgba(c: u32) -> [u8; 4] {
    c.to_le_bytes()
}

fn to_linear(c: u8) -> f32 {
    let x = c as f32 / 255.0;
    if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}

/// The three gradient stops the game builds from one sub-material colour
/// (sRGB in, linear RGB out): dark = the colour at 0.1 x value, middle =
/// the colour, light = saturation x 0.55 at value x 0.55 + 0.45. The
/// shader maps the fabric texture's grey value g over them (g < 0.5: dark
/// to middle by 2g, else middle towards light by g - 0.5) and mixes the
/// result with plain g by the colour's alpha.
pub fn stops(srgb: [u8; 3]) -> [[f32; 3]; 3] {
    let lin = srgb.map(to_linear);
    let v = lin[0].max(lin[1]).max(lin[2]);
    let light = lin.map(|c| if v > 0.0 { (0.55 * (c / v - 1.0) + 1.0) * (0.55 * v + 0.45) } else { 0.45 });
    [lin.map(|c| 0.1 * c), lin, light]
}

/// Compact form of the libraries for other commands
/// (`<work>/index/material_presets.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    /// Library files read: entry id, base materials, presets.
    pub libraries: Vec<(String, usize, usize)>,
    /// Preset (hex) -> per sub-material the four colour entries as
    /// `rrggbbaa` sRGB (empty = none).
    pub presets: BTreeMap<String, Vec<[String; 4]>>,
    /// Weapon component name hash (hex) -> preset (hex).
    pub weapon_components: BTreeMap<String, String>,
    /// Weapon variant name hash (hex) -> preset (hex).
    pub weapon_variants: BTreeMap<String, String>,
}

pub fn path(ws: &crate::export::Workspace) -> PathBuf {
    ws.index_dir().join("material_presets.json")
}

impl Index {
    pub fn add(&mut self, id: &str, lib: &Library) {
        self.libraries.push((id.to_string(), lib.base.len(), lib.presets.len()));
        for &k in lib.presets.keys() {
            let subs = (0..8).map(|s| [0u8, 1, 2, 3].map(|i| lib.colour(k, s, i).map(|c| format!("{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])).unwrap_or_default())).collect();
            self.presets.insert(format!("{k:08x}"), subs);
        }
    }

    /// Weapon links of one decoded weapon family file (`Pso::to_json`).
    pub fn add_weapon_file(&mut self, pso_json: &Value) {
        let (comp, var) = (field_name(0x72f5_e33e), field_name(0x14b0_64c2));
        for (st, objs) in pso_json["objects"].as_object().into_iter().flatten() {
            let (field, out) = match st.get(st.len().saturating_sub(8)..) {
                Some("85492dc3") => (&comp, &mut self.weapon_components),
                Some("6724161c") => (&var, &mut self.weapon_variants),
                _ => continue,
            };
            for o in objs.as_array().into_iter().flatten() {
                if let (Some(n), Some(p)) = (o["Name"].as_u64(), o[field].as_u64().filter(|&p| p != 0)) {
                    out.insert(format!("{n:08x}"), format!("{p:08x}"));
                }
            }
        }
    }

    pub fn load(ws: &crate::export::Workspace) -> Option<Index> {
        let ix: Index = serde_json::from_slice(&std::fs::read(path(ws)).ok()?).ok()?;
        (ix.version == VERSION).then_some(ix)
    }

    /// Base colour (entry 0) per sub-material of a preset, sRGB RGBA.
    pub fn colours(&self, preset: &str) -> Option<Vec<Option<[u8; 4]>>> {
        let hex = |s: &str| -> Option<[u8; 4]> { Some(u32::from_str_radix(s, 16).ok().filter(|_| s.len() == 8)?.to_be_bytes()) };
        Some(self.presets.get(preset)?.iter().map(|c| hex(&c[0])).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn param(name: u32, index: u8, sub: u8, value: Value) -> Value {
        json!({"Name": name, "Index": index, field_name(0xaade_b53e): sub, "Value": value})
    }

    #[test]
    fn preset_colour_falls_back_to_the_base_material() {
        let base = json!({field_name(0xfc24_d9c5): [param(COLOUR, 0, 255, json!(0xff7a_7a7au32)), param(COLOUR, 1, 255, json!(0xff7f_7fbbu32))]});
        let preset = json!({
            field_name(0x4a1e_bc36): [0x11u32, 0x11, 0, 0, 0, 0, 0, 0],
            field_name(0xfc24_d9c5): [param(COLOUR, 0, 0, json!(0x80a2_551du32)), param(0x1c85_bdf6, 0, 255, json!(0xff21_2121u32))],
            field_name(0x1652_fce8): [param(0x01ae_45d3, 255, 0, json!(336.5))],
            field_name(0x0f20_8402): [param(0xd2b5_98e5, 2, 3, json!("6d81fbf3df274f6d"))],
        });
        let mut lib = Library::default();
        lib.base.insert(0x11, record(&base));
        lib.presets.insert(0x22, record(&preset));
        let p = &lib.presets[&0x22];
        assert_eq!((p.floats[0], p.textures[0], p.bases[1]), ((0x01ae_45d3, 255, 0, 336.5), (0xd2b5_98e5, 2, 3, 0x6d81_fbf3_df27_4f6d), 0x11));
        assert_eq!(lib.colour(0x22, 0, 0), Some([0x1d, 0x55, 0xa2, 0x80]));
        assert_eq!(lib.colour(0x22, 0, 1), Some([0xbb, 0x7f, 0x7f, 0xff]));
        assert_eq!(lib.colour(0x22, 1, 0), Some([0x7a, 0x7a, 0x7a, 0xff]));
        assert_eq!(lib.colour(0x22, 2, 0), None);

        let mut ix = Index::default();
        ix.add("data.rpf@0", &lib);
        assert_eq!(ix.libraries, vec![("data.rpf@0".to_string(), 1, 1)]);
        assert_eq!(ix.presets["00000022"][0], ["1d55a280".to_string(), "bb7f7fff".to_string(), String::new(), String::new()]);
        assert_eq!(ix.colours("00000022").unwrap()[..3], [Some([0x1d, 0x55, 0xa2, 0x80]), Some([0x7a, 0x7a, 0x7a, 0xff]), None]);
    }

    #[test]
    fn stops_are_dark_colour_and_a_desaturated_light() {
        let [dark, mid, light] = stops([255, 0, 0]);
        assert_eq!((dark, mid), ([0.1, 0.0, 0.0], [1.0, 0.0, 0.0]));
        assert!((light[0] - 1.0).abs() < 1e-6 && (light[1] - 0.45).abs() < 1e-6 && light[1] == light[2]);
        // a grey stays grey: value 0.55 v + 0.45
        let g = stops([128, 128, 128]);
        let v = g[1][0];
        assert!((v - 0.2158).abs() < 1e-3 && (g[2][0] - (0.55 * v + 0.45)).abs() < 1e-6 && g[2][0] == g[2][1]);
        assert_eq!(stops([0, 0, 0])[2], [0.45; 3]);
    }

    #[test]
    fn weapon_links_come_from_the_component_and_variant_records() {
        let j = json!({"objects": {
            "9a16508d85492dc3": [{"Name": 0x921b_2361u32, field_name(0x72f5_e33e): 0xd9f0_240du32}, {"Name": 5, field_name(0x72f5_e33e): 0}],
            "eafa47ee6724161c": [{"Name": 0xd0b1_bd2au32, field_name(0x14b0_64c2): 0x5e52_bcd0u32}],
        }});
        let mut ix = Index::default();
        ix.add_weapon_file(&j);
        assert_eq!(ix.weapon_components.get("921b2361").map(String::as_str), Some("d9f0240d"));
        assert_eq!((ix.weapon_components.len(), ix.weapon_variants["d0b1bd2a"].as_str()), (1, "5e52bcd0"));
    }
}
