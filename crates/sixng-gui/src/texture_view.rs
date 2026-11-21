//! Texture viewer: every texture in the game's texture stores (`data/ttdb/*.ttpf`),
//! filterable by store, format, size and hash, with a decoded preview of any mip,
//! single-channel views and PNG export. Textures have no recovered names; they are
//! identified by their 64-bit hash (the same `hash64` that model.json lists per
//! material slot, so the model viewer can jump here).

use eframe::egui;
use sixng_core::ttpf::{self, TtpfSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
    pending: Arc<Mutex<Option<Result<Decoded, String>>>>,
    decoding: bool,
    error: Option<String>,
    status: Option<String>,
    formats: Vec<u32>,
    scroll_to_sel: bool,
    pending_hash: Option<u64>,
    last_export: Option<PathBuf>,
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
            decoding: false,
            error: None,
            status: None,
            formats: Vec::new(),
            scroll_to_sel: false,
            pending_hash: None,
            last_export: None,
        }
    }

    /// Ready for a scripted screenshot: stores listed and the selection (if any) shown.
    pub fn idle(&self) -> bool {
        self.set.is_some() && !self.decoding && (self.sel.is_none() || self.tex.is_some() || self.error.is_some())
    }

    fn ensure_loading(&mut self) {
        if self.set.is_none() {
            if let Some(s) = self.loading.lock().unwrap().take() {
                self.set = Some(s);
                let mut f: Vec<u32> = s.stores.iter().flat_map(|st| st.textures.iter().map(|t| t.dxgi_format)).collect();
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
        let q = self.search.trim().trim_start_matches("0x").to_ascii_lowercase();
        self.rows.clear();
        for (si, st) in set.stores.iter().enumerate() {
            if self.store_sel.is_some_and(|s| s != si) {
                continue;
            }
            for (ti, t) in st.textures.iter().enumerate() {
                if self.format_sel.is_some_and(|f| f != t.dxgi_format) || t.width.max(t.height) < self.min_size {
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
            if let Some(&r) = self.rows.iter().find(|&&(s, t)| set.stores[s].textures[t].hash == h) {
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
        // largest mip that is fully stored (BCPack-compressed mips cannot be decoded), capped to 2048 px for the preview
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

    fn decode(&mut self) {
        let (Some(set), Some(r)) = (self.set, self.sel) else { return };
        let (si, ti, mip) = (r.0, r.1, self.mip);
        let slot = self.pending.clone();
        self.decoding = true;
        std::thread::spawn(move || {
            let st = &set.stores[si];
            let t = &st.textures[ti];
            let res = st.data_ex(t).map_err(|e| format!("{e:#}")).and_then(|(data, undecoded)| {
                ttpf::decode_rgba(t, &data, mip).map(|(rgba, w, h)| Decoded { hash: t.hash, mip, rgba, w, h, undecoded }).map_err(|e| format!("{e:#}"))
            });
            *slot.lock().unwrap() = Some(res);
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
        egui::ColorImage { size: [d.w as usize, d.h as usize], pixels: px }
    }

    fn export_png(&mut self) {
        let Some(d) = &self.decoded else { return };
        let dir = Path::new(&self.work).join("out").join("textures");
        let p: PathBuf = dir.join(format!("{:016x}_m{}.png", d.hash, d.mip));
        let r = std::fs::create_dir_all(&dir).map_err(anyhow::Error::from).and_then(|_| sixng_core::texture::save_png(&p, &d.rgba, d.w, d.h));
        self.status = Some(match r {
            Ok(()) => format!("Saved {}", p.display()),
            Err(e) => format!("Export failed: {e:#}"),
        });
        self.last_export = r_ok_path(&self.status, p);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.ensure_loading();
        if let Some(res) = self.pending.lock().unwrap().take() {
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
            ui.centered_and_justified(|ui| ui.spinner().on_hover_text("reading texture store indexes"));
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
            let store_name = |i: usize| set.stores[i].path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            let cur = self.store_sel.map(store_name).unwrap_or_else(|| format!("all stores ({})", set.stores.len()));
            egui::ComboBox::from_id_salt("tv_store").selected_text(cur).width(220.0).show_ui(ui, |ui| {
                if ui.selectable_label(self.store_sel.is_none(), "all stores").clicked() {
                    self.store_sel = None;
                    self.dirty = true;
                }
                for i in 0..set.stores.len() {
                    if ui.selectable_label(self.store_sel == Some(i), format!("{} ({})", store_name(i), set.stores[i].textures.len())).clicked() {
                        self.store_sel = Some(i);
                        self.dirty = true;
                    }
                }
            });
            let fname = |f: u32| sixng_core::texture::dxgi_name(f);
            egui::ComboBox::from_id_salt("tv_fmt").selected_text(self.format_sel.map(fname).unwrap_or_else(|| "any format".into())).show_ui(ui, |ui| {
                if ui.selectable_label(self.format_sel.is_none(), "any format").clicked() {
                    self.format_sel = None;
                    self.dirty = true;
                }
                for &f in &self.formats.clone() {
                    if ui.selectable_label(self.format_sel == Some(f), fname(f)).clicked() {
                        self.format_sel = Some(f);
                        self.dirty = true;
                    }
                }
            });
            egui::ComboBox::from_id_salt("tv_min").selected_text(if self.min_size == 0 { "any size".to_string() } else { format!("≥ {} px", self.min_size) }).show_ui(ui, |ui| {
                for v in [0u32, 64, 256, 512, 1024, 2048] {
                    if ui.selectable_value(&mut self.min_size, v, if v == 0 { "any size".to_string() } else { format!("≥ {v} px") }).changed() {
                        self.dirty = true;
                    }
                }
            });
            if ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("hash (hex)").desired_width(170.0)).changed() {
                self.dirty = true;
            }
            ui.label(format!("{} textures", self.rows.len()));
        });
        ui.separator();
        egui::SidePanel::left("tv_list").resizable(true).default_width(330.0).width_range(240.0..=560.0).show_inside(ui, |ui| {
            let h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
            let mut sa = egui::ScrollArea::vertical().auto_shrink([false; 2]);
            if self.scroll_to_sel {
                if let Some(i) = self.sel.and_then(|s| self.rows.iter().position(|&r| r == s)) {
                    sa = sa.vertical_scroll_offset((i as f32 * (h + ui.spacing().item_spacing.y) - 100.0).max(0.0));
                }
                self.scroll_to_sel = false;
            }
            let mut clicked = None;
            sa.show_rows(ui, h, self.rows.len(), |ui, range| {
                for i in range {
                    let r = self.rows[i];
                    let t = &set.stores[r.0].textures[r.1];
                    let txt = format!("{:016x}  {}×{}  {}", t.hash, t.width, t.height, sixng_core::texture::dxgi_name(t.dxgi_format).trim_start_matches("DXGI_FORMAT_"));
                    if ui.selectable_label(self.sel == Some(r), egui::RichText::new(txt).monospace().small()).clicked() {
                        clicked = Some(r);
                    }
                }
            });
            if let Some(r) = clicked {
                self.select(r);
            }
        });
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
                st.path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()
            ));
        });
        ui.horizontal_wrapped(|ui| {
            let mut m = self.mip;
            egui::ComboBox::from_id_salt("tv_mip").selected_text(format!("mip {} ({}×{})", m, (t.width >> m).max(1), (t.height >> m).max(1))).show_ui(ui, |ui| {
                for k in 0..t.mips() {
                    ui.selectable_value(&mut m, k, format!("mip {} ({}×{})", k, (t.width >> k).max(1), (t.height >> k).max(1)));
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
            if ui.add_enabled(self.decoded.is_some(), egui::Button::new("Export PNG")).on_hover_text("Save this mip to <work>/out/textures/").clicked() {
                self.export_png();
            }
            if let Some(p) = &self.last_export {
                if ui.button(format!("Show in {}", crate::FILE_MANAGER)).clicked() {
                    crate::reveal(p);
                }
            }
        });
        if let Some(s) = &self.status {
            ui.label(egui::RichText::new(s).small());
        }
        if let Some(e) = &self.error {
            ui.colored_label(egui::Color32::from_rgb(230, 120, 90), format!("Cannot decode: {e}"));
            return;
        }
        if self.decoding {
            ui.spinner();
            return;
        }
        let Some(d) = &self.decoded else { return };
        if d.undecoded > 0 {
            ui.colored_label(egui::Color32::from_rgb(220, 180, 80), format!("{} KiB of this texture is BCPack-compressed and cannot be decoded; lower mips may be clean.", d.undecoded / 1024));
        }
        let need = (d.hash, d.mip, self.channel);
        if self.tex.as_ref().map(|x| (x.0, x.1, x.2)) != Some(need) {
            let img = Self::channel_image(d, self.channel);
            let h = ui.ctx().load_texture(format!("tv_{:016x}_{}_{}", d.hash, d.mip, self.channel), img, egui::TextureOptions::NEAREST);
            self.tex = Some((need.0, need.1, need.2, h));
        }
        let (dw, dh) = (d.w as f32, d.h as f32);
        let tex = self.tex.as_ref().unwrap().3.clone();
        egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
            let avail = ui.available_size();
            let z = if self.zoom <= 0.0 { (avail.x / dw).min(avail.y / dh).min(8.0).max(0.01) } else { self.zoom };
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
                            let col = if (x + y) % 2 == 0 { egui::Color32::from_gray(90) } else { egui::Color32::from_gray(60) };
                            let r = egui::Rect::from_min_size(rect.min + egui::vec2(x as f32 * c, y as f32 * c), egui::vec2(c, c)).intersect(rect);
                            p.rect_filled(r, 0.0, col);
                        }
                    }
                }
            }
            ui.painter().image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        });
    }
}

fn r_ok_path(status: &Option<String>, p: PathBuf) -> Option<PathBuf> {
    status.as_ref().filter(|s| s.starts_with("Saved")).map(|_| p)
}
