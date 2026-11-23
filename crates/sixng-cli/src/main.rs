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
    /// Body colour of car-paint materials as sRGB hex (RRGGBB, or AARRGGBB
    /// as `sixng vehicles` lists it; alpha ignored). Without it paint is a
    /// neutral grey.
    #[arg(long)]
    paint: Option<String>,
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
            paint: self.paint.as_deref().map(|p| match sixng_core::material::parse_paint(p) {
                Some(c) => c,
                None => {
                    eprintln!("--paint wants RRGGBB or AARRGGBB hex, got {p:?}");
                    std::process::exit(2);
                }
            }),
            tex_remap: Default::default(),
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
    /// Strand hair grooms (hair, beards, brows, fur: unnamed packs.rpf resources) as GLB line strips or ribbons in model space: <out>/grooms/<archive>/<offset>.glb and index.jsonl (strands, points, bones, region, LOD chain). No link to peds is stored; pick them by region and bones.
    Grooms {
        /// Archive (default packs).
        #[arg(default_value = "packs")]
        archive: String,
        /// Also write the scalp triangles between strand roots.
        #[arg(long)]
        roots: bool,
        /// Write crossed ribbon meshes of this half-width (mm) instead of lines (render in any viewer).
        #[arg(long)]
        ribbons: Option<f32>,
        /// Only these groom offsets (hex, comma-separated).
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Only list the grooms (catalog with LOD chains and region) to <out>/grooms/<archive>/catalog.json.
        #[arg(long)]
        list: bool,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Verlet cloth and soft bodies (particles + constraint edges, drawn as lines): <out>/cloth/<archive>/<offset>.glb + index.jsonl.
    Cloth {
        /// Archive (default packs).
        #[arg(default_value = "packs")]
        archive: String,
        /// Only these resource offsets (hex, comma-separated).
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Text tables (localised UI text, subtitles, item names, phone and ambient speech; one table per language): <out>/text/<archive>/<offset>.json (label hash -> string) + index.jsonl. Labels are joaat(lowercase label); a subtitle's label is its voice-line name plus a 2-digit variation (AG_SUR_GEAR_FIRST1_01). With --label, prints those labels' strings from every table instead
    Text {
        /// Archive (default data; the minigame text is in pack/shared_minigame000).
        #[arg(default_value = "data")]
        archive: String,
        /// Only these resource offsets (hex, comma-separated).
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Look up these labels (names or 0x-hashes, comma-separated) and print their strings.
        #[arg(long, value_delimiter = ',')]
        label: Vec<String>,
        /// Strip the inline markup (~sl:…~, ~b~, ~n~ …) from printed strings.
        #[arg(long)]
        plain: bool,
        /// Output directory (needed unless --label is given).
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// Head identity deltas (per-head vertex offsets from the class's base head): matches each to its head drawable, recovers the vertex order and with --build writes the heads that have no drawable as GLBs (<out>/models/<archive>/head_<class>_delta_<offset>/model.glb). Index: <out>/head_deltas/<archive>/index.jsonl.
    HeadDeltas {
        /// Archive (default packs).
        #[arg(default_value = "packs")]
        archive: String,
        /// Build GLBs of the LOD0 heads that exist only as deltas.
        #[arg(long)]
        build: bool,
        /// With --build: only these delta offsets (hex, comma-separated).
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
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
    /// Names in the world exports, from common.rpf's XML: collision surface ids -> physics material names (`surface_names.json`, `names_by_surface_id` in surfaces.json, `name` in surface_categories.json) and road-node street hashes -> location key, street label and text (`street_key`, `street_label`, `street_text` on the nodes of graph.json). Run after `collision`, `surfaces` and `paths`; needs common (and data for the street text) indexed.
    Annotate {
        /// Archive the exports came from (default levels).
        #[arg(default_value = "levels")]
        archive: String,
        /// Output directory (the same one the other exports wrote to).
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
        /// With the dump, also write every struct instance by struct hash
        /// ("instances"); for resources whose records the root does not reach.
        #[arg(long)]
        instances: bool,
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
    /// Which materials use each texture: every material of every model (drawables, fragments, dictionary members) with its texture slots, so a base colour texture leads to the same material's normal map, packed roughness/metal map and so on. Writes <work>/index/texture_users.jsonl.gz, which the GUI's Textures tab reads ("Used by"). With --texture, prints the materials of one texture instead.
    TextureUsers {
        /// Archives to scan (default: every indexed archive with models).
        archives: Vec<String>,
        /// Print the materials (and their other slots) that use this texture hash (16 hex digits), from the existing index.
        #[arg(long)]
        texture: Option<String>,
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
    /// Particle effect dictionaries (unnamed resources with keyframe
    /// textures; data.rpf has 147): per dictionary its name (scr_arc_tl),
    /// effect/emitter rule names and keyframe-texture count, as JSON. With
    /// -o, also <out>/ptfx/<archive>/index.json and per dictionary
    /// <offset>.json with the keyframe curves (16 RGBA samples x 4 rows per
    /// keyframe texture) and the rules: each effect's events (emitter rule +
    /// particle rule), keyframe properties (name, keys over time), behaviour
    /// classes, each particle rule's keyframe texture and its sprite
    /// textures (`particles[].sprites`: texture-store hash, store, size and
    /// format; the sprites live in data/ttdb/vfx/ttdb_vfx.ttpf). With
    /// --sprites, also writes each sprite to <out>/ptfx/<archive>/sprites/<hash>.png
    /// and adds a flipbook grid guess (`grid_guess`) for animated particles
    /// (`particles[].frames`).
    Ptfx {
        #[arg(default_value = "data")]
        archive: String,
        /// Regex on the dictionary name or any rule name.
        #[arg(long)]
        name: Option<String>,
        #[arg(long, short)]
        out: Option<PathBuf>,
        /// With -o: decode the referenced sprite textures to PNG.
        #[arg(long)]
        sprites: bool,
    },
    /// Fonts of the UI movies (Scaleform font libraries in data.rpf:
    /// DefineFont3 outlines, code table, layout). Lists them as JSON; with
    /// -o writes <out>/fonts/<name>.ttf (TrueType, for CSS @font-face) and
    /// <name>.typeface.json (three.js FontLoader) per font. A font found in
    /// several movies is written once, from the copy with the most glyphs.
    /// The fonts are the owners'; for local use.
    Fonts {
        #[arg(default_value = "data")]
        archive: String,
        /// Regex on the font name.
        #[arg(long)]
        name: Option<String>,
        #[arg(long, short)]
        out: Option<PathBuf>,
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
    /// Ped wardrobe variations (packs.rpf meta resources with struct f795e8ba, one per ped model): per variation its parts (slot, name, drawable), as JSON. Drawables are found through the item's textures, so run `texture-users` first. A variation is a clothing set or a head set (head, eyes, teeth); a full look is one of each. `parts` and `tex_remap` per variation are ready for `outfit <name> --parts … --tex-remap …` (the remap carries the variation's own skin / dirt / blood textures).
    PedVariations {
        /// Resource offsets (hex) or entry ids, e.g. 2b21010
        #[arg(required = true)]
        ids: Vec<String>,
        #[arg(long, default_value = "packs")]
        archive: String,
    },
    /// Script data documents (script.rpf): the binary XML documents in its PSO files to XML text, `<out>/script_docs/<archive>/<offset>.xml` plus index.jsonl. Tags, attribute names and string values are named from the name dictionary (`sixng names`); the rest stay hashes.
    ScriptDocs {
        #[arg(default_value = "script")]
        archive: String,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Audio metadata files (the sound, game-object, curve, speech, music and mixer definitions beside a pack's banks: plain files with 0x045a88ea at +8): per file its schema id, every object's name hash, offset, size and class id (first byte), as <out>/audio_meta/<archive>/<offset>.json + index.jsonl. Sound files (schema 166) also get each simple sound's wave (container hash, stream id and, when the archive's audio index exists, its bank), the child sounds of randomized, sequential, wrapper and eleven other sound classes, and every sound's reachable (bank, stream) pairs. Names are hashes; other class fields are not decoded.
    AudioMeta {
        archive: String,
        #[arg(short, long)]
        out: PathBuf,
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
    /// Export audio: Ogg Opus (lossless remux), WAV (decoded) or the raw bank (.awc). PCM streams always export as WAV and XMA2 streams as .xma (RIFF XMA2, lossless remux). Needs `sixng audio` first.
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
        /// opus | wav | both | awc (raw bank). Applies to Opus streams; PCM gives .wav and XMA2 .xma whatever the choice (except awc).
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
    /// Export a character or an animal as ONE rigged GLB: its outfit parts
    /// (head, hair, top, bottom, shoes …; the default look, or --parts), or
    /// an animal's parts (`crb_eagle_bald_m_00`: body, feathers, eyes …),
    /// skinned to one shared skeleton, optionally with an animation clip as
    /// a glTF animation. Imports into Blender as a single armature. Needs
    /// packs (and for --clip, anim) indexed.
    Outfit {
        /// Character prefix (player_story000_00, cs_ali, sc_slow_demon) or
        /// animal root (crb_eagle_bald_m_00, crm_deer_agh_f_00). Any other
        /// name with --parts makes an ad-hoc character from generic items
        /// (world peds such as hikers have no named parts).
        character: String,
        /// Output .glb file. Textures go to a textures/ folder next to it.
        #[arg(long, short)]
        out: Option<PathBuf>,
        /// Archive holding the parts.
        #[arg(long, default_value = "packs")]
        archive: String,
        /// Parts to use instead of the default look: regexes, each picks the
        /// first part (in name order) whose name matches; when none of the
        /// character's parts match, the first drawable in the archive that
        /// does (generic clothing, accessories such as schornsdemon_mr1_000).
        #[arg(long, value_delimiter = ',')]
        parts: Vec<String>,
        /// Parts to add to the default look (regexes, as for --parts).
        #[arg(long, value_delimiter = ',')]
        add: Vec<String>,
        /// List the character's parts (group, name, default) and stop.
        #[arg(long)]
        list: bool,
        /// Animation clip: <dictionary>/<clip>, the dictionary by name or
        /// entry id (anim.rpf@…), e.g.
        /// gameplay@locomotion@female_character@kat@normal@standing@walk@stance_neutral/move.
        /// Repeat (or separate with commas) for several clips: each becomes
        /// one glTF animation named <dictionary>/<clip>. `sixng clips` lists
        /// dictionaries and clips.
        #[arg(long, value_delimiter = ',')]
        clip: Vec<String>,
        /// Write only the skeleton and the --clip animations (no meshes, no
        /// textures): small clip files for the full model, whose joints
        /// have the same names.
        #[arg(long)]
        anim_only: bool,
        /// Texture output: png, dds, or both.
        #[arg(long, default_value = "png")]
        tex: String,
        /// Body shape, as for extract (e.g. "gcweightbelly=0.5").
        #[arg(long)]
        body_shape: Option<String>,
        /// Face variant for the head, as for extract.
        #[arg(long, default_value_t = 0)]
        head_skin: usize,
        /// Texture replacements, `old=new` 64-bit store hashes (hex), comma
        /// separated: a ped variation's skin / dirt / blood overrides, as
        /// `ped-variations` lists them (`tex_remap`).
        #[arg(long, value_delimiter = ',')]
        tex_remap: Vec<String>,
    },
    /// Characters and animals that `outfit` can assemble: named characters
    /// (outfit prefixes such as cs_ali, sc_slow_demon) and animal roots
    /// (crb_eagle_bald_m_00), with their part counts. JSON to stdout.
    Characters {
        /// Regex on the name.
        #[arg(long)]
        name: Option<String>,
        /// "character" or "animal".
        #[arg(long)]
        kind: Option<String>,
        /// Archive holding the parts.
        #[arg(long, default_value = "packs")]
        archive: String,
    },
    /// Animation clips in anim.rpf: list the clip dictionaries (filter with
    /// --name), or the clips of one dictionary (by name or entry id). The
    /// first listing reads every resource of the anim.rpf index once (about
    /// a minute) and caches the names in <work>/index/anim.clipdicts.json.
    Clips {
        /// Clip dictionary name or entry id (anim.rpf@…).
        dict: Option<String>,
        /// Regex on dictionary names (listing) or clip names (one dictionary).
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value_t = 200)]
        limit: usize,
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
        Cmd::Outfit { character, out: glb, archive, parts, add, list, clip, anim_only, tex, body_shape, head_skin, tex_remap } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let prefix = character.to_ascii_lowercase();
            let animal = sixng_core::outfit::is_animal(&prefix);
            let opts = if animal { sixng_core::outfit::animal_options(&ix, &prefix) } else { sixng_core::outfit::outfit_options(&ix, &prefix) };
            // a name with no parts of its own is an ad-hoc character made of
            // --parts (generic world peds: head, hand, feet, clothing items)
            if opts.is_empty() && parts.is_empty() {
                bail!("no parts named {prefix}_<item>_<fit>_… (characters) or {prefix}_<part>_NN (animals) in {} (`sixng search '^{prefix}_'` to check the name), and no --parts for an ad-hoc character", a.id);
            }
            if *list {
                let v: Vec<_> = opts.iter().map(|o| json!({"group": o.group, "name": o.name, "id": o.id, "default": o.default})).collect();
                out.print(&json!({"ok": true, "character": prefix, "count": v.len(), "parts": v}));
                return Ok(());
            }
            let pick = |pats: &[String]| -> Result<Vec<u64>> {
                let mut v = Vec::new();
                for p in pats {
                    let re = regex::RegexBuilder::new(p).case_insensitive(true).build()?;
                    // the character's own parts first, then any drawable in the
                    // archive (generic clothing, `sc*demon` horns …), LOD0 copy
                    let off = match opts.iter().find(|o| re.is_match(&o.name)) {
                        Some(o) => o.off,
                        None => {
                            let any = sixng_core::outfit::dedupe(ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| {
                                let n = e.name.as_ref()?.to_ascii_lowercase();
                                re.is_match(&n).then_some((n, e))
                            }));
                            any.values().next().map(|e| e.off).ok_or_else(|| anyhow!("no part of {prefix} and no drawable in {} matches {p:?} (see --list)", a.id))?
                        }
                    };
                    v.push(off);
                }
                Ok(v)
            };
            let mut offs: Vec<u64> = if parts.is_empty() { opts.iter().filter(|o| o.default).map(|o| o.off).collect() } else { pick(parts)? };
            for o in pick(add)? {
                if !offs.contains(&o) {
                    offs.push(o);
                }
            }
            let sel: Vec<&sixng_core::scan::Entry> = offs.iter().filter_map(|o| ix.entries.iter().find(|e| e.off == *o)).collect();
            let glb = glb.clone().unwrap_or_else(|| PathBuf::from(format!("{prefix}.glb")));
            let oa = OutArgs { out: glb.parent().map(|p| p.to_path_buf()).unwrap_or_default(), tex: tex.clone(), raw: false, force: true, no_embedded_textures: false, dry_run: false, body_shape: body_shape.clone(), head_skin: *head_skin, paint: None };
            if *anim_only && clip.is_empty() {
                bail!("--anim-only needs at least one --clip");
            }
            let mut dicts: Vec<sixng_core::anim::ClipDictionary> = Vec::new();
            let mut picks: Vec<(usize, usize)> = Vec::new();
            for c in clip {
                let (dq, cq) = c.rsplit_once('/').ok_or_else(|| anyhow!("--clip wants <dictionary>/<clip>"))?;
                let (_, d) = sixng_core::anim::load_dictionary(&game, &ws.work, dq)?;
                let di = match dicts.iter().position(|x| x.name == d.name) {
                    Some(i) => i,
                    None => {
                        dicts.push(d);
                        dicts.len() - 1
                    }
                };
                let d = &dicts[di];
                let ci = d.clip(cq).and_then(|want| d.clips.iter().position(|x| std::ptr::eq(x, want)));
                let Some(ci) = ci else {
                    let names: Vec<&str> = d.clips.iter().map(|c| c.short_name()).collect();
                    bail!("no clip {cq:?} in {}; it has: {}", d.name, names.join(", "));
                };
                picks.push((di, ci));
            }
            let clip_refs: Vec<(&sixng_core::anim::ClipDictionary, &sixng_core::anim::Clip)> = picks.iter().map(|&(di, ci)| (&dicts[di], &dicts[di].clips[ci])).collect();
            let t = Instant::now();
            let mut eo = oa.opts();
            for kv in tex_remap {
                let (k, v) = kv.split_once('=').ok_or_else(|| anyhow!("--tex-remap wants old=new, got {kv:?}"))?;
                let hex = |s: &str| u64::from_str_radix(s.trim().trim_start_matches("0x"), 16).map_err(|_| anyhow!("--tex-remap: bad hash {s:?}"));
                eo.tex_remap.insert(hex(k)?, hex(v)?);
            }
            let mut r = sixng_core::outfit::export_outfit(&a, &sel, &prefix, &glb, &eo, &clip_refs, *anim_only)?;
            r["seconds"] = t.elapsed().as_secs_f64().into();
            out.print(&json!({"ok": true, "result": r}));
        }
        Cmd::Characters { name, kind, archive } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let re = name.as_deref().map(|n| regex::RegexBuilder::new(n).case_insensitive(true).build()).transpose()?;
            let v: Vec<_> = sixng_core::outfit::characters(&ix)
                .into_iter()
                .filter(|c| re.as_ref().is_none_or(|r| r.is_match(&c.name)) && kind.as_deref().is_none_or(|k| k == c.kind))
                .collect();
            out.print(&json!({"ok": true, "archive": a.id, "count": v.len(), "characters": v}));
        }
        Cmd::Clips { dict, name, limit } => {
            let re = name.as_deref().map(|n| regex::RegexBuilder::new(n).case_insensitive(true).build()).transpose()?;
            match dict {
                None => {
                    let all = sixng_core::anim::dictionaries(&game, &ws.work)?;
                    let hits: Vec<_> = all.iter().filter(|d| re.as_ref().is_none_or(|r| r.is_match(&d.name))).collect();
                    out.print(&json!({"ok": true, "dictionaries": all.len(), "count": hits.len(), "matches": hits.iter().take(*limit).map(|d| json!({"name": d.name, "id": d.id})).collect::<Vec<_>>()}));
                }
                Some(q) => {
                    let (de, d) = sixng_core::anim::load_dictionary(&game, &ws.work, q)?;
                    let clips: Vec<_> = d
                        .clips
                        .iter()
                        .filter(|c| re.as_ref().is_none_or(|r| r.is_match(&c.name)))
                        .take(*limit)
                        .map(|c| {
                            let tr = sixng_core::anim::all_tracks(&d, c);
                            json!({"name": c.short_name(), "hash": format!("{:08x}", c.hash), "type": c.kind, "duration": c.duration, "bindings": c.bindings.len(),
                                "tracks": tr.len(), "bones": tr.iter().filter(|t| t.0 != 0).map(|t| t.0).collect::<std::collections::BTreeSet<_>>().len()})
                        })
                        .collect();
                    out.print(&json!({"ok": true, "dictionary": de.name, "id": de.id, "clip_count": d.clips.len(), "animations": d.animations.len(), "clips": clips}));
                }
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
                    // Data entirely in BCPack chunks that do not decode: the PNG
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
                    let note = "One PNG per texture (<hash>_m<mip>.png). partial = some of the data is in BCPack chunks that did not decode, so part of the image is blank or noise; bcpack_only = nothing decodable yet, no PNG written. Names are not recovered (64-bit hashes).";
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
        Cmd::Grooms { archive, roots, ribbons, only, list, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let t = Instant::now();
            if *list {
                let cat = sixng_core::groom::catalog(&a, &ix.entries)?;
                let dir = po.join("grooms").join(a.id.trim_end_matches(".rpf"));
                std::fs::create_dir_all(&dir)?;
                std::fs::write(dir.join("catalog.json"), serde_json::to_vec_pretty(&cat)?)?;
                let mut by: std::collections::BTreeMap<&str, usize> = Default::default();
                for g in cat.iter().filter(|g| g.lod == 0) {
                    *by.entry(g.category.as_str()).or_default() += 1;
                }
                out.print(&json!({"ok": true, "grooms": cat.len(), "chains": cat.iter().filter(|g| g.lod == 0).count(), "chains_by_region": by, "file": dir.join("catalog.json"), "seconds": t.elapsed().as_secs_f64()}));
            } else {
                let only: Vec<u64> = only.iter().filter_map(|h| u64::from_str_radix(h.trim_start_matches("0x"), 16).ok()).collect();
                let (n, errors) = sixng_core::groom::export_grooms(&a, &ix.entries, &only, po, *roots, ribbons.map(|mm| mm / 1000.0), pr)?;
                out.print(&json!({"ok": true, "archive": a.id, "grooms": n, "errors": errors,
                    "seconds": t.elapsed().as_secs_f64(), "index": po.join("grooms").join(a.id.trim_end_matches(".rpf")).join("index.jsonl")}));
            }
        }
        Cmd::Cloth { archive, only, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let t = Instant::now();
            let only: Vec<u64> = only.iter().filter_map(|h| u64::from_str_radix(h.trim_start_matches("0x"), 16).ok()).collect();
            let (n, errors) = sixng_core::cloth::export_cloth(&a, &ix.entries, &only, po, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "cloth": n, "errors": errors,
                "seconds": t.elapsed().as_secs_f64(), "index": po.join("cloth").join(a.id.trim_end_matches(".rpf")).join("index.jsonl")}));
        }
        Cmd::Text { archive, only, label, plain, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let t = Instant::now();
            let only: Vec<u64> = only.iter().filter_map(|h| u64::from_str_radix(h.trim_start_matches("0x"), 16).ok()).collect();
            let (tables, errors) = sixng_core::text::read_all(&a, &ix.entries, &only, pr)?;
            if !label.is_empty() {
                let keys: Vec<(String, u32)> = label
                    .iter()
                    .map(|l| {
                        let h = l.strip_prefix("0x").and_then(|x| u32::from_str_radix(x, 16).ok()).unwrap_or_else(|| sixng_core::meta7a::joaat(l));
                        (l.clone(), h)
                    })
                    .collect();
                let found: Vec<serde_json::Value> = keys
                    .iter()
                    .map(|(l, h)| {
                        let hits: Vec<serde_json::Value> = tables
                            .iter()
                            .filter_map(|t| t.strings.get(h).map(|s| json!({"table": t.id, "text": if *plain { sixng_core::text::plain(s) } else { s.clone() }})))
                            .collect();
                        json!({"label": l, "hash": format!("{h:08x}"), "found": hits})
                    })
                    .collect();
                out.print(&json!({"ok": true, "archive": a.id, "tables": tables.len(), "labels": found}));
            } else {
                let Some(po) = po else { bail!("--out is needed unless --label is given") };
                let (n, strings) = sixng_core::text::export(&a, &tables, po)?;
                out.print(&json!({"ok": true, "archive": a.id, "tables": n, "strings": strings, "errors": errors,
                    "seconds": t.elapsed().as_secs_f64(), "index": po.join("text").join(sixng_core::game::index_stem(&a.id)).join("index.jsonl")}));
            }
        }
        Cmd::HeadDeltas { archive, build, only, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let t = Instant::now();
            let only: Vec<u64> = only.iter().filter_map(|h| u64::from_str_radix(h.trim_start_matches("0x"), 16).ok()).collect();
            let rows = sixng_core::headdelta::run(&a, &ix.entries, po, *build, &only, pr)?;
            let mut by: std::collections::BTreeMap<String, [usize; 4]> = Default::default();
            for r in &rows {
                let c = by.entry(format!("{} lod{}", r.class, r.lod)).or_default();
                c[0] += 1;
                c[1] += r.head.is_some() as usize;
                c[2] += r.head_by_order.is_some() as usize;
                c[3] += r.file.is_some() as usize;
            }
            out.print(&json!({"ok": true, "archive": a.id, "deltas": rows.len(), "by_class_lod_total_matched_named_built": by,
                "seconds": t.elapsed().as_secs_f64(), "index": po.join("head_deltas").join(a.id.trim_end_matches(".rpf")).join("index.jsonl")}));
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
        Cmd::Annotate { archive, out: po } => {
            let a = ws.resolve(archive)?;
            let t = Instant::now();
            let s = sixng_core::gamenames::annotate(&ws, &a.id, po, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "result": s, "seconds": t.elapsed().as_secs_f64()}));
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
        Cmd::Meta { archive, filter, dump, instances, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::Resource];
            }
            let sel: Vec<_> = f.select(&ix)?.into_iter().filter(|e| e.sub == "7a").collect();
            let t = Instant::now();
            let n = export::export_meta(&a, &sel, po, *dump, *instances, pr)?;
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
        Cmd::TextureUsers { archives, texture } => {
            let t = Instant::now();
            if let Some(h) = texture {
                let h = u64::from_str_radix(h.trim_start_matches("0x"), 16).map_err(|_| anyhow::anyhow!("--texture wants a hex hash"))?;
                let tu = sixng_core::texusers::TexUsers::load(&sixng_core::texusers::path(&ws))?;
                let mats: Vec<serde_json::Value> = tu
                    .users(h)
                    .iter()
                    .map(|&m| {
                        let mat = &tu.materials[m as usize];
                        let users = &tu.material_models[m as usize];
                        json!({"shader": mat.shader, "slots": mat.slots.iter().map(|(n, x)| json!({"param": n, "texture": format!("{x:016x}")})).collect::<Vec<_>>(),
                            "models": users.len(), "examples": users.iter().take(5).map(|&i| json!({"id": tu.models[i as usize].0, "name": tu.models[i as usize].1})).collect::<Vec<_>>()})
                    })
                    .collect();
                out.print(&json!({"ok": true, "texture": format!("{h:016x}"), "materials": mats}));
            } else {
                let sel = sixng_core::texusers::archives(&ws, archives)?;
                let s = sixng_core::texusers::build(&ws, &sel, pr)?;
                out.print(&json!({"ok": true, "archives": sel.iter().map(|a| a.0.id.clone()).collect::<Vec<_>>(), "result": s, "seconds": t.elapsed().as_secs_f64()}));
            }
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
        Cmd::Ptfx { archive, name, out: po, sprites } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let re = name.as_deref().map(|n| regex::RegexBuilder::new(n).case_insensitive(true).build()).transpose()?;
            use rayon::prelude::*;
            let cands: Vec<&sixng_core::scan::Entry> = ix.entries.iter().filter(|e| e.kind == Kind::RageObj && e.rtype.is_none()).collect();
            let mut found: Vec<(sixng_core::ptfx::PtfxDict, Option<serde_json::Value>)> = cands
                .par_iter()
                .filter_map(|e| {
                    let r = sixng_core::resource::Resource::load(&a.path, e).ok()?;
                    let info = sixng_core::drawable::describe(&r);
                    if !sixng_core::ptfx::is_ptfx(&info) {
                        return None;
                    }
                    let d = sixng_core::ptfx::describe(&e.id, e.usize, &info);
                    if let Some(re) = &re {
                        if !d.name.iter().chain(d.rules.iter()).any(|s| re.is_match(s)) {
                            return None;
                        }
                    }
                    let kf = po.as_ref().map(|_| json!({"keyframes": sixng_core::ptfx::keyframes(&r, &info), "rules": sixng_core::ptfx::rules(&r, &info)}));
                    Some((d, kf))
                })
                .collect();
            found.sort_by(|x, y| x.0.id.cmp(&y.0.id));
            if let Some(po) = po {
                let dir = po.join("ptfx").join(a.id.trim_end_matches(".rpf").replace(['/', '\\'], "_"));
                std::fs::create_dir_all(&dir)?;
                let set = sixng_core::ttpf::global(&ws.game);
                // hash -> frame counts of the animated particles that use it
                let mut wanted: std::collections::BTreeMap<u64, std::collections::BTreeSet<u32>> = Default::default();
                let mut docs: Vec<(String, serde_json::Value)> = Vec::new();
                for (d, kf) in &found {
                    let off = d.id.rsplit('@').next().unwrap_or("x").to_string();
                    let mut kf = kf.clone().unwrap_or_default();
                    // Sprite textures: texture_refs are texture-store hashes
                    // (237 of the 242 distinct values in data.rpf are in the vfx store).
                    if let Some(ps) = kf["rules"]["particles"].as_array_mut() {
                        for pt in ps {
                            let frames = pt["frames"].as_u64().map(|n| n as u32);
                            let refs = pt["texture_refs"].as_array().cloned().unwrap_or_default();
                            let sp: Vec<serde_json::Value> = refs
                                .iter()
                                .filter_map(|r| {
                                    let slot = r[0].as_u64()?;
                                    let hs = r[1].as_str()?;
                                    let h = u64::from_str_radix(hs, 16).ok()?;
                                    Some(match set.get(h) {
                                        Some((st, t)) => {
                                            if *sprites {
                                                let e = wanted.entry(h).or_default();
                                                if let Some(n) = frames {
                                                    e.insert(n);
                                                }
                                            }
                                            let store = st.path.strip_prefix(&ws.game).unwrap_or(&st.path).to_string_lossy().replace('\\', "/");
                                            json!({"slot": slot, "hash": hs, "store": store, "w": t.width, "h": t.height, "format": sixng_core::texture::dxgi_name(t.dxgi_format), "png": if *sprites { json!(format!("sprites/{hs}.png")) } else { serde_json::Value::Null }})
                                        }
                                        None => json!({"slot": slot, "hash": hs, "store": null}),
                                    })
                                })
                                .collect();
                            pt["sprites"] = json!(sp);
                        }
                    }
                    docs.push((off, json!({"dictionary": d, "keyframes": kf["keyframes"], "rules": kf["rules"], "rules_note": "effects[].events pair an emitter rule with a particle rule. Keyframe properties: id = joaat of the lower-cased `<class>:m_<member>KFP`, name where it resolved (all effect and emitter properties, about 70 % of behaviour ones; ptxu_Velocity:m_angleKFP is less certain), keys = [time 0..1, value vec4]; min/max pairs are separate properties or x/y of one. Scalars such as sizeScalar look like percentages (medians 70-100; unverified). Particle props are grouped by behaviour, with the behaviour class taken from its property names. particles[].keyframe_texture indexes `keyframes` (probably the particle shader's variables, not colour or size: those are the behaviour properties, colour in the two ptxu_Colour props with a `role`); particles[].sprites resolve particles[].texture_refs (slot offset, 64-bit texture-store hash) to textures in the vfx store: by format the slots hold flipbook atlases and colour sprites (BC7 / BC6H), masks (BC4), normal or lighting maps (BC5) and 4-pixel-high colour ramps (BC6H); the role of each slot is inferred from formats only. particles[].frames is the flipbook frame count (last frame id + 1, from the ptxu_AnimateTexture behaviour). With --sprites, each sprite of an animated particle gets grid_guess {cols, rows, score}: the grid (dividing the image evenly, cols x rows >= frames) under which consecutive frames differ least; score = adjacent / shuffled frame difference, about 0.1-0.25 on smoke and fire sheets, near 1 when the sheet holds random variants rather than an animation. The grid itself is not stored in the data."})));
                }
                let mut grids: std::collections::HashMap<(u64, u32), (u32, u32, f32)> = Default::default();
                if !wanted.is_empty() {
                    let sd = dir.join("sprites");
                    std::fs::create_dir_all(&sd)?;
                    use rayon::prelude::*;
                    let res: Vec<(u64, Result<Vec<(u32, (u32, u32, f32))>>)> = wanted
                        .par_iter()
                        .filter_map(|(&h, ns)| {
                            let (st, t) = set.get(h)?;
                            let p = sd.join(format!("{h:016x}.png"));
                            let r = st.data_ex(t).and_then(|(data, _)| sixng_core::ttpf::decode_rgba_slice(t, &data, 0, 0)).and_then(|(rgba, w, hh)| {
                                sixng_core::texture::save_png(&p, &rgba, w, hh)?;
                                Ok(ns.iter().filter_map(|&n| sixng_core::ptfx::flipbook_grid(&rgba, w, hh, n).map(|g| (n, g))).collect())
                            });
                            Some((h, r))
                        })
                        .collect();
                    let mut fails = Vec::new();
                    for (h, r) in res {
                        match r {
                            Ok(v) => grids.extend(v.into_iter().map(|(n, g)| ((h, n), g))),
                            Err(e) => fails.push(format!("{h:016x}: {e}")),
                        }
                    }
                    if !fails.is_empty() {
                        eprintln!("sprites not decoded: {}", fails.join("; "));
                    }
                }
                for (off, mut doc) in docs {
                    if let Some(ps) = doc["rules"]["particles"].as_array_mut() {
                        for pt in ps {
                            let Some(n) = pt["frames"].as_u64().map(|n| n as u32) else { continue };
                            if let Some(sp) = pt["sprites"].as_array_mut() {
                                for s in sp {
                                    let Some(h) = s["hash"].as_str().and_then(|x| u64::from_str_radix(x, 16).ok()) else { continue };
                                    if let Some(&(c, r, sc)) = grids.get(&(h, n)) {
                                        s["grid_guess"] = json!({"cols": c, "rows": r, "score": (sc * 100.0).round() / 100.0});
                                    }
                                }
                            }
                        }
                    }
                    std::fs::write(dir.join(format!("{off}.json")), serde_json::to_vec_pretty(&doc)?)?;
                }
                std::fs::write(dir.join("index.json"), serde_json::to_vec_pretty(&found.iter().map(|x| &x.0).collect::<Vec<_>>())?)?;
            }
            out.print(&json!({"ok": true, "archive": a.id, "count": found.len(), "dictionaries": found.iter().map(|x| &x.0).collect::<Vec<_>>()}));
        }
        Cmd::Fonts { archive, name, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let re = name.as_deref().map(|n| regex::RegexBuilder::new(n).case_insensitive(true).build()).transpose()?;
            use rayon::prelude::*;
            let movies: Vec<&sixng_core::scan::Entry> = ix.entries.iter().filter(|e| e.kind == Kind::Gfx).collect();
            let all: Vec<(String, sixng_core::gfxfont::Font)> = movies
                .par_iter()
                .flat_map_iter(|e| {
                    let fs = sixng_core::scan::read_entry(&a.path, e).ok().and_then(|b| sixng_core::gfxfont::fonts(&b).ok()).unwrap_or_default();
                    fs.into_iter().map(move |f| (e.id.clone(), f))
                })
                .collect();
            let mut best: std::collections::BTreeMap<String, (String, sixng_core::gfxfont::Font, usize)> = Default::default();
            for (id, f) in all {
                if f.glyphs.is_empty() || re.as_ref().is_some_and(|r| !r.is_match(&f.name)) {
                    continue;
                }
                let key = format!("{}{}{}", f.name, if f.bold { " bold" } else { "" }, if f.italic { " italic" } else { "" });
                let e = best.entry(key).or_insert((id.clone(), f.clone(), 0));
                e.2 += 1;
                if f.glyphs.len() > e.1.glyphs.len() {
                    e.0 = id;
                    e.1 = f;
                }
            }
            let dir = po.as_ref().map(|p| p.join("fonts"));
            if let Some(d) = &dir {
                std::fs::create_dir_all(d)?;
            }
            let mut list = Vec::new();
            for (key, (id, f, copies)) in &best {
                let file: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
                if let Some(d) = &dir {
                    std::fs::write(d.join(format!("{file}.ttf")), sixng_core::gfxfont::to_ttf(f))?;
                    std::fs::write(d.join(format!("{file}.typeface.json")), serde_json::to_vec(&sixng_core::gfxfont::to_typeface(f))?)?;
                }
                let sample: String = f.glyphs.iter().filter_map(|g| char::from_u32(g.code as u32)).take(48).collect();
                list.push(json!({"name": f.name, "bold": f.bold, "italic": f.italic, "file": file, "glyphs": f.glyphs.len(), "movie": id, "movies": copies, "chars": sample}));
            }
            if let Some(d) = &dir {
                std::fs::write(d.join("fonts.json"), serde_json::to_vec_pretty(&list)?)?;
            }
            out.print(&json!({"ok": true, "archive": a.id, "movies": movies.len(), "count": list.len(), "fonts": list}));
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
        Cmd::PedVariations { ids, archive } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let tu = sixng_core::texusers::TexUsers::load(&sixng_core::texusers::path(&ws))?;
            let pre = sixng_core::wardrobe::prefixes(ix.entries.iter().filter(|e| e.kind == Kind::RageObj).filter_map(|e| e.name.as_deref()));
            let mut res = serde_json::Map::new();
            for id in ids {
                let off = id.rsplit('@').next().unwrap_or(id).trim_start_matches("0x");
                let off = u64::from_str_radix(off, 16).map_err(|_| anyhow::anyhow!("bad offset {id}"))?;
                let e = ix.entries.iter().find(|e| e.off == off).ok_or_else(|| anyhow::anyhow!("no entry at {off:x}"))?;
                let r = sixng_core::resource::Resource::load_meta(&a.path, e)?;
                let m = sixng_core::meta7a::Meta::parse(&r).ok_or_else(|| anyhow::anyhow!("not a meta resource"))?;
                let vs = sixng_core::wardrobe::variations(&m, &pre, &tu, &a.id);
                let vs: Vec<_> = vs
                    .iter()
                    .map(|v| {
                        let mut d: Vec<&str> = v.parts.iter().filter_map(|p| p.drawable.as_deref()).collect();
                        d.dedup();
                        let remap: Vec<String> = v.parts.iter().flat_map(|p| p.tex_remap.iter().map(|(a, b)| format!("{a}={b}"))).collect();
                        json!({"id": v.id, "scale": v.scale, "tags": v.tags, "parts": d.iter().map(|n| format!("^{n}$")).collect::<Vec<_>>().join(","), "tex_remap": remap.join(","), "items": v.parts})
                    })
                    .collect();
                res.insert(e.id.clone(), json!(vs));
            }
            out.print(&serde_json::Value::Object(res));
        }
        Cmd::ScriptDocs { archive, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let sel: Vec<_> = ix.entries.iter().filter(|e| e.kind == Kind::Pso).collect();
            let t = Instant::now();
            let (ok, bad) = sixng_core::scriptdoc::export(&a, &sel, po)?;
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "documents": ok, "other": bad, "seconds": t.elapsed().as_secs_f64(), "out": po.join("script_docs")}));
        }
        Cmd::AudioMeta { archive, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let sel: Vec<_> = ix.entries.iter().filter(|e| e.kind == Kind::Bin).collect();
            let t = Instant::now();
            // The archive's audio index, if scanned, links sounds to streams.
            let banks: Vec<(u64, std::collections::HashSet<u32>)> = sixng_core::audio::load_index(&ws.work, &a.id).map(|ix| ix.banks.iter().map(|b| (b.off, b.streams.iter().map(|s| s.id).collect())).collect()).unwrap_or_default();
            let (ok, bad) = sixng_core::audiometa::export(&a, &sel, &banks, po)?;
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "decoded": ok, "failed": bad, "linked_to_banks": !banks.is_empty(), "seconds": t.elapsed().as_secs_f64(), "out": po.join("audio_meta")}));
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
