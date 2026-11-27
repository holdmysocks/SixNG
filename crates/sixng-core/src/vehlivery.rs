//! A livery in a vehicle export (`sixng vehicle --livery`): the stickers of
//! a livery file, as the viewer's Liveries panel saves it, written into the
//! vehicle's GLB as decal meshes.
//!
//! The game (and the viewer) draw a sticker in the surface shader: on the
//! faces of its sticker zones, where the mesh UV of its UV set, taken
//! through the sticker's transform (`u = U*a + V*b + ou`, `v = U*c + V*d +
//! ov`), lies in 0..1. glTF has no such layer, so the export cuts that
//! area out of the faces and writes it as a mesh of its own:
//!
//! - one primitive per sticker and model, in the node of the model it lies
//!   on, skinned like the faces under it (it follows a door);
//! - the faces are cut at the sticker's border, so the picture is not
//!   repeated and nothing outside it is covered;
//! - `TEXCOORD_0` is the place in the sticker's picture;
//! - lifted off the surface along the normal (`LIFT`, and `STEP` more per
//!   sticker so that a later one lies on top of an earlier one);
//! - its own material: the picture with its alpha, blended.
//!
//! A colour mask (the picture's red, green and blue each take a colour) is
//! written as a picture in those colours.
use crate::gltf::{Material, NodeGroup, TriPrim};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// How far a sticker is lifted off the surface, metres, and how much more
/// each further sticker is.
pub const LIFT: f32 = 0.0008;
pub const STEP: f32 = 0.0002;

/// One sticker to write.
#[derive(Clone, Debug)]
pub struct Decal {
    pub name: String,
    /// The model whose zones are meant: "" = the vehicle itself, else the
    /// name of a fitted part's model.
    pub model: String,
    pub zones: Vec<u8>,
    /// Which UV set of the mesh (0 to 3).
    pub uv_set: usize,
    /// a, b, c, d, ou, ov.
    pub m: [f32; 6],
    /// The picture, relative to the GLB.
    pub uri: String,
    /// Linear RGBA the picture is multiplied by.
    pub factor: [f32; 4],
}

fn lin(c: u8) -> f32 {
    let x = c as f32 / 255.0;
    if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}

fn srgb(x: f32) -> u8 {
    let x = x.clamp(0.0, 1.0);
    let y = if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    (y * 255.0 + 0.5) as u8
}

/// A colour mask as a picture: each pixel's red, green and blue are the
/// cover of the three colours (sRGB); the result is their mix, with the
/// cover as alpha. The same rule as the viewer's shader.
pub fn mask_picture(rgba: &mut [u8], colours: [[u8; 3]; 3]) {
    let c = colours.map(|c| c.map(lin));
    for p in rgba.chunks_exact_mut(4) {
        let t = [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0];
        let sum = t[0] + t[1] + t[2];
        for k in 0..3 {
            p[k] = srgb((c[0][k] * t[0] + c[1][k] * t[1] + c[2][k] * t[2]) / sum.max(1e-4));
        }
        p[3] = (sum.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    }
}

/// Read a livery file and write its pictures to `dir` (as
/// `livery_<vehicle>_<n>.png`). Returns the stickers to write and what was
/// left out, in words.
pub fn prepare(game: &Path, file: &Path, dir: &Path) -> Result<(Vec<Decal>, Vec<String>)> {
    let l: Value = serde_json::from_slice(&std::fs::read(file).map_err(|e| anyhow!("read {}: {e}", file.display()))?).map_err(|e| anyhow!("{}: {e}", file.display()))?;
    let Some(stickers) = l["stickers"].as_array() else { bail!("{} is not a livery file (no stickers list)", file.display()) };
    let stem: String = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    std::fs::create_dir_all(dir)?;
    // game textures are decoded to a folder of this run's own and read from there
    let cache = dir.join(format!(".livery_{}", std::process::id()));
    let (mut out, mut left) = (Vec::new(), Vec::new());
    for (i, s) in stickers.iter().enumerate() {
        let name = s["name"].as_str().filter(|n| !n.is_empty()).map_or_else(|| format!("sticker {}", i + 1), str::to_string);
        if s["visible"] == false {
            left.push(format!("{name}: hidden in the livery"));
            continue;
        }
        let six = |v: &Value| -> Option<[f32; 6]> {
            let a = v.as_array().filter(|a| a.len() == 6)?;
            let mut m = [0.0; 6];
            for (k, x) in a.iter().enumerate() {
                m[k] = x.as_f64()? as f32;
            }
            Some(m)
        };
        // the game's own transform, or the numbers the viewer drew a
        // hand-placed sticker with when the file was saved
        let Some(m) = six(&s["game_uv"]).or_else(|| six(&s["shader_uv"])) else {
            left.push(format!("{name}: the file has no shader numbers for it (open and save the livery in the viewer)"));
            continue;
        };
        let zones: Vec<u8> = s["zones"].as_array().into_iter().flatten().filter_map(|z| z.as_u64().map(|z| z as u8)).collect();
        if zones.is_empty() {
            left.push(format!("{name}: no zones chosen"));
            continue;
        }
        let colours: [[u8; 3]; 3] = std::array::from_fn(|k| std::array::from_fn(|c| s["colours"][k][c].as_u64().unwrap_or(255) as u8));
        let mask = s["mask"] == true;
        let read = || -> Result<image::RgbaImage> {
            if let Some(id) = s["picture"]["game"].as_str() {
                let id = crate::vehstickers::parse_texture_id(id).ok_or_else(|| anyhow!("{id} is not a texture id"))?;
                let (png, _) = crate::vehstickers::sticker_png(game, id, &cache)?;
                Ok(image::open(&png)?.to_rgba8())
            } else if let Some(p) = s["picture"]["file"].as_str() {
                Ok(image::open(p).map_err(|e| anyhow!("{p}: {e}"))?.to_rgba8())
            } else {
                bail!("no picture")
            }
        };
        let mut img = match read() {
            Ok(i) => i,
            Err(e) => {
                left.push(format!("{name}: {e:#}"));
                continue;
            }
        };
        if mask {
            mask_picture(&mut img, colours);
        }
        let uri = format!("livery_{stem}_{:02}.png", i + 1);
        img.save(dir.join(&uri))?;
        let opacity = s["opacity"].as_f64().unwrap_or(1.0).clamp(0.0, 1.0) as f32;
        let tint = if mask { [1.0; 3] } else { colours[0].map(lin) };
        out.push(Decal { name, model: s["model"].as_str().unwrap_or("").to_ascii_lowercase(), zones, uv_set: s["uv_set"].as_u64().unwrap_or(1).min(3) as usize, m, uri, factor: [tint[0], tint[1], tint[2], opacity] });
    }
    let _ = std::fs::remove_dir_all(&cache);
    Ok((out, left))
}

/// Where the viewer keeps a vehicle's livery.
pub fn saved_file(work: &Path, vehicle: &str) -> PathBuf {
    work.join("liveries").join(format!("{}.json", vehicle.to_ascii_lowercase()))
}

/// A corner of a cut face: its weights on the face's three corners and its
/// place in the picture.
#[derive(Clone, Copy)]
struct Corner {
    w: [f32; 3],
    p: [f32; 2],
}

/// Cut a face (three corners with their place in the picture) to the
/// picture's square 0..1.
fn cut(p: [[f32; 2]; 3]) -> Vec<Corner> {
    let mut poly: Vec<Corner> = (0..3).map(|i| Corner { w: std::array::from_fn(|k| (k == i) as u8 as f32), p: p[i] }).collect();
    // (axis, keep the side where sign * (value - limit) >= 0)
    for (axis, limit, sign) in [(0, 0.0, 1.0f32), (0, 1.0, -1.0), (1, 0.0, 1.0), (1, 1.0, -1.0)] {
        let d = |c: &Corner| sign * (c.p[axis] - limit);
        let mut next = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let (da, db) = (d(&a), d(&b));
            if da >= 0.0 {
                next.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                let t = da / (da - db);
                let mut c = Corner { w: std::array::from_fn(|k| a.w[k] + (b.w[k] - a.w[k]) * t), p: [a.p[0] + (b.p[0] - a.p[0]) * t, a.p[1] + (b.p[1] - a.p[1]) * t] };
                c.p[axis] = limit;
                next.push(c);
            }
        }
        poly = next;
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly
}

/// The four heaviest joints of a mix of three corners, weights summing to 255.
fn mix_skin(j: [[u16; 4]; 3], wt: [[u8; 4]; 3], w: [f32; 3]) -> ([u16; 4], [u8; 4]) {
    let mut acc: Vec<(u16, f32)> = Vec::with_capacity(12);
    for c in 0..3 {
        for k in 0..4 {
            let x = wt[c][k] as f32 * w[c];
            if x <= 0.0 {
                continue;
            }
            match acc.iter_mut().find(|a| a.0 == j[c][k]) {
                Some(a) => a.1 += x,
                None => acc.push((j[c][k], x)),
            }
        }
    }
    acc.sort_by(|a, b| b.1.total_cmp(&a.1));
    acc.truncate(4);
    let sum: f32 = acc.iter().map(|a| a.1).sum();
    let (mut jo, mut wo) = ([0u16; 4], [0u8; 4]);
    if sum <= 0.0 {
        return (j[0], wt[0]);
    }
    let mut left = 255i32;
    for (k, a) in acc.iter().enumerate() {
        let v = if k + 1 == acc.len() { left } else { ((a.1 / sum * 255.0).round() as i32).min(left) };
        jo[k] = a.0;
        wo[k] = v.max(0) as u8;
        left -= v;
    }
    // the rounding rest goes to the heaviest
    if left > 0 {
        wo[0] = (wo[0] as i32 + left).min(255) as u8;
    }
    (jo, wo)
}

/// The decal mesh of one sticker on the primitives of one model. `skip`:
/// primitives that take no stickers (the model's own decal meshes, whose
/// second colour stream is not a zone). `lift`: metres off the surface.
/// None when the sticker covers nothing there.
pub fn decal_prim(prims: &[TriPrim], skip: &dyn Fn(&TriPrim) -> bool, d: &Decal, lift: f32) -> Option<TriPrim> {
    let mut on = [false; 256];
    for z in &d.zones {
        on[*z as usize] = true;
    }
    let mut out = TriPrim { name: String::new(), positions: Vec::new(), normals: Some(Vec::new()), tangents: None, uvs: vec![Vec::new()], colors: None, colors1: None, regions: None, tint: None, indices: Vec::new(), material: None, joints: None, weights: None, joints1: None, weights1: None, morphs: Vec::new() };
    let (mut joints, mut weights) = (Vec::new(), Vec::new());
    let mut skinned = true;
    let m = d.m;
    for p in prims {
        let (Some(zone), Some(uv)) = (&p.colors1, p.uvs.get(d.uv_set)) else { continue };
        if skip(p) || uv.len() != p.positions.len() || zone.len() != p.positions.len() {
            continue;
        }
        let skin = match (&p.joints, &p.weights) {
            (Some(j), Some(w)) => Some((j, w)),
            _ => None,
        };
        for t in p.indices.chunks_exact(3) {
            let i = [t[0] as usize, t[1] as usize, t[2] as usize];
            // the zone of a face is its first corner's, as in the shader
            if !on[zone[i[0]][3] as usize] {
                continue;
            }
            let at = i.map(|i| [uv[i][0] * m[0] + uv[i][1] * m[1] + m[4], uv[i][0] * m[2] + uv[i][1] * m[3] + m[5]]);
            if (0..2).any(|a| at.iter().all(|q| q[a] < 0.0) || at.iter().all(|q| q[a] > 1.0)) || at.iter().flatten().any(|x| !x.is_finite()) {
                continue;
            }
            let poly = cut(at);
            if poly.len() < 3 {
                continue;
            }
            let pos = i.map(|i| p.positions[i]);
            let face = {
                let (a, b) = (std::array::from_fn::<f32, 3, _>(|k| pos[1][k] - pos[0][k]), std::array::from_fn::<f32, 3, _>(|k| pos[2][k] - pos[0][k]));
                [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
            };
            let nrm = match &p.normals {
                Some(n) => i.map(|i| n[i]),
                None => [face; 3],
            };
            let base = out.positions.len() as u32;
            for c in &poly {
                let mut n: [f32; 3] = std::array::from_fn(|k| nrm[0][k] * c.w[0] + nrm[1][k] * c.w[1] + nrm[2][k] * c.w[2]);
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                if len > 1e-9 {
                    n = n.map(|x| x / len);
                }
                out.positions.push(std::array::from_fn(|k| pos[0][k] * c.w[0] + pos[1][k] * c.w[1] + pos[2][k] * c.w[2] + n[k] * lift));
                out.normals.as_mut().unwrap().push(n);
                out.uvs[0].push(c.p);
                match skin {
                    Some((j, w)) => {
                        let (jo, wo) = mix_skin(i.map(|i| j[i]), i.map(|i| w[i]), c.w);
                        joints.push(jo);
                        weights.push(wo);
                    }
                    None => skinned = false,
                }
            }
            for k in 1..poly.len() as u32 - 1 {
                out.indices.extend([base, base + k, base + k + 1]);
            }
        }
    }
    if out.indices.is_empty() {
        return None;
    }
    if skinned && joints.len() == out.positions.len() {
        out.joints = Some(joints);
        out.weights = Some(weights);
    }
    Some(out)
}

/// Add the stickers to the groups of a vehicle export. `groups[0]` is the
/// vehicle itself; a fitted part's group has its model's name. Returns one
/// report per sticker.
pub fn add(groups: &mut [NodeGroup], materials: &mut Vec<Material>, decals: &[Decal]) -> Vec<Value> {
    // the model's own decal meshes (bolts, markers; shader 389206d1) have
    // the second colour stream too, but its bytes are not zones there
    let skip_at: Vec<bool> = materials.iter().map(|m| m.name.ends_with("389206d1")).collect();
    let skip = |p: &TriPrim| p.material.is_some_and(|m| skip_at.get(m).copied().unwrap_or(false));
    let mut reports = Vec::new();
    for (n, d) in decals.iter().enumerate() {
        let lift = LIFT + STEP * n as f32;
        let mut placed = Vec::new();
        let mut material = None;
        for (g, group) in groups.iter_mut().enumerate() {
            let own = if d.model.is_empty() { g == 0 } else { group.name.eq_ignore_ascii_case(&d.model) };
            if !own {
                continue;
            }
            let Some(mut p) = decal_prim(&group.prims, &skip, d, lift) else { continue };
            let at = *material.get_or_insert_with(|| {
                materials.push(Material {
                    name: format!("livery/{:02} {}", n + 1, d.name),
                    base_color_uri: Some(d.uri.clone()),
                    normal_uri: None,
                    gltf: json!({"alphaMode": "BLEND", "pbrMetallicRoughness": {"baseColorFactor": d.factor, "metallicFactor": 0.0, "roughnessFactor": 0.45}}),
                    extras: json!({"livery": {"sticker": d.name, "model": d.model, "zones": d.zones, "uv_set": d.uv_set, "uv": d.m}, "clamp": true}),
                });
                materials.len() - 1
            });
            p.material = Some(at);
            p.name = format!("{}/livery {:02} {}", group.name, n + 1, d.name);
            placed.push(json!({"model": group.name, "triangles": p.indices.len() / 3}));
            group.prims.push(p);
        }
        reports.push(json!({"sticker": d.name, "picture": d.uri, "on": placed, "note": if placed.is_empty() { Value::from(format!("covers nothing: no face of {} with zones {:?} lies under it", if d.model.is_empty() { "the vehicle" } else { &d.model }, d.zones)) } else { Value::Null }}));
    }
    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall() -> TriPrim {
        // a 4 m x 2 m wall facing -Y (RAGE: X right, Z up); second UV set
        // 0..1 across it, V down; zone 7 on the left half, 8 on the right
        let v = |x: f32, z: f32| ([x, 0.0, z], [(x + 2.0) / 4.0, 1.0 - z / 2.0]);
        let quads = [(-2.0, 0.0, 7u8), (0.0, 2.0, 8u8)];
        let (mut pos, mut uv, mut zone, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (x0, x1, z) in quads {
            let b = pos.len() as u32;
            for (x, h) in [(x0, 0.0), (x1, 0.0), (x1, 2.0), (x0, 2.0)] {
                let (p, t) = v(x, h);
                pos.push(p);
                uv.push(t);
                zone.push([0, 0, 0, z]);
            }
            idx.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        let n = pos.len();
        TriPrim { name: "wall".into(), positions: pos, normals: Some(vec![[0.0, -1.0, 0.0]; n]), tangents: None, uvs: vec![vec![[0.0; 2]; n], uv], colors: None, colors1: Some(zone), regions: None, tint: None, indices: idx, material: None, joints: Some(vec![[3, 0, 0, 0]; n]), weights: Some(vec![[255, 0, 0, 0]; n]), joints1: None, weights1: None, morphs: Vec::new() }
    }

    fn area(p: &TriPrim) -> f32 {
        p.indices
            .chunks_exact(3)
            .map(|t| {
                let q = [p.positions[t[0] as usize], p.positions[t[1] as usize], p.positions[t[2] as usize]];
                let (a, b) = (std::array::from_fn::<f32, 3, _>(|k| q[1][k] - q[0][k]), std::array::from_fn::<f32, 3, _>(|k| q[2][k] - q[0][k]));
                let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
                (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() * 0.5
            })
            .sum()
    }

    #[test]
    fn a_sticker_is_cut_out_of_the_faces_under_it() {
        // 2 m x 1 m in the middle of the wall: U 0.25..0.75, V 0.25..0.75
        let d = Decal { name: "s".into(), model: String::new(), zones: vec![7, 8], uv_set: 1, m: [2.0, 0.0, 0.0, 2.0, -0.5, -0.5], uri: "s.png".into(), factor: [1.0; 4] };
        let p = decal_prim(&[wall()], &|_| false, &d, 0.001).unwrap();
        assert!((area(&p) - 2.0).abs() < 1e-4, "{}", area(&p));
        // inside the picture, lifted off the wall, skinned as the wall is
        assert!(p.uvs[0].iter().flatten().all(|x| (-1e-6..=1.0 + 1e-6).contains(x)));
        assert!(p.positions.iter().all(|q| (q[1] + 0.001).abs() < 1e-6 && q[0].abs() <= 1.0 + 1e-5 && (0.5 - 1e-5..=1.5 + 1e-5).contains(&q[2])));
        assert_eq!(p.joints.as_ref().unwrap()[0], [3, 0, 0, 0]);
        assert_eq!(p.weights.as_ref().unwrap()[0], [255, 0, 0, 0]);
        // the picture's left top corner is at x -1, z 1.5
        let k = p.uvs[0].iter().position(|t| t[0] < 1e-5 && t[1] < 1e-5).unwrap();
        assert!((p.positions[k][0] + 1.0).abs() < 1e-5 && (p.positions[k][2] - 1.5).abs() < 1e-5);
        // only on its zones: with zone 7 alone, the left half of it
        let left = decal_prim(&[wall()], &|_| false, &Decal { zones: vec![7], ..d.clone() }, 0.001).unwrap();
        assert!((area(&left) - 1.0).abs() < 1e-4);
        assert!(left.positions.iter().all(|q| q[0] <= 1e-5));
        // and nothing on other zones or off the wall
        assert!(decal_prim(&[wall()], &|_| false, &Decal { zones: vec![9], ..d.clone() }, 0.001).is_none());
        assert!(decal_prim(&[wall()], &|_| false, &Decal { m: [2.0, 0.0, 0.0, 2.0, 5.0, 5.0], ..d.clone() }, 0.001).is_none());
        assert!(decal_prim(&[wall()], &|_| true, &d, 0.001).is_none());
    }

    #[test]
    fn stickers_go_to_the_model_they_name() {
        let mut groups = vec![NodeGroup { name: "truck".into(), prims: vec![wall()] }, NodeGroup { name: "box_hi".into(), prims: vec![wall()] }];
        let mut mats = Vec::new();
        let d = |model: &str| Decal { name: "s".into(), model: model.into(), zones: vec![7], uv_set: 1, m: [2.0, 0.0, 0.0, 2.0, -0.5, -0.5], uri: "s.png".into(), factor: [1.0, 1.0, 1.0, 0.5] };
        let r = add(&mut groups, &mut mats, &[d(""), d("box_hi"), d("other")]);
        assert_eq!((groups[0].prims.len(), groups[1].prims.len(), mats.len()), (2, 2, 2));
        assert_eq!(groups[1].prims[1].material, Some(1));
        assert_eq!(r[0]["on"][0]["model"], "truck");
        assert_eq!(r[1]["on"][0]["model"], "box_hi");
        assert!(r[2]["note"].as_str().unwrap().contains("covers nothing"));
        // a later sticker lies above an earlier one
        assert!(groups[1].prims[1].positions[0][1] < groups[0].prims[1].positions[0][1]);
    }

    #[test]
    fn a_mask_takes_its_colours() {
        // full red cover, half green cover, nothing
        let mut px = vec![255, 0, 0, 255, 0, 128, 0, 255, 0, 0, 0, 255];
        mask_picture(&mut px, [[10, 20, 30], [200, 100, 50], [0, 0, 0]]);
        assert_eq!(&px[0..4], &[10, 20, 30, 255]);
        assert_eq!(&px[4..8], &[200, 100, 50, 128]);
        assert_eq!(px[11], 0);
    }
}
