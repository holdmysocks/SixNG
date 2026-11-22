//! sixng: command-line front end (machine-readable JSON on stdout,
//! NDJSON progress events on stderr).
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use sixng_core::classify::Kind;
use sixng_core::export::{self, ExportOptions, Filter, Progress, Workspace};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "sixng", version, about = "GTA VI (RPF9 / Xbox Series) asset extractor")]
struct Cli {
    /// Game install root (required: this flag, SIXNG_GAME, or the folder saved by sixng-gui).
    #[arg(long, global = true, env = "SIXNG_GAME")]
    game: Option<PathBuf>,
    /// Work dir (indexes live in <work>/index).
    #[arg(long, global = true, env = "SIXNG_WORK", default_value = "work")]
    work: PathBuf,
    /// Suppress progress events on stderr.
    #[arg(long, global = true)]
    quiet: bool,
    /// Pretty-print JSON output.
    #[arg(long, global = true)]
    pretty: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Args, Clone, Default)]
struct FilterArgs {
    /// Entry kind(s): resource, rageobj, xml, rbf, pso, gfx, dds, png, text, bin
    #[arg(long, value_delimiter = ',')]
    kind: Vec<String>,
    /// Resource type(s) from names: dr (drawable), ft (fragment), dd (drawable dict), txd, ...
    #[arg(long, value_delimiter = ',')]
    rtype: Vec<String>,
    /// Regex on recovered name (or id).
    #[arg(long)]
    name: Option<String>,
    /// Regex on name to exclude.
    #[arg(long)]
    exclude_name: Option<String>,
    /// Exact entry id(s) ("levels.rpf@1a2b3c") or hex offsets.
    #[arg(long, value_delimiter = ',')]
    id: Vec<String>,
    /// File with entry ids, one per line (blank lines and # comments ignored); combined with --id.
    #[arg(long)]
    id_file: Option<std::path::PathBuf>,
    /// Minimum decompressed size in bytes.
    #[arg(long, default_value_t = 0)]
    min_size: u64,
    #[arg(long)]
    limit: Option<usize>,
    /// World region "minx,miny,maxx,maxy" (RAGE metres) for collision, navmesh, placements and lights.
    #[arg(long, value_delimiter = ',', allow_hyphen_values = true)]
    bbox: Vec<f32>,
}

impl FilterArgs {
    fn to_filter(&self) -> Result<Filter> {
        if self.bbox.len() == 4 {
            export::set_region([self.bbox[0], self.bbox[1], self.bbox[2], self.bbox[3]]);
        } else if !self.bbox.is_empty() {
            return Err(anyhow!("--bbox needs minx,miny,maxx,maxy"));
        }
        let mut kinds = Vec::new();
        for k in &self.kind {
            let k = Kind::all().iter().find(|x| x.name() == k.to_lowercase()).ok_or_else(|| anyhow!("unknown kind {k}"))?;
            kinds.push(*k);
        }
        Ok(Filter {
            kinds,
            rtypes: self.rtype.clone(),
            name: self.name.clone(),
            exclude_name: self.exclude_name.clone(),
            ids: {
                let mut ids = self.id.clone();
                if let Some(f) = &self.id_file {
                    let t = std::fs::read_to_string(f).map_err(|e| anyhow!("--id-file {}: {e}", f.display()))?;
                    ids.extend(t.lines().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from));
                }
                ids
            },
            min_size: self.min_size,
            limit: self.limit,
        })
    }
}

#[derive(clap::Args, Clone)]
struct OutArgs {
    /// Output directory.
    #[arg(long, short)]
    out: PathBuf,
    /// Texture output: png, dds, or both.
    #[arg(long, default_value = "png")]
    tex: String,
    /// Also dump decompressed resource bytes.
    #[arg(long)]
    raw: bool,
    /// Re-export items whose outputs already exist.
    #[arg(long)]
    force: bool,
    /// Don't export textures embedded in models.
    #[arg(long)]
    no_embedded_textures: bool,
    /// Only print the selection and size estimate.
    #[arg(long)]
    dry_run: bool,
    /// Body shape for ped clothing, e.g. "gcweightbelly=0.5,gcmusclechest=1"
    /// (gcweight* -1..1, gcmuscle* 0..1, gcbutt, gcbreasts; allweight=v and
    /// allmuscle=v set a whole group). Poses the shared ped skeleton with the
    /// game's body-shape rig; needs packs.rpf and anim.rpf indexed.
    #[arg(long)]
    body_shape: Option<String>,
    /// Face variant for ped heads (0 = first). Heads get the face the game
    /// links to them, or one of the faces of the same sex, body type and
    /// ethnicity; model.json `head_skin.count` says how many there are.
    #[arg(long, default_value_t = 0)]
    head_skin: usize,
}

impl OutArgs {
    fn opts(&self) -> ExportOptions {
        ExportOptions {
            out: self.out.clone(),
            png: self.tex == "png" || self.tex == "both",
            dds: self.tex == "dds" || self.tex == "both",
            raw: self.raw,
            force: self.force,
            skip_embedded_textures: self.no_embedded_textures,
            body_shape: match self.body_shape.as_deref().map(sixng_core::bodyshape::parse_spec) {
                None => Vec::new(),
                Some(Ok(v)) => v,
                Some(Err(e)) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            },
            head_skin: self.head_skin,
        }
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Show archives in the install, their headers and index status.
    Info,
    /// Build (resumable) carving indexes + name recovery.
    Index {
        /// Archive ids (e.g. levels, data/ttdb/vehicles/ttdb_vehicles.ttpf). Empty = all RPF9 archives.
        archives: Vec<String>,
        #[arg(long)]
        force: bool,
        /// Skip the name-recovery pass.
        #[arg(long)]
        no_names: bool,
    },
    /// List entries of an indexed archive.
    List {
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        /// Output NDJSON (one entry per line) instead of a JSON array.
        #[arg(long)]
        ndjson: bool,
        /// Include stream lists.
        #[arg(long)]
        streams: bool,
    },
    /// Search names/ids across all indexed archives (regex).
    Search {
        query: String,
        #[arg(long)]
        archive: Option<String>,
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Extract entries matching a filter from one archive.
    Extract {
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        #[command(flatten)]
        out: OutArgs,
    },
    /// Extract a built-in category: textures, map, vehicles, meta.
    ExtractCategory {
        category: String,
        /// Extra regex on names (e.g. '^phoenix' for one vehicle).
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[command(flatten)]
        out: OutArgs,
    },
    /// Describe one entry (header, recovered metadata, textures) without writing files.
    Inspect { id: String },
    /// Decode a linear BC/RGBA dump to PNG (debugging aid).
    #[command(hide = true)]
    RawDecode { input: PathBuf, dxgi: u32, width: u32, height: u32, output: PathBuf },
    /// Decode map placements (entity lists) to JSON: <out>/placements/<archive>/.
    Placements {
        /// Archive (default levels).
        #[arg(default_value = "levels")]
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
        /// Also write instanced batches (props, grass) to batches.jsonl (large: combine with --bbox for a region).
        #[arg(long)]
        batches: bool,
        /// Write only batches.jsonl (skip the per-map-file entity JSON).
        #[arg(long)]
        batches_only: bool,
    },
    /// Decode physics bounds (collision meshes with surface ids) to GLB: <out>/collision/<archive>/.
    Collision {
        /// Archive (default levels).
        #[arg(default_value = "levels")]
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Decode navmesh tiles (walkable surface, road polygons flagged) to GLB: <out>/navmesh/<archive>/.
    Navmesh {
        /// Archive (default navmeshes).
        #[arg(default_value = "navmeshes")]
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        /// Only write road polygons (flag 0x2000) to the GLBs (much smaller).
        #[arg(long)]
        roads_only: bool,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Road/path-node graph (nodes with speed limits, links with lane counts): <out>/paths/<archive>/graph.json.
    Paths {
        /// Archive (default levels).
        #[arg(default_value = "levels")]
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Interior (MLO) instances with world transforms and their entity lists: <out>/placements/<archive>/interiors.jsonl.
    Interiors {
        /// Archive (default levels).
        #[arg(default_value = "levels")]
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Run world-data steps with default selections (same as the GUI buttons): placements, batches, collision, navmesh, paths, lights, interiors, surfaces, or "all" in that order.
    World {
        /// Step id or "all".
        step: String,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Inferred collision surface categories (needs `collision`; uses `paths`, `placements --batches` and the navmeshes archive when present): <out>/collision/<archive>/surface_categories.json.
    Surfaces {
        /// Archive (default levels).
        #[arg(default_value = "levels")]
        archive: String,
        /// Navmesh archive for the road-overlap cue ("none" to skip).
        #[arg(long, default_value = "navmeshes")]
        navmeshes: String,
        /// Output directory (the same one the other exports wrote to).
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Research: find u32 values (hex, one per line in a file) inside meta (0x7a) struct instances; reports struct/offset hits.
    FindU32 {
        /// Archive (default levels).
        #[arg(default_value = "levels")]
        archive: String,
        /// File with one hex u32 per line.
        #[arg(long)]
        values: PathBuf,
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Light emitters from archetype/entity light extensions, placed in world space: <out>/lights/<archive>/.
    Lights {
        /// Archive (default levels).
        #[arg(default_value = "levels")]
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Scan self-describing meta resources (0x7a): schema index, optional full JSON dumps.
    Meta {
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        /// Also write each resource decoded to <out>/meta/<archive>/<off>.json.
        #[arg(long)]
        dump: bool,
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Shader census: per shader hash, usage, params, bound texture formats, flags (JSON).
    Shaders {
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        #[arg(long, short)]
        out: PathBuf,
    },
    /// BCPack texture gaps: every material whose textures are flat (no clean mip) or low-mip, with slot roles, class, terrain-layer surface guesses and usage weight. Writes <out>/texture_gaps.json.
    TextureGaps {
        /// Archives with drawables (default levels).
        #[arg(default_values_t = vec!["levels".to_string()])]
        archives: Vec<String>,
        /// Placement dirs (`*.json` from `placements`, `batches.jsonl`) or batches.jsonl files. Default: <out>/placements/<archive>.
        #[arg(long)]
        placements: Vec<PathBuf>,
        /// Output dir: also where `collision`/`surfaces` wrote (terrain-layer cue) and cached texture stats live.
        #[arg(long, short)]
        out: PathBuf,
        /// Output file (default <out>/texture_gaps.json).
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Link archetypes whose drawable has no recovered name to unnamed drawables (bbox match) and guess their names: <work>/index/archetype_links.json, used by placements/lights from then on.
    LinkArchetypes {
        /// Archive (default levels).
        #[arg(default_value = "levels")]
        archive: String,
    },
    /// Archetype LOD distances and resolved LOD parents of every placement (batch instance or LOD entity): <work>/index/archetype_lod.jsonl and lod_parents.jsonl, merged into placements from then on.
    LodLinks {
        /// Archive (default levels).
        #[arg(default_value = "levels")]
        archive: String,
    },
    /// Decode PSO files (self-describing meta in plain files) to JSON: <out>/pso/<archive>/<offset>.json (root and objects by struct; field names resolve where known).
    Pso {
        /// Archive (default data).
        #[arg(default_value = "data")]
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Vehicle handling records from the PSO files (mass, drag, transmission, engine, suspension, steering, damage …), named by model where the name is known: <out>/pso/handling.json.
    Handling {
        /// Archive (default data).
        #[arg(default_value = "data")]
        archive: String,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Vehicle data from the game-data meta resource: model info (wheel scale, wheel type, handling id, …), configurations (body colour ARGB, rim, tyre, parts), paint palettes and weighted part groups: <out>/vehicles/vehicles.json.
    Vehicles {
        /// Archive (default packs).
        #[arg(default_value = "packs")]
        archive: String,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Bink videos in movies.rpf (stored uncompressed, so the stream index has none): lists every video with size, resolution, frame rate, duration and audio tracks in <out>/movies/movies.json. With --extract, writes the .bk2 files as they are (playable in any Bink 2 player); all of them take about 8.8 GB, so --only picks some.
    Movies {
        /// Archive (default movies).
        #[arg(default_value = "movies")]
        archive: String,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
        /// Write the .bk2 files (all, or those given with --only).
        #[arg(long)]
        extract: bool,
        /// Comma-separated hex offsets (from movies.json) to extract.
        #[arg(long)]
        only: Option<String>,
    },
    /// Interior membership of every placement: per map file its space (world, interior-local, group-local) and per entity its interior, room and entity set: <work>/index/interior_entities.jsonl. Placements carry the same fields.
    InteriorEntities {
        /// Archive (default levels).
        #[arg(default_value = "levels")]
        archive: String,
    },
    /// Audio banks (ADAT) in an archive: scan once (index in <work>/index/audio_<archive>.json), then list banks and streams. Stream ids are hashes; names are not recovered.
    Audio {
        /// Archive (default audio).
        #[arg(default_value = "audio")]
        archive: String,
        /// Scan again even if an index exists.
        #[arg(long)]
        rescan: bool,
        /// Only list streams at least this long (seconds).
        #[arg(long, default_value_t = 0.0)]
        min_seconds: f64,
        /// How many banks to list (0 = all).
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Export audio: Ogg Opus (lossless remux), WAV (decoded) or the raw bank (.awc). Needs `sixng audio` first.
    AudioExport {
        /// Archive (default audio).
        #[arg(long, default_value = "audio")]
        archive: String,
        /// Bank offset in hex (from `sixng audio`).
        #[arg(long)]
        bank: Option<String>,
        /// Stream id in hex (default: every stream of the bank).
        #[arg(long)]
        stream: Option<String>,
        /// opus | wav | both | awc (raw bank).
        #[arg(long, default_value = "opus")]
        format: String,
        /// Output folder.
        #[arg(long, short)]
        out: PathBuf,
        /// Skip streams shorter than this (seconds).
        #[arg(long, default_value_t = 0.0)]
        min_seconds: f64,
        /// Export every bank (with --bank omitted).
        #[arg(long)]
        all: bool,
    },
    /// Name dictionary for shader, parameter and meta field hashes (joaat of
    /// the lower-case name). Without --lookup: build `<work>/names/names.json`
    /// from the install (common.rpf text/XML and shaders.rpf strings).
    Names {
        /// Hashes (hex) to look up instead of building.
        #[arg(long, value_delimiter = ',')]
        lookup: Vec<String>,
    },
    /// Inspect a ttpf texture store: list its textures, or decode some to PNG.
    Ttpf {
        /// Store id or substring (e.g. vehicles/ttdb_vehicles.ttpf).
        store: String,
        /// Texture hashes (hex) to decode; empty = list.
        #[arg(long)]
        hash: Vec<String>,
        /// Decode the first N textures.
        #[arg(long)]
        first: Option<usize>,
        /// Mip level to decode.
        #[arg(long, default_value_t = 0)]
        mip: u32,
        /// Decode every slice of array textures and cube maps (<hash>_m<mip>_s<slice>.png); default is slice 0 only.
        #[arg(long)]
        all_slices: bool,
        /// Decode every texture in the store (in parallel) and write <out>/index.json; prints a summary instead of every result.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

struct Out {
    pretty: bool,
}
impl Out {
    fn print(&self, v: &serde_json::Value) {
        let s = if self.pretty { serde_json::to_string_pretty(v) } else { serde_json::to_string(v) }.unwrap();
        println!("{s}");
    }
}

fn progress(quiet: bool) -> Progress {
    if quiet {
        return export::no_progress();
    }
    let last = Arc::new(std::sync::Mutex::new(Instant::now()));
    Arc::new(move |stage: &str, done: u64, total: u64| {
        let mut l = last.lock().unwrap();
        if done == total || l.elapsed().as_millis() >= 1000 {
            *l = Instant::now();
            let e = json!({"event":"progress","stage":stage,"done":done,"total":total,
                "pct": if total>0 {(done as f64*1000.0/total as f64).round()/10.0} else {100.0}});
            let mut err = std::io::stderr().lock();
            let _ = writeln!(err, "{e}");
        }
    })
}

fn entry_json(e: &sixng_core::scan::Entry, streams: bool) -> serde_json::Value {
    let mut v = json!({
        "id": e.id, "offset": e.off, "csize": e.csize, "usize": e.usize,
        "kind": e.kind.name(), "sub": e.sub, "ext": e.ext, "name": e.name, "rtype": e.rtype,
        "nstreams": e.streams.len(),
    });
    if streams {
        v["streams"] = json!(e.streams);
    }
    v
}

fn main() {
    let cli = Cli::parse();
    let out = Out { pretty: cli.pretty };
    if let Err(e) = run(&cli, &out) {
        out.print(&json!({"ok": false, "error": format!("{e:#}")}));
        std::process::exit(1);
    }
}

/// The game folder: --game / SIXNG_GAME, else the one saved by sixng-gui
/// (`sixng-settings.json` next to the exe, else the per-user settings folder).
/// There is no built-in default.
fn game_root(cli: &Cli) -> Result<PathBuf> {
    if let Some(g) = &cli.game {
        return Ok(g.clone());
    }
    let local = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("sixng-settings.json")));
    let roaming = sixng_core::user_settings_dir().map(|d| d.join("settings.json"));
    let saved = [local, roaming]
        .into_iter()
        .flatten()
        .filter_map(|p| std::fs::read(p).ok())
        .filter_map(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .find_map(|v| v["game"].as_str().filter(|s| !s.is_empty()).map(PathBuf::from));
    saved.ok_or_else(|| anyhow!(r"no game folder set: pass --game <folder with the GTA VI install>, set the SIXNG_GAME environment variable, or pick the folder once in sixng-gui (it is saved to sixng-settings.json next to the exe, or settings.json in %APPDATA%\SixNG, ~/Library/Application Support/SixNG or ~/.config/SixNG)"))
}

fn run(cli: &Cli, out: &Out) -> Result<()> {
    let game = game_root(cli)?;
    let ws = Workspace::new(&game, &cli.work);
    let pr = progress(cli.quiet);
    match &cli.cmd {
        Cmd::Info => {
            let mut v = Vec::new();
            for a in ws.archives() {
                let header = sixng_core::rpf9::read_header(&a.path).ok();
                let ix = ws.load_index(&a).ok();
                v.push(json!({
                    "id": a.id, "path": a.path, "size": a.size, "container": a.container,
                    "header": header, "indexed": ix.is_some(),
                    "names": ix.as_ref().map(|i| i.names_done),
                    "stats": ix.as_ref().map(|i| &i.stats),
                }));
            }
            out.print(&json!({"ok": true, "game": ws.game, "work": ws.work, "archives": v}));
        }
        Cmd::Index { archives, force, no_names } => {
            let list = if archives.is_empty() {
                ws.archives().into_iter().filter(|a| a.container == "rpf9").collect::<Vec<_>>()
            } else {
                archives.iter().map(|q| ws.resolve(q)).collect::<Result<Vec<_>>>()?
            };
            let mut res = Vec::new();
            for a in list {
                let t = Instant::now();
                let p2 = pr.clone();
                let id = a.id.clone();
                let pa: Progress = Arc::new(move |s, d, tot| p2(&format!("{id}:{s}"), d, tot));
                let ix = ws.ensure_index(&a, *force, !*no_names, pa)?;
                let named = ix.entries.iter().filter(|e| e.name.is_some()).count();
                res.push(json!({"archive": a.id, "seconds": t.elapsed().as_secs_f64(), "stats": ix.stats, "named_entries": named}));
            }
            out.print(&json!({"ok": true, "indexed": res}));
        }
        Cmd::List { archive, filter, ndjson, streams } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let sel = filter.to_filter()?.select(&ix)?;
            if *ndjson {
                let mut so = std::io::stdout().lock();
                for e in sel {
                    writeln!(so, "{}", entry_json(e, *streams))?;
                }
            } else {
                let v: Vec<_> = sel.iter().map(|e| entry_json(e, *streams)).collect();
                out.print(&json!({"ok": true, "archive": a.id, "count": v.len(), "entries": v}));
            }
        }
        Cmd::Search { query, archive, filter } => {
            let mut f = filter.to_filter()?;
            f.name = Some(query.clone());
            let lim = f.limit.unwrap_or(200);
            f.limit = None;
            let mut hits = Vec::new();
            for a in ws.archives() {
                if let Some(q) = archive {
                    if !a.id.contains(q.as_str()) {
                        continue;
                    }
                }
                let Ok(ix) = ws.load_index(&a) else { continue };
                for e in f.select(&ix)? {
                    if hits.len() >= lim {
                        break;
                    }
                    hits.push(entry_json(e, false));
                }
            }
            out.print(&json!({"ok": true, "query": query, "count": hits.len(), "hits": hits}));
        }
        Cmd::Extract { archive, filter, out: oa } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let selected = filter.to_filter()?.select_exports(&ix)?;
            let sel: Vec<_> = selected.iter().collect();
            do_export(&a, &sel, oa, pr, out)?;
        }
        Cmd::ExtractCategory { category, name, limit, out: oa } => {
            let parts = export::category(category)?;
            let mut all = Vec::new();
            for (aq, mut f) in parts {
                let Ok(a) = ws.resolve(&aq) else { continue };
                let Ok(ix) = ws.load_index(&a) else {
                    out.print(&json!({"ok": false, "error": format!("{} not indexed; run `sixng index {}`", a.id, a.id)}));
                    continue;
                };
                if name.is_some() {
                    f.name = name.clone();
                }
                f.limit = *limit;
                let sel = f.select(&ix)?;
                all.push(do_export_collect(&a, &sel, oa, pr.clone())?);
            }
            out.print(&json!({"ok": true, "category": category, "results": all}));
        }
        Cmd::Names { lookup } => {
            if lookup.is_empty() {
                let t = Instant::now();
                let mut r = sixng_core::names::build(&ws.game, &ws.index_dir(), &ws.work)?;
                r["seconds"] = json!(t.elapsed().as_secs_f64());
                r["ok"] = json!(true);
                out.print(&r);
            } else {
                let v: Vec<_> = lookup
                    .iter()
                    .filter_map(|h| u32::from_str_radix(h.trim().trim_start_matches("0x"), 16).ok())
                    .map(|h| json!({"hash": format!("{h:08x}"), "names": sixng_core::names::all(h).into_iter().map(|(n, s)| json!({"name": n, "source": s})).collect::<Vec<_>>()}))
                    .collect();
                out.print(&json!({"ok": true, "results": v}));
            }
        }
        Cmd::Ttpf { store, hash, first, mip, all_slices, all, out: tout } => {
            let a = ws.resolve(store)?;
            let st = sixng_core::ttpf::Ttpf::open(&a.path)?;
            let mut sel: Vec<&sixng_core::ttpf::TtpfTex> = Vec::new();
            for h in hash {
                let x = u64::from_str_radix(h.trim_start_matches("0x"), 16)?;
                sel.push(st.get(x).ok_or_else(|| anyhow!("hash {h} not in store"))?);
            }
            if let Some(n) = first {
                sel.extend(st.textures.iter().take(*n));
            }
            if *all {
                sel = st.textures.iter().collect();
            }
            if sel.is_empty() {
                out.print(&json!({"store": a.id, "virt_size": st.virt_size, "textures": st.textures}));
            } else {
                let dir = tout.clone().unwrap_or_else(|| PathBuf::from("work/ttpf"));
                std::fs::create_dir_all(&dir)?;
                use rayon::prelude::*;
                let one = |t: &sixng_core::ttpf::TtpfTex| -> Vec<serde_json::Value> {
                    let mut res = Vec::new();
                    let (data, skipped) = match st.data_ex(t) {
                        Ok(x) => x,
                        Err(e) => return vec![json!({"hash": format!("{:016x}", t.hash), "slices": t.slices(), "error": e.to_string()})],
                    };
                    // Data entirely in BCPack chunks other than BC1, BC3 and BC7 (no decoder yet): the PNG
                    // would be blank, so --all writes nothing for it.
                    if *all && skipped >= t.size as u64 {
                        return vec![json!({"hash": format!("{:016x}", t.hash), "w": t.width, "h": t.height, "format": sixng_core::texture::dxgi_name(t.dxgi_format), "error": "all data in BCPack chunks (not decodable yet)", "bcpack_only": true})];
                    }
                    let n = if *all_slices { t.slices() } else { 1 };
                    for sl in 0..n {
                        let p = if *all_slices && t.slices() > 1 { dir.join(format!("{:016x}_m{mip}_s{sl}.png", t.hash)) } else { dir.join(format!("{:016x}_m{mip}.png", t.hash)) };
                        let r = sixng_core::ttpf::decode_rgba_slice(t, &data, *mip, sl).and_then(|(rgba, w, h)| sixng_core::texture::save_png(&p, &rgba, w, h).map(|_| (w, h)));
                        match r {
                            Ok((w, h)) => res.push(json!({"hash": format!("{:016x}", t.hash), "slice": sl, "slices": t.slices(), "png": p, "w": w, "h": h, "format": sixng_core::texture::dxgi_name(t.dxgi_format), "undecoded_bytes": skipped, "partial": skipped > 0})),
                            Err(e) => {
                                res.push(json!({"hash": format!("{:016x}", t.hash), "slices": t.slices(), "error": e.to_string()}));
                                break;
                            }
                        }
                    }
                    res
                };
                let res: Vec<serde_json::Value> = if *all { sel.par_iter().flat_map(|t| one(t)).collect() } else { sel.iter().flat_map(|t| one(t)).collect() };
                if *all {
                    let ok = res.iter().filter(|r| r.get("png").is_some()).count();
                    let partial = res.iter().filter(|r| r["partial"] == true).count();
                    let bcpack = res.iter().filter(|r| r["bcpack_only"] == true).count();
                    let failed = res.len() - ok - bcpack;
                    let idx = dir.join("index.json");
                    let note = "One PNG per texture (<hash>_m<mip>.png). partial = some of the data is in BCPack chunks other than BC1, BC3 and BC7, so part of the image is blank or noise; bcpack_only = nothing decodable yet, no PNG written. Names are not recovered (64-bit hashes).";
                    std::fs::write(&idx, serde_json::to_vec_pretty(&json!({"note": note, "store": a.id, "textures": st.textures.len(), "written": ok, "complete": ok - partial, "partial": partial, "bcpack_only": bcpack, "failed": failed, "results": res}))?)?;
                    out.print(&json!({"ok": true, "store": a.id, "textures": st.textures.len(), "written": ok, "complete": ok - partial, "partial": partial, "bcpack_only": bcpack, "failed": failed, "index": idx}));
                } else {
                    out.print(&json!(res));
                }
            }
        }
        Cmd::Placements { archive, filter, out: po, batches, batches_only } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::Resource];
            }
            let sel: Vec<_> = f.select(&ix)?.into_iter().filter(|e| e.sub == "7a").collect();
            let t = Instant::now();
            let bs = if *batches || *batches_only { Some(export::export_batches(&ws, &a, &sel, po, pr.clone())?) } else { None };
            if *batches_only {
                out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "batches": bs, "seconds": t.elapsed().as_secs_f32()}));
                return Ok(());
            }
            let v = export::export_placements(&ws, &a, &sel, po, pr)?;
            let ents: usize = v.iter().map(|x| x.entities).sum();
            let res: usize = v.iter().map(|x| x.resolved).sum();
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "map_files": v.len(), "entities": ents, "resolved": res, "batches": bs,
                "seconds": t.elapsed().as_secs_f64(), "index": po.join("placements").join(a.id.trim_end_matches(".rpf")).join("index.jsonl")}));
        }
        Cmd::Collision { archive, filter, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::RageObj];
            }
            let sel: Vec<_> = f.select(&ix)?.into_iter().filter(|e| e.name.is_none()).collect();
            let t = Instant::now();
            let (n, census) = export::export_collision(&a, &sel, po, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "bounds": n, "surface_ids": census.len(),
                "seconds": t.elapsed().as_secs_f64(), "index": po.join("collision").join(a.id.trim_end_matches(".rpf")).join("index.jsonl")}));
        }
        Cmd::Navmesh { archive, filter, roads_only, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::RageObj];
            }
            let sel: Vec<_> = f.select(&ix)?;
            let t = Instant::now();
            let (n, roads) = export::export_navmesh(&a, &sel, po, *roads_only, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "tiles": n, "tiles_with_roads": roads,
                "seconds": t.elapsed().as_secs_f64(), "index": po.join("navmesh").join(a.id.trim_end_matches(".rpf")).join("index.jsonl")}));
        }
        Cmd::Paths { archive, filter, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::Resource];
            }
            let sel: Vec<_> = f.select(&ix)?.into_iter().filter(|e| e.sub == "7a").collect();
            let t = Instant::now();
            let (n, l, bad) = export::export_paths(&a, &sel, po, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "nodes": n, "links": l, "unresolved_links": bad,
                "seconds": t.elapsed().as_secs_f64(), "file": po.join("paths").join(a.id.trim_end_matches(".rpf")).join("graph.json")}));
        }
        Cmd::Interiors { archive, filter, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::Resource];
            }
            let sel: Vec<_> = f.select(&ix)?.into_iter().filter(|e| e.sub == "7a").collect();
            let t = Instant::now();
            let (n, ok, mlos) = export::export_interiors(&a, &sel, &ix.entries, po, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "instances": n, "resolved": ok, "interior_archetypes": mlos,
                "seconds": t.elapsed().as_secs_f64(), "index": po.join("placements").join(a.id.trim_end_matches(".rpf")).join("interiors.jsonl")}));
        }
        Cmd::World { step, out: po } => {
            let steps: Vec<&str> = if step == "all" { sixng_core::world::STEPS.iter().map(|s| s.0).collect() } else { vec![step.as_str()] };
            for s in steps {
                let t = Instant::now();
                let msg = sixng_core::world::run_step(&ws, s, po, pr.clone())?;
                out.print(&json!({"ok": true, "step": s, "result": msg, "seconds": t.elapsed().as_secs_f64()}));
            }
        }
        Cmd::Surfaces { archive, navmeshes, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let nav = if navmeshes == "none" { None } else { Some(ws.resolve(navmeshes)?) };
            let nix = match &nav {
                Some(n) => Some(ws.load_index(n)?),
                None => None,
            };
            let nsel: Vec<sixng_core::scan::Entry> = nix.as_ref().map(|x| x.entries.iter().filter(|e| e.kind == Kind::RageObj).cloned().collect()).unwrap_or_default();
            let t = Instant::now();
            let inp = sixng_core::surface::SurfaceInputs { levels: &a, level_entries: &ix.entries, navmeshes: nav.as_ref().map(|n| (n, nsel.as_slice())) };
            let s = sixng_core::surface::surface_categories(&inp, po, &*pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "surface_ids": s.ids, "area_covered": s.area_covered, "area_by_confidence": s.by_confidence,
                "samples": s.samples, "seconds": t.elapsed().as_secs_f64(),
                "file": po.join("collision").join(a.id.trim_end_matches(".rpf")).join("surface_categories.json")}));
        }
        Cmd::Lights { archive, filter, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::Resource];
            }
            let sel: Vec<_> = f.select(&ix)?.into_iter().filter(|e| e.sub == "7a").collect();
            let t = Instant::now();
            let (sets, lights, placed) = export::export_lights(&ws, &a, &sel, po, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "light_sets": sets, "lights": lights, "placed_lights": placed,
                "seconds": t.elapsed().as_secs_f64(), "dir": po.join("lights").join(a.id.trim_end_matches(".rpf"))}));
        }
        Cmd::FindU32 { archive, values, filter } => {
            use rayon::prelude::*;
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::Resource];
            }
            let want: std::collections::HashSet<u32> =
                std::fs::read_to_string(values)?.lines().filter_map(|l| u32::from_str_radix(l.trim().trim_start_matches("0x"), 16).ok()).collect();
            let sel: Vec<_> = f.select(&ix)?.into_iter().filter(|e| e.sub == "7a").collect();
            let hits: Vec<(String, u32, usize, u32)> = sel
                .par_iter()
                .flat_map_iter(|e| {
                    let mut v = Vec::new();
                    if let Ok(r) = sixng_core::resource::Resource::load_sys(&a.path, e) {
                        if let Some(m) = sixng_core::meta7a::Meta::parse(&r) {
                            for &(h, size, o) in m.blocks.iter().filter(|b| b.2 != usize::MAX) {
                                let sz = m.structs.get(&h).map(|x| x.size as usize).unwrap_or(4).max(4);
                                for k in (0..size as usize).step_by(4) {
                                    let x = r.u32(o + k);
                                    if want.contains(&x) {
                                        v.push((e.id.clone(), h, k % sz, x));
                                    }
                                }
                            }
                        }
                    }
                    v
                })
                .collect();
            let mut by: std::collections::BTreeMap<String, (usize, Vec<String>)> = Default::default();
            for (id, h, k, x) in &hits {
                let e = by.entry(format!("{h:08x}+{k:x}")).or_default();
                e.0 += 1;
                if e.1.len() < 5 {
                    e.1.push(format!("{id}:{x:08x}"));
                }
            }
            out.print(&json!({"hits": hits.len(), "distinct_values": hits.iter().map(|x| x.3).collect::<std::collections::HashSet<_>>().len(), "by_struct_offset": by}));
        }
        Cmd::Meta { archive, filter, dump, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::Resource];
            }
            let sel: Vec<_> = f.select(&ix)?.into_iter().filter(|e| e.sub == "7a").collect();
            let t = Instant::now();
            let n = export::export_meta(&a, &sel, po, *dump, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "decoded": n, "seconds": t.elapsed().as_secs_f64(),
                "index": po.join("meta").join(a.id.trim_end_matches(".rpf")).join("index.jsonl")}));
        }
        Cmd::Shaders { archive, filter, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::RageObj];
            }
            if f.rtypes.is_empty() {
                f.rtypes = vec!["dr".into(), "ft".into(), "dd".into()];
            }
            let sel = f.select(&ix)?;
            let t = Instant::now();
            let v = export::shader_census(&a, &sel, pr)?;
            std::fs::write(po, serde_json::to_vec_pretty(&v)?)?;
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "shaders": v.as_object().map(|o| o.len()), "seconds": t.elapsed().as_secs_f64(), "out": po}));
        }
        Cmd::TextureGaps { archives, placements, out: po, file } => {
            let mut sel_all = Vec::new();
            for an in archives {
                let a = ws.resolve(an)?;
                let ix = ws.load_index(&a)?;
                let sel: Vec<sixng_core::scan::Entry> = ix
                    .entries
                    .iter()
                    .filter(|e| e.kind == Kind::RageObj && matches!(e.rtype.as_deref(), Some("dr") | Some("ft") | Some("dd")))
                    .cloned()
                    .collect();
                sel_all.push((a, sel));
            }
            let pl = if placements.is_empty() {
                sel_all.iter().map(|(a, _)| po.join("placements").join(a.id.trim_end_matches(".rpf"))).collect()
            } else {
                placements.clone()
            };
            let (mut v, models) = sixng_core::gaps::texture_gaps(&sel_all, &sixng_core::gaps::GapInputs { placements: pl, out: po.clone() }, pr)?;
            let f = file.clone().unwrap_or_else(|| po.join("texture_gaps.json"));
            let fm = f.with_file_name(format!("{}_models.jsonl.gz", f.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default()));
            sixng_core::gaps::write_models_gz(&fm, &models)?;
            v["models_file"] = json!(fm.file_name().map(|x| x.to_string_lossy().to_string()));
            v["models_in_file"] = json!(models.len());
            std::fs::write(&f, serde_json::to_vec_pretty(&v)?)?;
            out.print(&json!({"ok": true, "out": f, "models_file": fm, "models": models.len(), "summary": {"materials": v["summary"]["materials"], "textures_referenced": v["summary"]["textures_referenced"]}, "seconds": v["seconds"]}));
        }
        Cmd::LinkArchetypes { archive } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let metas: Vec<_> = ix.entries.iter().filter(|e| e.kind == Kind::Resource && e.sub == "7a").collect();
            let unnamed: Vec<_> = ix.entries.iter().filter(|e| e.kind == Kind::RageObj && (e.name.is_none() || matches!(e.rtype.as_deref(), Some("dr") | Some("ft")))).collect();
            let t = Instant::now();
            let v = export::link_archetypes(&ws, &a, &metas, &unnamed, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "stats": v, "seconds": t.elapsed().as_secs_f64(), "out": ws.index_dir().join("archetype_links.json")}));
        }
        Cmd::Pso { archive, filter, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::Pso];
            }
            let sel = f.select(&ix)?;
            let t = Instant::now();
            let (ok, bad) = sixng_core::pso::export(&a, &sel, po, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "decoded": ok, "failed": bad, "seconds": t.elapsed().as_secs_f64(), "out": po.join("pso")}));
        }
        Cmd::Handling { archive, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let sel: Vec<_> = ix.entries.iter().filter(|e| e.kind == Kind::Pso).collect();
            let names = model_names(&ws);
            let t = Instant::now();
            let v = sixng_core::pso::export_handling(&a, &sel, &names, po)?;
            out.print(&json!({"ok": true, "archive": a.id, "stats": v, "seconds": t.elapsed().as_secs_f64()}));
        }
        Cmd::Vehicles { archive, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let metas: Vec<_> = ix.entries.iter().filter(|e| e.kind == Kind::Resource && e.sub == "7a").collect();
            let names = model_names(&ws);
            let t = Instant::now();
            let v = sixng_core::vehicles::export(&a, &metas, &names, po)?;
            out.print(&json!({"ok": true, "archive": a.id, "stats": v, "seconds": t.elapsed().as_secs_f64()}));
        }
        Cmd::Movies { archive, out: po, extract, only } => {
            let a = ws.resolve(archive)?;
            let t = Instant::now();
            let movies = sixng_core::bink::scan(&a.path, &pr)?;
            let dir = po.join("movies");
            std::fs::create_dir_all(&dir)?;
            let pick: Option<Vec<String>> = only.as_ref().map(|s| s.split(',').map(|x| x.trim().trim_start_matches("0x").to_ascii_lowercase()).filter(|x| !x.is_empty()).collect());
            let mut written = Vec::new();
            if *extract {
                for m in &movies {
                    if pick.as_ref().is_some_and(|p| !p.contains(&m.offset_hex)) {
                        continue;
                    }
                    let dest = dir.join(format!("{}_{}x{}.bk2", m.offset_hex, m.width, m.height));
                    sixng_core::bink::extract(&a.path, m, &dest)?;
                    written.push(dest);
                }
            }
            let total: u64 = movies.iter().map(|m| m.size).sum();
            let secs: f64 = movies.iter().map(|m| m.seconds).sum();
            let file = dir.join("movies.json");
            std::fs::write(&file, serde_json::to_vec_pretty(&json!({
                "note": "Bink videos carved from the archive by their headers (4 KiB aligned; names are not recovered, so each is named by its offset). Frames are not decoded; the .bk2 files play in any Bink 2 player.",
                "archive": a.id, "count": movies.len(), "bytes": total, "seconds": secs, "movies": movies,
            }))?)?;
            let bad = movies.iter().filter(|m| !m.frame_table_ok).count();
            out.print(&json!({"ok": true, "archive": a.id, "movies": movies.len(), "frame_table_not_checked": bad, "bytes": total, "minutes": secs / 60.0, "extracted": written.len(), "file": file, "seconds": t.elapsed().as_secs_f64()}));
        }
        Cmd::InteriorEntities { archive } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let metas: Vec<_> = ix.entries.iter().filter(|e| e.kind == Kind::Resource && e.sub == "7a").collect();
            let t = Instant::now();
            let v = sixng_core::interiorents::build(&ws, &a, &metas, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "stats": v, "seconds": t.elapsed().as_secs_f64()}));
        }
        Cmd::LodLinks { archive } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let metas: Vec<_> = ix.entries.iter().filter(|e| e.kind == Kind::Resource && e.sub == "7a").collect();
            let t = Instant::now();
            let v = sixng_core::lodlinks::build(&ws, &a, &metas, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "stats": v, "seconds": t.elapsed().as_secs_f64()}));
        }
        Cmd::Audio { archive, rescan, min_seconds, limit } => {
            use sixng_core::audio;
            let a = ws.resolve(archive)?;
            let t = Instant::now();
            let ix = match (rescan, audio::load_index(&ws.work, &a.id)) {
                (false, Ok(ix)) => ix,
                _ => {
                    let ix = audio::scan(&a.path, &a.id, pr)?;
                    audio::save_index(&ws.work, &ix)?;
                    ix
                }
            };
            let mut layouts = std::collections::BTreeMap::<String, usize>::new();
            let mut codecs = std::collections::BTreeMap::<String, usize>::new();
            let mut secs = 0.0;
            for s in ix.banks.iter().flat_map(|b| &b.streams) {
                *layouts.entry(s.layout.clone()).or_default() += 1;
                if s.layout != "none" {
                    *codecs.entry(format!("{}", s.codec)).or_default() += 1;
                    secs += s.seconds();
                }
            }
            let list: Vec<_> = ix
                .banks
                .iter()
                .filter(|b| b.streams.iter().any(|s| s.layout != "none" && s.seconds() >= *min_seconds))
                .take(if *limit == 0 { usize::MAX } else { *limit })
                .map(|b| {
                    json!({"bank": format!("{:x}", b.off), "size": b.size, "seconds": b.seconds(), "streams": b.streams.iter().filter(|s| s.layout != "none" && s.seconds() >= *min_seconds).map(|s| json!({"id": format!("{:08x}", s.id), "seconds": s.seconds(), "samples": s.samples, "rate": s.rate, "codec": s.codec, "channels": s.channels, "layout": s.layout})).collect::<Vec<_>>()})
                })
                .collect();
            out.print(&json!({"ok": true, "archive": a.id, "index": audio::index_path(&ws.work, &a.id), "banks": ix.banks.len(), "streams_by_layout": layouts, "codec_bytes": codecs, "total_seconds": secs, "list": list, "seconds": t.elapsed().as_secs_f64(), "note": ix.note}));
        }
        Cmd::AudioExport { archive, bank, stream, format, out: dir, min_seconds, all } => {
            use sixng_core::audio;
            let a = ws.resolve(archive)?;
            let ix = audio::load_index(&ws.work, &a.id)?;
            if !["opus", "wav", "both", "awc"].contains(&format.as_str()) {
                bail!("--format must be opus, wav, both or awc");
            }
            let banks: Vec<&audio::Bank> = match bank {
                Some(h) => {
                    let off = u64::from_str_radix(h.trim_start_matches("0x"), 16)?;
                    vec![ix.banks.iter().find(|b| b.off == off).ok_or_else(|| anyhow!("no bank at {off:#x}"))?]
                }
                None if *all => ix.banks.iter().collect(),
                None => bail!("give --bank <hex> or --all"),
            };
            let sid = stream.as_ref().map(|h| u32::from_str_radix(h.trim_start_matches("0x"), 16)).transpose()?;
            let t = Instant::now();
            let (mut files, mut errors) = (Vec::new(), Vec::new());
            for (i, b) in banks.iter().enumerate() {
                pr("export", i as u64, banks.len() as u64);
                let bytes = audio::read_bank(&a.path, b)?;
                if format == "awc" {
                    std::fs::create_dir_all(dir)?;
                    let p = dir.join(format!("{:x}.awc", b.off));
                    std::fs::write(&p, &bytes)?;
                    files.push(p.display().to_string());
                    continue;
                }
                for s in &b.streams {
                    if sid.map_or(false, |x| x != s.id) || s.layout == "none" || s.seconds() < *min_seconds {
                        continue;
                    }
                    match audio::export_stream(&bytes, b, s, dir, format) {
                        Ok(f) => files.extend(f),
                        Err(e) => errors.push(e.to_string()),
                    }
                }
            }
            pr("export", banks.len() as u64, banks.len() as u64);
            out.print(&json!({"ok": true, "files": files.len(), "first": files.iter().take(20).collect::<Vec<_>>(), "errors": errors.len(), "first_errors": errors.iter().take(10).collect::<Vec<_>>(), "seconds": t.elapsed().as_secs_f64()}));
        }
        Cmd::RawDecode { input, dxgi, width, height, output } => {
            let d = std::fs::read(input)?;
            let f = sixng_core::texture::Fmt::from_dxgi(*dxgi);
            let rgba = sixng_core::texture::to_rgba(f, &d, *width, *height)?;
            sixng_core::texture::save_png(output, &rgba, *width, *height)?;
            out.print(&json!({"ok": true}));
        }
        Cmd::Inspect { id } => {
            let (aid, _) = id.rsplit_once('@').ok_or_else(|| anyhow!("id must be <archive>@<hexoffset>"))?;
            let a = ws.resolve(aid)?;
            let ix = ws.load_index(&a)?;
            let (e, member) = export::ModelResolver::new(&ix.entries).resolve(&ix.entries, &id)?;
            let selected = member.map(|name| export::member_entry(e, name)).transpose()?;
            let e = selected.as_ref().unwrap_or(e);
            let mut v = entry_json(e, true);
            if matches!(e.kind, Kind::RageObj | Kind::Resource) {
                let r = sixng_core::resource::Resource::load(&a.path, e)?;
                if e.rtype.as_deref() == Some("txd") {
                    v["textures"] = json!(sixng_core::texture::find_textures(&r));
                } else {
                    let mut info = sixng_core::drawable::describe(&r);
                    if let Some(name) = member {
                        sixng_core::drawable::select_member(&r, &mut info, name)?;
                    }
                    v["drawable"] = json!(info);
                }
            } else {
                let data = sixng_core::scan::read_entry(&a.path, e)?;
                v["head_hex"] = json!(sixng_core::rpf9::hex(&data[..data.len().min(64)]));
            }
            out.print(&json!({"ok": true, "entry": v}));
        }
    }
    Ok(())
}

fn do_export_collect(
    a: &sixng_core::game::ArchiveInfo,
    sel: &[&sixng_core::scan::Entry],
    oa: &OutArgs,
    pr: Progress,
) -> Result<serde_json::Value> {
    let (dec, est) = export::estimate(sel);
    if oa.dry_run {
        return Ok(json!({"archive": a.id, "selected": sel.len(), "decompressed_bytes": dec, "estimated_output_bytes": est,
            "ids": sel.iter().take(50).map(|e| &e.id).collect::<Vec<_>>()}));
    }
    let t = Instant::now();
    let items = export::export_many(a, sel, &oa.opts(), pr)?;
    let ok = items.iter().filter(|i| i.status == "ok").count();
    let skipped = items.iter().filter(|i| i.status == "skipped").count();
    // Keep the summary short: id + message only (full detail is in manifest.jsonl).
    let n_err = items.iter().filter(|i| i.status == "error").count();
    let errors: Vec<_> = items.iter().filter(|i| i.status == "error").take(50)
        .map(|i| json!({"id": i.id, "name": i.name, "error": i.error})).collect();
    let n_warn: usize = items.iter().map(|i| i.warnings.len()).sum();
    let warnings: Vec<_> = items.iter().flat_map(|i| i.warnings.iter().map(move |w| json!({"id": i.id, "warning": w}))).take(50).collect();
    Ok(json!({"archive": a.id, "selected": sel.len(), "exported": ok, "skipped": skipped,
        "error_count": n_err, "errors": errors, "warning_count": n_warn, "warnings": warnings, "seconds": t.elapsed().as_secs_f64(), "out": oa.out,
        "manifest": oa.out.join("manifest.jsonl")}))
}

fn do_export(a: &sixng_core::game::ArchiveInfo, sel: &[&sixng_core::scan::Entry], oa: &OutArgs, pr: Progress, out: &Out) -> Result<()> {
    let v = do_export_collect(a, sel, oa, pr)?;
    out.print(&json!({"ok": true, "result": v}));
    Ok(())
}

/// joaat(lower-case model name) -> name, from the recovered entry names of
/// every indexed archive (with and without the `_hi` suffix).
fn model_names(ws: &sixng_core::export::Workspace) -> std::collections::HashMap<u32, String> {
    sixng_core::vehicles::model_names(ws)
}
