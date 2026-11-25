use anyhow::{ensure, Context, Result};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use sixng_core::audio::{self, AudioIndex, StemWindow, StreamAddress};
use sixng_core::export::Workspace;
use sixng_core::game::ArchiveInfo;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Occurrences {
    schema_version: u32,
    turns: Vec<Turn>,
}

#[derive(Deserialize)]
struct Turn {
    scene: String,
    speaker: String,
    speaker_hash: u32,
    subtitle_key: u32,
    audio: Option<AudioTarget>,
}

#[derive(Deserialize)]
struct AudioTarget {
    archive: String,
    bank_offset: u64,
    stream_index: usize,
    stream_id: u32,
    stem: Option<StemWindow>,
}

#[derive(Deserialize)]
struct TextTable {
    strings: HashMap<String, String>,
}

struct AudioSource {
    archive: ArchiveInfo,
    index: AudioIndex,
    banks: HashMap<u64, usize>,
}

impl AudioSource {
    fn open(work: &Path, archive: &ArchiveInfo) -> Result<Self> {
        let index = audio::load_index(work, &archive.id)?;
        ensure!(index.archive == archive.id && index.archive_size == archive.size,
            "audio index does not match the archive");
        let banks: HashMap<_, _> = index.banks.iter().enumerate().map(|(i, b)| (b.off, i)).collect();
        ensure!(banks.len() == index.banks.len(), "duplicate bank offsets in audio index");
        Ok(Self { archive: archive.clone(), index, banks })
    }

    fn resolve(&self, target: &AudioTarget) -> Result<Value> {
        let &bank_index = self.banks.get(&target.bank_offset).context("bank not indexed")?;
        let bank = &self.index.banks[bank_index];
        let stream = bank.streams.get(target.stream_index).context("stream not indexed")?;
        let data_offset = bank.off.checked_add(u64::from(stream.data.0)).context("invalid data offset")?;
        let address = StreamAddress {
            archive: self.archive.id.clone(),
            bank_offset: bank.off,
            stream_index: target.stream_index,
            stream_id: target.stream_id,
            data_offset,
            data_size: stream.data.1,
        };
        ensure!(address.matches_bank(bank, self.index.archive_size), "stream ID or encoded range does not match");
        let mut output = serde_json::to_value(address)?;
        if let Some(window) = target.stem {
            ensure!(stream.layout == "music", "stem requires an interleaved stream");
            let bytes = audio::read_bank(&self.archive.path, bank)?;
            window.decode(&bytes, bank, stream)?;
            output["stem"] = serde_json::to_value(window)?;
        } else {
            ensure!(stream.exportable() && stream.layout != "music", "expected an individual audio stream");
        }
        Ok(output)
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    serde_json::from_reader(BufReader::new(file)).with_context(|| format!("read {}", path.display()))
}

fn write_index(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut file = sixng_core::atomic::AtomicFile::create(path)?;
    serde_json::to_writer(&mut file, value)?;
    writeln!(file)?;
    file.commit()?;
    Ok(())
}

pub fn generate(ws: &Workspace, input: &Path, tables: &[PathBuf], output: &Path) -> Result<Value> {
    let input: Occurrences = read_json(input)?;
    ensure!(input.schema_version == 1, "unsupported scene-occurrence schema");
    let wanted: HashSet<_> = input.turns.iter().map(|turn| turn.subtitle_key).collect();
    let mut texts: HashMap<u32, HashSet<String>> = HashMap::new();
    for path in tables {
        let table: TextTable = read_json(path)?;
        for (hash, text) in table.strings {
            let key = u32::from_str_radix(&hash, 16).with_context(|| format!("invalid text key in {}", path.display()))?;
            if wanted.contains(&key) && !text.trim().is_empty() {
                texts.entry(key).or_default().insert(text);
            }
        }
    }

    let archives: HashMap<_, _> = ws.archives().into_iter().map(|a| (a.id.clone(), a)).collect();
    let mut sources: HashMap<String, Result<AudioSource>> = HashMap::new();
    let mut scenes: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut issues = Vec::new();
    let mut rows = 0;
    let mut audio_links = 0;
    for (i, turn) in input.turns.into_iter().enumerate() {
        ensure!(turn.speaker_hash != 0 && !turn.speaker.trim().is_empty() && !turn.scene.trim().is_empty(),
            "turn {} needs a scene and a resolved speaker label and native hash", i + 1);
        let Some(variants) = texts.get(&turn.subtitle_key) else {
            issues.push(json!({"turn": i + 1, "reason": "subtitle not in the selected tables"}));
            continue;
        };
        if variants.len() != 1 {
            issues.push(json!({"turn": i + 1, "reason": "conflicting subtitle variants"}));
            continue;
        }
        let text = variants.iter().next().unwrap();
        let mut row = json!({"speaker": turn.speaker, "speaker_hash": turn.speaker_hash,
            "subtitle_key": turn.subtitle_key, "text": text});
        if let Some(target) = turn.audio {
            let source = sources.entry(target.archive.clone()).or_insert_with(|| {
                let archive = archives.get(&target.archive).context("archive ID not found")?;
                AudioSource::open(&ws.work, archive)
            });
            let resolved = match source {
                Ok(source) => source.resolve(&target),
                Err(error) => Err(anyhow::anyhow!("{error:#}")),
            };
            match resolved {
                Ok(address) => {
                    row["audio"] = address;
                    audio_links += 1;
                }
                Err(error) => issues.push(json!({"turn": i + 1, "reason": format!("{error:#}")})),
            }
        }
        scenes.entry(turn.scene).or_default().push(row);
        rows += 1;
    }
    ensure!(rows > 0, "no usable dialogue rows; check the occurrence input and selected text tables");
    write_index(output, &json!({"schema_version": 1, "scenes": scenes}))?;
    Ok(json!({"ok": true, "file": output, "scenes": scenes.len(), "rows": rows,
        "audio_links": audio_links, "issues": issues}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_local_tables_and_exact_streams_without_merging_occurrences() -> Result<()> {
        let root = std::env::temp_dir().join(format!("sixng-scene-dialogue-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let result = (|| {
            let ws = Workspace::new(root.join("game"), root.join("work"));
            fs::create_dir_all(&ws.game)?;
            sixng_core::atomic::write(ws.game.join("fixture.rpf"), [0; 8192])?;
            let index: AudioIndex = serde_json::from_value(json!({
                "version": audio::INDEX_VERSION, "archive": "fixture.rpf", "archive_size": 8192, "note": "",
                "banks": [{"off":4096,"size":768,"flags":0,"streams":[
                    [66,48,48000,0,1,4,128,128],
                    [66,240,48000,0,1,4,256,512]
                ]}]
            }))?;
            audio::save_index(&ws.work, &index)?;

            let input = root.join("turns.json");
            let one = root.join("one.json");
            let two = root.join("two.json");
            let output = ws.work.join("scene_dialogue.json");
            write_index(&one, &json!({"strings":{"00000003":"Come in.","00000004":"One variant."}}))?;
            write_index(&two, &json!({"strings":{"00000003":"Come in.","00000004":"Another variant."}}))?;
            write_index(&input, &json!({"schema_version":1,"turns":[
                {"scene":"fixture","speaker":"Guard","speaker_hash":1,"subtitle_key":3,
                 "audio":{"archive":"fixture.rpf","bank_offset":4096,"stream_index":1,"stream_id":66}},
                {"scene":"fixture","speaker":"Guard","speaker_hash":2,"subtitle_key":3,
                 "audio":{"archive":"fixture.rpf","bank_offset":4096,"stream_index":0,"stream_id":67}},
                {"scene":"fixture","speaker":"Guard","speaker_hash":2,"subtitle_key":4},
                {"scene":"fixture","speaker":"Guard","speaker_hash":2,"subtitle_key":5}
            ]}))?;
            let summary = generate(&ws, &input, &[one, two], &output)?;
            let saved: Value = read_json(&output)?;
            let rows = saved["scenes"]["fixture"].as_array().unwrap();
            assert_eq!(summary["rows"], 2);
            assert_eq!(summary["audio_links"], 1);
            assert_eq!(summary["issues"].as_array().unwrap().len(), 3);
            assert_eq!(rows[0]["text"], "Come in.");
            assert_eq!(rows[0]["audio"]["data_offset"], 4352);
            assert_eq!(rows[0]["audio"]["data_size"], 512);
            assert_eq!(rows[0]["speaker_hash"], 1);
            assert_eq!(rows[1]["speaker_hash"], 2);
            assert!(rows[1].get("audio").is_none());
            Ok(())
        })();
        let cleanup = fs::remove_dir_all(&root);
        result.and_then(|()| Ok(cleanup?))
    }
}
