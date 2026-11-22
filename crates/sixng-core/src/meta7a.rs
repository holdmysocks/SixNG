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

    /// Resolve a pointer field: a real system pointer (`0x5xxxxxxx`) or a
    /// pool reference into the instance blocks: u64 with the 1-based block
    /// index in the low 12 bits and the byte offset in bits 20+.
    pub fn ptr(&self, v: u64) -> Option<(u32, usize)> {
        if !self.pso {
            if let Some(o) = self.r.meta_off(v) {
                return Some((0, o));
            }
        }
        let bi = (v & 0xFFF) as usize;
        let off = (v >> 20) as usize;
        let &(h, size, o) = self.blocks.get(bi.checked_sub(1)?)?;
        (off < size as usize && o != usize::MAX).then_some((h, o + off))
    }

    /// Index (1-based) of the root instance block, from the header (+0x4A).
    pub fn root_block(&self) -> Option<(u32, usize)> {
        let &(h, _, o) = self.blocks.get((self.r.u16(0x4A) as usize).checked_sub(1)?)?;
        (o != usize::MAX).then_some((h, o))
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
        let mut out = BTreeMap::new();
        let Some(sd) = self.structs.get(&hash) else { return out };
        let r = self.r;
        for m in &sd.members {
            if m.hash == 0x100 || skip.contains(&m.off) {
                continue;
            }
            let p = o + m.off as usize;
            let f = |i: usize| serde_json::json!(r.f32(p + 4 * i));
            let v = match m.ty {
                0x10 => serde_json::json!(r.sys.get(p).map(|&x| x as i8)),
                0x11 | 0x60 => serde_json::json!(r.sys.get(p).copied()),
                0x12 => serde_json::json!(r.u16(p) as i16),
                0x13 => serde_json::json!(r.u16(p)),
                0x15 | 0x4a => serde_json::json!(r.u32(p)),
                0x21 => f(0),
                0x32 | 0x33 => serde_json::json!([f(0), f(1), f(2)]),
                0x34 => serde_json::json!([f(0), f(1), f(2), f(3)]),
                0x08 | 0x4f => serde_json::json!(format!("{:016x}", r.u64(p))),
                _ => continue,
            };
            out.insert(field_name(m.hash), v);
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
            0x14 | 0x15 | 0x4a | 0x21 => 4,
            0x31 | 0x32 | 0x33 | 0x34 => 16,
            0x08 | 0x4f | 0x07 | 0x4d => 8,
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
            0x21 => f(0),
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
            // Map (PSO): u32 count, u32 capacity, u64 pointer to a hash table
            // struct (control bytes `b8794c89`, slots `93c495af`); only the
            // occupied slots ({key, value}) are returned.
            0x75 => {
                let Some((h, o)) = self.ptr(r.u64(p + 8)) else { return Some(json!([])) };
                let t = self.object(h, o, depth + 1)?;
                let (Some(ctrl), Some(slots)) = (t.get(field_name(0xb879_4c89)).and_then(|x| x.as_array()), t.get(field_name(0x93c4_95af)).and_then(|x| x.as_array())) else { return Some(t) };
                json!(ctrl
                    .iter()
                    .zip(slots)
                    .filter(|(c, _)| c.as_u64().is_some_and(|c| c < 0x80))
                    .map(|(_, sl)| json!({"key": sl.get(field_name(0x6098_a50e)), "value": sl.get(field_name(0x063f_a3f2))}))
                    .collect::<Vec<_>>())
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
            // Bit set / flags (PSO, 16 bytes): the first 64 bits.
            0x65 => json!(format!("{:016x}", r.u64(p))),
            // Tagged value (PSO, 16 bytes): u32 tag, then payload.
            0x62 => json!({"tag": r.u32(p), "u32": [r.u32(p + 4), r.u32(p + 8), r.u32(p + 12)], "f32": [r.f32(p + 4), r.f32(p + 8), r.f32(p + 12)]}),
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
        serde_json::json!({"schema": self.schema(), "roots": roots})
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
    /// the row layout from the model's model.json (rows past the last one
    /// are assumed to clamp to it).
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
