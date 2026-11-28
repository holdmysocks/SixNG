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
//! Tint-mask layers (read from the clothing pixel shader and the block
//! builder; [`Library::layers`]): over the base colour the shader lays up
//! to four layers per sub-material. Layer L has a mask texture
//! `tintmaskmap[L]` (red channel), sampled on the tiling UV set through
//! its own transform (`tintmaskscale[2L, 2L+1]`, `tintmaskrotation[L]`,
//! `tintmasktranslation[2L, 2L+1]`), an opacity `tintmaskopacity[L]`, a
//! flag word `tintmaskflags[L]` (bit 2 inverts the mask) and a colour
//! `tintmasktint[L]` that is expanded to gradient stops like the base
//! colour. In order L = 0..3:
//!
//! ```text
//! m = invert ? 1 - mask : mask
//! t = m * opacity * tint.alpha
//! colour = lerp(colour, gradient(tint, g), t)        // g: the tile's grey value
//! ```
//!
//! `tintmasktint[4 + L]` is layer L's colour in the second colour set
//! (the one built on `tiletint[2..3]`). The other colour parameters are
//! `tintablepatterntint`, `stitchtint` (3, per preset), `dirttint`,
//! `sheencolor`, `engravingcolor`; their composition is not decoded.
//!
//! Defaults ([`Defaults`]): a parameter that neither a preset nor its base
//! material sets takes the game's default, from the file
//! `$(target)/data/dynamic_art/muv_parameter_default_values` (content type
//! `DYNAMIC_ART_MUV_PARAMETER_DEFAULTS_FILE`, a PSO in data.rpf that only
//! the archive's table of contents reaches). Its root `e5f515b3` holds the
//! same four lists as a record, as maps from parameter name to one value
//! (no index or sub-material). The game builds its starting parameter
//! blocks from them; a name missing from the file is 0.
//!
//! Not decoded: the remaining parameter names and how the layers above are
//! composed. The item's
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
/// Two parameters whose names are not recovered but whose values behave as
/// a sub-material's metalness and roughness in the weapon library (read
/// from the data, not from the shader): `0e555ef5` is 0.85 to 1.0 where
/// present (96 % of gold sub-materials, 78 % of steel-grey ones, 34 % of
/// wood-brown ones); `83472381` has a median of 0.15 on gold, 0.40 to 0.45
/// on steel greys and 0.75 on wood browns.
pub const METALLIC: u32 = 0x0e55_5ef5;
pub const ROUGHNESS: u32 = 0x8347_2381;

/// Index format version of [`Index`].
pub const VERSION: u32 = 7;

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
    /// The game's parameter defaults (empty when the file was not read).
    pub defaults: Defaults,
}

/// Root struct of the parameter-defaults file.
pub const DEFAULTS_ROOT: u32 = 0xe5f5_15b3;

/// The game's default for each parameter, by name hash (see the module
/// notes). Keys are hex in JSON.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Defaults {
    pub floats: BTreeMap<String, f32>,
    pub colours: BTreeMap<String, u32>,
    pub ints: BTreeMap<String, i64>,
    pub textures: BTreeMap<String, String>,
}

impl Defaults {
    /// From the decoded root object of the defaults file (`Pso::root_json`).
    pub fn from_root(v: &Value) -> Defaults {
        fn map<T>(v: &Value, list: u32, val: impl Fn(&Value) -> Option<T>) -> BTreeMap<String, T> {
            v[field_name(list)].as_array().into_iter().flatten().filter_map(|e| Some((format!("{:08x}", e["key"].as_u64()?), val(&e["value"])?))).collect()
        }
        Defaults {
            floats: map(v, 0x1652_fce8, |x| x.as_f64().map(|f| f as f32)),
            colours: map(v, 0xfc24_d9c5, |x| x.as_u64().map(|c| c as u32)),
            ints: map(v, 0x9f6b_5b03, |x| x.as_i64()),
            textures: map(v, 0x0f20_8402, |x| Some(x.as_str()?.to_string())),
        }
    }

    /// Read the defaults file from data.rpf through its table of contents:
    /// the small plain PSO whose root is [`DEFAULTS_ROOT`].
    pub fn read(ws: &crate::export::Workspace) -> Result<Defaults, String> {
        let e = |e: anyhow::Error| format!("{e:#}");
        let a = ws.resolve("data").map_err(e)?;
        let mut cipher = crate::gamecipher::open(&crate::export::game_root(&a)).map_err(e)?;
        let mut f = std::fs::File::open(&a.path).map_err(|x| x.to_string())?;
        let toc = crate::rpf9toc::open_toc(cipher.as_mut(), &mut f, 0).map_err(e)?;
        for fe in toc.files().map_err(e)?.iter().filter(|x| x.resource.is_none() && x.unpacked > 0 && x.unpacked < 1 << 16) {
            let Ok((d, _)) = crate::rpf9toc::read_file(cipher.as_mut(), &mut f, 0, fe) else { continue };
            let Some(p) = crate::pso::parse(&d).filter(|p| p.structs.contains_key(&DEFAULTS_ROOT)) else { continue };
            let m = p.meta();
            if let Some(r) = m.objects(DEFAULTS_ROOT).into_iter().next().and_then(|o| m.object(DEFAULTS_ROOT, o, 0)) {
                return Ok(Defaults::from_root(&r));
            }
        }
        Err("no parameter-defaults file (PSO with root e5f515b3) in data.rpf".into())
    }

    pub fn is_empty(&self) -> bool {
        self.floats.is_empty() && self.colours.is_empty() && self.ints.is_empty() && self.textures.is_empty()
    }

    pub fn float(&self, name: u32) -> Option<f32> {
        self.floats.get(&format!("{name:08x}")).copied()
    }
    pub fn colour(&self, name: u32) -> Option<u32> {
        self.colours.get(&format!("{name:08x}")).copied()
    }
    pub fn int(&self, name: u32) -> Option<i64> {
        self.ints.get(&format!("{name:08x}")).copied()
    }
    pub fn texture(&self, name: u32) -> Option<u64> {
        u64::from_str_radix(self.textures.get(&format!("{name:08x}"))?, 16).ok()
    }
}

/// One tint-mask layer of a sub-material (see the module notes).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub layer: u8,
    /// `tintmaskmap` texture hash (hex); the mask is its red channel.
    pub mask_map: String,
    /// `tintmasktint[layer]` and `[4 + layer]`, sRGB RGBA.
    pub tint: Option<[u8; 4]>,
    pub tint2: Option<[u8; 4]>,
    pub opacity: f32,
    pub flags: u32,
    /// Bit 2 of the flags: the mask is used as 1 - mask.
    pub invert: bool,
    pub scale: [Option<f32>; 2],
    pub rotation: Option<f32>,
    pub translation: [Option<f32>; 2],
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

    /// Every layered-material library of data.rpf as one (clothing and
    /// weapons; their preset names do not collide). A few seconds.
    pub fn load(ws: &crate::export::Workspace) -> Result<Library, String> {
        let a = ws.resolve("data").map_err(|e| format!("{e:#}"))?;
        let aix = ws.load_index(&a).map_err(|e| format!("data.rpf index: {e:#}; index data.rpf first"))?;
        let mut all = Library::default();
        for e in aix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Pso) {
            let Ok(b) = crate::scan::read_entry(&a.path, e) else { continue };
            let Some(p) = crate::pso::parse(&b).filter(|p| p.structs.contains_key(&ROOT)) else { continue };
            let lib = Library::from_meta(&p.meta());
            all.base.extend(lib.base);
            all.presets.extend(lib.presets);
            all.tiles.extend(lib.tiles);
            all.profiles.extend(lib.profiles);
        }
        if all.presets.is_empty() {
            return Err("no layered-material library found in data.rpf".into());
        }
        // the index keeps a copy, which saves reading the table of contents;
        // SIXNG_NO_MUV_DEFAULTS leaves them out (before / after comparisons)
        if std::env::var_os("SIXNG_NO_MUV_DEFAULTS").is_some() {
            return Ok(all);
        }
        all.defaults = Index::load(ws).map(|i| i.defaults).filter(|d| !d.is_empty()).unwrap_or_else(|| Defaults::read(ws).unwrap_or_default());
        Ok(all)
    }

    /// Colour entry `index` (0..=3) of sub-material `sub` of a preset, as
    /// sRGB RGBA: the preset's own, else its base material's.
    pub fn colour(&self, preset: u32, sub: usize, index: u8) -> Option<[u8; 4]> {
        let p = self.presets.get(&preset)?;
        let own = p.colours.iter().find(|c| c.0 == COLOUR && c.1 == index && c.2 as usize == sub);
        let c = own.or_else(|| self.base.get(p.bases.get(sub)?)?.colours.iter().find(|c| c.0 == COLOUR && c.1 == index))?;
        Some(rgba(c.3))
    }

    /// The tint-mask layers of sub-material `sub` of a preset that have a
    /// mask texture and a non-zero opacity. Each value is the preset's
    /// entry for the sub-material, else its base material's, else the
    /// game's default ([`Defaults`]: opacity 1, scale 1, rotation and
    /// translation 0, flags 8). The tint has no default here: the file's is
    /// transparent black, which leaves the layer out.
    pub fn layers(&self, preset: u32, sub: usize) -> Vec<Layer> {
        let j = crate::meta7a::joaat;
        let Some(p) = self.presets.get(&preset) else { return Vec::new() };
        let base = p.bases.get(sub).and_then(|b| self.base.get(b));
        fn pick<T: Copy>(own: &[Param<T>], base: Option<&[Param<T>]>, name: u32, index: u8, sub: usize) -> Option<T> {
            own.iter().find(|x| x.0 == name && x.1 == index && x.2 as usize == sub).or_else(|| base?.iter().find(|x| x.0 == name && x.1 == index)).map(|x| x.3)
        }
        let d = &self.defaults;
        let f = |n: &str, i: u8| pick(&p.floats, base.map(|b| &b.floats[..]), j(n), i, sub).or_else(|| d.float(j(n)));
        let c = |i: u8| pick(&p.colours, base.map(|b| &b.colours[..]), j("tintmasktint"), i, sub).map(rgba);
        (0..4u8)
            .filter_map(|l| {
                let mask = pick(&p.textures, base.map(|b| &b.textures[..]), j("tintmaskmap"), l, sub).or_else(|| d.texture(j("tintmaskmap"))).filter(|&h| h != 0)?;
                let opacity = f("tintmaskopacity", l).filter(|&o| o > 0.0)?;
                let flags = pick(&p.ints, base.map(|b| &b.ints[..]), j("tintmaskflags"), l, sub).or_else(|| d.int(j("tintmaskflags"))).unwrap_or(0) as u32;
                Some(Layer {
                    layer: l,
                    mask_map: format!("{mask:016x}"),
                    tint: c(l),
                    tint2: c(4 + l),
                    opacity,
                    flags,
                    invert: flags & 4 != 0,
                    scale: [f("tintmaskscale", 2 * l), f("tintmaskscale", 2 * l + 1)],
                    rotation: f("tintmaskrotation", l),
                    translation: [f("tintmasktranslation", 2 * l), f("tintmasktranslation", 2 * l + 1)],
                })
            })
            .collect()
    }

    /// Metalness and roughness of sub-material `sub` of a preset (see
    /// [`METALLIC`]): the preset's value for the sub-material, else its
    /// base material's. Without one, metalness is the tile's
    /// `tilemetallic`, else 0, and roughness is 0.5. The defaults file gives
    /// 0 for both, but in the weapon shader these two are blend targets
    /// under a mask, not the base surface, so a missing roughness is not
    /// taken as 0 (mirror-smooth) here.
    pub fn surface(&self, preset: u32, sub: usize) -> Option<[f32; 2]> {
        let p = self.presets.get(&preset)?;
        let base = self.base.get(p.bases.get(sub)?);
        let f = |name: u32| p.floats.iter().find(|x| x.0 == name && x.2 as usize == sub).or_else(|| base?.floats.iter().find(|x| x.0 == name)).map(|x| x.3);
        let tile = Some(p.tiles[sub]).filter(|&t| t != 0).or_else(|| base.map(|b| b.tiles[0])).and_then(|t| self.tiles.get(&t));
        let tile_metal = tile.and_then(|t| t.floats.iter().find(|x| x.0 == crate::meta7a::joaat("tilemetallic")).map(|x| x.3));
        Some([f(METALLIC).or(tile_metal).unwrap_or(0.0).clamp(0.0, 1.0), f(ROUGHNESS).unwrap_or(0.5).clamp(0.0, 1.0)])
    }

    /// The tiling surface of sub-material `sub` of a preset. `scale` and
    /// `rotation` fall back to the game's defaults (1 and 0).
    pub fn tile(&self, preset: u32, sub: usize) -> Option<Tile> {
        let j = crate::meta7a::joaat;
        let p = self.presets.get(&preset)?;
        let base = self.base.get(p.bases.get(sub)?);
        let id = Some(p.tiles[sub]).filter(|&t| t != 0).or_else(|| base.map(|b| b.tiles[0]))?;
        let t = self.tiles.get(&id)?;
        let tex = |n: &str| t.textures.iter().find(|x| x.0 == j(n)).filter(|x| x.3 != 0).map(|x| format!("{:016x}", x.3)).unwrap_or_default();
        let num = |n: &str| {
            let own = p.floats.iter().find(|x| x.0 == j(n) && x.2 as usize == sub);
            own.or_else(|| base?.floats.iter().find(|x| x.0 == j(n))).or_else(|| t.floats.iter().find(|x| x.0 == j(n))).map(|x| x.3).or_else(|| self.defaults.float(j(n)))
        };
        Some(Tile { tile: format!("{id:08x}"), material_map: tex("tilematerialmap"), normal_map: tex("tilenormalmap"), scale: num("tilescale"), rotation: num("tilerotation") })
    }
}

/// A sub-material's transmission record: what the see-through clothing
/// shader (`ped_standard_muv_alpha`, lenses) reads per sub-material from the
/// game's per-material table, as the game's parser fills it from a profile
/// record ([`PROFILE`]): the parameter defaults first ([`Defaults`]; a
/// parameter without one is 0), then each of the profile's parameters
/// whose index is 255 or the field's own. A sub-material whose preset and
/// base name no profile keeps the all-defaults record. Traced from the
/// executable and the shader (shared notes 42es and its follow-ups); the
/// copy of the record into the GPU table is taken as verbatim (the field
/// offsets line up with the shader's reads).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transmission {
    /// `Absorption[0..2]` (σA) and `f1088873` (how far the shaded base
    /// colour lowers it, wA).
    pub absorption: [f32; 3],
    pub absorption_albedo: f32,
    /// `scattering[0..2]` (σB) and `000482aa` (wB).
    pub scattering: [f32; 3],
    pub scattering_albedo: f32,
    /// `5ba33632`: the thickness (d_base); `95f8d114`: its mask scale
    /// (d_mask); `a14dce4e`: the cap on the view path (d_max).
    pub thickness: f32,
    pub thickness_mask: f32,
    pub path_cap: f32,
    /// `thicknessanisotropy` (p): how much of the slanted view path counts;
    /// its sign picks 1 / |N.V| (positive) or |N.V|.
    pub anisotropy: f32,
    /// `0604e865` (h0): back / front thickness anchor; negative = none.
    pub back_front: f32,
    /// `alphathreshold` (k: opacity = saturate(a0 / k)) and `f920ffb1`
    /// (t: the part of a0 above it is drawn opaque).
    pub alpha_divisor: f32,
    pub opaque_from: f32,
    /// The profile's name (0 = none, the all-defaults record).
    pub profile: u32,
}

impl Library {
    /// The transmission record of sub-material `sub` of a preset (see
    /// [`Transmission`]): the preset's profile override for the
    /// sub-material, else its base material's profile, else none.
    pub fn transmission(&self, preset: u32, sub: usize) -> Option<Transmission> {
        let p = self.presets.get(&preset)?;
        let base = p.bases.get(sub).and_then(|b| self.base.get(b));
        let id = Some(p.profiles.get(sub).copied().unwrap_or(0)).filter(|&x| x != 0).or_else(|| base.map(|b| b.profiles[0]).filter(|&x| x != 0)).unwrap_or(0);
        let prof = self.profiles.get(&id);
        let j = crate::meta7a::joaat;
        // field `name` at slot `index`: the profile's value (index 255 or the slot's), else the default, else 0
        let f = |name: u32, index: u8| {
            prof.and_then(|r| r.floats.iter().find(|x| x.0 == name && (x.1 == 255 || x.1 == index)).map(|x| x.3)).or_else(|| self.defaults.float(name)).unwrap_or(0.0)
        };
        let rgb = |name: u32| [0u8, 1, 2].map(|i| f(name, i));
        Some(Transmission {
            absorption: rgb(j("absorption")),
            absorption_albedo: f(0xf108_8873, 255),
            scattering: rgb(j("scattering")),
            scattering_albedo: f(0x0004_82aa, 255),
            thickness: f(0x5ba3_3632, 255),
            thickness_mask: f(0x95f8_d114, 255),
            path_cap: f(0xa14d_ce4e, 255),
            anisotropy: f(j("thicknessanisotropy"), 255),
            back_front: f(0x0604_e865, 255),
            alpha_divisor: f(0xc391_fd51, 255),
            opaque_from: f(0xf920_ffb1, 255),
            profile: if prof.is_some() { id } else { 0 },
        })
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
/// to middle by 2g, else middle to light by 2(g - 0.5)) and mixes the
/// result with plain g by the colour's alpha.
pub fn stops(srgb: [u8; 3]) -> [[f32; 3]; 3] {
    let lin = srgb.map(to_linear);
    let v = lin[0].max(lin[1]).max(lin[2]);
    let light = lin.map(|c| if v > 0.0 { (0.55 * (c / v - 1.0) + 1.0) * (0.55 * v + 0.45) } else { 0.45 });
    [lin.map(|c| 0.1 * c), lin, light]
}

/// Weapon part types that finishes are listed under (`c763edb4` of a
/// variant record is the joaat of one of these): 53 of the 54 hashes in
/// use, covering all but 4 of the 1,301 variants (`060dc09e`, the fore
/// grips, is unnamed).
pub const PART_TYPES: &[&str] = &[
    "barrel", "receiver", "stock", "magazine", "frame", "trigger", "bolt", "pgrip", "muzzle", "grip", "slide", "optic", "handguard", "hguard", "dustcover", "body", "pump",
    "sight_front", "sight_rear", "sight", "scopemount", "slide_cover", "magwell", "magtube", "railmount", "safety", "lid", "charge", "ammo", "selector", "barrel_rail", "cheekriser", "sear",
    "frontrail", "front_rail", "railplate", "reel", "rod", "rail", "flashlight", "reflexmount", "heatshield", "accessory", "underbarrel", "under", "riser",
    "optics", "sight_optic", "optic_rail", "carry_handle", "mag_assist", "saddle", "accs",
];

/// The name of a part type hash (hex), when it is one of [`PART_TYPES`].
pub fn part_type_name(hex: &str) -> Option<&'static str> {
    PART_TYPES.iter().copied().find(|n| format!("{:08x}", crate::meta7a::joaat(n)) == hex)
}

/// Records of the weapon family files that are joined once every file is
/// read ([`Index::link_weapons`]); not stored.
#[derive(Debug, Clone, Default)]
struct WeaponLinks {
    /// Component record -> model name hash.
    comp_model: HashMap<u32, u32>,
    /// Part set `5ad6772f` -> its entries: (part type, component options).
    sets: HashMap<u32, Vec<(u32, Vec<u32>)>>,
    /// Weapon type `454c2e3b` -> part set.
    type_set: HashMap<u32, u32>,
    /// Weapon `bc92474a`: own part set (0 = the type's), type, mod list.
    weapons: Vec<(u32, u32, Vec<u32>)>,
    /// Mod `a5791c62` -> the records it lists (`c897af02`).
    mods: HashMap<u32, Vec<u32>>,
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
    /// Preset (hex) -> metalness and roughness per sub-material
    /// (`Library::surface`).
    pub surfaces: BTreeMap<String, Vec<[f32; 2]>>,
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
    /// Weapon model name hash (hex) -> the finishes it can take, per part
    /// type (hex), longest list first: (variant name hash hex, preset hex)
    /// in the order of the weapon's mod list (`link_weapons`).
    pub model_finishes: BTreeMap<String, Vec<(String, Vec<(String, String)>)>>,
    /// The game's parameter defaults, applied to every preset above.
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(skip)]
    links: WeaponLinks,
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
            self.surfaces.insert(format!("{k:08x}"), (0..8).map(|s| lib.surface(k, s).unwrap_or([0.0, 0.5])).collect());
        }
    }

    /// Weapon records of one decoded weapon family file (`Pso::to_json`).
    /// The records of a weapon are spread over several files; call
    /// [`Index::link_weapons`] after the last one.
    pub fn add_weapon_file(&mut self, pso_json: &Value) {
        let (comp, var, model, part) = (field_name(0x72f5_e33e), field_name(0x14b0_64c2), field_name(0x873e_f899), field_name(0xc763_edb4));
        let (set, mods, list, entry_part, option) = (field_name(0x20fa_4150), field_name(0x43ab_621f), field_name(0xc897_af02), field_name(0x5cce_ffc2), field_name(0xcac8_083d));
        let u = |v: &Value| v.as_u64().unwrap_or(0) as u32;
        let us = |v: &Value| v.as_array().into_iter().flatten().map(u).collect::<Vec<u32>>();
        for (st, objs) in pso_json["objects"].as_object().into_iter().flatten() {
            let st = st.get(st.len().saturating_sub(8)..);
            for o in objs.as_array().into_iter().flatten() {
                let (Some(n), preset) = (o["Name"].as_u64(), |f: &str| o[f].as_u64().filter(|&p| p != 0).map(|p| format!("{p:08x}"))) else { continue };
                match st {
                    Some("85492dc3") => {
                        if let Some(m) = o[&model].as_u64().filter(|&m| m != 0) {
                            self.links.comp_model.insert(n as u32, m as u32);
                        }
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
                            self.weapon_variants.insert(format!("{n:08x}"), (format!("{:08x}", u(&o[&part])), p));
                        }
                    }
                    Some("5ad6772f") => {
                        let entries = o["Entries"].as_array().into_iter().flatten().map(|e| (u(&e[&entry_part]), e["Options"].as_array().into_iter().flatten().map(|x| u(&x[&option])).collect())).collect();
                        self.links.sets.insert(n as u32, entries);
                    }
                    Some("454c2e3b") => {
                        self.links.type_set.insert(n as u32, u(&o[&set]));
                    }
                    Some("bc92474a") => self.links.weapons.push((u(&o[&set]), u(&o["WeaponType"]), us(&o[&mods]))),
                    Some("a5791c62") => {
                        self.links.mods.insert(n as u32, us(&o[&list]));
                    }
                    _ => {}
                }
            }
        }
    }

    /// Joins the weapon records into `model_finishes`: a weapon's part set
    /// (its own, else its type's) lists component options per part type,
    /// its mod list names mods, and a finish mod lists variant records, each
    /// for one part type. A variant is offered to the models of the
    /// components that the weapon's set lists under that part type.
    pub fn link_weapons(&mut self) {
        let l = std::mem::take(&mut self.links);
        let mut out: BTreeMap<String, Vec<(String, Vec<(String, String)>)>> = BTreeMap::new();
        for (set, ty, mods) in &l.weapons {
            let set = if *set != 0 { *set } else { l.type_set.get(ty).copied().unwrap_or(0) };
            let Some(entries) = l.sets.get(&set) else { continue };
            for x in mods.iter().filter_map(|m| l.mods.get(m)).flatten() {
                let var = format!("{x:08x}");
                let Some((slot, preset)) = self.weapon_variants.get(&var) else { continue };
                for model in entries.iter().filter(|e| format!("{:08x}", e.0) == *slot).flat_map(|e| &e.1).filter_map(|c| l.comp_model.get(c)) {
                    let slots = out.entry(format!("{model:08x}")).or_default();
                    let i = slots.iter().position(|s| s.0 == *slot).unwrap_or_else(|| {
                        slots.push((slot.clone(), Vec::new()));
                        slots.len() - 1
                    });
                    if !slots[i].1.iter().any(|f| f.0 == var) {
                        slots[i].1.push((var.clone(), preset.clone()));
                    }
                }
            }
        }
        for slots in out.values_mut() {
            slots.sort_by_key(|s| std::cmp::Reverse(s.1.len()));
        }
        self.model_finishes = out;
    }

    /// Reads the weapon family files of packs.rpf (PSO files with weapon,
    /// part set, component, mod or variant records) when that archive is
    /// indexed.
    pub fn add_weapon_files(&mut self, ws: &crate::export::Workspace) {
        let Some((pa, pix)) = ws.resolve("packs").ok().and_then(|pa| Some((ws.load_index(&pa).ok()?, pa))).map(|(i, a)| (a, i)) else { return };
        let tags = [0x8549_2dc3u32, 0x6724_161c, 0x5ad6_772f, 0x454c_2e3b, 0xbc92_474a, 0xa579_1c62].map(u32::to_le_bytes);
        for e in pix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Pso) {
            let Ok(b) = crate::scan::read_entry(&pa.path, e) else { continue };
            if b.windows(4).any(|w| tags.iter().any(|t| w == t)) {
                if let Some(p) = crate::pso::parse(&b) {
                    self.add_weapon_file(&p.to_json());
                }
            }
        }
        self.link_weapons();
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
        let mut index = Index { version: VERSION, defaults: Defaults::read(ws).unwrap_or_default(), ..Default::default() };
        for e in aix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Pso) {
            let Ok(b) = crate::scan::read_entry(&a.path, e) else { continue };
            let Some(p) = crate::pso::parse(&b).filter(|p| p.structs.contains_key(&ROOT)) else { continue };
            let mut lib = Library::from_meta(&p.meta());
            lib.defaults = index.defaults.clone();
            index.add(&e.id, &lib);
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
            let _ = crate::atomic::write(p, b);
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

    /// The finishes a weapon model can take, as (part type hex, [(variant
    /// hex, preset hex)]): the variants that the finish mods of a weapon
    /// list for the part type its part set gives the model's component
    /// (`link_weapons`). A model under several part types (27 of 621) gets
    /// its longest list. No finish mod is a weapon's default mod, so
    /// without one a part shows its component's own preset.
    pub fn weapon_finishes(&self, model: &str) -> Option<(&str, &[(String, String)])> {
        let m = model.to_ascii_lowercase();
        let base = ["_hi", "_mid"].iter().find_map(|s| m.strip_suffix(s)).unwrap_or(&m);
        let slots = self.model_finishes.get(&format!("{:08x}", crate::meta7a::joaat(base)))?;
        slots.first().map(|s| (s.0.as_str(), s.1.as_slice()))
    }

    /// Metalness and roughness per sub-material of a preset.
    pub fn surface(&self, preset: &str) -> Option<&[[f32; 2]]> {
        self.surfaces.get(preset).map(Vec::as_slice)
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
    fn a_lens_profile_fills_the_transmission_record_over_the_defaults() {
        let j = crate::meta7a::joaat;
        // the names the traced parser matches
        assert_eq!((j("alphathreshold"), j("absorptionalbedoblend"), j("scatteringalbedoblend")), (0xc391_fd51, 0xf108_8873, 0x0004_82aa));
        let mut lib = Library::default();
        for (k, v) in [(j("absorption"), 500.0), (j("scattering"), 100.0), (0x5ba3_3632, 0.02), (0x0604_e865, -0.1), (0xc391_fd51, 0.1), (0xf920_ffb1, 0.9)] {
            lib.defaults.floats.insert(format!("{k:08x}"), v);
        }
        // a glasses preset: sub-material 0 a frame base without a profile, 3 the glass base with profile 0x55
        lib.base.insert(0x11, Record::default());
        let mut glass = Record::default();
        glass.profiles[0] = 0x55;
        lib.base.insert(0x12, glass);
        let mut preset = Record::default();
        preset.bases = [0x11, 0x11, 0x11, 0x12, 0, 0, 0, 0];
        lib.presets.insert(0x22, preset);
        let mut prof = Record::default();
        prof.floats = vec![(j("absorption"), 0, 255, 5.0), (j("absorption"), 1, 255, 5.0), (j("absorption"), 2, 255, 4.0), (j("scattering"), 255, 255, 0.01), (j("thicknessanisotropy"), 255, 255, 10.79), (0xa14d_ce4e, 255, 255, 11.67)];
        lib.profiles.insert(0x55, prof);
        let t = lib.transmission(0x22, 3).unwrap();
        assert_eq!(t.profile, 0x55);
        assert_eq!((t.absorption, t.scattering), ([5.0, 5.0, 4.0], [0.01; 3]));
        assert_eq!((t.anisotropy, t.path_cap), (10.79, 11.67));
        // left to the defaults, or 0 without one
        assert_eq!((t.thickness, t.thickness_mask, t.back_front, t.alpha_divisor, t.opaque_from, t.absorption_albedo), (0.02, 0.0, -0.1, 0.1, 0.9, 0.0));
        // no profile: the all-defaults record
        let d = lib.transmission(0x22, 0).unwrap();
        assert_eq!((d.profile, d.absorption, d.scattering, d.path_cap), (0, [500.0; 3], [100.0; 3], 0.0));
        // a preset's own profile wins over its base's
        lib.presets.get_mut(&0x22).unwrap().profiles[3] = 0x66;
        let mut other = Record::default();
        other.floats = vec![(j("absorption"), 255, 255, 1.0)];
        lib.profiles.insert(0x66, other);
        assert_eq!(lib.transmission(0x22, 3).unwrap().absorption, [1.0; 3]);
        assert_eq!(lib.transmission(0x99, 3), None);
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
        // metalness and roughness: preset value, else base, else the tile's metallic, else 0 / 0.5
        assert_eq!(lib.surface(0x22, 0), Some([0.0, 0.5]));
        lib.tiles.get_mut(&0x33).unwrap().floats.push((j("tilemetallic"), 255, 255, 0.95));
        lib.base.get_mut(&0x11).unwrap().floats.push((ROUGHNESS, 255, 255, 0.75));
        lib.presets.get_mut(&0x22).unwrap().floats.push((METALLIC, 255, 1, 1.0));
        assert_eq!(lib.surface(0x22, 0), Some([0.95, 0.75]));
        assert_eq!(lib.surface(0x22, 1), Some([1.0, 0.75]));
        lib.tiles.get_mut(&0x33).unwrap().floats.pop();
        // tint-mask layers: the base material has layer 1 (mask, opacity, tint); the preset overrides its opacity and tint for sub-material 0 and inverts it
        let b = lib.base.get_mut(&0x11).unwrap();
        b.textures.push((j("tintmaskmap"), 1, 255, 0xcc));
        b.textures.push((j("tintmaskmap"), 2, 255, 0xdd)); // no opacity: not a layer
        b.floats.push((j("tintmaskopacity"), 1, 255, 0.5));
        b.floats.push((j("tintmaskscale"), 2, 255, 4.0));
        b.colours.push((j("tintmasktint"), 1, 255, 0xff00_00ff));
        let p = lib.presets.get_mut(&0x22).unwrap();
        p.floats.push((j("tintmaskopacity"), 1, 0, 1.0));
        p.colours.push((j("tintmasktint"), 5, 0, 0xff00_ff00));
        p.ints.push((j("tintmaskflags"), 1, 0, 12));
        let l0 = lib.layers(0x22, 0);
        assert_eq!(l0.len(), 1);
        assert_eq!((l0[0].layer, l0[0].mask_map.as_str(), l0[0].opacity, l0[0].invert, l0[0].tint, l0[0].tint2, l0[0].scale), (1, "00000000000000cc", 1.0, true, Some([255, 0, 0, 255]), Some([0, 255, 0, 255]), [Some(4.0), None]));
        let l1 = lib.layers(0x22, 1);
        assert_eq!((l1[0].opacity, l1[0].invert, l1[0].tint2), (0.5, false, None));
        assert!(lib.layers(0x22, 2).is_empty());
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
    fn missing_parameters_take_the_game_defaults() {
        let j = crate::meta7a::joaat;
        let kv = |k: u32, v: Value| json!({"key": k, "value": v});
        let root = json!({
            field_name(0x1652_fce8): [kv(j("tintmaskopacity"), json!(1.0)), kv(j("tintmaskscale"), json!(1.0)), kv(j("tilescale"), json!(1.0)), kv(ROUGHNESS, json!(0.0))],
            field_name(0x9f6b_5b03): [kv(j("tintmaskflags"), json!(8))],
            field_name(0xfc24_d9c5): [kv(COLOUR, json!(0))],
            field_name(0x0f20_8402): [kv(j("tintmaskmap"), json!("0000000000000000"))],
        });
        let d = Defaults::from_root(&root);
        assert_eq!((d.float(j("tintmaskscale")), d.int(j("tintmaskflags")), d.colour(COLOUR), d.texture(j("tintmaskmap")), d.float(0x1234)), (Some(1.0), Some(8), Some(0), Some(0), None));
        let mut lib = Library { defaults: d, ..Default::default() };
        let base = json!({field_name(0x0f20_8402): [param(j("tintmaskmap"), 0, 255, json!("00000000000000ee"))]});
        lib.base.insert(0x11, record(&base));
        let preset = json!({field_name(0x4a1e_bc36): [0x11u32, 0, 0, 0, 0, 0, 0, 0], field_name(0x1652_fce8): [param(j("tintmaskscale"), 1, 0, json!(3.0))]});
        lib.presets.insert(0x22, record(&preset));
        lib.tiles.insert(0x33, Record::default());
        lib.base.get_mut(&0x11).unwrap().tiles[0] = 0x33;
        // a mask with no opacity, scale or flags of its own takes opacity 1, scale 1 (entry 1 is the preset's) and flags 8
        let l = lib.layers(0x22, 0);
        assert_eq!((l.len(), l[0].opacity, l[0].scale, l[0].flags, l[0].invert, l[0].rotation), (1, 1.0, [Some(1.0), Some(3.0)], 8, false, None));
        assert_eq!(lib.tile(0x22, 0).map(|t| (t.scale, t.rotation)), Some((Some(1.0), None)));
        // roughness stays 0.5 when missing, although the file's default is 0
        assert_eq!(lib.surface(0x22, 0), Some([0.0, 0.5]));
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
        // finishes: weapon -> part set (own, else its type's) -> component -> model; weapon -> mods -> variants of a part type
        let jj = crate::meta7a::joaat;
        let (set, mods, list, epart, opt) = (field_name(0x20fa_4150), field_name(0x43ab_621f), field_name(0xc897_af02), field_name(0x5cce_ffc2), field_name(0xcac8_083d));
        let entry = |part: &str, comps: &[u32]| json!({&epart: jj(part), "Options": comps.iter().map(|c| json!({&opt: c})).collect::<Vec<_>>()});
        let a = json!({"objects": {
            "9a16508d85492dc3": [{"Name": 1, field_name(0x72f5_e33e): 0x10u32, field_name(0x873e_f899): jj("w_pi_duke_01_slide_02")}, {"Name": 2, field_name(0x873e_f899): jj("w_pi_duke_01_mag_01")},
                {"Name": 3, field_name(0x72f5_e33e): 0x10u32, field_name(0x873e_f899): jj("w_pi_other_01_slide_01")}],
            "c74572e55ad6772f": [{"Name": 0x51u32, "Entries": [entry("slide", &[1]), entry("magazine", &[2]), entry("frame", &[2])]}, {"Name": 0x52u32, "Entries": [entry("slide", &[3])]}],
            "000000006724161c": [
                {"Name": 0xa1u32, field_name(0x14b0_64c2): 0x20u32, field_name(0xc763_edb4): jj("slide")},
                {"Name": 0xa2u32, field_name(0x14b0_64c2): 0x21u32, field_name(0xc763_edb4): jj("slide")},
                {"Name": 0xa3u32, field_name(0x14b0_64c2): 0x22u32, field_name(0xc763_edb4): jj("magazine")},
                {"Name": 0xa4u32, field_name(0x14b0_64c2): 0x23u32, field_name(0xc763_edb4): jj("frame")},
                {"Name": 0xa5u32, field_name(0x14b0_64c2): 0x24u32, field_name(0xc763_edb4): jj("frame")},
                {"Name": 0xa6u32, field_name(0x14b0_64c2): 0x25u32, field_name(0xc763_edb4): jj("barrel")}],
        }});
        // the weapon, its type and its mods sit in another file
        let b = json!({"objects": {
            "00000000454c2e3b": [{"Name": 0x71u32, &set: 0x51u32}],
            "00000000bc92474a": [{"Name": 0x81u32, &set: 0, "WeaponType": 0x71u32, &mods: [0x91u32, 0x92u32, 0x93u32, 0x99u32]}, {"Name": 0x82u32, &set: 0x52u32, "WeaponType": 0x71u32, &mods: [0x94u32]}],
            "00000000a5791c62": [{"Name": 0x91u32, &list: [0xa2u32]}, {"Name": 0x92u32, &list: [0xa1u32, 0xa3u32]}, {"Name": 0x93u32, &list: [0xa4u32, 0xa5u32, 0xa6u32, 0xa2u32]}, {"Name": 0x94u32, &list: []}],
        }});
        ix.add_weapon_file(&a);
        ix.add_weapon_file(&b);
        ix.link_weapons();
        // in the order of the weapon's mod list, each variant once
        let (slot, f) = ix.weapon_finishes("w_pi_duke_01_slide_02_hi").unwrap();
        assert_eq!((slot, f.iter().map(|f| f.1.as_str()).collect::<Vec<_>>()), (format!("{:08x}", jj("slide")).as_str(), vec!["00000021", "00000020"]));
        // a model under two part types gets the longer list; the model's own name plays no part
        let (slot, f) = ix.weapon_finishes("w_pi_duke_01_mag_01").unwrap();
        assert_eq!((slot, f.len()), (format!("{:08x}", jj("frame")).as_str(), 2));
        assert_eq!(ix.model_finishes[&format!("{:08x}", jj("w_pi_duke_01_mag_01"))][1].1[0].1, "00000022");
        // a weapon whose mods list no finish offers none; a part type no entry has is dropped
        assert!(ix.weapon_finishes("w_pi_other_01_slide_01").is_none());
        assert!(ix.weapon_finishes("w_ar_adb_01_barrel_01").is_none());
        assert_eq!((part_type_name(&format!("{:08x}", jj("sight_rear"))), part_type_name("12345678")), (Some("sight_rear"), None));
    }
}
