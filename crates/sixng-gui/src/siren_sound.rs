//! Siren sounds for the model viewer.
//!
//! The game's sound definitions (the schema-166 files of audio.rpf) hold
//! simple sounds under the names below; each plays one wave of a bank. A
//! chosen sound is found by its name hash, its stream decoded (Opus) in a
//! background thread and looped on the default output device.
//!
//! Which vehicle plays which of these is not read from the files (a
//! vehicle's audio record points into an object graph that is not decoded),
//! so the choice is the user's.

use sixng_core::{audio, audiometa, meta7a, scan};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Label and the game's sound name.
pub const SOUNDS: [(&str, &str); 10] = [
    ("wail 1", "RESIDENT_VEHICLES_SIREN_WAIL_01"),
    ("wail 2", "RESIDENT_VEHICLES_SIREN_WAIL_02"),
    ("wail 3", "RESIDENT_VEHICLES_SIREN_WAIL_03"),
    ("wail 4", "RESIDENT_VEHICLES_SIREN_WAIL_04"),
    ("yelp 1", "RESIDENT_VEHICLES_SIREN_QUICK_01"),
    ("yelp 2", "RESIDENT_VEHICLES_SIREN_QUICK_02"),
    ("yelp 3", "RESIDENT_VEHICLES_SIREN_QUICK_03"),
    ("yelp 4", "RESIDENT_VEHICLES_SIREN_QUICK_04"),
    ("fire truck wail", "RESIDENT_VEHICLES_SIREN_FIRETRUCK_WAIL_01"),
    ("fire truck yelp", "RESIDENT_VEHICLES_SIREN_FIRETRUCK_QUICK_01"),
];

type Pcm = (u8, Arc<Vec<i16>>);

#[derive(Default)]
pub struct SirenSound {
    /// 0 off, n the nth of `SOUNDS`.
    pub want: usize,
    playing: usize,
    loading: Option<usize>,
    slot: Arc<Mutex<Option<(usize, Result<Pcm, String>)>>>,
    cache: HashMap<usize, Pcm>,
    player: Option<(rodio::OutputStream, rodio::Sink)>,
    pub error: Option<String>,
}

fn decode(game: &str, work: &str, name: &str) -> anyhow::Result<Pcm> {
    let archive = Path::new(game).join("audio.rpf");
    let work = Path::new(work);
    let ix = scan::load_index(&scan::index_path(&work.join("index"), "audio.rpf")).map_err(|e| anyhow::anyhow!("audio.rpf is not indexed ({e})"))?;
    let sel: Vec<&scan::Entry> = ix.entries.iter().filter(|e| e.kind == sixng_core::classify::Kind::Bin).collect();
    let h = meta7a::joaat(name);
    // reading the sound definitions takes seconds: all ten names at once, kept
    static WAVES: Mutex<Option<HashMap<u32, (u32, u32)>>> = Mutex::new(None);
    let stream = {
        let mut w = WAVES.lock().unwrap();
        let all: Vec<u32> = SOUNDS.iter().map(|s| meta7a::joaat(s.1)).collect();
        w.get_or_insert_with(|| audiometa::sound_waves(&archive, &sel, &all)).get(&h).map(|x| x.1)
    };
    let stream = &stream.ok_or_else(|| anyhow::anyhow!("no sound named {name} in the sound definitions"))?;
    let aix = audio::load_index(work, "audio.rpf").map_err(|e| anyhow::anyhow!("audio.rpf's banks are not scanned yet: open the Audio page once ({e})"))?;
    let (bank, s) = aix.banks.iter().find_map(|b| b.streams.iter().find(|s| s.id == *stream).map(|s| (b, s))).ok_or_else(|| anyhow::anyhow!("stream {stream:08x} is in no scanned bank"))?;
    let bytes = audio::read_bank(&archive, bank)?;
    let pk = audio::packets(&bytes, s);
    if pk.is_empty() {
        anyhow::bail!("no Opus packets in stream {stream:08x}");
    }
    let (ch, pcm) = audio::decode(&pk, s.frames())?;
    Ok((ch, Arc::new(pcm)))
}

impl SirenSound {
    /// Follows `want`: stops, starts decoding or starts the loop. Call once
    /// a frame. True while a sound is being decoded.
    pub fn update(&mut self, game: &str, work: &str) -> bool {
        if let Some((i, r)) = self.slot.lock().unwrap().take() {
            self.loading = None;
            match r {
                Ok(p) => {
                    self.cache.insert(i, p);
                }
                Err(e) => {
                    self.error = Some(e);
                    if self.want == i {
                        self.want = 0;
                    }
                }
            }
        }
        if self.want != self.playing {
            self.player = None;
            self.playing = 0;
            if self.want == 0 {
                return false;
            }
            if let Some((ch, pcm)) = self.cache.get(&self.want) {
                use rodio::Source;
                let r = rodio::OutputStream::try_default().map_err(|e| format!("no audio output: {e}")).and_then(|(stream, handle)| {
                    let sink = rodio::Sink::try_new(&handle).map_err(|e| e.to_string())?;
                    sink.set_volume(0.5);
                    sink.append(rodio::buffer::SamplesBuffer::new(*ch as u16, 48000, pcm.as_ref().clone()).repeat_infinite());
                    Ok((stream, sink))
                });
                match r {
                    Ok(p) => {
                        self.player = Some(p);
                        self.playing = self.want;
                        self.error = None;
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.want = 0;
                    }
                }
            } else if self.loading.is_none() {
                let (i, slot) = (self.want, self.slot.clone());
                let (game, work, name) = (game.to_string(), work.to_string(), SOUNDS[i - 1].1);
                self.loading = Some(i);
                std::thread::spawn(move || {
                    let r = decode(&game, &work, name).map_err(|e| e.to_string());
                    *slot.lock().unwrap() = Some((i, r));
                });
            }
        }
        self.loading.is_some()
    }
}

#[cfg(test)]
mod tests {
    /// Needs the game: SIXNG_GAME and SIXNG_WORK, with audio.rpf indexed and
    /// its banks scanned.
    #[test]
    #[ignore]
    fn every_siren_sound_decodes() {
        let (game, work) = (std::env::var("SIXNG_GAME").unwrap(), std::env::var("SIXNG_WORK").unwrap());
        for (label, name) in super::SOUNDS {
            let (ch, pcm) = super::decode(&game, &work, name).unwrap_or_else(|e| panic!("{label}: {e}"));
            let secs = pcm.len() as f64 / ch.max(1) as f64 / 48000.0;
            let peak = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
            println!("{label}: {ch} ch, {secs:.2} s, peak {peak}");
            assert!(secs > 0.1 && peak > 1000, "{label}: silent or empty");
        }
    }
}
