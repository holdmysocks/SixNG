//! Jobs: every `sixng` CLI command as a background job, with a form built from
//! the command's own `--help`, a queue (one job runs at a time), progress from
//! the CLI's JSON progress events, a readable log, the result and Cancel.
//!
//! Jobs run `sixng.exe` from the folder of this exe as a child process, so a
//! failure or out-of-memory cannot take the app down.

use eframe::egui;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Options every command has; set from the app's folders, not shown in forms.
const GLOBAL: &[&str] = &["game", "work", "quiet", "pretty", "help", "version"];

#[derive(Clone, Debug)]
pub struct Field {
    /// long option name (without --), or the positional's value name
    pub name: String,
    pub value: Option<String>,
    pub desc: String,
    pub default: Option<String>,
    pub positional: bool,
    pub required: bool,
    pub multiple: bool,
}

#[derive(Clone, Debug)]
pub struct CmdSpec {
    pub name: String,
    pub about: String,
    pub usage: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl State {
    pub fn finished(&self) -> bool {
        matches!(self, State::Done | State::Failed | State::Cancelled)
    }
    pub fn label(&self) -> &'static str {
        match self {
            State::Queued => "queued",
            State::Running => "running",
            State::Done => "✔ done",
            State::Failed => "✖ failed",
            State::Cancelled => "cancelled",
        }
    }
}

#[derive(Default)]
struct Io {
    stage: String,
    done: u64,
    total: u64,
    pct: f32,
    log: VecDeque<String>,
    stdout: String,
}

pub struct Job {
    pub id: u32,
    pub label: String,
    pub args: Vec<String>,
    pub state: State,
    child: Option<Child>,
    io: Arc<Mutex<Io>>,
    started: Option<Instant>,
    pub seconds: f32,
    pub result: Option<serde_json::Value>,
    pub out_dir: Option<PathBuf>,
}

impl Job {
    pub fn progress(&self) -> (String, f32, u64, u64) {
        let io = self.io.lock().unwrap();
        (io.stage.clone(), io.pct, io.done, io.total)
    }
    pub fn log_lines(&self) -> Vec<String> {
        self.io.lock().unwrap().log.iter().cloned().collect()
    }
}

pub struct Jobs {
    game: String,
    work: String,
    cli: PathBuf,
    pub jobs: Vec<Job>,
    next_id: u32,
    catalog: Option<Result<Vec<CmdSpec>, String>>,
    /// The command list loads in the background (one `--help` call per command).
    catalog_pending: Option<std::sync::Arc<std::sync::Mutex<Option<Result<Vec<CmdSpec>, String>>>>>,
    sel_cmd: Option<usize>,
    form: Vec<String>,
    form_flags: Vec<bool>,
    sel_job: Option<u32>,
    /// ids of jobs that finished since the last `take_finished`
    finished: Vec<(u32, bool)>,
}

fn cli_path() -> PathBuf {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default();
    dir.join(if cfg!(windows) { "sixng.exe" } else { "sixng" })
}

/// Parse `sixng --help` (the "Commands:" list).
fn parse_commands(help: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut on = false;
    for l in help.lines() {
        if l.starts_with("Commands:") {
            on = true;
            continue;
        }
        if on {
            if l.trim().is_empty() || !l.starts_with("  ") {
                if !out.is_empty() {
                    break;
                }
                continue;
            }
            let t = l.trim_start();
            let (name, about) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
            if name != "help" {
                out.push((name.to_string(), about.trim().to_string()));
            }
        }
    }
    out
}

/// Parse `sixng <cmd> --help`: usage line, positionals and options.
fn parse_command(name: &str, help: &str) -> CmdSpec {
    let mut fields = Vec::new();
    let mut about = String::new();
    let mut usage = String::new();
    let mut section = "";
    for l in help.lines() {
        if about.is_empty()
            && !l.trim().is_empty()
            && section.is_empty()
            && !l.starts_with("Usage:")
        {
            about = l.trim().to_string();
            continue;
        }
        if let Some(u) = l.strip_prefix("Usage:") {
            usage = u.trim().to_string();
            continue;
        }
        if l.starts_with("Arguments:") {
            section = "args";
            continue;
        }
        if l.starts_with("Options:") {
            section = "opts";
            continue;
        }
        if !l.starts_with("  ") {
            continue;
        }
        let t = l.trim_start();
        // split "<head>   <desc>" on the first run of 2+ spaces
        let (head, desc) = match t.find("  ") {
            Some(p) => (t[..p].trim(), t[p..].trim()),
            None => (t, ""),
        };
        let default = desc
            .find("[default: ")
            .map(|p| desc[p + 10..].split(']').next().unwrap_or("").to_string());
        let desc_clean = desc
            .split(" [default:")
            .next()
            .unwrap_or(desc)
            .split(" [env:")
            .next()
            .unwrap_or(desc)
            .to_string();
        if section == "args" {
            let multiple = head.ends_with("...");
            let req = head.starts_with('<');
            let n = head
                .trim_end_matches("...")
                .trim_matches(|c| c == '[' || c == ']' || c == '<' || c == '>')
                .to_string();
            fields.push(Field {
                name: n.to_ascii_lowercase(),
                value: Some(n),
                desc: desc_clean,
                default,
                positional: true,
                required: req,
                multiple,
            });
        } else if section == "opts" {
            // "-o, --out <OUT>" or "--force" or "--id <ID>..."
            let long = head.split(", ").last().unwrap_or(head);
            let Some(rest) = long.strip_prefix("--") else {
                continue;
            };
            let (n, v) = match rest.split_once(' ') {
                Some((n, v)) => (
                    n.to_string(),
                    Some(
                        v.trim_matches(|c| c == '<' || c == '>' || c == '.')
                            .to_string(),
                    ),
                ),
                None => (rest.to_string(), None),
            };
            if GLOBAL.contains(&n.as_str()) {
                continue;
            }
            let required =
                usage.contains(&format!("--{n} <")) && !usage.contains(&format!("[--{n}"));
            fields.push(Field {
                name: n,
                value: v,
                desc: desc_clean,
                default,
                positional: false,
                required,
                multiple: head.ends_with("..."),
            });
        }
    }
    CmdSpec {
        name: name.to_string(),
        about,
        usage,
        fields,
    }
}

fn load_catalog(cli: &Path) -> Result<Vec<CmdSpec>, String> {
    let run = |args: &[&str]| -> Result<String, String> {
        let mut cmd = Command::new(cli);
        cmd.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let o = cmd
            .output()
            .map_err(|e| format!("cannot run {}: {e}", cli.display()))?;
        Ok(String::from_utf8_lossy(&o.stdout).to_string())
    };
    let top = run(&["--help"])?;
    // one process per command, run side by side
    let names: Vec<String> = parse_commands(&top).into_iter().map(|(n, _)| n).collect();
    let helps: Vec<Result<String, String>> = std::thread::scope(|s| {
        let run = &run;
        let hs: Vec<_> = names
            .iter()
            .map(|n| s.spawn(move || run(&[n.as_str(), "--help"])))
            .collect();
        hs.into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Err("help call crashed".into())))
            .collect()
    });
    let mut v = Vec::new();
    for (n, h) in names.iter().zip(helps) {
        v.push(parse_command(n, &h?));
    }
    if v.is_empty() {
        return Err("no commands found in `sixng --help`".into());
    }
    Ok(v)
}

/// Split a form value into arguments: quoted strings stay whole.
fn split_values(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut q = false;
    for c in s.chars() {
        match c {
            '"' => q = !q,
            c if c.is_whitespace() && !q => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

impl Jobs {
    pub fn new(game: &str, work: &str) -> Self {
        Jobs {
            game: game.into(),
            work: work.into(),
            cli: cli_path(),
            jobs: Vec::new(),
            next_id: 1,
            catalog: None,
            catalog_pending: None,
            sel_cmd: None,
            form: Vec::new(),
            form_flags: Vec::new(),
            sel_job: None,
            finished: Vec::new(),
        }
    }

    pub fn set_folders(&mut self, game: &str, work: &str) {
        self.game = game.into();
        self.work = work.into();
    }

    pub fn cli_found(&self) -> bool {
        self.cli.exists()
    }

    /// Queue `sixng <args>`; returns the job id.
    pub fn enqueue(&mut self, label: &str, args: Vec<String>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let out_dir = args
            .iter()
            .position(|a| a == "--out" || a == "-o")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from);
        self.jobs.push(Job {
            id,
            label: label.into(),
            args,
            state: State::Queued,
            child: None,
            io: Arc::new(Mutex::new(Io::default())),
            started: None,
            seconds: 0.0,
            result: None,
            out_dir,
        });
        id
    }

    pub fn state_of(&self, id: u32) -> Option<&Job> {
        self.jobs.iter().find(|j| j.id == id)
    }

    pub fn running(&self) -> bool {
        self.jobs.iter().any(|j| j.state == State::Running)
    }

    pub fn active_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|j| matches!(j.state, State::Running | State::Queued))
            .count()
    }

    /// Jobs that ended since the last call: (id, ok).
    pub fn take_finished(&mut self) -> Vec<(u32, bool)> {
        std::mem::take(&mut self.finished)
    }

    pub fn cancel(&mut self, id: u32) {
        if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
            if let Some(mut c) = j.child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            if matches!(j.state, State::Running | State::Queued) {
                j.state = State::Cancelled;
                self.finished.push((id, false));
            }
        }
    }

    fn start(&mut self, idx: usize) {
        let (game, work, cli) = (self.game.clone(), self.work.clone(), self.cli.clone());
        let j = &mut self.jobs[idx];
        let mut cmd = Command::new(&cli);
        cmd.args(&j.args)
            .env("SIXNG_GAME", &game)
            .env("SIXNG_WORK", &work)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        match cmd.spawn() {
            Ok(mut child) => {
                let io = j.io.clone();
                if let Some(err) = child.stderr.take() {
                    let io = io.clone();
                    std::thread::spawn(move || {
                        for l in BufReader::new(err).lines().map_while(Result::ok) {
                            let mut g = io.lock().unwrap();
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&l) {
                                if v["event"] == "progress" {
                                    g.stage = v["stage"].as_str().unwrap_or("").to_string();
                                    g.done = v["done"].as_u64().unwrap_or(0);
                                    g.total = v["total"].as_u64().unwrap_or(0);
                                    g.pct = v["pct"].as_f64().unwrap_or(0.0) as f32;
                                    continue;
                                }
                            }
                            g.log.push_back(l);
                            if g.log.len() > 2000 {
                                g.log.pop_front();
                            }
                        }
                    });
                }
                if let Some(out) = child.stdout.take() {
                    std::thread::spawn(move || {
                        for l in BufReader::new(out).lines().map_while(Result::ok) {
                            let mut g = io.lock().unwrap();
                            if g.stdout.len() < 4 << 20 {
                                g.stdout.push_str(&l);
                                g.stdout.push('\n');
                            }
                        }
                    });
                }
                j.child = Some(child);
                j.state = State::Running;
                j.started = Some(Instant::now());
            }
            Err(e) => {
                j.io.lock()
                    .unwrap()
                    .log
                    .push_back(format!("could not start {}: {e}", cli.display()));
                j.state = State::Failed;
                self.finished.push((j.id, false));
            }
        }
    }

    /// Advance the queue; call every frame.
    pub fn poll(&mut self) {
        for j in self.jobs.iter_mut() {
            if j.state != State::Running {
                continue;
            }
            if let Some(s) = j.started {
                j.seconds = s.elapsed().as_secs_f32();
            }
            let Some(c) = j.child.as_mut() else { continue };
            if let Ok(Some(st)) = c.try_wait() {
                j.child = None;
                // give the reader threads a moment to drain the pipes
                std::thread::sleep(std::time::Duration::from_millis(30));
                let out = j.io.lock().unwrap().stdout.clone();
                let last = out
                    .lines()
                    .rev()
                    .find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok());
                let ok = st.success() && last.as_ref().map_or(true, |v| v["ok"] != false);
                j.result = last;
                j.state = if ok { State::Done } else { State::Failed };
                self.finished.push((j.id, ok));
            }
        }
        if !self.running() {
            if let Some(i) = self.jobs.iter().position(|j| j.state == State::Queued) {
                self.start(i);
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if self.running() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
        if !self.cli_found() {
            ui.colored_label(egui::Color32::from_rgb(230, 150, 90), format!("The command-line tool was not found at {}. Jobs need sixng.exe next to sixng-gui.exe.", self.cli.display()));
            return;
        }
        if self.catalog.is_none() {
            // never on the UI thread: ~30 process starts take seconds on a busy disk
            let slot = self.catalog_pending.get_or_insert_with(|| {
                let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
                let (out, cli, ctx) = (slot.clone(), self.cli.clone(), ui.ctx().clone());
                std::thread::spawn(move || {
                    let r = load_catalog(&cli);
                    *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
                    ctx.request_repaint();
                });
                slot
            });
            let done = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
            match done {
                Some(r) => {
                    self.catalog = Some(r);
                    self.catalog_pending = None;
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("reading the command list …");
                    });
                    return;
                }
            }
        }
        let cat = match self.catalog.as_ref().unwrap() {
            Ok(c) => c.clone(),
            Err(e) => {
                ui.colored_label(egui::Color32::from_rgb(230, 150, 90), e);
                return;
            }
        };
        egui::Panel::left("jobs_cmds")
            .resizable(true)
            .default_size(250.0)
            .show(ui, |ui| {
                ui.heading("Commands");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, c) in cat.iter().enumerate() {
                        let r = ui
                            .selectable_label(
                                self.sel_cmd == Some(i),
                                egui::RichText::new(&c.name).strong(),
                            )
                            .on_hover_text(&c.about);
                        ui.label(egui::RichText::new(&c.about).small().weak());
                        if r.clicked() {
                            self.sel_cmd = Some(i);
                            self.form = c
                                .fields
                                .iter()
                                .map(|f| {
                                    if f.value.is_some() {
                                        f.default.clone().unwrap_or_default()
                                    } else {
                                        String::new()
                                    }
                                })
                                .collect();
                            self.form_flags = vec![false; c.fields.len()];
                        }
                    }
                });
            });
        egui::Panel::right("jobs_queue")
            .resizable(true)
            .default_size(420.0)
            .show(ui, |ui| self.queue_ui(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            let Some(ci) = self.sel_cmd else {
                ui.label("Pick a command on the left to fill in its options and queue it. Jobs run one at a time in the background; you can keep using the app.");
                return;
            };
            let c = &cat[ci];
            ui.heading(&c.name);
            ui.label(&c.about).on_hover_text(egui::RichText::new(&c.usage).monospace());
            ui.add_space(6.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("jobform").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    for (k, f) in c.fields.iter().enumerate() {
                        let label = format!("{}{}", f.name, if f.required { " *" } else { "" });
                        ui.label(egui::RichText::new(label).strong()).on_hover_text(&f.desc);
                        ui.vertical(|ui| {
                            if f.value.is_some() {
                                ui.horizontal(|ui| {
                                    ui.add(egui::TextEdit::singleline(&mut self.form[k]).desired_width(320.0).hint_text(f.value.clone().unwrap_or_default()));
                                    let pathish = ["out", "file", "dir", "input", "output", "placements"].iter().any(|p| f.name.contains(p));
                                    if pathish && ui.small_button("…").clicked() {
                                        let dlg = rfd::FileDialog::new();
                                        let p = if f.name.contains("file") || f.name == "input" { dlg.pick_file() } else { dlg.pick_folder() };
                                        if let Some(p) = p {
                                            self.form[k] = p.display().to_string();
                                        }
                                    }
                                });
                            } else {
                                ui.checkbox(&mut self.form_flags[k], "");
                            }
                            if !f.desc.is_empty() {
                                ui.label(egui::RichText::new(&f.desc).small().weak());
                            }
                        });
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                let missing: Vec<&str> = c.fields.iter().enumerate().filter(|(k, f)| f.required && f.value.is_some() && self.form[*k].trim().is_empty()).map(|(_, f)| f.name.as_str()).collect();
                let args = self.form_args(c);
                ui.label(egui::RichText::new(format!("sixng {}", args.join(" "))).monospace().small());
                ui.add_enabled_ui(missing.is_empty(), |ui| {
                    if ui.button("▶ Queue job").clicked() {
                        let id = self.enqueue(&c.name, args.clone());
                        self.sel_job = Some(id);
                    }
                });
                if !missing.is_empty() {
                    ui.label(egui::RichText::new(format!("Fill in: {}", missing.join(", "))).small());
                }
            });
        });
    }

    fn form_args(&self, c: &CmdSpec) -> Vec<String> {
        let mut a = vec![c.name.clone()];
        let mut pos = Vec::new();
        for (k, f) in c.fields.iter().enumerate() {
            if f.value.is_none() {
                if self.form_flags[k] {
                    a.push(format!("--{}", f.name));
                }
                continue;
            }
            let v = self.form[k].trim();
            if v.is_empty() || (f.default.as_deref() == Some(v) && !f.required) {
                continue;
            }
            if f.positional {
                pos.extend(split_values(v));
            } else if f.multiple {
                for x in split_values(v) {
                    a.push(format!("--{}", f.name));
                    a.push(x);
                }
            } else {
                a.push(format!("--{}", f.name));
                a.push(v.trim_matches('"').to_string());
            }
        }
        a.extend(pos);
        a
    }

    fn queue_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Queue");
            if ui.small_button("Clear finished").clicked() {
                self.jobs
                    .retain(|j| matches!(j.state, State::Running | State::Queued));
            }
        });
        let mut cancel = None;
        egui::ScrollArea::vertical()
            .id_salt("jobq")
            .max_height(ui.available_height() * 0.45)
            .show(ui, |ui| {
                for j in self.jobs.iter().rev() {
                    let (stage, pct, done, total) = j.progress();
                    let col = match j.state {
                        State::Done => egui::Color32::from_rgb(120, 200, 120),
                        State::Failed => egui::Color32::from_rgb(230, 120, 90),
                        State::Cancelled => egui::Color32::GRAY,
                        _ => ui.visuals().text_color(),
                    };
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                self.sel_job == Some(j.id),
                                egui::RichText::new(format!("#{} {}", j.id, j.label)).color(col),
                            )
                            .clicked()
                        {
                            self.sel_job = Some(j.id);
                        }
                        ui.label(
                            egui::RichText::new(format!("{:?} · {:.0}s", j.state, j.seconds))
                                .small(),
                        );
                        if matches!(j.state, State::Running | State::Queued)
                            && ui.small_button("Cancel").clicked()
                        {
                            cancel = Some(j.id);
                        }
                    });
                    if j.state == State::Running {
                        let frac = if total > 0 { pct / 100.0 } else { 0.0 };
                        ui.add(
                            egui::ProgressBar::new(frac)
                                .text(if total > 0 {
                                    format!("{stage} {done}/{total} ({pct:.1} %)")
                                } else {
                                    "starting …".into()
                                })
                                .animate(true),
                        );
                    }
                }
            });
        if let Some(id) = cancel {
            self.cancel(id);
        }
        ui.separator();
        let Some(j) = self
            .sel_job
            .and_then(|id| self.jobs.iter().find(|j| j.id == id))
        else {
            ui.label("Select a job to see its log and result.");
            return;
        };
        ui.label(
            egui::RichText::new(format!("sixng {}", j.args.join(" ")))
                .monospace()
                .small(),
        );
        if let Some(d) = &j.out_dir {
            if ui.button("📂 Open output folder").clicked() {
                let p = if d.is_absolute() {
                    d.clone()
                } else {
                    std::env::current_dir().unwrap_or_default().join(d)
                };
                crate::open_path(&p);
            }
        }
        if let Some(r) = &j.result {
            ui.collapsing("Result", |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("jobres")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(
                                serde_json::to_string_pretty(r).unwrap_or_default(),
                            )
                            .monospace()
                            .small(),
                        );
                    });
            });
        }
        ui.label("Log");
        egui::ScrollArea::vertical()
            .id_salt("joblog")
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for l in j.log_lines() {
                    ui.label(egui::RichText::new(l).monospace().small());
                }
            });
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        for j in self.jobs.iter_mut() {
            if let Some(mut c) = j.child.take() {
                let _ = c.kill();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_help() {
        let top = "x\n\nUsage: sixng [OPTIONS] <COMMAND>\n\nCommands:\n  info     Show archives\n  index    Build indexes\n  help     Print this\n\nOptions:\n";
        assert_eq!(
            parse_commands(top)
                .iter()
                .map(|x| x.0.as_str())
                .collect::<Vec<_>>(),
            vec!["info", "index"]
        );
        let h = "Decode things\n\nUsage: sixng.exe placements [OPTIONS] --out <OUT> [ARCHIVE]\n\nArguments:\n  [ARCHIVE]  Archive (default levels) [default: levels]\n\nOptions:\n      --game <GAME>  root\n      --id <ID>...  ids\n  -o, --out <OUT>  Output directory\n      --batches  Also batches\n";
        let c = parse_command("placements", h);
        assert_eq!(c.about, "Decode things");
        let n: Vec<_> = c
            .fields
            .iter()
            .map(|f| (f.name.as_str(), f.required, f.positional))
            .collect();
        assert_eq!(
            n,
            vec![
                ("archive", false, true),
                ("id", false, false),
                ("out", true, false),
                ("batches", false, false)
            ]
        );
        assert_eq!(c.fields[0].default.as_deref(), Some("levels"));
        assert_eq!(split_values("a \"b c\" d"), vec!["a", "b c", "d"]);
    }
}
