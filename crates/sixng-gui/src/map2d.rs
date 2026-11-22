#![allow(dead_code)]
//! 2D top-down map over the baked `<work>/map` (see mapbake.rs): raster
//! tile layers, vector roads / junctions / lights / interiors, click to
//! inspect placements, search, coordinate readout and a measure tool.
use crate::mapbake::{self, EntRec, LightRec, KIND_NAMES};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};

type TileKey = (usize, u32, usize, usize); // layer, z, x, y

enum TileState {
    Loading,
    Ready(egui::TextureHandle, u64),
    Missing,
}

struct Loader {
    queue: Mutex<Vec<(TileKey, PathBuf)>>,
    cv: Condvar,
    done: Mutex<Vec<(TileKey, Option<egui::ColorImage>)>>,
}

pub struct Road {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub speed: f32,
    pub lf: u8,
    pub lt: u8,
    pub flags: u32,
    pub class: u8,
    pub nodes: [u32; 2],
}

pub struct Junction {
    pub p: [f32; 3],
    pub entrances: Vec<([f32; 2], bool, bool)>,
    pub phases: Vec<f32>,
}

pub struct Grid {
    cell: f32,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

impl Grid {
    fn new(cell: f32) -> Self {
        let nx = (mapbake::W as f32 * mapbake::CELL / cell).ceil() as usize + 1;
        let ny = (mapbake::H as f32 * mapbake::CELL / cell).ceil() as usize + 1;
        Grid {
            cell,
            nx,
            ny,
            cells: vec![Vec::new(); nx * ny],
        }
    }
    fn key(&self, x: f32, y: f32) -> Option<usize> {
        let cx = ((x - mapbake::X0) / self.cell).floor();
        let cy = ((mapbake::Y1 - y) / self.cell).floor();
        (cx >= 0.0 && cy >= 0.0 && (cx as usize) < self.nx && (cy as usize) < self.ny)
            .then(|| cy as usize * self.nx + cx as usize)
    }
    fn add(&mut self, x: f32, y: f32, i: u32) {
        if let Some(k) = self.key(x, y) {
            self.cells[k].push(i);
        }
    }
    /// Indices in cells overlapping the world box.
    pub fn query(&self, x0: f32, y0: f32, x1: f32, y1: f32, mut f: impl FnMut(u32)) {
        let cx0 = (((x0 - mapbake::X0) / self.cell).floor().max(0.0) as usize).min(self.nx - 1);
        let cx1 = (((x1 - mapbake::X0) / self.cell).floor().max(0.0) as usize).min(self.nx - 1);
        let cy0 = (((mapbake::Y1 - y1) / self.cell).floor().max(0.0) as usize).min(self.ny - 1);
        let cy1 = (((mapbake::Y1 - y0) / self.cell).floor().max(0.0) as usize).min(self.ny - 1);
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                for &i in &self.cells[cy * self.nx + cx] {
                    f(i);
                }
            }
        }
    }
}

pub struct MapData {
    pub meta: Value,
    pub roads: Vec<Road>,
    pub road_grid: Grid,
    pub junctions: Vec<Junction>,
    pub lights: Vec<LightRec>,
    pub light_grid: Grid,
    pub ents: Vec<EntRec>,
    pub ent_grid: Grid,
    pub strings: Vec<String>,
    pub files: Vec<String>,
    pub interiors: Vec<Value>,
    pub h8: Vec<f32>,
    pub surf_legend: Vec<(String, [u8; 3])>,
}

impl MapData {
    pub fn load(dir: &std::path::Path) -> anyhow::Result<MapData> {
        let meta: Value = serde_json::from_slice(&std::fs::read(dir.join("meta.json"))?)?;
        let rj: Value = std::fs::read(dir.join("roads.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut roads = Vec::new();
        let mut road_grid = Grid::new(256.0);
        for l in rj["links"].as_array().into_iter().flatten() {
            let f = |k: usize| l[k].as_f64().unwrap_or(0.0) as f32;
            let r = Road {
                a: [f(0), f(1), f(2)],
                b: [f(3), f(4), f(5)],
                speed: f(6),
                lf: l[7].as_u64().unwrap_or(0) as u8,
                lt: l[8].as_u64().unwrap_or(0) as u8,
                flags: l[9].as_u64().unwrap_or(0) as u32,
                class: l[10].as_u64().unwrap_or(0) as u8,
                nodes: [
                    l[12].as_u64().unwrap_or(0) as u32,
                    l[13].as_u64().unwrap_or(0) as u32,
                ],
            };
            road_grid.add(
                (r.a[0] + r.b[0]) / 2.0,
                (r.a[1] + r.b[1]) / 2.0,
                roads.len() as u32,
            );
            roads.push(r);
        }
        let jj: Value = std::fs::read(dir.join("junctions.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let junctions = jj
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|j| {
                let p: [f32; 3] = serde_json::from_value(j["p"].clone()).ok()?;
                let entrances = j["entrances"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| {
                        Some((
                            [e[0].as_f64()? as f32, e[1].as_f64()? as f32],
                            e[2].as_bool().unwrap_or(false),
                            e[3].as_bool().unwrap_or(false),
                        ))
                    })
                    .collect();
                let phases = j["phases"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| {
                        p["duration"]
                            .as_f64()
                            .or_else(|| p[0].as_f64())
                            .unwrap_or(10.0) as f32
                    })
                    .collect();
                Some(Junction {
                    p,
                    entrances,
                    phases,
                })
            })
            .collect();
        let lb = std::fs::read(dir.join("lights.bin")).unwrap_or_default();
        let ln = std::mem::size_of::<LightRec>();
        let lights: Vec<LightRec> = bytemuck::pod_collect_to_vec(&lb[..lb.len() / ln * ln]);
        let mut light_grid = Grid::new(128.0);
        for (i, l) in lights.iter().enumerate() {
            light_grid.add(l.pos[0], l.pos[1], i as u32);
        }
        let eb = std::fs::read(dir.join("entities.bin")).unwrap_or_default();
        let n = std::mem::size_of::<EntRec>();
        let ents: Vec<EntRec> = bytemuck::pod_collect_to_vec(&eb[..eb.len() / n * n]);
        let mut ent_grid = Grid::new(128.0);
        for (i, e) in ents.iter().enumerate() {
            ent_grid.add(e.pos[0], e.pos[1], i as u32);
        }
        let sj: Value = std::fs::read(dir.join("strings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let strings = sj["strings"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| s.as_str().unwrap_or("").to_string())
            .collect();
        let files = sj["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| s.as_str().unwrap_or("").to_string())
            .collect();
        let interiors = std::fs::read(dir.join("interiors.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok())
            .unwrap_or_default();
        let hb = std::fs::read(dir.join("heights_8m.bin")).unwrap_or_default();
        let h8 = bytemuck::pod_collect_to_vec(&hb[..hb.len() / 4 * 4]);
        let surf_legend = meta["surface_categories"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| {
                Some((
                    v[0].as_str()?.to_string(),
                    serde_json::from_value(v[1].clone()).ok()?,
                ))
            })
            .collect();
        Ok(MapData {
            meta,
            roads,
            road_grid,
            junctions,
            lights,
            light_grid,
            ents,
            ent_grid,
            strings,
            files,
            interiors,
            h8,
            surf_legend,
        })
    }

    pub fn height(&self, x: f32, y: f32) -> Option<f32> {
        let cw = mapbake::W / mapbake::COARSE;
        let ch = mapbake::H / mapbake::COARSE;
        let c = ((x - mapbake::X0) / (mapbake::CELL * mapbake::COARSE as f32)).floor();
        let r = ((mapbake::Y1 - y) / (mapbake::CELL * mapbake::COARSE as f32)).floor();
        if c < 0.0 || r < 0.0 || c as usize >= cw || r as usize >= ch {
            return None;
        }
        let v = *self.h8.get(r as usize * cw + c as usize)?;
        (!v.is_nan()).then_some(v)
    }

    pub fn name(&self, i: u32) -> &str {
        self.strings
            .get(i as usize)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn ent_corners(&self, e: &EntRec) -> [[f32; 2]; 4] {
        mapbake::corners(e.pos, e.yaw, [e.lo[0], e.lo[1]], [e.hi[0], e.hi[1]], 1.0)
    }

    pub fn ent_contains(&self, e: &EntRec, x: f32, y: f32) -> bool {
        let (s, c) = (-e.yaw).sin_cos();
        let dx = x - e.pos[0];
        let dy = y - e.pos[1];
        let lx = dx * c - dy * s;
        let ly = dx * s + dy * c;
        lx >= e.lo[0] && lx <= e.hi[0] && ly >= e.lo[1] && ly <= e.hi[1]
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Inspect,
    Measure,
    BoxSelect,
}

pub struct Map2D {
    pub dir: PathBuf,
    pub data: Option<Arc<MapData>>,
    pending: Arc<Mutex<Option<Result<MapData, String>>>>,
    loading: bool,
    pub status: String,
    pub cx: f32,
    pub cy: f32,
    pub mpp: f32,
    pub base: usize,
    pub overlays: Vec<(bool, f32)>,
    pub show_roads: bool,
    pub road_colour: usize,
    pub show_arrows: bool,
    pub show_junctions: bool,
    pub animate_phases: bool,
    pub show_lights: bool,
    pub light_ext: bool,
    pub light_int: bool,
    pub hour: f32,
    pub show_interiors: bool,
    pub show_footprints: bool,
    tiles: HashMap<TileKey, TileState>,
    loader: Arc<Loader>,
    frame: u64,
    pub selected: Option<u32>,
    pub hover_info: String,
    pub search: String,
    pub results: Vec<u32>,
    pub tool: Tool,
    pub measure: Vec<[f32; 2]>,
    pub box_sel: Option<[f32; 4]>,
    drag_start: Option<[f32; 2]>,
    pub open_model: Option<String>,
    pub export_req: Option<[f32; 4]>,
    started: std::time::Instant,
    pub pending_search: bool,
}

pub const ROAD_COLOURS: [&str; 3] = ["speed limit", "lane count", "road class"];

impl Map2D {
    pub fn new(work: &str) -> Self {
        let loader = Arc::new(Loader {
            queue: Mutex::new(Vec::new()),
            cv: Condvar::new(),
            done: Mutex::new(Vec::new()),
        });
        for _ in 0..6 {
            let l = loader.clone();
            std::thread::spawn(move || loop {
                let job = {
                    let mut q = l.queue.lock().unwrap();
                    while q.is_empty() {
                        q = l.cv.wait(q).unwrap();
                    }
                    q.pop()
                };
                let Some((key, path)) = job else { continue };
                let img = image::open(&path).ok().map(|i| {
                    let i = i.to_rgba8();
                    egui::ColorImage::from_rgba_unmultiplied(
                        [i.width() as usize, i.height() as usize],
                        i.as_raw(),
                    )
                });
                l.done.lock().unwrap().push((key, img));
            });
        }
        let n = mapbake::LAYERS.len();
        let mut overlays = vec![(false, 0.75); n];
        overlays[2] = (true, 0.85); // buildings
        Map2D {
            dir: PathBuf::from(work).join("map"),
            data: None,
            pending: Arc::new(Mutex::new(None)),
            loading: false,
            status: String::new(),
            cx: -2800.0,
            cy: -800.0,
            mpp: 4.0,
            base: 0,
            overlays,
            show_roads: true,
            road_colour: 0,
            show_arrows: true,
            show_junctions: true,
            animate_phases: true,
            show_lights: false,
            light_ext: true,
            light_int: false,
            hour: 22.0,
            show_interiors: false,
            show_footprints: true,
            tiles: HashMap::new(),
            loader,
            frame: 0,
            selected: None,
            hover_info: String::new(),
            search: String::new(),
            results: Vec::new(),
            tool: Tool::Inspect,
            measure: Vec::new(),
            box_sel: None,
            drag_start: None,
            open_model: None,
            export_req: None,
            started: std::time::Instant::now(),
            pending_search: false,
        }
    }

    /// Drop the loaded map (after a re-bake) so the next frame loads it again.
    pub fn reload(&mut self) {
        self.data = None;
        self.loading = false;
        *self.pending.lock().unwrap() = None;
        self.tiles.clear();
        self.selected = None;
        self.results.clear();
        self.status = "reloading map …".into();
    }

    pub fn ensure_loaded(&mut self, ctx: &egui::Context) {
        if self.data.is_some() || self.loading {
            if let Some(r) = self.pending.lock().unwrap().take() {
                self.loading = false;
                match r {
                    Ok(d) => {
                        self.status = format!(
                            "{} roads · {} junctions · {} lights · {} placements · {} interiors",
                            d.roads.len(),
                            d.junctions.len(),
                            d.lights.len(),
                            d.ents.len(),
                            d.interiors.len()
                        );
                        self.data = Some(Arc::new(d));
                    }
                    Err(e) => {
                        self.status =
                            format!("map not baked or unreadable: {e} (run sixng-gui --bake-map)")
                    }
                }
            }
            return;
        }
        if !self.dir.join("meta.json").exists() {
            self.status = format!(
                "No baked map in {}. Run `sixng-gui --bake-map` (≈10 min).",
                self.dir.display()
            );
            return;
        }
        self.loading = true;
        self.status = "loading map data …".into();
        let dir = self.dir.clone();
        let slot = self.pending.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            *slot.lock().unwrap() = Some(MapData::load(&dir).map_err(|e| e.to_string()));
            ctx.request_repaint();
        });
    }

    fn w2s(&self, rect: Rect, x: f32, y: f32) -> Pos2 {
        rect.center() + Vec2::new((x - self.cx) / self.mpp, -(y - self.cy) / self.mpp)
    }
    fn s2w(&self, rect: Rect, p: Pos2) -> [f32; 2] {
        let d = p - rect.center();
        [self.cx + d.x * self.mpp, self.cy - d.y * self.mpp]
    }

    fn draw_layer(&mut self, ui: &egui::Ui, rect: Rect, layer: usize, opacity: f32) {
        let zmax = self
            .data
            .as_ref()
            .and_then(|d| d.meta["zmax"].as_u64())
            .unwrap_or(6) as u32;
        let ppp = ui.ctx().pixels_per_point();
        // metres per physical pixel; pick the level whose pixel is <= that
        let mpx = self.mpp / ppp;
        let mut z = zmax as i32 - (mpx / mapbake::CELL).log2().floor().max(0.0) as i32;
        z = z.clamp(0, zmax as i32);
        let z = z as u32;
        let tile_m = mapbake::TILE as f32 * mapbake::CELL * (1u32 << (zmax - z)) as f32;
        let [wx0, wy1] = self.s2w(rect, rect.left_top());
        let [wx1, wy0] = self.s2w(rect, rect.right_bottom());
        let tx0 = ((wx0 - mapbake::X0) / tile_m).floor().max(0.0) as usize;
        let tx1 = ((wx1 - mapbake::X0) / tile_m).floor().max(0.0) as usize;
        let ty0 = ((mapbake::Y1 - wy1) / tile_m).floor().max(0.0) as usize;
        let ty1 = ((mapbake::Y1 - wy0) / tile_m).floor().max(0.0) as usize;
        let name = mapbake::LAYERS[layer].0;
        let tint = Color32::from_white_alpha((opacity * 255.0) as u8);
        let painter = ui.painter_at(rect);
        let max_t = 1usize << zmax;
        for ty in ty0..=ty1.min(max_t * 2) {
            for tx in tx0..=tx1.min(max_t * 2) {
                let key = (layer, z, tx, ty);
                let x = mapbake::X0 + tx as f32 * tile_m;
                let y = mapbake::Y1 - ty as f32 * tile_m;
                let r = Rect::from_min_max(
                    self.w2s(rect, x, y),
                    self.w2s(rect, x + tile_m, y - tile_m),
                );
                match self.tiles.get_mut(&key) {
                    Some(TileState::Ready(h, last)) => {
                        *last = self.frame;
                        painter.image(
                            h.id(),
                            r,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            tint,
                        );
                    }
                    Some(TileState::Missing) => {}
                    Some(TileState::Loading) => {
                        // draw the parent level meanwhile
                        if z > 0 {
                            let pk = (layer, z - 1, tx / 2, ty / 2);
                            if let Some(TileState::Ready(h, _)) = self.tiles.get(&pk) {
                                let u0 = (tx % 2) as f32 * 0.5;
                                let v0 = (ty % 2) as f32 * 0.5;
                                painter.image(
                                    h.id(),
                                    r,
                                    Rect::from_min_max(
                                        Pos2::new(u0, v0),
                                        Pos2::new(u0 + 0.5, v0 + 0.5),
                                    ),
                                    tint,
                                );
                            }
                        }
                    }
                    None => {
                        let p = self
                            .dir
                            .join("tiles")
                            .join(name)
                            .join(z.to_string())
                            .join(format!("{tx}_{ty}.png"));
                        if p.exists() {
                            self.tiles.insert(key, TileState::Loading);
                            self.loader.queue.lock().unwrap().push((key, p));
                            self.loader.cv.notify_one();
                            // make sure the parent is requested too (placeholder)
                            if z > 0 && !self.tiles.contains_key(&(layer, z - 1, tx / 2, ty / 2)) {
                                let pp = self
                                    .dir
                                    .join("tiles")
                                    .join(name)
                                    .join((z - 1).to_string())
                                    .join(format!("{}_{}.png", tx / 2, ty / 2));
                                if pp.exists() {
                                    self.tiles
                                        .insert((layer, z - 1, tx / 2, ty / 2), TileState::Loading);
                                    self.loader
                                        .queue
                                        .lock()
                                        .unwrap()
                                        .push(((layer, z - 1, tx / 2, ty / 2), pp));
                                    self.loader.cv.notify_one();
                                }
                            }
                        } else {
                            self.tiles.insert(key, TileState::Missing);
                        }
                    }
                }
            }
        }
    }

    fn poll_tiles(&mut self, ctx: &egui::Context) -> bool {
        let done: Vec<_> = std::mem::take(&mut *self.loader.done.lock().unwrap());
        let any = !done.is_empty();
        for (key, img) in done {
            match img {
                Some(img) => {
                    let h =
                        ctx.load_texture(format!("tile{key:?}"), img, egui::TextureOptions::LINEAR);
                    self.tiles.insert(key, TileState::Ready(h, self.frame));
                }
                None => {
                    self.tiles.insert(key, TileState::Missing);
                }
            }
        }
        // evict old tiles
        if self.tiles.len() > 900 {
            let f = self.frame;
            self.tiles.retain(|_, t| match t {
                TileState::Ready(_, last) => f - *last < 120,
                _ => true,
            });
        }
        any || !self.loader.queue.lock().unwrap().is_empty()
    }

    pub fn road_colour(&self, r: &Road) -> Color32 {
        match self.road_colour {
            0 => {
                // speed in m/s
                let t = (r.speed / 35.0).clamp(0.0, 1.0);
                ramp(t)
            }
            1 => ramp(((r.lf.max(r.lt) as f32) / 4.0).clamp(0.0, 1.0)),
            _ => ramp((r.class as f32 / 4.0).clamp(0.0, 1.0)),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.frame += 1;
        self.ensure_loaded(&ctx);
        let tiles_busy = self.poll_tiles(&ctx);
        egui::Panel::left("map_side")
            .resizable(true)
            .default_size(290.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.side_ui(ui));
            });
        if self.selected.is_some() || !self.results.is_empty() {
            egui::Panel::right("map_inspect")
                .resizable(true)
                .default_size(330.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.inspect_ui(ui));
                });
        }
        let (rect, resp) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 32, 40));
        // pan / zoom
        let tool_drag =
            matches!(self.tool, Tool::BoxSelect) && resp.dragged_by(egui::PointerButton::Primary);
        if (resp.dragged_by(egui::PointerButton::Primary) && !tool_drag)
            || resp.dragged_by(egui::PointerButton::Secondary)
            || resp.dragged_by(egui::PointerButton::Middle)
        {
            let d = resp.drag_delta();
            self.cx -= d.x * self.mpp;
            self.cy += d.y * self.mpp;
        }
        if let Some(hp) = resp.hover_pos() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let before = self.s2w(rect, hp);
                self.mpp = (self.mpp * (1.0 - scroll * 0.002)).clamp(0.05, 80.0);
                let after = self.s2w(rect, hp);
                self.cx += before[0] - after[0];
                self.cy += before[1] - after[1];
            }
        }
        // raster layers
        self.draw_layer(ui, rect, self.base, 1.0);
        for k in 0..self.overlays.len() {
            let (on, op) = self.overlays[k];
            if on && k != self.base {
                self.draw_layer(ui, rect, k, op);
            }
        }
        if self.pending_search && self.data.is_some() {
            self.pending_search = false;
            self.do_search();
        }
        let Some(d) = self.data.clone() else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &self.status,
                egui::FontId::proportional(16.0),
                Color32::WHITE,
            );
            return;
        };
        let [wx0, wy1] = self.s2w(rect, rect.left_top());
        let [wx1, wy0] = self.s2w(rect, rect.right_bottom());
        let t = self.started.elapsed().as_secs_f32();
        // interiors
        if self.show_interiors && self.mpp < 30.0 {
            for v in &d.interiors {
                let (Ok(p), Ok(q), Ok(bb)) = (
                    serde_json::from_value::<[f32; 3]>(v["position"].clone()),
                    serde_json::from_value::<[f32; 4]>(v["rotation_world"].clone()),
                    serde_json::from_value::<[[f32; 3]; 2]>(v["bbox"].clone()),
                ) else {
                    continue;
                };
                if p[0] < wx0 - 200.0
                    || p[0] > wx1 + 200.0
                    || p[1] < wy0 - 200.0
                    || p[1] > wy1 + 200.0
                {
                    continue;
                }
                let yaw = {
                    let [x, y, z, w] = q;
                    (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z))
                };
                let c = mapbake::corners(p, yaw, [bb[0][0], bb[0][1]], [bb[1][0], bb[1][1]], 1.0);
                let pts: Vec<Pos2> = c.iter().map(|v| self.w2s(rect, v[0], v[1])).collect();
                painter.add(egui::Shape::convex_polygon(
                    pts,
                    Color32::from_rgba_unmultiplied(160, 90, 230, 50),
                    Stroke::new(1.2_f32, Color32::from_rgb(190, 120, 255)),
                ));
            }
        }
        // roads
        if self.show_roads {
            let major_only = self.mpp > 12.0;
            let mut n = 0;
            d.road_grid
                .query(wx0 - 300.0, wy0 - 300.0, wx1 + 300.0, wy1 + 300.0, |i| {
                    let r = &d.roads[i as usize];
                    if major_only && r.speed < 15.0 {
                        return;
                    }
                    n += 1;
                    let a = self.w2s(rect, r.a[0], r.a[1]);
                    let b = self.w2s(rect, r.b[0], r.b[1]);
                    let lanes = (r.lf + r.lt).max(1) as f32;
                    let w = if self.mpp < 1.5 {
                        (lanes * 3.4 / self.mpp).clamp(1.0, 40.0)
                    } else {
                        (0.6 + lanes * 0.5).min(3.0)
                    };
                    painter.line_segment([a, b], Stroke::new(w, self.road_colour(r)));
                    if self.show_arrows && self.mpp < 2.0 && (r.lf == 0) != (r.lt == 0) {
                        // one-way: arrow at the midpoint in the travel direction
                        let (s, e) = if r.lt > 0 { (a, b) } else { (b, a) };
                        let m = s + (e - s) * 0.5;
                        let dir = (e - s).normalized();
                        let nrm = Vec2::new(-dir.y, dir.x);
                        let sz = 6.0;
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                m + dir * sz,
                                m - dir * sz * 0.6 + nrm * sz * 0.6,
                                m - dir * sz * 0.6 - nrm * sz * 0.6,
                            ],
                            Color32::WHITE,
                            Stroke::NONE,
                        ));
                    }
                });
            let _ = n;
        }
        // junctions, stop / yield
        if self.show_junctions && self.mpp < 3.0 {
            for j in &d.junctions {
                if j.p[0] < wx0 || j.p[0] > wx1 || j.p[1] < wy0 || j.p[1] > wy1 {
                    continue;
                }
                let p = self.w2s(rect, j.p[0], j.p[1]);
                if !j.phases.is_empty() {
                    // signalised: cycle through the phases by their durations
                    let total: f32 = j.phases.iter().sum::<f32>().max(1.0);
                    let mut tt = if self.animate_phases { t % total } else { 0.0 };
                    let mut k = 0;
                    while k < j.phases.len() && tt > j.phases[k] {
                        tt -= j.phases[k];
                        k += 1;
                    }
                    let col = [
                        Color32::from_rgb(60, 220, 90),
                        Color32::from_rgb(240, 200, 40),
                        Color32::from_rgb(230, 60, 50),
                    ][k % 3];
                    painter.circle(p, 6.0, col, Stroke::new(1.5_f32, Color32::BLACK));
                    if self.mpp < 1.5 {
                        painter.text(
                            p + Vec2::new(8.0, -8.0),
                            egui::Align2::LEFT_BOTTOM,
                            format!("phase {}/{}", k + 1, j.phases.len()),
                            egui::FontId::proportional(10.0),
                            Color32::WHITE,
                        );
                    }
                } else {
                    painter.circle_stroke(
                        p,
                        4.0,
                        Stroke::new(1.5_f32, Color32::from_rgb(230, 230, 230)),
                    );
                }
                for (q, stop, yld) in &j.entrances {
                    if !stop && !yld {
                        continue;
                    }
                    let e = self.w2s(rect, q[0], q[1]);
                    if *stop {
                        painter.add(egui::Shape::convex_polygon(
                            (0..8)
                                .map(|k| {
                                    e + Vec2::angled(k as f32 * std::f32::consts::TAU / 8.0 + 0.39)
                                        * 5.0
                                })
                                .collect(),
                            Color32::from_rgb(210, 30, 30),
                            Stroke::new(1.0_f32, Color32::WHITE),
                        ));
                    } else {
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                e + Vec2::new(0.0, 5.0),
                                e + Vec2::new(-5.0, -4.0),
                                e + Vec2::new(5.0, -4.0),
                            ],
                            Color32::WHITE,
                            Stroke::new(1.5_f32, Color32::from_rgb(210, 30, 30)),
                        ));
                    }
                }
            }
        }
        // lights at the chosen hour
        if self.show_lights {
            let h = (self.hour.floor() as u32).min(23);
            let budget = 120_000usize;
            let mut seen = 0usize;
            let stride = if self.mpp > 8.0 {
                6
            } else if self.mpp > 3.0 {
                2
            } else {
                1
            };
            let r = if self.mpp < 1.0 {
                3.0
            } else if self.mpp < 4.0 {
                2.0
            } else {
                1.2
            };
            d.light_grid.query(wx0, wy0, wx1, wy1, |i| {
                if (i as usize) % stride != 0 || seen > budget {
                    return;
                }
                let l = &d.lights[i as usize];
                let interior = l.flags & 1 != 0;
                if (interior && !self.light_int) || (!interior && !self.light_ext) {
                    return;
                }
                if l.mask & (1 << h) == 0 || (l.flags & 2 != 0 && (7..19).contains(&h)) {
                    return;
                }
                seen += 1;
                let p = self.w2s(rect, l.pos[0], l.pos[1]);
                let c = l.rgb_type;
                painter.circle_filled(p, r, Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 230));
            });
        }
        // selected footprint, search hits
        if let Some(s) = self.selected {
            if let Some(e) = d.ents.get(s as usize) {
                let pts: Vec<Pos2> = d
                    .ent_corners(e)
                    .iter()
                    .map(|v| self.w2s(rect, v[0], v[1]))
                    .collect();
                painter.add(egui::Shape::closed_line(
                    pts,
                    Stroke::new(2.5_f32, Color32::from_rgb(0, 255, 255)),
                ));
            }
        }
        for &i in self.results.iter().take(500) {
            if let Some(e) = d.ents.get(i as usize) {
                painter.circle_stroke(
                    self.w2s(rect, e.pos[0], e.pos[1]),
                    5.0,
                    Stroke::new(1.5_f32, Color32::from_rgb(255, 0, 255)),
                );
            }
        }
        // hover: footprints of what is under the cursor
        let mut hover_ent = None;
        if let Some(hp) = resp.hover_pos() {
            let [x, y] = self.s2w(rect, hp);
            let mut best: Option<(f32, u32)> = None;
            if self.mpp < 8.0 {
                d.ent_grid
                    .query(x - 300.0, y - 300.0, x + 300.0, y + 300.0, |i| {
                        let e = &d.ents[i as usize];
                        if e.kind == 4 {
                            return;
                        }
                        if d.ent_contains(e, x, y) {
                            let a = (e.hi[0] - e.lo[0]) * (e.hi[1] - e.lo[1]);
                            if best.map(|b| a < b.0).unwrap_or(true) {
                                best = Some((a, i));
                            }
                        }
                    });
            }
            hover_ent = best.map(|b| b.1);
            if let (Some(i), true) = (hover_ent, self.show_footprints) {
                let e = &d.ents[i as usize];
                let pts: Vec<Pos2> = d
                    .ent_corners(e)
                    .iter()
                    .map(|v| self.w2s(rect, v[0], v[1]))
                    .collect();
                painter.add(egui::Shape::closed_line(
                    pts,
                    Stroke::new(1.5_f32, Color32::YELLOW),
                ));
            }
            let z = d.height(x, y);
            self.hover_info = format!(
                "game x {:.1}  y {:.1}  z {}   ·   glTF/BeamNG (x, z, −y) = ({:.1}, {}, {:.1})   ·   {:.2} m/px{}",
                x,
                y,
                z.map(|z| format!("{z:.1}")).unwrap_or("-".into()),
                x,
                z.map(|z| format!("{z:.1}")).unwrap_or("-".into()),
                -y,
                self.mpp,
                hover_ent.map(|i| format!("   ·   {}", d.name(d.ents[i as usize].name))).unwrap_or_default()
            );
        }
        // tools
        match self.tool {
            Tool::Inspect => {
                if resp.clicked() {
                    self.selected = hover_ent;
                }
            }
            Tool::Measure => {
                if resp.clicked() {
                    if let Some(hp) = resp.interact_pointer_pos() {
                        self.measure.push(self.s2w(rect, hp));
                    }
                }
                if resp.secondary_clicked() {
                    self.measure.clear();
                }
            }
            Tool::BoxSelect => {
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    self.drag_start = resp.interact_pointer_pos().map(|p| self.s2w(rect, p));
                }
                if let (Some(s), Some(p)) = (self.drag_start, resp.interact_pointer_pos()) {
                    if resp.dragged_by(egui::PointerButton::Primary) {
                        let e = self.s2w(rect, p);
                        self.box_sel = Some([
                            s[0].min(e[0]),
                            s[1].min(e[1]),
                            s[0].max(e[0]),
                            s[1].max(e[1]),
                        ]);
                    }
                }
                if resp.drag_stopped() {
                    self.drag_start = None;
                }
            }
        }
        if !self.measure.is_empty() {
            let pts: Vec<Pos2> = self
                .measure
                .iter()
                .map(|v| self.w2s(rect, v[0], v[1]))
                .collect();
            painter.add(egui::Shape::line(
                pts.clone(),
                Stroke::new(2.0_f32, Color32::from_rgb(255, 80, 200)),
            ));
            let mut total = 0.0;
            for k in 1..self.measure.len() {
                let (a, b) = (self.measure[k - 1], self.measure[k]);
                total += ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
            }
            for p in &pts {
                painter.circle_filled(*p, 3.5, Color32::from_rgb(255, 80, 200));
            }
            painter.text(
                *pts.last().unwrap() + Vec2::new(8.0, 0.0),
                egui::Align2::LEFT_CENTER,
                format!("{total:.1} m"),
                egui::FontId::proportional(14.0),
                Color32::WHITE,
            );
        }
        if let Some(b) = self.box_sel {
            let r = Rect::from_two_pos(self.w2s(rect, b[0], b[1]), self.w2s(rect, b[2], b[3]));
            painter.rect(
                r,
                0.0,
                Color32::from_rgba_unmultiplied(0, 200, 255, 30),
                Stroke::new(2.0_f32, Color32::from_rgb(0, 200, 255)),
                egui::StrokeKind::Middle,
            );
            painter.text(
                r.left_top() + Vec2::new(4.0, 4.0),
                egui::Align2::LEFT_TOP,
                format!(
                    "--bbox={:.0},{:.0},{:.0},{:.0}  ({:.0} × {:.0} m)",
                    b[0],
                    b[1],
                    b[2],
                    b[3],
                    b[2] - b[0],
                    b[3] - b[1]
                ),
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
        // scale bar + readout
        let bar_m = nice_len(self.mpp * 120.0);
        let bl = rect.left_bottom() + Vec2::new(12.0, -28.0);
        painter.line_segment(
            [bl, bl + Vec2::new(bar_m / self.mpp, 0.0)],
            Stroke::new(3.0_f32, Color32::WHITE),
        );
        painter.text(
            bl + Vec2::new(0.0, -4.0),
            egui::Align2::LEFT_BOTTOM,
            if bar_m >= 1000.0 {
                format!("{} km", bar_m / 1000.0)
            } else {
                format!("{bar_m} m")
            },
            egui::FontId::proportional(12.0),
            Color32::WHITE,
        );
        painter.text(
            rect.left_bottom() + Vec2::new(12.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            &self.hover_info,
            egui::FontId::monospace(11.0),
            Color32::from_white_alpha(220),
        );
        // north arrow
        let na = rect.right_top() + Vec2::new(-24.0, 30.0);
        painter.add(egui::Shape::convex_polygon(
            vec![
                na + Vec2::new(0.0, -14.0),
                na + Vec2::new(-7.0, 6.0),
                na + Vec2::new(7.0, 6.0),
            ],
            Color32::WHITE,
            Stroke::NONE,
        ));
        painter.text(
            na + Vec2::new(0.0, 10.0),
            egui::Align2::CENTER_TOP,
            "N",
            egui::FontId::proportional(11.0),
            Color32::WHITE,
        );
        if tiles_busy
            || (self.show_junctions && self.animate_phases && self.mpp < 6.0)
            || resp.dragged()
        {
            ctx.request_repaint();
        }
    }

    fn side_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(&self.status).small());
        ui.horizontal(|ui| {
            ui.label("Search");
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("name, model id or archetype hash")
                    .desired_width(170.0),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.do_search();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Go to x, y");
            let mut s = String::new();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut s)
                        .hint_text("-2800, -800")
                        .desired_width(120.0),
                )
                .lost_focus()
            {
                let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if v.len() >= 2 {
                    self.cx = v[0];
                    self.cy = v[1];
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Tool");
            ui.selectable_value(&mut self.tool, Tool::Inspect, "inspect");
            ui.selectable_value(&mut self.tool, Tool::Measure, "measure");
            ui.selectable_value(&mut self.tool, Tool::BoxSelect, "box select");
        });
        if self.tool == Tool::Measure {
            ui.label(egui::RichText::new("click to add points, right-click to clear").small());
        }
        if let Some(b) = self.box_sel {
            ui.label(format!(
                "box: {:.0},{:.0},{:.0},{:.0}",
                b[0], b[1], b[2], b[3]
            ));
            if ui.button("Export this region…").clicked() {
                self.export_req = Some(b);
            }
        }
        ui.separator();
        ui.heading("Base layer");
        for (k, (n, d)) in mapbake::LAYERS
            .iter()
            .enumerate()
            .filter(|(k, _)| [0usize, 1, 7].contains(k))
        {
            ui.radio_value(&mut self.base, k, *n).on_hover_text(*d);
        }
        ui.heading("Overlays");
        for k in 0..mapbake::LAYERS.len() {
            if k == self.base {
                continue;
            }
            let (n, d) = mapbake::LAYERS[k];
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.overlays[k].0, n).on_hover_text(d);
                if self.overlays[k].0 {
                    ui.add(egui::Slider::new(&mut self.overlays[k].1, 0.1..=1.0).show_value(false));
                }
            });
        }
        ui.separator();
        ui.heading("Vectors");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_roads, "lane graph");
            egui::ComboBox::from_id_salt("roadcol")
                .selected_text(ROAD_COLOURS[self.road_colour])
                .show_ui(ui, |ui| {
                    for (k, n) in ROAD_COLOURS.iter().enumerate() {
                        ui.selectable_value(&mut self.road_colour, k, *n);
                    }
                });
        });
        ui.checkbox(&mut self.show_arrows, "one-way arrows");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_junctions, "junctions, stop / yield");
            ui.checkbox(&mut self.animate_phases, "animate");
        });
        ui.checkbox(&mut self.show_interiors, "interiors");
        ui.checkbox(&mut self.show_footprints, "hover footprints");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_lights, "lights");
            ui.checkbox(&mut self.light_ext, "exterior");
            ui.checkbox(&mut self.light_int, "interior");
        });
        ui.add(egui::Slider::new(&mut self.hour, 0.0..=23.99).text("hour"))
            .on_hover_text(
                "hides lights whose time_flags hour bit is off, and night-only lights 07:00-19:00",
            );
        ui.separator();
        // legends
        if self.overlays[1].0 || self.base == 1 {
            if let Some(d) = &self.data {
                ui.collapsing("surface legend", |ui| {
                    for (n, c) in &d.surf_legend {
                        ui.horizontal(|ui| {
                            let (r, _) =
                                ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                            ui.painter()
                                .rect_filled(r, 2.0, Color32::from_rgb(c[0], c[1], c[2]));
                            ui.label(egui::RichText::new(n).small());
                        });
                    }
                });
            }
        }
        ui.collapsing("road colours", |ui| {
            for (k, t) in ["low", "", "", "", "high"].iter().enumerate() {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(30.0, 8.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 2.0, ramp(k as f32 / 4.0));
                    ui.label(*t);
                });
            }
            ui.label(egui::RichText::new("speed: 0 → 35 m/s · lanes: 0 → 4 · class 0 → 4").small());
        });
    }

    fn do_search(&mut self) {
        let Some(d) = &self.data else { return };
        let q = self.search.trim().to_lowercase();
        self.results.clear();
        if q.is_empty() {
            return;
        }
        let hash = u32::from_str_radix(q.trim_start_matches("0x"), 16)
            .ok()
            .filter(|_| q.len() == 8 || q.starts_with("0x"));
        let mut hit = vec![false; d.strings.len()];
        for (i, s) in d.strings.iter().enumerate() {
            if s.to_lowercase().contains(&q) {
                hit[i] = true;
            }
        }
        for (i, e) in d.ents.iter().enumerate() {
            if hit[e.name as usize] || hit[e.model as usize] || Some(e.arch) == hash {
                self.results.push(i as u32);
                if self.results.len() >= 2000 {
                    break;
                }
            }
        }
        if let Some(&f) = self.results.first() {
            let e = &d.ents[f as usize];
            self.cx = e.pos[0];
            self.cy = e.pos[1];
            self.selected = Some(f);
            self.mpp = self.mpp.min(1.0);
        }
    }

    fn inspect_ui(&mut self, ui: &mut egui::Ui) {
        let Some(d) = self.data.clone() else { return };
        if !self.results.is_empty() {
            ui.heading(format!("Search: {} hits", self.results.len()));
            egui::ScrollArea::vertical()
                .id_salt("hits")
                .max_height(220.0)
                .show(ui, |ui| {
                    for &i in self.results.iter().take(300) {
                        let e = &d.ents[i as usize];
                        if ui
                            .selectable_label(
                                self.selected == Some(i),
                                format!("{}  ({:.0}, {:.0})", d.name(e.name), e.pos[0], e.pos[1]),
                            )
                            .clicked()
                        {
                            self.selected = Some(i);
                            self.cx = e.pos[0];
                            self.cy = e.pos[1];
                        }
                    }
                });
            if ui.small_button("clear search").clicked() {
                self.results.clear();
            }
            ui.separator();
        }
        let Some(s) = self.selected else { return };
        let Some(e) = d.ents.get(s as usize) else {
            return;
        };
        ui.heading(d.name(e.name));
        let model = d.name(e.model).to_string();
        egui::Grid::new("insp").num_columns(2).show(ui, |ui| {
            let mut row = |k: &str, v: String| {
                ui.label(k);
                ui.label(egui::RichText::new(v).monospace());
                ui.end_row();
            };
            row("model", model.clone());
            row("archetype", format!("{:08x}", e.arch));
            row(
                "kind",
                KIND_NAMES.get(e.kind as usize).unwrap_or(&"?").to_string(),
            );
            row(
                "LOD level",
                ["HD", "LOD", "SLOD1", "SLOD2", "SLOD3", "SLOD4"]
                    .get(e.lod as usize)
                    .unwrap_or(&"?")
                    .to_string(),
            );
            row(
                "position",
                format!("{:.2}, {:.2}, {:.2}", e.pos[0], e.pos[1], e.pos[2]),
            );
            row(
                "glTF (x, z, −y)",
                format!("{:.2}, {:.2}, {:.2}", e.pos[0], e.pos[2], -e.pos[1]),
            );
            row("yaw", format!("{:.1}°", e.yaw.to_degrees()));
            row(
                "size",
                format!(
                    "{:.1} × {:.1} × {:.1} m",
                    e.hi[0] - e.lo[0],
                    e.hi[1] - e.lo[1],
                    e.hi[2] - e.lo[2]
                ),
            );
            row(
                "map file",
                d.files.get(e.file as usize).cloned().unwrap_or_default(),
            );
        });
        // lights in the footprint
        let mut nl = 0;
        let mut night = 0;
        d.light_grid.query(
            e.pos[0] - 200.0,
            e.pos[1] - 200.0,
            e.pos[0] + 200.0,
            e.pos[1] + 200.0,
            |i| {
                let l = &d.lights[i as usize];
                if d.ent_contains(e, l.pos[0], l.pos[1]) {
                    nl += 1;
                    if l.flags & 2 != 0 {
                        night += 1;
                    }
                }
            },
        );
        ui.label(format!(
            "lights inside the footprint: {nl} ({night} night-only)"
        ));
        ui.horizontal(|ui| {
            if !model.is_empty() && ui.button("🧊 Open in model viewer").clicked() {
                self.open_model = Some(model.clone());
            }
            if ui.button("copy id").clicked() {
                ui.ctx().copy_text(model.clone());
            }
            if ui.button("close").clicked() {
                self.selected = None;
            }
        });
    }
}

pub fn ramp(t: f32) -> Color32 {
    // blue -> cyan -> green -> yellow -> red
    let stops = [
        [40.0, 90.0, 230.0],
        [40.0, 200.0, 230.0],
        [60.0, 210.0, 90.0],
        [240.0, 220.0, 50.0],
        [235.0, 60.0, 40.0],
    ];
    let x = t.clamp(0.0, 1.0) * 4.0;
    let k = (x.floor() as usize).min(3);
    let f = x - k as f32;
    let a = stops[k];
    let b = stops[k + 1];
    Color32::from_rgb(
        (a[0] + (b[0] - a[0]) * f) as u8,
        (a[1] + (b[1] - a[1]) * f) as u8,
        (a[2] + (b[2] - a[2]) * f) as u8,
    )
}

fn nice_len(m: f32) -> f32 {
    let p = 10f32.powf(m.log10().floor());
    for k in [5.0, 2.0, 1.0] {
        if k * p <= m {
            return k * p;
        }
    }
    p
}
