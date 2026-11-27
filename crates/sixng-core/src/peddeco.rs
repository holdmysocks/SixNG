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
//! Colour: a decal is either full-colour art or a mask. In a mask, alpha
//! is the line art, green a flat 0.5 and red / blue the regions of a second
//! and third ink. The shader takes three colour stops from the layer's
//! record as plain bytes, and skips a stop whose alpha byte is 0, which
//! leaves the decal's own colour. The ink words of the catalogue are not
//! plain RGB (most read as mint green, and their low byte is always about
//! 0x82). The code that turns them into the shader's stops was not found.
//! So only full-colour art is baked: layers with no ink (every alpha byte
//! 0) whose decal is not a mask. BC3 masks with ink words of alpha 0 exist,
//! so the decal is checked too ([`Image::is_mask`]). The others are counted
//! as left out.

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

const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// Decal textures (non-zero ones of the five, in order).
    pub textures: Vec<u64>,
    /// Ink colours, ARGB.
    pub colours: [u32; 3],
    /// Clip quad: four edges `[x0, y0, x1, y1]` in the part's UV
    /// (`f32::MAX` coordinates: no clipping).
    pub clip: [[f32; 4]; 4],
    pub flags: u32,
    pub opacity: f32,
    pub saturation: f32,
    /// 0..5; what it selects is not decoded.
    pub mode: u32,
    /// Part UV -> decal UV (rows; see the module notes).
    pub uv: [[f32; 3]; 3],
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
    Some(Layer {
        textures: (0..5).map(|i| le64(d, tex + 8 * i)).filter(|&h| h != 0).collect(),
        colours: std::array::from_fn(|i| le32(d, col + 4 * i)),
        clip: std::array::from_fn(|i| std::array::from_fn(|k| f32at(d, clip + 16 * i + 4 * k))),
        flags: at(0x4b62_1302).map_or(0, |x| le32(d, x)),
        opacity: at(0xc88b_9e6d).map_or(1.0, |x| f32at(d, x)),
        saturation: at(0x0d4d_d331).map_or(1.0, |x| f32at(d, x)),
        mode: at(0x8765_2efd).map_or(0, |x| le32(d, x)),
        uv: std::array::from_fn(|i| std::array::from_fn(|k| f32at(d, uv + 16 * i + 4 * k))),
    })
}

impl Catalogue {
    /// The decorations of a PSO file that holds the record struct.
    pub fn from_pso(p: &Pso, source: &str) -> Option<Catalogue> {
        let rm = |h: u32| member(p, RECORD, h);
        let lm = |h: u32| member(p, LAYER, h);
        let lsize = p.structs.get(&LAYER)?.size as usize;
        let (name, cat, layers) = (rm(0xacec_22be)?, rm(0xb536_67be)?, rm(0xef88_8d7e)?);
        let d = &p.res.sys;
        let decorations: Vec<Decoration> = p
            .meta()
            .objects(RECORD)
            .into_iter()
            .map(|o| {
                let ls = array(p, o + layers).map(|(n, b)| (0..n).filter_map(|i| read_layer(p, b + i * lsize, &lm)).collect()).unwrap_or_default();
                Decoration { name: le32(d, o + name), category: le32(d, o + cat), layers: ls }
            })
            .collect();
        (!decorations.is_empty()).then(|| Catalogue { version: VERSION, source: source.into(), decorations, by_name: HashMap::new() }.indexed())
    }

    fn indexed(mut self) -> Self {
        self.by_name = self.decorations.iter().enumerate().map(|(i, d)| (d.name, i)).collect();
        self
    }

    pub fn get(&self, name: u32) -> Option<&Decoration> {
        self.by_name.get(&name).map(|&i| &self.decorations[i])
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
        let c = aix
            .entries
            .iter()
            .filter(|e| e.kind == crate::classify::Kind::Pso)
            .find_map(|e| {
                let b = crate::scan::read_entry(&a.path, e).ok()?;
                let p = crate::pso::parse(&b).filter(|p| p.structs.contains_key(&RECORD))?;
                Catalogue::from_pso(&p, &e.id)
            })
            .ok_or("no ped decoration catalogue found in data.rpf")?;
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

/// Part UV -> decal UV (None where the projective divisor vanishes).
pub fn decal_uv(m: &[[f32; 3]; 3], u: f32, v: f32) -> Option<(f32, f32)> {
    let w = m[0][2] * u + m[1][2] * v + m[2][2];
    if w.abs() < 1e-8 {
        return None;
    }
    Some(((m[0][0] * u + m[1][0] * v + m[2][0]) / w, (m[0][1] * u + m[1][1] * v + m[2][1]) / w))
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
impl Layer {
    /// The layer sets an ink: an ink word with a non-zero alpha byte. The
    /// game's shader then takes its colour from the ink words, whose encoding
    /// is not decoded (see the module notes), so such layers are not baked.
    pub fn uses_ink(&self) -> bool {
        self.colours.iter().any(|c| c >> 24 != 0)
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
    let hue = [(6.0 * h - 3.0).abs() - 1.0, 2.0 - (6.0 * h - 2.0).abs(), 2.0 - (6.0 * h - 4.0).abs()].map(|x| x.clamp(0.0, 1.0));
    hue.map(|x| ((x - 1.0) * sat + 1.0) * mx)
}

/// A full-colour layer's colour and coverage at one decal sample (`t`:
/// RGBA 0..1, colour channels sRGB-encoded): the decal's own colour,
/// saturated, with coverage = the decal's alpha x the layer's opacity.
/// Returns (sRGB colour, coverage).
pub fn shade(l: &Layer, t: [f32; 4]) -> ([f32; 3], f32) {
    (saturate([t[0], t[1], t[2]], l.saturation), (t[3] * l.opacity).clamp(0.0, 1.0))
}

/// An RGBA8 image.
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// A mask decal: where it covers (alpha over half), red and blue are
    /// about 0 and green is flat, for most texels. Its colour comes from the
    /// layer's inks in the game, also when the ink words' alpha bytes are 0
    /// (seen on BC3 masks), so it is not baked as art.
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

    /// Bilinear sample at UV (clamped to the edge texels), RGBA 0..1.
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
}

/// What [`bake`] left out.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Skipped {
    /// Layers whose decal texture is in no store.
    pub missing: usize,
    /// Layers with an ink ([`Layer::uses_ink`]), or whose decal is a mask
    /// ([`Image::is_mask`]): their colour is the undecoded ink.
    pub ink: usize,
}

/// Bake the full-colour layers, in order, into a part's colour texture
/// (sRGB RGBA8; its alpha is kept). `decal` gives a layer's first decal
/// texture. Layers with an ink, and layers whose decal is missing, are left
/// out and counted.
pub fn bake(skin: &mut Image, layers: &[&Layer], decal: &dyn Fn(u64) -> Option<std::sync::Arc<Image>>) -> Skipped {
    let mut skipped = Skipped::default();
    let (w, h) = (skin.w as usize, skin.h as usize);
    for l in layers {
        if l.uses_ink() {
            skipped.ink += 1;
            continue;
        }
        let Some(t) = l.textures.first().and_then(|&x| decal(x)) else {
            skipped.missing += 1;
            continue;
        };
        if t.is_mask() {
            skipped.ink += 1;
            continue;
        }
        skin.rgba.par_chunks_mut(4 * w).enumerate().for_each(|(y, row)| {
            let v = (y as f32 + 0.5) / h as f32;
            for x in 0..w {
                let u = (x as f32 + 0.5) / w as f32;
                if !inside_clip(&l.clip, u, v) {
                    continue;
                }
                let Some((du, dv)) = decal_uv(&l.uv, u, v) else { continue };
                // outside the decal: nothing (its edges are clear in every
                // decal checked, so this matches clamping too)
                if !(0.0..=1.0).contains(&du) || !(0.0..=1.0).contains(&dv) {
                    continue;
                }
                let (c, k) = shade(l, t.sample(du, dv));
                if k <= 0.0 {
                    continue;
                }
                let p = &mut row[4 * x..4 * x + 3];
                for i in 0..3 {
                    let s = srgb_to_lin(p[i] as f32 / 255.0);
                    let d = srgb_to_lin(c[i]);
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
        Layer { textures: vec![1], colours: [0, 0, 0], clip: [none; 4], flags: 0, opacity: 1.0, saturation: 1.0, mode: 0, uv: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] }
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
        l.colours = [0xff43_8982, 0, 0];
        assert!(l.uses_ink());
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
        let mut skin = Image { w: 2, h: 1, rgba: vec![255, 255, 255, 200, 255, 255, 255, 200] };
        let decal = Image { w: 1, h: 1, rgba: vec![0, 0, 0, 255] };
        let d = std::sync::Arc::new(decal);
        let mut l = layer();
        l.opacity = 0.5;
        // only the left half: u -> u * 2
        l.uv = [[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert_eq!(bake(&mut skin, &[&l], &|_| Some(d.clone())), Skipped::default());
        // black art at half coverage over white: linear 0.5 -> sRGB 188
        assert_eq!(&skin.rgba[..4], &[188, 188, 188, 200]);
        assert_eq!(&skin.rgba[4..], &[255, 255, 255, 200]);
        assert_eq!(bake(&mut skin, &[&l], &|_| None), Skipped { missing: 1, ink: 0 });
        // a mask decal is left out though its ink words are 0
        let mask = std::sync::Arc::new(Image { w: 2, h: 1, rgba: vec![0, 128, 0, 255, 0, 120, 0, 200] });
        l.colours = [0x004c_c382, 0, 0];
        assert!(mask.is_mask() && !d.is_mask());
        assert_eq!(bake(&mut skin, &[&l], &|_| Some(mask.clone())), Skipped { missing: 0, ink: 1 });
        // an ink layer is left out, and the texture stays as it was
        l.colours = [0xff00_0000, 0, 0];
        assert_eq!(bake(&mut skin, &[&l], &|_| Some(d.clone())), Skipped { missing: 0, ink: 1 });
        assert_eq!(&skin.rgba[..4], &[188, 188, 188, 200]);
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
        put32(&mut sys, lay_at + 212, 0.75f32.to_bits());
        put32(&mut sys, lay_at + 216, 1.0f32.to_bits());
        put32(&mut sys, lay_at + 220, 4);
        let m = |hash: u32, off: u32| Member { hash, off, ty: 0, sub: 0, reference: 0, extra: 0 };
        let mut structs = HashMap::new();
        structs.insert(RECORD, StructDef { hash: RECORD, size: 224, members: vec![m(0xacec_22be, 4), m(0xb536_67be, 108), m(0xef88_8d7e, 160)] });
        structs.insert(LAYER, StructDef { hash: LAYER, size: 320, members: vec![m(0x5ada_ff50, 8), m(0x0d23_ee2d, 68), m(0x207e_ec03, 80), m(0x4b62_1302, 208), m(0xc88b_9e6d, 212), m(0x0d4d_d331, 216), m(0x8765_2efd, 220), m(0x8b42_4621, 224)] });
        let p = Pso { res: crate::resource::Resource { sys, gfx: Vec::new(), gfx_base: 0 }, structs, struct64: HashMap::new(), blocks: vec![(RECORD, 224, rec_at), (LAYER, 320, lay_at)], root: None };
        let c = Catalogue::from_pso(&p, "test").unwrap();
        let d = c.get(0x6b3b_2e6f).unwrap();
        assert_eq!(d.category, 0x3817_131d);
        let l = &d.layers[0];
        assert_eq!(l.textures, vec![0x0742_adfc_36b5_6660]);
        assert_eq!(l.colours, [0xff11_2233, 0, 0]);
        assert_eq!((l.opacity, l.mode), (0.75, 4));
        assert_eq!(l.uv[2], [0.25, 0.5, 1.0]);
        assert_eq!(skin_slot(d.category), Some("body upper"));
    }
}
