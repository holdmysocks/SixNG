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
            None if pos == chunk_end => {
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
            let cp = seg_dir.join(format!("{:05}_{}_{}_s{}.json", i, size, mtime, SCAN_VERSION));
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
