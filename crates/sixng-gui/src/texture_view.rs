//! Texture viewer: every texture in the game's texture stores (`data/ttdb/*.ttpf`),
//! filterable by store, format, size and hash, with a decoded preview of any mip,
//! single-channel views and PNG export. Textures have no recovered names; they are
//! identified by their 64-bit hash (the same `hash64` that model.json lists per
//! material slot, so the model viewer can jump here). The "Used by" panel lists
//! the materials that use the selected texture and their other slots (from the
//! `sixng texture-users` index), so a base colour leads to its normal map and
//! the rest of its material.

use eframe::egui;
use sixng_core::ttpf::{self, TtpfSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What the wardrobe data says about textures no model binds (ped skin):
/// the table, and names for its heads and wardrobe resources.
pub struct WardrobeUsers {
    table: &'static sixng_core::pedlook::WardrobeTextures,
    /// joaat(head drawable name) -> (entry id, name)
    heads: std::collections::HashMap<u32, (String, String)>,
    /// wardrobe resource -> named characters that use it
    characters: std::collections::HashMap<u64, Vec<String>>,
}
type WardrobeSlot = Arc<Mutex<Option<Result<Arc<WardrobeUsers>, String>>>>;

type UsersSlot = Arc<Mutex<Option<Result<Arc<sixng_core::texusers::TexUsers>, String>>>>;

const CHANNELS: [&str; 6] = ["RGBA", "RGB", "R", "G", "B", "A"];

struct Decoded {
    hash: u64,
    mip: u32,
    rgba: Vec<u8>,
    w: u32,
    h: u32,
    undecoded: u64,
}

pub struct TextureView {
    game: String,
    work: String,
    set: Option<&'static TtpfSet>,
    loading: Arc<Mutex<Option<&'static TtpfSet>>>,
    load_started: bool,
    /// (store, texture) indices passing the filter
    rows: Vec<(usize, usize)>,
    dirty: bool,
    store_sel: Option<usize>,
    format_sel: Option<u32>,
    min_size: u32,
    search: String,
    sel: Option<(usize, usize)>,
    mip: u32,
    channel: usize,
    zoom: f32,
    tex: Option<(u64, u32, usize, egui::TextureHandle)>,
    decoded: Option<Decoded>,
    /// (decode generation, result): results of superseded decodes are dropped
    pending: Arc<Mutex<Option<(u64, Result<Decoded, String>)>>>,
    decode_gen: u64,
    decoding: bool,
    /// scroll-wheel travel not yet turned into a texture step
    wheel: f32,
    error: Option<String>,
    status: Option<String>,
    formats: Vec<u32>,
    scroll_to_sel: bool,
    pending_hash: Option<u64>,
    last_export: Option<PathBuf>,
    users: Option<Result<Arc<sixng_core::texusers::TexUsers>, String>>,
    users_slot: UsersSlot,
    users_started: bool,
    wardrobe: Option<Result<Arc<WardrobeUsers>, String>>,
    wardrobe_slot: WardrobeSlot,
    wardrobe_started: bool,
    /// Set when "Used by" asks to open a model in the Model viewer.
    pub open_model: Option<String>,
    /// Set when "Used by" asks to build its index (a `texture-users` job).
    pub want_users_job: bool,
}

impl TextureView {
    pub fn new(game: &str, work: &str) -> Self {
        TextureView {
            game: game.into(),
            work: work.into(),
            set: None,
            loading: Arc::new(Mutex::new(None)),
            load_started: false,
            rows: Vec::new(),
            dirty: true,
            store_sel: None,
            format_sel: None,
            min_size: 0,
            search: String::new(),
            sel: None,
            mip: 0,
            channel: 0,
            zoom: 0.0,
            tex: None,
            decoded: None,
            pending: Arc::new(Mutex::new(None)),
            decode_gen: 0,
            decoding: false,
            wheel: 0.0,
            error: None,
            status: None,
            formats: Vec::new(),
            scroll_to_sel: false,
            pending_hash: None,
            last_export: None,
            users: None,
            users_slot: Arc::new(Mutex::new(None)),
            users_started: false,
            wardrobe: None,
            wardrobe_slot: Arc::new(Mutex::new(None)),
            wardrobe_started: false,
            open_model: None,
            want_users_job: false,
        }
    }

    /// Read the texture-users index again (after a `texture-users` job).
    pub fn reload_users(&mut self) {
        self.users = None;
        self.users_started = false;
    }

    fn ensure_users(&mut self) {
        if self.users.is_some() {
            return;
        }
        if let Some(r) = self.users_slot.lock().unwrap().take() {
            self.users = Some(r);
            return;
        }
        if !self.users_started {
            self.users_started = true;
            let ws = sixng_core::export::Workspace::new(&self.game, &self.work);
            let p = sixng_core::texusers::path(&ws);
            let slot = self.users_slot.clone();
            std::thread::spawn(move || {
                let r = if p.exists() {
                    sixng_core::texusers::TexUsers::load(&p).map(Arc::new).map_err(|e| format!("{e:#}"))
                } else {
                    Err("missing".into())
                };
                *slot.lock().unwrap() = Some(r);
            });
        }
    }

    /// The wardrobe's texture links, read on a worker thread the first time a
    /// texture has no material that uses it.
    fn ensure_wardrobe(&mut self) {
        if self.wardrobe.is_some() {
            return;
        }
        if let Some(r) = self.wardrobe_slot.lock().unwrap().take() {
            self.wardrobe = Some(r);
            return;
        }
        if !self.wardrobe_started {
            self.wardrobe_started = true;
            let (game, work, slot) = (std::path::PathBuf::from(&self.game), std::path::PathBuf::from(&self.work), self.wardrobe_slot.clone());
            std::thread::spawn(move || {
                let r = (|| -> Result<Arc<WardrobeUsers>, String> {
                    let table = sixng_core::pedlook::wardrobe_textures(&game)?;
                    let ix = sixng_core::scan::load_index(&sixng_core::scan::index_path(&work.join("index"), "packs.rpf")).map_err(|e| format!("{e:#}"))?;
                    let joaat = sixng_core::meta7a::joaat;
                    let mut heads = std::collections::HashMap::new();
                    for (n, e) in sixng_core::outfit::dedupe(ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| {
                        let n = e.name.as_ref()?.to_ascii_lowercase();
                        n.contains("head_").then_some((n, e))
                    })) {
                        heads.insert(joaat(&n), (e.id.clone(), n));
                    }
                    let mut characters: std::collections::HashMap<u64, Vec<String>> = Default::default();
                    if let Ok(models) = sixng_core::pedlook::models(&game) {
                        for c in sixng_core::outfit::characters(&ix).into_iter().filter(|c| c.kind == "character") {
                            if let Some(row) = models.by_model.get(&joaat(&c.name)) {
                                characters.entry(row.resource).or_default().push(c.name);
                            }
                        }
                    }
                    Ok(Arc::new(WardrobeUsers { table, heads, characters }))
                })();
                *slot.lock().unwrap() = Some(r);
            });
        }
    }

    /// For a texture no material uses: what the wardrobe data names it for
    /// (heads to open, characters to open, and how many other wardrobes).
    fn wardrobe_users_ui(&mut self, ui: &mut egui::Ui, hash: u64) {
        self.ensure_wardrobe();
        let w = match &self.wardrobe {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.weak("reading the wardrobe data (about a minute the first time)");
                });
                ui.ctx().request_repaint();
                return;
            }
            Some(Err(e)) => {
                ui.weak(format!("No material in the indexed models uses this texture. (Wardrobe data not read: {e})"));
                return;
            }
            Some(Ok(w)) => w.clone(),
        };
        let Some(rows) = w.table.by_texture.get(&hash) else {
            ui.weak("No material in the indexed models uses this texture, and no wardrobe item names it.");
            return;
        };
        ui.label("No model binds this texture. Ped skin (faces, body, eyes) gets its textures from the wardrobe data, which names this one for:");
        let mut heads: Vec<&(String, String)> = rows.iter().filter_map(|r| w.heads.get(&r.1)).collect();
        heads.sort();
        heads.dedup();
        let mut chars: Vec<&String> = rows.iter().flat_map(|r| w.characters.get(&r.0).into_iter().flatten()).collect();
        chars.sort();
        chars.dedup();
        let mut res: Vec<u64> = rows.iter().map(|r| r.0).collect();
        res.sort();
        res.dedup();
        let unnamed = res.iter().filter(|r| !w.characters.contains_key(r)).count();
        let mut open = None;
        egui::ScrollArea::vertical().id_salt("tv_wardrobe_scroll").auto_shrink([false; 2]).show(ui, |ui| {
            if !heads.is_empty() {
                ui.strong(format!("{} head{}", heads.len(), if heads.len() == 1 { "" } else { "s" }));
                for (id, name) in heads.iter().take(40) {
                    if ui.link(egui::RichText::new(name.as_str()).small()).on_hover_text(format!("open {id} in the Model viewer")).clicked() {
                        open = Some(id.clone());
                    }
                }
            }
            if !chars.is_empty() {
                ui.strong(format!("{} character{}", chars.len(), if chars.len() == 1 { "" } else { "s" }));
                for c in chars.iter().take(40) {
                    if ui.link(egui::RichText::new(c.as_str()).small()).on_hover_text("open this character in the Model viewer").clicked() {
                        open = Some(format!("packs.rpf#outfit:{c}"));
                    }
                }
                if chars.len() > 40 {
                    ui.weak(format!("+ {} more", chars.len() - 40));
                }
            }
            if unnamed > 0 {
                ui.weak(format!("{}{unnamed} wardrobe{} of models without a name in the index", if heads.is_empty() && chars.is_empty() { "" } else { "+ " }, if unnamed == 1 { "" } else { "s" }));
            }
        });
        if open.is_some() {
            self.open_model = open;
        }
    }

    /// "Used by": the materials that use `hash`, each with its slots
    /// (click one to view that texture) and some of its models (click to
    /// open in the Model viewer).
    fn users_ui(&mut self, ui: &mut egui::Ui, hash: u64) {
        self.ensure_users();
        ui.strong("Used by");
        let users = match &self.users {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.weak("reading the texture-users index");
                });
                ui.ctx().request_repaint();
                return;
            }
            Some(Err(e)) if e == "missing" => {
                ui.label("Which materials use a texture, and their other maps (normal, roughness/metal …), comes from an index of every model's materials. It isn't built yet.");
                if ui
                    .button("Build index")
                    .on_hover_text("Runs `sixng texture-users` as a job (a few minutes; needs the archives indexed)")
                    .clicked()
                {
                    self.want_users_job = true;
                }
                return;
            }
            Some(Err(e)) => {
                ui.colored_label(egui::Color32::from_rgb(230, 120, 90), e);
                return;
            }
            Some(Ok(u)) => u.clone(),
        };
        let mats = users.users(hash);
        if mats.is_empty() {
            self.wardrobe_users_ui(ui, hash);
            return;
        }
        let models: std::collections::HashSet<u32> = mats
            .iter()
            .flat_map(|&m| users.material_models[m as usize].iter().copied())
            .collect();
        let s = |n: usize| if n == 1 { "" } else { "s" };
        ui.label(format!("{} material{} in {} model{}", mats.len(), s(mats.len()), models.len(), s(models.len())));
        let mut view = None;
        let mut open = None;
        egui::ScrollArea::vertical()
            .id_salt("tv_users_scroll")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for (k, &m) in mats.iter().take(60).enumerate() {
                    let mat = &users.materials[m as usize];
                    let ms = &users.material_models[m as usize];
                    egui::CollapsingHeader::new(format!("{} · {} model{}", mat.shader, ms.len(), s(ms.len())))
                        .id_salt(("tv_mat", m))
                        .default_open(k < 3)
                        .show(ui, |ui| {
                            egui::Grid::new(("tv_slots", m))
                                .num_columns(2)
                                .spacing([8.0, 2.0])
                                .show(ui, |ui| {
                                    for (name, h) in &mat.slots {
                                        ui.label(egui::RichText::new(name).small());
                                        let txt = egui::RichText::new(format!("{h:016x}")).monospace().small();
                                        if *h == hash {
                                            ui.label(txt.strong());
                                        } else if ui.link(txt).on_hover_text("view this texture").clicked() {
                                            view = Some(*h);
                                        }
                                        ui.end_row();
                                    }
                                });
                            for &i in ms.iter().take(5) {
                                let (id, name) = &users.models[i as usize];
                                let label = if name.is_empty() { id.clone() } else { name.clone() };
                                if ui
                                    .link(egui::RichText::new(label).small())
                                    .on_hover_text(format!("open {id} in the Model viewer"))
                                    .clicked()
                                {
                                    open = Some(id.clone());
                                }
                            }
                            if ms.len() > 5 {
                                ui.weak(format!("+ {} more", ms.len() - 5));
                            }
                        });
                }
                if mats.len() > 60 {
                    ui.weak(format!("+ {} more materials", mats.len() - 60));
                }
            });
        if let Some(h) = view {
            self.show_hash(h);
        }
        if open.is_some() {
            self.open_model = open;
        }
    }

    /// Ready for a scripted screenshot: stores listed and the selection (if any) shown.
    pub fn idle(&self) -> bool {
        self.set.is_some()
            && !self.decoding
            && (self.sel.is_none() || self.tex.is_some() || self.error.is_some())
            && (self.sel.is_none() || self.users.is_some())
            // the wardrobe links, once asked for, have arrived
            && (!self.wardrobe_started || self.wardrobe.is_some())
    }

    fn ensure_loading(&mut self) {
        if self.set.is_none() {
            if let Some(s) = self.loading.lock().unwrap().take() {
                self.set = Some(s);
                let mut f: Vec<u32> = s
                    .stores
                    .iter()
                    .flat_map(|st| st.textures.iter().map(|t| t.dxgi_format))
                    .collect();
                f.sort();
                f.dedup();
                self.formats = f;
                self.dirty = true;
            } else if !self.load_started {
                self.load_started = true;
                let game = self.game.clone();
                let slot = self.loading.clone();
                std::thread::spawn(move || {
                    let s = ttpf::global(Path::new(&game));
                    *slot.lock().unwrap() = Some(s);
                });
            }
        }
    }

    /// Select a texture by hash (e.g. from a model's material slot).
    pub fn show_hash(&mut self, hash: u64) {
        self.search = format!("{hash:016x}");
        self.store_sel = None;
        self.format_sel = None;
        self.min_size = 0;
        self.dirty = true;
        self.pending_hash = Some(hash);
    }

    fn refilter(&mut self) {
        let Some(set) = self.set else { return };
        let q = self
            .search
            .trim()
            .trim_start_matches("0x")
            .to_ascii_lowercase();
        self.rows.clear();
        for (si, st) in set.stores.iter().enumerate() {
            if self.store_sel.is_some_and(|s| s != si) {
                continue;
            }
            for (ti, t) in st.textures.iter().enumerate() {
                if self.format_sel.is_some_and(|f| f != t.dxgi_format)
                    || t.width.max(t.height) < self.min_size
                {
                    continue;
                }
                if !q.is_empty() && !format!("{:016x}", t.hash).contains(&q) {
                    continue;
                }
                self.rows.push((si, ti));
            }
        }
        self.dirty = false;
        if let Some(h) = self.pending_hash.take() {
            if let Some(&r) = self
                .rows
                .iter()
                .find(|&&(s, t)| set.stores[s].textures[t].hash == h)
            {
                self.select(r);
                self.scroll_to_sel = true;
            } else {
                self.error = Some(format!("texture {h:016x} is not in any texture store"));
            }
        }
    }

    fn select(&mut self, r: (usize, usize)) {
        let Some(set) = self.set else { return };
        self.sel = Some(r);
        let t = &set.stores[r.0].textures[r.1];
        // largest mip outside undecodable BCPack chunks (all formats but BC1), capped to 2048 px for the preview
        let mut m = set.stores[r.0].best_mip(t).unwrap_or(0);
        while (t.width >> m).max(t.height >> m) > 2048 && m + 1 < t.mips() {
            m += 1;
        }
        self.mip = m;
        self.zoom = 0.0;
        self.error = None;
        self.status = None;
        self.decode();
    }

    /// Select the texture `d` rows below (above when negative) the current one.
    fn step(&mut self, d: i64) {
        if self.rows.is_empty() {
            return;
        }
        let i = self
            .sel
            .and_then(|s| self.rows.iter().position(|&r| r == s))
            .map_or(0, |i| {
                (i as i64 + d).clamp(0, self.rows.len() as i64 - 1) as usize
            });
        if Some(self.rows[i]) != self.sel {
            self.select(self.rows[i]);
            self.scroll_to_sel = true;
        }
    }

    fn decode(&mut self) {
        let (Some(set), Some(r)) = (self.set, self.sel) else {
            return;
        };
        let (si, ti, mip) = (r.0, r.1, self.mip);
        let slot = self.pending.clone();
        self.decode_gen += 1;
        let gen = self.decode_gen;
        self.decoding = true;
        std::thread::spawn(move || {
            let st = &set.stores[si];
            let t = &st.textures[ti];
            let res = st
                .data_ex(t)
                .map_err(|e| format!("{e:#}"))
                .and_then(|(data, undecoded)| {
                    ttpf::decode_rgba(t, &data, mip)
                        .map(|(rgba, w, h)| Decoded {
                            hash: t.hash,
                            mip,
                            rgba,
                            w,
                            h,
                            undecoded,
                        })
                        .map_err(|e| format!("{e:#}"))
                });
            *slot.lock().unwrap() = Some((gen, res));
        });
    }

    fn channel_image(d: &Decoded, ch: usize) -> egui::ColorImage {
        let px: Vec<egui::Color32> = d
            .rgba
            .chunks_exact(4)
            .map(|p| match ch {
                0 => egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]),
                1 => egui::Color32::from_rgb(p[0], p[1], p[2]),
                2..=5 => {
                    let v = p[ch - 2];
                    egui::Color32::from_rgb(v, v, v)
                }
                _ => egui::Color32::BLACK,
            })
            .collect();
        egui::ColorImage {
            size: [d.w as usize, d.h as usize],
            source_size: egui::vec2(d.w as f32, d.h as f32),
            pixels: px,
        }
    }

    fn export_png(&mut self) {
        let Some(d) = &self.decoded else { return };
        let dir = Path::new(&self.work).join("out").join("textures");
        let p: PathBuf = dir.join(format!("{:016x}_m{}.png", d.hash, d.mip));
        let r = std::fs::create_dir_all(&dir)
            .map_err(anyhow::Error::from)
            .and_then(|_| sixng_core::texture::save_png(&p, &d.rgba, d.w, d.h));
        self.status = Some(match r {
            Ok(()) => format!("Saved {}", p.display()),
            Err(e) => format!("Export failed: {e:#}"),
        });
        self.last_export = r_ok_path(&self.status, p);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.ensure_loading();
        let done = self
            .pending
            .lock()
            .unwrap()
            .take()
            .filter(|(g, _)| *g == self.decode_gen);
        if let Some((_, res)) = done {
            self.decoding = false;
            match res {
                Ok(d) => {
                    self.decoded = Some(d);
                    self.tex = None;
                }
                Err(e) => {
                    self.error = Some(e);
                    self.decoded = None;
                    self.tex = None;
                }
            }
        }
        let Some(set) = self.set else {
            ui.centered_and_justified(|ui| {
                ui.spinner().on_hover_text("reading texture store indexes")
            });
            ui.ctx().request_repaint();
            return;
        };
        if self.dirty {
            self.refilter();
        }
        if self.decoding {
            ui.ctx().request_repaint();
        }
        // filters
        ui.horizontal_wrapped(|ui| {
            let store_name = |i: usize| set.stores[i].label();
            let cur = self
                .store_sel
                .map(store_name)
                .unwrap_or_else(|| format!("all stores ({})", set.stores.len()));
            egui::ComboBox::from_id_salt("tv_store")
                .selected_text(cur)
                .width(220.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.store_sel.is_none(), "all stores")
                        .clicked()
                    {
                        self.store_sel = None;
                        self.dirty = true;
                    }
                    for i in 0..set.stores.len() {
                        if ui
                            .selectable_label(
                                self.store_sel == Some(i),
                                format!("{} ({})", store_name(i), set.stores[i].textures.len()),
                            )
                            .clicked()
                        {
                            self.store_sel = Some(i);
                            self.dirty = true;
                        }
                    }
                });
            let fname = |f: u32| sixng_core::texture::dxgi_name(f);
            egui::ComboBox::from_id_salt("tv_fmt")
                .selected_text(
                    self.format_sel
                        .map(fname)
                        .unwrap_or_else(|| "any format".into()),
                )
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.format_sel.is_none(), "any format")
                        .clicked()
                    {
                        self.format_sel = None;
                        self.dirty = true;
                    }
                    for &f in &self.formats.clone() {
                        if ui
                            .selectable_label(self.format_sel == Some(f), fname(f))
                            .clicked()
                        {
                            self.format_sel = Some(f);
                            self.dirty = true;
                        }
                    }
                });
            egui::ComboBox::from_id_salt("tv_min")
                .selected_text(if self.min_size == 0 {
                    "any size".to_string()
                } else {
                    format!("≥ {} px", self.min_size)
                })
                .show_ui(ui, |ui| {
                    for v in [0u32, 64, 256, 512, 1024, 2048] {
                        if ui
                            .selectable_value(
                                &mut self.min_size,
                                v,
                                if v == 0 {
                                    "any size".to_string()
                                } else {
                                    format!("≥ {v} px")
                                },
                            )
                            .changed()
                        {
                            self.dirty = true;
                        }
                    }
                });
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("hash (hex)")
                        .desired_width(170.0),
                )
                .changed()
            {
                self.dirty = true;
            }
            ui.label(format!("{} textures", self.rows.len()));
        });
        ui.separator();
        // Up/Down (and Page Up/Down for 10) step through the list while no text field has focus.
        if ui.ctx().memory(|m| m.focused().is_none()) {
            let d = ui.input(|i| {
                i.key_pressed(egui::Key::ArrowDown) as i64
                    - i.key_pressed(egui::Key::ArrowUp) as i64
                    + 10 * (i.key_pressed(egui::Key::PageDown) as i64
                        - i.key_pressed(egui::Key::PageUp) as i64)
            });
            if d != 0 {
                self.step(d);
            }
        }
        egui::Panel::left("tv_list")
            .resizable(true)
            .default_size(330.0)
            .size_range(240.0..=560.0)
            .show(ui, |ui| {
                let h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
                let mut sa = egui::ScrollArea::vertical().auto_shrink([false; 2]);
                if self.scroll_to_sel {
                    if let Some(i) = self
                        .sel
                        .and_then(|s| self.rows.iter().position(|&r| r == s))
                    {
                        sa = sa.vertical_scroll_offset(
                            (i as f32 * (h + ui.spacing().item_spacing.y) - 100.0).max(0.0),
                        );
                    }
                    self.scroll_to_sel = false;
                }
                let mut clicked = None;
                sa.show_rows(ui, h, self.rows.len(), |ui, range| {
                    for i in range {
                        let r = self.rows[i];
                        let t = &set.stores[r.0].textures[r.1];
                        let txt = format!(
                            "{:016x}  {}×{}  {}",
                            t.hash,
                            t.width,
                            t.height,
                            sixng_core::texture::dxgi_name(t.dxgi_format)
                                .trim_start_matches("DXGI_FORMAT_")
                        );
                        if ui
                            .selectable_label(
                                self.sel == Some(r),
                                egui::RichText::new(txt).monospace().small(),
                            )
                            .clicked()
                        {
                            clicked = Some(r);
                        }
                    }
                });
                if let Some(r) = clicked {
                    self.select(r);
                }
            });
        if let Some(r) = self.sel {
            let h = set.stores[r.0].textures[r.1].hash;
            egui::Panel::right("tv_users")
                .resizable(true)
                .default_size(330.0)
                .size_range(220.0..=600.0)
                .show(ui, |ui| self.users_ui(ui, h));
        }
        // Over the preview at fit zoom the wheel steps through the list (one
        // texture per notch, also past textures that cannot decode); zoomed in, it pans.
        if self.zoom <= 0.0 && ui.rect_contains_pointer(ui.available_rect_before_wrap()) {
            self.wheel += ui.input(|i| i.smooth_scroll_delta.y);
            let notch = 40.0;
            if self.wheel.abs() >= notch {
                let d = -(self.wheel / notch).trunc();
                self.wheel -= -d * notch;
                self.step(d as i64);
            }
        } else {
            self.wheel = 0.0;
        }
        // preview
        let Some(r) = self.sel else {
            ui.centered_and_justified(|ui| ui.label("Pick a texture on the left, or use \"view\" on a material slot in the Model viewer."));
            return;
        };
        let st = &set.stores[r.0];
        let t = &st.textures[r.1];
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("{:016x}", t.hash));
            ui.label(format!(
                "{}×{} · {} · {} mips · tile mode {} · {}",
                t.width,
                t.height,
                sixng_core::texture::dxgi_name(t.dxgi_format),
                t.mips(),
                t.tile_mode(),
                st.label()
            ));
        });
        ui.horizontal_wrapped(|ui| {
            let mut m = self.mip;
            egui::ComboBox::from_id_salt("tv_mip")
                .selected_text(format!(
                    "mip {} ({}×{})",
                    m,
                    (t.width >> m).max(1),
                    (t.height >> m).max(1)
                ))
                .show_ui(ui, |ui| {
                    for k in 0..t.mips() {
                        ui.selectable_value(
                            &mut m,
                            k,
                            format!(
                                "mip {} ({}×{})",
                                k,
                                (t.width >> k).max(1),
                                (t.height >> k).max(1)
                            ),
                        );
                    }
                });
            if m != self.mip {
                self.mip = m;
                self.decode();
            }
            for (k, c) in CHANNELS.iter().enumerate() {
                if ui.selectable_label(self.channel == k, *c).clicked() && self.channel != k {
                    self.channel = k;
                    self.tex = None;
                }
            }
            ui.add(egui::Slider::new(&mut self.zoom, 0.0..=8.0).text("zoom (0 = fit)"));
            if ui
                .add_enabled(self.decoded.is_some(), egui::Button::new("Export PNG"))
                .on_hover_text("Save this mip to <work>/out/textures/")
                .clicked()
            {
                self.export_png();
            }
            if let Some(p) = &self.last_export {
                if ui
                    .button(format!("Show in {}", crate::FILE_MANAGER))
                    .clicked()
                {
                    crate::reveal(p);
                }
            }
        });
        if let Some(s) = &self.status {
            ui.label(egui::RichText::new(s).small());
        }
        if let Some(e) = &self.error {
            ui.colored_label(
                egui::Color32::from_rgb(230, 120, 90),
                format!("Cannot decode: {e}"),
            );
            return;
        }
        if self.decoding {
            ui.spinner();
            return;
        }
        let Some(d) = &self.decoded else { return };
        if d.undecoded > 0 {
            ui.colored_label(egui::Color32::from_rgb(220, 180, 80), format!("{} KiB of this texture is in BCPack chunks that did not decode; lower mips may be clean.", d.undecoded / 1024));
        }
        let need = (d.hash, d.mip, self.channel);
        if self.tex.as_ref().map(|x| (x.0, x.1, x.2)) != Some(need) {
            let img = Self::channel_image(d, self.channel);
            let h = ui.ctx().load_texture(
                format!("tv_{:016x}_{}_{}", d.hash, d.mip, self.channel),
                img,
                egui::TextureOptions::NEAREST,
            );
            self.tex = Some((need.0, need.1, need.2, h));
        }
        let (dw, dh) = (d.w as f32, d.h as f32);
        let tex = self.tex.as_ref().unwrap().3.clone();
        egui::ScrollArea::both()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let avail = ui.available_size();
                let z = if self.zoom <= 0.0 {
                    (avail.x / dw).min(avail.y / dh).min(8.0).max(0.01)
                } else {
                    self.zoom
                };
                let size = egui::vec2(dw * z, dh * z);
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                // checkerboard behind transparent texels
                if self.channel == 0 {
                    let p = ui.painter();
                    let c = 16.0;
                    let (nx, ny) = ((size.x / c).ceil() as i32, (size.y / c).ceil() as i32);
                    if nx * ny < 40_000 {
                        for y in 0..ny {
                            for x in 0..nx {
                                let col = if (x + y) % 2 == 0 {
                                    egui::Color32::from_gray(90)
                                } else {
                                    egui::Color32::from_gray(60)
                                };
                                let r = egui::Rect::from_min_size(
                                    rect.min + egui::vec2(x as f32 * c, y as f32 * c),
                                    egui::vec2(c, c),
                                )
                                .intersect(rect);
                                p.rect_filled(r, 0.0, col);
                            }
                        }
                    }
                }
                ui.painter().image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            });
    }
}

fn r_ok_path(status: &Option<String>, p: PathBuf) -> Option<PathBuf> {
    status
        .as_ref()
        .filter(|s| s.starts_with("Saved"))
        .map(|_| p)
}
