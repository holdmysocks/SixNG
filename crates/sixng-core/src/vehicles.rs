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
/// Layout version of vehicles.json; the model viewer rebuilds older files.
pub const FORMAT: u32 = 4;
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

/// The mod parts of a vehicle pack that have models: `{name, slot,
/// elements: [{bone, hides}]}`, all joaat hashes as 8 hex digits. Records
/// without elements (colours, liveries, wheel choices) are left out.
fn mod_parts(mods: &[Value], elements: &[Value]) -> Vec<Value> {
    let hex = |h: u32| format!("{h:08x}");
    let by_name: HashMap<u32, &Value> = elements.iter().filter_map(|e| Some((get(e, NAME)?.as_u64()? as u32, e))).collect();
    mods.iter()
        .filter_map(|m| {
            let els: Vec<Value> = u32s(get(m, ELEMENTS))
                .iter()
                .filter_map(|n| by_name.get(n))
                .map(|e| {
                    // a u64 written as 16 hex digits; the bone is its low half
                    let bone = get(e, ELEMENT_BONE).and_then(|x| x.as_str()).and_then(|s| u32::from_str_radix(s.get(8..16)?, 16).ok()).unwrap_or(0);
                    json!({"bone": hex(bone), "hides": u32s(get(e, ELEMENT_HIDES)).into_iter().map(hex).collect::<Vec<_>>()})
                })
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
    let found: Vec<(String, Vec<Value>, Vec<Value>, Vec<Value>, Vec<Value>)> = metas
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
            let mods = if infos.len() == 1 { mod_parts(&dump(MOD), &dump(MOD_ELEMENT)) } else { Vec::new() };
            Some((e.id.clone(), infos, dump(CONFIG), dump(VARIATION), mods))
        })
        .collect();
    let name = |h: u32| names.get(&h).cloned();
    let mut models: BTreeMap<u32, Value> = BTreeMap::new();
    let mut sources = Vec::new();
    let (mut n_cfg, mut n_pal, mut n_rim) = (0, 0, 0);
    // Per-vehicle pack resources repeat rows of the merged table: keep each
    // distinct row once per model.
    let mut seen: std::collections::HashSet<(u32, String)> = std::collections::HashSet::new();
    let mut by_handling: Vec<(u32, Value)> = Vec::new();
    let mut mods_of: Vec<(u32, Vec<Value>)> = Vec::new();
    for (id, infos, cfgs, vars, mods) in found {
        sources.push(id);
        if let Some(mi) = infos.first().filter(|_| !mods.is_empty()) {
            let h = get(mi, HANDLING_ID).and_then(|x| x.as_u64()).filter(|&h| h != 0).or_else(|| get(mi, GAME_NAME).and_then(|x| x.as_u64())).unwrap_or(0) as u32;
            mods_of.push((h, mods));
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
            e["configurations"].as_array_mut().unwrap().push(json!({
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
    let list: Vec<&Value> = models.values().collect();
    let with_info = list.iter().filter(|m| !m["model_info"].is_null()).count();
    let named = list.iter().filter(|m| !m["model"].is_null()).count();
    let dir = out.join("vehicles");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("vehicles.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&json!({
        "note": "Per vehicle model: model_info (vehicles.meta-like; wheelScale / wheelScaleRear read as the rim radius in metres (0.217 on a BMX, 0.25 / 0.22 on a dirt bike); the shared rim_* models have radius 0.5, so the model viewer scales rim and tyre by wheelScale / 0.5. The game shapes the tyre through the wheel model's own bones, which is not decoded), configurations (body colour ARGB + the part models incl. rim and tyre), palettes (ARGB paint-colour lists; colours repeat, so repetition reads as the draw weight), part_groups (weighted part choices). Names resolve where known; others are joaat hex.",
        "format": FORMAT,
        "sources": sources,
        "vehicles": list,
    }))?)?;
    Ok(json!({"models": list.len(), "named": named, "with_model_info": with_info, "configurations": n_cfg, "configurations_with_rim": n_rim, "palettes": n_pal, "file": path}))
}
