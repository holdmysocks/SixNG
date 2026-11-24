//! Vehicle data from the game-data meta resource (`sixng vehicles`).
//!
//! One self-describing `0x7a` resource in `packs.rpf` holds the vehicle
//! tables. Field names are case-sensitive joaat and match GTA V's
//! vehicles.meta names where they exist:
//! - `e59aa857` model info, one per vehicle model (327): `gameName`
//!   (`120e5836`, joaat of the model name), `handlingId`, `txdName`,
//!   `wheelScale` / `wheelScaleRear`, `wheelType`, `layout`,
//!   `HDTextureDist`, `lodDistances`, … (about 50 names resolve).
//! - `51019114` configurations (3,879, about 300 models): `fb1f80ff` model,
//!   `5bdc13e3` body colour (u32, red in the low byte), `e89b8540` the part models of that
//!   configuration (joaat names: frame, bumpers, exhaust, brake disc,
//!   caliper, **one `rim_*` and one `tyre_*`** in 3,197 of them).
//! - `fc1c7000` variation sets (4,772): `fb1f80ff` model, `8c866829` an
//!   ARGB paint-colour list in 2,246 sets (1 to 400 entries with repeats,
//!   e.g. 400 entries / 10 distinct: a weighted draw table), `59a59c3e` weighted
//!   part groups (`37a6f9ef` slot, `a12ff3a2` [{`8e6612bf` part,
//!   `dc732774` weight %}]).

use crate::game::ArchiveInfo;
use crate::meta7a::{self, Meta};
use crate::resource::Resource;
use crate::scan::Entry;
use anyhow::Result;
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

pub const MODEL_INFO: u32 = 0xe59a_a857;
pub const CONFIG: u32 = 0x5101_9114;
pub const VARIATION: u32 = 0xfc1c_7000;
const GAME_NAME: u32 = 0x120e_5836;
const OWNER: u32 = 0xfb1f_80ff;
/// `handlingId` (case-sensitive joaat).
const HANDLING_ID: u32 = 0xec41_bd9c;
/// The number plate bones: `plate_f`, `plate_r`, `mod_plate_f`, `mod_plate_r`.
const PLATE_BONES: [&str; 4] = ["03e3e512", "567b0a43", "a0976fcb", "8e174ab3"];
/// Layout version of vehicles.json; the model viewer rebuilds older files.
pub const FORMAT: u32 = 17;
/// A mod part or kit of one vehicle (in that vehicle's own pack resource):
/// `name` joaat of the part model's name, or of a kit's name (the names of
/// its left and right models without the side word:
/// `comet7_exhaust_histr_19` for `comet7_exhaust_histr_lr_19` and `_rr_19`),
/// `37a6f9ef` its slot (joaat of `bonnet`, `bumper_f`, `spoiler`, `exhaust`
/// ...), `elements` the names of its element records.
pub const MOD: u32 = 0x0cae_f606;
/// One model of a mod part: `name`, `079baba7` whose low u32 is joaat of the
/// bone it is modelled around (`chassis` = the car's own space, `boot`,
/// `bumper_f`, `mod_spoiler_attach` ...), `2b490ab7` the stock bones it
/// replaces.
pub const MOD_ELEMENT: u32 = 0x77f2_b89e;
const NAME: u32 = 0xe6e1_7f14;
const ELEMENTS: u32 = 0x7e5d_1612;
const MOD_SLOT: u32 = 0x37a6_f9ef;
const ELEMENT_BONE: u32 = 0x079b_aba7;
const ELEMENT_HIDES: u32 = 0x2b49_0ab7;
/// A mod element's model (`asset`), which is also the name of the model's
/// part set. In the library it is 32 bits, the joaat of the model's name
/// (1,996 of the library's 2,027 elements name an existing model that
/// way); in a vehicle pack it is 64 bits, `meta7a::asset_id` of the name
/// (all 69 elements of one pickup's pack name an existing model).
const ELEMENT_SET: u32 = 0x8605_bf0e;
const COLOUR: u32 = 0x5bdc_13e3;
const PARTS: u32 = 0xe89b_8540;
const PALETTE: u32 = 0x8c86_6829;
const GROUPS: u32 = 0x59a5_9c3e;
const GROUP_SLOT: u32 = 0x37a6_f9ef;
const GROUP_ITEMS: u32 = 0xa12f_f3a2;
const ITEM_PART: u32 = 0x8e66_12bf;
const ITEM_WEIGHT: u32 = 0xdc73_2774;

/// A stored colour (red in the low byte, alpha in the high one: the
/// library's named colours `4ce375e6` begin with GTA IV's car colour list
/// only when read that way) as ARGB, plus an sRGB reading that assumes the
/// stored channels are linear (all stored body colours are dark, as linear
/// values of ordinary paints would be; not verified in game). `stored` is
/// the value as it is in the file.
fn argb(stored: u32) -> Value {
    let c = (stored & 0xFF00_FF00) | ((stored & 0xFF) << 16) | ((stored >> 16) & 0xFF);
    let s = |x: u32| {
        let l = x as f32 / 255.0;
        let v = if l <= 0.003_130_8 { 12.92 * l } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
        (v * 255.0).round().clamp(0.0, 255.0) as u32
    };
    let (r, g, b) = ((c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF);
    json!({"stored": format!("{stored:08x}"), "argb": format!("{c:08x}"), "rgb": format!("#{:06x}", c & 0xFF_FFFF), "srgb_if_linear": format!("#{:02x}{:02x}{:02x}", s(r), s(g), s(b))})
}

fn get<'v>(v: &'v Value, h: u32) -> Option<&'v Value> {
    v.get(meta7a::field_name(h))
}

/// The mod parts of a vehicle pack or of the library that have models:
/// `{name, slot, elements: [{bone, hides, model}]}`, all joaat hashes as 8
/// hex digits. Records without elements (colours, liveries, wheel choices)
/// are left out.
fn mod_parts(mods: &[Value], elements: &[Value]) -> Vec<Value> {
    let hex = |h: u32| format!("{h:08x}");
    let by_name: HashMap<u32, &Value> = elements.iter().filter_map(|e| Some((get(e, NAME)?.as_u64()? as u32, e))).collect();
    mods.iter()
        .filter_map(|m| {
            let els: Vec<Value> = u32s(get(m, ELEMENTS))
                .iter()
                .filter_map(|n| by_name.get(n))
                .map(|e| element_json(e))
                .collect();
            if els.is_empty() {
                return None;
            }
            Some(json!({
                "name": hex(get(m, NAME)?.as_u64()? as u32),
                "slot": hex(get(m, MOD_SLOT).and_then(|x| x.as_u64()).unwrap_or(0) as u32),
                "elements": els,
            }))
        })
        .collect()
}

/// A part's modifier list (names of records of several kinds).
const MOD_LIST: u32 = 0x61e1_e572;
/// A modifier that sets a key (`c3afd061`) while its part is fitted.
const KEY_MOD: u32 = 0xb21f_a2e2;
const KEY_MOD_KEY: u32 = 0xc3af_d061;
/// A switch, named in the modifier list of the part it belongs to (the
/// chassis part, a bumper ...) and in force while that part is fitted: it
/// shows the element of the option whose key a fitted part sets (an option
/// without an element shows none), else its default element (`89f7b2c6`,
/// 0 = none). An option can name a second key (`295c3278`) that must be
/// set too; such an option goes before one without.
const SWITCH: u32 = 0x6191_ddc2;
const SWITCH_DEFAULT: u32 = 0x89f7_b2c6;
const SWITCH_OPTIONS: u32 = 0x9977_3916;
const OPTION_KEY: u32 = 0x3ffe_5fbc;
const OPTION_ELEMENT: u32 = 0x5899_4e97;
const OPTION_ALSO: u32 = 0x295c_3278;
/// joaat("chassis"): the slot of the part that is always fitted.
const SLOT_CHASSIS: u32 = 0xd418_625e;

/// One mod element as vehicles.json gives it.
fn element_json(e: &Value) -> Value {
    let hex = |h: u32| format!("{h:08x}");
    // a u64 written as 16 hex digits (the bone is its low half) in the
    // packs, a u32 in the library
    let low = |h: u32| get(e, h).and_then(|x| x.as_str().and_then(|s| u32::from_str_radix(s.get(8..16)?, 16).ok()).or_else(|| x.as_u64().map(|v| v as u32))).unwrap_or(0);
    // the packs' 64-bit value is the model's asset id
    // (`meta7a::asset_id` of its name)
    let asset = get(e, ELEMENT_SET).and_then(|x| x.as_str()).filter(|s| s.len() == 16);
    json!({"bone": hex(low(ELEMENT_BONE)), "hides": u32s(get(e, ELEMENT_HIDES)).into_iter().map(hex).collect::<Vec<_>>(), "model": hex(low(ELEMENT_SET)), "asset": asset})
}

/// The keys a part sets while fitted: part name -> keys, from its modifier
/// list and the key modifiers of the same resource.
/// A wheel fitment record (`9a172293`), named in a part's modifier list.
/// One marked relative (`0341ff8f`) changes the wheels of the vehicle the
/// part is on: a widebody kit moves them out and widens the tyres.
const WHEEL_FIT: u32 = 0x9a17_2293;
const FIT_RELATIVE: u32 = 0x0341_ff8f;
const FIT_OFFSET: u32 = 0x534c_819f;
const FIT_AXLES: u32 = 0xba43_4b0e;

/// What a wheel fitment record changes, or None when it changes none of
/// width, offset and spacing (a rim size record).
fn fit_json(f: &Value) -> Option<Value> {
    let num = |h: u32| get(f, h).and_then(|x| x.as_f64()).unwrap_or(0.0);
    let (width, offset, spacing) = (num(crate::meta7a::joaat("width")), num(FIT_OFFSET), num(crate::meta7a::joaat("spacing")));
    if width == 0.0 && offset == 0.0 && spacing == 0.0 {
        return None;
    }
    // a 64-bit value whose high half says which axles (0, 2 or 3)
    let axles = get(f, FIT_AXLES).and_then(|x| x.as_str().and_then(|s| u64::from_str_radix(s, 16).ok()).or_else(|| x.as_u64())).map_or(0, |v| (v >> 32) as u32);
    Some(json!({"axles": axles, "width": width, "offset": offset, "spacing": spacing, "relative": get(f, FIT_RELATIVE).and_then(|x| x.as_bool()).unwrap_or(false)}))
}

/// Part name -> its modifier list, and the wheel fitment records of a
/// resource by name.
type FitSource = (HashMap<u32, Vec<u32>>, HashMap<u32, Value>);

fn fit_source(mods: &[Value], fits: &[Value]) -> FitSource {
    let lists = mods.iter().filter_map(|m| Some((get(m, NAME)?.as_u64()? as u32, u32s(get(m, MOD_LIST))))).filter(|(_, l)| !l.is_empty()).collect();
    let recs = fits.iter().filter_map(|f| Some((get(f, NAME)?.as_u64()? as u32, fit_json(f)?))).collect();
    (lists, recs)
}

fn part_keys(mods: &[Value], key_mods: &HashMap<u32, u32>) -> HashMap<u32, Vec<u32>> {
    mods.iter()
        .filter_map(|m| {
            let keys: Vec<u32> = u32s(get(m, MOD_LIST)).iter().filter_map(|n| key_mods.get(n).copied()).collect();
            (!keys.is_empty()).then_some((get(m, NAME)?.as_u64()? as u32, keys))
        })
        .collect()
}

fn key_mods(recs: &[Value]) -> HashMap<u32, u32> {
    recs.iter().filter_map(|r| Some((get(r, NAME)?.as_u64()? as u32, get(r, KEY_MOD_KEY)?.as_u64()? as u32))).collect()
}

/// The switches of a vehicle pack: `[{name, part, default, options: [{key,
/// also, element}]}]` with elements as in `mods` (null = nothing shown);
/// `part` is the part the switch belongs to, null for the chassis part. A
/// switch several parts name (the number plate positions, shared by every
/// plate part) is given once per part.
fn switches_json(switches: &[Value], elements: &[Value], mods: &[Value]) -> Vec<Value> {
    // switch name -> the parts whose modifier lists name it
    let mut owner: HashMap<u32, Vec<Value>> = HashMap::new();
    for m in mods {
        let Some(part) = get(m, NAME).and_then(|x| x.as_u64()) else { continue };
        let chassis = get(m, MOD_SLOT).and_then(|x| x.as_u64()) == Some(SLOT_CHASSIS as u64);
        for s in u32s(get(m, MOD_LIST)) {
            let p = if chassis { Value::Null } else { json!(format!("{part:08x}")) };
            let o = owner.entry(s).or_default();
            if !o.contains(&p) {
                o.push(p);
            }
        }
    }
    let by_name: HashMap<u32, &Value> = elements.iter().filter_map(|e| Some((get(e, NAME)?.as_u64()? as u32, e))).collect();
    let el = |n: u32| by_name.get(&n).map(|e| element_json(e)).unwrap_or(Value::Null);
    let num = |v: &Value, h: u32| get(v, h).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    let unowned = [Value::Null];
    switches
        .iter()
        .flat_map(|s| {
            let also = |o: &Value| Some(num(o, OPTION_ALSO)).filter(|a| *a != 0).map(|a| format!("{a:08x}"));
            let options: Vec<Value> = get(s, SWITCH_OPTIONS).and_then(|x| x.as_array()).into_iter().flatten().map(|o| json!({"key": format!("{:08x}", num(o, OPTION_KEY)), "also": also(o), "element": el(num(o, OPTION_ELEMENT))})).collect();
            let default = el(num(s, SWITCH_DEFAULT));
            let parts = owner.get(&num(s, NAME)).map(|o| o.as_slice()).unwrap_or(&unowned);
            parts.iter().map(|part| json!({"name": format!("{:08x}", num(s, NAME)), "part": part, "default": default, "options": options})).collect::<Vec<_>>()
        })
        .collect()
}

fn u32s(v: Option<&Value>) -> Vec<u32> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as u32)).collect()).unwrap_or_default()
}

/// joaat -> lower-case model name, from every index's entry names (with and
/// without `_hi`).
pub fn model_names(ws: &crate::export::Workspace) -> HashMap<u32, String> {
    let mut names = HashMap::new();
    for ar in ws.archives() {
        let Ok(x) = ws.load_index(&ar) else { continue };
        for e in &x.entries {
            let Some(n) = e.name.as_deref() else { continue };
            let b = n.rsplit(|c| c == '/' || c == char::from(92u8)).next().unwrap_or(n).split('.').next().unwrap_or(n).to_ascii_lowercase();
            for c in [b.clone(), b.trim_end_matches("_hi").to_string()] {
                names.entry(meta7a::joaat(&c)).or_insert(c);
            }
        }
    }
    names
}

/// Strings of the text tables in data.rpf for the given label hashes: for
/// each hash the string of the first table (in file order) that has it. The
/// tables come one set per language and the first set is English.
fn display_names(game_root: &std::path::Path, work: &std::path::Path, want: &[u32]) -> HashMap<u32, String> {
    let Ok(ix) = crate::scan::load_index(&crate::scan::index_path(&work.join("index"), "data.rpf")) else { return HashMap::new() };
    let path = game_root.join("data.rpf");
    let mut tables: Vec<(u64, Vec<(u32, String)>)> = ix
        .entries
        .par_iter()
        .filter(|e| e.sub == "7a" && e.usize <= 64 << 20)
        .filter_map(|e| {
            let b = crate::scan::read_entry(&path, e).ok()?;
            if !crate::text::is_text_table(&b) {
                return None;
            }
            let t = crate::text::parse(&b).ok()?;
            let hits: Vec<(u32, String)> = want.iter().filter_map(|h| t.get(h).map(|s| (*h, crate::text::plain(s)))).collect();
            (!hits.is_empty()).then_some((e.off, hits))
        })
        .collect();
    tables.sort_by_key(|t| t.0);
    let mut out = HashMap::new();
    for (_, hits) in tables {
        for (h, s) in hits {
            if !s.trim().is_empty() {
                out.entry(h).or_insert(s);
            }
        }
    }
    out
}

/// `sixng vehicles packs --out <out>` in one call.
pub fn build(ws: &crate::export::Workspace, out: &std::path::Path) -> Result<Value> {
    let a = ws.resolve("packs")?;
    let ix = ws.load_index(&a)?;
    let metas: Vec<&Entry> = ix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Resource && e.sub == "7a").collect();
    export(&a, &metas, &model_names(ws), out)
}

/// Write `<out>/vehicles/vehicles.json`. `names`: joaat -> model name.
pub fn export(a: &ArchiveInfo, metas: &[&Entry], names: &HashMap<u32, String>, out: &std::path::Path) -> Result<Value> {
    // The resource(s) with model-info records.
    type Switches = (Vec<Value>, HashMap<u32, Vec<u32>>);
    let found: Vec<(String, Vec<Value>, Vec<Value>, Vec<Value>, Vec<Value>, Switches, FitSource)> = metas
        .par_iter()
        .filter_map(|e| {
            let r = Resource::load_sys(&a.path, e).ok()?;
            let m = Meta::parse(&r)?;
            if !m.structs.contains_key(&MODEL_INFO) {
                return None;
            }
            let dump = |h: u32| m.objects(h).iter().filter_map(|&o| m.object(h, o, 0)).collect::<Vec<_>>();
            // mod parts carry no owner: taken only from a vehicle's own pack
            // (one model info), where they are that vehicle's
            let infos = dump(MODEL_INFO);
            let single = infos.len() == 1;
            let mods = if single { mod_parts(&dump(MOD), &dump(MOD_ELEMENT)) } else { Vec::new() };
            // the chassis part's switches and the keys the pack's own parts set
            let switches = if single { (switches_json(&dump(SWITCH), &dump(MOD_ELEMENT), &dump(MOD)), part_keys(&dump(MOD), &key_mods(&dump(KEY_MOD)))) } else { Default::default() };
            let fit = if single { fit_source(&dump(MOD), &dump(WHEEL_FIT)) } else { Default::default() };
            Some((e.id.clone(), infos, dump(CONFIG), dump(VARIATION), mods, switches, fit))
        })
        .collect();
    let name = |h: u32| names.get(&h).cloned();
    // rim radius / tyre radius of the wheel fitment records (in the library)
    let fits = crate::names::work_dir().and_then(|w| crate::vehcolour::tables(&crate::export::game_root(a), &w).ok());
    // light patterns (sirens, strobes), reached through configuration parts
    let lights = crate::names::work_dir().and_then(|w| crate::vehlights::build(&crate::export::game_root(a), &w).ok());
    let mut parts_of: HashMap<u32, std::collections::BTreeSet<u32>> = HashMap::new();
    let mut models: BTreeMap<u32, Value> = BTreeMap::new();
    let mut sources = Vec::new();
    let (mut n_cfg, mut n_pal, mut n_rim) = (0, 0, 0);
    // Per-vehicle pack resources repeat rows of the merged table: keep each
    // distinct row once per model.
    let mut seen: std::collections::HashSet<(u32, String)> = std::collections::HashSet::new();
    let mut by_handling: Vec<(u32, Value)> = Vec::new();
    let mut mods_of: Vec<(u32, Vec<Value>)> = Vec::new();
    let mut switches_of: Vec<(u32, Switches)> = Vec::new();
    let mut fits_of: Vec<(u32, FitSource)> = Vec::new();
    for (id, infos, cfgs, vars, mods, switches, fit) in found {
        sources.push(id);
        if let Some(mi) = infos.first().filter(|_| !mods.is_empty()) {
            let h = get(mi, HANDLING_ID).and_then(|x| x.as_u64()).filter(|&h| h != 0).or_else(|| get(mi, GAME_NAME).and_then(|x| x.as_u64())).unwrap_or(0) as u32;
            mods_of.push((h, mods));
            switches_of.push((h, switches));
            fits_of.push((h, fit));
        }
        for mut mi in infos {
            let h = get(&mi, GAME_NAME).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            if let Some(o) = mi.as_object_mut() {
                o.remove("_struct");
                o.remove("_struct_name");
            }
            // `gameName` is not always the model: 46 of 325 records carry
            // another name there (bison4's says bison900, mobilityscooter's
            // an unresolved hash) while `handlingId` is the name the
            // configurations and the drawable use. Kept for the second pass.
            let hid = get(&mi, HANDLING_ID).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            if hid != 0 && hid != h {
                by_handling.push((hid, mi.clone()));
            }
            if models.get(&h).is_some_and(|m| !m["model_info"].is_null()) {
                continue;
            }
            models.entry(h).or_insert_with(|| json!({"model": name(h), "model_hash": format!("{h:08x}"), "model_info": null, "configurations": [], "palettes": [], "part_groups": []}))["model_info"] = mi;
        }
        for c in cfgs {
            let h = get(&c, OWNER).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            if !seen.insert((h, c.to_string())) {
                continue;
            }
            let parts: Vec<u32> = u32s(get(&c, PARTS));
            let pn: Vec<Option<String>> = parts.iter().map(|&p| name(p)).collect();
            let pick = |pre: &str| pn.iter().flatten().filter(|n| n.starts_with(pre)).cloned().collect::<Vec<_>>();
            let (rims, tyres) = (pick("rim_"), pick("tyre_"));
            if !rims.is_empty() {
                n_rim += 1;
            }
            n_cfg += 1;
            let colour = get(&c, COLOUR).and_then(|x| x.as_u64()).map(|x| argb(x as u32));
            let e = models.entry(h).or_insert_with(|| json!({"model": name(h), "model_hash": format!("{h:08x}"), "model_info": null, "configurations": [], "palettes": [], "part_groups": []}));
            let rim_ratio = fits.as_ref().and_then(|t| parts.iter().find_map(|p| t.rim_ratio.get(&format!("{p:08x}"))));
            parts_of.entry(h).or_default().extend(parts.iter().copied());
            let patterns: Vec<String> = lights.as_ref().map(|l| l.linked(&parts).iter().map(|n| format!("{n:08x}")).collect()).unwrap_or_default();
            e["configurations"].as_array_mut().unwrap().push(json!({
                "light_patterns": patterns,
                "rim_ratio": rim_ratio,
                "parts_key": crate::vehcolour::config_key(&parts),
                "colour": colour, "rim": rims.first(), "tyre": tyres.first(), "rims": rims, "tyres": tyres,
                "parts": parts.iter().zip(&pn).map(|(p, n)| n.clone().map(Value::String).unwrap_or(json!(format!("{p:08x}")))).collect::<Vec<_>>(),
            }));
        }
        for v in vars {
            let h = get(&v, OWNER).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            let pal = u32s(get(&v, PALETTE));
            let groups: Vec<Value> = get(&v, GROUPS)
                .and_then(|x| x.as_array())
                .map(|gs| {
                    gs.iter()
                        .map(|g| {
                            let slot = get(g, GROUP_SLOT).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                            let items: Vec<Value> = get(g, GROUP_ITEMS)
                                .and_then(|x| x.as_array())
                                .map(|is| {
                                    is.iter()
                                        .map(|i| {
                                            let p = get(i, ITEM_PART).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                                            json!({"part": name(p).map(Value::String).unwrap_or(json!(format!("{p:08x}"))), "weight": get(i, ITEM_WEIGHT)})
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            json!({"slot": format!("{slot:08x}"), "choices": items})
                        })
                        .collect()
                })
                .unwrap_or_default();
            if (pal.is_empty() && groups.is_empty()) || !seen.insert((h, v.to_string())) {
                continue;
            }
            let e = models.entry(h).or_insert_with(|| json!({"model": name(h), "model_hash": format!("{h:08x}"), "model_info": null, "configurations": [], "palettes": [], "part_groups": []}));
            if !pal.is_empty() {
                n_pal += 1;
                e["palettes"].as_array_mut().unwrap().push(json!(pal.iter().map(|&c| argb(c)).collect::<Vec<_>>()));
            }
            if !groups.is_empty() {
                e["part_groups"].as_array_mut().unwrap().push(json!(groups));
            }
        }
    }
    // Models whose record is filed under another `gameName`: take the
    // record whose `handlingId` names them.
    for (hid, mi) in by_handling {
        if let Some(m) = models.get_mut(&hid) {
            if m["model_info"].is_null() {
                m["model_info"] = mi;
                m["model_info_via"] = "handlingId".into();
            }
        }
    }
    for (h, mods) in mods_of {
        if let Some(m) = models.get_mut(&h) {
            m["mods"] = json!(mods);
        }
    }
    // Shared parts with models (truck beds and boxes, light bars, racks):
    // they are records of the library, named in a configuration's part
    // list like any other part. Each vehicle gets the ones its
    // configurations use.
    let library_parts: Vec<(Vec<Value>, HashMap<u32, Vec<u32>>, FitSource, Vec<Value>, HashMap<u32, u32>)> = crate::names::work_dir()
        .and_then(|w| crate::scan::load_index(&crate::scan::index_path(&w.join("index"), "data.rpf")).ok())
        .map(|ix| {
            let path = crate::export::game_root(a).join("data.rpf");
            ix.entries
                .par_iter()
                .filter(|e| e.kind == crate::classify::Kind::Resource && e.sub == "7a")
                .filter_map(|e| {
                    let r = Resource::load_sys(&path, e).ok()?;
                    let m = Meta::parse(&r)?;
                    if !m.structs.contains_key(&MOD_ELEMENT) || m.structs.contains_key(&MODEL_INFO) {
                        return None;
                    }
                    let dump = |h: u32| m.objects(h).iter().filter_map(|&o| m.object(h, o, 0)).collect::<Vec<_>>();
                    let mods = dump(MOD);
                    Some((mod_parts(&mods, &dump(MOD_ELEMENT)), part_keys(&mods, &key_mods(&dump(KEY_MOD))), fit_source(&mods, &dump(WHEEL_FIT)), switches_json(&dump(SWITCH), &dump(MOD_ELEMENT), &mods), key_mods(&dump(KEY_MOD))))
                })
                .collect()
        })
        .unwrap_or_default();
    // the keys the library's parts set while fitted
    let library_keys: HashMap<u32, Vec<u32>> = library_parts.iter().flat_map(|l| l.1.iter().map(|(k, v)| (*k, v.clone()))).collect();
    // the library's modifier lists and wheel fitment records
    let mut library_fit: FitSource = Default::default();
    for l in &library_parts {
        let (lists, recs) = &l.2;
        library_fit.0.extend(lists.iter().map(|(k, v)| (*k, v.clone())));
        library_fit.1.extend(recs.iter().map(|(k, v)| (*k, v.clone())));
    }
    // the switches of the library's parts (a truck's side guards come in
    // lengths and heights, of which the keys of the bed and of the truck's
    // own parts pick one), by the part they belong to
    // the library's key modifiers: a pack's own part can name one (a
    // truck's wheelbase key)
    let library_key_mods: HashMap<u32, u32> = library_parts.iter().flat_map(|l| l.4.iter().map(|(k, v)| (*k, *v))).collect();
    // each pack's own modifier lists
    let own_lists: HashMap<u32, HashMap<u32, Vec<u32>>> = fits_of.iter().map(|(h, f)| (*h, f.0.clone())).collect();
    let mut library_switches: HashMap<u32, Vec<Value>> = HashMap::new();
    for s in library_parts.iter().flat_map(|l| l.3.iter()) {
        if let Some(p) = s["part"].as_str().and_then(|p| u32::from_str_radix(p, 16).ok()) {
            library_switches.entry(p).or_default().push(s.clone());
        }
    }
    let library: HashMap<u32, Value> = library_parts.into_iter().flat_map(|l| l.0).filter_map(|p| Some((u32::from_str_radix(p["name"].as_str()?, 16).ok()?, p))).collect();
    let mut n_shared = 0;
    for (h, parts) in &parts_of {
        let Some(m) = models.get_mut(h) else { continue };
        let own: std::collections::HashSet<String> = m["mods"].as_array().into_iter().flatten().filter_map(|x| x["name"].as_str().map(|s| s.to_string())).collect();
        let add: Vec<Value> = parts
            .iter()
            .filter_map(|p| library.get(p))
            .filter(|p| !own.contains(p["name"].as_str().unwrap_or("")))
            .map(|p| {
                let mut p = p.clone();
                p["shared"] = json!(true);
                p
            })
            .collect();
        if add.is_empty() {
            continue;
        }
        n_shared += add.len();
        match m["mods"].as_array_mut() {
            Some(a) => a.extend(add),
            None => m["mods"] = json!(add),
        }
    }
    // The switches of the chassis part, of the pack's other parts and of
    // the library parts its configurations name, and the keys of the parts
    // that can be fitted.
    let mut n_switches = 0;
    let mut own_switches: HashMap<u32, Switches> = switches_of.into_iter().collect();
    let hashes: Vec<u32> = models.keys().copied().collect();
    for h in hashes {
        let (mut switches, own_keys) = own_switches.remove(&h).unwrap_or_default();
        let mut keys: BTreeMap<String, Vec<String>> = own_keys.iter().map(|(p, k)| (format!("{p:08x}"), k.iter().map(|k| format!("{k:08x}")).collect())).collect();
        for p in parts_of.get(&h).into_iter().flatten() {
            if let Some(k) = library_keys.get(p) {
                keys.entry(format!("{p:08x}")).or_insert_with(|| k.iter().map(|k| format!("{k:08x}")).collect());
            }
        }
        // keys an own part sets through a key modifier of the library
        for (p, list) in own_lists.get(&h).into_iter().flatten() {
            for k in list.iter().filter_map(|n| library_key_mods.get(n)) {
                let e = keys.entry(format!("{p:08x}")).or_default();
                let k = format!("{k:08x}");
                if !e.contains(&k) {
                    e.push(k);
                }
            }
        }
        // A library part's switch is taken where it is a plain choice of one
        // element (no default, no option without an element) that this
        // vehicle's parts can answer: the key (and second key) of an option
        // is one some part sets. The elements it chooses from then leave the
        // part's element list, so that the switch shows one of them and not
        // the part all (a truck's side guards: the bed sets high or low, the
        // chassis part the wheelbase).
        // The number plate switches are taken as they are: every element on
        // one of the four plate bones. The rear one shows the plate on
        // `plate_r`, on `mod_plate_r` where a fitted part (rear bumper, bed,
        // boot) sets c623bf2c, and nothing with 68bd62ca; the front one
        // shows nothing unless a part sets 2aef799d (then `plate_f`, or
        // `mod_plate_f` where a front bumper sets 7be534e0), and nothing
        // with 503c4352. Other library switches with a default or an empty
        // option are left out, and their parts keep fitting every element.
        let settable: std::collections::HashSet<&str> = keys.values().flatten().map(|k| k.as_str()).collect();
        let answerable = |s: &Value| {
            let o = s["options"].as_array().map(|o| o.as_slice()).unwrap_or(&[]);
            // a plain choice of one element: no default, no empty option
            let plain = s["default"].is_null() && o.iter().all(|o| !o["element"].is_null()) && o.iter().any(|o| o["key"].as_str().is_some_and(|k| settable.contains(k)) && o["also"].as_str().is_none_or(|k| settable.contains(k)));
            let mut bones = o.iter().map(|o| &o["element"]).chain(std::iter::once(&s["default"])).filter(|e| !e.is_null()).map(|e| e["bone"].as_str().unwrap_or("")).peekable();
            plain || (bones.peek().is_some() && bones.all(|b| PLATE_BONES.contains(&b)))
        };
        let mut library_taken: Vec<Value> = Vec::new();
        for p in parts_of.get(&h).into_iter().flatten() {
            library_taken.extend(library_switches.get(p).into_iter().flatten().filter(|s| answerable(s)).cloned());
        }
        let Some(m) = models.get_mut(&h) else { continue };
        for part in m.get_mut("mods").and_then(|x| x.as_array_mut()).into_iter().flatten().filter(|p| p.get("shared") == Some(&json!(true))) {
            let governed: Vec<(String, String)> = library_taken
                .iter()
                .filter(|s| s["part"] == part["name"])
                .flat_map(|s| s["options"].as_array().into_iter().flatten().map(|o| &o["element"]).chain(std::iter::once(&s["default"])))
                .filter_map(|e| Some((e["model"].as_str()?.to_string(), e["bone"].as_str()?.to_string())))
                .collect();
            if governed.is_empty() {
                continue;
            }
            if let Some(els) = part["elements"].as_array_mut() {
                let before = els.len();
                els.retain(|e| !governed.iter().any(|g| e["model"].as_str() == Some(&g.0) && e["bone"].as_str() == Some(&g.1)));
                let n = before - els.len();
                if n > 0 {
                    part["switched_elements"] = json!(n);
                }
            }
        }
        drop(settable);
        switches.extend(library_taken);
        if switches.is_empty() {
            continue;
        }
        n_switches += switches.len();
        m["switches"] = json!(switches);
        m["part_keys"] = json!(keys);
    }
    // What the parts that can be fitted change on the wheels: the pack's
    // own parts and the library parts its configurations name. A part's
    // fitment records can be in the library while the part is the pack's.
    let mut n_fits = 0;
    for (h, (lists, recs)) in fits_of {
        let Some(m) = models.get_mut(&h) else { continue };
        let parts: std::collections::BTreeSet<u32> = lists.keys().copied().chain(parts_of.get(&h).into_iter().flatten().copied()).collect();
        let mut out: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for p in parts {
            let Some(list) = lists.get(&p).or_else(|| library_fit.0.get(&p)) else { continue };
            let f: Vec<Value> = list.iter().filter_map(|x| recs.get(x).or_else(|| library_fit.1.get(x))).cloned().collect();
            if !f.is_empty() {
                out.insert(format!("{p:08x}"), f);
            }
        }
        if !out.is_empty() {
            n_fits += out.len();
            m["wheel_fits"] = json!(out);
        }
    }
    let mut n_lights = 0;
    if let Some(l) = &lights {
        for (h, parts) in &parts_of {
            if let (Some(m), Some(v)) = (models.get_mut(h), l.for_vehicle(parts)) {
                n_lights += 1;
                m["lights"] = v;
            }
        }
    }
    // The game's own names: the text tables in data.rpf hold a string under
    // the model's name hash (`gameName`) and one under `vehicleMakeName`.
    // vehicleMakeName (case-sensitive joaat)
    let make_field = meta7a::field_name(0x3db8df3e);
    let mut want: Vec<u32> = models.keys().copied().collect();
    want.extend(models.values().filter_map(|m| m["model_info"][&make_field].as_u64().map(|x| x as u32)).filter(|h| *h != 0));
    let text = crate::names::work_dir().map(|w| display_names(&crate::export::game_root(a), &w, &want)).unwrap_or_default();
    let mut n_named = 0;
    for (h, m) in models.iter_mut() {
        let make = m["model_info"][&make_field].as_u64().and_then(|x| text.get(&(x as u32)));
        if let Some(n) = text.get(h) {
            n_named += 1;
            m["display_name"] = json!(n);
        }
        if let Some(mk) = make {
            m["make"] = json!(mk);
        }
    }
    // The steering lock from the handling records (PSO files in data.rpf),
    // joined by handlingId.
    let locks = crate::names::work_dir()
        .and_then(|w| crate::scan::load_index(&crate::scan::index_path(&w.join("index"), "data.rpf")).ok())
        .map(|ix| {
            let sel: Vec<&Entry> = ix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Pso).collect();
            crate::pso::steering_locks(&crate::export::game_root(a).join("data.rpf"), &sel)
        })
        .unwrap_or_default();
    let mut n_lock = 0;
    for (h, m) in models.iter_mut() {
        let hid = get(&m["model_info"], HANDLING_ID).and_then(|x| x.as_u64()).filter(|x| *x != 0).map_or(*h, |x| x as u32);
        if let Some(l) = crate::pso::steering_lock(&locks, hid) {
            n_lock += 1;
            m["steering_lock"] = json!(l);
        }
    }
    let list: Vec<&Value> = models.values().collect();
    let with_info = list.iter().filter(|m| !m["model_info"].is_null()).count();
    let named = list.iter().filter(|m| !m["model"].is_null()).count();
    let dir = out.join("vehicles");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("vehicles.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&json!({
        "note": "Per vehicle model: model_info (vehicles.meta-like; wheelScale / wheelScaleRear are not understood: they are not the rim radius (0.219 on comet7, whose rim is 0.82 x a 0.35 m tyre). A wheel's size, width and position are in the model's export (model.json wheel_mounts, from the collision discs), and the rim's share of it in the configuration's rim_ratio), configurations (body colour ARGB + the part models incl. rim and tyre; parts_key = a key of the part list that a vehicle export's model.json part_colours.configs uses to name the part colours of this configuration; rim_ratio = rim radius / tyre radius from the configuration's wheel fitment record, slot 39d5e253, where it has one), palettes (ARGB paint-colour lists; colours repeat, so repetition reads as the draw weight), part_groups (weighted part choices). mods are the parts with models: the vehicle pack's own and, marked shared, the library parts its configurations name (truck beds and boxes, light bars ...); an element's asset names its model in a vehicle pack (16 hex digits: the first half of MurmurHash3 x64 128, seed 0, of the model's lower-case name without LOD suffix; null in the library), and model is the joaat of the model's name for library parts (in a pack it is only the low half of asset), bone the bone it is modelled around and hides the stock bones it replaces. switches are the stock alternatives of a part (bonnet variants, aerial, light bar and plate mounts, engine pieces): a fitted part sets keys (part_keys: part name -> keys, for the pack's own parts and the library parts its configurations name), and a switch, in force while its part is fitted (part; null = the chassis part, always fitted), shows the element of the option whose key is set (and whose also key, where it has one, is set too: such an option goes first; a null element shows nothing), else its default element (null = nothing). wheel_fits is what a fitted part changes on the wheels (part name -> wheel fitment records, struct 9a172293: a widebody kit, spacers): offset and spacing in metres (both taken as moving the wheel out from the centre line), width in metres added to the tyre, axles 0, 2 or 3 (taken as front, rear and both; not confirmed), relative as stored. lights are the vehicle's light patterns (sirens, strobes, indicators): patterns by name with slot, speed, clock and per lamp the bone name hash (joaat of siren12, indicator_lf ...) and steps of [phase offset, flash type number]; pack names the patterns of the vehicle's own light pack, a configuration's light_patterns the ones its parts link; archetypes gives a lamp bone's light set name (red, amber, led ..., a hash where the name is not known) and lamp_lights its light as [RRGGBB, intensity], from that light set of its light archetype in levels.rpf. What a flash type number looks like over time is not decoded. steering_lock is fSteeringLock of the model's handling record (radians, the largest angle of the steered wheels; from the PSO files in data.rpf, joined by handlingId). display_name and make are the game's own text for the model's name hash and its vehicleMakeName (from the first text table in data.rpf that has them: English). Names resolve where known; others are joaat hex.",
        "format": FORMAT,
        "sources": sources,
        "vehicles": list,
    }))?)?;
    Ok(json!({"models": list.len(), "named": named, "with_model_info": with_info, "configurations": n_cfg, "configurations_with_rim": n_rim, "palettes": n_pal, "display_names": n_named, "shared_parts": n_shared, "wheel_fits": n_fits, "switches": n_switches, "light_patterns": n_lights, "steering_locks": n_lock, "file": path}))
}
