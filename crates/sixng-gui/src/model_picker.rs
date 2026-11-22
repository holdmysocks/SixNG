//! Model picker for the model viewer: every drawable / fragment in the
//! indexed archives, grouped by category, searchable, with recents and
//! favourites. Click (or ↑/↓) loads a model.

use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sixng_core::export::Workspace;
use sixng_core::scan::ArchiveIndex;

/// Archives with models (drawables `dr`, fragments `ft`).
const ARCHIVES: &[&str] = &["levels", "packs"];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Cat {
    All,
    Vehicles,
    VehicleParts,
    Wheels,
    Weapons,
    People,
    Animals,
    Props,
    Vegetation,
    Interiors,
    Buildings,
    Unnamed,
}

pub const CATS: &[(Cat, &str)] = &[
    (Cat::All, "All"),
    (Cat::Vehicles, "Vehicles"),
    (Cat::VehicleParts, "Vehicle parts"),
    (Cat::Wheels, "Wheels"),
    (Cat::Weapons, "Weapons"),
    (Cat::People, "People"),
    (Cat::Animals, "Animals"),
    (Cat::Props, "Props"),
    (Cat::Vegetation, "Vegetation"),
    (Cat::Interiors, "Interiors"),
    (Cat::Buildings, "Buildings & map"),
    (Cat::Unnamed, "Unnamed"),
];

pub struct Item {
    pub id: String,
    pub name: String,
    pub lname: String,
    pub rtype: &'static str,
    pub cat: Cat,
    pub bytes: u64,
    /// LOD / batch copy (`_lod`, `_slod2`, `batchlod`, `_mid` ...).
    pub variant: bool,
}

fn is_variant(n: &str) -> bool {
    let n = n.trim_end_matches(|c: char| c.is_ascii_digit());
    [
        "_lod",
        "_slod",
        "batchlod",
        "_batch_lod",
        "_batch_slod",
        "_mid",
        "_lo",
    ]
    .iter()
    .any(|s| n.ends_with(s))
}

fn categorise(name: Option<&str>, rtype: &str, vehicle_re: &regex::Regex) -> Cat {
    let Some(n) = name else { return Cat::Unnamed };
    let n = n.to_ascii_lowercase();
    if crate::assemble::is_animal(&n) {
        return Cat::Animals;
    }
    if crate::assemble::is_ped_part(&n) || crate::assemble::is_whole_ped(&n) {
        return Cat::People;
    }
    if n.starts_with("rim_") || n.starts_with("tyre_") || n.starts_with("wheel_") {
        Cat::Wheels
    } else if n.starts_with("w_") || n.starts_with("wpn_") {
        Cat::Weapons
    } else if n.starts_with("prop_")
        || n.starts_with("p_")
        || n.starts_with("prefab_")
        || n.starts_with("le_")
    {
        Cat::Props
    } else if rtype == "ft" && n.starts_with("cmn_") {
        Cat::VehicleParts
    } else if rtype == "ft" && !vehicle_re.is_match(&n) {
        // refined after all names are known (vehicle vs part), see refine_vehicles
        Cat::Vehicles
    } else if n.starts_with("veg_") || n.contains("_tree") || n.contains("_bush") {
        Cat::Vegetation
    } else if n.starts_with("int_") || n.starts_with("v_") || n.contains("_int_") {
        Cat::Interiors
    } else {
        Cat::Buildings
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Saved {
    recents: Vec<String>,
    favourites: Vec<String>,
}

pub struct ModelPicker {
    items: Option<Arc<Vec<Item>>>,
    pending: Arc<Mutex<Option<Result<Vec<Item>, String>>>>,
    loading: bool,
    /// Last load failed: wait for Retry instead of reloading every frame.
    failed: bool,
    pub status: String,
    pub cat: Cat,
    pub search: String,
    pub favs_only: bool,
    pub hide_variants: bool,
    filtered: Vec<u32>,
    dirty: bool,
    /// Index into `filtered` of the current model.
    pub cursor: Option<usize>,
    scroll_to_cursor: bool,
    counts: HashMap<Cat, usize>,
    by_id: HashMap<String, u32>,
    recents: Vec<String>,
    favourites: Vec<String>,
    settings: PathBuf,
    /// One-click indexing of missing archives: (stage text, done, total, finished result).
    indexing: Option<Arc<Mutex<(String, u64, u64, Option<Result<(), String>>)>>>,
}

impl ModelPicker {
    pub fn new(work: &str) -> Self {
        let settings = Path::new(work).join("gui_models.json");
        let saved: Saved = std::fs::read(&settings)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        ModelPicker {
            items: None,
            pending: Arc::new(Mutex::new(None)),
            loading: false,
            failed: false,
            indexing: None,
            status: String::new(),
            cat: Cat::Vehicles,
            search: String::new(),
            favs_only: false,
            hide_variants: true,
            filtered: Vec::new(),
            dirty: true,
            cursor: None,
            scroll_to_cursor: false,
            counts: HashMap::new(),
            by_id: HashMap::new(),
            recents: saved.recents,
            favourites: saved.favourites,
            settings,
        }
    }

    /// The full model list once loaded (shared with the Library).
    pub fn items(&self) -> Option<Arc<Vec<Item>>> {
        self.items.clone()
    }

    pub fn count(&self, c: Cat) -> usize {
        self.counts.get(&c).copied().unwrap_or(0)
    }

    pub fn is_favourite(&self, id: &str) -> bool {
        self.favourites.iter().any(|f| f == id)
    }

    pub fn toggle_favourite(&mut self, id: &str) {
        if self.is_favourite(id) {
            self.favourites.retain(|f| f != id);
        } else {
            self.favourites.push(id.to_string());
        }
        self.save();
        self.dirty = true;
    }

    pub fn recents(&self) -> &[String] {
        &self.recents
    }

    pub fn favourites(&self) -> &[String] {
        &self.favourites
    }

    pub fn loaded(&self) -> bool {
        self.items.is_some() || self.failed
    }

    /// Select a category by its label (case-insensitive prefix), for automation.
    pub fn set_cat(&mut self, s: &str) {
        let s = s.to_ascii_lowercase();
        if let Some((c, _)) = CATS
            .iter()
            .find(|(_, l)| l.to_ascii_lowercase().starts_with(&s))
        {
            self.cat = *c;
            self.dirty = true;
        }
    }

    fn save(&self) {
        let s = Saved {
            recents: self.recents.clone(),
            favourites: self.favourites.clone(),
        };
        if let Ok(b) = serde_json::to_vec_pretty(&s) {
            let _ = std::fs::write(&self.settings, b);
        }
    }

    /// Load the archive indexes on a worker thread (shared with the viewer's cache).
    pub fn ensure_loaded(
        &mut self,
        ws: Workspace,
        idx: Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>,
        ctx: &egui::Context,
    ) {
        if let Some(r) = self.pending.lock().unwrap().take() {
            self.loading = false;
            match r {
                Ok(v) => {
                    self.counts.clear();
                    for it in &v {
                        *self.counts.entry(it.cat).or_default() += 1;
                    }
                    self.counts.insert(Cat::All, v.len());
                    self.by_id = v
                        .iter()
                        .enumerate()
                        .map(|(i, it)| (it.id.clone(), i as u32))
                        .collect();
                    self.status = format!("{} models in {}", v.len(), ARCHIVES.join(" + "));
                    self.items = Some(Arc::new(v));
                    self.dirty = true;
                }
                Err(e) => {
                    self.status = format!("model list unavailable: {e}");
                    self.failed = true;
                }
            }
        }
        if self.items.is_some() || self.loading || self.failed {
            return;
        }
        self.loading = true;
        self.status = "reading archive indexes …".into();
        let slot = self.pending.clone();
        let ctx = ctx.clone();
        let work = ws.work.clone();
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            let logp = std::path::Path::new(&work).join("gui.log");
            let log = |m: &str| {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&logp)
                {
                    let _ = writeln!(f, "[model list {:.1}s] {m}", t0.elapsed().as_secs_f32());
                }
            };
            log("start");
            // a panic must not leave the list spinning forever
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let vehicle_re = regex::Regex::new(sixng_core::export::NON_VEHICLE_FT).unwrap();
                let mut v = Vec::new();
                let mut errs = Vec::new();
                let mut outfits: std::collections::BTreeMap<String, u64> =
                    std::collections::BTreeMap::new();
                for a in ARCHIVES {
                    log(&format!("loading index {a}"));
                    match crate::model_view::index_for(&ws, a, &idx) {
                        Ok((_, ix)) => {
                            log(&format!("index {a}: {} entries", ix.entries.len()));
                            for e in ix.entries.iter() {
                                let rtype: &'static str = match e.rtype.as_deref() {
                                    Some("dr" | "dd") => "dr",
                                    Some("ft") => "ft",
                                    _ => continue,
                                };
                                let choices: Vec<(String, Option<&str>)> = if e.members.is_empty() {
                                    vec![(e.id.clone(), e.name.as_deref())]
                                } else {
                                    e.members
                                        .iter()
                                        .map(|name| {
                                            (format!("{}#{name}", e.id), Some(name.as_str()))
                                        })
                                        .collect()
                                };
                                for (id, source_name) in choices {
                                    let name = source_name
                                        .map(str::to_string)
                                        .unwrap_or_else(|| format!("unnamed_{:x}", e.off));
                                    let lname = name.to_ascii_lowercase();
                                    let cat = categorise(source_name, rtype, &vehicle_re);
                                    // animals: only the roots (their parts are put together in the viewer);
                                    // people: pieces of a named character are shown through its outfit
                                    // entry below; generic pieces (masks, costumes, `char_` items) have
                                    // no outfit, so they stay listed unless they are real LOD copies
                                    let variant = match cat {
                                        Cat::Animals => rtype == "dr" || lname.contains("_bounds"),
                                        Cat::People => {
                                            !crate::assemble::is_whole_ped(&lname)
                                                && (crate::assemble::outfit_prefix(&lname)
                                                    .is_some()
                                                    || is_variant(&lname))
                                        }
                                        _ => source_name.is_some() && is_variant(&lname),
                                    };
                                    if let Some(p) = crate::assemble::outfit_prefix(&lname) {
                                        *outfits.entry(p).or_insert(0) += e.usize;
                                    }
                                    v.push(Item {
                                        cat,
                                        variant,
                                        lname,
                                        name,
                                        id,
                                        rtype,
                                        bytes: e.usize,
                                    });
                                }
                            }
                            let arch = a.to_string();
                            for (p, bytes) in std::mem::take(&mut outfits) {
                                v.push(Item {
                                    cat: Cat::People,
                                    variant: false,
                                    lname: p.clone(),
                                    name: p.clone(),
                                    id: format!("{arch}.rpf#outfit:{p}"),
                                    rtype: "outfit",
                                    bytes,
                                });
                            }
                        }
                        Err(e) => errs.push(format!("{a}: {e:#}")),
                    }
                }
                v.sort_by(|a, b| a.lname.cmp(&b.lname));
                // generic ped pieces come as several entries with one name (their LOD
                // copies): keep the largest listed, the others count as LOD copies
                let mut best: std::collections::HashMap<&str, usize> =
                    std::collections::HashMap::new();
                for (i, it) in v.iter().enumerate() {
                    if it.cat == Cat::People && !it.variant && it.rtype != "outfit" {
                        best.entry(it.lname.as_str())
                            .and_modify(|b| {
                                if v[*b].bytes < it.bytes {
                                    *b = i
                                }
                            })
                            .or_insert(i);
                    }
                }
                let copies: Vec<usize> = v
                    .iter()
                    .enumerate()
                    .filter(|(i, it)| {
                        it.cat == Cat::People
                            && !it.variant
                            && it.rtype != "outfit"
                            && best.get(it.lname.as_str()) != Some(i)
                    })
                    .map(|(i, _)| i)
                    .collect();
                for i in copies {
                    v[i].variant = true;
                }
                refine_vehicles(&mut v);
                if v.is_empty() {
                    Err(if errs.is_empty() {
                        "no indexed models (run the setup or index the archives on the Jobs page first)".into()
                    } else {
                        errs.join("; ")
                    })
                } else {
                    Ok(v)
                }
            }));
            let r = r.unwrap_or_else(|p| {
                Err(format!(
                    "model list crashed: {}",
                    p.downcast_ref::<String>()
                        .cloned()
                        .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_default()
                ))
            });
            log(&match &r {
                Ok(v) => format!("done: {} models", v.len()),
                Err(e) => format!("error: {e}"),
            });
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
            ctx.request_repaint();
        });
    }

    fn refilter(&mut self, current: Option<&str>) {
        let Some(items) = &self.items else { return };
        let terms: Vec<String> = self
            .search
            .to_ascii_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect();
        self.filtered = items
            .iter()
            .enumerate()
            .filter(|(_, it)| self.cat == Cat::All || it.cat == self.cat)
            .filter(|(_, it)| {
                terms
                    .iter()
                    .all(|t| it.lname.contains(t.as_str()) || it.id.contains(t.as_str()))
            })
            .filter(|(_, it)| !self.favs_only || self.favourites.contains(&it.id))
            .filter(|(_, it)| !self.hide_variants || !it.variant)
            .map(|(i, _)| i as u32)
            .collect();
        self.cursor = current.and_then(|c| {
            self.filtered
                .iter()
                .position(|&i| items[i as usize].id == c)
        });
        self.dirty = false;
    }

    /// Record a model as opened (recents) and point the cursor at it.
    pub fn opened(&mut self, id: &str) {
        self.recents.retain(|r| r != id);
        self.recents.insert(0, id.to_string());
        self.recents.truncate(20);
        self.save();
        if let Some(items) = &self.items {
            self.cursor = self
                .filtered
                .iter()
                .position(|&i| items[i as usize].id == id);
        }
    }

    fn name_of(&self, id: &str) -> String {
        match (&self.items, self.by_id.get(id)) {
            (Some(items), Some(&i)) => items[i as usize].name.clone(),
            _ => id.to_string(),
        }
    }

    /// Previous / next model in the current list (for ↑/↓ and the toolbar).
    pub fn step(&mut self, d: i64) -> Option<String> {
        let items = self.items.as_ref()?;
        if self.filtered.is_empty() {
            return None;
        }
        let n = self.filtered.len() as i64;
        let c = match self.cursor {
            Some(c) => (c as i64 + d).clamp(0, n - 1),
            None => {
                if d >= 0 {
                    0
                } else {
                    n - 1
                }
            }
        } as usize;
        if Some(c) == self.cursor {
            return None;
        }
        self.cursor = Some(c);
        self.scroll_to_cursor = true;
        Some(items[self.filtered[c] as usize].id.clone())
    }

    /// Draw the picker. Returns an entry id to open.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ws: Workspace,
        idx: Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>,
        current: Option<&str>,
    ) -> Option<String> {
        let ctx = ui.ctx().clone();
        self.ensure_loaded(ws.clone(), idx, &ctx);
        let mut open = None;
        ui.heading("Models");
        if self.items.is_none() {
            ui.horizontal(|ui| {
                if self.loading {
                    ui.spinner();
                }
                ui.label(&self.status);
            });
            if let Some(job) = self.indexing.clone() {
                let j = job.lock().unwrap_or_else(|e| e.into_inner()).clone();
                match j.3 {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(&j.0);
                        });
                        if j.2 > 0 {
                            ui.add(
                                egui::ProgressBar::new(j.1 as f32 / j.2 as f32).show_percentage(),
                            );
                        }
                        ctx.request_repaint_after(std::time::Duration::from_millis(200));
                        return None;
                    }
                    Some(Ok(())) => {
                        self.indexing = None;
                        self.failed = false;
                        return None;
                    }
                    Some(Err(e)) => {
                        self.indexing = None;
                        self.status = format!("Indexing failed: {e}");
                    }
                }
            }
            if self.failed {
                if ui.button("⚙ Index archives").on_hover_text("Index levels.rpf and packs.rpf (whichever are missing; ~3 min for levels), then load the list").clicked() {
                    let job = Arc::new(Mutex::new(("starting …".to_string(), 0, 0, None)));
                    self.indexing = Some(job.clone());
                    let c = ctx.clone();
                    std::thread::spawn(move || {
                        let r = (|| -> Result<(), String> {
                            for want in ARCHIVES {
                                let Some(a) = ws.archives().into_iter().find(|a| a.id.strip_suffix(".rpf").unwrap_or(&a.id) == *want) else {
                                    return Err(format!("{want}.rpf not found in the game folder"));
                                };
                                if ws.is_indexed(&a) {
                                    continue;
                                }
                                let j2 = job.clone();
                                let c2 = c.clone();
                                let id = a.id.clone();
                                let pr: sixng_core::export::Progress = Arc::new(move |stage, d, t| {
                                    *j2.lock().unwrap_or_else(|e| e.into_inner()) = (format!("Indexing {id}.rpf: {stage}"), d, t, None);
                                    c2.request_repaint();
                                });
                                ws.ensure_index(&a, false, true, pr).map_err(|e| e.to_string())?;
                            }
                            Ok(())
                        })();
                        job.lock().unwrap_or_else(|e| e.into_inner()).3 = Some(r);
                        c.request_repaint();
                    });
                }
                ui.label(
                    egui::RichText::new(format!(
                        "Work folder: {}",
                        self.settings
                            .parent()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                    ))
                    .small()
                    .weak(),
                );
                if ui.button("Retry").clicked() {
                    self.failed = false;
                }
            }
            return None;
        }
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("search names / ids")
                    .desired_width(ui.available_width() - 28.0),
            );
            if r.changed() {
                self.dirty = true;
            }
            if ui.small_button("✖").on_hover_text("clear").clicked() {
                self.search.clear();
                self.dirty = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (c, label) in CATS {
                let n = self.counts.get(c).copied().unwrap_or(0);
                if n == 0 && *c != Cat::All {
                    continue;
                }
                if ui
                    .selectable_label(self.cat == *c, format!("{label} {n}"))
                    .clicked()
                {
                    self.cat = *c;
                    self.dirty = true;
                }
            }
        });
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut self.favs_only, "★ favourites only")
                .changed()
            {
                self.dirty = true;
            }
            if ui
                .checkbox(&mut self.hide_variants, "hide LOD copies")
                .on_hover_text("Hide _lod, _slod, batch-LOD and _mid copies")
                .changed()
            {
                self.dirty = true;
            }
        });
        if self.dirty {
            self.refilter(current);
        }
        // recents
        if !self.recents.is_empty() {
            egui::CollapsingHeader::new(format!("Recent ({})", self.recents.len()))
                .id_salt("picker_recent")
                .show(ui, |ui| {
                    for id in self.recents.clone().iter().take(10) {
                        if ui
                            .selectable_label(current == Some(id.as_str()), self.name_of(id))
                            .on_hover_text(id)
                            .clicked()
                        {
                            open = Some(id.clone());
                        }
                    }
                });
        }
        ui.label(
            egui::RichText::new(format!(
                "{} shown · Up / Down arrow keys step through the list",
                self.filtered.len()
            ))
            .small()
            .weak(),
        );
        ui.separator();
        let items = self.items.clone().unwrap();
        let row_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
        let mut toggle_fav: Option<String> = None;
        let scroll_to = if self.scroll_to_cursor {
            self.cursor
        } else {
            None
        };
        self.scroll_to_cursor = false;
        let mut sa = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if let Some(c) = scroll_to {
            sa = sa.vertical_scroll_offset((c as f32 * row_h - 120.0).max(0.0));
        }
        sa.show_rows(ui, row_h, self.filtered.len(), |ui, range| {
            for r in range {
                let it = &items[self.filtered[r] as usize];
                ui.horizontal(|ui| {
                    let fav = self.favourites.contains(&it.id);
                    if ui
                        .add(
                            egui::Button::new(if fav { "★" } else { "☆" })
                                .small()
                                .frame(false),
                        )
                        .on_hover_text("favourite")
                        .clicked()
                    {
                        toggle_fav = Some(it.id.clone());
                    }
                    let sel = current == Some(it.id.as_str()) || self.cursor == Some(r);
                    let tag = if it.rtype == "ft" { "frag" } else { "draw" };
                    let resp = ui.selectable_label(sel, &it.name).on_hover_text(format!(
                        "{}\n{} · {:.1} MB",
                        it.id,
                        tag,
                        it.bytes as f64 / 1e6
                    ));
                    if resp.clicked() {
                        self.cursor = Some(r);
                        open = Some(it.id.clone());
                    }
                });
            }
        });
        if let Some(id) = toggle_fav {
            if let Some(p) = self.favourites.iter().position(|f| *f == id) {
                self.favourites.remove(p);
            } else {
                self.favourites.push(id);
            }
            self.save();
            if self.favs_only {
                self.dirty = true;
            }
        }
        open
    }
}

/// Split fragment "vehicles" into vehicles and their parts. A vehicle is a
/// fragment with a `<name>_hi` sibling whose name is one word or one word
/// plus a short suffix (`sandking_4dr`); everything else in that group is a
/// mod / damage part (bonnets, aerials, cages ...). About 240 vehicles.
fn refine_vehicles(v: &mut [Item]) {
    use std::collections::HashSet;
    let names: HashSet<String> = v
        .iter()
        .filter(|i| i.cat == Cat::Vehicles)
        .map(|i| i.lname.clone())
        .collect();
    let shape_ok = |n: &str| {
        let parts: Vec<&str> = n.split('_').collect();
        parts.len() == 1
            || (parts.len() == 2
                && parts[1].len() <= 4
                && !["roof", "mast", "horn", "mod"].contains(&parts[1]))
    };
    let vehicles: HashSet<String> = names
        .iter()
        .filter(|n| !n.ends_with("_hi") && names.contains(&format!("{n}_hi")) && shape_ok(n))
        .cloned()
        .collect();
    for it in v.iter_mut().filter(|i| i.cat == Cat::Vehicles) {
        let stem = it
            .lname
            .strip_suffix("_hi")
            .or_else(|| it.lname.strip_suffix("_mid"))
            .unwrap_or(&it.lname);
        if !vehicles.contains(stem) {
            it.cat = Cat::VehicleParts;
        }
    }
}
