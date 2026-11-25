//! Cutscenes of the story pack: each scene's cast, props, cameras and clips.
//!
//! Two kinds of files describe a cutscene, both found without names (the
//! archive's table of contents is encrypted, so entries are carved):
//!
//! 1. **Scene clip dictionaries** (`cin@<scene>`, also `cine@<scene>` for
//!    crowd layers). The base dictionary holds one clip per animated object,
//!    named after the object's scene handle: `pack:/lucia.clip`,
//!    `pack:/lucia_face.clip` (facial clip), `pack:/rs_camera_8.clip`,
//!    `pack:/prop_am_drink_soda_03a^2.clip` (`^N` = Nth copy of the same
//!    model). A clip's map key is the joaat of its short name.
//!
//!    A scene clip plays the scene through in sections, one per camera
//!    cut: the clip's +0x68 / +0x70 list `{windows, count, duration}` per
//!    section, and each window names an animation by its map key and the
//!    stretch of it to play ([`anim::Pending`]). The animations are pooled
//!    per skeleton or model (`pack:/cinematic_q8@female@0@60_0.anim`: every
//!    female ped's sections, 60 frames a second, cut into ~16 or ~32 s
//!    pieces) and mostly streamed in dictionaries named `cin@<scene>-<N>`,
//!    which hold those `.anim` objects in their clip map and no clips.
//!    [`load_dictionary`] joins them back: on the base dictionary, the windows
//!    play one after another over the clip's whole duration.
//!
//! 2. **Scene files**: self-describing meta resources (`0x7a`) holding a
//!    small state machine whose state names (struct `dd71c9b4`, member
//!    `name`) are joaat hashes of the scene names, and hash-keyed tables
//!    (inline hash-map buckets of `{Key u32, Item}` entries) of the scene's
//!    objects:
//!    - `64d97283` ped: the key is the joaat of the ped's scene handle
//!      (`lucia`), member `8a944233` (u64) the ped model's 64-bit asset id
//!      ([`crate::meta7a::asset_id`] of the model name, e.g.
//!      `player_story000_01`).
//!    - `1c0668f2` object: member `fbfa718f` points to `8a392c8a`, whose
//!      `modelName` (u64, `0b939623`) is the asset id of a prop or map model.
//!      The keys of these are not scene handles (they name set dressing).
//!    - `5287c842` camera: the key is the joaat of an `rs_camera_*` handle.
//!
//! What is not decoded: the meaning of most other members (times, blend
//! settings, conditions), the state-machine transitions and the event lists.
//! Struct and member names are hashes, except the few resolved ones.
use crate::anim;
use crate::classify::Kind;
use crate::game::ArchiveInfo;
use crate::meta7a::{asset_id, joaat, Meta};
use crate::resource::Resource;
use crate::scan::{ArchiveIndex, Entry};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// The archive the story cutscenes are in.
pub const STORY_ARCHIVE: &str = "pack/story000/story000.rpf";

/// A scene name's mission and its place in that mission, from the name:
/// `<arc>_<mission>_<stage>_…`, where the stage is the intro (`int`,
/// `int1a`, `intro`), a mid scene (`mcs1`, `mcs2` …) or the outro (`ext`).
/// Returns (mission, stage rank 0 intro / 1 mid / 2 outro, stage number).
/// Names with no stage word count as mid scenes of their first two words.
pub fn mission_stage(name: &str) -> (String, u8, u64) {
    let t: Vec<&str> = name.split('_').collect();
    let num = |s: &str| s.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u64>().unwrap_or(0);
    let stage = |w: &str| -> Option<(u8, u64)> {
        let (head, tail) = w.split_at(w.find(|c: char| c.is_ascii_digit()).unwrap_or(w.len()));
        let tail_ok = tail.is_empty() || tail.trim_start_matches(|c: char| c.is_ascii_digit()).chars().all(|c| c.is_ascii_lowercase());
        match head {
            "intro" if tail.is_empty() => Some((0, 0)),
            "int" if tail_ok => Some((0, num(w))),
            "mcs" | "rs" if !tail.is_empty() && tail_ok => Some((1, num(w))),
            "ext" if tail_ok => Some((2, num(w))),
            _ => None,
        }
    };
    for i in 2..t.len().min(5) {
        if let Some((r, n)) = stage(t[i]) {
            return (t[..i].join("_"), r, n);
        }
    }
    (t[..t.len().min(2)].join("_"), 1, 0)
}

/// The mission ids of scene names (see [`mission_stage`]), sorted, once each.
pub fn mission_ids<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut v: Vec<String> = names.into_iter().map(|n| mission_stage(&n.to_ascii_lowercase()).0).collect();
    v.sort();
    v.dedup();
    v
}
/// Bumped when the cached catalog's layout or contents change.
pub const CATALOG_VERSION: u32 = 4;

/// Struct and member hashes of the scene files.
pub mod hashes {
    /// State of the scene state machine; `name` = joaat of the scene name.
    pub const STATE: u32 = 0xdd71_c9b4;
    pub const PED: u32 = 0x64d9_7283;
    /// u64 asset id of the ped model.
    pub const PED_MODEL: u32 = 0x8a94_4233;
    pub const OBJECT: u32 = 0x1c06_68f2;
    /// Pointer to the object's model record (`MODEL_REC`).
    pub const OBJECT_MODEL: u32 = 0xfbfa_718f;
    pub const MODEL_REC: u32 = 0x8a39_2c8a;
    /// `modelName`: u64 asset id.
    pub const MODEL_NAME: u32 = 0x0b93_9623;
    pub const CAMERA: u32 = 0x5287_c842;
    /// Hash-map entry members: `Key` (u32) and `Item` (embedded or pointer).
    pub const ENTRY_KEY: u32 = 0x6098_a50e;
    pub const ENTRY_ITEM: u32 = 0x063f_a3f2;
}

/// A scene clip dictionary name split up: `cin@air_beach_ext_p1_t05_x1-3`
/// -> scene `air_beach_ext_p1_t05_x1`, section 3. `cine@cine@<x>` (crowd
/// layers) keeps the second prefix in the scene name.
pub fn parse_dict_name(n: &str) -> Option<(&str, Option<u32>)> {
    let rest = n.strip_prefix("cin@").or_else(|| n.strip_prefix("cine@"))?;
    if rest.is_empty() {
        return None;
    }
    if let Some((base, sec)) = rest.rsplit_once('-') {
        if !base.is_empty() && !sec.is_empty() && sec.bytes().all(|c| c.is_ascii_digit()) {
            return Some((base, sec.parse().ok()));
        }
    }
    Some((rest, None))
}

/// The scene a variant belongs to: `<scene>_x<N>` -> `<scene>`.
pub fn parent_scene(scene: &str) -> &str {
    match scene.rsplit_once("_x") {
        Some((b, n)) if !b.is_empty() && !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()) => b,
        _ => scene,
    }
}

/// An object's scene handle from a clip short name: (handle, copy, is face clip).
/// `prop_am_drink_soda_03a^2` -> (`prop_am_drink_soda_03a`, 2, false),
/// `lucia_face` -> (`lucia`, 0, true).
pub fn split_clip_name(n: &str) -> (&str, u32, bool) {
    let (n, copy) = match n.rsplit_once('^') {
        Some((b, c)) if c.bytes().all(|x| x.is_ascii_digit()) && !c.is_empty() => (b, c.parse().unwrap_or(0)),
        _ => (n, 0),
    };
    match n.strip_suffix("_face") {
        Some(b) if !b.is_empty() => (b, copy, true),
        _ => (n, copy, false),
    }
}

/// One base clip dictionary of a scene.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneDict {
    /// Dictionary name (`cin@<scene>`).
    pub name: String,
    /// Index entry id.
    pub id: String,
    /// Clip short names, as stored (`lucia`, `lucia_face`, `prop_x^2`).
    pub clips: Vec<String>,
    /// Number of `-N` section dictionaries found for this scene.
    pub sections: usize,
    /// Index entry ids of those section dictionaries.
    #[serde(default)]
    pub section_ids: Vec<String>,
    /// How many of them are named after a longer scene name that has no
    /// base dictionary (`<scene>fixme-<N>`), listed last.
    #[serde(default)]
    pub adopted_sections: usize,
    /// Clips with at least one window streamed in the section dictionaries.
    #[serde(default)]
    pub streamed: Vec<String>,
    /// Clips with windows whose animation is in none of the section
    /// dictionaries (they play with gaps, or not at all).
    #[serde(default)]
    pub unjoined: Vec<String>,
    /// Streamed windows of all clips, and how many of them have no
    /// animation in the section dictionaries.
    #[serde(default)]
    pub windows_streamed: usize,
    #[serde(default)]
    pub windows_missing: usize,
}

/// An object of a scene file's hash-keyed tables.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileObject {
    pub key: u32,
    /// "ped", "object" or "camera".
    pub kind: String,
    /// 64-bit asset id of the model (peds and objects).
    pub model: Option<u64>,
}

/// One scene file (meta resource).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneFile {
    pub id: String,
    /// joaat of the scene names (the state names).
    pub states: Vec<u32>,
    pub objects: Vec<FileObject>,
}

/// Everything read from one archive, cached as JSON in the index folder.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub version: u32,
    pub archive: String,
    pub dicts: Vec<SceneDict>,
    pub files: Vec<SceneFile>,
}

/// Cache file of an archive's catalog.
pub fn cache_path(index_dir: &Path, archive_id: &str) -> std::path::PathBuf {
    let stem: String = archive_id.trim_end_matches(".rpf").chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    index_dir.join(format!("{stem}.cutscenes.json"))
}

/// Every `(key, item struct, item offset)` of a meta resource's hash maps:
/// entry structs (a `Key` u32 and an `Item`) stored in blocks of their own
/// or inline in arrays (`0x50`) of other structs (the hash-map buckets).
/// Empty slots (key 0) are skipped.
pub fn map_entries(m: &Meta) -> Vec<(u32, u32, usize)> {
    use hashes::*;
    // entry struct -> (key offset, item member type, item offset, item struct)
    let mut ent: HashMap<u32, (usize, u8, usize, u32)> = HashMap::new();
    for (h, sd) in &m.structs {
        let k = sd.members.iter().find(|x| x.hash == ENTRY_KEY && x.ty == 0x4a);
        let i = sd.members.iter().find(|x| x.hash == ENTRY_ITEM && (x.ty == 0x05 || x.ty == 0x07));
        if let (Some(k), Some(i)) = (k, i) {
            ent.insert(*h, (k.off as usize, i.ty, i.off as usize, i.extra));
        }
    }
    let mut out = Vec::new();
    let mut take = |o: usize, e: &(usize, u8, usize, u32)| {
        let key = m.r.u32(o + e.0);
        if key == 0 {
            return;
        }
        let item = if e.1 == 0x05 {
            Some((e.3, o + e.2))
        } else {
            m.ptr(m.r.u64(o + e.2)).map(|(h, p)| (if h != 0 { h } else { e.3 }, p))
        };
        if let Some((h, p)) = item {
            out.push((key, h, p));
        }
    };
    for (h, e) in &ent {
        for o in m.objects(*h) {
            take(o, e);
        }
    }
    // inline arrays of entries
    for (sh, sd) in &m.structs {
        for a in sd.members.iter().filter(|x| x.ty == 0x50) {
            let Some(el) = sd.members.get(a.reference as usize) else { continue };
            if el.ty != 0x05 {
                continue;
            }
            let Some(e) = ent.get(&el.extra) else { continue };
            let esz = m.structs.get(&el.extra).map_or(0, |s| s.size as usize);
            if esz == 0 {
                continue;
            }
            for o in m.objects(*sh) {
                for k in 0..a.extra as usize {
                    take(o + a.off as usize + k * esz, e);
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn member_off(m: &Meta, s: u32, h: u32) -> Option<(usize, u8)> {
    m.structs.get(&s)?.members.iter().find(|x| x.hash == h).map(|x| (x.off as usize, x.ty))
}

/// Read a scene file: its state names and its ped, object and camera
/// entries. None when the resource has no state machine states or none of
/// the three object kinds.
pub fn scene_file(id: &str, m: &Meta) -> Option<SceneFile> {
    use hashes::*;
    let name = member_off(m, STATE, joaat("name"))?;
    let mut states: Vec<u32> = m.objects(STATE).into_iter().map(|o| m.r.u32(o + name.0)).filter(|&h| h != 0).collect();
    states.sort_unstable();
    states.dedup();
    if states.is_empty() || ![PED, OBJECT, CAMERA].iter().any(|s| m.structs.contains_key(s)) {
        return None;
    }
    let ped_model = member_off(m, PED, PED_MODEL);
    let obj_model = member_off(m, OBJECT, OBJECT_MODEL);
    let rec_name = member_off(m, MODEL_REC, MODEL_NAME);
    let mut objects = Vec::new();
    for (key, h, p) in map_entries(m) {
        let (kind, model) = match h {
            PED => ("ped", ped_model.map(|(o, _)| m.r.u64(p + o))),
            OBJECT => {
                let model = obj_model.and_then(|(o, ty)| {
                    let rec = if ty == 0x05 { Some(p + o) } else { m.ptr(m.r.u64(p + o)).map(|x| x.1) }?;
                    rec_name.map(|(n, _)| m.r.u64(rec + n))
                });
                ("object", model)
            }
            CAMERA => ("camera", None),
            _ => continue,
        };
        objects.push(FileObject { key, kind: kind.into(), model: model.filter(|&x| x != 0) });
    }
    objects.sort_by_key(|o| (o.key, o.kind.clone()));
    objects.dedup();
    Some(SceneFile { id: id.to_string(), states, objects })
}

/// Scan one archive: every scene clip dictionary (unnamed objects whose
/// dictionary name starts with `cin@` / `cine@`) and every scene file (meta
/// resources with a scene state machine). Reads the whole archive once
/// (the story pack: several minutes); see [`catalog`] for the cached form.
pub fn scan(a: &ArchiveInfo, ix: &ArchiveIndex, progress: crate::export::Progress) -> Result<Catalog> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    enum Hit {
        /// name, id, clip names, (clip, pending animation keys)
        Dict(String, String, Vec<String>, Vec<(String, Vec<u32>)>),
        /// scene, id, animation keys
        Section(String, String, Vec<u32>),
        File(SceneFile),
    }
    let todo: Vec<&Entry> = ix
        .entries
        .iter()
        // every resource object, named ones too: a recovered name can be
        // wrong (one section dictionary carries the name `jj.xl`)
        .filter(|e| !e.streams.is_empty() && (e.kind == Kind::RageObj || (e.kind == Kind::Resource && e.sub == "7a")))
        .collect();
    let total = todo.len() as u64;
    let done = AtomicU64::new(0);
    let hits: Vec<Hit> = todo
        .par_iter()
        .filter_map(|e| {
            progress(&e.id, done.fetch_add(1, Ordering::Relaxed) + 1, total);
            if e.kind == Kind::Resource {
                let r = Resource::load_meta(&a.path, e).ok()?;
                let m = Meta::parse(&r)?;
                return scene_file(&e.id, &m).map(Hit::File);
            }
            let r = Resource::load_sys(&a.path, e).ok()?;
            let name = anim::dictionary_name(&r.sys)?;
            let (scene, section) = parse_dict_name(&name)?;
            if section.is_some() {
                return Some(Hit::Section(scene.to_string(), e.id.clone(), anim::map_keys(&r.sys)));
            }
            let d = anim::ClipDictionary::parse(&anim::dictionary_bytes(&a.path, e).ok()?).ok()?;
            let clips = d.clips.iter().map(|c| c.short_name().to_string()).collect();
            let pending = d.clips.iter().filter(|c| !c.pending.is_empty()).map(|c| (c.short_name().to_string(), c.pending.iter().map(|p| p.animation).collect())).collect();
            Some(Hit::Dict(name, e.id.clone(), clips, pending))
        })
        .collect();
    let mut sections = Sections::new();
    let mut dicts = Vec::new();
    let mut pendings = Vec::new();
    let mut files = Vec::new();
    for h in hits {
        match h {
            Hit::Dict(name, id, clips, pending) => {
                dicts.push(SceneDict { name, id, clips, sections: 0, section_ids: Vec::new(), adopted_sections: 0, streamed: Vec::new(), unjoined: Vec::new(), windows_streamed: 0, windows_missing: 0 });
                pendings.push(pending);
            }
            Hit::Section(s, id, keys) => sections.entry(s).or_default().push((id, keys)),
            Hit::File(f) => files.push(f),
        }
    }
    let bases: HashSet<String> = dicts.iter().filter_map(|d| parse_dict_name(&d.name)).map(|x| x.0.to_string()).collect();
    let adopted = adopt_sections(&bases, &mut sections);
    for (d, pending) in dicts.iter_mut().zip(pendings) {
        let s = parse_dict_name(&d.name).map(|x| x.0).unwrap_or_default();
        let mut secs: Vec<(String, Vec<u32>)> = sections.get(s).cloned().unwrap_or_default();
        secs.sort();
        let extra = adopted.get(s).cloned().unwrap_or_default();
        d.adopted_sections = extra.len();
        secs.extend(extra);
        d.sections = secs.len();
        d.section_ids = secs.iter().map(|x| x.0.clone()).collect();
        let have: HashSet<u32> = secs.iter().flat_map(|x| x.1.iter().copied()).collect();
        for (clip, keys) in pending {
            let missing = keys.iter().filter(|k| !have.contains(k)).count();
            d.windows_streamed += keys.len();
            d.windows_missing += missing;
            if missing > 0 {
                d.unjoined.push(clip.clone());
            }
            d.streamed.push(clip);
        }
        d.streamed.sort();
        d.unjoined.sort();
    }
    // keep only scene files that name at least one of the scenes
    let scene_hashes: HashSet<u32> = dicts.iter().filter_map(|d| parse_dict_name(&d.name)).flat_map(|(s, _)| [joaat(s), joaat(parent_scene(s))]).collect();
    files.retain(|f| f.states.iter().any(|h| scene_hashes.contains(h)));
    dicts.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    files.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Catalog { version: CATALOG_VERSION, archive: a.id.clone(), dicts, files })
}

/// Section dictionaries grouped by scene name: (entry id, animation keys).
type Sections = HashMap<String, Vec<(String, Vec<u32>)>>;

/// Section groups with no base dictionary of their own belong to the scene
/// whose name is the longest prefix of theirs (`cin@mtl_mont_int_p0l_t01fixme-N`
/// for `cin@mtl_mont_int_p0l_t01`). Moves them out of `sections` and returns
/// them by that scene; the scene's own sections are joined first and win on a
/// shared key.
fn adopt_sections(bases: &HashSet<String>, sections: &mut Sections) -> Sections {
    let mut adopted = Sections::new();
    let mut orphans: Vec<String> = sections.keys().filter(|k| !bases.contains(*k)).cloned().collect();
    orphans.sort();
    for o in orphans {
        if let Some(b) = bases.iter().filter(|b| o.starts_with(b.as_str())).max_by_key(|b| b.len()) {
            let mut v = sections.remove(&o).unwrap_or_default();
            v.sort();
            adopted.entry(b.clone()).or_default().extend(v);
        }
    }
    adopted
}

/// The catalog of an archive, from the cache in the index folder or
/// scanned (and cached) when missing or out of date.
pub fn catalog(ws: &crate::export::Workspace, archive: &str, rescan: bool, progress: crate::export::Progress) -> Result<Catalog> {
    let a = ws.resolve(archive)?;
    let cache = cache_path(&ws.index_dir(), &a.id);
    if !rescan {
        if let Ok(b) = std::fs::read(&cache) {
            if let Ok(c) = serde_json::from_slice::<Catalog>(&b) {
                if c.version == CATALOG_VERSION {
                    return Ok(c);
                }
            }
        }
    }
    let ix = ws.load_index(&a)?;
    let c = scan(&a, &ix, progress)?;
    crate::atomic::write(&cache, serde_json::to_vec(&c)?)?;
    Ok(c)
}

/// A scene's base clip dictionary with its section dictionaries joined in
/// ([`anim::ClipDictionary::join`]), so every clip plays the whole scene.
pub fn load_dictionary(ws: &crate::export::Workspace, c: &Catalog, d: &SceneDict) -> Result<anim::ClipDictionary> {
    use rayon::prelude::*;
    let a = ws.resolve(&c.archive)?;
    let ix = ws.load_index(&a)?;
    let ids: Vec<&String> = std::iter::once(&d.id).chain(&d.section_ids).collect();
    let mut parsed: Vec<anim::ClipDictionary> = ids
        .par_iter()
        .map(|id| {
            let e = ix.entries.iter().find(|e| &e.id == *id).ok_or_else(|| anyhow!("{id} is not in the {} index (re-run `sixng cutscenes --rescan`)", a.id))?;
            anim::ClipDictionary::parse(&anim::dictionary_bytes(&a.path, e)?)
        })
        .collect::<Result<_>>()?;
    let sections = parsed.split_off(1);
    let mut base = parsed.pop().unwrap();
    base.join(sections);
    Ok(base)
}

/// Names to resolve 64-bit asset ids and to tell models from other handles.
#[derive(Default)]
pub struct Models {
    /// asset id -> model name
    pub by_id: HashMap<u64, String>,
    /// lower-case model names
    pub names: HashSet<String>,
    /// models `outfit` can assemble (character prefixes)
    pub characters: HashSet<String>,
}

impl Models {
    /// Add model names (resource names: drawables, fragments, ...).
    pub fn add<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        for n in names {
            let n = n.rsplit('/').next().unwrap_or(n);
            let n = n.split('.').next().unwrap_or(n).to_ascii_lowercase();
            if n.is_empty() || self.names.contains(&n) {
                continue;
            }
            self.by_id.insert(asset_id(&n), n.clone());
            self.names.insert(n);
        }
    }
    pub fn add_characters<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        let v: Vec<String> = names.into_iter().map(|n| n.to_ascii_lowercase()).collect();
        self.add(v.iter().map(|s| s.as_str()));
        self.characters.extend(v);
    }
    /// Models from the recovered names of an archive index.
    pub fn add_index(&mut self, ix: &ArchiveIndex) {
        self.add(ix.entries.iter().filter_map(|e| e.name.as_deref()));
    }
    /// Names of the work folder's name dictionary (`sixng names`): model
    /// names that are no resource name (generic ped bodies such as
    /// `zz_male_187cm`). Used only to resolve asset ids, not as props.
    pub fn add_name_dict(&mut self, work: &Path) {
        let Ok(b) = std::fs::read(crate::names::dict_path(work)) else { return };
        let Ok(d) = serde_json::from_slice::<crate::names::Dict>(&b) else { return };
        for (n, _) in d.names.values().flatten() {
            let n = n.to_ascii_lowercase();
            self.by_id.entry(asset_id(&n)).or_insert(n);
        }
    }
    pub fn name(&self, id: u64) -> Option<&str> {
        self.by_id.get(&id).map(|s| s.as_str())
    }
}

/// One animated object of a scene.
#[derive(Clone, Debug, Serialize)]
pub struct SceneObject {
    /// Scene handle (`lucia`, `prop_am_drink_soda_03a`, `rs_camera_8`).
    pub handle: String,
    /// "character" (a model `outfit` assembles), "ped" (a ped model that is
    /// not one), "prop" (handle names a model), "camera" or "unknown".
    pub kind: &'static str,
    /// Model name when resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 64-bit model asset id from the scene file (hex), when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Where the model id came from: "scene file" (this scene's own) or
    /// "other scenes" (the model this handle has most often elsewhere).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_source: Option<&'static str>,
    /// Body / object clips (short names; several for `^N` copies).
    pub clips: Vec<String>,
    /// Facial clips.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub face_clips: Vec<String>,
    /// True when the body clips' animation is streamed in the section
    /// dictionaries (joined by [`load_dictionary`]).
    pub streamed: bool,
}

/// A cutscene: one base clip dictionary with its objects.
#[derive(Clone, Debug, Serialize)]
pub struct Scene {
    pub name: String,
    /// The scene this one is a variant (`_xN`) of, if it is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant_of: Option<String>,
    pub dictionary: String,
    pub dictionary_id: String,
    pub sections: usize,
    /// Clip windows streamed in the section dictionaries, and those whose
    /// animation none of them holds.
    pub windows_streamed: usize,
    pub windows_missing: usize,
    /// Clips with missing windows.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unjoined: Vec<String>,
    /// Scene files naming this scene (entry ids).
    pub scene_files: Vec<String>,
    pub cast: Vec<SceneObject>,
    pub props: Vec<SceneObject>,
    pub cameras: Vec<String>,
    pub other: Vec<SceneObject>,
    /// Models of the scene files' object records (set dressing the scene
    /// spawns or controls): resolved names, then the ids that did not resolve.
    pub set_models: Vec<String>,
    pub set_models_unresolved: Vec<String>,
}

/// Build the scene list of a catalog.
pub fn scenes(c: &Catalog, models: &Models) -> Vec<Scene> {
    // state hash -> scene files
    let mut by_state: HashMap<u32, Vec<&SceneFile>> = HashMap::new();
    for f in &c.files {
        for &s in &f.states {
            by_state.entry(s).or_default().push(f);
        }
    }
    // the model a ped handle has most often across all scene files: the
    // fallback when the scene's own files have no record of it (ties: the
    // lower id, so the result does not depend on file order)
    let mut seen: HashMap<u32, HashMap<u64, usize>> = HashMap::new();
    for f in &c.files {
        for o in f.objects.iter().filter(|o| o.kind == "ped") {
            if let Some(m) = o.model {
                *seen.entry(o.key).or_default().entry(m).or_default() += 1;
            }
        }
    }
    let global: HashMap<u32, u64> = seen.into_iter().filter_map(|(k, v)| v.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|(m, _)| (k, m))).collect();
    // handles with a facial clip in any scene are peds everywhere
    let face_handles: HashSet<&str> = c.dicts.iter().flat_map(|d| &d.clips).map(|cl| split_clip_name(cl)).filter(|x| x.2).map(|x| x.0).collect();
    // some handles are model names themselves (generic bodies such as
    // `zz_male_182cm`): candidates for the ids, not props
    let mut by_id: HashMap<u64, String> = HashMap::new();
    for d in &c.dicts {
        for cl in &d.clips {
            let h = split_clip_name(cl).0.to_ascii_lowercase();
            by_id.entry(asset_id(&h)).or_insert(h);
        }
    }
    let model_name = |id: u64| models.name(id).map(|s| s.to_string()).or_else(|| by_id.get(&id).cloned());
    let mut out = Vec::new();
    for d in &c.dicts {
        let Some((scene, _)) = parse_dict_name(&d.name) else { continue };
        let parent = parent_scene(scene);
        let mut files: Vec<&SceneFile> = by_state.get(&joaat(scene)).cloned().unwrap_or_default();
        if files.is_empty() && parent != scene {
            files = by_state.get(&joaat(parent)).cloned().unwrap_or_default();
        }
        let mut peds: HashMap<u32, u64> = HashMap::new();
        let mut cams: HashSet<u32> = HashSet::new();
        let mut set: BTreeMap<u64, ()> = BTreeMap::new();
        for f in &files {
            for o in &f.objects {
                match (o.kind.as_str(), o.model) {
                    ("ped", Some(m)) => {
                        peds.insert(o.key, m);
                    }
                    ("camera", _) => {
                        cams.insert(o.key);
                    }
                    ("object", Some(m)) => {
                        set.insert(m, ());
                    }
                    _ => {}
                }
            }
        }
        // group clips by handle, in clip order
        let mut order: Vec<String> = Vec::new();
        let mut groups: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();
        for cl in &d.clips {
            let (h, _, face) = split_clip_name(cl);
            let g = groups.entry(h.to_string()).or_insert_with(|| {
                order.push(h.to_string());
                Default::default()
            });
            if face { &mut g.1 } else { &mut g.0 }.push(cl.clone());
        }
        order.sort();
        let (mut cast, mut props, mut cameras, mut other) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for h in order {
            let (clips, face_clips) = groups.remove(&h).unwrap_or_default();
            let hh = joaat(&h);
            if h.starts_with("rs_camera") || cams.contains(&hh) {
                cameras.push(h);
                continue;
            }
            let own = peds.get(&hh).copied();
            let ped = own.or_else(|| global.get(&hh).copied());
            let model_source = ped.map(|_| if own.is_some() { "scene file" } else { "other scenes" });
            let lower = h.to_ascii_lowercase();
            let (kind, model) = if let Some(id) = ped {
                let name = model_name(id);
                let kind = if name.as_ref().is_some_and(|n| models.characters.contains(n)) { "character" } else { "ped" };
                (kind, name)
            } else if models.characters.contains(&lower) {
                ("character", Some(lower.clone()))
            } else if models.names.contains(&lower) {
                ("prop", Some(lower.clone()))
            } else if lower.starts_with("ib_") {
                // in-hand items (phone, weapons, drinks): no model of that name
                ("prop", None)
            } else if !face_clips.is_empty() || face_handles.contains(h.as_str()) {
                // a facial clip (here or in another scene) means a ped, even
                // without a model record
                ("ped", None)
            } else {
                ("unknown", None)
            };
            let streamed = !clips.is_empty() && clips.iter().all(|cl| d.streamed.contains(cl));
            let o = SceneObject { handle: h, kind, model, model_id: ped.map(|x| format!("{x:016x}")), model_source, clips, face_clips, streamed };
            match kind {
                "character" | "ped" => cast.push(o),
                "prop" => props.push(o),
                _ => other.push(o),
            }
        }
        let (mut set_models, mut set_unres) = (Vec::new(), Vec::new());
        for m in set.keys() {
            match models.name(*m) {
                Some(n) => set_models.push(n.to_string()),
                None => set_unres.push(format!("{m:016x}")),
            }
        }
        set_models.sort();
        set_models.dedup();
        out.push(Scene {
            name: scene.to_string(),
            variant_of: (parent != scene).then(|| parent.to_string()),
            dictionary: d.name.clone(),
            dictionary_id: d.id.clone(),
            sections: d.sections,
            windows_streamed: d.windows_streamed,
            windows_missing: d.windows_missing,
            unjoined: d.unjoined.clone(),
            scene_files: files.iter().map(|f| f.id.clone()).collect(),
            cast,
            props,
            cameras,
            other,
            set_models,
            set_models_unresolved: set_unres,
        });
    }
    out
}

/// The dictionary entry id of a scene clip dictionary by name (`cin@<scene>`),
/// from a catalog.
pub fn dictionary_id<'a>(c: &'a Catalog, name: &str) -> Option<&'a str> {
    c.dicts.iter().find(|d| d.name.eq_ignore_ascii_case(name)).map(|d| d.id.as_str())
}

/// The body clips one model plays in a scene: (dictionary, clip short name)
/// for every object of the scene whose ped record names `model` (by asset
/// id) or whose handle is `model`. `scene` is the scene name, with or
/// without `cin@`. Errors list the scene's handles when nothing matches.
pub fn model_clips<'a>(c: &'a Catalog, scene: &str, model: &str) -> Result<Vec<(&'a SceneDict, String)>> {
    let q = scene.strip_prefix("cin@").unwrap_or(scene);
    let d = c
        .dicts
        .iter()
        .find(|d| parse_dict_name(&d.name).is_some_and(|(s, _)| s.eq_ignore_ascii_case(q)))
        .ok_or_else(|| anyhow!("no cutscene named {q:?} in {} (`sixng cutscenes --name <regex>` lists them)", c.archive))?;
    let (s, _) = parse_dict_name(&d.name).unwrap_or_default();
    let want = asset_id(&model.to_ascii_lowercase());
    let mut keys: HashSet<u32> = HashSet::new();
    for st in [joaat(s), joaat(parent_scene(s))] {
        for f in c.files.iter().filter(|f| f.states.contains(&st)) {
            keys.extend(f.objects.iter().filter(|o| o.kind == "ped" && o.model == Some(want)).map(|o| o.key));
        }
        if !keys.is_empty() {
            break;
        }
    }
    if keys.is_empty() {
        // no record in the scene's own files: handles that are this model elsewhere
        keys.extend(c.files.iter().flat_map(|f| &f.objects).filter(|o| o.kind == "ped" && o.model == Some(want)).map(|o| o.key));
    }
    let picks: Vec<(&SceneDict, String)> = d
        .clips
        .iter()
        .filter(|cl| {
            let (h, _, face) = split_clip_name(cl);
            !face && (keys.contains(&joaat(h)) || h.eq_ignore_ascii_case(model))
        })
        .map(|cl| (d, cl.clone()))
        .collect();
    if picks.is_empty() {
        let mut hs: Vec<&str> = d.clips.iter().map(|cl| split_clip_name(cl)).filter(|x| !x.2).map(|x| x.0).collect();
        hs.sort();
        hs.dedup();
        anyhow::bail!("{model} is not in {} (its objects: {}); pick one with --clip {}/<clip>", d.name, hs.join(", "), d.name);
    }
    Ok(picks)
}

/// Find a scene by name (`cin@` prefix optional).
pub fn find_scene<'a>(all: &'a [Scene], q: &str) -> Result<&'a Scene> {
    let q = q.strip_prefix("cin@").unwrap_or(q);
    all.iter().find(|s| s.name.eq_ignore_ascii_case(q) || s.dictionary.eq_ignore_ascii_case(q)).ok_or_else(|| anyhow!("no cutscene named {q:?} (`sixng cutscenes --name <regex>` lists them)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_names() {
        assert_eq!(parse_dict_name("cin@air_beach_ext_p1_t05"), Some(("air_beach_ext_p1_t05", None)));
        assert_eq!(parse_dict_name("cin@bil_confr_mcs5_confrontation_p1_t09-4"), Some(("bil_confr_mcs5_confrontation_p1_t09", Some(4))));
        assert_eq!(parse_dict_name("cin@css_skymr_mcs1_plan_p1_0f0a1_t02_x1-12"), Some(("css_skymr_mcs1_plan_p1_0f0a1_t02_x1", Some(12))));
        assert_eq!(parse_dict_name("cine@cine@air_final_int_p5_t06_crowd_peds_01"), Some(("cine@air_final_int_p5_t06_crowd_peds_01", None)));
        assert_eq!(parse_dict_name("ambient@x"), None);
        assert_eq!(parent_scene("css_skymr_mcs1_plan_p1_0f0a1_t02_x1"), "css_skymr_mcs1_plan_p1_0f0a1_t02");
        assert_eq!(parent_scene("air_beach_ext_p1_t05"), "air_beach_ext_p1_t05");
    }

    #[test]
    fn orphan_sections_join_the_longest_prefix_scene() {
        let bases: HashSet<String> = ["mtl_int_p0l_t01", "mtl_int_p0", "other_t01"].map(String::from).into();
        let mut s = Sections::new();
        s.insert("mtl_int_p0l_t01".into(), vec![("a@2".into(), vec![1])]);
        s.insert("mtl_int_p0l_t01fixme".into(), vec![("a@9".into(), vec![2]), ("a@8".into(), vec![3])]);
        s.insert("unrelated".into(), vec![("a@5".into(), vec![4])]);
        let a = adopt_sections(&bases, &mut s);
        assert_eq!(a.len(), 1);
        assert_eq!(a["mtl_int_p0l_t01"].iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), vec!["a@8", "a@9"]);
        // the scene's own sections stay; a group no scene name prefixes is left alone
        assert!(s.contains_key("mtl_int_p0l_t01") && s.contains_key("unrelated") && !s.contains_key("mtl_int_p0l_t01fixme"));
    }

    #[test]
    fn clip_handles() {
        assert_eq!(split_clip_name("lucia"), ("lucia", 0, false));
        assert_eq!(split_clip_name("lucia_face"), ("lucia", 0, true));
        assert_eq!(split_clip_name("prop_am_drink_soda_03a^2"), ("prop_am_drink_soda_03a", 2, false));
        assert_eq!(split_clip_name("rs_camera_8_x1"), ("rs_camera_8_x1", 0, false));
        assert_eq!(split_clip_name("_face"), ("_face", 0, false));
    }

    #[test]
    fn clip_keys_and_model_ids() {
        // a scene dictionary's clip map key is the joaat of the clip's short name
        assert_eq!(joaat("jason_face"), 0xed1c_7cfe);
        assert_eq!(joaat("lucia"), 0x2fe5_af54);
        // a scene file object's modelName is the asset id of the model name
        assert_eq!(asset_id("mint_fridge_icecream_01b"), 0xf184_83aa_e926_b136);
    }

    fn catalog() -> Catalog {
        let d = SceneDict {
            name: "cin@test_scene_p1_t01".into(),
            id: "a.rpf@10".into(),
            clips: ["lucia", "lucia_face", "boss", "boss_face", "prop_cup_01a", "prop_cup_01a^1", "rs_camera_1", "thing"].map(String::from).to_vec(),
            sections: 2,
            section_ids: vec!["a.rpf@11".into(), "a.rpf@12".into()],
            adopted_sections: 0,
            streamed: vec!["boss".into()],
            unjoined: vec![],
            windows_streamed: 2,
            windows_missing: 0,
        };
        let f = SceneFile {
            id: "a.rpf@20".into(),
            states: vec![joaat("test_scene_p1_t01")],
            objects: vec![
                FileObject { key: joaat("lucia"), kind: "ped".into(), model: Some(asset_id("player_story000_01")) },
                FileObject { key: joaat("boss"), kind: "ped".into(), model: Some(asset_id("cs_boss")) },
                FileObject { key: 7, kind: "object".into(), model: Some(asset_id("prop_table_01a")) },
                FileObject { key: 8, kind: "object".into(), model: Some(0x1234) },
            ],
        };
        // a second scene without a scene file of its own
        let d2 = SceneDict { name: "cin@other_scene_p1_t01".into(), id: "a.rpf@30".into(), clips: ["boss", "boss_face", "ib_phone"].map(String::from).to_vec(), sections: 0, section_ids: vec![], adopted_sections: 0, streamed: vec![], unjoined: vec![], windows_streamed: 0, windows_missing: 0 };
        Catalog { version: CATALOG_VERSION, archive: "a.rpf".into(), dicts: vec![d, d2], files: vec![f] }
    }

    #[test]
    fn scene_objects_resolve() {
        let mut m = Models::default();
        m.add_characters(["player_story000_01"]);
        m.add(["cs_boss", "prop_cup_01a", "prop_table_01a"]);
        let all = scenes(&catalog(), &m);
        assert_eq!(all.len(), 2);
        // the second scene: the ped's model from the other scene, the in-hand item a prop
        let o = &all[1];
        assert!(o.scene_files.is_empty());
        assert_eq!((o.cast[0].model.as_deref(), o.cast[0].model_source), (Some("cs_boss"), Some("other scenes")));
        assert_eq!((o.props[0].handle.as_str(), o.props[0].model.as_deref()), ("ib_phone", None));
        let s = &all[0];
        assert_eq!(s.cast[0].model_source, Some("scene file"));
        assert_eq!(s.sections, 2);
        assert_eq!(s.scene_files, vec!["a.rpf@20".to_string()]);
        let lucia = s.cast.iter().find(|o| o.handle == "lucia").unwrap();
        assert_eq!((lucia.kind, lucia.model.as_deref()), ("character", Some("player_story000_01")));
        assert_eq!(lucia.face_clips, vec!["lucia_face".to_string()]);
        let boss = s.cast.iter().find(|o| o.handle == "boss").unwrap();
        assert_eq!((boss.kind, boss.model.as_deref(), boss.streamed), ("ped", Some("cs_boss"), true));
        assert!(!lucia.streamed);
        assert_eq!(s.props.len(), 1);
        assert_eq!(s.props[0].clips, vec!["prop_cup_01a".to_string(), "prop_cup_01a^1".to_string()]);
        assert_eq!(s.cameras, vec!["rs_camera_1".to_string()]);
        assert_eq!(s.other[0].handle, "thing");
        assert_eq!(s.set_models, vec!["prop_table_01a".to_string()]);
        assert_eq!(s.set_models_unresolved, vec!["0000000000001234".to_string()]);
    }

    #[test]
    fn a_models_scene_clips() {
        let c = catalog();
        let p = model_clips(&c, "cin@test_scene_p1_t01", "player_story000_01").unwrap();
        assert_eq!(p.iter().map(|(d, cl)| (d.id.as_str(), cl.as_str())).collect::<Vec<_>>(), vec![("a.rpf@10", "lucia")]);
        // a prop by its handle, every copy
        let p = model_clips(&c, "test_scene_p1_t01", "prop_cup_01a").unwrap();
        assert_eq!(p.len(), 2);
        let e = model_clips(&c, "test_scene_p1_t01", "cs_nobody").unwrap_err().to_string();
        assert!(e.contains("boss, lucia, prop_cup_01a, rs_camera_1, thing"), "{e}");
        assert!(model_clips(&c, "no_such_scene", "x").is_err());
        // a streamed clip is picked too (its sections are joined on load)
        let p = model_clips(&c, "test_scene_p1_t01", "cs_boss").unwrap();
        assert_eq!(p.iter().map(|(_, cl)| cl.as_str()).collect::<Vec<_>>(), vec!["boss"]);
        // no scene file: the handle that is this model in other scenes
        let p = model_clips(&c, "other_scene_p1_t01", "cs_boss").unwrap();
        assert_eq!(p.iter().map(|(d, cl)| (d.id.as_str(), cl.as_str())).collect::<Vec<_>>(), vec![("a.rpf@30", "boss")]);
    }
}
