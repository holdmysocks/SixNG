//! Head identity deltas: unnamed `packs.rpf` resources that move every
//! vertex of a class's base head (`head_<class>_base_000_<x>_full`) to one
//! head: `head_vertex = base_vertex + delta`.
//!
//! System page (240 bytes): ptr 0x10 = buffer descriptor at 0x70 (flags
//! 0x1601 at +0x10, graphics pointer at +0x18); u32 0x58 = joaat of the base
//! head (LOD0; the LOD1-3 deltas carry other, unresolved hashes). Graphics
//! buffer: u32 1, f32x3 base at 0x04, f32x3 step at 0x10, then V x u64 at
//! 0x20 quantised like mesh-blob positions (x, y 24-bit, z 16-bit). Model
//! space, Z-up metres.
//!
//! The deltas are in one fixed vertex order per base mesh that is not the
//! stored order of the base head. The order is recovered from heads that
//! exist as drawables (their offset from the base, matched by UV and
//! position) and then applies to every delta of that base mesh, including
//! the heads that have no drawable. Deltas run in alphabetical order of the
//! heads, so the matched neighbours bracket the name of an unmatched one.
use crate::resource::Resource;
use crate::scan::Entry;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;

pub struct Delta {
    pub off: u64,
    pub base_hash: u32,
    /// Vertex slots in the buffer (may include padding).
    pub capacity: usize,
}

/// Cheap test on the system page alone.
pub fn is_delta(r: &Resource) -> bool {
    r.sys.len() >= 0x90 && r.sys.len() <= 0x200 && r.u64(0x10) == 0x5000_0070 && r.u32(0x80) == 0x1601 && r.u64(0x88) == 0x6000_0000 && r.u32(0x58) != 0
}

/// Decode the first `v` deltas (RAGE space).
pub fn decode(r: &Resource, v: usize) -> Result<Vec<[f32; 3]>> {
    let g = &r.gfx;
    if g.len() < 0x20 {
        bail!("graphics buffer too short");
    }
    let f = |o: usize| f32::from_le_bytes(g[o..o + 4].try_into().unwrap());
    let (b, s) = ([f(4), f(8), f(12)], [f(0x10), f(0x14), f(0x18)]);
    let n = ((g.len() - 0x20) / 8).min(v);
    if n < v {
        bail!("{n} deltas stored, {v} needed");
    }
    Ok((0..n)
        .map(|i| {
            let q = u64::from_le_bytes(g[0x20 + 8 * i..0x28 + 8 * i].try_into().unwrap());
            [((((q >> 16) & 0xffff) << 8) | (q & 0xff)) as f32 * s[0] + b[0], ((((q >> 32) & 0xffff) << 8) | ((q >> 8) & 0xff)) as f32 * s[1] + b[1], (q >> 48) as f32 * s[2] + b[2]]
        })
        .collect())
}

/// Triangle meshes of a drawable resource: (positions, UV0), one per geometry.
fn meshes(r: &Resource) -> Vec<(Vec<[f32; 3]>, Vec<[f32; 2]>)> {
    let bufs = crate::drawable::find_buffers(r);
    let geoms = crate::drawable::find_geometries(r, &bufs);
    let slice = |i: usize| {
        let b = &bufs[i];
        let end = (b.gfx_off + b.bytes).min(r.gfx.len() as u64) as usize;
        &r.gfx[(b.gfx_off as usize).min(end)..end]
    };
    geoms
        .iter()
        .filter_map(|g| {
            let h = bufs[g.vertex_buffer].mesh_blob.as_ref()?;
            let m = crate::drawable::decode_mesh(slice(g.vertex_buffer), h, slice(g.index_buffer));
            let uv = m.uvs.iter().find(|(slot, _)| *slot == 3).map(|(_, u)| u.clone()).unwrap_or_default();
            (uv.len() == m.positions.len()).then_some((m.positions, uv))
        })
        .collect()
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Offset of each base vertex in a head with the same topology: vertices
/// matched by UV0, the nearest one when several share it.
fn offsets(base: &(Vec<[f32; 3]>, Vec<[f32; 2]>), head: &(Vec<[f32; 3]>, Vec<[f32; 2]>)) -> Vec<Option<[f32; 3]>> {
    let key = |u: [f32; 2]| ((u[0] * 1e5).round() as i64, (u[1] * 1e5).round() as i64);
    let mut by_uv: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, &u) in head.1.iter().enumerate() {
        by_uv.entry(key(u)).or_default().push(i);
    }
    base.0
        .iter()
        .zip(&base.1)
        .map(|(&p, &u)| {
            let c = by_uv.get(&key(u))?;
            let &i = c.iter().min_by(|&&a, &&b| dist2(head.0[a], p).total_cmp(&dist2(head.0[b], p)))?;
            let q = head.0[i];
            let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
            (d.iter().all(|v| v.abs() < 0.05)).then_some(d)
        })
        .collect()
}

#[derive(serde::Serialize, Clone)]
pub struct DeltaRow {
    pub id: String,
    pub class: String,
    pub lod: usize,
    pub vertices: usize,
    pub base: String,
    /// Head drawable this delta belongs to (None: the head has no drawable).
    pub head: Option<String>,
    /// Median / max |head - (base + delta)| over the vertices, micrometres.
    pub error_um: Option<[f32; 2]>,
    /// Nearest matched heads before and after it in archive (alphabetical) order.
    pub between: [Option<String>; 2],
    /// Head name from the archive order when the drawable did not match
    /// (many head drawables are copies of one placeholder mesh; the delta
    /// holds the real shape): the unmatched names between two matched heads,
    /// assigned in order when their count equals the number of deltas there.
    /// LOD0 deltas take the name of the LOD1 delta that follows them.
    pub head_by_order: Option<String>,
    pub file: Option<String>,
}

struct ClassLod {
    class: String,
    lod: usize,
    base_entry: Entry,
    base_name: String,
    base: (Vec<[f32; 3]>, Vec<[f32; 2]>),
}

/// Match every delta to its head drawable, recover the vertex order per base
/// mesh, and (with `build`) write a GLB for each LOD0 delta whose head has no
/// drawable: the exported base head with its positions moved.
/// Writes `<out>/head_deltas/<archive>/index.jsonl`; the GLBs go to
/// `<out>/models/<archive>/head_<class>_delta_<offset>/model.glb` next to the
/// base head's textures. Returns the rows.
pub fn run(a: &crate::game::ArchiveInfo, all: &[Entry], out: &std::path::Path, build: bool, only: &[u64], progress: crate::export::Progress) -> Result<Vec<DeltaRow>> {
    use rayon::prelude::*;
    let name_re = regex::Regex::new(r"^head_([a-z]+\d)_base_000_([a-z])_full$").unwrap();
    let head_re = regex::Regex::new(r"^head_([a-z]+\d)_([a-z]+)_(\d+)_([a-z])_full$").unwrap();
    // base heads by LOD0 hash
    let mut bases: HashMap<u32, (String, String)> = HashMap::new();
    for e in all {
        if let Some(n) = e.name.as_deref() {
            if let Some(c) = name_re.captures(n) {
                bases.insert(crate::meta7a::joaat(n), (c[1].to_string(), n.to_string()));
            }
        }
    }
    // the deltas
    let cands: Vec<&Entry> = all.iter().filter(|e| e.name.is_none() && e.kind == crate::classify::Kind::RageObj && e.usize < 0x40000).collect();
    let mut deltas: Vec<Delta> = cands
        .par_iter()
        .filter_map(|e| {
            let r = Resource::load_sys(&a.path, e).ok()?;
            is_delta(&r).then(|| Delta { off: e.off, base_hash: r.u32(0x58), capacity: ((r.u32(0x78) as usize * 4).saturating_sub(0x20)) / 8 })
        })
        .collect();
    deltas.sort_by_key(|d| d.off);
    progress("deltas listed", 1, 4);
    // class of the LOD1-3 hashes: the class of the LOD0 delta before them
    let mut votes: HashMap<u32, HashMap<String, usize>> = HashMap::new();
    let mut last: Option<String> = None;
    for d in &deltas {
        match bases.get(&d.base_hash) {
            Some((c, _)) => last = Some(c.clone()),
            None => {
                if let Some(c) = &last {
                    *votes.entry(d.base_hash).or_default().entry(c.clone()).or_default() += 1;
                }
            }
        }
    }
    let class_of = |h: u32| -> Option<String> { bases.get(&h).map(|b| b.0.clone()).or_else(|| votes.get(&h).and_then(|v| v.iter().max_by_key(|x| x.1).map(|x| x.0.clone()))) };
    // base meshes per class: every geometry of the base head's resources
    let by_name: HashMap<&str, Vec<&Entry>> = all.iter().filter_map(|e| e.name.as_deref().map(|n| (n, e))).fold(HashMap::new(), |mut m, (n, e)| {
        m.entry(n).or_default().push(e);
        m
    });
    let mut base_meshes: HashMap<String, Vec<(Entry, String, (Vec<[f32; 3]>, Vec<[f32; 2]>))>> = HashMap::new();
    for (c, n) in bases.values() {
        for e in by_name.get(n.as_str()).into_iter().flatten() {
            let Ok(r) = Resource::load(&a.path, e) else { continue };
            for m in meshes(&r) {
                base_meshes.entry(c.clone()).or_default().push(((*e).clone(), n.clone(), m));
            }
        }
    }
    for v in base_meshes.values_mut() {
        v.sort_by_key(|m| std::cmp::Reverse(m.2 .0.len()));
    }
    // (class, vertex count) of each delta: the largest base mesh that fits
    let mut groups: HashMap<(String, usize), Vec<usize>> = HashMap::new();
    for (i, d) in deltas.iter().enumerate() {
        let Some(c) = class_of(d.base_hash) else { continue };
        let Some(ms) = base_meshes.get(&c) else { continue };
        if let Some(m) = ms.iter().find(|m| m.2 .0.len() <= d.capacity && d.capacity - m.2 .0.len() < 16) {
            groups.entry((c, m.2 .0.len())).or_default().push(i);
        }
    }
    // head drawables per class with their meshes
    let mut heads_of: HashMap<String, Vec<String>> = HashMap::new();
    for n in by_name.keys() {
        if let Some(c) = head_re.captures(n) {
            if &c[2] != "base" {
                heads_of.entry(c[1].to_string()).or_default().push(n.to_string());
            }
        }
    }
    let by_off: HashMap<u64, &Entry> = all.iter().map(|e| (e.off, e)).collect();
    let mut rows: HashMap<u64, DeltaRow> = HashMap::new();
    let mut keys: Vec<(String, usize)> = groups.keys().cloned().collect();
    keys.sort();
    let mut perms: HashMap<(String, usize), Vec<usize>> = HashMap::new();
    for (gi, key) in keys.iter().enumerate() {
        progress(&format!("{} {} vertices", key.0, key.1), gi as u64 + 1, keys.len() as u64);
        let (class, v) = key.clone();
        let idx = &groups[key];
        let bm = base_meshes[&class].iter().find(|m| m.2 .0.len() == v).unwrap();
        let lod = base_meshes[&class].iter().position(|m| m.2 .0.len() == v).unwrap_or(0);
        let cl = ClassLod { class: class.clone(), lod, base_entry: bm.0.clone(), base_name: bm.1.clone(), base: bm.2.clone() };
        // decode this group's deltas
        let dec: Vec<Option<Vec<[f32; 3]>>> = idx.par_iter().map(|&i| Resource::load(&a.path, by_off[&deltas[i].off]).ok().and_then(|r| decode(&r, v).ok())).collect();
        // head offsets from the base, per head drawable with this topology
        let mut names = heads_of.get(&class).cloned().unwrap_or_default();
        names.sort();
        let head_offsets: Vec<(String, Vec<Option<[f32; 3]>>)> = names
            .par_iter()
            .filter_map(|n| {
                for e in by_name.get(n.as_str())? {
                    let Ok(r) = Resource::load(&a.path, e) else { continue };
                    if let Some(m) = meshes(&r).into_iter().find(|m| m.0.len() == v) {
                        return Some((n.clone(), offsets(&cl.base, &m)));
                    }
                }
                None
            })
            .collect();
        // reference heads (complete offsets), matched to deltas by multiset
        let q = |d: [f32; 3]| [(d[0] * 5e4).round() as i32, (d[1] * 5e4).round() as i32, (d[2] * 5e4).round() as i32];
        let mut refs: Vec<(usize, usize)> = Vec::new(); // (head index, delta index in group)
        for (hi, (_, off)) in head_offsets.iter().enumerate() {
            if refs.len() >= 3 {
                break;
            }
            if off.iter().any(|o| o.is_none()) {
                continue;
            }
            let mut want: HashMap<[i32; 3], i32> = HashMap::new();
            for o in off.iter().flatten() {
                *want.entry(q(*o)).or_default() += 1;
            }
            let best = dec
                .par_iter()
                .enumerate()
                .filter_map(|(k, d)| {
                    let d = d.as_ref()?;
                    let mut w = want.clone();
                    let mut hit = 0;
                    for x in d {
                        if let Some(c) = w.get_mut(&q(*x)) {
                            if *c > 0 {
                                *c -= 1;
                                hit += 1;
                            }
                        }
                    }
                    Some((hit, k))
                })
                .max();
            if let Some((hit, k)) = best {
                if hit * 10 >= v * 6 && !refs.iter().any(|r| r.1 == k) {
                    refs.push((hi, k));
                }
            }
        }
        if refs.is_empty() {
            for &i in idx {
                rows.insert(deltas[i].off, row(&deltas[i], all_id(a, deltas[i].off), &cl, v, None, None));
            }
            continue;
        }
        // vertex order: for each base vertex the delta slot whose values over
        // the reference heads are closest
        let perm: Vec<usize> = (0..v)
            .into_par_iter()
            .map(|j| {
                let mut best = (f32::MAX, 0);
                for s in 0..v {
                    let mut e = 0.0;
                    for &(hi, k) in &refs {
                        e += dist2(head_offsets[hi].1[j].unwrap(), dec[k].as_ref().unwrap()[s]);
                    }
                    if e < best.0 {
                        best = (e, s);
                    }
                }
                best.1
            })
            .collect();
        // every head drawable: its delta by a sample of the most varying vertices
        let sample: Vec<usize> = {
            let mut var: Vec<(f32, usize)> = (0..v).map(|j| (refs.iter().map(|&(_, k)| { let d = dec[k].as_ref().unwrap()[perm[j]]; d[0].abs() + d[1].abs() + d[2].abs() }).sum::<f32>(), j)).collect();
            var.sort_by(|a, b| b.0.total_cmp(&a.0));
            var.iter().take(64).map(|x| x.1).collect()
        };
        // candidates: every delta that reproduces the head (several heads
        // share one shape, with identical deltas)
        let cands: Vec<Vec<usize>> = head_offsets
            .par_iter()
            .map(|(_, off)| {
                let pts: Vec<usize> = sample.iter().copied().filter(|&j| off[j].is_some()).collect();
                if pts.len() < 16 {
                    return Vec::new();
                }
                dec.iter()
                    .enumerate()
                    .filter_map(|(k, d)| {
                        let d = d.as_ref()?;
                        let e = pts.iter().map(|&j| dist2(off[j].unwrap(), d[perm[j]])).sum::<f32>() / pts.len() as f32;
                        (e.sqrt() <= 5e-5).then_some(k)
                    })
                    .collect()
            })
            .collect();
        // heads in name order take the first free candidate in archive order
        // (both run alphabetically, so equal shapes pair up in order)
        let mut head_of: HashMap<usize, (String, [f32; 2])> = HashMap::new();
        for (hi, c) in cands.iter().enumerate() {
            let Some(&k) = c.iter().find(|k| !head_of.contains_key(k)) else { continue };
            let (off, d) = (&head_offsets[hi].1, dec[k].as_ref().unwrap());
            let mut errs: Vec<f32> = (0..v).filter_map(|j| off[j].map(|o| dist2(o, d[perm[j]]).sqrt() * 1e6)).collect();
            errs.sort_by(f32::total_cmp);
            head_of.insert(k, (head_offsets[hi].0.clone(), [errs[errs.len() / 2], *errs.last().unwrap()]));
        }
        for (k, &i) in idx.iter().enumerate() {
            let h = head_of.get(&k);
            rows.insert(deltas[i].off, row(&deltas[i], all_id(a, deltas[i].off), &cl, v, h.map(|h| h.0.clone()), h.map(|h| h.1)));
        }
        perms.insert(key.clone(), perm);
        let _ = &cl.base_entry;
    }
    // neighbours in archive order within one class and LOD
    let mut v: Vec<DeltaRow> = deltas.iter().filter_map(|d| rows.remove(&d.off)).collect();
    let mut last_head: HashMap<(String, usize), String> = HashMap::new();
    for r in v.iter_mut() {
        let k = (r.class.clone(), r.vertices);
        match &r.head {
            Some(h) => {
                last_head.insert(k, h.clone());
            }
            None => r.between[0] = last_head.get(&k).cloned(),
        }
    }
    let mut next_head: HashMap<(String, usize), String> = HashMap::new();
    for r in v.iter_mut().rev() {
        let k = (r.class.clone(), r.vertices);
        match &r.head {
            Some(h) => {
                next_head.insert(k, h.clone());
            }
            None => r.between[1] = next_head.get(&k).cloned(),
        }
    }
    // names from the order: per class and LOD (1-3), gaps between matched heads
    let mut sorted_heads: HashMap<String, Vec<String>> = heads_of.clone();
    for n in sorted_heads.values_mut() {
        n.sort();
    }
    let mut seq: HashMap<(String, usize), Vec<usize>> = HashMap::new();
    for (i, r) in v.iter().enumerate() {
        seq.entry((r.class.clone(), r.lod)).or_default().push(i);
    }
    for ((class, lod), ix) in &seq {
        if *lod == 0 {
            continue;
        }
        let names = sorted_heads.get(class).cloned().unwrap_or_default();
        let matched: std::collections::HashSet<String> = ix.iter().filter_map(|&i| v[i].head.clone()).collect();
        let mut k = 0;
        while k < ix.len() {
            if v[ix[k]].head.is_some() {
                k += 1;
                continue;
            }
            let start = k;
            while k < ix.len() && v[ix[k]].head.is_none() {
                k += 1;
            }
            let prev = if start > 0 { v[ix[start - 1]].head.clone() } else { None };
            let next = if k < ix.len() { v[ix[k]].head.clone() } else { None };
            let gap: Vec<&String> = names.iter().filter(|n| !matched.contains(*n) && prev.as_ref().is_none_or(|p| *n > p) && next.as_ref().is_none_or(|x| *n < x)).collect();
            if gap.len() == k - start {
                for (t, n) in gap.into_iter().enumerate() {
                    v[ix[start + t]].head_by_order = Some(n.clone());
                }
            }
        }
    }
    // LOD0: the name of the LOD1 delta right after it
    for i in 0..v.len().saturating_sub(1) {
        if v[i].lod == 0 && v[i].head.is_none() && v[i + 1].lod == 1 && v[i + 1].class == v[i].class {
            v[i].head_by_order = v[i + 1].head.clone().or_else(|| v[i + 1].head_by_order.clone());
            v[i].between = v[i + 1].between.clone();
        }
    }
    progress("matched", 3, 4);
    if build {
        let todo: Vec<usize> = (0..v.len()).filter(|&i| v[i].head.is_none() && v[i].lod == 0 && perms.contains_key(&(v[i].class.clone(), v[i].vertices)) && (only.is_empty() || only.contains(&off_of(&v[i].id)))).collect();
        let eo = crate::export::ExportOptions { out: out.to_path_buf(), png: true, dds: false, raw: false, force: false, skip_embedded_textures: false, body_shape: Vec::new(), head_skin: 0, paint: None, tex_remap: Default::default(), ped_tints: Default::default() };
        let glb_of = |e: &Entry, n: usize| -> Result<std::path::PathBuf> {
            crate::export::export_entry(a, e, &eo)?;
            // (an earlier export is kept and not listed again: look in its folder)
            let dir = out.join("models").join(crate::game::index_stem(&a.id)).join(crate::export::export_base_name(e));
            std::fs::read_dir(&dir)?.filter_map(|d| d.ok().map(|d| d.path())).filter(|f| f.extension().is_some_and(|x| x == "glb")).find(|f| glb_vertex_count(f).ok() == Some(n)).ok_or_else(|| anyhow!("{} exported no {n}-vertex GLB", e.id))
        };
        // base head GLBs (untextured fallback: the base heads have no colour texture)
        let mut base_glbs: HashMap<String, std::path::PathBuf> = HashMap::new();
        for &i in &todo {
            if !base_glbs.contains_key(&v[i].class) {
                let bm = base_meshes[&v[i].class].iter().find(|m| m.2 .0.len() == v[i].vertices).unwrap();
                base_glbs.insert(v[i].class.clone(), glb_of(&bm.0, v[i].vertices)?);
            }
        }
        let built: Vec<(usize, Result<String>)> = todo
            .par_iter()
            .map(|&i| {
                let r = &v[i];
                let res = (|| -> Result<String> {
                    let perm = &perms[&(r.class.clone(), r.vertices)];
                    let bm = base_meshes[&r.class].iter().find(|m| m.2 .0.len() == r.vertices).unwrap();
                    let d = decode(&Resource::load(&a.path, by_off[&off_of(&r.id)])?, r.vertices)?;
                    let target: Vec<[f32; 3]> = (0..r.vertices).map(|j| { let (p, q) = (bm.2 .0[j], d[perm[j]]); [p[0] + q[0], p[1] + q[1], p[2] + q[2]] }).collect();
                    // the head's own drawable (copy of another head's mesh,
                    // but with this head's face textures): its vertices take
                    // the target positions of the base vertex with their UV
                    let own = r.head_by_order.as_deref().and_then(|n| {
                        by_name.get(n)?.iter().find_map(|e| {
                            let res = Resource::load(&a.path, e).ok()?;
                            let m = meshes(&res).into_iter().find(|m| m.0.len() == r.vertices)?;
                            let to_base = offsets(&m, &bm.2);
                            if to_base.iter().any(|o| o.is_none()) {
                                return None;
                            }
                            Some(((*e).clone(), m))
                        })
                    });
                    let (src, positions, how) = match own {
                        Some((e, m)) => {
                            let key = |u: [f32; 2]| ((u[0] * 1e5).round() as i64, (u[1] * 1e5).round() as i64);
                            let mut by_uv: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
                            for (j, &u) in bm.2 .1.iter().enumerate() {
                                by_uv.entry(key(u)).or_default().push(j);
                            }
                            let pos: Option<Vec<[f32; 3]>> = m.0.iter().zip(&m.1).map(|(&p, &u)| by_uv.get(&key(u))?.iter().min_by(|&&x, &&y| dist2(bm.2 .0[x], p).total_cmp(&dist2(bm.2 .0[y], p))).map(|&j| target[j])).collect();
                            (glb_of(&e, r.vertices)?, pos.ok_or_else(|| anyhow!("UV not in the base head"))?, "the head's own drawable (its face textures) with every vertex moved to base + delta")
                        }
                        None => (base_glbs[&r.class].clone(), target, "the base head moved by the delta (base heads carry no colour texture)"),
                    };
                    let stem = r.head_by_order.clone().unwrap_or_else(|| format!("head_{}", r.class));
                    // same folder depth as the exported models, so texture paths hold
                    let dir = out.join("models").join(crate::game::index_stem(&a.id)).join(format!("{stem}_delta_{:x}", off_of(&r.id)));
                    std::fs::create_dir_all(&dir)?;
                    let extras = serde_json::json!({"head_delta": r.id, "base": r.base, "head_by_order": r.head_by_order, "between": r.between, "built_from": how, "note": "normals are those of the source mesh"});
                    let p = dir.join("model.glb");
                    std::fs::write(&p, set_positions(&std::fs::read(&src)?, &positions, extras)?)?;
                    Ok(p.strip_prefix(out).unwrap_or(&p).to_string_lossy().replace('\\', "/"))
                })();
                (i, res)
            })
            .collect();
        for (i, res) in built {
            match res {
                Ok(f) => v[i].file = Some(f),
                Err(e) => v[i].file = Some(format!("error: {e:#}")),
            }
        }
    }
    let dir = out.join("head_deltas").join(a.id.trim_end_matches(".rpf"));
    std::fs::create_dir_all(&dir)?;
    let mut s = String::new();
    for r in &v {
        s.push_str(&serde_json::to_string(r)?);
        s.push('\n');
    }
    std::fs::write(dir.join("index.jsonl"), s)?;
    progress("done", 4, 4);
    Ok(v)
}

fn all_id(a: &crate::game::ArchiveInfo, off: u64) -> String {
    format!("{}@{off:x}", a.id)
}
fn off_of(id: &str) -> u64 {
    id.rsplit('@').next().and_then(|h| u64::from_str_radix(h, 16).ok()).unwrap_or(0)
}

fn row(d: &Delta, id: String, cl: &ClassLod, v: usize, head: Option<String>, err: Option<[f32; 2]>) -> DeltaRow {
    let _ = d;
    DeltaRow { id, class: cl.class.clone(), lod: cl.lod, vertices: v, base: cl.base_name.clone(), head, error_um: err, between: [None, None], head_by_order: None, file: None }
}

fn read_glb(b: &[u8]) -> Result<(serde_json::Value, Vec<u8>)> {
    if b.len() < 20 || &b[0..4] != b"glTF" {
        bail!("not a GLB");
    }
    let jl = u32::from_le_bytes(b[12..16].try_into()?) as usize;
    let doc: serde_json::Value = serde_json::from_slice(&b[20..20 + jl])?;
    let bo = 20 + jl;
    let bl = u32::from_le_bytes(b[bo..bo + 4].try_into()?) as usize;
    Ok((doc, b[bo + 8..bo + 8 + bl].to_vec()))
}

fn glb_vertex_count(p: &std::path::Path) -> Result<usize> {
    let (doc, _) = read_glb(&std::fs::read(p)?)?;
    let prims = doc["meshes"].as_array().context("meshes")?.iter().flat_map(|m| m["primitives"].as_array().cloned().unwrap_or_default()).collect::<Vec<_>>();
    if prims.len() != 1 {
        bail!("{} primitives", prims.len());
    }
    Ok(doc["accessors"][prims[0]["attributes"]["POSITION"].as_u64().context("POSITION")? as usize]["count"].as_u64().unwrap_or(0) as usize)
}

/// Replace the single primitive's POSITION with RAGE-space positions
/// (stored glTF Y-up: (x, y, z) -> (x, z, -y)).
fn set_positions(b: &[u8], delta: &[[f32; 3]], extras: serde_json::Value) -> Result<Vec<u8>> {
    let (mut doc, mut bin) = read_glb(b)?;
    let ai = doc["meshes"][0]["primitives"][0]["attributes"]["POSITION"].as_u64().context("POSITION")? as usize;
    let acc = doc["accessors"][ai].clone();
    if acc["count"].as_u64() != Some(delta.len() as u64) {
        bail!("vertex count differs");
    }
    let bv = &doc["bufferViews"][acc["bufferView"].as_u64().context("bufferView")? as usize];
    let start = bv["byteOffset"].as_u64().unwrap_or(0) as usize + acc["byteOffset"].as_u64().unwrap_or(0) as usize;
    let stride = bv["byteStride"].as_u64().unwrap_or(12) as usize;
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for (i, d) in delta.iter().enumerate() {
        let g = [d[0], d[2], -d[1]];
        for k in 0..3 {
            let o = start + stride * i + 4 * k;
            let v = g[k];
            bin[o..o + 4].copy_from_slice(&v.to_le_bytes());
            lo[k] = lo[k].min(v);
            hi[k] = hi[k].max(v);
        }
    }
    doc["accessors"][ai]["min"] = serde_json::json!(lo);
    doc["accessors"][ai]["max"] = serde_json::json!(hi);
    doc["asset"]["extras"] = extras;
    Ok(crate::gltf::pack(&doc, bin))
}
