//! Carving scanner: finds every zlib stream in an RPF9 archive's data area.
//!
//! The TOC is encrypted, but entry payloads are plain zlib. Entries are
//! 16-byte aligned. Large entries are either one zlib stream or a run of
//! back-to-back streams that each inflate to exactly 64 KiB (last one shorter).
//!
//! The scan is streamed (fixed-size windows, never the whole file), split into
//! segments that run in parallel and are checkpointed to disk, so an
//! interrupted scan resumes where it stopped.
use crate::classify::{classify, Kind};
use anyhow::Result;
use flate2::{Decompress, FlushDecompress, Status};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
#[allow(unused_imports)]
use crate::classify::Kind as _K;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub const SEGMENT: u64 = 256 << 20;
const WINDOW: usize = 32 << 20;
pub const HEAD_LEN: usize = 256;
pub const CHUNK: u64 = 65536;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawStream {
    pub off: u64,
    pub csize: u64,
    pub usize: u64,
    #[serde(with = "hexbytes")]
    pub head: Vec<u8>,
}

mod hexbytes {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&crate::rpf9::hex(v))
    }
    pub fn deserialize<'d, D: Deserializer<'d>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        Ok((0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap_or(0))
            .collect())
    }
}

fn zlib_header_ok(a: u8, b: u8) -> bool {
    a == 0x78 && matches!(b, 0x01 | 0x5e | 0x9c | 0xda) && ((a as u16) << 8 | b as u16) % 31 == 0
}

/// Inflate a zlib stream starting at `off`. `pre` holds already-read bytes
/// starting at `off`; more is read from `file` if the stream runs past it.
/// When `sink` is Some, the full output is appended to it.
pub fn inflate_at(
    file: &mut File,
    off: u64,
    pre: &[u8],
    mut sink: Option<&mut Vec<u8>>,
) -> Option<RawStream> {
    let mut d = Decompress::new(true);
    let mut out = vec![0u8; 256 * 1024];
    let mut head = Vec::new();
    // Phase 1: bytes already in memory.
    let mut pos = 0usize;
    while pos < pre.len() {
        match step(&mut d, pre, &mut pos, &mut out, &mut head, &mut sink) {
            Step::Continue => {}
            Step::End => return Some(RawStream { off, csize: d.total_in(), usize: d.total_out(), head }),
            Step::Err => return None,
        }
    }
    // Phase 2: stream from the file.
    let mut next = off + pre.len() as u64;
    file.seek(SeekFrom::Start(next)).ok()?;
    let mut buf = vec![0u8; 4 << 20];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        next += n as u64;
        let mut p = 0usize;
        while p < n {
            match step(&mut d, &buf[..n], &mut p, &mut out, &mut head, &mut sink) {
                Step::Continue => {}
                Step::End => return Some(RawStream { off, csize: d.total_in(), usize: d.total_out(), head }),
                Step::Err => return None,
            }
        }
        let _ = next;
    }
}

enum Step {
    Continue,
    End,
    Err,
}

fn step(
    d: &mut Decompress,
    src: &[u8],
    pos: &mut usize,
    out: &mut [u8],
    head: &mut Vec<u8>,
    sink: &mut Option<&mut Vec<u8>>,
) -> Step {
    let before_in = d.total_in();
    let before_out = d.total_out();
    let r = d.decompress(&src[*pos..], out, FlushDecompress::None);
    let used = (d.total_in() - before_in) as usize;
    let produced = (d.total_out() - before_out) as usize;
    *pos += used;
    if head.len() < HEAD_LEN {
        let take = (HEAD_LEN - head.len()).min(produced);
        head.extend_from_slice(&out[..take]);
    }
    if let Some(s) = sink.as_mut() {
        s.extend_from_slice(&out[..produced]);
    }
    match r {
        Ok(Status::StreamEnd) => Step::End,
        Ok(_) => {
            if used == 0 && produced == 0 && *pos < src.len() {
                Step::Err
            } else {
                Step::Continue
            }
        }
        Err(_) => Step::Err,
    }
}

/// Scan one segment `[start, end)` of the file. Streams that start inside the
/// segment are returned, even if they run past its end.
pub fn scan_segment(path: &Path, start: u64, end: u64, progress: &AtomicU64) -> Result<Vec<RawStream>> {
    let mut f = File::open(path)?;
    let mut f2 = File::open(path)?;
    let mut res = Vec::new();
    let mut pos = (start + 15) & !15;
    let mut win = vec![0u8; WINDOW];
    let mut win_off = u64::MAX;
    let mut win_len = 0usize;
    let mut last_report = pos;
    while pos < end {
        if win_off == u64::MAX || pos < win_off || pos + 2 > win_off + win_len as u64 {
            f.seek(SeekFrom::Start(pos))?;
            let mut n = 0;
            while n < WINDOW {
                let r = f.read(&mut win[n..])?;
                if r == 0 {
                    break;
                }
                n += r;
            }
            if n < 2 {
                break;
            }
            win_off = pos;
            win_len = n;
        }
        let i = (pos - win_off) as usize;
        if zlib_header_ok(win[i], win[i + 1]) {
            if let Some(s) = inflate_at(&mut f2, pos, &win[i..win_len], None) {
                if s.usize > 0 {
                    let next = (pos + s.csize + 15) & !15;
                    res.push(s);
                    pos = next;
                    if pos - last_report > (8 << 20) {
                        progress.fetch_add(pos - last_report, Ordering::Relaxed);
                        last_report = pos;
                    }
                    continue;
                }
            }
        }
        pos += 16;
        if pos - last_report > (8 << 20) {
            progress.fetch_add(pos - last_report, Ordering::Relaxed);
            last_report = pos;
        }
    }
    let fin = end.max(last_report);
    progress.fetch_add(fin.saturating_sub(last_report).min(end.saturating_sub(last_report)), Ordering::Relaxed);
    Ok(res)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Stable id: "<archive id>@<offset hex>"
    pub id: String,
    pub off: u64,
    /// Total compressed bytes (all streams).
    pub csize: u64,
    /// Total decompressed bytes (all streams).
    pub usize: u64,
    /// Every zlib stream of this entry: [offset, compressed, decompressed].
    /// For RAGE resources stream 0 is the system (0x5...) page and the rest,
    /// concatenated, are the graphics (0x6...) pages.
    pub streams: Vec<[u64; 3]>,
    pub kind: Kind,
    /// Sub type: resource type id (PRD0), "txd", or magic hex.
    pub sub: String,
    /// Suggested file extension.
    pub ext: String,
    /// Recovered name (resource name, dictionary name) when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Resource type tag from the name suffix (dr, ft, dd, td, ...) or "txd".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtype: Option<String>,
    /// Names of drawable dictionary members, recovered with the system page.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveIndex {
    pub tool_version: String,
    /// Archive id: path relative to the game root, forward slashes.
    pub archive: String,
    pub path: String,
    pub size: u64,
    pub mtime: u64,
    pub header: Option<crate::rpf9::Rpf9Header>,
    pub container: String,
    pub entries: Vec<Entry>,
    pub stats: IndexStats,
    #[serde(default)]
    pub names_done: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexStats {
    pub streams: u64,
    pub entries: u64,
    pub compressed_bytes: u64,
    pub uncompressed_bytes: u64,
    /// Fraction of the archive's bytes covered by zlib streams.
    pub coverage: f64,
    pub by_kind: std::collections::BTreeMap<String, (u64, u64)>,
}

fn starts_new_file(kind: Kind) -> bool {
    matches!(kind, Kind::Resource | Kind::RageObj | Kind::Pso | Kind::Rbf | Kind::Gfx | Kind::Dds | Kind::Png)
}

/// Merge raw streams into entries.
/// * back-to-back streams after a stream that inflated to exactly 64 KiB are
///   continuation chunks of one file;
/// * other back-to-back streams are extra pages of the same resource unless
///   the stream itself looks like the start of a new file.
pub fn group_streams(archive_id: &str, mut streams: Vec<RawStream>) -> Vec<Entry> {
    streams.sort_by_key(|s| s.off);
    let mut clean: Vec<RawStream> = Vec::with_capacity(streams.len());
    for s in streams {
        if let Some(p) = clean.last() {
            if s.off < p.off + p.csize {
                continue;
            }
        }
        clean.push(s);
    }
    let mut out: Vec<Entry> = Vec::new();
    let mut next_off = u64::MAX;
    let mut last_u = 0u64;
    for s in clean {
        let aligned_end = (s.off + s.csize + 15) & !15;
        let (kind, sub, ext) = classify(&s.head);
        let contiguous = s.off == next_off;
        // Resources start on a 4 KiB boundary + 0x10. A contiguous RAGE
        // object stream that does not is a later system-page group of the
        // previous resource (big fragments put the root page first).
        let misaligned_obj = kind == Kind::RageObj && s.off % 4096 != 0x10;
        let cont = contiguous
            && (last_u == CHUNK || !starts_new_file(kind) || misaligned_obj)
            && out.last().is_some();
        if cont {
            let e = out.last_mut().unwrap();
            e.csize += s.csize;
            e.usize += s.usize;
            e.streams.push([s.off, s.csize, s.usize]);
        } else {
            out.push(Entry {
                id: format!("{}@{:x}", archive_id, s.off),
                off: s.off,
                csize: s.csize,
                usize: s.usize,
                streams: vec![[s.off, s.csize, s.usize]],
                kind,
                sub,
                ext,
                name: None,
                rtype: None,
                members: Vec::new(),
            });
        }
        next_off = aligned_end;
        last_u = s.usize;
    }
    out
}

pub fn archive_stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Progress callback: (bytes_done, bytes_total).
pub type ProgressFn = dyn Fn(u64, u64) + Sync + Send;

/// Build (or resume) the index for one archive. Segment results are cached
/// under `work/index/<stem>.segs/`, the final index is `work/index/<stem>.json`.
pub fn build_index(
    archive: &Path,
    archive_id: &str,
    index_dir: &Path,
    force: bool,
    progress: Option<Arc<ProgressFn>>,
) -> Result<ArchiveIndex> {
    let stem = crate::game::index_stem(archive_id);
    let meta = fs::metadata(archive)?;
    let size = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let final_path = index_dir.join(format!("{stem}.json"));
    if !force {
        if let Ok(ix) = load_index(&final_path) {
            if ix.size == size && ix.mtime == mtime && ix.tool_version == crate::INDEX_VERSION {
                return Ok(ix);
            }
        }
    }
    let header = crate::rpf9::read_header(archive).ok();
    let container = if header.is_some() { "rpf9".to_string() } else { crate::game::container_kind(archive) };
    let seg_dir = index_dir.join(format!("{stem}.segs"));
    fs::create_dir_all(&seg_dir)?;
    let data_start = header.as_ref().map(|h| ((h.toc_end as u64) + 15) & !15).unwrap_or(0);
    let nseg = size.div_ceil(SEGMENT);
    let done = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    // reporter thread
    let rep = progress.clone().map(|p| {
        let done = done.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                p(done.load(Ordering::Relaxed).min(size), size);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        })
    });
    let segs: Vec<u64> = (0..nseg).collect();
    let results: Vec<Result<Vec<RawStream>>> = segs
        .par_iter()
        .map(|&i| {
            let s = (i * SEGMENT).max(data_start);
            let e = ((i + 1) * SEGMENT).min(size);
            let cp = seg_dir.join(format!("{:05}_{}_{}.json", i, size, mtime));
            if !force {
                if let Ok(txt) = fs::read(&cp) {
                    if let Ok(v) = serde_json::from_slice::<Vec<RawStream>>(&txt) {
                        done.fetch_add(e.saturating_sub(i * SEGMENT), Ordering::Relaxed);
                        return Ok(v);
                    }
                }
            }
            if s >= e {
                done.fetch_add(e.saturating_sub(i * SEGMENT), Ordering::Relaxed);
                return Ok(vec![]);
            }
            done.fetch_add(s - i * SEGMENT, Ordering::Relaxed);
            let v = scan_segment(archive, s, e, &done)?;
            let tmp = cp.with_extension("tmp");
            fs::write(&tmp, serde_json::to_vec(&v)?)?;
            fs::rename(&tmp, &cp)?;
            Ok(v)
        })
        .collect();
    stop.store(true, Ordering::Relaxed);
    if let Some(h) = rep {
        let _ = h.join();
    }
    let mut all = Vec::new();
    for r in results {
        all.extend(r?);
    }
    let nstreams = all.len() as u64;
    let mut entries = group_streams(archive_id, all);
    for e in entries.iter_mut() {
        crate::classify::refine(e);
    }
    let mut stats = IndexStats { streams: nstreams, entries: entries.len() as u64, ..Default::default() };
    for e in &entries {
        stats.compressed_bytes += e.csize;
        stats.uncompressed_bytes += e.usize;
        let k = stats.by_kind.entry(format!("{:?}", e.kind).to_lowercase()).or_default();
        k.0 += 1;
        k.1 += e.usize;
    }
    stats.coverage = stats.compressed_bytes as f64 / size as f64;
    let ix = ArchiveIndex {
        tool_version: crate::INDEX_VERSION.into(),
        archive: archive_id.to_string(),
        path: archive.display().to_string(),
        size,
        mtime,
        header,
        container,
        entries,
        stats,
        names_done: false,
    };
    fs::create_dir_all(index_dir)?;
    let tmp = final_path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec(&ix)?)?;
    fs::rename(&tmp, &final_path)?;
    Ok(ix)
}

pub fn load_index(path: &Path) -> Result<ArchiveIndex> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

/// Read and decompress one stream.
pub fn read_stream(f: &mut File, st: &[u64; 3]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(st[2] as usize);
    inflate_at(f, st[0], &[], Some(&mut out)).ok_or_else(|| anyhow::anyhow!("inflate failed at {:#x}", st[0]))?;
    Ok(out)
}

/// Read and fully decompress one entry (all streams concatenated).
pub fn read_entry(archive: &Path, e: &Entry) -> Result<Vec<u8>> {
    let mut f = File::open(archive)?;
    let mut out = Vec::with_capacity(e.usize as usize);
    for st in &e.streams {
        inflate_at(&mut f, st[0], &[], Some(&mut out)).ok_or_else(|| anyhow::anyhow!("inflate failed at {:#x}", st[0]))?;
    }
    Ok(out)
}

pub fn index_path(index_dir: &Path, archive_id: &str) -> PathBuf {
    index_dir.join(format!("{}.json", crate::game::index_stem(archive_id)))
}

pub fn save_index(index_dir: &Path, ix: &ArchiveIndex) -> Result<()> {
    let p = index_path(index_dir, &ix.archive);
    let tmp = p.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec(ix)?)?;
    fs::rename(&tmp, &p)?;
    Ok(())
}
