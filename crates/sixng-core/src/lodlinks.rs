//! LOD distances and LOD parent links (`sixng lod-links`).
//!
//! Field names are case-sensitive joaat of the GTA V `CEntityDef` /
//! `CBaseArchetypeDef` member names (checked by hash):
//! * archetype `82d6fc83`: `lodDist` +0x38, `bsCentre` +0x58, `bsRadius`
//!   +0x64, `hdTextureDist` +0x70;
//! * entity `ce501483`: `lodLevel` `6c8f1715`, `parentIndex` `d89231bd`,
//!   `parentInstance` `e74c3c23`, `numChildren` `a687ac89`, `lodDist`
//!   `bf74dfa7`, `childLodDist` `ca974bcd`, `priorityLevel` `2691f019`.
//!
//! Parent rules (from the data):
//! * `parentInstance >= 0`: the parent is instance `parentInstance` of a
//!   prop batch (`ba049770`, a `*_batchlod` model) in another map file; the
//!   batch index is `parentIndex - <entity count of that file>` (a file's
//!   LOD slots are its entities followed by its prop batches). The file
//!   is not named anywhere we decode, so it is found by position: the
//!   instance sits exactly on the entity (median 0.0 m).
//! * `parentInstance == -1`, `parentIndex >= 0`: the parent is entity
//!   `parentIndex` of one map file. When the file's `parent_hash` names a
//!   file (2,089 files) that file is used. Otherwise the parent file is
//!   chosen per child file (all its children share it): candidate files in
//!   the same region (map-data field `6c0f9c8c`; SLOD1 -> SLOD2 links
//!   leave the region, so then any nearby file) whose entity at each
//!   child's index is one LOD level up with children; ranked by how many
//!   children lie inside the parent's bounding sphere, then child counts
//!   not exceeding `numChildren`, then the nearest following file in the
//!   archive. Checked against the files that do have a parent hash: 95 %
//!   pick the same file (the misses are state variants of one LOD at the
//!   same place).
//! * Time archetypes (`CTimeArchetypeDef` `76b0c56c`) are archetypes
//!   with the same base layout plus `timeFlags` `8609d52c`, whose low 24
//!   bits are the in-game hours the model is shown (bit h = hour h). Only
//!   a few dozen exist (shop open/shut variants, beach loungers); they are
//!   read with the plain archetypes and keep their mask.
//! * An entity's `lodDist` of -1 means "use the archetype's `lodDist`";
//!   explicit values equal the archetype value for most entities and
//!   override it for the rest.

use crate::export::{Progress, Workspace, ARCHETYPE_DEF};
use crate::game::ArchiveInfo;
use crate::meta7a::{field_name, Meta, ENTITY, MAPDATA};
use crate::resource::Resource;
use crate::scan::Entry;
use anyhow::Result;
use rayon::prelude::*;
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Archetype LOD data (one archetype definition).
#[derive(Clone, Copy, Debug)]
pub struct ArchLod {
    pub lod_dist: f32,
    pub hd_texture_dist: f32,
    pub bs_radius: f32,
    pub bs_centre: [f32; 3],
    /// `flags` (+0x68; +0x30 holds a copy). None when read from an
    /// archetype_lod.jsonl written before it was recorded.
    pub flags: Option<u32>,
    /// The 64-bit keys at +0x08, +0x10 and +0x28 (0 = none); one of them
    /// keys the archetype's entry in the tunable-object table of data.
    pub tuning_keys: [u64; 3],
    /// `timeFlags` of a time archetype (`CTimeArchetypeDef`); None for a
    /// plain archetype.
    pub time_flags: Option<u32>,
}

/// Struct hash of `CTimeArchetypeDef` (a `CBaseArchetypeDef` plus
/// `timeFlags`).
pub const TIME_ARCHETYPE_DEF: u32 = 0x76b0_c56c;
/// Field hash of `timeFlags`.
pub const TIME_FLAGS_FIELD: u32 = 0x8609_d52c;

/// In-game hours (0..=23) a `timeFlags` mask shows the model in.
pub fn time_hours(time_flags: u32) -> Vec<u8> {
    (0..24u8).filter(|h| time_flags >> h & 1 != 0).collect()
}

/// JSON for an archetype's time mask: `time_flags` (hex, low 24 bits) and
/// `hours`, or None for a plain archetype.
pub fn time_json(time_flags: Option<u32>) -> Option<serde_json::Value> {
    let t = time_flags?;
    Some(json!({"time_flags": format!("{:06x}", t & 0xFF_FFFF), "hours": time_hours(t)}))
}

pub struct Ent {
    pub level: i64,
    pub pi: i64,
    pub slot: i64,
    pub arch: u32,
    pub pos: [f32; 3],
    pub scale: f32,
    pub nc: i64,
}

pub struct FileRec {
    pub id: String,
    pub off: u64,
    pub name_hash: u32,
    pub parent_hash: u32,
    pub region: u64,
    pub ents: Vec<Ent>,
    /// Prop batches: (archetype, instance positions).
    pub props: Vec<(u32, Vec<[f32; 3]>)>,
    pub archs: Vec<(u32, ArchLod)>,
}

pub fn archetype_lod_path(ws: &Workspace) -> PathBuf {
    ws.index_dir().join("archetype_lod.jsonl")
}
pub fn parents_path(ws: &Workspace) -> PathBuf {
    ws.index_dir().join("lod_parents.jsonl")
}

pub fn read_file(a: &ArchiveInfo, e: &Entry) -> Option<FileRec> {
    let r = Resource::load_sys(&a.path, e).ok()?;
    let m = Meta::parse(&r)?;
    let ents_o = m.objects(ENTITY);
    let root = m.objects(MAPDATA);
    let arch_o = m.objects(ARCHETYPE_DEF);
    let time_o = m.objects(TIME_ARCHETYPE_DEF);
    let has_props = !m.objects(crate::batches::PROP_BATCH).is_empty();
    if ents_o.is_empty() && root.is_empty() && arch_o.is_empty() && time_o.is_empty() && !has_props {
        return None;
    }
    let (name_hash, parent_hash, region) = match root.first() {
        Some(&o) => {
            let f = m.scalars(MAPDATA, o, &[]);
            (r.u32(o + 8), r.u32(o + 0x68), f.get(&field_name(0x6c0f_9c8c)).and_then(|v| v.as_u64()).unwrap_or(0))
        }
        None => (0, 0, 0),
    };
    let ents = ents_o
        .iter()
        .map(|&o| {
            let f = m.scalars(ENTITY, o, &[8, 0x10, 0x20, 0x38, 0x44, 0x48, 0x54, 0x58]);
            let i = |h: u32| f.get(&field_name(h)).and_then(|v| v.as_i64()).unwrap_or(-1);
            Ent {
                level: i(0x6c8f_1715),
                pi: i(0xd892_31bd),
                slot: i(0xe74c_3c23),
                arch: r.u32(o + 0x44),
                pos: [r.f32(o + 0x38), r.f32(o + 0x3C), r.f32(o + 0x40)],
                scale: r.f32(o + 0x54).abs().max(r.f32(o + 0x58).abs()),
                nc: i(0xa687_ac89),
            }
        })
        .collect();
    let base = |o: usize, time_flags: Option<u32>| {
        (r.u32(o + 0x74), ArchLod { lod_dist: r.f32(o + 0x38), hd_texture_dist: r.f32(o + 0x70), bs_radius: r.f32(o + 0x64), bs_centre: [r.f32(o + 0x58), r.f32(o + 0x5C), r.f32(o + 0x60)], flags: Some(r.u32(o + 0x68)), tuning_keys: [r.u64(o + 0x08), r.u64(o + 0x10), r.u64(o + 0x28)], time_flags })
    };
    let archs = arch_o
        .iter()
        .map(|&o| base(o, None))
        .chain(time_o.iter().map(|&o| {
            let t = m.scalars_by_hash(TIME_ARCHETYPE_DEF, o, &[]).get(&TIME_FLAGS_FIELD).and_then(|v| v.as_u64()).map(|x| x as u32);
            base(o, Some(t.unwrap_or(0)))
        }))
        .collect();
    let mut props = Vec::new();
    if has_props {
        if let Ok(rm) = Resource::load_meta(&a.path, e) {
            if let Some(mm) = Meta::parse(&rm) {
                props = crate::batches::prop_batches(&rm, &mm).into_iter().map(|b| (b.archetype, b.instances.iter().map(|i| i.p).collect())).collect();
            }
        }
    }
    Some(FileRec { id: e.id.clone(), off: e.off, name_hash, parent_hash, region, ents, props, archs })
}

const CELL: f32 = 256.0;
fn cell(p: [f32; 3]) -> (i32, i32) {
    ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32)
}
fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
/// One LOD level up (HD -> LOD -> SLOD1 -> SLOD2 -> SLOD3 -> SLOD4).
fn up(level: i64) -> Option<i64> {
    match level {
        0 => Some(1),
        1 => Some(2),
        2 => Some(3),
        3 => Some(4),
        4 => Some(6),
        _ => None,
    }
}

/// Scan every map meta file, write `<work>/index/archetype_lod.jsonl` and
/// `<work>/index/lod_parents.jsonl`, return statistics.
pub fn build(ws: &Workspace, a: &ArchiveInfo, metas: &[&Entry], progress: Progress) -> Result<serde_json::Value> {
    let total = metas.len() as u64;
    let done = AtomicU64::new(0);
    let mut files: Vec<FileRec> = metas
        .par_iter()
        .filter_map(|e| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if d % 256 == 0 {
                progress(&e.id, d, total);
            }
            read_file(a, e)
        })
        .collect();
    files.sort_by_key(|f| f.off);
    let names = crate::export::model_names(ws);

    // ---- archetypes: most common lodDist per hash (ties: the larger)
    let mut arch_all: HashMap<u32, Vec<ArchLod>> = HashMap::new();
    for f in &files {
        for (h, l) in &f.archs {
            arch_all.entry(*h).or_default().push(*l);
        }
    }
    let mut arch: HashMap<u32, (ArchLod, usize, Vec<f32>)> = HashMap::new();
    for (h, v) in &arch_all {
        let mut c: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
        for (i, l) in v.iter().enumerate() {
            c.entry(l.lod_dist.to_bits()).or_insert((0, i)).0 += 1;
        }
        let best = c.iter().max_by(|x, y| x.1 .0.cmp(&y.1 .0).then(f32::from_bits(*x.0).total_cmp(&f32::from_bits(*y.0)))).map(|x| x.1 .1).unwrap_or(0);
        let vals: Vec<f32> = c.keys().map(|b| f32::from_bits(*b)).collect();
        let mut l = v[best];
        // a time archetype keeps its mask even if a plain definition of the same hash wins
        l.time_flags = l.time_flags.or_else(|| v.iter().find_map(|x| x.time_flags));
        arch.insert(*h, (l, v.len(), vals));
    }
    {
        let mut w = crate::atomic::AtomicFile::create(archetype_lod_path(ws))?;
        let mut keys: Vec<_> = arch.keys().copied().collect();
        keys.sort();
        for h in keys {
            let (l, n, vals) = &arch[&h];
            let nm = names.get(&h);
            let mut v = json!({"archetype": format!("{h:08x}"), "name": nm.map(|x| &x.0), "model_id": nm.map(|x| &x.1),
                "lod_dist": l.lod_dist, "hd_texture_dist": l.hd_texture_dist, "bs_radius": l.bs_radius, "bs_centre": l.bs_centre, "definitions": n,
                "flags": l.flags, "tuning_keys": l.tuning_keys.iter().map(|k| format!("{k:016x}")).collect::<Vec<_>>()});
            if vals.len() > 1 {
                v["lod_dist_values"] = json!(vals);
            }
            if let Some(t) = time_json(l.time_flags) {
                v["time"] = t;
            }
            writeln!(w, "{v}")?;
        }
        w.commit()?;
    }

    // ---- indexes over files
    let by_name: HashMap<u32, Vec<usize>> = files.iter().enumerate().filter(|(_, f)| f.name_hash != 0).fold(HashMap::new(), |mut m, (i, f)| {
        m.entry(f.name_hash).or_default().push(i);
        m
    });
    let mut by_region: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if !f.ents.is_empty() {
            by_region.entry(f.region).or_default().push(i);
        }
    }
    // (cell, LOD slot) -> (file, batch); every cell an instance of the batch touches
    let mut batch_cells: HashMap<((i32, i32), i64), Vec<(usize, usize)>> = HashMap::new();
    for (fi, f) in files.iter().enumerate() {
        for (j, (_, inst)) in f.props.iter().enumerate() {
            let slot = (f.ents.len() + j) as i64;
            let mut cs: Vec<(i32, i32)> = inst.iter().map(|p| cell(*p)).collect();
            cs.sort();
            cs.dedup();
            for c in cs {
                batch_cells.entry((c, slot)).or_default().push((fi, j));
            }
        }
    }
    // Batches whose instance array does not decode (about 3 %: garbage
    // positions, a known decoder gap): (LOD slot, base model name) -> (file,
    // batch), for a name-based fallback.
    let base = |h: u32| -> Option<String> {
        let n = names.get(&h)?.0.to_ascii_lowercase();
        for suf in ["_batchlod", "_batch_lod", "_batch_slod", "_batchslod", "_batch"] {
            if let Some(x) = n.strip_suffix(suf) {
                return Some(x.to_string());
            }
        }
        Some(n)
    };
    let bad_pos = |p: &[f32; 3]| p.iter().any(|v| !v.is_finite() || v.abs() > 20000.0 || (*v != 0.0 && v.abs() < 1e-6));
    let mut bad_batches: HashMap<(i64, String), Vec<(usize, usize)>> = HashMap::new();
    for (fi, f) in files.iter().enumerate() {
        for (j, (ba, inst)) in f.props.iter().enumerate() {
            if inst.iter().any(bad_pos) {
                if let Some(n) = base(*ba) {
                    bad_batches.entry(((f.ents.len() + j) as i64, n)).or_default().push((fi, j));
                }
            }
        }
    }
    // files with entities by cell (for the out-of-region fallback)
    let mut ent_cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (fi, f) in files.iter().enumerate() {
        let mut cs: Vec<(i32, i32)> = f.ents.iter().map(|e| cell(e.pos)).collect();
        cs.sort();
        cs.dedup();
        for c in cs {
            ent_cells.entry(c).or_default().push(fi);
        }
    }

    let model_of = |h: u32| names.get(&h).map(|x| x.1.clone());
    let name_of = |h: u32| names.get(&h).map(|x| x.0.clone());
    let inside = |p: &Ent, c: &Ent| -> bool {
        let (r, cen) = arch.get(&p.arch).map(|x| (x.0.bs_radius, (x.0.bs_centre[0].powi(2) + x.0.bs_centre[1].powi(2) + x.0.bs_centre[2].powi(2)).sqrt())).unwrap_or((0.0, 0.0));
        dist(p.pos, c.pos) <= (r + cen) * p.scale.max(1.0) + 2.0
    };

    // ---- resolve per child file
    #[derive(Default)]
    struct St {
        batch_ok: usize,
        batch_ambiguous: usize,
        batch_named: usize,
        batch_none: usize,
        ent_hash: usize,
        ent_region: usize,
        ent_spatial: usize,
        ent_none: usize,
        high: usize,
        medium: usize,
        low: usize,
    }
    let results: Vec<(String, Vec<serde_json::Value>, St)> = files
        .par_iter()
        .enumerate()
        .filter_map(|(fi, f)| {
            let mut st = St::default();
            let mut out: Vec<serde_json::Value> = Vec::new();
            // batch parents
            for (i, e) in f.ents.iter().enumerate() {
                if e.slot < 0 || e.pi < 0 {
                    continue;
                }
                let mut hits: Vec<(f32, usize, usize)> = Vec::new();
                if let Some(v) = batch_cells.get(&(cell(e.pos), e.pi)) {
                    for &(bf, j) in v {
                        if bf == fi {
                            continue;
                        }
                        let inst = &files[bf].props[j].1;
                        if let Some(p) = inst.get(e.slot as usize) {
                            let d = dist(*p, e.pos);
                            if d < 0.5 {
                                hits.push((d, bf, j));
                            }
                        }
                    }
                }
                // same region first, then nearest, then archive order
                hits.sort_by(|x, y| ((files[x.1].region != f.region) as u8).cmp(&((files[y.1].region != f.region) as u8)).then(x.0.total_cmp(&y.0)).then(files[x.1].off.cmp(&files[y.1].off)));
                match hits.first() {
                    Some(&(d, bf, j)) => {
                        let amb = hits.len() > 1;
                        if amb {
                            st.batch_ambiguous += 1;
                        } else {
                            st.batch_ok += 1;
                        }
                        let ba = files[bf].props[j].0;
                        out.push(json!({"index": i, "kind": "batch", "map": files[bf].id, "batch": j, "instance": e.slot,
                            "archetype": format!("{ba:08x}"), "name": name_of(ba), "model_id": model_of(ba), "distance": d,
                            "method": "batch_position", "confidence": if amb { "medium" } else { "high" }, "alternatives": hits.len() - 1}));
                    }
                    None => {
                        // name fallback for batches that do not decode
                        let mut c: Vec<(usize, usize)> = names
                            .get(&e.arch)
                            .map(|x| x.0.to_ascii_lowercase())
                            .and_then(|n| bad_batches.get(&(e.pi, n)))
                            .map(|v| v.iter().copied().filter(|&(bf, j)| bf != fi && (e.slot as usize) < files[bf].props[j].1.len()).collect())
                            .unwrap_or_default();
                        c.sort_by_key(|&(bf, _)| ((files[bf].region != f.region) as u8, files[bf].off.abs_diff(f.off)));
                        match c.first() {
                            Some(&(bf, j)) => {
                                let same = c.iter().filter(|x| files[x.0].region == f.region).count();
                                let conf = if same == 1 { "medium" } else { "low" };
                                st.batch_named += 1;
                                let ba = files[bf].props[j].0;
                                out.push(json!({"index": i, "kind": "batch", "map": files[bf].id, "batch": j, "instance": e.slot,
                                    "archetype": format!("{ba:08x}"), "name": name_of(ba), "model_id": model_of(ba), "distance": null,
                                    "method": "batch_index_name", "confidence": conf, "alternatives": c.len() - 1}));
                            }
                            None => st.batch_none += 1,
                        }
                    }
                }
            }
            // entity parents
            let kids: Vec<(usize, &Ent)> = f.ents.iter().enumerate().filter(|(_, e)| e.slot < 0 && e.pi >= 0).collect();
            if !kids.is_empty() {
                let score = |pf: usize| -> Option<(usize, usize, usize)> {
                    let pe = &files[pf].ents;
                    let (mut ins, mut lvl) = (0, 0);
                    let mut cnt: HashMap<i64, i64> = HashMap::new();
                    for (_, e) in &kids {
                        let p = pe.get(e.pi as usize)?;
                        if Some(p.level) != up(e.level) || p.nc <= 0 {
                            continue;
                        }
                        lvl += 1;
                        *cnt.entry(e.pi).or_default() += 1;
                        if inside(p, e) {
                            ins += 1;
                        }
                    }
                    if lvl == 0 {
                        return None;
                    }
                    let over = cnt.iter().filter(|(k, n)| **n > pe[**k as usize].nc).count();
                    Some((ins, over, lvl))
                };
                let rank = |cands: &[usize]| -> Vec<(usize, (usize, usize, usize))> {
                    let mut v: Vec<(usize, (usize, usize, usize))> = cands.iter().filter(|&&p| p != fi).filter_map(|&p| score(p).map(|s| (p, s))).collect();
                    let order = |p: usize| if files[p].off > f.off { files[p].off - f.off } else { u64::MAX / 2 + (f.off - files[p].off) };
                    v.sort_by(|x, y| y.1 .0.cmp(&x.1 .0).then(x.1 .1.cmp(&y.1 .1)).then(y.1 .2.cmp(&x.1 .2)).then(order(x.0).cmp(&order(y.0))));
                    v
                };
                let mut method = "parent_hash";
                let mut ranked = if f.parent_hash != 0 { rank(by_name.get(&f.parent_hash).map(|v| v.as_slice()).unwrap_or(&[])) } else { Vec::new() };
                if ranked.is_empty() {
                    method = "region";
                    ranked = rank(by_region.get(&f.region).map(|v| v.as_slice()).unwrap_or(&[]));
                }
                if ranked.is_empty() {
                    method = "spatial";
                    let mut c: Vec<usize> = kids.iter().flat_map(|(_, e)| ent_cells.get(&cell(e.pos)).cloned().unwrap_or_default()).collect();
                    c.sort();
                    c.dedup();
                    ranked = rank(&c);
                }
                match ranked.first() {
                    None => st.ent_none += kids.len(),
                    Some(&(pf, s)) => {
                        let tie = ranked.get(1).map_or(false, |x| x.1 == s);
                        let all_in = s.0 == kids.len();
                        let conf = if method == "parent_hash" || (all_in && !tie) {
                            "high"
                        } else if all_in || s.0 * 2 >= kids.len() {
                            "medium"
                        } else {
                            "low"
                        };
                        let pe = &files[pf].ents;
                        for (i, e) in &kids {
                            let Some(p) = pe.get(e.pi as usize) else { continue };
                            match method {
                                "parent_hash" => st.ent_hash += 1,
                                "region" => st.ent_region += 1,
                                _ => st.ent_spatial += 1,
                            }
                            match conf {
                                "high" => st.high += 1,
                                "medium" => st.medium += 1,
                                _ => st.low += 1,
                            }
                            out.push(json!({"index": i, "kind": "entity", "map": files[pf].id, "parent_index": e.pi,
                                "archetype": format!("{:08x}", p.arch), "name": name_of(p.arch), "model_id": model_of(p.arch),
                                "level_ok": Some(p.level) == up(e.level), "inside": inside(p, e), "distance": dist(p.pos, e.pos),
                                "method": method, "confidence": conf, "alternatives": ranked.len() - 1}));
                        }
                    }
                }
            }
            if out.is_empty() && st.batch_none == 0 && st.ent_none == 0 {
                return None;
            }
            Some((f.id.clone(), out, st))
        })
        .collect();
    let mut w = crate::atomic::AtomicFile::create(parents_path(ws))?;
    let mut t = St::default();
    for (id, v, s) in &results {
        if !v.is_empty() {
            writeln!(w, "{}", json!({"map": id, "parents": v}))?;
        }
        t.batch_ok += s.batch_ok;
        t.batch_ambiguous += s.batch_ambiguous;
        t.batch_none += s.batch_none;
        t.batch_named += s.batch_named;
        t.ent_hash += s.ent_hash;
        t.ent_region += s.ent_region;
        t.ent_spatial += s.ent_spatial;
        t.ent_none += s.ent_none;
        t.high += s.high;
        t.medium += s.medium;
        t.low += s.low;
    }
    w.commit()?;
    let n_ent: usize = files.iter().map(|f| f.ents.len()).sum();
    Ok(json!({"map_files": files.len(), "entities": n_ent, "archetypes": arch.len(),
        "archetypes_with_differing_lod_dist": arch.values().filter(|x| x.2.len() > 1).count(),
        "batch_parents": {"resolved": t.batch_ok, "ambiguous_resolved": t.batch_ambiguous, "by_index_and_name": t.batch_named, "unresolved": t.batch_none},
        "entity_parents": {"by_parent_hash": t.ent_hash, "by_region": t.ent_region, "by_position": t.ent_spatial, "unresolved": t.ent_none,
            "confidence": {"high": t.high, "medium": t.medium, "low": t.low}},
        "out": [archetype_lod_path(ws), parents_path(ws)]}))
}

/// Archetype LOD table from `<work>/index/archetype_lod.jsonl` (empty if
/// `sixng lod-links` has not been run).
pub fn load_archetype_lod(ws: &Workspace) -> HashMap<u32, ArchLod> {
    load_archetype_lod_from(&ws.index_dir())
}

/// Same, from an index directory.
pub fn load_archetype_lod_from(index_dir: &std::path::Path) -> HashMap<u32, ArchLod> {
    let mut m = HashMap::new();
    let Ok(s) = fs::read_to_string(index_dir.join("archetype_lod.jsonl")) else { return m };
    for l in s.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(l) else { continue };
        let Some(h) = v["archetype"].as_str().and_then(|x| u32::from_str_radix(x, 16).ok()) else { continue };
        let f = |k: &str| v[k].as_f64().unwrap_or(-1.0) as f32;
        let c = |i: usize| v["bs_centre"][i].as_f64().unwrap_or(0.0) as f32;
        let k = |i: usize| v["tuning_keys"][i].as_str().and_then(|x| u64::from_str_radix(x, 16).ok()).unwrap_or(0);
        m.insert(h, ArchLod { lod_dist: f("lod_dist"), hd_texture_dist: f("hd_texture_dist"), bs_radius: f("bs_radius"), bs_centre: [c(0), c(1), c(2)], flags: v["flags"].as_u64().map(|x| x as u32), tuning_keys: [k(0), k(1), k(2)], time_flags: v["time"]["time_flags"].as_str().and_then(|x| u32::from_str_radix(x, 16).ok()) });
    }
    m
}

/// Resolved parents from `<work>/index/lod_parents.jsonl`: map id ->
/// entity index -> parent object (empty if not built).
pub fn load_parents(ws: &Workspace) -> HashMap<String, HashMap<u64, serde_json::Value>> {
    load_parents_from(&ws.index_dir())
}

/// Same, from an index directory.
pub fn load_parents_from(index_dir: &std::path::Path) -> HashMap<String, HashMap<u64, serde_json::Value>> {
    let mut m: HashMap<String, HashMap<u64, serde_json::Value>> = HashMap::new();
    let Ok(f) = fs::File::open(index_dir.join("lod_parents.jsonl")) else { return m };
    use std::io::BufRead;
    for l in std::io::BufReader::new(f).lines().map_while(|l| l.ok()) {
        let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&l) else { continue };
        let Some(id) = v["map"].as_str().map(|s| s.to_string()) else { continue };
        let e = m.entry(id).or_default();
        if let Some(arr) = v["parents"].as_array_mut() {
            for p in arr.iter_mut() {
                if let Some(i) = p["index"].as_u64() {
                    if let Some(o) = p.as_object_mut() {
                        o.remove("index");
                    }
                    e.insert(i, p.take());
                }
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_hours_from_mask() {
        // beach loungers: 07:00 to 20:59
        assert_eq!(time_hours(0x1f_ff80), (7..=20).collect::<Vec<u8>>());
        // shut variant: wraps past midnight
        assert_eq!(time_hours(0xf8_007f), vec![0, 1, 2, 3, 4, 5, 6, 19, 20, 21, 22, 23]);
        // bits above 23 are not hours
        assert_eq!(time_hours(0xff00_00e0), vec![5, 6, 7]);
        assert!(time_hours(0).is_empty());
        assert_eq!(time_json(None), None);
        assert_eq!(time_json(Some(0xe0)).unwrap(), json!({"time_flags": "0000e0", "hours": [5, 6, 7]}));
    }

    #[test]
    fn archetype_lod_reads_time_flags() {
        let d = std::env::temp_dir().join(format!("sixng_lodlinks_time_{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("archetype_lod.jsonl"),
            "{\"archetype\":\"03d83f1e\",\"lod_dist\":60.0,\"flags\":128,\"time\":{\"time_flags\":\"1fff80\",\"hours\":[7]}}
{\"archetype\":\"00000001\",\"lod_dist\":10.0}
",
        )
        .unwrap();
        let m = load_archetype_lod_from(&d);
        let _ = fs::remove_dir_all(&d);
        assert_eq!(m[&0x03d8_3f1e].time_flags, Some(0x1f_ff80));
        assert_eq!(m[&0x03d8_3f1e].lod_dist, 60.0);
        assert_eq!(m[&1].time_flags, None);
    }
}
