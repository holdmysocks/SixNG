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
const VERSION: u32 = 1;

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
        let suffix = format!("{PRESET:08x}");
        for (k, objs) in pso_json["objects"].as_object().into_iter().flatten() {
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

    pub fn get(&self, name: u32) -> Option<&Preset> {
        self.presets.get(&format!("{name:08x}"))
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
            let _ = crate::atomic::write(p, b);
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
        assert_eq!(t.presets.len(), 2);
        let p = t.get(333928843).unwrap();
        assert_eq!((p.a, p.b, p.texture, p.flag), ([19, 13, 10], [1, 1, 1], 0xb79e27e80e2c837d, false));
        assert!((p.values[TILING] - 8.283).abs() < 1e-4 && p.values.len() == 3);
        let q = t.get(7).unwrap();
        assert_eq!((q.a, q.texture, q.flag), ([255; 3], 0, true));
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
