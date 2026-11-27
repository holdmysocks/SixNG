//! `sixng --bench`: times indexing, search, texture decoding and model export
//! on the installed game data and prints a checksum of every result, so a
//! speed change can be shown not to change the output.
//!
//! Nothing is written to the work dir. The model export goes to a scratch
//! folder that is removed afterwards. Times depend on the machine and on the
//! file cache (run it twice and compare the second runs); checksums do not.
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use serde_json::{json, Value};
use sixng_core::classify::Kind;
use sixng_core::export::{self, ExportOptions, Filter, Workspace};
use sixng_core::game::ArchiveInfo;
use sixng_core::resource::Resource;
use sixng_core::scan::{self, ArchiveIndex, Entry, RawStream};
use sixng_core::texture;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::time::Instant;

#[derive(clap::Args, Clone)]
pub struct BenchArgs {
    /// Time indexing, search, texture decoding and model export on the
    /// installed game data, with a checksum of every result. Needs the
    /// archives indexed; writes nothing to the work dir.
    #[arg(long)]
    pub bench: bool,
    /// Bench: archive whose scan is timed (default: the largest RPF9 archive).
    #[arg(long, requires = "bench", value_name = "ARCHIVE")]
    pub bench_index: Option<String>,
    /// Bench: megabytes of that archive to scan.
    #[arg(long, requires = "bench", default_value_t = 1024, value_name = "MB")]
    pub bench_mb: u64,
    /// Bench: where in that archive the scan starts, in megabytes.
    #[arg(long, requires = "bench", default_value_t = 0, value_name = "MB")]
    pub bench_at_mb: u64,
    /// Bench: texture dictionaries to decode.
    #[arg(long, requires = "bench", default_value_t = 192, value_name = "N")]
    pub bench_count: usize,
    /// Bench: models to export.
    #[arg(long, requires = "bench", default_value_t = 8, value_name = "N")]
    pub bench_models: usize,
    /// Bench: scratch folder for the model export (default: the system temp folder).
    #[arg(long, requires = "bench", value_name = "DIR")]
    pub bench_out: Option<PathBuf>,
    /// Bench: parts to run, from index, search, textures, models (default: all).
    #[arg(long, requires = "bench", value_delimiter = ',', value_name = "PARTS")]
    pub bench_only: Vec<String>,
}

/// FNV-1a over 64-bit little-endian words (the tail byte by byte). Only for
/// comparing two runs; not a cryptographic hash.
#[derive(Clone, Copy)]
struct Sum(u64);

impl Sum {
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    fn new() -> Self {
        Sum(0xCBF2_9CE4_8422_2325)
    }
    fn word(&mut self, w: u64) {
        self.0 = (self.0 ^ w).wrapping_mul(Self::PRIME);
    }
    fn bytes(&mut self, b: &[u8]) {
        self.word(b.len() as u64);
        let mut c = b.chunks_exact(8);
        for w in &mut c {
            self.word(u64::from_le_bytes(w.try_into().unwrap()));
        }
        for &x in c.remainder() {
            self.word(x as u64);
        }
    }
    fn of(b: &[u8]) -> u64 {
        let mut s = Sum::new();
        s.bytes(b);
        s.0
    }
    fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

fn secs(t: Instant) -> f64 {
    (t.elapsed().as_secs_f64() * 1000.0).round() / 1000.0
}

/// `n` items of `v`, evenly spaced, in order.
fn spread<T: Copy>(v: &[T], n: usize) -> Vec<T> {
    if v.len() <= n {
        return v.to_vec();
    }
    (0..n).map(|i| v[i * v.len() / n]).collect()
}

pub fn run(ws: &Workspace, a: &BenchArgs) -> Result<Value> {
    let on = |p: &str| a.bench_only.is_empty() || a.bench_only.iter().any(|x| x == p);
    let archives = ws.archives();
    let mut res = json!({"ok": true, "threads": rayon::current_num_threads()});
    if on("index") {
        res["index"] = index(&archives, a)?;
    }
    if !(on("search") || on("textures") || on("models")) {
        return Ok(res);
    }
    let t = Instant::now();
    let indexes: Vec<(ArchiveInfo, ArchiveIndex)> = archives.iter().filter_map(|ar| ws.load_index(ar).ok().map(|ix| (ar.clone(), ix))).collect();
    let load_seconds = secs(t);
    if indexes.is_empty() {
        return Err(anyhow!("no archive is indexed yet: run `sixng index` first"));
    }
    if on("search") {
        res["search"] = search(&indexes, load_seconds)?;
    }
    if on("textures") {
        res["textures"] = textures(&indexes, a.bench_count)?;
    }
    if on("models") {
        res["models"] = models(&indexes, a)?;
    }
    Ok(res)
}

/// The carving scan and the name pass over one part of one archive.
fn index(archives: &[ArchiveInfo], a: &BenchArgs) -> Result<Value> {
    let ar = match &a.bench_index {
        Some(q) => sixng_core::game::resolve(archives, q).ok_or_else(|| anyhow!("archive not found or ambiguous: {q}"))?,
        None => archives.iter().filter(|x| x.container == "rpf9").max_by_key(|x| x.size).ok_or_else(|| anyhow!("no RPF9 archive in the game folder"))?,
    };
    let header = sixng_core::rpf9::read_header(&ar.path).ok();
    let data_start = header.as_ref().map(|h| ((h.toc_end as u64) + 15) & !15).unwrap_or(0);
    let first = (a.bench_at_mb << 20) / scan::SEGMENT;
    let last = (((a.bench_at_mb + a.bench_mb) << 20).min(ar.size)).div_ceil(scan::SEGMENT);
    let segs: Vec<u64> = (first..last).collect();
    let done = AtomicU64::new(0);
    let t = Instant::now();
    let parts: Vec<Result<Vec<RawStream>>> = segs
        .par_iter()
        .map(|&i| {
            let s = (i * scan::SEGMENT).max(data_start);
            let e = ((i + 1) * scan::SEGMENT).min(ar.size);
            if s >= e { Ok(Vec::new()) } else { scan::scan_segment(&ar.path, s, e, &done) }
        })
        .collect();
    let scan_seconds = secs(t);
    let mut streams = Vec::new();
    for p in parts {
        streams.extend(p?);
    }
    streams.sort_by_key(|s| s.off);
    let mut sum = Sum::new();
    for s in &streams {
        sum.word(s.off);
        sum.word(s.csize);
        sum.word(s.usize);
        sum.bytes(&s.head);
    }
    let nstreams = streams.len();
    let bytes = segs.len() as u64 * scan::SEGMENT;
    let t = Instant::now();
    let mut entries = scan::group_streams(&ar.id, streams);
    for e in entries.iter_mut() {
        sixng_core::classify::refine(e);
    }
    let mut ix = ArchiveIndex {
        tool_version: sixng_core::INDEX_VERSION.into(),
        archive: ar.id.clone(),
        path: String::new(),
        size: ar.size,
        mtime: 0,
        header,
        container: ar.container.clone(),
        entries,
        stats: Default::default(),
        names_done: false,
    };
    export::recover_names(&ar.path, &mut ix, export::no_progress())?;
    let names_seconds = secs(t);
    let named = ix.entries.iter().filter(|e| e.name.is_some()).count();
    Ok(json!({
        "archive": ar.id, "at_mb": first * (scan::SEGMENT >> 20), "mb": bytes >> 20,
        "scan_seconds": scan_seconds, "scan_mb_per_second": ((bytes >> 20) as f64 / scan_seconds.max(1e-3)).round(),
        "streams": nstreams, "streams_checksum": sum.hex(),
        "names_seconds": names_seconds, "entries": ix.entries.len(), "named_entries": named,
        "entries_checksum": format!("{:016x}", Sum::of(&serde_json::to_vec(&ix.entries)?)),
    }))
}

/// Regex searches over every loaded index, as `sixng search` runs them.
fn search(indexes: &[(ArchiveInfo, ArchiveIndex)], load_seconds: f64) -> Result<Value> {
    const QUERIES: [&str; 6] = ["door", "^prop_.*light", "_lod$", "(?i)tree|bush|palm", "wheel.*_(l|r)$", "@[0-9a-f]*00$"];
    let t = Instant::now();
    let mut sum = Sum::new();
    let mut hits = 0usize;
    for q in QUERIES {
        let f = Filter { name: Some(q.to_string()), ..Default::default() };
        for (_, ix) in indexes {
            for e in f.select(ix)? {
                sum.bytes(e.id.as_bytes());
                hits += 1;
            }
        }
    }
    let entries: usize = indexes.iter().map(|(_, ix)| ix.entries.len()).sum();
    Ok(json!({
        "indexes": indexes.len(), "entries": entries, "load_seconds": load_seconds,
        "queries": QUERIES.len(), "search_seconds": secs(t), "hits": hits, "checksum": sum.hex(),
    }))
}

/// The archive with the most data of one resource type, and those entries.
fn pick<'a>(indexes: &'a [(ArchiveInfo, ArchiveIndex)], rtype: &str) -> Option<(&'a ArchiveInfo, Vec<&'a Entry>)> {
    indexes
        .iter()
        .map(|(ar, ix)| (ar, ix.entries.iter().filter(|e| matches!(e.kind, Kind::RageObj | Kind::Resource) && e.rtype.as_deref() == Some(rtype)).collect::<Vec<_>>()))
        .max_by_key(|(_, v)| v.iter().map(|e| e.usize).sum::<u64>())
        .filter(|(_, v)| !v.is_empty())
}

#[derive(Default)]
struct FmtStat {
    textures: usize,
    pixels: u64,
    seconds: f64,
}

/// Texture dictionaries: read, untile and decode mip 0 of every texture.
/// One texture at a time first (what a viewer does), then the same set with
/// one texture per core (what an export does).
fn textures(indexes: &[(ArchiveInfo, ArchiveIndex)], count: usize) -> Result<Value> {
    let (ar, all) = pick(indexes, "txd").ok_or_else(|| anyhow!("no indexed archive has texture dictionaries"))?;
    let sel = spread(&all, count);
    let (mut read, mut untile, mut decode) = (0f64, 0f64, 0f64);
    let mut by_fmt: BTreeMap<String, FmtStat> = BTreeMap::new();
    let mut sum = Sum::new();
    let mut failed = 0usize;
    for e in &sel {
        let t = Instant::now();
        let Ok(r) = Resource::load(&ar.path, e) else {
            failed += 1;
            continue;
        };
        read += t.elapsed().as_secs_f64();
        for tx in texture::find_textures(&r) {
            let t = Instant::now();
            let Ok((fmt, lin)) = texture::mip0_linear(&r, &tx, tx.height == 1) else {
                failed += 1;
                continue;
            };
            untile += t.elapsed().as_secs_f64();
            let t = Instant::now();
            let rgba = texture::to_rgba(fmt, &lin, tx.width, tx.height);
            let dt = t.elapsed().as_secs_f64();
            let Ok(rgba) = rgba else {
                failed += 1;
                continue;
            };
            decode += dt;
            let s = by_fmt.entry(tx.format.clone()).or_default();
            s.textures += 1;
            s.pixels += tx.width as u64 * tx.height as u64;
            s.seconds += dt;
            sum.bytes(&rgba);
        }
    }
    let t = Instant::now();
    let batch: Vec<u64> = sel
        .par_iter()
        .map(|e| {
            let Ok(r) = Resource::load(&ar.path, e) else { return Vec::new() };
            let txs = texture::find_textures(&r);
            txs.par_iter()
                .filter_map(|tx| {
                    let (fmt, lin) = texture::mip0_linear(&r, tx, tx.height == 1).ok()?;
                    texture::to_rgba(fmt, &lin, tx.width, tx.height).ok().map(|p| Sum::of(&p))
                })
                .collect::<Vec<u64>>()
        })
        .flatten()
        .collect();
    let batch_seconds = secs(t);
    let mut bsum = Sum::new();
    batch.iter().for_each(|&h| bsum.word(h));
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    let formats: BTreeMap<&String, Value> = by_fmt
        .iter()
        .map(|(k, s)| (k, json!({"textures": s.textures, "megapixels": r3(s.pixels as f64 / 1e6), "decode_seconds": r3(s.seconds), "megapixels_per_second": (s.pixels as f64 / 1e6 / s.seconds.max(1e-6)).round()})))
        .collect();
    Ok(json!({
        "archive": ar.id, "dictionaries": sel.len(), "textures": by_fmt.values().map(|s| s.textures).sum::<usize>(), "failed": failed,
        "read_seconds": r3(read), "untile_seconds": r3(untile), "decode_seconds": r3(decode), "formats": formats,
        "checksum": sum.hex(), "batch_seconds": batch_seconds, "batch_checksum": bsum.hex(),
    }))
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            files_under(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Model export (drawables, with their textures as PNG) into a scratch
/// folder; every file written goes into the checksum.
fn models(indexes: &[(ArchiveInfo, ArchiveIndex)], a: &BenchArgs) -> Result<Value> {
    let (ar, all) = pick(indexes, "dr").ok_or_else(|| anyhow!("no indexed archive has drawables"))?;
    let sel = spread(&all, a.bench_models);
    let out = a.bench_out.clone().unwrap_or_else(std::env::temp_dir).join("sixng-bench");
    let _ = std::fs::remove_dir_all(&out);
    let o = ExportOptions {
        out: out.clone(),
        png: true,
        dds: false,
        raw: false,
        force: true,
        skip_embedded_textures: false,
        body_shape: Vec::new(),
        head_skin: 0,
        paint: None,
        tex_remap: Default::default(),
        ped_tints: Default::default(),
        part_skins: Default::default(), part_decorations: Default::default(),
        part_colours: Default::default(),
        part_hidden: Default::default(),
    };
    let t = Instant::now();
    let items = export::export_many(ar, &sel, &o, export::no_progress())?;
    let seconds = secs(t);
    let mut files = Vec::new();
    files_under(&out, &mut files);
    // The manifest is appended to from several threads: its line order varies.
    files.retain(|p| p.file_name().is_some_and(|n| n != "manifest.jsonl"));
    let mut named: Vec<(String, PathBuf)> = files.into_iter().map(|p| (p.strip_prefix(&out).unwrap_or(&p).to_string_lossy().replace('\\', "/"), p)).collect();
    named.sort();
    let mut sum = Sum::new();
    let mut bytes = 0u64;
    for (name, p) in &named {
        let b = std::fs::read(p)?;
        bytes += b.len() as u64;
        sum.bytes(name.as_bytes());
        sum.bytes(&b);
    }
    let _ = std::fs::remove_dir_all(&out);
    Ok(json!({
        "archive": ar.id, "models": sel.len(), "ok": items.iter().filter(|i| i.status == "ok").count(),
        "errors": items.iter().filter(|i| i.status == "error").count(), "warnings": items.iter().map(|i| i.warnings.len()).sum::<usize>(),
        "export_seconds": seconds, "files": named.len(), "megabytes": bytes >> 20, "checksum": sum.hex(),
    }))
}
