//! Bink video carving (`sixng movies`).
//!
//! `movies.rpf` stores its videos uncompressed, so the zlib-stream index
//! finds nothing in it. The videos are plain Bink 2 files (`KB2` + revision
//! letter; a few older `BIK` ones), each starting on a 4 KiB boundary, and
//! the next one starts at the first boundary after the previous one ends.
//! The public container header is enough to carve and describe them:
//!
//! | off | field |
//! |---|---|
//! | 0 | signature (`KB2n`, `BIKi` …) |
//! | 4 | file size − 8 |
//! | 8 | frame count |
//! | 12 | largest frame size |
//! | 16 | frame count again |
//! | 20, 24 | width, height |
//! | 28, 32 | fps numerator, denominator |
//! | 36 | video flags |
//! | 40 | audio track count *n* |
//! | 44 | (Bink 2 only) one more u32 |
//! | then | *n* × u32 max decoded audio size, *n* × (u16 rate, u16 flags), *n* × u32 track id |
//! | then | frame count + 1 × u32 frame offsets (bit 0 = keyframe; the last is the file size) |
//!
//! The frames are not decoded here: the files are written as they are and
//! play in any player that supports Bink 2.

use crate::export::Progress;
use anyhow::Result;
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

#[derive(Serialize, Clone, Debug)]
pub struct Movie {
    pub offset: u64,
    pub offset_hex: String,
    pub size: u64,
    pub signature: String,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub fps: f64,
    pub seconds: f64,
    pub keyframes: u32,
    pub audio: Vec<AudioTrack>,
    /// Frame table checked: offsets rise and the last equals the file size.
    pub frame_table_ok: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct AudioTrack {
    pub id: u32,
    pub sample_rate: u32,
    pub stereo: bool,
    pub flags: String,
}

fn u32le(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()))
}

/// Parse a header at the start of `b` (`b` holds the first bytes of the
/// candidate: at least 64 KiB where available). `left` = bytes from here to
/// the end of the archive.
fn parse(b: &[u8], offset: u64, left: u64) -> Option<Movie> {
    let sig = b.get(0..4)?;
    let bink2 = &sig[0..3] == b"KB2";
    if !(bink2 || &sig[0..3] == b"BIK") || !sig[3].is_ascii_lowercase() {
        return None;
    }
    let size = u32le(b, 4)? as u64 + 8;
    let (frames, frames2) = (u32le(b, 8)?, u32le(b, 16)?);
    let (width, height) = (u32le(b, 20)?, u32le(b, 24)?);
    let (fnum, fden) = (u32le(b, 28)?, u32le(b, 32)?);
    let na = u32le(b, 40)? as usize;
    if size > left || size < 64 || frames == 0 || frames != frames2 || !(16..=8192).contains(&width) || !(16..=8192).contains(&height) || fnum == 0 || fden == 0 || na > 16 {
        return None;
    }
    let base = if bink2 { 48 } else { 44 };
    let mut audio = Vec::new();
    for i in 0..na {
        let rf = u32le(b, base + na * 4 + i * 4)?;
        let (rate, flags) = (rf & 0xFFFF, rf >> 16);
        audio.push(AudioTrack { id: u32le(b, base + na * 8 + i * 4)?, sample_rate: rate, stereo: flags & 0x2000 != 0, flags: format!("{flags:04x}") });
    }
    // Frame table (checked as far as the buffer reaches; the last entry only
    // when the whole table is in the buffer).
    let t = base + na * 12;
    let mut prev = 0u32;
    let mut ok = true;
    let mut keyframes = 0;
    let mut last = None;
    for i in 0..=frames as usize {
        let Some(v) = u32le(b, t + i * 4) else { break };
        let o = v & !1;
        if o < prev || o as u64 > size {
            ok = false;
            break;
        }
        if i < frames as usize && v & 1 == 1 {
            keyframes += 1;
        }
        prev = o;
        last = Some((i, o));
    }
    let first = u32le(b, t).map(|v| v & !1)? as usize;
    if first < t || first > t + (frames as usize + 1) * 4 + 64 {
        return None;
    }
    let complete = last.map(|(i, o)| i == frames as usize && o as u64 == size).unwrap_or(false);
    let fps = fnum as f64 / fden as f64;
    Some(Movie {
        offset,
        offset_hex: format!("{offset:x}"),
        size,
        signature: String::from_utf8_lossy(sig).into_owned(),
        width,
        height,
        frames,
        fps,
        seconds: frames as f64 / fps,
        keyframes,
        audio,
        frame_table_ok: ok && complete,
    })
}

/// Find every Bink file in `archive`: check each 4 KiB boundary, skip to the
/// end of each file found. Reads the archive once, in order.
pub fn scan(archive: &Path, progress: &Progress) -> Result<Vec<Movie>> {
    const ALIGN: u64 = 4096;
    let mut f = std::fs::File::open(archive)?;
    let len = f.metadata()?.len();
    let mut out = Vec::new();
    let mut pos = 0u64;
    // Header probes need the frame table: read up to 256 KiB per candidate.
    let mut probe = vec![0u8; 256 << 10];
    // Bulk buffer for boundary checks.
    let chunk = 32u64 << 20;
    let mut buf = vec![0u8; chunk as usize];
    let mut buf_at = u64::MAX;
    let mut buf_len = 0usize;
    let mut last_report = 0u64;
    while pos + 64 <= len {
        if pos < buf_at || pos + 4 > buf_at + buf_len as u64 {
            f.seek(SeekFrom::Start(pos))?;
            buf_len = read_full(&mut f, &mut buf)?;
            buf_at = pos;
            if buf_len < 4 {
                break;
            }
        }
        let i = (pos - buf_at) as usize;
        let s = &buf[i..i + 3];
        if s == b"KB2" || s == b"BIK" {
            f.seek(SeekFrom::Start(pos))?;
            let n = read_full(&mut f, &mut probe)?;
            if let Some(m) = parse(&probe[..n], pos, len - pos) {
                pos = (pos + m.size).div_ceil(ALIGN) * ALIGN;
                out.push(m);
                continue;
            }
        }
        pos += ALIGN;
        if pos - last_report > 256 << 20 {
            last_report = pos;
            progress("scan", pos, len);
        }
    }
    progress("scan", len, len);
    Ok(out)
}

fn read_full(f: &mut std::fs::File, buf: &mut [u8]) -> Result<usize> {
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

/// Copy one movie out (streamed, 8 MiB at a time).
pub fn extract(archive: &Path, m: &Movie, dest: &Path) -> Result<()> {
    let mut f = std::fs::File::open(archive)?;
    f.seek(SeekFrom::Start(m.offset))?;
    let mut w = crate::atomic::AtomicFile::create(dest)?;
    let mut left = m.size;
    let mut buf = vec![0u8; 8 << 20];
    while left > 0 {
        let n = left.min(buf.len() as u64) as usize;
        f.read_exact(&mut buf[..n])?;
        w.write_all(&buf[..n])?;
        left -= n as u64;
    }
    w.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_bink2_header() {
        let mut b = vec![0u8; 48 + 12 + 3 * 4];
        b[0..4].copy_from_slice(b"KB2n");
        let size = b.len() as u32 + 100;
        b[4..8].copy_from_slice(&(size - 8).to_le_bytes());
        for (o, v) in [(8, 2u32), (16, 2), (20, 1920), (24, 1080), (28, 30), (32, 1), (40, 1)] {
            b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        b[52..56].copy_from_slice(&(48000u32 | (0x2000 << 16)).to_le_bytes());
        let t = 60;
        for (i, v) in [72u32 | 1, 120, size].iter().enumerate() {
            b[t + i * 4..t + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        let m = parse(&b, 0, 1 << 20).unwrap();
        assert_eq!((m.width, m.height, m.frames, m.keyframes), (1920, 1080, 2, 1));
        assert_eq!(m.audio[0].sample_rate, 48000);
        assert!(m.audio[0].stereo && m.frame_table_ok);
    }
}
