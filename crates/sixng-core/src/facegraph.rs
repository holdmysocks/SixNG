//! Face compute graphs: the expression shape keys of ped heads.
//!
//! Heads store no morph targets. The game deforms a face with a small
//! network per face, run on the GPU by the `compute_graph_ops` shaders:
//! expression controls in, one xyz translation per driven face vertex out.
//! `ped_standard_head_cs:CS_BlendshapeTranslationsHSSGraphOutputExtract`
//! adds the output to the vertex (`pos = base + out[3 * gv ..]`, after the
//! identity delta) and `CS_ReconstructBlendshapeNormals` rebuilds normals.
//!
//! Two unnamed `packs.rpf` resources make one network (table-of-contents
//! resource types 95 ComputeGraph and 114 ComputeGraphConstants), paired by
//! the 64-bit id at graph sys+0x48 / constants sys+0x20.
//!
//! Graph system page: u32 0x10 = byte length of the op records, ptr 0x18 =
//! first record (0x60). A record is u32 `stage << 8 | op` and a fixed number
//! of u32 fields per op; a header whose op byte is 0 (`0xff00`) ends the
//! list. Buffer handles end in 0x03 / 0x83; handle 3 is the output scratch
//! buffer. A constant field holds `(offset / 16) << 7 | 2` into the
//! constants' graphics page. Op ids are the exe's op enum (names by joaat):
//!
//! | op   | name             | fields used                                   |
//! |------|------------------|-----------------------------------------------|
//! | 0x0b | (input upload)   | out                                           |
//! | 0x2e | MatMul           | out, in, const, 1, K, N (fp16 K x N, no bias) |
//! | 0x2f | Transpose_MatMul | out, in, const, 1, K, N (fp16 N x K)          |
//! | 0x35 | Concat           | out, 2, 18, 1, 1, a, b                        |
//! | 0x36 | Slice_Copy       | out, in, 2, width .. offset at 14, len at 19  |
//! | 0x3c | Gather           | out, in, const (u32 indices), 2, 1, count     |
//! | 0x43 | Place            | dst, src, dst offset, src offset, len         |
//! | 0x44 | Place_Add        | the same, added                               |
//!
//! There are no activation ops and no biases, so a face network is linear in
//! its controls: control `i` at weight `w` moves the face by `w * basis[i]`,
//! which is exactly a shape key.
//!
//! The head's LOD0 mesh stream 8 (format 0x8c, 16 bytes) is not skinning on
//! heads: the high u16 of its first word is the graph vertex + 1 (0xffff =
//! not driven), the second the position id + 1 (welded across UV seams), the
//! third another vertex number + 1 (1..vertex count, not the stored order),
//! the fourth the tag 0xb600.
use anyhow::{bail, ensure, Result};
use std::collections::HashMap;

/// u32 count of each op's record, header included.
fn record_len(op: u8) -> Option<usize> {
    Some(match op {
        0x0b => 10,
        0x2e => 24,
        0x2f => 39,
        0x35 => 12,
        0x36 => 40,
        0x3c => 25,
        0x43 | 0x44 => 6,
        _ => return None,
    })
}

#[derive(Clone, Debug)]
pub struct Op {
    pub stage: u32,
    pub op: u8,
    pub f: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct Graph {
    /// Pairs the graph with its constants.
    pub id: u64,
    pub inputs: usize,
    pub outputs: usize,
    pub ops: Vec<Op>,
}

impl Graph {
    /// Driven vertices (3 outputs each).
    pub fn vertices(&self) -> usize {
        self.outputs / 3
    }
}

fn u32le(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()))
}
fn u64le(b: &[u8], o: usize) -> Option<u64> {
    b.get(o..o + 8).map(|x| u64::from_le_bytes(x.try_into().unwrap()))
}

/// Parse a face graph's system page. `None` for anything else (other
/// compute graphs use ops this reader does not run).
pub fn parse(sys: &[u8]) -> Option<Graph> {
    if sys.len() < 0x60 || u64le(sys, 0x18)? != 0x5000_0060 {
        return None;
    }
    let n = u32le(sys, 0x10)? as usize;
    let end = (0x60 + n).min(sys.len());
    let mut o = 0x60;
    let mut ops = Vec::new();
    while o + 4 <= end {
        let h = u32le(sys, o)?;
        let op = (h & 0xff) as u8;
        if op == 0 {
            break;
        }
        let len = record_len(op)?;
        let f: Vec<u32> = (1..len).map(|k| u32le(sys, o + 4 * k)).collect::<Option<_>>()?;
        ops.push(Op { stage: h >> 8, op, f });
        o += 4 * len;
    }
    let gathers: Vec<&Op> = ops.iter().filter(|x| x.op == 0x3c).collect();
    if gathers.len() != 2 || !ops.iter().any(|x| matches!(x.op, 0x2e | 0x2f)) {
        return None;
    }
    let (inputs, outputs) = (gathers[0].f[5] as usize, gathers[1].f[5] as usize);
    if inputs == 0 || outputs == 0 || outputs % 3 != 0 {
        return None;
    }
    Some(Graph { id: u64le(sys, 0x48)?, inputs, outputs, ops })
}

/// The id of a constants resource (system page of 176 bytes).
pub fn constants_id(sys: &[u8]) -> Option<u64> {
    (sys.len() == 176 && u64le(sys, 0x10)? == 0x5000_0030).then(|| u64le(sys, 0x20)).flatten()
}

/// Batched row-major buffer: `rows` x `w`.
struct Buf {
    w: usize,
    d: Vec<f32>,
}

fn const_off(v: u32) -> usize {
    (v >> 7) as usize * 16
}

/// The linear map of a graph: `basis[i * outputs + k]` = output `k` for
/// control `i` at 1 and every other control at 0. `constants` is the
/// constants resource's graphics page.
pub fn basis(g: &Graph, constants: &[u8]) -> Result<Vec<f32>> {
    use rayon::prelude::*;
    let rows = g.inputs;
    let mut bufs: HashMap<u32, Buf> = HashMap::new();
    let zero = |w: usize| Buf { w, d: vec![0.0; rows * w] };
    let indices = |off: usize, n: usize| -> Result<Vec<usize>> {
        let b = constants.get(off..off + 4 * n).ok_or_else(|| anyhow::anyhow!("gather indices out of range"))?;
        Ok(b.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap()) as usize).collect())
    };
    let mut first_gather = true;
    let mut out_handle = None;
    for op in &g.ops {
        let f = &op.f;
        match op.op {
            0x0b => {}
            0x3c => {
                let n = f[5] as usize;
                let p = indices(const_off(f[2]), n)?;
                let src = if first_gather {
                    // the controls: identity rows
                    let mut b = zero(g.inputs);
                    for i in 0..rows {
                        b.d[i * g.inputs + i] = 1.0;
                    }
                    b
                } else {
                    bufs.remove(&f[1]).unwrap_or_else(|| zero(n))
                };
                ensure!(p.iter().all(|&i| i < src.w), "gather index past the source");
                let mut o = zero(n);
                for r in 0..rows {
                    for (k, &i) in p.iter().enumerate() {
                        o.d[r * n + k] = src.d[r * src.w + i];
                    }
                }
                if !first_gather {
                    out_handle = Some(f[0]);
                }
                first_gather = false;
                bufs.insert(f[0], o);
            }
            0x36 => {
                let (w, a, n) = (f[3] as usize, f[14] as usize, f[19] as usize);
                let src = bufs.entry(f[1]).or_insert_with(|| zero(w));
                ensure!(a + n <= src.w, "slice past the buffer");
                let mut o = zero(n);
                for r in 0..rows {
                    o.d[r * n..(r + 1) * n].copy_from_slice(&src.d[r * src.w + a..r * src.w + a + n]);
                }
                bufs.insert(f[0], o);
            }
            0x2e | 0x2f => {
                let (k, n) = (f[4] as usize, f[5] as usize);
                let off = const_off(f[2]);
                let wb = constants.get(off..off + 2 * k * n).ok_or_else(|| anyhow::anyhow!("weights out of range"))?;
                let wt: Vec<f32> = wb.chunks_exact(2).map(|c| crate::groom::f16(u16::from_le_bytes([c[0], c[1]]))).collect();
                // as K x N, row-major
                let w: Vec<f32> = if op.op == 0x2f { (0..k * n).map(|x| wt[(x % n) * k + x / n]).collect() } else { wt };
                let src = bufs.get(&f[1]).ok_or_else(|| anyhow::anyhow!("matmul input {:x} not set", f[1]))?;
                ensure!(src.w == k, "matmul width {} != {k}", src.w);
                let mut o = zero(n);
                o.d.par_chunks_mut(n).enumerate().for_each(|(r, out)| {
                    let x = &src.d[r * k..(r + 1) * k];
                    for (j, &xv) in x.iter().enumerate() {
                        if xv != 0.0 {
                            let row = &w[j * n..(j + 1) * n];
                            for (y, &wv) in out.iter_mut().zip(row) {
                                *y += xv * wv;
                            }
                        }
                    }
                });
                bufs.insert(f[0], o);
            }
            0x35 => {
                let (a, b) = (bufs.get(&f[5]).ok_or_else(|| anyhow::anyhow!("concat input"))?, bufs.get(&f[6]).ok_or_else(|| anyhow::anyhow!("concat input"))?);
                let w = a.w + b.w;
                let mut o = zero(w);
                for r in 0..rows {
                    o.d[r * w..r * w + a.w].copy_from_slice(&a.d[r * a.w..(r + 1) * a.w]);
                    o.d[r * w + a.w..(r + 1) * w].copy_from_slice(&b.d[r * b.w..(r + 1) * b.w]);
                }
                bufs.insert(f[0], o);
            }
            0x43 | 0x44 => {
                let (dsto, srco, n) = (f[2] as usize, f[3] as usize, f[4] as usize);
                let src = bufs.remove(&f[1]).ok_or_else(|| anyhow::anyhow!("place source {:x} not set", f[1]))?;
                ensure!(srco + n <= src.w, "place past the source");
                let min_w = if f[0] == 3 { g.outputs } else { dsto + n };
                let mut dst = bufs.remove(&f[0]).unwrap_or_else(|| zero(min_w));
                if dst.w < dsto + n {
                    let mut d2 = zero(dsto + n);
                    for r in 0..rows {
                        d2.d[r * d2.w..r * d2.w + dst.w].copy_from_slice(&dst.d[r * dst.w..(r + 1) * dst.w]);
                    }
                    dst = d2;
                }
                for r in 0..rows {
                    let s = &src.d[r * src.w + srco..r * src.w + srco + n];
                    let d = &mut dst.d[r * dst.w + dsto..r * dst.w + dsto + n];
                    if op.op == 0x43 {
                        d.copy_from_slice(s);
                    } else {
                        for (a, b) in d.iter_mut().zip(s) {
                            *a += b;
                        }
                    }
                }
                if f[1] != f[0] {
                    bufs.insert(f[1], src);
                }
                bufs.insert(f[0], dst);
            }
            x => bail!("op {x:#x} not supported"),
        }
    }
    let h = out_handle.ok_or_else(|| anyhow::anyhow!("no output gather"))?;
    let o = bufs.remove(&h).unwrap();
    ensure!(o.w == g.outputs, "output width {} != {}", o.w, g.outputs);
    Ok(o.d)
}

/// Graph vertex of each mesh vertex, from a head mesh's stream 8 (16 bytes
/// per vertex): `None` where the vertex is not driven.
pub fn vertex_map(stream8: &[u8], vertices: usize) -> Option<Vec<Option<u32>>> {
    let b = stream8.get(..16 * vertices)?;
    let map: Vec<Option<u32>> = b
        .chunks_exact(16)
        .map(|c| {
            let hi = u16::from_le_bytes([c[2], c[3]]);
            (hi != 0 && hi != 0xffff).then(|| hi as u32 - 1)
        })
        .collect();
    // the fourth word's high half is the tag 0xb600 on every vertex: a check
    // that this is a mapping stream and not blend indices
    let ok = b.chunks_exact(16).all(|c| u16::from_le_bytes([c[14], c[15]]) == 0xb600);
    ok.then_some(map)
}

/// A face network ready to use: its linear map and size.
pub struct FaceNet {
    /// Archive offset of the graph resource.
    pub graph: u64,
    pub inputs: usize,
    pub outputs: usize,
    /// `inputs` x `outputs`, row-major (see [`basis`]).
    pub basis: Vec<f32>,
}

impl FaceNet {
    pub fn vertices(&self) -> usize {
        self.outputs / 3
    }

    /// Offset of graph vertex `gv` for control `i` at weight 1.
    pub fn delta(&self, i: usize, gv: usize) -> [f32; 3] {
        let o = i * self.outputs + 3 * gv;
        [self.basis[o], self.basis[o + 1], self.basis[o + 2]]
    }
}

/// Name of control `i` in exports (the controls' own names are not known).
pub fn control_name(i: usize) -> String {
    format!("face_{i:03}")
}

type Ids = std::collections::HashMap<u64, Vec<u64>>;
type Small = std::collections::HashMap<u64, crate::scan::Entry>;

/// Per archive: file ids from the table of contents, and the unnamed
/// resources by offset (graphs and constants have no recovered names).
fn archive_tables(a: &crate::game::ArchiveInfo) -> Option<std::sync::Arc<(Ids, Small)>> {
    use std::sync::{Arc, Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<(Ids, Small)>>>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().ok()?;
    c.entry(a.id.clone())
        .or_insert_with(|| {
            let game = crate::export::game_root(a);
            let mut cipher = crate::gamecipher::open(&game).ok()?;
            let mut f = std::fs::File::open(&a.path).ok()?;
            let ids = crate::rpf9toc::file_ids(cipher.as_mut(), &mut f).ok()?;
            let ws = crate::export::Workspace::new(game, crate::names::work_dir()?);
            let ix = ws.load_index(a).ok()?;
            let small: Small = ix.entries.into_iter().filter(|e| e.name.is_none() && e.kind == crate::classify::Kind::RageObj).map(|e| (e.off, e)).collect();
            Some(Arc::new((ids, small)))
        })
        .clone()
}

/// The face networks stored under one file name (a head drawable's own
/// name: `<head>` for LOD0, `<head>_lod1` for LOD1), with their constants.
fn nets_named(a: &crate::game::ArchiveInfo, name: &str) -> Vec<std::sync::Arc<FaceNet>> {
    use std::sync::{Arc, Mutex, OnceLock};
    static NETS: OnceLock<Mutex<HashMap<u64, Option<Arc<FaceNet>>>>> = OnceLock::new();
    let Some(t) = archive_tables(a) else { return Vec::new() };
    let (ids, small) = (&t.0, &t.1);
    let offs = ids.get(&crate::meta7a::asset_id(&name.to_ascii_lowercase())).cloned().unwrap_or_default();
    let mut graphs = Vec::new();
    let mut consts: HashMap<u64, u64> = HashMap::new();
    for o in &offs {
        let Some(e) = small.get(o) else { continue };
        let Ok(r) = crate::resource::Resource::load_sys(&a.path, e) else { continue };
        if let Some(g) = parse(&r.sys) {
            graphs.push((*o, g));
        } else if let Some(id) = constants_id(&r.sys) {
            consts.insert(id, *o);
        }
    }
    let mut out = Vec::new();
    for (go, g) in graphs {
        let mut cache = NETS.get_or_init(Default::default).lock().unwrap();
        let net = cache
            .entry(go)
            .or_insert_with(|| {
                let ce = small.get(consts.get(&g.id)?)?;
                let r = crate::resource::Resource::load(&a.path, ce).ok()?;
                let b = basis(&g, &r.gfx).ok()?;
                Some(Arc::new(FaceNet { graph: go, inputs: g.inputs, outputs: g.outputs, basis: b }))
            })
            .clone();
        out.extend(net);
    }
    out
}

/// The face networks of a head drawable (LOD0 and LOD1 graphs), smallest
/// first. Empty when the head has none (or the table of contents cannot be
/// read on this platform).
pub fn nets_for_head(a: &crate::game::ArchiveInfo, head_name: &str) -> Vec<std::sync::Arc<FaceNet>> {
    let mut v = nets_named(a, head_name);
    v.extend(nets_named(a, &format!("{head_name}_lod1")));
    v.sort_by_key(|n| n.outputs);
    v.dedup_by_key(|n| n.graph);
    v
}

/// Morph targets for one head geometry: one per control that moves it,
/// named by [`control_name`], offsets per mesh vertex in model space.
/// `map` is [`vertex_map`] of the geometry; the network used is the
/// smallest one that covers every graph vertex the mesh names.
pub fn morph_targets(nets: &[std::sync::Arc<FaceNet>], map: &[Option<u32>]) -> Vec<(String, Vec<[f32; 3]>)> {
    let Some(top) = map.iter().flatten().max().copied() else { return Vec::new() };
    let Some(net) = nets.iter().find(|n| n.vertices() > top as usize) else { return Vec::new() };
    (0..net.inputs)
        .filter_map(|i| {
            let d: Vec<[f32; 3]> = map
                .iter()
                .map(|g| match g {
                    // below 0.01 mm: zero (keeps the sparse targets small)
                    Some(g) => net.delta(i, *g as usize).map(|x| if x.abs() < 1e-5 { 0.0 } else { x }),
                    None => [0.0; 3],
                })
                .collect();
            d.iter().any(|x| x.iter().any(|&c| c != 0.0)).then(|| (control_name(i), d))
        })
        .collect()
}

/// Pose correctives: besides its face networks, a character has one network
/// stored under its ped model's name (`player_story000_00`: 294 inputs in 8
/// groups of 36 x 7 + 42, 4,675 points). It is a body surface that bends
/// with the pose, and garments sample it: a garment mesh's stream 8 (format
/// 0x8c, 16 bytes) holds three surface points + 1 in the high halves of its
/// first three words and the tag 0xb210 or 0xb220 in the fourth (all zero
/// where the vertex does not follow the surface). A vertex moves by the mean
/// of its three points (the equal-weight mean fits the garments' own
/// neighbours best: the low halves do not work as weights). What feeds the
/// inputs (body pose rigs reading the skeleton's rotations) is not decoded,
/// so the inputs are exported as plain shape keys.
pub fn surface_map(stream8: &[u8], vertices: usize) -> Option<Vec<Option<[u32; 3]>>> {
    let b = stream8.get(..16 * vertices)?;
    let mut tagged = false;
    let mut map = Vec::with_capacity(vertices);
    for c in b.chunks_exact(16) {
        let w = |k: usize| u32::from_le_bytes(c[4 * k..4 * k + 4].try_into().unwrap());
        let hi = |k: usize| w(k) >> 16;
        match hi(3) {
            0xb210 | 0xb220 if (0..3).all(|k| hi(k) != 0) => {
                tagged = true;
                map.push(Some([hi(0) - 1, hi(1) - 1, hi(2) - 1]));
            }
            0 if (0..3).all(|k| hi(k) == 0) => map.push(None),
            _ => return None,
        }
    }
    tagged.then_some(map)
}

/// The pose network of a ped part (`<ped model>_<part>`): the networks
/// stored under the longest leading `_`-joined piece of its name that has
/// any, largest first.
pub fn nets_for_part(a: &crate::game::ArchiveInfo, part_name: &str) -> Vec<std::sync::Arc<FaceNet>> {
    let words: Vec<&str> = part_name.split('_').collect();
    for n in (1..words.len()).rev() {
        let mut v = nets_named(a, &words[..n].join("_"));
        if !v.is_empty() {
            v.sort_by_key(|n| std::cmp::Reverse(n.outputs));
            return v;
        }
    }
    Vec::new()
}

/// Name of pose input `i` in exports.
pub fn pose_name(i: usize) -> String {
    format!("pose_{i:03}")
}

/// Shape keys of one garment geometry from its character's pose network:
/// one per input that moves it, named by [`pose_name`]. `map` is
/// [`surface_map`] of the geometry; the network used is the first that
/// covers every point the mesh names.
pub fn pose_targets(nets: &[std::sync::Arc<FaceNet>], map: &[Option<[u32; 3]>]) -> Vec<(String, Vec<[f32; 3]>)> {
    let Some(top) = map.iter().flatten().flatten().max().copied() else { return Vec::new() };
    let Some(net) = nets.iter().find(|n| n.vertices() > top as usize) else { return Vec::new() };
    (0..net.inputs)
        .filter_map(|i| {
            let d: Vec<[f32; 3]> = map
                .iter()
                .map(|p| match p {
                    Some(p) => {
                        let mut s = [0.0f32; 3];
                        for &g in p {
                            let x = net.delta(i, g as usize);
                            (0..3).for_each(|k| s[k] += x[k] / 3.0);
                        }
                        s.map(|x| if x.abs() < 1e-5 { 0.0 } else { x })
                    }
                    None => [0.0; 3],
                })
                .collect();
            d.iter().any(|x| x.iter().any(|&c| c != 0.0)).then(|| (pose_name(i), d))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(out: &mut Vec<u32>, stage: u32, op: u8, fields: &[u32]) {
        let len = record_len(op).unwrap();
        out.push(stage << 8 | op as u32);
        let mut f = fields.to_vec();
        f.resize(len - 1, 0);
        out.extend(f);
    }

    /// Two controls, one vertex: gather [1, 0], dense 2 -> 3, place into the
    /// output, gather [2, 1, 0].
    fn tiny() -> (Vec<u8>, Vec<u8>) {
        let mut r = Vec::new();
        rec(&mut r, 0, 0x0b, &[0x103]);
        rec(&mut r, 1, 0x3c, &[0x203, 0x103, 2, 2, 1, 2]);
        rec(&mut r, 2, 0x2e, &[0x303, 0x203, (1 << 7) | 2, 1, 2, 3, 2]);
        rec(&mut r, 3, 0x43, &[3, 0x303, 0, 0, 3]);
        rec(&mut r, 4, 0x3c, &[5, 3, (2 << 7) | 2, 2, 1, 3]);
        r.push(0xff00);
        let mut sys = vec![0u8; 0x60];
        sys[0x10..0x14].copy_from_slice(&((r.len() * 4) as u32).to_le_bytes());
        sys[0x18..0x20].copy_from_slice(&0x5000_0060u64.to_le_bytes());
        sys[0x48..0x50].copy_from_slice(&0x1234u64.to_le_bytes());
        sys.extend(r.iter().flat_map(|x| x.to_le_bytes()));
        let mut c = vec![0u8; 48];
        for (k, v) in [1u32, 0].iter().enumerate() {
            c[4 * k..4 * k + 4].copy_from_slice(&v.to_le_bytes());
        }
        // fp16 1..6, K x N row-major
        for (k, h) in [0x3c00u16, 0x4000, 0x4200, 0x4400, 0x4500, 0x4600].iter().enumerate() {
            c[16 + 2 * k..18 + 2 * k].copy_from_slice(&h.to_le_bytes());
        }
        for (k, v) in [2u32, 1, 0].iter().enumerate() {
            c[32 + 4 * k..36 + 4 * k].copy_from_slice(&v.to_le_bytes());
        }
        (sys, c)
    }

    #[test]
    fn parses_and_evaluates_a_tiny_graph() {
        let (sys, c) = tiny();
        let g = parse(&sys).unwrap();
        assert_eq!((g.id, g.inputs, g.outputs, g.ops.len()), (0x1234, 2, 3, 5));
        let b = basis(&g, &c).unwrap();
        // control 0 -> gathered [0, 1] -> row 1 [4, 5, 6] -> reversed
        assert_eq!(b, vec![6.0, 5.0, 4.0, 3.0, 2.0, 1.0]);
    }

    #[test]
    fn transpose_matmul_reads_n_by_k() {
        let (mut sys, mut c) = tiny();
        // the dense record becomes Transpose_MatMul with the weights stored
        // N x K: the same map
        let at = 0x60 + 4 * 10 + 4 * 25;
        sys[at] = 0x2f;
        let rest: Vec<u8> = sys[at + 4 * 24..].to_vec();
        sys.truncate(at + 4 * 24);
        sys.extend([0u8; 4 * 15]);
        sys.extend(rest);
        let n = u32::from_le_bytes(sys[0x10..0x14].try_into().unwrap()) + 60;
        sys[0x10..0x14].copy_from_slice(&n.to_le_bytes());
        for (k, h) in [0x3c00u16, 0x4400, 0x4000, 0x4500, 0x4200, 0x4600].iter().enumerate() {
            c[16 + 2 * k..18 + 2 * k].copy_from_slice(&h.to_le_bytes());
        }
        let g = parse(&sys).unwrap();
        assert_eq!(basis(&g, &c).unwrap(), vec![6.0, 5.0, 4.0, 3.0, 2.0, 1.0]);
    }

    #[test]
    fn other_graphs_are_not_face_graphs() {
        let (mut sys, _) = tiny();
        sys[0x60 + 4 * 10] = 0x4e; // an op this reader does not run
        assert!(parse(&sys).is_none());
        assert!(parse(&[0u8; 0x40]).is_none());
    }

    #[test]
    fn vertex_map_reads_the_high_halves() {
        let mut s = Vec::new();
        for i in 0..70u16 {
            let gv: u16 = if i == 3 { 0xffff } else { i + 1 };
            for (lo, hi) in [(7u16, gv), (0, i + 1), (0, 70 - i), (0, 0xb600)] {
                s.extend(lo.to_le_bytes());
                s.extend(hi.to_le_bytes());
            }
        }
        let m = vertex_map(&s, 70).unwrap();
        assert_eq!((m[0], m[3], m[69]), (Some(0), None, Some(69)));
        // blend indices (no tag in the fourth word) are refused
        assert!(vertex_map(&vec![0u8; 16 * 70], 70).is_none());
    }

    #[test]
    fn morph_targets_pick_the_network_that_covers_the_mesh() {
        let small = std::sync::Arc::new(FaceNet { graph: 1, inputs: 1, outputs: 3, basis: vec![1.0, 0.0, 0.0] });
        let big = std::sync::Arc::new(FaceNet { graph: 2, inputs: 2, outputs: 6, basis: vec![0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1e-6] });
        let nets = vec![small, big];
        let t = morph_targets(&nets, &[Some(0), None]);
        assert_eq!(t, vec![(control_name(0), vec![[1.0, 0.0, 0.0], [0.0; 3]])]);
        // vertex 1 needs the larger network; control 1 moves under 0.01 mm: dropped
        let t = morph_targets(&nets, &[Some(1), Some(0)]);
        assert_eq!(t, vec![(control_name(0), vec![[0.0, 2.0, 0.0], [0.0; 3]])]);
    }

    fn s8(w: [u32; 4]) -> Vec<u8> {
        w.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn surface_map_reads_three_points() {
        let mut s = s8([3 << 16 | 7, 1 << 16, 2 << 16 | 9, 0xb210 << 16 | 5]);
        s.extend(s8([0, 0, 0, 40]));
        s.extend(s8([1 << 16, 1 << 16, 1 << 16, 0xb220 << 16]));
        assert_eq!(surface_map(&s, 3).unwrap(), vec![Some([2, 0, 1]), None, Some([0, 0, 0])]);
        // nothing tagged, or a head's mapping stream: not a garment surface map
        assert!(surface_map(&s8([0, 0, 0, 40]), 1).is_none());
        assert!(surface_map(&s8([1 << 16, 0, 0, 0xb600 << 16]), 1).is_none());
    }

    #[test]
    fn pose_targets_average_the_three_points() {
        // one input, two points: point 0 moves +x by 3, point 1 not at all
        let net = std::sync::Arc::new(FaceNet { graph: 1, inputs: 1, outputs: 6, basis: vec![3.0, 0.0, 0.0, 0.0, 0.0, 0.0] });
        let t = pose_targets(&[net.clone()], &[Some([0, 0, 1]), None]);
        assert_eq!(t, vec![(pose_name(0), vec![[2.0, 0.0, 0.0], [0.0; 3]])]);
        // a point past the network: no targets
        assert!(pose_targets(&[net], &[Some([0, 0, 2])]).is_empty());
    }
}
