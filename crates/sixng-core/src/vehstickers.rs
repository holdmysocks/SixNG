//! Vehicle liveries as data: which stickers a brand puts where.
//!
//! A livery is a "brand": a vehicle variation record (`fc1c7000`) such as
//! `BENSON_BRAND_BAUMHARDERDAIRY`. Like any variation it gives slots with
//! weighted part options and can build on parent variations (`03ae1cb0`).
//! Some of its parts list **sticker entries** (`86cdf3df`) in their
//! modifier list (`61e1e572`). An entry names
//!
//! - a sticker record (`ed9cf5df`, vehicle library): a texture by its
//!   64-bit id in the vehicle texture stores, colours, an aspect class;
//! - a layout group and a layout in it (one PSO file of data.rpf, structs
//!   `b0dd20cb` and `e2d9c73a`): the sticker zones it covers, a position, a
//!   size, a rotation in degrees and the UV set to use.
//!
//! A vertex's sticker zone is the fourth byte of the mesh's second colour
//! stream (GLB `_COLOR1`). The two-layer vehicle shader draws a sticker
//! where its transformed UV is between 0 and 1; the size acts as a scale
//! (a larger number is a smaller sticker).
//!
//! Not decoded: the exact map from a layout's position to the sticker's
//! place in UV space (about: a constant per panel group minus half the
//! position), what the layout's `Location`, `80c88a97` and `9f8b8007` mean,
//! and how the game picks a brand for a vehicle.

use crate::meta7a::{self, Meta};
use crate::resource::Resource;
use anyhow::Result;
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

const VARIATION: u32 = 0xfc1c_7000;
const OWNER: u32 = 0xfb1f_80ff;
const PARENTS: u32 = 0x03ae_1cb0;
const PART: u32 = 0x0cae_f606;
const PART_LIST: u32 = 0x61e1_e572;
const ENTRY: u32 = 0x86cd_f3df;
const ENTRY_GROUP: u32 = 0xb13f_2c2f;
const STICKER: u32 = 0xed9c_f5df;
const STICKER_TEXTURE: u32 = 0x3950_a760;
const STICKER_COLOURS: [u32; 3] = [0x9b5d_cd73, 0x9dbb_d803, 0xb575_2ae0];
const GROUP: u32 = 0xb0dd_20cb;
const GROUP_LAYOUTS: u32 = 0xc19c_0de4;
/// The list of groups in the layout file's root record.
const ROOT_GROUPS: u32 = 0x3274_d53f;
const LAYOUT_ZONES: u32 = 0x20b7_57b6;
const LAYOUT_POS: [u32; 2] = [0x26ba_b388, 0x79dd_59cc];
const LAYOUT_SIZE: [u32; 2] = [0x8eed_562c, 0x9d2f_f2b1];
const LAYOUT_ASPECT: u32 = 0x041e_bd1d;
const LAYOUT_UV: u32 = 0x4d76_3aa5;
const LAYOUT_OTHER: [u32; 2] = [0x80c8_8a97, 0x9f8b_8007];

fn get<'v>(v: &'v Value, h: u32) -> Option<&'v Value> {
    v.get(meta7a::field_name(h))
}

fn named<'v>(v: &'v Value, name: &str) -> Option<&'v Value> {
    get(v, meta7a::joaat(name))
}

fn u(v: Option<&Value>) -> u32 {
    v.and_then(|x| x.as_u64()).unwrap_or(0) as u32
}

fn list(v: Option<&Value>) -> Vec<u32> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as u32)).collect()).unwrap_or_default()
}

fn hex(h: u32) -> String {
    format!("{h:08x}")
}

fn name_of(h: u32) -> Option<String> {
    crate::names::all(h).into_iter().next().map(|x| x.0)
}

/// One resource's records: variations, part modifier lists, sticker
/// entries and sticker records, by name.
#[derive(Default)]
struct Records {
    variations: Vec<Value>,
    parts: HashMap<u32, Vec<u32>>,
    entries: HashMap<u32, Value>,
    stickers: HashMap<u32, Value>,
}

fn records(m: &Meta) -> Records {
    let dump = |h: u32, depth: u32| m.objects(h).into_iter().filter_map(|o| m.object(h, o, depth)).collect::<Vec<_>>();
    let by_name = |h: u32| dump(h, 0).into_iter().map(|v| (u(named(&v, "name")), v)).collect::<HashMap<u32, Value>>();
    Records {
        variations: dump(VARIATION, 3),
        parts: dump(PART, 0).iter().map(|p| (u(named(p, "name")), list(get(p, PART_LIST)))).filter(|(_, l)| !l.is_empty()).collect(),
        entries: by_name(ENTRY),
        stickers: by_name(STICKER),
    }
}

fn metas(ws: &crate::export::Workspace, archive: &str) -> Result<Vec<Records>> {
    let a = ws.resolve(archive)?;
    let ix = ws.load_index(&a)?;
    Ok(ix
        .entries
        .par_iter()
        .filter(|e| e.kind == crate::classify::Kind::Resource && e.sub == "7a")
        .filter_map(|e| {
            let r = Resource::load_sys(&a.path, e).ok()?;
            let m = Meta::parse(&r)?;
            let has = |h: u32| m.structs.contains_key(&h);
            (has(VARIATION) || has(ENTRY) || has(STICKER)).then(|| records(&m))
        })
        .collect())
}

/// The layout groups of data.rpf: group name -> layout name -> layout.
fn layouts(ws: &crate::export::Workspace) -> Result<HashMap<u32, HashMap<u32, Value>>> {
    let a = ws.resolve("data")?;
    let ix = ws.load_index(&a)?;
    let found: Vec<Vec<Value>> = ix
        .entries
        .par_iter()
        .filter(|e| e.kind == crate::classify::Kind::Pso)
        .filter_map(|e| {
            let p = crate::pso::parse(&crate::scan::read_entry(&a.path, e).ok()?)?;
            if !p.structs.contains_key(&GROUP) {
                return None;
            }
            let m = p.meta();
            // the groups are the elements of the root record's list
            let roots: Vec<u32> = p.structs.iter().filter(|(_, sd)| sd.members.iter().any(|x| x.hash == ROOT_GROUPS)).map(|(h, _)| *h).collect();
            let mut groups = Vec::new();
            for &h in &roots {
                for o in m.objects(h) {
                    if let Some(r) = m.object(h, o, 4) {
                        groups.extend(get(&r, ROOT_GROUPS).and_then(|x| x.as_array()).cloned().unwrap_or_default());
                    }
                }
            }
            Some(groups)
        })
        .collect();
    let mut out: HashMap<u32, HashMap<u32, Value>> = HashMap::new();
    for g in found.into_iter().flatten() {
        let e = out.entry(u(named(&g, "name"))).or_default();
        for l in get(&g, GROUP_LAYOUTS).and_then(|x| x.as_array()).into_iter().flatten() {
            e.entry(u(named(l, "name"))).or_insert_with(|| l.clone());
        }
    }
    Ok(out)
}

fn layout_json(l: &Value) -> Value {
    let f = |h: u32| get(l, h).and_then(|x| x.as_f64()).unwrap_or(0.0);
    json!({
        "zones": list(get(l, LAYOUT_ZONES)),
        "position": [f(LAYOUT_POS[0]), f(LAYOUT_POS[1])],
        "size": [f(LAYOUT_SIZE[0]), f(LAYOUT_SIZE[1])],
        "rotation": named(l, "rotation").and_then(|x| x.as_f64()).unwrap_or(0.0),
        "uv_mode": u(get(l, LAYOUT_UV)),
        "aspect": u(get(l, LAYOUT_ASPECT)),
        "location": hex(u(named(l, "location"))),
        "other": [u(get(l, LAYOUT_OTHER[0])), u(get(l, LAYOUT_OTHER[1]))],
    })
}

/// Write `<out>/vehicles/liveries.json`: per vehicle its brands, each with
/// the stickers it places. `models`: joaat -> model name.
pub fn build(ws: &crate::export::Workspace, models: &HashMap<u32, String>, out: &std::path::Path) -> Result<Value> {
    let library = metas(ws, "data")?;
    let packs = metas(ws, "packs")?;
    let groups = layouts(ws)?;
    // library records by name (parents, shared parts, stickers)
    let mut lib_var: HashMap<u32, &Value> = HashMap::new();
    let mut lib_parts: HashMap<u32, &Vec<u32>> = HashMap::new();
    let mut lib_entries: HashMap<u32, &Value> = HashMap::new();
    let mut stickers: HashMap<u32, &Value> = HashMap::new();
    for r in &library {
        lib_var.extend(r.variations.iter().map(|v| (u(named(v, "name")), v)));
        lib_parts.extend(r.parts.iter().map(|(k, v)| (*k, v)));
        lib_entries.extend(r.entries.iter().map(|(k, v)| (*k, v)));
        stickers.extend(r.stickers.iter().map(|(k, v)| (*k, v)));
    }
    let mut vehicles: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let (mut n_brands, mut n_stickers, mut n_unplaced) = (0, 0, 0);
    for r in &packs {
        let own: HashMap<u32, &Value> = r.variations.iter().map(|v| (u(named(v, "name")), v)).collect();
        for v in &r.variations {
            let owner = u(get(v, OWNER));
            if owner == 0 {
                continue;
            }
            // the variation's parts: its parents' first (a slot of the
            // child replaces the parent's), then its own
            let mut chain: Vec<&Value> = Vec::new();
            let mut todo = vec![v];
            while let Some(x) = todo.pop().filter(|_| chain.len() < 16) {
                chain.push(x);
                todo.extend(list(get(x, PARENTS)).iter().filter_map(|p| own.get(p).or_else(|| lib_var.get(p)).copied()));
            }
            let mut slots: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for x in chain.iter().rev() {
                for s in named(x, "slots").and_then(|s| s.as_array()).into_iter().flatten() {
                    let mods: Vec<u32> = named(s, "options").and_then(|o| o.as_array()).into_iter().flatten().map(|o| u(named(o, "mod"))).collect();
                    slots.insert(u(named(s, "slot")), mods);
                }
            }
            let mut placed: Vec<Value> = Vec::new();
            for part in slots.values().flatten() {
                let Some(mods) = r.parts.get(part).or_else(|| lib_parts.get(part).copied()) else { continue };
                for e in mods.iter().filter_map(|m| r.entries.get(m).or_else(|| lib_entries.get(m).copied())) {
                    let (sticker, group, layout) = (u(named(e, "sticker")), u(get(e, ENTRY_GROUP)), u(named(e, "layout")));
                    let s = r.stickers.get(&sticker).or_else(|| stickers.get(&sticker).copied());
                    let l = groups.get(&group).and_then(|g| g.get(&layout));
                    if l.is_none() {
                        n_unplaced += 1;
                    }
                    placed.push(json!({
                        "part": hex(*part),
                        "sticker": hex(sticker),
                        "texture": s.and_then(|s| get(s, STICKER_TEXTURE)).and_then(|x| x.as_str()),
                        "colours": s.map(|s| STICKER_COLOURS.iter().map(|h| hex(u(get(s, *h)))).collect::<Vec<_>>()),
                        "group": hex(group),
                        "layout_name": hex(layout),
                        "layout": l.map(layout_json),
                    }));
                }
            }
            if placed.is_empty() {
                continue;
            }
            n_brands += 1;
            n_stickers += placed.len();
            let name = u(named(v, "name"));
            let model = models.get(&owner).cloned().unwrap_or_else(|| hex(owner));
            vehicles.entry(model).or_default().push(json!({"brand": hex(name), "name": name_of(name), "stickers": placed}));
        }
    }
    for b in vehicles.values_mut() {
        b.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()).then(a["brand"].as_str().cmp(&b["brand"].as_str())));
        b.dedup_by(|a, b| a["brand"] == b["brand"]);
    }
    let stats = json!({"vehicles": vehicles.len(), "brands": n_brands, "stickers": n_stickers, "without_layout": n_unplaced, "layout_groups": groups.len(), "layouts": groups.values().map(|g| g.len()).sum::<usize>(), "sticker_records": stickers.len()});
    let doc = json!({
        "format": 1,
        "note": "Vehicle liveries. Per vehicle its brands (variation records whose parts place stickers), each with its stickers: the part that carries the entry, the sticker record, its texture (64-bit id in the vehicle texture stores) and colours (ARGB as stored), and the layout: zones = the sticker zones it covers (a vertex's zone is the fourth byte of the mesh's second colour stream, GLB _COLOR1), position and size as stored, rotation in degrees, uv_mode (1 = first UV set, 3 = second, 4 = third; 0 and 2 = the second half of the first), aspect (0 square, 1 two to one, 2 four to one). The size acts as a scale: a larger number is a smaller sticker. The exact map from position to UV space is not decoded (about: a constant per panel group minus half the position); location and other are not understood. A brand also lists the stickers of its parent variations. How the game picks a brand for a vehicle is not known.",
        "stats": stats,
        "vehicles": vehicles,
    });
    let dir = out.join("vehicles");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("liveries.json"), serde_json::to_vec(&doc)?)?;
    Ok(stats)
}
