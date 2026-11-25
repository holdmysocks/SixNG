//! Hair colour presets: what a character's hair, brows, lashes and beard
//! are coloured with.
//!
//! A wardrobe hair item (`wardrobe`, struct `f795e8ba`) names a preset with
//! tag `f7b14eb8`, itself or through its parents. The presets are records of
//! struct `6f03dd54` in one PSO of data.rpf (the appearance library, which
//! also holds the eye, skin and skin-overlay presets): 121 of them. A preset
//! stores
//!
//! * two colours (`143a8221` and `88466a37`, RGBA bytes with red first),
//! * a texture (`00557ff5`: 13 presets name one of two 256 x 256 noise
//!   textures, the rest none),
//! * seven numbers and a flag (`e7dff641`: set on the 87 presets whose two
//!   colours are white; what they are for is not decoded).
//!
//! The item's own tint bytes are placeholders on story characters (palette
//! `ped_tint_generic_clean`, rows 0, 0, 0: checked on 196 hair items of 70
//! characters, all of which name a preset), so the preset is where their
//! hair colour is. The presets story characters use are dark, low values
//! (black hair is 1, 1, 1; brown about 19, 13, 10), as a hair shader that
//! scatters light wants them; others are blonde, red, orange, green and teal.
//!
//! Not decoded: how the game mixes the two colours (here: by the noise
//! texture at the strand's root UV, an inference from the preset naming a
//! noise texture and a tiling-like number), and what the seven numbers are.
//!
//! The table is built once from the data.rpf index and cached as
//! `<work>/index/hair_presets.json`.

use crate::meta7a::field_name;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Struct of a preset record.
pub const PRESET: u32 = 0x6f03_dd54;
/// Wardrobe tag whose value is the preset name of a hair item.
pub const TAG: u32 = 0xf7b1_4eb8;
/// The number that looks like the tiling of the noise texture (1 to 10).
pub const TILING: &str = "a43cad33";
/// Struct of a set: four references to presets, one per strand variant.
pub const SET: u32 = 0xcb83_80c7;
/// Struct of a painted hair layer of the head skin (the hair colour on the
/// scalp under the strands, the brows under the brow strands): a mask
/// texture, three more textures and numbers. 642 records in the appearance
/// library.
pub const UNDER: u32 = 0x7fc5_6571;
/// Struct of a layer set: four references to layers. A hair item's set has
/// the name of its placeholder (joaat of the drawable name without
/// `_full`); the third reference (`c13c74b5`) is the item's own layer.
pub const UNDER_SET: u32 = 0x1e87_5c1c;
const VERSION: u32 = 3;

/// The painted hair layer of a hair item: its mask texture (store hash)
/// and the number that reads as its opacity (`0a43bed0`: 0.45 to 0.66 on
/// scalp layers, 0.83 to 1 on brows; what the game does with it is not
/// traced).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnderLayer {
    pub mask: u64,
    pub opacity: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// First and second colour as stored (sRGB bytes).
    pub a: [u8; 3],
    pub b: [u8; 3],
    /// Texture the preset names (0 when none).
    pub texture: u64,
    /// `e7dff641`.
    pub flag: bool,
    /// The preset's numbers by field (hex hash, or the name where known).
    pub values: BTreeMap<String, f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Table {
    version: u32,
    /// Preset name hash (hex) -> preset.
    pub presets: BTreeMap<String, Preset>,
    /// Set name hash (hex) -> the preset names (hex) of strand variants 0..3.
    /// The game's strand generation gives every strand a 2-bit variant
    /// (`hairgen`), and a set names the colour ramp of each. Sets have the
    /// names of the presets; most name one or two distinct presets.
    #[serde(default)]
    pub sets: BTreeMap<String, [String; 4]>,
    /// Layer set name hash (hex) -> the item's own painted hair layer.
    #[serde(default)]
    pub under: BTreeMap<String, UnderLayer>,
}

fn rgb(v: &Value) -> [u8; 3] {
    let c = v.as_u64().unwrap_or(0) as u32;
    [c as u8, (c >> 8) as u8, (c >> 16) as u8]
}

/// One preset from its decoded record.
pub fn preset(v: &Value) -> Preset {
    let tex = &v[field_name(0x0055_7ff5)];
    let texture = tex["name"].as_str().filter(|_| tex["enable"].as_bool() == Some(true)).and_then(|s| u64::from_str_radix(s, 16).ok()).unwrap_or(0);
    let skip = [field_name(0x143a_8221), field_name(0x8846_6a37)];
    let values = v.as_object().into_iter().flatten().filter(|(k, x)| x.is_f64() && !skip.contains(k)).filter_map(|(k, x)| Some((k.clone(), x.as_f64()? as f32))).collect();
    Preset { a: rgb(&v[field_name(0x143a_8221)]), b: rgb(&v[field_name(0x8846_6a37)]), texture, flag: v[field_name(0xe7df_f641)].as_bool().unwrap_or(false), values }
}

impl Table {
    /// The presets of a decoded PSO (`Pso::to_json`).
    pub fn add(&mut self, pso_json: &Value) {
        let (suffix, set) = (format!("{PRESET:08x}"), format!("{SET:08x}"));
        let of = |s: u32| {
            let end = format!("{s:08x}");
            pso_json["objects"].as_object().into_iter().flatten().filter(move |(k, _)| k.ends_with(&end)).flat_map(|(_, v)| v.as_array().into_iter().flatten())
        };
        let layers: std::collections::HashMap<u32, &Value> = of(UNDER).filter_map(|o| Some((o["name"].as_u64()? as u32, o))).collect();
        for s in of(UNDER_SET) {
            let own = s[field_name(0xc13c_74b5)]["name"].as_u64().and_then(|n| layers.get(&(n as u32)));
            let (Some(n), Some(l)) = (s["name"].as_u64(), own) else { continue };
            let m = &l[field_name(crate::meta7a::joaat("mask"))];
            let mask = m["name"].as_str().filter(|_| m["enable"].as_bool() == Some(true)).and_then(|x| u64::from_str_radix(x, 16).ok()).unwrap_or(0);
            if mask != 0 {
                self.under.insert(format!("{:08x}", n as u32), UnderLayer { mask, opacity: l[field_name(0x0a43_bed0)].as_f64().unwrap_or(0.9) as f32 });
            }
        }
        for (k, objs) in pso_json["objects"].as_object().into_iter().flatten() {
            if k.ends_with(&set) {
                for o in objs.as_array().into_iter().flatten() {
                    let r = [0x115f_f942u32, 0xb308_b4a8, 0xc13c_74b5, 0xeda9_786c].map(|h| format!("{:08x}", o[field_name(h)]["name"].as_u64().unwrap_or(0) as u32));
                    if let Some(n) = o["name"].as_u64() {
                        self.sets.insert(format!("{:08x}", n as u32), r);
                    }
                }
            }
            if !k.ends_with(&suffix) {
                continue;
            }
            for o in objs.as_array().into_iter().flatten() {
                if let Some(n) = o["name"].as_u64() {
                    self.presets.insert(format!("{:08x}", n as u32), preset(o));
                }
            }
        }
    }

    /// The painted hair layer of a layer set (a hair item's placeholder name).
    pub fn under_layer(&self, set: u32) -> Option<&UnderLayer> {
        self.under.get(&format!("{set:08x}"))
    }

    pub fn get(&self, name: u32) -> Option<&Preset> {
        self.presets.get(&format!("{name:08x}"))
    }

    /// The presets of strand variants 0..3 of a hair item's set (the
    /// preset of the same name where the set or a reference is missing).
    pub fn variants(&self, name: u32) -> Option<[Preset; 4]> {
        let own = self.get(name)?;
        let key = format!("{name:08x}");
        Some([0, 1, 2, 3].map(|i| self.sets.get(&key).and_then(|s| self.presets.get(&s[i])).unwrap_or(own).clone()))
    }

    /// The table, read from the work directory, or built from the data.rpf
    /// index and written there (a few seconds, once).
    pub fn load_or_build(ws: &crate::export::Workspace) -> Result<Table, String> {
        let p = path(ws);
        if let Some(t) = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Table>(&b).ok()).filter(|t| t.version == VERSION) {
            return Ok(t);
        }
        let a = ws.resolve("data").map_err(|e| format!("{e:#}"))?;
        let aix = ws.load_index(&a).map_err(|e| format!("data.rpf index: {e:#}; index data.rpf first"))?;
        let mut t = Table { version: VERSION, ..Default::default() };
        for e in aix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Pso) {
            let Ok(b) = crate::scan::read_entry(&a.path, e) else { continue };
            if let Some(pso) = crate::pso::parse(&b).filter(|p| p.structs.contains_key(&PRESET)) {
                t.add(&pso.to_json());
            }
        }
        if t.presets.is_empty() {
            return Err("no hair presets found in data.rpf".into());
        }
        if let Ok(b) = serde_json::to_vec(&t) {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, b);
        }
        Ok(t)
    }
}

pub fn path(ws: &crate::export::Workspace) -> PathBuf {
    ws.work.join("index").join("hair_presets.json")
}

/// A stored colour as a viewer without a hair light-scattering model can
/// show it (sRGB): the square root of each channel. A display choice, not
/// the game's shading: the stored values are the dark colours a scattering
/// hair shader takes (brown hair is about 19, 13, 10), and drawn as they
/// are every natural colour is close to black.
pub fn display(c: [u8; 3]) -> [u8; 3] {
    c.map(|x| ((x as f32 / 255.0).sqrt() * 255.0 + 0.5) as u8)
}

/// A noise texture of the presets: one grey value per pixel.
#[derive(Debug, Clone)]
pub struct Noise {
    pub w: usize,
    pub h: usize,
    pub v: Vec<u8>,
}

impl Noise {
    /// Value at a UV (wrapping, nearest pixel), 0..1.
    pub fn at(&self, uv: [f32; 2]) -> f32 {
        if self.w == 0 || self.h == 0 {
            return 0.5;
        }
        let f = |x: f32, n: usize| ((x.rem_euclid(1.0) * n as f32) as usize).min(n - 1);
        self.v[f(uv[1], self.h) * self.w + f(uv[0], self.w)] as f32 / 255.0
    }

    /// The texture of a preset from the game's texture stores.
    pub fn load(game: &Path, texture: u64) -> Option<Noise> {
        let (store, t) = crate::ttpf::global(game).get(texture)?;
        let data = store.data(t).ok()?;
        let (rgba, w, h) = crate::ttpf::decode_rgba(t, &data, 0).ok()?;
        Some(Noise { w: w as usize, h: h as usize, v: rgba.chunks_exact(4).map(|p| p[0]).collect() })
    }
}

/// The colour of one strand (sRGB, for display): the two colours mixed by
/// `n` (0..1, the noise value at the strand's root), the first colour where
/// the noise is bright. Which colour the game puts where is not decoded.
pub fn strand(p: &Preset, n: f32) -> [u8; 3] {
    let (a, b) = (display(p.a), display(p.b));
    let n = n.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| (a[i] as f32 * n + b[i] as f32 * (1.0 - n) + 0.5) as u8)
}

/// The colour of a ramp at `t` (0 root, 1 tip), 0..1 per channel: the two
/// colours blended the way the game's ramp bake blends its colour stops
/// (raised to 2.4, mixed, back by 1 / 2.4; read from the game's code by
/// the shader archive work). The bake takes four colours and a curve from
/// a record that was not found; `color1` at the root to `color2` at the
/// tip, on a straight line, is a stand-in.
pub fn ramp(a: [u8; 3], b: [u8; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| {
        let (x, y) = ((a[i] as f32 / 255.0).powf(2.4), (b[i] as f32 / 255.0).powf(2.4));
        (x + (y - x) * t).powf(1.0 / 2.4)
    })
}

/// A parameter ramp of the game's hair ramp files (binary XML in the packs
/// archive; found and parsed by the shader archive work): four numbers and
/// four curves along the strand. The game bakes one row of its parameter
/// atlas from it, and the strand shader reads that row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamRamp {
    /// First number: about 0.0001 to 0.0003, read as the strand width in metres.
    pub width: f32,
    /// Numbers 2, 3, 4: the multipliers of curves B, C and D.
    pub mul: [f32; 3],
    /// Curves A, B, C, D: ten (x, y) pairs each.
    pub curves: [[f32; 20]; 4],
}

/// A ramp curve at `x` (0 root, 1 tip): its ten points are three cubic
/// Bezier segments sharing end points, evaluated as y at x. Unused trailing
/// segments are stored as (1, 1) points.
pub fn curve(c: &[f32; 20], x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let p = |i: usize| (c[2 * i], c[2 * i + 1]);
    let bez = |a: f32, b: f32, d: f32, e: f32, t: f32| {
        let u = 1.0 - t;
        u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * d + t * t * t * e
    };
    for s in 0..3 {
        let (p0, p1, p2, p3) = (p(3 * s), p(3 * s + 1), p(3 * s + 2), p(3 * s + 3));
        if x > p3.0 && s < 2 && p(3 * s + 6).0 > p3.0 {
            continue;
        }
        // x(t) by bisection (the x control points do not always rise)
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let m = 0.5 * (lo + hi);
            if bez(p0.0, p1.0, p2.0, p3.0, m) < x {
                lo = m;
            } else {
                hi = m;
            }
        }
        return bez(p0.1, p1.1, p2.1, p3.1, 0.5 * (lo + hi));
    }
    c[19]
}

impl ParamRamp {
    /// What the strand shader gets at `t` along the strand: the roughness
    /// value g0 (number 2 x curve B), the fill of the unlit side (number 3 x
    /// curve C), the profile scale (number 4 x curve D) and the coverage
    /// (curve A).
    pub fn at(&self, t: f32) -> [f32; 4] {
        [self.mul[0] * curve(&self.curves[1], t), self.mul[1] * curve(&self.curves[2], t), self.mul[2] * curve(&self.curves[3], t), curve(&self.curves[0], t)]
    }

    /// One record of a parsed ramp file as JSON: `fields` 05..08 and `curves`.
    pub fn from_json(v: &Value) -> Option<ParamRamp> {
        let f = |k: &str| v["fields"][k].as_f64().map(|x| x as f32);
        let cs = v["curves"].as_array()?;
        let mut curves = [[1.0f32; 20]; 4];
        for (i, c) in cs.iter().take(4).enumerate() {
            for (j, x) in c.as_array()?.iter().take(20).enumerate() {
                curves[i][j] = x.as_f64()? as f32;
            }
        }
        Some(ParamRamp { width: f("05")?, mul: [f("06")?, f("07")?, f("08")?], curves })
    }
}

/// A colour ramp of the game's hair ramp files: four colour stops and one
/// curve. The game bakes 16 texels of its colour atlas from it per strand
/// variant (the bake is read from the game's code by the shader archive
/// work): v = curve(t); the stops, raised to 2.4, are mixed as a four-stop
/// gradient over v; the result goes back by 1 / 2.4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColourRamp {
    /// The four stops, 0..1 per channel as stored.
    pub stops: [[f32; 3]; 4],
    pub curve: [f32; 20],
}

impl ColourRamp {
    /// The atlas colour at `t` along the strand (0 root, 1 tip).
    pub fn at(&self, t: f32) -> [f32; 3] {
        let v = curve(&self.curve, t).clamp(0.0, 1.0);
        let w = [(3.0 * v).clamp(0.0, 1.0), (3.0 * v - 1.0).clamp(0.0, 1.0), (3.0 * v - 2.0).clamp(0.0, 1.0)];
        [0, 1, 2].map(|c| {
            let g = |i: usize| self.stops[i][c].max(0.0).powf(2.4);
            let x = g(0) + (g(1) - g(0)) * w[0];
            let x = x + (g(2) - x) * w[1];
            let x = x + (g(3) - x) * w[2];
            x.powf(1.0 / 2.4)
        })
    }

    /// One record of a parsed colour ramp file as JSON: `colours` (four
    /// RGB triples, 0..1) and `curve` (ten (x, y) pairs).
    pub fn from_json(v: &Value) -> Option<ColourRamp> {
        let mut stops = [[0.0f32; 3]; 4];
        for (i, c) in v["colours"].as_array()?.iter().take(4).enumerate() {
            for (j, x) in c.as_array()?.iter().take(3).enumerate() {
                stops[i][j] = x.as_f64()? as f32;
            }
        }
        let mut curve = [1.0f32; 20];
        for (i, p) in v["curve"].as_array()?.iter().take(10).enumerate() {
            curve[2 * i] = p[0].as_f64()? as f32;
            curve[2 * i + 1] = p[1].as_f64()? as f32;
        }
        Some(ColourRamp { stops, curve })
    }
}

/// The game's hair and fur strand tables as parsed from its four ramp files
/// (colour ramps, colour sets, parameter ramps, parameter sets), from one
/// JSON of those four maps keyed by name.
///
/// A hair or fur asset names a colour set and a parameter set (asset
/// catalogue properties 960033653 and 2259149558: the joaat of the set's
/// lower-case name); a wardrobe hair item names a colour set with tag
/// `f7b14eb8`. A set gives one ramp to each of the four strand variants.
/// Field names of a parameter ramp, with what the game's bake puts in its
/// parameter atlas: Roughness x its curve (red: the roughness value g0),
/// BinormalRoughness x its curve (green: the profile), Translucency x its
/// curve (blue: the fill of the unlit side), the Thickness curve (alpha);
/// Thickness is the strand width in metres.
#[derive(Debug, Clone, Default)]
pub struct StrandData {
    colour_sets: std::collections::HashMap<u32, [ColourRamp; 4]>,
    param_sets: std::collections::HashMap<u32, [ParamRamp; 4]>,
    param_flags: std::collections::HashMap<u32, (bool, f32)>,
}

fn pairs20(v: &Value) -> Option<[f32; 20]> {
    let mut c = [1.0f32; 20];
    for (i, p) in v.as_array()?.iter().take(10).enumerate() {
        c[2 * i] = p[0].as_f64()? as f32;
        c[2 * i + 1] = p[1].as_f64()? as f32;
    }
    Some(c)
}

impl StrandData {
    /// From the JSON of the four tables. Sets with a slot whose ramp has
    /// no record are left out.
    pub fn from_json(v: &Value) -> StrandData {
        let hash = |r: &Value| r["hash"].as_str().and_then(|h| u32::from_str_radix(h, 16).ok());
        let pramp = |r: &Value| -> Option<ParamRamp> {
            let f = |k: &str| r[k].as_f64().map(|x| x as f32);
            Some(ParamRamp { width: f("Thickness")?, mul: [f("Roughness")?, f("Translucency")?, f("BinormalRoughness")?], curves: [pairs20(&r["ThicknessCurve"])?, pairs20(&r["RoughnessCurve"])?, pairs20(&r["TranslucencyCurve"])?, pairs20(&r["BinormalRoughnessCurve"])?] })
        };
        let mut d = StrandData::default();
        for s in v["colour_sets"].as_object().into_iter().flat_map(|m| m.values()) {
            let ramps: Vec<ColourRamp> = s["colours"].as_array().into_iter().flatten().filter_map(|n| ColourRamp::from_json(&v["colour_ramps"][n.as_str()?])).collect();
            if let (Some(h), Ok(r)) = (hash(s), <[ColourRamp; 4]>::try_from(ramps)) {
                d.colour_sets.insert(h, r);
            }
        }
        for s in v["parameter_sets"].as_object().into_iter().flat_map(|m| m.values()) {
            let ramps: Vec<ParamRamp> = s["materials"].as_array().into_iter().flatten().filter_map(|n| pramp(&v["parameter_ramps"][n.as_str()?])).collect();
            if let (Some(h), Ok(r)) = (hash(s), <[ParamRamp; 4]>::try_from(ramps)) {
                d.param_sets.insert(h, r);
                d.param_flags.insert(h, (s["UseStandardBsdf"].as_bool().unwrap_or(false), (s["FresnelScale"].as_f64().unwrap_or(255.0) as f32 / 255.0).max(0.5)));
            }
        }
        d
    }

    /// The colour ramps of the four strand variants of a colour set, by the
    /// set's name hash.
    pub fn colour_set(&self, name: u32) -> Option<&[ColourRamp; 4]> {
        self.colour_sets.get(&name)
    }

    /// The parameter ramps of the four strand variants of a parameter set.
    pub fn param_set(&self, name: u32) -> Option<&[ParamRamp; 4]> {
        self.param_sets.get(&name)
    }

    /// Of a parameter set: whether its strands are lit by the standard
    /// lighting (UseStandardBsdf: lighting type 0; else the hair lighting,
    /// type 6), and its reflectance scale max(0.5, FresnelScale / 255) on
    /// F0 = 0.0465.
    pub fn param_flags(&self, name: u32) -> Option<(bool, f32)> {
        self.param_flags.get(&name).copied()
    }

    pub fn counts(&self) -> (usize, usize) {
        (self.colour_sets.len(), self.param_sets.len())
    }
}

/// A value in 0..1 for strand `i`, for presets that name no noise texture.
pub fn scatter(i: usize) -> f32 {
    let mut x = (i as u32).wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x & 0xffff) as f32 / 65535.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layer_set_leads_to_the_mask_and_opacity_of_its_own_layer() {
        let (own, mask, op) = (field_name(0xc13c_74b5), field_name(crate::meta7a::joaat("mask")), field_name(0x0a43_bed0));
        let v = serde_json::json!({ "objects": {
            "090e63b51e875c1c": [
                { "name": 749565774u32, &own: {"name": 858404632u32}, "115ff942": {"name": 3839837909u32} },
                { "name": 5u32, &own: {"name": 6u32} },
                { "name": 8u32, &own: {"name": 9u32} }
            ],
            "9598b7e97fc56571": [
                { "name": 858404632u32, &mask: {"enable": true, "name": "c909cb7bfd7916f7"}, &op: 0.451 },
                { "name": 9u32, &mask: {"enable": false, "name": "0000000000000000"}, &op: 1.0 }
            ]
        }});
        let mut t = Table::default();
        t.add(&v);
        let l = t.under_layer(749565774).unwrap();
        assert_eq!(l.mask, 0xc909_cb7b_fd79_16f7);
        assert!((l.opacity - 0.451).abs() < 1e-6);
        // a set whose layer is missing or has no mask has no layer
        assert!(t.under_layer(5).is_none() && t.under_layer(8).is_none());
    }

    #[test]
    fn a_preset_reads_its_two_colours_texture_and_numbers() {
        let v = serde_json::json!({
            "objects": { "2c87fd416f03dd54": [{
                "00557ff5": {"7be8f914": 1, "_struct": "fd0b85d1", "enable": true, "name": "b79e27e80e2c837d"},
                "143a8221": 4278848787u32, "88466a37": 4278255873u32,
                "3bcc1228": 0.06, "a43cad33": 8.283, "e7dff641": false, "name": 333928843u32, "translucency": 0.0
            }, {
                "00557ff5": {"enable": false, "name": "0000000000000000"},
                "143a8221": 4294967295u32, "88466a37": 4294967295u32, "e7dff641": true, "name": 7u32
            }], "aaaaaaaa00000100": [{"name": 1}] }
        });
        let mut t = Table::default();
        t.add(&v);
        t.add(&serde_json::json!({"objects": {"84df66adcb8380c7": [{"115ff942": {"name": 333928843u32}, "b308b4a8": {"name": 7u32}, "c13c74b5": {"name": 99u32}, "eda9786c": {"name": 333928843u32}, "name": 333928843u32}]}}));
        // variants through the set; a reference to a missing preset falls back
        let var = t.variants(333928843).unwrap();
        assert_eq!((var[0].a, var[1].a, var[2].a, var[3].a), ([19, 13, 10], [255; 3], [19, 13, 10], [19, 13, 10]));
        assert!(t.variants(7).is_some() && t.variants(1234).is_none());
        let r = ramp([255, 0, 128], [0, 0, 128], 0.0);
        assert!((r[0] - 1.0).abs() < 1e-6 && r[1] == 0.0 && (r[2] - 128.0 / 255.0).abs() < 1e-5);
        let m = ramp([255, 0, 0], [0, 0, 0], 0.5);
        assert!((m[0] - 0.5f32.powf(1.0 / 2.4)).abs() < 1e-5);
        assert_eq!(t.presets.len(), 2);
        let p = t.get(333928843).unwrap();
        assert_eq!((p.a, p.b, p.texture, p.flag), ([19, 13, 10], [1, 1, 1], 0xb79e27e80e2c837d, false));
        assert!((p.values[TILING] - 8.283).abs() < 1e-4 && p.values.len() == 3);
        let q = t.get(7).unwrap();
        assert_eq!((q.a, q.texture, q.flag), ([255; 3], 0, true));
    }

    #[test]
    fn a_colour_ramp_runs_through_its_four_stops() {
        // a medium brown of the game's ramp file: straight curve
        let v = serde_json::json!({"colours": [[0.149, 0.137, 0.125], [0.259, 0.2, 0.153], [0.478, 0.396, 0.341], [0.443, 0.384, 0.341]],
            "curve": [[0.0, 0.0], [0.5, 0.5], [0.5, 0.5], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]]});
        let r = ColourRamp::from_json(&v).unwrap();
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 2e-3);
        // the stops sit at v = 0, 1/3, 2/3 and 1
        assert!(near(r.at(0.0), r.stops[0]) && near(r.at(1.0), r.stops[3]), "{:?} {:?}", r.at(0.0), r.at(1.0));
        assert!(near(r.at(1.0 / 3.0), r.stops[1]) && near(r.at(2.0 / 3.0), r.stops[2]));
        // between two stops: between their values, mixed in the 2.4 space (above the plain mean of the lower pair is not required, only inside)
        let m = r.at(0.5);
        assert!(m[0] > r.stops[1][0] && m[0] < r.stops[2][0]);
    }

    #[test]
    fn strand_tables_give_each_variant_its_ramps() {
        let curve = serde_json::json!([[0.0, 0.0], [0.5, 0.5], [0.5, 0.5], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]]);
        let flat = serde_json::json!([[0.0, 1.0], [0.5, 1.0], [0.5, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]]);
        let v = serde_json::json!({
            "colour_ramps": {"A": {"colours": [[0.1, 0.1, 0.1], [0.2, 0.2, 0.2], [0.3, 0.3, 0.3], [0.4, 0.4, 0.4]], "curve": curve}, "B": {"colours": [[1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 1.0]], "curve": curve}},
            "colour_sets": {"S": {"hash": "0000000a", "colours": ["A", "B", "A", "A"]}, "Broken": {"hash": "0000000b", "colours": ["A", "Missing", "A", "A"]}},
            "parameter_ramps": {"P": {"Thickness": 0.00025, "ThicknessCurve": flat, "Roughness": 0.5, "RoughnessCurve": flat, "Translucency": 0.25, "TranslucencyCurve": flat, "BinormalRoughness": 0.9, "BinormalRoughnessCurve": flat}},
            "parameter_sets": {"PS": {"hash": "0000000c", "materials": ["P", "P", "P", "P"], "UseStandardBsdf": true, "FresnelScale": 102}}
        });
        let d = StrandData::from_json(&v);
        assert_eq!(d.counts(), (1, 1));
        let s = d.colour_set(10).unwrap();
        assert!((s[0].at(0.0)[0] - 0.1).abs() < 2e-3 && (s[1].at(0.3)[0] - 1.0).abs() < 1e-4 && (s[0].at(1.0)[0] - 0.4).abs() < 2e-3);
        assert!(d.colour_set(11).is_none());
        assert_eq!(d.param_flags(12), Some((true, 0.5)));
        // roughness, fill (translucency), profile (binormal roughness), coverage
        let p = d.param_set(12).unwrap()[3].at(0.5);
        assert!((p[0] - 0.5).abs() < 1e-4 && (p[1] - 0.25).abs() < 1e-4 && (p[2] - 0.9).abs() < 1e-4 && (p[3] - 1.0).abs() < 1e-4, "{p:?}");
    }

    #[test]
    fn a_parameter_ramp_is_read_along_the_strand() {
        // the feral cat's high-gloss ramp, from the game's ramp file
        let v = serde_json::json!({"fields": {"05": 0.00024, "06": 0.85, "07": 1, "08": 1}, "curves": [
            [0.0, 0.494, 0.022, 0.556, 0.108, 0.997, 0.2, 0.997, 0.4, 0.997, 0.49, 1.0, 0.69, 1.0, 0.89, 1.0, 0.959, 0.29, 1.0, 0.0],
            [0.0, 1.0, 0.014, 0.61, 0.195, 0.66, 0.548, 0.595, 1.001, 0.561, 1.016, 0.262, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            [0.0, 0.0, 0.5, 0.0, 1.0, 0.037, 1.0, 0.537, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            [0.0, 1.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]]});
        let r = ParamRamp::from_json(&v).unwrap();
        assert!((r.width - 0.00024).abs() < 1e-7 && r.mul == [0.85, 1.0, 1.0]);
        // end points of the curves
        assert!((curve(&r.curves[0], 0.0) - 0.494).abs() < 1e-3 && curve(&r.curves[0], 1.0).abs() < 2e-3);
        assert!((curve(&r.curves[1], 0.0) - 1.0).abs() < 1e-3);
        // the profile curve is flat 1, the coverage is full along the middle
        assert!((curve(&r.curves[3], 0.37) - 1.0).abs() < 1e-4 && (curve(&r.curves[0], 0.5) - 1.0).abs() < 0.01);
        // g0 falls from the root towards about 0.5 mid-strand; the fill stays near 0 until the tip
        let (a, m) = (r.at(0.0), r.at(0.5));
        assert!((a[0] - 0.85).abs() < 1e-3 && m[0] > 0.45 && m[0] < 0.6, "{a:?} {m:?}");
        assert!(m[1] < 0.05 && m[2] > 0.99);
        // a straight line as a curve gives its own x
        let line: [f32; 20] = [0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        assert!((curve(&line, 0.3) - 0.3).abs() < 1e-3);
    }

    #[test]
    fn strands_mix_the_two_colours_and_show_brighter_than_stored() {
        let p = Preset { a: [19, 13, 10], b: [1, 1, 1], texture: 0, flag: false, values: Default::default() };
        assert_eq!(display([255, 0, 64]), [255, 0, 128]);
        assert_eq!(strand(&p, 1.0), display(p.a));
        assert_eq!(strand(&p, 0.0), display(p.b));
        let m = strand(&p, 0.5);
        assert!(m[0] > display(p.b)[0] && m[0] < display(p.a)[0]);
        // brown reads as brown, not black
        assert!(display(p.a)[0] > 60 && display(p.a)[0] > display(p.a)[2]);
        let n = Noise { w: 2, h: 1, v: vec![0, 255] };
        assert_eq!((n.at([0.1, 0.5]), n.at([1.6, -3.0])), (0.0, 1.0));
        assert!((0..100).map(scatter).all(|x| (0.0..=1.0).contains(&x)));
        assert!((0..100).map(scatter).fold(0.0, |s, x| s + x) > 30.0);
    }
}
