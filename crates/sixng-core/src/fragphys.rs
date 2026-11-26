//! Fragment physics (GTA V `fragType` / `fragPhysicsLOD` equivalent).
//!
//! The fragment root (system page offset 0) holds the physics LOD inline:
//!
//! ```text
//! root
//!   +0x10 ptr  archetype (phArchetypeDamp-like, below)
//!   +0x18 ptr  main drawable; its +0x90 points at the joint limits:
//!              +0x28 u16 count, +0x2C u16 count again, +0x30 ptr rotation
//!              limits; +0x38 / +0x3C / +0x40 the same for translation
//!              limits. Each limit is 0x30 bytes: vec4 min, vec4 max,
//!              u32 joaat of the bone name, padding (`JointLimit`).
//!   +0x20 ptr  groups: count x 0x70-byte records (u64 count at ptr - 8)
//!   +0x28 ptr  children: count x 0x50-byte records (u64 count at ptr - 0x10)
//!   +0x30 u16  ?, u16 ?, u16 group count, u16 child count
//!   +0x60 vec3 centre of gravity (the archetype bound's, model space)
//!   +0x80 vec3 bounding-sphere centre, +0x8C f32 radius
//!   +0xA8 ptr  name
//!   +0xB8 u32  flags (`ROOT_FLAGS`), +0xBC f32 minimum move force? (-1 = fixed until
//!              broken), +0xC0..+0xD4 f32 not named (-1 -1 -1 1 0.7 mostly)
//! archetype
//!   +0x10 u32  kind (2)
//!   +0x18 ptr  name, +0x20 ptr bound (composite, type 6)
//!   +0x28 u32  type flags, +0x30 u32 include flags
//!   +0x40 vec3 angular inertia, +0x4C f32 mass (sum of the children)
//!   +0x50 vec3 inverse inertia, +0x5C f32 inverse mass
//!   +0x60 f32  gravity factor, max speed, max angular speed, buoyancy factor
//!   +0x70 6 x vec4 damping: linear C, V, V2, angular C, V, V2
//! group (0x70)
//!   +0x00 u32  flags, +0x04 u32 more flags (both in `GROUP_FLAGS`)
//!   +0x08 u32  joaat of the group's bone name (= group name)
//!   +0x10 u16  bone index, ?, first child group, parent group (ffff = none),
//!              child group count, first child, child end, child end, child end
//!   +0x28 36 x f16 tuning, named in `TUNING_NAMES` (strength, force
//!              transmission, damage, joint motor, per-source scales)
//! child (0x50)
//!   +0x00 quat orientation of the child's bound in its bone frame
//!   +0x10 vec3 offset from the bone pivot (bone frame), u32 1
//!   +0x20 vec3 angular inertia, +0x2C f32 mass
//!   +0x30 ptr  bound (one child of the archetype's composite)
//!   +0x38 u32  type flags, +0x40 u32 include flags
//!   +0x48 u16  group index, u16 child number (ffff = spare record, not
//!              in use; the groups' child ranges count record positions)
//! ```
//!
//! Inertia = mass x the bound's volume distribution (bound +0x60), and the
//! archetype's mass is the sum of the child masses: both hold on every
//! fragment checked. The child offset plus its bone's pivot is the
//! composite's child transform.
//!
//! Drawables (`dr`) have no archetype; their collision is a composite
//! bound at root+0xC8 (`drawable_bound`). Per-model overrides (mass, ...)
//! live in the tunable-object table of data (`tunable_objects`), keyed by
//! a 64-bit key of the map archetype definition.
use crate::resource::Resource;
use serde::Serialize;

pub const GROUP_SIZE: usize = 0x70;
pub const CHILD_SIZE: usize = 0x50;
/// Version of the `physics` block in model.json.
pub const EXPORT_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Archetype {
    pub kind: u32,
    pub name: Option<String>,
    pub mass: f32,
    /// Angular inertia (kg m^2) about the principal axes.
    pub inertia: [f32; 3],
    pub gravity_factor: f32,
    pub max_speed: f32,
    pub max_ang_speed: f32,
    pub buoyancy_factor: f32,
    /// Damping constants: linear C, V, V2, angular C, V, V2 (x, y, z).
    pub damping: [[f32; 3]; 6],
    /// Collision layers (`LAYERS`) the object is in / collides with.
    pub type_flags: u64,
    pub include_flags: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoundInfo {
    /// Bound type byte (+0x10): 1 capsule, 3 box, 4 geometry, 5 BVH,
    /// 6 composite, 8 disc; 2, 9 and 13 are primitives whose names are
    /// not known (13 holds the long round pieces: poles, pipes).
    pub kind: u8,
    pub radius: f32,
    pub margin: f32,
    /// Local [min, max].
    pub bbox: [[f32; 3]; 2],
    pub centroid: [f32; 3],
    pub cg: [f32; 3],
    /// Volume (bound +0x6C).
    pub volume: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Group {
    pub index: usize,
    pub name_hash: String,
    pub bone: u16,
    pub parent: Option<u16>,
    pub first_child_group: Option<u16>,
    pub child_group_count: u16,
    /// Children of this group: `first_child .. child_end`.
    pub first_child: u16,
    pub child_end: u16,
    pub strength: f32,
    pub force_transmission_up: f32,
    pub force_transmission_down: f32,
    pub joint_stiffness: f32,
    pub min_damage_force: f32,
    pub damage_health: f32,
    /// Sum of the group's child masses.
    pub mass: f32,
    /// The 36 half floats at +0x28, as read ([0]..[5] are the named ones above).
    pub tuning_raw: Vec<f32>,
    /// u32 at +0x00 and +0x04 (meaning unknown).
    pub flags: [u32; 2],
    /// u16 at +0x12 (always 0 so far).
    pub unknown_0a: u16,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Child {
    /// Record position in the array (= the composite bound's child slot).
    pub index: usize,
    /// The record's own number (+0x4A). Usually the position; spare
    /// records (not in use) have ffff and push the numbers of later
    /// records down. Group child ranges count positions, not numbers.
    pub number: Option<u16>,
    pub in_use: bool,
    pub group: u16,
    pub mass: f32,
    pub inertia: [f32; 3],
    pub rotation: [f32; 4],
    pub offset: [f32; 3],
    /// Collision layers (`LAYERS`) the child's bound is in / collides with.
    pub type_flags: u64,
    pub include_flags: u64,
    pub bound: Option<BoundInfo>,
    /// Translation of the composite's transform for this child (model space).
    pub position: Option<[f32; 3]>,
}

/// One bone's joint limit (drawable +0x90), per axis of the bone's own
/// frame. Rotation limits are radians: 99.2% of the level values are whole
/// degrees, and a door's hinge bone has z -pi/2..pi/2. Translation limits
/// are metres: a 2.09 m sliding door slides 2.09 m, and a 3 m garage door
/// lifts 3.24 m. The frame is the bone's: the left panels of two-panel
/// elevator doors have bones turned 180 degrees about z and the same +x
/// limit as the right panels, so both open outwards (in model axes the
/// left panel would cross the middle). Rotation w is always -pi / pi and
/// translation w always 0; neither is exported.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct JointLimit {
    /// joaat of the bone name.
    pub bone_hash: u32,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

pub const JOINT_LIMIT_SIZE: usize = 0x30;

/// Rotation and translation limits behind a fragment drawable at `d`.
/// Empty when the drawable has no joint data or it does not check out.
pub fn joint_limits(r: &Resource, d: usize) -> (Vec<JointLimit>, Vec<JointLimit>) {
    let n = r.sys.len();
    let Some(j) = r.sys_off(r.u64(d + 0x90)).filter(|&j| j + 0x48 <= n) else { return Default::default() };
    let read = |co: usize, po: usize| -> Vec<JointLimit> {
        let count = r.u16(j + co) as usize;
        let Some(t) = r.sys_off(r.u64(j + po)) else { return Vec::new() };
        if count > 1024 || t + JOINT_LIMIT_SIZE * count > n {
            return Vec::new();
        }
        let v: Vec<JointLimit> = (0..count)
            .map(|i| {
                let o = t + JOINT_LIMIT_SIZE * i;
                JointLimit { bone_hash: r.u32(o + 0x20), min: v3(r, o), max: v3(r, o + 0x10) }
            })
            .collect();
        if v.iter().all(|l| finite(&l.min) && finite(&l.max)) { v } else { Vec::new() }
    };
    (read(0x28, 0x30), read(0x38, 0x40))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FragPhysics {
    pub archetype: Archetype,
    pub centre_of_gravity: [f32; 3],
    pub bounding_sphere: [f32; 4],
    pub groups: Vec<Group>,
    pub children: Vec<Child>,
    /// Root floats +0xB8..+0xD8 (meaning unknown; see notes).
    pub root_tail: Vec<f32>,
    pub rotation_limits: Vec<JointLimit>,
    pub translation_limits: Vec<JointLimit>,
}

fn v3(r: &Resource, o: usize) -> [f32; 3] {
    [r.f32(o), r.f32(o + 4), r.f32(o + 8)]
}

fn finite(v: &[f32]) -> bool {
    v.iter().all(|x| x.is_finite())
}

fn opt(v: u16) -> Option<u16> {
    (v != 0xFFFF).then_some(v)
}

/// Read a bound header (any type).
pub fn bound_info(r: &Resource, o: usize) -> Option<BoundInfo> {
    if o + 0x70 > r.sys.len() {
        return None;
    }
    let mx = v3(r, o + 0x20);
    let mn = v3(r, o + 0x30);
    let b = BoundInfo { kind: r.sys[o + 0x10], radius: r.f32(o + 0x14), margin: r.f32(o + 0x2C), bbox: [mn, mx], centroid: v3(r, o + 0x40), cg: v3(r, o + 0x50), volume: r.f32(o + 0x6C) };
    (finite(&mx) && finite(&mn) && (0..3).all(|k| mn[k] <= mx[k] + 1e-4)).then_some(b)
}

/// Read a phArchetype at `o` (fragment root+0x10, or a drawable's
/// embedded physics).
pub fn archetype(r: &Resource, o: usize) -> Option<Archetype> {
    if o + 0xD0 > r.sys.len() {
        return None;
    }
    let mass = r.f32(o + 0x4C);
    let inv = r.f32(o + 0x5C);
    // mass and its inverse are stored together
    if !(mass > 0.0 && mass.is_finite() && (mass * inv - 1.0).abs() < 1e-3) {
        return None;
    }
    let mut damping = [[0.0; 3]; 6];
    for (k, d) in damping.iter_mut().enumerate() {
        *d = v3(r, o + 0x70 + 16 * k);
    }
    Some(Archetype {
        kind: r.u32(o + 0x10),
        name: r.sys_off(r.u64(o + 0x18)).and_then(|p| r.cstr(p)),
        mass,
        inertia: v3(r, o + 0x40),
        gravity_factor: r.f32(o + 0x60),
        max_speed: r.f32(o + 0x64),
        max_ang_speed: r.f32(o + 0x68),
        buoyancy_factor: r.f32(o + 0x6C),
        damping,
        type_flags: r.u64(o + 0x28),
        include_flags: r.u64(o + 0x30),
    })
}

/// Decode the physics of a fragment (root at system offset 0). None when
/// the layout does not check out.
pub fn decode(r: &Resource) -> Option<FragPhysics> {
    try_decode(r).ok()
}

/// `decode`, saying which check failed.
pub fn try_decode(r: &Resource) -> Result<FragPhysics, &'static str> {
    let n = r.sys.len();
    if n < 0x100 {
        return Err("short");
    }
    let ngroups = r.u16(0x34) as usize;
    let nchildren = r.u16(0x36) as usize;
    if !(1..=255).contains(&ngroups) || !(1..=1024).contains(&nchildren) {
        return Err("counts");
    }
    let arch_o = r.sys_off(r.u64(0x10)).ok_or("archetype ptr")?;
    let archetype = archetype(r, arch_o).ok_or("archetype")?;
    let gp = r.sys_off(r.u64(0x20)).ok_or("groups ptr")?;
    let cp = r.sys_off(r.u64(0x28)).ok_or("children ptr")?;
    if gp < 8 || r.u64(gp - 8) as usize != ngroups || gp + GROUP_SIZE * ngroups > n || cp + CHILD_SIZE * nchildren > n {
        return Err("arrays");
    }
    // composite bound of the archetype: children and their transforms
    let comp = r.sys_off(r.u64(arch_o + 0x20)).filter(|&b| b + 0xA0 <= n && r.sys[b + 0x10] == 6);
    let xforms = comp.and_then(|b| {
        let count = r.u16(b + 0x98) as usize;
        let t = r.sys_off(r.u64(b + 0x78))?;
        (count == nchildren && t + 64 * count <= n).then_some(t)
    });
    let mut children = Vec::with_capacity(nchildren);
    for i in 0..nchildren {
        let o = cp + CHILD_SIZE * i;
        let mass = r.f32(o + 0x2C);
        let rotation = [r.f32(o), r.f32(o + 4), r.f32(o + 8), r.f32(o + 12)];
        let number = opt(r.u16(o + 0x4A));
        if !mass.is_finite() || mass < 0.0 || !finite(&rotation) || number.is_some_and(|k| k as usize > i) {
            return Err("child");
        }
        children.push(Child {
            index: i,
            number,
            in_use: number.is_some(),
            group: r.u16(o + 0x48),
            mass,
            inertia: v3(r, o + 0x20),
            rotation,
            offset: v3(r, o + 0x10),
            type_flags: r.u64(o + 0x38),
            include_flags: r.u64(o + 0x40),
            bound: r.sys_off(r.u64(o + 0x30)).and_then(|b| bound_info(r, b)),
            position: xforms.map(|t| v3(r, t + 64 * i + 48)),
        });
    }
    let mut groups = Vec::with_capacity(ngroups);
    for g in 0..ngroups {
        let o = gp + GROUP_SIZE * g;
        let u = |k: usize| r.u16(o + 0x10 + 2 * k);
        let t: Vec<f32> = (0..36).map(|k| crate::texture::f16(r.u16(o + 0x28 + 2 * k))).collect();
        let (first_child, child_end) = (u(5), u(6));
        if first_child > child_end || child_end as usize > nchildren {
            return Err("group range");
        }
        let mass = children[first_child as usize..child_end as usize].iter().filter(|c| c.in_use).map(|c| c.mass).sum();
        groups.push(Group {
            index: g,
            name_hash: format!("{:08x}", r.u32(o + 8)),
            flags: [r.u32(o), r.u32(o + 4)],
            bone: u(0),
            unknown_0a: u(1),
            first_child_group: opt(u(2)),
            parent: opt(u(3)),
            child_group_count: u(4),
            first_child,
            child_end,
            strength: t[0],
            force_transmission_up: t[1],
            force_transmission_down: t[2],
            joint_stiffness: t[7],
            min_damage_force: t[4],
            damage_health: t[5],
            mass,
            tuning_raw: t,
        });
    }
    if groups.iter().any(|g| g.parent.is_some_and(|p| p as usize >= ngroups)) || children.iter().any(|c| c.group as usize >= ngroups) {
        return Err("links");
    }
    let (rotation_limits, translation_limits) = r.sys_off(r.u64(0x18)).filter(|&d| d + 0x98 <= n).map(|d| joint_limits(r, d)).unwrap_or_default();
    Ok(FragPhysics {
        archetype,
        centre_of_gravity: v3(r, 0x60),
        bounding_sphere: [r.f32(0x80), r.f32(0x84), r.f32(0x88), r.f32(0x8C)],
        groups,
        children,
        root_tail: (0..9).map(|k| r.f32(0xB8 + 4 * k)).collect(),
        rotation_limits,
        translation_limits,
    })
}

/// Names of the group tuning half floats (`Group::tuning_raw`), from the
/// fragment tuning presets in common (`<Item><Name>...`): a preset's values
/// land at these indices in the groups that use it. The flag says whether a
/// preset with distinct values pinned the index; the others are the only
/// index left for their name. Indices not listed are not named.
pub const TUNING_NAMES: &[(usize, &str, bool)] = &[
    (0, "strength", true),
    (1, "force_transmission_up", true),
    (2, "force_transmission_down", true),
    (3, "disappears_when_dead_chance", true),
    (4, "min_damage_force", true),
    (5, "damage_health", true),
    (6, "weapon_health", true),
    (7, "joint_stiffness", true),
    (8, "motor_frequency", true),
    (9, "motor_damping", true),
    (11, "motor_max_torque", true),
    (12, "joint_latch_strength", false),
    (13, "joint_limit_strength", true),
    (14, "joint_limit_elasticity", false),
    (15, "joint_plastic_deflection_speed", false),
    (16, "joint_angle_plastic_deflection", true),
    (17, "melee_scale", true),
    (18, "weapon_scale", true),
    (19, "vehicle_scale", true),
    (20, "ped_scale", true),
    (21, "horse_scale", true),
    (22, "ragdoll_scale", true),
    (23, "explosion_scale", true),
    (24, "object_scale", true),
    (25, "ped_inv_mass_scale", true),
    (26, "horse_inv_mass_scale", false),
    (27, "vehicle_inv_mass_scale", false),
    (33, "weapon_inv_mass_scale", true),
    (34, "ped_melee_inv_mass_scale", true),
    (35, "bike_scale", true),
];

/// Group flag bits, as (word, bit, preset name, confirmed). Word 0 is the
/// u32 at group +0x00, word 1 the u32 at +0x04. Matched against the
/// booleans of the fragment tuning presets in common (the full table, 157
/// presets, each listing every boolean): across the 11,865 level groups
/// whose tuning values match exactly one preset, a confirmed bit is set on
/// at least 95% of the groups whose preset says true (at least 100 of them)
/// and on at most 0.2% of at least 100 whose preset says false. The others
/// have small samples or no preset that says false (see each line).
///
/// Never true in any preset, so not placed: DisableMoverUntilBreak,
/// BreakUsingTwistLimits, DoesntPushVehiclesDown, DisconnectFromParent,
/// UseConstraintForJoint, NoConnections, ConnectivityRoot,
/// LatchAfterAnimation, IsPortal, ScaleStrengthByMass (and MadeOfGlass,
/// placed from group names instead). InitialArticulationDisabled and
/// UseSoftConstraintForMuscle have no bit of their own. Unnamed bits with
/// a clear pattern in the group names: word 0 bit 13 (litter, 64% of its
/// groups are rub_litter roots), 17 (reeds, cattails, corn stalks), 21
/// (safe keypads only), 25 (bending poles and leaves); word 1 bits 1, 2
/// and 5 (wooden fences, pickets, crates, cardboard).
///
/// Checked against the joint data, bone flags and all 36 tuning values
/// too, with no exact match for any unnamed bit. Patterns:
/// - Passed down the group tree: word 0 bits 4, 6, 10, 13, 19, 27, 31 and
///   word 1 bits 2 and 5 are set on 44-95% of the groups whose parent has
///   them, and on at most 0.8% of the others.
/// - Subsets of joints: word 0 bit 21 is on 36 groups, all translation-
///   limited keypad buttons (1,172 other groups have translation limits);
///   bit 17 is on 86 groups, 84 with motor_max_torque > 0; bit 25 is on 242
///   groups, 223 rotation-limited.
pub const GROUP_FLAGS: &[(usize, u32, &str, bool)] = &[
    (0, 0, "break_when_damaged", true),
    // no preset says true; 91% of the 1,186 groups with it are named
    // Glass_* or *pane*, nearly all box bounds
    (0, 1, "made_of_glass", false),
    (0, 2, "damage_when_broken", true),
    (0, 3, "doesnt_affect_vehicles", true),
    // 81%: one road-sign preset's 236 groups lack it; none of 767 false
    (0, 8, "break_when_parent_damaged", true),
    (0, 11, "enable_motor", true),
    (0, 12, "damage_around_break", true),
    (0, 14, "disappear_when_broken", true),
    (0, 16, "disable_map_collision_until_break", true),
    // 326 of 327 groups, none of 11 false; set on aircraft and other
    // vehicle groups
    (0, 18, "override_vehicle_defaults", false),
    // all 7 groups that say true, none of 336 false
    (0, 20, "break_when_walked_over", false),
    // set together with word 1 bit 3, as the presets set the two collide
    // booleans together: one of the pair each, order unknown
    (0, 23, "collide_convex_against_ground_or_larger_bounds", false),
    // 71 of 76 groups, 19 of 1,925 false
    (0, 28, "obliterate_at_high_speed", false),
    // 92% of 2,001 groups, none of 4 false
    (0, 29, "obliterate_at_very_high_speed", false),
    // all 315 groups, none says false; set on none of the other matched groups
    (0, 30, "break_when_tilted", false),
    // 84% of 607 groups, none says false; set on none of the other matched groups
    (1, 0, "fade_with_time", false),
    (1, 3, "collide_convex_against_ground_or_larger_bounds", false),
];

/// Fragment root flag bits (u32 at root +0xB8), as (bit, name, confirmed).
/// The `any_group_*` bits equal "some group has that bit" on all 40,705
/// level fragments. Bits 0, 5 and 30 are always set; 1, 4, 7, 9 and 31
/// never.
pub const ROOT_FLAGS: &[(u32, &str, bool)] = &[
    (2, "any_group_flag_13", true),
    (3, "any_group_disable_map_collision_until_break", true),
    // set on every veg_ fragment; 99% of the fragments with it are veg_
    (8, "vegetation", false),
    // only on vehicle fragments
    (10, "vehicle", false),
    // set on all 11,950 fragments with more than one group and a group that
    // can break (strength >= 0), and on 80 of the 1,921 multi-group ones
    // with none; never on single-group fragments. Bits 16 and 24 are set
    // together on all but one fragment and share the name.
    (16, "has_breakable_groups", false),
    // = some group has joint_angle_plastic_deflection (tuning 16) > 0, on
    // all 40,705 level fragments (1,066 set)
    (20, "any_group_joint_angle_plastic_deflection", true),
    (21, "any_group_obliterate_at_high_speed", true),
    (24, "has_breakable_groups", false),
    (22, "any_group_obliterate_at_very_high_speed", true),
    (23, "any_group_break_when_tilted", true),
    (28, "any_group_flag2_1", true),
    (29, "any_group_flag2_2", true),
];

/// Names of the set bits of group flag words `w`, and the bits left
/// unnamed (as [word, bit]).
/// One round piece of a character's body bound: the fragments
/// `defaultfemale_fr1_000_bounds` and `defaultmale_mr1_000_bounds` (and the
/// per-garment `char_bipedbounds_*`) hold a composite whose children sit
/// on the ragdoll bones, one group per bone (group name = joaat of the bone
/// name). Kind 2 is a tapered capsule along its local y: +0x70 f32 radius
/// at the +y end, +0x74 f32 radius at the -y end (= the margin), +0x78 f32
/// length of the segment between the two centres; its bbox y half extent
/// is length / 2 + the larger radius on every piece checked. Kind 0 is a
/// sphere of the margin's radius. The larger radius is at the hip end of
/// the thigh pieces, which is +y. Boxes (hands) and kind 13 (feet) are
/// left out.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyPiece {
    /// joaat of the bone name (`SKEL_Head`).
    pub bone_hash: u32,
    /// The piece's frame in its bone's frame (column-major).
    pub local: [f32; 16],
    pub kind: u8,
    /// Radius at local +y (at length / 2) and at -y.
    pub r_pos: f32,
    pub r_neg: f32,
    pub length: f32,
}

/// The round pieces of a body bound fragment (`BodyPiece`).
pub fn body_pieces(r: &Resource) -> Vec<BodyPiece> {
    let Ok(f) = try_decode(r) else { return Vec::new() };
    let n = r.sys.len();
    let Some(arch) = r.sys_off(r.u64(0x10)) else { return Vec::new() };
    let Some(comp) = r.sys_off(r.u64(arch + 0x20)).filter(|&b| b + 0xA0 <= n && r.sys[b + 0x10] == 6) else { return Vec::new() };
    let Some(t) = r.sys_off(r.u64(comp + 0x78)).filter(|&t| t + 64 * f.children.len() <= n) else { return Vec::new() };
    let Some(cp) = r.sys_off(r.u64(0x28)) else { return Vec::new() };
    let mut out = Vec::new();
    for c in f.children.iter().filter(|c| c.in_use) {
        let Some(g) = f.groups.get(c.group as usize) else { continue };
        let Ok(bone_hash) = u32::from_str_radix(&g.name_hash, 16) else { continue };
        let Some(b) = r.sys_off(r.u64(cp + CHILD_SIZE * c.index + 0x30)).filter(|&b| b + 0x80 <= n) else { continue };
        let kind = r.sys[b + 0x10];
        let margin = r.f32(b + 0x2C);
        let (r_pos, r_neg, length) = match kind {
            0 => (margin, margin, 0.0),
            2 => (r.f32(b + 0x70), r.f32(b + 0x74), r.f32(b + 0x78)),
            _ => continue,
        };
        if ![r_pos, r_neg, length].iter().all(|v| v.is_finite() && *v >= 0.0 && *v < 2.0) {
            continue;
        }
        let mut local = [0f32; 16];
        for (j, v) in local.iter_mut().enumerate() {
            *v = r.f32(t + 64 * c.index + 4 * j);
        }
        // stored with w = 0 in the last row
        local[3] = 0.0;
        local[7] = 0.0;
        local[11] = 0.0;
        local[15] = 1.0;
        if !finite(&local) {
            continue;
        }
        out.push(BodyPiece { bone_hash, local, kind, r_pos, r_neg, length });
    }
    out
}

pub fn group_flag_names(w: [u32; 2]) -> (Vec<&'static str>, Vec<[u32; 2]>) {
    let mut names = Vec::new();
    let mut rest = Vec::new();
    for (word, v) in w.iter().enumerate() {
        for bit in (0..32).filter(|b| v >> b & 1 == 1) {
            match GROUP_FLAGS.iter().find(|f| f.0 == word && f.1 == bit) {
                Some(f) if !names.contains(&f.2) => names.push(f.2),
                Some(_) => {}
                None => rest.push([word as u32, bit]),
            }
        }
    }
    (names, rest)
}

/// Names of the set bits of the root flags, and the bits left unnamed.
pub fn root_flag_names(v: u32) -> (Vec<&'static str>, Vec<u32>) {
    let bits = (0..32).filter(|b| v >> b & 1 == 1);
    let (named, rest): (Vec<u32>, Vec<u32>) = bits.partition(|b| ROOT_FLAGS.iter().any(|f| f.0 == *b));
    let mut names = Vec::new();
    for n in named.iter().filter_map(|b| ROOT_FLAGS.iter().find(|f| f.0 == *b).map(|f| f.1)) {
        if !names.contains(&n) {
            names.push(n);
        }
    }
    (names, rest)
}

/// Collision layer bits of the type / include masks (u64: the object is in
/// the `type` layers and collides with the `include` layers), as (bit,
/// name, checked). Bits 0..32 follow GTA V's layer order. Checked against
/// the map collision of levels (composite child masks at +0x88, 16 bytes
/// each) and the fragments, where the masks fit the names: the plain map
/// mesh is in layers 1-5 (0x3e) and collides with peds, ragdolls,
/// animals, objects, projectiles and the test layers; the weapon-only
/// mesh (0x2) collides only with projectile, test_weapon and test_camera;
/// map_cover (0x10 alone) only with test_ai; map_vehicle (0x20 alone,
/// road reflectors and submerged rock) only with vehicle_not_bvh and
/// test_vehicle_wheel; map_stairs (bit 30) only with peds, animals and
/// the test layers; map_river (bit 27) is all LIQ_WATER; map_deep_surface
/// (bit 31) is the DEFORM_* sand, mud and gravel; foliage (bit 19) is the
/// VEG_* leaves and twigs, and only on vegetation fragments. The bits not
/// checked are placed by the same order only. Bit 0 is set on every
/// fragment archetype (the OR of its children's masks plus bit 0, on all
/// 40,705 level fragments). Bits 32 and up are layers the older engine did
/// not have and are not named (`layer_<bit>`): most include masks have 37;
/// fragment children use 35 (bushes, colliding with test_camera only), 41
/// (trees), 42 (light bulbs, colliding with explosion and test_weapon), 46
/// (shop shelf rows), 36 and 40 (colliding with the test layers only), 34
/// and 43; map collision uses 34, 35, 41, 43 and 47.
pub const LAYERS: &[(u32, &str, bool)] = &[
    (1, "map_weapon", true),
    (2, "map_dynamic", false),
    (3, "map_animal", false),
    (4, "map_cover", true),
    (5, "map_vehicle", true),
    (6, "vehicle_not_bvh", true),
    (7, "vehicle_bvh", false),
    (8, "vehicle_box", false),
    (9, "ped", true),
    (10, "ragdoll", false),
    (11, "animal", true),
    (12, "animal_ragdoll", false),
    (13, "object", false),
    (14, "object_env_cloth", false),
    (15, "plant", false),
    (16, "projectile", true),
    (17, "explosion", false),
    (18, "pickup", false),
    (19, "foliage", true),
    (20, "forklift_forks", false),
    (21, "test_weapon", true),
    (22, "test_camera", true),
    (23, "test_ai", true),
    (24, "test_script", false),
    (25, "test_vehicle_wheel", true),
    (26, "glass", false),
    (27, "map_river", true),
    (28, "smoke", false),
    (29, "unsmashed", false),
    (30, "map_stairs", true),
    (31, "map_deep_surface", true),
];

/// Layer names of a type / include mask: "all" for an all-ones mask, else
/// each set bit's name (`layer_<bit>` where the bit has none).
pub fn layer_names(mask: u64) -> serde_json::Value {
    if mask == u64::MAX {
        return serde_json::json!(["all"]);
    }
    let names: Vec<String> = (0..64)
        .filter(|b| mask >> b & 1 == 1)
        .map(|b| LAYERS.iter().find(|l| l.0 == b).map(|l| l.1.to_string()).unwrap_or_else(|| format!("layer_{b}")))
        .collect();
    serde_json::json!(names)
}

fn bound_kind_name(k: u8) -> Option<&'static str> {
    Some(match k {
        1 => "capsule",
        3 => "box",
        4 => "geometry",
        5 => "bvh",
        6 => "composite",
        8 => "disc",
        _ => return None,
    })
}

fn bound_json(b: &BoundInfo) -> serde_json::Value {
    serde_json::json!({"type": b.kind, "type_name": bound_kind_name(b.kind), "bbox": b.bbox, "centroid": b.centroid, "cg": b.cg, "radius": b.radius, "margin": b.margin, "volume": b.volume})
}

/// Summary of a drawable's embedded collision (composite at root+0xC8):
/// its children's types, local boxes, volumes and placements. Drawables
/// carry no physics archetype, so no mass.
pub fn drawable_bound(r: &Resource) -> Option<serde_json::Value> {
    let n = r.sys.len();
    let b = r.sys_off(r.u64(0xC8)).filter(|&b| b + 0xA0 <= n && r.sys[b + 0x10] == 6)?;
    let root = bound_info(r, b)?;
    let count = r.u16(b + 0x98) as usize;
    let kids = r.sys_off(r.u64(b + 0x70))?;
    let xf = r.sys_off(r.u64(b + 0x78));
    let fl = r.sys_off(r.u64(b + 0x88)).filter(|f| f + 16 * count <= n);
    if count == 0 || count > 4096 || kids + 8 * count > n {
        return None;
    }
    let children: Vec<serde_json::Value> = (0..count)
        .map(|i| {
            let c = r.sys_off(r.u64(kids + 8 * i)).and_then(|o| bound_info(r, o));
            let pos = xf.filter(|t| t + 64 * (i + 1) <= n).map(|t| v3(r, t + 64 * i + 48));
            let mut j = serde_json::json!({"bound": c.as_ref().map(bound_json), "position": pos});
            if let Some(f) = fl {
                let (t, inc) = (r.u64(f + 16 * i), r.u64(f + 16 * i + 8));
                j["type_flags"] = format!("{t:016x}").into();
                j["include_flags"] = format!("{inc:016x}").into();
                j["type_layers"] = layer_names(t);
                j["include_layers"] = layer_names(inc);
            }
            j
        })
        .collect();
    Some(serde_json::json!({"composite": bound_json(&root), "children": children}))
}

/// The `physics` block of a fragment's model.json. `bones` are the
/// skeleton's bone names by index.
pub fn fragment_json(p: &FragPhysics, bones: &[String]) -> serde_json::Value {
    let a = &p.archetype;
    let limits = |v: &[JointLimit]| limits_json(p, bones, v);
    let groups: Vec<serde_json::Value> = p
        .groups
        .iter()
        .map(|g| {
            let name = bones.get(g.bone as usize).filter(|n| format!("{:08x}", crate::meta7a::joaat(n)) == g.name_hash);
            let mut tuning = serde_json::Map::new();
            for &(k, n, _) in TUNING_NAMES {
                if let Some(v) = g.tuning_raw.get(k) {
                    tuning.insert(n.into(), serde_json::json!(v));
                }
            }
            let (flag_names, flag_bits_unnamed) = group_flag_names(g.flags);
            serde_json::json!({
                "index": g.index, "name": name, "name_hash": g.name_hash, "bone": g.bone, "parent": g.parent,
                "child_groups": g.first_child_group.map(|f| (f..f + g.child_group_count).collect::<Vec<_>>()).unwrap_or_default(),
                "children": p.children[g.first_child as usize..g.child_end as usize].iter().filter(|c| c.in_use).map(|c| c.index).collect::<Vec<_>>(),
                "child_range": [g.first_child, g.child_end],
                "mass": g.mass, "tuning": tuning, "tuning_raw": g.tuning_raw,
                "flags": format!("{:08x}", g.flags[0]), "flags2": format!("{:08x}", g.flags[1]),
                "flag_names": flag_names, "flag_bits_unnamed": flag_bits_unnamed,
            })
        })
        .collect();
    let children: Vec<serde_json::Value> = p
        .children
        .iter()
        .map(|c| {
            let bone = p.groups.get(c.group as usize).map(|g| g.bone);
            serde_json::json!({
                "index": c.index, "number": c.number, "in_use": c.in_use, "group": c.group, "bone": bone, "bone_name": bone.and_then(|b| bones.get(b as usize)),
                "mass": c.mass, "inertia": c.inertia, "rotation": c.rotation, "offset": c.offset, "position": c.position,
                "bound": c.bound.as_ref().map(bound_json),
                "type_flags": format!("{:016x}", c.type_flags), "include_flags": format!("{:016x}", c.include_flags),
                "type_layers": layer_names(c.type_flags), "include_layers": layer_names(c.include_flags),
            })
        })
        .collect();
    serde_json::json!({
        "version": EXPORT_VERSION,
        "kind": "fragment",
        "mass": a.mass, "inertia": a.inertia, "centre_of_gravity": p.centre_of_gravity, "bounding_sphere": p.bounding_sphere,
        "gravity_factor": a.gravity_factor, "max_speed": a.max_speed, "max_ang_speed": a.max_ang_speed, "buoyancy_factor": a.buoyancy_factor,
        "damping": {"linear_c": a.damping[0], "linear_v": a.damping[1], "linear_v2": a.damping[2], "angular_c": a.damping[3], "angular_v": a.damping[4], "angular_v2": a.damping[5]},
        "min_move_force": p.root_tail[1],
        "root_flags": format!("{:08x}", p.root_tail[0].to_bits()),
        "root_flag_names": root_flag_names(p.root_tail[0].to_bits()).0,
        "root_flag_bits_unnamed": root_flag_names(p.root_tail[0].to_bits()).1,
        "root_tail": &p.root_tail[2..7],
        "archetype_type_flags": format!("{:016x}", a.type_flags), "archetype_include_flags": format!("{:016x}", a.include_flags),
        "archetype_type_layers": layer_names(a.type_flags), "archetype_include_layers": layer_names(a.include_flags),
        "groups": groups, "children": children,
        "rotation_limits": limits(&p.rotation_limits), "translation_limits": limits(&p.translation_limits),
        "note": NOTE,
    })
}

/// The joint limits of a fragment's bones.
fn limits_json(p: &FragPhysics, bones: &[String], v: &[JointLimit]) -> serde_json::Value {
    v.iter()
        .map(|l| {
            let name = bones.iter().find(|b| crate::meta7a::joaat(b) == l.bone_hash);
            let group = p.groups.iter().find(|g| g.name_hash == format!("{:08x}", l.bone_hash)).map(|g| g.index);
            serde_json::json!({"bone_name": name, "bone_hash": format!("{:08x}", l.bone_hash), "group": group, "min": l.min, "max": l.max})
        })
        .collect()
}

pub const NOTE: &str = "Units: kg, metres, kg m^2; model space is RAGE Z-up. Confirmed: mass (= sum of in-use child masses on every fragment), inertia (= child mass x bound volume distribution), centre_of_gravity, bounding_sphere, the group tree (parent, child_groups, children, bone; name = the bone whose joaat is name_hash), child mass, bound and position (= bone pivot + offset). tuning: names matched against the game's fragment tuning presets (see TUNING_NAMES in the source for which are pinned); tuning_raw keeps all 36 half floats. strength -1 = never breaks off its parent; on group 0 it plausibly frees the whole object from the world (guess). min_move_force (root +0xBC): -1 on trees, gates, signs and poles, small on loose props, 0 on vehicle parts (guess: force needed to start moving, -1 = fixed until broken). gravity_factor, max_speed, max_ang_speed, buoyancy_factor and damping are read in the GTA V archetype order (likely). Group flags (flags = word 0, flags2 = word 1) and root_flags are named in flag_names / root_flag_names from the presets' booleans (see GROUP_FLAGS and ROOT_FLAGS in the source for which are confirmed); bits not named are listed in flag_bits_unnamed. made_of_glass is set on the glass panes of windows and doors. Type/include flags are 64-bit collision layer masks (the layers an object is in / collides with), named in type_layers / include_layers after GTA V's layer order (see LAYERS in the source for which bits were checked against the map collision); the archetype's type mask is the OR of its children's plus bit 0. root_tail is raw. rotation_limits / translation_limits: per-bone joint limits from the main drawable (min / max per axis of the bone frame; rotation in radians, translation in metres; checked on doors, sliding doors and elevator doors); the bone is the group whose name_hash matches (on 98% of level limits), and a fragment has one rotation limit per bone flagged rotation-limited in the skeleton (96%). Damaged masses are not in the fragment as far as decoded.";

/// Facts about a model's map archetype definition, from
/// `<work>/index/archetype_lod.jsonl` (`sixng lod-links`), joined with the
/// tunable-object table of data (`tunable_objects.json`, built on first
/// use). None when the table has no flags for the model.
pub fn archetype_json(a: &crate::game::ArchiveInfo, name: &str) -> Option<serde_json::Value> {
    static ARCH: std::sync::OnceLock<std::collections::HashMap<u32, crate::lodlinks::ArchLod>> = std::sync::OnceLock::new();
    let work = crate::names::work_dir()?;
    let ws = crate::export::Workspace::new(crate::export::game_root(a), work);
    let arch = ARCH.get_or_init(|| crate::lodlinks::load_archetype_lod(&ws));
    let l = arch.get(&crate::meta7a::joaat(name))?;
    let flags = l.flags?;
    let bits: Vec<u32> = (0..32).filter(|b| flags >> b & 1 == 1).collect();
    let tun = tunable_objects(&ws);
    let mut keys: Vec<u64> = l.tuning_keys.iter().copied().filter(|&k| k != 0).collect();
    keys.dedup();
    let entry = keys.iter().find_map(|k| tun.get(k).map(|v| (k, v)));
    Some(serde_json::json!({
        "flags": format!("{flags:08x}"), "flag_bits": bits,
        "dynamic": flags >> 17 & 1 == 1,
        "flags_note": "CBaseArchetypeDef flags of the map archetype definition. dynamic = bit 17: set on every fragment's archetype and on 8% of drawables (the loose props), like GTA V's dynamic flag (likely). Bit 24 is set on every fragment too; bit 29 only on drawables (40%). Other bits not decoded.",
        "tunable_keys": keys.iter().map(|k| format!("{k:016x}")).collect::<Vec<_>>(),
        "tunable": entry.map(|(k, v)| { let mut v = v.clone(); v["key"] = serde_json::json!(format!("{k:016x}")); v }),
        "tunable_note": "the archetype's entry in the tunable-object table of data (CTunableObjectEntry), found by one of the archetype's 64-bit keys; -1 / -999 = no override. Named fields resolve by joaat; the others keep their field hashes.",
    }))
}

/// Struct hash of the tunable-object entries (`CTunableObjectEntry`).
pub const TUNABLE_ENTRY: u32 = 0x1c30_53ec;
/// Field hashes of `CTunableObjectEntry` whose names resolve (joaat).
const TUNABLE_FIELDS: &[(&str, &str)] = &[("b21037ab", "object_mass"), ("c008f481", "object_centre_of_mass_offset"), ("48ee2434", "buoyancy_factor"), ("4e073d98", "max_health"), ("a92e3683", "max_speed")];

/// The tunable-object table of data, cached in
/// `<work>/index/tunable_objects.json`. Empty when data is not indexed.
pub fn tunable_objects(ws: &crate::export::Workspace) -> &'static std::collections::HashMap<u64, serde_json::Value> {
    static T: std::sync::OnceLock<std::collections::HashMap<u64, serde_json::Value>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let path = ws.index_dir().join("tunable_objects.json");
        let parse = |v: &serde_json::Value| -> std::collections::HashMap<u64, serde_json::Value> {
            v["entries"].as_object().map(|o| o.iter().filter_map(|(k, x)| Some((u64::from_str_radix(k, 16).ok()?, x.clone()))).collect()).unwrap_or_default()
        };
        if let Some(v) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) {
            return parse(&v);
        }
        match build_tunable_objects(ws) {
            Ok(v) => {
                let _ = crate::atomic::write(&path, serde_json::to_vec(&v).unwrap_or_default());
                parse(&v)
            }
            Err(_) => Default::default(),
        }
    })
}

/// Find the PSO files of data that hold `CTunableObjectEntry` maps and
/// flatten them: key (hex) -> named fields plus the rest by hash.
pub fn build_tunable_objects(ws: &crate::export::Workspace) -> anyhow::Result<serde_json::Value> {
    use rayon::prelude::*;
    let a = ws.resolve("data")?;
    let ix = ws.load_index(&a)?;
    let needle = TUNABLE_ENTRY.to_le_bytes();
    let found: Vec<(String, serde_json::Value)> = ix
        .entries
        .par_iter()
        .filter(|e| e.ext == "pso")
        .filter_map(|e| {
            let d = crate::scan::read_entry(&a.path, e).ok()?;
            crate::resource::memfind(&d, &needle)?;
            let p = crate::pso::parse(&d)?;
            Some((e.id.clone(), p.to_json()))
        })
        .collect();
    let mut entries = serde_json::Map::new();
    let mut source = Vec::new();
    for (id, j) in &found {
        let before = entries.len();
        collect_tunables(j, &mut entries);
        if entries.len() > before {
            source.push(id.clone());
        }
    }
    Ok(serde_json::json!({"source": source, "entries": entries}))
}

fn collect_tunables(v: &serde_json::Value, out: &mut serde_json::Map<String, serde_json::Value>) {
    match v {
        serde_json::Value::Object(o) => {
            if let (Some(k), Some(item)) = (o.get("Key").and_then(|k| k.as_str()), o.get("Item")) {
                if item["_struct"].as_str() == Some(format!("{TUNABLE_ENTRY:08x}").as_str()) {
                    out.insert(k.to_string(), tunable_entry(item));
                    return;
                }
            }
            o.values().for_each(|x| collect_tunables(x, out));
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| collect_tunables(x, out)),
        _ => {}
    }
}

/// One `CTunableObjectEntry` as exported: the named fields, then the rest
/// under `fields` by hash.
pub fn tunable_entry(item: &serde_json::Value) -> serde_json::Value {
    let mut named = serde_json::Map::new();
    let mut raw = item.as_object().cloned().unwrap_or_default();
    raw.remove("_struct");
    for (h, nm) in TUNABLE_FIELDS {
        if let Some(x) = raw.remove(*h) {
            named.insert(nm.to_string(), x);
        }
    }
    named.insert("fields".into(), serde_json::Value::Object(raw));
    serde_json::Value::Object(named)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(r: &mut Resource, at: usize, b: &[u8]) {
        r.sys[at..at + b.len()].copy_from_slice(b);
    }
    fn p64(r: &mut Resource, at: usize, v: u64) {
        put(r, at, &v.to_le_bytes());
    }
    fn ptr(r: &mut Resource, at: usize, to: usize) {
        p64(r, at, 0x5000_0000 | to as u64);
    }
    fn f(r: &mut Resource, at: usize, v: &[f32]) {
        for (k, x) in v.iter().enumerate() {
            put(r, at + 4 * k, &x.to_le_bytes());
        }
    }
    fn h(r: &mut Resource, at: usize, v: &[u16]) {
        for (k, x) in v.iter().enumerate() {
            put(r, at + 2 * k, &x.to_le_bytes());
        }
    }
    fn bound(r: &mut Resource, at: usize, kind: u8, half: [f32; 3], volume: f32) {
        r.sys[at + 0x10] = kind;
        f(r, at + 0x14, &[half[0].max(half[1]).max(half[2])]);
        f(r, at + 0x20, &[half[0], half[1], half[2], 0.01]);
        f(r, at + 0x30, &[-half[0], -half[1], -half[2]]);
        put(r, at + 0x3C, &1u32.to_le_bytes());
        f(r, at + 0x6C, &[volume]);
    }

    /// A two-group, three-child fragment (one spare child record).
    fn fragment() -> Resource {
        let mut r = Resource { sys: vec![0; 0x1000], gfx: Vec::new(), gfx_base: 0 };
        ptr(&mut r, 0x10, 0x100);
        ptr(&mut r, 0x20, 0x308);
        p64(&mut r, 0x300, 2);
        ptr(&mut r, 0x28, 0x410);
        p64(&mut r, 0x400, 3);
        h(&mut r, 0x30, &[1, 1, 2, 3]);
        f(&mut r, 0x60, &[0.0, 0.1, 0.5]);
        f(&mut r, 0x80, &[0.0, 0.0, 0.5, 1.2]);
        f(&mut r, 0xBC, &[-1.0]);
        // archetype
        put(&mut r, 0x110, &2u32.to_le_bytes());
        ptr(&mut r, 0x118, 0x800);
        put(&mut r, 0x800, b"test_prop\0");
        ptr(&mut r, 0x120, 0x600);
        f(&mut r, 0x140, &[3.0, 3.0, 1.0, 12.0, 1.0 / 3.0, 1.0 / 3.0, 1.0, 1.0 / 12.0, 1.0, 150.0, 6.283, 1.0]);
        f(&mut r, 0x170, &[0.02, 0.02, 0.02]);
        // composite bound with three children and their transforms
        bound(&mut r, 0x600, 6, [0.5, 0.5, 1.0], 0.0);
        ptr(&mut r, 0x670, 0x700);
        ptr(&mut r, 0x678, 0xB00);
        h(&mut r, 0x698, &[3, 3]);
        for (i, at) in [0x900usize, 0x980, 0xA00].into_iter().enumerate() {
            bound(&mut r, at, 3, [0.25, 0.25, 0.5], 0.25);
            ptr(&mut r, 0x700 + 8 * i, at);
            f(&mut r, 0xB00 + 64 * i + 48, &[0.0, 0.0, 0.5 * i as f32]);
        }
        // groups: root (bone 0) -> arm (bone 1)
        let g0 = 0x308;
        put(&mut r, g0 + 8, &crate::meta7a::joaat("root_bone").to_le_bytes());
        h(&mut r, g0 + 0x10, &[0, 0, 1, 0xFFFF, 1, 0, 2, 2, 2]);
        h(&mut r, g0 + 0x28, &[0xBC00, 0x3400, 0x3400]); // -1, 0.25, 0.25
        let g1 = g0 + GROUP_SIZE;
        put(&mut r, g1 + 8, &crate::meta7a::joaat("arm").to_le_bytes());
        h(&mut r, g1 + 0x10, &[1, 0, 0xFFFF, 0, 0, 2, 3, 3, 3]);
        // glass, damage when broken, and an unnamed bit 5; word 1: fade with time
        put(&mut r, g1, &(0x2u32 | 0x4 | 0x20).to_le_bytes());
        put(&mut r, g1 + 4, &1u32.to_le_bytes());
        h(&mut r, g1 + 0x28, &[0x5DDC, 0x3400, 0x3400, 0, 0x5640, 0x63D0, 0, 0x3800]); // 375, .25, .25, 0, 100, 1000, 0, 0.5
        // children: 10 kg in the root, a spare 1 kg record, 2 kg on the arm
        // (numbered 1, as the spare takes no number; ranges count positions)
        for (i, (mass, group, index)) in [(10.0f32, 0u16, 0u16), (1.0, 0, 0xFFFF), (2.0, 1, 1)].into_iter().enumerate() {
            let c = 0x410 + CHILD_SIZE * i;
            f(&mut r, c, &[0.0, 0.0, 0.0, 1.0]);
            f(&mut r, c + 0x20, &[0.5 * mass, 0.5 * mass, 0.25 * mass, mass]);
            ptr(&mut r, c + 0x30, [0x900, 0x980, 0xA00][i]);
            h(&mut r, c + 0x48, &[group, index]);
        }
        // main drawable with joint data: the arm turns about z, slides along x
        ptr(&mut r, 0x18, 0xC00);
        ptr(&mut r, 0xC90, 0xD00);
        h(&mut r, 0xD28, &[1, 0, 1]);
        ptr(&mut r, 0xD30, 0xE00);
        h(&mut r, 0xD38, &[1, 0, 1]);
        ptr(&mut r, 0xD40, 0xE40);
        let pi = std::f32::consts::PI;
        f(&mut r, 0xE00, &[0.0, 0.0, -1.5708, -pi, 0.0, 0.0, 1.5708, pi]);
        put(&mut r, 0xE20, &crate::meta7a::joaat("arm").to_le_bytes());
        f(&mut r, 0xE40, &[0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0]);
        put(&mut r, 0xE60, &crate::meta7a::joaat("arm").to_le_bytes());
        r
    }

    #[test]
    fn decodes_fragment_physics() {
        let r = fragment();
        let p = try_decode(&r).expect("decodes");
        assert_eq!(p.archetype.name.as_deref(), Some("test_prop"));
        assert_eq!(p.archetype.mass, 12.0);
        assert_eq!(p.archetype.max_speed, 150.0);
        assert_eq!(p.archetype.damping[0], [0.02, 0.02, 0.02]);
        assert_eq!(p.bounding_sphere, [0.0, 0.0, 0.5, 1.2]);
        assert_eq!(p.groups.len(), 2);
        assert_eq!((p.groups[0].parent, p.groups[0].first_child_group, p.groups[0].strength), (None, Some(1), -1.0));
        let arm = &p.groups[1];
        assert_eq!((arm.parent, arm.bone, arm.first_child, arm.child_end), (Some(0), 1, 2, 3));
        assert_eq!((arm.strength, arm.min_damage_force, arm.damage_health, arm.joint_stiffness), (375.0, 100.0, 1000.0, 0.5));
        assert_eq!(arm.mass, 2.0);
        assert!(!p.children[1].in_use && p.children[2].in_use);
        assert_eq!(p.children[2].number, Some(1));
        assert_eq!(p.children[2].position, Some([0.0, 0.0, 1.0]));
        let b = p.children[0].bound.as_ref().unwrap();
        assert_eq!((b.kind, b.bbox[1], b.volume), (3, [0.25, 0.25, 0.5], 0.25));
        // in-use child masses add up to the archetype's
        assert_eq!(p.children.iter().filter(|c| c.in_use).map(|c| c.mass).sum::<f32>(), p.archetype.mass);

        let j = fragment_json(&p, &["root_bone".into(), "arm".into()]);
        assert_eq!(j["min_move_force"], -1.0);
        assert_eq!(j["groups"][0]["name"], "root_bone");
        assert_eq!(j["groups"][0]["child_groups"], serde_json::json!([1]));
        assert_eq!(j["groups"][1]["tuning"]["strength"], 375.0);
        assert_eq!(j["groups"][1]["flag_names"], serde_json::json!(["made_of_glass", "damage_when_broken", "fade_with_time"]));
        assert_eq!(j["groups"][1]["flag_bits_unnamed"], serde_json::json!([[0, 5]]));
        assert_eq!(j["groups"][0]["flag_names"], serde_json::json!([]));
        assert_eq!(j["children"][2]["bone_name"], "arm");
        assert_eq!(j["groups"][1]["children"], serde_json::json!([2]));
        assert_eq!(j["groups"][0]["children"], serde_json::json!([0]));
        assert_eq!(j["children"][0]["bound"]["type_name"], "box");
        assert_eq!(p.rotation_limits, vec![JointLimit { bone_hash: crate::meta7a::joaat("arm"), min: [0.0, 0.0, -1.5708], max: [0.0, 0.0, 1.5708] }]);
        assert_eq!(p.translation_limits[0].max, [0.5, 0.0, 0.0]);
        assert_eq!(j["rotation_limits"][0]["bone_name"], "arm");
        assert_eq!(j["rotation_limits"][0]["group"], 1);
        assert_eq!(j["translation_limits"][0]["max"], serde_json::json!([0.5, 0.0, 0.0]));
        // a group whose name hash does not match its bone gets no name
        assert!(fragment_json(&p, &["other".into(), "arm".into()])["groups"][0]["name"].is_null());
    }

    #[test]
    fn names_flag_bits() {
        // the two collide bits share a name and give it once
        let (n, rest) = group_flag_names([1 << 23 | 1 << 16 | 1 << 13, 1 << 3]);
        assert_eq!(n, ["disable_map_collision_until_break", "collide_convex_against_ground_or_larger_bounds"]);
        assert_eq!(rest, [[0, 13]]);
        // a fallen-log fragment's root flags
        let (n, rest) = root_flag_names(0x4113_0169);
        // bits 16 and 24 share one name, given once
        assert_eq!(n, ["any_group_disable_map_collision_until_break", "vegetation", "has_breakable_groups", "any_group_joint_angle_plastic_deflection"]);
        assert_eq!(rest, [0, 5, 6, 17, 30]);
    }

    #[test]
    fn rejects_broken_layouts() {
        let mut r = fragment();
        p64(&mut r, 0x300, 5); // group count disagrees with the root
        assert_eq!(try_decode(&r).err(), Some("arrays"));
        let mut r = fragment();
        f(&mut r, 0x15C, &[1.0]); // inverse mass no longer matches mass
        assert_eq!(try_decode(&r).err(), Some("archetype"));
        assert!(decode(&Resource { sys: vec![0; 0x200], gfx: Vec::new(), gfx_base: 0 }).is_none());
    }

    #[test]
    fn drawable_bound_lists_children() {
        let mut r = Resource { sys: vec![0; 0x1000], gfx: Vec::new(), gfx_base: 0 };
        ptr(&mut r, 0xC8, 0x600);
        bound(&mut r, 0x600, 6, [1.0, 1.0, 1.0], 0.0);
        ptr(&mut r, 0x670, 0x700);
        ptr(&mut r, 0x678, 0xB00);
        h(&mut r, 0x698, &[1, 1]);
        bound(&mut r, 0x900, 1, [0.1, 0.5, 0.1], 0.03);
        ptr(&mut r, 0x700, 0x900);
        f(&mut r, 0xB00 + 48, &[0.0, 0.0, 1.0]);
        let j = drawable_bound(&r).unwrap();
        assert_eq!(j["children"][0]["bound"]["type_name"], "capsule");
        assert_eq!(j["children"][0]["position"], serde_json::json!([0.0, 0.0, 1.0]));
        assert_eq!(j["composite"]["type"], 6);
    }

    #[test]
    fn tunable_entry_names_known_fields() {
        let item = serde_json::json!({"_struct": "1c3053ec", "b21037ab": 500.0, "48ee2434": 0.2, "deadbeef": true});
        let e = tunable_entry(&item);
        assert_eq!(e["object_mass"], 500.0);
        assert_eq!(e["buoyancy_factor"], 0.2);
        assert_eq!(e["fields"], serde_json::json!({"deadbeef": true}));
    }
}
