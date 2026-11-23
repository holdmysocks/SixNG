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
    model: Option<String>,
    #[serde(default)]
    model_hash: Option<String>,
    #[serde(default)]
    model_info: Option<Info>,
    #[serde(default)]
    configurations: Vec<Cfg>,
    #[serde(default)]
    palettes: Vec<Vec<Colour>>,
}

#[derive(Deserialize)]
struct Info {
    #[serde(rename = "wheelScale")]
    wheel_scale: Option<f32>,
    #[serde(rename = "wheelScaleRear")]
    wheel_scale_rear: Option<f32>,
}

#[derive(Deserialize)]
struct Cfg {
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
}

#[derive(Clone, Default)]
pub struct Vehicle {
    pub wheel_scale: Option<f32>,
    pub wheel_scale_rear: Option<f32>,
    pub configs: Vec<Config>,
    /// Paint palettes as (colour, share 0..1), most frequent first. The
    /// stored lists repeat colours (e.g. 400 entries, 10 distinct), so they
    /// read as weighted draw tables.
    pub palettes: Vec<Vec<([f32; 3], f32)>>,
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
            let r = load(&game, &work, &path);
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
                colour: c.colour.and_then(|c| hex(&c.srgb_if_linear)),
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
        let (ws, wr) = v
            .model_info
            .map(|i| (i.wheel_scale, i.wheel_scale_rear))
            .unwrap_or((None, None));
        m.insert(
            name.to_ascii_lowercase(),
            Vehicle {
                wheel_scale: ws,
                wheel_scale_rear: wr,
                configs,
                palettes,
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
