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

mod bench;
mod scene_dialogue;

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
    #[command(flatten)]
    bench: bench::BenchArgs,
    #[command(subcommand)]
    cmd: Option<Cmd>,
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
    /// Re-export items whose outputs already exist, shared textures included.
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
            tex_remap: Default::default(), ped_tints: Default::default(), part_skins: Default::default(), part_decorations: Default::default(), skin_tone: 0, part_colours: Default::default(), part_hidden: Default::default(), part_layers: Default::default(),
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
    /// Read an archive's table of contents: columns, file and resource entries; archives stored inside it are followed. Windows x64 only; needs the game executable in the game folder. No index needed.
    Toc {
        archive: String,
        /// List every file entry (offset, sizes, flags, key id).
        #[arg(long)]
        files: bool,
    },
    /// Extract the files the carving index cannot read (a few hundred metadata files), using the table of contents: <out>/toc_files/<archive>[@offset]/<offset>.<ext> + index.jsonl. Each file is checked against its zlib checksums and the size in the table. Same requirements as `toc`. Without archives, all of them.
    TocExtract {
        archives: Vec<String>,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
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
    /// Strand hair grooms (hair, beards, brows, fur: unnamed packs.rpf resources) as GLB line strips or ribbons in model space: <out>/grooms/<archive>/<offset>.glb and index.jsonl (strands, points, bones, region, LOD chain). Pick them by region and bones, or an animal's fur with --animal.
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
        /// Write the full strand set instead of the guide strands alone:
        /// the strands the groom stores per root triangle, grown between
        /// its three guides (the game's own clumping, curl and noise are
        /// not decoded). Also writes <offset>.growth.json with the root
        /// triangles, their strand counts, the strand width and its taper,
        /// to rebuild the hair elsewhere. Grooms that store no counts
        /// keep their guides.
        #[arg(long)]
        full: bool,
        /// With --full: share of the stored strands to write, 0 to 1.
        #[arg(long, default_value_t = 1.0)]
        density: f32,
        /// Export the fur of this animal (its root name, e.g.
        /// crm_cat_feral_m_00): the grooms of its default body and head
        /// fur, in the animal's model space, so they sit on the model that
        /// `outfit <root>` exports. The link is read from the game (the
        /// variants' placeholder name ids, the piece table, the file ids of
        /// the table of contents), or inferred from archive order, piece
        /// counts and distance to the skin where the table of contents
        /// cannot be read; it is written to <root>.fur.json.
        #[arg(long)]
        animal: Option<String>,
        /// Only these groom offsets (hex, comma-separated).
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// With --full: the model GLB the grooms sit on (`outfit` writes
        /// one; or a head drawable exported on its own). Each grown groom's
        /// mesh then names the head UV set (TEXCOORD_n) its root UVs are in
        /// (head_uv_set), for scripts/blender_hair_curves.py --model.
        #[arg(long)]
        head: Option<PathBuf>,
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
        /// Search the strings with a regex (case-insensitive, matched on the text without its inline markup) and print the hits: label hash, text, and how many tables hold it.
        #[arg(long)]
        find: Option<String>,
        /// Most hits --find prints.
        #[arg(long, default_value_t = 200)]
        limit: usize,
        /// Strip the inline markup (~sl:…~, ~b~, ~n~ …) from printed strings.
        #[arg(long)]
        plain: bool,
        /// Output directory (needed unless --label or --find is given).
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
    /// Light emitters from archetype/entity light extensions, placed in world space, plus the map files' distant / LOD lights and timecycle modifier regions: <out>/lights/<archive>/.
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
    /// Decode RBF binary XML files (RBF0: UI layouts, subtitles, GPS and gamepad configs) to XML text: <out>/rbf/<archive>/<offset>.xml plus index.jsonl (per file its stats, or why it did not decode). Tag and attribute names are stored as text in the files, so all of them are real names; value elements are written as <name value="..."/>.
    Rbf {
        /// Archive (default data).
        #[arg(default_value = "data")]
        archive: String,
        #[command(flatten)]
        filter: FilterArgs,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Layered-material libraries (data.rpf PSO files with root struct 8419ab01): the base materials and layered presets behind the clothing and weapon shaders, which bind no colour texture. A wardrobe item's GarmentPreset and a weapon component record name a preset; a preset gives each of its 8 sub-materials a colour. Builds <work>/index/material_presets.json (per preset the four colour entries of each sub-material as rrggbbaa sRGB, entry 0 = base colour; weapon component and variant name hashes -> preset, from the weapon family files of packs.rpf when indexed), which `ped-variations` reads. With --preset, prints those presets in full (every parameter of the preset and of its base materials; parameter names are hashes). With -o, also writes each library as <out>/materials/<archive>/<offset>.json.
    Materials {
        /// Archive with the libraries (default data).
        #[arg(default_value = "data")]
        archive: String,
        /// Presets to print in full (name hash, hex).
        #[arg(long)]
        preset: Vec<String>,
        /// Weapon models (regex on the drawable name, e.g. ^w_ar_adb_01) to list with their presets and sub-material colours.
        #[arg(long)]
        weapon: Vec<String>,
        /// Output directory for the full libraries.
        #[arg(long, short)]
        out: Option<PathBuf>,
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
    /// A whole vehicle as ONE GLB: its model with the parts of one of its
    /// configurations fitted (truck bed or box, bumpers, light bars, the
    /// stock alternatives the fitted parts choose), in the configuration's
    /// part colours and paint colour, with rims and tyres on the wheel
    /// bones. Skinned to the vehicle's skeleton. Needs levels, packs and
    /// data indexed. With --list, prints the vehicle's configurations
    /// instead.
    Vehicle {
        /// Vehicle model name (benson, comet7).
        name: String,
        /// Output .glb path (textures go beside it).
        #[arg(long, short)]
        out: Option<PathBuf>,
        /// Which configuration (0 = the first; see --list).
        #[arg(long, default_value_t = 0)]
        config: usize,
        /// Leave the wheels out.
        #[arg(long)]
        no_wheels: bool,
        /// Print the vehicle's configurations (colour, part count) and stop.
        #[arg(long)]
        list: bool,
        /// Print the vehicle's parts that have models (part name, slot,
        /// models, how many configurations name it) and stop.
        #[arg(long)]
        list_parts: bool,
        /// Parts to fit on top of the configuration's, by part name (8 hex
        /// digits from --list-parts, comma separated). A part replaces the
        /// configuration's part in the same slot.
        #[arg(long, value_delimiter = ',')]
        with: Vec<String>,
        /// Parts of the configuration to leave off (8 hex digits, comma
        /// separated).
        #[arg(long, value_delimiter = ',')]
        without: Vec<String>,
        /// A vehicles.json from `sixng vehicles` (default: built now, into
        /// the output folder).
        #[arg(long)]
        data: Option<PathBuf>,
        /// Put a livery on it: a livery file saved by the viewer's Liveries
        /// panel. Without a file name, the one the viewer keeps for this
        /// vehicle (<work>/liveries/<vehicle>.json). Each sticker becomes a
        /// decal mesh with its picture beside the GLB.
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        livery: Option<PathBuf>,
        /// Archive with the vehicle models.
        #[arg(long, default_value = "levels")]
        archive: String,
        /// Texture output: png, dds, or both.
        #[arg(long, default_value = "png")]
        tex: String,
    },
    Vehicles {
        /// Archive (default packs).
        #[arg(default_value = "packs")]
        archive: String,
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Vehicle sticker records (the logos and lettering liveries are made
    /// of): <out>/vehicles/stickers.json, and with --textures every sticker
    /// texture as <out>/vehicles/stickers/<64-bit id>.png. With --id, only
    /// that texture is written (to <out>/vehicles/stickers).
    Stickers {
        /// Output directory.
        #[arg(long, short)]
        out: PathBuf,
        /// Also decode the sticker textures to PNG.
        #[arg(long)]
        textures: bool,
        /// Only stickers whose name, name hash or texture id matches (regex).
        #[arg(long)]
        name: Option<String>,
        /// One texture by its 64-bit id (hex); nothing else is written.
        #[arg(long)]
        id: Option<String>,
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
    /// Timecycle, weather and cloud data (the plain XML files of common.rpf):
    /// every timecycle modifier (name, attributes, submodifiers, each
    /// variable's two numbers and the keyframes of animated ones), every
    /// weather cycle (per region, one number per time sample), the time
    /// samples, the weather types and the custom cloud groups. The archive
    /// has no file names; modifier files are named from the game's own file
    /// list by packing order and content (`fit`). Prints the summary and the
    /// modifiers matching --name (default: the arcade minigame set, i.e.
    /// TC_mods_Arcade_Games and every arcade modifier, with the
    /// submodifiers they use). With -o writes <out>/timecycle/: modifiers,
    /// cycles, time_samples, weather, custom_clouds, variables, files and
    /// selected (.json).
    /// Time of day and weather lighting from the timecycle (common.rpf):
    /// per weather type and hour, the resolved timecycle values the viewer
    /// uses (directional colour and multiplier, ambient intensity and tint,
    /// exposure) and the light derived from them. Uses the cached table
    /// (<work>/cache/daylight.json) or decodes common.rpf; --rebuild
    /// decodes again.
    Daylight {
        /// Weather ids (sunny, clear, clouds, overcast, overcastdark, misty,
        /// fog, drizzle, shower, rain, thunderstorm, hurricane); default all.
        #[arg(long, value_delimiter = ',')]
        weather: Vec<String>,
        /// Hours (0-24), default the 13 time samples.
        #[arg(long, value_delimiter = ',')]
        hour: Vec<f32>,
        /// Also print these timecycle variables.
        #[arg(long, value_delimiter = ',')]
        var: Vec<String>,
        #[arg(long)]
        rebuild: bool,
    },
    Timecycle {
        /// Archive (default common).
        #[arg(default_value = "common")]
        archive: String,
        /// Regex on modifier name or file name (case-insensitive).
        #[arg(long)]
        name: Option<String>,
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// Whole-world terrain height map (the MHIU blobs of common.rpf:
    /// 2048 x 2048 u8 heights over the world box, row-compressed). The
    /// archive has two copies; `--map` picks one (default: the one with
    /// the most stored samples). Prints the header of each. With -o writes
    /// <out>/heightmap/: heightmap.png (16-bit greyscale, north up,
    /// 0..65535 = min z..max z), heightmap.json (world box, metres per
    /// pixel, height scale, fill/sea value) and, with --preview,
    /// hillshade.png. With --check <map dir> compares the heights with the
    /// collision top-surface grid of the map bake (heights_8m.bin, meta.json).
    Heightmap {
        /// Archive (default common).
        #[arg(default_value = "common")]
        archive: String,
        /// Which copy to write (0 or 1, in archive order).
        #[arg(long)]
        map: Option<usize>,
        /// Also write a hill-shaded RGB preview.
        #[arg(long)]
        preview: bool,
        /// Map bake folder (<work>/map) to compare against.
        #[arg(long)]
        check: Option<PathBuf>,
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// Water system export: the water fog atlas (scattering and extinction,
    /// textures.rpf), the water height map and the shoreline wave grid
    /// (levels.rpf), each placed on its 32 x 66 grid of 512 m cells; the
    /// grid definitions, ocean and shore-wave settings of common.rpf as
    /// JSON; and every water_* timecycle key. Tiles are placed by aligning
    /// their stored order with the decimal-string order of the cell numbers
    /// against each grid's fallback texture, then checked by edge
    /// continuity. Prints the counts and checks. With -o writes
    /// <out>/water/: grids.json, timecycle_water.json, settings/*.json,
    /// fog/ (scattering.f32, extinction.f32, cells.json with the terms per
    /// cell, previews), height/ (height.png, cells.json) and shore/
    /// (tiles/<cell>.png, mosaic.png, cells.json). Needs the common,
    /// textures and levels indexes.
    Water {
        /// Fog mosaic texels per cell (tiles have 128).
        #[arg(long, default_value_t = 32)]
        fog_px: usize,
        /// Height mosaic texels per cell (tiles have 512, 1 m each).
        #[arg(long, default_value_t = 128)]
        height_px: usize,
        /// Shore mosaic texels per cell (tiles have 256).
        #[arg(long, default_value_t = 32)]
        shore_px: usize,
        /// Also write each height tile at full resolution (height/tiles/).
        #[arg(long)]
        height_tiles: bool,
        /// Also export the inland water surfaces (rivers, canals, creeks,
        /// reservoirs, ponds) placed in world space: inland/inland.json and
        /// inland/inland.glb. Scans every map file of levels.rpf.
        #[arg(long)]
        inland: bool,
        /// Also export pools, hot tubs, fountains, waterfalls, standing water
        /// and other small water bodies placed in world space: pools/pools.json
        /// and pools/pools.glb. Scans every map file of levels.rpf.
        #[arg(long)]
        pools: bool,
        /// Also export the water textures the game binds at run time (bump,
        /// foam, caustics, shore wave profiles, ocean control maps):
        /// detail/*.png, detail/*.dds and detail/detail.json.
        #[arg(long)]
        detail: bool,
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// World sector grids (the PSO files of levels.rpf with struct
    /// 9c834886). levels.rpf holds eight, in three cell sizes (1024, 512
    /// and 60 m, measured; cell = floor(world / size)). Each size has a
    /// cell list and a map-group list (map group id = the map files'
    /// 65876a4d -> the cells its entities reach). The world grid is the
    /// 60 m cell list, 26,212 cells; it is used unless --entry picks
    /// another. Records also carry a bundle key and the index in the
    /// bundle. Prints a summary of each grid (kind, cell size, counts, cell
    /// range, world box). With -o writes <out>/sector_grid/:
    /// sector_grid.csv (one line per record with its world box),
    /// sector_grid.json (summary and records) and sector_grid.png (mask,
    /// one pixel per cell, north up).
    SectorGrid {
        /// Archive (default levels).
        #[arg(default_value = "levels")]
        archive: String,
        /// Entry id of the grid to write (default: the one with most records).
        #[arg(long)]
        entry: Option<String>,
        /// Cell size in metres for the written grid (default: the measured
        /// size of that file).
        #[arg(long)]
        cell_m: Option<f32>,
        #[arg(long, short)]
        out: Option<PathBuf>,
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
    /// Ped wardrobe variations (packs.rpf meta resources with struct f795e8ba, one per ped model): per variation its parts (slot, name, drawable), as JSON. Drawables are found through the item's textures, so run `texture-users` first. A variation is a clothing set or a head set (head, eyes, teeth); a full look is one of each. `parts`, `tex_remap` and `tints` per variation are ready for `outfit <name> --parts … --tex-remap … --tint …` (the remap carries the item textures that differ from the drawable's own; `tints` the clothing colour each part's item stores: tint palette and rows, also per item as `tint`; parts whose items store none are left out). `listed` is the model's variation list entry (model hash, outfit type, body scale): the clothing sets the game picks from; head sets are not listed and pair with any of them. `dirt` holds the variation's dirt or blood preset from common.rpf (colour, dirt-mask channels, cloth and skin strengths), which the game applies at run time and the export does not.
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
    /// Audio metadata files (the sound, game-object, curve, speech, music and mixer definitions beside a pack's banks: plain files with 0x045a88ea at +8): per file its schema id, every object's name hash, offset, size and class id (first byte), as <out>/audio_meta/<archive>/<offset>.json + index.jsonl. Sound files (schema 166) also get each simple sound's wave (container hash, stream id and, when the archive's audio index exists, its bank), the child sounds of randomized, sequential, wrapper and twenty other sound classes, and every sound's reachable (bank, stream) pairs. Names are hashes; other class fields are not decoded.
    AudioMeta {
        archive: String,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Audio banks (ADAT) in an archive: scan once (index in <work>/index/audio_<archive>.json), then list banks and streams. Stream ids are hashes. A bank shows its container (the bank name hash, from the sound files or the bank list; run `audio-meta` once to record them) and the container's name where the name dictionary has one (`sixng names`); a stream shows its index in the bank and, where it has a name, the name, where it came from (`stored`: the bank's own name tag; `search`: the name search) and its full joaat (`hash`).
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
    /// What each compiled shader program and each effect (a material's
    /// shader) of shaders.rpf declares: resources (textures, samplers,
    /// constant buffers and others) with their register index, and the
    /// members of the constant buffers with their byte offsets, by name
    /// hash (names where the dictionary has them). Prints a summary and the
    /// material texture parameters that have no name; with an effect regex,
    /// the textures (slot in the material's texture list, parameter and
    /// shader-variable name), constants and programs of each matching
    /// effect, and its techniques with the programs of each pass. Reads the
    /// game's own reflection tables; no shader code is
    /// decoded. With --cache, the pipeline cache says which declared
    /// resources each compiled program really reads.
    ShaderBindings {
        /// Regex on effect names (a material's shader name, e.g. `^standard$`).
        effect: Option<String>,
        /// Also list every program of the matching effects with its resources and constant buffers.
        #[arg(long)]
        programs: bool,
        /// Also read the pipeline cache (the archive's compressed entry; shaders.rpf must be indexed) and join it to the programs: how many pipelines use each program, and which of its declared resources the compiled code reads (`used`).
        #[arg(long)]
        cache: bool,
    },
    /// Name dictionary for shader, parameter and meta field hashes (joaat of
    /// the lower-case name). Without --lookup: build `<work>/names/names.json`
    /// from the install (common.rpf text/XML, shaders.rpf strings, audio.rpf's
    /// config and metadata names, and the virtual file-system mount paths
    /// such as `platform:/levels/…/x.rpf`). Audio bank names
    /// (`core.resident/gun_pistol`) are added when `audio-meta` has recorded
    /// the banks' container hashes (it adds them itself when run later).
    Names {
        /// Hashes (hex) to look up instead of building.
        #[arg(long, value_delimiter = ',')]
        lookup: Vec<String>,
        /// Import a gta6-namehash dictionary.json (public GTA V names) into
        /// `<work>/names/namehash.json` instead of building. Its names are
        /// used only for hashes no other source names.
        #[arg(long)]
        namehash: Option<PathBuf>,
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
        /// Leave the clip's own movement over the ground (its mover
        /// tracks) out of the animation, so it plays in place.
        #[arg(long)]
        in_place: bool,
        /// Texture output: png, dds, or both.
        #[arg(long, default_value = "png")]
        tex: String,
        /// Body shape, as for extract (e.g. "gcweightbelly=0.5"). Without it a
        /// --look outfit is posed in its own body-shape preset (from
        /// common.rpf's body-type XML; "gcweightback=0" for neutral).
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
        /// Clothing colours, `<drawable>=<palette hex>:<row0>.<row1>.<row2>`,
        /// comma separated, as `ped-variations` lists them (`tints`): the
        /// part's shade map is coloured through that tint palette row
        /// (tint 0). Parts without one are shaded grey.
        #[arg(long, value_delimiter = ',')]
        tint: Vec<String>,
        /// Use outfit N (0 = the first) of the game's outfit list for this
        /// model instead of its own parts: clothes, skin meshes, head, eyes
        /// and teeth (`ped-look` lists the outfits). The head gets the face
        /// its outfit item names, and parts with a stored tint their tint.
        /// Stored flat colours, colour tags, the body skin tone and strand
        /// hair are viewer features and are not in the export (see --hair).
        /// A model with no parts of its own (`cs_mandy`) uses outfit 0 when
        /// neither this nor --parts is given.
        #[arg(long)]
        look: Option<usize>,
        /// Also write the character's strand hair to <out>.hair.glb, in the
        /// same space as the model: per piece (hair, brows, lashes, beard
        /// of the outfit --look picks, or the first) the full strand set
        /// the game grows from the guide strands, as line strips with the
        /// root UV (TEXCOORD_0), width (_WIDTH, metres), clump (_CLUMP, 0 =
        /// none) and colour variant (_VARIANT) on every point, and the
        /// guides as <piece>_guides. The root UVs are coordinates in one of
        /// the head mesh's UV sets; each piece's extras name it
        /// (head_uv_set, matched against the exported head). scripts/blender_hair_curves.py makes
        /// Blender hair curves of it.
        #[arg(long)]
        hair: bool,
        /// With --hair: share of the stored strands to write, 0 to 1.
        #[arg(long, default_value_t = 1.0)]
        hair_density: f32,
        /// With --hair: at most this many strands per piece (0 = no cap).
        #[arg(long, default_value_t = 0)]
        hair_max: usize,
        /// With --hair: points per strand, resampled along its length (0 =
        /// as stored; fewer points make a smaller file).
        #[arg(long, default_value_t = 0)]
        hair_points: usize,
        /// With --hair: the outfit's strand slots to write (hair, brows,
        /// lashes, beard, and the body-hair slots such as chesthairfull).
        /// `all` for every one.
        #[arg(long, value_delimiter = ',', default_value = "hair,brows,lashes,beard")]
        hair_slots: Vec<String>,
        /// With --hair: also the grooms placed under hair placeholders
        /// matching this regex (another hair style or beard of the
        /// character, e.g. `cs_jason_beardstubble_.*_l04`).
        #[arg(long)]
        hair_style: Vec<String>,
        /// With --hair: leave out the guide strands.
        #[arg(long)]
        hair_no_guides: bool,
    },
    /// The outfits of a character model, from the game's outfit list and the
    /// model's wardrobe resource (needs the `texture-users` index): per
    /// outfit its parts (slot, drawable, tint, hair asset), the body-shape
    /// preset and what did not resolve. The first outfit is the default look
    /// of the viewer.
    PedLook {
        /// Model name (`cs_ali`, `player_story000_00`), or a regex starting
        /// with `^` for one summary row per matching character or outfit-list model (outfit
        /// count, and whether the default outfit has a head, a body, clothes,
        /// shoes and hair).
        model: String,
        /// Archive holding the parts.
        #[arg(long, default_value = "packs")]
        archive: String,
    },
    /// Every part a character uses: each drawable of all its outfits and of
    /// the other variations of its wardrobe, grouped as body, hair,
    /// clothing and accessory, with each texture / colour variant (wardrobe
    /// item) and the outfits that wear it. Writes `<out>/<model>/parts.json`
    /// with --out; prints the set (or, for a regex, one summary row per
    /// character) as JSON.
    PedParts {
        /// Model name (`cs_mandy`), or a regex starting with `^` for every
        /// matching character: those with parts of their own and the named
        /// models of the outfit list.
        model: String,
        /// Folder for `<model>/parts.json`.
        #[arg(long, short)]
        out: Option<PathBuf>,
        /// Archive holding the parts.
        #[arg(long, default_value = "packs")]
        archive: String,
    },
    /// Characters and animals that `outfit` can assemble: named characters
    /// (outfit prefixes such as cs_ali, sc_slow_demon) and animal roots
    /// (crb_eagle_bald_m_00), with their part counts. JSON to stdout.
    Characters {
        /// Regex on the name.
        #[arg(long)]
        name: Option<String>,
        /// "character", "animal" or "model" (only the game's ped model list).
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
    /// Mission and arc names for the story scenes, from the story-flow tags
    /// in the story pack's protected rule files. Each tag that a mission id
    /// shortens (letters left out) names that mission; the tags are cached in
    /// <work>/index/story_flow_tags.json. Needs the cutscene catalog and, the
    /// first time, the game executable (Windows x64) for the table of contents.
    StoryNames {
        /// Print the flow tags as well.
        #[arg(long)]
        tags: bool,
    },
    /// Cutscenes of the story pack: per scene its cast (scene handles with
    /// the ped model each one is, and whether `outfit` can assemble it),
    /// props, cameras, the clip names of each object and the set-dressing
    /// models its scene file spawns. Read from the scene clip dictionaries
    /// (`cin@<scene>`) and the scene files (meta resources naming the
    /// scene); the first run reads the whole archive (several minutes) and
    /// caches the result in <work>/index/<archive>.cutscenes.json. Models
    /// resolve against the names of every indexed archive. A scene's clips
    /// play on a character with `outfit <character> --clip cin@<scene>`.
    Cutscenes {
        /// One scene in full (name, `cin@` optional).
        scene: Option<String>,
        /// Regex on scene names.
        #[arg(long)]
        name: Option<String>,
        /// Only scenes with this model or handle in the cast (regex on both).
        #[arg(long)]
        cast: Option<String>,
        /// Archive with the cutscenes.
        #[arg(long, default_value = sixng_core::cutscene::STORY_ARCHIVE)]
        archive: String,
        /// Scan the archive again instead of using the cached catalog.
        #[arg(long)]
        rescan: bool,
        /// Only the totals.
        #[arg(long)]
        summary: bool,
        /// At most this many scenes (0 = all).
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Build the Scenes dialogue index from verified local occurrences and GXT tables.
    SceneDialogue {
        /// Local JSON with verified occurrence records.
        #[arg(long)]
        input: PathBuf,
        /// A JSON table from `sixng text --out`. Repeat for tables in one locale.
        #[arg(long, required = true)]
        text_table: Vec<PathBuf>,
        /// Output file (default <work>/scene_dialogue.json).
        #[arg(long)]
        out: Option<PathBuf>,
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
        /// For float-format textures (lookup tables), also write the unclamped values as <hash>_m<mip>.f32: little-endian 32-bit floats, every slice of the mip in order, rows top to bottom, channels interleaved. The PNG clamps to 0..1 and keeps 8 bits. The result gives width, height, slices and channels.
        #[arg(long)]
        float: bool,
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
    if cli.bench.bench {
        out.print(&bench::run(&ws, &cli.bench)?);
        return Ok(());
    }
    let Some(cmd) = &cli.cmd else { bail!("no command given: see `sixng --help`") };
    match cmd {
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
                let (ix, how) = ws.ensure_index_report(&a, *force, !*no_names, pa)?;
                let named = ix.entries.iter().filter(|e| e.name.is_some()).count();
                if !cli.quiet {
                    eprintln!("{}: {}", a.id, how.summary());
                }
                res.push(json!({"archive": a.id, "status": how.status, "summary": how.summary(), "outcome": how,
                    "seconds": t.elapsed().as_secs_f64(), "stats": ix.stats, "named_entries": named}));
            }
            out.print(&json!({"ok": true, "indexed": res}));
        }
        Cmd::Toc { archive, files } => {
            let a = ws.resolve(archive)?;
            let mut cipher = sixng_core::gamecipher::open(&game)?;
            let mut f = std::fs::File::open(&a.path)?;
            let mut list = Vec::new();
            toc_walk(cipher.as_mut(), &mut f, 0, &mut |_, _, base, toc, entries| {
                let enc: Vec<_> = entries.iter().filter(|e| e.crypto.is_some()).collect();
                let mut v = json!({
                    "base": base, "rows": toc.rows, "index_width": toc.index_width, "selector": toc.selector,
                    "columns": toc.columns, "known_entry_column": toc.has_entries(),
                    "files": entries.len(), "resources": entries.iter().filter(|e| e.resource.is_some()).count(), "protected": enc.len(),
                    "protected_stored_bytes": enc.iter().map(|e| e.stored).sum::<u64>(),
                });
                if *files {
                    v["entries"] = entries.iter().map(|e| json!({
                        "row": e.row, "offset": e.offset, "stored": e.stored, "unpacked": e.unpacked, "flags": e.flags,
                        "resource_type": e.resource.map(|r| r.1),
                        "key_id": e.crypto.as_ref().map(|c| format!("{:08x}", c.key_id)),
                        "pattern": e.crypto.as_ref().map(|c| c.pattern),
                    })).collect();
                }
                list.push(v);
                Ok(())
            })?;
            out.print(&json!({"ok": true, "archive": a.id, "tables": list}));
        }
        Cmd::TocExtract { archives, out: po } => {
            let list = if archives.is_empty() {
                ws.archives().into_iter().filter(|a| a.container == "rpf9").collect::<Vec<_>>()
            } else {
                archives.iter().map(|q| ws.resolve(q)).collect::<Result<Vec<_>>>()?
            };
            let mut cipher = sixng_core::gamecipher::open(&game)?;
            let t = Instant::now();
            let mut res = Vec::new();
            let (mut total, mut failed) = (0usize, 0usize);
            for (n, a) in list.iter().enumerate() {
                pr(&format!("toc-extract:{}", a.id), n as u64, list.len() as u64);
                let mut f = std::fs::File::open(&a.path)?;
                let stem = sixng_core::game::index_stem(&a.id);
                toc_walk(cipher.as_mut(), &mut f, 0, &mut |cipher, f, base, _, entries| {
                    let enc: Vec<_> = entries.iter().filter(|e| e.crypto.is_some()).collect();
                    if enc.is_empty() {
                        return Ok(());
                    }
                    let name = if base == 0 { stem.clone() } else { format!("{stem}@{base:x}") };
                    let dir = po.join("toc_files").join(&name);
                    std::fs::create_dir_all(&dir)?;
                    let mut index = sixng_core::atomic::AtomicFile::create(dir.join("index.jsonl"))?;
                    let mut errors = Vec::new();
                    let mut bytes = 0u64;
                    for e in &enc {
                        let c = e.crypto.as_ref().unwrap();
                        let mut rec = json!({"row": e.row, "offset": e.offset, "stored": e.stored, "unpacked": e.unpacked,
                            "flags": e.flags, "resource_type": e.resource.map(|r| r.1), "key_id": format!("{:08x}", c.key_id)});
                        match sixng_core::rpf9toc::read_file(cipher, f, base, e) {
                            Ok((data, check)) => {
                                let ext = if e.resource.is_some() { "rsc" } else { sixng_core::rpf9toc::sniff_ext(&data) };
                                let file = format!("{:09x}.{ext}", e.offset);
                                sixng_core::atomic::write(dir.join(&file), &data)?;
                                bytes += data.len() as u64;
                                rec["file"] = json!(file);
                                rec["check"] = json!(check);
                            }
                            Err(err) => {
                                rec["error"] = json!(format!("{err:#}"));
                                errors.push(json!({"offset": e.offset, "error": format!("{err:#}")}));
                            }
                        }
                        writeln!(index, "{rec}")?;
                    }
                    index.commit()?;
                    total += enc.len();
                    failed += errors.len();
                    res.push(json!({"archive": a.id, "base": base, "dir": dir, "files": enc.len() - errors.len(), "bytes": bytes, "errors": errors}));
                    Ok(())
                })?;
            }
            pr("toc-extract", list.len() as u64, list.len() as u64);
            out.print(&json!({"ok": failed == 0, "files": total - failed, "failed": failed, "seconds": t.elapsed().as_secs_f64(), "archives": res}));
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
        Cmd::ShaderBindings { effect, programs, cache } => {
            use sixng_core::names::{lookup, Ctx};
            use sixng_core::shaderfx::{self, Table, Var};
            let t = Instant::now();
            let bytes = std::fs::read(ws.game.join("shaders.rpf"))?;
            let sh = shaderfx::read(&bytes)?;
            // the pipeline cache: the archive's entry that starts with its magic
            let joined = if *cache {
                let a = ws.resolve("shaders")?;
                let ix = ws.load_index(&a)?;
                let e = ix.entries.iter().find(|e| e.sub == "09005350").ok_or_else(|| anyhow::anyhow!("no pipeline cache entry in the shaders.rpf index"))?;
                let c = shaderfx::read_cache(&sixng_core::scan::read_entry(&a.path, e)?)?;
                let j = sh.compiled(&c, &bytes);
                Some((c, j))
            } else {
                None
            };
            drop(bytes);
            let cache_summary = joined.as_ref().map(|(c, j)| {
                let used: Vec<_> = j.iter().flatten().collect();
                json!({"version": c.version, "graphics_pipelines": c.graphics, "compute_pipelines": c.compute, "metadata_blobs": c.metadata_blobs, "code_blobs": c.code_blobs, "stage_blobs": c.stages.len(),
                    "programs_in_a_pipeline": used.len(), "programs_in_no_pipeline": j.len() - used.len(),
                    "read_records_matched": used.iter().map(|u| u.read.iter().filter(|x| **x).count()).sum::<usize>(), "read_records_unmatched": used.iter().map(|u| u.unmatched_reads).sum::<usize>(),
                    "declared_resources_never_read": used.iter().map(|u| u.read.iter().filter(|x| !**x).count()).sum::<usize>(), "write_records": used.iter().map(|u| u.writes.len()).sum::<usize>(),
                    "note": "counted per program over the stage blob of its first pipeline; read_records_matched counts declared resources (samplers included in declared_resources_never_read: the cache has no sampler records)"})
            });
            let named = |h: u32| json!({"hash": format!("{h:08x}"), "name": lookup(h, Ctx::Param)});
            let member = |m: &Var| json!({"hash": format!("{:08x}", m.name), "name": lookup(m.name, Ctx::Param), "parameter": (m.alias != m.name && m.alias != 0).then(|| named(m.alias)), "type": m.member_type(), "offset": m.a});
            let resource = |t: &Table, i: usize, r: &Var| {
                let mut v = json!({"hash": format!("{:08x}", r.name), "name": t.sampler_name(i).map(str::to_string).or_else(|| lookup(r.name, Ctx::Param)), "kind": r.resource_kind(), "dimension": r.dim, "index": r.index, "flags": format!("{:02x}", r.flags())});
                if r.is_cbuffer() {
                    v["size"] = json!(r.c & 0xffff);
                    v["members"] = t.members_of(i).iter().map(member).collect();
                }
                v
            };
            // material texture parameters over all effects: parameter hash -> (variable hash, array) -> effects
            use std::collections::BTreeMap;
            let mut tex: BTreeMap<u32, BTreeMap<(u32, bool), Vec<&str>>> = BTreeMap::new();
            for e in &sh.effects {
                for r in e.table.iter().flat_map(|t| &t.resources) {
                    if let Some((_, array)) = r.material_texture() {
                        tex.entry(r.alias).or_default().entry((r.name, array)).or_default().push(&e.name);
                    }
                }
            }
            let unnamed: Vec<_> = tex
                .iter()
                .filter(|(h, _)| lookup(**h, Ctx::Param).is_none())
                .map(|(h, v)| json!({"parameter": format!("{h:08x}"), "effects": v.values().map(Vec::len).sum::<usize>(), "example_effects": v.values().flatten().take(3).collect::<Vec<_>>(),
                    "variables": v.iter().map(|((n, array), es)| json!({"hash": format!("{n:08x}"), "name": lookup(*n, Ctx::Param), "array": array, "effects": es.len()})).collect::<Vec<_>>()}))
                .collect();
            let re = effect.as_deref().map(regex::Regex::new).transpose()?;
            let stages = ["vertex_or_mesh", "pixel", "geometry", "section_3", "section_4", "compute"];
            let effects: Vec<_> = sh
                .effects
                .iter()
                .filter(|e| re.as_ref().is_some_and(|r| r.is_match(&e.name)))
                .map(|e| {
                    let progs = |full: bool| -> serde_json::Value {
                        stages
                            .iter()
                            .enumerate()
                            .filter(|(k, _)| !e.programs[*k].is_empty())
                            .map(|(k, st)| {
                                let list: Vec<_> = e.programs[k]
                                    .iter()
                                    .filter_map(|&n| sh.program(e, k, n))
                                    .map(|(pi, p)| {
                                        if !full {
                                            return json!(p.name);
                                        }
                                        let comp = joined.as_ref().and_then(|(_, j)| j.get(pi)).and_then(Option::as_ref);
                                        let res: Vec<_> = p.table.resources.iter().enumerate().map(|(i, r)| {
                                            let mut v = resource(&p.table, i, r);
                                            if let Some(c) = comp.filter(|_| r.kind != 0x23) {
                                                v["used"] = json!(c.read[i]);
                                            }
                                            v
                                        }).collect();
                                        let mut v = json!({"name": p.name, "id": format!("{:016x}", p.id), "resources": res});
                                        if joined.is_some() {
                                            v["compiled"] = match comp {
                                                Some(c) => json!({"pipelines": c.pipelines, "unmatched_reads": c.unmatched_reads, "writes": c.writes}),
                                                None => json!(null),
                                            };
                                        }
                                        v
                                    })
                                    .collect();
                                (st.to_string(), json!(list))
                            })
                            .collect::<serde_json::Map<_, _>>()
                            .into()
                    };
                    let mut v = json!({"effect": e.name, "programs": progs(*programs)});
                    // techniques -> passes -> the program of each stage, by name
                    v["techniques"] = e
                        .techniques
                        .iter()
                        .map(|t| {
                            let passes: Vec<_> = t
                                .passes
                                .iter()
                                .map(|ps| {
                                    let mut o = serde_json::Map::new();
                                    o.insert("pass".into(), json!(ps.name));
                                    for (k, st) in stages.iter().enumerate() {
                                        if let Some(p) = ps.programs[k].and_then(|i| e.programs[k].get(i as usize)).and_then(|&n| sh.program(e, k, n)).map(|x| x.1) {
                                            o.insert(st.to_string(), json!(p.name));
                                        }
                                    }
                                    o.insert("flags".into(), json!(ps.flags.iter().map(|b| format!("{b:02x}")).collect::<String>()));
                                    // the state table items the pass sets, as indices into the effect's `states`
                                    for (key, v) in [("rasterizer", ps.rasterizer()), ("depth_stencil", ps.depth_stencil()), ("blend", ps.blend()), ("stencil_ref", ps.stencil_ref())] {
                                        if let Some(v) = v {
                                            o.insert(key.into(), json!(v));
                                        }
                                    }
                                    // what the pass binds, by name hash
                                    if let Some(b) = &ps.bindings {
                                        let l = |x: &[u32]| x.iter().map(|&h| named(h)).collect::<Vec<_>>();
                                        o.insert("bindings".into(), json!({"read": l(&b.read), "written": l(&b.written), "third": l(&b.third), "constant_buffers": l(&b.cbuffers), "word": b.word.map(|w| format!("{w:08x}"))}));
                                    }
                                    o.insert("blocks".into(), json!(ps.blocks()));
                                    o.insert("tail".into(), json!(ps.tail.iter().map(|b| format!("{b:02x}")).collect::<String>()));
                                    serde_json::Value::Object(o)
                                })
                                .collect();
                            json!({"technique": t.name, "passes": passes})
                        })
                        .collect();
                    let hex = |items: &[Vec<u8>]| items.iter().map(|x| x.iter().map(|b| format!("{b:02x}")).collect::<String>()).collect::<Vec<_>>();
                    v["database"] = json!(e.database);
                    v["states"] = json!({"rasterizer": hex(&e.states.rasterizer), "depth_stencil": hex(&e.states.depth_stencil), "blend": hex(&e.states.blend), "samplers": hex(&e.states.samplers)});
                    // the state items read as Direct3D 11 state descriptions (one byte per field)
                    v["states_read"] = json!({
                        "rasterizer": e.states.rasterizer.iter().map(|x| shaderfx::rasterizer_state(x).map(|(fill, cull)| json!({"fill": fill, "cull": *["?", "none", "front", "back"].get(cull as usize).unwrap_or(&"?")}))).collect::<Vec<_>>(),
                        "depth_stencil": e.states.depth_stencil.iter().map(|x| shaderfx::depth_stencil_state(x).map(|d| {
                            let face = |f: [u8; 4]| json!({"fail": shaderfx::stencil_op(f[0]), "depth_fail": shaderfx::stencil_op(f[1]), "pass": shaderfx::stencil_op(f[2]), "func": shaderfx::comparison(f[3])});
                            json!({"depth": d.depth, "depth_write": d.depth_write, "depth_func": shaderfx::comparison(d.depth_func), "stencil": d.stencil, "stencil_read_mask": d.stencil_read_mask, "stencil_write_mask": d.stencil_write_mask, "front": face(d.faces[0]), "back": face(d.faces[1])})
                        })).collect::<Vec<_>>(),
                        "blend": e.states.blend.iter().map(|x| shaderfx::blend_state(x).map(|(a2c, independent, t)| {
                            // without independent blend only the first target applies
                            let targets: Vec<_> = t.iter().take(if independent { 8 } else { 1 }).map(|t| json!({"enable": t.enable, "src": shaderfx::blend_factor(t.src), "dst": shaderfx::blend_factor(t.dst), "op": shaderfx::blend_op(t.op),
                                "src_alpha": shaderfx::blend_factor(t.src_alpha), "dst_alpha": shaderfx::blend_factor(t.dst_alpha), "op_alpha": shaderfx::blend_op(t.op_alpha), "mask": t.mask})).collect();
                            json!({"alpha_to_coverage": a2c, "independent": independent, "targets": targets})
                        })).collect::<Vec<_>>(),
                        "samplers": e.states.samplers.iter().map(|x| shaderfx::sampler_state(x).map(|s| json!({"address": s.address, "lod_bias": s.lod_bias, "anisotropy": s.anisotropy, "comparison": shaderfx::comparison(s.comparison), "filter": format!("{:02x}", s.filter), "border": s.border, "min_lod": s.min_lod, "max_lod": s.max_lod}))).collect::<Vec<_>>(),
                    });
                    let layout = |l: Option<&shaderfx::Layout>| l.map(|l| json!({"id": format!("{:016x}", l.id), "header": l.header.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                        "entries": l.entries.iter().map(|x| json!({"hash": format!("{:08x}", x.name), "name": lookup(x.name, Ctx::Param), "kind": format!("{:02x}{:02x}", x.kind[0], x.kind[1]), "slot": x.slot})).collect::<Vec<_>>()}));
                    let db = &sh.databases[e.database];
                    v["blocks"] = e.blocks.iter().zip(&e.layouts).map(|(b, l)| match b {
                        shaderfx::Block::Inline(_, len) => json!({"bytes": len, "read": layout(l.as_ref())}),
                        shaderfx::Block::Shared(key) => json!({"shared": format!("{key:08x}"), "bytes": db.shared_block(*key).map(|i| db.shared_blocks[i].1), "read": layout(db.shared_block(*key).and_then(|i| db.shared_layouts[i].as_ref()))}),
                    }).collect();
                    v["second_table"] = json!(e.second.as_ref().map(|t| t.resources.iter().map(|r| named(r.name)).collect::<Vec<_>>()));
                    if let Some(t) = &e.table {
                        let mut textures: Vec<_> = t.resources.iter().filter_map(|r| Some((r.material_texture()?, r))).collect();
                        textures.sort_by_key(|x| x.0 .0);
                        v["textures"] = textures.iter().map(|((slot, array), r)| json!({"slot": slot, "array": array, "dimension": r.dim, "parameter": named(r.alias), "variable": named(r.name)})).collect();
                        v["constants"] = t.resources.iter().enumerate().filter(|(_, r)| r.is_cbuffer() && r.flags() & 0x80 != 0).map(|(i, r)| json!({"buffer": named(r.name), "size": r.c & 0xffff, "members": t.members_of(i).iter().map(member).collect::<Vec<_>>()})).collect();
                        v["samplers"] = (0..t.resources.len()).filter_map(|i| t.sampler_name(i)).collect();
                        v["other_resources"] = t.resources.iter().enumerate().filter(|(_, r)| r.material_texture().is_none() && r.kind != 0x23 && !(r.is_cbuffer() && r.flags() & 0x80 != 0)).map(|(i, r)| resource(t, i, r)).collect();
                    }
                    v
                })
                .collect();
            out.print(&json!({"ok": true,
                "databases": sh.databases.iter().map(|d| json!({"offset": d.offset, "id": format!("{:016x}", d.id), "programs": d.sections.iter().map(|s| s.1).sum::<usize>(), "vertex_or_mesh": d.sections[0].1, "pixel": d.sections[1].1, "geometry": d.sections[2].1, "compute": d.sections[5].1,
                    "effects_stated": d.effects_stated, "effects_read": d.effects.1, "shared_blocks": d.shared_blocks.len(), "shared_table_resources": d.table.as_ref().map(|t| t.resources.len()), "shared_table_members": d.table.as_ref().map(|t| t.members.len())})).collect::<Vec<_>>(),
                "programs": {"total": sh.programs.len(), "vertex_or_mesh": sh.section_total(0), "pixel": sh.section_total(1), "geometry": sh.section_total(2), "compute": sh.section_total(5),
                    "resources": sh.programs.iter().map(|p| p.table.resources.len()).sum::<usize>(), "constant_buffer_members": sh.programs.iter().map(|p| p.table.members.len()).sum::<usize>()},
                "effects": {"stated": sh.effects_stated(), "read": sh.effects.len(), "loaded_size_matches": sh.effects.iter().filter(|e| e.loaded_size == e.loaded_size_computed).count(),
                    "state_items": {"rasterizer": sh.effects.iter().map(|e| e.states.rasterizer.len()).sum::<usize>(), "depth_stencil": sh.effects.iter().map(|e| e.states.depth_stencil.len()).sum::<usize>(), "blend": sh.effects.iter().map(|e| e.states.blend.len()).sum::<usize>(), "samplers": sh.effects.iter().map(|e| e.states.samplers.len()).sum::<usize>()},
                    "pass_state_index_out_of_range": sh.effects.iter().map(|e| e.techniques.iter().flat_map(|t| &t.passes).filter(|ps| ps.rasterizer().is_some_and(|i| i as usize >= e.states.rasterizer.len()) || ps.depth_stencil().is_some_and(|i| i as usize >= e.states.depth_stencil.len()) || ps.blend().is_some_and(|i| i as usize >= e.states.blend.len())).count()).sum::<usize>(),
                    "passes_with_bindings": sh.effects.iter().flat_map(|e| &e.techniques).flat_map(|t| &t.passes).filter(|ps| ps.bindings.is_some()).count(),
                    "passes_with_binding_word": sh.effects.iter().flat_map(|e| &e.techniques).flat_map(|t| &t.passes).filter(|ps| ps.bindings.as_ref().is_some_and(|b| b.word.is_some())).count(),
                    "pass_block_index_out_of_range": sh.effects.iter().map(|e| e.techniques.iter().flat_map(|t| &t.passes).filter(|ps| ps.blocks().iter().flatten().any(|&i| i as usize >= e.blocks.len())).count()).sum::<usize>(),
                    "second_table_one_buffer": sh.effects.iter().filter(|e| e.second.as_ref().is_some_and(|t| t.resources.len() == 1 && t.members.is_empty())).count(),
                    "blocks_inline_read": sh.effects.iter().flat_map(|e| &e.layouts).flatten().count(),
                    "members_with_default": sh.effects.iter().filter_map(|e| e.table.as_ref()).chain(sh.databases.iter().filter_map(|d| d.table.as_ref())).chain(sh.programs.iter().map(|p| &p.table)).map(|t| (0..t.members.len()).filter(|&i| t.member_default(i).is_some()).count()).sum::<usize>(),
                    "resources_with_entry": sh.effects.iter().filter_map(|e| e.table.as_ref()).chain(sh.databases.iter().filter_map(|d| d.table.as_ref())).chain(sh.programs.iter().map(|p| &p.table)).map(|t| (0..t.resources.len()).filter(|&i| t.resource_entry(i).is_some()).count()).sum::<usize>(),
                    "blocks_inline": sh.effects.iter().flat_map(|e| &e.blocks).filter(|b| matches!(b, shaderfx::Block::Inline(..))).count(),
                    "blocks_shared": sh.effects.iter().flat_map(|e| &e.blocks).filter(|b| matches!(b, shaderfx::Block::Shared(_))).count(),
                    "blocks_shared_unresolved": sh.effects.iter().map(|e| e.blocks.iter().filter(|b| matches!(b, shaderfx::Block::Shared(k) if sh.databases[e.database].shared_block(*k).is_none())).count()).sum::<usize>(), "with_table": sh.effects.iter().filter(|e| e.table.is_some()).count(),
                    "with_programs": sh.effects.iter().filter(|e| e.programs.iter().any(|l| !l.is_empty())).count(), "with_techniques": sh.effects.iter().filter(|e| !e.techniques.is_empty()).count(),
                    "techniques": sh.effects.iter().map(|e| e.techniques.len()).sum::<usize>(), "passes": sh.effects.iter().flat_map(|e| &e.techniques).map(|t| t.passes.len()).sum::<usize>(),
                    "programs_not_in_any_pass": sh.effects.iter().map(|e| (0..6).map(|k| (0..e.programs[k].len()).filter(|&i| !e.techniques.iter().flat_map(|t| &t.passes).any(|ps| ps.programs[k] == Some(i as u8))).count()).sum::<usize>()).sum::<usize>()},
                "material_texture_parameters": {"total": tex.len(), "unnamed": unnamed.len(), "unnamed_list": unnamed},
                "pipeline_cache": cache_summary, "matched_effects": effects, "seconds": t.elapsed().as_secs_f64()}));
        }
        Cmd::Names { lookup, namehash } => {
            if let Some(p) = namehash {
                let mut r = sixng_core::names::import_namehash(&p, &ws.work)?;
                r["ok"] = json!(true);
                out.print(&r);
            } else if lookup.is_empty() {
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
        Cmd::Outfit { character, out: glb, archive, parts, add, list, clip, anim_only, in_place, tex, body_shape, head_skin, tex_remap, tint, look, hair, hair_density, hair_max, hair_points, hair_slots, hair_style, hair_no_guides } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let prefix = character.to_ascii_lowercase();
            let animal = sixng_core::outfit::is_animal(&prefix);
            let opts = if animal { sixng_core::outfit::animal_options(&ix, &prefix) } else { sixng_core::outfit::outfit_options(&ix, &prefix) };
            // a name with no parts of its own is an ad-hoc character made of
            // --parts (generic world peds: head, hand, feet, clothing items)
            // --look: the parts of one outfit of the game's outfit list
            // A model with no parts of its own (`cs_mandy`, made of shared
            // items) exports its first outfit, as if --look 0 were given.
            let auto = look.is_none() && !animal && opts.is_empty() && parts.is_empty();
            let look: &Option<usize> = &look.or(auto.then_some(0));
            // all parts of the --look outfit (strands and unresolved items too):
            // the hide masks below are worked out over them
            let mut look_outfit: Vec<sixng_core::pedlook::LookPart> = Vec::new();
            // the outfit's body-shape preset (its tag), used unless --body-shape is given
            let mut look_preset: Option<(Option<String>, Vec<String>)> = None;
            let mut look_parts: Vec<(u64, sixng_core::pedlook::LookPart)> = match look {
                Some(k) => match sixng_core::pedlook::look(&ws.game, &a, &ix, &prefix) {
                    // not a listed model either: the error below names the part rule
                    Err(_) if auto => Vec::new(),
                    Err(e) => bail!(e),
                    Ok(l) => match l.outfits.get(*k) {
                        None if auto => Vec::new(),
                        None => bail!("{prefix} has {} outfits (--look 0..{})", l.outfits.len(), l.outfits.len().saturating_sub(1)),
                        Some(o) => {
                            look_outfit = o.parts.clone();
                            look_preset = Some((o.body_shape.clone(), o.body_preset_items.clone()));
                            let ps = &o.parts;
                            let mut by_name = sixng_core::outfit::dedupe(ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| {
                                let n = e.name.as_ref()?.to_ascii_lowercase();
                                ps.iter().any(|p| !p.strands && p.drawable == n).then_some((n, e))
                            }));
                            // the parts in a content pack (the anatomical inserts, `packparts`)
                            let packs = sixng_core::packparts::get(&ws.game);
                            for p in ps.iter().filter(|p| !p.strands) {
                                if let (false, Some(e)) = (by_name.contains_key(&p.drawable), packs.by_name(&p.drawable)) {
                                    by_name.insert(p.drawable.clone(), e);
                                }
                            }
                            ps.iter().filter(|p| !p.strands).filter_map(|p| Some((by_name.get(&p.drawable)?.off, p.clone()))).collect()
                        }
                    },
                },
                None => Vec::new(),
            };
            if opts.is_empty() && parts.is_empty() && look_parts.is_empty() {
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
                            // then the content packs' ped drawables (`packparts`)
                            let pack = || sixng_core::packparts::get(&ws.game).entries.iter().find(|e| e.name.as_deref().is_some_and(|n| re.is_match(&n.to_ascii_lowercase())));
                            any.values().next().copied().or_else(pack).map(|e| e.off).ok_or_else(|| anyhow!("no part of {prefix} and no drawable in {} or the content packs matches {p:?} (see --list)", a.id))?
                        }
                    };
                    v.push(off);
                }
                Ok(v)
            };
            let mut offs: Vec<u64> = if !parts.is_empty() {
                pick(parts)?
            } else if !look_parts.is_empty() {
                look_parts.iter().map(|p| p.0).collect()
            } else {
                opts.iter().filter(|o| o.default).map(|o| o.off).collect()
            };
            for o in pick(add)? {
                if !offs.contains(&o) {
                    offs.push(o);
                }
            }
            // Hide masks over the exported parts, by drawable name. A part's item
            // is its --look outfit part, else the same drawable in the
            // character's wardrobe (the first outfit or variation that wears
            // it): --parts, --add and the default parts. A garment another
            // part hides regions of is drawn as its `_cull_00` twin.
            let mut hidden: std::collections::HashMap<String, (Vec<String>, Vec<String>)> = Default::default();
            let mut layers: std::collections::HashMap<String, sixng_core::export::PartLayer> = Default::default();
            if !animal {
                let name_of = |off: u64| ix.entries.iter().find(|e| e.off == off).and_then(|e| e.name.as_ref()).map(|n| n.to_ascii_lowercase());
                let mut all = look_outfit.clone();
                let extra: Vec<u64> = offs.iter().copied().filter(|o| !look_parts.iter().any(|p| p.0 == *o)).collect();
                if !extra.is_empty() {
                    if let Ok(l) = sixng_core::pedlook::look(&ws.game, &a, &ix, &prefix) {
                        let worn: Vec<&sixng_core::pedlook::LookPart> = l.outfits.iter().chain(&l.other_variations).flat_map(|o| &o.parts).filter(|p| !p.strands).collect();
                        for n in extra.iter().filter_map(|&o| name_of(o)) {
                            if let Some(p) = worn.iter().find(|p| p.drawable == n) {
                                all.push((*p).clone());
                            }
                        }
                    }
                }
                let names: std::collections::HashSet<String> = ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| e.name.as_ref().map(|n| n.to_ascii_lowercase())).collect();
                let twins = sixng_core::pedlook::cull_twins(&all, |n| names.contains(n));
                for (i, p) in all.iter().enumerate() {
                    let (own, waist) = sixng_core::pedlook::hidden_masks(&all, i);
                    if !own.is_empty() || !waist.is_empty() {
                        hidden.insert(twins.get(&p.drawable).unwrap_or(&p.drawable).clone(), (own, waist));
                    }
                    if let Some(layer) = &p.layer {
                        let insert = sixng_core::pedlook::is_skin_insert(&p.slot);
                        let garment = !insert && !matches!(p.slot.as_str(), "body upper" | "body lower" | "head" | "eyes" | "teeth");
                        layers.insert(twins.get(&p.drawable).unwrap_or(&p.drawable).clone(), sixng_core::export::PartLayer { layer: layer.clone(), insert, garment, hides: p.hides.iter().map(|h| h.0.clone()).collect() });
                    }
                }
                let twin_of = sixng_core::outfit::dedupe(ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| {
                    let n = e.name.as_ref()?.to_ascii_lowercase();
                    twins.values().any(|t| *t == n).then_some((n, e))
                }));
                for off in offs.iter_mut() {
                    let Some(t) = name_of(*off).and_then(|n| twins.get(&n)) else { continue };
                    let Some(e) = twin_of.get(t) else { continue };
                    for lp in look_parts.iter_mut().filter(|lp| lp.0 == *off) {
                        lp.0 = e.off;
                        lp.1.drawable = t.clone();
                    }
                    *off = e.off;
                }
            }
            let sel: Vec<&sixng_core::scan::Entry> = offs
                .iter()
                .filter_map(|o| match sixng_core::packparts::is_pack_key(*o) {
                    true => sixng_core::packparts::loaded().and_then(|t| t.entries.iter().find(|e| e.off == *o)),
                    false => ix.entries.iter().find(|e| e.off == *o),
                })
                .collect();
            let glb = glb.clone().unwrap_or_else(|| PathBuf::from(format!("{prefix}.glb")));
            // without --body-shape a --look outfit is posed in its own preset
            let body_shape = match (body_shape, &look_preset) {
                (Some(s), _) => Some(s.clone()),
                (None, Some((tag, items))) => match sixng_core::bodyshape::outfit_preset(&ws.game, tag.as_deref(), items) {
                    None => None,
                    Some(Ok(c)) => Some(c.iter().filter(|(_, v)| *v != 0.0).map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join(",")).filter(|s| !s.is_empty()),
                    Some(Err(e)) => {
                        eprintln!("body-shape preset {}: {e} (exported neutral)", tag.as_deref().unwrap_or("of the outfit"));
                        None
                    }
                },
                (None, None) => None,
            };
            let oa = OutArgs { out: glb.parent().map(|p| p.to_path_buf()).unwrap_or_default(), tex: tex.clone(), raw: false, force: true, no_embedded_textures: false, dry_run: false, body_shape, head_skin: *head_skin, paint: None };
            if *anim_only && clip.is_empty() {
                bail!("--anim-only needs at least one --clip");
            }
            let mut dicts: Vec<sixng_core::anim::ClipDictionary> = Vec::new();
            let mut picks: Vec<(usize, usize)> = Vec::new();
            // cutscene clips: `cin@<scene>` = this character's own clips in
            // that scene; `cin@<scene>/<clip>` = one clip of the scene. Their
            // dictionaries are loaded with the scene's sections joined in.
            let mut wanted: Vec<(String, String)> = Vec::new();
            let mut cat: Option<sixng_core::cutscene::Catalog> = None;
            let scene_dict = |dicts: &mut Vec<sixng_core::anim::ClipDictionary>, c: &sixng_core::cutscene::Catalog, sd: &sixng_core::cutscene::SceneDict| -> Result<()> {
                if !dicts.iter().any(|x| x.name == sd.name) {
                    dicts.push(sixng_core::cutscene::load_dictionary(&ws, c, sd)?);
                }
                Ok(())
            };
            for c in clip {
                let is_scene = c.starts_with("cin@") || c.starts_with("cine@");
                if !is_scene {
                    let (dq, cq) = c.rsplit_once('/').ok_or_else(|| anyhow!("--clip wants <dictionary>/<clip>"))?;
                    wanted.push((dq.to_string(), cq.to_string()));
                    continue;
                }
                if cat.is_none() {
                    cat = Some(sixng_core::cutscene::catalog(&ws, sixng_core::cutscene::STORY_ARCHIVE, false, pr.clone())?);
                }
                let c_ = cat.as_ref().unwrap();
                match c.split_once('/') {
                    Some((dq, cq)) => {
                        let sd = c_.dicts.iter().find(|d| d.name.eq_ignore_ascii_case(dq)).ok_or_else(|| anyhow!("no cutscene dictionary {dq:?}"))?;
                        scene_dict(&mut dicts, c_, sd)?;
                        wanted.push((sd.name.clone(), cq.to_string()));
                    }
                    None => {
                        for (d, cl) in sixng_core::cutscene::model_clips(c_, c, &prefix)? {
                            eprintln!("--clip {c}: {}/{cl}", d.name);
                            scene_dict(&mut dicts, c_, d)?;
                            wanted.push((d.name.clone(), cl));
                        }
                    }
                }
            }
            for (dq, cq) in &wanted {
                let (dq, cq) = (dq.as_str(), cq.as_str());
                // scene dictionaries are loaded above, by name
                let di = match dicts.iter().position(|x| x.name == dq) {
                    Some(i) => i,
                    None => {
                        let (_, d) = sixng_core::anim::load_dictionary(&game, &ws.work, dq)?;
                        match dicts.iter().position(|x| x.name == d.name) {
                            Some(i) => i,
                            None => {
                                dicts.push(d);
                                dicts.len() - 1
                            }
                        }
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
            for (_, p) in &look_parts {
                // a head's face and a body item's painted skin, where stored
                if (matches!(p.slot.as_str(), "head" | "body upper" | "body lower") || sixng_core::pedlook::is_skin_insert(&p.slot)) && !p.skins.is_empty() {
                    eo.part_skins.insert(p.drawable.clone(), p.skins.iter().filter_map(|(a, n)| Some((u64::from_str_radix(a, 16).ok()?, u64::from_str_radix(n, 16).unwrap_or(0)))).collect());
                }
                // tattoos, makeup and body marks the outfit bakes into the skin
                if !p.decorations.is_empty() {
                    eo.part_decorations.insert(p.drawable.to_ascii_lowercase(), p.decorations.iter().filter_map(|n| u32::from_str_radix(n, 16).ok()).collect());
                    if let Some(t) = p.skin_tone.as_deref().and_then(|t| u32::from_str_radix(t, 16).ok()) {
                        eo.skin_tone = t;
                    }
                }
                if p.colour.is_none() && p.preset_colours.iter().any(|c| c.is_some()) {
                    eo.part_colours.insert(p.drawable.clone(), p.preset_colours.clone());
                }
                if let Some((pal, t)) = p.tint.as_ref().and_then(|t| Some((u32::from_str_radix(&t.palette, 16).ok()?, t))) {
                    eo.ped_tints.insert(p.drawable.clone(), (pal, t.rows));
                }
            }
            eo.part_hidden = hidden;
            eo.part_layers = layers;
            for t in tint {
                let (name, pal, rows) = sixng_core::pedtint::parse_spec(t).map_err(|e| anyhow!(e))?;
                eo.ped_tints.insert(name, (pal, rows));
            }
            let mut r = sixng_core::outfit::export_outfit(&a, &sel, &prefix, &glb, &eo, &clip_refs, *anim_only, *in_place)?;
            if *hair && !*anim_only {
                // the strand pieces of the outfit, then the extra styles
                let slots: Vec<String> = if hair_slots.iter().any(|s| s == "all") { Vec::new() } else { hair_slots.clone() };
                let ped = sixng_core::pedlook::look(&ws.game, &a, &ix, &prefix);
                let mut pieces = match &ped {
                    Ok(l) => sixng_core::hairexport::character_grooms(&ws.work, &a, &ix.entries, l, look.unwrap_or(0), &slots).map_err(|e| anyhow!(e))?,
                    Err(_) if !hair_style.is_empty() => Vec::new(),
                    Err(e) => bail!("--hair: no outfit list for {prefix}: {e}"),
                };
                // the colours the viewer gives each slot: the game's strand tables, else the items' presets
                let outfit_of = ped.as_ref().ok().and_then(|l| l.outfits.get(look.unwrap_or(0)));
                let tables = sixng_core::hairtables::load_or_build(&ws, &a).map_err(|e| eprintln!("--hair: strand tables not read: {e:#}")).ok().map(|v| sixng_core::hairtone::StrandData::from_json(&v));
                let presets = sixng_core::hairtone::Table::load_or_build(&ws).ok();
                for st in hair_style {
                    let re = regex::RegexBuilder::new(st).case_insensitive(true).build()?;
                    pieces.extend(sixng_core::hairexport::placeholder_grooms(&ws.work, &a, &ix.entries, &re).map_err(|e| anyhow!(e))?);
                }
                let cat = sixng_core::groom::catalog_cached(&ws.work, &a, &ix.entries)?;
                let o = sixng_core::hairexport::Options { density: *hair_density, max_strands: *hair_max, points: *hair_points, guides: !*hair_no_guides };
                let (mut prims, mut rows) = (Vec::new(), Vec::new());
                // the head as written, to find which of its UV sets the root UVs are in
                let model = std::fs::read(&glb).ok();
                let head = model.as_deref().and_then(sixng_core::hairexport::head_from_glb);
                // a hat of the outfit: the scalp hair is pressed under it, as in the viewer
                let hats: Vec<String> = outfit_of.map(|of| of.parts.iter().filter(|p| p.slot == "hat").map(|p| p.drawable.to_ascii_lowercase()).collect()).unwrap_or_default();
                let hat = model.as_deref().and_then(|b| sixng_core::hairexport::hat_shell(b, &hats));
                for (slot, placeholder, grooms, confidence) in &pieces {
                    for off in grooms {
                        let g = sixng_core::groom::load(&a, &ix.entries, *off)?;
                        let bones = cat.iter().find(|c| c.off == *off).map(|c| c.bones.clone()).unwrap_or_default();
                        let name = format!("{}_{off:x}", slot.replace(' ', "_"));
                        let mut ex = json!({"slot": slot, "placeholder": placeholder, "groom": format!("{off:x}"), "bones": bones});
                        let uv_set = sixng_core::hairexport::tag_uv_set(&g, head.as_ref(), &mut ex);
                        let colour = outfit_of.and_then(|of| sixng_core::hairexport::slot_colour(of, slot, tables.as_ref(), presets.as_ref()));
                        if hat.is_some() && slot == "hair" {
                            ex["under_hat"] = json!(hats);
                        }
                        if let Some((c, set)) = &colour {
                            ex["colour_set"] = json!(format!("{set:08x}"));
                            ex["colour_source"] = json!(match c { sixng_core::hairexport::HairColour::Set(_) => "strand tables", sixng_core::hairexport::HairColour::Presets(_) => "item presets" });
                        }
                        let (p, mut sum) = sixng_core::hairexport::piece(&g, &name, &o, colour.as_ref().map(|c| &c.0), hat.as_ref().filter(|_| slot == "hair"), ex);
                        sum["head_uv_set"] = json!(uv_set);
                        sum["colour_set"] = json!(colour.as_ref().map(|c| format!("{:08x}", c.1)));
                        sum["slot"] = json!(slot);
                        sum["placeholder"] = json!(placeholder);
                        sum["confidence"] = json!(confidence);
                        rows.push(sum);
                        prims.extend(p);
                    }
                }
                let file = glb.with_extension("hair.glb");
                let bytes = sixng_core::gltf::strands_glb(&format!("{prefix}_hair"), &prims, json!({"character": prefix, "coordinate_system": "model space (Z-up metres) converted to glTF Y-up (x, z, -y), as the model"}));
                let size = bytes.len();
                sixng_core::atomic::write(&file, bytes)?;
                r["hair"] = json!({"file": file, "bytes": size, "pieces": rows});
            }
            r["seconds"] = t.elapsed().as_secs_f64().into();
            out.print(&json!({"ok": true, "result": r}));
        }
        Cmd::PedLook { model, archive } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            // a regex (starts with ^): one summary line per matching character
            if model.starts_with('^') {
                let re = regex::RegexBuilder::new(model).case_insensitive(true).build()?;
                let mut rows = Vec::new();
                // the characters with parts of their own, then the outfit list's other models
                let mut names: Vec<String> = sixng_core::outfit::characters(&ix).into_iter().filter(|c| c.kind == "character").map(|c| c.name).collect();
                let own: std::collections::HashSet<String> = names.iter().cloned().collect();
                names.extend(sixng_core::pedlook::listed_models(&ws.game).unwrap_or_default().into_iter().map(|m| m.name).filter(|n| !own.contains(n) && !sixng_core::outfit::is_animal(n)));
                for name in names.into_iter().filter(|n| re.is_match(n)) {
                    let own_parts = own.contains(&name);
                    rows.push(match sixng_core::pedlook::look(&ws.game, &a, &ix, &name) {
                        Ok(l) => {
                            let o = &l.outfits[0];
                            let has = |s: &str| o.parts.iter().any(|p| p.slot == s);
                            json!({"model": name, "own_parts": own_parts, "outfits": l.outfits.len(), "fit": l.fit, "default": o.label, "parts": o.parts.len(), "head": has("head"), "eyes": has("eyes"), "body": has("body upper") && has("body lower"),
                                "clothed": has("full") || (has("top") || has("outer")) && has("bottom"), "shoes": has("feet"), "hair": o.parts.iter().filter(|p| p.slot == "hair").map(|p| p.guid.clone().unwrap_or_default()).next(), "unresolved": o.unresolved.len()})
                        }
                        Err(e) => json!({"model": name, "own_parts": own_parts, "error": e}),
                    });
                }
                out.print(&json!({"ok": true, "count": rows.len(), "characters": rows}));
                return Ok(());
            }
            let l = sixng_core::pedlook::look(&ws.game, &a, &ix, model).map_err(|e| anyhow!(e))?;
            out.print(&json!({"ok": true, "look": l}));
        }
        Cmd::PedParts { model, out: dir, archive } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let write = |set: &sixng_core::pedset::PartSet| -> Result<Option<String>> {
                let Some(d) = dir else { return Ok(None) };
                let p = d.join(&set.model).join("parts.json");
                std::fs::create_dir_all(p.parent().unwrap())?;
                std::fs::write(&p, serde_json::to_vec_pretty(set)?)?;
                Ok(Some(p.display().to_string()))
            };
            if model.starts_with('^') {
                let re = regex::RegexBuilder::new(model).case_insensitive(true).build()?;
                let mut rows = Vec::new();
                let (mut ok, mut complete) = (0, 0);
                // the characters with parts of their own, then the named
                // models of the outfit list (`cs_mandy` has no drawable of
                // its own: all of its parts are shared items)
                let mut names: Vec<String> = sixng_core::outfit::characters(&ix).into_iter().filter(|c| c.kind == "character").map(|c| c.name).collect();
                let own = names.len();
                let mut seen: std::collections::HashSet<String> = names.iter().cloned().collect();
                for m in sixng_core::pedlook::listed_models(&ws.game).unwrap_or_default().into_iter().filter(|m| m.named && !m.uncertain) {
                    if seen.insert(m.name.clone()) {
                        names.push(m.name);
                    }
                }
                for (i, name) in names.iter().enumerate().filter(|(_, n)| re.is_match(n)) {
                    let own_parts = i < own;
                    rows.push(match sixng_core::pedset::of(&ws.game, &a, &ix, name) {
                        Ok(set) => {
                            ok += 1;
                            complete += set.complete as usize;
                            let variants: usize = set.parts.iter().map(|p| p.variants.len()).sum();
                            json!({"model": set.model, "display": set.display, "own_parts": own_parts, "outfits": set.outfits.iter().filter(|o| o.listed).count(), "other_variations": set.outfits.iter().filter(|o| !o.listed).count(), "parts": set.parts.len(), "variants": variants, "complete": set.complete, "unresolved": set.unresolved, "file": write(&set)?})
                        }
                        Err(e) => json!({"model": name, "own_parts": own_parts, "error": e}),
                    });
                }
                out.print(&json!({"ok": true, "count": rows.len(), "with_set": ok, "complete": complete, "characters": rows}));
                return Ok(());
            }
            let set = sixng_core::pedset::of(&ws.game, &a, &ix, model).map_err(|e| anyhow!(e))?;
            let file = write(&set)?;
            out.print(&json!({"ok": true, "file": file, "set": set}));
        }
        Cmd::Characters { name, kind, archive } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let re = name.as_deref().map(|n| regex::RegexBuilder::new(n).case_insensitive(true).build()).transpose()?;
            let all = sixng_core::outfit::characters(&ix);
            let listed: std::collections::HashSet<String> = all.iter().map(|c| c.name.clone()).collect();
            let v: Vec<_> = all.into_iter().filter(|c| re.as_ref().is_none_or(|r| r.is_match(&c.name)) && kind.as_deref().is_none_or(|k| k == c.kind)).collect();
            // every model of the game's outfit list, named where a name is known
            let (models, models_note) = match sixng_core::pedlook::listed_models(&ws.game) {
                Ok(m) => (
                    m.into_iter()
                        .filter(|m| re.as_ref().is_none_or(|r| r.is_match(&m.name)) && kind.as_deref().is_none_or(|k| k == "model"))
                        .map(|m| json!({"name": m.name, "hash": m.hash, "named": m.named, "display": m.display, "uncertain": m.uncertain, "outfits": m.outfits, "listed_as_character": listed.contains(&m.name)}))
                        .collect::<Vec<_>>(),
                    None,
                ),
                Err(e) => (Vec::new(), Some(e)),
            };
            out.print(&json!({"ok": true, "archive": a.id, "count": v.len(), "characters": v, "models_count": models.len(), "models_named": models.iter().filter(|m| m["named"] == true).count(), "models": models, "models_note": models_note}));
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
                    // scene clip dictionaries by name, from the cutscene catalog
                    let (de, d) = if q.starts_with("cin@") || q.starts_with("cine@") {
                        let c = sixng_core::cutscene::catalog(&ws, sixng_core::cutscene::STORY_ARCHIVE, false, pr.clone())?;
                        let sd = c.dicts.iter().find(|d| d.name.eq_ignore_ascii_case(q)).ok_or_else(|| anyhow!("no cutscene dictionary {q:?} (`sixng cutscenes --name <regex>` lists them)"))?;
                        (sixng_core::anim::DictEntry { name: sd.name.clone(), id: sd.id.clone() }, sixng_core::cutscene::load_dictionary(&ws, &c, sd)?)
                    } else {
                        sixng_core::anim::load_dictionary(&game, &ws.work, q)?
                    };
                    let clips: Vec<_> = d
                        .clips
                        .iter()
                        .filter(|c| re.as_ref().is_none_or(|r| r.is_match(&c.name)))
                        .take(*limit)
                        .map(|c| {
                            let tr = sixng_core::anim::all_tracks(&d, c);
                            json!({"name": c.short_name(), "hash": format!("{:08x}", c.hash), "type": c.kind, "duration": c.duration, "bindings": c.bindings.len(), "sections": c.sections, "missing_windows": c.pending.len(),
                                "tracks": tr.len(), "bones": tr.iter().filter(|t| t.0 != 0).map(|t| t.0).collect::<std::collections::BTreeSet<_>>().len(),
                                // facial clips: the face controls they animate, by name (hex when unknown)
                                "facial_controls": tr.iter().filter(|t| t.1 == sixng_core::facecontrols::FACIAL_TRANSLATION).map(|t| sixng_core::facecontrols::label(t.0)).collect::<Vec<_>>()})
                        })
                        .collect();
                    out.print(&json!({"ok": true, "dictionary": de.name, "id": de.id, "clip_count": d.clips.len(), "animations": d.animations.len(), "clips": clips}));
                }
            }
        }
        Cmd::StoryNames { tags } => {
            use sixng_core::cutscene;
            let c = cutscene::catalog(&ws, cutscene::STORY_ARCHIVE, false, pr.clone())?;
            let ids = cutscene::mission_ids(c.dicts.iter().filter_map(|d| cutscene::parse_dict_name(&d.name)).map(|(s, _)| s));
            let t = sixng_core::storyflow::tags(&ws)?;
            let n = sixng_core::storyflow::match_names(&t, &ids);
            let mut v = json!({"ok": true, "missions_in_scenes": ids.len(), "named_missions": n.missions.len(), "named_arcs": n.arcs.len(), "arcs": n.arcs, "missions": n.missions});
            if *tags {
                v["tags"] = json!(t);
            }
            out.print(&v);
        }
        Cmd::Cutscenes { scene, name, cast, archive, rescan, summary, limit } => {
            use sixng_core::cutscene;
            let t = Instant::now();
            let c = cutscene::catalog(&ws, archive, *rescan, pr.clone())?;
            let models = cutscene::Models::of_workspace(&ws);
            let all = cutscene::scenes(&c, &models);
            let re = name.as_deref().map(|n| regex::RegexBuilder::new(n).case_insensitive(true).build()).transpose()?;
            let cre = cast.as_deref().map(|n| regex::RegexBuilder::new(n).case_insensitive(true).build()).transpose()?;
            let sel: Vec<&cutscene::Scene> = match scene {
                Some(q) => vec![cutscene::find_scene(&all, q)?],
                None => all
                    .iter()
                    .filter(|s| re.as_ref().is_none_or(|r| r.is_match(&s.name)))
                    .filter(|s| cre.as_ref().is_none_or(|r| s.cast.iter().any(|o| r.is_match(&o.handle) || o.model.as_deref().is_some_and(|m| r.is_match(m)))))
                    .collect(),
            };
            let objs = || sel.iter().flat_map(|s| s.cast.iter().chain(&s.props).chain(&s.other));
            let count = |k: &str| objs().filter(|o| o.kind == k).count();
            let clips: usize = sel.iter().map(|s| s.cast.iter().chain(&s.props).chain(&s.other).map(|o| o.clips.len() + o.face_clips.len()).sum::<usize>()).sum();
            let totals = json!({
                "scenes": sel.len(),
                "variants": sel.iter().filter(|s| s.variant_of.is_some()).count(),
                "with_scene_file": sel.iter().filter(|s| !s.scene_files.is_empty()).count(),
                "section_dictionaries": sel.iter().map(|s| s.sections).sum::<usize>(),
                "section_dictionaries_adopted": c.dicts.iter().filter(|d| sel.iter().any(|s| s.dictionary_id == d.id)).map(|d| d.adopted_sections).sum::<usize>(),
                "clips": clips + sel.iter().map(|s| s.cameras.len()).sum::<usize>(),
                "cast": {"characters": count("character"), "peds_with_model": objs().filter(|o| o.kind == "ped" && o.model.is_some()).count(),
                    "peds_model_unresolved": objs().filter(|o| o.kind == "ped" && o.model.is_none() && o.model_id.is_some()).count(),
                    "peds_without_record": objs().filter(|o| o.kind == "ped" && o.model_id.is_none()).count(),
                    "model_from_other_scenes": objs().filter(|o| o.model_source == Some("other scenes")).count()},
                "props": count("prop"),
                "cameras": sel.iter().map(|s| s.cameras.len()).sum::<usize>(),
                "unknown": count("unknown"),
                "streamed_objects": objs().filter(|o| o.streamed).count(),
                "streamed_windows": sel.iter().map(|s| s.windows_streamed).sum::<usize>(),
                "streamed_windows_missing": sel.iter().map(|s| s.windows_missing).sum::<usize>(),
                "scenes_with_missing_windows": sel.iter().filter(|s| s.windows_missing > 0).count(),
                "set_models": sel.iter().map(|s| s.set_models.len()).sum::<usize>(),
                "set_models_unresolved": sel.iter().map(|s| s.set_models_unresolved.len()).sum::<usize>(),
            });
            let mut o = json!({"ok": true, "archive": c.archive, "dictionaries": c.dicts.len(), "scene_files": c.files.len(), "totals": totals, "seconds": t.elapsed().as_secs_f64()});
            if !*summary {
                let take = if *limit == 0 { sel.len() } else { *limit };
                o["scenes"] = json!(sel.iter().take(take).collect::<Vec<_>>());
            }
            out.print(&o);
        }
        Cmd::SceneDialogue { input, text_table, out: path } => {
            let default = ws.work.join("scene_dialogue.json");
            out.print(&scene_dialogue::generate(&ws, input, text_table, path.as_deref().unwrap_or(&default))?);
        }
        Cmd::Ttpf { store, hash, first, mip, all_slices, float, all, out: tout } => {
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
                    if *float && sixng_core::texture::Fmt::from_dxgi(t.dxgi_format).is_float() {
                        let p = dir.join(format!("{:016x}_m{mip}.f32", t.hash));
                        let r = (0..t.slices()).try_fold((Vec::new(), 0, 0, 0), |mut acc, sl| {
                            let (v, ch, w, h) = sixng_core::ttpf::decode_float_slice(t, &data, *mip, sl)?;
                            acc.0.extend(v.iter().flat_map(|x| x.to_le_bytes()));
                            anyhow::Ok((acc.0, ch, w, h))
                        });
                        match r.and_then(|(bytes, ch, w, h)| sixng_core::atomic::write(&p, bytes).map(|_| (ch, w, h)).map_err(Into::into)) {
                            Ok((ch, w, h)) => res.push(json!({"hash": format!("{:016x}", t.hash), "f32": p, "w": w, "h": h, "slices": t.slices(), "channels": ch, "format": sixng_core::texture::dxgi_name(t.dxgi_format), "layout": "little-endian f32; slices in order, rows top to bottom, channels interleaved", "undecoded_bytes": skipped, "partial": skipped > 0})),
                            Err(e) => res.push(json!({"hash": format!("{:016x}", t.hash), "slices": t.slices(), "error": format!("float export: {e}")})),
                        }
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
                    sixng_core::atomic::write(&idx, serde_json::to_vec_pretty(&json!({"note": note, "store": a.id, "textures": st.textures.len(), "written": ok, "complete": ok - partial, "partial": partial, "bcpack_only": bcpack, "failed": failed, "results": res}))?)?;
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
        Cmd::Grooms { archive, roots, ribbons, full, density, animal, only, list, head, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let t = Instant::now();
            if *list {
                let mut cat = sixng_core::groom::catalog(&a, &ix.entries)?;
                if let Ok(t) = sixng_core::hairlink::get(&ws.work, &a, &ix.entries) {
                    t.relabel(&mut cat);
                }
                let dir = po.join("grooms").join(a.id.trim_end_matches(".rpf"));
                std::fs::create_dir_all(&dir)?;
                sixng_core::atomic::write(dir.join("catalog.json"), serde_json::to_vec_pretty(&cat)?)?;
                let mut by: std::collections::BTreeMap<&str, usize> = Default::default();
                for g in cat.iter().filter(|g| g.lod == 0) {
                    *by.entry(g.category.as_str()).or_default() += 1;
                }
                out.print(&json!({"ok": true, "grooms": cat.len(), "chains": cat.iter().filter(|g| g.lod == 0).count(), "chains_by_region": by, "file": dir.join("catalog.json"), "seconds": t.elapsed().as_secs_f64()}));
            } else {
                let mut only: Vec<u64> = only.iter().filter_map(|h| u64::from_str_radix(h.trim_start_matches("0x"), 16).ok()).collect();
                let mut fur = serde_json::Value::Null;
                if let Some(root) = animal {
                    let root = root.to_ascii_lowercase();
                    // the animal's parts (not its fur placeholders), one entry per name
                    let mut seen = std::collections::HashSet::new();
                    let skin: Vec<(String, Vec<[f32; 3]>)> = ix
                        .entries
                        .iter()
                        .filter(|e| e.rtype.as_deref() == Some("dr"))
                        .filter_map(|e| {
                            let n = e.name.as_ref()?.to_ascii_lowercase();
                            (n.starts_with(&format!("{root}_")) && !n.contains("_fur_") && seen.insert(n.clone())).then_some((n, e))
                        })
                        .filter_map(|(n, e)| Some((n, sixng_core::pedlook::mesh_points(&a, e)?)))
                        .collect();
                    if skin.is_empty() {
                        bail!("no parts named {root}_<part>_NN in {} (`sixng characters --kind animal` lists the roots)", a.id);
                    }
                    let link = sixng_core::furlink::for_animal(&ws.work, &a, &ix.entries, &root, &skin)?;
                    let picked: Vec<&sixng_core::furlink::Variant> = ["body", "head"].iter().filter_map(|s| link.default_variant(s)).collect();
                    if picked.is_empty() {
                        bail!("no fur grooms were placed for {root}: {}", link.note);
                    }
                    only = picked.iter().flat_map(|v| v.grooms.iter().copied()).collect();
                    fur = json!({"animal": root, "variants": picked.iter().map(|v| json!({"name": v.name, "slot": v.slot, "grooms": v.grooms.iter().map(|g| format!("{g:x}")).collect::<Vec<_>>(), "count_known": v.known, "count_exact": v.exact})).collect::<Vec<_>>(),
                        "other_variants": link.variants.iter().filter(|v| !picked.iter().any(|p| p.name == v.name)).map(|v| v.name.clone()).collect::<Vec<_>>(),
                        "agreement": link.agreement, "note": link.note,
                        "link": if link.read { "read from the game: placeholder name id, piece table, file ids of the table of contents" } else { "inferred from archive order, piece counts and distance to the skin (the table of contents could not be read)" }});
                    let dir = po.join("grooms").join(a.id.trim_end_matches(".rpf"));
                    std::fs::create_dir_all(&dir)?;
                    sixng_core::atomic::write(dir.join(format!("{root}.fur.json")), serde_json::to_vec_pretty(&fur)?)?;
                }
                let head = match head {
                    Some(p) => Some(sixng_core::hairexport::head_from_glb(&std::fs::read(p)?).ok_or_else(|| anyhow!("--head: no mesh in {}", p.display()))?),
                    None => None,
                };
                let (n, errors) = sixng_core::groom::export_grooms(&a, &ix.entries, &only, po, *roots, ribbons.map(|mm| mm / 1000.0), full.then_some(density.clamp(0.0, 1.0)), head.as_ref(), pr)?;
                out.print(&json!({"ok": true, "archive": a.id, "grooms": n, "errors": errors, "fur": fur,
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
        Cmd::Text { archive, only, label, find, limit, plain, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let t = Instant::now();
            let only: Vec<u64> = only.iter().filter_map(|h| u64::from_str_radix(h.trim_start_matches("0x"), 16).ok()).collect();
            let (tables, errors) = sixng_core::text::read_all(&a, &ix.entries, &only, pr)?;
            if let Some(q) = find {
                let re = regex::RegexBuilder::new(q).case_insensitive(true).build()?;
                // one copy of a string per language table: group by label and text
                let mut hits: std::collections::BTreeMap<(u32, String), (usize, String)> = Default::default();
                for tb in &tables {
                    for (h, s) in &tb.strings {
                        let p = sixng_core::text::plain(s);
                        if re.is_match(&p) {
                            let e = hits.entry((*h, if *plain { p } else { s.clone() })).or_insert((0, tb.id.clone()));
                            e.0 += 1;
                        }
                    }
                }
                let total = hits.len();
                let rows: Vec<serde_json::Value> = hits.into_iter().take(*limit).map(|((h, s), (n, first))| json!({"label": format!("{h:08x}"), "text": s, "tables": n, "first_table": first})).collect();
                out.print(&json!({"ok": true, "archive": a.id, "tables": tables.len(), "errors": errors, "matches": total, "shown": rows.len(), "seconds": t.elapsed().as_secs_f64(), "hits": rows}));
            } else if !label.is_empty() {
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
                let Some(po) = po else { bail!("--out is needed unless --label or --find is given") };
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
            let (sets, lights, placed, (tc, distant, lod)) = export::export_lights(&ws, &a, &sel, po, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "light_sets": sets, "lights": lights, "placed_lights": placed,
                "distant_lights": distant, "unpaired_lod_lights": lod, "timecycle_modifier_regions": tc,
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
            sixng_core::atomic::write(po, serde_json::to_vec_pretty(&v)?)?;
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
            sixng_core::atomic::write(&f, serde_json::to_vec_pretty(&v)?)?;
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
        Cmd::Rbf { archive, filter, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let mut f = filter.to_filter()?;
            if f.kinds.is_empty() {
                f.kinds = vec![Kind::Rbf];
            }
            let sel = f.select(&ix)?;
            let t = Instant::now();
            let s = sixng_core::rbf::export(&a, &sel, po, pr)?;
            out.print(&json!({"ok": true, "archive": a.id, "selected": sel.len(), "decoded": s.decoded, "failed": s.failed, "errors": s.errors, "seconds": t.elapsed().as_secs_f64(), "out": po.join("rbf").join(a.id.trim_end_matches(".rpf"))}));
        }
        Cmd::Materials { archive, preset, weapon, out: po } => {
            use sixng_core::matlib;
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let t = Instant::now();
            // parameter name where recovered, else the hash
            let pn = |h: u32| sixng_core::names::lookup(h, sixng_core::names::Ctx::Param).unwrap_or_else(|| format!("{h:08x}"));
            let ids = |a: &[u32; 8]| a.iter().map(|b| format!("{b:08x}")).collect::<Vec<_>>();
            let rec = |r: &matlib::Record| {
                json!({
                    "tiles": ids(&r.tiles),
                    "profiles": ids(&r.profiles),
                    "bases": r.bases.iter().map(|b| format!("{b:08x}")).collect::<Vec<_>>(),
                    "floats": r.floats.iter().map(|p| json!([pn(p.0), p.1, p.2, p.3])).collect::<Vec<_>>(),
                    "textures": r.textures.iter().map(|p| json!([pn(p.0), p.1, p.2, format!("{:016x}", p.3)])).collect::<Vec<_>>(),
                    "colours": r.colours.iter().map(|p| json!([pn(p.0), p.1, p.2, format!("{:08x}", p.3)])).collect::<Vec<_>>(),
                    "ints": r.ints.iter().map(|p| json!([pn(p.0), p.1, p.2, p.3])).collect::<Vec<_>>(),
                })
            };
            let want: Vec<u32> = preset.iter().map(|p| u32::from_str_radix(p.trim_start_matches("0x"), 16).map_err(|_| anyhow!("bad preset {p}"))).collect::<Result<_>>()?;
            let (mut index, mut shown) = (matlib::Index::default(), serde_json::Map::new());
            index.version = matlib::VERSION;
            // the parameter-defaults file is reached through the table of contents only
            let defaults_err = match matlib::Defaults::read(&ws) {
                Ok(d) => {
                    index.defaults = d;
                    None
                }
                Err(e) => Some(e),
            };
            for e in ix.entries.iter().filter(|e| e.kind == Kind::Pso) {
                let Ok(b) = sixng_core::scan::read_entry(&a.path, e) else { continue };
                let Some(p) = sixng_core::pso::parse(&b).filter(|p| p.structs.contains_key(&matlib::ROOT)) else { continue };
                let mut lib = matlib::Library::from_meta(&p.meta());
                lib.defaults = index.defaults.clone();
                index.add(&e.id, &lib);
                for &w in &want {
                    if let Some(r) = lib.presets.get(&w) {
                        let bases: serde_json::Map<String, serde_json::Value> = r.bases.iter().filter_map(|b| Some((format!("{b:08x}"), rec(lib.base.get(b)?)))).collect();
                        let colours: Vec<_> = (0..8).map(|s| lib.colour(w, s, 0).map(|c| json!({"srgb": format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2]), "alpha": c[3], "stops_linear": matlib::stops([c[0], c[1], c[2]])}))).collect();
                        let tiles: Vec<_> = (0..8).map(|s| lib.tile(w, s)).collect();
                        let tile_recs: serde_json::Map<String, serde_json::Value> = tiles.iter().flatten().filter_map(|t| Some((t.tile.clone(), rec(lib.tiles.get(&u32::from_str_radix(&t.tile, 16).ok()?)?)))).collect();
                        let layers: Vec<_> = (0..8).map(|s| lib.layers(w, s)).collect();
                        shown.insert(format!("{w:08x}"), json!({"library": e.id, "colours": colours, "tiles": tiles, "tint_mask_layers": layers, "tile_records": tile_recs, "preset": rec(r), "base_materials": bases}));
                    }
                }
                if let Some(po) = po {
                    let dir = po.join("materials").join(sixng_core::game::index_stem(&a.id));
                    std::fs::create_dir_all(&dir)?;
                    let maps = |m: &std::collections::HashMap<u32, matlib::Record>| -> std::collections::BTreeMap<String, serde_json::Value> { m.iter().map(|(k, r)| (format!("{k:08x}"), rec(r))).collect() };
                    sixng_core::atomic::write(dir.join(format!("{:x}.json", e.off)), serde_json::to_vec(&json!({"id": e.id, "base_materials": maps(&lib.base), "presets": maps(&lib.presets)}))?)?;
                }
            }
            // weapon family files of packs.rpf name presets per component
            index.add_weapon_files(&ws);
            if index.libraries.is_empty() {
                anyhow::bail!("no material library (PSO with root struct 8419ab01) in {}", a.id);
            }
            std::fs::create_dir_all(ws.index_dir())?;
            sixng_core::atomic::write(matlib::path(&ws), serde_json::to_vec(&index)?)?;
            let resolved = |m: &std::collections::BTreeMap<String, String>| m.values().filter(|p| index.presets.contains_key(*p)).count();
            // weapon models by name: presets and base colours per sub-material (sRGB rrggbb)
            let res: Vec<regex::Regex> = weapon.iter().map(|w| regex::Regex::new(w)).collect::<std::result::Result<_, _>>()?;
            let mut named = std::collections::BTreeSet::new();
            let mut weapons = serde_json::Map::new();
            for n in ix.entries.iter().filter(|e| e.kind == Kind::RageObj).filter_map(|e| e.name.as_deref()) {
                let ps = index.weapon_presets(n);
                if ps.is_empty() {
                    continue;
                }
                named.insert(format!("{:08x}", sixng_core::meta7a::joaat(["_hi", "_mid"].iter().find_map(|s| n.strip_suffix(s)).unwrap_or(n))));
                if res.iter().any(|r| r.is_match(n)) {
                    let v: Vec<_> = ps.iter().map(|p| json!({"preset": p, "sub_materials": index.colours(p).map(|c| c.iter().map(|c| c.map(|c| format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2]))).collect::<Vec<_>>())})).collect();
                    weapons.insert(n.to_string(), json!(v));
                }
            }
            out.print(&json!({"ok": true, "archive": a.id, "libraries": index.libraries.iter().map(|l| json!({"id": l.0, "base_materials": l.1, "presets": l.2})).collect::<Vec<_>>(), "presets": index.presets.len(),
                "weapon_components": {"count": index.weapon_components.len(), "resolved": resolved(&index.weapon_components)}, "weapon_variants": {"count": index.weapon_variants.len(), "resolved": index.weapon_variants.values().filter(|p| index.presets.contains_key(&p.1)).count()},
                "weapon_models": {"count": index.weapon_models.len(), "named": named.len()},
                "weapon_finishes": {"models": index.model_finishes.len(), "variants": index.model_finishes.values().flatten().flat_map(|s| &s.1).map(|f| &f.0).collect::<std::collections::BTreeSet<_>>().len()}, "weapons": weapons,
                "defaults": {"error": defaults_err, "floats": index.defaults.floats.iter().map(|(k, v)| (pn(u32::from_str_radix(k, 16).unwrap_or(0)), json!(v))).collect::<serde_json::Map<_, _>>(),
                    "colours": index.defaults.colours.iter().map(|(k, v)| (pn(u32::from_str_radix(k, 16).unwrap_or(0)), json!(format!("{v:08x}")))).collect::<serde_json::Map<_, _>>(),
                    "ints": index.defaults.ints.iter().map(|(k, v)| (pn(u32::from_str_radix(k, 16).unwrap_or(0)), json!(v))).collect::<serde_json::Map<_, _>>(),
                    "textures": index.defaults.textures.iter().map(|(k, v)| (pn(u32::from_str_radix(k, 16).unwrap_or(0)), json!(v))).collect::<serde_json::Map<_, _>>()},
                "index": matlib::path(&ws), "shown": shown, "seconds": t.elapsed().as_secs_f64()}));
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
        Cmd::Vehicle { name, out: glb, config, no_wheels, list, list_parts, with, without, data, livery, archive, tex } => {
            let t = Instant::now();
            let dir = glb.as_ref().and_then(|g| g.parent().map(|p| p.to_path_buf())).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| PathBuf::from("."));
            let file = match data {
                Some(d) => d.clone(),
                None => {
                    sixng_core::vehicles::build(&ws, &dir)?;
                    dir.join("vehicles").join("vehicles.json")
                }
            };
            let all: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).map_err(|e| anyhow::anyhow!("read {}: {e}", file.display()))?)?;
            let want = name.to_ascii_lowercase();
            let Some(v) = all["vehicles"].as_array().into_iter().flatten().find(|v| v["model"].as_str().is_some_and(|m| m.eq_ignore_ascii_case(&want))) else {
                bail!("no vehicle named {name} in {}", file.display());
            };
            if *list {
                let cfgs: Vec<serde_json::Value> = v["configurations"].as_array().into_iter().flatten().enumerate().map(|(i, c)| json!({"config": i, "colour": c["colour"]["rgb"], "parts": c["parts"].as_array().map_or(0, |p| p.len()), "parts_key": c["parts_key"]})).collect();
                out.print(&json!({"ok": true, "vehicle": want, "configurations": cfgs}));
                return Ok(());
            }
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            if *list_parts {
                out.print(&json!({"ok": true, "vehicle": want, "parts": sixng_core::vehexport::list_parts(&ix, v)}));
                return Ok(());
            }
            let Some(glb) = glb else { bail!("give the output file with -o <file.glb>") };
            let parts = |l: &[String]| -> Result<Vec<u32>> { l.iter().map(|p| u32::from_str_radix(p.trim(), 16).map_err(|_| anyhow::anyhow!("{p} is not a part name (8 hex digits, see --list-parts)"))).collect() };
            let (with, without) = (parts(with)?, parts(without)?);
            let oa = OutArgs { out: dir.clone(), tex: tex.clone(), raw: false, force: true, no_embedded_textures: false, dry_run: false, body_shape: None, head_skin: 0, paint: None };
            let (decals, left) = match livery {
                Some(f) => {
                    let f = if f.as_os_str().is_empty() { sixng_core::vehlivery::saved_file(&ws.work, &want) } else { f.clone() };
                    if !f.is_file() {
                        bail!("no livery file {} (save one in the viewer's Liveries panel, or name a file after --livery)", f.display());
                    }
                    sixng_core::vehlivery::prepare(&ws.game, &f, &dir)?
                }
                None => Default::default(),
            };
            let mut r = sixng_core::vehexport::export_vehicle(&a, &ix, v, *config, &with, &without, !*no_wheels, &decals, glb, &oa.opts())?;
            if livery.is_some() {
                r["livery"]["left_out"] = json!(left);
            }
            r["seconds"] = t.elapsed().as_secs_f64().into();
            out.print(&json!({"ok": true, "result": r}));
        }
        Cmd::Vehicles { archive, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let metas: Vec<_> = ix.entries.iter().filter(|e| e.kind == Kind::Resource && e.sub == "7a").collect();
            let names = model_names(&ws);
            let t = Instant::now();
            let v = sixng_core::vehicles::export(&a, &metas, &names, po)?;
            // liveries: the stickers each brand places (<out>/vehicles/liveries.json)
            let liveries = match sixng_core::vehstickers::build(&ws, &names, po) {
                Ok(s) => s,
                Err(e) => json!({"error": format!("{e:#}")}),
            };
            out.print(&json!({"ok": true, "archive": a.id, "stats": v, "liveries": liveries, "seconds": t.elapsed().as_secs_f64()}));
        }
        Cmd::Stickers { out: po, textures, name, id } => {
            let t = Instant::now();
            if let Some(id) = id {
                let h = sixng_core::vehstickers::parse_texture_id(id).ok_or_else(|| anyhow::anyhow!("--id takes a 64-bit id in hex"))?;
                let (p, size) = sixng_core::vehstickers::sticker_png(&ws.game, h, &po.join("vehicles").join("stickers"))?;
                out.print(&json!({"ok": true, "png": p, "size": size, "seconds": t.elapsed().as_secs_f64()}));
            } else {
                let re = name.as_deref().map(|n| regex::RegexBuilder::new(n).case_insensitive(true).build()).transpose()?;
                let stats = sixng_core::vehstickers::export_stickers(&ws, po, *textures, re.as_ref())?;
                out.print(&json!({"ok": true, "stats": stats, "seconds": t.elapsed().as_secs_f64()}));
            }
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
                    sixng_core::atomic::write(dir.join(format!("{off}.json")), serde_json::to_vec_pretty(&doc)?)?;
                }
                sixng_core::atomic::write(dir.join("index.json"), serde_json::to_vec_pretty(&found.iter().map(|x| &x.0).collect::<Vec<_>>())?)?;
            }
            out.print(&json!({"ok": true, "archive": a.id, "count": found.len(), "dictionaries": found.iter().map(|x| &x.0).collect::<Vec<_>>()}));
        }
        Cmd::Daylight { weather, hour, var, rebuild } => {
            use sixng_core::daylight as dl;
            if *rebuild {
                let _ = std::fs::remove_file(dl::cache_path(&ws));
            }
            let t = dl::load_or_build(&ws)?;
            let hours = if hour.is_empty() { t.hours.clone() } else { hour.clone() };
            const VARS: [&str; 12] = ["light_dir_mult", "light_dir_col_r", "light_dir_col_g", "light_dir_col_b", "light_dir_sun_mult", "light_dir_moon_mult", "sky_scattering_sun_intensity_multiplier", "ambient_sky_intensity", "ambient_sky_color_mult_r", "ambient_sky_color_mult_g", "ambient_sky_color_mult_b", "postfx_exposure"];
            let mut rows = vec![];
            for w in t.weathers.iter().filter(|w| weather.is_empty() || weather.iter().any(|x| x.eq_ignore_ascii_case(&w.id))) {
                for &h in &hours {
                    let mut vals = serde_json::Map::new();
                    for k in VARS.iter().copied().chain(var.iter().map(String::as_str)) {
                        vals.insert(k.to_string(), json!(t.value(w, k, h)));
                    }
                    rows.push(json!({"weather": w.id, "hour": h, "vars": vals, "light": dl::light(&t, w, h)}));
                }
            }
            let weathers: Vec<_> = t.weathers.iter().map(|w| json!({"id": w.id, "type": w.kind, "cycle": w.cycle, "mods": w.mods, "cloud_cover": w.cloud_cover, "vars": w.vars.len()})).collect();
            out.print(&json!({"ok": true, "hours": t.hours, "ease": t.ease, "weathers": weathers, "rows": rows}));
        }
        Cmd::Timecycle { archive, name, out: po } => {
            let a = ws.resolve(archive)?;
            let ix = ws.load_index(&a)?;
            let entries: Vec<&sixng_core::scan::Entry> = ix.entries.iter().collect();
            let d = sixng_core::timecycle::decode(&a.path, &entries)?;
            let pat = name.clone().unwrap_or_else(|| "arcade".to_string());
            let re = regex::RegexBuilder::new(&pat).case_insensitive(true).build()?;
            let sel = sixng_core::timecycle::select(&d.modifiers, &re);
            let re_clouds = regex::RegexBuilder::new(&pat).case_insensitive(true).build()?;
            let clouds: Vec<serde_json::Value> = d.clouds.pointer("/CustomCloudGroupList").and_then(|v| v.as_array()).map(|g| g.iter().filter(|x| re_clouds.is_match(x["Name"].as_str().unwrap_or(""))).cloned().collect()).unwrap_or_default();
            let summary = json!({
                "archive": a.id,
                "modifiers": d.modifiers.len(),
                "cycles": d.cycles.iter().map(|c| c["name"].clone()).collect::<Vec<_>>(),
                "hours": d.time["samples"].as_array().map(|s| s.iter().map(|x| x["hour"].clone()).collect::<Vec<_>>()),
                "files": d.files,
                "listed_missing": d.missing,
            });
            let selected = json!({"pattern": pat, "modifiers": sel, "custom_clouds": clouds});
            if let Some(po) = po {
                let dir = po.join("timecycle");
                std::fs::create_dir_all(&dir)?;
                let w = |f: &str, v: &serde_json::Value| -> Result<()> { Ok(sixng_core::atomic::write(dir.join(f), serde_json::to_vec_pretty(v)?)?) };
                w("modifiers.json", &json!(d.modifiers))?;
                w("cycles.json", &json!({"hours": summary["hours"], "time_samples": d.time, "cycles": d.cycles}))?;
                w("time_samples.json", &d.time)?;
                w("weather.json", &d.weather)?;
                w("custom_clouds.json", &d.clouds)?;
                w("variables.json", &sixng_core::timecycle::variables(&d))?;
                w("files.json", &summary)?;
                w("selected.json", &selected)?;
            }
            out.print(&json!({"ok": true, "summary": summary, "selected": selected}));
        }
        Cmd::Heightmap { archive, map, preview, check, out: po } => {
            use sixng_core::heightmap as hm;
            let a = ws.resolve(archive)?;
            let maps = hm::load_all(&ws, archive)?;
            if maps.is_empty() {
                anyhow::bail!("no MHIU height map in {}", a.id);
            }
            let pick = match map {
                Some(i) if *i < maps.len() => *i,
                Some(i) => anyhow::bail!("--map {i}: only {} height maps", maps.len()),
                None => hm::best(&maps).unwrap(),
            };
            let desc = |id: &str, m: &hm::HeightMap| {
                json!({
                    "entry": id, "version": m.version, "width": m.width, "height": m.height,
                    "min": m.min, "max": m.max, "metres_per_pixel": m.cell(),
                    "metres_per_step": m.step(), "fill": m.fill, "sea_level_m": m.z(m.fill),
                    "unknown_header_bytes": format!("{:02x}{:02x}{:02x}", m.unknown[0], m.unknown[1], m.unknown[2]),
                    "stored_samples": m.stored,
                })
            };
            let list: Vec<serde_json::Value> = maps.iter().map(|(id, m)| desc(id, m)).collect();
            let (id, m) = &maps[pick];
            let mut res = json!({"archive": a.id, "maps": list, "selected": pick});
            if let Some(dir) = check {
                res["check"] = heightmap_check(m, dir)?;
            }
            if let Some(po) = po {
                let dir = po.join("heightmap");
                std::fs::create_dir_all(&dir)?;
                hm::write_png16(m, &dir.join("heightmap.png"))?;
                if *preview {
                    hm::write_hillshade(m, &dir.join("hillshade.png"))?;
                }
                let mut j = desc(id, m);
                j["image"] = json!("heightmap.png");
                j["image_rows"] = json!("north to south: pixel (c, r) centre = (min.x + (c + 0.5) * mpp.x, max.y - (r + 0.5) * mpp.y)");
                j["height_from_png"] = json!(format!("z = {} + png / 65535 * {}", m.min[2], m.max[2] - m.min[2]));
                j["height_scale_m"] = json!(m.max[2] - m.min[2]);
                if let Some(c) = res.get("check") {
                    j["check"] = c.clone();
                }
                sixng_core::atomic::write(dir.join("heightmap.json"), serde_json::to_vec_pretty(&j)?)?;
                res["out"] = json!(dir);
            }
            out.print(&json!({"ok": true, "result": res}));
        }
        Cmd::Water { fog_px, height_px, shore_px, height_tiles, inland, pools, detail, out: po } => {
            for (n, v, max) in [("--fog-px", *fog_px, 128), ("--height-px", *height_px, 512), ("--shore-px", *shore_px, 256)] {
                if v == 0 || max % v != 0 {
                    anyhow::bail!("{n} {v}: must divide {max}");
                }
            }
            let o = sixng_core::water::Options { fog_px: *fog_px, height_px: *height_px, shore_px: *shore_px, height_tiles: *height_tiles, inland: *inland, pools: *pools, detail: *detail };
            let r = sixng_core::water::export(&ws, po.as_deref(), &o)?;
            out.print(&json!({"ok": true, "result": r}));
        }
        Cmd::SectorGrid { archive, entry, cell_m, out: po } => {
            use sixng_core::sectorgrid as sg;
            let mut grids = sg::load_all(&ws, archive)?;
            if grids.is_empty() {
                anyhow::bail!("no sector grid (PSO struct {:08x}) in {archive}", sg::RECORD);
            }
            let pick = match entry {
                Some(id) => grids.iter().position(|(i, _)| i == id || i.ends_with(&format!("@{}", id.trim_start_matches("0x")))).ok_or_else(|| anyhow!("--entry {id}: not a sector grid"))?,
                None => sg::largest(&grids).unwrap(),
            };
            let desc = |id: &str, g: &sg::SectorGrid| {
                json!({
                    "entry": id, "kind": g.kind(), "cell_m": g.cell_m, "records": g.sectors.len(),
                    "cells": g.cells().len(), "bundles": g.groups(), "map_groups": g.map_groups(),
                    "cell_range": g.cell_range().map(|r| json!({"min_x": r[0], "min_y": r[1], "max_x": r[2], "max_y": r[3]})),
                    "world_bounds_m": g.world_bounds().map(|(lo, hi)| json!({"min": lo, "max": hi})),
                    "nonzero_zero_field": g.sectors.iter().filter(|s| s.zero != 0).count(),
                    "nonzero_hash64": g.sectors.iter().filter(|s| s.hash64 != 0).count(),
                })
            };
            if let Some(c) = cell_m {
                grids[pick].1.cell_m = *c;
            }
            let (id, g) = &grids[pick];
            let mut summary = json!({
                "grids": grids.iter().map(|(i, g)| desc(i, g)).collect::<Vec<_>>(), "selected": pick,
                "cell_rule": "cell = floor(world / cell_m); cell (x, y) covers x*cell_m .. (x+1)*cell_m. cell_m is measured, not stored (see sectorgrid.rs)",
            });
            if let Some(po) = po {
                let dir = po.join("sector_grid");
                std::fs::create_dir_all(&dir)?;
                sixng_core::atomic::write(dir.join("sector_grid.csv"), sg::to_csv(g))?;
                sg::write_mask_png(g, &dir.join("sector_grid.png"))?;
                let mut j = desc(id, g);
                j["cell_rule"] = summary["cell_rule"].clone();
                j["image"] = json!("sector_grid.png");
                j["image_rows"] = json!("north to south: pixel (c, r) = cell (min_x + c, max_y - r)");
                j["sectors"] = serde_json::to_value(&g.sectors)?;
                sixng_core::atomic::write(dir.join("sector_grid.json"), serde_json::to_vec(&j)?)?;
                summary["out"] = json!(dir);
            }
            out.print(&json!({"ok": true, "result": summary}));
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
                    sixng_core::atomic::write(d.join(format!("{file}.ttf")), sixng_core::gfxfont::to_ttf(f))?;
                    sixng_core::atomic::write(d.join(format!("{file}.typeface.json")), serde_json::to_vec(&sixng_core::gfxfont::to_typeface(f))?)?;
                }
                let sample: String = f.glyphs.iter().filter_map(|g| char::from_u32(g.code as u32)).take(48).collect();
                list.push(json!({"name": f.name, "bold": f.bold, "italic": f.italic, "file": file, "glyphs": f.glyphs.len(), "movie": id, "movies": copies, "chars": sample}));
            }
            if let Some(d) = &dir {
                sixng_core::atomic::write(d.join("fonts.json"), serde_json::to_vec_pretty(&list)?)?;
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
            sixng_core::atomic::write(&file, serde_json::to_vec_pretty(&json!({
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
            // per-model variation lists (PSO files holding struct a5ea6310)
            let mut lists = std::collections::HashMap::new();
            for e in ix.entries.iter().filter(|e| e.kind == Kind::Pso) {
                let Ok(b) = sixng_core::scan::read_entry(&a.path, e) else { continue };
                let tag = 0xa5ea_6310u32;
                if !b.windows(4).any(|w| w == tag.to_le_bytes() || w == tag.to_be_bytes()) {
                    continue;
                }
                if let Some(p) = sixng_core::pso::parse(&b) {
                    lists.extend(sixng_core::wardrobe::variation_lists(&p.to_json()));
                }
            }
            // dirt / blood presets from common.rpf's graphics definitions, if indexed
            let dirt = ws
                .resolve("common")
                .ok()
                .and_then(|ca| {
                    let cix = ws.load_index(&ca).ok()?;
                    cix.entries.iter().filter(|e| e.kind == Kind::Xml).find_map(|e| {
                        let b = sixng_core::scan::read_entry(&ca.path, e).ok()?;
                        let s = String::from_utf8_lossy(&b);
                        s.contains("<CGraphicsDefinitionsData>").then(|| sixng_core::wardrobe::dirt_presets(&s))
                    })
                })
                .unwrap_or_default();
            // layered-material presets (`sixng materials`), if built
            let presets = sixng_core::matlib::Index::load(&ws);
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
                        let mut tints: Vec<String> = v.parts.iter().filter_map(|p| Some(format!("{}={}", p.drawable.as_deref()?, p.tint.as_ref()?.spec))).collect();
                        tints.dedup();
                        // per drawable: the preset and its sub-materials' base colours (sRGB rrggbb, null = none)
                        let pc: serde_json::Map<String, serde_json::Value> = v
                            .parts
                            .iter()
                            .filter_map(|p| {
                                let (d, pr) = (p.drawable.as_deref()?, p.preset.as_deref()?);
                                let cols = presets.as_ref()?.colours(pr)?;
                                Some((d.to_string(), json!({"preset": pr, "sub_materials": cols.iter().map(|c| c.map(|c| format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2]))).collect::<Vec<_>>()})))
                            })
                            .collect();
                        let dv: serde_json::Map<String, serde_json::Value> = v.parts.iter().flat_map(|p| p.tags.values().flatten()).filter_map(|w| dirt.get(w).map(|d| (w.clone(), d.clone()))).collect();
                        let listed = u32::from_str_radix(&v.id, 16).ok().and_then(|i| lists.get(&i)).map(|&(m, c, s)| json!({"model": format!("{m:08x}"), "outfit_type": format!("{c:08x}"), "scale": s}));
                        json!({"id": v.id, "scale": v.scale, "listed": listed, "tags": v.tags, "parts": d.iter().map(|n| format!("^{n}$")).collect::<Vec<_>>().join(","), "tex_remap": remap.join(","), "tints": tints.join(","), "dirt": dv, "preset_colours": pc, "items": v.parts})
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
            let aix = sixng_core::audio::load_index(&ws.work, &a.id).ok();
            let banks: Vec<(u64, std::collections::HashSet<u32>)> = aix.as_ref().map(|ix| ix.banks.iter().map(|b| (b.off, b.streams.iter().map(|s| s.id).collect())).collect()).unwrap_or_default();
            let sizes: Vec<(u64, u64)> = aix.as_ref().map(|ix| ix.banks.iter().map(|b| (b.off, b.size)).collect()).unwrap_or_default();
            let (ok, bad, containers, list, objects) = sixng_core::audiometa::export(&a, &sel, &banks, &sizes, po)?;
            // which container (bank name hash) each bank holds, for `sixng audio`
            let named = if containers.is_empty() {
                None
            } else {
                sixng_core::audiometa::save_containers(&ws.work, &a.id, &containers)?;
                sixng_core::audiometa::save_groups(&ws.work, &a.id, &sixng_core::audiometa::bank_groups(&list))?;
                sixng_core::audiometa::save_objects(&ws.work, &a.id, &objects)?;
                let c = sixng_core::audiometa::bank_container(&containers);
                // bank names into the name dictionary (needs `sixng names` first)
                let streams: std::collections::BTreeMap<u64, Vec<u32>> = aix.as_ref().map(|ix| ix.banks.iter().map(|b| (b.off, b.streams.iter().map(|s| s.id).collect())).collect()).unwrap_or_default();
                let names = match sixng_core::names::add_bank_names(&ws.work, &a.id, &c, &streams) {
                    Ok(r) => r,
                    Err(e) => json!({"error": e.to_string()}),
                };
                Some(json!({"banks": c.len(), "from_sounds": containers.values().filter(|m| m.values().any(|n| *n > 0)).count(), "bank_list_entries": list.len(), "names": names, "file": sixng_core::audiometa::containers_path(&ws.work, &a.id)}))
            };
            out.print(&json!({"ok": true, "archive": a.id, "scanned": sel.len(), "decoded": ok, "failed": bad, "linked_to_banks": !banks.is_empty(), "bank_containers": named, "seconds": t.elapsed().as_secs_f64(), "out": po.join("audio_meta")}));
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
            // bank names: each bank's container hash (from `audio-meta`) and
            // its name where the dictionary has one
            let containers = sixng_core::audiometa::load_containers(&ws.work, &a.id).map(|c| sixng_core::audiometa::bank_container(&c)).unwrap_or_default();
            let bank_name = |off: u64| containers.get(&off).map(|h| (format!("{h:08x}"), sixng_core::names::lookup(*h, sixng_core::names::Ctx::Audio)));
            let named = ix.banks.iter().filter(|b| bank_name(b.off).is_some_and(|(_, n)| n.is_some())).count();
            // stream names recovered by `audio-meta` / `names`
            let stream_names = sixng_core::names::all_stream_names(&ws.work, &ix);
            let list: Vec<_> = ix
                .banks
                .iter()
                .filter(|b| b.streams.iter().any(|s| s.layout != "none" && s.seconds() >= *min_seconds))
                .take(if *limit == 0 { usize::MAX } else { *limit })
                .map(|b| {
                    let (container, name) = bank_name(b.off).map_or((None, None), |(c, n)| (Some(c), n));
                    json!({"bank": format!("{:x}", b.off), "container": container, "name": name, "size": b.size, "seconds": b.seconds(), "streams": b.streams.iter().enumerate().filter(|(_, s)| s.layout != "none" && s.seconds() >= *min_seconds).map(|(i, s)| json!({"index": i, "id": format!("{:08x}", s.id), "name": stream_names.get(&b.off).and_then(|m| m.get(&s.id)), "name_from": if b.names.contains_key(&s.id) { Some("stored") } else { stream_names.get(&b.off).and_then(|m| m.get(&s.id)).map(|_| "search") }, "hash": stream_names.get(&b.off).and_then(|m| m.get(&s.id)).map(|n| format!("{:08x}", sixng_core::meta7a::joaat(n))), "seconds": s.seconds(), "samples": s.samples, "rate": s.rate, "codec": s.codec, "channels": s.channels, "layout": s.layout})).collect::<Vec<_>>()})
                })
                .collect();
            out.print(&json!({"ok": true, "archive": a.id, "index": audio::index_path(&ws.work, &a.id), "banks": ix.banks.len(), "banks_with_container": ix.banks.iter().filter(|b| containers.contains_key(&b.off)).count(), "banks_named": named, "streams_named": stream_names.values().map(|m| m.len()).sum::<usize>(), "streams_by_layout": layouts, "codec_bytes": codecs, "total_seconds": secs, "list": list, "seconds": t.elapsed().as_secs_f64(), "note": ix.note}));
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
                    sixng_core::atomic::write(&p, &bytes)?;
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

type TocVisit<'a> = dyn FnMut(&mut dyn sixng_core::rpf9toc::ArchiveCipher, &mut std::fs::File, u64, &sixng_core::rpf9toc::Toc, &[sixng_core::rpf9toc::FileEntry]) -> Result<()> + 'a;

/// Visit the table of contents at `base` and those of the archives stored
/// inside it. An archive without a known entry column is visited with no
/// entries.
fn toc_walk(cipher: &mut dyn sixng_core::rpf9toc::ArchiveCipher, f: &mut std::fs::File, base: u64, visit: &mut TocVisit) -> Result<()> {
    use sixng_core::rpf9toc;
    let toc = rpf9toc::open_toc(cipher, f, base).map_err(|e| anyhow!("table of contents at {base:#x}: {e:#}"))?;
    let entries = if toc.has_entries() { toc.files()? } else { Vec::new() };
    visit(cipher, f, base, &toc, &entries)?;
    for off in rpf9toc::nested_archives(f, base, &entries) {
        toc_walk(cipher, f, base + off, visit)?;
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

/// Compares a height map with the map bake's 8 m collision top-surface grid.
fn heightmap_check(m: &sixng_core::heightmap::HeightMap, dir: &std::path::Path) -> Result<serde_json::Value> {
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("meta.json"))?)?;
    let hb = std::fs::read(dir.join("heights_8m.bin"))?;
    let h8: Vec<f32> = hb.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    let cell_m = meta["cell_m"].as_f64().unwrap_or(2.0) as f32;
    let cell = cell_m * meta["coarse_cells"].as_f64().unwrap_or(4.0) as f32;
    let (x0, y1) = (meta["x0"].as_f64().unwrap_or(0.0) as f32, meta["y1"].as_f64().unwrap_or(0.0) as f32);
    let w = (meta["w"].as_u64().unwrap_or(0) as f32 * cell_m / cell) as usize;
    let h = if w > 0 { h8.len() / w } else { 0 };
    let [cx, cy] = m.cell();
    let mut d = Vec::new();
    let mut sea_vs_land = 0usize;
    for r in 0..m.height {
        for c in 0..m.width {
            let x = m.min[0] + (c as f32 + 0.5) * cx;
            let y = m.min[1] + (r as f32 + 0.5) * cy;
            let (gc, gr) = (((x - x0) / cell).floor(), ((y1 - y) / cell).floor());
            if gc < 0.0 || gr < 0.0 || gc as usize >= w || gr as usize >= h {
                continue;
            }
            let rz = h8[gr as usize * w + gc as usize];
            if !rz.is_finite() || rz < -1000.0 {
                continue;
            }
            let v = m.samples[r * m.width + c];
            if v <= m.fill {
                if rz > 5.0 {
                    sea_vs_land += 1;
                }
                continue;
            }
            d.push(m.z(v) - rz);
        }
    }
    if d.is_empty() {
        anyhow::bail!("no overlap with the map bake grid");
    }
    let n = d.len() as f64;
    let mean = d.iter().map(|&x| x as f64).sum::<f64>() / n;
    let mut a: Vec<f32> = d.iter().map(|x| x.abs()).collect();
    d.sort_by(|p, q| p.total_cmp(q));
    a.sort_by(|p, q| p.total_cmp(q));
    let q = |v: &Vec<f32>, f: f64| v[((v.len() - 1) as f64 * f) as usize];
    let within = |t: f32| a.iter().filter(|&&x| x <= t).count() as f64 / n;
    Ok(json!({
        "reference": "collision top surface, 8 m grid (map bake)",
        "land_samples_compared": d.len(),
        "diff": "height map minus collision top, metres",
        "mean_diff_m": mean, "median_diff_m": q(&d, 0.5), "p10_diff_m": q(&d, 0.1), "p90_diff_m": q(&d, 0.9),
        "median_abs_m": q(&a, 0.5), "p90_abs_m": q(&a, 0.9),
        "within_3m": within(3.0), "within_10m": within(10.0),
        "sea_samples_over_land_gt_5m": sea_vs_land,
    }))
}
