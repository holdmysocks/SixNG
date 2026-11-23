//! Audio metadata files: the sound, game-object, curve, speech, music,
//! mixer and synth definitions that go with a pack's audio banks (plain
//! `bin` files; a content pack's text file lists them as `<pack>:<file>:
//! Speech / GameObjects / AudioConfig / Sounds / ModularSynth / Curves /
//! DynamicMixer / Music`).
//!
//! Layout (little-endian), close to the GTA V audio metadata files:
//!
//! ```text
//! u32 type (schema id: 18, 20, 25, 26, 61, 86, 166, 581 in the minigame pack)
//! u32 data length, then the data (+8): the objects, back to back
//! u32 0x045a88ea at +8 (the first data word in every file)
//! two tables, each u32 byte length L then L - 4 bytes: u32 count, then
//!   count x {u32, u32} pairs and the hashes they refer to (empty: 4, 0)
//! u32 object count, then count x {u32 name hash, u32 data offset}
//!   (the index, sorted by the hash bytes; names are not stored)
//! further offset tables (hash and container references) follow; their
//!   layout differs between schemas and is not decoded.
//! ```
//!
//! Each object starts with a u8 class id (the first byte at its offset) and
//! a u8 header flags byte; flags 0 means no header fields (most objects),
//! otherwise a second bit mask and optional fields follow (not decoded).
//! Object sizes are the distance to the next object's offset.
//!
//! **Sound files** (schema 166, the `Sounds` file of a pack). The class ids
//! follow the usual numbering (12 simple, 6 wrapper, 14 randomized, 7
//! sequential ...). References between objects are u32 indices into the
//! objects in data-offset order (not the hash-sorted index). Decoded from
//! the end of each object, so the header does not have to be parsed:
//! - simple sound (12): `container hash, wave hash, u8, u8, u8 n, n x ref`
//!   after the header. The container is the bank and the low 29 bits of the
//!   wave hash are the stream id in it (2,050 of 2,051 in the minigame pack
//!   match a stream of the pack's own banks). The trailing list points at
//!   other sounds; its role is not known (many sounds share one target).
//! - randomized (14), sequential (7) and class 8: the object ends with
//!   `u8 n, n x ref`, the child sounds (siblings come from one bank in 16
//!   of 17 multi-child groups, against 1 of 19 when read as hash order).
//! - wrapper (6): the last 12 bytes are `child, u32, ref`; the child is
//!   always a simple or wrapper sound (or none, 0xffffffff).
//! - class 15: after the header, 6 bytes, `u8 k`, k bytes, `u8 n`, then n
//!   entries of `ref, f32, u8` (70 of 70; the f32 is mostly 1.0).
//! - child refs at fixed body offsets (every object of the class in the
//!   minigame pack reads as valid indices or none, and they point at
//!   sounds): class 1 at 6; 2 at 56; 5 at 0, 4 and 8; 16 at 15; 20, 27 at
//!   0; 28 at 0 and 4; 30 at 0 and 25; 48 at 0 and 8.
//!
//! **Sound header** (sound files, every class except 17, 49, 56 and 57,
//! whose second byte is not a header). Byte 1 is a mask: each set bit `i`
//! adds one sub-mask byte (in bit order) after it. Each set bit `j` of
//! sub-mask `i` adds one field, in (i, j) order, of a fixed size: (1,0) is
//! 8 bytes of 2-bit tristate flags (`aa` = all unset); (2,4), (2,5), (3,2),
//! (4,5), (5,3) are 4 bytes; (3,7), (4,2), (4,7) are 1 byte; the others
//! 2 bytes. Fields (0,0) and (0,2) are nested u8 masks whose u16 values
//! (one per set bit) come after all other header fields. Checked on the
//! minigame pack: 4,392 of 4,395 headers decode; every simple sound's header
//! ends where its container hash starts (2,051 of 2,051) and 2,015 of 2,016
//! wrappers then have exactly their 12-byte body. Field meanings are not
//! named (2-byte fields read as i16; (1,2) looks like a volume in mB).
use anyhow::{bail, Result};
use serde::Serialize;

pub const MAGIC: u32 = 0x045a_88ea;

#[derive(Debug, Clone, Serialize)]
pub struct Object {
    /// Name hash (joaat of the object name; names are not in the install).
    pub hash: String,
    /// Offset in the data block.
    pub offset: u32,
    /// Bytes to the next object (or the end of the data).
    pub size: u32,
    /// Class id: the first byte of the object.
    pub class: u8,
    /// Sound files: the wave a simple sound plays.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wave: Option<Wave>,
    /// Sound files: child sounds (name hashes) of randomized, sequential,
    /// class-8 and wrapper sounds.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    /// Sound files: the trailing reference list of a simple sound (role not
    /// known), as name hashes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
    /// Sound files, after `link`: every (bank offset, stream id) this sound
    /// can play, through its children.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub streams: Vec<(String, String)>,
    /// Sound files: header fields by "mask.bit" (nested: "mask.bit.bit"):
    /// 1-byte as u8, 2-byte as i16, 4- and 8-byte as hex.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    /// Sound files: the bytes after the header (hex), when the header decodes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Decoded class fields where the layout is known. Class 16: three name
    /// hashes, the slot bit count and mask, a value and the child slots
    /// (all empty, 0xff, in the files: filled at run time). Class 54: its own
    /// name hash, a group hash, a count, an f32 weight and two optional
    /// hashes. Class 56: a count and a hash (its two refs are `children`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<serde_json::Value>,
}

/// Classes of the sound files that have no sound header.
const NO_HEADER: [u8; 4] = [17, 49, 56, 57];

/// Size of a sound header field (mask byte, bit); `None` for the nested
/// masks (0,0) and (0,2), handled by `sound_header`, and for unknown bits.
fn field_size(i: usize, j: usize) -> Option<usize> {
    Some(match (i, j) {
        (1, 0) => 8,
        (1, 1) | (2, 4) | (2, 5) | (3, 2) | (4, 0) | (4, 5) | (5, 1..=4) | (5, 6) | (5, 7) | (6, 0..=3) | (6, 5) | (7, 1) | (7, 4..=6) => 4,
        (0, 1) | (0, 3) => 3,
        (3, 7) | (4, 2) | (4, 6) | (4, 7) | (5, 0) => 1,
        (1, 2..=6) | (2, 0..=3) | (2, 6) | (2, 7) | (3, 0) | (3, 1) | (3, 3..=6) | (4, 1) => 2,
        _ => return None,
    })
}

/// Decode a sound header (`d` is the whole object). Returns the fields and
/// the offset where the class body starts.
pub fn sound_header(d: &[u8]) -> Option<(std::collections::BTreeMap<String, serde_json::Value>, usize)> {
    let f = *d.get(1)?;
    let masks: Vec<usize> = (0..8).filter(|i| f >> i & 1 != 0).collect();
    let mut p = 2 + masks.len();
    let mb = d.get(2..p)?;
    let mut out = std::collections::BTreeMap::new();
    let mut nested = Vec::new();
    for (&i, &m) in masks.iter().zip(mb) {
        for j in 0..8 {
            if m >> j & 1 == 0 {
                continue;
            }
            if i == 0 && (j == 0 || j == 2) {
                nested.push((j, *d.get(p)?));
                p += 1;
                continue;
            }
            let n = field_size(i, j)?;
            let v = d.get(p..p + n)?;
            out.insert(
                format!("{i}.{j}"),
                match n {
                    1 => serde_json::json!(v[0]),
                    2 => serde_json::json!(i16::from_le_bytes([v[0], v[1]])),
                    _ => serde_json::json!(v.iter().rev().map(|x| format!("{x:02x}")).collect::<String>()),
                },
            );
            p += n;
        }
    }
    for (j, m) in nested {
        for k in 0..8 {
            if m >> k & 1 != 0 {
                let v = d.get(p..p + 2)?;
                out.insert(format!("0.{j}.{k}"), serde_json::json!(i16::from_le_bytes([v[0], v[1]])));
                p += 2;
            }
        }
    }
    Some((out, p))
}

#[derive(Debug, Clone, Serialize)]
pub struct Wave {
    /// Container (bank) name hash.
    pub container: String,
    /// Stream id in the bank: the low 29 bits of the wave name hash.
    pub stream: String,
    /// Bank offset in the archive (hex), after `link`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank: Option<String>,
}

/// Schema id of the sound files.
pub const SCHEMA_SOUNDS: u32 = 166;

#[derive(Debug, Clone, Serialize)]
pub struct AudioMeta {
    pub schema: u32,
    pub data_len: u32,
    /// Entry counts of the two tables before the index.
    pub table_counts: [u32; 2],
    pub objects: Vec<Object>,
    /// Objects per class id.
    pub classes: std::collections::BTreeMap<u8, usize>,
}

pub fn is_audio_meta(b: &[u8]) -> bool {
    b.len() >= 16 && u32::from_le_bytes(b[8..12].try_into().unwrap()) == MAGIC && (u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize) + 8 <= b.len()
}

pub fn parse(b: &[u8]) -> Result<AudioMeta> {
    parse_with(b, None)
}

/// `u8 n, n x u32` at the end of `d`, refs below `max`, list start at or
/// after `min_p`: every reading, shortest list first.
fn tail_lists(d: &[u8], min_p: usize, max: u32) -> Vec<(usize, Vec<u32>)> {
    let mut out = Vec::new();
    for n in 0..64usize {
        let Some(p) = d.len().checked_sub(4 * n + 1) else { break };
        if p < min_p {
            break;
        }
        if d[p] as usize == n {
            let r: Vec<u32> = (0..n).map(|i| u32::from_le_bytes(d[p + 1 + 4 * i..p + 5 + 4 * i].try_into().unwrap())).collect();
            if r.iter().all(|&x| x < max) {
                out.push((p, r));
            }
        }
    }
    out
}

/// Like `parse`; with `streams` (stream ids of the archive's banks), a
/// simple sound whose end reads two ways takes the reading whose wave is a
/// known stream.
pub fn parse_with(b: &[u8], streams: Option<&std::collections::HashSet<u32>>) -> Result<AudioMeta> {
    if !is_audio_meta(b) {
        bail!("not an audio metadata file");
    }
    let u = |o: usize| -> Result<u32> { b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap())).ok_or_else(|| anyhow::anyhow!("truncated at {o:#x}")) };
    let (schema, data_len) = (u(0)?, u(4)?);
    let mut p = 8 + data_len as usize;
    let mut table_counts = [0u32; 2];
    for c in &mut table_counts {
        let len = u(p)? as usize;
        if len < 4 {
            bail!("bad table length {len} at {p:#x}");
        }
        *c = u(p + 4)?;
        p += 4 + len;
    }
    let n = u(p)? as usize;
    if n > 1 << 20 || p + 4 + 8 * n > b.len() {
        bail!("bad object count {n}");
    }
    let mut idx: Vec<(u32, u32)> = (0..n).map(|i| Ok((u(p + 4 + 8 * i)?, u(p + 8 + 8 * i)?))).collect::<Result<_>>()?;
    if idx.iter().any(|&(_, o)| o >= data_len) {
        bail!("object offset past the data");
    }
    let mut offs: Vec<u32> = idx.iter().map(|x| x.1).collect();
    offs.sort_unstable();
    offs.dedup();
    idx.sort_by_key(|x| x.1);
    let objects: Vec<Object> = idx
        .iter()
        .map(|&(h, o)| {
            let next = offs.iter().find(|&&x| x > o).copied().unwrap_or(data_len);
            Object { hash: format!("{h:08x}"), offset: o, size: next - o, class: b[8 + o as usize], wave: None, children: Vec::new(), refs: Vec::new(), streams: Vec::new(), header: None, body: None, fields: None }
        })
        .collect();
    let mut objects = objects;
    if schema == SCHEMA_SOUNDS {
        // References are indices in data order; `objects` is sorted by offset.
        let order: Vec<String> = objects.iter().map(|o| o.hash.clone()).collect();
        let max = order.len() as u32;
        let name = |r: u32| order[r as usize].clone();
        let u32at = |d: &[u8], p: usize| u32::from_le_bytes(d[p..p + 4].try_into().unwrap());
        for o in objects.iter_mut() {
            let d = &b[8 + o.offset as usize..8 + (o.offset + o.size) as usize];
            let mut body_at = None;
            if o.class >= 50 && d.len() >= 5 {
                // game objects: the class byte, then 4 bytes of 2-bit flags
                // (0xaa: all unset) instead of the sound header
                o.header = Some([("flags".to_string(), serde_json::json!(format!("{:08x}", u32at(d, 1))))].into_iter().collect());
                o.body = Some(d[5..].iter().map(|x| format!("{x:02x}")).collect());
                body_at = Some(5);
            } else if !NO_HEADER.contains(&o.class) {
                if let Some((h, end)) = sound_header(d) {
                    o.header = Some(h);
                    o.body = Some(d[end..].iter().map(|x| format!("{x:02x}")).collect());
                    body_at = Some(end);
                }
            }
            if let Some(e) = body_at {
                let body = &d[e..];
                let at = |p: usize| (p + 4 <= body.len()).then(|| u32at(body, p));
                // Child refs at fixed body offsets, checked on the 242k-object
                // main sound file: >= 96% valid indices, and nearer the parent
                // in data order than chance.
                let fixed: &[usize] = match o.class {
                    1 => &[6],
                    2 => &[56],
                    3 => &[29, 33],
                    5 => &[0, 4, 8],
                    9 => &[20],
                    10 | 28 => &[0, 4],
                    19 => &[33],
                    20 | 27 | 29 | 30 | 31 | 36 | 40 | 48 => &[0],
                    41 => &[0, 6, 12],
                    46 => &[9],
                    // game objects that point at sounds
                    56 => &[6, 10],
                    61 | 62 | 64 => &[0],
                    68 => &[14],
                    _ => &[],
                };
                let mut kids: Vec<u32> = Vec::new();
                if o.class == 54 && body.len() == 23 {
                    let h = |p: usize| format!("{:08x}", u32at(body, p));
                    let opt = |p: usize| (u32at(body, p) != 0).then(|| h(p));
                    o.fields = Some(serde_json::json!({
                        "name": h(0),
                        "group": h(4),
                        "count": body[8],
                        "weight": f32::from_le_bytes(body[9..13].try_into().unwrap()),
                        "hash_a": opt(13),
                        "hash_b": opt(17),
                        "tail": u16::from_le_bytes([body[21], body[22]]),
                    }));
                }
                if o.class == 56 && body.len() == 14 {
                    o.fields = Some(serde_json::json!({"count": body[0], "hash": format!("{:08x}", u32at(body, 2))}));
                }
                if o.class == 16 && body.len() >= 23 {
                    let c = body.len().checked_sub(1).and_then(|_| (0..=40usize).find(|&c| body.len() >= c + 2 && body[body.len() - c - 1] as usize == c && body[body.len() - c - 2] == 2));
                    if let Some(c) = c {
                        let h = |p: usize| format!("{:08x}", u32at(body, p));
                        o.fields = Some(serde_json::json!({
                            "k": body[1],
                            "hashes": [h(2), h(6), h(10)],
                            "bits": body[14],
                            "mask": h(15),
                            "value": u32at(body, 19),
                            "slots": body[body.len() - c..].iter().map(|&x| if x == 0xff { serde_json::Value::Null } else { serde_json::json!(x) }).collect::<Vec<_>>(),
                        }));
                    }
                }
                let u16at = |p: usize| body.get(p..p + 2).map(|x| u16::from_le_bytes([x[0], x[1]]) as usize);
                if o.class == 51 {
                    // u16 count, then that many refs
                    if let Some(n) = u16at(0).filter(|&n| 2 + 4 * n == body.len()) {
                        kids = (0..n).filter_map(|i| at(2 + 4 * i)).collect();
                    }
                } else if o.class == 52 {
                    // u16, u16 count, then (ref, ref or none) pairs
                    if let Some(n) = u16at(2).filter(|&n| 4 + 8 * n == body.len()) {
                        kids = (0..2 * n).filter_map(|i| at(4 + 4 * i)).collect();
                    }
                } else if o.class == 24 {
                    // a count byte, then that many refs, at the end of the body
                    if let Some(n) = (1..=64usize).find(|&n| body.len() > 4 * n && body[body.len() - 4 * n - 1] as usize == n) {
                        let s = body.len() - 4 * n;
                        kids = (0..n).filter_map(|i| at(s + 4 * i)).collect();
                    }
                } else if o.class == 15 {
                    let mut p = 7 + *body.get(6).unwrap_or(&0) as usize;
                    let n = body.get(p).copied().unwrap_or(0) as usize;
                    p += 1;
                    if p + 9 * n == body.len() {
                        kids = (0..n).filter_map(|i| at(p + 9 * i)).collect();
                    }
                } else {
                    let v: Vec<Option<u32>> = fixed.iter().map(|&p| at(p)).collect();
                    if v.iter().all(|x| x.is_some_and(|x| x < max || x == u32::MAX)) {
                        kids = v.into_iter().flatten().collect();
                    }
                }
                if kids.iter().all(|&x| x < max || x == u32::MAX) {
                    o.children.extend(kids.into_iter().filter(|&x| x < max).map(name));
                }
            }
            match o.class {
                12 => {
                    let ts = tail_lists(d, 12, max);
                    let known = |p: usize| streams.is_some_and(|s| s.contains(&(u32at(d, p - 6) & 0x1fff_ffff)));
                    if let Some((p, r)) = ts.iter().rev().find(|(p, _)| known(*p)).or(ts.last()) {
                        o.wave = Some(Wave { container: format!("{:08x}", u32at(d, p - 10)), stream: format!("{:08x}", u32at(d, p - 6) & 0x1fff_ffff), bank: None });
                        o.refs = r.iter().map(|&x| name(x)).collect();
                    }
                }
                14 | 7 | 8 => {
                    if let Some((_, r)) = tail_lists(d, 2, max).last() {
                        o.children = r.iter().map(|&x| name(x)).collect();
                    }
                }
                6 if d.len() >= 14 => {
                    let c = u32at(d, d.len() - 12);
                    if c < max {
                        o.children = vec![name(c)];
                    }
                }
                _ => {}
            }
        }
    }
    let mut classes = std::collections::BTreeMap::new();
    for o in &objects {
        *classes.entry(o.class).or_insert(0) += 1;
    }
    Ok(AudioMeta { schema, data_len, table_counts, objects, classes })
}

/// Fill in each wave's bank and every sound's reachable streams. `banks` is
/// (bank offset, stream ids). A container takes the bank that holds most of
/// its waves (the lowest offset on a tie; stream ids repeat across banks).
/// Returns (waves found in their bank, waves).
pub fn link(m: &mut AudioMeta, banks: &[(u64, std::collections::HashSet<u32>)]) -> (usize, usize) {
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    let id = |s: &str| u32::from_str_radix(s, 16).unwrap_or(0);
    let mut by_container: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for o in &m.objects {
        if let Some(w) = &o.wave {
            by_container.entry(w.container.clone()).or_default().insert(id(&w.stream));
        }
    }
    let bank_of: HashMap<String, usize> = by_container
        .iter()
        .filter_map(|(c, ws)| {
            let (i, n) = banks.iter().enumerate().map(|(i, (off, ids))| (i, ws.iter().filter(|w| ids.contains(w)).count(), std::cmp::Reverse(*off))).max_by_key(|x| (x.1, x.2)).map(|x| (x.0, x.1))?;
            (n > 0).then(|| (c.clone(), i))
        })
        .collect();
    let (mut ok, mut all) = (0, 0);
    for o in m.objects.iter_mut() {
        if let Some(w) = &mut o.wave {
            all += 1;
            if let Some(&i) = bank_of.get(&w.container) {
                if banks[i].1.contains(&id(&w.stream)) {
                    w.bank = Some(format!("{:x}", banks[i].0));
                    ok += 1;
                }
            }
        }
    }
    // Reachable streams, depth-first through children (cycle-safe).
    let at: HashMap<String, usize> = m.objects.iter().enumerate().map(|(i, o)| (o.hash.clone(), i)).collect();
    type Set = BTreeSet<(String, String)>;
    fn walk(i: usize, m: &AudioMeta, at: &HashMap<String, usize>, memo: &mut Vec<Option<Set>>, stack: &mut Vec<usize>) -> Set {
        if let Some(s) = &memo[i] {
            return s.clone();
        }
        if stack.contains(&i) || stack.len() > 64 {
            return Set::new();
        }
        stack.push(i);
        let o = &m.objects[i];
        let mut s = Set::new();
        if let Some(w) = &o.wave {
            if let Some(b) = &w.bank {
                s.insert((b.clone(), w.stream.clone()));
            }
        }
        for c in &o.children {
            if let Some(&j) = at.get(c) {
                s.extend(walk(j, m, at, memo, stack));
            }
        }
        stack.pop();
        memo[i] = Some(s.clone());
        s
    }
    let mut memo: Vec<Option<Set>> = vec![None; m.objects.len()];
    let reach: Vec<Set> = (0..m.objects.len()).map(|i| walk(i, m, &at, &mut memo, &mut Vec::new())).collect();
    for (o, r) in m.objects.iter_mut().zip(reach) {
        o.streams = r.into_iter().collect();
    }
    (ok, all)
}

/// Decode every audio metadata file of an archive to
/// `<out>/audio_meta/<archive>/<offset>.json` plus `index.jsonl`.
/// Returns (decoded, failed).
/// `banks`: (bank offset, stream ids) of the archive's audio index, to link
/// sound files to streams (empty: not linked).
pub fn export(a: &crate::game::ArchiveInfo, entries: &[&crate::scan::Entry], banks: &[(u64, std::collections::HashSet<u32>)], out: &std::path::Path) -> Result<(usize, usize)> {
    let known: std::collections::HashSet<u32> = banks.iter().flat_map(|(_, s)| s.iter().copied()).collect();
    let dir = out.join("audio_meta").join(a.id.trim_end_matches(".rpf"));
    std::fs::create_dir_all(&dir)?;
    let (mut ok, mut bad) = (0, 0);
    let mut lines = String::new();
    let mut by_off: Vec<&crate::scan::Entry> = entries.to_vec();
    by_off.sort_by_key(|e| e.off);
    for e in entries {
        let Ok(b) = crate::scan::read_entry(&a.path, e) else { continue };
        if !is_audio_meta(&b) {
            continue;
        }
        let mut parsed = parse_with(&b, (!known.is_empty()).then_some(&known));
        let mut stitched = None;
        if parsed.is_err() {
            if let Some((full, info)) = stitch(&a.path, e, &b, &by_off) {
                if let Ok(m) = parse_with(&full, (!known.is_empty()).then_some(&known)) {
                    parsed = Ok(m);
                    stitched = Some(info);
                }
            }
        }
        match parsed {
            Ok(mut m) => {
                let linked = if m.schema == SCHEMA_SOUNDS && !banks.is_empty() { Some(link(&mut m, banks)) } else { None };
                ok += 1;
                let file = format!("{:x}.json", e.off);
                std::fs::write(dir.join(&file), serde_json::to_vec_pretty(&serde_json::json!({"id": e.id, "meta": m, "stitched": stitched}))?)?;
                lines += &serde_json::json!({"id": e.id, "file": file, "schema": m.schema, "objects": m.objects.len(), "table_counts": m.table_counts, "classes": m.classes, "waves_linked": linked.map(|l| serde_json::json!({"in_bank": l.0, "waves": l.1})), "stitched": stitched}).to_string();
            }
            Err(x) => {
                bad += 1;
                lines += &serde_json::json!({"id": e.id, "error": x.to_string()}).to_string();
            }
        }
        lines.push('\n');
    }
    std::fs::write(dir.join("index.jsonl"), lines)?;
    Ok((ok, bad))
}

/// A large file can be indexed as two entries with stored (not zlib) 64 KiB
/// chunks between them, which the stream scan does not read. When `b` is cut
/// short, joins it with the next entry, the skipped chunks filled with zeros,
/// but only when the skipped bytes lie past the object data (in the hash
/// tables, of which only the lengths are read), so every object is intact.
fn stitch(path: &std::path::Path, e: &crate::scan::Entry, b: &[u8], by_off: &[&crate::scan::Entry]) -> Option<(Vec<u8>, serde_json::Value)> {
    let last = e.streams.last()?;
    let end = (last[0] + last[1] + 15) & !15;
    let next = by_off.iter().find(|x| x.off >= end)?;
    let gap = next.off - end;
    if gap % 0x10000 != 0 || gap > 64 * 0x10000 || b.len() < 8 {
        return None;
    }
    let data_end = 8 + u32::from_le_bytes(b[4..8].try_into().ok()?) as u64;
    if (b.len() as u64) < data_end {
        return None;
    }
    let tail = crate::scan::read_entry(path, next).ok()?;
    let mut full = b.to_vec();
    full.resize(b.len() + gap as usize, 0);
    full.extend_from_slice(&tail);
    Some((full, serde_json::json!({"next": next.id, "unread_bytes": gap, "unread_at": format!("{:#x}", b.len())})))
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_minimal_file() {
        // schema 26, 8 data bytes (two objects of 4), empty tables, 2 index entries
        let mut b = Vec::new();
        for v in [26u32, 8, super::MAGIC, 0x0300_0000, 4, 0, 4, 0, 2, 0xaaaa, 0, 0xbbbb, 4] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let m = super::parse(&b).unwrap();
        assert_eq!(m.objects.len(), 2);
        assert_eq!((m.objects[0].class, m.objects[0].size), (0xea, 4));
        assert_eq!((m.objects[1].class, m.objects[1].size), (0x00, 4));
    }

    #[test]
    fn class15_children() {
        // a class-15 sound (no header fields; body: 6 bytes, k = 0, n = 2,
        // then `ref, f32, u8` x 2) pointing at two simple sounds
        let mut d: Vec<u8> = super::MAGIC.to_le_bytes().to_vec();
        d.extend_from_slice(&[15, 0, 0, 0, 0, 0, 0, 0, 0, 2]);
        for r in [2u32, 3] {
            d.extend_from_slice(&r.to_le_bytes());
            d.extend_from_slice(&1f32.to_le_bytes());
            d.push(0);
        }
        for w in [0x11u32, 0x22] {
            d.extend_from_slice(&[12, 0]);
            d.extend_from_slice(&0xc0u32.to_le_bytes());
            d.extend_from_slice(&w.to_le_bytes());
            d.extend_from_slice(&[0, 0, 0]);
        }
        let mut b = Vec::new();
        for v in [166u32, d.len() as u32] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&d);
        for v in [4u32, 0, 4, 0, 4, 0xa, 0, 0xb, 4, 0xc, 32, 0xd, 45] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let m = super::parse(&b).unwrap();
        assert_eq!((m.objects[1].class, m.objects[1].children.clone()), (15, vec!["0000000c".to_string(), "0000000d".to_string()]));
    }

    #[test]
    fn sound_header_fields() {
        // wrapper: masks 0, 1, 3 = 01 04 04; (0,0) nested mask 0x80, (1,2)
        // volume 600, (3,2) hash; the nested u16 (50) comes last; 12-byte body
        let d = [6u8, 0x0b, 1, 4, 4, 0x80, 0x58, 0x02, 0xfa, 0xe0, 0xff, 0xd0, 0x32, 0x00, 0xf9, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
        let (h, end) = super::sound_header(&d).unwrap();
        assert_eq!(end, d.len() - 12);
        assert_eq!(h["1.2"], serde_json::json!(600));
        assert_eq!(h["3.2"], serde_json::json!("d0ffe0fa"));
        assert_eq!(h["0.0.7"], serde_json::json!(50));
    }

    #[test]
    fn sound_graph() {
        // schema 166: after the magic word, a randomized sound with children
        // 2 and 3 (data order: 0 is the magic word's "object"), then two
        // simple sounds in container 0xc0 playing waves 0x2000_0011 / 0x22
        let mut d: Vec<u8> = super::MAGIC.to_le_bytes().to_vec();
        d.extend_from_slice(&[14, 0, 2, 2, 0, 0, 0, 3, 0, 0, 0]);
        for w in [0x2000_0011u32, 0x22] {
            d.extend_from_slice(&[12, 0]);
            d.extend_from_slice(&0xc0u32.to_le_bytes());
            d.extend_from_slice(&w.to_le_bytes());
            d.extend_from_slice(&[0, 0, 0]);
        }
        let mut b = Vec::new();
        for v in [166u32, d.len() as u32] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&d);
        for v in [4u32, 0, 4, 0, 4, 0xa, 0, 0xb, 4, 0xc, 15, 0xd, 28] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let mut m = super::parse(&b).unwrap();
        let r = &m.objects[1];
        assert_eq!((r.class, r.children.clone()), (14, vec!["0000000c".to_string(), "0000000d".to_string()]));
        assert_eq!(m.objects[2].wave.as_ref().unwrap().stream, "00000011");
        let banks = vec![(0x100u64, [0x11u32, 0x22].into_iter().collect())];
        assert_eq!(super::link(&mut m, &banks), (2, 2));
        assert_eq!(m.objects[1].streams, vec![("100".to_string(), "00000011".to_string()), ("100".to_string(), "00000022".to_string())]);
    }
}
