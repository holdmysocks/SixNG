//! Spec maps of ped skin: the `speculartex` and `speculartex2` textures of
//! the body, hand and feet meshes (and of any other skin a wardrobe item
//! names), from the game's material assets.
//!
//! The skin meshes (`hand_*` = upper body, `feet_*` = lower body) use the
//! `ped_standard_skin` shader. Every main mesh of every fit binds a colour
//! map and two spec maps (`speculartex` = slot `c6f46e79`, `speculartex2` =
//! slot `739e3d0d`) that are in no texture store: placeholders. Only the
//! normal map is stored. The wardrobe item gives the real textures: each
//! texture state of an item has an `Asset` (struct `147fdab5`) with
//! `Albedo`, `Normal` and `Material`, a 64-bit material asset id.
//!
//! The material assets are packs.rpf PSO files (47 of them), each an asset
//! set (root struct `1edee47e`) with hash maps of material records (struct
//! `28689f25`: `guid` and a keyed texture list `53760af2`). Two keys hold
//! the spec maps:
//! * `179eb44a` -> `speculartex`
//! * `4cf41ef4` -> `speculartex2`
//!
//! Checked on the stored first-person hands: the record of
//! `hand_fb1_000_nude_cull_fp`'s material names the very two textures that
//! mesh binds. Other keys of the list (`7afafb01`, `aaccd95c`, `c8126724`,
//! `aeadd0ac`, `c849b74a`) hold colour, normal and wrinkle textures; they
//! are not used here.
//!
//! A skin is picked by its colour texture, so the table maps each wardrobe
//! `Albedo` to its `Material` (`by_albedo`), and each material id to its
//! two spec maps (`materials`). A colour texture that states name with more
//! than one material (shared defaults, such as a jewellery colour map used
//! by about 300 materials) does not say which material is meant, so it is
//! left out and counted in `conflicts`. Built once from the packs.rpf index (meta
//! resources and PSO files) and cached as `<work>/index/skin_spec.json`.
//!
//! The channels, from the skin pixel program (`PS_DeferredPedTexturedNC`):
//! * `speculartex` R: translucency mask, `saturate(R - c)` written to a
//!   G-buffer target of its own (non-zero on thin parts such as fingers).
//! * `speculartex` G: roughness. Overlay layers blend it toward their own
//!   value, the detail map adjusts it, and the G-buffer stores 1 - roughness.
//! * `speculartex` B: occlusion, `lerp(1, B, strength)`, stored in 6 bits.
//! * `speculartex2` R: detail strength mask.
//! * `speculartex2` G: skin region (`G * 16`, high nibble 0-15). A
//!   per-draw table maps the region to a layer of the detail-normal
//!   and detail-control atlases.
//!
//! The forward studio program (`PS_StudioPedTextured_Eight`), the game's
//! own complete skin shading outside the deferred path, reads only
//! `speculartex` G and `speculartex2` R: roughness `r = G` (plus the detail
//! term), GGX alpha `r^2`, no metal, and F0 `0.04 * (1 + fresnel)` from the
//! material's `fresnel` constant (`specularfresnel`), with f90 =
//! `saturate(49.5 * F0)`. The GLB maps that as glTF does: the `speculartex`
//! PNG is the metallic-roughness texture (G = roughness, metallic factor 0
//! so B is not read as metal) and `KHR_materials_specular` carries
//! `1 + fresnel` as its specular factor (glTF's dielectric F0 is 0.04). The
//! raw maps stay in the material extras (`skin_spec`).
//!
//! The detail-normal and detail-control atlases are engine-wide: no mesh
//! binds them, and the material slots fall back to two 2D arrays of 48
//! 256 x 256 tiles in `data/ttdb/graphics/ttdb_graphics.ttpf`
//! (`DETAIL_NORMAL_ATLAS`, `DETAIL_CONTROL_ATLAS`). Tiles 16-31 are the
//! skin pores and wrinkles, 0-15 fabric. A material's region table
//! (`a6b9e97a`, three words of 2-bit fields) turns the region in
//! `speculartex2` G into a tile. The export writes the material's 16
//! tiles in region order as two 4 x 4 sheets (`detail_json`) and records
//! the strengths and the tiling. The game's use (both skin programs):
//! with `c` = control R and `mask` = `speculartex2` R,
//! `colour *= saturate(1 + strength.x * mask * (2c - 1))`, tangent normal
//! `xy += strength.y * mask * detail.xy`, at UV0 x `uv_scale`. Two values
//! of a per-ped engine record scale the normal and colour terms (taken as
//! 1); a per-tile engine value scales a roughness term (taken as 0).
//!
//! Occlusion (`speculartex` B) enters the G-buffer as
//! `lerp(1, B, strength)` with the material's constant `4d1ecedd`; the
//! lighting multiplies the diffuse ambient by it and the specular ambient
//! by its fourth root, the direct light not at all. The strength is 0 on
//! human body skin (hands and feet in every material read) and 1 on
//! teeth. Where it is above 0 the GLB gets an occlusion texture (B moved
//! to R, glTF's channel) with that strength.
//!
//! Translucency (`speculartex` R) goes to a G-buffer target of its own;
//! the lighting turns it into a thickness through a per-entity profile
//! and adds a Burley-style transmittance per light. The profiles are
//! loaded data not found yet, so it is recorded, not mapped.
//!
//! Both skin programs scale F0 by a window on the albedo's brightness,
//! `saturate((max(r, g, b) - lo) / (hi - lo))` on the linear albedo. The
//! bounds are in the per-frame buffer (`6e8725e4` + 0x6c0 / 0x6c4); the
//! engine fills them every frame from two constants, sRGB 5 and 20 out of
//! 255, so lo = 0.0015176 and hi = 0.0069954 linear. Only near-black
//! albedo loses its specular; on skin the factor is 1. glTF has no such
//! rule, so the GLB keeps the factor as is and records the window in
//! `extras.skin_spec.albedo_window`; the viewer applies it.

use crate::meta7a::{field_name, Meta};
use crate::resource::Resource;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

const CACHE_VERSION: u32 = 2;
/// Wardrobe item (meta resources).
const ITEM: u32 = 0xf795_e8ba;
/// Material record (PSO asset sets).
const MATERIAL: u32 = 0x2868_9f25;
/// Keyed texture list of a material record.
const TEXTURES: u32 = 0x5376_0af2;
/// Keys of the spec maps in a material record's texture list.
pub const KEY_SPEC: u32 = 0x179e_b44a;
pub const KEY_SPEC2: u32 = 0x4cf4_1ef4;
/// The shader slots the two maps go in.
pub const SLOT_SPEC: &str = "c6f46e79";
pub const SLOT_SPEC2: &str = "739e3d0d";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SkinSpec {
    version: u32,
    /// Material asset id -> [speculartex, speculartex2] (0 = none).
    pub materials: BTreeMap<u64, [u64; 2]>,
    /// Wardrobe colour texture -> the material asset id its state names.
    pub by_albedo: BTreeMap<u64, u64>,
    /// Colour textures whose states name more than one material with spec
    /// maps (left out of `by_albedo`).
    #[serde(default)]
    pub conflicts: usize,
}

/// The spec maps of a skin: material asset id and the two textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub material: u64,
    pub spec: u64,
    pub spec2: u64,
}

impl SkinSpec {
    /// The spec maps for the skin with this colour texture.
    pub fn of_albedo(&self, albedo: u64) -> Option<Spec> {
        let material = *self.by_albedo.get(&albedo)?;
        let [spec, spec2] = *self.materials.get(&material)?;
        Some(Spec { material, spec, spec2 }).filter(|s| s.spec != 0 || s.spec2 != 0)
    }
}

fn hex(v: &Value) -> u64 {
    v.as_str().and_then(|s| u64::from_str_radix(s, 16).ok()).unwrap_or(0)
}

/// The spec pair of one material record (its keyed texture list).
pub fn record_spec(rec: &Value) -> [u64; 2] {
    let mut out = [0u64; 2];
    for e in rec[field_name(TEXTURES).as_str()].as_array().into_iter().flatten() {
        match e["Key"].as_u64().map(|k| k as u32) {
            Some(KEY_SPEC) => out[0] = hex(&e["Item"]),
            Some(KEY_SPEC2) => out[1] = hex(&e["Item"]),
            _ => {}
        }
    }
    out
}

/// Every material record in a decoded PSO (`Pso::to_json`), with its spec pair.
pub fn walk_materials(v: &Value, out: &mut Vec<(u64, [u64; 2])>) {
    match v {
        Value::Object(o) => {
            if o.get("_struct").and_then(Value::as_str).is_some_and(|s| s.ends_with(&format!("{MATERIAL:08x}"))) {
                let g = hex(&v["guid"]);
                let s = record_spec(v);
                if g != 0 && s != [0, 0] {
                    out.push((g, s));
                }
            }
            o.values().for_each(|x| walk_materials(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| walk_materials(x, out)),
        _ => {}
    }
}

/// Every texture state `Asset` in a decoded wardrobe item: (albedo, material).
pub fn walk_assets(v: &Value, out: &mut Vec<(u64, u64)>) {
    match v {
        Value::Object(o) => {
            let (a, m) = (hex(&v[field_name(0xead7_33fc).as_str()]), hex(&v[field_name(crate::names::joaat_exact("Material")).as_str()]));
            if a != 0 && m != 0 {
                out.push((a, m));
            }
            o.values().for_each(|x| walk_assets(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| walk_assets(x, out)),
        _ => {}
    }
}

/// Link each colour texture to the one material (with spec maps) its states
/// name. Colour textures named with more than one such material are left
/// out; their count is returned too.
fn link(assets: impl Iterator<Item = (u64, u64)>, materials: &BTreeMap<u64, [u64; 2]>) -> (BTreeMap<u64, u64>, usize) {
    let mut all: BTreeMap<u64, std::collections::BTreeSet<u64>> = BTreeMap::new();
    for (a, m) in assets.filter(|(_, m)| materials.contains_key(m)) {
        all.entry(a).or_default().insert(m);
    }
    let conflicts = all.values().filter(|m| m.len() > 1).count();
    let by_albedo = all.into_iter().filter_map(|(a, m)| (m.len() == 1).then(|| (a, *m.first().unwrap()))).collect();
    (by_albedo, conflicts)
}

fn build(packs: &Path, ix: &crate::scan::ArchiveIndex) -> SkinSpec {
    let found: Vec<(Vec<(u64, [u64; 2])>, Vec<(u64, u64)>)> = ix
        .entries
        .par_iter()
        .filter(|e| e.sub == "7a" || e.kind == crate::classify::Kind::Pso)
        .map(|e| {
            let (mut mats, mut assets) = (Vec::new(), Vec::new());
            if e.kind == crate::classify::Kind::Pso {
                let Some(p) = crate::scan::read_entry(packs, e).ok().and_then(|d| crate::pso::parse(&d)) else { return (mats, assets) };
                // only the asset sets: the others are large and hold none
                if p.structs.contains_key(&MATERIAL) {
                    walk_materials(&p.to_json(), &mut mats);
                }
            } else {
                let Ok(r) = Resource::load_meta(packs, e) else { return (mats, assets) };
                let Some(m) = Meta::parse(&r) else { return (mats, assets) };
                for o in m.objects(ITEM) {
                    if let Some(it) = m.object(ITEM, o, 4) {
                        walk_assets(&it, &mut assets);
                    }
                }
            }
            (mats, assets)
        })
        .collect();
    let mut t = SkinSpec { version: CACHE_VERSION, ..Default::default() };
    for (mats, _) in &found {
        for &(g, s) in mats {
            t.materials.entry(g).or_insert(s);
        }
    }
    let (by_albedo, conflicts) = link(found.iter().flat_map(|(_, a)| a.iter().copied()), &t.materials);
    t.by_albedo = by_albedo;
    t.conflicts = conflicts;
    t
}

/// The skin spec table (built from the packs.rpf index on first use, then
/// cached in the work directory).
pub fn get(game_root: &Path) -> Result<&'static SkinSpec, String> {
    static T: OnceLock<Result<SkinSpec, String>> = OnceLock::new();
    T.get_or_init(|| {
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let cache = work.join("index").join("skin_spec.json");
        let ixp = crate::scan::index_path(&work.join("index"), "packs.rpf");
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let Ok(b) = std::fs::read(&cache) {
            if let Ok(t) = serde_json::from_slice::<SkinSpec>(&b) {
                if t.version == CACHE_VERSION && stamp(&cache) >= stamp(&ixp) {
                    return Ok(t);
                }
            }
        }
        let ix = crate::scan::load_index(&ixp).map_err(|e| format!("packs.rpf index ({}): {e}; index packs.rpf first", ixp.display()))?;
        let t = build(&game_root.join("packs.rpf"), &ix);
        if t.materials.is_empty() {
            return Err("no material assets with spec maps found in packs.rpf".into());
        }
        if let Ok(b) = serde_json::to_vec(&t) {
            let _ = crate::atomic::write(&cache, b);
        }
        Ok(t)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// The skin shaders: `ped_standard_skin`, `ped_standard_lod_skin`.
pub const SKIN_SHADERS: [&str; 2] = ["210fbc6a", "8d69edab"];
/// Lower bound of the engine's albedo window on skin F0 (sRGB 5 / 255, linear).
pub const ALBEDO_LO: f32 = 0.0015176;
/// Upper bound of the window (sRGB 20 / 255, linear).
pub const ALBEDO_HI: f32 = 0.0069954;
/// The window's factor on F0 for a linear albedo.
pub fn albedo_window(rgb: [f32; 3]) -> f32 {
    let m = rgb[0].max(rgb[1]).max(rgb[2]);
    ((m - ALBEDO_LO) / (ALBEDO_HI - ALBEDO_LO)).clamp(0.0, 1.0)
}

/// Engine detail-normal atlas (BC5, 48 tiles), the default of the skin
/// shader's slot `0cd27763`.
pub const DETAIL_NORMAL_ATLAS: u64 = 0xe4bb_0ab7_0797_9121;
/// Engine detail-control atlas (BC7, data in R), slot `7c0055b9`.
pub const DETAIL_CONTROL_ATLAS: u64 = 0x3e83_0fd3_713f_2e21;
/// Material constant +0xc0 (int3): region -> tile, three 2-bit fields per region.
pub const DETAIL_TABLE_PARAM: &str = "a6b9e97a";
/// Material constant +0xa0: x colour (and roughness) strength, y normal strength, zw UV tiling.
pub const DETAIL_SCALE_PARAM: &str = "6022028b";
/// Material constant +0xb4: one tile for every region when >= 0.
pub const DETAIL_FORCE_PARAM: &str = "24ffd0a6";
/// Material constant +0x8: occlusion strength (`lerp(1, speculartex B, s)`).
pub const OCCLUSION_PARAM: &str = "4d1ecedd";
/// Material constant +0x60: subtracted from `speculartex` R for the translucency.
pub const TRANSLUCENCY_BIAS_PARAM: &str = "381999f3";

/// The tile of the detail atlases a skin region (0-15) uses.
pub fn detail_slice(table: [u32; 3], region: u32) -> u32 {
    let f = |w: u32| (w >> (2 * region)) & 3;
    f(table[0]) | f(table[1]) << 2 | f(table[2]) << 4
}

/// The tiles of all 16 regions; a forced tile (>= 0) for all.
pub fn detail_slices(table: [u32; 3], force: f32) -> [u32; 16] {
    std::array::from_fn(|r| if force >= 0.0 { force as u32 } else { detail_slice(table, r as u32) })
}

/// Write the two 4 x 4 sheets of these tiles (region r at column r % 4,
/// row r / 4) under `dir`, once per tile set: (normal, control).
pub fn write_detail_sheets(game_root: &Path, dir: &Path, slices: &[u32; 16]) -> anyhow::Result<[std::path::PathBuf; 2]> {
    let key: String = slices.iter().map(|s| format!("{s:02x}")).collect();
    let mut out = Vec::new();
    for (tag, h) in [("n", DETAIL_NORMAL_ATLAS), ("c", DETAIL_CONTROL_ATLAS)] {
        let p = dir.join(format!("skindetail_{tag}_{key}.png"));
        if !p.exists() {
            let (store, tx) = crate::ttpf::global(game_root).get(h).ok_or_else(|| anyhow::anyhow!("skin detail atlas {h:016x} is in no store"))?;
            let data = store.data(tx)?;
            let (w, hh) = (tx.width as usize, tx.height as usize);
            let mut sheet = vec![0u8; 16 * w * hh * 4];
            for (r, &sl) in slices.iter().enumerate() {
                let (rgba, tw, th) = crate::ttpf::decode_rgba_slice(tx, &data, 0, sl.min(tx.slices().saturating_sub(1)))?;
                anyhow::ensure!(tw as usize == w && th as usize == hh, "atlas tile size");
                let (cx, cy) = (r % 4, r / 4);
                for y in 0..hh {
                    let d = ((cy * hh + y) * 4 * w + cx * w) * 4;
                    sheet[d..d + w * 4].copy_from_slice(&rgba[y * w * 4..(y + 1) * w * 4]);
                }
            }
            std::fs::create_dir_all(dir)?;
            crate::texture::save_png(&p, &sheet, (4 * w) as u32, (4 * hh) as u32)?;
        }
        out.push(p);
    }
    Ok([out[0].clone(), out[1].clone()])
}

/// Values of a skin material's constants that the exports use.
#[derive(Debug, Clone, Copy, Default)]
pub struct SkinConsts {
    pub fresnel: Option<f32>,
    pub occlusion: Option<f32>,
    pub translucency_bias: Option<f32>,
}

impl SkinConsts {
    pub fn of(consts: &[crate::drawable::ShaderConst]) -> SkinConsts {
        let one = |h: &str| consts.iter().find(|c| c.hash == h).and_then(|c| c.components()).and_then(|v| v.first().copied());
        SkinConsts { fresnel: one(FRESNEL_PARAM), occlusion: one(OCCLUSION_PARAM), translucency_bias: one(TRANSLUCENCY_BIAS_PARAM) }
    }
}

/// `extras.skin_spec.detail` of a skin material: its tiles, strengths and
/// tiling, and the two sheets (written under `tdir`, paths through `rel`).
/// None for other shaders or without a region table.
pub fn detail_json(shader_hash: &str, consts: &[crate::drawable::ShaderConst], game_root: &Path, tdir: &Path, rel: &dyn Fn(&Path) -> String) -> Option<Value> {
    if !SKIN_SHADERS.contains(&shader_hash) {
        return None;
    }
    let c = |h: &str| consts.iter().find(|c| c.hash == h).and_then(|c| c.components()).map(|v| v.to_vec());
    let t = c(DETAIL_TABLE_PARAM).filter(|v| v.len() >= 3)?;
    let table = [t[0].to_bits(), t[1].to_bits(), t[2].to_bits()];
    let scale = c(DETAIL_SCALE_PARAM).filter(|v| v.len() >= 4)?;
    let force = c(DETAIL_FORCE_PARAM).and_then(|v| v.first().copied()).unwrap_or(-1.0);
    let slices = detail_slices(table, force);
    let mut j = serde_json::json!({
        "region_slices": slices,
        "strength": [scale[0], scale[1]],
        "uv_scale": [scale[2], scale[3]],
        "forced_slice": force,
        "atlases": {"normal": format!("{DETAIL_NORMAL_ATLAS:016x}"), "control": format!("{DETAIL_CONTROL_ATLAS:016x}"), "store": "data/ttdb/graphics/ttdb_graphics.ttpf"},
        "layout": "4 x 4 sheets of 256 x 256 tiles, region r (speculartex2 G: u32(16 G + 0.5)) at column r % 4, row r / 4",
        "rule": "uv = UV0 * uv_scale; c = control sheet R, d = normal sheet RG * 2 - 1, mask = speculartex2 R; colour *= saturate(1 + strength.x * mask * (2c - 1)); tangent-space normal xy += strength.y * mask * d (before z is rebuilt). Where speculartex2 G straddles two regions the game samples both and averages."
    });
    match write_detail_sheets(game_root, tdir, &slices) {
        Ok([n, c]) => {
            j["normal_sheet"] = rel(&n).into();
            j["control_sheet"] = rel(&c).into();
        }
        Err(e) => j["error"] = e.to_string().into(),
    }
    Some(j)
}

/// A skin spec map's blue (occlusion) as an occlusion PNG for glTF (which
/// reads red), next to it: `<stem>_occ.png`. `out` is the export root and
/// `spec` the spec map's path under it.
pub fn write_occlusion_png(out: &Path, spec: &str) -> Option<String> {
    let rel = spec.strip_suffix(".png")?.to_string() + "_occ.png";
    let dst = out.join(&rel);
    if !dst.exists() {
        let img = image::open(out.join(spec)).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let mut px = img.into_raw();
        for p in px.chunks_exact_mut(4) {
            let b = p[2];
            p[0] = b;
            p[1] = b;
            p[3] = 255;
        }
        crate::texture::save_png(&dst, &px, w, h).ok()?;
    }
    Some(rel)
}

/// The skin material's `fresnel` constant (`specularfresnel`).
pub const FRESNEL_PARAM: &str = "b5fe3acb";

/// `KHR_materials_specular` factor for a skin `fresnel`: the game's F0 is
/// `0.04 * (1 + fresnel)` and glTF's is `0.04 * factor`. Clamped so F0 stays
/// within 0..1.
pub fn specular_factor(fresnel: f32) -> f32 {
    (1.0 + fresnel).clamp(0.0, 25.0)
}

/// PNG of a texture slot in a material's texture list.
fn slot_png<'a>(entries: &'a [Value], slot: &str) -> Option<&'a str> {
    entries.iter().find(|t| t["param_hash"] == slot).and_then(|t| t["png"].as_str())
}

/// The glTF fields of a skin material whose `speculartex` decoded:
/// metallic-roughness texture (G = roughness, metallic 0), roughness and
/// metallic factors, and `KHR_materials_specular`. Merged into `g`; the
/// raw maps go in `extras.skin_spec`. Not a skin shader, or no decoded
/// `speculartex`: nothing changes.
pub fn apply_gltf(g: &mut Value, extras: &mut Value, shader_hash: &str, entries: &[Value], k: &SkinConsts, detail: Option<Value>, occlusion_png: Option<&str>) {
    let fresnel = k.fresnel;
    if !SKIN_SHADERS.contains(&shader_hash) {
        return;
    }
    let Some(spec) = slot_png(entries, SLOT_SPEC) else { return };
    let f = fresnel.unwrap_or(0.0);
    if !g["pbrMetallicRoughness"].is_object() {
        g["pbrMetallicRoughness"] = serde_json::json!({});
    }
    let pbr = &mut g["pbrMetallicRoughness"];
    pbr["metallicFactor"] = serde_json::json!(0.0);
    pbr["roughnessFactor"] = serde_json::json!(1.0);
    pbr["metallicRoughnessTexture"] = serde_json::json!({"uri": spec});
    if !g["extensions"].is_object() {
        g["extensions"] = serde_json::json!({});
    }
    g["extensions"]["KHR_materials_specular"] = serde_json::json!({"specularFactor": specular_factor(f)});
    let occ = k.occlusion.unwrap_or(0.0).clamp(0.0, 1.0);
    if let (true, Some(p)) = (occ > 0.0, occlusion_png) {
        g["occlusionTexture"] = serde_json::json!({"uri": p, "strength": occ});
    }
    extras["skin_spec"] = serde_json::json!({
        "speculartex": spec,
        "speculartex2": slot_png(entries, SLOT_SPEC2),
        "fresnel": fresnel,
        "f0": 0.04 * specular_factor(f),
        "albedo_window": {"lo": ALBEDO_LO, "hi": ALBEDO_HI, "srgb": [5, 20], "rule": "F0 *= saturate((max(r, g, b) of the linear albedo - lo) / (hi - lo))"},
        "occlusion": {"strength": k.occlusion, "png": if occ > 0.0 { occlusion_png } else { None }, "rule": "G-buffer occlusion = lerp(1, speculartex B, strength); the game's lighting: diffuse ambient x it, specular ambient x its fourth root, direct light not at all. Strength 0 = B unused (human body skin)"},
        "translucency": {"bias": k.translucency_bias, "rule": "t = saturate(speculartex R - bias) to a G-buffer target of its own; per light the game adds 0.17182 x T x (2 + 0.75 / (1.25 - cos)^1.5) with T = (e^3 + 3e) / 4, e = exp(-0.480898 s / d) per channel, thickness s = lo + (hi - lo)(1 - t)^p from a per-entity profile (d, p, lo, hi: loaded data, not found). Not in the GLB."},
        "detail": detail,
        "channels": {
            "speculartex": "R translucency mask, G roughness, B occlusion",
            "speculartex2": "R detail strength, G skin region (G * 16) for the engine detail atlases"
        },
        "gltf": "metallicRoughnessTexture = speculartex (G roughness; metallic 0, so B is not read), KHR_materials_specular.specularFactor = 1 + fresnel"
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn key_hashes() {
        assert_eq!(KEY_SPEC, 396_276_810);
        assert_eq!(KEY_SPEC2, 1_291_067_124);
        // the wardrobe asset fields as the meta decoder names them (the
        // material id is the case-sensitive hash of `Material`)
        assert_eq!(field_name(crate::names::joaat_exact("Material")), "Material");
    }

    #[test]
    fn fresnel_hash() {
        assert_eq!(format!("{:08x}", crate::names::joaat_exact("fresnel")), FRESNEL_PARAM);
    }

    #[test]
    fn detail_tiles_of_the_human_table() {
        // Lucia's and Jason's region table: tile 16 + region
        let t = [0xE4E4_E4E4, 0xFFAA_5500, 0x5555_5555];
        let s = detail_slices(t, -1.0);
        assert_eq!(s, std::array::from_fn(|r| 16 + r as u32));
        assert_eq!(detail_slices(t, 3.0), [3; 16]);
    }

    #[test]
    fn albedo_window_bounds() {
        fn lin(c: f32) -> f32 {
            let c = c / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }
        assert!((lin(5.0) - ALBEDO_LO).abs() < 1e-6);
        assert!((lin(20.0) - ALBEDO_HI).abs() < 1e-6);
        assert_eq!(albedo_window([0.0, 0.0, 0.001]), 0.0);
        assert_eq!(albedo_window([0.3, 0.2, 0.15]), 1.0);
        assert!((albedo_window([0.0, 0.004, 0.0]) - 0.4532).abs() < 1e-3);
    }

    #[test]
    fn skin_spec_becomes_gltf_channels() {
        let entries = vec![
            json!({"param_hash": SLOT_SPEC, "png": "textures/ttpf/a.png"}),
            json!({"param_hash": SLOT_SPEC2, "png": "textures/ttpf/b.png"}),
        ];
        let (mut g, mut x) = (json!({}), json!({}));
        let k = SkinConsts { fresnel: Some(-0.3), occlusion: Some(1.0), translucency_bias: Some(0.0) };
        apply_gltf(&mut g, &mut x, "210fbc6a", &entries, &k, None, Some("textures/ttpf/a_occ.png"));
        assert_eq!(g["occlusionTexture"]["uri"], "textures/ttpf/a_occ.png");
        assert_eq!(g["occlusionTexture"]["strength"], 1.0);
        assert_eq!(g["pbrMetallicRoughness"]["metallicRoughnessTexture"]["uri"], "textures/ttpf/a.png");
        assert_eq!(g["pbrMetallicRoughness"]["metallicFactor"], 0.0);
        assert_eq!(g["pbrMetallicRoughness"]["roughnessFactor"], 1.0);
        let f = g["extensions"]["KHR_materials_specular"]["specularFactor"].as_f64().unwrap();
        assert!((f - 0.7).abs() < 1e-6);
        assert_eq!(x["skin_spec"]["speculartex2"], "textures/ttpf/b.png");
        // other shaders, and skin without a decoded speculartex, are left alone
        let (mut g2, mut x2) = (json!({}), json!({}));
        apply_gltf(&mut g2, &mut x2, "deadbeef", &entries, &SkinConsts { fresnel: None, ..Default::default() }, None, None);
        apply_gltf(&mut g2, &mut x2, "210fbc6a", &entries[1..], &SkinConsts { fresnel: None, ..Default::default() }, None, None);
        assert_eq!(g2, json!({}));
        assert_eq!(x2, json!({}));
    }

    #[test]
    fn shared_colour_maps_are_left_out() {
        let mats = BTreeMap::from([(10, [1, 2]), (11, [3, 4]), (12, [5, 6])]);
        // 1 -> one material (twice), 2 -> two materials, 3 -> a material without spec maps
        let (by, n) = link([(1, 10), (1, 10), (2, 11), (2, 12), (3, 99)].into_iter(), &mats);
        assert_eq!(by, BTreeMap::from([(1, 10)]));
        assert_eq!(n, 1);
    }

    #[test]
    fn material_records_give_their_spec_pair() {
        // a record as `Pso::to_json` writes it (the female upper-body skin's)
        let k = field_name(TEXTURES);
        let rec = json!({"_struct": "28689f25", "guid": "79a6a00eabdb393e", k.clone(): [
            {"Item": "603b8715c4624666", "Key": KEY_SPEC, "_struct": "00000103"},
            {"Item": "79e9d1c0a266e986", "Key": KEY_SPEC2, "_struct": "00000103"},
        ]});
        let colour = json!({"_struct": "28689f25", "guid": "1111111111111111", k: [{"Item": "2222222222222222", "Key": 2_063_268_609u32}]});
        let doc = json!({"root": null, "objects": {"e532cef000000101": [{"93c495af": [{"Item": rec, "Key": "79a6a00eabdb393e"}, {"Item": colour, "Key": "1111111111111111"}]}]}});
        let mut out = Vec::new();
        walk_materials(&doc, &mut out);
        assert_eq!(out, vec![(0x79a6_a00e_abdb_393e, [0x603b_8715_c462_4666, 0x79e9_d1c0_a266_e986])]);
    }

    #[test]
    fn wardrobe_assets_link_albedo_to_material() {
        let asset = |a: &str, m: &str| json!({"_struct": "147fdab5", field_name(0xead7_33fc): a, field_name(crate::names::joaat_exact("Material")): m, "Normal": "9d337dabca636906"});
        let item = json!({"states": [{"Asset": asset("cb95198f8a74cb70", "79a6a00eabdb393e")}, {"Asset": asset("f108653387cdfddd", "0000000000000000")}]});
        let mut out = Vec::new();
        walk_assets(&item, &mut out);
        assert_eq!(out, vec![(0xcb95_198f_8a74_cb70, 0x79a6_a00e_abdb_393e)]);
        let mut t = SkinSpec::default();
        t.materials.insert(0x79a6_a00e_abdb_393e, [1, 2]);
        t.by_albedo.insert(0xcb95_198f_8a74_cb70, 0x79a6_a00e_abdb_393e);
        assert_eq!(t.of_albedo(0xcb95_198f_8a74_cb70), Some(Spec { material: 0x79a6_a00e_abdb_393e, spec: 1, spec2: 2 }));
        assert_eq!(t.of_albedo(5), None);
    }
}
