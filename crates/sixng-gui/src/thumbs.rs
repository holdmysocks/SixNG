//! Model thumbnails for the Library: rendered in the background with the app's
//! own 3D renderer (one model at a time) and cached as 256 px PNGs in
//! `<work>/thumbs/`, under a disk budget. Tiles ask for a thumbnail every frame
//! they are visible; the most recent requests are rendered first.

use crate::glb;
use crate::render3d::{Camera, Look, Renderer3D};
use eframe::egui;
use sixng_core::export::Workspace;
use sixng_core::scan::ArchiveIndex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SIZE: u32 = 256;
/// GPU textures kept for tiles (least recently drawn are dropped).
const MAX_HANDLES: usize = 600;

enum Slot {
    Ready(egui::TextureHandle, u64),
    Failed,
}

pub struct Thumbs {
    dir: PathBuf,
    game: String,
    work: String,
    pub budget: u64,
    pub used: u64,
    slots: HashMap<String, Slot>,
    want: Vec<String>,
    frame: u64,
    busy: Option<String>,
    loaded: Arc<Mutex<Option<(String, Result<glb::Model, String>)>>>,
    renderer: Option<Renderer3D>,
    failed_ids: HashSet<String>,
    pub rendered: usize,
}

fn file_name(id: &str) -> String {
    id.replace(['@', '/', '\\', ':'], "_") + ".png"
}

fn dir_size(d: &Path) -> u64 {
    std::fs::read_dir(d)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
}

impl Thumbs {
    pub fn new(game: &str, work: &str, budget_mb: u64) -> Self {
        // v2: animals and characters are put together from their parts
        let dir = Path::new(work).join("thumbs").join("v3");
        let used = dir_size(&dir);
        let failed_ids = std::fs::read_to_string(dir.join("failed.txt"))
            .map(|s| s.lines().map(String::from).collect())
            .unwrap_or_default();
        Thumbs {
            dir,
            game: game.into(),
            work: work.into(),
            budget: budget_mb << 20,
            used,
            slots: HashMap::new(),
            want: Vec::new(),
            frame: 0,
            busy: None,
            loaded: Arc::new(Mutex::new(None)),
            renderer: None,
            failed_ids,
            rendered: 0,
        }
    }

    pub fn over_budget(&self) -> bool {
        self.used >= self.budget
    }

    /// Thumbnail for a model id, if ready; otherwise it is queued.
    pub fn get(&mut self, ctx: &egui::Context, id: &str) -> Option<egui::TextureHandle> {
        self.get_with_request(ctx, id, true)
    }

    /// Return a cached thumbnail without scheduling a GPU render on a cache miss.
    pub fn get_cached(&mut self, ctx: &egui::Context, id: &str) -> Option<egui::TextureHandle> {
        self.get_with_request(ctx, id, false)
    }

    fn get_with_request(
        &mut self,
        ctx: &egui::Context,
        id: &str,
        request_on_miss: bool,
    ) -> Option<egui::TextureHandle> {
        match self.slots.get_mut(id) {
            Some(Slot::Ready(h, used)) => {
                *used = self.frame;
                return Some(h.clone());
            }
            Some(Slot::Failed) => return None,
            None => {}
        }
        if self.failed_ids.contains(id) {
            self.slots.insert(id.to_string(), Slot::Failed);
            return None;
        }
        let p = self.dir.join(file_name(id));
        if let Ok(img) = image::open(&p) {
            let img = img.to_rgba8();
            let (w, h) = img.dimensions();
            let ci =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
            let hnd = ctx.load_texture(format!("thumb_{id}"), ci, egui::TextureOptions::LINEAR);
            self.slots
                .insert(id.to_string(), Slot::Ready(hnd.clone(), self.frame));
            return Some(hnd);
        }
        if request_on_miss && !self.want.iter().any(|w| w == id) {
            self.want.push(id.to_string());
        }
        None
    }

    pub fn is_pending(&self, id: &str) -> bool {
        self.busy.as_deref() == Some(id) || self.want.iter().any(|w| w == id)
    }

    pub fn queued(&self) -> usize {
        self.want.len() + self.busy.is_some() as usize
    }

    /// Render at most one finished model and start the next load; call once per frame.
    pub fn pump(
        &mut self,
        ctx: &egui::Context,
        rs: Option<&eframe::egui_wgpu::RenderState>,
        idx: &Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>,
    ) {
        self.frame += 1;
        // drop old handles
        if self.slots.len() > MAX_HANDLES {
            let mut v: Vec<(u64, String)> = self
                .slots
                .iter()
                .filter_map(|(k, s)| {
                    if let Slot::Ready(_, u) = s {
                        Some((*u, k.clone()))
                    } else {
                        None
                    }
                })
                .collect();
            v.sort();
            for (_, k) in v.into_iter().take(self.slots.len() - MAX_HANDLES) {
                self.slots.remove(&k);
            }
        }

        let Some(rs) = rs else { return };
        let done = self.loaded.lock().unwrap().take();
        if let Some((id, r)) = done {
            self.busy = None;
            match r {
                Ok(model) => match self.render(rs, &model) {
                    Some((w, h, px)) => {
                        let _ = std::fs::create_dir_all(&self.dir);
                        let p = self.dir.join(file_name(&id));
                        if crate::render3d::save_png(&p, w, h, &px).is_ok() {
                            self.used += std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                            self.rendered += 1;
                        }
                        let ci =
                            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
                        let hnd = ctx.load_texture(
                            format!("thumb_{id}"),
                            ci,
                            egui::TextureOptions::LINEAR,
                        );
                        self.slots.insert(id, Slot::Ready(hnd, self.frame));
                    }
                    None => self.fail(id),
                },
                Err(_) => self.fail(id),
            }
            ctx.request_repaint();
        }
        if self.busy.is_some() || self.over_budget() {
            return;
        }
        // newest request first: what the user is looking at now
        let Some(id) = self.want.pop() else { return };
        // keep only recent requests (tiles scrolled away stop asking)
        if self.want.len() > 200 {
            self.want.drain(..self.want.len() - 200);
        }
        self.busy = Some(id.clone());
        let ws = Workspace::new(&self.game, &self.work);
        let cache = Path::new(&self.work).join(crate::GUI_CACHE);
        let slot = self.loaded.clone();
        let idx = idx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::model_view::load_query(&ws, &cache, &id, 0, &idx)
                    .map(|l| l.model)
                    .map_err(|e| format!("{e:#}"))
            }))
            .unwrap_or_else(|_| Err("crashed".into()));
            *slot.lock().unwrap() = Some((id, r));
            ctx.request_repaint();
        });
    }

    fn fail(&mut self, id: String) {
        use std::io::Write;
        let _ = std::fs::create_dir_all(&self.dir);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("failed.txt"))
        {
            let _ = writeln!(f, "{id}");
        }
        self.failed_ids.insert(id.clone());
        self.slots.insert(id, Slot::Failed);
    }

    fn render(
        &mut self,
        rs: &eframe::egui_wgpu::RenderState,
        model: &glb::Model,
    ) -> Option<(u32, u32, Vec<u8>)> {
        if model.meshes.is_empty() {
            return None;
        }
        let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
        let g = ren.upload(rs, model, &[]);
        let mut cam = Camera::default();
        cam.frame_bbox(g.bbox[0], g.bbox[1]);
        cam.yaw = 0.75;
        cam.pitch = 0.35;
        cam.dist *= 0.85;
        let mut look = Look::default();
        look.set_time_of_day(13.0);
        look.clear = [0.16, 0.17, 0.19];
        ren.render(rs, [SIZE, SIZE], &cam, &look, &[&g], &[], &[]);
        ren.read_back(rs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_lookup_does_not_queue_gpu_work_on_a_miss() {
        let work = std::env::temp_dir().join(format!(
            "sixng-thumbs-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut thumbs = Thumbs::new("", work.to_str().unwrap(), 1);
        assert!(thumbs
            .get_cached(&egui::Context::default(), "missing-model")
            .is_none());
        assert_eq!(thumbs.queued(), 0);
    }
}
