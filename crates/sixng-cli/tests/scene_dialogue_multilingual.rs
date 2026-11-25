#![cfg(feature = "opus-decode")]

use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sixng_core::audio::{self, AudioIndex};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::Command;

const ARCHIVE: &str = "synthetic.rpf";
const BANK_OFFSET: usize = 4096;
const STREAM_ID: u32 = 85;
const CHANNEL_IDS: [u32; 2] = [161, 178];
const SAMPLE_RATE: u16 = 48_000;
const BLOCK_BYTES: usize = 2048;
const PACKET_COUNT: u32 = 12;
const CHANNEL_FRAMES: u32 = PACKET_COUNT * 960;
const LOCALES: [&str; 5] = ["en", "es", "fr", "de", "pt"];

#[derive(Deserialize)]
struct Fixture {
    scene: String,
    speaker_label: String,
    subscenes_in_playback_order: Vec<String>,
    events: Vec<Event>,
    localized_strings: BTreeMap<String, BTreeMap<String, String>>,
    expected: Expected,
}

#[derive(Deserialize)]
struct Event {
    subscene: String,
    local_ms: u32,
    speaker_hashes: Vec<u32>,
    subtitle_key: u32,
    stem: Option<Value>,
}

#[derive(Deserialize)]
struct Expected {
    playback_clock_ms: Vec<u32>,
    review_subtitle_keys: Vec<u32>,
    rows: Vec<Value>,
}

/// Build the archive and index in scratch space so no audio binary is needed.
fn write_silence_carrier(root: &Path) -> Result<()> {
    // One 20 ms Opus silence packet per length prefix, repeated on each channel.
    const SILENCE_PACKET: [u8; 3] = [0xf8, 0xff, 0xfe];
    const ADAT_HEADER_BYTES: usize = 38;
    let channel_count = CHANNEL_IDS.len() as u32;

    let mut channel_block = Vec::new();
    for _ in 0..PACKET_COUNT {
        channel_block.extend_from_slice(&(SILENCE_PACKET.len() as u16).to_le_bytes());
        channel_block.extend_from_slice(&SILENCE_PACKET);
    }
    channel_block.resize(BLOCK_BYTES, 0);

    let mut data = Vec::new();
    for _ in CHANNEL_IDS {
        for field in [u32::MAX, 1, 0, CHANNEL_FRAMES, 0, 0] {
            data.extend_from_slice(&field.to_le_bytes());
        }
    }
    data.resize(BLOCK_BYTES, 0);
    for _ in CHANNEL_IDS {
        data.extend_from_slice(&channel_block);
    }

    let mut format = Vec::new();
    for field in [1, data.len() as u32, channel_count] {
        format.extend_from_slice(&field.to_le_bytes());
    }
    for channel_id in CHANNEL_IDS {
        format.extend_from_slice(&channel_id.to_le_bytes());
        format.extend_from_slice(&CHANNEL_FRAMES.to_le_bytes());
        format.extend_from_slice(&0_u16.to_le_bytes());
        format.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        format.extend_from_slice(&0_u32.to_le_bytes());
    }

    let data_offset = ADAT_HEADER_BYTES + format.len();
    let mut archive_bytes = vec![0; BANK_OFFSET];
    archive_bytes.extend_from_slice(b"ADAT");
    for field in [0xff01_0001_u32, 1, ADAT_HEADER_BYTES as u32] {
        archive_bytes.extend_from_slice(&field.to_le_bytes());
    }
    archive_bytes.extend_from_slice(&0_u16.to_le_bytes());
    archive_bytes.extend_from_slice(&((2 << 29) | STREAM_ID).to_le_bytes());
    for (tag, length, offset) in [
        (0x48_u64, format.len(), ADAT_HEADER_BYTES),
        (0x55_u64, data.len(), data_offset),
    ] {
        let chunk = (tag << 56) | ((length as u64) << 28) | offset as u64;
        archive_bytes.extend_from_slice(&chunk.to_le_bytes());
    }
    archive_bytes.extend_from_slice(&format);
    archive_bytes.extend_from_slice(&data);
    fs::create_dir(root.join("game"))?;
    fs::write(root.join("game").join(ARCHIVE), &archive_bytes)?;

    let index: AudioIndex = serde_json::from_value(json!({
        "version": audio::INDEX_VERSION,
        "archive": ARCHIVE,
        "archive_size": archive_bytes.len(),
        "note": "Synthetic two-channel Opus silence",
        "banks": [{
            "off": BANK_OFFSET,
            "size": archive_bytes.len() - BANK_OFFSET,
            "flags": 0,
            "streams": [[
                STREAM_ID, CHANNEL_FRAMES * channel_count, SAMPLE_RATE, 11,
                channel_count, 2, data_offset, data.len()
            ]]
        }]
    }))?;
    audio::save_index(&root.join("work"), &index)
}

/// Supply only resolved occurrences to the CLI, in the recorded playback order.
fn prepare_occurrences(fixture: &mut Fixture) -> Value {
    fixture.events.sort_by_key(|event| {
        fixture
            .subscenes_in_playback_order
            .iter()
            .position(|subscene| subscene == &event.subscene)
            .expect("every fixture event belongs to a listed subscene")
    });
    let mut turns = Vec::new();
    let mut playback_clock = Vec::new();
    let mut review = Vec::new();
    for event in &fixture.events {
        let [speaker_hash] = event.speaker_hashes.as_slice() else {
            review.push(event.subtitle_key);
            continue;
        };
        playback_clock.push(event.local_ms);
        let mut turn = json!({
            "scene": fixture.scene,
            "speaker": fixture.speaker_label,
            "speaker_hash": speaker_hash,
            "subtitle_key": event.subtitle_key
        });
        if let Some(stem) = &event.stem {
            turn["audio"] = json!({
                "archive": ARCHIVE, "bank_offset": BANK_OFFSET,
                "stream_index": 0, "stream_id": STREAM_ID, "stem": stem
            });
        }
        turns.push(turn);
    }
    assert_eq!(playback_clock, fixture.expected.playback_clock_ms);
    assert_eq!(review, fixture.expected.review_subtitle_keys);
    json!({ "schema_version": 1, "turns": turns })
}

/// Import one locale using the actual CLI and read both of its JSON outputs.
fn import_dialogue(
    executable: &OsStr,
    root: &Path,
    input: &Value,
    strings: &BTreeMap<String, String>,
) -> Result<(Value, Value)> {
    let turns = root.join("turns.json");
    let table = root.join("table.json");
    let output = root.join("dialogue.json");
    fs::write(&turns, serde_json::to_vec(input)?)?;
    fs::write(&table, serde_json::to_vec(&json!({ "strings": strings }))?)?;
    let result = Command::new(executable)
        .arg("scene-dialogue")
        .arg("--game")
        .arg(root.join("game"))
        .arg("--work")
        .arg(root.join("work"))
        .arg("--input")
        .arg(turns)
        .arg("--text-table")
        .arg(table)
        .arg("--out")
        .arg(&output)
        .arg("--quiet")
        .output()
        .context("run sixng scene-dialogue")?;
    ensure!(
        result.status.success(),
        "scene-dialogue exited with {}: {}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
    Ok((
        serde_json::from_slice(&fs::read(output)?)?,
        serde_json::from_slice(&result.stdout)?,
    ))
}

/// Compare every emitted row with its expected identity, text and optional audio.
fn check_dialogue(root: &Path) -> Result<()> {
    let executable = std::env::var_os("SIXNG_TEST_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_sixng").into());
    let mut fixture: Fixture =
        serde_json::from_str(include_str!("data/scene_dialogue_multilingual.json"))?;
    write_silence_carrier(root)?;
    let input = prepare_occurrences(&mut fixture);
    let expected_rows = fixture.expected.rows;
    assert_eq!(fixture.localized_strings.len(), LOCALES.len());
    for locale in LOCALES {
        let strings = &fixture.localized_strings[locale];
        let (actual, summary) = import_dialogue(&executable, root, &input, strings)?;
        let mut rows = expected_rows.clone();
        for row in &mut rows {
            let hash = row["subtitle_key"].as_u64().context("fixture subtitle key")?;
            row["text"] = json!(strings[&format!("{hash:08x}")]);
        }
        assert_eq!(
            actual,
            json!({ "schema_version": 1, "scenes": { (fixture.scene.as_str()): rows } }),
            "locale {locale}"
        );
        assert_eq!(summary["rows"], 3, "locale {locale}");
        assert_eq!(summary["audio_links"], 2, "locale {locale}");
        assert_eq!(summary["issues"], json!([]), "locale {locale}");
    }

    let mut invalid = input;
    invalid["turns"][0]["audio"]["stem"]["end_frame"] = json!(CHANNEL_FRAMES + 1);
    let (actual, summary) = import_dialogue(
        &executable,
        root,
        &invalid,
        &fixture.localized_strings["en"],
    )?;
    assert_eq!(summary["rows"], 3);
    assert_eq!(summary["audio_links"], 1);
    assert_eq!(summary["issues"].as_array().unwrap().len(), 1);
    assert_eq!(summary["issues"][0]["turn"], 1);
    assert!(summary["issues"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("exceeds the channel length"));
    assert!(actual["scenes"][fixture.scene.as_str()][0].get("audio").is_none());
    Ok(())
}

/// Exercise the same fixture in the cross-platform cargo test jobs.
#[test]
fn imports_synthetic_scene_dialogue_in_five_locales() -> Result<()> {
    let root = std::env::temp_dir().join(format!(
        "sixng-scene-dialogue-multilingual-{}",
        std::process::id()
    ));
    fs::create_dir(&root)?;
    let result = check_dialogue(&root);
    let cleanup = fs::remove_dir_all(&root);
    result.and_then(|()| Ok(cleanup?))
}
