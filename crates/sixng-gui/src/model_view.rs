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

pub const MODES: [&str; 8] = [
    "shaded",
    "albedo",
    "normals",
    "UV checker",
    "texture slot",
    "BCPack gaps",
    "vertex colour",
    "lighting only",
];
pub const RMA_ORDERS: [&str; 4] = [
    "R rough / G metal / B AO",
    "ORM (R AO / G rough / B metal)",
    "R metal / G rough / B AO",
    "ignore packed",
];
pub const FINISHES: [&str; 4] = ["metallic", "matte", "chrome", "gloss"];
/// `standard_vehicle_2lyr_wheel_inst`: the rim material of the shared wheels.
const RIM_SHADER: &str = "_c5cdbdd2";
/// Default rim colour: a plain alloy (the true per-vehicle colour is not decoded).
const RIM_ALLOY: [f32; 3] = [0.6, 0.6, 0.62];
pub const SLOTS: [&str; 4] = ["base colour", "normal", "packed RMA", "emissive"];

/// Which side-panel controls do something for a model.
#[derive(Clone, Copy, Default)]
struct Caps {
    paint: bool,
    wheels: bool,
    doors: bool,
    roof: bool,
    mounts: bool,
    joints: bool,
    tint: bool,
    decals: bool,
}

impl Caps {
    fn of(l: &Loaded) -> Caps {
        let m = &l.model;
        let names: Vec<String> = m.joints.iter().map(|j| j.name.to_lowercase()).collect();
        Caps {
            paint: m.materials.iter().any(|x| x.class == "car_paint"),
            wheels: names.iter().any(|n| n.starts_with("wheel_") && !n.contains("dummy")),
            doors: names.iter().any(|n| matches!(n.as_str(), "door_dside_f" | "door_dside_r" | "door_pside_f" | "door_pside_r")),
            roof: names.iter().any(|n| n == "convertible_roof"),
            mounts: l.info["wheel_mounts"].as_array().is_some_and(|a| !a.is_empty()),
            joints: !m.joints.is_empty(),
            tint: m.meshes.iter().any(|x| x.verts.iter().any(|v| v.tint != [255, 255, 255, 255] && v.tint != [0, 0, 0, 0])),
            decals: m.materials.iter().any(|x| x.decal.is_some()),
        }
    }
}

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
    /// Animals: the fur variants and their grooms (`furlink`), or why none.
    pub fur: Option<Result<sixng_core::furlink::FurLink, String>>,
    /// Animals: skin samples (RAGE model space position, linear colour) the
    /// fur takes its colour from.
    pub fur_skin: SkinColours,
}

/// A finished load, tagged with the request it answers.
type Pending = Arc<Mutex<Option<(u64, Result<Loaded, String>)>>>;

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
    /// Model to open once the first one is shown (`--then`, tests switching).
    pub then: Option<String>,
    /// `--find-hair`: fit grooms to the model and show the run with the
    /// most strands (tests "Find grooms on this model").
    pub find_hair: bool,
}

/// Shared-wheel scale per unit of the car's wheelScale. Every shared rim
/// model has radius 0.5 and wheelScale reads as the rim radius in metres
/// (0.217 on a BMX = 20 inch wheel, 0.303 on a cruiser bicycle, 0.25 / 0.22
/// on a dirt bike = 21 / 18 inch), so the rim is scaled by wheelScale / 0.5.
/// The tyre is scaled with it; the game shapes it through the wheel's own
/// bones (rim_scale, wheel_width_mover, tyre_profile), which is not decoded.
pub const WHEEL_FACTOR: f32 = 2.0;

pub struct ModelView {
    pub query: String,
    pub status: String,
    pending: Pending,
    /// Number of the newest load request: a load that finishes after a
    /// newer one was requested is dropped, so the viewport never shows an
    /// older model under the newer name.
    load_gen: Arc<std::sync::atomic::AtomicU64>,
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
    /// Convertibles: how far the roof is folded, 0 = up, 1 = stowed (the
    /// position in the car's `va_<model>/roof` clip).
    pub roof_fold: f32,
    /// Convertibles without that clip: hide the soft top.
    pub roof_down: bool,
    /// The loaded model's roof clip (model path it was read for, the clip).
    roof_anim: (PathBuf, Option<glb::Anim>),
    /// Its other clips (a sunroof) and how far each is played, 0..1.
    clips: Vec<glb::Anim>,
    clip_pos: Vec<f32>,
    /// Automation: play every other clip to this position.
    pub clip_pos_all: f32,
    /// Bonnet and boot opening (radians at the hinge).
    pub bonnet_open: f32,
    pub boot_open: f32,
    /// Hinged panels of the loaded model: joint index -> (0 = door, 1 =
    /// bonnet, 2 = boot; rotation axis; sign that swings it open).
    hinges: HashMap<usize, (u8, Vec3, f32)>,
    /// Brake disc and caliper models at the wheels.
    brakes: Vec<(bool, GpuModel)>,
    brake_pending: Arc<Mutex<Option<Vec<(bool, Loaded)>>>>,
    brake_key: String,
    brake_loading: bool,
    veh_brake: (Option<String>, Option<String>),
    pub paint_on: bool,
    /// Side-panel controls that apply to the loaded model, keyed by (GLB path, triangles).
    caps: Option<(PathBuf, usize, Caps)>,
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
    /// Rim colour (linear). The game sets it per vehicle at runtime and no
    /// decoded table holds it, so it starts as a plain alloy.
    pub rim_colour: [f32; 3],
    rim_applied: [f32; 3],
    /// Material names of the loaded wheel models (rim, tyre).
    wheel_mats: Vec<Vec<String>>,
    pub wheel_tyre: String,
    pub wheel_scale: f32,
    pub wheel_scale_rear: f32,
    /// Size the shared wheels from the car's wheelScale (WHEEL_FACTOR x).
    pub wheel_auto: bool,
    pub vehicles: crate::vehicle_db::VehicleDb,
    /// Model the vehicle data was applied to, and the chosen configuration.
    veh_applied: String,
    /// A helicopter on skids (see `vehicle_ui`): no shared wheels are fitted.
    veh_no_wheels: bool,
    /// Bounding boxes of the loaded wheel models (rim, tyre).
    wheel_boxes: Vec<[[f32; 3]; 2]>,
    /// Width factor per wheel mount (two-wheelers), and what it was made for.
    /// and the arch-fit factor on the whole wheel: [width, fit].
    wheel_xs: (String, Vec<[f32; 2]>),
    pub veh_cfg: Option<usize>,
    wheel_pending: Arc<Mutex<Option<Result<Vec<Loaded>, String>>>>,
    wheel_key: String,
    wheels: Vec<GpuModel>,
    wheel_loading: bool,
    pub picker: crate::model_picker::ModelPicker,
    /// Set when the user asks to open a material's texture in the Textures tab.
    pub view_texture: Option<u64>,
    pub show_picker: bool,
    pub hair: Hair,
}

type GroomList = Result<Vec<sixng_core::groom::GroomInfo>, String>;
/// Most strands an animal's interpolated coat is drawn with (a cat's short
/// body coat stores about 365,000). Fewer strands are drawn wider.
const COAT_STRANDS: usize = 90_000;
/// Skin samples: position (RAGE model space) and linear colour.
pub type SkinColours = Arc<Vec<([f32; 3], [u8; 3])>>;

/// Strand hair / fur shown with the model: grooms picked from the packs
/// catalog (no groom-to-ped link is stored), drawn as crossed ribbons.
pub struct Hair {
    catalog: Arc<Mutex<Option<GroomList>>>,
    list: Option<GroomList>,
    listing: bool,
    pub region: String,
    pub search: String,
    pub selected: Vec<u64>,
    pub width_mm: f32,
    pub colour: [f32; 3],
    pending: Arc<Mutex<Option<Result<Vec<glb::Model>, String>>>>,
    key: String,
    loading: bool,
    gpu: Vec<GpuModel>,
    pub status: String,
    /// Animals: the fur link of the model shown, and the body / head
    /// variants chosen from it.
    fur: Option<sixng_core::furlink::FurLink>,
    fur_note: String,
    fur_pick: [String; 2],
    /// Colour each strand from the skin under its root (automatic fur).
    pub skin_colour: bool,
    /// Draw the chosen grooms (the guide strands); off hides them without
    /// dropping the choice.
    pub show: bool,
    /// Automatic fur: grow the coat between the guides from the stored
    /// per-triangle strand counts, instead of drawing the guides alone.
    pub full_coat: bool,
    /// Share of the strand budget (`COAT_STRANDS`) to draw, 0.05-1.
    pub density: f32,
    /// What the last coat build drew (strands drawn / stored).
    coat_note: Arc<Mutex<String>>,
    /// Grooms found on the loaded model's skin ("Find grooms on this model").
    fits: Option<Result<Vec<sixng_core::furlink::OnSkin>, String>>,
    fits_pending: Arc<Mutex<Option<Result<Vec<sixng_core::furlink::OnSkin>, String>>>>,
    fitting: bool,
    skin: SkinColours,
}

impl Default for Hair {
    fn default() -> Self {
        Hair {
            catalog: Arc::new(Mutex::new(None)),
            list: None,
            listing: false,
            region: "scalp".into(),
            search: String::new(),
            selected: Vec::new(),
            width_mm: 1.0,
            colour: [0.10, 0.06, 0.035],
            pending: Arc::new(Mutex::new(None)),
            key: String::new(),
            loading: false,
            gpu: Vec::new(),
            status: String::new(),
            fur: None,
            fur_note: String::new(),
            fur_pick: [String::new(), String::new()],
            skin_colour: true,
            show: true,
            full_coat: true,
            density: 1.0,
            coat_note: Arc::new(Mutex::new(String::new())),
            fits: None,
            fits_pending: Arc::new(Mutex::new(None)),
            fitting: false,
            skin: Default::default(),
        }
    }
}

impl Hair {
    fn want_key(&self) -> String {
        format!("{:?}|{}|{:?}|{}|{}|{}", self.selected, self.width_mm, self.colour, self.skin_colour && !self.skin.is_empty(), self.full_coat, self.density)
    }
    pub fn settled(&self) -> bool {
        !self.loading && !self.listing && self.key == self.want_key()
    }

    /// Show the grooms of the chosen body and head variants.
    fn apply_fur(&mut self) {
        let Some(l) = &self.fur else { return };
        let mut sel: Vec<u64> = Vec::new();
        for g in self.fur_pick.iter().filter_map(|n| l.variants.iter().find(|v| v.name == *n)).flat_map(|v| v.grooms.iter().copied()) {
            if !sel.contains(&g) {
                sel.push(g);
            }
        }
        self.selected = sel;
    }

    /// A model finished loading: take its fur link (animals) and show the
    /// default variants, or drop the fur of the previous animal.
    fn on_loaded(&mut self, fur: &Option<Result<sixng_core::furlink::FurLink, String>>, skin: &SkinColours, source: &str) {
        match fur {
            Some(Ok(l)) => {
                self.skin = skin.clone();
                if self.fur.as_ref().is_some_and(|f| f.root == l.root) {
                    return;
                }
                // guide strands are sparse: wider ribbons read as a coat
                if self.width_mm == 1.0 {
                    self.width_mm = 3.0;
                }
                self.fur_pick = ["body", "head"].map(|s| l.default_variant(s).map(|v| v.name.clone()).unwrap_or_default());
                self.fur_note.clear();
                self.fur = Some(l.clone());
                self.apply_fur();
            }
            other => {
                // a LOD or part change of the same animal keeps its fur
                if self.fur.as_ref().is_some_and(|f| !source.contains(&f.root)) {
                    self.fur = None;
                    self.selected.clear();
                    self.skin = Default::default();
                    if self.width_mm == 3.0 {
                        self.width_mm = 1.0;
                    }
                }
                if self.fur.is_none() {
                    self.fur_note = match other {
                        Some(Err(e)) => e.clone(),
                        _ => String::new(),
                    };
                }
            }
        }
    }
}

impl ModelView {
    pub fn new(game: &str, work: &str) -> Self {
        ModelView {
            query: String::new(),
            status:
                "Type an entry id (levels.rpf@219c8b010), a name regex (^phoenix) or open a .glb."
                    .into(),
            pending: Arc::new(Mutex::new(None)),
            load_gen: Arc::new(std::sync::atomic::AtomicU64::new(0)),
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
            roof_fold: 0.0,
            roof_down: false,
            roof_anim: (PathBuf::new(), None),
            clips: Vec::new(),
            clip_pos: Vec::new(),
            clip_pos_all: 0.0,
            bonnet_open: 0.0,
            boot_open: 0.0,
            hinges: HashMap::new(),
            brakes: Vec::new(),
            brake_pending: Arc::new(Mutex::new(None)),
            brake_key: String::new(),
            brake_loading: false,
            veh_brake: (None, None),
            paint_on: true,
            caps: None,
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
            rim_colour: RIM_ALLOY,
            rim_applied: RIM_ALLOY,
            wheel_mats: Vec::new(),
            wheel_tyre: "tyre_muscle_001_hi".into(),
            wheel_scale: 0.37,
            wheel_scale_rear: 0.37,
            wheel_auto: true,
            vehicles: crate::vehicle_db::VehicleDb::new(),
            veh_applied: String::new(),
            veh_no_wheels: false,
            wheel_boxes: Vec::new(),
            wheel_xs: (String::new(), Vec::new()),
            veh_cfg: None,
            wheel_pending: Arc::new(Mutex::new(None)),
            wheel_key: String::new(),
            wheels: Vec::new(),
            wheel_loading: false,
            picker: crate::model_picker::ModelPicker::new(work),
            view_texture: None,
            show_picker: true,
            hair: Hair::default(),
        }
    }

    /// Nothing loading (model or picker list): for automation screenshots.
    pub fn idle(&self) -> bool {
        !self.loading && self.idle_for_shot() && (!self.show_picker || self.picker.loaded())
    }

    /// Start reading the groom catalog (cached in <work>/index after the
    /// first time).
    fn start_hair_list(&mut self, ctx: &egui::Context) {
        if self.hair.listing || self.hair.list.is_some() {
            return;
        }
        self.hair.listing = true;
        let ws = Workspace::new(&self.game, &self.work);
        let idx = self.indexes.clone();
        let slot = self.hair.catalog.clone();
        let work = PathBuf::from(&self.work);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = index_for(&ws, "packs", &idx)
                .and_then(|(a, ix)| sixng_core::groom::catalog_cached(&work, &a, &ix.entries))
                .map_err(|e| format!("{e:#}"));
            *slot.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    /// Load the chosen grooms when the choice, width or colour changed, and
    /// upload finished loads.
    fn update_hair(&mut self, rs: &egui_wgpu::RenderState, ctx: &egui::Context) {
        if let Some(r) = self.hair.catalog.lock().unwrap().take() {
            self.hair.listing = false;
            self.hair.list = Some(r);
        }
        if let Some(r) = self.hair.pending.lock().unwrap().take() {
            self.hair.loading = false;
            match r {
                Ok(models) => {
                    let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
                    self.hair.gpu = models.iter().map(|m| ren.upload(rs, m, &[])).collect();
                    self.hair.status = format!("{} groom(s) shown", models.len());
                    if std::env::var_os("SIXNG_DEBUG_FUR").is_some() {
                        eprintln!("[hair] {} groom(s), {} triangles, selected {:x?}", models.len(), models.iter().map(|m| m.triangles).sum::<usize>(), self.hair.selected);
                    }
                }
                Err(e) => {
                    eprintln!("[hair] {e}");
                    self.hair.gpu.clear();
                    self.hair.status = e;
                }
            }
        }
        let want = self.hair.want_key();
        if self.hair.loading || self.hair.key == want {
            return;
        }
        self.hair.key = want;
        if self.hair.selected.is_empty() {
            self.hair.gpu.clear();
            self.hair.status.clear();
            return;
        }
        self.hair.loading = true;
        if std::env::var_os("SIXNG_DEBUG_FUR").is_some() {
            eprintln!("[hair] loading {} groom(s)", self.hair.selected.len());
        }
        let ws = Workspace::new(&self.game, &self.work);
        let idx = self.indexes.clone();
        let slot = self.hair.pending.clone();
        let dir = self.cache_dir().join("grooms");
        let (sel, hw, rgb) = (
            self.hair.selected.clone(),
            self.hair.width_mm / 1000.0,
            self.hair.colour,
        );
        let skin = if self.hair.skin_colour { self.hair.skin.clone() } else { Default::default() };
        // the coat: automatic fur (coloured from the skin) and hand-picked
        // grooms (one colour)
        let coat_on = self.hair.full_coat;
        let (density, width_scale) = (self.hair.density.clamp(0.05, 1.0), self.hair.width_mm / 3.0);
        let note = self.hair.coat_note.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = (|| -> anyhow::Result<Vec<glb::Model>> {
                let (a, ix) = index_for(&ws, "packs", &idx)?;
                std::fs::create_dir_all(&dir)?;
                let grooms: Vec<(u64, sixng_core::groom::Groom)> = sel.iter().map(|&off| sixng_core::groom::load(&a, &ix.entries, off).map(|g| (off, g))).collect::<anyhow::Result<_>>()?;
                let stored: usize = grooms.iter().map(|g| g.1.stored_strands()).sum();
                let share = density * (COAT_STRANDS as f32 / stored.max(1) as f32).min(1.0);
                let mut drawn = 0;
                let mut out = Vec::new();
                for (off, g) in grooms {
                    // linear colour from the sRGB picker value
                    let lin = rgb.map(|c| c.powf(2.2));
                    let p = dir.join(format!("{off:x}.glb"));
                    let nearest = |r: [f32; 3], skin: &[([f32; 3], [u8; 3])]| {
                        let d = |p: &[f32; 3]| (p[0] - r[0]).powi(2) + (p[1] - r[1]).powi(2) + (p[2] - r[2]).powi(2);
                        skin.iter().min_by(|a, b| d(&a.0).total_cmp(&d(&b.0))).map_or([255; 3], |x| x.1)
                    };
                    let glb_bytes = if coat_on && g.stored_strands() > 0 {
                        let k = g.points_per_strand.max(1);
                        let one = lin.map(|c| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
                        let cols: Vec<[u8; 3]> = (0..g.strands).map(|s| if skin.is_empty() { one } else { nearest(g.points[s * k], &skin) }).collect();
                        let coat = g.coat(share);
                        drawn += coat.guides.len();
                        // fewer strands than stored: wider ones, for the same cover
                        let base = if g.width > 0.0 { g.width } else { 0.0003 };
                        let w = (base / share.max(1e-3)).clamp(base, 0.004) * width_scale;
                        g.coat_glb(&format!("coat_{off:x}"), &coat, w, &cols, serde_json::json!({}))
                    } else if skin.is_empty() {
                        g.ribbon_glb(&format!("groom_{off:x}"), hw, lin, serde_json::json!({}))
                    } else {
                        // each strand takes the colour of the nearest skin sample to its root
                        let k = g.points_per_strand.max(1);
                        let cols: Vec<[u8; 3]> = (0..g.strands)
                            .map(|s| {
                                let r = g.points[s * k];
                                let d = |p: &[f32; 3]| (p[0] - r[0]).powi(2) + (p[1] - r[1]).powi(2) + (p[2] - r[2]).powi(2);
                                skin.iter().min_by(|a, b| d(&a.0).total_cmp(&d(&b.0))).map_or([255; 3], |x| x.1)
                            })
                            .collect();
                        g.ribbon_glb_coloured(&format!("groom_{off:x}"), hw, &cols, serde_json::json!({}))
                    };
                    std::fs::write(&p, glb_bytes)?;
                    out.push(glb::load(&p, &dir)?);
                }
                *note.lock().unwrap() = if coat_on && stored > 0 { format!("coat: {drawn} of the {stored} stored strands drawn") } else { String::new() };
                Ok(out)
            })()
            .map_err(|e| format!("hair: {e:#}"));
            *slot.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    /// "Find grooms on this model": for people, whose hair has no decoded
    /// link. Lists the groom pieces whose roots lie on the loaded mesh, in
    /// archive order with a gap between runs (the pieces of one style are
    /// neighbours), to tick on and off.
    fn hair_fit_ui(&mut self, ui: &mut egui::Ui) {
        if let Some(r) = self.hair.fits_pending.lock().unwrap().take() {
            self.hair.fitting = false;
            self.hair.fits = Some(r);
        }
        if self.hair.fur.is_some() || self.loaded.is_none() {
            return;
        }
        // automation: start the fit, then show the run with the most strands
        let auto_start = self.auto.find_hair && !self.hair.fitting && self.hair.fits.is_none();
        if let (true, Some(Ok(list))) = (self.auto.find_hair, &self.hair.fits) {
            let mut runs: Vec<Vec<&sixng_core::furlink::OnSkin>> = Vec::new();
            for g in list {
                match runs.last_mut() {
                    Some(r) if r.last().is_some_and(|p| g.piece == p.piece + 1) => r.push(g),
                    _ => runs.push(vec![g]),
                }
            }
            if let Some(r) = runs.iter().filter(|r| r.len() <= 8).max_by_key(|r| r.iter().map(|g| g.strands).sum::<u32>()) {
                self.hair.selected = r.iter().map(|g| g.off).collect();
                // let the screenshot wait for these grooms
                self.frames_since_load = 0;
                if std::env::var_os("SIXNG_DEBUG_FUR").is_some() {
                    eprintln!("[hair] picked run #{} with {} piece(s)", r[0].piece, r.len());
                }
            }
            self.auto.find_hair = false;
        } else if matches!(self.hair.fits, Some(Err(_))) {
            self.auto.find_hair = false;
        }
        if self.hair.fitting {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("fitting grooms to the model …");
            });
        } else if ui.button("Find grooms on this model").on_hover_text("Lists the grooms whose strand roots lie on this mesh (a head, or a whole character). Hair made for this head shape fits; which style a character wears is not decoded.").clicked() || auto_start {
            let pts: Vec<[f32; 3]> = self
                .loaded
                .as_ref()
                .map(|l| {
                    let p: Vec<[f32; 3]> = l.model.meshes.iter().flat_map(|me| sixng_core::furlink::skin_points(&me.verts.iter().map(|v| [v.pos[0], -v.pos[2], v.pos[1]]).collect::<Vec<_>>(), &me.indices)).collect();
                    let step = (p.len() / 60_000).max(1);
                    p.into_iter().step_by(step).collect()
                })
                .unwrap_or_default();
            self.hair.fitting = true;
            let (ws, idx, slot, work, ctx) = (Workspace::new(&self.game, &self.work), self.indexes.clone(), self.hair.fits_pending.clone(), PathBuf::from(&self.work), ui.ctx().clone());
            std::thread::spawn(move || {
                let r = index_for(&ws, "packs", &idx).and_then(|(a, ix)| sixng_core::furlink::on_skin(&work, &a, &ix.entries, &pts)).map_err(|e| format!("{e:#}"));
                *slot.lock().unwrap() = Some(r);
                ctx.request_repaint();
            });
        }
        match &self.hair.fits {
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).weak());
            }
            Some(Ok(list)) if list.is_empty() => {
                ui.label(egui::RichText::new("No groom lies on this mesh.").weak());
            }
            Some(Ok(list)) => {
                // runs of neighbouring pieces: one row each
                let mut runs: Vec<Vec<sixng_core::furlink::OnSkin>> = Vec::new();
                for g in list {
                    match runs.last_mut() {
                        Some(r) if r.last().is_some_and(|p| g.piece == p.piece + 1) => r.push(g.clone()),
                        _ => runs.push(vec![g.clone()]),
                    }
                }
                ui.label(egui::RichText::new(format!("{} groom pieces in {} runs lie on this mesh. The pieces of one style are neighbours, but styles made for the same head follow each other without a marker: a short run is one style, a long run is many (open it and tick pieces).", list.len(), runs.len())).weak().small());
                egui::ScrollArea::vertical().id_salt("hair_fits").max_height(260.0).show(ui, |ui| {
                    for r in &runs {
                        let offs: Vec<u64> = r.iter().map(|g| g.off).collect();
                        let n_on = offs.iter().filter(|o| self.hair.selected.contains(o)).count();
                        let strands: u32 = r.iter().map(|g| g.strands).sum();
                        let longest = r.iter().map(|g| g.length_mm).fold(0.0, f32::max);
                        let label = format!("#{}{} · {} piece(s) · {} strands · up to {:.0} mm", r[0].piece, if r.len() > 1 { format!("-{}", r[r.len() - 1].piece) } else { String::new() }, r.len(), strands, longest);
                        if r.len() <= 8 {
                            let mut on = n_on == offs.len();
                            if ui.checkbox(&mut on, label).on_hover_text(format!("packs.rpf@{:x} onwards", r[0].off)).changed() {
                                self.hair.selected.retain(|x| !offs.contains(x));
                                if on {
                                    self.hair.selected.extend(offs);
                                }
                            }
                        } else {
                            // many styles in a row: tick pieces one by one
                            egui::CollapsingHeader::new(format!("{label}{}", if n_on > 0 { format!(" · {n_on} shown") } else { String::new() })).id_salt(("hair_run", r[0].piece)).show(ui, |ui| {
                                for g in r {
                                    let mut on = self.hair.selected.contains(&g.off);
                                    if ui.checkbox(&mut on, format!("#{} · {} strands · {:.0} mm", g.piece, g.strands, g.length_mm)).on_hover_text(format!("packs.rpf@{:x}", g.off)).changed() {
                                        if on {
                                            self.hair.selected.push(g.off);
                                        } else {
                                            self.hair.selected.retain(|x| *x != g.off);
                                        }
                                    }
                                }
                            });
                        }
                    }
                });
            }
            None => {}
        }
    }

    /// Hair and fur panel: browse the grooms by region and bone, add them
    /// to the view, set width and colour.
    fn hair_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Hair and fur (grooms)").default_open(!self.hair.selected.is_empty() || self.auto.find_hair).show(ui, |ui| {
            ui.label(egui::RichText::new("Strand grooms from packs.rpf. Animals get their fur automatically (worked out from the order of the files and a fit to the skin, not from a stored link); for characters pick grooms here. Shown as ribbons; width and colour are display choices.").weak());
            self.hair_fit_ui(ui);
            if let Some(l) = self.hair.fur.clone() {
                let mut changed = false;
                for (k, slot) in ["body", "head"].iter().enumerate() {
                    let opts: Vec<&sixng_core::furlink::Variant> = l.variants.iter().filter(|v| v.slot == *slot && !v.grooms.is_empty()).collect();
                    if opts.is_empty() {
                        continue;
                    }
                    let short = |n: &str| n.split_once("_fur_").map_or(n.to_string(), |x| x.1.to_string());
                    ui.horizontal(|ui| {
                        ui.label(format!("{slot} fur"));
                        egui::ComboBox::from_id_salt(("fur_variant", k)).selected_text(if self.hair.fur_pick[k].is_empty() { "none".to_string() } else { short(&self.hair.fur_pick[k]) }).show_ui(ui, |ui| {
                            changed |= ui.selectable_value(&mut self.hair.fur_pick[k], String::new(), "none").changed();
                            for v in &opts {
                                let label = format!("{} · {} piece(s){}", short(&v.name), v.grooms.len(), if v.known { "" } else { " · count inferred" });
                                changed |= ui.selectable_value(&mut self.hair.fur_pick[k], v.name.clone(), label).changed();
                            }
                        });
                    });
                }
                if changed {
                    self.hair.apply_fur();
                }
                let with = if l.roots.len() > 1 { format!("; walked together with {}", l.roots.iter().filter(|r| **r != l.root).cloned().collect::<Vec<_>>().join(", ")) } else { String::new() };
                ui.label(egui::RichText::new(format!("{} of {} groom pieces in this run fit the skin; {:.0} % of the pieces with a known count sit on their own slot{with}.", l.fitting, l.block, l.agreement * 100.0)).weak().small());
            } else if !self.hair.fur_note.is_empty() {
                ui.label(egui::RichText::new(format!("No automatic fur: {}", self.hair.fur_note)).weak().small());
            }
            let Some(list) = &self.hair.list else {
                if self.hair.listing {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("reading the groom list (first time about 2 minutes) …");
                    });
                } else if ui.button("Load the groom list").clicked() {
                    let ctx = ui.ctx().clone();
                    self.start_hair_list(&ctx);
                }
                if !self.hair.status.is_empty() {
                    ui.label(egui::RichText::new(&self.hair.status).weak());
                }
                return;
            };
            let list = match list {
                Ok(l) => l.clone(),
                Err(e) => {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                    return;
                }
            };
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("hair_region").selected_text(self.hair.region.clone()).show_ui(ui, |ui| {
                    for r in ["scalp", "face", "brows / lashes", "body", "animal", "other", "all"] {
                        ui.selectable_value(&mut self.hair.region, r.to_string(), r);
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut self.hair.search).hint_text("bone or offset").desired_width(120.0));
            });
            let q = self.hair.search.to_lowercase();
            let rows: Vec<&sixng_core::groom::GroomInfo> = list
                .iter()
                .filter(|g| g.lod == 0 && (self.hair.region == "all" || g.category == self.hair.region))
                .filter(|g| q.is_empty() || format!("{:x}", g.off).contains(&q) || g.bones.iter().any(|b| b.to_lowercase().contains(&q)))
                .collect();
            ui.label(format!("{} grooms (densest LOD of each chain)", rows.len()));
            egui::ScrollArea::vertical().id_salt("hair_list").max_height(180.0).show_rows(ui, 18.0, rows.len(), |ui, range| {
                for g in &rows[range] {
                    let on = self.hair.selected.contains(&g.off);
                    let text = format!("{:x} · {} strands · {}", g.off, g.strands, g.bones.iter().take(3).cloned().collect::<Vec<_>>().join(", "));
                    if ui.selectable_label(on, text).on_hover_text(g.bones.join(", ")).clicked() {
                        if on {
                            self.hair.selected.retain(|x| *x != g.off);
                        } else {
                            self.hair.selected.push(g.off);
                        }
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.hair.width_mm, 0.2..=5.0).text("width (mm)"));
                ui.color_edit_button_rgb(&mut self.hair.colour);
                {
                    ui.checkbox(&mut self.hair.full_coat, "full coat").on_hover_text("Grow the coat between the guide strands: each root triangle gets the strand count the groom stores for it, blended from its three guides. Placement, clumping and curl are not the game's (not decoded), and the strands drawn are capped and widened to match.");
                    if self.hair.full_coat {
                        ui.add(egui::Slider::new(&mut self.hair.density, 0.05..=1.0).text("density"));
                    }
                }
                if !self.hair.skin.is_empty() {
                    ui.checkbox(&mut self.hair.skin_colour, "colour from the skin").on_hover_text("Each strand takes the colour of the skin texture under its root. A display choice: the game colours fur with its own textures and tints.");
                }
                if ui.button("Clear").clicked() {
                    self.hair.selected.clear();
                }
            });
            if self.hair.loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("loading grooms …");
                });
            } else if !self.hair.status.is_empty() {
                let note = self.hair.coat_note.lock().unwrap().clone();
                ui.label(egui::RichText::new(if note.is_empty() { self.hair.status.clone() } else { format!("{} · {note}", self.hair.status) }).weak());
            }
        });
    }

    /// Car extras settled: vehicle data applied and the fitted wheels loaded.
    fn idle_for_shot(&self) -> bool {
        if !self.hair.settled() || self.auto.then.is_some() || self.auto.find_hair || self.hair.fitting {
            return false;
        }
        let Some(l) = self
            .loaded
            .as_ref()
            .filter(|l| l.info["wheel_mounts"].as_array().is_some_and(|a| !a.is_empty()))
        else {
            return true;
        };
        let veh = match &*self.vehicles.state.lock().unwrap() {
            crate::vehicle_db::State::Idle | crate::vehicle_db::State::Loading(_) => false,
            crate::vehicle_db::State::Failed(_) => true,
            crate::vehicle_db::State::Ready(db) => {
                let key = crate::vehicle_db::model_key(&l.source);
                crate::vehicle_db::find(db, &key).is_none() || self.veh_applied == key
            }
        };
        let wheels = !self.wheels_on
            || self.veh_no_wheels
            || (!self.wheel_loading
                && self.wheel_key == format!("{}|{}", self.wheel_rim, self.wheel_tyre));
        veh && wheels && !self.brake_loading
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
        let (gen, latest) = self.next_load();
        let idx = self.indexes.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = load_query(&ws, &cache, &q, lod, &idx).map_err(|e| format!("{e:#}"));
            finish_load(&slot, &latest, gen, r);
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
        Some(
            root.into_iter()
                .chain(
                    l.parts
                        .iter()
                        .filter(|o| l.selected.contains(&o.off))
                        .map(|o| o.id.clone()),
                )
                .collect(),
        )
    }

    /// Parts panel for animals and characters: one choice (or none) per
    /// group, plus buttons for the named alternate sets. Returns the query to
    /// reload with when the choice changes.
    fn parts_ui(&self, ui: &mut egui::Ui) -> Option<String> {
        let l = self.loaded.as_ref()?;
        if l.parts.is_empty() {
            return None;
        }
        let mut groups: std::collections::BTreeMap<&str, Vec<&crate::assemble::PartOpt>> =
            Default::default();
        for o in &l.parts {
            groups.entry(o.group.as_str()).or_default().push(o);
        }
        let mut sel = l.selected.clone();
        let mut changed = false;
        ui.heading(format!(
            "Parts ({} shown of {})",
            l.selected.len(),
            l.parts.len()
        ));
        ui.label(
            egui::RichText::new("One choice per group. Accessories and alternate sets start off.")
                .weak(),
        );
        egui::Grid::new("parts_grid")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (g, opts) in &groups {
                    let cur = opts.iter().find(|o| l.selected.contains(&o.off));
                    ui.label(*g);
                    let text = cur
                        .map(|o| o.label.clone())
                        .unwrap_or_else(|| "none".into());
                    egui::ComboBox::from_id_salt(("part", *g))
                        .selected_text(text)
                        .width(240.0)
                        .show_ui(ui, |ui| {
                            let mut set = |keep: Option<u64>| {
                                sel.retain(|x| !opts.iter().any(|o| o.off == *x));
                                sel.extend(keep);
                                changed = true;
                            };
                            if ui.selectable_label(cur.is_none(), "none").clicked() && cur.is_some()
                            {
                                set(None);
                            }
                            for o in opts {
                                let mut label = o.label.clone();
                                if o.default && o.label != "default" {
                                    label += "  (default)";
                                }
                                let on = cur.map(|c| c.off) == Some(o.off);
                                if ui
                                    .selectable_label(on, label)
                                    .on_hover_text(&o.name)
                                    .clicked()
                                    && !on
                                {
                                    set(Some(o.off));
                                }
                            }
                        });
                    ui.end_row();
                }
            });
        // named sets that span several groups (`08 collarharness`: body + harness)
        let mut sets: Vec<&str> = l
            .parts
            .iter()
            .filter(|o| {
                o.label != "default"
                    && o.label.contains(' ')
                    && l.parts.iter().filter(|p| p.label == o.label).count() > 1
            })
            .map(|o| o.label.as_str())
            .collect();
        sets.sort();
        sets.dedup();
        ui.horizontal_wrapped(|ui| {
            for s in &sets {
                if ui
                    .button(format!("set {s}"))
                    .on_hover_text("Switch every group that has this set to it")
                    .clicked()
                {
                    for opts in groups.values() {
                        if let Some(o) = opts.iter().find(|o| o.label == *s) {
                            sel.retain(|x| !opts.iter().any(|p| p.off == *x));
                            sel.push(o.off);
                        }
                    }
                    changed = true;
                }
            }
            if ui
                .button("Defaults")
                .on_hover_text("Back to the default parts")
                .clicked()
            {
                sel = l
                    .parts
                    .iter()
                    .filter(|o| o.default)
                    .map(|o| o.off)
                    .collect();
                changed = true;
            }
        });
        ui.separator();
        changed.then(|| {
            format!(
                "{}#parts={}",
                l.base,
                sel.iter()
                    .map(|o| format!("{o:x}"))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
    }

    pub fn open_lod(&mut self, lod: usize, ctx: &egui::Context) {
        let Some(l) = &self.loaded else { return };
        let Some(p) = l.lods.get(lod).cloned() else {
            return;
        };
        let root = l.out_root.clone();
        let lods = l.lods.clone();
        let source = l.source.clone();
        let info = l.info.clone();
        let (parts, selected, base) = (l.parts.clone(), l.selected.clone(), l.base.clone());
        self.loading = true;
        let slot = self.pending.clone();
        let (gen, latest) = self.next_load();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = glb::load(&p, &root)
                .map(|model| Loaded {
                    model,
                    lods,
                    lod,
                    out_root: root,
                    source,
                    info,
                    parts,
                    selected,
                    base,
                    fur: None,
                    fur_skin: Default::default(),
                })
                .map_err(|e| format!("{e:#}"));
            finish_load(&slot, &latest, gen, r);
            ctx.request_repaint();
        });
    }

    /// Start a new load request: its number and the shared newest number.
    fn next_load(&self) -> (u64, Arc<std::sync::atomic::AtomicU64>) {
        let gen = self.load_gen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        (gen, self.load_gen.clone())
    }

    fn poll(&mut self, rs: &egui_wgpu::RenderState) {
        let Some((gen, r)) = self.pending.lock().unwrap().take() else {
            return;
        };
        if gen != self.load_gen.load(std::sync::atomic::Ordering::SeqCst) {
            // answer to an older request; the newest one is still loading
            return;
        }
        self.loading = false;
        match r {
            Ok(l) => {
                let keep_cam = std::mem::take(&mut self.keep_cam_once)
                    || self
                        .loaded
                        .as_ref()
                        .map(|o| o.source == l.source)
                        .unwrap_or(false);
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
                let missing = l
                    .model
                    .materials
                    .iter()
                    .filter(|m| m.gap == "no colour texture")
                    .count();
                if missing > 0 {
                    self.status += &format!(
                        " · {missing} of {} materials have no colour texture (not decodable or not in the game files; shown plain white, not guessed)",
                        l.model.materials.len()
                    );
                }
                self.gpu = Some(g);
                self.hair.on_loaded(&l.fur, &l.fur_skin, &l.source);
                self.loaded = Some(l);
                self.frames_since_load = 0;
                self.apply_paint(rs);
            }
            Err(e) => {
                // do not keep showing the previous model under the new name
                self.gpu = None;
                self.loaded = None;
                self.status = format!("error: {e}");
            }
        }
    }

    fn apply_paint(&mut self, _rs: &egui_wgpu::RenderState) {}

    /// Decoded vehicle data for the loaded car: spawn configurations (body
    /// colour + rim + tyre), paint palettes, wheelScale. The first
    /// configuration is applied when a car opens.
    fn vehicle_ui(&mut self, ui: &mut egui::Ui) {
        let Some(l) = &self.loaded else { return };
        if !l.info["wheel_mounts"].as_array().is_some_and(|a| !a.is_empty()) {
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
        let Some(v) = crate::vehicle_db::find(&db, &key) else {
            // No wheelScale to go by: plain car wheels, sized so the tyre
            // reaches 5 cm below the lowest point of the body (an estimate).
            if self.veh_applied != key {
                let floor = l.model.bbox[0][1];
                let axle = self.mounts().iter().map(|m| m.1.y).fold(f32::MAX, f32::min);
                self.veh_applied = key.clone();
                self.veh_cfg = None;
                self.veh_no_wheels = false;
                if self.wheel_auto && axle < f32::MAX {
                    // tyre_car_001 has radius 0.79 at unit scale
                    let sc = ((axle - floor + 0.05) / 0.79).clamp(0.1, 1.2);
                    let (rim, tyre) = self.plain_wheels();
                    self.wheel_rim = rim.into();
                    self.wheel_tyre = tyre.into();
                    self.veh_brake = (None, None);
                    self.wheel_scale = sc;
                    self.wheel_scale_rear = sc;
                }
            }
            ui.label(egui::RichText::new(format!("no vehicle data for '{key}': wheel size estimated from the model")).weak());
            ui.separator();
            return;
        };
        if self.veh_applied != key {
            self.veh_applied = key.clone();
            // helicopters: no configuration names a rim and wheelScale is a
            // token value (0.05 to 0.08); their wheel bones mark the skids
            // (an electric scooter has the same token wheelScale and no rim,
            // but its wheels have collision discs)
            self.veh_no_wheels = !v.configs.is_empty()
                && v.configs.iter().all(|c| c.rim.is_none())
                && v.wheel_scale.is_some_and(|w| w < 0.1)
                && self.discs().iter().all(|d| d.is_none());
            self.veh_cfg = None;
            if !v.configs.is_empty() {
                self.apply_config(v, 0);
            } else {
                // no configuration names a rim or tyre: plain ones of a kind
                // that suits the stored tyre size, not those of the vehicle
                // opened before
                let (rim, tyre) = self.plain_wheels();
                self.wheel_rim = rim.into();
                self.wheel_tyre = tyre.into();
                self.veh_brake = (None, None);
                self.apply_scale(v);
            }
        }
        ui.heading(format!("Vehicle: {key}"));
        let ws = |x: Option<f32>| x.map(|x| format!("{x:.4}")).unwrap_or_else(|| "?".into());
        ui.label(format!(
            "wheelScale {} / rear {}",
            ws(v.wheel_scale),
            ws(v.wheel_scale_rear)
        ));
        if !v.configs.is_empty() {
            let label = |i: usize| {
                let c = &v.configs[i];
                format!(
                    "{} · {} · {}",
                    i + 1,
                    c.rim.as_deref().unwrap_or("no rim"),
                    c.tyre.as_deref().unwrap_or("no tyre")
                )
            };
            let mut pick = None;
            egui::ComboBox::from_label(format!("config ({})", v.configs.len()))
                .selected_text(self.veh_cfg.map(label).unwrap_or_else(|| "custom".into()))
                .width(220.0)
                .show_ui(ui, |ui| {
                    for i in 0..v.configs.len() {
                        ui.horizontal(|ui| {
                            if let Some(c) = v.configs[i].colour {
                                let (r, _) = ui.allocate_exact_size(
                                    egui::vec2(12.0, 12.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(r, 2.0, srgb32(c));
                            }
                            if ui
                                .selectable_label(self.veh_cfg == Some(i), label(i))
                                .clicked()
                            {
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
                    let (r, resp) =
                        ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
                    ui.painter().rect_filled(r, 2.0, srgb32(*c));
                    ui.painter().rect_stroke(
                        r,
                        2.0,
                        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(90)),
                        egui::StrokeKind::Inside,
                    );
                    let col = srgb32(*c);
                    if resp
                        .on_hover_text(format!(
                            "#{:02x}{:02x}{:02x} · {:.1} %",
                            col.r(),
                            col.g(),
                            col.b(),
                            w * 100.0
                        ))
                        .clicked()
                    {
                        self.look.paint = *c;
                        self.veh_cfg = None;
                    }
                }
            });
        }
        if v.palettes.len() > 4 {
            ui.label(
                egui::RichText::new(format!(
                    "{} more palettes in vehicles.json",
                    v.palettes.len() - 4
                ))
                .weak()
                .small(),
            );
        }
        ui.label(egui::RichText::new("Colours read as linear, shown in sRGB (unverified).").weak().small()).on_hover_text("Spawn configurations each set a body colour, rim and tyre; the first is applied when a car opens. Stored colours are all dark, so they are taken as linear values; not checked in game.");
        ui.separator();
    }

    /// A plain rim and tyre for a vehicle whose data names none, by the
    /// largest stored tyre radius: tractor-sized over 0.7 m, truck-sized over
    /// 0.45 m, else a car's.
    fn plain_wheels(&self) -> (&'static str, &'static str) {
        let r = self.discs().iter().flatten().map(|d| d[0]).fold(0.0f32, f32::max);
        if r > 0.7 {
            ("rim_industrial_020_hi", "tyre_tractor_001_hi")
        } else if r > 0.45 {
            ("rim_truck_003_hi", "tyre_truck_001_hi")
        } else {
            ("rim_muscle_001_hi", "tyre_car_001_hi")
        }
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
        // A configuration without a tyre takes the tyre of another
        // configuration of the same vehicle, else a plain one of the rim's kind
        // (not the tyre of whatever vehicle was open before).
        let kind = |r: &str| {
            if r.starts_with("rim_motorbike_bicycle") {
                "tyre_bicycle_hybrid_001"
            } else if r.starts_with("rim_motorbike") {
                "tyre_motorbike_001"
            } else if r.starts_with("rim_industrial") || r.starts_with("rim_truck") {
                "tyre_truck_001"
            } else if r.starts_with("rim_train") || r.starts_with("rim_metrotrain") {
                // a rail wheel: no tyre
                ""
            } else {
                "tyre_car_001"
            }
        };
        let tyre = c
            .tyre
            .clone()
            .or_else(|| v.configs.iter().find_map(|x| x.tyre.clone()))
            .or_else(|| c.rim.as_deref().map(|r| kind(r).to_string()));
        if let Some(t) = &tyre {
            self.wheel_tyre = if t.is_empty() { String::new() } else { format!("{t}_hi") };
        }
        // a configuration with a tyre and no rim (an electric scooter): a
        // rim of the tyre's kind, not the one of the vehicle opened before
        if c.rim.is_none() {
            self.wheel_rim = match tyre.as_deref() {
                Some(t) if t.starts_with("tyre_bicycle") => "rim_motorbike_bicycle_001_hi",
                Some(t) if t.starts_with("tyre_motorbike") => "rim_motorbike_sports_001_hi",
                _ => "rim_muscle_001_hi",
            }
            .into();
        }
        // like the tyre: a configuration without brake parts takes those of
        // another configuration of the vehicle
        self.veh_brake = (
            c.brake_disc.clone().or_else(|| v.configs.iter().find_map(|x| x.brake_disc.clone())),
            c.caliper.clone().or_else(|| v.configs.iter().find_map(|x| x.caliper.clone())),
        );
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
                        // the stored wheel disc's centre where there is one
                        let p: [f32; 3] = serde_json::from_value(m["centre"].clone())
                            .ok()
                            .or_else(|| serde_json::from_value(m["pivot"].clone()).ok())?;
                        Some((
                            m["bone"].as_str()?.to_string(),
                            Vec3::new(p[0], p[2], -p[1]),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Brake discs and calipers. A configuration names a `brakedisc_*` and a
    /// `caliper_*` model; both are modelled around a disc of radius 0.5 and
    /// are scaled to the wheel's stored brake disc (the smaller collision
    /// disc on its axle line) and put on its centre. The disc turns with the
    /// wheel, the caliper does not.
    fn update_brakes(&mut self, rs: &egui_wgpu::RenderState, ctx: &egui::Context) {
        if let Some(ls) = self.brake_pending.lock().unwrap().take() {
            self.brake_loading = false;
            let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
            self.brakes = ls.iter().map(|(disc, l)| (*disc, ren.upload(rs, &l.model, &[]))).collect();
        }
        let spots: Vec<(String, Vec3, f32)> = self
            .loaded
            .as_ref()
            .and_then(|l| l.info["wheel_mounts"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|m| {
                let c: [f32; 3] = serde_json::from_value(m["brake_centre"].clone()).ok()?;
                Some((m["bone"].as_str()?.to_string(), Vec3::new(c[0], c[2], -c[1]), m["brake_radius"].as_f64()? as f32))
            })
            .collect();
        let (disc, caliper) = if spots.is_empty() { (None, None) } else { self.veh_brake.clone() };
        let key = format!("{disc:?}|{caliper:?}");
        if key != self.brake_key {
            self.brake_key = key;
            self.brakes.clear();
            let qs: Vec<(bool, String)> = [(true, disc), (false, caliper)].into_iter().filter_map(|(d, n)| n.map(|n| (d, format!("^{n}_hi$")))).collect();
            if !qs.is_empty() {
                self.brake_loading = true;
                let ws = Workspace::new(&self.game, &self.work);
                let cache = self.cache_dir();
                let slot = self.brake_pending.clone();
                let idx = self.indexes.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    // a part that does not load is left out
                    let r: Vec<(bool, Loaded)> = qs.iter().filter_map(|(d, q)| load_query(&ws, &cache, q, 0, &idx).ok().map(|l| (*d, l))).collect();
                    *slot.lock().unwrap() = Some(r);
                    ctx.request_repaint();
                });
            }
        }
        if let Some(ren) = &self.renderer {
            for (is_disc, g) in self.brakes.iter_mut() {
                let inst: Vec<render3d::Instance> = spots
                    .iter()
                    .map(|(bone, p, r)| {
                        let right = bone.strip_prefix("wheel_").map(|s| s.starts_with('r')).unwrap_or(false);
                        let mirror = if right { Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)) } else { Mat4::IDENTITY };
                        let spin = if *is_disc { Mat4::from_quat(Quat::from_rotation_x(self.wheel_spin)) } else { Mat4::IDENTITY };
                        render3d::Instance::new(Mat4::from_translation(*p) * spin * mirror * Mat4::from_scale(Vec3::splat(r / 0.5)), false)
                    })
                    .collect();
                ren.set_instances(rs, g, &inst);
            }
        }
    }

    /// Stored wheel size per mount (tyre radius, half width in metres), from
    /// the fragment's collision discs.
    fn discs(&self) -> Vec<Option<[f32; 2]>> {
        let Some(l) = &self.loaded else { return vec![] };
        l.info["wheel_mounts"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|m| m["bone"].is_string() && (m["pivot"].is_array() || m["centre"].is_array()))
                    .map(|m| Some([m["radius"].as_f64()? as f32, m["half_width"].as_f64()? as f32]))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn update_wheels(&mut self, rs: &egui_wgpu::RenderState, ctx: &egui::Context) {
        // the roof clip of the loaded convertible, read once per model file
        if let Some(l) = &self.loaded {
            if self.roof_anim.0 != l.model.path {
                let any = l.info["roof_animation"]["animations"].as_array().is_some_and(|a| !a.is_empty()) || l.info["roof_animation"]["status"] == "ok";
                let mut all = if any { glb::load_anims(&l.model.path).unwrap_or_default() } else { Vec::new() };
                let roof = all.iter().position(|a| a.name.to_ascii_lowercase().ends_with("/roof")).map(|i| all.remove(i));
                self.roof_anim = (l.model.path.clone(), roof);
                self.clip_pos = vec![self.clip_pos_all; all.len()];
                self.clips = all;
                self.hinges = find_hinges(&l.model);
            }
        }
        if let Some(r) = self.wheel_pending.lock().unwrap().take() {
            self.wheel_loading = false;
            match r {
                Ok(ls) => {
                    let c = self.rim_colour;
                    self.rim_applied = c;
                    self.wheel_boxes = ls.iter().map(|l| l.model.bbox).collect();
                    self.wheel_mats = ls.iter().map(|l| l.model.materials.iter().map(|m| m.name.clone()).collect()).collect();
                    let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
                    self.wheels = ls
                        .iter()
                        .map(|l| {
                            let mut g = ren.upload(rs, &l.model, &[]);
                            // rims take the rim colour (the game sets it per
                            // vehicle; it is not in the files)
                            for (k, m) in l.model.materials.iter().enumerate() {
                                if m.name.ends_with(RIM_SHADER) {
                                    ren.set_material_base(rs, &mut g, k, [c[0], c[1], c[2], 1.0]);
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
        if self.rim_applied != self.rim_colour {
            self.rim_applied = self.rim_colour;
            let c = self.rim_colour;
            if let Some(ren) = &self.renderer {
                for (g, names) in self.wheels.iter_mut().zip(&self.wheel_mats) {
                    for (k, n) in names.iter().enumerate() {
                        if n.ends_with(RIM_SHADER) {
                            ren.set_material_base(rs, g, k, [c[0], c[1], c[2], 1.0]);
                        }
                    }
                }
            }
        }
        let mounts = self.mounts();
        if !self.wheels_on || self.veh_no_wheels || mounts.is_empty() {
            return;
        }
        self.update_brakes(rs, ctx);
        let key = format!("{}|{}", self.wheel_rim, self.wheel_tyre);
        if key != self.wheel_key {
            self.wheel_key = key;
            self.wheels.clear();
            self.wheel_loading = true;
            let qs = [
                format!("^{}$", self.wheel_rim),
                format!("^{}$", self.wheel_tyre),
            ];
            let ws = Workspace::new(&self.game, &self.work);
            let cache = self.cache_dir();
            let slot = self.wheel_pending.clone();
            let idx = self.indexes.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let r: Result<Vec<Loaded>, String> = qs
                    .iter()
                    .filter(|q| q.len() > 3)
                    .map(|q| load_query(&ws, &cache, q, 0, &idx).map_err(|e| format!("{e:#}")))
                    .collect();
                *slot.lock().unwrap() = Some(r);
                ctx.request_repaint();
            });
        }
        // Two-wheelers: the stored data gives no wheel width, and at the rim's
        // scale a tyre is as wide as a car's. Narrow the wheel to fit between
        // the fork legs or the swingarm (an estimate).
        let xs_key = format!(
            "{}|{:?}|{}|{}|{}|{}|{}",
            self.wheel_auto,
            // the loaded wheel models' sizes: they arrive after the key's
            // names change, and the fit depends on them
            self.wheel_boxes,
            self.loaded.as_ref().map(|l| l.source.as_str()).unwrap_or(""),
            self.wheel_scale,
            self.wheel_scale_rear,
            self.wheel_key,
            self.wheel_loading
        );
        if self.wheel_xs.0 != xs_key {
            let discs = self.discs();
            let tyre = tyre_size(&self.wheel_boxes);
            let v = match &self.loaded {
                // The vehicle stores each wheel's radius and width (its
                // collision discs): size the wheel to them.
                Some(_) if self.wheel_auto && tyre.is_some() && discs.len() == mounts.len() && discs.iter().any(|d| d.is_some()) => {
                    let (tr, tw) = tyre.unwrap();
                    let any = discs.iter().flatten().next().copied().unwrap();
                    mounts
                        .iter()
                        .zip(&discs)
                        .map(|((bone, _), d)| {
                            // a wheel without a disc takes the size of one that has
                            let [r, hw] = d.unwrap_or(any);
                            let base = if is_rear(bone) { self.wheel_scale_rear } else { self.wheel_scale };
                            let sc = r / tr;
                            // a rail wheel has no tyre and comes with its axle: keep its width
                            let width = if self.wheel_tyre.is_empty() { 1.0 } else { (2.0 * hw / (tw * sc)).clamp(0.05, 2.0) };
                            [width, sc / base.max(1e-4)]
                        })
                        .collect()
                }
                Some(l) => {
                    let scale = |bone: &str| if is_rear(bone) { self.wheel_scale_rear } else { self.wheel_scale };
                    // Sized from wheelScale, a wheel is as big as its arch on
                    // about half the cars: shrink it to clear the arch (the
                    // same on both sides of an axle).
                    let mut fit: Vec<f32> = mounts
                        .iter()
                        .map(|(bone, p)| if self.wheel_auto { arch_fit(&l.model, *p, scale(bone), &self.wheel_boxes) } else { 1.0 })
                        .collect();
                    // one size per axle, and one for the car where front and
                    // rear wheelScale are the same: the tightest arch decides
                    let same = self.wheel_scale == self.wheel_scale_rear;
                    for rear in [false, true] {
                        let m = mounts.iter().zip(&fit).filter(|(b, _)| same || is_rear(&b.0) == rear).map(|(_, f)| *f).fold(f32::MAX, f32::min);
                        for (b, f) in mounts.iter().zip(fit.iter_mut()) {
                            if m < f32::MAX && (same || is_rear(&b.0) == rear) {
                                *f = m;
                            }
                        }
                    }
                    mounts
                        .iter()
                        .zip(&fit)
                        .map(|((bone, p), f)| [fork_fit(&l.model, *p, scale(bone) * f, &self.wheel_boxes), *f])
                        .collect()
                }
                None => vec![],
            };
            self.wheel_xs = (xs_key, v);
        }
        let xs = self.wheel_xs.1.clone();
        let inst: Vec<render3d::Instance> = mounts
            .iter()
            .enumerate()
            .map(|(mi, (bone, p))| {
                let right = bone
                    .strip_prefix("wheel_")
                    .map(|s| s.starts_with('r'))
                    .unwrap_or(false);
                // the inner wheel of a dual pair faces the other way
                let mirror = if right != bone.ends_with("_inr") {
                    Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0))
                } else {
                    Mat4::IDENTITY
                };
                let spin = Mat4::from_quat(Quat::from_rotation_x(self.wheel_spin));
                let rear = is_rear(bone);
                let [width, fit] = xs.get(mi).copied().unwrap_or([1.0, 1.0]);
                let sc = fit
                    * if rear {
                        self.wheel_scale_rear
                    } else {
                        self.wheel_scale
                    };
                render3d::Instance::new(
                    Mat4::from_translation(*p)
                        * spin
                        * mirror
                        * Mat4::from_scale(Vec3::new(sc * width, sc, sc)),
                    false,
                )
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
        let Some(l) = &self.loaded else {
            return (vec![], vec![]);
        };
        // the roof clip replaces the local transform of the joints it drives
        let posed: Vec<glb::Joint>;
        // clips being played: (clip, position 0..1)
        let mut playing: Vec<(&glb::Anim, f32)> = Vec::new();
        if self.roof_anim.0 == l.model.path {
            if let Some(a) = &self.roof_anim.1 {
                if self.roof_fold > 0.0 {
                    playing.push((a, self.roof_fold));
                }
            }
            playing.extend(self.clips.iter().zip(&self.clip_pos).filter(|(_, p)| **p > 0.0).map(|(a, p)| (a, *p)));
        }
        let js: &[glb::Joint] = match playing.is_empty() {
            false => {
                let mut v: Vec<glb::Joint> = l.model.joints.iter().map(|j| glb::Joint { name: j.name.clone(), parent: j.parent, local: j.local, ibm: j.ibm }).collect();
                let mut trs: Vec<Option<(Vec3, Quat, Vec3)>> = vec![None; v.len()];
                for (a, pos) in &playing {
                    let t = pos * a.duration;
                    for c in &a.channels {
                        let Some(j) = v.get(c.joint) else { continue };
                        let e = trs[c.joint].get_or_insert_with(|| {
                            let (s, r, t) = Mat4::from_cols_array(&j.local).to_scale_rotation_translation();
                            (t, r, s)
                        });
                        let x = c.sample(t);
                        match c.path {
                            0 => e.0 = Vec3::new(x[0], x[1], x[2]),
                            1 => e.1 = Quat::from_xyzw(x[0], x[1], x[2], x[3]).normalize(),
                            _ => e.2 = Vec3::new(x[0], x[1], x[2]),
                        }
                    }
                }
                for (j, e) in v.iter_mut().zip(trs) {
                    if let Some((t, r, s)) = e {
                        j.local = Mat4::from_scale_rotation_translation(s, r, t).to_cols_array();
                    }
                }
                posed = v;
                &posed
            }
            true => &l.model.joints,
        };
        // hinged panels: this frame's rotation per joint
        let open = [self.door_open, self.bonnet_open, self.boot_open];
        let swing: HashMap<usize, Mat4> = self.hinges.iter().filter(|(_, h)| open[h.0 as usize] != 0.0).map(|(i, h)| (*i, Mat4::from_quat(Quat::from_axis_angle(h.1, h.2 * open[h.0 as usize])))).collect();
        let mut global: Vec<Option<Mat4>> = vec![None; js.len()];
        fn g(
            i: usize,
            js: &[glb::Joint],
            global: &mut Vec<Option<Mat4>>,
            spin: f32,
            swing: &HashMap<usize, Mat4>,
            roof_down: bool,
            depth: usize,
        ) -> Mat4 {
            if let Some(m) = global[i] {
                return m;
            }
            let j = &js[i];
            let mut local = Mat4::from_cols_array(&j.local);
            let n = j.name.to_lowercase();
            if spin != 0.0 && is_wheel_bone(&n) {
                local = local * Mat4::from_quat(Quat::from_rotation_x(spin));
            }
            if let Some(m) = swing.get(&i) {
                local = local * *m;
            }
            // roof down: collapse what hangs on convertible_roof, but not the
            // cover that closes over the stowed roof
            if roof_down && n != "roof_cover_ctrl" && j.parent.is_some_and(|p| js[p].name.eq_ignore_ascii_case("convertible_roof")) {
                local = local * Mat4::from_scale(Vec3::ZERO);
            }
            let m = match j.parent {
                Some(p) if p != i && depth < 64 => g(p, js, global, spin, swing, roof_down, depth + 1) * local,
                _ => local,
            };
            global[i] = Some(m);
            m
        }
        let gl: Vec<Mat4> = (0..js.len())
            .map(|i| g(i, js, &mut global, self.wheel_spin, &swing, self.roof_down && self.roof_anim.1.is_none(), 0))
            .collect();
        let skin: Vec<Mat4> = gl
            .iter()
            .zip(js)
            .map(|(m, j)| *m * Mat4::from_cols_array(&j.ibm))
            .collect();
        (gl, skin)
    }

    fn lines(&self, globals: &[Mat4]) -> (Vec<LineV>, Vec<LineV>) {
        let mut under = Vec::new();
        let mut top = Vec::new();
        let Some(l) = &self.loaded else {
            return (under, top);
        };
        if self.show_skeleton {
            for (i, j) in l.model.joints.iter().enumerate() {
                let p = globals[i].w_axis.truncate();
                if let Some(pa) = j.parent {
                    let q = globals[pa].w_axis.truncate();
                    let a = if pa == 0 { 70 } else { 255 };
                    top.push(LineV {
                        pos: q.into(),
                        col: [255, 220, 40, a],
                    });
                    top.push(LineV {
                        pos: p.into(),
                        col: [255, 120, 40, a],
                    });
                }
                let s = (self.cam.dist * 0.01).max(0.01);
                for (ax, c) in [
                    (globals[i].x_axis, [255, 60, 60, 255]),
                    (globals[i].y_axis, [60, 255, 60, 255]),
                    (globals[i].z_axis, [80, 120, 255, 255]),
                ] {
                    top.push(LineV {
                        pos: p.into(),
                        col: c,
                    });
                    top.push(LineV {
                        pos: (p + ax.truncate().normalize_or_zero() * s).into(),
                        col: c,
                    });
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
                    under.push(LineV {
                        pos: p.into(),
                        col: [80, 160, 255, 255],
                    });
                    under.push(LineV {
                        pos: (p + n * len).into(),
                        col: [80, 255, 160, 255],
                    });
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
            let col = if k == 0 {
                [255, 255, 255, 90]
            } else {
                [255, 255, 255, 35]
            };
            let x = c.x + k as f32 * step;
            let z = c.z + k as f32 * step;
            under.push(LineV {
                pos: [x, lo.y, c.z - n as f32 * step],
                col,
            });
            under.push(LineV {
                pos: [x, lo.y, c.z + n as f32 * step],
                col,
            });
            under.push(LineV {
                pos: [c.x - n as f32 * step, lo.y, z],
                col,
            });
            under.push(LineV {
                pos: [c.x + n as f32 * step, lo.y, z],
                col,
            });
        }
        (under, top)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, render_state: Option<&egui_wgpu::RenderState>) {
        let ctx = ui.ctx().clone();
        let Some(rs) = render_state else {
            ui.label("The model viewer needs the wgpu renderer.");
            return;
        };
        self.poll(rs);
        // ↑/↓ step through the picker list (when no text field has focus)
        if self.show_picker && !ctx.egui_wants_keyboard_input() && !self.loading {
            let d = ui.input(|i| {
                if i.key_pressed(egui::Key::ArrowDown) {
                    1
                } else if i.key_pressed(egui::Key::ArrowUp) {
                    -1
                } else {
                    0
                }
            });
            if d != 0 {
                if let Some(id) = self.picker.step(d) {
                    self.open(&id, 0, &ctx);
                }
            }
        }
        // toolbar
        ui.horizontal(|ui| {
            ui.toggle_value(&mut self.show_picker, "☰ Models")
                .on_hover_text("Show / hide the model list");
            if self.show_picker {
                if ui
                    .add_enabled(!self.loading, egui::Button::new("◀"))
                    .on_hover_text("previous in list (↑)")
                    .clicked()
                {
                    if let Some(id) = self.picker.step(-1) {
                        self.open(&id, 0, &ctx);
                    }
                }
                if ui
                    .add_enabled(!self.loading, egui::Button::new("▶"))
                    .on_hover_text("next in list (↓)")
                    .clicked()
                {
                    if let Some(id) = self.picker.step(1) {
                        self.open(&id, 0, &ctx);
                    }
                }
            }
            ui.separator();
            ui.label("Model:");
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("levels.rpf@219c8b010 · ^phoenix · path.glb")
                    .desired_width(320.0),
            );
            if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                || ui
                    .add_enabled(!self.loading, egui::Button::new("Open"))
                    .clicked()
            {
                let q = self.query.clone();
                self.open(&q, 0, &ctx);
            }
            if ui.button("Open .glb…").clicked() {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("glb", &["glb"])
                    .pick_file()
                {
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
                        if ui
                            .selectable_label(l.lod == k, format!("{level}"))
                            .clicked()
                            && l.lod != k
                        {
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
                let name = self
                    .loaded
                    .as_ref()
                    .map(|l| sanitize(&l.source))
                    .unwrap_or_else(|| "empty".into());
                let p = Path::new(&self.work).join("screenshots").join(format!(
                    "{name}_{}.png",
                    MODES[self.look.mode as usize].replace(' ', "_")
                ));
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
            let current = if self.query.contains('@') {
                Some(self.query.trim().to_string())
            } else {
                None
            };
            let mut want = None;
            egui::Panel::left("model_picker")
                .resizable(true)
                .default_size(300.0)
                .size_range(220.0..=560.0)
                .show(ui, |ui| {
                    want = self.picker.ui(ui, ws, idx, current.as_deref());
                });
            if let Some(id) = want {
                self.open(&id, 0, &ctx);
            }
        }
        egui::Panel::right("model_side")
            .resizable(true)
            .default_size(330.0)
            .show(ui, |ui| {
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
        let show_wheels = self.wheels_on && !self.veh_no_wheels && !self.mounts().is_empty();
        let (under, top) = self.lines(&globals);
        let settled = self.idle_for_shot();
        let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
        if let Some(g) = &self.gpu {
            if !skin.is_empty() {
                ren.set_joints(rs, g, &skin);
            }
        }
        self.update_hair(rs, &ctx);
        let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
        let mut models: Vec<&GpuModel> = self.gpu.iter().collect();
        if show_wheels {
            models.extend(self.wheels.iter());
            models.extend(self.brakes.iter().map(|b| &b.1));
        }
        if self.hair.show {
            models.extend(self.hair.gpu.iter());
        }
        ren.decal_bias = self.decal_bias;
        let tid = ren.render(rs, px, &self.cam, &self.look, &models, &under, &top);
        ui.painter().image(
            tid,
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        // joint labels
        if self.show_skeleton && self.joint_labels {
            if let Some(l) = &self.loaded {
                for (i, j) in l.model.joints.iter().enumerate() {
                    let n = j.name.to_lowercase();
                    let important = n.starts_with("wheel")
                        || n.starts_with("door")
                        || n.starts_with("bonnet")
                        || n.starts_with("boot")
                        || n == "chassis"
                        || l.model.joints.len() < 40;
                    if !important {
                        continue;
                    }
                    if let Some(p) =
                        Renderer3D::project(&self.cam, rect.size(), globals[i].w_axis.truncate())
                    {
                        ui.painter().text(
                            rect.min + p.to_vec2(),
                            egui::Align2::LEFT_BOTTOM,
                            &j.name,
                            egui::FontId::proportional(11.0),
                            egui::Color32::from_rgb(255, 230, 120),
                        );
                    }
                }
            }
        }
        // legend for the gap view
        if self.look.mode == 5 {
            let p = rect.left_bottom() + egui::vec2(10.0, -70.0);
            for (k, (c, t)) in [
                (egui::Color32::from_rgb(60, 200, 90), "ok (full mip chain)"),
                (
                    egui::Color32::from_rgb(240, 190, 40),
                    "lowmip (top mips in BCPack)",
                ),
                (
                    egui::Color32::from_rgb(230, 50, 50),
                    "flat (no decodable mip)",
                ),
            ]
            .iter()
            .enumerate()
            {
                let y = p.y + k as f32 * 18.0;
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(egui::pos2(p.x, y), egui::vec2(12.0, 12.0)),
                    2.0,
                    *c,
                );
                ui.painter().text(
                    egui::pos2(p.x + 18.0, y),
                    egui::Align2::LEFT_TOP,
                    *t,
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
            }
        }
        ui.painter().text(
            rect.right_bottom() + egui::vec2(-8.0, -6.0),
            egui::Align2::RIGHT_BOTTOM,
            if self.cam.fly {
                "fly: WASD/QE, Shift fast, drag look"
            } else {
                "orbit: drag · pan: right drag · zoom: wheel · double-click: frame"
            },
            egui::FontId::proportional(11.0),
            egui::Color32::from_white_alpha(160),
        );

        // screenshots (UI button and --screenshot automation)
        if self.gpu.is_some()
            || self.loaded.is_none()
                && !self.loading
                && self.auto.screenshot.is_some()
                && self.auto.frames > 30
        {
            self.frames_since_load += 1;
        }
        if let Some(path) = self.auto.screenshot.clone() {
            self.auto.frames += 1;
            let ready = (self.gpu.is_some() && self.frames_since_load >= 3 && settled)
                || (!self.loading && !self.hair.fitting && !self.auto.find_hair && self.hair.settled() && self.auto.frames > 600);
            if ready {
                if let Some((w, h, px)) = ren.read_back(rs) {
                    match render3d::save_png(&path, w, h, &px) {
                        Ok(()) => self.last_shot = Some(format!("saved {}", path.display())),
                        Err(e) => self.last_shot = Some(format!("screenshot failed: {e}")),
                    }
                }
                self.auto.screenshot = None;
                // full window (UI included) next to it
                let ui_path = path.with_file_name(format!(
                    "{}_ui.png",
                    path.file_stem().unwrap_or_default().to_string_lossy()
                ));
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
        if self.auto.then.is_some() && self.gpu.is_some() && !self.loading && self.frames_since_load >= 3 {
            let q = self.auto.then.take().unwrap();
            self.open(&q, 0, &ctx);
        }
        if self.loading || resp.dragged() || self.cam.fly || self.auto.then.is_some() {
            ctx.request_repaint();
        }
    }

    /// Controls that apply to the loaded model (none when nothing is loaded).
    fn caps(&mut self) -> Caps {
        let Some(l) = &self.loaded else {
            return Caps::default();
        };
        if !matches!(&self.caps, Some((p, t, _)) if *p == l.model.path && *t == l.model.triangles) {
            self.caps = Some((l.model.path.clone(), l.model.triangles, Caps::of(l)));
        }
        self.caps.as_ref().unwrap().2
    }

    fn side_ui(&mut self, ui: &mut egui::Ui, rs: &egui_wgpu::RenderState) {
        let caps = self.caps();
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
            ui.label(if src == "linked" {
                "faces the game links to this head"
            } else {
                "automatic: faces of the same sex, body type and ethnicity"
            });
            if let Some(k) = step {
                HEAD_SKIN.store(k, std::sync::atomic::Ordering::Relaxed);
                let lod = self
                    .loaded
                    .as_ref()
                    .and_then(|l| lod_level(&l.lods[l.lod]))
                    .unwrap_or(0);
                let q = self.query.clone();
                self.keep_cam_once = true;
                self.open(&q, lod, ui.ctx());
            }
            ui.separator();
        }
        ui.heading("View");
        egui::ComboBox::from_label("mode")
            .selected_text(MODES[self.look.mode as usize])
            .show_ui(ui, |ui| {
                for (k, m) in MODES.iter().enumerate() {
                    ui.selectable_value(&mut self.look.mode, k as u32, *m);
                }
            });
        if self.look.mode == 4 {
            egui::ComboBox::from_label("slot")
                .selected_text(SLOTS[self.look.slot as usize])
                .show_ui(ui, |ui| {
                    for (k, m) in SLOTS.iter().enumerate() {
                        ui.selectable_value(&mut self.look.slot, k as u32, *m);
                    }
                });
        }
        egui::ComboBox::from_label("packed RMA order")
            .selected_text(RMA_ORDERS[self.look.rma_order as usize])
            .show_ui(ui, |ui| {
                for (k, m) in RMA_ORDERS.iter().enumerate() {
                    ui.selectable_value(&mut self.look.rma_order, k as u32, *m);
                }
            });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.look.wire, "wireframe");
            if caps.tint {
                ui.checkbox(&mut self.look.tint, "tint").on_hover_text("Multiply the model's tint palette colour into the base colour (the game's per-model colour variation; GLB COLOR_0)");
            }
            ui.checkbox(&mut self.show_normals, "normals");
            if caps.decals {
                ui.checkbox(&mut self.decal_bias, "decal bias").on_hover_text("Draw decal-bucket materials (markings, grime, signage, terrain blend layers) after opaque surfaces with a depth bias, by layer order");
            }
        });
        ui.horizontal(|ui| {
            if caps.joints {
                ui.checkbox(&mut self.show_skeleton, "skeleton");
                ui.checkbox(&mut self.joint_labels, "labels");
            }
            ui.checkbox(&mut self.cam.fly, "fly camera");
        });
        ui.separator();
        ui.heading("Light");
        if ui
            .add(egui::Slider::new(&mut self.tod, 0.0..=24.0).text("time of day (h)"))
            .changed()
        {
            self.look.set_time_of_day(self.tod);
        }
        ui.add(
            egui::Slider::new(&mut self.look.exposure, 0.2..=4.0)
                .logarithmic(true)
                .text("exposure"),
        );
        ui.add(egui::Slider::new(&mut self.look.ambient, 0.0..=2.0).text("sky / ambient"));
        ui.add(egui::Slider::new(&mut self.look.emissive, 0.0..=12.0).text("emissive"));
        ui.add(egui::Slider::new(&mut self.look.wet, 0.0..=1.0).text("wet"));
        ui.separator();
        self.vehicle_ui(ui);
        if caps.paint {
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
            egui::ComboBox::from_label("finish")
                .selected_text(FINISHES[self.look.finish as usize])
                .show_ui(ui, |ui| {
                    for (k, m) in FINISHES.iter().enumerate() {
                        ui.selectable_value(&mut self.look.finish, k as u32, *m);
                    }
                });
        }
        if self.loaded.is_some() {
            if caps.wheels || caps.doors || caps.mounts {
                ui.separator();
                ui.heading("Rig");
                if caps.wheels {
                    ui.add(egui::Slider::new(&mut self.wheel_spin, -3.14..=3.14).text("wheel spin"));
                }
                if caps.doors {
                    ui.add(egui::Slider::new(&mut self.door_open, 0.0..=1.3).text("doors open"));
                }
                if self.hinges.values().any(|h| h.0 == 1) {
                    ui.add(egui::Slider::new(&mut self.bonnet_open, 0.0..=1.1).text("bonnet open"));
                }
                if self.hinges.values().any(|h| h.0 == 2) {
                    ui.add(egui::Slider::new(&mut self.boot_open, 0.0..=1.1).text("boot open"));
                }
                for (a, p) in self.clips.iter().zip(self.clip_pos.iter_mut()) {
                    let n = a.name.rsplit('/').next().unwrap_or(&a.name).trim_end_matches("_anim").replace('_', " ");
                    ui.add(egui::Slider::new(p, 0.0..=1.0).text(n)).on_hover_text(format!("plays the vehicle's clip {} ({:.1} s)", a.name, a.duration));
                }
                if caps.roof || self.roof_anim.1.is_some() {
                    match &self.roof_anim.1 {
                        Some(a) => {
                            ui.add(egui::Slider::new(&mut self.roof_fold, 0.0..=1.0).text("roof fold")).on_hover_text(format!("plays the car's own roof clip ({}, {:.1} s): 0 = roof up, 1 = stowed", a.name, a.duration));
                        }
                        None => {
                            ui.checkbox(&mut self.roof_down, "roof down").on_hover_text("no roof clip was found for this car: hides the soft top and its frame (everything under the convertible_roof bone except the roof cover)");
                        }
                    }
                }
                if caps.mounts {
                    ui.checkbox(&mut self.wheels_on, "fit shared wheels at the wheel mounts");
                    ui.horizontal(|ui| {
                        ui.label("rim");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.wheel_rim).desired_width(140.0),
                        );
                        ui.color_edit_button_rgb(&mut self.rim_colour)
                            .on_hover_text("rim colour. The game sets it per vehicle while it runs and it is not in the decoded vehicle data, so every car opens with a plain alloy");
                        if ui.small_button("alloy").on_hover_text("back to the plain alloy").clicked() {
                            self.rim_colour = RIM_ALLOY;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("tyre");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.wheel_tyre).desired_width(140.0),
                        );
                    });
                    ui.checkbox(&mut self.wheel_auto, "size the wheels from the vehicle's data").on_hover_text(format!("scale = {WHEEL_FACTOR:.0} x wheelScale (front / rear): wheelScale is the rim radius in metres and the shared rims have radius 0.5. The tyre is scaled with the rim; used only where the vehicle stores no wheel size. Most do: each wheel has a collision disc with the tyre's radius and width, and the wheel is sized and placed to it. Without one, the wheel is sized to fill its arch and narrowed to fit the fork (estimates)."));
                    ui.add_enabled_ui(!self.wheel_auto, |ui| {
                        ui.add(
                            egui::Slider::new(&mut self.wheel_scale, 0.1..=1.2)
                                .text("front wheel scale"),
                        );
                        ui.add(
                            egui::Slider::new(&mut self.wheel_scale_rear, 0.1..=1.2)
                                .text("rear wheel scale"),
                        );
                    });
                }
            }
        }
        ui.separator();
        // Grooms go on characters: rigged models that are not vehicles (or
        // nothing loaded), and wherever some are already picked.
        if (caps.joints && !caps.mounts) || self.loaded.is_none() || !self.hair.selected.is_empty() || self.auto.find_hair {
            // on the panel itself, not inside the collapsed section
            if !self.hair.selected.is_empty() {
                let label = if self.hair.fur.is_some() { "fur guide strands" } else { "hair guide strands" };
                ui.checkbox(&mut self.hair.show, label).on_hover_text("Show the strand grooms. These are the guide strands the game grows its hair and fur from, drawn as ribbons; the full coat is not decoded.");
            }
            self.hair_ui(ui);
            ui.separator();
        }
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
            let visible = self
                .gpu
                .as_ref()
                .map(|g| {
                    g.meshes
                        .iter()
                        .filter(|me| me.material == k)
                        .all(|me| me.visible)
                })
                .unwrap_or(true);
            ui.horizontal(|ui| {
                let mut v = visible;
                if ui.checkbox(&mut v, "").changed() {
                    toggle = Some((k, v));
                }
                if m.class == "car_paint" {
                    let mut sec = self
                        .gpu
                        .as_ref()
                        .map(|g| Renderer3D::is_secondary(g, k))
                        .unwrap_or(false);
                    if ui
                        .checkbox(&mut sec, "2nd")
                        .on_hover_text("use the secondary paint colour")
                        .changed()
                    {
                        second = Some((k, sec));
                    }
                }
                let sel = self.material_sel == Some(k);
                // "shader07_<hash>" -> "shader07 standard_vehicle" when the name is known
                let shown = match sixng_core::names::shader(&m.shader_hash) {
                    Some(n) => format!("{} {n}", m.name.split('_').next().unwrap_or("")),
                    None => m.name.clone(),
                };
                if ui
                    .selectable_label(
                        sel,
                        egui::RichText::new(format!(
                            "{} · {} · {}{}",
                            shown,
                            m.class,
                            m.alpha_mode,
                            if m.gap == "no colour texture" {
                                " · no colour texture (shown white)"
                            } else {
                                ""
                            }
                        ))
                        .color(col),
                    )
                    .clicked()
                {
                    self.material_sel = if sel { None } else { Some(k) };
                }
            });
            if self.material_sel == Some(k) {
                ui.indent(k, |ui| {
                    ui.label(format!(
                        "shader {} ({}) · gaps: {}",
                        sixng_core::names::shader(&m.shader_hash)
                            .unwrap_or_else(|| "name unknown".into()),
                        m.shader_hash,
                        m.gap
                    ));
                    for s in &m.slots {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} {} {}x{} {} — {}{}",
                                    s.role,
                                    sixng_core::names::param(&s.param_hash)
                                        .unwrap_or_else(|| s.param_hash.clone()),
                                    s.size[0],
                                    s.size[1],
                                    s.format,
                                    s.status,
                                    if s.mip > 0 {
                                        format!(" (mip {})", s.mip)
                                    } else {
                                        String::new()
                                    }
                                ))
                                .small(),
                            )
                            .on_hover_text(format!("parameter hash {}", s.param_hash));
                            if let Ok(h) = u64::from_str_radix(&s.hash, 16) {
                                if ui
                                    .small_button("view")
                                    .on_hover_text(format!(
                                        "open texture {} in the Textures tab",
                                        s.hash
                                    ))
                                    .clicked()
                                {
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
                    ui.label(
                        egui::RichText::new(format!(
                            "{}  ← {}",
                            j.name,
                            j.parent
                                .map(|p| l.model.joints[p].name.as_str())
                                .unwrap_or("-")
                        ))
                        .small(),
                    );
                }
            });
        }
        ui.collapsing("model.json", |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(serde_json::to_string_pretty(&l.info).unwrap_or_default())
                        .monospace()
                        .small(),
                )
                .extend(),
            );
        });
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
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
    info["meshes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["lod"].as_u64())
        .min()
        .unwrap_or(0) as usize
}

/// Physical LOD number of the loaded file at `index`.
fn shown_lod(lods: &[PathBuf], index: usize, info: &serde_json::Value) -> usize {
    lods.get(index)
        .and_then(|p| lod_level(p))
        .unwrap_or_else(|| primary_lod(info))
}

fn lod_level(path: &Path) -> Option<usize> {
    path.file_stem()?
        .to_str()?
        .strip_prefix("model_lod")?
        .parse()
        .ok()
}

/// Choose by physical slot, falling back to the preceding populated LOD.
/// model.glb is the primary fallback, even when its physical slot is not zero.
pub(crate) fn lod_index(lods: &[PathBuf], lod: usize) -> usize {
    lods.iter()
        .rposition(|p| lod_level(p).unwrap_or(0) <= lod)
        .unwrap_or(0)
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

pub fn index_for(
    ws: &Workspace,
    archive: &str,
    cache: &Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>,
) -> anyhow::Result<(sixng_core::game::ArchiveInfo, Arc<ArchiveIndex>)> {
    let a = ws.resolve(archive)?;
    if let Some(ix) = cache.lock().unwrap().get(&a.id) {
        return Ok((a, ix.clone()));
    }
    let ix = Arc::new(ws.load_index(&a)?);
    cache.lock().unwrap().insert(a.id.clone(), ix.clone());
    Ok((a, ix))
}

pub(crate) fn load_query(
    ws: &Workspace,
    cache: &Path,
    q: &str,
    lod: usize,
    idx: &Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>,
) -> anyhow::Result<Loaded> {
    let (q, sel) = split_parts(q);
    let mut l = load_query_sel(ws, cache, q, sel, lod, idx)?;
    l.base = q.to_string();
    Ok(l)
}

/// `"<query>#parts=<hex>,<hex>"` → the query and the chosen part offsets.
fn split_parts(q: &str) -> (&str, Option<Vec<u64>>) {
    match q.trim().split_once("#parts=") {
        Some((b, l)) => (
            b.trim(),
            Some(
                l.split(',')
                    .filter_map(|x| u64::from_str_radix(x.trim(), 16).ok())
                    .collect(),
            ),
        ),
        None => (q.trim(), None),
    }
}

/// Export and merge a set of parts; returns the model (None if none loaded),
/// the first part's LOD files, the loaded names and the errors. A model
/// passed in (an animal root) has its placeholder geometry replaced.
fn load_parts(
    a: &sixng_core::game::ArchiveInfo,
    parts: &[&sixng_core::scan::Entry],
    cache: &Path,
    lod: usize,
    mut model: Option<glb::Model>,
) -> (Option<glb::Model>, Vec<PathBuf>, Vec<String>, Vec<String>, Vec<(String, Vec<[f32; 3]>)>, Vec<([f32; 3], [u8; 3])>, Vec<bool>) {
    let mut first = Vec::new();
    let mut names = Vec::new();
    let mut errs = Vec::new();
    let mut skin = Vec::new();
    let mut colours = Vec::new();
    let mut mask = Vec::new();
    let mut fresh = true;
    for e in parts {
        match export_load(a, e, cache, lod) {
            Ok((m, lods, _)) => {
                if first.is_empty() {
                    first = lods;
                }
                // skin points per part, in RAGE model space (glTF is Y-up)
                let pts: Vec<[f32; 3]> = m
                    .meshes
                    .iter()
                    .flat_map(|me| {
                        let v: Vec<[f32; 3]> = me.verts.iter().map(|v| [v.pos[0], -v.pos[2], v.pos[1]]).collect();
                        sixng_core::furlink::skin_points(&v, &me.indices)
                    })
                    .collect();
                let step = (pts.len() / 60_000).max(1);
                skin.push((e.name.clone().unwrap_or_default(), pts.into_iter().step_by(step).collect()));
                // skin colour at the vertices of the body and head (not eyes or teeth)
                if e.name.as_deref().is_some_and(|n| n.contains("_body_") || n.contains("_head_")) {
                    colours.extend(skin_colours(&m, &mut mask));
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
    (model, first, names, errs, skin, colours, mask)
}

/// Tint-mask coats (grey: only the mask's shading is known) next to
/// painted parts of the same animal: colour them with the painted parts'
/// mean colour, so a deer's body matches its head. A stand-in, not the
/// game's tint. Returns whether it was applied.
fn tint_masks_from_painted(model: &mut glb::Model, colours: &mut [([f32; 3], [u8; 3])], mask: &[bool]) -> bool {
    let mean = |want: bool| {
        let (mut s, mut n) = ([0f64; 3], 0usize);
        for (c, m) in colours.iter().zip(mask) {
            if *m == want {
                for k in 0..3 {
                    s[k] += c.1[k] as f64;
                }
                n += 1;
            }
        }
        (n >= 200).then(|| s.map(|x| (x / n as f64 / 255.0) as f32))
    };
    let (Some(painted), Some(grey)) = (mean(false), mean(true)) else { return false };
    let f: [f32; 3] = std::array::from_fn(|k| (painted[k] / grey[k].max(1e-3)).clamp(0.05, 6.0));
    for m in model.materials.iter_mut().filter(|m| m.tint_mask) {
        for k in 0..3 {
            m.base_factor[k] *= f[k];
        }
    }
    for (c, m) in colours.iter_mut().zip(mask) {
        if *m {
            for k in 0..3 {
                c.1[k] = (c.1[k] as f32 * f[k]).min(255.0) as u8;
            }
        }
    }
    true
}

/// Linear colour of the base texture at each vertex of a part (every
/// vertex up to 20,000, then every n-th), with its RAGE-space position.
fn skin_colours(m: &glb::Model, mask: &mut Vec<bool>) -> Vec<([f32; 3], [u8; 3])> {
    let lin = |v: u8| {
        let c = v as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let total: usize = m.meshes.iter().map(|me| me.verts.len()).sum();
    let step = (total / 20_000).max(1);
    let mut out = Vec::new();
    for me in &m.meshes {
        let Some(mat) = me.material.and_then(|i| m.materials.get(i)) else { continue };
        let img = mat.base.and_then(|i| m.images.get(i)).and_then(|im| im.mips.first().filter(|l| l.2.len() >= (l.0 * l.1 * 4) as usize).map(|l| (l, im.srgb)));
        for v in me.verts.iter().step_by(step) {
            let mut c = [mat.base_factor[0], mat.base_factor[1], mat.base_factor[2]];
            if let Some(((w, h, px), srgb)) = img {
                let x = ((v.uv0[0].rem_euclid(1.0)) * *w as f32) as usize % *w as usize;
                let y = ((v.uv0[1].rem_euclid(1.0)) * *h as f32) as usize % *h as usize;
                let o = (y * *w as usize + x) * 4;
                let t = if mat.tint_mask { [px[o + 1]; 3] } else { [px[o], px[o + 1], px[o + 2]] };
                for k in 0..3 {
                    c[k] *= if srgb { lin(t[k]) } else { t[k] as f32 / 255.0 };
                }
            }
            out.push(([v.pos[0], -v.pos[2], v.pos[1]], c.map(|x| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)));
            mask.push(mat.tint_mask);
        }
    }
    out
}

/// The chosen parts: the selection when given, else the defaults.
fn pick<'a>(
    ix: &'a ArchiveIndex,
    opts: &[crate::assemble::PartOpt],
    sel: &Option<Vec<u64>>,
) -> (Vec<u64>, Vec<&'a sixng_core::scan::Entry>) {
    let chosen: Vec<u64> = match sel {
        Some(s) => opts
            .iter()
            .filter(|o| s.contains(&o.off))
            .map(|o| o.off)
            .collect(),
        None => opts.iter().filter(|o| o.default).map(|o| o.off).collect(),
    };
    let parts = chosen
        .iter()
        .filter_map(|off| ix.entries.iter().find(|e| e.off == *off))
        .collect();
    (chosen, parts)
}

fn load_query_sel(
    ws: &Workspace,
    cache: &Path,
    q: &str,
    sel: Option<Vec<u64>>,
    lod: usize,
    idx: &Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>,
) -> anyhow::Result<Loaded> {
    if q.to_lowercase().ends_with(".glb") {
        let p = PathBuf::from(q);
        let dir = p.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut lods = lods_in(&dir);
        if !lods.contains(&p) {
            lods = vec![p.clone()];
        }
        let lod = lods.iter().position(|x| x == &p).unwrap_or(0);
        let root = out_root_of(&p);
        let info = std::fs::read(dir.join("model.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let model = glb::load(&p, &root)?;
        return Ok(Loaded {
            model,
            lods,
            lod,
            out_root: root,
            source: dir
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            info,
            parts: Vec::new(),
            selected: Vec::new(),
            base: String::new(),
            fur: None,
            fur_skin: Default::default(),
        });
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
        let (model, first, names, errs, _, _, _) = load_parts(&a, &parts, cache, lod, None);
        let model = model
            .ok_or_else(|| anyhow::anyhow!("no part of {prefix} loaded: {}", errs.join("; ")))?;
        let info = serde_json::json!({ "assembled_from": names, "ids": parts.iter().map(|e| e.id.clone()).collect::<Vec<_>>(), "errors": errs });
        return Ok(Loaded {
            model,
            lods: first.into_iter().take(1).collect(),
            lod: 0,
            out_root: cache.to_path_buf(),
            source: format!("{} (outfit, {} parts)", prefix, names.len()),
            info,
            parts: opts,
            selected,
            base: String::new(),
            fur: None,
            fur_skin: Default::default(),
        });
    }
    let (q, member) = q
        .split_once('#')
        .map_or((q, None), |(id, member)| (id, Some(member)));
    // entry: "<archive>@<hex>" or a name regex (in levels unless "archive:regex")
    let (archive, rest) = match q.split_once('@') {
        Some((a, o)) => (a.to_string(), Some(o.to_string())),
        None => match q.split_once(':') {
            Some((a, r)) if !a.is_empty() && !a.contains('^') => {
                (a.to_string(), None).clone_with(r)
            }
            _ => ("levels".to_string(), None).clone_with(q),
        },
    };
    let (a, ix) = index_for(ws, &archive, idx)?;
    let e = match rest {
        Some(o) if !o.starts_with('~') => {
            let id = format!("{o}{}", member.map_or(String::new(), |m| format!("#{m}")));
            export::ModelResolver::new(&ix.entries)
                .resolve(&ix.entries, &id)?
                .0
        }
        Some(re) => find_name(&ix, re.trim_start_matches('~'))?,
        None => unreachable!(),
    };
    let selected = member
        .map(|name| export::member_entry(e, name))
        .transpose()?;
    let e = selected.as_ref().unwrap_or(e);
    let (model, lods, info) = export_load(&a, e, cache, lod)?;
    let level = lod;
    let lod = lod_index(&lods, level);
    // animals: the fragment is a skeleton with a placeholder plane; add the textured parts
    let name = e.name.as_deref().unwrap_or("");
    if crate::assemble::is_animal(name) && e.rtype.as_deref() == Some("ft") {
        let opts = crate::assemble::animal_options(&ix, name);
        let (selected, parts) = pick(&ix, &opts, &sel);
        let (m, _, names, _, skin, mut colours, mask) = load_parts(&a, &parts, cache, level, Some(model));
        let mut model = m.expect("root model passed in");
        let borrowed = tint_masks_from_painted(&mut model, &mut colours, &mask);
        // fur: the grooms of this animal's fur variants (cached per animal)
        let fur = match sixng_core::names::work_dir() {
            Some(work) if !skin.is_empty() => Some(sixng_core::furlink::for_animal(&work, &a, &ix.entries, name, &skin).map_err(|e| {
                eprintln!("[fur] {name}: {e:#}");
                format!("{e:#}")
            })),
            _ => None,
        };
        let mut info = info;
        if let Some(o) = info.as_object_mut() {
            o.insert("assembled_from".into(), serde_json::json!(names));
            if borrowed {
                o.insert("tint_mask_colour".into(), "tint-mask coats take the mean colour of this animal's painted parts (a stand-in: the game's tint palette is not decoded)".into());
            }
        }
        let source = if names.is_empty() {
            format!("{} {}", e.id, name)
        } else {
            format!("{} {} (+{} parts)", e.id, name, names.len())
        };
        let lods = if opts.is_empty() {
            lods
        } else {
            lods.into_iter().skip(lod).take(1).collect()
        };
        let lod = if opts.is_empty() { lod } else { 0 };
        return Ok(Loaded {
            model,
            lods,
            lod,
            out_root: cache.to_path_buf(),
            source,
            info,
            parts: opts,
            selected,
            base: String::new(),
            fur,
            fur_skin: Arc::new(colours),
        });
    }
    // weapons: the model is a skeleton with a placeholder shell; the parts
    // (receiver, barrel, stock, magazine ...) are separate models that mount
    // on its attach-point bones
    if sixng_core::weapon::is_weapon(name) {
        let opts = sixng_core::weapon::weapon_options(&ix.entries, name);
        if !opts.is_empty() {
            let (selected, parts) = pick(&ix, &opts, &sel);
            let bones = |info: &serde_json::Value| -> Vec<(String, sixng_core::weapon::Mat)> {
                info["drawable"]["skeleton"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|b| Some((b["name"].as_str()?.to_string(), serde_json::from_value(b["bind_matrix"].clone()).ok()?)))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let mut got = Vec::new();
            let mut errs = Vec::new();
            for p in &parts {
                let pn = p.name.clone().unwrap_or_default();
                match export_load(&a, p, cache, level) {
                    Ok((m, _, pi)) => got.push((sixng_core::weapon::part_slot(name, &pn).unwrap_or_default(), bones(&pi), m, pn)),
                    Err(err) => errs.push(format!("{pn}: {err:#}")),
                }
            }
            let placed = sixng_core::weapon::place(&bones(&info), &got.iter().map(|g| (g.0.clone(), g.1.clone())).collect::<Vec<_>>());
            let mut model = model;
            let (mut names, mut loose) = (Vec::new(), Vec::new());
            if placed.iter().any(|t| t.is_some()) {
                // the shell stands in for the parts
                model.meshes.clear();
                model.triangles = 0;
                model.vertices = 0;
            }
            for ((_, _, mut m, pn), t) in got.into_iter().zip(placed) {
                match t {
                    Some(t) => {
                        // RAGE model space (Z-up) to glTF (Y-up): (x, y, z) -> (x, z, -y)
                        let c = Mat4::from_cols_array(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
                        transform_model(&mut m, c * Mat4::from_cols_array(&t) * c.inverse());
                        glb::merge(&mut model, m);
                        names.push(pn);
                    }
                    None => loose.push(pn),
                }
            }
            let mut info = info;
            if let Some(o) = info.as_object_mut() {
                o.insert("assembled_from".into(), serde_json::json!(names));
                o.insert("parts_without_mount".into(), serde_json::json!(loose));
                o.insert("assembly_note".into(), "weapon parts mount on attach-point bones (a part's AAP_<slot> root on the WAP_<slot> bone of the base or of another part). The lowest-numbered variant of each slot is shown: which variant the game gives a weapon by default is not decoded. Parts listed in parts_without_mount found no attach point on this build and are left out.".into());
                if !errs.is_empty() {
                    o.insert("part_errors".into(), serde_json::json!(errs));
                }
            }
            return Ok(Loaded {
                model,
                lods: lods.into_iter().skip(lod).take(1).collect(),
                lod: 0,
                out_root: cache.to_path_buf(),
                source: format!("{} {} (+{} parts)", e.id, name, names.len()),
                info,
                parts: opts,
                selected,
                base: String::new(),
                fur: None,
                fur_skin: Arc::new(Vec::new()),
            });
        }
    }
    Ok(Loaded {
        model,
        lods,
        lod,
        out_root: cache.to_path_buf(),
        source: format!("{} {}", e.id, name).trim().to_string(),
        info,
        parts: Vec::new(),
        selected: Vec::new(),
        base: String::new(),
        fur: None,
        fur_skin: Default::default(),
    })
}

/// The hinged panels of a vehicle: door, bonnet and boot bones, with the
/// axis and the direction that opens them. Doors turn about the vertical
/// through their bone, away from the car's centre line; bonnet and boot turn
/// about the lateral axis so that the panel rises. The direction comes from
/// where the panel's vertices lie relative to its bone (a front-hinged
/// bonnet and a rear-hinged door open the right way); the opening angle is
/// the slider's, not a stored limit.
fn find_hinges(model: &glb::Model) -> HashMap<usize, (u8, Vec3, f32)> {
    let kind = |n: &str| match n.to_ascii_lowercase().as_str() {
        "door_dside_f" | "door_dside_r" | "door_pside_f" | "door_pside_r" => Some(0u8),
        "bonnet" => Some(1),
        "boot" => Some(2),
        _ => None,
    };
    // hinge bone of each joint (itself or its nearest hinge ancestor)
    let mut owner: Vec<Option<usize>> = vec![None; model.joints.len()];
    for i in 0..model.joints.len() {
        let (mut k, mut depth) = (Some(i), 0);
        while let Some(j) = k {
            if kind(&model.joints[j].name).is_some() {
                owner[i] = Some(j);
                break;
            }
            depth += 1;
            k = model.joints[j].parent.filter(|p| *p != j && depth < 64);
        }
    }
    let mut sum: HashMap<usize, (Vec3, f32)> = HashMap::new();
    for me in model.meshes.iter().filter(|m| m.skinned) {
        for v in &me.verts {
            if let Some(h) = owner.get(v.joints[0] as usize).copied().flatten() {
                let e = sum.entry(h).or_insert((Vec3::ZERO, 0.0));
                e.0 += Vec3::from(v.pos);
                e.1 += 1.0;
            }
        }
    }
    sum.into_iter()
        .filter_map(|(h, (s, n))| {
            let centre = s / n.max(1.0);
            let pivot = Mat4::from_cols_array(&model.joints[h].ibm).inverse().w_axis.truncate();
            let d = centre - pivot;
            let k = kind(&model.joints[h].name)?;
            Some((
                h,
                if k == 0 {
                    // about Y a point moves in x by d.z * angle: outwards from the centre line
                    (k, Vec3::Y, if centre.x * d.z >= 0.0 { 1.0 } else { -1.0 })
                } else {
                    // about X a point moves in y by -d.z * angle: upwards
                    (k, Vec3::X, if d.z > 0.0 { -1.0 } else { 1.0 })
                },
            ))
        })
        .collect()
}

fn is_rear(bone: &str) -> bool {
    bone.strip_prefix("wheel_").map(|s| s.chars().nth(1) == Some('r')).unwrap_or(false)
}

/// Outer radius and width of the tyre among the wheel models (the one with
/// the largest radius), at unit scale.
fn tyre_size(boxes: &[[[f32; 3]; 2]]) -> Option<(f32, f32)> {
    let radius = |b: &[[f32; 3]; 2]| b[0][1].abs().max(b[1][1].abs()).max(b[0][2].abs()).max(b[1][2].abs());
    let t = boxes.iter().max_by(|a, b| radius(a).total_cmp(&radius(b)))?;
    Some((radius(t), t[1][0] - t[0][0]))
}

/// Factor that fits a wheel to its arch: the arch radius is the nearest body
/// vertex (on a bodyshell, wing, arch liner, bumper, mudguard or chassis
/// bone) above the axle within 12 cm of the wheel's plane, and the tyre is
/// made to reach 93 % of it, smaller or larger than wheelScale gives (0.6 to
/// 1.5 times). 1.0 where no arch is found. An estimate: over 213 vehicles
/// 2 x wheelScale puts the tyre at a median 0.96 of the arch radius (0.83 to
/// 1.42), too big on ordinary cars and too small on sports cars, so no single
/// factor on wheelScale fits them all.
fn arch_fit(model: &glb::Model, p: Vec3, scale: f32, boxes: &[[[f32; 3]; 2]]) -> f32 {
    let Some((r, _)) = tyre_size(boxes) else { return 1.0 };
    let r = r * scale;
    if r <= 0.0 {
        return 1.0;
    }
    let body: Vec<bool> = model
        .joints
        .iter()
        .map(|j| {
            let n = j.name.to_lowercase();
            !n.contains("dummy") && ["bodyshell", "wing_", "archliner", "bumper_", "mudguard", "chassis"].iter().any(|k| n.contains(k))
        })
        .collect();
    let mut arch = f32::MAX;
    for m in &model.meshes {
        if !m.skinned {
            continue;
        }
        for v in &m.verts {
            if !body.get(v.joints[0] as usize).copied().unwrap_or(false) {
                continue;
            }
            if (v.pos[0] - p.x).abs() > 0.12 || v.pos[1] < p.y + 0.02 {
                continue;
            }
            let d = ((v.pos[1] - p.y).powi(2) + (v.pos[2] - p.z).powi(2)).sqrt();
            if d > 0.15 {
                arch = arch.min(d);
            }
        }
    }
    if arch == f32::MAX {
        return 1.0;
    }
    let f = 0.93 * arch / r;
    // nothing that far from the tyre is its arch (open wheels, high bodies)
    if f > 1.5 {
        return 1.0;
    }
    f.max(0.6)
}

/// Width factor of a shared wheel on a two-wheeler (mount on the centre
/// line): the widest the tyre can be and still pass between the body parts
/// beside it (fork legs, swingarm, stays), measured in the band between half
/// and 92 % of the tyre radius around the axle. 1.0 for other vehicles and
/// where nothing is beside the wheel. `p` and the model are in glTF space;
/// the wheel's axle is X.
fn fork_fit(model: &glb::Model, p: Vec3, scale: f32, boxes: &[[[f32; 3]; 2]]) -> f32 {
    if p.x.abs() > 0.05 || boxes.is_empty() {
        return 1.0;
    }
    let Some((r, w)) = tyre_size(boxes) else { return 1.0 };
    let (r, w) = (r * scale, w * scale);
    if r <= 0.0 || w <= 0.0 {
        return 1.0;
    }
    let mut half = f32::MAX;
    for m in &model.meshes {
        for v in &m.verts {
            let d = ((v.pos[1] - p.y).powi(2) + (v.pos[2] - p.z).powi(2)).sqrt();
            if d > 0.5 * r && d < 0.92 * r {
                half = half.min((v.pos[0] - p.x).abs());
            }
        }
    }
    if half == f32::MAX {
        return 1.0;
    }
    (0.9 * 2.0 * half / w).clamp(0.08, 1.0)
}

/// A wheel's own bone (`wheel_lf`, `wheel_rr`, `wheel_lm1`), not one of its
/// helpers: `wheel_turn_lf` steers a motorcycle's fork and
/// `wheel_parent_lr` / `wheel_lr_drive` carry the swingarm and the drive,
/// so spinning those threw the fork and the rear end around.
fn is_wheel_bone(n: &str) -> bool {
    let Some(r) = n.strip_prefix("wheel_") else { return false };
    let r = r.strip_suffix("_inr").unwrap_or(r);
    let b = r.as_bytes();
    b.len() >= 2 && matches!(b[0], b'l' | b'r') && matches!(b[1], b'f' | b'm' | b'r') && b[2..].iter().all(|c| c.is_ascii_digit())
}

/// Move a model's geometry by a rigid transform (weapon parts onto their
/// attach points).
fn transform_model(m: &mut glb::Model, t: Mat4) {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for me in &mut m.meshes {
        let mut c = Vec3::ZERO;
        for v in &mut me.verts {
            let p = t.transform_point3(Vec3::from(v.pos));
            v.pos = p.to_array();
            v.nrm = t.transform_vector3(Vec3::from(v.nrm)).to_array();
            let tan = t.transform_vector3(Vec3::new(v.tan[0], v.tan[1], v.tan[2]));
            v.tan = [tan.x, tan.y, tan.z, v.tan[3]];
            c += p;
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        me.centre = (c / me.verts.len().max(1) as f32).to_array();
    }
    if lo[0] <= hi[0] {
        m.bbox = [lo, hi];
    }
}

/// Export one entry into the viewer cache and load the GLB of a LOD level.
fn export_load(
    a: &sixng_core::game::ArchiveInfo,
    e: &sixng_core::scan::Entry,
    cache: &Path,
    lod: usize,
) -> anyhow::Result<(glb::Model, Vec<PathBuf>, serde_json::Value)> {
    // Heads are exported again each time (small), so the chosen face variant applies.
    let head = sixng_core::headskin::is_head(e.name.as_deref().unwrap_or(""));
    let dir = cache
        .join("models")
        .join(sixng_core::game::index_stem(&a.id))
        .join(export::export_base_name(e));
    // Cached exports from before tint palettes were applied: export again.
    let stale_tint = std::fs::read_to_string(dir.join("model.json"))
        .is_ok_and(|j| j.contains("\"tintpalettetex\"") && !j.contains("\"tint_applied\""));
    // Cached vehicle exports from before lamp lenses, reflectors, tyres and
    // lamps got their classes (classification.stand_in dates the export).
    let stale_vehicle = std::fs::read_to_string(dir.join("model.json"))
        .is_ok_and(|j| {
            // by shader hash: exports from before shader names were recovered have no names
            ["99ad6fc9", "389206d1", "03b27476", "5a693bbe", "29648098"].iter().any(|h| j.contains(h)) && !j.contains("\"stand_in\"")
                // and from before interior and trim stopped taking the body colour
                || j.contains("\"car_paint\"") && !j.contains("\"paint_rule\"")
                // and from before wiper masks, water stencils and runtime screen overlays were left out
                || (j.contains("bd71bdbf") || j.contains("fc34f08b") || j.contains("a800afccf01aa36b")) && !j.contains("\"hidden\"")
                // and from before rims stopped taking their placeholder tint palette
                || (j.contains("c5cdbdd2") || j.contains("99ad6fc9")) && j.contains("\"tintpalettetex\"") && !j.contains("a placeholder of marker colours")
                // and from before the 4 x u16 joint stream (0x4A) was read: body and
                // trim meshes that did not follow their bones
                || j.contains("\"skinned\"") && !j.contains("\"joint_format\"")
                // and from before the wheels' collision discs were read
                // and the inner wheels of dual pairs
                || (j.contains("\"bone\": \"wheel_") || j.contains("\"bone\":\"wheel_") || j.contains("\"wheel_discs\"")) && !j.contains("\"wheel_mounts_version\"")
                // and convertibles from before their roof clip was written
                || (j.contains("\"convertible_roof\"") || j.contains("\"wheel_mounts_version\"")) && !j.contains("\"roof_animation\"")
                // and from before brake discs and the vehicle's other clips
                || j.contains("\"wheel_mounts_version\"") && !j.contains("\"wheel_mounts_version\": 3")
                // and weapon parts from before their stand-in surface
                || j.contains("e68a4a98") && !j.contains("\"gunmetal\"")
        });
    // Cached animal parts from before part skins (eyes, tint-mask coats).
    let stale_animal = e.name.as_deref().is_some_and(|n| sixng_core::partskin::animal_root(n).is_some())
        && std::fs::read_to_string(dir.join("model.json")).is_ok_and(|j| !j.contains("\"part_skin\""));
    let opts = ExportOptions {
        out: cache.to_path_buf(),
        png: true,
        dds: false,
        raw: false,
        force: head || stale_tint || stale_vehicle || stale_animal,
        skip_embedded_textures: false,
        body_shape: Vec::new(),
        head_skin: HEAD_SKIN.load(std::sync::atomic::Ordering::Relaxed),
        paint: None,
        tex_remap: Default::default(), ped_tints: Default::default(),
    };
    let item = export::export_entry(a, e, &opts)?;
    let lods = lods_in(&dir);
    if lods.is_empty() {
        return Err(anyhow::anyhow!(
            "{} exported no model.glb (status {}, files {:?}{})",
            e.id,
            item.status,
            item.files,
            item.error.map(|x| format!(", {x}")).unwrap_or_default()
        ));
    }
    let lod = lod_index(&lods, lod);
    let info: serde_json::Value = std::fs::read(dir.join("model.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if let Some(h) = info.get("head_skin") {
        let n = h["count"].as_u64().unwrap_or(0) as usize;
        if n > 0 {
            *HEAD_SKIN_SEEN.lock().unwrap() = Some((
                h["index"].as_u64().unwrap_or(0) as usize,
                n,
                h["source"].as_str().unwrap_or("").to_string(),
            ));
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
    let r = regex::RegexBuilder::new(re)
        .case_insensitive(true)
        .build()?;
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
    best.filter(|e| matches!(e.rtype.as_deref(), Some("ft") | Some("dr") | Some("dd")))
        .ok_or_else(|| anyhow::anyhow!("no drawable/fragment named like {re}"))
}

/// Same rule as sixng_core::export::sanitize (kept local: it is private there).
pub(crate) fn export_sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Entry ids of the parts an animal root or a character outfit is put
/// together from (None for ordinary models).
pub(crate) fn assembly_ids(
    ws: &Workspace,
    idx: &Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>,
    id: &str,
) -> Option<Vec<String>> {
    if let Some((archive, prefix)) = id.split_once("#outfit:") {
        let (_, ix) = index_for(ws, archive, idx).ok()?;
        return Some(
            crate::assemble::outfit_parts(&ix, prefix)
                .iter()
                .map(|e| e.id.clone())
                .collect(),
        );
    }
    let (archive, _) = id.split_once('@')?;
    let (_, ix) = index_for(ws, archive, idx).ok()?;
    let (e, member) = export::ModelResolver::new(&ix.entries)
        .resolve(&ix.entries, id)
        .ok()?;
    if member.is_some() {
        return None;
    }
    let name = e.name.as_deref()?;
    if e.rtype.as_deref() == Some("ft") && crate::assemble::is_animal(name) {
        let parts = crate::assemble::animal_parts(&ix, name);
        if !parts.is_empty() {
            return Some(
                std::iter::once(e.id.clone())
                    .chain(parts.iter().map(|p| p.id.clone()))
                    .collect(),
            );
        }
    }
    None
}

/// sRGB [0,1] triple as an egui colour.
fn srgb32(c: [f32; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_discovery_keeps_physical_numbers_across_holes() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("sixng-lod-discovery-test-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        // Slots 2 and 3 export as model.glb and model_lod3.glb.
        for name in ["model.glb", "model_lod3.glb", "model_lod5.glb"] {
            std::fs::write(dir.join(name), []).unwrap();
        }
        let lods = lods_in(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            lods,
            [
                dir.join("model.glb"),
                dir.join("model_lod3.glb"),
                dir.join("model_lod5.glb")
            ]
        );
        assert_eq!(
            lods.iter().map(|p| lod_level(p)).collect::<Vec<_>>(),
            [None, Some(3), Some(5)]
        );
        for (level, index) in [(0, 0), (2, 0), (3, 1), (4, 1), (5, 2), (7, 2)] {
            assert_eq!(lod_index(&lods, level), index);
        }
    }
}

/// Hand a finished load to the viewer, unless a newer request was made
/// meanwhile.
fn finish_load(slot: &Pending, latest: &std::sync::atomic::AtomicU64, gen: u64, r: Result<Loaded, String>) {
    let mut s = slot.lock().unwrap();
    if latest.load(std::sync::atomic::Ordering::SeqCst) == gen {
        *s = Some((gen, r));
    }
}
