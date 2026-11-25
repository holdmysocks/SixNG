//! Vehicle part colours: what each of the 256 part ids of a vehicle's
//! two-layer materials looks like.
//!
//! The body, interior and mechanical meshes of a vehicle share a few
//! `standard_vehicle_2lyr*` materials; each vertex carries a part id (0-255)
//! in the alpha of its vertex colour. Three things give that id a look:
//!
//! - A **part set** (`f14244fd`, one per vehicle body or mod part model, in
//!   its own `0x7a` resource in `packs.rpf`): two 256-entry tables, one
//!   naming a colour record per part id (`8b464cf4.2efff0ae[i].4f9f7603`),
//!   one naming a surface record (`578f6a03.4e3e21e7[i].8576d517`). Its
//!   `name` is a 64-bit value that the vehicle's variation sets
//!   (`fc1c7000`) and mod part elements (`77f2b89e.8605bf0e`) carry.
//! - A **library** (one `0x7a` resource in `data.rpf`) with the records the
//!   names resolve to: colour records `f1379639` (`colour`, a row
//!   `c8313558` and a page `adb52ec3`), surface records `9e4de959` (their
//!   `2cc72f7c` names a `307f162c` record with `metalLow` / `metalHigh` /
//!   `roughnessLow` / `roughnessHigh`; a second one, `174770cc`, is not
//!   used here: which applies where is not known).
//! - The **material palette** texture (slot `19df25d7`, the same texture on
//!   every vehicle): an array of 8 slices of 128 x 256. A colour record's
//!   page is the slice and its row the row (every one of the 1,851 records'
//!   rows is non-black in its slice). Pages 1 and 3 are the ones in use;
//!   their rows run bright (left) to dark (right).
//!
//! Colours (`type 0x15`) are stored with red in the low byte: the 151 named
//! colours `4ce375e6` of the library start with GTA IV's car colour list
//! (37,37,39 / 101,106,121 / 88,89,90 ...) only when read that way.
//!
//! A configuration's part list names records in the library (`0caef606`),
//! each with a list of modifier records. Three kinds are applied here, for
//! the result most of a model's configurations give: group definitions, bindings of a group to
//! a channel, and assignments of a colour and surface record to a channel
//! (see the constants). Body panels are bound to one channel that nothing
//! assigns; it is taken as the body paint.
//!
//! What the paint shader does with these was read from its machine code
//! (the G-buffer pixel shader of `standard_vehicle_2lyr`): the position
//! along a palette row is the green channel of `lyr0diffusetex` (plus a
//! small detail term), the palette is only read where the part's own colour
//! is zero (a record with a colour never reads its row), and metalness and
//! roughness are low / high pairs of the part's surface blended by the red
//! and green channels of `lyr0materialatex`. An export has one value of
//! each of those three texture channels per model (the textures' means),
//! not one per pixel.
//!
//! A third record gives a part its **coat**: the layers of a paint (clear
//! coat, flakes, thin film, sheen, anisotropy). Coat records (`2b84f07e`,
//! 92 in the library) are named by `paintHash` next to a part id's colour
//! record in the part set, and per configuration by the paint type parts:
//! a modifier `1f6c6730` gives a channel a coat (`paintType`), an
//! assignment a default one (`defaultPaintType`), and a binding with its
//! `paintType` flag set passes the channel's coat to its group of part ids.
//! That the paint type modifier wins over the assignment's default, and
//! what the shader does with each value, is not checked in code.
//!
//! Not decoded: the other modifier kinds (dirt, liveries, decal layers),
//! which configuration a spawned vehicle uses, the wear layers (a colour
//! record's second and third picks, mostly on page 5).

use crate::meta7a::{self, Meta};
use crate::resource::Resource;
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// Version of what an export writes for part colours (model.json
/// `part_colours_version`); the model viewer exports older vehicles again.
/// 21: the second colour stream (`_COLOR1`, sticker zones) is in the GLB.
/// 22: `part_patches` (what a fitted part changes).
/// 23: `sliding_doors` in model.json.
/// 24: the tyre colour slot is left unapplied (tyres stay black).
/// 25: coat records (`coats`, `coat_records`, `paint_coats`, `paint_coat`).
pub const EXPORT_VERSION: u32 = 25;
/// Layout version of `<work>/index/vehicle_parts.json`.
/// 21: a part's coat and the library's coat records.
pub const FORMAT: u32 = 21;

const PART_SET: u32 = 0xf142_44fd;
const SET_COLOURS: u32 = 0x7847_6477;
const SET_SURFACES: u32 = 0x9ac2_b332;
const COLOUR_ENTRIES: u32 = 0x2eff_f0ae;
const ENTRY_COLOUR: u32 = 0x4f9f_7603;
const SURFACE_ENTRIES: u32 = 0x4e3e_21e7;
const ENTRY_SURFACE: u32 = 0x8576_d517;
const COLOUR_REC: u32 = 0xf137_9639;
const COLOUR: u32 = 0x5bdc_13e3;
const COLOUR_MODE: u32 = 0xadb5_2ec3;
const COLOUR_ROW: u32 = 0xc831_3558;
const SURFACE: u32 = 0x9e4d_e959;
const SURFACE_ROUGH: u32 = 0x2cc7_2f7c;
/// A surface's second layer: its colour record and its `307f162c` record.
const SURFACE_WEAR_COLOUR: u32 = 0x5cff_e5e0;
const SURFACE_WEAR_ROUGH: u32 = 0x1747_70cc;
const ROUGH: u32 = 0x307f_162c;
const ROUGH_LOW: u32 = 0xe4be_3b70;
const ROUGH_HIGH: u32 = 0x26e5_d460;
/// Metalness low and high of a `307f162c` record. The paint shader blends a
/// low / high pair by the red channel of the layer's material texture and
/// the roughness pair by its green channel; these two fields are 0 on
/// plastics, rubber, leather and paint and 0.7 to 1.0 on chrome and alloy.
const METAL_LOW: u32 = 0xf763_652d;
const METAL_HIGH: u32 = 0x09dd_33c1;
const NAME: u32 = 0xe6e1_7f14;
/// A coat record of the library and the fields that name one: `paintHash`
/// of a part set's colour entry, `defaultPaintType` of an assignment,
/// `paintType` of a paint type modifier (`1f6c6730`: `group` = channel) and,
/// as a flag, of a binding.
const COAT: u32 = 0x2b84_f07e;
const ENTRY_COAT: u32 = 0x9647_9496;
const ASSIGN_COAT: u32 = 0xa816_7b82;
const PAINT_TYPE: u32 = 0x1f6c_6730;
const PAINT_TYPE_COAT: u32 = 0xdb4b_dec4;
/// The coat record most parts name (`default`: no clear coat, no flakes).
pub const DEFAULT_COAT: u32 = 0xe4df_46d5;
/// A configuration's part list (`51019114.e89b8540`).
const CONFIG_PARTS: u32 = 0xe89b_8540;
/// A library record a configuration's part list names (`0caef606`): slot
/// `37a6f9ef` and a list `61e1e572` of modifier record names.
const MOD: u32 = 0x0cae_f606;
const MOD_SLOT: u32 = 0x37a6_f9ef;
const MOD_LIST: u32 = 0x61e1_e572;
/// A part's elements (names of `77f2b89e` records) and an element's model.
const MOD_ELEMENTS: u32 = 0x7e5d_1612;
const ELEMENT_MODEL: u32 = 0x8605_bf0e;
/// A named group of part ids in the library: `3acbce85` is a byte array of
/// part ids.
const GROUP: u32 = 0x0006_39e9;
const GROUP_IDS: u32 = 0x3acb_ce85;
/// A modifier that defines a group for one configuration part: `662d9b79`
/// group name, `e9bf9f2d` part ids.
const GROUP_DEF: u32 = 0x6225_734f;
const DEF_GROUP: u32 = 0x662d_9b79;
const DEF_IDS: u32 = 0xe9bf_9f2d;
/// A modifier that binds a group to a channel: `662d9b79` group, `178cc382`
/// channel, `1470edc0` priority, flags `c8d27281` (colour) and `7a84e7bf`
/// (surface).
const BIND: u32 = 0x501f_f112;
const BIND_CHANNEL: u32 = 0x178c_c382;
const PRIORITY: u32 = 0x1470_edc0;
const FLAG_COLOUR: u32 = 0xc8d2_7281;
const FLAG_SURFACE: u32 = 0x7a84_e7bf;
/// A modifier that gives a channel its look: `ca2065a9` channel, `2c89a8d2`
/// colour record, `7a84e7bf` surface record.
const ASSIGN: u32 = 0xb943_08f5;
const ASSIGN_CHANNEL: u32 = 0xca20_65a9;
const ASSIGN_COLOUR: u32 = 0x2c89_a8d2;
/// A modifier that lists parts to choose one from (`bf340aab`): paint
/// finishes, trims, wheels. Which one the game picks is not decoded; the
/// first is taken.
const OPTION_LIST: u32 = 0xf5e9_d7df;
const OPTION_PARTS: u32 = 0xbf34_0aab;
/// A modifier that names something else by type (`type.tag`): 5 is a
/// library part whose modifiers come along. The other types (0 to 4) are
/// not read.
const PART_REF: u32 = 0x51aa_b7c1;
const REF_TYPE: u32 = 0x3165_b05d;
const REF_DATA: u32 = 0x5eb5_e655;
/// The channel body panels are bound to (bonnet, boot, bumpers, doors, at
/// priority 1 over a default); no record assigns it a colour, so it is taken
/// as the body paint.
const PAINT_CHANNEL: u32 = 0x7ed7_6b9f;
/// The channel of the tyre colour slot (`f3bd038d`): ten library parts,
/// each binding the tyre's group (part id 35, the one id of every shared
/// `tyre_*` model) to this channel and assigning it one of ten colours
/// (greys, purple, magenta, greens, blue, yellow, orange, red). Five
/// vehicles fit one in some configurations (boxville, burrito, journey,
/// rentalbus, stanier). What the game does with it is not decoded; applied
/// to the tyre it paints the whole tyre red or light grey, so it is left
/// unapplied and the tyre keeps the rubber of its part set.
const TYRE_COLOUR_CHANNEL: u32 = 0x976f_e07e;
/// The slot of the wheel fitment record.
const SLOT_WHEEL_FIT: u32 = 0x39d5_e253;
/// A wheel fitment modifier: `3d37e0df` = rim radius / tyre radius (0.82 on
/// a sports car, 0.74 on a saloon, 0.65 on a truck); with `0341ff8f` set
/// the values are offsets added by other slots, which are not applied.
const WHEEL_FIT: u32 = 0x9a17_2293;
const FIT_RIM: u32 = 0x3d37_e0df;
const FIT_RELATIVE: u32 = 0x0341_ff8f;
const OWNER: u32 = 0xfb1f_80ff;
/// The colour record of the primary body paint (its stored colour is a
/// dark grey default; a configuration's body colour replaces it).
pub const PRIMARY_PAINT: u32 = 0x2640_0a11;

/// A coat record (`2b84f07e`, 56 bytes): the layers of a paint. Field names
/// are the game's (case-sensitive joaat of the member hashes), except the
/// two that are not named. Booleans are bytes, `base_coat_roughness` and
/// `value_9851c9f1` are f32, the other numbers half floats.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Coat {
    pub use_anisotropy: bool,
    /// 0 to 0.55.
    pub anisotropy_intensity: f32,
    /// `f6d2a18e` = `useClearCoat`.
    pub use_clear_coat: bool,
    /// 0 to 1.
    pub clear_coat_intensity: f32,
    /// 0 (gloss) to 0.7 (matte).
    pub clear_coat_roughness: f32,
    pub use_specular_color: bool,
    /// Not named; set on one record.
    pub flag_b97f4910: bool,
    pub use_flakes: bool,
    /// 0 or 1.
    pub flake_coverage: f32,
    /// 0 to 0.104.
    pub flake_size: f32,
    /// 0 to 0.1.
    pub flake_orientation: f32,
    /// `4bd9d63e` = `overrideBaseCoatRoughness`.
    pub override_base_coat_roughness: bool,
    /// 0.09 to 0.7.
    pub base_coat_roughness: f32,
    /// Not named; 1 on one record, else 0.
    pub value_9851c9f1: f32,
    /// False on every record.
    pub use_emissive: bool,
    /// `5f4aa231` = `filmIOR`: 1, or 2.3 / 2.6 on the records with a film.
    pub film_ior: f32,
    /// 0 (no film) to 2.75; the unit is not known.
    pub film_thickness: f32,
    /// 0 to 0.8.
    pub sheen_intensity: f32,
    pub sheen_fade: f32,
    pub sheen_reflectance: f32,
}

/// The coat records of a library, by name hash. Each member is found by its
/// name hash and read by its declared type, so a record with members moved
/// or missing still reads (a missing one stays 0 / false).
pub fn coats(m: &Meta) -> HashMap<u32, Coat> {
    let Some(sd) = m.structs.get(&COAT) else { return HashMap::new() };
    let r = m.r;
    let member = |h: u32| sd.members.iter().find(|x| x.hash == h);
    m.objects(COAT)
        .into_iter()
        .map(|o| {
            let num = |h: u32| match member(h) {
                Some(x) if x.ty == 0x20 => meta7a::half(r.u16(o + x.off as usize)),
                Some(x) if x.ty == 0x21 => r.f32(o + x.off as usize),
                _ => 0.0,
            };
            let flag = |h: u32| member(h).filter(|x| x.ty == 0x01).and_then(|x| r.sys.get(o + x.off as usize)).is_some_and(|b| *b != 0);
            let c = Coat {
                use_anisotropy: flag(0x4953_63e4),
                anisotropy_intensity: num(0xd926_02db),
                use_clear_coat: flag(0xf6d2_a18e),
                clear_coat_intensity: num(0xf031_0a8b),
                clear_coat_roughness: num(0xc5cb_49ba),
                use_specular_color: flag(0x61e8_2f14),
                flag_b97f4910: flag(0xb97f_4910),
                use_flakes: flag(0xe57c_6a65),
                flake_coverage: num(0xdf5b_7432),
                flake_size: num(0xef79_abb2),
                flake_orientation: num(0xa17a_dbe1),
                override_base_coat_roughness: flag(0x4bd9_d63e),
                base_coat_roughness: num(0xc920_19cc),
                value_9851c9f1: num(0x9851_c9f1),
                use_emissive: flag(0xc8d4_8032),
                film_ior: num(0x5f4a_a231),
                film_thickness: num(0x85d1_88cc),
                sheen_intensity: num(0x210d_19b3),
                sheen_fade: num(0x469c_3c58),
                sheen_reflectance: num(0x850e_331c),
            };
            (member(NAME).map_or(0, |x| r.u32(o + x.off as usize)), c)
        })
        .collect()
}

/// One part id of a part set.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Part {
    /// Name hash of the colour record (`f1379639`).
    pub colour: u32,
    /// Name hash of the surface record (`9e4de959`).
    pub surface: u32,
    /// Row of the material palette, or -1.
    pub row: i16,
    /// The colour record's mode: 3 = a fixed material whose colours are the
    /// stored palette row, 1 = a paint whose row the game fills from `rgb`.
    pub mode: i8,
    /// The colour record's colour (sRGB bytes).
    pub rgb: [u8; 3],
    /// `roughnessLow`, `roughnessHigh` of the surface's first layer.
    pub rough: [f32; 2],
    /// Metalness low and high of the surface's first layer (`307f162c`
    /// fields `f763652d`, `09dd33c1`).
    #[serde(default)]
    pub metal: [f32; 2],
    /// The surface's second layer, which shows where the first is worn:
    /// its palette row (slice x 256 + row; the surface's `5cffe5e0` names a
    /// colour record), None when that record has no row.
    #[serde(default)]
    pub wear_row: Option<u16>,
    /// Roughness and metalness, low and high, of the second layer (the
    /// surface's `174770cc` names a `307f162c` record).
    #[serde(default)]
    pub wear_rough: [f32; 2],
    #[serde(default)]
    pub wear_metal: [f32; 2],
    /// Name hash of the coat record (`2b84f07e`, see `Tables::coats`); 0
    /// when the part set names none.
    #[serde(default)]
    pub coat: u32,
}

impl Part {
    pub fn is_paint(&self) -> bool {
        self.mode != 3
    }
    pub fn is_primary(&self) -> bool {
        self.colour == PRIMARY_PAINT
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Tables {
    pub format: u32,
    /// Part set name (16 hex digits) -> 256 parts.
    pub sets: HashMap<String, Vec<Part>>,
    /// joaat of a vehicle model name (8 hex digits) -> part set name.
    pub models: HashMap<String, String>,
    /// joaat of a mod part model name -> part set name.
    pub elements: HashMap<String, String>,
    pub library: Option<String>,
    /// Name of a configuration part (8 hex digits) -> rim radius / tyre
    /// radius, for the wheel fitment records.
    #[serde(default)]
    pub rim_ratio: HashMap<String, f32>,
    /// Model (8 hex digits) -> configuration key (`config_key` of its part
    /// list) -> the part set that configuration gives.
    #[serde(default)]
    pub config_sets: HashMap<String, HashMap<String, String>>,
    /// Asset id of a mod part's model (`meta7a::asset_id`, 16 hex digits)
    /// -> its vehicle (8 hex digits): the part's vertices carry part ids of
    /// the vehicle's part set (a pickup's bed, bumpers, bonnets ...).
    #[serde(default)]
    pub part_owner: HashMap<String, String>,
    /// Model of a library part (joaat of its name, 8 hex digits: a truck
    /// bed or box, a light bar ...) -> the part set of a configuration that
    /// names the part. Such a model is fitted to several vehicles and its
    /// vertices carry part ids of whichever one it is on; on its own it
    /// takes the set of the first configuration found (vehicles in order
    /// of their name hash).
    #[serde(default)]
    pub shared_parts: HashMap<String, String>,
    /// Model (8 hex digits) -> one of its own parts (8 hex digits) -> what
    /// fitting that part on top of the model's usual configuration changes:
    /// (part id, its new colour and surface). A body kit's part rebinds the
    /// ids of its panels to the body paint this way.
    #[serde(default)]
    pub part_patches: HashMap<String, HashMap<String, Vec<(u8, Part)>>>,
    /// The library's coat records by name (8 hex digits): what `Part::coat`
    /// names.
    #[serde(default)]
    pub coats: HashMap<String, Coat>,
}

/// A key for a configuration: FNV-1a of its part list (the name hashes in
/// order), 16 hex digits. vehicles.json writes it as `parts_key`.
pub fn config_key(parts: &[u32]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in p.to_le_bytes() {
            h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

fn get<'v>(v: &'v Value, h: u32) -> Option<&'v Value> {
    v.get(meta7a::field_name(h))
}

fn num(v: &Value, h: u32) -> Option<i64> {
    get(v, h).and_then(|x| x.as_i64())
}

fn metas(game_root: &Path, work: &Path, archive: &str) -> Result<(std::path::PathBuf, Vec<crate::scan::Entry>)> {
    let ixp = crate::scan::index_path(&work.join("index"), archive);
    let ix = crate::scan::load_index(&ixp).map_err(|e| anyhow!("{archive} index ({}): {e}; index it first", ixp.display()))?;
    let es = ix.entries.into_iter().filter(|e| e.kind == crate::classify::Kind::Resource && e.sub == "7a").collect();
    Ok((game_root.join(archive), es))
}

struct Library {
    /// name -> (mode, row, rgb)
    colours: HashMap<u32, (i8, i16, [u8; 3])>,
    /// surface name -> roughness record name
    surfaces: HashMap<u32, u32>,
    /// surface name -> (colour record, roughness record) of its second layer
    wear: HashMap<u32, (u32, u32)>,
    rough: HashMap<u32, [f32; 2]>,
    /// roughness record name -> metalness low, high
    metal: HashMap<u32, [f32; 2]>,
    rim_ratio: HashMap<String, f32>,
    /// configuration part name -> its modifier record names
    mods: HashMap<u32, Vec<u32>>,
    /// group name -> part ids
    groups: HashMap<u32, Vec<u8>>,
    /// modifier name -> (group, part ids)
    group_defs: HashMap<u32, (u32, Vec<u8>)>,
    /// modifier name -> (group, channel, priority, colour, surface, coat)
    binds: HashMap<u32, (u32, u32, i64, bool, bool, bool)>,
    /// modifier name -> (channel, colour record, surface record, coat record)
    assigns: HashMap<u32, (u32, u32, u32, u32)>,
    /// paint type modifier name -> (channel, coat record)
    paint_types: HashMap<u32, (u32, u32)>,
    coats: HashMap<u32, Coat>,
    /// modifier name -> the parts of an option list (`f5e9d7df.bf340aab`)
    options: HashMap<u32, Vec<u32>>,
    /// modifier name -> the library part it brings along (`51aab7c1` with
    /// type 5): a body kit's part names the colour scheme part that binds
    /// the kit's panels to the body paint
    refs: HashMap<u32, u32>,
    /// configuration part name -> the models of its elements (joaat)
    part_models: HashMap<u32, Vec<u32>>,
    id: Option<String>,
}

impl Library {
    /// The second layer of a surface: palette row, roughness, metalness.
    fn wear_of(&self, surface: u32) -> (Option<u16>, [f32; 2], [f32; 2]) {
        let Some(&(c, r)) = self.wear.get(&surface) else { return (None, [0.5, 0.5], [0.0, 0.0]) };
        let row = self.colours.get(&c).filter(|(mode, row, _)| (0..8).contains(mode) && *row >= 0).map(|(mode, row, _)| *mode as u16 * 256 + (*row as u16 & 255));
        (row, self.rough.get(&r).copied().unwrap_or([0.5, 0.5]), self.metal.get(&r).copied().unwrap_or([0.0, 0.0]))
    }
}

fn library(game_root: &Path, work: &Path) -> Result<Library> {
    let (path, es) = metas(game_root, work, "data.rpf")?;
    let found: Vec<Library> = es
        .par_iter()
        .filter_map(|e| {
            let r = Resource::load_sys(&path, e).ok()?;
            let m = Meta::parse(&r)?;
            if !m.structs.contains_key(&COLOUR_REC) {
                return None;
            }
            let m = &m;
            let dump = |h: u32| m.objects(h).into_iter().filter_map(|o| m.object(h, o, 0)).collect::<Vec<_>>();
            let mut l = Library { colours: HashMap::new(), surfaces: HashMap::new(), wear: HashMap::new(), rough: HashMap::new(), metal: HashMap::new(), rim_ratio: HashMap::new(), mods: HashMap::new(), groups: HashMap::new(), group_defs: HashMap::new(), binds: HashMap::new(), assigns: HashMap::new(), paint_types: HashMap::new(), coats: coats(m), options: HashMap::new(), refs: HashMap::new(), part_models: HashMap::new(), id: Some(e.id.clone()) };
            for o in dump(OPTION_LIST) {
                if let Some(n) = num(&o, NAME) {
                    l.options.insert(n as u32, get(&o, OPTION_PARTS).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as u32)).collect()).unwrap_or_default());
                }
            }
            let ids = |v: &Value, h: u32| get(v, h).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as u8)).collect::<Vec<u8>>()).unwrap_or_default();
            let flag = |v: &Value, h: u32| get(v, h).and_then(|x| x.as_bool()).unwrap_or(false);
            // group part ids are a byte array: pool reference, then count (u16)
            if let Some(off) = m.structs.get(&GROUP).and_then(|s| s.members.iter().find(|x| x.hash == GROUP_IDS)).map(|x| x.off as usize) {
                let name_off = m.structs[&GROUP].members.iter().find(|x| x.hash == NAME).map_or(0, |x| x.off as usize);
                for o in m.objects(GROUP) {
                    let n = r.u16(o + off + 8) as usize;
                    if let Some((_, p)) = m.ptr(r.u64(o + off)).filter(|_| n > 0) {
                        if let Some(b) = r.sys.get(p..p + n) {
                            l.groups.insert(r.u32(o + name_off), b.to_vec());
                        }
                    }
                }
            }
            for o in dump(PART_REF) {
                // by member hash: `data` is not among the built-in field names, so
                // by name these read only with the name dictionary loaded
                if let (Some(n), Some(5), Some(p)) = (num(&o, NAME), get(&o, REF_TYPE).and_then(|t| t["tag"].as_u64()), num(&o, REF_DATA)) {
                    l.refs.insert(n as u32, p as u32);
                }
            }
            for g in dump(GROUP_DEF) {
                if let (Some(n), Some(gr)) = (num(&g, NAME), num(&g, DEF_GROUP)) {
                    l.group_defs.insert(n as u32, (gr as u32, ids(&g, DEF_IDS)));
                }
            }
            for b in dump(BIND) {
                if let (Some(n), Some(gr), Some(ch)) = (num(&b, NAME), num(&b, DEF_GROUP), num(&b, BIND_CHANNEL)) {
                    l.binds.insert(n as u32, (gr as u32, ch as u32, num(&b, PRIORITY).unwrap_or(0), flag(&b, FLAG_COLOUR), flag(&b, FLAG_SURFACE), flag(&b, PAINT_TYPE_COAT)));
                }
            }
            for a in dump(ASSIGN) {
                if let (Some(n), Some(ch)) = (num(&a, NAME), num(&a, ASSIGN_CHANNEL)) {
                    l.assigns.insert(n as u32, (ch as u32, num(&a, ASSIGN_COLOUR).unwrap_or(0) as u32, num(&a, FLAG_SURFACE).unwrap_or(0) as u32, num(&a, ASSIGN_COAT).unwrap_or(0) as u32));
                }
            }
            for t in dump(PAINT_TYPE) {
                if let (Some(n), Some(ch), Some(c)) = (num(&t, NAME), num(&t, ASSIGN_CHANNEL), num(&t, PAINT_TYPE_COAT)) {
                    l.paint_types.insert(n as u32, (ch as u32, c as u32));
                }
            }
            let fits: HashMap<u32, f32> = dump(WHEEL_FIT)
                .iter()
                .filter(|f| get(f, FIT_RELATIVE).and_then(|x| x.as_bool()) != Some(true))
                .filter_map(|f| Some((num(f, NAME)? as u32, get(f, FIT_RIM)?.as_f64()? as f32)))
                .filter(|(_, r)| *r > 0.2 && *r < 1.0)
                .collect();
            // an element's model: a 64-bit value whose low half is the
            // joaat of the model's name (16 hex digits), or that number
            let models: HashMap<u32, u32> = dump(crate::vehicles::MOD_ELEMENT)
                .iter()
                .filter_map(|e| {
                    let m = get(e, ELEMENT_MODEL)?;
                    let m = m.as_str().and_then(|s| u32::from_str_radix(s.get(8..16)?, 16).ok()).or_else(|| m.as_u64().map(|v| v as u32))?;
                    Some((num(e, NAME)? as u32, m))
                })
                .collect();
            for p in dump(MOD) {
                let list = get(&p, MOD_LIST).and_then(|x| x.as_array()).cloned().unwrap_or_default();
                if let Some(n) = num(&p, NAME) {
                    l.mods.insert(n as u32, list.iter().filter_map(|x| x.as_u64().map(|x| x as u32)).collect());
                    let of: Vec<u32> = get(&p, MOD_ELEMENTS).and_then(|x| x.as_array()).into_iter().flatten().filter_map(|x| models.get(&(x.as_u64()? as u32)).copied()).filter(|m| *m != 0).collect();
                    if !of.is_empty() {
                        l.part_models.insert(n as u32, of);
                    }
                }
                if num(&p, MOD_SLOT) != Some(SLOT_WHEEL_FIT as i64) {
                    continue;
                }
                if let (Some(n), Some(r)) = (num(&p, NAME), list.iter().filter_map(|x| fits.get(&(x.as_u64()? as u32))).next()) {
                    l.rim_ratio.insert(format!("{:08x}", n as u32), *r);
                }
            }
            for c in dump(COLOUR_REC) {
                let (Some(n), Some(col)) = (num(&c, NAME), num(&c, COLOUR)) else { continue };
                let col = col as u32;
                l.colours.insert(n as u32, (num(&c, COLOUR_MODE).unwrap_or(-1) as i8, num(&c, COLOUR_ROW).unwrap_or(-1) as i16, [col as u8, (col >> 8) as u8, (col >> 16) as u8]));
            }
            for s in dump(SURFACE) {
                if let (Some(n), Some(r)) = (num(&s, NAME), num(&s, SURFACE_ROUGH)) {
                    l.surfaces.insert(n as u32, r as u32);
                }
                if let (Some(n), Some(c), Some(r)) = (num(&s, NAME), num(&s, SURFACE_WEAR_COLOUR), num(&s, SURFACE_WEAR_ROUGH)) {
                    l.wear.insert(n as u32, (c as u32, r as u32));
                }
            }
            for s in dump(ROUGH) {
                let f = |h| get(&s, h).and_then(|x| x.as_f64()).unwrap_or(0.5) as f32;
                if let Some(n) = num(&s, NAME) {
                    l.rough.insert(n as u32, [f(ROUGH_LOW), f(ROUGH_HIGH)]);
                    let g = |h| get(&s, h).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                    l.metal.insert(n as u32, [g(METAL_LOW), g(METAL_HIGH)]);
                }
            }
            Some(l)
        })
        .collect();
    found.into_iter().max_by_key(|l| l.colours.len()).ok_or_else(|| anyhow!("no colour library (struct f1379639) in data.rpf"))
}

/// Read the part sets, the library and the links from the game files.
pub fn build(game_root: &Path, work: &Path) -> Result<Tables> {
    let lib = library(game_root, work)?;
    let (path, es) = metas(game_root, work, "packs.rpf")?;
    // (sets, model links, element links) per resource
    type Found = (Vec<(String, Vec<Part>)>, Vec<(u32, Vec<String>)>, Vec<(u32, Vec<String>)>, Vec<(u32, Vec<u32>)>, HashMap<u32, Vec<u32>>, bool);
    let found: Vec<Found> = es
        .par_iter()
        .filter_map(|e| {
            let r = Resource::load_sys(&path, e).ok()?;
            let m = Meta::parse(&r)?;
            let has = |h: u32| m.structs.contains_key(&h);
            if !has(PART_SET) && !has(crate::vehicles::MODEL_INFO) {
                return None;
            }
            let mut sets = Vec::new();
            for o in m.objects(PART_SET) {
                let Some(v) = m.object(PART_SET, o, 3) else { continue };
                let Some(name) = get(&v, NAME).and_then(|x| x.as_str()) else { continue };
                let list = |outer: u32, inner: u32| get(&v, outer).and_then(|x| x.as_array()).and_then(|a| a.first()).and_then(|t| get(t, inner)).and_then(|x| x.as_array()).cloned().unwrap_or_default();
                let (cols, surfs) = (list(SET_COLOURS, COLOUR_ENTRIES), list(SET_SURFACES, SURFACE_ENTRIES));
                if cols.len() != 256 || surfs.len() != 256 {
                    continue;
                }
                let parts = cols
                    .iter()
                    .zip(&surfs)
                    .map(|(c, s)| {
                        let colour = num(c, ENTRY_COLOUR).unwrap_or(0) as u32;
                        let surface = num(s, ENTRY_SURFACE).unwrap_or(0) as u32;
                        let (mode, row, rgb) = lib.colours.get(&colour).copied().unwrap_or((-1, -1, [0; 3]));
                        let rec = lib.surfaces.get(&surface);
                        let rough = rec.and_then(|r| lib.rough.get(r)).copied().unwrap_or([0.5, 0.5]);
                        let metal = rec.and_then(|r| lib.metal.get(r)).copied().unwrap_or([0.0, 0.0]);
                        let (wear_row, wear_rough, wear_metal) = lib.wear_of(surface);
                        let coat = num(c, ENTRY_COAT).unwrap_or(0) as u32;
                        Part { colour, surface, row, mode, rgb, rough, metal, wear_row, wear_rough, wear_metal, coat }
                    })
                    .collect();
                sets.push((name.to_string(), parts));
            }
            // every 64-bit value of a record is a candidate link; the ones
            // that name a part set are kept once all sets are known
            let u64s = |v: &Value| v.as_object().into_iter().flatten().filter_map(|(_, x)| x.as_str()).filter(|s| s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit())).map(|s| s.to_string()).collect::<Vec<_>>();
            let links = |h: u32, key: u32| m.objects(h).into_iter().filter_map(|o| m.object(h, o, 0)).filter_map(|v| Some((num(&v, key)? as u32, u64s(&v)))).collect::<Vec<_>>();
            let single = m.objects(crate::vehicles::MODEL_INFO).len() == 1;
            // Variation sets and configurations name their vehicle, so they
            // are also read from the tables that hold several vehicles:
            // some vehicles (journey) have no pack of their own. Mod part
            // elements carry no owner and are a single vehicle's only.
            let models = links(crate::vehicles::VARIATION, OWNER);
            let elements = if single { links(crate::vehicles::MOD_ELEMENT, NAME) } else { Vec::new() };
            // the part list of every configuration, by model
            let mut configs: Vec<(u32, Vec<u32>)> = Vec::new();
            for c in m.objects(crate::vehicles::CONFIG).into_iter().filter_map(|o| m.object(crate::vehicles::CONFIG, o, 0)) {
                let Some(owner) = num(&c, OWNER) else { continue };
                configs.push((owner as u32, get(&c, CONFIG_PARTS).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as u32)).collect()).unwrap_or_default()));
            }
            // the vehicle's own part records (its body and mod part
            // elements): their modifier lists name the library's group
            // definitions for this vehicle
            let mut own: HashMap<u32, Vec<u32>> = HashMap::new();
            if single {
                for p in m.objects(MOD).into_iter().filter_map(|o| m.object(MOD, o, 0)) {
                    if let Some(n) = num(&p, NAME) {
                        own.insert(n as u32, get(&p, MOD_LIST).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as u32)).collect()).unwrap_or_default());
                    }
                }
            }
            Some((sets, models, elements, configs, own, single))
        })
        .collect();
    // a vehicle's own pack before the shared tables: its records win, and
    // each vehicle's configurations are worked through once
    let mut found = found;
    found.sort_by_key(|f| !f.5);
    let found: Vec<_> = found.into_iter().map(|f| (f.0, f.1, f.2, f.3, f.4)).collect();
    // A vehicle's configurations can sit in more than one resource (its
    // own pack and a shared table, or two packs): all of them, each part
    // list once, with the part records of its own packs.
    let mut by_owner: std::collections::BTreeMap<u32, (Vec<Vec<u32>>, HashMap<u32, Vec<u32>>)> = Default::default();
    for (_, _, _, configs, own) in &found {
        for (owner, parts) in configs {
            let e = by_owner.entry(*owner).or_default();
            if !e.0.contains(parts) {
                e.0.push(parts.clone());
            }
            for (k, v) in own {
                e.1.entry(*k).or_insert_with(|| v.clone());
            }
        }
    }
    let mut t = Tables { format: FORMAT, library: lib.id.clone(), rim_ratio: lib.rim_ratio.clone(), coats: lib.coats.iter().map(|(n, c)| (format!("{n:08x}"), *c)).collect(), ..Default::default() };
    for (sets, _, _, _, _) in &found {
        for (n, p) in sets {
            t.sets.insert(n.clone(), p.clone());
        }
    }
    for (_, models, elements, _, _) in &found {
        for (h, c) in models {
            if let Some(s) = c.iter().find(|s| t.sets.contains_key(*s)) {
                t.models.entry(format!("{h:08x}")).or_insert_with(|| s.clone());
            }
        }
        for (h, c) in elements {
            if let Some(s) = c.iter().find(|s| t.sets.contains_key(*s)) {
                t.elements.entry(format!("{h:08x}")).or_insert_with(|| s.clone());
            }
        }
    }
    // A single vehicle's pack: the models of its elements belong to the
    // vehicle. The chassis part has an element for the vehicle's own model,
    // whose asset id is the name of the vehicle's part set, which says
    // whose pack it is. Every 64-bit
    // value of an element is kept; only a model's asset id is looked up.
    // (a variation set without an owner is filed under 0: not a vehicle)
    let mut vehicles: Vec<(&String, &String)> = t.models.iter().filter(|(v, _)| v.as_str() != "00000000").collect();
    vehicles.sort();
    let mut by_asset: HashMap<&str, &String> = HashMap::new();
    for (v, set) in vehicles {
        by_asset.entry(set.split('+').next().unwrap_or(set)).or_insert(v);
    }
    for (_, _, elements, _, _) in &found {
        let Some(owner) = elements.iter().flat_map(|(_, c)| c.iter()).find_map(|a| by_asset.get(a.as_str())) else { continue };
        for a in elements.iter().flat_map(|(_, c)| c.iter()) {
            t.part_owner.entry(a.clone()).or_insert_with(|| (*owner).clone());
        }

    }
    // A configuration reassigns colours and surfaces to groups of part
    // ids. Configurations of one model differ (comet7's first one leaves the
    // rear deck unpainted, 15 of its other 16 paint it), so the model gets
    // its own copy of the set with the result most of its configurations
    // give (the earliest of those on a tie).
    {
        for (owner, (configs, own)) in &by_owner {
            let owner = *owner;
            let key = format!("{owner:08x}");
            let Some(set) = t.models.get(&key).cloned() else { continue };
            let Some(base) = t.sets.get(&set).cloned() else { continue };
            // (result, the configurations that give it), in first-seen order
            let mut results: Vec<(Vec<Part>, Vec<String>)> = Vec::new();
            // the part list of the first configuration that gave each result
            let mut firsts: Vec<&Vec<u32>> = Vec::new();
            for parts in configs {
                let mut out = base.clone();
                apply_config(&lib, own, parts, &mut out);
                let ck = config_key(parts);
                match results.iter_mut().find(|r| r.0 == out) {
                    Some(r) => r.1.push(ck),
                    None => {
                        results.push((out, vec![ck]));
                        firsts.push(parts);
                    }
                }
            }
            // The most common result counts configurations by their colours
            // and surfaces alone (paint types split them further); of the
            // results with those, the one most configurations give.
            let plain = |r: &[Part]| r.iter().map(|p| Part { coat: 0, ..*p }).collect::<Vec<_>>();
            let plains: Vec<Vec<Part>> = results.iter().map(|r| plain(&r.0)).collect();
            let shared = |i: usize| results.iter().zip(&plains).filter(|(_, p)| **p == plains[i]).map(|(r, _)| r.1.len()).sum::<usize>();
            let most = (0..results.len()).map(shared).max().unwrap_or(0);
            let group = (0..results.len()).find(|i| shared(*i) == most);
            let in_group = |i: usize| group.is_some_and(|g| plains[i] == plains[g]);
            let most = (0..results.len()).filter(|i| in_group(*i)).map(|i| results[i].1.len()).max().unwrap_or(0);
            let best = (0..results.len()).find(|i| in_group(*i) && results[*i].1.len() == most);
            // What one more of the vehicle's own parts changes when it is
            // fitted on top of the usual configuration: a part's modifier
            // list can define a group of part ids (a kit's panels) that the
            // configuration's bindings then colour (with the body paint).
            if let Some(b) = best {
                let (usual, parts0) = (&results[b].0, firsts[b]);
                let mut patches: HashMap<String, Vec<(u8, Part)>> = HashMap::new();
                for (x, _) in own.iter().filter(|(x, list)| !parts0.contains(x) && list.iter().any(|m| lib.group_defs.contains_key(m) || lib.refs.contains_key(m))) {
                    let mut parts = parts0.clone();
                    parts.push(*x);
                    let mut out = base.clone();
                    apply_config(&lib, own, &parts, &mut out);
                    let d: Vec<(u8, Part)> = out.iter().zip(usual.iter()).enumerate().filter(|(_, (a, b))| a != b).map(|(i, (a, _))| (i as u8, *a)).collect();
                    if !d.is_empty() {
                        patches.insert(format!("{x:08x}"), d);
                    }
                }
                if !patches.is_empty() {
                    t.part_patches.insert(key.clone(), patches);
                }
            }
            for (ri, (out, keys)) in results.into_iter().enumerate() {
                // each distinct result is a set of its own; the most common one is the model's
                let name = if out == base {
                    set.clone()
                } else if Some(ri) == best {
                    format!("{set}+{key}")
                } else {
                    format!("{set}+{key}#{ri}")
                };
                t.sets.entry(name.clone()).or_insert(out);
                let by_config = t.config_sets.entry(key.clone()).or_default();
                for k in keys {
                    by_config.insert(k, name.clone());
                }
                if Some(ri) == best {
                    t.models.insert(key.clone(), name);
                }
            }
        }
    }
    // The models of the library parts a configuration names: the same ids
    // mean the same piece in every vehicle's set (a box body, a flatbed
    // deck, a tipper's paint), and the configuration that fits the part
    // recolours them, so a shared model takes the set of the vehicle and
    // configuration it is on. For the model on its own: the first found.
    for (owner, (configs, _)) in by_owner.iter().filter(|(o, _)| **o != 0) {
        let Some(by_config) = t.config_sets.get(&format!("{owner:08x}")) else { continue };
        for parts in configs {
            let Some(set) = by_config.get(&config_key(parts)) else { continue };
            for m in parts.iter().filter_map(|p| lib.part_models.get(p)).flatten() {
                t.shared_parts.entry(format!("{m:08x}")).or_insert_with(|| set.clone());
            }
        }
    }
    Ok(t)
}

/// Apply the colour and surface modifiers of a configuration's parts to a
/// part set; returns the number of part ids changed. Bindings are applied in
/// priority order (as listed within a priority). Of an option list the
/// first part is followed (which one the game picks is not decoded).
fn apply_config(lib: &Library, own: &HashMap<u32, Vec<u32>>, parts: &[u32], set: &mut [Part]) -> usize {
    // a part's modifiers, with the first part of each option list followed
    fn collect(lib: &Library, own: &HashMap<u32, Vec<u32>>, part: u32, depth: u32, out: &mut Vec<u32>) {
        let Some(list) = own.get(&part).or_else(|| lib.mods.get(&part)) else { return };
        for &m in list {
            out.push(m);
            if let Some(first) = lib.options.get(&m).and_then(|o| o.first()).filter(|_| depth < 3) {
                collect(lib, own, *first, depth + 1, out);
            }
            if let Some(part) = lib.refs.get(&m).filter(|_| depth < 3) {
                collect(lib, own, *part, depth + 1, out);
            }
        }
    }
    let mut mods: Vec<u32> = Vec::new();
    for &p in parts {
        collect(lib, own, p, 0, &mut mods);
    }
    let local: HashMap<u32, &Vec<u8>> = mods.iter().filter_map(|m| lib.group_defs.get(m)).map(|(g, ids)| (*g, ids)).collect();
    let channel: HashMap<u32, (u32, u32)> = mods.iter().filter_map(|m| lib.assigns.get(m)).map(|&(ch, c, s, _)| (ch, (c, s))).collect();
    // a channel's coat: its paint type modifier, else its assignment's default
    let mut coats: HashMap<u32, u32> = mods.iter().filter_map(|m| lib.assigns.get(m)).filter(|a| a.3 != 0).map(|a| (a.0, a.3)).collect();
    coats.extend(mods.iter().filter_map(|m| lib.paint_types.get(m)).filter(|t| t.1 != 0).copied());
    let mut binds: Vec<&(u32, u32, i64, bool, bool, bool)> = mods.iter().filter_map(|m| lib.binds.get(m)).collect();
    binds.sort_by_key(|b| b.2);
    let mut changed = vec![false; set.len()];
    for &&(group, ch, _, colour, surface, coat) in &binds {
        if ch == TYRE_COLOUR_CHANNEL {
            continue;
        }
        let Some(ids) = local.get(&group).copied().or_else(|| lib.groups.get(&group)) else { continue };
        let (c, s) = if ch == PAINT_CHANNEL { (PRIMARY_PAINT, 0) } else { channel.get(&ch).copied().unwrap_or((0, 0)) };
        for &i in ids {
            let Some(p) = set.get_mut(i as usize) else { continue };
            let before = *p;
            if colour && c != 0 {
                if let Some(&(mode, row, rgb)) = lib.colours.get(&c) {
                    (p.colour, p.mode, p.row, p.rgb) = (c, mode, row, rgb);
                }
            }
            if surface && s != 0 {
                if let Some((r, rec)) = lib.surfaces.get(&s).and_then(|rec| lib.rough.get(rec).map(|r| (r, rec))) {
                    (p.surface, p.rough, p.metal) = (s, *r, lib.metal.get(rec).copied().unwrap_or([0.0, 0.0]));
                    (p.wear_row, p.wear_rough, p.wear_metal) = lib.wear_of(s);
                }
            }
            if let Some(k) = coats.get(&ch).filter(|k| coat && lib.coats.contains_key(k)) {
                p.coat = *k;
            }
            changed[i as usize] |= *p != before;
        }
    }
    changed.iter().filter(|c| **c).count()
}

fn cache_path(work: &Path) -> std::path::PathBuf {
    work.join("index").join("vehicle_parts.json")
}

/// The tables, from `<work>/index/vehicle_parts.json` (built on first use).
pub fn tables(game_root: &Path, work: &Path) -> Result<Arc<Tables>> {
    static CACHE: OnceLock<Mutex<HashMap<std::path::PathBuf, Arc<Tables>>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().unwrap();
    if let Some(t) = c.get(work) {
        return Ok(t.clone());
    }
    let p = cache_path(work);
    let stored = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Tables>(&b).ok()).filter(|t| t.format == FORMAT);
    let t = match stored {
        Some(t) => t,
        None => {
            let t = build(game_root, work)?;
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&p, serde_json::to_vec(&t)?);
            t
        }
    };
    let t = Arc::new(t);
    c.insert(work.to_path_buf(), t.clone());
    Ok(t)
}

impl Tables {
    /// The part set of a model: a vehicle (`comet7`, `comet7_hi`), one of
    /// its mod part models, or a library part's model (`shared_parts`).
    pub fn for_model(&self, model: &str) -> Option<(&str, &[Part])> {
        let m = model.to_ascii_lowercase();
        let base = m.strip_suffix("_hi").or_else(|| m.strip_suffix("_mid")).unwrap_or(&m);
        let key = |s: &str| format!("{:08x}", meta7a::joaat(s));
        let owned = || self.part_owner.get(&format!("{:016x}", meta7a::asset_id(base))).and_then(|v| self.models.get(v));
        let set = self.elements.get(&key(&m)).or_else(|| self.elements.get(&key(base))).or_else(|| self.models.get(&key(base))).or_else(owned).or_else(|| self.shared_parts.get(&key(base)))?;
        self.sets.get_key_value(set).map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Whether a model's part set is only that of one of the vehicles it
    /// can be fitted to (a library part's model).
    pub fn is_shared(&self, model: &str) -> bool {
        let m = model.to_ascii_lowercase();
        let base = m.strip_suffix("_hi").or_else(|| m.strip_suffix("_mid")).unwrap_or(&m);
        let key = |s: &str| format!("{:08x}", meta7a::joaat(s));
        let own = self.elements.contains_key(&key(&m)) || self.elements.contains_key(&key(base)) || self.models.contains_key(&key(base)) || self.part_owner.contains_key(&format!("{:016x}", meta7a::asset_id(base)));
        !own && self.shared_parts.contains_key(&key(base))
    }
}

/// Vertex colours (linear RGB, alpha see below) for the meshes of a
/// two-layer vehicle material: `ids` are the part ids (vertex colour alpha),
/// `palette` the 128 x 256 material palette (sRGB RGBA8).
///
/// Alpha: 254 = primary body paint (a viewer may swap in a paint colour);
/// otherwise `surface_code` (metalness and roughness of the part's surface).
pub fn vertex_colours(parts: &[Part], palette: &[[u8; 4]], u: f32, mat: [f32; 2], ids: impl Iterator<Item = u8>) -> Vec<[u8; 4]> {
    let look = look_table(parts, palette, u, mat);
    ids.map(|i| look[i as usize]).collect()
}

/// The colour of each of the 256 part ids (see `vertex_colours`). `palette`
/// is the palette's slices one after the other, `u` the position along a
/// palette row (0 to 1) and `mat` the blend between a surface's low and
/// high metalness and roughness (the red and green of `lyr0materialatex`).
/// A part with a colour of its own shows it; one without reads its row of
/// the slice its page names.
pub fn look_table(parts: &[Part], palette: &[[u8; 4]], u: f32, mat: [f32; 2]) -> Vec<[u8; 4]> {
    let lin = |v: u8| {
        let c = v as f32 / 255.0;
        let l = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        (l * 255.0 + 0.5) as u8
    };
    (0..256)
        .map(|i| {
            let Some(p) = parts.get(i) else { return [255; 4] };
            let from_row = p.rgb == [0; 3] && p.row >= 0 && (0..8).contains(&p.mode);
            let rgb = if from_row { ramp(palette, p.mode as usize, p.row as usize, u) } else { p.rgb };
            [lin(rgb[0]), lin(rgb[1]), lin(rgb[2]), if p.is_primary() { 254 } else { surface_code(p, mat) }]
        })
        .collect()
}

/// The finish of a vehicle's body paint: the metalness and roughness of
/// the surfaces of its primary-paint parts, weighted by `counts` (vertices
/// per part id). None when no counted part is primary paint.
pub fn paint_finish(parts: &[Part], mat: [f32; 2], counts: &[u64; 256]) -> Option<[f32; 2]> {
    let at = |v: [f32; 2], t: f32| (v[0] + (v[1] - v[0]) * t.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    let (mut n, mut metal, mut rough) = (0.0f64, 0.0f64, 0.0f64);
    for (p, c) in parts.iter().zip(counts).filter(|(p, c)| p.is_primary() && **c > 0) {
        n += *c as f64;
        metal += at(p.metal, mat[0]) as f64 * *c as f64;
        rough += at(p.rough, mat[1]) as f64 * *c as f64;
    }
    (n > 0.0).then(|| [(metal / n) as f32, (rough / n) as f32])
}

/// A part's surface as the alpha of its vertex colour: metalness in four
/// steps x 50 + roughness in 50 steps (0 to 199): metalLow to metalHigh by
/// `mat[0]`, roughnessLow to roughnessHigh by `mat[1]`.
pub fn surface_code(p: &Part, mat: [f32; 2]) -> u8 {
    let at = |v: [f32; 2], t: f32| (v[0] + (v[1] - v[0]) * t.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    let metal = (at(p.metal, mat[0]) * 3.0 + 0.5) as u8;
    let rough = (at(p.rough, mat[1]) * 49.0 + 0.5) as u8;
    metal * 50 + rough
}

impl Tables {
    /// What model.json records so a viewer can recolour a vehicle for a
    /// chosen configuration: `looks` (each distinct result as 256 x RGBA,
    /// 2,048 hex digits, encoded like GLB COLOR_0), `configs` (configuration
    /// key -> index into looks) and `default` (the look in the GLB). Null
    /// for models without configurations (mod parts).
    pub fn looks_json(&self, model: &str, palette: &[[u8; 4]], u: f32, mat: [f32; 2]) -> Value {
        let m = model.to_ascii_lowercase();
        let base = m.strip_suffix("_hi").or_else(|| m.strip_suffix("_mid")).unwrap_or(&m);
        let key = format!("{:08x}", meta7a::joaat(base));
        let (Some(by_config), Some(default)) = (self.config_sets.get(&key), self.models.get(&key)) else { return Value::Null };
        let mut names: Vec<&String> = by_config.values().collect();
        names.push(default);
        names.sort();
        names.dedup();
        let hex = |n: &String| self.sets.get(n).map(|p| look_table(p, palette, u, mat).iter().flat_map(|c| c.iter().map(|b| format!("{b:02x}"))).collect::<String>());
        let looks: Vec<Value> = names.iter().map(|n| hex(n).map_or(Value::Null, Value::String)).collect();
        let rows: Vec<Value> = names.iter().map(|n| self.sets.get(*n).map_or(Value::Null, |p| Value::String(row_table(p)))).collect();
        let wear: Vec<Value> = names.iter().map(|n| self.sets.get(*n).map_or(Value::Null, |p| Value::String(wear_table(p, mat)))).collect();
        // the coat records the looks use, and per look the index of each part id's
        let mut used: Vec<u32> = names.iter().filter_map(|n| self.sets.get(*n)).flatten().map(|p| p.coat).filter(|c| self.coats.contains_key(&format!("{c:08x}"))).collect();
        used.sort();
        used.dedup();
        let coats: Vec<Value> = names.iter().map(|n| self.sets.get(*n).map_or(Value::Null, |p| Value::String(coat_table(p, &used)))).collect();
        let paint_coats: Vec<Value> = names.iter().map(|n| self.sets.get(*n).and_then(|p| paint_coat(p, &[1; 256])).and_then(|c| used.iter().position(|u| *u == c)).map_or(Value::Null, Value::from)).collect();
        let coat_records: Vec<Value> = used.iter().map(|c| self.coat_json(*c)).collect();
        let index = |n: &String| names.iter().position(|x| *x == n);
        // what fitting one more part changes, as look entries
        let patches: serde_json::Map<String, Value> = self
            .part_patches
            .get(&key)
            .into_iter()
            .flatten()
            .filter_map(|(part, d)| {
                let mut set = self.sets.get(default)?.clone();
                for (i, p) in d {
                    set[*i as usize] = *p;
                }
                let look = look_table(&set, palette, u, mat);
                let rgba: String = d.iter().flat_map(|x| look[x.0 as usize]).map(|b| format!("{b:02x}")).collect();
                Some((part.clone(), serde_json::json!({"ids": d.iter().map(|x| x.0).collect::<Vec<_>>(), "rgba": rgba})))
            })
            .collect();
        serde_json::json!({
            "looks": looks,
            "part_patches": patches,
            "rows": rows,
            "wear": wear,
            "coats": coats,
            "paint_coats": paint_coats,
            "coat_records": coat_records,
            "configs": by_config.iter().filter_map(|(k, n)| Some((k.clone(), Value::from(index(n)?)))).collect::<serde_json::Map<String, Value>>(),
            "default": index(default),
        })
    }
}

impl Tables {
    /// A coat record as JSON: its name (8 hex digits) and fields; Null when
    /// the library has none of that name.
    pub fn coat_json(&self, name: u32) -> Value {
        let key = format!("{name:08x}");
        let Some(mut v) = self.coats.get(&key).and_then(|c| serde_json::to_value(c).ok()) else { return Value::Null };
        v["name"] = Value::String(key);
        v
    }
}

/// The coat of a vehicle's body paint: the coat record most of its
/// primary-paint parts name, weighted by `counts` (vertices per part id).
/// None when no counted part is primary paint or none names a coat.
pub fn paint_coat(parts: &[Part], counts: &[u64; 256]) -> Option<u32> {
    let mut n: Vec<(u32, u64)> = Vec::new();
    for (p, c) in parts.iter().zip(counts).filter(|(p, c)| p.is_primary() && p.coat != 0 && **c > 0) {
        match n.iter_mut().find(|x| x.0 == p.coat) {
            Some(x) => x.1 += *c,
            None => n.push((p.coat, *c)),
        }
    }
    // the first seen wins a tie
    n.iter().rev().max_by_key(|x| x.1).map(|x| x.0)
}

/// The coat of each of the 256 part ids, 2 hex digits each: the index into
/// `used` (coat record names), `ff` for a part whose coat is not among them.
pub fn coat_table(parts: &[Part], used: &[u32]) -> String {
    (0..256).map(|i| format!("{:02x}", parts.get(i).and_then(|p| used.iter().position(|u| *u == p.coat)).filter(|k| *k < 255).unwrap_or(255))).collect()
}

/// The palette row of each of the 256 part ids, 4 hex digits each: slice x
/// 256 + row for a part that reads its colour from the material palette,
/// `ffff` for one with a colour of its own (see `look_table`).
pub fn row_table(parts: &[Part]) -> String {
    (0..256)
        .map(|i| match parts.get(i) {
            Some(p) if p.rgb == [0; 3] && p.row >= 0 && (0..8).contains(&p.mode) => format!("{:04x}", p.mode as usize * 256 + (p.row as usize & 255)),
            _ => "ffff".to_string(),
        })
        .collect()
}

/// The second layer of each of the 256 part ids, 8 hex digits each: palette
/// row (4 digits: slice x 256 + row, `ffff` = none), roughness and metalness
/// (2 digits each, 0 to 255), blended between low and high like the first
/// layer's (`mat`, see `look_table`).
pub fn wear_table(parts: &[Part], mat: [f32; 2]) -> String {
    let at = |v: [f32; 2], t: f32| ((v[0] + (v[1] - v[0]) * t.clamp(0.0, 1.0)).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    (0..256)
        .map(|i| match parts.get(i) {
            Some(p) => format!("{:04x}{:02x}{:02x}", p.wear_row.unwrap_or(0xffff), at(p.wear_rough, mat[1]), at(p.wear_metal, mat[0])),
            None => "ffff0000".to_string(),
        })
        .collect()
}

/// The colour of a row of a palette slice at position `u` (0 to 1): the
/// mean of the 17 columns around it. In the paint shader the position is the green
/// channel of the layer's detail texture (`lyr0diffusetex`), which is a
/// gradient-position map, not a colour; an export uses that texture's mean
/// green for all of a model's parts.
pub fn ramp(palette: &[[u8; 4]], slice: usize, row: usize, u: f32) -> [u8; 3] {
    const W: usize = 128;
    let row = slice * 256 + row;
    let mut s = [0u32; 3];
    let c = (u.clamp(0.0, 1.0) * (W - 1) as f32 + 0.5) as usize;
    let cols = c.saturating_sub(8)..(c + 9).min(W);
    let n = cols.len() as u32;
    for x in cols {
        let Some(p) = palette.get(row * W + x) else { return [128; 3] };
        for k in 0..3 {
            s[k] += p[k] as u32;
        }
    }
    [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_from_rows_and_paints() {
        // 8 slices; the row is read from the slice the part's page names
        let mut pal = vec![[0u8, 0, 0, 255]; 128 * 256 * 8];
        let at = |slice: usize, row: usize, x: usize| (slice * 256 + row) * 128 + x;
        for x in 0..128 {
            pal[at(3, 5, x)] = [200, 100, 50, 255];
            pal[at(0, 5, x)] = [9, 9, 9, 255];
            pal[at(1, 6, x)] = [60, 70, 80, 255];
        }
        let mid = [0.5, 0.5];
        let mut parts = vec![Part { mode: 3, row: 5, rough: [0.2, 0.4], ..Default::default() }; 256];
        parts[1] = Part { colour: PRIMARY_PAINT, mode: 1, row: 1, rgb: [255, 0, 0], ..Default::default() };
        parts[2] = Part { colour: 7, mode: 1, row: 2, rgb: [0, 255, 0], rough: [1.0, 1.0], ..Default::default() };
        // page 1 without a colour of its own: its row of slice 1
        parts[3] = Part { colour: 8, mode: 1, row: 6, ..Default::default() };
        // page 3 with a colour: the colour, not the row
        parts[4] = Part { colour: 9, mode: 3, row: 5, rgb: [0, 0, 255], ..Default::default() };
        let c = vertex_colours(&parts, &pal, 0.5, mid, [0u8, 1, 2, 3, 4].into_iter());
        assert_eq!(ramp(&pal, 3, 5, 0.5), [200, 100, 50]);
        assert!(c[3][2] > c[3][0] && c[3][0] > 0);
        assert_eq!(&c[4][..3], &[0, 0, 255]);
        // fixed material: the palette row (linear), alpha = roughness 0.3 in 50 steps
        assert_eq!(c[0][3], 15);
        assert!(c[0][0] > c[0][1] && c[0][1] > c[0][2]);
        // primary paint is marked, other paints keep their own colour
        assert_eq!(c[1], [255, 0, 0, 254]);
        assert_eq!(c[2], [0, 255, 0, 49]);
        // metalness comes from the surface: 0.9 to 1.0 is the top step
        parts[0].metal = [0.9, 1.0];
        assert_eq!(vertex_colours(&parts, &pal, 0.5, mid, [0u8].into_iter())[0][3], 165);
        // the material map's channels pick between low and high
        parts[0].metal = [0.0, 1.0];
        assert_eq!(vertex_colours(&parts, &pal, 0.5, [1.0, 0.0], [0u8].into_iter())[0][3], 150 + 10);
        assert_eq!(vertex_colours(&parts, &pal, 0.5, [0.0, 1.0], [0u8].into_iter())[0][3], 20);
        // the position along the row picks the column
        for x in 0..128 {
            pal[at(3, 5, x)] = [x as u8 * 2, 0, 0, 255];
        }
        assert!(ramp(&pal, 3, 5, 0.1)[0] < ramp(&pal, 3, 5, 0.9)[0]);
    }

    /// A meta resource with one struct (`members`: hash, offset, type) of
    /// `size` bytes and one block holding `objects`.
    fn meta_resource(hash: u32, size: usize, members: &[(u32, u32, u8)], objects: &[Vec<u8>]) -> Resource {
        let mut sys = vec![0u8; 0x200];
        sys[0x10..0x15].copy_from_slice(b"0DRPz");
        let (st, mem, it, data) = (0x60usize, 0x80usize, 0x1e0usize, 0x200usize);
        let ptr = |o: usize| 0x5000_0000u64 + o as u64;
        sys[0x28..0x30].copy_from_slice(&ptr(st).to_le_bytes());
        sys[0x38..0x40].copy_from_slice(&ptr(it).to_le_bytes());
        sys[0x48..0x4a].copy_from_slice(&1u16.to_le_bytes());
        sys[0x4c..0x4e].copy_from_slice(&1u16.to_le_bytes());
        sys[st..st + 4].copy_from_slice(&hash.to_le_bytes());
        sys[st + 0x10..st + 0x18].copy_from_slice(&ptr(mem).to_le_bytes());
        sys[st + 0x18..st + 0x1c].copy_from_slice(&(size as u32).to_le_bytes());
        sys[st + 0x1e..st + 0x20].copy_from_slice(&(members.len() as u16).to_le_bytes());
        assert!(mem + 16 * members.len() <= it);
        for (k, (h, off, ty)) in members.iter().enumerate() {
            let m = mem + 16 * k;
            sys[m..m + 4].copy_from_slice(&h.to_le_bytes());
            sys[m + 4..m + 8].copy_from_slice(&off.to_le_bytes());
            sys[m + 8] = *ty;
        }
        sys[it..it + 4].copy_from_slice(&hash.to_le_bytes());
        sys[it + 4..it + 8].copy_from_slice(&((size * objects.len()) as u32).to_le_bytes());
        sys[it + 8..it + 16].copy_from_slice(&ptr(data).to_le_bytes());
        for o in objects {
            assert_eq!(o.len(), size);
            sys.extend_from_slice(o);
        }
        Resource { sys, gfx: Vec::new(), gfx_base: 0 }
    }

    /// The members of the library's coat struct, as stored (data.rpf).
    const COAT_MEMBERS: &[(u32, u32, u8)] = &[
        (0xe6e1_7f14, 8, 0x4a),
        (0x4953_63e4, 12, 0x01),
        (0xd926_02db, 14, 0x20),
        (0xf6d2_a18e, 16, 0x01),
        (0xf031_0a8b, 18, 0x20),
        (0xc5cb_49ba, 20, 0x20),
        (0x61e8_2f14, 22, 0x01),
        (0xb97f_4910, 23, 0x01),
        (0xe57c_6a65, 24, 0x01),
        (0xdf5b_7432, 26, 0x20),
        (0xef79_abb2, 28, 0x20),
        (0xa17a_dbe1, 30, 0x20),
        (0x4bd9_d63e, 32, 0x01),
        (0xc920_19cc, 36, 0x21),
        (0x9851_c9f1, 40, 0x21),
        (0xc8d4_8032, 44, 0x01),
        (0x5f4a_a231, 46, 0x20),
        (0x85d1_88cc, 48, 0x20),
        (0x210d_19b3, 50, 0x20),
        (0x469c_3c58, 52, 0x20),
        (0x850e_331c, 54, 0x20),
    ];
    const COAT_NAME_OFF: usize = 8;

    #[test]
    fn coat_records_read_by_member() {
        // two records with the bytes of the library's `metallic` and of one
        // of its film paints
        let rec = |name: u32, f: &dyn Fn(&mut Vec<u8>)| {
            let mut b = vec![0u8; 56];
            b[COAT_NAME_OFF..COAT_NAME_OFF + 4].copy_from_slice(&name.to_le_bytes());
            f(&mut b);
            b
        };
        let off = |h: u32| COAT_MEMBERS.iter().find(|m| m.0 == h).unwrap().1 as usize;
        let half = |b: &mut Vec<u8>, h: u32, bits: u16| b[off(h)..off(h) + 2].copy_from_slice(&bits.to_le_bytes());
        let metallic = rec(0x8369_7767, &|b| {
            b[off(0xf6d2_a18e)] = 1; // useClearCoat
            half(b, 0xf031_0a8b, 0x3c00); // clearCoatIntensity 1.0
            b[off(0x61e8_2f14)] = 1; // useSpecularColor
            b[off(0xe57c_6a65)] = 1; // useFlakes
            half(b, 0xdf5b_7432, 0x3c00); // flakeCoverage 1.0
            half(b, 0xa17a_dbe1, 0x26e9); // flakeOrientation 0.027
            b[off(0x4bd9_d63e)] = 1;
            b[off(0xc920_19cc)..off(0xc920_19cc) + 4].copy_from_slice(&0.4f32.to_le_bytes());
            half(b, 0x5f4a_a231, 0x3c00); // filmIOR 1.0
            half(b, 0x469c_3c58, 0x3c00); // sheenFade 1.0
        });
        let film = rec(0x6b0a_629f, &|b| {
            half(b, 0x5f4a_a231, 0x4098); // filmIOR 2.297
            half(b, 0x85d1_88cc, 0x3800); // filmThickness 0.5
            b[off(0x4953_63e4)] = 1; // useAnisotropy
            half(b, 0xd926_02db, 0x3866); // anisotropyIntensity 0.5498
        });
        // the library's own bytes of `metallic` build the same record
        let stored: Vec<u8> = (0..56).map(|i| u8::from_str_radix(&"000000000000000067776983000000000100003c000001000100003c0000e92601000000cdcccc3e000000000000003c00000000003c0000"[2 * i..2 * i + 2], 16).unwrap()).collect();
        assert_eq!(stored, metallic);
        let r = meta_resource(COAT, 56, COAT_MEMBERS, &[metallic, film]);
        let m = Meta::parse(&r).unwrap();
        let c = coats(&m);
        assert_eq!(c.len(), 2);
        let a = c[&0x8369_7767];
        assert!(a.use_clear_coat && a.use_flakes && a.use_specular_color && a.override_base_coat_roughness);
        assert!(!a.use_anisotropy && !a.use_emissive && !a.flag_b97f4910);
        assert_eq!((a.clear_coat_intensity, a.clear_coat_roughness, a.flake_coverage, a.film_ior, a.sheen_fade), (1.0, 0.0, 1.0, 1.0, 1.0));
        assert!((a.flake_orientation - 0.027).abs() < 1e-4 && a.base_coat_roughness == 0.4);
        let b = c[&0x6b0a_629f];
        assert!(b.use_anisotropy && !b.use_clear_coat);
        assert!((b.film_ior - 2.2969).abs() < 1e-3 && b.film_thickness == 0.5 && (b.anisotropy_intensity - 0.5498).abs() < 1e-3);
        // a library without the struct, and a struct without the members
        assert!(coats(&Meta::parse(&meta_resource(0x1234_5678, 8, &[(NAME, 0, 0x4a)], &[vec![0; 8]])).unwrap()).is_empty());
        let bare = meta_resource(COAT, 8, &[(NAME, 4, 0x4a)], &[vec![0, 0, 0, 0, 7, 0, 0, 0]]);
        assert_eq!(coats(&Meta::parse(&bare).unwrap())[&7], Coat::default());
    }

    #[test]
    fn half_floats() {
        assert_eq!(meta7a::half(0x3c00), 1.0);
        assert_eq!(meta7a::half(0xbc00), -1.0);
        assert_eq!(meta7a::half(0x3800), 0.5);
        assert_eq!(meta7a::half(0), 0.0);
        assert!((meta7a::half(0x2e66) - 0.1).abs() < 1e-4);
    }

    #[test]
    fn coats_follow_the_paint_type_of_a_channel() {
        let mut lib = Library { colours: HashMap::new(), surfaces: HashMap::new(), wear: HashMap::new(), rough: HashMap::new(), metal: HashMap::new(), rim_ratio: HashMap::new(), mods: HashMap::new(), groups: HashMap::new(), group_defs: HashMap::new(), binds: HashMap::new(), assigns: HashMap::new(), paint_types: HashMap::new(), coats: HashMap::new(), options: HashMap::new(), refs: HashMap::new(), part_models: HashMap::new(), id: None };
        for k in [0xa1, 0xa2, 0xa3] {
            lib.coats.insert(k, Coat::default());
        }
        lib.groups.insert(0x10, vec![1, 2]);
        lib.groups.insert(0x11, vec![3]);
        lib.groups.insert(0x12, vec![4]);
        // modifiers: bindings (group, channel, priority, colour, surface, coat)
        lib.binds.insert(1, (0x10, PAINT_CHANNEL, 1, true, false, true));
        lib.binds.insert(2, (0x11, 0x77, 1, false, false, true));
        lib.binds.insert(3, (0x12, PAINT_CHANNEL, 1, true, false, false));
        // channel 0x77: a default from its assignment; the paint channel: a paint type
        lib.assigns.insert(4, (0x77, 0, 0, 0xa2));
        lib.assigns.insert(5, (PAINT_CHANNEL, 0, 0, 0xa3));
        lib.paint_types.insert(6, (PAINT_CHANNEL, 0xa1));
        lib.mods.insert(100, vec![1, 2, 3, 4, 5]);
        lib.mods.insert(101, vec![6]);
        let mut set = vec![Part { coat: DEFAULT_COAT, ..Default::default() }; 256];
        apply_config(&lib, &HashMap::new(), &[100], &mut set);
        // without a paint type part the assignment's default applies
        assert_eq!((set[1].coat, set[3].coat, set[4].coat, set[0].coat), (0xa3, 0xa2, DEFAULT_COAT, DEFAULT_COAT));
        let mut set = vec![Part { coat: DEFAULT_COAT, ..Default::default() }; 256];
        apply_config(&lib, &HashMap::new(), &[100, 101], &mut set);
        // the paint type wins; a binding without the flag leaves the coat
        assert_eq!((set[1].coat, set[2].coat, set[3].coat, set[4].coat), (0xa1, 0xa1, 0xa2, DEFAULT_COAT));
        // a coat name the library does not have is not applied
        lib.paint_types.insert(6, (PAINT_CHANNEL, 0xdead));
        let mut set = vec![Part { coat: DEFAULT_COAT, ..Default::default() }; 256];
        apply_config(&lib, &HashMap::new(), &[100, 101], &mut set);
        assert_eq!(set[1].coat, DEFAULT_COAT);
    }

    #[test]
    fn paint_coat_and_tables() {
        let mut parts = vec![Part { coat: 0xa1, ..Default::default() }; 256];
        parts[1] = Part { colour: PRIMARY_PAINT, coat: 0xa2, ..Default::default() };
        parts[2] = Part { colour: PRIMARY_PAINT, coat: 0xa3, ..Default::default() };
        parts[3] = Part { colour: PRIMARY_PAINT, coat: 0xa3, ..Default::default() };
        let mut counts = [0u64; 256];
        assert_eq!(paint_coat(&parts, &counts), None);
        (counts[1], counts[2], counts[3]) = (10, 4, 5);
        assert_eq!(paint_coat(&parts, &counts), Some(0xa2));
        counts[3] = 7;
        assert_eq!(paint_coat(&parts, &counts), Some(0xa3));
        let t = coat_table(&parts, &[0xa2, 0xa3]);
        assert_eq!(t.len(), 512);
        assert_eq!(&t[..8], "ff000101");
        let mut tables = Tables::default();
        tables.coats.insert("000000a2".into(), Coat { use_flakes: true, flake_size: 0.5, ..Default::default() });
        let j = tables.coat_json(0xa2);
        assert_eq!((j["name"].as_str(), j["use_flakes"].as_bool(), j["flake_size"].as_f64()), (Some("000000a2"), Some(true), Some(0.5)));
        assert!(tables.coat_json(0xa3).is_null());
    }

    #[test]
    fn model_lookup_strips_lod_suffix() {
        let mut t = Tables::default();
        t.sets.insert("00000000000000aa".into(), vec![Part::default(); 256]);
        t.models.insert(format!("{:08x}", meta7a::joaat("comet7")), "00000000000000aa".into());
        assert!(t.for_model("comet7_hi").is_some());
        assert!(t.for_model("Comet7").is_some());
        assert!(t.for_model("comet8").is_none());
    }
}
