#![allow(dead_code)]
//! In-app GPU model viewer: exports a drawable/fragment to the GUI cache,
//! loads the GLB + textures and shows it with PBR shading, debug views,
//! car paint, LOD switch, skeleton and screenshots.
use crate::glb;
use crate::render3d::{self, Camera, GpuModel, LineV, Look, Renderer3D};
use eframe::egui;
use eframe::egui_wgpu;
use glam::{Mat4, Quat, Vec3};
use sixng_core::export::{self, ExportOptions, Workspace};
use sixng_core::scan::ArchiveIndex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const MODES: [&str; 8] = ["shaded", "albedo", "normals", "UV checker", "texture slot", "BCPack gaps", "vertex colour", "lighting only"];
pub const RMA_ORDERS: [&str; 4] = ["R rough / G metal / B AO", "ORM (R AO / G rough / B metal)", "R metal / G rough / B AO", "ignore packed"];
pub const FINISHES: [&str; 4] = ["metallic", "matte", "chrome", "gloss"];
pub const SLOTS: [&str; 4] = ["base colour", "normal", "packed RMA", "emissive"];

pub struct Loaded {
    pub model: glb::Model,
    pub lods: Vec<PathBuf>,
    pub lod: usize,
    pub out_root: PathBuf,
    pub source: String,
    pub info: serde_json::Value,
    /// Choosable parts (animals and characters) and the offsets shown now.
    pub parts: Vec<crate::assemble::PartOpt>,
    pub selected: Vec<u64>,
    /// Query without the `#parts=` selection.
    pub base: String,
}

type Pending = Arc<Mutex<Option<Result<Loaded, String>>>>;

/// Face variant the viewer exports ped heads with (`ExportOptions::head_skin`).
pub static HEAD_SKIN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Last head exported for the viewer: (variant, variant count, source).
static HEAD_SKIN_SEEN: Mutex<Option<(usize, usize, String)>> = Mutex::new(None);

#[derive(Default, Clone)]
pub struct Automation {
    pub screenshot: Option<PathBuf>,
    pub exit: bool,
    pub frames: u32,
    pub ui_shot: Option<PathBuf>,
}

/// Shared-wheel scale per unit of the car's wheelScale: fitted by eye on
/// phoenix (0.37 at wheelScale 0.2277); the game's rule is not decoded.
pub const WHEEL_FACTOR: f32 = 0.37 / 0.227_721;

pub struct ModelView {
    pub query: String,
    pub status: String,
    pending: Pending,
    loading: bool,
    pub loaded: Option<Loaded>,
    gpu: Option<GpuModel>,
    pub renderer: Option<Renderer3D>,
    pub cam: Camera,
    pub look: Look,
    pub tod: f32,
    pub show_skeleton: bool,
    pub show_normals: bool,
    /// Depth-bias decal-bucket materials (off = raw z-fighting, for comparison).
    pub decal_bias: bool,
    pub joint_labels: bool,
    pub wheel_spin: f32,
    pub door_open: f32,
    pub paint_on: bool,
    pub material_sel: Option<usize>,
    pub auto: Automation,
    frames_since_load: u32,
    pub indexes: Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>,
    pub game: String,
    pub work: String,
    pub size: [u32; 2],
    last_shot: Option<String>,
    pub zoom_after_load: Option<f32>,
    keep_cam_once: bool,
    pub wheels_on: bool,
    pub wheel_rim: String,
    pub wheel_tyre: String,
    pub wheel_scale: f32,
    pub wheel_scale_rear: f32,
    /// Size the shared wheels from the car's wheelScale (WHEEL_FACTOR x).
    pub wheel_auto: bool,
    pub vehicles: crate::vehicle_db::VehicleDb,
    /// Model the vehicle data was applied to, and the chosen configuration.
    veh_applied: String,
    pub veh_cfg: Option<usize>,
    wheel_pending: Arc<Mutex<Option<Result<Vec<Loaded>, String>>>>,
    wheel_key: String,
    wheels: Vec<GpuModel>,
    wheel_loading: bool,
    pub picker: crate::model_picker::ModelPicker,
    /// Set when the user asks to open a material's texture in the Textures tab.
    pub view_texture: Option<u64>,
    pub show_picker: bool,
}

impl ModelView {
    pub fn new(game: &str, work: &str) -> Self {
        ModelView {
            query: String::new(),
            status: "Type an entry id (levels.rpf@219c8b010), a name regex (^phoenix) or open a .glb.".into(),
            pending: Arc::new(Mutex::new(None)),
            loading: false,
            loaded: None,
            gpu: None,
            renderer: None,
            cam: Camera::default(),
            look: Look::default(),
            tod: 13.0,
            show_skeleton: false,
            show_normals: false,
            decal_bias: true,
            joint_labels: true,
            wheel_spin: 0.0,
            door_open: 0.0,
            paint_on: true,
            material_sel: None,
            auto: Automation::default(),
            frames_since_load: 0,
            indexes: Arc::new(Mutex::new(HashMap::new())),
            game: game.into(),
            work: work.into(),
            size: [0, 0],
            last_shot: None,
            zoom_after_load: None,
            keep_cam_once: false,
            wheels_on: true,
            wheel_rim: "rim_muscle_001_hi".into(),
            wheel_tyre: "tyre_muscle_001_hi".into(),
            wheel_scale: 0.37,
            wheel_scale_rear: 0.37,
            wheel_auto: true,
            vehicles: crate::vehicle_db::VehicleDb::new(),
            veh_applied: String::new(),
            veh_cfg: None,
            wheel_pending: Arc::new(Mutex::new(None)),
            wheel_key: String::new(),
            wheels: Vec::new(),
            wheel_loading: false,
            picker: crate::model_picker::ModelPicker::new(work),
            view_texture: None,
            show_picker: true,
        }
    }

    /// Nothing loading (model or picker list): for automation screenshots.
    pub fn idle(&self) -> bool {
        !self.loading && self.idle_for_shot() && (!self.show_picker || self.picker.loaded())
    }

    /// Car extras settled: vehicle data applied and the fitted wheels loaded.
    fn idle_for_shot(&self) -> bool {
        let Some(l) = self.loaded.as_ref().filter(|l| l.info["wheel_mounts"].is_array()) else { return true };
        let veh = match &*self.vehicles.state.lock().unwrap() {
            crate::vehicle_db::State::Idle | crate::vehicle_db::State::Loading(_) => false,
            crate::vehicle_db::State::Failed(_) => true,
            crate::vehicle_db::State::Ready(db) => {
                let key = crate::vehicle_db::model_key(&l.source);
                !db.contains_key(&key) || self.veh_applied == key
            }
        };
        let wheels = !self.wheels_on || (!self.wheel_loading && self.wheel_key == format!("{}|{}", self.wheel_rim, self.wheel_tyre));
        veh && wheels
    }

    pub fn cache_dir(&self) -> PathBuf {
        Path::new(&self.work).join(crate::GUI_CACHE)
    }

    /// Open by entry id, name regex or .glb path, on a worker thread.
    pub fn open(&mut self, query: &str, lod: usize, ctx: &egui::Context) {
        self.query = query.to_string();
        if query.contains('@') && !query.contains("#parts=") {
            self.picker.opened(query.trim());
        }
        self.loading = true;
        *HEAD_SKIN_SEEN.lock().unwrap() = None;
        self.status = format!("loading {query} …");
        let q = query.to_string();
        let ws = Workspace::new(&self.game, &self.work);
        let cache = self.cache_dir();
        let slot = self.pending.clone();
        let idx = self.indexes.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = load_query(&ws, &cache, &q, lod, &idx).map_err(|e| format!("{e:#}"));
            *slot.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    /// Entry ids of the parts the viewer shows for `id` now (root first for
    /// animals), when that is the open model and it has parts.
    pub fn selected_parts(&self, id: &str) -> Option<Vec<String>> {
        let l = self.loaded.as_ref()?;
        if l.base != id || l.parts.is_empty() {
            return None;
        }
        let root = (!id.contains("#outfit:")).then(|| id.to_string());
        Some(root.into_iter().chain(l.parts.iter().filter(|o| l.selected.contains(&o.off)).map(|o| o.id.clone())).collect())
    }

    /// Parts panel for animals and characters: one choice (or none) per
    /// group, plus buttons for the named alternate sets. Returns the query to
    /// reload with when the choice changes.
    fn parts_ui(&self, ui: &mut egui::Ui) -> Option<String> {
        let l = self.loaded.as_ref()?;
        if l.parts.is_empty() {
            return None;
        }
        let mut groups: std::collections::BTreeMap<&str, Vec<&crate::assemble::PartOpt>> = Default::default();
        for o in &l.parts {
            groups.entry(o.group.as_str()).or_default().push(o);
        }
        let mut sel = l.selected.clone();
        let mut changed = false;
        ui.heading(format!("Parts ({} shown of {})", l.selected.len(), l.parts.len()));
        ui.label(egui::RichText::new("One choice per group. Accessories and alternate sets start off.").weak());
        egui::Grid::new("parts_grid").num_columns(2).striped(true).show(ui, |ui| {
            for (g, opts) in &groups {
                let cur = opts.iter().find(|o| l.selected.contains(&o.off));
                ui.label(*g);
                let text = cur.map(|o| o.label.clone()).unwrap_or_else(|| "none".into());
                egui::ComboBox::from_id_salt(("part", *g)).selected_text(text).width(240.0).show_ui(ui, |ui| {
                    let mut set = |keep: Option<u64>| {
                        sel.retain(|x| !opts.iter().any(|o| o.off == *x));
                        sel.extend(keep);
                        changed = true;
                    };
                    if ui.selectable_label(cur.is_none(), "none").clicked() && cur.is_some() {
                        set(None);
                    }
                    for o in opts {
                        let mut label = o.label.clone();
                        if o.default && o.label != "default" {
                            label += "  (default)";
                        }
                        let on = cur.map(|c| c.off) == Some(o.off);
                        if ui.selectable_label(on, label).on_hover_text(&o.name).clicked() && !on {
                            set(Some(o.off));
                        }
                    }
                });
                ui.end_row();
            }
        });
        // named sets that span several groups (`08 collarharness`: body + harness)
        let mut sets: Vec<&str> = l.parts.iter().filter(|o| o.label != "default" && o.label.contains(' ') && l.parts.iter().filter(|p| p.label == o.label).count() > 1).map(|o| o.label.as_str()).collect();
        sets.sort();
        sets.dedup();
        ui.horizontal_wrapped(|ui| {
            for s in &sets {
                if ui.button(format!("set {s}")).on_hover_text("Switch every group that has this set to it").clicked() {
                    for opts in groups.values() {
                        if let Some(o) = opts.iter().find(|o| o.label == *s) {
                            sel.retain(|x| !opts.iter().any(|p| p.off == *x));
                            sel.push(o.off);
                        }
                    }
                    changed = true;
                }
            }
            if ui.button("Defaults").on_hover_text("Back to the default parts").clicked() {
                sel = l.parts.iter().filter(|o| o.default).map(|o| o.off).collect();
                changed = true;
            }
        });
        ui.separator();
        changed.then(|| format!("{}#parts={}", l.base, sel.iter().map(|o| format!("{o:x}")).collect::<Vec<_>>().join(",")))
    }

    pub fn open_lod(&mut self, lod: usize, ctx: &egui::Context) {
        let Some(l) = &self.loaded else { return };
        let Some(p) = l.lods.get(lod).cloned() else { return };
        let root = l.out_root.clone();
        let lods = l.lods.clone();
        let source = l.source.clone();
        let info = l.info.clone();
        let (parts, selected, base) = (l.parts.clone(), l.selected.clone(), l.base.clone());
        self.loading = true;
        let slot = self.pending.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = glb::load(&p, &root).map(|model| Loaded { model, lods, lod, out_root: root, source, info, parts, selected, base }).map_err(|e| format!("{e:#}"));
            *slot.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    fn poll(&mut self, rs: &egui_wgpu::RenderState) {
        let Some(r) = self.pending.lock().unwrap().take() else { return };
        self.loading = false;
        match r {
            Ok(l) => {
                let keep_cam = std::mem::take(&mut self.keep_cam_once) || self.loaded.as_ref().map(|o| o.source == l.source).unwrap_or(false);
                let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
                let mut g = ren.upload(rs, &l.model, &[]);
                ren.ensure_edges(rs, &mut g, &l.model);
                if !keep_cam {
                    self.cam.frame_bbox(g.bbox[0], g.bbox[1]);
                    if let Some(z) = self.zoom_after_load {
                        self.cam.dist *= z;
                    }
                    self.material_sel = None;
                }
                self.status = format!(
                    "{} — LOD{} · {} meshes · {} materials · {} tris · {} verts · {} joints · {} textures",
                    l.source,
                    shown_lod(&l.lods, l.lod, &l.info),
                    l.model.meshes.len(),
                    l.model.materials.len(),
                    l.model.triangles,
                    l.model.vertices,
                    l.model.joints.len(),
                    l.model.images.len()
                );
                let missing = l.model.materials.iter().filter(|m| m.gap == "no colour texture").count();
                if missing > 0 {
                    self.status += &format!(
                        " · {missing} of {} materials have no colour texture (not decodable or not in the game files; shown plain white, not guessed)",
                        l.model.materials.len()
                    );
                }
                self.gpu = Some(g);
                self.loaded = Some(l);
                self.frames_since_load = 0;
                self.apply_paint(rs);
            }
            Err(e) => self.status = format!("error: {e}"),
        }
    }

    fn apply_paint(&mut self, _rs: &egui_wgpu::RenderState) {}

    /// Decoded vehicle data for the loaded car: spawn configurations (body
    /// colour + rim + tyre), paint palettes, wheelScale. The first
    /// configuration is applied when a car opens.
    fn vehicle_ui(&mut self, ui: &mut egui::Ui) {
        let Some(l) = &self.loaded else { return };
        if !l.info["wheel_mounts"].is_array() {
            return;
        }
        let key = crate::vehicle_db::model_key(&l.source);
        let Some(db) = self.vehicles.get(&self.game, &self.work, ui.ctx()) else {
            if let Some(s) = self.vehicles.status() {
                ui.label(egui::RichText::new(s).weak());
                ui.separator();
            }
            return;
        };
        let Some(v) = db.get(&key) else {
            ui.label(egui::RichText::new(format!("no vehicle data for '{key}'")).weak());
            ui.separator();
            return;
        };
        if self.veh_applied != key {
            self.veh_applied = key.clone();
            self.veh_cfg = None;
            if !v.configs.is_empty() {
                self.apply_config(v, 0);
            } else {
                self.apply_scale(v);
            }
        }
        ui.heading(format!("Vehicle: {key}"));
        let ws = |x: Option<f32>| x.map(|x| format!("{x:.4}")).unwrap_or_else(|| "?".into());
        ui.label(format!("wheelScale {} / rear {}", ws(v.wheel_scale), ws(v.wheel_scale_rear)));
        if !v.configs.is_empty() {
            let label = |i: usize| {
                let c = &v.configs[i];
                format!("{} · {} · {}", i + 1, c.rim.as_deref().unwrap_or("no rim"), c.tyre.as_deref().unwrap_or("no tyre"))
            };
            let mut pick = None;
            egui::ComboBox::from_label(format!("config ({})", v.configs.len()))
                .selected_text(self.veh_cfg.map(label).unwrap_or_else(|| "custom".into()))
                .width(220.0)
                .show_ui(ui, |ui| {
                    for i in 0..v.configs.len() {
                        ui.horizontal(|ui| {
                            if let Some(c) = v.configs[i].colour {
                                let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                                ui.painter().rect_filled(r, 2.0, srgb32(c));
                            }
                            if ui.selectable_label(self.veh_cfg == Some(i), label(i)).clicked() {
                                pick = Some(i);
                            }
                        });
                    }
                });
            if let Some(i) = pick {
                self.apply_config(v, i);
            }
        }
        for (pi, p) in v.palettes.iter().enumerate().take(4) {
            ui.label(format!("palette {} · {} colours", pi + 1, p.len())).on_hover_text("Weighted paint colours (the stored list repeats colours; hover a swatch for its share). Click one to use it as the primary colour.");
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(300.0);
                ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                for (c, w) in p {
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
                    ui.painter().rect_filled(r, 2.0, srgb32(*c));
                    ui.painter().rect_stroke(r, 2.0, egui::Stroke::new(1.0_f32, egui::Color32::from_gray(90)), egui::StrokeKind::Inside);
                    let col = srgb32(*c);
                    if resp.on_hover_text(format!("#{:02x}{:02x}{:02x} · {:.1} %", col.r(), col.g(), col.b(), w * 100.0)).clicked() {
                        self.look.paint = *c;
                        self.veh_cfg = None;
                    }
                }
            });
        }
        if v.palettes.len() > 4 {
            ui.label(egui::RichText::new(format!("{} more palettes in vehicles.json", v.palettes.len() - 4)).weak().small());
        }
        ui.label(egui::RichText::new("Colours read as linear, shown in sRGB (unverified).").weak().small()).on_hover_text("Spawn configurations each set a body colour, rim and tyre; the first is applied when a car opens. Stored colours are all dark, so they are taken as linear values; not checked in game.");
        ui.separator();
    }

    fn apply_scale(&mut self, v: &crate::vehicle_db::Vehicle) {
        if self.wheel_auto {
            if let Some(f) = v.wheel_scale {
                self.wheel_scale = f * WHEEL_FACTOR;
                self.wheel_scale_rear = v.wheel_scale_rear.unwrap_or(f) * WHEEL_FACTOR;
            }
        }
    }

    fn apply_config(&mut self, v: &crate::vehicle_db::Vehicle, i: usize) {
        let c = &v.configs[i];
        if let Some(col) = c.colour {
            self.look.paint = col;
        }
        if let Some(r) = &c.rim {
            self.wheel_rim = format!("{r}_hi");
        }
        if let Some(t) = &c.tyre {
            self.wheel_tyre = format!("{t}_hi");
        }
        self.veh_cfg = Some(i);
        self.apply_scale(v);
    }

    /// Wheel mounts of the loaded car (bone, glTF-space pivot).
    fn mounts(&self) -> Vec<(String, Vec3)> {
        let Some(l) = &self.loaded else { return vec![] };
        l.info["wheel_mounts"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| {
                        let p: [f32; 3] = serde_json::from_value(m["pivot"].clone()).ok()?;
                        Some((m["bone"].as_str()?.to_string(), Vec3::new(p[0], p[2], -p[1])))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn update_wheels(&mut self, rs: &egui_wgpu::RenderState, ctx: &egui::Context) {
        if let Some(r) = self.wheel_pending.lock().unwrap().take() {
            self.wheel_loading = false;
            match r {
                Ok(ls) => {
                    let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
                    self.wheels = ls
                        .iter()
                        .map(|l| {
                            let mut g = ren.upload(rs, &l.model, &[]);
                            // rims take the secondary colour
                            for k in 0..g.mat_class.len() {
                                if g.mat_class[k] == 10 {
                                    ren.set_material_secondary(rs, &mut g, k, true);
                                }
                            }
                            g
                        })
                        .collect();
                }
                Err(e) => {
                    self.status = format!("{} · wheels: {e}", self.status);
                    self.wheels.clear();
                }
            }
        }
        let mounts = self.mounts();
        if !self.wheels_on || mounts.is_empty() {
            return;
        }
        let key = format!("{}|{}", self.wheel_rim, self.wheel_tyre);
        if key != self.wheel_key {
            self.wheel_key = key;
            self.wheels.clear();
            self.wheel_loading = true;
            let qs = [format!("^{}$", self.wheel_rim), format!("^{}$", self.wheel_tyre)];
            let ws = Workspace::new(&self.game, &self.work);
            let cache = self.cache_dir();
            let slot = self.wheel_pending.clone();
            let idx = self.indexes.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let r: Result<Vec<Loaded>, String> = qs.iter().filter(|q| q.len() > 3).map(|q| load_query(&ws, &cache, q, 0, &idx).map_err(|e| format!("{e:#}"))).collect();
                *slot.lock().unwrap() = Some(r);
                ctx.request_repaint();
            });
        }
        let inst: Vec<render3d::Instance> = mounts
            .iter()
            .map(|(bone, p)| {
                let right = bone.strip_prefix("wheel_").map(|s| s.starts_with('r')).unwrap_or(false);
                let mirror = if right { Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)) } else { Mat4::IDENTITY };
                let spin = Mat4::from_quat(Quat::from_rotation_x(self.wheel_spin));
                let rear = bone.strip_prefix("wheel_").map(|s| s.chars().nth(1) == Some('r')).unwrap_or(false);
                let sc = if rear { self.wheel_scale_rear } else { self.wheel_scale };
                render3d::Instance::new(Mat4::from_translation(*p) * spin * mirror * Mat4::from_scale(Vec3::splat(sc)), false)
            })
            .collect();
        if let Some(ren) = &self.renderer {
            for g in self.wheels.iter_mut() {
                ren.set_instances(rs, g, &inst);
            }
        }
    }

    /// Joint matrices (global * inverse bind) with wheel / door animation.
    fn joint_mats(&self) -> (Vec<Mat4>, Vec<Mat4>) {
        let Some(l) = &self.loaded else { return (vec![], vec![]) };
        let js = &l.model.joints;
        let mut global: Vec<Option<Mat4>> = vec![None; js.len()];
        fn g(i: usize, js: &[glb::Joint], global: &mut Vec<Option<Mat4>>, spin: f32, door: f32, depth: usize) -> Mat4 {
            if let Some(m) = global[i] {
                return m;
            }
            let j = &js[i];
            let mut local = Mat4::from_cols_array(&j.local);
            let n = j.name.to_lowercase();
            if spin != 0.0 && n.starts_with("wheel_") && !n.contains("dummy") {
                local = local * Mat4::from_quat(Quat::from_rotation_x(spin));
            }
            if door != 0.0 && matches!(n.as_str(), "door_dside_f" | "door_dside_r" | "door_pside_f" | "door_pside_r") {
                // hinge axis is vertical (glTF +Y); driver side is -X
                let side = if n.contains("dside") { -1.0 } else { 1.0 };
                local = local * Mat4::from_quat(Quat::from_rotation_y(side * door));
            }
            let m = match j.parent {
                Some(p) if p != i && depth < 64 => g(p, js, global, spin, door, depth + 1) * local,
                _ => local,
            };
            global[i] = Some(m);
            m
        }
        let gl: Vec<Mat4> = (0..js.len()).map(|i| g(i, js, &mut global, self.wheel_spin, self.door_open, 0)).collect();
        let skin: Vec<Mat4> = gl.iter().zip(js).map(|(m, j)| *m * Mat4::from_cols_array(&j.ibm)).collect();
        (gl, skin)
    }

    fn lines(&self, globals: &[Mat4]) -> (Vec<LineV>, Vec<LineV>) {
        let mut under = Vec::new();
        let mut top = Vec::new();
        let Some(l) = &self.loaded else { return (under, top) };
        if self.show_skeleton {
            for (i, j) in l.model.joints.iter().enumerate() {
                let p = globals[i].w_axis.truncate();
                if let Some(pa) = j.parent {
                    let q = globals[pa].w_axis.truncate();
                    let a = if pa == 0 { 70 } else { 255 };
                    top.push(LineV { pos: q.into(), col: [255, 220, 40, a] });
                    top.push(LineV { pos: p.into(), col: [255, 120, 40, a] });
                }
                let s = (self.cam.dist * 0.01).max(0.01);
                for (ax, c) in [(globals[i].x_axis, [255, 60, 60, 255]), (globals[i].y_axis, [60, 255, 60, 255]), (globals[i].z_axis, [80, 120, 255, 255])] {
                    top.push(LineV { pos: p.into(), col: c });
                    top.push(LineV { pos: (p + ax.truncate().normalize_or_zero() * s).into(), col: c });
                }
            }
        }
        if self.show_normals {
            let len = (self.cam.dist * 0.008).max(0.005);
            let mut budget = 120_000usize;
            for m in &l.model.meshes {
                let step = (m.verts.len() / budget.max(1)).max(1);
                for v in m.verts.iter().step_by(step) {
                    let p = Vec3::from(v.pos);
                    let n = Vec3::from(v.nrm);
                    under.push(LineV { pos: p.into(), col: [80, 160, 255, 255] });
                    under.push(LineV { pos: (p + n * len).into(), col: [80, 255, 160, 255] });
                }
                budget = budget.saturating_sub(m.verts.len() / step);
            }
        }
        // ground grid
        let lo = Vec3::from(l.model.bbox[0]);
        let hi = Vec3::from(l.model.bbox[1]);
        let ext = (hi - lo).max_element().max(1.0);
        let step = 10f32.powf((ext / 4.0).log10().floor()).max(0.1);
        let n = ((ext * 0.75) / step).ceil() as i32;
        let c = (lo + hi) * 0.5;
        for k in -n..=n {
            let col = if k == 0 { [255, 255, 255, 90] } else { [255, 255, 255, 35] };
            let x = c.x + k as f32 * step;
            let z = c.z + k as f32 * step;
            under.push(LineV { pos: [x, lo.y, c.z - n as f32 * step], col });
            under.push(LineV { pos: [x, lo.y, c.z + n as f32 * step], col });
            under.push(LineV { pos: [c.x - n as f32 * step, lo.y, z], col });
            under.push(LineV { pos: [c.x + n as f32 * step, lo.y, z], col });
        }
        (under, top)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let ctx = ui.ctx().clone();
        let Some(rs) = frame.wgpu_render_state() else {
            ui.label("The model viewer needs the wgpu renderer.");
            return;
        };
        self.poll(rs);
        // ↑/↓ step through the picker list (when no text field has focus)
        if self.show_picker && !ctx.wants_keyboard_input() && !self.loading {
            let d = ui.input(|i| if i.key_pressed(egui::Key::ArrowDown) { 1 } else if i.key_pressed(egui::Key::ArrowUp) { -1 } else { 0 });
            if d != 0 {
                if let Some(id) = self.picker.step(d) {
                    self.open(&id, 0, &ctx);
                }
            }
        }
        // toolbar
        ui.horizontal(|ui| {
            ui.toggle_value(&mut self.show_picker, "☰ Models").on_hover_text("Show / hide the model list");
            if self.show_picker {
                if ui.add_enabled(!self.loading, egui::Button::new("◀")).on_hover_text("previous in list (↑)").clicked() {
                    if let Some(id) = self.picker.step(-1) {
                        self.open(&id, 0, &ctx);
                    }
                }
                if ui.add_enabled(!self.loading, egui::Button::new("▶")).on_hover_text("next in list (↓)").clicked() {
                    if let Some(id) = self.picker.step(1) {
                        self.open(&id, 0, &ctx);
                    }
                }
            }
            ui.separator();
            ui.label("Model:");
            let r = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("levels.rpf@219c8b010 · ^phoenix · path.glb").desired_width(320.0));
            if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.add_enabled(!self.loading, egui::Button::new("Open")).clicked() {
                let q = self.query.clone();
                self.open(&q, 0, &ctx);
            }
            if ui.button("Open .glb…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("glb", &["glb"]).pick_file() {
                    self.open(&p.display().to_string(), 0, &ctx);
                }
            }
            if self.loading {
                ui.spinner();
            }
            if let Some(l) = &self.loaded {
                if l.lods.len() > 1 {
                    ui.separator();
                    ui.label("LOD");
                    let mut want = None;
                    let primary = primary_lod(&l.info);
                    for k in 0..l.lods.len() {
                        let level = lod_level(&l.lods[k]).unwrap_or(primary);
                        if ui.selectable_label(l.lod == k, format!("{level}")).clicked() && l.lod != k {
                            want = Some(k);
                        }
                    }
                    if let Some(k) = want {
                        self.open_lod(k, &ctx);
                    }
                }
            }
            ui.separator();
            if ui.button("📷 Screenshot").clicked() {
                let name = self.loaded.as_ref().map(|l| sanitize(&l.source)).unwrap_or_else(|| "empty".into());
                let p = Path::new(&self.work).join("screenshots").join(format!("{name}_{}.png", MODES[self.look.mode as usize].replace(' ', "_")));
                self.auto.screenshot = Some(p);
                self.auto.frames = 0;
            }
            if let Some(s) = &self.last_shot {
                ui.label(egui::RichText::new(s).small());
            }
        });
        ui.label(egui::RichText::new(&self.status).small());
        ui.separator();

        if self.show_picker {
            let ws = Workspace::new(&self.game, &self.work);
            let idx = self.indexes.clone();
            let current = if self.query.contains('@') { Some(self.query.trim().to_string()) } else { None };
            let mut want = None;
            egui::SidePanel::left("model_picker").resizable(true).default_width(300.0).width_range(220.0..=560.0).show_inside(ui, |ui| {
                want = self.picker.ui(ui, ws, idx, current.as_deref());
            });
            if let Some(id) = want {
                if !self.loading {
                    self.open(&id, 0, &ctx);
                }
            }
        }
        egui::SidePanel::right("model_side").resizable(true).default_width(330.0).show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.side_ui(ui, rs));
        });

        // viewport
        let avail = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(avail, egui::Sense::click_and_drag());
        self.cam.handle(ui, &resp);
        if resp.double_clicked() {
            if let Some(g) = &self.gpu {
                self.cam.fly = false;
                self.cam.frame_bbox(g.bbox[0], g.bbox[1]);
            }
        }
        let ppp = ctx.pixels_per_point();
        let px = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
        self.size = px;
        self.update_wheels(rs, &ctx);
        let (globals, skin) = self.joint_mats();
        let show_wheels = self.wheels_on && !self.mounts().is_empty();
        let (under, top) = self.lines(&globals);
        let settled = self.idle_for_shot();
        let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
        if let Some(g) = &self.gpu {
            if !skin.is_empty() {
                ren.set_joints(rs, g, &skin);
            }
        }
        let mut models: Vec<&GpuModel> = self.gpu.iter().collect();
        if show_wheels {
            models.extend(self.wheels.iter());
        }
        ren.decal_bias = self.decal_bias;
        let tid = ren.render(rs, px, &self.cam, &self.look, &models, &under, &top);
        ui.painter().image(tid, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        // joint labels
        if self.show_skeleton && self.joint_labels {
            if let Some(l) = &self.loaded {
                for (i, j) in l.model.joints.iter().enumerate() {
                    let n = j.name.to_lowercase();
                    let important = n.starts_with("wheel") || n.starts_with("door") || n.starts_with("bonnet") || n.starts_with("boot") || n == "chassis" || l.model.joints.len() < 40;
                    if !important {
                        continue;
                    }
                    if let Some(p) = Renderer3D::project(&self.cam, rect.size(), globals[i].w_axis.truncate()) {
                        ui.painter().text(rect.min + p.to_vec2(), egui::Align2::LEFT_BOTTOM, &j.name, egui::FontId::proportional(11.0), egui::Color32::from_rgb(255, 230, 120));
                    }
                }
            }
        }
        // legend for the gap view
        if self.look.mode == 5 {
            let p = rect.left_bottom() + egui::vec2(10.0, -70.0);
            for (k, (c, t)) in [(egui::Color32::from_rgb(60, 200, 90), "ok (full mip chain)"), (egui::Color32::from_rgb(240, 190, 40), "lowmip (top mips in BCPack)"), (egui::Color32::from_rgb(230, 50, 50), "flat (no decodable mip)")].iter().enumerate() {
                let y = p.y + k as f32 * 18.0;
                ui.painter().rect_filled(egui::Rect::from_min_size(egui::pos2(p.x, y), egui::vec2(12.0, 12.0)), 2.0, *c);
                ui.painter().text(egui::pos2(p.x + 18.0, y), egui::Align2::LEFT_TOP, *t, egui::FontId::proportional(12.0), egui::Color32::WHITE);
            }
        }
        ui.painter().text(rect.right_bottom() + egui::vec2(-8.0, -6.0), egui::Align2::RIGHT_BOTTOM, if self.cam.fly { "fly: WASD/QE, Shift fast, drag look" } else { "orbit: drag · pan: right drag · zoom: wheel · double-click: frame" }, egui::FontId::proportional(11.0), egui::Color32::from_white_alpha(160));

        // screenshots (UI button and --screenshot automation)
        if self.gpu.is_some() || self.loaded.is_none() && !self.loading && self.auto.screenshot.is_some() && self.auto.frames > 30 {
            self.frames_since_load += 1;
        }
        if let Some(path) = self.auto.screenshot.clone() {
            self.auto.frames += 1;
            let ready = (self.gpu.is_some() && self.frames_since_load >= 3 && settled) || (!self.loading && self.auto.frames > 600);
            if ready {
                if let Some((w, h, px)) = ren.read_back(rs) {
                    match render3d::save_png(&path, w, h, &px) {
                        Ok(()) => self.last_shot = Some(format!("saved {}", path.display())),
                        Err(e) => self.last_shot = Some(format!("screenshot failed: {e}")),
                    }
                }
                self.auto.screenshot = None;
                // full window (UI included) next to it
                let ui_path = path.with_file_name(format!("{}_ui.png", path.file_stem().unwrap_or_default().to_string_lossy()));
                self.auto.ui_shot = Some(ui_path);
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            ctx.request_repaint();
        }
        if let Some(p) = self.auto.ui_shot.clone() {
            let shot = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                let _ = render3d::save_png(&p, img.size[0] as u32, img.size[1] as u32, &rgba);
                self.auto.ui_shot = None;
                if self.auto.exit {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            ctx.request_repaint();
        }
        if self.loading || resp.dragged() || self.cam.fly {
            ctx.request_repaint();
        }
    }

    fn side_ui(&mut self, ui: &mut egui::Ui, rs: &egui_wgpu::RenderState) {
        let seen = HEAD_SKIN_SEEN.lock().unwrap().clone();
        if let Some((i, n, src)) = seen.filter(|_| !self.loading) {
            ui.heading("Head skin");
            let mut step = None;
            ui.horizontal(|ui| {
                if ui.button("◀").on_hover_text("Previous face").clicked() {
                    step = Some((i + n - 1) % n);
                }
                ui.label(format!("{} / {n}", i + 1));
                if ui.button("▶").on_hover_text("Next face").clicked() {
                    step = Some((i + 1) % n);
                }
            });
            ui.label(if src == "linked" { "faces the game links to this head" } else { "automatic: faces of the same sex, body type and ethnicity" });
            if let Some(k) = step {
                HEAD_SKIN.store(k, std::sync::atomic::Ordering::Relaxed);
                let lod = self.loaded.as_ref().and_then(|l| lod_level(&l.lods[l.lod])).unwrap_or(0);
                let q = self.query.clone();
                self.keep_cam_once = true;
                self.open(&q, lod, ui.ctx());
            }
            ui.separator();
        }
        ui.heading("View");
        egui::ComboBox::from_label("mode").selected_text(MODES[self.look.mode as usize]).show_ui(ui, |ui| {
            for (k, m) in MODES.iter().enumerate() {
                ui.selectable_value(&mut self.look.mode, k as u32, *m);
            }
        });
        if self.look.mode == 4 {
            egui::ComboBox::from_label("slot").selected_text(SLOTS[self.look.slot as usize]).show_ui(ui, |ui| {
                for (k, m) in SLOTS.iter().enumerate() {
                    ui.selectable_value(&mut self.look.slot, k as u32, *m);
                }
            });
        }
        egui::ComboBox::from_label("packed RMA order").selected_text(RMA_ORDERS[self.look.rma_order as usize]).show_ui(ui, |ui| {
            for (k, m) in RMA_ORDERS.iter().enumerate() {
                ui.selectable_value(&mut self.look.rma_order, k as u32, *m);
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.look.wire, "wireframe");
            ui.checkbox(&mut self.look.tint, "tint").on_hover_text("Multiply the model's tint palette colour into the base colour (the game's per-model colour variation; GLB COLOR_0)");
            ui.checkbox(&mut self.show_normals, "normals");
            ui.checkbox(&mut self.decal_bias, "decal bias").on_hover_text("Draw decal-bucket materials (markings, grime, signage, terrain blend layers) after opaque surfaces with a depth bias, by layer order");
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_skeleton, "skeleton");
            ui.checkbox(&mut self.joint_labels, "labels");
            ui.checkbox(&mut self.cam.fly, "fly camera");
        });
        ui.separator();
        ui.heading("Light");
        if ui.add(egui::Slider::new(&mut self.tod, 0.0..=24.0).text("time of day (h)")).changed() {
            self.look.set_time_of_day(self.tod);
        }
        ui.add(egui::Slider::new(&mut self.look.exposure, 0.2..=4.0).logarithmic(true).text("exposure"));
        ui.add(egui::Slider::new(&mut self.look.ambient, 0.0..=2.0).text("sky / ambient"));
        ui.add(egui::Slider::new(&mut self.look.emissive, 0.0..=12.0).text("emissive"));
        ui.add(egui::Slider::new(&mut self.look.wet, 0.0..=1.0).text("wet"));
        ui.separator();
        self.vehicle_ui(ui);
        ui.heading("Car paint");
        ui.horizontal(|ui| {
            ui.label("primary");
            ui.color_edit_button_rgb(&mut self.look.paint);
            ui.label("secondary");
            ui.color_edit_button_rgb(&mut self.look.paint2nd);
            ui.label("pearl");
            ui.color_edit_button_rgb(&mut self.look.pearl);
        });
        ui.add(egui::Slider::new(&mut self.look.pearl_amount, 0.0..=1.0).text("pearl amount"));
        egui::ComboBox::from_label("finish").selected_text(FINISHES[self.look.finish as usize]).show_ui(ui, |ui| {
            for (k, m) in FINISHES.iter().enumerate() {
                ui.selectable_value(&mut self.look.finish, k as u32, *m);
            }
        });
        if let Some(l) = &self.loaded {
            if !l.model.joints.is_empty() {
                ui.separator();
                ui.heading("Rig");
                ui.add(egui::Slider::new(&mut self.wheel_spin, -3.14..=3.14).text("wheel spin"));
                ui.add(egui::Slider::new(&mut self.door_open, 0.0..=1.3).text("doors open"));
                if l.info["wheel_mounts"].is_array() {
                    ui.checkbox(&mut self.wheels_on, "fit shared wheels at the wheel mounts");
                    ui.horizontal(|ui| {
                        ui.label("rim");
                        ui.add(egui::TextEdit::singleline(&mut self.wheel_rim).desired_width(140.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("tyre");
                        ui.add(egui::TextEdit::singleline(&mut self.wheel_tyre).desired_width(140.0));
                    });
                    ui.checkbox(&mut self.wheel_auto, "size from the car's wheelScale").on_hover_text(format!("scale = {WHEEL_FACTOR:.3} x wheelScale (front / rear). The factor is fitted by eye on phoenix; the game's exact rule is not decoded."));
                    ui.add_enabled_ui(!self.wheel_auto, |ui| {
                        ui.add(egui::Slider::new(&mut self.wheel_scale, 0.2..=0.8).text("front wheel scale"));
                        ui.add(egui::Slider::new(&mut self.wheel_scale_rear, 0.2..=0.8).text("rear wheel scale"));
                    });
                }
            }
        }
        ui.separator();
        if let Some(q) = self.parts_ui(ui) {
            self.keep_cam_once = true;
            let ctx = ui.ctx().clone();
            self.open(&q, 0, &ctx);
            return;
        }
        let Some(l) = &self.loaded else { return };
        ui.heading(format!("Materials ({})", l.model.materials.len()));
        let mut toggle: Option<(usize, bool)> = None;
        let mut second: Option<(usize, bool)> = None;
        let mut solo: Option<Option<usize>> = None;
        let mut view_tex: Option<u64> = None;
        for (k, m) in l.model.materials.iter().enumerate() {
            let col = match m.gap.as_str() {
                "flat" => egui::Color32::from_rgb(230, 70, 70),
                "lowmip" => egui::Color32::from_rgb(240, 190, 40),
                "no colour texture" => egui::Color32::from_rgb(170, 170, 170),
                _ => egui::Color32::from_rgb(90, 200, 110),
            };
            let visible = self.gpu.as_ref().map(|g| g.meshes.iter().filter(|me| me.material == k).all(|me| me.visible)).unwrap_or(true);
            ui.horizontal(|ui| {
                let mut v = visible;
                if ui.checkbox(&mut v, "").changed() {
                    toggle = Some((k, v));
                }
                if m.class == "car_paint" {
                    let mut sec = self.gpu.as_ref().map(|g| Renderer3D::is_secondary(g, k)).unwrap_or(false);
                    if ui.checkbox(&mut sec, "2nd").on_hover_text("use the secondary paint colour").changed() {
                        second = Some((k, sec));
                    }
                }
                let sel = self.material_sel == Some(k);
                // "shader07_<hash>" -> "shader07 standard_vehicle" when the name is known
                let shown = match sixng_core::names::shader(&m.shader_hash) {
                    Some(n) => format!("{} {n}", m.name.split('_').next().unwrap_or("")),
                    None => m.name.clone(),
                };
                if ui.selectable_label(sel, egui::RichText::new(format!("{} · {} · {}{}", shown, m.class, m.alpha_mode, if m.gap == "no colour texture" { " · no colour texture (shown white)" } else { "" })).color(col)).clicked() {
                    self.material_sel = if sel { None } else { Some(k) };
                }
            });
            if self.material_sel == Some(k) {
                ui.indent(k, |ui| {
                    ui.label(format!("shader {} ({}) · gaps: {}", sixng_core::names::shader(&m.shader_hash).unwrap_or_else(|| "name unknown".into()), m.shader_hash, m.gap));
                    for s in &m.slots {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("{} {} {}x{} {} — {}{}", s.role, sixng_core::names::param(&s.param_hash).unwrap_or_else(|| s.param_hash.clone()), s.size[0], s.size[1], s.format, s.status, if s.mip > 0 { format!(" (mip {})", s.mip) } else { String::new() })).small()).on_hover_text(format!("parameter hash {}", s.param_hash));
                            if let Ok(h) = u64::from_str_radix(&s.hash, 16) {
                                if ui.small_button("view").on_hover_text(format!("open texture {} in the Textures tab", s.hash)).clicked() {
                                    view_tex = Some(h);
                                }
                            }
                        });
                    }
                    if ui.small_button("solo").clicked() {
                        solo = Some(Some(k));
                    }
                });
            }
        }
        if ui.small_button("show all").clicked() {
            solo = Some(None);
        }
        if view_tex.is_some() {
            self.view_texture = view_tex;
        }
        if let Some(g) = &mut self.gpu {
            if let Some((k, v)) = toggle {
                for me in g.meshes.iter_mut().filter(|me| me.material == k) {
                    me.visible = v;
                }
            }
            if let (Some((k, on)), Some(ren)) = (second, &self.renderer) {
                ren.set_material_secondary(rs, g, k, on);
            }
            if let Some(s) = solo {
                for me in g.meshes.iter_mut() {
                    me.visible = s.map(|k| me.material == k).unwrap_or(true);
                }
            }
        }
        if !l.model.joints.is_empty() {
            ui.collapsing(format!("Joints ({})", l.model.joints.len()), |ui| {
                for j in &l.model.joints {
                    ui.label(egui::RichText::new(format!("{}  ← {}", j.name, j.parent.map(|p| l.model.joints[p].name.as_str()).unwrap_or("-"))).small());
                }
            });
        }
        ui.collapsing("model.json", |ui| {
            ui.add(egui::Label::new(egui::RichText::new(serde_json::to_string_pretty(&l.info).unwrap_or_default()).monospace().small()).extend());
        });
    }
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

/// Export root of a GLB under `<root>/models/<archive>/<name>/model.glb`.
pub(crate) fn out_root_of(p: &Path) -> PathBuf {
    let mut cur = p.parent();
    while let Some(d) = cur {
        if d.file_name().map(|n| n == "models").unwrap_or(false) {
            return d.parent().unwrap_or(d).to_path_buf();
        }
        cur = d.parent();
    }
    p.parent().unwrap_or(Path::new(".")).to_path_buf()
}

/// Physical LOD of the primary model (model.glb): the lowest populated slot.
fn primary_lod(info: &serde_json::Value) -> usize {
    info["meshes"].as_array().into_iter().flatten().filter_map(|m| m["lod"].as_u64()).min().unwrap_or(0) as usize
}

/// Physical LOD number of the loaded file at `index`.
fn shown_lod(lods: &[PathBuf], index: usize, info: &serde_json::Value) -> usize {
    lods.get(index).and_then(|p| lod_level(p)).unwrap_or_else(|| primary_lod(info))
}

fn lod_level(path: &Path) -> Option<usize> {
    path.file_stem()?.to_str()?.strip_prefix("model_lod")?.parse().ok()
}

/// Choose by physical slot, falling back to the preceding populated LOD.
/// model.glb is the primary fallback, even when its physical slot is not zero.
pub(crate) fn lod_index(lods: &[PathBuf], lod: usize) -> usize {
    lods.iter().rposition(|p| lod_level(p).unwrap_or(0) <= lod).unwrap_or(0)
}

pub(crate) fn lods_in(dir: &Path) -> Vec<PathBuf> {
    let mut v = vec![];
    if dir.join("model.glb").exists() {
        v.push(dir.join("model.glb"));
        for k in 1..8 {
            let p = dir.join(format!("model_lod{k}.glb"));
            if p.exists() {
                v.push(p);
            }
        }
    }
    v
}

pub fn index_for(ws: &Workspace, archive: &str, cache: &Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>) -> anyhow::Result<(sixng_core::game::ArchiveInfo, Arc<ArchiveIndex>)> {
    let a = ws.resolve(archive)?;
    if let Some(ix) = cache.lock().unwrap().get(&a.id) {
        return Ok((a, ix.clone()));
    }
    let ix = Arc::new(ws.load_index(&a)?);
    cache.lock().unwrap().insert(a.id.clone(), ix.clone());
    Ok((a, ix))
}

pub(crate) fn load_query(ws: &Workspace, cache: &Path, q: &str, lod: usize, idx: &Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>) -> anyhow::Result<Loaded> {
    let (q, sel) = split_parts(q);
    let mut l = load_query_sel(ws, cache, q, sel, lod, idx)?;
    l.base = q.to_string();
    Ok(l)
}

/// `"<query>#parts=<hex>,<hex>"` → the query and the chosen part offsets.
fn split_parts(q: &str) -> (&str, Option<Vec<u64>>) {
    match q.trim().split_once("#parts=") {
        Some((b, l)) => (b.trim(), Some(l.split(',').filter_map(|x| u64::from_str_radix(x.trim(), 16).ok()).collect())),
        None => (q.trim(), None),
    }
}

/// Export and merge a set of parts; returns the model (None if none loaded),
/// the first part's LOD files, the loaded names and the errors. A model
/// passed in (an animal root) has its placeholder geometry replaced.
fn load_parts(a: &sixng_core::game::ArchiveInfo, parts: &[&sixng_core::scan::Entry], cache: &Path, lod: usize, mut model: Option<glb::Model>) -> (Option<glb::Model>, Vec<PathBuf>, Vec<String>, Vec<String>) {
    let mut first = Vec::new();
    let mut names = Vec::new();
    let mut errs = Vec::new();
    let mut fresh = true;
    for e in parts {
        match export_load(a, e, cache, lod) {
            Ok((m, lods, _)) => {
                if first.is_empty() {
                    first = lods;
                }
                match &mut model {
                    Some(into) if fresh => {
                        into.meshes.clear();
                        into.triangles = 0;
                        into.vertices = 0;
                        glb::merge(into, m);
                    }
                    Some(into) => glb::merge(into, m),
                    None => model = Some(m),
                }
                fresh = false;
                names.push(e.name.clone().unwrap_or_default());
            }
            Err(err) => errs.push(format!("{}: {err:#}", e.id)),
        }
    }
    (model, first, names, errs)
}

/// The chosen parts: the selection when given, else the defaults.
fn pick<'a>(ix: &'a ArchiveIndex, opts: &[crate::assemble::PartOpt], sel: &Option<Vec<u64>>) -> (Vec<u64>, Vec<&'a sixng_core::scan::Entry>) {
    let chosen: Vec<u64> = match sel {
        Some(s) => opts.iter().filter(|o| s.contains(&o.off)).map(|o| o.off).collect(),
        None => opts.iter().filter(|o| o.default).map(|o| o.off).collect(),
    };
    let parts = chosen.iter().filter_map(|off| ix.entries.iter().find(|e| e.off == *off)).collect();
    (chosen, parts)
}

fn load_query_sel(ws: &Workspace, cache: &Path, q: &str, sel: Option<Vec<u64>>, lod: usize, idx: &Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>) -> anyhow::Result<Loaded> {
    if q.to_lowercase().ends_with(".glb") {
        let p = PathBuf::from(q);
        let dir = p.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut lods = lods_in(&dir);
        if !lods.contains(&p) {
            lods = vec![p.clone()];
        }
        let lod = lods.iter().position(|x| x == &p).unwrap_or(0);
        let root = out_root_of(&p);
        let info = std::fs::read(dir.join("model.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let model = glb::load(&p, &root)?;
        return Ok(Loaded { model, lods, lod, out_root: root, source: dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), info, parts: Vec::new(), selected: Vec::new(), base: String::new() });
    }
    // character outfit: "<archive>#outfit:<prefix>"
    if let Some((archive, prefix)) = q.split_once("#outfit:") {
        let (a, ix) = index_for(ws, archive, idx)?;
        let opts = crate::assemble::outfit_options(&ix, prefix);
        if opts.is_empty() {
            return Err(anyhow::anyhow!("no parts found for {prefix}"));
        }
        let (selected, parts) = pick(&ix, &opts, &sel);
        if parts.is_empty() {
            return Err(anyhow::anyhow!("no parts of {prefix} selected"));
        }
        let (model, first, names, errs) = load_parts(&a, &parts, cache, lod, None);
        let model = model.ok_or_else(|| anyhow::anyhow!("no part of {prefix} loaded: {}", errs.join("; ")))?;
        let info = serde_json::json!({ "assembled_from": names, "ids": parts.iter().map(|e| e.id.clone()).collect::<Vec<_>>(), "errors": errs });
        return Ok(Loaded { model, lods: first.into_iter().take(1).collect(), lod: 0, out_root: cache.to_path_buf(), source: format!("{} (outfit, {} parts)", prefix, names.len()), info, parts: opts, selected, base: String::new() });
    }
    let (q, member) = q.split_once('#').map_or((q, None), |(id, member)| (id, Some(member)));
    // entry: "<archive>@<hex>" or a name regex (in levels unless "archive:regex")
    let (archive, rest) = match q.split_once('@') {
        Some((a, o)) => (a.to_string(), Some(o.to_string())),
        None => match q.split_once(':') {
            Some((a, r)) if !a.is_empty() && !a.contains('^') => (a.to_string(), None).clone_with(r),
            _ => ("levels".to_string(), None).clone_with(q),
        },
    };
    let (a, ix) = index_for(ws, &archive, idx)?;
    let e = match rest {
        Some(o) if !o.starts_with('~') => {
            let id = format!("{o}{}", member.map_or(String::new(), |m| format!("#{m}")));
            export::ModelResolver::new(&ix.entries).resolve(&ix.entries, &id)?.0
        }
        Some(re) => find_name(&ix, re.trim_start_matches('~'))?,
        None => unreachable!(),
    };
    let selected = member.map(|name| export::member_entry(e, name)).transpose()?;
    let e = selected.as_ref().unwrap_or(e);
    let (model, lods, info) = export_load(&a, e, cache, lod)?;
    let level = lod;
    let lod = lod_index(&lods, level);
    // animals: the fragment is a skeleton with a placeholder plane; add the textured parts
    let name = e.name.as_deref().unwrap_or("");
    if crate::assemble::is_animal(name) && e.rtype.as_deref() == Some("ft") {
        let opts = crate::assemble::animal_options(&ix, name);
        let (selected, parts) = pick(&ix, &opts, &sel);
        let (m, _, names, _) = load_parts(&a, &parts, cache, level, Some(model));
        let model = m.expect("root model passed in");
        let mut info = info;
        if let Some(o) = info.as_object_mut() {
            o.insert("assembled_from".into(), serde_json::json!(names));
        }
        let source = if names.is_empty() { format!("{} {}", e.id, name) } else { format!("{} {} (+{} parts)", e.id, name, names.len()) };
        let lods = if opts.is_empty() { lods } else { lods.into_iter().skip(lod).take(1).collect() };
        let lod = if opts.is_empty() { lod } else { 0 };
        return Ok(Loaded { model, lods, lod, out_root: cache.to_path_buf(), source, info, parts: opts, selected, base: String::new() });
    }
    Ok(Loaded { model, lods, lod, out_root: cache.to_path_buf(), source: format!("{} {}", e.id, name).trim().to_string(), info, parts: Vec::new(), selected: Vec::new(), base: String::new() })
}

/// Export one entry into the viewer cache and load the GLB of a LOD level.
fn export_load(a: &sixng_core::game::ArchiveInfo, e: &sixng_core::scan::Entry, cache: &Path, lod: usize) -> anyhow::Result<(glb::Model, Vec<PathBuf>, serde_json::Value)> {
    // Heads are exported again each time (small), so the chosen face variant applies.
    let head = sixng_core::headskin::is_head(e.name.as_deref().unwrap_or(""));
    let dir = cache.join("models").join(sixng_core::game::index_stem(&a.id)).join(export::export_base_name(e));
    // Cached exports from before tint palettes were applied: export again.
    let stale_tint = std::fs::read_to_string(dir.join("model.json")).is_ok_and(|j| j.contains("\"tintpalettetex\"") && !j.contains("\"tint_applied\""));
    let opts = ExportOptions { out: cache.to_path_buf(), png: true, dds: false, raw: false, force: head || stale_tint, skip_embedded_textures: false, body_shape: Vec::new(), head_skin: HEAD_SKIN.load(std::sync::atomic::Ordering::Relaxed) };
    let item = export::export_entry(a, e, &opts)?;
    let lods = lods_in(&dir);
    if lods.is_empty() {
        return Err(anyhow::anyhow!("{} exported no model.glb (status {}, files {:?}{})", e.id, item.status, item.files, item.error.map(|x| format!(", {x}")).unwrap_or_default()));
    }
    let lod = lod_index(&lods, lod);
    let info: serde_json::Value = std::fs::read(dir.join("model.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if let Some(h) = info.get("head_skin") {
        let n = h["count"].as_u64().unwrap_or(0) as usize;
        if n > 0 {
            *HEAD_SKIN_SEEN.lock().unwrap() = Some((h["index"].as_u64().unwrap_or(0) as usize, n, h["source"].as_str().unwrap_or("").to_string()));
        }
    }
    let mut model = glb::load(&lods[lod], cache)?;
    crate::assemble::mark_missing_colour(&mut model);
    Ok((model, lods, info))
}

trait CloneWith {
    fn clone_with(self, r: &str) -> (String, Option<String>);
}
impl CloneWith for (String, Option<String>) {
    fn clone_with(self, r: &str) -> (String, Option<String>) {
        (self.0, Some(format!("~{r}")))
    }
}

fn find_name<'a>(ix: &'a ArchiveIndex, re: &str) -> anyhow::Result<&'a sixng_core::scan::Entry> {
    let r = regex::RegexBuilder::new(re).case_insensitive(true).build()?;
    // prefer fragments, then drawables
    let mut best: Option<&sixng_core::scan::Entry> = None;
    for e in &ix.entries {
        let Some(n) = &e.name else { continue };
        if !r.is_match(n) {
            continue;
        }
        let rank = |e: &sixng_core::scan::Entry| match e.rtype.as_deref() {
            Some("ft") => 0,
            Some("dr") => 1,
            _ => 2,
        };
        if best.map(|b| rank(e) < rank(b)).unwrap_or(true) {
            best = Some(e);
        }
    }
    best.filter(|e| matches!(e.rtype.as_deref(), Some("ft") | Some("dr") | Some("dd"))).ok_or_else(|| anyhow::anyhow!("no drawable/fragment named like {re}"))
}

/// Same rule as sixng_core::export::sanitize (kept local: it is private there).
pub(crate) fn export_sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' { c } else { '_' }).collect()
}

/// Entry ids of the parts an animal root or a character outfit is put
/// together from (None for ordinary models).
pub(crate) fn assembly_ids(ws: &Workspace, idx: &Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>, id: &str) -> Option<Vec<String>> {
    if let Some((archive, prefix)) = id.split_once("#outfit:") {
        let (_, ix) = index_for(ws, archive, idx).ok()?;
        return Some(crate::assemble::outfit_parts(&ix, prefix).iter().map(|e| e.id.clone()).collect());
    }
    let (archive, _) = id.split_once('@')?;
    let (_, ix) = index_for(ws, archive, idx).ok()?;
    let (e, member) = export::ModelResolver::new(&ix.entries).resolve(&ix.entries, id).ok()?;
    if member.is_some() { return None; }
    let name = e.name.as_deref()?;
    if e.rtype.as_deref() == Some("ft") && crate::assemble::is_animal(name) {
        let parts = crate::assemble::animal_parts(&ix, name);
        if !parts.is_empty() {
            return Some(std::iter::once(e.id.clone()).chain(parts.iter().map(|p| p.id.clone())).collect());
        }
    }
    None
}

/// sRGB [0,1] triple as an egui colour.
fn srgb32(c: [f32; 3]) -> egui::Color32 {
    egui::Color32::from_rgb((c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_discovery_keeps_physical_numbers_across_holes() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("sixng-lod-discovery-test-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        // Slots 2 and 3 export as model.glb and model_lod3.glb.
        for name in ["model.glb", "model_lod3.glb", "model_lod5.glb"] {
            std::fs::write(dir.join(name), []).unwrap();
        }
        let lods = lods_in(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(lods, [dir.join("model.glb"), dir.join("model_lod3.glb"), dir.join("model_lod5.glb")]);
        assert_eq!(lods.iter().map(|p| lod_level(p)).collect::<Vec<_>>(), [None, Some(3), Some(5)]);
        for (level, index) in [(0, 0), (2, 0), (3, 1), (4, 1), (5, 2), (7, 2)] {
            assert_eq!(lod_index(&lods, level), index);
        }
    }
}
