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
pub const CATALOG_VERSION: u32 = 8;

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
    // The master timeline of a scene file (struct and member names from
    // vihook's par_names.txt, issue #516). The start-time map's items are
    // `START` (`Offset`, f32 seconds) under the same keys as the id map,
    // whose items are references to a scene section or composites that
    // point on to one.
    pub const START: u32 = 0x0f53_4235;
    pub const START_OFFSET: u32 = 0x9132_2c71;
    /// `fwAnimSceneReferenceSectionData`: one section of a scene, `sceneId` (joaat of the scene
    /// name) and `sectionId`.
    pub const REFERENCE: u32 = 0x7f09_852f;
    pub const REFERENCE_SCENE: u32 = 0xfafe_97a5;
    pub const REFERENCE_SECTION: u32 = 0x1a43_c2eb;
    /// `fwAnimSceneCompositeSectionData`: the id of the entry it plays (`9a3e57d2`), else its
    /// `sectionIds` (`379a4935`).
    pub const COMPOSITE: u32 = 0xc8d5_1b9e;
    pub const COMPOSITE_ID: u32 = 0x9a3e_57d2;
    pub const COMPOSITE_SECTIONS: u32 = 0x379a_4935;
    /// The scene's audio: member `52c09a55` holds the audio name, whose
    /// streams are `joaat("<name>.<channel>") & 0x1FFFFFFF`.
    pub const AUDIO: u32 = 0xdb64_6fb2;
    pub const AUDIO_NAME: u32 = 0x52c0_9a55;
    // The master's state machine: its states (`STATE`) are keys of the id
    // map, whose item (`STATE_ITEM`) lists the start-map keys the state
    // plays (`STATE_STARTS`).
    pub const STATE_ITEM: u32 = 0xa90b_7f7e;
    pub const STATE_STARTS: u32 = 0x32c6_9aae;
    /// `transitions` of a state: an array of `rage__fwDataFsmStateTransition`.
    pub const STATE_TRANSITIONS: u32 = 0xb0fc_1c4f;
    pub const TRANSITION: u32 = 0x891b_2a0f;
    /// `toState`: the state the transition leads to.
    pub const TRANSITION_TO: u32 = 0x2f6e_2b9a;
    /// `conditionSet`.
    pub const TRANSITION_CONDITIONS: u32 = 0x60aa_e83b;
    /// A condition naming a state (`f82a367d`): nearly always the
    /// transition's own source state, so it reads "when this state ends".
    pub const CONDITION_STATE: u32 = 0x5743_9252;
    pub const CONDITION_STATE_ID: u32 = 0xf82a_367d;
    /// A condition on an entity's value (`entityName`, `value`): a branch.
    pub const CONDITION_ENTITY: u32 = 0xd5f7_c394;
    /// `fwAnimSceneSubtitleEventData`: a subtitle line of a scene, with
    /// `TextLabelName` (the text label), `Speaker` (joaat of the speaker's
    /// character name), `startTime` and `endTime` (seconds into the scene).
    pub const SUBTITLE: u32 = 0x2e33_8a28;
    pub const SUBTITLE_LABEL: u32 = 0x14ff_380c;
    pub const SUBTITLE_SPEAKER: u32 = 0x97bf_dfda;
    pub const EVENT_START: u32 = 0x929d_b78d;
    pub const EVENT_END: u32 = 0x2b30_c60b;
    /// `fwAnimSceneSectionData`: one layer of a composite section, holding
    /// an event list (`fwAnimSceneEventListData`, member `events`: pointers
    /// to the events). The sections a composite lists play together from
    /// the composite's start, so their event times count from there.
    pub const SECTION: u32 = 0x64e8_ac67;
    pub const SECTION_EVENTS: u32 = 0xdb01_2f3b;
    pub const EVENT_LIST: u32 = 0xeed7_e46f;
    pub const EVENT_LIST_EVENTS: u32 = 0xdb01_2f3b;
}

/// A subtitle event of a scene file.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SubtitleEvent {
    /// Text label (`TextLabelName`); its joaat is the text-table key.
    pub label: String,
    /// `Speaker`: joaat of the speaker's character name (0 for none).
    pub speaker: u32,
    pub start: f32,
    pub end: f32,
    /// Id-map keys the event plays under: its section and the composite
    /// sections that list that section. A master slot's `sectionId` names
    /// one of them, and the event's times count from that slot's start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<u32>,
}

/// The objects a section's event list points to: (struct hash, offset).
fn section_events(m: &Meta, sec: usize) -> Vec<(u32, usize)> {
    use hashes::*;
    let Some((list_off, _)) = member_off(m, SECTION, SECTION_EVENTS) else { return Vec::new() };
    let Some(arr) = m.structs.get(&EVENT_LIST).and_then(|sd| sd.members.iter().find(|x| x.hash == EVENT_LIST_EVENTS && x.ty == 0x5c)).map(|x| x.off as usize) else { return Vec::new() };
    let a = sec + list_off + arr;
    let n = m.r.u32(a) as usize;
    let Some((_, base)) = m.ptr(m.r.u64(a + 8)).filter(|_| n < 100_000) else { return Vec::new() };
    (0..n).filter_map(|i| m.ptr(m.r.u64(base + 8 * i))).collect()
}

/// Event offset -> the id-map keys it plays under: the section whose event
/// list holds it and every composite that lists that section.
fn event_sections(m: &Meta) -> HashMap<usize, Vec<u32>> {
    use hashes::*;
    let entries = map_entries(m);
    let mut by_section: HashMap<u32, Vec<usize>> = HashMap::new();
    for &(key, h, p) in &entries {
        if h == SECTION {
            by_section.entry(key).or_insert_with(|| section_events(m, p).into_iter().map(|x| x.1).collect());
        }
    }
    let mut out: HashMap<usize, Vec<u32>> = HashMap::new();
    for (key, evs) in &by_section {
        for &o in evs {
            out.entry(o).or_default().push(*key);
        }
    }
    for &(key, h, p) in &entries {
        if h != COMPOSITE {
            continue;
        }
        let Some(serde_json::Value::Array(a)) = m.member_value(h, p, COMPOSITE_SECTIONS) else { continue };
        for sec in a.iter().filter_map(|v| v.as_u64()).map(|x| x as u32) {
            for &o in by_section.get(&sec).into_iter().flatten() {
                out.entry(o).or_default().push(key);
            }
        }
    }
    for v in out.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    out
}

/// The subtitle events of a scene file, by start time, each with the
/// sections it plays under ([`SubtitleEvent::sections`]). Events without a
/// label are left out.
pub fn subtitle_events(m: &Meta) -> Vec<SubtitleEvent> {
    use hashes::*;
    let (Some(sp), Some(st), Some(en)) = (member_off(m, SUBTITLE, SUBTITLE_SPEAKER), member_off(m, SUBTITLE, EVENT_START), member_off(m, SUBTITLE, EVENT_END)) else { return Vec::new() };
    let secs = event_sections(m);
    let mut out: Vec<SubtitleEvent> = m
        .objects(SUBTITLE)
        .into_iter()
        .filter_map(|o| {
            let label = m.member_value(SUBTITLE, o, SUBTITLE_LABEL)?.as_str().filter(|x| !x.is_empty())?.to_string();
            Some(SubtitleEvent { label, speaker: m.r.u32(o + sp.0), start: m.r.f32(o + st.0), end: m.r.f32(o + en.0), sections: secs.get(&o).cloned().unwrap_or_default() })
        })
        .collect();
    out.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.label.cmp(&b.label)));
    out.dedup();
    out
}

/// One slot of a master timeline: a scene section that starts `start`
/// seconds into the master.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Slot {
    /// The start-map key (the id the master's states list).
    #[serde(default)]
    pub key: u32,
    /// Other start-map keys with the same slot (a file can hold the start
    /// map twice, under other keys).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<u32>,
    pub start: f32,
    /// joaat of the scene name (`sceneId`).
    pub scene: u32,
    /// `sectionId`.
    pub section: u32,
}

/// Channels of a cutscene audio bank, in the order reports list them.
pub const AUDIO_CHANNELS: [&str; 8] = ["c", "l", "r", "ad", "ls", "rs", "music.l", "music.r"];

/// Stream id of one channel of a cutscene's audio: the low 29 bits of
/// `joaat("<name>.<channel>")` (lower case).
pub fn audio_stream_id(name: &str, channel: &str) -> u32 {
    joaat(&format!("{name}.{channel}")) & 0x1FFF_FFFF
}

/// A scene name without its part, take and variant suffixes
/// (`_p1`, `_p0l`, `_t05`, `_x1`): the name its audio bank is hashed from.
pub fn audio_base_name(scene: &str) -> &str {
    let mut s = scene;
    for (lead, rest_ok) in [('x', false), ('t', false), ('p', true)] {
        if let Some((head, last)) = s.rsplit_once('_') {
            let mut b = last.bytes();
            let ok = b.next() == Some(lead as u8) && {
                let tail = &last[1..];
                let digits = tail.bytes().take_while(|c| c.is_ascii_digit()).count();
                digits > 0 && (if rest_ok { tail[digits..].bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) } else { digits == tail.len() })
            };
            if ok && !head.is_empty() {
                s = head;
            }
        }
    }
    s
}

/// One channel of a cutscene audio bank.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct AudioChannel {
    pub channel: &'static str,
    /// Stream id (hex).
    pub stream: String,
    pub seconds: f64,
}

/// The audio bank of a scene.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct SceneAudio {
    /// The name the stream ids are hashed from.
    pub name: String,
    /// "scene name" (the scene's own name without suffixes), "audio name"
    /// (named by a scene file's audio record) or "master timeline" (the
    /// bank of the masters that place the scene).
    pub source: &'static str,
    pub archive: String,
    /// Bank offset in the archive (hex).
    pub bank: String,
    pub channels: Vec<AudioChannel>,
}

/// Stream ids of an audio index, for finding cutscene banks.
pub struct AudioBanks<'a> {
    ix: &'a crate::audio::AudioIndex,
    by_id: HashMap<u32, Vec<usize>>,
}

impl<'a> AudioBanks<'a> {
    pub fn new(ix: &'a crate::audio::AudioIndex) -> Self {
        let mut by_id: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, b) in ix.banks.iter().enumerate() {
            for s in &b.streams {
                let v = by_id.entry(s.id).or_default();
                if v.last() != Some(&i) {
                    v.push(i);
                }
            }
        }
        AudioBanks { ix, by_id }
    }

    /// The one bank that holds at least two channels of `name`. A single
    /// channel id is left alone (a chance match on 29 bits is likely
    /// among a few hundred thousand streams), as are names whose channels
    /// are split over several banks that each hold two or more.
    pub fn find(&self, name: &str, source: &'static str) -> Option<SceneAudio> {
        let lower = name.to_ascii_lowercase();
        let mut per: BTreeMap<usize, Vec<(&'static str, u32)>> = BTreeMap::new();
        for ch in AUDIO_CHANNELS {
            let id = audio_stream_id(&lower, ch);
            for &b in self.by_id.get(&id).into_iter().flatten() {
                per.entry(b).or_default().push((ch, id));
            }
        }
        let mut hits = per.into_iter().filter(|x| x.1.len() >= 2);
        let (b, chans) = hits.next()?;
        if hits.next().is_some() {
            return None;
        }
        let bank = &self.ix.banks[b];
        let channels = chans
            .into_iter()
            .map(|(channel, id)| AudioChannel { channel, stream: format!("{id:08x}"), seconds: bank.streams.iter().find(|s| s.id == id).map_or(0.0, |s| s.seconds()) })
            .collect();
        Some(SceneAudio { name: name.to_string(), source, archive: self.ix.archive.clone(), bank: format!("{:x}", bank.off), channels })
    }

    /// The bank of a scene file's audio name (`<mission>_<scene>`, such as
    /// `air_tail_air_tail_int`): the name lower-cased, then without one
    /// leading word after another, until a name has banks with two or more
    /// channels ([`AudioBanks::find`] then decides).
    pub fn find_audio_name(&self, name: &str) -> Option<SceneAudio> {
        let lower = name.to_ascii_lowercase();
        let mut cand = lower.as_str();
        loop {
            if self.has_channels(cand) {
                return self.find(cand, "audio name");
            }
            cand = cand.split_once('_')?.1;
        }
    }

    fn has_channels(&self, name: &str) -> bool {
        let mut per: HashMap<usize, usize> = HashMap::new();
        AUDIO_CHANNELS.iter().any(|ch| {
            self.by_id.get(&audio_stream_id(name, ch)).into_iter().flatten().any(|&b| {
                let n = per.entry(b).or_default();
                *n += 1;
                *n >= 2
            })
        })
    }
}

/// Where a scene section plays in a master timeline.
#[derive(Clone, Debug, Serialize)]
pub struct Placement {
    /// Scene file (entry id) with the master timeline.
    pub master: String,
    /// The master's audio names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub master_audio: Vec<String>,
    /// Seconds into the master.
    pub start: f32,
    /// The next later start in the master, where this slot ends (none for
    /// the last).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<f32>,
    /// `start` on the master's audio bank clock: minus the master's
    /// earliest start.
    pub bank_time: f32,
    /// `sectionId` (hex).
    pub section: String,
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
    /// Audio names of the file's audio records (`AUDIO`), sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<String>,
    /// The master timeline, by start time ([`timeline`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timeline: Vec<Slot>,
    /// The master's state machine ([`master_states`]); masters only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fsm: Vec<FsmState>,
    /// Subtitle events ([`subtitle_events`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitles: Vec<SubtitleEvent>,
}

/// A transition of a master's state machine.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FsmTransition {
    /// The state it leads to (`toState`).
    pub to: u32,
    /// It has a condition naming its own source state ("when this state
    /// ends").
    pub on_end: bool,
    /// Conditions on an entity's value (branches), and all conditions.
    pub entity_conditions: usize,
    pub conditions: usize,
}

/// A state of a master's state machine.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FsmState {
    /// The state's name hash: a key of the id map.
    pub name: u32,
    /// The start-map keys it plays ([`Slot::key`]).
    pub starts: Vec<u32>,
    pub transitions: Vec<FsmTransition>,
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

/// Read a scene file: its state names, its ped, object and camera
/// entries, its audio names and its master timeline. None when the
/// resource has no state machine states, or neither a timeline nor any of
/// the three object kinds.
pub fn scene_file(id: &str, m: &Meta) -> Option<SceneFile> {
    use hashes::*;
    let name = member_off(m, STATE, joaat("name"))?;
    let mut states: Vec<u32> = m.objects(STATE).into_iter().map(|o| m.r.u32(o + name.0)).filter(|&h| h != 0).collect();
    states.sort_unstable();
    states.dedup();
    // master timelines often have no object records of their own
    let timeline = timeline(m);
    if states.is_empty() || (timeline.is_empty() && ![PED, OBJECT, CAMERA].iter().any(|s| m.structs.contains_key(s))) {
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
    let mut audio: Vec<String> = m.objects(AUDIO).into_iter().filter_map(|o| m.member_value(AUDIO, o, AUDIO_NAME)?.as_str().filter(|s| !s.is_empty()).map(String::from)).collect();
    audio.sort();
    audio.dedup();
    let fsm = if timeline.is_empty() { Vec::new() } else { master_states(m) };
    Some(SceneFile { id: id.to_string(), states, objects, audio, timeline, fsm, subtitles: subtitle_events(m) })
}

/// A timeline item: a section reference or a composite.
#[derive(Clone, Debug, PartialEq)]
pub enum TimelineItem {
    Reference { scene: u32, section: u32 },
    Composite { id: Option<u32>, sections: Vec<u32> },
}

/// Resolve start times against the id map: each start's key is followed
/// through composites (their own id, else the first of their listed ids
/// that is in the map) to a section reference, at most 8 steps. Starts
/// that end nowhere are dropped. Sorted by start time, without repeats.
pub fn resolve_timeline(starts: &[(u32, f32)], ids: &HashMap<u32, TimelineItem>) -> Vec<Slot> {
    let mut out = Vec::new();
    for &(key, start) in starts {
        let mut it = ids.get(&key);
        for _ in 0..8 {
            match it {
                Some(TimelineItem::Composite { id, sections }) => {
                    it = id.and_then(|i| ids.get(&i)).or_else(|| sections.iter().find_map(|s| ids.get(s)));
                }
                _ => break,
            }
        }
        if let Some(TimelineItem::Reference { scene, section }) = it {
            out.push(Slot { key, also: Vec::new(), start, scene: *scene, section: *section });
        }
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.scene.cmp(&b.scene)).then(a.section.cmp(&b.section)).then(a.key.cmp(&b.key)));
    // a file can hold the start map twice: one slot, the other keys kept
    let mut merged: Vec<Slot> = Vec::with_capacity(out.len());
    for sl in out {
        match merged.last_mut() {
            Some(p) if p.start == sl.start && p.scene == sl.scene && p.section == sl.section => {
                if sl.key != p.key && !p.also.contains(&sl.key) {
                    p.also.push(sl.key);
                }
            }
            _ => merged.push(sl),
        }
    }
    merged
}

/// The master timeline of a scene file: its start-time map's entries
/// resolved against its id map ([`resolve_timeline`]). Empty for files
/// without one.
pub fn timeline(m: &Meta) -> Vec<Slot> {
    use hashes::*;
    let Some((off, _)) = member_off(m, START, START_OFFSET) else { return Vec::new() };
    let u32_of = |v: serde_json::Value| v.as_u64().map(|x| x as u32);
    let mut starts = Vec::new();
    let mut ids: HashMap<u32, TimelineItem> = HashMap::new();
    for (key, h, p) in map_entries(m) {
        match h {
            START => starts.push((key, m.r.f32(p + off))),
            REFERENCE => {
                let scene = m.member_value(h, p, REFERENCE_SCENE).and_then(u32_of).unwrap_or(0);
                let section = m.member_value(h, p, REFERENCE_SECTION).and_then(u32_of).unwrap_or(0);
                ids.entry(key).or_insert(TimelineItem::Reference { scene, section });
            }
            COMPOSITE => {
                let id = m.member_value(h, p, COMPOSITE_ID).and_then(u32_of).filter(|&x| x != 0);
                let sections = match m.member_value(h, p, COMPOSITE_SECTIONS) {
                    Some(serde_json::Value::Array(a)) => a.into_iter().filter_map(u32_of).collect(),
                    _ => Vec::new(),
                };
                ids.entry(key).or_insert(TimelineItem::Composite { id, sections });
            }
            _ => {}
        }
    }
    resolve_timeline(&starts, &ids)
}

/// Count a transition's conditions: (names its source state, entity value
/// tests, all). `conds` is its condition set as [`Meta::member_value`]
/// decodes it; every object below it with a `_struct` is one condition,
/// except condition lists (objects holding only arrays).
pub fn count_conditions(conds: &serde_json::Value, source: u32) -> (bool, usize, usize) {
    use hashes::*;
    let state_key = crate::meta7a::field_name(CONDITION_STATE_ID);
    let (mut on_end, mut entity, mut all) = (false, 0, 0);
    let mut todo: Vec<(&serde_json::Value, bool)> = vec![(conds, true)];
    while let Some((v, top)) = todo.pop() {
        match v {
            serde_json::Value::Object(o) => {
                let st = o.get("_struct").and_then(|v| v.as_str()).and_then(|v| u32::from_str_radix(v, 16).ok());
                let list = o.iter().filter(|(k, _)| *k != "_struct").all(|(_, v)| v.is_array());
                match st {
                    _ if top => {}
                    Some(CONDITION_STATE) => {
                        all += 1;
                        on_end |= o.get(&state_key).and_then(|v| v.as_u64()) == Some(source as u64);
                    }
                    Some(CONDITION_ENTITY) => {
                        all += 1;
                        entity += 1;
                    }
                    Some(_) if !list => all += 1,
                    _ => {}
                }
                todo.extend(o.values().map(|x| (x, false)));
            }
            serde_json::Value::Array(a) => todo.extend(a.iter().map(|x| (x, false))),
            _ => {}
        }
    }
    (on_end, entity, all)
}

/// The state machine of a master: each state (`STATE`) with the start keys
/// its id-map item lists and its transitions ([`count_conditions`]).
/// Sorted by state name.
pub fn master_states(m: &Meta) -> Vec<FsmState> {
    use hashes::*;
    let mut starts_of: HashMap<u32, Vec<u32>> = HashMap::new();
    for (key, h, p) in map_entries(m) {
        if h == STATE_ITEM {
            if let Some(serde_json::Value::Array(a)) = m.member_value(h, p, STATE_STARTS) {
                starts_of.entry(key).or_insert_with(|| a.iter().filter_map(|v| v.as_u64()).map(|x| x as u32).collect());
            }
        }
    }
    let Some((name_off, _)) = member_off(m, STATE, joaat("name")) else { return Vec::new() };
    let tr_member = m.structs.get(&STATE).and_then(|sd| sd.members.iter().find(|x| x.hash == STATE_TRANSITIONS && x.ty == 0x5c).map(|x| x.off as usize));
    let tr_size = m.structs.get(&TRANSITION).map_or(0, |sd| sd.size as usize);
    let to_off = member_off(m, TRANSITION, TRANSITION_TO).map(|x| x.0);
    let mut out = Vec::new();
    for o in m.objects(STATE) {
        let name = m.r.u32(o + name_off);
        if name == 0 {
            continue;
        }
        let mut transitions = Vec::new();
        if let (Some(a), Some(to_off)) = (tr_member, to_off) {
            let n = m.r.u32(o + a) as usize;
            if let Some((_, base)) = m.ptr(m.r.u64(o + a + 8)).filter(|_| tr_size > 0 && n < 10_000) {
                for i in 0..n {
                    let t = base + i * tr_size;
                    let (on_end, entity_conditions, conditions) = m.member_value(TRANSITION, t, TRANSITION_CONDITIONS).map_or((false, 0, 0), |c| count_conditions(&c, name));
                    transitions.push(FsmTransition { to: m.r.u32(t + to_off), on_end, entity_conditions, conditions });
                }
            }
        }
        out.push(FsmState { name, starts: starts_of.get(&name).cloned().unwrap_or_default(), transitions });
    }
    out.sort_by_key(|x| x.name);
    out.dedup_by_key(|x| x.name);
    out
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
    // (and master timelines, which place them)
    files.retain(|f| !f.timeline.is_empty() || f.states.iter().any(|h| scene_hashes.contains(h)));
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
    /// The named models of the game's outfit list, as characters: the viewer
    /// puts them together from their outfits, also those made of shared
    /// parts only (`cs_mandy`), which no drawable prefix names.
    pub fn add_outfit_list(&mut self, game_root: &Path) {
        if let Ok(m) = crate::pedlook::listed_models(game_root) {
            self.add_characters(m.iter().filter(|m| m.named && !crate::outfit::is_animal(&m.name)).map(|m| m.name.as_str()));
        }
    }
    /// Every source of model names: the recovered names of the indexed
    /// archives, the characters of packs.rpf, the outfit list and the name
    /// dictionary.
    pub fn of_workspace(ws: &crate::export::Workspace) -> Models {
        let mut models = Models::default();
        for a in ws.archives() {
            if !ws.is_indexed(&a) {
                continue;
            }
            let Ok(ix) = ws.load_index(&a) else { continue };
            if a.id == "packs.rpf" {
                let ch = crate::outfit::characters(&ix);
                models.add_characters(ch.iter().filter(|c| c.kind == "character").map(|c| c.name.as_str()));
            }
            models.add_index(&ix);
        }
        models.add_outfit_list(&ws.game);
        models.add_name_dict(&ws.work);
        models
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
    /// The scene's audio bank, when an audio index was given ([`attach_audio`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<SceneAudio>,
    /// Where the scene's sections play in master timelines.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placements: Vec<Placement>,
    /// The audio bank of each master in `placements` (master entry id ->
    /// bank), when an audio index was given.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub master_audio: BTreeMap<String, SceneAudio>,
    /// Subtitle lines of the scene's own scene files, by start time
    /// ([`attach_dialogue`] adds text and speaker names).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subtitles: Vec<SceneSubtitle>,
}

/// A line's time on a master's audio bank clock.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct BankTime {
    /// The master (scene file entry id).
    pub master: String,
    pub time: f32,
}

/// A subtitle line of a scene.
#[derive(Clone, Debug, Serialize)]
pub struct SceneSubtitle {
    pub label: String,
    /// Speaker: the dialogue links' name for the hash, else the hash (hex).
    pub speaker: String,
    /// Seconds into the scene.
    pub start: f32,
    pub end: f32,
    /// Where the line plays on master bank clocks: for each slot whose
    /// section the line plays under, the slot's `bank_time` plus `start`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bank_times: Vec<BankTime>,
    /// Text from the dialogue links.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Whether the dialogue links give this label the same speaker (by
    /// hash); none when they give no speaker hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_agrees: Option<bool>,
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
    // scene hash -> its slots in master timelines
    let mut placed: HashMap<u32, Vec<Placement>> = HashMap::new();
    for f in &c.files {
        let first = f.timeline.first().map_or(0.0, |x| x.start);
        for sl in &f.timeline {
            let end = f.timeline.iter().map(|x| x.start).find(|&x| x > sl.start);
            placed.entry(sl.scene).or_default().push(Placement { master: f.id.clone(), master_audio: f.audio.clone(), start: sl.start, end, bank_time: sl.start - first, section: format!("{:08x}", sl.section) });
        }
    }
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
            audio: None,
            subtitles: {
                let pl: &[Placement] = placed.get(&joaat(scene)).map_or(&[], |v| v.as_slice());
                let mut v: Vec<SceneSubtitle> = files
                    .iter()
                    .filter(|f| f.timeline.is_empty())
                    .flat_map(|f| &f.subtitles)
                    .map(|e| {
                        let bank_times = pl
                            .iter()
                            .filter(|p| u32::from_str_radix(&p.section, 16).is_ok_and(|x| e.sections.contains(&x)))
                            .map(|p| BankTime { master: p.master.clone(), time: p.bank_time + e.start })
                            .collect();
                        SceneSubtitle { label: e.label.clone(), speaker: format!("{:08x}", e.speaker), start: e.start, end: e.end, bank_times, text: None, speaker_agrees: None }
                    })
                    .collect();
                v.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.label.cmp(&b.label)));
                // the same line in two files (a scene and its variant): one line, all its times
                v.dedup_by(|a, b| {
                    let same = a.label == b.label && a.start == b.start;
                    if same {
                        b.bank_times.append(&mut a.bank_times);
                    }
                    same
                });
                for x in &mut v {
                    x.bank_times.sort_by(|a, b| a.master.cmp(&b.master).then(a.time.total_cmp(&b.time)));
                    x.bank_times.dedup();
                }
                v
            },
            placements: placed.get(&joaat(scene)).cloned().unwrap_or_default(),
            master_audio: BTreeMap::new(),
        });
    }
    out
}

/// Find each scene's audio bank: by the scene's own name, then without its
/// suffixes ([`audio_base_name`]), then by the audio names of its scene
/// files ([`AudioBanks::find_audio_name`]), then the bank of the master
/// timelines that place it (when they agree); and the bank of each master
/// that places it, by the master's audio names. The first name with one
/// bank wins.
pub fn attach_audio(all: &mut [Scene], c: &Catalog, banks: &AudioBanks) {
    let files: HashMap<&str, &SceneFile> = c.files.iter().map(|f| (f.id.as_str(), f)).collect();
    let by_names = |names: &[String]| names.iter().find_map(|n| banks.find_audio_name(n));
    for s in all.iter_mut() {
        let base = audio_base_name(&s.name).to_string();
        s.audio = banks.find(&s.name, "scene name").or_else(|| banks.find(&base, "scene name")).or_else(|| {
            let names: Vec<String> = s.scene_files.iter().filter_map(|f| files.get(f.as_str())).flat_map(|f| f.audio.iter().cloned()).collect();
            by_names(&names)
        });
        for p in &s.placements {
            if !s.master_audio.contains_key(&p.master) {
                if let Some(a) = by_names(&p.master_audio) {
                    s.master_audio.insert(p.master.clone(), a);
                }
            }
        }
        // else the bank of the masters that place it, when they share one
        if s.audio.is_none() {
            let mut v = s.master_audio.values();
            if let Some(first) = v.next() {
                if v.all(|a| a.bank == first.bank) {
                    s.audio = Some(SceneAudio { source: "master timeline", ..first.clone() });
                }
            }
        }
    }
}

/// Give the scenes' subtitle lines their text and speaker names from the
/// dialogue links (looked up by the joaat of the label), and say whether
/// the links' speaker hash is the event's own. Speaker names come from
/// the links' speaker labels that are not the bare hash.
pub fn attach_dialogue(all: &mut [Scene], links: &crate::dialogue::DialogueLinks) {
    let by = links.by_subtitle();
    let mut names: HashMap<u32, &str> = HashMap::new();
    for l in &links.links {
        if let Some(sp) = &l.speaker {
            if let Some(h) = sp.hash.filter(|h| sp.label != format!("{h:08x}")) {
                names.entry(h).or_insert(sp.label.as_str());
            }
        }
    }
    for s in all.iter_mut() {
        for sub in &mut s.subtitles {
            let hash = u32::from_str_radix(&sub.speaker, 16).ok();
            let l = by.get(&joaat(&sub.label)).and_then(|v| v.first());
            sub.text = l.and_then(|l| l.text.clone());
            sub.speaker_agrees = l.and_then(|l| l.speaker.as_ref()?.hash).zip(hash).map(|(a, b)| a == b);
            if let Some(n) = hash.and_then(|h| names.get(&h)) {
                sub.speaker = n.to_string();
            }
        }
    }
}

/// One slot of a master in a report: the scene by name when it is
/// catalogued, else its hash.
#[derive(Clone, Debug, Serialize)]
pub struct MasterSlot {
    /// Start-map key (hex).
    pub key: String,
    pub start: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<f32>,
    pub scene: String,
    /// `sectionId` (hex).
    pub section: String,
}

/// A transition in a report, with the target state's start.
#[derive(Clone, Debug, Serialize)]
pub struct MasterTransition {
    /// `toState` (hex).
    pub to: String,
    /// The target state's earliest start, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_start: Option<f32>,
    pub on_end: bool,
    pub entity_conditions: usize,
    pub conditions: usize,
}

/// A state in a report: its name, the starts it plays and its transitions.
#[derive(Clone, Debug, Serialize)]
pub struct MasterStateView {
    /// Name hash (hex).
    pub name: String,
    /// The start times of the slots it plays, sorted.
    pub starts: Vec<f32>,
    pub transitions: Vec<MasterTransition>,
}

/// A master timeline with its state machine, for reports.
#[derive(Clone, Debug, Serialize)]
pub struct Master {
    /// Scene file (entry id).
    pub id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank: Option<SceneAudio>,
    pub slots: Vec<MasterSlot>,
    pub states: Vec<MasterStateView>,
}

/// Totals of the masters' state machines.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct FsmTotals {
    pub states: usize,
    /// States with no transition (where a master ends).
    pub end_states: usize,
    pub transitions: usize,
    pub on_end: usize,
    pub with_entity_conditions: usize,
    /// Transitions placed by the earliest start of both states: to a
    /// later state, to the next later start, to an earlier one, and those
    /// with a state that plays no start.
    pub forward: usize,
    pub to_next_start: usize,
    pub backward: usize,
    pub unplaced: usize,
}

/// The start time of each start-map key of a master.
fn key_starts(f: &SceneFile) -> HashMap<u32, f32> {
    f.timeline.iter().flat_map(|x| std::iter::once(x.key).chain(x.also.iter().copied()).map(move |k| (k, x.start))).collect()
}

/// The earliest start of each state of a master.
fn state_starts(f: &SceneFile) -> HashMap<u32, f32> {
    let at = key_starts(f);
    f.fsm.iter().filter_map(|st| st.starts.iter().filter_map(|k| at.get(k).copied()).min_by(|a, b| a.total_cmp(b)).map(|t| (st.name, t))).collect()
}

/// Totals over the masters of a catalog.
pub fn fsm_totals(c: &Catalog) -> FsmTotals {
    let mut t = FsmTotals::default();
    for f in c.files.iter().filter(|f| !f.timeline.is_empty()) {
        let first = state_starts(f);
        let mut later: Vec<f32> = first.values().copied().collect();
        later.sort_by(|a, b| a.total_cmp(b));
        later.dedup();
        for st in &f.fsm {
            t.states += 1;
            t.end_states += st.transitions.is_empty() as usize;
            for tr in &st.transitions {
                t.transitions += 1;
                t.on_end += tr.on_end as usize;
                t.with_entity_conditions += (tr.entity_conditions > 0) as usize;
                match (first.get(&st.name), first.get(&tr.to)) {
                    (Some(a), Some(b)) if b > a => {
                        t.forward += 1;
                        t.to_next_start += (later.iter().find(|&&x| x > *a) == Some(b)) as usize;
                    }
                    (Some(a), Some(b)) if b < a => t.backward += 1,
                    (Some(_), Some(_)) => {}
                    _ => t.unplaced += 1,
                }
            }
        }
    }
    t
}

/// The masters of a catalog for reports: slots with scene names from
/// `all`, states with the start times they play, and the bank of the
/// master's audio names when `banks` is given.
pub fn masters(c: &Catalog, all: &[Scene], banks: Option<&AudioBanks>) -> Vec<Master> {
    let names: HashMap<u32, &str> = all.iter().map(|s| (joaat(&s.name), s.name.as_str())).collect();
    let mut out = Vec::new();
    for f in c.files.iter().filter(|f| !f.timeline.is_empty()) {
        let at = key_starts(f);
        let first = state_starts(f);
        let slots = f
            .timeline
            .iter()
            .map(|x| MasterSlot {
                key: format!("{:08x}", x.key),
                start: x.start,
                end: f.timeline.iter().map(|y| y.start).find(|&y| y > x.start),
                scene: names.get(&x.scene).map_or_else(|| format!("{:08x}", x.scene), |n| n.to_string()),
                section: format!("{:08x}", x.section),
            })
            .collect();
        let states = f
            .fsm
            .iter()
            .map(|st| {
                let mut starts: Vec<f32> = st.starts.iter().filter_map(|k| at.get(k).copied()).collect();
                starts.sort_by(|a, b| a.total_cmp(b));
                starts.dedup();
                let transitions = st
                    .transitions
                    .iter()
                    .map(|tr| MasterTransition { to: format!("{:08x}", tr.to), to_start: first.get(&tr.to).copied(), on_end: tr.on_end, entity_conditions: tr.entity_conditions, conditions: tr.conditions })
                    .collect();
                MasterStateView { name: format!("{:08x}", st.name), starts, transitions }
            })
            .collect();
        let bank = banks.and_then(|b| f.audio.iter().find_map(|n| b.find_audio_name(n)));
        out.push(Master { id: f.id.clone(), audio: f.audio.clone(), bank, slots, states });
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
            audio: vec!["test_scene_audio".into()],
            timeline: vec![],
            fsm: vec![],
            subtitles: vec![SubtitleEvent { label: "TEST_SCENE_2".into(), speaker: joaat("lucia"), start: 3.0, end: 4.0, sections: vec![] }, SubtitleEvent { label: "TEST_SCENE_1".into(), speaker: joaat("boss"), start: 1.0, end: 2.0, sections: vec![] }],
        };
        // the second scene's own file: a line of section 2 (placed in the master at 12.5 s,
        // bank time 10 s) and one of a section no slot plays
        let f2 = SceneFile {
            id: "a.rpf@50".into(),
            states: vec![joaat("other_scene_p1_t01")],
            objects: vec![],
            audio: vec![],
            timeline: vec![],
            fsm: vec![],
            subtitles: vec![SubtitleEvent { label: "OTHER_1".into(), speaker: 0x1234, start: 0.5, end: 1.5, sections: vec![2, 9] }, SubtitleEvent { label: "OTHER_2".into(), speaker: 0x1234, start: 0.25, end: 1.0, sections: vec![9] }],
        };
        // a master timeline that places the second scene's section 2 at 12.5 s
        let master = SceneFile { id: "a.rpf@40".into(), states: vec![joaat("test_master")], objects: vec![], audio: vec!["tst_test_master".into()], timeline: vec![Slot { key: 11, also: vec![], start: 2.5, scene: 5, section: 1 }, Slot { key: 12, also: vec![], start: 12.5, scene: joaat("other_scene_p1_t01"), section: 2 }, Slot { key: 13, also: vec![14], start: 20.0, scene: 6, section: 3 }], fsm: test_fsm(), subtitles: vec![] };
        // a second scene without a scene file of its own
        let d2 = SceneDict { name: "cin@other_scene_p1_t01".into(), id: "a.rpf@30".into(), clips: ["boss", "boss_face", "ib_phone"].map(String::from).to_vec(), sections: 0, section_ids: vec![], adopted_sections: 0, streamed: vec![], unjoined: vec![], windows_streamed: 0, windows_missing: 0 };
        Catalog { version: CATALOG_VERSION, archive: "a.rpf".into(), dicts: vec![d, d2], files: vec![f, master, f2] }
    }

    #[test]
    fn audio_stream_ids_and_base_names() {
        // the issue's test vector
        let ids: Vec<u32> = ["c", "l", "r", "ad"].iter().map(|c| audio_stream_id("air_tail_int", c)).collect();
        assert_eq!(ids, vec![0x106b_5e4b, 0x045b_8607, 0x1f3e_fbbf, 0x1673_5206]);
        assert_eq!(audio_stream_id("AIR_TAIL_INT", "c"), 0x106b_5e4b);
        assert_eq!(audio_base_name("air_beach_ext_p1_t05_x1"), "air_beach_ext");
        assert_eq!(audio_base_name("mtl_mont_int_p0l_t01"), "mtl_mont_int");
        assert_eq!(audio_base_name("abc_mcs2_t10"), "abc_mcs2");
        assert_eq!(audio_base_name("abc_int"), "abc_int");
        // only whole suffix words: `_part` and `_tx` stay
        assert_eq!(audio_base_name("abc_part"), "abc_part");
        assert_eq!(audio_base_name("abc_tx"), "abc_tx");
        assert_eq!(audio_base_name("x1"), "x1");
    }

    /// Three states: 100 plays the first start and leads on to 200 when it
    /// ends, or to 300 on a branch; 200 plays the second start and leads
    /// back to 100; 300 plays the last and ends. 400 plays nothing.
    fn test_fsm() -> Vec<FsmState> {
        let tr = |to, on_end, entity| FsmTransition { to, on_end, entity_conditions: entity, conditions: on_end as usize + entity };
        vec![
            FsmState { name: 100, starts: vec![11], transitions: vec![tr(200, true, 0), tr(300, true, 1)] },
            FsmState { name: 200, starts: vec![12], transitions: vec![tr(100, true, 0), tr(400, false, 1)] },
            FsmState { name: 300, starts: vec![13, 99], transitions: vec![] },
            FsmState { name: 400, starts: vec![], transitions: vec![] },
            // a state listing only the repeat key of the last slot
            FsmState { name: 500, starts: vec![14], transitions: vec![] },
        ]
    }

    #[test]
    fn scene_subtitles_get_text_speakers_and_bank_times() {
        use crate::dialogue::{DialogueLink, DialogueLinks, Speaker};
        let c = catalog();
        let mut all = scenes(&c, &Models::default());
        let s = &all[0];
        assert_eq!(s.subtitles.iter().map(|x| (x.label.as_str(), x.start, x.bank_times.len())).collect::<Vec<_>>(), vec![("TEST_SCENE_1", 1.0, 0), ("TEST_SCENE_2", 3.0, 0)]);
        // a line of the slot's section is on the master's bank clock; the other is not
        let bt = BankTime { master: "a.rpf@40".into(), time: 10.5 };
        assert_eq!(all[1].subtitles.iter().map(|x| (x.label.as_str(), x.bank_times.clone())).collect::<Vec<_>>(), vec![("OTHER_2", vec![]), ("OTHER_1", vec![bt])]);
        let link = |label: &str, sp: Option<(&str, u32)>, text: &str| DialogueLink { subtitle_key: joaat(label), speaker: sp.map(|(l, h)| Speaker { label: l.into(), hash: Some(h) }), audio: None, text: Some(text.into()), mission: None, scene: None };
        let links = DialogueLinks {
            schema_version: 0,
            links: vec![
                link("TEST_SCENE_1", Some(("boss", joaat("boss"))), "Hello."),
                // the links name lucia's hash through another line, and give this one another speaker
                link("TEST_SCENE_2", Some(("00000007", 7)), "Bye."),
                link("SOMEWHERE_ELSE", Some(("lucia", joaat("lucia"))), "Hi."),
            ],
        };
        attach_dialogue(&mut all, &links);
        let v: Vec<_> = all[0].subtitles.iter().map(|x| (x.speaker.as_str(), x.text.as_deref(), x.speaker_agrees)).collect();
        assert_eq!(v, vec![("boss", Some("Hello."), Some(true)), ("lucia", Some("Bye."), Some(false))]);
        // a label the links lack keeps its hash and has no text
        assert_eq!((all[1].subtitles[1].speaker.as_str(), all[1].subtitles[1].text.as_deref()), ("00001234", None));
    }

    #[test]
    fn masters_report_states_and_transitions() {
        let c = catalog();
        let t = fsm_totals(&c);
        assert_eq!(t, FsmTotals { states: 5, end_states: 3, transitions: 4, on_end: 3, with_entity_conditions: 2, forward: 2, to_next_start: 1, backward: 1, unplaced: 1 });
        let all = scenes(&c, &Models::default());
        let ms = masters(&c, &all, None);
        assert_eq!(ms.len(), 1);
        let m = &ms[0];
        assert_eq!(m.slots.iter().map(|x| x.scene.as_str()).collect::<Vec<_>>(), vec!["00000005", "other_scene_p1_t01", "00000006"]);
        assert_eq!((m.slots[1].key.as_str(), m.slots[1].end), ("0000000c", Some(20.0)));
        let s100 = &m.states[0];
        assert_eq!((s100.name.as_str(), s100.starts.clone()), ("00000064", vec![2.5]));
        assert_eq!(s100.transitions.iter().map(|x| (x.to.as_str(), x.to_start)).collect::<Vec<_>>(), vec![("000000c8", Some(12.5)), ("0000012c", Some(20.0))]);
        // a listed key with no slot is left out of the starts; a repeat key finds its slot
        assert_eq!(m.states[2].starts, vec![20.0]);
        assert_eq!(m.states[4].starts, vec![20.0]);
    }

    #[test]
    fn transition_conditions_are_counted() {
        let key = crate::meta7a::field_name(hashes::CONDITION_STATE_ID);
        let mut first = serde_json::json!({"_struct": "57439252", "invert": false});
        first[key.as_str()] = serde_json::json!(7);
        let c = serde_json::json!({
            "_struct": "4b852c44",
            "conditions": [
                first,
                {"_struct": "d5f7c394", "entityName": 1, "value": 2, "invert": false},
                // a nested condition list: its members count, the list does not
                {"_struct": "262073c2", "conditions": [{"_struct": "d5f7c394", "entityName": 3, "value": 4}, {"_struct": "0ca33eea", "x": 1}]}
            ]
        });
        assert_eq!(count_conditions(&c, 7), (true, 2, 4));
        // a state condition naming another state is not "on end"
        assert_eq!(count_conditions(&c, 8), (false, 2, 4));
        assert_eq!(count_conditions(&serde_json::json!({"_struct": "4b852c44", "conditions": []}), 7), (false, 0, 0));
    }

    #[test]
    fn timelines_resolve_through_composites() {
        use TimelineItem::*;
        let ids: HashMap<u32, TimelineItem> = [
            (1, Reference { scene: 100, section: 7 }),
            // a second key for the same section (a repeated start map)
            (7, Reference { scene: 100, section: 7 }),
            // composite with its own id
            (2, Composite { id: Some(1), sections: vec![] }),
            // composite whose id is missing: the first listed id in the map
            (3, Composite { id: Some(99), sections: vec![98, 2] }),
            // a loop ends nowhere
            (4, Composite { id: Some(5), sections: vec![] }),
            (5, Composite { id: Some(4), sections: vec![] }),
        ]
        .into_iter()
        .collect();
        let starts = [(3, 30.0), (1, 10.0), (2, 20.0), (4, 40.0), (6, 50.0), (1, 10.0), (7, 10.0)];
        let got = resolve_timeline(&starts, &ids);
        assert_eq!(got.iter().map(|s| (s.start, s.scene, s.section)).collect::<Vec<_>>(), vec![(10.0, 100, 7), (20.0, 100, 7), (30.0, 100, 7)]);
        assert_eq!((got[0].key, got[0].also.clone()), (1, vec![7]));
    }

    fn audio_index(banks: &[(u64, &[u32])]) -> crate::audio::AudioIndex {
        let stream = |id: u32| crate::audio::Stream { id, chunks: vec![], data: (0, 16), samples: 96000, rate: 48000, codec: 0, channels: 1, layout: "blocks".into() };
        crate::audio::AudioIndex {
            version: 0,
            archive: "a.rpf".into(),
            archive_size: 0,
            banks: banks.iter().map(|(off, ids)| crate::audio::Bank { off: *off, size: 0, flags: 0, streams: ids.iter().map(|&i| stream(i)).collect(), names: Default::default() }).collect(),
            note: String::new(),
        }
    }

    #[test]
    fn scenes_find_their_audio_banks_and_placements() {
        let id = audio_stream_id;
        let ix = audio_index(&[
            // the first scene's bank, by its name without suffixes
            (0x1000, &[id("test_scene", "c"), id("test_scene", "l"), id("test_scene", "ad"), 77]),
            // the master's bank, by its audio name
            (0x2000, &[id("test_master", "l"), id("test_master", "r")]),
            // one channel of the second scene alone: a possible chance match, not a bank
            (0x3000, &[id("other_scene", "c")]),
            // a name with two banks of two channels each: left unresolved
            (0x4000, &[id("test_scene_audio", "c"), id("test_scene_audio", "l")]),
            (0x5000, &[id("test_scene_audio", "r"), id("test_scene_audio", "ad")]),
        ]);
        let banks = AudioBanks::new(&ix);
        assert!(banks.find("other_scene", "scene name").is_none());
        assert!(banks.find("test_scene_audio", "audio name").is_none());
        let c = catalog();
        let mut all = scenes(&c, &Models::default());
        attach_audio(&mut all, &c, &banks);
        let a = all[0].audio.as_ref().unwrap();
        assert_eq!((a.name.as_str(), a.source, a.bank.as_str()), ("test_scene", "scene name", "1000"));
        assert_eq!(a.channels.iter().map(|x| x.channel).collect::<Vec<_>>(), vec!["c", "l", "ad"]);
        assert_eq!((a.channels[0].stream.clone(), a.channels[0].seconds), (format!("{:08x}", id("test_scene", "c")), 2.0));
        assert!(all[0].placements.is_empty());
        let o = &all[1];
        // no bank by its own names: the bank of the master that places it
        let oa = o.audio.as_ref().unwrap();
        assert_eq!((oa.name.as_str(), oa.source, oa.bank.as_str()), ("test_master", "master timeline", "2000"));
        assert_eq!(o.placements.len(), 1);
        let p = &o.placements[0];
        assert_eq!((p.master.as_str(), p.start, p.end, p.bank_time, p.section.as_str()), ("a.rpf@40", 12.5, Some(20.0), 10.0, "00000002"));
        // the master's audio name `<mission>_<scene>` resolves without its first word
        let m = &o.master_audio["a.rpf@40"];
        assert_eq!((m.name.as_str(), m.source, m.bank.as_str()), ("test_master", "audio name", "2000"));
    }

    #[test]
    fn scene_objects_resolve() {
        let mut m = Models::default();
        m.add_characters(["player_story000_01"]);
        m.add(["cs_boss", "prop_cup_01a", "prop_table_01a"]);
        let all = scenes(&catalog(), &m);
        assert_eq!(all.len(), 2);
        // the second scene: no ped record in its own file, so the model from the other scene; the in-hand item a prop
        let o = &all[1];
        assert_eq!(o.scene_files, vec!["a.rpf@50".to_string()]);
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
