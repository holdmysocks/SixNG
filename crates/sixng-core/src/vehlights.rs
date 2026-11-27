//! Vehicle light patterns: which lamps flash, and how, with the siren on.
//!
//! A pattern record (`0f93517e`) lists lamps by the hash of their bone
//! (`siren12`, `indicator_lf`, `strobelight_1` ...), each with steps of a
//! phase offset (half float, mostly 0..1) and a flash type number. The
//! records sit in three places: the vehicle library (data.rpf), a shared
//! pattern pack and a light pack per emergency vehicle (packs.rpf).
//!
//! A configuration reaches a pattern through its parts: part record
//! (`0caef606`) -> modifier list (`61e1e572`) -> link record (`da592d57`)
//! -> pattern names (`1ff6a884`). A light pack whose pattern a vehicle
//! links is taken as that vehicle's; it also holds patterns no part links
//! (the ambulance's has 9, 3 linked), given as the vehicle's `pack`
//! patterns, since how the game picks among them is not known.
//!
//! A lamp's light: its record (`ba860a07`) names a light set (`066532dd`)
//! of a light archetype (`ea42cb63`: in the library the set name in the
//! high half and the archetype's name hash in the low half, in the packs
//! the archetype's 64-bit id). The archetypes are ordinary archetypes of
//! levels.rpf (`82d6fc83`, name at +0x74, id at +0) whose extension holds
//! named light sets (`76402c3c`) of lights (`f54b227b`, colour and
//! intensity). All of the vehicles' sit in one resource; it is found by a
//! scan of levels.rpf (two minutes) and kept in
//! `<work>/index/vehicle_light_archetypes.json`.
//!
//! Not decoded: what each flash type number looks like over time (there is
//! no table of them in the meta resources; 29 numbers are in use) and the
//! two clocks (`1b953013`).

use crate::meta7a::{self, Meta};
use crate::resource::Resource;
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

const PATTERN: u32 = 0x0f93_517e;
const PATTERN_LAMPS: u32 = 0xd3d4_919b;
const PATTERN_SPEED: u32 = 0x699d_d4d9;
const PATTERN_CLOCK: u32 = 0x1b95_3013;
const SLOT: u32 = 0x37a6_f9ef;
const LAMP_BONE: u32 = 0x3950_39a9;
const LAMP_STEPS: u32 = 0xa478_703e;
const STEP_OFFSET: u32 = 0xa13c_79ef;
/// The flash type: a byte in the packs, a u16 in the library.
const STEP_TYPE: [u32; 2] = [0x5b92_fa4b, 0xfc46_055f];
const NAME: u32 = 0xe6e1_7f14;
const PART: u32 = 0x0cae_f606;
const PART_LIST: u32 = 0x61e1_e572;
const LINK: u32 = 0xda59_2d57;
const LINK_PATTERNS: u32 = 0x1ff6_a884;
const LAMP_SET: u32 = 0x8654_00aa;
const SET_LAMPS: u32 = 0x804f_dca7;
const LAMP_REF: u32 = 0x079b_aba7;
const LAMP_ARCHETYPE: u32 = 0x0665_32dd;
const LAMP_ARCH_REF: u32 = 0xea42_cb63;
const LINK_SETS: u32 = 0xdd46_6dfe;
const LIGHT_SET: u32 = 0x7640_2c3c;
const ARCH_ID: u32 = 0x8bfb_6095;
const LIGHT_TABLE_FORMAT: u32 = 1;

/// Lamp archetype names found by guessing (joaat of the lower-case name).
const ARCHETYPES: [&str; 9] = ["red", "amber", "halogen", "halogen_warm", "led", "led_warm_white", "warm_white", "indicator_amber", "amber_roof_spotlight"];

#[derive(Clone, Debug)]
pub struct Pattern {
    pub name: u32,
    pub slot: u32,
    pub speed: f32,
    pub clock: String,
    /// bone hash, then its steps: (offset, flash type)
    pub lamps: Vec<(u32, Vec<(f32, u32)>)>,
}

#[derive(Default)]
struct Pack {
    patterns: Vec<Pattern>,
    /// part name -> modifier names
    parts: HashMap<u32, Vec<u32>>,
    /// link name -> pattern names
    links: HashMap<u32, Vec<u32>>,
    /// lamp bone hash -> archetype name hash
    archetypes: HashMap<u32, u32>,
    /// lamp set name -> lamps: (bone hash, light set name, archetype reference)
    lamp_sets: HashMap<u32, Vec<(u32, u32, u64)>>,
    /// link name -> lamp set names
    link_sets: HashMap<u32, Vec<u32>>,
}

/// The named light sets of the vehicles' light archetypes.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct LightTable {
    format: u32,
    /// every resource of levels.rpf was looked at
    complete: bool,
    /// archetype name hash, archetype id (hex), light set name hash, RGB,
    /// intensity (of the set's brightest light)
    sets: Vec<(u32, String, u32, [u8; 3], f32)>,
}

#[derive(Default)]
pub struct Index {
    patterns: HashMap<u32, Pattern>,
    parts: HashMap<u32, Vec<u32>>,
    links: HashMap<u32, Vec<u32>>,
    /// the light packs of single vehicles
    packs: Vec<Pack>,
    lamp_sets: HashMap<u32, Vec<(u32, u32, u64)>>,
    link_sets: HashMap<u32, Vec<u32>>,
    /// (archetype name, light set name) -> RGB, intensity
    lights: HashMap<(u32, u32), ([u8; 3], f32)>,
    /// archetype id -> name
    arch_ids: HashMap<u64, u32>,
    pub n_patterns: usize,
}

fn get<'v>(v: &'v Value, h: u32) -> Option<&'v Value> {
    v.get(meta7a::field_name(h))
}

fn u32s(v: Option<&Value>) -> Vec<u32> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as u32)).collect()).unwrap_or_default()
}

fn read(m: &Meta) -> Pack {
    let mut p = Pack::default();
    let name = |v: &Value| get(v, NAME).and_then(|x| x.as_u64()).map(|x| x as u32);
    for o in m.objects(PATTERN) {
        let Some(v) = m.object(PATTERN, o, 0) else { continue };
        let Some(n) = name(&v) else { continue };
        let lamps = get(&v, PATTERN_LAMPS)
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|l| {
                        let bone = get(l, LAMP_BONE)?.as_u64()? as u32;
                        let steps = get(l, LAMP_STEPS)?
                            .as_array()?
                            .iter()
                            .map(|s| (get(s, STEP_OFFSET).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32, STEP_TYPE.iter().find_map(|h| get(s, *h)?.as_u64()).unwrap_or(0) as u32))
                            .collect();
                        Some((bone, steps))
                    })
                    .collect()
            })
            .unwrap_or_default();
        p.patterns.push(Pattern {
            name: n,
            slot: get(&v, SLOT).and_then(|x| x.as_u64()).unwrap_or(0) as u32,
            speed: get(&v, PATTERN_SPEED).and_then(|x| x.as_f64()).unwrap_or(1.0) as f32,
            clock: get(&v, PATTERN_CLOCK).and_then(|x| x.as_str()).unwrap_or("").to_string(),
            lamps,
        });
    }
    for o in m.objects(PART) {
        let Some(v) = m.object(PART, o, 0) else { continue };
        if let Some(n) = name(&v) {
            p.parts.insert(n, u32s(get(&v, PART_LIST)));
        }
    }
    for o in m.objects(LINK) {
        let Some(v) = m.object(LINK, o, 0) else { continue };
        let pats = u32s(get(&v, LINK_PATTERNS));
        if let (Some(n), false) = (name(&v), pats.is_empty()) {
            p.links.insert(n, pats);
        }
        let sets = u32s(get(&v, LINK_SETS));
        if let (Some(n), false) = (name(&v), sets.is_empty()) {
            p.link_sets.insert(n, sets);
        }
    }
    for o in m.objects(LAMP_SET) {
        let Some(v) = m.object(LAMP_SET, o, 0) else { continue };
        let mut lamps = Vec::new();
        for l in get(&v, SET_LAMPS).and_then(|x| x.as_array()).into_iter().flatten() {
            // the bone is the low half of a 64-bit reference
            let bone = get(l, LAMP_REF).and_then(|x| x.as_str()).and_then(|s| u32::from_str_radix(s.get(8..)?, 16).ok());
            if let (Some(b), Some(a)) = (bone, get(l, LAMP_ARCHETYPE).and_then(|x| x.as_u64())) {
                p.archetypes.insert(b, a as u32);
                let arch = get(l, LAMP_ARCH_REF).and_then(|x| x.as_str()).and_then(|s| u64::from_str_radix(s, 16).ok()).unwrap_or(0);
                lamps.push((b, a as u32, arch));
            }
        }
        if let Some(n) = name(&v) {
            p.lamp_sets.insert(n, lamps);
        }
    }
    p
}

/// Every light pattern of the library and the packs.
pub fn build(game_root: &Path, work: &Path) -> Result<Index> {
    let mut ix = Index::default();
    for archive in ["data.rpf", "packs.rpf"] {
        let ixp = crate::scan::index_path(&work.join("index"), archive);
        let es: Vec<crate::scan::Entry> =
            crate::scan::load_index(&ixp).map_err(|e| anyhow!("{archive} index ({}): {e}; index it first", ixp.display()))?.entries.into_iter().filter(|e| e.kind == crate::classify::Kind::Resource && e.sub == "7a").collect();
        let path = game_root.join(archive);
        let found: Vec<(bool, Pack)> = es
            .par_iter()
            .filter_map(|e| {
                let r = Resource::load_sys(&path, e).ok()?;
                let m = Meta::parse(&r)?;
                if !m.structs.contains_key(&PATTERN) && !m.structs.contains_key(&LINK) {
                    return None;
                }
                // a light pack of one vehicle: patterns beside its lamp
                // sets, unlike the shared pattern pack (no lamp sets) and
                // the merged tables (every vehicle's lamp sets)
                let sets = m.objects(LAMP_SET).len();
                let own = archive == "packs.rpf" && (1..100).contains(&sets);
                let p = read(&m);
                Some((own && !p.patterns.is_empty(), p))
            })
            .collect();
        for (own, p) in found {
            ix.n_patterns += p.patterns.len();
            for pt in &p.patterns {
                ix.patterns.entry(pt.name).or_insert_with(|| pt.clone());
            }
            for (k, v) in &p.parts {
                // only parts that reach a link are worth keeping
                if !v.is_empty() {
                    ix.parts.entry(*k).or_default().extend(v);
                }
            }
            for (k, v) in &p.links {
                ix.links.entry(*k).or_default().extend(v);
            }
            for (k, v) in &p.link_sets {
                ix.link_sets.entry(*k).or_default().extend(v);
            }
            for (k, v) in &p.lamp_sets {
                ix.lamp_sets.entry(*k).or_insert_with(|| v.clone());
            }
            if own {
                ix.packs.push(p);
            }
        }
    }
    let (links, link_sets) = (&ix.links, &ix.link_sets);
    ix.parts.retain(|_, v| v.iter().any(|m| links.contains_key(m) || link_sets.contains_key(m)));
    // the light archetypes the lamps name
    let (mut names, mut ids) = (HashSet::new(), HashSet::new());
    for &(_, set, arch) in ix.lamp_sets.values().flatten() {
        if (arch >> 32) as u32 == set {
            names.insert(arch as u32);
        } else if arch != 0 {
            ids.insert(arch);
        }
    }
    let t = light_table(game_root, work, &names, &ids);
    for (arch, id, set, rgb, intensity) in t.sets {
        ix.lights.insert((arch, set), (rgb, intensity));
        if let Ok(id) = u64::from_str_radix(&id, 16) {
            ix.arch_ids.insert(id, arch);
        }
    }
    Ok(ix)
}

/// The light sets of one resource's archetypes, when it holds one of the
/// wanted archetypes.
fn light_rows(path: &Path, e: &crate::scan::Entry, names: &HashSet<u32>, ids: &HashSet<u64>) -> Option<Vec<(u32, String, u32, [u8; 3], f32)>> {
    let r = Resource::load_sys(path, e).ok()?;
    let m = Meta::parse(&r)?;
    if !m.structs.contains_key(&LIGHT_SET) {
        return None;
    }
    let objs = m.objects(crate::lights::ARCHETYPE);
    if !objs.iter().any(|&o| names.contains(&r.u32(o + 0x74)) || ids.contains(&r.u64(o))) {
        return None;
    }
    let mut rows = Vec::new();
    for o in objs {
        let Some(v) = m.object(crate::lights::ARCHETYPE, o, 0) else { continue };
        let (Some(arch), Some(id)) = (get(&v, NAME).and_then(|x| x.as_u64()), get(&v, ARCH_ID).and_then(|x| x.as_str())) else { continue };
        for ext in v["extensions"].as_array().into_iter().flatten() {
            for set in ext["instances"].as_array().into_iter().flatten() {
                let Some(n) = get(set, NAME).and_then(|x| x.as_u64()) else { continue };
                // the set's brightest light
                let best = set["instances"].as_array().into_iter().flatten().filter_map(|l| {
                    let c = l["colour"].as_array()?;
                    let rgb: [u8; 3] = std::array::from_fn(|k| c.get(k).and_then(|x| x.as_u64()).unwrap_or(255) as u8);
                    Some((rgb, l["intensity"].as_f64().unwrap_or(0.0) as f32))
                });
                if let Some((rgb, i)) = best.max_by(|a, b| a.1.total_cmp(&b.1)) {
                    rows.push((arch as u32, id.to_string(), n as u32, rgb, i));
                }
            }
        }
    }
    Some(rows)
}

/// The light sets of the wanted archetypes, from the cache or a scan of
/// levels.rpf's meta resources: the first resource (in index order) that
/// holds one is taken
/// when it covers them all (it does: the vehicles' light archetypes share a
/// resource), otherwise every resource is read.
fn light_table(game_root: &Path, work: &Path, names: &HashSet<u32>, ids: &HashSet<u64>) -> LightTable {
    let covers = |t: &LightTable| {
        let (have_n, have_i): (HashSet<u32>, HashSet<u64>) = (t.sets.iter().map(|s| s.0).collect(), t.sets.iter().filter_map(|s| u64::from_str_radix(&s.1, 16).ok()).collect());
        names.iter().all(|n| have_n.contains(n)) && ids.iter().all(|i| have_i.contains(i))
    };
    let p = work.join("index").join("vehicle_light_archetypes.json");
    if let Some(t) = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<LightTable>(&b).ok()).filter(|t| t.format == LIGHT_TABLE_FORMAT && (t.complete || covers(t))) {
        return t;
    }
    let Ok(ix) = crate::scan::load_index(&crate::scan::index_path(&work.join("index"), "levels.rpf")) else { return LightTable::default() };
    let es: Vec<&crate::scan::Entry> = ix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Resource && e.sub == "7a").collect();
    let path = game_root.join("levels.rpf");
    let mut t = LightTable { format: LIGHT_TABLE_FORMAT, complete: false, sets: es.par_iter().find_map_first(|e| light_rows(&path, e, names, ids)).unwrap_or_default() };
    if !covers(&t) {
        t.sets = es.par_iter().filter_map(|e| light_rows(&path, e, names, ids)).flatten().collect();
        t.complete = true;
    }
    if let Ok(b) = serde_json::to_vec(&t) {
        let _ = crate::atomic::write(&p, b);
    }
    t
}

fn archetype_name(h: u32) -> String {
    ARCHETYPES.iter().find(|n| meta7a::joaat(n) == h).map_or_else(|| format!("{h:08x}"), |n| n.to_string())
}

fn pattern_json(p: &Pattern) -> Value {
    json!({
        "slot": format!("{:08x}", p.slot),
        "speed": p.speed,
        "clock": p.clock,
        "lamps": p.lamps.iter().map(|(b, s)| json!({"bone": format!("{b:08x}"), "steps": s.iter().map(|(o, t)| json!([o, t])).collect::<Vec<_>>()})).collect::<Vec<_>>(),
    })
}

impl Index {
    /// The patterns the parts of one configuration link, in part order.
    pub fn linked(&self, parts: &[u32]) -> Vec<u32> {
        let mut out = Vec::new();
        for p in parts {
            for m in self.parts.get(p).into_iter().flatten() {
                for n in self.links.get(m).into_iter().flatten() {
                    if !out.contains(n) {
                        out.push(*n);
                    }
                }
            }
        }
        out
    }

    /// The light of a lamp: RGB and intensity of its light set.
    fn light(&self, set: u32, arch: u64) -> Option<([u8; 3], f32)> {
        let a = if (arch >> 32) as u32 == set { arch as u32 } else { *self.arch_ids.get(&arch)? };
        self.lights.get(&(a, set)).copied()
    }

    /// What a vehicle with these parts (over all its configurations) has:
    /// `patterns` by name (the linked ones and its light packs'), `pack`
    /// (the names in its light packs), `archetypes` (lamp bone hash ->
    /// light set name) and `lamp_lights` (lamp bone hash -> [RRGGBB,
    /// intensity] of its light set). None without any pattern.
    pub fn for_vehicle(&self, parts: &BTreeSet<u32>) -> Option<Value> {
        let mut pats: BTreeMap<String, Value> = BTreeMap::new();
        let mut pack = Vec::new();
        let mut arch: BTreeMap<String, String> = BTreeMap::new();
        let mut lamp_lights: BTreeMap<String, Value> = BTreeMap::new();
        let mut add = |lamps: &[(u32, u32, u64)], arch: &mut BTreeMap<String, String>| {
            for &(bone, set, a) in lamps {
                arch.entry(format!("{bone:08x}")).or_insert_with(|| archetype_name(set));
                if let Some((rgb, i)) = self.light(set, a) {
                    lamp_lights.entry(format!("{bone:08x}")).or_insert_with(|| json!([format!("{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]), i]));
                }
            }
        };
        let all: Vec<u32> = parts.iter().copied().collect();
        let linked = self.linked(&all);
        // its light packs: the ones holding a pattern it links
        for p in self.packs.iter().filter(|p| p.patterns.iter().any(|pt| linked.contains(&pt.name))) {
            for pt in &p.patterns {
                pats.insert(format!("{:08x}", pt.name), pattern_json(pt));
                pack.push(format!("{:08x}", pt.name));
            }
            for (b, a) in &p.archetypes {
                arch.insert(format!("{b:08x}"), archetype_name(*a));
            }
            // lamp sets by name hash, lowest first: a bone in several sets
            // takes the lowest set's light, the same on every run (the map's
            // own order is random per process)
            let mut sets: Vec<_> = p.lamp_sets.iter().collect();
            sets.sort_unstable_by_key(|(k, _)| **k);
            for (_, lamps) in sets {
                add(lamps, &mut arch);
            }
        }
        // the lamp sets its parts link
        for p in &all {
            for m in self.parts.get(p).into_iter().flatten() {
                for s in self.link_sets.get(m).into_iter().flatten() {
                    if let Some(lamps) = self.lamp_sets.get(s) {
                        add(lamps, &mut arch);
                    }
                }
            }
        }
        drop(add);
        for n in linked {
            if let Some(pt) = self.patterns.get(&n) {
                pats.entry(format!("{n:08x}")).or_insert_with(|| pattern_json(pt));
            }
        }
        (!pats.is_empty() || !lamp_lights.is_empty()).then(|| json!({"patterns": pats, "pack": pack, "archetypes": arch, "lamp_lights": lamp_lights}))
    }
}
