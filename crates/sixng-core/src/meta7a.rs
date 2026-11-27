//! Self-describing metadata resources (`PRD0` resources of type 0x7a).
//!
//! Each one carries its own schema, like a PSO file packed into a resource:
//!
//! | Off  | Meaning |
//! |------|---------|
//! | 0x10 | `0DRP` + u8 0x7a |
//! | 0x20 | u32 number of root objects |
//! | 0x28 | ptr → struct table (32 bytes per struct) |
//! | 0x30 | ptr → enum table |
//! | 0x38 | ptr → instance table (16 bytes per block) |
//! | 0x48 | u16 structs, u16 enums, u16 instance blocks, u16 ? |
//!
//! Struct: u32 name hash, u32 signature, u32 flags, u32 0, u64 ptr → members,
//! u32 size, u16 0, u16 member count. Member (16 bytes): u32 name hash
//! (joaat of the field name; 0x100 = anonymous array element), u32 offset,
//! u8 type, u8 sub type, u16 reference (for arrays: index of the element
//! member), u32 extra. Instance block: u32 struct hash, u32 byte size,
//! u64 ptr; a block holds size / struct size consecutive objects.
//!
//! Map placement files (the GTA V `.ymap` equivalent) are these resources
//! with a root struct `d3593fa6` (map data) and entity structs `ce501483`
//! (0x90 bytes). Field names are joaat hashes; `position`, `rotation`,
//! `flags`, `extensions`, `entities`, `parent`, `block` resolve, the rest
//! are named by hash here.
use crate::resource::Resource;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

pub fn joaat(s: &str) -> u32 {
    let mut h: u32 = 0;
    for c in s.to_ascii_lowercase().bytes() {
        h = h.wrapping_add(c as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}

/// The 64-bit id the vehicle data gives a model (a mod part element's
/// `asset`): the first half of MurmurHash3 x64 128 (seed 0) of the model's
/// lower-case name without a LOD suffix.
pub fn asset_id(name: &str) -> u64 {
    const C1: u64 = 0x87c3_7b91_1142_53d5;
    const C2: u64 = 0x4cf5_ad43_2745_937f;
    let key = name.to_ascii_lowercase().into_bytes();
    let word = |b: &[u8]| b.iter().enumerate().fold(0u64, |a, (i, c)| a | (*c as u64) << (8 * i));
    let fmix = |mut k: u64| {
        k ^= k >> 33;
        k = k.wrapping_mul(0xff51_afd7_ed55_8ccd);
        k ^= k >> 33;
        k = k.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        k ^ (k >> 33)
    };
    let (mut h1, mut h2) = (0u64, 0u64);
    let mut blocks = key.chunks_exact(16);
    for b in &mut blocks {
        h1 ^= word(&b[..8]).wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
        h1 = h1.rotate_left(27).wrapping_add(h2).wrapping_mul(5).wrapping_add(0x52dc_e729);
        h2 ^= word(&b[8..]).wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
        h2 = h2.rotate_left(31).wrapping_add(h1).wrapping_mul(5).wrapping_add(0x3849_5ab5);
    }
    let tail = blocks.remainder();
    if tail.len() > 8 {
        h2 ^= word(&tail[8..]).wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
    }
    if !tail.is_empty() {
        h1 ^= word(&tail[..tail.len().min(8)]).wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
    }
    h1 ^= key.len() as u64;
    h2 ^= key.len() as u64;
    h1 = h1.wrapping_add(h2);
    h2 = h2.wrapping_add(h1);
    fmix(h1).wrapping_add(fmix(h2))
}

#[cfg(test)]
mod asset_id_tests {
    #[test]
    fn known_ids() {
        // pairs of a part's model name and its element's asset id in a vehicle pack
        assert_eq!(super::asset_id("declasse500_bumf_01"), 0x2da2_50cd_ad02_445c);
        assert_eq!(super::asset_id("declasse500_pickup"), 0xe0da_7376_852e_a1fa);
        assert_eq!(super::asset_id("Declasse500_Sunshade"), 0x41c7_5e89_df70_9658);
        assert_eq!(super::asset_id("declasse500"), 0xb9b9_435d_f4f3_2ce5);
    }
}

/// Member types decoded as one plain value (no pointer, array, map or
/// embedded struct). Type codes are a family in the high nibble: 0x0_
/// bool, struct (05), pointer (07), 64-bit id (08); 0x1_ integers s8, u8,
/// s16, u16, s32, u32, s64, u64; 0x2_ f16, f32; 0x3_ vectors and matrices;
/// 0x4_ strings and hashes; 0x5_ arrays; 0x6_ enums (60-62) and bit sets
/// (63-68); 0x7_ maps. Checked against the executable's parser tables on
/// every meta file.
pub fn is_scalar_type(t: u8) -> bool {
    matches!(t, 0x01 | 0x08 | 0x10..=0x17 | 0x20 | 0x21 | 0x30..=0x36 | 0x42 | 0x44 | 0x4a | 0x4d | 0x4f | 0x60..=0x65 | 0x68)
}

/// Named values of the executable's enums (`enum_values.txt`): enum hash
/// -> value -> name.
pub fn enum_values() -> &'static HashMap<u32, BTreeMap<i64, &'static str>> {
    static M: std::sync::OnceLock<HashMap<u32, BTreeMap<i64, &'static str>>> = std::sync::OnceLock::new();
    M.get_or_init(|| {
        let mut m: HashMap<u32, BTreeMap<i64, &'static str>> = HashMap::new();
        for l in include_str!("enum_values.txt").lines().filter(|l| !l.starts_with('#')) {
            let mut it = l.split_whitespace();
            let (Some(h), Some(v), Some(n)) = (it.next().and_then(|h| u32::from_str_radix(h, 16).ok()), it.next().and_then(|v| v.parse().ok()), it.next()) else { continue };
            // a value with several names (aliases) keeps the first one listed
            m.entry(h).or_default().entry(v).or_insert(n);
        }
        m
    })
}

/// Name of one enum value (`enum_hash` is the member's `extra`), if the
/// executable's tables name it.
pub fn enum_value_name(enum_hash: u32, value: i64) -> Option<&'static str> {
    enum_values().get(&enum_hash)?.get(&value).copied()
}

/// A half float (member type 0x20) as f32. Infinity and NaN read as f32::MAX.
pub fn half(h: u16) -> f32 {
    let (s, e, f) = (if h & 0x8000 != 0 { -1.0f32 } else { 1.0 }, (h >> 10) & 0x1f, (h & 0x3ff) as f32);
    let x = match e {
        0 => f / 1024.0 * 2f32.powi(-14),
        31 => f32::MAX,
        _ => (1.0 + f / 1024.0) * 2f32.powi(e as i32 - 15),
    };
    s * x
}

#[derive(Debug, Clone, Serialize)]
pub struct Member {
    pub hash: u32,
    pub off: u32,
    pub ty: u8,
    pub sub: u8,
    pub reference: u16,
    /// u32 at +12: for embedded structs / pointers / arrays of structs, the
    /// struct name hash; for others a size or 0.
    pub extra: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructDef {
    pub hash: u32,
    pub size: u32,
    pub members: Vec<Member>,
}

pub struct Meta<'a> {
    pub r: &'a Resource,
    pub structs: HashMap<u32, StructDef>,
    /// (struct hash, byte size, sys offset)
    pub blocks: Vec<(u32, u32, usize)>,
    /// PSO file (`pso.rs`): pointers are pool references only.
    pub pso: bool,
}

pub const ENTITY: u32 = 0xce50_1483;
pub const MAPDATA: u32 = 0xd359_3fa6;

pub fn is_meta(r: &Resource) -> bool {
    r.sys.get(0x10..0x15) == Some(b"0DRPz")
}

/// Field names we could resolve (joaat). Everything else is reported as hex.
const FIELD_NAMES: &[&str] = &["name", "position", "rotation", "flags", "extensions", "entities", "parent", "block", "guid", "scale", "instances", "archetypes", "archetype"];

pub fn field_name(h: u32) -> String {
    static M: std::sync::OnceLock<HashMap<u32, &'static str>> = std::sync::OnceLock::new();
    let m = M.get_or_init(|| FIELD_NAMES.iter().map(|n| (joaat(n), *n)).collect());
    m.get(&h).map(|s| s.to_string()).or_else(|| crate::names::lookup(h, crate::names::Ctx::Field)).unwrap_or_else(|| format!("{h:08x}"))
}

/// Struct names that resolve: the map formats' struct hashes are the
/// case-sensitive joaat of the GTA V type names, with `::` written as `__`.
const STRUCT_NAMES: &[&str] = &[
    "CMapData", "CMapTypes", "CEntityDef", "CBaseArchetypeDef", "CMloArchetypeDef", "CMloInstanceDef", "CMloRoomDef", "CMloPortalDef",
    "CMloEntitySet", "CLightAttrDef", "CExtensionDefLightEffect", "rage__fwInstancedMapData", "rage__fwPropInstanceListDef",
    "rage__fwPropInstanceListDef__InstanceData", "rage__fwGrassInstanceListDef", "rage__fwGrassInstanceListDef__InstanceData",
];

/// The type name of a struct hash, when it is one of the resolved ones.
pub fn struct_name(h: u32) -> Option<&'static str> {
    static M: std::sync::OnceLock<HashMap<u32, &'static str>> = std::sync::OnceLock::new();
    M.get_or_init(|| STRUCT_NAMES.iter().map(|n| (crate::names::joaat_exact(n), *n)).collect()).get(&h).copied()
}

impl<'a> Meta<'a> {
    pub fn parse(r: &'a Resource) -> Option<Self> {
        if !is_meta(r) {
            return None;
        }
        let nst = r.u16(0x48) as usize;
        let nbl = r.u16(0x4C) as usize;
        let st = r.sys_off(r.u64(0x28))?;
        let mut structs = HashMap::new();
        for i in 0..nst {
            let o = st + 32 * i;
            let hash = r.u32(o);
            let mp = r.sys_off(r.u64(o + 0x10))?;
            let size = r.u32(o + 0x18);
            let mc = r.u16(o + 0x1E) as usize;
            let members = (0..mc)
                .map(|k| {
                    let m = mp + 16 * k;
                    Member { hash: r.u32(m), off: r.u32(m + 4), ty: r.sys.get(m + 8).copied().unwrap_or(0), sub: r.sys.get(m + 9).copied().unwrap_or(0), reference: r.u16(m + 10), extra: r.u32(m + 12) }
                })
                .collect();
            structs.insert(hash, StructDef { hash, size, members });
        }
        let it = r.sys_off(r.u64(0x38))?;
        // Keep every block in place (pool refs index them 1-based); a block in
        // the graphics pages resolves only for resources loaded with
        // `Resource::load_meta`, otherwise its offset is usize::MAX.
        let blocks = (0..nbl)
            .map(|i| {
                let o = it + 16 * i;
                (r.u32(o), r.u32(o + 4), r.meta_off(r.u64(o + 8)).unwrap_or(usize::MAX))
            })
            .collect();
        Some(Meta { r, structs, blocks, pso: false })
    }

    /// Resolve a pointer field: a pool reference into the instance blocks
    /// (u64 with the 1-based block index in the low 12 bits and the byte
    /// offset in bits 20+) or, failing that, a real system
    /// or graphics pointer (`0x5xxxxxxx` / `0x6xxxxxxx`).
    ///
    /// Pool references come first: one whose byte offset is 0x500-0x5FF or
    /// 0x600-0x6FF with a zero low byte (`0x5000_0003` = block 3, offset
    /// 0x500) also reads as a real pointer to a few bytes into the page,
    /// which turned 1.8% of prop-batch instances into garbage. In the map
    /// resources no pointer, array or map member needs the real reading.
    pub fn ptr(&self, v: u64) -> Option<(u32, usize)> {
        if let Some(p) = self.pool_ref(v) {
            return Some(p);
        }
        if !self.pso {
            if let Some(o) = self.r.meta_off(v) {
                return Some((0, o));
            }
        }
        None
    }

    /// A pool reference: (struct hash of the block, sys offset).
    fn pool_ref(&self, v: u64) -> Option<(u32, usize)> {
        let bi = (v & 0xFFF) as usize;
        let off = (v >> 20) as usize;
        let &(h, size, o) = self.blocks.get(bi.checked_sub(1)?)?;
        (off < size as usize && o != usize::MAX).then_some((h, o + off))
    }

    /// The root instance block: the first block of a struct that no member
    /// of any struct refers to (the top-level object). Blocks of plain values
    /// (hashes below 0x100: float, u8, u16, u32 and pointer arrays) and of
    /// nested structs are skipped. The header has no root index: +0x4A is the
    /// enum count, which only matched the root block by chance (14 of 131
    /// non-map resources in the minigame pack).
    pub fn root_block(&self) -> Option<(u32, usize)> {
        let referenced: std::collections::HashSet<u32> = self.structs.values().flat_map(|s| s.members.iter().map(|m| m.extra)).collect();
        let usable = |&&(h, _, o): &&(u32, u32, usize)| o != usize::MAX && self.structs.contains_key(&h);
        let pick = self.blocks.iter().filter(usable).find(|b| !referenced.contains(&b.0)).or_else(|| self.blocks.iter().find(usable))?;
        Some((pick.0, pick.2))
    }

    /// Objects of one struct type: sys offsets.
    pub fn objects(&self, hash: u32) -> Vec<usize> {
        let Some(sd) = self.structs.get(&hash) else { return vec![] };
        let sz = sd.size.max(1) as usize;
        let mut v = Vec::new();
        for &(h, size, o) in &self.blocks {
            if h == hash && o != usize::MAX {
                for k in 0..size as usize / sz {
                    if o + (k + 1) * sz <= self.r.sys.len() {
                        v.push(o + k * sz);
                    }
                }
            }
        }
        v
    }

    /// Decode the scalar fields of an object (arrays/pointers/structs skipped).
    pub fn scalars(&self, hash: u32, o: usize, skip: &[u32]) -> BTreeMap<String, serde_json::Value> {
        self.scalars_by_hash(hash, o, skip).into_iter().map(|(h, v)| (field_name(h), v)).collect()
    }

    /// Like [`Meta::scalars`], keyed by the member's name hash. Readers that
    /// pick fields by hash use this one, so a newly resolved field name does
    /// not change what they find.
    pub fn scalars_by_hash(&self, hash: u32, o: usize, skip: &[u32]) -> BTreeMap<u32, serde_json::Value> {
        let mut out = BTreeMap::new();
        let Some(sd) = self.structs.get(&hash) else { return out };
        for m in &sd.members {
            if m.hash == 0x100 || skip.contains(&m.off) {
                continue;
            }
            if !is_scalar_type(m.ty) {
                continue;
            }
            let Some(v) = self.value(sd, m, o + m.off as usize, 0) else { continue };
            out.insert(m.hash, v);
        }
        out
    }
}

impl<'a> Meta<'a> {
    /// Byte size of an array element member (0 if unknown).
    fn elem_size(&self, el: &Member) -> usize {
        match el.ty {
            0x01 | 0x10 | 0x11 | 0x60 => 1,
            0x12 | 0x13 => 2,
            0x20 | 0x61 => 2,
            0x14 | 0x15 | 0x4a | 0x21 | 0x62 => 4,
            0x32 => 12,
            0x31 | 0x33 | 0x34 => 16,
            0x35 => 48,
            0x36 => 64,
            0x08 | 0x4f | 0x07 | 0x4d | 0x16 | 0x17 | 0x30 | 0x42 => 8,
            0x44 => 16,
            // An array of arrays: each element is a 16-byte array header.
            0x52 => 16,
            0x05 => self.structs.get(&el.extra).map(|x| x.size as usize).unwrap_or(0),
            _ => 0,
        }
    }

    fn value(&self, sd: &StructDef, m: &Member, p: usize, depth: u32) -> Option<serde_json::Value> {
        use serde_json::json;
        let r = self.r;
        let f = |i: usize| json!(r.f32(p + 4 * i));
        Some(match m.ty {
            0x10 => json!(r.sys.get(p).map(|&x| x as i8)),
            0x11 | 0x60 => json!(r.sys.get(p).copied()),
            0x12 => json!(r.u16(p) as i16),
            0x13 => json!(r.u16(p)),
            0x14 => json!(r.u32(p) as i32),
            0x15 | 0x4a => json!(r.u32(p)),
            0x16 => json!(r.u64(p) as i64),
            0x17 => json!(r.u64(p)),
            0x21 => f(0),
            0x30 => json!([f(0), f(1)]),
            // 3x3 and 3x4 matrices: three or four 16-byte columns.
            0x35 => json!((0..3).map(|c| [r.f32(p + 16 * c), r.f32(p + 16 * c + 4), r.f32(p + 16 * c + 8)]).collect::<Vec<_>>()),
            0x36 => json!((0..4).map(|c| [r.f32(p + 16 * c), r.f32(p + 16 * c + 4), r.f32(p + 16 * c + 8)]).collect::<Vec<_>>()),
            // Enums: 8-, 16- and 32-bit values (0x60 is above, with u8).
            0x61 => json!(r.u16(p)),
            0x62 => json!(r.u32(p)),
            0x32 | 0x33 => json!([f(0), f(1), f(2)]),
            0x34 | 0x31 => json!([f(0), f(1), f(2), f(3)]),
            0x08 | 0x4f => json!(format!("{:016x}", r.u64(p))),
            0x05 => self.object(m.extra, p, depth + 1).unwrap_or_else(|| json!({"struct": format!("{:08x}", m.extra)})),
            // Polymorphic pointers (extra 0) take the struct of the block they point into.
            0x07 => match self.ptr(r.u64(p)) {
                Some((h, o)) => self.object(if m.extra != 0 { m.extra } else { h }, o, depth + 1).unwrap_or_else(|| json!({"ptr": o})),
                None => serde_json::Value::Null,
            },
            0x52 => {
                let n = r.u16(p + 8) as usize;
                let el = sd.members.get(m.reference as usize)?;
                let Some((_, base)) = self.ptr(r.u64(p)) else { return Some(json!([])) };
                let esz = self.elem_size(el);
                if esz == 0 || n > 100_000 {
                    return Some(json!({"array": n, "elem_type": el.ty, "elem_extra": format!("{:08x}", el.extra)}));
                }
                json!((0..n).filter_map(|i| self.value(sd, el, base + i * esz, depth)).collect::<Vec<_>>())
            }
            0x01 => json!(r.sys.get(p).map(|&x| x != 0)),
            // Map (PSO): u32 group count, u32 entry count, u64 pointer to
            // the first of `groups` consecutive hash-table group structs
            // (16 control bytes `b8794c89`, 16 slots `93c495af`); only the
            // occupied slots ({key, value}) are returned.
            0x75 => {
                let Some((h, o)) = self.ptr(r.u64(p + 8)) else { return Some(json!([])) };
                let size = self.structs.get(&h).map_or(0, |s| s.size as usize);
                let groups = (r.u32(p) as usize).clamp(1, 4096);
                let mut out = Vec::new();
                for g in 0..if size == 0 { 1 } else { groups } {
                    if g > 0 && o + (g + 1) * size > r.sys.len() {
                        break;
                    }
                    let Some(t) = self.object(h, o + g * size, depth + 1) else { break };
                    let (Some(ctrl), Some(slots)) = (t.get(field_name(0xb879_4c89)).and_then(|x| x.as_array()), t.get(field_name(0x93c4_95af)).and_then(|x| x.as_array())) else {
                        if g == 0 {
                            return Some(t);
                        }
                        break;
                    };
                    out.extend(ctrl.iter().zip(slots).filter(|(c, _)| c.as_u64().is_some_and(|c| c < 0x80)).map(|(_, sl)| json!({"key": sl.get(field_name(0x6098_a50e)), "value": sl.get(field_name(0x063f_a3f2))})));
                }
                json!(out)
            }
            // Map (meta, 24 bytes): u64 1, u64 pool reference, u16 count,
            // u16 capacity. The reference points at `count` consecutive entry
            // structs ({`6098a50e` key, `063fa3f2` value}, struct taken from
            // the block). Empty maps point at block 0.
            0x71 => {
                let n = r.u16(p + 16) as usize;
                let Some((h, base)) = self.ptr(r.u64(p + 8)) else { return Some(json!([])) };
                let esz = self.structs.get(&h).map_or(0, |s| s.size as usize);
                if esz == 0 || n > 100_000 {
                    return Some(json!({"map": n, "struct": format!("{h:08x}")}));
                }
                json!((0..n).filter_map(|i| self.object(h, base + i * esz, depth + 1)).collect::<Vec<_>>())
            }
            // Bit set / flags: `reference` bits, stored as 64-bit words, lowest
            // first. Printed as one hex number (16 digits per word, highest
            // word first), so a set of up to 64 bits is its single word.
            // 0x63, 0x64, 0x65 and 0x68 store the bits in 8-, 16-, 32- and
            // 64-bit words (the member sizes in every meta file agree);
            // `reference` 0 (one 256-bit set) takes 32 bytes.
            0x63 | 0x64 | 0x65 | 0x68 => {
                let wb = match m.ty { 0x63 => 1, 0x64 => 2, 0x65 => 4, _ => 8 };
                let bits = if m.reference == 0 { 256 } else { m.reference as usize };
                let words = bits.div_ceil(wb * 8).clamp(1, 64);
                let bytes = r.sys.get(p..p + words * wb).unwrap_or(&[]);
                json!(bytes.iter().rev().map(|b| format!("{b:02x}")).collect::<String>())
            }
            // 64-bit hash (PSO).
            0x4d => json!(format!("{:016x}", r.u64(p))),
            // Array, count first (PSO): u32 count, u32 capacity, u64 pointer.
            0x5c => {
                let n = r.u32(p) as usize;
                let el = sd.members.get(m.reference as usize)?;
                let esz = self.elem_size(el);
                let Some((_, base)) = self.ptr(r.u64(p + 8)) else { return Some(json!([])) };
                if esz == 0 || n > 100_000 {
                    return Some(json!({"array": n, "elem_type": el.ty, "elem_extra": format!("{:08x}", el.extra)}));
                }
                json!((0..n).filter_map(|i| self.value(sd, el, base + i * esz, depth)).collect::<Vec<_>>())
            }
            // Inline fixed array: `extra` elements of the referenced member.
            0x50 => {
                let el = sd.members.get(m.reference as usize)?;
                let esz = self.elem_size(el);
                let n = m.extra as usize;
                if esz == 0 || n > 100_000 {
                    return Some(json!({"inline_array": n, "elem_type": el.ty}));
                }
                json!((0..n).filter_map(|i| self.value(sd, el, p + i * esz, depth)).collect::<Vec<_>>())
            }
            // Fixed-capacity array with a count: `extra` elements of the
            // referenced member, then a u32 count of the used ones (the
            // game pads the rest after loading).
            0x53 => {
                let el = sd.members.get(m.reference as usize)?;
                let esz = self.elem_size(el);
                let cap = m.extra as usize;
                if esz == 0 || cap > 100_000 {
                    return Some(json!({"inline_array": cap, "elem_type": el.ty}));
                }
                let n = (r.u32(p + cap * esz) as usize).min(cap);
                json!((0..n).filter_map(|i| self.value(sd, el, p + i * esz, depth)).collect::<Vec<_>>())
            }
            // Half float (2 bytes).
            0x20 => json!(half(r.u16(p))),
            // Strings: 0x42 a pointer to NUL-terminated text, 0x44 the same
            // pointer followed by a u16 length (16 bytes).
            0x42 | 0x44 => match self.ptr(r.u64(p)) {
                Some((_, o)) => {
                    let max = if m.ty == 0x44 { r.u16(p + 8) as usize } else { 4096 };
                    let b = r.sys.get(o..(o + max.min(4096)).min(r.sys.len())).unwrap_or(&[]);
                    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                    json!(String::from_utf8_lossy(&b[..end]))
                }
                None => serde_json::Value::Null,
            },
            _ => json!({"type": m.ty, "sub": m.sub, "hex": r.sys.get(p..p + 16).map(|b| b.iter().map(|x| format!("{x:02x}")).collect::<String>())}),
        })
    }

    /// Every field of one object, recursing into embedded structs, pointers
    /// and arrays (depth-limited). Field names are joaat hashes unless known.
    pub fn object(&self, hash: u32, o: usize, depth: u32) -> Option<serde_json::Value> {
        let sd = self.structs.get(&hash)?;
        if depth > 16 {
            return Some(serde_json::json!({"struct": format!("{hash:08x}"), "depth_limit": true}));
        }
        let mut out = serde_json::Map::new();
        out.insert("_struct".into(), serde_json::json!(format!("{hash:08x}")));
        if let Some(n) = struct_name(hash) {
            out.insert("_struct_name".into(), serde_json::json!(n));
        }
        for m in &sd.members {
            if m.hash == 0x100 {
                continue;
            }
            if let Some(v) = self.value(sd, m, o + m.off as usize, depth) {
                out.insert(field_name(m.hash), v);
            }
        }
        Some(serde_json::Value::Object(out))
    }

    /// One member of an object (by member name hash), decoded like
    /// [`Meta::object`] does: arrays, pointers and embedded structs followed.
    pub fn member_value(&self, hash: u32, o: usize, member: u32) -> Option<serde_json::Value> {
        let sd = self.structs.get(&hash)?;
        let m = sd.members.iter().find(|m| m.hash == member)?;
        self.value(sd, m, o + m.off as usize, 0)
    }

    /// Schema: struct hash -> size, instance count, members as hash:type:extra.
    pub fn schema(&self) -> serde_json::Value {
        let mut v = serde_json::Map::new();
        for (h, sd) in &self.structs {
            let mut s = serde_json::json!({"size": sd.size, "count": self.objects(*h).len(),
                "members": sd.members.iter().map(|m| format!("{:08x}:{:02x}:{:08x}", m.hash, m.ty, m.extra)).collect::<Vec<_>>()});
            if let Some(n) = struct_name(*h) {
                s["name"] = serde_json::json!(n);
            }
            v.insert(format!("{h:08x}"), s);
        }
        serde_json::Value::Object(v)
    }

    /// The root object (header +0x4A) decoded in full, arrays and pointers
    /// followed through pool references.
    pub fn dump(&self) -> serde_json::Value {
        let roots: Vec<_> = self.root_block().and_then(|(h, o)| self.object(h, o, 0)).into_iter().collect();
        let mut v = serde_json::json!({"schema": self.schema(), "roots": roots});
        let en = self.enum_names();
        if !en.is_empty() {
            v["enum_names"] = serde_json::json!(en);
        }
        v
    }

    /// Value names of the enum members (types 0x60-0x62, and arrays of
    /// them) of this resource's structs, by field name: value -> name.
    /// Only values named in the executable's enum tables are listed, so a
    /// value missing here is unknown, not invalid.
    pub fn enum_names(&self) -> BTreeMap<String, BTreeMap<i64, &'static str>> {
        let mut out: BTreeMap<String, BTreeMap<i64, &'static str>> = BTreeMap::new();
        for sd in self.structs.values() {
            for m in &sd.members {
                if m.hash == 0x100 {
                    continue;
                }
                // an array's element is the member its `reference` points at
                let el = if matches!(m.ty, 0x50..=0x5f) { sd.members.get(m.reference as usize) } else { Some(m) };
                let Some(el) = el.filter(|e| matches!(e.ty, 0x60..=0x62)) else { continue };
                let Some(vals) = enum_values().get(&el.extra) else { continue };
                out.entry(field_name(m.hash)).or_default().extend(vals.iter().map(|(&v, &n)| (v, n)));
            }
        }
        out
    }

    /// Every instance of every struct (hash 0x100 and up), decoded in full,
    /// by struct hash. Some resources keep their records in blocks the root
    /// does not point to (they are referenced by index or hash), so the root
    /// dump alone misses them. At most `cap` instances per struct.
    pub fn instances(&self, cap: usize) -> serde_json::Value {
        let mut v = serde_json::Map::new();
        for h in self.structs.keys().filter(|&&h| h >= 0x100) {
            let objs: Vec<_> = self.objects(*h).into_iter().take(cap).filter_map(|o| self.object(*h, o, 0)).collect();
            if !objs.is_empty() {
                v.insert(format!("{h:08x}"), serde_json::Value::Array(objs));
            }
        }
        serde_json::Value::Object(v)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Entity {
    /// joaat of the archetype (model) name.
    pub archetype: u32,
    /// Resolved model name, when an indexed drawable/fragment has that hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Entry id of that model (first match).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// RAGE world space, Z-up, metres.
    pub position: [f32; 3],
    /// Quaternion x, y, z, w as stored. Like GTA V `.ymap`s this is the
    /// **inverse** of the entity's rotation; see `rotation_world`.
    pub rotation: [f32; 4],
    /// Entity rotation in RAGE world space (conjugate of `rotation`).
    pub rotation_world: [f32; 4],
    /// Ready-made glTF (Y-up) node transform for this entity's model.glb:
    /// translation (x, z, -y), rotation (x, y, z, w), scale.
    pub gltf: GltfTransform,
    pub scale_xy: f32,
    pub scale_z: f32,
    pub flags: u32,
    /// 64-bit hash at +8 (unknown function; differs per archetype).
    pub hash64: String,
    pub guid: String,
    /// Tint palette row of this instance (field `3c852527`; absent = 0):
    /// the colour is palette entry `tint_row * row_length + COLOR0.b`, with
    /// the row layout from the model's model.json. Rows past the last one
    /// clamp to it (the prop mesh shader takes `min(tint_row, rows - 1)`).
    #[serde(skip_serializing_if = "is_zero")]
    pub tint_row: u32,
    /// `false` when the placement should not be drawn by default (proxy,
    /// placeholder); absent otherwise. See `render_reason`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render: Option<bool>,
    /// Why `render` is false (see `render_hint`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_reason: Option<&'static str>,
    /// LOD hierarchy, decoded from the hashed fields (see `entity_lod`).
    pub lod: serde_json::Value,
    /// Every other scalar field, keyed by field name or hash.
    pub fields: BTreeMap<String, serde_json::Value>,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// Why a placement should not be rendered by default, or None.
///
/// Rules from our own data (checked by exporting examples):
/// * `placeholder`: no model resolved for the archetype.
/// * `vfx_proxy`: effect markers (`*vfx*proxy*`, `vfx_dummy*`); one is a
///   119 m box, others are 10-20 cm stubs.
/// * `interior_proxy`: interior-shell stand-ins (`*_pxy`, `*shell_proxy`,
///   `int_*_proxy`); they duplicate the interior shells.
/// * `shadow_proxy`: `*_shadow` meshes (shadow casters).
/// * `light_proxy`: `*_lit_proxy` (zero-size geometry).
/// Shop items named `gmt_*_proxy_*` and cables named `*_occl_*` are real
/// render geometry and stay visible.
pub fn render_hint(name: Option<&str>, has_model: bool) -> Option<&'static str> {
    if !has_model {
        return Some("placeholder");
    }
    let n = name.unwrap_or("").to_ascii_lowercase();
    if n.contains("vfx") && (n.contains("proxy") || n.contains("dummy")) {
        return Some("vfx_proxy");
    }
    if n.ends_with("_lit_proxy") {
        return Some("light_proxy");
    }
    if n.ends_with("_pxy") || n.ends_with("shell_proxy") || (n.starts_with("int_") && n.contains("_proxy")) {
        return Some("interior_proxy");
    }
    if n.ends_with("_shadow") {
        return Some("shadow_proxy");
    }
    None
}

/// LOD fields of an entity (GTA V `CEntityDef` member names, confirmed by
/// their case-sensitive joaat): `6c8f1715` lodLevel, `d89231bd`
/// parentIndex, `e74c3c23` parentInstance, `a687ac89` numChildren,
/// `bf74dfa7` lodDist (-1 = the archetype's lodDist), `ca974bcd`
/// childLodDist, `2691f019` priorityLevel.
/// The level and priority value names come from the executable's enum
/// tables (`examples/exeschema.rs`): rage__eLodType (`4b5acc2f`, HD 0,
/// LOD 1, SLOD1-3 2-4, ORPHANHD 5, SLOD4 6) and the priority enum
/// (`0701af5f`, PRI_REQUIRED 0, PRI_OPTIONAL_HIGH 1, PRI_OPTIONAL_MEDIUM 2,
/// PRI_OPTIONAL_LOW 3).
/// With `parentInstance` = -1, `parentIndex` indexes the entities of the
/// parent map file (named by `parent_hash` when set). With
/// `parentInstance` >= 0 the parent is that instance of a prop batch in
/// another map file (LOD slot = entity count + batch index). `sixng
/// lod-links` resolves both; placements then carry `lod.parent`.
pub fn entity_lod(f: &BTreeMap<String, serde_json::Value>) -> serde_json::Value {
    let g = |h: u32| f.get(&field_name(h)).or_else(|| f.get(&format!("{h:08x}")));
    let i = |h: u32| g(h).and_then(|v| v.as_i64());
    let level = i(0x6c8f_1715);
    let level_name = match level {
        Some(0) => "HD",
        Some(1) => "LOD",
        Some(2) => "SLOD1",
        Some(3) => "SLOD2",
        Some(4) => "SLOD3",
        Some(5) => "ORPHAN_HD",
        Some(6) => "SLOD4",
        _ => "unknown",
    };
    let instance = i(0xe74c_3c23).filter(|&v| v >= 0);
    let linked = i(0xe74c_3c23) == Some(-1);
    let parent = if linked { i(0xd892_31bd).filter(|&v| v >= 0) } else { None };
    serde_json::json!({
        "level": level_name,
        "parent_index": parent,
        "parent_instance": instance,
        "parent_link": if instance.is_some() { "batch_instance" } else { "parent_file" },
        "num_children": i(0xa687_ac89),
        "lod_dist": g(0xbf74_dfa7),
        "child_lod_dist": g(0xca97_4bcd),
        "priority": i(0x2691_f019),
        "priority_name": i(0x2691_f019).map(|p| match p {
            0 => "REQUIRED",
            1 => "OPTIONAL_HIGH",
            2 => "OPTIONAL_MEDIUM",
            3 => "OPTIONAL_LOW",
            _ => "unknown",
        }),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct GltfTransform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

pub fn gltf_transform(p: [f32; 3], q_stored: [f32; 4], sxy: f32, sz: f32) -> GltfTransform {
    let [x, y, z, w] = q_stored;
    // world = conjugate(stored) = (-x, -y, -z, w); Z-up -> Y-up maps a
    // vector part (a, b, c) to (a, c, -b).
    GltfTransform { translation: [p[0], p[2], -p[1]], rotation: [-x, -z, y, w], scale: [sxy, sz, sxy] }
}

#[derive(Debug, Clone, Serialize)]
pub struct MapFile {
    pub name_hash: u32,
    pub parent_hash: u32,
    pub fields: BTreeMap<String, serde_json::Value>,
    pub entities: Vec<Entity>,
    /// Struct hash → (instance count, size) for everything in the file.
    pub contents: BTreeMap<String, (usize, u32)>,
}

pub fn map_file(r: &Resource, names: &HashMap<u32, (String, String)>) -> Option<MapFile> {
    let m = Meta::parse(r)?;
    let mut contents = BTreeMap::new();
    for (&h, sd) in &m.structs {
        let n = m.objects(h).len();
        if n > 0 {
            contents.insert(format!("{h:08x}"), (n, sd.size));
        }
    }
    let ents = m.objects(ENTITY);
    let root = m.objects(MAPDATA);
    if ents.is_empty() && root.is_empty() {
        return None;
    }
    let (name_hash, parent_hash, fields) = match root.first() {
        Some(&o) => (r.u32(o + 8), r.u32(o + 0x68), m.scalars(MAPDATA, o, &[8, 0x68])),
        None => (0, 0, BTreeMap::new()),
    };
    let entities = ents
        .iter()
        .map(|&o| {
            let a = r.u32(o + 0x44);
            let fields = m.scalars(ENTITY, o, &[8, 0x10, 0x20, 0x38, 0x44, 0x48, 0x54, 0x58]);
            let hit = names.get(&a);
            let position = [r.f32(o + 0x38), r.f32(o + 0x3C), r.f32(o + 0x40)];
            let rotation = [r.f32(o + 0x10), r.f32(o + 0x14), r.f32(o + 0x18), r.f32(o + 0x1C)];
            let (scale_xy, scale_z) = (r.f32(o + 0x54), r.f32(o + 0x58));
            let reason = render_hint(hit.map(|x| x.0.as_str()), hit.is_some());
            Entity {
                render: reason.map(|_| false),
                render_reason: reason,
                archetype: a,
                name: hit.map(|x| x.0.clone()),
                model_id: hit.map(|x| x.1.clone()),
                position,
                rotation,
                rotation_world: [-rotation[0], -rotation[1], -rotation[2], rotation[3]],
                gltf: gltf_transform(position, rotation, scale_xy, scale_z),
                scale_xy,
                scale_z,
                flags: r.u32(o + 0x48),
                hash64: format!("{:016x}", r.u64(o + 8)),
                guid: format!("{:016x}", r.u64(o + 0x20)),
                tint_row: fields.get("3c852527").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                lod: entity_lod(&fields),
                fields,
            }
        })
        .collect();
    Some(MapFile { name_hash, parent_hash, fields, entities, contents })
}

/// Interior (MLO) archetype, GTA V `CMloArchetypeDef` equivalent.
pub const MLO_ARCHETYPE: u32 = 0x1050_6455;
/// Interior instance placed in a map file, GTA V `CMloInstanceDef`
/// equivalent (entity prefix, 592 bytes).
pub const MLO_INSTANCE: u32 = 0x09cc_28be;

#[derive(Debug, Clone, Serialize)]
pub struct Mlo {
    /// joaat of the interior archetype name (+0x74).
    pub name: u32,
    /// Indices into this file's entity list (`objects(ENTITY)` order) of the
    /// interior's entities (pointer array at +0xC0), in interior space.
    pub entities: Vec<usize>,
    /// Entities of the interior's entity sets (`d6a799f9`, optional
    /// interior states, array at +0xF0; each set's entity pointers at
    /// +0x18), same index space.
    pub set_entities: Vec<usize>,
    /// Archetype bbox min/max (+0x40 / +0x4C), interior space.
    pub bbox: [[f32; 3]; 2],
    /// Rooms (`0b1811f1`): (name hash, file entity indices of the room's
    /// attachedObjects). attachedObjects index the interior's entity list.
    pub rooms: Vec<(u32, Vec<usize>)>,
    /// Entity sets: (name hash, file entity indices), in set order.
    pub sets: Vec<(u32, Vec<usize>)>,
}

/// Interior archetypes in one meta resource.
pub fn mlos(m: &Meta) -> Vec<Mlo> {
    let r = m.r;
    let ents: HashMap<usize, usize> = m.objects(ENTITY).into_iter().enumerate().map(|(i, o)| (o, i)).collect();
    m.objects(MLO_ARCHETYPE)
        .into_iter()
        .map(|o| {
            let ptrs = |at: usize| -> Vec<usize> {
                let n = r.u16(at + 8) as usize;
                m.ptr(r.u64(at))
                    .map(|(_, base)| (0..n.min(100_000)).filter_map(|i| m.ptr(r.u64(base + 8 * i)).and_then(|(_, eo)| ents.get(&eo).copied())).collect())
                    .unwrap_or_default()
            };
            let entities = ptrs(o + 0xC0);
            let mut set_entities = Vec::new();
            let mut sets = Vec::new();
            let sz = m.structs.get(&0xd6a7_99f9).map(|x| x.size as usize).unwrap_or(0);
            if let (Some((_, base)), true) = (m.ptr(r.u64(o + 0xF0)), sz > 0) {
                for k in 0..(r.u16(o + 0xF8) as usize).min(1000) {
                    let es = ptrs(base + k * sz + 0x18);
                    set_entities.extend(&es);
                    sets.push((r.u32(base + k * sz), es));
                }
            }
            // Rooms via the generic decoder (field order differs per build).
            let get = |v: &serde_json::Value, h: u32| v.get(field_name(h)).or_else(|| v.get(format!("{h:08x}"))).cloned();
            let rooms = m
                .object(MLO_ARCHETYPE, o, 6)
                .and_then(|v| get(&v, 0xcd20_d7a8))
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .map(|rm| {
                    let name = get(rm, 0xe6e1_7f14).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                    let att = get(rm, 0x8e05_312c)
                        .and_then(|x| x.as_array().cloned())
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|i| i.as_u64().and_then(|i| entities.get(i as usize).copied()))
                        .collect();
                    (name, att)
                })
                .collect();
            let v3 = |a: usize| [r.f32(a), r.f32(a + 4), r.f32(a + 8)];
            Mlo { name: r.u32(o + 0x74), entities, set_entities, bbox: [v3(o + 0x40), v3(o + 0x4C)], rooms, sets }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct MloInstance {
    pub archetype: u32,
    pub position: [f32; 3],
    /// Instance rotation x, y, z, w (stored as-is, not inverted).
    pub rotation_world: [f32; 4],
    pub gltf: GltfTransform,
}

/// Interior instances in one map file.
pub fn mlo_instances(m: &Meta) -> Vec<MloInstance> {
    let r = m.r;
    m.objects(MLO_INSTANCE)
        .into_iter()
        .map(|o| {
            let p = [r.f32(o + 0x38), r.f32(o + 0x3C), r.f32(o + 0x40)];
            let q = [r.f32(o + 0x10), r.f32(o + 0x14), r.f32(o + 0x18), r.f32(o + 0x1C)];
            // Unlike entities, interior instances store the rotation itself
            // (not its inverse), as in GTA V; checked by fitting an
            // interior's collision into its exterior shell.
            MloInstance { archetype: r.u32(o + 0x44), position: p, rotation_world: q, gltf: gltf_transform(p, [-q[0], -q[1], -q[2], q[3]], 1.0, 1.0) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_types_from_the_parser_tables() {
        // one struct: u32 enum, 6-bit set (32-bit words), 40-bit set,
        // 8-byte vec2, s32, a string pointer into block 2, then u64
        let mut sys = vec![0u8; 0x80];
        sys[0..4].copy_from_slice(&5u32.to_le_bytes());
        sys[4..8].copy_from_slice(&0x2au32.to_le_bytes());
        sys[8..16].copy_from_slice(&0x0000_0081_0000_0003u64.to_le_bytes());
        sys[16..20].copy_from_slice(&1.5f32.to_le_bytes());
        sys[20..24].copy_from_slice(&(-2.0f32).to_le_bytes());
        sys[24..28].copy_from_slice(&(-7i32).to_le_bytes());
        sys[32..40].copy_from_slice(&2u64.to_le_bytes());
        sys[40..48].copy_from_slice(&u64::MAX.to_le_bytes());
        sys[0x60..0x63].copy_from_slice(b"ab\0");
        let r = Resource { sys, gfx: vec![], gfx_base: 0 };
        let mem = |hash, off, ty, reference| Member { hash, off, ty, sub: 0, reference, extra: 0 };
        let sd = StructDef { hash: 0x1234, size: 48, members: vec![mem(1, 0, 0x62, 0), mem(2, 4, 0x65, 6), mem(3, 8, 0x65, 40), mem(4, 16, 0x30, 0), mem(5, 24, 0x14, 0), mem(6, 32, 0x42, 0), mem(7, 40, 0x17, 0)] };
        let m = Meta { r: &r, structs: [(0x1234, sd)].into_iter().collect(), blocks: vec![(0x1234, 48, 0), (0x10, 3, 0x60)], pso: false };
        let v = m.scalars_by_hash(0x1234, 0, &[]);
        assert_eq!(v[&1], serde_json::json!(5));
        // a 6-bit set is one 32-bit word: the neighbouring field is not read
        assert_eq!(v[&2], serde_json::json!("0000002a"));
        assert_eq!(v[&3], serde_json::json!("0000008100000003"));
        assert_eq!(v[&4], serde_json::json!([1.5, -2.0]));
        assert_eq!(v[&5], serde_json::json!(-7));
        assert_eq!(v[&6], serde_json::json!("ab"));
        assert_eq!(v[&7], serde_json::json!(u64::MAX));
        assert!(is_scalar_type(0x01) && !is_scalar_type(0x05) && !is_scalar_type(0x52));
    }

    #[test]
    fn pso_map_reads_every_group() {
        let m = |hash: u32, off: u32, ty: u8, reference: u16, extra: u32| Member { hash, off, ty, sub: 0, reference, extra };
        let mut structs = HashMap::new();
        structs.insert(0x100, StructDef { hash: 0x100, size: 8, members: vec![m(0x6098_a50e, 0, 0x4a, 0, 0), m(0x063f_a3f2, 4, 0x21, 0, 0)] });
        structs.insert(0x101, StructDef { hash: 0x101, size: 144, members: vec![m(0x100, 0, 0x11, 0, 0), m(0xb879_4c89, 0, 0x50, 0, 16), m(0x100, 0, 0x05, 0, 0x100), m(0x93c4_95af, 16, 0x50, 2, 16)] });
        structs.insert(0xaaaa, StructDef { hash: 0xaaaa, size: 16, members: vec![m(0x1652_fce8, 0, 0x75, 0, 0)] });
        let mut sys = vec![0u8; 16 + 288];
        sys[0..4].copy_from_slice(&2u32.to_le_bytes()); // groups
        sys[4..8].copy_from_slice(&2u32.to_le_bytes()); // entries
        sys[8..16].copy_from_slice(&2u64.to_le_bytes()); // block 2, offset 0
        for g in 0..2 {
            sys[16 + 144 * g..16 + 144 * g + 16].fill(0x80);
        }
        let slot = |sys: &mut Vec<u8>, g: usize, i: usize, k: u32, v: f32| {
            sys[16 + 144 * g + i] = 0x05;
            let o = 16 + 144 * g + 16 + 8 * i;
            sys[o..o + 4].copy_from_slice(&k.to_le_bytes());
            sys[o + 4..o + 8].copy_from_slice(&v.to_le_bytes());
        };
        slot(&mut sys, 0, 0, 7, 1.5);
        slot(&mut sys, 1, 3, 9, 2.5);
        let r = Resource { sys, gfx: Vec::new(), gfx_base: 0 };
        let meta = Meta { r: &r, structs, blocks: vec![(0xaaaa, 16, 0), (0x101, 288, 16)], pso: true };
        let v = meta.object(0xaaaa, 0, 0).unwrap();
        assert_eq!(v[field_name(0x1652_fce8)], serde_json::json!([{"key": 7, "value": 1.5}, {"key": 9, "value": 2.5}]));
    }

    #[test]
    fn entity_lod_names() {
        let mut f = BTreeMap::new();
        f.insert("6c8f1715".to_string(), serde_json::json!(5));
        f.insert("2691f019".to_string(), serde_json::json!(2));
        let l = entity_lod(&f);
        assert_eq!(l["level"], "ORPHAN_HD");
        assert_eq!(l["priority"], 2);
        assert_eq!(l["priority_name"], "OPTIONAL_MEDIUM");
        assert!(entity_lod(&BTreeMap::new())["priority_name"].is_null());
    }

    #[test]
    fn enum_value_table() {
        // rage__eLodType and the entity priority enum
        assert_eq!(enum_value_name(0x4b5a_cc2f, 5), Some("LODTYPES_DEPTH_ORPHANHD"));
        assert_eq!(enum_value_name(0x4b5a_cc2f, 6), Some("LODTYPES_DEPTH_SLOD4"));
        assert_eq!(enum_value_name(0x0701_af5f, 0), Some("PRI_REQUIRED"));
        assert_eq!(enum_value_name(0x0701_af5f, 99), None);
        // from the prefix brute force
        assert_eq!(enum_value_name(0x0100_f80e, 2), Some("TYPE_QUADRATIC_OUT"));
        assert_eq!(enum_value_name(0x391d_8bab, 10), Some("RC_CarPark"));
        // the key enum follows the Windows virtual-key codes; pad buttons sit at 0x20000
        assert_eq!(enum_value_name(0xacc0_e29b, 112), Some("KEY_F1"));
        assert_eq!(enum_value_name(0xacc0_e29b, 0x2_0009), Some("L3"));
        assert!(enum_values().values().map(|v| v.len()).sum::<usize>() > 4000);
        for (h, vals) in enum_values() {
            assert_ne!(*h, 0);
            for n in vals.values() {
                assert!(!n.is_empty() && !n.contains(char::is_whitespace));
            }
        }
    }

    #[test]
    fn struct_names() {
        assert_eq!(struct_name(MAPDATA), Some("CMapData"));
        assert_eq!(struct_name(ENTITY), Some("CEntityDef"));
        assert_eq!(struct_name(0x0b18_11f1), Some("CMloRoomDef"));
        assert_eq!(struct_name(0xf14a_15be), Some("rage__fwInstancedMapData"));
        assert_eq!(struct_name(0xed86_f522), Some("rage__fwGrassInstanceListDef__InstanceData"));
        assert_eq!(STRUCT_NAMES.len(), 16);
        // the vehicle configuration struct does not resolve
        assert_eq!(struct_name(0x5101_9114), None);
    }
}
