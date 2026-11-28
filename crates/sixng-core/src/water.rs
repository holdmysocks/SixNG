//! Water system data: the three streamed water grids (fog atlas, water
//! height map, shoreline wave grid), placed in world space; the ocean and
//! shore-wave settings of `common.rpf`; and the water keys of the
//! timecycle.
//!
//! # Grids
//!
//! `common.rpf` holds four `GridAtlasInfo` documents (clients
//! `waterfogatlas`, `waterfogatlas2`, `WaterSystemHeightMap`,
//! `shorelinegrid1`). They share one grid: `origin` is the **centre** of a
//! `width` x `height` box (16384 x 33792 m around (-5120, 7680), so x
//! -13312..3072, y -9216..24576), cut into `tileSize` cells (512 m): 32
//! columns by 66 rows. Cell `n` is `row * 32 + column`, counted from the
//! north-west corner (row 0 is the north edge). Each document names the
//! tile texture (`suffix`) and a global fallback texture that covers the
//! whole grid at a lower resolution. Texture row 0 is north and column 0
//! west, in tiles and fallbacks alike.
//!
//! | layer  | archive | tiles | tile texture | fallback |
//! |--------|---------|-------|--------------|----------|
//! | fog    | textures | 1,312 + 1 | 128 x 128 BC6H UF16, two per dictionary (`waterfog` scattering, `waterfog2` extinction) | 512 x 1056 BC6H (16 px/cell), one per channel |
//! | height | levels | 738 | 512 x 512 R16_UNORM (1 m/px) | 1024 x 2112 R16_UNORM (32 px/cell) |
//! | shore  | levels | 268 | 256 x 256 B8G8R8A8 (2 m/px) | 1024 x 2112 BC7 (32 px/cell) |
//!
//! The 2- and 4-byte tiles are stored in `SW_4KB_S` blocks (see
//! [`crate::gfx10`]).
//!
//! # Placement
//!
//! A tile is a texture dictionary, one per cell, named `<prefix><cell>`.
//! The archives keep no names, but the dictionaries are stored in name
//! order, which puts the cells in **decimal-string order** ("1000" <
//! "1001" < … < "2111" < "800" < … < "999"). Which cells have a tile is not
//! stored, so [`align`] matches the tiles, in stored order, with the
//! string-ordered cells by comparing each tile with its cell of the
//! fallback, and [`seams`] checks the result by edge continuity with the
//! neighbours. Results on the install:
//!
//! * fog: 1,312 tiles on cells 800..2111 (rows 25..65, every column); one
//!   more tile is stored last, after every number, where names with the
//!   map-swap override suffix (`tblux-`) would sort; it is probably that
//!   override's tile and is left unplaced. 98.6 % of
//!   2,037 scattering and 99.3 % of 1,555 extinction seams continue.
//! * height: 738 tiles, all readable, rows of 13 to 27 cells; 1,396 of
//!   1,400 seams continue.
//! * shore: 268 tiles along the coast; 268 of 271 seams continue.
//!
//! # Values
//!
//! * **Fog**: RGB scattering (`waterfog`) and extinction (`waterfog2`)
//!   coefficients. Multiplied by 1000 they are in the units of the
//!   timecycle keys `water_clip_scattering_*` / `water_clip_extinction_*`:
//!   the open sea holds extinction (8.05, 6.64, 6.65) x 1e-3, the weather
//!   cycles (11.19, 6.60, 6.60). Inferred from that agreement.
//! * **Height**: `(v - 0.5) * 1600` metres ([`height_m`]); ground, roofs and
//!   the seabed, 0.5 is sea level. Checked against the terrain height map
//!   of common.rpf (`sixng heightmap`): a fitted slope of 1599 m and offset
//!   -1.1 m, correlation 0.998 over 4.7 M land samples.
//! * **Shore** (as a shader reads B8G8R8A8: R, G, B, A):
//!   * B: the wave group in the top two bits and the distance across the
//!     band below ([`shore_b`]): groups 0..2 are `64 * group + 63 *
//!     distance`, group 3 is `192 + 32 * flag + 31 * distance`. Groups
//!     match `ShoreWavePlacementData` waveType 0..3 for 99.6 % of 21,157
//!     samples; types 3 and 4 share group 3 and the flag is set for type 4
//!     (8,961 of 8,965).
//!   * G and A: the band's direction, `(v - 128) / 127`: G against the
//!     placement direction's x (correlation -0.90), A with its y (0.88).
//!   * R: the along-shore coordinate (correlation 0.69 with the fraction of
//!     lateralCoord).
//!   * The empty texel is (R, G, B, A) = (0, 128, 0, 128).
use crate::resource::Resource;
use crate::texture::{self, Fmt, TexInfo};
use crate::timecycle::{self, Node};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

/// One `GridAtlasInfo` document.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub client: String,
    pub prefix: String,
    pub suffix: String,
    pub fallback: String,
    /// Centre of the grid box (x, y).
    pub origin: [f32; 2],
    pub width: f32,
    pub height: f32,
    pub tile: f32,
    pub stream_radius: f32,
}

fn val(n: &Node, k: &str) -> Option<f32> {
    n.child(k)?.attr("value")?.parse().ok()
}
fn text(n: &Node, k: &str) -> String {
    n.child(k).map(|c| c.text.trim().to_string()).unwrap_or_default()
}

impl Grid {
    pub fn from_node(n: &Node) -> Option<Grid> {
        if n.name != "GridAtlasInfo" {
            return None;
        }
        let o = n.child("origin")?;
        let g = Grid {
            client: text(n, "clientSystemName"),
            prefix: text(n, "tileTextureNamePrefix"),
            suffix: text(n, "tileTextureNameSuffix"),
            fallback: text(n, "globalFallbackTextureName"),
            origin: [o.attr("x")?.parse().ok()?, o.attr("y")?.parse().ok()?],
            width: val(n, "width")?,
            height: val(n, "height")?,
            tile: val(n, "tileSize")?,
            stream_radius: val(n, "streamRadius").unwrap_or(0.0),
        };
        (g.tile > 0.0 && g.cols() > 0 && g.rows() > 0).then_some(g)
    }
    pub fn cols(&self) -> usize {
        (self.width / self.tile).round() as usize
    }
    pub fn rows(&self) -> usize {
        (self.height / self.tile).round() as usize
    }
    pub fn cells(&self) -> usize {
        self.cols() * self.rows()
    }
    /// World box: min x, min y, max x, max y.
    pub fn world_box(&self) -> [f32; 4] {
        let (hw, hh) = (self.width / 2.0, self.height / 2.0);
        [self.origin[0] - hw, self.origin[1] - hh, self.origin[0] + hw, self.origin[1] + hh]
    }
    /// (column, row) of a cell, row 0 north.
    pub fn col_row(&self, cell: usize) -> (usize, usize) {
        (cell % self.cols(), cell / self.cols())
    }
    /// World box of a cell: min x, min y, max x, max y.
    pub fn cell_box(&self, cell: usize) -> [f32; 4] {
        let (c, r) = self.col_row(cell);
        let b = self.world_box();
        let x0 = b[0] + c as f32 * self.tile;
        let y1 = b[3] - r as f32 * self.tile;
        [x0, y1 - self.tile, x0 + self.tile, y1]
    }
    /// Cell holding world (x, y), if inside the grid.
    pub fn cell_at(&self, x: f32, y: f32) -> Option<usize> {
        let b = self.world_box();
        let c = ((x - b[0]) / self.tile).floor();
        let r = ((b[3] - y) / self.tile).floor();
        (c >= 0.0 && r >= 0.0 && (c as usize) < self.cols() && (r as usize) < self.rows()).then(|| r as usize * self.cols() + c as usize)
    }
    pub fn to_json(&self) -> Value {
        let b = self.world_box();
        json!({
            "client": self.client, "tile_prefix": self.prefix, "tile_texture": self.suffix, "fallback_texture": self.fallback,
            "origin_centre": self.origin, "width_m": self.width, "height_m": self.height, "cell_m": self.tile,
            "cols": self.cols(), "rows": self.rows(), "stream_radius_m": self.stream_radius,
            "world_box": {"min": [b[0], b[1]], "max": [b[2], b[3]]},
            "cell_index": "row * cols + column, row 0 = north edge (max y), column 0 = west edge (min x)",
        })
    }
}

/// Cells `0..n` in the order their decimal names sort as strings.
pub fn string_order(n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    v.sort_by_cached_key(|c| c.to_string());
    v
}

/// A decoded texture: `ch` channels per texel, row 0 north.
#[derive(Debug, Clone)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub ch: usize,
    pub px: Vec<f32>,
}

impl Image {
    pub fn at(&self, x: usize, y: usize, c: usize) -> f32 {
        self.px[(y * self.w + x) * self.ch + c]
    }
    /// Box-average of the sub-rectangle `(x0, y0, w, h)` to `s` x `s`.
    pub fn block(&self, x0: usize, y0: usize, w: usize, h: usize, s: usize) -> Vec<f32> {
        let mut out = vec![0f32; s * s * self.ch];
        let (bw, bh) = (w / s, h / s);
        let n = (bw * bh) as f32;
        for by in 0..s {
            for bx in 0..s {
                for y in 0..bh {
                    let row = ((y0 + by * bh + y) * self.w + x0 + bx * bw) * self.ch;
                    for x in 0..bw {
                        for c in 0..self.ch {
                            out[(by * s + bx) * self.ch + c] += self.px[row + x * self.ch + c];
                        }
                    }
                }
            }
        }
        out.iter_mut().for_each(|v| *v /= n);
        out
    }
    /// Box-average down to `w / f` x `h / f`.
    pub fn shrink(&self, f: usize) -> Image {
        if f <= 1 {
            return self.clone();
        }
        let (w, h) = (self.w / f, self.h / f);
        let mut px = vec![0f32; w * h * self.ch];
        for y in 0..h {
            for x in 0..w {
                for yy in 0..f {
                    for xx in 0..f {
                        for c in 0..self.ch {
                            px[(y * w + x) * self.ch + c] += self.at(x * f + xx, y * f + yy, c);
                        }
                    }
                }
            }
        }
        let n = (f * f) as f32;
        px.iter_mut().for_each(|v| *v /= n);
        Image { w, h, ch: self.ch, px }
    }
    pub fn mean(&self) -> Vec<f32> {
        let mut m = vec![0f64; self.ch];
        for (i, v) in self.px.iter().enumerate() {
            m[i % self.ch] += *v as f64;
        }
        m.iter().map(|v| (v / (self.w * self.h) as f64) as f32).collect()
    }
}

/// Decodes mip 0 of a texture to floats: BC6H as RGB half floats, R16 as
/// 0..1, 8-bit formats as 0..255 in RGBA order (B8G8R8A8 is swapped to
/// RGBA, as a shader sees it).
pub fn decode(r: &Resource, t: &TexInfo) -> Result<Image> {
    // R16 heights and BGRA shore tiles are single-mip SW_4KB_S textures,
    // untiled by `mip0_linear` (1,396 of 1,400 height and 268 of 271 shore
    // tile edges continue)
    let (fmt, lin) = texture::mip0_linear(r, t, false)?;
    let (w, h) = (t.width as usize, t.height as usize);
    let need = texture::mip_bytes(fmt, t.width, t.height, 0) as usize;
    if lin.len() < need || r.gfx.len() < t.data_off as usize + need {
        bail!("texture data incomplete ({} of {} bytes)", r.gfx.len().saturating_sub(t.data_off as usize), need);
    }
    match fmt {
        Fmt::Bc6u | Fmt::Bc6s => {
            let mut px = vec![0f32; w * h * 3];
            let bw = w.div_ceil(4);
            for by in 0..h.div_ceil(4) {
                for bx in 0..bw {
                    let o = (by * bw + bx) * 16;
                    let b = crate::bc6h::decode_block(lin[o..o + 16].try_into().unwrap(), fmt == Fmt::Bc6s);
                    for (p, rgb) in b.iter().enumerate() {
                        let (x, y) = (bx * 4 + p % 4, by * 4 + p / 4);
                        if x < w && y < h {
                            for c in 0..3 {
                                px[(y * w + x) * 3 + c] = texture::f16(rgb[c]);
                            }
                        }
                    }
                }
            }
            Ok(Image { w, h, ch: 3, px })
        }
        Fmt::R16 => Ok(Image { w, h, ch: 1, px: lin.chunks_exact(2).take(w * h).map(|b| u16::from_le_bytes([b[0], b[1]]) as f32 / 65535.0).collect() }),
        Fmt::Bgra8 | Fmt::Rgba8 | Fmt::Bc7 | Fmt::Bc1 | Fmt::Bc3 => {
            let rgba = texture::to_rgba(fmt, &lin, t.width, t.height)?;
            Ok(Image { w, h, ch: 4, px: rgba.iter().map(|&v| v as f32).collect() })
        }
        _ => bail!("unsupported water texture format {}", t.format),
    }
}

/// A texture dictionary entry that holds water textures.
#[derive(Debug, Clone)]
pub struct Dict {
    pub entry: String,
    pub off: u64,
    pub textures: Vec<TexInfo>,
}

impl Dict {
    pub fn tex(&self, name: &str) -> Option<&TexInfo> {
        self.textures.iter().find(|t| t.name.eq_ignore_ascii_case(name))
    }
}

/// Texture dictionaries of an archive that may hold any of `names`: those the
/// index named after one of them, plus the unnamed ones (`@`), checked by
/// their texture names. Returned in stored (offset) order.
pub fn find_dicts(archive: &std::path::Path, entries: &[crate::scan::Entry], names: &[&str]) -> Vec<(Dict, crate::scan::Entry)> {
    use rayon::prelude::*;
    let cand: Vec<&crate::scan::Entry> = entries
        .iter()
        .filter(|e| e.rtype.as_deref() == Some("txd"))
        .filter(|e| e.name.as_deref().is_some_and(|n| n == "@" || names.iter().any(|w| w.eq_ignore_ascii_case(n))))
        .collect();
    let mut out: Vec<(Dict, crate::scan::Entry)> = cand
        .par_iter()
        .filter_map(|e| {
            let r = Resource::load_sys(archive, e).ok()?;
            let textures: Vec<TexInfo> = texture::find_textures(&r).into_iter().filter(|t| names.iter().any(|w| t.name.eq_ignore_ascii_case(w))).collect();
            (!textures.is_empty()).then(|| (Dict { entry: e.id.clone(), off: e.off, textures }, (*e).clone()))
        })
        .collect();
    out.sort_by_key(|d| d.0.off);
    out
}

/// Placement of one stored tile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub cell: usize,
    /// Mean difference from the fallback at this cell.
    pub cost: f32,
    /// Lowest difference at any other cell the alignment allowed (the
    /// cells between the neighbouring tiles' cells), or infinity.
    pub runner_up: f32,
}

/// Alignment weights for [`align`].
#[derive(Debug, Clone, Copy)]
pub struct Weights {
    /// Per position of the order skipped between consecutive placed tiles.
    pub gap: f32,
    /// Per tile left unplaced.
    pub drop: f32,
    /// Per cell number between consecutive placed tiles beyond the next
    /// one (`|next - previous - 1|`), up to `cap`: tiles stored one after
    /// another usually cover neighbouring cells.
    pub jump: f32,
    pub cap: f32,
}

/// Fog: costs are in log units summed over both channels. The open sea
/// matches anywhere, so the jump term keeps the stored run contiguous
/// (cells 800..2111) instead of slipping tiles into the three-digit cells
/// that sort between them; the map-swap tile is left out (`drop`).
pub const FOG_WEIGHTS: Weights = Weights { gap: 0.002, drop: 0.5, jump: 0.05, cap: 1.0 };
/// Height: costs in 0..1 values (1600 m); rows of tiles have gaps, so no
/// jump term.
pub const HEIGHT_WEIGHTS: Weights = Weights { gap: 0.0005, drop: 0.025, jump: 0.0, cap: 0.0 };
/// Shore: costs in 0..1 channel values; the tiles are scattered along the
/// coast.
pub const SHORE_WEIGHTS: Weights = Weights { gap: 0.002, drop: 0.1, jump: 0.0, cap: 0.0 };

/// How far back (in positions) the exact transition cost is evaluated;
/// further back it is taken as `cap`.
const WINDOW: usize = 64;

/// Aligns tiles (in stored order) with cells (in `order`) monotonically,
/// minimising the sum of `cost(tile, cell)` and the transition weights:
/// a tile can be left unplaced (tiles that belong to no cell of the order,
/// such as the map-swap override tiles, whose names sort after every
/// number). Returns the placement of each tile.
pub fn align(n_tiles: usize, order: &[usize], w: Weights, cost: &(dyn Fn(usize, usize) -> f32 + Sync)) -> Vec<Option<Placed>> {
    use rayon::prelude::*;
    let m = order.len();
    if n_tiles == 0 || m == 0 {
        return vec![None; n_tiles];
    }
    let c: Vec<Vec<f32>> = (0..n_tiles).into_par_iter().map(|i| order.iter().map(|&cell| cost(i, cell)).collect()).collect();
    let link = |q: usize, p: usize| -> f32 {
        let d = (order[p] as f32 - order[q] as f32 - 1.0).abs();
        w.gap * (p - q - 1) as f32 + (w.jump * d).min(w.cap)
    };
    let inf = f32::INFINITY;
    // f[p]: best total so far with the last placed tile at position p;
    // none: best total with no tile placed. back: -2 = this tile dropped,
    // -1 = placed first, q = placed after the tile at q.
    const DROPPED: i32 = -2;
    const FIRST: i32 = -1;
    let mut back = vec![DROPPED; n_tiles * m];
    let mut f = vec![inf; m];
    let mut none = 0f32;
    for i in 0..n_tiles {
        let fp = &f;
        // prefix minimum of fp[q] - gap * q (value, position)
        let mut pre = Vec::with_capacity(m);
        let mut run = (inf, 0usize);
        for (q, v) in fp.iter().enumerate() {
            let t = v - w.gap * q as f32;
            if t < run.0 {
                run = (t, q);
            }
            pre.push(run);
        }
        let row: Vec<(f32, i32)> = (0..m)
            .into_par_iter()
            .map(|p| {
                let (mut v, mut how) = (c[i][p] + none, FIRST);
                let mut best = (inf, FIRST);
                let lo = p.saturating_sub(WINDOW);
                for q in lo..p {
                    let t = fp[q] + link(q, p);
                    if t < best.0 {
                        best = (t, q as i32);
                    }
                }
                if lo > 0 {
                    // further back the cap stands in for the jump term
                    let (bv, bq) = pre[lo - 1];
                    let t = bv + w.gap * (p as f32 - 1.0) + w.cap;
                    if t < best.0 {
                        best = (t, bq as i32);
                    }
                }
                if c[i][p] + best.0 < v {
                    v = c[i][p] + best.0;
                    how = best.1;
                }
                if fp[p] + w.drop < v {
                    v = fp[p] + w.drop;
                    how = DROPPED;
                }
                (v, how)
            })
            .collect();
        for (p, &(_, how)) in row.iter().enumerate() {
            back[i * m + p] = how;
        }
        f = row.into_iter().map(|x| x.0).collect();
        none += w.drop;
    }
    let end = (0..m).min_by(|&a, &b| f[a].total_cmp(&f[b])).unwrap();
    let mut pos: Vec<Option<usize>> = vec![None; n_tiles];
    let mut state = (f[end] <= none).then_some(end);
    for i in (0..n_tiles).rev() {
        let Some(p) = state else { break };
        match back[i * m + p] {
            DROPPED => {}
            FIRST => {
                pos[i] = Some(p);
                state = None;
            }
            q => {
                pos[i] = Some(p);
                state = Some(q as usize);
            }
        }
    }
    (0..n_tiles)
        .map(|i| {
            let p = pos[i]?;
            let lo = pos[..i].iter().rev().find_map(|x| *x).map_or(0, |q| q + 1);
            let hi = pos[i + 1..].iter().find_map(|x| *x).unwrap_or(m);
            let runner_up = (lo..hi).filter(|&q| q != p).map(|q| c[i][q]).fold(inf, f32::min);
            Some(Placed { cell: order[p], cost: c[i][p], runner_up })
        })
        .collect()
}

/// Signature size (s x s blocks per tile) used for placement.
pub const SIG: usize = 8;

/// What placement, seam checks and mosaics need of a decoded tile, so the
/// full tiles need not stay in memory.
#[derive(Debug, Clone)]
pub struct Summary {
    pub w: usize,
    pub h: usize,
    pub ch: usize,
    /// `SIG` x `SIG` box averages.
    pub sig: Vec<f32>,
    /// Edge texel lines, each the outer line then the one inside it:
    /// north, south, west, east.
    pub edges: [Vec<f32>; 4],
    /// The tile at mosaic resolution.
    pub small: Image,
    pub mean: Vec<f32>,
}

impl Summary {
    /// `small_px` texels per side for the mosaic; `point` samples instead of
    /// averaging (for packed channels).
    pub fn new(im: &Image, small_px: usize, point: bool) -> Summary {
        let line = |horizontal: bool, k: usize| -> Vec<f32> {
            let n = if horizontal { im.w } else { im.h };
            (0..n).flat_map(|i| (0..im.ch).map(move |c| (i, c))).map(|(i, c)| if horizontal { im.at(i, k, c) } else { im.at(k, i, c) }).collect()
        };
        let two = |horizontal: bool, outer: usize, inner: usize| [line(horizontal, outer), line(horizontal, inner)].concat();
        let f = (im.w / small_px.max(1)).max(1);
        let small = if point {
            let (w, h) = (im.w / f, im.h / f);
            Image { w, h, ch: im.ch, px: (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).flat_map(|(x, y)| (0..im.ch).map(move |c| im.at(x * f + f / 2, y * f + f / 2, c))).collect() }
        } else {
            im.shrink(f)
        };
        Summary {
            w: im.w,
            h: im.h,
            ch: im.ch,
            sig: im.block(0, 0, im.w, im.h, SIG),
            edges: [two(true, 0, 1), two(true, im.h - 1, im.h - 2), two(false, 0, 1), two(false, im.w - 1, im.w - 2)],
            small,
            mean: im.mean(),
        }
    }
}

/// Per-cell `SIG` x `SIG` signatures of a fallback covering the whole grid.
pub fn fallback_sigs(grid: &Grid, fallback: &Image) -> Vec<Vec<f32>> {
    let (cw, chh) = (fallback.w / grid.cols(), fallback.h / grid.rows());
    (0..grid.cells())
        .map(|cell| {
            let (c, r) = grid.col_row(cell);
            fallback.block(c * cw, r * chh, cw, chh, SIG)
        })
        .collect()
}

/// One layer's input to [`place`]: tile summaries, the fallback and the
/// space differences are measured in.
pub type PlaceInput<'a> = (&'a [Option<Summary>], &'a Image, fn(f32) -> f32);

/// Placement of tiles against fallbacks covering the whole grid. Layers
/// that share dictionaries (the two fog channels) are placed together: the
/// cost is the sum over layers. Unreadable tiles cost the same anywhere.
pub fn place(grid: &Grid, layers: &[PlaceInput], w: Weights) -> Vec<Option<Placed>> {
    let n = layers.first().map_or(0, |l| l.0.len());
    let prep: Vec<(Vec<Vec<f32>>, Vec<Option<Vec<f32>>>)> = layers
        .iter()
        .map(|(tiles, fallback, f)| {
            let fb = fallback_sigs(grid, fallback).into_iter().map(|v| v.into_iter().map(f).collect()).collect();
            let sig = tiles.iter().map(|t| t.as_ref().map(|t| t.sig.iter().map(|&v| f(v)).collect())).collect();
            (fb, sig)
        })
        .collect();
    let cost = |i: usize, cell: usize| -> f32 {
        prep.iter()
            .map(|(fb, sig)| match &sig[i] {
                None => 0.0,
                Some(s) => s.iter().zip(&fb[cell]).map(|(a, b)| (a - b).abs()).sum::<f32>() / s.len() as f32,
            })
            .sum()
    };
    align(n, &string_order(grid.cells()), w, &cost)
}

/// Mean difference across an edge (`a` against `b`, both outer line then
/// inner line) and the mean difference between each outer line and the
/// line inside it.
fn edge_diff(la: &[f32], lb: &[f32], f: fn(f32) -> f32) -> (f32, f32) {
    let n = la.len() / 2;
    let (mut seam, mut inner) = (0f32, 0f32);
    for k in 0..n {
        let (s0, s1, i0, i1) = (f(la[k]), f(lb[k]), f(la[n + k]), f(lb[n + k]));
        seam += (s0 - s1).abs();
        inner += (s0 - i0).abs() + (s1 - i1).abs();
    }
    (seam / n as f32, inner / (2 * n) as f32)
}

/// Edge continuity of a placement. For each pair of placed tiles that are
/// east-west or north-south neighbours it compares the mean difference
/// across the shared edge with the mean difference between each edge line
/// and the line inside it; the pair continues when the first is at most
/// twice the second plus `tol` (block-compressed tiles do not meet
/// exactly). Flat pairs (open sea, empty shore) are skipped. As a baseline
/// each east edge is also compared with the west edge of an unrelated tile.
/// Returns per tile (neighbours checked, neighbours continuing) and totals.
pub fn seams(grid: &Grid, tiles: &[Option<Summary>], placed: &[Option<Placed>], f: fn(f32) -> f32, tol: f32) -> (Vec<(u32, u32)>, Value) {
    let mut at = vec![usize::MAX; grid.cells()];
    for (i, p) in placed.iter().enumerate() {
        if let (Some(p), Some(_)) = (p, &tiles[i]) {
            at[p.cell] = i;
        }
    }
    let mut per = vec![(0u32, 0u32); tiles.len()];
    let (mut h, mut v) = ([0u32; 2], [0u32; 2]);
    let (mut errs, mut base) = (Vec::new(), Vec::new());
    for (i, p) in placed.iter().enumerate() {
        let (Some(a), Some(p)) = (&tiles[i], p) else { continue };
        let (c, r) = grid.col_row(p.cell);
        // (east?, neighbour, my edge, its edge)
        for (east, nb, ea, eb) in [(true, (c + 1 < grid.cols()).then(|| at[p.cell + 1]), 3, 2), (false, (r + 1 < grid.rows()).then(|| at[p.cell + grid.cols()]), 1, 0)] {
            let Some(j) = nb.filter(|&j| j != usize::MAX) else { continue };
            let b = tiles[j].as_ref().unwrap();
            let (la, lb) = (&a.edges[ea], &b.edges[eb]);
            if la.len() != lb.len() {
                continue;
            }
            let (seam, inner) = edge_diff(la, lb, f);
            if seam < 1e-6 && inner < 1e-6 {
                continue;
            }
            let ok = seam <= 2.0 * inner + tol;
            errs.push(seam);
            if east {
                // an unrelated readable tile, a fixed stride away in stored order
                let k = (i + tiles.len() / 3 + 1) % tiles.len();
                if let Some(o) = tiles[k].as_ref().filter(|o| o.edges[2].len() == la.len()) {
                    base.push(edge_diff(la, &o.edges[2], f).0);
                }
            }
            let k = if east { &mut h } else { &mut v };
            k[0] += 1;
            k[1] += ok as u32;
            for t in [i, j] {
                per[t].0 += 1;
                per[t].1 += ok as u32;
            }
        }
    }
    let median = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    (per, json!({"east_west": {"pairs": h[0], "continuous": h[1]}, "north_south": {"pairs": v[0], "continuous": v[1]},
        "tolerance": tol, "median_seam_difference": median(errs), "median_difference_to_unrelated_tiles": median(base)}))
}

/// The XML documents of `common.rpf` this module reads, by root element.
pub const SETTINGS_ROOTS: &[&str] = &["OceanSimPresets", "OceanWaveCalmingCurves", "PatchConstants", "ShoreWavePlacementData", "EnvironmentalWaterDisturbManager"];

/// Every XML root element of the archive's XML entries, with its source
/// (`<entry id>` or `<entry id>#<n>`).
pub fn xml_docs(archive: &std::path::Path, entries: &[crate::scan::Entry]) -> Vec<(String, Node)> {
    use rayon::prelude::*;
    let mut docs: Vec<(u64, String, Node)> = entries
        .par_iter()
        .filter(|e| e.kind == crate::classify::Kind::Xml)
        .flat_map_iter(|e| {
            let roots = crate::scan::read_entry(archive, e).ok().and_then(|b| timecycle::parse_xml_all(&String::from_utf8_lossy(&b))).unwrap_or_default();
            roots.into_iter().enumerate().map(|(n, r)| (e.off, if n == 0 { e.id.clone() } else { format!("{}#{n}", e.id) }, r)).collect::<Vec<_>>()
        })
        .collect();
    docs.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    docs.into_iter().map(|(_, s, n)| (s, n)).collect()
}

/// The grids of the `GridAtlasInfo` documents.
pub fn grids(docs: &[(String, Node)]) -> Vec<(String, Grid)> {
    docs.iter().filter_map(|(s, n)| Some((s.clone(), Grid::from_node(n)?))).collect()
}

/// Water timecycle keys: every `water_*` variable of the weather cycles
/// (per cycle and region, one number per time sample) and of the modifiers
/// that set any (value, and the keyframes of animated ones).
pub fn timecycle_keys(d: &timecycle::Decoded) -> Value {
    let is_water = |k: &str| k.starts_with("water_");
    let filt = |o: &Value| -> Value {
        Value::Object(o.as_object().map(|m| m.iter().filter(|(k, _)| is_water(k)).map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default())
    };
    let cycles: Vec<Value> = d
        .cycles
        .iter()
        .map(|c| {
            let regions: serde_json::Map<String, Value> = c["regions"].as_object().map(|r| r.iter().map(|(k, v)| (k.clone(), filt(v))).collect()).unwrap_or_default();
            json!({"name": c["name"], "file": c["file"], "regions": regions})
        })
        .collect();
    let mods: Vec<Value> = d
        .modifiers
        .iter()
        .filter_map(|m| {
            let vars = filt(&m["vars"]);
            let axes: Vec<Value> = m["axes"].as_array().map(|a| a.iter().filter_map(|x| {
                let v = filt(&x["vars"]);
                (!v.as_object()?.is_empty()).then(|| json!({"control_value": x["control_value"], "blend_type": x["blend_type"], "keyframe_t_value": x["vars"]["keyframe_t_value"], "vars": v}))
            }).collect()).unwrap_or_default();
            (!vars.as_object()?.is_empty() || !axes.is_empty()).then(|| json!({"name": m["name"], "file": m["file"], "vars": vars, "axes": axes}))
        })
        .collect();
    let mut names = std::collections::BTreeSet::new();
    for c in &cycles {
        for r in c["regions"].as_object().into_iter().flat_map(|r| r.values()) {
            names.extend(r.as_object().into_iter().flat_map(|o| o.keys().cloned()));
        }
    }
    for m in &mods {
        names.extend(m["vars"].as_object().into_iter().flat_map(|o| o.keys().cloned()));
        for a in m["axes"].as_array().into_iter().flatten() {
            names.extend(a["vars"].as_object().into_iter().flat_map(|o| o.keys().cloned()));
        }
    }
    json!({"hours": d.time["samples"].as_array().map(|s| s.iter().map(|x| x["hour"].clone()).collect::<Vec<_>>()), "variables": names, "cycles": cycles, "modifiers": mods})
}

/// The settings documents as JSON, keyed by root element.
pub fn settings_json(docs: &[(String, Node)]) -> Vec<(String, String, Value)> {
    docs.iter().filter(|(_, n)| SETTINGS_ROOTS.contains(&n.name.as_str())).map(|(s, n)| (n.name.clone(), s.clone(), timecycle::to_json(n))).collect()
}

/// Loads and decodes one texture of a dictionary entry.
pub fn load_texture(archive: &std::path::Path, e: &crate::scan::Entry, t: &TexInfo) -> Result<Image> {
    let r = Resource::load(archive, e).with_context(|| format!("load {}", e.id))?;
    decode(&r, t).with_context(|| format!("{} {}", e.id, t.name))
}

/// The texture named `name` in the first dictionary that holds it.
pub fn find_tex<'a>(dicts: &'a [(Dict, crate::scan::Entry)], name: &str) -> Option<(&'a crate::scan::Entry, &'a TexInfo)> {
    dicts.iter().find_map(|(d, e)| d.tex(name).map(|t| (e, t)))
}

/// Export options.
#[derive(Debug, Clone)]
pub struct Options {
    /// Mosaic texels per cell side: fog (tiles have 128), height (512),
    /// shore (256).
    pub fog_px: usize,
    pub height_px: usize,
    pub shore_px: usize,
    /// Also write every height tile at its own resolution.
    pub height_tiles: bool,
    /// Also export the inland water surfaces ([`inland_tiles`]); scans
    /// every map file of levels.rpf.
    pub inland: bool,
    /// Also export pools, hot tubs, fountains, standing water and the other
    /// small water bodies ([`pool_surfaces`]); scans every map file of
    /// levels.rpf.
    pub pools: bool,
    /// Also export the water textures bound at run time ([`DETAIL_TEXTURES`]).
    pub detail: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { fog_px: 32, height_px: 128, shore_px: 32, height_tiles: false, inland: false, pools: false, detail: false }
    }
}

/// One decoded and placed grid layer.
pub struct Layer {
    pub name: &'static str,
    pub source: String,
    pub grid: Grid,
    pub tiles: Vec<(crate::scan::Entry, TexInfo)>,
    pub sums: Vec<Option<Summary>>,
    pub errors: Vec<Option<String>>,
    pub fallback: Option<(crate::scan::Entry, TexInfo, Image)>,
    pub placed: Vec<Option<Placed>>,
    pub seam_per_tile: Vec<(u32, u32)>,
    pub seams: Value,
    pub px: usize,
}

fn ident(v: f32) -> f32 {
    v
}
/// Fog coefficients span four decades: compare them on a log scale.
fn fog_space(v: f32) -> f32 {
    (1.0 + 1000.0 * v.max(0.0)).ln()
}
fn shore_space(v: f32) -> f32 {
    v / 255.0
}

impl Layer {
    /// Seam tolerance in the layer's difference space: 1 % of a fog
    /// coefficient, 0.3 m of height, two steps of an 8-bit shore channel.
    fn seam_tol(&self) -> f32 {
        match self.name {
            "fog_scattering" | "fog_extinction" => 0.01,
            "shore" => 2.0 / 255.0,
            _ => 0.3 / HEIGHT_RANGE_M,
        }
    }

    fn space(&self) -> fn(f32) -> f32 {
        match self.name {
            "fog_scattering" | "fog_extinction" => fog_space,
            "shore" => shore_space,
            _ => ident,
        }
    }

    /// Finds and decodes the tiles of `grid` among `dicts` (not placed yet,
    /// see [`Layer::place`]).
    #[allow(clippy::too_many_arguments)]
    pub fn build(name: &'static str, source: &str, grid: &Grid, archive: &std::path::Path, dicts: &[(Dict, crate::scan::Entry)], px: usize, point: bool) -> Result<Layer> {
        use rayon::prelude::*;
        let tiles: Vec<(crate::scan::Entry, TexInfo)> = dicts.iter().filter_map(|(d, e)| Some((e.clone(), d.tex(&grid.suffix)?.clone()))).collect();
        let fallback = match find_tex(dicts, &grid.fallback) {
            Some((e, t)) => (e.clone(), t.clone(), load_texture(archive, e, t)?),
            None => bail!("{name}: fallback texture {} not found", grid.fallback),
        };
        let dec: Vec<std::result::Result<Summary, String>> = tiles.par_iter().map(|(e, t)| load_texture(archive, e, t).map(|im| Summary::new(&im, px, point)).map_err(|x| format!("{x:#}"))).collect();
        let errors: Vec<Option<String>> = dec.iter().map(|r| r.as_ref().err().cloned()).collect();
        let sums: Vec<Option<Summary>> = dec.into_iter().map(|r| r.ok()).collect();
        Ok(Layer { name, source: source.into(), grid: grid.clone(), tiles, sums, errors, fallback: Some(fallback), placed: Vec::new(), seam_per_tile: Vec::new(), seams: Value::Null, px })
    }

    /// Places the tiles of layers that share their dictionaries (one
    /// placement for all) and checks the seams of each.
    pub fn place(layers: &mut [&mut Layer], w: Weights) {
        let Some(first) = layers.first() else { return };
        let grid = first.grid.clone();
        let inputs: Vec<PlaceInput> = layers.iter().filter_map(|l| Some((&l.sums[..], &l.fallback.as_ref()?.2, l.space()))).collect();
        let placed = place(&grid, &inputs, w);
        for l in layers.iter_mut() {
            let (per, sm) = seams(&l.grid, &l.sums, &placed, l.space(), l.seam_tol());
            l.placed = placed.clone();
            l.seam_per_tile = per;
            l.seams = sm;
        }
    }

    /// Cell -> tile index.
    pub fn by_cell(&self) -> Vec<Option<usize>> {
        let mut at = vec![None; self.grid.cells()];
        for (i, p) in self.placed.iter().enumerate() {
            if let Some(p) = p {
                at[p.cell] = Some(i);
            }
        }
        at
    }

    /// World mosaic, `px` texels per cell: tiles where placed and readable,
    /// the fallback elsewhere. Also returns the source per cell (2 tile,
    /// 1 fallback).
    pub fn mosaic(&self) -> (Image, Vec<u8>) {
        let (cols, rows, p) = (self.grid.cols(), self.grid.rows(), self.px);
        let ch = self.sums.iter().flatten().next().map(|s| s.ch).or(self.fallback.as_ref().map(|f| f.2.ch)).unwrap_or(1);
        let mut im = Image { w: cols * p, h: rows * p, ch, px: vec![0.0; cols * p * rows * p * ch] };
        let mut src = vec![0u8; cols * rows];
        let at = self.by_cell();
        for cell in 0..self.grid.cells() {
            let (c, r) = self.grid.col_row(cell);
            let tile = at[cell].and_then(|i| self.sums[i].as_ref()).filter(|s| s.small.w == p && s.ch == ch);
            for y in 0..p {
                for x in 0..p {
                    let o = ((r * p + y) * im.w + c * p + x) * ch;
                    if let Some(s) = tile {
                        im.px[o..o + ch].copy_from_slice(&s.small.px[(y * p + x) * ch..(y * p + x + 1) * ch]);
                    } else if let Some((_, _, fb)) = &self.fallback {
                        let (fw, fh) = (fb.w / cols, fb.h / rows);
                        let (sx, sy) = (c * fw + x * fw / p, r * fh + y * fh / p);
                        for k in 0..ch.min(fb.ch) {
                            im.px[o + k] = fb.at(sx, sy, k);
                        }
                    }
                }
            }
            src[cell] = if tile.is_some() { 2 } else if self.fallback.is_some() { 1 } else { 0 };
        }
        (im, src)
    }

    /// Per-tile records and the layer summary.
    pub fn report(&self) -> Value {
        let tiles: Vec<Value> = self
            .tiles
            .iter()
            .enumerate()
            .map(|(i, (e, t))| {
                let p = self.placed.get(i).copied().flatten();
                let (c, r) = p.map(|p| self.grid.col_row(p.cell)).unwrap_or((0, 0));
                json!({
                    "stored_index": i, "entry": e.id, "texture": t.name, "format": t.format, "size": [t.width, t.height],
                    "cell": p.map(|p| p.cell), "col": c, "row": r, "world_box": p.map(|p| self.grid.cell_box(p.cell)),
                    "match_cost": p.map(|p| p.cost), "next_best_cost": p.map(|p| if p.runner_up.is_finite() { json!(p.runner_up) } else { Value::Null }),
                    "seam_neighbours": self.seam_per_tile.get(i).map(|x| x.0), "seam_continuous": self.seam_per_tile.get(i).map(|x| x.1),
                    "mean": self.sums[i].as_ref().map(|s| s.mean.clone()), "error": self.errors[i],
                })
            })
            .collect();
        let unique = self.placed.iter().flatten().filter(|p| p.cost < p.runner_up).count();
        let unplaced: Vec<&str> = self.placed.iter().zip(&self.tiles).filter(|(p, _)| p.is_none()).map(|(_, t)| t.0.id.as_str()).collect();
        let verified = self.seam_per_tile.iter().filter(|x| x.1 > 0).count();
        let contested = self.seam_per_tile.iter().filter(|x| x.0 > 0 && x.1 == 0).count();
        json!({
            "layer": self.name, "grid_source": self.source, "grid": self.grid.to_json(),
            "tiles": self.tiles.len(), "readable": self.sums.iter().flatten().count(),
            "unreadable": self.errors.iter().flatten().count(),
            "placement": {
                "method": "monotone alignment of stored order with decimal-string cell order, matched against the fallback",
                "placed": self.placed.iter().flatten().count(),
                "unplaced": unplaced,
                "best_match_unique": unique,
                "tiles_with_a_continuous_neighbour": verified,
                "tiles_whose_checked_neighbours_all_break": contested,
                "tiles_without_textured_neighbour": self.seam_per_tile.iter().filter(|x| x.0 == 0).count(),
                "seams": self.seams,
            },
            "fallback": self.fallback.as_ref().map(|(e, t, _)| json!({"entry": e.id, "texture": t.name, "format": t.format, "size": [t.width, t.height]})),
            "tile_records": tiles,
        })
    }
}

fn write_json(p: &std::path::Path, v: &Value) -> Result<()> {
    crate::atomic::write(p, serde_json::to_vec_pretty(v)?)?;
    Ok(())
}

/// Raw little-endian f32, channels interleaved, rows north to south.
pub fn write_f32(p: &std::path::Path, im: &Image) -> Result<()> {
    let mut b = Vec::with_capacity(im.px.len() * 4);
    for v in &im.px {
        b.extend_from_slice(&v.to_le_bytes());
    }
    crate::atomic::write(p, b)?;
    Ok(())
}

/// Single channel 0..1 as 16-bit greyscale.
pub fn write_png16(p: &std::path::Path, im: &Image) -> Result<()> {
    let px: Vec<u16> = (0..im.w * im.h).map(|i| (im.px[i * im.ch].clamp(0.0, 1.0) * 65535.0).round() as u16).collect();
    let img = image::ImageBuffer::<image::Luma<u16>, Vec<u16>>::from_raw(im.w as u32, im.h as u32, px).ok_or_else(|| anyhow!("image size"))?;
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::from_path(p)?)?;
    crate::atomic::write(p, buf.into_inner())?;
    Ok(())
}

/// 0..255 RGBA as an RGBA8 PNG.
pub fn write_rgba8(p: &std::path::Path, im: &Image) -> Result<()> {
    let px: Vec<u8> = im.px.iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect();
    image::save_buffer(p, &px, im.w as u32, im.h as u32, image::ColorType::Rgba8)?;
    Ok(())
}

/// Fog coefficients as a preview: log scale, 1e-4 .. 1 per channel.
pub fn write_fog_preview(p: &std::path::Path, im: &Image) -> Result<()> {
    let px: Vec<u8> = im.px.iter().map(|&v| ((v.max(1e-4).log10() + 4.0) / 4.0 * 255.0).clamp(0.0, 255.0) as u8).collect();
    image::save_buffer(p, &px, im.w as u32, im.h as u32, image::ColorType::Rgb8)?;
    Ok(())
}

/// Value of sea level in a WaterSystemHeightMap texel (0..1).
pub const HEIGHT_ZERO: f32 = 0.5;
/// Metres per full value range.
pub const HEIGHT_RANGE_M: f32 = 1600.0;

/// Height in metres of a WaterSystemHeightMap value (0..1).
pub fn height_m(v: f32) -> f32 {
    (v - HEIGHT_ZERO) * HEIGHT_RANGE_M
}

/// Compares the placed height tiles with the terrain height map of
/// common.rpf: a least-squares line `terrain = a * (v - 0.5) + b` over land
/// samples of both, and how often the terrain map's sea is at or below 0.5.
pub fn check_heights(l: &Layer, hm: &crate::heightmap::HeightMap) -> Value {
    let (mut n, mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
    let (mut sea, mut sea_low) = (0u64, 0u64);
    let at = l.by_cell();
    for cell in 0..l.grid.cells() {
        let Some(s) = at[cell].and_then(|i| l.sums[i].as_ref()) else { continue };
        let b = l.grid.cell_box(cell);
        let p = s.small.w;
        let step = l.grid.tile / p as f32;
        for y in 0..p {
            for x in 0..p {
                let (wx, wy) = (b[0] + (x as f32 + 0.5) * step, b[3] - (y as f32 + 0.5) * step);
                let Some((z, land)) = hm.sample(wx, wy) else { continue };
                let v = s.small.at(x, y, 0);
                if !land {
                    sea += 1;
                    sea_low += (v <= HEIGHT_ZERO + 1e-4) as u64;
                } else if v > HEIGHT_ZERO + 1e-4 {
                    let (xv, yv) = ((v - HEIGHT_ZERO) as f64, z as f64);
                    n += 1.0;
                    sx += xv;
                    sy += yv;
                    sxx += xv * xv;
                    syy += yv * yv;
                    sxy += xv * yv;
                }
            }
        }
    }
    let cov = sxy - sx * sy / n;
    let (vx, vy) = (sxx - sx * sx / n, syy - sy * sy / n);
    let a = cov / vx;
    let b = (sy - a * sx) / n;
    json!({
        "land_samples": n as u64, "slope_m": a, "offset_m": b, "correlation": cov / (vx * vy).sqrt(),
        "terrain_sea_samples": sea, "terrain_sea_at_or_below_zero": if sea > 0 { json!(sea_low as f64 / sea as f64) } else { Value::Null },
    })
}

/// Pearson correlation of pairs.
fn correlation(v: &[(f32, f32)]) -> Option<f64> {
    let n = v.len() as f64;
    if v.len() < 3 {
        return None;
    }
    let (sx, sy) = v.iter().fold((0f64, 0f64), |a, p| (a.0 + p.0 as f64, a.1 + p.1 as f64));
    let (mx, my) = (sx / n, sy / n);
    let (mut c, mut vx, mut vy) = (0f64, 0f64, 0f64);
    for p in v {
        let (dx, dy) = (p.0 as f64 - mx, p.1 as f64 - my);
        c += dx * dy;
        vx += dx * dx;
        vy += dy * dy;
    }
    (vx > 0.0 && vy > 0.0).then(|| (c / (vx * vy).sqrt() * 1000.0).round() / 1000.0)
}

/// Shore wave group and distance across the band from channel B (0..255):
/// groups 0..2 use 64 values each (`64 * group + 63 * distance`); group 3
/// uses the top quarter in two halves, the second of which is flagged
/// (`192 + 32 * flag + 31 * distance`).
pub fn shore_b(b: u8) -> (u8, bool, f32) {
    let g = b >> 6;
    if g < 3 {
        (g, false, (b & 63) as f32 / 63.0)
    } else {
        (3, b & 32 != 0, (b & 31) as f32 / 31.0)
    }
}

/// Checks the shoreline tiles against `ShoreWavePlacementData`. From each
/// control point the texels at a quarter, half and three quarters of
/// `maxDistance` seaward (against `direction`) are read. The wave group of
/// channel B is compared with the patch's waveType (types 3 and 4 share
/// group 3, told apart by the flag bit), and B's distance, G and A and R
/// are correlated with the distance along the band, the two direction
/// components and the fraction of lateralCoord.
pub fn check_shore(l: &Layer, natives: &std::collections::HashMap<usize, Image>, placement: &Value) -> Value {
    let (mut pts, mut covered, mut active, mut agree, mut flag_ok, mut flag_n) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut by_type = [[0u64; 2]; 5];
    let (mut dist, mut gx, mut ay, mut rl) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let num = |v: &Value| v.as_f64().unwrap_or(0.0) as f32;
    for patch in placement.pointer("/Patches").and_then(Value::as_array).into_iter().flatten() {
        let wt = patch["waveType"].as_f64().unwrap_or(-1.0) as i64;
        for cp in patch["ControlPoints"].as_array().into_iter().flatten() {
            let (Some(p), Some(d)) = (cp["position"].as_array(), cp["direction"].as_array()) else { continue };
            if p.len() < 2 || d.len() < 2 {
                continue;
            }
            let md = num(&cp["maxDistance"]);
            let (dx, dy) = (num(&d[0]), num(&d[1]));
            let lat = num(&cp["lateralCoord"]);
            for t in [0.25f32, 0.5, 0.75] {
                let (x, y) = (num(&p[0]) - dx * md * t, num(&p[1]) - dy * md * t);
                pts += 1;
                let Some(cell) = l.grid.cell_at(x, y) else { continue };
                let Some(im) = natives.get(&cell) else { continue };
                covered += 1;
                let bx = l.grid.cell_box(cell);
                let tx = (((x - bx[0]) / l.grid.tile * im.w as f32) as usize).min(im.w - 1);
                let ty = (((bx[3] - y) / l.grid.tile * im.h as f32) as usize).min(im.h - 1);
                let (r, g, b, a) = (im.at(tx, ty, 0), im.at(tx, ty, 1), im.at(tx, ty, 2), im.at(tx, ty, 3));
                if r == 0.0 && b == 0.0 && g == 128.0 && a == 128.0 {
                    continue;
                }
                active += 1;
                let (group, flag, dd) = shore_b(b as u8);
                let ok = group as i64 == wt.clamp(0, 3);
                agree += ok as u64;
                if (0..5).contains(&wt) {
                    by_type[wt as usize][0] += 1;
                    by_type[wt as usize][1] += ok as u64;
                }
                if ok && wt >= 3 {
                    flag_n += 1;
                    flag_ok += (flag == (wt == 4)) as u64;
                }
                if ok {
                    dist.push((t, dd));
                    gx.push((dx, (g - 128.0) / 127.0));
                    ay.push((dy, (a - 128.0) / 127.0));
                    rl.push((lat.fract(), r / 255.0));
                }
            }
        }
    }
    json!({
        "samples": pts, "in_a_placed_tile": covered, "on_an_active_texel": active,
        "wave_group_matches": agree, "fraction": if active > 0 { json!(agree as f64 / active as f64) } else { Value::Null },
        "wave_type_4_flag_matches": {"samples": flag_n, "matches": flag_ok},
        "by_wave_type": by_type.iter().enumerate().map(|(t, x)| json!({"wave_type": t, "samples": x[0], "matches": x[1]})).collect::<Vec<_>>(),
        "correlation": {
            "b_distance_vs_distance_along_band": correlation(&dist),
            "g_vs_direction_x": correlation(&gx),
            "a_vs_direction_y": correlation(&ay),
            "r_vs_lateral_coord_fraction": correlation(&rl),
        },
    })
}

fn snake(s: &str) -> String {
    let mut o = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 && !s[..i].ends_with(|c: char| c.is_ascii_uppercase()) {
            o.push('_');
        }
        o.push(ch.to_ascii_lowercase());
    }
    o
}

/// Runs the whole export. Reads the common, textures and levels archives;
/// with `out` writes `<out>/water/`. Returns the report without per-tile
/// records.
pub fn export(ws: &crate::export::Workspace, out: Option<&std::path::Path>, o: &Options) -> Result<Value> {
    let common = ws.resolve("common")?;
    let cix = ws.load_index(&common)?;
    let docs = xml_docs(&common.path, &cix.entries);
    let grids = grids(&docs);
    let grid = |client: &str| grids.iter().find(|(_, g)| g.client.eq_ignore_ascii_case(client)).cloned().ok_or_else(|| anyhow!("no GridAtlasInfo for {client} in common"));
    let (fog1_src, fog1) = grid("waterfogatlas")?;
    let (fog2_src, fog2) = grid("waterfogatlas2")?;
    let (height_src, height) = grid("WaterSystemHeightMap")?;
    let (shore_src, shore) = grid("shorelinegrid1")?;
    let settings = settings_json(&docs);
    let entries: Vec<&crate::scan::Entry> = cix.entries.iter().collect();
    let tc = timecycle::decode(&common.path, &entries)?;
    let tc_keys = timecycle_keys(&tc);

    let tex = ws.resolve("textures")?;
    let tix = ws.load_index(&tex)?;
    let fog_dicts = find_dicts(&tex.path, &tix.entries, &[&fog1.suffix, &fog2.suffix, &fog1.fallback, &fog2.fallback]);
    drop(tix);
    let mut fog_s = Layer::build("fog_scattering", &fog1_src, &fog1, &tex.path, &fog_dicts, o.fog_px, false)?;
    let mut fog_e = Layer::build("fog_extinction", &fog2_src, &fog2, &tex.path, &fog_dicts, o.fog_px, false)?;
    Layer::place(&mut [&mut fog_s, &mut fog_e], FOG_WEIGHTS);

    let lv = ws.resolve("levels")?;
    let lix = ws.load_index(&lv)?;
    let lv_dicts = find_dicts(&lv.path, &lix.entries, &[&height.suffix, &height.fallback, &shore.suffix, &shore.fallback]);
    drop(lix);
    let mut hgt = Layer::build("height", &height_src, &height, &lv.path, &lv_dicts, o.height_px, false)?;
    let mut shr = Layer::build("shore", &shore_src, &shore, &lv.path, &lv_dicts, o.shore_px, true)?;
    Layer::place(&mut [&mut hgt], HEIGHT_WEIGHTS);
    Layer::place(&mut [&mut shr], SHORE_WEIGHTS);

    // shore tiles at full resolution, for the output and the check
    let shore_natives: std::collections::HashMap<usize, Image> = {
        use rayon::prelude::*;
        shr.tiles.par_iter().zip(&shr.placed).filter_map(|((e, t), p)| Some((p.as_ref()?.cell, load_texture(&lv.path, e, t).ok()?))).collect()
    };
    let placement = settings.iter().find(|s| s.0 == "ShoreWavePlacementData").map(|s| s.2.clone()).unwrap_or(Value::Null);
    let shore_check = check_shore(&shr, &shore_natives, &placement);
    let hm = crate::heightmap::load_all(ws, "common").ok().and_then(|m| crate::heightmap::best(&m).map(|i| m[i].1.clone()));
    let height_check = hm.as_ref().map(|m| check_heights(&hgt, m));
    let inland = if o.inland { Some(inland_tiles(ws)?) } else { None };
    let inland_check = inland.as_ref().map(|t| check_inland(t, &hgt));
    let pools = if o.pools { Some(pool_surfaces(ws)?) } else { None };
    let pools_check = pools.as_ref().map(|(p, _)| check_pools(p, &hgt));

    let mut rep = json!({
        "grids": grids.iter().map(|(s, g)| { let mut j = g.to_json(); j["source"] = json!(s); j }).collect::<Vec<_>>(),
        "fog_scattering": fog_s.report(), "fog_extinction": fog_e.report(), "height": hgt.report(), "shore": shr.report(),
        "height_check": height_check, "shore_check": shore_check,
        "settings": settings.iter().map(|(r, s, _)| json!({"root": r, "source": s})).collect::<Vec<_>>(),
        "timecycle_variables": tc_keys["variables"], "timecycle_cycles": tc_keys["cycles"].as_array().map(Vec::len),
        "timecycle_modifiers_with_water_keys": tc_keys["modifiers"].as_array().map(Vec::len),
    });
    if let (Some(t), Some(c)) = (&inland, &inland_check) {
        rep["inland"] = json!({"tiles": t.len(), "check_against_height_map": c});
    }
    let Some(out) = out else { return Ok(summary(&rep)) };
    let dir = out.join("water");
    for d in ["fog", "height", "shore/tiles", "settings"] {
        std::fs::create_dir_all(dir.join(d))?;
    }
    write_json(&dir.join("grids.json"), &rep["grids"])?;
    for (root, src, v) in &settings {
        write_json(&dir.join("settings").join(format!("{}.json", snake(root))), &json!({"source": src, "root": root, "data": v}))?;
    }
    write_json(&dir.join("timecycle_water.json"), &tc_keys)?;
    let mosaic = |l: &Layer, file: &str, fmt: &str| {
        let b = l.grid.world_box();
        json!({"file": file, "format": fmt, "width": l.grid.cols() * l.px, "height": l.grid.rows() * l.px, "texels_per_cell": l.px,
            "metres_per_texel": l.grid.tile / l.px as f32, "world_box": {"min": [b[0], b[1]], "max": [b[2], b[3]]},
            "texel_centre": "(min.x + (col + 0.5) * metres_per_texel, max.y - (row + 0.5) * metres_per_texel), row 0 north"})
    };

    // fog
    let (ms, src_s) = fog_s.mosaic();
    let (me, src_e) = fog_e.mosaic();
    write_f32(&dir.join("fog/scattering.f32"), &ms)?;
    write_f32(&dir.join("fog/extinction.f32"), &me)?;
    write_fog_preview(&dir.join("fog/scattering_preview.png"), &ms)?;
    write_fog_preview(&dir.join("fog/extinction_preview.png"), &me)?;
    let cell_mean = |l: &Layer, m: &Image, cell: usize| {
        let p = l.px;
        let (c, r) = l.grid.col_row(cell);
        let mut s = [0f64; 3];
        for y in 0..p {
            for x in 0..p {
                for (k, v) in s.iter_mut().enumerate() {
                    *v += m.at(c * p + x, r * p + y, k) as f64;
                }
            }
        }
        s.map(|v| (v / (p * p) as f64 * 1e7).round() / 1e7)
    };
    let names = ["none", "fallback", "tile"];
    let cells: Vec<Value> = (0..fog1.cells())
        .map(|cell| {
            let (c, r) = fog1.col_row(cell);
            json!({"cell": cell, "col": c, "row": r, "world_box": fog1.cell_box(cell),
                "scattering": cell_mean(&fog_s, &ms, cell), "extinction": cell_mean(&fog_e, &me, cell),
                "scattering_source": names[src_s[cell] as usize], "extinction_source": names[src_e[cell] as usize]})
        })
        .collect();
    write_json(&dir.join("fog/cells.json"), &json!({
        "units": "raw texture values (RGB); the timecycle keys water_clip_scattering_* and water_clip_extinction_* hold the same terms multiplied by 1000",
        "scattering_mosaic": mosaic(&fog_s, "scattering.f32", "f32 little endian, RGB interleaved"),
        "extinction_mosaic": mosaic(&fog_e, "extinction.f32", "f32 little endian, RGB interleaved"),
        "fallbacks": "scattering_fallback.f32 / extinction_fallback.f32: the whole grid, 16 texels per cell, same layout",
        "cells": cells,
    }))?;
    write_json(&dir.join("fog/scattering_tiles.json"), &rep["fog_scattering"])?;
    write_json(&dir.join("fog/extinction_tiles.json"), &rep["fog_extinction"])?;
    for (l, n) in [(&fog_s, "scattering"), (&fog_e, "extinction")] {
        if let Some((_, _, fb)) = &l.fallback {
            write_f32(&dir.join(format!("fog/{n}_fallback.f32")), fb)?;
        }
    }

    // height
    let (mh, src_h) = hgt.mosaic();
    write_png16(&dir.join("height/height.png"), &mh)?;
    let mask: Vec<u8> = src_h.iter().map(|&v| v * 127).collect();
    image::save_buffer(dir.join("height/sources.png"), &mask, height.cols() as u32, height.rows() as u32, image::ColorType::L8)?;
    if let Some((_, _, fb)) = &hgt.fallback {
        write_png16(&dir.join("height/fallback.png"), fb)?;
    }
    if o.height_tiles {
        use rayon::prelude::*;
        std::fs::create_dir_all(dir.join("height/tiles"))?;
        hgt.tiles.par_iter().zip(&hgt.placed).try_for_each(|((e, t), p)| -> Result<()> {
            let Some(p) = p else { return Ok(()) };
            if let Ok(im) = load_texture(&lv.path, e, t) {
                write_png16(&dir.join(format!("height/tiles/{}.png", p.cell)), &im)?;
            }
            Ok(())
        })?;
    }
    let mut hr = rep["height"].clone();
    hr["mosaic"] = mosaic(&hgt, "height.png", "16-bit greyscale PNG, the stored R16 value");
    hr["sources"] = json!("sources.png: one pixel per cell, 254 = tile, 127 = fallback");
    hr["height_m"] = json!(format!("(png / 65535 - {HEIGHT_ZERO}) * {HEIGHT_RANGE_M}"));
    hr["check_against_terrain"] = rep["height_check"].clone();
    write_json(&dir.join("height/cells.json"), &hr)?;

    // shore
    for (cell, im) in &shore_natives {
        write_rgba8(&dir.join(format!("shore/tiles/{cell}.png")), im)?;
    }
    let (msh, _) = shr.mosaic();
    write_rgba8(&dir.join("shore/mosaic.png"), &msh)?;
    if let Some((_, _, fb)) = &shr.fallback {
        write_rgba8(&dir.join("shore/fallback.png"), fb)?;
    }
    let mut sr = rep["shore"].clone();
    sr["tiles_dir"] = json!("tiles/<cell>.png: RGBA8 in the order a shader reads the B8G8R8A8 texture, 2 m per texel, row 0 north");
    sr["mosaic"] = mosaic(&shr, "mosaic.png", "RGBA8 PNG, point-sampled");
    sr["check_against_placement_data"] = rep["shore_check"].clone();
    write_json(&dir.join("shore/cells.json"), &sr)?;
    if let (Some(t), Some(c)) = (&inland, &inland_check) {
        rep["inland"] = write_inland(&dir.join("inland"), t, c, crate::ttpf::global(&ws.game))?;
    }
    if let (Some((p, unplaced)), Some(c)) = (&pools, &pools_check) {
        rep["pools"] = write_pools(&dir.join("pools"), p, unplaced, c)?;
    }
    if o.detail {
        rep["detail"] = write_detail(ws, &dir.join("detail"))?;
        rep["ocean"] = write_ocean(ws, &dir.join("ocean"), &hgt, inland.as_deref())?;
    }
    rep["out"] = json!(dir);
    Ok(summary(&rep))
}

/// The report without the per-tile records.
/// Kinds of inland water surface, from the drawable name
/// `<area>_water_<region>_<kind>_<n>`: code, name.
pub const INLAND_KINDS: &[(&str, &str)] = &[
    ("rvr", "river"),
    ("cnl", "canal"),
    ("crk", "creek"),
    ("rsr", "reservoir"),
    ("glf", "golf course pond"),
    ("stm", "stream"),
    ("dtc", "ditch"),
    ("mill", "mill pond"),
    ("drn", "drain"),
    ("nuc", "power plant water"),
];

/// The kind code of an inland water surface drawable, from its name: the
/// part after `_water_<region>_` (region `lo`, `fh`, `nb`, `ma`, ...).
/// `None` for other names and for LOD models (`lod` / `slod` anywhere in
/// the name).
pub fn inland_kind(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    if n.contains("lod") {
        return None;
    }
    let rest = &n[n.find("_water_")? + 7..];
    let mut parts = rest.split('_');
    let region = parts.next()?;
    let kind = parts.next()?;
    parts.next()?;
    if !(2..=3).contains(&region.len()) || !region.bytes().all(|b| b.is_ascii_lowercase()) {
        return None;
    }
    INLAND_KINDS.iter().find(|(c, _)| *c == kind).map(|(c, _)| *c)
}

/// One placed inland water surface: its mesh (LOD 0) in world space.
pub struct InlandTile {
    pub name: String,
    pub kind: &'static str,
    /// Map file (entry id) that places it.
    pub map_file: String,
    pub position: [f32; 3],
    /// Entity rotation (x, y, z, w), world.
    pub rotation: [f32; 4],
    pub scale: [f32; 2],
    /// World-space vertices (RAGE, Z-up, metres) and triangles.
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// UV0 per vertex (empty when the mesh has none).
    pub uvs: Vec<[f32; 2]>,
    /// The shader the LOD 0 mesh draws with (see [`shader_json`]).
    pub shader: Value,
    /// Store hashes of its flow texture and water height texture.
    pub flow: Option<u64>,
    pub height_tex: Option<u64>,
    /// The tile's surface map (`watersurfacemaptexture`, BC7).
    pub tile_tex: Option<u64>,
    /// The drawable's stored bbox (model space, min and max).
    pub bounds: Option<[[f32; 3]; 2]>,
}

impl InlandTile {
    /// Min and max corner of the world-space vertices.
    pub fn bbox(&self) -> [[f32; 3]; 2] {
        let mut b = [[f32::MAX; 3], [f32::MIN; 3]];
        for v in &self.positions {
            for c in 0..3 {
                b[0][c] = b[0][c].min(v[c]);
                b[1][c] = b[1][c].max(v[c]);
            }
        }
        b
    }
    /// Vertex heights at the 0, 5, 50, 95 and 100th percentiles. The
    /// median is the surface height; banks and slopes widen the spread.
    pub fn z_percentiles(&self) -> [f32; 5] {
        let mut z: Vec<f32> = self.positions.iter().map(|v| v[2]).collect();
        z.sort_by(f32::total_cmp);
        let at = |f: f32| z.get(((z.len().max(1) - 1) as f32 * f) as usize).copied().unwrap_or(0.0);
        [at(0.0), at(0.05), at(0.5), at(0.95), at(1.0)]
    }
}

/// A shader as JSON: effect name, textures (parameter, store hash, size,
/// format) and constants (parameter, values).
fn shader_json(s: &crate::drawable::Shader, set: &crate::ttpf::TtpfSet) -> Value {
    let tex: Vec<Value> = s.textures.iter().map(|t| {
        let h = u64::from_str_radix(&t.hash64, 16).unwrap_or(0);
        let mut e = json!({"param": crate::names::param(&t.param_hash).unwrap_or_else(|| t.param_hash.clone()), "hash64": t.hash64});
        if let Some((_, tx)) = set.get(h) {
            e["width"] = tx.width.into();
            e["height"] = tx.height.into();
            e["dxgi_format"] = tx.dxgi_format.into();
        }
        e
    }).collect();
    let consts: serde_json::Map<String, Value> = s.constants.iter().map(|c| (crate::names::param(&c.hash).unwrap_or_else(|| c.hash.clone()), c.json().unwrap_or(Value::Null))).collect();
    json!({"shader": crate::names::shader(&s.name_hash).unwrap_or_else(|| s.name_hash.clone()), "flags": s.flags, "textures": tex, "constants": consts})
}

/// How a tile's water height texture reads. A texel holds the surface
/// height relative to the placing entity: `z = origin z + (2 v - 1) H`,
/// where `H` is the top of the drawable's stored bbox (model z, times the
/// entity's z scale), and 0 marks texels without water. `H` equals the
/// largest distance of a surface vertex from the origin's height on every
/// sloped tile; on flat tiles it is under 1 cm, so the texture's values
/// there only span millimetres. Without a bbox, the vertex distance is
/// used. Checked by sampling the texture under every surface vertex
/// (nearest texel, texels of 0 skipped). Returns (H, check).
/// The shader slot of the per-tile BC7 texture (hash e0548ad3).
const TILE_TEX_PARAM: &str = "watersurfacemaptexture";

fn height_texture_check(t: &InlandTile, set: &crate::ttpf::TtpfSet) -> Option<(f32, Value)> {
    let (store, tx) = set.get(t.height_tex?)?;
    if t.uvs.len() != t.positions.len() || tx.width < 8 {
        return None;
    }
    let (f, ch, w, h) = store.data(tx).ok().and_then(|d| crate::ttpf::decode_float_slice(tx, &d, 0, 0).ok())?;
    let surface = t.z_percentiles()[2] - 0.05;
    let e = t.position[2];
    let top: Vec<usize> = (0..t.positions.len()).filter(|&i| t.positions[i][2] >= surface).collect();
    let hv = top.iter().map(|&i| (t.positions[i][2] - e).abs()).fold(0.0f32, f32::max);
    // flat tiles store a top a hair below 0 (about -0.00005 m)
    let hb = t.bounds.map(|b| b[1][2] * t.scale[1]).filter(|h| h.is_finite() && *h > -0.01).map(|h| h.max(0.0));
    let hr = hb.unwrap_or(hv);
    let mut vals = Vec::new();
    let mut err: Vec<f32> = top
        .iter()
        .filter_map(|&i| {
            let [u, v] = t.uvs[i];
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                return None;
            }
            let (x, y) = (((u * w as f32) as u32).min(w - 1), ((v * h as f32) as u32).min(h - 1));
            let val = f[((y * w + x) * ch) as usize];
            (val > 0.01).then(|| {
                vals.push(val);
                (e + (2.0 * val - 1.0) * hr - t.positions[i][2]).abs()
            })
        })
        .collect();
    err.sort_by(f32::total_cmp);
    let at = |q: f32| err.get(((err.len().max(1) - 1) as f32 * q) as usize).copied();
    // share of texels with water
    let wet = (0..(w * h) as usize).filter(|&k| f[k * ch as usize] > 0.0).count() as f32 / (w * h) as f32;
    vals.sort_by(f32::total_cmp);
    let median_value = vals.get(vals.len().saturating_sub(1) / 2).copied();
    Some((hr, json!({"scale_from": if hb.is_some() { "bounds" } else { "vertices" }, "vertex_distance_m": hv, "samples": err.len(), "median_error_m": at(0.5), "p95_error_m": at(0.95), "median_value": median_value, "wet_texel_share": wet})))
}

/// A drawable's LOD 0 mesh placed in world space.
struct PlacedMesh {
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
    /// UV0 per vertex ([0, 0] where the mesh has none).
    uvs: Vec<[f32; 2]>,
    /// Indices drawn per shader.
    used: std::collections::BTreeMap<u16, usize>,
}

/// The LOD 0 geometries of a drawable whose shader passes `keep` (all of
/// them when it has no model table), placed by an entity at `p` with
/// rotation `q` and scale (xy, z).
fn placed_mesh(r: &crate::resource::Resource, p: [f32; 3], q: [f32; 4], sc: [f32; 2], keep: impl Fn(u16) -> bool) -> PlacedMesh {
    let bufs = crate::drawable::find_buffers(r);
    let geoms = crate::drawable::find_geometries(r, &bufs);
    // LOD 0 geometries and the shader each draws with
    let lod0: std::collections::HashMap<usize, u16> = crate::drawable::find_models(r, &geoms).iter().filter(|m| m.lod == 0 && !m.part).flat_map(|m| m.geometries.iter().copied().zip(m.shader_indices.iter().copied())).collect();
    let mut out = PlacedMesh { positions: Vec::new(), indices: Vec::new(), uvs: Vec::new(), used: Default::default() };
    let slice = |i: usize| {
        let b = &bufs[i];
        let end = (b.gfx_off + b.bytes).min(r.gfx.len() as u64) as usize;
        &r.gfx[(b.gfx_off as usize).min(end)..end]
    };
    for (gi, g) in geoms.iter().enumerate() {
        if !lod0.is_empty() && !lod0.get(&gi).is_some_and(|&si| keep(si)) {
            continue;
        }
        let Some(h) = bufs[g.vertex_buffer].mesh_blob.as_ref() else { continue };
        let m = crate::drawable::decode_mesh(slice(g.vertex_buffer), h, slice(g.index_buffer));
        if let Some(&si) = lod0.get(&gi) {
            *out.used.entry(si).or_default() += m.indices.len();
        }
        let uv0 = m.uvs.iter().find(|(slot, _)| *slot == 3).map(|(_, u)| u.as_slice()).unwrap_or(&[]);
        out.uvs.extend((0..m.positions.len()).map(|i| uv0.get(i).copied().unwrap_or([0.0, 0.0])));
        let base = out.positions.len() as u32;
        out.indices.extend(m.indices.iter().map(|i| i + base));
        out.positions.extend(m.positions.iter().map(|v| {
            let w = crate::lights::rotate(q, [v[0] * sc[0], v[1] * sc[0], v[2] * sc[1]]);
            [w[0] + p[0], w[1] + p[1], w[2] + p[2]]
        }));
    }
    out
}

/// Every inland water surface placed on the map: the drawables of
/// levels.rpf whose name gives an [`inland_kind`], placed by the map files'
/// entities (not interior or group-local ones). Each is a tile of at most
/// 64 m; the install has 958, each placed once, with its own flow
/// texture and water height texture.
pub fn inland_tiles(ws: &crate::export::Workspace) -> Result<Vec<InlandTile>> {
    use rayon::prelude::*;
    let lv = ws.resolve("levels")?;
    let ix = ws.load_index(&lv)?;
    let mut water: std::collections::HashMap<u32, (String, &'static str, &crate::scan::Entry)> = std::collections::HashMap::new();
    for e in &ix.entries {
        if let (Some(n), Some("dr")) = (&e.name, e.rtype.as_deref()) {
            if let Some(k) = inland_kind(n) {
                water.insert(crate::meta7a::joaat(n), (n.clone(), k, e));
            }
        }
    }
    let set = crate::ttpf::global(&ws.game);
    let metas: Vec<&crate::scan::Entry> = ix.entries.iter().filter(|e| e.sub == "7a").collect();
    let scans = crate::export::scan_meta(&lv, &metas, false, std::sync::Arc::new(|_: &str, _: u64, _: u64| {}));
    let mut placed = Vec::new();
    for s in scans.iter().filter(|s| !s.group) {
        for (i, &(arch, p, q, sxy, sz)) in s.ents.iter().enumerate() {
            if !s.interior.contains(&i) && water.contains_key(&arch) {
                placed.push((arch, p, q, [sxy, sz], s.id.clone()));
            }
        }
    }
    let mut tiles: Vec<InlandTile> = placed
        .into_par_iter()
        .filter_map(|(arch, p, q, sc, file)| {
            let (name, kind, e) = &water[&arch];
            let r = crate::resource::Resource::load(&lv.path, e).ok()?;
            let shaders = crate::drawable::find_shaders(&r);
            let PlacedMesh { positions, indices, mut uvs, used } = placed_mesh(&r, p, q, sc, |_| true);
            // the shader drawing most triangles (the first one without a model table)
            let si = used.iter().max_by_key(|(_, n)| **n).map_or(0, |(i, _)| *i as usize);
            let sh = shaders.get(si);
            let tex = |param: &str| {
                sh.and_then(|s| s.textures.iter().find(|t| t.param_hash == param || crate::names::param(&t.param_hash).as_deref() == Some(param))).and_then(|t| u64::from_str_radix(&t.hash64, 16).ok())
            };
            if uvs.iter().all(|u| *u == [0.0, 0.0]) {
                uvs.clear();
            }
            (!indices.is_empty()).then(|| InlandTile { name: name.clone(), kind, map_file: file, position: p, rotation: q, scale: sc, positions, indices, uvs,
                shader: sh.map_or(Value::Null, |s| shader_json(s, set)), flow: tex("flowtexture"), height_tex: tex("waterheighttexture"), tile_tex: tex(TILE_TEX_PARAM),
                bounds: crate::drawable::drawable_bounds(&r) })
        })
        .collect();
    tiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(tiles)
}

/// Inland surfaces against the water height map (ground, river and lake
/// beds): for each tile the height map is sampled on a 9 x 9 grid over its
/// box. A surface sits above the lowest ground under it (the bed) and
/// below the highest (the banks).
pub fn check_inland(tiles: &[InlandTile], hgt: &Layer) -> Value {
    let (m, src) = hgt.mosaic();
    let b = hgt.grid.world_box();
    let mpt = hgt.grid.tile / hgt.px as f32;
    // the cell's source: its own tile (2) or the low-resolution fallback (1)
    let cell_src = |x: f32, y: f32| -> u8 {
        let (c, r) = (((x - b[0]) / hgt.grid.tile).floor(), ((b[3] - y) / hgt.grid.tile).floor());
        if c < 0.0 || r < 0.0 || c as usize >= hgt.grid.cols() || r as usize >= hgt.grid.rows() {
            return 0;
        }
        src.get(r as usize * hgt.grid.cols() + c as usize).copied().unwrap_or(0)
    };
    let mut outside = Vec::new();
    let ground = |x: f32, y: f32| -> Option<f32> {
        let (c, r) = (((x - b[0]) / mpt).floor(), ((b[3] - y) / mpt).floor());
        (c >= 0.0 && r >= 0.0 && (c as usize) < m.w && (r as usize) < m.h).then(|| height_m(m.at(c as usize, r as usize, 0)))
    };
    let mut per: std::collections::BTreeMap<&str, Vec<(f32, f32, bool)>> = Default::default();
    for t in tiles {
        let bb = t.bbox();
        let z = t.z_percentiles()[2];
        let g: Vec<f32> = (0..81).filter_map(|i| ground(bb[0][0] + (bb[1][0] - bb[0][0]) * (i % 9) as f32 / 8.0, bb[0][1] + (bb[1][1] - bb[0][1]) * (i / 9) as f32 / 8.0)).collect();
        if g.is_empty() {
            continue;
        }
        let lo = g.iter().copied().fold(f32::MAX, f32::min);
        let hi = g.iter().copied().fold(f32::MIN, f32::max);
        let ok = lo <= z && z <= hi;
        if !ok {
            let c = [(bb[0][0] + bb[1][0]) / 2.0, (bb[0][1] + bb[1][1]) / 2.0];
            let mut u: Vec<f32> = t.positions.iter().filter_map(|v| ground(v[0], v[1])).collect();
            u.sort_by(f32::total_cmp);
            let under = u.get(u.len() / 2).copied();
            outside.push(json!({"name": t.name, "kind": t.kind, "surface_z": z, "lowest_ground": lo, "highest_ground": hi,
                "side": if z < lo { "below the lowest ground" } else { "above the highest ground" }, "by_m": if z < lo { lo - z } else { z - hi },
                "cell": match cell_src(c[0], c[1]) { 2 => "tile", 1 => "fallback", _ => "none" }, "width_m": [bb[1][0] - bb[0][0], bb[1][1] - bb[0][1]],
                "vertices": t.positions.len(), "ground_above_surface_share": g.iter().filter(|&&h| h > z).count() as f32 / g.len() as f32,
                "ground_under_vertices_median": under}));
        }
        per.entry(t.kind).or_default().push((z - lo, hi - z, ok));
    }
    let med = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    let kinds: serde_json::Map<String, Value> = per
        .into_iter()
        .map(|(k, v)| {
            let between = v.iter().filter(|x| x.2).count();
            (k.to_string(), json!({"tiles": v.len(), "between_bed_and_banks": between,
                "median_surface_above_lowest_ground_m": med(v.iter().map(|x| x.0).collect()),
                "median_highest_ground_above_surface_m": med(v.iter().map(|x| x.1).collect())}))
        })
        .collect();
    // slivers: edge pieces of a channel cut by the 64 m tile grid, where the samples fall on the bank
    let sliver = |x: &Value| x["width_m"].as_array().is_some_and(|w| w.iter().any(|v| v.as_f64().unwrap_or(0.0) < 6.0)) || x["vertices"].as_u64().unwrap_or(0) < 30;
    let full: Vec<&Value> = outside.iter().filter(|x| !sliver(x)).collect();
    json!({"method": "height map sampled on a 9 x 9 grid over each tile's box; surface = median vertex height", "metres_per_texel": mpt, "kinds": kinds,
        "outside_tiles": outside.len(), "outside_slivers": outside.len() - full.len(), "outside_full_tiles": full.len(),
        "outside_full_tiles_within_0_65_m": full.iter().filter(|x| x["by_m"].as_f64().unwrap_or(f64::MAX) <= 0.65).count(),
        "slivers": "outside tiles under 6 m across or with fewer than 30 vertices: edge pieces of a channel cut by the 64 m tile grid", "outside": outside})
}

/// Kinds of small water body, from the drawable name (first match wins).
pub const POOL_KINDS: &[(&str, &[&str])] = &[
    ("hot tub", &["hottub", "hot_tub", "jacuzzi", "_spa_"]),
    ("waterfall", &["waterfall"]),
    ("fountain", &["fountain", "water_feature", "spout"]),
    ("water effect", &["vfx_water"]),
    ("standing water", &["standing_water", "puddle"]),
    ("tank", &["tank_water"]),
    ("ditch", &["ditch"]),
    ("pond", &["pond"]),
    ("pool", &["pool"]),
    ("other water", &["water"]),
];

/// The kind of a small water body drawable from its name, or None for
/// names that are not one: inland surfaces ([`inland_kind`]), LOD models,
/// the pause-menu map tiles, water plants, scum and splash decals.
pub fn pool_kind(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    if inland_kind(&n).is_some() || ["lod", "maptile_", "waterveg", "scum", "decal", "toilet", "tubes"].iter().any(|x| n.contains(x)) {
        return None;
    }
    POOL_KINDS.iter().find(|(_, keys)| keys.iter().any(|k| n.contains(k))).map(|(k, _)| *k)
}

/// A water shader: the water surface, fountain and waterfall effects, or
/// any shader whose render bucket is the water one (flag byte 4, 0x40).
fn is_water_shader(s: &crate::drawable::Shader) -> bool {
    ["7f3a6aa9", "97596162", "735d2de6"].contains(&s.name_hash.as_str()) || s.flags.get(4).is_some_and(|b| b & 0x40 != 0)
}

/// One placed small water body: the LOD 0 geometry of its water shaders,
/// in world space.
pub struct PoolSurface {
    pub name: String,
    pub kind: &'static str,
    pub map_file: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 2],
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub uvs: Vec<[f32; 2]>,
    /// The water shaders it draws with, most triangles first.
    pub shaders: Vec<String>,
    /// Set when an interior places it: position and rotation are then the
    /// entity's in interior space, and the geometry goes through the
    /// interior instance to world space.
    pub interior: Option<PoolInterior>,
}

/// The interior instance that places a [`PoolSurface`].
pub struct PoolInterior {
    /// joaat of the interior archetype name.
    pub archetype: u32,
    /// The interior archetype file that lists the entity.
    pub interior_file: String,
    /// The map file with the interior instance, and the instance's index in it.
    pub instance_map: String,
    pub instance: usize,
    /// The instance's world position and rotation (x, y, z, w, applied as stored).
    pub instance_position: [f32; 3],
    pub instance_rotation: [f32; 4],
    /// The entity belongs to an entity set (an optional interior state).
    pub entity_set: bool,
}

impl PoolSurface {
    fn bbox(&self) -> [[f32; 3]; 2] {
        let mut b = [[f32::MAX; 3], [f32::MIN; 3]];
        for v in &self.positions {
            for c in 0..3 {
                b[0][c] = b[0][c].min(v[c]);
                b[1][c] = b[1][c].max(v[c]);
            }
        }
        b
    }

    /// Median vertex height: the surface.
    fn surface_z(&self) -> f32 {
        let mut z: Vec<f32> = self.positions.iter().map(|v| v[2]).collect();
        z.sort_by(f32::total_cmp);
        z.get(z.len() / 2).copied().unwrap_or(0.0)
    }

    /// Area of the triangles seen from above, m².
    fn area(&self) -> f32 {
        self.indices
            .chunks(3)
            .filter(|t| t.len() == 3)
            .map(|t| {
                let (a, b, c) = (self.positions[t[0] as usize], self.positions[t[1] as usize], self.positions[t[2] as usize]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0
            })
            .sum()
    }
}

/// Number of named drawables per kind that neither a map file nor a placed
/// interior places.
pub type Unplaced = std::collections::BTreeMap<&'static str, usize>;

/// Every pool, hot tub, fountain, waterfall, standing water and other small
/// water body placed on the map by levels.rpf's map files, directly or
/// inside an interior instance (each instance places its interior's
/// entities, entity sets included): drawables picked by [`pool_kind`], kept
/// when a LOD 0 geometry draws with a water shader, and only those
/// geometries.
pub fn pool_surfaces(ws: &crate::export::Workspace) -> Result<(Vec<PoolSurface>, Unplaced)> {
    use rayon::prelude::*;
    let lv = ws.resolve("levels")?;
    let ix = ws.load_index(&lv)?;
    let mut cand: std::collections::HashMap<u32, (String, &'static str, &crate::scan::Entry)> = std::collections::HashMap::new();
    for e in &ix.entries {
        if let (Some(n), Some("dr")) = (&e.name, e.rtype.as_deref()) {
            if let Some(k) = pool_kind(n) {
                cand.insert(crate::meta7a::joaat(n), (n.clone(), k, e));
            }
        }
    }
    let metas: Vec<&crate::scan::Entry> = ix.entries.iter().filter(|e| e.sub == "7a").collect();
    let scans = crate::export::scan_meta(&lv, &metas, false, std::sync::Arc::new(|_: &str, _: u64, _: u64| {}));
    let mut placed: Vec<(u32, [f32; 3], [f32; 4], [f32; 2], String, Option<PoolInterior>)> = Vec::new();
    for s in scans.iter().filter(|s| !s.group) {
        for (i, &(arch, p, q, sxy, sz)) in s.ents.iter().enumerate() {
            if !s.interior.contains(&i) && cand.contains_key(&arch) {
                placed.push((arch, p, q, [sxy, sz], s.id.clone(), None));
            }
        }
    }
    // interiors: the first file that defines each archetype, as `sixng interiors` does
    let mut by_mlo: std::collections::HashMap<u32, (&crate::export::MetaScan, &crate::meta7a::Mlo)> = std::collections::HashMap::new();
    for s in &scans {
        for m in &s.mlos {
            by_mlo.entry(m.name).or_insert((s, m));
        }
    }
    for s in scans.iter().filter(|s| !s.group) {
        for (ii, inst) in s.instances.iter().enumerate() {
            let Some((isc, mlo)) = by_mlo.get(&inst.archetype) else { continue };
            let sets: std::collections::HashSet<usize> = mlo.set_entities.iter().copied().collect();
            for &ei in mlo.entities.iter().chain(&mlo.set_entities) {
                let Some(&(arch, p, q, sxy, sz)) = isc.ents.get(ei) else { continue };
                if cand.contains_key(&arch) {
                    let pi = PoolInterior { archetype: inst.archetype, interior_file: isc.id.clone(), instance_map: s.id.clone(), instance: ii,
                        instance_position: inst.position, instance_rotation: inst.rotation_world, entity_set: sets.contains(&ei) && !mlo.entities.contains(&ei) };
                    placed.push((arch, p, q, [sxy, sz], isc.id.clone(), Some(pi)));
                }
            }
        }
    }
    let seen: std::collections::HashSet<u32> = placed.iter().map(|x| x.0).collect();
    let mut unplaced = Unplaced::new();
    for (h, (_, k, _)) in &cand {
        if !seen.contains(h) {
            *unplaced.entry(*k).or_default() += 1;
        }
    }
    // each drawable is read once; its water shaders decide what is kept
    type Loaded = Option<(crate::resource::Resource, Vec<crate::drawable::Shader>)>;
    let loaded: std::collections::HashMap<u32, Loaded> = seen
        .par_iter()
        .map(|&h| {
            let r = crate::resource::Resource::load(&lv.path, cand[&h].2).ok().map(|r| {
                let sh = crate::drawable::find_shaders(&r);
                (r, sh)
            });
            (h, r.filter(|(_, sh)| sh.iter().any(is_water_shader)))
        })
        .collect();
    let mut out: Vec<PoolSurface> = placed
        .into_par_iter()
        .filter_map(|(arch, p, q, sc, file, interior)| {
            let (r, shaders) = loaded.get(&arch)?.as_ref()?;
            let (name, kind, _) = &cand[&arch];
            let PlacedMesh { mut positions, indices, mut uvs, used } = placed_mesh(r, p, q, sc, |si| shaders.get(si as usize).is_some_and(is_water_shader));
            if let Some(pi) = &interior {
                for v in &mut positions {
                    let w = crate::lights::rotate(pi.instance_rotation, *v);
                    *v = [w[0] + pi.instance_position[0], w[1] + pi.instance_position[1], w[2] + pi.instance_position[2]];
                }
            }
            if indices.is_empty() {
                return None;
            }
            if uvs.iter().all(|u| *u == [0.0, 0.0]) {
                uvs.clear();
            }
            let mut by: Vec<(u16, usize)> = used.into_iter().collect();
            by.sort_by(|a, b| b.1.cmp(&a.1));
            let mut names: Vec<String> = Vec::new();
            for s in by.iter().filter_map(|(si, _)| shaders.get(*si as usize)) {
                let n = crate::names::shader(&s.name_hash).unwrap_or_else(|| s.name_hash.clone());
                if !names.contains(&n) {
                    names.push(n);
                }
            }
            Some(PoolSurface { name: name.clone(), kind, map_file: file, position: p, rotation: q, scale: sc, positions, indices, uvs, shaders: names, interior })
        })
        .collect();
    let key = |p: &PoolSurface| p.interior.as_ref().map(|i| (i.instance_map.clone(), i.instance));
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.map_file.cmp(&b.map_file)).then(key(a).cmp(&key(b))).then(a.position.map(f32::to_bits).cmp(&b.position.map(f32::to_bits))));
    Ok((out, unplaced))
}

/// Water textures in the global dictionaries, which the game probably binds
/// at run time (the water drawables carry 4×4 placeholders for their detail
/// slots): archive, texture name, what it is for. The uses come from the names and the content, not
/// from shader code.
pub const DETAIL_TEXTURES: &[(&str, &str, &str)] = &[
    ("textures", "waterbump", "tiling water surface bump; data in green and alpha only"),
    ("textures", "waternoise", "tiling water noise; data in green and alpha only"),
    ("textures", "waterfoam", "tiling surface foam"),
    ("textures", "bubblefoam", "tiling bubble foam"),
    ("textures", "water_foam_height", "foam height"),
    ("textures", "caustic", "underwater caustics"),
    ("textures", "rainrippleuvoffset", "rain ripple UV offsets"),
    ("textures", "ocean_sim_gaussian_noise", "gaussian noise seeding the ocean wave simulation (R32 float)"),
    ("textures", "shore_wave_pixel_profile", "shore wave profile, per pixel"),
    ("textures", "shore_wave_vertex_profile", "shore wave profile, per vertex"),
    ("textures", "shore_wave_wavefront_profiles", "shore wave wavefront profiles"),
    ("textures", "underwaterfog", "map-wide underwater fog (512 x 1056)"),
    ("textures", "onoffshorewind", "map-wide onshore and offshore wind"),
    ("textures", "puddle_control", "map-wide puddle control"),
    ("textures", "puddle_layout", "tiling puddle layout"),
    ("textures", "puddle_layout1", "tiling puddle layout"),
    ("levels", "water_quality_control", "map-wide water quality (512 x 1056)"),
    ("levels", "ocean_wetness_control", "map-wide ocean wetness"),
    ("levels", "ocean_wave_control", "map-wide ocean wave strength (1024 x 2112)"),
    ("levels", "ocean_roughness_control", "map-wide ocean roughness"),
    ("levels", "mean_sea_level", "map-wide mean sea level (R16)"),
];

/// Every texture dictionary of an archive that holds one of `names`,
/// whatever the index calls it (the index names a dictionary after one of
/// its textures). Stored order.
fn dicts_holding(archive: &std::path::Path, entries: &[crate::scan::Entry], names: &[&str]) -> Vec<(Dict, crate::scan::Entry)> {
    use rayon::prelude::*;
    let mut out: Vec<(Dict, crate::scan::Entry)> = entries
        .par_iter()
        // the height tiles are many and hold nothing else
        .filter(|e| e.rtype.as_deref() == Some("txd") && e.name.as_deref() != Some("watersystemheightmap"))
        .filter_map(|e| {
            let r = Resource::load_sys(archive, e).ok()?;
            let textures: Vec<TexInfo> = texture::find_textures(&r).into_iter().filter(|t| names.iter().any(|w| t.name.eq_ignore_ascii_case(w))).collect();
            (!textures.is_empty()).then(|| (Dict { entry: e.id.clone(), off: e.off, textures }, e.clone()))
        })
        .collect();
    out.sort_by_key(|d| d.0.off);
    out
}

/// The map-wide ocean maps of levels.rpf's `water_quality_control`
/// dictionary. They cover the water grid's world box, row 0 north, at
/// 32 m (512 x 1056) or 16 m (1024 x 2112) per texel. Each entry is the
/// texture name and what the export's check found in it.
pub const OCEAN_MAPS: &[(&str, &str)] = &[
    ("mean_sea_level", "water surface height, metres = (v / 65535 - 0.5) * 1600 like the water height map; 0 = no water (land)"),
    ("water_quality_control", "red 255 on the sea and 0 on land and inland water; green about 40 on inland water only; blue and alpha constant"),
    ("ocean_wave_control", "red and green rise from 0 at the coast to 255 beyond 5 km out (waves calmed near the shore); blue varies"),
    ("ocean_roughness_control", "red and green centred on 128 near the coast (probably a direction), alpha above 0 within about 5 km of land; 128, 128, 0, 0 in the open sea"),
    ("ocean_wetness_control", "almost constant (150, 160, 255, 255)"),
];

/// Distance bands from land for the channel means, metres.
const OCEAN_BANDS: &[(f32, f32, &str)] = &[(0.0, 200.0, "sea, under 200 m from land"), (200.0, 1000.0, "sea, 200 m to 1 km"), (1000.0, 5000.0, "sea, 1 to 5 km"), (5000.0, f32::MAX, "sea, over 5 km")];

/// Distance in metres from each texel to the nearest `land` texel
/// (8-neighbour chamfer, two passes).
fn distance_to(land: &[bool], w: usize, h: usize, texel_m: f32) -> Vec<f32> {
    let mut d: Vec<f32> = land.iter().map(|&l| if l { 0.0 } else { f32::MAX }).collect();
    let (a, b) = (texel_m, texel_m * std::f32::consts::SQRT_2);
    for pass in 0..2 {
        let rows: Vec<usize> = if pass == 0 { (0..h).collect() } else { (0..h).rev().collect() };
        let cols: Vec<usize> = if pass == 0 { (0..w).collect() } else { (0..w).rev().collect() };
        let s: isize = if pass == 0 { -1 } else { 1 };
        for &y in &rows {
            for &x in &cols {
                let mut best = d[y * w + x];
                for (dx, dy, c) in [(s, 0, a), (0, s, a), (s, s, b), (-s, s, b)] {
                    let (nx, ny) = (x as isize + dx, y as isize + dy);
                    if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                        best = best.min(d[ny as usize * w + nx as usize] + c);
                    }
                }
                d[y * w + x] = best;
            }
        }
    }
    d
}

/// `ocean/`: [`OCEAN_MAPS`] placed on the water grid, as PNGs (16-bit for
/// the sea level) with `ocean/ocean.json`, checked against the water height
/// map and, when exported, the inland surfaces.
fn write_ocean(ws: &crate::export::Workspace, dir: &std::path::Path, hgt: &Layer, inland: Option<&[InlandTile]>) -> Result<Value> {
    std::fs::create_dir_all(dir)?;
    let lv = ws.resolve("levels")?;
    let ix = ws.load_index(&lv)?;
    let names: Vec<&str> = OCEAN_MAPS.iter().map(|m| m.0).collect();
    let dicts = dicts_holding(&lv.path, &ix.entries, &names);
    let wb = hgt.grid.world_box();
    let (gw, gh) = (wb[2] - wb[0], wb[3] - wb[1]);
    // the sea level map sets the land mask and the distance bands
    let (e, t) = find_tex(&dicts, "mean_sea_level").ok_or_else(|| anyhow!("mean_sea_level not found in levels"))?;
    let msl = load_texture(&lv.path, e, t)?;
    let (w, h) = (msl.w, msl.h);
    let texel_m = gw / w as f32;
    let land: Vec<bool> = (0..w * h).map(|i| msl.px[i] == 0.0).collect();
    let dist = distance_to(&land, w, h, texel_m);
    let sea_v = {
        let mut n: std::collections::HashMap<u32, usize> = Default::default();
        for (i, &v) in msl.px.iter().enumerate() {
            if !land[i] {
                *n.entry((v * 65535.0).round() as u32).or_default() += 1;
            }
        }
        n.into_iter().max_by_key(|x| x.1).map(|x| x.0).unwrap_or(0)
    };
    let is_sea = |i: usize| !land[i] && (msl.px[i] * 65535.0).round() as u32 == sea_v;
    let band = |i: usize| -> Option<&str> {
        if land[i] {
            return Some("land");
        }
        if !is_sea(i) {
            return Some("inland water");
        }
        OCEAN_BANDS.iter().find(|b| dist[i] >= b.0 && dist[i] < b.1).map(|b| b.2)
    };
    // land by the water height map: ground more than 5 m up at the texel centre
    let (m, _) = hgt.mosaic();
    let hb = hgt.grid.world_box();
    let mpt = hgt.grid.tile / hgt.px as f32;
    let ground = |x: f32, y: f32| -> Option<f32> {
        let (c, r) = (((x - hb[0]) / mpt).floor(), ((hb[3] - y) / mpt).floor());
        (c >= 0.0 && r >= 0.0 && (c as usize) < m.w && (r as usize) < m.h).then(|| height_m(m.at(c as usize, r as usize, 0)))
    };
    let centre = |c: usize, r: usize| (wb[0] + (c as f32 + 0.5) * texel_m, wb[3] - (r as f32 + 0.5) * texel_m);
    let (mut agree, mut n) = (0usize, 0usize);
    for r in 0..h {
        for c in 0..w {
            let (x, y) = centre(c, r);
            if let Some(g) = ground(x, y) {
                n += 1;
                agree += usize::from((g > 5.0) == land[r * w + c]);
            }
        }
    }
    let inland_check = inland.map(|tiles| {
        let mut err: Vec<f32> = Vec::new();
        let mut dry = 0;
        for t in tiles {
            let b = t.bbox();
            let (x, y) = ((b[0][0] + b[1][0]) / 2.0, (b[0][1] + b[1][1]) / 2.0);
            let (c, r) = (((x - wb[0]) / texel_m).floor(), ((wb[3] - y) / texel_m).floor());
            if c < 0.0 || r < 0.0 || c as usize >= w || r as usize >= h {
                continue;
            }
            let i = r as usize * w + c as usize;
            if land[i] {
                dry += 1;
            } else {
                err.push(height_m(msl.px[i]) - t.z_percentiles()[2]);
            }
        }
        let within = err.iter().filter(|e| e.abs() < 1.0).count();
        err.sort_by(f32::total_cmp);
        json!({"tiles": tiles.len(), "on_no_water_texel": dry, "compared": err.len(), "median_error_m": err.get(err.len() / 2), "within_1_m": within})
    });
    let mut maps = Vec::new();
    for &(name, about) in OCEAN_MAPS {
        let Some((e, t)) = find_tex(&dicts, name) else {
            maps.push(json!({"name": name, "error": "not found"}));
            continue;
        };
        let im = load_texture(&lv.path, e, t)?;
        let file = format!("{name}.png");
        if im.ch == 1 {
            write_png16(&dir.join(&file), &im)?;
        } else {
            write_rgba8(&dir.join(&file), &im)?;
        }
        // channel means per band, the map sampled at the sea level texel
        let (sx, sy) = (im.w / w, im.h / h);
        let mut sums: std::collections::BTreeMap<&str, (Vec<f64>, usize)> = Default::default();
        for r in 0..h {
            for c in 0..w {
                let Some(b) = band(r * w + c) else { continue };
                let s = sums.entry(b).or_insert((vec![0.0; im.ch], 0));
                for k in 0..im.ch {
                    s.0[k] += im.at(c * sx, r * sy, k) as f64;
                }
                s.1 += 1;
            }
        }
        let means: serde_json::Map<String, Value> = sums.iter().map(|(b, (s, k))| (b.to_string(), json!(s.iter().map(|v| ((v / *k as f64) * 10.0).round() / 10.0).collect::<Vec<_>>()))).collect();
        maps.push(json!({"name": name, "png": file, "dictionary": e.id, "width": t.width, "height": t.height, "format": t.format,
            "metres_per_texel": gw / t.width as f32, "aspect_matches_grid": (t.width as f32 / t.height as f32 - gw / gh).abs() < 0.01,
            "about": about, "channel_means_by_band": means}));
    }
    let check = json!({"land_agrees_with_height_map": agree as f32 / n.max(1) as f32, "texels_compared": n,
        "sea_value": sea_v, "sea_level_m": height_m(sea_v as f32 / 65535.0), "land_texels": land.iter().filter(|&&l| l).count(),
        "inland_water_texels": (0..w * h).filter(|&i| !land[i] && !is_sea(i)).count(), "inland_surfaces": inland_check});
    write_json(&dir.join("ocean.json"), &json!({
        "about": "Map-wide ocean maps from levels.rpf, placed on the water grid. Uses come from the names and the checks, not from shader code.",
        "world_box": {"min": [wb[0], wb[1]], "max": [wb[2], wb[3]]},
        "texel_centre": "(min.x + (col + 0.5) * metres_per_texel, max.y - (row + 0.5) * metres_per_texel), row 0 north",
        "png": "8-bit RGBA, or 16-bit greyscale for mean_sea_level (the stored value)",
        "channel_means_by_band": "per channel, 0 to 255; 0 to 1 (the stored value) for mean_sea_level",
        "bands": "land and inland water from mean_sea_level (0 = land; inland water = any value other than the sea's); sea bands by distance from land",
        "check": check, "maps": maps}))?;
    Ok(json!({"maps": maps.len(), "check": check}))
}

/// `detail/`: each of [`DETAIL_TEXTURES`] as an 8-bit RGBA PNG preview and
/// a DDS of the stored mip 0, plus `detail/detail.json`.
fn write_detail(ws: &crate::export::Workspace, dir: &std::path::Path) -> Result<Value> {
    std::fs::create_dir_all(dir)?;
    let mut rows = Vec::new();
    for archive in ["textures", "levels"] {
        let a = ws.resolve(archive)?;
        let ix = ws.load_index(&a)?;
        let names: Vec<&str> = DETAIL_TEXTURES.iter().filter(|d| d.0 == archive).map(|d| d.1).collect();
        let dicts = dicts_holding(&a.path, &ix.entries, &names);
        for &(_, name, about) in DETAIL_TEXTURES.iter().filter(|d| d.0 == archive) {
            let Some((e, t)) = find_tex(&dicts, name) else {
                rows.push(json!({"name": name, "archive": archive, "about": about, "error": "not found"}));
                continue;
            };
            let copies = dicts.iter().filter(|(d, _)| d.tex(name).is_some()).count();
            let res = (|| -> Result<Value> {
                let r = Resource::load(&a.path, e)?;
                let (fmt, lin) = texture::mip0_linear(&r, t, false)?;
                let rgba = texture::to_rgba(fmt, &lin, t.width, t.height)?;
                texture::save_png(&dir.join(format!("{name}.png")), &rgba, t.width, t.height)?;
                texture::save_dds(&dir.join(format!("{name}.dds")), t, fmt, &lin)?;
                Ok(json!({"png": format!("{name}.png"), "dds": format!("{name}.dds")}))
            })();
            let mut row = json!({"name": name, "archive": archive, "dictionary": e.id, "about": about, "width": t.width, "height": t.height,
                "format": t.format, "mips_stored": t.mips, "swizzle": t.swizzle, "dictionaries_holding_it": copies});
            match res {
                Ok(v) => {
                    row["png"] = v["png"].clone();
                    row["dds"] = v["dds"].clone();
                }
                Err(err) => row["error"] = json!(format!("{err:#}")),
            }
            rows.push(row);
        }
    }
    let written = rows.iter().filter(|r| r.get("png").is_some()).count();
    write_json(&dir.join("detail.json"), &json!({
        "about": "Water textures in the global dictionaries, probably bound at run time; the water drawables carry 4x4 placeholders for their detail slots. Uses come from the texture names and content.",
        "png": "8-bit RGBA preview of mip 0 (float and 16-bit formats clamped to 0..1)",
        "dds": "mip 0 as stored, untiled, DX10 header",
        "textures": rows}))?;
    Ok(json!({"textures": rows.len(), "written": written}))
}

/// The water height map under each small water body: the median of the
/// ground under its vertices.
pub fn check_pools(pools: &[PoolSurface], hgt: &Layer) -> Vec<Option<f32>> {
    let (m, _) = hgt.mosaic();
    let b = hgt.grid.world_box();
    let mpt = hgt.grid.tile / hgt.px as f32;
    let ground = |x: f32, y: f32| -> Option<f32> {
        let (c, r) = (((x - b[0]) / mpt).floor(), ((b[3] - y) / mpt).floor());
        (c >= 0.0 && r >= 0.0 && (c as usize) < m.w && (r as usize) < m.h).then(|| height_m(m.at(c as usize, r as usize, 0)))
    };
    pools
        .iter()
        .map(|p| {
            let mut g: Vec<f32> = p.positions.iter().filter_map(|v| ground(v[0], v[1])).collect();
            g.sort_by(f32::total_cmp);
            g.get(g.len() / 2).copied()
        })
        .collect()
}

/// `pools/pools.json` and `pools/pools.glb` (one node per kind, one
/// primitive per placement, world space).
fn write_pools(dir: &std::path::Path, pools: &[PoolSurface], unplaced: &Unplaced, ground: &[Option<f32>]) -> Result<Value> {
    use crate::gltf::{Material, NodeGroup, TriPrim};
    std::fs::create_dir_all(dir)?;
    let pct = |mut v: Vec<f32>| -> Value {
        v.sort_by(f32::total_cmp);
        let at = |q: f32| v.get(((v.len().max(1) - 1) as f32 * q) as usize).copied();
        json!({"p5": at(0.05), "median": at(0.5), "p95": at(0.95)})
    };
    let mut kinds: std::collections::BTreeMap<&str, Vec<usize>> = Default::default();
    for (i, p) in pools.iter().enumerate() {
        kinds.entry(p.kind).or_default().push(i);
    }
    let mut shaders: std::collections::BTreeMap<&str, usize> = Default::default();
    for p in pools {
        for s in &p.shaders {
            *shaders.entry(s.as_str()).or_default() += 1;
        }
    }
    let per_kind: serde_json::Map<String, Value> = kinds
        .iter()
        .map(|(k, ix)| {
            let names: std::collections::HashSet<&str> = ix.iter().map(|&i| pools[i].name.as_str()).collect();
            let above: Vec<f32> = ix.iter().filter_map(|&i| ground[i].map(|g| pools[i].surface_z() - g)).collect();
            (k.to_string(), json!({"placements": ix.len(), "drawables": names.len(), "area_m2": pct(ix.iter().map(|&i| pools[i].area()).collect()),
                "surface_above_ground_m": pct(above), "in_interiors": ix.iter().filter(|&&i| pools[i].interior.is_some()).count(),
                "named_but_not_placed": unplaced.get(k).copied().unwrap_or(0)}))
        })
        .collect();
    let rows: Vec<Value> = pools
        .iter()
        .zip(ground)
        .map(|(p, g)| {
            let mut v = json!({"name": p.name, "kind": p.kind, "map_file": p.map_file, "position": p.position, "rotation_world": p.rotation, "scale_xy_z": p.scale,
                "bbox": p.bbox(), "surface_z": p.surface_z(), "area_m2": p.area(), "ground_under_median": g, "shaders": p.shaders,
                "triangles": p.indices.len() / 3, "vertices": p.positions.len(), "uv0": !p.uvs.is_empty()});
            if let Some(i) = &p.interior {
                v["interior"] = json!({"archetype": crate::names::lookup(i.archetype, crate::names::Ctx::Field).unwrap_or_else(|| format!("{:08x}", i.archetype)),
                    "archetype_hash": format!("{:08x}", i.archetype), "instance_map": i.instance_map, "instance": i.instance,
                    "instance_position": i.instance_position, "instance_rotation": i.instance_rotation, "entity_set": i.entity_set});
            }
            v
        })
        .collect();
    let mut mats = Vec::new();
    let mut groups = Vec::new();
    for (k, ix) in &kinds {
        mats.push(Material { name: k.to_string(), base_color_uri: None, normal_uri: None,
            gltf: json!({"pbrMetallicRoughness": {"baseColorFactor": [0.2, 0.55, 0.75, 0.7], "metallicFactor": 0.0, "roughnessFactor": 0.05}, "alphaMode": "BLEND"}), extras: json!({"kind": k}) });
        let mi = mats.len() - 1;
        let prims = ix.iter().map(|&i| &pools[i]).map(|p| TriPrim {
            name: p.name.clone(), positions: p.positions.clone(), normals: None, tangents: None, uvs: if p.uvs.is_empty() { vec![] } else { vec![p.uvs.clone()] },
            colors: None, colors1: None, regions: None, tint: None, indices: p.indices.clone(), material: Some(mi),
            joints: None, weights: None, joints1: None, weights1: None, morphs: Vec::new(),
        }).collect();
        groups.push(NodeGroup { name: k.to_string(), prims });
    }
    crate::atomic::write(&dir.join("pools.glb"), crate::gltf::mesh_glb("pools", &groups, &mats, &[], json!({"coordinates": "world space"})))?;
    let rep = json!({"placements": pools.len(), "in_interiors": pools.iter().filter(|p| p.interior.is_some()).count(), "drawables": pools.iter().map(|p| p.name.as_str()).collect::<std::collections::HashSet<_>>().len(), "kinds": per_kind, "shaders": shaders});
    write_json(&dir.join("pools.json"), &json!({
        "about": "Pools, hot tubs, fountains, waterfalls, standing water and other small water bodies placed by the map files of levels.rpf, directly or inside a placed interior: the LOD 0 geometry drawn with a water shader, in world space. Rows with `interior` come from an interior instance: map_file is then the interior archetype file, position and rotation_world are the entity's in interior space, and the geometry and bbox are in world space through the instance (instance_position, then instance_rotation applied as stored). entity_set marks entities that only an entity set (an optional interior state) holds. An interior placed several times gives one row per instance. Kinds come from the drawable names; 'water effect' is spray and splash meshes (vfx_water), not a surface.",
        "coordinates": "RAGE world space, Z-up, metres; pools.glb is the same geometry in glTF Y-up (x, z, -y)",
        "ground": "ground_under_median: the water height map under the vertices (median); surface_z: median vertex height. Interiors can lie below the ground the height map holds",
        "summary": rep, "pools": rows}))?;
    Ok(rep)
}

/// `inland/inland.json` and `inland/inland.glb` (one node per kind, one
/// primitive per tile, world space).
fn write_inland(dir: &std::path::Path, tiles: &[InlandTile], check: &Value, set: &crate::ttpf::TtpfSet) -> Result<Value> {
    use crate::gltf::{Material, NodeGroup, TriPrim};
    use rayon::prelude::*;
    std::fs::create_dir_all(dir)?;
    // flow and water height textures as PNGs, one per store hash
    let tdir = dir.join("textures");
    std::fs::create_dir_all(&tdir)?;
    let hashes: std::collections::BTreeSet<u64> = tiles.iter().flat_map(|t| [t.flow, t.height_tex, t.tile_tex]).flatten().collect();
    let pngs: std::collections::HashMap<u64, Value> = hashes
        .into_par_iter()
        .map(|h| {
            let v = match set.get(h) {
                Some((store, tx)) => match crate::ttpf::write_png(store, tx, &tdir, false) {
                    Ok((p, st)) => {
                        let mut v = json!({"png": format!("textures/{}", p.file_name().unwrap_or_default().to_string_lossy()), "width": tx.width, "height": tx.height, "dxgi_format": tx.dxgi_format, "mip": st.mip});
                        // float textures: the PNG clamps to 0..1, so give the stored range
                        if let Ok((f, ..)) = store.data(tx).and_then(|d| crate::ttpf::decode_float_slice(tx, &d, 0, 0)) {
                            let (lo, hi) = f.iter().filter(|x| x.is_finite()).fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
                            if lo <= hi {
                                v["value_range"] = json!([lo, hi]);
                            }
                        }
                        v
                    }
                    Err(e) => json!({"hash64": format!("{h:016x}"), "width": tx.width, "height": tx.height, "dxgi_format": tx.dxgi_format, "error": e.to_string()}),
                },
                None => json!({"hash64": format!("{h:016x}"), "error": "not in the texture stores"}),
            };
            (h, v)
        })
        .collect();
    let tex_json = |h: Option<u64>| h.and_then(|h| pngs.get(&h)).cloned().unwrap_or(Value::Null);
    let hchecks: Vec<Option<(f32, Value)>> = tiles.par_iter().map(|t| height_texture_check(t, set)).collect();
    let kind_name = |k: &str| INLAND_KINDS.iter().find(|(c, _)| *c == k).map_or("unknown", |(_, n)| *n);
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for t in tiles {
        *counts.entry(t.kind).or_default() += 1;
    }
    let rows: Vec<Value> = tiles
        .iter()
        .zip(&hchecks)
        .map(|(t, hc)| {
            let z = t.z_percentiles();
            json!({"name": t.name, "kind": kind_name(t.kind), "kind_code": t.kind, "map_file": t.map_file,
                "position": t.position, "rotation_world": t.rotation, "scale_xy_z": t.scale, "bbox": t.bbox(),
                "surface_z": z[2], "z_percentiles_0_5_50_95_100": z, "model_bounds": t.bounds, "triangles": t.indices.len() / 3, "vertices": t.positions.len(),
                "shading": {"shader": t.shader["shader"], "flow_texture": tex_json(t.flow), "water_height_texture": tex_json(t.height_tex), "surface_map_texture": tex_json(t.tile_tex),
                    "water_height_scale_m": hc.as_ref().map(|h| h.0), "water_height_check": hc.as_ref().map(|h| h.1.clone()),
                    "ripple_scale": t.shader["constants"]["ripplescale"], "ripple_bumpiness": t.shader["constants"]["ripplebumpiness"], "uv0": !t.uvs.is_empty()},
                "shader": t.shader})
        })
        .collect();
    let mut mats = Vec::new();
    let mut groups = Vec::new();
    for k in counts.keys() {
        mats.push(Material { name: kind_name(k).to_string(), base_color_uri: None, normal_uri: None,
            gltf: json!({"pbrMetallicRoughness": {"baseColorFactor": [0.1, 0.35, 0.6, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.1}}), extras: json!({"kind_code": k}) });
        let mi = mats.len() - 1;
        let prims = tiles.iter().filter(|t| t.kind == *k).map(|t| TriPrim {
            name: t.name.clone(), positions: t.positions.clone(), normals: None, tangents: None, uvs: if t.uvs.is_empty() { vec![] } else { vec![t.uvs.clone()] },
            colors: None, colors1: None, regions: None, tint: None, indices: t.indices.clone(), material: Some(mi),
            joints: None, weights: None, joints1: None, weights1: None, morphs: Vec::new(),
        }).collect();
        groups.push(NodeGroup { name: kind_name(k).to_string(), prims });
    }
    crate::atomic::write(&dir.join("inland.glb"), crate::gltf::mesh_glb("inland_water", &groups, &mats, &[], json!({"coordinates": "world space"})))?;
    let kinds: serde_json::Map<String, Value> = counts.iter().map(|(k, n)| (kind_name(k).to_string(), json!(n))).collect();
    let still = tiles.iter().filter(|t| t.flow.and_then(|h| pngs.get(&h)).is_some_and(|v| v["width"] == 4)).count();
    let sloped: Vec<(f32, f32)> = hchecks.iter().flatten().filter(|(h, c)| *h > 0.05 && c["samples"].as_u64().unwrap_or(0) > 0).map(|(h, c)| (*h, c["median_error_m"].as_f64().unwrap_or(f64::MAX) as f32)).collect();
    let flat: Vec<&Value> = hchecks.iter().flatten().filter(|(h, _)| *h <= 0.05).map(|(_, c)| c).collect();
    let half = flat.iter().filter(|c| c["median_value"].as_f64().is_some_and(|v| (v - 0.5).abs() < 0.02)).count();
    // flat tiles whose texture under the surface is not 0.5: the scale of their values is not known
    let height_check = json!({"flat_tiles": flat.len(), "flat_tiles_reading_half": half, "sloped_tiles": sloped.len(),
        "sloped_median_error_under_10pct_of_scale": sloped.iter().filter(|(h, e)| *e <= 0.1 * h).count(),
        "sloped_median_error_under_3pct_of_scale": sloped.iter().filter(|(h, e)| *e <= 0.03 * h).count()});
    let rep = json!({"tiles": tiles.len(), "kinds": kinds, "water_height_texture_check": height_check, "flow_textures": tiles.iter().filter_map(|t| t.flow).collect::<std::collections::HashSet<_>>().len(), "tiles_with_placeholder_flow": still, "tile_textures": tiles.iter().filter_map(|t| t.tile_tex).collect::<std::collections::HashSet<_>>().len(), "check_against_height_map": check});
    write_json(&dir.join("inland.json"), &json!({
        "about": "Inland water surfaces (rivers, canals, creeks, reservoirs, ponds): the map's water drawables placed in world space, LOD 0. One tile per placement, at most 64 m across.",
        "coordinates": "RAGE world space, Z-up, metres; inland.glb is the same geometry in glTF Y-up (x, z, -y)",
        "shading": "Every tile draws with the water_micro_geom shader. Its flow texture (BC3: flow direction in red and green around 128; 4 x 4 placeholder on still tiles) and water height texture (R16 float, 0 to 1: surface height = origin z + (2 v - 1) * water_height_scale_m, where the scale is the top of the drawable's stored bbox (model_bounds max z, times the z scale), equal to the largest distance of a surface vertex from the origin's height on every sloped tile and under 1 cm on flat ones, whose varying values therefore span only millimetres; the bbox bottom is minus the scale and the unnamed constant 02596957, the depth of the skirt below the surface; 0 where there is no water, a mask that covers the mesh and reaches a little under the banks) cover the tile's 64 m square: on the surface vertices UV0 is world x / 64 and -world y / 64 (u east, v south). Skirt vertices below the surface carry other UVs. `shader` lists every texture and constant of the shader; hex keys are parameter names not yet known. The flags useflowmap, useheighttexture and usesurfacemap hold the integer 1, which reads as 1.4e-45 as a float.",
        "surface_map_texture": "watersurfacemaptexture (BC7), the tile's 64 m square like the flow texture, usually 256 x 256 (4 x 4 placeholder on 6 tiles). Alpha is the mesh footprint, green the part where the surface lies above the ground (visible water), and red and blue rise with flow speed (probably foam or turbulence). The constant usesurfacemap is 1 on every tile except the 6 placeholders, like useflowmap and useheighttexture.",
        "summary": rep, "tiles": rows}))?;
    Ok(rep)
}

fn summary(rep: &Value) -> Value {
    let mut r = rep.clone();
    for k in ["fog_scattering", "fog_extinction", "height", "shore"] {
        if let Some(o) = r[k].as_object_mut() {
            o.remove("tile_records");
            o.remove("grid");
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_kinds_from_names() {
        assert_eq!(pool_kind("nb_11_pool_water_01"), Some("pool"));
        assert_eq!(pool_kind("nb_01_pool_water_16_hottub"), Some("hot tub"));
        assert_eq!(pool_kind("lo_lr4_01_standing_water_26"), Some("standing water"));
        assert_eq!(pool_kind("vi_01_am_e_fountain_e_cln"), Some("fountain"));
        assert_eq!(pool_kind("ab_01_vfx_waterfall_02"), Some("waterfall"));
        assert_eq!(pool_kind("vi_03_vfx_water_04"), Some("water effect"));
        assert_eq!(pool_kind("fh_01_irrigditch_02_water"), Some("ditch"));
        // inland surfaces, map tiles, plants and decals are not small water bodies
        assert_eq!(pool_kind("lo_rvr_01_water_lo_rvr_01a"), None);
        assert_eq!(pool_kind("maptile_3d01_water_std"), None);
        assert_eq!(pool_kind("n10_w05_13_waterveg"), None);
        assert_eq!(pool_kind("nb_13_vfx_water_decal_01"), None);
        assert_eq!(pool_kind("lo_cnl_01_water_pool_d_slod"), None);
    }

    #[test]
    fn inland_kinds_from_names() {
        assert_eq!(inland_kind("lo_rvr_01_water_lo_rvr_01a"), Some("rvr"));
        assert_eq!(inland_kind("lo_cnl_01_water_fh_rsr_01m_001"), Some("rsr"));
        assert_eq!(inland_kind("nb_cnl_01_water_nb_glf_01a"), Some("glf"));
        assert_eq!(inland_kind("lo_rvr_04_water_nb_mill_01"), Some("mill"));
        // LOD models, pools, scum decals and props are not surfaces
        assert_eq!(inland_kind("vr_01_water_lo_rvr_01a_01_lod"), None);
        assert_eq!(inland_kind("lo_cnl_01_water_pool_d_slod"), None);
        assert_eq!(inland_kind("ap_14_pool_water_01_01"), None);
        assert_eq!(inland_kind("ap_02f_am_e_water_scum_algae_gen_a_30_cln"), None);
        assert_eq!(inland_kind("prop_am_water_raine_bottle_01a"), None);
        assert_eq!(inland_kind("vk_04_water_treatment_pump_01"), None);
    }

    const GRID: &str = "<GridAtlasInfo><clientSystemName>g</clientSystemName><tileTextureNamePrefix>g-</tileTextureNamePrefix><tileTextureNameSuffix>t</tileTextureNameSuffix><globalFallbackTextureName>t_gfb</globalFallbackTextureName><origin x=\"-5120.0\" y=\"7680.0\" z=\"0.0\" /><width value=\"16384\" /><height value=\"33792\" /><tileSize value=\"512\" /></GridAtlasInfo>";

    #[test]
    fn grid_geometry() {
        let g = Grid::from_node(&timecycle::parse_xml(GRID).unwrap()).unwrap();
        assert_eq!((g.cols(), g.rows(), g.cells()), (32, 66, 2112));
        assert_eq!(g.world_box(), [-13312.0, -9216.0, 3072.0, 24576.0]);
        // cell 0 is the north-west corner, 33 is one row south and one east
        assert_eq!(g.cell_box(0), [-13312.0, 24064.0, -12800.0, 24576.0]);
        assert_eq!(g.cell_box(33), [-12800.0, 23552.0, -12288.0, 24064.0]);
        assert_eq!(g.cell_at(-12500.0, 23600.0), Some(33));
        assert_eq!(g.cell_at(3072.5, 0.0), None);
    }

    #[test]
    fn decimal_string_order() {
        let o = string_order(2112);
        assert_eq!(&o[..6], &[0, 1, 10, 100, 1000, 1001]);
        assert_eq!(o[o.iter().position(|&c| c == 2111).unwrap() + 1], 212);
        assert_eq!(*o.last().unwrap(), 999);
    }

    #[test]
    fn alignment_prefers_matches_and_runs() {
        // 3 tiles, cells 0..6; tile i matches cell 2 + i; a flat tile (1)
        // follows the run rather than jumping
        let order: Vec<usize> = (0..6).collect();
        let cost = |i: usize, c: usize| if i == 1 { 0.5 } else if c == 2 + i { 0.0 } else { 1.0 };
        let w = Weights { gap: 0.01, drop: 10.0, jump: 0.0, cap: 0.0 };
        let p = align(3, &order, w, &cost);
        assert_eq!(p.iter().map(|x| x.unwrap().cell).collect::<Vec<_>>(), vec![2, 3, 4]);
        assert_eq!(p[0].unwrap().cost, 0.0);
        assert_eq!(p[0].unwrap().runner_up, 1.0);
        // a tile that matches nothing is left out when dropping is cheaper
        let cost = |i: usize, c: usize| if i == 1 { 5.0 } else if c == i { 0.0 } else { 1.0 };
        let p = align(3, &order, Weights { drop: 2.0, ..w }, &cost);
        assert_eq!(p.iter().map(|x| x.map(|x| x.cell)).collect::<Vec<_>>(), vec![Some(0), None, Some(2)]);
    }

    #[test]
    fn jump_term_keeps_runs_contiguous() {
        // order "1", "10", "2", "20": flat tiles that match anywhere; with
        // the jump term the run is 1, 2 (not 1, 10 or 10, 20)
        let order = vec![1, 10, 2, 20];
        let p = align(2, &order, Weights { gap: 0.0, drop: 5.0, jump: 1.0, cap: 3.0 }, &|_, _| 0.0);
        assert_eq!(p.iter().map(|x| x.unwrap().cell).collect::<Vec<_>>(), vec![1, 2]);
        // and an extra tile that fits nowhere after the run is dropped
        let p = align(3, &order, Weights { gap: 0.0, drop: 0.5, jump: 1.0, cap: 3.0 }, &|i, _| if i == 2 { 4.0 } else { 0.0 });
        assert_eq!(p.iter().map(|x| x.map(|x| x.cell)).collect::<Vec<_>>(), vec![Some(1), Some(2), None]);
    }

    #[test]
    fn shore_channel_b() {
        assert_eq!(shore_b(0), (0, false, 0.0));
        assert_eq!(shore_b(64 + 63), (1, false, 1.0));
        assert_eq!(shore_b(192 + 31), (3, false, 1.0));
        assert_eq!(shore_b(192 + 32), (3, true, 0.0));
    }

    #[test]
    fn height_scale() {
        assert_eq!(height_m(0.5), 0.0);
        assert_eq!(height_m(0.3125), -300.0);
        assert!((height_m(0.5011) - 1.76).abs() < 1e-3);
    }

    #[test]
    fn water_keys_only() {
        let d = timecycle::Decoded {
            cycles: vec![json!({"name": "Sunny", "regions": {"GLOBAL": {"water_ocean_fetch": [100.0], "fog_density": [0.1]}}})],
            modifiers: vec![json!({"name": "a", "vars": {"water_ocean_fetch": {"value": 200.0}}}), json!({"name": "b", "vars": {"fog_density": {"value": 1.0}}})],
            ..Default::default()
        };
        let k = timecycle_keys(&d);
        assert_eq!(k["cycles"][0]["regions"]["GLOBAL"], json!({"water_ocean_fetch": [100.0]}));
        assert_eq!(k["modifiers"].as_array().unwrap().len(), 1);
        assert_eq!(k["variables"], json!(["water_ocean_fetch"]));
    }

    #[test]
    fn block_average() {
        let im = Image { w: 4, h: 2, ch: 1, px: vec![0.0, 2.0, 4.0, 6.0, 2.0, 4.0, 6.0, 8.0] };
        assert_eq!(im.block(0, 0, 4, 2, 2), vec![1.0, 5.0, 3.0, 7.0]);
        assert_eq!(im.shrink(2).px, vec![2.0, 6.0]);
    }
}

/// The water grids' global fallbacks for a viewer: the water height map
/// (metres) and the fog atlas's scattering and extinction (raw values),
/// each covering the whole grid, row 0 north.
#[derive(Debug, Clone)]
pub struct ViewMaps {
    /// The grids' shared box: min x, min y, max x, max y.
    pub world_box: [f32; 4],
    /// Heights (m), one channel.
    pub height: Image,
    /// Scattering and extinction, three channels each.
    pub scattering: Image,
    pub extinction: Image,
}

const VIEW_MAGIC: &[u8; 8] = b"SXWVIEW1";

impl ViewMaps {
    fn to_bytes(&self) -> Vec<u8> {
        let mut b = VIEW_MAGIC.to_vec();
        self.world_box.iter().for_each(|v| b.extend(v.to_le_bytes()));
        for im in [&self.height, &self.scattering, &self.extinction] {
            for v in [im.w, im.h, im.ch] {
                b.extend((v as u32).to_le_bytes());
            }
            im.px.iter().for_each(|v| b.extend(v.to_le_bytes()));
        }
        b
    }

    fn from_bytes(b: &[u8]) -> Option<ViewMaps> {
        let mut at = VIEW_MAGIC.len();
        if b.get(..at)? != VIEW_MAGIC {
            return None;
        }
        let f32_at = |at: &mut usize| -> Option<f32> {
            let v = f32::from_le_bytes(b.get(*at..*at + 4)?.try_into().ok()?);
            *at += 4;
            Some(v)
        };
        let world_box = [f32_at(&mut at)?, f32_at(&mut at)?, f32_at(&mut at)?, f32_at(&mut at)?];
        let mut ims = Vec::new();
        for _ in 0..3 {
            let mut u = || -> Option<usize> {
                let v = u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize;
                at += 4;
                Some(v)
            };
            let (w, h, ch) = (u()?, u()?, u()?);
            let n = w.checked_mul(h)?.checked_mul(ch)?;
            let raw = b.get(at..at + n * 4)?;
            at += n * 4;
            ims.push(Image { w, h, ch, px: raw.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect() });
        }
        let extinction = ims.pop()?;
        let scattering = ims.pop()?;
        let height = ims.pop()?;
        Some(ViewMaps { world_box, height, scattering, extinction })
    }

    /// Loads the maps, from the work folder's cache when it is there (the
    /// fallback textures are found by name, else among the unnamed
    /// dictionaries, which takes a while once).
    pub fn load(ws: &crate::export::Workspace) -> Result<ViewMaps> {
        let cache = ws.work.join("cache").join("water_view_v1.bin");
        if let Some(m) = std::fs::read(&cache).ok().and_then(|b| ViewMaps::from_bytes(&b)) {
            return Ok(m);
        }
        let common = ws.resolve("common")?;
        let cix = ws.load_index(&common)?;
        let docs = xml_docs(&common.path, &cix.entries);
        let grids = grids(&docs);
        let grid = |client: &str| grids.iter().map(|g| g.1.clone()).find(|g| g.client.eq_ignore_ascii_case(client)).ok_or_else(|| anyhow!("no GridAtlasInfo for {client} in common"));
        let (fog1, fog2, hgt) = (grid("waterfogatlas")?, grid("waterfogatlas2")?, grid("WaterSystemHeightMap")?);
        let fallback = |archive: &str, name: &str| -> Result<Image> {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            // dictionaries named after the texture first, the unnamed ones only if needed
            let named: Vec<crate::scan::Entry> = ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("txd") && e.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name))).cloned().collect();
            for e in &named {
                if let Ok(r) = Resource::load_sys(&a.path, e) {
                    if let Some(t) = texture::find_textures(&r).into_iter().find(|t| t.name.eq_ignore_ascii_case(name)) {
                        return load_texture(&a.path, e, &t);
                    }
                }
            }
            let dicts = find_dicts(&a.path, &ix.entries, &[name]);
            let (e, t) = find_tex(&dicts, name).ok_or_else(|| anyhow!("{name} not found in {archive}"))?;
            load_texture(&a.path, e, t)
        };
        let mut height = fallback("levels", &hgt.fallback)?;
        height.px.iter_mut().for_each(|v| *v = height_m(*v));
        if height.ch > 1 {
            height = Image { w: height.w, h: height.h, ch: 1, px: height.px.iter().step_by(height.ch).copied().collect() };
        }
        let rgb = |im: Image| -> Image {
            if im.ch == 3 {
                return im;
            }
            let px = im.px.chunks(im.ch).flat_map(|c| [c[0], *c.get(1).unwrap_or(&c[0]), *c.get(2).unwrap_or(&c[0])]).collect();
            Image { w: im.w, h: im.h, ch: 3, px }
        };
        let scattering = rgb(fallback("textures", &fog1.fallback)?);
        let extinction = rgb(fallback("textures", &fog2.fallback)?);
        let m = ViewMaps { world_box: hgt.world_box(), height, scattering, extinction };
        if let Some(d) = cache.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = crate::atomic::write(&cache, m.to_bytes());
        Ok(m)
    }
}

#[cfg(test)]
mod view_maps_tests {
    use super::*;

    #[test]
    fn view_maps_round_trip() {
        let im = |ch: usize| Image { w: 2, h: 3, ch, px: (0..6 * ch).map(|v| v as f32 * 0.5).collect() };
        let m = ViewMaps { world_box: [-1.0, -2.0, 3.0, 4.0], height: im(1), scattering: im(3), extinction: im(3) };
        let back = ViewMaps::from_bytes(&m.to_bytes()).unwrap();
        assert_eq!(back.world_box, m.world_box);
        assert_eq!(back.height.px, m.height.px);
        assert_eq!(back.extinction.px, m.extinction.px);
        assert!(ViewMaps::from_bytes(b"nope").is_none());
    }
}
