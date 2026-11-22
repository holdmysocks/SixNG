//! App pages around the viewers: the first-run setup wizard, Settings,
//! Status, and the Ctrl+K search palette.

use crate::{save_settings, App, AppSettings, Tab};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// First-run wizard state.
#[derive(Default)]
pub struct Setup {
    pub open: bool,
    pub step: u8,
    pub index_all: bool,
    pub link: bool,
    pub world: bool,
    pub bake: bool,
    /// jobs started by the wizard, in order, with labels
    pub jobs: Vec<(u32, String)>,
    /// bake waits for these jobs
    pub bake_after: Option<Vec<u32>>,
    pub bake_started: bool,
    pub msg: String,
}

/// Folder sizes for the Status page (computed in the background).
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub when: u64,
    /// (relative folder, bytes, files)
    pub dirs: Vec<(String, u64, u64)>,
}

#[derive(Default)]
pub struct Status {
    pub sizes: Arc<Mutex<Option<Snapshot>>>,
    pub computing: bool,
    pub previous: Option<Snapshot>,
    pub previous_read: bool,
    pub gaps: Arc<Mutex<Option<Result<serde_json::Value, String>>>>,
    pub gaps_loading: bool,
}

#[derive(Default)]
pub struct Palette {
    pub open: bool,
    query: String,
    cursor: usize,
    focus: bool,
}

enum Hit {
    Page(Tab, &'static str),
    Model(String, String, &'static str),
}

const PAGES: &[(Tab, &str)] = &[
    (Tab::Library, "Library"),
    (Tab::Model, "Model viewer"),
    (Tab::Map2D, "Map 2D"),
    (Tab::Map3D, "Map 3D"),
    (Tab::Textures, "Textures"),
    (Tab::Audio, "Audio"),
    (Tab::Jobs, "Jobs"),
    (Tab::Status, "Status"),
    (Tab::Settings, "Settings"),
    (Tab::Browser, "Archives (advanced browser)"),
];

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn tree_size(p: &Path) -> (u64, u64) {
    let mut bytes = 0;
    let mut files = 0;
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(e.path());
            } else if let Ok(m) = e.metadata() {
                bytes += m.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}

/// Archives (.rpf) directly in a folder or one level down: a quick check that
/// a folder is the game install.
fn count_rpf(dir: &str) -> usize {
    let mut n = 0;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension()
            .map_or(false, |x| x.eq_ignore_ascii_case("rpf"))
        {
            n += 1;
        } else if p.is_dir() {
            if let Ok(rd2) = std::fs::read_dir(&p) {
                n += rd2
                    .flatten()
                    .filter(|e| {
                        e.path()
                            .extension()
                            .map_or(false, |x| x.eq_ignore_ascii_case("rpf"))
                    })
                    .count();
            }
        }
    }
    n
}

fn ago(secs: u64) -> String {
    let d = now().saturating_sub(secs);
    if d < 120 {
        format!("{d} s ago")
    } else if d < 7200 {
        format!("{} min ago", d / 60)
    } else if d < 172800 {
        format!("{} h ago", d / 3600)
    } else {
        format!("{} days ago", d / 86400)
    }
}

impl App {
    pub(crate) fn settings(&self) -> AppSettings {
        AppSettings {
            game: Some(self.game.clone()),
            work: Some(self.work.clone()),
            out: Some(self.out.clone()),
            thumb_budget_mb: Some(self.thumb_budget_mb),
            setup_done: self.setup_done,
        }
    }

    /// Rebuild the views that hold the game / work folders.
    pub(crate) fn rebuild_views(&mut self) {
        self.model = crate::model_view::ModelView::new(&self.game, &self.work);
        self.map2d = crate::map2d::Map2D::new(&self.work);
        self.map3d = crate::map3d::Map3D::new(&self.game, &self.work);
        self.map3d.load_settings();
        self.textures = crate::texture_view::TextureView::new(&self.game, &self.work);
        self.audio.stop();
        self.audio = crate::audio_view::AudioView::new(&self.game, &self.work);
        self.library = crate::library::Library::new(&self.game, &self.work, self.thumb_budget_mb);
        self.jobs.set_folders(&self.game, &self.work);
        self.status = Status::default();
        self.refresh();
    }

    fn folder_row(ui: &mut egui::Ui, label: &str, v: &mut String, hint: &str) -> bool {
        let mut changed = false;
        ui.label(label);
        ui.horizontal(|ui| {
            changed |= ui
                .add(
                    egui::TextEdit::singleline(v)
                        .desired_width(420.0)
                        .hint_text(hint),
                )
                .changed();
            if ui.button("Browse…").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    *v = p.display().to_string();
                    changed = true;
                }
            }
        });
        ui.end_row();
        changed
    }

    /// First-run wizard: folders, then what to prepare, then progress.
    pub(crate) fn setup_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);
                    let logo = crate::banner(&ctx);
                    let s = logo.size_vec2();
                    ui.image((logo.id(), s * (90.0 / s.y)));
                    ui.add_space(6.0);
                    ui.heading("Welcome to SixNG");
                    ui.label("A few steps to get the game's files ready to browse. Everything stays on this PC, in the work folder.");
                });
                ui.add_space(14.0);
                let w = 640.0f32.min(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| match self.setup.step {
                        0 => self.setup_folders(ui),
                        1 => self.setup_tasks(ui),
                        _ => self.setup_progress(ui),
                    });
                });
            });
        });
    }

    fn setup_folders(&mut self, ui: &mut egui::Ui) {
        ui.strong("1. Folders");
        egui::Grid::new("setup_folders")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                Self::folder_row(
                    ui,
                    "Game folder",
                    &mut self.game,
                    "the GTA VI install (contains the .rpf archives)",
                );
                Self::folder_row(
                    ui,
                    "Work folder",
                    &mut self.work,
                    "indexes, map bake, caches and exports",
                );
            });
        let n = count_rpf(self.game.trim());
        let ok = n > 0;
        if self.game.trim().is_empty() {
            ui.weak("Pick the game folder.");
        } else if ok {
            ui.colored_label(
                egui::Color32::from_rgb(120, 200, 120),
                format!("✔ {n} archives found."),
            );
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(230, 150, 90),
                "No .rpf archives in this folder (or one level down). Is it the game install?",
            );
        }
        ui.weak("The work folder needs room: about 1 GB for the indexes, 5–15 GB for world data and the map, more for exports.");
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    ok && !self.work.trim().is_empty(),
                    egui::Button::new("Next ▶"),
                )
                .clicked()
            {
                self.game = self.game.trim().to_string();
                self.work = self.work.trim().to_string();
                if !Path::new(&self.out).is_absolute() || self.out.is_empty() {
                    self.out = Path::new(&self.work).join("out").display().to_string();
                }
                let _ = std::fs::create_dir_all(&self.work);
                self.rebuild_views();
                let st = self.settings();
                if let Err(e) = save_settings(&st) {
                    self.setup.msg = format!("Could not save settings: {e}");
                }
                // preselect what is missing
                let ws = self.ws();
                let missing = self.rpf_archives().filter(|a| !ws.is_indexed(a)).count();
                self.setup.index_all = missing > 0;
                self.setup.link = !Path::new(&self.work)
                    .join("index")
                    .join("archetype_links.json")
                    .exists();
                let (req, _) = crate::bakejob::BakeJob::missing_inputs(&self.work);
                self.setup.world = !req.is_empty();
                self.setup.bake = !Path::new(&self.work).join("map").join("meta.json").exists();
                self.setup.step = 1;
            }
            if self.setup_done && ui.button("Cancel").clicked() {
                self.setup.open = false;
            }
        });
        if !self.setup.msg.is_empty() {
            ui.label(&self.setup.msg);
        }
    }

    fn setup_tasks(&mut self, ui: &mut egui::Ui) {
        ui.strong("2. Prepare");
        ui.label("These run in the background, one after another (you can browse meanwhile and follow them on the Jobs page).");
        ui.add_space(6.0);
        let ws = self.ws();
        let total = self.rpf_archives().count();
        let indexed = self.rpf_archives().filter(|a| ws.is_indexed(a)).count();
        let (req, opt) = crate::bakejob::BakeJob::missing_inputs(&self.work);
        let baked = Path::new(&self.work).join("map").join("meta.json").exists();
        let links = Path::new(&self.work)
            .join("index")
            .join("archetype_links.json")
            .exists();
        let s = &mut self.setup;
        ui.checkbox(&mut s.index_all, format!("Index the archives ({indexed} of {total} done)"))
            .on_hover_text("Finds every file inside the archives and recovers names. Needed for everything else. About 10–30 minutes the first time, ~1 GB.");
        ui.checkbox(&mut s.link, format!("Link unnamed buildings to their models{}", if links { " (done)" } else { "" }))
            .on_hover_text("Matches map entries whose model name is unknown to the right model by shape. Optional; makes more of the map show up. A few minutes.");
        ui.checkbox(&mut s.world, format!("Export world data{}", if req.is_empty() { " (done)".to_string() } else { format!(" (missing: {})", req.join(", ")) }))
            .on_hover_text("Placements, collision, navmesh, road paths, lights and interiors for the whole map, into <work>/out. Needed for the map. 10–40 minutes, several GB.");
        ui.checkbox(
            &mut s.bake,
            format!(
                "Bake the map{}",
                if baked {
                    " (done; tick to re-bake)"
                } else {
                    ""
                }
            ),
        )
        .on_hover_text(
            "Builds the 2D/3D map files from the world data. About 2 minutes and a few GB of RAM.",
        );
        if !opt.is_empty() {
            ui.weak(format!(
                "Optional layers not exported yet: {}.",
                opt.join(", ")
            ));
        }
        if !self.jobs.cli_found() {
            ui.colored_label(egui::Color32::from_rgb(230, 150, 90), "sixng.exe was not found next to this app, so jobs cannot run. Keep both exes in the same folder.");
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("◀ Back").clicked() {
                self.setup.step = 0;
            }
            let any =
                self.setup.index_all || self.setup.link || self.setup.world || self.setup.bake;
            if ui.add_enabled(any, egui::Button::new("Start ▶")).clicked() {
                self.start_setup_jobs();
                self.setup.step = 2;
            }
            if ui
                .button("Skip for now")
                .on_hover_text("Go to the Library; run these later from Settings → Run setup again")
                .clicked()
            {
                self.finish_setup();
            }
        });
    }

    fn start_setup_jobs(&mut self) {
        let out = self.world_out();
        let mut ids = Vec::new();
        if self.setup.index_all {
            ids.push((
                self.jobs
                    .enqueue("Index all archives", vec!["index".into()]),
                "Index all archives".to_string(),
            ));
        }
        if self.setup.link {
            ids.push((
                self.jobs
                    .enqueue("Link unnamed buildings", vec!["link-archetypes".into()]),
                "Link unnamed buildings".to_string(),
            ));
        }
        if self.setup.world {
            ids.push((
                self.jobs.enqueue(
                    "Export world data",
                    vec!["world".into(), "all".into(), "-o".into(), out],
                ),
                "Export world data".to_string(),
            ));
        }
        self.setup.bake_after = if self.setup.bake {
            Some(ids.iter().map(|x| x.0).collect())
        } else {
            None
        };
        self.setup.bake_started = false;
        self.setup.jobs = ids;
    }

    /// World data goes to `<work>/out` (where the map bake reads it).
    pub(crate) fn world_out(&self) -> String {
        Path::new(&self.work).join("out").display().to_string()
    }

    fn finish_setup(&mut self) {
        self.setup_done = true;
        self.setup.open = false;
        let st = self.settings();
        let _ = save_settings(&st);
        self.tab = Tab::Library;
        self.rebuild_views();
    }

    /// Start the bake once the wizard's jobs are done (called every frame).
    pub(crate) fn setup_tick(&mut self) {
        let Some(after) = &self.setup.bake_after else {
            return;
        };
        if self.setup.bake_started {
            return;
        }
        let done = after
            .iter()
            .all(|id| self.jobs.state_of(*id).map_or(true, |j| j.state.finished()));
        if done {
            self.setup.bake_started = true;
            if let Err(e) = self.bake.start(&self.game, &self.work) {
                self.bake.result = Some(e);
            }
        }
    }

    fn setup_progress(&mut self, ui: &mut egui::Ui) {
        ui.strong("3. Preparing");
        let mut all_done = true;
        egui::Grid::new("setup_prog")
            .num_columns(3)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                for (id, label) in self.setup.jobs.clone() {
                    ui.label(&label);
                    match self.jobs.state_of(id) {
                        Some(j) => {
                            let (stage, f, _, _) = j.progress();
                            if j.state.finished() {
                                ui.label(j.state.label());
                                ui.label("");
                            } else {
                                all_done = false;
                                ui.add(
                                    egui::ProgressBar::new(f)
                                        .desired_width(260.0)
                                        .show_percentage(),
                                );
                                ui.weak(stage);
                            }
                        }
                        None => {
                            ui.label("?");
                            ui.label("");
                        }
                    }
                    ui.end_row();
                }
                if self.setup.bake_after.is_some() {
                    ui.label("Bake the map");
                    if self.bake.running() {
                        all_done = false;
                        ui.spinner();
                        ui.weak(&self.bake.last_line);
                    } else if self.setup.bake_started {
                        ui.label(self.bake.result.clone().unwrap_or_else(|| "done".into()));
                        ui.label("");
                    } else {
                        all_done = false;
                        ui.weak("waits for the steps above");
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        ui.add_space(10.0);
        if all_done {
            ui.colored_label(egui::Color32::from_rgb(120, 200, 120), "All done.");
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(300));
        }
        ui.horizontal(|ui| {
            let label = if all_done {
                "Open the Library ▶"
            } else {
                "Continue in the background ▶"
            };
            if ui.button(label).clicked() {
                self.finish_setup();
            }
            if ui.button("Show jobs").clicked() {
                self.finish_setup();
                self.tab = Tab::Jobs;
            }
        });
    }

    pub(crate) fn settings_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Settings");
            ui.add_space(6.0);
            let mut g = self.game.clone();
            let mut w = self.work.clone();
            let mut o = self.out.clone();
            egui::Grid::new("settings_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    Self::folder_row(ui, "Game folder", &mut g, "");
                    Self::folder_row(ui, "Work folder", &mut w, "");
                    Self::folder_row(
                        ui,
                        "Export folder",
                        &mut o,
                        "where Export and extract jobs write",
                    );
                    ui.label("Thumbnail cache");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.thumb_budget_mb)
                                .range(16..=65536)
                                .suffix(" MB"),
                        );
                        ui.weak(format!(
                            "{} used in {}",
                            crate::human(self.library.thumbs.used),
                            Path::new(&self.work).join("thumbs").display()
                        ));
                    });
                    ui.end_row();
                });
            let folders_changed = g != self.game || w != self.work;
            self.out = o;
            if folders_changed {
                self.game = g;
                self.work = w;
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("💾 Save").clicked() {
                    let st = self.settings();
                    self.settings_msg = match save_settings(&st) {
                        Ok(()) => "Saved.".into(),
                        Err(e) => format!("Could not save settings: {e}"),
                    };
                    self.library.thumbs.budget = self.thumb_budget_mb << 20;
                    self.rebuild_views();
                }
                if ui.button("Run setup again").clicked() {
                    self.setup = Setup {
                        open: true,
                        ..Default::default()
                    };
                }
                if !self.settings_msg.is_empty() {
                    ui.label(&self.settings_msg);
                }
            });
            ui.add_space(6.0);
            ui.weak(format!(
                "Settings file: {}",
                crate::settings_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(none)".into())
            ));
            ui.weak("The command-line tool (sixng.exe) reads the same game folder.");
        });
    }

    pub(crate) fn status_ui(&mut self, ui: &mut egui::Ui) {
        let work = PathBuf::from(&self.work);
        // background folder sizes
        if !self.status.computing && self.status.sizes.lock().unwrap().is_none() {
            self.status.computing = true;
            if !self.status.previous_read {
                self.status.previous_read = true;
                self.status.previous = std::fs::read(work.join("status_snapshot.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok());
            }
            let slot = self.status.sizes.clone();
            let w = work.clone();
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let mut dirs = Vec::new();
                let mut tops: Vec<PathBuf> = std::fs::read_dir(&w)
                    .map(|rd| {
                        rd.flatten()
                            .map(|e| e.path())
                            .filter(|p| p.is_dir())
                            .collect()
                    })
                    .unwrap_or_default();
                tops.sort();
                for t in tops {
                    let rel = t.file_name().unwrap().to_string_lossy().to_string();
                    if rel == "out" {
                        // one row per output category
                        let mut subs: Vec<PathBuf> = std::fs::read_dir(&t)
                            .map(|rd| {
                                rd.flatten()
                                    .map(|e| e.path())
                                    .filter(|p| p.is_dir())
                                    .collect()
                            })
                            .unwrap_or_default();
                        subs.sort();
                        for s in subs {
                            let (b, f) = tree_size(&s);
                            dirs.push((
                                format!("out/{}", s.file_name().unwrap().to_string_lossy()),
                                b,
                                f,
                            ));
                        }
                        continue;
                    }
                    let (b, f) = tree_size(&t);
                    dirs.push((rel, b, f));
                }
                let snap = Snapshot { when: now(), dirs };
                let _ = std::fs::write(
                    w.join("status_snapshot.json"),
                    serde_json::to_vec(&snap).unwrap_or_default(),
                );
                *slot.lock().unwrap() = Some(snap);
                ctx.request_repaint();
            });
        }
        if !self.status.gaps_loading {
            self.status.gaps_loading = true;
            let slot = self.status.gaps.clone();
            let cands = [
                work.join("texture_gaps.json"),
                PathBuf::from(&self.out).join("texture_gaps.json"),
                work.join("out").join("texture_gaps.json"),
            ];
            std::thread::spawn(move || {
                let r = match cands.iter().find(|p| p.exists()) {
                    None => Err("not computed yet".to_string()),
                    Some(p) => std::fs::read(p).map_err(|e| e.to_string()).and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).map_err(|e| e.to_string())).map(|v| {
                        serde_json::json!({ "summary": v.get("summary").and_then(|s| s.get("materials")).cloned(), "by_class": v.get("summary").and_then(|s| s.get("by_class")).cloned(), "file": p.display().to_string() })
                    }),
                };
                *slot.lock().unwrap() = Some(r);
            });
        }
        let ws = self.ws();
        self.model
            .picker
            .ensure_loaded(ws.clone(), self.model.indexes.clone(), ui.ctx());
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Status");
            ui.add_space(4.0);
            // archives
            let indexed = self.rpf_archives().filter(|a| ws.is_indexed(a)).count();
            egui::CollapsingHeader::new(format!("Archives: {indexed} of {} indexed", self.rpf_archives().count())).default_open(false).show(ui, |ui| {
                let missing: Vec<String> = self.rpf_archives().filter(|a| !ws.is_indexed(a)).map(|a| a.id.clone()).collect();
                if !missing.is_empty() && ui.button(format!("Index the {} missing", missing.len())).clicked() {
                    let mut args = vec!["index".to_string()];
                    args.extend(missing.iter().map(|s| s.trim_end_matches(".rpf").to_string()));
                    self.jobs.enqueue("Index missing archives", args);
                    self.tab = Tab::Jobs;
                }
                egui::Grid::new("st_arch").striped(true).num_columns(3).show(ui, |ui| {
                    for a in self.rpf_archives() {
                        ui.label(&a.id);
                        ui.label(format!("{:.1} GB", a.size as f64 / 1e9));
                        ui.label(if ws.is_indexed(a) { "✔ indexed" } else { "○ not indexed" });
                        ui.end_row();
                    }
                });
            });
            // library
            egui::CollapsingHeader::new("Library").default_open(true).show(ui, |ui| {
                if self.model.picker.items().is_some() {
                    egui::Grid::new("st_lib").num_columns(2).show(ui, |ui| {
                        for (c, label) in crate::model_picker::CATS.iter().copied() {
                            ui.label(label);
                            ui.label(format!("{}", self.model.picker.count(c)));
                            ui.end_row();
                        }
                    });
                    ui.weak(format!("{} thumbnails rendered this session, {} on disk", self.library.thumbs.rendered, crate::human(self.library.thumbs.used)));
                } else {
                    ui.weak("Open the Library once to list the models.");
                }
            });
            // world data / map
            egui::CollapsingHeader::new("World data and map").default_open(true).show(ui, |ui| {
                let (req, opt) = crate::bakejob::BakeJob::missing_inputs(&self.work);
                let baked = work.join("map").join("meta.json");
                if req.is_empty() {
                    ui.label("✔ world data for the map is exported");
                } else {
                    ui.label(format!("○ missing world data: {}", req.join(", ")));
                    if ui.button("Export world data").clicked() {
                        self.jobs.enqueue("Export world data", vec!["world".into(), "all".into(), "-o".into(), self.world_out()]);
                        self.tab = Tab::Jobs;
                    }
                }
                if !opt.is_empty() {
                    ui.weak(format!("optional layers missing: {}", opt.join(", ")));
                }
                match std::fs::metadata(&baked).and_then(|m| m.modified()) {
                    Ok(t) => {
                        let s = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                        ui.label(format!("✔ map baked {}", ago(s)));
                    }
                    Err(_) => {
                        ui.label("○ map not baked");
                    }
                }
            });
            // texture gaps
            egui::CollapsingHeader::new("Texture gaps (BCPack)").default_open(true).show(ui, |ui| {
                ui.weak("BCPack is the Xbox hardware texture codec. All six BCPack formats decode; preset tables are fitted, so a few blocks can be off. A chunk that fails to decode falls back to a lower mip or a flat colour.");
                match &*self.status.gaps.lock().unwrap() {
                    None => {
                        ui.spinner();
                    }
                    Some(Err(e)) => {
                        ui.label(format!("No report ({e})."));
                        if ui.button("Compute the texture-gap report").clicked() {
                            self.jobs.enqueue("Texture gaps", vec!["texture-gaps".into(), "-o".into(), self.world_out()]);
                            self.tab = Tab::Jobs;
                        }
                    }
                    Some(Ok(v)) => {
                        if let Some(m) = v.get("summary").filter(|m| !m.is_null()) {
                            let g = |k: &str| m.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
                            ui.label(format!(
                                "Materials: {:.0} total, {:.0} flat ({:.1}%), {:.0} low-mip ({:.1}%)",
                                g("total"), g("flat"), g("pct_flat"), g("lowmip"), g("pct_lowmip")
                            ));
                        }
                        if let Some(bc) = v.get("by_class").and_then(|b| b.as_object()) {
                            egui::Grid::new("st_gaps").striped(true).num_columns(4).show(ui, |ui| {
                                ui.strong("class");
                                ui.strong("materials");
                                ui.strong("flat");
                                ui.strong("low-mip");
                                ui.end_row();
                                for (k, c) in bc {
                                    let g = |n: &str| c.get(n).and_then(|x| x.as_f64()).unwrap_or(0.0);
                                    ui.label(k);
                                    ui.label(format!("{:.0}", g("materials")));
                                    ui.label(format!("{:.1}%", g("pct_flat")));
                                    ui.label(format!("{:.1}%", g("pct_lowmip")));
                                    ui.end_row();
                                }
                            });
                        }
                        ui.weak(v.get("file").and_then(|f| f.as_str()).unwrap_or(""));
                    }
                }
            });
            // disk
            let cur = self.status.sizes.lock().unwrap().clone();
            egui::CollapsingHeader::new("Disk use in the work folder").default_open(true).show(ui, |ui| match &cur {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("measuring…");
                    });
                }
                Some(s) => {
                    let total: u64 = s.dirs.iter().map(|d| d.1).sum();
                    ui.label(format!("{} in {}", crate::human(total), self.work));
                    let prev = self.status.previous.as_ref();
                    if let Some(p) = prev {
                        ui.weak(format!("changes since the last check ({})", ago(p.when)));
                    }
                    egui::Grid::new("st_disk").striped(true).num_columns(4).show(ui, |ui| {
                        ui.strong("folder");
                        ui.strong("size");
                        ui.strong("files");
                        ui.strong("change");
                        ui.end_row();
                        for (name, b, f) in &s.dirs {
                            ui.label(name);
                            ui.label(crate::human(*b));
                            ui.label(format!("{f}"));
                            let ch = match prev.map(|p| p.dirs.iter().find(|d| &d.0 == name)) {
                                None => String::new(),
                                Some(None) => "new".into(),
                                Some(Some(o)) if o.1 == *b && o.2 == *f => String::new(),
                                Some(Some(o)) => {
                                    let db = *b as i64 - o.1 as i64;
                                    let df = *f as i64 - o.2 as i64;
                                    format!("{}{} , {df:+} files", if db < 0 { "-" } else { "+" }, crate::human(db.unsigned_abs()))
                                }
                            };
                            ui.label(ch);
                            ui.end_row();
                        }
                    });
                    if ui.button("🔄 Measure again").clicked() {
                        self.status.previous = Some(s.clone());
                        *self.status.sizes.lock().unwrap() = None;
                        self.status.computing = false;
                    }
                }
            });
        });
    }

    /// Ctrl+K: jump to a page or a model.
    pub(crate) fn palette_ui(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
            self.palette.open = !self.palette.open;
            self.palette.focus = true;
            self.palette.cursor = 0;
        }
        if !self.palette.open {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.palette.open = false;
            return;
        }
        let ws = self.ws();
        self.model
            .picker
            .ensure_loaded(ws, self.model.indexes.clone(), ctx);
        let q = self.palette.query.to_lowercase();
        let mut hits: Vec<Hit> = PAGES
            .iter()
            .filter(|(_, n)| q.is_empty() || n.to_lowercase().contains(&q))
            .map(|(t, n)| Hit::Page(*t, n))
            .collect();
        if let Some(items) = self.model.picker.items() {
            let label = |id: &str| -> Option<(String, &'static str)> {
                let it = items.iter().find(|it| it.id == id)?;
                Some((
                    format!("{} · {}", crate::library::display_name(&it.name), it.name),
                    "model",
                ))
            };
            if q.is_empty() {
                for (ids, kind) in [
                    (self.model.picker.favourites().to_vec(), "favourite"),
                    (self.model.picker.recents().to_vec(), "recent"),
                ] {
                    for id in ids.iter().take(8) {
                        if let Some((l, _)) = label(id) {
                            hits.push(Hit::Model(id.clone(), l, kind));
                        }
                    }
                }
            } else {
                let mut n = 0;
                for it in items.iter() {
                    if it.lname.contains(&q) && !it.variant {
                        hits.push(Hit::Model(
                            it.id.clone(),
                            format!("{} · {}", crate::library::display_name(&it.name), it.name),
                            "model",
                        ));
                        n += 1;
                        if n >= 40 {
                            break;
                        }
                    }
                }
            }
        }
        let (down, up, enter) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::Enter),
            )
        });
        if down {
            self.palette.cursor = (self.palette.cursor + 1).min(hits.len().saturating_sub(1));
        }
        if up {
            self.palette.cursor = self.palette.cursor.saturating_sub(1);
        }
        let mut chosen: Option<usize> = if enter && !hits.is_empty() {
            Some(self.palette.cursor.min(hits.len() - 1))
        } else {
            None
        };
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("palette"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(screen.center().x - 300.0, screen.top() + 60.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.set_width(600.0);
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.palette.query)
                                .hint_text("Search models and pages…  (Esc closes)")
                                .desired_width(f32::INFINITY),
                        );
                        if self.palette.focus {
                            r.request_focus();
                            self.palette.focus = false;
                        }
                        if r.changed() {
                            self.palette.cursor = 0;
                        }
                        ui.separator();
                        if q.is_empty() {
                            ui.weak(
                                "Pages, favourites and recent models. Type to search all models.",
                            );
                        }
                        egui::ScrollArea::vertical()
                            .max_height(420.0)
                            .min_scrolled_height((hits.len() as f32 * 22.0).min(420.0))
                            .show(ui, |ui| {
                                for (i, h) in hits.iter().enumerate() {
                                    let (txt, kind) = match h {
                                        Hit::Page(_, n) => (n.to_string(), "page"),
                                        Hit::Model(_, l, k) => (l.clone(), *k),
                                    };
                                    let sel = i == self.palette.cursor;
                                    let r = ui.horizontal(|ui| {
                                        let r = ui.selectable_label(sel, txt);
                                        ui.weak(kind);
                                        r
                                    });
                                    if sel && (up || down) {
                                        r.inner.scroll_to_me(None);
                                    }
                                    if r.inner.clicked() {
                                        chosen = Some(i);
                                    }
                                }
                                if hits.is_empty() {
                                    ui.weak("No matches.");
                                }
                            });
                    });
            });
        if let Some(i) = chosen {
            match &hits[i] {
                Hit::Page(t, _) => self.tab = *t,
                Hit::Model(id, _, _) => {
                    self.library.show_item(id);
                    self.tab = Tab::Library;
                }
            }
            self.palette.open = false;
            self.palette.query.clear();
        }
    }
}

impl App {
    /// Automation: open the palette with a query.
    pub(crate) fn palette_query(&mut self, q: &str) {
        self.palette.query = q.to_string();
        self.palette.focus = true;
    }
}

impl App {
    /// The RPF9 archives (the ones `index` covers; texture stores are read directly).
    pub(crate) fn rpf_archives(&self) -> impl Iterator<Item = &sixng_core::game::ArchiveInfo> {
        self.archives.iter().filter(|a| a.container == "rpf9")
    }
}
