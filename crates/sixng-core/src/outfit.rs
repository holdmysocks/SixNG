//! Characters as one rigged model: the parts of an outfit (head, hair, top,
//! bottom, shoes …) on one shared skeleton, in one GLB, optionally with an
//! animation clip.
//!
//! Part selection (moved from the GUI's `assemble`): named characters
//! (`cs_ali`, `u_m_m_farm_monster_man`, `player_story000_01`) are sets of
//! clothing and body drawables `<prefix>_<item>_<fit>_…` with fit tags like
//! `mr1` / `fr1`; the default look is one part per outfit slot.
//!
//! Assembly: every part is skinned to the shared ped skeleton followed by its
//! own helper bones (`export::build_drawable`). The parts' joint lists are
//! merged by name into one skeleton (a helper bone that two parts define
//! differently is kept twice, the second renamed `<name>.<part>`), and each
//! part's vertex joints are remapped to it. Clip tracks drive the joint whose
//! joaat(lowercase name) is the track's bone id.
use crate::export::{ExportOptions, ExportedItem};
use crate::game::ArchiveInfo;
use crate::gltf::{AnimChannel, AnimPath, Joint, NodeGroup, TriPrim};
use crate::scan::{ArchiveIndex, Entry};
use anyhow::{bail, Result};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// Animal roots: `cr` + class letter (m mammal, b bird, f fish, r reptile,
/// a amphibian, c crustacean, i insect).
pub fn is_animal(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() > 4 && name.starts_with("cr") && b[2].is_ascii_lowercase() && b[3] == b'_'
}

/// Fit tag (`mr1`, `fr1`, `mk1`, `fb1` …): sex + body type + number.
pub fn is_fit(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 3 && (b[0] == b'm' || b[0] == b'f') && b[1].is_ascii_lowercase() && b[2].is_ascii_digit()
}

/// Character prefix of a ped part: the tokens before `<item>_<fit>`, when
/// there are at least two (`cs_ali_head_fr1_…` → `cs_ali`). Generic items
/// (`tshirtcrewneck_mr1_…`) and the watch set `s01_` have none.
pub fn outfit_prefix(name: &str) -> Option<String> {
    let t: Vec<&str> = name.split('_').collect();
    if t[0] == "char" || t[0] == "fchar" || t[0] == "s01" || is_animal(name) {
        return None;
    }
    let i = t.iter().position(|x| is_fit(x))?;
    if i < 3 {
        return None;
    }
    Some(t[..i - 1].join("_"))
}

/// Ped item that is a whole body on its own (`legacypedworld_mr1_023_fullbody…`).
pub fn is_whole_ped(name: &str) -> bool {
    name.starts_with("legacypedworld_") && name.contains("_fullbody")
}

/// Ped part (has a fit tag), shown under People.
pub fn is_ped_part(name: &str) -> bool {
    name.split('_').any(is_fit) || name.starts_with("char_") || name.starts_with("fchar_")
}

/// One choosable part of an animal or a character, for the viewer's Parts
/// panel. Parts in the same `group` replace each other (one per group, or none).
#[derive(Clone, Debug)]
pub struct PartOpt {
    pub off: u64,
    pub id: String,
    pub name: String,
    pub group: String,
    pub label: String,
    pub default: bool,
}

/// One entry per name: the largest copy (a part is stored once per LOD
/// under the same name, and not always LOD0 first: `sc_boss_demon`'s lower
/// body has LOD1 first, without a skeleton), the first in the archive on a tie.
pub fn dedupe<'a>(it: impl Iterator<Item = (String, &'a Entry)>) -> BTreeMap<String, &'a Entry> {
    let mut by_name: BTreeMap<String, &Entry> = BTreeMap::new();
    for (n, e) in it {
        by_name.entry(n).and_modify(|x| if (e.usize, std::cmp::Reverse(e.off)) > (x.usize, std::cmp::Reverse(x.off)) { *x = e }).or_insert(e);
    }
    by_name
}

/// Outfit slot of a ped item word (`tshirtcrewneck` → top); None = left out
/// (hats, bags, watches, jewellery other than piercings, masks …).
fn slot(item: &str) -> Option<&'static str> {
    const SLOTS: &[(&str, &str)] = &[
        ("hair", "hair"),
        ("beard", "beard"),
        ("brows", "brows"),
        ("lashes", "lashes"),
        ("eyes", "eyes"),
        ("teeth", "teeth"),
        ("bodyupper", "body_upper"),
        ("bodylower", "body_lower"),
        ("fullbody", "full"),
        ("dress", "full"),
        ("jumpsuit", "full"),
        ("overalls", "full"),
        ("jacket", "outer"),
        ("coat", "outer"),
        ("blazer", "outer"),
        ("cardigan", "outer"),
        ("tshirt", "top"),
        ("shirt", "top"),
        ("sweatshirt", "top"),
        ("hoodie", "top"),
        ("tanktop", "top"),
        ("blouse", "top"),
        ("bra", "top"),
        ("pants", "bottom"),
        ("shorts", "bottom"),
        ("skirt", "bottom"),
        ("undies", "bottom"),
        ("leggings", "bottom"),
        ("boots", "feet"),
        ("shoes", "feet"),
        ("sneakers", "feet"),
        ("sandals", "feet"),
        ("heels", "feet"),
        ("piercing", "piercing"),
    ];
    if item == "head" {
        return Some("head");
    }
    // longest matching word first (sweatshirt before shirt)
    SLOTS.iter().filter(|(w, _)| item.starts_with(w)).max_by_key(|(w, _)| w.len()).map(|(_, s)| *s)
}

/// Slot or accessory group of an item word, else the word itself.
pub fn slot_word(item: &str) -> String {
    slot(item).map(|s| s.replace('_', " ")).or_else(|| accessory_group(item).map(str::to_string)).unwrap_or_else(|| item.to_string())
}

/// Group of an accessory item word (`hatflatcap` → hat); None = its own word.
fn accessory_group(item: &str) -> Option<&'static str> {
    const G: &[(&str, &str)] = &[
        ("hat", "hat"),
        ("cap", "hat"),
        ("helmet", "hat"),
        ("beanie", "hat"),
        ("sunglasses", "glasses"),
        ("glasses", "glasses"),
        ("backpack", "bag"),
        ("bag", "bag"),
        ("watch", "watch"),
        ("necklace", "neck"),
        ("chain", "neck"),
        ("earring", "ears"),
        ("glove", "gloves"),
        ("mask", "mask"),
    ];
    G.iter().filter(|(w, _)| item.starts_with(w)).max_by_key(|(w, _)| w.len()).map(|(_, g)| *g)
}

/// Every part of a character prefix, grouped by outfit slot (or by item word
/// for hats, bags, watches …). The default look is one part per slot (the
/// first in name order) plus the eyes that go with the head; a one-piece
/// outfit replaces top and bottom (a character with no slot parts shows the
/// first part of each group). Accessories, `_alt_` pieces, holsters and lash
/// cards are offered but off by default; clipping-cut (`_cull`) and
/// first-person copies are left out.
pub fn outfit_options(ix: &ArchiveIndex, prefix: &str) -> Vec<PartOpt> {
    let p = format!("{}_", prefix.to_ascii_lowercase());
    let by_name = dedupe(ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| {
        let n = e.name.as_ref()?.to_ascii_lowercase();
        (n.starts_with(&p) && outfit_prefix(&n).as_deref() == Some(prefix)).then_some((n, e))
    }));
    options_from(&p, by_name.iter().map(|(n, e)| (n.as_str(), e.off, e.id.as_str())))
}

/// `outfit_options` over (lower-case name, offset, id) in name order;
/// `p` is the prefix with its trailing underscore.
fn options_from<'a>(p: &str, parts: impl Iterator<Item = (&'a str, u64, &'a str)>) -> Vec<PartOpt> {
    let mut opts = Vec::new();
    let mut chosen: BTreeMap<String, u64> = BTreeMap::new();
    for (n, off, id) in parts {
        if ["_cull", "_fp", "eyecap", "tattooprojection"].iter().any(|x| n.contains(x)) {
            continue;
        }
        let rest = &n[p.len()..];
        let item = rest.split('_').next().unwrap_or("");
        let optional = n.contains("_alt_") || n.contains("holster") || n.contains("_eyelashes");
        let group = match slot(item) {
            Some("head") if n.contains("_eyes") => "head eyes".to_string(),
            // lashes stored as a head part (`…_head_…_eyelashes_full`): their
            // own group, or they would stand in for the head itself; off by
            // default (alpha cards, drawn as solid white slabs here)
            Some("head") if n.contains("_eyelashes") => "head lashes".to_string(),
            // heads with an accessory baked in (`cs_raul_head_…_earring_full`)
            // are their own group, so the plain head stays the default head
            Some("head") if n.contains("_earring") => "head earring".to_string(),
            Some(s) => s.replace('_', " "),
            None => accessory_group(item).unwrap_or(item).to_string(),
        };
        if !optional && slot(item).is_some() {
            chosen.entry(group.clone()).or_insert(off);
        }
        opts.push(PartOpt { off, id: id.to_string(), name: n.to_string(), group, label: rest.to_string(), default: false });
    }
    if chosen.contains_key("full") {
        chosen.remove("top");
        chosen.remove("bottom");
    }
    // A suit jacket goes with dress trousers and a dress shirt (as in the
    // outfit list's suit looks): with one as the default outer, the default
    // bottom is the character's dress trousers when it has some, and a top
    // that is not a dress shirt (a polo showing through a closed suit) is off.
    let name_of = |off: u64| opts.iter().find(|o| o.off == off).map_or("", |o| o.name.as_str());
    if chosen.get("outer").is_some_and(|&o| name_of(o)[p.len()..].starts_with("jacketsuit")) {
        let dress = |o: &&PartOpt| o.group == "bottom" && o.name[p.len()..].starts_with("pantsdress") && !o.name.contains("_alt_");
        if let Some(d) = opts.iter().find(dress) {
            chosen.insert("bottom".into(), d.off);
        }
        if chosen.get("top").is_some_and(|&t| !name_of(t)[p.len()..].starts_with("shirtdress")) {
            chosen.remove("top");
        }
    }
    // A character whose parts are all outside the outfit slots (one
    // `legacypedcs_…_fullbody` model) would show nothing: the first part of
    // each of its groups stands in.
    if chosen.is_empty() {
        for o in opts.iter().filter(|o| !["_alt_", "holster", "_eyelashes"].iter().any(|x| o.name.contains(x))) {
            chosen.entry(o.group.clone()).or_insert(o.off);
        }
    }
    for o in &mut opts {
        o.default = chosen.get(&o.group) == Some(&o.off);
    }
    opts
}

/// Default parts of a character prefix (see `outfit_options`).
pub fn outfit_parts<'a>(ix: &'a ArchiveIndex, prefix: &str) -> Vec<&'a Entry> {
    let offs: Vec<u64> = outfit_options(ix, prefix).into_iter().filter(|o| o.default).map(|o| o.off).collect();
    ix.entries.iter().filter(|e| offs.contains(&e.off)).collect()
}


/// Group words of animal parts left out of the default look: accessories,
/// corpse and roadkill copies, and `fur` (3-vertex placeholders with their
/// own skeleton copy; the fur itself is strand grooms, see `sixng grooms`).
const ANIMAL_SKIP: &[&str] = &[
    "fur",
    "generic", "costume", "collar", "harness", "hat", "badge", "ribbon", "bandana", "cape", "lace", "flower", "knife", "vest", "rope", "tranquilizer", "loop", "shadesaviators", "accessories", "human", "stick", "corpse", "roadkill", "gut", "gonad", "bone",
];

/// Every part of an animal root: `<root>_<words>_<NN>[_<set>]` (`body_00`,
/// `body_08_collarharness`, `harness_08_collarharness`). The default look is
/// the `_00` parts without accessories. Roots without parts of their own fall
/// back to `<root minus _NN>_…`.
pub fn animal_options(ix: &ArchiveIndex, root: &str) -> Vec<PartOpt> {
    let root = root.to_ascii_lowercase();
    let mut prefixes = vec![format!("{root}_")];
    if let Some((stem, n)) = root.rsplit_once('_') {
        if n.chars().all(|c| c.is_ascii_digit()) {
            // A variant root without parts of its own (`…_m_01`: a skeleton
            // only) takes the parts of the lowest-numbered sibling that has
            // them (`…_m_00_body_00`).
            let own = prefixes[0].clone();
            prefixes.extend((0..10).map(|k| format!("{stem}_{k:02}_")).filter(|p| *p != own));
            prefixes.push(format!("{stem}_"));
        }
    }
    let mut first_any: Vec<PartOpt> = Vec::new();
    for p in prefixes {
        let by_name = dedupe(ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| {
            let n = e.name.as_ref()?.to_ascii_lowercase();
            n.starts_with(&p).then_some((n, e))
        }));
        let mut opts = Vec::new();
        for (n, e) in by_name {
            let toks: Vec<&str> = n[p.len()..].split('_').collect();
            let Some(j) = toks.iter().position(|t| t.len() == 2 && t.chars().all(|c| c.is_ascii_digit())) else { continue };
            let group = &toks[..j];
            if group.is_empty() || group.iter().any(|t| t.is_empty() || !t.chars().all(|c| c.is_ascii_lowercase())) {
                continue;
            }
            let set = toks[j + 1..].join(" ");
            let default = toks[j] == "00" && set.is_empty() && !group.iter().any(|t| ANIMAL_SKIP.contains(t));
            let label = if toks[j] == "00" && set.is_empty() { "default".to_string() } else { format!("{} {}", toks[j], set).trim().to_string() };
            opts.push(PartOpt { off: e.off, id: e.id.clone(), name: n.clone(), group: group.join(" "), label, default });
        }
        if opts.iter().any(|o| o.default) {
            return opts;
        }
        if first_any.is_empty() {
            first_any = opts;
        }
    }
    first_any
}

/// Default parts of an animal root: `<root>_<words>_00` without accessories.
pub fn animal_parts<'a>(ix: &'a ArchiveIndex, root: &str) -> Vec<&'a Entry> {
    let offs: Vec<u64> = animal_options(ix, root).into_iter().filter(|o| o.default).map(|o| o.off).collect();
    ix.entries.iter().filter(|e| offs.contains(&e.off)).collect()
}

/// One assemblable character or animal (see `characters`).
#[derive(Clone, Debug, serde::Serialize)]
pub struct CharacterInfo {
    pub name: String,
    /// "character" (outfit prefix) or "animal" (fragment root).
    pub kind: &'static str,
    /// Distinct part names (each stored once per LOD).
    pub parts: usize,
    /// Parts in the default look.
    pub default_parts: usize,
}

/// Every named character (outfit prefix with parts) and every animal root
/// (an `ft` fragment `cr?_…` with parts of its own) in an archive index,
/// sorted by name.
pub fn characters(ix: &ArchiveIndex) -> Vec<CharacterInfo> {
    let mut prefixes: BTreeMap<String, ()> = BTreeMap::new();
    let mut roots: BTreeMap<String, ()> = BTreeMap::new();
    for e in &ix.entries {
        let Some(n) = e.name.as_ref().map(|n| n.to_ascii_lowercase()) else { continue };
        match e.rtype.as_deref() {
            Some("dr") => {
                if let Some(p) = outfit_prefix(&n) {
                    prefixes.insert(p, ());
                }
            }
            Some("ft") if is_animal(&n) => {
                roots.insert(n, ());
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    // A prefix without a head whose parts belong to a longer-named character
    // (`cs_andres_beardgoatee_…` beside `cs_andres_deleon_head_…`) is not a
    // character of its own.
    let headed: Vec<&String> = prefixes.keys().filter(|p| outfit_options(ix, p).iter().any(|o| o.group == "head")).collect();
    for p in prefixes.keys() {
        let o = outfit_options(ix, p);
        if !o.iter().any(|x| x.group == "head") && headed.iter().any(|h| h.starts_with(&format!("{p}_"))) {
            continue;
        }
        if !o.is_empty() {
            out.push(CharacterInfo { name: p.clone(), kind: "character", parts: o.len(), default_parts: o.iter().filter(|x| x.default).count() });
        }
    }
    for r in roots.keys() {
        // only roots with parts named after themselves (not the shared `_NN`-less fallback)
        let o: Vec<PartOpt> = animal_options(ix, r).into_iter().filter(|x| x.name.starts_with(&format!("{r}_"))).collect();
        if !o.is_empty() {
            out.push(CharacterInfo { name: r.clone(), kind: "animal", parts: o.len(), default_parts: o.iter().filter(|x| x.default).count() });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Merge the joint lists of several parts into one skeleton. Joints with the
/// same name, the same parent name and the same bind (within 0.1 mm / 1e-4)
/// are one joint; a name that comes back with another bind or parent is
/// added again as `<name>.<part index>`. Returns the merged joints and, per
/// part, its joint index -> merged joint index.
pub fn merge_skeletons(parts: &[Vec<Joint>]) -> (Vec<Joint>, Vec<Vec<u16>>) {
    let mut out: Vec<Joint> = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut maps = Vec::with_capacity(parts.len());
    let close = |a: &[f32; 16], b: &[f32; 16]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
    for (pi, joints) in parts.iter().enumerate() {
        let mut map = vec![u16::MAX; joints.len()];
        // parents first: joints are not always ordered parent-before-child
        let mut order: Vec<usize> = Vec::with_capacity(joints.len());
        let mut placed = vec![false; joints.len()];
        while order.len() < joints.len() {
            let before = order.len();
            for (k, j) in joints.iter().enumerate() {
                if !placed[k] && j.parent.filter(|&p| p < joints.len() && p != k).map_or(true, |p| placed[p]) {
                    placed[k] = true;
                    order.push(k);
                }
            }
            if order.len() == before {
                // a parent cycle: take the rest as they are
                order.extend((0..joints.len()).filter(|&k| !placed[k]));
                break;
            }
        }
        for k in order {
            let j = &joints[k];
            let parent = j.parent.filter(|&p| p < joints.len()).map(|p| map[p] as usize).filter(|&p| p < out.len());
            let parent_name = |p: Option<usize>| p.map(|p| out[p].name.as_str());
            let same = by_name.get(&j.name).copied().filter(|&m| {
                let o = &out[m];
                close(&o.bind, &j.bind) && o.pose.zip(j.pose).map_or(o.pose.is_none() == j.pose.is_none(), |(a, b)| close(&a, &b)) && parent_name(o.parent) == parent_name(parent)
            });
            map[k] = match same {
                Some(m) => m as u16,
                None => {
                    let name = if by_name.contains_key(&j.name) { format!("{}.{pi}", j.name) } else { j.name.clone() };
                    by_name.entry(j.name.clone()).or_insert(out.len());
                    out.push(Joint { name, parent, bind: j.bind, pose: j.pose });
                    (out.len() - 1) as u16
                }
            };
        }
        maps.push(map);
    }
    (out, maps)
}

/// Point a primitive's vertex joints at the merged skeleton; indices with no
/// joint become u16::MAX (dropped by the GLB writer, weight moved).
pub fn remap_joints(p: &mut TriPrim, map: &[u16]) {
    let f = |js: &mut Vec<[u16; 4]>| js.iter_mut().flatten().for_each(|j| *j = map.get(*j as usize).copied().unwrap_or(u16::MAX));
    p.joints.as_mut().map(f);
    p.joints1.as_mut().map(f);
}

/// The animation channels of a clip on a skeleton, plus coverage counts.
pub fn clip_channels(d: &crate::anim::ClipDictionary, c: &crate::anim::Clip, joints: &[Joint]) -> (Vec<AnimChannel>, serde_json::Value) {
    use crate::anim::track;
    let mut by_hash: HashMap<u32, usize> = HashMap::new();
    for (i, j) in joints.iter().enumerate() {
        by_hash.entry(crate::meta7a::joaat(&j.name)).or_insert(i);
    }
    let scale = track::scale();
    let all = crate::anim::all_tracks(d, c);
    let bone_tracks: Vec<_> = all.iter().filter(|t| t.0 != 0 && t.2 != crate::anim::ValueType::Float).collect();
    let mapped = bone_tracks.iter().filter(|t| by_hash.contains_key(&t.0)).count();
    let mut unmapped: Vec<String> = bone_tracks.iter().filter(|t| !by_hash.contains_key(&t.0)).map(|t| format!("{:08x}", t.0)).collect();
    unmapped.sort();
    unmapped.dedup();
    let mut channels = Vec::new();
    let mut skipped_kinds: BTreeMap<String, usize> = BTreeMap::new();
    for t in crate::anim::clip_tracks(d, c) {
        let Some(&joint) = by_hash.get(&t.bone) else { continue };
        let path = match t.kind {
            track::ROTATION => AnimPath::Rotation,
            track::TRANSLATION => AnimPath::Translation,
            k if k == scale => AnimPath::Scale,
            k => {
                *skipped_kinds.entry(format!("{k:08x}")).or_default() += 1;
                continue;
            }
        };
        channels.push(AnimChannel { joint, path, times: t.times, values: t.values });
    }
    let stats = serde_json::json!({
        "tracks": all.len(),
        "mover_tracks": all.iter().filter(|t| t.0 == 0).count(),
        "float_tracks": all.iter().filter(|t| t.0 != 0 && t.2 == crate::anim::ValueType::Float).count(),
        "bone_tracks": bone_tracks.len(),
        "bone_tracks_on_joints": mapped,
        "bone_tracks_on_joints_pct": if bone_tracks.is_empty() { 0.0 } else { 100.0 * mapped as f64 / bone_tracks.len() as f64 },
        "unmapped_bones": unmapped,
        "channels": channels.len(),
        "other_track_types_skipped": skipped_kinds,
    });
    (channels, stats)
}

/// Put a clip's movement over the ground (`anim::clip_mover`) on the root
/// joint: at every key time the root's own translation and rotation (its
/// channels, else its rest pose) are moved and turned by the mover since
/// the clip's start. Returns what the clip moves and turns over its
/// length (metres, degrees about the vertical), or None when the clip has
/// no mover tracks or the skeleton no root.
pub fn add_mover(channels: &mut Vec<AnimChannel>, d: &crate::anim::ClipDictionary, c: &crate::anim::Clip, joints: &[Joint]) -> Option<(f32, f32)> {
    let mover = crate::anim::clip_mover(d, c);
    let root = joints.iter().position(|j| j.parent.is_none())?;
    if mover.is_empty() {
        return None;
    }
    fn mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        [
            a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
            a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
            a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
            a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
        ]
    }
    fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
        let r = mul(mul(q, [v[0], v[1], v[2], 0.0]), [-q[0], -q[1], -q[2], q[3]]);
        [r[0], r[1], r[2]]
    }
    fn sample(ch: &AnimChannel, t: f32) -> [f32; 4] {
        let n = ch.times.len().min(ch.values.len());
        let i = ch.times[..n].partition_point(|&x| x <= t);
        if i == 0 {
            return ch.values[0];
        }
        if i >= n {
            return ch.values[n - 1];
        }
        let (a, mut b) = (ch.values[i - 1], ch.values[i]);
        let k = ((t - ch.times[i - 1]) / (ch.times[i] - ch.times[i - 1]).max(1e-9)).clamp(0.0, 1.0);
        if ch.path == AnimPath::Rotation && a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3] < 0.0 {
            b = b.map(|x| -x);
        }
        std::array::from_fn(|c| a[c] + (b[c] - a[c]) * k)
    }
    // the root's rest pose: translation and rotation of its matrix (column-major, no shear)
    let m = joints[root].pose.unwrap_or(joints[root].bind);
    let rest_t = [m[12], m[13], m[14], 0.0];
    let len = |c: usize| (m[c * 4] * m[c * 4] + m[c * 4 + 1] * m[c * 4 + 1] + m[c * 4 + 2] * m[c * 4 + 2]).sqrt().max(1e-9);
    let r = |row: usize, col: usize| m[col * 4 + row] / len(col);
    let w = (1.0 + r(0, 0) + r(1, 1) + r(2, 2)).max(0.0).sqrt() / 2.0;
    let rest_q = if w > 1e-4 {
        [(r(2, 1) - r(1, 2)) / (4.0 * w), (r(0, 2) - r(2, 0)) / (4.0 * w), (r(1, 0) - r(0, 1)) / (4.0 * w), w]
    } else {
        // a half turn: the axis is the column of largest diagonal
        let x = ((1.0 + r(0, 0)) / 2.0).max(0.0).sqrt();
        let y = ((1.0 + r(1, 1)) / 2.0).max(0.0).sqrt();
        let z = ((1.0 + r(2, 2)) / 2.0).max(0.0).sqrt();
        if x >= y && x >= z {
            [x, r(1, 0) / (2.0 * x), r(2, 0) / (2.0 * x), 0.0]
        } else if y >= z {
            [r(1, 0) / (2.0 * y), y, r(2, 1) / (2.0 * y), 0.0]
        } else {
            [r(2, 0) / (2.0 * z), r(2, 1) / (2.0 * z), z, 0.0]
        }
    };
    let take = |channels: &mut Vec<AnimChannel>, path: AnimPath| channels.iter().position(|ch| ch.joint == root && ch.path == path && !ch.times.is_empty()).map(|i| channels.remove(i));
    let (own_t, own_r) = (take(channels, AnimPath::Translation), take(channels, AnimPath::Rotation));
    let mut times: Vec<f32> = mover.iter().flat_map(|m| m.times.iter().copied()).chain(own_t.iter().chain(own_r.iter()).flat_map(|ch| ch.times.iter().copied())).collect();
    times.sort_by(f32::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
    let (mut tv, mut rv) = (Vec::with_capacity(times.len()), Vec::with_capacity(times.len()));
    for &t in &times {
        let (p, q) = crate::anim::mover_at(&mover, t);
        let t0 = own_t.as_ref().map_or(rest_t, |ch| sample(ch, t));
        let r0 = own_r.as_ref().map_or(rest_q, |ch| sample(ch, t));
        let l = r0.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
        let moved = rotate(q, [t0[0], t0[1], t0[2]]);
        tv.push([p[0] + moved[0], p[1] + moved[1], p[2] + moved[2], 0.0]);
        rv.push(mul(q, r0.map(|x| x / l)));
    }
    channels.push(AnimChannel { joint: root, path: AnimPath::Translation, times: times.clone(), values: tv });
    channels.push(AnimChannel { joint: root, path: AnimPath::Rotation, times, values: rv });
    let (p, q) = crate::anim::mover_at(&mover, c.duration);
    Some(((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt(), 2.0 * q[2].atan2(q[3]).to_degrees()))
}

/// Write one rigged GLB of `parts` (LOD0) at `out`. Textures go to
/// `<out folder>/textures/`. Each clip becomes one glTF animation, named
/// `<dictionary>/<clip>`. With `anim_only` the GLB holds the skeleton and
/// the animations without meshes or textures. Returns the JSON report.
pub fn export_outfit(a: &ArchiveInfo, parts: &[&Entry], name: &str, out: &Path, o: &ExportOptions, clips: &[(&crate::anim::ClipDictionary, &crate::anim::Clip)], anim_only: bool, in_place: bool) -> Result<serde_json::Value> {
    if parts.is_empty() {
        bail!("no parts selected");
    }
    let dir = out.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&dir)?;
    let mut o = o.clone();
    o.out = dir.clone();
    if anim_only {
        o.png = false;
        o.dds = false;
        o.skip_embedded_textures = true;
    }
    let mut groups: Vec<NodeGroup> = Vec::new();
    let mut materials = Vec::new();
    let mut part_joints: Vec<Vec<Joint>> = Vec::new();
    let mut reports = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    for e in parts {
        let mut item = ExportedItem { id: e.id.clone(), archive: a.id.clone(), kind: e.kind.name(), rtype: e.rtype.clone(), name: e.name.clone(), status: "ok".into(), files: vec![], error: None, warnings: vec![] };
        let r = crate::resource::Resource::load(&a.path, e)?;
        let info = crate::drawable::describe(&r);
        let mut b = crate::export::build_drawable(a, e, &r, &info, &dir, false, &o, &mut item);
        let pname = e.name.clone().unwrap_or_else(|| e.id.clone());
        let mut prims: Vec<TriPrim> = b.lod_groups.drain(..).next().unwrap_or_default().into_iter().flat_map(|g| g.prims).collect();
        // worn under something: the triangles whose vertices all lie in
        // regions the other parts hide (skin, and clothes drawn as their
        // `_cull_00` twin)
        let mut hidden_tris = 0;
        if let Some((own, waist)) = o.part_hidden.get(&pname.to_ascii_lowercase()) {
            let hide: Vec<bool> = (0..256u32).map(|n| n != 0 && own.iter().any(|k| crate::wardrobe::mask_has(k, n))).collect();
            let hide2: Vec<bool> = (0..256u32).map(|n| n != 0 && waist.iter().any(|k| crate::wardrobe::mask_has(k, n))).collect();
            for p in &mut prims {
                let Some(r) = p.regions.as_ref() else { continue };
                let gone = |i: u32| r.get(i as usize).is_some_and(|r| hide[r[0] as usize] || hide2[r[1] as usize]);
                let before = p.indices.len();
                p.indices = p.indices.chunks_exact(3).filter(|t| !t.iter().all(|&i| gone(i))).flatten().copied().collect();
                hidden_tris += (before - p.indices.len()) / 3;
            }
        }
        let off = materials.len();
        for p in &mut prims {
            p.material = p.material.map(|m| m + off);
            p.name = format!("{pname}/{}", p.name);
        }
        let verts: usize = prims.iter().map(|p| p.positions.len()).sum();
        let skinned = prims.iter().any(|p| p.joints.is_some()) && !b.joints.is_empty();
        reports.push(serde_json::json!({"name": pname, "id": e.id, "vertices": verts, "triangles": prims.iter().map(|p| p.indices.len() / 3).sum::<usize>(), "joints": b.joints.len(), "skinned": skinned, "hidden_triangles": hidden_tris, "warnings": item.warnings}));
        warnings.extend(item.warnings.iter().map(|w| format!("{pname}: {w}")));
        materials.extend(b.materials.drain(..).map(|mut m| {
            m.name = format!("{pname}/{}", m.name);
            m
        }));
        part_joints.push(if skinned { b.joints } else { Vec::new() });
        groups.push(NodeGroup { name: pname, prims });
    }
    let (joints, maps) = merge_skeletons(&part_joints);
    for (g, map) in groups.iter_mut().zip(&maps) {
        for p in &mut g.prims {
            if map.is_empty() {
                p.joints = None;
                p.joints1 = None;
            } else {
                remap_joints(p, map);
            }
        }
    }
    let mut anims = Vec::new();
    let mut clip_reports = Vec::new();
    for &(d, c) in clips {
        if c.kind != 1 {
            bail!("clip {} has type {} (only animation clips, type 1, are decoded)", c.short_name(), c.kind);
        }
        if c.bindings.is_empty() && !c.pending.is_empty() {
            bail!("clip {} of {}: all {} of its animation windows are streamed in dictionaries that are not loaded (a cutscene's `-N` sections: pick it as cin@<scene>/<clip>)", c.short_name(), d.name, c.pending.len());
        }
        if c.bindings.is_empty() {
            bail!("clip {} of {} binds no animation in this dictionary (not decoded yet); pick another clip", c.short_name(), d.name);
        }
        let (mut channels, mut stats) = clip_channels(d, c, &joints);
        // the clip's own movement over the ground, on the root joint
        stats["ground_movement"] = match (in_place, if in_place { None } else { add_mover(&mut channels, d, c, &joints) }) {
            (true, _) => serde_json::json!({"applied": false, "note": "left out (--in-place)"}),
            (_, Some((metres, degrees))) => serde_json::json!({"applied": true, "metres": metres, "turn_degrees": degrees, "note": "the mover's absolute translation and rotation since the clip's start, on the root joint's translation and rotation; --in-place leaves it out"}),
            (_, None) => serde_json::json!({"applied": false, "note": "the clip has no mover tracks"}),
        };
        stats["dictionary"] = d.name.clone().into();
        stats["clip"] = c.short_name().into();
        stats["duration"] = c.duration.into();
        if c.sections > 1 || !c.pending.is_empty() {
            stats["sections"] = c.sections.into();
            stats["missing_windows"] = c.pending.len().into();
        }
        stats["frames"] = channels.iter().map(|ch| ch.times.len()).max().unwrap_or(0).into();
        clip_reports.push(stats);
        anims.push(crate::gltf::Animation { name: format!("{}/{}", d.name, c.short_name()), channels });
    }
    // one clip: the report as before (an object); several: a list
    let clip_report = match clip_reports.len() {
        0 => serde_json::Value::Null,
        1 => clip_reports.pop().unwrap(),
        _ => serde_json::Value::Array(clip_reports),
    };
    if anim_only {
        groups.clear();
        materials.clear();
    }
    let extras = serde_json::json!({"outfit": name, "parts": parts.iter().map(|e| e.name.clone()).collect::<Vec<_>>(), "coordinate_system": crate::export::COORDS, "anim_only": anim_only});
    let opt = crate::gltf::GlbOptions { animations: &anims, trs_joints: true, skinned_meshes_at_root: true };
    let glb = crate::gltf::mesh_glb_with(name, &groups, &materials, &joints, extras, &opt);
    crate::atomic::write(out, &glb)?;
    // weighted influences whose bone is in no skeleton (weight moved by the writer)
    let unresolved_of = |p: &TriPrim| -> usize {
        let sets = [(&p.joints, &p.weights), (&p.joints1, &p.weights1)];
        sets.iter().filter_map(|(j, w)| j.as_ref().zip(w.as_ref())).map(|(j, w)| j.iter().zip(w).flat_map(|(a, b)| a.iter().zip(b)).filter(|(&j, &w)| j == u16::MAX && w > 0).count()).sum()
    };
    for (g, r) in groups.iter().zip(reports.iter_mut()) {
        r["unresolved_influences"] = g.prims.iter().map(unresolved_of).sum::<usize>().into();
    }
    let unresolved: usize = groups.iter().flat_map(|g| &g.prims).map(unresolved_of).sum();
    Ok(serde_json::json!({
        "glb": out, "bytes": glb.len(), "outfit": name, "parts": reports, "joints": joints.len(),
        "renamed_joints": joints.iter().filter(|j| j.name.contains('.')).map(|j| j.name.clone()).collect::<Vec<_>>(),
        "joint_roots": joints.iter().filter(|j| j.parent.is_none()).map(|j| j.name.clone()).collect::<Vec<_>>(),
        "unresolved_influences": unresolved,
        "clip": clip_report, "warnings": warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn j(name: &str, parent: Option<usize>, x: f32) -> Joint {
        let mut bind = [0f32; 16];
        bind[0] = 1.0;
        bind[5] = 1.0;
        bind[10] = 1.0;
        bind[15] = 1.0;
        bind[12] = x;
        Joint { name: name.into(), parent, bind, pose: None }
    }

    /// Two parts on a shared root + spine, each with its own helper; the
    /// second redefines the first's helper with another bind.
    #[test]
    fn merge_and_remap() {
        let a = vec![j("root", None, 0.0), j("spine", Some(0), 1.0), j("GM_a", Some(1), 1.5)];
        // listed child-first to check the parent ordering
        let b = vec![j("GM_b", Some(2), 2.5), j("root", None, 0.0), j("spine", Some(1), 1.0), j("GM_a", Some(2), 9.0)];
        let (m, maps) = merge_skeletons(&[a, b]);
        let names: Vec<&str> = m.iter().map(|j| j.name.as_str()).collect();
        assert_eq!(names, ["root", "spine", "GM_a", "GM_a.1", "GM_b"]);
        assert_eq!(maps[0], [0, 1, 2]);
        assert_eq!(maps[1], [4, 0, 1, 3]);
        assert_eq!(m[3].parent, Some(1));
        assert_eq!(m[4].parent, Some(1));
        assert_eq!(m[3].bind[12], 9.0);
        assert_eq!(m[0].parent, None);
        let mut p = TriPrim {
            name: "p".into(), positions: vec![[0.0; 3]; 2], normals: None, tangents: None, uvs: vec![], colors: None, colors1: None, regions: None, tint: None, indices: vec![], material: None,
            joints: Some(vec![[0, 3, 2, 7], [1, 1, 1, 1]]), weights: Some(vec![[100, 100, 55, 0], [255, 0, 0, 0]]), joints1: None, weights1: None,
        };
        remap_joints(&mut p, &maps[1]);
        assert_eq!(p.joints.unwrap(), [[4, 3, 1, u16::MAX], [0, 0, 0, 0]]);
    }

    #[test]
    fn default_parts() {
        let opts = |names: &[&str]| {
            let v: Vec<(String, u64)> = names.iter().enumerate().map(|(i, n)| (n.to_string(), i as u64)).collect();
            options_from("x_", v.iter().map(|(n, o)| (n.as_str(), *o, ""))).into_iter().filter(|o| o.default).map(|o| o.name).collect::<Vec<_>>()
        };
        // lashes stored as a head part do not replace the head (and start off)
        assert_eq!(opts(&["x_head_mr1_000_eyelashes_full", "x_head_mr1_000_eyes_full", "x_head_mr1_000_full"]), ["x_head_mr1_000_eyes_full", "x_head_mr1_000_full"]);
        // only parts outside the slots: the first of each group still shows
        assert_eq!(opts(&["x_legacypedcs_mr1_000_fullbody_full"]), ["x_legacypedcs_mr1_000_fullbody_full"]);
        // with a slot part, accessories stay off
        assert_eq!(opts(&["x_hatflatcap_mr1_000_full", "x_head_mr1_000_full"]), ["x_head_mr1_000_full"]);
        // a suit jacket takes the dress trousers and no polo under it
        let suit = ["x_head_mr1_000_full", "x_jacketsuit_mr1_000_o_hl_long_full", "x_pantschinos_mr1_000_mw_long_full", "x_pantsdress_mr1_000_mw_long_full", "x_tshirtpolo_mr1_000_oc_tu_short_full"];
        assert_eq!(opts(&suit), ["x_head_mr1_000_full", "x_jacketsuit_mr1_000_o_hl_long_full", "x_pantsdress_mr1_000_mw_long_full"]);
        // with a dress shirt, the shirt stays
        assert_eq!(opts(&["x_jacketsuit_mr1_000_o_hl_long_full", "x_shirtdress_mr1_000_oc_tu_long_full"]), ["x_jacketsuit_mr1_000_o_hl_long_full", "x_shirtdress_mr1_000_oc_tu_long_full"]);
    }

    /// Real data: a whole outfit with a walk clip, from a local install.
    /// `SIXNG_GAME=<install> SIXNG_WORK=<work with packs and anim indexed>
    /// cargo test -p sixng-core outfit_real -- --ignored --nocapture`
    /// Checks: one joint list with every weighted influence bound, the clip's
    /// bone tracks mostly on joints, and rest-pose skinning = stored vertices.
    #[test]
    #[ignore]
    fn outfit_real() {
        let (Some(game), Some(work)) = (std::env::var_os("SIXNG_GAME"), std::env::var_os("SIXNG_WORK")) else { return };
        let ws = crate::export::Workspace::new(&game, &work);
        let a = ws.resolve("packs").unwrap();
        let ix = ws.load_index(&a).unwrap();
        let (_, d) = crate::anim::load_dictionary(Path::new(&game), Path::new(&work), "gameplay@locomotion@female_character@kat@normal@standing@walk@stance_neutral").unwrap();
        let clip = d.clip("move").unwrap();
        let parts = outfit_parts(&ix, "player_story000_00");
        assert!(parts.len() >= 5, "{} parts", parts.len());
        let out = std::env::temp_dir().join("sixng_outfit_real").join("outfit.glb");
        let o = ExportOptions { out: out.parent().unwrap().into(), png: false, dds: false, raw: false, force: true, skip_embedded_textures: true, body_shape: vec![], head_skin: 0, paint: None, tex_remap: Default::default(), ped_tints: Default::default(), part_skins: Default::default(), part_colours: Default::default(), part_hidden: Default::default() };
        let r = export_outfit(&a, &parts, "player_story000_00", &out, &o, &[(&d, clip)], false, false).unwrap();
        println!("{}", serde_json::to_string_pretty(&r).unwrap());
        assert_eq!(r["unresolved_influences"], 0);
        assert!(r["clip"]["bone_tracks_on_joints_pct"].as_f64().unwrap() >= 90.0);
        assert!(r["parts"].as_array().unwrap().iter().filter(|p| p["vertices"].as_u64().unwrap() > 100).all(|p| p["skinned"] == true));
        // rest pose: the joint nodes compose to the bind matrices
        let glb = std::fs::read(&out).unwrap();
        let jl = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        let doc: serde_json::Value = serde_json::from_slice(&glb[20..20 + jl]).unwrap();
        assert_eq!(doc["skins"].as_array().unwrap().len(), 1);
        assert_eq!(doc["animations"][0]["channels"].as_array().unwrap().len(), r["clip"]["channels"].as_u64().unwrap() as usize);
    }

    /// The mover goes on the root joint: its own keys are moved and turned.
    #[test]
    fn mover_moves_and_turns_the_root() {
        use crate::anim::{track, Animation, Binding, Clip, ClipDictionary, Track, ValueType};
        let h = std::f32::consts::FRAC_1_SQRT_2;
        // two frames, one second: 2 m forward (+Y) and a quarter turn left
        let tracks = vec![
            Track { bone: 0, kind: track::MOVER_TRANSLATION, value: ValueType::Vector3, frames: vec![[0.0, 0.0, 0.0, 0.0], [0.0, 2.0, 0.0, 0.0]] },
            Track { bone: 0, kind: track::MOVER_ROTATION, value: ValueType::Quaternion, frames: vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, h, h]] },
        ];
        let d = ClipDictionary { name: "d".into(), clips: vec![], animations: vec![Animation { hash: 0, name: "a".into(), frames: 2, duration: 1.0, tracks }] };
        let c = Clip { hash: 0, kind: 1, name: "c".into(), duration: 1.0, bindings: vec![Binding { animation: 0, start: 0.0, end: 1.0, rate: 1.0, offset: 0.0 }], pending: Vec::new(), sections: 1 };
        // the root sits 1 m to the side (+X) at rest, and has no keys of its own
        let joints = vec![j("root", None, 1.0), j("spine", Some(0), 1.0)];
        let mut ch = Vec::new();
        let (metres, degrees) = add_mover(&mut ch, &d, &c, &joints).unwrap();
        assert!((metres - 2.0).abs() < 1e-5 && (degrees - 90.0).abs() < 1e-3, "{metres} {degrees}");
        let t = ch.iter().find(|x| x.joint == 0 && x.path == AnimPath::Translation).unwrap();
        let r = ch.iter().find(|x| x.joint == 0 && x.path == AnimPath::Rotation).unwrap();
        assert_eq!(t.values[0], [1.0, 0.0, 0.0, 0.0]);
        // turned a quarter left, the 1 m to the side points forward: (0, 2 + 1, 0)
        let e = t.values.last().unwrap();
        assert!(e[0].abs() < 1e-5 && (e[1] - 3.0).abs() < 1e-5, "{e:?}");
        let q = r.values.last().unwrap();
        assert!((q[2] - h).abs() < 1e-5 && (q[3] - h).abs() < 1e-5, "{q:?}");
        assert!(ch.iter().all(|x| x.joint == 0));
    }

    /// The merged GLB is valid glTF structure: one joint root, every vertex
    /// joint below the joint count.
    #[test]
    fn merged_glb_has_one_root() {
        let a = vec![j("root", None, 0.0), j("spine", Some(0), 1.0)];
        let b = vec![j("root", None, 0.0), j("spine", Some(0), 1.0), j("GM_x", Some(1), 1.2)];
        let (m, _) = merge_skeletons(&[a, b]);
        assert_eq!(m.iter().filter(|j| j.parent.is_none()).count(), 1);
        let glb = crate::gltf::mesh_glb_with("t", &[], &[], &m, serde_json::json!({}), &crate::gltf::GlbOptions { trs_joints: true, ..Default::default() });
        let jl = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        let doc: serde_json::Value = serde_json::from_slice(&glb[20..20 + jl]).unwrap();
        assert_eq!(doc["skins"][0]["skeleton"], 0);
        assert!(doc["nodes"][2]["translation"].is_array());
    }
}
