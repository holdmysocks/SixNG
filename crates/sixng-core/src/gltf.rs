//! Minimal binary glTF (GLB) writer.
//!
//! `mesh_glb` writes triangle meshes (POSITION, NORMAL, TEXCOORD_n, u32
//! indices). `points_glb` writes point clouds, used as a fallback when a
//! mesh blob has no index buffer.
//!
//! RAGE is Z-up; glTF is Y-up, so every position and normal is converted
//! with (x, y, z) -> (x, z, -y). UVs are written as stored (top-left
//! origin, same convention as glTF).
use serde_json::{json, Value};

pub struct PointPrim {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
}

/// One triangle primitive (one geometry), in RAGE space.
pub struct TriPrim {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    /// Tangents (x, y, z, w) in RAGE space, written as TANGENT.
    pub tangents: Option<Vec<[f32; 4]>>,
    /// UV sets in order (TEXCOORD_0, TEXCOORD_1, ...).
    pub uvs: Vec<Vec<[f32; 2]>>,
    /// Raw RGBA8 stream, written as the custom attribute `_COLOR0`
    /// (meaning unverified, so not COLOR_0).
    pub colors: Option<Vec<[u8; 4]>>,
    /// Per-vertex colour (linear RGBA8) written as COLOR_0, which glTF
    /// multiplies into the base colour: the model's tint palette entry.
    pub tint: Option<Vec<[u8; 4]>>,
    pub indices: Vec<u32>,
    pub material: Option<usize>,
    /// Per-vertex skin joints (skeleton bone indices) and u8 weights
    /// (sum 255), written as JOINTS_0 / WEIGHTS_0 when a skeleton is given.
    pub joints: Option<Vec<[u16; 4]>>,
    pub weights: Option<Vec<[u8; 4]>>,
    /// Influences 5-8, written as JOINTS_1 / WEIGHTS_1. The 8 weights
    /// together sum to 255.
    pub joints1: Option<Vec<[u16; 4]>>,
    pub weights1: Option<Vec<[u8; 4]>>,
}

/// A skeleton joint: name, parent index, bind pose (bone -> model,
/// column-major 4x4, RAGE Z-up).
pub struct Joint {
    pub name: String,
    pub parent: Option<usize>,
    pub bind: [f32; 16],
    /// Rest pose (bone -> model, Z-up) when it differs from the bind pose,
    /// e.g. a body shape applied to the shared ped skeleton. The node
    /// transforms use it; the inverse bind matrices still use `bind`.
    pub pose: Option<[f32; 16]>,
}

/// Joint property an animation channel drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimPath {
    Translation,
    Rotation,
    Scale,
}

/// Keyframes of one joint property, in RAGE space (Z-up) and relative to
/// the parent joint: translation / scale (x, y, z, unused), rotation
/// quaternion (x, y, z, w). Times in seconds, ascending.
pub struct AnimChannel {
    pub joint: usize,
    pub path: AnimPath,
    pub times: Vec<f32>,
    pub values: Vec<[f32; 4]>,
}

/// A glTF animation over the skeleton joints.
pub struct Animation {
    pub name: String,
    pub channels: Vec<AnimChannel>,
}

/// Extra choices for `mesh_glb_with`.
#[derive(Default)]
pub struct GlbOptions<'a> {
    /// Animations of the skeleton joints. Joint nodes are written as TRS
    /// when there are any (glTF animates TRS nodes only).
    pub animations: &'a [Animation],
    /// Write joint nodes as translation / rotation / scale instead of a matrix.
    pub trs_joints: bool,
    /// Put skinned mesh nodes at the scene root (glTF ignores their parent
    /// transforms anyway) instead of under one node per group.
    pub skinned_meshes_at_root: bool,
}

/// Split an affine column-major 4x4 into translation, rotation quaternion
/// (x, y, z, w) and scale. Assumes no shear.
pub fn decompose(m: &[f32; 16]) -> ([f32; 3], [f32; 4], [f32; 3]) {
    let col = |c: usize| [m[c * 4] as f64, m[c * 4 + 1] as f64, m[c * 4 + 2] as f64];
    let len = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let (c0, c1, c2) = (col(0), col(1), col(2));
    let mut sc = [len(c0), len(c1), len(c2)];
    let det = c0[0] * (c1[1] * c2[2] - c2[1] * c1[2]) - c1[0] * (c0[1] * c2[2] - c2[1] * c0[2]) + c2[0] * (c0[1] * c1[2] - c1[1] * c0[2]);
    if det < 0.0 {
        sc[0] = -sc[0];
    }
    let r = |row: usize, c: usize| [c0, c1, c2][c][row] / if sc[c].abs() < 1e-12 { 1.0 } else { sc[c] };
    let tr = r(0, 0) + r(1, 1) + r(2, 2);
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [(r(2, 1) - r(1, 2)) / s, (r(0, 2) - r(2, 0)) / s, (r(1, 0) - r(0, 1)) / s, 0.25 * s]
    } else if r(0, 0) > r(1, 1) && r(0, 0) > r(2, 2) {
        let s = (1.0 + r(0, 0) - r(1, 1) - r(2, 2)).sqrt() * 2.0;
        [0.25 * s, (r(0, 1) + r(1, 0)) / s, (r(0, 2) + r(2, 0)) / s, (r(2, 1) - r(1, 2)) / s]
    } else if r(1, 1) > r(2, 2) {
        let s = (1.0 + r(1, 1) - r(0, 0) - r(2, 2)).sqrt() * 2.0;
        [(r(0, 1) + r(1, 0)) / s, 0.25 * s, (r(1, 2) + r(2, 1)) / s, (r(0, 2) - r(2, 0)) / s]
    } else {
        let s = (1.0 + r(2, 2) - r(0, 0) - r(1, 1)).sqrt() * 2.0;
        [(r(0, 2) + r(2, 0)) / s, (r(1, 2) + r(2, 1)) / s, 0.25 * s, (r(1, 0) - r(0, 1)) / s]
    };
    let n = (q.iter().map(|x| x * x).sum::<f64>()).sqrt().max(1e-12);
    let q = [q[0] / n, q[1] / n, q[2] / n, q[3] / n];
    ([m[12], m[13], m[14]], q.map(|x| x as f32), sc.map(|x| x as f32))
}

/// A RAGE (Z-up) channel value in glTF (Y-up) space.
fn yup_value(path: AnimPath, v: [f32; 4]) -> [f32; 4] {
    match path {
        AnimPath::Translation => [v[0], v[2], -v[1], 0.0],
        // C is a proper rotation, so the quaternion's vector part turns with it
        AnimPath::Rotation => [v[0], v[2], -v[1], v[3]],
        AnimPath::Scale => [v[0], v[2], v[1], 0.0],
    }
}

/// RAGE (Z-up) -> glTF (Y-up) for a column-major 4x4: C * M * C^T with
/// C = (x, y, z) -> (x, z, -y).
fn yup_mat(m: &[f32; 16]) -> [f32; 16] {
    let c = [[1.0f32, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]];
    let a = |r: usize, k: usize| m[k * 4 + r];
    let mut o = [0f32; 16];
    for r in 0..3 {
        for k in 0..3 {
            let mut v = 0.0;
            for i in 0..3 {
                for j in 0..3 {
                    v += c[r][i] * a(i, j) * c[k][j];
                }
            }
            o[k * 4 + r] = v;
        }
        o[12 + r] = (0..3).map(|i| c[r][i] * a(i, 3)).sum();
    }
    o[15] = 1.0;
    o
}

/// Product of two column-major 4x4 matrices (a · b).
pub fn mat_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut o = [0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

/// A group of primitives under one parent node (one RAGE model).
pub struct NodeGroup {
    pub name: String,
    pub prims: Vec<TriPrim>,
}

pub struct Material {
    pub name: String,
    /// Relative URI of a base-colour image, if resolved.
    pub base_color_uri: Option<String>,
    /// Relative URI of a tangent-space normal map, if resolved.
    pub normal_uri: Option<String>,
    /// Extra glTF material fields (alphaMode, pbrMetallicRoughness factors), merged in.
    pub gltf: Value,
    pub extras: Value,
}

fn yup(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[2], -v[1]]
}

#[derive(Default)]
struct Builder {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Builder {
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let off = self.bin.len();
        self.bin.extend_from_slice(bytes);
        let mut v = json!({"buffer": 0, "byteOffset": off, "byteLength": bytes.len()});
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.views.push(v);
        self.views.len() - 1
    }
    fn vec3(&mut self, data: &[[f32; 3]], minmax: bool) -> usize {
        let mut bytes = Vec::with_capacity(data.len() * 12);
        let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in data {
            for k in 0..3 {
                mn[k] = mn[k].min(v[k]);
                mx[k] = mx[k].max(v[k]);
                bytes.extend_from_slice(&v[k].to_le_bytes());
            }
        }
        let bv = self.view(&bytes, Some(34962));
        let mut a = json!({"bufferView": bv, "componentType": 5126, "count": data.len(), "type": "VEC3"});
        if minmax {
            a["min"] = json!(mn);
            a["max"] = json!(mx);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }
    fn vec4(&mut self, data: &[[f32; 4]]) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.iter().flat_map(|x| x.to_le_bytes())).collect();
        let bv = self.view(&bytes, Some(34962));
        self.accessors.push(json!({"bufferView": bv, "componentType": 5126, "count": data.len(), "type": "VEC4"}));
        self.accessors.len() - 1
    }
    fn vec2(&mut self, data: &[[f32; 2]]) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| [v[0].to_le_bytes(), v[1].to_le_bytes()].concat()).collect();
        let bv = self.view(&bytes, Some(34962));
        self.accessors.push(json!({"bufferView": bv, "componentType": 5126, "count": data.len(), "type": "VEC2"}));
        self.accessors.len() - 1
    }
    fn rgba8(&mut self, data: &[[u8; 4]]) -> usize {
        let bytes: Vec<u8> = data.iter().flatten().copied().collect();
        let bv = self.view(&bytes, Some(34962));
        self.accessors.push(json!({"bufferView": bv, "componentType": 5121, "normalized": true, "count": data.len(), "type": "VEC4"}));
        self.accessors.len() - 1
    }
    fn scalar(&mut self, data: &[f32]) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let bv = self.view(&bytes, Some(34962));
        self.accessors.push(json!({"bufferView": bv, "componentType": 5126, "count": data.len(), "type": "SCALAR"}));
        self.accessors.len() - 1
    }
    fn indices(&mut self, data: &[u32]) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|i| i.to_le_bytes()).collect();
        let bv = self.view(&bytes, Some(34963));
        self.accessors.push(json!({"bufferView": bv, "componentType": 5125, "count": data.len(), "type": "SCALAR"}));
        self.accessors.len() - 1
    }
}

pub(crate) fn pack(doc: &Value, mut bin: Vec<u8>) -> Vec<u8> {
    let mut js = serde_json::to_vec(doc).unwrap();
    while js.len() % 4 != 0 {
        js.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

/// Build a triangle-mesh GLB: a root node, one child node per group, and
/// one node + mesh per primitive under it. Materials are double-sided.
pub fn mesh_glb(name: &str, groups: &[NodeGroup], materials: &[Material], skeleton: &[Joint], extras: Value) -> Vec<u8> {
    mesh_glb_with(name, groups, materials, skeleton, extras, &GlbOptions::default())
}

/// `mesh_glb` with animations and node layout options.
pub fn mesh_glb_with(name: &str, groups: &[NodeGroup], materials: &[Material], skeleton: &[Joint], extras: Value, opt: &GlbOptions) -> Vec<u8> {
    let mut b = Builder::default();
    let trs = opt.trs_joints || !opt.animations.is_empty();
    let mut nodes: Vec<Value> = Vec::new();
    // Joint nodes first (indices 0..n), local matrices from model binds.
    let binds: Vec<[f32; 16]> = skeleton.iter().map(|j| yup_mat(&j.bind)).collect();
    let poses: Vec<[f32; 16]> = skeleton.iter().map(|j| yup_mat(j.pose.as_ref().unwrap_or(&j.bind))).collect();
    for (i, j) in skeleton.iter().enumerate() {
        let local = match j.parent.filter(|&p| p < skeleton.len()) {
            Some(p) => mat_mul(&crate::drawable::affine_inverse(&poses[p]), &poses[i]),
            None => poses[i],
        };
        let children: Vec<usize> = skeleton.iter().enumerate().filter(|(_, c)| c.parent == Some(i)).map(|(k, _)| k).collect();
        let mut nd = json!({"name": j.name});
        if trs {
            let (t, r, sc) = decompose(&local);
            nd["translation"] = json!(t);
            nd["rotation"] = json!(r);
            nd["scale"] = json!(sc);
        } else {
            nd["matrix"] = json!(local);
        }
        if !children.is_empty() {
            nd["children"] = json!(children);
        }
        nodes.push(nd);
    }
    let joint_roots: Vec<usize> = skeleton.iter().enumerate().filter(|(_, j)| j.parent.map(|p| p >= skeleton.len()).unwrap_or(true)).map(|(i, _)| i).collect();
    let skin = if skeleton.is_empty() {
        None
    } else {
        let ibm: Vec<u8> = binds.iter().flat_map(|m| crate::drawable::affine_inverse(m)).flat_map(|f| f.to_le_bytes()).collect();
        let bv = b.view(&ibm, None);
        b.accessors.push(json!({"bufferView": bv, "componentType": 5126, "count": skeleton.len(), "type": "MAT4"}));
        let mut sk = json!({"name": "skeleton", "joints": (0..skeleton.len()).collect::<Vec<_>>(), "inverseBindMatrices": b.accessors.len() - 1});
        // `skeleton` must be a common root of all joints; with several joint
        // roots there is none among the joints, so it is left out.
        if joint_roots.len() == 1 {
            sk["skeleton"] = json!(joint_roots[0]);
        }
        Some(sk)
    };
    let mut meshes = Vec::new();
    let mut group_nodes = Vec::new();
    let mut scene_meshes = Vec::new();
    let mut images = Vec::new();
    let mut textures = Vec::new();
    let mats: Vec<Value> = materials
        .iter()
        .map(|m| {
            let mut pbr = json!({"metallicFactor": 0.0, "roughnessFactor": 0.8});
            if let Some(uri) = &m.base_color_uri {
                images.push(json!({"uri": uri}));
                textures.push(json!({"source": images.len() - 1}));
                pbr["baseColorTexture"] = json!({"index": textures.len() - 1});
            }
            let mut mat = json!({"name": m.name, "doubleSided": true, "pbrMetallicRoughness": pbr, "extras": m.extras});
            if let Some(uri) = &m.normal_uri {
                images.push(json!({"uri": uri}));
                textures.push(json!({"source": images.len() - 1}));
                mat["normalTexture"] = json!({"index": textures.len() - 1});
            }
            if let Some(o) = m.gltf.as_object() {
                for (k, v) in o {
                    if k == "pbrMetallicRoughness" {
                        if let Some(po) = v.as_object() {
                            for (pk, pv) in po {
                                mat["pbrMetallicRoughness"][pk] = pv.clone();
                            }
                        }
                    } else {
                        mat[k] = v.clone();
                    }
                }
            }
            // "emissiveTexture": "base" = light with the base-colour texture;
            // {"uri": …} = the material's own emissive map. With neither
            // texture nothing lights up (a flat white glow would be wrong).
            if let Some(uri) = mat["emissiveTexture"]["uri"].as_str().map(|u| u.to_string()) {
                images.push(json!({"uri": uri}));
                textures.push(json!({"source": images.len() - 1}));
                mat["emissiveTexture"] = json!({"index": textures.len() - 1});
            } else if mat["emissiveTexture"] == "base" {
                match mat["pbrMetallicRoughness"]["baseColorTexture"].get("index").cloned() {
                    Some(i) => mat["emissiveTexture"] = json!({"index": i}),
                    None => {
                        let o = mat.as_object_mut().unwrap();
                        o.remove("emissiveTexture");
                        o.remove("emissiveFactor");
                    }
                }
            }
            mat
        })
        .collect();
    for g in groups {
        let mut children = Vec::new();
        for p in g.prims.iter().filter(|p| !p.positions.is_empty() && !p.indices.is_empty()) {
            let pos: Vec<[f32; 3]> = p.positions.iter().map(|&v| yup(v)).collect();
            let mut attrs = json!({"POSITION": b.vec3(&pos, true)});
            if let Some(n) = &p.normals {
                let n: Vec<[f32; 3]> = n.iter().map(|&v| yup(v)).collect();
                attrs["NORMAL"] = json!(b.vec3(&n, false));
                if let Some(t) = &p.tangents {
                    let t: Vec<[f32; 4]> = t.iter().map(|v| { let y = yup([v[0], v[1], v[2]]); [y[0], y[1], y[2], v[3]] }).collect();
                    attrs["TANGENT"] = json!(b.vec4(&t));
                }
            }
            for (k, uv) in p.uvs.iter().enumerate() {
                attrs[format!("TEXCOORD_{k}")] = json!(b.vec2(uv));
            }
            if let Some(c) = &p.colors {
                attrs["_COLOR0"] = json!(b.rgba8(c));
            }
            if let Some(t) = &p.tint {
                attrs["COLOR_0"] = json!(b.rgba8(t));
            }
            let skinned = skin.is_some() && p.joints.is_some();
            if let (true, Some(js)) = (skinned, &p.joints) {
                let n = skeleton.len();
                let ws = p.weights.clone().unwrap_or_else(|| vec![[255, 0, 0, 0]; js.len()]);
                // Influences 5-8 when present (both sets together sum to 255).
                let second = match (&p.joints1, &p.weights1) {
                    (Some(j1), Some(w1)) if j1.len() == js.len() && w1.len() == js.len() => Some((j1, w1)),
                    _ => None,
                };
                let sets = if second.is_some() { 2 } else { 1 };
                let wide = js.iter().chain(second.iter().flat_map(|s| s.0.iter())).flatten().any(|&j| j > 255 && (j as usize) < n);
                let mut jb: Vec<Vec<u8>> = vec![Vec::with_capacity(js.len() * 8); sets];
                let mut wb: Vec<Vec<u8>> = vec![Vec::with_capacity(js.len() * 4); sets];
                for i in 0..js.len() {
                    let mut jj = [0u16; 8];
                    let mut ww = [0u8; 8];
                    jj[..4].copy_from_slice(&js[i]);
                    ww[..4].copy_from_slice(&ws[i]);
                    if let Some((j1, w1)) = second {
                        jj[4..].copy_from_slice(&j1[i]);
                        ww[4..].copy_from_slice(&w1[i]);
                    }
                    for k in 0..4 * sets {
                        if (jj[k] as usize) >= n {
                            jj[k] = 0;
                            ww[k] = 0;
                        }
                    }
                    // keep weights summing to 255 if an index was dropped:
                    // the remainder goes to the largest kept weight
                    let sum: u32 = ww.iter().map(|&x| x as u32).sum();
                    if sum != 255 {
                        let top = (0..4 * sets).max_by_key(|&k| ww[k]).unwrap_or(0);
                        ww[top] = (ww[top] as u32 + 255u32.saturating_sub(sum)).min(255) as u8;
                    }
                    // unused slots point at joint 0 (glTF: zero weight, joint 0)
                    for k in 0..4 * sets {
                        if ww[k] == 0 {
                            jj[k] = 0;
                        }
                    }
                    for s in 0..sets {
                        for k in 4 * s..4 * s + 4 {
                            if wide { jb[s].extend_from_slice(&jj[k].to_le_bytes()) } else { jb[s].push(jj[k] as u8) }
                            wb[s].push(ww[k]);
                        }
                    }
                }
                for s in 0..sets {
                    let bv = b.view(&jb[s], Some(34962));
                    b.accessors.push(json!({"bufferView": bv, "componentType": if wide { 5123 } else { 5121 }, "count": js.len(), "type": "VEC4"}));
                    attrs[format!("JOINTS_{s}")] = json!(b.accessors.len() - 1);
                    let bv = b.view(&wb[s], Some(34962));
                    b.accessors.push(json!({"bufferView": bv, "componentType": 5121, "normalized": true, "count": js.len(), "type": "VEC4"}));
                    attrs[format!("WEIGHTS_{s}")] = json!(b.accessors.len() - 1);
                }
            }
            let mut prim = json!({"attributes": attrs, "indices": b.indices(&p.indices), "mode": 4});
            if let Some(m) = p.material.filter(|&m| m < mats.len()) {
                prim["material"] = json!(m);
            }
            meshes.push(json!({"name": p.name, "primitives": [prim]}));
            let mut nd = json!({"name": p.name, "mesh": meshes.len() - 1});
            if skinned {
                nd["skin"] = json!(0);
            }
            nodes.push(nd);
            if skinned && opt.skinned_meshes_at_root {
                scene_meshes.push(nodes.len() - 1);
            } else {
                children.push(nodes.len() - 1);
            }
        }
        if !children.is_empty() || !opt.skinned_meshes_at_root {
            nodes.push(json!({"name": g.name, "children": children}));
            group_nodes.push(nodes.len() - 1);
        }
    }
    let mut root_children = joint_roots.clone();
    root_children.extend(group_nodes);
    nodes.push(json!({"name": name, "children": root_children}));
    let root = nodes.len() - 1;
    let mut scene_nodes = vec![root];
    scene_nodes.extend(scene_meshes);
    // Animations: one sampler per channel, time accessors shared.
    let mut anims = Vec::new();
    let mut time_acc: std::collections::HashMap<Vec<u32>, usize> = Default::default();
    for an in opt.animations {
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for ch in an.channels.iter().filter(|c| c.joint < skeleton.len() && !c.times.is_empty() && c.times.len() == c.values.len()) {
            let key: Vec<u32> = ch.times.iter().map(|t| t.to_bits()).collect();
            let input = *time_acc.entry(key).or_insert_with(|| {
                let bytes: Vec<u8> = ch.times.iter().flat_map(|t| t.to_le_bytes()).collect();
                let bv = b.view(&bytes, None);
                let (mn, mx) = (ch.times[0], *ch.times.last().unwrap());
                b.accessors.push(json!({"bufferView": bv, "componentType": 5126, "count": ch.times.len(), "type": "SCALAR", "min": [mn], "max": [mx]}));
                b.accessors.len() - 1
            });
            let n = if ch.path == AnimPath::Rotation { 4 } else { 3 };
            let bytes: Vec<u8> = ch.values.iter().flat_map(|v| {
                let mut y = yup_value(ch.path, *v);
                if ch.path == AnimPath::Rotation {
                    let l = (y.iter().map(|x| x * x).sum::<f32>()).sqrt().max(1e-12);
                    y = y.map(|x| x / l);
                }
                y[..n].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()
            }).collect();
            let bv = b.view(&bytes, None);
            b.accessors.push(json!({"bufferView": bv, "componentType": 5126, "count": ch.values.len(), "type": if n == 4 { "VEC4" } else { "VEC3" }}));
            samplers.push(json!({"input": input, "output": b.accessors.len() - 1, "interpolation": "LINEAR"}));
            let path = match ch.path {
                AnimPath::Translation => "translation",
                AnimPath::Rotation => "rotation",
                AnimPath::Scale => "scale",
            };
            channels.push(json!({"sampler": samplers.len() - 1, "target": {"node": ch.joint, "path": path}}));
        }
        if !channels.is_empty() {
            anims.push(json!({"name": an.name, "samplers": samplers, "channels": channels}));
        }
    }
    let mut doc = json!({
        "asset": {"version": "2.0", "generator": "sixng-extractor"},
        "scene": 0,
        "scenes": [{"name": name, "nodes": scene_nodes, "extras": extras}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": mats,
        "accessors": b.accessors,
        "bufferViews": b.views,
        "buffers": [{"byteLength": b.bin.len()}],
    });
    if let Some(sk) = skin {
        doc["skins"] = json!([sk]);
    }
    if !anims.is_empty() {
        doc["animations"] = json!(anims);
    }
    if !images.is_empty() {
        doc["images"] = json!(images);
        doc["textures"] = json!(textures);
        doc["samplers"] = json!([{"wrapS": 10497, "wrapT": 10497}]);
        for t in doc["textures"].as_array_mut().unwrap() {
            t["sampler"] = json!(0);
        }
    }
    pack(&doc, b.bin)
}

/// Build a line GLB: one node with line segments (mode 1, `segments` =
/// index pairs into `positions`) and, when `tris` is not empty, a second
/// node with triangles over the same positions (mode 4). RAGE space in.
/// `width` (custom `_WIDTH`) and `uv` (TEXCOORD_0) are written when they
/// have one value per position.
pub fn lines_glb(name: &str, positions: &[[f32; 3]], width: &[f32], uv: &[[f32; 2]], segments: &[u32], tris: &[u32], extras: Value) -> Vec<u8> {
    let mut b = Builder::default();
    let pos: Vec<[f32; 3]> = positions.iter().map(|&v| yup(v)).collect();
    let a = b.vec3(&pos, true);
    let mut attrs = json!({"POSITION": a});
    if width.len() == positions.len() {
        attrs["_WIDTH"] = json!(b.scalar(width));
    }
    if uv.len() == positions.len() {
        attrs["TEXCOORD_0"] = json!(b.vec2(uv));
    }
    let li = b.indices(segments);
    let mut meshes = vec![json!({"name": format!("{name}_strands"), "primitives": [{"attributes": attrs, "indices": li, "mode": 1}]})];
    let mut nodes = vec![json!({"name": format!("{name}_strands"), "mesh": 0})];
    if !tris.is_empty() {
        let ti = b.indices(tris);
        meshes.push(json!({"name": format!("{name}_roots"), "primitives": [{"attributes": {"POSITION": a}, "indices": ti, "mode": 4, "material": 0}]}));
        nodes.push(json!({"name": format!("{name}_roots"), "mesh": 1}));
    }
    let n = nodes.len();
    nodes.push(json!({"name": name, "children": (0..n).collect::<Vec<_>>()}));
    let doc = json!({
        "asset": {"version": "2.0", "generator": "sixng-extractor (lines)"},
        "scene": 0,
        "scenes": [{"name": name, "nodes": [n], "extras": extras}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": [{"name": "roots", "doubleSided": true, "pbrMetallicRoughness": {"baseColorFactor": [0.5, 0.5, 0.5, 1.0], "metallicFactor": 0.0}}],
        "accessors": b.accessors,
        "bufferViews": b.views,
        "buffers": [{"byteLength": b.bin.len()}],
    });
    pack(&doc, b.bin)
}

/// Build a point-cloud GLB (mode 0), one node per primitive.
pub fn points_glb(name: &str, prims: &[PointPrim], extras: Value) -> Vec<u8> {
    let mut b = Builder::default();
    let mut nodes = Vec::new();
    let mut meshes = Vec::new();
    for p in prims.iter().filter(|p| !p.positions.is_empty()) {
        let pos: Vec<[f32; 3]> = p.positions.iter().map(|&v| yup(v)).collect();
        let a = b.vec3(&pos, true);
        meshes.push(json!({"name": p.name, "primitives": [{"attributes": {"POSITION": a}, "mode": 0}]}));
        nodes.push(json!({"name": p.name, "mesh": meshes.len() - 1}));
    }
    let n = nodes.len();
    nodes.push(json!({"name": name, "children": (0..n).collect::<Vec<_>>()}));
    let doc = json!({
        "asset": {"version": "2.0", "generator": "sixng-extractor (points)"},
        "scene": 0,
        "scenes": [{"name": name, "nodes": [n], "extras": extras}],
        "nodes": nodes,
        "meshes": meshes,
        "accessors": b.accessors,
        "bufferViews": b.views,
        "buffers": [{"byteLength": b.bin.len()}],
    });
    pack(&doc, b.bin)
}
