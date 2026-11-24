//! Ped clothing surface, baked into the garment's base texture: the tiling
//! fabric texture of each sub-material under its colour gradient, then the
//! tint-mask layers (trims, washes, prints), as the clothing shader composes
//! them (`matlib` module notes).
//!
//! The game samples the tiles per pixel; here they are resampled into the
//! garment's own texture, so the weave is as fine as that texture allows
//! (each texel takes the tile at the mip level of its footprint, which
//! gives the right mean colour where the weave is finer than a texel).
//!
//! Read from the shader: the gradient over the tile's green channel, the
//! layer order and the lerp by mask x opacity x tint alpha. Assumed: the
//! tiling UV is the mesh's second UV set, transformed as scale, then
//! rotation, then translation; a layer without a scale uses the tile's;
//! texture values are taken as linear. Not applied: colour set B
//! (`tint2`), the layer flags other than invert, the tile normal map,
//! stitches, dirt and sheen.

use crate::glb;
use sixng_core::matlib::{self, Library};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// One channel of a tiling texture with its mip chain.
pub struct Tex {
    mips: Vec<(u32, u32, Vec<u8>)>,
}

impl Tex {
    fn new(w: u32, h: u32, px: Vec<u8>) -> Tex {
        let mut mips = vec![(w, h, px)];
        loop {
            let (pw, ph, p) = mips.last().unwrap();
            if *pw <= 1 && *ph <= 1 {
                break;
            }
            let (nw, nh) = ((*pw / 2).max(1), (*ph / 2).max(1));
            let mut n = Vec::with_capacity((nw * nh) as usize);
            for y in 0..nh {
                for x in 0..nw {
                    let at = |dx: u32, dy: u32| p[(((y * 2 + dy).min(ph - 1)) * pw + (x * 2 + dx).min(pw - 1)) as usize] as u32;
                    n.push(((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8);
                }
            }
            mips.push((nw, nh, n));
        }
        Tex { mips }
    }

    fn level(&self, l: usize, u: f32, v: f32) -> f32 {
        let (w, h, p) = &self.mips[l];
        let (x, y) = (u * *w as f32 - 0.5, v * *h as f32 - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let at = |dx: i64, dy: i64| p[((y0 as i64 + dy).rem_euclid(*h as i64) * *w as i64 + (x0 as i64 + dx).rem_euclid(*w as i64)) as usize] as f32;
        ((at(0, 0) * (1.0 - fx) + at(1, 0) * fx) * (1.0 - fy) + (at(0, 1) * (1.0 - fx) + at(1, 1) * fx) * fy) / 255.0
    }

    /// Value 0..1 at (u, v), repeating; `foot` = how much of the texture
    /// (in u, v units) one output texel covers.
    fn sample(&self, u: f32, v: f32, foot: f32) -> f32 {
        let lod = (foot * self.mips[0].0 as f32).max(1.0).log2().min((self.mips.len() - 1) as f32);
        let l = lod.floor() as usize;
        let a = self.level(l, u, v);
        if l + 1 >= self.mips.len() {
            return a;
        }
        a + (self.level(l + 1, u, v) - a) * (lod - l as f32)
    }
}

struct Layer {
    mask: Arc<Tex>,
    /// gradient stops of the tint (linear) and its weight (opacity x alpha)
    stops: [[f32; 3]; 3],
    weight: f32,
    invert: bool,
    scale: [f32; 2],
    rot: (f32, f32),
    shift: [f32; 2],
}

#[derive(Default)]
struct Sub {
    stops: Option<[[f32; 3]; 3]>,
    alpha: f32,
    tile: Option<Arc<Tex>>,
    scale: f32,
    rot: (f32, f32),
    layers: Vec<Layer>,
}

/// What a garment preset says about its eight sub-materials.
pub struct Fabric {
    subs: Vec<Sub>,
}

fn library(game: &Path) -> Option<&'static Library> {
    static LIB: OnceLock<Option<Library>> = OnceLock::new();
    LIB.get_or_init(|| {
        let work = sixng_core::names::work_dir()?;
        Library::load(&sixng_core::export::Workspace::new(game, work)).ok()
    })
    .as_ref()
}

/// One channel (0 = red, 1 = green) of a store texture.
fn texture(game: &Path, hash: &str, channel: usize) -> Option<Arc<Tex>> {
    static CACHE: OnceLock<Mutex<HashMap<(u64, usize), Option<Arc<Tex>>>>> = OnceLock::new();
    let h = u64::from_str_radix(hash, 16).ok().filter(|&h| h != 0)?;
    let cache = CACHE.get_or_init(Default::default);
    if let Some(t) = cache.lock().unwrap().get(&(h, channel)) {
        return t.clone();
    }
    let t = (|| {
        let (store, t) = sixng_core::ttpf::global(game).get(h)?;
        let data = store.data(t).ok()?;
        let (rgba, w, hh) = sixng_core::ttpf::decode_rgba(t, &data, 0).ok()?;
        Some(Arc::new(Tex::new(w, hh, rgba.chunks_exact(4).map(|p| p[channel]).collect())))
    })();
    cache.lock().unwrap().insert((h, channel), t.clone());
    t
}

/// The fabric of a garment preset; None when the library or the preset is
/// not there, or nothing in it has a colour.
pub fn fabric(game: &Path, preset: u32) -> Option<Arc<Fabric>> {
    static CACHE: OnceLock<Mutex<HashMap<u32, Option<Arc<Fabric>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(f) = cache.lock().unwrap().get(&preset) {
        return f.clone();
    }
    let f = (|| {
        let lib = library(game)?;
        lib.presets.get(&preset)?;
        let sc = |r: Option<f32>| r.map_or((0.0, 1.0), |r| r.sin_cos());
        let mut subs = Vec::new();
        for s in 0..8 {
            let Some(c) = lib.colour(preset, s, 0) else {
                subs.push(Sub::default());
                continue;
            };
            let tile = lib.tile(preset, s);
            let scale = tile.as_ref().and_then(|t| t.scale).unwrap_or(1.0);
            let layers = lib
                .layers(preset, s)
                .into_iter()
                .filter_map(|l| {
                    let tint = l.tint?;
                    Some(Layer {
                        mask: texture(game, &l.mask_map, 0)?,
                        stops: matlib::stops([tint[0], tint[1], tint[2]]),
                        weight: l.opacity * tint[3] as f32 / 255.0,
                        invert: l.invert,
                        scale: [l.scale[0].unwrap_or(scale), l.scale[1].or(l.scale[0]).unwrap_or(scale)],
                        rot: sc(l.rotation),
                        shift: [l.translation[0].unwrap_or(0.0), l.translation[1].unwrap_or(0.0)],
                    })
                })
                .collect();
            subs.push(Sub {
                stops: Some(matlib::stops([c[0], c[1], c[2]])),
                alpha: c[3] as f32 / 255.0,
                tile: tile.as_ref().and_then(|t| texture(game, &t.material_map, 1)),
                scale,
                rot: sc(tile.as_ref().and_then(|t| t.rotation)),
                layers,
            });
        }
        subs.iter().any(|s| s.stops.is_some()).then(|| Arc::new(Fabric { subs }))
    })();
    cache.lock().unwrap().insert(preset, f.clone());
    f
}

fn gradient(stops: &[[f32; 3]; 3], g: f32) -> [f32; 3] {
    let (a, b, t) = if g < 0.5 { (&stops[0], &stops[1], 2.0 * g) } else { (&stops[1], &stops[2], 2.0 * (g - 0.5)) };
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn srgb(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
}

impl Fabric {
    /// Colour (linear) of sub-material `s` at tiling UV `uv`; `foot` = the
    /// tiling-UV distance one output texel covers.
    fn colour(&self, s: usize, uv: [f32; 2], foot: f32) -> Option<[f32; 3]> {
        let sub = self.subs.get(s).filter(|x| x.stops.is_some()).or_else(|| self.subs.iter().find(|x| x.stops.is_some()))?;
        let stops = sub.stops.as_ref()?;
        let turn = |p: [f32; 2], (sn, cs): (f32, f32)| [p[0] * cs - p[1] * sn, p[0] * sn + p[1] * cs];
        let g = match &sub.tile {
            Some(t) => {
                let p = turn([uv[0] * sub.scale, uv[1] * sub.scale], sub.rot);
                t.sample(p[0], p[1], foot * sub.scale.abs())
            }
            None => 0.5,
        };
        let c = gradient(stops, g);
        let mut c = c.map(|x| x * sub.alpha + (1.0 - sub.alpha) * g);
        for l in &sub.layers {
            let p = turn([uv[0] * l.scale[0], uv[1] * l.scale[1]], l.rot);
            let m = l.mask.sample(p[0] + l.shift[0], p[1] + l.shift[1], foot * l.scale[0].abs().max(l.scale[1].abs()));
            let t = (if l.invert { 1.0 - m } else { m }) * l.weight;
            let k = gradient(&l.stops, g);
            c = [0, 1, 2].map(|i| c[i] + (k[i] - c[i]) * t);
        }
        Some(c)
    }
}

/// Bake the fabric into base image `image` of `m`, shared by the materials
/// `mats`. The image holds the garment's fabric shade (green, mid grey 125
/// on plain cloth), which stays multiplied in. `ids` gives the sub-material
/// per texel at a size (the garment's `materialid` texture); without it the
/// sub-material is the part number in the raw vertex colour's green byte.
/// False (and nothing changed) when no triangle maps into the texture.
pub fn bake(m: &mut glb::Model, mats: &[usize], image: usize, fab: &Fabric, ids: &dyn Fn(u32, u32) -> Option<Vec<u8>>) -> bool {
    let (sw, sh) = (m.images[image].mips[0].0, m.images[image].mips[0].1);
    if sw == 0 || sh == 0 {
        return false;
    }
    // the weave wants more texels than most shade maps have
    let up = (2048 / sw.max(sh)).max(1);
    let (w, h) = (sw * up, sh * up);
    let n = (w * h) as usize;
    let ids = ids(w, h);
    // per texel: tiling UV, its footprint, sub-material (255 = no triangle)
    let mut uv = vec![[0f32; 2]; n];
    let mut foot = vec![0f32; n];
    let mut sub = vec![255u8; n];
    for me in m.meshes.iter().filter(|me| me.material.is_some_and(|k| mats.contains(&k)) && me.uv_sets >= 2) {
        for t in me.indices.chunks_exact(3) {
            let v = [&me.verts[t[0] as usize], &me.verts[t[1] as usize], &me.verts[t[2] as usize]];
            let p = v.map(|v| [v.uv0[0] * w as f32, v.uv0[1] * h as f32]);
            let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
            if area.abs() < 1e-6 || !area.is_finite() {
                continue;
            }
            let s = ((v[0].col[1] >> 2) & 7) as u8;
            // how far the tiling UV moves per texel, along x and along y
            let d = |k: usize| {
                let (a, b) = (v[1].uv1[k] - v[0].uv1[k], v[2].uv1[k] - v[0].uv1[k]);
                [(a * (p[2][1] - p[0][1]) - b * (p[1][1] - p[0][1])) / area, (b * (p[1][0] - p[0][0]) - a * (p[2][0] - p[0][0])) / area]
            };
            let (du, dv) = (d(0), d(1));
            let f = (du[0] * du[0] + dv[0] * dv[0]).sqrt().max((du[1] * du[1] + dv[1] * dv[1]).sqrt());
            let (x0, x1) = (p.iter().map(|q| q[0]).fold(f32::MAX, f32::min).floor() as i64, p.iter().map(|q| q[0]).fold(f32::MIN, f32::max).ceil() as i64);
            let (y0, y1) = (p.iter().map(|q| q[1]).fold(f32::MAX, f32::min).floor() as i64, p.iter().map(|q| q[1]).fold(f32::MIN, f32::max).ceil() as i64);
            if (x1 - x0) > 4 * w as i64 || (y1 - y0) > 4 * h as i64 {
                continue;
            }
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let q = [x as f32 + 0.5, y as f32 + 0.5];
                    let b1 = ((q[0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (q[1] - p[0][1])) / area;
                    let b2 = ((p[1][0] - p[0][0]) * (q[1] - p[0][1]) - (q[0] - p[0][0]) * (p[1][1] - p[0][1])) / area;
                    // half a texel of slack, so neighbouring triangles leave no holes
                    let e = 0.5 / area.abs().sqrt().max(1.0);
                    if b1 < -e || b2 < -e || b1 + b2 > 1.0 + e {
                        continue;
                    }
                    let k = (y.rem_euclid(h as i64) * w as i64 + x.rem_euclid(w as i64)) as usize;
                    uv[k] = [0, 1].map(|i| v[0].uv1[i] + (v[1].uv1[i] - v[0].uv1[i]) * b1 + (v[2].uv1[i] - v[0].uv1[i]) * b2);
                    foot[k] = f;
                    sub[k] = s;
                }
            }
        }
    }
    if sub.iter().all(|&s| s == 255) {
        return false;
    }
    // grow the islands a few texels into the gutters (filtering and mips reach there)
    for _ in 0..4 {
        let prev = sub.clone();
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let k = (y * w as i64 + x) as usize;
                if prev[k] != 255 {
                    continue;
                }
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let j = ((y + dy).rem_euclid(h as i64) * w as i64 + (x + dx).rem_euclid(w as i64)) as usize;
                    if prev[j] != 255 {
                        (uv[k], foot[k], sub[k]) = (uv[j], foot[j], prev[j]);
                        break;
                    }
                }
            }
        }
    }
    let shade = &m.images[image].mips[0].2;
    let mut px = vec![0u8; n * 4];
    use rayon::prelude::*;
    px.par_chunks_exact_mut(4 * w as usize).enumerate().for_each(|(y, row)| {
        for x in 0..w as usize {
            let k = y * w as usize + x;
            let s = match (&ids, sub[k]) {
                (Some(v), _) => v[k] as usize,
                (None, 255) => 0,
                (None, s) => s as usize,
            };
            // outside every triangle: the plain colour (middle of the gradient, no layers)
            let c = if sub[k] == 255 { fab.subs.get(s).and_then(|x| x.stops).or(fab.subs.iter().find_map(|x| x.stops)).map(|st| st[1]) } else { fab.colour(s, uv[k], foot[k]) }.unwrap_or([0.5; 3]);
            let g = shade[((y as u32 / up) * sw + x as u32 / up) as usize * 4 + 1] as f32 / 125.0;
            for i in 0..3 {
                row[x * 4 + i] = (srgb(c[i]) * g * 255.0).min(255.0) as u8;
            }
            row[x * 4 + 3] = 255;
        }
    });
    m.images[image].mips = glb::mip_chain(w, h, px);
    true
}
