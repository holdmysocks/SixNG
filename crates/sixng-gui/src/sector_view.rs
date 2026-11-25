//! Sector grid overlay for Map 2D and Map 3D: the cell lists and map-group
//! lists of `levels.rpf` (see `sixng_core::sectorgrid`), loaded on a
//! background thread the first time the overlay is switched on. Cell lists
//! are coloured by bundle key, map-group lists by map group id.

use crate::map2d::MapData;
use crate::render3d::LineV;
use eframe::egui::{self, Color32};
use sixng_core::export::Workspace;
use sixng_core::sectorgrid::{self as sg, Kind};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// One grid, ready to draw.
pub struct GridView {
    pub label: String,
    pub cell_m: f32,
    pub kind: Kind,
    /// Distinct cells with the indices of their records.
    pub cells: HashMap<(i32, i32), Vec<u32>>,
    pub sectors: Vec<sg::Sector>,
}

impl GridView {
    fn new(n: usize, id: &str, g: sg::SectorGrid) -> Self {
        let kind = g.kind();
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, s) in g.sectors.iter().enumerate() {
            cells.entry((s.x, s.y)).or_default().push(i as u32);
        }
        let what = match kind {
            Kind::Cells if g.cell_m < 100.0 && g.sectors.len() > 10_000 => "world cells",
            Kind::Cells => "cells",
            Kind::MapGroups => "map groups",
        };
        let off = id.rsplit('@').next().unwrap_or(id);
        GridView {
            label: format!("{n}: {:.0} m {what} ({} records, @{off})", g.cell_m, g.sectors.len()),
            cell_m: g.cell_m,
            kind,
            cells,
            sectors: g.sectors,
        }
    }

    /// Colour of a cell: its bundle (cell lists) or its first map group.
    pub fn colour(&self, recs: &[u32]) -> Color32 {
        let s = &self.sectors[recs[0] as usize];
        let h = match self.kind {
            Kind::Cells => crate::map3d::hash32(s.group),
            Kind::MapGroups => crate::map3d::hash32((s.hash64 ^ (s.hash64 >> 32)) as u32),
        };
        let hue = (h % 360) as f32 / 360.0;
        egui::ecolor::Hsva::new(hue, 0.75, 1.0, 1.0).into()
    }

    /// Text for one cell: its coordinates and every record in it.
    pub fn describe(&self, c: (i32, i32)) -> Option<String> {
        let recs = self.cells.get(&c)?;
        let mut s = format!("sector cell ({}, {}) · {:.0} m", c.0, c.1, self.cell_m);
        for &i in recs.iter().take(6) {
            let r = &self.sectors[i as usize];
            match self.kind {
                Kind::Cells => s += &format!(" · bundle {:08x} #{}", r.group, r.index),
                Kind::MapGroups => s += &format!(" · map group {:016x}", r.hash64),
            }
        }
        if recs.len() > 6 {
            s += &format!(" · +{} more", recs.len() - 6);
        }
        Some(s)
    }
}

enum Load {
    Idle,
    Busy,
    Done(Arc<Vec<GridView>>),
    Failed(String),
}

/// The overlay's state: on/off, which grid, and the loaded grids.
pub struct SectorView {
    game: String,
    work: String,
    state: Arc<Mutex<Load>>,
    pub on: bool,
    pub pick: usize,
    /// 3D lines near the camera: (eye x, eye y, pick) they were built for.
    cache: Option<([f32; 2], usize, Vec<LineV>)>,
}

impl SectorView {
    pub fn new(game: &str, work: &str) -> Self {
        SectorView {
            game: game.into(),
            work: work.into(),
            state: Arc::new(Mutex::new(Load::Idle)),
            on: false,
            pick: usize::MAX,
            cache: None,
        }
    }

    /// The grids once loaded; starts the load the first time it's asked while on.
    pub fn grids(&mut self) -> Option<Arc<Vec<GridView>>> {
        let mut st = self.state.lock().unwrap();
        match &*st {
            Load::Done(g) => {
                let g = g.clone();
                drop(st);
                if self.pick >= g.len() {
                    self.pick = (0..g.len()).max_by_key(|&i| g[i].sectors.len()).unwrap_or(0);
                }
                Some(g)
            }
            Load::Idle if self.on => {
                *st = Load::Busy;
                let (game, work, state) = (self.game.clone(), self.work.clone(), self.state.clone());
                std::thread::spawn(move || {
                    crate::mem::lower_thread_priority();
                    let ws = Workspace::new(&game, &work);
                    let r = sg::load_all(&ws, "levels").map(|v| {
                        Arc::new(v.into_iter().enumerate().map(|(n, (id, g))| GridView::new(n + 1, &id, g)).collect::<Vec<_>>())
                    });
                    *state.lock().unwrap() = match r {
                        Ok(g) if g.is_empty() => Load::Failed("no sector grids in levels.rpf".into()),
                        Ok(g) => Load::Done(g),
                        Err(e) => Load::Failed(format!("{e:#}")),
                    };
                });
                None
            }
            _ => None,
        }
    }

    /// Nothing left to load (for automation screenshots).
    pub fn idle(&self) -> bool {
        !self.on || matches!(&*self.state.lock().unwrap(), Load::Done(_) | Load::Failed(_))
    }

    pub fn grid(&mut self) -> Option<(Arc<Vec<GridView>>, usize)> {
        if !self.on {
            return None;
        }
        let g = self.grids()?;
        let p = self.pick.min(g.len() - 1);
        Some((g, p))
    }

    /// The toggle, the grid picker and the load status.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.on, "sector grid").on_hover_text(
            "The world sector grids of levels.rpf: cell lists coloured by bundle, map-group lists coloured by map group. Loaded from the archive the first time.",
        );
        if !self.on {
            return;
        }
        let grids = self.grids();
        match (&grids, &*self.state.lock().unwrap()) {
            (Some(g), _) => {
                let cur = g.get(self.pick).map_or("", |v| v.label.as_str());
                egui::ComboBox::from_id_salt(("sector_grid_pick", ui.id()))
                    .selected_text(cur)
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        for (i, v) in g.iter().enumerate() {
                            ui.selectable_value(&mut self.pick, i, &v.label);
                        }
                    });
            }
            (None, Load::Failed(e)) => {
                ui.colored_label(Color32::LIGHT_RED, format!("sector grid: {e}"));
            }
            _ => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("loading sector grids…");
                });
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            }
        }
    }

    /// 3D outlines of the cells within reach of the eye (game x, y), laid on
    /// the baked ground height. Each shared edge is drawn once.
    pub fn lines3d(&mut self, data: &MapData, eye: [f32; 2]) -> Vec<LineV> {
        let Some((grids, p)) = self.grid() else {
            return Vec::new();
        };
        let g = &grids[p];
        let reach = (g.cell_m * 25.0).clamp(1500.0, 16000.0);
        if let Some((e, cp, l)) = &self.cache {
            let d = ((e[0] - eye[0]).powi(2) + (e[1] - eye[1]).powi(2)).sqrt();
            if *cp == p && d < reach * 0.1 {
                return l.clone();
            }
        }
        let step = (g.cell_m / 8.0).clamp(8.0, 64.0);
        let n = (g.cell_m / step).ceil() as usize;
        // Far terrain is drawn coarser than the 8 m heights, so lift the lines
        // more with distance to keep them above it.
        let ground = |x: f32, y: f32| {
            let d = ((x - eye[0]).powi(2) + (y - eye[1]).powi(2)).sqrt();
            data.height(x, y).unwrap_or(0.0) + 1.5 + d * 0.006
        };
        let mut out = Vec::new();
        let mut edge = |a: [f32; 2], b: [f32; 2], c: Color32| {
            let col = [c.r(), c.g(), c.b(), 255];
            let mut prev = None;
            for k in 0..=n {
                let t = k as f32 / n as f32;
                let (x, y) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
                let v = crate::map3d::gl([x, y, ground(x, y)]);
                if let Some(q) = prev {
                    out.push(LineV { pos: q, col });
                    out.push(LineV { pos: v.into(), col });
                }
                prev = Some(v.into());
            }
        };
        let s = g.cell_m;
        for (&(cx, cy), recs) in &g.cells {
            let (x0, y0) = (cx as f32 * s, cy as f32 * s);
            if x0 + s < eye[0] - reach || x0 > eye[0] + reach || y0 + s < eye[1] - reach || y0 > eye[1] + reach {
                continue;
            }
            let c = g.colour(recs);
            let (x1, y1) = (x0 + s, y0 + s);
            edge([x0, y0], [x1, y0], c);
            edge([x0, y0], [x0, y1], c);
            if !g.cells.contains_key(&(cx + 1, cy)) {
                edge([x1, y0], [x1, y1], c);
            }
            if !g.cells.contains_key(&(cx, cy + 1)) {
                edge([x0, y1], [x1, y1], c);
            }
        }
        self.cache = Some((eye, p, out.clone()));
        out
    }
}
