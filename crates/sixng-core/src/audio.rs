//! Audio banks (`ADAT` containers) in `audio.rpf`.
//!
//! The archive's file table is encrypted, so banks are found by scanning for
//! the `ADAT` magic and validating the header. A bank holds one or more
//! streams; each stream is a set of chunks (format, data, seek table, peaks,
//! markers …). The audio itself is Opus at 48 kHz, stored as
//! `[u16 length][packet]` records packed into 2 KiB blocks (a record never
//! crosses a block; a zero length or the end of the block moves to the next
//! block). Export is a lossless remux into Ogg Opus (`.opus`), or decoded to
//! 16-bit WAV with libopus.
//!
//! Header (little-endian): `ADAT`, u32 flags, u32 stream count N, u32 header
//! length; then (by flag) a u32 word (0x0040_0000), N u16 first-chunk indices
//! (0x0001_0000), N+1 u32 of unknown use (0x0020_0000); N u32 stream entries
//! (bits 0-28 id, 29-31 chunk count); the chunk records (u64: type 63-56,
//! size 55-28, offset 27-0 from the bank start), ending at the header length.
//! Stream ids and chunk types are hashes; names are not recovered.
//!
//! Music streams (a stream-format chunk next to the data chunk) hold several
//! sub-streams ("channels", each its own mono or stereo Opus stream, e.g.
//! stems). Stream-format chunk: u32 block count, u32 block size, u32 channel
//! count, then 16 bytes per channel (u32 id, u32 samples, u16 ?, u16 rate,
//! u8 codec, 3 bytes ?). The data is that many blocks of the block size (the
//! last one shorter). A block starts with 24 bytes per channel (u32 -1, u32
//! 2 KiB-block count, u32 0, u32 samples, u32 0, u32 0), then one u32 seek
//! table per channel (one sample position per 2 KiB block), padded to 2 KiB;
//! then each channel's 2 KiB blocks in turn, packed as in plain streams.
//! Sample counts (channel and block) count each channel of a stereo
//! sub-stream, so they are twice the frame count for stereo.
//!
//! Codec: the 32-byte format chunk holds a codec id at 0x1C and a channel
//! count at 0x1D: 11 Opus, 0 PCM signed 16-bit LE, 2 PCM float32 LE, 5 XMA2. In
//! audio.rpf: 1,598,051 Opus, 1,818 XMA2, 227 float32 PCM and 19 PCM16
//! streams outside music. The byte at 0x13, read as the codec before, is
//! not the codec (it is 0 on all PCM streams and on 268 XMA2 streams).
//! PCM exports as WAV (16-bit integer or 32-bit float, bit-exact). XMA2 data
//! is a run of 2,048-byte XMA2 packets (the seek table holds one u32 per
//! packet); it is remuxed losslessly into a RIFF WAVE file with an
//! XMA2WAVEFORMATEX header (`.xma`), which standard XMA2 decoders read.

use crate::export::Progress;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const CH_DATA: u8 = 0x55;
pub const CH_FORMAT: u8 = 0x76;
pub const CH_FORMAT_ALT: u8 = 0xFA;
pub const CH_STREAM_FORMAT: u8 = 0x48;
pub const CH_PEAK: u8 = 0x36;
pub const CH_SEEK: u8 = 0xA3;
pub const CH_MARKERS: u8 = 0xBD;

/// Codec ids (u8 at 0x1C of the 32-byte format chunk).
pub const CODEC_PCM16: u8 = 0;
pub const CODEC_PCM_F32: u8 = 2;
pub const CODEC_XMA2: u8 = 5;
pub const CODEC_OPUS: u8 = 11;

/// XMA2 packets are 2 KiB; each XMA frame decodes to 512 samples per channel.
pub const XMA2_PACKET: usize = 0x800;
pub const XMA2_FRAME_SAMPLES: u32 = 512;

/// Bumped when the index meaning changes, so older indexes get rescanned.
/// 2: codec from 0x1C; PCM and XMA2 streams get their own layouts.
/// 3: stream names from the name-tag chunks (`Bank::names`).
pub const INDEX_VERSION: u32 = 3;

const BLOCK: usize = 0x800;
/// Opus default encoder delay, used when the packet total does not give one.
const DEFAULT_PRE_SKIP: u32 = 312;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub kind: u8,
    pub off: u32,
    pub size: u32,
}

/// Saved as a tuple to keep the index small (1.6 M streams in audio.rpf).
type StreamT = (u32, u32, u32, u8, u32, u8, u32, u32);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "StreamT", into = "StreamT")]
pub struct Stream {
    /// 29-bit hash id (hex in reports).
    pub id: u32,
    /// All chunks (only while scanning; not saved in the index).
    pub chunks: Vec<Chunk>,
    /// Data chunk (offset from the bank start, size); size 0 = none.
    pub data: (u32, u32),
    /// Sample count from the format chunk. For Opus streams it is in 48 kHz
    /// units over all channels (a stereo stream counts each frame twice),
    /// whatever the rate field says (most say 32000: the source rate).
    pub samples: u32,
    /// Rate field of the format chunk.
    pub rate: u32,
    /// Codec id: 0x1C of a 32-byte format chunk (CODEC_*), else the byte at
    /// 0x13 of the shorter forms; for music streams, the first sub-stream's.
    pub codec: u8,
    /// Channels: from the first packet for Opus streams; for music streams
    /// the number of sub-streams (stems) in the stream-format chunk; for PCM
    /// and XMA2 the format chunk's channel count.
    pub channels: u32,
    /// "blocks" (Opus, 2 KiB packet blocks), "music" (sub-streams in large
    /// blocks, see the module notes), "pcm16" / "pcmf32" (PCM, exported as
    /// WAV), "xma2" (XMA2 packets, remuxed to RIFF XMA2), "other" (another
    /// codec id: not decoded), or "none" (no data chunk: a channel entry of
    /// a music stream, or metadata only).
    pub layout: String,
}

impl From<StreamT> for Stream {
    fn from(t: StreamT) -> Self {
        let layout = match t.5 { 1 => "blocks", 2 => "music", 3 => "other", 4 => "pcm16", 5 => "pcmf32", 6 => "xma2", _ => "none" }.into();
        Stream { id: t.0, chunks: Vec::new(), data: (t.6, t.7), samples: t.1, rate: t.2, codec: t.3, channels: t.4, layout }
    }
}

impl From<Stream> for StreamT {
    fn from(s: Stream) -> Self {
        let l = match s.layout.as_str() { "blocks" => 1, "music" => 2, "other" => 3, "pcm16" => 4, "pcmf32" => 5, "xma2" => 6, _ => 0 };
        (s.id, s.samples, s.rate, s.codec, s.channels, l, s.data.0, s.data.1)
    }
}

impl Stream {
    pub fn chunk(&self, kind: u8) -> Option<&Chunk> {
        self.chunks.iter().find(|c| c.kind == kind)
    }
    /// Samples per channel at 48 kHz (Opus streams).
    pub fn frames(&self) -> u32 {
        self.samples / self.channels.max(1)
    }
    pub fn playable(&self) -> bool {
        self.layout == "blocks"
    }
    /// Exported per stream (Opus, music, PCM or XMA2), as opposed to raw only.
    pub fn exportable(&self) -> bool {
        matches!(self.layout.as_str(), "blocks" | "music" | "pcm16" | "pcmf32" | "xma2")
    }
    pub fn seconds(&self) -> f64 {
        match self.layout.as_str() {
            "blocks" => self.frames() as f64 / 48000.0,
            "music" => self.samples as f64 / 48000.0,
            _ if self.rate > 0 => self.samples as f64 / self.rate as f64,
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bank {
    /// Offset of the `ADAT` magic in the archive.
    pub off: u64,
    pub size: u64,
    pub flags: u32,
    pub streams: Vec<Stream>,
    /// Stream id -> the wave name the bank stores itself ([`embedded_name`]).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub names: std::collections::BTreeMap<u32, String>,
}

/// A stream's name tag: chunk 0x5C, an embedded resource (`RSC8`) that holds
/// the stream's wave name as plain text (`panic_communicate_01`). 874 banks
/// of audio.rpf have one per stream.
pub const CH_NAME_TAG: u8 = 0x5C;

/// The name in a name-tag chunk: a NUL-terminated identifier whose joaat's
/// low 29 bits are the stream id `id`, and whose full joaat the chunk also
/// stores (the resource's dictionary hash). Checked against the stream's
/// own id only, so a chance match is about one in 2^61 per string.
pub fn embedded_name(b: &[u8], id: u32) -> Option<String> {
    let mut start = 0;
    for (i, &c) in b.iter().enumerate() {
        if c.is_ascii_alphanumeric() || c == b'_' {
            continue;
        }
        if c == 0 && i - start >= 3 {
            let s = std::str::from_utf8(&b[start..i]).ok()?;
            let h = crate::meta7a::joaat(s);
            if h & 0x1FFF_FFFF == id && b.windows(4).any(|w| w == h.to_le_bytes()) {
                return Some(s.to_string());
            }
        }
        start = i + 1;
    }
    None
}

impl Bank {
    /// Total length of the playable streams, in seconds.
    pub fn seconds(&self) -> f64 {
        self.streams.iter().filter(|s| s.layout != "none").map(|s| s.seconds()).sum()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioIndex {
    /// INDEX_VERSION when written (absent = 1).
    #[serde(default)]
    pub version: u32,
    pub archive: String,
    pub archive_size: u64,
    pub banks: Vec<Bank>,
    pub note: String,
}

fn u16le(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]]) as u32)
}
fn u32le(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()))
}

/// Parse a bank header (`h` = the first `header length` bytes or more).
/// None when it is not a plausible bank.
pub fn parse_header(h: &[u8], off: u64) -> Option<Bank> {
    if h.get(0..4)? != b"ADAT" {
        return None;
    }
    let flags = u32le(h, 4)?;
    let n = u32le(h, 8)? as usize;
    let hl = u32le(h, 12)? as usize;
    if flags >> 24 != 0xFF || flags & 0xFFFF != 1 || n == 0 || n > 20000 || !(16..=(4 << 20)).contains(&hl) || h.len() < hl {
        return None;
    }
    let mut p = 16;
    if flags & 0x0040_0000 != 0 {
        p += 4;
    }
    let first: Option<Vec<u32>> = if flags & 0x0001_0000 != 0 {
        let v = (0..n).map(|i| u16le(h, p + 2 * i)).collect::<Option<Vec<_>>>()?;
        p += 2 * n;
        Some(v)
    } else {
        None
    };
    if flags & 0x0020_0000 != 0 {
        p += 4 * (n + 1);
    }
    let entries: Vec<(u32, usize)> = (0..n).map(|i| u32le(h, p + 4 * i).map(|v| (v & 0x1FFF_FFFF, (v >> 29) as usize))).collect::<Option<Vec<_>>>()?;
    p += 4 * n;
    let total: usize = entries.iter().map(|e| e.1).sum();
    if p + 8 * total != hl {
        return None;
    }
    if let Some(f) = &first {
        let mut acc = 0u32;
        for (i, e) in entries.iter().enumerate() {
            if f[i] != acc {
                return None;
            }
            acc += e.1 as u32;
        }
    }
    let mut size = hl as u64;
    let mut streams = Vec::with_capacity(n);
    let mut k = 0;
    for (id, cnt) in entries {
        let mut chunks = Vec::with_capacity(cnt);
        for _ in 0..cnt {
            let r = u64::from_le_bytes(h.get(p + 8 * k..p + 8 * k + 8)?.try_into().ok()?);
            k += 1;
            let c = Chunk { kind: (r >> 56) as u8, size: ((r >> 28) & 0x0FFF_FFFF) as u32, off: (r & 0x0FFF_FFFF) as u32 };
            if c.size > 0 && (c.off as usize) < hl {
                return None;
            }
            size = size.max(c.off as u64 + c.size as u64);
            chunks.push(c);
        }
        streams.push(Stream { id, chunks, data: (0, 0), samples: 0, rate: 0, codec: 0, channels: 0, layout: String::new() });
    }
    Some(Bank { off, size, flags, streams, names: Default::default() })
}

/// Fill in format fields from the bank bytes (header + small chunks).
fn read_formats(f: &mut File, bank: &mut Bank) -> Result<()> {
    for s in bank.streams.iter_mut() {
        if let Some(c) = s.chunk(CH_NAME_TAG).cloned().filter(|c| c.size <= 1 << 20) {
            let mut b = vec![0u8; c.size as usize];
            f.seek(SeekFrom::Start(bank.off + c.off as u64))?;
            f.read_exact(&mut b)?;
            if let Some(n) = embedded_name(&b, s.id) {
                bank.names.insert(s.id, n);
            }
        }
        let fc = s.chunks.iter().find(|c| (c.kind == CH_FORMAT || c.kind == CH_FORMAT_ALT) && c.size >= 20).cloned();
        if let Some(c) = fc {
            let mut b = vec![0u8; c.size.min(64) as usize];
            f.seek(SeekFrom::Start(bank.off + c.off as u64))?;
            f.read_exact(&mut b)?;
            s.samples = u32le(&b, 0).unwrap_or(0);
            s.rate = u16le(&b, 8).unwrap_or(0);
            if b.len() >= 0x1E {
                s.codec = b[0x1C];
                s.channels = b[0x1D] as u32;
            } else {
                // 20/24-byte forms: earlier reading of the codec byte (0 = Opus)
                s.codec = match b.get(0x13).copied().unwrap_or(0) {
                    0 => CODEC_OPUS,
                    c => c,
                };
            }
        }
        if let Some(c) = s.chunk(CH_STREAM_FORMAT).cloned() {
            let mut b = vec![0u8; c.size.min(12 + 16 * 64) as usize];
            f.seek(SeekFrom::Start(bank.off + c.off as u64))?;
            f.read_exact(&mut b)?;
            let ch = u32le(&b, 8).unwrap_or(0);
            if ch <= 64 && b.len() >= 12 + 16 * ch as usize {
                s.channels = ch;
                if ch > 0 {
                    s.samples = u32le(&b, 12 + 4).unwrap_or(0);
                    s.rate = u16le(&b, 12 + 10).unwrap_or(0);
                    s.codec = b[12 + 12];
                }
            }
        }
        if let Some(c) = s.chunk(CH_DATA) {
            s.data = (c.off, c.size);
        }
        s.layout = match (s.chunk(CH_DATA), s.chunk(CH_STREAM_FORMAT)) {
            (Some(_), Some(_)) => "music",
            (Some(_), None) => match s.codec {
                CODEC_OPUS => "blocks",
                CODEC_PCM16 => "pcm16",
                CODEC_PCM_F32 => "pcmf32",
                CODEC_XMA2 => "xma2",
                _ => "other",
            },
            _ => "none",
        }
        .into();
        if s.layout == "music" && s.channels > 0 && s.data.1 >= 0x803 {
            // the sample count counts both channels of a stereo sub-stream:
            // keep frames, from the first packet's stereo bit
            let mut h = vec![0u8; 24 * s.channels as usize];
            f.seek(SeekFrom::Start(bank.off + s.data.0 as u64))?;
            f.read_exact(&mut h)?;
            let n: u64 = (0..s.channels as usize).map(|c| u32le(&h, 24 * c + 4).unwrap_or(0) as u64).sum();
            let hdr = 24 * s.channels as u64 + 4 * n;
            let mut t = [0u8; 3];
            f.seek(SeekFrom::Start(bank.off + s.data.0 as u64 + hdr.div_ceil(BLOCK as u64) * BLOCK as u64))?;
            if f.read_exact(&mut t).is_ok() && t[2] & 4 != 0 {
                s.samples /= 2;
            }
        }
        if s.layout == "blocks" && s.data.1 >= 3 {
            // channels from the first packet's TOC (stereo bit)
            let mut t = [0u8; 3];
            f.seek(SeekFrom::Start(bank.off + s.data.0 as u64))?;
            f.read_exact(&mut t)?;
            s.channels = if t[2] & 4 != 0 { 2 } else { 1 };
        }
    }
    Ok(())
}

/// Scan an archive for banks. Streams the file in 32 MiB windows.
pub fn scan(path: &Path, archive: &str, pr: Progress) -> Result<AudioIndex> {
    let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut g = File::open(path)?;
    let len = f.metadata()?.len();
    const WIN: usize = 32 << 20;
    let mut buf = vec![0u8; WIN + 3];
    let mut base = 0u64;
    let mut skip_until = 0u64;
    let mut banks = Vec::new();
    let mut carry = 0usize;
    loop {
        let n = read_full(&mut f, &mut buf[carry..])?;
        let avail = carry + n;
        if avail < 4 {
            break;
        }
        let mut i = 0;
        while let Some(rel) = find(&buf[i..avail], b"ADAT") {
            let at = i + rel;
            let abs = base + at as u64;
            i = at + 1;
            if abs < skip_until {
                continue;
            }
            // header: read 16 bytes, then the whole header
            let mut h16 = [0u8; 16];
            g.seek(SeekFrom::Start(abs))?;
            if g.read_exact(&mut h16).is_err() {
                continue;
            }
            let hl = u32::from_le_bytes(h16[12..16].try_into().unwrap()) as usize;
            if !(16..=(4 << 20)).contains(&hl) {
                continue;
            }
            let mut h = vec![0u8; hl];
            g.seek(SeekFrom::Start(abs))?;
            if g.read_exact(&mut h).is_err() {
                continue;
            }
            let Some(mut bank) = parse_header(&h, abs) else { continue };
            if abs + bank.size > len {
                continue;
            }
            read_formats(&mut g, &mut bank)?;
            for s in bank.streams.iter_mut() {
                s.chunks = Vec::new();
            }
            skip_until = abs + bank.size;
            banks.push(bank);
        }
        let consumed = avail.saturating_sub(3);
        base += consumed as u64;
        buf.copy_within(consumed..avail, 0);
        carry = avail - consumed;
        pr("scan", base.min(len), len);
        if n == 0 {
            break;
        }
    }
    Ok(AudioIndex {
        version: INDEX_VERSION,
        archive: archive.into(),
        archive_size: len,
        banks,
        note: "Audio banks (ADAT) found by scanning for the magic; offsets are in the archive. Stream ids are 29-bit hashes; names = the wave names banks store in name-tag chunks (0x5C), by stream id. layout: blocks = Opus packets in 2 KiB blocks (exported as Ogg Opus or WAV); music = sub-streams (stems), each its own Opus stream, exported one file per sub-stream; music lengths are per sub-stream; pcm16 / pcmf32 = PCM (exported as WAV); xma2 = XMA2 packets (remuxed to RIFF XMA2, .xma); other = another codec id, not decoded (exported raw); none = no data chunk. codec = format chunk byte 0x1C (11 Opus, 0 PCM16, 2 PCM float32, 5 XMA2). Opus sample counts are 48 kHz units over all channels.".into(),
    })
}

fn read_full(f: &mut File, buf: &mut [u8]) -> Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        let k = f.read(&mut buf[n..])?;
        if k == 0 {
            break;
        }
        n += k;
    }
    Ok(n)
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

/// Read a bank's bytes.
pub fn read_bank(path: &Path, b: &Bank) -> Result<Vec<u8>> {
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(b.off))?;
    let mut v = vec![0u8; b.size as usize];
    f.read_exact(&mut v)?;
    Ok(v)
}

/// Opus packets of a stream in 2 KiB-block layout.
pub fn packets(bank: &[u8], s: &Stream) -> Vec<Vec<u8>> {
    let (off, size) = s.data;
    let Some(d) = bank.get(off as usize..(off + size) as usize) else { return Vec::new() };
    let mut out = Vec::new();
    for blk in d.chunks(BLOCK) {
        let mut p = 0;
        while p + 2 <= blk.len() {
            let n = u16::from_le_bytes([blk[p], blk[p + 1]]) as usize;
            if n == 0 || p + 2 + n > blk.len() {
                break;
            }
            out.push(blk[p + 2..p + 2 + n].to_vec());
            p += 2 + n;
        }
    }
    out
}

/// One sub-stream of a music stream.
pub struct MusicChannel {
    pub id: u32,
    /// Frames per channel at 48 kHz.
    pub frames: u32,
    pub packets: Vec<Vec<u8>>,
}

/// Split a music stream into its sub-streams. `bank` = the whole bank.
pub fn music_channels(bank: &[u8], bank_off: u64, s: &Stream) -> Result<Vec<MusicChannel>> {
    let hb = parse_header(bank, bank_off).ok_or_else(|| anyhow!("bank header"))?;
    let hs = hb.streams.iter().find(|x| x.id == s.id).ok_or_else(|| anyhow!("stream {:08x} not in the bank header", s.id))?;
    let sf = hs.chunk(CH_STREAM_FORMAT).ok_or_else(|| anyhow!("stream {:08x}: no stream-format chunk", s.id))?;
    let dc = hs.chunk(CH_DATA).ok_or_else(|| anyhow!("stream {:08x}: no data chunk", s.id))?;
    let f = bank.get(sf.off as usize..(sf.off + sf.size) as usize).ok_or_else(|| anyhow!("stream-format chunk outside the bank"))?;
    let d = bank.get(dc.off as usize..(dc.off + dc.size) as usize).ok_or_else(|| anyhow!("data chunk outside the bank"))?;
    let (nblk, bsize, nch) = (u32le(f, 0).unwrap_or(0) as usize, u32le(f, 4).unwrap_or(0) as usize, u32le(f, 8).unwrap_or(0) as usize);
    if nch == 0 || nch > 64 || f.len() < 12 + 16 * nch || bsize < BLOCK || nblk == 0 || (nblk - 1) * bsize >= d.len() {
        return Err(anyhow!("stream {:08x}: unexpected music layout ({nblk} blocks of {bsize:#x}, {nch} ch, data {:#x})", s.id, d.len()));
    }
    let mut out: Vec<MusicChannel> = (0..nch).map(|c| MusicChannel { id: u32le(f, 12 + 16 * c).unwrap(), frames: u32le(f, 12 + 16 * c + 4).unwrap(), packets: Vec::new() }).collect();
    for bi in 0..nblk {
        let blk = &d[bi * bsize..((bi + 1) * bsize).min(d.len())];
        let counts: Vec<usize> = (0..nch).map(|c| u32le(blk, 24 * c + 4).unwrap_or(0) as usize).collect();
        let hdr = (24 * nch + 4 * counts.iter().sum::<usize>()).div_ceil(BLOCK) * BLOCK;
        if hdr + BLOCK * counts.iter().sum::<usize>() > blk.len() {
            return Err(anyhow!("stream {:08x}: block {bi} is shorter than its header says", s.id));
        }
        let mut p = hdr;
        for (c, &n) in counts.iter().enumerate() {
            let sub = Stream { id: 0, chunks: Vec::new(), data: (p as u32, (n * BLOCK) as u32), samples: 0, rate: 0, codec: 0, channels: 0, layout: String::new() };
            out[c].packets.extend(packets(blk, &sub));
            p += n * BLOCK;
        }
    }
    for c in out.iter_mut() {
        if packet_channels(&c.packets) == 2 {
            c.frames /= 2;
        }
    }
    Ok(out)
}

/// Samples (at 48 kHz) in an Opus packet, from its TOC byte (RFC 6716 3.1).
pub fn packet_samples(p: &[u8]) -> u32 {
    let Some(&toc) = p.first() else { return 0 };
    let cfg = toc >> 3;
    let frame = if cfg < 12 {
        [480, 960, 1920, 2880][(cfg & 3) as usize]
    } else if cfg < 16 {
        [480, 960][(cfg & 1) as usize]
    } else {
        [120, 240, 480, 960][(cfg & 3) as usize]
    };
    let frames = match toc & 3 {
        0 => 1,
        1 | 2 => 2,
        _ => p.get(1).map(|b| (b & 0x3F) as u32).unwrap_or(0),
    };
    frame * frames
}

pub fn packet_channels(pk: &[Vec<u8>]) -> u8 {
    if pk.iter().any(|p| p.first().map_or(false, |t| t & 4 != 0)) { 2 } else { 1 }
}

/// Encoder delay. The packets hold about 2.5 k samples more than the format
/// count, not frame-aligned; the split between start delay and end padding
/// is not known, so the Opus default delay is skipped and the end trimmed to
/// the count.
pub fn pre_skip() -> u32 {
    DEFAULT_PRE_SKIP
}

fn ogg_crc(data: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &b in data {
        crc ^= (b as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04C1_1DB7 } else { crc << 1 };
        }
    }
    crc
}

fn ogg_page(out: &mut Vec<u8>, serial: u32, seq: u32, granule: u64, flags: u8, packet: &[u8]) {
    // one packet per page (packets here are < 64 KiB)
    let mut lacing = Vec::new();
    let mut n = packet.len();
    loop {
        if n >= 255 {
            lacing.push(255u8);
            n -= 255;
        } else {
            lacing.push(n as u8);
            break;
        }
    }
    let start = out.len();
    out.extend_from_slice(b"OggS");
    out.push(0);
    out.push(flags);
    out.extend_from_slice(&granule.to_le_bytes());
    out.extend_from_slice(&serial.to_le_bytes());
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(lacing.len() as u8);
    out.extend_from_slice(&lacing);
    out.extend_from_slice(packet);
    let crc = ogg_crc(&out[start..]);
    out[start + 22..start + 26].copy_from_slice(&crc.to_le_bytes());
}

/// Lossless remux of Opus packets into an Ogg Opus file. `samples` = per
/// channel (`Stream::frames`).
pub fn ogg_opus(pk: &[Vec<u8>], samples: u32, tag: &str) -> Vec<u8> {
    let ch = packet_channels(pk);
    let skip = pre_skip();
    let serial = 0x5358_4E47; // "SXNG"
    let mut out = Vec::new();
    let mut head = b"OpusHead".to_vec();
    head.push(1);
    head.push(ch);
    head.extend_from_slice(&(skip as u16).to_le_bytes());
    head.extend_from_slice(&48000u32.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    ogg_page(&mut out, serial, 0, 0, 0x02, &head);
    let vendor = b"sixng";
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    let c = format!("TITLE={tag}");
    tags.extend_from_slice(&1u32.to_le_bytes());
    tags.extend_from_slice(&(c.len() as u32).to_le_bytes());
    tags.extend_from_slice(c.as_bytes());
    ogg_page(&mut out, serial, 1, 0, 0, &tags);
    let mut gran = skip as u64;
    let end = if samples > 0 { skip as u64 + samples as u64 } else { u64::MAX };
    for (i, p) in pk.iter().enumerate() {
        gran += packet_samples(p) as u64;
        let last = i + 1 == pk.len();
        let g = if last { gran.min(end) } else { gran };
        ogg_page(&mut out, serial, 2 + i as u32, g, if last { 0x04 } else { 0 }, p);
    }
    out
}

/// Decode to interleaved 16-bit PCM at 48 kHz (pre-skip removed, trimmed to
/// `samples` per channel). Returns (channels, interleaved samples).
#[cfg(feature = "opus-decode")]
pub fn decode(pk: &[Vec<u8>], samples: u32) -> Result<(u8, Vec<i16>)> {
    let ch = packet_channels(pk);
    let mut dec = opus::Decoder::new(48000, if ch == 2 { opus::Channels::Stereo } else { opus::Channels::Mono }).map_err(|e| anyhow!("opus: {e}"))?;
    let mut pcm = Vec::new();
    let mut buf = vec![0i16; 5760 * ch as usize];
    for p in pk {
        match dec.decode(p, &mut buf, false) {
            Ok(n) => pcm.extend_from_slice(&buf[..n * ch as usize]),
            Err(_) => {
                // keep timing: a lost packet decodes as concealment
                if let Ok(n) = dec.decode(&[], &mut buf, false) {
                    pcm.extend_from_slice(&buf[..n * ch as usize]);
                }
            }
        }
    }
    let skip = (pre_skip() as usize * ch as usize).min(pcm.len());
    pcm.drain(..skip);
    if samples > 0 {
        pcm.truncate(samples as usize * ch as usize);
    }
    Ok((ch, pcm))
}

/// Built without the `opus-decode` feature (no libopus): Opus can't be
/// decoded; export it as `.opus` instead.
#[cfg(not(feature = "opus-decode"))]
pub fn decode(_pk: &[Vec<u8>], _samples: u32) -> Result<(u8, Vec<i16>)> {
    Err(anyhow!("this build has no Opus decoder (built without the opus-decode feature); export as .opus instead"))
}

pub fn wav(ch: u8, rate: u32, pcm: &[i16]) -> Vec<u8> {
    let data = pcm.len() * 2;
    let mut o = Vec::with_capacity(44 + data);
    o.extend_from_slice(b"RIFF");
    o.extend_from_slice(&((36 + data) as u32).to_le_bytes());
    o.extend_from_slice(b"WAVEfmt ");
    o.extend_from_slice(&16u32.to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&(ch as u16).to_le_bytes());
    o.extend_from_slice(&rate.to_le_bytes());
    o.extend_from_slice(&(rate * ch as u32 * 2).to_le_bytes());
    o.extend_from_slice(&(ch as u16 * 2).to_le_bytes());
    o.extend_from_slice(&16u16.to_le_bytes());
    o.extend_from_slice(b"data");
    o.extend_from_slice(&(data as u32).to_le_bytes());
    for s in pcm {
        o.extend_from_slice(&s.to_le_bytes());
    }
    o
}

/// File name stem for a stream: `<bank offset>_<stream id>`.
pub fn stem(b: &Bank, s: &Stream) -> String {
    format!("{:x}_{:08x}", b.off, s.id)
}

/// Export one stream (a music stream: one file per sub-stream, named
/// `<stem>_<sub-stream id>`). `fmt`: opus | wav | both. Returns written files.
pub fn export_stream(bank_bytes: &[u8], b: &Bank, s: &Stream, out: &Path, fmt: &str) -> Result<Vec<String>> {
    std::fs::create_dir_all(out)?;
    if s.layout == "music" {
        let mut files = Vec::new();
        for c in music_channels(bank_bytes, b.off, s)? {
            if c.packets.is_empty() {
                continue;
            }
            files.extend(write_stream(&c.packets, c.frames, &format!("{}_{:08x}", stem(b, s), c.id), out, fmt)?);
        }
        return Ok(files);
    }
    if matches!(s.layout.as_str(), "pcm16" | "pcmf32" | "xma2") {
        // one lossless file whatever `fmt` asks for: .wav (PCM) or .xma (XMA2)
        let (name, bytes) = match s.layout.as_str() {
            "xma2" => (format!("{}.xma", stem(b, s)), xma2_riff(stream_data(bank_bytes, s)?, s)?),
            _ => (format!("{}.wav", stem(b, s)), pcm_wav(stream_data(bank_bytes, s)?, s)?),
        };
        let p = out.join(name);
        std::fs::write(&p, bytes)?;
        return Ok(vec![p.display().to_string()]);
    }
    if s.layout != "blocks" {
        return Err(anyhow!("stream {:08x}: layout '{}' is not exported per stream (export the bank raw)", s.id, s.layout));
    }
    let pk = packets(bank_bytes, s);
    if pk.is_empty() {
        return Err(anyhow!("stream {:08x}: no Opus packets", s.id));
    }
    write_stream(&pk, s.frames(), &stem(b, s), out, fmt)
}

fn write_stream(pk: &[Vec<u8>], frames: u32, st: &str, out: &Path, fmt: &str) -> Result<Vec<String>> {
    let mut files = Vec::new();
    if fmt == "opus" || fmt == "both" {
        let p = out.join(format!("{st}.opus"));
        std::fs::write(&p, ogg_opus(pk, frames, st))?;
        files.push(p.display().to_string());
    }
    if fmt == "wav" || fmt == "both" {
        let (ch, pcm) = decode(pk, frames)?;
        let p = out.join(format!("{st}.wav"));
        let mut f = File::create(&p)?;
        f.write_all(&wav(ch, 48000, &pcm))?;
        files.push(p.display().to_string());
    }
    Ok(files)
}

/// A stream's data chunk.
pub fn stream_data<'a>(bank: &'a [u8], s: &Stream) -> Result<&'a [u8]> {
    let (off, size) = s.data;
    bank.get(off as usize..off as usize + size as usize).ok_or_else(|| anyhow!("stream {:08x}: data chunk outside the bank", s.id))
}

/// WAV for a PCM stream (`pcm16`: 16-bit integer, `pcmf32`: 32-bit float),
/// samples copied as stored and cut to the format chunk's sample count.
pub fn pcm_wav(data: &[u8], s: &Stream) -> Result<Vec<u8>> {
    let ch = s.channels.max(1);
    let (bytes, tag) = match s.layout.as_str() {
        "pcm16" => (2u32, 1u16),
        "pcmf32" => (4, 3),
        l => return Err(anyhow!("stream {:08x}: layout '{l}' is not PCM", s.id)),
    };
    let block = bytes * ch;
    let n = (s.samples as usize * block as usize).min(data.len() / block as usize * block as usize);
    if s.rate == 0 || n == 0 {
        return Err(anyhow!("stream {:08x}: empty PCM stream", s.id));
    }
    let data = &data[..n];
    // non-integer formats carry a fact chunk and an 18-byte fmt (cbSize = 0)
    let float = tag == 3;
    let fmt_len: u32 = if float { 18 } else { 16 };
    let fact_len: u32 = if float { 12 } else { 0 };
    let mut o = Vec::with_capacity(44 + 14 + n);
    o.extend_from_slice(b"RIFF");
    o.extend_from_slice(&(4 + 8 + fmt_len + fact_len + 8 + n as u32).to_le_bytes());
    o.extend_from_slice(b"WAVEfmt ");
    o.extend_from_slice(&fmt_len.to_le_bytes());
    o.extend_from_slice(&tag.to_le_bytes());
    o.extend_from_slice(&(ch as u16).to_le_bytes());
    o.extend_from_slice(&s.rate.to_le_bytes());
    o.extend_from_slice(&(s.rate * block).to_le_bytes());
    o.extend_from_slice(&(block as u16).to_le_bytes());
    o.extend_from_slice(&(bytes as u16 * 8).to_le_bytes());
    if float {
        o.extend_from_slice(&0u16.to_le_bytes());
        o.extend_from_slice(b"fact");
        o.extend_from_slice(&4u32.to_le_bytes());
        o.extend_from_slice(&((n / block as usize) as u32).to_le_bytes());
    }
    o.extend_from_slice(b"data");
    o.extend_from_slice(&(n as u32).to_le_bytes());
    o.extend_from_slice(data);
    Ok(o)
}

/// XMA frames that start in each 2 KiB packet: the top 6 bits of the
/// big-endian packet header (XMA2 packet header: frame count 6, first frame
/// offset in bits 15, metadata 3, packet skip count 8).
pub fn xma2_frames(data: &[u8]) -> u32 {
    data.chunks_exact(XMA2_PACKET).map(|p| (p[0] >> 2) as u32).sum()
}

/// Lossless remux of an XMA2 stream into RIFF WAVE with an XMA2WAVEFORMATEX
/// `fmt ` chunk (format tag 0x166, cbSize 34) and the packets as `data`.
/// The play region is the format chunk's sample count from sample 0.
pub fn xma2_riff(data: &[u8], s: &Stream) -> Result<Vec<u8>> {
    let ch = s.channels.max(1);
    if data.is_empty() || data.len() % XMA2_PACKET != 0 {
        return Err(anyhow!("stream {:08x}: XMA2 data is not whole 2 KiB packets ({} bytes)", s.id, data.len()));
    }
    if ch > 2 || s.rate == 0 {
        return Err(anyhow!("stream {:08x}: unexpected XMA2 format ({ch} ch, {} Hz)", s.id, s.rate));
    }
    let encoded = xma2_frames(data) * XMA2_FRAME_SAMPLES;
    let samples = s.samples.min(encoded);
    let n = data.len() as u32;
    let avg = if samples > 0 { (n as u64 * s.rate as u64 / samples as u64) as u32 } else { 0 };
    let mut o = Vec::with_capacity(8 + 4 + 8 + 52 + 8 + data.len());
    o.extend_from_slice(b"RIFF");
    o.extend_from_slice(&(4 + 8 + 52 + 8 + n).to_le_bytes());
    o.extend_from_slice(b"WAVEfmt ");
    o.extend_from_slice(&52u32.to_le_bytes());
    // WAVEFORMATEX
    o.extend_from_slice(&0x0166u16.to_le_bytes()); // WAVE_FORMAT_XMA2
    o.extend_from_slice(&(ch as u16).to_le_bytes());
    o.extend_from_slice(&s.rate.to_le_bytes());
    o.extend_from_slice(&avg.to_le_bytes());
    o.extend_from_slice(&(2 * ch as u16).to_le_bytes()); // nBlockAlign: 16-bit output per channel
    o.extend_from_slice(&16u16.to_le_bytes()); // wBitsPerSample
    o.extend_from_slice(&34u16.to_le_bytes()); // cbSize
    // XMA2WAVEFORMATEX extension
    o.extend_from_slice(&1u16.to_le_bytes()); // NumStreams (one mono or stereo stream)
    o.extend_from_slice(&(if ch == 1 { 0x4u32 } else { 0x3 }).to_le_bytes()); // ChannelMask: FC, or FL|FR
    o.extend_from_slice(&encoded.to_le_bytes()); // SamplesEncoded
    o.extend_from_slice(&n.to_le_bytes()); // BytesPerBlock (one block holds every packet)
    o.extend_from_slice(&0u32.to_le_bytes()); // PlayBegin
    o.extend_from_slice(&samples.to_le_bytes()); // PlayLength
    o.extend_from_slice(&0u32.to_le_bytes()); // LoopBegin
    o.extend_from_slice(&0u32.to_le_bytes()); // LoopLength
    o.push(0); // LoopCount
    o.push(4); // EncoderVersion
    o.extend_from_slice(&1u16.to_le_bytes()); // BlockCount
    o.extend_from_slice(b"data");
    o.extend_from_slice(&n.to_le_bytes());
    o.extend_from_slice(data);
    Ok(o)
}

pub fn index_path(work: &Path, archive: &str) -> std::path::PathBuf {
    work.join("index").join(format!("audio_{}.json.gz", archive.trim_end_matches(".rpf").replace(['/', '\\'], "_")))
}

pub fn load_index(work: &Path, archive: &str) -> Result<AudioIndex> {
    let p = index_path(work, archive);
    let f = File::open(&p).with_context(|| format!("no audio index {} (run `sixng audio` first)", p.display()))?;
    let ix: AudioIndex = serde_json::from_reader(std::io::BufReader::new(flate2::read::GzDecoder::new(f)))?;
    if ix.version < INDEX_VERSION {
        return Err(anyhow!("the audio index {} is from an older SixNG (no stream names, or codecs read from the wrong byte): scan audio.rpf again (`sixng audio --rescan`)", p.display()));
    }
    Ok(ix)
}

pub fn save_index(work: &Path, ix: &AudioIndex) -> Result<()> {
    let p = index_path(work, &ix.archive);
    std::fs::create_dir_all(p.parent().unwrap())?;
    let mut z = flate2::write::GzEncoder::new(std::io::BufWriter::new(File::create(p)?), flate2::Compression::fast());
    serde_json::to_writer(&mut z, ix)?;
    z.finish()?.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_tag() {
        let h = crate::meta7a::joaat("door_creak_03");
        let id = h & 0x1FFF_FFFF;
        // other strings first (not the stream's), then the name, then its
        // full hash; a run not ended by NUL does not count
        let mut b = b"RSC8\x01\x00\x00\x00other_name\x00\x00\x9a\xbcdoor_creak_03\x00\x00".to_vec();
        assert_eq!(embedded_name(&b, id), None); // no dictionary hash yet
        b.extend(h.to_le_bytes());
        assert_eq!(embedded_name(&b, id).as_deref(), Some("door_creak_03"));
        assert_eq!(embedded_name(&b, id ^ 1), None);
        assert_eq!(embedded_name(b"xxdoor_creak_03", id), None);
    }
    #[test]
    fn toc() {
        assert_eq!(packet_samples(&[0xF8, 0]), 960); // CELT FB 20 ms
        assert_eq!(packet_samples(&[0x78, 0]), 960); // hybrid FB 20 ms
        assert_eq!(packet_samples(&[0x1B, 0x03]), 3 * 2880); // SILK 60 ms, code 3, 3 frames
    }
    #[test]
    fn header() {
        // one stream, two chunks, flags 0xFF210001
        let mut h = b"ADAT".to_vec();
        h.extend_from_slice(&0xFF21_0001u32.to_le_bytes());
        h.extend_from_slice(&1u32.to_le_bytes());
        let hl = 16 + 2 + 8 + 4 + 16;
        h.extend_from_slice(&(hl as u32).to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(&[0u8; 8]);
        h.extend_from_slice(&((2u32 << 29) | 0x1234).to_le_bytes());
        let rec = |t: u64, s: u64, o: u64| (t << 56) | (s << 28) | o;
        h.extend_from_slice(&rec(0x76, 32, hl as u64).to_le_bytes());
        h.extend_from_slice(&rec(0x55, 2048, hl as u64 + 32).to_le_bytes());
        let b = parse_header(&h, 0).unwrap();
        assert_eq!(b.streams.len(), 1);
        assert_eq!(b.streams[0].id, 0x1234);
        assert_eq!(b.size, hl as u64 + 32 + 2048);
    }
    fn stream(layout: &str, samples: u32, rate: u32, channels: u32, size: u32) -> Stream {
        Stream { id: 0x42, chunks: Vec::new(), data: (0, size), samples, rate, codec: 0, channels, layout: layout.into() }
    }
    #[test]
    fn index_tuple_layouts() {
        for l in ["blocks", "music", "other", "pcm16", "pcmf32", "xma2", "none"] {
            let t: StreamT = stream(l, 1, 2, 1, 3).into();
            assert_eq!(Stream::from(t).layout, l);
        }
    }
    #[test]
    fn pcm16_wav() {
        let data: Vec<u8> = (0..20u8).collect(); // 10 samples, header says 8
        let w = pcm_wav(&data, &stream("pcm16", 8, 48000, 1, 20)).unwrap();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32le(&w, 4).unwrap() as usize, w.len() - 8);
        assert_eq!(u16le(&w, 20), Some(1)); // PCM
        assert_eq!(u16le(&w, 34), Some(16));
        assert_eq!(&w[36..40], b"data");
        assert_eq!(u32le(&w, 40), Some(16));
        assert_eq!(&w[44..], &data[..16]);
    }
    #[test]
    fn pcm_float_wav() {
        let data: Vec<u8> = [0.5f32, -0.25, 1.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        let w = pcm_wav(&data, &stream("pcmf32", 3, 48000, 1, 12)).unwrap();
        assert_eq!(u32le(&w, 16), Some(18)); // fmt size with cbSize
        assert_eq!(u16le(&w, 20), Some(3)); // IEEE float
        assert_eq!(u16le(&w, 34), Some(32));
        assert_eq!(&w[38..42], b"fact");
        assert_eq!(u32le(&w, 46), Some(3));
        assert_eq!(&w[50..54], b"data");
        assert_eq!(&w[58..], &data[..]);
        assert_eq!(u32le(&w, 4).unwrap() as usize, w.len() - 8);
    }
    #[test]
    fn xma2_remux() {
        // two packets: 14 and 3 frames start in them
        let mut data = vec![0u8; 2 * XMA2_PACKET];
        data[0] = 14 << 2;
        data[XMA2_PACKET] = 3 << 2;
        assert_eq!(xma2_frames(&data), 17);
        let w = xma2_riff(&data, &stream("xma2", 8000, 32000, 1, data.len() as u32)).unwrap();
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32le(&w, 16), Some(52));
        assert_eq!(u16le(&w, 20), Some(0x166));
        assert_eq!(u16le(&w, 22), Some(1));
        assert_eq!(u32le(&w, 24), Some(32000));
        assert_eq!(u16le(&w, 36), Some(34)); // cbSize
        let x = 38; // XMA2WAVEFORMATEX extension
        assert_eq!(u16le(&w, x), Some(1)); // NumStreams
        assert_eq!(u32le(&w, x + 2), Some(4)); // FC
        assert_eq!(u32le(&w, x + 6), Some(17 * 512)); // SamplesEncoded
        assert_eq!(u32le(&w, x + 10), Some(data.len() as u32)); // BytesPerBlock
        assert_eq!(u32le(&w, x + 18), Some(8000)); // PlayLength
        assert_eq!(u16le(&w, x + 32), Some(1)); // BlockCount
        assert_eq!(&w[72..76], b"data");
        assert_eq!(&w[80..], &data[..]);
        assert!(xma2_riff(&data[..100], &stream("xma2", 1, 32000, 1, 100)).is_err());
    }
    #[test]
    fn music() {
        // one music stream: 1 block, 2 mono sub-streams, one 2 KiB block each
        let mut h = b"ADAT".to_vec();
        h.extend_from_slice(&0xFF01_0001u32.to_le_bytes());
        h.extend_from_slice(&1u32.to_le_bytes());
        let hl = 16 + 2 + 4 + 16;
        h.extend_from_slice(&(hl as u32).to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(&(2u32 << 29).to_le_bytes());
        let rec = |t: u64, s: u64, o: u64| (t << 56) | (s << 28) | o;
        let sf_len = 12 + 32;
        let data_len = 2048 + 2 * 2048;
        h.extend_from_slice(&rec(0x48, sf_len as u64, hl as u64).to_le_bytes());
        h.extend_from_slice(&rec(0x55, data_len as u64, (hl + sf_len) as u64).to_le_bytes());
        for v in [1u32, 0x4000, 2] {
            h.extend_from_slice(&v.to_le_bytes());
        }
        for id in [0xAAu32, 0xBB] {
            h.extend_from_slice(&id.to_le_bytes());
            h.extend_from_slice(&960u32.to_le_bytes());
            h.extend_from_slice(&[0, 0, 0x80, 0xBB, 0, 0, 0, 0]);
        }
        let d0 = h.len();
        for _ in 0..2 {
            for v in [u32::MAX, 1, 0, 960, 0, 0] {
                h.extend_from_slice(&v.to_le_bytes());
            }
        }
        h.extend_from_slice(&[0u8; 8]); // seek tables
        h.resize(d0 + 2048, 0);
        for tag in [1u8, 2] {
            let s = h.len();
            h.extend_from_slice(&[3, 0, 0xF8, tag, tag]);
            h.resize(s + 2048, 0);
        }
        let b = parse_header(&h, 0).unwrap();
        let mut st = b.streams[0].clone();
        st.layout = "music".into();
        let c = music_channels(&h, 0, &st).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].id, c[0].frames), (0xAA, 960));
        assert_eq!(c[1].packets, vec![vec![0xF8, 2, 2]]);
    }
}
