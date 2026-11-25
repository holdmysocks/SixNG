//! Interior membership of map entities (`sixng interior-entities`).
//!
//! Interior dressing does not sit in ordinary map files. It sits in
//! interior archetype files: meta resources with one or more interior (MLO)
//! archetypes (`10506455`, GTA V `CMloArchetypeDef`) and no map data struct.
//! Their entity list is the interiors' contents in interior-local space
//! (bbox around the origin, not world positions). Each interior lists its
//! entities, its rooms (`0b1811f1`; attachedObjects index the interior's
//! entity list) and its entity sets (optional states, `d6a799f9`). The world
//! places an interior only through interior instances in a map file (see
//! `sixng interiors`). Files with a named entity group (`35abc248`) are
//! group-local (prefab-like) and are not placed by any map data either.
//!
//! Per file, `space` is one of:
//! - `world`: ordinary map file, world positions;
//! - `interior`: interior archetype file, interior-local positions;
//! - `world+interior`: map data plus interior archetypes (rare);
//! - `group`: group-local positions.
//!
//! On levels.rpf: 540 K entities in interior files (389 K in a room, 139 K
//! in an entity set, 7 K in an interior's list but no room), 40 K in group
//! files, 1.67 M in world files.

use crate::game::ArchiveInfo;
use crate::scan::Entry;
use crate::export::{Progress, Workspace};
use crate::meta7a::{self, Meta};
use crate::resource::Resource;
use anyhow::Result;
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

fn hex(h: u32) -> String {
    format!("{h:08x}")
}

fn name(h: u32) -> Option<String> {
    crate::names::lookup(h, crate::names::Ctx::Field)
}

/// Per-file interior info: (space, interiors summary, per-entity membership).
pub struct FileInfo {
    pub space: &'static str,
    pub interiors: Vec<Value>,
    /// Entity index -> {interior, room, room_name, entity_set, entity_set_name}.
    pub entities: HashMap<usize, Value>,
}

/// Interior info of one parsed meta resource.
pub fn file_info(m: &Meta) -> FileInfo {
    let mlos = meta7a::mlos(m);
    let has_map = !m.objects(meta7a::MAPDATA).is_empty();
    let group = !m.objects(0x35ab_c248).is_empty();
    let space = match (mlos.is_empty(), has_map, group) {
        (false, true, _) => "world+interior",
        (false, false, _) => "interior",
        (true, _, true) => "group",
        _ => "world",
    };
    let mut entities: HashMap<usize, Value> = HashMap::new();
    let mut interiors = Vec::new();
    for x in &mlos {
        let base = |i: usize| json!({"index": i, "interior": hex(x.name), "room": null, "entity_set": null});
        for &i in &x.entities {
            entities.entry(i).or_insert_with(|| base(i));
        }
        for (rn, es) in &x.rooms {
            for &i in es {
                let v = entities.entry(i).or_insert_with(|| base(i));
                if v["room"].is_null() {
                    v["room"] = json!(hex(*rn));
                    if let Some(n) = name(*rn) {
                        v["room_name"] = json!(n);
                    }
                }
            }
        }
        for (sn, es) in &x.sets {
            for &i in es {
                let v = entities.entry(i).or_insert_with(|| base(i));
                v["entity_set"] = json!(hex(*sn));
                if let Some(n) = name(*sn) {
                    v["entity_set_name"] = json!(n);
                }
            }
        }
        interiors.push(json!({
            "interior": hex(x.name),
            "bbox": x.bbox,
            "entities": x.entities.len(),
            "rooms": x.rooms.iter().map(|(n, es)| json!({"room": hex(*n), "name": name(*n), "entities": es.len()})).collect::<Vec<_>>(),
            "entity_sets": x.sets.iter().map(|(n, es)| json!({"entity_set": hex(*n), "name": name(*n), "entities": es.len()})).collect::<Vec<_>>(),
        }));
    }
    FileInfo { space, interiors, entities }
}

/// Scan every meta resource and write `<work>/index/interior_entities.jsonl`:
/// one line per file whose space is not `world`, {map, space, entities
/// (count), interiors, members: [{index, interior, room, entity_set, ...}]}.
/// Every entity of a non-world file is interior-local or group-local;
/// `members` lists the ones an interior names. Returns stats.
pub fn build(ws: &Workspace, a: &ArchiveInfo, metas: &[&Entry], progress: Progress) -> Result<Value> {
    let total = metas.len() as u64;
    let done = AtomicU64::new(0);
    crate::names::set_work_dir(&ws.work);
    let mut rows: Vec<(u64, Value, usize, usize, usize, usize, &'static str)> = metas
        .par_iter()
        .filter_map(|e| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&e.id, d, total);
            let r = Resource::load_sys(&a.path, e).ok()?;
            let m = Meta::parse(&r)?;
            let n = m.objects(meta7a::ENTITY).len();
            if n == 0 {
                return None;
            }
            let fi = file_info(&m);
            let mut mem: Vec<_> = fi.entities.into_values().collect();
            mem.sort_by_key(|v| v["index"].as_u64());
            let rooms = mem.iter().filter(|v| !v["room"].is_null()).count();
            let sets = mem.iter().filter(|v| !v["entity_set"].is_null()).count();
            let line = (fi.space != "world").then(|| json!({"map": e.id, "space": fi.space, "entities": n, "interiors": fi.interiors, "members": mem}));
            Some((e.off, line.unwrap_or(Value::Null), n, mem.len(), rooms, sets, fi.space))
        })
        .collect();
    rows.sort_by_key(|x| x.0);
    let dir = ws.index_dir();
    fs::create_dir_all(&dir)?;
    let mut w = crate::atomic::AtomicFile::create(dir.join("interior_entities.jsonl"))?;
    let mut by_space: HashMap<&str, (usize, usize)> = HashMap::new();
    let (mut mem, mut rooms, mut sets) = (0, 0, 0);
    for (_, v, n, m, r, s, sp) in &rows {
        let t = by_space.entry(sp).or_default();
        t.0 += 1;
        t.1 += n;
        mem += m;
        rooms += r;
        sets += s;
        if !v.is_null() {
            writeln!(w, "{v}")?;
        }
    }
    w.commit()?;
    Ok(json!({
        "files_by_space": by_space.iter().map(|(k, v)| (k.to_string(), json!({"files": v.0, "entities": v.1}))).collect::<serde_json::Map<_, _>>(),
        "interior_members": mem, "in_room": rooms, "in_entity_set": sets,
        "file": dir.join("interior_entities.jsonl"),
    }))
}
