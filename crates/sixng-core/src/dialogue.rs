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

    #[test]
    fn wave_names_hash_to_29_bit_stream_ids() {
        let id = wave_stream_id("guide_hello_01");
        assert_eq!(id, crate::meta7a::joaat("guide_hello_01") & 0x1FFF_FFFF);
        assert!(id < 1 << 29);
        assert_eq!(wave_stream_id("GUIDE_HELLO_01"), id);
    }
}
