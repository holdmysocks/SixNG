//! The Animation section's pickers: a clip dictionary of `anim.rpf`, then a
//! clip in it. Both lists are read on worker threads (the dictionary names
//! once per run, a dictionary's clips when it is picked), so the panel does
//! not wait on them.

use eframe::egui;
use sixng_core::anim::{self, ClipFit, ClipInfo, DictEntry};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

type Slot<T> = Arc<Mutex<Option<T>>>;

#[derive(Default)]
pub struct ClipPicker {
    /// Every clip dictionary of anim.rpf (`anim::dictionaries`), once read.
    all: Option<Result<Vec<DictEntry>, String>>,
    all_slot: Slot<Result<Vec<DictEntry>, String>>,
    all_loading: bool,
    /// List every dictionary, not only those that fit the model.
    show_all: bool,
    /// The names of the dictionaries that fit, for `fit_key`.
    fit_key: Option<ClipFit>,
    fit: Vec<String>,
    /// The filter typed in the dictionary list, and the indices into `fit`
    /// that match it (for `shown_for`).
    filter: String,
    shown_for: Option<String>,
    shown: Vec<usize>,
    clip_filter: String,
    /// Clip lists by dictionary name.
    clips: HashMap<String, Result<Vec<ClipInfo>, String>>,
    clips_loading: Option<String>,
    clips_slot: Slot<(String, Result<Vec<ClipInfo>, String>)>,
    /// The text box for a typed or pasted name is open.
    typing: bool,
}

/// `<dictionary>[/<clip>]` split in two (as the clip loader does).
fn split(want: &str) -> (&str, &str) {
    let w = want.trim();
    w.rsplit_once('/').unwrap_or((w, ""))
}

/// Not a name of the anim.rpf list: a cutscene or an entry id.
fn listed_elsewhere(dict: &str) -> bool {
    crate::scenes_view::is_scene(dict) || dict.contains(".rpf@")
}

/// A combo box whose popup has a filter field on top and a scrolled list
/// below of the rows `shown` (each drawn by `row`, which returns whether it
/// was picked). Returns the picked row. The popup stays open while the
/// filter is typed in and closes on a pick.
fn filtered_combo(ui: &mut egui::Ui, id: &str, selected: egui::WidgetText, width: f32, filter: &mut String, note: &str, shown: &[usize], mut row: impl FnMut(&mut egui::Ui, usize) -> bool) -> Option<usize> {
    let mut picked = None;
    egui::ComboBox::from_id_salt(id).selected_text(selected).width(width).truncate().height(440.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show_ui(ui, |ui| {
        let r = ui.add(egui::TextEdit::singleline(filter).hint_text("filter: words of the name").desired_width(f32::INFINITY));
        if ui.memory(|m| m.focused().is_none()) {
            r.request_focus();
        }
        ui.label(egui::RichText::new(note).weak().small());
        let h = ui.spacing().interact_size.y;
        egui::ScrollArea::vertical().id_salt((id, "rows")).max_height(360.0).auto_shrink([false, true]).show_rows(ui, h, shown.len(), |ui, range| {
            for k in range {
                if row(ui, shown[k]) {
                    picked = Some(shown[k]);
                }
            }
        });
        if picked.is_some() {
            ui.close();
        }
    });
    picked
}

impl ClipPicker {
    /// Take in what the worker threads have read, and start reading what
    /// is missing.
    fn poll(&mut self, ctx: &egui::Context, game: &str, work: &str, dict: &str) {
        if let Some(r) = self.all_slot.lock().unwrap().take() {
            self.all = Some(r);
            self.all_loading = false;
            self.fit_key = None;
        }
        if let Some((d, r)) = self.clips_slot.lock().unwrap().take() {
            self.clips.insert(d, r);
            self.clips_loading = None;
        }
        if self.all.is_none() && !self.all_loading {
            self.all_loading = true;
            let (game, work, slot, ctx) = (PathBuf::from(game), PathBuf::from(work), self.all_slot.clone(), ctx.clone());
            std::thread::spawn(move || {
                *slot.lock().unwrap() = Some(anim::dictionaries(&game, &work).map_err(|e| format!("{e:#}")));
                ctx.request_repaint();
            });
        }
        let known = self.all.as_ref().and_then(|r| r.as_ref().ok()).is_some_and(|all| all.iter().any(|d| d.name.eq_ignore_ascii_case(dict)));
        if known && self.clips_loading.is_none() && !self.clips.contains_key(dict) {
            self.clips_loading = Some(dict.to_string());
            let (game, work, slot, ctx, d) = (PathBuf::from(game), PathBuf::from(work), self.clips_slot.clone(), ctx.clone(), dict.to_string());
            std::thread::spawn(move || {
                let r = anim::load_dictionary(&game, &work, &d).map(|(_, cd)| anim::clip_list(&cd)).map_err(|e| format!("{e:#}"));
                *slot.lock().unwrap() = Some((d, r));
                ctx.request_repaint();
            });
        }
    }

    /// The pickers for `want` (`<dictionary>[/<clip>]`, as the clip loader
    /// takes it), listing the dictionaries that `fit` the model. Returns
    /// true when a dictionary or clip was picked or a name typed.
    pub fn ui(&mut self, ui: &mut egui::Ui, want: &mut String, fit: ClipFit, game: &str, work: &str) -> bool {
        let (dict, clip) = {
            let (d, c) = split(want);
            (d.to_string(), c.to_string())
        };
        self.poll(ui.ctx(), game, work, &dict);
        let fit = if self.show_all { ClipFit::Any } else { fit };
        if let Some(Ok(all)) = &self.all {
            if self.fit_key.as_ref() != Some(&fit) {
                self.fit = anim::fitting_dictionaries(all, &fit).into_iter().map(|d| d.name.clone()).collect();
                self.fit_key = Some(fit.clone());
                self.shown_for = None;
            }
        }
        if self.shown_for.as_deref() != Some(self.filter.as_str()) {
            self.shown = self.fit.iter().enumerate().filter(|(_, n)| anim::name_matches(n, &self.filter)).map(|(k, _)| k).collect();
            self.shown_for = Some(self.filter.clone());
        }
        let mut picked = false;
        ui.horizontal(|ui| {
            match &self.all {
                Some(Ok(_)) => {
                    let selected: egui::WidgetText = if dict.is_empty() { egui::RichText::new(format!("pick one of {} dictionaries", self.fit.len())).weak().into() } else { dict.clone().into() };
                    let note = format!("{} of {} {}", self.shown.len(), self.fit.len(), if self.show_all { "dictionaries" } else { "dictionaries that fit this model" });
                    let (fit_names, shown) = (&self.fit, &self.shown);
                    if let Some(k) = filtered_combo(ui, "clip_dictionary", selected, 300.0, &mut self.filter, &note, shown, |ui, k| ui.selectable_label(fit_names[k].eq_ignore_ascii_case(&dict), &fit_names[k]).clicked()) {
                        *want = self.fit[k].clone();
                        picked = true;
                    }
                }
                Some(Err(_)) => {
                    ui.label(egui::RichText::new("no dictionary list").weak());
                }
                None => {
                    ui.spinner();
                    ui.label(egui::RichText::new("listing clip dictionaries …").weak());
                }
            }
            if ui.selectable_label(self.typing, "✏").on_hover_text("Type or paste a name: <dictionary>[/<clip>], an entry id (anim.rpf@…), or a cutscene (cin@<scene>[/<clip>])").clicked() {
                self.typing = !self.typing;
            }
            if ui.button("None").clicked() {
                want.clear();
                picked = true;
            }
        });
        if !dict.is_empty() && !listed_elsewhere(&dict) {
            ui.horizontal(|ui| match self.clips.get(&dict) {
                Some(Ok(clips)) => {
                    let current = clips.iter().position(|c| c.name.eq_ignore_ascii_case(&clip));
                    let selected: egui::WidgetText = match current {
                        Some(k) => clips[k].name.clone().into(),
                        None if clip.is_empty() => egui::RichText::new("default clip (move or idle)").weak().into(),
                        None => clip.clone().into(),
                    };
                    // row 0 is the default; clip k is row k + 1
                    let shown: Vec<usize> = std::iter::once(0).chain(clips.iter().enumerate().filter(|(_, c)| anim::name_matches(&c.name, &self.clip_filter)).map(|(k, _)| k + 1)).collect();
                    let playable = clips.iter().filter(|c| c.playable).count();
                    let note = format!("{} clips, {playable} with decoded animation data", clips.len());
                    let row = |ui: &mut egui::Ui, k: usize| {
                        if k == 0 {
                            return ui.selectable_label(clip.is_empty(), "default clip (move or idle)").on_hover_text("The dictionary alone: its move or idle clip, else the first one with decoded animation data").clicked();
                        }
                        let c = &clips[k - 1];
                        let text = format!("{}  ({:.2} s)", c.name, c.duration);
                        let text = if c.playable { egui::RichText::new(text) } else { egui::RichText::new(text).weak() };
                        let r = ui.selectable_label(current == Some(k - 1), text);
                        let r = if c.playable { r } else { r.on_hover_text("Its animation data is stored elsewhere and not decoded: it does not play") };
                        r.clicked()
                    };
                    if let Some(k) = filtered_combo(ui, "clip_in_dictionary", selected, 300.0, &mut self.clip_filter, &note, &shown, row) {
                        *want = if k == 0 { dict.clone() } else { format!("{dict}/{}", clips[k - 1].name) };
                        picked = true;
                    }
                }
                Some(Err(e)) => {
                    ui.label(egui::RichText::new(format!("clips not listed: {e}")).weak().small());
                }
                None => {
                    ui.spinner();
                    ui.label(egui::RichText::new("listing clips …").weak());
                }
            });
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_all, "all dictionaries").on_hover_text("List every clip dictionary of anim.rpf, not only those for people (or for this animal's species)");
        });
        if self.typing || listed_elsewhere(&dict) {
            let r = ui.add(egui::TextEdit::singleline(want).hint_text("<dictionary>[/<clip>]").desired_width(300.0));
            picked |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        }
        if let Some(Err(e)) = &self.all {
            ui.label(egui::RichText::new(format!("The clip dictionaries are not listed: {e}. A name can still be typed (✏).")).weak().small());
        } else if self.all.is_none() {
            ui.label(egui::RichText::new("The first time, this reads anim.rpf for about a minute; the list is then cached in the index folder.").weak().small());
        }
        picked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn want_splits_at_the_last_slash() {
        assert_eq!(split(" a@b/move "), ("a@b", "move"));
        assert_eq!(split("a@b"), ("a@b", ""));
        assert!(listed_elsewhere("anim.rpf@1a2b"));
        assert!(!listed_elsewhere("gameplay@locomotion@x"));
    }
}
