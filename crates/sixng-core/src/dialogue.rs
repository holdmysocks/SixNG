//! Dialogue links: which speaker says a subtitle, and which audio stream
//! holds the line.
//!
//! `sixng dialogue-links` writes `<work>/dialogue_links.json` from a local
//! address table. Each audio address is rebuilt from the SixNG audio index
//! (bank offset and stream position give the stream id and the encoded byte
//! range), so the table only names the stream; an address that does not
//! match the index is left out. A voice line's wave name hashes to its
//! stream id (`joaat(name)` low 29 bits), which is checked too.
//!
//! The links can also come from the game's own conversation files (PSO,
//! one per mission in a pack such as `shared_minigame000`): a conversation
//! lists its characters and its lines in spoken order, and each line stores
//! its subtitle label in plain text, its audio cue (`Context`) and which
//! take of the cue it uses (`Variation`). The subtitle key is
//! `joaat(label)` and the wave is `<cue>_<take, 2 digits>`
//! ([`conversation_lines`]).
//!
//! The CLI (`text --label`, `text --find`), the Audio page and the Scenes
//! dialogue read the file when it exists.
use crate::audio::{StemWindow, StreamAddress};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const LINKS_FILE: &str = "dialogue_links.json";
pub const SCHEMA_VERSION: u32 = 1;

/// The stream id of a wave name: the low 29 bits of its joaat.
pub fn wave_stream_id(name: &str) -> u32 {
    crate::meta7a::joaat(name) & 0x1FFF_FFFF
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speaker {
    /// Display label (character id).
    pub label: String,
    /// The game's speaker hash, where known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedAudio {
    #[serde(flatten)]
    pub address: StreamAddress,
    /// For a line inside an interleaved scene stem: the channel and frames.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stem: Option<StemWindow>,
    /// Wave name, kept only when it hashes to the stream id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wave: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueLink {
    /// Text label hash (the key in `sixng text` tables).
    pub subtitle_key: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<Speaker>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<LinkedAudio>,
    /// Subtitle text from the selected text tables, with its markup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DialogueLinks {
    pub schema_version: u32,
    pub links: Vec<DialogueLink>,
}

impl DialogueLinks {
    /// Links per subtitle key (a line can be said in several places).
    pub fn by_subtitle(&self) -> HashMap<u32, Vec<&DialogueLink>> {
        let mut m: HashMap<u32, Vec<&DialogueLink>> = HashMap::new();
        for l in &self.links {
            m.entry(l.subtitle_key).or_default().push(l);
        }
        m
    }

    /// Links of one archive per (bank offset, stream index).
    pub fn by_stream(&self, archive: &str) -> HashMap<(u64, usize), Vec<&DialogueLink>> {
        let mut m: HashMap<(u64, usize), Vec<&DialogueLink>> = HashMap::new();
        for l in &self.links {
            if let Some(a) = l.audio.as_ref().filter(|a| a.address.archive == archive) {
                m.entry((a.address.bank_offset, a.address.stream_index)).or_default().push(l);
            }
        }
        m
    }
}

pub fn links_path(work: &Path) -> PathBuf {
    work.join(LINKS_FILE)
}

pub fn parse(bytes: &[u8]) -> Result<DialogueLinks> {
    let file: DialogueLinks = serde_json::from_slice(bytes).context("read dialogue links")?;
    ensure!(file.schema_version == SCHEMA_VERSION, "unsupported dialogue-links schema {}", file.schema_version);
    Ok(file)
}

/// The links in `<work>`, or None when the file does not exist.
pub fn load(work: &Path) -> Result<Option<DialogueLinks>> {
    match std::fs::read(links_path(work)) {
        Ok(b) => parse(&b).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("open dialogue links"),
    }
}

pub fn save(path: &Path, links: &DialogueLinks) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    crate::atomic::write(path, serde_json::to_vec(links)?)?;
    Ok(())
}

/// One line for a subtitle in the CLI: speaker, then the audio address.
pub fn summary(l: &DialogueLink) -> serde_json::Value {
    let mut v = serde_json::json!({});
    if let Some(s) = &l.speaker {
        v["speaker"] = s.label.clone().into();
        if let Some(h) = s.hash {
            v["speaker_hash"] = format!("{h:08x}").into();
        }
    }
    if let Some(a) = &l.audio {
        let mut audio = serde_json::json!({
            "archive": a.address.archive, "bank_offset": format!("{:x}", a.address.bank_offset),
            "stream_index": a.address.stream_index, "stream_id": format!("{:08x}", a.address.stream_id),
        });
        if let Some(w) = &a.wave {
            audio["wave"] = w.clone().into();
        }
        if let Some(s) = a.stem {
            audio["stem"] = serde_json::json!({"channel_id": format!("{:08x}", s.channel_id), "start_frame": s.start_frame, "end_frame": s.end_frame});
        }
        v["audio"] = audio;
    }
    for (k, f) in [("mission", &l.mission), ("scene", &l.scene)] {
        if let Some(x) = f {
            v[k] = x.clone().into();
        }
    }
    v
}

/// Conversation fields without a known name (field hashes as `sixng pso`
/// prints them).
const F_ENTRIES: &str = "53c6b42d";
const F_LABEL: &str = "66fb3a21";
const F_CHARACTERS: &str = "827f53fd";
/// A line's voice: lines of one character mostly share it.
const F_VOICE: &str = "7f3eebb0";
/// A character's slot in its conversation (not its list position).
const F_SLOT: &str = "bd6915b4";
/// A line's speaker: the slot of the character who says it.
const F_SPEAKER_SLOT: &str = "59519816";

/// One spoken line of a conversation file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationLine {
    /// Subtitle label (`FX_INTRO_01`).
    pub label: String,
    /// `joaat(label)`: the key in the text tables.
    pub subtitle_key: u32,
    /// Audio cue (`FXAU_AAAB`).
    pub cue: String,
    pub variation: u32,
    /// `<cue>_<variation, 2 digits>`; its joaat's low 29 bits are the stream id.
    pub wave: String,
    /// The speaking character's hash, where the file pins it to one.
    pub speaker: Option<u32>,
    /// Why `speaker` is unset (one of the `GAP_*` reasons).
    pub speaker_gap: Option<&'static str>,
}

/// Reasons a line has no speaker.
pub const GAP_NO_CONVERSATION: &str = "line outside a conversation";
pub const GAP_NO_VOICE: &str = "several characters, line has no voice";
pub const GAP_SLOT_UNKNOWN: &str = "several characters, speaker slot matches none of them";
pub const GAP_VOICE_UNKNOWN: &str = "several characters, voice in no single-speaker conversation";
pub const GAP_VOICE_ELSEWHERE: &str = "several characters, voice belongs to none of them";
pub const GAP_VOICE_SHARED: &str = "several characters, voice used by more than one of them";

/// A field by name, or by the case-sensitive joaat of the name (as PSO
/// files hash field names) when the name
/// dictionary did not resolve it.
fn field<'a>(v: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    v.get(name).or_else(|| v.get(format!("{:08x}", crate::names::joaat_exact(name))))
}

fn u32_of(v: Option<&serde_json::Value>) -> Option<u32> {
    v.and_then(|x| x.as_u64()).and_then(|x| u32::try_from(x).ok())
}

/// Cheap test before decoding a PSO: conversation files declare the
/// `Context` field.
pub fn may_hold_conversations(bytes: &[u8]) -> bool {
    let h = crate::names::joaat_exact("Context").to_le_bytes();
    bytes.windows(4).any(|w| w == h)
}

struct RawLine {
    label: String,
    cue: String,
    variation: u32,
    voice: Option<u32>,
    /// The speaker's slot, where the line gives one.
    slot: Option<u32>,
    /// (slot, character) per character of the conversation.
    characters: Vec<(Option<u32>, u32)>,
}

fn walk_conversations(v: &serde_json::Value, characters: &[(Option<u32>, u32)], out: &mut Vec<RawLine>) {
    match v {
        serde_json::Value::Array(a) => a.iter().for_each(|x| walk_conversations(x, characters, out)),
        serde_json::Value::Object(o) => {
            let own: Vec<(Option<u32>, u32)>;
            let characters = match o.get(F_CHARACTERS).and_then(|c| c.as_array()) {
                Some(list) => {
                    own = list.iter().filter_map(|c| Some((u32_of(c.get(F_SLOT)), u32_of(field(c, "CharacterName"))?))).collect();
                    &own[..]
                }
                None => characters,
            };
            if let (Some(entries), Some(variation)) = (o.get(F_ENTRIES).and_then(|e| e.as_array()), u32_of(field(v, "Variation"))) {
                for e in entries {
                    let (Some(label), Some(cue)) = (e.get(F_LABEL).and_then(|x| x.as_str()), field(e, "Context").and_then(|x| x.as_str())) else { continue };
                    if label.is_empty() || cue.is_empty() || variation == 0 {
                        continue;
                    }
                    out.push(RawLine { label: label.to_string(), cue: cue.to_string(), variation, voice: u32_of(o.get(F_VOICE)), slot: u32_of(o.get(F_SPEAKER_SLOT)), characters: characters.to_vec() });
                }
            }
            for (k, x) in o {
                if k != F_ENTRIES {
                    walk_conversations(x, characters, out);
                }
            }
        }
        _ => {}
    }
}

/// The spoken lines of decoded conversation files (`pso::Pso::to_json`
/// values). Lines are found by shape, wherever they sit: a conversation has
/// a character list, a line a `Variation` and a list of entries, an entry a
/// `Context` and a label. A line's speaker is the conversation's only
/// character; in a conversation with several, it is the character in the
/// line's speaker slot or, for a line without one, the character whose
/// single-speaker conversations use the line's voice, when that is one of
/// them. Each (label, cue, take, speaker) is listed once, and a line found
/// inside a conversation is not listed again from outside one.
pub fn conversation_lines(files: &[serde_json::Value]) -> Vec<ConversationLine> {
    let mut raw = Vec::new();
    for f in files {
        walk_conversations(f, &[], &mut raw);
    }
    let distinct = |c: &[(Option<u32>, u32)]| {
        let mut c: Vec<u32> = c.iter().map(|x| x.1).collect();
        c.sort_unstable();
        c.dedup();
        c
    };
    let mut voices: HashMap<u32, std::collections::BTreeSet<u32>> = HashMap::new();
    for r in &raw {
        if let (Some(voice), [one]) = (r.voice, distinct(&r.characters).as_slice()) {
            voices.entry(voice).or_default().insert(*one);
        }
    }
    // PSO JSON lists objects both in place and flat: a line seen inside a
    // conversation drops its copies seen outside one.
    let in_conversation: std::collections::HashSet<(String, String, u32)> =
        raw.iter().filter(|r| !r.characters.is_empty()).map(|r| (r.label.to_ascii_lowercase(), r.cue.to_ascii_lowercase(), r.variation)).collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for r in raw {
        if r.characters.is_empty() && in_conversation.contains(&(r.label.to_ascii_lowercase(), r.cue.to_ascii_lowercase(), r.variation)) {
            continue;
        }
        let (speaker, speaker_gap) = match distinct(&r.characters).as_slice() {
            [] => (None, Some(GAP_NO_CONVERSATION)),
            [one] => (Some(*one), None),
            _ if r.slot.is_some() => match r.characters.iter().find(|c| c.0 == r.slot) {
                Some(c) => (Some(c.1), None),
                None => (None, Some(GAP_SLOT_UNKNOWN)),
            },
            many => match r.voice.map(|v| voices.get(&v)) {
                None => (None, Some(GAP_NO_VOICE)),
                Some(None) => (None, Some(GAP_VOICE_UNKNOWN)),
                Some(Some(s)) => match s.iter().copied().filter(|c| many.contains(c)).collect::<Vec<u32>>().as_slice() {
                    [one] => (Some(*one), None),
                    [] => (None, Some(GAP_VOICE_ELSEWHERE)),
                    _ => (None, Some(GAP_VOICE_SHARED)),
                },
            },
        };
        if !seen.insert((r.label.to_ascii_lowercase(), r.cue.to_ascii_lowercase(), r.variation, speaker)) {
            continue;
        }
        let wave = format!("{}_{:02}", r.cue, r.variation);
        out.push(ConversationLine { subtitle_key: crate::meta7a::joaat(&r.label), label: r.label, cue: r.cue, variation: r.variation, wave, speaker, speaker_gap });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(key: u32, bank: u64, index: usize) -> DialogueLink {
        DialogueLink {
            subtitle_key: key,
            speaker: Some(Speaker { label: "guide".into(), hash: Some(7) }),
            audio: Some(LinkedAudio {
                address: StreamAddress { archive: "fixture.rpf".into(), bank_offset: bank, stream_index: index, stream_id: 66, data_offset: bank + 128, data_size: 128 },
                stem: None,
                wave: Some("guide_hello_01".into()),
            }),
            text: Some("Hello.".into()),
            mission: None,
            scene: None,
        }
    }

    #[test]
    fn lookups_and_round_trip() -> Result<()> {
        let links = DialogueLinks { schema_version: SCHEMA_VERSION, links: vec![link(3, 4096, 1), link(3, 8192, 0), DialogueLink { audio: None, ..link(4, 0, 0) }] };
        let back = parse(&serde_json::to_vec(&links)?)?;
        assert_eq!(back.by_subtitle()[&3].len(), 2);
        assert!(back.by_subtitle()[&4][0].audio.is_none());
        let streams = back.by_stream("fixture.rpf");
        assert_eq!(streams.len(), 2);
        assert_eq!(streams[&(4096, 1)][0].subtitle_key, 3);
        assert!(back.by_stream("other.rpf").is_empty());
        let s = summary(&back.links[0]);
        assert_eq!(s["speaker"], "guide");
        assert_eq!(s["speaker_hash"], "00000007");
        assert_eq!(s["audio"]["bank_offset"], "1000");
        assert_eq!(s["audio"]["wave"], "guide_hello_01");
        assert!(parse(br#"{"schema_version":2,"links":[]}"#).is_err());
        Ok(())
    }

    fn by_label<'a>(lines: &'a [ConversationLine], label: &str) -> &'a ConversationLine {
        lines.iter().find(|l| l.label == label).unwrap()
    }

    #[test]
    fn conversation_lines_by_shape_with_speakers_from_slots_and_voices() {
        use serde_json::json;
        let line = |label: &str, cue: &str, var: u32, voice: u32| json!({"Variation": var, "7f3eebb0": voice,
            "53c6b42d": [{"66fb3a21": label, "Context": cue, "StartTime": 0}]});
        let slotted = |mut l: serde_json::Value, slot: u32| {
            l["59519816"] = json!(slot);
            l
        };
        let mut unresolved = json!({});
        unresolved[format!("{:08x}", crate::names::joaat_exact("CharacterName"))] = json!(100);
        let file = json!({"objects": {"x": [{"MissionId": 1, "list": [
            // one speaker; the field name left as its hash
            {"827f53fd": [unresolved],
             "cde324d0": [line("FX_INTRO_01", "FXAU_AAAB", 1, 7), line("FX_INTRO_02", "FXAU_AAAB", 2, 7)]},
            {"827f53fd": [{"CharacterName": 200}], "cde324d0": [line("FX_REPLY_01", "FXAU_AAAC", 1, 8)]},
            // two speakers: voices 7 and 8 name them; voice 9 is unknown
            {"827f53fd": [{"CharacterName": 100}, {"CharacterName": 200}], "cde324d0": [
                line("FX_TALK_1", "FXAU_AAAF", 1, 8), line("FX_TALK_2", "FXAU_AAAD", 1, 7), line("FX_TALK_3", "FXAU_AAAE", 3, 9)]},
            // speaker slots (not list positions) name the speaker, whatever the voice says
            {"827f53fd": [{"CharacterName": 100, "bd6915b4": 4}, {"CharacterName": 200, "bd6915b4": 1}], "cde324d0": [
                slotted(line("FX_SLOT_1", "FXAU_AAAH", 1, 7), 1), slotted(line("FX_SLOT_2", "FXAU_AAAI", 1, 8), 4),
                slotted(line("FX_SLOT_3", "FXAU_AAAJ", 1, 8), 2)]}
        ]}]}});
        // listed twice, and once more outside its conversation (PSO JSON
        // repeats objects): each line once, with its speaker
        let flat = json!({"objects": {"y": [line("FX_INTRO_01", "FXAU_AAAB", 1, 7)]}});
        let lines = conversation_lines(&[file.clone(), flat, file]);
        assert_eq!(lines.len(), 9);
        assert_eq!(by_label(&lines, "FX_SLOT_1").speaker, Some(200));
        assert_eq!(by_label(&lines, "FX_SLOT_2").speaker, Some(100));
        assert_eq!((by_label(&lines, "FX_SLOT_3").speaker, by_label(&lines, "FX_SLOT_3").speaker_gap), (None, Some(GAP_SLOT_UNKNOWN)));
        let by: HashMap<&str, &ConversationLine> = lines.iter().map(|l| (l.label.as_str(), l)).collect();
        assert_eq!(by["FX_INTRO_02"].wave, "FXAU_AAAB_02");
        assert_eq!(by["FX_INTRO_02"].subtitle_key, crate::meta7a::joaat("fx_intro_02"));
        assert_eq!(by["FX_INTRO_01"].speaker, Some(100));
        assert_eq!(by["FX_TALK_1"].speaker, Some(200));
        assert_eq!(by["FX_TALK_2"].speaker, Some(100));
        assert_eq!((by["FX_TALK_3"].speaker, by["FX_TALK_3"].speaker_gap), (None, Some(GAP_VOICE_UNKNOWN)));
        assert_eq!(by["FX_TALK_3"].wave, "FXAU_AAAE_03");
        // a line outside any conversation has no speaker
        let loose = json!({"objects": {"y": [line("FX_LOOSE_01", "FXAU_AAAG", 1, 7)]}});
        assert_eq!(conversation_lines(&[loose])[0].speaker_gap, Some(GAP_NO_CONVERSATION));
        assert!(may_hold_conversations(&crate::names::joaat_exact("Context").to_le_bytes()));
        assert!(!may_hold_conversations(b"none"));
    }

    #[test]
    fn wave_names_hash_to_29_bit_stream_ids() {
        let id = wave_stream_id("guide_hello_01");
        assert_eq!(id, crate::meta7a::joaat("guide_hello_01") & 0x1FFF_FFFF);
        assert!(id < 1 << 29);
        assert_eq!(wave_stream_id("GUIDE_HELLO_01"), id);
    }
}
