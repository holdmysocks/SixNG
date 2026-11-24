//! Layered-material libraries: the data behind the per-material parameter
//! block of the clothing and weapon shaders (`ped_standard_muv`, `weapon`).
//!
//! Those shaders bind no colour texture. A material has up to 8
//! sub-materials (picked per vertex or by the `materialid` texture) and the
//! game builds one 0xF00-byte block per material from a **layered preset**.
//! The presets live in two PSO files of data.rpf with root struct
//! `8419ab01`: a large one for clothing (7,994 base materials, 13,071
//! presets) and a small one for weapons (556 / 2,635). The root holds four
//! hash maps keyed by a name hash:
//!
//! ```text
//! 42fa4d39 -> 6cc4c278 base material (a fabric, a leather, a metal):
//!                      03e52760 = its tile, 04be3aea = its profile (or 0)
//! fd540657 -> 756aef9c layered preset: per sub-material 4a1ebc36 = base
//!                      material, 4bee7340 = tile override, cfc17384 =
//!                      profile override (0 = none), plus parameter overrides
//! 951cf6e3 -> 83021d7a tile: the tiling surface textures `tilematerialmap`
//!                      and `tilenormalmap` with `tilescale`, `tilesheen` ...
//! 2d71e4f7 -> 440c9ecb profile: scattering / absorption / transmission
//!                      numbers (not interpreted)
//! ```
//!
//! Every record is four parameter lists: floats `1652fce8`, textures
//! `0f208402` (64-bit texture hash), colours `fc24d9c5` (u32 0xAABBGGRR: red in the low byte) and
//! ints `9f6b5b03`. A parameter is `Name` (joaat of a lower-case name;
//! 55 of the 99 in use are recovered, see `names::DERIVED`), `Index`
//! (element of an array parameter, 255 = scalar), `Value` and `aadeb53e`,
//! the sub-material 0..7 (255 = the whole record).
//!
//! What points at a preset:
//! - a wardrobe item's `GarmentPreset` (`e13ebaf8`): clothing library;
//! - a weapon family file's component record `85492dc3`, field `72f5e33e`
//!   (its `873ef899` is the joaat of the component's model name, e.g.
//!   `w_ar_adb_01_barrel_01`; 972 of 1,006 resolve to a model), and its
//!   variant record `6724161c`: `14b064c2` = preset, `c763edb4` = the part
//!   the variant recolours (`frame`, `body` ...): weapon library.
//!
//! Colour (read from the game's block builder): parameter `tiletint`
//! (`f574261b`) has four entries per sub-material. Entry 0 is the base colour; the game
//! blends it towards entry 1 by a per-part byte whose source is not known
//! (0 = entry 0; entry 1 is usually a greyer shade). The blended sRGB
//! colour is expanded to the three stops of the shader's gradient map
//! ([`stops`]). Entries 2 and 3 give a second colour set the same way. A
//! preset's own entry wins; without one the sub-material's base material
//! supplies it.
//!
//! The other colour parameters are tints of further layers: `tintmasktint`
//! (8 entries, with the `tintmaskmap` textures, 5 entries, as their masks),
//! `tintablepatterntint`, `stitchtint` (3, per preset), `dirttint`,
//! `sheencolor`, `engravingcolor`.
//!
//! Not decoded: the remaining parameter names, the defaults of parameters
//! a record leaves out (loaded by the game from a separate file that was
//! not found), and how the layers above are composed. The item's
//! `Tint0..2` bytes are not the blend byte: an item's colour tag fits
//! entry 0 whatever they are.

use crate::meta7a::{field_name, Meta};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

/// Root struct of a library file.
pub const ROOT: u32 = 0x8419_ab01;
pub const BASE: u32 = 0x6cc4_c278;
pub const PRESET: u32 = 0x756a_ef9c;
pub const TILE: u32 = 0x8302_1d7a;
pub const PROFILE: u32 = 0x440c_9ecb;
/// The colour parameter: 4 entries per sub-material.
pub const COLOUR: u32 = 0xf574_261b;
/// Index format version of [`Index`].
pub const VERSION: u32 = 3;

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
    /// Tile names: a preset's override per sub-material (0 = the base
    /// material's), or a base material's own tile in element 0.
    pub tiles: [u32; 8],
    /// Profile names, laid out like `tiles`.
    pub profiles: [u32; 8],
}

#[derive(Debug, Clone, Default)]
pub struct Library {
    pub base: HashMap<u32, Record>,
    pub presets: HashMap<u32, Record>,
    pub tiles: HashMap<u32, Record>,
    pub profiles: HashMap<u32, Record>,
}

/// The tiling surface of one sub-material.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    /// Tile name hash (hex).
    pub tile: String,
    /// `tilematerialmap` and `tilenormalmap` texture hashes (hex, empty = none).
    pub material_map: String,
    pub normal_map: String,
    /// `tilescale` and `tilerotation` (radians): the preset's value for the
    /// sub-material, else the base material's, else the tile's own.
    pub scale: Option<f32>,
    pub rotation: Option<f32>,
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
    let list = |h: u32, single: u32| {
        let mut a = [0u32; 8];
        for (b, x) in a.iter_mut().zip(v[field_name(h)].as_array().into_iter().flatten()) {
            *b = x.as_u64().unwrap_or(0) as u32;
        }
        a[0] = v[field_name(single)].as_u64().map_or(a[0], |x| x as u32);
        a
    };
    let (bases, tiles, profiles) = (list(0x4a1e_bc36, 0), list(0x4bee_7340, 0x03e5_2760), list(0xcfc1_7384, 0x04be_3aea));
    Record {
        floats: params(v, 0x1652_fce8, |x| x.as_f64().map(|f| f as f32)),
        textures: params(v, 0x0f20_8402, |x| u64::from_str_radix(x.as_str()?, 16).ok()),
        colours: params(v, 0xfc24_d9c5, |x| x.as_u64().map(|c| c as u32)),
        ints: params(v, 0x9f6b_5b03, |x| x.as_i64()),
        bases,
        tiles,
        profiles,
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
                        Some(s) if s == st(TILE) => drop(lib.tiles.insert(k as u32, record(it))),
                        Some(s) if s == st(PROFILE) => drop(lib.profiles.insert(k as u32, record(it))),
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

    /// The tiling surface of sub-material `sub` of a preset.
    pub fn tile(&self, preset: u32, sub: usize) -> Option<Tile> {
        let j = crate::meta7a::joaat;
        let p = self.presets.get(&preset)?;
        let base = self.base.get(p.bases.get(sub)?);
        let id = Some(p.tiles[sub]).filter(|&t| t != 0).or_else(|| base.map(|b| b.tiles[0]))?;
        let t = self.tiles.get(&id)?;
        let tex = |n: &str| t.textures.iter().find(|x| x.0 == j(n)).filter(|x| x.3 != 0).map(|x| format!("{:016x}", x.3)).unwrap_or_default();
        let num = |n: &str| {
            let own = p.floats.iter().find(|x| x.0 == j(n) && x.2 as usize == sub);
            own.or_else(|| base?.floats.iter().find(|x| x.0 == j(n))).or_else(|| t.floats.iter().find(|x| x.0 == j(n))).map(|x| x.3)
        };
        Some(Tile { tile: format!("{id:08x}"), material_map: tex("tilematerialmap"), normal_map: tex("tilenormalmap"), scale: num("tilescale"), rotation: num("tilerotation") })
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
    /// Preset (hex) -> the tiling surface of each sub-material that has one.
    pub tiles: BTreeMap<String, Vec<Option<Tile>>>,
    /// Weapon component name hash (hex) -> preset (hex).
    pub weapon_components: BTreeMap<String, String>,
    /// Weapon variant name hash (hex) -> (part name hash hex, preset hex).
    pub weapon_variants: BTreeMap<String, (String, String)>,
    /// Weapon model name hash (hex, joaat of the drawable name without LOD
    /// suffix) -> the presets its component records name, in file order.
    /// 57 of 894 models have two (a second record of the same model).
    pub weapon_models: BTreeMap<String, Vec<String>>,
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
            self.tiles.insert(format!("{k:08x}"), (0..8).map(|s| lib.tile(k, s)).collect());
        }
    }

    /// Weapon links of one decoded weapon family file (`Pso::to_json`).
    pub fn add_weapon_file(&mut self, pso_json: &Value) {
        let (comp, var, model, part) = (field_name(0x72f5_e33e), field_name(0x14b0_64c2), field_name(0x873e_f899), field_name(0xc763_edb4));
        for (st, objs) in pso_json["objects"].as_object().into_iter().flatten() {
            let st = st.get(st.len().saturating_sub(8)..);
            for o in objs.as_array().into_iter().flatten() {
                let (Some(n), preset) = (o["Name"].as_u64(), |f: &str| o[f].as_u64().filter(|&p| p != 0).map(|p| format!("{p:08x}"))) else { continue };
                match st {
                    Some("85492dc3") => {
                        let Some(p) = preset(&comp) else { continue };
                        if let Some(m) = o[&model].as_u64().filter(|&m| m != 0) {
                            let v = self.weapon_models.entry(format!("{m:08x}")).or_default();
                            if !v.contains(&p) {
                                v.push(p.clone());
                            }
                        }
                        self.weapon_components.insert(format!("{n:08x}"), p);
                    }
                    Some("6724161c") => {
                        if let Some(p) = preset(&var) {
                            self.weapon_variants.insert(format!("{n:08x}"), (format!("{:08x}", o[&part].as_u64().unwrap_or(0)), p));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Add the weapon links of every weapon family file of packs.rpf
    /// (PSO files holding struct `85492dc3`), when packs.rpf is indexed.
    pub fn add_weapon_files(&mut self, ws: &crate::export::Workspace) {
        let Some((pa, pix)) = ws.resolve("packs").ok().and_then(|pa| Some((ws.load_index(&pa).ok()?, pa))).map(|(i, a)| (a, i)) else { return };
        let tag = 0x8549_2dc3u32.to_le_bytes();
        for e in pix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Pso) {
            let Ok(b) = crate::scan::read_entry(&pa.path, e) else { continue };
            if b.windows(4).any(|w| w == tag) {
                if let Some(p) = crate::pso::parse(&b) {
                    self.add_weapon_file(&p.to_json());
                }
            }
        }
    }

    /// The index, read from the work directory, or built from the data.rpf
    /// index and written there when it is missing or out of date (what
    /// `sixng materials` does; a few seconds).
    pub fn load_or_build(ws: &crate::export::Workspace) -> Result<Index, String> {
        if let Some(ix) = Index::load(ws) {
            return Ok(ix);
        }
        let a = ws.resolve("data").map_err(|e| format!("{e:#}"))?;
        let aix = ws.load_index(&a).map_err(|e| format!("data.rpf index: {e:#}; index data.rpf first"))?;
        let mut index = Index { version: VERSION, ..Default::default() };
        for e in aix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Pso) {
            let Ok(b) = crate::scan::read_entry(&a.path, e) else { continue };
            let Some(p) = crate::pso::parse(&b).filter(|p| p.structs.contains_key(&ROOT)) else { continue };
            index.add(&e.id, &Library::from_meta(&p.meta()));
        }
        if index.presets.is_empty() {
            return Err("no layered-material library found in data.rpf".into());
        }
        index.add_weapon_files(ws);
        if let Ok(b) = serde_json::to_vec(&index) {
            let p = path(ws);
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, b);
        }
        Ok(index)
    }

    pub fn load(ws: &crate::export::Workspace) -> Option<Index> {
        let ix: Index = serde_json::from_slice(&std::fs::read(path(ws)).ok()?).ok()?;
        (ix.version == VERSION).then_some(ix)
    }

    /// The presets of a weapon model, by drawable name (a LOD suffix
    /// `_hi` / `_mid` is ignored).
    pub fn weapon_presets(&self, model: &str) -> &[String] {
        let m = model.to_ascii_lowercase();
        let base = ["_hi", "_mid"].iter().find_map(|s| m.strip_suffix(s)).unwrap_or(&m);
        self.weapon_models.get(&format!("{:08x}", crate::meta7a::joaat(base))).map(Vec::as_slice).unwrap_or(&[])
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
        // tiles: base 0x11 uses tile 0x33; the preset overrides sub-material 1 with tile 0x34 and its own scale for sub-material 0
        let j = crate::meta7a::joaat;
        let tile = |map: &str, scale: f64| json!({field_name(0x0f20_8402): [param(j("tilematerialmap"), 255, 255, json!(map)), param(j("tilenormalmap"), 255, 255, json!("0000000000000000"))], field_name(0x1652_fce8): [param(j("tilescale"), 255, 255, json!(scale))]});
        lib.tiles.insert(0x33, record(&tile("00000000000000aa", 100.0)));
        lib.tiles.insert(0x34, record(&tile("00000000000000bb", 50.0)));
        lib.base.get_mut(&0x11).unwrap().tiles[0] = 0x33;
        lib.presets.get_mut(&0x22).unwrap().tiles[1] = 0x34;
        let t0 = lib.tile(0x22, 0).unwrap();
        assert_eq!((t0.tile.as_str(), t0.material_map.as_str(), t0.normal_map.as_str(), t0.scale), ("00000033", "00000000000000aa", "", Some(336.5)));
        let t1 = lib.tile(0x22, 1).unwrap();
        assert_eq!((t1.tile.as_str(), t1.material_map.as_str(), t1.scale, t1.rotation), ("00000034", "00000000000000bb", Some(50.0), None));
        assert_eq!(lib.tile(0x22, 2), None);
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
            "9a16508d85492dc3": [
                {"Name": 0x921b_2361u32, field_name(0x72f5_e33e): 0xd9f0_240du32, field_name(0x873e_f899): crate::meta7a::joaat("w_ar_adb_01_barrel_01")},
                {"Name": 6, field_name(0x72f5_e33e): 0x11u32, field_name(0x873e_f899): crate::meta7a::joaat("w_ar_adb_01_barrel_01")},
                {"Name": 5, field_name(0x72f5_e33e): 0}],
            "eafa47ee6724161c": [{"Name": 0xd0b1_bd2au32, field_name(0x14b0_64c2): 0x5e52_bcd0u32, field_name(0xc763_edb4): crate::meta7a::joaat("frame")}],
        }});
        let mut ix = Index::default();
        ix.add_weapon_file(&j);
        assert_eq!(ix.weapon_components.get("921b2361").map(String::as_str), Some("d9f0240d"));
        assert_eq!((ix.weapon_components.len(), ix.weapon_variants["d0b1bd2a"].1.as_str()), (2, "5e52bcd0"));
        assert_eq!(ix.weapon_variants["d0b1bd2a"].0, format!("{:08x}", crate::meta7a::joaat("frame")));
        assert_eq!(ix.weapon_presets("W_AR_ADB_01_barrel_01_hi"), ["d9f0240d".to_string(), "00000011".to_string()]);
        assert!(ix.weapon_presets("w_ar_adb_01").is_empty());
    }
}
