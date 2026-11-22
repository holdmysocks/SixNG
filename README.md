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

> **BCPack: all six BC formats decode.**
> BCPack is the Xbox Series hardware texture compression, with one codec
> per BC format and no public specification. About 12 % of the game's
> textures have their full-size image stored in BCPack. Almost all of them
> are BC1, which SixNG decodes, so they export at full size. BC3 decodes
> too (only 17 textures use it), and so do BC7 (1,872 textures), BC4 (40),
> BC5 (91) and BC6H (98). Their preset tables are fitted, so some blocks
> can be off (speckles, mostly in BC7 and BC6H). If a chunk ever fails to
> decode, SixNG uses the largest smaller mip that is stored normally, or a
> flat fallback colour flagged as `texture_fallback` in `model.json`.

## Status

| Area | State |
|------|-------|
| RPF9 header | **works**: parsed and shown by `sixng info` |
| RPF9 TOC / file names | **not read**: the TOC is encrypted and SixNG does not decrypt it. Contents are found by carving the zlib streams instead, with names recovered from inside resources. |
| Listing / search | **works**: carving index plus name recovery. On `levels.rpf`: 523,552 entries (1.12 M zlib streams, 98.3 % byte coverage), 227,766 named (drawables, fragments, texture dictionaries). A full index takes about 3 min. |
| Textures in texture dictionaries and models | **works** for BC1/BC2/BC3/BC4/BC5/BC6H/BC7: Xbox tiling removed, PNG and DDS output, checked visually. RGBA8 tiling is unverified. Only slice 0 of arrays is exported. BCPack-only textures get a flat fallback colour, flagged `texture_fallback` in `model.json`. Since 2026-11-21, small textures with mips whose whole chain fits in one 4 KiB mip-tail block (32×32 BC3 HUD icons and the like) decode from that block; before, they came out blank. Which materials are affected, and how much of the map they cover: `sixng texture-gaps` → `work/texture_gaps.json`. **Tint palettes**: the `tintpalettetex` slot (`2c8d7182`) of almost every map model points at a one-row RGBA8 palette embedded in the model itself (13 to 2,051 colours). Since 2026-11-21 the exporter finds it by its store key and writes `textures/<hash>.png` next to `model.json` (status `ok (embedded in the model)`); before, these showed as "not in any ttpf store" (30 K of the 33.8 K such hashes in a full map export). The vertex colour's blue channel is the palette index (checked on 36 models: never past the palette's real entries; tinted parts have near-grey base textures), so the GLB carries the tint as `COLOR_0` = palette entry in linear RGB, which glTF viewers and importers multiply into the base colour: white pool tiles come out pool-blue, white cables black, grey or blue. Palettes hold rows of colour variants (a shelf's 4 colour schemes, a filing cabinet's 16): the width is a power of two plus *k* extra texels, and a row holds 256 >> *k* entries (1024 wide = 4 rows of 256, 131 = 4 × 32, 260 = 16 × 16, 13 = 1 × 8). Each placed instance picks its row with the entity tint value (field `3c852527`), written as `tint_row` in `sixng placements` (absent = 0; 22 % of the 1.6 M entities on models with a palette use a row other than 0, and 99.8 % of those are within the palette's rows). `model.json` gives each palette's `tint_rows` and `tint_row_length`, so an instance's colour is palette entry `tint_row × tint_row_length + COLOR0.b`. The GLB uses row 0, the default. The sixng-gui model viewer shows it too, with a **tint** checkbox under View (on by default; `--no-tint` for screenshots); models cached from before are exported again when opened. The remaining 3.7 K referenced hashes (139 K references in a full map export) are in no store and no resource of the install; two of them (`1e2c9340e14a7d07`, `aa3eeff23a15dc75`) account for half of those references and are probably engine defaults. Since 2026-11-21 each such reference gets a `texture_fallback` in `model.json` (reason "not in the install") with a value by slot: engine-default hashes white (`5d7a3a87cc19967d`, the vehicle paint slot; `1e2c9340e14a7d07`, assumed); a missing base colour on a model with a tint palette white, so the tint shows; other colour slots the average of the model's decoded colour textures; normal maps flat; known data slots their typical value (median of the decoded textures in that slot); other data mid grey. A **decal** whose base colour is missing gets alpha 0, so it no longer draws as a white strip (bathroom decal sample: white edge strips and a white pillar gone). |
| Bulk texture stores (`data/ttdb/*.ttpf`, `pack/*/data/ttdb/*.ttpf`) | **works for all 357 K textures** by the chunk-table check (mip 0 outside undecodable BCPack chunks; 99.97 % before BC6H BCPack decoding, 99.94 % before BC4/BC5, 99.4 % before BC7, 86.8 % before BC1): maps 99.2 %, props 99.5 %, peds 99.2 %, interiors 99.7 %, vehicles 98.9 %, terrain 97.4 %, ui 91.1 %. The layout is AMD Gfx10: tile mode 0 = `SW_64KB_D`, modes 1–3 = `SW_4KB_S`, with the addrlib mip tail. That covers all sizes, including the car body/detail textures and the small ones. Each mip was checked against the next one downsampled, and the results were viewed on UI images, atlases, billboards, vehicle detail maps and normal atlases. **BCPack** (the Xbox Series hardware texture codec, one codec id per BC format, no zlib copies): 12.4 % of textures have mip 0 in BCPack chunks. **BC1 BCPack chunks (codec 0x04) decode** since 2026-11-21, which brings 11.6 % of all textures to full size; if a chunk does not decode, the texture falls back to the largest mip outside BCPack as before. **BC3 BCPack chunks (codec 0x0C) decode** since 2026-11-22 (35 chunks; 17 more textures at full size). Its preset table and colour index codebook were fitted against the neighbouring mips: decoded mips match the next mip within 27–47 dB RGB, mostly 31–39 dB. **BC7 BCPack chunks (codec 0x1C) decode** since 2026-11-22 (5,167 chunks; 1,872 more textures at full size, mip 0 coverage 99.41 % → 99.94 %). Its 256-slot preset history table was fitted against the neighbouring mips, so blocks whose reference chain starts in a mis-fitted slot (near the start of a chunk) can come out speckled; the rest matches the next mip closely. **BC4 and BC5 BCPack chunks (codecs 0x10, 0x14) decode** since 2026-11-22 (77 BC4 and 333 BC5 chunks; 40 and 91 more textures at full size, mip 0 coverage 99.94 % → 99.97 %). Their preset tables and index-pair tables were fitted against the neighbouring mips, then against decoded neighbours for textures stored only in BCPack: decoded mips match the reference within 35–50 dB (BC4) and a median of 43 dB (BC5); 0.04 % of BC5 blocks start from a preset slot that no texture let us fit. **BC6H BCPack chunks (codec 0x18) decode** since 2026-11-22 (268 chunks; the last 98 textures at full size). Its preset tables were fitted against the neighbouring mips: on textures with a plain neighbour, 86 % of blocks match the parent mip within 0.15 stops (tonemapped 27 dB; per texture 18–49 dB). Since a later fix on 2026-11-22 (how a one-region block hands its state to a two-region block), blocks no longer come out speckled where the reference chain changes BC6H mode: 81 % and 23 dB before, up to 26 dB better on single prop textures. A refit on the same day that scores the endpoint presets with the decoded indices (the index-free score was lenient on the second region) brings this to 88 % and 28 dB; the median texture went from 29 to 35 dB, up to 21 dB better on single prop textures, and on textures stored only in BCPack 62 % of blocks match their decoded neighbours (58 % before). 1.7 % of BC6H blocks start from a preset slot that no texture let us fit. If a chunk fails to decode, the texture is **exported at a lower mip** that sits in zlib chunks (`mip_used` in `model.json`) when there is one. `sixng ttpf <store>` lists a store or decodes textures by hash; `--all` decodes a whole store in parallel and writes `index.json` with each texture marked complete, `partial` (part of the data in BCPack chunks that did not decode, so part of the image is blank or noise) or `bcpack_only` (no PNG written). Store names must be unique: use the full id (`data/ttdb/ui/ttdb_ui.ttpf`) when a short one matches several. **Texture arrays** (2–64 slices) and **cube maps** (6 faces, tile modes 9–12) decode since 2026-11-20 (earlier they came out as a mosaic or not at all). Models use slice 0 / face 0; `sixng ttpf <store> --hash <h> --all-slices` writes every slice. **Mip counts** are corrected from the store layout since 2026-11-21: the descriptor's mip field undercounts on about 44.9 K textures (most 1024² terrain normal maps in tile modes 1 and 2, about 2.7 K UI sprites, and some vfx, ped, map and prop textures). Mip 0 comes last, so those decoded with a scrambled band of the smaller mips across the top, or as noise for small UI sprites. Re-export affected textures; cached PNGs re-decode on their own. **1-byte formats** (A8, used for decal and mask textures, and R8) decode in all tile modes since 2026-11-21 (126 textures; before, they failed with "tile mode N / format 65 not supported"). Checked the same way: mip 1 against mip 0 downsampled, median 63 dB. **4-, 2- and 8-byte formats** (RGBA8 / BGRA8 and their sRGB forms, R16_FLOAT, R16G16B16A16 UNORM and FLOAT) decode in all tile modes since 2026-11-21 (2,730 textures: terrain and prop masks, UI and graphics sprites, LUTs, height and data maps). Mode 0 RGBA8 matches its mip 1 to within 0.07 levels on average and modes 1 and 2 were viewed; R16_FLOAT shows as grey clamped to 0–1, and R16G16B16A16_FLOAT is clamped to 0–1. Uncompressed textures in modes 1–3 usually hold mip 0 only, whatever the mip field says; the mip-count correction now also covers texture arrays. One R16_FLOAT array (8 slices) still decodes wrongly. **Content-pack stores**: the 11 stores under `pack/<name>/data/ttdb` (4,654 textures, 1.2 GB) hold the textures of the models in that pack's archive (story000 map models, minigame maps, core_restofworld peds and props). The exporter and the texture viewer use them since 2026-11-21, where they show as `<pack>/<store>` (`story000/ttdb_maps.ttpf`). On 40 sample models each, missing textures dropped from 944 to 135 (story000), 288 to 96 (shared_minigame000) and 344 to 234 (core_restofworld, where one placeholder hash used by gore and body shaders makes up most of the rest). Only 4 hashes are in both a pack store and data/ttdb, with the same texture. |
| Models (drawables `.#dr`, fragments/vehicles `.#ft`) | **works (geometry, LODs, skeleton, skinning); partial (materials)**: one GLB per LOD (`model.glb`, `model_lodN.glb`), with triangle meshes, normals and every UV set (UV0–UV3), one node per geometry, Y-up, metres. Fragments carry their **skeleton as glTF joints with correct pivots** and a **skin** (JOINTS_0/WEIGHTS_0 from the game's bone streams; u16 joints for skeletons over 255 bones, fixed 2026-11-20: earlier vehicle exports have wrong bones on interior, lamp, window and bumper geometries and should be re-exported). Checked in a glTF viewer (three.js) on `phoenix_mid` and three map pieces: shaded, normals, UV checker, joint markers, textured. **Materials link `ttpf` textures** (base colour and normal map, picked from the parameter hash and format) where they decode: on the map samples about half the materials get a base colour and a normal map. **Shader and parameter names** resolve since 2026-11-21 (all 344 shaders, about 75 % of parameter uses; see `sixng names`); texture names are still 64-bit hashes. **Tangents** (`TANGENT`, decoded from the game's tangent stream) are written where the mesh has one. Vertex colours are raw. Both models that had no geometry in the placement test now decode: a second small system stream is now detected, which recovered geometry for about 10 % of multi-stream models that had none. **Cables and pool/fountain water** (float-position vertex blobs) decode since 2026-11-20; cables are widened into crossed 2 cm ribbons because the game widens them in a shader. Since the same date, a stricter system-stream rule recovers **fragment props** and some **64 m `_hd` ground tiles** whose graphics page was being read as system data. In a BeamNG test box this took the models with no geometry from 74 to 0. Fragments get a real bbox in `model.json` (`drawable.bounds`; `bounds_raw` is meaningless on fragments). Occluder and water-vegetation LOD buffers are still not decoded. **LOD levels** come from the main drawable's LOD group since 2026-11-20. Earlier exports ranked every model array, so fragment props (streetlights, signs) got a small base plate or bracket as `model.glb` and buildings got trim strips as `model_lodN.glb`; re-export those. Embedded-drawable models now go to `model_parts.glb`. **Two-layer blend materials** (walls, ground pieces; since 2026-11-22 also `prop_2lyr_batch` 89c8fe07 on rocks, containers and barriers) get a `multilayer` block: base/layer-2 colour, normal and data textures, UV set and weight channel (COLOR0.a). **Four-layer ground** (`standard_uber_4lyr_*`: 61e09df7, b21f1ba5, 360e396b, 27e5c49d; since 2026-11-22) gets a `multilayer` block with `family: "four_layer"`, also in the GLB material `extras`: per layer colour / normal / data textures and UV, the section-wide `lookuptexture` that weights them (TEXCOORD_0; R, G, B → layers 0–2, the rest → layer 3) and the section-wide tint, tint-normal and grass-mask maps. A top-down render from these fields matches the game's baked low-detail map of the section. `multilayer.uv_fit` (all multilayer materials) measures each UV set as a plane over model position (metres per unit, planar or not), and `detail_uv.recommended` uses it: TEXCOORD_0 × 64 on map section tiles, a planar detail set (TEXCOORD_1 at 3 m per unit on the street-trash grounds) on set pieces that reuse the ground shaders, else position / 8 m. `multilayer.params` adds the material's shader constants: `per_layer[k]` holds the vec4s whose component k follows layer k's texture (hash → value; meanings not decoded, see `per_layer_notes`), `material` the rest. |
| Strand hair and fur (grooms) | **works (strand geometry); not linked to peds**: `sixng grooms [packs] --out <dir> [--roots] [--ribbons MM] [--only HEX,…] [--list]` finds the **20,010** strand grooms among the unnamed resources of `packs.rpf` (hair, beards, brows, lashes, stubble and animal fur) and writes each as a GLB, one polyline per guide strand (**5.57 M strands, 35.3 M points**, about 0.8 GB, under a minute), in model space so they sit on the head drawables. Lines carry the per-point width (`_WIDTH`, about 0.5 mm along the strand, about 3 mm at the root; inferred) and the strand's root UV (`TEXCOORD_0`, inferred). `--ribbons` writes crossed ribbon meshes instead, which render in any viewer. `--roots` adds the scalp triangles between strand roots. `--list` writes `catalog.json`: every groom with its bones (named for 97 % of references), root box, body region (scalp, face, brows / lashes, body, animal, other) and LOD chain (5,915 chains). No resource links a groom to a ped (the hair, beard and brows placeholder drawables carry only their own name hash), so the **model viewer's "Hair and fur" panel** lets you pick grooms by region and bone and shows them on the current model with a chosen width and colour (`--hair HEX,…` for screenshots). The other groom data (per-strand and per-point attributes, render-strand weights) is not decoded. |
| Cloth and soft bodies | **works (simulation mesh)**: `sixng cloth [packs] --out <dir> [--only HEX,…]` decodes the **13,067** verlet cloth resources (flags, awnings, tarps, garment pieces; 702 of them soft bodies around ped hips and chests) to GLB lines: particles (x, y, z, w) and constraint edges (rest length, stiffness?, flags), both particle / edge sets. **1.00 M particles and 4.45 M edges** in the first set, 0 errors, 30 s. An edge's rest length equals its particles' distance on a median 85 % (cloth) and 97 % (soft body) of edges. `index.jsonl` lists the counts, bounds and the names found in each (tuning profiles, bones). The rest of the cloth data (per-instance structures) is not decoded. |
| Text (UI, subtitles, item names, speech) | **works**: `sixng text [data] --out <dir>` decodes the localised text tables (unnamed 0x7a resources, one copy per language): **14,482 tables with 4.21 M strings** in `data.rpf` (21 s) and 78 tables with 23 K strings in `pack/shared_minigame000` (the arcade minigames' subtitles), 0 errors. Each table becomes `<offset>.json` (label hash -> string) with `index.jsonl`. Labels are joaat(lowercase label); a subtitle's label is its voice-line name plus a 2-digit variation (`AG_SUR_GEAR_FIRST1_01`). `--label NAME,…` prints a label's string from every table (every language), `--plain` drops the inline markup (`~sl:…~` subtitle timing, `~b~`, `~n~`, button icons). Which table is which language is not stored; most label names are not recovered. |
| Head identity deltas | **works**: `sixng head-deltas [packs] --out <dir> [--build] [--only HEX,…]` reads the **6,757** per-head vertex offsets from each class's base head (`head_<class>_base_000_*`, LOD0-3), recovers their vertex order from heads that exist as drawables and matches every delta to its head: **1,198 of the 1,208 head drawables** reproduce to a median **2.7 µm**. Many head drawables share one shape (several heads, one mesh and identical deltas). **434 heads have deltas but no drawable** (mr1 263, fr1 148 and a few others; 247 more LOD0-only deltas sit apart), and `--build` writes each of them as a GLB (673, 0.5 GB, 40 s): the base head with every vertex moved, or the head's own drawable where its name follows from the order. `index.jsonl` gives each delta's class, LOD, matched head and the two matched heads around it in name order. Base heads have no colour texture in the install, so the new heads are untextured unless named. |
| Vehicles | **partial**: the body exports with pivots, skin and LODs; `^phoenix` gives 248 parts with 0 errors, including the `_dmg_mod_*` damage parts and mod-kit parts. **Wheels are shared models** (`rim_*`, `tyre_*`) placed at `model.json` → `wheel_mounts`. A test fit in all four arches looks right. Since 2026-11-21 `sixng vehicles` gives the rim and tyre of each spawn configuration and the per-model `wheelScale` / `wheelScaleRear`. The model viewer fits the configuration's rim and tyre and sizes them as 1.625 × `wheelScale` (front and rear separately). The factor is fitted by eye on phoenix (0.37 at wheelScale 0.2277); mesa and ridgway look right with it too. It is not the game's verified rule. **Textures**: 33 of `phoenix_mid`'s texture hashes now decode: interior, lights, trim, the shared detail masks (dirt, grime, scratches, decals) and normal atlases. **Paint**: the body's colour slot is an engine-default white texture; the colour is applied at runtime from the vehicle tables (see Car paint below). Handling: `sixng handling`. **Vehicle data** (2026-11-21): `sixng vehicles packs --out <dir>` writes `<out>/vehicles/vehicles.json` from the game-data meta resource, covering 373 models (305 named). Each has `model_info` (vehicles.meta-like, about 50 GTA V names resolve: `wheelScale`, `wheelScaleRear`, `wheelType`, `handlingId`, `txdName`, `lodDistances` …), `configurations` (4,063: body colour plus the part models, with one `rim_*` and one `tyre_*` in 3,339), `palettes` (2,377 weighted paint-colour lists of 1 to 400 entries; colours repeat, e.g. phoenix's 400 entries hold 10 distinct colours, so repetition reads as the draw weight) and weighted `part_groups`. |
| Textures in unusual formats (R32 float, R10G10B10A2, …) | **skipped with a warning**. The texture JSON is still written; DDS export works where the format is known. R16_FLOAT and R16G16B16A16 decode since 2026-11-21, including the one R16_FLOAT array stored at twice its size (every other KiB unused, its own element order; 256×32, 8 slices, a lookup table). None of these remain in the bulk stores. |
| Map placements | **works**: `sixng placements` decodes the map files (self-describing `0x7a` resources, the GTA V `.ymap` equivalent): **2,249,490 entities in 30,785 files** in `levels.rpf`, 95 % resolved to a named, exportable model. The count rose from 1.47 M after the resource stream-split fix. Position, rotation, scale, flags and GUID are decoded. **LOD hierarchy**: each entity has `lod {level (HD/LOD/SLOD1–4/ORPHAN_HD), parent_index, parent_instance, num_children, lod_dist, child_lod_dist, priority}`. After `sixng lod-links` it also has `archetype_lod_dist`, `hd_texture_dist`, `effective_lod_dist` (lod_dist -1 means the archetype's) and a resolved `parent`: an LOD entity in another map file, or an instance of a `*_batchlod` prop batch (99.0 % of 1.05 M batch-linked HD entities and 99.9 % of 215 K entity-linked ones resolve, each with a method and confidence). **Interiors**: each file has a `space`. `world` files hold world positions. `interior` files (961 interior archetype files, 540 K entities) hold interior contents in interior-local space, which the world places only through interior instances (`sixng interiors`). `group` files (6.3 K, 40 K entities) are group-local. Interior entities carry `interior {interior, room, entity_set}`; `sixng interior-entities` also writes them to `work\index\interior_entities.jsonl`. Other fields are kept by hash. `data.rpf` and `packs.rpf` contain no map files. Checked visually: two map files (a tower block with its base, walkway and planting, and an industrial site) rendered from their exported models line up. |
| Collision (physics bounds) | **works (geometry); partial (surface names)**: `sixng collision [levels]` decodes the unnamed bound resources in `levels.rpf` (the GTA V `.ybn` equivalent: composite → geometry bounds with triangles, boxes, spheres, capsules, cylinders) to one GLB per file in world coordinates, **split into one primitive per surface id**, plus `index.jsonl` (bbox, polygon counts, materials, primitive list) and `surfaces.json`. Surface ids are 15-bit hashes of the names in the game's material list (`ROAD_TARMAC`, `TRACK_GRASS_SHORT`, `METAL_RAILING` …); the hash function is unknown, so names are not attached yet. Checked visually: a building with window openings, a car park with railings and pillars, and terrain with coherent surface patches. A sample of 400 entries gave 353 bounds; the whole archive is about 87 K files, ~15 GB of GLB. |
| Lights | **works (position, colour, direction); partial (other attributes)**: `sixng lights [levels]` follows the light extensions on archetypes and entities (GTA V `CExtensionDefLightEffect` / `CLightAttrDef` equivalent) through the meta pool references and writes `lights/levels/sets.jsonl` (**136,466 light sets, 274,819 lights** with local position, RGB colour, direction/tangent and every other field by hash) and `world.jsonl` (**624,184 placed lights** in world space, transformed by each entity that uses the archetype). Checked visually: the whole-map plot reads like a night satellite view, and a 1 km city cell shows lamp rows along both edges of a bridge and the arterials, and grids over car parks. The light fields are now named from their own member hashes (case-sensitive joaat of the GTA V `CLightAttr` names): `intensity`, `falloff` (range), `falloffExponent`, cone angles, corona, volumetric and fade distances, `timeFlags`; 774,847 placed lights including interiors. ~1 min, ~0.7 GB. |
| Navmesh, road surface | **works (navmesh, road polygons, lane graph)**: `sixng navmesh [--roads-only]` decodes the navmesh tiles in `navmeshes.rpf` (GTA V `.ynv` equivalent, 128 m tiles) to GLB in world coordinates, one primitive per polygon flag; **flag 0x2000 = road surface** (checked visually). `sixng paths levels` writes the road/path-node graph (GTA V `.ynd` equivalent): 63,808 nodes with speed limit, street hash, **named node flags** (34 of 51: NoGps, Junction, InTunnel, Highway slip / exit, Stop, Yield, Bridge, WaterNode …), road class and density; 145,518 links with length, lane counts per direction and named flags (GpsCanGoBothWays, LeadsToDeadEnd / LeadsFromDeadEnd from the game's names; unpaved / track, two-way and no-ambient-traffic (bit 6) inferred); and **1,546 junctions** with entrances, turn paths and, for 369 signalised ones, **traffic-light phases** (duration and the placed traffic-light props, matched by entity hash for 99.6 %). |
| Audio (`audio.rpf`) | **works (Opus streams, multichannel music, PCM; XMA2 as lossless remux)**: `sixng audio` finds the sound banks (`ADAT` containers, the GTA V `.awc` equivalent) by scanning the archive (~2.5 min, index ~30 MB): **7,160 banks, 1.6 M streams, about 1,300 hours**. Opus streams (99.7 %) export as **Ogg Opus** (lossless remux) or **WAV** (decoded with libopus, 16-bit, 48 kHz) and play in the GUI's **Audio** page. Multichannel music (3,008 streams) holds 1 to 8 sub-streams (stems), each its own mono or stereo Opus stream; it exports as one `.opus` or `.wav` per sub-stream (`<bank>_<stream>_<sub-stream id>`), but does not play in the GUI. The codec id is the byte at 0x1C of the format chunk: besides Opus there are **1,818 XMA2** streams (mono, 32 kHz), **227 float PCM** and **19 16-bit PCM** streams. PCM exports as **WAV** (samples as stored, bit-exact) and XMA2 as **`.xma`** (RIFF WAVE with an XMA2WAVEFORMATEX header, a lossless remux of the 2 KiB XMA2 packets, readable by standard XMA2 decoders). Indexes from older versions read the codec from the wrong byte and must be rescanned (`sixng audio --rescan`, or Scan in the GUI). Stream names are 29-bit hashes and are **not recovered**; streams are listed by bank offset and stream id. The Opus encoder delay is taken as the default 312 samples and the end is trimmed to the stream's sample count. |
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

**macOS** (minimum macOS 11.0; Apple Silicon, tested on macOS with Rust 1.100):
the same `cargo build --release` works once CMake is installed
(`brew install cmake`) and uses the existing WGPU renderer.
`cargo build -p sixng-gui --release --features metal4-poc` opts into the
experimental native Metal 4 window. It is used only on macOS 26 or later with a
Metal 4-capable GPU; otherwise the app falls back to WGPU. Some GPU-dependent
views are not yet ported to Metal 4. `scripts/macos-app.sh` builds
`dist/SixNG.app` (GUI and CLI, ad-hoc
signed, not notarised); `scripts/macos-app.sh --universal` makes it run on
both Apple Silicon and Intel Macs (needs `rustup target add
aarch64-apple-darwin x86_64-apple-darwin`). The app keeps its settings and default work folder
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

# A character as ONE rigged GLB (one armature in Blender): its outfit parts on one shared skeleton,
# optionally with an animation clip. --list shows the parts; --parts / --add pick others (regexes)
.\target\release\sixng.exe outfit player_story000_00 --list
.\target\release\sixng.exe outfit player_story000_00 -o work\out\lucia.glb --clip "gameplay@locomotion@female_character@kat@normal@standing@walk@stance_neutral/move"
# Game text: all text tables to JSON, or one label's string in every language
.\target\release\sixng.exe text data -o work\out
.\target\release\sixng.exe text pack__shared_minigame000__shared_minigame000 --label AG_SUR_GEAR_FIRST1_01 --plain
# Animation clips in anim.rpf: find dictionaries (first run ~20 s, cached), list a dictionary's clips
.\target\release\sixng.exe clips --name "male_character@.*@walk"
.\target\release\sixng.exe clips "gameplay@locomotion@male_character@raul@normal@standing@walk@stance_neutral"
# Every character and animal `outfit` can build (name, kind, archive)
.\target\release\sixng.exe characters --kind animal --name deer
# An animal (root fragment + its parts on the animal's own skeleton), with two clips as two animations
.\target\release\sixng.exe outfit crb_deer_whitetailed_m_00 -o work\out\deer.glb --clip "gameplay@creatures@land@mammal@deer@normal@run@fast/move,gameplay@creatures@land@mammal@deer@idle/idle"
# Clip-only GLB (skeleton + animations, no meshes or textures) for the model above
.\target\release\sixng.exe outfit crb_deer_whitetailed_m_00 -o work\out\deer_walk.glb --anim-only --clip "gameplay@creatures@land@mammal@deer@normal@walk@slow/move"
# An ad-hoc character: any name plus --parts (regexes over every drawable in the archive)
.\target\release\sixng.exe outfit hiker_m -o work\out\hiker.glb --parts '^head_mr1_cau_000_m_full','^hand_mr1_000_tshirt','^feet_mr1_000_shortsmid','^tshirtcrewneck_mr1_000.*_full$','^shortscargo_mr1_000','^bootshiking_mr1','^backpackhiking'
# Particle effect dictionaries: names, rule names, keyframe curves
.\target\release\sixng.exe ptfx data --name scr_arc -o work\out
# UI fonts (Scaleform font libraries) as TrueType + three.js typeface JSON
.\target\release\sixng.exe fonts data -o work\out
# A vehicle with its body colour (sRGB hex; e.g. srgb_if_linear from vehicles.json)
.\target\release\sixng.exe extract levels --name '^stanierg2$' -o work\out --paint 8a1a12

# Built-in categories: textures | map | vehicles | meta
.\target\release\sixng.exe extract-category vehicles --name '^phoenix' -o work\out
.\target\release\sixng.exe extract-category map --limit 200 -o work\out --dry-run
```

**Characters** (`sixng outfit`, since 2026-11-22): the parts of a character
(the GUI's default look: head, eyes, hair, top, outer, bottom, shoes …, or
`--parts`) are skinned to the shared ped skeleton plus their own helper
bones and written as **one GLB with one skin**: the joint lists are merged
by name (a helper bone that two parts define differently is kept twice,
the second as `<name>.<part>`) and each part's vertex joints are remapped
to the merged list. `--clip <dictionary>/<clip>` adds the clip as a glTF
animation: each bone track drives the joint whose joaat(lowercase name) is
the track's bone id (rotation, translation, scale tracks; the mover / root
motion and float tracks are left out), sampled at the animation's own
frames on the clip's time line. Joint nodes are written as TRS so they can
be animated. The JSON report gives per part the vertices, joints and
weighted influences whose bone is in no skeleton, and for the clip how many
bone tracks land on a joint. Checked on five characters (player_story000_00
and _01, cs_raul, cs_iris, u_m_m_farm_monster_man) with female and male walk
clips: Khronos validator 0 errors; Blender 5.2 imports one armature with
every skinned mesh on it; rest-pose skinning matches the stored vertices
within 3e-7 m; 95.6 % of the clips' bone tracks land on a joint; joints
whose own translation is not animated keep their length to the parent
within 4e-7 m over the clip; rotation keys sit a median 3.6° from the rest
pose (22° for the conjugate). Not done: the mover (root motion), facial
animation, hair (most hair parts are 3-vertex stubs), cloth simulation.
One field jacket names 16 palette bones that are in no skeleton found yet
(2,161 influences, weight moved to the other influences).

**Animals and ad-hoc characters** (since 2026-11-22): an animal's root
fragment (`crb_eagle_bald_m_00`, `crb_deer_whitetailed_m_00`) holds its
skeleton, and its parts are the drawables named `<root>_<part>_NN`. `outfit`
skins them to that skeleton. Parts bound to one bone (eyes, tongue, rigid
props) are attached to that bone. The 3-vertex `fur` placeholders are left
out by default; `--add fur` brings them back. A name that is not a known
character plus `--parts` makes an ad-hoc character from any drawables, for
example the generic hikers. `--clip` takes several clips (comma-separated or
repeated), one glTF animation each. `--anim-only` writes only the skeleton
and the animations. `sixng characters` lists every character and animal
`outfit` can build (327 characters and 263 animals). The animal clips were
checked by rendering (eagle flap, deer walk/run/idle: 91–97 % of tracks land
on a joint). Animals have a 180° yaw on `SKEL_ROOT` in the bind pose that
the clips do not carry, so an animated animal faces the other way from its
bind pose.

**Particle effects** (`sixng ptfx`): the effect dictionaries are unnamed
resources with small embedded `keyframeTexture`s (16 × 4 half-float RGBA
curves) and no geometry (146 in data.rpf). The command lists each
dictionary's name (`scr_arc_tl`), its effect and emitter rule names, and
with `-o` the keyframe curves and the rules (`rules` in `<offset>.json`):
each effect's events, which pair an emitter rule with a particle rule
(`scr_arc_svr_blood_melee` fires decal, mist, main, splash, droplets and
burst); every emitter rule's ten keyframe properties and every particle
rule's properties grouped by behaviour, with keys `[time 0..1, value vec4]`; and the index of each particle
rule's keyframe texture (null for the 7 % that have none). Property ids are
joaat of `<class>:m_<member>KFP`, so names resolve: every effect property
(`ptxEffectRule:m_colourTintMinKFP` …) and emitter property
(`m_spawnRateOverTimeKFP`, `m_particleLifeKFP`, `m_sizeScalarKFP` …) and
about 70 % of behaviour properties (`ptxu_Size:m_whdMinKFP`,
`ptxu_Rotation:m_angleMinKFP`, `ptxu_Light:m_rangeKFP` …), which also
names each behaviour's class. The colour over life is in two unnamed
`ptxu_Colour` properties (RGBA, min/max pair; marked with `role`). The
keyframe textures are not the behaviour curves (no row matches one); they
look like the particle shader's variables. All 146 dictionaries decode (2,816 effects, 11,228
events, 8,224 particle rules). `texture_refs` lists the 64-bit values that
probably name each particle's sprite; no texture store holds them (hash
function unknown) and the sprite sheets are not embedded.

**UI fonts** (`sixng fonts`): the Scaleform font libraries in data.rpf
(`fontlib_efigs`, Chinese, Japanese and Korean) hold their fonts as SWF
DefineFont3 outlines. 29 fonts are written as TrueType (`.ttf`, for CSS
`@font-face`) and as three.js typeface JSON. Several are subsets of a full
font (digits only, a few icons). The fonts belong to their owners; use them
locally with your own install.

**Emissive and paint**: materials of the emissive shaders (`standard_emissive`,
`standard_emissive_layer` …, by recovered shader name or known family) get
their colour texture as `emissiveTexture` in the GLB, so lamps, neon and
screens light up in viewers and engines. `--paint RRGGBB` (or `AARRGGBB`)
sets the colour of car-paint materials (linear `baseColorFactor`, and
`paint.colour` in `model.json`). Map wreck props (`prop_am_wreck_*`) have no
car-paint material: their colour is in their textures.

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
  hair, top, jacket, trousers, shoes …; `sixng outfit` exports the same
  set as one rigged GLB). Single ped pieces are hidden as
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
- **Head skins** (since 2026-11-21): ped heads bind no textures in the
  model, so SixNG adds the face (colour and normal map). For about 900
  heads (named characters such as `cs_jada_watts` and most ambient
  `head_*` faces) the game's material data names the head, and that face
  is used. Other heads (`a_head_base_*`, crowd `hc_*` heads, characters
  whose materials are in archives not indexed yet) get a face of the same
  sex, body type and ethnicity from that library. The viewer shows
  **Head skin ◀ n / N ▶** to switch through the faces; exports take the
  **Head skin** number next to Body shape, or `extract --head-skin N`.
  `model.json` records the choice (`head_skin`: linked or auto, index,
  count, texture hashes). The table is built from the `packs` index on
  first use (about 5 s) and kept in `<work>/index/head_skins.json`.
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
  copies. Clothing pieces that come as several entries with one name keep
  one (the largest); pieces of a named character are listed through its
  outfit entry.
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
  per model id, all its placements as instances. Loads go nearest surface
  first (size only breaks near-ties), on screen before off screen;
  placements outside the view (widened a little) are not drawn but stay
  loaded for turning round. Sharper texture reloads wait until every wanted
  model has its first load, so a complete view comes first.
  The workers (¾ of the cores, 2–12) run below normal priority so the
  frame keeps its CPU. GPU uploads run on one upload thread at normal
  priority, and replaced or evicted models are freed on another thread,
  so neither stalls the frame. A model's meshes share one vertex, one
  index and one material buffer.
- **Smooth streaming**: the visible set is worked out on all cores. While
  the camera moves, models in range of where it will be in 2 s (up to
  800 m ahead) are loaded but not drawn yet. A placement already drawn
  keeps its place until it is 6 % past its limit (no flicker at the
  edge), and a parent LOD/SLOD that its children would replace stays up
  to 2.5 s longer while buildings in its 128 m cell are still loading, so
  hand-offs do not open holes. A child already drawn stays out to 1.5×
  its distance while its parent's model is still loading. Eviction runs
  in 3 ms slices, not in one burst.
- **Map cells** (far placements merged per area): a placement more than
  0.6× the HD distance from the camera can be drawn from its cell instead
  of one by one. A cell holds one LOD level's placements in one square of
  the map (128 m for HD, 256 m for LOD, 512–2048 m for SLOD1–4), merged
  into a few batches with one draw each:
  - geometry is placed in the cell, from the low drawable LOD where a model
    has one, and simplified by vertex clustering to about two pixels at
    the nearest distance the cell draws it;
  - base-colour textures go in at 64 px, in BC3 texture arrays;
  - each vertex carries its placement's index. The vertex shader checks
    that placement's own LOD distance range, so LOD hand-offs stay per
    placement.

  Built cells are cached in `<work>/gui_cache_v4/cells` (capped at 4 GB),
  so a revisited area loads with one file read per cell. Until a cell has
  loaded, its placements load and draw one by one as before. A cell the
  camera comes close to keeps drawing until its placements' models have
  loaded. LOD and SLOD cells are small and built on demand. HD cells are
  the heavy ones (a downtown HD cell is several MB), so new ones are only
  built while the camera is slow (under 15 m/s); while flying, only HD
  cells already on disk are used. Cells need BC textures and the default
  view settings (every kind and LOD level, no proxies);
  `SIXNG_MAP_NO_CELLS=1` turns them off.
- **Texture streaming**: a model first loads with 64 px base-colour
  textures only (normal and data maps come with the sharper reload), read
  from block-compressed 64 px copies in `<work>/gui_cache_v4/bc64` (a few
  KB each, capped at 1 GB; about 20× faster than decoding a PNG), else
  from reduced PNG copies in `lowtex128` (≤ 128 px, capped at 2 GB) and
  `lowtex` (≤ 256 px, capped at 4 GB), all written on first use. It then reloads with sharper textures as the camera comes
  closer (full size in the HD band, then 1/2, 1/4, 1/8). Textures are
  shared between models and go to the GPU block-compressed (BC1, or BC3
  with alpha: 4–8× less GPU memory) where the adapter supports it;
  `SIXNG_MAP_NO_BC=1` uploads them uncompressed.
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
- **Coverage fallbacks** (no holes near the camera): a SLOD placement (or
  a LOD with HD fallback off) with nothing finer to hand over to (no LOD
  link, or children that all lack a model) keeps drawing nearer than its
  own band, but only on pixels no real surface has covered. Real surfaces write a stencil
  mark as they draw; the fallbacks draw after them and only where the
  mark is missing (far-field terrain, ocean or sky), with back faces
  culled so a coarse block the camera is inside stays invisible. Before,
  such areas showed flat far-field terrain next to the camera while the
  same LODs farther away were drawn. Needs the 32-bit depth + stencil
  format (most desktop GPUs); without it the fallbacks draw plainly.
- **Building glass**: glazing materials carry only a light dirt texture
  (the game adds reflections and interiors behind them), so on the map
  they are drawn as dark, mostly opaque reflective glass; the model viewer
  keeps the see-through look.
- **Whole-map terrain** (far field): the whole map is always drawn, out to
  the horizon, as a quadtree of tiles over the baked 8 m top-surface
  heights, coloured with the 2D map's terrain tiles (coarse tiles for
  coarse nodes, so the colour sharpens as nodes refine). Nodes refine by
  screen-space error (**terrain error**, px) and frustum; children replace
  a parent only once all four are loaded, so nothing pops out. Beyond the
  SLOD3 band the tiles keep the roofs (max-pooled heights), so buildings
  show as blocks instead of a void; nearer, where buildings or their LODs
  are drawn, a tile switches to ground heights (a morphological opening
  that removes buildings) once at least 90 % of the building and LOD
  models on it have loaded, so coarse blocks do not swallow the LODs.
  Ground tiles draw in the coverage pass (only on pixels no real surface
  covers, so they never cover a street or sidewalk that sits a little
  lower) and 2 m above the ground estimate, so where the map data has no
  ground model near the camera, the 2D map's colours show instead of a
  pale coarse SLOD slab. Coarse SLOD placements near the camera also load
  their texture atlas at full size (they cover a whole area with few
  texels per metre). Built in the background from the map data already in
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
- **GPU memory**: the dedicated and shared GPU memory of the process are
  read once a second from the Windows performance counters. When the
  driver starts moving the map into shared (system) memory (more than
  384 MB), the streamer takes 85 % of what was in use at that point as the
  GPU budget and sheds models above it, like the RAM caps. Freed models
  give their GPU memory back: the renderer records no render bundles,
  because the graphics library version in use never frees a bundle, nor
  anything it draws.

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
| Vertex colours (tint, AO bake source) | **partial** | `_COLOR0` holds the raw RGBA8 of stream 1. Blue is the tint palette index (see textures above; applied as `COLOR_0`); alpha is the layer-2 weight on multilayer shaders. Red and green are still unverified, but narrowed down (4,000 sampled models): red looks like a baked shading term (mostly 255; soft dark blobs on ground tiles where trees stand outside the model; 23 on average on trees) but is ~252 on interior shaders, so it is not sky occlusion alone, and ray-cast sky visibility does not explain it, neither from the model's own geometry nor from the neighbouring placed world (two ground tiles with all models within 120 m: correlation about 0), and neither do nearby instanced trees, the grass batches' AO byte or distance to the road. Green equals red on 63 % of ordinary vertices; on ground shaders it is low and varies widely within one collision surface (standard deviation about 54 of 255 on a road surface), so it is not a surface-type mask either. Streams 12 and 13 (also RGBA8) are not exported. |

#### Materials

| CSP need | Status | Notes |
|----------|--------|-------|
| Stable, readable mesh and material names | **partial** | The names are stable (`geomNN_<offset>`, `shaderNN_<hash>`, `textures/ttpf/<hash>.png`). Shader names (all) and most parameter names are in `model.json` and the material `extras` (`shader_name`, `param_name`); texture names are unresolved 64-bit hashes. |
| The game shader or material class per material | **works (name); partial (classified)** | Every material has its game shader name (`shader_name`, e.g. `standard_decal_hbb`, `ped_standard_skin`, `standard_vehicle_glass`). Each material in `model.json` has a `classification`: `class` (opaque / alpha / cutout / decal / glass / window / emissive / water / terrain_multilayer / car_paint), `render_bucket` from the shader record flags, `alpha_mode` (glTF OPAQUE/MASK/BLEND, also set in the GLB), `instanced_or_batch_lod`, the shader `family` where known, and for decal-bucket materials `decal {bias_m, order, order_source, depth_write}`: draw after opaque geometry in ascending `order` (terrain blend layers 0, other decals 1; no per-material priority is known), offset by `bias_m` toward the camera. Buckets come from flags measured over 254 K drawables. Glass, window, emissive and terrain classes come from a list of known shader families. Emissive has no flag, so unknown emissive shaders show as opaque. |
| PBR maps (albedo, normal, roughness, metal, AO) and channel layout | **partial** | Base colour and normal map are linked in the GLB when they decode. `model.json` → `materials[]` lists every slot with its parameter hash, format, size, tile mode, `role_guess`, `slot_role` (what a known slot holds), and the PNG path or why it is missing. **Normal maps are DirectX (green down)**: invert G for glTF/Blender/OpenGL. Engines that use the DirectX convention take them as they are; check each converter. Roughness/metal/AO sit in packed linear slots (`739e3d0d`, `c6f46e79`). They are exported as PNG but not mapped. Channel statistics and a visual sheet suggest `c6f46e79` = R binary mask (metal?), G roughness or gloss, B AO/cavity, A unused; this is not confirmed. |
| Emissive flag and emissive textures | **partial** | Shader names now show the emissive families (`standard_emissive*`, `signs_emissive`, `*_emissive_layer*`) and the `emissivetex` slot; the viewer's classification does not use them yet. |
| Transparency / alpha-blend flag (glass) | **unknown** | It is probably in the shader record's +0x38 bytes or the draw bucket, which is not decoded. |
| RainFX classes (asphalt, lines, grass, glass, cloth) | **partial** | `surface_hint` from model names only. |
| Terrain RGBA blend masks, multilayer layers | **partial** | Two-layer families: `multilayer` with COLOR0.a as the weight (on `prop_2lyr_batch` the reverse, 1 − COLOR0.a: unpainted A = 255 means base only; its dirt overlay follows TEXCOORD_2, from the same seam test, and on rocks TEXCOORD_2 is parked at one point, so their dirt is uniform). Four-layer ground (`standard_uber_4lyr_*`): `multilayer.family = "four_layer"` with the lookup (splat) texture, its channels per layer, and the macro maps; the channel order is from a seam-continuity test (medium confidence for R/G/B, low for layer 3). Per-layer tiling factors (in the shader constant block) and the height-based blend are not decoded. |
| Window, water, screen and horizon tagging | **partial** | Shader names identify glass (`standard_vehicle_glass`, `standard_glass_breakable*`), water (`standard_alpha_water`, `water_micro_geom`, `standard_alpha_waterfall`) and signs; not yet used for tagging. Water type (sea, lake, pond, pool) is not located. |

#### Map

| CSP need | Status | Notes |
|----------|--------|-------|
| Instancing and placements (archetype, transform, LODs) | **works (transform); partial (LODs)** | `sixng placements` writes every entity with its model name, entry id, a ready-made glTF transform and its tint palette row (`tint_row`, see textures above). Models are still exported once per archetype, never baked. LOD parent/child links are in the per-entity fields but not interpreted. Interiors: `sixng interiors` gives each of the 2,590 interior instances a world transform and its entity list. **Instanced batches** (grass, weeds, debris, fences, reflectors, `*_batch_slod` LODs): `--batches[-only]` writes `batches.jsonl` (one line per batch: archetype, model name, bbox, LOD distance, `first` / `count` into the binary) plus `batch_props.bin` (44 B per instance: position f32×3, quaternion xyzw f32×4, scale f32×3, tint RGBA u8×4) and `batch_grass.bin` (20 B per instance: position f32×3, normal x/y u8, colour u8×3, scale u8, AO u8, pad). All 906,922 batches decode, 99.76 % with model names. |
| Trees and bushes separately, with LODs and placements | **works (placements); partial (LODs)** | Tree and plant models export like any drawable, and their instances (e.g. `veg_pot_hedge_lrg_02` × 25 in one sample) come out of `sixng placements` with scale. |
| Light emitters (position, colour, range, cone, night flag) | **works** | `sixng lights`: 774,847 placed lights in world space, interiors included. Intensity, falloff (range), falloff exponent, cone, corona, volumetric and fade fields are named from the game's own field hashes. |
| Collision split by surface type; walls identifiable | **works (split); partial (names)** | `sixng collision`: one GLB per bounds file, one primitive per surface id, in world coordinates. Surface ids are unnamed hashes; inferred categories (`sixng surfaces`) cover 98.3 % of area, each with a confidence (90.8 % high or medium), from plants and props standing on each surface, path-link density, slope and height. Interior collision: `sixng interiors` links 70.7 % of interior archetypes (`collision_linked`, ~91 % precision on the ordered links); its second pass links the rest by fitting the interior's entities onto candidate collision floors, for 97.8 % in all, 79.3 % at high or medium confidence. |
| Dynamic props: mass, centre of gravity, collider primitives | **unknown** | Fragment physics data is not decoded yet. |
| Animated props, rail and tram paths | **partial (clips); unknown (props, paths)** | `anim.rpf` clip dictionaries decode since 2026-11-22 (`sixng clips`): all 61,506 dictionaries with 0 errors, 316,150 clips (294,822 bind animations; the rest are in dictionaries whose animations are not decoded yet), 191,031 animations, 12.7 M tracks (`examples/clipcensus.rs`). They apply to characters with `sixng outfit --clip`. Which clips drive props, and rail / tram paths, are not decoded. |
| Tunnel and bridge extents; billboard, TV and skyline meshes | **unknown** | |
| Road/lane graph, traffic lights, pit and start positions | **works (lane graph, junctions, traffic lights, road surface); unknown (pit/start)** | `sixng paths`: node graph with lanes, speed limits and named node flags (tunnel, highway slip/exit, no-GPS, stop, yield …); junctions with turn paths and traffic-light phases linked to the placed signal props; `sixng navmesh --roads-only`: road polygons per 128 m tile. Link flags: 4 of 11 names recovered, 2 inferred (unpaved / track, two-way). Start positions not found. |

#### Cars

| CSP need | Status | Notes |
|----------|--------|-------|
| Node hierarchy with pivots | **now** | Skeleton joints with model-space pivots, in `model.json` and as glTF joints. |
| Bonnet and boot as their own parent nodes | **partial** | `bonnet` and `boot` are joints with correct hinge pivots, and their vertices are skinned to them (slot 8). They are not split into separate mesh nodes: a converter can split by joint (most parts use one bone per vertex). Separate `_dmg_mod_bonnet_*` / `_boot_*` fragments also exist. |
| Skinning (bones and weights) | **now** | glTF skin: one joint per vertex for car bodies (slot 8, format 0x00), and up to 4 joints and weights for wheels and tyres (slot 8 format 0x25 + slot 6). Since 2026-11-21 ped clothing gets **8 influences** (JOINTS_1 / WEIGHTS_1 from slots 9 and 7; before, the second four were dropped) and is **bound to the right bones**: its joint indices go through each model's bone palette (sorted bone-name hashes) to the shared ped skeleton (596 bones, read from `ambient_femalebody` / `ambient_malebody` in `packs.rpf`, so `packs` must be indexed) plus the file's own `GM_U01_*` helper bones. Earlier ped clothing exports used the file's own bone order and are wrongly skinned; re-export them. Since 2026-11-22 the `GM_U01_*` helper joints hang under the shared bone their bone record names (u32 joaat at record +0x2C, e.g. `SKEL_Spine3`, `RB_L_Thigh01`, `SKEL_L_Clavicle`), with their parent-relative bind moved to model space, so they follow the body when it is posed or animated (sports top: helper joints 0.36 m → 0.07 m median from the vertices they drive), and the GLB skin passes the Khronos validator (before: `SKIN_SKELETON_INVALID`, joints with zero weight not 0). Some T-shirts name 8–9 palette bones that are in neither skeleton (warning with their hashes); their weight goes to the other influences. **Body shape** (since 2026-11-21): `extract --body-shape "gcweightbelly=0.5,gcmusclechest=1"` (or the Body shape field in the GUI) poses the shared ped skeleton with the game's own body-shape rig (`LOD0_Body_Shapes_Female` / `_Male` from the rig dictionary in `anim.rpf`, so `anim` must be indexed with this version too). Controls: `gcweight<region>` -1 (thin) to 1 (heavy) and `gcmuscle<region>` 0 to 1 for back, belly, butt, calf, chest, forearm, hip, muffin, neck, shoulder, thigh, upperarm, plus `gcbutt` and `gcbreasts`; `allweight=v` / `allmuscle=v` set a whole group. The shape is in the joint node transforms (the inverse bind matrices stay neutral), so Blender and other glTF viewers show it; the GUI model viewer draws the bind pose and does not. Checked on a female sports top (thin, heavy and belly look right, left/right symmetric). On male T-shirts with palette bones in neither skeleton the belly can come out as a spike. **Heads, jackets and other 0x8c-skinned parts** are skinned since 2026-11-22 (before, they came out with no skin): slots 8 / 9 format 0x8c hold 4 × u32 per vertex with the joint index in the low 12 bits (stream 8 keeps other data in the rest; 8 influences with slots 6 / 7), through the part's bone palette; low-detail copies whose palette names only shared bones are bound the same way. A part can hold **several bone arrays** (torso, upper-arm and forearm helper sets: a biker jacket has 69 + 32 + 24 + 16 + 9 bones) and its palettes name bones of all of them; all are now joints, which binds the arm helpers that used to count as "in neither skeleton" (biker jacket: 98 helper joints used, median 3 cm from the vertices they drive; 0 unresolved influences). The bind pose is model space. |
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

## Metal 4 rendering

The standard build uses the WGPU renderer. The experimental Metal 4 path is
opt-in with `metal4-poc`; it is selected at runtime only on macOS 26 or later
with a Metal 4-capable GPU. Otherwise the GUI falls back to WGPU.

```sh
cargo run -p sixng-gui
cargo run -p sixng-gui --features metal4-poc
cargo run -p sixng-gui --features metal4-poc -- --metal4-window
```

The 3D map renderer uses MTL4 for streamed models, far-field terrain, and
road/light overlays. WGPU-specific viewer features and screenshot capture are
not yet at full parity on the MTL4 path. The pinned `metal-rust` fork and its
Apache-2.0 license changes are documented in `THIRD_PARTY_NOTICES.md`.

## Licence

SixNG is free software under the [GNU General Public License v3.0 only](LICENSE).
Third-party notices (AMD addrlib, MIT): [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
