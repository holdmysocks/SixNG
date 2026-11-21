<p align="center"><img src="assets/logo/sixng_banner.png" alt="SixNG" width="640"></p>

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

# SixNG Extractor

An asset extractor for your own GTA VI install (Xbox Series build, RPF9
archives). It is meant for personal modding.

There is one Rust core library with two front ends:

- `sixng`: a CLI with JSON output, filters, categories and resumable runs.
- `sixng-gui`: a desktop app to open an install, browse and search it,
  preview items and extract a selection.

> **Game data never goes in git.** Indexes, extracted output and scratch
> files all go under `work/` (or any `--out` you choose). `work/` and every
> `*.png/*.dds/*.glb` file are in `.gitignore`.

| Shaded | UV checker + wireframe |
|---|---|
| ![Model viewer, shaded](docs/images/model_viewer.png) | ![Model viewer, UV checker](docs/images/model_viewer_uv.png) |

*Screenshots show the app with a generated demo scene (crates, a barrel and a ground plane), not game content.*

> **BCPack: BC1 decodes, the other formats don't yet.**
> BCPack is the Xbox Series hardware texture compression, with one codec
> per BC format and no public specification. About 12 % of the game's
> textures have their full-size image stored in BCPack. Almost all of them
> are BC1, which SixNG decodes, so they export at full size. About
> 0.8 % of textures have their full-size image in the other BCPack
> formats (BC3/BC4/BC5/BC6H/BC7), which are not decoded.
> SixNG uses the largest smaller mip that is stored normally when there is
> one (0.4 % of textures). The rest (0.4 %) get a flat fallback colour,
> flagged as `texture_fallback` in `model.json`. Please don't open issues
> asking for the other formats: it is a known gap, and requests won't
> speed it up.

## Status

| Area | State |
|------|-------|
| RPF9 header | **works**: parsed and shown by `sixng info` |
| RPF9 TOC / file names | **not read**: the TOC is encrypted and SixNG does not decrypt it. Contents are found by carving the zlib streams instead, with names recovered from inside resources. |
| Listing / search | **works**: carving index plus name recovery. On `levels.rpf`: 523,552 entries (1.12 M zlib streams, 98.3 % byte coverage), 227,766 named (drawables, fragments, texture dictionaries). A full index takes about 3 min. |
| Textures in texture dictionaries and models | **works** for BC1/BC2/BC3/BC4/BC5/BC6H/BC7: Xbox tiling removed, PNG and DDS output, checked visually. RGBA8 tiling is unverified. Only slice 0 of arrays is exported. BCPack-only textures get a flat fallback colour, flagged `texture_fallback` in `model.json`. Since 2026-11-21, small textures with mips whose whole chain fits in one 4 KiB mip-tail block (32×32 BC3 HUD icons and the like) decode from that block; before, they came out blank. Which materials are affected, and how much of the map they cover: `sixng texture-gaps` → `work/texture_gaps.json`. |
| Bulk texture stores (`data/ttdb/*.ttpf`) | **works for 98.4 % of all 352 K textures** by the chunk-table check (mip 0 outside undecodable BCPack chunks; 86.8 % before BC1 BCPack decoding): maps 99.2 %, props 99.5 %, peds 99.2 %, interiors 99.7 %, vehicles 98.9 %, terrain 97.4 %, ui 91.1 %. The layout is AMD Gfx10: tile mode 0 = `SW_64KB_D`, modes 1–3 = `SW_4KB_S`, with the addrlib mip tail. That covers all sizes, including the car body/detail textures and the small ones. Each mip was checked against the next one downsampled, and the results were viewed on UI images, atlases, billboards, vehicle detail maps and normal atlases. **BCPack** (the Xbox Series hardware texture codec, one codec id per BC format, no zlib copies): 12.4 % of textures have mip 0 in BCPack chunks. **BC1 BCPack chunks (codec 0x04) decode** since 2026-11-21, which brings 11.6 % of all textures to full size; if a chunk does not decode, the texture falls back to the largest mip outside BCPack as before. The other formats (BC3/BC4/BC5/BC6H/BC7) are not decoded: 0.4 % of textures are **exported at a lower mip** that sits in zlib chunks (`mip_used` in `model.json`), **0.4 % stay undecoded**, plus 4-/2-byte formats (1 %). `sixng ttpf <store>` lists a store or decodes textures by hash; `--all` decodes a whole store in parallel and writes `index.json` with each texture marked complete, `partial` (part of the data in BCPack chunks other than BC1, so part of the image is blank or noise) or `bcpack_only` (no PNG written). Store names must be unique: use the full id (`data/ttdb/ui/ttdb_ui.ttpf`) when a short one matches several. **Texture arrays** (2–64 slices) and **cube maps** (6 faces, tile modes 9–12) decode since 2026-11-20 (earlier they came out as a mosaic or not at all). Models use slice 0 / face 0; `sixng ttpf <store> --hash <h> --all-slices` writes every slice. |
| Models (drawables `.#dr`, fragments/vehicles `.#ft`) | **works (geometry, LODs, skeleton, skinning); partial (materials)**: one GLB per LOD (`model.glb`, `model_lodN.glb`), with triangle meshes, normals and every UV set (UV0–UV3), one node per geometry, Y-up, metres. Fragments carry their **skeleton as glTF joints with correct pivots** and a **skin** (JOINTS_0/WEIGHTS_0 from the game's bone streams; u16 joints for skeletons over 255 bones, fixed 2026-11-20: earlier vehicle exports have wrong bones on interior, lamp, window and bumper geometries and should be re-exported). Checked in a glTF viewer (three.js) on `phoenix_mid` and three map pieces: shaded, normals, UV checker, joint markers, textured. **Materials link `ttpf` textures** (base colour and normal map, picked from the parameter hash and format) where they decode: on the map samples about half the materials get a base colour and a normal map. **Shader and parameter names** resolve since 2026-11-21 (all 344 shaders, about 75 % of parameter uses; see `sixng names`); texture names are still 64-bit hashes. **Tangents** (`TANGENT`, decoded from the game's tangent stream) are written where the mesh has one. Vertex colours are raw. Both models that had no geometry in the placement test now decode: a second small system stream is now detected, which recovered geometry for about 10 % of multi-stream models that had none. **Cables and pool/fountain water** (float-position vertex blobs) decode since 2026-11-20; cables are widened into crossed 2 cm ribbons because the game widens them in a shader. Since the same date, a stricter system-stream rule recovers **fragment props** and some **64 m `_hd` ground tiles** whose graphics page was being read as system data. In a BeamNG test box this took the models with no geometry from 74 to 0. Fragments get a real bbox in `model.json` (`drawable.bounds`; `bounds_raw` is meaningless on fragments). Occluder and water-vegetation LOD buffers are still not decoded. **LOD levels** come from the main drawable's LOD group since 2026-11-20. Earlier exports ranked every model array, so fragment props (streetlights, signs) got a small base plate or bracket as `model.glb` and buildings got trim strips as `model_lodN.glb`; re-export those. Embedded-drawable models now go to `model_parts.glb`. **Two-layer blend materials** (walls, ground pieces) get a `multilayer` block: base/layer-2 colour, normal and data textures, UV set and weight channel (COLOR0.a). |
| Vehicles | **partial**: the body exports with pivots, skin and LODs; `^phoenix` gives 248 parts with 0 errors, including the `_dmg_mod_*` damage parts and mod-kit parts. **Wheels are shared models** (`rim_*`, `tyre_*`) placed at `model.json` → `wheel_mounts`. A test fit in all four arches looks right. Since 2026-11-21 `sixng vehicles` gives the rim and tyre of each spawn configuration and the per-model `wheelScale` / `wheelScaleRear`. The model viewer fits the configuration's rim and tyre and sizes them as 1.625 × `wheelScale` (front and rear separately). The factor is fitted by eye on phoenix (0.37 at wheelScale 0.2277); mesa and ridgway look right with it too. It is not the game's verified rule. **Textures**: 33 of `phoenix_mid`'s texture hashes now decode: interior, lights, trim, the shared detail masks (dirt, grime, scratches, decals) and normal atlases. **Paint**: the body's colour slot is an engine-default white texture; the colour is applied at runtime from the vehicle tables (see Car paint below). Handling: `sixng handling`. **Vehicle data** (2026-11-21): `sixng vehicles packs --out <dir>` writes `<out>/vehicles/vehicles.json` from the game-data meta resource, covering 373 models (305 named). Each has `model_info` (vehicles.meta-like, about 50 GTA V names resolve: `wheelScale`, `wheelScaleRear`, `wheelType`, `handlingId`, `txdName`, `lodDistances` …), `configurations` (4,063: body colour plus the part models, with one `rim_*` and one `tyre_*` in 3,339), `palettes` (2,377 weighted paint-colour lists of 1 to 400 entries; colours repeat, e.g. phoenix's 400 entries hold 10 distinct colours, so repetition reads as the draw weight) and weighted `part_groups`. |
| Textures in unusual formats (R16/R32 float, …) | **skipped with a warning**. The texture JSON is still written; DDS export works where the format is known. |
| Map placements | **works**: `sixng placements` decodes the map files (self-describing `0x7a` resources, the GTA V `.ymap` equivalent): **2,249,490 entities in 30,785 files** in `levels.rpf`, 95 % resolved to a named, exportable model. The count rose from 1.47 M after the resource stream-split fix. Position, rotation, scale, flags and GUID are decoded. **LOD hierarchy**: each entity has `lod {level (HD/LOD/SLOD1–4/ORPHAN_HD), parent_index, parent_instance, num_children, lod_dist, child_lod_dist, priority}`. After `sixng lod-links` it also has `archetype_lod_dist`, `hd_texture_dist`, `effective_lod_dist` (lod_dist -1 means the archetype's) and a resolved `parent`: an LOD entity in another map file, or an instance of a `*_batchlod` prop batch (99.0 % of 1.05 M batch-linked HD entities and 99.9 % of 215 K entity-linked ones resolve, each with a method and confidence). **Interiors**: each file has a `space`. `world` files hold world positions. `interior` files (961 interior archetype files, 540 K entities) hold interior contents in interior-local space, which the world places only through interior instances (`sixng interiors`). `group` files (6.3 K, 40 K entities) are group-local. Interior entities carry `interior {interior, room, entity_set}`; `sixng interior-entities` also writes them to `work\index\interior_entities.jsonl`. Other fields are kept by hash. `data.rpf` and `packs.rpf` contain no map files. Checked visually: two map files (a tower block with its base, walkway and planting, and an industrial site) rendered from their exported models line up. |
| Collision (physics bounds) | **works (geometry); partial (surface names)**: `sixng collision [levels]` decodes the unnamed bound resources in `levels.rpf` (the GTA V `.ybn` equivalent: composite → geometry bounds with triangles, boxes, spheres, capsules, cylinders) to one GLB per file in world coordinates, **split into one primitive per surface id**, plus `index.jsonl` (bbox, polygon counts, materials, primitive list) and `surfaces.json`. Surface ids are 15-bit hashes of the names in the game's material list (`ROAD_TARMAC`, `TRACK_GRASS_SHORT`, `METAL_RAILING` …); the hash function is unknown, so names are not attached yet. Checked visually: a building with window openings, a car park with railings and pillars, and terrain with coherent surface patches. A sample of 400 entries gave 353 bounds; the whole archive is about 87 K files, ~15 GB of GLB. |
| Lights | **works (position, colour, direction); partial (other attributes)**: `sixng lights [levels]` follows the light extensions on archetypes and entities (GTA V `CExtensionDefLightEffect` / `CLightAttrDef` equivalent) through the meta pool references and writes `lights/levels/sets.jsonl` (**136,466 light sets, 274,819 lights** with local position, RGB colour, direction/tangent and every other field by hash) and `world.jsonl` (**624,184 placed lights** in world space, transformed by each entity that uses the archetype). Checked visually: the whole-map plot reads like a night satellite view, and a 1 km city cell shows lamp rows along both edges of a bridge and the arterials, and grids over car parks. The light fields are now named from their own member hashes (case-sensitive joaat of the GTA V `CLightAttr` names): `intensity`, `falloff` (range), `falloffExponent`, cone angles, corona, volumetric and fade distances, `timeFlags`; 774,847 placed lights including interiors. ~1 min, ~0.7 GB. |
| Navmesh, road surface | **works (navmesh, road polygons, lane graph)**: `sixng navmesh [--roads-only]` decodes the navmesh tiles in `navmeshes.rpf` (GTA V `.ynv` equivalent, 128 m tiles) to GLB in world coordinates, one primitive per polygon flag; **flag 0x2000 = road surface** (checked visually). `sixng paths levels` writes the road/path-node graph (GTA V `.ynd` equivalent): 63,808 nodes with speed limit, street hash, **named node flags** (34 of 51: NoGps, Junction, InTunnel, Highway slip / exit, Stop, Yield, Bridge, WaterNode …), road class and density; 145,518 links with length, lane counts per direction and named flags (GpsCanGoBothWays, LeadsToDeadEnd / LeadsFromDeadEnd from the game's names; unpaved / track, two-way and no-ambient-traffic (bit 6) inferred); and **1,546 junctions** with entrances, turn paths and, for 369 signalised ones, **traffic-light phases** (duration and the placed traffic-light props, matched by entity hash for 99.6 %). |
| Audio (`audio.rpf`) | **works (Opus streams); raw only (music, other codecs)**: `sixng audio` finds the sound banks (`ADAT` containers, the GTA V `.awc` equivalent) by scanning the archive (~2.5 min, index ~30 MB): **7,160 banks, 1.6 M streams, about 1,300 hours**. Opus streams (99.7 %) export as **Ogg Opus** (lossless remux) or **WAV** (decoded with libopus, 16-bit, 48 kHz) and play in the GUI's **Audio** page. Multichannel music (3,008 streams, 512 KiB blocks with an undecoded header) and 1,550 streams with another codec byte export only as the raw bank (`.awc`). Stream names are 29-bit hashes and are **not recovered**; streams are listed by bank offset and stream id. The Opus encoder delay is taken as the default 312 samples and the end is trimmed to the stream's sample count. |
| Video (`movies.rpf`) | **works (carved as-is; frames not decoded)** | Since 2026-11-21. The videos are stored uncompressed, so the stream index finds none; `sixng movies movies --out <dir>` finds them by their headers instead (about 7 s): **515 Bink 2 files** (the archive header also says 515), 8.83 GB, about 236 minutes, all at 30 fps. 246 are 1280×720, 130 are 1920×1080, 67 are portrait 720×1280, 14 are 2560×1440, and the rest are square or wide (512×512, 1024×1024 …). 196 have audio (48 kHz). `<out>/movies/movies.json` lists offset, size, resolution, frames, duration, keyframes and audio tracks for each; every frame table checks out. `--extract` writes the `.bk2` files as they are (all of them: 8.8 GB), and `--only <hex offsets>` picks some. They play in any player that supports Bink 2. Names are not recovered, so files are named by offset and size. |
| UI graphics (menus, HUD, phone, loading screens) | **partial (2026-11-21)** | Two sources. (1) **Named texture dictionaries in `data.rpf`** (75): 36 loading screens, `hud`, `blip`, `map_scripted_movie`, `credits`, `phone`, `controller_test` and the vehicle gauge sets (`vit_*`: dashboards, motorcycle displays). `sixng extract data --rtype txd -o <dir>` writes all 880 textures (108 MB) with 0 errors; texture names are placeholders (`hud_i8d3`). (2) **UI texture stores** (`data/ttdb/ui/ttdb_ui.ttpf` plus small ones in the `core_restofworld` packs): 10,013 textures, mostly sprite atlases (shop and brand logos, tattoo art, hair and beard icons, wheel icons, character portraits, in-game arcade and website art). `sixng ttpf data/ttdb/ui/ttdb_ui.ttpf --all --out <dir>` takes about 10 s: 9,012 complete, 345 partial, 656 only in BCPack chunks (mostly BC7). They have one mip and are named by 64-bit hash only. Font glyph atlases are in `data/ttdb/graphics/ttdb_graphics.ttpf`. **Minimap / pause map**: no tile set found in these stores or dictionaries; the map may be drawn from world data at runtime (not established). Menu layouts (which sprite goes where) are not decoded. |
| XML / text metadata | **works**: exported as is. |
| PSO / RBF metadata | **PSO decoded (2026-11-21); RBF raw only.** PSO files are the plain-file form of the self-describing meta format: schema section `LSCH` (struct names are 64-bit hashes, field names 32-bit case-sensitive joaat, the same type codes as the `0x7a` resources plus maps, count-first arrays, inline arrays, bit sets and tagged values). `sixng pso data --out <dir>` decodes 2,225 of the 2,281 PSO files in `data.rpf` to JSON (`<out>/pso/data/<offset>.json`). The other 56 use the older GTA V layout (PMAP/PSCH) and are not decoded yet. Field names resolve where the dictionary knows them (GTA V handling names such as `fMass` match exactly); most stay hashes. |

## Setup and build

You need Rust 1.80+ (stable) on Windows, and **CMake** (for libopus, used
by audio decoding; `.cargo/config.toml` sets the CMake policy libopus
needs with CMake 4).

```powershell
cd SixNG
cargo build --release
```

This produces `target\release\sixng.exe` (CLI) and
`target\release\sixng-gui.exe` (GUI).

**macOS** (Apple Silicon, tested on macOS with Rust 1.100): the same
`cargo build --release` works once CMake is installed (`brew install cmake`).
It produces `target/release/sixng` and `target/release/sixng-gui`; the GUI
renders through Metal. `scripts/macos-app.sh` builds `dist/SixNG.app` (GUI and CLI, ad-hoc
signed, not notarised). The app keeps its settings and default work folder
in `~/Library/Application Support/SixNG`, never inside the bundle; plain
binaries keep them next to the exe like on Windows.

There is no default game folder; you set it once:

- CLI: `--game <folder>` or the `SIXNG_GAME` environment variable, or the
  folder saved by the GUI. Without one, the CLI stops with a message that
  says how to set it.
- GUI: on first start a setup wizard asks for the game folder (the one
  with the `.rpf` archives) and the work folder, and saves them to
  `sixng-settings.json` next to the exe (or `%APPDATA%\SixNG\settings.json`
  if that folder is not writable). Change them later on the **Settings**
  page. `SIXNG_GAME` overrides the saved folder.

The work directory (indexes, output, caches) defaults to `./work` for the
CLI and to a `work` folder next to the exe for the GUI; override it with
`--work`, `SIXNG_WORK` or the saved setting. The GUI does not search parent
folders, so it behaves the same wherever it is launched from. Restart
after saving new folders.

## Disk space

The install is about 224 GB. Indexes are small: `levels.rpf` gives about
95 MB of JSON. Output sizes depend on what you extract:

| Extract | Items | Approximate output |
|---------|-------|--------------------|
| one vehicle (`vehicles --name '^phoenix'`) | 248 parts | about 100 MB (incl. GLBs) |
| `textures` (every texture dictionary, PNG) | 2,486 | **about 5.4 GB** |
| `map` (`levels.rpf` drawables + PSO/RBF/XML) | 183,200 | **about 93 GB** raw, plus GLBs; on the samples GLBs roughly double the output, so plan for **~180 GB** |
| `vehicles` (every fragment not matching the prop/char prefix list) | 37,056 | **about 8 GB** raw; **~15 GB** with GLBs |
| `meta` (XML/PSO/RBF/text from data and common) | 3,763 | about 0.2 GB |
| all of `levels.rpf` decompressed | 523 K | **about 175 GB** |
| `sixng placements levels` | 27,946 map files | **about 1.3 GB** JSON, ~1.5 min |
| `sixng placements levels --batches-only` | 906,922 instanced batches: 24.9 M prop + 219 M grass instances | **about 5.8 GB** (0.3 GB `batches.jsonl`, 1.1 GB `batch_props.bin`, 4.4 GB `batch_grass.bin`), ~2.5 min |
| `sixng collision levels` | ~77 K bounds files | **about 15 GB** GLB |
| `sixng navmesh --roads-only` | 27,644 tiles | about 0.25 GB GLB (a full navmesh export is ~25 GB) |
| `sixng lights levels` | 188 K meta files scanned | about 0.7 GB JSONL, ~1.5 min |
| `sixng interiors levels` / `sixng paths levels` | 2,590 interiors / 63,808 nodes + 1,546 junctions | about 15 MB / 40 MB JSON |
| any of collision, navmesh, placements, lights with `--bbox=minx,miny,maxx,maxy` | one region | e.g. a 500 m box: about 110 MB of collision |
| `ttpf` textures linked by models (PNG, `--tex png`) | shared | about **0.6 MB per texture** (1–4 MB for the 2048 px vehicle maps). Each texture is written once per output folder: `phoenix_mid` writes 33 (≈ 60 MB). A full `map` extract references most of the ~200 K map/terrain store textures, about 85 % of which decode: plan for **~100 GB** of PNG on top of the figures above. `--tex dds` alone skips them. |

These figures come from `--dry-run` on this install; the `map` and
`vehicles` totals are decompressed sizes. The `vehicles` category is a
name heuristic, so it also catches some non-vehicle fragments. Narrow it
with `--name`.

Always run with `--dry-run` first. It prints the selection count and the
decompressed byte total without writing anything.

## CLI

All commands print one JSON document on stdout; add `--pretty` to indent
it. Progress goes to stderr as NDJSON events:
`{"event":"progress","stage":…,"done":…,"total":…,"pct":…}`. `--quiet`
turns them off. On error the CLI prints `{"ok":false,"error":…}` and
exits with code 1.

```powershell
# What is in the install, and what is indexed
.\target\release\sixng.exe info

# Build indexes (resumable: re-running continues from checkpoints).
# Takes about 6 min for levels.rpf on NVMe. With no arguments, indexes every RPF9 archive.
.\target\release\sixng.exe index levels textures packs data common

# List entries (filters: --kind, --rtype, --name <regex>, --exclude-name, --id, --id-file, --min-size, --limit)
.\target\release\sixng.exe list levels --rtype txd --limit 20 --pretty
.\target\release\sixng.exe list levels --rtype ft --name '^phoenix' --ndjson

# Search names across all indexed archives
.\target\release\sixng.exe search 'elegy|buffalo' --archive levels

# Name dictionary for shader, parameter and meta field hashes (~2 s, needs the common index).
# Built from your install into work
ames
ames.json; exports and the app use it.
.\target\release\sixng.exe names
.\target\release\sixng.exe names --lookup bcf03fc1,c8e8c282

# Describe one entry without writing files
.\target\release\sixng.exe inspect 'levels.rpf@10f2d2d010' --pretty

# Extract by filter
.\target\release\sixng.exe extract levels --rtype txd --name '^island' -o work\out --tex both

# Map placements (entity lists: model, position, rotation, scale) for all of levels.rpf, ~1.5 min, ~1.3 GB JSON
.\target\release\sixng.exe placements levels -o work\out
# Instanced grass / prop batches (instance transforms; binary, ~5.8 GB, ~2.5 min). --batches adds them to the normal run
.\target\release\sixng.exe placements levels -o work\out --batches-only
# Collision GLBs split by surface id (~15 GB), road polygons from the navmesh, placed lights
.\target\release\sixng.exe collision levels -o work\out
.\target\release\sixng.exe navmesh --roads-only -o work\out
.\target\release\sixng.exe lights levels -o work\out
# Interior instances (world transform, entities, matched collision) and the road/lane graph
.\target\release\sixng.exe interiors levels -o work\out
.\target\release\sixng.exe paths levels -o work\out
# Inferred collision surface categories (after collision, paths, placements --batches; ~3 min)
.\target\release\sixng.exe surfaces levels -o work\out
# Link archetypes whose drawable has no recovered name (bbox + archive-order match; ~9 min, once after indexing).
# Writes work\index\archetype_links.json; placements / lights pick it up automatically
.\target\release\sixng.exe link-archetypes levels
# Archetype LOD distances and resolved LOD parents (~2.5 min). Writes work\index\archetype_lod.jsonl and
# lod_parents.jsonl; placements pick them up automatically (re-run them)
.\target\release\sixng.exe lod-links levels
# Interior membership: per map file its space (world / interior-local / group-local), per entity its
# interior, room and entity set (~1.5 min). Writes work\index\interior_entities.jsonl
.\target\release\sixng.exe interior-entities levels
# PSO metadata to JSON, and vehicle handling (both a few seconds)
.\target\release\sixng.exe pso data --out out
.\target\release\sixng.exe handling data --out out
# Vehicle model info (wheel scale/type), configurations (colour, rim, tyre, parts), paint palettes
.\target\release\sixng.exe vehicles packs --out out
# Bink videos: list (movies.json); add --extract [--only <hex offsets>] to write the .bk2 files
.\target\release\sixng.exe movies movies --out out
# UI graphics: named dictionaries (loading screens, HUD, gauges) and the UI texture store
.\target\release\sixng.exe extract data --rtype txd -o out
.\target\release\sixng.exe ttpf data/ttdb/ui/ttdb_ui.ttpf --all --out out\ui\ttdb_ui
# BCPack texture gaps: every material with flat / low-mip textures, slot roles, class, terrain-layer
# surface guesses and placement weight (~6 min; uses placements, batches, collision, surfaces)
.\target\release\sixng.exe texture-gaps levels -o work\out --file work\texture_gaps.json --placements work\out\placements\levels
# Audio banks in audio.rpf: scan once (~2.5 min), then list (--min-seconds, --limit)
.\target\release\sixng.exe audio --min-seconds 30 --limit 10 --pretty
# Export a bank's streams as Ogg Opus / WAV / both, or the raw bank (awc); --stream <hex> for one, --all for every bank
.\target\release\sixng.exe audio-export --bank 12a2000 --format both -o work\out\audio
# Or every world-data step in order (placements, batches, collision, navmesh, paths, lights, interiors, surfaces)
.\target\release\sixng.exe world all -o work\out
# Restrict the big exports to one region (RAGE metres; use "=" so negative values parse)
.\target\release\sixng.exe collision levels -o work\out --bbox=-1500,-1500,-1000,-1000

# Schema index of every self-describing meta resource (0x7a) in an archive; --dump adds JSON
.\target\release\sixng.exe meta data -o work\out

# Built-in categories: textures | map | vehicles | meta
.\target\release\sixng.exe extract-category vehicles --name '^phoenix' -o work\out
.\target\release\sixng.exe extract-category map --limit 200 -o work\out --dry-run
```

Output options: `--tex png|dds|both|none` (DDS = original BCn with a mip chain; `none` writes no texture files, which is useful for geometry-only re-exports), `--raw` (also write the decompressed
bytes), `--force` (redo items already done), `--no-embedded-textures`,
`--dry-run`.

**Resumable runs:** every item writes a marker file when it finishes, and
items that already have outputs are skipped. To continue a stopped
`extract`, run the same command again. Each run appends one line per item
to `<out>/manifest.jsonl`.

The summary JSON gives `exported`, `skipped`, `error_count` and `errors`
(item failures), plus `warning_count` and `warnings` (non-fatal problems,
for example one texture in an unsupported format). Each list shows at
most 50 items; the manifest has everything.

## GUI

```powershell
.\target\release\sixng-gui.exe
```

The GUI renders with wgpu (Direct3D 12 / Vulkan).

**First start.** A setup wizard asks for the game and work folders, then
offers to prepare everything in the background: index the archives, link
unnamed buildings to their models, export the world data and bake the
map. Each step can be skipped and run later (**Settings → Run setup
again**).

**Pages** (tabs along the top):

- **Library** (home): models by category as thumbnail grids, with
  readable names made from the model names (game display names are not
  decoded). Thumbnails render in the background with the app's own
  renderer and are cached in `<work>	humbs` under a size limit set in
  Settings. Recently opened models and favourites are listed on the home
  page. An item page shows the model's details, **Open in 3D viewer**,
  favourites, **Export (glTF + textures)** (queued as a job) and a
  **Files** panel for the exported files (Show in Explorer, Open, Copy
  path). Clicking a model on either map opens its item page.
- **People** and **Animals** are categories of their own. An animal's
  entry (`crr_iguana_green_m_00`) is only a skeleton with a placeholder
  plane; the viewer puts its default parts together (body, eyes, head, fur,
  teeth …, without accessories like harnesses or costumes), each with its
  own textures. Named characters (`cs_…`, `u_…`, `player_story…`) are
  listed as one entry that puts one item per outfit slot together (head,
  hair, top, jacket, trousers, shoes …). Single ped pieces are hidden as
  variants. Names are made readable: "Iguana (green, male)",
  "Dog (rottweiler, female)", "Farm monster man".
- The viewer's **Parts** panel lists every part an animal or character
  has, grouped by slot (body, eye, crown, shirt, harness …; head, top,
  outer, bottom, feet, hat, glasses …). Pick one per group or "none";
  accessories (hats, harnesses, holsters …) and alternate sets start off.
  Animal sets that span several parts (`08 collarharness`: body + harness,
  `09 corpse roadkill`) have a button each, and **Defaults** goes back.
  The camera stays put while parts change, and the item page's Export and
  Files follow the parts shown in the viewer.
- Colours are never guessed. Many ped clothing and skin materials have no colour
  texture that SixNG can read (clothing colour is in BCPack; skin is built
  by the game at run time). Such materials render plain white, and the
  viewer says how many materials have no colour texture; the material list
  marks each one. Normal maps and decals still show.
- Ped clothing sits in the game's transparent render bucket, but the
  alpha channel of its colour texture is one constant value (64 or 128),
  not opacity. Such materials are exported and drawn opaque; model.json
  says so in `classification.alpha_note`. Textures with real cut-outs
  (lace, crowns, hair) keep their transparency.
- **Model viewer**, **Map 2D**, **Map 3D**, **Textures**: below.
- **Jobs**: every CLI command as a form (built from `sixng --help`, so it
  stays in step with the CLI). Jobs run one at a time in the background
  with a queue, progress bars, Cancel, the log and the result, and a
  button to open the output folder. `sixng.exe` must sit next to
  `sixng-gui.exe`.
- **Status**: archives indexed, models per category, world data and map
  state, the BCPack texture-gap summary, and disk use per folder in the
  work folder with the change since the last check.
- **Settings**: game, work and export folders, the thumbnail cache size,
  and **Run setup again**.
- **Archives** (advanced): the raw entry browser. Pick an archive on the
  left (**Index** it first if needed), filter entries by regex, kind and
  type, preview them on the right (decoded textures, model JSON, text or
  hex) and extract the ticked or filtered entries. Its **World data** row
  runs the whole-map exports (same as `sixng world <step>`).

**Ctrl+K** (or **🔍 Search** at the top right) opens a search box over all
models and pages; with an empty query it lists favourites and recent
models. Arrow keys and Enter pick a result.

### Model viewer

Pick a model from the **Models** list on the left. It holds every
drawable and fragment in `levels.rpf` and `packs.rpf` (about 254 K), in
these categories: Vehicles (about 240, plus their `_hi` copies), Vehicle
parts, Wheels, Weapons, Props, Vegetation, Interiors, Buildings & map, and
Unnamed.
- Search box: words match names and ids.
- **hide LOD copies** (on by default) hides `_lod`/`_slod`/batch-LOD/`_mid`
  copies.
- ☆ marks favourites; there's a **favourites only** filter and a
  **Recent** list. Both are saved in `<work>/gui_models.json`.
- Click to load. **Up / Down** (or ◀ ▶) step through the list.
- **☰ Models** hides the panel.

You can also open by entry id (`levels.rpf@10f266d010`), by name regex
(`^phoenix_hi$`, searched in `levels`; `data:^w_pi` for another
archive), from an exported `.glb`, or with **Open in model viewer** in
the Archives page. Entries are exported on the fly into `<work>/gui_cache_v4`
(models plus PNG textures; v3 since the 2026-11-20 normals fix; older `gui_cache_v*` folders can be deleted).

- Shading: base colour, normal map (DirectX green), packed
  roughness/metal/AO with a channel-order switch, alpha / cutout / decal /
  glass classes (blended back to front), emissive.
- Car paint: primary, secondary (per material **2nd**; rims use it),
  pearl colour and amount, finish (metallic, matte, chrome, gloss).
- **LOD** buttons switch between `model.glb` and `model_lodN.glb`.
- Debug views: albedo, normals, UV checker, single texture slot (base,
  normal, packed, emissive), **BCPack gaps** (green ok, amber low-mip,
  red flat, from the export's texture status), vertex colour, lighting
  only; wireframe overlay, normal lines.
- Skeleton with joint axes and labels. The **Rig** sliders spin the
  wheels and open the four doors. Cars with `wheel_mounts` get the shared
  rim and tyre (default `rim_muscle_001_hi` / `tyre_muscle_001_hi` at
  scale 0.37, fitted by eye; `vehicles.json` now has each car's rims and `wheelScale`, but the viewer does not use them yet).
- Orbit camera (drag, right-drag pan, wheel zoom, double-click to frame),
  or fly camera (WASD/QE, Shift); time of day, exposure, ambient, emissive
  strength and a wet look.
- Per-material list: class, alpha mode, gap status, every texture slot
  with size, format and status; show/hide and solo.
- **Screenshot** saves the 3D view and the whole window to
  `<work>/screenshots/`.

### Map 2D

First bake the map from the world exports (`sixng world all` output in
`work/out`, plus full placements in `work/placements_full`).

**🍞 Bake map** (or **🔄 Re-bake map**) at the top of the Map 2D and Map
3D tabs does this in the app:
- It runs in a separate process, with progress from the log, a
  **Cancel** button and the elapsed time. Both map tabs reload when it
  finishes; it takes about 2 minutes.
- It lists missing world data. Placements are required; the other layers
  are optional, and the button says which world-data steps to
  run (from the Status page, Archives → World data, or Jobs).

From the command line:

```powershell
.\target\release\sixng-gui.exe --bake-map
```

This writes `work/map/` (log in `work/map/bake.log`):

- **Raster tile pyramid**: `tiles/<layer>/<z>/<x>_<y>.png`, 256 px tiles,
  2 m per pixel at the top level. Layers:
  - terrain: hill-shaded top collision surface;
  - collision surface categories;
  - building footprints;
  - navmesh;
  - vegetation/prop density;
  - texture-gap heatmap;
  - LOD regions;
  - water / no collision.
- **Vector data**: `roads.json`, `junctions.json`, `lights.bin`,
  `entities.bin`, `strings.json`, `interiors.json`, and the 8 m height
  grid `heights_8m.bin`.

The **Map 2D** tab shows it:

- Pan with drag; zoom with the wheel, around the cursor.
- Pick a base layer (terrain, surfaces or water). Every other raster
  layer is an overlay with its own opacity.
- Lane graph: coloured by speed limit, lane count or road class. Width
  follows the lane count; one-way links get arrows.
- Junctions: signalised ones cycle through their phase durations; stop
  and yield entrances are marked.
- Lights: exterior/interior toggles, and an hour slider that applies
  each light's on/off-hours mask (`time_flags`).
- Interiors are drawn as outlines.
- **Inspect**: hover shows the footprint under the cursor; click opens
  the placement (model id, archetype, kind, LOD level, position in game
  and glTF axes, size, map file, lights inside). **Open in model viewer**
  goes to the model.
- **Search**: by name, model id or archetype hash; all hits are circled
  and listed.
- **Measure**: click points; right-click clears.
- **Box select**: shows the `--bbox` for region exports.
- The readout gives game x/y/z and the glTF/BeamNG (x, z, −y) position.

Map screenshots: `--tab map2d --center X,Y --mpp M --base N --overlays
2,3 [--lights --hour H] [--search name] --screenshot out.png --exit`.

### Map 3D

The **Map 3D** tab is a fly-through of the placed models, streamed from the
baked map (`entities.bin`; bake first with `--bake-map`):

- Fly: drag to look, WASD/arrows to move, Q/E down/up, Shift faster. The
  speed scales with height above ground.
- Models are exported on demand into `<work>/gui_cache_v4` (the model
  viewer's cache) by background workers and drawn instanced: one GPU model
  per model id, all its placements as instances. Loads go on-screen first,
  then big and near before small and far; placements outside the view
  (widened a little) are not drawn but stay loaded for turning round.
- **Texture streaming**: a model first loads with 64 px textures, read from
  reduced copies (≤ 256 px) kept in `<work>/gui_cache_v4/lowtex` (written
  on first use, capped at 4 GB), then reloads with sharper textures as the
  camera comes closer (full size in the HD band, then 1/2, 1/4, 1/8).
  Textures are shared between models.
- **Quality** (low / medium / high / ultra, saved in
  `<work>/gui_settings.json`) sets the detail, a triangle budget, a
  draw-call budget, the far-field error and the texture cap. Over either
  budget the smallest objects on screen are dropped first (the Stream
  window shows the size cut); both budgets have their own sliders.
- **Detail** slider: sets the LOD distance bands (HD, LOD, SLOD1–4), prop
  and vegetation density and the minimum screen size. Each can be tuned
  on its own, together with the drawable LOD file and the texture cap.
- **HD fallback** (on by default): most LOD-level placements have no linked
  model yet (unresolved archetype hashes; about 88 % around downtown), so
  large HD models stay visible out to 1.75× the HD distance and the LOD
  level is skipped. SLOD placements mostly carry a placeholder unit-cube
  bbox, so they are never culled by screen size.
- **Whole-map terrain** (far field): the whole map is always drawn, out to
  the horizon, as a quadtree of tiles over the baked 8 m top-surface
  heights, coloured with the 2D map's terrain tiles (coarse tiles for
  coarse nodes, so the colour sharpens as nodes refine). Nodes refine by
  screen-space error (**terrain error**, px) and frustum; children replace
  a parent only once all four are loaded, so nothing pops out. Beyond the
  HD building range the tiles keep the roofs (max-pooled heights), so
  buildings without a loaded model show as blocks instead of a void; near
  the camera a tile switches to ground heights (a morphological opening
  that removes buildings) once at least 90 % of the building models on it
  have loaded. Built in the background from the map data already in
  memory (about 0.3 s); no extra bake. A sea plane fills the rest.
- Overlays: roads (by speed) and exterior lights (by hour) as lines; a
  draggable **Minimap** window (click to move there) and a **Stream**
  window (entities, models wanted/loaded, queue, instances, triangles,
  textures, per-band counts, memory). Click a model to inspect it (bbox
  outline, name, model id, LOD level, kind, position) and **Open in model
  viewer**.
- **LOD hand-off**: the bake keeps each placement's resolved LOD parent,
  `lod_dist` and `child_lod_dist`. A parent LOD/SLOD stays drawn until the
  camera is inside its `child_lod_dist` *and* every child has loaded; then
  the children replace it, so the two are never drawn together. A child
  whose parent has no model stays out to the parent's distance. Unlinked
  placements use the distance bands. Distances scale with the detail
  slider.
- **Proxies hidden**: interior-shell, shadow, light and VFX proxies,
  placements without a model and degenerate (< 1 cm) models are not drawn;
  **show proxies / placeholders** draws them (`--show-proxies`). The same
  rule writes `render: false` + `render_reason` into the placements.
- **Decals and depth**: decal-bucket materials (markings, grime, signage,
  terrain blend layers) are drawn after opaque geometry, by layer order,
  with a small depth bias; the depth buffer is reversed-Z 32-bit float.
  **decal depth bias** (`--no-decal-bias`) turns the decal pass off for
  comparison; the model viewer has the same toggle.
- **Memory**: the limits scale with the installed RAM. Above 20 % of RAM
  (the soft cap) the streamer loads only nearby models at 64 px and drops
  everything off screen; above 30 % (the hard cap) it also drops the
  farthest visible models. The texture budget is RAM/16 (0.5–3 GB) and the
  model cap is 120 per GB (800–8,000). The **Memory** panel shows usage
  against the caps; untick **automatic** to set your own limit with the
  slider. It is saved in `<work>/gui_settings.json`. The process figure
  includes memory the graphics driver commits for GPU buffers and
  textures, so it grows with what is on the GPU (about 3 GB plus the
  estimated GPU memory downtown).

- **Lighting**: ambient light comes from a mostly desaturated sky, and the
  fog fades towards a half-desaturated sky, so surfaces keep their texture
  colour instead of turning blue-white. **packed data** (Light panel)
  picks how the packed material-data texture is read; the default
  **ignore packed** is used because its channel order is unverified and
  the common maps (R = G = 0, B ≈ 0.5) read as "R = roughness" made every
  surface a mirror. The model viewer's default is the same (`--rma 3`).
- **Placeholder textures**: textures of 8 px or less (engine defaults such
  as a 4×4 all-zero data map or a flat-black normal map) are ignored in
  the normal and packed slots (they made walls black). Opaque materials
  whose every decoded texture is such a placeholder are hidden on the map
  (they showed as floating black boxes); the model viewer still shows them.

Map 3D screenshots: `--tab map3d --center X,Y --height H --yaw DEG --pitch
DEG [--quality low|medium|high|ultra] [--detail 0-1] [--tod H] [--lights]
[--roads] [--no-terrain] [--fog F] [--max-tex PX] [--no-textures]
[--no-hd-fallback] [--kinds a,b] [--lods a,b] [--ram-cap GB]
[--no-windows] [--no-decal-bias] [--show-proxies] [--fly X,Y,H,SECONDS]
[--no-vsync] --screenshot out.png --exit`.
The shot waits until every wanted model is loaded (15-minute limit) and
also saves `out_ui.png` with the panels. `--fly` first waits for the
start view, then flies to (X, Y, H metres above ground) and logs the
frame times of the flight. Timings (open time, time to a complete
low-texture view, frame times, RAM, estimated GPU memory, draw calls,
loader times) go to stderr and `<work>/map3d_perf.log`.

Screenshots from the command line (for checks and docs):

```powershell
.\target\release\sixng-gui.exe --model levels.rpf@10f266d010 --mode 5 --yaw 2.4 --pitch 0.25 --screenshot car.png --exit
```

Options: `--lod N`, `--mode 0-7` (shaded, albedo, normals, UV, slot,
gaps, vertex colour, lighting), `--slot 0-3`, `--rma 0-3`, `--wire`,
`--skeleton`, `--normals`, `--tod H`, `--paint r,g,b`, `--finish 0-3`,
`--pearl A`, `--wet A`, `--exposure E`, `--spin R`, `--door R`, `--yaw R`,
`--pitch R`, `--zoom F`, `--size WxH`.

### Textures

The **🖼 Textures** tab lists every texture in the game's texture stores
(`data/ttdb/*.ttpf`, about 352 K), filtered by store, format, minimum size
and hash. Pick one to see a decoded mip (the largest one that is not in
an undecodable BCPack chunk, at most 2048 px, by default; any mip from the list),
as RGBA, RGB or a single channel, zoomed to fit or 0–8×. **Export PNG**
saves the shown mip to `<work>/out/textures/<hash>_m<mip>.png`, and
**Show in Explorer** opens it. To browse, scroll the mouse wheel over the
preview (one texture per notch, at fit zoom; zoomed in, the wheel pans) or
press ↑ / ↓ (Page Up / Page Down: 10 at a time). Textures have no names, only their 64-bit
hash, the same `hash64` that `model.json` lists for each material slot:
in the Model viewer, open a material and click **view** on a slot to jump
here. From the command line: `--texture <hash>` (or `--tab textures`)
with `--window-screenshot out.png --exit`.

### Audio

The **🔊 Audio** tab (or the Library's **Audio** card) lists the sound
banks in `audio.rpf`. The first visit offers a scan, which reads the
archive once (about 2.5 min) and saves `<work>/index/audio_audio.json.gz`;
`sixng audio` writes the same index. Filter banks by bank offset or stream
id (hex) and a minimum length; pick a bank to see its streams with length,
rate field and layout. **▶ Play** decodes a stream and plays it on the
default output device; the speaker button mutes and unmutes, and the slider
next to it sets the volume. **.opus** / **.wav** export one stream,
**Export all .opus / .wav** the whole bank, and **Export bank .awc** its
raw bytes, all to `<work>/out/audio/`. Music and other-codec streams are
listed but only export raw. From the command line: `--tab audio [--bank
<hex>] --screenshot out.png --exit`.

## Output layout

```
<out>/
  manifest.jsonl                       one line per exported item
  textures/<archive>/<name>_<off>/     a texture dictionary
    dictionary.json
    <texture>.png / .dds / .json       mip 0, plus per-texture metadata
  models/<archive>/<name>_<off>/       a drawable or fragment
    model.glb, model_lodN.glb          triangle meshes per LOD (glTF 2.0, Y-up, metres;
                                       fragments: skeleton joints + skin):
                                       root > modelN > geomNN_<off> nodes, POSITION,
                                       NORMAL, TANGENT, TEXCOORD_0..3, _COLOR0 (raw), one
                                       double-sided material per shader
    model_parts.glb                    models of embedded drawables (fragment children,
                                       building sub-drawables): parts, not LODs (2026-11-20)
    points.glb                         only for mesh blobs with no index buffer
    model.json                         name, type, source streams, vertex/triangle
                                       counts, coordinate_system, surface_hint,
                                       meshes[] (material, uv_sets, normals, ...),
                                       drawable.models[] / shaders[] (hashes),
                                       drawable.skeleton[] (node hierarchy),
                                       buffers (+ mesh-blob headers),
                                       embedded textures, strings (bones / attach points)
    buffers/NN_<gfxoff>.bin            raw GPU buffers
    sys.bin                            decompressed system page
    textures/                          embedded textures (PNG/DDS)
  textures/ttpf/<hash>.png / .json     textures from the data/ttdb stores, shared by
                                       all models (written once; .json = size, mean
                                       colour, normal-map test). GLB materials link
                                       them as ../../../textures/ttpf/<hash>.png
  raw/<archive>/<name>_<off>.<ext>     anything else (xml, pso, rbf, bin…)
  placements/<archive>/<off>.json      one map file (`sixng placements`): entities[] with
                                       archetype (joaat), name + model_id when the model
                                       is indexed, position (RAGE Z-up), rotation (as
                                       stored = inverse), rotation_world, scale_xy/scale_z,
                                       flags, guid, gltf {translation, rotation, scale}
                                       ready for placing model.glb in a Y-up scene, and
                                       every other scalar field by hash; render: false +
                                       render_reason for proxies/placeholders; plus the map
                                       file's own fields (extents, parent hash)
  placements/<archive>/index.jsonl     one line per map file: id, entity count, resolved
                                       count, position bounding box
```

Entry ids are `<archive>@<hex offset>` (for example `levels.rpf@10f2d2d010`).
They are stable for a given install, so you can use them in scripts and
with `--id`.

## Using the output downstream (game engines, BeamNG.drive, Assetto Corsa)

The extractor's output is meant as an intermediate format for mods, for
example in a custom game engine, BeamNG.drive or Assetto Corsa. This table says what each
of them needs and what the output provides today.

| Need | Status in the output |
|------|----------------------|
| **Scale and axes** | **Done.** 1 unit = 1 m. Source data is RAGE Z-up, right-handed. `model.glb` (and `points.glb`) are glTF Y-up, with `glb(x, y, z) = rage(x, z, −y)`. Blender's glTF importer converts back to Z-up. The same note is in every `model.json` (`coordinate_system`) and in the GLB scene `extras`. |
| **Meshes → FBX (Blender/assimp → ksEditor → KN5)** | **Usable.** `model.glb` holds real triangle meshes. Each geometry is its own named node and mesh (`geomNN_<offset>`) with one material, grouped under `modelN` nodes. Blender's glTF importer, followed by FBX export, keeps the split. `phoenix_mid` models 0–2 are the intact car; 3–5 are an alternate set (probably damage) with the same layout, so delete or hide them. Small bone-space parts (hubs, rods) are offset until pivots are solved. About 3 % of triangles were re-wound to match the stored normals; all materials are double-sided. |
| **Per-mesh material names** | **Partial: stable but hashed.** Every geometry has a material named `shaderNN_<shader-name-hash>`, for example `shader03_77d8ab27`. Geometries that share a shader share a material. The hashes (shader, parameters, texture references) are kept in the material `extras` and in `model.json` → `materials[]` / `drawable.shaders[]`. Texture hashes resolve to `ttpf` textures (see below). Shader and parameter names are joaat of the lower-case name: since 2026-11-21 `model.json` and the material `extras` carry `shader_name` (all 344 shaders, e.g. `standard_vehicle_2lyr`) and each texture slot's `param_name` (`diffusetex`, `bumptex`, `lyr1normaltex` …; 77 of 229 parameters, about 75 % of uses). The material names in the GLB stay `shaderNN_<hash>` so existing pipelines keep working. |
| **Vertex normals, UV sets (incl. UV2)** | **Done.** `NORMAL` (decoded from octahedral) and `TEXCOORD_0..3` (the second set, UV1, is `TEXCOORD_1`, which ksEditor's UV2 needs) are written wherever the mesh has them. `meshes[]` lists `uv_sets`, `uv_slots` and `normals` per geometry. `TANGENT` (x, y, z, w) is the game's own tangent where the geometry has one (most vehicle parts; about a sixth of map geometries); elsewhere let the importer generate it. |
| **Textures as DDS** | **Done for texture dictionaries; PNG only for `ttpf`.** Use `--tex both` or `--tex dds`. The DDS keeps the **original BCn data** (untiled, not re-encoded), with a DX10 header and a mip chain down to 128 px. The smallest mips are left out because their packed layout is not solved; a partial chain is valid DDS. `ttpf` textures linked by models are written as PNG (mip 0) to `textures/ttpf/`. |
| **Drivable surface split (`1ROAD`, `1GRASS`, `1KERB` …)** | **Partial.** The collision export (`sixng collision`) splits every collision mesh by surface id (one glTF primitive each), but the ids are not mapped to names (the hash is unknown). Inferred categories (road surface, dirt track, hard surface, paved ground, grass / vegetated soil, forest floor, wetland, walls, shore, seabed, generic) cover 98.3 % of collision area (90.8 % at high or medium confidence; `sixng surfaces`): `collision/<archive>/surface_categories.json`. Separately, `model.json` → `surface_hint` (`road`, `kerb`, `sidewalk`, `grass`, `dirt`, `vegetation` or null) is guessed from the model name, e.g. `ap_road_upper_a` → `road`. |
| **Vehicle node hierarchy and pivots** | **Done.** `model.json` → `drawable.skeleton[]` gives every node with its parent and its model-space `pivot` / `bind_matrix` (from the game's inverse bind matrices). In the GLB, the skeleton is a glTF joint hierarchy with matching local transforms. Verified on `phoenix_mid`: wheels at (±0.79, 1.37) / (±0.76, −1.36), bonnet and boot hinges, door hinges and lamps all symmetric and in place. glTF forward is −Z (RAGE +Y). |
| **Handling / physics metadata** | **Decoded (2026-11-21):** `sixng handling data --out <dir>` writes `<out>/pso/handling.json`, 1,097 handling records (301 named by model, e.g. `phoenix`), each a list of typed components: body (`fMass`, `fInitialDragCoeff`), transmission (`fGearRatios`, `fFinalDriveRatio`), engine (RPM and torque curve), fuel, wheels/steering/brakes (`fSteeringLock`, `fBrakeForce`, `fWheelMass`), suspension, damage. Many fields are still hashes. |
| **AI lines, pit/start positions** | **Partial.** The lane graph (`sixng paths`: node positions, links, lane counts, speed limits) gives road centrelines to derive an AI line from; road polygons come from the navmesh. No start positions found. |

Recommended flags for these consumers:

```powershell
# Car: every part, DDS + PNG, metadata incl. skeleton
.\target\release\sixng.exe extract-category vehicles --name '^phoenix' -o work\out --tex both
# Track pieces whose names look like roads, with surface hints
.\target\release\sixng.exe extract levels --rtype dr --name 'road|_rd_|kerb|carpark' -o work\out --tex both --dry-run
```

### Custom Shaders Patch (CSP) needs

For Assetto Corsa mods that target CSP, this table tracks what the extractor
provides for each one and steers what gets decoded next.

Status key:
- **now**: exported.
- **partial**: exported, but incomplete or unverified.
- **raw**: the data is exported but not decoded.
- **unknown**: not located yet.

#### Format limits

| CSP need | Status | Notes |
|----------|--------|-------|
| KN5 16-bit indices (≤ 65,535 vertices per mesh) | **now** | `meshes[].vertex_count` and `meshes[].exceeds_kn5_16bit_limit` per geometry. The game's own remap is u16, so a single geometry never exceeds the limit, but merging meshes in the converter can. |
| Normals | **now** | `NORMAL`. |
| Tangents | **done** | Stream slot 11 (format 0x27) is R10G10B10A2: xyz in model space, A2 = bitangent sign. Written as glTF `TANGENT` (w = ±1), orthonormalised against the normal. Checked against tangents computed from UV0 (mean \|dot\| 1.000, sign agreement 0.999) and rendered with normal maps. Geometries without the stream have no tangents; let the importer generate them there. Normal maps are DirectX convention (green down). |
| UV2 (`.uv2`: scenery AO, car stickers) | **now** | Every UV set is written. `TEXCOORD_1` is the game's second set. Values are as stored, so they are not guaranteed to be in 0..1; check before using half precision. |
| Vertex colours (tint, AO bake source) | **partial** | `_COLOR0` holds the raw RGBA8 of stream 1. Its meaning is unverified. Streams 12 and 13 (also RGBA8) are not exported. |

#### Materials

| CSP need | Status | Notes |
|----------|--------|-------|
| Stable, readable mesh and material names | **partial** | The names are stable (`geomNN_<offset>`, `shaderNN_<hash>`, `textures/ttpf/<hash>.png`). Shader names (all) and most parameter names are in `model.json` and the material `extras` (`shader_name`, `param_name`); texture names are unresolved 64-bit hashes. |
| The game shader or material class per material | **works (name); partial (classified)** | Every material has its game shader name (`shader_name`, e.g. `standard_decal_hbb`, `ped_standard_skin`, `standard_vehicle_glass`). Each material in `model.json` has a `classification`: `class` (opaque / alpha / cutout / decal / glass / window / emissive / water / terrain_multilayer / car_paint), `render_bucket` from the shader record flags, `alpha_mode` (glTF OPAQUE/MASK/BLEND, also set in the GLB), `instanced_or_batch_lod`, the shader `family` where known, and for decal-bucket materials `decal {bias_m, order, order_source, depth_write}`: draw after opaque geometry in ascending `order` (terrain blend layers 0, other decals 1; no per-material priority is known), offset by `bias_m` toward the camera. Buckets come from flags measured over 254 K drawables. Glass, window, emissive and terrain classes come from a list of known shader families. Emissive has no flag, so unknown emissive shaders show as opaque. |
| PBR maps (albedo, normal, roughness, metal, AO) and channel layout | **partial** | Base colour and normal map are linked in the GLB when they decode. `model.json` → `materials[]` lists every slot with its parameter hash, format, size, tile mode, `role_guess`, `slot_role` (what a known slot holds), and the PNG path or why it is missing. **Normal maps are DirectX (green down)**: invert G for glTF/Blender/OpenGL. Engines that use the DirectX convention take them as they are; check each converter. Roughness/metal/AO sit in packed linear slots (`739e3d0d`, `c6f46e79`). They are exported as PNG but not mapped. Channel statistics and a visual sheet suggest `c6f46e79` = R binary mask (metal?), G roughness or gloss, B AO/cavity, A unused; this is not confirmed. |
| Emissive flag and emissive textures | **partial** | Shader names now show the emissive families (`standard_emissive*`, `signs_emissive`, `*_emissive_layer*`) and the `emissivetex` slot; the viewer's classification does not use them yet. |
| Transparency / alpha-blend flag (glass) | **unknown** | It is probably in the shader record's +0x38 bytes or the draw bucket, which is not decoded. |
| RainFX classes (asphalt, lines, grass, glass, cloth) | **partial** | `surface_hint` from model names only. |
| Terrain RGBA blend masks, multilayer layers | **unknown** | |
| Window, water, screen and horizon tagging | **partial** | Shader names identify glass (`standard_vehicle_glass`, `standard_glass_breakable*`), water (`standard_alpha_water`, `water_micro_geom`, `standard_alpha_waterfall`) and signs; not yet used for tagging. Water type (sea, lake, pond, pool) is not located. |

#### Map

| CSP need | Status | Notes |
|----------|--------|-------|
| Instancing and placements (archetype, transform, LODs) | **works (transform); partial (LODs)** | `sixng placements` writes every entity with its model name, entry id and a ready-made glTF transform. Models are still exported once per archetype, never baked. LOD parent/child links are in the per-entity fields but not interpreted. Interiors: `sixng interiors` gives each of the 2,590 interior instances a world transform and its entity list. **Instanced batches** (grass, weeds, debris, fences, reflectors, `*_batch_slod` LODs): `--batches[-only]` writes `batches.jsonl` (one line per batch: archetype, model name, bbox, LOD distance, `first` / `count` into the binary) plus `batch_props.bin` (44 B per instance: position f32×3, quaternion xyzw f32×4, scale f32×3, tint RGBA u8×4) and `batch_grass.bin` (20 B per instance: position f32×3, normal x/y u8, colour u8×3, scale u8, AO u8, pad). All 906,922 batches decode, 99.76 % with model names. |
| Trees and bushes separately, with LODs and placements | **works (placements); partial (LODs)** | Tree and plant models export like any drawable, and their instances (e.g. `veg_pot_hedge_lrg_02` × 25 in one sample) come out of `sixng placements` with scale. |
| Light emitters (position, colour, range, cone, night flag) | **works** | `sixng lights`: 774,847 placed lights in world space, interiors included. Intensity, falloff (range), falloff exponent, cone, corona, volumetric and fade fields are named from the game's own field hashes. |
| Collision split by surface type; walls identifiable | **works (split); partial (names)** | `sixng collision`: one GLB per bounds file, one primitive per surface id, in world coordinates. Surface ids are unnamed hashes; inferred categories (`sixng surfaces`) cover 98.3 % of area, each with a confidence (90.8 % high or medium), from plants and props standing on each surface, path-link density, slope and height. Interior collision: `sixng interiors` links 70.7 % of interior archetypes (`collision_linked`, ~91 % precision on the ordered links); its second pass links the rest by fitting the interior's entities onto candidate collision floors, for 97.8 % in all, 79.3 % at high or medium confidence. |
| Dynamic props: mass, centre of gravity, collider primitives | **unknown** | Fragment physics data is not decoded yet. |
| Animated props, rail and tram paths | **unknown** | `anim.rpf` and path data are not decoded. |
| Tunnel and bridge extents; billboard, TV and skyline meshes | **unknown** | |
| Road/lane graph, traffic lights, pit and start positions | **works (lane graph, junctions, traffic lights, road surface); unknown (pit/start)** | `sixng paths`: node graph with lanes, speed limits and named node flags (tunnel, highway slip/exit, no-GPS, stop, yield …); junctions with turn paths and traffic-light phases linked to the placed signal props; `sixng navmesh --roads-only`: road polygons per 128 m tile. Link flags: 4 of 11 names recovered, 2 inferred (unpaved / track, two-way). Start positions not found. |

#### Cars

| CSP need | Status | Notes |
|----------|--------|-------|
| Node hierarchy with pivots | **now** | Skeleton joints with model-space pivots, in `model.json` and as glTF joints. |
| Bonnet and boot as their own parent nodes | **partial** | `bonnet` and `boot` are joints with correct hinge pivots, and their vertices are skinned to them (slot 8). They are not split into separate mesh nodes: a converter can split by joint (most parts use one bone per vertex). Separate `_dmg_mod_bonnet_*` / `_boot_*` fragments also exist. |
| Skinning (bones and weights) | **now** | glTF skin: one joint per vertex for car bodies (slot 8, format 0x00), and up to 4 joints and weights for wheels and tyres (slot 8 format 0x25 + slot 6). The bind pose is model space. |
| Damage or deformation variants | **partial** | Damage parts are separate fragments (`<car>_dmg_mod_<part>_NNN`, with `_hi`/`_mid`) and are exported. How the game swaps them in is not decoded. (`phoenix_mid` models 3–5 are LOD1, not damage.) |
| Light meshes by function, or light IDs | **unknown** | |
| Car paint (`smCarPaint`: colour, flakes, clear coat; liveries) | **partial (colours decoded 2026-11-21)** | `sixng vehicles` gives each model's spawn configurations with a body colour and its paint palettes. Colours are stored as ARGB u32 and are all dark, so they are probably linear. `srgb_if_linear` gives the sRGB reading (phoenix: steel blue, sage green, navy …; not checked in game). Secondary/pearl colours and liveries are not identified. Material side: paint materials have `classification.class = \"car_paint\"` and a `paint` block in `model.json` (mask slot hashes, a neutral default colour); in the GLB they get a grey `baseColorFactor`, metallic 0.6 and roughness 0.25. Take the colour from `vehicles.json`. The model viewer applies a car's first spawn configuration (colour, rim, tyre) when it opens, lists the other configurations, and shows the palettes as swatches with their shares. Palette colours rarely match the configuration colours (168 of 4,024), so they are a separate random-paint pool. |
| Tyre, rim, disc and caliper per corner, with widths and radii | **partial** | Rims and tyres are shared models, exported separately, at unit scale for the left-front corner. Wheel centres are in `wheel_mounts`. `sixng vehicles` gives per configuration the rim, tyre, brake disc and caliper model names (e.g. `rim_uni_steel_007`, `tyre_muscle_001`, `brakedisc_std_001`, `caliper_2pot_001`) and per model `wheelScale` / `wheelScaleRear` (probably the rim radius in metres). The tyre width and the exact scaling rule are not established. |
| Animated parts and clips (spoiler, roof, doors, pop-ups) | **unknown** | |
| Handling: mass, centre of mass, inertia, gearing, torque, hardpoints, springs, anti-roll bars, tyres, aero | **partial** | `sixng handling` (see above). Mass, drag, gear ratios, final drive, brakes, steering lock, wheel mass, suspension travel and damping, anti-roll bar force and fuel are named; the engine torque curve and most other fields are decoded values with hashed names. |

## Layout of this repo

```
crates/sixng-core   library: rpf9, scan (carving index), classify, resource,
                    texture (decode + deswizzle), drawable, export, game
crates/sixng-cli    `sixng` binary
crates/sixng-gui    `sixng-gui` binary (eframe/egui)
docs/images/        README screenshots
work/               indexes and output (git-ignored)
```

## Licence

SixNG is free software under the [GNU General Public License v3.0 only](LICENSE).
Third-party notices (AMD addrlib, MIT): [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
