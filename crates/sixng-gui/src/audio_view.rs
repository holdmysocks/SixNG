//! Audio page: banks found in audio.rpf, their streams, playback and export.
//!
//! Stream names are hashes in the game data (not recovered), so streams are
//! listed by bank offset and stream id. Playback decodes the Opus packets
//! with libopus in a background thread and plays them through the default
//! output device.

use eframe::egui;
use sixng_core::audio::{self, AudioIndex, Bank, Stream};
use sixng_core::export::Workspace;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Slot<T> = Arc<Mutex<Option<Result<T, String>>>>;

struct Decoded {
    key: (u64, u32),
    ch: u8,
    pcm: Vec<i16>,
}

struct Player {
    _stream: rodio::OutputStream,
    sink: rodio::Sink,
    key: (u64, u32),
    len: f64,
}

pub struct AudioView {
    game: String,
    work: String,
    ix: Option<Arc<AudioIndex>>,
    archive: Option<sixng_core::game::ArchiveInfo>,
    loading: Slot<Arc<AudioIndex>>,
    load_started: bool,
    scan_progress: Arc<Mutex<(u64, u64)>>,
    scanning: bool,
    /// rows of the bank list (indices into ix.banks), after filters
    rows: Vec<usize>,
    dirty: bool,
    min_seconds: f32,
    search: String,
    sel: Option<usize>,
    decode: Slot<Decoded>,
    decoding: Option<(u64, u32)>,
    player: Option<Player>,
    volume: f32,
    muted: bool,
    status: Option<String>,
    error: Option<String>,
    last_export: Option<PathBuf>,
    pending_bank: Option<u64>,
    pending_play: Option<u32>,
}

fn fmt_secs(s: f64) -> String {
    let s = s.max(0.0);
    if s >= 3600.0 {
        format!("{}:{:02}:{:02}", (s / 3600.0) as u64, (s / 60.0) as u64 % 60, s as u64 % 60)
    } else {
        format!("{}:{:04.1}", (s / 60.0) as u64, s % 60.0)
    }
}

impl AudioView {
    pub fn new(game: &str, work: &str) -> Self {
        AudioView {
            game: game.into(),
            work: work.into(),
            ix: None,
            archive: None,
            loading: Arc::new(Mutex::new(None)),
            load_started: false,
            scan_progress: Arc::new(Mutex::new((0, 0))),
            scanning: false,
            rows: Vec::new(),
            dirty: true,
            min_seconds: 0.0,
            search: String::new(),
            sel: None,
            decode: Arc::new(Mutex::new(None)),
            decoding: None,
            player: None,
            volume: 0.8,
            muted: false,
            status: None,
            error: None,
            last_export: None,
            pending_bank: None,
            pending_play: None,
        }
    }

    /// Select a bank by archive offset once the index is loaded.
    pub fn select_bank(&mut self, off: u64, play: Option<u32>) {
        self.pending_bank = Some(off);
        self.pending_play = play;
    }

    /// Ready for a scripted screenshot.
    pub fn idle(&self) -> bool {
        self.load_started && !self.scanning && self.decoding.is_none() && self.pending_play.is_none() && (self.ix.is_some() || self.error.is_some())
    }

    fn out_dir(&self) -> PathBuf {
        Path::new(&self.work).join("out").join("audio")
    }

    /// Load the saved index (no scan); `scan` = scan the archive first.
    fn start_load(&mut self, scan: bool) {
        self.load_started = true;
        self.error = None;
        let ws = Workspace::new(&self.game, &self.work);
        let a = match ws.resolve("audio") {
            Ok(a) => a,
            Err(e) => {
                self.error = Some(format!("No audio archive in the game folder: {e}"));
                return;
            }
        };
        self.archive = Some(a.clone());
        let slot = self.loading.clone();
        let work = PathBuf::from(&self.work);
        if !scan {
            if !audio::index_path(&work, &a.id).exists() {
                return; // not scanned yet: the page offers the scan
            }
            self.scanning = true;
            let id = a.id.clone();
            std::thread::spawn(move || {
                let r = audio::load_index(&work, &id).map(Arc::new).map_err(|e| e.to_string());
                *slot.lock().unwrap() = Some(r);
            });
            return;
        }
        self.scanning = true;
        let prog = self.scan_progress.clone();
        std::thread::spawn(move || {
            let p: sixng_core::export::Progress = Arc::new(move |_s: &str, d: u64, t: u64| *prog.lock().unwrap() = (d, t));
            let r = audio::scan(&a.path, &a.id, p).and_then(|ix| {
                audio::save_index(&work, &ix)?;
                Ok(ix)
            });
            *slot.lock().unwrap() = Some(r.map(Arc::new).map_err(|e| e.to_string()));
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let loaded = self.loading.lock().unwrap().take();
        if let Some(r) = loaded {
            self.scanning = false;
            match r {
                Ok(ix) => {
                    self.ix = Some(ix);
                    self.dirty = true;
                }
                Err(e) => self.error = Some(e),
            }
        }
        if self.scanning {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        let done = self.decode.lock().unwrap().take();
        if let Some(r) = done {
            self.decoding = None;
            match r {
                Ok(d) => self.play_pcm(d),
                Err(e) => self.error = Some(e),
            }
        }
        if self.decoding.is_some() || self.player.as_ref().map_or(false, |p| !p.sink.empty()) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn refilter(&mut self) {
        self.dirty = false;
        let Some(ix) = &self.ix else { return };
        let q = self.search.trim().to_ascii_lowercase().trim_start_matches("0x").to_string();
        let min = self.min_seconds as f64;
        self.rows = ix
            .banks
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                let playable = b.streams.iter().any(|s| s.layout != "none" && s.seconds() >= min);
                let hit = q.is_empty() || format!("{:x}", b.off).contains(&q) || b.streams.iter().any(|s| format!("{:08x}", s.id).contains(&q));
                playable && hit
            })
            .map(|(i, _)| i)
            .collect();
    }

    fn play(&mut self, b: &Bank, s: &Stream) {
        self.stop();
        self.error = None;
        let Some(a) = self.archive.clone() else { return };
        let key = (b.off, s.id);
        self.decoding = Some(key);
        let (b, s) = (b.clone(), s.clone());
        let slot = self.decode.clone();
        std::thread::spawn(move || {
            let r = (|| -> anyhow::Result<Decoded> {
                let bytes = audio::read_bank(&a.path, &b)?;
                let pk = audio::packets(&bytes, &s);
                if pk.is_empty() {
                    anyhow::bail!("no Opus packets in this stream");
                }
                let (ch, pcm) = audio::decode(&pk, s.frames())?;
                Ok(Decoded { key, ch, pcm })
            })();
            *slot.lock().unwrap() = Some(r.map_err(|e| e.to_string()));
        });
    }

    fn play_pcm(&mut self, d: Decoded) {
        let r = (|| -> Result<Player, String> {
            let (stream, handle) = rodio::OutputStream::try_default().map_err(|e| format!("no audio output: {e}"))?;
            let sink = rodio::Sink::try_new(&handle).map_err(|e| e.to_string())?;
            let len = d.pcm.len() as f64 / d.ch.max(1) as f64 / 48000.0;
            sink.set_volume(self.out_volume());
            sink.append(rodio::buffer::SamplesBuffer::new(d.ch as u16, 48000, d.pcm));
            Ok(Player { _stream: stream, sink, key: d.key, len })
        })();
        match r {
            Ok(p) => self.player = Some(p),
            Err(e) => self.error = Some(e),
        }
    }

    fn out_volume(&self) -> f32 {
        if self.muted { 0.0 } else { self.volume }
    }

    /// Speaker button (mute / unmute) and a volume slider.
    fn volume_ui(&mut self, ui: &mut egui::Ui) {
        let icon = if self.muted || self.volume == 0.0 {
            "🔇"
        } else if self.volume < 0.5 {
            "🔉"
        } else {
            "🔊"
        };
        let mut changed = false;
        let tip = if self.muted { "Unmute" } else { "Mute" };
        if ui.button(egui::RichText::new(icon).size(16.0)).on_hover_text(tip).clicked() {
            self.muted = !self.muted;
            changed = true;
        }
        let r = ui.add(egui::Slider::new(&mut self.volume, 0.0..=1.0).show_value(false)).on_hover_text(format!("Volume {:.0}%", self.volume * 100.0));
        if r.changed() {
            self.muted = false;
            changed = true;
        }
        if changed {
            if let Some(p) = &self.player {
                p.sink.set_volume(self.out_volume());
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(p) = self.player.take() {
            p.sink.stop();
        }
    }

    fn export(&mut self, b: &Bank, s: Option<&Stream>, fmt: &str) {
        let Some(a) = self.archive.clone() else { return };
        let dir = self.out_dir();
        let r = (|| -> anyhow::Result<Vec<String>> {
            let bytes = audio::read_bank(&a.path, b)?;
            if fmt == "awc" {
                std::fs::create_dir_all(&dir)?;
                let p = dir.join(format!("{:x}.awc", b.off));
                std::fs::write(&p, &bytes)?;
                return Ok(vec![p.display().to_string()]);
            }
            let mut files = Vec::new();
            for st in b.streams.iter().filter(|st| s.map_or(st.layout == "blocks", |x| x.id == st.id)) {
                files.extend(audio::export_stream(&bytes, b, st, &dir, fmt)?);
            }
            Ok(files)
        })();
        match r {
            Ok(f) => {
                self.status = Some(format!("Saved {} file(s) to {}", f.len(), dir.display()));
                self.last_export = f.first().map(PathBuf::from);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if !self.load_started {
            self.start_load(false);
        }
        self.poll(ui.ctx());
        ui.horizontal(|ui| {
            ui.heading("🔊 Audio");
            ui.weak("Banks in audio.rpf. Stream names are hashes in the game data and are not recovered, so streams are listed by bank offset and stream id.");
        });
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        let Some(ix) = self.ix.clone() else {
            ui.add_space(12.0);
            if self.scanning {
                let (d, t) = *self.scan_progress.lock().unwrap();
                let f = if t > 0 { d as f32 / t as f32 } else { 0.0 };
                if t == 0 {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading the audio index…");
                    });
                    return;
                }
                ui.label("Scanning audio.rpf for audio banks (reads the whole archive once, a few minutes)…");
                ui.add(egui::ProgressBar::new(f).show_percentage().desired_width(420.0));
            } else if self.archive.is_some() {
                ui.label("audio.rpf has not been scanned yet. The scan reads the archive once and saves an index to <work>/index.");
                if ui.button("Scan audio.rpf").clicked() {
                    self.start_load(true);
                }
            }
            return;
        };
        if self.dirty {
            self.refilter();
        }
        if let Some(off) = self.pending_bank.take() {
            self.sel = ix.banks.iter().position(|b| b.off == off);
            if let (Some(bi), Some(id)) = (self.sel, self.pending_play.take()) {
                let b = ix.banks[bi].clone();
                if let Some(st) = b.streams.iter().find(|st| st.id == id).cloned() {
                    self.play(&b, &st);
                }
            }
        }
        ui.horizontal(|ui| {
            ui.label("Find:");
            if ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("bank offset or stream id (hex)").desired_width(220.0)).changed() {
                self.dirty = true;
            }
            ui.label("Min length:");
            if ui.add(egui::Slider::new(&mut self.min_seconds, 0.0..=120.0).suffix(" s").logarithmic(true).max_decimals(1)).changed() {
                self.dirty = true;
            }
            self.volume_ui(ui);
            let n: usize = ix.banks.iter().map(|b| b.streams.iter().filter(|s| s.layout != "none").count()).sum();
            ui.weak(format!("{} of {} banks, {} streams", self.rows.len(), ix.banks.len(), n));
            if ui.small_button("Rescan").on_hover_text("Scan audio.rpf again").clicked() {
                self.ix = None;
                self.start_load(true);
            }
        });
        if let Some(p) = &self.player {
            let pos = p.sink.get_pos().as_secs_f64().min(p.len);
            ui.horizontal(|ui| {
                if ui.button("⏹ Stop").clicked() {
                    p.sink.stop();
                }
                ui.label(format!("{:x} / {:08x}", p.key.0, p.key.1));
                ui.add(egui::ProgressBar::new((pos / p.len.max(0.001)) as f32).text(format!("{} / {}", fmt_secs(pos), fmt_secs(p.len))).desired_width(360.0));
            });
            if p.sink.empty() {
                self.player = None;
            }
        }
        if let Some(s) = &self.status {
            ui.horizontal(|ui| {
                ui.weak(s);
                if let Some(f) = &self.last_export {
                    if ui.small_button("Show folder").clicked() {
                        let dir = f.parent().unwrap_or(f).to_path_buf();
                        crate::open_path(&dir);
                    }
                }
            });
        }
        ui.separator();
        let avail = ui.available_size();
        ui.horizontal_top(|ui| {
            // bank list
            ui.vertical(|ui| {
                ui.set_width(320.0);
                ui.strong("Banks");
                let rows = self.rows.clone();
                egui::ScrollArea::vertical().id_salt("audio_banks").max_height(avail.y - 30.0).auto_shrink([false, false]).show_rows(ui, 18.0, rows.len(), |ui, range| {
                    for &bi in &rows[range] {
                        let b = &ix.banks[bi];
                        let n = b.streams.iter().filter(|s| s.layout != "none").count();
                        let t = format!("{:>9x}  {:>4} stream{}  {}", b.off, n, if n == 1 { " " } else { "s" }, fmt_secs(b.seconds()));
                        if ui.selectable_label(self.sel == Some(bi), egui::RichText::new(t).monospace()).clicked() {
                            self.sel = Some(bi);
                        }
                    }
                });
            });
            ui.separator();
            // streams of the selected bank
            ui.vertical(|ui| {
                let Some(bi) = self.sel else {
                    ui.weak("Pick a bank on the left.");
                    return;
                };
                let b = ix.banks[bi].clone();
                ui.horizontal(|ui| {
                    ui.strong(format!("Bank {:x}", b.off));
                    ui.weak(format!("{:.1} KiB, flags {:08x}", b.size as f64 / 1024.0, b.flags));
                    if ui.button("Export all .opus").on_hover_text("Lossless Ogg Opus remux of every stream").clicked() {
                        self.export(&b, None, "opus");
                    }
                    if ui.button("Export all .wav").on_hover_text("Decoded 16-bit PCM, 48 kHz").clicked() {
                        self.export(&b, None, "wav");
                    }
                    if ui.button("Export bank .awc").on_hover_text("The bank's raw bytes").clicked() {
                        self.export(&b, None, "awc");
                    }
                });
                let min = self.min_seconds as f64;
                let streams: Vec<Stream> = b.streams.iter().filter(|s| s.layout != "none" && s.seconds() >= min).cloned().collect();
                egui::ScrollArea::vertical().id_salt("audio_streams").auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("audio_stream_grid").striped(true).num_columns(6).show(ui, |ui| {
                        ui.strong("Stream id");
                        ui.strong("Length");
                        ui.strong("Rate");
                        ui.strong("Layout");
                        ui.strong("");
                        ui.strong("Export");
                        ui.end_row();
                        for s in &streams {
                            let key = (b.off, s.id);
                            ui.monospace(format!("{:08x}", s.id));
                            ui.label(fmt_secs(s.seconds()));
                            ui.label(format!("{} Hz", s.rate)).on_hover_text("Rate field of the stream. Opus always decodes at 48 kHz; this is likely the source rate.");
                            match s.layout.as_str() {
                                "music" => {
                                    ui.label(format!("music, {} ch", s.channels)).on_hover_text("Multichannel music: the block header is not decoded yet. Export the bank raw (.awc).");
                                }
                                "blocks" => {
                                    ui.label(format!("Opus, {}", if s.channels == 2 { "stereo" } else { "mono" }));
                                }
                                _ => {
                                    ui.label(format!("unknown codec {}", s.codec)).on_hover_text("Not decoded. Export the bank raw (.awc).");
                                }
                            }
                            let playing = self.player.as_ref().map_or(false, |p| p.key == key);
                            if self.decoding == Some(key) {
                                ui.spinner();
                            } else if playing {
                                if ui.button("⏹ Stop").clicked() {
                                    self.stop();
                                }
                            } else if ui.add_enabled(s.layout == "blocks" && self.decoding.is_none(), egui::Button::new("▶ Play")).clicked() {
                                self.play(&b, s);
                            }
                            ui.horizontal(|ui| {
                                let ok = s.layout == "blocks";
                                if ui.add_enabled(ok, egui::Button::new(".opus").small()).clicked() {
                                    self.export(&b, Some(s), "opus");
                                }
                                if ui.add_enabled(ok, egui::Button::new(".wav").small()).clicked() {
                                    self.export(&b, Some(s), "wav");
                                }
                            });
                            ui.end_row();
                        }
                    });
                    if streams.is_empty() {
                        ui.weak("No streams at this minimum length.");
                    }
                });
            });
        });
    }
}
