//! Scenes tab: the cutscenes of the story pack. Lists the scenes, searches
//! them by name and by cast, and shows a scene's cast, props and cameras;
//! a cast member's scene clip plays on that character in the model viewer.

use crate::audio_view::SceneAudio;
use eframe::egui;
use serde::Deserialize;
use sixng_core::anim::ClipDictionary;
pub use sixng_core::cutscene::mission_stage;
use sixng_core::cutscene::{self, Catalog, Models, Scene};
use sixng_core::export::{Progress, Workspace};
use sixng_core::storyflow::Names;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// The catalog, read once per run (the model viewer's clip loads reuse it).
static CATALOG: Mutex<Option<Arc<Catalog>>> = Mutex::new(None);

/// The story pack's cutscene catalog: from memory, the cache file, or a
/// scan (about 2 minutes, then cached).
pub fn catalog(ws: &Workspace, progress: Progress) -> anyhow::Result<Arc<Catalog>> {
    let mut g = CATALOG.lock().unwrap();
    if let Some(c) = g.as_ref() {
        return Ok(c.clone());
    }
    let c = Arc::new(cutscene::catalog(ws, cutscene::STORY_ARCHIVE, false, progress)?);
    *g = Some(c.clone());
    Ok(c)
}

/// A scene clip dictionary name (`cin@<scene>`).
pub fn is_scene(dict: &str) -> bool {
    dict.starts_with("cin@") || dict.starts_with("cine@")
}

/// A scene clip for the model viewer: `dict` is `cin@<scene>` (or the
/// dictionary's name), `clip` a clip short name or empty for `model`'s own
/// first clip in that scene. Returns the dictionary, with the scene's
/// sections joined so the whole scene plays, and the clip's short name.
pub fn scene_clip(game: &Path, work: &Path, dict: &str, clip: &str, model: &str) -> anyhow::Result<(ClipDictionary, String)> {
    let ws = Workspace::new(game, work);
    let c = catalog(&ws, Arc::new(|_: &str, _, _| {}))?;
    let (sd, cl) = if clip.is_empty() {
        if model.is_empty() {
            anyhow::bail!("name a clip: {dict}/<clip>");
        }
        let picks = cutscene::model_clips(&c, dict, model)?;
        let (sd, cl) = picks.into_iter().next().ok_or_else(|| anyhow::anyhow!("{model} has no clip in {dict}"))?;
        (sd, cl)
    } else {
        let q = dict.strip_prefix("cin@").unwrap_or(dict);
        let sd = c
            .dicts
            .iter()
            .find(|d| d.name.eq_ignore_ascii_case(dict) || cutscene::parse_dict_name(&d.name).is_some_and(|(s, n)| n.is_none() && s.eq_ignore_ascii_case(q)))
            .ok_or_else(|| anyhow::anyhow!("no cutscene dictionary {dict:?}"))?;
        (sd, clip.to_string())
    };
    Ok((cutscene::load_dictionary(&ws, &c, sd)?, cl))
}

/// The scene list with model names resolved, as `sixng cutscenes` builds it.
fn load_scenes(ws: &Workspace, progress: Progress) -> anyhow::Result<Vec<Scene>> {
    let c = catalog(ws, progress.clone())?;
    progress("model names", 0, 1);
    let models = Models::of_workspace(ws);
    Ok(cutscene::scenes(&c, &models))
}

/// Search terms: words split at spaces and commas, lower case.
pub fn terms(q: &str) -> Vec<String> {
    q.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).map(|t| t.to_ascii_lowercase()).collect()
}

/// Whether a scene matches: every name term is part of its name or of its
/// story label (lower case, see [`label`]), and every cast term is part of
/// some cast member's handle or model name.
pub fn matches(s: &Scene, label: &str, name: &[String], cast: &[String]) -> bool {
    let n = s.name.to_ascii_lowercase();
    name.iter().all(|t| n.contains(t.as_str()) || label.contains(t.as_str()))
        && cast.iter().all(|t| s.cast.iter().any(|o| o.handle.to_ascii_lowercase().contains(t.as_str()) || o.model.as_deref().is_some_and(|m| m.to_ascii_lowercase().contains(t.as_str()))))
}

/// Indexes of the scenes that match the two queries, in list order.
#[cfg(test)]
pub fn filter(all: &[Scene], name: &str, cast: &str) -> Vec<usize> {
    filter_labelled(all, &[], name, cast)
}

/// [`filter`], with the scenes' story labels (lower case, by scene index;
/// missing ones count as empty) searched as well.
pub fn filter_labelled(all: &[Scene], labels: &[String], name: &str, cast: &str) -> Vec<usize> {
    let (n, c) = (terms(name), terms(cast));
    all.iter().enumerate().filter(|(i, s)| matches(s, labels.get(*i).map_or("", String::as_str), &n, &c)).map(|(i, _)| i).collect()
}

/// A scene's story label: its arc and mission names ("Arc: Mission"), when
/// the story flow names them.
pub fn label(names: &Names, scene: &str) -> Option<String> {
    names.label(&mission_stage(&scene.to_ascii_lowercase()).0)
}

/// Mission and arc names from the story pack's flow tags.
fn load_names(ws: &Workspace, scenes: &[Scene]) -> anyhow::Result<Names> {
    let ids = cutscene::mission_ids(scenes.iter().map(|s| s.name.as_str()));
    sixng_core::storyflow::names(ws, &ids)
}

/// Text with its digit runs compared as numbers (`p2` before `p10`).
fn natural(s: &str) -> Vec<(u64, String)> {
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(&c) = it.peek() {
        let digit = c.is_ascii_digit();
        let mut run = String::new();
        while let Some(&c) = it.peek() {
            if c.is_ascii_digit() != digit {
                break;
            }
            run.push(c);
            it.next();
        }
        out.push(if digit { (run.parse().unwrap_or(u64::MAX), String::new()) } else { (u64::MAX, run) });
    }
    out
}

/// Sort scene indexes in mission order: missions by name, and in each
/// mission the intro, the mid scenes by number, then the outro; parts,
/// takes and variants in number order after that.
pub fn sort_mission_order(all: &[Scene], shown: &mut [usize]) {
    shown.sort_by_cached_key(|&i| {
        let n = all[i].name.to_ascii_lowercase();
        let (m, r, k) = mission_stage(&n);
        (m, r, k, natural(&n))
    });
}

/// The next selection when stepping `by` through the filtered list.
pub fn step(shown: &[usize], sel: Option<usize>, by: isize) -> Option<usize> {
    if shown.is_empty() {
        return None;
    }
    let at = sel.and_then(|s| shown.iter().position(|&i| i == s));
    let k = match at {
        Some(k) => (k as isize + by).clamp(0, shown.len() as isize - 1) as usize,
        None => 0,
    };
    Some(shown[k])
}

pub enum Action {
    /// Open the character (outfit prefix) and play the clip (`cin@…/<clip>`).
    Play { model: String, clip: String },
    OpenAudio(SceneAudio),
}

#[derive(Deserialize)]
struct DialogueRow {
    speaker: String,
    speaker_hash: u32,
    subtitle_key: u32,
    text: String,
    audio: Option<SceneAudio>,
}

type DialogueRows = BTreeMap<String, Vec<DialogueRow>>;

#[derive(Deserialize)]
struct DialogueFile {
    schema_version: u32,
    scenes: DialogueRows,
}

fn parse_dialogue(bytes: &[u8]) -> anyhow::Result<DialogueRows> {
    let file: DialogueFile = serde_json::from_slice(bytes)?;
    anyhow::ensure!(file.schema_version == 1, "unsupported schema version {}", file.schema_version);
    Ok(file.scenes)
}

fn load_dialogue(work: &Path) -> anyhow::Result<DialogueRows> {
    match std::fs::read(work.join("scene_dialogue.json")) {
        Ok(bytes) => parse_dialogue(&bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.into()),
    }
}

struct LoadedScenes {
    scenes: Arc<Vec<Scene>>,
    dialogue: Result<DialogueRows, String>,
    names: Result<Names, String>,
}

type Loaded = Option<Result<LoadedScenes, String>>;

pub struct ScenesView {
    game: String,
    work: String,
    started: bool,
    pending: Arc<Mutex<Loaded>>,
    progress: Arc<Mutex<(String, u64, u64)>>,
    scenes: Option<Arc<Vec<Scene>>>,
    dialogue: Result<DialogueRows, String>,
    /// Arc and mission names, or why there are none.
    names: Result<Names, String>,
    /// Each scene's story label in lower case, for the name search.
    labels: Vec<String>,
    error: Option<String>,
    pub name_q: String,
    pub cast_q: String,
    /// List scenes in mission order instead of by name.
    pub mission_order: bool,
    shown: Vec<usize>,
    shown_key: (String, String, usize, bool),
    sel: Option<usize>,
    scroll_to_sel: bool,
}

impl ScenesView {
    pub fn new(game: &str, work: &str) -> Self {
        Self {
            game: game.to_string(),
            work: work.to_string(),
            started: false,
            pending: Default::default(),
            progress: Default::default(),
            scenes: None,
            dialogue: Ok(BTreeMap::new()),
            names: Ok(Names::default()),
            labels: Vec::new(),
            error: None,
            name_q: String::new(),
            cast_q: String::new(),
            mission_order: false,
            shown: Vec::new(),
            shown_key: Default::default(),
            sel: None,
            scroll_to_sel: false,
        }
    }

    /// Show the scenes with a character (outfit prefix) in the cast.
    pub fn filter_character(&mut self, model: &str) {
        self.name_q.clear();
        self.cast_q = model.to_string();
        self.sel = None;
    }

    /// Select a scene by name (when loaded) or set the name search.
    pub fn select_scene(&mut self, q: &str) {
        self.name_q = q.strip_prefix("cin@").unwrap_or(q).to_string();
        self.sel = None;
    }

    pub fn idle(&self) -> bool {
        self.scenes.is_some() || self.error.is_some()
    }

    fn start(&mut self, ctx: &egui::Context) {
        self.started = true;
        let ws = Workspace::new(&self.game, &self.work);
        let (slot, prog, ctx) = (self.pending.clone(), self.progress.clone(), ctx.clone());
        std::thread::spawn(move || {
            let c2 = ctx.clone();
            let p: Progress = Arc::new(move |s: &str, d: u64, t: u64| {
                *prog.lock().unwrap() = (s.to_string(), d, t);
                c2.request_repaint();
            });
            let r = load_scenes(&ws, p.clone())
                .map(|scenes| {
                    p("mission names", 0, 1);
                    LoadedScenes {
                        names: load_names(&ws, &scenes).map_err(|e| format!("{e:#}")),
                        scenes: Arc::new(scenes),
                        dialogue: load_dialogue(&ws.work).map_err(|e| format!("{e:#}")),
                    }
                })
                .map_err(|e| format!("{e:#}"));
            *slot.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        if !self.started {
            self.start(ui.ctx());
        }
        if let Some(r) = self.pending.lock().unwrap().take() {
            match r {
                Ok(loaded) => {
                    self.labels = match &loaded.names {
                        Ok(n) => loaded.scenes.iter().map(|s| label(n, &s.name).unwrap_or_default().to_lowercase()).collect(),
                        Err(_) => Vec::new(),
                    };
                    self.scenes = Some(loaded.scenes);
                    self.dialogue = loaded.dialogue;
                    self.names = loaded.names;
                }
                Err(e) => self.error = Some(e),
            }
        }
        if let Some(e) = &self.error {
            ui.heading("Scenes");
            ui.colored_label(egui::Color32::LIGHT_RED, format!("Could not load the cutscenes: {e}"));
            if ui.button("Retry").clicked() {
                self.error = None;
                self.started = false;
            }
            return None;
        }
        let Some(all) = self.scenes.clone() else {
            ui.heading("Scenes");
            let (s, d, t) = self.progress.lock().unwrap().clone();
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(if s.is_empty() { "Loading the cutscene catalog …".to_string() } else { format!("Loading the cutscene catalog: {s}") });
            });
            if t > 0 {
                ui.add(egui::ProgressBar::new(d as f32 / t as f32).show_percentage());
            }
            ui.label(egui::RichText::new("The first time, the story pack is scanned for its scenes (about 2 minutes). After that the catalog is read from the cache.").weak().small());
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            return None;
        };
        let key = (self.name_q.clone(), self.cast_q.clone(), all.len(), self.mission_order);
        if key != self.shown_key {
            self.shown = filter_labelled(&all, &self.labels, &self.name_q, &self.cast_q);
            if self.mission_order {
                sort_mission_order(&all, &mut self.shown);
            }
            self.shown_key = key;
            if self.sel.is_none_or(|s| !self.shown.contains(&s)) {
                self.sel = self.shown.first().copied();
            }
            self.scroll_to_sel = true;
        }
        let mut act = None;
        egui::Panel::left("scenes_list").resizable(true).default_size(380.0).show(ui, |ui| {
            ui.heading("Scenes");
            egui::Grid::new("scenes_search").num_columns(2).show(ui, |ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut self.name_q).hint_text("words of the scene or mission name").desired_width(250.0));
                ui.end_row();
                ui.label("Cast");
                ui.add(egui::TextEdit::singleline(&mut self.cast_q).hint_text("lucia, player_story000_01 …").desired_width(250.0)).on_hover_text("Scenes whose cast has every word: each is part of a cast member's scene handle (lucia) or model (player_story000_00 is Lucia, player_story000_01 is Jason).");
                ui.end_row();
                ui.label("Order");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.mission_order, false, "A–Z");
                    ui.selectable_value(&mut self.mission_order, true, "Mission order").on_hover_text("Missions by name; in each mission the intro (int), the mid scenes (mcs1, mcs2 …), then the outro (ext), with parts and takes in number order. This is read from the scene names: the order of the missions in the story is not known yet.");
                });
                ui.end_row();
            });
            ui.horizontal(|ui| {
                let mut by = 0;
                if ui.button("◀ Previous").clicked() {
                    by = -1;
                }
                if ui.button("Next ▶").clicked() {
                    by = 1;
                }
                if !ui.ctx().egui_wants_keyboard_input() {
                    ui.input(|i| {
                        if i.key_pressed(egui::Key::ArrowUp) {
                            by = -1;
                        }
                        if i.key_pressed(egui::Key::ArrowDown) {
                            by = 1;
                        }
                    });
                }
                if by != 0 {
                    self.sel = step(&self.shown, self.sel, by);
                    self.scroll_to_sel = true;
                }
                ui.label(format!("{} of {} scenes", self.shown.len(), all.len()));
            });
            if let Err(e) = &self.names {
                ui.label(egui::RichText::new("Mission names not available").weak().small()).on_hover_text(format!("The arc and mission names come from the story flow tags in the story pack, read through its table of contents (Windows x64, with the game executable in the game folder). {e}"));
            }
            ui.separator();
            let row_h = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
            let mut sa = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if std::mem::take(&mut self.scroll_to_sel) {
                if let Some(k) = self.sel.and_then(|s| self.shown.iter().position(|&i| i == s)) {
                    sa = sa.vertical_scroll_offset((k as f32 * row_h - 120.0).max(0.0));
                }
            }
            sa.show_rows(ui, row_h, self.shown.len(), |ui, rows| {
                for k in rows {
                    let i = self.shown[k];
                    let s = &all[i];
                    let story = self.names.as_ref().ok().and_then(|n| label(n, &s.name)).map(|l| format!("{l}
")).unwrap_or_default();
                    if ui.selectable_label(self.sel == Some(i), &s.name).on_hover_text(format!("{story}{} cast, {} props, {} cameras", s.cast.len(), s.props.len(), s.cameras.len())).clicked() {
                        self.sel = Some(i);
                    }
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let Some(s) = self.sel.and_then(|i| all.get(i)) else {
                ui.label("No scene matches.");
                return;
            };
            let dialogue = self.dialogue.as_ref().map(|rows| {
                rows.get(&s.name).map(Vec::as_slice).unwrap_or_default()
            });
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                act = scene_ui(ui, s, self.names.as_ref().ok(), dialogue.map_err(String::as_str));
            });
        });
        act
    }
}

fn scene_ui(ui: &mut egui::Ui, s: &Scene, names: Option<&Names>, dialogue: Result<&[DialogueRow], &str>) -> Option<Action> {
    let mut act = None;
    ui.heading(&s.name);
    egui::Grid::new("scene_facts").num_columns(2).show(ui, |ui| {
        ui.label("Dictionary");
        ui.label(egui::RichText::new(&s.dictionary).monospace());
        ui.end_row();
        if let Some(v) = &s.variant_of {
            ui.label("Variant of");
            ui.label(v);
            ui.end_row();
        }
        let (m, r, k) = mission_stage(&s.name.to_ascii_lowercase());
        if let Some(l) = names.and_then(|n| n.label(&m)) {
            ui.label("Story");
            ui.label(egui::RichText::new(l).strong()).on_hover_text("Arc and mission names from the story flow tags (for example FlowHarborBoatRaceComplete), paired with the mission id because the id is a short form of the tag's words. Only tags that fit one id are used, so some missions have only their arc's name and some have none.");
            ui.end_row();
        }
        ui.label("Mission");
        ui.label(format!("{m}, {}", match (r, k) {
            (0, 0) => "intro".to_string(),
            (0, k) => format!("intro {k}"),
            (2, 0) => "outro".to_string(),
            (2, k) => format!("outro {k}"),
            (_, 0) => "scene".to_string(),
            (_, k) => format!("mid scene {k}"),
        }));
        ui.end_row();
        ui.label("Sections");
        ui.label(if s.windows_missing > 0 { format!("{} (joined; {} clip windows not found)", s.sections, s.windows_missing) } else { format!("{} (joined)", s.sections) });
        ui.end_row();
        ui.label("Scene files");
        ui.label(s.scene_files.len().to_string());
        ui.end_row();
    });
    ui.add_space(6.0);
    ui.heading(format!("Cast ({})", s.cast.len()));
    egui::Grid::new("scene_cast").num_columns(4).striped(true).show(ui, |ui| {
        ui.strong("Handle");
        ui.strong("Model");
        ui.strong("Clips");
        ui.strong("");
        ui.end_row();
        for o in &s.cast {
            ui.label(&o.handle);
            let model = o.model.clone().unwrap_or_else(|| "—".into());
            let r = ui.label(egui::RichText::new(&model).monospace());
            let mut hover = match (o.kind, o.model_source) {
                ("character", _) => "A character the viewer puts together.".to_string(),
                (_, None) => "A ped with no model record in the scene files: only its handle is known.".to_string(),
                _ => "A ped model the viewer does not put together as a character.".to_string(),
            };
            if o.model_source == Some("other scenes") {
                hover += " The model comes from the records of other scenes with this handle.";
            }
            r.on_hover_text(hover);
            ui.label(format!("{}{}", o.clips.len(), if o.face_clips.is_empty() { String::new() } else { format!(" + {} face", o.face_clips.len()) }));
            ui.horizontal(|ui| {
                if let (Some(m), "character") = (&o.model, o.kind) {
                    for cl in &o.clips {
                        let label = if o.clips.len() > 1 { format!("▶ {cl}") } else { "▶ Play".to_string() };
                        if ui.button(label).on_hover_text(format!("Open {m} in the model viewer and play its clip {cl} of this scene, with all the scene's sections joined")).clicked() {
                            act = Some(Action::Play { model: m.clone(), clip: format!("{}/{cl}", s.dictionary) });
                        }
                    }
                }
            });
            ui.end_row();
        }
    });
    ui.label(egui::RichText::new("Each clip starts at the character's own origin: where the characters stand in the scene relative to each other is not known.").weak().small());
    ui.add_space(6.0);
    match dialogue {
        Ok(rows) if !rows.is_empty() => {
            egui::CollapsingHeader::new(format!("Dialogue ({})", rows.len()))
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new("scene_dialogue").striped(true).show(ui, |ui| {
                        ui.strong("Speaker");
                        ui.strong("Subtitle key");
                        ui.strong("Dialogue");
                        ui.strong("Audio");
                        ui.end_row();
                        for row in rows {
                            ui.label(&row.speaker)
                                .on_hover_text(format!("Native speaker hash {:08x}", row.speaker_hash));
                            ui.monospace(format!("{:08x}", row.subtitle_key));
                            ui.add(egui::Label::new(&row.text).wrap());
                            if let Some(address) = &row.audio {
                                let label = if address.stem.is_some() { "Open voice excerpt" } else { "Open in Audio" };
                                if ui.button(label).clicked() {
                                    act = Some(Action::OpenAudio(address.clone()));
                                }
                            } else {
                                ui.weak("Unresolved");
                            }
                            ui.end_row();
                        }
                    });
                });
        }
        Err(e) => {
            ui.colored_label(ui.visuals().error_fg_color, format!("Could not load scene_dialogue.json: {e}"));
        }
        Ok(_) => {}
    }
    egui::CollapsingHeader::new(format!("Props ({})", s.props.len())).default_open(true).show(ui, |ui| {
        for o in &s.props {
            ui.label(match &o.model {
                Some(m) if *m != o.handle.to_ascii_lowercase() => format!("{} ({m})", o.handle),
                _ => o.handle.clone(),
            });
        }
    });
    egui::CollapsingHeader::new(format!("Cameras ({})", s.cameras.len())).default_open(true).show(ui, |ui| {
        for c in &s.cameras {
            ui.label(c);
        }
    });
    if !s.other.is_empty() {
        egui::CollapsingHeader::new(format!("Other objects ({})", s.other.len())).show(ui, |ui| {
            for o in &s.other {
                ui.label(&o.handle);
            }
        });
    }
    if !s.set_models.is_empty() || !s.set_models_unresolved.is_empty() {
        egui::CollapsingHeader::new(format!("Set models ({})", s.set_models.len() + s.set_models_unresolved.len())).show(ui, |ui| {
            for m in &s.set_models {
                ui.label(m);
            }
            if !s.set_models_unresolved.is_empty() {
                ui.label(egui::RichText::new(format!("{} more with no known name", s.set_models_unresolved.len())).weak());
            }
        });
    }
    act
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_dialogue_preserves_occurrences_with_and_without_audio() {
        let rows = parse_dialogue(br#"{
            "schema_version": 1,
            "scenes": {
                "fixture_scene": [
                    {"speaker": "Guide", "speaker_hash": 1, "subtitle_key": 3,
                     "text": "The lights are on.",
                     "audio": {"archive": "local/fixture.rpf", "bank_offset": 4096,
                               "stream_index": 1, "stream_id": 66, "data_offset": 4352, "data_size": 512,
                               "stem": {"channel_id": 52, "start_frame": 120, "end_frame": 240}}},
                    {"speaker": "Guide", "speaker_hash": 2, "subtitle_key": 3,
                     "text": "The lights are on."}
                ]
            }
        }"#).unwrap();
        let rows = &rows["fixture_scene"];
        assert_eq!(rows.len(), 2);
        let audio = rows[0].audio.as_ref().unwrap();
        assert_eq!(audio.address.stream_index, 1);
        assert_eq!(audio.stem.unwrap().channel_id, 52);
        assert!(rows[1].audio.is_none());
        assert_ne!(rows[0].speaker_hash, rows[1].speaker_hash);
        assert_eq!(rows[0].subtitle_key, rows[1].subtitle_key);
        assert!(parse_dialogue(br#"{"schema_version":2,"scenes":{}}"#).is_err());
    }
    use sixng_core::cutscene::SceneObject;

    fn obj(handle: &str, kind: &'static str, model: Option<&str>) -> SceneObject {
        SceneObject { handle: handle.into(), kind, model: model.map(String::from), model_id: None, model_source: None, clips: vec![handle.into()], face_clips: vec![], streamed: false }
    }

    fn scene(name: &str, cast: Vec<SceneObject>) -> Scene {
        Scene {
            name: name.into(),
            variant_of: None,
            dictionary: format!("cin@{name}"),
            dictionary_id: String::new(),
            sections: 0,
            windows_streamed: 0,
            windows_missing: 0,
            unjoined: vec![],
            scene_files: vec![],
            cast,
            props: vec![obj("prop_cup", "prop", Some("prop_cup"))],
            cameras: vec![],
            other: vec![],
            set_models: vec![],
            set_models_unresolved: vec![],
        }
    }

    fn sample() -> Vec<Scene> {
        vec![
            scene("mtl_mont_mcs1_arrive_at_diner_p1_t05", vec![obj("lucia", "character", Some("player_story000_00")), obj("jason", "character", Some("player_story000_01"))]),
            scene("air_beach_ext_p1_t05", vec![obj("Lucia", "character", Some("player_story000_00")), obj("nate", "ped", None)]),
            scene("bil_confr_mcs5_confrontation_p1_t09", vec![obj("jason", "character", Some("player_story000_01"))]),
        ]
    }

    #[test]
    fn name_search_takes_every_word_in_any_case() {
        let all = sample();
        assert_eq!(filter(&all, "", ""), vec![0, 1, 2]);
        assert_eq!(filter(&all, "DINER", ""), vec![0]);
        assert_eq!(filter(&all, "p1 t05", ""), vec![0, 1]);
        assert_eq!(filter(&all, "t05 confr", ""), Vec::<usize>::new());
    }

    #[test]
    fn cast_filter_matches_handles_and_models_and_needs_every_term() {
        let all = sample();
        // by model: Lucia
        assert_eq!(filter(&all, "", "player_story000_00"), vec![0, 1]);
        // by handle, any case
        assert_eq!(filter(&all, "", "LUCIA"), vec![0, 1]);
        // a ped known only by its handle
        assert_eq!(filter(&all, "", "nate"), vec![1]);
        // both characters
        assert_eq!(filter(&all, "", "lucia, jason"), vec![0]);
        assert_eq!(filter(&all, "", "player_story000_00 player_story000_01"), vec![0]);
        // props are not cast
        assert_eq!(filter(&all, "", "prop_cup"), Vec::<usize>::new());
        // name and cast together
        assert_eq!(filter(&all, "beach", "lucia"), vec![1]);
    }

    #[test]
    fn name_search_also_finds_the_story_label() {
        let all = sample();
        let mut names = Names::default();
        names.arcs.insert("mtl".into(), "Harbor".into());
        names.arcs.insert("air".into(), "Harbor".into());
        names.missions.insert("mtl_mont".into(), "Boat Race".into());
        let labels: Vec<String> = all.iter().map(|s| label(&names, &s.name).unwrap_or_default().to_lowercase()).collect();
        assert_eq!(label(&names, &all[0].name).as_deref(), Some("Harbor: Boat Race"));
        assert_eq!(label(&names, &all[1].name).as_deref(), Some("Harbor"));
        assert_eq!(label(&names, &all[2].name), None);
        assert_eq!(filter_labelled(&all, &labels, "boat race", ""), vec![0]);
        assert_eq!(filter_labelled(&all, &labels, "harbor", ""), vec![0, 1]);
        // A term may match the name and another the label.
        assert_eq!(filter_labelled(&all, &labels, "harbor diner", ""), vec![0]);
        assert_eq!(filter_labelled(&all, &labels, "harbor", "nate"), vec![1]);
        assert!(filter_labelled(&all, &[], "harbor", "").is_empty());
    }

    #[test]
    fn mission_stage_from_the_name() {
        assert_eq!(mission_stage("air_cater_ext_p1a1_t05_x1"), ("air_cater".into(), 2, 0));
        assert_eq!(mission_stage("mtl_mont_mcs1_arrive_at_diner_p1_t05"), ("mtl_mont".into(), 1, 1));
        assert_eq!(mission_stage("com_cbo_cbint_mcs12_meet"), ("com_cbo_cbint".into(), 1, 12));
        assert_eq!(mission_stage("com_cbo_cbint_int"), ("com_cbo_cbint".into(), 0, 0));
        assert_eq!(mission_stage("bil_x_int1a_p1_t01"), ("bil_x".into(), 0, 1));
        assert_eq!(mission_stage("bil_x_intro_p1"), ("bil_x".into(), 0, 0));
        assert_eq!(mission_stage("bil_x_ext2_p1"), ("bil_x".into(), 2, 2));
        // words that only start like a stage are not one
        assert_eq!(mission_stage("ban_interior_mcs2_p1"), ("ban_interior".into(), 1, 2));
        assert_eq!(mission_stage("collehunt_a_b"), ("collehunt_a".into(), 1, 0));
    }

    #[test]
    fn mission_order_puts_intro_mid_scenes_and_outro_in_turn() {
        let names = ["air_x_ext_p1_t01", "air_x_mcs10_a_p1_t01", "air_x_mcs2_a_p10_t01", "air_x_mcs2_a_p2_t01", "air_x_int_p1_t01", "air_x_int_p1_t01_x1", "air_w_ext_p1_t01", "air_x_mcs2_a_p2_t01_x1"];
        let all: Vec<Scene> = names.iter().map(|n| scene(n, vec![])).collect();
        let mut shown: Vec<usize> = (0..all.len()).collect();
        sort_mission_order(&all, &mut shown);
        let got: Vec<&str> = shown.iter().map(|&i| all[i].name.as_str()).collect();
        assert_eq!(got, ["air_w_ext_p1_t01", "air_x_int_p1_t01", "air_x_int_p1_t01_x1", "air_x_mcs2_a_p2_t01", "air_x_mcs2_a_p2_t01_x1", "air_x_mcs2_a_p10_t01", "air_x_mcs10_a_p1_t01", "air_x_ext_p1_t01"]);
    }

    #[test]
    fn stepping_stays_in_the_filtered_list() {
        let shown = vec![2, 5, 9];
        assert_eq!(step(&shown, None, 1), Some(2));
        assert_eq!(step(&shown, Some(2), 1), Some(5));
        assert_eq!(step(&shown, Some(9), 1), Some(9));
        assert_eq!(step(&shown, Some(2), -1), Some(2));
        assert_eq!(step(&shown, Some(5), -1), Some(2));
        // a selection outside the list starts at its first scene
        assert_eq!(step(&shown, Some(7), 1), Some(2));
        assert_eq!(step(&[], Some(1), 1), None);
    }

    #[test]
    fn scene_dictionary_names() {
        assert!(is_scene("cin@air_beach_ext_p1_t05"));
        assert!(is_scene("cine@cine@x"));
        assert!(!is_scene("gameplay@locomotion@x"));
        assert_eq!(terms(" Lucia,jason  nate "), vec!["lucia", "jason", "nate"]);
    }
}
