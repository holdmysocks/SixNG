//! A whole vehicle as one GLB: its model with the parts of one of its
//! configurations fitted (`sixng vehicle`).
//!
//! What a configuration fits is in `vehicles.json` (`vehicles`): its part
//! names, the models of each part's elements with the bone they are
//! modelled around and the stock bones they replace, and the switches that
//! pick the stock alternatives of the fitted parts. The same rules as the
//! viewer's:
//!
//! - an element's model is found by its asset id (a vehicle pack's own
//!   parts) or by the hash of its name (library parts);
//! - a part goes to the bind pose of its bone and is skinned to it; an
//!   element whose bone the vehicle does not have is left out (a part lists
//!   a model per chassis length);
//! - a body behind the cab (slot `1c9dd6fe`: pickup bed, flatbed, box) is
//!   fitted first and brings bones other parts mount on;
//! - triangles of the vehicle on a bone a fitted element replaces are
//!   dropped;
//! - every piece takes the vehicle's part colours for the configuration,
//!   and the primary paint takes the configuration's colour.
//!
//! Wheels: the configuration's rim and tyre models on every wheel bone, at
//! the centre and radius of the wheel's collision disc, mirrored on the
//! right. Rim and tyre are scaled alike (to the tyre's radius), so the
//! rim's share of the wheel is the models' own, not the configuration's
//! `rim_ratio`. Brake discs and calipers are not fitted.

use crate::export::{ExportOptions, ExportedItem};
use crate::game::ArchiveInfo;
use crate::gltf::{Joint, NodeGroup, TriPrim};
use crate::meta7a::{asset_id, joaat};
use crate::scan::{ArchiveIndex, Entry};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

type Mat = [f32; 16];
const IDENTITY: Mat = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
/// The slot of the part that is always fitted; its elements are the stock
/// alternatives the switches choose from.
const SLOT_CHASSIS: &str = "d418625e";
/// Whether a material (named `..._<shader hash>`) is of a shared rim or
/// tyre model.
fn wheel_material(name: &str) -> bool {
    name.len() >= 8 && name.get(name.len() - 8..).is_some_and(|h| h == "c5cdbdd2" || crate::material::is_tyre_shader(h))
}
/// The slot of a body behind the cab.
const SLOT_BODY: &str = "1c9dd6fe";

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

fn point(m: &Mat, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

fn dir(m: &Mat, p: [f32; 3]) -> [f32; 3] {
    let v: [f32; 3] = std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2]);
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-12 { [v[0] / l, v[1] / l, v[2] / l] } else { v }
}

/// Move a piece by `m` (a rotation, translation and scale; a mirror when
/// `flip`, which also turns the triangles round).
fn transform(prims: &mut [TriPrim], m: &Mat, flip: bool) {
    for p in prims {
        for v in &mut p.positions {
            *v = point(m, *v);
        }
        for n in p.normals.iter_mut().flatten() {
            *n = dir(m, *n);
        }
        for t in p.tangents.iter_mut().flatten() {
            let d = dir(m, [t[0], t[1], t[2]]);
            *t = [d[0], d[1], d[2], if flip { -t[3] } else { t[3] }];
        }
        if flip {
            for t in p.indices.chunks_exact_mut(3) {
                t.swap(1, 2);
            }
        }
    }
}

/// Skin a piece rigidly to one joint (or to none).
fn pin(prims: &mut [TriPrim], joint: Option<usize>) {
    for p in prims {
        let n = p.positions.len();
        p.joints = joint.map(|j| vec![[j as u16, 0, 0, 0]; n]);
        p.weights = joint.map(|_| vec![[255, 0, 0, 0]; n]);
        p.joints1 = None;
        p.weights1 = None;
    }
}

/// The brake slots, by joaat of their names: `brakediscs`,
/// `brakediscs_rear`, `calipers_front`, `calipers_rear`. A part in one of
/// them has one element that names a `brakedisc_*`, `drumbrake_*` or
/// `caliper_*` model (bone 0).
pub const BRAKE_SLOTS: [u32; 4] = [0x814a_6407, 0xee55_7cc2, 0x98b9_b49a, 0xc98d_bea6];

/// Brake models per axle, `[front, rear]`: (disc or drum, caliper), as
/// model names without `_hi`.
pub type AxleBrakes = [(Option<String>, Option<String>); 2];

/// The brakes of a configuration per axle. `named_*`: the disc (or drum)
/// and caliper its part list names by model; `slot_models`: the models of
/// its parts in the brake slots. The parts named by model and the slots
/// `brakediscs` / `calipers_front` are the front axle's (a model name and
/// a `brakediscs` part never come together; where a caliper comes both
/// ways the name is taken, which is not confirmed). The rear axle takes
/// the `_rear` slots, else what the front has; a rear drum without a rear
/// caliper part gets no caliper.
pub fn brake_axles(named_disc: Option<&str>, named_caliper: Option<&str>, slot_models: &[(u32, String)]) -> AxleBrakes {
    let of = |slot: u32| slot_models.iter().find(|(s, _)| *s == slot).map(|(_, n)| n.clone());
    let front_disc = named_disc.map(str::to_string).or_else(|| of(BRAKE_SLOTS[0]));
    let front_caliper = named_caliper.map(str::to_string).or_else(|| of(BRAKE_SLOTS[2]));
    let rear_disc = of(BRAKE_SLOTS[1]).or_else(|| front_disc.clone());
    let drum = rear_disc.as_deref().is_some_and(|d| d.starts_with("drumbrake_"));
    let rear_caliper = of(BRAKE_SLOTS[3]).or_else(|| if drum { None } else { front_caliper.clone() });
    [(front_disc, front_caliper), (rear_disc, rear_caliper)]
}

/// The rim light slots, by joaat of their names: `lights_rims`,
/// `lights_rims_rear`. A part in one of them has one element (bone 0) that
/// names `caliper_rimlight` or `drumbrake_rimlight`: a ring of lights on
/// the brake, modelled around the same unit disc as the discs and calipers
/// (ring radius 0.607 for a disc of 0.5), the drum's one set further in.
pub const RIM_LIGHT_SLOTS: [u32; 2] = [0x439f_ddac, 0x2ba6_699f];

/// The rim light ring of a configuration per axle, `[front, rear]`, from
/// the models of its parts in the rim light slots. The rear axle takes
/// `lights_rims_rear`, else what the front has, as the brake discs do
/// (not confirmed for the rings).
pub fn rim_light_axles(slot_models: &[(u32, String)]) -> [Option<String>; 2] {
    let of = |slot: u32| slot_models.iter().find(|(s, _)| *s == slot).map(|(_, n)| n.clone());
    let front = of(RIM_LIGHT_SLOTS[0]);
    let rear = of(RIM_LIGHT_SLOTS[1]).or_else(|| front.clone());
    [front, rear]
}

fn hex32(v: &Value) -> Option<u32> {
    u32::from_str_radix(v.as_str()?, 16).ok()
}

/// The name of a configuration part: 8 hex digits, or a model name.
fn part_hash(p: &str) -> u32 {
    if p.len() == 8 && p.bytes().all(|b| b.is_ascii_hexdigit()) { u32::from_str_radix(p, 16).unwrap_or(0) } else { joaat(&p.to_ascii_lowercase()) }
}

/// What a switch shows with these keys set: the element of the option whose
/// key (and second key, where it has one) is set, options with a second key
/// first; else its default. None = nothing.
fn pick<'v>(s: &'v Value, keys: &HashSet<u32>) -> Option<&'v Value> {
    let set = |v: &Value| hex32(v).is_some_and(|k| keys.contains(&k));
    let hit = s["options"].as_array().into_iter().flatten().filter(|o| set(&o["key"]) && (o["also"].is_null() || set(&o["also"]))).max_by_key(|o| !o["also"].is_null());
    let e = match hit {
        Some(o) => &o["element"],
        None => &s["default"],
    };
    (!e.is_null()).then_some(e)
}

/// Model stems (names without `_hi`) of an archive: the high LOD copy where
/// there is one, else the plain one; the largest of several.
fn stems<'x>(ix: &'x ArchiveIndex) -> HashMap<String, &'x Entry> {
    let mut out: HashMap<String, (bool, &Entry)> = HashMap::new();
    for e in ix.entries.iter().filter(|e| matches!(e.rtype.as_deref(), Some("ft") | Some("dr"))) {
        let Some(n) = e.name.as_deref() else { continue };
        let n = n.to_ascii_lowercase();
        if n.ends_with("_mid") {
            continue;
        }
        let (stem, hi) = match n.strip_suffix("_hi") {
            Some(s) => (s.to_string(), true),
            None => (n, false),
        };
        match out.get(&stem) {
            Some((h, x)) if (*h, x.usize) >= (hi, e.usize) => {}
            _ => {
                out.insert(stem, (hi, e));
            }
        }
    }
    out.into_iter().map(|(k, v)| (k, v.1)).collect()
}

struct Piece {
    name: String,
    prims: Vec<TriPrim>,
    joints: Vec<Joint>,
    part_looks: Value,
    parts_materials: Vec<bool>,
    discs: Vec<crate::bounds::WheelDisc>,
    pivots: Vec<(String, [f32; 3])>,
    warnings: Vec<String>,
    id: String,
}

/// Export `vehicle` (a record of `vehicles.json`) with the parts of its
/// configuration `config` to `out` (a .glb path; textures go beside it).
/// `with` adds parts to the configuration's (a part in the same slot is
/// replaced), `without` takes parts off.
#[allow(clippy::too_many_arguments)]
pub fn export_vehicle<'x>(a: &ArchiveInfo, ix: &'x ArchiveIndex, vehicle: &Value, config: usize, with: &[u32], without: &[u32], wheels: bool, out: &Path, o: &ExportOptions) -> Result<Value> {
    let model = vehicle["model"].as_str().ok_or_else(|| anyhow!("the vehicle record has no model name"))?.to_ascii_lowercase();
    let configs = vehicle["configurations"].as_array().cloned().unwrap_or_default();
    let cfg = configs.get(config).ok_or_else(|| anyhow!("{model} has {} configurations; --config {config} is out of range", configs.len()))?;
    let dir = out.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&dir)?;
    let mut o = o.clone();
    o.out = dir.clone();
    // the configuration's colour is the primary paint
    // (the stored colour is linear: its bytes are the paint as it is, and
    // `srgb_if_linear` in vehicles.json is how it looks on screen)
    let paint = cfg["colour"]["argb"].as_str().filter(|c| c.len() == 8).and_then(|c| u32::from_str_radix(c, 16).ok()).map(|v| [(v >> 16) & 255, (v >> 8) & 255, v & 255].map(|c| c as f32 / 255.0));
    if o.paint.is_none() {
        o.paint = paint;
    }
    let paint = o.paint;

    let stems = stems(ix);
    let by_asset: HashMap<u64, &Entry> = stems.iter().map(|(s, e)| (asset_id(s), *e)).collect();
    let by_hash: HashMap<u32, &Entry> = stems.iter().map(|(s, e)| (joaat(s), *e)).collect();
    let base = *stems.get(&model).ok_or_else(|| anyhow!("no model named {model} in {}", a.id))?;
    let resolve = |e: &Value| -> Option<&'x Entry> {
        let by_a = e["asset"].as_str().and_then(|s| u64::from_str_radix(s, 16).ok()).and_then(|x| by_asset.get(&x));
        by_a.or_else(|| hex32(&e["model"]).and_then(|h| by_hash.get(&h))).copied()
    };

    // the configuration's parts, and what they fit
    let mut part_names: Vec<String> = cfg["parts"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(|s| s.to_string())).collect();
    // the user's own choice of parts on top of the configuration's
    if !with.is_empty() || !without.is_empty() {
        let slot_of: HashMap<u32, &str> = vehicle["mods"].as_array().into_iter().flatten().filter_map(|m| Some((hex32(&m["name"])?, m["slot"].as_str()?))).collect();
        let taken: HashSet<&str> = with.iter().filter_map(|p| slot_of.get(p).copied()).filter(|s| *s != SLOT_CHASSIS).collect();
        part_names.retain(|p| {
            let h = part_hash(p);
            !without.contains(&h) && !with.contains(&h) && !slot_of.get(&h).is_some_and(|s| taken.contains(s))
        });
        part_names.extend(with.iter().map(|p| format!("{p:08x}")));
    }
    let fitted: HashSet<u32> = part_names.iter().map(|p| part_hash(p)).collect();
    let mods: HashMap<u32, &Value> = vehicle["mods"].as_array().into_iter().flatten().filter_map(|m| Some((hex32(&m["name"])?, m))).collect();
    let keys: HashSet<u32> = vehicle["part_keys"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(p, _)| u32::from_str_radix(p, 16).is_ok_and(|p| fitted.contains(&p)))
        .flat_map(|(_, k)| k.as_array().into_iter().flatten().filter_map(hex32))
        .collect();
    // (model, bone, stock bones replaced, body, what it is)
    let mut fit: Vec<(&'x Entry, u32, Vec<u32>, bool, String)> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    let mut add = |e: &Value, body: bool, what: String, fit: &mut Vec<(&'x Entry, u32, Vec<u32>, bool, String)>| {
        let hides = e["hides"].as_array().into_iter().flatten().filter_map(hex32).collect();
        match resolve(e) {
            // brake discs, drums and calipers are modelled around a unit
            // disc for the wheels; they are not fitted
            Some(en) if en.name.as_deref().is_some_and(|n| ["brakedisc_", "drumbrake_", "caliper_"].iter().any(|w| n.to_ascii_lowercase().starts_with(w))) => unresolved.push(format!("{what}: {} is a brake model; which wheels it goes on is not known, so it is not fitted", en.name.clone().unwrap_or_default())),
            Some(en) if en.name.as_deref().is_some_and(|n| ["rim_", "tyre_"].iter().any(|w| n.to_ascii_lowercase().starts_with(w))) => {}
            Some(en) if en.off != base.off => fit.push((en, hex32(&e["bone"]).unwrap_or(0), hides, body, what)),
            Some(_) => {}
            None => unresolved.push(format!("{what}: model {} not found", e["model"].as_str().unwrap_or("?"))),
        }
    };
    // (rims, tyres, brake discs and calipers are named by model: they go
    // on the wheels, not on a bone)
    let hex_name = |p: &str| p.len() == 8 && p.bytes().all(|b| b.is_ascii_hexdigit());
    for p in part_names.iter().filter(|p| hex_name(p)) {
        let Some(m) = mods.get(&part_hash(p)) else { continue };
        let slot = m["slot"].as_str().unwrap_or("");
        // (brake slots: fitted at the wheels, per axle)
        if slot == SLOT_CHASSIS || hex32(&m["slot"]).is_some_and(|s| wheels && (BRAKE_SLOTS.contains(&s) || RIM_LIGHT_SLOTS.contains(&s))) {
            continue;
        }
        for e in m["elements"].as_array().into_iter().flatten() {
            add(e, slot == SLOT_BODY, format!("part {p}"), &mut fit);
        }
    }
    for s in vehicle["switches"].as_array().into_iter().flatten() {
        if !s["part"].is_null() && !hex32(&s["part"]).is_some_and(|p| fitted.contains(&p)) {
            continue;
        }
        if let Some(e) = pick(s, &keys) {
            add(e, false, format!("switch {}", s["name"].as_str().unwrap_or("?")), &mut fit);
        }
    }
    fit.sort_by_key(|f| !f.3);
    // A model two parts fit on the same bone goes on once (a pickup's rear
    // bumper is an element of its bed part and of the part in the rear
    // bumper slot). What the stock pieces lose is the sum of both.
    let mut repeated = 0;
    let mut once: HashSet<(u64, u32)> = HashSet::new();
    let hidden: HashSet<u32> = fit.iter().flat_map(|f| f.2.iter().copied()).collect();
    fit.retain(|f| {
        let new = once.insert((f.0.off, f.1));
        repeated += usize::from(!new);
        new
    });

    let build = |e: &Entry| -> Result<(Piece, Vec<crate::gltf::Material>)> {
        let mut item = ExportedItem { id: e.id.clone(), archive: a.id.clone(), kind: e.kind.name(), rtype: e.rtype.clone(), name: e.name.clone(), status: "ok".into(), files: vec![], error: None, warnings: vec![] };
        let r = crate::resource::Resource::load(&a.path, e)?;
        let info = crate::drawable::describe(&r);
        let mut b = crate::export::build_drawable(a, e, &r, &info, &dir, false, &o, &mut item);
        let prims: Vec<TriPrim> = b.lod_groups.drain(..).next().unwrap_or_default().into_iter().flat_map(|g| g.prims).collect();
        // rims and tyres (`c5cdbdd2`, the tyre shaders) carry part ids of the
        // vehicle they are on, like its own parts
        let parts_materials = b.materials.iter().map(|m| m.extras["classification"]["stand_in"] == "parts" || wheel_material(&m.name)).collect();
        let pivots = info.skeleton.iter().filter(|b| crate::export::is_wheel_mount(&b.name)).filter_map(|b| Some((b.name.clone(), b.pivot?))).collect();
        let discs = if info.skeleton.iter().any(|b| b.name.starts_with("wheel_")) { crate::bounds::wheel_discs(&r) } else { Vec::new() };
        Ok((Piece { name: e.name.clone().unwrap_or_else(|| e.id.clone()), prims, joints: std::mem::take(&mut b.joints), part_looks: b.part_looks.take(), parts_materials, discs, pivots, warnings: item.warnings, id: e.id.clone() }, b.materials))
    };

    // the vehicle itself
    let (mut piece, base_materials) = build(base)?;
    let joints = std::mem::take(&mut piece.joints);
    let joint_of: HashMap<u32, usize> = joints.iter().enumerate().map(|(i, j)| (joaat(&j.name.to_ascii_lowercase()), i)).collect();
    let mut bind: HashMap<u32, Mat> = joints.iter().map(|j| (joaat(&j.name.to_ascii_lowercase()), j.bind)).collect();
    // the look of this configuration: the colour of each part id
    let mut look: Option<Vec<[u8; 4]>> = cfg["parts_key"]
        .as_str()
        .and_then(|k| piece.part_looks["configs"][k].as_u64())
        .or(piece.part_looks["default"].as_u64())
        .and_then(|i| piece.part_looks["looks"][i as usize].as_str())
        .filter(|h| h.len() == 2048)
        .map(|h| (0..256).map(|i| std::array::from_fn(|k| u8::from_str_radix(&h[i * 8 + k * 2..i * 8 + k * 2 + 2], 16).unwrap_or(255))).collect());
    // parts fitted on top of the configuration change the look (a body
    // kit's panels take the body paint)
    if let Some(l) = look.as_mut() {
        for (part, d) in piece.part_looks["part_patches"].as_object().into_iter().flatten() {
            if !u32::from_str_radix(part, 16).is_ok_and(|p| fitted.contains(&p)) {
                continue;
            }
            let rgba = d["rgba"].as_str().unwrap_or("");
            for (k, id) in d["ids"].as_array().into_iter().flatten().filter_map(|x| x.as_u64()).enumerate() {
                if let (Some(h), Some(e)) = (rgba.get(k * 8..k * 8 + 8), l.get_mut(id as usize)) {
                    *e = std::array::from_fn(|c| u8::from_str_radix(&h[c * 2..c * 2 + 2], 16).unwrap_or(255));
                }
            }
        }
    }
    let paint_bytes = paint.map(|p| p.map(|c| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
    let recolour = |prims: &mut [TriPrim], parts: &[bool]| {
        for p in prims.iter_mut().filter(|p| p.material.is_some_and(|m| parts.get(m).copied().unwrap_or(false))) {
            let Some(c) = &p.colors else { continue };
            let mut tint: Vec<[u8; 4]> = match (&look, &p.tint) {
                (Some(l), _) => c.iter().map(|c| l[c[3] as usize]).collect(),
                (None, Some(t)) => t.clone(),
                (None, None) => continue,
            };
            if let Some(pb) = paint_bytes {
                for t in tint.iter_mut().filter(|t| t[3] == 254) {
                    *t = [pb[0], pb[1], pb[2], 254];
                }
            }
            p.tint = Some(tint);
        }
    };

    // stock pieces the fitted elements replace (`hidden`)
    let gone: Vec<bool> = joints.iter().map(|j| hidden.contains(&joaat(&j.name.to_ascii_lowercase()))).collect();
    let mut dropped = 0;
    if gone.iter().any(|g| *g) {
        for p in &mut piece.prims {
            let Some(js) = &p.joints else { continue };
            let before = p.indices.len();
            p.indices = p.indices.chunks_exact(3).filter(|t| !t.iter().all(|&i| gone.get(js[i as usize][0] as usize).copied().unwrap_or(false))).flatten().copied().collect();
            dropped += (before - p.indices.len()) / 3;
        }
    }
    recolour(&mut piece.prims, &piece.parts_materials);

    let mut groups: Vec<NodeGroup> = Vec::new();
    let mut materials = Vec::new();
    let mut reports = Vec::new();
    let mut warnings: Vec<String> = piece.warnings.iter().map(|w| format!("{}: {w}", piece.name)).collect();
    // `mats`: the piece's materials, or where they already are (a wheel
    // model is placed several times with one set of materials)
    let push = |name: String, id: &str, what: &str, mut prims: Vec<TriPrim>, mats: std::result::Result<Vec<crate::gltf::Material>, usize>, groups: &mut Vec<NodeGroup>, materials: &mut Vec<crate::gltf::Material>, reports: &mut Vec<Value>| -> usize {
        let off = match &mats {
            Ok(_) => materials.len(),
            Err(at) => *at,
        };
        for p in &mut prims {
            p.material = p.material.map(|m| m + off);
            p.name = format!("{name}/{}", p.name);
        }
        reports.push(json!({"name": name, "id": id, "as": what, "vertices": prims.iter().map(|p| p.positions.len()).sum::<usize>(), "triangles": prims.iter().map(|p| p.indices.len() / 3).sum::<usize>()}));
        materials.extend(mats.into_iter().flatten().map(|mut m| {
            m.name = format!("{name}/{}", m.name);
            m
        }));
        groups.push(NodeGroup { name, prims });
        off
    };
    let (discs, pivots) = (std::mem::take(&mut piece.discs), std::mem::take(&mut piece.pivots));
    push(piece.name.clone(), &piece.id, "vehicle", std::mem::take(&mut piece.prims), Ok(base_materials), &mut groups, &mut materials, &mut reports);

    let mut skipped: Vec<String> = Vec::new();
    for (e, bone, _, body, what) in &fit {
        let pn = e.name.clone().unwrap_or_else(|| e.id.clone());
        if *bone != 0 && !bind.contains_key(bone) && !body {
            skipped.push(format!("{pn} ({what}): the vehicle has no bone {bone:08x}"));
            continue;
        }
        let (mut p, mats) = match build(e) {
            Ok(x) => x,
            Err(err) => {
                warnings.push(format!("{pn}: {err:#}"));
                continue;
            }
        };
        let t = bind.get(bone).copied().unwrap_or(IDENTITY);
        if *body {
            for j in &p.joints {
                bind.insert(joaat(&j.name.to_ascii_lowercase()), mul(&t, &j.bind));
            }
        }
        transform(&mut p.prims, &t, false);
        pin(&mut p.prims, joint_of.get(bone).copied());
        recolour(&mut p.prims, &p.parts_materials);
        warnings.extend(p.warnings.iter().map(|w| format!("{pn}: {w}")));
        push(pn, &p.id, what, p.prims, Ok(mats), &mut groups, &mut materials, &mut reports);
    }

    // What the configuration's parts change on the wheels (a widebody kit,
    // spacers): out from the centre line and wider, per axle (front, rear).
    let (mut fit_out, mut fit_wide) = ([0.0f32; 2], [0.0f32; 2]);
    for (p, fits) in vehicle["wheel_fits"].as_object().into_iter().flatten() {
        if !u32::from_str_radix(p, 16).is_ok_and(|p| fitted.contains(&p)) {
            continue;
        }
        for f in fits.as_array().into_iter().flatten() {
            let num = |k: &str| f[k].as_f64().unwrap_or(0.0) as f32;
            for axle in 0..2 {
                // the axle value is 0, 2 or 3 in the data: taken as front,
                // rear and both
                if matches!((f["axles"].as_u64().unwrap_or(9), axle), (0, 0) | (2, 1) | (3, _)) {
                    fit_out[axle] += num("offset") + num("spacing");
                    fit_wide[axle] += num("width");
                }
            }
        }
    }
    // wheels: rim and tyre on every wheel bone
    let mut wheel_report = Value::Null;
    // boats get no wheels, whatever bones they have (vehicle types 9 and 10)
    let boat = matches!(vehicle["model_info"]["type"]["tag"].as_u64(), Some(9) | Some(10));
    if boat {
        wheel_report = json!({"wheels": 0, "note": "a boat (vehicle type 9 or 10): no wheels are fitted"});
    }
    if wheels && !boat {
        let named = |prefix: &str| part_names.iter().find(|p| p.starts_with(prefix)).and_then(|p| stems.get(&p.to_ascii_lowercase()).copied());
        let models: Vec<&Entry> = [named("rim_"), named("tyre_")].into_iter().flatten().collect();
        let mut placed = 0;
        let mut built: Vec<(Piece, std::result::Result<Vec<crate::gltf::Material>, usize>, f32)> = Vec::new();
        for e in &models {
            match build(e) {
                Ok((p, mut mats)) => {
                    // the vertices bring the colour: no stand-in colour on top
                    if look.is_some() {
                        for m in mats.iter_mut().filter(|m| wheel_material(&m.name)) {
                            m.gltf["pbrMetallicRoughness"]["baseColorFactor"] = json!([1.0, 1.0, 1.0, 1.0]);
                        }
                    }
                    // the radius of the model about its axle (x)
                    let rad = p.prims.iter().flat_map(|p| p.positions.iter()).map(|v| v[1].abs().max(v[2].abs())).fold(0.0f32, f32::max);
                    built.push((p, Ok(mats), rad));
                }
                Err(err) => warnings.push(format!("{}: {err:#}", e.name.clone().unwrap_or_default())),
            }
        }
        // both scaled by the tyre's radius (the largest model radius)
        let unit = built.iter().map(|b| b.2).fold(0.0f32, f32::max);
        // The configuration's rim size (rim radius / tyre radius): all bones
        // of a wheel model sit on the hub, so the rim and the tyre's bead
        // are scaled about the axle and the tread (`tread_profile` bones)
        // keeps the tyre's radius.
        let rim_r = built.iter().map(|b| b.2).fold(f32::MAX, f32::min);
        let ratio = cfg["rim_ratio"].as_f64().unwrap_or(0.0) as f32;
        let rim_factor = if built.len() >= 2 && ratio > 0.0 && rim_r > 0.0 && unit > rim_r { (ratio * unit / rim_r).clamp(0.5, 0.97 * unit / rim_r) } else { 1.0 };
        if rim_factor != 1.0 {
            for (p, _, _) in built.iter_mut() {
                let tread: Vec<bool> = p.joints.iter().map(|j| j.name.to_ascii_lowercase().contains("tread_profile")).collect();
                for prim in &mut p.prims {
                    for i in 0..prim.positions.len() {
                        let wt: f32 = match (&prim.joints, &prim.weights) {
                            (Some(j), Some(w)) => (0..4).filter(|&q| tread.get(j[i][q] as usize).copied().unwrap_or(false)).map(|q| w[i][q] as f32 / 255.0).sum(),
                            _ => 0.0,
                        };
                        let f = rim_factor * (1.0 - wt) + wt;
                        prim.positions[i][1] *= f;
                        prim.positions[i][2] *= f;
                    }
                }
            }
        }
        // Brake discs (or drums) and calipers: modelled around a disc of
        // radius 0.5, scaled to the wheel's stored brake disc (the largest
        // of the clearly smaller collision discs on its axle line).
        // Per axle a disc (or drum) and a caliper: named by model in the
        // part list, or the model of a part in a brake slot (`brake_axles`).
        let part = |prefix: &str| part_names.iter().find(|p| p.starts_with(prefix)).map(|p| p.to_ascii_lowercase());
        let slot_models: Vec<(u32, String)> = part_names
            .iter()
            .filter(|p| hex_name(p))
            .filter_map(|p| mods.get(&part_hash(p)))
            .filter_map(|m| {
                let slot = hex32(&m["slot"]).filter(|s| BRAKE_SLOTS.contains(s) || RIM_LIGHT_SLOTS.contains(s))?;
                let n = m["elements"].as_array()?.iter().find_map(|e| resolve(e))?.name.clone()?.to_ascii_lowercase();
                Some((slot, n.strip_suffix("_hi").unwrap_or(&n).to_string()))
            })
            .collect();
        let axle_brakes = brake_axles(part("brakedisc_").or_else(|| part("drumbrake_")).as_deref(), part("caliper_").as_deref(), &slot_models);
        // the rim light rings go on the brakes the same way
        let rim_lights = rim_light_axles(&slot_models);
        // each distinct model once, with the axles it is on
        let mut brake_models: Vec<(&Entry, [bool; 2])> = Vec::new();
        for (axle, n) in axle_brakes.iter().enumerate().flat_map(|(i, a)| [(i, &a.0), (i, &a.1)]).chain(rim_lights.iter().enumerate()) {
            let Some(e) = n.as_ref().and_then(|n| stems.get(n).copied()) else { continue };
            match brake_models.iter_mut().find(|b| b.0.off == e.off) {
                Some(b) => b.1[axle] = true,
                None => brake_models.push((e, std::array::from_fn(|i| i == axle))),
            }
        }
        let mut brakes: Vec<(Piece, std::result::Result<Vec<crate::gltf::Material>, usize>, [bool; 2])> = Vec::new();
        for (e, on) in &brake_models {
            match build(e) {
                Ok((p, mats)) => brakes.push((p, Ok(mats), *on)),
                Err(err) => warnings.push(format!("{}: {err:#}", e.name.clone().unwrap_or_default())),
            }
        }
        let mut braked = 0;
        for (bone, pivot) in &pivots {
            // the wheel's collision disc: the largest on this axle line
            let near: Vec<&crate::bounds::WheelDisc> = discs.iter().filter(|d| (d.centre[0] - pivot[0]).abs() < 0.15 && (d.centre[1] - pivot[1]).abs() < 0.03 && (d.centre[2] - pivot[2]).abs() < 0.03).collect();
            let Some(d) = near.iter().max_by(|a, b| a.radius.total_cmp(&b.radius)) else {
                skipped.push(format!("wheel on {bone}: no collision disc"));
                continue;
            };
            if unit <= 0.0 {
                continue;
            }
            let s = d.radius / unit;
            let right = d.centre[0] > 0.0;
            // wheel_lr, wheel_rr, wheel_lr_inr ...: the second letter is the axle
            let axle = (bone.to_ascii_lowercase().strip_prefix("wheel_").and_then(|b| b.chars().nth(1)) == Some('r')) as usize;
            let x = d.centre[0] + fit_out[axle].copysign(d.centre[0]);
            // a wider tyre: the wheel stretched along its axle
            let sx = if d.half_width > 0.0 { s * (d.half_width + fit_wide[axle] / 2.0) / d.half_width } else { s };
            let m: Mat = [if right { -sx } else { sx }, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, s, 0.0, x, d.centre[1], d.centre[2], 1.0];
            for (p, mats, _) in built.iter_mut() {
                let mut prims = p.prims.clone();
                transform(&mut prims, &m, right);
                pin(&mut prims, joint_of.get(&joaat(&bone.to_ascii_lowercase())).copied());
                recolour(&mut prims, &p.parts_materials);
                let at = push(format!("{}@{bone}", p.name), &p.id, "wheel", prims, std::mem::replace(mats, Err(0)), &mut groups, &mut materials, &mut reports);
                *mats = Err(at);
            }
            if let Some(b) = near.iter().filter(|x| x.radius < 0.75 * d.radius).max_by(|a, b| a.radius.total_cmp(&b.radius)) {
                let sb = b.radius / 0.5;
                let bx = b.centre[0] + fit_out[axle].copysign(b.centre[0]);
                let mb: Mat = [if right { -sb } else { sb }, 0.0, 0.0, 0.0, 0.0, sb, 0.0, 0.0, 0.0, 0.0, sb, 0.0, bx, b.centre[1], b.centre[2], 1.0];
                for (p, mats, _) in brakes.iter_mut().filter(|b| b.2[axle]) {
                    let mut prims = p.prims.clone();
                    transform(&mut prims, &mb, right);
                    pin(&mut prims, joint_of.get(&joaat(&bone.to_ascii_lowercase())).copied());
                    recolour(&mut prims, &p.parts_materials);
                    let at = push(format!("{}@{bone}", p.name), &p.id, "brake", prims, std::mem::replace(mats, Err(0)), &mut groups, &mut materials, &mut reports);
                    *mats = Err(at);
                }
                braked += 1;
            }
            placed += 1;
        }
        wheel_report = json!({"moved_out": fit_out, "widened": fit_wide, "models": models.iter().map(|e| e.name.clone()).collect::<Vec<_>>(), "wheels": placed, "rim_ratio": ratio, "rim_factor": rim_factor, "brake_models": brake_models.iter().map(|b| b.0.name.clone()).collect::<Vec<_>>(), "brakes_by_axle": {"front": {"disc": axle_brakes[0].0, "caliper": axle_brakes[0].1}, "rear": {"disc": axle_brakes[1].0, "caliper": axle_brakes[1].1}}, "wheels_with_brakes": braked, "note": "the wheel is scaled to the collision disc's radius; rim and bead are scaled about the axle by rim_factor so that the rim has the configuration's rim_ratio of the tyre's radius (1 = the models' own share). Brakes are scaled to the wheel's stored brake disc."});
    }
    if groups.iter().all(|g| g.prims.is_empty()) {
        bail!("nothing to export for {model}");
    }
    let extras = json!({"vehicle": model, "configuration": config, "parts_key": cfg["parts_key"], "colour": cfg["colour"], "coordinate_system": crate::export::COORDS});
    let glb = crate::gltf::mesh_glb_with(&model, &groups, &materials, &joints, extras, &crate::gltf::GlbOptions { animations: &[], trs_joints: true, skinned_meshes_at_root: true });
    std::fs::write(out, &glb)?;
    Ok(json!({
        "glb": out, "bytes": glb.len(), "vehicle": model, "configuration": config, "configurations": configs.len(),
        "pieces": reports, "joints": joints.len(), "stock_triangles_replaced": dropped,
        "part_colours": if look.is_some() { "the configuration's" } else { "as exported (the vehicle has no part colours per configuration)" },
        "paint": paint, "wheels": wheel_report, "repeated_elements_skipped": repeated, "not_fitted": skipped, "left_out": unresolved, "warnings": warnings,
    }))
}

/// The parts of a vehicle that have models, for `sixng vehicle
/// --list-parts`: part name, slot, whether it is a library part, how many
/// of the vehicle's configurations name it and its models.
pub fn list_parts(ix: &ArchiveIndex, vehicle: &Value) -> Value {
    let stems = stems(ix);
    let by_asset: HashMap<u64, &String> = stems.keys().map(|s| (asset_id(s), s)).collect();
    let by_hash: HashMap<u32, &String> = stems.keys().map(|s| (joaat(s), s)).collect();
    let mut used: HashMap<u32, usize> = HashMap::new();
    for c in vehicle["configurations"].as_array().into_iter().flatten() {
        for p in c["parts"].as_array().into_iter().flatten().filter_map(|p| p.as_str()) {
            *used.entry(part_hash(p)).or_default() += 1;
        }
    }
    let mut out: Vec<Value> = vehicle["mods"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            let models: Vec<Value> = m["elements"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| {
                    let by_a = e["asset"].as_str().and_then(|s| u64::from_str_radix(s, 16).ok()).and_then(|x| by_asset.get(&x));
                    by_a.or_else(|| hex32(&e["model"]).and_then(|h| by_hash.get(&h))).map_or(Value::Null, |s| json!(s))
                })
                .collect();
            json!({"part": m["name"], "slot": m["slot"], "slot_name": hex32(&m["slot"]).and_then(crate::vehicles::mod_slot_name), "shared": m["shared"] == true, "configurations": hex32(&m["name"]).and_then(|h| used.get(&h)).copied().unwrap_or(0), "models": models})
        })
        .collect();
    out.sort_by(|a, b| a["slot"].as_str().cmp(&b["slot"].as_str()).then(a["part"].as_str().cmp(&b["part"].as_str())));
    json!(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_pick() {
        let s = json!({"default": {"model": "d"}, "options": [
            {"key": "00000001", "also": null, "element": {"model": "a"}},
            {"key": "00000001", "also": "00000002", "element": {"model": "b"}},
            {"key": "00000003", "also": null, "element": null},
        ]});
        let keys = |k: &[u32]| k.iter().copied().collect::<HashSet<u32>>();
        assert_eq!(pick(&s, &keys(&[])).unwrap()["model"], "d");
        assert_eq!(pick(&s, &keys(&[1])).unwrap()["model"], "a");
        assert_eq!(pick(&s, &keys(&[1, 2])).unwrap()["model"], "b");
        assert!(pick(&s, &keys(&[3])).is_none());
    }

    #[test]
    fn brakes_per_axle() {
        let s = |x: &str| Some(x.to_string());
        // named front disc and caliper, a smaller rear caliper part
        let b = brake_axles(Some("brakedisc_std_001"), Some("caliper_4pot_001"), &[(BRAKE_SLOTS[3], "caliper_2pot_001".into())]);
        assert_eq!(b, [(s("brakedisc_std_001"), s("caliper_4pot_001")), (s("brakedisc_std_001"), s("caliper_2pot_001"))]);
        // drums all round: no caliper anywhere
        let b = brake_axles(None, None, &[(BRAKE_SLOTS[0], "drumbrake_001".into()), (BRAKE_SLOTS[1], "drumbrake_001".into())]);
        assert_eq!(b, [(s("drumbrake_001"), None), (s("drumbrake_001"), None)]);
        // front discs with a caliper part, rear drums: the rear gets no caliper
        let b = brake_axles(Some("brakedisc_std_001"), None, &[(BRAKE_SLOTS[2], "caliper_2pot_001".into()), (BRAKE_SLOTS[1], "drumbrake_002".into())]);
        assert_eq!(b, [(s("brakedisc_std_001"), s("caliper_2pot_001")), (s("drumbrake_002"), None)]);
        assert_eq!(brake_axles(None, None, &[]), AxleBrakes::default());
    }

    #[test]
    fn rim_lights_per_axle() {
        let s = |x: &str| Some(x.to_string());
        // one part: the same ring on both axles
        assert_eq!(rim_light_axles(&[(RIM_LIGHT_SLOTS[0], "caliper_rimlight".into())]), [s("caliper_rimlight"), s("caliper_rimlight")]);
        // a rear part of its own (rear drums)
        let both = [(RIM_LIGHT_SLOTS[0], "caliper_rimlight".to_string()), (RIM_LIGHT_SLOTS[1], "drumbrake_rimlight".to_string())];
        assert_eq!(rim_light_axles(&both), [s("caliper_rimlight"), s("drumbrake_rimlight")]);
        // brake parts are not rings
        assert_eq!(rim_light_axles(&[(BRAKE_SLOTS[3], "caliper_2pot_001".into())]), [None, None]);
        // and a ring is not a brake
        assert_eq!(brake_axles(None, None, &both), AxleBrakes::default());
    }

    #[test]
    fn mirror_turns_triangles() {
        let mut p = vec![TriPrim { name: "t".into(), positions: vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], normals: Some(vec![[1.0, 0.0, 0.0]; 3]), tangents: None, uvs: vec![], colors: None, colors1: None, tint: None, indices: vec![0, 1, 2], material: None, joints: None, weights: None, joints1: None, weights1: None }];
        let m: Mat = [-2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 5.0, 0.0, 0.0, 1.0];
        transform(&mut p, &m, true);
        assert_eq!(p[0].positions[0], [3.0, 0.0, 0.0]);
        assert_eq!(p[0].normals.as_ref().unwrap()[0], [-1.0, 0.0, 0.0]);
        assert_eq!(p[0].indices, vec![0, 2, 1]);
    }
}
