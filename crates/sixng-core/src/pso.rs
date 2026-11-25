//! PSO files (`PSIN` magic): the plain-file form of the self-describing
//! meta format of the `0x7a` resources (`meta7a.rs`).
//!
//! Sections, each `magic (4) + u32 BE size (incl. header)`:
//! - `PSIN`: instance data. Little-endian block table of N entries
//!   `{u32 struct hash (low 32 bits), u32 byte length, 8 x aa}` from +8,
//!   then the blocks, each 16-byte aligned, from the first aligned offset
//!   after the table. Pointers are pool references `(offset << 20) | block`
//!   (1-based block index).
//! - `LSCH`: schema. Struct table of 32-byte entries `{u64 name hash, u32
//!   info (alignment/flags), u32 0, 8 x aa, u32 size, u16 0, u16 member
//!   count}`, then all members as 16-byte records in table order, the same
//!   record layout as the meta resources: `{u32 name (joaat, case-
//!   sensitive), u32 offset, u8 type, u8 sub, u16 reference, u32 extra}`.
//!   Struct references (`extra`) and block hashes use the low 32 bits of
//!   the 64-bit struct hash.
//! - `LENM`: enums (64-bit hashes; not decoded yet), `LSNS` (not decoded),
//!   `LGID`, `LRSC` (a copy of the meta resource header: `0DRPz`, then at
//!   +0x38 u16 struct count, u16 root block, u16 block count), `CHKS`.

use crate::meta7a::{Member, Meta, StructDef};
use crate::resource::Resource;
use std::collections::HashMap;

pub struct Pso {
    pub res: Resource,
    pub structs: HashMap<u32, StructDef>,
    /// Full 64-bit struct hashes by their low 32 bits.
    pub struct64: HashMap<u32, u64>,
    pub blocks: Vec<(u32, u32, usize)>,
    /// 1-based root block, if the header copy is present.
    pub root: Option<usize>,
}

fn be32(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(o..o + 4)?.try_into().ok()?))
}
fn le32(d: &[u8], o: usize) -> u32 {
    d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0)
}
fn le16(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map(|b| u16::from_le_bytes(b.try_into().unwrap())).unwrap_or(0)
}
fn le64(d: &[u8], o: usize) -> u64 {
    d.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap())).unwrap_or(0)
}

/// Section magic -> (data offset, data length).
pub fn sections(d: &[u8]) -> HashMap<[u8; 4], (usize, usize)> {
    let mut m = HashMap::new();
    let mut o = 0;
    while o + 8 <= d.len() {
        let Some(sz) = be32(d, o + 4).map(|x| x as usize) else { break };
        if sz < 8 || o + sz > d.len() {
            break;
        }
        m.insert(d[o..o + 4].try_into().unwrap(), (o + 8, sz - 8));
        o += sz;
    }
    m
}

pub fn is_pso(d: &[u8]) -> bool {
    d.starts_with(b"PSIN")
}

pub fn parse(d: &[u8]) -> Option<Pso> {
    let sec = sections(d);
    let &(pi, pl) = sec.get(b"PSIN")?;
    let &(si, sl) = sec.get(b"LSCH")?;
    let sys = d[pi - 8..pi + pl].to_vec();
    let s = &d[si..si + sl];
    // Schema.
    let mut ents = Vec::new();
    let mut k = 0;
    while k + 32 <= s.len() && s[k + 16..k + 24] == [0xaa; 8] {
        ents.push((le64(s, k), le32(s, k + 24), le16(s, k + 30) as usize));
        k += 32;
    }
    let mut structs = HashMap::new();
    let mut struct64 = HashMap::new();
    for (h, size, mc) in ents {
        let members = (0..mc)
            .map(|i| {
                let m = k + 16 * i;
                Member { hash: le32(s, m), off: le32(s, m + 4), ty: s.get(m + 8).copied().unwrap_or(0), sub: s.get(m + 9).copied().unwrap_or(0), reference: le16(s, m + 10), extra: le32(s, m + 12) }
            })
            .collect();
        k += 16 * mc;
        let h32 = h as u32;
        struct64.insert(h32, h);
        structs.insert(h32, StructDef { hash: h32, size, members });
    }
    // Block table, offsets relative to `sys` (which starts at the PSIN magic).
    let mut tab = Vec::new();
    let mut o = 8;
    while o + 16 <= sys.len() && sys[o + 8..o + 16] == [0xaa; 8] {
        tab.push((le32(&sys, o), le32(&sys, o + 4)));
        o += 16;
    }
    let mut blocks = Vec::new();
    for (h, len) in tab {
        o = (o + 15) & !15;
        blocks.push((h, len, if o + len as usize <= sys.len() { o } else { usize::MAX }));
        o += len as usize;
    }
    let root = sec.get(b"LRSC").and_then(|&(ri, _)| {
        (d.get(ri..ri + 5) == Some(b"0DRPz")).then(|| le16(d, ri + 0x3A) as usize).filter(|&b| b >= 1 && b <= blocks.len())
    });
    Some(Pso { res: Resource { sys, gfx: Vec::new(), gfx_base: 0 }, structs, struct64, blocks, root })
}

impl Pso {
    pub fn meta(&self) -> Meta<'_> {
        Meta { r: &self.res, structs: self.structs.clone(), blocks: self.blocks.clone(), pso: true }
    }

    /// The root object decoded to JSON (struct names as 64-bit hex).
    pub fn root_json(&self, depth: u32) -> Option<serde_json::Value> {
        let b = self.root?;
        let (h, _, o) = *self.blocks.get(b - 1)?;
        let mut v = self.meta().object(h, o, depth)?;
        if let Some(x) = self.struct64.get(&h) {
            v["_struct"] = serde_json::json!(format!("{x:016x}"));
        }
        Some(v)
    }
}

impl Pso {
    /// Root plus every block's objects by struct (64-bit hex keys).
    pub fn to_json(&self) -> serde_json::Value {
        let m = self.meta();
        let mut objs = serde_json::Map::new();
        let mut hs: Vec<u32> = self.blocks.iter().map(|b| b.0).filter(|h| self.structs.contains_key(h)).collect();
        hs.sort();
        hs.dedup();
        for h in hs {
            let v: Vec<_> = m.objects(h).iter().filter_map(|&o| m.object(h, o, 0)).collect();
            if !v.is_empty() {
                objs.insert(self.struct64.get(&h).map(|x| format!("{x:016x}")).unwrap_or(format!("{h:08x}")), serde_json::Value::Array(v));
            }
        }
        serde_json::json!({"root": self.root_json(0), "objects": objs})
    }
}

/// Decode PSO files to `<out>/pso/<archive>/<offset>.json` (root + objects
/// by struct; struct names are 64-bit hashes, field names resolve where the
/// name dictionary knows them). Returns (decoded, failed).
pub fn export(a: &crate::game::ArchiveInfo, entries: &[&crate::scan::Entry], out: &std::path::Path, progress: crate::export::Progress) -> anyhow::Result<(usize, usize)> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    let dir = out.join("pso").join(a.id.trim_end_matches(".rpf"));
    std::fs::create_dir_all(&dir)?;
    let total = entries.len() as u64;
    let done = AtomicU64::new(0);
    let (ok, bad) = (AtomicUsize::new(0), AtomicUsize::new(0));
    entries.par_iter().for_each(|e| {
        progress(&e.id, done.fetch_add(1, Ordering::Relaxed) + 1, total);
        let r = crate::scan::read_entry(&a.path, e).ok().and_then(|d| parse(&d)).map(|p| {
            let mut v = p.to_json();
            v["id"] = serde_json::json!(e.id);
            crate::atomic::write(dir.join(format!("{:x}.json", e.off)), serde_json::to_vec(&v).unwrap_or_default()).is_ok()
        });
        if r == Some(true) { &ok } else { &bad }.fetch_add(1, Ordering::Relaxed);
    });
    Ok((ok.into_inner(), bad.into_inner()))
}

/// Handling-record field hashes: handling name (u32 joaat of the lower-case
/// name, e.g. `phoenix`), second name hash (often 0), component list
/// (polymorphic sub-records: body/mass, transmission, engine, suspension, …).
pub const HANDLING_NAME: u32 = 0x93db_5464;
pub const HANDLING_BASE: u32 = 0x49a0_1a86;
pub const HANDLING_COMPONENTS: u32 = 0x41c8_dec7;

/// A label for a handling component from the known fields it has.
fn component_label(c: &serde_json::Value) -> Option<&'static str> {
    let has = |k: &str| c.get(k).is_some();
    Some(if has("fMass") {
        "body (mass, drag)"
    } else if has("fGearRatios") {
        "transmission"
    } else if has("fOilVolume") {
        "engine"
    } else if has("fPetrolTankVolume") {
        "fuel"
    } else if has("fSteeringLock") {
        "wheels, steering, brakes"
    } else if has("fSuspensionCompDamp") {
        "suspension"
    } else if has("fThrust") || has("fLiftMult") {
        "flight / water"
    } else if has("fCollisionDamageMult") {
        "damage"
    } else {
        return None;
    })
}

/// `fSteeringLock` (case-sensitive joaat).
const STEERING_LOCK: u32 = 0xd9cd_734b;

/// The steering lock (radians, the largest angle of the steered wheels) of
/// every handling record in the PSO files of `entries`: handling name hash
/// -> (lock where the record has one, base record's name hash). A record
/// without a lock takes its base's (`steering_lock`).
pub fn steering_locks(archive: &std::path::Path, entries: &[&crate::scan::Entry]) -> HashMap<u32, (Option<f32>, u32)> {
    use rayon::prelude::*;
    let (hn, hb, lock) = (crate::meta7a::field_name(HANDLING_NAME), crate::meta7a::field_name(HANDLING_BASE), crate::meta7a::field_name(STEERING_LOCK));
    fn find(v: &serde_json::Value, key: &str) -> Option<f64> {
        match v {
            serde_json::Value::Object(o) => o.get(key).and_then(|x| x.as_f64()).or_else(|| o.values().find_map(|x| find(x, key))),
            serde_json::Value::Array(a) => a.iter().find_map(|x| find(x, key)),
            _ => None,
        }
    }
    entries
        .par_iter()
        .filter_map(|e| {
            let p = parse(&crate::scan::read_entry(archive, e).ok()?)?;
            let m = p.meta();
            let mut v = Vec::new();
            for (&h, sd) in &p.structs {
                let has = |x: u32| sd.members.iter().any(|mm| mm.hash == x);
                if !(has(HANDLING_NAME) && has(HANDLING_COMPONENTS)) {
                    continue;
                }
                for o in m.objects(h) {
                    let Some(ob) = m.object(h, o, 0) else { continue };
                    let n = ob.get(&hn).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                    let b = ob.get(&hb).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                    v.push((n, (find(&ob, &lock).map(|x| x as f32), b)));
                }
            }
            Some(v)
        })
        .flatten()
        .collect()
}

/// The steering lock of a handling name, following base records.
pub fn steering_lock(locks: &HashMap<u32, (Option<f32>, u32)>, name: u32) -> Option<f32> {
    let mut n = name;
    for _ in 0..8 {
        let &(l, base) = locks.get(&n)?;
        if l.is_some() {
            return l;
        }
        n = base;
    }
    None
}

/// Vehicle handling (`sixng handling`): every PSO object with a handling
/// name and a component list, from every PSO in `entries`, with the name
/// resolved through `names` (joaat -> name). Writes `<out>/pso/handling.json`.
pub fn export_handling(a: &crate::game::ArchiveInfo, entries: &[&crate::scan::Entry], names: &HashMap<u32, String>, out: &std::path::Path) -> anyhow::Result<serde_json::Value> {
    use rayon::prelude::*;
    use serde_json::json;
    let hn = crate::meta7a::field_name(HANDLING_NAME);
    let hb = crate::meta7a::field_name(HANDLING_BASE);
    let hc = crate::meta7a::field_name(HANDLING_COMPONENTS);
    let mut recs: Vec<(String, serde_json::Value)> = entries
        .par_iter()
        .filter_map(|e| {
            let p = parse(&crate::scan::read_entry(&a.path, e).ok()?)?;
            let m = p.meta();
            let mut v = Vec::new();
            for (&h, sd) in &p.structs {
                let has = |x: u32| sd.members.iter().any(|mm| mm.hash == x);
                if !(has(HANDLING_NAME) && has(HANDLING_COMPONENTS)) {
                    continue;
                }
                for o in m.objects(h) {
                    let Some(mut ob) = m.object(h, o, 0) else { continue };
                    let n = ob.get(&hn).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                    let b = ob.get(&hb).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                    let mut comps = ob.get_mut(&hc).map(|x| x.take()).unwrap_or(json!([]));
                    for c in comps.as_array_mut().into_iter().flatten() {
                        if let Some(l) = component_label(c) {
                            c["_component"] = json!(l);
                        }
                    }
                    v.push((e.id.clone(), json!({
                        "handling": format!("{n:08x}"), "name": names.get(&n),
                        "base": format!("{b:08x}"), "base_name": names.get(&b),
                        "file": e.id, "components": comps,
                    })));
                }
            }
            Some(v)
        })
        .flatten()
        .collect();
    recs.sort_by(|x, y| (x.1["name"].as_str(), x.1["handling"].as_str()).cmp(&(y.1["name"].as_str(), y.1["handling"].as_str())));
    let named = recs.iter().filter(|r| !r.1["name"].is_null()).count();
    let dir = out.join("pso");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("handling.json");
    crate::atomic::write(&path, serde_json::to_vec_pretty(&json!({
        "note": "Vehicle handling records. Field names resolve where known (GTA V handling names, case-sensitive joaat); others are hashes. Components are typed sub-records (_struct).",
        "records": recs.iter().map(|r| &r.1).collect::<Vec<_>>(),
    }))?)?;
    Ok(json!({"records": recs.len(), "named": named, "file": path}))
}
