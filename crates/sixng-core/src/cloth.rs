//! Verlet cloth and soft bodies: unnamed `packs.rpf` resources with system
//! byte 0x1A = 2 (flags, canopies, awnings, tarps, garment pieces). The
//! soft-body variant (u32 0x1C = 0x7d8f3db3) uses the same layout and sits
//! around ped bodies.
//!
//! System page: {ptr, u16 n, u16 cap} at 0x28 = n particles of 4 x f32
//! (x, y, z, w; model space, Z-up metres; w per particle, meaning unknown,
//! probably mass-related); {ptr, u16 m, u16 cap} at 0x38 = m edges of
//! {u16 a, u16 b, f32 rest length, f32, u32 flags}. A second particle /
//! edge pair at 0x2C8 / 0x2D8 holds another, smaller set (partly the same
//! points). The rest of the page is not decoded.
//!
//! Check: an edge's rest length equals the distance between its particles
//! in the stored pose on most edges (stretch constraints); the others are
//! bend / shear constraints or a different rest pose.
use crate::resource::Resource;
use anyhow::{bail, Result};

pub const TYPE_CLOTH: u8 = 2;
/// Byte 0x1A of the box-volume resources (same tag at 0x1C as soft bodies).
pub const TYPE_VOLUMES: u8 = 5;

/// One oriented box of a volume set.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BoxVolume {
    /// Centre (model space, Z-up metres).
    pub centre: [f32; 3],
    /// Rotation quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    /// Half extents along the box's own axes.
    pub half: [f32; 3],
}

/// Box volumes stored next to the cloth resources of a map pack (8 in the
/// minigame pack, beside its 28 cloth pieces): system byte 0x1A = 5 and the
/// soft-body tag at 0x1C. Layout: u32 count at 0x28, ptr at 0x30 to
/// `count` entries of 0x90 bytes (+0x00 vec3 centre, +0x10 quaternion,
/// +0x50 vec3 half extents, +0x5C u32 70, +0x60 / +0x70 vec3 -half / +half);
/// u32 group count at 0x3C and ptr at 0x40 to a hash map: 16 control bytes
/// (low 7 bits of the key, 0x80 empty) then `groups` slots of 24 bytes
/// {u32 key hash, u32 0, u32 n, u32 n, ptr u16 box indices}. The group keys
/// do not resolve to names. Box sizes (1.4 x 3.3 x 1.9 m, 0.5 x 0.5 x 1 m)
/// look like vehicle- and person-sized volumes; what they drive is unknown.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Volumes {
    pub boxes: Vec<BoxVolume>,
    /// (key hash, box indices)
    pub groups: Vec<(String, Vec<u16>)>,
}

pub fn is_volumes(sys: &[u8]) -> bool {
    sys.len() >= 0x48 && sys[0x1A] == TYPE_VOLUMES && u32::from_le_bytes(sys[0x1C..0x20].try_into().unwrap()) == SOFT_TAG
}

pub fn parse_volumes(r: &Resource) -> Result<Volumes> {
    if !is_volumes(&r.sys) {
        bail!("not a volume resource");
    }
    let n = r.u32(0x28) as usize;
    let Some(arr) = r.sys_off(r.u64(0x30)) else { bail!("box array pointer outside the system page") };
    if n == 0 || n > 4096 || arr + 0x90 * n > r.sys.len() {
        bail!("bad box count {n}");
    }
    let f3 = |o: usize| [r.f32(o), r.f32(o + 4), r.f32(o + 8)];
    let boxes: Vec<BoxVolume> = (0..n)
        .map(|i| {
            let e = arr + 0x90 * i;
            BoxVolume { centre: f3(e), rotation: [r.f32(e + 0x10), r.f32(e + 0x14), r.f32(e + 0x18), r.f32(e + 0x1C)], half: f3(e + 0x50) }
        })
        .collect();
    if boxes.iter().any(|b| {
        let q = b.rotation.iter().map(|x| x * x).sum::<f32>();
        (q - 1.0).abs() > 1e-2 || !b.centre.iter().chain(&b.half).all(|v| v.is_finite() && v.abs() < 1e4)
    }) {
        bail!("box not a unit rotation / finite size");
    }
    let mut groups = Vec::new();
    if let Some(m) = r.sys_off(r.u64(0x40)) {
        for k in 0..(r.u32(0x3C) as usize).min(1024) {
            let e = m + 16 + 24 * k;
            if e + 24 > r.sys.len() {
                break;
            }
            let cnt = r.u32(e + 8) as usize;
            let idx: Vec<u16> = match r.sys_off(r.u64(e + 16)) {
                Some(l) if l + 2 * cnt <= r.sys.len() && cnt <= n => (0..cnt).map(|j| r.u16(l + 2 * j)).collect(),
                _ => Vec::new(),
            };
            groups.push((format!("{:08x}", r.u32(e)), idx));
        }
    }
    Ok(Volumes { boxes, groups })
}

impl Volumes {
    /// Wireframe GLB: the 12 edges of every box.
    pub fn glb(&self, name: &str, extras: serde_json::Value) -> Vec<u8> {
        let mut pos = Vec::new();
        let mut seg = Vec::new();
        for b in &self.boxes {
            let [x, y, z, w] = b.rotation;
            let rot = |v: [f32; 3]| {
                // v + 2 w (q x v) + 2 q x (q x v)
                let q = [x, y, z];
                let c = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
                let t = c(q, v).map(|k| 2.0 * k);
                let u = c(q, t);
                [0, 1, 2].map(|k| v[k] + w * t[k] + u[k])
            };
            let base = pos.len() as u32;
            for i in 0..8u32 {
                let s = |bit: u32, h: f32| if i >> bit & 1 == 1 { h } else { -h };
                let p = rot([s(0, b.half[0]), s(1, b.half[1]), s(2, b.half[2])]);
                pos.push([b.centre[0] + p[0], b.centre[1] + p[1], b.centre[2] + p[2]]);
            }
            for i in 0..8u32 {
                for bit in 0..3 {
                    if i >> bit & 1 == 0 {
                        seg.push(base + i);
                        seg.push(base + (i | 1 << bit));
                    }
                }
            }
        }
        crate::gltf::lines_glb(name, &pos, &[], &[], &seg, &[], extras)
    }
}
pub const SOFT_TAG: u32 = 0x7d8f_3db3;

pub struct Edge {
    pub a: u16,
    pub b: u16,
    pub rest: f32,
    pub k: f32,
    pub flags: u32,
}

pub struct Set {
    /// x, y, z, w
    pub particles: Vec<[f32; 4]>,
    pub edges: Vec<Edge>,
}

impl Set {
    /// Edges whose rest length equals the particle distance (1 mm + 0.1 %).
    pub fn rest_matches(&self) -> usize {
        self.edges
            .iter()
            .filter(|e| {
                let (p, q) = (self.particles[e.a as usize], self.particles[e.b as usize]);
                let d = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
                (d - e.rest).abs() <= 1e-3 + 1e-3 * e.rest
            })
            .count()
    }
}

pub struct Cloth {
    pub soft: bool,
    pub sets: Vec<Set>,
    /// Hashes in the system page that resolve to known names (garments,
    /// tuning profiles, bones), with their offsets.
    pub names: Vec<(usize, String)>,
}

/// Cheap test on the system page alone.
pub fn is_cloth(sys: &[u8]) -> bool {
    if sys.len() < 0x48 || sys[0x1A] != TYPE_CLOTH {
        return false;
    }
    let tag = u32::from_le_bytes(sys[0x1C..0x20].try_into().unwrap());
    tag == 0 || tag == SOFT_TAG
}

fn read_set(r: &Resource, at: usize) -> Result<Option<Set>> {
    if r.sys.len() < at + 0x1C {
        return Ok(None);
    }
    let (pp, n, ep, m) = (r.u64(at), r.u16(at + 8) as usize, r.u64(at + 0x10), r.u16(at + 0x18) as usize);
    if n == 0 {
        return Ok(None);
    }
    let (Some(po), Some(eo)) = (r.sys_off(pp), if m == 0 { Some(0) } else { r.sys_off(ep) }) else {
        bail!("particle / edge pointer outside the system page")
    };
    if po + 16 * n > r.sys.len() || eo + 16 * m > r.sys.len() {
        bail!("particle / edge array past the system page");
    }
    let particles: Vec<[f32; 4]> = (0..n).map(|i| [r.f32(po + 16 * i), r.f32(po + 16 * i + 4), r.f32(po + 16 * i + 8), r.f32(po + 16 * i + 12)]).collect();
    let edges: Vec<Edge> = (0..m)
        .map(|i| {
            let o = eo + 16 * i;
            Edge { a: r.u16(o), b: r.u16(o + 2), rest: r.f32(o + 4), k: r.f32(o + 8), flags: r.u32(o + 12) }
        })
        .collect();
    if edges.iter().any(|e| e.a as usize >= n || e.b as usize >= n) {
        bail!("edge index past the particle count");
    }
    if particles.iter().any(|p| !p[..3].iter().all(|v| v.is_finite() && v.abs() < 1e4)) {
        bail!("particle position not finite");
    }
    Ok(Some(Set { particles, edges }))
}

/// Decode a cloth / soft body from its system page.
pub fn parse(r: &Resource, names: &std::collections::HashMap<u32, String>) -> Result<Cloth> {
    if !is_cloth(&r.sys) {
        bail!("not a cloth resource");
    }
    let mut sets = Vec::new();
    match read_set(r, 0x28)? {
        Some(s) => sets.push(s),
        None => bail!("no particles"),
    }
    // second set: optional, ignored when it does not decode
    if let Ok(Some(s)) = read_set(r, 0x2C8) {
        sets.push(s);
    }
    let mut found = Vec::new();
    for o in (0..r.sys.len().saturating_sub(3)).step_by(4) {
        let h = r.u32(o);
        if h < 0x1000 || h == SOFT_TAG {
            continue;
        }
        if let Some(n) = names.get(&h).cloned().or_else(|| crate::names::lookup(h, crate::names::Ctx::Field).filter(|n| n.len() > 3)) {
            found.push((o, n));
        }
    }
    Ok(Cloth { soft: r.u32(0x1C) == SOFT_TAG, sets, names: found })
}

impl Cloth {
    /// Line GLB: the edges of every set (one strand mesh; the extras give
    /// where each set starts), plus particle w as `_W`.
    pub fn glb(&self, name: &str, extras: serde_json::Value) -> Vec<u8> {
        let mut pos = Vec::new();
        let mut seg = Vec::new();
        for s in &self.sets {
            let base = pos.len() as u32;
            pos.extend(s.particles.iter().map(|p| [p[0], p[1], p[2]]));
            for e in &s.edges {
                seg.push(base + e.a as u32);
                seg.push(base + e.b as u32);
            }
        }
        crate::gltf::lines_glb(name, &pos, &[], &[], &seg, &[], extras)
    }

    pub fn bbox(&self) -> [[f32; 3]; 2] {
        let mut b = [[f32::MAX; 3], [f32::MIN; 3]];
        for p in self.sets.iter().flat_map(|s| &s.particles) {
            for a in 0..3 {
                b[0][a] = b[0][a].min(p[a]);
                b[1][a] = b[1][a].max(p[a]);
            }
        }
        b
    }
}

/// Export every cloth / soft body to `<out>/cloth/<archive>/<offset>.glb`
/// (edges as lines) plus `index.jsonl`. Returns (exported, errors).
pub fn export_cloth(a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry], only: &[u64], out: &std::path::Path, progress: crate::export::Progress) -> Result<(usize, usize)> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    let names = crate::groom::bone_names(a, all);
    let cands: Vec<&crate::scan::Entry> = all.iter().filter(|e| e.name.is_none() && e.kind == crate::classify::Kind::RageObj && (only.is_empty() || only.contains(&e.off))).collect();
    let dir = out.join("cloth").join(a.id.trim_end_matches(".rpf"));
    std::fs::create_dir_all(&dir)?;
    let total = cands.len() as u64;
    let done = AtomicU64::new(0);
    let errors = AtomicUsize::new(0);
    let mut rows: Vec<(u64, String, bool)> = cands
        .par_iter()
        .filter_map(|e| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if d % 256 == 0 || d == total {
                progress(&e.id, d, total);
            }
            let r = Resource::load_sys(&a.path, e).ok()?;
            if is_volumes(&r.sys) {
                let v = match parse_volumes(&r) {
                    Ok(v) => v,
                    Err(x) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        return Some((e.off, serde_json::json!({"id": e.id, "error": x.to_string()}).to_string(), false));
                    }
                };
                let info = serde_json::json!({"id": e.id, "file": format!("{:x}.glb", e.off), "kind": "box volumes", "boxes": v.boxes, "groups": v.groups});
                let extras = serde_json::json!({"source": e.id, "kind": "box volumes", "groups": v.groups,
                    "coordinate_system": "model space (Z-up metres) converted to glTF Y-up (x, z, -y)", "note": "oriented boxes drawn as their 12 edges"});
                let p = dir.join(format!("{:x}.glb", e.off));
                crate::atomic::write(&p, v.glb(&format!("volumes_{:x}", e.off), extras)).ok()?;
                return Some((e.off, info.to_string(), true));
            }
            if !is_cloth(&r.sys) {
                return None;
            }
            let c = match parse(&r, &names) {
                Ok(c) => c,
                Err(x) => {
                    errors.fetch_add(1, Ordering::Relaxed);
                    return Some((e.off, serde_json::json!({"id": e.id, "error": x.to_string()}).to_string(), false));
                }
            };
            let sets: Vec<serde_json::Value> = c
                .sets
                .iter()
                .map(|s| {
                    let (wmin, wmax) = s.particles.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[3]), b.max(p[3])));
                    serde_json::json!({"particles": s.particles.len(), "edges": s.edges.len(), "rest_length_matches": s.rest_matches(), "w_range": [wmin, wmax]})
                })
                .collect();
            let mut nm: Vec<&str> = c.names.iter().map(|(_, n)| n.as_str()).collect();
            nm.sort();
            nm.dedup();
            let info = serde_json::json!({"id": e.id, "file": format!("{:x}.glb", e.off), "kind": if c.soft { "soft body" } else { "cloth" }, "sets": sets, "bbox": c.bbox(), "names": nm});
            let extras = serde_json::json!({"source": e.id, "kind": info["kind"], "sets": info["sets"], "names": nm,
                "coordinate_system": "model space (Z-up metres) converted to glTF Y-up (x, z, -y)",
                "note": "edges of the verlet constraint graph drawn as lines; each set's particles follow the previous set's"});
            let p = dir.join(format!("{:x}.glb", e.off));
            if let Err(x) = crate::atomic::write(&p, c.glb(&format!("cloth_{:x}", e.off), extras)) {
                errors.fetch_add(1, Ordering::Relaxed);
                return Some((e.off, serde_json::json!({"id": e.id, "error": x.to_string()}).to_string(), false));
            }
            Some((e.off, info.to_string(), true))
        })
        .collect();
    rows.sort_by_key(|r| r.0);
    let ok = rows.iter().filter(|r| r.2).count();
    let mut s = String::new();
    for (_, line, _) in &rows {
        s.push_str(line);
        s.push('\n');
    }
    crate::atomic::write(dir.join("index.jsonl"), s)?;
    Ok((ok, errors.load(Ordering::Relaxed)))
}
