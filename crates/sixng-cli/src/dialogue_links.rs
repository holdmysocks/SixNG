//! `sixng dialogue-links`: link subtitles to their speaker and audio stream
//! from a local address table, checked against the SixNG audio index.
use crate::scene_dialogue::{read_json, AudioSource, AudioTarget, TextTable};
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use sixng_core::audio::{StemWindow, StreamAddress};
use sixng_core::dialogue::{self, DialogueLink, DialogueLinks, LinkedAudio, Speaker};
use sixng_core::export::Workspace;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Speaker states that do not name one speaker.
const NO_SPEAKER: [&str; 3] = ["conflict", "unattributed", "no_verified_dialogue"];

struct Columns(HashMap<String, usize>);

impl Columns {
    fn new(header: &str) -> Result<Self> {
        let map: HashMap<_, _> = header.trim_end_matches('\r').split('\t').enumerate().map(|(i, c)| (c.trim().to_string(), i)).collect();
        for need in ["subtitle_key", "archive", "bank_offset", "stream_index", "stream_id_hex"] {
            ensure!(map.contains_key(need), "address table has no {need} column");
        }
        Ok(Self(map))
    }

    fn get<'a>(&self, row: &[&'a str], name: &str) -> &'a str {
        self.0.get(name).and_then(|&i| row.get(i)).map(|s| s.trim()).unwrap_or("")
    }
}

fn hex32(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
}

fn opt<T: std::str::FromStr>(s: &str, what: &str) -> Result<Option<T>> {
    if s.is_empty() {
        return Ok(None);
    }
    s.parse().map(Some).map_err(|_| anyhow::anyhow!("invalid {what} {s:?}"))
}

fn some(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

#[derive(Default)]
struct Tally {
    linked: usize,
    rejected: usize,
}

pub fn generate(ws: &Workspace, addresses: &Path, tables: &[PathBuf], output: &Path) -> Result<Value> {
    let raw = std::fs::read_to_string(addresses).with_context(|| format!("read {}", addresses.display()))?;
    let mut lines = raw.lines();
    let cols = Columns::new(lines.next().context("empty address table")?)?;
    let rows: Vec<Vec<&str>> = lines.filter(|l| !l.trim().is_empty()).map(|l| l.trim_end_matches('\r').split('\t').collect()).collect();

    let mut texts: HashMap<u32, HashSet<String>> = HashMap::new();
    if !tables.is_empty() {
        let wanted: HashSet<u32> = rows.iter().filter_map(|r| cols.get(r, "subtitle_key").rsplit('/').next().and_then(hex32)).collect();
        for path in tables {
            let table: TextTable = read_json(path)?;
            for (hash, text) in table.strings {
                let key = hex32(&hash).with_context(|| format!("invalid text key in {}", path.display()))?;
                if wanted.contains(&key) && !text.trim().is_empty() {
                    texts.entry(key).or_default().insert(text);
                }
            }
        }
    }

    let archives: HashMap<_, _> = ws.archives().into_iter().map(|a| (a.id.clone(), a)).collect();
    let mut sources: HashMap<String, Result<AudioSource, String>> = HashMap::new();
    let mut links = Vec::new();
    let mut reasons: BTreeMap<String, (usize, Vec<usize>)> = BTreeMap::new();
    let mut reject = |line: usize, why: String| {
        let e = reasons.entry(why).or_default();
        e.0 += 1;
        if e.1.len() < 10 {
            e.1.push(line);
        }
    };
    let (mut no_key, mut speakers, mut with_text, mut wave_mismatch) = (0, 0, 0, 0);
    let mut tally: BTreeMap<&str, Tally> = BTreeMap::new();
    for (i, r) in rows.iter().enumerate() {
        let line = i + 2;
        let Some(subtitle_key) = cols.get(r, "subtitle_key").rsplit('/').next().and_then(hex32) else {
            no_key += 1;
            continue;
        };
        let status = cols.get(r, "speaker_status");
        let label = cols.get(r, "character_id");
        let speaker = (!label.is_empty() && !NO_SPEAKER.contains(&status)).then(|| Speaker { label: label.to_string(), hash: hex32(cols.get(r, "speaker_hash_hex")) });
        speakers += speaker.is_some() as usize;

        let mut audio = None;
        let archive = cols.get(r, "archive");
        if !archive.is_empty() {
            let group = if cols.get(r, "confirmed") == "1" { "confirmed" } else { "unconfirmed" };
            let resolved = resolve_row(ws, &archives, &mut sources, &cols, r, archive);
            let t = tally.entry(group).or_default();
            match resolved {
                Ok((a, wave_ok)) => {
                    t.linked += 1;
                    wave_mismatch += !wave_ok as usize;
                    audio = Some(a);
                }
                Err(e) => {
                    t.rejected += 1;
                    reject(line, format!("{e:#}"));
                }
            }
        }
        let text = texts.get(&subtitle_key).filter(|v| v.len() == 1).and_then(|v| v.iter().next().cloned());
        with_text += text.is_some() as usize;
        if speaker.is_none() && audio.is_none() {
            continue;
        }
        links.push(DialogueLink {
            subtitle_key,
            speaker,
            audio,
            text,
            mission: some(cols.get(r, "mission_id")),
            scene: some(cols.get(r, "scene")),
        });
    }
    ensure!(!links.is_empty(), "no usable rows in the address table");
    let audio_links = links.iter().filter(|l| l.audio.is_some()).count();
    let count = links.len();
    dialogue::save(output, &DialogueLinks { schema_version: dialogue::SCHEMA_VERSION, links })?;
    let tally: BTreeMap<_, _> = tally.into_iter().map(|(k, t)| (k, json!({"linked": t.linked, "rejected": t.rejected}))).collect();
    let rejected: Vec<Value> = reasons.into_iter().map(|(why, (n, lines))| json!({"reason": why, "rows": n, "first_lines": lines})).collect();
    Ok(json!({"ok": true, "file": output, "rows": rows.len(), "links": count, "speakers": speakers,
        "audio_links": audio_links, "with_text": with_text, "rows_without_subtitle_key": no_key,
        "wave_names_dropped": wave_mismatch, "audio": tally, "rejected": rejected}))
}

/// Rebuild a row's audio address from the audio index. Returns the address
/// and whether the row's wave name (if any) hashes to the stream id.
fn resolve_row(
    ws: &Workspace,
    archives: &HashMap<String, sixng_core::game::ArchiveInfo>,
    sources: &mut HashMap<String, Result<AudioSource, String>>,
    cols: &Columns,
    r: &[&str],
    archive: &str,
) -> Result<(LinkedAudio, bool)> {
    let bank_offset: u64 = opt(cols.get(r, "bank_offset"), "bank offset")?.context("no bank offset")?;
    let stream_index: usize = opt(cols.get(r, "stream_index"), "stream index")?.context("no stream index")?;
    let stream_id = hex32(cols.get(r, "stream_id_hex")).context("no stream id")?;
    let stem = match hex32(cols.get(r, "channel_id_hex")) {
        Some(channel_id) => Some(StemWindow {
            channel_id,
            start_frame: opt(cols.get(r, "pcm_start_frame"), "start frame")?.context("stem without a start frame")?,
            end_frame: opt(cols.get(r, "pcm_end_frame_exclusive"), "end frame")?.context("stem without an end frame")?,
        }),
        None => None,
    };
    let source = sources.entry(archive.to_string()).or_insert_with(|| {
        let a = archives.get(archive).context("archive not in the install").map_err(|e| format!("{e:#}"))?;
        AudioSource::open(&ws.work, a).map_err(|e| format!("{e:#}"))
    });
    let source = match source {
        Ok(s) => s,
        Err(e) => bail!("{e}"),
    };
    let target = AudioTarget { archive: archive.to_string(), bank_offset, stream_index, stream_id, stem };
    let value = source.resolve(&target)?;
    let address: StreamAddress = serde_json::from_value(value)?;
    // The table's byte range is only a cross-check of the index's.
    let data_offset: Option<u64> = opt(cols.get(r, "data_offset"), "data offset")?;
    let data_size: Option<u32> = opt(cols.get(r, "data_size"), "data size")?;
    if data_offset.is_some_and(|o| o != address.data_offset) || data_size.is_some_and(|s| s != address.data_size) {
        bail!("encoded range differs from the audio index");
    }
    let clip = cols.get(r, "clip_name");
    let wave_ok = clip.is_empty() || dialogue::wave_stream_id(clip) == stream_id;
    let wave = (!clip.is_empty() && wave_ok).then(|| clip.to_string());
    Ok((LinkedAudio { address, stem, wave }, wave_ok))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sixng_core::audio::{self, AudioIndex};

    const HEADER: &str = "game_build\tsubtitle_key\tcharacter_id\tspeaker_hash_hex\tspeaker_status\tlink_status\tmission_id\tscene\tarchive\tnested_archive\tbank_name\tbank_cue\tbank_offset\tbank_size\tstream_index\tstream_id_hex\tclip_name\tdata_offset\tdata_size\tchannel_id_hex\tchannel_descriptor_index\tpcm_start_frame\tpcm_end_frame_exclusive\tsample_rate\tencoded_sha256\twav_sha256\tconfirmed";

    fn row(key: &str, speaker: &str, hash: &str, status: &str, bank: &str, index: &str, id: u32, clip: &str, range: (&str, &str), confirmed: &str) -> String {
        let id = if bank.is_empty() { String::new() } else { format!("0x{id:x}") };
        let archive = if bank.is_empty() { "" } else { "fixture.rpf" };
        format!("test\t{key}\t{speaker}\t{hash}\t{status}\tx\tMISSION\t\t{archive}\t\t\t\t{bank}\t768\t{index}\t{id}\t{clip}\t{}\t{}\t\t\t\t\t48000\t\t\t{confirmed}", range.0, range.1)
    }

    #[test]
    fn links_rows_through_the_audio_index_and_rejects_stale_addresses() -> Result<()> {
        let root = std::env::temp_dir().join(format!("sixng-dialogue-links-{}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let result = (|| {
            let ws = Workspace::new(root.join("game"), root.join("work"));
            std::fs::create_dir_all(&ws.game)?;
            sixng_core::atomic::write(ws.game.join("fixture.rpf"), [0; 8192])?;
            let hello = dialogue::wave_stream_id("guide_hello_01");
            let bye = dialogue::wave_stream_id("guide_bye_01");
            let index: AudioIndex = serde_json::from_value(json!({
                "version": audio::INDEX_VERSION, "archive": "fixture.rpf", "archive_size": 8192, "note": "",
                "banks": [{"off":4096,"size":768,"flags":0,"streams":[
                    [hello,48,48000,0,1,4,128,128],
                    [bye,240,48000,0,1,4,256,512]
                ]}]
            }))?;
            audio::save_index(&ws.work, &index)?;
            let rows = [
                // confirmed, range matches the index
                row("Core/C0001/00000003", "guide", "0x7", "verified", "4096", "0", hello, "guide_hello_01", ("4224", "128"), "1"),
                // no range in the table: taken from the index; wrong wave name dropped
                row("Core/C0001/00000004", "guide", "", "verified", "4096", "1", bye, "guide_hello_01", ("", ""), "0"),
                // stale range
                row("Core/C0001/00000005", "guide", "", "verified", "4096", "1", bye, "", ("4352", "500"), "0"),
                // stream id does not match the stream at that position
                row("Core/C0001/00000006", "guide", "", "verified", "4096", "0", bye, "", ("", ""), "1"),
                // caption only, conflicting speaker: nothing to link
                row("Core/C0001/00000007", "guide", "", "conflict", "", "", 0, "", ("", ""), "0"),
                // caption only with a speaker
                row("Core/C0001/00000008", "visitor", "", "caption", "", "", 0, "", ("", ""), "0"),
                // no subtitle key
                row("", "guide", "", "no_verified_dialogue", "", "", 0, "", ("", ""), "0"),
            ];
            let tsv = root.join("addresses.tsv");
            std::fs::write(&tsv, format!("{HEADER}\n{}\n", rows.join("\n")))?;
            let table = root.join("en.json");
            std::fs::write(&table, r#"{"strings":{"00000003":"Hello.","00000008":"Who is there?"}}"#)?;
            let out = ws.work.join(dialogue::LINKS_FILE);
            let summary = generate(&ws, &tsv, &[table], &out)?;
            assert_eq!(summary["rows"], 7);
            assert_eq!(summary["links"], 5);
            assert_eq!(summary["audio_links"], 2);
            assert_eq!(summary["rows_without_subtitle_key"], 1);
            assert_eq!(summary["wave_names_dropped"], 1);
            assert_eq!(summary["audio"]["confirmed"]["linked"], 1);
            assert_eq!(summary["audio"]["confirmed"]["rejected"], 1);
            assert_eq!(summary["audio"]["unconfirmed"]["linked"], 1);
            assert_eq!(summary["audio"]["unconfirmed"]["rejected"], 1);
            assert_eq!(summary["rejected"].as_array().unwrap().len(), 2);

            let saved = dialogue::load(&ws.work)?.unwrap();
            let by = saved.by_subtitle();
            let hello_link = by[&3][0];
            assert_eq!(hello_link.speaker, Some(Speaker { label: "guide".into(), hash: Some(7) }));
            assert_eq!(hello_link.text.as_deref(), Some("Hello."));
            let a = hello_link.audio.as_ref().unwrap();
            assert_eq!((a.address.data_offset, a.address.data_size), (4224, 128));
            assert_eq!(a.wave.as_deref(), Some("guide_hello_01"));
            let bye_link = by[&4][0].audio.as_ref().unwrap();
            assert_eq!((bye_link.address.data_offset, bye_link.address.data_size), (4352, 512));
            assert!(bye_link.wave.is_none());
            // a rejected address keeps the speaker; a conflicting speaker with no audio is left out
            assert!(by[&5][0].audio.is_none() && by[&6][0].audio.is_none());
            assert_eq!(by[&5][0].speaker.as_ref().unwrap().label, "guide");
            assert!(!by.contains_key(&7));
            assert!(by[&8][0].audio.is_none());
            assert_eq!(by[&8][0].text.as_deref(), Some("Who is there?"));
            assert_eq!(saved.by_stream("fixture.rpf")[&(4096, 1)][0].subtitle_key, 4);
            Ok(())
        })();
        let cleanup = std::fs::remove_dir_all(&root);
        result.and_then(|()| Ok(cleanup?))
    }
}
