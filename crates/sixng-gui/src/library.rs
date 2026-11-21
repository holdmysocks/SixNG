//! Library: the home screen. Models by category as thumbnail grids, an item
//! page with a Files panel and Export on demand, recents and favourites.
//! Map and Textures are entries too (they switch tabs).

use crate::model_picker::{Cat, Item, CATS};
use crate::model_view::ModelView;
use crate::thumbs::Thumbs;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub enum Action {
    OpenModel(String),
    ShowMap,
    ShowTextures,
    ShowAudio,
    /// queue `sixng <args>` as a job
    Job(String, Vec<String>),
    OpenJobs,
}

#[derive(Clone, PartialEq)]
enum View {
    Home,
    Category(Cat),
    Item(String),
}

pub struct Library {
    view: View,
    back: Vec<View>,
    search: String,
    hide_variants: bool,
    filtered: Vec<u32>,
    filter_key: (Option<Cat>, String, bool, usize),
    pub thumbs: Thumbs,
    tile: f32,
    /// part ids of put-together models (animals, characters), by item id
    parts: std::collections::HashMap<String, Option<Vec<String>>>,
}

/// Readable name from a model name: `phoenix_hi` → "Phoenix",
/// `prop_am_parking_valet_01a` → "Parking valet 01a". Game display names are
/// not decoded, so this is a best effort from the model name.
pub fn display_name(name: &str) -> String {
    if let Some(p) = crate::assemble::pretty_name(name) {
        return p;
    }
    let mut n = name.to_ascii_lowercase();
    for s in ["_hi", "_mid", "_lo"] {
        if let Some(x) = n.strip_suffix(s) {
            n = x.to_string();
        }
    }
    for p in ["prop_am_", "prop_", "p_", "veg_", "int_", "w_", "wpn_"] {
        if let Some(x) = n.strip_prefix(p) {
            n = x.to_string();
            break;
        }
    }
    let s = n.replace('_', " ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => name.to_string(),
    }
}

fn cat_label(c: Cat) -> &'static str {
    CATS.iter().find(|x| x.0 == c).map(|x| x.1).unwrap_or("?")
}

/// The archive part of an entry id: "levels.rpf@1a2b" → "levels.rpf".
fn archive_of(id: &str) -> &str {
    id.split('@').next().unwrap_or(id)
}

/// Export folder of a model under an output root, if it was exported.
pub fn export_dir(out: &Path, id: &str) -> Option<PathBuf> {
    let (a, off) = id.split_once('@')?;
    let d = out.join("models").join(a.trim_end_matches(".rpf"));
    let suffix = format!("_{}", off.to_ascii_lowercase());
    std::fs::read_dir(&d).ok()?.flatten().map(|e| e.path()).find(|p| p.is_dir() && p.file_name().map_or(false, |f| f.to_string_lossy().ends_with(&suffix)))
}

impl Library {
    pub fn new(game: &str, work: &str, thumb_budget_mb: u64) -> Self {
        Library { view: View::Home, back: Vec::new(), search: String::new(), hide_variants: true, filtered: Vec::new(), filter_key: (None, String::new(), true, 0), thumbs: Thumbs::new(game, work, thumb_budget_mb), tile: 150.0, parts: Default::default() }
    }

    pub fn show_item(&mut self, id: &str) {
        self.go(View::Item(id.to_string()));
    }

    fn go(&mut self, v: View) {
        if self.view != v {
            self.back.push(self.view.clone());
            self.view = v;
        }
    }

    pub fn idle(&self) -> bool {
        self.thumbs.queued() == 0
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame, mv: &mut ModelView, out_root: &str) -> Option<Action> {
        let ctx = ui.ctx().clone();
        let ws = sixng_core::export::Workspace::new(&mv.game, &mv.work);
        mv.picker.ensure_loaded(ws, mv.indexes.clone(), &ctx);
        self.thumbs.pump(&ctx, frame.wgpu_render_state(), &mv.indexes);
        if self.thumbs.queued() > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut act = None;
        // header: back, breadcrumb, search
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.back.is_empty(), egui::Button::new("◀ Back")).clicked() {
                if let Some(v) = self.back.pop() {
                    self.view = v;
                }
            }
            if ui.button("🏠 Library").clicked() {
                self.go(View::Home);
            }
            match &self.view {
                View::Category(c) => {
                    ui.label(format!("› {}", cat_label(*c)));
                }
                View::Item(id) => {
                    ui.label(format!("› {id}"));
                }
                View::Home => {}
            }
            ui.separator();
            let r = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search all models (Ctrl+K)").desired_width(260.0));
            if r.changed() && !self.search.is_empty() && self.view == View::Home {
                self.go(View::Category(Cat::All));
            }
            if self.thumbs.queued() > 0 {
                ui.spinner();
                ui.label(egui::RichText::new(format!("rendering thumbnails ({} queued)", self.thumbs.queued())).small());
            } else if self.thumbs.over_budget() {
                ui.label(egui::RichText::new("thumbnail budget full (Settings)").small());
            }
        });
        ui.separator();
        let Some(items) = mv.picker.items() else {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                if mv.picker.status.is_empty() {
                    ui.spinner();
                }
                ui.label(&mv.picker.status);
                ui.label("The Library lists the models in the indexed archives. If nothing is indexed yet, run the setup (Settings → Run setup again) or index the archives on the Jobs page.");
                if ui.button("Open Jobs").clicked() {
                    act = Some(Action::OpenJobs);
                }
            });
            return act;
        };
        match self.view.clone() {
            View::Home => self.home(ui, &items, mv, &mut act),
            View::Category(c) => self.grid(ui, &items, c, mv),
            View::Item(id) => self.item(ui, &items, &id, mv, out_root, &mut act),
        }
        act
    }

    fn card(&mut self, ui: &mut egui::Ui, title: &str, sub: &str, thumb: Option<egui::TextureHandle>) -> bool {
        let size = egui::vec2(200.0, 170.0);
        let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
        let vis = ui.style().interact(&r);
        let p = ui.painter();
        p.rect_filled(rect, 8.0, if r.hovered() { ui.visuals().widgets.hovered.bg_fill } else { ui.visuals().faint_bg_color });
        let img = egui::Rect::from_min_size(rect.min + egui::vec2(8.0, 8.0), egui::vec2(size.x - 16.0, 110.0));
        if let Some(t) = thumb {
            let s = t.size_vec2();
            let k = (img.width() / s.x).min(img.height() / s.y);
            let r2 = egui::Rect::from_center_size(img.center(), s * k);
            p.image(t.id(), r2, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        } else {
            p.rect_filled(img, 6.0, ui.visuals().extreme_bg_color);
            let glyph = match title {
                "Map" => "🗺",
                "Textures" => "🖼",
                "Unnamed" => "❓",
                _ => "",
            };
            if !glyph.is_empty() {
                p.text(img.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(48.0), ui.visuals().weak_text_color());
            }
        }
        p.text(rect.min + egui::vec2(10.0, 126.0), egui::Align2::LEFT_TOP, title, egui::FontId::proportional(16.0), vis.text_color());
        p.text(rect.min + egui::vec2(10.0, 148.0), egui::Align2::LEFT_TOP, sub, egui::FontId::proportional(12.0), ui.visuals().weak_text_color());
        r.clicked()
    }

    fn home(&mut self, ui: &mut egui::Ui, items: &Arc<Vec<Item>>, mv: &mut ModelView, act: &mut Option<Action>) {
        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let logo = crate::banner(&ctx);
            let s = logo.size_vec2();
            ui.image((logo.id(), s * (64.0 / s.y)));
            ui.add_space(6.0);
            ui.heading("Browse");
            ui.horizontal_wrapped(|ui| {
                for (c, label) in CATS.iter().copied() {
                    if c == Cat::All || c == Cat::Unnamed {
                        continue;
                    }
                    // first non-variant item of the category as the cover
                    let cover = items.iter().find(|it| it.cat == c && !it.variant).map(|it| it.id.clone());
                    let thumb = cover.and_then(|id| self.thumbs.get(&ctx, &id));
                    if self.card(ui, label, &format!("{} models", mv.picker.count(c)), thumb) {
                        self.go(View::Category(c));
                    }
                }
                if self.card(ui, "Map", "2D and 3D map", None) {
                    *act = Some(Action::ShowMap);
                }
                if self.card(ui, "Textures", "every texture store", None) {
                    *act = Some(Action::ShowTextures);
                }
                if self.card(ui, "Audio", "sound banks: play and export", None) {
                    *act = Some(Action::ShowAudio);
                }
                if self.card(ui, "Unnamed", &format!("{} models without a recovered name", mv.picker.count(Cat::Unnamed)), None) {
                    self.go(View::Category(Cat::Unnamed));
                }
            });
            for (title, ids) in [("Recently opened", mv.picker.recents().to_vec()), ("Favourites", mv.picker.favourites().to_vec())] {
                if ids.is_empty() {
                    continue;
                }
                ui.add_space(10.0);
                ui.heading(title);
                ui.horizontal_wrapped(|ui| {
                    for id in ids.iter().take(12) {
                        let name = items.iter().find(|it| &it.id == id).map(|it| it.name.clone()).unwrap_or_else(|| id.clone());
                        let t = self.thumbs.get(&ctx, id);
                        if self.card(ui, &display_name(&name), &name, t) {
                            self.go(View::Item(id.clone()));
                        }
                    }
                });
            }
        });
    }

    fn refilter(&mut self, items: &[Item], c: Cat) {
        let key = (Some(c), self.search.clone(), self.hide_variants, items.len());
        if key == self.filter_key {
            return;
        }
        let terms: Vec<String> = self.search.to_ascii_lowercase().split_whitespace().map(String::from).collect();
        self.filtered = items
            .iter()
            .enumerate()
            .filter(|(_, it)| c == Cat::All || it.cat == c)
            .filter(|(_, it)| !self.hide_variants || !it.variant)
            .filter(|(_, it)| terms.iter().all(|t| it.lname.contains(t.as_str()) || it.id.contains(t.as_str())))
            .map(|(i, _)| i as u32)
            .collect();
        self.filter_key = key;
    }

    fn grid(&mut self, ui: &mut egui::Ui, items: &Arc<Vec<Item>>, c: Cat, mv: &mut ModelView) {
        self.refilter(items, c);
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.heading(cat_label(c));
            ui.label(format!("{} models", self.filtered.len()));
            ui.checkbox(&mut self.hide_variants, "hide LOD copies");
            ui.add(egui::Slider::new(&mut self.tile, 100.0..=260.0).text("tile size"));
        });
        if c == Cat::Vehicles {
            ui.label(egui::RichText::new("Names come from the model names; the game's display names are not decoded yet.").small().weak());
        }
        let tile = self.tile;
        let cols = ((ui.available_width() + 8.0) / (tile + 8.0)).floor().max(1.0) as usize;
        let rows = self.filtered.len().div_ceil(cols);
        let row_h = tile + 40.0;
        let mut open = None;
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show_rows(ui, row_h, rows, |ui, range| {
            for r in range {
                ui.horizontal(|ui| {
                    for k in 0..cols {
                        let Some(&i) = self.filtered.get(r * cols + k) else { break };
                        let it = &items[i as usize];
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(tile, row_h - 8.0), egui::Sense::click());
                        let p = ui.painter();
                        p.rect_filled(rect, 6.0, if resp.hovered() { ui.visuals().widgets.hovered.bg_fill } else { ui.visuals().faint_bg_color });
                        let img = egui::Rect::from_min_size(rect.min + egui::vec2(4.0, 4.0), egui::vec2(tile - 8.0, tile - 8.0));
                        match self.thumbs.get(&ctx, &it.id) {
                            Some(t) => {
                                p.image(t.id(), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                            }
                            None => {
                                p.rect_filled(img, 4.0, ui.visuals().extreme_bg_color);
                                let msg = if self.thumbs.is_pending(&it.id) { "…" } else { "no preview" };
                                p.text(img.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(12.0), ui.visuals().weak_text_color());
                            }
                        }
                        let fav = if mv.picker.is_favourite(&it.id) { "★ " } else { "" };
                        p.text(egui::pos2(rect.min.x + 6.0, img.max.y + 4.0), egui::Align2::LEFT_TOP, format!("{fav}{}", display_name(&it.name)), egui::FontId::proportional(13.0), ui.visuals().text_color());
                        p.text(egui::pos2(rect.min.x + 6.0, img.max.y + 20.0), egui::Align2::LEFT_TOP, &it.name, egui::FontId::proportional(10.0), ui.visuals().weak_text_color());
                        if resp.on_hover_text(format!("{}\n{}", it.name, it.id)).clicked() {
                            open = Some(it.id.clone());
                        }
                    }
                });
            }
        });
        if let Some(id) = open {
            self.go(View::Item(id));
        }
    }

    fn item(&mut self, ui: &mut egui::Ui, items: &Arc<Vec<Item>>, id: &str, mv: &mut ModelView, out_root: &str, act: &mut Option<Action>) {
        let ctx = ui.ctx().clone();
        let it = items.iter().find(|it| it.id == id);
        let name = it.map(|i| i.name.clone()).unwrap_or_else(|| id.to_string());
        let parts = self
            .parts
            .entry(id.to_string())
            .or_insert_with(|| crate::model_view::assembly_ids(&sixng_core::export::Workspace::new(&mv.game, &mv.work), &mv.indexes, id))
            .clone();
        // follow the parts chosen in the viewer, when it shows this model
        let parts = mv.selected_parts(id).or(parts);
        ui.horizontal_top(|ui| {
            let t = self.thumbs.get(&ctx, id);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(320.0, 320.0), egui::Sense::hover());
            match t {
                Some(t) => {
                    ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                }
                None => {
                    ui.painter().rect_filled(rect, 6.0, ui.visuals().extreme_bg_color);
                }
            }
            ui.vertical(|ui| {
                ui.heading(display_name(&name));
                ui.label(egui::RichText::new(&name).monospace());
                egui::Grid::new("item_info").num_columns(2).show(ui, |ui| {
                    ui.label("Entry");
                    ui.label(egui::RichText::new(id).monospace());
                    ui.end_row();
                    if let Some(it) = it {
                        ui.label("Category");
                        ui.label(cat_label(it.cat));
                        ui.end_row();
                        ui.label("Type");
                        ui.label(match it.rtype {
                            "ft" if crate::assemble::is_animal(&it.lname) => "animal (put together from its parts)",
                            "ft" => "fragment (skinned / breakable)",
                            "outfit" => "character (default outfit put together from its parts)",
                            _ => "drawable",
                        });
                        ui.end_row();
                        ui.label("Size");
                        ui.label(crate::human(it.bytes));
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("🧊 Open in 3D viewer").clicked() {
                        *act = Some(Action::OpenModel(id.to_string()));
                    }
                    let fav = mv.picker.is_favourite(id);
                    if ui.button(if fav { "★ Favourite" } else { "☆ Add to favourites" }).clicked() {
                        mv.picker.toggle_favourite(id);
                    }
                });
                ui.horizontal(|ui| {
                    let (ids, hover) = match &parts {
                        Some(p) => (p.join(","), format!("Queues a job: extract the {} parts to {out_root} (one glTF each, in the same bind pose, so they line up when imported together)", p.len())),
                        None => (id.to_string(), format!("Queues a job: extract to {out_root}")),
                    };
                    if ui.button("⬇ Export (glTF + textures)").on_hover_text(hover).clicked() {
                        let first = parts.as_ref().and_then(|p| p.first().cloned()).unwrap_or_else(|| id.to_string());
                        *act = Some(Action::Job(format!("export {name}"), vec!["extract".into(), archive_of(&first).into(), "--id".into(), ids, "-o".into(), out_root.into(), "--tex".into(), "both".into()]));
                    }
                });
            });
        });
        ui.separator();
        ui.heading("Files");
        if let Some(p) = &parts {
            ui.label(format!("Put together from {} parts:", p.len()));
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                egui::Grid::new("item_parts").num_columns(3).striped(true).show(ui, |ui| {
                    for pid in p {
                        let pname = items.iter().find(|i| &i.id == pid).map(|i| i.name.clone()).unwrap_or_else(|| pid.clone());
                        ui.label(egui::RichText::new(&pname).monospace());
                        match export_dir(Path::new(out_root), pid) {
                            Some(dir) => {
                                if ui.small_button(format!("Show in {}", crate::FILE_MANAGER)).clicked() {
                                    crate::open_path(&dir);
                                }
                                if ui.small_button("Copy path").clicked() {
                                    ui.ctx().copy_text(dir.display().to_string());
                                }
                            }
                            None => {
                                ui.weak("not exported yet");
                                ui.label("");
                            }
                        }
                        ui.end_row();
                    }
                });
            });
            return;
        }
        match export_dir(Path::new(out_root), id) {
            None => {
                ui.label(format!("Not exported yet to {out_root}. Use Export above; the files appear here when the job finishes."));
            }
            Some(dir) => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(dir.display().to_string()).monospace());
                    if ui.small_button(format!("Show in {}", crate::FILE_MANAGER)).clicked() {
                        crate::open_path(&dir);
                    }
                    if ui.small_button("Copy path").clicked() {
                        ui.ctx().copy_text(dir.display().to_string());
                    }
                });
                let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect()).unwrap_or_default();
                files.sort();
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    egui::Grid::new("item_files").num_columns(4).striped(true).show(ui, |ui| {
                        for f in files {
                            let n = f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                            ui.label(egui::RichText::new(&n).monospace());
                            ui.label(crate::human(std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0)));
                            if ui.small_button("Open").clicked() {
                                crate::open_path(&f);
                            }
                            if ui.small_button("Show").clicked() {
                                crate::reveal(&f);
                            }
                            ui.end_row();
                        }
                    });
                });
                ui.label(egui::RichText::new("Textures referenced by the model are in the output's textures/ folder (see model.json).").small().weak());
            }
        }
    }
}
