//! Vehicle part colours: what each of the 256 part ids of a vehicle's
//! two-layer materials looks like.
//!
//! The body, interior and mechanical meshes of a vehicle share a few
//! `standard_vehicle_2lyr*` materials; each vertex carries a part id (0-255)
//! in the alpha of its vertex colour. Three things give that id a look:
//!
//! - A **part set** (`f14244fd`, one per vehicle body or mod part model, in
//!   its own `0x7a` resource in `packs.rpf`): two 256-entry tables, one
//!   naming a colour record per part id (`8b464cf4.2efff0ae[i].4f9f7603`),
//!   one naming a surface record (`578f6a03.4e3e21e7[i].8576d517`). Its
//!   `name` is a 64-bit value that the vehicle's variation sets
//!   (`fc1c7000`) and mod part elements (`77f2b89e.8605bf0e`) carry.
//! - A **library** (one `0x7a` resource in `data.rpf`) with the records the
//!   names resolve to: colour records `f1379639` (`colour`, a row index
//!   `c8313558` and a mode `adb52ec3`), surface records `9e4de959` (their
//!   `2cc72f7c` names a `307f162c` record with `roughnessLow` /
//!   `roughnessHigh`).
//! - The **material palette** texture (128 x 256, slot `19df25d7`, the same
//!   texture on every vehicle): row = a colour record's row index, 128
//!   columns from dark to bright. Rows of fixed materials (mode 3: rubber,
//!   plastics, leathers, chrome) are stored; the rows of paints (mode 1) are
//!   black in the file and filled by the game from the paint colours.
//!
//! Colours (`type 0x15`) are stored with red in the low byte: the 151 named
//! colours `4ce375e6` of the library start with GTA IV's car colour list
//! (37,37,39 / 101,106,121 / 88,89,90 ...) only when read that way.
//!
//! Not decoded: what picks the column of a palette row (the middle of the
//! row is used), the per-configuration modifier records that reassign
//! colours and surfaces to groups of part ids (interior colour choices,
//! trim options), the second layer's colour, metalness.

use crate::meta7a::{self, Meta};
use crate::resource::Resource;
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// Version of what an export writes for part colours (model.json
/// `part_colours_version`); the model viewer exports older vehicles again.
pub const EXPORT_VERSION: u32 = 2;
/// Layout version of `<work>/index/vehicle_parts.json`.
pub const FORMAT: u32 = 1;

const PART_SET: u32 = 0xf142_44fd;
const SET_COLOURS: u32 = 0x7847_6477;
const SET_SURFACES: u32 = 0x9ac2_b332;
const COLOUR_ENTRIES: u32 = 0x2eff_f0ae;
const ENTRY_COLOUR: u32 = 0x4f9f_7603;
const SURFACE_ENTRIES: u32 = 0x4e3e_21e7;
const ENTRY_SURFACE: u32 = 0x8576_d517;
const COLOUR_REC: u32 = 0xf137_9639;
const COLOUR: u32 = 0x5bdc_13e3;
const COLOUR_MODE: u32 = 0xadb5_2ec3;
const COLOUR_ROW: u32 = 0xc831_3558;
const SURFACE: u32 = 0x9e4d_e959;
const SURFACE_ROUGH: u32 = 0x2cc7_2f7c;
const ROUGH: u32 = 0x307f_162c;
const ROUGH_LOW: u32 = 0xe4be_3b70;
const ROUGH_HIGH: u32 = 0x26e5_d460;
const NAME: u32 = 0xe6e1_7f14;
const OWNER: u32 = 0xfb1f_80ff;
/// The colour record of the primary body paint (its stored colour is a
/// dark grey default; a configuration's body colour replaces it).
pub const PRIMARY_PAINT: u32 = 0x2640_0a11;

/// One part id of a part set.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Part {
    /// Name hash of the colour record (`f1379639`).
    pub colour: u32,
    /// Name hash of the surface record (`9e4de959`).
    pub surface: u32,
    /// Row of the material palette, or -1.
    pub row: i16,
    /// The colour record's mode: 3 = a fixed material whose colours are the
    /// stored palette row, 1 = a paint whose row the game fills from `rgb`.
    pub mode: i8,
    /// The colour record's colour (sRGB bytes).
    pub rgb: [u8; 3],
    /// `roughnessLow`, `roughnessHigh` of the surface's first layer.
    pub rough: [f32; 2],
}

impl Part {
    pub fn is_paint(&self) -> bool {
        self.mode != 3
    }
    pub fn is_primary(&self) -> bool {
        self.colour == PRIMARY_PAINT
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Tables {
    pub format: u32,
    /// Part set name (16 hex digits) -> 256 parts.
    pub sets: HashMap<String, Vec<Part>>,
    /// joaat of a vehicle model name (8 hex digits) -> part set name.
    pub models: HashMap<String, String>,
    /// joaat of a mod part model name -> part set name.
    pub elements: HashMap<String, String>,
    pub library: Option<String>,
}

fn get<'v>(v: &'v Value, h: u32) -> Option<&'v Value> {
    v.get(meta7a::field_name(h))
}

fn num(v: &Value, h: u32) -> Option<i64> {
    get(v, h).and_then(|x| x.as_i64())
}

fn metas(game_root: &Path, work: &Path, archive: &str) -> Result<(std::path::PathBuf, Vec<crate::scan::Entry>)> {
    let ixp = crate::scan::index_path(&work.join("index"), archive);
    let ix = crate::scan::load_index(&ixp).map_err(|e| anyhow!("{archive} index ({}): {e}; index it first", ixp.display()))?;
    let es = ix.entries.into_iter().filter(|e| e.kind == crate::classify::Kind::Resource && e.sub == "7a").collect();
    Ok((game_root.join(archive), es))
}

struct Library {
    /// name -> (mode, row, rgb)
    colours: HashMap<u32, (i8, i16, [u8; 3])>,
    /// surface name -> roughness record name
    surfaces: HashMap<u32, u32>,
    rough: HashMap<u32, [f32; 2]>,
    id: Option<String>,
}

fn library(game_root: &Path, work: &Path) -> Result<Library> {
    let (path, es) = metas(game_root, work, "data.rpf")?;
    let found: Vec<Library> = es
        .par_iter()
        .filter_map(|e| {
            let r = Resource::load_sys(&path, e).ok()?;
            let m = Meta::parse(&r)?;
            if !m.structs.contains_key(&COLOUR_REC) {
                return None;
            }
            let m = &m;
            let dump = |h: u32| m.objects(h).into_iter().filter_map(|o| m.object(h, o, 0)).collect::<Vec<_>>();
            let mut l = Library { colours: HashMap::new(), surfaces: HashMap::new(), rough: HashMap::new(), id: Some(e.id.clone()) };
            for c in dump(COLOUR_REC) {
                let (Some(n), Some(col)) = (num(&c, NAME), num(&c, COLOUR)) else { continue };
                let col = col as u32;
                l.colours.insert(n as u32, (num(&c, COLOUR_MODE).unwrap_or(-1) as i8, num(&c, COLOUR_ROW).unwrap_or(-1) as i16, [col as u8, (col >> 8) as u8, (col >> 16) as u8]));
            }
            for s in dump(SURFACE) {
                if let (Some(n), Some(r)) = (num(&s, NAME), num(&s, SURFACE_ROUGH)) {
                    l.surfaces.insert(n as u32, r as u32);
                }
            }
            for s in dump(ROUGH) {
                let f = |h| get(&s, h).and_then(|x| x.as_f64()).unwrap_or(0.5) as f32;
                if let Some(n) = num(&s, NAME) {
                    l.rough.insert(n as u32, [f(ROUGH_LOW), f(ROUGH_HIGH)]);
                }
            }
            Some(l)
        })
        .collect();
    found.into_iter().max_by_key(|l| l.colours.len()).ok_or_else(|| anyhow!("no colour library (struct f1379639) in data.rpf"))
}

/// Read the part sets, the library and the links from the game files.
pub fn build(game_root: &Path, work: &Path) -> Result<Tables> {
    let lib = library(game_root, work)?;
    let (path, es) = metas(game_root, work, "packs.rpf")?;
    // (sets, model links, element links) per resource
    type Found = (Vec<(String, Vec<Part>)>, Vec<(u32, Vec<String>)>, Vec<(u32, Vec<String>)>);
    let found: Vec<Found> = es
        .par_iter()
        .filter_map(|e| {
            let r = Resource::load_sys(&path, e).ok()?;
            let m = Meta::parse(&r)?;
            let has = |h: u32| m.structs.contains_key(&h);
            if !has(PART_SET) && !has(crate::vehicles::MODEL_INFO) {
                return None;
            }
            let mut sets = Vec::new();
            for o in m.objects(PART_SET) {
                let Some(v) = m.object(PART_SET, o, 3) else { continue };
                let Some(name) = get(&v, NAME).and_then(|x| x.as_str()) else { continue };
                let list = |outer: u32, inner: u32| get(&v, outer).and_then(|x| x.as_array()).and_then(|a| a.first()).and_then(|t| get(t, inner)).and_then(|x| x.as_array()).cloned().unwrap_or_default();
                let (cols, surfs) = (list(SET_COLOURS, COLOUR_ENTRIES), list(SET_SURFACES, SURFACE_ENTRIES));
                if cols.len() != 256 || surfs.len() != 256 {
                    continue;
                }
                let parts = cols
                    .iter()
                    .zip(&surfs)
                    .map(|(c, s)| {
                        let colour = num(c, ENTRY_COLOUR).unwrap_or(0) as u32;
                        let surface = num(s, ENTRY_SURFACE).unwrap_or(0) as u32;
                        let (mode, row, rgb) = lib.colours.get(&colour).copied().unwrap_or((-1, -1, [0; 3]));
                        let rough = lib.surfaces.get(&surface).and_then(|r| lib.rough.get(r)).copied().unwrap_or([0.5, 0.5]);
                        Part { colour, surface, row, mode, rgb, rough }
                    })
                    .collect();
                sets.push((name.to_string(), parts));
            }
            // every 64-bit value of a record is a candidate link; the ones
            // that name a part set are kept once all sets are known
            let u64s = |v: &Value| v.as_object().into_iter().flatten().filter_map(|(_, x)| x.as_str()).filter(|s| s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit())).map(|s| s.to_string()).collect::<Vec<_>>();
            let links = |h: u32, key: u32| m.objects(h).into_iter().filter_map(|o| m.object(h, o, 0)).filter_map(|v| Some((num(&v, key)? as u32, u64s(&v)))).collect::<Vec<_>>();
            let single = m.objects(crate::vehicles::MODEL_INFO).len() == 1;
            let (models, elements) = if single { (links(crate::vehicles::VARIATION, OWNER), links(crate::vehicles::MOD_ELEMENT, NAME)) } else { (Vec::new(), Vec::new()) };
            Some((sets, models, elements))
        })
        .collect();
    let mut t = Tables { format: FORMAT, library: lib.id, ..Default::default() };
    for (sets, _, _) in &found {
        for (n, p) in sets {
            t.sets.insert(n.clone(), p.clone());
        }
    }
    for (_, models, elements) in &found {
        for (h, c) in models {
            if let Some(s) = c.iter().find(|s| t.sets.contains_key(*s)) {
                t.models.entry(format!("{h:08x}")).or_insert_with(|| s.clone());
            }
        }
        for (h, c) in elements {
            if let Some(s) = c.iter().find(|s| t.sets.contains_key(*s)) {
                t.elements.entry(format!("{h:08x}")).or_insert_with(|| s.clone());
            }
        }
    }
    Ok(t)
}

fn cache_path(work: &Path) -> std::path::PathBuf {
    work.join("index").join("vehicle_parts.json")
}

/// The tables, from `<work>/index/vehicle_parts.json` (built on first use).
pub fn tables(game_root: &Path, work: &Path) -> Result<Arc<Tables>> {
    static CACHE: OnceLock<Mutex<HashMap<std::path::PathBuf, Arc<Tables>>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().unwrap();
    if let Some(t) = c.get(work) {
        return Ok(t.clone());
    }
    let p = cache_path(work);
    let stored = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Tables>(&b).ok()).filter(|t| t.format == FORMAT);
    let t = match stored {
        Some(t) => t,
        None => {
            let t = build(game_root, work)?;
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&p, serde_json::to_vec(&t)?);
            t
        }
    };
    let t = Arc::new(t);
    c.insert(work.to_path_buf(), t.clone());
    Ok(t)
}

impl Tables {
    /// The part set of a model: a vehicle (`comet7`, `comet7_hi`) or one of
    /// its mod part models.
    pub fn for_model(&self, model: &str) -> Option<(&str, &[Part])> {
        let m = model.to_ascii_lowercase();
        let base = m.strip_suffix("_hi").or_else(|| m.strip_suffix("_mid")).unwrap_or(&m);
        let key = |s: &str| format!("{:08x}", meta7a::joaat(s));
        let set = self.elements.get(&key(&m)).or_else(|| self.elements.get(&key(base))).or_else(|| self.models.get(&key(base)))?;
        self.sets.get_key_value(set).map(|(k, v)| (k.as_str(), v.as_slice()))
    }
}

/// Vertex colours (linear RGB, alpha see below) for the meshes of a
/// two-layer vehicle material: `ids` are the part ids (vertex colour alpha),
/// `palette` the 128 x 256 material palette (sRGB RGBA8).
///
/// Alpha: 254 = primary body paint (a viewer may swap in a paint colour);
/// 0-100 = the middle of the surface's roughness range x 100; 101-201 = the
/// same + 101 on a part taken as bare metal. Metalness is not decoded: a
/// fixed material counts as metal when its palette row is bright and
/// neutral (chrome, polished aluminium) and its surface is smooth.
pub fn vertex_colours(parts: &[Part], palette: &[[u8; 4]], ids: impl Iterator<Item = u8>) -> Vec<[u8; 4]> {
    let lin = |v: u8| {
        let c = v as f32 / 255.0;
        let l = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        (l * 255.0 + 0.5) as u8
    };
    let look: Vec<[u8; 4]> = (0..256)
        .map(|i| {
            let Some(p) = parts.get(i) else { return [255; 4] };
            let mid = ((p.rough[0] + p.rough[1]) * 0.5).clamp(0.0, 1.0);
            let rough = (mid * 100.0 + 0.5) as u8;
            let fixed = !p.is_paint() && p.row >= 0;
            let rgb = if fixed { ramp(palette, p.row as usize) } else { p.rgb };
            let (lo, hi) = (rgb.iter().min().copied().unwrap_or(0), rgb.iter().max().copied().unwrap_or(0));
            let metal = fixed && lo >= 190 && hi - lo <= 24 && mid <= 0.55;
            [lin(rgb[0]), lin(rgb[1]), lin(rgb[2]), if p.is_primary() { 254 } else if metal { rough + 101 } else { rough }]
        })
        .collect();
    ids.map(|i| look[i as usize]).collect()
}

/// The colour of a palette row: the mean of its middle quarter (what picks
/// the column in game is not decoded).
pub fn ramp(palette: &[[u8; 4]], row: usize) -> [u8; 3] {
    const W: usize = 128;
    let mut s = [0u32; 3];
    let cols = W * 3 / 8..W * 5 / 8;
    let n = cols.len() as u32;
    for x in cols {
        let Some(p) = palette.get(row * W + x) else { return [128; 3] };
        for k in 0..3 {
            s[k] += p[k] as u32;
        }
    }
    [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_from_rows_and_paints() {
        let mut pal = vec![[0u8, 0, 0, 255]; 128 * 256];
        for x in 0..128 {
            pal[5 * 128 + x] = [200, 100, 50, 255];
        }
        let mut parts = vec![Part { mode: 3, row: 5, rough: [0.2, 0.4], ..Default::default() }; 256];
        parts[1] = Part { colour: PRIMARY_PAINT, mode: 1, row: 1, rgb: [255, 0, 0], ..Default::default() };
        parts[2] = Part { colour: 7, mode: 1, row: 2, rgb: [0, 255, 0], rough: [1.0, 1.0], ..Default::default() };
        let c = vertex_colours(&parts, &pal, [0u8, 1, 2].into_iter());
        assert_eq!(ramp(&pal, 5), [200, 100, 50]);
        // fixed material: the palette row (linear), alpha = roughness 0.3 x 100
        assert_eq!(c[0][3], 30);
        assert!(c[0][0] > c[0][1] && c[0][1] > c[0][2]);
        // primary paint is marked, other paints keep their own colour
        assert_eq!(c[1], [255, 0, 0, 254]);
        assert_eq!(c[2], [0, 255, 0, 100]);
        // a bright neutral row on a smooth surface reads as metal
        for x in 0..128 {
            pal[5 * 128 + x] = [240, 240, 244, 255];
        }
        assert_eq!(vertex_colours(&parts, &pal, [0u8].into_iter())[0][3], 131);
    }

    #[test]
    fn model_lookup_strips_lod_suffix() {
        let mut t = Tables::default();
        t.sets.insert("00000000000000aa".into(), vec![Part::default(); 256]);
        t.models.insert(format!("{:08x}", meta7a::joaat("comet7")), "00000000000000aa".into());
        assert!(t.for_model("comet7_hi").is_some());
        assert!(t.for_model("Comet7").is_some());
        assert!(t.for_model("comet8").is_none());
    }
}
