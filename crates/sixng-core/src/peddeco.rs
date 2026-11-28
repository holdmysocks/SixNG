//! Ped decorations: tattoos, makeup, body marks and garment prints, and how
//! they are baked into a part's colour texture.
//!
//! The catalogue is one PSO file of data.rpf. Its records (struct
//! `40e79f52`, 224 bytes) are one decoration each:
//! - `Name` (+4): the u32 hash a wardrobe item's tag `d9f1087b`
//!   (decoration presets) lists;
//! - `Category` (+108): the wardrobe item category of the part it goes on
//!   (`3817131d` body upper, `44925bfd` body lower, `378ad10c` head, and
//!   garment categories for prints);
//! - layers (`ef888d7e`, +160, array of struct `77f0121d`, 320 bytes).
//!
//! A layer holds:
//! - up to five decal textures (`5adaff50`, u64), which live in the stores
//!   `data/ttdb/peds/decorations_*`;
//! - three ink colours (`0d23ee2d`, ARGB);
//! - a clip quad (`207eec03`, four edges `x0 y0 x1 y1` in the part's UV);
//! - flags, opacity (`c88b9e6d`), saturation, a mode (0..5);
//! - the UV transform (`8b424621`, a 3x3 matrix stored as three vec4 rows).
//!
//! Placement, from the game's bake shader (`PS_BakeDecorations`) and checked
//! on a full-body decal: the part's UV `(u, v)`, as a row vector, times the
//! matrix gives the decal UV. With `w = m02 u + m12 v + m22`, the decal UV is
//! `((m00 u + m10 v + m20) / w, (m01 u + m11 v + m21) / w)`. A part pixel
//! takes the layer only inside every edge of the clip quad: the shader drops
//! it when `(u - x0)(y1 - y0) - (v - y0)(x1 - x0) < 0` for any edge.
//!
//! The record has a matrix too (`6e8249bf`, +32, same layout). The game's
//! bake mesh shader (`MSNC_VS_BakeDecorations`) rasterises the part in the
//! record's own UV, the part UV times that matrix, and hands that UV to the
//! pixel shader, so a layer's clip quad, fold and matrix all work in it;
//! outside `0..1` the record draws nothing. Most records' matrices are the
//! identity or a uniform scale and offset that zooms into one region (arm
//! tattoos into the arm), which is why some layer matrices alone put
//! their decal somewhere else on the body.
//!
//! Colour: a decal is either full-colour art or a mask. In a mask, alpha
//! is the line art, green a flat 0.5 and red / blue the regions of a second
//! and third ink. Each ink word is hue, saturation, value and alpha bytes
//! (`0xAAVVSSHH`); the game's CPU packer turns it into three linear colour
//! stops ([`ink_stops`]) and the shader picks along them by the decal's
//! green ([`shade`]), so a mask's flat 0.5 gives the ink itself. An ink
//! with alpha 0 is off, and with ink 0 off the decal keeps its own colour.
//! The result goes on the skin by the blend the flag bits select
//! ([`Layer::blend`]), in linear light.
//!
//! Head makeup ([`Layer::is_makeup`]): before packing, the game moves such
//! an ink toward the ped's skin tone shifted in CIELAB by the layer's `p3`
//! row ([`makeup_ink`]). The skin tone is a wardrobe tag (`0d6de047`) that
//! names a record of the skin tone table in data.rpf (struct `27a7a4ef`,
//! a linear colour).
//!
//! Upper body space: body-upper decorations are not placed in the part's
//! UV0. That UV is mirrored (both arms share one UV island), so it cannot
//! hold a tattoo on one arm only. They are placed in the part's second UV
//! set, which gives each side its own islands, with `u` doubled: a space
//! two units wide ([`Space`]). The evidence is the data: a record's
//! mirrored pair of sleeves has one layer at `u` 0.37..0.60 and its mirror
//! at 1.41..1.64; a whole-sheet record's picture follows the second set's
//! island outlines (Viper's chest piece, portrait and lettering each sit
//! inside one island), and sampled at both sides of every seam of the mesh
//! it changes four to five times less than in the fourth set (mean jump
//! 0.022 against 0.097, linear RGB); over the whole catalogue, 81 % of a
//! body-upper layer's decal centre lands on the upper body in the second
//! set (78 % in the fourth, 59 % in UV0). The fourth set, UV0 at half
//! width by side, only marks a mesh as an upper body ([`is_split_layout`]).
//! The upper body is therefore baked into a texture laid out in its second
//! UV set ([`uv_layout`]), which the part's material then reads through
//! that set. The legs are not mirrored and keep their UV0.
//!
//! Left out: mask decals on a layer with no ink, which the game colours at
//! run time.

use crate::pso::Pso;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// The decoration record struct (low 32 bits of the PSO struct hash).
pub const RECORD: u32 = 0x40e7_9f52;
/// The layer struct.
pub const LAYER: u32 = 0x77f0_121d;
/// Wardrobe tag key whose items are decoration names (`decorationpresets`).
pub const TAG: u32 = 0xd9f1_087b;
/// Decoration categories that go on skin, with the outfit slot that takes
/// them when no part has that item category.
pub const SKIN_CATEGORIES: [(u32, &str); 3] = [(0x3817_131d, "body upper"), (0x4492_5bfd, "body lower"), (0x378a_d10c, "head")];

const VERSION: u32 = 5;
/// Changes when the bake rule changes, so baked textures are made again.
pub const BAKE_RULE: u32 = 11;
/// The skin tone record struct (`data.rpf`): a name, a linear colour.
pub const SKIN_TONE: u32 = 0x27a7_a4ef;
/// Wardrobe tag key whose item names the skin tone (a skin-tone item, or
/// the kit).
pub const SKIN_TONE_TAG: u32 = 0x0d6d_e047;
/// The skin tone the game uses when a name is not in the table.
pub const DEFAULT_SKIN_TONE: u32 = 0xac23_15da;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// Decal textures (non-zero ones of the five, in order).
    pub textures: Vec<u64>,
    /// Ink words, `0xAAVVSSHH` (HSV and alpha; see [`ink_stops`]).
    pub colours: [u32; 3],
    /// Clip quad: four edges `[x0, y0, x1, y1]` in the part's UV
    /// (`f32::MAX` coordinates: no clipping).
    pub clip: [[f32; 4]; 4],
    pub flags: u32,
    pub opacity: f32,
    pub saturation: f32,
    /// 0..5: the game matches it against a field of the record it bakes
    /// into (a target or slot index, not a blend mode).
    pub mode: u32,
    /// Part UV -> decal UV (rows; see the module notes).
    pub uv: [[f32; 3]; 3],
    /// Per ink: four values; a non-zero fourth marks head makeup (see
    /// [`Layer::is_makeup`]).
    #[serde(default)]
    pub p3: [[f32; 4]; 3],
    /// The record's matrix (`6e8249bf`, the same for all its layers): part
    /// UV -> the record's own UV, in which the game bakes the record and
    /// places its layers (see the module notes).
    #[serde(default = "identity")]
    pub zone: [[f32; 3]; 3],
}

fn identity() -> [[f32; 3]; 3] {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decoration {
    pub name: u32,
    pub category: u32,
    pub layers: Vec<Layer>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Catalogue {
    pub version: u32,
    /// The data.rpf entry the records were read from.
    pub source: String,
    pub decorations: Vec<Decoration>,
    /// Skin tones: name, linear RGB (the colour head makeup starts from).
    #[serde(default)]
    pub skin_tones: Vec<(u32, [f32; 3])>,
    #[serde(skip)]
    by_name: HashMap<u32, usize>,
}

fn le32(d: &[u8], o: usize) -> u32 {
    d.get(o..o + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
}
fn le64(d: &[u8], o: usize) -> u64 {
    d.get(o..o + 8).map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap()))
}
fn f32at(d: &[u8], o: usize) -> f32 {
    f32::from_bits(le32(d, o))
}

/// Byte offset of a member of a PSO struct, by its name hash.
fn member(p: &Pso, s: u32, name: u32) -> Option<usize> {
    p.structs.get(&s)?.members.iter().find(|m| m.hash == name).map(|m| m.off as usize)
}

/// An array member (`u32 count`, then at +8 a pool pointer
/// `(offset << 20) | block`): (count, byte offset of the first element).
fn array(p: &Pso, at: usize) -> Option<(usize, usize)> {
    let d = &p.res.sys;
    let n = le32(d, at) as usize;
    let ptr = le64(d, at + 8);
    let (b, off) = ((ptr & 0xf_ffff) as usize, (ptr >> 20) as usize);
    if n == 0 {
        return Some((0, 0));
    }
    let &(_, len, o) = p.blocks.get(b.checked_sub(1)?)?;
    (o != usize::MAX && off <= len as usize).then_some((n, o + off))
}

fn read_layer(p: &Pso, o: usize, m: &impl Fn(u32) -> Option<usize>) -> Option<Layer> {
    let d = &p.res.sys;
    let at = |h: u32| m(h).map(|x| o + x);
    let tex = at(0x5ada_ff50)?;
    let col = at(0x0d23_ee2d)?;
    let clip = at(0x207e_ec03)?;
    let uv = at(0x8b42_4621)?;
    let p3 = at(0x6eea_8588);
    Some(Layer {
        textures: (0..5).map(|i| le64(d, tex + 8 * i)).filter(|&h| h != 0).collect(),
        colours: std::array::from_fn(|i| le32(d, col + 4 * i)),
        clip: std::array::from_fn(|i| std::array::from_fn(|k| f32at(d, clip + 16 * i + 4 * k))),
        flags: at(0x4b62_1302).map_or(0, |x| le32(d, x)),
        opacity: at(0xc88b_9e6d).map_or(1.0, |x| f32at(d, x)),
        saturation: at(0x0d4d_d331).map_or(1.0, |x| f32at(d, x)),
        mode: at(0x8765_2efd).map_or(0, |x| le32(d, x)),
        uv: std::array::from_fn(|i| std::array::from_fn(|k| f32at(d, uv + 16 * i + 4 * k))),
        p3: p3.map_or([[0.0; 4]; 3], |p| std::array::from_fn(|i| std::array::from_fn(|k| f32at(d, p + 16 * i + 4 * k)))),
        zone: identity(),
    })
}

impl Catalogue {
    /// The decorations of a PSO file that holds the record struct.
    pub fn from_pso(p: &Pso, source: &str) -> Option<Catalogue> {
        let rm = |h: u32| member(p, RECORD, h);
        let lm = |h: u32| member(p, LAYER, h);
        let lsize = p.structs.get(&LAYER)?.size as usize;
        let (name, cat, layers) = (rm(0xacec_22be)?, rm(0xb536_67be)?, rm(0xef88_8d7e)?);
        let zone = rm(0x6e82_49bf);
        let d = &p.res.sys;
        let decorations: Vec<Decoration> = p
            .meta()
            .objects(RECORD)
            .into_iter()
            .map(|o| {
                let z = zone.map_or_else(identity, |z| std::array::from_fn(|i| std::array::from_fn(|k| f32at(d, o + z + 16 * i + 4 * k))));
                let ls = array(p, o + layers).map(|(n, b)| (0..n).filter_map(|i| read_layer(p, b + i * lsize, &lm)).map(|l| Layer { zone: z, ..l }).collect()).unwrap_or_default();
                Decoration { name: le32(d, o + name), category: le32(d, o + cat), layers: ls }
            })
            .collect();
        (!decorations.is_empty()).then(|| Catalogue { version: VERSION, source: source.into(), decorations, skin_tones: Vec::new(), by_name: HashMap::new() }.indexed())
    }

    fn indexed(mut self) -> Self {
        self.by_name = self.decorations.iter().enumerate().map(|(i, d)| (d.name, i)).collect();
        self
    }

    pub fn get(&self, name: u32) -> Option<&Decoration> {
        self.by_name.get(&name).map(|&i| &self.decorations[i])
    }

    /// The skin tones of a PSO file that holds the skin tone struct.
    pub fn skin_tones_from_pso(p: &Pso) -> Vec<(u32, [f32; 3])> {
        let m = |h: u32| member(p, SKIN_TONE, h);
        let (Some(name), Some(col)) = (m(crate::meta7a::joaat("name")), m(0xf644_6016)) else { return Vec::new() };
        let d = &p.res.sys;
        p.meta().objects(SKIN_TONE).into_iter().map(|o| (le32(d, o + name), std::array::from_fn(|i| f32at(d, o + col + 4 * i)))).collect()
    }

    /// A skin tone's colour as the game packs it for makeup: the linear
    /// colour to sRGB bytes, `0xAABBGGRR`. A name that is not in the table
    /// takes the game's default tone; None without a table.
    pub fn skin_tone_word(&self, name: u32) -> Option<u32> {
        let c = self.skin_tones.iter().find(|t| t.0 == name).or_else(|| self.skin_tones.iter().find(|t| t.0 == DEFAULT_SKIN_TONE))?.1;
        let b = c.map(|x| (lin_to_srgb(x) * 255.0 + 0.5).clamp(0.0, 255.0) as u32);
        Some(0xff00_0000 | b[2] << 16 | b[1] << 8 | b[0])
    }

    pub fn path(ws: &crate::export::Workspace) -> std::path::PathBuf {
        ws.index_dir().join("ped_decorations.json")
    }

    pub fn load(ws: &crate::export::Workspace) -> Option<Catalogue> {
        let c: Catalogue = serde_json::from_slice(&std::fs::read(Self::path(ws)).ok()?).ok()?;
        (c.version == VERSION).then(|| c.indexed())
    }

    /// From the cache, else from data.rpf (its first PSO that holds the
    /// record struct), cached.
    pub fn load_or_build(ws: &crate::export::Workspace) -> Result<Catalogue, String> {
        if let Some(c) = Self::load(ws) {
            return Ok(c);
        }
        let a = ws.resolve("data").map_err(|e| format!("{e:#}"))?;
        let aix = ws.load_index(&a).map_err(|e| format!("data.rpf index: {e:#}; index data.rpf first"))?;
        let psos = || aix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Pso).filter_map(|e| Some((e, crate::pso::parse(&crate::scan::read_entry(&a.path, e).ok()?)?)));
        let mut c = psos().find_map(|(e, p)| p.structs.contains_key(&RECORD).then(|| Catalogue::from_pso(&p, &e.id)).flatten()).ok_or("no ped decoration catalogue found in data.rpf")?;
        c.skin_tones = psos().find(|(_, p)| p.structs.contains_key(&SKIN_TONE)).map(|(_, p)| Catalogue::skin_tones_from_pso(&p)).unwrap_or_default();
        if let Ok(b) = serde_json::to_vec(&c) {
            let p = Self::path(ws);
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = crate::atomic::write(p, b);
        }
        Ok(c)
    }
}

/// The catalogue of this install (built once per run; None without a work
/// folder or data.rpf index).
pub fn get(game_root: &Path) -> Option<&'static Catalogue> {
    static C: OnceLock<Option<Catalogue>> = OnceLock::new();
    C.get_or_init(|| {
        let work = crate::names::work_dir()?;
        Catalogue::load_or_build(&crate::export::Workspace::new(game_root, work)).ok()
    })
    .as_ref()
}

/// The outfit slot a skin decoration category goes on when no part has that
/// item category.
pub fn skin_slot(category: u32) -> Option<&'static str> {
    SKIN_CATEGORIES.iter().find(|c| c.0 == category).map(|c| c.1)
}

/// The body-upper category: placed in the upper body's space ([`Space`]).
pub fn is_body_upper(category: u32) -> bool {
    skin_slot(category) == Some("body upper")
}

/// Part UV -> decal UV (None where the projective divisor vanishes).
pub fn decal_uv(m: &[[f32; 3]; 3], u: f32, v: f32) -> Option<(f32, f32)> {
    let w = m[0][2] * u + m[1][2] * v + m[2][2];
    if w.abs() < 1e-8 {
        return None;
    }
    Some(((m[0][0] * u + m[1][0] * v + m[2][0]) / w, (m[0][1] * u + m[1][1] * v + m[2][1]) / w))
}

impl Layer {
    /// The part UV the decal transform reads. Flag 0x400 folds U about the
    /// middle (`0.5 - |u - 0.5|`), 0x800 folds V, so one decal lands on both
    /// sides (eye make-up on both eyes). The clip quad uses the unfolded UV.
    pub fn fold(&self, u: f32, v: f32) -> (f32, f32) {
        let f = |x: f32, bit: u32| if self.flags & bit != 0 { 0.5 - (x - 0.5).abs() } else { x };
        (f(u, 0x400), f(v, 0x800))
    }

    /// The record's UV at a part UV: the part UV times the record matrix
    /// ([`Layer::zone`]). `None` outside `0..1`, where the game draws
    /// nothing of the record (its bake covers that square only).
    pub fn zone_uv(&self, u: f32, v: f32) -> Option<(f32, f32)> {
        if self.zone == identity() {
            return Some((u, v));
        }
        let (zu, zv) = decal_uv(&self.zone, u, v)?;
        ((0.0..=1.0).contains(&zu) && (0.0..=1.0).contains(&zv)).then_some((zu, zv))
    }
}

/// Where a baked texture's UV lands in the decoration space its layers are
/// placed in (see the module notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    /// The part's own UV (heads, and parts with no body layout).
    Part,
    /// The upper body, baked into a texture laid out in its second UV set:
    /// placed at `(2u, v)`.
    BodyUpper,
}

impl Space {
    pub fn place(self, u: f32, v: f32) -> (f32, f32) {
        match self {
            Space::Part => (u, v),
            Space::BodyUpper => (2.0 * u, v),
        }
    }
}

/// Whether a mesh's fourth UV set is its UV0 at half width, split by side
/// (`u0 / 2` or `0.5 + u0 / 2`, same `v`): the upper body's layout. `uv` is
/// (UV0, UV3) per vertex; true when nine in ten vertices follow it and some
/// of them are on each side.
pub fn is_split_layout(uv: &[([f32; 2], [f32; 2])]) -> bool {
    let (mut fit, mut lo, mut hi) = (0usize, 0usize, 0usize);
    for &(a, b) in uv {
        if (b[1] - a[1]).abs() > 2e-3 {
            continue;
        }
        if (b[0] - a[0] / 2.0).abs() < 2e-3 && a[0] > 0.01 {
            fit += 1;
            lo += 1;
        } else if (b[0] - 0.5 - a[0] / 2.0).abs() < 2e-3 {
            fit += 1;
            hi += 1;
        } else if (b[0] - a[0]).abs() < 2e-3 {
            // a strip both layouts share (the waist)
            fit += 1;
        }
    }
    !uv.is_empty() && fit * 10 >= uv.len() * 9 && lo * 20 >= uv.len() && hi * 20 >= uv.len()
}

/// The upper body's colour texture laid out in another of its UV sets (the
/// second, [`Space::BodyUpper`]): twice the width of `base` (laid out in
/// UV0), each texel the base colour at the UV0 of the surface there.
/// `tris` are the mesh's triangles, each corner (UV0, the layout's UV).
/// A texel no triangle covers (a gutter) takes the UV0 of the nearest
/// covered one, so filtering at an island's edge reads the same skin;
/// with no triangles at all, the half-width rule (`u0 = fract(2 u)`).
pub fn uv_layout(base: &Image, tris: &[[([f32; 2], [f32; 2]); 3]]) -> Image {
    let (w, h) = (base.w as usize * 2, base.h as usize);
    let mut uv0: Vec<[f32; 2]> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let u = (x as f32 + 0.5) / w as f32;
            [(2.0 * u).fract(), (y as f32 + 0.5) / h as f32]
        })
        .collect();
    let mut covered = vec![false; w * h];
    for t in tris {
        let p: [[f32; 2]; 3] = std::array::from_fn(|k| [t[k].1[0] * w as f32, t[k].1[1] * h as f32]);
        let d = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
        if d.abs() < 1e-12 {
            continue;
        }
        let x0 = p.iter().map(|q| q[0]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let x1 = (p.iter().map(|q| q[0]).fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(w);
        let y0 = p.iter().map(|q| q[1]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let y1 = (p.iter().map(|q| q[1]).fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let b1 = ((px - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (py - p[0][1])) / d;
                let b2 = ((p[1][0] - p[0][0]) * (py - p[0][1]) - (px - p[0][0]) * (p[1][1] - p[0][1])) / d;
                let b0 = 1.0 - b1 - b2;
                // texel centres inside the triangle
                if b0 < -1e-4 || b1 < -1e-4 || b2 < -1e-4 {
                    continue;
                }
                let a = [t[0].0, t[1].0, t[2].0];
                uv0[y * w + x] = [b0 * a[0][0] + b1 * a[1][0] + b2 * a[2][0], b0 * a[0][1] + b1 * a[1][1] + b2 * a[2][1]];
                covered[y * w + x] = true;
            }
        }
    }
    // gutters: breadth-first from the covered texels, each taking the UV0
    // of the neighbour it was reached from
    let mut queue: std::collections::VecDeque<usize> = (0..w * h).filter(|&i| covered[i]).collect();
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % w, i / w);
        let near = [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)];
        for j in near.into_iter().flatten() {
            if !covered[j] {
                covered[j] = true;
                uv0[j] = uv0[i];
                queue.push_back(j);
            }
        }
    }
    let mut rgba = vec![0u8; w * h * 4];
    rgba.par_chunks_mut(4).zip(uv0.par_iter()).for_each(|(px, q)| {
        let c = base.sample(q[0], q[1]);
        for i in 0..4 {
            px[i] = (c[i] * 255.0).round() as u8;
        }
    });
    Image { w: w as u32, h: h as u32, rgba, srgb: base.srgb }
}

/// Inside the clip quad (every edge; see the module notes).
pub fn inside_clip(q: &[[f32; 4]; 4], u: f32, v: f32) -> bool {
    q.iter().all(|e| {
        let [x0, y0, x1, y1] = e.map(f64::from);
        (u as f64 - x0) * (y1 - y0) - (v as f64 - y0) * (x1 - x0) >= 0.0
    })
}

fn srgb_to_lin(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
fn lin_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// DXGI formats whose colour channels are sRGB-encoded: a sampler returns
/// them in linear light, which is what the bake shader sees.
pub fn dxgi_is_srgb(f: u32) -> bool {
    matches!(f, 29 | 72 | 75 | 78 | 91 | 93 | 99)
}

/// The hue ramp of the game's HSV code (`h` 0..1): RGB 0..1.
fn hue(h: f32) -> [f32; 3] {
    [(6.0 * h - 3.0).abs() - 1.0, 2.0 - (6.0 * h - 2.0).abs(), 2.0 - (6.0 * h - 4.0).abs()].map(|x| x.clamp(0.0, 1.0))
}

/// An ink word's three colour stops, as the game's CPU packer makes them
/// for the bake shader: linear RGB and alpha, each quantised to a byte and
/// returned as byte / 255. None when the ink is off (alpha byte 0).
///
/// The word is `0xAAVVSSHH`: hue, saturation, value and alpha bytes, each
/// / 255. With `ramp = hue(H)`, the stops in sRGB are
/// - stop 0 (dark): `0.1 V x lerp(1, ramp, S)`, alpha 1;
/// - stop 1 (the ink): `V x lerp(1, ramp, S)`, alpha A;
/// - stop 2 (light): `(0.45 + 0.55 V) x lerp(1, ramp, 0.55 S)`, alpha 1;
///
/// and each channel goes to linear light before it is quantised
/// (`trunc(x 255 + 0.5)`).
pub fn ink_stops(w: u32) -> Option<[[f32; 4]; 3]> {
    if w >> 24 == 0 {
        return None;
    }
    let [h, s, v, a] = w.to_le_bytes().map(|b| b as f32 / 255.0);
    let ramp = hue(h);
    let stop = |val: f32, sat: f32, alpha: f32| -> [f32; 4] {
        let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5).trunc() / 255.0;
        let c = ramp.map(|r| q(srgb_to_lin(val * (1.0 + (r - 1.0) * sat))));
        [c[0], c[1], c[2], alpha]
    };
    Some([stop(0.1 * v, s, 1.0), stop(v, s, a), stop(0.45 + 0.55 * v, 0.55 * s, 1.0)])
}

/// The game's HSV to RGB (all 0..1).
fn hsv_to_rgb([h, s, v]: [f32; 3]) -> [f32; 3] {
    if s == 0.0 {
        return [v; 3];
    }
    let h6 = (h - h.floor()) * 6.0;
    let i = h6 as i32;
    let f = h6 - h6.floor();
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// The game's RGB to HSV.
fn rgb_to_hsv([r, g, b]: [f32; 3]) -> [f32; 3] {
    let v = r.max(g).max(b);
    let d = v - r.min(g).min(b);
    if d == 0.0 {
        return [0.0, 0.0, v];
    }
    let h = if r == v {
        (g - b) / d
    } else if g == v {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [if h < 0.0 { h + 6.0 } else { h } / 6.0, d / v, v]
}

/// sRGB (D65) to XYZ, as the game's makeup code has it.
const XYZ: [[f32; 3]; 3] = [[0.412_456_4, 0.357_576_1, 0.180_437_5], [0.212_672_9, 0.715_152_2, 0.072_175], [0.019_333_9, 0.119_192, 0.950_304_1]];

/// The reference white the game's Lab conversion divides by: the D65 entry
/// of its illuminant table (CIE 1931 2°), copied at start-up.
fn white() -> [f32; 3] {
    [0.950_47, 1.0, 1.088_83]
}

fn signed(f: fn(f32) -> f32, c: f32) -> f32 {
    if c < 0.0 { -f(-c) } else { f(c) }
}

/// sRGB (0..1, may leave the range) to CIELAB.
fn srgb_to_lab(c: [f32; 3]) -> [f32; 3] {
    let l = c.map(|x| signed(srgb_to_lin, x));
    let w = white();
    let xyz: [f32; 3] = std::array::from_fn(|i| (XYZ[i][0] * l[0] + XYZ[i][1] * l[1] + XYZ[i][2] * l[2]) / w[i]);
    let f = |t: f32| if t > 0.008_856_452 { t.powf(1.0 / 3.0) } else { 7.787_037 * t + 16.0 / 116.0 };
    let [fx, fy, fz] = xyz.map(f);
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIELAB to sRGB (not clamped).
fn lab_to_srgb([l, a, b]: [f32; 3]) -> [f32; 3] {
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (fy + a * 0.002, fy - b * 0.005);
    let finv = |t: f32| if t * t * t > 0.008_856_452 { t * t * t } else { (t - 16.0 / 116.0) / 7.787_037 };
    let w = white();
    let y = if l > 8.0 { fy * fy * fy } else { l * 0.001_107_056 };
    let xyz = [finv(fx) * w[0], y * w[1], finv(fz) * w[2]];
    // the inverse of XYZ
    let m = XYZ;
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let inv = [
        [m[1][1] * m[2][2] - m[1][2] * m[2][1], m[0][2] * m[2][1] - m[0][1] * m[2][2], m[0][1] * m[1][2] - m[0][2] * m[1][1]],
        [m[1][2] * m[2][0] - m[1][0] * m[2][2], m[0][0] * m[2][2] - m[0][2] * m[2][0], m[0][2] * m[1][0] - m[0][0] * m[1][2]],
        [m[1][0] * m[2][1] - m[1][1] * m[2][0], m[0][1] * m[2][0] - m[0][0] * m[2][1], m[0][0] * m[1][1] - m[0][1] * m[1][0]],
    ];
    let lin: [f32; 3] = std::array::from_fn(|i| (inv[i][0] * xyz[0] + inv[i][1] * xyz[1] + inv[i][2] * xyz[2]) / det);
    lin.map(|x| signed(|c| if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }, x))
}

/// Head makeup: the game moves an ink toward a colour made from the skin
/// tone before it packs the stops. With `p = (dL, da, db, w)` (the ink's
/// `p3` row): the target is `base` (an sRGB word `0xAABBGGRR`, the ped's
/// skin tone) moved by `(dL, da, db)` in CIELAB; the ink (HSV) goes to RGB,
/// `w` of the way to the target, back to HSV, and is packed again with its
/// own alpha. `w = 0` leaves the ink as it is.
pub fn makeup_ink(ink: u32, p: [f32; 4], base: u32) -> u32 {
    if p[3] == 0.0 {
        return ink;
    }
    let byte = |w: u32, i: u32| (w >> (8 * i) & 0xff) as f32 / 255.0;
    let rgb = hsv_to_rgb([byte(ink, 0), byte(ink, 1), byte(ink, 2)]);
    let lab = srgb_to_lab([byte(base, 0), byte(base, 1), byte(base, 2)]);
    let target = lab_to_srgb([lab[0] + p[0], lab[1] + p[1], lab[2] + p[2]]);
    let hsv = rgb_to_hsv(std::array::from_fn(|i| rgb[i] + (target[i] - rgb[i]) * p[3]));
    let q = |x: f32| ((x * 255.0 + 0.5) as i32).clamp(0, 255) as u32;
    (ink & 0xff00_0000) | q(hsv[2]) << 16 | q(hsv[1]) << 8 | q(hsv[0])
}

/// The colour of a set of stops at gradient position `g`: stop 0 to 1 over
/// the first half, 1 to 2 over the second.
fn gradient(s: &[[f32; 4]; 3], g: f32) -> [f32; 3] {
    let (a, b, t) = if g < 0.5 { (s[0], s[1], 2.0 * g) } else { (s[1], s[2], 2.0 * g - 1.0) };
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

impl Layer {
    /// The layer sets an ink: an ink word with a non-zero alpha byte.
    pub fn uses_ink(&self) -> bool {
        self.colours.iter().any(|c| c >> 24 != 0)
    }

    /// Head makeup: an ink whose `p3` row has a non-zero fourth value. The
    /// game then moves that ink toward another colour (a palette lookup in
    /// another colour space) that is not decoded, so such layers are not
    /// baked.
    pub fn is_makeup(&self) -> bool {
        (0..3).any(|i| self.colours[i] >> 24 != 0 && self.p3[i][3] != 0.0)
    }

    /// The layer with its makeup inks moved toward the skin tone
    /// ([`makeup_ink`]; `base` is the tone's sRGB word).
    pub fn with_makeup(&self, base: u32) -> Layer {
        let mut l = self.clone();
        for i in 0..3 {
            if l.colours[i] >> 24 != 0 {
                l.colours[i] = makeup_ink(l.colours[i], l.p3[i], base);
            }
        }
        l
    }

    /// The blend mode, from the flag bits the shader tests.
    pub fn blend(&self) -> Blend {
        if self.flags & 0x0400_0000 != 0 {
            Blend::Multiply
        } else if self.flags & 0x0800_0000 != 0 {
            Blend::Screen
        } else if self.flags & 0x1000_0000 != 0 {
            Blend::Overlay
        } else {
            Blend::Normal
        }
    }
}

/// How a layer's colour combines with the texture under it (linear light).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    Normal,
    Multiply,
    Screen,
    Overlay,
}

impl Blend {
    pub fn apply(self, base: f32, src: f32) -> f32 {
        match self {
            Blend::Normal => src,
            Blend::Multiply => base * src,
            Blend::Screen => 1.0 - (1.0 - base) * (1.0 - src),
            Blend::Overlay => {
                if base < 0.5 {
                    2.0 * base * src
                } else {
                    1.0 - 2.0 * (1.0 - base) * (1.0 - src)
                }
            }
        }
    }
}

/// Saturation as the game's bake shader applies it (skipped at 1.0): the
/// colour to HSV, the saturation scaled, back to RGB.
pub fn saturate(c: [f32; 3], s: f32) -> [f32; 3] {
    if s == 1.0 {
        return c;
    }
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    let d = mx - mn;
    // hue in 0..1 (the shader's 6 x delta form)
    let h = if d <= 0.0 {
        0.0
    } else if mx == c[0] {
        ((c[1] - c[2]) / (6.0 * d)).rem_euclid(1.0)
    } else if mx == c[1] {
        (c[2] - c[0]) / (6.0 * d) + 1.0 / 3.0
    } else {
        (c[0] - c[1]) / (6.0 * d) + 2.0 / 3.0
    };
    let sat = d / (mx + 1e-10) * s;
    hue(h).map(|x| ((x - 1.0) * sat + 1.0) * mx)
}

/// A layer's colour and coverage at one decal sample `t` (RGBA 0..1, as the
/// shader samples it: colour in linear light for an sRGB decal). Returns
/// (linear colour, coverage).
///
/// Ink 0 colours the whole decal: its gradient at the decal's green, times
/// ink 0's alpha; with ink 0 off, the decal's own colour. Ink 1 then covers
/// that by `alpha(ink 1) x red`, and ink 2 by `alpha(ink 2) x blue`, each
/// with its own gradient at green. Saturation follows; coverage is the
/// decal's alpha x the layer's opacity.
pub fn shade(l: &Layer, t: [f32; 4]) -> ([f32; 3], f32) {
    let inks = l.colours.map(ink_stops);
    let mut c = match &inks[0] {
        Some(s) => gradient(s, t[1]).map(|x| x * s[1][3]),
        None => [t[0], t[1], t[2]],
    };
    for (ink, ch) in [(&inks[1], t[0]), (&inks[2], t[2])] {
        if let Some(s) = ink {
            let w = s[1][3] * ch;
            let g = gradient(s, t[1]);
            c = std::array::from_fn(|i| g[i] * w + c[i] * (1.0 - w));
        }
    }
    (saturate(c, l.saturation), (t[3] * l.opacity).clamp(0.0, 1.0))
}

/// An RGBA8 image.
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
    /// The colour channels are sRGB-encoded (an sRGB texture format).
    pub srgb: bool,
}

impl Image {
    /// A mask decal: where it covers (alpha over half), red and blue are
    /// about 0 and green is flat, for most texels.
    pub fn is_mask(&self) -> bool {
        let (mut n, mut flat) = (0usize, 0usize);
        for p in self.rgba.chunks_exact(4).filter(|p| p[3] > 127) {
            n += 1;
            if p[0] < 13 && p[2] < 13 && (100..=156).contains(&p[1]) {
                flat += 1;
            }
        }
        n > 0 && flat * 2 > n
    }

    /// Bilinear sample at UV (clamped to the edge texels), RGBA 0..1, as
    /// the stored bytes.
    pub fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        let (w, h) = (self.w as f32, self.h as f32);
        let x = (u * w - 0.5).clamp(0.0, w - 1.0);
        let y = (v * h - 0.5).clamp(0.0, h - 1.0);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let px = |x: u32, y: u32| -> [f32; 4] {
            let o = 4 * (y as usize * self.w as usize + x as usize);
            std::array::from_fn(|i| self.rgba[o + i] as f32 / 255.0)
        };
        let (a, b, c, d) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));
        std::array::from_fn(|i| (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + d[i] * fx) * fy)
    }

    /// A sample as the shader sees it: colour in linear light for an sRGB
    /// image (the sampler decodes it), the stored values otherwise.
    pub fn shader_sample(&self, u: f32, v: f32) -> [f32; 4] {
        let t = self.sample(u, v);
        if self.srgb { [srgb_to_lin(t[0]), srgb_to_lin(t[1]), srgb_to_lin(t[2]), t[3]] } else { t }
    }
}

/// What [`bake`] left out.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Skipped {
    /// Layers whose decal texture is in no store.
    pub missing: usize,
    /// Head makeup layers ([`Layer::is_makeup`]) baked with no skin tone
    /// to start from.
    pub makeup: usize,
    /// Mask decals ([`Image::is_mask`]) on a layer with no ink that is not
    /// itself a mask ([`MASK`]): how the game colours them is not decoded.
    pub uncoloured_masks: usize,
}

/// Bake layers, in order, into a part's colour texture (sRGB RGBA8; its
/// alpha is kept). `decal` gives a layer's first decal texture;
/// `skin_tone` is the ped's skin tone word ([`Catalogue::skin_tone_word`]),
/// which head makeup starts from; `space` places the texture's UV in the
/// decoration space ([`Space`]). Makeup with no skin tone, uncoloured
/// masks and layers whose decal is missing are left out and counted.
/// Layer flag: the layer is a mask on the layer before it in its record
/// (`PS_BakeDecorations`: after drawing a layer's coverage, the shader looks
/// at the next layer; if it has this bit, its decal's alpha `a` at its own
/// UV scales the coverage by `1 - (1 - a) x opacity`, and the loop goes on
/// after it, so a mask is never drawn itself).
pub const MASK: u32 = 0x4;
/// Mask flag: the mask's alpha is inverted (`1 - a`): it cuts out.
pub const MASK_INVERTED: u32 = 0x8000_0000;

/// A record's layers as the bake shader draws them: each layer with the
/// mask that follows it, mask layers left out. A mask that opens a record
/// has no layer before it and is drawn as a layer.
pub fn masked(layers: &[Layer]) -> Vec<(&Layer, Option<&Layer>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < layers.len() {
        let mask = layers.get(i + 1).filter(|m| m.masks_previous());
        out.push((&layers[i], mask));
        i += 1 + mask.is_some() as usize;
    }
    out
}

impl Layer {
    /// A mask on the layer before it ([`MASK`]).
    pub fn masks_previous(&self) -> bool {
        self.flags & MASK != 0
    }

    /// How much of the masked layer shows at record-space UV `(u, v)`:
    /// `1 - (1 - a) x opacity`, `a` the mask decal's alpha there (clamped
    /// to its edge texels; inverted with [`MASK_INVERTED`]).
    pub fn mask_factor(&self, t: &Image, u: f32, v: f32) -> f32 {
        let Some((du, dv)) = decal_uv(&self.uv, u, v) else { return 1.0 };
        let mut a = t.shader_sample(du.clamp(0.0, 1.0), dv.clamp(0.0, 1.0))[3];
        if self.flags & MASK_INVERTED != 0 {
            a = 1.0 - a;
        }
        1.0 - (1.0 - a) * self.opacity
    }

    /// A whole sheet: neither the layer nor its record moves it, so it
    /// covers the part's texture as laid out. A character's own tattoo
    /// sheet on the upper body is one; it spans both sides of the second UV
    /// set (its edges meet across the mesh's seams there, and not when it
    /// is read through UV0, the fourth set or on one side).
    pub fn is_sheet(&self) -> bool {
        self.uv == identity() && self.zone == identity()
    }
}

/// Region flag (shader record +0x50): a record with a layer that has it
/// draws every layer limited to the part's mesh coverage, from a mask the
/// bake renders first by drawing the part's own mesh into it. The bake
/// mesh shader (`MSNC_VS_BakeDecorations`) sets its TEXCOORD1.z to 1 at a
/// vertex whose four mask texels `(x-1, y-1), (x-1, y), (x, y+1),
/// (x+1, y+1)` all lie on the mesh, else 0; the pixel shader starts a
/// layer's coverage at `min(opacity, z)`.
///
/// Left out: flag 0x40, which (in a record without 0x2) has the shader take
/// z from a vertex colour channel picked by the layer's group. Its result
/// on the characters checked is doubtful, so it is not applied.
pub const REGION_MASK: u32 = 0x2;

/// Whether a record's layers are limited to the mesh ([`REGION_MASK`]).
pub fn has_region_mask(layers: &[Layer]) -> bool {
    layers.iter().any(|l| l.flags & REGION_MASK != 0)
}

/// A triangle corner of the part being baked: where it lies in the baked
/// texture (0..1, in the texture's own layout).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corner {
    pub at: [f32; 2],
}

/// Texels whose centres a triangle covers, with the barycentric weights.
fn raster(w: usize, h: usize, t: &[Corner; 3], mut f: impl FnMut(usize, [f32; 3])) {
    let p: [[f32; 2]; 3] = std::array::from_fn(|k| [t[k].at[0] * w as f32, t[k].at[1] * h as f32]);
    let d = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
    if d.abs() < 1e-12 {
        return;
    }
    let lo = |i: usize| p.iter().map(|q| q[i]).fold(f32::MAX, f32::min);
    let hi = |i: usize| p.iter().map(|q| q[i]).fold(f32::MIN, f32::max);
    let (x0, x1) = ((lo(0) - 0.5).floor().max(0.0) as usize, ((hi(0) - 0.5).ceil().max(0.0) as usize).min(w.saturating_sub(1)));
    let (y0, y1) = ((lo(1) - 0.5).floor().max(0.0) as usize, ((hi(1) - 0.5).ceil().max(0.0) as usize).min(h.saturating_sub(1)));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            let b1 = ((cx - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (cy - p[0][1])) / d;
            let b2 = ((p[1][0] - p[0][0]) * (cy - p[0][1]) - (cx - p[0][0]) * (p[1][1] - p[0][1])) / d;
            let b0 = 1.0 - b1 - b2;
            if b0 >= -1e-5 && b1 >= -1e-5 && b2 >= -1e-5 {
                f(y * w + x, [b0, b1, b2]);
            }
        }
    }
}

/// The per-texel cap ([`REGION_MASK`]) over a `w` x `h` baked texture,
/// 0..255: each vertex's value interpolated over the part's triangles.
/// Texels no triangle covers (the gutters, which the game fills afterwards
/// by dilating) take the value of the nearest covered texel.
pub fn region_cap(w: usize, h: usize, tris: &[[Corner; 3]]) -> Vec<u8> {
    let mut covered = vec![false; w * h];
    for t in tris {
        raster(w, h, t, |i, _| covered[i] = true);
    }
    let on = |x: i64, y: i64| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && covered[y as usize * w + x as usize];
    let value = |c: &Corner| -> f32 {
        let (x, y) = ((c.at[0] * w as f32).floor() as i64, (c.at[1] * h as f32).floor() as i64);
        (on(x - 1, y - 1) && on(x - 1, y) && on(x, y + 1) && on(x + 1, y + 1)) as u8 as f32
    };
    let mut cap = vec![0.0f32; w * h];
    let mut hit = vec![false; w * h];
    for t in tris {
        let z = [value(&t[0]), value(&t[1]), value(&t[2])];
        raster(w, h, t, |i, b| {
            cap[i] = (b[0] * z[0] + b[1] * z[1] + b[2] * z[2]).clamp(0.0, 1.0);
            hit[i] = true;
        });
    }
    // gutters: grow the covered values outwards, then 1 where none reached
    for _ in 0..16 {
        let (was, prev) = (hit.clone(), cap.clone());
        let mut grew = false;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if was[i] {
                    continue;
                }
                let (mut sum, mut n) = (0.0, 0);
                for (a, b) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                    if a < w && b < h && was[b * w + a] {
                        sum += prev[b * w + a];
                        n += 1;
                    }
                }
                if n > 0 {
                    cap[i] = sum / n as f32;
                    hit[i] = true;
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    cap.iter().zip(&hit).map(|(&c, &h)| if h { (c * 255.0).round() as u8 } else { 255 }).collect()
}

pub fn bake(skin: &mut Image, layers: &[(&Layer, Option<&Layer>)], decal: &dyn Fn(u64) -> Option<std::sync::Arc<Image>>, skin_tone: Option<u32>, space: Space) -> Skipped {
    bake_capped(skin, layers, &[], decal, skin_tone, space)
}

/// [`bake`] with a per-texel coverage cap for each layer (`caps[i]` for
/// `layers[i]`, from [`region_cap`] at the skin's size; a missing entry or
/// `None` is no cap).
pub fn bake_capped(skin: &mut Image, layers: &[(&Layer, Option<&Layer>)], caps: &[Option<&[u8]>], decal: &dyn Fn(u64) -> Option<std::sync::Arc<Image>>, skin_tone: Option<u32>, space: Space) -> Skipped {
    let mut skipped = Skipped::default();
    let (w, h) = (skin.w as usize, skin.h as usize);
    for (li, &(l, mask)) in layers.iter().enumerate() {
        let cap = caps.get(li).copied().flatten().filter(|c| c.len() == w * h);
        // a mask whose decal is missing leaves the layer whole
        let mask = mask.and_then(|m| Some((m, m.textures.first().and_then(|&x| decal(x))?)));
        let made;
        let l = if l.is_makeup() {
            let Some(base) = skin_tone else {
                skipped.makeup += 1;
                continue;
            };
            made = l.with_makeup(base);
            &made
        } else {
            l
        };
        let Some(t) = l.textures.first().and_then(|&x| decal(x)) else {
            skipped.missing += 1;
            continue;
        };
        if !l.uses_ink() && t.is_mask() {
            skipped.uncoloured_masks += 1;
            continue;
        }
        let blend = l.blend();
        // a whole sheet (no transform) is drawn in the part's own layout:
        // on the upper body that is its second UV set, both sides
        let space = if l.is_sheet() { Space::Part } else { space };
        skin.rgba.par_chunks_mut(4 * w).enumerate().for_each(|(y, row)| {
            let v = (y as f32 + 0.5) / h as f32;
            for x in 0..w {
                let (su, sv) = space.place((x as f32 + 0.5) / w as f32, v);
                let Some((u, v)) = l.zone_uv(su, sv) else { continue };
                if !inside_clip(&l.clip, u, v) {
                    continue;
                }
                let (fu, fv) = l.fold(u, v);
                let Some((du, dv)) = decal_uv(&l.uv, fu, fv) else { continue };
                // outside the decal: nothing (its edges are clear in every
                // decal checked, so this matches clamping too)
                if !(0.0..=1.0).contains(&du) || !(0.0..=1.0).contains(&dv) {
                    continue;
                }
                let (c, mut k) = shade(l, t.shader_sample(du, dv));
                if let Some((m, mt)) = &mask {
                    k *= m.mask_factor(mt, u, v);
                }
                // the shader starts the coverage at min(opacity, z), not
                // at the opacity
                if let Some(cap) = cap {
                    let z = cap[y * w + x] as f32 / 255.0;
                    if l.opacity > 0.0 && z < l.opacity {
                        k *= z / l.opacity;
                    }
                }
                if k <= 0.0 {
                    continue;
                }
                let p = &mut row[4 * x..4 * x + 3];
                for i in 0..3 {
                    let s = srgb_to_lin(p[i] as f32 / 255.0);
                    let d = blend.apply(s, c[i].clamp(0.0, 1.0));
                    p[i] = (lin_to_srgb(s + (d - s) * k) * 255.0).round() as u8;
                }
            }
        });
    }
    skipped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer() -> Layer {
        let none = [0.0, f32::MAX, f32::MAX, f32::MAX];
        Layer { textures: vec![1], colours: [0, 0, 0], clip: [none; 4], flags: 0, opacity: 1.0, saturation: 1.0, mode: 0, uv: identity(), p3: [[0.0; 4]; 3], zone: identity() }
    }

    fn corner(u: f32, v: f32) -> Corner {
        Corner { at: [u, v] }
    }

    #[test]
    fn the_region_flag_marks_the_record() {
        let mut a = layer();
        let mut b = layer();
        b.flags = 0x40;
        assert!(!has_region_mask(&[a.clone(), b.clone()]));
        a.flags = REGION_MASK;
        assert!(has_region_mask(&[a, b]));
    }

    #[test]
    fn a_mask_cap_is_one_where_the_mesh_covers_the_vertex() {
        let full = [corner(0.0, 0.0), corner(1.0, 0.0), corner(0.0, 1.0)];
        let other = [corner(1.0, 0.0), corner(1.0, 1.0), corner(0.0, 1.0)];
        // the whole square covered: the small triangle's vertices see four
        // covered texels each (texel (3, 3) lies in it)
        let small = [corner(0.4, 0.4), corner(0.6, 0.4), corner(0.4, 0.6)];
        let c = region_cap(8, 8, &[full, other, small]);
        assert_eq!(c[3 * 8 + 3], 255);
        // a vertex at the texture's corner has texels off the texture: 0
        let c = region_cap(8, 8, &[full]);
        assert_eq!(c[0], 0);
    }

    #[test]
    fn a_cap_lowers_the_opacity_only_below_it() {
        let mut l = layer();
        l.opacity = 0.5;
        let d = std::sync::Arc::new(Image { w: 1, h: 1, rgba: vec![0, 0, 0, 255], srgb: true });
        let base = || Image { w: 2, h: 1, rgba: vec![255; 8], srgb: true };
        let cap = vec![255u8, 64];
        let mut skin = base();
        bake_capped(&mut skin, &[(&l, None)], &[Some(&cap)], &|_| Some(d.clone()), None, Space::Part);
        let mut plain = base();
        bake(&mut plain, &[(&l, None)], &|_| Some(d.clone()), None, Space::Part);
        // cap 1 >= opacity 0.5: as without a cap; cap 0.25 < 0.5: half the coverage
        assert_eq!(skin.rgba[..4], plain.rgba[..4]);
        assert!(skin.rgba[4] > plain.rgba[4]);
    }

    #[test]
    fn uv_is_a_row_vector_times_the_matrix() {
        // scale 2 and move by (-0.5, 0.25)
        let m = [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [-0.5, 0.25, 1.0]];
        assert_eq!(decal_uv(&m, 0.25, 0.5), Some((0.0, 1.25)));
        // the off-diagonal terms: u feeds v through m01, v feeds u through m10
        let m = [[1.0, 0.5, 0.0], [0.25, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let (du, dv) = decal_uv(&m, 0.4, 0.8).unwrap();
        assert!((du - 0.6).abs() < 1e-6 && (dv - 1.0).abs() < 1e-6);
        // projective: divided by w = m02 u + m12 v + m22
        let m = [[1.0, 0.0, 1.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert_eq!(decal_uv(&m, 1.0, 0.5), Some((0.5, 0.25)));
    }

    #[test]
    fn record_matrix_maps_into_its_square_first() {
        let mut l = layer();
        assert_eq!(l.zone_uv(1.5, -0.25), Some((1.5, -0.25)));
        // scale 4 in u, 2 in v, then move u by -0.25: part u 0.0625..0.3125
        // is the record's 0..1
        l.zone = [[4.0, 0.0, 0.0], [0.0, 2.0, 0.0], [-0.25, 0.0, 1.0]];
        assert_eq!(l.zone_uv(0.125, 0.25), Some((0.25, 0.5)));
        assert_eq!(l.zone_uv(0.5, 0.25), None);
        assert_eq!(l.zone_uv(0.125, 0.75), None);
    }

    #[test]
    fn fold_flags_mirror_about_the_middle() {
        let mut l = layer();
        assert_eq!(l.fold(0.8, 0.9), (0.8, 0.9));
        l.flags = 0x400;
        let (u, v) = l.fold(0.8, 0.9);
        assert!((u - 0.2).abs() < 1e-6 && v == 0.9);
        assert_eq!(l.fold(0.3, 0.9), (0.3, 0.9));
        l.flags = 0x800;
        let (u, v) = l.fold(0.8, 0.9);
        assert!(u == 0.8 && (v - 0.1).abs() < 1e-6);
    }

    #[test]
    fn clip_quad_edges() {
        let l = layer();
        // FLT_MAX edges (the common "no clip") keep everything
        assert!(inside_clip(&l.clip, 0.3, 0.9));
        // a box u 0.0128..0.98, v 0.127..0.738 from the data
        let q = [[0.98056, 0.12671, 0.01285, 0.12671], [0.9958, 0.73771, 0.98056, 0.12671], [0.32992, 0.73771, 0.6391, 0.72856], [0.01285, 0.12671, 0.01285, 1.09442]];
        assert!(inside_clip(&q, 0.5, 0.5));
        assert!(!inside_clip(&q, 0.5, 0.05));
        assert!(!inside_clip(&q, 0.5, 0.9));
        assert!(!inside_clip(&q, 0.005, 0.5));
        assert!(!inside_clip(&q, 0.999, 0.5));
        // the degenerate edges (0,1)->(1,1) keep everything up to v = 1
        assert!(inside_clip(&[[0.0, 1.0, 1.0, 1.0]; 4], 0.7, 0.2));
    }

    #[test]
    fn full_colour_layers_keep_the_decal_colour() {
        let mut l = layer();
        l.opacity = 0.5;
        let (c, k) = shade(&l, [1.0, 0.0, 0.5, 0.8]);
        assert_eq!(c, [1.0, 0.0, 0.5]);
        assert_eq!(k, 0.4);
        // an ink word with alpha 0 is no ink
        l.colours = [0x0084_aa6b, 0, 0];
        assert!(!l.uses_ink());
        assert_eq!(shade(&l, [1.0, 0.0, 0.5, 0.8]).0, [1.0, 0.0, 0.5]);
        l.colours = [0xff43_8982, 0, 0];
        assert!(l.uses_ink());
    }

    fn close(a: [f32; 3], b: [f32; 3], e: f32) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < e)
    }

    #[test]
    fn ink_words_are_hsv() {
        // white (V 1, S 0): stop 1 white, stop 0 at a tenth of the value,
        // stop 2 white
        let s = ink_stops(0xffff_0000).unwrap();
        assert_eq!(s[1], [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(s[2], [1.0, 1.0, 1.0, 1.0]);
        // 0.1 in sRGB is 0.0100 linear: byte 3
        assert_eq!(s[0], [3.0 / 255.0, 3.0 / 255.0, 3.0 / 255.0, 1.0]);
        // alpha 0: off
        assert_eq!(ink_stops(0x00ff_0000), None);
        // the common body ink: dark teal, sRGB about (0.12, 0.25, 0.26)
        let s = ink_stops(0xff43_8982).unwrap();
        let srgb = [0, 1, 2].map(|i| lin_to_srgb(s[1][i]));
        assert!(close(srgb, [0.122, 0.255, 0.263], 0.015), "{srgb:?}");
        // pure red at full value and saturation; the light stop is
        // desaturated by 0.55
        let s = ink_stops(0x80ff_ff00).unwrap();
        assert_eq!(s[1], [1.0, 0.0, 0.0, 128.0 / 255.0]);
        let light = [0, 1, 2].map(|i| lin_to_srgb(s[2][i]));
        assert!(close(light, [1.0, 0.45, 0.45], 0.01), "{light:?}");
    }

    #[test]
    fn inks_follow_the_mask_channels() {
        let mut l = layer();
        let red = ink_stops(0xffff_ff00).unwrap();
        let green = ink_stops(0xffff_ff55).unwrap();
        // green 0.5 picks stop 1 of ink 0
        l.colours = [0xffff_ff00, 0, 0];
        assert_eq!(shade(&l, [0.0, 0.5, 0.0, 1.0]).0, [red[1][0], red[1][1], red[1][2]]);
        // green 0 and 1 pick the dark and light stops
        assert_eq!(shade(&l, [0.0, 0.0, 0.0, 1.0]).0, [red[0][0], red[0][1], red[0][2]]);
        assert_eq!(shade(&l, [0.0, 1.0, 0.0, 1.0]).0, [red[2][0], red[2][1], red[2][2]]);
        // ink 1 covers by red, ink 2 by blue
        l.colours = [0xffff_ff00, 0xffff_ff55, 0];
        assert!(close(shade(&l, [1.0, 0.5, 0.0, 1.0]).0, [green[1][0], green[1][1], green[1][2]], 1e-6));
        let half = shade(&l, [0.5, 0.5, 0.0, 1.0]).0;
        assert!(close(half, std::array::from_fn(|i| (red[1][i] + green[1][i]) / 2.0), 1e-6));
        l.colours = [0xffff_ff00, 0, 0xffff_ff55];
        assert!(close(shade(&l, [0.0, 0.5, 1.0, 1.0]).0, [green[1][0], green[1][1], green[1][2]], 1e-6));
        // ink 0 is scaled by its alpha
        l.colours = [0x80ff_ff00, 0, 0];
        assert!((shade(&l, [0.0, 0.5, 0.0, 1.0]).0[0] - 128.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn blend_modes_from_the_flags() {
        let mut l = layer();
        assert_eq!(l.blend(), Blend::Normal);
        l.flags = 0x0400_0000 | 0x0800_0000;
        assert_eq!(l.blend(), Blend::Multiply);
        l.flags = 0x0800_0000;
        assert_eq!(l.blend(), Blend::Screen);
        l.flags = 0x1000_0000;
        assert_eq!(l.blend(), Blend::Overlay);
        assert_eq!(Blend::Multiply.apply(0.5, 0.5), 0.25);
        assert_eq!(Blend::Screen.apply(0.5, 0.5), 0.75);
        assert_eq!(Blend::Overlay.apply(0.25, 0.5), 0.25);
        assert_eq!(Blend::Overlay.apply(0.75, 0.5), 0.75);
        // makeup: an ink on with a non-zero fourth p3 value
        l.colours = [0xff00_ff00, 0, 0];
        assert!(!l.is_makeup());
        l.p3[0][3] = 0.5;
        assert!(l.is_makeup());
        l.colours = [0, 0xff00_ff00, 0];
        assert!(!l.is_makeup());
    }

    #[test]
    fn saturation_scales_hsv_saturation() {
        let c = [0.8, 0.4, 0.2];
        assert_eq!(saturate(c, 1.0), c);
        // none: grey at the value (the max channel)
        let g = saturate(c, 0.0);
        assert!(g.iter().all(|x| (x - 0.8).abs() < 1e-6));
        // half: each channel halfway to the max, hue kept
        let h = saturate(c, 0.5);
        assert!((h[0] - 0.8).abs() < 1e-5 && (h[1] - 0.6).abs() < 1e-5 && (h[2] - 0.5).abs() < 1e-5, "{h:?}");
    }

    #[test]
    fn bake_blends_in_linear_light() {
        let mut skin = Image { w: 2, h: 1, rgba: vec![255, 255, 255, 200, 255, 255, 255, 200], srgb: true };
        let d = std::sync::Arc::new(Image { w: 1, h: 1, rgba: vec![0, 0, 0, 255], srgb: true });
        let mut l = layer();
        l.opacity = 0.5;
        // only the left half: u -> u * 2
        l.uv = [[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert_eq!(bake(&mut skin, &[(&l, None)], &|_| Some(d.clone()), None, Space::Part), Skipped::default());
        // black art at half coverage over white: linear 0.5 -> sRGB 188
        assert_eq!(&skin.rgba[..4], &[188, 188, 188, 200]);
        assert_eq!(&skin.rgba[4..], &[255, 255, 255, 200]);
        assert_eq!(bake(&mut skin, &[(&l, None)], &|_| None, None, Space::Part), Skipped { missing: 1, ..Default::default() });
        // a mask on a layer with no ink is left out
        let mask = std::sync::Arc::new(Image { w: 2, h: 1, rgba: vec![0, 128, 0, 255, 0, 120, 0, 200], srgb: false });
        l.colours = [0x004c_c382, 0, 0];
        assert!(mask.is_mask() && !d.is_mask());
        assert_eq!(bake(&mut skin, &[(&l, None)], &|_| Some(mask.clone()), None, Space::Part), Skipped { uncoloured_masks: 1, ..Default::default() });
        // makeup is left out, and the texture stays as it was
        l.colours = [0xff00_ff00, 0, 0];
        l.p3[0][3] = 1.0;
        assert_eq!(bake(&mut skin, &[(&l, None)], &|_| Some(d.clone()), None, Space::Part), Skipped { makeup: 1, ..Default::default() });
        assert_eq!(&skin.rgba[..4], &[188, 188, 188, 200]);
        // a black ink on the mask, multiplied: 188 x black at full coverage
        l.p3 = [[0.0; 4]; 3];
        l.colours = [0xff00_0000, 0, 0];
        l.opacity = 1.0;
        l.flags = 0x0400_0000;
        let solid = std::sync::Arc::new(Image { w: 1, h: 1, rgba: vec![0, 127, 0, 255], srgb: false });
        assert_eq!(bake(&mut skin, &[(&l, None)], &|_| Some(solid.clone()), None, Space::Part), Skipped::default());
        assert_eq!(&skin.rgba[..4], &[0, 0, 0, 200]);
        assert_eq!(&skin.rgba[4..], &[255, 255, 255, 200]);
    }

    #[test]
    fn makeup_moves_the_ink_toward_the_skin_tone() {
        // HSV round trips
        for c in [[0.2, 0.5, 0.9], [0.9, 0.1, 0.1], [0.4, 0.4, 0.4]] {
            let back = hsv_to_rgb(rgb_to_hsv(c));
            assert!(close(back, c, 1e-6), "{c:?} {back:?}");
        }
        // CIELAB: white is L 100, a 0, b 0; round trip
        let w = srgb_to_lab([1.0, 1.0, 1.0]);
        assert!(close(w, [100.0, 0.0, 0.0], 1e-3), "{w:?}");
        let c = [0.8, 0.5, 0.35];
        assert!(close(lab_to_srgb(srgb_to_lab(c)), c, 1e-4));
        // w = 0: unchanged
        assert_eq!(makeup_ink(0xff91_6610, [5.0, 0.0, 0.0, 0.0], 0xff42_6890), 0xff91_6610);
        // w = 1 with no offset: the skin tone itself (as HSV), alpha kept
        let base = 0xff6b_8ec7; // sRGB (199, 142, 107)
        let m = makeup_ink(0x8091_6610, [0.0, 0.0, 0.0, 1.0], base);
        assert_eq!(m >> 24, 0x80);
        let rgb = hsv_to_rgb([(m & 0xff) as f32 / 255.0, (m >> 8 & 0xff) as f32 / 255.0, (m >> 16 & 0xff) as f32 / 255.0]);
        assert!(close(rgb, [199.0 / 255.0, 142.0 / 255.0, 107.0 / 255.0], 0.01), "{rgb:?}");
        // a lower L darkens it
        let dark = makeup_ink(0xff91_6610, [-20.0, 0.0, 0.0, 1.0], base);
        assert!((dark >> 16 & 0xff) < (m >> 16 & 0xff));
        // the tone word: linear colour to sRGB bytes, R in the low byte
        let c = Catalogue { skin_tones: vec![(DEFAULT_SKIN_TONE, [1.0, 0.2, 0.0])], ..Default::default() };
        // linear 0.2 is sRGB 0.4845: byte 124
        assert_eq!(c.skin_tone_word(1), Some(0xff00_7cff));
        assert_eq!(Catalogue::default().skin_tone_word(1), None);
        // the bake takes makeup only with a tone
        let mut skin = Image { w: 1, h: 1, rgba: vec![255, 255, 255, 255], srgb: true };
        let mask = std::sync::Arc::new(Image { w: 1, h: 1, rgba: vec![0, 127, 0, 255], srgb: false });
        let mut l = layer();
        l.colours = [0xff91_6610, 0, 0];
        l.p3[0] = [0.0, 0.0, 0.0, 1.0];
        assert_eq!(bake(&mut skin, &[(&l, None)], &|_| Some(mask.clone()), None, Space::Part), Skipped { makeup: 1, ..Default::default() });
        assert_eq!(bake(&mut skin, &[(&l, None)], &|_| Some(mask.clone()), Some(base), Space::Part), Skipped::default());
        assert_ne!(skin.rgba, vec![255, 255, 255, 255]);
    }

    /// A record and its layer laid out as the game's PSO stores them.
    #[test]
    fn reads_records_from_a_pso() {
        use crate::meta7a::{Member, StructDef};
        let mut sys = vec![0u8; 0x40];
        let rec_at = sys.len();
        sys.resize(rec_at + 224, 0);
        let lay_at = sys.len();
        sys.resize(lay_at + 320, 0);
        let put32 = |s: &mut Vec<u8>, o: usize, v: u32| s[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put32(&mut sys, rec_at + 4, 0x6b3b_2e6f);
        put32(&mut sys, rec_at + 108, 0x3817_131d);
        put32(&mut sys, rec_at + 160, 1);
        // pointer to block 2, offset 0
        sys[rec_at + 168..rec_at + 176].copy_from_slice(&((0u64 << 20) | 2).to_le_bytes());
        sys[lay_at + 8..lay_at + 16].copy_from_slice(&0x0742_adfc_36b5_6660u64.to_le_bytes());
        put32(&mut sys, lay_at + 68, 0xff11_2233);
        for (i, f) in [1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.25, 0.5, 1.0, 0.0].iter().enumerate() {
            put32(&mut sys, lay_at + 224 + 4 * i, f.to_bits());
        }
        put32(&mut sys, lay_at + 144 + 12, 0.5f32.to_bits());
        put32(&mut sys, lay_at + 212, 0.75f32.to_bits());
        put32(&mut sys, lay_at + 216, 1.0f32.to_bits());
        put32(&mut sys, lay_at + 220, 4);
        let m = |hash: u32, off: u32| Member { hash, off, ty: 0, sub: 0, reference: 0, extra: 0 };
        let mut structs = HashMap::new();
        structs.insert(RECORD, StructDef { hash: RECORD, size: 224, members: vec![m(0xacec_22be, 4), m(0xb536_67be, 108), m(0xef88_8d7e, 160)] });
        structs.insert(LAYER, StructDef { hash: LAYER, size: 320, members: vec![m(0x5ada_ff50, 8), m(0x0d23_ee2d, 68), m(0x207e_ec03, 80), m(0x4b62_1302, 208), m(0xc88b_9e6d, 212), m(0x0d4d_d331, 216), m(0x8765_2efd, 220), m(0x8b42_4621, 224), m(0x6eea_8588, 144)] });
        let p = Pso { res: crate::resource::Resource { sys, gfx: Vec::new(), gfx_base: 0 }, structs, struct64: HashMap::new(), blocks: vec![(RECORD, 224, rec_at), (LAYER, 320, lay_at)], root: None };
        let c = Catalogue::from_pso(&p, "test").unwrap();
        let d = c.get(0x6b3b_2e6f).unwrap();
        assert_eq!(d.category, 0x3817_131d);
        let l = &d.layers[0];
        assert_eq!(l.textures, vec![0x0742_adfc_36b5_6660]);
        assert_eq!(l.colours, [0xff11_2233, 0, 0]);
        assert_eq!((l.opacity, l.mode), (0.75, 4));
        assert_eq!(l.uv[2], [0.25, 0.5, 1.0]);
        assert_eq!(l.p3[0], [0.0, 0.0, 0.0, 0.5]);
        assert!(l.is_makeup());
        assert_eq!(skin_slot(d.category), Some("body upper"));
    }

    #[test]
    fn a_mask_goes_with_the_layer_before_it() {
        let a = layer();
        let mut m = layer();
        m.flags = MASK;
        let b = layer();
        let ls = [a.clone(), m.clone(), b.clone(), m.clone()];
        let got = masked(&ls);
        assert_eq!(got.len(), 2);
        assert!(got[0].1.is_some() && got[1].1.is_some());
        // a mask opening a record is drawn as a layer
        let got = masked(std::slice::from_ref(&m));
        assert_eq!((got.len(), got[0].1.is_some()), (1, false));
    }

    #[test]
    fn a_mask_scales_the_coverage_by_its_alpha() {
        // the mask decal: left texel clear, right texel opaque
        let mt = Image { w: 2, h: 1, rgba: vec![0, 0, 0, 0, 0, 0, 0, 255], srgb: false };
        let mut m = layer();
        m.flags = MASK;
        m.opacity = 0.5;
        let (lo, hi) = (m.mask_factor(&mt, 0.0, 0.5), m.mask_factor(&mt, 1.0, 0.5));
        assert!((lo - 0.5).abs() < 1e-6 && (hi - 1.0).abs() < 1e-6, "{lo} {hi}");
        m.flags |= MASK_INVERTED;
        let (lo, hi) = (m.mask_factor(&mt, 0.0, 0.5), m.mask_factor(&mt, 1.0, 0.5));
        assert!((lo - 1.0).abs() < 1e-6 && (hi - 0.5).abs() < 1e-6, "{lo} {hi}");
        // baked: a black layer over white skin, masked by a full-opacity
        // mask whose decal is clear on the left half
        m.flags = MASK;
        m.opacity = 1.0;
        m.textures = vec![2];
        let l = layer();
        let black = std::sync::Arc::new(Image { w: 1, h: 1, rgba: vec![0, 0, 0, 255], srgb: false });
        let mt = std::sync::Arc::new(mt);
        let mut skin = Image { w: 4, h: 1, rgba: vec![255; 16], srgb: true };
        bake(&mut skin, &[(&l, Some(&m))], &|h| Some(if h == 2 { mt.clone() } else { black.clone() }), None, Space::Part);
        let red: Vec<u8> = skin.rgba.chunks(4).map(|p| p[0]).collect();
        assert_eq!((red[0], red[3]), (255, 0), "{red:?}");
    }

    #[test]
    fn a_whole_sheet_covers_both_sides_of_the_upper_body() {
        // no transform: the sheet is the upper body's texture as laid out
        // (UV3), both sides; moved at all, it is placed at (2u, v) and
        // stops at the first side
        let mut l = layer();
        assert!(l.is_sheet());
        let d = std::sync::Arc::new(Image { w: 1, h: 1, rgba: vec![0, 0, 0, 255], srgb: false });
        let white = || Image { w: 4, h: 1, rgba: vec![255; 16], srgb: true };
        let red = |s: &Image| s.rgba.chunks(4).map(|p| p[0]).collect::<Vec<_>>();
        let mut sheet = white();
        bake(&mut sheet, &[(&l, None)], &|_| Some(d.clone()), None, Space::BodyUpper);
        assert_eq!(red(&sheet), [0, 0, 0, 0]);
        l.uv[0][0] = 1.0001;
        assert!(!l.is_sheet());
        let mut moved = white();
        bake(&mut moved, &[(&l, None)], &|_| Some(d.clone()), None, Space::BodyUpper);
        assert_eq!(red(&moved), [0, 0, 255, 255]);
    }

    #[test]
    fn upper_body_space_holds_both_sides() {
        assert_eq!(Space::Part.place(0.75, 0.5), (0.75, 0.5));
        assert_eq!(Space::BodyUpper.place(0.75, 0.5), (1.5, 0.5));
        // a full-colour layer on body u 1.25..1.75 (the second side): the
        // upper body's texture (UV3 layout) takes it at u 0.625..0.875,
        // a texture placed in its own UV never reaches it
        let mut l = layer();
        l.uv = [[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-2.5, 0.0, 1.0]];
        let d = std::sync::Arc::new(Image { w: 1, h: 1, rgba: vec![0, 0, 0, 255], srgb: false });
        let white = || Image { w: 4, h: 1, rgba: vec![255; 16], srgb: true };
        let mut up = white();
        bake(&mut up, &[(&l, None)], &|_| Some(d.clone()), None, Space::BodyUpper);
        assert_eq!(up.rgba.chunks(4).map(|p| p[0]).collect::<Vec<_>>(), [255, 255, 0, 0]);
        let mut own = white();
        bake(&mut own, &[(&l, None)], &|_| Some(d.clone()), None, Space::Part);
        assert!(own.rgba.iter().all(|&b| b == 255));
    }

    #[test]
    fn split_layout_is_uv0_at_half_width_by_side() {
        let one = |u: f32, v: f32| [([u, v], [u / 2.0, v]), ([u, v], [0.5 + u / 2.0, v])];
        let body: Vec<_> = (1..20).flat_map(|i| one(i as f32 / 20.0, 0.3)).collect();
        assert!(is_split_layout(&body));
        // legs and heads: the fourth set is UV0, or something else
        let same: Vec<_> = (1..20).map(|i| ([i as f32 / 20.0, 0.3], [i as f32 / 20.0, 0.3])).collect();
        assert!(!is_split_layout(&same));
        let other: Vec<_> = (1..20).map(|i| ([i as f32 / 20.0, 0.3], [0.9, i as f32 / 20.0])).collect();
        assert!(!is_split_layout(&other));
    }

    #[test]
    fn uv_layout_reads_the_base_at_each_texels_uv0() {
        // base: red at u0 < 0.5, blue above
        let base = Image { w: 2, h: 1, rgba: vec![255, 0, 0, 255, 0, 0, 255, 255], srgb: true };
        let reds = |im: &Image| im.rgba.chunks(4).map(|p| p[0]).collect::<Vec<_>>();
        // no triangles: the half-width rule, u0 = fract(2 u)
        let t = uv_layout(&base, &[]);
        assert_eq!((t.w, t.h), (4, 1));
        assert_eq!(reds(&t), [255, 0, 255, 0]);
        // a strip whose layout UV is its UV0: read in place
        let c = |u: f32, v: f32| ([u, v], [u, v]);
        let quad = [[c(0.0, 0.0), c(1.0, 0.0), c(1.0, 1.0)], [c(0.0, 0.0), c(1.0, 1.0), c(0.0, 1.0)]];
        // (bilinear: u0 0.375 and 0.625 sit between the two base texels)
        assert_eq!(reds(&uv_layout(&base, &quad)), [255, 191, 64, 0]);
    }

    #[test]
    fn a_gutter_takes_the_nearest_covered_texels_uv0() {
        // base: red at u0 < 0.5, blue above; an island in the layout's
        // left half (u 0..0.5) that reads the base's right half (blue)
        let base = Image { w: 4, h: 1, rgba: [[255, 0, 0, 255], [255, 0, 0, 255], [0, 0, 255, 255], [0, 0, 255, 255]].concat(), srgb: true };
        let c = |u0: f32, u: f32, v: f32| ([u0, v], [u, v]);
        let quad = [[c(0.75, 0.0, 0.0), c(1.0, 0.5, 0.0), c(1.0, 0.5, 1.0)], [c(0.75, 0.0, 0.0), c(1.0, 0.5, 1.0), c(0.75, 0.0, 1.0)]];
        let t = uv_layout(&base, &quad);
        assert_eq!(t.w, 8);
        // the island and every gutter texel right of it are blue; the
        // half-width rule would have read red at u 0.5..0.75
        assert!(t.rgba.chunks(4).all(|p| p[0] == 0 && p[2] == 255), "{:?}", t.rgba);
    }
}
