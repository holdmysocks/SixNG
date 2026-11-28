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
/// The second region layer of body skins (see `glb::Mesh::region2`): the waist strip both halves carry.
const SKIN_LAYER2: u32 = 0x2758_22c4;
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
    /// That preset's control values (`None` when it could not be read).
    pub body_preset: Option<BodyShape>,
    /// The body shape the outfit was loaded with.
    pub body_used: Option<BodyShape>,
    /// Strand-hair assets of the outfit shown: (slot, placeholder name, asset GUID).
    pub strands: Vec<(String, String, String)>,
    /// Their stored tints by slot: palette name hash and row of tint 0.
    pub strand_tints: Vec<(String, u32, u8)>,
    /// Hair colour preset (name hash) of the strand parts, by slot.
    pub strand_presets: Vec<(String, u32)>,
    /// Painted hair layer set per strand item (slot, tag `023e5a2f` along the item's chain).
    pub strand_layer_sets: Vec<(String, u32)>,
    /// What the viewer did or could not do (skin tone source, unresolved items).
    pub notes: Vec<String>,
    /// Strand hair found for the outfit's hair assets (`hairlink`).
    pub hair: Vec<HairPick>,
    /// Drawables of the outfit's hats (lower case).
    pub hats: Vec<String>,
    /// The hat of the outfit shown, to keep the hair under it.
    pub hat: Option<Arc<sixng_core::groom::Shell>>,
    /// The character's head, body and garments, which the strand style
    /// pass is damped against.
    pub collide: Option<Arc<sixng_core::groom::Colliders>>,
    /// The colours of the game's hair palette (`ped_tint_hair`, each row at
    /// mid shade, sRGB), to pick from.
    pub hair_swatches: Vec<[u8; 3]>,
    /// Every part of the character's outfits and wardrobe (`pedset`).
    pub parts: Option<Arc<sixng_core::pedset::PartSet>>,
}

/// The grooms of one hair asset of an outfit.
#[derive(Clone)]
pub struct HairPick {
    /// hair, brows, beard, lashes, body hair
    pub slot: String,
    /// The placeholder the grooms were placed under.
    pub placeholder: String,
    /// 1 for a link read from the game. For an inferred one the model
    /// probability of the placement (0..1); under 0.5 it is a guess and
    /// starts hidden.
    pub confidence: f32,
    /// The link was read from the game (`HairLinks::exact`), not inferred.
    pub exact: bool,
    pub grooms: Vec<u64>,
    /// Pieces of an inferred link left out because their strand roots do
    /// not lie on this head.
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

/// A character's hair piece (hair, brows, lashes, beard ...) and how it is
/// coloured: its own colour or stored colours, or the same as another piece.
#[derive(Clone, Debug)]
pub struct HairPiece {
    /// The item slot (alternatives count as their piece: "other hair" as "hair").
    pub slot: String,
    /// Coloured as this other piece (which has its own colour); None: its own.
    pub link: Option<String>,
    pub colour: [f32; 3],
    /// Each groom takes the colours of the hair preset its item names.
    pub stored: bool,
}

/// The piece a hair item's slot belongs to: an alternative ("other hair",
/// "other beard") is coloured with its piece.
fn piece_slot(slot: &str) -> &str {
    slot.strip_prefix("other ").unwrap_or(slot)
}

/// The hair colour of one hair item as the viewer draws it: the preset of
/// each of the four strand variants (`hairtone::Table::variants`).
#[derive(Clone)]
pub struct HairTone {
    pub name: u32,
    /// The strand colour set: the one the outfit's colour item gives the piece's category, else `name`.
    pub set: u32,
    /// hair, brows, lashes, beard ...
    pub slot: String,
    /// The strand parameter set the hair asset names (`hairsets`; 0: none read).
    pub param: u32,
    pub variants: [sixng_core::hairtone::Preset; 4],
}

impl HairTone {
    /// Base colour of a strand of `variant` at `t` along it (0 root, 1 tip),
    /// as the hair lighting takes it (`hairtone::ramp`; the stored bytes
    /// used as linear values).
    fn rgb(&self, variant: u8, t: f32) -> [u8; 4] {
        let p = &self.variants[(variant & 3) as usize];
        let c = sixng_core::hairtone::ramp(p.a, p.b, t);
        [(c[0] * 255.0 + 0.5) as u8, (c[1] * 255.0 + 0.5) as u8, (c[2] * 255.0 + 0.5) as u8, 255]
    }

    /// Colour of each guide strand of a groom (guides drawn alone).
    fn strands(&self, g: &sixng_core::groom::Groom) -> Vec<[u8; 3]> {
        (0..g.strands).map(|s| self.rgb((s & 3) as u8, 0.5)).map(|c| [c[0], c[1], c[2]]).collect()
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
    /// An anatomical insert drawn over the body skin (the genitals, the
    /// nipples: `pedlook::is_skin_insert`).
    pub insert: bool,
    /// Heads and body skin: the textures the outfit item names (colour,
    /// normal); the first colour texture in a store is used.
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
    /// Heads: the painted hair layers of the outfit's hair items.
    pub under: Vec<UnderPaint>,
    /// The layer the item's vertex regions belong to (upper body skin
    /// `b4d9d470`, lower body skin `e0622b84`, …); 0 when not known.
    pub region_layer: u32,
    /// The item's hide masks: layer -> 256-bit region set (hex).
    pub hides: Vec<(u32, String)>,
    /// The item's second mask list (`hides_sub`): layer, key (1 to 3), mask.
    pub hides_sub: Vec<(u32, u32, String)>,
    /// Heads and body skin: the decorations (tattoos, makeup, body marks)
    /// the outfit bakes into the colour texture (`peddeco` names).
    pub decorations: Vec<u32>,
    /// The outfit's skin tone (`peddeco::SKIN_TONE_TAG` name; 0: none),
    /// which head makeup starts from.
    pub skin_tone: u32,
}

/// The strand count rule of a piece from its hair LOD preset (a record of the parsed presets) and the first
/// ramp of its parameter set, whose mean width the game takes: Thickness x its curve at 16 samples.
fn strand_lod(lp: &serde_json::Value, ramp: &sixng_core::hairtone::ParamRamp, coat: &sixng_core::groom::Coat) -> Option<glb::StrandLod> {
    let num = |k: &str| lp[k].as_f64().map(|x| x as f32);
    let t = ((0..16).map(|i| (ramp.width * ramp.at(i as f32 / 15.0)[3]).max(1e-6)).sum::<f32>() / 16.0).min(0.0255);
    let version = num("ThicknessCurveVersion")? as u8;
    if version != 1 && version != 2 {
        return None;
    }
    Some(glb::StrandLod { strands: coat.guides.len() as u32, stored: coat.stored as u32, version, t, target: num("TargetThickness")?, dist_scale: num("DistanceScale")?, min_dist: num("MinDistance")?, use_curve: num("UseThicknessCurve")?, cutoff: num("DistanceCutoff").unwrap_or(f32::MAX), radius: 0.0, shadow_scale: num("ShadowLodScale").unwrap_or(1.0), gi_scale: num("GILodScale").unwrap_or(0.0), gi_cutoff: num("GIDistanceCutoff").unwrap_or(f32::MAX) })
}

/// A painted hair layer of the head skin with its two colours: the hair
/// colour the game draws on the scalp under the strands, and the brows
/// under the brow strands.
#[derive(Clone)]
pub struct UnderPaint {
    pub layer: sixng_core::hairtone::UnderLayer,
    /// The two colours of the layer's preset (linear: the game takes the bytes / 255 as they are).
    pub a: [f32; 3],
    pub b: [f32; 3],
}

/// Paints the hair layers into the head's colour texture; returns how many
/// were painted. The game draws them in the head's own pixel program
/// (read from its code by the shader archive work); per layer, with M the
/// mask and the detail texture hex-tiled, each tile turned by the direction
/// map:
///
///   d = detail.r ^ 2.2, k = detail.b ^ 2.2
///   tri = max(0, M.g / hi if that is <= 1, else (1 - M.g) / (1 - hi))
///   density = sat((M.g - lo) / (hi - lo) + strength x (2 d - 1) x tri)
///   w = density x k x opacity, colour = mix(A, B, d)
///   albedo = skin x (1 - sat(sum w)) + sum(w x colour)
///
/// The game picks one of the three hex vertices per pixel by a dither that
/// changes every frame; here the three are blended by their squared
/// weights, which is its mean. Left out: the layer's normal map and
/// material values, mask overrides
/// and the extra mask, and two skin-side sums whose use is not read. The
/// fourth coordinate set holds the scalp in its first tile; triangles
/// outside it are not painted (how the game's sampler treats them is not
/// read).
fn bake_under(m: &mut glb::Model, layers: &[UnderPaint], game: &Path) -> usize {
    if layers.is_empty() {
        return 0;
    }
    let Ok(sets) = glb::uv_sets(&m.path) else { return 0 };
    let lin = |x: f32| if x < 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) };
    let enc = |x: f32| if x < 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    struct Tex {
        w: usize,
        h: usize,
        px: Vec<u8>,
    }
    impl Tex {
        /// Bilinear; `wrap` repeats, else the edges are held.
        fn at(&self, u: f32, v: f32, wrap: bool) -> [f32; 3] {
            let (x, y) = (u * self.w as f32 - 0.5, v * self.h as f32 - 0.5);
            let (x0, y0) = (x.floor(), y.floor());
            let (fx, fy) = (x - x0, y - y0);
            let ix = |i: f32, n: usize| if wrap { (i as i64).rem_euclid(n as i64) as usize } else { (i.max(0.0) as usize).min(n - 1) };
            let p = |i: f32, j: f32, c: usize| self.px[(ix(j, self.h) * self.w + ix(i, self.w)) * 4 + c] as f32 / 255.0;
            [0, 1, 2].map(|c| (p(x0, y0, c) * (1.0 - fx) + p(x0 + 1.0, y0, c) * fx) * (1.0 - fy) + (p(x0, y0 + 1.0, c) * (1.0 - fx) + p(x0 + 1.0, y0 + 1.0, c) * fx) * fy)
        }
    }
    let load = |h: u64| -> Option<Tex> {
        let (store, t) = sixng_core::ttpf::global(game).get(h)?;
        let (px, w, h) = sixng_core::ttpf::decode_rgba(t, &store.data(t).ok()?, 0).ok()?;
        (w > 0 && h > 0).then_some(Tex { w: w as usize, h: h as usize, px })
    };
    let mut done = 0;
    for (k, (uv, idx)) in sets.iter().enumerate() {
        let Some(ii) = m.meshes.get(k).and_then(|me| me.material).and_then(|mi| m.materials.get(mi)).and_then(|mt| mt.base) else { continue };
        let img = &mut m.images[ii];
        if img.bc.is_some() || img.mips.is_empty() {
            continue;
        }
        let (w, h, srgb) = (img.mips[0].0 as usize, img.mips[0].1 as usize, img.srgb);
        let mut px = std::mem::take(&mut img.mips[0].2);
        // per texel: the sum of the layers' weights and of weight x colour
        let mut acc: HashMap<usize, [f32; 4]> = HashMap::new();
        for l in layers {
            let y = &l.layer;
            let luv = &uv[if y.other_uv { 2 } else { 1 }];
            if luv.is_empty() || luv.len() != uv[0].len() {
                continue;
            }
            let (Some(mask), Some(dir), Some(det)) = (load(y.mask), load(y.direction), load(y.detail)) else { continue };
            let s = 2.0 * 3f32.sqrt() * y.tile;
            let weight_colour = |u: f32, v: f32| -> (f32, [f32; 3]) {
                let mg = mask.at(u, v, false)[1];
                // hex grid: skewed coordinates, the three vertices of the triangle and their weights
                let (sx, sy) = (u * s - v * s / 3f32.sqrt(), 2.0 * v * s / 3f32.sqrt());
                let (bx, by) = (sx.floor(), sy.floor());
                let (fx, fy) = (sx - bx, sy - by);
                let tz = 1.0 - fx - fy;
                let st = if tz < 0.0 { 1.0 } else { 0.0 };
                let s2 = 2.0 * st - 1.0;
                let ws = [-tz * s2, st - fy * s2, st - fx * s2].map(|x: f32| x * x);
                let vs = [[bx + st, by + st], [bx + st, by + 1.0 - st], [bx + 1.0 - st, by + st]];
                let (mut d, mut kk) = (0.0, 0.0);
                for (wt, vx) in ws.iter().zip(vs) {
                    // the vertex in texture coordinates, a random shift, and the tile's direction
                    let cy = vx[1] * 3f32.sqrt() / (2.0 * s);
                    let cx = vx[0] / s + cy / 3f32.sqrt();
                    let r = ((12.9898 * vx[0] + 78.233 * vx[1]).sin() * 43758.5).rem_euclid(1.0);
                    let dv = dir.at(cx, cy, false);
                    let (mut dx, mut dy) = (dv[0] * 2.0 - 1.0, dv[1] * 2.0 - 1.0);
                    let dl = (dx * dx + dy * dy).sqrt();
                    if dl > 1e-6 {
                        dx /= dl;
                        dy /= dl;
                    } else {
                        (dx, dy) = (1.0, 0.0);
                    }
                    let (ox, oy) = (u - cx, v - cy);
                    // the game's turn: direction (0, 1) is none; where the direction points to +x a flagged coordinate is mirrored
                    let (mut tu, mut tv) = ((ox * dy + oy * dx) * y.detail_scale + cx + r, (oy * dy - ox * dx) * y.detail_scale + cy + r);
                    if dx > 0.0 && y.mirror[0] {
                        tu = 1.0 - tu;
                    }
                    if dx > 0.0 && y.mirror[1] {
                        tv = 1.0 - tv;
                    }
                    let t = det.at(tu, tv, true);
                    d += wt * t[0];
                    kk += wt * t[2];
                }
                let wsum = ws.iter().sum::<f32>().max(1e-6);
                let (d, kk) = ((d / wsum).powf(2.2), (kk / wsum).powf(2.2));
                let tri = if y.hi > 0.0 && mg / y.hi <= 1.0 { mg / y.hi } else if y.hi < 1.0 { (1.0 - mg) / (1.0 - y.hi) } else { 0.0 }.max(0.0);
                let density = ((mg - y.lo) / (y.hi - y.lo).max(1e-6) + y.strength * (2.0 * d - 1.0) * tri).clamp(0.0, 1.0);
                (density * kk * y.opacity, [0, 1, 2].map(|c| l.a[c] + (l.b[c] - l.a[c]) * d))
            };
            for tri in idx.chunks_exact(3) {
                let v = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                if v.iter().any(|&i| i >= luv.len()) {
                    continue;
                }
                let b = v.map(|i| luv[i]);
                if y.other_uv && b.iter().any(|p| p[0] < 0.0 || p[0] > 1.0 || p[1] < 0.0 || p[1] > 1.0) {
                    continue;
                }
                let a = v.map(|i| [uv[0][i][0] * w as f32, uv[0][i][1] * h as f32]);
                let area = (a[1][0] - a[0][0]) * (a[2][1] - a[0][1]) - (a[2][0] - a[0][0]) * (a[1][1] - a[0][1]);
                if area.abs() < 1e-6 {
                    continue;
                }
                let lo = |c: usize| a.iter().map(|p| p[c]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
                let hi = |c: usize, n: usize| (a.iter().map(|p| p[c]).fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(n.saturating_sub(1));
                for py in lo(1)..=hi(1, h) {
                    for pxx in lo(0)..=hi(0, w) {
                        let p = [pxx as f32 + 0.5, py as f32 + 0.5];
                        let e = |i: usize, j: usize| ((a[j][0] - a[i][0]) * (p[1] - a[i][1]) - (a[j][1] - a[i][1]) * (p[0] - a[i][0])) / area;
                        let bw = [e(1, 2), e(2, 0), e(0, 1)];
                        // a little over the edge, so the texture filter finds no unpainted texel at a seam
                        if bw.iter().any(|x| *x < -0.02) {
                            continue;
                        }
                        let (wt, col) = weight_colour(bw[0] * b[0][0] + bw[1] * b[1][0] + bw[2] * b[2][0], bw[0] * b[0][1] + bw[1] * b[1][1] + bw[2] * b[2][1]);
                        if wt > 0.0 {
                            // a texel two triangles reach (a shared edge) keeps the larger weight of this layer
                            let e = acc.entry(py * w + pxx).or_insert([0.0; 4]);
                            if wt > e[3] {
                                *e = [col[0] * wt, col[1] * wt, col[2] * wt, wt];
                            }
                        }
                    }
                }
            }
            // fold this layer into the texture: albedo = skin x (1 - w) + w x colour (layers of one head do not
            // overlap in practice, so one after the other equals the game's sum)
            for (o, e) in acc.drain() {
                let wt = e[3].clamp(0.0, 1.0);
                for c in 0..3 {
                    let old = px[o * 4 + c] as f32 / 255.0;
                    let old = if srgb { lin(old) } else { old };
                    let new = old * (1.0 - wt) + e[c] / e[3] * wt;
                    px[o * 4 + c] = ((if srgb { enc(new) } else { new }).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
            // another layer or colour is another texture for the renderer's cache
            img.key += &format!("#under{:016x}:{:?}:{:?}", y.mask, l.a.map(|c| (c * 255.0) as u8), l.b.map(|c| (c * 255.0) as u8));
            done += 1;
        }
        img.mips = glb::mip_chain(w as u32, h as u32, px);
    }
    done
}

/// A finished load, tagged with the request it answers.
type Pending = Arc<Mutex<Option<(u64, Result<Loaded, String>)>>>;

/// Face variant the viewer exports ped heads with (`ExportOptions::head_skin`).
pub static HEAD_SKIN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Body shape: one value per control of `bodyshape::CONTROLS`, in its order
/// (12 `gcweight*` -1..1, 12 `gcmuscle*` 0..1, `gcbutt` 0..1, `gcbreasts`
/// -1..1), for the game's body-shape rig on the shared skeleton
/// (`ExportOptions::body_shape`).
pub type BodyShape = [f32; sixng_core::bodyshape::CONTROLS.len()];
/// The neutral body shape (every control 0).
pub const NEUTRAL_BODY: BodyShape = [0.0; sixng_core::bodyshape::CONTROLS.len()];
/// Index of `gcbutt` and `gcbreasts` in a `BodyShape`.
const BODY_BUTT: usize = 24;
const BODY_BREASTS: usize = 25;
/// First of the further shape controls (breast height and spacing, chest
/// height, cleavage, hips, belly roundness, head scale, biceps, triceps),
/// shown under "Shape details".
const BODY_DETAILS: usize = 26;

/// Slider label of a shape control (`gcbreastheight` -> `breast height`).
fn detail_label(name: &str) -> String {
    let n = name.trim_start_matches("gc").trim_start_matches('_');
    for (a, b) in [("breastheight", "breast height"), ("breastspacing", "breast spacing"), ("chestheight", "chest height"), ("bellyround", "belly roundness"), ("hipwidth", "hip width"), ("hipheight", "hip height"), ("headscale", "head scale"), ("musclebiceps", "biceps"), ("muscletriceps", "triceps"), ("plasticbreasts", "plastic breasts")] {
        if n == a {
            return b.to_string();
        }
    }
    n.to_string()
}
/// Body shape the viewer exports outfit parts with.
pub static BODY_SHAPE: Mutex<BodyShape> = Mutex::new(NEUTRAL_BODY);
/// Set by `--body`: outfits keep that shape instead of their preset.
static BODY_PINNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The outfit whose preset was applied last (query up to the look), so a
/// reload of the same outfit keeps the sliders.
static BODY_PRESET_FOR: Mutex<String> = Mutex::new(String::new());

/// The `--body-shape` spec of a shape (non-zero controls only).
pub fn body_spec(s: &BodyShape) -> String {
    sixng_core::bodyshape::CONTROLS.iter().zip(s).filter(|(_, v)| **v != 0.0).map(|(c, v)| format!("{}={v}", c.0)).collect::<Vec<_>>().join(",")
}

/// A shape from control values (`bodyshape::parse_spec`, `preset_controls`).
pub fn body_from(controls: &[(String, f32)]) -> BodyShape {
    let mut s = NEUTRAL_BODY;
    for (n, v) in controls {
        // Controls past the slider set (CONTROLS may grow) have no slot here.
        if let Some(slot) = sixng_core::bodyshape::CONTROLS.iter().position(|c| c.0 == n).and_then(|k| s.get_mut(k)) {
            *slot = *v;
        }
    }
    s
}

/// The game's shape for an outfit's body-shape preset (its preset items, or
/// its profile word; neutral when the outfit has none; `None` when the
/// preset cannot be read).
fn preset_shape(game: &Path, tag: Option<&str>, items: &[String]) -> Option<BodyShape> {
    match sixng_core::bodyshape::outfit_preset(game, tag, items) {
        None => Some(NEUTRAL_BODY),
        Some(r) => r.ok().map(|c| body_from(&c)),
    }
}
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
    /// `--hair-colour r,g,b`: one colour for a character's hair in place of
    /// its stored colours.
    pub hair_colour: Option<[f32; 3]>,
    /// `--hair-pieces "brows:r,g,b;lashes:r,g,b"`: own colours for some of
    /// a character's hair pieces.
    pub hair_pieces: Vec<(String, [f32; 3])>,
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
    /// Weather type id for the game's lighting (`sixng_core::daylight`);
    /// empty = the viewer's own light set-up.
    pub weather: String,
    /// The game's lighting for `weather` has been applied to `look`.
    daylight_applied: bool,
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
    /// model, look and the configuration whose wear levels the image holds
    part_ramp_done: Option<(PathBuf, usize, String, u8)>,
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
    /// Frames summed for an averaged screenshot (count, linear sums).
    shot_acc: (u32, Vec<f32>),
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
    clip_picker: crate::clip_picker::ClipPicker,
    /// Pose of the character shown (degrees), see `PedPose`.
    pub ped_pose: PedPose,
    /// Body shape of the character shown (`BODY_SHAPE` when it was loaded).
    body_shape_shown: BodyShape,
    /// Face expression: the shape keys (morph targets) of the meshes shown,
    /// by name, with the weight set in the panel; and the weights the GPU
    /// vertices were last built with.
    expr: Vec<(String, f32)>,
    expr_shown: Vec<f32>,
    expr_filter: String,
    /// Weights to set on the next model that has them (`--expr`).
    pub expr_auto: Vec<(String, f32)>,
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

/// Parameter set (fibre widths, roughness, translucency) per hair slot for a piece whose hair asset
/// is not known (each asset names its own, `hairsets`): the sets one character's assets name.
use sixng_core::hairexport::DEFAULT_PARAMSETS;
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
    collide: Option<Arc<sixng_core::groom::Colliders>>,
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
    /// Characters: the colour of each hair piece, and the piece of each groom.
    pub pieces: Vec<HairPiece>,
    slot_of: HashMap<u64, String>,
    /// The piece the hair palette swatches paint.
    paint: usize,
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
            collide: None,
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
            pieces: Vec::new(),
            slot_of: HashMap::new(),
            paint: 0,
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
    /// The piece whose colour piece `i` takes: itself, or the one it links to.
    fn resolve(&self, mut i: usize) -> usize {
        for _ in 0..self.pieces.len() {
            match self.pieces[i].link.as_ref().and_then(|l| self.pieces.iter().position(|p| p.slot == *l)) {
                Some(j) if j != i => i = j,
                _ => break,
            }
        }
        i
    }

    /// Per groom of a character: its colour and whether it takes its stored colours.
    fn groom_colours(&self) -> HashMap<u64, ([f32; 3], bool)> {
        self.slot_of
            .iter()
            .filter_map(|(g, slot)| {
                let i = self.resolve(self.pieces.iter().position(|p| p.slot == *slot)?);
                Some((*g, (self.pieces[i].colour, self.pieces[i].stored)))
            })
            .collect()
    }

    /// The stored colours drawn: per piece for a character, else all or none.
    fn tones_drawn(&self) -> HashMap<u64, HairTone> {
        if self.pieces.is_empty() {
            return if self.stored_colours { self.tones.clone() } else { HashMap::new() };
        }
        let gc = self.groom_colours();
        self.tones.iter().filter(|(g, _)| gc.get(*g).is_none_or(|c| c.1)).map(|(g, t)| (*g, t.clone())).collect()
    }

    fn want_key(&self) -> String {
        let mut tones: Vec<(u64, u32)> = self.tones_drawn().iter().map(|(k, t)| (*k, t.name)).collect();
        tones.sort();
        let mut own: Vec<(u64, [u32; 3])> = self.groom_colours().iter().map(|(g, c)| (*g, c.0.map(f32::to_bits))).collect();
        own.sort();
        format!("{tones:?}|{own:?}|{:?}|{}|{:?}|{}|{}|{}|{:?}|{:?}|{:?}", self.selected, self.width_mm, self.colour, self.skin_colour && !self.skin.is_empty(), self.full_coat, self.density, self.hat.as_ref().map(|h| h.id), self.collide.as_ref().map(|c| c.id), self.rig.as_ref().map(|r| (r.verts.len(), r.joints.len())))
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
            weather: String::new(),
            daylight_applied: false,
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
            shot_acc: (0, Vec::new()),
            zoom_after_load: None,
            keep_cam_once: false,
            hair_head: None,
            hair_head_n: 0,
            ped_clip: PedClip::default(),
            clip_picker: Default::default(),
            ped_pose: PedPose::default(),
            body_shape_shown: NEUTRAL_BODY,
            expr: Vec::new(),
            expr_shown: Vec::new(),
            expr_filter: String::new(),
            expr_auto: Vec::new(),
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
                .and_then(|(a, ix)| {
                    let mut cat = sixng_core::groom::catalog_cached(&work, &a, &ix.entries)?;
                    // LOD chains by the files' names where the table of contents is readable
                    if let Ok(t) = sixng_core::hairlink::get(&work, &a, &ix.entries) {
                        t.relabel(&mut cat);
                    }
                    Ok(cat)
                })
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
                    let ren = self.renderer.get_or_insert_with(|| Renderer3D::new_viewer(rs));
                    // strands are lit with the game's hair tables
                    if let Some(note) = ren.load_hair_luts(rs, std::path::Path::new(&self.game)) {
                        eprintln!("[hair] {note}");
                    }
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
        let collide = self.hair.collide.clone();
        let rig = self.hair.rig.clone();
        let tones = self.hair.tones_drawn();
        let own: HashMap<u64, [f32; 3]> = self.hair.groom_colours().into_iter().map(|(g, c)| (g, c.0)).collect();
        // The game's strand tables (colour and parameter ramps and sets, hair LOD presets) are read from the
        // game's own files (`hairtables`, cached in the index folder) when the hair is built. Overrides for
        // research: SIXNG_HAIR_DATA=<json> and SIXNG_HAIR_LOD=<json> take parsed tables from files;
        // SIXNG_HAIR_COLOURSET=<name> forces one colour set; SIXNG_HAIR_PARAMSETS=hair=<set>,brows=<set>
        // names the parameter set per slot over the one the hair asset names (`hairsets`; for a piece of no
        // known asset: `DEFAULT_PARAMSETS`); SIXNG_FUR_SETS=<colour set>,<parameter set> gives an animal's strands those sets.
        let joaat = sixng_core::meta7a::joaat;
        let env_data: Option<serde_json::Value> = std::env::var("SIXNG_HAIR_DATA").ok().and_then(|f| serde_json::from_slice(&std::fs::read(f).ok()?).ok());
        // SIXNG_HAIR_SUM=1 lights every strand by the hair lighting, whatever its set's UseStandardBsdf says
        let force_hair_sum = std::env::var("SIXNG_HAIR_SUM").is_ok();
        let force_colour: Option<u32> = std::env::var("SIXNG_HAIR_COLOURSET").ok().map(|n| joaat(&n.to_ascii_lowercase()));
        let env_params: Option<HashMap<String, u32>> = std::env::var("SIXNG_HAIR_PARAMSETS").ok().map(|s| s.split(',').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), joaat(&v.to_ascii_lowercase()))).collect());
        let default_params: HashMap<String, u32> = DEFAULT_PARAMSETS.iter().map(|(k, v)| (k.to_string(), joaat(&v.to_ascii_lowercase()))).collect();
        // the parameter set of a piece: the forced one, else its asset's, else the slot's default
        let param_of = move |t: &HairTone| -> Option<u32> {
            match &env_params {
                Some(m) => m.get(&t.slot).copied(),
                None => (t.param != 0).then_some(t.param).or_else(|| default_params.get(&t.slot).copied()),
            }
        };
        // SIXNG_HAIR_LOD=<json of the parsed hair LOD presets> applies the game's strand count rule to a character's pieces
        // (head hair by the preset Default: whether its items name another one is not traced; brows, lashes, beard and
        // body hair by the presets of those names) and to an animal's fur given with SIXNG_FUR_SETS (the preset Animal)
        let env_lod: Option<serde_json::Value> = std::env::var("SIXNG_HAIR_LOD").ok().and_then(|f| serde_json::from_slice(&std::fs::read(f).ok()?).ok());
        let fur_sets: Option<(u32, u32)> = std::env::var("SIXNG_FUR_SETS").ok().and_then(|s| s.split_once(',').map(|(c, p)| (joaat(&c.to_ascii_lowercase()), joaat(&p.to_ascii_lowercase()))));
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = (|| -> anyhow::Result<Vec<glb::Model>> {
                let (a, ix) = index_for(&ws, "packs", &idx)?;
                std::fs::create_dir_all(&dir)?;
                // the game's tables (unless given by file); without them the hair keeps its stored preset colours
                let game_tables = if env_data.is_some() && env_lod.is_some() { None } else {
                    match sixng_core::hairtables::load_or_build(&ws, &a) {
                        Ok(v) => Some(v),
                        Err(e) => { eprintln!("[hair] strand tables not read: {e:#}"); None }
                    }
                };
                let strand_data: Option<Arc<sixng_core::hairtone::StrandData>> = env_data.as_ref().or(game_tables.as_ref()).map(|v| Arc::new(sixng_core::hairtone::StrandData::from_json(v)));
                let lod_presets: Option<serde_json::Value> = env_lod.clone().or_else(|| game_tables.as_ref().map(|v| v["lod_presets"].clone()));
                if std::env::var("SIXNG_DEBUG_HAIR").is_ok() { eprintln!("strand tables: {:?}, LOD presets {}", strand_data.as_ref().map(|d| d.counts()), lod_presets.as_ref().and_then(|v| v.as_object()).map_or(0, |m| m.len())); }
                // research: SIXNG_NO_STRANDS=1 draws no strand pieces (to see the skin under them)
                let grooms: Vec<(u64, sixng_core::groom::Groom)> = sel.iter().filter(|_| std::env::var("SIXNG_NO_STRANDS").is_err()).map(|&off| sixng_core::groom::load(&a, &ix.entries, off).map(|g| (off, g))).collect::<anyhow::Result<_>>()?;
                let stored: usize = grooms.iter().map(|g| g.1.stored_strands()).sum();
                let share = density * (COAT_STRANDS as f32 / stored.max(1) as f32).min(1.0);
                let mut drawn = 0;
                let mut out = Vec::new();
                for (off, mut g) in grooms {
                    // the guides as the game's strand simulation settles them on a still character (gravity,
                    // rest lengths, the pull back to the stored shape, the body's colliders); research:
                    // SIXNG_HAIR_NO_SIM=1 keeps the stored guides
                    if coat_on && std::env::var_os("SIXNG_HAIR_NO_SIM").is_none() {
                        // scalp hair under a hat stays inside its crown
                        let crown = hat.as_ref().filter(|_| under_hat.contains(&off)).and_then(|h| h.crown);
                        let settled = g.settle_in(collide.as_deref(), crown.as_ref());
                        if std::env::var("SIXNG_DEBUG_HAIR").is_ok() { eprintln!("coat {off:x}: settled {settled}"); }
                    }
                    // scalp hair stays under the hat (3 mm inside its lining)
                    if let Some(h) = hat.as_ref().filter(|_| under_hat.contains(&off)) {
                        h.press_strands(&mut g.points, g.points_per_strand, 0.003);
                    }
                    // linear colour from the sRGB picker value
                    let lin = own.get(&off).copied().unwrap_or(rgb).map(|c| c.powf(2.2));
                    let p = dir.join(format!("{off:x}.glb"));
                    let nearest = |r: [f32; 3], skin: &[([f32; 3], [u8; 3])]| {
                        let d = |p: &[f32; 3]| (p[0] - r[0]).powi(2) + (p[1] - r[1]).powi(2) + (p[2] - r[2]).powi(2);
                        skin.iter().min_by(|a, b| d(&a.0).total_cmp(&d(&b.0))).map_or([255; 3], |x| x.1)
                    };
                    let mut lod: Option<glb::StrandLod> = None;
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
                        let share = share.min(750_000.0 / (g.stored_strands() * k).max(1) as f32);
                        let mut coat = g.coat_with(share, collide.as_deref());
                        // noise, curl and clumping move grown strands off the
                        // pressed guides: press them again
                        if let Some(h) = hat.as_ref().filter(|_| under_hat.contains(&off)) {
                            h.press_strands(&mut coat.points, k, 0.003);
                        }
                        drawn += coat.guides.len();
                        // fewer strands than stored: wider ones, for the same cover
                        let base = if g.width > 0.0 { g.width } else { 0.0003 };
                        let w = (base / share.max(1e-3)).clamp(base, 0.004) * width_scale;
                        let var = |s: usize| (coat.variants.get(s).copied().unwrap_or(0) & 3) as usize;
                        // with the game's tables the strand keeps its own width (Thickness x curve): the game's divisor on it
                        // reads as 1 in the normal state (its per-instance scale is not fully read), and drawing fewer
                        // strands does not widen the rest
                        let share_widen = 1.0f32;
                        if std::env::var("SIXNG_DEBUG_HAIR").is_ok() { let mut h = [0usize; 4]; for s in 0..coat.guides.len() { h[var(s)] += 1; } let mut st: std::collections::BTreeMap<[u8; 4], usize> = Default::default(); for p in &g.pick_steps { *st.entry(*p).or_default() += 1; } eprintln!("coat {off:x}: stored {} k {} share {share:.3} width {:.2} mm (base {:.2} mm) tint weight {:.3} variants {h:?} thresholds {:?}", g.stored_strands(), k, w * 1000.0, base * 1000.0, g.tint_weight, st.iter().take(6).collect::<Vec<_>>()); eprintln!("  head +0x20 {:x} +0x24 {:x} clump {} styles {:?}", g.head.get(0x20..0x24).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap())), g.head.get(0x24..0x28).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap())), g.clump_type, g.styles.iter().map(|s| (s.lo, s.hi, s.samples.len())).collect::<Vec<_>>()); }
                        // the atlas colour is stored sRGB-encoded: the linear base colour is its sRGB decode
                        let lin8 = |c: [f32; 3], m: [u8; 3]| -> [u8; 4] { [0, 1, 2].map(|q| ({ let x = c[q].clamp(0.0, 1.0); if x < 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) } } * m[q] as f32 + 0.5) as u8).iter().copied().chain([255u8]).collect::<Vec<u8>>().try_into().unwrap() };
                        let blend = |s: usize| -> [u8; 3] {
                            let (gd, wt) = coat.guides[s];
                            let at = |i: u32| cols.get(i as usize).copied().unwrap_or([255; 3]);
                            let (a, b, d) = (at(gd[0]), at(gd[1]), at(gd[2]));
                            [0, 1, 2].map(|q| (a[q] as f32 * wt[0] + b[q] as f32 * wt[1] + d[q] as f32 * wt[2] + 0.5) as u8)
                        };
                        let name = format!("coat_{off:x}");
                        lod = None;
                        match (tones.get(&off), strand_data.as_deref()) {
                            // a character's hair with the game's tables: the colour set its item names
                            // (or the forced one) and the parameter set named for its slot
                            (Some(t), Some(d)) if d.colour_set(force_colour.unwrap_or(t.set)).is_some() => {
                                let cs = d.colour_set(force_colour.unwrap_or(t.set)).unwrap();
                                let pn = param_of(t);
                                let ps = pn.and_then(|n| d.param_set(n));
                                // the piece type, by the game's hair LOD presets (TargetThickness 1.5 hair and fur, 0.5 brows,
                                // lashes, beard, 0.25 body hair): it sets how many strands the game draws at a distance; not used yet
                                let thick = match t.slot.as_str() { "brows" | "lashes" | "beard" => 1.0, "bodyhair" => 2.0, _ => 0.0 };
                                let lit = pn.and_then(|n| d.param_flags(n)).filter(|_| !force_hair_sum).map_or(1.0, |(std, fs)| if std { 2.0 + fs } else { 1.0 });
                                if std::env::var("SIXNG_DEBUG_HAIR").is_ok() { eprintln!("coat {off:x}: slot {:?} lighting flag {lit} (sets flags {:?}, forced hair sum {force_hair_sum})", t.slot, pn.map(|n| d.param_flags(n))); }
                                if let (Some(ps), Some(lp)) = (ps, lod_presets.as_ref()) {
                                    lod = strand_lod(&lp[match t.slot.as_str() { "brows" => "Brows", "lashes" => "Lashes", "beard" => "Beard", "body hair" => "BodyHair", _ => "Default" }], &ps[0], &coat);
                                }
                                let pf = |s: usize, f: f32| { let r = &ps.unwrap()[var(s)]; let v = r.at(f); [v[0], v[1], v[2], r.width * v[3].max(0.05) * share_widen, lit, thick] };
                                g.coat_glb_ramped(&name, &coat, w, &|s, f| lin8(cs[var(s)].at(f), [255; 3]), ps.map(|_| &pf as &dyn Fn(usize, f32) -> [f32; 6]), serde_json::json!({}))
                            }
                            // a character's hair without them: the appearance preset of each strand's variant
                            (Some(t), _) => g.coat_glb_with(&name, &coat, w, &|s, f| t.rgb(coat.variants.get(s).copied().unwrap_or(0), f), serde_json::json!({})),
                            // an animal's strands with the game's tables: the ramp of the variant times
                            // the colour of the skin under the root. The game's strand resolve does this: colour = mix(C, C x T, w), T read
                            // at the strand's root UV from the first texture of one of the ped's own components (with that
                            // component's palette rows when it has them): wiring read from the game's code, and the animal's fur items name
                            // that body component (item tag 2f9afe84 = the component's category). Here the body's displayed
                            // (already tinted) colour stands in for texture and palette; the coat variant's own rows are not
                            // selected for the fur. w is the groom's stored tint weight (the texture's alpha taken as 1).
                            (None, Some(d)) if fur_sets.is_some_and(|f| d.colour_set(f.0).is_some() || d.param_set(f.1).is_some()) => {
                                let (cn, pn) = fur_sets.unwrap();
                                let (cs, ps) = (d.colour_set(cn), d.param_set(pn));
                                let thick = 0.0;
                                // an animal's fur: the preset Animal
                                if let (Some(ps), Some(lp)) = (ps, lod_presets.as_ref()) {
                                    lod = strand_lod(&lp["Animal"], &ps[0], &coat);
                                }
                                let lit = d.param_flags(pn).filter(|_| !force_hair_sum).map_or(1.0, |(std, fs)| if std { 2.0 + fs } else { 1.0 });
                                let pf = |s: usize, f: f32| { let r = &ps.unwrap()[var(s)]; let v = r.at(f); [v[0], v[1], v[2], r.width * v[3].max(0.05) * share_widen, lit, thick] };
                                g.coat_glb_ramped(&name, &coat, w, &|s, f| match cs { Some(cs) => lin8(cs[var(s)].at(f), blend(s).map(|m| (m as f32 * g.tint_weight + 255.0 * (1.0 - g.tint_weight) + 0.5) as u8)), None => { let c = blend(s); [c[0], c[1], c[2], 255] } }, ps.map(|_| &pf as &dyn Fn(usize, f32) -> [f32; 6]), serde_json::json!({}))
                            }
                            (None, _) => g.coat_glb(&name, &coat, w, &cols, serde_json::json!({})),
                        }
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
                    sixng_core::atomic::write(&p, glb_bytes)?;
                    let mut gm = glb::load(&p, &dir)?;
                    if let Some(mut l) = lod.filter(|l| gm.meshes.len() == 1 && l.strands > 0 && gm.meshes[0].indices.len() as u32 % l.strands == 0) {
                        // the piece's extent: the largest half-size of its box
                        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                        for v in &gm.meshes[0].verts {
                            for q in 0..3 {
                                lo[q] = lo[q].min(v.pos[q]);
                                hi[q] = hi[q].max(v.pos[q]);
                            }
                        }
                        l.radius = 0.5 * (0..3).map(|q| (hi[q] - lo[q]).max(0.0)).fold(0.0, f32::max);
                        if std::env::var("SIXNG_DEBUG_HAIR").is_ok() {
                            eprintln!("coat {off:x}: count rule version {} t {:.2e} m extent {:.2} m", l.version, l.t, l.radius);
                        }
                        gm.meshes[0].strand_lod = Some(l);
                    }
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
        let (shown, cache): (Vec<u64>, PathBuf) = (self.loaded.as_ref().map_or_else(Vec::new, |l| l.selected.clone()), self.cache_dir());
        // a folder per model, so an earlier `sixng grooms` export and its index stay as they are
        let model: String = self.loaded.as_ref().map_or("grooms", |l| l.source.split_whitespace().next().unwrap_or("").rsplit([':', '#', '/']).find(|s| !s.is_empty() && !s.starts_with("look=")).unwrap_or("grooms")).chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
        let out = Path::new(&self.work).join("out").join("hair").join(model);
        std::thread::spawn(move || {
            let r = (|| -> anyhow::Result<String> {
                let (a, ix) = index_for(&ws, "packs", &idx)?;
                // the shown character's head as the viewer exported it, to name the
                // head UV set the root UVs are in (`hairexport::tag_uv_set`)
                let head = shown
                    .iter()
                    .filter_map(|o| ix.entries.iter().find(|e| e.off == *o))
                    .filter(|e| e.name.as_deref().is_some_and(sixng_core::headskin::is_head))
                    .find_map(|e| std::fs::read(cache.join("models").join(sixng_core::game::index_stem(&a.id)).join(export::export_base_name(e)).join("model.glb")).ok())
                    .and_then(|b| sixng_core::hairexport::head_from_glb(&b));
                let (written, errors) = sixng_core::groom::export_grooms(&a, &ix.entries, &sel, &out, false, None, full, head.as_ref(), sixng_core::export::no_progress())?;
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
            ui.label(egui::RichText::new("Strand grooms from packs.rpf. Characters get their hair and animals their fur automatically, read from the game's own link (where the archive's table of contents cannot be read, worked out from the order of the files and a fit to the skin instead); other grooms can be picked here. Shown as ribbons; width and colour are display choices.").weak());
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
                                let label = format!("{} · {} piece(s){}", short(&v.name), v.grooms.len(), if v.known || l.read { "" } else { " · count inferred" });
                                changed |= ui.selectable_value(&mut self.hair.fur_pick[k], v.name.clone(), label).changed();
                            }
                        });
                    });
                }
                if changed {
                    self.hair.apply_fur();
                }
                let with = if l.read {
                    String::new()
                } else if l.roots.len() > 1 { format!("; walked together with {}", l.roots.iter().filter(|r| **r != l.root).cloned().collect::<Vec<_>>().join(", ")) } else { String::new() };
                let text = if l.read {
                    format!("{} groom pieces, read from the game (each variant lists its pieces).", l.block)
                } else {
                    format!("{} of {} groom pieces in this run fit the skin; {:.0} % of the pieces with a known count sit on their own slot{with}. Inferred: the game's own link could not be read here.", l.fitting, l.block, l.agreement * 100.0)
                };
                ui.label(egui::RichText::new(text).weak().small());
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
                    ui.checkbox(&mut self.hair.export_full, "full strands").on_hover_text("Write the strands the groom stores per root triangle, grown between its guides, instead of the guide strands alone, with clumping, curl and noise as the viewer draws them (same as `sixng grooms --full`; the density slider below sets the share). Each groom names the head UV set its root UVs are in, for scripts/blender_hair_curves.py.");
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
                if !self.hair.tones.is_empty() && !self.hair.pieces.is_empty() {
                    let mut all = self.hair.pieces.iter().filter(|p| p.link.is_none()).all(|p| p.stored);
                    if ui.checkbox(&mut all, "stored hair colours").on_hover_text("Each piece takes the colours of the hair preset its item names. Per piece: Hair colours in the Character panel.").changed() {
                        for p in &mut self.hair.pieces {
                            p.stored = all;
                        }
                    }
                } else if !self.hair.tones.is_empty() {
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
        if self.parts_request.is_some() || self.fit_parts_once.is_some() || self.random_parts_once || !self.hair.settled() || self.auto.then.is_some() || self.auto.find_hair || self.auto.export_hair || self.hair.fitting || self.ped_clip.loading || self.clip_picker.busy() || (!self.ped_clip.want.trim().is_empty() && self.ped_clip.anim.is_none() && self.ped_clip.status.starts_with("loading")) {
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
                        sixng_core::anim::default_clip(&d).ok_or_else(|| anyhow::anyhow!("{} holds no clip with animation data that is decoded (its {} clip(s) bind none here)", d.name, d.clips.len()))?
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

    /// Body shape for characters, before a model is opened (`--body`:
    /// outfits keep it instead of their own preset).
    pub fn set_body_shape(&mut self, shape: BodyShape) {
        BODY_PINNED.store(true, std::sync::atomic::Ordering::Relaxed);
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
        let fit = sixng_core::anim::ClipFit::Animal(l.source.clone());
        egui::CollapsingHeader::new("Animation").default_open(true).show(ui, |ui| {
            if self.clip_picker.ui(ui, &mut self.ped_clip.want, fit, &self.game, &self.work) {
                self.ped_clip.playing = !self.ped_clip.want.trim().is_empty();
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
        if let Some(k) = look.parts.as_deref().and_then(|set| crate::ped_parts::ui(ui, set, look.current)) {
            let base = l.base.split("#look=").next().unwrap_or(&l.base);
            reload = Some(format!("{base}#look={k}"));
        }
        let mut shape = *BODY_SHAPE.lock().unwrap();
        let before = shape;
        let mut released = false;
        // the overall sliders show the mean and set every region
        let mean = |s: &[f32]| s.iter().sum::<f32>() / s.len() as f32;
        let (mut weight, mut muscle) = (mean(&shape[..12]), mean(&shape[12..24]));
        let (w0, m0) = (weight, muscle);
        ui.horizontal(|ui| {
            ui.label("Body");
            released |= ui.add(egui::Slider::new(&mut weight, -1.0..=1.0).text("weight")).on_hover_text("The game's body-shape rig on the shared skeleton (sets all 12 weight regions). Body and clothes follow the same bones, so the clothes change with the body.").drag_stopped();
        });
        ui.horizontal(|ui| {
            ui.label("        ");
            released |= ui.add(egui::Slider::new(&mut muscle, 0.0..=1.0).text("muscle")).on_hover_text("Sets all 12 muscle regions.").drag_stopped();
        });
        if weight != w0 {
            shape[..12].fill(weight);
        }
        if muscle != m0 {
            shape[12..24].fill(muscle);
        }
        ui.horizontal(|ui| {
            ui.label("        ");
            released |= ui.add(egui::Slider::new(&mut shape[BODY_BREASTS], -1.0..=1.0).text("breasts")).on_hover_text("The game's gcbreasts control: -1 smaller, 1 larger (breast and pectoral bones).").drag_stopped();
        });
        ui.horizontal(|ui| {
            ui.label("        ");
            released |= ui.add(egui::Slider::new(&mut shape[BODY_BUTT], 0.0..=1.0).text("butt")).on_hover_text("The game's gcbutt control: 0 neutral, 1 fuller.").drag_stopped();
        });
        egui::CollapsingHeader::new("Regions").id_salt("body_regions").show(ui, |ui| {
            egui::Grid::new("body_region_grid").num_columns(3).show(ui, |ui| {
                for k in 0..12 {
                    ui.label(sixng_core::bodyshape::CONTROLS[k].0.trim_start_matches("gcweight"));
                    released |= ui.add(egui::Slider::new(&mut shape[k], -1.0..=1.0).text("weight")).drag_stopped();
                    released |= ui.add(egui::Slider::new(&mut shape[12 + k], 0.0..=1.0).text("muscle")).drag_stopped();
                    ui.end_row();
                }
            });
        });
        egui::CollapsingHeader::new("Shape details").id_salt("body_details").show(ui, |ui| {
            egui::Grid::new("body_detail_grid").num_columns(2).show(ui, |ui| {
                for k in BODY_DETAILS..shape.len() {
                    let name = sixng_core::bodyshape::CONTROLS[k].0;
                    let lo = sixng_core::bodyshape::control_min(name);
                    let more = if name == "gcheadscale" { " The rig scales the head by 1 + v (x 2 at 1); below -0.5 the head shrinks out of sight (1 % at -1), so the slider stops at half size." } else { "" };
                    ui.label(detail_label(name)).on_hover_text(format!("The game's {name} control ({lo}..1).{more}"));
                    released |= ui.add(egui::Slider::new(&mut shape[k], lo..=1.0)).drag_stopped();
                    ui.end_row();
                }
            });
        });
        ui.horizontal(|ui| {
            let preset = look.body_preset;
            if ui.add_enabled(preset.is_some(), egui::Button::new("Reset")).on_hover_text("Back to the outfit's own body-shape preset.").clicked() {
                shape = preset.unwrap_or(NEUTRAL_BODY);
                released = true;
            }
            if ui.button("Neutral").on_hover_text("Every control 0 (the game's REGULAR shape).").clicked() {
                shape = NEUTRAL_BODY;
                released = true;
            }
        });
        if let Some(p) = &look.body_shape {
            let how = if look.body_preset.is_some() { "the sliders open in its values" } else { "its values could not be read (anim.rpf and common.rpf need to be indexed); the sliders open neutral" };
            ui.label(egui::RichText::new(format!("Body-shape preset of this outfit: {p} ({how}).")).weak().small());
        }
        ui.checkbox(&mut self.look.skin_detail, "skin detail").on_hover_text("The engine's skin detail maps (pores, fine wrinkles) on the bare skin, as the game draws them up close. Turn it off to see whether a pattern on the skin comes from them. Also --skin-detail off or SIXNG_SKIN_DETAIL=0.");
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
            if self.clip_picker.ui(ui, &mut self.ped_clip.want, sixng_core::anim::ClipFit::Person, &self.game, &self.work) {
                self.ped_clip.playing = !self.ped_clip.want.trim().is_empty();
            }
            if ui.button("Walk").on_hover_text("A walk clip of the game for this body type").clicked() {
                self.ped_clip.want = format!("gameplay@locomotion@{}_character@{}@normal@standing@walk@stance_neutral/move", if female { "female" } else { "male" }, if female { "kat" } else { "raul" });
                self.ped_clip.playing = true;
                self.ped_pose = PedPose::default();
            }
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
            ui.label(egui::RichText::new("A clip of anim.rpf on this skeleton: pick a dictionary (type words of its name to narrow the list), then a clip in it; greyed clips store their animation data elsewhere and do not play. ✏ takes a typed or pasted name, also a cutscene: cin@<scene> plays this character's clip of that scene. The Pose angles are added on top.").weak().small());
        });
        for n in &look.notes {
            ui.label(egui::RichText::new(n).weak().small());
        }
        if !look.hair.is_empty() {
            ui.label("Hair");
            for h in &look.hair {
                let mut on = !h.grooms.is_empty() && h.grooms.iter().all(|g| self.hair.selected.contains(g));
                let name = h.placeholder.trim_end_matches("_full");
                let mut text = if h.exact { format!("{}: {name}", h.slot) } else { format!("{}: {name} ({:.0} %)", h.slot, h.confidence * 100.0) };
                if h.off_head > 0 {
                    text += &format!(", {} piece(s) not on this head left out", h.off_head);
                }
                if h.colour.is_some() {
                    text += ", stored colour";
                }
                if ui.checkbox(&mut on, text).on_hover_text(if h.exact { "The strand grooms of this hair asset, read from the game: the asset lists its pieces by the ids of their files." } else { "The strand grooms placed for this hair asset. The game's own link could not be read here (it needs the archive's table of contents), so they are placed by name words, archive order, piece counts and where the strand roots lie. The percentage is that model's probability of this placement. Shown at the start: 50 % or more, or 20 % or more when every piece is rooted on this head (a probable style, not a certain one). Pieces whose strand roots do not lie on this head are left out." }).changed() {
                    self.hair.selected.retain(|g| !h.grooms.contains(g));
                    if on {
                        self.hair.selected.extend(h.grooms.iter().copied());
                    }
                }
            }
        }
        let unplaced: Vec<String> = look.strands.iter().filter(|s| !look.hair.iter().any(|h| h.slot == s.0)).map(|s| s.1.clone()).collect();
        let swatches = (!look.hair.is_empty()).then(|| look.hair_swatches.clone());
        if let Some(sw) = swatches {
            self.hair_pieces_ui(ui, &sw);
        }
        if !unplaced.is_empty() {
            ui.label(egui::RichText::new(format!("Strand hair with no grooms placed: {}", unplaced.join(", "))).weak().small());
        }
        ui.separator();
        reload
    }

    /// A character's hair colours: one row per piece (hair, brows, lashes,
    /// beard ...), each with its own colour, its stored colours, or the same
    /// colour as another piece; the palette swatches paint the chosen piece.
    fn hair_pieces_ui(&mut self, ui: &mut egui::Ui, swatches: &[[u8; 3]]) {
        let n = self.hair.pieces.len();
        if n == 0 {
            return;
        }
        let has_tones = !self.hair.tones.is_empty();
        ui.label("Hair colours").on_hover_text("Each piece has its own colour, or the same colour as another piece. Click a piece's name to choose it for the palette swatches below.");
        egui::Grid::new("hair_pieces").num_columns(3).spacing([6.0, 2.0]).show(ui, |ui| {
            for i in 0..n {
                let slot = self.hair.pieces[i].slot.clone();
                if ui.selectable_label(self.hair.paint == i, &slot).on_hover_text("Paint this piece with the palette swatches").clicked() {
                    self.hair.paint = i;
                }
                let link = self.hair.pieces[i].link.clone();
                let targets: Vec<String> = self.hair.pieces.iter().enumerate().filter(|(j, p)| *j != i && p.link.is_none()).map(|(_, p)| p.slot.clone()).collect();
                let mut pick = link.clone();
                egui::ComboBox::from_id_salt(("hair_link", i))
                    .selected_text(link.as_ref().map_or("own colour".to_string(), |l| format!("same as {l}")))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut pick, None, "own colour");
                        for t in &targets {
                            ui.selectable_value(&mut pick, Some(t.clone()), format!("same as {t}"));
                        }
                    });
                if pick != link {
                    // keep what is drawn: a piece cut loose starts from the colour it showed
                    let r = self.hair.resolve(i);
                    let (c, st) = (self.hair.pieces[r].colour, self.hair.pieces[r].stored);
                    let p = &mut self.hair.pieces[i];
                    p.colour = c;
                    p.stored = st;
                    p.link = pick.clone();
                    // pieces that followed this one now follow its new target
                    if let Some(t) = pick {
                        for q in &mut self.hair.pieces {
                            if q.link.as_deref() == Some(slot.as_str()) {
                                q.link = Some(t.clone());
                            }
                        }
                    }
                }
                ui.horizontal(|ui| {
                    if self.hair.pieces[i].link.is_none() {
                        let p = &mut self.hair.pieces[i];
                        // a colour picked here is drawn: it turns the stored colours off
                        if ui.color_edit_button_rgb(&mut p.colour).changed() {
                            p.stored = false;
                        }
                        if has_tones {
                            ui.checkbox(&mut p.stored, "stored").on_hover_text("On: each groom of this piece (and of the pieces linked to it) takes the colours of the hair preset its item names. Off: the colour picked here.");
                        }
                    } else {
                        let r = self.hair.resolve(i);
                        let p = &self.hair.pieces[r];
                        let c = p.colour.map(|x| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
                        if p.stored {
                            ui.label(egui::RichText::new("stored").weak());
                        }
                    }
                });
                ui.end_row();
            }
        });
        ui.label(egui::RichText::new("(stored for items that name the hair palette; most leave it to the game, so pick one)").weak().small());
        if !swatches.is_empty() {
            let r = self.hair.resolve(self.hair.paint.min(n - 1));
            ui.label(egui::RichText::new(format!("palette: paints {}", self.hair.pieces[r].slot)).weak().small());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                for c in swatches.iter().take(64) {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
                    ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
                    if resp.on_hover_text("A colour of the game's hair palette (ped_tint_hair)").clicked() {
                        let p = &mut self.hair.pieces[r];
                        p.colour = c.map(|x| x as f32 / 255.0);
                        p.stored = false;
                    }
                }
            });
        }
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
                // the shape the outfit opened in (its preset): no reload for it
                if let Some(li) = &l.look {
                    self.body_shape_shown = li.body_used.unwrap_or(NEUTRAL_BODY);
                }
                let keep_cam = std::mem::take(&mut self.keep_cam_once)
                    || self
                        .loaded
                        .as_ref()
                        .map(|o| o.source == l.source)
                        .unwrap_or(false);
                let ren = self.renderer.get_or_insert_with(|| Renderer3D::new_viewer(rs));
                let mut g = ren.upload(rs, &l.model, &[]);
                ren.ensure_edges(rs, &mut g, &l.model);
                if !keep_cam {
                    self.cam.frame_bbox(g.bbox[0], g.bbox[1]);
                    // framed on the stored pose: the walked distance is added again
                    self.ped_clip.travel = Vec3::ZERO;
                    if let Some(z) = self.zoom_after_load {
                        self.cam.dist *= z;
                    }
                    // research: SIXNG_CAM_UP=<metres> raises the point the camera looks at (close-ups of a head)
                    if let Some(u) = std::env::var("SIXNG_CAM_UP").ok().and_then(|v| v.parse::<f32>().ok()) {
                        self.cam.target.y += u;
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
                    self.hair.collide = look.collide.clone();
                    self.hair.under_hat = look.hair.iter().filter(|h| h.slot == "hair").flat_map(|h| h.grooms.iter().copied()).collect();
                    // a character: its confident hair, grown to full density
                    let key = format!("{}#{}", look.model, look.current);
                    if self.hair.look_key != key {
                        self.hair.look_key = key;
                        let want = self.auto.hair_style.clone();
                        self.hair.selected = look.hair.iter().filter(|h| h.shown || want.as_ref().is_some_and(|w| h.placeholder.contains(w.as_str()))).flat_map(|h| h.grooms.iter().copied()).collect();
                        if let Some(c) = self.auto.hair_colour {
                            self.hair.colour = c;
                            self.hair.stored_colours = false;
                        } else if let Some(c) = look.hair.iter().find(|h| h.slot == "hair").or(look.hair.first()).and_then(|h| h.colour) {
                            self.hair.colour = c.map(|x| x as f32 / 255.0);
                        }
                        self.hair.tones = look.hair.iter().filter_map(|h| Some((h, h.tone.as_ref()?))).flat_map(|(h, t)| h.grooms.iter().map(move |g| (*g, t.clone()))).collect();
                        // one piece per slot: hair has its own colour, the rest start linked to it
                        self.hair.pieces.clear();
                        self.hair.slot_of.clear();
                        self.hair.paint = 0;
                        for h in &look.hair {
                            let slot = piece_slot(&h.slot).to_string();
                            if !self.hair.pieces.iter().any(|p| p.slot == slot) {
                                let colour = look.hair.iter().filter(|x| piece_slot(&x.slot) == slot).find_map(|x| x.colour).map_or(self.hair.colour, |c| c.map(|x| x as f32 / 255.0));
                                self.hair.pieces.push(HairPiece { slot: slot.clone(), link: None, colour, stored: self.auto.hair_colour.is_none() });
                            }
                            for g in &h.grooms {
                                self.hair.slot_of.insert(*g, slot.clone());
                            }
                        }
                        self.hair.pieces.sort_by_key(|p| p.slot != "hair");
                        if let Some(first) = self.hair.pieces.first().map(|p| p.slot.clone()) {
                            for p in self.hair.pieces.iter_mut().skip(1) {
                                p.link = Some(first.clone());
                            }
                        }
                        if let Some(c) = self.auto.hair_colour {
                            for p in &mut self.hair.pieces {
                                p.colour = c;
                            }
                        }
                        for (slot, c) in &self.auto.hair_pieces {
                            if let Some(p) = self.hair.pieces.iter_mut().find(|p| p.slot == *slot) {
                                p.link = None;
                                p.colour = *c;
                                p.stored = false;
                            }
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
                    self.hair.pieces.clear();
                    self.hair.slot_of.clear();
                    self.hair.hat = None;
                    self.hair.collide = None;
                    if self.hair.fur.is_none() {
                        self.hair.selected.clear();
                    }
                }
                // the morph targets of the new meshes, all at 0 (as uploaded)
                self.expr.clear();
                for me in &l.model.meshes {
                    for (n, _) in &me.morphs {
                        if !self.expr.iter().any(|e| &e.0 == n) {
                            self.expr.push((n.clone(), 0.0));
                        }
                    }
                }
                self.expr_shown = vec![0.0; self.expr.len()];
                for (n, w) in &self.expr_auto {
                    if let Some(e) = self.expr.iter_mut().find(|e| &e.0 == n) {
                        e.1 = *w;
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
            let ren = self.renderer.get_or_insert_with(|| Renderer3D::new_viewer(rs));
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
        // each part id's wear amount under this configuration
        let wear_key = self.part_look_want.clone().unwrap_or_default();
        let part_wears = part_wear(&pc["wear_records"], idx as usize, self.part_look_want.as_deref());
        self.look.wear_levels = part_wears.is_some();
        let primer_surface = primer_surface(&pc["wear_records"]["global"]);
        // the coat of this look's body paint (clear coat, flakes)
        let coat = &pc["coat"];
        let record = coat["paint_by_look"][idx].as_u64().map(|k| &coat["records"][k as usize]).filter(|r| r.is_object());
        (self.look.coat, self.look.coat_base) = record.map_or(([0.0; 4], None), paint_coat);
        // What the fitted parts change in the look (a body kit's part
        // rebinds the ids of its panels, mostly to the body paint): part id
        // and its new entry.
        let mut patched: Vec<(usize, [u8; 4])> = Vec::new();
        // and the surface (low and high metalness and roughness) of those ids
        let mut patched_surf: Vec<(usize, [u8; 4])> = Vec::new();
        for part in l.info["fitted_parts"].as_array().into_iter().flatten().filter_map(|p| p.as_str()) {
            let d = &pc["part_patches"][part];
            let rgba = d["rgba"].as_str().unwrap_or("");
            let surf = d["surfaces"].as_str().unwrap_or("");
            for (k, id) in d["ids"].as_array().into_iter().flatten().filter_map(|x| x.as_u64()).enumerate() {
                if let Some(h) = rgba.get(k * 8..k * 8 + 8) {
                    patched.push((id as usize & 255, std::array::from_fn(|c| u8::from_str_radix(&h[c * 2..c * 2 + 2], 16).unwrap_or(255))));
                }
                if let Some(s) = surf.get(k * 8..k * 8 + 8).and_then(hex4) {
                    patched_surf.push((id as usize & 255, s));
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
        let ramp_key = (l.model.path.clone(), idx, wear_key, self.look.primer_set);
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
                    // each part id's second-layer colour (sRGB), which the
                    // game's shader uses as the first wear layer's tint
                    let wear_tint = pc["wear_tint"][idx].as_str().filter(|w| w.len() == 256 * 6);
                    // each part id's colour step amounts (sun bleach screen,
                    // albedo multiply, albedo screen of both layers)
                    let tone = pc["surface_tone"][idx].as_str().filter(|w| w.len() == 256 * 12);
                    // rows 512..: the low and high of the first layer's
                    // metalness and roughness, which the shader blends by the
                    // material map at each pixel
                    let surfaces = pc["surfaces"][idx].as_str().filter(|s| s.len() == 2048);
                    // rows 1025..1281: each part id's primer palette row
                    let primer_rows = pc["primer"][idx].as_str().filter(|s| s.len() == 256 * 12);
                    let primer_sets = primer_set_masks(&pc["wear_records"], self.part_look_want.as_deref());
                    let mut px = vec![255u8; 128 * PART_RAMP_ROWS * 4];
                    // rows 768..: each part id's coat record, and in row
                    // 1024 the body paint's
                    let coat = &pc["coat"];
                    let coats = coat["by_look"][idx].as_str().filter(|s| s.len() == 512);
                    let record = |k: usize| Some(&coat["records"][k]).filter(|r| r.is_object());
                    let paint = coat["paint_by_look"][idx].as_u64().and_then(|k| record(k as usize));
                    px[1024 * 512..1025 * 512].copy_from_slice(&coat_row(paint));
                    for id in 0..256 {
                        let own = coats.and_then(|s| usize::from_str_radix(&s[id * 2..id * 2 + 2], 16).ok()).and_then(record);
                        px[(768 + id) * 512..(769 + id) * 512].copy_from_slice(&coat_row(own));
                        // column 11 of the coat row: the part id's wear amount
                        let pw = part_wears.as_ref().map_or(PartWear::default(), |w| w[id]);
                        px[(768 + id) * 512 + 11 * 4 + 3] = pw.amount;
                        // columns 16 to 24 (read by textureLoad, not
                        // repeated): per wear mask channel the scale, the
                        // window start and the window width
                        for (c, v) in pw.scale.iter().chain(&pw.start).chain(&pw.width).enumerate() {
                            px[(768 + id) * 512 + (16 + c) * 4 + 3] = *v;
                        }
                        // columns 25 to 33: the primer's scale per channel,
                        // cross byte, window start and width, mask switch,
                        // and the library's primer metalness low and high
                        // (roughness low and high in columns 48, 49)
                        let primer = [pw.primer[0], pw.primer[1], pw.primer[2], pw.primer_cross, pw.primer_start, pw.primer_width, if pw.primer_on_wear_mask { 255 } else { 0 }, primer_surface[0], primer_surface[1]];
                        px[(768 + id) * 512 + 48 * 4 + 3] = primer_surface[2];
                        px[(768 + id) * 512 + 49 * 4 + 3] = primer_surface[3];
                        for (c, v) in primer.iter().enumerate() {
                            px[(768 + id) * 512 + (25 + c) * 4 + 3] = *v;
                        }
                        // columns 34 to 45: the bias and gamma of the first
                        // layer, then of the primer, per channel
                        for (c, v) in pw.bias.iter().chain(&pw.gamma).chain(&pw.primer_bias).chain(&pw.primer_gamma).enumerate() {
                            px[(768 + id) * 512 + (34 + c) * 4 + 3] = *v;
                        }
                        // columns 46 and 47: the primer's palette column
                        // window (primerGradientStart, width)
                        px[(768 + id) * 512 + 46 * 4 + 3] = pw.primer_gradient_start;
                        px[(768 + id) * 512 + 47 * 4 + 3] = pw.primer_gradient_width;
                        // columns 50 to 64: W2's window starts, widths and
                        // divisors per channel R, G, B, D4, then the 4th
                        // channel's scale and its window in the first layer;
                        // 65 to 67: the first layer's tint (linear, from
                        // the second layer's colour record), 68: 255 when
                        // the part has that tint and a palette row (else
                        // W2 is 0)
                        let tint = wear_tint.and_then(|t| hex3(&t[id * 6..id * 6 + 6]));
                        let lin = |v: u8| {
                            let c = v as f32 / 255.0;
                            ((if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }) * 255.0 + 0.5) as u8
                        };
                        let has_row = wear.and_then(|w| usize::from_str_radix(&w[id * 8..id * 8 + 4], 16).ok()).is_some_and(|r| r != 0xffff);
                        let t = tint.unwrap_or([0; 3]);
                        // 69 to 72: the edge record's intensity, start and
                        // end, and the clear-coat loss byte
                        let extra = pw.w2_start.iter().chain(&pw.w2_width).chain(&pw.w2_div).copied().chain([pw.d4_scale, pw.d4_start, pw.d4_width, lin(t[0]), lin(t[1]), lin(t[2]), if tint.is_some() && has_row { 255 } else { 0 }, pw.edge_intensity, pw.edge_start, pw.edge_end, pw.coat_loss]);
                        for (c, v) in extra.enumerate() {
                            px[(768 + id) * 512 + (50 + c) * 4 + 3] = v;
                        }
                        // columns 73 to 88: the dust and scratch mask's
                        // bytes (PartWear::dust)
                        for (c, v) in pw.dust.iter().enumerate() {
                            px[(768 + id) * 512 + (73 + c) * 4 + 3] = *v;
                        }
                        // columns 89 to 94: the colour step amounts of the
                        // surface's first layer, then of its second (sun
                        // bleach screen, albedo multiply, albedo screen);
                        // 95: 255 when the part's own colour is a palette row
                        let t6 = tone.and_then(|t| (0..6).map(|k| u8::from_str_radix(&t[id * 12 + k * 2..id * 12 + k * 2 + 2], 16).ok()).collect::<Option<Vec<u8>>>()).unwrap_or(vec![0; 6]);
                        let pal_row = rows.get(id * 4..id * 4 + 4).is_some_and(|r| r != "ffff");
                        for (c, v) in t6.iter().copied().chain([if pal_row { 255 } else { 0 }]).enumerate() {
                            px[(768 + id) * 512 + (89 + c) * 4 + 3] = v;
                        }
                        let set = primer_set(primer_sets.as_ref().map_or(1, |m| m[id]), self.look.primer_set);
                        let prow = primer_rows.and_then(|s| usize::from_str_radix(&s[id * 12 + set * 4..id * 12 + set * 4 + 4], 16).ok()).filter(|r| *r != 0xffff);
                        let out3 = &mut px[(1025 + id) * 512..(1026 + id) * 512];
                        match prow.and_then(|r| pal.get(r * 512..r * 512 + 512)) {
                            Some(src) => out3.copy_from_slice(src),
                            None => out3.chunks_exact_mut(4).for_each(|c| c.copy_from_slice(&[PRIMER_GREY, PRIMER_GREY, PRIMER_GREY, 255])),
                        }
                        let stored = surfaces.and_then(|s| hex4(&s[id * 8..id * 8 + 8]));
                        let s = patched_surf.iter().rev().find(|p| p.0 == id).map(|p| p.1).or(stored).unwrap_or_else(|| surface_of_code(look[id][3]));
                        px[(512 + id) * 512..(513 + id) * 512].copy_from_slice(&part_surface_row(s));
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
                    let im = glb::Image { key: "part ramp".into(), mips: vec![(128, PART_RAMP_ROWS as u32, px)], srgb: true, bc: None };
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
                    // every part material: one without a palette column map
                    // still reads its parts' surfaces from the image
                    for m in l.model.materials.iter_mut().filter(|m| m.parts) {
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
        let ren = self.renderer.get_or_insert_with(|| Renderer3D::new_viewer(rs));
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
        // clicks land on the vehicle as it is drawn: doors open, a roof down
        let skin = if self.livery.ed.open { self.joint_mats().1 } else { Vec::new() };
        let Some(l) = &self.loaded else { return false };
        let env = crate::livery_ui::Env { game: &self.game, work: &self.work, cache: &cache, config: self.veh_cfg, skin: &skin };
        self.livery.pointer(ui, resp, rect, &self.cam, &l.model, &env)
    }

    /// Keep the loaded vehicle's stickers (custom liveries) on the GPU.
    fn sync_livery(&mut self, rs: &egui_wgpu::RenderState) {
        let cache = self.cache_dir();
        if !self.is_vehicle() {
            return;
        }
        let (Some(l), Some(g)) = (&self.loaded, self.gpu.as_mut()) else { return };
        let ren = self.renderer.get_or_insert_with(|| Renderer3D::new_viewer(rs));
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
            let ren = self.renderer.get_or_insert_with(|| Renderer3D::new_viewer(rs));
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
        if !self.weather.is_empty() && !self.daylight_applied {
            // the weather's table arrives in the background
            match daylight_table(&self.game, &self.work, true) {
                DaylightState::Ready(_) => self.apply_light(),
                DaylightState::Loading => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
                DaylightState::Failed(_) => {}
            }
        }
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
            let env = crate::livery_ui::Env { game: &self.game, work: &self.work, cache: &cache, config: self.veh_cfg, skin: &[] };
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
        let ren = self.renderer.get_or_insert_with(|| Renderer3D::new_viewer(rs));
        if let Some(g) = &self.gpu {
            if !skin.is_empty() {
                ren.set_joints(rs, g, &skin);
            }
        }
        // face expression: rebuild the morphed meshes when a weight changed
        let weights: Vec<f32> = self.expr.iter().map(|e| e.1).collect();
        if weights != self.expr_shown {
            if let (Some(l), Some(g)) = (&self.loaded, &self.gpu) {
                for (k, me) in l.model.meshes.iter().enumerate().filter(|m| !m.1.morphs.is_empty()) {
                    ren.write_vertices(rs, g, k, &morph_vertices(me, &self.expr));
                }
            }
            self.expr_shown = weights;
        }
        self.update_hair(rs, &ctx);
        let ren = self.renderer.get_or_insert_with(|| Renderer3D::new_viewer(rs));
        // an animal's fur is bound to its skin and takes the same joint
        // matrices; right after a switch from a character, `hair.gpu` still
        // holds the character's hair (one-matrix buffers) until the fur has
        // loaded, so only models built for this skeleton take them
        if self.hair.rig.is_some() && !skin.is_empty() {
            for g in self.hair.gpu.iter().filter(|g| crate::render3d::joints_fit(g.joints_buf.size(), skin.len())) {
                ren.set_joints(rs, g, &skin);
            }
        }
        // a character's hair, brows and beard ride on the head bone, scaled
        // as the body shape scales the head (`head_shape`: how it moves
        // BM_Head, which the scalp and the ponytail root hang from)
        if let Some(l) = self.loaded.as_ref().filter(|l| l.look.is_some()) {
            let head = l.model.joints.iter().position(|j| j.name == "SKEL_Head").and_then(|i| skin.get(i)).copied().unwrap_or(Mat4::IDENTITY);
            let shaped = l.model.extras["head_shape"].as_array().filter(|a| a.len() == 16).map(|a| {
                let mut c = [0f32; 16];
                c.iter_mut().zip(a).for_each(|(c, v)| *c = v.as_f64().unwrap_or(0.0) as f32);
                Mat4::from_cols_array(&c)
            });
            let head = shaped.map_or(head, |s| head * s);
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
        // a still view settles into the mean of many dither patterns (SIXNG_ACCUM=0: each frame on its own)
        ren.accumulate = std::env::var("SIXNG_ACCUM").map_or(true, |v| v != "0");
        let tid = ren.render(rs, px, &self.cam, &self.look, &models, &under, &top);
        if ren.accumulating() {
            ctx.request_repaint();
        }
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
            // SIXNG_SHOT_FRAMES=<n> (up to 64): the screenshot is the mean of n frames, each with another strand
            // dither pattern (the game averages its per-frame dither over time)
            let frames = std::env::var("SIXNG_SHOT_FRAMES").ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1).clamp(1, 64);
            let mut averaged = None;
            // a plain shot waits for the still-camera mean to settle
            let ready = ready && (frames > 1 || !ren.accumulating());
            let ready = ready && (frames == 1 || match ren.read_back(rs) {
                Some((w, h, px)) => {
                    if self.shot_acc.1.len() != px.len() {
                        self.shot_acc = (0, vec![0.0; px.len()]);
                    }
                    for (i, (a, p)) in self.shot_acc.1.iter_mut().zip(&px).enumerate() {
                        let x = *p as f32 / 255.0;
                        *a += if i % 4 == 3 { x } else { x.powf(2.2) };
                    }
                    self.shot_acc.0 += 1;
                    ren.dither_frame = self.shot_acc.0 as f32;
                    if self.shot_acc.0 >= frames {
                        let n = self.shot_acc.0 as f32;
                        averaged = Some((w, h, self.shot_acc.1.iter().enumerate().map(|(i, a)| { let x = a / n; ((if i % 4 == 3 { x } else { x.powf(1.0 / 2.2) }) * 255.0 + 0.5) as u8 }).collect::<Vec<u8>>()));
                        self.shot_acc = (0, Vec::new());
                        ren.dither_frame = 0.0;
                        true
                    } else {
                        ctx.request_repaint();
                        false
                    }
                }
                None => true,
            });
            if ready {
                if let Some((w, h, px)) = averaged.or_else(|| ren.read_back(rs)) {
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

    /// Weather choice for the game's time-of-day lighting. Returns true when
    /// the light should be set again (weather picked, or the table arrived).
    fn weather_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let state = daylight_table(&self.game, &self.work, !self.weather.is_empty());
        let label = |id: &str| {
            sixng_core::daylight::WEATHERS.iter().find(|w| w.0 == id).map(|w| w.2).unwrap_or("viewer default")
        };
        let mut changed = false;
        egui::ComboBox::from_label("weather")
            .selected_text(label(&self.weather))
            .show_ui(ui, |ui| {
                changed |= ui.selectable_value(&mut self.weather, String::new(), "viewer default").on_hover_text("The viewer's own fixed light set-up").changed();
                for (id, _, l) in sixng_core::daylight::WEATHERS {
                    changed |= ui.selectable_value(&mut self.weather, id.to_string(), l).changed();
                }
            })
            .response
            .on_hover_text("Light the model as the game does for this weather at the chosen hour: sun or moon direction, sun colour and strength, sky tint and ambient from the game's timecycle (needs common.rpf indexed)");
        if self.weather.is_empty() {
            return changed;
        }
        match state {
            DaylightState::Loading => {
                ui.label("reading the game's timecycle…");
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                self.daylight_applied = false;
            }
            DaylightState::Failed(e) => {
                ui.colored_label(egui::Color32::from_rgb(220, 140, 60), format!("no timecycle data, viewer light kept: {e}"));
            }
            DaylightState::Ready(t) => {
                if let Some(w) = t.weather(&self.weather) {
                    let l = sixng_core::daylight::light(&t, w, self.tod);
                    ui.label(format!(
                        "{} {:.0}°, light ×{:.2} (cycle ×{:.2}), ambient ×{:.2} (cycle ×{:.2}), clouds {:.0}%",
                        if l.moon { "moon" } else { "sun" },
                        if l.moon { l.dir[1].asin().to_degrees() } else { l.sun_elev },
                        l.dir_strength,
                        l.light_dir_mult,
                        l.ambient,
                        l.ambient_sky_intensity,
                        l.cloud_cover * 100.0
                    ));
                }
                if !self.daylight_applied {
                    changed = true;
                }
            }
        }
        changed
    }

    /// Set the light from the hour and weather (the game's lighting when a
    /// weather is picked and its table is loaded, else the viewer's own).
    pub fn apply_light(&mut self) {
        if !self.weather.is_empty() {
            if let DaylightState::Ready(t) = daylight_table(&self.game, &self.work, true) {
                if let Some(w) = t.weather(&self.weather) {
                    self.look.set_game_light(&sixng_core::daylight::light(&t, w, self.tod));
                    self.daylight_applied = true;
                    return;
                }
            }
            self.daylight_applied = false;
        }
        self.look.set_time_of_day(self.tod);
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
            } else if src == "character" {
                "the face of this character's other head"
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
        if !self.expr.is_empty() {
            egui::CollapsingHeader::new("Expression").id_salt("face_expr").show(ui, |ui| {
                ui.label(egui::RichText::new("Shape keys (exported as glTF morph targets): face_* are the face network's expression controls, pose_* the pose correctives garments take from the character's body network (driven by the pose in the game; what drives each one is not decoded).").weak().small());
                ui.horizontal(|ui| {
                    ui.label("filter");
                    ui.text_edit_singleline(&mut self.expr_filter);
                    if ui.button("Reset").on_hover_text("Every control back to 0 (the neutral face).").clicked() {
                        self.expr.iter_mut().for_each(|e| e.1 = 0.0);
                    }
                });
                let f = self.expr_filter.to_ascii_lowercase();
                egui::ScrollArea::vertical().max_height(320.0).id_salt("face_expr_list").show(ui, |ui| {
                    for e in self.expr.iter_mut().filter(|e| e.0.to_ascii_lowercase().contains(&f)) {
                        // pose inputs come from joint rotations and go both ways
                        let lo = if e.0.starts_with("pose_") { -1.0 } else { 0.0 };
                        ui.add(egui::Slider::new(&mut e.1, lo..=1.0).text(e.0.as_str()));
                    }
                });
            });
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
        let mut relight = ui
            .add(egui::Slider::new(&mut self.tod, 0.0..=24.0).text("time of day (h)"))
            .changed();
        relight |= self.weather_ui(ui);
        if relight {
            self.apply_light();
        }
        let mut turn = self.look.sun_turn;
        if ui
            .add(egui::Slider::new(&mut turn, -180.0..=180.0).suffix("°").text("sun direction"))
            .on_hover_text("Turns the sun (or moon) around the model, at the height the time of day gives it.")
            .changed()
        {
            self.look.turn_sun(turn);
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
        if self.part_ramp_done.is_some() && self.loaded.as_ref().is_some_and(|l| l.model.materials.iter().any(|m| m.ramp.is_some() && m.ramp_u.is_some())) {
            if self.look.wear_levels {
                ui.checkbox(&mut self.look.wear_from_config, "wear from the configuration")
                    .on_hover_text("Each part shows wear as the game works it out: the vehicle's configuration gives the part a wear level (0 new to 5 worn out), which picks a wear preset. Its age record sets the wear amount, and the preset's control intensities and rust windows say how much of each channel of the game's tiling wear mask turns to the worn layer (the game's packing of these values is read from its code). Not read: a per-part random sample and the vertex colours the game also multiplies in, and the per-vehicle pick within the age record's range (the middle is taken).");
            }
            if self.look.wear_levels && self.look.wear_from_config {
                ui.add(egui::Slider::new(&mut self.look.primer_amount, 0.0..=1.0).text("primer amount"))
                    .on_hover_text("The game's second wear layer, the primer, drawn by its rules: its mask texture scaled per channel by the wear preset's primer intensities, summed, times this amount, and windowed by primerStart / primerEnd, where the first layer is not. The game takes the amount per vehicle while it runs (from the spawn data, or from a file that is not shipped), so it is yours to set; 0 (the default) turns the layer off. Its colour is the part's primer palette row, read at the column the summed mask gives through the preset's primerGradientStart / primerGradientEnd.");
                ui.add(egui::Slider::new(&mut self.look.primer_set, 0..=2).text("primer set"))
                    .on_hover_text("Which of the three primer colours a part takes when its configuration allows several (primerSets): the game picks one per vehicle at random. Parts allowed only one keep it.");
            }
            if !(self.look.wear_levels && self.look.wear_from_config) {
                ui.add(egui::Slider::new(&mut self.look.wear, 0.0..=1.0).text("wear"))
                    .on_hover_text("Shows the second layer of the vehicle's surfaces (rust under paint, grey under bare metal), which the game's files give for every surface. Where it shows is taken from the blue channel of the parts' detail texture; how much the game shows is not known, so the amount is yours to set.");
            }
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
/// The round pieces of a character's body bound (`fragphys::body_pieces`)
/// placed on the model's bind pose, in RAGE model space (Z-up). The joints
/// are the game's bone frames turned to Y-up (C M C^T), so the game frame
/// is C^T W C.
fn body_tapers(a: &sixng_core::game::ArchiveInfo, entries: &[sixng_core::scan::Entry], name: &str, joints: &[glb::Joint]) -> (Vec<sixng_core::groom::Taper>, Vec<sixng_core::groom::Cuboid>) {
    let joints: Vec<(String, [f32; 16])> = joints.iter().map(|j| (j.name.clone(), j.ibm)).collect();
    sixng_core::hairexport::body_bound(a, entries, name, &joints)
}

fn load_parts(
    a: &sixng_core::game::ArchiveInfo,
    parts: &[&sixng_core::scan::Entry],
    cache: &Path,
    lod: usize,
    mut model: Option<glb::Model>,
    fx: &HashMap<u64, PartFx>,
    game: &Path,
    hats: &[String],
) -> (Option<glb::Model>, Vec<PathBuf>, Vec<String>, Vec<String>, Vec<(String, Vec<[f32; 3]>)>, Vec<([f32; 3], [u8; 3])>, Vec<bool>, Vec<String>, Vec<[[f32; 3]; 3]>) {
    let mut first = Vec::new();
    // the triangles of the parts named in `hats` (RAGE model space), for pressing hair under them
    let mut hat_tris = Vec::new();
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
    // per garment: its meshes and how far out it is worn (`levels`)
    let mut layered: Vec<(std::ops::Range<usize>, u8)> = Vec::new();
    // per merged mesh of a garment: its vertex positions before it followed
    // the shaped skin (`keep_layer_order`)
    let mut unshaped: HashMap<usize, Vec<[f32; 3]>> = HashMap::new();
    let levels = garment_levels(parts.iter().map(|e| e.off), fx);
    // Body shape: the skin and head are posed by the game's rig; clothes
    // and accessories follow the skin under them (`Wrap`).
    let shape = if fx.is_empty() { NEUTRAL_BODY } else { *BODY_SHAPE.lock().unwrap() };
    let mut posed: HashMap<u64, anyhow::Result<(glb::Model, Vec<PathBuf>, serde_json::Value)>> = HashMap::new();
    let mut field: Vec<([f32; 3], [f32; 3])> = Vec::new();
    // how the shape moves BM_Head (head scale): head accessories and the
    // strand hair follow it, not the skin offsets around them
    let mut head_shape: Option<glam::Mat4> = None;
    if shape != NEUTRAL_BODY {
        for e in parts {
            let Some(f) = fx.get(&e.off).filter(|f| f.skeletal) else { continue };
            let mut r = export_load_shaped(a, e, cache, lod, &f.skins, (f.decorations.as_slice(), f.skin_tone), shape);
            if let Ok((m, _, _)) = &mut r {
                if f.head || head_shape.is_none() {
                    if let Some(s) = joint_skin(m, "BM_Head").filter(|s| !s.abs_diff_eq(glam::Mat4::IDENTITY, 1e-5)) {
                        head_shape = Some(s);
                    }
                }
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
            None => export_load_skin(a, e, cache, lod, f.map_or(&[], |f| &f.skins), f.map_or((&[][..], 0), |f| (f.decorations.as_slice(), f.skin_tone))),
        };
        match r {
            Ok((mut m, lods, _)) => {
                if first.is_empty() {
                    first = lods;
                }
                let mut attached = None;
                let mut pre: Option<Vec<Vec<[f32; 3]>>> = None;
                if let Some(f) = f {
                    if !f.skeletal {
                        attached = place_attached(&mut m);
                        if let Some(s) = head_shape.filter(|_| on_head(&m) || attached.is_some_and(|at| under_head(&m, at))) {
                            transform_part(&mut m, s);
                        } else if let Some(w) = &wrap {
                            pre = Some(m.meshes.iter().map(|me| me.verts.iter().map(|v| v.pos).collect()).collect());
                            w.apply(&mut m);
                        }
                    }
                    if colour_part(&mut m, f, game, cache) {
                        coloured += 1;
                    }
                    if f.head {
                        tone = neck_tone(&m);
                        bake_under(&mut m, &f.under, game);
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
                if hats.contains(&e.name.clone().unwrap_or_default().to_ascii_lowercase()) {
                    for me in &m.meshes {
                        let v = |i: u32| me.verts.get(i as usize).map(|v| [v.pos[0], -v.pos[2], v.pos[1]]);
                        hat_tris.extend(me.indices.chunks_exact(3).filter_map(|t| Some([v(t[0])?, v(t[1])?, v(t[2])?])));
                    }
                }
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
                if let Some(&l) = levels.get(&e.off) {
                    layered.push((mesh0..mesh0 + m.meshes.len(), l));
                    for (i, v) in pre.take().into_iter().flatten().enumerate() {
                        unshaped.insert(mesh0 + i, v);
                    }
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
    // body materials without a colour texture (a body item that names a
    // stored skin texture has one: `PartFx::skins`)
    let bare_body: Vec<usize> = model.as_ref().map(|m| body_mats.into_iter().flatten().filter(|&i| m.materials.get(i).is_some_and(|x| x.base.is_none())).collect()).unwrap_or_default();
    if let (Some(m), false) = (&mut model, bare_body.is_empty()) {
        match tone {
            Some(t) => {
                for i in bare_body {
                    let mat = &mut m.materials[i];
                    mat.base_factor = [t[0], t[1], t[2], mat.base_factor[3]];
                    mat.gap = "skin tone taken from the face texture at the neck".into();
                }
                notes.push("Body skin: the outfit item names no body texture that is in the install, so the body takes the skin tone of the face texture at the neck.".to_string());
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
    // Body shape: each garment follows the skin on its own, so on a heavy or
    // muscled body a garment worn under another can come out in front of it
    // (a t-shirt through an open shirt's pocket). Put it back behind.
    if let (Some(m), false) = (&mut model, unshaped.is_empty()) {
        let mut ls: Vec<u8> = layered.iter().map(|x| x.1).collect();
        ls.sort();
        ls.dedup();
        let mut moved = 0;
        for &l in ls.iter().rev().skip(1) {
            let pick = |f: &dyn Fn(u8) -> bool| layered.iter().filter(|x| f(x.1)).map(|x| x.0.clone()).collect::<Vec<_>>();
            moved += keep_layer_order(m, &pick(&|x| x == l), &pick(&|x| x > l), &unshaped);
        }
        if moved > 0 {
            notes.push(format!("Body shape and layers: {moved} vertices of clothes worn under other clothes are kept behind the clothes over them, as far behind as on the neutral body."));
        }
    }
    let mut by_distance: Vec<std::ops::Range<usize>> = skin_meshes.clone();
    if let Some(m) = &mut model {
        let inherited = inherit_skin_regions(m, &region_parts, &skin_meshes);
        if inherited > 0 {
            notes.push(format!("Skin inserts: {inherited} vertices of a skin mesh without regions (the genitals, the nipples) take the region of the nearest body skin vertex of their layer, so the clothes that hide that skin hide them too (by nearness, not the game's own data)."));
        }
        let (mut skin_hidden, mut cloth_hidden) = (0, 0);
        for (range, layer, off) in &region_parts {
            let has_regions = range.clone().filter_map(|i| m.meshes.get(i)).any(|me| me.verts.iter().any(|v| v.zone[3] != 0));
            if !has_regions {
                continue;
            }
            // Clothes: the `_full` garment meshes carry no regions, so only a
            // garment drawn as its `_cull_00` twin (one another item hides
            // regions of) is cut here. Every garment under another still goes
            // through the distance test below as well: the masks leave some
            // of an under-layer that pokes through (Pretzel's tank top under
            // his open shirt).
            let is_skin = skin_meshes.contains(range);
            by_distance.retain(|r| r != range);
            // only the items shown hide anything: a garment switched off
            // in the Parts panel leaves the skin under it whole
            let mut masks: Vec<&str> = fx.iter().filter(|(o, _)| *o != off && parts.iter().any(|e| e.off == **o)).flat_map(|(_, f)| f.hides.iter()).filter(|h| h.0 == *layer).map(|h| h.1.as_str()).collect();
            // The second mask list (`hides_sub`, keys 1 to 3, nested sets that add the belly band, the torso sides
            // and, for key 3, the upper arms) is left out: when the game applies a key is not decoded (applying
            // key 2 on a heavy body, the body-shape guess, removes only a few of the patches). Research:
            // SIXNG_SUB=1,2 applies those keys.
            let keys: Vec<u32> = std::env::var("SIXNG_SUB").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default();
            masks.extend(fx.iter().filter(|(o, _)| *o != off && parts.iter().any(|e| e.off == **o)).flat_map(|(_, f)| f.hides_sub.iter()).filter(|h| h.0 == *layer && keys.contains(&h.1)).map(|h| h.2.as_str()));
            let hide: Vec<bool> = (0..256u32).map(|n| masks.iter().any(|k| sixng_core::wardrobe::mask_has(k, n))).collect();
            // The second byte of a body skin's regions is a region in a second layer, shared by the
            // two halves: the strip round the waist that both carry. Each half hides it there, so
            // the upper half's copy goes when the lower half is worn and the two do not fight. Only
            // on skin: a garment's second byte is not that layer (a shoe would go).
            let masks2: Vec<&str> = fx.iter().filter(|(o, _)| *o != off && parts.iter().any(|e| e.off == **o)).flat_map(|(_, f)| f.hides.iter()).filter(|h| h.0 == SKIN_LAYER2).map(|h| h.1.as_str()).collect();
            let hide2: Vec<bool> = (0..256u32).map(|n| n != 0 && is_skin && masks2.iter().any(|k| sixng_core::wardrobe::mask_has(k, n))).collect();
            for i in range.clone() {
                let Some(me) = m.meshes.get_mut(i) else { continue };
                let before = me.indices.len() / 3;
                let r2 = std::mem::take(&mut me.region2);
                let gone = |i: u32| hide[me.verts[i as usize].zone[3] as usize] || r2.get(i as usize).is_some_and(|&r| hide2[r as usize]);
                me.indices = me.indices.chunks_exact(3).filter(|t| !t.iter().all(|&i| gone(i))).flatten().copied().collect();
                me.region2 = r2;
                *(if is_skin { &mut skin_hidden } else { &mut cloth_hidden }) += before - me.indices.len() / 3;
            }
        }
        m.triangles = m.triangles.saturating_sub(skin_hidden + cloth_hidden);
        if skin_hidden + cloth_hidden > 0 {
            notes.push(format!("Worn under something: {skin_hidden} skin triangles and {cloth_hidden} triangles of clothes are hidden by the hide masks of the worn items (each item lists per layer the regions it covers; every vertex carries its region)."));
        }
    }
    if let Some(m) = &mut model {
        let inserts: Vec<(std::ops::Range<usize>, u32)> = region_parts.iter().filter(|p| fx.get(&p.2).is_some_and(|f| f.insert)).map(|p| (p.0.clone(), p.1)).collect();
        if !inserts.is_empty() {
            let garments: Vec<(std::ops::Range<usize>, Vec<u32>)> = region_parts.iter().filter_map(|p| fx.get(&p.2).filter(|f| !f.body && !f.head).map(|f| (p.0.clone(), f.hides.iter().map(|h| h.0).collect()))).collect();
            let hidden = hide_covered_inserts(m, &inserts, &garments);
            if hidden > 0 {
                notes.push(format!("Skin inserts under clothes: {hidden} triangles of an insert (the genitals, the nipples) over which a garment of its layer lies are hidden, so it does not come through a loose garment (by distance, not the game's own rule)."));
            }
        }
    }
    if let (Some(m), false, false) = (&mut model, by_distance.is_empty(), cover_meshes.is_empty()) {
        let hidden = hide_covered_skin(m, &by_distance, &cover_meshes, 0.015);
        if hidden > 0 {
            notes.push(format!("Skin under clothes: {hidden} triangles of a skin mesh without regions that lie right under a garment are hidden, so skin does not show through it (by distance, not the game's masks)."));
        }
    }
    if let Some(m) = &mut model {
        // each level of clothes, inside out, hidden where the levels over it cover it
        let mut ls: Vec<u8> = layered.iter().map(|x| x.1).collect();
        ls.sort();
        ls.dedup();
        let mut hidden = 0;
        for &l in ls.iter().rev().skip(1).rev() {
            let pick = |f: &dyn Fn(u8) -> bool| layered.iter().filter(|x| f(x.1)).map(|x| x.0.clone()).collect::<Vec<_>>();
            hidden += hide_covered_skin(m, &pick(&|x| x == l), &pick(&|x| x > l), 0.03);
        }
        if hidden > 0 {
            notes.push(format!("Layers: {hidden} triangles of clothes worn under other clothes (under a jacket, or a t-shirt under an open shirt) are hidden where the clothes over them cover them, so they do not show through. Which is worn over which: a jacket over the rest, and an item whose hide masks name another item's layer over that item."));
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
            if let Some(n) = teeth_follow_jaw(m).filter(|&n| n > 0) {
                notes.push(format!("Jaw: the teeth, tongue and inner mouth take {n} of the head's shape keys that move the jaw (open, forward, back, sideways), fitted on the chin as a turn about the jaw bone or a shift, so they open with the mouth in the Expression panel. The game turns the jaw in code; the angle here is fitted from the skin."));
            }
        }
    }
    if let (Some(m), Some(s)) = (&mut model, head_shape) {
        m.extras["head_shape"] = serde_json::json!(s.to_cols_array());
    }
    if coloured > 0 {
        notes.push(format!("Clothing colours: {coloured} part(s) coloured from the outfit data: the item's garment preset with its fabric (weave under the colour gradient, then the trim and wash layers, baked into the garment's texture; one per sub-material, picked by the garment's materialid texture, or by the part number in its vertex colour where it has none), else the item's stored colour, else row tint 0 of its tint palette (for the generic palette only when it agrees with the item's colour tag), else the colour tag itself (one of 18 words such as navy or khaki, shown in a plain shade of ours). Second and third tints are not applied."));
    }
    (model, first, names, errs, skin, colours, mask, notes, hat_tris)
}

/// Colour a ped clothing part of an outfit: its grey shade maps become the
/// item's stored colour times the shade, or the tint palette row at the
/// shade. Returns whether anything was coloured.
/// Sub-material per texel of a material's base texture: its `materialid`
/// texture (red x 63, as the clothing shader reads it), resampled to `w` x
/// `h`. None when the material binds none or the file is not there.
/// Rows of the part ramp image (128 wide): 256 of the first layer's palette
/// rows, 256 of the second layer's, 256 of surfaces (`part_surface_row`),
/// 256 of the parts' coats and one of the body paint's coat (`coat_row`).
const PART_RAMP_ROWS: usize = 1281;
/// The primer's colour where a part has no primer palette row: a grey
/// (sRGB byte of linear 0.2).
const PRIMER_GREY: u8 = 124;

/// One row of the part ramp image for a coat record (model.json
/// `part_colours.coat.records`), all zero without one: the values in the
/// alpha of the columns 0 to 10 (repeated every 16 columns), as the shader
/// reads them. 0 clear coat intensity, 1 clear coat roughness, 2 flake
/// orientation x 4 (0 = no flakes), 3 flake size x 4, 4 the base coat
/// roughness when the record overrides it (0 = not), 5 anisotropy, 6 sheen
/// intensity, 7 sheen fade, 8 sheen reflectance x 4, 9 film IOR / 4,
/// 10 film thickness.
fn coat_row(r: Option<&serde_json::Value>) -> Vec<u8> {
    let mut v = [0u8; 16];
    if let Some(r) = r.filter(|r| r.is_object()) {
        let f = |k: &str| r[k].as_f64().unwrap_or(0.0) as f32;
        let on = |k: &str| r[k].as_bool().unwrap_or(false);
        let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let ([clear, clear_rough, flakes, size], base) = paint_coat(r);
        let sheen = f("sheen_intensity");
        let film = f("film_thickness");
        v[..11].copy_from_slice(&[
            q(clear),
            q(clear_rough),
            if flakes > 0.0 { q(flakes * 4.0).max(1) } else { 0 },
            q(size * 4.0),
            base.map_or(0, |b| q(b).max(1)),
            if on("use_anisotropy") { q(f("anisotropy_intensity")) } else { 0 },
            q(sheen),
            if sheen > 0.0 { q(f("sheen_fade")) } else { 0 },
            if sheen > 0.0 { q(f("sheen_reflectance") * 4.0) } else { 0 },
            if film > 0.0 { q(f("film_ior") / 4.0) } else { 0 },
            q(film),
        ]);
    }
    (0..128).flat_map(|x| [0, 0, 0, v[x & 15]]).collect()
}

/// Average wear amount (0 to 255) of the wear presets of each level whose
/// age is a range: the middle of their `minWear` and `maxWear`, averaged
/// over the library's ladders. A preset whose age record is not a range
/// takes its level's value.
const LEVEL_WEAR: [f32; 6] = [15.0, 150.0, 197.0, 211.0, 239.0, 239.0];

/// The wear preset bytes (`cc518af0`) the game packs into a part's shader
/// record for the first wear layer (packer at rva 0x37766a0): per mask
/// channel A, B, C the control intensity, its activation start and end
/// (`controlIntensityA`, b18a6db5, 1b6a3e09 / `controlIntensityB`,
/// 1e688003, 1c58826f / `controlIntensityC`, f97f84c3, 4d46aee9) and the
/// window on the scaled mask (`rustStart` / `rustEnd`, `B`, `C`).
const WEAR_CONTROL: [[u32; 3]; 3] = [[0x262c_839d, 0xb18a_6db5, 0x1b6a_3e09], [0x9b82_6e47, 0x1e68_8003, 0x1c58_826f], [0xb770_a627, 0xf97f_84c3, 0x4d46_aee9]];
const WEAR_WINDOW: [[u32; 2]; 3] = [[0x8f02_219f, 0x7a2d_3f04], [0x9d7c_73e7, 0x1170_352b], [0x39ab_ac47, 0x9970_c52a]];
/// The wear preset bytes of the first wear layer's palette weight W2 (the
/// game's shader blends the layer's tint into its palette colour by it):
/// per channel R, G, B and the 4th channel D4 the window start and end
/// (preset +0x38/39, +0x3c/3d, +0x40/41, +0x4f/50), their activation start
/// and end on the wear amount (+0x46/47, +0x48/49, +0x4a/4b, +0x51/52) and
/// the divisor byte (+0x4c, +0x4d, +0x4e, +0x53; the shader multiplies each
/// window by 255 / byte). Packer at rva 0x37766a0 (part record +80, +84,
/// +88).
const W2_WINDOW: [[u32; 2]; 4] = [[0x951a_7677, 0xb588_96f4], [0x87df_7517, 0x7c99_9233], [0x4549_efed, 0x8c27_314e], [0xbe3b_1bd2, 0x0ee0_92f0]];
const W2_ACTIVATION: [[u32; 2]; 4] = [[0x8482_c039, 0x4cac_ba9c], [0x851c_6b0b, 0xac8e_3a46], [0x91c6_845f, 0x9ae0_16ea], [0x80c7_dae3, 0x356e_05be]];
const W2_DIVISOR: [u32; 4] = [0xfcf8_c89c, 0xeeee_2c87, 0xd948_013b, 0xa21f_3ea9];
/// The 4th wear channel D4: the preset byte that scales it (f86ce666,
/// +0x21; the game packs f16(v0 x byte / 255) at part record +52 and the
/// shader multiplies it by the layer detail texture's blue), and its window
/// in the first layer's weight (eb7223d6, d54c040a, +0x22 / +0x23; record
/// +64 bytes 2 and 3).
const D4_SCALE: u32 = 0xf86c_e666;
/// The wear preset's link to its edge record (`487f0f4c`: edgeIntensity,
/// edgeStart, edgeEnd), which the game packs into part record +40 (rva
/// 0x1a0d7a0: f16 of the intensity, start and end x 255 truncated).
const PRESET_EDGE: u32 = 0x7f1f_e0a2;
/// The age record's clear-coat loss (aa7d4fbd, +0xf0): the game packs
/// trunc(25.5 x value) into part record +56 byte 3, and its shader takes
/// 10 x byte / 255 of the clear coat per unit of each wear layer's weight.
const AGE_COAT_LOSS: u32 = 0xaa7d_4fbd;
/// The age record's curves (`fe5d32e7`) the game packs for the dust and
/// scratch mask (rva 0x37766a0; age floats at +k .. +k+16): per curve its
/// min, max, start, end and exponent. The value at wear amount v0 is
/// lerp(min, max, sat((v0^exp - start) / max(1e-5, end - start))).
/// sunBleach (+0x30), baseBleach (+0x58), +0x44, +0x6c, redScratch (+0x8c),
/// blueScratch (+0xa4), +0xc8, +0xdc.
const AGE_CURVES: [[&str; 5]; 8] = [
    ["c8482ad4", "e50025b1", "sunBleachStart", "sunBleachEnd", "3ef91ac2"],
    ["baseBleachMin", "baseBleachMax", "baseBleachStart", "baseBleachEnd", "63ab1a06"],
    ["db62bdc3", "a53b57b9", "6a4efd46", "ded82b1b", "a1a33c04"],
    ["90294599", "38c12016", "cec65eaa", "16356415", "e921e50c"],
    ["1da6a331", "5b934fe3", "redScratchStart", "redScratchEnd", "d1f62b9a"],
    ["a1e80b6a", "46123d7b", "blueScratchStart", "blueScratchEnd", "79d86748"],
    ["0c9cba21", "8c08b137", "1978a268", "1dce80e3", "aac54cc0"],
    ["5ccb8f95", "7a78f02e", "8ed733ec", "90baf5cf", "8243d8af"],
];
/// The age record's bools the game packs as part flag bits 18 to 23 and
/// 24 to 26 (age +0xbf..+0xc4, +0xbc..+0xbe): which mask channel (red,
/// blue, the dust green) drives the clear-coat loss, a third G-buffer value
/// and the added roughness.
const AGE_DUST_BITS: [&str; 9] = ["6d9d81b3", "f4c2b82b", "32e128c3", "0e6ac3da", "6a2b9ab4", "11379ec6", "3ca53c6d", "2d6ae1aa", "a06d3fe0"];
const D4_WINDOW: [u32; 2] = [0xeb72_23d6, 0xd54c_040a];
/// The age record's wear amount range (`fe5d32e7` +0x0c, +0x10): the game
/// takes a per-vehicle value between them.
const AGE_WEAR: [u32; 2] = [0x4e71_efc8, 0xb5fc_dec9];
/// The wear preset bytes the game packs for the second wear layer (the
/// primer, layer B): per mask channel the primer intensity and its
/// activation start and end (`primerIntensityA`, 90824e97, df7be5cf /
/// `primerIntensityB`, 4eb87881, 26007b2d / `primerIntensityC`, 601bdcc6,
/// ad2cb19a), and the window on the summed mask (`primerStart`,
/// `primerEnd`).
const PRIMER_CONTROL: [[u32; 3]; 3] = [[0x9ed9_875c, 0x9082_4e97, 0xdf7b_e5cf], [0xc74b_d840, 0x4eb8_7881, 0x2600_7b2d], [0x4a7f_5ea9, 0x601b_dcc6, 0xad2c_b19a]];
const PRIMER_WINDOW: [u32; 2] = [0x70b0_608e, 0xf47f_68a1];
/// The preset byte by which the game takes the second and third mask
/// channels off the first before the primer's first channel (8ed71149).
const PRIMER_CROSS: u32 = 0x8ed7_1149;
/// The preset bool that makes the primer read the wear mask instead of its
/// own mask texture (eff390bd).
const PRIMER_ON_WEAR_MASK: u32 = 0xeff3_90bd;
/// The wear preset's links to its two colour range records (`3539c333`):
/// the first feeds the first wear layer, the second the primer (by link
/// order; the game holds them at wear preset object +0x38 and +0x40).
const RANGE_LINKS: [u32; 2] = [0x02a2_7216, 0x8797_74bd];
/// A colour range record's low and high bytes per mask channel R, G, B of
/// the scale (`minRed` .. `maxBlue`), the bias and the gamma: the game draws
/// 50 samples per record at load (rva 0xc7b660), each value
/// lo + rand x (hi - lo), and each vehicle part picks one of them.
const RANGE_SCALE: [[u32; 3]; 2] = [[0x0904_e7f5, 0xa3f6_55bd, 0x17fe_6786], [0x30a6_2d96, 0x7cdc_2a90, 0x0d16_6a4e]];
const RANGE_BIAS: [[u32; 3]; 2] = [[0x9d9e_c6c9, 0xfb2b_7f54, 0xf0fb_4e0e], [0x1900_6c48, 0xc580_1a83, 0x77d2_db40]];
const RANGE_GAMMA: [[u32; 3]; 2] = [[0x9543_82a0, 0x7e3d_ffd5, 0xaa03_fd91], [0x87f8_9264, 0xc82d_2173, 0x483f_35b8]];

/// The middle of a colour range record's samples (scale, bias, gamma per
/// channel, 0 to 1): what a part shows on average, as the part's own pick
/// depends on a per-vehicle seed. Without a record: scale 1, bias 0,
/// gamma 1.
fn range_mid(r: &serde_json::Value) -> [[f32; 3]; 3] {
    if !r.is_object() {
        return [[1.0; 3], [0.0; 3], [1.0; 3]];
    }
    let f = |h: u32| r[sixng_core::meta7a::field_name(h)].as_f64().unwrap_or(0.0) as f32 / 255.0;
    let mid = |t: &[[u32; 3]; 2]| std::array::from_fn(|c| (f(t[0][c]) + f(t[1][c])) * 0.5);
    [mid(&RANGE_SCALE), mid(&RANGE_BIAS), mid(&RANGE_GAMMA)]
}

/// What the viewer draws of a part id's wear: the amount (0 to 255) and,
/// per channel of the game's wear mask, the scale, window start and window
/// width (0 to 255 each); for the primer (layer B) the scale per channel of
/// its mask, the cross byte, the window start and width on the summed mask,
/// and whether its mask is the wear mask.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PartWear {
    amount: u8,
    scale: [u8; 3],
    /// the bias added to the scaled mask, and the gamma (0 to 255 each)
    bias: [u8; 3],
    gamma: [u8; 3],
    start: [u8; 3],
    width: [u8; 3],
    primer: [u8; 3],
    primer_bias: [u8; 3],
    primer_gamma: [u8; 3],
    primer_cross: u8,
    primer_start: u8,
    primer_width: u8,
    primer_on_wear_mask: bool,
    /// the window on the primer's summed mask that gives its palette
    /// column (`primerGradientStart`, `primerGradientEnd` - start)
    primer_gradient_start: u8,
    primer_gradient_width: u8,
    /// the 4th channel's scale (0 to 255 for 0 to 1) and its window in the
    /// first layer's weight
    d4_scale: u8,
    d4_start: u8,
    d4_width: u8,
    /// W2's window start and width and divisor byte per channel R, G, B, D4
    w2_start: [u8; 4],
    w2_width: [u8; 4],
    w2_div: [u8; 4],
    /// the edge record's intensity (0 to 255 for 0 to 1), start and end
    edge_intensity: u8,
    edge_start: u8,
    edge_end: u8,
    /// the clear coat lost per unit of wear weight, x 10 / 255
    coat_loss: u8,
    /// the dust and scratch mask, as the game packs it: part record +92
    /// bytes 3, 1, 2 (tiling x 255 / 50, red and blue intensities), +0
    /// bytes 0, 1, +4 bytes 0, 1 (dust scale and bias), +56 bytes 0 to 2
    /// (gains per layer: base, primer, first), +60 bytes 0 to 3, and the
    /// flag bits 18 to 23 and 24 to 26
    dust: [u8; 16],
}

/// Each part id's wear under a configuration, from model.json
/// `part_colours.wear_records`: the part's wear level (`levels_by_config`,
/// or `levels_default` without a configuration) picks an entry of its
/// ladder (`ladder_by_look` of this look) and so a wear preset.
/// - The amount v0: the middle of the preset's age record's wear range
///   (`AGE_WEAR`; the game takes a per-vehicle value in it); for an age
///   range record the middle of `minWear` and `maxWear`; else
///   `LEVEL_WEAR` of the level.
/// - Per mask channel, as the game packs it: scale = v0 x intensity x
///   sat((v0 - start) / (end - start)) of `WEAR_CONTROL`, and the window
///   start and width of `WEAR_WINDOW`; the scale and the bias times the
///   middle of the first colour range's samples (`range_mid`), and its
///   gamma. The game takes one random sample per part, and multiplies by
///   the vertex colours, which are not read.
/// - The primer the same way from `PRIMER_CONTROL`, its window from
///   `PRIMER_WINDOW` (no width floor), `PRIMER_CROSS` and
///   `PRIMER_ON_WEAR_MASK`, with the second colour range.
/// - Both layers only when v0 is above 0.1 (the game's flag bit 0).
/// None when the model has no wear levels; all zero for a part id without
/// a level.
fn part_wear(w: &serde_json::Value, look: usize, config: Option<&str>) -> Option<Vec<PartWear>> {
    let levels = match config {
        Some(k) => w["levels_by_config"][k].as_str(),
        None => w["levels_default"].as_str(),
    };
    if w["levels_by_config"].as_object().is_none_or(|m| m.is_empty()) {
        return None;
    }
    let mut out = vec![PartWear::default(); 256];
    let Some(levels) = levels.filter(|s| s.len() == 1024) else { return Some(out) };
    let ladders = w["ladder_by_look"][look].as_str().filter(|s| s.len() == 512);
    let field = |v: &serde_json::Value, h: u32| v[sixng_core::meta7a::field_name(h)].as_f64();
    for (id, o) in out.iter_mut().enumerate() {
        let Ok(level) = u8::from_str_radix(&levels[id * 4..id * 4 + 2], 16) else { continue };
        if level == 255 {
            continue;
        }
        let level = level.min(5) as usize;
        let ladder = ladders.and_then(|s| usize::from_str_radix(&s[id * 2..id * 2 + 2], 16).ok()).filter(|l| *l != 255);
        let preset = ladder.and_then(|l| w["ladders"][l]["wearPresets"][level].as_str()).map(|p| &w["presets"][p]).filter(|p| p.is_object());
        let age = preset.and_then(|p| p["ageWearHash"].as_str());
        let from_age = age.map(|a| &w["ages"][a]).and_then(|r| Some(((field(r, AGE_WEAR[0])? + field(r, AGE_WEAR[1])?) * 0.5) as f32));
        let from_range = age.map(|a| &w["age_ranges"][a]).and_then(|r| Some(((r["minWear"].as_f64()? + r["maxWear"].as_f64()?) * 0.5 / 255.0) as f32));
        let v0 = from_age.or(from_range).unwrap_or(LEVEL_WEAR[level] / 255.0).clamp(0.0, 1.0);
        o.amount = (v0 * 255.0 + 0.5) as u8;
        let Some(p) = preset else { continue };
        if v0 <= 0.1 {
            continue;
        }
        // a preset byte as the game reads it: times the float nearest 1 / 255
        let b = |h: u32| field(p, h).unwrap_or(0.0) as f32 * f32::from_bits(0x3b80_8081);
        // the game's B(): truncated
        let trunc = |x: f32| (x.clamp(0.0, 1.0) * 255.0) as u8;
        // a window width without a floor: truncated, a negative one wrapping
        // round as the game's low byte of the integer does
        let wrap = |x: f32| (x * 255.0) as i32 as u8;
        let [ra, rb] = RANGE_LINKS.map(|l| range_mid(p[sixng_core::meta7a::field_name(l)].as_str().map_or(&serde_json::Value::Null, |k| &w["colour_ranges"][k])));
        for c in 0..3 {
            let [i, s, e] = PRIMER_CONTROL[c].map(b);
            let act = ((v0 - s) / (e - s).max(1.0e-4)).clamp(0.0, 1.0);
            let k = v0 * i * act;
            o.primer[c] = trunc(rb[0][c] * k);
            o.primer_bias[c] = trunc(rb[1][c] * k);
            o.primer_gamma[c] = trunc(rb[2][c]);
        }
        let [ps, pe] = PRIMER_WINDOW.map(b);
        (o.primer_start, o.primer_width) = (trunc(ps), wrap(pe - ps));
        o.primer_cross = trunc(b(PRIMER_CROSS));
        o.primer_on_wear_mask = p[sixng_core::meta7a::field_name(PRIMER_ON_WEAR_MASK)].as_bool() == Some(true);
        let g = |k: &str| p[k].as_f64().unwrap_or(0.0) as f32 * f32::from_bits(0x3b80_8081);
        (o.primer_gradient_start, o.primer_gradient_width) = (trunc(g("primerGradientStart")), wrap(g("primerGradientEnd") - g("primerGradientStart")));
        for c in 0..3 {
            let [i, s, e] = WEAR_CONTROL[c].map(b);
            let act = ((v0 - s) / (e - s).max(1.0e-4)).clamp(0.0, 1.0);
            let k = v0 * i * act;
            o.scale[c] = trunc(ra[0][c] * k);
            o.bias[c] = trunc(ra[1][c] * k);
            o.gamma[c] = trunc(ra[2][c]);
            let [ws, we] = WEAR_WINDOW[c].map(b);
            // the game's width (rva 0x173f430, in f32): 255 x max(1e-4,
            // end - start), truncated, so often one less than the bytes'
            // difference
            o.start[c] = trunc(ws);
            o.width[c] = trunc((we - ws).max(1.0e-4));
        }
        // the 4th channel: its scale (the game's f16 of v0 x byte, here a
        // byte) and its window, packed like the rust windows
        o.d4_scale = (v0 * b(D4_SCALE) * 255.0 + 0.5) as u8;
        let [ds, de] = D4_WINDOW.map(b);
        (o.d4_start, o.d4_width) = (trunc(ds), trunc((de - ds).max(1.0e-4)));
        // W2 (rva 0x37766a0, part record +80, +84): per channel the
        // activation a = sat((v0 - start) / max(1e-4, end - start)), then
        // the window start a x s and width a x e - a x s (one fused
        // multiply-subtract, no floor), each x 255 truncated to its low byte
        for c in 0..4 {
            let [s, e] = W2_ACTIVATION[c].map(b);
            let a = ((v0 - s) / (e - s).max(1.0e-4)).clamp(0.0, 1.0);
            let [ws, we] = W2_WINDOW[c].map(b);
            let lo = a * ws;
            o.w2_start[c] = wrap(lo);
            o.w2_width[c] = wrap(a.mul_add(we, -lo));
            o.w2_div[c] = field(p, W2_DIVISOR[c]).unwrap_or(0.0) as u8;
        }
        // the edge record (none in the export: no edge term)
        if let Some(e) = p[sixng_core::meta7a::field_name(PRESET_EDGE)].as_str().map(|k| &w["edges"][k]).filter(|e| e.is_object()) {
            let f = |k: &str| e[k].as_f64().unwrap_or(0.0) as f32;
            o.edge_intensity = (f("edgeIntensity").clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            o.edge_start = wrap(f("edgeStart"));
            o.edge_end = wrap(f("edgeEnd"));
        }
        let loss = age.map(|a| &w["ages"][a]).and_then(|r| field(r, AGE_COAT_LOSS)).unwrap_or(0.0) as f32;
        o.coat_loss = (loss * 25.5) as i32 as u8;
        // the dust and scratch mask, from the age record (none for an age
        // range record): the game's packing of part record +0, +4, +56,
        // +60, +92 and the flag bits
        if let Some(r) = age.map(|a| &w["ages"][a]).filter(|r| r.is_object()) {
            let af = |k: &str| r[k].as_f64().unwrap_or(0.0) as f32;
            let crv = |k: usize| {
                let [lo, hi, s, e, x] = AGE_CURVES[k].map(af);
                lo + (hi - lo) * ((v0.powf(x) - s) / (e - s).max(1.0e-5)).clamp(0.0, 1.0)
            };
            let bb = |x: f32| (x * 255.0) as i32 as u8;
            let tenth = |x: f32| (x * 25.5) as i32 as u8;
            let signed = |x: f32| (127.5 + 127.5 * x) as i32 as u8;
            // the tiling: the middle of minTiling .. maxTiling (the game
            // picks a per-vehicle point), x 5.1 truncated
            let tiling = ((af("minTiling") + af("maxTiling")) * 0.5 * 5.1) as i32 as u8;
            let bit = |k: &str, b: u32| if r[k].as_bool() == Some(true) { 1u8 << b } else { 0 };
            let lo_bits = (0..6).fold(0, |m, i| m | bit(AGE_DUST_BITS[i], i as u32));
            let hi_bits = (0..3).fold(0, |m, i| m | bit(AGE_DUST_BITS[6 + i], i as u32));
            o.dust = [
                tiling,
                bb(crv(4)),
                bb(crv(5)),
                bb(crv(0)),
                bb(crv(1)),
                bb(crv(2)),
                bb(crv(3)),
                bb(af("91b26647")),
                bb(af("cf9f9929")),
                bb(af("inclineComponentsScale")),
                tenth(crv(6)),
                signed(crv(4) * af("193c7eb1")),
                tenth(crv(7)),
                signed(crv(5) * af("2c04933e")),
                lo_bits,
                hi_bits,
            ];
        }
    }
    Some(out)
}

/// Each part id's `primerSets` mask under a configuration (the low byte
/// of its `levels_by_config` entry, or of `levels_default`); None when the
/// model has no wear levels.
fn primer_set_masks(w: &serde_json::Value, config: Option<&str>) -> Option<Vec<u8>> {
    let levels = config.and_then(|k| w["levels_by_config"][k].as_str()).or_else(|| w["levels_default"].as_str()).filter(|s| s.len() == 1024)?;
    Some((0..256).map(|id| u8::from_str_radix(&levels[id * 4 + 2..id * 4 + 4], 16).unwrap_or(1)).collect())
}

/// The primer set a part takes (rva 0x55bd870): the first of an order of
/// the sets 0, 1, 2 that its `primerSets` mask allows, else 0. The game's
/// order is a per-vehicle random one (rva 0x9aea00); here it starts at
/// `first` and goes on in turn.
fn primer_set(mask: u8, first: u8) -> usize {
    (0..3).map(|k| (first as usize + k) % 3).find(|s| mask & (1 << s) != 0).unwrap_or(0)
}

/// The primer's metalness and roughness, low and high (0 to 255 each:
/// metalness low, high, roughness low, high): the library's `primerPalette`
/// (global record), which the game packs once into its global object
/// (rva 0x5b4cce0, each value x 255 truncated) and copies into every
/// part's primer surface record; its paint shader blends each pair by the
/// material map's red (metalness) and green (roughness), as for the first
/// layer. Without the record: roughness 0.6, metalness 0.
fn primer_surface(global: &serde_json::Value) -> [u8; 4] {
    let p = &global["primerPalette"];
    let byte = |k: &str, d: f64| (p[k].as_f64().unwrap_or(d).clamp(0.0, 1.0) * 255.0) as u8;
    [byte("metallicLow", 0.0), byte("metallicHigh", 0.0), byte("roughnessLow", 0.6), byte("roughnessHigh", 0.6)]
}

/// Four bytes from 8 hex digits.
/// Three bytes from 6 hex digits.
fn hex3(h: &str) -> Option<[u8; 3]> {
    let b = h.as_bytes();
    if b.len() != 6 || !b.iter().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(std::array::from_fn(|k| u8::from_str_radix(&h[k * 2..k * 2 + 2], 16).unwrap_or(0)))
}

fn hex4(h: &str) -> Option<[u8; 4]> {
    let b = h.as_bytes();
    if b.len() != 8 || !b.iter().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(std::array::from_fn(|k| u8::from_str_radix(&h[k * 2..k * 2 + 2], 16).unwrap_or(0)))
}

/// One row of the part ramp image for a part id's surface (metalness low
/// and high, roughness low and high): the four values in the alpha of the
/// columns 0 to 3 (repeated along the row). Alpha is not sRGB-decoded, so
/// the shader reads the bytes as stored.
fn part_surface_row(s: [u8; 4]) -> Vec<u8> {
    (0..128).flat_map(|x| [0, 0, 0, s[x & 3]]).collect()
}

/// The surface a look entry's alpha stands for when an export has no
/// `surfaces` (see `vehcolour::surface_code`: metalness in four steps x 50 +
/// roughness in 50 steps): low = high. The body paint (254) and ids without
/// a part read as a plain non-metal.
fn surface_of_code(code: u8) -> [u8; 4] {
    if code >= 200 {
        return [0, 0, 192, 192];
    }
    let (m, r) = ((code / 50) as f32 / 3.0, (code % 50) as f32 / 49.0);
    let q = |v: f32| (v * 255.0 + 0.5) as u8;
    [q(m), q(m), q(r), q(r)]
}

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

pub(crate) enum DaylightState {
    Loading,
    Failed(String),
    Ready(Arc<sixng_core::daylight::Table>),
}

/// The game's time-of-day table (`sixng_core::daylight`), loaded or
/// decoded once in the background when first wanted (`start`).
pub(crate) fn daylight_table(game: &str, work: &str, start: bool) -> DaylightState {
    type Slot = Mutex<Option<Result<Arc<sixng_core::daylight::Table>, String>>>;
    static TABLE: std::sync::OnceLock<Arc<Slot>> = std::sync::OnceLock::new();
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let slot = TABLE.get_or_init(|| Arc::new(Mutex::new(None))).clone();
    if let Some(r) = slot.lock().unwrap().as_ref() {
        return match r {
            Ok(t) => DaylightState::Ready(t.clone()),
            Err(e) => DaylightState::Failed(e.clone()),
        };
    }
    if start && !STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let ws = Workspace::new(game, work);
        std::thread::spawn(move || {
            let r = sixng_core::daylight::load_or_build(&ws).map(Arc::new).map_err(|e| format!("{e:#}"));
            *slot.lock().unwrap() = Some(r);
        });
    }
    DaylightState::Loading
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
        // clothing whose textures are not in the install: the preset colour
        // per vertex (part number in the vertex colour's green, as below)
        if bare_tint(m, |col| f.subs.get(((col[1] >> 2) & 7) as usize).copied().flatten().or(f.subs[0]).unwrap_or(first)) {
            done.insert(usize::MAX);
        }
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
                // materials sharing the texture that mark their sub-material per
                // vertex (a lens on a frame that has a `materialid` texture)
                let by_vertex: Vec<usize> = mats.iter().copied().filter(|&k| sub_material_map(&m.materials[k], &[cache, shaped.as_path()], 1, 1).is_none()).collect();
                // ped glass (`ped_standard_muv_alpha` drawn blended, its
                // sub-material per vertex): the transmission record of the
                // sub-material most of its vertices name, for the viewer's
                // tinted-glass blend; its texels' alpha becomes the glass
                // opacity in the bake
                let mut glass = Vec::new();
                let lenses: Vec<usize> = by_vertex.iter().copied().filter(|&k| m.materials[k].shader_hash == "1c43a65b" && m.materials[k].alpha_mode == "BLEND").collect();
                for k in lenses {
                    let mut count = [0usize; 8];
                    for me in m.meshes.iter().filter(|me| me.material == Some(k)) {
                        for v in &me.verts {
                            count[((v.col[1] >> 2) & 7) as usize] += 1;
                        }
                    }
                    let Some(s) = (0..8).filter(|&s| count[s] > 0).max_by_key(|&s| (count[s], 8 - s)) else { continue };
                    if let Some(rec) = f.preset.and_then(|p| crate::fabric::transmission(game, p, s)) {
                        m.materials[k].glass = Some(rec);
                        glass.push(k);
                    }
                }
                if let Some(main) = crate::fabric::bake(m, &mats, &by_vertex, &glass, i, &fab, &|w, h| sub_material_map(&mat, &[cache, shaped.as_path()], w, h), detail) {
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
        (None, None, None) => {
            // Nothing in the install gives this item a colour: its own
            // textures are missing and it stores no preset, tint row, flat
            // colour or colour tag (19 items in the 292 named characters'
            // default looks, mostly watches). A neutral dark grey instead of
            // white; not the game's colour. Not skin: the body takes the
            // face's skin tone later, and a grey tint would darken it.
            if !f.body && !f.head {
                bare_tint(m, |_| BARE_FALLBACK);
            }
            return false;
        }
    };
    let mut done = std::collections::HashSet::new();
    // clothing whose textures are not in the install: the colour of plain
    // cloth (shade 125) per vertex
    if bare_tint(m, |_| lut[125]) {
        done.insert(usize::MAX);
    }
    // and materials whose colour slot binds a tint data map rather than a
    // colour (`packed_tint_map`: the shared jewellery map of about 300
    // materials), as on items with a tint and no preset (Raul's hoop)
    let packed: Vec<usize> = m.materials.iter().filter(|x| !x.ped_shade).filter_map(|x| x.base).filter(|&i| m.images.get(i).is_some_and(|im| im.bc.is_none() && im.mips.first().is_some_and(|l| packed_tint_map(&l.2)))).collect();
    let bases: Vec<usize> = m.materials.iter().filter(|x| x.ped_shade).filter_map(|x| x.base).chain(packed).collect();
    for i in bases {
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

/// Whether RGBA8 texels are a tint data map in the layout of the clothing
/// shade map (`sixng_core::pedtint`): R and B are each off or full (they
/// pick the region), G is the shade. The jewellery map (texture
/// d01c36ca7ea7eef4, bound as `diffusetex`) is one: orange (255, 110, 0),
/// blue (0, 110, 255) and green (0, 110, 0) regions. Needs most texels to
/// have R or B full, so a black colour map does not count. Which of the
/// three tint rows each region takes is not decoded (as for the clothing
/// shade map): the map is coloured with tint 0.
fn packed_tint_map(px: &[u8]) -> bool {
    let edge = |v: u8| v <= 12 || v >= 243;
    let (mut n, mut ok, mut lit) = (0usize, 0usize, 0usize);
    for t in px.chunks_exact(4).filter(|t| t[3] > 0) {
        n += 1;
        ok += usize::from(edge(t[0]) && edge(t[2]));
        lit += usize::from(t[0] >= 243 || t[2] >= 243);
    }
    // (98 % on the jewellery map: block compression blends region edges)
    n > 0 && ok * 100 >= n * 95 && lit * 2 >= n
}

/// sRGB colour of clothing that has no texture in the install and no colour
/// in its item either (see `colour_part`).
const BARE_FALLBACK: [u8; 3] = [88, 88, 88];

/// Colour the ped clothing materials that have no base texture (their
/// textures are not in the install; `glb` marks them `ped_shade`) with a
/// vertex tint, `colour(raw vertex colour)` in sRGB. Returns whether any
/// was coloured.
fn bare_tint(m: &mut glb::Model, colour: impl Fn([u8; 4]) -> [u8; 3]) -> bool {
    let lin = |v: u8| {
        let x = v as f32 / 255.0;
        ((if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }) * 255.0 + 0.5) as u8
    };
    let bare: Vec<usize> = (0..m.materials.len()).filter(|&k| m.materials[k].ped_shade && m.materials[k].base.is_none()).collect();
    for &k in &bare {
        let a = m.materials[k].base_factor[3];
        m.materials[k].base_factor = [1.0, 1.0, 1.0, a];
    }
    for me in m.meshes.iter_mut().filter(|me| me.material.is_some_and(|k| bare.contains(&k))) {
        for v in &mut me.verts {
            let c = colour(v.col);
            v.tint = [lin(c[0]), lin(c[1]), lin(c[2]), 255];
        }
    }
    !bare.is_empty()
}

/// Attached accessories (a watch, a ring) are modelled at their own root
/// bone (`Watch_Root`, at the authoring origin under the feet) and carry
/// attach bones under the body's bones (`Watch_L_Wrist` under the wrist)
/// that no vertex uses (`sixng_core::outfit::accessory_attach`). The game
/// moves the root onto an attach bone; do the same in the bind pose, so the
/// part sits on the body in the merged model. Parts modelled in place
/// (their vertices are near the bones that move them) are left alone.
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
    let names: Vec<&str> = m.joints.iter().map(|j| j.name.as_str()).collect();
    let parents: Vec<Option<usize>> = m.joints.iter().map(|j| j.parent).collect();
    let pos: Vec<[f32; 3]> = (0..m.joints.len()).map(|i| bind(i).transform_point3(Vec3::ZERO).to_array()).collect();
    let c = (Vec3::from(m.bbox[0]) + Vec3::from(m.bbox[1])) * 0.5;
    let (root, at) = sixng_core::outfit::accessory_attach(&names, &parents, &pos, &used, c.to_array())?;
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

/// The head's jaw shape keys on the teeth, tongue and inner mouth (see
/// `sixng_core::jawkeys`): the vertices skinned under `FB_Jaw` get a key of
/// the same name that moves them with the chin. The model is Y up, Z forward;
/// the fit works Z up, -Y forward. Returns the number of keys, or `None`
/// when the model has no jaw bone, head keys or teeth.
fn teeth_follow_jaw(m: &mut glb::Model) -> Option<usize> {
    use sixng_core::jawkeys;
    let jaw = m.joints.iter().position(|j| j.name.eq_ignore_ascii_case("FB_Jaw"))?;
    let under: Vec<bool> = (0..m.joints.len())
        .map(|i| std::iter::successors(Some(i), |&x| m.joints[x].parent.filter(|&p| p != x)).take(256).any(|x| x == jaw))
        .collect();
    let zup = |p: [f32; 3]| [p[0], -p[2], p[1]];
    let yup = |p: [f32; 3]| [p[0], p[2], -p[1]];
    let pivot = zup(glam::Mat4::from_cols_array(&m.joints[jaw].ibm).inverse().w_axis.truncate().to_array());
    let weight = |v: &glb::Vertex| -> f32 {
        let all: u32 = v.weights.iter().map(|&w| w as u32).sum();
        let on: u32 = (0..4).filter(|&k| under.get(v.joints[k] as usize).copied().unwrap_or(false)).map(|k| v.weights[k] as u32).sum();
        if all > 0 { on as f32 / all as f32 } else { 0.0 }
    };
    let jaw_w: Vec<Option<Vec<f32>>> = m.meshes.iter().map(|me| me.skinned.then(|| me.verts.iter().map(weight).collect::<Vec<f32>>()).filter(|w| w.iter().any(|&x| x > 0.0))).collect();
    let head = m.meshes.iter().position(|me| me.morphs.iter().any(|x| x.0.starts_with("face_")))?;
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for (mi, me) in m.meshes.iter().enumerate().filter(|x| x.0 != head) {
        let Some(w) = &jaw_w[mi] else { continue };
        for (v, &x) in me.verts.iter().zip(w) {
            if x > 0.5 {
                let p = zup(v.pos);
                for a in 0..3 {
                    lo[a] = lo[a].min(p[a]);
                    hi[a] = hi[a].max(p[a]);
                }
            }
        }
    }
    if lo[0] > hi[0] {
        return None;
    }
    let hm = &m.meshes[head];
    let hpos: Vec<[f32; 3]> = hm.verts.iter().map(|v| zup(v.pos)).collect();
    let hmorphs: Vec<(String, Vec<[f32; 3]>)> = hm.morphs.iter().map(|(n, d)| (n.clone(), d.iter().map(|&x| zup(x)).collect())).collect();
    let hw = jaw_w[head].clone().unwrap_or_else(|| vec![0.0; hpos.len()]);
    let (_, keys) = jawkeys::fit_keys(pivot, &hpos, &hw, &hmorphs, lo, hi).ok()?;
    if keys.is_empty() {
        return Some(0);
    }
    for (mi, me) in m.meshes.iter_mut().enumerate() {
        let Some(w) = &jaw_w[mi] else { continue };
        let pos: Vec<[f32; 3]> = me.verts.iter().map(|v| zup(v.pos)).collect();
        let mut morphs: Vec<(String, Vec<[f32; 3]>)> = me.morphs.iter().map(|(n, d)| (n.clone(), d.iter().map(|&x| zup(x)).collect())).collect();
        jawkeys::add_keys(&mut morphs, &keys, pivot, &pos, w);
        me.morphs = morphs.into_iter().map(|(n, d)| (n, d.into_iter().map(yup).collect())).collect();
    }
    Some(keys.len())
}

/// How far out each shown garment is worn, for hiding what is under it:
/// tops, bottoms and one-pieces start at 1 and jackets at 8; an item whose
/// hide masks name the region layer of another shown garment (an open
/// shirt naming the t-shirt under it) is put one level over that one.
fn garment_levels(shown: impl Iterator<Item = u64>, fx: &HashMap<u64, PartFx>) -> HashMap<u64, u8> {
    let shown: Vec<u64> = shown.filter(|o| fx.get(o).is_some_and(|f| f.layer > 0)).collect();
    let mut level: HashMap<u64, u8> = shown.iter().map(|o| (*o, if fx[o].layer == 2 { 8 } else { 1 })).collect();
    // a few rounds settle chains (jacket over shirt over t-shirt); two items
    // that name each other's layer say nothing about which is over
    for _ in 0..shown.len() {
        let mut changed = false;
        for &x in &shown {
            for &y in &shown {
                let (fx_x, fx_y) = (&fx[&x], &fx[&y]);
                if x != y && fx_y.region_layer != 0 && fx_x.hides.iter().any(|h| h.0 == fx_y.region_layer)
                    && !(fx_x.region_layer != 0 && fx_y.hides.iter().any(|h| h.0 == fx_x.region_layer))
                    && level[&x] <= level[&y]
                {
                    level.insert(x, level[&y] + 1);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    level
}

/// A skin mesh that carries no regions (an anatomical insert: the genitals,
/// the nipples) takes, per vertex, the region of the nearest vertex of a body
/// skin mesh of the same layer that does, so the hide masks that cover that
/// skin cover the insert too. Returns how many vertices took a region.
fn inherit_skin_regions(m: &mut glb::Model, region_parts: &[(std::ops::Range<usize>, u32, u64)], skin: &[std::ops::Range<usize>]) -> usize {
    let has = |m: &glb::Model, r: &std::ops::Range<usize>| r.clone().filter_map(|i| m.meshes.get(i)).any(|me| me.verts.iter().any(|v| v.zone[3] != 0));
    let mut n = 0;
    for (range, layer, _) in region_parts.iter().filter(|p| skin.contains(&p.0)) {
        if has(m, range) {
            continue;
        }
        let donors: Vec<([f32; 3], u8)> = region_parts
            .iter()
            .filter(|p| p.1 == *layer && p.0 != *range && skin.contains(&p.0))
            .flat_map(|p| p.0.clone())
            .filter_map(|i| m.meshes.get(i))
            .flat_map(|me| me.verts.iter().filter(|v| v.zone[3] != 0).map(|v| (v.pos, v.zone[3])))
            .collect();
        if donors.is_empty() {
            continue;
        }
        for i in range.clone() {
            let Some(me) = m.meshes.get_mut(i) else { continue };
            for v in &mut me.verts {
                let d = |q: [f32; 3]| (q[0] - v.pos[0]).powi(2) + (q[1] - v.pos[1]).powi(2) + (q[2] - v.pos[2]).powi(2);
                if let Some(z) = donors.iter().min_by(|a, b| d(a.0).total_cmp(&d(b.0))).map(|x| x.1) {
                    v.zone[3] = z;
                    n += 1;
                }
            }
        }
    }
    n
}

/// Keeps the `inner` meshes behind the `outer` ones after both followed the
/// shaped skin. An inner vertex that lay behind an outer garment's surface on
/// the neutral body (an outer vertex within 6 cm, at most 2 cm to the side
/// along that vertex's normal, turned to face the way the inner vertex does)
/// is put back as far behind it; with several outer surfaces there (a
/// jacket's shell over its pocket bag), the one that asks most; the nearest
/// outer vertex counts however far to the side it is. Vertices only
/// move inward. `unshaped` holds each mesh's positions before shaping.
/// Returns how many vertices moved.
fn keep_layer_order(m: &mut glb::Model, inner: &[std::ops::Range<usize>], outer: &[std::ops::Range<usize>], unshaped: &HashMap<usize, Vec<[f32; 3]>>) -> usize {
    use glam::Vec3;
    const REACH: f32 = 0.06;
    const SIDE: f32 = 0.02;
    let key = |p: Vec3| ((p.x / REACH).floor() as i32, (p.y / REACH).floor() as i32, (p.z / REACH).floor() as i32);
    // outer vertices: neutral position, shaped position, normal
    let mut grid: HashMap<(i32, i32, i32), Vec<(Vec3, Vec3, Vec3)>> = HashMap::new();
    for i in outer.iter().cloned().flatten() {
        let (Some(me), Some(pre)) = (m.meshes.get(i), unshaped.get(&i)) else { continue };
        for (v, q) in me.verts.iter().zip(pre) {
            let n = Vec3::from(v.nrm);
            if (n.length() - 1.0).abs() < 0.1 {
                grid.entry(key(Vec3::from(*q))).or_default().push((Vec3::from(*q), Vec3::from(v.pos), n));
            }
        }
    }
    if grid.is_empty() {
        return 0;
    }
    let mut moved = 0;
    for i in inner.iter().cloned().flatten() {
        let (Some(me), Some(pre)) = (m.meshes.get_mut(i), unshaped.get(&i)) else { continue };
        for (v, p) in me.verts.iter_mut().zip(pre) {
            let (p, p2, vn) = (Vec3::from(*p), Vec3::from(v.pos), Vec3::from(v.nrm));
            let k = key(p);
            let mut push: Option<(f32, Vec3)> = None;
            let mut nearest: Option<(f32, (Vec3, Vec3, Vec3))> = None;
            // a two-sided garment's inner face points at the body: take the
            // side facing the way the inner garment does (outward). Returns
            // how far the vertex is to move back behind that surface.
            let need = |(q, q2, n): (Vec3, Vec3, Vec3), side: f32| -> Option<(f32, Vec3)> {
                let n = if n.dot(vn) < 0.0 { -n } else { n };
                let d = p - q;
                let s = d.dot(n);
                if s >= 0.0 || (d - n * s).length() > side {
                    return None;
                }
                Some(((p2 - q2).dot(n) - s, n)).filter(|x| x.0 > 0.0)
            };
            for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        for &o in grid.get(&(k.0 + x, k.1 + y, k.2 + z)).into_iter().flatten() {
                            let d2 = (p - o.0).length_squared();
                            if d2 >= REACH * REACH {
                                continue;
                            }
                            if nearest.is_none_or(|b| d2 < b.0) {
                                nearest = Some((d2, o));
                            }
                            if let Some(w) = need(o, SIDE).filter(|w| push.is_none_or(|b| w.0 > b.0)) {
                                push = Some(w);
                            }
                        }
                    }
                }
            }
            // the nearest outer vertex counts however far to the side it is
            if let Some(w) = nearest.and_then(|(_, o)| need(o, f32::MAX)).filter(|w| push.is_none_or(|b| w.0 > b.0)) {
                push = Some(w);
            }
            if let Some((need, n)) = push {
                v.pos = (p2 - n * need).to_array();
                moved += 1;
            }
        }
    }
    moved
}

/// Hide the skin triangles that lie right under a garment: a skin vertex is
/// covered when a point of a garment surface is within 9 mm of the line
/// through the vertex along its normal, between `inside` (15 mm for skin;
/// 30 mm for clothes under clothes, which poke further through at the
/// armpits) inside and 50 mm outside the skin; a triangle goes when all
/// three vertices are covered. Returns the number of triangles hidden.
fn hide_covered_skin(m: &mut glb::Model, skin: &[std::ops::Range<usize>], cover: &[std::ops::Range<usize>], inside: f32) -> usize {
    use glam::Vec3;
    let Some(grid) = CoverGrid::new(m, cover) else { return 0 };
    let covered = |p: Vec3, n: Vec3| grid.covered(p, n, inside);
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

/// Points on garment surfaces, about 8 mm apart, in 2 cm cells.
struct CoverGrid {
    grid: HashMap<(i32, i32, i32), Vec<glam::Vec3>>,
}

impl CoverGrid {
    const CELL: f32 = 0.02;
    const STEP: f32 = 0.008;

    fn key(p: glam::Vec3) -> (i32, i32, i32) {
        ((p.x / Self::CELL).floor() as i32, (p.y / Self::CELL).floor() as i32, (p.z / Self::CELL).floor() as i32)
    }

    /// None when the meshes have no triangles.
    fn new(m: &glb::Model, cover: &[std::ops::Range<usize>]) -> Option<CoverGrid> {
        use glam::Vec3;
        let mut grid: HashMap<(i32, i32, i32), Vec<Vec3>> = HashMap::new();
        for me in cover.iter().cloned().flatten().filter_map(|i| m.meshes.get(i)) {
            for t in me.indices.chunks_exact(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(me.verts[i as usize].pos));
                let n = (((b - a).length().max((c - a).length()).max((c - b).length()) / Self::STEP).ceil() as usize).clamp(1, 40);
                for i in 0..=n {
                    for j in 0..=(n - i) {
                        let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
                        let p = a + (b - a) * u + (c - a) * v;
                        grid.entry(Self::key(p)).or_default().push(p);
                    }
                }
            }
        }
        (!grid.is_empty()).then_some(CoverGrid { grid })
    }

    /// A garment surface point within 9 mm of the line through `p` along
    /// `n`, between `inside` inside and 50 mm outside.
    fn covered(&self, p: glam::Vec3, n: glam::Vec3, inside: f32) -> bool {
        let k = Self::key(p);
        // cells within 50 mm
        for x in -3..=3 {
            for y in -3..=3 {
                for z in -3..=3 {
                    let Some(v) = self.grid.get(&(k.0 + x, k.1 + y, k.2 + z)) else { continue };
                    for g in v {
                        let d = *g - p;
                        let along = d.dot(n);
                        if (-inside..=0.05).contains(&along) && (d - n * along).length_squared() < 0.009 * 0.009 {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

/// Hide an anatomical insert (`PartFx::insert`) whole where a garment whose
/// hide masks name the insert's layer lies over most of it (half or more of
/// its vertices, `CoverGrid::covered` from 30 mm inside): the insert stands
/// out of the body and comes through a loose garment (a towel) whose masks
/// leave the skin under it. Returns the number of triangles hidden.
fn hide_covered_inserts(m: &mut glb::Model, inserts: &[(std::ops::Range<usize>, u32)], garments: &[(std::ops::Range<usize>, Vec<u32>)]) -> usize {
    use glam::Vec3;
    let mut hidden = 0;
    for (range, layer) in inserts {
        let cover: Vec<std::ops::Range<usize>> = garments.iter().filter(|g| g.1.contains(layer) && g.0 != *range).map(|g| g.0.clone()).collect();
        let Some(grid) = CoverGrid::new(m, &cover) else { continue };
        let (mut n, mut hit) = (0usize, 0usize);
        for me in range.clone().filter_map(|i| m.meshes.get(i)) {
            for v in &me.verts {
                n += 1;
                hit += grid.covered(Vec3::from(v.pos), Vec3::from(v.nrm).normalize_or_zero(), 0.03) as usize;
            }
        }
        if n > 0 && hit * 2 >= n {
            for i in range.clone() {
                if let Some(me) = m.meshes.get_mut(i) {
                    hidden += me.indices.len() / 3;
                    me.indices.clear();
                }
            }
        }
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
    /// Skin vertices an offset is taken from.
    const NEAR: usize = 16;
    /// Distance (m) below which nearer skin vertices weigh no more.
    const SOFT: f32 = 0.02;

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

    /// Offset at a point: the mean of the `NEAR` nearest skin vertices
    /// (searched up to about 30 cm away), weighted 1 / (d² + `SOFT`²). The
    /// softening keeps neighbouring garment points from each taking the
    /// offset of a different single skin vertex, which on a heavy body
    /// left hems jagged. With the point's normal, the offset along it is at
    /// least the largest of those vertices', so where the skin swells
    /// unevenly (a belly at full weight) a garment is not left inside it.
    fn at(&self, p: [f32; 3], n: Option<[f32; 3]>) -> [f32; 3] {
        let best = self.nearest(p);
        if best.is_empty() {
            return [0.0; 3];
        }
        let (mut o, mut wsum) = ([0f32; 3], 0f32);
        for (d2, i) in &best {
            let w = 1.0 / (d2 + Self::SOFT * Self::SOFT);
            let d = self.pts[*i as usize].1;
            for c in 0..3 {
                o[c] += d[c] * w;
            }
            wsum += w;
        }
        let mut o = o.map(|x| x / wsum);
        if let Some(n) = n.filter(|n| (n[0] * n[0] + n[1] * n[1] + n[2] * n[2] - 1.0).abs() < 0.1) {
            let dot = |d: [f32; 3]| d[0] * n[0] + d[1] * n[1] + d[2] * n[2];
            let out = best.iter().map(|(_, i)| dot(self.pts[*i as usize].1)).fold(f32::MIN, f32::max);
            let gap = out - dot(o);
            if out > 0.0 && gap > 0.0 {
                for c in 0..3 {
                    o[c] += n[c] * gap;
                }
            }
        }
        o
    }

    /// The `NEAR` nearest skin vertices (squared distance, index), nearest
    /// first; searched up to about 30 cm away.
    fn nearest(&self, p: [f32; 3]) -> Vec<(f32, u32)> {
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
            if best.iter().filter(|b| b.0 <= sure).count() >= Self::NEAR || r >= 8 {
                break;
            }
            r += if best.is_empty() { 2 } else { 1 };
        }
        best.sort_by(|a, b| a.0.total_cmp(&b.0));
        best.truncate(Self::NEAR);
        best
    }

    /// For a part moved as one piece: how much further out than `rigid` the
    /// skin under `p` swelled, along the way from the nearest skin vertex to
    /// `p`; zero where it did not. A necklace on a neck thicker than the
    /// neutral one is opened up by it, so it stays on the skin.
    fn swell(&self, p: [f32; 3], rigid: [f32; 3]) -> [f32; 3] {
        use glam::Vec3;
        let Some(&(_, i)) = self.nearest(p).first() else { return [0.0; 3] };
        let u = (Vec3::from(p) - Vec3::from(self.pts[i as usize].0)).normalize_or_zero();
        if u == Vec3::ZERO {
            return [0.0; 3];
        }
        let more = (Vec3::from(self.at(p, Some(u.to_array()))) - Vec3::from(rigid)).dot(u);
        if more > 0.0 { (u * more).to_array() } else { [0.0; 3] }
    }

    /// Move a part with the skin. Small parts (a watch, glasses, a ring)
    /// move as one piece by the offset at their centre, so they keep their
    /// shape, and only go further out where the skin under them swelled
    /// more (`swell`); larger ones move point by point.
    fn apply(&self, m: &mut glb::Model) {
        use glam::Vec3;
        let size = (Vec3::from(m.bbox[1]) - Vec3::from(m.bbox[0])).length();
        let rigid = (size < 0.3).then(|| self.at(((Vec3::from(m.bbox[0]) + Vec3::from(m.bbox[1])) * 0.5).to_array(), None));
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for me in &mut m.meshes {
            let mut sum = Vec3::ZERO;
            for v in &mut me.verts {
                let o = match rigid {
                    Some(r) => {
                        let s = self.swell(v.pos, r);
                        [r[0] + s[0], r[1] + s[1], r[2] + s[2]]
                    }
                    None => self.at(v.pos, Some(v.nrm)),
                };
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

/// Skin matrix (posed world * inverse bind) of a joint by name: how the
/// pose a part's GLB stores moves that bone. `None` without that joint.
fn joint_skin(m: &glb::Model, name: &str) -> Option<glam::Mat4> {
    use glam::Mat4;
    let i = m.joints.iter().position(|j| j.name == name)?;
    let mut w = Mat4::from_cols_array(&m.joints[i].local);
    let (mut k, mut depth) = (i, 0);
    while let Some(p) = m.joints[k].parent.filter(|&p| p < m.joints.len() && p != k && depth < 256) {
        w = Mat4::from_cols_array(&m.joints[p].local) * w;
        k = p;
        depth += 1;
    }
    Some(w * Mat4::from_cols_array(&m.joints[i].ibm))
}

/// Whether joint `i` is SKEL_Head or hangs under it.
fn under_head(m: &glb::Model, mut i: usize) -> bool {
    for _ in 0..256 {
        if m.joints.get(i).is_none() {
            return false;
        }
        if m.joints[i].name == "SKEL_Head" {
            return true;
        }
        match m.joints[i].parent.filter(|&p| p < m.joints.len() && p != i) {
            Some(p) => i = p,
            None => return false,
        }
    }
    false
}

/// Whether every bone a part's vertices use hangs under `SKEL_Head` (an
/// earring, glasses): such a part follows the head's own shaping.
fn on_head(m: &glb::Model) -> bool {
    let under = |i: usize| under_head(m, i);
    let mut any = false;
    for v in m.meshes.iter().filter(|me| me.skinned).flat_map(|me| &me.verts) {
        for k in 0..4 {
            if v.weights[k] > 0 {
                let j = v.joints[k] as usize;
                if j >= m.joints.len() || !under(j) {
                    return false;
                }
                any = true;
            }
        }
    }
    any
}

/// Move a part rigidly by a matrix (positions, normals, centres, bounds).
fn transform_part(m: &mut glb::Model, s: glam::Mat4) {
    use glam::Vec3;
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for me in &mut m.meshes {
        let mut sum = Vec3::ZERO;
        for v in &mut me.verts {
            let p = s.transform_point3(Vec3::from(v.pos));
            v.pos = p.to_array();
            v.nrm = s.transform_vector3(Vec3::from(v.nrm)).normalize_or_zero().to_array();
            sum += p;
            lo = lo.min(p);
            hi = hi.max(p);
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
        .filter_map(|off| match sixng_core::packparts::is_pack_key(*off) {
            // a content-pack part (see `packparts`)
            true => sixng_core::packparts::loaded().and_then(|t| t.entries.iter().find(|e| e.off == *off)),
            false => ix.entries.iter().find(|e| e.off == *off),
        })
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
                // A garment another shown item hides regions of (a t-shirt under an open shirt) is drawn
                // as its `_cull_00` twin: the same garment cut a little closer to the body, whose vertices
                // carry the regions the hide masks name (the `_full` meshes carry none).
                let swap: HashMap<String, String> = if std::env::var("SIXNG_NO_CULL").is_err() {
                    let names: std::collections::HashSet<String> = ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| e.name.as_ref().map(|n| n.to_ascii_lowercase())).collect();
                    sixng_core::pedlook::cull_twins(&look.outfits[k].parts, |n| names.contains(n))
                } else {
                    HashMap::new()
                };
                let by_name = {
                    let mut b = by_name;
                    if !swap.is_empty() {
                        b.extend(sixng_core::outfit::dedupe(ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| {
                            let n = e.name.as_ref()?.to_ascii_lowercase();
                            swap.values().any(|t| *t == n).then_some((n, e))
                        })));
                    }
                    // the parts that are in a content pack (the anatomical inserts): their entries carry a
                    // part key (`packparts::KEY_BIT`) and are loaded from their own archive
                    let packs = sixng_core::packparts::get(&ws.game);
                    for o in &look.outfits {
                        for p in &o.parts {
                            if !b.contains_key(&p.drawable) {
                                if let Some(e) = packs.by_name(&p.drawable) {
                                    b.insert(p.drawable.clone(), e);
                                }
                            }
                        }
                    }
                    b
                };
                opts.iter_mut().for_each(|o| o.default = false);
                let mut info = LookInfo { model: look.model.clone(), labels: look.outfits.iter().map(|o| o.label.clone()).collect(), current: k, ..Default::default() };
                info.notes.extend(look.note.clone());
                let display = sixng_core::pedlook::model_names(&ws.game).ok().and_then(|n| n.display.get(&sixng_core::pedlook::model_hash(&look.model)).cloned());
                info.parts = Some(Arc::new(sixng_core::pedset::part_set(&look, display)));
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
                                if let Some(h) = p.layer_set.as_deref().and_then(|h| u32::from_str_radix(h, 16).ok()) {
                                    info.strand_layer_sets.push((p.slot.clone(), h));
                                }
                            }
                            continue;
                        }
                        let drawable = if oi == k { swap.get(&p.drawable).unwrap_or(&p.drawable) } else { &p.drawable };
                        let Some(e) = by_name.get(drawable) else { continue };
                        match opts.iter_mut().find(|x| x.off == e.off) {
                            Some(x) => {
                                x.group = p.slot.clone();
                                x.default |= oi == k;
                            }
                            None => opts.push(crate::assemble::PartOpt { off: e.off, id: e.id.clone(), name: drawable.clone(), group: p.slot.clone(), label: drawable.trim_start_matches(&format!("{prefix}_")).to_string(), default: oi == k }),
                        }
                        if oi == k {
                            // the anatomical inserts are skin too: the body's skin tone, under the clothes
                            let body = p.slot == "body upper" || p.slot == "body lower" || sixng_core::pedlook::is_skin_insert(&p.slot);
                            if let Some(how) = p.skin_from.filter(|_| body) {
                                let why = match how {
                                    "paired" => "the one other characters wear with this outfit's other body skin",
                                    "tone" => "the one of other characters' body sets whose colour is nearest the face",
                                    _ => "the one other characters' body sets wear most",
                                };
                                info.notes.push(format!("Body skin ({}): approximate. The outfit data read here names no body set for this character, so the skin is {why}.", p.slot));
                            }
                            fx.insert(e.off, PartFx { tint: p.tint.as_ref().and_then(|t| Some((u32::from_str_radix(&t.palette, 16).ok()?, t.rows[0]))).filter(|t| t.1 != 255), colour: p.colour.map(|c| [c[1], c[2], c[3]]), tag: p.colour_tag, subs: p.preset_colours.clone(), preset: p.preset.as_deref().and_then(|h| u32::from_str_radix(h, 16).ok()), head: p.slot == "head", body, insert: sixng_core::pedlook::is_skin_insert(&p.slot), skeletal: body || matches!(p.slot.as_str(), "head" | "eyes" | "teeth" | "teeth accessory"), covers: matches!(p.slot.as_str(), "top" | "outer" | "bottom" | "full" | "feet" | "gloves"), layer: match p.slot.as_str() { "outer" => 2, "top" | "bottom" | "full" => 1, _ => 0 }, heel: p.slot == "feet" && p.drawable.split('_').any(|t| matches!(t, "sh" | "mh" | "hh")), region_layer: p.layer.as_deref().and_then(|l| u32::from_str_radix(l, 16).ok()).unwrap_or(match p.slot.as_str() { "body upper" => 0xb4d9_d470, "body lower" => 0xe062_2b84, _ => 0 }), hides: p.hides.iter().filter_map(|(l, m)| Some((u32::from_str_radix(l, 16).ok()?, m.clone()))).collect(), hides_sub: p.hides_sub.iter().filter_map(|(l, k, m)| Some((u32::from_str_radix(l, 16).ok()?, *k, m.clone()))).collect(), under: Vec::new(), decorations: p.decorations.iter().filter_map(|n| u32::from_str_radix(n, 16).ok()).collect(), skin_tone: p.skin_tone.as_deref().and_then(|t| u32::from_str_radix(t, 16).ok()).unwrap_or(0), skins: if p.slot == "head" || body { p.skins.iter().filter_map(|(a, n)| Some((u64::from_str_radix(a, 16).ok()?, u64::from_str_radix(n, 16).unwrap_or(0)))).collect() } else { Vec::new() } });
                        }
                    }
                }
                // The painted hair of the head skin: the layers of each hair and brow item's layer set (its tag 023e5a2f
                // along the item's chain, often but not always the placeholder's name; the name where the tag is
                // missing), slot i coloured by preset i of the item's colour preset set (its
                // tag f7b14eb8). That the game's per-character entry is filled from these two tags is read from the data,
                // not from its code. SIXNG_NO_UNDER=1 leaves the layers out.
                if std::env::var("SIXNG_NO_UNDER").is_err() {
                    let table = sixng_core::hairtone::Table::load_or_build(&ws).ok();
                    let mut under = Vec::new();
                    for (slot, drawable, _) in &info.strands {
                        let Some(t) = table.as_ref() else { continue };
                        let Some(set) = info.strand_presets.iter().find(|p| p.0 == *slot).map(|p| p.1) else { continue };
                        let key = info.strand_layer_sets.iter().find(|p| p.0 == *slot).map(|p| p.1).unwrap_or_else(|| sixng_core::meta7a::joaat(drawable.to_ascii_lowercase().trim_end_matches("_full")));
                        for l in t.under_layers(key) {
                            // a preset with its flag set draws the skin's own colour: nothing to paint
                            let Some(p) = t.slot_preset(set, l.slot).filter(|p| !p.flag) else { continue };
                            under.push(UnderPaint { layer: l.clone(), a: p.a.map(|x| x as f32 / 255.0), b: p.b.map(|x| x as f32 / 255.0) });
                        }
                    }
                    if !under.is_empty() {
                        info.notes.push(format!("Painted hair on the head skin: {} layer(s) of the hair and brow items, drawn as the game's head shader does (density from the mask, colour from the item's two-colour preset). Left out: the layers' normal maps and material values.", under.len()));
                        fx.values_mut().filter(|f| f.head).for_each(|f| f.under = under.clone());
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
                        let asset_sets = sixng_core::hairsets::get(&ws.work, &a, &ix.entries);
                        for (slot, placeholder, guid) in &info.strands {
                            // by the item's hair asset, else by the character's own placeholder
                            // research: SIXNG_STRANDS_BY_NAME=1 takes the placement under the item's own placeholder name before the asset's best one
                            let by_name = std::env::var("SIXNG_STRANDS_BY_NAME").is_ok().then(|| t.by_placeholder(placeholder)).flatten();
                            let Some(l) = by_name.or_else(|| u64::from_str_radix(guid, 16).ok().and_then(|g| t.resolve(g, look.fit.as_deref()))).or_else(|| guid.is_empty().then(|| t.by_placeholder(placeholder)).flatten()) else { continue };
                            let colour = info.strand_tints.iter().find(|t| t.0 == *slot && t.1 == 0x1e64_f584).and_then(|t| Some(sixng_core::pedtint::palettes(&ws.game).ok()?.get(&t.1)?.sample(t.2, 128)));
                            // presets whose colours are white are not hair colours (their flag is set)
                            let tone = info.strand_presets.iter().find(|p| p.0 == *slot).and_then(|p| Some((p.1, presets.as_ref()?.variants(p.1)?))).filter(|(_, v)| !v[0].flag).map(|(name, variants)| HairTone { name, set: { let cat = match slot.as_str() { "hair" => "864b03ae", "brows" => "7c50ad43", "lashes" => "83a664e7", "beard" => "44d2709f", "body hair" => "a228cc57", _ => "" }; look.outfits[k].strand_colours.iter().find(|c| c.0 == cat).and_then(|c| u32::from_str_radix(&c.1, 16).ok()).unwrap_or(name) }, slot: slot.clone(), param: 0, variants });
                            // a piece whose item names no colour preset (lashes): the colour set the outfit's colour item gives its
                            // category, when there is one; the stand-in colours here are only used without the strand tables
                            let tone = tone.or_else(|| {
                                let cat = match slot.as_str() { "hair" => "864b03ae", "brows" => "7c50ad43", "lashes" => "83a664e7", "beard" => "44d2709f", "body hair" => "a228cc57", _ => return None };
                                let set = look.outfits[k].strand_colours.iter().find(|c| c.0 == cat).and_then(|c| u32::from_str_radix(&c.1, 16).ok())?;
                                let dark = sixng_core::hairtone::Preset { a: [20, 14, 10], b: [20, 14, 10], texture: 0, flag: false, values: Default::default() };
                                Some(HairTone { name: set, set, slot: slot.clone(), param: 0, variants: [dark.clone(), dark.clone(), dark.clone(), dark] })
                            });
                            // the strand sets the hair asset itself names: its parameter set, and its colour set for a
                            // piece whose item and outfit name none
                            let asset = u64::from_str_radix(guid, 16).ok().and_then(|g| asset_sets.of(g));
                            let tone = tone.or_else(|| {
                                let set = asset.map(|s| s.colour).filter(|&c| c != 0)?;
                                let dark = sixng_core::hairtone::Preset { a: [20, 14, 10], b: [20, 14, 10], texture: 0, flag: false, values: Default::default() };
                                Some(HairTone { name: set, set, slot: slot.clone(), param: 0, variants: [dark.clone(), dark.clone(), dark.clone(), dark] })
                            });
                            let tone = tone.map(|mut t| {
                                t.param = asset.map_or(0, |s| s.param);
                                t
                            });
                            let colour = colour.or(tone.as_ref().map(|t| sixng_core::hairtone::display(t.variants[0].a)));
                            info.hair.push(HairPick { slot: slot.clone(), placeholder: l.placeholder.clone(), confidence: l.confidence, exact: t.exact, grooms: t.uncontested(l), off_head: 0, shown: l.confidence >= 0.5 && slot != "body hair" && l.part != "body", colour, tone });
                            // research: SIXNG_HAIR_GROOMS=hair:10c5cc010+10c605010+... draws those pieces (first LOD offsets) for the slot
                            if let Ok(v) = std::env::var("SIXNG_HAIR_GROOMS") {
                                for part in v.split(',') {
                                    if let Some((sl, list)) = part.split_once(':') {
                                        if sl == slot.as_str() {
                                            if let Some(h) = info.hair.last_mut() {
                                                h.grooms = list.split('+').filter_map(|x| u64::from_str_radix(x, 16).ok()).collect();
                                            }
                                        }
                                    }
                                }
                            }
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
                            info.hair.push(HairPick { slot: format!("other {slot}"), placeholder: l.placeholder.clone(), confidence: l.confidence, exact: t.exact, grooms: t.uncontested(l), off_head: 0, shown: false, colour: None, tone: None });
                        }
                    }
                }
                let o = &look.outfits[k];
                info.body_shape = match (&o.body_shape, o.body_preset_items.len()) {
                    (Some(w), 0) => Some(w.clone()),
                    (Some(w), n) => Some(format!("{w}, {n} preset item(s) from anim.rpf")),
                    (None, 0) => None,
                    (None, n) => Some(format!("{n} preset item(s) from anim.rpf")),
                };
                // the outfit's own body shape, when it is opened (a reload of
                // the same outfit keeps the sliders; `--body` keeps its shape)
                info.body_preset = preset_shape(&ws.game, o.body_shape.as_deref(), &o.body_preset_items);
                let key = format!("{archive}#outfit:{prefix}#look={k}");
                let mut last = BODY_PRESET_FOR.lock().unwrap();
                if *last != key {
                    *last = key;
                    if !BODY_PINNED.load(std::sync::atomic::Ordering::Relaxed) {
                        *BODY_SHAPE.lock().unwrap() = info.body_preset.unwrap_or(NEUTRAL_BODY);
                    }
                }
                drop(last);
                info.body_used = Some(*BODY_SHAPE.lock().unwrap());
                if !o.unresolved.is_empty() {
                    info.notes.push(format!("{} item(s) of this outfit have no model: {}", o.unresolved.len(), o.unresolved.join(", ")));
                }
                look_info = Some(info);
            }
            Err(e) => {
                // Not in the game's outfit list (cutscene heads and the like):
                // its own parts are drawn. Its hair parts are strand-hair
                // placeholders, and their grooms are linked by the
                // placeholder's name, so they are drawn the same way.
                if let Ok(t) = sixng_core::hairlink::get(&ws.work, &a, &ix.entries) {
                    let mut info = LookInfo { model: prefix.to_string(), labels: vec!["own parts (not in the game's outfit list)".into()], ..Default::default() };
                    for o in opts.iter().filter(|o| o.default) {
                        let Some(l) = t.by_placeholder(&o.name.to_ascii_lowercase()) else { continue };
                        let slot = sixng_core::pedlook::slot_of(&l.placeholder);
                        let shown = l.confidence >= 0.5 && slot != "body hair" && l.part != "body";
                        info.hair.push(HairPick { slot, placeholder: l.placeholder.clone(), confidence: l.confidence, exact: t.exact, grooms: t.uncontested(l), off_head: 0, shown, colour: None, tone: None });
                    }
                    if !info.hair.is_empty() {
                        info.notes.push("Not in the game's outfit list: its own parts, with the strand hair linked to its hair parts by name. No hair colour is stored for it here: the hair takes the picker's colour.".into());
                        look_info = Some(info);
                    }
                }
                look_err = Some(e);
            }
        }
        if opts.is_empty() {
            return Err(anyhow::anyhow!("no parts found for {prefix}"));
        }
        let (selected, parts) = pick(&ix, &opts, &sel);
        if parts.is_empty() {
            return Err(anyhow::anyhow!("no parts of {prefix} selected"));
        }
        let hats = look_info.as_ref().map(|l| l.hats.clone()).unwrap_or_default();
        let (model, first, names, errs, skin, _, _, notes, hat_tris) = load_parts(&a, &parts, cache, lod, None, &fx, &ws.game, &hats);
        let model = model
            .ok_or_else(|| anyhow::anyhow!("no part of {prefix} loaded: {}", errs.join("; ")))?;
        if let Some(l) = &mut look_info {
            l.notes.extend(notes);
            // The hat as seen from inside the head: its centre is 10 cm
            // under the crown, in the middle of the top of the head.
            let is_head = |n: &str| n.contains("_head_") || n.starts_with("head_");
            // the head and hat come scaled by the body shape's head scale, the hair
            // is scaled with BM_Head only when drawn: back to the hair's space
            let unshape = model.extras["head_shape"].as_array().filter(|a| a.len() == 16).map(|a| {
                let mut c = [0f32; 16];
                c.iter_mut().zip(a).for_each(|(c, v)| *c = v.as_f64().unwrap_or(0.0) as f32);
                glam::Mat4::from_cols_array(&c).inverse()
            }).filter(|m| m.is_finite());
            let back = |p: [f32; 3]| unshape.map_or(p, |m| m.transform_point3(glam::Vec3::from(p)).to_array());
            let hskin: Vec<(String, Vec<[f32; 3]>)> = skin.iter().map(|(n, p)| (n.clone(), p.iter().map(|q| back(*q)).collect())).collect();
            let hat_tris: Vec<[[f32; 3]; 3]> = hat_tris.iter().map(|t| t.map(back)).collect();
            let hat: Vec<[f32; 3]> = hskin.iter().filter(|(n, _)| l.hats.contains(&n.to_ascii_lowercase())).flat_map(|(_, p)| p.iter().copied()).collect();
            let top = hskin.iter().filter(|(n, _)| is_head(n)).flat_map(|(_, p)| p.iter()).map(|p| p[2]).fold(f32::MIN, f32::max);
            if !hat.is_empty() && top > f32::MIN && !l.hair.is_empty() {
                let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                for p in hskin.iter().filter(|(n, _)| is_head(n)).flat_map(|(_, p)| p.iter()).filter(|p| p[2] > top - 0.12) {
                    for a in 0..2 {
                        lo[a] = lo[a].min(p[a]);
                        hi[a] = hi[a].max(p[a]);
                    }
                }
                let head: Vec<[f32; 3]> = hskin.iter().filter(|(n, _)| is_head(n)).flat_map(|(_, p)| p.iter().copied()).collect();
                l.hat = Some(Arc::new(sixng_core::groom::Shell::new([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, top - 0.10], &hat, &head).with_lining(hat_tris).with_crown(&hat)));
                l.notes.push("Hat: the scalp hair is kept inside the hat's crown while it settles, then pressed under the hat's lining (strand points beyond it are pulled in to 3 mm under it), so the hair does not come through the hat; an opening the hat encloses (over a cap's strap) counts as closed. A stand-in, not the game's method: the outfit asks for the plain hair, and no stored rule says how the game keeps it under the hat.".to_string());
            }
            // The style pass is damped near the body bound's pieces (head,
            // neck, shoulders, chest), as the game does; without one, near
            // the head, body and garments (hats press the hair on their
            // own; strand and card hair are not colliders).
            if !l.hair.is_empty() {
                let rig = |k: &str| parts.iter().any(|p| p.name.as_deref().is_some_and(|n| n.contains(k)));
                let bound = if rig("_fr1_") { Some(("defaultfemale_fr1_000_bounds", "female")) } else if rig("_mr1_") { Some(("defaultmale_mr1_000_bounds", "male")) } else { None };
                let (mut shapes, mut boxes) = bound.map(|(b, _)| body_tapers(&a, &ix.entries, b, &model.joints)).unwrap_or_default();
                // A garment's own bound is named after its drawable:
                // `char_bipedbounds_<sex>_<drawable>`. Its pieces are added
                // to the body's (they enclose it where the garment is).
                let mut worn = 0;
                if let (Some((_, sex)), false) = (bound, shapes.is_empty()) {
                    for n in parts.iter().filter_map(|p| p.name.as_deref()) {
                        let (t, bx) = body_tapers(&a, &ix.entries, &format!("char_bipedbounds_{sex}_{}", n.to_ascii_lowercase()), &model.joints);
                        worn += usize::from(!t.is_empty() || !bx.is_empty());
                        shapes.extend(t);
                        boxes.extend(bx);
                    }
                }
                if std::env::var("SIXNG_DEBUG_HAIR").is_ok() {
                    for t in &shapes {
                        eprintln!("body piece {:?} {:?} r {:.3} {:.3}", t.a, t.b, t.ra, t.rb);
                    }
                    for b in &boxes {
                        eprintln!("body box {:?} half {:.3?}", b.centre, b.half);
                    }
                }
                if !shapes.is_empty() {
                    l.notes.push(format!("Hair styling is damped near the {} round pieces and {} boxes of the body bound ({}) and of {worn} garment bounds, as the game does. A garment's bound is found by its name (`char_bipedbounds_<sex>_<garment>`); that the game damps hair against it is inferred, and the 3 cm falloff is a guess.", shapes.len(), boxes.len(), bound.map(|b| b.0).unwrap_or_default()));
                    l.collide = Some(Arc::new(sixng_core::groom::Colliders::shapes(shapes).with_boxes(boxes)));
                } else {
                    let hairy = ["hair", "brows", "beard", "lashes", "body hair", "eyes", "eyecaps", "teeth", "teeth accessory"];
                    let pts: Vec<[f32; 3]> = skin.iter().filter(|(n, _)| !l.hats.contains(&n.to_ascii_lowercase()) && !hairy.contains(&sixng_core::pedlook::slot_of(&n.to_ascii_lowercase()).as_str())).flat_map(|(_, p)| p.iter().copied()).collect();
                    if !pts.is_empty() {
                        l.collide = Some(Arc::new(sixng_core::groom::Colliders::new(&pts)));
                    }
                }
            }
            // Strand roots of an inferred link must lie on this character:
            // pieces that do not (another head shape, a neighbour's piece)
            // are left out. Only in the neutral body shape, where the skin is
            // where the grooms expect it. A link read from the game is kept whole.
            if *BODY_SHAPE.lock().unwrap() == NEUTRAL_BODY && !l.hair.is_empty() {
                const CELL: f32 = 0.012;
                let key = |p: &[f32; 3]| ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32, (p[2] / CELL).floor() as i32);
                // hair, brows, beards and lashes must root on the head; body
                // hair on the head or the body
                let is_head = |n: &str| n.contains("_head_") || n.starts_with("head_");
                let head_cells: std::collections::HashSet<(i32, i32, i32)> = skin.iter().filter(|(n, _)| is_head(n)).flat_map(|(_, pts)| pts.iter().map(key)).collect();
                let all_cells: std::collections::HashSet<(i32, i32, i32)> = skin.iter().filter(|(n, _)| is_head(n) || n.starts_with("hand_") || n.starts_with("feet_")).flat_map(|(_, pts)| pts.iter().map(key)).collect();
                if !head_cells.is_empty() {
                    for h in l.hair.iter_mut().filter(|h| !h.exact) {
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
                        // research: SIXNG_SHOW_STRANDS=lashes,... shows those slots' picks whatever their confidence
                        if std::env::var("SIXNG_SHOW_STRANDS").is_ok_and(|v| v.split(',').any(|x| x == h.slot)) && !h.grooms.is_empty() {
                            h.shown = true;
                        }
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
        let (m, _, names, _, skin, mut colours, mask, _, _) = load_parts(&a, &parts, cache, level, Some(model), &HashMap::new(), &ws.game, &[]);
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
    // first configuration are on by default. Boats have no wheel mounts
    // but fit parts the same way (the Rhode's outboard engine): a fragment
    // with a chassis bone counts as a vehicle too.
    let mut veh_opts = Vec::new();
    let has_chassis = info["drawable"]["skeleton"].as_array().is_some_and(|b| b.iter().any(|b| b["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case("chassis"))));
    if e.rtype.as_deref() == Some("ft") && (has_chassis || info["wheel_mounts"].as_array().is_some_and(|w| !w.is_empty())) {
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
/// overrides it. Film, sheen and anisotropy go by `coat_row`.
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
    export_load_skin(a, e, cache, lod, &[], (&[], 0))
}

/// `export_load` with the face textures an outfit names for a head
/// (`ExportOptions::part_skins`).
fn export_load_skin(
    a: &sixng_core::game::ArchiveInfo,
    e: &sixng_core::scan::Entry,
    cache: &Path,
    lod: usize,
    skins: &[(u64, u64)],
    decorations: (&[u32], u32),
) -> anyhow::Result<(glb::Model, Vec<PathBuf>, serde_json::Value)> {
    export_load_shaped(a, e, cache, lod, skins, decorations, NEUTRAL_BODY)
}

/// `export_load_skin` with a body shape (`BodyShape`). Shaped exports go
/// to their own folder, so the cached neutral models stay neutral.
fn export_load_shaped(
    a: &sixng_core::game::ArchiveInfo,
    e: &sixng_core::scan::Entry,
    cache: &Path,
    lod: usize,
    skins: &[(u64, u64)],
    (decorations, skin_tone): (&[u32], u32),
    shape: BodyShape,
) -> anyhow::Result<(glb::Model, Vec<PathBuf>, serde_json::Value)> {
    // a content-pack part of an outfit is read from its own archive
    let a = &*sixng_core::packparts::archive_of(a, e);
    let shaped = shape != NEUTRAL_BODY;
    let shaped_dir = cache.join("shaped");
    let cache = if shaped { shaped_dir.as_path() } else { cache };
    let body_shape = if shaped { sixng_core::bodyshape::parse_spec(&body_spec(&shape)).unwrap_or_default() } else { Vec::new() };
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
    // Cached character glow layers from before they stopped lighting by their colour map.
    let stale_glow = std::fs::read_to_string(dir.join("model.json")).is_ok_and(|j| (j.contains("b8916043") || j.contains("2e920e9f")) && j.contains("\"colour_map\""));
    // Cached animal parts from before part skins (eyes, tint-mask coats).
    let stale_animal = e.name.as_deref().is_some_and(|n| sixng_core::partskin::animal_root(n).is_some())
        && std::fs::read_to_string(dir.join("model.json")).is_ok_and(|j| !j.contains("\"part_skin\""));
    // Skin meshes are shared by many characters: baked decorations of an
    // earlier outfit must not stay in the cached export.
    let stale_deco = std::fs::read_to_string(dir.join("model.json")).is_ok_and(|j| j.contains("\"decorations\""));
    let opts = ExportOptions {
        out: cache.to_path_buf(),
        png: true,
        dds: false,
        raw: false,
        force: head || shaped || !skins.is_empty() || !decorations.is_empty() || stale_deco || stale_tint || stale_vehicle || stale_animal || stale_cloth || stale_tyre || stale_glow,
        skip_embedded_textures: false,
        body_shape,
        head_skin: HEAD_SKIN.load(std::sync::atomic::Ordering::Relaxed),
        paint: None,
        tex_remap: Default::default(), ped_tints: Default::default(),
        part_skins: e.name.as_ref().filter(|_| !skins.is_empty()).map(|n| (n.to_ascii_lowercase(), skins.to_vec())).into_iter().collect(),
        part_decorations: e.name.as_ref().filter(|_| !decorations.is_empty()).map(|n| (n.to_ascii_lowercase(), decorations.to_vec())).into_iter().collect(),
        skin_tone,
        part_colours: Default::default(),
        part_hidden: Default::default(), part_layers: Default::default(),
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
    fn a_tint_data_map_is_told_from_a_colour_map() {
        use super::packed_tint_map;
        let map = |texels: &[[u8; 4]]| texels.iter().flatten().copied().collect::<Vec<u8>>();
        // the jewellery map's orange, blue and green regions
        assert!(packed_tint_map(&map(&[[255, 110, 1, 255], [1, 110, 255, 255], [0, 110, 0, 254], [255, 107, 1, 255]])));
        // a black or a plain colour map is not one
        assert!(!packed_tint_map(&map(&[[0, 0, 0, 255]; 4])));
        assert!(!packed_tint_map(&map(&[[200, 150, 90, 255], [255, 110, 1, 255], [180, 120, 60, 255], [190, 140, 80, 255]])));
    }

    #[test]
    fn coat_records_in_the_ramp_image() {
        use super::coat_row;
        // no record: all zero, so the shader draws no coat
        assert!(coat_row(None).iter().all(|b| *b == 0));
        let r = serde_json::json!({
            "use_clear_coat": true, "clear_coat_intensity": 0.5, "clear_coat_roughness": 0.0,
            "use_flakes": false, "flake_coverage": 0.0, "flake_orientation": 0.0, "flake_size": 0.0,
            "override_base_coat_roughness": true, "base_coat_roughness": 0.7,
            "use_anisotropy": true, "anisotropy_intensity": 0.55,
            "sheen_intensity": 0.0, "sheen_fade": 1.0, "sheen_reflectance": 0.0,
            "film_ior": 2.6, "film_thickness": 0.2,
        });
        let row = coat_row(Some(&r));
        assert_eq!(row.len(), 128 * 4);
        let a: Vec<u8> = row.chunks_exact(4).map(|c| c[3]).collect();
        // columns: clear coat, its roughness, flakes (off), flake size, base
        // roughness, anisotropy, sheen (off: no fade or reflectance), film IOR / 4, thickness
        assert_eq!(&a[..11], &[128, 0, 0, 0, 179, 140, 0, 0, 0, 166, 51]);
        // repeated every 16 columns, with zeros between
        assert_eq!(a[16..27], a[..11]);
        assert!(a[11..16].iter().all(|b| *b == 0));
        // anisotropy without useAnisotropy is not drawn
        let mut off = r.clone();
        off["use_anisotropy"] = false.into();
        assert_eq!(coat_row(Some(&off))[5 * 4 + 3], 0);
    }

    #[test]
    fn garment_levels_follow_the_hide_masks() {
        let fx = |layer: u8, region: u32, hides: &[u32]| PartFx { layer, region_layer: region, hides: hides.iter().map(|&h| (h, String::new())).collect(), ..Default::default() };
        // an open shirt naming the t-shirt's layer, both tops, under a jacket that names neither
        let m: HashMap<u64, PartFx> = [(1, fx(1, 0xa, &[])), (2, fx(1, 0xb, &[0xa])), (3, fx(2, 0xc, &[])), (4, fx(0, 0, &[0xa]))].into_iter().collect();
        let l = garment_levels([1, 2, 3, 4].into_iter(), &m);
        assert_eq!((l[&1], l[&2], l[&3]), (1, 2, 8));
        assert!(!l.contains_key(&4));
        // items naming each other's layer stay level
        let m: HashMap<u64, PartFx> = [(1, fx(1, 0xa, &[0xb])), (2, fx(1, 0xb, &[0xa]))].into_iter().collect();
        assert!(garment_levels([1, 2].into_iter(), &m).values().all(|&v| v == 1));
    }

    #[test]
    fn primer_set_from_the_mask() {
        use super::{primer_set, primer_set_masks};
        // one allowed set: always that one
        assert_eq!(primer_set(1, 2), 0);
        assert_eq!(primer_set(2, 0), 1);
        // all three: the chosen first set
        assert_eq!((0..3).map(|f| primer_set(7, f)).collect::<Vec<_>>(), [0, 1, 2]);
        // none: set 0
        assert_eq!(primer_set(0, 1), 0);
        let levels = "0307".to_string() + &"0101".repeat(255);
        let w = serde_json::json!({"levels_default": levels});
        assert_eq!(primer_set_masks(&w, None).map(|m| (m[0], m[1])), Some((7, 1)));
        assert_eq!(primer_set_masks(&serde_json::json!({}), None), None);
    }

    #[test]
    fn part_wear_from_the_configuration() {
        use super::{part_wear, PartWear};
        // part 0: level 1 on ladder 0, whose entry 1 has an age range 100..200;
        // part 1: level 4, a preset whose age record gives the range 0.6..1.0
        // and whose channel A turns on from 0.5; part 2: no level
        let mut levels = "ffff".repeat(256);
        levels.replace_range(0..8, "01010407");
        let mut ladder = "ff".repeat(256);
        ladder.replace_range(0..4, "0000");
        let w = serde_json::json!({
            "levels_by_config": {"k1": levels.clone(), "k2": "0000".repeat(256)},
            "levels_default": levels,
            "ladder_by_look": [ladder],
            "ladders": [{"wearPresets": ["p0", "p1", "p1", "p1", "p4", "p4"]}],
            "presets": {"p1": {"ageWearHash": "r1"}, "p4": {"ageWearHash": "a4", "controlIntensityA": 255, "b18a6db5": 127.5, "1b6a3e09": 255, "rustStart": 51, "rustEnd": 102,
                "rustStartB": 12, "rustEndB": 44, "primerGradientStart": 19, "primerGradientEnd": 255,
                "primerIntensityB": 102, "4eb87881": 0, "26007b2d": 255, "primerStart": 10, "primerEnd": 127, "8ed71149": 51, "eff390bd": true,
                "951a7677": 51, "b58896f4": 153, "8482c039": 0, "4cacba9c": 255, "fcf8c89c": 200,
                "87df7517": 102, "7c999233": 51, "851c6b0b": 0, "ac8e3a46": 255,
                "f86ce666": 128, "eb7223d6": 25, "d54c040a": 76, "7f1fe0a2": "e1",
                "879774bd": "cr"}},
            "colour_ranges": {"cr": {"minGreen": 102, "maxGreen": 153, "fb2b7f54": 0, "c5801a83": 102, "7e3dffd5": 255, "c82d2173": 255}},
            "age_ranges": {"r1": {"minWear": 100, "maxWear": 200}},
            "ages": {"a4": {"4e71efc8": 0.6, "b5fcdec9": 1.0, "aa7d4fbd": 2.0, "minTiling": 6.0, "maxTiling": 7.0,
                "1da6a331": 0.0, "5b934fe3": 0.5, "redScratchStart": 0.0, "redScratchEnd": 1.0, "d1f62b9a": 1.0, "193c7eb1": 1.0,
                "91b26647": 1.0, "6d9d81b3": true, "a06d3fe0": true}},
            "edges": {"e1": {"edgeIntensity": 0.275, "edgeStart": 0.05, "edgeEnd": 0.8}},
        });
        let a = part_wear(&w, 0, Some("k1")).unwrap();
        assert_eq!((a[0].amount, a[1].amount, a[2]), (150, 204, PartWear::default()));
        // v0 0.8: activation (0.8 - 0.5) / 0.5 = 0.6, scale 0.8 x 1 x 0.6 = 0.48
        assert_eq!((a[1].scale, a[1].start[0], a[1].width[0]), ([122, 0, 0], 51, 51));
        // primer channel B: 0.8 x 0.4 x act 0.8 = 0.256 times the range's
        // middle scale 0.5 = 0.128, bias 0.2 x 0.256, gamma 1; window 10,
        // width 117; no first range: scale 1, bias 0, gamma 1
        assert_eq!((a[1].primer, a[1].primer_bias, a[1].primer_gamma[1]), ([0, 32, 0], [0, 13, 0], 255));
        assert_eq!((a[1].primer_start, a[1].primer_width, a[1].primer_cross, a[1].primer_on_wear_mask), (10, 117, 51, true));
        assert_eq!((a[1].bias, a[1].gamma), ([0; 3], [255; 3]));
        // the game's float widths (rva 0x173f430): 12..44 gives one less
        // than the bytes' difference, 19..255 the difference itself
        assert_eq!((a[1].start[1], a[1].width[1]), (12, 31));
        assert_eq!((a[1].primer_gradient_start, a[1].primer_gradient_width), (19, 236));
        // W2 channel R: activation 0.8, start 0.8 x 51, width 0.8 x (153 -
        // 51), truncated; channel G ends before it starts: its width wraps
        // round (0.8 x -51 = -40.8 -> -40 -> 216); B and D4 not activated
        assert_eq!((a[1].w2_start, a[1].w2_width, a[1].w2_div), ([40, 81, 0, 0], [81, 216, 0, 0], [200, 0, 0, 0]));
        // the 4th channel: scale 0.8 x 128, window 25 with width 51
        assert_eq!((a[1].d4_scale, a[1].d4_start, a[1].d4_width), (102, 25, 51));
        // the edge record: intensity 0.275, start and end x 255 truncated;
        // the clear-coat loss 25.5 x 2
        assert_eq!((a[1].edge_intensity, a[1].edge_start, a[1].edge_end, a[1].coat_loss), (70, 12, 204, 51));
        // the dust and scratch mask: tiling 6.5 x 5.1, the red scratch
        // curve 0.5 x v0 = 0.4 (x 255; signed with its multiplier 1:
        // 127.5 + 127.5 x 0.4), base gain 1, flag bits 18 and 26; curves
        // without fields are 0 (signed: 127)
        assert_eq!(a[1].dust, [33, 102, 0, 0, 0, 0, 0, 255, 0, 0, 0, 178, 0, 127, 1, 4]);
        assert_eq!(super::primer_surface(&serde_json::json!({"primerPalette": {"roughnessLow": 0.45, "roughnessHigh": 0.78, "metallicLow": 0.0, "metallicHigh": 0.0}})), [0, 0, 114, 198]);
        assert_eq!(super::primer_surface(&serde_json::json!({})), [0, 0, 153, 153]);
        assert_eq!(part_wear(&w, 0, None).unwrap()[0].amount, 150);
        // a configuration without levels: no wear; level 0 without a ladder: the level's value
        assert_eq!(part_wear(&w, 0, Some("nope")).unwrap()[0], PartWear::default());
        assert_eq!(part_wear(&w, 0, Some("k2")).unwrap()[5].amount, 15);
        // a model without wear levels
        assert_eq!(part_wear(&serde_json::json!({"ladders": []}), 0, None), None);
    }

    #[test]
    fn part_surfaces_in_the_ramp_image() {
        use super::{hex4, part_surface_row, surface_of_code, PART_RAMP_ROWS};
        assert_eq!(hex4("e6ff1a66"), Some([0xe6, 0xff, 0x1a, 0x66]));
        assert_eq!(hex4("e6ff1a6"), None);
        assert_eq!(hex4("e6ff1a6g"), None);
        // the four values in the alpha of columns 0 to 3, where the shader loads them
        let row = part_surface_row([1, 2, 3, 4]);
        assert_eq!(row.len(), 128 * 4);
        assert_eq!([row[3], row[7], row[11], row[15]], [1, 2, 3, 4]);
        assert_eq!(&row[..3], &[0, 0, 0]);
        // three blocks of 256 rows: the shader's row coordinates divide by this
        assert_eq!(PART_RAMP_ROWS, 1281);
        // an export without surfaces: the look's code, low = high
        assert_eq!(surface_of_code(165), [255, 255, 78, 78]);
        assert_eq!(surface_of_code(49), [0, 0, 255, 255]);
        assert_eq!(surface_of_code(254), [0, 0, 192, 192]);
    }

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

/// A mesh's vertices with its morph targets applied at the given weights
/// (by target name). Normals turn with the faces: each vertex's normal
/// gets the change of its face-averaged normal.
fn morph_vertices(me: &glb::Mesh, weights: &[(String, f32)]) -> Vec<glb::Vertex> {
    let mut v = me.verts.clone();
    let mut any = false;
    for (name, d) in &me.morphs {
        let Some(w) = weights.iter().find(|e| &e.0 == name).map(|e| e.1).filter(|w| *w != 0.0) else { continue };
        any = true;
        for (x, d) in v.iter_mut().zip(d) {
            for k in 0..3 {
                x.pos[k] += w * d[k];
            }
        }
    }
    if !any {
        return v;
    }
    let normals = |vs: &[glb::Vertex]| -> Vec<Vec3> {
        let mut acc = vec![Vec3::ZERO; vs.len()];
        for t in me.indices.chunks_exact(3) {
            let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
            if a.max(b).max(c) >= vs.len() {
                continue;
            }
            let (pa, pb, pc) = (Vec3::from(vs[a].pos), Vec3::from(vs[b].pos), Vec3::from(vs[c].pos));
            let n = (pb - pa).cross(pc - pa);
            for i in [a, b, c] {
                acc[i] += n;
            }
        }
        acc.into_iter().map(|n| n.normalize_or_zero()).collect()
    };
    let (n0, n1) = (normals(&me.verts), normals(&v));
    for ((x, a), b) in v.iter_mut().zip(n0).zip(n1) {
        let n = (Vec3::from(x.nrm) + b - a).normalize_or_zero();
        if n != Vec3::ZERO {
            x.nrm = n.into();
        }
    }
    v
}
