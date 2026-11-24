//! Per-model vehicle data for the model viewer: spawn configurations (body
//! colour, rim, tyre), paint palettes and wheelScale, read from
//! `<work>/out/vehicles/vehicles.json` (built on first use when missing, as
//! `sixng vehicles packs` would).

use eframe::egui;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Deserialize)]
struct File {
    vehicles: Vec<Veh>,
}

#[derive(Deserialize)]
struct Veh {
    #[serde(default)]
    lights: Option<LightsRec>,
    #[serde(default)]
    steering_lock: Option<f32>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    make: Option<String>,
    model: Option<String>,
    #[serde(default)]
    model_hash: Option<String>,
    #[serde(default)]
    model_info: Option<Info>,
    #[serde(default)]
    configurations: Vec<Cfg>,
    #[serde(default)]
    palettes: Vec<Vec<Colour>>,
    #[serde(default)]
    mods: Vec<ModRec>,
    #[serde(default)]
    switches: Vec<SwitchRec>,
    #[serde(default)]
    part_keys: HashMap<String, Vec<String>>,
    #[serde(default)]
    wheel_fits: HashMap<String, Vec<WheelFit>>,
}

/// What a fitted part changes on the wheels (metres; `axles` 1 = front,
/// 2 = rear, as a mask).
#[derive(Deserialize, Clone, Copy)]
pub struct WheelFit {
    #[serde(default)]
    pub axles: u32,
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub offset: f32,
    #[serde(default)]
    pub spacing: f32,
}

#[derive(Deserialize)]
struct SwitchRec {
    #[serde(default)]
    part: Option<String>,
    #[serde(default)]
    default: Option<ElemRec>,
    #[serde(default)]
    options: Vec<OptionRec>,
}

#[derive(Deserialize)]
struct OptionRec {
    key: String,
    #[serde(default)]
    also: Option<String>,
    #[serde(default)]
    element: Option<ElemRec>,
}

#[derive(Deserialize)]
struct ModRec {
    name: String,
    slot: String,
    #[serde(default)]
    elements: Vec<ElemRec>,
}

#[derive(Deserialize)]
struct ElemRec {
    bone: String,
    #[serde(default)]
    hides: Vec<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    asset: Option<String>,
}

#[derive(Deserialize)]
struct Info {
    #[serde(rename = "wheelScale")]
    wheel_scale: Option<f32>,
    #[serde(rename = "wheelScaleRear")]
    wheel_scale_rear: Option<f32>,
    #[serde(rename = "steerWheelMult")]
    steer_wheel_mult: Option<f32>,
}

#[derive(Deserialize)]
struct LightsRec {
    #[serde(default)]
    patterns: std::collections::BTreeMap<String, PatternRec>,
    #[serde(default)]
    pack: Vec<String>,
    #[serde(default)]
    archetypes: HashMap<String, String>,
    #[serde(default)]
    lamp_lights: HashMap<String, (String, f32)>,
}

#[derive(Deserialize)]
struct PatternRec {
    #[serde(default)]
    slot: String,
    #[serde(default)]
    speed: f32,
    #[serde(default)]
    lamps: Vec<LampRec>,
}

#[derive(Deserialize)]
struct LampRec {
    bone: String,
    #[serde(default)]
    steps: Vec<(f32, u32)>,
}

/// One light pattern of a vehicle: per lamp the bone name hash and its
/// steps of (phase offset, flash type number).
pub struct LightPattern {
    pub name: String,
    pub slot: String,
    pub speed: f32,
    pub lamps: Vec<(u32, Vec<(f32, u32)>)>,
}

/// A vehicle's light patterns (`lights` of vehicles.json).
#[derive(Default)]
pub struct Lights {
    pub patterns: Vec<LightPattern>,
    /// names of the patterns in the vehicle's own light pack
    pub pack: Vec<String>,
    /// lamp bone name hash -> light archetype name (or its hash)
    pub archetypes: HashMap<u32, String>,
    /// lamp bone name hash -> the colour of its light set (the stored bytes
    /// / 255, taken as linear) and its intensity
    pub lamp_lights: HashMap<u32, ([f32; 3], f32)>,
}

#[derive(Deserialize)]
struct Cfg {
    #[serde(default)]
    light_patterns: Vec<String>,
    #[serde(default)]
    rim_ratio: Option<f32>,
    #[serde(default)]
    parts_key: Option<String>,
    colour: Option<Colour>,
    rim: Option<String>,
    tyre: Option<String>,
    #[serde(default)]
    parts: Vec<String>,
}

#[derive(Deserialize)]
struct Colour {
    srgb_if_linear: String,
}

/// Pure magenta: the colour stored where none is set.
pub fn is_placeholder(c: [f32; 3]) -> bool {
    c[0] > 0.99 && c[1] < 0.01 && c[2] > 0.99
}

fn hex(s: &str) -> Option<[f32; 3]> {
    let s = s.trim_start_matches('#');
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([
        ((v >> 16) & 255) as f32 / 255.0,
        ((v >> 8) & 255) as f32 / 255.0,
        (v & 255) as f32 / 255.0,
    ])
}

/// One spawn configuration: body colour (sRGB, read as if stored linear) and
/// the rim / tyre model names (without `_hi`).
#[derive(Clone)]
pub struct Config {
    pub colour: Option<[f32; 3]>,
    pub rim: Option<String>,
    pub tyre: Option<String>,
    /// `brakedisc_*` and `caliper_*` part models of the configuration.
    pub brake_disc: Option<String>,
    pub caliper: Option<String>,
    /// Rim radius / tyre radius of the configuration's wheel fitment record.
    pub rim_ratio: Option<f32>,
    /// Key of the configuration's part list (names its part colours in an export).
    pub parts_key: Option<String>,
    /// Names of the light patterns the configuration's parts link.
    pub light_patterns: Vec<String>,
    /// The configuration's part list as name hashes.
    pub parts: Vec<u32>,
}

#[derive(Clone, Default)]
pub struct Vehicle {
    pub wheel_scale: Option<f32>,
    pub wheel_scale_rear: Option<f32>,
    /// `steerWheelMult` of the model info.
    pub steer_wheel_mult: Option<f32>,
    /// `fSteeringLock` of the handling record (radians).
    pub steer_lock: Option<f32>,
    /// Light patterns (sirens, strobes).
    pub lights: Option<std::sync::Arc<Lights>>,
    /// The game's own name for the model, with its make ("Bravado '19 Buffalo").
    pub display: Option<String>,
    pub configs: Vec<Config>,
    /// Paint palettes as (colour, share 0..1), most frequent first. The
    /// stored lists repeat colours (e.g. 400 entries, 10 distinct), so they
    /// read as weighted draw tables.
    pub palettes: Vec<Vec<([f32; 3], f32)>>,
    /// Mod parts and kits that have models.
    pub mods: Vec<Mod>,
    /// The chassis part's stock alternatives.
    pub switches: Vec<Switch>,
    /// Part name -> the keys it sets while fitted.
    pub part_keys: HashMap<u32, Vec<u32>>,
    /// Part name -> what it changes on the wheels while fitted.
    pub wheel_fits: HashMap<u32, Vec<WheelFit>>,
}

/// A stock alternative of a part (None = the chassis part, always fitted):
/// while the part is fitted, the element of the option whose key (and
/// second key, where not 0) a fitted part sets is shown (None = nothing),
/// else the default element.
#[derive(Clone)]
pub struct Switch {
    pub part: Option<u32>,
    pub default: Option<SwitchElement>,
    pub options: Vec<(u32, u32, Option<SwitchElement>)>,
}

impl Switch {
    /// What the switch shows with these keys set.
    pub fn pick(&self, keys: &std::collections::HashSet<u32>) -> Option<&SwitchElement> {
        let hit = |o: &&(u32, u32, Option<SwitchElement>)| keys.contains(&o.0) && (o.1 == 0 || keys.contains(&o.1));
        match self.options.iter().filter(hit).max_by_key(|o| o.1 != 0) {
            Some(o) => o.2.as_ref(),
            None => self.default.as_ref(),
        }
    }
}

/// A model by asset id, the bone it is modelled around and the stock bones
/// it replaces.
#[derive(Clone)]
pub struct SwitchElement {
    pub asset: u64,
    pub bone: u32,
    pub hides: Vec<u32>,
}

/// A mod part or kit: joaat of its name and slot, and per model the joaat
/// of the bone it is modelled around and of the stock bones it replaces.
#[derive(Clone)]
pub struct Mod {
    pub name: u32,
    pub slot: u32,
    pub elements: Vec<(u32, Vec<u32>)>,
    /// Per element, the joaat of its model's name (0 when not given).
    pub models: Vec<u32>,
    /// Per element, its model's asset id (`meta7a::asset_id` of the name;
    /// 0 when not given: library parts).
    pub assets: Vec<u64>,
}

/// The vehicle table, read once per process (model loading uses it off the
/// UI thread).
pub fn shared(game: &str, work: &str) -> anyhow::Result<Arc<HashMap<String, Vehicle>>> {
    static DB: std::sync::OnceLock<Mutex<Option<Arc<HashMap<String, Vehicle>>>>> = std::sync::OnceLock::new();
    let mut g = DB.get_or_init(|| Mutex::new(None)).lock().unwrap();
    if let Some(d) = &*g {
        return Ok(d.clone());
    }
    let d = Arc::new(load(game, work, &VehicleDb::path(work))?);
    *g = Some(d.clone());
    Ok(d)
}

pub enum State {
    Idle,
    Loading(String),
    Ready(Arc<HashMap<String, Vehicle>>),
    Failed(String),
}

pub struct VehicleDb {
    pub state: Arc<Mutex<State>>,
}

impl VehicleDb {
    pub fn new() -> Self {
        VehicleDb {
            state: Arc::new(Mutex::new(State::Idle)),
        }
    }

    pub fn path(work: &str) -> PathBuf {
        Path::new(work)
            .join("out")
            .join("vehicles")
            .join("vehicles.json")
    }

    /// The table, starting the load (or build) on first call.
    pub fn get(
        &self,
        game: &str,
        work: &str,
        ctx: &egui::Context,
    ) -> Option<Arc<HashMap<String, Vehicle>>> {
        let mut st = self.state.lock().unwrap();
        match &*st {
            State::Ready(m) => return Some(m.clone()),
            State::Loading(_) | State::Failed(_) => return None,
            State::Idle => {}
        }
        let path = Self::path(work);
        *st = State::Loading(if path.exists() && !is_old(&path) {
            "reading vehicle data …".into()
        } else {
            "building vehicle data (sixng vehicles, a few seconds) …".into()
        });
        let (slot, game, work, ctx) = (
            self.state.clone(),
            game.to_string(),
            work.to_string(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let _ = &path;
            let r = shared(&game, &work).map(|d| (*d).clone());
            *slot.lock().unwrap() = match r {
                Ok(m) => State::Ready(Arc::new(m)),
                Err(e) => State::Failed(format!("{e:#}")),
            };
            ctx.request_repaint();
        });
        None
    }

    pub fn status(&self) -> Option<String> {
        match &*self.state.lock().unwrap() {
            State::Loading(s) => Some(s.clone()),
            State::Failed(e) => Some(format!("vehicle data not available: {e}")),
            _ => None,
        }
    }
}

/// A table from before `format` (model info matched by gameName only, names
/// missing where the index had none yet): built again. `format` is the
/// file's first key, so its head is enough.
fn is_old(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 64];
    let Ok(n) = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)) else { return false };
    let h = String::from_utf8_lossy(&head[..n]);
    let v = h.split("\"format\":").nth(1).and_then(|x| x.trim().split(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse::<u32>().ok()));
    v.unwrap_or(0) < sixng_core::vehicles::FORMAT
}

fn load(game: &str, work: &str, path: &Path) -> anyhow::Result<HashMap<String, Vehicle>> {
    let old = is_old(path);
    if !path.exists() || old {
        let ws = sixng_core::export::Workspace::new(game, work);
        sixng_core::vehicles::build(&ws, &Path::new(work).join("out"))?;
    }
    let f: File = serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path)?))?;
    let mut m = HashMap::new();
    for v in f.vehicles {
        // unnamed models go in under their name hash (see `find`)
        let Some(name) = v.model.or(v.model_hash) else { continue };
        let configs = v
            .configurations
            .into_iter()
            .map(|c| Config {
                rim_ratio: c.rim_ratio,
                parts_key: c.parts_key,
                light_patterns: c.light_patterns,
                // a name where it is known, else the hash as 8 hex digits
                parts: c.parts.iter().map(|p| if p.len() == 8 && p.bytes().all(|b| b.is_ascii_hexdigit()) && !p.contains('_') { u32::from_str_radix(p, 16).unwrap_or(0) } else { sixng_core::meta7a::joaat(p) }).collect(),
                // pure magenta is stored where a configuration has no body
                // colour of its own (aircraft, some trailers)
                colour: c.colour.and_then(|c| hex(&c.srgb_if_linear)).filter(|c| !is_placeholder(*c)),
                rim: c.rim,
                tyre: c.tyre,
                brake_disc: c.parts.iter().find(|p| p.starts_with("brakedisc_")).cloned(),
                caliper: c.parts.iter().find(|p| p.starts_with("caliper_")).cloned(),
            })
            .collect();
        let palettes = v
            .palettes
            .into_iter()
            .filter(|p| !p.is_empty())
            .map(|p| {
                let mut n: Vec<(String, usize)> = Vec::new();
                for c in &p {
                    match n.iter_mut().find(|x| x.0 == c.srgb_if_linear) {
                        Some(x) => x.1 += 1,
                        None => n.push((c.srgb_if_linear.clone(), 1)),
                    }
                }
                n.sort_by(|a, b| b.1.cmp(&a.1));
                n.iter()
                    .filter_map(|(h, k)| hex(h).map(|c| (c, *k as f32 / p.len() as f32)))
                    .collect()
            })
            .collect();
        let display = v.display_name.as_ref().map(|n| match &v.make {
            Some(mk) if !n.to_lowercase().starts_with(&mk.to_lowercase()) => format!("{mk} {n}"),
            _ => n.clone(),
        });
        let (ws, wr, sm) = v
            .model_info
            .map(|i| (i.wheel_scale, i.wheel_scale_rear, i.steer_wheel_mult))
            .unwrap_or((None, None, None));
        m.insert(
            name.to_ascii_lowercase(),
            Vehicle {
                wheel_scale: ws,
                wheel_scale_rear: wr,
                steer_wheel_mult: sm,
                steer_lock: v.steering_lock,
                lights: v.lights.map(|l| {
                    let hex = |s: &str| u32::from_str_radix(s, 16).unwrap_or(0);
                    std::sync::Arc::new(Lights {
                        patterns: l.patterns.into_iter().map(|(name, p)| LightPattern { name, slot: p.slot, speed: p.speed, lamps: p.lamps.into_iter().map(|x| (hex(&x.bone), x.steps)).collect() }).collect(),
                        pack: l.pack,
                        archetypes: l.archetypes.into_iter().map(|(k, v)| (hex(&k), v)).collect(),
                        lamp_lights: l
                            .lamp_lights
                            .into_iter()
                            .filter(|(_, (c, _))| c.len() == 6)
                            .map(|(k, (c, i))| (hex(&k), (std::array::from_fn(|n| u8::from_str_radix(&c[n * 2..n * 2 + 2], 16).unwrap_or(255) as f32 / 255.0), i)))
                            .collect(),
                    })
                }),
                display,
                configs,
                palettes,
                mods: v
                    .mods
                    .iter()
                    .map(|m| {
                        let h = |s: &str| u32::from_str_radix(s, 16).unwrap_or(0);
                        Mod { name: h(&m.name), slot: h(&m.slot), assets: m.elements.iter().map(|e| e.asset.as_deref().and_then(|a| u64::from_str_radix(a, 16).ok()).unwrap_or(0)).collect(), models: m.elements.iter().map(|e| e.model.as_deref().map_or(0, h)).collect(), elements: m.elements.iter().map(|e| (h(&e.bone), e.hides.iter().map(|x| h(x)).collect())).collect() }
                    })
                    .collect(),
                switches: {
                    let h = |s: &str| u32::from_str_radix(s, 16).unwrap_or(0);
                    let el = |e: &ElemRec| e.asset.as_deref().and_then(|a| u64::from_str_radix(a, 16).ok()).map(|asset| SwitchElement { asset, bone: h(&e.bone), hides: e.hides.iter().map(|x| h(x)).collect() });
                    v.switches.iter().map(|s| Switch { part: s.part.as_deref().map(h), default: s.default.as_ref().and_then(el), options: s.options.iter().map(|o| (h(&o.key), o.also.as_deref().map_or(0, h), o.element.as_ref().and_then(el))).collect() }).collect()
                },
                wheel_fits: v.wheel_fits.iter().map(|(p, f)| (u32::from_str_radix(p, 16).unwrap_or(0), f.clone())).collect(),
                part_keys: v.part_keys.iter().map(|(p, k)| { let h = |s: &str| u32::from_str_radix(s, 16).unwrap_or(0); (h(p), k.iter().map(|k| h(k)).collect()) }).collect(),
            },
        );
    }
    Ok(m)
}

/// The vehicle of a model key: by name, else by the joaat of the name.
pub fn find<'a>(db: &'a HashMap<String, Vehicle>, key: &str) -> Option<&'a Vehicle> {
    db.get(key)
        .or_else(|| db.get(&format!("{:08x}", sixng_core::meta7a::joaat(key))))
}

/// Model name of a loaded source string (`packs.rpf@… phoenix_hi` -> `phoenix`).
pub fn model_key(source: &str) -> String {
    // a model shown with mod parts ends in " (+3 parts)"
    let source = source.split(" (+").next().unwrap_or(source);
    let n = source
        .split_whitespace()
        .last()
        .unwrap_or("")
        .to_ascii_lowercase();
    let n = n
        .rsplit(|c| c == '/' || c == char::from(92u8))
        .next()
        .unwrap_or(&n)
        .split('.')
        .next()
        .unwrap_or("")
        .to_string();
    for s in ["_hi", "_mid", "_lo"] {
        if let Some(b) = n.strip_suffix(s) {
            return b.to_string();
        }
    }
    n
}
