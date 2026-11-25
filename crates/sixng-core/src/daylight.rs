//! Time of day and weather lighting from the game's timecycle data.
//!
//! [`resolve`] turns the decoded timecycle documents ([`crate::timecycle`])
//! into one variable table per weather type, the way the weather list
//! builds a weather: a named weather is sent to a weather type (the
//! indirection list), whose `DataName` is a base cycle and whose eight
//! `Mod*` fields name modifiers. The base cycle's `GLOBAL` region gives every
//! variable 13 time samples; each modifier (with its submodifiers, depth
//! first, later ones win) replaces variables with a constant or with a curve
//! over the time of day (`keyframe_t_value`, 0..1 = 0..24 h).
//!
//! [`Table::value`] interpolates a variable at any hour between the two
//! neighbouring time samples, eased with the easing of the earlier sample
//! (linear or quartic in-out); a modifier curve is read at hour / 24.
//!
//! [`light`] derives the inputs a viewer needs (sun or moon direction,
//! directional colour and strength, ambient colour and strength, an
//! exposure offset). The timecycle holds multipliers on top of a physical
//! sky model that is not decoded; the sun path, the air-mass tint of the
//! sun, the sky brightness over the day and the dimming of the sun by cloud
//! cover are this module's own simple model (documented on [`light`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Bump when the table layout or the resolution rules change (cache key).
pub const VERSION: u32 = 1;

/// The 12 weather types: (id, named weather in the weather list, label).
/// Order follows the game's weather cycle order.
pub const WEATHERS: [(&str, &str, &str); 12] = [
    ("sunny", "SUNNY", "Sunny"),
    ("clear", "HIGHPRESSURE", "Clear"),
    ("clouds", "CLOUDS", "Clouds"),
    ("overcast", "OVERCAST", "Overcast"),
    ("overcastdark", "OVERCASTDARK", "Overcast, dark"),
    ("misty", "misty", "Misty"),
    ("fog", "FOG", "Fog"),
    ("drizzle", "DRIZZLE", "Drizzle"),
    ("shower", "Shower", "Shower"),
    ("rain", "Rain", "Rain"),
    ("thunderstorm", "THUNDERSTORM", "Thunderstorm"),
    ("hurricane", "HURRICANE", "Hurricane"),
];

/// The modifier fields of a weather type, in the order they are applied.
const MOD_KEYS: [&str; 8] = ["ModAtmosphere", "ModFetch", "ModHumidity", "ModMisc", "ModNiceness", "ModPrecipitation", "ModTemperature", "ModWind"];

/// Where a variable's value comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Track {
    /// One value per time sample (the base cycle).
    Samples(Vec<f32>),
    /// A modifier's constant.
    Const(f32),
    /// A modifier curve: keyframe times (0..1 of the day) and values.
    Curve(Vec<f32>, Vec<f32>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weather {
    pub id: String,
    pub label: String,
    /// Weather type the named weather is sent to (None: the cycle is used as is).
    pub kind: Option<String>,
    /// Base cycle name.
    pub cycle: String,
    /// Modifiers applied, in order.
    pub mods: Vec<String>,
    /// High-level cloud cover (0..1).
    pub cloud_cover: f32,
    pub vars: BTreeMap<String, Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub version: u32,
    /// Hour of each time sample.
    pub hours: Vec<f32>,
    /// Easing of each time sample (`EASE_LINEAR_IN`, `EASE_QUARTIC_INOUT`, ...).
    pub ease: Vec<String>,
    pub weathers: Vec<Weather>,
}

fn f(v: &Value) -> Option<f32> {
    v.as_f64().map(|x| x as f32)
}

fn nums(v: &Value) -> Option<Vec<f32>> {
    v.as_array()?.iter().map(f).collect()
}

fn lower(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_ascii_lowercase()
}

/// A high-level variable of a modifier block (`{output_max, input_max, ...}`)
/// or a plain number.
fn high_level(v: &Value) -> Option<f32> {
    f(v).or_else(|| f(&v["output_max"])).or_else(|| f(&v["input_max"]))
}

fn find_ci<'a>(o: &'a Value, k: &str) -> Option<&'a Value> {
    o.as_object()?.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|x| x.1)
}

/// Build the per-weather tables from the decoded timecycle documents.
pub fn resolve(d: &crate::timecycle::Decoded) -> anyhow::Result<Table> {
    let samples = d.time["samples"].as_array().ok_or_else(|| anyhow::anyhow!("no time samples in the timecycle data"))?;
    let hours: Vec<f32> = samples.iter().filter_map(|s| f(&s["hour"])).collect();
    let ease: Vec<String> = samples.iter().map(|s| s["interpolation_type"].as_str().unwrap_or("EASE_LINEAR_IN").to_string()).collect();
    anyhow::ensure!(hours.len() >= 2 && hours.len() == ease.len(), "time samples incomplete");
    let cycles: BTreeMap<String, &Value> = d.cycles.iter().map(|c| (lower(c, "name"), c)).collect();
    let mods: BTreeMap<String, &Value> = d.modifiers.iter().map(|m| (lower(m, "name"), m)).collect();
    let w = &d.weather;
    let indir: BTreeMap<String, String> = w["WeatherTypeIndirectionList"]
        .as_array()
        .map(|l| l.iter().map(|i| (lower(i, "From"), lower(i, "To"))).collect())
        .unwrap_or_default();
    let types: BTreeMap<String, &Value> = w["WeatherTypes"].as_array().map(|l| l.iter().map(|t| (lower(t, "Name"), t)).collect()).unwrap_or_default();
    let mut out = Table { version: VERSION, hours: hours.clone(), ease, weathers: vec![] };
    for (id, name, label) in WEATHERS {
        let kind = indir.get(&name.to_ascii_lowercase()).and_then(|t| types.get(t)).copied();
        let base_name = kind.and_then(|k| k["DataName"].as_str()).unwrap_or(name).to_ascii_lowercase();
        let Some(base) = cycles.get(&base_name) else { continue };
        let mut vars = BTreeMap::new();
        if let Some(g) = find_ci(&base["regions"], "GLOBAL").and_then(Value::as_object) {
            for (k, v) in g {
                if let Some(s) = nums(v).filter(|s| s.len() == hours.len()) {
                    vars.insert(k.clone(), Track::Samples(s));
                }
            }
        }
        let mut cloud = find_ci(&base["high_level_vars"], "cloud_cover").and_then(high_level).unwrap_or(0.0);
        let mut used = vec![];
        if let Some(k) = kind {
            let mut queue: Vec<String> = MOD_KEYS.iter().filter_map(|m| k[*m].as_str()).filter(|s| !s.is_empty()).map(str::to_string).collect();
            let mut seen = std::collections::BTreeSet::new();
            while !queue.is_empty() {
                let n = queue.remove(0);
                if !seen.insert(n.to_ascii_lowercase()) {
                    continue;
                }
                let Some(m) = mods.get(&n.to_ascii_lowercase()) else { continue };
                used.push(m["name"].as_str().unwrap_or(&n).to_string());
                if let Some(s) = m["submodifiers"].as_array() {
                    queue.extend(s.iter().filter_map(Value::as_str).map(str::to_string));
                }
                if let Some(vs) = m["vars"].as_object() {
                    for (k, v) in vs {
                        if let Some(x) = f(&v["value"]) {
                            vars.insert(k.clone(), Track::Const(x));
                        }
                    }
                }
                for ax in m["axes"].as_array().into_iter().flatten() {
                    let av = &ax["vars"];
                    let Some(ts) = nums(&av["keyframe_t_value"]).filter(|t| !t.is_empty()) else { continue };
                    for (k, v) in av.as_object().into_iter().flatten() {
                        if k == "keyframe_t_value" || k == "keyframe_alpha_value" {
                            continue;
                        }
                        if let Some(vs) = nums(v).filter(|v| v.len() == ts.len()) {
                            vars.insert(k.clone(), Track::Curve(ts.clone(), vs));
                        }
                    }
                }
                if let Some(c) = find_ci(&m["high_level_vars"], "cloud_cover").and_then(high_level) {
                    cloud = c;
                }
            }
        }
        out.weathers.push(Weather {
            id: id.into(),
            label: label.into(),
            kind: kind.and_then(|k| k["Name"].as_str()).map(str::to_string),
            cycle: base["name"].as_str().unwrap_or_default().to_string(),
            mods: used,
            cloud_cover: cloud,
            vars,
        });
    }
    anyhow::ensure!(!out.weathers.is_empty(), "no weather cycles found");
    Ok(out)
}

/// Easing of a time-sample segment, t in 0..1.
pub fn ease(kind: &str, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if kind.contains("QUARTIC") && kind.contains("INOUT") {
        if t < 0.5 {
            8.0 * t.powi(4)
        } else {
            1.0 - 8.0 * (1.0 - t).powi(4)
        }
    } else {
        t
    }
}

impl Table {
    pub fn weather(&self, id: &str) -> Option<&Weather> {
        self.weathers.iter().find(|w| w.id == id)
    }

    /// The two samples around `hour` (wrapping past the last sample to the
    /// first at +24 h) and the eased blend factor.
    pub fn segment(&self, hour: f32) -> (usize, usize, f32) {
        let h = hour.rem_euclid(24.0);
        let n = self.hours.len();
        let mut i = n - 1;
        for k in 0..n {
            if self.hours[k] <= h {
                i = k;
            }
        }
        let j = (i + 1) % n;
        let (a, mut b) = (self.hours[i], self.hours[j]);
        if b <= a {
            b += 24.0;
        }
        let hh = if h < a { h + 24.0 } else { h };
        let t = (hh - a) / (b - a).max(1e-6);
        (i, j, ease(&self.ease[i], t))
    }

    /// A variable of a weather at an hour (None if the weather does not set it).
    pub fn value(&self, w: &Weather, var: &str, hour: f32) -> Option<f32> {
        Some(match w.vars.get(var)? {
            Track::Const(x) => *x,
            Track::Samples(s) => {
                let (i, j, t) = self.segment(hour);
                s[i] + (s[j] - s[i]) * t
            }
            Track::Curve(ts, vs) => curve_at(ts, vs, hour.rem_euclid(24.0) / 24.0),
        })
    }

    pub fn get(&self, w: &Weather, var: &str, hour: f32, default: f32) -> f32 {
        self.value(w, var, hour).unwrap_or(default)
    }

    fn rgb(&self, w: &Weather, var: &str, hour: f32) -> [f32; 3] {
        ["r", "g", "b"].map(|c| self.get(w, &format!("{var}_{c}"), hour, 1.0))
    }
}

fn curve_at(ts: &[f32], vs: &[f32], t: f32) -> f32 {
    if t <= ts[0] {
        return vs[0];
    }
    for i in 0..ts.len() - 1 {
        if ts[i] <= t && t <= ts[i + 1] {
            let f = (t - ts[i]) / (ts[i + 1] - ts[i]).max(1e-6);
            return vs[i] + (vs[i + 1] - vs[i]) * f;
        }
    }
    vs[vs.len() - 1]
}

/// Lighting inputs for a viewer at one hour and weather.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Light {
    /// Unit direction towards the directional light, y up, x east, z south.
    pub dir: [f32; 3],
    /// The directional light is the moon.
    pub moon: bool,
    /// Directional colour (max channel 1) and strength, relative to Sunny 12:00 = 1.
    pub dir_col: [f32; 3],
    pub dir_strength: f32,
    /// Sky / ambient tint (max channel 1) and strength, relative to Sunny 12:00 = 1.
    pub sky_col: [f32; 3],
    pub ambient: f32,
    /// Sun elevation in degrees.
    pub sun_elev: f32,
    /// 0 at night, 1 in full day (from the sun elevation).
    pub day: f32,
    /// Twilight (sun near the horizon), 0..1.
    pub dusk: f32,
    /// Cloud cover used to dim the sun.
    pub cloud_cover: f32,
    /// Exposure offset in EV (`postfx_exposure`).
    pub exposure_ev: f32,
    /// The timecycle values used (for display and checks).
    pub light_dir_mult: f32,
    pub ambient_sky_intensity: f32,
}

/// Latitude of the sun path, degrees north (the game's sun path is not decoded).
pub const LATITUDE: f32 = 25.8;
/// Ambient at night relative to the day sky before the timecycle's own
/// ambient multiplier (the physical night sky is not decoded).
pub const NIGHT_SKY: f32 = 0.02;
/// Moonlight relative to sunlight before the timecycle's own multipliers,
/// chosen so a clear night is readable without the game's eye adaptation.
pub const MOON: f32 = 0.12;
/// Share of sunlight that full cloud cover takes away (the game's
/// volumetric cloud shadow is not decoded).
pub const CLOUD_DIM: f32 = 0.85;

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn norm3(c: [f32; 3]) -> [f32; 3] {
    let m = c[0].max(c[1]).max(c[2]).max(1e-6);
    c.map(|x| (x / m).max(0.0))
}

/// Direction to the sun at an hour (equinox, [`LATITUDE`]): x east, y up, z south.
pub fn sun_dir(hour: f32) -> [f32; 3] {
    let h = (hour - 12.0) * 15f32.to_radians();
    let lat = LATITUDE.to_radians();
    let (e, n, u) = (-h.sin(), -lat.sin() * h.cos(), lat.cos() * h.cos());
    [e, u, -n]
}

/// Sun tint from the air it crosses: Rayleigh plus aerosol extinction
/// along the Kasten-Young air mass, relative to the sun at the zenith.
fn air_tint(elev_deg: f32, aerosol: f32) -> ([f32; 3], f32) {
    const RAYLEIGH: [f32; 3] = [0.046, 0.108, 0.265];
    let e = elev_deg.max(-1.0);
    let m = 1.0 / (e.to_radians().sin().max(0.0) + 0.50572 * (e + 6.07995).powf(-1.6364));
    let ta = 0.1 * aerosol;
    let t = [0, 1, 2].map(|c| (-(RAYLEIGH[c] + ta) * (m - 1.0)).exp());
    let lum = 0.2126 * t[0] + 0.7152 * t[1] + 0.0722 * t[2];
    (norm3(t), lum)
}

/// Raw (un-normalised) lighting of a weather at an hour.
fn raw(t: &Table, w: &Weather, hour: f32) -> Light {
    let g = |k: &str, d: f32| t.get(w, k, hour, d);
    let s = sun_dir(hour);
    let elev = s[1].clamp(-1.0, 1.0).asin().to_degrees();
    let day = smooth(-4.0, 20.0, elev);
    let dusk = (1.0 - (elev.abs() / 12.0).min(1.0)).max(0.0);
    let mult = g("light_dir_mult", 1.0);
    let base = t.rgb(w, "light_dir_col", hour);
    let cloud = w.cloud_cover.clamp(0.0, 1.0);
    let sun_fade = smooth(-2.0, 3.0, elev);
    let (dir, moon, col, strength) = if sun_fade > 0.0 {
        let sc = t.rgb(w, "sky_scattering_sun_col", hour);
        let (tint, lum) = air_tint(elev, g("sky_scattering_aerosol_density_scale", 1.0));
        let c = [0, 1, 2].map(|i| base[i] * sc[i] * tint[i]);
        let k = mult * g("light_dir_sun_mult", 1.0) * g("sky_scattering_sun_intensity_multiplier", 1.0) * lum * sun_fade * (1.0 - CLOUD_DIM * cloud);
        (s, false, c, k)
    } else {
        // full moon opposite the sun (the moon's orbit is not decoded)
        let m = [-s[0], -s[1], -s[2]];
        let melev = m[1].asin().to_degrees();
        let mc = t.rgb(w, "sky_moon_col", hour);
        let c = [0, 1, 2].map(|i| base[i] * mc[i]);
        let k = MOON * mult * g("light_dir_moon_mult", 1.0) * smooth(2.0, 10.0, melev) * (1.0 - CLOUD_DIM * cloud);
        (m, true, c, k)
    };
    let amb_i = g("ambient_sky_intensity", 1.0);
    let sky_l = day + NIGHT_SKY * (1.0 - day);
    let ambient = amb_i * g("indirect_light_sky_multiplier", 1.0) * sky_l;
    let a = t.rgb(w, "ambient_sky_color_mult", hour);
    let k = t.rgb(w, "sky_scattering_sky_col", hour);
    Light {
        dir,
        moon,
        dir_col: norm3(col),
        dir_strength: strength * col[0].max(col[1]).max(col[2]),
        sky_col: norm3([a[0] * k[0], a[1] * k[1], a[2] * k[2]]),
        ambient,
        sun_elev: elev,
        day,
        dusk,
        cloud_cover: cloud,
        exposure_ev: g("postfx_exposure", 0.0),
        light_dir_mult: mult,
        ambient_sky_intensity: amb_i,
    }
}

/// Lighting of a weather at an hour, with the directional and ambient
/// strengths relative to Sunny at 12:00 (= 1), so the viewer's own scale
/// is kept.
///
/// From the timecycle: `light_dir_col_rgb` x `light_dir_mult` (and
/// `light_dir_sun_mult` x `sky_scattering_sun_intensity_multiplier` x
/// `sky_scattering_sun_col_rgb` for the sun, `light_dir_moon_mult` x
/// `sky_moon_col_rgb` for the moon); `ambient_sky_intensity` x
/// `indirect_light_sky_multiplier` with the tint `ambient_sky_color_mult_rgb`
/// x `sky_scattering_sky_col_rgb`; `postfx_exposure`; the high-level
/// `cloud_cover`. Own model (not decoded): the sun path ([`sun_dir`]), a
/// full moon opposite the sun, the air-mass tint and fade of the sun near
/// the horizon, the sky brightness over the day ([`NIGHT_SKY`]), the moon
/// strength ([`MOON`]) and the sun dimming by cloud cover ([`CLOUD_DIM`]).
pub fn light(t: &Table, w: &Weather, hour: f32) -> Light {
    let mut l = raw(t, w, hour);
    if let Some(r) = t.weather("sunny").map(|s| raw(t, s, 12.0)) {
        l.dir_strength /= r.dir_strength.max(1e-6);
        l.ambient /= r.ambient.max(1e-6);
    }
    l
}

/// Cache file of the resolved table.
pub fn cache_path(ws: &crate::export::Workspace) -> std::path::PathBuf {
    ws.work.join("cache").join("daylight.json")
}

/// The resolved table, from the cache or decoded from `common.rpf` (needs its index).
pub fn load_or_build(ws: &crate::export::Workspace) -> anyhow::Result<Table> {
    let p = cache_path(ws);
    if let Some(t) = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Table>(&b).ok()).filter(|t| t.version == VERSION) {
        return Ok(t);
    }
    let a = ws.resolve("common")?;
    let ix = ws.load_index(&a)?;
    let entries: Vec<&crate::scan::Entry> = ix.entries.iter().collect();
    let d = crate::timecycle::decode(&a.path, &entries)?;
    let t = resolve(&d)?;
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = crate::atomic::write(&p, serde_json::to_vec(&t)?);
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decoded() -> crate::timecycle::Decoded {
        let hours = [0, 4, 5, 6, 7, 9, 12, 16, 18, 19, 20, 21, 22];
        let mut samples = vec![];
        for h in hours {
            let e = if [4, 20, 21, 22].contains(&h) { "EASE_QUARTIC_INOUT" } else { "EASE_LINEAR_IN" };
            samples.push(json!({"hour": h, "interpolation_type": e}));
        }
        let col: Vec<f64> = hours.iter().map(|&h| h as f64 / 24.0).collect();
        crate::timecycle::Decoded {
            time: json!({"samples": samples}),
            cycles: vec![
                json!({"name": "BASE_A", "high_level_vars": {"cloud_cover": 0.2}, "regions": {"GLOBAL": {"light_dir_mult": col, "ambient_sky_intensity": vec![1.0; 13]}}}),
            ],
            modifiers: vec![
                json!({"name": "M1", "submodifiers": ["M2"], "vars": {"ambient_sky_intensity": {"value": 2.0}}}),
                json!({"name": "M2", "vars": {}, "axes": [{"vars": {"keyframe_t_value": [0.0, 1.0], "light_dir_col_r": [0.0, 1.0]}}], "high_level_vars": {"cloud_cover": {"output_max": 0.9}}}),
            ],
            weather: json!({
                "WeatherTypeIndirectionList": [{"From": "SUNNY", "To": "AM_A"}, {"From": "rain", "To": "AM_A"}],
                "WeatherTypes": [{"Name": "AM_A", "DataName": "BASE_A", "ModAtmosphere": "M1"}],
            }),
            ..Default::default()
        }
    }

    #[test]
    fn resolves_modifier_stack() {
        let t = resolve(&decoded()).unwrap();
        let w = t.weather("sunny").unwrap();
        assert_eq!(w.cycle, "BASE_A");
        assert_eq!(w.mods, vec!["M1", "M2"]);
        assert_eq!(w.cloud_cover, 0.9);
        assert_eq!(t.value(w, "ambient_sky_intensity", 3.0), Some(2.0));
        assert!((t.value(w, "light_dir_col_r", 6.0).unwrap() - 0.25).abs() < 1e-6);
        // only weathers whose cycle exists
        assert_eq!(t.weathers.len(), 2);
    }

    #[test]
    fn interpolates_with_easing() {
        let t = resolve(&decoded()).unwrap();
        let w = t.weather("sunny").unwrap();
        // at a sample: its value
        assert!((t.get(w, "light_dir_mult", 12.0, 0.0) - 0.5).abs() < 1e-6);
        // linear segment 12 -> 16
        assert!((t.get(w, "light_dir_mult", 14.0, 0.0) - 14.0 / 24.0).abs() < 1e-6);
        // quartic segment 4 -> 5: a quarter in, 8 t^4 = 1/32 of the way
        let v = t.get(w, "light_dir_mult", 4.25, 0.0);
        assert!((v - (4.0 + 1.0 / 32.0) / 24.0).abs() < 1e-6, "{v}");
        // wrap 22 -> 0 (quartic): halfway at 23 h
        let v = t.get(w, "light_dir_mult", 23.0, 0.0);
        assert!((v - (22.0 / 24.0) * 0.5).abs() < 1e-6, "{v}");
    }

    #[test]
    fn sun_path() {
        let n = sun_dir(12.0);
        assert!(n[1] > 0.89 && n[2] > 0.4, "{n:?}");
        let m = sun_dir(6.0);
        assert!(m[1].abs() < 1e-5 && m[0] > 0.99);
        assert!(sun_dir(0.0)[1] < -0.89);
    }

    #[test]
    fn light_is_relative_to_sunny_noon() {
        let t = resolve(&decoded()).unwrap();
        let l = light(&t, t.weather("sunny").unwrap(), 12.0);
        assert!((l.dir_strength - 1.0).abs() < 1e-5 && (l.ambient - 1.0).abs() < 1e-5);
        let n = light(&t, t.weather("sunny").unwrap(), 0.5);
        assert!(n.moon && n.ambient < 0.1);
    }
}
