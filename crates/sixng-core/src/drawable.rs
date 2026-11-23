//! Drawable / fragment resources (".#dr", ".#ft", ...).
//!
//! Geometry lives in "mesh blobs" (graphics buffers starting with u32 0x4042):
//! a 0x80-byte header holding a vertex-stream table (slot k at +4k:
//! `offset_in_2_byte_units << 8 | format`), a position base (f32 x3 at +0x60)
//! and a quantisation step (f32 x3 at +0x70). Triangles live in a separate
//! "meshlet" buffer: descriptors, 7-bit local indices packed in u32s and a u16
//! vertex remap.
use crate::resource::{find_resource_name, Resource};
use serde::{Deserialize, Serialize};

/// One drawable root in a drawable dictionary, in name-string order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictMember {
    pub name: String,
    pub root: usize,
}

/// Resolve and validate one drawable LOD entry and its model pointer array.
fn lod_models(r: &Resource, root: usize, lod: usize) -> Option<Vec<usize>> {
    let entry = r.sys_off(r.u64(root + 0x50 + 8 * lod))?;
    if entry + 16 > r.sys.len() {
        return None;
    }
    let array = r.sys_off(r.u64(entry))?;
    let count = r.u16(entry + 8) as usize;
    if !(1..64).contains(&count) || array + 8 * count > r.sys.len() {
        return None;
    }
    (0..count).map(|i| r.sys_off(r.u64(array + 8 * i))).collect()
}

/// Find named drawable roots with valid LOD tables. A single drawable is
/// deliberately not a dictionary. A root with its own drawable LOD table is
/// also not a dictionary: other named drawables are its embedded parts.
/// All pointers resolve against assembled sys.
pub fn dictionary_members(r: &Resource) -> Vec<DictMember> {
    use std::collections::{HashMap, HashSet};
    if find_resource_name(&r.sys).is_some_and(|(_, tag)| tag == "ft") || (0..4).any(|lod| lod_models(r, 0, lod).is_some()) {
        return Vec::new();
    }
    let mut names = Vec::new();
    for (at, suffix) in r.sys.windows(4).enumerate() {
        if suffix != b".#dr" || r.sys.get(at + 4) != Some(&0) {
            continue;
        }
        let mut start = at;
        while start > 0 && crate::resource::is_name_char(r.sys[start - 1]) {
            start -= 1;
        }
        if at - start >= 2 {
            names.push((start, String::from_utf8_lossy(&r.sys[start..at]).into_owned()));
        }
    }
    if names.len() < 2 {
        return Vec::new();
    }
    // Scan aligned references once, rather than once per member name.
    let mut references: HashMap<usize, Vec<usize>> = names.iter().map(|(offset, _)| (*offset, Vec::new())).collect();
    for location in (0xA8..r.sys.len().saturating_sub(7)).step_by(8) {
        if let Some(target) = r.sys_off(r.u64(location)) {
            if let Some(locations) = references.get_mut(&target) {
                locations.push(location);
            }
        }
    }
    let mut seen = HashSet::new();
    let mut members = Vec::new();
    for (start, name) in names {
        for &location in references.get(&start).into_iter().flatten() {
            let root = location - 0xA8;
            if (0..4).any(|lod| lod_models(r, root, lod).is_some()) && seen.insert(name.clone()) {
                members.push(DictMember { name: name.clone(), root });
            }
        }
    }
    if members.len() < 2 { Vec::new() } else { members }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GfxBuffer {
    pub struct_off: u64,
    pub gfx_off: u64,
    pub bytes: u64,
    pub flags: u32,
    pub kind_hint: u16,
    /// Present when the buffer is a 0x4042 mesh blob.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh_blob: Option<MeshBlobHeader>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshBlobHeader {
    /// (slot, byte offset, format code) for every non-empty vertex stream.
    /// Slot 0 is the position stream (always at 0x80, format 0x42).
    pub sections: Vec<(u32, u32, u32)>,
    pub position_base: [f32; 3],
    pub position_step: [f32; 3],
    pub w: f32,
    pub unk_5c: u32,
    /// Vertex count: positions are 8 bytes each from 0x80 up to the next
    /// stream (12 bytes from 0x60 for float positions).
    pub vertex_count_guess: Option<u32>,
    /// Slot 0 is format 0x63 (3 x f32, unquantised) at 0x60 instead of
    /// packed u16 at 0x80. Seen on cable drawables.
    #[serde(default)]
    pub float_positions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawableInfo {
    #[serde(default)]
    pub members: Vec<String>,
    pub name: Option<String>,
    pub tag: Option<String>,
    /// Floats at +0x2C..+0x4C of the root object (radius?, bbox min, bbox max).
    /// Meaningless on fragments: use `bounds`.
    pub bounds_raw: Vec<f32>,
    /// Model-space bbox [min, max] (RAGE, Z-up): the root drawable's, or for
    /// a fragment the drawable that the root's +0x18 pointer reaches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[[f32; 3]; 2]>,
    pub buffers: Vec<GfxBuffer>,
    pub embedded_textures: Vec<crate::texture::TexInfo>,
    /// Mesh blob + index buffer pairs.
    #[serde(default)]
    pub geometries: Vec<Geometry>,
    /// Geometry groups with per-geometry shader indices.
    #[serde(default)]
    pub models: Vec<Model>,
    /// Shader instances (materials); names are hashed.
    #[serde(default)]
    pub shaders: Vec<Shader>,
    /// Skeleton / node hierarchy (vehicles: chassis, wheels, hubs, doors,
    /// steering wheel...). Hierarchy verified; local transforms unverified.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub skeleton: Vec<Bone>,
    /// Identifier-like strings found in the system page (names of bones,
    /// attach points, etc.).
    pub strings: Vec<String>,
}

/// One bone record (0x50 bytes): +0x00 rotation quaternion (x,y,z,w),
/// +0x10 translation (+ u32 joaat of the name), +0x20 scale (+ u32 parent
/// hash, see `parent_hash`), +0x30 name ptr, +0x40 8×u16 (flags, ?, next
/// sibling, parent, index, name hash, index, ?).
/// The skeleton header (found by its pointer to the bone array) is followed
/// by pointers to the inverse bind matrices (+0x08, 4x4 f32, row-vector
/// convention: p_bone = p_model · M), local bind matrices (+0x10) and u16
/// parent indices (+0x18).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bone {
    pub index: u16,
    pub name: String,
    /// Parent bone index, None for the root.
    pub parent: Option<u16>,
    pub next_sibling: Option<u16>,
    pub flags: u16,
    pub hash16: u16,
    /// Local rotation (x, y, z, w) as stored in the bone record.
    pub rotation: [f32; 4],
    /// Local translation as stored in the bone record, metres.
    pub translation: [f32; 3],
    pub scale: [f32; 3],
    /// Bind pose in model space (bone -> model), 4x4 column-major, RAGE
    /// Z-up, from the inverse bind matrices. Verified: wheels, doors and
    /// lights come out symmetric and in place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_matrix: Option<[f32; 16]>,
    /// Translation part of `bind_matrix` (the pivot, model space).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot: Option<[f32; 3]>,
    /// joaat of the parent bone in another skeleton (u32 at +0x2C, after
    /// the scale; 0 = none). Ped clothing helper bones (`GM_U01_*`) are
    /// roots of the drawable's own skeleton and name their parent in the
    /// shared ped skeleton here (`SKEL_Spine3`, `RB_L_Thigh01` …); their
    /// translation and bind are relative to that parent. Bones of full
    /// skeletons have 0. Only read for root bones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_hash: Option<u32>,
}

/// Invert an affine 4x4 column-major matrix (last row 0,0,0,1).
pub fn affine_inverse(m: &[f32; 16]) -> [f32; 16] {
    let a = |r: usize, c: usize| m[c * 4 + r];
    let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
        + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
    let id = if det.abs() < 1e-12 { 0.0 } else { 1.0 / det };
    let mut inv = [0f32; 9];
    inv[0] = (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) * id;
    inv[1] = (a(0, 2) * a(2, 1) - a(0, 1) * a(2, 2)) * id;
    inv[2] = (a(0, 1) * a(1, 2) - a(0, 2) * a(1, 1)) * id;
    inv[3] = (a(1, 2) * a(2, 0) - a(1, 0) * a(2, 2)) * id;
    inv[4] = (a(0, 0) * a(2, 2) - a(0, 2) * a(2, 0)) * id;
    inv[5] = (a(0, 2) * a(1, 0) - a(0, 0) * a(1, 2)) * id;
    inv[6] = (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0)) * id;
    inv[7] = (a(0, 1) * a(2, 0) - a(0, 0) * a(2, 1)) * id;
    inv[8] = (a(0, 0) * a(1, 1) - a(0, 1) * a(1, 0)) * id;
    let t = [a(0, 3), a(1, 3), a(2, 3)];
    let mut o = [0f32; 16];
    for r in 0..3 {
        for c in 0..3 {
            o[c * 4 + r] = inv[r * 3 + c];
        }
        o[12 + r] = -(inv[r * 3] * t[0] + inv[r * 3 + 1] * t[1] + inv[r * 3 + 2] * t[2]);
    }
    o[15] = 1.0;
    o
}

/// Find the bone array: consecutive 0x50-byte records whose index field
/// counts 0, 1, 2... and whose name pointer resolves to a name.
pub fn find_skeleton(r: &Resource) -> Vec<Bone> {
    scan_skeletons(r, true).into_iter().next().unwrap_or_default()
}

/// Every bone array of a resource, in system page order. Ped clothing can
/// hold several (helper sets for the torso, upper arms, forearms …), and its
/// bone palettes name bones of all of them; `find_skeleton` returns the first.
pub fn find_skeletons(r: &Resource) -> Vec<Vec<Bone>> {
    scan_skeletons(r, false)
}

fn scan_skeletons(r: &Resource, first_only: bool) -> Vec<Vec<Bone>> {
    let ok_name = |s: &str| !s.is_empty() && s.len() < 96 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_');
    // q = record start; name ptr at +0x30, u16 fields at +0x40.
    let rec = |q: usize, k: u16| -> Option<Bone> {
        let np = r.sys_off(r.u64(q + 0x30))?;
        let name = r.cstr(np)?;
        if !ok_name(&name) || r.u16(q + 0x48) != k {
            return None;
        }
        let opt = |v: u16| if v == 0xffff { None } else { Some(v) };
        Some(Bone {
            index: k,
            name,
            parent: opt(r.u16(q + 0x46)),
            next_sibling: opt(r.u16(q + 0x44)),
            flags: r.u16(q + 0x40),
            hash16: r.u16(q + 0x4a),
            rotation: [r.f32(q), r.f32(q + 4), r.f32(q + 8), r.f32(q + 12)],
            translation: [r.f32(q + 0x10), r.f32(q + 0x14), r.f32(q + 0x18)],
            scale: [r.f32(q + 0x20), r.f32(q + 0x24), r.f32(q + 0x28)],
            bind_matrix: None,
            pivot: None,
            parent_hash: Some(r.u32(q + 0x2c)).filter(|&h| h != 0 && r.u16(q + 0x46) == 0xffff),
        })
    };
    let mut out = Vec::new();
    let mut p = 0usize;
    while p + 0x50 <= r.sys.len() {
        if let Some(b0) = rec(p, 0) {
            if b0.parent.is_none() {
                let mut v = vec![b0];
                while let Some(b) = rec(p + v.len() * 0x50, v.len() as u16) {
                    v.push(b);
                }
                if v.len() >= 2 {
                    attach_binds(r, p, &mut v);
                    p += v.len() * 0x50;
                    out.push(v);
                    if first_only {
                        break;
                    }
                    continue;
                }
            }
        }
        p += 16;
    }
    out
}

/// Read the inverse bind matrices through the skeleton header and store
/// each bone's model-space bind matrix.
fn attach_binds(r: &Resource, start: usize, bones: &mut [Bone]) {
    let target = 0x5000_0000u64 | start as u64;
    let n = bones.len();
    let Some(hdr) = (0..r.sys.len().saturating_sub(0x20)).step_by(8).find(|&o| r.u64(o) == target) else { return };
    let Some(ibm) = r.sys_off(r.u64(hdr + 8)) else { return };
    if ibm + 64 * n > r.sys.len() {
        return;
    }
    if let Some(par) = r.sys_off(r.u64(hdr + 0x18)) {
        // sanity: the parent table must agree with the bone records
        let ok = bones.iter().all(|b| {
            let v = r.u16(par + 2 * b.index as usize);
            b.parent.map(|x| x == v).unwrap_or(v == 0xffff)
        });
        if !ok {
            return;
        }
    }
    for b in bones.iter_mut() {
        let o = ibm + 64 * b.index as usize;
        // Stored for row vectors (p · M); read column-major = column-vector form.
        let mut m = [0f32; 16];
        for (k, x) in m.iter_mut().enumerate() {
            *x = r.f32(o + 4 * k);
        }
        m[3] = 0.0;
        m[7] = 0.0;
        m[11] = 0.0;
        m[15] = 1.0;
        let w = affine_inverse(&m);
        b.pivot = Some([w[12], w[13], w[14]]);
        b.bind_matrix = Some(w);
    }
}

fn f32le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

pub fn parse_mesh_blob(b: &[u8]) -> Option<MeshBlobHeader> {
    if b.len() < 0x80 {
        return None;
    }
    let float_positions = u32le(b, 0) == 0x3063;
    if u32le(b, 0) != 0x4042 && !float_positions {
        return None;
    }
    let (p0, stride) = if float_positions { (0x60u32, 12u32) } else { (0x80, 8) };
    let mut sections = Vec::new();
    for slot in 0..23u32 {
        let v = u32le(b, (slot * 4) as usize);
        if v != 0 && slot * 4 < 0x5c {
            sections.push((slot, (v >> 8) * 2, v & 0xff));
        }
    }
    let vertex_count_guess = sections
        .iter()
        .map(|s| s.1)
        .filter(|&o| o > p0 && (o as usize) <= b.len())
        .min()
        .map(|o| (o - p0) / stride);
    Some(MeshBlobHeader {
        sections,
        position_base: if float_positions { [0.0; 3] } else { [f32le(b, 0x60), f32le(b, 0x64), f32le(b, 0x68)] },
        position_step: if float_positions { [0.0; 3] } else { [f32le(b, 0x70), f32le(b, 0x74), f32le(b, 0x78)] },
        w: if float_positions { 0.0 } else { f32le(b, 0x7c) },
        unk_5c: u32le(b, 0x5c),
        vertex_count_guess,
        float_positions,
    })
}

pub fn find_buffers(r: &Resource) -> Vec<GfxBuffer> {
    let mut v = Vec::new();
    let mut p = 0usize;
    while p + 0x20 <= r.sys.len() {
        let ptr = r.u64(p + 0x18);
        let k = r.u16(p + 0x0E);
        if ptr >> 28 == 6 && r.u16(p + 0x0C) == 4 && (k == 8 || k == 9) {
            let dw = r.u32(p + 8) as u64;
            if let Some(go) = r.gfx_off(ptr) {
                let bytes = dw * 4;
                let mesh_blob = r.gfx.get(go..(go + bytes as usize).min(r.gfx.len())).and_then(parse_mesh_blob);
                v.push(GfxBuffer {
                    struct_off: p as u64,
                    gfx_off: go as u64,
                    bytes,
                    flags: r.u32(p + 0x10),
                    kind_hint: k,
                    mesh_blob,
                });
                p += 0x20;
                continue;
            }
        }
        p += 8;
    }
    v
}

pub fn strings(sys: &[u8], min: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    for &c in sys {
        if (0x20..0x7f).contains(&c) {
            cur.push(c);
        } else {
            if cur.len() >= min && c == 0 {
                let s = String::from_utf8_lossy(&cur).to_string();
                if s.chars().any(|c| c.is_ascii_alphabetic()) && !out.contains(&s) {
                    out.push(s);
                }
            }
            cur.clear();
        }
    }
    out
}

/// Model-space bbox of a drawable or fragment resource (system page only).
/// Drawables keep it at +0x30 (min) / +0x40 (max) of the root. A fragment
/// root has other data there; its drawable is at the pointer in +0x18.
pub fn drawable_bounds(r: &Resource) -> Option<[[f32; 3]; 2]> {
    let at = |o| bounds_at(r, o);
    if find_resource_name(&r.sys).map_or(false, |t| t.1 == "ft") {
        return r.sys_off(r.u64(0x18)).and_then(at).or_else(|| at(0));
    }
    at(0)
}

/// Apply the same validation to root and decoded geometry bounds.
fn sane_bounds(b: [[f32; 3]; 2]) -> Option<[[f32; 3]; 2]> {
    let sane = |v: f32| v.is_finite() && v.abs() < 1e5 && (v == 0.0 || v.abs() > 1e-20);
    (b.iter().flatten().all(|&v| sane(v)) && (0..3).all(|k| b[1][k] >= b[0][k])
        && (0..3).any(|k| b[1][k] - b[0][k] > 1e-3)).then_some(b)
}

fn bounds_at(r: &Resource, o: usize) -> Option<[[f32; 3]; 2]> {
    if r.sys.len() < o + 0x4c { return None; }
    sane_bounds([[r.f32(o + 0x30), r.f32(o + 0x34), r.f32(o + 0x38)],
        [r.f32(o + 0x40), r.f32(o + 0x44), r.f32(o + 0x48)]])
}

/// Bounds for a selected member prefer its highest populated LOD's geometry.
/// Ordinary drawables/fragments retain their existing root bounds behaviour.
pub fn model_bounds(r: &Resource, member: Option<&str>) -> Option<[[f32; 3]; 2]> {
    let Some(name) = member else { return drawable_bounds(r) };
    let members = dictionary_members(r);
    let index = members.iter().position(|m| m.name == name)?;
    let bufs = find_buffers(r);
    let geoms = find_geometries(r, &bufs);
    let models = find_models(r, &geoms);
    let lod = models.iter().filter(|m| m.member == Some(index as u32)).map(|m| m.lod).min();
    let mut b = [[f32::MAX; 3], [f32::MIN; 3]];
    for m in models.iter().filter(|m| m.member == Some(index as u32) && Some(m.lod) == lod) {
        for &gi in &m.geometries {
            let buf = &bufs[geoms[gi].vertex_buffer];
            if let Some(h) = &buf.mesh_blob {
                let end = (buf.gfx_off + buf.bytes).min(r.gfx.len() as u64) as usize;
                if let Some(bytes) = r.gfx.get(buf.gfx_off as usize..end) {
                    let ib = &bufs[geoms[gi].index_buffer];
                    let ib_end = (ib.gfx_off + ib.bytes).min(r.gfx.len() as u64) as usize;
                    let indices = r.gfx.get(ib.gfx_off as usize..ib_end).unwrap_or_default();
                    for p in decode_mesh(bytes, h, indices).positions {
                        if !p.iter().all(|v| v.is_finite()) { return bounds_at(r, members[index].root); }
                        for k in 0..3 { b[0][k] = b[0][k].min(p[k]); b[1][k] = b[1][k].max(p[k]); }
                    }
                }
            }
        }
    }
    sane_bounds(b).or_else(|| bounds_at(r, members[index].root))
}

/// Keep only the assets referenced by one dictionary member and remap indices.
pub fn select_member(r: &Resource, info: &mut DrawableInfo, name: &str) -> anyhow::Result<()> {
    use std::collections::{BTreeSet, HashMap};
    let index = info.members.iter().position(|n| n == name)
        .ok_or_else(|| anyhow::anyhow!("no drawable member {name}"))?;
    info.models.retain(|m| m.member == Some(index as u32));
    info.name = Some(name.to_string());
    let root = dictionary_members(r)[index].root;
    info.bounds_raw = (0..8).map(|i| r.f32(root + 0x2c + i * 4)).collect();
    info.bounds = model_bounds(r, Some(name));
    // Model shader indices are local to the member's shader group. The
    // resource-wide longest run may belong to a different drawable.
    if let Some(array) = r.sys_off(r.u64(root + 0x10)).and_then(|s| r.sys_off(r.u64(s + 0x10))) {
        let shaders = shaders_at(r, array);
        if !shaders.is_empty() {
            info.shaders = shaders;
        }
    }
    let shader_ids: BTreeSet<usize> = info.models.iter().flat_map(|m| m.shader_indices.iter().map(|&i| i as usize)).collect();
    let mut shader_map = HashMap::new();
    info.shaders = info.shaders.iter().enumerate().filter(|(i, _)| shader_ids.contains(i)).map(|(i, s)| {
        let mut s = s.clone(); s.index = shader_map.len(); shader_map.insert(i, s.index); s
    }).collect();
    for m in &mut info.models { for i in &mut m.shader_indices { *i = shader_map.get(&(*i as usize)).copied().unwrap_or(u16::MAX as usize) as u16; } }
    let geom_ids: BTreeSet<usize> = info.models.iter().flat_map(|m| m.geometries.iter().copied()).collect();
    let geom_map: HashMap<usize, usize> = geom_ids.iter().enumerate().map(|(new, &old)| (old, new)).collect();
    info.geometries = geom_ids.iter().map(|&i| info.geometries[i].clone()).collect();
    for m in &mut info.models { for i in &mut m.geometries { *i = geom_map[i]; } }
    let buf_ids: BTreeSet<usize> = info.geometries.iter().flat_map(|g| [g.vertex_buffer, g.index_buffer]).collect();
    let buf_map: HashMap<usize, usize> = buf_ids.iter().enumerate().map(|(new, &old)| (old, new)).collect();
    info.buffers = buf_ids.iter().map(|&i| info.buffers[i].clone()).collect();
    for g in &mut info.geometries { g.vertex_buffer = buf_map[&g.vertex_buffer]; g.index_buffer = buf_map[&g.index_buffer]; }
    let hashes: BTreeSet<u32> = info.shaders.iter().flat_map(|s| &s.textures).filter_map(|t| u32::from_str_radix(&t.hash32, 16).ok()).collect();
    // Embedded texture slots point straight to texture structs; external
    // references use hashes. Keep both forms, including unnamed embedded data.
    let embedded_offsets: BTreeSet<u64> = info.shaders.iter().flat_map(|s| {
        let slots = r.sys_off(r.u64(s.record_off as usize + 0x10));
        (0..s.param_hashes.len()).filter_map(move |i| slots.and_then(|at| r.sys_off(r.u64(at + 8 * i))).map(|at| at as u64))
    }).collect();
    info.embedded_textures.retain(|t| embedded_offsets.contains(&t.struct_off) || hashes.contains(&crate::meta7a::joaat(&t.name)));
    Ok(())
}

pub fn describe(r: &Resource) -> DrawableInfo {
    let nt = find_resource_name(&r.sys);
    let bounds_raw = if r.sys.len() >= 0x4c { (0..8).map(|i| r.f32(0x2c + i * 4)).collect() } else { vec![] };
    let buffers = find_buffers(r);
    let geometries = find_geometries(r, &buffers);
    DrawableInfo {
        members: dictionary_members(r).into_iter().map(|m| m.name).collect(),
        name: nt.as_ref().map(|x| x.0.clone()),
        tag: nt.map(|x| x.1),
        bounds_raw,
        bounds: drawable_bounds(r),
        models: find_models(r, &geometries),
        shaders: find_shaders(r),
        geometries,
        buffers,
        embedded_textures: crate::texture::find_textures(r),
        skeleton: find_skeleton(r),
        strings: strings(&r.sys, 4)
            .into_iter()
            .filter(|s| s.len() >= 4 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == b'#') && s.bytes().any(|c| c.is_ascii_lowercase()))
            .take(4000)
            .collect(),
    }
}

/// Decode the packed positions of a mesh blob. Each vertex is one u64 at
/// 0x80 + 8*i. x and y are 24-bit: high 16 bits at 16..31 (x) and 32..47
/// (y), low bytes at 0..7 (x) and 8..15 (y). z is 16-bit at 48..63.
pub fn decode_positions(b: &[u8], h: &MeshBlobHeader) -> Vec<[f32; 3]> {
    let n = h.vertex_count_guess.unwrap_or(0) as usize;
    let mut v = Vec::with_capacity(n);
    if h.float_positions {
        for i in 0..n {
            let o = 0x60 + 12 * i;
            if o + 12 > b.len() { break }
            v.push([f32le(b, o), f32le(b, o + 4), f32le(b, o + 8)]);
        }
        return v;
    }
    for i in 0..n {
        let o = 0x80 + 8 * i;
        let Some(q) = b.get(o..o + 8).map(|x| u64::from_le_bytes(x.try_into().unwrap())) else { break };
        let (bx, sx) = (h.position_base, h.position_step);
        v.push([
            ((((q >> 16) & 0xffff) << 8) | (q & 0xff)) as f32 * sx[0] + bx[0],
            ((((q >> 32) & 0xffff) << 8) | ((q >> 8) & 0xff)) as f32 * sx[1] + bx[1],
            (q >> 48) as f32 * sx[2] + bx[2],
        ]);
    }
    v
}

/// A decoded mesh (one geometry), RAGE space (Z-up, metres).
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    /// (vertex-stream slot, UVs). Slots 3, 4, 14, 15 = UV0..UV3.
    pub uvs: Vec<(u32, Vec<[f32; 2]>)>,
    /// Slot 1 RGBA8 (vertex colour).
    pub colors: Option<Vec<[u8; 4]>>,
    pub indices: Vec<u32>,
    /// Triangles whose winding was flipped to agree with stored normals.
    pub flipped: usize,
    /// Triangles dropped (index out of range or degenerate).
    pub dropped: usize,
    /// Vertices whose stored normal word held no normal (bit 31 clear);
    /// their normals were rebuilt from the faces around them.
    pub rebuilt_normals: usize,
    /// Model axis held by each quantised position slot (x, y, z slot).
    /// [0, 1, 2] = identity. The encoder puts the flattest axis in the
    /// 16-bit z slot and rotates the others cyclically.
    pub axis_order: [usize; 3],
    /// Skin joints per vertex (skeleton bone indices). Stream 8: format
    /// 0x25 = 4 x u8 blend indices; format 0x00 is sized per vertex:
    /// 1 byte = one u8 bone, 2 = one u16 bone (rigid), 4 = 4 x u8,
    /// 8 = 4 x u16 blend indices. Format 0x8C (ped heads and clothing) = 4 x
    /// u32 per vertex, the joint index in the low 12 bits of each; in stream
    /// 8 bits 12-15 and the high u16 hold other per-vertex data (on a
    /// jacket, 0 of 82,042 weighted low-12-bit indices fall outside the bone
    /// palette; the full low u16 misses on 26 %).
    pub joints: Option<Vec<[u16; 4]>>,
    /// Skin weights per vertex (u8, sum 255). Stream 6 (format 0x26) when
    /// stream 8 has 4 indices; otherwise (255, 0, 0, 0).
    pub weights: Option<Vec<[u8; 4]>>,
    /// Second influence set (influences 5-8): stream 9 (format 0x25, 4 x u8
    /// joint indices, or 0x8C next to a 0x8C stream 8) and stream 7 (format 0x26, weights). Seen on ped
    /// meshes; the 8 weights together sum to 255 and are sorted descending.
    pub joints1: Option<Vec<[u16; 4]>>,
    pub weights1: Option<Vec<[u8; 4]>>,
    /// Tangents (x, y, z, w), model space like the normals: stream 11,
    /// format 0x27 = R10G10B10A2 (xyz = v / 1023 * 2 - 1, A2 = 3 when the
    /// bitangent is flipped, so w = -1). Orthonormalised against the normal.
    pub tangents: Option<Vec<[f32; 4]>>,
}

/// Half-width (m) given to cable ribbons, which the game widens in its
/// cable shader. Power and phone lines are a few centimetres thick.
pub const CABLE_RADIUS: f32 = 0.02;

/// Streams that hold texture coordinates, in UV-set order.
pub const UV_SLOTS: [u32; 4] = [3, 4, 14, 15];

/// Stream 11 (format 0x27): R10G10B10A2 tangent. Checked against tangents
/// computed from UV0 (mean |dot| 1.000) and handedness (A2 vs UV winding,
/// correlation 0.999) on a map piece.
fn tangent_decode(w: u32, n: Option<[f32; 3]>) -> [f32; 4] {
    let c = |s: u32| ((w >> s) & 1023) as f32 / 1023.0 * 2.0 - 1.0;
    let mut t = [c(0), c(10), c(20)];
    if let Some(n) = n {
        let d = t[0] * n[0] + t[1] * n[1] + t[2] * n[2];
        for k in 0..3 {
            t[k] -= d * n[k];
        }
        if t.iter().map(|x| x * x).sum::<f32>() < 1e-8 {
            // Degenerate: any unit vector perpendicular to the normal.
            t = if n[0].abs() < 0.9 { [1.0 - n[0] * n[0], -n[0] * n[1], -n[0] * n[2]] } else { [-n[1] * n[0], 1.0 - n[1] * n[1], -n[1] * n[2]] };
        }
    }
    let l = t.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    let sign = if (w >> 31) & 1 == 1 { -1.0 } else { 1.0 };
    [t[0] / l, t[1] / l, t[2] / l, sign]
}

/// Normal word (stream 2, format 0x21): octahedral x in bits 0-9, y in
/// bits 10-19. Bit 31 is set on words that hold a normal. Words with it
/// clear (about a third of the vertices of ped heads, a few per cent of
/// other skinned meshes, almost none on static ones) decode to directions
/// that disagree with the surface in ~75 % of cases (checked against the
/// face normals of `cs_ali` / `crr_iguana` heads and bodies, a helicopter
/// and buildings), and no other field layout fits them; they are treated as
/// missing and rebuilt from the faces.
fn octa_decode_checked(w: u32) -> Option<[f32; 3]> {
    if w >> 31 == 0 {
        return None;
    }
    Some(octa_decode(w))
}

fn octa_decode(w: u32) -> [f32; 3] {
    let x = (w & 1023) as f32 / 1023.0 * 2.0 - 1.0;
    let y = ((w >> 10) & 1023) as f32 / 1023.0 * 2.0 - 1.0;
    let z = 1.0 - x.abs() - y.abs();
    let t = (-z).max(0.0);
    let x = x + if x >= 0.0 { -t } else { t };
    let y = y + if y >= 0.0 { -t } else { t };
    let l = (x * x + y * y + z * z).sqrt().max(1e-12);
    [x / l, y / l, z / l]
}

/// Decode the meshlet index buffer into a triangle list (vertex indices).
/// Layout: a 12-byte header (u16 meshlet count, u16 ?, u32 triangle count,
/// u32 remap count); then 16-byte meshlet descriptors from 0x0C (u32 ?,
/// u16 remap start, u16 triangle start, ...); then one u32 per triangle
/// (7 bits per local index, meshlet number from bit 21); then the u16 remap
/// list (local -> vertex index). The two counts are 32-bit: a motorcycle
/// body has 70,057 triangles and 69,901 remap entries. The descriptor's
/// remap start stays 16-bit and wraps there, so it is unwrapped along the
/// (ascending) meshlet order.
pub fn decode_meshlets(ib: &[u8], vertex_count: usize) -> (Vec<u32>, usize) {
    let u16at = |o: usize| ib.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]]) as usize);
    let u32at = |o: usize| ib.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()) as usize);
    let (Some(n), Some(ntri), Some(nremap)) = (u16at(0), u32at(4), u32at(8)) else { return (vec![], 0) };
    let tri0 = 0x0c + 16 * n;
    let ntri = ntri.min(ib.len().saturating_sub(tri0) / 4);
    let rem0 = tri0 + 4 * ntri;
    let mut wrap = 0;
    let mut prev = 0;
    let starts: Vec<usize> = (0..n)
        .map(|k| {
            let s = u16at(0x10 + 16 * k).unwrap_or(0);
            if s < prev {
                wrap += 0x1_0000;
            }
            prev = s;
            s + wrap
        })
        .collect();
    let nremap = nremap.min(ib.len().saturating_sub(rem0) / 2);
    let mut idx = Vec::with_capacity(ntri * 3);
    let mut dropped = 0;
    for t in 0..ntri {
        let Some(w) = ib.get(tri0 + 4 * t..tri0 + 4 * t + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap())) else { break };
        let ml = (w >> 21) as usize;
        let Some(&base) = starts.get(ml) else {
            dropped += 1;
            continue;
        };
        let mut tri = [0u32; 3];
        let mut ok = true;
        for (k, sh) in [0u32, 7, 14].iter().enumerate() {
            let r = base + ((w >> sh) & 0x7f) as usize;
            match if r < nremap { u16at(rem0 + 2 * r) } else { None } {
                Some(v) if v < vertex_count => tri[k] = v as u32,
                _ => ok = false,
            }
        }
        if ok && tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2] {
            idx.extend_from_slice(&tri);
        } else {
            dropped += 1;
        }
    }
    (idx, dropped)
}

/// Decode a full mesh from a mesh blob and its meshlet index buffer.
pub fn decode_mesh(blob: &[u8], h: &MeshBlobHeader, ib: &[u8]) -> Mesh {
    let mut positions = decode_positions(blob, h);
    let v = positions.len();
    let stream = |slot: u32| h.sections.iter().find(|s| s.0 == slot).map(|s| (s.1 as usize, s.2));
    // Vertices whose normal word has no usable normal get a zero normal here
    // (left out of the axis and winding checks) and are rebuilt from the
    // faces at the end.
    let mut invalid: Vec<usize> = Vec::new();
    let mut normals = stream(2).filter(|s| s.1 == 0x21).and_then(|s| blob.get(s.0..s.0 + 4 * v)).map(|b| {
        b.chunks_exact(4)
            .enumerate()
            .map(|(i, c)| match octa_decode_checked(u32::from_le_bytes(c.try_into().unwrap())) {
                Some(n) => n,
                None => {
                    invalid.push(i);
                    [0.0; 3]
                }
            })
            .collect::<Vec<_>>()
    });
    let mut uvs = Vec::new();
    for slot in UV_SLOTS {
        let Some((off, fmt)) = stream(slot) else { continue };
        let uv: Option<Vec<[f32; 2]>> = match fmt {
            // 2 x s16, 1.0 = 8192
            0x28 => blob.get(off..off + 4 * v).map(|b| {
                b.chunks_exact(4)
                    .map(|c| [i16::from_le_bytes([c[0], c[1]]) as f32 / 8192.0, i16::from_le_bytes([c[2], c[3]]) as f32 / 8192.0])
                    .collect()
            }),
            // 2 x f32
            0x42 => blob.get(off..off + 8 * v).map(|b| {
                b.chunks_exact(8)
                    .map(|c| [f32::from_le_bytes(c[0..4].try_into().unwrap()), f32::from_le_bytes(c[4..8].try_into().unwrap())])
                    .collect()
            }),
            _ => None,
        };
        if let Some(uv) = uv {
            uvs.push((slot, uv));
        }
    }
    let colors = stream(1)
        .filter(|s| s.1 == 0x26)
        .and_then(|s| blob.get(s.0..s.0 + 4 * v))
        .map(|b| b.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect());
    let (mut indices, dropped) = decode_meshlets(ib, v);
    // Stream 8 is sized per vertex: the next section's offset (or the blob
    // end) minus this one, over the vertex count, gives its width.
    let width = |slot: u32| -> usize {
        let Some((o, _)) = stream(slot) else { return 0 };
        let next = h.sections.iter().map(|s| s.1 as usize).filter(|&x| x > o).min().unwrap_or(blob.len());
        if v == 0 { 0 } else { (next.min(blob.len()) - o) / v }
    };
    let u16le = |c: &[u8]| u16::from_le_bytes([c[0], c[1]]);
    let quad_u32 = |b: &[u8]| -> Vec<[u16; 4]> { b.chunks_exact(16).map(|c| [0, 4, 8, 12].map(|k| u16le(&c[k..]) & 0x0fff)).collect() };
    let jw = width(8);
    let joints: Option<Vec<[u16; 4]>> = match stream(8) {
        Some((o, 0x25)) => blob.get(o..o + 4 * v).map(|b| b.chunks_exact(4).map(|c| [c[0] as u16, c[1] as u16, c[2] as u16, c[3] as u16]).collect()),
        // 4 x u32 (peds): joint index in the low 12 bits of each
        Some((o, 0x8c)) => blob.get(o..o + 16 * v).map(quad_u32),
        // one u16 bone per vertex (rigid, skeletons over 255 bones)
        Some((o, 0x00)) if (2..4).contains(&jw) => blob.get(o..o + 2 * v).map(|b| b.chunks_exact(2).map(|c| [u16le(c), 0, 0, 0]).collect()),
        // 4 x u16 blend indices
        Some((o, 0x00)) if (8..16).contains(&jw) => blob.get(o..o + 8 * v).map(|b| b.chunks_exact(8).map(|c| [u16le(&c[0..]), u16le(&c[2..]), u16le(&c[4..]), u16le(&c[6..])]).collect()),
        // 4 x u8 blend indices in a format-0 stream
        Some((o, 0x00)) if (4..8).contains(&jw) => blob.get(o..o + 4 * v).map(|b| b.chunks_exact(4).map(|c| [c[0] as u16, c[1] as u16, c[2] as u16, c[3] as u16]).collect()),
        Some((o, 0x00)) if jw < 2 => blob.get(o..o + v).map(|b| b.iter().map(|&j| [j as u16, 0, 0, 0]).collect()),
        _ => None,
    };
    let blend = joints.is_some() && (matches!(stream(8).map(|s| s.1), Some(0x25 | 0x8c)) || (4..16).contains(&jw));
    let weights: Option<Vec<[u8; 4]>> = joints.as_ref().map(|j| {
        if blend {
            stream(6)
                .filter(|s| s.1 == 0x26)
                .and_then(|s| blob.get(s.0..s.0 + 4 * v))
                .map(|b| b.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect())
                .unwrap_or_else(|| vec![[255, 0, 0, 0]; j.len()])
        } else {
            vec![[255, 0, 0, 0]; j.len()]
        }
    });
    // Influences 5-8 (peds): only next to a 4-index blend set.
    let quad = |slot: u32, fmt: u32| stream(slot).filter(|s| s.1 == fmt).and_then(|s| blob.get(s.0..s.0 + 4 * v)).map(|b| b.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect::<Vec<[u8; 4]>>());
    let second = match stream(8).map(|s| s.1) {
        Some(0x25) if blend => quad(9, 0x25).map(|j| j.iter().map(|c| c.map(|x| x as u16)).collect::<Vec<_>>()),
        Some(0x8c) if blend => stream(9).filter(|s| s.1 == 0x8c).and_then(|s| blob.get(s.0..s.0 + 16 * v)).map(quad_u32),
        _ => None,
    };
    let (joints1, weights1) = match (second, quad(7, 0x26)) {
        (Some(j), Some(w)) if w.iter().any(|x| x.iter().any(|&y| y > 0)) => (Some(j), Some(w)),
        _ => (None, None),
    };
    if std::env::var_os("SIXNG_DEBUG_SKIN").is_some() {
        eprintln!("skin: v={v} stream8 fmt={:?} width={jw} max={:?}", stream(8).map(|s| s.1), joints.as_ref().map(|j| j.iter().flatten().max().copied()));
    }
    // Undo the axis rotation of the position quantiser: the header's u32 at
    // +0x6C says which order was used (2 = identity, 1 / 0 = rotated, see
    // below). Only when it holds another value is the order picked by how
    // well the faces agree with the stored (model-space) normals.
    // Cables: float positions, slot 2 = unit direction along the wire, UV0.x
    // = +-1 = which side the shader pushes the vertex to. Each vertex pair
    // shares a position, so the stored ribbons have zero width. Widen them
    // here into two crossed ribbons (horizontal and vertical) so the wire is
    // visible without the cable shader.
    if h.float_positions {
        let dir: Option<Vec<[f32; 3]>> = stream(2).filter(|s| s.1 == 0x63).and_then(|s| blob.get(s.0..s.0 + 12 * v)).map(|b| {
            b.chunks_exact(12).map(|c| [f32le(c, 0), f32le(c, 4), f32le(c, 8)]).collect()
        });
        let side: Option<Vec<f32>> = uvs.iter().find(|u| u.0 == 3).map(|u| u.1.iter().map(|x| x[0]).collect());
        if let (Some(dir), Some(side)) = (dir, side) {
            // Every position appears exactly twice (one vertex per side).
            let mut seen: std::collections::HashMap<[u32; 3], u32> = std::collections::HashMap::new();
            for p in &positions {
                *seen.entry(p.map(f32::to_bits)).or_default() += 1;
            }
            let paired = v >= 4 && seen.values().all(|&c| c == 2);
            if paired && side.iter().all(|s| (s.abs() - 1.0).abs() < 1e-3) {
                let norm = |a: [f32; 3]| { let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt(); if l < 1e-6 { None } else { Some([a[0] / l, a[1] / l, a[2] / l]) } };
                let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
                let mut pos2 = Vec::with_capacity(2 * v);
                let mut nrm2 = Vec::with_capacity(2 * v);
                let mut pos_v = Vec::with_capacity(v);
                let mut nrm_v = Vec::with_capacity(v);
                for i in 0..v {
                    let t = norm(dir[i]).unwrap_or([1.0, 0.0, 0.0]);
                    let hz = norm(cross(t, [0.0, 0.0, 1.0])).unwrap_or([1.0, 0.0, 0.0]);
                    let vt = norm(cross(hz, t)).unwrap_or([0.0, 0.0, 1.0]);
                    let (p, s) = (positions[i], side[i] * CABLE_RADIUS);
                    pos2.push([p[0] + s * hz[0], p[1] + s * hz[1], p[2] + s * hz[2]]);
                    nrm2.push(vt);
                    pos_v.push([p[0] + s * vt[0], p[1] + s * vt[1], p[2] + s * vt[2]]);
                    nrm_v.push(hz);
                }
                pos2.extend(pos_v);
                nrm2.extend(nrm_v);
                let n0 = indices.len();
                for k in 0..n0 { let x = indices[k] + v as u32; indices.push(x); }
                for u in uvs.iter_mut() { let c = u.1.clone(); u.1.extend(c); }
                let colors: Option<Vec<[u8; 4]>> = colors.map(|c: Vec<[u8; 4]>| { let mut d = c.clone(); d.extend(c); d });
                return Mesh { positions: pos2, normals: Some(nrm2), uvs, colors, indices, flipped: 0, dropped, axis_order: [0, 1, 2], joints: None, weights: None, joints1: None, weights1: None, tangents: None, rebuilt_normals: 0 };
            }
        }
    }
    // Float-position meshes (water planes) keep 3 x f32 normals in slot 2.
    if normals.is_none() && h.float_positions {
        normals = stream(2).filter(|s| s.1 == 0x63).and_then(|s| blob.get(s.0..s.0 + 12 * v)).map(|b| {
            b.chunks_exact(12).map(|c| { let n = [f32le(c, 0), f32le(c, 4), f32le(c, 8)]; let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12); [n[0] / l, n[1] / l, n[2] / l] }).collect()
        });
    }
    let mode = if h.float_positions { 2 } else { blob.get(0x6c..0x70).map(|x| u32::from_le_bytes(x.try_into().unwrap())).unwrap_or(2) };
    // The three cyclic orders first (the header hints between them), then
    // the three swaps, which some building drawables use (wall panels
    // came out lying flat with only the cyclic orders).
    let orders: [[usize; 3]; 6] = [[0, 1, 2], [2, 0, 1], [1, 2, 0], [0, 2, 1], [2, 1, 0], [1, 0, 2]];
    // +0x6C is the model axis held in the flat 16-bit slot: 0 = x (slots
    // hold z, y, x: a swap), 1 = y (z, x, y), 2 = z (identity). Checked on
    // dn_14_structure_03_b: the hinted order scores ~1.0 against the normals
    // on every geometry.
    //
    // The header is taken at its word. An earlier rule switched to another
    // order when its faces agreed more than 0.05 better with the normals;
    // over 36,305 sampled meshes that switched 114, and all 7 that could be
    // checked another way (against the model's other LODs, its bounds or
    // matching parts) were right before the switch: a handcuffs chain of
    // 52,586 triangles, one ped head of 55, helicopter rotor-blade pieces.
    // The switches came from meshes with few usable normals (often 1-20
    // scored triangles) or normals that fit no order (best score 0.5-0.7).
    let hinted = match mode { 1 => 1, 0 => 4, _ => 0 };
    let known = h.float_positions || mode <= 2;
    let mut axis_order = orders[hinted];
    if let Some(n) = &normals {
        let score = |o: &[usize; 3]| -> f32 {
            let mut sum = 0.0;
            let mut cnt = 0usize;
            for t in indices.chunks_exact(3).step_by(1 + indices.len() / 3 / 4000) {
                let m = |i: u32| { let p = positions[i as usize]; let mut q = [0f32; 3]; for k in 0..3 { q[o[k]] = p[k]; } q };
                let (a, b, c) = (m(t[0]), m(t[1]), m(t[2]));
                let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let f = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
                let nn: Vec<f32> = (0..3).map(|k| n[t[0] as usize][k] + n[t[1] as usize][k] + n[t[2] as usize][k]).collect();
                let lf = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
                let ln = (nn[0] * nn[0] + nn[1] * nn[1] + nn[2] * nn[2]).sqrt();
                if lf > 0.0 && ln > 0.0 {
                    sum += ((f[0] * nn[0] + f[1] * nn[1] + f[2] * nn[2]) / (lf * ln)).abs();
                    cnt += 1;
                }
            }
            if cnt == 0 { 0.0 } else { sum / cnt as f32 }
        };
        let sh = score(&orders[hinted]);
        if std::env::var_os("SIXNG_DEBUG_AXIS").is_some() {
            eprintln!("axis: v={v} mode={mode} scores={:?}", orders.iter().map(|o| (score(o) * 1000.0).round() / 1000.0).collect::<Vec<_>>());
        }
        if !known {
            let (bi, _) = orders.iter().enumerate().map(|(i, o)| (i, score(o))).fold((hinted, sh), |a, b| if b.1 > a.1 + 0.05 { b } else { a });
            axis_order = orders[bi];
        }
    }
    if axis_order != [0, 1, 2] {
        for p in positions.iter_mut() {
            let d = *p;
            for k in 0..3 { p[axis_order[k]] = d[k]; }
        }
    }
    // Orient each triangle so its geometric normal agrees with the stored
    // vertex normals (the stored winding is not consistent with them).
    let mut flipped = 0;
    if let Some(n) = &normals {
        for t in indices.chunks_exact_mut(3) {
            let (a, b, c) = (positions[t[0] as usize], positions[t[1] as usize], positions[t[2] as usize]);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let f = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
            let s: f32 = (0..3).map(|k| f[k] * (n[t[0] as usize][k] + n[t[1] as usize][k] + n[t[2] as usize][k])).sum();
            if s < 0.0 {
                t.swap(1, 2);
                flipped += 1;
            }
        }
    }
    // Rebuild the normals of vertices whose stored word has no usable normal
    // (bit 31 clear, see `octa_decode_checked`) from the oriented faces
    // around them, welding vertices at the same position so UV seams stay
    // smooth.
    let mut rebuilt_normals = 0;
    if let (Some(n), false) = (normals.as_mut(), invalid.is_empty()) {
        let key = |p: [f32; 3]| [(p[0] * 1e4).round() as i64, (p[1] * 1e4).round() as i64, (p[2] * 1e4).round() as i64];
        let mut acc: std::collections::HashMap<[i64; 3], [f32; 3]> = std::collections::HashMap::new();
        for t in indices.chunks_exact(3) {
            let (a, b, c) = (positions[t[0] as usize], positions[t[1] as usize], positions[t[2] as usize]);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let f = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
            for &i in t {
                if invalid.contains(&(i as usize)) {
                    let e = acc.entry(key(positions[i as usize])).or_insert([0.0; 3]);
                    for k in 0..3 {
                        e[k] += f[k];
                    }
                }
            }
        }
        for &i in &invalid {
            if let Some(g) = acc.get(&key(positions[i])) {
                let l = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
                if l > 0.0 {
                    n[i] = [g[0] / l, g[1] / l, g[2] / l];
                    rebuilt_normals += 1;
                }
            }
        }
    }
    let tangents = stream(11).filter(|s| s.1 == 0x27).and_then(|s| blob.get(s.0..s.0 + 4 * v)).map(|b| {
        b.chunks_exact(4)
            .enumerate()
            .map(|(i, c)| tangent_decode(u32::from_le_bytes(c.try_into().unwrap()), normals.as_ref().map(|n| n[i])))
            .collect::<Vec<_>>()
    });
    Mesh { positions, normals, uvs, colors, indices, flipped, dropped, axis_order, joints, weights, joints1, weights1, tangents, rebuilt_normals }
}

/// A geometry: a mesh blob paired with its meshlet index buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Geometry {
    /// Index into `DrawableInfo::buffers` of the mesh blob.
    pub vertex_buffer: usize,
    /// Index into `DrawableInfo::buffers` of the meshlet index buffer.
    pub index_buffer: usize,
    /// System-page offset of the geometry record (blob pointer at +0x00,
    /// pointer to the index-buffer holder at +0x38).
    pub record_off: u64,
}

/// Pair mesh blobs with their index buffers via the geometry records.
pub fn find_geometries(r: &Resource, bufs: &[GfxBuffer]) -> Vec<Geometry> {
    use std::collections::HashMap;
    let by_struct: HashMap<u64, usize> = bufs.iter().enumerate().map(|(i, b)| (b.struct_off, i)).collect();
    let target = |ptr: u64| -> Option<usize> { r.sys_off(ptr).and_then(|o| by_struct.get(&(o as u64)).copied()) };
    let mut out = Vec::new();
    let mut o = 0;
    while o + 0x40 <= r.sys.len() {
        if let Some(vb) = target(r.u64(o)).filter(|&i| bufs[i].mesh_blob.is_some()) {
            if let Some(holder) = r.sys_off(r.u64(o + 0x38)) {
                if let Some(ib) = target(r.u64(holder)).filter(|&i| bufs[i].mesh_blob.is_none()) {
                    out.push(Geometry { vertex_buffer: vb, index_buffer: ib, record_off: o as u64 });
                }
            }
        }
        o += 8;
    }
    out.sort_by_key(|g| g.vertex_buffer);
    out.dedup_by_key(|g| g.vertex_buffer);
    out
}

/// A model: a group of geometries drawn together, each with a shader index.
/// Struct: +0x00 ptr to an array of geometry-record ptrs, +0x08 u16 count,
/// +0x0A u16 capacity, +0x0C u32 (bone count?), +0x18 ptr to u16 shader
/// indices (one per geometry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Index into `dictionary_members`, absent for ordinary drawables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<u32>,
    pub struct_off: u64,
    /// Indices into `DrawableInfo::geometries`.
    pub geometries: Vec<usize>,
    /// Shader index (into `DrawableInfo::shaders`) for each geometry.
    pub shader_indices: Vec<u16>,
    /// Level of detail (0 = highest), from the drawable's LOD table:
    /// 16-byte entries {ptr to an array of model ptrs, u16 count, u16 cap}.
    /// Ranked by table position; empty levels are not counted.
    #[serde(default)]
    pub lod: usize,
    /// Not in the main drawable's LOD group: a model of an embedded drawable
    /// (fragment child, nested sub-drawable). `lod` is then 0.
    #[serde(default)]
    pub part: bool,
    /// Skinning palette (peds): the vertex joint indices of this model's
    /// geometries index this list of bone name hashes (joaat, sorted
    /// ascending), not the drawable's own bone order. The bones are the
    /// drawable's own (`GM_U01_*` girth helpers) and bones of the shared ped
    /// skeleton, which is not in the file. Model record: +0x0C u16 count,
    /// +0x20 pointer. Vehicle models use these fields for something else
    /// (the list is not ascending), so only an ascending list is taken.
    /// A one-bone palette is a rigid part on that bone (an animal's eyes and
    /// tongue on `SKEL_Head`): its vertices carry no joints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bone_palette: Option<Vec<u32>>,
}

/// Bone palette of a model record (see `Model::bone_palette`).
fn model_palette(r: &Resource, o: usize) -> Option<Vec<u32>> {
    let n = r.u16(o + 12) as usize;
    if !(1..=1024).contains(&n) {
        return None;
    }
    let p = r.sys_off(r.u64(o + 0x20))?;
    if p + 4 * n > r.sys.len() {
        return None;
    }
    let v: Vec<u32> = (0..n).map(|k| r.u32(p + 4 * k)).collect();
    v.windows(2).all(|w| w[0] < w[1]).then_some(v)
}

/// One shader instance (64-byte record tagged "atem"): +0x00 u32 shader
/// name hash, +0x04 "atem", +0x10 ptr to per-parameter texture slots,
/// +0x20 ptr to the parameter table (u8 ?, u8 count, ..., then count x
/// (u32 parameter name hash, u32 data offset)).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shader {
    pub index: usize,
    pub record_off: u64,
    /// Shader name hash (unresolved: does not match joaat of known names).
    pub name_hash: String,
    pub param_hashes: Vec<String>,
    pub textures: Vec<TexRef>,
    /// Record bytes +0x38..+0x40 (render bucket flags, see material.rs).
    #[serde(default)]
    pub flags: Vec<u8>,
    /// Non-texture parameters (constants), in table order.
    #[serde(default)]
    pub constants: Vec<ShaderConst>,
}

/// A constant of a shader: the parameter table continues after the
/// texture entries with {u32 name hash, u32 descriptor}. Descriptors
/// `0x0040_XX_tt` index the constant block (record +0x08, which runs up to
/// the texture-pointer array at +0x10) at float offset XX; others
/// (`0x01…`, `0x0041…`) point into buffers that are not read. `values` is
/// the vec4 at that offset, or None when it is not in the block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaderConst {
    pub hash: String,
    pub desc: String,
    pub offset: usize,
    pub values: Option<[f32; 4]>,
}

/// A texture slot of a shader: the parameter it binds to and the texture
/// reference hashes (64-bit and 32-bit; unresolved).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TexRef {
    pub param: usize,
    pub param_hash: String,
    pub hash64: String,
    pub hash32: String,
}

/// Find the shader array: the longest run of pointers to "atem" records.
pub fn find_shaders(r: &Resource) -> Vec<Shader> {
    let is_shader = |p: u64| r.sys_off(p).filter(|&o| r.sys.get(o + 4..o + 8) == Some(b"atem")).is_some();
    let mut best = (0usize, 0usize);
    let mut o = 0;
    while o + 8 <= r.sys.len() {
        let mut n = 0;
        while is_shader(r.u64(o + 8 * n)) {
            n += 1;
        }
        if n > best.1 {
            best = (o, n);
        }
        o += 8 * n.max(1);
    }
    shaders_at(r, best.0)
}

/// Decode consecutive pointers to "atem" records starting at this array.
fn shaders_at(r: &Resource, arr: usize) -> Vec<Shader> {
    (arr..r.sys.len().saturating_sub(7)).step_by(8)
        .map_while(|at| r.sys_off(r.u64(at)).filter(|&s| r.sys.get(s + 4..s + 8) == Some(b"atem")))
        .enumerate()
        .map(|(k, s)| {
            let mut sh = Shader { index: k, record_off: s as u64, name_hash: format!("{:08x}", r.u32(s)), param_hashes: vec![], textures: vec![], flags: r.sys.get(s + 0x38..s + 0x40).map(|b| b.to_vec()).unwrap_or_default(), constants: vec![] };
            if let Some(pt) = r.sys_off(r.u64(s + 0x20)) {
                let np = r.sys.get(pt + 1).copied().unwrap_or(0) as usize;
                sh.param_hashes = (0..np).map(|i| format!("{:08x}", r.u32(pt + 8 + 8 * i))).collect();
                if let Some(ta) = r.sys_off(r.u64(s + 0x10)) {
                    for i in 0..np {
                        let Some(t) = r.sys_off(r.u64(ta + 8 * i)) else { continue };
                        let Some(tr) = r.sys_off(r.u64(t + 0x30)) else { continue };
                        if r.u32(tr + 0x10) != 6 {
                            continue;
                        }
                        sh.textures.push(TexRef {
                            param: i,
                            param_hash: sh.param_hashes[i].clone(),
                            hash64: format!("{:016x}", r.u64(tr + 8)),
                            hash32: format!("{:08x}", r.u32(tr + 0x18)),
                        });
                    }
                }
                // constants: table entries after the textures (total count in byte 4 of the header)
                let total = r.sys.get(pt + 4).copied().unwrap_or(0) as usize;
                let block = r.sys_off(r.u64(s + 0x08)).and_then(|cb| r.sys_off(r.u64(s + 0x10)).filter(|&ta| ta > cb).map(|ta| (cb, (ta - cb) / 4)));
                for i in np..total.min(256) {
                    let v = r.u64(pt + 8 + 8 * i);
                    if v == 0 {
                        break;
                    }
                    let (h, d) = (v as u32, (v >> 32) as u32);
                    // descriptor 0x0040_XX_tt indexes this block (XX = float offset);
                    // others (0x01…, 0x0041…) point into buffers not read here
                    let offset = ((d >> 8) & 0xff) as usize;
                    let in_block = d >> 16 == 0x40;
                    let values = block.filter(|&(_, nf)| in_block && offset + 4 <= nf).map(|(cb, _)| [0, 1, 2, 3].map(|k| r.f32(cb + 4 * (offset + k))));
                    sh.constants.push(ShaderConst { hash: format!("{h:08x}"), desc: format!("{d:08x}"), offset, values });
                }
            }
            sh
        })
        .collect()
}

/// Find the models (geometry groups) and their shader index maps.
pub fn find_models(r: &Resource, geoms: &[Geometry]) -> Vec<Model> {
    use std::collections::HashMap;
    // Geometry records start 8 bytes before the blob pointer.
    let by_rec: HashMap<usize, usize> = geoms.iter().enumerate().map(|(i, g)| (g.record_off.saturating_sub(8) as usize, i)).collect();
    let mut out = Vec::new();
    let mut o = 0;
    while o + 0x30 <= r.sys.len() {
        let n = r.u16(o + 8) as usize;
        if n > 0 && n == r.u16(o + 10) as usize {
            if let (Some(a), Some(m)) = (r.sys_off(r.u64(o)), r.sys_off(r.u64(o + 0x18))) {
                let g: Option<Vec<usize>> = (0..n).map(|k| r.sys_off(r.u64(a + 8 * k)).and_then(|x| by_rec.get(&x).copied())).collect();
                if let Some(g) = g {
                    out.push(Model { member: None, struct_off: o as u64, geometries: g, shader_indices: (0..n).map(|k| r.u16(m + 2 * k)).collect(), lod: 0, part: false, bone_palette: model_palette(r, o) });
                }
            }
        }
        o += 8;
    }
    let members = dictionary_members(r);
    if !members.is_empty() {
        let model_at: HashMap<usize, usize> = out.iter().enumerate()
            .filter_map(|(i, m)| m.struct_off.checked_sub(8).map(|o| (o as usize, i))).collect();
        let mut occurrences = Vec::new();
        let mut covered = vec![false; out.len()];
        for (member, d) in members.iter().enumerate() {
            for lod in 0..4 {
                if let Some(models) = lod_models(r, d.root, lod) {
                    for offset in models {
                        if let Some(&i) = model_at.get(&offset) {
                            let mut model = out[i].clone();
                            model.member = Some(member as u32);
                            model.lod = lod;
                            model.part = false;
                            occurrences.push(model);
                            covered[i] = true;
                        }
                    }
                }
            }
        }
        // Uncovered models are embedded parts; dictionary tables determine LODs.
        for (i, mut model) in out.into_iter().enumerate() {
            if !covered[i] {
                model.part = true;
                occurrences.push(model);
            }
        }
        return occurrences;
    }
    // LOD table: model structs are referenced (at struct - 8) from arrays of
    // pointers; each array is referenced by a LOD entry.
    let mut arrays: Vec<(usize, Vec<usize>)> = Vec::new();
    let model_at: HashMap<u64, usize> = out.iter().enumerate().map(|(i, m)| (0x5000_0000 | (m.struct_off - 8), i)).collect();
    let mut o = 0;
    while o + 0x10 <= r.sys.len() {
        let n = r.u16(o + 8) as usize;
        if n > 0 && n == r.u16(o + 10) as usize {
            if let Some(a) = r.sys_off(r.u64(o)) {
                let ms: Option<Vec<usize>> = (0..n).map(|k| model_at.get(&r.u64(a + 8 * k)).copied()).collect();
                if let Some(ms) = ms {
                    arrays.push((o, ms));
                }
            }
        }
        o += 8;
    }
    // The main drawable's LOD group holds the model lists at +0x50 + 8*k
    // (k = 0..3: high, med, low, vlow). The main drawable is the root for
    // drawables and the struct at root+0x18 for fragments. Other model lists
    // belong to embedded drawables (fragment children at root+0x10, nested
    // sub-drawables): those models are parts, not LODs.
    let base = if find_resource_name(&r.sys).map_or(false, |t| t.1 == "ft") { r.sys_off(r.u64(0x18)) } else { Some(0) };
    let mut main_hit = false;
    if let Some(d) = base {
        let slot_of: HashMap<u64, usize> = (0..4).filter(|k| d + 0x58 + 8 * k <= r.sys.len()).map(|k| (r.u64(d + 0x50 + 8 * k), k)).filter(|x| x.0 != 0).collect();
        for (o, ms) in &arrays {
            let k = slot_of.get(&(0x5000_0000 | *o as u64)).copied();
            main_hit |= k.is_some();
            for &m in ms {
                match k {
                    Some(k) => out[m].lod = k,
                    None => out[m].part = true,
                }
            }
        }
    }
    if !main_hit {
        // layout not recognised (e.g. drawable dictionaries): rank by position
        arrays.sort_by_key(|a| a.0);
        for (lod, (_, ms)) in arrays.iter().enumerate() {
            for &m in ms {
                out[m].lod = lod;
                out[m].part = false;
            }
        }
    }
    out
}

#[cfg(test)]
mod dictionary_tests {
    use super::*;

    fn put64(r: &mut Resource, at: usize, value: u64) {
        r.sys[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn fixture() -> (Resource, Vec<Geometry>) {
        let mut r = Resource { sys: vec![0; 0x1000], gfx: vec![], gfx_base: 0 };
        let mut geoms = Vec::new();
        for (i, name) in ["alpha-01", "beta_02"].iter().enumerate() {
            let root = 0x100 + i * 0x100;
            let string = 0x800 + i * 0x20;
            let entry = 0x400 + i * 0x20;
            let array = 0x480 + i * 0x10;
            let model = 0x508 + i * 0x100;
            let geometry_array = 0x580 + i * 0x100;
            let shader_array = 0x590 + i * 0x100;
            let geometry = 0x700 + i * 0x20;
            let text = format!("{name}.#dr\0");
            r.sys[string..string + text.len()].copy_from_slice(text.as_bytes());
            put64(&mut r, root + 0xA8, 0x5000_0000 | string as u64);
            // Physical LOD slots matter, even with holes before them.
            put64(&mut r, root + 0x50 + if i == 0 { 16 } else { 0 }, 0x5000_0000 | entry as u64);
            put64(&mut r, entry, 0x5000_0000 | array as u64);
            // Spare capacity is permitted by the dictionary layout.
            r.sys[entry + 8..entry + 12].copy_from_slice(&[1, 0, 2, 0]);
            put64(&mut r, array, 0x5000_0000 | (model - 8) as u64);
            put64(&mut r, model, 0x5000_0000 | geometry_array as u64);
            r.sys[model + 8..model + 12].copy_from_slice(&[1, 0, 1, 0]);
            put64(&mut r, model + 0x18, 0x5000_0000 | shader_array as u64);
            put64(&mut r, geometry_array, 0x5000_0000 | geometry as u64);
            geoms.push(Geometry { vertex_buffer: i, index_buffer: i, record_off: (geometry + 8) as u64 });
        }
        (r, geoms)
    }

    #[test]
    fn members_and_models_follow_each_roots_lod_slots() {
        let (r, geoms) = fixture();
        assert_eq!(dictionary_members(&r), vec![
            DictMember { name: "alpha-01".into(), root: 0x100 },
            DictMember { name: "beta_02".into(), root: 0x200 },
        ]);
        let models = find_models(&r, &geoms);
        assert_eq!(models.len(), 2);
        assert_eq!((models[0].member, models[0].lod, models[0].part), (Some(0), 2, false));
        assert_eq!((models[1].member, models[1].lod, models[1].part), (Some(1), 0, false));
        assert_eq!(describe(&r).members, ["alpha-01", "beta_02"]);
    }

    #[test]
    fn shared_models_keep_each_member_and_lod_occurrence() {
        let (mut r, geoms) = fixture();
        // Both members and two slots of alpha reference the same model.
        put64(&mut r, 0x490, 0x5000_0500);
        put64(&mut r, 0x168, 0x5000_0400);
        let models = find_models(&r, &geoms);
        let occurrences: Vec<_> = models.iter().filter(|m| m.struct_off == 0x508)
            .map(|m| (m.member, m.lod, m.part, m.geometries.clone())).collect();
        assert_eq!(occurrences, [
            (Some(0), 2, false, vec![0]),
            (Some(0), 3, false, vec![0]),
            (Some(1), 0, false, vec![0]),
        ]);
        assert!(models.iter().any(|m| m.struct_off == 0x608 && m.member.is_none() && m.part));
        for (name, lods) in [("alpha-01", vec![2, 3]), ("beta_02", vec![0])] {
            let mut info = describe(&r);
            info.models = models.clone();
            info.geometries = geoms.clone();
            info.buffers = (0..2).map(|i| GfxBuffer { struct_off: i, gfx_off: i, bytes: 0, flags: 0, kind_hint: 8, mesh_blob: None }).collect();
            select_member(&r, &mut info, name).unwrap();
            assert_eq!(info.models.iter().map(|m| m.lod).collect::<Vec<_>>(), lods);
            assert!(info.models.iter().all(|m| m.geometries == [0]));
        }
    }

    #[test]
    fn named_nested_drawables_are_parts_of_an_ordinary_drawable() {
        let (mut r, geoms) = fixture();
        // The resource root is itself a named drawable, with beta nested inside.
        put64(&mut r, 0xA8, 0x5000_0800);
        put64(&mut r, 0x58, 0x5000_0400);
        for entry in [0x400, 0x420] {
            r.sys[entry + 10..entry + 12].copy_from_slice(&1u16.to_le_bytes());
        }
        assert!(dictionary_members(&r).is_empty());
        assert!(describe(&r).members.is_empty());
        let models = find_models(&r, &geoms);
        assert_eq!((models[0].member, models[0].lod, models[0].part), (None, 1, false));
        assert_eq!((models[1].member, models[1].part), (None, true));
    }

    #[test]
    fn plain_drawable_and_fragment_keep_the_main_lod_and_embedded_parts() {
        let (mut r, geoms) = fixture();
        // A single named drawable is not treated as a dictionary.
        put64(&mut r, 0x2A8, 0);
        for entry in [0x400, 0x420] {
            r.sys[entry + 10..entry + 12].copy_from_slice(&1u16.to_le_bytes());
        }
        put64(&mut r, 0x58, 0x5000_0400);
        assert!(dictionary_members(&r).is_empty());
        let models = find_models(&r, &geoms);
        assert_eq!((models[0].member, models[0].lod, models[0].part), (None, 1, false));
        assert!(models[1].part);
        // The fragment's main drawable is reached through root + 0x18.
        r.sys[0x80a..0x80c].copy_from_slice(b"ft");
        put64(&mut r, 0x18, 0x5000_0100);
        let models = find_models(&r, &geoms);
        assert_eq!((models[0].member, models[0].lod, models[0].part), (None, 2, false));
        assert!(models[1].part);
    }

    #[test]
    fn member_bounds_prefer_own_geometry_even_without_lod_zero() {
        let (mut r, _) = fixture();
        // One float-position vertex buffer and its paired index buffer for alpha.
        r.gfx = vec![0; 0x94];
        r.gfx[0..4].copy_from_slice(&0x3063u32.to_le_bytes());
        r.gfx[4..8].copy_from_slice(&0x4201u32.to_le_bytes()); // next stream at 0x84: three vertices
        for (i, p) in [[-8.0f32, -2.0, 1.0], [6.0, 4.0, 3.0], [0.0, 0.0, 0.0]].iter().enumerate() {
            for (k, v) in p.iter().enumerate() { r.gfx[0x60 + i * 12 + k * 4..0x64 + i * 12 + k * 4].copy_from_slice(&v.to_le_bytes()); }
        }
        for (offset, gfx_offset, words) in [(0x900, 0, 0x24u32), (0x920, 0x90, 1)] {
            r.sys[offset + 8..offset + 12].copy_from_slice(&words.to_le_bytes());
            r.sys[offset + 12..offset + 16].copy_from_slice(&[4, 0, 8, 0]);
            put64(&mut r, offset + 0x18, 0x6000_0000 | gfx_offset);
        }
        put64(&mut r, 0x708, 0x5000_0900);
        put64(&mut r, 0x740, 0x5000_0b00);
        put64(&mut r, 0xb00, 0x5000_0920);
        // The other member has no geometry; it must not borrow alpha's bounds.
        assert_eq!(model_bounds(&r, Some("alpha-01")), Some([[-8.0, -2.0, 0.0], [6.0, 4.0, 3.0]]));
        assert!(model_bounds(&r, Some("beta_02")).is_none());
    }

    #[test]
    fn member_selection_uses_its_own_shader_array_and_textures() {
        let (mut r, geoms) = fixture();
        r.sys.resize(0x1400, 0);
        for (i, texture) in ["alpha_texture", "beta_texture"].iter().enumerate() {
            let base = 0x1000 + i * 0x200;
            put64(&mut r, 0x100 + i * 0x100 + 0x10, 0x5000_0000 | base as u64);
            put64(&mut r, base + 0x10, 0x5000_0000 | (base + 0x20) as u64);
            put64(&mut r, base + 0x20, 0x5000_0000 | (base + 0x40) as u64);
            // Beta has the longest run, so the global heuristic chooses it.
            if i == 1 {
                put64(&mut r, base + 0x28, 0x5000_0000 | (base + 0x80) as u64);
                r.sys[base + 0x84..base + 0x88].copy_from_slice(b"atem");
            }
            r.sys[base + 0x40..base + 0x44].copy_from_slice(&(0xa000_0000u32 + i as u32).to_le_bytes());
            r.sys[base + 0x44..base + 0x48].copy_from_slice(b"atem");
            put64(&mut r, base + 0x50, 0x5000_0000 | (base + 0xe0) as u64);
            put64(&mut r, base + 0x60, 0x5000_0000 | (base + 0xc0) as u64);
            r.sys[base + 0xc1] = 1;
            r.sys[base + 0xc8..base + 0xcc].copy_from_slice(&0x12345678u32.to_le_bytes());
            put64(&mut r, base + 0xe0, 0x5000_0000 | (base + 0x100) as u64);
            put64(&mut r, base + 0x130, 0x5000_0000 | (base + 0x140) as u64);
            r.sys[base + 0x150..base + 0x154].copy_from_slice(&6u32.to_le_bytes());
            r.sys[base + 0x158..base + 0x15c].copy_from_slice(&crate::meta7a::joaat(texture).to_le_bytes());
        }
        let mut original = describe(&r);
        assert_eq!(original.shaders.len(), 2);
        assert_eq!(original.shaders[0].name_hash, "a0000001");
        original.models = find_models(&r, &geoms);
        original.geometries = geoms;
        original.buffers = (0..2).map(|i| GfxBuffer { struct_off: i, gfx_off: i, bytes: 0, flags: 0, kind_hint: 8, mesh_blob: None }).collect();
        original.embedded_textures = ["alpha_texture", "beta_texture", "", ""].iter().enumerate().map(|(i, name)| crate::texture::TexInfo {
            name: name.to_string(), width: 1, height: 1, depth: 1, mips: 1, dxgi_format: 28,
            format: String::new(), struct_off: if i < 2 { 0 } else { 0x1100 + (i as u64 - 2) * 0x200 },
            data_off: 0, size_field: 0, swizzle: String::new(),
        }).collect();
        for (i, name) in ["alpha-01", "beta_02"].iter().enumerate() {
            let mut info = original.clone();
            select_member(&r, &mut info, name).unwrap();
            assert_eq!(info.shaders.len(), 1);
            assert_eq!(info.shaders[0].index, 0);
            assert_eq!(info.shaders[0].record_off, 0x1040 + i as u64 * 0x200);
            assert_eq!(info.shaders[0].name_hash, format!("{:08x}", 0xa000_0000u32 + i as u32));
            let texture = ["alpha_texture", "beta_texture"][i];
            assert_eq!(info.shaders[0].textures.len(), 1);
            assert_eq!(info.shaders[0].textures[0].hash32, format!("{:08x}", crate::meta7a::joaat(texture)));
            assert_eq!(info.models[0].shader_indices, [0]);
            assert_eq!(info.embedded_textures.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), [texture, ""]);
            assert_eq!(info.embedded_textures[1].struct_off, 0x1100 + i as u64 * 0x200);
        }
        // Unrecognised member layouts retain the resource-wide fallback.
        for (group, array) in [(0, 0), (0x5000_1000, 0), (0x5000_1000, 0x5000_1100), (0x5000_1000, 0x5000_13fc)] {
            put64(&mut r, 0x110, group);
            put64(&mut r, 0x1010, array);
            let mut info = original.clone();
            select_member(&r, &mut info, "alpha-01").unwrap();
            assert_eq!(info.shaders[0].name_hash, "a0000001");
            assert_eq!(info.embedded_textures.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["beta_texture", ""]);
        }
    }

    #[test]
    fn member_selection_scopes_assets_and_validates_bounds() {
        let (mut r, mut geoms) = fixture();
        for (o, value) in [(0x130, -2.0f32), (0x134, -3.0), (0x138, -4.0), (0x140, 2.0), (0x144, 3.0), (0x148, 4.0)] {
            r.sys[o..o + 4].copy_from_slice(&value.to_le_bytes());
        }
        let mut info = describe(&r);
        info.models = find_models(&r, &geoms);
        info.models[0].shader_indices = vec![2];
        geoms[0].vertex_buffer = 2; geoms[0].index_buffer = 3;
        geoms[1].vertex_buffer = 0; geoms[1].index_buffer = 1;
        info.geometries = geoms;
        info.buffers = (0..4).map(|i| GfxBuffer { struct_off: i, gfx_off: i, bytes: 0, flags: 0, kind_hint: 8, mesh_blob: None }).collect();
        info.shaders = (0..3).map(|i| Shader { index: i, record_off: i as u64, name_hash: format!("shader{i}"), param_hashes: vec![], textures: vec![], flags: vec![], constants: vec![] }).collect();
        info.shaders[2].textures.push(TexRef { param: 0, param_hash: String::new(), hash64: String::new(), hash32: format!("{:08x}", crate::meta7a::joaat("own")) });
        info.embedded_textures = ["other", "own"].iter().map(|name| crate::texture::TexInfo {
            name: name.to_string(), width: 1, height: 1, depth: 1, mips: 1, dxgi_format: 28,
            format: String::new(), struct_off: 0, data_off: 0, size_field: 0, swizzle: String::new(),
        }).collect();
        info.shaders[2].record_off = 0xc00;
        info.shaders[2].param_hashes.push("embedded".into());
        put64(&mut r, 0xc10, 0x5000_0c80);
        put64(&mut r, 0xc80, 0x5000_0d00);
        let mut unnamed = info.embedded_textures[0].clone();
        unnamed.name.clear(); unnamed.struct_off = 0xd00;
        info.embedded_textures.push(unnamed);
        select_member(&r, &mut info, "alpha-01").unwrap();
        assert_eq!(info.models.len(), 1);
        assert_eq!(info.models[0].shader_indices, [0]);
        assert_eq!(info.models[0].geometries, [0]);
        assert_eq!(info.shaders.len(), 1);
        assert_eq!((info.shaders[0].index, info.shaders[0].name_hash.as_str()), (0, "shader2"));
        assert_eq!(info.buffers.iter().map(|b| b.gfx_off).collect::<Vec<_>>(), [2, 3]);
        assert_eq!((info.geometries[0].vertex_buffer, info.geometries[0].index_buffer), (0, 1));
        assert_eq!(info.embedded_textures.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["own", ""]);
        assert_eq!(info.bounds, Some([[-2.0, -3.0, -4.0], [2.0, 3.0, 4.0]]));
        r.sys[0x140..0x144].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(model_bounds(&r, Some("alpha-01")).is_none());
    }

    #[test]
    fn member_names_require_nul_and_fragments_never_have_members() {
        let (mut r, _) = fixture();
        r.sys[0x80c] = b'x'; // alpha-01.#dr must end here
        assert!(dictionary_members(&r).is_empty());
        let (mut r, _) = fixture();
        r.sys[0x40..0x4c].copy_from_slice(b"fragment.#ft");
        assert!(dictionary_members(&r).is_empty());
    }

    #[test]
    fn invalid_arrays_single_members_and_duplicate_names_are_rejected() {
        let (mut r, _) = fixture();
        // A second valid root referencing the same name does not add a member.
        put64(&mut r, 0x3A8, 0x5000_0800);
        put64(&mut r, 0x350, 0x5000_0400);
        assert_eq!(dictionary_members(&r).len(), 2);
        // Array starts in sys but extends beyond its end.
        put64(&mut r, 0x420, 0x5000_0ffc);
        assert!(dictionary_members(&r).is_empty());
        put64(&mut r, 0x420, 0x5000_0490);
        // Every array element must be a valid system pointer.
        put64(&mut r, 0x490, 0x6000_0600);
        assert!(dictionary_members(&r).is_empty());
        put64(&mut r, 0x490, 0x5000_0600);
        r.sys[0x428..0x42a].copy_from_slice(&64u16.to_le_bytes());
        assert!(dictionary_members(&r).is_empty());
        assert!(dictionary_members(&Resource { sys: vec![], gfx: vec![], gfx_base: 0 }).is_empty());
    }
}

#[cfg(test)]
mod meshlet_tests {
    use super::*;

    #[test]
    fn meshlets_with_32_bit_counts() {
        // two meshlets; the second one's remap start (65,636) wraps in its
        // u16 field, and the remap count does not fit 16 bits
        let nremap: u32 = 65_639;
        let mut ib = vec![0u8; 0x0c + 32];
        ib[0..2].copy_from_slice(&2u16.to_le_bytes());
        ib[4..8].copy_from_slice(&1u32.to_le_bytes());
        ib[8..12].copy_from_slice(&nremap.to_le_bytes());
        ib[0x10..0x12].copy_from_slice(&65_000u16.to_le_bytes());
        ib[0x20..0x22].copy_from_slice(&100u16.to_le_bytes());
        // one triangle in meshlet 1: local indices 0, 1, 2
        ib.extend_from_slice(&((1u32 << 21) | (2 << 14) | (1 << 7)).to_le_bytes());
        let mut remap = vec![0u16; nremap as usize];
        remap[65_636..].copy_from_slice(&[5, 6, 7]);
        ib.extend(remap.iter().flat_map(|v| v.to_le_bytes()));
        assert_eq!(decode_meshlets(&ib, 10), (vec![5, 6, 7], 0));
    }
}
