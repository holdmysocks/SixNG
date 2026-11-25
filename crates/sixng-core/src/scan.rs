//! Carving scanner: finds every zlib stream in an RPF9 archive's data area.
//!
//! The TOC is encrypted, but entry payloads are plain zlib. Entries are
//! 16-byte aligned. Large entries are either one zlib stream or a run of
//! back-to-back streams that each inflate to exactly 64 KiB (last one shorter).
//! A 64 KiB chunk that does not compress is stored raw instead: exactly
//! 65,536 bytes with no zlib header, at the next 16-byte aligned offset; the
//! chunk after it is zlib again. Such a raw chunk is recorded as a stream
//! whose compressed and decompressed sizes are equal (a zlib stream is always
//! larger than its output), see [`is_raw`].
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
/// Version of the raw stream scan; part of the segment cache file names so
/// a scanner change re-scans instead of reusing old segment results.
const SCAN_VERSION: u32 = 2;
/// Most raw chunks in a row accepted inside one chunked entry.
const MAX_RAW_RUN: usize = 8;
/// Most chunks followed in one chunked entry (1 GiB).
const MAX_RUN_CHUNKS: usize = 16 << 10;

/// True when a stream `[offset, compressed, decompressed]` is a stored,
/// uncompressed chunk rather than a zlib stream.
pub fn is_raw(st: &[u64; 3]) -> bool {
    st[1] == st[2] && st[1] > 0
}

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

/// True when a zlib stream that ends inside `buf` starts at some 16-byte
/// aligned offset of it (a raw chunk never holds one).
fn holds_complete_stream(buf: &[u8]) -> bool {
    let mut out = vec![0u8; 64 * 1024];
    let mut at = 0;
    while at + 2 <= buf.len() {
        if zlib_header_ok(buf[at], buf[at + 1]) {
            let mut d = Decompress::new(true);
            loop {
                let src = &buf[at + d.total_in() as usize..];
                let (bi, bo) = (d.total_in(), d.total_out());
                match d.decompress(src, &mut out, FlushDecompress::None) {
                    Ok(Status::StreamEnd) => {
                        if d.total_out() > 0 {
                            return true;
                        }
                        break;
                    }
                    Ok(_) if d.total_in() == bi && d.total_out() == bo => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
        at += 16;
    }
    false
}

/// Follow a chunked entry from `pos`, the aligned end of a zlib chunk that
/// inflated to exactly 64 KiB, when the bytes there are not a zlib stream.
/// Each chunk is either a zlib stream or 64 KiB stored raw. The run is
/// returned only when it holds at least one raw chunk and ends in a zlib
/// chunk shorter than 64 KiB; anything else (too many raw chunks in a row, a
/// stream inflating past 64 KiB, a "raw" chunk that holds a whole zlib
/// stream, end of file) gives None and the bytes are scanned as before.
fn raw_run(f: &mut File, pos: u64) -> Option<Vec<RawStream>> {
    let mut run: Vec<RawStream> = Vec::new();
    let mut p = pos;
    let mut raw_in_row = 0usize;
    let mut buf = vec![0u8; CHUNK as usize];
    while run.len() < MAX_RUN_CHUNKS {
        f.seek(SeekFrom::Start(p)).ok()?;
        let mut hdr = [0u8; 2];
        f.read_exact(&mut hdr).ok()?;
        let z = if zlib_header_ok(hdr[0], hdr[1]) { inflate_at(f, p, &[], None).filter(|s| s.usize > 0) } else { None };
        if let Some(s) = z {
            if s.usize > CHUNK {
                return None;
            }
            let (short, next) = (s.usize < CHUNK, (p + s.csize + 15) & !15);
            run.push(s);
            if short {
                return run.iter().any(|s| s.csize == s.usize).then_some(run);
            }
            raw_in_row = 0;
            p = next;
            continue;
        }
        if raw_in_row >= MAX_RAW_RUN {
            return None;
        }
        f.seek(SeekFrom::Start(p)).ok()?;
        f.read_exact(&mut buf).ok()?;
        if holds_complete_stream(&buf) {
            return None;
        }
        run.push(RawStream { off: p, csize: CHUNK, usize: CHUNK, head: buf[..HEAD_LEN].to_vec() });
        raw_in_row += 1;
        p += CHUNK;
    }
    None
}

/// Scan one segment `[start, end)` of the file. Streams that start inside the
/// segment are returned, even if they run past its end.
pub fn scan_segment(path: &Path, start: u64, end: u64, progress: &AtomicU64) -> Result<Vec<RawStream>> {
    scan_segment_with(path, start, end, progress, true)
}

/// [`scan_segment`]; `follow_raw` false is scan 1, which did not follow
/// stored 64 KiB chunks (kept for the upgrade tests).
fn scan_segment_with(path: &Path, start: u64, end: u64, progress: &AtomicU64, follow_raw: bool) -> Result<Vec<RawStream>> {
    let mut f = File::open(path)?;
    let mut f2 = File::open(path)?;
    let mut res = Vec::new();
    let mut pos = (start + 15) & !15;
    let mut win = vec![0u8; WINDOW];
    let mut win_off = u64::MAX;
    let mut win_len = 0usize;
    let mut last_report = pos;
    // Aligned end of the last stream when it inflated to exactly 64 KiB:
    // the next chunk of the same entry may be stored raw there.
    let mut chunk_end = u64::MAX;
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
        let zlib = if zlib_header_ok(win[i], win[i + 1]) {
            inflate_at(&mut f2, pos, &win[i..win_len], None).filter(|s| s.usize > 0)
        } else {
            None
        };
        let found = match zlib {
            Some(s) => {
                let next = (pos + s.csize + 15) & !15;
                chunk_end = if s.usize == CHUNK { next } else { u64::MAX };
                res.push(s);
                Some(next)
            }
            None if follow_raw && pos == chunk_end => {
                chunk_end = u64::MAX;
                raw_run(&mut f2, pos).map(|run| {
                    let last = run.last().unwrap();
                    let next = (last.off + last.csize + 15) & !15;
                    res.extend(run);
                    next
                })
            }
            None => None,
        };
        if let Some(next) = found {
            pos = next;
            if pos - last_report > (8 << 20) {
                progress.fetch_add(pos - last_report, Ordering::Relaxed);
                last_report = pos;
            }
            continue;
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

/// System-page bytes a RAGE object needs: its page-map pointer (+0x08, a
/// 0x5xxxxxxx system pointer) plus the 8-byte map header. 0 when the head
/// has no such pointer.
fn sys_need_of(kind: Kind, head: &[u8]) -> u64 {
    if kind != Kind::RageObj {
        return 0;
    }
    let pm = head.get(8..16).map(|b| u64::from_le_bytes(b.try_into().unwrap())).unwrap_or(0);
    if pm >> 28 == 5 { (pm & 0x0FFF_FFFF) + 8 } else { 0 }
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
    // Bytes of system page the current entry needs (its page map sits at
    // the end of it), 0 when unknown.
    let mut sys_need = 0u64;
    for s in clean {
        let aligned_end = (s.off + s.csize + 15) & !15;
        let (kind, sub, ext) = classify(&s.head);
        let contiguous = s.off == next_off;
        // Resources start on a 4 KiB boundary + 0x10. A contiguous RAGE
        // object stream that does not is a later system-page group of the
        // previous resource (big fragments put the root page first).
        let misaligned_obj = kind == Kind::RageObj && s.off % 4096 != 0x10;
        // A later system-page group can also land on such a boundary: the
        // previous entry's page-map pointer still lies past the bytes it
        // has (a texture dictionary whose first stream inflates to 0x30000
        // with its page map at 0x31000, split from its own graphics pages).
        let sys_short = out.last().is_some_and(|e| e.usize < sys_need);
        let cont = contiguous
            && (last_u == CHUNK || !starts_new_file(kind) || misaligned_obj || sys_short)
            && out.last().is_some();
        if !cont {
            sys_need = sys_need_of(kind, &s.head);
        }
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
/// Progress callback with a stage: (stage, bytes_done, bytes_total). The
/// stage is "scan", or "upgrade" while an older index is brought up to date.
pub type StageProgressFn = dyn Fn(&str, u64, u64) + Sync + Send;

/// What an `INDEX_VERSION` step changed, so what an index written before it
/// needs to become current.
///
/// Entries are always grouped again from the saved segment streams (that is
/// cheap), so a change to grouping or classification needs no rescan. The
/// raw stream scan has its own version, [`SCAN_VERSION`], in the segment file
/// names: a segment from an older scan is either upgraded in place (see
/// [`scan1_segment_is_current`]) or scanned again. What is left for a step to
/// say is whether the saved names can be kept, or whether nothing old can be
/// trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepNeeds {
    /// Entries come from the segments again; a name, type and member list
    /// is kept for an entry whose streams are unchanged.
    Regroup,
    /// As `Regroup`, but names recovered from a "<name>.#<tag>" string may
    /// change, so those entries get their names again. Texture dictionary
    /// names and entries without a name are kept.
    RenameTagged,
    /// Nothing old is reused: segments are scanned again and every name is
    /// recovered again.
    Rescan,
}

/// Every `INDEX_VERSION` step, oldest first: (from, to, needs, what changed).
/// A version bump adds its step here (a test checks the chain ends at the
/// current version); an index older than the first step is rebuilt from its
/// segments with every name recovered again.
pub const INDEX_STEPS: &[(&str, &str, StepNeeds, &str)] = &[
    ("5", "6", StepNeeds::Regroup, "a resource whose system page continues on a resource boundary stays one entry"),
    ("6", "7", StepNeeds::Regroup, "stored (uncompressed) 64 KiB chunks join their chunked entry; scan 1 -> 2"),
    ("7", "8", StepNeeds::RenameTagged, "a resource name starts where its pointer points"),
];

/// What an index written by `from` needs to become current: the steps in
/// order, folded to the strongest need. `None` when `from` is not in the
/// chain (too old, or unknown): entries come from the segments again and
/// every name is recovered again.
pub fn upgrade_needs(from: &str) -> Option<StepNeeds> {
    if from == crate::INDEX_VERSION {
        return Some(StepNeeds::Regroup);
    }
    let mut at = INDEX_STEPS.iter().position(|s| s.0 == from)?;
    let mut needs = StepNeeds::Regroup;
    loop {
        let (_, to, n, _) = INDEX_STEPS[at];
        needs = match (needs, n) {
            (StepNeeds::Rescan, _) | (_, StepNeeds::Rescan) => StepNeeds::Rescan,
            (StepNeeds::RenameTagged, _) | (_, StepNeeds::RenameTagged) => StepNeeds::RenameTagged,
            _ => StepNeeds::Regroup,
        };
        if to == crate::INDEX_VERSION {
            return Some(needs);
        }
        at = INDEX_STEPS.iter().position(|s| s.0 == to)?;
    }
}

/// True when the saved name of an entry from an older index can be kept
/// under `needs` (the entry's streams being unchanged).
fn name_kept(needs: StepNeeds, old: &Entry) -> bool {
    match needs {
        StepNeeds::Regroup => true,
        StepNeeds::RenameTagged => old.rtype.is_none() || old.rtype.as_deref() == Some("txd"),
        StepNeeds::Rescan => false,
    }
}

/// A scan 1 segment (before stored 64 KiB chunks were followed) is what scan
/// 2 finds as well, unless scan 2 follows a run of stored chunks somewhere in
/// it. Scan 2 only does something different at the aligned end of a stream
/// that inflated to exactly 64 KiB when no stream starts there: it tries
/// [`raw_run`]. Up to the first such place both scans take the same path, and
/// where `raw_run` finds nothing both go on the same way. So the segment is
/// current when `raw_run` finds nothing at each of those places (a few
/// 64 KiB reads, not a rescan). `streams` is the segment's scan 1 result in
/// the order found, `end` the segment's end.
pub fn scan1_segment_is_current(f: &mut File, streams: &[RawStream], end: u64) -> bool {
    for (k, s) in streams.iter().enumerate() {
        if s.usize != CHUNK {
            continue;
        }
        let p = (s.off + s.csize + 15) & !15;
        if p >= end || streams.get(k + 1).is_some_and(|n| n.off == p) {
            continue;
        }
        if raw_run(f, p).is_some() {
            return false;
        }
    }
    true
}

/// How `build_index` got an archive's index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexStatus {
    /// The saved index was current; nothing was read.
    Reused,
    /// An index of an older version was brought up to date from its saved
    /// segments, scanning again only the segments the scan change touched.
    Upgraded,
    /// An interrupted scan went on from its saved segments.
    Resumed,
    /// The whole archive was scanned (`--force`, the archive changed, or an
    /// old index whose scan cannot be upgraded).
    Rescanned,
    /// First index of this archive.
    New,
}

/// What `build_index` did for one archive.
#[derive(Debug, Clone, Serialize)]
pub struct IndexOutcome {
    pub status: IndexStatus,
    /// INDEX_VERSION of the index found, when there was one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_version: Option<String>,
    /// Segments taken as saved, upgraded from an older scan, scanned.
    pub segments_reused: u64,
    pub segments_upgraded: u64,
    pub segments_scanned: u64,
    /// Entries whose names were kept from the older index.
    pub names_kept: u64,
    /// Entries (indices into `entries`) that still need the name pass when
    /// the rest was kept. Empty with `names_done` false: all need it.
    #[serde(skip)]
    pub names_todo: Vec<usize>,
}

impl IndexOutcome {
    /// Short text for logs and the viewer, e.g. "upgraded from 6 (3 of 284
    /// segments scanned)".
    pub fn summary(&self) -> String {
        let segs = self.segments_reused + self.segments_upgraded + self.segments_scanned;
        match self.status {
            IndexStatus::Reused => "reused".into(),
            IndexStatus::Upgraded => format!(
                "upgraded from {} ({} of {} segments scanned)",
                self.from_version.as_deref().unwrap_or("?"),
                self.segments_scanned,
                segs
            ),
            IndexStatus::Resumed => format!("resumed ({} of {} segments scanned)", self.segments_scanned, segs),
            IndexStatus::Rescanned => match &self.from_version {
                Some(v) if v != crate::INDEX_VERSION => format!("rescanned (was version {v})"),
                _ => "rescanned".into(),
            },
            IndexStatus::New => "new".into(),
        }
    }
}

/// Build (or resume) the index for one archive. Segment results are cached
/// under `work/index/<stem>.segs/`, the final index is `work/index/<stem>.json`.
pub fn build_index(
    archive: &Path,
    archive_id: &str,
    index_dir: &Path,
    force: bool,
    progress: Option<Arc<ProgressFn>>,
) -> Result<ArchiveIndex> {
    let p = progress.map(|p| Arc::new(move |_: &str, d, t| p(d, t)) as Arc<StageProgressFn>);
    Ok(build_index_report(archive, archive_id, index_dir, force, p)?.0)
}

fn seg_path(seg_dir: &Path, i: u64, size: u64, mtime: u64, scan: u32) -> PathBuf {
    if scan == 1 {
        // scan 1 had no version in the name
        seg_dir.join(format!("{:05}_{}_{}.json", i, size, mtime))
    } else {
        seg_dir.join(format!("{:05}_{}_{}_s{}.json", i, size, mtime, scan))
    }
}

/// Where one segment's streams came from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SegFrom {
    Empty,
    Saved,
    Upgraded,
    Scanned,
}

/// [`build_index`], also saying whether the index was reused, upgraded from
/// an older version or scanned (see [`IndexStatus`]).
///
/// An index of an older `INDEX_VERSION` is upgraded where [`INDEX_STEPS`]
/// allows: entries are grouped again from the saved segments (an older scan's
/// segments are upgraded or scanned again one by one), and the names of
/// unchanged entries are kept, so only the rest needs the name pass
/// ([`IndexOutcome::names_todo`]).
pub fn build_index_report(
    archive: &Path,
    archive_id: &str,
    index_dir: &Path,
    force: bool,
    progress: Option<Arc<StageProgressFn>>,
) -> Result<(ArchiveIndex, IndexOutcome)> {
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
    // The saved index, when it is of this archive as it is now.
    let mut old: Option<ArchiveIndex> = None;
    let mut found_version = None;
    if !force {
        if let Ok(ix) = load_index(&final_path) {
            found_version = Some(ix.tool_version.clone());
            if ix.size == size && ix.mtime == mtime {
                if ix.tool_version == crate::INDEX_VERSION {
                    let outcome = IndexOutcome {
                        status: IndexStatus::Reused,
                        from_version: found_version,
                        segments_reused: 0,
                        segments_upgraded: 0,
                        segments_scanned: 0,
                        names_kept: 0,
                        names_todo: vec![],
                    };
                    return Ok((ix, outcome));
                }
                old = Some(ix);
            }
        }
    }
    let needs = old.as_ref().map(|ix| upgrade_needs(&ix.tool_version));
    // Segments can be trusted unless a step says nothing old can.
    let reuse_segments = !force && needs != Some(Some(StepNeeds::Rescan));
    let stage = if old.is_some() && reuse_segments { "upgrade" } else { "scan" };
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
                p(stage, done.load(Ordering::Relaxed).min(size), size);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        })
    });
    let save = |cp: &Path, v: &Vec<RawStream>| -> Result<()> {
        let tmp = cp.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(v)?)?;
        fs::rename(&tmp, cp)?;
        Ok(())
    };
    let read_seg = |p: &Path| fs::read(p).ok().and_then(|t| serde_json::from_slice::<Vec<RawStream>>(&t).ok());
    let segs: Vec<u64> = (0..nseg).collect();
    let results: Vec<Result<(SegFrom, Vec<RawStream>)>> = segs
        .par_iter()
        .map(|&i| {
            let s = (i * SEGMENT).max(data_start);
            let e = ((i + 1) * SEGMENT).min(size);
            let cp = seg_path(&seg_dir, i, size, mtime, SCAN_VERSION);
            let span = e.saturating_sub(i * SEGMENT);
            if reuse_segments {
                if let Some(v) = read_seg(&cp) {
                    done.fetch_add(span, Ordering::Relaxed);
                    return Ok((SegFrom::Saved, v));
                }
                // A scan 1 segment (SCAN_VERSION 2 only follows stored chunks
                // in addition) is current unless scan 2 would follow one.
                if s < e {
                    if let Some(v) = read_seg(&seg_path(&seg_dir, i, size, mtime, 1)) {
                        if scan1_segment_is_current(&mut File::open(archive)?, &v, e) {
                            save(&cp, &v)?;
                            done.fetch_add(span, Ordering::Relaxed);
                            return Ok((SegFrom::Upgraded, v));
                        }
                    }
                }
            }
            if s >= e {
                done.fetch_add(span, Ordering::Relaxed);
                return Ok((SegFrom::Empty, vec![]));
            }
            done.fetch_add(s - i * SEGMENT, Ordering::Relaxed);
            let v = scan_segment(archive, s, e, &done)?;
            save(&cp, &v)?;
            Ok((SegFrom::Scanned, v))
        })
        .collect();
    stop.store(true, Ordering::Relaxed);
    if let Some(h) = rep {
        let _ = h.join();
    }
    let mut all = Vec::new();
    let (mut n_saved, mut n_upgraded, mut n_scanned) = (0u64, 0u64, 0u64);
    for r in results {
        let (from, v) = r?;
        match from {
            SegFrom::Saved => n_saved += 1,
            SegFrom::Upgraded => n_upgraded += 1,
            SegFrom::Scanned => n_scanned += 1,
            SegFrom::Empty => {}
        }
        all.extend(v);
    }
    let nstreams = all.len() as u64;
    let mut entries = group_streams(archive_id, all);
    for e in entries.iter_mut() {
        crate::classify::refine(e);
    }
    // Names of entries that did not change, where the steps allow it.
    let mut names_kept = 0u64;
    let mut names_todo = Vec::new();
    let mut names_done = false;
    if let (Some(o), Some(Some(n))) = (old.as_ref(), needs) {
        if o.names_done && n != StepNeeds::Rescan {
            let by_off: std::collections::HashMap<u64, &Entry> = o.entries.iter().map(|e| (e.off, e)).collect();
            for (i, e) in entries.iter_mut().enumerate() {
                if !matches!(e.kind, Kind::RageObj | Kind::Resource) {
                    continue;
                }
                match by_off.get(&e.off) {
                    Some(oe) if oe.streams == e.streams && oe.kind == e.kind && name_kept(n, oe) => {
                        e.name = oe.name.clone();
                        e.rtype = oe.rtype.clone();
                        e.members = oe.members.clone();
                        names_kept += 1;
                    }
                    _ => names_todo.push(i),
                }
            }
            names_done = names_todo.is_empty();
        }
    }
    let partial = n_scanned < n_saved + n_upgraded + n_scanned;
    let status = if old.is_some() && partial {
        IndexStatus::Upgraded
    } else if old.is_none() && partial {
        IndexStatus::Resumed
    } else if found_version.is_none() && !force {
        IndexStatus::New
    } else {
        IndexStatus::Rescanned
    };
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
        names_done,
    };
    fs::create_dir_all(index_dir)?;
    let tmp = final_path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec(&ix)?)?;
    fs::rename(&tmp, &final_path)?;
    let outcome = IndexOutcome {
        status,
        from_version: found_version,
        segments_reused: n_saved,
        segments_upgraded: n_upgraded,
        segments_scanned: n_scanned,
        names_kept,
        names_todo: if names_done || names_kept == 0 { vec![] } else { names_todo },
    };
    Ok((ix, outcome))
}

pub fn load_index(path: &Path) -> Result<ArchiveIndex> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

/// Read and decompress one stream (or copy a raw chunk, see [`is_raw`]).
pub fn read_stream(f: &mut File, st: &[u64; 3]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(st[2] as usize);
    read_stream_into(f, st, &mut out)?;
    Ok(out)
}

fn read_stream_into(f: &mut File, st: &[u64; 3], out: &mut Vec<u8>) -> Result<()> {
    if is_raw(st) {
        let at = out.len();
        out.resize(at + st[2] as usize, 0);
        f.seek(SeekFrom::Start(st[0]))?;
        f.read_exact(&mut out[at..])?;
        return Ok(());
    }
    inflate_at(f, st[0], &[], Some(out)).ok_or_else(|| anyhow::anyhow!("inflate failed at {:#x}", st[0]))?;
    Ok(())
}

/// Read and fully decompress one entry (all streams concatenated).
pub fn read_entry(archive: &Path, e: &Entry) -> Result<Vec<u8>> {
    let mut f = File::open(archive)?;
    let mut out = Vec::with_capacity(e.usize as usize);
    for st in &e.streams {
        read_stream_into(&mut f, st, &mut out)?;
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

#[cfg(test)]
mod group_tests {
    use super::*;

    /// A RAGE object head: page-map pointer `pm` at +0x08.
    fn obj(off: u64, csize: u64, usize: u64, pm: u64) -> RawStream {
        let mut head = vec![0u8; 64];
        head[8..16].copy_from_slice(&pm.to_le_bytes());
        RawStream { off, csize, usize, head }
    }

    #[test]
    fn later_system_page_on_a_resource_boundary_stays_with_its_resource() {
        // Root system stream of 0x30000 bytes whose page map is at 0x31000;
        // the next contiguous stream starts on a 4 KiB + 0x10 boundary and
        // looks like a RAGE object, but is the rest of the system page.
        let a = obj(0x1010, 0xBFF8, 0x30000, 0x5003_1000);
        let b = obj(0xD010, 0x500, 0x5DD0, 0x5000_2000);
        let g = RawStream { off: 0xD510, csize: 0x100, usize: 0x1000, head: vec![0xAB; 64] };
        let e = group_streams("t", vec![a.clone(), b.clone(), g.clone()]);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].streams.len(), 3);

        // Once the system page is complete, a resource-aligned RAGE object
        // starts a new entry as before.
        let a2 = obj(0x1010, 0xBFF8, 0x30000, 0x5002_F000);
        let e = group_streams("t", vec![a2, b, g]);
        assert_eq!(e.len(), 2);
    }

    fn zlib(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(data).unwrap();
        z.finish().unwrap()
    }

    /// Incompressible bytes (xorshift).
    fn noise(n: usize, mut x: u64) -> Vec<u8> {
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect()
    }

    fn pad16(f: &mut Vec<u8>) {
        while f.len() % 16 != 0 {
            f.push(0);
        }
    }

    /// Lay out an archive body: 16 bytes of zeros, then the chunks (zlib or
    /// raw), each 16-byte aligned. Returns the file and the joined content.
    fn chunked(chunks: &[(bool, Vec<u8>)]) -> (Vec<u8>, Vec<u8>) {
        let (mut f, mut joined) = (vec![0u8; 16], Vec::new());
        for (raw, c) in chunks {
            f.extend_from_slice(&if *raw { c.clone() } else { zlib(c) });
            pad16(&mut f);
            joined.extend_from_slice(c);
        }
        f.extend_from_slice(&[0u8; 64]);
        (f, joined)
    }

    fn scan_file(name: &str, bytes: &[u8]) -> (PathBuf, Vec<Entry>) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scan-raw-chunk-test");
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        fs::write(&p, bytes).unwrap();
        let s = scan_segment(&p, 0, bytes.len() as u64, &AtomicU64::new(0)).unwrap();
        (p.clone(), group_streams("t", s))
    }

    fn text(n: usize, seed: u8) -> Vec<u8> {
        (0..n).map(|i| b"PSIN chunk text "[i % 16].wrapping_add(seed * (i % 3 == 0) as u8)).collect()
    }

    #[test]
    fn stored_raw_chunks_join_their_chunked_entry() {
        let c = CHUNK as usize;
        // zlib, zlib, raw, zlib, raw, raw, short zlib tail.
        let chunks = vec![
            (false, text(c, 1)),
            (false, text(c, 2)),
            (true, noise(c, 7)),
            (false, text(c, 3)),
            (true, noise(c, 9)),
            (true, noise(c, 11)),
            (false, text(12_345, 4)),
        ];
        let (file, joined) = chunked(&chunks);
        let (p, e) = scan_file("raw.bin", &file);
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(e[0].usize, 6 * CHUNK + 12_345);
        assert_eq!(e[0].streams.len(), 7);
        assert_eq!(e[0].streams.iter().filter(|s| is_raw(s)).count(), 3);
        assert_eq!(read_entry(&p, &e[0]).unwrap(), joined);
    }

    #[test]
    fn raw_bytes_without_a_short_zlib_end_are_not_joined() {
        let c = CHUNK as usize;
        // Two full zlib chunks, then raw bytes and no zlib chunk after them:
        // nothing proves the raw bytes belong to the entry, so it stays as
        // before (two chunks).
        let (mut file, _) = chunked(&[(false, text(c, 1)), (false, text(c, 2))]);
        file.truncate(file.len() - 64);
        file.extend_from_slice(&noise(3 * c, 5));
        let (_, e) = scan_file("noend.bin", &file);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].usize, 2 * CHUNK);
        assert!(e[0].streams.iter().all(|s| !is_raw(s)));
    }
}

#[cfg(test)]
mod upgrade_tests {
    use super::*;

    #[test]
    fn steps_reach_the_current_version() {
        // A version bump must add its step to INDEX_STEPS.
        assert_eq!(INDEX_STEPS.last().unwrap().1, crate::INDEX_VERSION);
        for w in INDEX_STEPS.windows(2) {
            assert_eq!(w[0].1, w[1].0);
        }
        assert_eq!(upgrade_needs(crate::INDEX_VERSION), Some(StepNeeds::Regroup));
        assert_eq!(upgrade_needs("7"), Some(StepNeeds::RenameTagged));
        assert_eq!(upgrade_needs("6"), Some(StepNeeds::RenameTagged));
        assert_eq!(upgrade_needs("5"), Some(StepNeeds::RenameTagged));
        assert_eq!(upgrade_needs("4"), None);
        assert_eq!(upgrade_needs("x"), None);
    }

    fn zlib(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(data).unwrap();
        z.finish().unwrap()
    }

    fn noise(n: usize, mut x: u64) -> Vec<u8> {
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect()
    }

    fn text(n: usize, seed: u8) -> Vec<u8> {
        (0..n).map(|i| b"PSIN chunk text "[i % 16].wrapping_add(seed * (i % 3 == 0) as u8)).collect()
    }

    /// A RAGE object system page of 0x200 bytes (page map at 0x100), with
    /// "<name>.#dr" at 0x40 behind a pointer when `name` is set.
    fn page(name: Option<&str>) -> Vec<u8> {
        let mut p = vec![0u8; 0x200];
        p[8..16].copy_from_slice(&0x5000_0100u64.to_le_bytes());
        if let Some(n) = name {
            p[0x18..0x20].copy_from_slice(&0x5000_0040u64.to_le_bytes());
            // a name character in front, as in the name pointer step
            p[0x3f] = b'd';
            let s = format!("{n}.#dr");
            p[0x40..0x40 + s.len()].copy_from_slice(s.as_bytes());
        }
        p
    }

    /// Put `bytes` at `at` (zero fill up to it).
    fn put(f: &mut Vec<u8>, at: usize, bytes: &[u8]) {
        assert!(f.len() <= at);
        f.resize(at, 0);
        f.extend_from_slice(bytes);
    }

    /// A named and an unnamed resource, then a chunked entry of exactly two
    /// 64 KiB chunks followed by padding (scan 2 probes there and finds
    /// nothing), then a PSO; with `raw`, also a chunked entry with a stored
    /// chunk (scan 2 joins it, scan 1 did not).
    fn archive_bytes(raw: bool) -> Vec<u8> {
        let c = CHUNK as usize;
        let mut f = Vec::new();
        put(&mut f, 0x1010, &zlib(&page(Some("propa"))));
        put(&mut f, 0x2010, &zlib(&page(None)));
        put(&mut f, 0x3010, &zlib(&text(c, 1)));
        let at = (f.len() + 15) & !15;
        put(&mut f, at, &zlib(&text(c, 2)));
        let at = ((f.len() + 4095) & !4095) + 0x10;
        put(&mut f, at, &zlib(&text(5000, 3)));
        if raw {
            let at = ((f.len() + 4095) & !4095) + 0x10;
            put(&mut f, at, &zlib(&text(c, 4)));
            let at = (f.len() + 15) & !15;
            put(&mut f, at, &noise(c, 7));
            let at = (f.len() + 15) & !15;
            put(&mut f, at, &zlib(&text(777, 5)));
        }
        f.resize(((f.len() + 4095) & !4095) + 64, 0);
        f
    }

    struct Case {
        archive: PathBuf,
        dir: PathBuf,
        size: u64,
        mtime: u64,
    }

    fn case(name: &str, raw: bool) -> Case {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/index-upgrade-test").join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("t.rpf");
        fs::write(&archive, archive_bytes(raw)).unwrap();
        let m = fs::metadata(&archive).unwrap();
        let mtime = m.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        Case { archive, dir: root.join("index"), size: m.len(), mtime }
    }

    /// The index as an older version left it: its segment (scan 1 or the
    /// current scan) and an index of version `v` grouped from it.
    fn write_old(c: &Case, v: &str, scan1: bool) -> ArchiveIndex {
        let segs = c.dir.join("t.segs");
        fs::create_dir_all(&segs).unwrap();
        let streams = scan_segment_with(&c.archive, 0, c.size, &AtomicU64::new(0), !scan1).unwrap();
        let sv = if scan1 { 1 } else { SCAN_VERSION };
        fs::write(seg_path(&segs, 0, c.size, c.mtime, sv), serde_json::to_vec(&streams).unwrap()).unwrap();
        let mut entries = group_streams("t.rpf", streams);
        for e in entries.iter_mut() {
            crate::classify::refine(e);
        }
        let ix = ArchiveIndex {
            tool_version: v.into(),
            archive: "t.rpf".into(),
            path: c.archive.display().to_string(),
            size: c.size,
            mtime: c.mtime,
            header: None,
            container: "other".into(),
            stats: IndexStats::default(),
            entries,
            names_done: false,
        };
        save_index(&c.dir, &ix).unwrap();
        ix
    }

    fn fresh(c: &Case) -> ArchiveIndex {
        let dir = c.dir.with_file_name("fresh");
        let _ = fs::remove_dir_all(&dir);
        build_index_report(&c.archive, "t.rpf", &dir, true, None).unwrap().0
    }

    fn same(a: &ArchiveIndex, b: &ArchiveIndex) {
        assert_eq!(serde_json::to_value(a).unwrap(), serde_json::to_value(b).unwrap());
    }

    #[test]
    fn scan1_segment_without_stored_chunks_is_upgraded_in_place() {
        let c = case("noraw", false);
        let s1 = scan_segment_with(&c.archive, 0, c.size, &AtomicU64::new(0), false).unwrap();
        assert!(s1.iter().any(|s| s.usize == CHUNK), "a 64 KiB chunk for the probe");
        assert!(scan1_segment_is_current(&mut File::open(&c.archive).unwrap(), &s1, c.size));
        write_old(&c, "6", true);
        let (ix, how) = build_index_report(&c.archive, "t.rpf", &c.dir, false, None).unwrap();
        assert_eq!(how.status, IndexStatus::Upgraded);
        assert_eq!(how.from_version.as_deref(), Some("6"));
        assert_eq!((how.segments_reused, how.segments_upgraded, how.segments_scanned), (0, 1, 0));
        same(&ix, &fresh(&c));
        // the upgraded segment is saved under the current scan version
        assert!(seg_path(&c.dir.join("t.segs"), 0, c.size, c.mtime, SCAN_VERSION).exists());
        // and the next run reuses the index
        let (_, how) = build_index_report(&c.archive, "t.rpf", &c.dir, false, None).unwrap();
        assert_eq!(how.status, IndexStatus::Reused);
    }

    #[test]
    fn scan1_segment_with_stored_chunks_is_scanned_again() {
        let c = case("raw", true);
        let s1 = scan_segment_with(&c.archive, 0, c.size, &AtomicU64::new(0), false).unwrap();
        assert!(!scan1_segment_is_current(&mut File::open(&c.archive).unwrap(), &s1, c.size));
        write_old(&c, "6", true);
        let (ix, how) = build_index_report(&c.archive, "t.rpf", &c.dir, false, None).unwrap();
        assert_eq!(how.status, IndexStatus::Rescanned);
        assert_eq!((how.segments_upgraded, how.segments_scanned), (0, 1));
        assert!(ix.entries.iter().any(|e| e.streams.iter().any(is_raw)));
        same(&ix, &fresh(&c));
    }

    #[test]
    fn force_rescans_and_unknown_versions_keep_no_names() {
        let c = case("force", false);
        write_old(&c, "7", false);
        let (_, how) = build_index_report(&c.archive, "t.rpf", &c.dir, true, None).unwrap();
        assert_eq!(how.status, IndexStatus::Rescanned);
        assert_eq!(how.segments_scanned, 1);
        // An index from before the first step: entries come from the
        // segments, every name is recovered again.
        let mut old = write_old(&c, "3", false);
        old.names_done = true;
        old.entries[0].name = Some("kept?".into());
        save_index(&c.dir, &old).unwrap();
        let (ix, how) = build_index_report(&c.archive, "t.rpf", &c.dir, false, None).unwrap();
        assert_eq!(how.status, IndexStatus::Upgraded);
        assert_eq!(how.names_kept, 0);
        assert!(!ix.names_done && ix.entries.iter().all(|e| e.name.is_none()));
    }

    #[test]
    fn version_7_names_from_a_tag_are_recovered_again_others_kept() {
        let c = case("names", false);
        let mut old = write_old(&c, "7", false);
        assert_eq!((old.entries[0].off, old.entries[0].kind), (0x1010, Kind::RageObj));
        assert_eq!((old.entries[1].off, old.entries[1].kind), (0x2010, Kind::RageObj));
        // version 7 took the name character in front along
        old.entries[0].name = Some("dpropa".into());
        old.entries[0].rtype = Some("dr".into());
        // a texture dictionary name stays
        old.entries[1].rtype = Some("txd".into());
        old.entries[1].name = Some("dict_b".into());
        old.names_done = true;
        save_index(&c.dir, &old).unwrap();
        let (mut ix, how) = build_index_report(&c.archive, "t.rpf", &c.dir, false, None).unwrap();
        assert_eq!(how.status, IndexStatus::Upgraded);
        assert_eq!((how.segments_reused, how.segments_scanned), (1, 0));
        assert_eq!(how.names_kept, 1);
        assert_eq!(how.names_todo, vec![0]);
        assert!(!ix.names_done);
        assert_eq!(ix.entries[1].name.as_deref(), Some("dict_b"));
        crate::export::recover_names_of(&c.archive, &mut ix, Some(&how.names_todo), crate::export::no_progress()).unwrap();
        assert_eq!(ix.entries[0].name.as_deref(), Some("propa"));
        assert_eq!(ix.entries[1].name.as_deref(), Some("dict_b"));
        assert!(ix.names_done);
    }

    #[test]
    fn regroup_keeps_names_only_of_unchanged_entries() {
        let c = case("regroup", false);
        let mut old = write_old(&c, "6", true);
        // an untagged name is kept through 6 -> 8; a changed entry's is not
        old.entries[0].rtype = Some("txd".into());
        old.entries[0].name = Some("dict_a".into());
        old.entries[1].streams[0][1] += 1;
        old.entries[1].name = Some("stale".into());
        old.names_done = true;
        save_index(&c.dir, &old).unwrap();
        let (ix, how) = build_index_report(&c.archive, "t.rpf", &c.dir, false, None).unwrap();
        assert_eq!(how.names_kept, 1);
        assert_eq!(how.names_todo, vec![1]);
        assert_eq!(ix.entries[0].name.as_deref(), Some("dict_a"));
        assert!(ix.entries[1].name.is_none());
    }
}
