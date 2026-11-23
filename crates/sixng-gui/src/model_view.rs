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
            paint: m.materials.iter().any(|x| x.class == "car_paint" || x.parts),
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
    /// Characters: the outfits of the game's outfit list (`pedlook`) and the
    /// one shown.
    pub look: Option<LookInfo>,
}

/// A game animation clip on the character's skeleton.
#[derive(Default)]
pub struct PedClip {
    /// `<dictionary>/<clip>` typed or passed with `--anim`.
    pub want: String,
    /// What `anim` was loaded from, with the model it was mapped to.
    loaded_key: String,
    pub anim: Option<glb::Anim>,
    /// Position 0..1 and whether it advances.
    pub pos: f32,
    pub playing: bool,
    pub status: String,
    loading: bool,
    pending: Arc<Mutex<Option<(String, Result<(glb::Anim, String), String>)>>>,
    last: Option<std::time::Instant>,
}

/// A character's pose: angles in degrees on the main bones of the shared
/// skeleton. Zero everywhere is the stored pose (arms out).
#[derive(Clone, Copy, Default, PartialEq)]
pub struct PedPose {
    /// Per arm (left, right): raise (+ up, - down), swing (+ forward).
    pub arm: [[f32; 2]; 2],
    pub elbow: [f32; 2],
    /// Per leg (left, right): lift (+ forward), spread (+ outward).
    pub leg: [[f32; 2]; 2],
    pub knee: [f32; 2],
    /// Bend forward, twist to the left, lean to the left.
    pub spine: [f32; 3],
    /// Nod down, turn to the left, tilt to the left.
    pub head: [f32; 3],
}

impl PedPose {
    pub const PRESETS: &'static [&'static str] = &["rest", "arms down", "walk", "sit", "wave"];

    pub fn preset(name: &str) -> Option<PedPose> {
        // the stored pose already holds the arms about 35 degrees down and a little forward
        let down = [-52.0, -12.0];
        Some(match name {
            "rest" => PedPose::default(),
            "arms down" | "down" => PedPose { arm: [down, down], elbow: [8.0, 8.0], ..Default::default() },
            "walk" => PedPose { arm: [[-52.0, 16.0], [-52.0, -36.0]], elbow: [22.0, 12.0], leg: [[-18.0, 0.0], [26.0, 0.0]], knee: [28.0, 8.0], spine: [3.0, 0.0, 0.0], ..Default::default() },
            "sit" => PedPose { arm: [[-48.0, 6.0], [-48.0, 6.0]], elbow: [48.0, 48.0], leg: [[86.0, 6.0], [86.0, 6.0]], knee: [88.0, 88.0], ..Default::default() },
            "wave" => PedPose { arm: [down, [38.0, 10.0]], elbow: [8.0, 96.0], head: [0.0, -8.0, 0.0], ..Default::default() },
            _ => return None,
        })
    }

    /// Bone name and rotation in model axes (x to the character's left, y up,
    /// z forward; degrees about x, y, z) for every bone the pose turns.
    fn bones(&self) -> Vec<(String, [f32; 3])> {
        let mut v = Vec::new();
        for (i, s) in ["L", "R"].iter().enumerate() {
            // the left arm points along +x, the right along -x
            let sg = if i == 0 { 1.0 } else { -1.0 };
            v.push((format!("SKEL_{s}_UpperArm"), [0.0, -sg * self.arm[i][1], sg * self.arm[i][0]]));
            v.push((format!("SKEL_{s}_Forearm"), [0.0, -sg * self.elbow[i], 0.0]));
            v.push((format!("SKEL_{s}_Thigh"), [-self.leg[i][0], 0.0, sg * self.leg[i][1]]));
            v.push((format!("SKEL_{s}_Calf"), [self.knee[i], 0.0, 0.0]));
        }
        for b in ["SKEL_Spine1", "SKEL_Spine2", "SKEL_Spine3", "SKEL_Spine4"] {
            v.push((b.to_string(), [self.spine[0] / 4.0, self.spine[1] / 4.0, -self.spine[2] / 4.0]));
        }
        for b in ["SKEL_Neck1", "SKEL_Head"] {
            v.push((b.to_string(), [self.head[0] / 2.0, self.head[1] / 2.0, -self.head[2] / 2.0]));
        }
        v.retain(|x| x.1 != [0.0; 3]);
        v
    }
}

/// A character's outfits, for the Parts panel.
#[derive(Clone, Default)]
pub struct LookInfo {
    pub model: String,
    pub labels: Vec<String>,
    pub current: usize,
    /// Body-shape preset of the outfit shown.
    pub body_shape: Option<String>,
    /// Strand-hair assets of the outfit shown: (slot, placeholder name, asset GUID).
    pub strands: Vec<(String, String, String)>,
    /// Their stored tints by slot: palette name hash and row of tint 0.
    pub strand_tints: Vec<(String, u32, u8)>,
    /// What the viewer did or could not do (skin tone source, unresolved items).
    pub notes: Vec<String>,
    /// Strand hair found for the outfit's hair assets (`hairlink`).
    pub hair: Vec<HairPick>,
}

/// The grooms of one hair asset of an outfit.
#[derive(Clone)]
pub struct HairPick {
    /// hair, brows, beard, lashes, body hair
    pub slot: String,
    /// The placeholder the grooms were placed under.
    pub placeholder: String,
    /// Model probability of the placement (0..1); under 0.5 it is a guess
    /// and starts hidden.
    pub confidence: f32,
    pub grooms: Vec<u64>,
    /// Pieces left out because their strand roots do not lie on this head.
    pub off_head: usize,
    /// Shown when the character loads: confidence of 50 % or more, or 20 %
    /// or more with every piece rooted on this head.
    pub shown: bool,
    /// Stored colour (sRGB): row tint 0 of `ped_tint_hair`, when the item
    /// names that palette.
    pub colour: Option<[u8; 3]>,
}

/// Colouring of one loaded part of an outfit.
#[derive(Clone, Default)]
pub struct PartFx {
    /// Tint palette (name hash) and row of tint 0.
    pub tint: Option<(u32, u8)>,
    /// Flat colour of the item (sRGB).
    pub colour: Option<[u8; 3]>,
    /// Colour word of the item's colour tag.
    pub tag: Option<&'static str>,
    pub head: bool,
    /// A skin mesh without a colour texture: takes the head's neck tone.
    pub body: bool,
    /// Heads: the face textures the outfit item names (colour, normal).
    pub skins: Vec<(u64, u64)>,
    /// Skin, head, eyes and teeth: skinned to the shared skeleton only, so
    /// the body-shape rig moves them. Everything else follows the skin.
    pub skeletal: bool,
    /// A garment that covers skin (top, jacket, trousers, dress, shoes,
    /// gloves): skin right under it is hidden.
    pub covers: bool,
    /// Clothing layer: 1 = worn under a jacket or coat (top, bottom, one
    /// piece), 2 = the jacket or coat. Layer 1 is hidden where layer 2
    /// covers it.
    pub layer: u8,
    /// A shoe with a heel (name token `sh`, `mh` or `hh`; flat ones have
    /// `fh`): the feet are posed into it.
    pub heel: bool,
}

/// A finished load, tagged with the request it answers.
type Pending = Arc<Mutex<Option<(u64, Result<Loaded, String>)>>>;

/// Face variant the viewer exports ped heads with (`ExportOptions::head_skin`).
pub static HEAD_SKIN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Body shape the viewer exports outfit parts with: (weight -1..1, muscle
/// 0..1), the game's body-shape rig on the shared skeleton
/// (`ExportOptions::body_shape`, `allweight` / `allmuscle`).
pub static BODY_SHAPE: Mutex<(f32, f32)> = Mutex::new((0.0, 0.0));
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
    /// `--hair-width MM` for a character's own hair.
    pub hair_width: Option<f32>,
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
    /// Steering: angle of the front wheels in radians (positive = left).
    pub steer: f32,
    /// `steerWheelMult` of the loaded vehicle (1 when it has none).
    steer_mult: f32,
    /// The loaded vehicle's steering lock in radians (`fSteeringLock`), or
    /// a default of 35 degrees with `steer_lock_known` false.
    steer_lock: f32,
    steer_lock_known: bool,
    /// The steering wheel's joint and the axis it turns about (joint space).
    steer_wheel: Option<(usize, Vec3)>,
    /// How hot the brake discs glow, 0 (cold) to 1.
    pub brake_heat: f32,
    /// How far the side windows are rolled down, 0 (up) to 1.
    pub windows_down: f32,
    /// The window movers of the loaded model: joint index, the downward
    /// direction along the glass and a point on its bottom edge, both in
    /// the joint's own space.
    window_movers: Vec<(usize, Vec3, Vec3)>,
    /// The moving parts of an aircraft among the loaded model's joints.
    air: Vec<AirPart>,
    /// Rotor and propeller speed, 0 (still) to 1; the angle they are at.
    pub rotor_speed: f32,
    rotor_angle: f32,
    /// Control surfaces in radians: flaps and air brakes out, ailerons,
    /// elevators and rudder either way.
    pub flaps: f32,
    pub ailerons: f32,
    pub elevators: f32,
    pub rudder: f32,
    pub airbrakes: f32,
    /// Retractable landing gear: legs and doors, and how far it is up (0 to 1).
    gear: Vec<GearPart>,
    pub gear_up: f32,
    /// The chosen configuration's key (its part colours), and what the
    /// loaded model's vertices currently show: (model file, look index).
    part_look_want: Option<String>,
    /// Automation: the configuration to choose when a vehicle's data is applied (0-based).
    pub cfg_request: Option<usize>,
    part_look_done: Option<(PathBuf, usize)>,
    /// The part colours on show (256 x RGBA, by part id), for the fitted
    /// wheels: a rim's and a tyre's vertices carry part ids of the vehicle.
    part_look_table: Option<Arc<Vec<[u8; 4]>>>,
    /// The part colours the uploaded wheels were coloured with.
    wheel_look: Option<Arc<Vec<[u8; 4]>>>,
    /// Vehicle lamps that are on.
    pub lamps: Lamps,
    /// The loaded model has lamp bones.
    has_lamps: bool,
    /// The siren sound on loop (the user's choice among the game's).
    pub siren_sound: crate::siren_sound::SirenSound,
    /// The siren pattern playing: 0 off, n the nth of `sirens`.
    pub siren: usize,
    /// The siren patterns of the loaded vehicle that reach its bones.
    sirens: Vec<Siren>,
    /// The loaded vehicle's light patterns and the names the configuration
    /// on show links.
    veh_lights: Option<Arc<crate::vehicle_db::Lights>>,
    cfg_patterns: Vec<String>,
    siren_dirty: bool,
    /// Per joint of the loaded model: the colour of its lamp's light set
    /// (largest channel 1), where the vehicle data has one.
    lamp_rgb: Vec<Option<[f32; 3]>>,
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
    /// Head bone matrix the hair models were last placed with, and how many.
    hair_head: Option<Mat4>,
    hair_head_n: usize,
    /// Animation clip for the character shown (`anim.rpf`): what is asked
    /// for (`<dictionary>/<clip>`), what is loaded, and the playback state.
    pub ped_clip: PedClip,
    /// Pose of the character shown (degrees), see `PedPose`.
    pub ped_pose: PedPose,
    /// Body shape of the character shown (`BODY_SHAPE` when it was loaded).
    body_shape_shown: (f32, f32),
    pub wheels_on: bool,
    pub wheel_rim: String,
    /// Rim colour (linear). The game sets it per vehicle at runtime and no
    /// decoded table holds it, so it starts as a plain alloy.
    pub rim_colour: [f32; 3],
    rim_applied: [f32; 3],
    /// Material names of the loaded wheel models (rim, tyre).
    wheel_mats: Vec<Vec<String>>,
    /// The loaded wheel models and their vertex positions as loaded: the
    /// rim size is baked into the vertices before upload.
    wheel_src: Vec<Loaded>,
    wheel_orig: Vec<Vec<Vec<[f32; 3]>>>,
    /// Rim radius / tyre radius the wheels are shaped to; 0 = as modelled.
    pub rim_ratio: f32,
    /// Take the rim size from the configuration's wheel fitment record.
    pub rim_auto: bool,
    /// The loaded vehicle's wheelScale / wheelScaleRear as stored.
    veh_wheel_scale: Option<(f32, f32)>,
    rim_baked: f32,
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
    /// Finished wheel loads, each with the rim|tyre key it was asked for:
    /// only the one for the current key is used (an older request can
    /// finish after a newer one was made).
    wheel_pending: Arc<Mutex<Vec<(String, Result<Vec<Loaded>, String>)>>>,
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
    /// The character outfit the selection was made for (`model#outfit`).
    pub look_key: String,
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
            look_key: String::new(),
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
            steer: 0.0,
            steer_mult: 1.0,
            steer_lock: 35f32.to_radians(),
            steer_lock_known: false,
            steer_wheel: None,
            brake_heat: 0.0,
            windows_down: 0.0,
            window_movers: Vec::new(),
            air: Vec::new(),
            rotor_speed: 0.0,
            rotor_angle: 0.0,
            flaps: 0.0,
            ailerons: 0.0,
            elevators: 0.0,
            rudder: 0.0,
            airbrakes: 0.0,
            gear: Vec::new(),
            gear_up: 0.0,
            part_look_want: None,
            cfg_request: None,
            part_look_done: None,
            part_look_table: None,
            wheel_look: None,
            lamps: Lamps::default(),
            has_lamps: false,
            siren_sound: Default::default(),
            siren: 0,
            sirens: Vec::new(),
            veh_lights: None,
            cfg_patterns: Vec::new(),
            siren_dirty: false,
            lamp_rgb: Vec::new(),
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
            hair_head: None,
            hair_head_n: 0,
            ped_clip: PedClip::default(),
            ped_pose: PedPose::default(),
            body_shape_shown: (0.0, 0.0),
            wheels_on: true,
            wheel_rim: "rim_muscle_001_hi".into(),
            rim_colour: RIM_ALLOY,
            rim_applied: RIM_ALLOY,
            wheel_mats: Vec::new(),
            wheel_src: Vec::new(),
            wheel_orig: Vec::new(),
            rim_ratio: 0.0,
            rim_auto: true,
            veh_wheel_scale: None,
            rim_baked: -1.0,
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
            wheel_pending: Arc::new(Mutex::new(Vec::new())),
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
                        // Long hair has many points per strand: keep one groom
                        // under about 1.5 million ribbon vertices (a vertex
                        // buffer may not exceed 256 MiB).
                        let share = share.min(375_000.0 / (g.stored_strands() * k).max(1) as f32);
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
                    let mut gm = glb::load(&p, &dir)?;
                    gm.materials.iter_mut().for_each(|x| x.mr = Some([0.0, 0.85]));
                    out.push(gm);
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
        if !self.hair.settled() || self.auto.then.is_some() || self.auto.find_hair || self.hair.fitting || self.ped_clip.loading || (!self.ped_clip.want.trim().is_empty() && self.ped_clip.anim.is_none() && self.ped_clip.status.starts_with("loading")) {
            return false;
        }
        let Some(l) = self
            .loaded
            .as_ref()
            // a vehicle: it has wheels, or part colours (boats, aircraft)
            .filter(|l| l.info["wheel_mounts"].as_array().is_some_and(|a| !a.is_empty()) || l.info["part_colours"].is_object())
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

    /// Load the clip asked for (on a worker thread) and advance playback.
    fn update_ped_clip(&mut self, ctx: &egui::Context) {
        if let Some((key, r)) = self.ped_clip.pending.lock().unwrap().take() {
            self.ped_clip.loading = false;
            self.ped_clip.loaded_key = key;
            match r {
                Ok((a, note)) => {
                    self.ped_clip.status = note;
                    self.ped_clip.anim = Some(a);
                }
                Err(e) => {
                    self.ped_clip.status = e;
                    self.ped_clip.anim = None;
                }
            }
        }
        let Some(l) = self.loaded.as_ref().filter(|l| l.look.is_some() && !l.model.joints.is_empty()) else { return };
        let want = self.ped_clip.want.trim().to_string();
        let key = format!("{want}#{}#{}", l.model.joints.len(), l.model.joints[0].name);
        if want.is_empty() {
            self.ped_clip.anim = None;
            self.ped_clip.loaded_key.clear();
        } else if !self.ped_clip.loading && self.ped_clip.loaded_key != key {
            self.ped_clip.loading = true;
            self.ped_clip.status = format!("loading {want} …");
            let names: Vec<String> = l.model.joints.iter().map(|j| j.name.clone()).collect();
            let (game, work, slot, ctx2) = (PathBuf::from(&self.game), PathBuf::from(&self.work), self.ped_clip.pending.clone(), ctx.clone());
            std::thread::spawn(move || {
                let r = (|| -> anyhow::Result<(glb::Anim, String)> {
                    let (dq, cq) = want.rsplit_once('/').ok_or_else(|| anyhow::anyhow!("write it as <dictionary>/<clip> (`sixng clips --name walk` lists dictionaries)"))?;
                    let (_, d) = sixng_core::anim::load_dictionary(&game, &work, dq)?;
                    let c = d.clip(cq).ok_or_else(|| anyhow::anyhow!("no clip {cq:?} in {}; it has: {}", d.name, d.clips.iter().map(|c| c.short_name()).collect::<Vec<_>>().join(", ")))?;
                    let joints: Vec<sixng_core::gltf::Joint> = names.iter().map(|n| sixng_core::gltf::Joint { name: n.clone(), parent: None, bind: [0.0; 16], pose: None }).collect();
                    let (ch, stats) = sixng_core::outfit::clip_channels(&d, c, &joints);
                    // RAGE (Z-up) values to the viewer's Y-up, as the GLB export does
                    let channels: Vec<glb::AnimChannel> = ch
                        .into_iter()
                        .map(|c| {
                            let (path, f): (u8, fn([f32; 4]) -> [f32; 4]) = match c.path {
                                sixng_core::gltf::AnimPath::Translation => (0, |v| [v[0], v[2], -v[1], 0.0]),
                                sixng_core::gltf::AnimPath::Rotation => (1, |v| [v[0], v[2], -v[1], v[3]]),
                                sixng_core::gltf::AnimPath::Scale => (2, |v| [v[0], v[2], v[1], 0.0]),
                            };
                            glb::AnimChannel { joint: c.joint, path, times: c.times, values: c.values.into_iter().map(f).collect() }
                        })
                        .collect();
                    let duration = channels.iter().filter_map(|c| c.times.last().copied()).fold(c.duration, f32::max).max(1e-3);
                    let note = format!("{} bone tracks of {} on this skeleton, {:.2} s. The clip's own movement over the ground is left out (it plays in place).", stats["bone_tracks_on_joints"], stats["bone_tracks"], duration);
                    Ok((glb::Anim { name: want.clone(), duration, channels }, note))
                })()
                .map_err(|e| format!("{e:#}"));
                *slot.lock().unwrap() = Some((key, r));
                ctx2.request_repaint();
            });
        }
        // playback
        let now = std::time::Instant::now();
        let dt = self.ped_clip.last.replace(now).map_or(0.0, |t| (now - t).as_secs_f32()).min(0.1);
        if let (true, Some(a)) = (self.ped_clip.playing, &self.ped_clip.anim) {
            self.ped_clip.pos = (self.ped_clip.pos + dt / a.duration).fract();
            ctx.request_repaint();
        }
    }

    /// Body shape for characters (weight, muscle), before a model is opened.
    pub fn set_body_shape(&mut self, shape: (f32, f32)) {
        *BODY_SHAPE.lock().unwrap() = shape;
        self.body_shape_shown = shape;
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

    /// Character panel: the outfit of the game's outfit list, the body shape
    /// and what the viewer filled in. Returns the query to reload with.
    fn look_ui(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let l = self.loaded.as_ref()?;
        let look = l.look.as_ref()?;
        let mut reload = None;
        ui.heading("Character");
        egui::Grid::new("look_grid").num_columns(2).show(ui, |ui| {
            ui.label("Outfit");
            let text = |k: usize| format!("{}: {}", k + 1, look.labels.get(k).map(String::as_str).unwrap_or("?"));
            egui::ComboBox::from_id_salt("look_outfit").selected_text(text(look.current)).width(240.0).show_ui(ui, |ui| {
                for k in 0..look.labels.len() {
                    let label = if k == 0 { format!("{}  (default)", text(k)) } else { text(k) };
                    if ui.selectable_label(k == look.current, label).clicked() && k != look.current {
                        let base = l.base.split("#look=").next().unwrap_or(&l.base);
                        reload = Some(format!("{base}#look={k}"));
                    }
                }
            });
            ui.end_row();
        });
        ui.label(egui::RichText::new("The outfits the game lists for this character, in its order. The first one is shown by default.").weak().small());
        let mut shape = *BODY_SHAPE.lock().unwrap();
        let before = shape;
        let mut released = false;
        ui.horizontal(|ui| {
            ui.label("Body");
            released |= ui.add(egui::Slider::new(&mut shape.0, -1.0..=1.0).text("weight")).on_hover_text("The game's body-shape rig on the shared skeleton (all weight controls). Body and clothes follow the same bones, so the clothes change with the body.").drag_stopped();
        });
        ui.horizontal(|ui| {
            ui.label("        ");
            released |= ui.add(egui::Slider::new(&mut shape.1, 0.0..=1.0).text("muscle")).drag_stopped();
            if ui.button("Reset").clicked() {
                shape = (0.0, 0.0);
                released = true;
            }
        });
        if let Some(p) = &look.body_shape {
            ui.label(egui::RichText::new(format!("Body-shape preset of this outfit: {p} (its control values are not applied; the sliders start neutral).")).weak().small());
        }
        if shape != before {
            *BODY_SHAPE.lock().unwrap() = shape;
        }
        // reload when a drag ends (or a value was typed), not on every step
        if (released || !ui.ctx().input(|i| i.pointer.any_down())) && shape != self.body_shape_shown {
            self.body_shape_shown = shape;
            reload = reload.or_else(|| Some(self.query.clone()));
        }
        egui::CollapsingHeader::new("Pose").default_open(true).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for name in PedPose::PRESETS {
                    if ui.button(*name).clicked() {
                        self.ped_pose = PedPose::preset(name).unwrap_or_default();
                    }
                }
            });
            let p = &mut self.ped_pose;
            let pair = |ui: &mut egui::Ui, label: &str, a: &mut f32, b: &mut f32, lo: f32, hi: f32| {
                ui.label(label);
                ui.add(egui::DragValue::new(a).range(lo..=hi).speed(1.0).suffix("°").prefix("L "));
                ui.add(egui::DragValue::new(b).range(lo..=hi).speed(1.0).suffix("°").prefix("R "));
                ui.end_row();
            };
            egui::Grid::new("pose_grid").num_columns(3).show(ui, |ui| {
                let [al, ar] = &mut p.arm;
                pair(ui, "arm raise", &mut al[0], &mut ar[0], -90.0, 90.0);
                pair(ui, "arm swing", &mut al[1], &mut ar[1], -90.0, 120.0);
                let [el, er] = &mut p.elbow;
                pair(ui, "elbow", el, er, 0.0, 140.0);
                let [ll, lr] = &mut p.leg;
                pair(ui, "leg lift", &mut ll[0], &mut lr[0], -45.0, 110.0);
                pair(ui, "leg spread", &mut ll[1], &mut lr[1], -20.0, 60.0);
                let [kl, kr] = &mut p.knee;
                pair(ui, "knee", kl, kr, 0.0, 140.0);
            });
            ui.horizontal(|ui| {
                ui.label("spine");
                ui.add(egui::DragValue::new(&mut p.spine[0]).range(-40.0..=80.0).suffix("°").prefix("bend "));
                ui.add(egui::DragValue::new(&mut p.spine[1]).range(-60.0..=60.0).suffix("°").prefix("twist "));
                ui.add(egui::DragValue::new(&mut p.spine[2]).range(-40.0..=40.0).suffix("°").prefix("lean "));
            });
            ui.horizontal(|ui| {
                ui.label("head");
                ui.add(egui::DragValue::new(&mut p.head[0]).range(-50.0..=60.0).suffix("°").prefix("nod "));
                ui.add(egui::DragValue::new(&mut p.head[1]).range(-80.0..=80.0).suffix("°").prefix("turn "));
                ui.add(egui::DragValue::new(&mut p.head[2]).range(-40.0..=40.0).suffix("°").prefix("tilt "));
            });
            ui.label(egui::RichText::new("Angles on the main bones of the shared skeleton; zero is the stored pose. Drag a value or type it.").weak().small());
        });
        egui::CollapsingHeader::new("Animation").default_open(true).show(ui, |ui| {
            let female = look.model.is_empty() || l.parts.iter().any(|p| l.selected.contains(&p.off) && p.name.contains("_fr1_"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.ped_clip.want).hint_text("<dictionary>/<clip>").desired_width(250.0));
                if ui.button("Walk").on_hover_text("A walk clip of the game for this body type").clicked() {
                    self.ped_clip.want = format!("gameplay@locomotion@{}_character@{}@normal@standing@walk@stance_neutral/move", if female { "female" } else { "male" }, if female { "kat" } else { "raul" });
                    self.ped_clip.playing = true;
                    self.ped_pose = PedPose::default();
                }
                if ui.button("None").clicked() {
                    self.ped_clip.want.clear();
                    self.ped_clip.playing = false;
                }
            });
            if self.ped_clip.anim.is_some() {
                ui.horizontal(|ui| {
                    if ui.button(if self.ped_clip.playing { "⏸" } else { "▶" }).clicked() {
                        self.ped_clip.playing = !self.ped_clip.playing;
                    }
                    ui.add(egui::Slider::new(&mut self.ped_clip.pos, 0.0..=1.0).show_value(false));
                });
            }
            if !self.ped_clip.status.is_empty() {
                ui.label(egui::RichText::new(&self.ped_clip.status).weak().small());
            }
            ui.label(egui::RichText::new("A clip of anim.rpf on this skeleton (`sixng clips --name <regex>` lists them). The Pose angles are added on top.").weak().small());
        });
        for n in &look.notes {
            ui.label(egui::RichText::new(n).weak().small());
        }
        if !look.hair.is_empty() {
            ui.label("Hair");
            for h in &look.hair {
                let mut on = !h.grooms.is_empty() && h.grooms.iter().all(|g| self.hair.selected.contains(g));
                let name = h.placeholder.trim_end_matches("_full");
                let mut text = format!("{}: {name} ({:.0} %)", h.slot, h.confidence * 100.0);
                if h.off_head > 0 {
                    text += &format!(", {} piece(s) not on this head left out", h.off_head);
                }
                if h.colour.is_some() {
                    text += ", stored colour";
                }
                if ui.checkbox(&mut on, text).on_hover_text("The strand grooms placed for this hair asset. Nothing in the game files links a hair asset to its strand files; they are placed by name words, archive order, piece counts and where the strand roots lie. The percentage is that model's probability of this placement. Shown at the start: 50 % or more, or 20 % or more when every piece is rooted on this head (a probable style, not a certain one). Pieces whose strand roots do not lie on this head are left out.").changed() {
                    self.hair.selected.retain(|g| !h.grooms.contains(g));
                    if on {
                        self.hair.selected.extend(h.grooms.iter().copied());
                    }
                }
            }
            ui.horizontal(|ui| {
                ui.label("colour");
                ui.color_edit_button_rgb(&mut self.hair.colour);
                ui.label(egui::RichText::new("(stored for items that name the hair palette; most leave it to the game, so pick one)").weak().small());
            });
        }
        let unplaced: Vec<&str> = look.strands.iter().filter(|s| !look.hair.iter().any(|h| h.slot == s.0)).map(|s| s.1.as_str()).collect();
        if !unplaced.is_empty() {
            ui.label(egui::RichText::new(format!("Strand hair with no grooms placed: {}", unplaced.join(", "))).weak().small());
        }
        ui.separator();
        reload
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
        let (parts, selected, base, look) = (l.parts.clone(), l.selected.clone(), l.base.clone(), l.look.clone());
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
            look,
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
                if let Some(look) = &l.look {
                    // a character: its confident hair, grown to full density
                    let key = format!("{}#{}", look.model, look.current);
                    if self.hair.look_key != key {
                        self.hair.look_key = key;
                        self.hair.selected = look.hair.iter().filter(|h| h.shown).flat_map(|h| h.grooms.iter().copied()).collect();
                        if let Some(c) = look.hair.iter().find(|h| h.slot == "hair").or(look.hair.first()).and_then(|h| h.colour) {
                            self.hair.colour = c.map(|x| x as f32 / 255.0);
                        }
                        if !look.hair.is_empty() {
                            self.hair.full_coat = true;
                            self.hair.skin_colour = false;
                            // hair strands are thinner than a pixel at
                            // viewing distance: wider ribbons read as hair
                            self.hair.density = 1.0;
                            if self.hair.width_mm == 1.0 {
                                self.hair.width_mm = self.auto.hair_width.unwrap_or(6.0);
                            }
                            if self.hair.colour == Hair::default().colour {
                                self.hair.colour = [0.09, 0.06, 0.045];
                            }
                        }
                    }
                } else if !self.hair.look_key.is_empty() {
                    self.hair.look_key.clear();
                    if self.hair.fur.is_none() {
                        self.hair.selected.clear();
                    }
                }
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
        // a vehicle has wheels, or part colours (boats, aircraft without
        // wheel bones): those get their configurations and paint too
        let wheeled = l.info["wheel_mounts"].as_array().is_some_and(|a| !a.is_empty());
        if !wheeled && !l.info["part_colours"].is_object() {
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
            if !wheeled {
                return;
            }
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
                self.apply_config(v, self.cfg_request.filter(|i| *i < v.configs.len()).unwrap_or(0));
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
        match &v.display {
            // the game's own name, with the model name kept beside it
            Some(d) => {
                ui.heading(format!("Vehicle: {d}"));
                ui.label(format!("model {key}")).on_hover_text("The game's text for this model's name and make (vehicles.json display_name and make). The model name and id stay as they are in the files.");
            }
            None => {
                ui.heading(format!("Vehicle: {key}"));
            }
        }
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

    /// The rim radius / tyre radius the wheels are shaped to. A
    /// configuration's wheel fitment record gives it on cars. Two-wheelers
    /// have no such record; there wheelScale is the rim radius (0.217 on a
    /// BMX whose tyre is 0.268, 0.254 / 0.223 on a dirt bike), so the ratio
    /// is wheelScale over the tyre radius of the collision discs. On cars
    /// wheelScale is not the rim radius, so it is only used where no
    /// fitment record exists and the result is a plausible wheel.
    fn rim_ratio_now(&self) -> f32 {
        if self.rim_ratio > 0.0 || !self.rim_auto {
            return self.rim_ratio;
        }
        let Some((front, rear)) = self.veh_wheel_scale else { return 0.0 };
        let mounts = self.mounts();
        let ratios: Vec<f32> = mounts
            .iter()
            .zip(self.discs())
            .filter_map(|((bone, _), d)| {
                let r = d?[0];
                (r > 0.0).then(|| if is_rear(bone) { rear } else { front } / r)
            })
            .collect();
        if ratios.is_empty() {
            return 0.0;
        }
        let mean = ratios.iter().sum::<f32>() / ratios.len() as f32;
        if (0.55..=0.95).contains(&mean) { mean } else { 0.0 }
    }

    fn apply_scale(&mut self, v: &crate::vehicle_db::Vehicle) {
        self.veh_wheel_scale = v.wheel_scale.map(|f| (f, v.wheel_scale_rear.unwrap_or(f)));
        self.steer_mult = v.steer_wheel_mult.filter(|m| *m > 0.0).unwrap_or(1.0);
        if !matches!((&self.veh_lights, &v.lights), (Some(a), Some(b)) if Arc::ptr_eq(a, b)) {
            self.veh_lights = v.lights.clone();
            self.cfg_patterns.clear();
            self.siren_dirty = true;
        }
        let lock = v.steer_lock.filter(|l| *l > 0.01 && *l < 1.6);
        self.steer_lock_known = lock.is_some();
        self.steer_lock = lock.unwrap_or(35f32.to_radians());
        self.steer = self.steer.clamp(-self.steer_lock, self.steer_lock);
        if self.wheel_auto {
            if let Some(f) = v.wheel_scale {
                self.wheel_scale = f * WHEEL_FACTOR;
                self.wheel_scale_rear = v.wheel_scale_rear.unwrap_or(f) * WHEEL_FACTOR;
            }
        }
    }

    fn apply_config(&mut self, v: &crate::vehicle_db::Vehicle, i: usize) {
        let c = &v.configs[i];
        // A configuration without a colour of its own (stored as pure
        // magenta) takes the most frequent colour of the vehicle's first
        // paint list that has one.
        let listed = || v.palettes.iter().flatten().map(|p| p.0).find(|c| !crate::vehicle_db::is_placeholder(*c));
        if let Some(col) = c.colour.or_else(listed) {
            self.look.paint = col;
        }
        if self.rim_auto {
            self.rim_ratio = c.rim_ratio.unwrap_or(0.0);
        }
        self.part_look_want = c.parts_key.clone();
        self.cfg_patterns = c.light_patterns.clone();
        self.siren_dirty = true;
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
    /// The turn steering gives what is fitted at a wheel bone (shared
    /// wheels, brake discs, calipers): a rotation about the vertical through
    /// the wheel's `wheel_turn_*` ancestor; identity for unsteered wheels.
    fn steer_at(&self, bone: &str) -> Mat4 {
        let Some(l) = self.loaded.as_ref().filter(|_| self.steer != 0.0) else { return Mat4::IDENTITY };
        let js = &l.model.joints;
        let mut k = js.iter().position(|j| j.name.eq_ignore_ascii_case(bone));
        let mut depth = 0;
        while let Some(i) = k.filter(|_| depth < 64) {
            if is_steer_bone(&js[i].name) {
                let pivot = Mat4::from_cols_array(&js[i].ibm).inverse().w_axis.truncate();
                return Mat4::from_translation(pivot) * Mat4::from_rotation_y(self.steer) * Mat4::from_translation(-pivot);
            }
            k = js[i].parent.filter(|p| *p != i);
            depth += 1;
        }
        Mat4::IDENTITY
    }

    /// The move of the landing gear leg a bone hangs on (model space):
    /// what is fitted at the bone (a wheel, a brake) goes up with the leg.
    fn gear_at(&self, bone: &str) -> Mat4 {
        let Some(l) = self.loaded.as_ref().filter(|_| self.gear_up > 0.0 && !self.gear.is_empty()) else { return Mat4::IDENTITY };
        let js = &l.model.joints;
        let turn = |g: &GearPart| {
            let to_model = Mat4::from_cols_array(&js[g.joint].ibm).inverse();
            let (pivot, axis) = g.follow.unwrap_or_else(|| (to_model.w_axis.truncate(), to_model.transform_vector3(g.axis).normalize_or_zero()));
            Mat4::from_translation(pivot) * Mat4::from_quat(Quat::from_axis_angle(axis, g.angle * self.gear_up.clamp(0.0, 1.0))) * Mat4::from_translation(-pivot)
        };
        let start = js.iter().position(|j| j.name.eq_ignore_ascii_case(bone));
        let (mut k, mut depth) = (start, 0);
        while let Some(i) = k.filter(|_| depth < 64) {
            if let Some(g) = self.gear.iter().find(|g| g.joint == i && !g.door) {
                return turn(g);
            }
            k = js[i].parent.filter(|p| *p != i);
            depth += 1;
        }
        // a wheel bone outside the leg's bones: the leg whose mesh it sits in
        if let Some(i) = start {
            let p = Mat4::from_cols_array(&js[i].ibm).inverse().w_axis.truncate();
            let pad = Vec3::splat(0.25);
            if let Some(g) = self.gear.iter().find(|g| !g.door && p.cmpge(g.bbox[0] - pad).all() && p.cmple(g.bbox[1] + pad).all()) {
                return turn(g);
            }
        }
        Mat4::IDENTITY
    }

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
        let steers: Vec<Mat4> = spots.iter().map(|(bone, _, _)| self.gear_at(bone) * self.steer_at(bone)).collect();
        let wheel_spin = self.wheel_spin;
        let brake_heat = self.brake_heat.clamp(0.0, 1.0);
        if let Some(ren) = &self.renderer {
            for (is_disc, g) in self.brakes.iter_mut() {
                let inst: Vec<render3d::Instance> = spots
                    .iter()
                    .zip(&steers)
                    .map(|((bone, p, r), steer)| {
                        let right = bone.strip_prefix("wheel_").map(|s| s.starts_with('r')).unwrap_or(false);
                        let mirror = if right { Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)) } else { Mat4::IDENTITY };
                        let spin = if *is_disc { Mat4::from_quat(Quat::from_rotation_x(wheel_spin)) } else { Mat4::IDENTITY };
                        let mut inst = render3d::Instance::new(*steer * Mat4::from_translation(*p) * spin * mirror * Mat4::from_scale(Vec3::splat(r / 0.5)), false);
                        if *is_disc {
                            // negative extra.z = brake heat (see the shader)
                            inst.extra[2] = -brake_heat;
                        }
                        inst
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

    /// Recolour the vehicle's parts for the chosen configuration. An export
    /// lists the part colours of each distinct configuration result
    /// (model.json `part_colours.looks`); the vertices' part ids are in their
    /// raw vertex colour alpha.
    fn sync_part_look(&mut self, rs: &egui_wgpu::RenderState) {
        let Some(l) = self.loaded.as_mut() else {
            self.part_look_table = None;
            return;
        };
        let pc = &l.info["part_colours"];
        let default = pc["default"].as_u64();
        let Some(idx) = self.part_look_want.as_ref().and_then(|k| pc["configs"][k.as_str()].as_u64()).or(default) else {
            self.part_look_table = None;
            return;
        };
        let idx = idx as usize;
        let decode = |hex: &str| -> Arc<Vec<[u8; 4]>> { Arc::new((0..256).map(|i| std::array::from_fn(|k| u8::from_str_radix(&hex[i * 8 + k * 2..i * 8 + k * 2 + 2], 16).unwrap_or(255))).collect()) };
        // a freshly loaded model shows the default look
        let shown = match &self.part_look_done {
            Some((p, i)) if *p == l.model.path => Some(*i),
            _ => default.map(|d| d as usize),
        };
        let Some(hex) = pc["looks"][idx].as_str().filter(|h| h.len() == 2048) else {
            self.part_look_table = None;
            return;
        };
        if shown == Some(idx) {
            if self.part_look_done.as_ref() != Some(&(l.model.path.clone(), idx)) || self.part_look_table.is_none() {
                self.part_look_table = Some(decode(hex));
            }
            self.part_look_done = Some((l.model.path.clone(), idx));
            return;
        }
        let look = decode(hex);
        self.part_look_table = Some(look.clone());
        let parts: Vec<bool> = l.model.materials.iter().map(|m| m.parts).collect();
        for m in l.model.meshes.iter_mut().filter(|m| m.material.is_some_and(|k| parts.get(k).copied().unwrap_or(false))) {
            for v in m.verts.iter_mut() {
                v.tint = look[v.col[3] as usize];
            }
        }
        self.part_look_done = Some((l.model.path.clone(), idx));
        let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
        let mut g = ren.upload(rs, &l.model, &[]);
        ren.ensure_edges(rs, &mut g, &l.model);
        self.gpu = Some(g);
        self.apply_paint(rs);
    }

    fn update_wheels(&mut self, rs: &egui_wgpu::RenderState, ctx: &egui::Context) {
        self.sync_part_look(rs);
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
                self.steer_wheel = find_steering_wheel(&l.model);
                self.window_movers = find_window_movers(&l.model);
                self.air = find_air_parts(&l.model);
                self.gear = find_gear(&l.model);
                self.has_lamps = l.model.joints.iter().any(|j| self.lamps.kind(&j.name).is_some());
                self.siren_dirty = true;
            }
        }
        let arrived = {
            let mut q = self.wheel_pending.lock().unwrap();
            // results for wheels that are no longer wanted are dropped
            q.retain(|x| x.0 == self.wheel_key);
            q.pop().map(|x| x.1)
        };
        if let Some(r) = arrived {
            self.wheel_loading = false;
            match r {
                Ok(ls) => {
                    let c = self.rim_colour;
                    self.rim_applied = c;
                    self.wheel_boxes = ls.iter().map(|l| l.model.bbox).collect();
                    self.wheel_mats = ls.iter().map(|l| l.model.materials.iter().map(|m| m.name.clone()).collect()).collect();
                    self.wheel_orig = ls.iter().map(|l| l.model.meshes.iter().map(|m| m.verts.iter().map(|v| v.pos).collect()).collect()).collect();
                    self.wheel_src = ls;
                    self.rim_baked = -1.0;
                    let _ = c;
                }
                Err(e) => {
                    self.status = format!("{} · wheels: {e}", self.status);
                    self.wheels.clear();
                }
            }
        }
        // Shape rim and tyre to the rim size, then upload. All bones of a
        // shared wheel model sit on the hub, so the game can only shape the
        // wheel by scaling them: the rim and the tyre's bead (rim and
        // tyre_profile bones) to the rim radius, the tread (tread_profile
        // bones) to the tyre radius. The instances scale the whole wheel to
        // the tyre radius, so here everything but the tread is scaled by
        // rim ratio x tyre model radius / rim model radius.
        let look_changed = match (&self.wheel_look, &self.part_look_table) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        let rim_ratio = self.rim_ratio_now();
        if !self.wheel_src.is_empty() && (self.rim_baked != rim_ratio || look_changed) {
            self.rim_baked = rim_ratio;
            self.wheel_look = self.part_look_table.clone();
            // Rim and tyre colours: their vertices carry part ids of the
            // vehicle they are fitted to (vertex colour alpha), coloured
            // from the vehicle's part colours like its own parts.
            for l in self.wheel_src.iter_mut() {
                let wheel: Vec<bool> = l.model.materials.iter().map(|m| m.name.ends_with(RIM_SHADER) || m.name.ends_with("_99ad6fc9")).collect();
                for (k, m) in l.model.materials.iter_mut().enumerate() {
                    if wheel[k] {
                        m.parts = self.wheel_look.is_some();
                    }
                }
                for m in l.model.meshes.iter_mut().filter(|m| m.material.is_some_and(|k| wheel.get(k).copied().unwrap_or(false))) {
                    for v in m.verts.iter_mut() {
                        v.tint = self.wheel_look.as_ref().map_or([255; 4], |t| t[v.col[3] as usize]);
                    }
                }
            }
            let k = rim_factor(&self.wheel_boxes, rim_ratio);
            for (l, orig) in self.wheel_src.iter_mut().zip(&self.wheel_orig) {
                let tread: Vec<bool> = l.model.joints.iter().map(|j| j.name.to_ascii_lowercase().contains("tread_profile")).collect();
                for (m, o) in l.model.meshes.iter_mut().zip(orig) {
                    for (v, p) in m.verts.iter_mut().zip(o) {
                        let wt: f32 = (0..4).filter(|&i| tread.get(v.joints[i] as usize).copied().unwrap_or(false)).map(|i| v.weights[i] as f32 / 255.0).sum();
                        let f = k * (1.0 - wt) + wt;
                        v.pos = [p[0], p[1] * f, p[2] * f];
                    }
                }
            }
            let c = self.rim_colour;
            self.rim_applied = c;
            let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
            self.wheels = self
                .wheel_src
                .iter()
                .map(|l| {
                    let mut g = ren.upload(rs, &l.model, &[]);
                    // rims take the rim colour (the game sets it per
                    // vehicle; it is not in the files)
                    for (k, m) in l.model.materials.iter().enumerate() {
                        if m.name.ends_with(RIM_SHADER) || (m.parts && m.name.ends_with("_99ad6fc9")) {
                            // with part colours the vertices carry the colour
                            let b = if m.parts { [1.0, 1.0, 1.0, 1.0] } else { [c[0], c[1], c[2], 1.0] };
                            ren.set_material_base(rs, &mut g, k, b);
                        }
                    }
                    g
                })
                .collect();
            // the instances are set again below
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
            self.wheel_key = key.clone();
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
                slot.lock().unwrap().push((key, r));
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
                    self.gear_at(bone) * self.steer_at(bone)
                        * Mat4::from_translation(*p)
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
        // a character's animation clip
        if let (true, Some(a)) = (l.look.is_some(), &self.ped_clip.anim) {
            playing.push((a, self.ped_clip.pos));
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
        // steering: the wheel_turn bones turn about the vertical, the
        // steering wheel about its own axis
        let mut swing = swing;
        // side windows: each mover slides its glass down into the door
        if self.windows_down > 0.0 {
            for (i, d, p) in &self.window_movers {
                // The glass is drawn down to its bottom edge instead of
                // being moved: x' = x - t d (d . (x - p)). A straight slide
                // along the glass's plane would leave the door (the glass
                // leans in at the top), and the path it takes inside the
                // door is not known.
                let t = self.windows_down.clamp(0.0, 0.985);
                let a = glam::Mat3::IDENTITY - glam::Mat3::from_cols(*d * d.x, *d * d.y, *d * d.z) * t;
                let slide = Mat4::from_translation(*d * (t * d.dot(*p))) * Mat4::from_mat3(a);
                let m = swing.get(i).map_or(slide, |m| *m * slide);
                swing.insert(*i, m);
            }
        }
        if self.steer != 0.0 {
            for (i, j) in js.iter().enumerate() {
                if is_steer_bone(&j.name) {
                    let (_, r, _) = Mat4::from_cols_array(&j.local).to_scale_rotation_translation();
                    let turn = Mat4::from_quat(r.inverse() * Quat::from_rotation_y(self.steer) * r);
                    let m = swing.get(&i).map_or(turn, |m| *m * turn);
                    swing.insert(i, m);
                }
            }
            if let Some((i, axis)) = self.steer_wheel {
                swing.insert(i, Mat4::from_quat(Quat::from_axis_angle(axis, self.steer * STEERING_RATIO * self.steer_mult)));
            }
        }
        // landing gear: the legs swing up over the whole travel, the doors
        // (modelled open) close over its last part
        if self.gear_up > 0.0 {
            let t = self.gear_up.clamp(0.0, 1.0);
            let late = ((t - 0.6) / 0.4).clamp(0.0, 1.0);
            for p in &self.gear {
                if p.hide {
                    if late > 0.0 {
                        let m = Mat4::from_scale(Vec3::splat(1e-5));
                        let m = swing.get(&p.joint).map_or(m, |s| *s * m);
                        swing.insert(p.joint, m);
                    }
                    continue;
                }
                let a = p.angle * if p.door { late * late * (3.0 - 2.0 * late) } else { t };
                if a != 0.0 {
                    let m = match p.follow {
                        // the leg's turn in model space, brought into this joint's own
                        Some((pivot, axis)) => {
                            let g = Mat4::from_cols_array(&js[p.joint].ibm).inverse();
                            g.inverse() * Mat4::from_translation(pivot) * Mat4::from_quat(Quat::from_axis_angle(axis, a)) * Mat4::from_translation(-pivot) * g
                        }
                        None => Mat4::from_quat(Quat::from_axis_angle(p.axis, a)),
                    };
                    let m = swing.get(&p.joint).map_or(m, |s| *s * m);
                    swing.insert(p.joint, m);
                }
            }
        }
        // aircraft: rotors and propellers turn, control surfaces swing
        // about their hinge, and of the still and the blurred blades only
        // the set for the speed is left at its size
        let fast = self.rotor_speed >= 0.5;
        for p in &self.air {
            let m = match p.kind {
                AirKind::Spin => (self.rotor_angle != 0.0).then(|| Mat4::from_quat(Quat::from_axis_angle(p.axis, self.rotor_angle * p.sign))),
                AirKind::Still => fast.then(|| Mat4::from_scale(Vec3::splat(1e-5))),
                AirKind::Blur => (!fast).then(|| Mat4::from_scale(Vec3::splat(1e-5))),
                AirKind::Flap | AirKind::Aileron | AirKind::Elevator | AirKind::Rudder | AirKind::Airbrake => {
                    let a = match p.kind {
                        AirKind::Flap => self.flaps,
                        AirKind::Aileron => self.ailerons,
                        AirKind::Elevator => self.elevators,
                        AirKind::Rudder => self.rudder,
                        _ => self.airbrakes,
                    };
                    (a != 0.0).then(|| Mat4::from_quat(Quat::from_axis_angle(p.axis, a * p.sign)))
                }
            };
            if let Some(m) = m {
                let m = swing.get(&p.joint).map_or(m, |s| *s * m);
                swing.insert(p.joint, m);
            }
        }
        // characters: the pose turns its bones about the model's axes
        if l.look.is_some() && self.ped_pose != PedPose::default() {
            for (name, deg) in self.ped_pose.bones() {
                let Some(i) = js.iter().position(|j| j.name == name) else { continue };
                // the bone's bind orientation in model space
                let (_, qw, _) = Mat4::from_cols_array(&js[i].ibm).inverse().to_scale_rotation_translation();
                let r = Quat::from_rotation_y(deg[1].to_radians()) * Quat::from_rotation_z(deg[2].to_radians()) * Quat::from_rotation_x(deg[0].to_radians());
                let turn = Mat4::from_quat(qw.inverse() * r * qw);
                let m = swing.get(&i).map_or(turn, |m| *m * turn);
                swing.insert(i, m);
            }
        }
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
            // the game's display names for the list, once the vehicle data is read
            if !self.picker.has_friendly() {
                if let Some(db) = self.vehicles.get(&self.game, &self.work, &ctx) {
                    self.picker.set_friendly(Arc::new(db.iter().filter_map(|(k, v)| v.display.clone().map(|d| (k.clone(), d))).collect()));
                }
            }
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
        if self.rotor_speed > 0.0 && self.air.iter().any(|p| p.kind == AirKind::Spin) {
            // slow enough to see the blades below half speed; the blurred
            // set turns slowly, as a blur does
            let rate = if self.rotor_speed < 0.5 { 2.0 + self.rotor_speed * 20.0 } else { 3.0 + self.rotor_speed * 4.0 };
            self.rotor_angle = (self.rotor_angle + ctx.input(|i| i.stable_dt).min(0.1) * rate) % std::f32::consts::TAU;
            ctx.request_repaint();
        }
        self.update_ped_clip(&ctx);
        let (globals, mut skin) = self.joint_mats();
        // Lit lamps: the light of each lamp bone rides in the bottom row of
        // its joint matrix (see the shader). Not on the Metal renderer.
        if self.siren_dirty {
            self.siren_dirty = false;
            self.sirens = match (&self.loaded, &self.veh_lights) {
                (Some(l), Some(v)) => find_sirens(&l.model, v, &self.cfg_patterns),
                _ => Vec::new(),
            };
            self.lamp_rgb = match (&self.loaded, &self.veh_lights) {
                (Some(l), Some(v)) => l.model.joints.iter().map(|j| lamp_colour(v, &j.name)).collect(),
                _ => Vec::new(),
            };
            if self.sirens.is_empty() {
                self.siren_sound.want = 0;
            }
            // kept while nothing is known yet (a pattern asked for on the command line)
            if !self.sirens.is_empty() {
                self.siren = self.siren.min(self.sirens.len());
            }
        }
        if (self.has_lamps || !self.sirens.is_empty()) && cfg!(not(target_os = "macos")) {
            let t = ctx.input(|i| i.time);
            if let Some(l) = &self.loaded {
                for (i, (j, m)) in l.model.joints.iter().zip(skin.iter_mut()).enumerate() {
                    if let Some(mut c) = self.lamps.light(&j.name, t) {
                        // the lamp's own colour from the vehicle data, at
                        // the brightness of its kind
                        if let Some(rgb) = self.lamp_rgb.get(i).copied().flatten() {
                            let k = c[0].max(c[1]).max(c[2]);
                            c = [rgb[0] * k, rgb[1] * k, rgb[2] * k];
                        }
                        m.x_axis.w = c[0];
                        m.y_axis.w = c[1];
                        m.z_axis.w = c[2];
                    }
                }
            }
            let siren = self.siren.checked_sub(1).and_then(|i| self.sirens.get(i));
            if let Some(s) = siren {
                for (ji, steps, col) in &s.lamps {
                    let k = steps.iter().map(|(o, ty)| flash(*ty, t * s.speed as f64 - *o as f64)).fold(0.0, f32::max);
                    if let (true, Some(m)) = (k > 0.0, skin.get_mut(*ji)) {
                        m.x_axis.w = col[0] * k;
                        m.y_axis.w = col[1] * k;
                        m.z_axis.w = col[2] * k;
                    }
                }
            }
            if self.lamps.left || self.lamps.right || self.lamps.hazard {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
            if siren.is_some() {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
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
        // a character's hair, brows and beard ride on the head bone
        if let Some(l) = self.loaded.as_ref().filter(|l| l.look.is_some()) {
            let head = l.model.joints.iter().position(|j| j.name == "SKEL_Head").and_then(|i| skin.get(i)).copied().unwrap_or(Mat4::IDENTITY);
            if self.hair_head != Some(head) || self.hair.gpu.len() != self.hair_head_n {
                for g in self.hair.gpu.iter_mut() {
                    ren.set_instances(rs, g, &[crate::render3d::Instance::new(head, false)]);
                }
                self.hair_head = Some(head);
                self.hair_head_n = self.hair.gpu.len();
            }
        }
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
            // The fallback after 600 frames must not fire while a vehicle's
            // data, wheels or brakes are still loading (a first export of
            // the wheel models takes longer than 600 frames, and the shot
            // then showed the car without wheels); 180 s is the last resort.
            let busy = self.wheel_loading || self.brake_loading || matches!(&*self.vehicles.state.lock().unwrap(), crate::vehicle_db::State::Loading(_));
            let ready = (self.gpu.is_some() && self.frames_since_load >= 3 && settled)
                || (!self.loading && !self.hair.fitting && !self.auto.find_hair && self.hair.settled() && self.auto.frames > 600 && (!busy || ctx.input(|i| i.time) > 180.0));
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
                if let Some(l) = self.loaded.as_ref().filter(|l| l.model.joints.iter().any(|j| is_steer_bone(&j.name))) {
                    let _ = l;
                    let mut deg = self.steer.to_degrees();
                    let lock = self.steer_lock.to_degrees();
                    ui.add(egui::Slider::new(&mut deg, -lock..=lock).text("steering (degrees)")).on_hover_text(format!(
                        "Turns the wheel_turn bones (front wheels, a motorcycle's fork) about the vertical and what is fitted to them, up to the vehicle's steering lock: {}. The steering wheel turns {STEERING_RATIO:.0} x steerWheelMult ({:.2}) as far (an estimate). Both wheels take the same angle; a fork turns about the vertical, not its own rake.",
                        if self.steer_lock_known { format!("{lock:.1} degrees (fSteeringLock of its handling record)") } else { format!("{lock:.0} degrees (a default: no handling record was found for this model)") },
                        self.steer_mult
                    ));
                    self.steer = deg.to_radians();
                }
                if !self.gear.is_empty() {
                    ui.add(egui::Slider::new(&mut self.gear_up, 0.0..=1.0).text("landing gear up")).on_hover_text("Folds the landing gear of aircraft that have gear doors. The game's files hold no clip or angles for it, so the fold is worked out from the model: each leg (gear_f, gear_rl, gear_rr ...) swings a quarter turn up about its bone towards the bay its doors mark (the nose leg forwards and the main legs inwards where the doors do not say), and the doors, which are modelled open, close over the last part of the travel. Struts do not compress and a leg may not sit exactly in its bay.");
                }
                if !self.air.is_empty() {
                    let has = |k: AirKind| self.air.iter().any(|p| p.kind == k);
                    let note = "Aircraft parts are found by their bones. The axis of each is taken from the shape of its mesh (a rotor turns about its thin direction, a control surface about its long one) and the direction from where the mesh lies; the angles are the sliders', not limits read from the files.";
                    if has(AirKind::Spin) {
                        ui.add(egui::Slider::new(&mut self.rotor_speed, 0.0..=1.0).text("rotor / propeller speed")).on_hover_text(format!("Turns the rotor_main, rotor_rear and prop_N bones. Below half speed the still blades are shown, from half speed the game's blurred blades (the _fast meshes). {note}"));
                    }
                    let deg = |ui: &mut egui::Ui, v: &mut f32, lo: f32, hi: f32, text: &str| {
                        let mut d = v.to_degrees();
                        ui.add(egui::Slider::new(&mut d, lo..=hi).text(text)).on_hover_text(note);
                        *v = d.to_radians();
                    };
                    if has(AirKind::Flap) {
                        deg(ui, &mut self.flaps, 0.0, 40.0, "flaps (degrees down)");
                    }
                    if has(AirKind::Aileron) {
                        deg(ui, &mut self.ailerons, -25.0, 25.0, "ailerons (left down, right up)");
                    }
                    if has(AirKind::Elevator) {
                        deg(ui, &mut self.elevators, -25.0, 25.0, "elevators (degrees down)");
                    }
                    if has(AirKind::Rudder) {
                        deg(ui, &mut self.rudder, -30.0, 30.0, "rudder (degrees)");
                    }
                    if has(AirKind::Airbrake) {
                        deg(ui, &mut self.airbrakes, 0.0, 60.0, "air brakes (degrees up)");
                    }
                }
                if !self.window_movers.is_empty() {
                    ui.add(egui::Slider::new(&mut self.windows_down, 0.0..=1.0).text("windows down")).on_hover_text("Lowers each side window by its mover bone (window_lf_mover ...): the glass is drawn down to its bottom edge, so its top edge drops to the sill. The bones are the game's; the path the game moves the glass along inside the door is not read from the files, so the glass is shortened instead of slid.");
                }
                if !self.brakes.is_empty() {
                    ui.add(egui::Slider::new(&mut self.brake_heat, 0.0..=1.0).text("brake glow")).on_hover_text("Makes the brake discs glow as if hot, from dull red to orange. A viewer effect on the fitted disc models: how the game heats and lights its discs is not read from the files.");
                }
                if self.has_lamps {
                    let l = &mut self.lamps;
                    ui.horizontal_wrapped(|ui| {
                        ui.label("lights");
                        ui.checkbox(&mut l.head, "head");
                        ui.checkbox(&mut l.high, "high beam");
                        ui.checkbox(&mut l.day, "daytime");
                        ui.checkbox(&mut l.tail, "tail");
                        ui.checkbox(&mut l.brake, "brake");
                        ui.checkbox(&mut l.reverse, "reversing");
                        ui.checkbox(&mut l.left, "indicator left");
                        ui.checkbox(&mut l.right, "indicator right");
                        ui.checkbox(&mut l.hazard, "hazards");
                        ui.checkbox(&mut l.interior, "interior");
                    })
                    .response
                    .on_hover_text("Lamps are found by their bones (headlight_l, taillight_l, brakelight_l, reversinglight_l, indicator_lf ... and the _glass bone of each lens). A lit lamp's reflector and lens glow in a fixed colour for its kind, and its LED mesh (standard_vehicle_emissive_animated_bones) shows its own colour map. How bright the game makes each lamp, and the light it casts, are not read from the files.");
                }
                if !self.sirens.is_empty() {
                    let label = |i: usize, s: &[Siren]| if i == 0 { "off".to_string() } else { s[i - 1].label.clone() };
                    egui::ComboBox::from_label("siren lights")
                        .selected_text(label(self.siren, &self.sirens))
                        .show_ui(ui, |ui| {
                            for i in 0..=self.sirens.len() {
                                ui.selectable_value(&mut self.siren, i, label(i, &self.sirens));
                            }
                        })
                        .response
                        .on_hover_text("The vehicle's light patterns that reach its siren and strobe bones. Read from the game's files: which lamps a pattern uses, each lamp's phase offset in the cycle and its flash type number, the pattern's speed, and the lamp's light archetype name. Not read: what a flash type looks like over time (0 is shown as off, 1 as steady, 6 as an even blink, every other number as a double flash), the length of the cycle (shown as one second at speed 1) and how bright each lamp is (all the same here). A lamp's colour is its light set's in the game's light archetypes (the stored bytes taken as linear); many siren lamps are white lights behind a coloured lens. Patterns marked 'pack' are in the vehicle's light pack without a part of this configuration linking them.");
                }
                if !self.sirens.is_empty() {
                    let s = &mut self.siren_sound;
                    let label = |i: usize| if i == 0 { "off" } else { crate::siren_sound::SOUNDS[i - 1].0 };
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_label("siren sound")
                            .selected_text(label(s.want))
                            .show_ui(ui, |ui| {
                                for i in 0..=crate::siren_sound::SOUNDS.len() {
                                    ui.selectable_value(&mut s.want, i, label(i));
                                }
                            })
                            .response
                            .on_hover_text("Loops one of the game's siren sounds, found by its name in the sound definitions of audio.rpf (RESIDENT_VEHICLES_SIREN_WAIL_01 ..., _QUICK_01 ..., _FIRETRUCK_WAIL_01). Which vehicle plays which is not read from the files, so the choice is yours. Needs audio.rpf's banks scanned (open the Audio page once).");
                        if s.update(&self.game, &self.work) {
                            ui.spinner();
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                        }
                    });
                    if let Some(e) = &s.error {
                        ui.colored_label(egui::Color32::from_rgb(230, 120, 90), e);
                    }
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
                    ui.checkbox(&mut self.rim_auto, "rim size from the configuration").on_hover_text("The configuration's wheel fitment record stores rim radius / tyre radius (0.82 on a sports car, 0.65 on a truck). Two-wheelers have no such record: there wheelScale is the rim radius and is used instead. The tyre's outer radius comes from the wheel's collision disc. Offsets that other parts add to the fitment are not applied.");
                    ui.add_enabled_ui(!self.rim_auto, |ui| {
                        ui.add(egui::Slider::new(&mut self.rim_ratio, 0.0..=0.95).text("rim / tyre radius")).on_hover_text("0 = the proportions of the tyre model");
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
        if let Some(q) = self.look_ui(ui).or_else(|| self.parts_ui(ui)) {
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
    fx: &HashMap<u64, PartFx>,
    game: &Path,
) -> (Option<glb::Model>, Vec<PathBuf>, Vec<String>, Vec<String>, Vec<(String, Vec<[f32; 3]>)>, Vec<([f32; 3], [u8; 3])>, Vec<bool>, Vec<String>) {
    let mut first = Vec::new();
    let mut names = Vec::new();
    let mut errs = Vec::new();
    let mut skin = Vec::new();
    let mut colours = Vec::new();
    let mut mask = Vec::new();
    let mut notes = Vec::new();
    let mut fresh = true;
    // outfits: the head's neck tone and the material ranges of the skin meshes
    let mut tone: Option<[f32; 3]> = None;
    let mut body_mats: Vec<std::ops::Range<usize>> = Vec::new();
    let mut coloured = 0;
    // mesh ranges of the skin meshes and of the garments that cover skin
    let mut skin_meshes: Vec<std::ops::Range<usize>> = Vec::new();
    let mut cover_meshes: Vec<std::ops::Range<usize>> = Vec::new();
    let mut heel_meshes: Vec<std::ops::Range<usize>> = Vec::new();
    // per outfit part: its meshes, its joint list and the joint an attached
    // accessory was moved onto
    let mut rig_parts: Vec<(std::ops::Range<usize>, Vec<glb::Joint>, Option<usize>)> = Vec::new();
    let mut under_meshes: Vec<std::ops::Range<usize>> = Vec::new();
    let mut outer_meshes: Vec<std::ops::Range<usize>> = Vec::new();
    // Body shape: the skin and head are posed by the game's rig; clothes
    // and accessories follow the skin under them (`Wrap`).
    let shape = if fx.is_empty() { (0.0, 0.0) } else { *BODY_SHAPE.lock().unwrap() };
    let mut posed: HashMap<u64, anyhow::Result<(glb::Model, Vec<PathBuf>, serde_json::Value)>> = HashMap::new();
    let mut field: Vec<([f32; 3], [f32; 3])> = Vec::new();
    if shape != (0.0, 0.0) {
        for e in parts {
            let Some(f) = fx.get(&e.off).filter(|f| f.skeletal) else { continue };
            let mut r = export_load_shaped(a, e, cache, lod, &f.skins, shape);
            if let Ok((m, _, _)) = &mut r {
                let before: Vec<[f32; 3]> = m.meshes.iter().flat_map(|me| me.verts.iter().map(|v| v.pos)).collect();
                apply_pose(m);
                if f.body || f.head {
                    field.extend(before.iter().zip(m.meshes.iter().flat_map(|me| me.verts.iter())).map(|(b, v)| (*b, [v.pos[0] - b[0], v.pos[1] - b[1], v.pos[2] - b[2]])));
                }
            }
            posed.insert(e.off, r);
        }
        if field.is_empty() {
            notes.push("Body shape: no skin mesh to shape in this outfit, so it is shown neutral.".to_string());
        } else {
            notes.push("Body shape: the skin and head follow the game's body-shape rig. Clothes follow the skin under them (each point moves with the nearest skin), because the game's own data for shaping clothes is not decoded.".to_string());
        }
    }
    let wrap = (!field.is_empty()).then(|| Wrap::new(field));
    for e in parts {
        let f = fx.get(&e.off);
        let r = match posed.remove(&e.off) {
            Some(r) => r,
            None => export_load_skin(a, e, cache, lod, f.map_or(&[], |f| &f.skins)),
        };
        match r {
            Ok((mut m, lods, _)) => {
                if first.is_empty() {
                    first = lods;
                }
                let mut attached = None;
                if let Some(f) = f {
                    if !f.skeletal {
                        attached = place_attached(&mut m);
                        if let Some(w) = &wrap {
                            w.apply(&mut m);
                        }
                    }
                    if colour_part(&mut m, f, game) {
                        coloured += 1;
                    }
                    if f.head {
                        tone = neck_tone(&m);
                    }
                    // The vertices are final (posed here, or following the
                    // skin): the renderer must not skin the part that
                    // happens to come first again.
                    m.meshes.iter_mut().for_each(|me| me.skinned = false);
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
                let mat0 = model.as_ref().map_or(0, |into| into.materials.len());
                if f.is_some_and(|f| f.body) {
                    body_mats.push(mat0..mat0 + m.materials.len());
                }
                // (an animal root's placeholder meshes are cleared before the first merge)
                let mesh0 = if fresh { 0 } else { model.as_ref().map_or(0, |into| into.meshes.len()) };
                if f.is_some_and(|f| f.body) {
                    skin_meshes.push(mesh0..mesh0 + m.meshes.len());
                }
                if f.is_some_and(|f| f.covers) {
                    cover_meshes.push(mesh0..mesh0 + m.meshes.len());
                }
                if f.is_some() {
                    rig_parts.push((mesh0..mesh0 + m.meshes.len(), m.joints.iter().map(|j| glb::Joint { name: j.name.clone(), parent: j.parent, local: j.local, ibm: j.ibm }).collect(), attached));
                }
                match f.map_or(0, |f| f.layer) {
                    1 => under_meshes.push(mesh0..mesh0 + m.meshes.len()),
                    2 => outer_meshes.push(mesh0..mesh0 + m.meshes.len()),
                    _ => {}
                }
                if f.is_some_and(|f| f.heel) {
                    heel_meshes.push(mesh0..mesh0 + m.meshes.len());
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
    if let (Some(m), false) = (&mut model, body_mats.is_empty()) {
        match tone {
            Some(t) => {
                for i in body_mats.into_iter().flatten() {
                    let Some(mat) = m.materials.get_mut(i) else { continue };
                    if mat.base.is_none() {
                        mat.base_factor = [t[0], t[1], t[2], mat.base_factor[3]];
                        mat.gap = "skin tone taken from the face texture at the neck".into();
                    }
                }
                notes.push("Body skin: the game builds it at run time and no painted body texture is stored, so the body takes the skin tone of the face texture at the neck.".to_string());
            }
            None => notes.push("Body skin: no face texture to take a skin tone from, so the body is shown white.".to_string()),
        }
    }
    if let (Some(m), false, false) = (&mut model, skin_meshes.is_empty(), heel_meshes.is_empty()) {
        if let Some(deg) = pose_feet_for_heels(m, &skin_meshes, &heel_meshes) {
            notes.push(format!("Heeled shoes: the feet are tilted {deg:.0} degrees at the ankle with the toes bent back level, the angle that puts most of the foot's skin on the shoe. (The game poses the foot for the shoe's heel class; that pose is not decoded.)"));
        }
    }
    if let (Some(m), false, false) = (&mut model, skin_meshes.is_empty(), cover_meshes.is_empty()) {
        let hidden = hide_covered_skin(m, &skin_meshes, &cover_meshes);
        if hidden > 0 {
            notes.push(format!("Skin under clothes: {hidden} skin triangles that lie right under a garment are hidden, so skin does not show through it. (The game uses skin meshes cut to the garment; which cut goes with which garment is not decoded.)"));
        }
    }
    if let (Some(m), false, false) = (&mut model, under_meshes.is_empty(), outer_meshes.is_empty()) {
        let hidden = hide_covered_skin(m, &under_meshes, &outer_meshes);
        if hidden > 0 {
            notes.push(format!("Layers: {hidden} triangles of the clothes worn under the jacket are hidden where the jacket covers them, so they do not show through it."));
        }
    }
    if let (Some(m), false) = (&mut model, rig_parts.is_empty()) {
        if rig_character(m, &rig_parts, &skin_meshes) {
            notes.push("Rig: every part is bound to the shared skeleton, so the Pose controls move the body with its clothes. A part's own helper bones follow the body bone they hang from (the game drives them with data that is not decoded), and the first four of a vertex's eight bone weights are used, so joints bend more simply than in the game.".to_string());
        }
    }
    if coloured > 0 {
        notes.push(format!("Clothing colours: {coloured} part(s) coloured from the outfit data: the item's stored colour, else row tint 0 of its tint palette (for the generic palette only when it agrees with the item's colour tag), else the colour tag itself (one of 18 words such as navy or khaki, shown in a plain shade of ours). Second and third tints are not applied."));
    }
    (model, first, names, errs, skin, colours, mask, notes)
}

/// Colour a ped clothing part of an outfit: its grey shade maps become the
/// item's stored colour times the shade, or the tint palette row at the
/// shade. Returns whether anything was coloured.
fn colour_part(m: &mut glb::Model, f: &PartFx, game: &Path) -> bool {
    let pal = f.tint.and_then(|(h, row)| sixng_core::pedtint::palettes(game).ok()?.get(&h).map(|p| (p, row)));
    // `ped_tint_generic_clean` rows are often placeholders the game fills at
    // run time: such a row counts only when it is the colour the item's
    // colour tag names.
    let generic = f.tint.is_some_and(|t| t.0 == 0xa7c8_d8e0);
    let pal = pal.filter(|(p, row)| !generic || f.tag.is_none_or(|t| sixng_core::pedtint::nearest_word(p.sample(*row, 128)) == t));
    let word = f.tag.and_then(|t| sixng_core::pedtint::COLOUR_WORDS.iter().find(|w| w.0 == t)).map(|w| w.1);
    // shade maps are mid grey on plain cloth: 190 keeps the colour there
    let flat = |c: [u8; 3]| -> [[u8; 3]; 256] { std::array::from_fn(|v| c.map(|x| ((x as f32) * (v as f32 / 190.0)).min(255.0) as u8)) };
    let lut: [[u8; 3]; 256] = match (f.colour, pal, word) {
        (Some(c), _, _) => flat(c),
        (None, Some((p, row)), _) => std::array::from_fn(|v| p.sample(row, v as u8)),
        (None, None, Some(c)) => flat(c),
        (None, None, None) => return false,
    };
    let mut done = std::collections::HashSet::new();
    for mat in m.materials.iter().filter(|x| x.ped_shade) {
        let Some(i) = mat.base else { continue };
        if !done.insert(i) {
            continue;
        }
        let Some(im) = m.images.get_mut(i).filter(|im| im.bc.is_none()) else { continue };
        for (_, _, px) in im.mips.iter_mut() {
            for t in px.chunks_exact_mut(4) {
                let c = lut[t[1] as usize];
                t[0] = c[0];
                t[1] = c[1];
                t[2] = c[2];
            }
        }
        im.key = format!("{}#{:?}{:?}", im.key, f.colour, f.tint);
    }
    !done.is_empty()
}

/// Attached accessories (a watch, a ring) are modelled at their own root
/// bone (`Watch_Root`, at the authoring origin under the feet) and carry
/// attach bones under the body's bones (`Watch_L_Wrist` under the wrist)
/// that no vertex uses. The game moves the root onto an attach bone; do the
/// same in the bind pose, so the part sits on the body in the merged model.
/// The left one is taken when there are two. Parts modelled in place (their
/// vertices are near the bones that move them) are left alone.
fn place_attached(m: &mut glb::Model) -> Option<usize> {
    use glam::{Mat4, Vec3};
    if m.joints.is_empty() || m.meshes.iter().any(|me| !me.skinned) {
        return None;
    }
    let mut used = vec![false; m.joints.len()];
    for v in m.meshes.iter().flat_map(|me| &me.verts) {
        for k in 0..4 {
            if v.weights[k] > 0 {
                if let Some(u) = used.get_mut(v.joints[k] as usize) {
                    *u = true;
                }
            }
        }
    }
    let bind = |i: usize| Mat4::from_cols_array(&m.joints[i].ibm).inverse();
    let pos = |i: usize| bind(i).transform_point3(Vec3::ZERO);
    let prefix = |i: usize| m.joints[i].name.split('_').next().unwrap_or("").to_string();
    let first = used.iter().position(|u| *u)?;
    let pre = prefix(first);
    // every used bone belongs to the accessory (same name prefix)
    if pre.is_empty() || ["SKEL", "BM", "MH", "RB", "PH", "GM", "SEC", "FACIAL"].contains(&pre.as_str()) || (0..used.len()).any(|i| used[i] && prefix(i) != pre) {
        return None;
    }
    // its root: the topmost ancestor with that prefix
    let mut root = first;
    while let Some(p) = m.joints[root].parent.filter(|&p| prefix(p) == pre) {
        root = p;
    }
    // attach bones: same prefix, unused, not under the root
    let under_root = |mut i: usize| loop {
        if i == root {
            return true;
        }
        match m.joints[i].parent {
            Some(p) => i = p,
            None => return false,
        }
    };
    let mut attach: Vec<usize> = (0..used.len()).filter(|&i| !used[i] && prefix(i) == pre && !under_root(i)).collect();
    attach.sort_by_key(|&i| (!m.joints[i].name.contains("_L_"), i));
    let &at = attach.first()?;
    let c = (Vec3::from(m.bbox[0]) + Vec3::from(m.bbox[1])) * 0.5;
    if (c - pos(root)).length() > 0.25 || (pos(at) - pos(root)).length() < 0.3 {
        return None;
    }
    let t = bind(at) * bind(root).inverse();
    if !t.is_finite() {
        return None;
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for me in &mut m.meshes {
        let mut sum = Vec3::ZERO;
        for v in &mut me.verts {
            let p = t.transform_point3(Vec3::from(v.pos));
            v.pos = p.to_array();
            v.nrm = t.transform_vector3(Vec3::from(v.nrm)).normalize_or_zero().to_array();
            let tn = t.transform_vector3(Vec3::new(v.tan[0], v.tan[1], v.tan[2])).normalize_or_zero();
            v.tan = [tn.x, tn.y, tn.z, v.tan[3]];
            sum += p;
            lo = lo.min(p);
            hi = hi.max(p);
        }
        me.centre = (sum / me.verts.len().max(1) as f32).to_array();
    }
    if lo.x <= hi.x {
        m.bbox = [lo.to_array(), hi.to_array()];
    }
    Some(at)
}

/// Bind every part of an outfit to one skeleton, so the renderer can pose
/// the whole character. The parts' joint lists all start with the shared
/// ped skeleton; that prefix becomes the model's skeleton, in its bind pose
/// (the vertices already hold the body shape). A vertex on a part's own
/// helper bone goes to the nearest ancestor in the shared skeleton, and an
/// attached accessory to the bone it was moved onto. Returns whether a
/// skeleton was found.
fn rig_character(m: &mut glb::Model, parts: &[(std::ops::Range<usize>, Vec<glb::Joint>, Option<usize>)], skin: &[std::ops::Range<usize>]) -> bool {
    use glam::Mat4;
    // the shared skeleton: the shortest full-size joint list, where the
    // other parts' lists begin with the same names
    let Some(n) = parts.iter().map(|p| p.1.len()).filter(|&n| n >= 400).min() else { return false };
    let Some(canon) = parts.iter().find(|p| p.1.len() == n).map(|p| &p.1) else { return false };
    let mut joints: Vec<glb::Joint> = canon.iter().map(|j| glb::Joint { name: j.name.clone(), parent: j.parent.filter(|&p| p < n), local: j.local, ibm: j.ibm }).collect();
    // bind pose: local = inverse(parent bind) * bind
    let bind: Vec<Mat4> = joints.iter().map(|j| Mat4::from_cols_array(&j.ibm).inverse()).collect();
    for i in 0..n {
        let local = match joints[i].parent {
            Some(p) => bind[p].inverse() * bind[i],
            None => bind[i],
        };
        if local.is_finite() {
            joints[i].local = local.to_cols_array();
        }
    }
    // vertices that hang mostly on a part's own helper bones: (mesh, vertex)
    let mut helper: Vec<(usize, usize)> = Vec::new();
    for (range, pj, attached) in parts {
        let same = pj.len() >= n && (0..n).all(|i| pj[i].name == joints[i].name);
        if !same {
            continue;
        }
        // joint of this part -> joint of the shared skeleton
        let up = |mut j: usize| -> u16 {
            let mut guard = 0;
            while j >= n && guard < 64 {
                match pj.get(j).and_then(|x| x.parent) {
                    Some(p) if p != j => j = p,
                    _ => return 0,
                }
                guard += 1;
            }
            j.min(n - 1) as u16
        };
        let fixed = attached.map(|a| up(a));
        for mi in range.clone() {
            let Some(me) = m.meshes.get_mut(mi) else { continue };
            for (vi, v) in me.verts.iter_mut().enumerate() {
                if let Some(a) = fixed {
                    v.joints = [a, 0, 0, 0];
                    v.weights = [255, 0, 0, 0];
                    continue;
                }
                let tot: u32 = v.weights.iter().map(|w| *w as u32).sum();
                if tot == 0 {
                    // an unskinned vertex stays with the root
                    v.joints = [0; 4];
                    v.weights = [255, 0, 0, 0];
                    continue;
                }
                let own: u32 = (0..4).filter(|&k| v.joints[k] as usize >= n).map(|k| v.weights[k] as u32).sum();
                if own * 2 >= tot {
                    helper.push((mi, vi));
                }
                let mut w: [u32; 4] = std::array::from_fn(|k| v.weights[k] as u32 * 255 / tot);
                let rest = 255 - w.iter().sum::<u32>();
                w[0] += rest;
                for k in 0..4 {
                    v.joints[k] = if w[k] > 0 { up(v.joints[k] as usize) } else { 0 };
                    v.weights[k] = w[k] as u8;
                }
            }
            me.skinned = true;
        }
    }
    // Clothing on helper bones takes the bone weights of the nearest skin
    // vertex (the helper bones are moved by game data that is not decoded,
    // and the bone they hang from is often the torso, which would leave a
    // sleeve behind when the arm moves).
    const CELL: f32 = 0.03;
    let key = |p: [f32; 3]| ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32, (p[2] / CELL).floor() as i32);
    let mut grid: HashMap<(i32, i32, i32), Vec<([f32; 3], [u16; 4], [u8; 4])>> = HashMap::new();
    let in_skin = |mi: usize| skin.iter().any(|r| r.contains(&mi));
    for mi in skin.iter().cloned().flatten() {
        let Some(me) = m.meshes.get(mi).filter(|me| me.skinned) else { continue };
        for v in &me.verts {
            grid.entry(key(v.pos)).or_default().push((v.pos, v.joints, v.weights));
        }
    }
    if !grid.is_empty() {
        for (mi, vi) in helper {
            if in_skin(mi) {
                continue;
            }
            let Some(v) = m.meshes.get_mut(mi).and_then(|me| me.verts.get_mut(vi)) else { continue };
            let k = key(v.pos);
            let mut best: Option<(f32, [u16; 4], [u8; 4])> = None;
            for r in 1..=5i32 {
                for x in -r..=r {
                    for y in -r..=r {
                        for z in -r..=r {
                            // only the new shell of cells
                            if x.abs().max(y.abs()).max(z.abs()) != r && r > 1 {
                                continue;
                            }
                            for (p, j, w) in grid.get(&(k.0 + x, k.1 + y, k.2 + z)).into_iter().flatten() {
                                let d = (p[0] - v.pos[0]).powi(2) + (p[1] - v.pos[1]).powi(2) + (p[2] - v.pos[2]).powi(2);
                                if best.is_none_or(|b| d < b.0) {
                                    best = Some((d, *j, *w));
                                }
                            }
                        }
                    }
                }
                // a hit within the shells searched so far is the nearest
                if best.is_some_and(|b| b.0.sqrt() <= (r - 1).max(1) as f32 * CELL) {
                    break;
                }
            }
            if let Some((_, j, w)) = best {
                v.joints = j;
                v.weights = w;
            }
        }
    }
    m.joints = joints;
    true
}

/// Hide the skin triangles that lie right under a garment: a skin vertex is
/// covered when a point of a garment surface is within 9 mm of the line
/// through the vertex along its normal, between 15 mm inside and 50 mm
/// outside the skin; a triangle goes when all three vertices are covered.
/// Returns the number of triangles hidden.
fn hide_covered_skin(m: &mut glb::Model, skin: &[std::ops::Range<usize>], cover: &[std::ops::Range<usize>]) -> usize {
    use glam::Vec3;
    const CELL: f32 = 0.02;
    const STEP: f32 = 0.008;
    let key = |p: Vec3| ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32, (p.z / CELL).floor() as i32);
    // points on the garment surfaces, about 8 mm apart
    let mut grid: HashMap<(i32, i32, i32), Vec<Vec3>> = HashMap::new();
    for me in cover.iter().cloned().flatten().filter_map(|i| m.meshes.get(i)) {
        for t in me.indices.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(me.verts[i as usize].pos));
            let n = (((b - a).length().max((c - a).length()).max((c - b).length()) / STEP).ceil() as usize).clamp(1, 40);
            for i in 0..=n {
                for j in 0..=(n - i) {
                    let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
                    let p = a + (b - a) * u + (c - a) * v;
                    grid.entry(key(p)).or_default().push(p);
                }
            }
        }
    }
    if grid.is_empty() {
        return 0;
    }
    let covered = |p: Vec3, n: Vec3| -> bool {
        let k = key(p);
        // cells within 50 mm
        for x in -3..=3 {
            for y in -3..=3 {
                for z in -3..=3 {
                    let Some(v) = grid.get(&(k.0 + x, k.1 + y, k.2 + z)) else { continue };
                    for g in v {
                        let d = *g - p;
                        let along = d.dot(n);
                        if (-0.015..=0.05).contains(&along) && (d - n * along).length_squared() < 0.009 * 0.009 {
                            return true;
                        }
                    }
                }
            }
        }
        false
    };
    let mut hidden = 0;
    for i in skin.iter().cloned().flatten() {
        let Some(me) = m.meshes.get_mut(i) else { continue };
        let mut cov: Vec<bool> = me.verts.iter().map(|v| covered(Vec3::from(v.pos), Vec3::from(v.nrm).normalize_or_zero())).collect();
        // Keep the skin for two rings of vertices in from every edge of the
        // covered area, so the skin runs on under the hem (no gap between a
        // shirt and the body). Vertices doubled at texture seams count as
        // one place.
        let place = |p: [f32; 3]| ((p[0] * 2000.0).round() as i32, (p[1] * 2000.0).round() as i32, (p[2] * 2000.0).round() as i32);
        for _ in 0..2 {
            let open: std::collections::HashSet<(i32, i32, i32)> = me.verts.iter().zip(&cov).filter(|(_, c)| !**c).map(|(v, _)| place(v.pos)).collect();
            let mut next = cov.clone();
            for t in me.indices.chunks_exact(3) {
                if t.iter().any(|&i| !cov[i as usize] || open.contains(&place(me.verts[i as usize].pos))) {
                    t.iter().for_each(|&i| next[i as usize] = false);
                }
            }
            cov = next;
        }
        let before = me.indices.len() / 3;
        me.indices = me.indices.chunks_exact(3).filter(|t| !t.iter().all(|&i| cov[i as usize])).flatten().copied().collect();
        hidden += before - me.indices.len() / 3;
    }
    m.triangles = m.triangles.saturating_sub(hidden);
    hidden
}

/// Pose the feet for heeled shoes: rotate each foot about its ankle (toes
/// down) and the toes back about the toe joint, by the angle (0 to 44
/// degrees) that puts the most foot-skin vertices within 10 mm of the shoe
/// surface and the fewest below the shoe. Uses the skin's own weights on the
/// shared skeleton's foot and toe bones. Returns the angle when one was
/// applied.
fn pose_feet_for_heels(m: &mut glb::Model, skin: &[std::ops::Range<usize>], shoes: &[std::ops::Range<usize>]) -> Option<f32> {
    use glam::{Mat4, Vec3};
    let find = |n: &str| m.joints.iter().position(|j| j.name == n);
    let pos = |i: usize| Mat4::from_cols_array(&m.joints[i].ibm).inverse().transform_point3(Vec3::ZERO);
    let under = |mut i: usize, root: usize| loop {
        if i == root {
            return true;
        }
        match m.joints[i].parent.filter(|&p| p != i) {
            Some(p) => i = p,
            None => return false,
        }
    };
    // per side: ankle, toe joint, and per joint 0 = other, 1 = foot, 2 = toes
    let mut sides = Vec::new();
    for s in ["L", "R"] {
        let (foot, toe) = (find(&format!("SKEL_{s}_Foot"))?, find(&format!("SKEL_{s}_Toe0"))?);
        let bm_toe = find(&format!("BM_{s}_Toe"));
        let kind: Vec<u8> = (0..m.joints.len()).map(|i| if under(i, toe) || bm_toe.is_some_and(|b| under(i, b)) { 2 } else if under(i, foot) { 1 } else { 0 }).collect();
        sides.push((pos(foot), pos(toe), kind));
    }
    // shoe surface points and the shoes' lowest point
    const CELL: f32 = 0.01;
    let key = |p: Vec3| ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32, (p.z / CELL).floor() as i32);
    let mut grid: std::collections::HashSet<(i32, i32, i32)> = Default::default();
    let mut floor = f32::MAX;
    // heights of the shoe surface per 5 mm column (x, z), for the sole's top
    let col = |p: Vec3| ((p.x / 0.005).floor() as i32, (p.z / 0.005).floor() as i32);
    let mut cols: HashMap<(i32, i32), Vec<f32>> = HashMap::new();
    for me in shoes.iter().cloned().flatten().filter_map(|i| m.meshes.get(i)) {
        for t in me.indices.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(me.verts[i as usize].pos));
            let n = (((b - a).length().max((c - a).length()).max((c - b).length()) / 0.005).ceil() as usize).clamp(1, 40);
            for i in 0..=n {
                for j in 0..=(n - i) {
                    let p = a + (b - a) * (i as f32 / n as f32) + (c - a) * (j as f32 / n as f32);
                    grid.insert(key(p));
                    floor = floor.min(p.y);
                    cols.entry(col(p)).or_default().push(p.y);
                }
            }
        }
    }
    if grid.is_empty() {
        return None;
    }
    // The top of the sole in each column: the top of the lowest slab of
    // surface (walking up from the bottom until a gap of 8 mm). Straps
    // higher up do not count.
    let sole: HashMap<(i32, i32), f32> = cols
        .into_iter()
        .map(|(k, mut ys)| {
            ys.sort_by(|a, b| a.total_cmp(b));
            let mut top = ys[0];
            for y in &ys[1..] {
                if *y - top > 0.008 {
                    break;
                }
                top = *y;
            }
            (k, top)
        })
        .collect();
    // the two transforms of a side: the foot turned `deg` about the ankle,
    // and the toes turned back `toes` about the toe joint
    let mats = |ankle: Vec3, toe: Vec3, deg: f32, toes: f32| -> (Mat4, Mat4) {
        let about = |c: Vec3, a: f32| Mat4::from_translation(c) * Mat4::from_rotation_x(a) * Mat4::from_translation(-c);
        // front is +z and up is +y: a positive turn about x takes +z towards -y (toes down)
        let r1 = about(ankle, deg.to_radians());
        let r2 = about(r1.transform_point3(toe), -toes.to_radians()) * r1;
        (r1, r2)
    };
    let moved = |v: &glb::Vertex, side: &(Vec3, Vec3, Vec<u8>), mt: &(Mat4, Mat4)| -> Option<Vec3> {
        let tot: f32 = v.weights.iter().map(|x| *x as f32).sum();
        let p0 = Vec3::from(v.pos);
        let (mut p, mut on) = (Vec3::ZERO, 0.0);
        for k in 0..4 {
            if v.weights[k] == 0 {
                continue;
            }
            let w = v.weights[k] as f32 / tot.max(1.0);
            p += w * match side.2.get(v.joints[k] as usize) {
                Some(1) => {
                    on += w;
                    mt.0.transform_point3(p0)
                }
                Some(2) => {
                    on += w;
                    mt.1.transform_point3(p0)
                }
                _ => p0,
            };
        }
        (on > 0.0).then_some(p)
    };
    // Fit on both feet: skin that sinks into the sole counts against an
    // angle (three times), skin resting on the sole (within 8 mm above it)
    // or touching any shoe surface counts for it.
    let mut best = (0.0f32, 0.0f32, i64::MIN);
    for step in 0..=25 {
        let deg = step as f32 * 2.0;
        for tstep in 0..=((deg / 4.0) as i32 + 3) {
            let toes = tstep as f32 * 4.0;
            let mut score = 0i64;
            for side in &sides {
                let mt = mats(side.0, side.1, deg, toes);
                for me in skin.iter().cloned().flatten().filter_map(|i| m.meshes.get(i)) {
                    for v in &me.verts {
                        let Some(p) = moved(v, side, &mt) else { continue };
                        if p.y < floor - 0.002 {
                            score -= 3;
                            continue;
                        }
                        match sole.get(&col(p)) {
                            Some(top) if p.y < *top - 0.003 => score -= 3,
                            Some(top) if p.y < *top + 0.008 => score += 2,
                            _ => {
                                let k = key(p);
                                if (-1..=1).any(|x| (-1..=1).any(|y| (-1..=1).any(|z| grid.contains(&(k.0 + x, k.1 + y, k.2 + z))))) {
                                    score += 1;
                                }
                            }
                        }
                    }
                }
            }
            if score > best.2 {
                best = (deg, toes, score);
            }
        }
    }
    if best.0 < 4.0 {
        return None;
    }
    for side in &sides {
        let mt = mats(side.0, side.1, best.0, best.1);
        for i in skin.iter().cloned().flatten() {
            let Some(me) = m.meshes.get_mut(i) else { continue };
            for v in &mut me.verts {
                if let Some(p) = moved(v, side, &mt) {
                    // the normal turns with the stronger of the two parts
                    let toes = (0..4).filter(|&k| side.2.get(v.joints[k] as usize) == Some(&2)).map(|k| v.weights[k] as u32).sum::<u32>();
                    let foot = (0..4).filter(|&k| side.2.get(v.joints[k] as usize) == Some(&1)).map(|k| v.weights[k] as u32).sum::<u32>();
                    let r = if toes > foot { mt.1 } else { mt.0 };
                    v.nrm = r.transform_vector3(Vec3::from(v.nrm)).normalize_or_zero().to_array();
                    v.pos = p.to_array();
                }
            }
        }
    }
    Some(best.0)
}

/// Displacement field of the shaped skin: (neutral position, offset) per
/// skin vertex, in a grid for nearest-point lookups.
struct Wrap {
    pts: Vec<([f32; 3], [f32; 3])>,
    grid: HashMap<(i32, i32, i32), Vec<u32>>,
}

impl Wrap {
    const CELL: f32 = 0.04;

    fn key(p: [f32; 3]) -> (i32, i32, i32) {
        ((p[0] / Self::CELL).floor() as i32, (p[1] / Self::CELL).floor() as i32, (p[2] / Self::CELL).floor() as i32)
    }

    fn new(pts: Vec<([f32; 3], [f32; 3])>) -> Wrap {
        let mut grid: HashMap<(i32, i32, i32), Vec<u32>> = HashMap::new();
        for (i, (p, _)) in pts.iter().enumerate() {
            grid.entry(Self::key(*p)).or_default().push(i as u32);
        }
        Wrap { pts, grid }
    }

    /// Offset at a point: the inverse-square-distance mean of the six
    /// nearest skin vertices (searched up to about 30 cm away).
    fn at(&self, p: [f32; 3]) -> [f32; 3] {
        let k = Self::key(p);
        let mut best: Vec<(f32, u32)> = Vec::new();
        let mut r = 1;
        loop {
            best.clear();
            for x in -r..=r {
                for y in -r..=r {
                    for z in -r..=r {
                        if let Some(v) = self.grid.get(&(k.0 + x, k.1 + y, k.2 + z)) {
                            for &i in v {
                                let q = self.pts[i as usize].0;
                                best.push(((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2), i));
                            }
                        }
                    }
                }
            }
            // points found within (r - 1) cells are certainly the nearest
            let sure = ((r - 1) as f32 * Self::CELL).powi(2);
            if best.iter().filter(|b| b.0 <= sure).count() >= 6 || r >= 8 {
                break;
            }
            r += if best.is_empty() { 2 } else { 1 };
        }
        if best.is_empty() {
            return [0.0; 3];
        }
        best.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (mut o, mut wsum) = ([0f32; 3], 0f32);
        for (d2, i) in best.iter().take(6) {
            let w = 1.0 / (d2 + 1e-5);
            let d = self.pts[*i as usize].1;
            for c in 0..3 {
                o[c] += d[c] * w;
            }
            wsum += w;
        }
        o.map(|x| x / wsum)
    }

    /// Move a part with the skin. Small parts (a watch, glasses, a ring)
    /// move as one piece by the offset at their centre, so they keep their
    /// shape; larger ones move point by point.
    fn apply(&self, m: &mut glb::Model) {
        use glam::Vec3;
        let size = (Vec3::from(m.bbox[1]) - Vec3::from(m.bbox[0])).length();
        let rigid = (size < 0.3).then(|| self.at(((Vec3::from(m.bbox[0]) + Vec3::from(m.bbox[1])) * 0.5).to_array()));
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for me in &mut m.meshes {
            let mut sum = Vec3::ZERO;
            for v in &mut me.verts {
                let o = rigid.unwrap_or_else(|| self.at(v.pos));
                for c in 0..3 {
                    v.pos[c] += o[c];
                }
                let p = Vec3::from(v.pos);
                sum += p;
                lo = lo.min(p);
                hi = hi.max(p);
            }
            me.centre = (sum / me.verts.len().max(1) as f32).to_array();
        }
        if lo.x <= hi.x {
            m.bbox = [lo.to_array(), hi.to_array()];
        }
        if std::env::var("SIXNG_DEBUG_WRAP").is_ok() {
            let far = self.pts.iter().map(|p| Vec3::from(p.1).length()).fold(0f32, f32::max);
            eprintln!("[wrap] {} verts, size {size:.2}, rigid {:?}, field {} pts max offset {far:.3}, bbox {:?}", m.vertices, rigid, self.pts.len(), m.bbox);
        }
    }
}

/// Skin a part's vertices to the pose its GLB stores (the body shape: the
/// joints' node transforms against their inverse bind matrices), so the
/// merged, unskinned model shows it. Uses the first four influences.
fn apply_pose(m: &mut glb::Model) {
    use glam::{Mat4, Vec3};
    let n = m.joints.len();
    if n == 0 {
        return;
    }
    let mut world: Vec<Option<Mat4>> = vec![None; n];
    fn w(i: usize, js: &[glb::Joint], out: &mut Vec<Option<glam::Mat4>>, depth: usize) -> glam::Mat4 {
        if let Some(x) = out[i] {
            return x;
        }
        let l = glam::Mat4::from_cols_array(&js[i].local);
        let x = match js[i].parent.filter(|&p| p < js.len() && p != i && depth < 256) {
            Some(p) => w(p, js, out, depth + 1) * l,
            None => l,
        };
        out[i] = Some(x);
        x
    }
    let skin: Vec<Mat4> = (0..n).map(|i| w(i, &m.joints, &mut world, 0) * Mat4::from_cols_array(&m.joints[i].ibm)).collect();
    if skin.iter().all(|s| s.abs_diff_eq(Mat4::IDENTITY, 1e-5)) {
        return;
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for me in m.meshes.iter_mut().filter(|me| me.skinned) {
        let mut sum = Vec3::ZERO;
        for v in &mut me.verts {
            let tot: f32 = v.weights.iter().map(|x| *x as f32).sum();
            if tot <= 0.0 {
                continue;
            }
            let (p0, n0) = (Vec3::from(v.pos), Vec3::from(v.nrm));
            let (mut p, mut nr) = (Vec3::ZERO, Vec3::ZERO);
            for k in 0..4 {
                let Some(s) = skin.get(v.joints[k] as usize).filter(|_| v.weights[k] > 0) else { continue };
                let wt = v.weights[k] as f32 / tot;
                p += s.transform_point3(p0) * wt;
                nr += s.transform_vector3(n0) * wt;
            }
            if p.is_finite() {
                v.pos = p.to_array();
                v.nrm = nr.normalize_or_zero().to_array();
            }
            sum += Vec3::from(v.pos);
            lo = lo.min(Vec3::from(v.pos));
            hi = hi.max(Vec3::from(v.pos));
        }
        me.centre = (sum / me.verts.len().max(1) as f32).to_array();
    }
    if lo.x <= hi.x {
        m.bbox = [lo.to_array(), hi.to_array()];
    }
}

/// Skin tone (linear RGB) of a head at the base of its neck: the mean of the
/// face texture at the lowest vertices.
fn neck_tone(head: &glb::Model) -> Option<[f32; 3]> {
    let mut mask = Vec::new();
    let s = skin_colours(head, &mut mask);
    if s.len() < 50 || !head.materials.iter().any(|m| m.base.is_some()) {
        return None;
    }
    let mut z: Vec<f32> = s.iter().map(|x| x.0[2]).collect();
    z.sort_by(|a, b| a.total_cmp(b));
    let cut = z[z.len() * 8 / 100];
    let low: Vec<_> = s.iter().filter(|x| x.0[2] <= cut).collect();
    let mut c = [0f32; 3];
    for x in &low {
        for k in 0..3 {
            c[k] += x.1[k] as f32 / 255.0;
        }
    }
    Some(c.map(|v| v / low.len() as f32))
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
            look: None,
        });
    }
    // character outfit: "<archive>#outfit:<prefix>[#look=<n>]"
    if let Some((archive, prefix)) = q.split_once("#outfit:") {
        let (prefix, want_look) = match prefix.split_once("#look=") {
            Some((p, n)) => (p, n.trim().parse::<usize>().ok()),
            None => (prefix, None),
        };
        let (a, ix) = index_for(ws, archive, idx)?;
        let mut opts = crate::assemble::outfit_options(&ix, prefix);
        // The game's outfit list: the character's own outfits (clothes, body,
        // hair) instead of one own part per slot.
        let mut fx: HashMap<u64, PartFx> = HashMap::new();
        let mut look_info = None;
        let mut look_err = None;
        match sixng_core::pedlook::look(&ws.game, &a, &ix, prefix) {
            Ok(look) => {
                let k = want_look.unwrap_or(0).min(look.outfits.len().saturating_sub(1));
                let by_name = sixng_core::outfit::dedupe(ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| {
                    let n = e.name.as_ref()?.to_ascii_lowercase();
                    look.outfits.iter().any(|o| o.parts.iter().any(|p| p.drawable == n)).then_some((n, e))
                }));
                opts.iter_mut().for_each(|o| o.default = false);
                let mut info = LookInfo { model: look.model.clone(), labels: look.outfits.iter().map(|o| o.label.clone()).collect(), current: k, ..Default::default() };
                info.notes.extend(look.note.clone());
                for (oi, o) in look.outfits.iter().enumerate() {
                    for p in &o.parts {
                        if p.strands {
                            if oi == k {
                                info.strands.push((p.slot.clone(), p.drawable.clone(), p.guid.clone().unwrap_or_default()));
                                if let Some((pal, t)) = p.tint.as_ref().and_then(|t| Some((u32::from_str_radix(&t.palette, 16).ok()?, t))) {
                                    info.strand_tints.push((p.slot.clone(), pal, t.rows[0]));
                                }
                            }
                            continue;
                        }
                        let Some(e) = by_name.get(&p.drawable) else { continue };
                        match opts.iter_mut().find(|x| x.off == e.off) {
                            Some(x) => {
                                x.group = p.slot.clone();
                                x.default |= oi == k;
                            }
                            None => opts.push(crate::assemble::PartOpt { off: e.off, id: e.id.clone(), name: p.drawable.clone(), group: p.slot.clone(), label: p.drawable.trim_start_matches(&format!("{prefix}_")).to_string(), default: oi == k }),
                        }
                        if oi == k {
                            let body = p.slot == "body upper" || p.slot == "body lower";
                            fx.insert(e.off, PartFx { tint: p.tint.as_ref().and_then(|t| Some((u32::from_str_radix(&t.palette, 16).ok()?, t.rows[0]))).filter(|t| t.1 != 255), colour: p.colour.map(|c| [c[1], c[2], c[3]]), tag: p.colour_tag, head: p.slot == "head", body, skeletal: body || matches!(p.slot.as_str(), "head" | "eyes" | "teeth" | "teeth accessory"), covers: matches!(p.slot.as_str(), "top" | "outer" | "bottom" | "full" | "feet" | "gloves"), layer: match p.slot.as_str() { "outer" => 2, "top" | "bottom" | "full" => 1, _ => 0 }, heel: p.slot == "feet" && p.drawable.split('_').any(|t| matches!(t, "sh" | "mh" | "hh")), skins: if p.slot == "head" { p.skins.iter().filter_map(|(a, n)| Some((u64::from_str_radix(a, 16).ok()?, u64::from_str_radix(n, 16).unwrap_or(0)))).collect() } else { Vec::new() } });
                        }
                    }
                }
                // strand hair: the grooms placed for each hair asset
                match sixng_core::hairlink::get(&ws.work, &a, &ix.entries) {
                    Ok(t) => {
                        for (slot, placeholder, guid) in &info.strands {
                            // by the item's hair asset, else by the character's own placeholder
                            let Some(l) = u64::from_str_radix(guid, 16).ok().and_then(|g| t.resolve(g, look.fit.as_deref())).or_else(|| guid.is_empty().then(|| t.by_placeholder(placeholder)).flatten()) else { continue };
                            let colour = info.strand_tints.iter().find(|t| t.0 == *slot && t.1 == 0x1e64_f584).and_then(|t| Some(sixng_core::pedtint::palettes(&ws.game).ok()?.get(&t.1)?.sample(t.2, 128)));
                            info.hair.push(HairPick { slot: slot.clone(), placeholder: l.placeholder.clone(), confidence: l.confidence, grooms: t.uncontested(l), off_head: 0, shown: l.confidence >= 0.5 && slot != "body hair", colour });
                        }
                    }
                    Err(e) => info.notes.push(format!("Strand hair: {e}")),
                }
                let o = &look.outfits[k];
                info.body_shape = o.body_shape.clone();
                if !o.unresolved.is_empty() {
                    info.notes.push(format!("{} item(s) of this outfit have no model: {}", o.unresolved.len(), o.unresolved.join(", ")));
                }
                look_info = Some(info);
            }
            Err(e) => look_err = Some(e),
        }
        if opts.is_empty() {
            return Err(anyhow::anyhow!("no parts found for {prefix}"));
        }
        let (selected, parts) = pick(&ix, &opts, &sel);
        if parts.is_empty() {
            return Err(anyhow::anyhow!("no parts of {prefix} selected"));
        }
        let (model, first, names, errs, skin, _, _, notes) = load_parts(&a, &parts, cache, lod, None, &fx, &ws.game);
        let model = model
            .ok_or_else(|| anyhow::anyhow!("no part of {prefix} loaded: {}", errs.join("; ")))?;
        if let Some(l) = &mut look_info {
            l.notes.extend(notes);
            // Strand roots must lie on this character: pieces that do not
            // (another head shape, a neighbour's piece) are left out. Only in
            // the neutral body shape, where the skin is where the grooms expect it.
            if *BODY_SHAPE.lock().unwrap() == (0.0, 0.0) && !l.hair.is_empty() {
                const CELL: f32 = 0.012;
                let key = |p: &[f32; 3]| ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32, (p[2] / CELL).floor() as i32);
                // hair, brows, beards and lashes must root on the head; body
                // hair on the head or the body
                let is_head = |n: &str| n.contains("_head_") || n.starts_with("head_");
                let head_cells: std::collections::HashSet<(i32, i32, i32)> = skin.iter().filter(|(n, _)| is_head(n)).flat_map(|(_, pts)| pts.iter().map(key)).collect();
                let all_cells: std::collections::HashSet<(i32, i32, i32)> = skin.iter().filter(|(n, _)| is_head(n) || n.starts_with("hand_") || n.starts_with("feet_")).flat_map(|(_, pts)| pts.iter().map(key)).collect();
                if !head_cells.is_empty() {
                    for h in &mut l.hair {
                        let cells = if h.slot == "body hair" { &all_cells } else { &head_cells };
                        let all = h.grooms.len();
                        h.grooms.retain(|&off| {
                            let Ok(g) = sixng_core::groom::load(&a, &ix.entries, off) else { return true };
                            let k = g.points_per_strand.max(1);
                            let n = g.strands.max(1);
                            let on = (0..g.strands).filter(|&s| g.points.get(s * k).is_some_and(|r| {
                                let c = key(r);
                                (-1..=1).any(|x| (-1..=1).any(|y| (-1..=1).any(|z| cells.contains(&(c.0 + x, c.1 + y, c.2 + z)))))
                            })).count();
                            on * 10 >= n * 8
                        });
                        h.off_head = all - h.grooms.len();
                        // body hair is fine fuzz in the game; drawn at hair
                        // width it reads as fur, so it is offered but starts off
                        h.shown = h.slot != "body hair" && !h.grooms.is_empty() && (h.confidence >= 0.5 || (h.confidence >= 0.2 && h.off_head == 0));
                    }
                }
            }
        }
        let info = serde_json::json!({ "assembled_from": names, "ids": parts.iter().map(|e| e.id.clone()).collect::<Vec<_>>(), "errors": errs, "outfit_list": look_err.map(|e| format!("not used: {e}")) });
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
            look: look_info,
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
        let (m, _, names, _, skin, mut colours, mask, _) = load_parts(&a, &parts, cache, level, Some(model), &HashMap::new(), &ws.game);
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
            look: None,
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
            look: None,
        });
        }
    }
    // vehicles: mod parts (bonnets, bumpers, spoilers, exhausts ...) are
    // separate models; the vehicle data says which bone each is modelled
    // around and which stock bones it replaces. None is on by default.
    let mut veh_opts = Vec::new();
    if e.rtype.as_deref() == Some("ft") && info["wheel_mounts"].as_array().is_some_and(|w| !w.is_empty()) {
        let db = sixng_core::names::work_dir().and_then(|w| crate::vehicle_db::shared(&export::game_root(&a).to_string_lossy(), &w.to_string_lossy()).ok());
        if let Some(v) = db.as_ref().and_then(|db| crate::vehicle_db::find(db, &crate::vehicle_db::model_key(name))) {
            let (opts, kits) = vehicle_part_options(&ix, name, v);
            let chosen: Vec<u64> = sel.clone().unwrap_or_default().into_iter().filter(|o| kits.contains_key(o)).collect();
            if !chosen.is_empty() {
                let joaat = |s: &str| sixng_core::meta7a::joaat(&s.to_ascii_lowercase());
                let mut model = model;
                // stock pieces the chosen parts replace
                let hidden: std::collections::HashSet<u32> = chosen.iter().flat_map(|o| kits[o].iter().flat_map(|k| k.2.iter().copied())).collect();
                let gone: Vec<bool> = model.joints.iter().map(|j| hidden.contains(&joaat(&j.name))).collect();
                if gone.iter().any(|g| *g) {
                    for me in model.meshes.iter_mut().filter(|m| m.skinned) {
                        let keep: Vec<u32> = me.indices.chunks_exact(3).filter(|t| !t.iter().all(|&i| gone.get(me.verts[i as usize].joints[0] as usize).copied().unwrap_or(false))).flatten().copied().collect();
                        me.indices = keep;
                    }
                }
                let bind: HashMap<u32, sixng_core::weapon::Mat> = info["drawable"]["skeleton"]
                    .as_array()
                    .map(|b| b.iter().filter_map(|b| Some((joaat(b["name"].as_str()?), serde_json::from_value(b["bind_matrix"].clone()).ok()?))).collect())
                    .unwrap_or_default();
                let (mut names, mut errs) = (Vec::new(), Vec::new());
                for o in &chosen {
                    for (off, bone, _) in &kits[o] {
                        let Some(pe) = ix.entries.iter().find(|x| x.off == *off) else { continue };
                        let pn = pe.name.clone().unwrap_or_default();
                        match export_load(&a, pe, cache, level) {
                            Ok((mut m, _, _)) => {
                                // the part is modelled around its bone: put it at the bone's bind pose
                                let t = bind.get(bone).copied().unwrap_or(sixng_core::weapon::IDENTITY);
                                let c = Mat4::from_cols_array(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
                                transform_model(&mut m, c * Mat4::from_cols_array(&t) * c.inverse());
                                let first = model.meshes.len();
                                glb::merge(&mut model, m);
                                // and follows that bone (an opened bonnet takes its replacement with it)
                                if let Some(j) = model.joints.iter().position(|j| joaat(&j.name) == *bone) {
                                    for me in &mut model.meshes[first..] {
                                        me.skinned = true;
                                        for v in &mut me.verts {
                                            v.joints = [j as u16, 0, 0, 0];
                                            v.weights = [255, 0, 0, 0];
                                        }
                                    }
                                }
                                names.push(pn);
                            }
                            Err(err) => errs.push(format!("{pn}: {err:#}")),
                        }
                    }
                }
                let mut info = info;
                if let Some(o) = info.as_object_mut() {
                    o.insert("assembled_from".into(), serde_json::json!(names));
                    o.insert("assembly_note".into(), "mod parts are placed at the bind pose of the bone the vehicle data names for them, and the stock pieces on the bones they replace are left out".into());
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
                    selected: chosen,
                    base: String::new(),
                    fur: None,
                    fur_skin: Default::default(),
            look: None,
                });
            }
            veh_opts = opts;
        }
    }
    Ok(Loaded {
        model,
        lods,
        lod,
        out_root: cache.to_path_buf(),
        source: format!("{} {}", e.id, name).trim().to_string(),
        info,
        parts: veh_opts,
        selected: Vec::new(),
        base: String::new(),
        fur: None,
        fur_skin: Default::default(),
            look: None,
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

/// Radial scale of rim and tyre bead that puts the rim at `ratio` x the
/// tyre's outer radius (1 with no ratio, a single model, or a ratio the
/// models cannot take).
fn rim_factor(boxes: &[[[f32; 3]; 2]], ratio: f32) -> f32 {
    let radius = |b: &[[f32; 3]; 2]| b[0][1].abs().max(b[1][1].abs()).max(b[0][2].abs()).max(b[1][2].abs());
    let mut r: Vec<f32> = boxes.iter().map(radius).collect();
    r.sort_by(f32::total_cmp);
    match (r.first(), r.last()) {
        (Some(&rim), Some(&tyre)) if r.len() >= 2 && ratio > 0.0 && rim > 0.0 && tyre > rim => (ratio * tyre / rim).clamp(0.5, 0.97 * tyre / rim),
        _ => 1.0,
    }
}

/// The movers of the side windows (`window_lf_mover` with the glass on its
/// child `window_lf`): joint index, the downward direction along the glass
/// and a point on the glass's bottom edge, in the mover's own space. The
/// plane of the glass is taken from its triangles.
/// A leg or a door of a retractable landing gear: its joint, the axis it
/// turns about (in the joint's own space) and the angle of the full travel.
struct GearPart {
    joint: usize,
    axis: Vec3,
    angle: f32,
    door: bool,
    /// a leg's mesh in model space (bind pose): what is fitted inside it
    /// and hangs on no leg bone goes up with it
    bbox: [Vec3; 2],
    /// a wheel bone outside the leg's bones that sits in a leg's mesh: the
    /// leg's pivot and axis in model space, which it turns about instead
    follow: Option<(Vec3, Vec3)>,
    /// a door's actuator rod: drawn only while the doors are open
    hide: bool,
}

/// The retractable landing gear of an aircraft: only models with gear door
/// bones (`gear_door_*`, `gear_inner_door_*`); fixed gear and floats carry
/// leg bones too. Nothing in the files says how a leg folds, so it is
/// worked out from the meshes in the bind pose (gear down, doors open):
///
/// - a leg turns a quarter turn about a horizontal axis through its bone,
///   towards its doors (the middle of the door meshes on its side within
///   2.5 m along the body), along the body or across it, whichever is the
///   larger part; where the doors lie within 0.3 of the leg's length of
///   the bone, a leg on the centre line folds forwards and a side leg
///   inwards;
/// - a door that does not hang on a leg turns about its longer horizontal
///   side until it lies level: towards the nearest leg when it hangs
///   straight down, the way it already leans otherwise;
/// - wheel bones and brace or strut bones that hang from the body instead
///   of a leg bone go up with the leg they belong to, as one piece.
fn find_gear(model: &glb::Model) -> Vec<GearPart> {
    let kind = |n: &str| -> Option<bool> {
        let n = n.to_ascii_lowercase();
        if n.ends_with("_mesh") || n.ends_with("_parent") || n.ends_with("_frag") {
            return None;
        }
        if n.starts_with("gear_door_") || n.starts_with("gear_inner_door_") {
            return Some(true);
        }
        let r = n.strip_prefix("gear_")?;
        let r = r.trim_end_matches(|c: char| c.is_ascii_digit());
        matches!(r, "f" | "rl" | "rr" | "lf" | "rf" | "lr" | "lm" | "rm").then_some(false)
    };
    let kinds: Vec<Option<bool>> = model.joints.iter().map(|j| kind(&j.name)).collect();
    if !kinds.iter().any(|k| *k == Some(true)) || !kinds.iter().any(|k| *k == Some(false)) {
        return Vec::new();
    }
    // the leg or door each joint belongs to (itself or the nearest above)
    let owner_of = |i: usize| {
        let (mut k, mut depth) = (Some(i), 0);
        while let Some(j) = k {
            if kinds[j].is_some() {
                return Some(j);
            }
            depth += 1;
            k = model.joints[j].parent.filter(|p| *p != j && depth < 64);
        }
        None
    };
    let owner: Vec<Option<usize>> = (0..model.joints.len()).map(owner_of).collect();
    // model-space box and middle of each part's vertices
    let mut boxes: HashMap<usize, (Vec3, Vec3, Vec3, f32)> = HashMap::new();
    for me in model.meshes.iter().filter(|m| m.skinned) {
        for v in &me.verts {
            let Some(o) = (0..4).filter(|k| v.weights[*k] > 127).find_map(|k| owner.get(v.joints[k] as usize).copied().flatten()) else { continue };
            let p = Vec3::from(v.pos);
            let e = boxes.entry(o).or_insert((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), Vec3::ZERO, 0.0));
            *e = (e.0.min(p), e.1.max(p), e.2 + p, e.3 + 1.0);
        }
    }
    let pivot = |i: usize| Mat4::from_cols_array(&model.joints[i].ibm).inverse().w_axis.truncate();
    let part = |i: usize| boxes.get(&i).filter(|b| b.3 >= 8.0).map(|b| (pivot(i), b.2 / b.3, b.1 - b.0));
    let legs: Vec<(usize, Vec3, Vec3, Vec3)> = (0..kinds.len()).filter(|i| kinds[*i] == Some(false)).filter_map(|i| part(i).map(|(p, c, e)| (i, p, c, e))).collect();
    let doors: Vec<(usize, Vec3, Vec3, Vec3)> = (0..kinds.len()).filter(|i| kinds[*i] == Some(true)).filter_map(|i| part(i).map(|(p, c, e)| (i, p, c, e))).collect();
    // model axis -> the joint's own space
    let local = |i: usize, a: Vec3| Mat4::from_cols_array(&model.joints[i].ibm).transform_vector3(a).normalize_or_zero();
    let quarter = std::f32::consts::FRAC_PI_2;
    let mut out = Vec::new();
    for &(i, p, c, e) in &legs {
        let centre = p.x.abs() < 0.3;
        let mine: Vec<Vec3> = doors.iter().filter(|d| (d.2.z - p.z).abs() < 2.5 && if centre { d.2.x.abs() < 1.0 } else { d.2.x * p.x > 0.0 }).map(|d| d.2).collect();
        let to_bay = if mine.is_empty() { Vec3::ZERO } else { mine.iter().copied().sum::<Vec3>() / mine.len() as f32 - p };
        let h = Vec3::new(to_bay.x, 0.0, to_bay.z);
        let dir = if h.length() < 0.3 * e.y.max(0.1) {
            // forwards is -Z; inwards is towards the centre line
            if centre { Vec3::NEG_Z } else { Vec3::new(-p.x.signum(), 0.0, 0.0) }
        } else if h.x.abs() > h.z.abs() {
            Vec3::new(h.x.signum(), 0.0, 0.0)
        } else {
            Vec3::new(0.0, 0.0, h.z.signum())
        };
        // the leg hangs from its bone: a quarter turn about down x dir
        // brings it level, pointing along dir
        let down = (c - p).normalize_or(Vec3::NEG_Y);
        let axis = local(i, down.cross(dir));
        if axis != Vec3::ZERO {
            let b = boxes[&i];
            out.push(GearPart { joint: i, axis, angle: quarter, door: false, bbox: [b.0, b.1], follow: None, hide: false });
        }
    }
    for &(i, p, c, e) in &doors {
        // a door on a leg goes up with it
        let on_leg = {
            let (mut k, mut depth, mut found) = (model.joints[i].parent, 0, false);
            while let Some(j) = k {
                if kinds[j] == Some(false) {
                    found = true;
                    break;
                }
                depth += 1;
                k = model.joints[j].parent.filter(|q| *q != j && depth < 64);
            }
            found
        };
        let Some(leg) = legs.iter().min_by(|a, b| (a.1 - p).length().total_cmp(&(b.1 - p).length())) else { continue };
        if on_leg {
            continue;
        }
        // hinge along the longer horizontal side; the lower edge swings
        // towards the leg across that side
        let hinge = if e.z >= e.x { Vec3::Z } else { Vec3::X };
        let across = if hinge == Vec3::Z { Vec3::X } else { Vec3::Z };
        // where the door hangs from its hinge, seen along the hinge
        let hang = (c - p) - hinge * (c - p).dot(hinge);
        let hang = hang.normalize_or(Vec3::NEG_Y);
        // a door hanging down swings towards the leg; one that already
        // leans well to a side closes that way, by the angle it has left
        let lean = hang.dot(across);
        let side = if lean.abs() > 0.35 { lean } else { (leg.1 - c).dot(across) };
        let want = across * if side >= 0.0 { 1.0 } else { -1.0 };
        let axis = local(i, hang.cross(want));
        let angle = hang.dot(want).clamp(-1.0, 1.0).acos();
        if axis != Vec3::ZERO && angle > 0.02 {
            out.push(GearPart { joint: i, axis, angle, door: true, bbox: NOWHERE, follow: None, hide: false });
        }
    }
    // wheel bones that hang on no leg bone but sit in a leg's mesh
    let pad = Vec3::splat(0.25);
    let mut followers = Vec::new();
    for (i, j) in model.joints.iter().enumerate() {
        if !is_wheel_bone(&j.name.to_ascii_lowercase()) || owner[i].is_some_and(|o| kinds[o] == Some(false)) {
            continue;
        }
        let p = pivot(i);
        if let Some(g) = out.iter().find(|g| !g.door && p.cmpge(g.bbox[0] - pad).all() && p.cmple(g.bbox[1] + pad).all()) {
            let to_model = Mat4::from_cols_array(&model.joints[g.joint].ibm).inverse();
            followers.push(GearPart { joint: i, axis: Vec3::ZERO, angle: g.angle, door: false, bbox: NOWHERE, follow: Some((to_model.w_axis.truncate(), to_model.transform_vector3(g.axis).normalize_or_zero())), hide: false });
        }
    }
    // braces and struts that hang from the body, not from a leg bone
    // (`IK_landgear_upr_strt_rr`, `landgear_strut_rr_1_mesh` ...): the top
    // one of each chain goes with the nearest leg, within 1.2 of the leg's
    // height, and takes the bones below it along
    // door actuator rods (`strut_01_female_lookat_lf`, `strut_male_mesh_f`):
    // two halves that aim at each other; they are not posed here, so they
    // are put away once the doors start to close
    let is_rod = |i: usize| {
        let n = model.joints[i].name.to_ascii_lowercase();
        n.contains("strut") && (n.contains("female") || n.contains("male"))
    };
    for i in (0..model.joints.len()).filter(|i| is_rod(*i)) {
        followers.push(GearPart { joint: i, axis: Vec3::ZERO, angle: 0.0, door: true, bbox: NOWHERE, follow: None, hide: true });
    }
    let is_brace = |i: usize| {
        let n = model.joints[i].name.to_ascii_lowercase();
        (n.contains("landgear") || n.contains("strut")) && !is_rod(i) && kinds[i].is_none() && !owner[i].is_some_and(|o| kinds[o] == Some(false))
    };
    // the top brace above each joint, and the middle of each brace's mesh
    let top_brace: Vec<Option<usize>> = (0..model.joints.len())
        .map(|i| {
            let (mut k, mut depth, mut top) = (Some(i), 0, None);
            while let Some(j) = k {
                if is_brace(j) {
                    top = Some(j);
                }
                depth += 1;
                k = model.joints[j].parent.filter(|q| *q != j && depth < 64);
            }
            top
        })
        .collect();
    let mut middles: HashMap<usize, (Vec3, f32)> = HashMap::new();
    for me in model.meshes.iter().filter(|m| m.skinned) {
        for v in &me.verts {
            if let Some(o) = (0..4).filter(|k| v.weights[*k] > 127).find_map(|k| top_brace.get(v.joints[k] as usize).copied().flatten()) {
                let e = middles.entry(o).or_insert((Vec3::ZERO, 0.0));
                *e = (e.0 + Vec3::from(v.pos), e.1 + 1.0);
            }
        }
    }
    for i in 0..model.joints.len() {
        if top_brace[i] != Some(i) {
            continue;
        }
        let p = pivot(i);
        let near = legs.iter().filter(|l| (l.1 - p).length() < 1.2 * l.3.y).min_by(|a, b| (a.1 - p).length().total_cmp(&(b.1 - p).length()));
        if let Some(g) = near.and_then(|l| out.iter().find(|g| g.joint == l.0 && !g.door)) {
            let to_model = Mat4::from_cols_array(&model.joints[g.joint].ibm).inverse();
            let (leg_pivot, leg_axis) = (to_model.w_axis.truncate(), to_model.transform_vector3(g.axis).normalize_or_zero());
            // going up with the leg must lift the brace; one anchored on
            // the far side of the leg's pivot would swing down and out
            // instead, and is put away with the doors
            let mid = middles.get(&i).filter(|m| m.1 > 0.0).map_or(p, |m| m.0 / m.1);
            let up = leg_pivot + Quat::from_axis_angle(leg_axis, g.angle) * (mid - leg_pivot);
            if up.y >= mid.y - 0.02 {
                followers.push(GearPart { joint: i, axis: Vec3::ZERO, angle: g.angle, door: false, bbox: NOWHERE, follow: Some((leg_pivot, leg_axis)), hide: false });
            } else {
                followers.push(GearPart { joint: i, axis: Vec3::ZERO, angle: 0.0, door: true, bbox: NOWHERE, follow: None, hide: true });
            }
        }
    }
    out.extend(followers);
    out
}

/// A box nothing lies in.
const NOWHERE: [Vec3; 2] = [Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)];

/// What an aircraft bone does in the viewer.
#[derive(Clone, Copy, PartialEq, Debug)]
enum AirKind {
    /// rotor or propeller hub: turns
    Spin,
    /// the blades shown when still or slow (`_slow` meshes)
    Still,
    /// the blurred blades shown at speed (`_fast` meshes, `_disc`)
    Blur,
    Flap,
    Aileron,
    Elevator,
    Rudder,
    Airbrake,
}

/// One moving part of an aircraft: its joint, the axis it turns about (in
/// the joint's own space) and the sign that turns it the named way (a flap
/// or elevator down, an air brake up, the left aileron down and the right
/// one up, the rudder's trailing edge to +X).
struct AirPart {
    joint: usize,
    kind: AirKind,
    axis: Vec3,
    sign: f32,
}

fn air_kind(name: &str) -> Option<AirKind> {
    let n = name.to_ascii_lowercase();
    let numbered = |pre: &str| n.strip_prefix(pre).is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()));
    let blade = n.starts_with("rotor_") || n.starts_with("prop_");
    Some(if n == "rotor_main" || n == "rotor_rear" || n == "rotor_main_2" || numbered("prop_") {
        AirKind::Spin
    } else if blade && (n.ends_with("_fast") || n.contains("_fast_") || n.ends_with("_disc")) {
        AirKind::Blur
    } else if blade && (n.ends_with("_slow") || n.contains("_slow_")) {
        AirKind::Still
    } else if matches!(n.as_str(), "flap_l" | "flap_r" | "flap_lm" | "flap_rm") {
        AirKind::Flap
    } else if n == "aileron_l" || n == "aileron_r" {
        AirKind::Aileron
    } else if n == "elevator_l" || n == "elevator_r" {
        AirKind::Elevator
    } else if n == "rudder" {
        AirKind::Rudder
    } else if n.starts_with("airbrake_") && !n.ends_with("_mesh") && !n.ends_with("_flex") {
        AirKind::Airbrake
    } else {
        return None;
    })
}

/// The rotors, propellers and control surfaces of an aircraft. A part's
/// axis is the axis of its joint along which its mesh (the vertices on the
/// joint and the joints below it) is thinnest for a rotor or propeller and
/// longest for a control surface; the sign comes from which way the middle
/// of the mesh moves.
fn find_air_parts(model: &glb::Model) -> Vec<AirPart> {
    let kinds: Vec<Option<AirKind>> = model.joints.iter().map(|j| air_kind(&j.name)).collect();
    if kinds.iter().all(|k| k.is_none()) {
        return Vec::new();
    }
    let moves = |k: AirKind| !matches!(k, AirKind::Still | AirKind::Blur);
    // the moving part each joint belongs to (itself or the nearest above)
    let owner: Vec<Option<usize>> = (0..model.joints.len())
        .map(|i| {
            let (mut k, mut depth) = (Some(i), 0);
            while let Some(j) = k {
                if kinds[j].is_some_and(moves) {
                    return Some(j);
                }
                depth += 1;
                k = model.joints[j].parent.filter(|p| *p != j && depth < 64);
            }
            None
        })
        .collect();
    // extent and middle of each part's vertices in its joint's space
    let mut boxes: HashMap<usize, (Vec3, Vec3, Vec3, f32)> = HashMap::new();
    for me in model.meshes.iter().filter(|m| m.skinned) {
        for v in &me.verts {
            let Some(o) = (0..4).filter(|k| v.weights[*k] > 127).find_map(|k| owner.get(v.joints[k] as usize).copied().flatten()) else { continue };
            let p = Mat4::from_cols_array(&model.joints[o].ibm).transform_point3(Vec3::from(v.pos));
            let e = boxes.entry(o).or_insert((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), Vec3::ZERO, 0.0));
            *e = (e.0.min(p), e.1.max(p), e.2 + p, e.3 + 1.0);
        }
    }
    let mut out = Vec::new();
    for (i, k) in kinds.iter().enumerate() {
        let Some(k) = *k else { continue };
        if !moves(k) {
            out.push(AirPart { joint: i, kind: k, axis: Vec3::Y, sign: 1.0 });
            continue;
        }
        let Some(&(lo, hi, sum, n)) = boxes.get(&i).filter(|b| b.3 >= 8.0) else { continue };
        let e = hi - lo;
        let pick = |want_min: bool| {
            let v = [e.x, e.y, e.z];
            let k = (0..3).min_by(|a, b| if want_min { v[*a].total_cmp(&v[*b]) } else { v[*b].total_cmp(&v[*a]) }).unwrap_or(0);
            [Vec3::X, Vec3::Y, Vec3::Z][k]
        };
        let axis = pick(k == AirKind::Spin);
        // a small turn about the axis moves the middle of the mesh by
        // axis x middle; in model space that tells down from up
        let to_model = Mat4::from_cols_array(&model.joints[i].ibm).inverse();
        let d = to_model.transform_vector3(axis.cross(sum / n));
        let left = model.joints[i].name.to_ascii_lowercase().ends_with("_l");
        let sign = match k {
            AirKind::Spin => 1.0,
            AirKind::Rudder => if d.x >= 0.0 { 1.0 } else { -1.0 },
            AirKind::Airbrake => if d.y >= 0.0 { 1.0 } else { -1.0 },
            // down; the right aileron goes the other way
            _ => (if d.y <= 0.0 { 1.0 } else { -1.0 }) * if k == AirKind::Aileron && !left { -1.0 } else { 1.0 },
        };
        out.push(AirPart { joint: i, kind: k, axis, sign });
    }
    out
}

fn find_window_movers(model: &glb::Model) -> Vec<(usize, Vec3, Vec3)> {
    let mut out = Vec::new();
    for (mi, mover) in model.joints.iter().enumerate() {
        let name = mover.name.to_ascii_lowercase();
        let Some(glass_name) = name.strip_suffix("_mover").filter(|n| n.starts_with("window_")) else { continue };
        let Some(gi) = model.joints.iter().position(|j| j.name.eq_ignore_ascii_case(glass_name)) else { continue };
        let (mut normal, mut pts) = (Vec3::ZERO, Vec::new());
        for m in &model.meshes {
            let on = |i: u32| m.verts.get(i as usize).is_some_and(|v| (0..4).any(|k| v.joints[k] as usize == gi && v.weights[k] > 127));
            // the plane from the glass triangles themselves (area-weighted
            // face normals; both faces of the glass count the same way)
            for t in m.indices.chunks_exact(3).filter(|t| t.iter().all(|&i| on(i))) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(m.verts[i as usize].pos));
                let n = (b - a).cross(c - a);
                normal += if n.dot(normal) < 0.0 { -n } else { n };
                pts.extend([a, b, c]);
            }
        }
        if pts.len() < 8 || normal.length() < 1e-3 {
            continue;
        }
        let normal = normal.normalize();
        // straight down, kept in the plane of the glass
        let down = (Vec3::NEG_Y - normal * Vec3::NEG_Y.dot(normal)).normalize_or_zero();
        if down == Vec3::ZERO {
            continue;
        }
        // the lowest point of the glass (furthest along `down`)
        let Some(bottom) = pts.iter().copied().max_by(|a, b| a.dot(down).total_cmp(&b.dot(down))) else { continue };
        let ibm = Mat4::from_cols_array(&mover.ibm);
        out.push((mi, ibm.transform_vector3(down).normalize_or_zero(), ibm.transform_point3(bottom)));
    }
    out
}

/// Steering wheel turn per unit of road wheel angle, before the vehicle's
/// steerWheelMult (an estimate: about 1.2 turns each way at 35 degrees on a
/// car whose steerWheelMult is 0.7).
const STEERING_RATIO: f32 = 17.0;

/// A bone that steering turns (`wheel_turn_lf`, `wheel_turn_rf`).
fn is_steer_bone(n: &str) -> bool {
    n.to_ascii_lowercase().starts_with("wheel_turn_")
}

/// The steering wheel's joint and the axis it turns about, in joint space:
/// the direction in which the vertices skinned to it are thinnest (a
/// steering wheel is a flat ring around its column).
fn find_steering_wheel(model: &glb::Model) -> Option<(usize, Vec3)> {
    let ji = model.joints.iter().position(|j| j.name.eq_ignore_ascii_case("steeringwheel"))?;
    let ibm = Mat4::from_cols_array(&model.joints[ji].ibm);
    let (mut lo, mut hi, mut n) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), 0usize);
    for m in &model.meshes {
        for v in &m.verts {
            if (0..4).any(|k| v.joints[k] as usize == ji && v.weights[k] > 127) {
                let p = ibm.transform_point3(Vec3::from(v.pos));
                lo = lo.min(p);
                hi = hi.max(p);
                n += 1;
            }
        }
    }
    if n < 8 {
        return None;
    }
    let e = hi - lo;
    let axis = if e.x <= e.y && e.x <= e.z {
        Vec3::X
    } else if e.y <= e.z {
        Vec3::Y
    } else {
        Vec3::Z
    };
    Some((ji, axis))
}

/// One siren pattern on the loaded model: per lamp the joint, its steps of
/// (phase offset, flash type number) and its light (linear colour x intensity).
struct Siren {
    label: String,
    speed: f32,
    lamps: Vec<(usize, Vec<(f32, u32)>, [f32; 3])>,
}

/// The light patterns of `lights` that reach a siren, strobe or beacon bone
/// of `model`: the ones `linked` by the configuration first, then the rest
/// of the vehicle's light pack. A lamp's lens bone (`<lamp>_glass`) flashes
/// with it.
fn find_sirens(model: &glb::Model, lights: &crate::vehicle_db::Lights, linked: &[String]) -> Vec<Siren> {
    let mut by_hash: HashMap<u32, Vec<(usize, f32)>> = HashMap::new();
    let mut special = std::collections::HashSet::new();
    for (i, j) in model.joints.iter().enumerate() {
        let n = j.name.to_ascii_lowercase();
        let (base, k) = n.strip_suffix("_glass").map_or((n.as_str(), 1.0), |b| (b, 0.7));
        let h = sixng_core::meta7a::joaat(base);
        if ["siren", "strobe", "beacon"].iter().any(|w| base.contains(w)) {
            special.insert(h);
        }
        by_hash.entry(h).or_default().push((i, k));
    }
    let colour = |bone: u32| -> [f32; 3] {
        if let Some((c, _)) = lights.lamp_lights.get(&bone) {
            let m = c[0].max(c[1]).max(c[2]).max(1e-3);
            return [c[0] / m * 1.5, c[1] / m * 1.5, c[2] / m * 1.5];
        }
        let a = lights.archetypes.get(&bone).map(|s| s.as_str()).unwrap_or("");
        if a.contains("red") {
            [1.6, 0.04, 0.02]
        } else if a.contains("amber") {
            [1.5, 0.6, 0.03]
        } else if a.contains("warm") {
            [1.5, 1.3, 1.0]
        } else {
            [1.5, 1.5, 1.5]
        }
    };
    let mut out: Vec<(bool, Siren)> = Vec::new();
    for p in &lights.patterns {
        let is_linked = linked.contains(&p.name);
        if !is_linked && !lights.pack.contains(&p.name) {
            continue;
        }
        if !p.lamps.iter().any(|(b, s)| special.contains(b) && s.iter().any(|x| x.1 != 0)) {
            continue;
        }
        let mut lamps = Vec::new();
        for (b, steps) in &p.lamps {
            let c = colour(*b);
            for (ji, k) in by_hash.get(b).into_iter().flatten() {
                lamps.push((*ji, steps.clone(), [c[0] * k, c[1] * k, c[2] * k]));
            }
        }
        let n = p.lamps.iter().filter(|(b, s)| by_hash.contains_key(b) && s.iter().any(|x| x.1 != 0)).count();
        let flashing = p.lamps.iter().any(|(_, s)| s.iter().any(|x| x.1 > 1));
        let speed = if p.speed != 1.0 { format!(", speed {}", p.speed) } else { String::new() };
        let label = format!("{} {} (slot {}, {n} lamps{speed}{})", if flashing { "flashing" } else { "steady" }, p.name, p.slot, if is_linked { "" } else { ", pack" });
        out.push((is_linked, Siren { label, speed: if p.speed > 0.0 { p.speed } else { 1.0 }, lamps }));
    }
    // linked first, flashing before steady
    out.sort_by_key(|(l, s)| (!*l, s.label.starts_with("steady")));
    out.into_iter().map(|x| x.1).collect()
}

/// The colour of a lamp bone's light set (largest channel 1); a lens bone
/// (`<lamp>_glass`) takes its lamp's.
fn lamp_colour(lights: &crate::vehicle_db::Lights, joint: &str) -> Option<[f32; 3]> {
    let n = joint.to_ascii_lowercase();
    let (c, _) = lights.lamp_lights.get(&sixng_core::meta7a::joaat(n.strip_suffix("_glass").unwrap_or(&n)))?;
    let m = c[0].max(c[1]).max(c[2]);
    (m > 1e-3).then(|| [c[0] / m, c[1] / m, c[2] / m])
}

/// Brightness 0..1 of a flash type at `phase` (in cycles; the lamp's offset
/// already taken off). Only the type numbers are in the files: 0 is taken
/// as off, 1 as steady, 6 (the indicators' type) as an even blink, and
/// every other number as a double flash at the start of the cycle.
fn flash(ty: u32, phase: f64) -> f32 {
    let p = phase.rem_euclid(1.0);
    let on = match ty {
        0 => false,
        1 => true,
        6 => p < 0.5,
        _ => p < 0.1 || (0.18..0.28).contains(&p),
    };
    if on { 1.0 } else { 0.0 }
}

/// Which vehicle lamps are on.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Lamps {
    pub head: bool,
    pub high: bool,
    pub day: bool,
    pub tail: bool,
    pub brake: bool,
    pub reverse: bool,
    pub left: bool,
    pub right: bool,
    pub hazard: bool,
    pub interior: bool,
}

impl Lamps {
    /// The switch a lamp bone belongs to: 0 head, 1 high beam, 2 daytime,
    /// 3 tail (and number plate), 4 brake, 5 reversing, 6 left indicator,
    /// 7 right indicator, 8 interior. The lens bone `<lamp>_glass` goes
    /// with its lamp.
    pub fn kind(&self, bone: &str) -> Option<u8> {
        let n = bone.to_ascii_lowercase();
        let n = n.strip_suffix("_glass").unwrap_or(&n);
        Some(if n.starts_with("headlight_hi") {
            1
        } else if n.starts_with("headlight_") {
            0
        } else if n.starts_with("drlight") {
            2
        } else if n.starts_with("taillight") || n.starts_with("platelight") {
            3
        } else if n.starts_with("brakelight") {
            4
        } else if n.starts_with("reversinglight") {
            5
        } else if n.starts_with("indicator_l") {
            6
        } else if n.starts_with("indicator_r") {
            7
        } else if n.starts_with("interiorlight") {
            8
        } else {
            return None;
        })
    }

    /// Light of a lamp bone at time `t` (seconds): linear colour x
    /// intensity, None when it is off. Indicators blink at 1.5 Hz. The
    /// colours here are fixed per kind (white, red, amber); the caller swaps
    /// in the lamp's own colour where the vehicle data has one.
    pub fn light(&self, bone: &str, t: f64) -> Option<[f32; 3]> {
        let blink = (t * 1.5).fract() < 0.5;
        let (on, c) = match self.kind(bone)? {
            0 => (self.head, [1.0, 0.95, 0.85]),
            1 => (self.high, [1.0, 0.97, 0.92]),
            2 => (self.day, [0.95, 0.97, 1.0]),
            3 => (self.tail, [0.55, 0.02, 0.01]),
            4 => (self.brake, [1.2, 0.04, 0.02]),
            5 => (self.reverse, [1.0, 1.0, 1.0]),
            6 => ((self.left || self.hazard) && blink, [1.0, 0.42, 0.02]),
            7 => ((self.right || self.hazard) && blink, [1.0, 0.42, 0.02]),
            _ => (self.interior, [1.0, 0.9, 0.75]),
        };
        // the lens passes a little less than the reflector shows
        let k = if bone.to_ascii_lowercase().ends_with("_glass") { 0.7 } else { 1.0 };
        on.then(|| [c[0] * k, c[1] * k, c[2] * k])
    }

    /// From a list such as `head,tail,left` (the `--lights` flag).
    pub fn parse(s: &str) -> Self {
        let has = |k: &str| s.split(',').any(|x| x.trim().eq_ignore_ascii_case(k));
        Lamps { head: has("head"), high: has("high"), day: has("day"), tail: has("tail"), brake: has("brake"), reverse: has("reverse"), left: has("left"), right: has("right"), hazard: has("hazard"), interior: has("interior") }
    }
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

/// The mod parts of a vehicle for the Parts panel, and per option (keyed by
/// its `off`) the models it stands for: (entry offset, joaat of the attach
/// bone, joaat of the stock bones it replaces).
///
/// A part record is named after its model (`comet7_bonnet_01`) or, for a
/// left / right pair, after the models' names without the side word
/// (`comet7_exhaust_histr_19` for `…_lr_19` and `…_rr_19`). Models are taken
/// at the vehicle's own LOD suffix. Options in the same slot replace each
/// other.
#[allow(clippy::type_complexity)]
fn vehicle_part_options(ix: &ArchiveIndex, vehicle: &str, v: &crate::vehicle_db::Vehicle) -> (Vec<crate::assemble::PartOpt>, HashMap<u64, Vec<(u64, u32, Vec<u32>)>>) {
    let joaat = sixng_core::meta7a::joaat;
    let vl = vehicle.to_ascii_lowercase();
    let suffix = ["_hi", "_mid"].into_iter().find(|s| vl.ends_with(s)).unwrap_or("");
    // model stem -> the largest copy at this LOD level
    let mut stems: HashMap<String, &sixng_core::scan::Entry> = HashMap::new();
    for e in &ix.entries {
        let Some(n) = e.name.as_deref() else { continue };
        let n = n.to_ascii_lowercase();
        let stem = match suffix {
            "" if n.ends_with("_hi") || n.ends_with("_mid") => continue,
            "" => n,
            s => match n.strip_suffix(s) {
                Some(b) => b.to_string(),
                None => continue,
            },
        };
        match stems.get(&stem) {
            Some(x) if x.usize >= e.usize => {}
            _ => {
                stems.insert(stem, e);
            }
        }
    }
    // name hash -> models: a model's own name, and kit names without the side word
    let mut by_hash: HashMap<u32, Vec<(&String, &sixng_core::scan::Entry)>> = HashMap::new();
    for (stem, e) in &stems {
        by_hash.entry(joaat(stem)).or_default().push((stem, e));
        for side in ["_l_", "_r_", "_lr_", "_rr_", "_lf_", "_rf_"] {
            if stem.contains(side) {
                by_hash.entry(joaat(&stem.replacen(side, "_", 1))).or_default().push((stem, e));
            }
        }
    }
    const SLOTS: [&str; 24] = [
        "bonnet", "boot", "bumper_f", "bumper_r", "exhaust", "spoiler", "splitter", "diffuser", "towing", "roof", "chassis", "skirt", "skirts", "grille", "wing", "cage", "seats", "intake", "plate", "canards", "interior", "engine", "suspension", "lights",
    ];
    let slot_name: HashMap<u32, &str> = SLOTS.iter().map(|s| (joaat(s), *s)).collect();
    let mut opts = Vec::new();
    let mut kits = HashMap::new();
    for m in &v.mods {
        let Some(models) = by_hash.get(&m.name) else { continue };
        // a name that is both a model and a kit: the model itself
        let mut ms: Vec<&(&String, &sixng_core::scan::Entry)> = models.iter().filter(|x| joaat(x.0) == m.name).collect();
        if ms.is_empty() {
            ms = models.iter().collect();
        }
        ms.sort_by(|a, b| a.0.cmp(b.0));
        ms.dedup_by(|a, b| a.0 == b.0);
        if ms.is_empty() || m.elements.is_empty() {
            continue;
        }
        let first = ms[0].1;
        if kits.contains_key(&first.off) {
            continue;
        }
        // elements and models pair up in order (left before right in both)
        let list: Vec<(u64, u32, Vec<u32>)> = ms.iter().enumerate().map(|(k, x)| {
            let el = m.elements.get(k).or(m.elements.last()).unwrap();
            (x.1.off, el.0, el.1.clone())
        }).collect();
        let part = ms[0].0.strip_prefix(&format!("{}_", crate::vehicle_db::model_key(vehicle))).unwrap_or(ms[0].0).to_string();
        let label = if ms.len() > 1 { format!("{} (+{})", part.replace('_', " "), ms.len() - 1) } else { part.replace('_', " ") };
        opts.push(crate::assemble::PartOpt {
            off: first.off,
            id: first.id.clone(),
            name: ms[0].0.to_string(),
            group: slot_name.get(&m.slot).map(|s| s.to_string()).unwrap_or_else(|| format!("slot {:08x}", m.slot)),
            label,
            default: false,
        });
        kits.insert(first.off, list);
    }
    opts.sort_by(|a, b| a.group.cmp(&b.group).then(a.label.cmp(&b.label)));
    (opts, kits)
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
    export_load_skin(a, e, cache, lod, &[])
}

/// `export_load` with the face textures an outfit names for a head
/// (`ExportOptions::part_skins`).
fn export_load_skin(
    a: &sixng_core::game::ArchiveInfo,
    e: &sixng_core::scan::Entry,
    cache: &Path,
    lod: usize,
    skins: &[(u64, u64)],
) -> anyhow::Result<(glb::Model, Vec<PathBuf>, serde_json::Value)> {
    export_load_shaped(a, e, cache, lod, skins, (0.0, 0.0))
}

/// `export_load_skin` with a body shape (weight, muscle). Shaped exports go
/// to their own folder, so the cached neutral models stay neutral.
fn export_load_shaped(
    a: &sixng_core::game::ArchiveInfo,
    e: &sixng_core::scan::Entry,
    cache: &Path,
    lod: usize,
    skins: &[(u64, u64)],
    shape: (f32, f32),
) -> anyhow::Result<(glb::Model, Vec<PathBuf>, serde_json::Value)> {
    let shaped = shape != (0.0, 0.0);
    let shaped_dir = cache.join("shaped");
    let cache = if shaped { shaped_dir.as_path() } else { cache };
    let body_shape = if shaped { sixng_core::bodyshape::parse_spec(&format!("allweight={},allmuscle={}", shape.0, shape.1)).unwrap_or_default() } else { Vec::new() };
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
                // and from before the part colours were decoded (materials that
                // bind the vehicle material palette)
                || j.contains("\"19df25d7\"") && !j.contains(&format!("\"part_colours_version\": {}", sixng_core::vehcolour::EXPORT_VERSION)) && !j.contains(&format!("\"part_colours_version\":{}", sixng_core::vehcolour::EXPORT_VERSION))
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
        force: head || shaped || !skins.is_empty() || stale_tint || stale_vehicle || stale_animal,
        skip_embedded_textures: false,
        body_shape,
        head_skin: HEAD_SKIN.load(std::sync::atomic::Ordering::Relaxed),
        paint: None,
        tex_remap: Default::default(), ped_tints: Default::default(),
        part_skins: e.name.as_ref().filter(|_| !skins.is_empty()).map(|n| (n.to_ascii_lowercase(), skins.to_vec())).into_iter().collect(),
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
