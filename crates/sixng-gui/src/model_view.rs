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
pub const FINISHES: [&str; 5] = ["metallic", "matte", "chrome", "gloss", "as stored"];
/// `standard_vehicle_2lyr_wheel_inst`: the rim material of the shared wheels.
const RIM_SHADER: &str = "_c5cdbdd2";

/// Whether a material (named `..._<shader hash>`) has a tyre shader.
fn tyre_material(name: &str) -> bool {
    name.len() >= 8 && name.get(name.len() - 8..).is_some_and(sixng_core::material::is_tyre_shader)
}
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
            doors: names.iter().any(|n| matches!(n.as_str(), "door_dside_f" | "door_dside_r" | "door_pside_f" | "door_pside_r" | "door_dside_m" | "door_pside_m")),
            roof: names.iter().any(|n| n == "convertible_roof"),
            mounts: l.info["wheel_mounts"].as_array().is_some_and(|a| !a.is_empty()),
            joints: !m.joints.is_empty(),
            tint: m.meshes.iter().any(|x| x.verts.iter().any(|v| v.tint != [255, 255, 255, 255] && v.tint != [0, 0, 0, 0])),
            decals: m.materials.iter().any(|x| x.decal.is_some()),
        }
    }
}

impl Loaded {
    /// A character or an animal with a skeleton: a clip of anim.rpf can be played on it.
    pub fn takes_clips(&self) -> bool {
        !self.model.joints.is_empty() && (self.look.is_some() || self.fur.is_some())
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
    pending: Arc<Mutex<Option<(String, Result<(glb::Anim, Vec<sixng_core::anim::ClipTrack>, String), String>)>>>,
    last: Option<std::time::Instant>,
    /// The clip's own movement over the ground (`anim::clip_mover`).
    mover: Vec<sixng_core::anim::ClipTrack>,
    /// Play in place: leave that movement out.
    pub in_place: bool,
    /// Where the finished loops have taken the character, and where it
    /// is now (viewer space); `travel` is what the camera has followed.
    base: Mat4,
    root: Mat4,
    travel: Vec3,
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
    /// Per shoulder (left, right), on the collar bone: shrug (+ up),
    /// forward (+ forward).
    pub shoulder: [[f32; 2]; 2],
    /// Per arm: turn of the upper arm and of the forearm about their own
    /// length (+ rolls the thumb forward and down).
    pub arm_twist: [f32; 2],
    pub forearm_twist: [f32; 2],
    /// Per wrist: bend (+ up, the back of the hand towards the forearm),
    /// side (+ forward, towards the thumb).
    pub wrist: [[f32; 2]; 2],
    /// Per leg: turn of the thigh about its own length (+ toes outward).
    pub leg_twist: [f32; 2],
    /// Per ankle: + toes up.
    pub ankle: [f32; 2],
    /// Both eyeballs: look to the character's left (+), look up (+).
    pub eyes: [f32; 2],
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
            // a turn about x is a turn about the arm's own length; it is applied first
            v.push((format!("SKEL_{s}_Clavicle"), [0.0, -sg * self.shoulder[i][1], sg * self.shoulder[i][0]]));
            v.push((format!("SKEL_{s}_UpperArm"), [sg * self.arm_twist[i], -sg * self.arm[i][1], sg * self.arm[i][0]]));
            v.push((format!("SKEL_{s}_Forearm"), [sg * self.forearm_twist[i], -sg * self.elbow[i], 0.0]));
            v.push((format!("SKEL_{s}_Hand"), [0.0, -sg * self.wrist[i][1], sg * self.wrist[i][0]]));
            // the leg hangs along -y: a turn about y is a turn about its own length
            v.push((format!("SKEL_{s}_Thigh"), [-self.leg[i][0], sg * self.leg_twist[i], sg * self.leg[i][1]]));
            v.push((format!("SKEL_{s}_Calf"), [self.knee[i], 0.0, 0.0]));
            v.push((format!("SKEL_{s}_Foot"), [-self.ankle[i], 0.0, 0.0]));
            v.push((format!("FB_{s}_Eyeball"), [-self.eyes[1], self.eyes[0], 0.0]));
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

/// The skin of an animal as the fur sees it: every skinned vertex with its
/// joints and weights, and the joint list. Fur strands carry no bone
/// weights that are decoded, so each fur vertex takes those of the nearest
/// skin vertex; the fur then moves with the body under it.
pub struct FurRig {
    verts: Vec<([f32; 3], [u16; 4], [u8; 4])>,
    joints: Vec<(String, Option<usize>, [f32; 16], [f32; 16])>,
    grid: HashMap<(i32, i32, i32), Vec<u32>>,
}

impl FurRig {
    const CELL: f32 = 0.03;

    fn key(p: [f32; 3]) -> (i32, i32, i32) {
        ((p[0] / Self::CELL).floor() as i32, (p[1] / Self::CELL).floor() as i32, (p[2] / Self::CELL).floor() as i32)
    }

    /// From a loaded model; None when it has no skinned mesh.
    pub fn of(m: &glb::Model) -> Option<FurRig> {
        let verts: Vec<([f32; 3], [u16; 4], [u8; 4])> = m.meshes.iter().filter(|me| me.skinned).flat_map(|me| me.verts.iter().filter(|v| v.weights.iter().any(|w| *w > 0)).map(|v| (v.pos, v.joints, v.weights))).collect();
        if verts.is_empty() || m.joints.is_empty() {
            return None;
        }
        let mut grid: HashMap<(i32, i32, i32), Vec<u32>> = HashMap::new();
        for (i, v) in verts.iter().enumerate() {
            grid.entry(Self::key(v.0)).or_default().push(i as u32);
        }
        Some(FurRig { verts, joints: m.joints.iter().map(|j| (j.name.clone(), j.parent, j.local, j.ibm)).collect(), grid })
    }

    /// Joints and weights of the skin vertex nearest to `p` (within about 15 cm).
    fn nearest(&self, p: [f32; 3]) -> Option<([u16; 4], [u8; 4])> {
        let k = Self::key(p);
        let mut best: Option<(f32, u32)> = None;
        for r in 1..=5i32 {
            for x in -r..=r {
                for y in -r..=r {
                    for z in -r..=r {
                        // only the new shell of cells
                        if x.abs().max(y.abs()).max(z.abs()) != r && r > 1 {
                            continue;
                        }
                        for &i in self.grid.get(&(k.0 + x, k.1 + y, k.2 + z)).into_iter().flatten() {
                            let q = self.verts[i as usize].0;
                            let d = (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2);
                            if best.is_none_or(|b| d < b.0) {
                                best = Some((d, i));
                            }
                        }
                    }
                }
            }
            // a hit within the shells searched so far is the nearest
            if best.is_some_and(|b| b.0.sqrt() <= (r - 1).max(1) as f32 * Self::CELL) {
                break;
            }
        }
        best.map(|b| (self.verts[b.1 as usize].1, self.verts[b.1 as usize].2))
    }

    /// Bind a groom's display mesh to the skin: joints and weights per
    /// vertex, and the animal's joint list on the model.
    pub fn bind(&self, gm: &mut glb::Model) {
        for me in &mut gm.meshes {
            for v in &mut me.verts {
                if let Some((j, w)) = self.nearest(v.pos) {
                    v.joints = j;
                    v.weights = w;
                } else {
                    v.joints = [0; 4];
                    v.weights = [255, 0, 0, 0];
                }
            }
            me.skinned = true;
        }
        gm.joints = self.joints.iter().map(|j| glb::Joint { name: j.0.clone(), parent: j.1, local: j.2, ibm: j.3 }).collect();
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
    /// Hair colour preset (name hash) of the strand parts, by slot.
    pub strand_presets: Vec<(String, u32)>,
    /// What the viewer did or could not do (skin tone source, unresolved items).
    pub notes: Vec<String>,
    /// Strand hair found for the outfit's hair assets (`hairlink`).
    pub hair: Vec<HairPick>,
    /// Drawables of the outfit's hats (lower case).
    pub hats: Vec<String>,
    /// The hat of the outfit shown, to keep the hair under it.
    pub hat: Option<Arc<sixng_core::groom::Shell>>,
    /// The colours of the game's hair palette (`ped_tint_hair`, each row at
    /// mid shade, sRGB), to pick from.
    pub hair_swatches: Vec<[u8; 3]>,
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
    /// names that palette; else the first colour of its hair preset, as
    /// shown (`hairtone::display`).
    pub colour: Option<[u8; 3]>,
    /// The item's hair colour preset, when it names one.
    pub tone: Option<HairTone>,
}

/// A hair colour preset as the viewer draws it: its two colours, mixed per
/// strand by the preset's noise texture (`hairtone`).
#[derive(Clone)]
pub struct HairTone {
    pub name: u32,
    pub preset: sixng_core::hairtone::Preset,
    pub noise: Option<Arc<sixng_core::hairtone::Noise>>,
}

impl HairTone {
    /// Colour of each guide strand of a groom, linear RGB bytes.
    fn strands(&self, g: &sixng_core::groom::Groom) -> Vec<[u8; 3]> {
        let tiling = self.preset.values.get(sixng_core::hairtone::TILING).copied().filter(|t| *t > 0.0).unwrap_or(1.0);
        (0..g.strands)
            .map(|s| {
                let n = match (&self.noise, g.root_uv.get(s)) {
                    (Some(n), Some(uv)) => n.at([uv[0] * tiling, uv[1] * tiling]),
                    _ => sixng_core::hairtone::scatter(s),
                };
                sixng_core::hairtone::strand(&self.preset, n).map(|c| ((c as f32 / 255.0).powf(2.2) * 255.0 + 0.5) as u8)
            })
            .collect()
    }
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
    /// Base colour (sRGB) per sub-material from the item's garment preset.
    pub subs: Vec<Option<[u8; 3]>>,
    /// The item's garment preset: its fabric (tiles, tint-mask layers).
    pub preset: Option<u32>,
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
    /// The layer the item's vertex regions belong to (upper body skin
    /// `b4d9d470`, lower body skin `e0622b84`, …); 0 when not known.
    pub region_layer: u32,
    /// The item's hide masks: layer -> 256-bit region set (hex).
    pub hides: Vec<(u32, String)>,
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
    /// `--export-hair`: export the shown grooms once they are loaded
    /// (tests the Hair panel's export button).
    pub export_hair: bool,
    /// `--hair-style TEXT`: also show the character's hair entries whose
    /// placeholder name contains this.
    pub hair_style: Option<String>,
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
    /// A reload asked for by a configuration: its parts with models.
    parts_request: Option<String>,
    /// The paint finish follows the model: a vehicle's own where its export
    /// has one, metallic otherwise (until the user picks one).
    pub finish_auto: bool,
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
    /// `--random-parts <seed>`: pick random parts once after the load.
    pub random_parts_once: bool,
    pub random_parts_seed: u64,
    /// The material palette of the loaded vehicle (its file, RGBA rows of
    /// 128) and the look whose rows the model's part materials carry.
    part_palette: Option<(PathBuf, Arc<Vec<u8>>)>,
    part_ramp_done: Option<(PathBuf, usize)>,
    /// Stickers the user placed on the loaded vehicle (custom liveries).
    pub livery: crate::livery::State,
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
    /// bonnet, 2 = boot; rotation axis; sign that swings it open). A sliding
    /// door is 3, with its whole travel in the bone's own space in place of
    /// the axis.
    hinges: HashMap<usize, (u8, Vec3, f32)>,
    /// Doors, side windows, bonnet and boot left shut whatever their slider
    /// says (the tick boxes under the sliders), by bone name in lower case.
    /// Kept from one vehicle to the next.
    pub panels_off: std::collections::HashSet<String>,
    /// What the configuration's parts without a model change on the wheels:
    /// [out from the centre line, tyre width], each per axle (front, rear).
    wheel_fit_cfg: [[f32; 2]; 2],
    /// `--fit-parts <hex,hex>`: part options (archive offsets) to fit once
    /// after the load, in place of the configuration's.
    pub fit_parts_once: Option<String>,
    /// Struts and rods between a hinged panel and the body: (bone, the
    /// bone it aims at).
    aims: Vec<(usize, usize)>,
    /// Brake disc and caliper models at the wheels.
    /// (turns with the wheel, on the front / rear axle, model)
    brakes: Vec<(bool, [bool; 2], GpuModel)>,
    brake_pending: Arc<Mutex<Option<Vec<(bool, [bool; 2], Loaded)>>>>,
    brake_key: String,
    brake_loading: bool,
    veh_brake: sixng_core::vehexport::AxleBrakes,
    /// The rim light ring per axle (`lights_rims` parts), by model name.
    veh_rim_light: [Option<String>; 2],
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
    /// Animals: the clip dictionaries of the species shown (root name, the
    /// names), from the cached dictionary list.
    animal_clips: Option<(String, Vec<String>)>,
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
    /// Height of the lowest point of the fitted wheels (None without wheels).
    wheel_floor: Option<f32>,
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
    /// Set when the user asks for a character's cutscenes (outfit prefix).
    pub open_scenes: Option<String>,
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
    /// The hat worn and the grooms it presses down (the scalp hair).
    hat: Option<Arc<sixng_core::groom::Shell>>,
    under_hat: Vec<u64>,
    /// Animals: the skinned vertices and joints of the model shown, so the
    /// fur can be bound to the skin under it and move with a clip.
    rig: Option<Arc<FurRig>>,
    /// Export of the shown grooms: write the full strand set, a run in
    /// progress, its result and where the last one went.
    pub export_full: bool,
    exporting: bool,
    export_done: Arc<Mutex<Option<String>>>,
    export_status: String,
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
    /// Characters: the hair colour preset of each groom shown.
    pub tones: HashMap<u64, HairTone>,
    /// Colour the grooms that have a preset with it (else the one colour).
    pub stored_colours: bool,
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
            hat: None,
            under_hat: Vec::new(),
            rig: None,
            export_full: true,
            exporting: false,
            export_done: Arc::new(Mutex::new(None)),
            export_status: String::new(),
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
            tones: HashMap::new(),
            stored_colours: true,
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
        let mut tones: Vec<(u64, u32)> = self.tones.iter().filter(|_| self.stored_colours).map(|(k, t)| (*k, t.name)).collect();
        tones.sort();
        format!("{tones:?}|{:?}|{}|{:?}|{}|{}|{}|{:?}|{:?}", self.selected, self.width_mm, self.colour, self.skin_colour && !self.skin.is_empty(), self.full_coat, self.density, self.hat.as_ref().map(|h| h.id), self.rig.as_ref().map(|r| (r.verts.len(), r.joints.len())))
    }
    pub fn settled(&self) -> bool {
        !self.loading && !self.listing && !self.exporting && self.key == self.want_key()
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
            parts_request: None,
            finish_auto: true,
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
            random_parts_once: false,
            random_parts_seed: 0,
            part_palette: None,
            part_ramp_done: None,
            livery: Default::default(),
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
            panels_off: Default::default(),
            wheel_fit_cfg: [[0.0; 2]; 2],
            fit_parts_once: None,
            aims: Vec::new(),
            brakes: Vec::new(),
            brake_pending: Arc::new(Mutex::new(None)),
            brake_key: String::new(),
            brake_loading: false,
            veh_brake: Default::default(),
            veh_rim_light: Default::default(),
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
            animal_clips: None,
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
            wheel_floor: None,
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
            open_scenes: None,
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
        if let Some(r) = self.hair.export_done.lock().unwrap().take() {
            self.hair.exporting = false;
            eprintln!("[hair] {r}");
            self.hair.export_status = r;
        }
        // `--export-hair`: once the hair of the model is shown
        if self.auto.export_hair && !self.hair.selected.is_empty() && !self.hair.loading && !self.hair.exporting && self.hair.key == self.hair.want_key() {
            self.auto.export_hair = false;
            self.start_hair_export(ctx);
        }
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
        let (hat, under_hat) = (self.hair.hat.clone(), self.hair.under_hat.clone());
        let rig = self.hair.rig.clone();
        let tones = if self.hair.stored_colours { self.hair.tones.clone() } else { HashMap::new() };
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
                for (off, mut g) in grooms {
                    // scalp hair stays under the hat (3 mm inside its lining)
                    if let Some(h) = hat.as_ref().filter(|_| under_hat.contains(&off)) {
                        h.press(&mut g.points, 0.003);
                    }
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
                        let cols: Vec<[u8; 3]> = match tones.get(&off) {
                            // a character's hair: the two colours of its preset
                            Some(t) => t.strands(&g),
                            None => (0..g.strands).map(|s| if skin.is_empty() { one } else { nearest(g.points[s * k], &skin) }).collect(),
                        };
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
                    } else if let Some(t) = tones.get(&off) {
                        g.ribbon_glb_coloured(&format!("groom_{off:x}"), hw, &t.strands(&g), serde_json::json!({}))
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
                    if let Some(r) = &rig {
                        r.bind(&mut gm);
                    }
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
    /// Write the shown grooms to `<work>/out/hair/<model>/grooms/packs` on a worker
    /// thread (the "Export shown grooms" button, `--export-hair`).
    fn start_hair_export(&mut self, ctx: &egui::Context) {
        self.hair.exporting = true;
        self.hair.export_status.clear();
        let ws = Workspace::new(&self.game, &self.work);
        let (idx, sel, slot, ctx) = (self.indexes.clone(), self.hair.selected.clone(), self.hair.export_done.clone(), ctx.clone());
        let full = self.hair.export_full.then_some(self.hair.density.clamp(0.05, 1.0));
        // a folder per model, so an earlier `sixng grooms` export and its index stay as they are
        let model: String = self.loaded.as_ref().map_or("grooms", |l| l.source.split_whitespace().next().unwrap_or("").rsplit([':', '#', '/']).find(|s| !s.is_empty() && !s.starts_with("look=")).unwrap_or("grooms")).chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
        let out = Path::new(&self.work).join("out").join("hair").join(model);
        std::thread::spawn(move || {
            let r = (|| -> anyhow::Result<String> {
                let (a, ix) = index_for(&ws, "packs", &idx)?;
                let (written, errors) = sixng_core::groom::export_grooms(&a, &ix.entries, &sel, &out, false, None, full, sixng_core::export::no_progress())?;
                let dir = out.join("grooms").join("packs");
                Ok(format!("{written} groom(s) written to {}{}{}", dir.display(), if full.is_some() { " with the full strand set and a growth.json each" } else { " (guide strands)" }, if errors > 0 { format!("; {errors} failed") } else { String::new() }))
            })()
            .unwrap_or_else(|e| format!("export failed: {e:#}"));
            *slot.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    fn hair_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Hair and fur (grooms)").default_open(!self.hair.selected.is_empty() || self.auto.find_hair || self.auto.export_hair).show(ui, |ui| {
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
            // export of the grooms shown (a character's hair, an animal's fur, or the ones ticked below)
            if !self.hair.selected.is_empty() {
                ui.horizontal(|ui| {
                    let n = self.hair.selected.len();
                    if ui.add_enabled(!self.hair.exporting, egui::Button::new(format!("Export {n} shown groom(s)"))).on_hover_text("Writes each shown groom as a GLB of line strips (strand width as _WIDTH), in model space so it sits on the exported model, to <work>/out/hair/<model>/grooms/packs/<offset>.glb. Same as `sixng grooms --only ...`.").clicked() {
                        let ctx = ui.ctx().clone();
                        self.start_hair_export(&ctx);
                    }
                    ui.checkbox(&mut self.hair.export_full, "full strands").on_hover_text("Write the strands the groom stores per root triangle, grown between its guides, instead of the guide strands alone (same as `sixng grooms --full`; the density slider below sets the share). The game's own clumping, curl and noise are not decoded, so the result is smoother than in game.");
                    if self.hair.exporting {
                        ui.spinner();
                        ui.label("exporting (the first run reads the groom list, about a minute) …");
                    }
                });
                if !self.hair.export_status.is_empty() {
                    ui.label(egui::RichText::new(&self.hair.export_status).weak().small());
                }
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
                if !self.hair.tones.is_empty() {
                    ui.checkbox(&mut self.hair.stored_colours, "stored hair colours").on_hover_text("Each hair, brow, lash and beard piece takes the two colours of the hair preset its item names, mixed per strand by the preset's noise texture (how the game mixes them is not decoded). Shown brighter than stored: the game's hair shader scatters light and the viewer's does not. Off: the one colour above.");
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
        self.idle_for_model() && !self.livery.ed.busy()
    }

    /// The loaded model is complete (a vehicle has its data, parts and wheels).
    fn idle_for_model(&self) -> bool {
        if self.parts_request.is_some() || self.fit_parts_once.is_some() || self.random_parts_once || !self.hair.settled() || self.auto.then.is_some() || self.auto.find_hair || self.auto.export_hair || self.hair.fitting || self.ped_clip.loading || (!self.ped_clip.want.trim().is_empty() && self.ped_clip.anim.is_none() && self.ped_clip.status.starts_with("loading")) {
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
                Ok((a, mover, note)) => {
                    self.ped_clip.status = note;
                    self.ped_clip.anim = Some(a);
                    self.ped_clip.mover = mover;
                    self.ped_clip.base = Mat4::IDENTITY;
                }
                Err(e) => {
                    self.ped_clip.status = e;
                    self.ped_clip.anim = None;
                }
            }
        }
        let Some(l) = self.loaded.as_ref().filter(|l| l.takes_clips()) else { return };
        let want = self.ped_clip.want.trim().to_string();
        let key = format!("{want}#{}#{}", l.model.joints.len(), l.model.joints[0].name);
        if want.is_empty() {
            self.ped_clip.anim = None;
            self.ped_clip.loaded_key.clear();
        } else if !self.ped_clip.loading && self.ped_clip.loaded_key != key {
            self.ped_clip.loading = true;
            self.ped_clip.status = format!("loading {want} …");
            let names: Vec<String> = l.model.joints.iter().map(|j| j.name.clone()).collect();
            let model = l.base.split_once("#outfit:").map(|x| x.1.split('#').next().unwrap_or("").to_string()).unwrap_or_default();
            let (game, work, slot, ctx2) = (PathBuf::from(&self.game), PathBuf::from(&self.work), self.ped_clip.pending.clone(), ctx.clone());
            std::thread::spawn(move || {
                let r = (|| -> anyhow::Result<(glb::Anim, Vec<sixng_core::anim::ClipTrack>, String)> {
                    // "<dictionary>/<clip>", or a dictionary alone: then its `move` or `idle` clip, else the first that binds an animation
                    let (dq, cq) = want.rsplit_once('/').unwrap_or((want.as_str(), ""));
                    // a cutscene (`cin@<scene>[/<clip>]`): its sections joined; no clip = this character's own
                    let (d, cq) = if crate::scenes_view::is_scene(dq) {
                        crate::scenes_view::scene_clip(&game, &work, dq, cq, &model)?
                    } else {
                        (sixng_core::anim::load_dictionary(&game, &work, dq)?.1, cq.to_string())
                    };
                    let cq = cq.as_str();
                    let c = if cq.is_empty() {
                        let bound: Vec<&sixng_core::anim::Clip> = d.clips.iter().filter(|c| c.kind == 1 && !c.bindings.is_empty()).collect();
                        ["move", "idle"].iter().find_map(|n| bound.iter().find(|c| c.short_name() == *n).copied()).or(bound.first().copied()).ok_or_else(|| anyhow::anyhow!("{} holds no clip with animation data that is decoded (its {} clip(s) bind none here)", d.name, d.clips.len()))?
                    } else {
                        d.clip(cq).ok_or_else(|| anyhow::anyhow!("no clip {cq:?} in {}; it has: {}", d.name, d.clips.iter().map(|c| c.short_name()).collect::<Vec<_>>().join(", ")))?
                    };
                    if c.bindings.is_empty() {
                        anyhow::bail!("clip {} of {} binds no animation in this dictionary (not decoded yet); pick another clip", c.short_name(), d.name);
                    }
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
                    let mover = sixng_core::anim::clip_mover(&d, c);
                    let (moved, turned) = sixng_core::anim::mover_at(&mover, c.duration);
                    let ground = if mover.is_empty() {
                        "The clip stores no movement over the ground, so it plays in place.".to_string()
                    } else {
                        format!("Over the ground the clip moves {:.2} m and turns {:.0} degrees (its mover tracks); each loop carries on from where the last one ended, and the camera follows.", (moved[0] * moved[0] + moved[1] * moved[1] + moved[2] * moved[2]).sqrt(), (2.0 * turned[2].atan2(turned[3]).to_degrees()).abs())
                    };
                    let note = format!("{} bone tracks of {} on this skeleton, {:.2} s. {ground}", stats["bone_tracks_on_joints"], stats["bone_tracks"], duration);
                    Ok((glb::Anim { name: want.clone(), duration, channels }, mover, note))
                })()
                .map_err(|e| format!("{e:#}"));
                *slot.lock().unwrap() = Some((key, r));
                ctx2.request_repaint();
            });
        }
        // playback
        let now = std::time::Instant::now();
        let dt = self.ped_clip.last.replace(now).map_or(0.0, |t| (now - t).as_secs_f32()).min(0.1);
        // the mover in viewer space (Y-up) at a clip time
        let at = |m: &[sixng_core::anim::ClipTrack], t: f32| {
            let (p, q) = sixng_core::anim::mover_at(m, t);
            Mat4::from_rotation_translation(Quat::from_xyzw(q[0], q[2], -q[1], q[3]).normalize(), Vec3::new(p[0], p[2], -p[1]))
        };
        if let (true, Some(a)) = (self.ped_clip.playing, &self.ped_clip.anim) {
            let next = self.ped_clip.pos + dt / a.duration;
            if next >= 1.0 {
                // a loop ended: the next one starts where this one stopped
                self.ped_clip.base *= at(&self.ped_clip.mover, a.duration);
            }
            self.ped_clip.pos = next.fract();
            ctx.request_repaint();
        }
        let root = match &self.ped_clip.anim {
            Some(a) if !self.ped_clip.in_place && !self.ped_clip.mover.is_empty() => self.ped_clip.base * at(&self.ped_clip.mover, self.ped_clip.pos * a.duration),
            _ => {
                self.ped_clip.base = Mat4::IDENTITY;
                Mat4::IDENTITY
            }
        };
        // the camera goes with the character
        let moved = root.w_axis.truncate();
        self.cam.target += moved - self.ped_clip.travel;
        self.cam.eye += moved - self.ped_clip.travel;
        self.ped_clip.travel = moved;
        self.ped_clip.root = root;
        if std::env::var_os("SIXNG_DEBUG_RIG").is_some() && self.ped_clip.anim.is_some() {
            eprintln!("[clip] pos {:.2} moved {moved:?} cam target {:?} fly {}", self.ped_clip.pos, self.cam.target, self.cam.fly);
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
    /// "Animation" for an animal: the clip dictionaries of its species to
    /// pick from, and the same playback controls as a character has.
    fn animal_clip_ui(&mut self, ui: &mut egui::Ui) {
        let Some(l) = self.loaded.as_ref().filter(|l| l.look.is_none() && l.takes_clips()) else { return };
        let root = l.source.split_whitespace().next().unwrap_or("").rsplit([':', '#', '/']).find(|s| !s.is_empty()).unwrap_or("").to_ascii_lowercase();
        if self.animal_clips.as_ref().is_none_or(|c| c.0 != root) {
            // the species word of `crm_cat_feral_m_00` is `cat`; some dictionaries use two words (`bengal_tiger`)
            let t: Vec<&str> = root.split('_').collect();
            let words: Vec<String> = match t.as_slice() {
                [_, a, b, ..] => vec![format!("{b}_{a}"), format!("{a}_{b}"), a.to_string()],
                [_, a] => vec![a.to_string()],
                _ => vec![],
            };
            // only the cached list: building it reads every clip dictionary (about a minute)
            let cached = Path::new(&self.work).join("index").join(sixng_core::anim::DICT_CACHE).exists();
            let all = if cached { sixng_core::anim::dictionaries(Path::new(&self.game), Path::new(&self.work)).unwrap_or_default() } else { Vec::new() };
            let names = words
                .iter()
                .map(|w| all.iter().filter(|d| d.name.starts_with("gameplay@creatures@") && d.name.split('@').any(|p| p == w)).map(|d| d.name.clone()).collect::<Vec<_>>())
                .find(|v| !v.is_empty())
                .unwrap_or_default();
            self.animal_clips = Some((root.clone(), names));
        }
        let names = self.animal_clips.as_ref().map(|c| c.1.clone()).unwrap_or_default();
        egui::CollapsingHeader::new("Animation").default_open(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.ped_clip.want).hint_text("<dictionary>[/<clip>]").desired_width(250.0));
                if ui.button("None").clicked() {
                    self.ped_clip.want.clear();
                    self.ped_clip.playing = false;
                }
            });
            if names.is_empty() {
                ui.label(egui::RichText::new("No clip dictionaries listed for this animal. The list comes from `sixng clips` (run it once; it reads anim.rpf for about a minute), by the species word of the animal's name.").weak().small());
            } else {
                // the part of the name after the species: `normal@walk@slow`
                let tail = |n: &str| n.split('@').skip(5).collect::<Vec<_>>().join("@");
                let current = self.ped_clip.want.split('/').next().unwrap_or("").to_string();
                egui::ComboBox::from_id_salt("animal_clip_dict").selected_text(if names.contains(&current) { tail(&current) } else { format!("{} clip dictionaries", names.len()) }).width(260.0).show_ui(ui, |ui| {
                    for n in &names {
                        if ui.selectable_label(*n == current, tail(n)).on_hover_text(n).clicked() {
                            self.ped_clip.want = n.clone();
                            self.ped_clip.playing = true;
                        }
                    }
                });
            }
            if self.ped_clip.anim.is_some() {
                ui.horizontal(|ui| {
                    if ui.button(if self.ped_clip.playing { "⏸" } else { "▶" }).clicked() {
                        self.ped_clip.playing = !self.ped_clip.playing;
                    }
                    ui.add(egui::Slider::new(&mut self.ped_clip.pos, 0.0..=1.0).show_value(false));
                    ui.checkbox(&mut self.ped_clip.in_place, "in place").on_hover_text("Leave out the clip's own movement over the ground (its mover tracks), so the animal stays where it is.");
                });
            }
            if !self.ped_clip.status.is_empty() {
                ui.label(egui::RichText::new(&self.ped_clip.status).weak().small());
            }
            ui.label(egui::RichText::new("A clip of anim.rpf on this animal's skeleton. A dictionary alone plays its move or idle clip; some dictionaries hold clips whose animation data is stored elsewhere and is not decoded. The fur is bound to the nearest skin and moves with the body.").weak().small());
        });
    }

    fn look_ui(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let l = self.loaded.as_ref()?;
        let look = l.look.as_ref()?;
        let mut reload = None;
        ui.horizontal(|ui| {
            ui.heading("Character");
            if let Some((_, p)) = l.base.split_once("#outfit:") {
                if ui.button("🎬 Open in scenes").on_hover_text("The Scenes tab, filtered to every cutscene with this character in the cast").clicked() {
                    self.open_scenes = Some(p.split('#').next().unwrap_or(p).to_string());
                }
            }
        });
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
                let [sl, sr] = &mut p.shoulder;
                pair(ui, "shoulder shrug", &mut sl[0], &mut sr[0], -15.0, 35.0);
                pair(ui, "shoulder forward", &mut sl[1], &mut sr[1], -25.0, 35.0);
                let [al, ar] = &mut p.arm;
                pair(ui, "arm raise", &mut al[0], &mut ar[0], -90.0, 90.0);
                pair(ui, "arm swing", &mut al[1], &mut ar[1], -90.0, 120.0);
                let [tl, tr] = &mut p.arm_twist;
                pair(ui, "arm twist", tl, tr, -90.0, 90.0);
                let [el, er] = &mut p.elbow;
                pair(ui, "elbow", el, er, 0.0, 140.0);
                let [fl, fr] = &mut p.forearm_twist;
                pair(ui, "forearm twist", fl, fr, -90.0, 90.0);
                let [wl, wr] = &mut p.wrist;
                pair(ui, "wrist bend", &mut wl[0], &mut wr[0], -80.0, 70.0);
                pair(ui, "wrist side", &mut wl[1], &mut wr[1], -30.0, 20.0);
                let [ll, lr] = &mut p.leg;
                pair(ui, "leg lift", &mut ll[0], &mut lr[0], -45.0, 110.0);
                pair(ui, "leg spread", &mut ll[1], &mut lr[1], -20.0, 60.0);
                let [gl, gr] = &mut p.leg_twist;
                pair(ui, "leg twist", gl, gr, -45.0, 45.0);
                let [kl, kr] = &mut p.knee;
                pair(ui, "knee", kl, kr, 0.0, 140.0);
                let [nl, nr] = &mut p.ankle;
                pair(ui, "ankle", nl, nr, -45.0, 30.0);
            });
            if ui.button("Reset pose").on_hover_text("Every angle back to zero: the stored pose.").clicked() {
                *p = PedPose::default();
            }
            ui.horizontal(|ui| {
                ui.label("spine");
                ui.add(egui::DragValue::new(&mut p.spine[0]).range(-40.0..=80.0).suffix("°").prefix("bend "));
                ui.add(egui::DragValue::new(&mut p.spine[1]).range(-60.0..=60.0).suffix("°").prefix("twist "));
                ui.add(egui::DragValue::new(&mut p.spine[2]).range(-40.0..=40.0).suffix("°").prefix("lean "));
            });
            ui.horizontal(|ui| {
                ui.label("eyes");
                ui.add(egui::DragValue::new(&mut p.eyes[0]).range(-35.0..=35.0).suffix("°").prefix("left "));
                ui.add(egui::DragValue::new(&mut p.eyes[1]).range(-25.0..=25.0).suffix("°").prefix("up "));
            });
            ui.horizontal(|ui| {
                ui.label("head");
                ui.add(egui::DragValue::new(&mut p.head[0]).range(-50.0..=60.0).suffix("°").prefix("nod "));
                ui.add(egui::DragValue::new(&mut p.head[1]).range(-80.0..=80.0).suffix("°").prefix("turn "));
                ui.add(egui::DragValue::new(&mut p.head[2]).range(-40.0..=40.0).suffix("°").prefix("tilt "));
            });
            ui.label(egui::RichText::new("Angles on the main bones of the shared skeleton (collar bone, upper arm, forearm, hand, thigh, calf, foot, spine, neck and head); zero is the stored pose. The limits are a person's rough range, not from the game. Drag a value or type it.").weak().small());
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
                    ui.checkbox(&mut self.ped_clip.in_place, "in place").on_hover_text("Leave out the clip's own movement over the ground (its mover tracks), so the character stays where it is.");
                });
            }
            if !self.ped_clip.status.is_empty() {
                ui.label(egui::RichText::new(&self.ped_clip.status).weak().small());
            }
            ui.label(egui::RichText::new("A clip of anim.rpf on this skeleton (`sixng clips --name <regex>` lists them), or a cutscene: cin@<scene> plays this character's clip of that scene. The Pose angles are added on top.").weak().small());
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
            if !look.hair_swatches.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                    for c in look.hair_swatches.iter().take(64) {
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
                        ui.painter().rect_filled(r, 2.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
                        if resp.on_hover_text("A colour of the game's hair palette (ped_tint_hair)").clicked() {
                            self.hair.colour = c.map(|x| x as f32 / 255.0);
                        }
                    }
                });
            }
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
            egui::RichText::new("One choice per group. Accessories and alternate sets start off; a vehicle starts with the parts of its first configuration.")
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
        // weapon finishes: the variant presets of the weapon's finish mods, per part type
        let mut finish: Vec<(String, Option<String>)> = Vec::new();
        let mut finish_changed = false;
        if let Some(fs) = l.info["weapon_finishes"].as_array().filter(|a| !a.is_empty()) {
            ui.label(egui::RichText::new("Finishes (the colour variants a weapon's finish mods offer for this part; their names are not recovered, so each is labelled by its colours)").weak());
            let squares = |ui: &mut egui::Ui, o: &serde_json::Value| {
                for c in o["colours"].as_array().into_iter().flatten().filter_map(|c| u32::from_str_radix(c.as_str()?, 16).ok()) {
                    ui.label(egui::RichText::new("\u{25a0}").color(egui::Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)));
                }
            };
            egui::Grid::new("finish_grid").num_columns(2).striped(true).show(ui, |ui| {
                for f in fs {
                    let slot = f["slot"].as_str().unwrap_or("").to_string();
                    let mut cur = f["selected"].as_str().map(str::to_string);
                    ui.label(f["group"].as_str().filter(|g| !g.is_empty()).unwrap_or(&slot));
                    let name_of = |o: &serde_json::Value| format!("{} ({})", o["label"].as_str().filter(|l| !l.is_empty()).unwrap_or("finish"), o["variant"].as_str().unwrap_or(""));
                    let text = match &cur {
                        Some(p) => f["options"].as_array().into_iter().flatten().find(|o| o["preset"] == p.as_str()).map(name_of).unwrap_or_else(|| p.clone()),
                        None => "default".to_string(),
                    };
                    egui::ComboBox::from_id_salt(("finish", slot.as_str())).selected_text(text).width(240.0).show_ui(ui, |ui| {
                        if ui.selectable_label(cur.is_none(), "default").clicked() && cur.is_some() {
                            cur = None;
                            finish_changed = true;
                        }
                        for o in f["options"].as_array().into_iter().flatten() {
                            let pr = o["preset"].as_str().unwrap_or("");
                            let on = cur.as_deref() == Some(pr);
                            ui.horizontal(|ui| {
                                squares(ui, o);
                                if ui.selectable_label(on, name_of(o)).on_hover_text(format!("preset {pr}")).clicked() && !on {
                                    cur = Some(pr.to_string());
                                    finish_changed = true;
                                }
                            });
                        }
                    });
                    ui.end_row();
                    finish.push((slot, cur));
                }
            });
        }
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
            // vehicles: a random choice in every group
            if l.info["wheel_mounts"].as_array().is_some_and(|a| !a.is_empty())
                && ui
                    .button("🎲 Randomize")
                    .on_hover_text("A random choice for every group of parts (bonnet, bumpers, exhaust, spoiler, body ...), from the options this vehicle has. A group that starts with a part always gets one; a group that starts empty may stay empty.")
                    .clicked()
            {
                let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0x9e37_79b9_7f4a_7c15, |d| d.as_nanos() as u64) | 1;
                let mut next = |n: usize| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    (seed >> 11) as usize % n.max(1)
                };
                sel = random_parts(&groups, &mut next);
                changed = true;
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
        (changed || finish_changed).then(|| {
            // a changed finish replaces the query's `#finish=` list
            let mut base = l.base.clone();
            if finish_changed {
                base = base.split("#finish=").next().unwrap_or(&base).to_string();
                let f: Vec<String> = finish.iter().filter_map(|(s, p)| Some(format!("{s}:{}", p.as_ref()?))).collect();
                if !f.is_empty() {
                    base = format!("{base}#finish={}", f.join(","));
                }
            }
            format!(
                "{}#parts={}",
                base,
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
                    // framed on the stored pose: the walked distance is added again
                    self.ped_clip.travel = Vec3::ZERO;
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
                self.hair.rig = (l.look.is_none() && l.fur.is_some()).then(|| FurRig::of(&l.model)).flatten().map(Arc::new);
                if std::env::var_os("SIXNG_DEBUG_RIG").is_some() {
                    eprintln!("[rig] loaded: {} meshes, {} skinned, {} joints, fur rig {}, max joint index {:?}", l.model.meshes.len(), l.model.meshes.iter().filter(|m| m.skinned).count(), l.model.joints.len(), self.hair.rig.is_some(), l.model.meshes.iter().flat_map(|m| m.verts.iter().flat_map(|v| v.joints)).max());
                }
                if let Some(look) = &l.look {
                    self.hair.hat = look.hat.clone();
                    self.hair.under_hat = look.hair.iter().filter(|h| h.slot == "hair").flat_map(|h| h.grooms.iter().copied()).collect();
                    // a character: its confident hair, grown to full density
                    let key = format!("{}#{}", look.model, look.current);
                    if self.hair.look_key != key {
                        self.hair.look_key = key;
                        let want = self.auto.hair_style.clone();
                        self.hair.selected = look.hair.iter().filter(|h| h.shown || want.as_ref().is_some_and(|w| h.placeholder.contains(w.as_str()))).flat_map(|h| h.grooms.iter().copied()).collect();
                        if let Some(c) = look.hair.iter().find(|h| h.slot == "hair").or(look.hair.first()).and_then(|h| h.colour) {
                            self.hair.colour = c.map(|x| x as f32 / 255.0);
                        }
                        self.hair.tones = look.hair.iter().filter_map(|h| Some((h, h.tone.as_ref()?))).flat_map(|(h, t)| h.grooms.iter().map(move |g| (*g, t.clone()))).collect();
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
                    self.hair.hat = None;
                    if self.hair.fur.is_none() {
                        self.hair.selected.clear();
                    }
                }
                self.loaded = Some(l);
                self.frames_since_load = 0;
                // the new meshes carry the colours of their files
                self.part_look_done = None;
                self.part_ramp_done = None;
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
        // the body paint's own finish, from the surfaces of its painted parts
        let stored = &l.info["part_colours"]["paint_finish"];
        let stored = stored["metalness"].as_f64().zip(stored["roughness"].as_f64()).map(|(m, r)| [m as f32, r as f32]);
        if let Some(f) = stored {
            self.look.stored_finish = f;
        }
        if self.finish_auto {
            self.look.finish = if stored.is_some() { 4 } else { 0 };
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
                    self.veh_brake = Default::default();
                    self.veh_rim_light = Default::default();
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
            // boats: an airboat has wheel bones and an ordinary wheelScale,
            // but it is a boat
            self.veh_no_wheels = crate::vehicle_db::is_boat(v.vtype)
                || (!v.configs.is_empty() && v.configs.iter().all(|c| c.rim.is_none()) && v.wheel_scale.is_some_and(|w| w < 0.1) && self.discs().iter().all(|d| d.is_none()));
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
                self.veh_brake = Default::default();
                self.veh_rim_light = Default::default();
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
                        self.look.paint = srgb_to_linear(*c);
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
        // What the configuration's parts without a model change on the
        // wheels (spacers, fitment parts): out from the centre line and
        // wider, per axle. Parts with a model are counted where they are
        // fitted (the loader), so a kit taken off takes its change along.
        let with_model: std::collections::HashSet<u32> = v.mods.iter().map(|m| m.name).collect();
        let mut fit = [[0.0f32; 2]; 2];
        for f in c.parts.iter().filter(|p| !with_model.contains(p)).flat_map(|p| v.wheel_fits.get(p).into_iter().flatten()) {
            for axle in 0..2 {
                if crate::vehicle_db::fit_on_axle(f.axles, axle) {
                    fit[0][axle] += f.offset + f.spacing;
                    fit[1][axle] += f.width;
                }
            }
        }
        self.wheel_fit_cfg = fit;
        // A configuration without a colour of its own (stored as pure
        // magenta) takes the most frequent colour of the vehicle's first
        // paint list that has one.
        let listed = || v.palettes.iter().flatten().map(|p| p.0).find(|c| !crate::vehicle_db::is_placeholder(*c));
        if let Some(col) = c.colour.or_else(listed) {
            // the lists hold display (sRGB) values, the paint is linear
            self.look.paint = srgb_to_linear(col);
        }
        if self.rim_auto {
            self.rim_ratio = c.rim_ratio.unwrap_or(0.0);
        }
        self.part_look_want = c.parts_key.clone();
        // the parts with models this configuration names
        if let Some(l) = self.loaded.as_ref().filter(|l| !l.parts.is_empty()) {
            // the index its parts come from (ids are "<archive>@<offset>")
            let archive = l.parts[0].id.split('@').next().unwrap_or("");
            let ix = self.indexes.lock().unwrap().values().find(|x| x.entries.first().is_some_and(|e| e.id.split('@').next() == Some(archive))).cloned();
            if let Some(ix) = ix {
                // the model's own name, LOD suffix and all ("<id> <name> (+3 parts)")
                let model = l.source.split_whitespace().nth(1).unwrap_or("");
                let (_, _, part_names, _) = vehicle_part_options(&ix, model, v);
                // one part per group: a part list also holds the choices
                // of its option lists, of which the game takes one
                let mut groups = std::collections::HashSet::new();
                let named = |o: &crate::assemble::PartOpt| part_names.get(&o.off).is_some_and(|n| n.iter().any(|n| c.parts.contains(n)));
                // a body the configuration does not name stays as it was
                let names_body = l.parts.iter().any(|o| o.group.starts_with("body") && named(o));
                let mut want: Vec<u64> = l.parts.iter().filter(|o| (named(o) || (o.default && !names_body && o.group.starts_with("body"))) && groups.insert(o.group.clone())).map(|o| o.off).collect();
                let mut have = l.selected.clone();
                want.sort_unstable();
                have.sort_unstable();
                if want != have {
                    self.parts_request = Some(format!("{}#parts={}", l.base.split("#parts=").next().unwrap_or(&l.base), want.iter().map(|o| format!("{o:x}")).collect::<Vec<_>>().join(",")));
                }
            }
        }
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
        // Brakes per axle: named by model, or the model of a part in a
        // brake slot (drums come that way). Like the tyre, a configuration
        // without any brake part takes those of another configuration.
        let brake_names: HashMap<u32, String> = self
            .indexes
            .lock()
            .unwrap()
            .values()
            .flat_map(|ix| ix.entries.iter())
            .filter_map(|e| e.name.as_deref())
            .filter(|n| ["brakedisc_", "drumbrake_", "caliper_"].iter().any(|w| n.starts_with(w)))
            .map(|n| {
                let s = n.strip_suffix("_hi").unwrap_or(n);
                (sixng_core::meta7a::joaat(s), s.to_string())
            })
            .collect();
        let axles = |c: &crate::vehicle_db::Config| {
            let slot_models: Vec<(u32, String)> = v
                .mods
                .iter()
                .filter(|m| sixng_core::vehexport::BRAKE_SLOTS.contains(&m.slot) && c.parts.contains(&m.name))
                .filter_map(|m| Some((m.slot, m.models.iter().find_map(|h| brake_names.get(h))?.clone())))
                .collect();
            sixng_core::vehexport::brake_axles(c.brake_disc.as_deref(), c.caliper.as_deref(), &slot_models)
        };
        let none = |b: &sixng_core::vehexport::AxleBrakes| b.iter().all(|a| a.0.is_none() && a.1.is_none());
        let mut brakes = axles(c);
        if none(&brakes) {
            brakes = v.configs.iter().map(axles).find(|b| !none(b)).unwrap_or_default();
        }
        self.veh_brake = brakes;
        // the rim light rings: only what this configuration's own parts give
        let ring_models: Vec<(u32, String)> = v
            .mods
            .iter()
            .filter(|m| sixng_core::vehexport::RIM_LIGHT_SLOTS.contains(&m.slot) && c.parts.contains(&m.name))
            .filter_map(|m| Some((m.slot, m.models.iter().find_map(|h| brake_names.get(h))?.clone())))
            .collect();
        self.veh_rim_light = sixng_core::vehexport::rim_light_axles(&ring_models);
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
                        let bone = m["bone"].as_str()?;
                        // the configuration's spacers move the wheel out
                        let x = p[0] + self.wheel_fit_cfg[0][is_rear(bone) as usize].copysign(p[0]);
                        Some((bone.to_string(), Vec3::new(x, p[2], -p[1])))
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
            self.brakes = ls.iter().map(|(disc, on, l)| (*disc, *on, ren.upload(rs, &l.model, &[]))).collect();
        }
        let spots: Vec<(String, Vec3, f32)> = self
            .loaded
            .as_ref()
            .and_then(|l| l.info["wheel_mounts"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|m| {
                let mut c: [f32; 3] = serde_json::from_value(m["brake_centre"].clone()).ok()?;
                c[0] += self.wheel_fit_cfg[0][m["bone"].as_str().is_some_and(is_rear) as usize].copysign(c[0]);
                Some((m["bone"].as_str()?.to_string(), Vec3::new(c[0], c[2], -c[1]), m["brake_radius"].as_f64()? as f32))
            })
            .collect();
        let axles = if spots.is_empty() { Default::default() } else { self.veh_brake.clone() };
        let rings = if spots.is_empty() { Default::default() } else { self.veh_rim_light.clone() };
        let key = format!("{axles:?}{rings:?}");
        if key != self.brake_key {
            self.brake_key = key;
            self.brakes.clear();
            // each distinct model once, with the axles it is on
            let mut qs: Vec<(bool, [bool; 2], String)> = Vec::new();
            for (axle, a) in axles.iter().enumerate() {
                // (the rim light ring sits on the brake and does not turn)
                for (d, n) in [(true, &a.0), (false, &a.1), (false, &rings[axle])] {
                    let Some(n) = n else { continue };
                    let q = format!("^{n}_hi$");
                    match qs.iter_mut().find(|x| x.0 == d && x.2 == q) {
                        Some(x) => x.1[axle] = true,
                        None => qs.push((d, std::array::from_fn(|i| i == axle), q)),
                    }
                }
            }
            if !qs.is_empty() {
                self.brake_loading = true;
                let ws = Workspace::new(&self.game, &self.work);
                let cache = self.cache_dir();
                let slot = self.brake_pending.clone();
                let idx = self.indexes.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    // a part that does not load is left out
                    let r: Vec<(bool, [bool; 2], Loaded)> = qs.iter().filter_map(|(d, on, q)| load_query(&ws, &cache, q, 0, &idx).ok().map(|l| (*d, *on, l))).collect();
                    *slot.lock().unwrap() = Some(r);
                    ctx.request_repaint();
                });
            }
        }
        let steers: Vec<Mat4> = spots.iter().map(|(bone, _, _)| self.gear_at(bone) * self.steer_at(bone)).collect();
        let wheel_spin = self.wheel_spin;
        let brake_heat = self.brake_heat.clamp(0.0, 1.0);
        if let Some(ren) = &self.renderer {
            for (is_disc, on, g) in self.brakes.iter_mut() {
                let inst: Vec<render3d::Instance> = spots
                    .iter()
                    .zip(&steers)
                    .filter(|((bone, _, _), _)| on[is_rear(bone) as usize])
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
                    .map(|m| Some([m["radius"].as_f64()? as f32, m["half_width"].as_f64()? as f32 + self.wheel_fit_cfg[1][m["bone"].as_str().is_some_and(is_rear) as usize] / 2.0]))
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
            (self.look.coat, self.look.coat_base) = ([0.0; 4], None);
            return;
        };
        let pc = &l.info["part_colours"];
        let default = pc["default"].as_u64();
        let Some(idx) = self.part_look_want.as_ref().and_then(|k| pc["configs"][k.as_str()].as_u64()).or(default) else {
            self.part_look_table = None;
            (self.look.coat, self.look.coat_base) = ([0.0; 4], None);
            return;
        };
        let idx = idx as usize;
        // the coat of this look's body paint (clear coat, flakes)
        let coat = &pc["coat"];
        let record = coat["paint_by_look"][idx].as_u64().map(|k| &coat["records"][k as usize]).filter(|r| r.is_object());
        (self.look.coat, self.look.coat_base) = record.map_or(([0.0; 4], None), paint_coat);
        // What the fitted parts change in the look (a body kit's part
        // rebinds the ids of its panels, mostly to the body paint): part id
        // and its new entry.
        let mut patched: Vec<(usize, [u8; 4])> = Vec::new();
        for part in l.info["fitted_parts"].as_array().into_iter().flatten().filter_map(|p| p.as_str()) {
            let d = &pc["part_patches"][part];
            let rgba = d["rgba"].as_str().unwrap_or("");
            for (k, id) in d["ids"].as_array().into_iter().flatten().filter_map(|x| x.as_u64()).enumerate() {
                if let Some(h) = rgba.get(k * 8..k * 8 + 8) {
                    patched.push((id as usize & 255, std::array::from_fn(|c| u8::from_str_radix(&h[c * 2..c * 2 + 2], 16).unwrap_or(255))));
                }
            }
        }
        let patched = &patched;
        let decode = |hex: &str| -> Arc<Vec<[u8; 4]>> {
            let mut t: Vec<[u8; 4]> = (0..256).map(|i| std::array::from_fn(|k| u8::from_str_radix(&hex[i * 8 + k * 2..i * 8 + k * 2 + 2], 16).unwrap_or(255))).collect();
            for (id, e) in patched {
                t[*id] = *e;
            }
            Arc::new(t)
        };
        // a freshly loaded model shows the default look, but a fitted
        // library part (a truck bed or box) comes with the colours of
        // another vehicle and is coloured again
        let shown = match &self.part_look_done {
            Some((p, i)) if *p == l.model.path => Some(*i),
            _ if l.selected.is_empty() => default.map(|d| d as usize),
            _ => None,
        };
        let Some(hex) = pc["looks"][idx].as_str().filter(|h| h.len() == 2048) else {
            self.part_look_table = None;
            return;
        };
        // The palette rows of this look as an image for the part materials:
        // row = part id, 128 columns (a part with a colour of its own gets
        // that colour in every column).
        let ramp_key = (l.model.path.clone(), idx);
        let mut ramp_new = false;
        if self.part_ramp_done.as_ref() != Some(&ramp_key) {
            let file = l.model.path.parent().map(|d| d.join(pc["palette"].as_str().unwrap_or("vehicle_palette.png")));
            if let Some(f) = file.filter(|_| pc["rows"][idx].as_str().is_some_and(|r| r.len() == 1024)) {
                if self.part_palette.as_ref().is_none_or(|(p, _)| *p != f) {
                    self.part_palette = image::open(&f).ok().map(|i| i.to_rgba8()).filter(|i| i.width() == 128).map(|i| (f.clone(), Arc::new(i.into_raw())));
                }
                if let (Some((_, pal)), Some(rows)) = (&self.part_palette, pc["rows"][idx].as_str()) {
                    let look = decode(hex);
                    let srgb = |v: u8| {
                        let c = v as f32 / 255.0;
                        ((if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }) * 255.0 + 0.5) as u8
                    };
                    // rows 256..: the surface's second layer (colour, alpha =
                    // roughness); its metalness is the alpha of rows 0..256
                    let wear = pc["wear"][idx].as_str().filter(|w| w.len() == 2048);
                    let mut px = vec![255u8; 128 * 512 * 4];
                    for id in 0..256 {
                        let w = wear.map(|w| &w[id * 8..id * 8 + 8]);
                        let byte = |k: usize| w.and_then(|w| u8::from_str_radix(&w[k..k + 2], 16).ok());
                        let wrow = w.and_then(|w| usize::from_str_radix(&w[..4], 16).ok()).filter(|r| *r != 0xffff);
                        let (first, second) = px.split_at_mut(256 * 512);
                        let out2 = &mut second[id * 512..id * 512 + 512];
                        if let Some(src) = wrow.and_then(|r| pal.get(r * 512..r * 512 + 512)) {
                            out2.copy_from_slice(src);
                        } else {
                            out2.chunks_exact_mut(4).for_each(|c| c.copy_from_slice(&[srgb(look[id][0]), srgb(look[id][1]), srgb(look[id][2]), 255]));
                        }
                        out2.chunks_exact_mut(4).for_each(|c| c[3] = byte(4).unwrap_or(128));
                        let metal1 = byte(6).unwrap_or(0);
                        let px = first;
                        // a part id a fitted part recolours has that colour, not its old row
                        let row = if patched.iter().any(|p| p.0 == id) { 0xffff } else { usize::from_str_radix(&rows[id * 4..id * 4 + 4], 16).unwrap_or(0xffff) };
                        let out = &mut px[id * 512..id * 512 + 512];
                        match pal.get(row * 512..row * 512 + 512).filter(|_| row != 0xffff) {
                            Some(src) => out.copy_from_slice(src),
                            None => out.chunks_exact_mut(4).for_each(|c| c.copy_from_slice(&[srgb(look[id][0]), srgb(look[id][1]), srgb(look[id][2]), 255])),
                        }
                        out.chunks_exact_mut(4).for_each(|c| c[3] = metal1);
                    }
                    let im = glb::Image { key: "part ramp".into(), mips: vec![(128, 512, px)], srgb: true, bc: None };
                    let at = match l.model.images.iter().position(|i| i.key == "part ramp") {
                        Some(i) => {
                            l.model.images[i] = im;
                            i
                        }
                        None => {
                            l.model.images.push(im);
                            l.model.images.len() - 1
                        }
                    };
                    for m in l.model.materials.iter_mut().filter(|m| m.parts && m.ramp_u.is_some()) {
                        m.ramp = Some(at);
                    }
                    ramp_new = true;
                }
            }
            self.part_ramp_done = Some(ramp_key);
        }
        if shown == Some(idx) && !ramp_new {
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

    /// A vehicle is loaded: it has wheels, or part colours (boats, aircraft).
    fn is_vehicle(&self) -> bool {
        self.loaded.as_ref().is_some_and(|l| l.info["part_colours"].is_object() || l.info["wheel_mounts"].as_array().is_some_and(|a| !a.is_empty()))
    }

    /// The pointer on the model while the Liveries panel has a tool on.
    /// True when the panel used it (the camera then leaves it alone).
    fn livery_pointer(&mut self, ui: &egui::Ui, resp: &egui::Response, rect: egui::Rect) -> bool {
        if !self.is_vehicle() || self.gpu.is_none() {
            return false;
        }
        let cache = self.cache_dir();
        self.livery.ed.ready = !self.loading && self.idle_for_model() && self.frames_since_load >= 3;
        let Some(l) = &self.loaded else { return false };
        let env = crate::livery_ui::Env { game: &self.game, work: &self.work, cache: &cache };
        self.livery.pointer(ui, resp, rect, &self.cam, &l.model, &env)
    }

    /// Keep the loaded vehicle's stickers (custom liveries) on the GPU.
    fn sync_livery(&mut self, rs: &egui_wgpu::RenderState) {
        let cache = self.cache_dir();
        if !self.is_vehicle() {
            return;
        }
        let (Some(l), Some(g)) = (&self.loaded, self.gpu.as_mut()) else { return };
        let ren = self.renderer.get_or_insert_with(|| Renderer3D::new(rs));
        let parts: Vec<String> = l.info["assembled_from"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let key = crate::livery::vehicle_key(&crate::vehicle_db::model_key(&l.source));
        self.livery.sync(rs, ren, g, &l.model, &key, &parts, Path::new(&self.game), Path::new(&self.work), &cache);
    }

    fn update_wheels(&mut self, rs: &egui_wgpu::RenderState, ctx: &egui::Context) {
        // --fit-parts: the given part choice, once the vehicle and its
        // configuration's parts are in
        if self.fit_parts_once.is_some() && !self.loading && self.parts_request.is_none() {
            if let Some(l) = &self.loaded {
                let list = self.fit_parts_once.take().unwrap_or_default();
                self.parts_request = Some(format!("{}#parts={list}", l.base.split("#parts=").next().unwrap_or(&l.base)));
            }
        }
        // --random-parts: one random choice of parts once the vehicle is in
        if self.random_parts_once && !self.loading && self.parts_request.is_none() {
            if let Some(l) = self.loaded.as_ref().filter(|l| !l.parts.is_empty()) {
                let mut groups: std::collections::BTreeMap<&str, Vec<&crate::assemble::PartOpt>> = Default::default();
                for o in &l.parts {
                    groups.entry(o.group.as_str()).or_default().push(o);
                }
                let mut seed = self.random_parts_seed | 1;
                let sel = random_parts(&groups, &mut |n| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    (seed >> 11) as usize % n.max(1)
                });
                self.parts_request = Some(format!("{}#parts={}", l.base.split("#parts=").next().unwrap_or(&l.base), sel.iter().map(|o| format!("{o:x}")).collect::<Vec<_>>().join(",")));
                self.random_parts_once = false;
            } else if self.loaded.is_some() {
                self.random_parts_once = false;
            }
        }
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
                let sliding: Vec<String> = l.info["sliding_doors"].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(|n| n.to_ascii_lowercase())).collect();
                self.hinges = find_hinges(&l.model, &sliding);
                self.aims = find_aims(&l.model);
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
                let wheel: Vec<bool> = l.model.materials.iter().map(|m| m.name.ends_with(RIM_SHADER) || tyre_material(&m.name)).collect();
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
                        if m.name.ends_with(RIM_SHADER) || (m.parts && tyre_material(&m.name)) {
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
        self.wheel_floor = None;
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
            "{:?}|{}|{:?}|{}|{}|{}|{}|{}",
            self.wheel_fit_cfg,
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
        // where the tyres touch the ground
        if let Some((tr, _)) = tyre_size(&self.wheel_boxes).filter(|_| !self.wheels.is_empty()) {
            self.wheel_floor = mounts
                .iter()
                .enumerate()
                .map(|(mi, (bone, p))| {
                    let sc = xs.get(mi).map_or(1.0, |x| x[1]) * if is_rear(bone) { self.wheel_scale_rear } else { self.wheel_scale };
                    (self.gear_at(bone) * self.steer_at(bone)).transform_point3(*p).y - tr * sc
                })
                .reduce(f32::min);
        }
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
        if let (true, Some(a)) = (l.takes_clips(), &self.ped_clip.anim) {
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
                // a character clip's movement over the ground, on the root joints
                if l.takes_clips() && self.ped_clip.root != Mat4::IDENTITY {
                    if std::env::var_os("SIXNG_DEBUG_RIG").is_some() {
                        eprintln!("[clip] joints {}: {:?}; animated joints {:?}", v.len(), v.iter().take(6).map(|j| (&j.name, j.parent)).collect::<Vec<_>>(), playing.iter().flat_map(|(a, _)| a.channels.iter().map(|c| c.joint)).filter(|j| *j < 4).collect::<std::collections::BTreeSet<_>>());
                    }
                    // a root has no parent, or names itself as its parent
                    for (i, j) in v.iter_mut().enumerate().filter(|(i, j)| j.parent.is_none_or(|p| p == *i)) {
                        let _ = i;
                        j.local = (self.ped_clip.root * Mat4::from_cols_array(&j.local)).to_cols_array();
                    }
                }
                posed = v;
                &posed
            }
            true => &l.model.joints,
        };
        // hinged panels: this frame's rotation per joint
        // (a sliding door has made its whole travel at 1 on the doors slider)
        let open = [self.door_open, self.bonnet_open, self.boot_open, self.door_open.min(1.0)];
        let swing: HashMap<usize, Mat4> = self
            .hinges
            .iter()
            .filter(|(i, h)| open[h.0 as usize] != 0.0 && !self.panel_off(&l.model.joints[**i].name))
            .map(|(i, h)| (*i, if h.0 == 3 { Mat4::from_translation(h.1 * open[3]) } else { Mat4::from_quat(Quat::from_axis_angle(h.1, h.2 * open[h.0 as usize])) }))
            .collect();
        // steering: the wheel_turn bones turn about the vertical, the
        // steering wheel about its own axis
        let mut swing = swing;
        // side windows: each mover slides its glass down into the door
        if self.windows_down > 0.0 {
            for (i, d, p) in self.window_movers.iter().filter(|w| !self.panel_off(&l.model.joints[w.0].name)) {
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
        // Gas struts and stay rods of a bonnet, boot or door: each half is
        // a bone on one side (panel or body) with a target bone on the
        // other, and turns about its own mount to keep aiming at it. With
        // the panel swung, the half is turned from where its target was to
        // where it is now.
        if !self.aims.is_empty() && !swing.is_empty() {
            let pose = |i: usize, swing: &HashMap<usize, Mat4>| -> Mat4 {
                let mut chain = Vec::new();
                let mut k = Some(i);
                while let Some(j) = k.filter(|_| chain.len() < 64) {
                    chain.push(j);
                    k = js[j].parent.filter(|p| *p != j);
                }
                chain.iter().rev().fold(Mat4::IDENTITY, |m, j| m * Mat4::from_cols_array(&js[*j].local) * swing.get(j).copied().unwrap_or(Mat4::IDENTITY))
            };
            let none = HashMap::new();
            let turns: Vec<(usize, Mat4)> = self
                .aims
                .iter()
                .filter_map(|&(j, t)| {
                    let was = pose(j, &none).inverse().transform_point3(pose(t, &none).w_axis.truncate());
                    let now = pose(j, &swing).inverse().transform_point3(pose(t, &swing).w_axis.truncate());
                    (was.length() > 1e-4 && now.length() > 1e-4 && was.normalize().dot(now.normalize()) < 0.999_999).then(|| (j, Mat4::from_quat(Quat::from_rotation_arc(was.normalize(), now.normalize()))))
                })
                .collect();
            for (j, m) in turns {
                let m = swing.get(&j).map_or(m, |s| *s * m);
                swing.insert(j, m);
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

    /// Height of the ground under a model whose own lowest point is at
    /// `floor`: fitted wheels reach lower than the body they are fitted to.
    fn ground_y(&self, floor: f32) -> f32 {
        let on_wheels = self.wheels_on && !self.veh_no_wheels;
        self.wheel_floor.filter(|_| on_wheels).map_or(floor, |w| w.min(floor))
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
        // ground grid, under the tyres of a vehicle on its wheels
        let mut lo = Vec3::from(l.model.bbox[0]);
        let hi = Vec3::from(l.model.bbox[1]);
        lo.y = self.ground_y(lo.y);
        let ext = (hi - lo).max_element().max(1.0);
        let step = 10f32.powf((ext / 4.0).log10().floor()).max(0.1);
        let n = ((ext * 0.75) / step).ceil() as i32;
        // the grid stays put on the ground; its window goes with a walking
        // character, and then every tenth line is brighter so the ground
        // is seen to pass
        let walked = self.ped_clip.travel;
        let shift = ((walked.x / step).round() as i32, (walked.z / step).round() as i32);
        let c = (lo + hi) * 0.5 + Vec3::new(shift.0 as f32 * step, 0.0, shift.1 as f32 * step);
        let col = |world: i32| match (walked == Vec3::ZERO, world) {
            (true, 0) => [255, 255, 255, 90],
            (false, w) if w % 10 == 0 => [255, 255, 255, 90],
            _ => [255, 255, 255, 35],
        };
        for k in -n..=n {
            let x = c.x + k as f32 * step;
            let z = c.z + k as f32 * step;
            under.push(LineV { pos: [x, lo.y, c.z - n as f32 * step], col: col(k + shift.0) });
            under.push(LineV { pos: [x, lo.y, c.z + n as f32 * step], col: col(k + shift.0) });
            under.push(LineV { pos: [c.x - n as f32 * step, lo.y, z], col: col(k + shift.1) });
            under.push(LineV { pos: [c.x + n as f32 * step, lo.y, z], col: col(k + shift.1) });
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
        // the Liveries panel's tools take a click or drag on the model first
        if !self.livery_pointer(ui, &resp, rect) {
            self.cam.handle(ui, &resp);
        }
        if resp.double_clicked() {
            if let Some(g) = &self.gpu {
                self.cam.fly = false;
                self.cam.frame_bbox(g.bbox[0], g.bbox[1]);
                // framed on the stored pose: the walked distance is added again
                self.ped_clip.travel = Vec3::ZERO;
            }
        }
        let ppp = ctx.pixels_per_point();
        let px = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
        self.size = px;
        self.update_wheels(rs, &ctx);
        // the Liveries panel floats over the view
        if self.is_vehicle() {
            let cache = self.cache_dir();
            let env = crate::livery_ui::Env { game: &self.game, work: &self.work, cache: &cache };
            self.livery.panel(&ctx, rect, &env);
        }
        self.sync_livery(rs);
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
        // its joint matrix (see the shader). Not in a Metal 4 build.
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
        if (self.has_lamps || !self.sirens.is_empty()) && !crate::METAL4_BUILD {
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
        // a soft shadow on the ground the grid lies on, under the model
        // (not under big pieces of scenery, which bring their own ground)
        let ground = self.loaded.as_ref().and_then(|l| {
            let mut lo = Vec3::from(l.model.bbox[0]);
            let hi = Vec3::from(l.model.bbox[1]);
            lo.y = self.ground_y(lo.y);
            let size = hi - lo;
            (size.x.max(size.z) <= 40.0 && size.min_element() > 0.0).then(|| crate::render3d::GroundShadow {
                centre: Vec3::new((lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5) + Vec3::new(self.ped_clip.travel.x, 0.0, self.ped_clip.travel.z),
                half: [size.x * 0.5, size.z * 0.5],
                height: size.y,
            })
        });
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
        // an animal's fur is bound to its skin and takes the same joint matrices
        if self.hair.rig.is_some() && !skin.is_empty() {
            for g in &self.hair.gpu {
                ren.set_joints(rs, g, &skin);
            }
        }
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
            models.extend(self.brakes.iter().map(|b| &b.2));
        }
        if self.hair.show {
            models.extend(self.hair.gpu.iter());
        }
        ren.decal_bias = self.decal_bias;
        ren.ground = ground;
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
            let ready = (self.gpu.is_some() && !self.loading && self.frames_since_load >= 3 && settled)
                || (!self.loading && !self.hair.fitting && !self.auto.find_hair && !self.auto.export_hair && self.hair.settled() && self.auto.frames > 600 && !self.livery.ed.busy() && (!busy || ctx.input(|i| i.time) > 180.0));
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
        if self.is_vehicle() {
            ui.toggle_value(&mut self.livery.ed.open, "🎨 Liveries")
                .on_hover_text("Place stickers (logos, lettering) on this vehicle by hand: pictures from the game's sticker textures or your own PNG files, on the panels you choose. Saved per vehicle.");
        }
        self.vehicle_ui(ui);
        if self.part_ramp_done.is_some() && self.loaded.as_ref().is_some_and(|l| l.model.materials.iter().any(|m| m.ramp.is_some())) {
            ui.add(egui::Slider::new(&mut self.look.wear, 0.0..=1.0).text("wear"))
                .on_hover_text("Shows the second layer of the vehicle's surfaces (rust under paint, grey under bare metal), which the game's files give for every surface. Where it shows is taken from the blue channel of the parts' detail texture; how much the game shows is not known, so the amount is yours to set.");
        }
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
                        if ui.selectable_value(&mut self.look.finish, k as u32, *m).clicked() {
                            self.finish_auto = false;
                        }
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
                    let doors: Vec<usize> = self.hinges.iter().filter(|(_, h)| matches!(h.0, 0 | 3)).map(|(i, _)| *i).collect();
                    self.panel_boxes(ui, doors);
                }
                if self.hinges.values().any(|h| h.0 == 1) {
                    ui.add(egui::Slider::new(&mut self.bonnet_open, 0.0..=1.1).text("bonnet open"));
                    let bonnet: Vec<usize> = self.hinges.iter().filter(|(_, h)| h.0 == 1).map(|(i, _)| *i).collect();
                    self.panel_boxes(ui, bonnet);
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
                    let windows: Vec<usize> = self.window_movers.iter().map(|w| w.0).collect();
                    self.panel_boxes(ui, windows);
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
                    let boot: Vec<usize> = self.hinges.iter().filter(|(_, h)| h.0 == 2).map(|(i, _)| *i).collect();
                    self.panel_boxes(ui, boot);
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
        self.animal_clip_ui(ui);
        if let Some(q) = self.look_ui(ui).or_else(|| self.parts_ui(ui)).or_else(|| self.parts_request.take()) {
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

/// A random choice of parts: one option of every group. A group with a
/// default option always gets one of its options; a group without (an
/// accessory that starts off) may also get none. `next(n)` gives a number
/// below `n`.
fn random_parts(groups: &std::collections::BTreeMap<&str, Vec<&crate::assemble::PartOpt>>, next: &mut impl FnMut(usize) -> usize) -> Vec<u64> {
    let mut out = Vec::new();
    for opts in groups.values() {
        let none = if opts.iter().any(|o| o.default) { 0 } else { 1 };
        if let Some(o) = opts.get(next(opts.len() + none)) {
            out.push(o.off);
        }
    }
    out
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
    // parts whose item names a region layer: meshes, layer, archive offset
    let mut region_parts: Vec<(std::ops::Range<usize>, u32, u64)> = Vec::new();
    let mut cover_meshes: Vec<std::ops::Range<usize>> = Vec::new();
    let mut heel_meshes: Vec<std::ops::Range<usize>> = Vec::new();
    // per outfit part: its meshes, its joint list and the joint an attached
    // accessory was moved onto
    let mut rig_parts: Vec<(std::ops::Range<usize>, Vec<glb::Joint>, Option<usize>)> = Vec::new();
    let mut named_parts: Vec<(std::ops::Range<usize>, Vec<(String, Option<usize>)>)> = Vec::new();
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
                    if colour_part(&mut m, f, game, cache) {
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
                if let Some(f) = f.filter(|f| f.region_layer != 0) {
                    region_parts.push((mesh0..mesh0 + m.meshes.len(), f.region_layer, e.off));
                }
                if f.is_some_and(|f| f.covers) {
                    cover_meshes.push(mesh0..mesh0 + m.meshes.len());
                }
                if f.is_some() {
                    rig_parts.push((mesh0..mesh0 + m.meshes.len(), m.joints.iter().map(|j| glb::Joint { name: j.name.clone(), parent: j.parent, local: j.local, ibm: j.ibm }).collect(), attached));
                } else {
                    // an animal's part: its own joint list, to bind it to the root's skeleton by name
                    // (empty for a part without bones, which then rides on the bone nearest to it)
                    named_parts.push((mesh0..mesh0 + m.meshes.len(), m.joints.iter().map(|j| (j.name.clone(), j.parent)).collect()));
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
    // Hiding what is worn under something, the game's way: every vertex of
    // a part carries a region of the part's layer, and every worn item
    // lists per layer the regions it hides. A triangle goes when all its
    // vertices are in hidden regions. Skin meshes without regions fall back
    // to hiding what lies right under a garment.
    let mut by_distance: Vec<std::ops::Range<usize>> = skin_meshes.clone();
    if let Some(m) = &mut model {
        let (mut skin_hidden, mut cloth_hidden) = (0, 0);
        for (range, layer, off) in &region_parts {
            let has_regions = range.clone().filter_map(|i| m.meshes.get(i)).any(|me| me.verts.iter().any(|v| v.zone[3] != 0));
            if !has_regions {
                continue;
            }
            // Clothes under clothes stay with the distance test below: the
            // `_full` garment meshes carry no regions (their `_cull_00`
            // twins do) and part of those masks sits in a second list whose
            // key is not decoded.
            let is_skin = skin_meshes.contains(range);
            if !is_skin {
                continue;
            }
            by_distance.retain(|r| r != range);
            let masks: Vec<&str> = fx.iter().filter(|(o, _)| *o != off).flat_map(|(_, f)| f.hides.iter()).filter(|h| h.0 == *layer).map(|h| h.1.as_str()).collect();
            let hide: Vec<bool> = (0..256u32).map(|n| masks.iter().any(|k| sixng_core::wardrobe::mask_has(k, n))).collect();
            for i in range.clone() {
                let Some(me) = m.meshes.get_mut(i) else { continue };
                let before = me.indices.len() / 3;
                me.indices = me.indices.chunks_exact(3).filter(|t| !t.iter().all(|&i| hide[me.verts[i as usize].zone[3] as usize])).flatten().copied().collect();
                *(if is_skin { &mut skin_hidden } else { &mut cloth_hidden }) += before - me.indices.len() / 3;
            }
        }
        m.triangles = m.triangles.saturating_sub(skin_hidden + cloth_hidden);
        if skin_hidden + cloth_hidden > 0 {
            notes.push(format!("Worn under something: {skin_hidden} skin triangles and {cloth_hidden} triangles of clothes are hidden by the hide masks of the worn items (each item lists per layer the regions it covers; every vertex carries its region)."));
        }
    }
    if let (Some(m), false, false) = (&mut model, by_distance.is_empty(), cover_meshes.is_empty()) {
        let hidden = hide_covered_skin(m, &by_distance, &cover_meshes);
        if hidden > 0 {
            notes.push(format!("Skin under clothes: {hidden} triangles of a skin mesh without regions that lie right under a garment are hidden, so skin does not show through it (by distance, not the game's masks)."));
        }
    }
    if let (Some(m), false, false) = (&mut model, under_meshes.is_empty(), outer_meshes.is_empty()) {
        let hidden = hide_covered_skin(m, &under_meshes, &outer_meshes);
        if hidden > 0 {
            notes.push(format!("Layers: {hidden} triangles of the clothes worn under the jacket are hidden where the jacket covers them, so they do not show through it."));
        }
    }
    if let (Some(m), true, false) = (&mut model, rig_parts.is_empty(), named_parts.is_empty()) {
        let bound = rig_by_name(m, &named_parts);
        if bound > 0 {
            notes.push(format!("Rig: {bound} mesh(es) are bound to the skeleton by bone name, so a clip moves the animal."));
        }
    }
    if let (Some(m), false) = (&mut model, rig_parts.is_empty()) {
        if rig_character(m, &rig_parts, &skin_meshes) {
            notes.push("Rig: every part is bound to the shared skeleton, so the Pose controls move the body with its clothes. A part's own helper bones follow the body bone they hang from (the game drives them with data that is not decoded), and the first four of a vertex's eight bone weights are used, so joints bend more simply than in the game.".to_string());
        }
    }
    if coloured > 0 {
        notes.push(format!("Clothing colours: {coloured} part(s) coloured from the outfit data: the item's garment preset with its fabric (weave under the colour gradient, then the trim and wash layers, baked into the garment's texture; one per sub-material, picked by the garment's materialid texture, or by the part number in its vertex colour where it has none), else the item's stored colour, else row tint 0 of its tint palette (for the generic palette only when it agrees with the item's colour tag), else the colour tag itself (one of 18 words such as navy or khaki, shown in a plain shade of ours). Second and third tints are not applied."));
    }
    (model, first, names, errs, skin, colours, mask, notes)
}

/// Colour a ped clothing part of an outfit: its grey shade maps become the
/// item's stored colour times the shade, or the tint palette row at the
/// shade. Returns whether anything was coloured.
/// Sub-material per texel of a material's base texture: its `materialid`
/// texture (red x 63, as the clothing shader reads it), resampled to `w` x
/// `h`. None when the material binds none or the file is not there.
fn sub_material_map(mat: &glb::Material, roots: &[&Path], w: u32, h: u32) -> Option<Vec<u8>> {
    let want = format!("{:08x}", sixng_core::meta7a::joaat("materialid"));
    let png = mat.slots.iter().find(|s| s.param_hash == want).and_then(|s| s.png.as_ref())?;
    let img = roots.iter().find_map(|r| image::open(r.join(png)).ok())?.to_luma8();
    let (iw, ih) = img.dimensions();
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel((x * iw / w.max(1)).min(iw - 1), (y * ih / h.max(1)).min(ih - 1)).0[0];
            out.push(((p as u32 * 63 + 127) / 255).min(7) as u8);
        }
    }
    Some(out)
}

/// The layered-material index (`sixng materials`), loaded or built once.
fn material_presets(ws: &Workspace) -> Option<&'static sixng_core::matlib::Index> {
    static PRESETS: std::sync::OnceLock<Option<sixng_core::matlib::Index>> = std::sync::OnceLock::new();
    PRESETS.get_or_init(|| sixng_core::matlib::Index::load_or_build(ws).ok()).as_ref()
}

/// Colour the `weapon` materials of a weapon model from its layered-material
/// preset (the weapon library names one per model): each sub-material's base
/// colour, the sub-material of a texel from the material's `materialid`
/// texture, else of a vertex from the green byte of its raw colour stream
/// (as a vertex tint). Returns the preset used when anything was coloured.
fn colour_weapon(m: &mut glb::Model, name: &str, presets: &sixng_core::matlib::Index, cache: &Path, finish: Option<&str>) -> Option<String> {
    let preset = finish.map(str::to_string).or_else(|| presets.weapon_presets(name).first().cloned())?;
    let subs: Vec<Option<[u8; 3]>> = presets.colours(&preset)?.iter().map(|c| c.map(|c| [c[0], c[1], c[2]])).collect();
    let first = subs.iter().flatten().next().copied()?;
    // metalness and roughness per sub-material, in the tint's alpha
    let surf = presets.surface(&preset).unwrap_or(&[]);
    let code = |s: usize| surf.get(s).map_or(255, |&x| sixng_core::export::surface_code(x));
    let want = format!("{:08x}", sixng_core::meta7a::joaat("materialid"));
    let lin = |c: u8| {
        let x = c as f32 / 255.0;
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    let mut done = false;
    for mi in 0..m.materials.len() {
        if m.materials[mi].shader_hash != "e68a4a98" {
            continue;
        }
        done = true;
        let ids = m.materials[mi].slots.iter().find(|s| s.param_hash == want).and_then(|s| s.png.as_ref()).and_then(|png| Some((image::open(cache.join(png)).ok()?.to_luma8(), png.clone())));
        match ids {
            Some((img, png)) => {
                let (w, h) = img.dimensions();
                let mut px = Vec::with_capacity((w * h * 4) as usize);
                for p in img.pixels() {
                    let s = ((p.0[0] as u32 * 63 + 127) / 255).min(7) as usize;
                    let c = subs.get(s).copied().flatten().unwrap_or(first);
                    px.extend_from_slice(&[c[0], c[1], c[2], 255]);
                }
                m.images.push(glb::Image { key: format!("{png}#weaponpreset{preset}"), mips: glb::mip_chain(w, h, px), srgb: true, bc: None });
                m.materials[mi].base = Some(m.images.len() - 1);
                m.materials[mi].base_factor = [1.0, 1.0, 1.0, m.materials[mi].base_factor[3]];
            }
            None => {
                // no `materialid` texture: the sub-material is per vertex, in
                // the green byte of the raw colour stream (values 4k + 2 on
                // every weapon part looked at: k = sub-material)
                m.materials[mi].base_factor = [1.0, 1.0, 1.0, m.materials[mi].base_factor[3]];
                let b = |c: u8| (lin(c) * 255.0 + 0.5) as u8;
                for mesh in m.meshes.iter_mut().filter(|x| x.material == Some(mi)) {
                    for v in &mut mesh.verts {
                        let s = (v.col[1] >> 2).min(7) as usize;
                        let c = subs.get(s).copied().flatten().unwrap_or(first);
                        v.tint = [b(c[0]), b(c[1]), b(c[2]), code(s)];
                    }
                }
            }
        }
    }
    done.then_some(preset)
}

fn colour_part(m: &mut glb::Model, f: &PartFx, game: &Path, cache: &Path) -> bool {
    // Garment preset colours: one base colour per sub-material. The
    // sub-material of a texel comes from the material's `materialid`
    // texture; a material without one marks it per vertex, in the green
    // byte of the raw vertex colour (values 2, 6, 10 … 30 = 4 x id + 2 on
    // every garment looked at), and takes the colour as its vertex tint.
    if f.colour.is_none() && f.subs.iter().any(|c| c.is_some()) {
        let first = f.subs.iter().flatten().next().copied().unwrap_or([128; 3]);
        let shaped = cache.join("shaped");
        let mut done = std::collections::HashSet::new();
        for mi in 0..m.materials.len() {
            if !m.materials[mi].ped_shade {
                continue;
            }
            let Some(i) = m.materials[mi].base else { continue };
            if !done.insert(i) {
                continue;
            }
            let dims: Vec<(u32, u32)> = match m.images.get(i).filter(|im| im.bc.is_none()) {
                Some(im) => im.mips.iter().map(|l| (l.0, l.1)).collect(),
                None => continue,
            };
            // the fabric itself where the preset's library is at hand:
            // weave under the colour gradient, then the tint-mask layers
            if let Some(fab) = f.preset.and_then(|p| crate::fabric::fabric(game, p)) {
                let mats: Vec<usize> = (0..m.materials.len()).filter(|&k| m.materials[k].base == Some(i)).collect();
                let mat = m.materials[mi].clone();
                // The wgpu shader draws the main sub-material's weave per
                // pixel on opaque garments (alpha carries its texels); the
                // Metal 4 renderer has no such path and keeps the baked weave.
                let detail = !crate::METAL4_BUILD && std::env::var_os("SIXNG_NO_FABRIC_DETAIL").is_none() && mats.iter().all(|&k| m.materials[k].alpha_mode == "OPAQUE");
                if let Some(main) = crate::fabric::bake(m, &mats, i, &fab, &|w, h| sub_material_map(&mat, &[cache, shaped.as_path()], w, h), detail) {
                    m.images[i].key = format!("{}#fabric{:08x}{}", m.images[i].key, f.preset.unwrap_or(0), if main.is_some() { "d" } else { "" });
                    if let Some((tile, scale, rot)) = main.and_then(|s| fab.detail(s)) {
                        let (w, h, px) = tile.rgba();
                        m.images.push(glb::Image { key: format!("{}#tile", m.images[i].key), mips: glb::mip_chain(w, h, px), srgb: false, bc: None });
                        let d = glb::Detail { image: m.images.len() - 1, scale, rot, mean: tile.mean() };
                        for &k in &mats {
                            m.materials[k].detail = Some(d);
                        }
                    }
                    continue;
                }
            }
            let by_texture = sub_material_map(&m.materials[mi], &[cache, shaped.as_path()], 1, 1).is_some();
            for (level, (w, h)) in dims.into_iter().enumerate() {
                let ids = if by_texture { sub_material_map(&m.materials[mi], &[cache, shaped.as_path()], w, h) } else { None };
                let px = &mut m.images[i].mips[level].2;
                for (k, t) in px.chunks_exact_mut(4).enumerate() {
                    // the fabric shade is mid grey (about 125) on plain cloth
                    let g = t[1] as f32 / 125.0;
                    // by vertex: the texture keeps the shade alone, the colour is the vertex tint
                    let c = match &ids {
                        Some(v) => f.subs.get(v[k] as usize).copied().flatten().or(f.subs[0]).unwrap_or(first),
                        None => [255; 3],
                    };
                    for ch in 0..3 {
                        t[ch] = (c[ch] as f32 * g).min(255.0) as u8;
                    }
                }
            }
            if !by_texture {
                let lin = |v: u8| {
                    let x = v as f32 / 255.0;
                    ((if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }) * 255.0 + 0.5) as u8
                };
                // every material that shares this base texture
                let mats: Vec<usize> = (0..m.materials.len()).filter(|&k| m.materials[k].base == Some(i)).collect();
                for me in m.meshes.iter_mut().filter(|me| me.material.is_some_and(|k| mats.contains(&k))) {
                    for v in &mut me.verts {
                        let s = ((v.col[1] >> 2) & 7) as usize;
                        let c = f.subs.get(s).copied().flatten().or(f.subs[0]).unwrap_or(first);
                        v.tint = [lin(c[0]), lin(c[1]), lin(c[2]), 255];
                    }
                }
            }
            m.images[i].key = format!("{}#preset{:?}", m.images[i].key, f.subs);
        }
        if !done.is_empty() {
            return true;
        }
    }
    let pal = f.tint.and_then(|(h, row)| sixng_core::pedtint::palettes(game).ok()?.get(&h).map(|p| (p, row)));
    // `ped_tint_generic_clean` rows are often placeholders the game fills at
    // run time: such a row counts only when it is the colour the item's
    // colour tag names.
    let generic = f.tint.is_some_and(|t| t.0 == 0xa7c8_d8e0);
    let pal = pal.filter(|(p, row)| !generic || f.tag.is_none_or(|t| sixng_core::pedtint::nearest_word(p.sample(*row, 128)) == t));
    let word = f.tag.and_then(|t| sixng_core::pedtint::COLOUR_WORDS.iter().find(|w| w.0 == t)).map(|w| w.1);
    // the fabric shade is mid grey (about 125) on plain cloth: the colour is kept there
    let flat = |c: [u8; 3]| -> [[u8; 3]; 256] { std::array::from_fn(|v| c.map(|x| ((x as f32) * (v as f32 / 125.0)).min(255.0) as u8)) };
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
/// Bind the parts of an animal to the model's skeleton: every part comes
/// with its own joint list, and a vertex joint is the model's joint of the
/// same name (else of the nearest parent that has one, else the root).
/// Weights are scaled to sum to 255. Returns the meshes bound.
fn rig_by_name(m: &mut glb::Model, parts: &[(std::ops::Range<usize>, Vec<(String, Option<usize>)>)]) -> usize {
    if m.joints.is_empty() {
        return 0;
    }
    let by_name: HashMap<&str, usize> = m.joints.iter().enumerate().map(|(i, j)| (j.name.as_str(), i)).rev().collect();
    let mut bound = 0;
    for (range, pj) in parts.iter().filter(|p| !p.1.is_empty()) {
        let map: Vec<u16> = (0..pj.len())
            .map(|j| {
                std::iter::successors(Some(j), |&x| pj[x].1.filter(|&p| p != x && p < pj.len())).take(64).find_map(|x| by_name.get(pj[x].0.as_str()).copied()).unwrap_or(0) as u16
            })
            .collect();
        let maps: Vec<u16> = map;
        for mi in range.clone() {
            let Some(me) = m.meshes.get_mut(mi) else { continue };
            for v in &mut me.verts {
                let tot: u32 = v.weights.iter().map(|w| *w as u32).sum();
                if tot == 0 {
                    v.joints = [0; 4];
                    v.weights = [255, 0, 0, 0];
                    continue;
                }
                let mut w: [u32; 4] = std::array::from_fn(|k| v.weights[k] as u32 * 255 / tot);
                w[0] += 255 - w.iter().sum::<u32>();
                for k in 0..4 {
                    v.joints[k] = if w[k] > 0 { maps.get(v.joints[k] as usize).copied().unwrap_or(0) } else { 0 };
                    v.weights[k] = w[k] as u8;
                }
            }
            me.skinned = true;
            bound += 1;
        }
    }
    // Parts without bones (eyes, teeth, tongue): each vertex rides on the
    // strongest bone of the bound vertex nearest to its mesh's middle.
    let skinned: Vec<([f32; 3], u16)> = m
        .meshes
        .iter()
        .filter(|me| me.skinned)
        .flat_map(|me| me.verts.iter().map(|v| (v.pos, v.joints[(0..4).max_by_key(|&k| v.weights[k]).unwrap_or(0)])))
        .collect();
    if skinned.is_empty() {
        return bound;
    }
    for (range, _) in parts.iter().filter(|p| p.1.is_empty()) {
        for mi in range.clone() {
            let Some(me) = m.meshes.get_mut(mi).filter(|me| !me.verts.is_empty()) else { continue };
            let n = me.verts.len() as f32;
            let c = me.verts.iter().fold([0.0f32; 3], |a, v| [a[0] + v.pos[0] / n, a[1] + v.pos[1] / n, a[2] + v.pos[2] / n]);
            let d = |p: &[f32; 3]| (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2);
            let Some(j) = skinned.iter().min_by(|a, b| d(&a.0).total_cmp(&d(&b.0))).map(|x| x.1) else { continue };
            for v in &mut me.verts {
                v.joints = [j, 0, 0, 0];
                v.weights = [255, 0, 0, 0];
            }
            me.skinned = true;
            bound += 1;
        }
    }
    bound
}

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
        if std::env::var_os("SIXNG_DEBUG_RIG").is_some() {
            let first = (0..n.min(pj.len())).find(|&i| pj[i].name != joints[i].name);
            eprintln!("[rig] meshes {range:?}: {} joints (shared {n}), same {same}, first difference {:?}, attached {attached:?}, mesh {:?}", pj.len(), first.map(|i| (i, &pj[i].name, &joints[i].name)), m.meshes.get(range.start).map(|x| (&x.name, x.skinned, x.verts.len())));
        }
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
        if std::env::var_os("SIXNG_DEBUG_RIG").is_some() {
            let mut seen = std::collections::BTreeSet::new();
            for me in range.clone().filter_map(|mi| m.meshes.get(mi)) {
                for v in &me.verts {
                    for k in 0..4 {
                        if v.weights[k] > 0 && v.joints[k] as usize >= n {
                            seen.insert(v.joints[k] as usize);
                        }
                    }
                }
            }
            for j in seen.iter().take(12) {
                let chain: Vec<String> = std::iter::successors(Some(*j), |&x| pj.get(x).and_then(|q| q.parent).filter(|&p| p != x)).take(6).map(|x| format!("{}#{x}", pj[x].name)).collect();
                eprintln!("[rig]   own joint {} -> {} | chain {chain:?}", pj[*j].name, joints[up(*j) as usize].name);
            }
        }
        // The face rig's own bones (eyeballs, irises, tongue: `FB_...`,
        // `FACIAL_...`) hang from the head, and their parts must stay with
        // it; the nearest body skin under an eye is the neck.
        let face = |j: usize| std::iter::successors(Some(j), |&x| pj.get(x).and_then(|q| q.parent).filter(|&p| p != x && x >= n)).take(64).any(|x| pj.get(x).is_some_and(|q| q.name.starts_with("FB_") || q.name.starts_with("FACIAL_")));
        // The eyeball bones (`FB_L_Eyeball`, its roll, iris and pupil, and
        // the right side's) join the shared skeleton under the head, so
        // the Pose controls can turn the eyes. Parents come first in a
        // joint list, so one pass in order places each under its own.
        let mut eye: HashMap<usize, u16> = HashMap::new();
        for j in n..pj.len() {
            if !pj[j].name.starts_with("FB_") {
                continue;
            }
            let at = match joints.iter().position(|q| q.name == pj[j].name) {
                Some(i) => i,
                None => {
                    let parent = pj[j].parent.filter(|&p| p != j).map_or(0, |p| eye.get(&p).copied().unwrap_or_else(|| up(p))) as usize;
                    let local = Mat4::from_cols_array(&joints[parent].ibm) * Mat4::from_cols_array(&pj[j].ibm).inverse();
                    joints.push(glb::Joint { name: pj[j].name.clone(), parent: Some(parent), local: if local.is_finite() { local.to_cols_array() } else { pj[j].local }, ibm: pj[j].ibm });
                    joints.len() - 1
                }
            };
            eye.insert(j, at as u16);
        }
        let to_shared = |j: usize| eye.get(&j).copied().unwrap_or_else(|| up(j));
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
                let on_face: u32 = (0..4).filter(|&k| v.joints[k] as usize >= n && face(v.joints[k] as usize)).map(|k| v.weights[k] as u32).sum();
                if own * 2 >= tot && on_face * 2 < own {
                    helper.push((mi, vi));
                }
                let mut w: [u32; 4] = std::array::from_fn(|k| v.weights[k] as u32 * 255 / tot);
                let rest = 255 - w.iter().sum::<u32>();
                w[0] += rest;
                for k in 0..4 {
                    v.joints[k] = if w[k] > 0 { to_shared(v.joints[k] as usize) } else { 0 };
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

/// `"<query>#finish=<part type>:<preset>,…"` (hex pairs) → the query and the
/// chosen weapon finishes by part type.
fn split_finish(q: &str) -> (&str, HashMap<String, String>) {
    match q.split_once("#finish=") {
        Some((b, l)) => (b.trim(), l.split(',').filter_map(|x| x.trim().split_once(':')).map(|(a, b)| (a.to_string(), b.to_string())).collect()),
        None => (q, HashMap::new()),
    }
}

fn load_query_sel(
    ws: &Workspace,
    cache: &Path,
    q: &str,
    sel: Option<Vec<u64>>,
    lod: usize,
    idx: &Arc<Mutex<HashMap<String, Arc<ArchiveIndex>>>>,
) -> anyhow::Result<Loaded> {
    let (q, finish) = split_finish(q);
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
                info.hats = look.outfits[k].parts.iter().filter(|p| p.slot == "hat").map(|p| p.drawable.to_ascii_lowercase()).collect();
                for (oi, o) in look.outfits.iter().enumerate() {
                    for p in &o.parts {
                        if p.strands {
                            if oi == k {
                                info.strands.push((p.slot.clone(), p.drawable.clone(), p.guid.clone().unwrap_or_default()));
                                if let Some((pal, t)) = p.tint.as_ref().and_then(|t| Some((u32::from_str_radix(&t.palette, 16).ok()?, t))) {
                                    info.strand_tints.push((p.slot.clone(), pal, t.rows[0]));
                                }
                                if let Some(h) = p.hair_preset.as_deref().and_then(|h| u32::from_str_radix(h, 16).ok()) {
                                    info.strand_presets.push((p.slot.clone(), h));
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
                            fx.insert(e.off, PartFx { tint: p.tint.as_ref().and_then(|t| Some((u32::from_str_radix(&t.palette, 16).ok()?, t.rows[0]))).filter(|t| t.1 != 255), colour: p.colour.map(|c| [c[1], c[2], c[3]]), tag: p.colour_tag, subs: p.preset_colours.clone(), preset: p.preset.as_deref().and_then(|h| u32::from_str_radix(h, 16).ok()), head: p.slot == "head", body, skeletal: body || matches!(p.slot.as_str(), "head" | "eyes" | "teeth" | "teeth accessory"), covers: matches!(p.slot.as_str(), "top" | "outer" | "bottom" | "full" | "feet" | "gloves"), layer: match p.slot.as_str() { "outer" => 2, "top" | "bottom" | "full" => 1, _ => 0 }, heel: p.slot == "feet" && p.drawable.split('_').any(|t| matches!(t, "sh" | "mh" | "hh")), region_layer: p.layer.as_deref().and_then(|l| u32::from_str_radix(l, 16).ok()).unwrap_or(match p.slot.as_str() { "body upper" => 0xb4d9_d470, "body lower" => 0xe062_2b84, _ => 0 }), hides: p.hides.iter().filter_map(|(l, m)| Some((u32::from_str_radix(l, 16).ok()?, m.clone()))).collect(), skins: if p.slot == "head" { p.skins.iter().filter_map(|(a, n)| Some((u64::from_str_radix(a, 16).ok()?, u64::from_str_radix(n, 16).unwrap_or(0)))).collect() } else { Vec::new() } });
                        }
                    }
                }
                if let Some(p) = sixng_core::pedtint::palettes(&ws.game).ok().and_then(|ps| ps.get(&0x1e64_f584)) {
                    for row in 0..p.h.min(256) {
                        let c = p.sample(row as u8, 128);
                        // rows repeat and the unused ones are black or white
                        let dull = c.iter().all(|x| *x < 6) || c.iter().all(|x| *x > 250);
                        if !dull && !info.hair_swatches.iter().any(|s| (0..3).all(|k| (s[k] as i32 - c[k] as i32).abs() < 6)) {
                            info.hair_swatches.push(c);
                        }
                    }
                }
                // strand hair: the grooms placed for each hair asset
                match sixng_core::hairlink::get(&ws.work, &a, &ix.entries) {
                    Ok(t) => {
                        // the hair colour preset of each strand part, with its noise texture
                        let presets = if info.strand_presets.is_empty() { None } else { sixng_core::hairtone::Table::load_or_build(&ws).map_err(|e| info.notes.push(format!("Hair colours: {e}"))).ok() };
                        let mut noises: HashMap<u64, Option<Arc<sixng_core::hairtone::Noise>>> = HashMap::new();
                        for (slot, placeholder, guid) in &info.strands {
                            // by the item's hair asset, else by the character's own placeholder
                            let Some(l) = u64::from_str_radix(guid, 16).ok().and_then(|g| t.resolve(g, look.fit.as_deref())).or_else(|| guid.is_empty().then(|| t.by_placeholder(placeholder)).flatten()) else { continue };
                            let colour = info.strand_tints.iter().find(|t| t.0 == *slot && t.1 == 0x1e64_f584).and_then(|t| Some(sixng_core::pedtint::palettes(&ws.game).ok()?.get(&t.1)?.sample(t.2, 128)));
                            // presets whose colours are white are not hair colours (their flag is set)
                            let tone = info.strand_presets.iter().find(|p| p.0 == *slot).and_then(|p| Some((p.1, presets.as_ref()?.get(p.1)?.clone()))).filter(|(_, p)| !p.flag).map(|(name, preset)| {
                                let noise = if preset.texture == 0 { None } else { noises.entry(preset.texture).or_insert_with(|| sixng_core::hairtone::Noise::load(&ws.game, preset.texture).map(Arc::new)).clone() };
                                HairTone { name, preset, noise }
                            });
                            let colour = colour.or(tone.as_ref().map(|t| sixng_core::hairtone::display(t.preset.a)));
                            info.hair.push(HairPick { slot: slot.clone(), placeholder: l.placeholder.clone(), confidence: l.confidence, grooms: t.uncontested(l), off_head: 0, shown: l.confidence >= 0.5 && slot != "body hair", colour, tone });
                        }
                    }
                    Err(e) => info.notes.push(format!("Strand hair: {e}")),
                }
                // the character's other hair styles (its own placeholders
                // that have grooms placed), to tick instead
                if let Ok(t) = sixng_core::hairlink::get(&ws.work, &a, &ix.entries) {
                    let mut owners: Vec<String> = vec![format!("{}_", look.model)];
                    owners.extend(info.strands.iter().filter_map(|s| sixng_core::outfit::outfit_prefix(&s.1)).map(|p| format!("{p}_")));
                    let mut extra: Vec<&sixng_core::hairlink::Link> = t.links.values().flatten().filter(|l| !l.grooms.is_empty() && owners.iter().any(|o| l.placeholder.starts_with(o.as_str())) && !info.hair.iter().any(|h| h.placeholder == l.placeholder)).collect();
                    extra.sort_by(|x, y| x.placeholder.cmp(&y.placeholder).then(y.confidence.total_cmp(&x.confidence)));
                    extra.dedup_by(|x, y| x.placeholder == y.placeholder);
                    for l in extra {
                        let slot = sixng_core::pedlook::slot_of(&l.placeholder);
                        if slot == "hair" || slot == "beard" {
                            info.hair.push(HairPick { slot: format!("other {slot}"), placeholder: l.placeholder.clone(), confidence: l.confidence, grooms: t.uncontested(l), off_head: 0, shown: false, colour: None, tone: None });
                        }
                    }
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
            // The hat as seen from inside the head: its centre is 10 cm
            // under the crown, in the middle of the top of the head.
            let is_head = |n: &str| n.contains("_head_") || n.starts_with("head_");
            let hat: Vec<[f32; 3]> = skin.iter().filter(|(n, _)| l.hats.contains(&n.to_ascii_lowercase())).flat_map(|(_, p)| p.iter().copied()).collect();
            let top = skin.iter().filter(|(n, _)| is_head(n)).flat_map(|(_, p)| p.iter()).map(|p| p[2]).fold(f32::MIN, f32::max);
            if !hat.is_empty() && top > f32::MIN && !l.hair.is_empty() {
                let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                for p in skin.iter().filter(|(n, _)| is_head(n)).flat_map(|(_, p)| p.iter()).filter(|p| p[2] > top - 0.12) {
                    for a in 0..2 {
                        lo[a] = lo[a].min(p[a]);
                        hi[a] = hi[a].max(p[a]);
                    }
                }
                let head: Vec<[f32; 3]> = skin.iter().filter(|(n, _)| is_head(n)).flat_map(|(_, p)| p.iter().copied()).collect();
                l.hat = Some(Arc::new(sixng_core::groom::Shell::new([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, top - 0.10], &hat, &head)));
                l.notes.push("Hat: the scalp hair is pressed under the hat's lining (strand points beyond it are pulled in to 3 mm under it), so the hair does not come through the hat; an opening the hat encloses (over a cap's strap) counts as closed. A stand-in, not the game's method: the outfit asks for the plain hair, and no stored rule says how the game keeps it under the hat.".to_string());
            }
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
                        h.shown = h.slot != "body hair" && !h.slot.starts_with("other ") && !h.grooms.is_empty() && (h.confidence >= 0.5 || (h.confidence >= 0.2 && h.off_head == 0));
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
    let (model, lods, mut info) = export_load(&a, e, cache, lod)?;
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
            let presets = material_presets(ws);
            let mut coloured = Vec::new();
            // the finishes the weapon's family offers for each shown part's type
            let mut finishes: Vec<serde_json::Value> = Vec::new();
            for p in &parts {
                let pn = p.name.clone().unwrap_or_default();
                match export_load(&a, p, cache, level) {
                    Ok((mut m, _, pi)) => {
                        let offered = presets.and_then(|ix| Some((ix, ix.weapon_finishes(&pn)?)));
                        let chosen = offered.and_then(|(_, (slot, list))| finish.get(slot).filter(|pr| list.iter().any(|v| &v.1 == *pr)).map(String::as_str));
                        if let Some((ix, (slot, list))) = offered {
                            if !finishes.iter().any(|f| f["slot"] == slot) {
                                let swatch = |pr: &str| -> Vec<String> {
                                    let mut c: Vec<String> = ix.colours(pr).into_iter().flatten().flatten().map(|c| format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect();
                                    c.dedup();
                                    c.truncate(3);
                                    c
                                };
                                // a finish has no recovered name: label it by its colours (nearest colour words)
                                let label = |pr: &str| -> String {
                                    let mut w: Vec<&str> = ix.colours(pr).into_iter().flatten().flatten().map(|c| sixng_core::pedtint::nearest_word([c[0], c[1], c[2]])).collect();
                                    w.dedup();
                                    let mut seen = std::collections::HashSet::new();
                                    w.retain(|x| seen.insert(*x));
                                    w.truncate(3);
                                    w.join(" / ")
                                };
                                let group = sixng_core::matlib::part_type_name(slot).map(str::to_string).unwrap_or_else(|| sixng_core::weapon::part_slot(name, &pn).unwrap_or_default());
                                finishes.push(serde_json::json!({"slot": slot, "group": group, "part": pn, "selected": chosen,
                                    "options": list.iter().map(|(v, pr)| serde_json::json!({"variant": v, "preset": pr, "colours": swatch(pr), "label": label(pr)})).collect::<Vec<_>>()}));
                            }
                        }
                        if let Some(pr) = presets.and_then(|ix| colour_weapon(&mut m, &pn, ix, cache, chosen)) {
                            coloured.push(format!("{pn}={pr}"));
                        }
                        got.push((sixng_core::weapon::part_slot(name, &pn).unwrap_or_default(), bones(&pi), m, pn))
                    }
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
                if !finishes.is_empty() {
                    o.insert("weapon_finishes".into(), serde_json::json!(finishes));
                    o.insert("weapon_finishes_note".into(), "finishes are the variant presets that the finish mods of a weapon list for the part type of this component (frame, slide, grip, barrel …). No finish mod is a weapon's default mod, so the default is the part's own preset. Variant names are not recovered.".into());
                }
                if !coloured.is_empty() {
                    o.insert("weapon_presets".into(), serde_json::json!(coloured));
                    o.insert("weapon_colour_note".into(), "weapon parts take the base colours of their layered-material preset (weapon library of data.rpf), one per sub-material, picked by the part's materialid texture or, without one, per vertex (green byte of the colour stream / 4). The tile texture, the gradient and the tint-mask, dirt and wear layers are not applied.".into());
                }
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
    // vehicles: mod parts (bonnets, bumpers, spoilers, exhausts, truck beds
    // ...) are separate models; the vehicle data says which bone each is
    // modelled around and which stock bones it replaces. The parts of the
    // first configuration are on by default.
    let mut veh_opts = Vec::new();
    if e.rtype.as_deref() == Some("ft") && info["wheel_mounts"].as_array().is_some_and(|w| !w.is_empty()) {
        let db = sixng_core::names::work_dir().and_then(|w| crate::vehicle_db::shared(&export::game_root(&a).to_string_lossy(), &w.to_string_lossy()).ok());
        if let Some(v) = db.as_ref().and_then(|db| crate::vehicle_db::find(db, &crate::vehicle_db::model_key(name))) {
            let (opts, mut kits, part_names, assets) = vehicle_part_options(&ix, name, v);
            // no choice made yet: the parts of the first configuration
            let chosen: Vec<u64> = sel
                .clone()
                .unwrap_or_else(|| {
                    // one per group (a part list also holds the choices of its option lists)
                    let mut groups = std::collections::HashSet::new();
                    opts.iter().filter(|o| o.default && groups.insert(o.group.clone())).map(|o| o.off).collect()
                })
                .into_iter()
                .filter(|o| kits.contains_key(o))
                .collect();
            // The stock alternatives of the fitted parts (bonnet variants,
            // aerial, light bar and plate mounts, engine pieces): a fitted
            // part sets keys, and each switch of a fitted part shows the
            // element its options give for them, else its default.
            const STOCK: u64 = u64::MAX;
            let mut chosen = chosen;
            let fitted: std::collections::HashSet<u32> = chosen.iter().flat_map(|o| part_names.get(o).into_iter().flatten().copied()).collect();
            let keys: std::collections::HashSet<u32> = fitted.iter().flat_map(|p| v.part_keys.get(p).into_iter().flatten().copied()).collect();
            // A fitted part can change the wheels (a widebody kit, spacers):
            // each wheel moves out from the centre line by the offsets and
            // spacings for its axle, and its tyre widens. Applied to the
            // stored wheel mounts, which place wheels and brakes.
            let (mut out, mut wide) = ([0.0f32; 2], [0.0f32; 2]);
            for f in fitted.iter().flat_map(|p| v.wheel_fits.get(p).into_iter().flatten()) {
                for axle in 0..2 {
                    if crate::vehicle_db::fit_on_axle(f.axles, axle) {
                        out[axle] += f.offset + f.spacing;
                        wide[axle] += f.width;
                    }
                }
            }
            if out != [0.0; 2] || wide != [0.0; 2] {
                for m in info["wheel_mounts"].as_array_mut().into_iter().flatten() {
                    let axle = m["bone"].as_str().is_some_and(is_rear) as usize;
                    for k in ["centre", "pivot", "brake_centre"] {
                        if let Some(x) = m[k][0].as_f64() {
                            m[k][0] = (x + (out[axle] as f64).copysign(x)).into();
                        }
                    }
                    if let Some(w) = m["half_width"].as_f64() {
                        m["half_width"] = (w + wide[axle] as f64 / 2.0).into();
                    }
                }
                info["wheel_fit_applied"] = serde_json::json!({"out": out, "width": wide});
            }
            // the part names fitted: their colour changes are applied with the look
            info["fitted_parts"] = serde_json::json!(fitted.iter().map(|p| format!("{p:08x}")).collect::<Vec<_>>());
            let stock: Vec<(u64, u32, Vec<u32>)> = v
                .switches
                .iter()
                .filter(|s| s.part.is_none_or(|p| fitted.contains(&p)))
                .filter_map(|s| s.pick(&keys))
                .filter_map(|e| Some((*assets.get(&e.asset)?, e.bone, e.hides.clone())))
                .collect();
            if !stock.is_empty() {
                kits.insert(STOCK, stock);
                chosen.push(STOCK);
            }
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
                let binds = |info: &serde_json::Value| -> HashMap<u32, sixng_core::weapon::Mat> {
                    info["drawable"]["skeleton"]
                        .as_array()
                        .map(|b| b.iter().filter_map(|b| Some((joaat(b["name"].as_str()?), serde_json::from_value(b["bind_matrix"].clone()).ok()?))).collect())
                        .unwrap_or_default()
                };
                let mut bind = binds(&info);
                let (mut names, mut errs) = (Vec::new(), Vec::new());
                // bodies first: the parts that mount on a bed's bones (its
                // rear bumper, its tailgate) need the bed in place
                let is_body = |o: &u64| opts.iter().any(|x| x.off == *o && x.group.starts_with("body"));
                let mut order: Vec<&u64> = chosen.iter().filter(|o| is_body(o)).collect();
                order.extend(chosen.iter().filter(|o| !is_body(o)));
                // a model two chosen parts fit on the same bone goes on once
                // (a pickup's rear bumper: of its bed part and of the part
                // in the rear bumper slot)
                let mut once: std::collections::HashSet<(u64, u32)> = std::collections::HashSet::new();
                for o in order {
                    // the pickup bed is attached by name: it may sit at the origin
                    let by_name = is_body(o);
                    for (off, bone, _) in &kits[o] {
                        // A part lists a model per mounting bone (side
                        // guards in short, mid and long for the chassis
                        // lengths): only the ones whose bone this vehicle
                        // has are fitted. Bone 0 is the vehicle itself.
                        if *bone != 0 && !bind.contains_key(bone) && !by_name {
                            continue;
                        }
                        if !once.insert((*off, *bone)) {
                            continue;
                        }
                        let Some(pe) = ix.entries.iter().find(|x| x.off == *off) else { continue };
                        let pn = pe.name.clone().unwrap_or_default();
                        match export_load(&a, pe, cache, level) {
                            Ok((mut m, _, part_info)) => {
                                // the part is modelled around its bone: put it at the bone's bind pose
                                let t = bind.get(bone).copied().unwrap_or(sixng_core::weapon::IDENTITY);
                                // A body brings bones of its own (the bed's
                                // bumper_r, boot ...). A part on a bone of
                                // that name belongs on the body's, not on
                                // the chassis's bone of the same name.
                                if by_name {
                                    for (h, b) in binds(&part_info) {
                                        bind.insert(h, (Mat4::from_cols_array(&t) * Mat4::from_cols_array(&b)).to_cols_array());
                                    }
                                }
                                let c = Mat4::from_cols_array(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
                                transform_model(&mut m, c * Mat4::from_cols_array(&t) * c.inverse());
                                let first = model.meshes.len();
                                glb::merge(&mut model, m);
                                // sticker zones are numbered per model: mark
                                // which fitted part a vertex came from (the
                                // part's place in `assembled_from`, from 1)
                                let slot = (names.iter().position(|n| *n == pn).unwrap_or(names.len()) + 1).min(255) as u8;
                                for v in model.meshes[first..].iter_mut().flat_map(|me| me.verts.iter_mut()) {
                                    v.zone[1] = slot;
                                }
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
                    selected: chosen.into_iter().filter(|o| *o != STOCK).collect(),
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

impl ModelView {
    /// Whether the panel on this bone is to stay shut (its box is unticked).
    fn panel_off(&self, bone: &str) -> bool {
        !self.panels_off.is_empty() && self.panels_off.contains(&bone.to_ascii_lowercase())
    }

    /// A row of tick boxes, one per panel bone: an unticked panel stays shut
    /// while its slider moves the others.
    fn panel_boxes(&mut self, ui: &mut egui::Ui, joints: Vec<usize>) {
        let Some(l) = self.loaded.as_ref() else { return };
        let mut names: Vec<String> = joints.iter().filter_map(|i| l.model.joints.get(*i)).map(|j| j.name.to_ascii_lowercase()).collect();
        names.sort();
        ui.horizontal_wrapped(|ui| {
            for n in names {
                let mut on = !self.panels_off.contains(&n);
                if ui.checkbox(&mut on, panel_label(&n)).on_hover_text(format!("Unticked, this one stays shut while the slider moves the others ({n}).")).changed() {
                    if on {
                        self.panels_off.remove(&n);
                    } else {
                        self.panels_off.insert(n);
                    }
                }
            }
        });
    }
}

/// What a panel's tick box says, from its bone name: `door_dside_f` is the
/// driver's front door, `window_lr_mover` the left rear window.
fn panel_label(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    let end = |c: &str| match c {
        "f" => "front",
        "r" => "rear",
        "m" => "middle",
        _ => "",
    };
    if let Some(r) = n.strip_prefix("door_") {
        let (side, at) = r.split_once('_').unwrap_or((r, ""));
        let side = match side {
            "dside" => "driver",
            "pside" => "passenger",
            o => o,
        };
        return format!("{side} {}", end(at)).trim().to_string();
    }
    if let Some(r) = n.strip_prefix("window_").map(|r| r.trim_end_matches("_mover")) {
        if r.len() == 2 {
            let side = if r.starts_with('l') { "left" } else { "right" };
            return format!("{side} {}", end(&r[1..])).trim().to_string();
        }
        return r.to_string();
    }
    n
}

/// The hinged panels of a vehicle: door, bonnet and boot bones, with the
/// axis and the direction that opens them. Doors turn about the vertical
/// through their bone, away from the car's centre line; bonnet and boot turn
/// about the lateral axis so that the panel rises. The direction comes from
/// where the panel's vertices lie relative to its bone (a front-hinged
/// bonnet and a rear-hinged door open the right way); the opening angle is
/// the slider's, not a stored limit.
///
/// A door bone named in `sliding` (model.json `sliding_doors`, from the bone
/// flags) slides along its bone's track instead: the bone's own forward
/// axis, which the model turns a little about the vertical so that the door
/// moves out of its opening as it moves back. It always moves towards the
/// rear. How far is not stored: the door's front edge stops 5 cm short of
/// the rear end of its opening at sill height (where a wheel arch cuts into
/// the door the opening is shorter there), so that a roller bracket at the
/// door's bottom front edge stays in the opening instead of running into
/// the panel behind it; and never more than 0.9 of the door's length.
fn find_hinges(model: &glb::Model, sliding: &[String]) -> HashMap<usize, (u8, Vec3, f32)> {
    let kind = |n: &str| match n.to_ascii_lowercase().as_str() {
        "door_dside_f" | "door_dside_r" | "door_pside_f" | "door_pside_r" | "door_dside_m" | "door_pside_m" => Some(0u8),
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
    // a sliding door's track in model space, pointing to the rear (+Z here;
    // the bone's forward axis is its local Z after the change to Y-up)
    let track = |h: usize| -> Option<Vec3> {
        sliding.contains(&model.joints[h].name.to_ascii_lowercase()).then(|| {
            let z = Mat4::from_cols_array(&model.joints[h].ibm).inverse().z_axis.truncate().normalize_or_zero();
            if z.z < 0.0 { -z } else { z }
        })
    };
    let mut sum: HashMap<usize, (Vec3, f32)> = HashMap::new();
    // a sliding door's own vertices: place along its track, height
    let mut span: HashMap<usize, Vec<(f32, f32)>> = HashMap::new();
    for me in model.meshes.iter().filter(|m| m.skinned) {
        for v in &me.verts {
            if let Some(h) = owner.get(v.joints[0] as usize).copied().flatten() {
                let e = sum.entry(h).or_insert((Vec3::ZERO, 0.0));
                e.0 += Vec3::from(v.pos);
                e.1 += 1.0;
                if let Some(t) = track(h).filter(|_| v.joints[0] as usize == h) {
                    span.entry(h).or_default().push((t.dot(Vec3::from(v.pos)), v.pos[1]));
                }
            }
        }
    }
    sum.into_iter()
        .filter_map(|(h, (s, n))| {
            let centre = s / n.max(1.0);
            let pivot = Mat4::from_cols_array(&model.joints[h].ibm).inverse().w_axis.truncate();
            let d = centre - pivot;
            let k = kind(&model.joints[h].name)?;
            if let (Some(t), Some(s)) = (track(h), span.get(&h).filter(|s| s.len() >= 8)) {
                let mut along: Vec<f32> = s.iter().map(|p| p.0).collect();
                along.sort_by(|a, b| a.total_cmp(b));
                // the panel's front edge: a bracket that reaches forward of
                // it is a few vertices only (32 of 3,564 on the minivan)
                let (front, rear) = (along[along.len() * 3 / 100], along[along.len() - 1]);
                let (low, high) = s.iter().fold((f32::MAX, f32::MIN), |m, p| (m.0.min(p.1), m.1.max(p.1)));
                // the rear end of the opening at sill height
                let sill = s.iter().filter(|p| p.1 < low + 0.15 * (high - low)).map(|p| p.0).fold(f32::MIN, f32::max);
                let travel = (sill - front - 0.05).min(0.9 * (rear - along[0])).max(0.0);
                // the travel in the bone's own space (a swing is applied there)
                let local = Mat4::from_cols_array(&model.joints[h].ibm).transform_vector3(t * travel);
                return Some((h, (3, local, 1.0)));
            }
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

/// Struts and rods that span a hinged panel and the body: a bone named
/// `strut_male_<x>` / `strut_female_<x>` with a `strut_male_target_<x>` /
/// `strut_female_target_<x>` bone, or `<x>_lookat<y>` with `<x>_target<y>`,
/// where one of the two hangs from a door, the bonnet or the boot and the
/// other does not hang from the same panel.
fn find_aims(model: &glb::Model) -> Vec<(usize, usize)> {
    let names: Vec<String> = model.joints.iter().map(|j| j.name.to_ascii_lowercase()).collect();
    let by_name: HashMap<&str, usize> = names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let panel = |n: &str| matches!(n, "door_dside_f" | "door_dside_r" | "door_pside_f" | "door_pside_r" | "bonnet" | "boot");
    // the panel a joint hangs from
    let owner = |i: usize| {
        let (mut k, mut depth) = (Some(i), 0);
        while let Some(j) = k {
            if panel(&names[j]) {
                return Some(j);
            }
            depth += 1;
            k = model.joints[j].parent.filter(|p| *p != j && depth < 64);
        }
        None
    };
    names
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let target = if n.contains("_lookat") {
                n.replacen("_lookat", "_target", 1)
            } else if n.starts_with("strut_") && !n.contains("target") && !n.contains("mesh") {
                if n.contains("_female") { n.replacen("_female", "_female_target", 1) } else if n.contains("_male") { n.replacen("_male", "_male_target", 1) } else { return None }
            } else {
                return None;
            };
            let t = *by_name.get(target.as_str())?;
            (owner(i) != owner(t)).then_some((i, t))
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
///   side, towards the nearest leg's bone, until it lies level;
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
        // the bay is on the leg's side of the hinge: the door swings that
        // way until it lies level, by whatever angle that takes (a quarter
        // turn for a door hanging straight down, most of a half turn for
        // one modelled swung right out, as on lazer's main bays)
        let side = (leg.1 - p).dot(across);
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

/// What the viewer draws of a coat record (model.json
/// `part_colours.coat.records`): clear coat intensity and roughness, flake
/// orientation and size, and the base coat roughness when the record
/// overrides it. Film, sheen and anisotropy are not drawn.
fn paint_coat(r: &serde_json::Value) -> ([f32; 4], Option<f32>) {
    let f = |k: &str| r[k].as_f64().unwrap_or(0.0) as f32;
    let on = |k: &str| r[k].as_bool().unwrap_or(false);
    let clear = if on("use_clear_coat") { f("clear_coat_intensity").clamp(0.0, 1.0) } else { 0.0 };
    let flakes = on("use_flakes") && f("flake_coverage") > 0.0;
    (
        [clear, f("clear_coat_roughness").clamp(0.0, 1.0), if flakes { f("flake_orientation").max(0.001) } else { 0.0 }, f("flake_size").max(0.0)],
        on("override_base_coat_roughness").then(|| f("base_coat_roughness").clamp(0.04, 1.0)),
    )
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
/// The mod parts of a vehicle that have models here: the options, per
/// option the models it places (model, bone, stock bones it hides), and per
/// option the name hashes of its parts (what a configuration's part list
/// holds; two parts can fit the same model).
#[allow(clippy::type_complexity)]
fn vehicle_part_options(ix: &ArchiveIndex, vehicle: &str, v: &crate::vehicle_db::Vehicle) -> (Vec<crate::assemble::PartOpt>, HashMap<u64, Vec<(u64, u32, Vec<u32>)>>, HashMap<u64, Vec<u32>>, HashMap<u64, u64>) {
    let mut part_names: HashMap<u64, Vec<u32>> = HashMap::new();
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
    let is_body = |stem: &str| ["_pickup", "_flatbed", "_cargobed"].iter().any(|b| stem.ends_with(b));
    // the slot of the parts that fit a body behind the cab (pickup bed,
    // flatbed, tow bed ...); its name is not known
    const BODY_SLOT: u32 = 0x1c9d_d6fe;
    // asset id -> model
    let by_asset: HashMap<u64, (&String, &sixng_core::scan::Entry)> = stems.iter().map(|(stem, e)| (sixng_core::meta7a::asset_id(stem), (stem, *e))).collect();
    let mut opts = Vec::new();
    let mut kits = HashMap::new();
    // the parts of the first configuration are on to begin with
    let first_config: std::collections::HashSet<u32> = v.configs.first().map(|c| c.parts.iter().copied().collect()).unwrap_or_default();
    for m in &v.mods {
        if m.elements.is_empty() {
            continue;
        }
        // the models its elements name: by asset id in a vehicle's own
        // pack (a pickup's bed and rear bumper, kits), by name hash in the
        // library (truck beds, shared light bars ...). The chassis part
        // lists every stock alternative (bonnets, aerials, roof lights), of
        // which the game takes some by rules that are not decoded: it is
        // left to its name.
        let named: Vec<(&(&String, &sixng_core::scan::Entry), usize)> = m
            .models
            .iter()
            .enumerate()
            .filter(|_| m.slot != joaat("chassis"))
            .filter_map(|(k, h)| {
                let by_asset = m.assets.get(k).filter(|a| **a != 0).and_then(|a| by_asset.get(a));
                by_asset.or_else(|| by_hash.get(h).and_then(|x| x.iter().find(|x| joaat(x.0) == *h))).map(|x| (x, k))
            })
            .collect();
        let mut ms: Vec<&(&String, &sixng_core::scan::Entry)>;
        let list: Vec<(u64, u32, Vec<u32>)> = if !named.is_empty() {
            ms = named.iter().map(|x| x.0).collect();
            named.iter().map(|(x, k)| (x.1.off, m.elements[*k].0, m.elements[*k].1.clone())).collect()
        } else {
            // else the model called like the part itself
            let Some(models) = by_hash.get(&m.name) else { continue };
            // a name that is both a model and a kit: the model itself
            ms = models.iter().filter(|x| joaat(x.0) == m.name).collect();
            if ms.is_empty() {
                ms = models.iter().collect();
            }
            ms.sort_by(|a, b| a.0.cmp(b.0));
            ms.dedup_by(|a, b| a.0 == b.0);
            if ms.is_empty() {
                continue;
            }
            // elements and models pair up in order (left before right in both)
            ms.iter().enumerate().map(|(k, x)| {
                let el = m.elements.get(k).or(m.elements.last()).unwrap();
                (x.1.off, el.0, el.1.clone())
            }).collect()
        };
        let first = ms[0].1;
        // a second part that fits the same model: the option answers to
        // both names
        if kits.contains_key(&first.off) {
            part_names.entry(first.off).or_default().push(m.name);
            if first_config.contains(&m.name) {
                if let Some(o) = opts.iter_mut().find(|o: &&mut crate::assemble::PartOpt| o.off == first.off) {
                    o.default = true;
                }
            }
            continue;
        }
        // wheels and brakes are fitted at the wheel mounts by the viewer
        if ["rim_", "tyre_", "brakedisc_", "caliper_", "drumbrake_"].iter().any(|w| ms[0].0.starts_with(w)) {
            continue;
        }
        let part = ms[0].0.strip_prefix(&format!("{}_", crate::vehicle_db::model_key(vehicle))).unwrap_or(ms[0].0).to_string();
        let label = if ms.len() > 1 { format!("{} (+{})", part.replace('_', " "), ms.len() - 1) } else { part.replace('_', " ") };
        opts.push(crate::assemble::PartOpt {
            off: first.off,
            id: first.id.clone(),
            name: ms[0].0.to_string(),
            // the part that fits a pickup's bed (and its rear bumper)
            group: if m.slot == BODY_SLOT || ms.iter().any(|x| is_body(x.0)) { "body".to_string() } else { sixng_core::vehicles::mod_slot_name(m.slot).map(|s| s.to_string()).unwrap_or_else(|| format!("slot {:08x}", m.slot)) },
            label,
            default: first_config.contains(&m.name),
        });
        part_names.entry(first.off).or_default().push(m.name);
        kits.insert(first.off, list);
    }
    // A pickup's bed is a model named `<vehicle>_pickup` (also `_flatbed`
    // ...), fitted by a part of the vehicle's pack whose elements name it
    // by asset id. Where no part names it (vehicle data from before the
    // asset ids, or a bed no part lists) it is offered as a part of its
    // own, placed at the `mod_bed_attach` bone (where those parts put
    // their models; the vehicle's origin without it), and is on to begin
    // with when the vehicle has configurations.
    let key = crate::vehicle_db::model_key(vehicle);
    let placed: std::collections::HashSet<u64> = kits.values().flatten().map(|x: &(u64, u32, Vec<u32>)| x.0).collect();
    // no configuration names a body part: the first one is on to begin with
    if !v.configs.is_empty() && !opts.iter().any(|o| o.group == "body" && o.default) {
        if let Some(o) = opts.iter_mut().filter(|o| o.group == "body").min_by(|a, b| a.label.cmp(&b.label)) {
            o.default = true;
        }
    }
    for (i, body) in ["pickup", "flatbed", "cargobed"].into_iter().enumerate() {
        let stem = format!("{key}_{body}");
        let Some(e) = stems.get(&stem) else { continue };
        if placed.contains(&e.off) || kits.contains_key(&e.off) || e.rtype.as_deref() != Some("ft") {
            continue;
        }
        opts.push(crate::assemble::PartOpt { off: e.off, id: e.id.clone(), name: stem.clone(), group: "body (link not known)".into(), label: body.to_string(), default: i == 0 && !v.configs.is_empty() && !opts.iter().any(|o| o.group == "body") });
        kits.insert(e.off, vec![(e.off, joaat("mod_bed_attach"), Vec::new())]);
    }
    // Parts without a model that set a key some switch answers to (a
    // front number plate, a roof aerial ...): offered as choices too, so a
    // configuration's and the user's choice of them reaches the switches.
    // They place nothing themselves.
    let answered: std::collections::HashSet<u32> = v.switches.iter().flat_map(|s| s.options.iter().flat_map(|o| [o.0, o.1])).filter(|k| *k != 0).collect();
    let with_model: std::collections::HashSet<u32> = part_names.values().flatten().copied().collect();
    // So are parts whose every model is a switch's to show (a truck's side
    // guards: one of several lengths and heights).
    let mut keyed: Vec<&u32> = v.part_keys.iter().filter(|(p, k)| !with_model.contains(p) && k.iter().any(|k| answered.contains(k))).map(|(p, _)| p).collect();
    let owners: Vec<u32> = v.switches.iter().filter_map(|s| s.part).filter(|p| !with_model.contains(p)).collect();
    keyed.extend(owners.iter());
    keyed.sort();
    keyed.dedup();
    if let Some(any) = ix.entries.first() {
        for p in keyed {
            // not an archive offset: the top bit marks a part without a model
            let off = (1u64 << 63) | *p as u64;
            opts.push(crate::assemble::PartOpt { off, id: any.id.clone(), name: format!("{p:08x}"), group: format!("no model {p:08x}"), label: "fitted (changes stock pieces)".into(), default: first_config.contains(p) });
            part_names.entry(off).or_default().push(*p);
            kits.insert(off, Vec::new());
        }
    }
    opts.sort_by(|a, b| a.group.cmp(&b.group).then(a.label.cmp(&b.label)));
    // asset id -> model, for the chassis part's stock alternatives
    let mut assets: HashMap<u64, u64> = by_asset.iter().map(|(a, x)| (*a, x.1.off)).collect();
    // a library part's element names its model by name hash (the low half
    // of the value), not by asset id
    for e in v.switches.iter().flat_map(|s| s.options.iter().filter_map(|o| o.2.as_ref()).chain(s.default.as_ref())) {
        if !assets.contains_key(&e.asset) {
            let h = e.asset as u32;
            if let Some(x) = by_hash.get(&h).and_then(|x| x.iter().find(|x| joaat(x.0) == h)) {
                assets.insert(e.asset, x.1.off);
            }
        }
    }
    (opts, kits, part_names, assets)
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
                || j.contains("e68a4a98") && !j.contains("\"gunmetal\"") && !j.contains("\"weapon_parts\"")
                // and from before weapon parts took their preset colours
                || j.contains("e68a4a98") && !j.contains("\"weapon_preset\"")
                // and from before their metalness and roughness per sub-material
                || j.contains("\"weapon_parts\"") && !j.contains("\"sub_material_surface\"")
        });
    // Cached ped clothing from before the fabric shade came from the packed map.
    // Cached tyres whose shader is the `_lod` tyre variant, from before its
    // placeholder palette (entry 0 pure red) was left unapplied.
    let stale_tyre = std::fs::read_to_string(dir.join("model.json")).is_ok_and(|j| j.contains("b994d670") && j.contains("GLB COLOR_0 = palette entry"));
    let stale_cloth = std::fs::read_to_string(dir.join("model.json")).is_ok_and(|j| j.contains("739e3d0d") && !j.contains("shade_source"));
    // Cached animal parts from before part skins (eyes, tint-mask coats).
    let stale_animal = e.name.as_deref().is_some_and(|n| sixng_core::partskin::animal_root(n).is_some())
        && std::fs::read_to_string(dir.join("model.json")).is_ok_and(|j| !j.contains("\"part_skin\""));
    let opts = ExportOptions {
        out: cache.to_path_buf(),
        png: true,
        dds: false,
        raw: false,
        force: head || shaped || !skins.is_empty() || stale_tint || stale_vehicle || stale_animal || stale_cloth || stale_tyre,
        skip_embedded_textures: false,
        body_shape,
        head_skin: HEAD_SKIN.load(std::sync::atomic::Ordering::Relaxed),
        paint: None,
        tex_remap: Default::default(), ped_tints: Default::default(),
        part_skins: e.name.as_ref().filter(|_| !skins.is_empty()).map(|n| (n.to_ascii_lowercase(), skins.to_vec())).into_iter().collect(),
        part_colours: Default::default(),
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
/// A display (sRGB) colour as the linear one the renderer's paint takes.
fn srgb_to_linear(c: [f32; 3]) -> [f32; 3] {
    c.map(|v| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) })
}

fn srgb32(c: [f32; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn random_parts_keep_fitted_groups_fitted() {
        let opt = |off: u64, group: &str, default: bool| crate::assemble::PartOpt { off, id: String::new(), name: String::new(), group: group.into(), label: String::new(), default };
        let all = [opt(1, "bonnet", true), opt(2, "bonnet", false), opt(3, "spoiler", false), opt(4, "spoiler", false)];
        let mut groups: std::collections::BTreeMap<&str, Vec<&crate::assemble::PartOpt>> = Default::default();
        for o in &all {
            groups.entry(o.group.as_str()).or_default().push(o);
        }
        // the highest number: the last option, or none where none is allowed
        let top = super::random_parts(&groups, &mut |n| n - 1);
        assert_eq!(top, vec![2]);
        let first = super::random_parts(&groups, &mut |_| 0);
        assert_eq!(first, vec![1, 3]);
    }

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
