//! The Liveries panel of the model viewer: place stickers on the loaded
//! vehicle by hand (see `livery`). A list of placed stickers; for the
//! chosen one its picture, the zones it lies on (clicked on the model or
//! ticked in a list), position, size, rotation and colours; a picker for
//! the game's sticker textures; save and load per vehicle.

use crate::glb;
use crate::livery::{self, Picture, State, Sticker, MAX_IMAGES, MAX_STICKERS};
use crate::render3d::Camera;
use eframe::egui;
use glam::{Vec2, Vec3};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What a click or drag on the model does while the panel is open.
#[derive(Clone, Copy, PartialEq, Default)]
pub enum Tool {
    /// The camera only.
    #[default]
    None,
    /// A click adds the zone under the pointer to the sticker, or takes it
    /// off.
    Zones,
    /// A drag that starts on the sticker's zones moves the sticker.
    Move,
}

/// A sticker record of the game, for the picker.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct GameSticker {
    /// The record's name hash.
    pub sticker: String,
    /// 64-bit texture id (16 hex digits).
    pub texture: String,
    pub mask: bool,
    /// sRGB.
    pub colours: [[u8; 3]; 3],
    pub aspect: u32,
}

enum Thumb {
    Wait,
    Ready(egui::TextureHandle, [u32; 2]),
    Failed(String),
}

struct ThumbJob {
    key: String,
    texture: String,
    mask: bool,
    colours: [[u8; 3]; 3],
}

type ThumbDone = (String, Result<(Vec<u8>, [usize; 2], [u32; 2]), String>);

/// Where the game, the work folder and the viewer's cache are.
pub struct Env<'a> {
    pub game: &'a str,
    pub work: &'a str,
    pub cache: &'a Path,
}

/// The panel's own state.
pub struct Editor {
    pub open: bool,
    pub selected: Option<usize>,
    pub tool: Tool,
    keep_aspect: bool,
    show_small: bool,
    pub picker_open: bool,
    /// The picker changes the chosen sticker's picture instead of adding one.
    picker_replace: bool,
    search: String,
    kind: u8,
    aspect: Option<u32>,
    list: Arc<Mutex<Option<Result<Arc<Vec<GameSticker>>, String>>>>,
    list_started: bool,
    filtered: Vec<usize>,
    filter_key: Option<(String, u8, Option<u32>)>,
    thumbs: HashMap<String, Thumb>,
    jobs: Arc<Mutex<Vec<ThumbJob>>>,
    done: Arc<Mutex<Vec<ThumbDone>>>,
    workers: bool,
    previews: HashMap<String, egui::TextureHandle>,
    /// While a sticker is dragged: its centre minus the UV under the pointer.
    grab: Option<Vec2>,
    /// Bounding boxes of the model's meshes, for picking.
    boxes: (std::path::PathBuf, usize, Vec<[Vec3; 2]>),
    /// Scripted checks (`--livery-click`, `--livery-drag`): a click that
    /// adds the zone at this place of the view (fractions of its width and
    /// height) to the chosen sticker, and a drag of the sticker to a place.
    /// `--livery-add ID[,ID]`: add game stickers by texture id first
    /// (`ID:mask` for a colour mask).
    pub auto_add: Vec<String>,
    /// `--livery-select N`: choose this sticker once the vehicle is in.
    pub auto_select: Option<usize>,
    /// The vehicle is complete, so the scripted steps may run.
    pub ready: bool,
    pub auto_click: Vec<[f32; 2]>,
    pub auto_drag: Option<[f32; 2]>,
    /// `--livery-save`: save once the scripted steps are done.
    pub auto_save: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Editor {
            open: false,
            selected: None,
            tool: Tool::None,
            keep_aspect: false,
            show_small: false,
            picker_open: false,
            picker_replace: false,
            search: String::new(),
            kind: 0,
            aspect: None,
            list: Default::default(),
            list_started: false,
            filtered: Vec::new(),
            filter_key: None,
            thumbs: HashMap::new(),
            jobs: Default::default(),
            done: Default::default(),
            workers: false,
            previews: HashMap::new(),
            grab: None,
            boxes: Default::default(),
            auto_add: Vec::new(),
            auto_select: None,
            ready: false,
            auto_click: Vec::new(),
            auto_drag: None,
            auto_save: false,
        }
    }
}

fn argb(hex: &str) -> [u8; 3] {
    let v = u32::from_str_radix(hex, 16).unwrap_or(0xffff_ffff);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// The game's sticker records, from the viewer's cache or (once, about 20
/// seconds) from the game files.
fn read_list(game: &str, work: &str, cache: &Path) -> Result<Vec<GameSticker>, String> {
    let file = cache.join("sticker_list_v1.json");
    if let Some(l) = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Vec<GameSticker>>(&b).ok()) {
        return Ok(l);
    }
    let ws = sixng_core::export::Workspace::new(game, work);
    let all = sixng_core::vehstickers::sticker_list(&ws).map_err(|e| format!("{e:#}"))?;
    let list: Vec<GameSticker> = all
        .iter()
        .filter_map(|s| {
            let texture = s["texture"].as_str()?.to_string();
            if sixng_core::vehstickers::parse_texture_id(&texture).is_none_or(|id| id == 0) {
                return None;
            }
            let c = |k: usize| argb(s["colours"][k].as_str().unwrap_or("ffffffff"));
            Some(GameSticker { sticker: s["sticker"].as_str()?.to_string(), texture, mask: s["mask"].as_bool().unwrap_or(false), colours: [c(0), c(1), c(2)], aspect: s["aspect"].as_u64().unwrap_or(0) as u32 })
        })
        .collect();
    let _ = std::fs::create_dir_all(cache);
    let _ = sixng_core::atomic::write(&file, serde_json::to_vec(&list).unwrap_or_default());
    Ok(list)
}

/// A picture as it will look on the vehicle, over a dark checker: a colour
/// mask takes its three colours, another picture its own colours and alpha.
fn compose(px: &[u8], w: usize, mask: bool, colours: &[[u8; 3]; 3]) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len());
    for (i, p) in px.chunks_exact(4).enumerate() {
        let (x, y) = (i % w.max(1), i / w.max(1));
        let bg = if (x / 8 + y / 8) % 2 == 0 { 120.0 } else { 100.0 };
        let (c, a) = if mask {
            let (r, g, b) = (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0);
            let sum = (r + g + b).max(1e-4);
            let c: [f32; 3] = std::array::from_fn(|k| (colours[0][k] as f32 * r + colours[1][k] as f32 * g + colours[2][k] as f32 * b) / sum);
            (c, sum.min(1.0))
        } else {
            ([p[0] as f32, p[1] as f32, p[2] as f32], p[3] as f32 / 255.0)
        };
        for k in 0..3 {
            out.push((bg * (1.0 - a) + c[k] * a) as u8);
        }
        out.push(255);
    }
    out
}

fn thumb_of(job: &ThumbJob, game: &Path, cache: &Path) -> Result<(Vec<u8>, [usize; 2], [u32; 2]), String> {
    let id = sixng_core::vehstickers::parse_texture_id(&job.texture).ok_or("not a texture id")?;
    let (file, size) = sixng_core::vehstickers::sticker_png(game, id, &cache.join("stickers")).map_err(|e| format!("{e:#}"))?;
    let img = image::open(&file).map_err(|e| e.to_string())?.to_rgba8();
    let k = 128.0 / size[0].max(size[1]).max(1) as f32;
    let (w, h) = (((size[0] as f32 * k) as u32).max(1), ((size[1] as f32 * k) as u32).max(1));
    let small = image::imageops::thumbnail(&img, w, h);
    Ok((compose(small.as_raw(), w as usize, job.mask, &job.colours), [w as usize, h as usize], size))
}

impl Editor {
    /// Still reading the sticker list or drawing thumbnails (a scripted
    /// screenshot waits for it).
    pub fn busy(&self) -> bool {
        self.open && self.picker_open && (self.list.lock().map(|l| l.is_none()).unwrap_or(false) || self.thumbs.is_empty() || self.thumbs.values().any(|t| matches!(t, Thumb::Wait))) || self.scripting()
    }

    /// Scripted steps are still to run.
    fn scripting(&self) -> bool {
        self.auto_select.is_some() || !self.auto_add.is_empty() || !self.auto_click.is_empty() || self.auto_drag.is_some() || self.auto_save
    }

    fn start_list(&mut self, env: &Env, ctx: &egui::Context) {
        if self.list_started {
            return;
        }
        self.list_started = true;
        let (game, work, cache, slot, ctx) = (env.game.to_string(), env.work.to_string(), env.cache.to_path_buf(), self.list.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = read_list(&game, &work, &cache).map(Arc::new);
            *slot.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    fn want_thumb(&mut self, s: &GameSticker, env: &Env, ctx: &egui::Context) {
        if self.thumbs.contains_key(&s.sticker) {
            return;
        }
        self.thumbs.insert(s.sticker.clone(), Thumb::Wait);
        self.jobs.lock().unwrap().push(ThumbJob { key: s.sticker.clone(), texture: s.texture.clone(), mask: s.mask, colours: s.colours });
        if !self.workers {
            self.workers = true;
            for _ in 0..4 {
                let (jobs, done, game, cache, ctx) = (self.jobs.clone(), self.done.clone(), PathBuf::from(env.game), env.cache.to_path_buf(), ctx.clone());
                std::thread::spawn(move || loop {
                    // the editor is gone when only the workers hold the queue
                    if Arc::strong_count(&jobs) <= 4 {
                        break;
                    }
                    // newest first: what the user is looking at now
                    let job = jobs.lock().unwrap().pop();
                    match job {
                        Some(j) => {
                            let r = thumb_of(&j, &game, &cache);
                            done.lock().unwrap().push((j.key, r));
                            ctx.request_repaint();
                        }
                        None => std::thread::sleep(std::time::Duration::from_millis(30)),
                    }
                });
            }
        }
    }

    fn take_thumbs(&mut self, ctx: &egui::Context) {
        let done: Vec<ThumbDone> = std::mem::take(&mut *self.done.lock().unwrap());
        for (key, r) in done {
            let t = match r {
                Ok((px, size, full)) => Thumb::Ready(ctx.load_texture(format!("sticker_{key}"), egui::ColorImage::from_rgba_unmultiplied(size, &px), egui::TextureOptions::LINEAR), full),
                Err(e) => Thumb::Failed(e),
            };
            self.thumbs.insert(key, t);
        }
    }
}

fn model_label(name: &str) -> String {
    if name.is_empty() { "the vehicle itself".to_string() } else { name.trim_end_matches("_hi").to_string() }
}

fn zone_label(z: &livery::Zone) -> String {
    let rest = if z.zone == 63 { " (rest of the model)" } else { "" };
    format!("{} · {} · {:.2} m²{rest}", z.zone, z.side(), z.area)
}

/// The user moved this slider or typed into it (a slider also reports a
/// change when it only rounds the number it shows).
fn by_hand(r: &egui::Response) -> bool {
    r.dragged() || r.changed() && r.ctx.egui_wants_keyboard_input()
}

enum Act {
    Select(usize),
    Up(usize),
    Down(usize),
    Duplicate(usize),
    Delete(usize),
}

impl State {
    fn changed(&mut self) {
        self.dirty = true;
    }

    /// A first placement of sticker `i` on its zones.
    fn fit(&mut self, i: usize, env: &Env) {
        let Some(s) = self.livery.stickers.get(i) else { return };
        let Some(set) = self.zones.slot_of(&s.model).and_then(|slot| self.zones.set(slot, &s.zones, s.uv_set.min(1) as usize)) else { return };
        let pic = s.picture.clone();
        let aspect = self.picture_size(&pic, Path::new(env.game), env.cache).map_or(1.0, |sz| sz[0] as f32 / sz[1].max(1) as f32);
        let (c, size) = set.fit(aspect);
        let s = &mut self.livery.stickers[i];
        s.centre = c;
        s.size = size;
        self.changed();
    }

    /// Add a sticker with this picture; it takes the zones of the chosen
    /// sticker (a second logo on the same wall), else none yet.
    fn add(&mut self, picture: Picture, name: String, mask: bool, colours: Option<[[u8; 3]; 3]>, env: &Env) {
        if self.livery.stickers.len() >= MAX_STICKERS {
            self.status = format!("A vehicle takes up to {MAX_STICKERS} stickers.");
            return;
        }
        let mut s = Sticker::new(picture);
        s.name = name;
        s.mask = mask;
        if let Some(c) = colours {
            s.colours = c;
        }
        if let Some(from) = self.ed.selected.and_then(|i| self.livery.stickers.get(i)) {
            s.model = from.model.clone();
            s.zones = from.zones.clone();
            s.uv_set = from.uv_set;
        }
        let has_zones = !s.zones.is_empty();
        self.livery.stickers.push(s);
        let i = self.livery.stickers.len() - 1;
        self.ed.selected = Some(i);
        if has_zones {
            self.fit(i, env);
            self.ed.tool = Tool::Move;
        } else {
            self.ed.tool = Tool::Zones;
            self.status = "Click the panels of the vehicle the sticker goes on.".into();
        }
        self.changed();
    }

    /// Add the zone to sticker `i`, or take it off. A zone of another model
    /// moves the sticker to that model.
    fn toggle_zone(&mut self, i: usize, slot: u8, zone: u8, env: &Env) {
        let Some(model) = self.zones.models.get(slot as usize).cloned() else { return };
        let Some(s) = self.livery.stickers.get_mut(i) else { return };
        if s.model != model {
            s.model = model;
            s.zones.clear();
        }
        let first = s.zones.is_empty() || s.fresh;
        match s.zones.iter().position(|z| *z == zone) {
            Some(k) => {
                s.zones.remove(k);
            }
            None => {
                s.zones.push(zone);
                s.zones.sort_unstable();
            }
        }
        if first {
            self.fit(i, env);
        }
        self.changed();
    }

    fn save_to(&mut self, file: &Path) {
        match livery::save(file, &self.livery) {
            Ok(()) => {
                self.saved = self.livery.clone();
                self.status = format!("saved {} sticker(s) to {}", self.livery.stickers.len(), file.display());
            }
            Err(e) => self.status = format!("not saved: {e:#}"),
        }
    }

    /// The Liveries window and, when asked for, the sticker picker.
    pub fn panel(&mut self, ctx: &egui::Context, within: egui::Rect, env: &Env) {
        let selected = if self.ed.open { self.ed.selected.filter(|i| *i < self.livery.stickers.len()) } else { None };
        self.ed.selected = self.ed.selected.filter(|i| *i < self.livery.stickers.len());
        let tint = selected.is_some() && self.ed.tool == Tool::Zones;
        if self.highlight.selected != selected || self.highlight.tint != tint {
            self.highlight.selected = selected;
            self.highlight.tint = tint;
            self.changed();
        }
        if !self.ed.open {
            if self.highlight.hover.take().is_some() {
                self.changed();
            }
            return;
        }
        let mut open = true;
        egui::Window::new("Liveries").open(&mut open).default_pos(within.left_top() + egui::vec2(12.0, 12.0)).default_size([340.0, (within.height() - 70.0).clamp(300.0, 760.0)]).constrain_to(within).resizable(true).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height((within.height() - 80.0).max(200.0)).show(ui, |ui| self.panel_body(ui, env));
        });
        self.ed.open = open;
        if self.ed.picker_open {
            self.picker(ctx, within, env);
        }
    }

    fn panel_body(&mut self, ui: &mut egui::Ui, env: &Env) {
        let file = self.file(Path::new(env.work));
        ui.horizontal_wrapped(|ui| {
            if ui.button("➕ Game sticker…").on_hover_text("Pick one of the game's sticker textures").clicked() {
                self.ed.picker_open = true;
                self.ed.picker_replace = false;
            }
            if ui.button("➕ PNG file…").on_hover_text("Use a picture of your own (PNG; its transparency is kept)").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("picture", &["png", "jpg", "jpeg"]).pick_file() {
                    let name = p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.add(Picture::File(p), name, false, None, env);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            let unsaved = self.unsaved();
            if ui.add_enabled(unsaved, egui::Button::new("💾 Save")).on_hover_text(format!("Save this vehicle's stickers to {}; they are shown again when the vehicle is opened", file.display())).clicked() {
                self.save_to(&file);
            }
            if ui.add_enabled(file.exists(), egui::Button::new("↺ Load")).on_hover_text("Read the saved stickers again (changes since the last save are dropped)").clicked() {
                // reads the file again on the next frame
                self.vehicle.clear();
            }
            if ui.add_enabled(!self.livery.stickers.is_empty(), egui::Button::new("Clear")).on_hover_text("Take every sticker off. The saved file changes only when you save.").clicked() {
                self.livery.stickers.clear();
                self.ed.selected = None;
                self.changed();
            }
            ui.menu_button("File", |ui| {
                if ui.button("Save a copy…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("livery", &["json"]).set_file_name(format!("{}.json", self.vehicle)).save_file() {
                        let _ = livery::save(&p, &self.livery).map_err(|e| self.status = format!("not saved: {e:#}"));
                    }
                    ui.close();
                }
                if ui.button("Open a livery file…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("livery", &["json"]).pick_file() {
                        match livery::load(&p) {
                            Ok(l) => {
                                self.livery.stickers = l.stickers;
                                self.ed.selected = None;
                                self.status = format!("opened {}", p.display());
                                self.changed();
                            }
                            Err(e) => self.status = format!("{}: {e:#}", p.display()),
                        }
                    }
                    ui.close();
                }
            });
            if unsaved {
                ui.label(egui::RichText::new("not saved").weak());
            }
        });
        if !self.status.is_empty() {
            ui.label(egui::RichText::new(&self.status).small());
        }
        if self.zones.zones.keys().all(|(_, z)| *z == 0) {
            ui.colored_label(egui::Color32::from_rgb(230, 170, 60), "This model has no sticker zones, so stickers have nowhere to go.");
        }
        ui.separator();
        // the placed stickers
        let pictures = {
            let mut p: Vec<&Picture> = Vec::new();
            for s in &self.livery.stickers {
                if !p.contains(&&s.picture) {
                    p.push(&s.picture);
                }
            }
            p.len()
        };
        ui.label(format!("{} of {MAX_STICKERS} stickers · {pictures} of {MAX_IMAGES} pictures", self.livery.stickers.len()))
            .on_hover_text("Stickers lower in the list lie on top. The limits are this viewer's: the game's own shader takes three stickers per zone; here any number of the list may share a zone.");
        let mut act = None;
        let n = self.livery.stickers.len();
        for i in 0..n {
            let fitted = self.zones.slot_of(&self.livery.stickers[i].model).is_some();
            let err = self.picture_error(&self.livery.stickers[i].picture).map(str::to_string);
            let s = &mut self.livery.stickers[i];
            ui.horizontal(|ui| {
                if ui.checkbox(&mut s.visible, "").on_hover_text("Shown").changed() {
                    self.dirty = true;
                }
                let name = if s.name.is_empty() { s.picture.label() } else { s.name.clone() };
                let mut text = egui::RichText::new(format!("{}. {name}", i + 1));
                if !fitted || err.is_some() || s.zones.is_empty() {
                    text = text.color(egui::Color32::from_rgb(230, 170, 60));
                }
                let r = ui.selectable_label(self.ed.selected == Some(i), text);
                let r = if !fitted {
                    r.on_hover_text(format!("Its part is not fitted now: {}", s.model))
                } else if let Some(e) = &err {
                    r.on_hover_text(format!("The picture could not be read: {e}"))
                } else if s.zones.is_empty() {
                    r.on_hover_text("No zones chosen yet")
                } else {
                    r.on_hover_text(format!("on {} · zones {:?}", model_label(&s.model), s.zones))
                };
                if r.clicked() {
                    act = Some(Act::Select(i));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                        act = Some(Act::Delete(i));
                    }
                    if ui.small_button("⧉").on_hover_text("Duplicate").clicked() {
                        act = Some(Act::Duplicate(i));
                    }
                    if ui.add_enabled(i + 1 < n, egui::Button::new("⏷").small()).on_hover_text("Move down (on top of the next)").clicked() {
                        act = Some(Act::Down(i));
                    }
                    if ui.add_enabled(i > 0, egui::Button::new("⏶").small()).on_hover_text("Move up (under the previous)").clicked() {
                        act = Some(Act::Up(i));
                    }
                });
            });
        }
        match act {
            Some(Act::Select(i)) => self.ed.selected = if self.ed.selected == Some(i) { None } else { Some(i) },
            Some(Act::Up(i)) => {
                self.livery.stickers.swap(i, i - 1);
                self.ed.selected = Some(i - 1);
                self.changed();
            }
            Some(Act::Down(i)) => {
                self.livery.stickers.swap(i, i + 1);
                self.ed.selected = Some(i + 1);
                self.changed();
            }
            Some(Act::Duplicate(i)) => {
                if self.livery.stickers.len() < MAX_STICKERS {
                    let mut s = self.livery.stickers[i].clone();
                    if !s.name.is_empty() {
                        s.name += " copy";
                    }
                    // a little to the side, so the copy is seen
                    s.size[0] = s.size[0].max(0.01);
                    self.livery.stickers.insert(i + 1, s);
                    self.ed.selected = Some(i + 1);
                    self.ed.tool = Tool::Move;
                    self.changed();
                } else {
                    self.status = format!("A vehicle takes up to {MAX_STICKERS} stickers.");
                }
            }
            Some(Act::Delete(i)) => {
                self.livery.stickers.remove(i);
                self.ed.selected = None;
                self.changed();
            }
            None => {}
        }
        if self.livery.stickers.is_empty() {
            ui.label(egui::RichText::new("No stickers yet. Add one, then click the panels of the vehicle it goes on.").weak());
        }
        let Some(i) = self.ed.selected.filter(|i| *i < self.livery.stickers.len()) else { return };
        ui.separator();
        self.sticker_ui(ui, i, env);
    }

    /// The controls of the chosen sticker.
    fn sticker_ui(&mut self, ui: &mut egui::Ui, i: usize, env: &Env) {
        let before = self.livery.stickers[i].clone();
        // picture
        let pic = before.picture.clone();
        let size = self.picture_size(&pic, Path::new(env.game), env.cache);
        ui.horizontal(|ui| {
            ui.label("name");
            ui.text_edit_singleline(&mut self.livery.stickers[i].name);
        });
        ui.horizontal(|ui| {
            if let Some(t) = self.preview(ui.ctx(), i, env) {
                // the preview is square like the picture's layer: show it in the picture's own shape
                let a = size.map_or(1.0, |sz| sz[0] as f32 / sz[1].max(1) as f32);
                let d = if a >= 1.0 { egui::vec2(96.0, 96.0 / a) } else { egui::vec2(96.0 * a, 96.0) };
                ui.image((t.id(), d));
            }
            ui.vertical(|ui| {
                match (&pic, size) {
                    (Picture::Game(id), Some(sz)) => ui.label(format!("game texture {id}\n{} × {} px", sz[0], sz[1])),
                    (Picture::File(p), Some(sz)) => ui.label(format!("{}\n{} × {} px", p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), sz[0], sz[1])),
                    (p, None) => ui.colored_label(egui::Color32::from_rgb(230, 90, 70), format!("{}: {}", p.label(), self.picture_error(p).unwrap_or("not read"))),
                };
                ui.horizontal(|ui| {
                    if ui.button("Change…").on_hover_text("Pick another game sticker for this placement").clicked() {
                        self.ed.picker_open = true;
                        self.ed.picker_replace = true;
                    }
                    if matches!(pic, Picture::File(_)) && ui.button("Read again").on_hover_text("Read the file again after you changed it").clicked() {
                        self.forget(&pic);
                        self.ed.previews.clear();
                    }
                });
            });
        });
        ui.separator();
        // where
        ui.label(egui::RichText::new("Panels").strong());
        ui.horizontal_wrapped(|ui| {
            let on = self.ed.tool == Tool::Zones;
            if ui.selectable_label(on, "🖱 Click panels on the model").on_hover_text("Click a panel of the vehicle to put the sticker on it; click it again to take it off. A sticker can lie on several panels of one model (a box wall is six). The panels it lies on are tinted blue.").clicked() {
                self.ed.tool = if on { Tool::None } else { Tool::Zones };
            }
            if ui.button("Fit").on_hover_text("Put the sticker in the middle of its panels, as large as fits").clicked() {
                self.fit(i, env);
                self.livery.stickers[i].fresh = true;
            }
        });
        let models = self.zones.models.clone();
        let cur = self.livery.stickers[i].model.clone();
        let mut seen = std::collections::HashSet::new();
        egui::ComboBox::from_label("model").selected_text(model_label(&cur)).width(210.0).show_ui(ui, |ui| {
            for (slot, m) in models.iter().enumerate() {
                // a part fitted twice is one entry; parts without zones other than "rest" are left out
                if !seen.insert(m.clone()) || !self.zones.zones.keys().any(|(s, z)| *s as usize == slot && *z != 63 && *z != 0) {
                    continue;
                }
                if ui.selectable_label(cur == *m, model_label(m)).clicked() && cur != *m {
                    let s = &mut self.livery.stickers[i];
                    s.model = m.clone();
                    s.zones.clear();
                }
            }
        })
        .response
        .on_hover_text("Zones are numbered per model: the vehicle itself and each fitted part (a box, a bed) have their own.");
        if self.zones.slot_of(&cur).is_none() {
            ui.colored_label(egui::Color32::from_rgb(230, 170, 60), format!("{} is not fitted now, so this sticker is not drawn.", model_label(&cur)));
        }
        let zs = self.livery.stickers[i].zones.clone();
        ui.label(if zs.is_empty() { "zones: none yet".to_string() } else { format!("zones: {}", zs.iter().map(|z| z.to_string()).collect::<Vec<_>>().join(", ")) });
        if let Some(slot) = self.zones.slot_of(&cur) {
            egui::CollapsingHeader::new("All zones of this model").id_salt("livery_zones").show(ui, |ui| {
                ui.checkbox(&mut self.ed.show_small, "also the small ones (under 0.05 m²)");
                let mut list: Vec<livery::Zone> = self.zones.zones.values().filter(|z| z.slot == slot && (self.ed.show_small || z.area >= 0.05 || zs.contains(&z.zone))).cloned().collect();
                list.sort_by(|a, b| (a.zone == 63).cmp(&(b.zone == 63)).then(b.area.total_cmp(&a.area)));
                let mut hover = None;
                egui::ScrollArea::vertical().id_salt("livery_zone_list").max_height(170.0).show(ui, |ui| {
                    for z in &list {
                        let mut on = zs.contains(&z.zone);
                        let r = ui.checkbox(&mut on, zone_label(z));
                        if r.hovered() {
                            hover = Some((z.slot, z.zone));
                        }
                        if r.changed() {
                            self.toggle_zone(i, z.slot, z.zone, env);
                        }
                    }
                });
                if self.ed.tool != Tool::Zones && self.highlight.hover != hover {
                    self.highlight.hover = hover;
                    self.dirty = true;
                }
            });
        }
        let set = {
            let s = &self.livery.stickers[i];
            self.zones.slot_of(&s.model).and_then(|slot| self.zones.set(slot, &s.zones, s.uv_set.min(1) as usize))
        };
        ui.horizontal(|ui| {
            ui.label("UV set");
            let s = &mut self.livery.stickers[i];
            ui.selectable_value(&mut s.uv_set, 0, "first");
            ui.selectable_value(&mut s.uv_set, 1, "second").on_hover_text("The game places most of its stickers in the second UV set: on box walls it is a flat unwrap at one scale.");
        });
        ui.separator();
        // placement
        ui.label(egui::RichText::new("Place").strong());
        let on = self.ed.tool == Tool::Move;
        if ui.selectable_label(on, "✋ Drag it on the model").on_hover_text("Drag the sticker over its panels. With the pointer on the model: Ctrl + wheel changes its size, Shift + wheel stretches its width, Ctrl + Shift + wheel its height, Alt + wheel turns it.").clicked() {
            self.ed.tool = if on { Tool::None } else { Tool::Move };
        }
        let aspect = size.map_or(1.0, |sz| sz[0] as f32 / sz[1].max(1) as f32);
        let keep = self.ed.keep_aspect;
        let s = &mut self.livery.stickers[i];
        if let Some(set) = &set {
            // position across and down the panel, metres from the middle of the zones
            let mut p = set.to_panel(Vec2::from(s.centre));
            let reach = set.reach().max(0.5) + s.size[0].max(s.size[1]);
            let mut moved = false;
            moved |= by_hand(&ui.add(egui::Slider::new(&mut p.x, -reach..=reach).text("across (m)").fixed_decimals(3)));
            moved |= by_hand(&ui.add(egui::Slider::new(&mut p.y, -reach..=reach).text("down (m)").fixed_decimals(3)));
            if moved {
                s.centre = set.from_panel(p).to_array();
                s.fresh = false;
            }
        } else {
            ui.label(egui::RichText::new("Choose zones first: the sticker is placed on them.").weak());
        }
        // width and height are set apart (a sticker can be stretched either
        // way) unless the proportions are locked
        let size0 = s.size;
        let w = by_hand(&ui.add(egui::Slider::new(&mut s.size[0], 0.02..=12.0).logarithmic(true).text("width (m)").fixed_decimals(3)));
        let h = by_hand(&ui.add(egui::Slider::new(&mut s.size[1], 0.02..=12.0).logarithmic(true).text("height (m)").fixed_decimals(3)));
        if w && keep {
            s.size[1] = s.size[0] / aspect;
        } else if h && keep {
            s.size[0] = s.size[1] * aspect;
        }
        if w || h {
            s.fresh = false;
        } else {
            // a slider shows a rounded number; the stored one stays as it is
            s.size = size0;
        }
        ui.horizontal(|ui| {
            if ui.checkbox(&mut self.ed.keep_aspect, "lock proportions").on_hover_text("Off: width and height are set apart, so the sticker can be stretched either way. On: changing one changes the other to keep the picture's own shape.").changed() && self.ed.keep_aspect {
                s.size[1] = s.size[0] / aspect;
            }
            if ui.small_button("picture's shape").on_hover_text("Undo a stretch: set the height from the width and the picture's own proportions").clicked() {
                s.size[1] = s.size[0] / aspect;
            }
            ui.label(egui::RichText::new(format!("stretch ×{:.2}", s.size[0] / (s.size[1] * aspect).max(1e-6))).weak().small()).on_hover_text("Width against height, compared with the picture's own shape (1.00 = not stretched)");
        });
        let rot0 = s.rotation;
        if !by_hand(&ui.add(egui::Slider::new(&mut s.rotation, -180.0..=180.0).text("rotation (°)").fixed_decimals(1))) {
            s.rotation = rot0;
        }
        ui.horizontal(|ui| {
            for a in [0.0, 90.0, 180.0, -90.0] {
                if ui.small_button(format!("{a:.0}°")).clicked() {
                    s.rotation = a;
                }
            }
            ui.checkbox(&mut s.mirror, "mirror");
        });
        ui.separator();
        // colours
        ui.label(egui::RichText::new("Colours").strong());
        ui.checkbox(&mut s.mask, "colour mask").on_hover_text("The picture's red, green and blue each take one colour (most of the game's sticker textures are made this way). Off: the picture's own colours and transparency, times the tint.");
        ui.horizontal(|ui| {
            if s.mask {
                for (k, n) in ["red", "green", "blue"].iter().enumerate() {
                    ui.color_edit_button_srgb(&mut s.colours[k]).on_hover_text(format!("Colour of the mask's {n}"));
                }
            } else {
                ui.color_edit_button_srgb(&mut s.colours[0]).on_hover_text("Tint (white = the picture's own colours)");
                ui.label("tint");
            }
        });
        ui.add(egui::Slider::new(&mut s.opacity, 0.0..=1.0).text("opacity"));
        if let Some(m) = s.shader_uv {
            ui.label(egui::RichText::new(format!("a {:.4}  b {:.4}  ou {:.4}\nc {:.4}  d {:.4}  ov {:.4}", m[0], m[1], m[4], m[2], m[3], m[5])).monospace().small())
                .on_hover_text("The six numbers the shader gets, as the game's sticker records hold them: u = U·a + V·b + ou, v = U·c + V·d + ov, in the chosen UV set. The sticker is drawn where both are between 0 and 1.");
        }
        if let Some(set) = &set {
            let k = set.scale();
            ui.label(egui::RichText::new(format!("these panels: {:.2} m per UV unit along U, {:.2} along V", k.x, k.y)).weak().small());
        }
        if self.livery.stickers[i] != before {
            if self.livery.stickers[i].mask != before.mask || self.livery.stickers[i].colours != before.colours {
                self.ed.previews.clear();
            }
            self.changed();
        }
    }

    /// A small picture of sticker `i` as it will look.
    fn preview(&mut self, ctx: &egui::Context, i: usize, env: &Env) -> Option<egui::TextureHandle> {
        let s = self.livery.stickers.get(i)?.clone();
        let key = format!("{}|{}|{:?}", s.picture.key(), s.mask, s.colours);
        if let Some(t) = self.ed.previews.get(&key) {
            return Some(t.clone());
        }
        let (px, w) = self.layer_small(&s.picture, Path::new(env.game), env.cache)?;
        let img = egui::ColorImage::from_rgba_unmultiplied([w, px.len() / 4 / w], &compose(&px, w, s.mask, &s.colours));
        let t = ctx.load_texture(format!("livery_preview_{}", self.ed.previews.len()), img, egui::TextureOptions::LINEAR);
        if self.ed.previews.len() > 64 {
            self.ed.previews.clear();
        }
        self.ed.previews.insert(key, t.clone());
        Some(t)
    }

    /// The picker of the game's sticker textures.
    fn picker(&mut self, ctx: &egui::Context, within: egui::Rect, env: &Env) {
        self.ed.start_list(env, ctx);
        self.ed.take_thumbs(ctx);
        let list = self.ed.list.lock().unwrap().clone();
        let mut open = true;
        let mut chosen: Option<GameSticker> = None;
        let title = if self.ed.picker_replace { "Game stickers: change the picture" } else { "Game stickers: add one" };
        egui::Window::new(title).id(egui::Id::new("livery_picker")).open(&mut open).default_pos(within.left_top() + egui::vec2(370.0, 12.0)).default_size([500.0, 520.0]).constrain_to(within).show(ctx, |ui| {
            let list = match list {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Reading the game's sticker records (about 20 seconds, once).");
                    });
                    return;
                }
                Some(Err(e)) => {
                    ui.colored_label(egui::Color32::from_rgb(230, 90, 70), e);
                    return;
                }
                Some(Ok(l)) => l,
            };
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.ed.search).hint_text("search: texture id or sticker hash").desired_width(210.0));
                for (k, n) in ["all", "colour masks", "pictures"].iter().enumerate() {
                    ui.selectable_value(&mut self.ed.kind, k as u8, *n);
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("shape");
                ui.selectable_value(&mut self.ed.aspect, None, "any");
                for (k, n) in [(0, "1:1"), (1, "2:1"), (2, "4:1"), (3, "8:1")] {
                    ui.selectable_value(&mut self.ed.aspect, Some(k), n).on_hover_text("The record's aspect class");
                }
                ui.selectable_value(&mut self.ed.aspect, Some(4), "other");
            });
            let key = (self.ed.search.trim().to_lowercase(), self.ed.kind, self.ed.aspect);
            if self.ed.filter_key.as_ref() != Some(&key) {
                self.ed.filtered = list
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| (key.0.is_empty() || s.texture.contains(&key.0) || s.sticker.contains(&key.0)) && (key.1 == 0 || (key.1 == 1) == s.mask) && key.2.is_none_or(|a| if a >= 4 { s.aspect >= 4 } else { s.aspect == a }))
                    .map(|(i, _)| i)
                    .collect();
                self.ed.filter_key = Some(key);
            }
            ui.label(egui::RichText::new(format!("{} of {} sticker records. The game's files give them no names, so they are found by eye.", self.ed.filtered.len(), list.len())).small().weak());
            let cell = 112.0;
            let cols = ((ui.available_width() + 4.0) / (cell + 4.0)).floor().max(1.0) as usize;
            let rows = self.ed.filtered.len().div_ceil(cols);
            egui::ScrollArea::vertical().id_salt("livery_picker_grid").auto_shrink([false, false]).show_rows(ui, cell + 4.0, rows, |ui, range| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for row in range {
                    ui.horizontal(|ui| {
                        for k in row * cols..((row + 1) * cols).min(self.ed.filtered.len()) {
                            let s = &list[self.ed.filtered[k]];
                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(cell, cell), egui::Sense::click());
                            ui.painter().rect_filled(rect, 4.0, if resp.hovered() { egui::Color32::from_gray(70) } else { egui::Color32::from_gray(38) });
                            self.ed.want_thumb(s, env, ui.ctx());
                            let mut tip = format!("sticker {} · texture {} · {}", s.sticker, s.texture, if s.mask { "colour mask" } else { "picture" });
                            match self.ed.thumbs.get(&s.sticker) {
                                Some(Thumb::Ready(t, full)) => {
                                    let sz = t.size_vec2();
                                    let sz = sz * ((cell - 8.0) / sz.x.max(sz.y));
                                    ui.painter().image(t.id(), egui::Rect::from_center_size(rect.center(), sz), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                                    tip += &format!(" · {} × {} px", full[0], full[1]);
                                    if resp.clicked() {
                                        chosen = Some(s.clone());
                                    }
                                }
                                Some(Thumb::Failed(e)) => {
                                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "✖", egui::FontId::proportional(20.0), egui::Color32::from_gray(110));
                                    tip += &format!("\nnot readable: {e}");
                                }
                                _ => {
                                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(20.0), egui::Color32::from_gray(110));
                                }
                            }
                            resp.on_hover_text(tip);
                        }
                    });
                }
            });
        });
        self.ed.picker_open = open;
        if let Some(g) = chosen {
            match self.ed.selected.filter(|i| self.ed.picker_replace && *i < self.livery.stickers.len()) {
                Some(i) => {
                    let s = &mut self.livery.stickers[i];
                    s.picture = Picture::Game(g.texture.clone());
                    s.mask = g.mask;
                    if g.mask {
                        s.colours = g.colours;
                    }
                    self.ed.previews.clear();
                    self.changed();
                }
                None => self.add(Picture::Game(g.texture.clone()), format!("sticker {}", g.sticker), g.mask, g.mask.then_some(g.colours), env),
            }
            self.ed.picker_open = false;
        }
    }

    /// The pointer on the model. Returns true when the editor used it, so
    /// the camera must leave it alone.
    pub fn pointer(&mut self, ui: &egui::Ui, resp: &egui::Response, rect: egui::Rect, cam: &Camera, model: &glb::Model, env: &Env) -> bool {
        // scripted steps (checks without a mouse), one per frame
        if self.ed.busy() && (!self.ed.ready || self.zones.models.is_empty()) {
            return false;
        }
        if let Some(k) = self.ed.auto_select.take() {
            self.ed.selected = Some(k).filter(|k| *k < self.livery.stickers.len());
            return false;
        }
        if !self.ed.auto_add.is_empty() {
            let id = self.ed.auto_add.remove(0);
            let (id, mask) = match id.strip_suffix(":mask") {
                Some(i) => (i.to_string(), true),
                None => (id, false),
            };
            self.add(Picture::Game(id.clone()), format!("sticker {id}"), mask, None, env);
            return false;
        }
        let sel = self.ed.selected.filter(|i| *i < self.livery.stickers.len());
        if let (Some(i), Some(c)) = (sel, self.ed.auto_click.first().copied()) {
            self.ed.auto_click.remove(0);
            let pos = rect.min + egui::vec2(rect.width() * c[0], rect.height() * c[1]);
            match self.hit(pos, rect, cam, model) {
                Some(h) => self.toggle_zone(i, h.slot, h.zone, env),
                None => self.status = format!("scripted click at {c:?}: nothing there"),
            }
            return false;
        }
        if let (Some(i), Some(c)) = (sel, self.ed.auto_drag) {
            self.ed.auto_drag = None;
            let pos = rect.min + egui::vec2(rect.width() * c[0], rect.height() * c[1]);
            let (zones, set) = (self.livery.stickers[i].zones.clone(), self.livery.stickers[i].uv_set.min(1) as usize);
            let slot = self.zones.slot_of(&self.livery.stickers[i].model);
            match self.hit(pos, rect, cam, model).filter(|h| Some(h.slot) == slot && zones.contains(&h.zone)) {
                Some(h) => {
                    self.livery.stickers[i].fresh = false;
                    self.livery.stickers[i].centre = h.uv[set].to_array();
                    self.changed();
                }
                None => self.status = format!("scripted drag to {c:?}: not on the sticker's zones"),
            }
            return false;
        }
        self.ed.auto_click.clear();
        self.ed.auto_drag = None;
        if self.ed.auto_save {
            self.ed.auto_save = false;
            let file = self.file(Path::new(env.work));
            self.save_to(&file);
        }
        let Some(i) = sel.filter(|_| self.ed.open && self.ed.tool != Tool::None) else {
            if self.highlight.hover.take().is_some() && self.ed.tool != Tool::None {
                self.changed();
            }
            return false;
        };
        let pos = resp.hover_pos().or(resp.interact_pointer_pos());
        let hit = pos.and_then(|p| self.hit(p, rect, cam, model));
        match self.ed.tool {
            Tool::Zones => {
                let hover = hit.map(|h| (h.slot, h.zone));
                if self.highlight.hover != hover {
                    self.highlight.hover = hover;
                    self.changed();
                }
                if let (true, Some(h)) = (resp.clicked(), hit) {
                    self.toggle_zone(i, h.slot, h.zone, env);
                }
                false
            }
            Tool::Move => {
                if self.highlight.hover.take().is_some() {
                    self.changed();
                }
                let s = &self.livery.stickers[i];
                let set = s.uv_set.min(1) as usize;
                let slot = self.zones.slot_of(&s.model);
                let on = hit.filter(|h| Some(h.slot) == slot && s.zones.contains(&h.zone));
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    self.ed.grab = on.map(|h| Vec2::from(s.centre) - h.uv[set]);
                }
                if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
                    self.ed.grab = None;
                }
                if let (Some(g), Some(h), true) = (self.ed.grab, on, resp.dragged_by(egui::PointerButton::Primary)) {
                    let c = (h.uv[set] + g).to_array();
                    if self.livery.stickers[i].centre != c {
                        self.livery.stickers[i].centre = c;
                        self.livery.stickers[i].fresh = false;
                        self.changed();
                    }
                }
                // Ctrl + wheel: size; Shift + wheel: width; Ctrl + Shift +
                // wheel: height; Alt + wheel: rotation (a wheel with Shift
                // held arrives as a sideways scroll on some systems)
                let (scroll, ctrl, alt, shift) = ui.input(|i| (if i.smooth_scroll_delta.y != 0.0 { i.smooth_scroll_delta.y } else { i.smooth_scroll_delta.x }, i.modifiers.command, i.modifiers.alt, i.modifiers.shift));
                let mut used = self.ed.grab.is_some();
                if resp.hovered() && scroll != 0.0 && (ctrl || alt || shift) {
                    let s = &mut self.livery.stickers[i];
                    s.fresh = false;
                    let k = (scroll * 0.002).exp();
                    if shift && !alt {
                        let axis = usize::from(ctrl);
                        s.size[axis] = (s.size[axis] * k).clamp(0.02, 12.0);
                    } else if ctrl {
                        s.size = [(s.size[0] * k).clamp(0.02, 12.0), (s.size[1] * k).clamp(0.02, 12.0)];
                    } else {
                        s.rotation = ((s.rotation + scroll * 0.1 + 180.0).rem_euclid(360.0)) - 180.0;
                    }
                    self.changed();
                    used = true;
                }
                used
            }
            Tool::None => false,
        }
    }

    /// What the view shows at a place of the viewport.
    fn hit(&mut self, pos: egui::Pos2, rect: egui::Rect, cam: &Camera, model: &glb::Model) -> Option<livery::Hit> {
        if self.ed.boxes.0 != model.path || self.ed.boxes.1 != model.triangles {
            let boxes = model
                .meshes
                .iter()
                .map(|me| {
                    let mut b = [Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)];
                    for v in &me.verts {
                        b[0] = b[0].min(Vec3::from(v.pos));
                        b[1] = b[1].max(Vec3::from(v.pos));
                    }
                    b
                })
                .collect();
            self.ed.boxes = (model.path.clone(), model.triangles, boxes);
        }
        let ndc = Vec2::new((pos.x - rect.min.x) / rect.width().max(1.0) * 2.0 - 1.0, 1.0 - (pos.y - rect.min.y) / rect.height().max(1.0) * 2.0);
        let inv = (cam.proj(rect.width() / rect.height().max(1.0)) * cam.view()).inverse();
        let a = inv.project_point3(Vec3::new(ndc.x, ndc.y, 0.25));
        let b = inv.project_point3(Vec3::new(ndc.x, ndc.y, 0.75));
        let eye = cam.eye();
        let mut dir = (b - a).normalize_or_zero();
        // whichever way the depth runs, the ray leaves the eye
        if dir.dot(a.lerp(b, 0.5) - eye) < 0.0 {
            dir = -dir;
        }
        livery::pick(model, eye, dir, Some(&self.ed.boxes.2))
    }
}
