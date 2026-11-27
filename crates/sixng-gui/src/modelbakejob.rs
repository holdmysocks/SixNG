//! "Bake model cache" from inside the app: runs `sixng-gui --bake-model-cache`
//! as a child process (so a failure or out-of-memory cannot take the app down),
//! and shows progress from `<work>/model_cache_bake.log`. Gated to macOS +
//! `metal4-poc`, matching the CLI subcommand and the Metal 4 renderer.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

pub struct ModelBakeJob {
    child: Option<Child>,
    started: Instant,
    log_start: u64,
    pub last_line: String,
    /// Some(true) = finished OK, Some(false) = failed or cancelled; taken by `poll`'s caller.
    finished: Option<bool>,
    pub result: Option<String>,
}

impl Default for ModelBakeJob {
    fn default() -> Self {
        ModelBakeJob {
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
    Path::new(work).join("model_cache_bake.log")
}

impl ModelBakeJob {
    pub fn running(&self) -> bool {
        self.child.is_some()
    }

    pub fn start(&mut self, work: &str) -> Result<(), String> {
        if self.running() {
            return Ok(());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let log = log_path(work);
        // 只读取本次运行新写入的日志行。
        self.log_start = std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
        let child = Command::new(exe)
            .arg("--bake-model-cache")
            .arg("--bake-log")
            .arg(&log)
            .env("SIXNG_WORK", work)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not start the model cache bake: {e}"))?;
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
            self.result = Some(
                "Model cache bake cancelled. Partially written cache entries are safe: they are \
                 validated by CRC on load and will simply be treated as a miss."
                    .into(),
            );
            self.finished = Some(false);
        }
    }

    /// Update progress; returns Some(ok) once when the bake has just ended.
    pub fn poll(&mut self, work: &str) -> Option<bool> {
        if let Some(c) = self.child.as_mut() {
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
                        format!("Model cache baked in {}m {:02}s.", secs / 60, secs % 60)
                    } else {
                        format!(
                            "Model cache bake failed: {} (see {})",
                            self.last_line,
                            log_path(work).display()
                        )
                    });
                    self.finished = Some(ok);
                }
                Ok(None) => {}
                Err(e) => {
                    self.child = None;
                    self.result = Some(format!("Model cache bake process error: {e}"));
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

impl Drop for ModelBakeJob {
    fn drop(&mut self) {
        // don't leave an orphaned bake writing caches after the app closes
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
        }
    }
}
