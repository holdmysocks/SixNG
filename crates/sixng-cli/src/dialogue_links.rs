//! `sixng dialogue-links`: link subtitles to their speaker and audio stream
//! from the game's conversation files and/or a local address table, checked
//! against the SixNG audio index.
use crate::scene_dialogue::{read_json, AudioSource, AudioTarget, TextTable};
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use sixng_core::audio::{StemWindow, StreamAddress};
use sixng_core::audiometa::{BankContainers, Voices};
use sixng_core::dialogue::{self, DialogueLink, DialogueLinks, LinkedAudio, Speaker};
use sixng_core::export::Workspace;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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

/// Links from the conversation files of `archive`: the subtitle key, the
/// speaker and the wave come from the files; the stream is the one stream
/// with the wave's id in the archive's audio index or, when that has none,
/// in the first other indexed archive that has it.
fn derive(ws: &Workspace, archive: &str, sources: &mut HashMap<String, Result<AudioSource, String>>, reject: &mut impl FnMut(String, String)) -> Result<(Vec<DialogueLink>, Value)> {
    let a = ws.resolve(archive)?;
    let (lines, n_files) = {
        let ix = ws.load_index(&a)?;
        let mut files = Vec::new();
        for e in ix.entries.iter().filter(|e| e.kind == sixng_core::classify::Kind::Pso) {
            let Ok(bytes) = sixng_core::scan::read_entry(&a.path, e) else { continue };
            if !dialogue::may_hold_conversations(&bytes) {
                continue;
            }
            if let Some(p) = sixng_core::pso::parse(&bytes) {
                files.push(p.to_json());
            }
        }
        (dialogue::conversation_lines(&files), files.len())
    };
    // the archive's own audio first, then every other archive with an audio index
    let mut order = vec![a.clone()];
    order.extend(ws.archives().into_iter().filter(|o| o.id != a.id && sixng_core::audio::index_path(&ws.work, &o.id).is_file()));
    for o in &order {
        sources.entry(o.id.clone()).or_insert_with(|| AudioSource::open(&ws.work, o).map_err(|e| format!("{e:#}")));
    }
    if let Some(Err(e)) = sources.get(&a.id) {
        bail!("{e}");
    }
    let mut usable = Vec::new();
    let (mut containers, mut voices) = (Vec::new(), Vec::new());
    for o in &order {
        if let Some(Ok(s)) = sources.get(&o.id) {
            usable.push(s);
            containers.push(sixng_core::audiometa::load_containers(&ws.work, &o.id).unwrap_or_default());
            voices.push(sixng_core::audiometa::load_voices(&ws.work, &o.id).unwrap_or_default());
        }
    }
    let (links, audio, by_voice, speakers) = link_lines(&lines, &usable, &containers, &voices, reject);
    let mut gaps: BTreeMap<&str, usize> = BTreeMap::new();
    for l in &lines {
        if let Some(g) = l.speaker_gap {
            *gaps.entry(g).or_default() += 1;
        }
    }
    let summary = json!({"archive": a.id, "conversation_files": n_files, "lines": lines.len(), "audio": audio, "audio_from_voice_banks": by_voice, "speakers": speakers, "no_speaker": gaps});
    Ok((links, summary))
}

/// A wave in several banks goes to its speaker's voice bank. First the
/// speaker's speech voice (`audio-meta`: a voice named like the character
/// says which container holds each context's takes): the one candidate in
/// that container. Else the candidate bank whose container holds at least
/// this many of the speaker's lines whose wave is in one bank only, while
/// no other candidate holds any.
const VOICE_BANK_SUPPORT: usize = 2;

/// Links for conversation lines: each wave's stream, from the first source
/// (archive audio index) that has the wave's id, when that source has
/// exactly one such stream or the speaker's voice bank picks one
/// (`containers` and `voices`: each source's bank containers and speech
/// voices, from `audio-meta`). Returns the links, audio links per archive,
/// how many of them each voice-bank rule picked, and the number with a
/// speaker.
fn link_lines(
    lines: &[dialogue::ConversationLine],
    sources: &[&AudioSource],
    containers: &[BankContainers],
    voices: &[Voices],
    reject: &mut impl FnMut(String, String),
) -> (Vec<DialogueLink>, BTreeMap<String, usize>, BTreeMap<&'static str, usize>, usize) {
    let by_id: Vec<HashMap<u32, Vec<(u64, usize)>>> = sources
        .iter()
        .map(|s| {
            let mut m: HashMap<u32, Vec<(u64, usize)>> = HashMap::new();
            for b in &s.index.banks {
                for (i, st) in b.streams.iter().enumerate().filter(|(_, st)| st.layout != "none") {
                    m.entry(st.id).or_default().push((b.off, i));
                }
            }
            m
        })
        .collect();
    // (source, its streams with the line's wave id), per line
    let hits: Vec<Option<(usize, &[(u64, usize)])>> = lines
        .iter()
        .map(|l| {
            let id = dialogue::wave_stream_id(&l.wave);
            by_id.iter().enumerate().find_map(|(i, m)| m.get(&id).map(|v| (i, v.as_slice())))
        })
        .collect();
    let bank_containers = |source: usize, bank: u64| containers.get(source).and_then(|c| c.get(&bank)).into_iter().flat_map(|m| m.keys().copied());
    // per (speaker, source, container): the speaker's lines with a wave in that one bank only
    let mut support: HashMap<(u32, usize, u32), usize> = HashMap::new();
    for (l, hit) in lines.iter().zip(&hits) {
        if let (Some(speaker), Some((source, &[(bank, _)]))) = (l.speaker, hit) {
            for c in bank_containers(*source, bank) {
                *support.entry((speaker, *source, c)).or_default() += 1;
            }
        }
    }
    // the speaker's speech voice: the candidate in the container of the wave's context
    let speech_voice = |speaker: Option<u32>, source: usize, wave: &str, candidates: &[(u64, usize)]| -> Option<(u64, usize)> {
        let (ctx, _) = wave.rsplit_once('_')?;
        let (container, _) = *voices.get(source)?.get(&speaker?)?.get(&sixng_core::audiometa::context_hash(ctx))?;
        let mut hit = candidates.iter().filter(|&&(bank, _)| bank_containers(source, bank).any(|c| c == container));
        match (hit.next(), hit.next()) {
            (Some(&one), None) => Some(one),
            _ => None,
        }
    };
    let voice_bank = |speaker: Option<u32>, source: usize, candidates: &[(u64, usize)]| -> Option<(u64, usize)> {
        let speaker = speaker?;
        let score: Vec<usize> = candidates.iter().map(|&(bank, _)| bank_containers(source, bank).map(|c| support.get(&(speaker, source, c)).copied().unwrap_or(0)).max().unwrap_or(0)).collect();
        let best = score.iter().copied().max()?;
        (best >= VOICE_BANK_SUPPORT && score.iter().filter(|&&s| s > 0).count() == 1).then(|| candidates[score.iter().position(|&s| s == best).unwrap()])
    };
    let own = sources.first().map(|s| s.index.archive.clone()).unwrap_or_default();
    let mut links = Vec::new();
    let mut audio: BTreeMap<String, usize> = BTreeMap::new();
    let mut speakers = 0;
    let mut by_voice: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (l, hit) in lines.iter().zip(&hits) {
        let id = dialogue::wave_stream_id(&l.wave);
        let picked = match *hit {
            Some((source, &[one])) => Ok((source, one)),
            Some((source, several)) => match (speech_voice(l.speaker, source, &l.wave, several), voice_bank(l.speaker, source, several)) {
                (Some(one), _) => {
                    *by_voice.entry("speech_voice").or_default() += 1;
                    Ok((source, one))
                }
                (None, Some(one)) => {
                    *by_voice.entry("speaker_lines").or_default() += 1;
                    Ok((source, one))
                }
                (None, None) => Err(anyhow::anyhow!("wave id in several banks")),
            },
            None => Err(anyhow::anyhow!("wave in no audio index")),
        };
        let found = picked.and_then(|(source, (bank_offset, stream_index))| {
            let source = sources[source];
            let target = AudioTarget { archive: source.index.archive.clone(), bank_offset, stream_index, stream_id: id, stem: None };
            source.resolve(&target).and_then(|v| Ok(serde_json::from_value::<StreamAddress>(v)?))
        });
        let audio_link = match found {
            Ok(address) => {
                *audio.entry(address.archive.clone()).or_default() += 1;
                Some(LinkedAudio { address, stem: None, wave: Some(l.wave.clone()) })
            }
            Err(e) => {
                reject(format!("{own}: {} ({})", l.label, l.wave), format!("conversation line: {e:#}"));
                None
            }
        };
        speakers += l.speaker.is_some() as usize;
        links.push(DialogueLink {
            subtitle_key: l.subtitle_key,
            speaker: l.speaker.map(|h| Speaker { label: format!("{h:08x}"), hash: Some(h) }),
            audio: audio_link,
            text: None,
            mission: None,
            scene: None,
        });
    }
    (links, audio, by_voice, speakers)
}

/// Build `dialogue_links.json` from the conversation files of `conversations`
/// (archives) and from a local address table. A table row for a subtitle
/// that the conversation files already link is compared with it and only
/// fills a stream they give none.
pub fn generate(ws: &Workspace, addresses: Option<&Path>, conversations: &[String], tables: &[PathBuf], output: &Path) -> Result<Value> {
    ensure!(addresses.is_some() || !conversations.is_empty(), "give --addresses, --conversations or both");
    let archives: HashMap<_, _> = ws.archives().into_iter().map(|a| (a.id.clone(), a)).collect();
    let mut sources: HashMap<String, Result<AudioSource, String>> = HashMap::new();
    let mut reasons: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    let mut reject = |at: String, why: String| {
        let e = reasons.entry(why).or_default();
        e.0 += 1;
        if e.1.len() < 10 {
            e.1.push(at);
        }
    };

    let mut links = Vec::new();
    let mut derived = Vec::new();
    for archive in conversations {
        let (l, summary) = derive(ws, archive, &mut sources, &mut reject)?;
        links.extend(l);
        derived.push(summary);
    }
    let derived_keys: HashMap<u32, Vec<usize>> = links.iter().enumerate().fold(HashMap::new(), |mut m, (i, l)| {
        m.entry(l.subtitle_key).or_default().push(i);
        m
    });
    // every character hash the conversation files use: a table speaker
    // hash outside it is the table's own ID, not a game character
    let characters: HashSet<u32> = links.iter().filter_map(|l| l.speaker.as_ref().and_then(|s| s.hash)).collect();

    let raw = match addresses {
        Some(path) => std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?,
        None => String::new(),
    };
    let mut table_lines = raw.lines();
    let header = table_lines.next();
    ensure!(addresses.is_none() || header.is_some(), "empty address table");
    let cols = header.map(Columns::new).transpose()?;
    let rows: Vec<Vec<&str>> = table_lines.filter(|l| !l.trim().is_empty()).map(|l| l.trim_end_matches('\r').split('\t').collect()).collect();

    let (mut no_key, mut wave_mismatch) = (0, 0);
    let mut table_names: Vec<String> = Vec::new();
    let mut tally: BTreeMap<&str, Tally> = BTreeMap::new();
    let mut check: BTreeMap<&'static str, usize> = BTreeMap::new();
    if let Some(cols) = &cols {
        for (i, r) in rows.iter().enumerate() {
            let line = (i + 2).to_string();
            let Some(subtitle_key) = cols.get(r, "subtitle_key").rsplit('/').next().and_then(hex32) else {
                no_key += 1;
                continue;
            };
            let status = cols.get(r, "speaker_status");
            let label = cols.get(r, "character_id");
            if !label.is_empty() {
                table_names.push(label.to_string());
            }
            let speaker = (!label.is_empty() && !NO_SPEAKER.contains(&status)).then(|| Speaker { label: label.to_string(), hash: hex32(cols.get(r, "speaker_hash_hex")) });

            let mut audio = None;
            let archive = cols.get(r, "archive");
            if !archive.is_empty() {
                let group = if cols.get(r, "confirmed") == "1" { "confirmed" } else { "unconfirmed" };
                let resolved = resolve_row(ws, &archives, &mut sources, cols, r, archive);
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
            if let Some(found) = derived_keys.get(&subtitle_key) {
                // the conversation files decide; the table checks them and
                // fills a stream they have none for
                check_row(&mut links, found, &characters, audio, speaker.as_ref(), &mut check);
                continue;
            }
            if speaker.is_none() && audio.is_none() {
                continue;
            }
            links.push(DialogueLink { subtitle_key, speaker, audio, text: None, mission: some(cols.get(r, "mission_id")), scene: some(cols.get(r, "scene")) });
        }
    }
    ensure!(!links.is_empty(), "nothing to link");

    let mut namer = Namer::new(&links);
    for h in namer.wanted.clone() {
        if let Some(n) = sixng_core::names::lookup(h, sixng_core::names::Ctx::Audio) {
            namer.offer("names", &n);
        }
    }
    if let Ok(models) = sixng_core::pedlook::model_names(&ws.game) {
        for (h, n) in &models.names {
            if !models.uncertain.contains(h) {
                namer.offer_parts("ped_models", n);
            }
        }
        for n in models.display.values() {
            namer.offer_words("ped_models", n);
        }
    }
    for n in &table_names {
        namer.offer("address_table", n);
    }
    if !tables.is_empty() {
        let wanted: HashSet<u32> = links.iter().map(|l| l.subtitle_key).collect();
        let mut texts: HashMap<u32, HashSet<String>> = HashMap::new();
        for path in tables {
            let table: TextTable = read_json(path)?;
            for (hash, text) in table.strings {
                namer.offer_words("subtitles", &text);
                let key = hex32(&hash).with_context(|| format!("invalid text key in {}", path.display()))?;
                if wanted.contains(&key) && !text.trim().is_empty() {
                    texts.entry(key).or_default().insert(text);
                }
            }
        }
        for l in &mut links {
            l.text = texts.get(&l.subtitle_key).filter(|v| v.len() == 1).and_then(|v| v.iter().next().cloned());
        }
    }
    let speaker_names = namer.apply(&mut links);
    let audio_links = links.iter().filter(|l| l.audio.is_some()).count();
    let speakers = links.iter().filter(|l| l.speaker.is_some()).count();
    let with_text = links.iter().filter(|l| l.text.is_some()).count();
    let count = links.len();
    dialogue::save(output, &DialogueLinks { schema_version: dialogue::SCHEMA_VERSION, links })?;
    let tally: BTreeMap<_, _> = tally.into_iter().map(|(k, t)| (k, json!({"linked": t.linked, "rejected": t.rejected}))).collect();
    let rejected: Vec<Value> = reasons.into_iter().map(|(why, (n, at))| json!({"reason": why, "count": n, "first": at})).collect();
    Ok(json!({"ok": true, "file": output, "conversations": derived, "rows": rows.len(), "links": count, "speakers": speakers, "speaker_names": speaker_names,
        "audio_links": audio_links, "with_text": with_text, "rows_without_subtitle_key": no_key,
        "wave_names_dropped": wave_mismatch, "audio": tally, "table_check": check, "rejected": rejected}))
}

/// Names for speakers known only by hash: a candidate names a speaker when
/// the joaat of its lower-case spelling is the speaker's hash, and a speaker
/// gets a name only when one distinct name matches it.
struct Namer {
    wanted: HashSet<u32>,
    /// hash -> (names, first source that named it)
    found: HashMap<u32, (BTreeSet<String>, &'static str)>,
}

impl Namer {
    fn new(links: &[DialogueLink]) -> Self {
        let wanted = links.iter().filter_map(|l| l.speaker.as_ref()).filter_map(|s| s.hash.filter(|h| s.label == format!("{h:08x}"))).collect();
        Self { wanted, found: HashMap::new() }
    }

    fn offer(&mut self, source: &'static str, name: &str) {
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() || name.contains(':') {
            return;
        }
        let h = sixng_core::meta7a::joaat(&name);
        if self.wanted.contains(&h) {
            self.found.entry(h).or_insert_with(|| (BTreeSet::new(), source)).0.insert(name);
        }
    }

    /// A model name and its parts (`cs_bret_vogel`: `bret_vogel`, `vogel`,
    /// `cs_bret`, ...), without a trailing number.
    fn offer_parts(&mut self, source: &'static str, model: &str) {
        let model = model.to_ascii_lowercase();
        let base = model.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end_matches('_');
        for m in [model.as_str(), base] {
            let parts: Vec<&str> = m.split('_').collect();
            for i in 0..parts.len() {
                self.offer(source, &parts[i..].join("_"));
                self.offer(source, &parts[..=i].join("_"));
            }
        }
    }

    /// Words of a text and runs of two or three of them joined by `_`
    /// ("Raul Bautista" -> `raul`, `bautista`, `raul_bautista`), with and
    /// without apostrophes and dots.
    fn offer_words(&mut self, source: &'static str, text: &str) {
        let plain = sixng_core::text::plain(text);
        const MARKS: [char; 3] = ['\'', '.', '-'];
        let words: Vec<&str> = plain.split(|c: char| !(c.is_ascii_alphabetic() || MARKS.contains(&c))).map(|w| w.trim_matches(&MARKS[..])).filter(|w| !w.is_empty()).collect();
        for i in 0..words.len() {
            for k in 1..=3.min(words.len() - i) {
                let joined = words[i..i + k].join("_");
                self.offer(source, &joined);
                if joined.contains(['\'', '.']) {
                    self.offer(source, &joined.replace(['\'', '.'], ""));
                }
            }
        }
    }

    /// Name the speakers; report named speakers and lines per source and
    /// the speakers several names match.
    fn apply(&self, links: &mut [DialogueLink]) -> Value {
        let mut speakers: BTreeMap<&str, usize> = BTreeMap::new();
        let mut lines: BTreeMap<&str, usize> = BTreeMap::new();
        for (names, source) in self.found.values() {
            if names.len() == 1 {
                *speakers.entry(source).or_default() += 1;
            }
        }
        for l in links.iter_mut() {
            let Some(s) = l.speaker.as_mut() else { continue };
            let Some(h) = s.hash.filter(|h| s.label == format!("{h:08x}")) else { continue };
            if let Some((names, source)) = self.found.get(&h).filter(|(n, _)| n.len() == 1) {
                s.label = names.iter().next().unwrap().clone();
                *lines.entry(source).or_default() += 1;
            }
        }
        let several = self.found.values().filter(|(n, _)| n.len() > 1).count();
        json!({"speakers": self.wanted.len(), "named": speakers, "lines_named": lines, "several_names": several})
    }
}

/// Compare a table row with the links derived for its subtitle (`found`,
/// indexes into `links`): a stream agrees or differs, or fills derived
/// links that have none; a speaker hash agrees, differs, or is an ID the
/// conversation files never use for a character (`characters`).
fn check_row(links: &mut [DialogueLink], found: &[usize], characters: &HashSet<u32>, audio: Option<LinkedAudio>, speaker: Option<&Speaker>, check: &mut BTreeMap<&'static str, usize>) {
    if let Some(a) = audio {
        if found.iter().all(|&i| links[i].audio.is_none()) {
            for &i in found {
                links[i].audio = Some(a.clone());
            }
            *check.entry("audio_filled").or_default() += 1;
        } else {
            let same = found.iter().any(|&i| links[i].audio.as_ref().is_some_and(|d| d.address.archive == a.address.archive && d.address.bank_offset == a.address.bank_offset && d.address.stream_index == a.address.stream_index));
            *check.entry(if same { "audio_agrees" } else { "audio_differs" }).or_default() += 1;
        }
    }
    if let Some(h) = speaker.and_then(|s| s.hash) {
        let ours: Vec<u32> = found.iter().filter_map(|&i| links[i].speaker.as_ref().and_then(|s| s.hash)).collect();
        if !ours.is_empty() {
            let k = if ours.contains(&h) {
                "speaker_agrees"
            } else if characters.contains(&h) {
                "speaker_differs"
            } else {
                "speaker_id_not_in_files"
            };
            *check.entry(k).or_default() += 1;
        }
    }
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
            let summary = generate(&ws, Some(&tsv), &[], &[table], &out)?;
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
            assert_eq!(summary["rejected"][0]["count"], 1);

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

    #[test]
    fn conversation_lines_link_to_their_one_stream() -> Result<()> {
        let root = std::env::temp_dir().join(format!("sixng-dialogue-conv-{}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let result = (|| {
            let ws = Workspace::new(root.join("game"), root.join("work"));
            std::fs::create_dir_all(&ws.game)?;
            sixng_core::atomic::write(ws.game.join("fixture.rpf"), [0; 8192])?;
            sixng_core::atomic::write(ws.game.join("other.rpf"), [0; 4096])?;
            let one = dialogue::wave_stream_id("FXAU_AAAB_01");
            let twice = dialogue::wave_stream_id("FXAU_AAAC_01");
            let elsewhere = dialogue::wave_stream_id("FXAU_AAAE_01");
            let (v1, v2, w1) = (dialogue::wave_stream_id("FXAU_AAAV_01"), dialogue::wave_stream_id("FXAU_AAAW_01"), dialogue::wave_stream_id("FXAU_AAAX_01"));
            let index: AudioIndex = serde_json::from_value(json!({
                "version": audio::INDEX_VERSION, "archive": "fixture.rpf", "archive_size": 8192, "note": "",
                "banks": [{"off":4096,"size":768,"flags":0,"streams":[[one,48,48000,0,1,4,128,128],[twice,48,48000,0,1,4,256,128],[w1,48,48000,0,1,4,384,128]]},
                          {"off":6144,"size":768,"flags":0,"streams":[[twice,48,48000,0,1,4,128,128],[v1,48,48000,0,1,4,256,128],[v2,48,48000,0,1,4,384,128]]}]
            }))?;
            audio::save_index(&ws.work, &index)?;
            // another archive has the first wave too (the own archive wins) and one wave the first lacks
            let other: AudioIndex = serde_json::from_value(json!({
                "version": audio::INDEX_VERSION, "archive": "other.rpf", "archive_size": 4096, "note": "",
                "banks": [{"off":1024,"size":768,"flags":0,"streams":[[one,48,48000,0,1,4,128,128],[elsewhere,48,48000,0,1,4,256,128]]}]
            }))?;
            audio::save_index(&ws.work, &other)?;
            let open = |id: &str| -> Result<AudioSource> {
                let a = ws.archives().into_iter().find(|a| a.id == id).context("fixture archive")?;
                AudioSource::open(&ws.work, &a)
            };
            let (own, second) = (open("fixture.rpf")?, open("other.rpf")?);
            let line = |label: &str, wave: &str, speaker| dialogue::ConversationLine {
                label: label.into(), subtitle_key: sixng_core::meta7a::joaat(label), cue: wave[..wave.len() - 3].into(), variation: 1, wave: wave.into(), speaker, speaker_gap: None };
            let lines = [line("FX_A_01", "FXAU_AAAB_01", Some(5)), line("FX_B_01", "FXAU_AAAC_01", None), line("FX_C_01", "FXAU_AAAD_01", Some(5)),
                line("FX_E_01", "FXAU_AAAE_01", None),
                // speaker 7 has two lines in the second bank's voice alone: its line in both banks plays from there
                line("FX_V_01", "FXAU_AAAV_01", Some(7)), line("FX_V_02", "FXAU_AAAW_01", Some(7)), line("FX_V_03", "FXAU_AAAC_01", Some(7)),
                // speaker 8 has only one: not enough to pick
                line("FX_W_01", "FXAU_AAAX_01", Some(8)), line("FX_W_02", "FXAU_AAAC_01", Some(8)),
                // speaker 9 has no single-bank lines, but a speech voice: its context FXAU_AAAC is in voice 0xa
                line("FX_X_01", "FXAU_AAAC_01", Some(9))];
            // bank containers (voices): the first bank is voice 0xa, the second 0xb
            let containers = [BankContainers::from([(4096, [(0xa, 1)].into()), (6144, [(0xb, 1)].into())]), BankContainers::new()];
            let mut rejected = Vec::new();
            let voices = [Voices::from([(9, [(sixng_core::audiometa::context_hash("FXAU_AAAC"), (0xa, 1))].into())]), Voices::new()];
            let (links, audio_links, by_voice, speakers) = link_lines(&lines, &[&own, &second], &containers, &voices, &mut |at, why| rejected.push((at, why)));
            assert_eq!((links.len(), speakers), (10, 8));
            assert_eq!(by_voice, BTreeMap::from([("speaker_lines", 1), ("speech_voice", 1)]));
            assert_eq!(audio_links, BTreeMap::from([("fixture.rpf".to_string(), 6), ("other.rpf".to_string(), 1)]));
            let x = links[9].audio.as_ref().unwrap();
            assert_eq!((x.address.bank_offset, x.address.stream_index), (4096, 1));
            let v3 = links[6].audio.as_ref().unwrap();
            assert_eq!((v3.address.bank_offset, v3.address.stream_index), (6144, 0));
            assert!(links[8].audio.is_none());
            let a0 = links[0].audio.as_ref().unwrap();
            assert_eq!((a0.address.archive.as_str(), a0.address.bank_offset, a0.address.stream_index, a0.address.data_offset), ("fixture.rpf", 4096, 0, 4224));
            assert_eq!(a0.wave.as_deref(), Some("FXAU_AAAB_01"));
            assert_eq!(links[0].speaker, Some(Speaker { label: "00000005".into(), hash: Some(5) }));
            assert!(links[1].audio.is_none() && links[2].audio.is_none());
            let a3 = links[3].audio.as_ref().unwrap();
            assert_eq!((a3.address.archive.as_str(), a3.address.bank_offset, a3.address.stream_index), ("other.rpf", 1024, 1));
            assert_eq!(rejected.len(), 3);
            assert!(rejected[0].1.contains("several banks") && rejected[1].1.contains("no audio index") && rejected[2].1.contains("several banks"));
            assert!(rejected[1].0.contains("FXAU_AAAD_01"));
            Ok(())
        })();
        let cleanup = std::fs::remove_dir_all(&root);
        result.and_then(|()| Ok(cleanup?))
    }

    #[test]
    fn table_rows_check_and_fill_derived_links() {
        let address = |bank: u64| LinkedAudio {
            address: StreamAddress { archive: "fixture.rpf".into(), bank_offset: bank, stream_index: 0, stream_id: 9, data_offset: bank + 128, data_size: 64 },
            stem: None,
            wave: None,
        };
        let derived = |key: u32, speaker: u32, audio: Option<LinkedAudio>| DialogueLink {
            subtitle_key: key, speaker: Some(Speaker { label: format!("{speaker:08x}"), hash: Some(speaker) }), audio, text: None, mission: None, scene: None };
        let mut links = vec![derived(1, 100, Some(address(4096))), derived(2, 200, None)];
        let characters: HashSet<u32> = [100, 200].into();
        let mut check = BTreeMap::new();
        let who = |h: u32| Speaker { label: "x".into(), hash: Some(h) };
        check_row(&mut links, &[0], &characters, Some(address(4096)), Some(&who(100)), &mut check);
        check_row(&mut links, &[0], &characters, Some(address(8192)), Some(&who(200)), &mut check);
        check_row(&mut links, &[0], &characters, None, Some(&who(777)), &mut check);
        // no derived stream: the table's fills it
        check_row(&mut links, &[1], &characters, Some(address(8192)), None, &mut check);
        assert_eq!(links[1].audio.as_ref().unwrap().address.bank_offset, 8192);
        assert_eq!(check, BTreeMap::from([("audio_agrees", 1), ("audio_differs", 1), ("audio_filled", 1), ("speaker_agrees", 1), ("speaker_differs", 1), ("speaker_id_not_in_files", 1)]));
    }

    #[test]
    fn speakers_are_named_by_hash_from_one_distinct_name() {
        let j = sixng_core::meta7a::joaat;
        let link = |h: u32| DialogueLink { subtitle_key: h, speaker: Some(Speaker { label: format!("{h:08x}"), hash: Some(h) }), audio: None, text: None, mission: None, scene: None };
        let (guide, bret, twin) = (j("guide_fx"), j("bret_vogel"), j("fx"));
        let mut links = vec![link(guide), link(bret), link(twin), link(j("nobody_fx")),
            // a table speaker keeps its own label
            DialogueLink { subtitle_key: 9, speaker: Some(Speaker { label: "visitor".into(), hash: Some(j("visitor")) }), audio: None, text: None, mission: None, scene: None }];
        let mut namer = Namer::new(&links);
        namer.offer_words("subtitles", "~sl:0:1~Hey, Guide FX! I'm here.");
        namer.offer_parts("ped_models", "cs_bret_vogel_01");
        // two distinct names for one hash: no name
        namer.offer("names", "fx");
        namer.found.get_mut(&twin).unwrap().0.insert("other".into());
        namer.offer("address_table", "credit:someone");
        let report = namer.apply(&mut links);
        let labels: Vec<&str> = links.iter().map(|l| l.speaker.as_ref().unwrap().label.as_str()).collect();
        assert_eq!(labels[..2], ["guide_fx", "bret_vogel"]);
        assert_eq!(labels[2], format!("{twin:08x}"));
        assert_eq!(labels[4], "visitor");
        assert_eq!(report["named"], json!({"ped_models": 1, "subtitles": 1}));
        assert_eq!(report["several_names"], 1);
        assert_eq!(report["speakers"], 4);
    }
}
