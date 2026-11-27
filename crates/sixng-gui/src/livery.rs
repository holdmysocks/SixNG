//! Custom liveries: stickers the user places on a vehicle by hand.
//!
//! A vehicle's panels are numbered: every vertex carries a **sticker zone**
//! (the fourth byte of the model's second colour stream, GLB `_COLOR1`; on
//! vehicle glass the first byte of `_COLOR0`), and
//! the game's two-layer vehicle shader draws a sticker on the zones it is
//! given where a transformed mesh UV lies between 0 and 1:
//! `u = U*a + V*b + ou`, `v = U*c + V*d + ov`. The viewer draws stickers the
//! same way (`shader.wgsl`), from a list of placements per vehicle.
//!
//! A placement here is given in terms a person can set: the zones, the UV
//! set, the sticker's centre in that UV set, its width and height in metres
//! and a rotation. The six numbers of the shader come from those and from
//! how the chosen zones' UVs lie on the model (`ZoneSet`): the sticker is
//! laid upright on the panel as seen from outside.
//!
//! Zones are numbered per model. An assembled vehicle has several (the cab,
//! a fitted box or bed, other fitted parts), so a placement names its model
//! as well (`Vertex::zone[1]`; "" = the vehicle itself).
//!
//! The game's own liveries (brands) are not drawn: the rule that turns a
//! brand's layout numbers into a place on the panel is not decoded.
//!
//! The panel that edits the placements is in `livery_ui`.

use crate::glb;
use eframe::egui_wgpu::{self, wgpu};
use glam::{Mat4, Vec2, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Stickers drawn per vehicle (the shader walks the list per pixel).
pub const MAX_STICKERS: usize = 64;
/// Distinct pictures per vehicle (layers of one texture array).
pub const MAX_IMAGES: usize = 32;
/// Side of a picture's layer in the texture array (pictures are resampled).
pub const LAYER: u32 = 1024;

/// Where a sticker's picture comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Picture {
    /// A sticker texture of the game: its 64-bit id (16 hex digits).
    Game(String),
    /// A PNG file of the user's.
    File(PathBuf),
}

impl Picture {
    pub fn key(&self) -> String {
        match self {
            Picture::Game(id) => format!("game:{id}"),
            Picture::File(p) => format!("file:{}", p.display()),
        }
    }
    pub fn label(&self) -> String {
        match self {
            Picture::Game(id) => id.clone(),
            Picture::File(p) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        }
    }
}

fn one() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}
fn white3() -> [[u8; 3]; 3] {
    [[255; 3]; 3]
}

/// One placed sticker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sticker {
    #[serde(default)]
    pub name: String,
    pub picture: Picture,
    /// The model whose zones are meant: "" = the vehicle itself, else the
    /// name of a fitted part (a box, a bed).
    #[serde(default)]
    pub model: String,
    pub zones: Vec<u8>,
    /// UV set: 0 = first, 1 = second. The game's own placements (`game_uv`)
    /// may also use 2 and 3 (its layouts' UV mode minus one).
    #[serde(default)]
    pub uv_set: u8,
    /// Centre in the UV set.
    pub centre: [f32; 2],
    /// Width and height on the panel, metres.
    pub size: [f32; 2],
    /// Degrees, anticlockwise as seen from outside.
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub mirror: bool,
    /// Colour mask: the picture's red, green and blue each take one of
    /// `colours`. Otherwise the picture's own colours are used, times the
    /// first colour.
    #[serde(default)]
    pub mask: bool,
    /// sRGB.
    #[serde(default = "white3")]
    pub colours: [[u8; 3]; 3],
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default = "yes")]
    pub visible: bool,
    /// The six numbers the shader used when this was saved (a, b, c, d, ou,
    /// ov): written for reference, recomputed from the model on load.
    #[serde(default)]
    pub shader_uv: Option<[f32; 6]>,
    /// A sticker of one of the game's liveries: the game's own transform
    /// (a, b, c, d, ou, ov, from its layout; see
    /// `sixng_core::vehstickers::game_uv`), used as it is instead of the
    /// placement above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_uv: Option<[f32; 6]>,
    /// Not placed by hand yet: it is fitted again when its zones change.
    #[serde(skip)]
    pub fresh: bool,
}

impl Sticker {
    pub fn new(picture: Picture) -> Sticker {
        Sticker { name: String::new(), picture, model: String::new(), zones: Vec::new(), uv_set: 1, centre: [0.5, 0.5], size: [1.0, 1.0], rotation: 0.0, mirror: false, mask: false, colours: white3(), opacity: 1.0, visible: true, shader_uv: None, game_uv: None, fresh: true }
    }
}

/// The placements of one vehicle, as saved.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Livery {
    pub format: u32,
    pub vehicle: String,
    pub stickers: Vec<Sticker>,
}

/// File name part for a vehicle's livery: its model name without the LOD
/// suffix characters a file system dislikes.
pub fn vehicle_key(source: &str) -> String {
    let s = source.split("#parts=").next().unwrap_or(source);
    let s = s.rsplit(['/', '\\']).next().unwrap_or(s);
    let s = s.trim_start_matches(['~', '^']).trim_end_matches('$').trim_end_matches(".glb");
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c.to_ascii_lowercase() } else { '_' }).collect()
}

pub fn file_for(work: &Path, vehicle: &str) -> PathBuf {
    work.join("liveries").join(format!("{vehicle}.json"))
}

pub fn load(path: &Path) -> anyhow::Result<Livery> {
    let l: Livery = serde_json::from_slice(&std::fs::read(path)?)?;
    Ok(l)
}

pub fn save(path: &Path, l: &Livery) -> anyhow::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    sixng_core::atomic::write(path, serde_json::to_vec_pretty(l)?)?;
    Ok(())
}

/// The undo history of a vehicle's livery.
///
/// A step is what changed between two moments of rest: a drag, a turn of
/// the wheel or a run of typing is one step, taken when the pointer is up
/// and nothing has changed for `History::REST` seconds.
#[derive(Default)]
pub struct History {
    /// The vehicle the history belongs to; another vehicle starts anew.
    tag: String,
    /// The livery at the last moment of rest.
    base: Livery,
    /// As it was a frame ago, and when it last changed.
    seen: Livery,
    changed_at: f64,
    undo: Vec<Livery>,
    redo: Vec<Livery>,
}

impl History {
    /// Seconds without a change before a step is taken.
    pub const REST: f64 = 0.35;
    /// Steps kept.
    pub const STEPS: usize = 200;

    /// What counts as the livery: not the numbers the viewer writes back
    /// for reference (`shader_uv` changes a frame after the placement).
    fn plain(l: &Livery) -> Livery {
        let mut l = l.clone();
        for s in &mut l.stickers {
            s.shader_uv = None;
        }
        l
    }

    /// Forget everything; `now` is the start.
    pub fn reset(&mut self, tag: &str, now: &Livery) {
        *self = History { tag: tag.to_string(), base: Self::plain(now), seen: Self::plain(now), ..Default::default() };
    }

    /// Call every frame. `t`: the time, seconds; `held`: a pointer button
    /// is down (a drag is not over). True while a change waits to become
    /// a step (call again within `REST`).
    pub fn track(&mut self, tag: &str, now: &Livery, t: f64, held: bool) -> bool {
        if self.tag != tag {
            self.reset(tag, now);
            return false;
        }
        let now = Self::plain(now);
        if now != self.seen {
            self.seen = now;
            self.changed_at = t;
        }
        if self.seen == self.base {
            return false;
        }
        if held || t - self.changed_at < Self::REST {
            return true;
        }
        self.step();
        false
    }

    /// Take the pending change as a step now.
    fn step(&mut self) {
        if self.seen != self.base {
            self.undo.push(std::mem::replace(&mut self.base, self.seen.clone()));
            if self.undo.len() > Self::STEPS {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.seen != self.base
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty() && self.seen == self.base
    }

    /// Go back one step. True when `now` changed.
    pub fn undo(&mut self, now: &mut Livery) -> bool {
        self.seen = Self::plain(now);
        self.step();
        let Some(back) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.base, back));
        self.seen = self.base.clone();
        *now = self.base.clone();
        true
    }

    /// Go forward one step again. Nothing after a new change.
    pub fn redo(&mut self, now: &mut Livery) -> bool {
        if Self::plain(now) != self.base {
            return false;
        }
        let Some(on) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.base, on));
        self.seen = self.base.clone();
        *now = self.base.clone();
        true
    }
}

/// One sticker zone of a loaded model.
#[derive(Clone, Debug, Default)]
pub struct Zone {
    pub slot: u8,
    pub zone: u8,
    pub triangles: usize,
    /// Surface area, square metres.
    pub area: f32,
    /// Area-weighted mean normal and centre (viewer space, Y up).
    pub normal: Vec3,
    pub centre: Vec3,
    /// Per UV set: bounding box, and how the UVs lie on the model: the
    /// model-space step per unit of U and of V (area-weighted mean).
    pub uv_lo: [Vec2; 2],
    pub uv_hi: [Vec2; 2],
    pub du: [Vec3; 2],
    pub dv: [Vec3; 2],
    /// Area that has a usable UV mapping in the set.
    pub uv_area: [f32; 2],
    /// Area-weighted sums of the triangles' UV centres and of their
    /// squares: where the bulk of the zone lies in UV space (a few faces
    /// unwrapped elsewhere stretch the box, not these).
    pub uv_sum: [Vec2; 2],
    pub uv_sq: [Vec2; 2],
    /// Per UV set: the zone's share of each UV island it lies on.
    pub islands: [Vec<Island>; 2],
}

/// A zone's share of one UV island: a patch of faces that hang together in
/// the UV set. `id` numbers the island within the model slot and UV set, so
/// neighbouring zones on one island (a wing and a door unwrapped as one
/// piece) share it.
#[derive(Clone, Copy, Debug)]
pub struct Island {
    pub id: u32,
    /// Mapped surface area, square metres.
    pub area: f32,
    pub lo: Vec2,
    pub hi: Vec2,
    /// Area-weighted sums, as `Zone::uv_sum`, `uv_sq`, `du`, `dv`.
    pub sum: Vec2,
    pub sq: Vec2,
    pub du: Vec3,
    pub dv: Vec3,
}

impl Island {
    fn new(id: u32) -> Island {
        Island { id, area: 0.0, lo: Vec2::splat(f32::MAX), hi: Vec2::splat(f32::MIN), sum: Vec2::ZERO, sq: Vec2::ZERO, du: Vec3::ZERO, dv: Vec3::ZERO }
    }

    fn add(&mut self, o: &Island) {
        self.area += o.area;
        self.lo = self.lo.min(o.lo);
        self.hi = self.hi.max(o.hi);
        self.sum += o.sum;
        self.sq += o.sq;
        self.du += o.du;
        self.dv += o.dv;
    }
}

/// UV islands of a model: faces that share a UV point (in the same slot and
/// UV set) are on one island.
#[derive(Default)]
struct Islands {
    parent: Vec<u32>,
    nodes: std::collections::HashMap<(u8, u8, i32, i32), u32>,
}

impl Islands {
    fn node(&mut self, slot: u8, set: usize, uv: Vec2) -> u32 {
        // a 4096th of the texture: split vertices of a seam hold the same
        // numbers, and separate islands lie further apart than this
        let key = (slot, set as u8, (uv.x * 4096.0).round() as i32, (uv.y * 4096.0).round() as i32);
        let next = self.parent.len() as u32;
        let n = *self.nodes.entry(key).or_insert(next);
        if n == next {
            self.parent.push(next);
        }
        n
    }

    fn find(&mut self, mut n: u32) -> u32 {
        while self.parent[n as usize] != n {
            let up = self.parent[self.parent[n as usize] as usize];
            self.parent[n as usize] = up;
            n = up;
        }
        n
    }

    fn join(&mut self, a: u32, b: u32) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.parent[a.max(b) as usize] = a.min(b);
        }
    }
}

impl Zone {
    /// Which way the zone faces, in words.
    pub fn side(&self) -> &'static str {
        let n = self.normal.normalize_or_zero();
        let a = n.abs();
        if a.max_element() < 0.5 {
            "mixed"
        } else if a.y >= a.x && a.y >= a.z {
            if n.y > 0.0 { "top" } else { "underside" }
        } else if a.x >= a.z {
            // the viewer shows RAGE's +X (the vehicle's right) as +X
            if n.x > 0.0 { "right" } else { "left" }
        } else if n.z > 0.0 {
            "rear"
        } else {
            "front"
        }
    }
}

/// The zones of a model, by (slot, zone), and the fitted part names by slot.
#[derive(Clone, Debug, Default)]
pub struct Zones {
    pub zones: BTreeMap<(u8, u8), Zone>,
    /// Slot -> model name ("" for slot 0).
    pub models: Vec<String>,
}

impl Zones {
    pub fn slot_of(&self, model: &str) -> Option<u8> {
        self.models.iter().position(|m| m == model).map(|i| i as u8)
    }

    /// Survey a loaded model. `parts`: the fitted part names in slot order
    /// (slot 1 first).
    pub fn of(m: &glb::Model, parts: &[String]) -> Zones {
        let mut zones: BTreeMap<(u8, u8), Zone> = BTreeMap::new();
        // first the UV islands, over every face that can take a sticker
        // (also the faces on a border between two zones: they tie the
        // islands of neighbouring zones together)
        let mut isl = Islands::default();
        for me in &m.meshes {
            for t in me.indices.chunks_exact(3) {
                let v = [&me.verts[t[0] as usize], &me.verts[t[1] as usize], &me.verts[t[2] as usize]];
                if v[0].zone[2] == 0 {
                    continue;
                }
                for set in 0..2 {
                    let slot = v[0].zone[1];
                    let n = v.map(|q| isl.node(slot, set, Vec2::from(if set == 0 { q.uv0 } else { q.uv1 })));
                    isl.join(n[0], n[1]);
                    isl.join(n[0], n[2]);
                }
            }
        }
        for me in &m.meshes {
            for t in me.indices.chunks_exact(3) {
                let v = [&me.verts[t[0] as usize], &me.verts[t[1] as usize], &me.verts[t[2] as usize]];
                let key = (v[0].zone[1], v[0].zone[0]);
                if v[0].zone[2] == 0 || v[1].zone[0] != key.1 || v[2].zone[0] != key.1 {
                    continue;
                }
                let p = v.map(|v| Vec3::from(v.pos));
                let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
                let n = e1.cross(e2);
                let area = n.length() * 0.5;
                if !(area > 0.0) {
                    continue;
                }
                let z = zones.entry(key).or_insert_with(|| Zone { slot: key.0, zone: key.1, uv_lo: [Vec2::splat(f32::MAX); 2], uv_hi: [Vec2::splat(f32::MIN); 2], ..Default::default() });
                z.triangles += 1;
                z.area += area;
                z.normal += n * 0.5;
                z.centre += (p[0] + p[1] + p[2]) / 3.0 * area;
                for set in 0..2 {
                    let uv = v.map(|v| Vec2::from(if set == 0 { v.uv0 } else { v.uv1 }));
                    let (d1, d2) = (uv[1] - uv[0], uv[2] - uv[0]);
                    let det = d1.x * d2.y - d1.y * d2.x;
                    if det.abs() < 1e-12 {
                        continue;
                    }
                    for q in uv {
                        z.uv_lo[set] = z.uv_lo[set].min(q);
                        z.uv_hi[set] = z.uv_hi[set].max(q);
                    }
                    // [e1 e2] = [du dv] * [d1 d2]
                    let (du, dv) = ((e1 * d2.y - e2 * d1.y) / det * area, (e2 * d1.x - e1 * d2.x) / det * area);
                    z.du[set] += du;
                    z.dv[set] += dv;
                    z.uv_area[set] += area;
                    let c = (uv[0] + uv[1] + uv[2]) / 3.0;
                    // a triangle's own spread about its centre: (sum of squared corner offsets) / 12
                    let own = ((uv[0] - c) * (uv[0] - c) + (uv[1] - c) * (uv[1] - c) + (uv[2] - c) * (uv[2] - c)) / 12.0;
                    z.uv_sum[set] += c * area;
                    z.uv_sq[set] += (c * c + own) * area;
                    let n = isl.node(key.0, set, uv[0]);
                    let id = isl.find(n);
                    let at = match z.islands[set].iter().position(|i| i.id == id) {
                        Some(at) => at,
                        None => {
                            z.islands[set].push(Island::new(id));
                            z.islands[set].len() - 1
                        }
                    };
                    z.islands[set][at].add(&Island { id, area, lo: uv[0].min(uv[1]).min(uv[2]), hi: uv[0].max(uv[1]).max(uv[2]), sum: c * area, sq: (c * c + own) * area, du, dv });
                }
            }
        }
        for z in zones.values_mut() {
            z.centre /= z.area.max(1e-12);
            for set in 0..2 {
                let a = z.uv_area[set].max(1e-12);
                z.du[set] /= a;
                z.dv[set] /= a;
            }
        }
        let mut models = vec![String::new()];
        models.extend(parts.iter().cloned());
        Zones { zones, models }
    }

    /// The chosen zones of a placement taken together.
    pub fn set(&self, slot: u8, zones: &[u8], uv_set: usize) -> Option<ZoneSet> {
        let mut s = ZoneSet { uv_lo: Vec2::splat(f32::MAX), uv_hi: Vec2::splat(f32::MIN), ..Default::default() };
        let mut w = 0.0;
        // The UV box a sticker is fitted to is that of the zones that fill
        // their own box (panels). A zone that is a thin frame around other
        // things (a wall's rails) has a box far larger than itself and
        // would put the middle somewhere off the panels.
        let fill = |z: &Zone| {
            let e = z.uv_hi[uv_set] - z.uv_lo[uv_set];
            z.uv_area[uv_set] / (e.x * z.du[uv_set].length() * e.y * z.dv[uv_set].length()).max(1e-9)
        };
        let chosen: Vec<&Zone> = zones.iter().filter_map(|z| self.zones.get(&(slot, *z))).filter(|z| z.uv_area[uv_set] > 0.0).collect();
        let solid = chosen.iter().any(|z| fill(z) >= 0.3);
        // The chosen zones' faces by UV island. A zone can be unwrapped in
        // several pieces that lie apart in the texture (a rear quarter: the
        // outer skin, the door shut, the arch lip); the mean of them all is
        // a place between the pieces, on none of them.
        let mut islands: Vec<Island> = Vec::new();
        for z in chosen {
            let a = z.uv_area[uv_set];
            s.normal += z.normal;
            s.area += z.area;
            if solid && fill(z) < 0.3 {
                continue;
            }
            for i in &z.islands[uv_set] {
                match islands.iter_mut().find(|o| o.id == i.id) {
                    Some(o) => o.add(i),
                    None => islands.push(*i),
                }
            }
            s.uv_lo = s.uv_lo.min(z.uv_lo[uv_set]);
            s.uv_hi = s.uv_hi.max(z.uv_hi[uv_set]);
            w += a;
        }
        // the largest island is the panel; a sticker is laid out on it
        let main = islands.iter().max_by(|a, b| a.area.total_cmp(&b.area))?;
        if w <= 0.0 || main.area <= 0.0 {
            return None;
        }
        s.du = main.du / main.area;
        s.dv = main.dv / main.area;
        s.normal = s.normal.normalize_or_zero();
        s.islands = islands.len();
        s.main_share = main.area / w;
        // the box of the bulk: mean +- sqrt(3) standard deviations (exactly
        // the box of an evenly filled rectangle), inside the island's box
        let mean = main.sum / main.area;
        let half = ((main.sq / main.area - mean * mean).max(Vec2::ZERO)).powf(0.5) * 3f32.sqrt();
        s.fit_lo = (mean - half).max(main.lo);
        s.fit_hi = (mean + half).min(main.hi);
        Some(s)
    }
}

/// How the UVs of a set of zones lie on the model.
#[derive(Clone, Copy, Debug, Default)]
pub struct ZoneSet {
    pub normal: Vec3,
    /// Model-space step per unit of U and of V.
    pub du: Vec3,
    pub dv: Vec3,
    pub uv_lo: Vec2,
    pub uv_hi: Vec2,
    /// The UV box of the bulk of the zones (see `Zone::uv_sum`): what a
    /// sticker is fitted to and measured from.
    pub fit_lo: Vec2,
    pub fit_hi: Vec2,
    pub area: f32,
    /// How many UV islands the zones lie on, and the largest one's share
    /// of their mapped area. The layout (`du`, `dv`, the fit box) is the
    /// largest island's.
    pub islands: usize,
    pub main_share: f32,
}

impl ZoneSet {
    /// The panel's "to the right" and "down" as seen from outside (unit
    /// vectors in the surface): a picture laid along them reads upright.
    pub fn axes(&self) -> (Vec3, Vec3) {
        let n = if self.normal.length_squared() > 0.0 { self.normal } else { self.du.cross(self.dv).normalize_or_zero() };
        // on a roof or bonnet "up" is towards the front of the vehicle
        let up = if n.y.abs() > 0.9 { Vec3::new(0.0, 0.0, -1.0) } else { Vec3::Y };
        let right = up.cross(n).normalize_or_zero();
        let down = right.cross(n).normalize_or_zero();
        (right, down)
    }

    /// Metres per UV unit along U and V.
    pub fn scale(&self) -> Vec2 {
        Vec2::new(self.du.length(), self.dv.length())
    }

    /// The shader's six numbers (a, b, c, d, ou, ov) for a sticker.
    pub fn shader_uv(&self, s: &Sticker) -> [f32; 6] {
        let (right, down) = self.axes();
        let (sin, cos) = s.rotation.to_radians().sin_cos();
        // anticlockwise seen from outside: the picture's right turns upward
        let mut r = right * cos - down * sin;
        let d = down * cos + right * sin;
        if s.mirror {
            r = -r;
        }
        let w = s.size[0].max(1e-4);
        let h = s.size[1].max(1e-4);
        let (a, b) = (r.dot(self.du) / w, r.dot(self.dv) / w);
        let (c, e) = (d.dot(self.du) / h, d.dot(self.dv) / h);
        let ou = 0.5 - (a * s.centre[0] + b * s.centre[1]);
        let ov = 0.5 - (c * s.centre[0] + e * s.centre[1]);
        [a, b, c, e, ou, ov]
    }

    /// The middle of the zones (of the box of their bulk).
    fn middle(&self) -> Vec2 {
        (self.fit_lo + self.fit_hi) * 0.5
    }

    /// A place in the UV set as metres across and down the panel from the
    /// middle of the zones.
    pub fn to_panel(&self, c: Vec2) -> Vec2 {
        let (right, down) = self.axes();
        let d = c - self.middle();
        let w = self.du * d.x + self.dv * d.y;
        Vec2::new(right.dot(w), down.dot(w))
    }

    /// The reverse of `to_panel`.
    pub fn from_panel(&self, p: Vec2) -> Vec2 {
        let (right, down) = self.axes();
        let (a, b, c, d) = (right.dot(self.du), right.dot(self.dv), down.dot(self.du), down.dot(self.dv));
        let det = a * d - b * c;
        if det.abs() < 1e-9 {
            return self.middle();
        }
        self.middle() + Vec2::new(p.x * d - p.y * b, p.y * a - p.x * c) / det
    }

    /// Half the zones' UV box measured on the panel, metres (the larger
    /// side): how far from the middle a sticker can sensibly go.
    pub fn reach(&self) -> f32 {
        let p = self.to_panel(self.fit_hi);
        let q = self.to_panel(Vec2::new(self.fit_hi.x, self.fit_lo.y));
        p.abs().max(q.abs()).max_element()
    }

    /// A first placement: in the middle of the zones, as large as fits in
    /// 80 % of the box of their bulk, with the picture's proportions (`aspect` = width
    /// / height).
    pub fn fit(&self, aspect: f32) -> ([f32; 2], [f32; 2]) {
        let c = self.middle();
        // the UV box measured on the panel: its corners across and down, metres
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for q in [self.fit_lo, self.fit_hi, Vec2::new(self.fit_lo.x, self.fit_hi.y), Vec2::new(self.fit_hi.x, self.fit_lo.y)] {
            let p = self.to_panel(q);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        let (bw, bh) = ((hi.x - lo.x).max(0.05), (hi.y - lo.y).max(0.05));
        let aspect = aspect.clamp(0.05, 20.0);
        let w = (bw * 0.8).min(bh * 0.8 * aspect);
        ([c.x, c.y], [w, w / aspect])
    }
}

/// One sticker as the shader reads it (`shader.wgsl`, `Sticker`).
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Record {
    /// One bit per zone.
    pub zones: [u32; 8],
    /// a, b, c, d
    pub m: [f32; 4],
    /// ou, ov, opacity, texture layer (-1 = none)
    pub o: [f32; 4],
    /// x: 1 drawn, 2 colour mask, 4 highlight the zones, 8 outline the
    /// sticker; y: model slot; z: UV set; in the first record (the header)
    /// x is the number of records after it
    pub flags: [u32; 4],
    pub c: [[f32; 4]; 3],
}

fn lin(c: [u8; 3]) -> [f32; 4] {
    let f = |v: u8| {
        let c = v as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    [f(c[0]), f(c[1]), f(c[2]), 1.0]
}

fn zone_bits(zones: &[u8]) -> [u32; 8] {
    let mut b = [0u32; 8];
    for z in zones {
        b[(*z >> 5) as usize] |= 1 << (*z & 31);
    }
    b
}

/// What is highlighted on the model besides the stickers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Highlight {
    /// Index of the sticker whose outline is shown.
    pub selected: Option<usize>,
    /// Also tint the zones it lies on (while zones are being chosen).
    pub tint: bool,
    /// A zone under the pointer.
    pub hover: Option<(u8, u8)>,
}

/// The shader records for a livery on a model: a header and one record per
/// sticker that can be drawn, in list order (later ones lie on top).
/// `layer_of`: the texture array layer of a picture. Also fills each
/// sticker's `shader_uv`.
pub fn records(l: &mut Livery, zones: &Zones, layer_of: &dyn Fn(&Picture) -> Option<u32>, hl: &Highlight) -> Vec<Record> {
    let mut out = vec![Record::default()];
    for (i, s) in l.stickers.iter_mut().enumerate() {
        if out.len() > MAX_STICKERS {
            break;
        }
        let selected = hl.selected == Some(i);
        let Some(slot) = zones.slot_of(&s.model) else { continue };
        let uv = match s.game_uv {
            Some(m) => m,
            None => {
                let Some(set) = zones.set(slot, &s.zones, s.uv_set.min(1) as usize) else { continue };
                set.shader_uv(s)
            }
        };
        s.shader_uv = Some(uv);
        if !s.visible && !selected {
            continue;
        }
        let layer = layer_of(&s.picture).filter(|_| s.visible);
        let mut flags = 1;
        if s.mask {
            flags |= 2;
        }
        if selected {
            flags |= 8;
            if hl.tint {
                flags |= 4;
            }
        }
        out.push(Record {
            zones: zone_bits(&s.zones),
            m: [uv[0], uv[1], uv[2], uv[3]],
            o: [uv[4], uv[5], s.opacity.clamp(0.0, 1.0), layer.map_or(-1.0, |l| l as f32)],
            // UV sets 2 and 3 only for the game's own placements (hand
            // placing measures the zones in the first two)
            flags: [flags, slot as u32, s.uv_set.min(if s.game_uv.is_some() { 3 } else { 1 }) as u32, 0],
            c: s.colours.map(lin),
        });
    }
    if let Some((slot, zone)) = hl.hover {
        out.push(Record { zones: zone_bits(&[zone]), o: [0.0, 0.0, 0.0, -1.0], flags: [1 | 4, slot as u32, 0, 1], ..Default::default() });
    }
    out[0].flags[0] = (out.len() - 1) as u32;
    out
}

/// A picture as a layer: `LAYER` x `LAYER` RGBA with its mip chain, and
/// the picture's own size.
pub struct Layer {
    pub mips: Vec<Vec<u8>>,
    pub size: [u32; 2],
}

/// Read a picture (a game sticker texture through the cache directory, or a
/// file) and resample it to a layer.
pub fn read_layer(p: &Picture, game: &Path, cache: &Path) -> anyhow::Result<Layer> {
    let file = match p {
        Picture::Game(id) => {
            let id = sixng_core::vehstickers::parse_texture_id(id).ok_or_else(|| anyhow::anyhow!("not a texture id: {id}"))?;
            sixng_core::vehstickers::sticker_png(game, id, &cache.join("stickers"))?.0
        }
        Picture::File(f) => f.clone(),
    };
    let img = image::open(&file)?.to_rgba8();
    let size = [img.width(), img.height()];
    let top = if size == [LAYER, LAYER] { img } else { image::imageops::resize(&img, LAYER, LAYER, image::imageops::FilterType::Triangle) };
    let mut mips = vec![top.into_raw()];
    let mut w = LAYER as usize;
    while w > 1 {
        let src = mips.last().unwrap();
        let h = w / 2;
        let mut dst = vec![0u8; h * h * 4];
        for y in 0..h {
            for x in 0..h {
                // alpha-weighted, so see-through texels do not darken edges
                let mut acc = [0u32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let i = ((y * 2 + dy) * w + x * 2 + dx) * 4;
                    let a = src[i + 3] as u32;
                    acc[0] += src[i] as u32 * a;
                    acc[1] += src[i + 1] as u32 * a;
                    acc[2] += src[i + 2] as u32 * a;
                    acc[3] += a;
                }
                let o = (y * h + x) * 4;
                if acc[3] > 0 {
                    for k in 0..3 {
                        dst[o + k] = (acc[k] / acc[3]) as u8;
                    }
                } else {
                    for k in 0..3 {
                        let i = (y * 2 * w + x * 2) * 4;
                        dst[o + k] = src[i + k];
                    }
                }
                dst[o + 3] = (acc[3] / 4) as u8;
            }
        }
        mips.push(dst);
        w = h;
    }
    Ok(Layer { mips, size })
}

/// A picture that was read, or why it could not be.
type Read = Result<std::sync::Arc<Layer>, String>;

/// A vehicle's livery in the viewer: the placements, the survey of the
/// loaded model's zones, and the pictures on the GPU.
#[derive(Default)]
pub struct State {
    pub livery: Livery,
    /// What is in the file (to tell unsaved changes).
    pub saved: Livery,
    /// `--livery FILE`: open this file for the next vehicle instead of its
    /// saved one.
    pub open_once: Option<PathBuf>,
    /// `--livery-info FILE`: write the zones of the next vehicle there.
    pub info_once: Option<PathBuf>,
    /// The vehicle the livery belongs to (`vehicle_key`).
    pub vehicle: String,
    zones_key: (PathBuf, usize, usize),
    pub zones: Zones,
    /// Pictures read so far, by `Picture::key`; an error text for the ones
    /// that could not be read.
    layers: std::collections::HashMap<String, Read>,
    /// Pictures being read on worker threads (by key, with the number of
    /// the read), and what the threads hand back.
    reading: std::collections::HashMap<String, u64>,
    reads: u64,
    read: std::sync::Arc<std::sync::Mutex<Vec<(String, u64, Read)>>>,
    /// Pictures that came in and that the panel has not looked at yet.
    pub arrived: Vec<String>,
    /// The keys of the texture array's layers, in order.
    on_gpu: Vec<String>,
    view: Option<wgpu::TextureView>,
    /// Set after any change to the placements or the highlight.
    pub dirty: bool,
    pub highlight: Highlight,
    pub status: String,
    /// The Liveries panel.
    pub ed: crate::livery_ui::Editor,
}

impl State {
    /// The size of a picture in pixels (starts reading it on first use;
    /// None until it is read, see `picture_reading`).
    pub fn picture_size(&mut self, p: &Picture, game: &Path, cache: &Path) -> Option<[u32; 2]> {
        self.layer(p, game, cache)?.ok().map(|l| l.size)
    }

    /// A picture as a layer. It is read on a thread of its own (a large
    /// file, or a game texture that is not in the cache yet, takes a
    /// while): None until that is done.
    fn layer(&mut self, p: &Picture, game: &Path, cache: &Path) -> Option<Read> {
        let key = p.key();
        if let Some(r) = self.layers.get(&key) {
            return Some(r.clone());
        }
        if !self.reading.contains_key(&key) {
            self.reads += 1;
            self.reading.insert(key.clone(), self.reads);
            let (n, p, game, cache, out) = (self.reads, p.clone(), game.to_path_buf(), cache.to_path_buf(), self.read.clone());
            std::thread::spawn(move || {
                let r = read_layer(&p, &game, &cache).map(std::sync::Arc::new).map_err(|e| format!("{e:#}"));
                if let Ok(mut out) = out.lock() {
                    out.push((key, n, r));
                }
            });
        }
        None
    }

    /// Take in the pictures the threads have read since. True when one
    /// came in.
    fn take_read(&mut self) -> bool {
        let got = self.read.lock().map(|mut r| std::mem::take(&mut *r)).unwrap_or_default();
        let mut any = false;
        for (key, n, r) in got {
            // (not a read that was started before the picture was forgotten)
            if self.reading.get(&key) == Some(&n) {
                self.reading.remove(&key);
                self.layers.insert(key.clone(), r);
                self.arrived.push(key);
                any = true;
            }
        }
        any
    }

    /// How many pictures are still being read.
    pub fn reading(&self) -> usize {
        self.reading.len()
    }

    /// This picture is still being read.
    pub fn picture_reading(&self, p: &Picture) -> bool {
        self.reading.contains_key(&p.key())
    }

    /// A picture at 128 x 128 (stretched like its layer), for previews.
    pub fn layer_small(&mut self, p: &Picture, game: &Path, cache: &Path) -> Option<(Vec<u8>, usize)> {
        let l = self.layer(p, game, cache)?.ok()?;
        let k = l.mips.iter().position(|m| m.len() == 128 * 128 * 4)?;
        Some((l.mips[k].clone(), 128))
    }

    /// Why a picture is not shown, if it could not be read.
    pub fn picture_error(&self, p: &Picture) -> Option<&str> {
        self.layers.get(&p.key()).and_then(|r| r.as_ref().err()).map(|s| s.as_str())
    }

    /// Forget a picture read from a file (after the file changed).
    pub fn forget(&mut self, p: &Picture) {
        self.layers.remove(&p.key());
        self.reading.remove(&p.key());
        self.on_gpu.clear();
        self.dirty = true;
    }

    pub fn file(&self, work: &Path) -> PathBuf {
        file_for(work, &self.vehicle)
    }

    pub fn unsaved(&self) -> bool {
        // `fresh` is not saved, so it does not count
        let plain = |l: &Livery| l.stickers.iter().map(|s| Sticker { fresh: false, ..s.clone() }).collect::<Vec<_>>();
        plain(&self.livery) != plain(&self.saved)
    }

    /// Bring the GPU in line with the placements. Called every frame with
    /// the loaded vehicle; does nothing while nothing changed. `parts`: the
    /// fitted part names in slot order.
    #[allow(clippy::too_many_arguments)]
    pub fn sync(&mut self, rs: &egui_wgpu::RenderState, ren: &crate::render3d::Renderer3D, gpu: &mut crate::render3d::GpuModel, model: &glb::Model, vehicle: &str, parts: &[String], game: &Path, work: &Path, cache: &Path) {
        if vehicle != self.vehicle {
            self.vehicle = vehicle.to_string();
            let file = self.open_once.take().unwrap_or_else(|| file_for(work, vehicle));
            self.status.clear();
            self.livery = match load(&file) {
                Ok(l) => {
                    self.status = format!("loaded {} sticker(s) from {}", l.stickers.len(), file.display());
                    l
                }
                Err(e) => {
                    if file.exists() {
                        self.status = format!("{}: {e:#}", file.display());
                    }
                    Livery::default()
                }
            };
            self.livery.format = 1;
            self.livery.vehicle = vehicle.to_string();
            self.saved = self.livery.clone();
            self.highlight = Highlight::default();
            self.dirty = true;
        }
        let key = (model.path.clone(), model.triangles, model.vertices);
        if key != self.zones_key {
            self.zones = Zones::of(model, parts);
            self.zones_key = key;
            self.dirty = true;
            if let Some(f) = self.info_once.take() {
                let _ = sixng_core::atomic::write(f, serde_json::to_vec_pretty(&self.zones.describe()).unwrap_or_default());
            }
        }
        // a picture that has been read now is drawn from this frame on
        if self.take_read() {
            self.dirty = true;
        }
        self.ed.reading = self.reading.len();
        let nothing = self.livery.stickers.is_empty() && self.highlight == Highlight::default();
        if !self.dirty && (gpu.sticker_buf.is_some() || nothing) {
            return;
        }
        // the pictures of the stickers shown, as layers
        let mut want: Vec<Picture> = Vec::new();
        for s in self.livery.stickers.iter().filter(|s| s.visible) {
            if !want.contains(&s.picture) && want.len() < MAX_IMAGES {
                want.push(s.picture.clone());
            }
        }
        let mut keys = Vec::new();
        let mut data = Vec::new();
        for p in &want {
            // (one still being read is drawn when it comes in)
            if let Some(Ok(l)) = self.layer(p, game, cache) {
                keys.push(p.key());
                data.push(l);
            }
        }
        self.ed.reading = self.reading.len();
        if keys != self.on_gpu {
            self.view = (!data.is_empty()).then(|| crate::render3d::sticker_array(&rs.device, &rs.queue, LAYER, &data.iter().map(|l| l.mips.clone()).collect::<Vec<_>>()));
            self.on_gpu = keys;
        }
        let on_gpu = &self.on_gpu;
        let recs = records(&mut self.livery, &self.zones, &|p| on_gpu.iter().position(|k| *k == p.key()).map(|i| i as u32), &self.highlight);
        ren.set_stickers(rs, gpu, &recs, self.view.as_ref());
        self.dirty = false;
    }
}

impl Zones {
    /// The zones as JSON (`--livery-info`): per model its zones with the
    /// side they face, their area and how their UVs lie.
    pub fn describe(&self) -> serde_json::Value {
        let models: Vec<serde_json::Value> = self
            .models
            .iter()
            .enumerate()
            .map(|(slot, name)| {
                let zones: Vec<serde_json::Value> = self
                    .zones
                    .values()
                    .filter(|z| z.slot as usize == slot)
                    .map(|z| {
                        let set = |k: usize| serde_json::json!({"lo": z.uv_lo[k].to_array(), "hi": z.uv_hi[k].to_array(), "metres_per_unit": [z.du[k].length(), z.dv[k].length()], "mapped_area": z.uv_area[k], "islands": z.islands[k].iter().map(|i| serde_json::json!({"area": i.area, "lo": i.lo.to_array(), "hi": i.hi.to_array()})).collect::<Vec<_>>()});
                        serde_json::json!({"zone": z.zone, "side": z.side(), "triangles": z.triangles, "area_m2": z.area, "centre": z.centre.to_array(), "normal": z.normal.normalize_or_zero().to_array(), "uv": [set(0), set(1)]})
                    })
                    .collect();
                serde_json::json!({"model": name, "zones": zones})
            })
            .collect();
        serde_json::json!({"note": "Sticker zones of the loaded vehicle. The model with an empty name is the vehicle itself, the others are fitted parts. uv[0] is the first UV set, uv[1] the second.", "models": models})
    }
}

/// A ray hit on a model: where, on which zone, and the UVs there.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub t: f32,
    pub slot: u8,
    pub zone: u8,
    pub uv: [Vec2; 2],
}

/// Whether a ray passes through a box (slab test).
fn ray_box(origin: Vec3, dir: Vec3, b: &[Vec3; 2]) -> bool {
    let inv = dir.recip();
    let t0 = (b[0] - origin) * inv;
    let t1 = (b[1] - origin) * inv;
    let near = t0.min(t1).max_element();
    let far = t0.max(t1).min_element();
    far >= near.max(0.0)
}

/// Where a vertex is drawn: skinned by the joint matrices as the viewer's
/// shader does it (a vertex without weights, or of a mesh that is not
/// skinned, stays where it is stored).
fn posed_vertex(v: &glb::Vertex, skinned: bool, skin: &[Mat4]) -> Vec3 {
    let p = Vec3::from(v.pos);
    if !skinned {
        return p;
    }
    let (mut s, mut tw) = (Vec3::ZERO, 0.0);
    for k in 0..4 {
        let w = v.weights[k] as f32 / 255.0;
        if let Some(m) = skin.get(v.joints[k] as usize).filter(|_| w > 0.0) {
            s += m.transform_point3(p) * w;
            tw += w;
        }
    }
    if tw < 0.001 {
        p
    } else {
        s / tw
    }
}

/// The meshes with sticker zones as the view draws them: with doors, lids
/// and whatever else the joints move where they are now.
#[derive(Default)]
pub struct Posed {
    /// Per mesh the place of every vertex (empty: a mesh without zones).
    pos: Vec<Vec<Vec3>>,
    /// Per mesh the box around them.
    boxes: Vec<[Vec3; 2]>,
}

impl Posed {
    /// `skin`: the joint matrices drawn with (none: as stored).
    pub fn of(m: &glb::Model, skin: &[Mat4]) -> Posed {
        let mut p = Posed::default();
        for me in &m.meshes {
            let mut b = [Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)];
            let mut pos = Vec::new();
            if me.verts.first().is_some_and(|v| v.zone[2] != 0) {
                pos.reserve(me.verts.len());
                for v in &me.verts {
                    let q = posed_vertex(v, me.skinned, skin);
                    b = [b[0].min(q), b[1].max(q)];
                    pos.push(q);
                }
            }
            p.pos.push(pos);
            p.boxes.push(b);
        }
        p
    }
}

/// The nearest triangle with sticker zones a ray hits. `posed`: the meshes
/// as they are drawn, when the caller keeps them (meshes the ray misses
/// are skipped); without it the model as stored, doors shut.
pub fn pick(m: &glb::Model, origin: Vec3, dir: Vec3, posed: Option<&Posed>) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for (mi, me) in m.meshes.iter().enumerate() {
        // a mesh without the zone stream carries no stickers: the ray
        // passes through it (bolts and trim in front of a wall)
        if me.verts.first().is_none_or(|v| v.zone[2] == 0) || posed.and_then(|p| p.boxes.get(mi)).is_some_and(|b| !ray_box(origin, dir, b)) {
            continue;
        }
        let at = posed.and_then(|p| p.pos.get(mi)).filter(|p| p.len() == me.verts.len());
        let place = |i: u32| at.map_or_else(|| Vec3::from(me.verts[i as usize].pos), |p| p[i as usize]);
        for t in me.indices.chunks_exact(3) {
            let v = [&me.verts[t[0] as usize], &me.verts[t[1] as usize], &me.verts[t[2] as usize]];
            let p0 = place(t[0]);
            let e1 = place(t[1]) - p0;
            let e2 = place(t[2]) - p0;
            let h = dir.cross(e2);
            let det = e1.dot(h);
            if det.abs() < 1e-12 {
                continue;
            }
            let inv = 1.0 / det;
            let s = origin - p0;
            let a = s.dot(h) * inv;
            if !(0.0..=1.0).contains(&a) {
                continue;
            }
            let q = s.cross(e1);
            let b = dir.dot(q) * inv;
            if b < 0.0 || a + b > 1.0 {
                continue;
            }
            let dist = e2.dot(q) * inv;
            if dist <= 1e-4 || best.is_some_and(|x| x.t <= dist) {
                continue;
            }
            let w = 1.0 - a - b;
            let uv = |k: usize| {
                let g = |v: &glb::Vertex| Vec2::from(if k == 0 { v.uv0 } else { v.uv1 });
                g(v[0]) * w + g(v[1]) * a + g(v[2]) * b
            };
            best = Some(Hit { t: dist, slot: v[0].zone[1], zone: v[0].zone[0], uv: [uv(0), uv(1)] });
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(zone: u8, flip_u: bool) -> glb::Model {
        // a 4 m x 2 m wall in the XY plane facing +Z; UV 0..1 across it,
        // V down
        let v = |x: f32, y: f32| {
            let u = (x + 2.0) / 4.0;
            glb::Vertex { pos: [x, y, 0.0], nrm: [0.0, 0.0, 1.0], uv0: [u, 1.0 - y / 2.0], uv1: [if flip_u { 1.0 - u } else { u }, 1.0 - y / 2.0], zone: [zone, 0, 1, 0], ..Default::default() }
        };
        let mesh = glb::Mesh { strand_lod: None, region2: Vec::new(), morphs: Vec::new(), name: String::new(), verts: vec![v(-2.0, 0.0), v(2.0, 0.0), v(2.0, 2.0), v(-2.0, 2.0)], indices: vec![0, 1, 2, 0, 2, 3], material: None, skinned: false, has_tangents: false, uv_sets: 2, centre: [0.0; 3] };
        glb::Model { path: PathBuf::new(), meshes: vec![mesh], materials: Vec::new(), joints: Vec::new(), images: Vec::new(), bbox: [[0.0; 3]; 2], triangles: 2, vertices: 4, extras: serde_json::Value::Null }
    }

    #[test]
    fn a_sticker_is_upright_and_metric_whatever_the_uv_direction() {
        for flip in [false, true] {
            let z = Zones::of(&quad(7, flip), &[]);
            let zone = &z.zones[&(0, 7)];
            assert!((zone.area - 8.0).abs() < 1e-4);
            assert_eq!(zone.side(), "rear");
            let set = z.set(0, &[7], 1).unwrap();
            let mut s = Sticker::new(Picture::Game("0".into()));
            s.zones = vec![7];
            // 2 m wide, 1 m high, centred on the wall's middle (x 0, y 1)
            s.centre = [0.5, 0.5];
            s.size = [2.0, 1.0];
            let m = set.shader_uv(&s);
            let at = |x: f32, y: f32| {
                let u = (x + 2.0) / 4.0;
                let (u, v) = (if flip { 1.0 - u } else { u }, 1.0 - y / 2.0);
                (u * m[0] + v * m[1] + m[4], u * m[2] + v * m[3] + m[5])
            };
            // the picture's left top corner is at x -1, y 1.5 on the wall
            let (a, b) = at(-1.0, 1.5);
            assert!(a.abs() < 1e-4 && b.abs() < 1e-4, "{a} {b}");
            let (a, b) = at(1.0, 0.5);
            assert!((a - 1.0).abs() < 1e-4 && (b - 1.0).abs() < 1e-4, "{a} {b}");
        }
    }

    #[test]
    fn a_first_placement_fits_the_zone() {
        let z = Zones::of(&quad(3, false), &[]);
        let set = z.set(0, &[3], 0).unwrap();
        let (c, size) = set.fit(4.0);
        assert!((c[0] - 0.5).abs() < 1e-4 && (c[1] - 0.5).abs() < 1e-4);
        // 80 % of 4 m wide is 3.2 m; at 4:1 that is 0.8 m high, which fits 2 m
        assert!((size[0] - 3.2).abs() < 1e-3 && (size[1] - 0.8).abs() < 1e-3, "{size:?}");
    }

    #[test]
    fn a_first_placement_lies_on_the_largest_uv_island() {
        // the wall on the left half of the texture, and a small strip of
        // the same zone unwrapped far away, turned a quarter
        let mut m = quad(3, false);
        for v in &mut m.meshes[0].verts {
            v.uv0[0] *= 0.5;
        }
        let v = |x: f32, y: f32, u: f32, w: f32| glb::Vertex { pos: [x, y, 0.0], nrm: [0.0, 0.0, 1.0], uv0: [u, w], uv1: [u, w], zone: [3, 0, 1, 0], ..Default::default() };
        m.meshes[0].verts.extend([v(2.0, 0.0, 0.9, 0.9), v(2.5, 0.0, 0.9, 0.95), v(2.5, 2.0, 1.0, 0.95), v(2.0, 2.0, 1.0, 0.9)]);
        m.meshes[0].indices.extend([4, 5, 6, 4, 6, 7]);
        let z = Zones::of(&m, &[]);
        assert_eq!(z.zones[&(0, 3)].islands[0].len(), 2);
        let set = z.set(0, &[3], 0).unwrap();
        assert_eq!(set.islands, 2);
        assert!((set.main_share - 8.0 / 9.0).abs() < 1e-3, "{}", set.main_share);
        let (c, size) = set.fit(4.0);
        // the middle of the wall, not a place between the two pieces
        assert!((c[0] - 0.25).abs() < 1e-4 && (c[1] - 0.5).abs() < 1e-4, "{c:?}");
        // and the wall's own scale: 8 m per unit of U, 2 m per unit of V
        assert!((set.scale().x - 8.0).abs() < 1e-3 && (set.scale().y - 2.0).abs() < 1e-3, "{:?}", set.scale());
        assert!((size[0] - 3.2).abs() < 1e-3 && (size[1] - 0.8).abs() < 1e-3, "{size:?}");
    }

    #[test]
    fn neighbouring_zones_on_one_island_are_fitted_together() {
        // two zones side by side on one continuous unwrap
        let mut m = quad(3, false);
        let mut other = quad(4, false).meshes.remove(0);
        for v in &mut other.verts {
            v.pos[0] += 4.0;
            v.uv0[0] += 1.0;
        }
        m.meshes.push(other);
        let z = Zones::of(&m, &[]);
        let set = z.set(0, &[3, 4], 0).unwrap();
        assert_eq!(set.islands, 1);
        let (c, _) = set.fit(4.0);
        assert!((c[0] - 1.0).abs() < 1e-4, "{c:?}");
    }

    #[test]
    fn undo_goes_back_a_step_and_redo_forward() {
        let sticker = |x: f32| {
            let mut s = Sticker::new(Picture::Game("0".into()));
            s.centre = [x, 0.5];
            s
        };
        let mut l = Livery::default();
        let mut h = History::default();
        assert!(!h.track("car", &l, 0.0, false));
        assert!(!h.can_undo() && !h.can_redo());
        // a sticker is added, then dragged over several frames: two steps
        l.stickers.push(sticker(0.1));
        assert!(h.track("car", &l, 1.0, false));
        assert!(!h.track("car", &l, 1.5, false));
        for (k, t) in [2.0, 2.1, 2.2].into_iter().enumerate() {
            l.stickers[0].centre[0] = 0.2 + k as f32 * 0.1;
            assert!(h.track("car", &l, t, true));
        }
        // still held: no step however long it rests
        assert!(h.track("car", &l, 9.0, true));
        // what the viewer writes back is not a change
        l.stickers[0].shader_uv = Some([1.0; 6]);
        assert!(!h.track("car", &l, 9.5, false));
        assert!(h.can_undo() && !h.can_redo());
        assert!(h.undo(&mut l));
        assert_eq!(l.stickers[0].centre[0], 0.1);
        assert!(h.undo(&mut l));
        assert!(l.stickers.is_empty());
        assert!(!h.undo(&mut l) && !h.can_undo());
        assert!(h.redo(&mut l) && h.redo(&mut l) && !h.redo(&mut l));
        assert_eq!(l.stickers[0].centre[0], 0.4);
        // undo right after a change takes the change back, without waiting
        l.stickers[0].rotation = 90.0;
        assert!(h.track("car", &l, 10.0, false));
        assert!(h.undo(&mut l));
        assert_eq!(l.stickers[0].rotation, 0.0);
        // a new change after an undo drops what could be redone
        assert!(h.can_redo());
        l.stickers[0].mirror = true;
        h.track("car", &l, 11.0, false);
        h.track("car", &l, 12.0, false);
        assert!(!h.can_redo() && !h.redo(&mut l));
        // another vehicle starts anew
        assert!(!h.track("truck", &l, 13.0, false));
        assert!(!h.can_undo());
    }

    #[test]
    fn a_ray_finds_the_zone_and_uv() {
        let m = quad(9, false);
        let h = pick(&m, Vec3::new(1.0, 0.5, 5.0), Vec3::new(0.0, 0.0, -1.0), None).unwrap();
        let boxes = Posed::of(&m, &[]);
        assert!(pick(&m, Vec3::new(1.0, 0.5, 5.0), Vec3::new(0.0, 0.0, -1.0), Some(&boxes)).is_some());
        assert_eq!((h.slot, h.zone), (0, 9));
        assert!((h.uv[0].x - 0.75).abs() < 1e-4 && (h.uv[0].y - 0.75).abs() < 1e-4);
        assert!(pick(&m, Vec3::new(5.0, 0.5, 5.0), Vec3::new(0.0, 0.0, -1.0), None).is_none());
        assert!(pick(&m, Vec3::new(5.0, 0.5, 5.0), Vec3::new(0.0, 0.0, -1.0), Some(&boxes)).is_none());
    }

    #[test]
    fn a_ray_finds_a_door_where_it_is_swung_to() {
        // the wall is a door on joint 1, hinged at its left edge (x -2) and
        // swung a quarter turn towards the viewer: it now lies in the
        // plane x -2, from z 0 to z 4
        let mut m = quad(9, false);
        m.meshes[0].skinned = true;
        for v in &mut m.meshes[0].verts {
            v.joints = [1, 0, 0, 0];
            v.weights = [255, 0, 0, 0];
        }
        let hinge = Vec3::new(-2.0, 0.0, 0.0);
        let swing = Mat4::from_translation(hinge) * Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2) * Mat4::from_translation(-hinge);
        let skin = [Mat4::IDENTITY, swing];
        let open = Posed::of(&m, &skin);
        let (from_front, from_side) = ((Vec3::new(1.0, 0.5, 5.0), Vec3::new(0.0, 0.0, -1.0)), (Vec3::new(-7.0, 0.5, 3.0), Vec3::new(1.0, 0.0, 0.0)));
        // where the door was there is nothing now
        assert!(pick(&m, from_front.0, from_front.1, Some(&open)).is_none());
        // a ray from the side meets it 3 m out along the door: u 0.75
        let h = pick(&m, from_side.0, from_side.1, Some(&open)).unwrap();
        assert_eq!((h.slot, h.zone), (0, 9));
        assert!((h.t - 5.0).abs() < 1e-3, "{}", h.t);
        assert!((h.uv[0].x - 0.75).abs() < 1e-3 && (h.uv[0].y - 0.75).abs() < 1e-3, "{:?}", h.uv[0]);
        // shut (no matrices, or the mesh not skinned) it is where it is stored
        let shut = Posed::of(&m, &[]);
        assert!(pick(&m, from_front.0, from_front.1, Some(&shut)).is_some());
        assert!(pick(&m, from_side.0, from_side.1, Some(&shut)).is_none());
        m.meshes[0].skinned = false;
        assert!(pick(&m, from_front.0, from_front.1, Some(&Posed::of(&m, &skin))).is_some());
    }

    #[test]
    fn panel_metres_and_uv_convert_both_ways() {
        for flip in [false, true] {
            let z = Zones::of(&quad(7, flip), &[]);
            let set = z.set(0, &[7], 1).unwrap();
            // 1 m to the right of the middle and 0.5 m down
            let uv = set.from_panel(Vec2::new(1.0, 0.5));
            let u = if flip { 0.25 } else { 0.75 };
            assert!((uv.x - u).abs() < 1e-4 && (uv.y - 0.75).abs() < 1e-4, "{uv:?}");
            let back = set.to_panel(uv);
            assert!((back.x - 1.0).abs() < 1e-4 && (back.y - 0.5).abs() < 1e-4);
            assert!((set.reach() - 2.0).abs() < 1e-4);
        }
    }

    #[test]
    fn a_livery_file_round_trips() {
        let mut s = Sticker::new(Picture::Game("00112233445566aa".into()));
        s.zones = vec![169, 170];
        s.model = "truckbeds_box_01_hi".into();
        // not saved: a sticker read from a file counts as placed by hand
        s.fresh = false;
        let l = Livery { format: 1, vehicle: "benson_hi".into(), stickers: vec![s] };
        let back: Livery = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        assert_eq!(l, back);
        assert_eq!(vehicle_key("~^benson_hi$#parts=1a,2b"), "benson_hi");
    }
}
