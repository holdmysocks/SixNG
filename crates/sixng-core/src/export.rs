//! Workspace, name recovery, filtering, categories and resumable extraction.
use crate::classify::{looks_like_txd, Kind};
use crate::game::{self, ArchiveInfo};
use crate::resource::{find_resource_name, Resource};
use crate::scan::{self, ArchiveIndex, Entry};
use crate::texture;
use anyhow::{anyhow, Context, Result};
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub type Progress = Arc<dyn Fn(&str, u64, u64) + Send + Sync>;

pub fn no_progress() -> Progress {
    Arc::new(|_, _, _| {})
}

#[derive(Clone)]
pub struct Workspace {
    pub game: PathBuf,
    pub work: PathBuf,
}

impl Workspace {
    pub fn new(game: impl Into<PathBuf>, work: impl Into<PathBuf>) -> Self {
        let work = work.into();
        crate::names::set_work_dir(&work);
        Workspace { game: game.into(), work }
    }
    pub fn index_dir(&self) -> PathBuf {
        self.work.join("index")
    }
    pub fn archives(&self) -> Vec<ArchiveInfo> {
        game::discover(&self.game)
    }
    pub fn resolve(&self, q: &str) -> Result<ArchiveInfo> {
        let all = self.archives();
        game::resolve(&all, q).cloned().ok_or_else(|| anyhow!("archive not found or ambiguous: {q}"))
    }
    /// True when an index exists and was written by this tool version
    /// (older formats need re-indexing; checked from the first bytes only).
    pub fn is_indexed(&self, a: &ArchiveInfo) -> bool {
        use std::io::Read;
        let mut head = [0u8; 48];
        let Ok(mut f) = fs::File::open(scan::index_path(&self.index_dir(), &a.id)) else { return false };
        let n = f.read(&mut head).unwrap_or(0);
        let want = format!("\"tool_version\":\"{}\"", crate::INDEX_VERSION);
        String::from_utf8_lossy(&head[..n]).contains(&want)
    }
    pub fn load_index(&self, a: &ArchiveInfo) -> Result<ArchiveIndex> {
        scan::load_index(&scan::index_path(&self.index_dir(), &a.id))
            .and_then(|ix| { anyhow::ensure!(ix.tool_version == crate::INDEX_VERSION, "index version mismatch"); Ok(ix) })
            .with_context(|| format!("{} is not indexed yet, or the index is from an older version (run `sixng index {}`)", a.id, a.id))
    }
    /// Build (or load) the index and run the name pass.
    pub fn ensure_index(&self, a: &ArchiveInfo, force: bool, names: bool, progress: Progress) -> Result<ArchiveIndex> {
        let p2 = progress.clone();
        let pf: Arc<scan::ProgressFn> = Arc::new(move |d, t| p2("scan", d, t));
        let mut ix = scan::build_index(&a.path, &a.id, &self.index_dir(), force, Some(pf))?;
        if names && !ix.names_done {
            recover_names(&a.path, &mut ix, progress)?;
            scan::save_index(&self.index_dir(), &ix)?;
        }
        Ok(ix)
    }
}

/// Load each resource's system page and recover name / type.
pub fn recover_names(archive: &Path, ix: &mut ArchiveIndex, progress: Progress) -> Result<()> {
    let idx: Vec<usize> = ix
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.kind, Kind::RageObj | Kind::Resource))
        .map(|(i, _)| i)
        .collect();
    let total = idx.len() as u64;
    let done = AtomicU64::new(0);
    let results: Vec<(usize, Option<String>, Option<String>, Vec<String>)> = idx
        .par_chunks(256)
        .flat_map_iter(|chunk| {
            let mut f = fs::File::open(archive).ok();
            let mut out = Vec::new();
            for &i in chunk {
                let e = &ix.entries[i];
                let sys = f.as_mut().and_then(|f| crate::resource::read_sys(f, e).ok().map(|x| x.0));
                let mut name = None;
                let mut rtype = None;
                let mut members = Vec::new();
                if let Some(sys) = sys {
                    if let Some((n, t)) = find_resource_name(&sys) {
                        name = Some(n);
                        rtype = Some(t);
                        let r = Resource { sys, gfx: vec![], gfx_base: 0 };
                        members = crate::drawable::dictionary_members(&r).into_iter().map(|m| m.name).collect();
                    } else if looks_like_txd(&sys) {
                        rtype = Some("txd".into());
                        let r = Resource { sys, gfx: vec![], gfx_base: 0 };
                        name = texture::dict_name(&r);
                    }
                }
                out.push((i, name, rtype, members));
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                if d % 2000 == 0 {
                    progress("names", d, total);
                }
            }
            out
        })
        .collect();
    for (i, n, t, members) in results {
        ix.entries[i].name = n;
        ix.entries[i].rtype = t;
        ix.entries[i].members = members;
    }
    progress("names", total, total);
    ix.names_done = true;
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Filter {
    pub kinds: Vec<Kind>,
    pub rtypes: Vec<String>,
    pub name: Option<String>,
    pub exclude_name: Option<String>,
    pub ids: Vec<String>,
    pub min_size: u64,
    pub limit: Option<usize>,
}

impl Filter {
    pub fn select<'a>(&self, ix: &'a ArchiveIndex) -> Result<Vec<&'a Entry>> {
        self.select_entries(ix.entries.iter())
    }

    /// Export selection also accepts explicit `<entry id>#<member name>` ids.
    pub fn select_exports(&self, ix: &ArchiveIndex) -> Result<Vec<Entry>> {
        let mut members = Vec::new();
        let mut filter = self.clone();
        let resolver = ModelResolver::new(&ix.entries);
        for id in &mut filter.ids {
            if let Some((_base, name)) = id.trim().split_once('#') {
                let (e, _) = resolver.resolve(&ix.entries, id.trim())?;
                let selected = member_entry(e, name)?;
                *id = selected.id.clone();
                if !members.iter().any(|e: &Entry| e.id == selected.id) {
                    members.push(selected);
                }
            }
        }
        Ok(filter.select_entries(ix.entries.iter().chain(members.iter()))?.into_iter().cloned().collect())
    }

    fn select_entries<'a>(&self, entries: impl Iterator<Item = &'a Entry>) -> Result<Vec<&'a Entry>> {
        let re = self.name.as_deref().map(Regex::new).transpose()?;
        let ex = self.exclude_name.as_deref().map(Regex::new).transpose()?;
        // ids as a set: lists can hold tens of thousands of entries
        let idset: std::collections::HashSet<String> = self.ids.iter().map(|i| i.trim().to_ascii_lowercase()).collect();
        let mut v: Vec<&Entry> = entries
            .filter(|e| self.kinds.is_empty() || self.kinds.contains(&e.kind))
            .filter(|e| self.rtypes.is_empty() || e.rtype.as_ref().is_some_and(|t| self.rtypes.contains(t)))
            .filter(|e| idset.is_empty() || idset.contains(&e.id.to_ascii_lowercase()) || idset.contains(&format!("{:x}{}", e.off, e.id.split_once('#').map_or(String::new(), |(_, m)| format!("#{m}")))))
            .filter(|e| e.usize >= self.min_size)
            .filter(|e| match &re {
                Some(r) => e.name.as_deref().is_some_and(|n| r.is_match(n)) || r.is_match(&e.id),
                None => true,
            })
            .filter(|e| match &ex {
                Some(r) => !e.name.as_deref().is_some_and(|n| r.is_match(n)),
                None => true,
            })
            .collect();
        if let Some(l) = self.limit {
            v.truncate(l);
        }
        Ok(v)
    }
}

/// Non-vehicle fragment name prefixes (props, vegetation, peds, glass...).
pub const NON_VEHICLE_FT: &str = r"^(prop_|veg_|glass_|char_|crb_|crm_|armor_|p_|des_|int_|v_|sm_|hei_|xm_|ba_|hc_|player_|wpn_|w_|sprayer_|lo_|vi_|nb|id\d|n\d)";

/// Optional world-space region (min x, min y, max x, max y; RAGE metres) set
/// by the CLI `--bbox`: collision, navmesh, placements and lights only write
/// what intersects it.
static REGION: std::sync::OnceLock<[f32; 4]> = std::sync::OnceLock::new();

pub fn set_region(r: [f32; 4]) {
    let _ = REGION.set(r);
}

/// True when no region is set or the XY box [min, max] intersects it.
pub fn in_region(min: [f32; 3], max: [f32; 3]) -> bool {
    match REGION.get() {
        None => true,
        Some(r) => max[0] >= r[0] && min[0] <= r[2] && max[1] >= r[1] && min[1] <= r[3],
    }
}

/// Coordinate-system note written into every model.json / GLB.
pub const COORDS: &str = "units: metres (1 unit = 1 m). Source data is RAGE Z-up right-handed. model.glb and points.glb are glTF Y-up: glb(x, y, z) = rage(x, z, -y) (positions and normals); UVs as stored (top-left origin). Blender's glTF importer converts back to Z-up.";

/// Name-based guess at the driving surface of a model, for track builders
/// (e.g. Assetto Corsa 1ROAD/1GRASS/1KERB). Not from collision data.
pub fn surface_hint(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    let has = |ks: &[&str]| ks.iter().any(|k| n.contains(k));
    if has(&["kerb", "curb"]) {
        Some("kerb")
    } else if has(&["sidewalk", "pavement", "footpath", "_pave"]) {
        Some("sidewalk")
    } else if has(&["road", "_rd_", "_rd", "street", "hwy", "highway", "freeway", "motorway", "junction", "carpark", "parking", "tunnel", "bridge", "ramp", "asphalt"]) {
        Some("road")
    } else if has(&["grass", "lawn", "field", "meadow", "turf"]) {
        Some("grass")
    } else if has(&["dirt", "sand", "gravel", "mud", "beach", "track_dirt"]) {
        Some("dirt")
    } else if has(&["veg_", "tree", "bush", "hedge", "foliage", "plant"]) {
        Some("vegetation")
    } else {
        None
    }
}

/// Built-in categories. Returns (archive id substring, filter) pairs.
pub fn category(name: &str) -> Result<Vec<(String, Filter)>> {
    let txd = Filter { rtypes: vec!["txd".into()], ..Default::default() };
    Ok(match name {
        "textures" => vec![("textures.rpf".into(), txd.clone()), ("levels.rpf".into(), txd.clone()), ("packs.rpf".into(), txd)],
        "map" => vec![
            ("levels.rpf".into(), Filter { rtypes: vec!["dr".into(), "dd".into()], ..Default::default() }),
            ("levels.rpf".into(), Filter { kinds: vec![Kind::Pso, Kind::Rbf, Kind::Xml], ..Default::default() }),
        ],
        "vehicles" => vec![(
            "levels.rpf".into(),
            Filter { rtypes: vec!["ft".into()], exclude_name: Some(NON_VEHICLE_FT.into()), ..Default::default() },
        )],
        "meta" => {
            let f = Filter { kinds: vec![Kind::Xml, Kind::Pso, Kind::Rbf, Kind::Text], ..Default::default() };
            vec![("data.rpf".into(), f.clone()), ("common.rpf".into(), f)]
        }
        _ => return Err(anyhow!("unknown category {name} (textures|map|vehicles|meta)")),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOptions {
    pub out: PathBuf,
    /// Write PNG for textures.
    pub png: bool,
    /// Write linear DDS (mip 0) for textures.
    pub dds: bool,
    /// Also dump the decompressed entry bytes.
    pub raw: bool,
    /// Re-export even if outputs exist.
    pub force: bool,
    /// Skip textures in drawables (only metadata).
    pub skip_embedded_textures: bool,
    /// Body shape for ped clothing (`bodyshape::parse_spec` output): the
    /// shared ped skeleton is posed with the game's body-shape rig. Empty =
    /// neutral body.
    pub body_shape: Vec<(String, f32)>,
    /// Head skin variant for ped heads (`headskin`): index into the faces
    /// linked to the head, or into the automatic candidates; wraps around.
    #[serde(default)]
    pub head_skin: usize,
    /// Body colour for car-paint materials (linear RGB 0..1), e.g. a wreck's
    /// colour from `sixng vehicles`. None = neutral grey stand-in.
    #[serde(default)]
    pub paint: Option<[f32; 3]>,
    /// Texture replacements by 64-bit store hash (old -> new), applied where
    /// a material's textures are resolved: ped variation overrides (skin,
    /// dirt, blood) from `ped-variations`.
    #[serde(default)]
    pub tex_remap: std::collections::HashMap<u64, u64>,
    /// Ped clothing tints by drawable name (`outfit --tint`, from
    /// `ped-variations`): palette name hash and the rows of tint 0, 1, 2.
    /// Parts without one are shaded grey (`pedtint`).
    #[serde(default)]
    pub ped_tints: std::collections::HashMap<String, (u32, [u8; 3])>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedItem {
    pub id: String,
    pub archive: String,
    pub kind: String,
    pub rtype: Option<String>,
    pub name: Option<String>,
    pub status: String,
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Non-fatal problems (e.g. one texture in an unsupported format).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' { c } else { '_' }).collect()
}

/// Game root for an archive (the folder its id is relative to).
pub fn game_root(a: &ArchiveInfo) -> PathBuf {
    let depth = a.id.split('/').count();
    let mut p = a.path.clone();
    for _ in 0..depth {
        p.pop();
    }
    p
}

/// Parameter hashes whose texture is the albedo / colour map.
const ALBEDO_PARAMS: &[&str] = &["c8e8c282"];
/// Parameter hashes whose texture is a tangent-space normal map.
const NORMAL_PARAMS: &[&str] = &["84c69350", "58b6e707", "1e77bf4c"];
/// The vehicle material palette (128x256 sRGB colour bands shared by the
/// two-layer vehicle shaders). Its lookup is not decoded, and as the only
/// sRGB texture of a tyre or trim material it must not become the base colour.
const VEHICLE_PALETTE: &str = "19df25d7";
/// `lyr0diffusetex`, `lyr1diffusetex`: a layer's detail map (linear: grey
/// detail in R and G around 0.5, a wear mask in B), not a colour.
const LAYER_DETAIL: [&str; 2] = ["346394ea", "7d4f12e2"];
/// Vehicle bones whose meshes are not body panels (substring of the bone
/// name): interior, lamps, mechanical parts.
const NOT_BODY_BONES: [&str; 22] = [
    "interior", "steering", "doorcard", "seat", "sunvisor", "dash", "pedal", "handbrake", "gear", "headrest", "brake", "gas", "clutch", "winder", "light", "siren", "indicator", "lamp", "engine", "exhaust", "suspension", "axle",
];
/// `emissivetex`: the lit parts of a lamp or panel.
const EMISSIVE_PARAM: &str = "8e0d63ee";
/// The texture the game draws vehicle screens into while it runs (dials,
/// needles, navigation): bound as the colour and emissive map of a second
/// `standard_vehicle_emissive_layer` mesh that lies about 1 mm in front of
/// the screen's stored picture. No store holds it (552 vehicle models bind
/// it), so that overlay is left out and the stored picture shows.
const RUNTIME_SCREEN: &str = "a800afccf01aa36b";
/// `standard_vehicle_2lyr_wheel_inst`, the shader of the shared `rim_*`
/// models. Its embedded tint palette (129 x 1, the same in every rim) is a
/// placeholder: entries 0-7 are pure red, yellow, green, blue, magenta,
/// grey, orange and cyan, then shades of them. The game writes the wheel
/// colours over it, so it is not multiplied into the rim.
const WHEEL_RIM: &str = "c5cdbdd2";
/// `standard_vehicle_2lyr_tyre_inst`: the shared `tyre_*` models carry the
/// same placeholder palette.
const WHEEL_TYRE: &str = "99ad6fc9";

#[derive(Default)]
struct ShaderTextures {
    base: Option<String>,
    normal: Option<String>,
    /// The `emissivetex` map, when the shader has one that decodes.
    emissive: Option<String>,
    entries: Vec<serde_json::Value>,
    /// Flat base colour (0..1) when the base-colour texture is BCPack-only.
    base_factor: Option<[f32; 4]>,
    /// The `texture_fallback` record for that base colour.
    fallback: Option<serde_json::Value>,
    /// Alpha range of the base-colour texture.
    base_alpha: Option<[u8; 2]>,
    /// The model's tint palette (sRGB RGBA8), indexed by vertex colour blue.
    tint_palette: Option<Vec<[u8; 4]>>,
    /// Ped clothing colour baked from the shade map (`bake_ped_tint`).
    ped_tint: Option<serde_json::Value>,
    /// Vehicles: most of the bones this shader's meshes hang on are
    /// interior, lamp or mechanical bones (`NOT_BODY_BONES`), so it is not
    /// body paint. Set once the meshes are decoded.
    not_body: bool,
}

/// Shader slot of the per-model tint palette.
const TINT_PALETTE: &str = "2c8d7182";

/// Referenced textures that no store or resource of the install holds and
/// that stand for an engine default: (hash, value, note). `5d7a3a87cc19967d`
/// is the white vehicle colour slot (body colour = runtime paint);
/// `1e2c9340e14a7d07` (19 % of all unresolved references) sits in base
/// colour and other colour slots of every shader family, with and without a
/// tint palette, so it is taken as white too.
const ENGINE_DEFAULTS: &[(&str, [f64; 4], &str)] = &[
    ("5d7a3a87cc19967d", [255.0, 255.0, 255.0, 255.0], "engine default texture (white; vehicle paint slot)"),
    ("1e2c9340e14a7d07", [255.0, 255.0, 255.0, 255.0], "engine default texture, assumed white (used in colour slots of every shader family)"),
];

/// Typical value of data slots (median mean colour of the decoded textures
/// in that slot over 3,000 sampled map models), for unresolved references.
const SLOT_TYPICAL: &[(&str, [f64; 4])] = &[
    ("c6f46e79", [0.0, 150.0, 224.0, 255.0]),
    ("739e3d0d", [0.0, 0.0, 0.0, 255.0]),
    ("38ebcdb3", [85.0, 84.0, 91.0, 255.0]),
    ("3c4af3d4", [0.0, 0.0, 123.0, 255.0]),
    ("2bb2ede8", [0.0, 172.0, 128.0, 255.0]),
    ("4d56d7fe", [0.0, 176.0, 129.0, 255.0]),
];

/// Role of a texture slot whose texture is not in the install (no format to
/// go by): from the known slots.
fn unresolved_role(param_hash: &str) -> &'static str {
    let sr = crate::material::slot_role(param_hash).unwrap_or("");
    if param_hash == TINT_PALETTE {
        "tint palette"
    } else if param_hash == VEHICLE_PALETTE {
        "palette"
    } else if ALBEDO_PARAMS.contains(&param_hash) {
        "base_color"
    } else if NORMAL_PARAMS.contains(&param_hash) || sr.starts_with("normal") {
        "normal"
    } else if sr.contains("sRGB") {
        "colour (sRGB)"
    } else {
        "data (linear)"
    }
}

const UNRESOLVED: &str = "not in the install: no store or resource holds this hash";

/// Per-vertex tint from a palette: entry `colour.b` of row 0 (the default
/// tint), as linear RGBA8 for glTF COLOR_0. Indices past the palette are
/// white (no tint).
fn tint_colours(palette: &[[u8; 4]], colours: &[[u8; 4]]) -> Vec<[u8; 4]> {
    let lin = |v: u8| {
        let c = v as f32 / 255.0;
        let l = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        (l * 255.0 + 0.5) as u8
    };
    colours.iter().map(|c| palette.get(c[2] as usize).map_or([255; 4], |p| [lin(p[0]), lin(p[1]), lin(p[2]), 255])).collect()
}

/// Classification of a shader, corrected by its base-colour texture: an
/// alpha-bucket material whose colour alpha is one constant value (ped
/// clothing holds 64 or 128 there) is not see-through, so it is drawn opaque.
fn classify_shader(s: &crate::drawable::Shader, st: &ShaderTextures) -> serde_json::Value {
    let mut c = crate::material::classify(&s.name_hash, &s.flags);
    if let Some([lo, hi]) = st.base_alpha {
        if c["alpha_mode"] == "BLEND" && c["class"] == "alpha" && hi.saturating_sub(lo) <= 12 {
            c["alpha_mode"] = "OPAQUE".into();
            c["alpha_note"] = format!("colour texture alpha is constant ({lo}-{hi}), not opacity: drawn opaque").into();
        }
    }
    // Animal coats (`ped_standard_animal`) whose colour texture is in a
    // linear format hold a tint mask, not a colour: R and B are patch masks
    // and G the shading (cattle). The game colours it from the material
    // preset's tint bytes; without them only G is usable.
    let mask = s.name_hash == "295b85a9"
        && st.entries.iter().any(|e| e["role_guess"] == "base_color" && e["status"].as_str().is_some_and(|x| x.starts_with("ok")) && e["format"].as_str().is_some_and(|f| !f.ends_with("_SRGB")));
    if mask {
        c["tint_mask"] = true.into();
        c["tint_mask_note"] = "the colour texture is a tint mask (R, B: patch masks; G: shading), coloured in game through a tint palette that is not decoded: show G as grey".into();
    }
    // The two-layer vehicle shaders cover the whole car (body panels,
    // interior, mechanical parts). What colours each part is not decoded:
    // vertex colour alpha is a part id (0-255) into the material palette
    // (slot 19df25d7), whose stored texture is a placeholder the game
    // replaces. Two usable differences: plain body panels bind no layer
    // detail texture (`lyr0diffusetex` / `lyr1diffusetex` are engine
    // defaults) where interior and mechanical materials do, and body
    // panels hang on body bones (bodyshell, doors, wings, bonnet) where
    // seats, the steering wheel and lamp housings hang on their own. Only
    // untextured materials on body bones are taken as body paint; the rest
    // gets a neutral stand-in.
    if c["class"] == "car_paint" {
        let textured = st.entries.iter().any(|e| LAYER_DETAIL.iter().any(|h| e["param_hash"] == *h) && e["status"].as_str().is_some_and(|x| x.starts_with("ok")));
        c["paint_rule"] = "body paint only where neither layer has a detail texture (lyr0diffusetex, lyr1diffusetex) and the meshes hang on body bones; an inference, the per-part colours are not decoded".into();
        if textured || st.not_body {
            c["class"] = "opaque".into();
            c["stand_in"] = "trim".into();
            c["family"] = if textured { "vehicle interior, trim and mechanical parts (two-layer shader with a layer detail texture)" } else { "vehicle interior, lamp housings and mechanical parts (two-layer shader on interior, lamp or mechanical bones)" }.into();
        }
    }
    // Shared wheel rims (`standard_vehicle_2lyr_wheel_inst`): the colour is
    // set per vehicle while the game runs; nothing stored gives it.
    if s.name_hash == WHEEL_RIM {
        c["class"] = "opaque".into();
        c["stand_in"] = "rim".into();
        c["family"] = "wheel rim (standard_vehicle_2lyr_wheel_inst)".into();
        c["rim_note"] = "rim colour is set per vehicle at runtime and is not decoded: a plain alloy stand-in. The embedded tint palette is a placeholder of marker colours (red, yellow, green, ...) and is not applied".into();
    }
    if is_hidden_screen(st) {
        c["stand_in"] = "hidden".into();
        c["hidden_note"] = "runtime screen overlay: its colour map is drawn by the game while it runs (dials, needles, navigation) and is not in the files; left out so the stored screen picture under it shows".into();
    }
    c
}

/// A screen overlay whose picture only exists at runtime (`RUNTIME_SCREEN`).
fn is_hidden_screen(st: &ShaderTextures) -> bool {
    st.entries.iter().any(|e| e["role_guess"] == "base_color" && e["hash64"] == RUNTIME_SCREEN && !e["status"].as_str().is_some_and(|x| x.starts_with("ok")))
}

/// Not a visible surface: no colour, normal or emissive map is written.
fn is_hidden(s: &crate::drawable::Shader, st: &ShaderTextures) -> bool {
    classify_shader(s, st)["stand_in"] == "hidden"
}

/// The roof clip of a convertible as a glTF animation `va_<model>/roof`, and what model.json says
/// about it (null for other models).
fn roof_animation(a: &ArchiveInfo, name: &str, joints: &[crate::gltf::Joint]) -> (Vec<crate::gltf::Animation>, serde_json::Value) {
    // vehicles only (not every convertible has a `convertible_roof` bone)
    if !joints.iter().any(|j| j.name.eq_ignore_ascii_case("convertible_roof") || is_wheel_mount(&j.name)) {
        return (Vec::new(), serde_json::Value::Null);
    }
    let Some(work) = crate::names::work_dir() else {
        return (Vec::new(), serde_json::json!({"status": "not looked up: no work directory is set"}));
    };
    match crate::anim::vehicle_roof(&game_root(a), &work, name) {
        Ok(Some((de, d))) => {
            let c = d.clip("roof").expect("vehicle_roof checked the clip");
            let (channels, stats) = crate::outfit::clip_channels(&d, c, joints);
            let n = channels.len();
            let anim = crate::gltf::Animation { name: format!("{}/{}", d.name, c.short_name()), channels };
            let info = serde_json::json!({
                "status": "ok", "name": anim.name, "dictionary": d.name, "source": de.id, "duration": c.duration, "channels": n, "coverage": stats,
                "note": "The model info names the clip as in GTA V (animConvRoofDictName = joaat of va_<model>, animConvRoofName = joaat of roof). Time 0 is the roof up, the end the roof stowed. Written into the GLB as a glTF animation of the joint nodes.",
            });
            (vec![anim], info)
        }
        Ok(None) => (Vec::new(), serde_json::json!({"status": "no va_<model> clip dictionary with a roof clip in levels.rpf"})),
        Err(e) => (Vec::new(), serde_json::json!({"status": format!("not read: {e:#}")})),
    }
}

/// A wheel bone that carries a wheel: `wheel_lf`, `wheel_rm1`, and the inner
/// wheel of a dual pair, `wheel_lr_inr`. Not the helpers (`wheel_turn_lf`,
/// `wheel_lr_drive`, `wheel_parent_lr`).
fn is_wheel_mount(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let Some(r) = n.strip_prefix("wheel_") else { return false };
    let r = r.strip_suffix("_inr").unwrap_or(r);
    let b = r.as_bytes();
    b.len() >= 2 && matches!(b[0], b'l' | b'r') && matches!(b[1], b'f' | b'm' | b'r') && b[2..].iter().all(|c| c.is_ascii_digit())
}

/// Shaders drawn with the runtime body colour (see `classify_shader`).
fn is_paint(s: &crate::drawable::Shader, st: &ShaderTextures) -> bool {
    classify_shader(s, st)["class"] == "car_paint"
}

/// Resolve each shader's texture references in the ttpf stores, write the
/// decodable ones as shared PNGs, and pick base-colour and normal maps.
fn link_textures(a: &ArchiveInfo, shaders: &[crate::drawable::Shader], r: &Resource, embedded: &[texture::TexInfo], model_dir: &Path, o: &ExportOptions, part: Option<&str>) -> Vec<ShaderTextures> {
    let set = crate::ttpf::global(&game_root(a));
    // Textures embedded in the model itself (tint palettes), by store key.
    let emb: std::collections::HashMap<u64, &texture::TexInfo> = embedded.iter().filter_map(|t| texture::embedded_hash(r, t).map(|h| (h, t))).collect();
    let tdir = o.out.join("textures").join("ttpf");
    let uri_of = |p: &Path| -> String {
        // relative from the model folder: models/<archive>/<name>/ -> ../../../textures/ttpf/x.png
        let up = model_dir.strip_prefix(&o.out).map(|r| r.components().count()).unwrap_or(3);
        format!("{}{}", "../".repeat(up), rel(&o.out, p))
    };
    let mut out: Vec<ShaderTextures> = shaders
        .iter()
        .map(|s| {
            let mut st = ShaderTextures::default();
            let mut srgb_fallback: Option<Option<String>> = None; // first sRGB slot only
            let mut srgb_alpha = None;
            let mut normal_fallback = None;
            for t in &s.textures {
                let h0 = u64::from_str_radix(&t.hash64, 16).unwrap_or(0);
                let h = o.tex_remap.get(&h0).copied().unwrap_or(h0);
                let mut e = serde_json::json!({"param": t.param, "param_hash": t.param_hash, "param_name": crate::names::param(&t.param_hash), "hash64": t.hash64, "hash32": t.hash32});
                if h != h0 {
                    e["replaced_by"] = format!("{h:016x}").into();
                }
                let Some((store, tx)) = set.get(h) else {
                    match emb.get(&h) {
                        Some(et) => {
                            e["status"] = "embedded in the model".into();
                            e["width"] = et.width.into();
                            e["height"] = et.height.into();
                            e["format"] = et.format.clone().into();
                            if let Some(sr) = crate::material::slot_role(&t.param_hash) {
                                e["slot_role"] = sr.into();
                            }
                            match texture::embedded_rgba(r, et) {
                                Ok(rgba) => {
                                    if t.param_hash == TINT_PALETTE && et.height == 1 && (s.name_hash == WHEEL_RIM || s.name_hash == WHEEL_TYRE) {
                                        e["applied"] = "not applied: a placeholder of marker colours that the game replaces with the wheel colours".into();
                                    } else if t.param_hash == TINT_PALETTE && et.height == 1 {
                                        st.tint_palette = Some(rgba.chunks(4).map(|p| [p[0], p[1], p[2], p[3]]).collect());
                                        let (len, rows) = texture::tint_layout(et.width);
                                        e["tint_row_length"] = len.into();
                                        e["tint_rows"] = rows.into();
                                        e["applied"] = "GLB COLOR_0 = palette entry at vertex colour blue (row 0), multiplied into the base colour. Placed instances pick a row: entry tint_row * tint_row_length + COLOR0.b (tint_row from the placement, assumed to clamp at tint_rows - 1)".into();
                                    }
                                    e["status"] = "ok (embedded in the model)".into();
                                    if o.png {
                                        let p = model_dir.join("textures").join(format!("{}.png", t.hash64));
                                        let w = p.exists() || fs::create_dir_all(model_dir.join("textures")).map_err(anyhow::Error::from).and_then(|_| texture::save_png(&p, &rgba, et.width, et.height)).is_ok();
                                        if w {
                                            e["png"] = rel(&o.out, &p).into();
                                        }
                                    }
                                }
                                Err(err) => e["status"] = format!("embedded in the model, not decoded: {err}").into(),
                            }
                        }
                        None => {
                            e["status"] = "not in any ttpf store".into();
                            let role = unresolved_role(&t.param_hash);
                            e["role_guess"] = role.into();
                            if let Some(sr) = crate::material::slot_role(&t.param_hash) {
                                e["slot_role"] = sr.into();
                            }
                            e["texture_fallback"] = serde_json::json!({"reason": UNRESOLVED, "pending_role": role});
                        }
                    }
                    st.entries.push(e);
                    continue;
                };
                e["store"] = store.label().into();
                e["width"] = tx.width.into();
                e["height"] = tx.height.into();
                e["format"] = texture::dxgi_name(tx.dxgi_format).into();
                e["tile_mode"] = tx.tile_mode().into();
                let srgb = matches!(tx.dxgi_format, 29 | 72 | 75 | 78 | 91 | 99);
                let role = if ALBEDO_PARAMS.contains(&t.param_hash.as_str()) {
                    "base_color"
                } else if NORMAL_PARAMS.contains(&t.param_hash.as_str()) || tx.dxgi_format == 83 {
                    "normal"
                } else if t.param_hash == VEHICLE_PALETTE {
                    "palette"
                } else if srgb {
                    "colour (sRGB)"
                } else {
                    "data (linear)"
                };
                e["role_guess"] = role.into();
                if let Some(r) = crate::material::slot_role(&t.param_hash) {
                    e["slot_role"] = r.into();
                }
                if !o.png {
                    e["status"] = "found (PNG output off)".into();
                    st.entries.push(e);
                    continue;
                }
                // Tiny sRGB textures (4x4 defaults such as a black dirt map) are
                // never a usable base colour.
                let first_srgb = role == "colour (sRGB)" && srgb_fallback.is_none() && tx.width >= 32 && tx.height >= 32;
                match crate::ttpf::write_png(store, tx, &tdir) {
                    Ok((p, stt)) => {
                        let u = uri_of(&p);
                        e["png"] = rel(&o.out, &p).into();
                        e["status"] = "ok".into();
                        if stt.mip > 0 {
                            e["status"] = format!("ok (mip {} of {}x{}: top mips are BCPack)", stt.mip, tx.width, tx.height).into();
                            e["mip_used"] = stt.mip.into();
                        }
                        e["mean_rgba"] = serde_json::json!(stt.mean_rgba.map(|x| x.round()));
                        if stt.normal_like {
                            e["looks_like_normal_map"] = true.into();
                        }
                        if t.param_hash == EMISSIVE_PARAM && st.emissive.is_none() && !stt.normal_like {
                            st.emissive = Some(u.clone());
                        }
                        match role {
                            _ if stt.normal_like => {
                                if role == "normal" && st.normal.is_none() {
                                    st.normal = Some(u);
                                } else if normal_fallback.is_none() {
                                    normal_fallback = Some(u);
                                }
                                if first_srgb {
                                    srgb_fallback = Some(None);
                                }
                            }
                            "base_color" if st.base.is_none() => {
                                st.base = Some(u);
                                st.base_alpha = stt.alpha_range;
                            }
                            "normal" if st.normal.is_none() => st.normal = Some(u),
                            "colour (sRGB)" if first_srgb => {
                                srgb_fallback = Some(Some(u));
                                srgb_alpha = stt.alpha_range;
                            }
                            _ => {}
                        }
                    }
                    Err(err) => {
                        e["status"] = format!("not decoded: {err}").into();
                        if err.to_string().contains("BCPack") {
                            // Filled in below once the model's decoded colours are known.
                            e["texture_fallback"] = serde_json::json!({"reason": "BCPack: no mip outside BCPack chunks", "pending_role": if first_srgb { "base_color" } else { role }});
                        }
                        if first_srgb {
                            srgb_fallback = Some(None);
                        }
                    }
                }
                st.entries.push(e);
            }
            if st.base.is_none() {
                st.base = srgb_fallback.flatten();
                if st.base.is_some() {
                    st.base_alpha = srgb_alpha;
                }
            }
            if st.normal.is_none() {
                st.normal = normal_fallback;
            }
            if st.base.is_none() && o.png {
                if let Some((p, rec)) = bake_ped_tint(&st, part, o, &tdir, &game_root(a)) {
                    st.base = Some(uri_of(&p));
                    st.ped_tint = Some(rec);
                }
            }
            st
        })
        .collect();
    // BCPack-only textures: a flat colour. Colour slots take the average of
    // the model's decoded colour textures (same asset family), else a
    // neutral value by role; normal maps get a flat normal.
    let decoded: Vec<[f64; 4]> = out
        .iter()
        .flat_map(|st| st.entries.iter())
        .filter(|e| matches!(e["role_guess"].as_str(), Some("base_color") | Some("colour (sRGB)")) && e["looks_like_normal_map"].is_null())
        .filter_map(|e| serde_json::from_value::<[f64; 4]>(e["mean_rgba"].clone()).ok())
        .collect();
    let avg = (!decoded.is_empty()).then(|| {
        let mut a = [0.0; 4];
        for d in &decoded {
            for k in 0..4 {
                a[k] += d[k] / decoded.len() as f64;
            }
        }
        a.map(|x| x.round())
    });
    for (st, s) in out.iter_mut().zip(shaders) {
        let mut base_fb = None;
        let tinted = st.tint_palette.is_some();
        let decal = crate::material::classify(&s.name_hash, &s.flags)["class"] == "decal";
        for e in st.entries.iter_mut() {
            let Some(role) = e["texture_fallback"]["pending_role"].as_str().map(|x| x.to_string()) else { continue };
            let unresolved = e["texture_fallback"]["reason"] == UNRESOLVED;
            let colour = matches!(role.as_str(), "base_color" | "colour (sRGB)");
            let dflt = ENGINE_DEFAULTS.iter().find(|d| e["hash64"] == d.0);
            let typical = SLOT_TYPICAL.iter().find(|d| e["param_hash"] == d.0);
            let white = [255.0; 4];
            let (mut rgba, mut source) = match (colour, role.as_str(), avg) {
                (_, _, _) if unresolved && dflt.is_some() => (dflt.unwrap().1, dflt.unwrap().2.to_string()),
                (_, "tint palette", _) => (white, "white (no tint)".to_string()),
                (true, "base_color", _) if unresolved && tinted => (white, "white: the model's tint palette gives the colour".to_string()),
                (true, _, Some(a)) => (a, format!("average of {} decoded colour textures in this model", decoded.len())),
                (true, _, None) => ([128.0, 128.0, 128.0, 255.0], "neutral grey (no decoded colour texture in this model)".to_string()),
                (false, "normal", _) => ([128.0, 128.0, 255.0, 255.0], "flat normal".to_string()),
                _ if unresolved && typical.is_some() => (typical.unwrap().1, "typical value of this slot (median over decoded textures in it)".to_string()),
                _ => ([128.0, 128.0, 128.0, 255.0], "neutral mid value (linear data map)".to_string()),
            };
            // A decal without its colour texture would paint a flat patch.
            if role == "base_color" && decal && unresolved {
                rgba[3] = 0.0;
                source += "; alpha 0: decal hidden without its texture";
            }
            let reason = if unresolved { UNRESOLVED } else { "BCPack: no mip outside BCPack chunks (Xbox hardware texture codec; only BC1 decodes so far)" };
            let fb = serde_json::json!({"reason": reason, "role": role, "rgba": rgba, "source": source});
            e["texture_fallback"] = fb.clone();
            if role == "base_color" && base_fb.is_none() {
                base_fb = Some((rgba, fb));
            }
        }
        if let (None, Some((rgba, fb))) = (&st.base, base_fb) {
            st.base_factor = Some(rgba.map(|x| (x / 255.0) as f32));
            st.fallback = Some(fb);
        }
    }
    out
}

/// Ped clothing without a colour texture: colour its shade map (slot
/// `739e3d0d`: R = 255, G = shade, alpha about 127) through the part's tint
/// palette row (`ExportOptions::ped_tints`, tint 0), or as grey when the
/// part has no tint. Writes `<shade>_tint_<palette>_<row>.png` or
/// `<shade>_shade.png` next to the store textures; returns its path and the
/// model.json record.
fn bake_ped_tint(st: &ShaderTextures, part: Option<&str>, o: &ExportOptions, tdir: &Path, game: &Path) -> Option<(PathBuf, serde_json::Value)> {
    let e = st.entries.iter().find(|e| e["param_hash"] == crate::pedtint::SHADE_PARAM && e["status"].as_str().is_some_and(|x| x.starts_with("ok")))?;
    let m: [f64; 4] = serde_json::from_value(e["mean_rgba"].clone()).ok()?;
    if m[0] < 240.0 || !(96.0..=160.0).contains(&m[3]) {
        return None;
    }
    let shade = o.out.join(e["png"].as_str()?);
    let stem = shade.file_stem()?.to_string_lossy().to_string();
    let tint = part.and_then(|p| o.ped_tints.get(p)).copied();
    let pals = tint.map(|_| crate::pedtint::palettes(game));
    let pal = match (tint, &pals) {
        (Some((h, r)), Some(Ok(ps))) if r[0] != 255 => ps.get(&h).map(|p| (p, r[0])),
        _ => None,
    };
    let p = tdir.join(match (tint, pal) {
        (Some((h, r)), Some(_)) => format!("{stem}_tint_{h:08x}_{:02x}.png", r[0]),
        _ => format!("{stem}_shade.png"),
    });
    if !p.exists() {
        let img = image::open(&shade).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let tmp = p.with_extension(format!("{:?}.tmp.png", std::thread::current().id()).replace(['(', ')'], ""));
        texture::save_png(&tmp, &crate::pedtint::bake(img.as_raw(), pal), w, h).ok()?;
        fs::rename(&tmp, &p).ok()?;
    }
    let mut rec = serde_json::json!({"shade_texture": e["png"], "png": rel(&o.out, &p)});
    match (tint, pal) {
        (Some((h, r)), Some((pl, _))) => {
            rec["palette"] = format!("{h:08x}").into();
            rec["palette_name"] = crate::pedtint::palette_name(h).into();
            rec["rows"] = serde_json::json!(r);
            rec["mid_colour"] = serde_json::json!(pl.sample(r[0], 128));
            rec["applied"] = "base colour = palette row tint 0 at the shade map's G (texel 2 + G * 53 / 255); tint 1 and 2 (rows[1], rows[2]) are not applied: how the R and B channels select them is not decoded".into();
        }
        (Some((h, r)), None) => {
            rec["palette"] = format!("{h:08x}").into();
            rec["rows"] = serde_json::json!(r);
            rec["applied"] = match &pals {
                Some(Err(err)) => format!("grey shade only: palettes not loaded ({err})"),
                _ if r[0] == 255 => "grey shade only: tint 0 is off (255)".to_string(),
                _ => "grey shade only: this palette is not in textures.rpf's ped tint dictionary".to_string(),
            }
            .into();
        }
        _ => rec["applied"] = "grey shade only: no tint for this part (outfit --tint, from ped-variations); in game the colour comes from a tint palette".into(),
    }
    Some((p, rec))
}

fn rel(out: &Path, p: &Path) -> String {
    p.strip_prefix(out).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// Indexed resolution shared by all model-id consumers. Accepts full ids or bare hex offsets.
pub struct ModelResolver {
    entries: std::collections::HashMap<u64, usize>,
}
impl ModelResolver {
    pub fn new(entries: &[Entry]) -> Self {
        Self { entries: entries.iter().enumerate().map(|(i, e)| (e.off, i)).collect() }
    }
    pub fn resolve<'a, 's>(&self, entries: &'a [Entry], id: &'s str) -> Result<(&'a Entry, Option<&'s str>)> {
        let (base, member) = id.split_once('#').map_or((id, None), |(b, m)| (b, Some(m)));
        let hex = base.rsplit_once('@').map_or(base, |(_, off)| off);
        let off = u64::from_str_radix(hex.trim_start_matches("0x"), 16)?;
        let e = self.entries.get(&off).and_then(|&i| entries.get(i)).ok_or_else(|| anyhow!("no entry {base}"))?;
        if let Some((archive, _)) = base.rsplit_once('@') {
            let source_archive = e.id.rsplit_once('@').map_or("", |(a, _)| a);
            anyhow::ensure!(archive.eq_ignore_ascii_case(source_archive) || game::index_stem(archive).eq_ignore_ascii_case(&game::index_stem(source_archive)), "no entry {base}");
        }
        if let Some(name) = member {
            anyhow::ensure!(matches!(e.kind, Kind::RageObj | Kind::Resource) && matches!(e.rtype.as_deref(), Some("dr" | "dd")), "{} is not a drawable dictionary", e.id);
            member_entry(e, name)?;
        }
        Ok((e, member))
    }
}

/// Select a dictionary member while retaining stream metadata.
/// The member is validated against the resource when exported.
pub fn member_entry(e: &Entry, member: &str) -> Result<Entry> {
    if member.len() < 2 || !member.bytes().all(crate::resource::is_name_char) {
        return Err(anyhow!("invalid drawable member name: {member}"));
    }
    let mut selected = e.clone();
    selected.id = format!("{}#{member}", e.id.split('#').next().unwrap());
    selected.name = Some(member.to_string());
    Ok(selected)
}

/// Shared cache directory name for whole resources and selected members.
pub fn export_base_name(e: &Entry) -> String {
    let member = e.id.split_once('#').map(|(_, name)| name);
    let name = member.or(e.name.as_deref()).unwrap_or("unnamed");
    format!("{}_{:x}{}", sanitize(name), e.off, if member.is_some() { "_member" } else { "" })
}

/// Export one entry. Returns the files written.
pub fn export_entry(a: &ArchiveInfo, e: &Entry, o: &ExportOptions) -> Result<ExportedItem> {
    // Callers may pass a suffixed id directly on a cloned index entry.
    let selected = e.id.split_once('#').map(|(_, member)| member_entry(e, member)).transpose()?;
    let e = selected.as_ref().unwrap_or(e);
    let requested_member = e.id.split_once('#').map(|(_, member)| member);
    let astem = game::index_stem(&a.id);
    let mut item = ExportedItem {
        id: e.id.clone(),
        archive: a.id.clone(),
        kind: e.kind.name(),
        rtype: e.rtype.clone(),
        name: e.name.clone(),
        status: "ok".into(),
        files: vec![],
        error: None,
        warnings: vec![],
    };
    let base_name = export_base_name(e);
    // Unnamed carved resources have no type; those with shaders are drawables
    // (reached through archetype links, `sixng link-archetypes`).
    let drawable_like = e.kind == Kind::RageObj && e.rtype.is_none() && Resource::load_sys(&a.path, e).map(|r| !crate::drawable::find_shaders(&r).is_empty()).unwrap_or(false);
    match (e.kind, if drawable_like { Some("dr") } else { e.rtype.as_deref() }) {
        (Kind::RageObj | Kind::Resource, Some("txd")) => {
            let dir = o.out.join("textures").join(&astem).join(&base_name);
            let marker = dir.join("dictionary.json");
            if marker.exists() && !o.force {
                item.status = "skipped".into();
                return Ok(item);
            }
            fs::create_dir_all(&dir)?;
            let r = Resource::load(&a.path, e)?;
            let texs = texture::find_textures(&r);
            let mut out_tex = Vec::new();
            for t in &texs {
                // One undecodable texture (e.g. R32_FLOAT) must not fail the dictionary.
                match export_texture(&r, t, &dir, o, &mut item) {
                    Ok(files) => out_tex.push(serde_json::json!({ "info": t, "files": files })),
                    Err(err) => {
                        item.warnings.push(format!("{}: {err}", t.name));
                        out_tex.push(serde_json::json!({ "info": t, "files": [], "error": err.to_string() }))
                    }
                }
            }
            let j = serde_json::json!({
                "source": { "archive": a.id, "id": e.id, "offset": e.off },
                "name": e.name, "textures": out_tex,
            });
            fs::write(&marker, serde_json::to_vec_pretty(&j)?)?;
            item.files.push(rel(&o.out, &marker));
        }
        (Kind::RageObj | Kind::Resource, Some(_)) => {
            let dir = o.out.join("models").join(&astem).join(&base_name);
            let marker = dir.join("model.json");
            // Old exports could leave the primary mesh in model_lodN.glb.
            // Repair those caches, while keeping resources without meshes cached.
            let missing_model = !dir.join("model.glb").exists() && (requested_member.is_some()
                || fs::read(&marker).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                    .and_then(|meta| meta["meshes"].as_array().map(|meshes| !meshes.is_empty())).unwrap_or(false));
            if marker.exists() && !o.force && !missing_model {
                item.status = "skipped".into();
                return Ok(item);
            }
            fs::create_dir_all(dir.join("buffers"))?;
            let r = Resource::load(&a.path, e)?;
            let mut info = crate::drawable::describe(&r);
            let wheel_discs = if info.skeleton.iter().any(|b| b.name.starts_with("wheel_")) { crate::bounds::wheel_discs(&r) } else { Vec::new() };
            if let Some(name) = requested_member {
                crate::drawable::select_member(&r, &mut info, name)?;
            }
            let mut bufs = Vec::new();
            for (i, b) in info.buffers.iter().enumerate() {
                let end = (b.gfx_off + b.bytes).min(r.gfx.len() as u64) as usize;
                let p = dir.join("buffers").join(format!("{:02}_{:x}.bin", i, b.gfx_off));
                fs::write(&p, &r.gfx[b.gfx_off as usize..end])?;
                bufs.push(rel(&dir, &p));
                item.files.push(rel(&o.out, &p));
            }
            let mut tex_files = Vec::new();
            if !o.skip_embedded_textures && !info.embedded_textures.is_empty() {
                let tdir = dir.join("textures");
                fs::create_dir_all(&tdir)?;
                for t in &info.embedded_textures {
                    match export_texture(&r, t, &tdir, o, &mut item) {
                        Ok(f) => tex_files.push(f),
                        Err(err) => item.warnings.push(format!("{}: {err}", t.name)),
                    }
                }
            }
            fs::write(dir.join("sys.bin"), &r.sys)?;
            item.files.push(rel(&o.out, &dir.join("sys.bin")));
            let BuiltDrawable { lod_groups, part_groups, materials: mats, joints, mesh_meta, shader_tex, head } = build_drawable(a, e, &r, &info, &dir, requested_member.is_some(), o, &mut item);
            let all = || lod_groups.iter().flatten().chain(part_groups.iter()).flat_map(|g| &g.prims);
            let ntris: usize = all().map(|p| p.indices.len() / 3).sum();
            let nverts: usize = all().map(|p| p.positions.len()).sum();
            let extras = serde_json::json!({
                "source_id": e.id, "type": e.rtype, "coordinate_system": COORDS,
                "surface_hint": e.name.as_deref().and_then(surface_hint),
            });
            let mut glb_file = None;
            let mut lod_files = Vec::new();
            let mut parts_file = None;
            let mut points_file = None;
            for old in fs::read_dir(&dir)?.flatten() {
                let n = old.file_name().to_string_lossy().to_string();
                if n == "model.glb" || n == "model_parts.glb" || (n.starts_with("model_lod") && n.ends_with(".glb")) {
                    let _ = fs::remove_file(old.path());
                }
            }
            let lods = lod_exports(&lod_groups);
            let primary = lods.first().map_or(0, |l| l.0);
            // Convertibles: the roof-folding clip as a glTF animation.
            let (roof_anims, roof_info) = roof_animation(a, e.name.as_deref().unwrap_or(""), &joints);
            if ntris > 0 {
                for (lod, fname) in &lods {
                    let lod = *lod;
                    let groups = &lod_groups[lod];
                    let p = dir.join(&fname);
                    let mut ex = extras.clone();
                    ex["lod"] = serde_json::json!(lod);
                    let nm = format!("{}{}", e.name.as_deref().unwrap_or("model"), if lod == 0 { String::new() } else { format!("_lod{lod}") });
                    ex["roof_animation"] = roof_info.clone();
                    let opt = crate::gltf::GlbOptions { animations: &roof_anims, ..Default::default() };
                    fs::write(&p, crate::gltf::mesh_glb_with(&nm, groups, &mats, &joints, ex, &opt))?;
                    item.files.push(rel(&o.out, &p));
                    if glb_file.is_none() {
                        glb_file = Some(rel(&dir, &p));
                    }
                    lod_files.push(rel(&dir, &p));
                }
                if part_groups.iter().any(|g| !g.prims.is_empty()) {
                    let p = dir.join("model_parts.glb");
                    let mut ex = extras.clone();
                    ex["parts"] = serde_json::json!(true);
                    fs::write(&p, crate::gltf::mesh_glb(&format!("{}_parts", e.name.as_deref().unwrap_or("model")), &part_groups, &mats, &joints, ex))?;
                    item.files.push(rel(&o.out, &p));
                    parts_file = Some(rel(&dir, &p));
                }
            }
            // Fallback: point cloud for mesh blobs without an index buffer.
            let paired: std::collections::HashSet<usize> = info.geometries.iter().map(|g| g.vertex_buffer).collect();
            let mut prims = Vec::new();
            for (i, b) in info.buffers.iter().enumerate() {
                if let (Some(h), false, true) = (&b.mesh_blob, paired.contains(&i), requested_member.is_none()) {
                    let end = (b.gfx_off + b.bytes).min(r.gfx.len() as u64) as usize;
                    let pos = crate::drawable::decode_positions(&r.gfx[(b.gfx_off as usize).min(end)..end], h);
                    prims.push(crate::gltf::PointPrim { name: format!("blob{:02}_{:x}", i, b.gfx_off), positions: pos });
                }
            }
            let _ = fs::remove_file(dir.join("points.glb")); // stale from older runs
            if prims.iter().any(|p| !p.positions.is_empty()) {
                let p = dir.join("points.glb");
                fs::write(&p, crate::gltf::points_glb(e.name.as_deref().unwrap_or("model"), &prims, extras))?;
                item.files.push(rel(&o.out, &p));
                points_file = Some(rel(&dir, &p));
            }
            let geometry_bbox = {
                let mut g = [[f32::MAX; 3], [f32::MIN; 3]];
                let mut any = false;
                for mm in mesh_meta.iter().filter(|mm| mm["lod"].as_u64().unwrap_or(0) == if requested_member.is_some() { primary as u64 } else { 0 }) {
                    if let Ok(b) = serde_json::from_value::<[[f32; 3]; 2]>(mm["bbox"].clone()) {
                        any = true;
                        for k in 0..3 {
                            g[0][k] = g[0][k].min(b[0][k]);
                            g[1][k] = g[1][k].max(b[1][k]);
                        }
                    }
                }
                if any { serde_json::json!(g) } else { serde_json::Value::Null }
            };
            let j = serde_json::json!({
                "source": { "archive": a.id, "id": e.id, "offset": e.off, "streams": e.streams },
                "name": e.name, "type": e.rtype,
                "geometry": "triangle meshes, one GLB per LOD (model.glb = lowest populated LOD, model_lodN.glb). Y-up, metres; one node per geometry grouped by model; fragments carry the skeleton as glTF joints and a rigid skin (JOINTS_0 = bone). Mesh blobs without an index buffer go to points.glb. Material names are shader index + unresolved name hash.",
                "glb": glb_file, "lod_glbs": lod_files, "parts_glb": parts_file,
                "parts_note": "model_parts.glb: models of embedded drawables (fragment children at root+0x10, nested sub-drawables), not LODs. LOD levels come from the main drawable's LOD group (+0x50 + 8*k). Placement of fragment-child parts is unverified.", "points_glb": points_file, "vertex_count": nverts, "triangle_count": ntris,
                "coordinate_system": COORDS,
                "surface_hint": e.name.as_deref().and_then(surface_hint),
                "surface_hint_source": "model name (collision/material surface types are not decoded yet)",
                "geometry_bbox": geometry_bbox,
                "geometry_bbox_note": "RAGE model space (Z-up) [min, max] over the primary meshes (LOD0, or the lowest populated LOD for a dictionary member), from the decoded vertices. Prefer it to drawable.bounds / bounds_raw: on some fragments and drawables the stored bbox is offset or rotated (route-sign prefabs, dn_20_scaffolding_02); the archetype definitions agree with the geometry.",
                "meshes": mesh_meta,
                "wheel_mounts": info.skeleton.iter().filter(|b| is_wheel_mount(&b.name) && b.pivot.is_some()).map(|b| {
                    // the wheel's collision disc: the largest one on this axle
                    // line (it can sit a few cm in or out of the bone's pivot)
                    let p = b.pivot.unwrap_or_default();
                    // (of two equally large, the nearer: the outer and inner wheel of a dual pair can be 12 cm apart)
                    let near: Vec<&crate::bounds::WheelDisc> = wheel_discs.iter().filter(|d| (d.centre[0] - p[0]).abs() < 0.15 && (d.centre[1] - p[1]).abs() < 0.03 && (d.centre[2] - p[2]).abs() < 0.03).collect();
                    let big = near.iter().map(|d| d.radius).fold(0.0f32, f32::max);
                    let d = near.iter().filter(|d| d.radius > big - 1e-3).min_by(|a, b| (a.centre[0] - p[0]).abs().total_cmp(&(b.centre[0] - p[0]).abs())).copied();
                    serde_json::json!({"bone": b.name, "pivot": b.pivot, "radius": d.map(|d| d.radius), "half_width": d.map(|d| d.half_width), "centre": d.map(|d| d.centre)})
                }).collect::<Vec<_>>(),
                "wheel_mounts_version": 2,
                "roof_animation": roof_info,
                "wheel_discs": wheel_discs,
                "wheel_discs_note": "Disc bounds of the fragment's physics (type 8 children of its composite bound): centre, radius and half width in metres. The largest disc on a wheel bone's axle line is the wheel (its centre, radius and half_width are repeated in wheel_mounts); a thinner one is the brake disc.",
                "wheel_mounts_note": "Wheel centres (model space, RAGE Z-up). Rims and tyres are shared models (rim_<class>_NNN, tyre_<class>_NNN) authored for the left-front at unit scale (rim radius 0.5); mirror X for right-hand wheels and scale by the vehicle's wheel size (not decoded yet).",
                "drawable": info, "buffer_files": bufs, "texture_files": tex_files,
                "materials": info.shaders.iter().zip(shader_tex.iter()).enumerate().map(|(si, (s, st))| {
                    let c = classify_shader(s, st);
                    let mut m = serde_json::json!({
                        "shader": s.index, "shader_name_hash": s.name_hash, "shader_name": crate::names::shader(&s.name_hash), "classification": c,
                        "base_color": if is_paint(s, st) { None } else { st.base.clone() }, "normal": st.normal, "normal_green": "directx (+Y down); invert G for glTF/OpenGL",
                        "textures": st.entries,
                    });
                    if let Some(fb) = &st.fallback {
                        m["texture_fallback"] = fb.clone();
                    }
                    if let Some(l) = crate::material::multilayer(s, &st.entries, || material_uv_fit(&lod_groups, si)) {
                        m["multilayer"] = l;
                    }
                    if m["classification"]["class"] == "car_paint" {
                        m["paint"] = crate::material::paint_block();
                        if let Some(c) = o.paint {
                            m["paint"]["colour"] = serde_json::json!(c);
                            m["paint"]["colour_source"] = serde_json::json!("--paint (linear RGB, also the glTF baseColorFactor)");
                        }
                    }
                    m
                }).collect::<Vec<_>>(),
                "materials_note": "Textures come from the data/ttdb/*.ttpf stores by 64-bit hash and are written once to <out>/textures/ttpf/<hash>.png. Roles are guessed from the shader parameter hash and the format (sRGB = colour, BC5 / known normal slots = normal map). classification.class is one of opaque/alpha/cutout/decal/glass/window/emissive/water/terrain_multilayer/car_paint, from the shader record flags and known shader families. shader_name and param_name are joaat names from <work>/names/names.json (`sixng names`), null when unknown. slot_role names the PBR map a known parameter slot holds.",
            });
            let mut j = j;
            if let Some((_, rec)) = head.as_ref().filter(|h| !h.1["preset"].is_null()) {
                j["part_skin"] = rec.clone();
                j["part_skin_note"] = "Animal parts: the colour texture comes from the part's material preset (packs.rpf meta, struct f795e8ba) when the drawable's own one is in no store or is a tint mask (cattle: R and B patch masks, G shading, coloured in game by the preset's tint bytes through a palette that is not decoded). preset: name hash of the preset used; slot_presets: how many presets (variations) the part has; replaced: the texture the drawable binds.".into();
            } else if e.name.as_deref().is_some_and(|n| crate::partskin::animal_root(n).is_some()) {
                // null: an animal part that needed no preset (dates the export)
                j["part_skin"] = serde_json::Value::Null;
            }
            if let Some((_, rec)) = head.as_ref().filter(|h| h.1["preset"].is_null()) {
                j["head_skin"] = rec.clone();
                j["head_skin_note"] = "Heads bind no textures; the face comes from the material preset whose drawable tag lists this head (source \"linked\"), else from the faces of linked heads with the same sex, body type and ethnicity code (\"auto\"). index / count: the variant chosen with --head-skin.".into();
            }
            fs::write(&marker, serde_json::to_vec_pretty(&j)?)?;
            item.files.push(rel(&o.out, &marker));
        }
        _ => {
            let dir = o.out.join("raw").join(&astem);
            let p = dir.join(format!("{}.{}", base_name, e.ext));
            if p.exists() && !o.force {
                item.status = "skipped".into();
                return Ok(item);
            }
            fs::create_dir_all(&dir)?;
            let data = scan::read_entry(&a.path, e)?;
            fs::write(&p, &data)?;
            item.files.push(rel(&o.out, &p));
        }
    }
    if o.raw && matches!(e.kind, Kind::RageObj | Kind::Resource) {
        let dir = o.out.join("raw").join(&astem);
        fs::create_dir_all(&dir)?;
        let p = dir.join(format!("{base_name}.{}", e.ext));
        if !p.exists() || o.force {
            fs::write(&p, scan::read_entry(&a.path, e)?)?;
        }
        item.files.push(rel(&o.out, &p));
    }
    Ok(item)
}

/// A drawable's meshes, materials and skin, ready for `gltf::mesh_glb`.
/// Measured UV mapping of one material over the first populated LOD.
fn material_uv_fit(lod_groups: &[Vec<crate::gltf::NodeGroup>], shader: usize) -> serde_json::Value {
    let lod = lod_groups.iter().find(|g| g.iter().any(|n| !n.prims.is_empty()));
    crate::material::uv_fit(lod.into_iter().flatten().flat_map(|n| &n.prims).filter(|p| p.material == Some(shader)))
}

pub(crate) struct BuiltDrawable {
    /// Node groups per LOD (index = LOD level).
    pub lod_groups: Vec<Vec<crate::gltf::NodeGroup>>,
    /// Models of embedded drawables (fragment children, sub-drawables).
    pub part_groups: Vec<crate::gltf::NodeGroup>,
    pub materials: Vec<crate::gltf::Material>,
    pub joints: Vec<crate::gltf::Joint>,
    /// model.json `meshes` records.
    pub mesh_meta: Vec<serde_json::Value>,
    shader_tex: Vec<ShaderTextures>,
    /// Head skin shaders and the model.json record, for heads.
    head: Option<(Vec<crate::drawable::Shader>, serde_json::Value)>,
}

/// Decode a drawable's geometry, skin and materials. Texture URIs are
/// relative to `dir` (a folder under `o.out`); textures are written under
/// `o.out`. `member`: `info` is one drawable-dictionary member
/// (`drawable::select_member`), so geometries outside its models are left out.
pub(crate) fn build_drawable(a: &ArchiveInfo, e: &Entry, r: &Resource, info: &crate::drawable::DrawableInfo, dir: &Path, member: bool, o: &ExportOptions, item: &mut ExportedItem) -> BuiltDrawable {
    let head = head_skin_shaders(a, e, &info.shaders, o, item).or_else(|| part_skin_shaders(a, e, &info.shaders, item));
    let shader_tex = link_textures(a, head.as_ref().map_or(&info.shaders[..], |h| &h.0[..]), r, &info.embedded_textures, dir, o, e.name.as_deref());
    let slice = |i: usize| {
        let b = &info.buffers[i];
        let end = (b.gfx_off + b.bytes).min(r.gfx.len() as u64) as usize;
        &r.gfx[(b.gfx_off as usize).min(end)..end]
    };
    let skin = ped_skin(a, e, r, info, o, item);
    // Triangle meshes: one primitive per geometry, grouped by model.
    let mut mesh_meta = Vec::new();
    let prims_of = |gi: usize, material: Option<usize>, mesh_meta: &mut Vec<serde_json::Value>| -> Option<crate::gltf::TriPrim> {
        let g = &info.geometries[gi];
        let h = info.buffers[g.vertex_buffer].mesh_blob.as_ref()?;
        let mut m = crate::drawable::decode_mesh(slice(g.vertex_buffer), h, slice(g.index_buffer));
        // Ped clothing: joint index -> bone palette -> joint list.
        if let Some(map) = skin.geometry_map.get(&gi) {
            let remap = |js: &mut Vec<[u16; 4]>| js.iter_mut().flatten().for_each(|j| *j = map.get(*j as usize).copied().unwrap_or(u16::MAX));
            m.joints.as_mut().map(remap);
            m.joints1.as_mut().map(remap);
            // rigid part (one-bone palette): every vertex on that bone
            if m.joints.is_none() && map.len() == 1 && map[0] != u16::MAX {
                m.joints = Some(vec![[map[0], 0, 0, 0]; m.positions.len()]);
                m.weights = Some(vec![[255, 0, 0, 0]; m.positions.len()]);
            }
        }
        // Bone indices with non-zero weight in either influence set.
        let skin_used: Option<Vec<u16>> = m.joints.as_ref().map(|js| {
            let w0 = m.weights.as_ref().unwrap();
            let mut u: Vec<u16> = Vec::new();
            for (i, j) in js.iter().enumerate() {
                u.extend((0..4).filter(|&k| w0[i][k] > 0).map(|k| j[k]));
                if let (Some(j1), Some(w1)) = (&m.joints1, &m.weights1) {
                    u.extend((0..4).filter(|&k| w1[i][k] > 0).map(|k| j1[i][k]));
                }
            }
            u.sort();
            u.dedup();
            u
        });
        let name = format!("geom{:02}_{:x}", gi, info.buffers[g.vertex_buffer].gfx_off);
        let mut bb = [[f32::MAX; 3], [f32::MIN; 3]];
        for p in &m.positions {
            for k in 0..3 {
                bb[0][k] = bb[0][k].min(p[k]);
                bb[1][k] = bb[1][k].max(p[k]);
            }
        }
        mesh_meta.push(serde_json::json!({
            "name": name, "geometry": gi, "vertex_count": m.positions.len(), "triangle_count": m.indices.len() / 3,
            "bbox": if m.positions.is_empty() { serde_json::Value::Null } else { serde_json::json!(bb) },
            "exceeds_kn5_16bit_limit": m.positions.len() > 65535,
            "material": material.and_then(|s| info.shaders.get(s)).map(|s| format!("shader{:02}_{}", s.index, s.name_hash)),
            "shader_index": material,
            "uv_sets": m.uvs.len(), "uv_slots": m.uvs.iter().map(|u| u.0).collect::<Vec<_>>(),
            "normals": m.normals.is_some(), "vertex_colors_raw": m.colors.is_some(),
            "triangles_flipped_to_match_normals": m.flipped, "triangles_dropped": m.dropped, "normals_rebuilt_from_faces": m.rebuilt_normals,
            "position_axis_order": m.axis_order, "skinned": m.joints.is_some(), "joint_format": m.joint_format.map(|f| format!("{f:02x}")),
            "skin_influences": m.joints.as_ref().map(|j| j.iter().zip(m.weights.as_ref().unwrap()).enumerate().map(|(i, (a, w))| w.iter().filter(|&&x| x > 0).count().max(1).min(a.len()) + m.weights1.as_ref().map_or(0, |w1| w1[i].iter().filter(|&&x| x > 0).count())).max().unwrap_or(1)),
            "bones": skin_used.as_ref().map(|u| u.iter().filter_map(|&i| skin.joints.get(i as usize).map(|x| x.name.clone())).collect::<Vec<_>>()),
            // Joint indices with no bone (out of range, or a palette
            // bone that is in neither skeleton); their weight is moved
            // to the largest kept influence in the GLB.
            "unresolved_joints": skin_used.as_ref().map(|u| u.iter().filter(|&&i| i as usize >= skin.joints.len()).count()),
        }));
        let tint = match (material.and_then(|s| shader_tex.get(s)).and_then(|st| st.tint_palette.as_ref()), &m.colors) {
            (Some(p), Some(c)) => Some(tint_colours(p, c)),
            _ => None,
        };
        if let Some(mm) = mesh_meta.last_mut() {
            mm["tint_applied"] = tint.is_some().into();
        }
        Some(crate::gltf::TriPrim {
            name, positions: m.positions, normals: m.normals, tangents: m.tangents, uvs: m.uvs.into_iter().map(|u| u.1).collect(),
            colors: m.colors, tint, indices: m.indices, material, joints: m.joints, weights: m.weights, joints1: m.joints1, weights1: m.weights1,
        })
    };
    let nlod = info.models.iter().filter(|m| !m.part).map(|m| m.lod + 1).max().unwrap_or(1);
    let mut lod_groups: Vec<Vec<crate::gltf::NodeGroup>> = (0..nlod).map(|_| Vec::new()).collect();
    // models of embedded drawables (fragment children, sub-drawables)
    let mut part_groups: Vec<crate::gltf::NodeGroup> = Vec::new();
    let mut used = vec![false; info.geometries.len()];
    for (mi, md) in info.models.iter().enumerate() {
        let mut prims = Vec::new();
        for (k, &gi) in md.geometries.iter().enumerate() {
            used[gi] = true;
            let sh = md.shader_indices.get(k).map(|&s| s as usize).filter(|&s| s < info.shaders.len());
            let before = mesh_meta.len();
            prims.extend(prims_of(gi, sh, &mut mesh_meta));
            for mm in mesh_meta[before..].iter_mut() {
                mm["lod"] = if md.part { serde_json::Value::Null } else { serde_json::json!(md.lod) };
                mm["model"] = serde_json::json!(mi);
                if md.part {
                    mm["part"] = serde_json::json!(true);
                }
            }
        }
        if md.part {
            part_groups.push(crate::gltf::NodeGroup { name: format!("model{mi}"), prims });
        } else {
            lod_groups[md.lod].push(crate::gltf::NodeGroup { name: format!("model{mi}"), prims });
        }
    }
    let rest: Vec<usize> = (0..info.geometries.len()).filter(|&i| !used[i]).collect();
    if !member && !rest.is_empty() {
        let prims = rest.into_iter().filter_map(|gi| prims_of(gi, None, &mut mesh_meta)).collect();
        lod_groups[0].push(crate::gltf::NodeGroup { name: "unassigned".into(), prims });
    }
    // Vehicles: shaders whose meshes hang mostly on interior, lamp or
    // mechanical bones are not body paint (see `classify_shader`).
    let mut shader_tex = shader_tex;
    for (si, st) in shader_tex.iter_mut().enumerate() {
        let mut bones: Vec<&str> = mesh_meta.iter().filter(|m| m["shader_index"] == si).flat_map(|m| m["bones"].as_array().into_iter().flatten()).filter_map(|b| b.as_str()).collect();
        bones.sort();
        bones.dedup();
        let off = bones.iter().filter(|b| NOT_BODY_BONES.iter().any(|k| b.contains(k))).count();
        st.not_body = !bones.is_empty() && off * 10 >= bones.len() * 7;
    }
    let mats: Vec<crate::gltf::Material> = info.shaders.iter().zip(shader_tex.iter()).enumerate().map(|(si, (s, st))| crate::gltf::Material {
        name: format!("shader{:02}_{}", s.index, s.name_hash),
        // Paint: the colour slot is the engine-default white; any sRGB
        // fallback (decal layer, masks) would be wrong as albedo.
        base_color_uri: if is_paint(s, st) || is_hidden(s, st) { None } else { st.base.clone() },
        normal_uri: if is_hidden(s, st) { None } else { st.normal.clone() },
        gltf: if is_hidden(s, st) {
            crate::material::gltf_fields(&classify_shader(s, st), false)
        } else {
            let mut g = crate::material::gltf_fields(&classify_shader(s, st), st.base.is_some() && !is_paint(s, st));
            // Lamps, neon, screens: the emissive shaders (`standard_emissive`,
            // `standard_emissive_layer` …, by recovered name) sit in any
            // render bucket; their colour map is what lights up.
            if g["emissiveFactor"].is_null() && crate::names::shader(&s.name_hash).is_some_and(|n| n.contains("emissive")) {
                g["emissiveFactor"] = serde_json::json!([1.0, 1.0, 1.0]);
                g["emissiveTexture"] = serde_json::json!("base");
            }
            // A shader with its own emissive map lights only what that map
            // shows (lamp bulbs, not the whole lamp unit).
            if let (false, Some(u)) = (g["emissiveFactor"].is_null(), &st.emissive) {
                g["emissiveTexture"] = serde_json::json!({"uri": u});
            }
            if let (Some(p), true) = (o.paint, is_paint(s, st)) {
                g["pbrMetallicRoughness"]["baseColorFactor"] = serde_json::json!([p[0], p[1], p[2], 1.0]);
            }
            if let (Some(f), false) = (st.base_factor, is_paint(s, st)) {
                if g["pbrMetallicRoughness"].is_null() {
                    g["pbrMetallicRoughness"] = serde_json::json!({});
                }
                if g["pbrMetallicRoughness"]["baseColorFactor"].is_null() {
                    g["pbrMetallicRoughness"]["baseColorFactor"] = serde_json::json!(f);
                }
            }
            g
        },
        extras: {
            let mut x = serde_json::json!({"shader_name_hash": s.name_hash, "shader_name": crate::names::shader(&s.name_hash), "classification": classify_shader(s, st), "textures": st.entries, "texture_fallback": st.fallback});
            if let Some(t) = &st.ped_tint {
                x["ped_tint"] = t.clone();
            }
            if let Some(l) = crate::material::multilayer(s, &st.entries, || material_uv_fit(&lod_groups, si)) {
                x["multilayer"] = l;
            }
            x
        },
    }).collect();
    BuiltDrawable { lod_groups, part_groups, materials: mats, joints: skin.joints, mesh_meta, shader_tex, head }
}

/// Joint list of a drawable's GLB skin, and for ped clothing the joint
/// index remap of each geometry.
struct SkinSetup {
    joints: Vec<crate::gltf::Joint>,
    /// geometry index -> (vertex joint index -> joint list index; u16::MAX = none)
    geometry_map: std::collections::HashMap<usize, Vec<u16>>,
}

/// Ped heads bind no textures: add the face (albedo and normal map) chosen
/// by `headskin` to their texture-less shaders, with the choice record for
/// model.json. None when not a head.
fn head_skin_shaders(a: &ArchiveInfo, e: &Entry, shaders: &[crate::drawable::Shader], o: &ExportOptions, item: &mut ExportedItem) -> Option<(Vec<crate::drawable::Shader>, serde_json::Value)> {
    let name = e.name.as_deref()?;
    if !crate::headskin::is_head(name) || shaders.iter().any(|s| !s.textures.is_empty()) {
        return None;
    }
    let t = match crate::headskin::get(&game_root(a)) {
        Ok(t) => t,
        Err(err) => {
            item.warnings.push(format!("head skin: {err}"));
            return None;
        }
    };
    let set = crate::ttpf::global(&game_root(a));
    let Some(c) = t.choose(name, o.head_skin, |h| set.get(h).is_some()) else {
        item.warnings.push("head skin: no face texture found".into());
        return None;
    };
    let rec = serde_json::json!({"source": c.source, "group": c.group, "index": c.index, "count": c.count, "albedo": format!("{:016x}", c.skin.albedo), "normal": format!("{:016x}", c.skin.normal)});
    let tex = |param_hash: &str, h: u64| crate::drawable::TexRef { param: 0, param_hash: param_hash.into(), hash64: format!("{h:016x}"), hash32: String::new() };
    let sh = shaders
        .iter()
        .map(|s| {
            let mut s = s.clone();
            s.textures.push(tex(ALBEDO_PARAMS[0], c.skin.albedo));
            if c.skin.normal != 0 {
                s.textures.push(tex("1e77bf4c", c.skin.normal));
            }
            s
        })
        .collect();
    Some((sh, rec))
}

/// Shaders of an animal part with the colour texture of its material preset
/// (`partskin`), where the drawable's own one is in no store or is a tint
/// mask (a linear format), with the choice record for model.json.
fn part_skin_shaders(a: &ArchiveInfo, e: &Entry, shaders: &[crate::drawable::Shader], item: &mut ExportedItem) -> Option<(Vec<crate::drawable::Shader>, serde_json::Value)> {
    let name = e.name.as_deref()?;
    crate::partskin::animal_root(name)?;
    let set = crate::ttpf::global(&game_root(a));
    let painted = |h: u64| set.get(h).is_some_and(|(_, t)| matches!(t.dxgi_format, 29 | 72 | 75 | 78 | 91 | 99));
    let bound = |s: &crate::drawable::Shader| s.textures.iter().find(|t| ALBEDO_PARAMS.contains(&t.param_hash.as_str())).and_then(|t| u64::from_str_radix(&t.hash64, 16).ok());
    if !shaders.iter().any(|s| bound(s).is_some_and(|h| !painted(h))) {
        return None;
    }
    let t = match crate::partskin::get(&game_root(a)) {
        Ok(t) => t,
        Err(err) => {
            item.warnings.push(format!("part skin: {err}"));
            return None;
        }
    };
    let mut recs = Vec::new();
    let sh = shaders
        .iter()
        .map(|s| {
            let mut s = s.clone();
            let Some(b) = bound(&s).filter(|&h| !painted(h)) else { return s };
            let Some(c) = t.choose(name, b, painted) else { return s };
            for tx in s.textures.iter_mut() {
                let new = if ALBEDO_PARAMS.contains(&tx.param_hash.as_str()) {
                    c.albedo
                } else if tx.param_hash == "1e77bf4c" && c.normal != 0 && set.get(c.normal).is_some() {
                    c.normal
                } else {
                    continue;
                };
                tx.hash64 = format!("{new:016x}");
                tx.hash32 = String::new();
            }
            recs.push(serde_json::json!({
                "shader": s.index, "preset": format!("{:08x}", c.preset), "slot_source": c.slot_source, "slot_presets": c.count,
                "albedo": format!("{:016x}", c.albedo), "normal": format!("{:016x}", c.normal), "replaced": format!("{b:016x}"),
                "replaced_was": if set.get(b).is_some() { "a tint mask (linear format)" } else { "in no store" },
                "tint": c.tint, "tint_palette": format!("{:08x}", c.palette),
            }));
            s
        })
        .collect();
    if recs.is_empty() {
        item.warnings.push("part skin: no preset with a painted colour texture for this part".into());
        return None;
    }
    Some((sh, serde_json::json!({"preset": recs[0]["preset"], "shaders": recs})))
}

/// Skin joints for a drawable. Normally the drawable's own skeleton, bone k =
/// joint k. Ped clothing (models with a bone palette that names the
/// drawable's own bones) and ped heads (no skeleton of their own, palette
/// bones in the shared skeleton) index the palette instead; the joint list is
/// then the shared ped skeleton followed by the drawable's own bones. The own
/// `GM_U01_*` helper bones hang under the shared bone their record names
/// (`Bone::parent_hash`), with their parent-relative bind moved to model space.
fn ped_skin(a: &ArchiveInfo, e: &Entry, r: &Resource, info: &crate::drawable::DrawableInfo, o: &ExportOptions, item: &mut ExportedItem) -> SkinSetup {
    use crate::gltf::Joint;
    let joint = |b: &crate::drawable::Bone, off: usize| Joint { name: b.name.clone(), parent: b.parent.map(|p| p as usize + off), bind: b.bind_matrix.unwrap(), pose: None };
    let ped_name = crate::pedskel::is_ped_name(e.name.as_deref().unwrap_or(""));
    // Ped parts can hold several bone arrays (torso, upper-arm and forearm
    // helper sets); their palettes name bones of all of them.
    let all_bones: Vec<crate::drawable::Bone>;
    let skeleton: &[crate::drawable::Bone] = if ped_name {
        let mut v = Vec::new();
        for mut arr in crate::drawable::find_skeletons(r) {
            let off = v.len() as u16;
            arr.iter_mut().for_each(|b| b.parent = b.parent.map(|p| p + off));
            v.extend(arr);
        }
        all_bones = v;
        &all_bones
    } else {
        &info.skeleton
    };
    let own_ok = skeleton.iter().all(|b| b.bind_matrix.is_some());
    let own: Vec<Joint> = if own_ok { skeleton.iter().map(|b| joint(b, 0)).collect() } else { Vec::new() };
    let mut out = SkinSetup { joints: own, geometry_map: Default::default() };
    let mut own_hash: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for (i, b) in skeleton.iter().enumerate() {
        own_hash.entry(crate::meta7a::joaat(&b.name)).or_insert(i);
    }
    // An animal part's shared skeleton is its root fragment's.
    let animal = crate::pedskel::animal(&game_root(a), e.name.as_deref().unwrap_or(""));
    // one-bone (rigid) palettes only on ped and animal parts
    let cand: Vec<(&crate::drawable::Model, &Vec<u32>)> =
        info.models.iter().filter_map(|m| m.bone_palette.as_ref().map(|p| (m, p))).filter(|(_, p)| p.len() > 1 || ped_name || animal.is_some()).collect();
    if !own_ok || cand.is_empty() {
        return out;
    }
    let female = crate::pedskel::is_female_name(e.name.as_deref().unwrap_or(""));
    // A ped drawable: some palette names one of the drawable's own bones
    // (the ascending check alone is not proof), or a ped part's palette
    // names shared ped skeleton bones only (heads, low-detail copies).
    let names_own = cand.iter().any(|(_, p)| p.iter().any(|h| own_hash.contains_key(h)));
    let ped = if animal.is_some() {
        animal
    } else if names_own || ped_name {
        crate::pedskel::get(&game_root(a), female)
    } else {
        None
    };
    let in_ped = |p: &Vec<u32>| ped.is_some_and(|s| !p.is_empty() && p.iter().filter(|h| s.by_hash.contains_key(h)).count() * 10 >= p.len() * 9);
    if !names_own && !cand.iter().any(|(_, p)| in_ped(p)) {
        return out;
    }
    let n_ped = ped.map_or(0, |p| p.bones.len());
    // Each model's palette is used when most of its bones resolve (lower LODs
    // may name only shared-skeleton bones). With the shared skeleton loaded,
    // a palette that names any known bone is used even when some of its
    // bones are in neither skeleton: reading the vertex joints as skeleton
    // indices instead binds whole meshes to unrelated bones (a field jacket
    // with 16 of 118 palette bones unknown came out bound to the legs).
    let known = |h: &u32| own_hash.contains_key(h) || ped.is_some_and(|p| p.by_hash.contains_key(h));
    let pal: Vec<_> = cand
        .into_iter()
        .filter(|(_, p)| {
            let k = p.iter().filter(|h| known(h)).count();
            k * 10 >= p.len() * 9 || (ped.is_some() && k > 0) || (ped.is_none() && p.iter().any(|h| own_hash.contains_key(h)))
        })
        .collect();
    if let Some(p) = ped {
        let mut j: Vec<Joint> = p.bones.iter().map(|b| joint(b, 0)).collect();
        if !o.body_shape.is_empty() && animal.is_none() {
            match crate::bodyshape::get(&game_root(a), female) {
                Ok(rig) => match crate::bodyshape::pose(&p.bones, rig, &crate::bodyshape::input_values(&o.body_shape)) {
                    Some((posed, _)) => j.iter_mut().zip(posed).for_each(|(j, m)| j.pose = Some(m)),
                    None => item.warnings.push("body shape: shared skeleton has bones without a bind pose; neutral body exported".into()),
                },
                Err(err) => item.warnings.push(format!("body shape: {err}; neutral body exported")),
            }
        }
        let ped_joints = j.len();
        j.extend(skeleton.iter().map(|b| joint(b, n_ped)));
        // Helper bones (`GM_U01_*`) are roots of the drawable's skeleton that
        // name their parent in the shared skeleton (`Bone::parent_hash`);
        // their binds are relative to it. Hang them under that bone and put
        // their bind (and body-shape pose) in model space, so they follow
        // the body when it moves.
        let mut unattached = 0;
        for (k, b) in skeleton.iter().enumerate() {
            // the drawable root this bone hangs from, and its shared parent
            let mut root = k;
            while let Some(q) = skeleton[root].parent.map(|q| q as usize).filter(|&q| q < skeleton.len() && q != root) {
                root = q;
            }
            let Some(&pi) = skeleton[root].parent_hash.and_then(|h| p.by_hash.get(&h)) else {
                unattached += 1;
                continue;
            };
            let jt = &mut j[ped_joints + k];
            if b.parent.is_none() {
                jt.parent = Some(pi);
            }
            let (pb, pp) = (j[pi].bind, j[pi].pose);
            let jt = &mut j[ped_joints + k];
            let local = jt.bind;
            jt.bind = crate::gltf::mat_mul(&pb, &local);
            if let Some(pp) = pp {
                jt.pose = Some(crate::gltf::mat_mul(&pp, &local));
            }
        }
        if unattached > 0 {
            item.warnings.push(format!("ped clothing skin: {unattached} of the drawable's own bones name no parent in the shared skeleton; they are left at their stored (parent-relative) bind"));
        }
        out.joints = j;
    } else {
        item.warnings.push(format!("ped clothing skin: shared ped skeleton ({}) not found in the packs index; only the drawable's own bones are bound", crate::pedskel::source_name(female)));
    }
    let resolve = |h: &u32| -> u16 {
        if let Some(&i) = ped.and_then(|p| p.by_hash.get(h)) {
            return i as u16;
        }
        own_hash.get(h).map_or(u16::MAX, |&i| (i + n_ped) as u16)
    };
    let mut missing: Vec<u32> = Vec::new();
    for (m, p) in pal {
        let map: Vec<u16> = p.iter().map(resolve).collect();
        missing.extend(p.iter().zip(&map).filter(|(_, &x)| x == u16::MAX).map(|(h, _)| *h));
        for &g in &m.geometries {
            out.geometry_map.insert(g, map.clone());
        }
    }
    missing.sort();
    missing.dedup();
    if !missing.is_empty() && ped.is_some() {
        let hx: Vec<String> = missing.iter().map(|h| format!("{h:08x}")).collect();
        item.warnings.push(format!("ped clothing skin: {} palette bones are in neither skeleton (joaat {}); their weight goes to other influences", missing.len(), hx.join(" ")));
    }
    out
}

fn export_texture(r: &Resource, t: &texture::TexInfo, dir: &Path, o: &ExportOptions, item: &mut ExportedItem) -> Result<Vec<String>> {
    let stem = if t.name.is_empty() { format!("tex_{:x}", t.struct_off) } else { sanitize(&t.name) };
    let mut files = Vec::new();
    // one-row textures (tint palettes) are stored linear
    match texture::mip0_linear(r, t, t.height == 1) {
        Ok((fmt, lin)) => {
            if o.png {
                match texture::to_rgba(fmt, &lin, t.width, t.height) {
                    Ok(rgba) => {
                        let p = dir.join(format!("{stem}.png"));
                        texture::save_png(&p, &rgba, t.width, t.height)?;
                        files.push(rel(dir, &p));
                        item.files.push(rel(&o.out, &p));
                    }
                    Err(err) => item.warnings.push(format!("{}: png: {err}", t.name)),
                }
            }
            if o.dds {
                // Original BCn data, untiled, with the verified part of the mip chain.
                let p = dir.join(format!("{stem}.dds"));
                let mut mips = vec![lin.clone()];
                for m in 1..texture::exportable_mips(t) {
                    match texture::mip_linear(r, t, m) {
                        Ok((_, d, _, _)) => mips.push(d),
                        Err(_) => break,
                    }
                }
                texture::save_dds_mips(&p, t, fmt, &mips)?;
                files.push(rel(dir, &p));
                item.files.push(rel(&o.out, &p));
            }
        }
        Err(err) => item.warnings.push(format!("{}: {err}", t.name)),
    }
    let p = dir.join(format!("{stem}.json"));
    fs::write(&p, serde_json::to_vec_pretty(t)?)?;
    files.push(rel(dir, &p));
    Ok(files)
}

/// Export many entries in parallel, appending to `<out>/manifest.jsonl`.
pub fn export_many(a: &ArchiveInfo, entries: &[&Entry], o: &ExportOptions, progress: Progress) -> Result<Vec<ExportedItem>> {
    fs::create_dir_all(&o.out)?;
    let manifest = Mutex::new(
        fs::OpenOptions::new().create(true).append(true).open(o.out.join("manifest.jsonl"))?,
    );
    let total = entries.len() as u64;
    let done = AtomicU64::new(0);
    let items: Vec<ExportedItem> = entries
        .par_iter()
        .map(|e| {
            let it = export_entry(a, e, o).unwrap_or_else(|err| ExportedItem {
                id: e.id.clone(),
                archive: a.id.clone(),
                kind: e.kind.name(),
                rtype: e.rtype.clone(),
                name: e.name.clone(),
                status: "error".into(),
                files: vec![],
                error: Some(format!("{err:#}")),
                warnings: vec![],
            });
            if it.status != "skipped" {
                use std::io::Write;
                if let Ok(line) = serde_json::to_string(&it) {
                    let _ = writeln!(manifest.lock().unwrap(), "{line}");
                }
            }
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&e.id, d, total);
            it
        })
        .collect();
    Ok(items)
}

/// Rough output-size estimate: decompressed bytes of the selection (PNG of
/// BC textures is typically 2-4x the BC size; raw buffers are 1x).
pub fn estimate(entries: &[&Entry]) -> (u64, u64) {
    let dec: u64 = entries.iter().map(|e| e.usize).sum();
    let txd: u64 = entries.iter().filter(|e| e.rtype.as_deref() == Some("txd")).map(|e| e.usize).sum();
    (dec, dec + txd * 3)
}

/// joaat(name) → (name, entry id) for every named drawable / fragment /
/// drawable dictionary in all indexed archives (first match wins).
pub fn model_names(ws: &Workspace) -> std::collections::HashMap<u32, (String, String)> {
    model_names_linked(ws)
}

/// Named drawables only (no archetype links).
pub fn model_names_base(ws: &Workspace) -> std::collections::HashMap<u32, (String, String)> {
    let mut m = std::collections::HashMap::new();
    for a in ws.archives() {
        let Ok(ix) = ws.load_index(&a) else { continue };
        for e in &ix.entries {
            register_model_names(&mut m, e);
        }
    }
    m
}

fn register_model_names(m: &mut std::collections::HashMap<u32, (String, String)>, e: &Entry) {
    // Register members first: the entry name is often its first member's name.
    for n in &e.members {
        m.entry(crate::meta7a::joaat(n)).or_insert_with(|| (n.clone(), format!("{}#{n}", e.id)));
    }
    if let (Some(n), Some("dr" | "ft" | "dd")) = (&e.name, e.rtype.as_deref()) {
        m.entry(crate::meta7a::joaat(n)).or_insert_with(|| (n.clone(), e.id.clone()));
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlacementSummary {
    pub id: String,
    pub file: String,
    pub name_hash: String,
    pub parent_hash: String,
    /// Map file (entry id) whose `name_hash` is this file's `parent_hash`:
    /// the file that entities' `lod.parent_index` points into.
    pub parent_id: Option<String>,
    pub entities: usize,
    pub resolved: usize,
    /// Min / max entity position (RAGE world space; interior- or
    /// group-local when `space` says so).
    pub bbox: Option<[[f32; 3]; 2]>,
    /// world, interior (interior-local), world+interior or group (group-local).
    #[serde(default)]
    pub space: String,
}

/// Decode map placement files (0x7a meta resources with entities) from
/// `entries` into `<out>/placements/<archive>/<offset>.json` plus
/// `<out>/placements/<archive>/index.jsonl`. Non-placement resources are skipped.
pub fn export_placements(ws: &Workspace, a: &ArchiveInfo, entries: &[&Entry], out: &Path, progress: Progress) -> Result<Vec<PlacementSummary>> {
    let names = model_names(ws);
    // from `sixng lod-links` (empty when not run)
    let arch_lod = crate::lodlinks::load_archetype_lod(ws);
    let parents = crate::lodlinks::load_parents(ws);
    let dir = out.join("placements").join(a.id.trim_end_matches(".rpf"));
    fs::create_dir_all(&dir)?;
    let total = entries.len() as u64;
    let done = AtomicU64::new(0);
    let mut v: Vec<PlacementSummary> = entries
        .par_iter()
        .filter_map(|e| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&e.id, d, total);
            let r = Resource::load_sys(&a.path, e).ok()?;
            let mf = crate::meta7a::map_file(&r, &names)?;
            if mf.entities.is_empty() || (REGION.get().is_some() && !mf.entities.iter().any(|x| in_region(x.position, x.position))) {
                return None;
            }
            let p = dir.join(format!("{:x}.json", e.off));
            let mut doc = serde_json::to_value(&mf).ok()?;
            doc["id"] = serde_json::json!(e.id);
            let fi = crate::meta7a::Meta::parse(&r).map(|m| crate::interiorents::file_info(&m));
            let space = fi.as_ref().map(|f| f.space).unwrap_or("world");
            doc["space"] = serde_json::json!(space);
            if let Some(f) = fi.as_ref().filter(|f| !f.interiors.is_empty()) {
                doc["interiors"] = serde_json::json!(f.interiors);
            }
            if let Some(ents) = doc["entities"].as_array_mut() {
                if let Some(f) = &fi {
                    for (i, j) in ents.iter_mut().enumerate() {
                        if let Some(v) = f.entities.get(&i) {
                            let mut v = v.clone();
                            if let Some(o) = v.as_object_mut() {
                                o.remove("index");
                            }
                            j["interior"] = v;
                        }
                    }
                }
                let ps = parents.get(&e.id);
                for (i, (j, en)) in ents.iter_mut().zip(&mf.entities).enumerate() {
                    let lod = &mut j["lod"];
                    if let Some(al) = arch_lod.get(&en.archetype) {
                        let own = lod["lod_dist"].as_f64().unwrap_or(-1.0);
                        lod["archetype_lod_dist"] = serde_json::json!(al.lod_dist);
                        lod["hd_texture_dist"] = serde_json::json!(al.hd_texture_dist);
                        lod["effective_lod_dist"] = serde_json::json!(if own > 0.0 { own as f32 } else { al.lod_dist });
                    }
                    if let Some(p) = ps.and_then(|m| m.get(&(i as u64))) {
                        lod["parent"] = p.clone();
                    }
                }
            }
            doc["coordinate_system"] = serde_json::json!(match space {
                "interior" => "position: INTERIOR-LOCAL space (interior archetype file: these entities are an interior's contents, placed in the world only through interior instances, see `sixng interiors`), Z-up, right-handed, metres. rotation and gltf as for world files.",
                "group" => "position: GROUP-LOCAL space (named entity group; no map data places it), Z-up, right-handed, metres. rotation and gltf as for world files.",
                _ => "position: RAGE world space, Z-up, right-handed, metres. rotation: quaternion (x, y, z, w) as stored = inverse of the entity rotation (rotation_world is the entity rotation). gltf: node transform to place this entity's model.glb in a Y-up glTF scene (translation = (x, z, -y)). Checked visually on two map files.",
            });
            fs::write(&p, serde_json::to_vec(&doc).ok()?).ok()?;
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for en in &mf.entities {
                for k in 0..3 {
                    lo[k] = lo[k].min(en.position[k]);
                    hi[k] = hi[k].max(en.position[k]);
                }
            }
            Some(PlacementSummary {
                id: e.id.clone(),
                file: rel(out, &p),
                name_hash: format!("{:08x}", mf.name_hash),
                parent_hash: format!("{:08x}", mf.parent_hash),
                parent_id: None,
                entities: mf.entities.len(),
                resolved: mf.entities.iter().filter(|x| x.name.is_some()).count(),
                bbox: Some([lo, hi]),
                space: space.to_string(),
            })
        })
        .collect();
    v.sort_by(|x, y| x.id.cmp(&y.id));
    let by_name: std::collections::HashMap<String, String> =
        v.iter().filter(|x| x.name_hash != "00000000").map(|x| (x.name_hash.clone(), x.id.clone())).collect();
    for x in v.iter_mut() {
        if x.parent_hash != "00000000" {
            x.parent_id = by_name.get(&x.parent_hash).cloned();
        }
    }
    let mut s = String::new();
    for x in &v {
        s.push_str(&serde_json::to_string(x)?);
        s.push('\n');
    }
    fs::write(dir.join("index.jsonl"), s)?;
    Ok(v)
}

#[derive(Debug, Default, Serialize)]
pub struct BatchSummary {
    pub map_files: usize,
    pub prop_batches: usize,
    pub prop_instances: usize,
    pub grass_batches: usize,
    pub grass_instances: usize,
    pub resolved_batches: usize,
}

/// Instanced batches (props, grass) of every map file, in
/// `<out>/placements/<archive>/`:
/// * `batches.jsonl`: one line per batch (kind, map file id, archetype,
///   name, bbox, LOD distance, `first` / `count` into the instance file);
/// * `batch_props.bin`: 44 bytes per prop instance, little-endian:
///   position f32 x3, world rotation quaternion f32 x4 (x, y, z, w),
///   scale f32 x3, tint u8 x4 (RGBA);
/// * `batch_grass.bin`: 20 bytes per grass instance: position f32 x3,
///   normal x / y u8 (n = b / 255 * 2 - 1, z positive), colour u8 x3,
///   scale u8, ambient occlusion u8, one pad byte.
/// Honours `--bbox` (per batch bbox).
pub fn export_batches(ws: &Workspace, a: &ArchiveInfo, entries: &[&Entry], out: &Path, progress: Progress) -> Result<BatchSummary> {
    use std::io::Write;
    let names = model_names(ws);
    let dir = out.join("placements").join(a.id.trim_end_matches(".rpf"));
    fs::create_dir_all(&dir)?;
    let total = entries.len() as u64;
    let done = AtomicU64::new(0);
    struct W {
        meta: std::io::BufWriter<fs::File>,
        props: std::io::BufWriter<fs::File>,
        grass: std::io::BufWriter<fs::File>,
        np: usize,
        ng: usize,
        s: BatchSummary,
    }
    let w = std::sync::Mutex::new(W {
        meta: std::io::BufWriter::new(fs::File::create(dir.join("batches.jsonl"))?),
        props: std::io::BufWriter::new(fs::File::create(dir.join("batch_props.bin"))?),
        grass: std::io::BufWriter::new(fs::File::create(dir.join("batch_grass.bin"))?),
        np: 0,
        ng: 0,
        s: BatchSummary::default(),
    });
    entries.par_iter().for_each(|e| {
        let d = done.fetch_add(1, Ordering::Relaxed) + 1;
        progress(&e.id, d, total);
        let Ok(r) = Resource::load_sys(&a.path, e) else { return };
        let Some(m) = crate::meta7a::Meta::parse(&r) else { return };
        if m.objects(crate::batches::PROP_BATCH).is_empty() && m.objects(crate::batches::GRASS_BATCH).is_empty() {
            return;
        }
        let Ok(r) = Resource::load_meta(&a.path, e) else { return };
        let Some(m) = crate::meta7a::Meta::parse(&r) else { return };
        let props: Vec<_> = crate::batches::prop_batches(&r, &m).into_iter().filter(|b| in_region(b.bbox[0], b.bbox[1])).collect();
        let grass: Vec<_> = crate::batches::grass_batches(&r, &m).into_iter().filter(|b| in_region(b.bbox[0], b.bbox[1])).collect();
        if props.is_empty() && grass.is_empty() {
            return;
        }
        let mut w = w.lock().unwrap();
        w.s.map_files += 1;
        for b in props {
            let h = names.get(&b.archetype);
            let v = serde_json::json!({"kind": "prop", "map": e.id, "archetype": format!("{:08x}", b.archetype), "name": h.map(|x| &x.0), "model_id": h.map(|x| &x.1),
                "bbox": b.bbox, "lod_distance": b.lod_distance, "first": w.np, "count": b.instances.len()});
            let _ = writeln!(w.meta, "{v}");
            for i in &b.instances {
                let mut buf = Vec::with_capacity(44);
                for f in i.p.iter().chain(i.q.iter()).chain(i.s.iter()) {
                    buf.extend_from_slice(&f.to_le_bytes());
                }
                buf.extend_from_slice(&i.tint);
                let _ = w.props.write_all(&buf);
            }
            w.np += b.instances.len();
            w.s.prop_batches += 1;
            w.s.prop_instances += b.instances.len();
            w.s.resolved_batches += h.is_some() as usize;
        }
        for b in grass {
            let h = names.get(&b.archetype);
            let v = serde_json::json!({"kind": "grass", "map": e.id, "archetype": format!("{:08x}", b.archetype), "name": h.map(|x| &x.0), "model_id": h.map(|x| &x.1),
                "bbox": b.bbox, "scale_range": b.scale_range, "lod_distance": b.lod_distance, "first": w.ng, "count": b.instances.len()});
            let _ = writeln!(w.meta, "{v}");
            for i in &b.instances {
                let mut buf = Vec::with_capacity(20);
                for f in &i.p {
                    buf.extend_from_slice(&f.to_le_bytes());
                }
                let q = |x: f32| (((x + 1.0) * 0.5 * 255.0).round().clamp(0.0, 255.0)) as u8;
                buf.extend_from_slice(&[q(i.n[0]), q(i.n[1]), i.colour[0], i.colour[1], i.colour[2], i.scale, i.ao, 0]);
                let _ = w.grass.write_all(&buf);
            }
            w.ng += b.instances.len();
            w.s.grass_batches += 1;
            w.s.grass_instances += b.instances.len();
            w.s.resolved_batches += h.is_some() as usize;
        }
    });
    let mut w = w.into_inner().unwrap();
    w.meta.flush()?;
    w.props.flush()?;
    w.grass.flush()?;
    Ok(w.s)
}

/// Scan the self-describing meta resources (`0x7a`) of an archive: write
/// `<out>/meta/<archive>/index.jsonl` (one line per resource: id, root
/// struct, struct hashes with counts) and, when `dump` is set, the decoded
/// resource as `<off>.json` (with `instances`, also every struct instance
/// under "instances").
pub fn export_meta(a: &ArchiveInfo, entries: &[&Entry], out: &Path, dump: bool, instances: bool, progress: Progress) -> Result<usize> {
    let dump = dump || instances;
    let dir = out.join("meta").join(a.id.trim_end_matches(".rpf"));
    fs::create_dir_all(&dir)?;
    let total = entries.len() as u64;
    let done = AtomicU64::new(0);
    let mut lines: Vec<(String, String)> = entries
        .par_iter()
        .filter_map(|e| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&e.id, d, total);
            let r = if dump { Resource::load_meta(&a.path, e) } else { Resource::load_sys(&a.path, e) }.ok()?;
            let m = crate::meta7a::Meta::parse(&r)?;
            let root = m.root_block().or_else(|| m.blocks.first().map(|b| (b.0, 0))).map(|b| format!("{:08x}", b.0)).unwrap_or_default();
            let mut structs: Vec<_> = m.structs.keys().map(|h| (format!("{h:08x}"), m.objects(*h).len())).collect();
            structs.sort();
            if dump {
                let mut doc = m.dump();
                doc["id"] = serde_json::json!(e.id);
                if instances {
                    doc["instances"] = m.instances(100_000);
                }
                fs::write(dir.join(format!("{:x}.json", e.off)), serde_json::to_vec(&doc).ok()?).ok()?;
            }
            let line = serde_json::json!({"id": e.id, "name": e.name, "size": e.usize, "root": root, "structs": structs});
            Some((e.id.clone(), line.to_string()))
        })
        .collect();
    lines.sort();
    let n = lines.len();
    fs::write(dir.join("index.jsonl"), lines.into_iter().map(|x| x.1 + "\n").collect::<String>())?;
    Ok(n)
}

/// Shader census over many drawables: per shader-name hash, how often it is
/// used, its parameter list, the texture formats/sizes bound per parameter
/// (from the ttpf stores), the record's flag bytes and example model names.
/// Used to classify materials when shader names are unknown.
pub fn shader_census(a: &ArchiveInfo, entries: &[&Entry], progress: Progress) -> Result<serde_json::Value> {
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Mutex;
    #[derive(Default)]
    struct Agg {
        count: usize,
        models: BTreeMap<String, usize>,
        params: BTreeMap<String, usize>,
        fmts: BTreeMap<String, BTreeMap<String, usize>>,
        sizes: BTreeMap<String, BTreeMap<u32, usize>>,
        rec: BTreeMap<String, usize>,
        cb: BTreeMap<String, usize>,
    }
    let set = crate::ttpf::global(&game_root(a));
    let agg: Mutex<HashMap<String, Agg>> = Mutex::new(HashMap::new());
    let total = entries.len() as u64;
    let done = AtomicU64::new(0);
    entries.par_iter().for_each(|e| {
        let d = done.fetch_add(1, Ordering::Relaxed) + 1;
        progress(&e.id, d, total);
        let Ok(r) = Resource::load_sys(&a.path, e) else { return };
        let shaders = crate::drawable::find_shaders(&r);
        let name = e.name.clone().unwrap_or_else(|| e.id.clone());
        let mut local: Vec<(String, Vec<String>, Vec<(String, String, u32)>, String, String)> = Vec::new();
        for s in &shaders {
            let o = s.record_off as usize;
            let rec = r.sys.get(o + 0x38..o + 0x40).map(|b| b.iter().map(|x| format!("{x:02x}")).collect::<String>()).unwrap_or_default();
            // first 16 floats of the constant buffer (record +8 -> block; data after the self pointer)
            let cb = r
                .sys_off(r.u64(o + 8))
                .map(|c| (0..12).map(|k| format!("{}", (r.f32(c + 16 + 4 * k) * 1000.0).round() / 1000.0)).collect::<Vec<_>>().join(","))
                .unwrap_or_default();
            let tex = s
                .textures
                .iter()
                .map(|t| {
                    let h = u64::from_str_radix(&t.hash64, 16).unwrap_or(0);
                    match set.get(h) {
                        Some((_, tx)) => (t.param_hash.clone(), texture::dxgi_name(tx.dxgi_format).to_string(), tx.width.max(tx.height)),
                        None => (t.param_hash.clone(), "missing".to_string(), 0),
                    }
                })
                .collect();
            local.push((s.name_hash.clone(), s.param_hashes.clone(), tex, rec, cb));
        }
        let mut g = agg.lock().unwrap();
        for (h, params, tex, rec, cb) in local {
            let x = g.entry(h).or_default();
            x.count += 1;
            if x.models.len() < 40 {
                *x.models.entry(name.clone()).or_default() += 1;
            }
            *x.params.entry(params.join(" ")).or_default() += 1;
            for (p, f, sz) in tex {
                *x.fmts.entry(p.clone()).or_default().entry(f).or_default() += 1;
                *x.sizes.entry(p).or_default().entry(sz).or_default() += 1;
            }
            *x.rec.entry(rec).or_default() += 1;
            if x.cb.len() < 30 {
                *x.cb.entry(cb).or_default() += 1;
            }
        }
    });
    let g = agg.into_inner().unwrap();
    let mut out = serde_json::Map::new();
    for (h, x) in g {
        let param_names: std::collections::BTreeMap<String, String> = x.params.keys().flat_map(|k| k.split(' ')).filter_map(|p| Some((p.to_string(), crate::names::param(p)?))).collect();
        out.insert(
            h.clone(),
            serde_json::json!({"name": crate::names::shader(&h), "param_names": param_names, "count": x.count, "models": x.models, "params": x.params, "formats": x.fmts, "sizes": x.sizes, "record_38": x.rec, "cbuffer_head": x.cb}),
        );
    }
    Ok(serde_json::Value::Object(out))
}

/// Decode physics bounds (collision) from unnamed resources: write
/// `<out>/collision/<archive>/<off>.glb` (one primitive per surface id, RAGE
/// world coordinates converted to glTF Y-up) and `index.jsonl` (id, file,
/// bbox, polygon counts, materials, primitives). Returns (decoded, surface
/// id -> triangle count over all files).
pub fn export_collision(a: &ArchiveInfo, entries: &[&Entry], out: &Path, progress: Progress) -> Result<(usize, std::collections::BTreeMap<String, usize>)> {
    use std::sync::Mutex;
    let dir = out.join("collision").join(a.id.trim_end_matches(".rpf"));
    fs::create_dir_all(&dir)?;
    let total = entries.len() as u64;
    let done = AtomicU64::new(0);
    let census: Mutex<std::collections::BTreeMap<String, usize>> = Mutex::new(Default::default());
    let mut lines: Vec<(String, String)> = entries
        .par_iter()
        .filter_map(|e| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&e.id, d, total);
            let mut r = Resource::load_sys(&a.path, e).ok()?;
            let need = crate::bounds::sys_extent(&r);
            if need > r.sys.len() {
                r.extend_sys_tail(&a.path, e, need).ok()?;
            }
            let c = crate::bounds::decode(&r)?;
            if c.by_mat.values().all(|x| x.1.is_empty()) || !in_region(c.bbox[0], c.bbox[1]) {
                return None;
            }
            let mats = c.materials();
            {
                let mut g = census.lock().unwrap();
                for m in &mats {
                    *g.entry(m.id.clone()).or_default() += m.triangles;
                }
            }
            let p = dir.join(format!("{:x}.glb", e.off));
            let extras = serde_json::json!({"source": e.id, "coordinate_system": "RAGE world space converted to glTF Y-up (x, z, -y); no node transform needed"});
            fs::write(&p, c.glb(&format!("collision_{:x}", e.off), extras)).ok()?;
            let line = serde_json::json!({
                "id": e.id, "file": rel(out, &p), "bbox": c.bbox, "stats": c.stats,
                "materials": mats, "primitives": c.primitives,
            });
            Some((e.id.clone(), serde_json::to_string(&line).ok()?))
        })
        .collect();
    lines.sort();
    let n = lines.len();
    let mut s = String::new();
    for (_, l) in lines {
        s.push_str(&l);
        s.push('\n');
    }
    fs::write(dir.join("index.jsonl"), s)?;
    let census = census.into_inner().unwrap();
    fs::write(dir.join("surfaces.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "note": "surface id = u16 at +0 of each 8-byte bound material: 18 + elf(material name) % 32749, the material names being those of common.rpf's material XML (`sixng annotate` adds them as names_by_surface_id). Values: triangles using it.",
        "triangles_by_surface_id": census,
    }))?)?;
    Ok((n, census))
}

/// Decode navmesh tiles: `<out>/navmesh/<archive>/<off>.glb` (one primitive
/// per polygon flag value; `roads_only` keeps road polygons only) and
/// `index.jsonl` (id, file, tile bbox, polygon counts by flag, road count).
pub fn export_navmesh(a: &ArchiveInfo, entries: &[&Entry], out: &Path, roads_only: bool, progress: Progress) -> Result<(usize, usize)> {
    let dir = out.join("navmesh").join(a.id.trim_end_matches(".rpf"));
    fs::create_dir_all(&dir)?;
    let total = entries.len() as u64;
    let done = AtomicU64::new(0);
    let mut lines: Vec<(String, String, usize)> = entries
        .par_iter()
        .filter_map(|e| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&e.id, d, total);
            let r = Resource::load_sys(&a.path, e).ok()?;
            let nm = crate::navmesh::decode(&r)?;
            if !in_region(nm.bbox[0], nm.bbox[1]) {
                return None;
            }
            let roads = nm.polys.iter().filter(|p| p.0 & crate::navmesh::ROAD != 0).count();
            let p = dir.join(format!("{:x}.glb", e.off));
            let extras = serde_json::json!({"source": e.id, "roads_only": roads_only, "coordinate_system": "RAGE world space converted to glTF Y-up (x, z, -y)"});
            let file = match nm.glb(&format!("navmesh_{:x}", e.off), roads_only, extras) {
                Some(g) => {
                    fs::write(&p, g).ok()?;
                    Some(rel(out, &p))
                }
                None => None,
            };
            let line = serde_json::json!({"id": e.id, "file": file, "bbox": nm.bbox, "area_id": nm.area_id,
                "vertices": nm.vertices.len(), "polygons": nm.polys.len(), "road_polygons": roads, "by_flags": nm.flag_counts()});
            Some((e.id.clone(), serde_json::to_string(&line).ok()?, roads))
        })
        .collect();
    lines.sort();
    let roads = lines.iter().filter(|x| x.2 > 0).count();
    let mut s = String::new();
    for (_, l, _) in &lines {
        s.push_str(l);
        s.push('\n');
    }
    fs::write(dir.join("index.jsonl"), s)?;
    Ok((lines.len(), roads))
}

/// Per-file scan shared by `sixng lights` and `sixng interiors`.
pub struct MetaScan {
    pub id: String,
    pub sets: Vec<crate::lights::LightSet>,
    /// (archetype, position, rotation_world, scale_xy, scale_z) per entity.
    pub ents: Vec<(u32, [f32; 3], [f32; 4], f32, f32)>,
    pub mlos: Vec<crate::meta7a::Mlo>,
    pub instances: Vec<crate::meta7a::MloInstance>,
    /// Union of all MLO entity indices: entities in interior space.
    pub interior: std::collections::HashSet<usize>,
    /// The file is a named entity group (`35abc248`, prefab-like): its
    /// entities are in group-local space and nothing in the map data
    /// places the group.
    pub group: bool,
}

pub fn scan_meta(a: &ArchiveInfo, entries: &[&Entry], lights: bool, progress: Progress) -> Vec<MetaScan> {
    let total = entries.len() as u64;
    let done = AtomicU64::new(0);
    let mut v: Vec<MetaScan> = entries
        .par_iter()
        .filter_map(|e| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&e.id, d, total);
            let r = Resource::load_sys(&a.path, e).ok()?;
            let m = crate::meta7a::Meta::parse(&r)?;
            let sets = if lights { crate::lights::light_sets(&r) } else { vec![] };
            let ents: Vec<_> = m
                .objects(crate::meta7a::ENTITY)
                .into_iter()
                .map(|o| {
                    let q = [r.f32(o + 0x10), r.f32(o + 0x14), r.f32(o + 0x18), r.f32(o + 0x1C)];
                    (r.u32(o + 0x44), [r.f32(o + 0x38), r.f32(o + 0x3C), r.f32(o + 0x40)], [-q[0], -q[1], -q[2], q[3]], r.f32(o + 0x54), r.f32(o + 0x58))
                })
                .collect();
            let mlos = crate::meta7a::mlos(&m);
            let instances = crate::meta7a::mlo_instances(&m);
            if sets.is_empty() && ents.is_empty() && mlos.is_empty() && instances.is_empty() {
                return None;
            }
            let interior = mlos.iter().flat_map(|x| x.entities.iter().chain(&x.set_entities).copied()).collect();
            let group = !m.objects(0x35ab_c248).is_empty();
            Some(MetaScan { id: e.id.clone(), sets, ents, mlos, instances, interior, group })
        })
        .collect();
    v.sort_by(|x, y| x.id.cmp(&y.id));
    v
}

/// Interior placements (`sixng interiors`): every interior instance with
/// its world transform, the interior archetype file and the indices of its
/// entities in that file's placement list. Writes
/// `<out>/placements/<archive>/interiors.jsonl`. Returns (instances,
/// resolved, interior archetypes).
pub fn export_interiors(a: &ArchiveInfo, entries: &[&Entry], all: &[Entry], out: &Path, progress: Progress) -> Result<(usize, usize, usize)> {
    use std::io::Write;
    let scans = scan_meta(a, entries, false, progress);
    let mut by_name = std::collections::HashMap::new();
    for s in &scans {
        for m in &s.mlos {
            by_name.entry(m.name).or_insert((s.id.as_str(), m));
        }
    }
    // Interior collision: bounds files (from `sixng collision`) whose bbox
    // matches the interior archetype bbox (bounds are unnamed, so the bbox
    // is the only link found).
    let mut coll: Vec<(String, [[f32; 3]; 2])> = Vec::new();
    if let Ok(t) = fs::read_to_string(out.join("collision").join(a.id.trim_end_matches(".rpf")).join("index.jsonl")) {
        for l in t.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let (Some(id), Ok(bb)) = (v["id"].as_str(), serde_json::from_value::<[[f32; 3]; 2]>(v["bbox"].clone())) {
                    coll.push((id.to_string(), bb));
                }
            }
        }
    }
    let close = |a: &[[f32; 3]; 2], b: &[[f32; 3]; 2]| (0..2).all(|i| (0..3).all(|k| (a[i][k] - b[i][k]).abs() < 0.25));
    let linked = link_interior_collision(&by_name, &coll, close);
    let scan_by_id: std::collections::HashMap<&str, &MetaScan> = scans.iter().map(|s| (s.id.as_str(), s)).collect();
    let second = interior_second_pass(a, all, &by_name, &scan_by_id, &coll, &linked);
    let dir = out.join("placements").join(a.id.trim_end_matches(".rpf"));
    fs::create_dir_all(&dir)?;
    let mut w = std::io::BufWriter::new(fs::File::create(dir.join("interiors.jsonl"))?);
    let (mut n, mut ok) = (0, 0);
    for s in &scans {
        for (i, inst) in s.instances.iter().enumerate() {
            n += 1;
            let mut v = serde_json::to_value(inst)?;
            v["archetype"] = serde_json::json!(format!("{:08x}", inst.archetype));
            v["map"] = serde_json::json!(s.id);
            v["instance"] = serde_json::json!(i);
            if let Some((fid, m)) = by_name.get(&inst.archetype) {
                ok += 1;
                v["interior_file"] = serde_json::json!(fid);
                v["entities"] = serde_json::json!(m.entities);
                v["entity_set_entities"] = serde_json::json!(m.set_entities);
                v["bbox"] = serde_json::json!(m.bbox);
                let c: Vec<&str> = coll.iter().filter(|(_, bb)| close(bb, &m.bbox)).map(|(id, _)| id.as_str()).collect();
                let vol = |b: &[[f32; 3]; 2]| (0..3).map(|k| (b[1][k] - b[0][k]).max(1e-3)).product::<f32>();
                let loose: Vec<&str> = coll
                    .iter()
                    .filter(|(_, bb)| (0..3).all(|k| bb[0][k] >= m.bbox[0][k] - 1.0 && bb[1][k] <= m.bbox[1][k] + 1.0) && vol(bb) > 0.5 * vol(&m.bbox) && !close(bb, &m.bbox))
                    .map(|(id, _)| id.as_str())
                    .collect();
                v["collision"] = serde_json::json!(c);
                v["collision_candidates"] = serde_json::json!(loose);
                if let Some((id, method, score)) = linked.get(&inst.archetype) {
                    let conf = if *method == "exact" { "high" } else { "medium" };
                    v["collision_linked"] = serde_json::json!({"id": id, "method": method, "score": score, "confidence": conf});
                } else if let Some(x) = second.get(&inst.archetype) {
                    v["collision_linked"] = x.clone();
                }
            }
            writeln!(w, "{}", serde_json::to_string(&v)?)?;
        }
    }
    w.flush()?;
    Ok((n, ok, by_name.len()))
}

/// Second-pass interior collision links for archetypes the first pass
/// left open. Interiors with >= 3 entities: floor fit of
/// up to 150 entity positions (evenly spaced) against the 8 interior-space
/// bounds files with the best bbox IoU (>= 0.2); score = fit + 0.3 IoU.
/// Others: best bbox IoU (>= 0.3). Confidence: high (fit, margin >= 0.2),
/// medium (fit margin >= 0.05, or IoU >= 0.7 with margin > 0.1), low.
fn interior_second_pass(
    a: &ArchiveInfo,
    all: &[Entry],
    by_name: &std::collections::HashMap<u32, (&str, &crate::meta7a::Mlo)>,
    scans: &std::collections::HashMap<&str, &MetaScan>,
    coll: &[(String, [[f32; 3]; 2])],
    linked: &std::collections::HashMap<u32, (String, &'static str, f32)>,
) -> std::collections::HashMap<u32, serde_json::Value> {
    use crate::surface::{floor_fit, iou, is_local_xyz, spread, triangles};
    let by_id: std::collections::HashMap<&str, &Entry> = all.iter().map(|e| (e.id.as_str(), e)).collect();
    let local: Vec<usize> = (0..coll.len()).filter(|&i| is_local_xyz(&coll[i].1)).collect();
    let r3 = |x: f64| (x * 1000.0).round() / 1000.0;
    let todo: Vec<(&u32, &(&str, &crate::meta7a::Mlo))> = by_name.iter().filter(|(k, _)| !linked.contains_key(*k)).collect();
    todo.par_iter()
        .filter_map(|(name, (fid, m))| {
            let mut ious: Vec<(f64, usize)> = local.iter().map(|&i| (iou(&coll[i].1, &m.bbox), i)).collect();
            ious.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap().then(x.1.cmp(&y.1)));
            let ents = scans.get(fid).map(|s| &s.ents);
            let pts: Vec<[f32; 3]> = ents.map(|e| m.entities.iter().filter_map(|&i| e.get(i).map(|x| x.1)).collect()).unwrap_or_default();
            if pts.len() >= 3 {
                let pts = spread(&pts, 150);
                let mut r: Vec<(f64, f64, f64, usize)> = Vec::new();
                for &(io, i) in ious.iter().take(8) {
                    if io < 0.2 {
                        break;
                    }
                    let tris = by_id.get(coll[i].0.as_str()).and_then(|e| crate::surface::decode_bounds(&a.path, e)).map(|c| triangles(&c, true)).unwrap_or_default();
                    let f = floor_fit(&pts, &tris);
                    r.push((f + 0.3 * io, f, io, i));
                }
                r.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap().then(y.1.partial_cmp(&x.1).unwrap()));
                if r.is_empty() || r[0].1 == 0.0 {
                    return None;
                }
                let m_ = r[0].0 - r.get(1).map_or(0.0, |x| x.0);
                let conf = if m_ >= 0.2 { "high" } else if m_ >= 0.05 { "medium" } else { "low" };
                Some((**name, serde_json::json!({"id": coll[r[0].3].0, "method": "floor_fit", "score": r3(r[0].0), "fit": r3(r[0].1), "iou": r3(r[0].2), "margin": r3(m_), "confidence": conf})))
            } else {
                let (b0, i0) = *ious.first()?;
                let b1 = ious.get(1).map_or(0.0, |x| x.0);
                if b0 < 0.3 {
                    return None;
                }
                let m_ = b0 - b1;
                let conf = if b0 >= 0.7 && m_ > 0.1 { "medium" } else { "low" };
                Some((**name, serde_json::json!({"id": coll[i0].0, "method": "bbox", "score": r3(b0), "iou": r3(b0), "margin": r3(m_), "confidence": conf})))
            }
        })
        .collect()
}

fn entry_off(id: &str) -> u64 {
    id.split('#').next().unwrap_or(id).rsplit('@').next().and_then(|x| u64::from_str_radix(x, 16).ok()).unwrap_or(0)
}

/// One collision file per interior archetype. Bounds are unnamed, but the
/// archive stores interiors and their bounds in the same order (94 % of the
/// unique exact-bbox pairs are monotonic). Anchors = unique exact bbox
/// matches on the longest monotonic chain; any other interior takes the
/// bounds file between its neighbouring anchors' bounds that lies inside its
/// bbox (±2 m) with the best bbox IoU (> 0.3). Hold-out on the anchors: 91 %
/// of predictions are right. Returns archetype → (bounds id, method, score).
fn link_interior_collision(
    by_name: &std::collections::HashMap<u32, (&str, &crate::meta7a::Mlo)>,
    coll: &[(String, [[f32; 3]; 2])],
    close: impl Fn(&[[f32; 3]; 2], &[[f32; 3]; 2]) -> bool,
) -> std::collections::HashMap<u32, (String, &'static str, f32)> {
    let vol = |b: &[[f32; 3]; 2]| (0..3).map(|k| (b[1][k] - b[0][k]).max(0.0)).product::<f32>();
    let iou = |a: &[[f32; 3]; 2], b: &[[f32; 3]; 2]| {
        let i = [std::array::from_fn::<f32, 3, _>(|k| a[0][k].max(b[0][k])), std::array::from_fn(|k| a[1][k].min(b[1][k]))];
        let x = vol(&i);
        let u = vol(a) + vol(b) - x;
        if u > 0.0 { x / u } else { 0.0 }
    };
    let mut bounds: Vec<(u64, usize)> = coll.iter().enumerate().map(|(i, c)| (entry_off(&c.0), i)).collect();
    bounds.sort();
    let mut out = std::collections::HashMap::new();
    // Anchors: (interior file offset, bounds offset, archetype).
    let mut pairs: Vec<(u64, u64, u32, usize)> = Vec::new();
    for (&name, (fid, m)) in by_name {
        let ex: Vec<usize> = (0..coll.len()).filter(|&i| close(&coll[i].1, &m.bbox)).collect();
        if ex.len() == 1 {
            pairs.push((entry_off(fid), entry_off(&coll[ex[0]].0), name, ex[0]));
        }
    }
    pairs.sort();
    // Longest increasing subsequence on the bounds offset.
    let (mut tails, mut tail_idx, mut prev) = (Vec::<u64>::new(), Vec::<usize>::new(), vec![usize::MAX; pairs.len()]);
    for (j, p) in pairs.iter().enumerate() {
        let k = tails.partition_point(|&t| t < p.1);
        if k == tails.len() {
            tails.push(p.1);
            tail_idx.push(j);
        } else {
            tails[k] = p.1;
            tail_idx[k] = j;
        }
        prev[j] = if k > 0 { tail_idx[k - 1] } else { usize::MAX };
    }
    let mut chain = Vec::new();
    let mut j = tail_idx.last().copied().unwrap_or(usize::MAX);
    while j != usize::MAX {
        chain.push(pairs[j]);
        j = prev[j];
    }
    chain.reverse();
    for p in &chain {
        out.insert(p.2, (coll[p.3].0.clone(), "exact", 1.0));
    }
    for (&name, (fid, m)) in by_name {
        if out.contains_key(&name) {
            continue;
        }
        let io = entry_off(fid);
        let k = chain.partition_point(|p| p.0 < io);
        let lo = if k > 0 { chain[k - 1].1 } else { 0 };
        let hi = if k < chain.len() { chain[k].1 + u64::from(chain[k].0 == io) } else { u64::MAX };
        let s = bounds.partition_point(|b| b.0 < lo);
        let e = bounds.partition_point(|b| b.0 <= hi);
        let best = bounds[s..e]
            .iter()
            .map(|&(_, i)| (i, &coll[i].1))
            .filter(|(_, bb)| (0..3).all(|k| bb[0][k] >= m.bbox[0][k] - 2.0 && bb[1][k] <= m.bbox[1][k] + 2.0))
            .map(|(i, bb)| (i, iou(bb, &m.bbox)))
            .filter(|x| x.1 > 0.3)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, sc)) = best {
            out.insert(name, (coll[i].0.clone(), "ordered", (sc * 100.0).round() / 100.0));
        }
    }
    out
}

/// Light emitters (`sixng lights`): every light extension on archetypes and
/// entities in the meta resources, then every placed light in world space.
/// Writes `<out>/lights/<archive>/sets.jsonl` (one line per light set, with
/// local positions and all fields) and `world.jsonl` (one line per placed
/// light: world position, archetype, map file, entity index, light index).
/// Returns (light sets, lights in sets, placed lights).
pub fn export_lights(ws: &Workspace, a: &ArchiveInfo, entries: &[&Entry], out: &Path, progress: Progress) -> Result<(usize, usize, usize)> {
    use crate::lights::{rotate, LightSet};
    use std::io::Write;
    let names = model_names(ws);
    let dir = out.join("lights").join(a.id.trim_end_matches(".rpf"));
    fs::create_dir_all(&dir)?;
    let scans = scan_meta(a, entries, true, progress);
    type Ent = (u32, [f32; 3], [f32; 4], f32, f32);
    let mut by_arch: std::collections::HashMap<u32, &LightSet> = std::collections::HashMap::new();
    let mut by_mlo = std::collections::HashMap::new();
    let mut sets_out = std::io::BufWriter::new(fs::File::create(dir.join("sets.jsonl"))?);
    let (mut n_sets, mut n_lights) = (0, 0);
    for sc in &scans {
        for m in &sc.mlos {
            by_mlo.entry(m.name).or_insert((sc, m));
        }
        for s in &sc.sets {
            n_sets += 1;
            n_lights += s.lights.len();
            let mut v = serde_json::to_value(s)?;
            v["id"] = serde_json::json!(sc.id);
            v["archetype"] = serde_json::json!(format!("{:08x}", s.archetype));
            v["ext_name"] = serde_json::json!(format!("{:08x}", s.ext_name));
            if let Some(n) = names.get(&s.archetype) {
                v["name"] = serde_json::json!(n.0);
                v["model_id"] = serde_json::json!(n.1);
            }
            writeln!(sets_out, "{}", serde_json::to_string(&v)?)?;
            if s.owner == "archetype" {
                by_arch.entry(s.archetype).or_insert(s);
            }
        }
    }
    sets_out.flush()?;
    let mut w = std::io::BufWriter::new(fs::File::create(dir.join("world.jsonl"))?);
    let mut placed = 0;
    // Entity-space point -> world, optionally through an interior instance.
    let place = |en: &Ent, inst: Option<&crate::meta7a::MloInstance>, l: [f32; 3]| -> [f32; 3] {
        let (_, p, q, sxy, sz) = *en;
        let v = rotate(q, [l[0] * sxy, l[1] * sxy, l[2] * sz]);
        let e = [p[0] + v[0], p[1] + v[1], p[2] + v[2]];
        match inst {
            None => e,
            Some(i) => {
                let v = rotate(i.rotation_world, e);
                [i.position[0] + v[0], i.position[1] + v[1], i.position[2] + v[2]]
            }
        }
    };
    // Combined rotation entity (then interior instance), for directions.
    let q_of = |en: &Ent, inst: Option<&crate::meta7a::MloInstance>| -> [f32; 4] {
        let a = en.2;
        match inst {
            None => a,
            Some(i) => {
                let b = i.rotation_world;
                // b * a
                [
                    b[3] * a[0] + b[0] * a[3] + b[1] * a[2] - b[2] * a[1],
                    b[3] * a[1] - b[0] * a[2] + b[1] * a[3] + b[2] * a[0],
                    b[3] * a[2] + b[0] * a[1] - b[1] * a[0] + b[2] * a[3],
                    b[3] * a[3] - b[0] * a[0] - b[1] * a[1] - b[2] * a[2],
                ]
            }
        }
    };
    let mut emit = |w: &mut std::io::BufWriter<fs::File>, map: &str, ei: usize, en: &Ent, s: &LightSet, inst: Option<(&str, usize, &crate::meta7a::MloInstance)>, group: bool| -> Result<()> {
        for (li, l) in s.lights.iter().enumerate() {
            let wp = place(en, inst.map(|x| x.2), l.position);
            if !in_region(wp, wp) {
                continue;
            }
            let dir = rotate(q_of(en, inst.map(|x| x.2)), l.direction);
            let mut v = serde_json::json!({"p": wp, "dir": dir, "archetype": format!("{:08x}", en.0), "light": li, "colour": l.colour, "type": l.light_type,
                "intensity": l.intensity, "range": l.range, "cone": [l.cone_inner, l.cone_outer], "night_only": l.night_only, "map": map, "entity": ei, "owner": s.owner});
            if let Some((im, ii, _)) = inst {
                v["interior_map"] = serde_json::json!(im);
                v["interior_instance"] = serde_json::json!(ii);
            }
            v["space"] = serde_json::json!(if group { "group_local" } else { "world" });
            writeln!(w, "{v}")?;
            placed += 1;
        }
        Ok(())
    };
    for sc in &scans {
        // Exterior entities.
        for (ei, en) in sc.ents.iter().enumerate().filter(|(i, _)| !sc.interior.contains(i)) {
            if let Some(s) = by_arch.get(&en.0) {
                emit(&mut w, &sc.id, ei, en, s, None, sc.group)?;
            }
        }
        for s in sc.sets.iter().filter(|s| s.owner == "entity" && !s.entity_index.is_some_and(|i| sc.interior.contains(&i))) {
            if let Some(en) = s.entity_index.and_then(|i| sc.ents.get(i)) {
                emit(&mut w, &sc.id, s.entity_index.unwrap(), en, s, None, sc.group)?;
            }
        }
        // Interior instances: the interior's entities through the instance transform.
        for (ii, inst) in sc.instances.iter().enumerate() {
            let Some((isc, mlo)) = by_mlo.get(&inst.archetype) else { continue };
            for &ei in mlo.entities.iter().chain(&mlo.set_entities) {
                let Some(en) = isc.ents.get(ei) else { continue };
                if let Some(s) = by_arch.get(&en.0) {
                    emit(&mut w, &isc.id, ei, en, s, Some((&sc.id, ii, inst)), false)?;
                }
                for s in isc.sets.iter().filter(|s| s.owner == "entity" && s.entity_index == Some(ei)) {
                    emit(&mut w, &isc.id, ei, en, s, Some((&sc.id, ii, inst)), false)?;
                }
            }
        }
    }
    w.flush()?;
    Ok((n_sets, n_lights, placed))
}

/// Road / path-node graph (`sixng paths`): finds the node (`0c69a100`) and
/// link (`0f2da4c2`) meta resources and writes `<out>/paths/<archive>/graph.json`
/// (nodes with position, speed limit, street hash; links with length and
/// lane counts). Returns (nodes, links, unresolved links).
pub fn export_paths(a: &ArchiveInfo, entries: &[&Entry], out: &Path, progress: Progress) -> Result<(usize, usize, usize)> {
    let total = entries.len() as u64;
    let done = AtomicU64::new(0);
    let jfile: std::sync::Mutex<Vec<&Entry>> = Default::default();
    let found: Vec<(String, Vec<crate::paths::Node>, Vec<Vec<u8>>)> = entries
        .par_iter()
        .filter_map(|e| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&e.id, d, total);
            let r = Resource::load_sys(&a.path, e).ok()?;
            let m = crate::meta7a::Meta::parse(&r)?;
            let has_n = m.structs.contains_key(&crate::paths::NODE);
            let has_l = m.structs.contains_key(&crate::paths::LINK) && !m.objects(crate::paths::LINK).is_empty();
            if !m.objects(crate::paths::JUNCTION).is_empty() {
                if let Ok(mut x) = jfile.lock() {
                    x.push(*e);
                }
            }
            if !has_n && !has_l {
                return None;
            }
            Some((e.id.clone(), if has_n { crate::paths::nodes(&r) } else { vec![] }, if has_l { crate::paths::link_records(&r) } else { vec![] }))
        })
        .collect();
    let (nf, nodes) = found.iter().filter(|x| !x.1.is_empty()).max_by_key(|x| x.1.len()).map(|x| (x.0.clone(), x.1.clone())).ok_or_else(|| anyhow!("no path-node resource found"))?;
    let (lf, recs) = found.iter().filter(|x| !x.2.is_empty()).max_by_key(|x| x.2.len()).map(|x| (x.0.clone(), x.2.clone())).ok_or_else(|| anyhow!("no path-link resource found"))?;
    let (links, bad) = crate::paths::links(&nodes, &recs);
    let jf = jfile.into_inner().unwrap_or_default();
    let (jid, junctions) = match jf.iter().max_by_key(|x| x.usize) {
        Some(e) => (
            Some(e.id.clone()),
            // Its system page spans both streams (the phase light records sit
            // in the second one), so read the whole entry as sys.
            crate::scan::read_entry(&a.path, e).map(|sys| crate::paths::junctions(&Resource { sys, gfx: vec![], gfx_base: 0 }, &nodes)).unwrap_or_default(),
        ),
        None => (None, vec![]),
    };
    let dir = out.join("paths").join(a.id.trim_end_matches(".rpf"));
    fs::create_dir_all(&dir)?;
    let doc = serde_json::json!({
        "node_file": nf, "link_file": lf,
        "coordinate_system": "RAGE world space, Z-up, metres",
        "note": "GTA V nodes*.ynd equivalent. speed: m/s from a half float (10/30/40/50 mph values). lanes_to/lanes_from per link direction; almost every link has its reverse. street: joaat of a lower-case location key (`sixng annotate` adds street_key, street_label and street_text). Nodes at z = 0.3 are boat paths.",
        "junction_file": jid,
        "junction_note": "junctions: node = junction node index; entrances with their node and turn paths (start/end points, length, target node); phases = traffic-light phases (duration s, lights as u64 hash + boxId). A junction with phases is signalised.",
        "flags_note": "Node flag_names come from the PNF_ enum value hashes (34 of 51 recovered); unnamed bits are bitN. Link flag_names: GpsCanGoBothWays, ShortCut, LeadsToDeadEnd, LeadsFromDeadEnd are the game's PNLF_ names; [unpaved/track] (bit 5) and [two-way road] (bit 10) are inferred from geometry; [no ambient traffic?] (bit 6) is inferred from node traffic density (set on every link touching a density-0 node).",
        "nodes": nodes, "links": links, "junctions": junctions,
    });
    fs::write(dir.join("graph.json"), serde_json::to_vec(&doc)?)?;
    Ok((nodes.len(), links.len(), bad))
}

/// Archetype definition (GTA V `CBaseArchetypeDef` equivalent, struct
/// `82d6fc83`, 184 B): +0x40 bbox min, +0x4C bbox max, +0x74 name,
/// +0x78 texture dictionary, +0x7C drawable dictionary, +0x80 physics
/// dictionary, +0x88 asset name (all joaat).
pub const ARCHETYPE_DEF: u32 = 0x82d6_fc83;

fn links_path(ws: &Workspace) -> PathBuf {
    ws.index_dir().join("archetype_links.json")
}

/// Named drawables plus the archetype links from `sixng link-archetypes`
/// (cached in `<work>/index/archetype_links.json`) for archetypes whose
/// drawable has no recovered name.
pub fn model_names_linked(ws: &Workspace) -> std::collections::HashMap<u32, (String, String)> {
    let mut m = model_names_base(ws);
    if let Some(v) = fs::read(links_path(ws)).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) {
        if let Some(o) = v["links"].as_object() {
            for (k, x) in o {
                let (Ok(h), Some(id)) = (u32::from_str_radix(k, 16), x["model_id"].as_str()) else { continue };
                let name = x["name"].as_str().map(|s| s.to_string()).unwrap_or_else(|| format!("arch_{k}"));
                m.entry(h).or_insert((name, id.to_string()));
            }
        }
    }
    m
}

/// Link archetypes that have no named drawable to unnamed drawables by
/// their bounding box (archetype bbox = drawable bounds), and guess their
/// names from sibling names (joaat of name stems + common suffixes).
/// Writes `<work>/index/archetype_links.json`.
pub fn link_archetypes(ws: &Workspace, a: &ArchiveInfo, metas: &[&Entry], candidates: &[&Entry], progress: Progress) -> Result<serde_json::Value> {
    use std::collections::{HashMap, HashSet};
    let names = model_names_base(ws);
    // 1. archetype definitions
    let total = (metas.len() + candidates.len()) as u64;
    let done = AtomicU64::new(0);
    type Def = (u32, [[f32; 3]; 2], u32, String);
    let defs: Vec<Def> = metas
        .par_iter()
        .flat_map_iter(|e| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if d % 256 == 0 {
                progress(&e.id, d, total);
            }
            let mut out = Vec::new();
            if let Ok(r) = Resource::load_sys(&a.path, e) {
                if let Some(m) = crate::meta7a::Meta::parse(&r) {
                    for o in m.objects(ARCHETYPE_DEF) {
                        let v3 = |x: usize| [r.f32(x), r.f32(x + 4), r.f32(x + 8)];
                        out.push((r.u32(o + 0x74), [v3(o + 0x40), v3(o + 0x4C)], r.u32(o + 0x7C), e.id.clone()));
                    }
                }
            }
            out
        })
        .collect();
    let mut unresolved: HashMap<u32, &Def> = HashMap::new();
    for d in &defs {
        if !names.contains_key(&d.0) {
            unresolved.entry(d.0).or_insert(d);
        }
    }
    let close = |a: &[[f32; 3]; 2], b: &[[f32; 3]; 2]| (0..2).all(|i| (0..3).all(|k| (a[i][k] - b[i][k]).abs() < 0.03));
    let def_box: HashMap<u32, [[f32; 3]; 2]> = defs.iter().map(|d| (d.0, d.1)).collect();
    // 2. candidate drawables (unnamed, or named but not matching their own archetype) and their bounds
    // Each candidate also yields the drawable name stored inside the resource
    // (its name block), which the carving index can miss.
    type Cand = (String, Option<[[f32; 3]; 2]>, Option<String>);
    let cands: Vec<Cand> = candidates
        .par_iter()
        .filter_map(|e| {
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if d % 256 == 0 {
                progress(&e.id, d, total);
            }
            let r = Resource::load_sys(&a.path, e).ok()?;
            if !e.members.is_empty() || !crate::drawable::dictionary_members(&r).is_empty() || r.sys.len() < 0x4c || crate::drawable::find_shaders(&r).is_empty() {
                return None;
            }
            let emb = if e.name.is_none() { crate::resource::find_resource_name(&r.sys).map(|t| t.0) } else { None };
            // Fragments keep the bbox on their drawable (root +0x18).
            let b = crate::drawable::drawable_bounds(&r).filter(|b| b.iter().flatten().all(|v| v.is_finite() && v.abs() < 1e5) && (0..3).all(|k| b[1][k] >= b[0][k]));
            // A named drawable is a candidate only when its own archetype's bbox differs (name recovered wrongly).
            let b = match (&e.name, b) {
                (Some(n), Some(b)) if def_box.get(&crate::meta7a::joaat(n)).map_or(true, |db| close(db, &b)) => None,
                (_, b) => b,
            };
            (b.is_some() || emb.is_some()).then(|| (e.id.clone(), b, emb))
        })
        .collect();
    let draws: Vec<(String, [[f32; 3]; 2])> = cands.iter().filter_map(|c| c.1.map(|b| (c.0.clone(), b))).collect();
    // embedded name hash -> drawable ids carrying it
    let mut embedded: HashMap<u32, (String, Vec<String>)> = HashMap::new();
    for c in &cands {
        if let Some(n) = &c.2 {
            let e = embedded.entry(crate::meta7a::joaat(n)).or_insert_with(|| (n.clone(), Vec::new()));
            e.1.push(c.0.clone());
        }
    }
    // 3. bbox match (tolerance 0.03 m on all six values), bucketed by min x
    let mut buckets: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, (_, b)) in draws.iter().enumerate() {
        buckets.entry((b[0][0] * 4.0).floor() as i64).or_default().push(i);
    }
    // 4. name guesses from sibling stems
    let want: HashSet<u32> = unresolved.keys().copied().collect();
    let mut guessed: HashMap<u32, String> = HashMap::new();
    let mut suffixes: Vec<String> = vec![String::new()];
    for c in "abcdefghijkl".chars() {
        suffixes.push(format!("_{c}"));
        suffixes.push(c.to_string()); // utilitypole_01 + d
    }
    for i in 0..=40 {
        suffixes.push(format!("_{i:02}"));
        suffixes.push(format!("_{i}"));
        suffixes.push(format!("{i:02}")); // railing + 17
        suffixes.push(format!("{i}"));
        for c in "abcd".chars() {
            suffixes.push(format!("_{i:02}{c}")); // stem + _01a
        }
    }
    for s in ["_hd", "_lod", "_slod", "_dtl", "_shell", "_main", "_base", "_ground", "_grnd", "_terrain", "_detail", "_decal", "_dec", "_struct", "_structure", "_bld", "_building"] {
        suffixes.push(s.to_string());
    }
    let mut stems: HashSet<String> = HashSet::new();
    for (n, _) in names.values() {
        let mut s = n.as_str();
        for _ in 0..4 {
            stems.insert(s.to_string());
            match s.rfind('_') {
                Some(p) if p > 0 => s = &s[..p],
                _ => break,
            }
        }
    }
    for st in &stems {
        for sf in &suffixes {
            let cand = format!("{st}{sf}");
            let h = crate::meta7a::joaat(&cand);
            if want.contains(&h) {
                guessed.entry(h).or_insert(cand);
            }
        }
    }
    // Locality: the archive is stored in name order, so an unnamed drawable
    // sits between the named entries around its (guessed) name, and near the
    // drawables of the other archetypes defined in the same meta file.
    let off_of = |id: &str| entry_off(id);
    let mut by_name: Vec<(String, u64)> = names.values().map(|(n, id)| (n.to_ascii_lowercase(), off_of(id))).collect();
    by_name.sort();
    let mut meta_span: HashMap<&str, (u64, u64)> = HashMap::new();
    for d in &defs {
        if let Some((_, id)) = names.get(&d.0) {
            let o = off_of(id);
            let e = meta_span.entry(d.3.as_str()).or_insert((o, o));
            e.0 = e.0.min(o);
            e.1 = e.1.max(o);
        }
    }
    // unlinkable-by-bbox drawables in archive order, for the name-order gap rule
    let mut draw_by_off: Vec<(u64, usize)> = draws.iter().enumerate().map(|(i, d)| (off_of(&d.0), i)).collect();
    draw_by_off.sort();
    let named_offs: Vec<u64> = { let mut v: Vec<u64> = by_name.iter().map(|x| x.1).collect(); v.sort(); v };
    // Name-order gap: levels.rpf stores entries in name order, so the drawable of
    // an archetype whose name is known sits between the named entries just
    // before and after that name. When exactly one candidate drawable lies in
    // that gap, it is the one (checked against geometry, 2026-11-20).
    let gap_pick = |g: &str, want_box: &[[f32; 3]; 2]| -> Option<(String, &'static str)> {
        let g = g.to_ascii_lowercase();
        let i = by_name.partition_point(|x| x.0 < g);
        let pred = by_name[..i].last().map(|x| x.1)?;
        let succ = by_name.get(i).map(|x| x.1)?;
        let (lo, hi) = (pred.min(succ), pred.max(succ));
        if lo == hi { return None; }
        // other named entries inside the gap mean the name order does not hold here
        let a = named_offs.partition_point(|&o| o <= lo);
        let b = named_offs.partition_point(|&o| o < hi);
        if b > a {
            return None;
        }
        let inside = gap_candidates(&draw_by_off, lo, hi)?;
        match inside.len() {
            1 => Some((draws[inside[0].1].0.clone(), "name-order gap (single candidate)")),
            2..=6 => {
                // several unnamed entries in the gap: the one whose bounds fit best (within 0.5 m)
                let err = |b: &[[f32; 3]; 2]| (0..2).flat_map(|j| (0..3).map(move |k| (j, k))).map(|(j, k)| (b[j][k] - want_box[j][k]).abs()).fold(0f32, f32::max);
                let mut best: Vec<(f32, usize)> = inside.iter().map(|x| (err(&draws[x.1].1), x.1)).collect();
                best.sort_by(|a, b| a.0.total_cmp(&b.0));
                (best[0].0 < 0.5 && best.get(1).map_or(true, |x| x.0 > best[0].0 * 2.0 + 0.05)).then(|| (draws[best[0].1].0.clone(), "name-order gap + closest bounds"))
            }
            _ => None,
        }
    };
    let mut n_gap = 0;
    let mut n_emb = 0;
    let mut n_local = 0;
    let mut links = serde_json::Map::new();
    let (mut n_bbox, mut n_amb, mut n_dd, mut n_name) = (0, 0, 0, 0);
    let mut keys: Vec<&u32> = unresolved.keys().collect();
    keys.sort();
    for h in keys {
        let d = unresolved[h];
        let mut v = serde_json::json!({"meta": d.3, "bbox": d.1});
        if let Some(n) = guessed.get(h) {
            v["name"] = serde_json::json!(n);
            v["name_source"] = serde_json::json!("joaat of a sibling-name stem + suffix");
            n_name += 1;
        }
        if let Some((n, ids)) = embedded.get(h) {
            // the drawable names itself: joaat(name) is this archetype's hash
            v["name"] = serde_json::json!(n);
            v["name_source"] = serde_json::json!("name stored in the drawable resource");
            if ids.len() == 1 {
                v["model_id"] = serde_json::json!(ids[0]);
                v["match"] = serde_json::json!("embedded drawable name");
                n_emb += 1;
                links.insert(format!("{h:08x}"), v);
                continue;
            }
            v["embedded_name_candidates"] = serde_json::json!(ids.iter().take(8).collect::<Vec<_>>());
        }
        let k = (d.1[0][0] * 4.0).floor() as i64;
        let mut c: Vec<&str> = (k - 1..=k + 1).flat_map(|k| buckets.get(&k).into_iter().flatten()).filter(|&&i| close(&draws[i].1, &d.1)).map(|&i| draws[i].0.as_str()).collect();
        c.sort();
        c.dedup();
        let vol = (0..3).map(|k| (d.1[1][k] - d.1[0][k]).max(0.0)).product::<f32>();
        if vol < 0.01 {
            v["match"] = serde_json::json!("none (degenerate bbox)");
        } else if c.len() == 1 {
            v["model_id"] = serde_json::json!(c[0]);
            v["match"] = serde_json::json!("bbox (unique)");
            n_bbox += 1;
        } else if c.len() > 1 {
            // Several drawables share this bbox: keep the one between the guessed
            // name's alphabetical neighbours, else the one inside the meta file's
            // sibling span; otherwise leave it unlinked.
            let by_order = guessed.get(h).and_then(|g| {
                let g = g.to_ascii_lowercase();
                let tail = g.rsplit('_').next().unwrap_or("").to_string();
                let i = by_name.partition_point(|x| x.0 < g);
                let pred = by_name[..i].iter().rev().find(|x| x.0.ends_with(&tail)).map(|x| x.1)?;
                let succ = by_name[i..].iter().find(|x| x.0.ends_with(&tail)).map(|x| x.1)?;
                let (lo, hi) = (pred.min(succ), pred.max(succ));
                let inside: Vec<&&str> = c.iter().filter(|id| (lo..=hi).contains(&off_of(id))).collect();
                (inside.len() == 1).then(|| (inside[0].to_string(), "bbox + name order"))
            });
            let by_meta = || {
                let (lo, hi) = *meta_span.get(d.3.as_str())?;
                let inside: Vec<&&str> = c.iter().filter(|id| (lo..=hi).contains(&off_of(id))).collect();
                (inside.len() == 1).then(|| (inside[0].to_string(), "bbox + meta-sibling span"))
            };
            if let Some((id, how)) = by_order.or_else(by_meta) {
                v["model_id"] = serde_json::json!(id);
                v["match"] = serde_json::json!(how);
                n_local += 1;
            } else {
                v["match"] = serde_json::json!("bbox (ambiguous, not linked)");
                n_amb += 1;
            }
            v["candidates"] = serde_json::json!(c.iter().take(8).collect::<Vec<_>>());
        } else if d.2 != 0 && names.contains_key(&d.2) {
            let (dn, id) = &names[&d.2];
            v["model_id"] = serde_json::json!(id);
            v["match"] = serde_json::json!(format!("drawable dictionary {dn}"));
            n_dd += 1;
        }
        if v.get("model_id").is_none() {
            if let Some((id, how)) = guessed.get(h).and_then(|g| gap_pick(g, &d.1)) {
                v["model_id"] = serde_json::json!(id);
                v["match"] = serde_json::json!(how);
                n_gap += 1;
            }
        }
        links.insert(format!("{h:08x}"), v);
    }
    let unique_defs: HashSet<_> = defs.iter().map(|d| d.0).collect();
    let stats = serde_json::json!({"archetype_defs": defs.len(), "unique_archetypes": unique_defs.len(), "unresolved_archetypes": unresolved.len(), "candidate_drawables": draws.len(),
        "named_archetypes": unique_defs.len() - unresolved.len(),
        "named_member_archetypes": unique_defs.iter().filter(|h| names.get(h).is_some_and(|n| n.1.contains('#'))).count(),
        "linked_by_bbox": n_bbox, "linked_by_bbox_and_locality": n_local, "ambiguous": n_amb, "linked_by_drawable_dictionary": n_dd, "linked_by_name_order_gap": n_gap, "linked_by_embedded_name": n_emb, "embedded_names_found": embedded.len(), "names_guessed": n_name});
    let doc = serde_json::json!({"note": "Archetypes (struct 82d6fc83) with no named drawable, linked first by the name stored inside an unindexed drawable (joaat match), else to the one drawable (unnamed, or named but not matching its own archetype) whose bounds equal the archetype bbox (0.03 m; ambiguous and degenerate boxes are not linked); names guessed by joaat of sibling-name stems. Merged into model_id resolution (placements, lights, ...). Regenerate with `sixng link-archetypes` after re-indexing.",
        "stats": stats, "links": links});
    fs::create_dir_all(ws.index_dir())?;
    fs::write(links_path(ws), serde_json::to_vec_pretty(&doc)?)?;
    Ok(stats)
}

/// Strictly interior candidates; equal or empty archive spans cannot identify a model.
fn gap_candidates(draws: &[(u64, usize)], lo: u64, hi: u64) -> Option<&[(u64, usize)]> {
    if lo >= hi { return None; }
    let s = draws.partition_point(|x| x.0 <= lo);
    let e = draws.partition_point(|x| x.0 < hi);
    if s >= e { return None; }
    Some(&draws[s..e])
}

fn lod_exports(groups: &[Vec<crate::gltf::NodeGroup>]) -> Vec<(usize, String)> {
    let mut lods = Vec::new();
    for (lod, gs) in groups.iter().enumerate() {
        if gs.iter().all(|g| g.prims.is_empty()) { continue; }
        let name = if lods.is_empty() { "model.glb".into() } else { format!("model_lod{lod}.glb") };
        lods.push((lod, name));
    }
    lods
}

#[cfg(test)]
mod dictionary_tests {
    use super::*;

    #[test]
    fn name_gap_rejects_same_dictionary_and_empty_spans() {
        let draws = [(10, 0), (20, 1), (30, 2)];
        assert!(gap_candidates(&draws, 20, 20).is_none());
        assert!(gap_candidates(&draws, 10, 20).is_none());
        assert!(gap_candidates(&draws, 21, 29).is_none());
        assert_eq!(gap_candidates(&draws, 10, 30), Some(&draws[1..2]));
    }

    #[test]
    fn lowest_populated_lod_is_primary_without_reordering_other_lods() {
        let group = || crate::gltf::NodeGroup { name: "member".into(), prims: vec![crate::gltf::TriPrim {
            name: "triangle".into(), positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: None, tangents: None, uvs: vec![], colors: None, tint: None, indices: vec![0, 1, 2],
            material: None, joints: None, weights: None, joints1: None, weights1: None,
        }] };
        let mut groups = vec![vec![], vec![], vec![group()], vec![group()], vec![], vec![group()]];
        assert_eq!(lod_exports(&groups), [(2, "model.glb".into()), (3, "model_lod3.glb".into()), (5, "model_lod5.glb".into())]);
        groups[0].push(group());
        assert_eq!(lod_exports(&groups)[0], (0, "model.glb".into()));
        assert_eq!(lod_exports(&groups)[1], (2, "model_lod2.glb".into()));
    }

    fn entry() -> Entry {
        Entry {
            id: "levels.rpf@10".into(), off: 0x10, csize: 100, usize: 100,
            streams: vec![[0x10, 100, 100]], kind: Kind::RageObj,
            sub: String::new(), ext: "bin".into(), name: Some("alpha_01".into()),
            rtype: Some("dr".into()), members: vec!["alpha_01".into(), "beta_02".into()],
        }
    }

    #[test]
    fn cached_ordinary_drawable_with_meshes_regenerates_missing_primary_glb() {
        use std::io::Write;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let work = std::env::temp_dir().join(format!("sixng-export-cache-test-{stamp}"));
        fs::create_dir_all(&work).unwrap();
        let mut sys = vec![0u8; 0x600];
        let put64 = |data: &mut [u8], at: usize, value: u64| data[at..at + 8].copy_from_slice(&value.to_le_bytes());
        // Ordinary drawable with one triangle, present only at LOD 2.
        for (at, target) in [(0x60, 0x100), (0x100, 0x120), (0x120, 0x180),
            (0x188, 0x200), (0x1a0, 0x220), (0x200, 0x280), (0x288, 0x400),
            (0x2c0, 0x480), (0x480, 0x420)] {
            put64(&mut sys, at, 0x5000_0000 | target);
        }
        for at in [0x108, 0x190] { sys[at..at + 4].copy_from_slice(&[1, 0, 1, 0]); }
        let mut gfx = vec![0u8; 0xb8];
        gfx[..4].copy_from_slice(&0x3063u32.to_le_bytes());
        gfx[4..8].copy_from_slice(&0x4201u32.to_le_bytes()); // positions end at 0x84
        for (i, p) in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]].iter().enumerate() {
            for (k, v) in p.iter().enumerate() { gfx[0x60 + i * 12 + k * 4..0x64 + i * 12 + k * 4].copy_from_slice(&v.to_le_bytes()); }
        }
        // One meshlet with one triangle and three vertex remaps.
        for (at, value) in [(0x90, 1u16), (0x94, 1), (0x98, 3), (0xb2, 1), (0xb4, 2)] {
            gfx[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }
        gfx[0xac..0xb0].copy_from_slice(&(1u32 << 7 | 2 << 14).to_le_bytes());
        for (at, offset, words) in [(0x400, 0u64, 0x24u32), (0x420, 0x90, 10)] {
            sys[at + 8..at + 12].copy_from_slice(&words.to_le_bytes());
            sys[at + 12..at + 16].copy_from_slice(&[4, 0, 8, 0]);
            put64(&mut sys, at + 0x18, 0x6000_0000 | offset);
        }
        let mut e = entry();
        e.members.clear();
        e.streams.clear();
        let mut archive = Vec::new();
        for data in [&sys, &gfx] {
            let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(data).unwrap();
            let compressed = encoder.finish().unwrap();
            e.streams.push([archive.len() as u64, compressed.len() as u64, data.len() as u64]);
            archive.extend(compressed);
        }
        let a = ArchiveInfo { id: "levels.rpf".into(), path: work.join("levels.rpf"), size: archive.len() as u64, container: "rpf9".into() };
        fs::write(&a.path, archive).unwrap();
        let o = ExportOptions { out: work.join("out"), png: false, dds: false, raw: false,
            force: false, skip_embedded_textures: true, body_shape: vec![], head_skin: 0, paint: None, tex_remap: Default::default(), ped_tints: Default::default() };
        let dir = o.out.join("models/levels").join(export_base_name(&e));
        fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("model.json");
        fs::write(&marker, br#"{"meshes":[{"lod":2}],"glb":null,"lod_glbs":["model_lod2.glb"]}"#).unwrap();
        fs::write(dir.join("model_lod2.glb"), b"old layout").unwrap();
        let item = export_entry(&a, &e, &o).unwrap();
        assert_eq!(item.status, "ok");
        assert_eq!(&fs::read(dir.join("model.glb")).unwrap()[..4], b"glTF");
        assert!(!dir.join("model_lod2.glb").exists());
        let meta: serde_json::Value = serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
        assert_eq!(meta["glb"], "model.glb");
        assert_eq!(meta["meshes"][0]["lod"], 2);
        assert_eq!(meta["triangle_count"], 1);
        assert_eq!(export_entry(&a, &e, &o).unwrap().status, "skipped");
        fs::remove_file(dir.join("model.glb")).unwrap();
        // Resources with no meshes must remain cached even without a GLB.
        for metadata in [br#"{"meshes":[]}"#.as_slice(), b"{}"] {
            fs::write(&marker, metadata).unwrap();
            assert_eq!(export_entry(&a, &e, &o).unwrap().status, "skipped");
            assert_eq!(fs::read(&marker).unwrap(), metadata);
        }
        e.rtype = Some("txd".into());
        let tdir = o.out.join("textures/levels").join(export_base_name(&e));
        fs::create_dir_all(&tdir).unwrap();
        fs::write(tdir.join("dictionary.json"), b"{}").unwrap();
        assert_eq!(export_entry(&a, &e, &o).unwrap().status, "skipped");
        fs::remove_dir_all(work).unwrap();
    }

    #[test]
    fn indexed_member_names_override_the_first_resource_name() {
        let e: Entry = serde_json::from_slice(&serde_json::to_vec(&entry()).unwrap()).unwrap();
        let mut names = std::collections::HashMap::new();
        register_model_names(&mut names, &e);
        for name in &e.members {
            assert_eq!(names[&crate::meta7a::joaat(name)], (name.clone(), format!("{}#{name}", e.id)));
        }
        let selected = member_entry(&e, "alpha_01").unwrap();
        assert_eq!(selected.streams, e.streams);
        assert_eq!(selected.name.as_deref(), Some("alpha_01"));
        assert_eq!(export_base_name(&selected), "alpha_01_10_member");
        assert_eq!(export_base_name(&e), "alpha_01_10");
        assert_eq!(entry_off(&selected.id), e.off);
        assert!(!export_base_name(&selected).contains('#'));
        assert!(member_entry(&e, "../bad").is_err());
    }

    #[test]
    fn workspace_rejects_older_index_versions() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let work = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/index-version-test-{stamp}"));
        let ws = Workspace { game: work.clone(), work: work.clone() };
        let a = ArchiveInfo { id: "levels.rpf".into(), path: work.join("levels.rpf"), size: 100, container: "rpf9".into() };
        let mut ix = ArchiveIndex {
            tool_version: "4".into(), archive: a.id.clone(), path: String::new(), size: 100,
            mtime: 0, header: None, container: "rpf9".into(), entries: vec![entry()], stats: Default::default(), names_done: true,
        };
        fs::create_dir_all(ws.index_dir()).unwrap();
        scan::save_index(&ws.index_dir(), &ix).unwrap();
        assert!(ws.load_index(&a).unwrap_err().to_string().contains("index is from an older version"));
        ix.tool_version = crate::INDEX_VERSION.into();
        scan::save_index(&ws.index_dir(), &ix).unwrap();
        assert!(ws.load_index(&a).is_ok());
        fs::remove_dir_all(work).unwrap();
    }

    #[test]
    fn resolver_rejects_members_on_non_drawables() {
        let mut e = entry();
        e.rtype = Some("txd".into());
        let entries = [e];
        let resolver = ModelResolver::new(&entries);
        assert!(resolver.resolve(&entries, "10#alpha_01").is_err());
        assert!(resolver.resolve(&entries, "levels.rpf@10").is_ok());
        assert!(resolver.resolve(&entries, "LEVELS.RPF@10").is_ok());
        assert!(resolver.resolve(&entries, "0x10").is_ok());
        assert!(resolver.resolve(&entries, "other.rpf@10").is_err());
    }

    #[test]
    fn export_filter_uses_resolved_member_ids() {
        let ix = ArchiveIndex {
            tool_version: crate::INDEX_VERSION.into(), archive: "levels.rpf".into(),
            path: String::new(), size: 100, mtime: 0, header: None, container: "rpf9".into(),
            entries: vec![entry()], stats: Default::default(), names_done: true,
        };
        for id in ["0x10#beta_02", "levels@10#beta_02", "LEVELS.RPF@10#beta_02", " 10#beta_02 "] {
            let f = Filter { ids: vec![id.into()], ..Default::default() };
            let selected = f.select_exports(&ix).unwrap();
            assert_eq!(selected.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["levels.rpf@10#beta_02"], "{id}");
        }
        let f = Filter { ids: vec!["levels.rpf@10".into(), "0x10#beta_02".into(), "levels@10#beta_02".into()], ..Default::default() };
        assert_eq!(f.select_exports(&ix).unwrap().len(), 2);
    }

    #[test]
    fn export_filter_accepts_member_ids_and_preserves_plain_entry_selection() {
        let e = entry();
        let ix = ArchiveIndex {
            tool_version: crate::INDEX_VERSION.into(), archive: "levels.rpf".into(),
            path: String::new(), size: 100, mtime: 0, header: None, container: "rpf9".into(),
            entries: vec![e.clone()], stats: Default::default(), names_done: true,
        };
        let f = Filter { ids: vec![format!("{}#beta_02", e.id)], ..Default::default() };
        let selected = f.select_exports(&ix).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "levels.rpf@10#beta_02");
        assert_eq!(selected[0].name.as_deref(), Some("beta_02"));
        let bare = Filter { ids: vec!["10#beta_02".into()], ..Default::default() };
        assert_eq!(bare.select_exports(&ix).unwrap()[0].id, selected[0].id);
        assert_eq!(Filter::default().select_exports(&ix).unwrap()[0].id, e.id);
        let f = Filter { ids: vec![e.id.clone(), format!("{}#alpha_01", e.id)], ..Default::default() };
        assert_eq!(f.select_exports(&ix).unwrap().len(), 2);
    }
}
