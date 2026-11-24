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
pub const EXPORT_VERSION: u32 = 11;
/// Layout version of `<work>/index/vehicle_parts.json`.
pub const FORMAT: u32 = 10;

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
/// A configuration's part list (`51019114.e89b8540`).
const CONFIG_PARTS: u32 = 0xe89b_8540;
/// A library record a configuration's part list names (`0caef606`): slot
/// `37a6f9ef` and a list `61e1e572` of modifier record names.
const MOD: u32 = 0x0cae_f606;
const MOD_SLOT: u32 = 0x37a6_f9ef;
const MOD_LIST: u32 = 0x61e1_e572;
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
/// The channel body panels are bound to (bonnet, boot, bumpers, doors, at
/// priority 1 over a default); no record assigns it a colour, so it is taken
/// as the body paint.
const PAINT_CHANNEL: u32 = 0x7ed7_6b9f;
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
    /// modifier name -> (group, channel, priority, colour, surface)
    binds: HashMap<u32, (u32, u32, i64, bool, bool)>,
    /// modifier name -> (channel, colour record, surface record)
    assigns: HashMap<u32, (u32, u32, u32)>,
    /// modifier name -> the parts of an option list (`f5e9d7df.bf340aab`)
    options: HashMap<u32, Vec<u32>>,
    id: Option<String>,
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
            let mut l = Library { colours: HashMap::new(), surfaces: HashMap::new(), rough: HashMap::new(), metal: HashMap::new(), rim_ratio: HashMap::new(), mods: HashMap::new(), groups: HashMap::new(), group_defs: HashMap::new(), binds: HashMap::new(), assigns: HashMap::new(), options: HashMap::new(), id: Some(e.id.clone()) };
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
            for g in dump(GROUP_DEF) {
                if let (Some(n), Some(gr)) = (num(&g, NAME), num(&g, DEF_GROUP)) {
                    l.group_defs.insert(n as u32, (gr as u32, ids(&g, DEF_IDS)));
                }
            }
            for b in dump(BIND) {
                if let (Some(n), Some(gr), Some(ch)) = (num(&b, NAME), num(&b, DEF_GROUP), num(&b, BIND_CHANNEL)) {
                    l.binds.insert(n as u32, (gr as u32, ch as u32, num(&b, PRIORITY).unwrap_or(0), flag(&b, FLAG_COLOUR), flag(&b, FLAG_SURFACE)));
                }
            }
            for a in dump(ASSIGN) {
                if let (Some(n), Some(ch)) = (num(&a, NAME), num(&a, ASSIGN_CHANNEL)) {
                    l.assigns.insert(n as u32, (ch as u32, num(&a, ASSIGN_COLOUR).unwrap_or(0) as u32, num(&a, FLAG_SURFACE).unwrap_or(0) as u32));
                }
            }
            let fits: HashMap<u32, f32> = dump(WHEEL_FIT)
                .iter()
                .filter(|f| get(f, FIT_RELATIVE).and_then(|x| x.as_bool()) != Some(true))
                .filter_map(|f| Some((num(f, NAME)? as u32, get(f, FIT_RIM)?.as_f64()? as f32)))
                .filter(|(_, r)| *r > 0.2 && *r < 1.0)
                .collect();
            for p in dump(MOD) {
                let list = get(&p, MOD_LIST).and_then(|x| x.as_array()).cloned().unwrap_or_default();
                if let Some(n) = num(&p, NAME) {
                    l.mods.insert(n as u32, list.iter().filter_map(|x| x.as_u64().map(|x| x as u32)).collect());
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
                        Part { colour, surface, row, mode, rgb, rough, metal }
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
    let mut t = Tables { format: FORMAT, library: lib.id.clone(), rim_ratio: lib.rim_ratio.clone(), ..Default::default() };
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
            for parts in configs {
                let mut out = base.clone();
                apply_config(&lib, own, parts, &mut out);
                let ck = config_key(parts);
                match results.iter_mut().find(|r| r.0 == out) {
                    Some(r) => r.1.push(ck),
                    None => results.push((out, vec![ck])),
                }
            }
            let most = results.iter().map(|r| r.1.len()).max().unwrap_or(0);
            let best = results.iter().position(|r| r.1.len() == most);
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
        }
    }
    let mut mods: Vec<u32> = Vec::new();
    for &p in parts {
        collect(lib, own, p, 0, &mut mods);
    }
    let local: HashMap<u32, &Vec<u8>> = mods.iter().filter_map(|m| lib.group_defs.get(m)).map(|(g, ids)| (*g, ids)).collect();
    let channel: HashMap<u32, (u32, u32)> = mods.iter().filter_map(|m| lib.assigns.get(m)).map(|&(ch, c, s)| (ch, (c, s))).collect();
    let mut binds: Vec<&(u32, u32, i64, bool, bool)> = mods.iter().filter_map(|m| lib.binds.get(m)).collect();
    binds.sort_by_key(|b| b.2);
    let mut changed = vec![false; set.len()];
    for &&(group, ch, _, colour, surface) in &binds {
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
                }
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
    /// The part set of a model: a vehicle (`comet7`, `comet7_hi`) or one of
    /// its mod part models.
    pub fn for_model(&self, model: &str) -> Option<(&str, &[Part])> {
        let m = model.to_ascii_lowercase();
        let base = m.strip_suffix("_hi").or_else(|| m.strip_suffix("_mid")).unwrap_or(&m);
        let key = |s: &str| format!("{:08x}", meta7a::joaat(s));
        let set = self.elements.get(&key(&m)).or_else(|| self.elements.get(&key(base))).or_else(|| self.models.get(&key(base)))?;
        self.sets.get_key_value(set).map(|(k, v)| (k.as_str(), v.as_slice()))
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
        let index = |n: &String| names.iter().position(|x| *x == n);
        serde_json::json!({
            "looks": looks,
            "configs": by_config.iter().filter_map(|(k, n)| Some((k.clone(), Value::from(index(n)?)))).collect::<serde_json::Map<String, Value>>(),
            "default": index(default),
        })
    }
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
