//! Research: read the parser struct table from the game executable, giving
//! the name hash, size, base struct and members (name hash, offset, type) of
//! every meta struct the game can load, including the many that no shipped
//! meta file's own schema describes.
//!
//! exeschema [--work <work dir>] <exe> <out.json> [<struct hash> ...]
//!
//! Output: `{"structs": [...], "enums": [...]}`. The listed struct hashes
//! are also printed as flattened layouts (base struct members first). With
//! `--work`, hashes are named from that work folder's name dictionary as
//! well as the built-in lists.
//!
//! Layout (found by hand, checked against the schemas of the levels' meta
//! files: 233 of their 234 structs are in the table, and with base structs
//! followed, all 1421 of their members are found, with matching types):
//!
//! - Each struct has a getter in .text: `lea rcx, [slot]; lea rdx, [..];
//!   call <ctor>` builds the runtime struct into a u64 `slot` in .data. The
//!   static header follows the slot: u64 pointer to the member table, u32
//!   name hash (case-sensitive joaat of the class name), u32 size, u16 flags,
//!   then one u32 member offset per member (unaligned, at header + 18).
//! - Member table: u8, u16 member count at +1, u16 record offsets at +3.
//!   Member record k starts at table + 0x18 + offset k: u32 name hash
//!   (case-sensitive joaat of the field name), u8 type, u8 sub type.
//! - Arrays and maps describe their element with records named `Item`
//!   (063fa3f2) placed just before the array's own record (a map has a key
//!   record and a value record).
//! - Later in the getter, `call <getter>; mov [rip + x], rax` stores the
//!   struct of a struct member or element record 12 bytes into its record,
//!   and `call <getter>; mov rcx, <this>; mov rdx, rax; call <link>` sets
//!   the base struct.
//! - Enum and bitset member records hold, at +20, a pointer to a 0x20-byte
//!   enum object: u64 pointer to its value table, two u64 (0 in the file),
//!   u32 enum name hash, u16 value count. Value table entries are 12 bytes:
//!   u32 value name hash (case-sensitive joaat), s32 value, u32 0; the
//!   table ends with {0, -1, -1}.
use std::collections::HashMap;

const ITEM: u32 = 0x063f_a3f2;

/// rage parMemberType order. Checked against meta file schemas (bool, ints,
/// float, string, struct, array, enum, bitset, map, vec3v, vec4v) and
/// against the member sizes for the rest; 0x28 is a 12-byte vector.
const TYPES: &[&str] = &[
    "bool", "s8", "u8", "s16", "u16", "s32", "u32", "f32", "vec2", "vec3", "vec4", "string",
    "struct", "array", "enum", "bitset", "map", "mat34", "mat44", "vec2v", "vec3v", "vec4v",
    "mat33v", "mat34v", "mat44v", "scalarv", "boolv", "vecboolv", "ptrdiff", "size_t", "f16",
    "s64", "u64", "f64",
];

/// Types after rage's list, named from the meta files' own schemas (all
/// archives, 1,294 shared structs):
/// * 0x22: 8 bytes, file type 0x08, the `id` of entity and archetype
///   definitions and `Entity` references: a 64-bit object id.
/// * 0x23: 8 bytes at 4-byte offsets, file type 0x30 (`Scale`, `Pos`, `Min`):
///   a two-float vector without SIMD padding.
/// * 0x24: 16 bytes, stored as the files' vec4 type 0x34, always `Rotation`:
///   a quaternion.
///
/// 0x25..0x27 do not occur in any meta file.
fn type_name(t: u8) -> String {
    match t {
        0x22 => "id64".into(),
        0x23 => "vec2_8".into(),
        0x24 => "quatv".into(),
        0x28 => "vec3_12".into(),
        _ => TYPES
            .get(t as usize)
            .map(|s| s.to_string())
            .unwrap_or(format!("t{t:02x}")),
    }
}

struct Pe {
    base: u64,
    /// (name, rva, virtual size, raw offset, raw size)
    secs: Vec<([u8; 8], u32, u32, u32, u32)>,
}

impl Pe {
    fn new(m: &[u8]) -> Option<Self> {
        let pe = u32le(m, 0x3c)? as usize;
        (m.get(pe..pe + 4)? == b"PE\0\0").then_some(())?;
        let ns = u16le(m, pe + 6)? as usize;
        let oh = u16le(m, pe + 0x14)? as usize;
        let base = u64le(m, pe + 0x30)?;
        let so = pe + 0x18 + oh;
        let secs = (0..ns)
            .map(|i| {
                let o = so + 40 * i;
                let mut n = [0u8; 8];
                n.copy_from_slice(m.get(o..o + 8)?);
                Some((
                    n,
                    u32le(m, o + 12)?,
                    u32le(m, o + 8)?,
                    u32le(m, o + 20)?,
                    u32le(m, o + 16)?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Pe { base, secs })
    }
    fn sec(&self, name: &[u8]) -> Option<(u32, u32, u32, u32)> {
        self.secs
            .iter()
            .find(|s| s.0.starts_with(name))
            .map(|s| (s.1, s.2, s.3, s.4))
    }
}

fn u16le(m: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(m.get(o..o + 2)?.try_into().ok()?))
}
fn u32le(m: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(m.get(o..o + 4)?.try_into().ok()?))
}
fn u64le(m: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(m.get(o..o + 8)?.try_into().ok()?))
}

#[derive(Clone)]
struct Mem {
    hash: u32,
    off: u32,
    ty: u8,
    sub: u8,
    rec: u32,
    strukt: Option<u32>,
    /// element records (rva) of an array or map
    items: Vec<u32>,
    /// enum object (pointer) of an enum or bitset member
    enm: Option<u64>,
}

struct Def {
    hash: u32,
    size: u32,
    flags: u16,
    base: Option<u32>,
    members: Vec<Mem>,
}

fn main() -> anyhow::Result<()> {
    let mut a: Vec<String> = std::env::args().collect();
    if let Some(i) = a.iter().position(|x| x == "--work") {
        let w = a
            .get(i + 1)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("--work needs a folder"))?;
        sixng_core::names::set_work_dir(std::path::Path::new(&w));
        a.drain(i..i + 2);
    }
    if a.len() < 3 {
        anyhow::bail!("usage: exeschema [--work <work dir>] <exe> <out.json> [<struct hash> ...]");
    }
    let m = std::fs::read(&a[1])?;
    let pe = Pe::new(&m).ok_or_else(|| anyhow::anyhow!("not a PE file"))?;
    let (tva, _, tro, trs) = pe
        .sec(b".text")
        .ok_or_else(|| anyhow::anyhow!("no .text"))?;
    let (dva, _, dro, drs) = pe
        .sec(b".data")
        .ok_or_else(|| anyhow::anyhow!("no .data"))?;
    let text = &m[tro as usize..(tro + trs) as usize];
    let tva = tva as i64;
    // .data rva -> file offset (initialised part only)
    let dat = |r: i64| -> Option<usize> {
        (r >= dva as i64 && r + 0x20 < (dva + drs) as i64)
            .then(|| (r - dva as i64 + dro as i64) as usize)
    };
    let ptr = |p: u64| -> Option<usize> { p.checked_sub(pe.base).and_then(|r| dat(r as i64)) };
    // any section's raw data
    let file_of = |p: u64| -> Option<usize> {
        let r = p.checked_sub(pe.base)? as u32;
        pe.secs
            .iter()
            .find(|s| r >= s.1 && r < s.1 + s.4)
            .map(|s| (r - s.1 + s.3) as usize)
            .filter(|&o| o + 0x20 <= m.len())
    };
    let rel = |i: usize, len: usize| -> i64 {
        tva + (i + len) as i64
            + i32::from_le_bytes(text[i + len - 4..i + len].try_into().unwrap()) as i64
    };

    // Getter anchors: lea rcx,[slot]; lea rdx,[..]; call X. The constructor
    // is the call target with the most valid headers behind its slots.
    let mut cand: HashMap<i64, Vec<(i64, usize)>> = HashMap::new();
    let mut i = 0;
    while i + 19 <= text.len() {
        if text[i] == 0x48
            && text[i + 1] == 0x8d
            && text[i + 2] == 0x0d
            && text[i + 7] == 0x48
            && text[i + 8] == 0x8d
            && text[i + 9] == 0x15
            && text[i + 14] == 0xe8
        {
            let slot = rel(i, 7);
            if dat(slot + 8).is_some() {
                cand.entry(rel(i + 14, 5)).or_default().push((slot, i));
            }
        }
        i += 1;
    }
    let (ctor, anchors) = cand
        .into_iter()
        .max_by_key(|(_, v)| v.len())
        .ok_or_else(|| anyhow::anyhow!("no struct getters found"))?;
    eprintln!("constructor {ctor:x}, {} getters", anchors.len());

    let mut getter: HashMap<i64, usize> = HashMap::new();
    for &(s, i) in &anchors {
        getter.entry(s).or_insert(i);
    }
    let mut defs: HashMap<i64, Def> = HashMap::new();
    let mut rec_owner: HashMap<u32, (i64, usize)> = HashMap::new(); // record rva -> (slot, member)
    let mut item_owner: HashMap<u32, (i64, usize)> = HashMap::new();
    let mut bad_rec = 0usize;
    let drva = |o: usize| -> u32 { (o as i64 - dro as i64 + dva as i64) as u32 };
    for (&s, _) in &getter {
        let Some(h) = dat(s + 8) else { continue };
        let (p, hash, size, flags) = (
            u64le(&m, h).unwrap(),
            u32le(&m, h + 8).unwrap(),
            u32le(&m, h + 12).unwrap(),
            u16le(&m, h + 16).unwrap(),
        );
        let mut members = Vec::new();
        if let Some(t) = ptr(p) {
            let n = u16le(&m, t + 1).unwrap_or(0) as usize;
            if n < 4000 {
                for k in 0..n {
                    let (Some(r), Some(off)) =
                        (u16le(&m, t + 3 + 2 * k), u32le(&m, h + 18 + 4 * k))
                    else {
                        break;
                    };
                    let q = t + 0x18 + r as usize;
                    let (Some(mh), Some(&ty), Some(&sub)) =
                        (u32le(&m, q), m.get(q + 4), m.get(q + 5))
                    else {
                        break;
                    };
                    if u16le(&m, q + 8) != Some(0xffff) {
                        bad_rec += 1;
                    }
                    members.push(Mem {
                        hash: mh,
                        off,
                        ty,
                        sub,
                        rec: drva(q),
                        strukt: None,
                        items: Vec::new(),
                        enm: (ty == 0x0e || ty == 0x0f)
                            .then(|| u64le(&m, q + 20))
                            .flatten()
                            .filter(|&p| file_of(p).is_some()),
                    });
                }
            }
        }
        // element records sit between the previous member record and an array / map record
        let mut prev = members
            .first()
            .map(|x| x.rec.saturating_sub(0x180))
            .unwrap_or(0);
        for (k, mm) in members.iter_mut().enumerate() {
            rec_owner.insert(mm.rec, (s, k));
            if mm.ty == 0x0d || mm.ty == 0x10 {
                let lo = prev.max(mm.rec.saturating_sub(0x180));
                let mut items: Vec<u32> = (lo + 6..mm.rec.saturating_sub(15))
                    .filter(|&r| dat(r as i64).and_then(|o| u32le(&m, o)) == Some(ITEM))
                    .collect();
                items.truncate(2);
                for &r in &items {
                    item_owner.insert(r, (s, k));
                }
                mm.items = items;
            }
            prev = mm.rec + 6;
        }
        defs.insert(
            s,
            Def {
                hash,
                size,
                flags,
                base: None,
                members,
            },
        );
    }

    // Resolve `call getter` to that getter's struct slot.
    let slot_of = |f: i64| -> Option<i64> {
        let mut i = (f - tva) as usize;
        for _ in 0..3 {
            if i + 0x40 > text.len() {
                return None;
            }
            if text[i] != 0xe9 {
                break;
            }
            i = (rel(i, 5) - tva) as usize;
        }
        if i + 0x40 > text.len() {
            return None;
        }
        (i..i + 0x30).find_map(|k| {
            ((text[k] == 0x48 || text[k] == 0x4c) && text[k + 1] == 0x8b && text[k + 2] & 0xc7 == 5)
                .then(|| rel(k, 7))
                .filter(|t| defs.contains_key(t))
        })
    };
    // Walk each getter body: struct references and base links.
    let mut links: Vec<(i64, i64, i64)> = Vec::new(); // (slot, base getter, link fn)
    let mut stores: Vec<(u32, i64)> = Vec::new();
    for (&s, &i) in &getter {
        let end = text[i..]
            .windows(2)
            .position(|w| w == [0xc3, 0xcc])
            .map(|p| (i + p + 1).min(i + 0x1000))
            .unwrap_or(i + 0x800)
            .min(text.len() - 16);
        for j in i + 19..end {
            if text[j] != 0xe8 {
                continue;
            }
            let k = j + 5;
            // mov [rip + x], rax; or mov rcx, rax; ...; mov [rip + x], rcx
            let st = if text[k..k + 3] == [0x48, 0x89, 0x05] {
                Some(k)
            } else if text[k..k + 3] == [0x48, 0x89, 0xc1] {
                (k + 3..k + 9).find(|&q| text[q..q + 3] == [0x48, 0x89, 0x0d])
            } else {
                None
            };
            if let Some(q) = st {
                if let Some(b) = slot_of(rel(j, 5)) {
                    stores.push(((rel(q, 7) - 12) as u32, b));
                }
            }
            if let Some(q) = text[k..k + 8]
                .windows(3)
                .position(|w| w == [0x48, 0x89, 0xc2])
            {
                for q2 in [k + q + 3, k + q + 6] {
                    if text[q2] == 0xe8 {
                        if let Some(b) = slot_of(rel(j, 5)) {
                            links.push((s, b, rel(q2, 5)));
                        }
                    }
                }
            }
        }
    }
    let mut lc: HashMap<i64, usize> = HashMap::new();
    for l in &links {
        *lc.entry(l.2).or_default() += 1;
    }
    let link = lc.into_iter().max_by_key(|x| x.1).map(|x| x.0);
    let mut nb = 0;
    for &(s, b, l) in &links {
        if Some(l) == link {
            let bh = defs[&b].hash;
            let d = defs.get_mut(&s).unwrap();
            if d.base.is_none() {
                d.base = Some(bh);
                nb += 1;
            }
        }
    }
    let mut ns = 0;
    for &(r, b) in &stores {
        let bh = defs[&b].hash;
        if let Some(&(s, k)) = rec_owner.get(&r).or_else(|| item_owner.get(&r)) {
            let mm = &mut defs.get_mut(&s).unwrap().members[k];
            if mm.strukt.is_none() {
                mm.strukt = Some(bh);
                ns += 1;
            }
        }
    }
    // Enum objects and their value tables.
    let mut enums: HashMap<u64, (u32, Vec<(u32, i32)>)> = HashMap::new();
    for mm in defs.values().flat_map(|d| d.members.iter()) {
        let Some(p) = mm.enm else { continue };
        if enums.contains_key(&p) {
            continue;
        }
        let o = file_of(p).unwrap();
        let (hash, n) = (
            u32le(&m, o + 24).unwrap_or(0),
            u16le(&m, o + 28).unwrap_or(0) as usize,
        );
        let mut vals = Vec::new();
        if let Some(t) = u64le(&m, o).and_then(file_of) {
            for k in 0..n.min(4096) {
                let (Some(h), Some(v)) = (u32le(&m, t + 12 * k), u32le(&m, t + 12 * k + 4)) else {
                    break;
                };
                if h == 0 && v == u32::MAX {
                    break;
                }
                vals.push((h, v as i32));
            }
        }
        enums.insert(p, (hash, vals));
    }
    let nm: usize = defs.values().map(|d| d.members.len()).sum();
    eprintln!(
        "{} structs, {nm} members ({bad_rec} odd records), {nb} base links, {ns} struct references, {} enums ({} values)",
        defs.len(),
        enums.len(),
        enums.values().map(|e| e.1.len()).sum::<usize>()
    );

    // Output, by struct hash.
    let name = |h: u32| sixng_core::names::lookup(h, sixng_core::names::Ctx::Field);
    let item = |r: u32| -> serde_json::Value {
        let o = dat(r as i64).unwrap();
        let (ty, sub) = (m[o + 4], m[o + 5]);
        let st = stores
            .iter()
            .find(|x| x.0 == r)
            .map(|x| format!("{:08x}", defs[&x.1].hash));
        serde_json::json!({"type": type_name(ty), "sub": sub, "struct": st})
    };
    let mut all: Vec<&Def> = defs.values().collect();
    all.sort_by_key(|d| (d.hash, d.size));
    let by_hash: HashMap<u32, &Def> = all.iter().map(|d| (d.hash, *d)).collect();
    let out: Vec<serde_json::Value> = all
        .iter()
        .map(|d| {
            let ms: Vec<_> = d
                .members
                .iter()
                .map(|mm| {
                    let mut v = serde_json::json!({"hash": format!("{:08x}", mm.hash), "offset": mm.off, "type": type_name(mm.ty), "sub": mm.sub});
                    if let Some(n) = name(mm.hash) {
                        v["name"] = n.into();
                    }
                    if let Some(st) = mm.strukt {
                        v["struct"] = format!("{st:08x}").into();
                    }
                    if !mm.items.is_empty() {
                        v["items"] = mm.items.iter().map(|&r| item(r)).collect();
                    }
                    if let Some(e) = mm.enm.and_then(|p| enums.get(&p)) {
                        v["enum"] = format!("{:08x}", e.0).into();
                    }
                    v
                })
                .collect();
            let mut v = serde_json::json!({"hash": format!("{:08x}", d.hash), "size": d.size, "flags": d.flags, "members": ms});
            if let Some(n) = name(d.hash) {
                v["name"] = n.into();
            }
            if let Some(b) = d.base {
                v["base"] = format!("{b:08x}").into();
            }
            v
        })
        .collect();
    let mut el: Vec<&(u32, Vec<(u32, i32)>)> = enums.values().collect();
    el.sort_by_key(|e| e.0);
    el.dedup_by_key(|e| e.0);
    let eout: Vec<serde_json::Value> = el
        .iter()
        .map(|(h, vals)| {
            let vs: Vec<_> = vals
                .iter()
                .map(|&(vh, v)| {
                    let mut j = serde_json::json!({"hash": format!("{vh:08x}"), "value": v});
                    if let Some(n) = name(vh) {
                        j["name"] = n.into();
                    }
                    j
                })
                .collect();
            let mut j = serde_json::json!({"hash": format!("{h:08x}"), "values": vs});
            if let Some(n) = name(*h) {
                j["name"] = n.into();
            }
            j
        })
        .collect();
    std::fs::write(
        &a[2],
        serde_json::to_vec_pretty(&serde_json::json!({"structs": out, "enums": eout}))?,
    )?;

    for h in &a[3..] {
        let h = u32::from_str_radix(h.trim_start_matches("0x"), 16)?;
        let Some(d) = by_hash.get(&h) else {
            println!("{h:08x}: not in the table");
            continue;
        };
        let mut chain = vec![*d];
        while let Some(b) = chain.last().unwrap().base.and_then(|b| by_hash.get(&b)) {
            if chain.len() > 16 {
                break;
            }
            chain.push(b);
        }
        println!("{h:08x} {} size {:#x}", name(h).unwrap_or_default(), d.size);
        for d in chain.iter().rev() {
            println!("  -- {:08x} {}", d.hash, name(d.hash).unwrap_or_default());
            for mm in &d.members {
                let st = mm
                    .strukt
                    .map(|s| format!(" -> {s:08x}"))
                    .unwrap_or_default();
                let it: Vec<String> = mm
                    .items
                    .iter()
                    .map(|&r| item(r))
                    .map(|v| {
                        format!(
                            "{}{}",
                            v["type"].as_str().unwrap_or(""),
                            v["struct"]
                                .as_str()
                                .map(|s| format!(" {s}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect();
                let it = if it.is_empty() {
                    String::new()
                } else {
                    format!(" of [{}]", it.join(", "))
                };
                println!(
                    "    {:#06x}  {:<8} {}{st}{it}",
                    mm.off,
                    type_name(mm.ty),
                    name(mm.hash).unwrap_or(format!("{:08x}", mm.hash))
                );
                if let Some((eh, vals)) = mm.enm.and_then(|p| enums.get(&p)) {
                    let vs: Vec<String> = vals
                        .iter()
                        .map(|&(vh, v)| format!("{}={v}", name(vh).unwrap_or(format!("{vh:08x}"))))
                        .collect();
                    println!(
                        "              {} {{{}}}",
                        name(*eh).unwrap_or(format!("{eh:08x}")),
                        vs.join(", ")
                    );
                }
            }
        }
    }
    Ok(())
}
