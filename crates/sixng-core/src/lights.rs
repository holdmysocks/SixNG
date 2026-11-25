//! Light emitters in the self-describing meta resources (`levels.rpf`).
//!
//! Like GTA V's `CExtensionDefLightEffect` / `CLightAttrDef`, lights hang off
//! archetypes (and some entities) as extensions:
//!
//! ```text
//! archetype 82d6fc83 (184 B): +0x74 u32 name (joaat), +0x90 array of
//!     extension pointers (pool references)
//! entity    ce501483 (144 B): +0x28 array of extension pointers
//! light extension 27922c43 (80 B): +0x08 u32 name, +0x10 vec3 offset,
//!     +0x20 array of lights (f54b227b), +0x30 second light array
//! light f54b227b (672 B): +0x120 vec3 position (local to the archetype),
//!     +0x1A0 / +0x1AC direction / tangent, +0x288 u8[3] RGB colour
//!     (+0x282, +0x285 two more colours), other fields by hash
//! ```
//! Pool reference: u64, 1-based block index in bits 0..12, byte offset in
//! bits 20+ (see `Meta::ptr`).
use crate::meta7a::{Meta, ENTITY};
use crate::resource::Resource;
use serde::Serialize;
use std::collections::BTreeMap;

pub const ARCHETYPE: u32 = 0x82d6_fc83;
pub const LIGHT_EXT: u32 = 0x2792_2c43;
pub const LIGHT: u32 = 0xf54b_227b;

#[derive(Debug, Clone, Serialize)]
pub struct Light {
    /// Local position (archetype / entity space, Z-up, metres).
    pub position: [f32; 3],
    /// u8[3] at +0x288 as RRGGBB: the light colour (warm orange `ff8026`
    /// on street lamps, cool `d8faff`, warm white `fff5c3`, ...).
    pub colour: String,
    /// u8[3] at +0x282 and +0x285: two more colours (mostly `ffffff`;
    /// volume / corona tint?).
    pub colour2: String,
    pub colour3: String,
    /// Unit vectors at +0x1A0 / +0x1AC (direction and tangent, as in GTA V
    /// `CLightAttrDef`).
    pub direction: [f32; 3],
    pub tangent: [f32; 3],
    /// vec4 at +0x00 (unknown).
    pub v0: [f32; 4],
    /// Field names are the case-sensitive joaat of the GTA V `CLightAttr`
    /// names; the hashed originals stay in `fields`.
    /// `lightType` (`1cbbbe30`): 1 point, 2 spot, 4 capsule, 32 (rare, other).
    pub light_type: u32,
    /// `intensity` (`efcd993d`, median 50).
    pub intensity: f32,
    /// `falloff` (`cc6806e8`): the range in metres (spot median 6).
    pub range: f32,
    /// `falloffExponent` (`2bb342ba`; 8 = GTA V default).
    pub falloff_exponent: f32,
    /// `coronaSize`, `coronaIntensity`, `volIntensity`, `flashiness`.
    pub corona_size: f32,
    pub corona_intensity: f32,
    pub vol_intensity: f32,
    pub flashiness: u8,
    /// `lightFadeDistance` / `shadowFadeDistance` (u8 at +0x28D / +0x28E).
    pub light_fade_distance: u8,
    pub shadow_fade_distance: u8,
    /// `455c3938` / `f8d9e344`, degrees (inner <= outer for 98 % of spots).
    pub cone_inner: f32,
    pub cone_outer: f32,
    /// `8609d52c`: 24-bit hour mask, bit h = on at hour h (`ffffff` always).
    pub time_flags: String,
    /// On between 07:00 and 19:59 at no hour (night-only lamp).
    pub night_only: bool,
    /// Every scalar field by hash.
    pub fields: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LightSet {
    /// Owner: "archetype" (name = joaat of the model name) or "entity".
    pub owner: &'static str,
    pub archetype: u32,
    /// For entity-owned sets: index of the entity in its map file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_index: Option<usize>,
    pub ext_name: u32,
    pub ext_offset: [f32; 3],
    pub lights: Vec<Light>,
}

fn num(f: &BTreeMap<u32, serde_json::Value>, k: u32) -> f64 {
    f.get(&k).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

fn rgb(r: &Resource, o: usize) -> String {
    r.sys.get(o..o + 3).map(|b| format!("{:02x}{:02x}{:02x}", b[0], b[1], b[2])).unwrap_or_default()
}

/// One light record (`f54b227b`, 672 bytes) at sys offset `o`. Scalar
/// fields are picked by member hash, never by resolved name.
fn light_at(m: &Meta, o: usize) -> Light {
    let r = m.r;
    let f = m.scalars_by_hash(LIGHT, o, &[0, 0x120]);
    Light {
        position: [r.f32(o + 0x120), r.f32(o + 0x124), r.f32(o + 0x128)],
        colour: rgb(r, o + 0x288),
        colour2: rgb(r, o + 0x282),
        colour3: rgb(r, o + 0x285),
        direction: [r.f32(o + 0x1A0), r.f32(o + 0x1A4), r.f32(o + 0x1A8)],
        tangent: [r.f32(o + 0x1AC), r.f32(o + 0x1B0), r.f32(o + 0x1B4)],
        v0: [r.f32(o), r.f32(o + 4), r.f32(o + 8), r.f32(o + 12)],
        light_type: num(&f, 0x1cbb_be30) as u32,
        intensity: num(&f, 0xefcd_993d) as f32,
        range: num(&f, 0xcc68_06e8) as f32,
        falloff_exponent: num(&f, 0x2bb3_42ba) as f32,
        corona_size: r.f32(o + 0x168),
        corona_intensity: r.f32(o + 0x174),
        vol_intensity: r.f32(o + 0x154),
        flashiness: r.sys[o + 0x28B],
        light_fade_distance: r.sys[o + 0x28D],
        shadow_fade_distance: r.sys[o + 0x28E],
        cone_inner: num(&f, 0x455c_3938) as f32,
        cone_outer: num(&f, 0xf8d9_e344) as f32,
        time_flags: format!("{:06x}", r.u32(o + 0x144) & 0xFF_FFFF),
        night_only: { let t = r.u32(o + 0x144) & 0xFF_FFFF; t != 0 && t & 0x0F_FF80 == 0 },
        fields: f.into_iter().map(|(h, v)| (crate::meta7a::field_name(h), v)).collect(),
    }
}

fn lights_of_ext(m: &Meta, eo: usize) -> Vec<Light> {
    let r = m.r;
    let mut out = Vec::new();
    for arr in [0x20usize, 0x30] {
        let n = r.u16(eo + arr + 8) as usize;
        let Some((h, base)) = m.ptr(r.u64(eo + arr)) else { continue };
        if h != LIGHT || n == 0 || n > 10_000 {
            continue;
        }
        for k in 0..n {
            let o = base + k * 672;
            if o + 672 > r.sys.len() {
                break;
            }
            out.push(light_at(m, o));
        }
    }
    out
}

/// Light extensions reachable from an extension-pointer array at `at`.
fn ext_sets(m: &Meta, at: usize) -> Vec<(u32, [f32; 3], Vec<Light>)> {
    let r = m.r;
    let n = r.u16(at + 8) as usize;
    let Some((_, base)) = m.ptr(r.u64(at)) else { return vec![] };
    (0..n.min(256))
        .filter_map(|i| {
            let (h, eo) = m.ptr(r.u64(base + 8 * i))?;
            if h != LIGHT_EXT {
                return None;
            }
            let l = lights_of_ext(m, eo);
            (!l.is_empty()).then(|| (r.u32(eo + 8), [r.f32(eo + 0x10), r.f32(eo + 0x14), r.f32(eo + 0x18)], l))
        })
        .collect()
}

/// All light sets in one meta resource.
pub fn light_sets(r: &Resource) -> Vec<LightSet> {
    let Some(m) = Meta::parse(r) else { return vec![] };
    if !m.structs.contains_key(&LIGHT_EXT) {
        return vec![];
    }
    let mut v = Vec::new();
    for o in m.objects(ARCHETYPE) {
        for (en, eo, lights) in ext_sets(&m, o + 0x90) {
            v.push(LightSet { owner: "archetype", archetype: r.u32(o + 0x74), entity_index: None, ext_name: en, ext_offset: eo, lights });
        }
    }
    for (i, o) in m.objects(ENTITY).into_iter().enumerate() {
        for (en, eo, lights) in ext_sets(&m, o + 0x28) {
            v.push(LightSet { owner: "entity", archetype: r.u32(o + 0x44), entity_index: Some(i), ext_name: en, ext_offset: eo, lights });
        }
    }
    v
}

/// `CMapData` members (case-sensitive joaat of the GTA V names) that hold
/// the map file's timecycle modifier boxes and its LOD / distant light
/// arrays.
pub const TC_MODIFIERS: u32 = 0xaf9c_3bd9; // timeCycleModifiers (empty in every file)
/// The modifier volumes GTA VI uses instead: an array of `3bc4ac19`
/// (112 B): `name` (joaat of the modifier name), `center`, `orientation`
/// (always 0), `scale`, `fe27ffde` (the shape, [`tc_shape`]), `percentage`,
/// `range`, `startHour`, `endHour`, `854db4c2` (u32, 0 on 96 %) and
/// `rotation` (quaternion x, y, z, w, applied as stored: unlike entities it
/// is not the inverse).
pub const TC_BOXES: u32 = 0x8471_928b;

/// Shape of a modifier volume (`fe27ffde`) and its half extents from
/// `scale`. Boxes (469 of 586) store the full size; cylinders (103, axis
/// local z) and ellipsoids (14) store the radii. Checked on the map files
/// that hold one volume: their extents enclose a sphere of radius
/// |scale| / 2 around a box and |scale| around the other two shapes.
pub fn tc_shape(shape: u32, scale: [f32; 3]) -> (&'static str, [f32; 3]) {
    match shape {
        0x9fde_1ac6 => ("box", [scale[0] / 2.0, scale[1] / 2.0, scale[2] / 2.0]),
        0x27ac_c35b => ("cylinder", scale),
        0xe63a_0d71 => ("ellipsoid", scale),
        _ => ("unknown", scale),
    }
}

/// World-space bounds of a modifier volume: its local box of `half`
/// extents rotated by `rotation` (as stored) around `center`.
pub fn tc_bounds(center: [f32; 3], rotation: [f32; 4], half: [f32; 3]) -> [[f32; 3]; 2] {
    let mut b = [center, center];
    for k in 0..8 {
        let s = |i: usize| if k >> i & 1 == 1 { 1.0 } else { -1.0 };
        let v = rotate(rotation, [s(0) * half[0], s(1) * half[1], s(2) * half[2]]);
        for i in 0..3 {
            b[0][i] = b[0][i].min(center[i] + v[i]);
            b[1][i] = b[1][i].max(center[i] + v[i]);
        }
    }
    b
}
/// Interior room (`CMloRoomDef`) and its timecycle members.
pub const ROOM: u32 = 0x0b18_11f1;
pub const LOD_LIGHTS: u32 = 0x69c2_c0fa; // LODLightsSOA
pub const DISTANT_LIGHTS: u32 = 0xb019_9551; // DistantLODLightsSOA

/// The timecycle modifier boxes and LOD / distant lights of one map file,
/// decoded from its `CMapData` root. The light arrays are kept as stored:
/// one column per field (structure of arrays).
#[derive(Debug, Clone, Default)]
pub struct MapLightData {
    /// `CMapData` name, parent and flags (a LOD light file's parent is the
    /// name of its distant light file; distant light files have flag 2).
    pub name: u32,
    pub parent: u32,
    pub flags: u32,
    pub tc_modifiers: Vec<serde_json::Value>,
    /// Interior rooms: every scalar field (`timecycleName`,
    /// `secondaryTimecycleName`, `blend`, ...), by resolved name.
    pub rooms: Vec<serde_json::Value>,
    pub lod: Option<serde_json::Value>,
    pub distant: Option<serde_json::Value>,
}

/// Replace each {x, y, z} struct (`e2cbcfd4`, 12 bytes: the light arrays'
/// positions and directions) by `[x, y, z]`, recursively.
fn flatten_xyz(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Array(a) => a.iter_mut().for_each(flatten_xyz),
        serde_json::Value::Object(o) => {
            if o.get("_struct").and_then(|s| s.as_str()) == Some("e2cbcfd4") {
                let k = |h: u32| o.get(&crate::meta7a::field_name(h)).and_then(|x| x.as_f64()).unwrap_or(0.0);
                *v = serde_json::json!([k(0x9303_a5e5), k(0x8095_0108), k(0xb660_6c9e)]);
                return;
            }
            o.values_mut().for_each(flatten_xyz);
        }
        _ => {}
    }
}

fn nonempty(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(o) => o.iter().any(|(k, x)| k != "_struct" && nonempty(x)),
        _ => false,
    }
}

/// Decode the modifier boxes and LOD / distant light arrays of a map file
/// (`None` when its root is not `CMapData` or all three are empty).
pub fn map_light_data(m: &Meta) -> Option<MapLightData> {
    let (h, o) = m.root_block()?;
    if h != crate::meta7a::MAPDATA {
        let rooms: Vec<_> = m.objects(ROOM).into_iter().map(|ro| serde_json::Value::Object(m.scalars(ROOM, ro, &[]).into_iter().collect())).collect();
        return (!rooms.is_empty()).then(|| MapLightData { rooms, ..Default::default() });
    }
    let get = |f: u32| {
        let mut v = m.member_value(h, o, f)?;
        flatten_xyz(&mut v);
        nonempty(&v).then_some(v)
    };
    let u = |f: u32| m.member_value(h, o, f).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let d = MapLightData {
        name: u(0xe6e1_7f14),
        parent: u(0xa99c_4651),
        flags: u(0x67d2_6872),
        tc_modifiers: [TC_MODIFIERS, TC_BOXES].into_iter().filter_map(get).filter_map(|v| v.as_array().cloned()).flatten().collect(),
        rooms: m.objects(ROOM).into_iter().map(|ro| serde_json::Value::Object(m.scalars(ROOM, ro, &[]).into_iter().collect())).collect(),
        lod: get(LOD_LIGHTS),
        distant: get(DISTANT_LIGHTS),
    };
    (!d.tc_modifiers.is_empty() || !d.rooms.is_empty() || d.lod.is_some() || d.distant.is_some()).then_some(d)
}

/// Split a structure of arrays into one row per element: every array
/// member with `n` elements becomes a field of the rows, everything else
/// (arrays of other lengths, embedded structs) is returned separately.
pub fn soa_rows(v: &serde_json::Value, n: usize) -> (Vec<serde_json::Map<String, serde_json::Value>>, serde_json::Map<String, serde_json::Value>) {
    let mut rows = vec![serde_json::Map::new(); n];
    let mut rest = serde_json::Map::new();
    for (k, x) in v.as_object().into_iter().flatten() {
        match x.as_array() {
            Some(a) if a.len() == n && n > 0 => {
                for (r, e) in rows.iter_mut().zip(a) {
                    r.insert(k.clone(), e.clone());
                }
            }
            _ if k == "_struct" => {}
            _ => {
                rest.insert(k.clone(), x.clone());
            }
        }
    }
    (rows, rest)
}

/// Element count of a structure of arrays: the length of its member `f`.
pub fn soa_len(v: &serde_json::Value, f: u32) -> usize {
    v.get(crate::meta7a::field_name(f)).and_then(|x| x.as_array()).map_or(0, |a| a.len())
}

fn half(h: u16) -> f32 {
    let (s, e, f) = (if h & 0x8000 != 0 { -1.0f32 } else { 1.0 }, (h >> 10) & 0x1f, (h & 0x3ff) as f32);
    s * match e {
        0 => f / 1024.0 * 2f32.powi(-14),
        31 => f32::INFINITY,
        _ => (1.0 + f / 1024.0) * 2f32.powi(e as i32 - 15),
    }
}

/// One distant light in world space: the distant light record (`d`, a row
/// of `DistantLODLightsSOA`) joined with the record of the same index in
/// the paired LOD light file (`l`, a row of `LODLightsSOA`), with the
/// decoded fields under plain names and every stored column under
/// `distant_raw` / `lod_raw`.
///
/// Distant record: `position` (world), `94720dec` colour as u8 R, G, B and
/// a fourth byte (`i`, 254 or 255 on 70 % of lights), `intensity`,
/// `lightType` (as the placed lights), `flags` (u8), `hash`, and u16 fields
/// that read as half floats: `699dd4d9` (`time_multiplier`, 1.0 on 93 %,
/// 0 on 4.6 %; distant `flags` 2 and 7 occur only with 0), `fdafb23b`, and
/// `a13c79ef` (`random`: spread evenly over 0..100 and independent of every
/// other field, of the placed light it comes from, its hash and its
/// position: a per-light random value). None of the three is a copy of a
/// placed-light field. LOD record: `direction`, `falloff` (the range),
/// `falloffExponent`, cone angles as u8 = degrees x 255 / 180 (for
/// capsules, the outer one is the capsule extent), `8e6f0ba9` (two half
/// floats), `0e852fda` (two u16, the second equal to the first or one
/// more), and the u8 fields not identified yet.
pub fn distant_row(d: &serde_json::Map<String, serde_json::Value>, l: Option<&serde_json::Map<String, serde_json::Value>>) -> serde_json::Value {
    use crate::meta7a::field_name;
    let g = |m: &serde_json::Map<String, serde_json::Value>, h: u32| m.get(&field_name(h)).cloned().unwrap_or(serde_json::Value::Null);
    let n = |m: &serde_json::Map<String, serde_json::Value>, h: u32| m.get(&field_name(h)).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let c = n(d, 0x9472_0dec) as u32;
    let mut v = serde_json::json!({
        "p": g(d, 0x0116_6164),
        "colour": format!("{:02x}{:02x}{:02x}", c & 0xff, (c >> 8) & 0xff, (c >> 16) & 0xff),
        "i": c >> 24,
        "intensity": g(d, 0xefcd_993d),
        "type": g(d, 0x1cbb_be30),
        "flags": g(d, 0x67d2_6872),
        "hash": format!("{:08x}", n(d, 0x3e81_8018) as u32),
        "time_multiplier": half(n(d, 0x699d_d4d9) as u16),
        "half_fdafb23b": half(n(d, 0xfdaf_b23b) as u16),
        "half_a13c79ef": half(n(d, 0xa13c_79ef) as u16),
        "random": half(n(d, 0xa13c_79ef) as u16),
        "distant_raw": d,
    });
    if let Some(l) = l {
        let deg = |h: u32| n(l, h) * 180.0 / 255.0;
        v["dir"] = g(l, 0x34df_9828);
        v["range"] = g(l, 0xcc68_06e8);
        v["falloff_exponent"] = g(l, 0x2bb3_42ba);
        v["cone"] = serde_json::json!([deg(0x455c_3938), deg(0xbc76_acc0)]);
        let (a, b) = (n(l, 0x8e6f_0ba9) as u32, n(l, 0x0e85_2fda) as u32);
        v["halves_8e6f0ba9"] = serde_json::json!([half(a as u16), half((a >> 16) as u16)]);
        v["u16_0e852fda"] = serde_json::json!([b & 0xffff, b >> 16]);
        v["lod_raw"] = serde_json::Value::Object(l.clone());
    }
    v
}

/// Rotate `v` by quaternion `q` (x, y, z, w).
pub fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let [x, y, z, w] = q;
    let t = [2.0 * (y * v[2] - z * v[1]), 2.0 * (z * v[0] - x * v[2]), 2.0 * (x * v[1] - y * v[0])];
    [
        v[0] + w * t[0] + (y * t[2] - z * t[1]),
        v[1] + w * t[1] + (z * t[0] - x * t[2]),
        v[2] + w * t[2] + (x * t[1] - y * t[0]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta7a::{Member, StructDef};
    use crate::names::joaat_exact as joaat;

    /// The scalar fields come from the member hashes, whatever names the
    /// name tables give them (a resolved name once turned them all to 0).
    #[test]
    fn light_fields_by_hash() {
        let mut sys = vec![0u8; 672];
        let put = |sys: &mut Vec<u8>, o: usize, b: [u8; 4]| sys[o..o + 4].copy_from_slice(&b);
        put(&mut sys, 0x40, 2u32.to_le_bytes());
        put(&mut sys, 0x44, 64f32.to_le_bytes());
        put(&mut sys, 0x48, 2.5f32.to_le_bytes());
        put(&mut sys, 0x4C, 8f32.to_le_bytes());
        put(&mut sys, 0x50, 45f32.to_le_bytes());
        put(&mut sys, 0x54, 60f32.to_le_bytes());
        put(&mut sys, 0x144, 0x00ff_ff80u32.to_le_bytes());
        sys[0x288..0x28B].copy_from_slice(&[0xff, 0x80, 0x26]);
        let mem = |name: &str, off: u32, ty: u8| Member { hash: joaat(name), off, ty, sub: 0, reference: 0, extra: 0 };
        let members = vec![
            mem("lightType", 0x40, 0x15),
            mem("intensity", 0x44, 0x21),
            mem("falloff", 0x48, 0x21),
            mem("falloffExponent", 0x4C, 0x21),
            mem("coneInnerAngle", 0x50, 0x21),
            mem("coneOuterAngle", 0x54, 0x21),
            mem("timeFlags", 0x144, 0x15),
        ];
        assert_eq!(members[0].hash, 0x1cbb_be30);
        assert_eq!(members[1].hash, 0xefcd_993d);
        let r = Resource { sys, gfx: vec![], gfx_base: 0 };
        let structs = [(LIGHT, StructDef { hash: LIGHT, size: 672, members })].into_iter().collect();
        let m = Meta { r: &r, structs, blocks: vec![], pso: false };
        let l = light_at(&m, 0);
        assert_eq!(l.light_type, 2);
        assert_eq!(l.intensity, 64.0);
        assert_eq!(l.range, 2.5);
        assert_eq!(l.falloff_exponent, 8.0);
        assert_eq!((l.cone_inner, l.cone_outer), (45.0, 60.0));
        assert_eq!(l.colour, "ff8026");
        assert_eq!(l.time_flags, "ffff80");
        assert!(!l.night_only);
        assert_eq!(l.fields.len(), 7);
    }
    #[test]
    fn soa_to_rows() {
        let v = serde_json::json!({"_struct": "793441f4", "a": [1, 2, 3], "b": [4.0, 5.0, 6.0], "boxes": [7], "sub": {"x": []}});
        let (rows, rest) = soa_rows(&v, 3);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1]["a"], 2);
        assert_eq!(rows[2]["b"], 6.0);
        assert!(rest.contains_key("boxes") && rest.contains_key("sub") && !rest.contains_key("_struct"));
    }

    #[test]
    fn xyz_flattened() {
        let x = crate::meta7a::field_name(0x9303_a5e5);
        let y = crate::meta7a::field_name(0x8095_0108);
        let z = crate::meta7a::field_name(0xb660_6c9e);
        let mut v = serde_json::json!({"position": [{"_struct": "e2cbcfd4", x.as_str(): 1.0, y.as_str(): 2.0, z.as_str(): 3.0}]});
        flatten_xyz(&mut v);
        assert_eq!(v["position"][0], serde_json::json!([1.0, 2.0, 3.0]));
    }

    #[test]
    fn distant_and_lod_joined() {
        use crate::meta7a::field_name;
        let mut d = serde_json::Map::new();
        d.insert(field_name(0x0116_6164), serde_json::json!([1.0, 2.0, 3.0]));
        d.insert(field_name(0x9472_0dec), serde_json::json!(0xfebb_daffu32));
        d.insert(field_name(0x699d_d4d9), serde_json::json!(0x3c00));
        let mut l = serde_json::Map::new();
        l.insert(field_name(0x455c_3938), serde_json::json!(85));
        l.insert(field_name(0xbc76_acc0), serde_json::json!(255));
        let v = distant_row(&d, Some(&l));
        assert_eq!(v["colour"], "ffdabb");
        assert_eq!(v["i"], 0xfe);
        assert_eq!(v["time_multiplier"], 1.0);
        assert_eq!(v["cone"], serde_json::json!([60.0, 180.0]));
        assert_eq!(v["p"], serde_json::json!([1.0, 2.0, 3.0]));
        assert_eq!(half(0x4200), 3.0);
    }
    #[test]
    fn tc_volume_bounds() {
        // A box of full size 4 x 2 x 6 turned 90 degrees about X (y up).
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let (shape, half) = tc_shape(0x9fde_1ac6, [4.0, 2.0, 6.0]);
        assert_eq!((shape, half), ("box", [2.0, 1.0, 3.0]));
        let b = tc_bounds([10.0, 20.0, 30.0], [s, 0.0, 0.0, s], half);
        let r = |x: f32| (x * 1000.0).round() / 1000.0;
        assert_eq!(b[0].map(r), [8.0, 17.0, 29.0]);
        assert_eq!(b[1].map(r), [12.0, 23.0, 31.0]);
        assert_eq!(tc_shape(0x27ac_c35b, [3.0, 3.0, 1.0]), ("cylinder", [3.0, 3.0, 1.0]));
    }
}
