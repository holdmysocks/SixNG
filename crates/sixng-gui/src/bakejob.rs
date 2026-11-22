//! "Bake map" from inside the app: runs `sixng-gui --bake-map` as a child
//! process (so a failure or out-of-memory cannot take the app down), shows
//! progress from `<work>/map/bake.log`, and tells the map tabs to reload
//! when it finishes.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Instant;

/// World data the bake reads, relative to `<work>/out` (see mapbake.rs),
/// with the World-data step that produces it. Placements are required; the
/// rest only fill layers (heights, roads, lights ...).
const INPUTS: &[(&str, &str, bool)] = &[
    ("placements/levels/index.jsonl", "Placements", true),
    ("collision/levels", "Collision", false),
    ("navmesh/navmeshes", "Road navmesh", false),
    ("paths/levels/graph.json", "Lane graph", false),
    ("lights/levels", "Lights", false),
    ("placements/levels/interiors.jsonl", "Interiors", false),
    ("placements/levels/batches.jsonl", "Batches", false),
];

pub struct BakeJob {
    child: Option<Child>,
    started: Instant,
    log_start: u64,
    pub last_line: String,
    /// Some(true) = finished OK, Some(false) = failed or cancelled; taken by `poll`'s caller.
    finished: Option<bool>,
    pub result: Option<String>,
}

impl Default for BakeJob {
    fn default() -> Self {
        BakeJob {
            child: None,
            started: Instant::now(),
            log_start: 0,
            last_line: String::new(),
            finished: None,
            result: None,
        }
    }
}

fn log_path(work: &str) -> PathBuf {
    Path::new(work).join("map").join("bake.log")
}

impl BakeJob {
    pub fn running(&self) -> bool {
        self.child.is_some()
    }

    /// (missing required, missing optional) inputs, as World-data step names.
    pub fn missing_inputs(work: &str) -> (Vec<&'static str>, Vec<&'static str>) {
        let w = Path::new(work);
        let full = w
            .join("placements_full")
            .join("placements")
            .join("levels")
            .join("index.jsonl")
            .exists();
        let mut req = Vec::new();
        let mut opt = Vec::new();
        for (rel, step, required) in INPUTS {
            if *required && full {
                continue;
            }
            if !w.join("out").join(rel).exists() {
                if *required {
                    req.push(*step)
                } else {
                    opt.push(*step)
                }
            }
        }
        (req, opt)
    }

    pub fn start(&mut self, game: &str, work: &str) -> Result<(), String> {
        if self.running() {
            return Ok(());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        self.log_start = std::fs::metadata(log_path(work))
            .map(|m| m.len())
            .unwrap_or(0);
        let child = Command::new(exe)
            .arg("--bake-map")
            .env("SIXNG_GAME", game)
            .env("SIXNG_WORK", work)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("could not start the bake: {e}"))?;
        self.child = Some(child);
        self.started = Instant::now();
        self.last_line = "starting …".into();
        self.result = None;
        Ok(())
    }

    pub fn cancel(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
            self.result = Some("Bake cancelled. The previous map files may be partly overwritten; bake again before using the map.".into());
            self.finished = Some(false);
        }
    }

    /// Update progress; returns Some(ok) once when the bake has just ended.
    pub fn poll(&mut self, work: &str) -> Option<bool> {
        if let Some(c) = self.child.as_mut() {
            // newest line written by this run
            if let Ok(b) = std::fs::read(log_path(work)) {
                let from = (self.log_start as usize).min(b.len());
                let text = String::from_utf8_lossy(&b[from..]);
                if let Some(l) = text.lines().rev().find(|l| !l.trim().is_empty()) {
                    self.last_line = l.trim().to_string();
                }
            }
            match c.try_wait() {
                Ok(Some(st)) => {
                    self.child = None;
                    let ok = st.success();
                    let secs = self.started.elapsed().as_secs();
                    self.result = Some(if ok {
                        format!("Map baked in {}m {:02}s.", secs / 60, secs % 60)
                    } else {
                        format!(
                            "Bake failed: {} (see {})",
                            self.last_line,
                            log_path(work).display()
                        )
                    });
                    self.finished = Some(ok);
                }
                Ok(None) => {}
                Err(e) => {
                    self.child = None;
                    self.result = Some(format!("Bake process error: {e}"));
                    self.finished = Some(false);
                }
            }
        }
        self.finished.take()
    }

    pub fn elapsed(&self) -> u64 {
        self.started.elapsed().as_secs()
    }
}

impl Drop for BakeJob {
    fn drop(&mut self) {
        // don't leave an orphaned bake writing the map files after the app closes
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
        }
    }
}
