//! sixng-gui: manual browser / extractor over sixng-core.
#![windows_subsystem = "windows"]
use eframe::egui;
use sixng_core::classify::Kind;
use sixng_core::export::{self, ExportOptions, Filter, Progress, Workspace};
use sixng_core::game::ArchiveInfo;
use sixng_core::resource::Resource;
use sixng_core::scan::{ArchiveIndex, Entry};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod assemble;
mod bc;
mod cells;
mod glb;
mod map2d;
mod map3d;
mod map_cells;
mod mem;
mod world3d;

/// Export cache for the viewers. Bumped when the export layout changes
/// (v2, 2026-11-20: LOD levels from the main LOD group; parts split out;
/// v3, 2026-11-20: normals without a stored normal rebuilt from the faces;
/// v4, 2026-11-20: alpha-bucket cloth with a constant colour alpha drawn opaque).
pub const GUI_CACHE: &str = "gui_cache_v4";
mod audio_view;
mod bakejob;
mod jobs;
mod library;
mod mapbake;
#[cfg(all(target_os = "macos", feature = "metal4-poc"))]
mod metal4_app;
#[cfg(all(target_os = "macos", feature = "metal4-poc"))]
mod metal4_egui;
mod model_picker;
mod model_view;
mod pages;
mod render3d;
#[cfg(all(target_os = "macos", feature = "metal4-poc"))]
mod render3d_metal4;
mod texture_view;
mod thumbs;
mod vehicle_db;

/// Command-line automation (verification screenshots):
/// `sixng-gui --model <id|regex|glb> [--lod N] [--mode N] [--slot N] [--rma N] [--wire] [--skeleton]
///  [--tod H] [--paint r,g,b] [--finish N] [--pearl A] [--spin R] [--door R] [--yaw R] [--pitch R] [--zoom F]
///  [--no-tint] [--hair hex,hex] [--hair-width MM] [--size WxH] --screenshot out.png --exit`
#[derive(Default, Clone)]
struct Cli {
    args: std::collections::HashMap<String, String>,
}
impl Cli {
    fn parse() -> Self {
        let mut args = std::collections::HashMap::new();
        let v: Vec<String> = std::env::args().skip(1).collect();
        let mut i = 0;
        while i < v.len() {
            if let Some(k) = v[i].strip_prefix("--") {
                if i + 1 < v.len() && !v[i + 1].starts_with("--") {
                    args.insert(k.to_string(), v[i + 1].clone());
                    i += 2;
                    continue;
                }
                args.insert(k.to_string(), String::new());
            }
            i += 1;
        }
        Cli { args }
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.args.get(k).map(|s| s.as_str())
    }
    fn f(&self, k: &str) -> Option<f32> {
        self.get(k).and_then(|s| s.parse().ok())
    }
    fn u(&self, k: &str) -> Option<u32> {
        self.get(k).and_then(|s| s.parse().ok())
    }
    fn has(&self, k: &str) -> bool {
        self.args.contains_key(k)
    }
}

/// App settings (game and work folders); see `settings_path`.
#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
#[serde(default)]
struct AppSettings {
    game: Option<String>,
    work: Option<String>,
    /// export folder (Export / extract jobs)
    out: Option<String>,
    thumb_budget_mb: Option<u64>,
    /// the first-run setup was finished or skipped
    setup_done: bool,
}

/// The exe's folder for portable installs; None inside a macOS .app bundle,
/// which must not be written to (settings and work go to the user folder).
fn portable_dir() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    (!dir.ends_with("Contents/MacOS")).then_some(dir)
}

/// `sixng-settings.json` next to the exe (portable installs), else in the
/// per-user settings folder (`sixng_core::user_settings_dir`).
fn settings_path() -> Option<PathBuf> {
    let local = portable_dir().map(|d| d.join("sixng-settings.json"));
    if let Some(l) = &local {
        if l.exists() {
            return local;
        }
    }
    let user = sixng_core::user_settings_dir().map(|d| d.join("settings.json"));
    user.clone().filter(|p| p.exists()).or(local).or(user)
}

fn load_settings() -> AppSettings {
    settings_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_settings(s: &AppSettings) -> std::io::Result<()> {
    let p = settings_path().ok_or_else(|| std::io::Error::other("no settings location"))?;
    let data = serde_json::to_vec_pretty(s).unwrap();
    match std::fs::write(&p, &data) {
        Ok(()) => Ok(()),
        Err(_) => {
            // exe folder not writable: use the per-user settings folder
            let p = sixng_core::user_settings_dir()
                .ok_or_else(|| std::io::Error::other("no per-user settings folder"))?
                .join("settings.json");
            std::fs::create_dir_all(p.parent().unwrap())?;
            std::fs::write(p, data)
        }
    }
}

/// Work folder: $SIXNG_WORK, else the saved setting, else `work` next to the
/// exe (in the user settings folder for a macOS .app).
fn default_work() -> String {
    if let Ok(w) = std::env::var("SIXNG_WORK") {
        return w;
    }
    if let Some(w) = load_settings().work {
        return w;
    }
    portable_dir()
        .or_else(sixng_core::user_settings_dir)
        .map(|d| d.join("work").display().to_string())
        .unwrap_or_else(|| "work".into())
}

/// Game folder: $SIXNG_GAME, else the saved setting, else empty (the app
/// then asks for it on start).
fn default_game() -> String {
    std::env::var("SIXNG_GAME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| load_settings().game)
        .unwrap_or_default()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
    if cli.has("metal4-window") || cli.args.is_empty() {
        match metal4_app::is_available() {
            Ok(true) => return metal4_app::run(App::new(cli)).map_err(Into::into),
            Ok(false) => {
                eprintln!("Metal 4 is unavailable on this Mac; falling back to WGPU.");
            }
            Err(error) => {
                eprintln!("Could not check Metal 4 support ({error:#}); falling back to WGPU.");
            }
        }
    }
    if cli.has("bake-map") {
        let game = default_game();
        let work = default_work();
        let only: Vec<String> = cli
            .get("bake-map")
            .unwrap_or("")
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let b = mapbake::Baker::new(&game, &work);
        if let Err(e) = b.run(&only) {
            mapbake::log(std::path::Path::new(&work), &format!("bake failed: {e:#}"));
            std::process::exit(1);
        }
        return Ok(());
    }
    let size = cli
        .get("size")
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?]))
        .unwrap_or([1400.0, 860.0]);
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_title("SixNG Extractor")
            .with_icon(app_icon()),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: wgpu_options(cli.has("no-vsync")),
        ..Default::default()
    };
    eframe::run_native(
        "SixNG Extractor",
        opts,
        Box::new(move |_cc| Ok(Box::new(App::new(cli)))),
    )?;
    Ok(())
}

/// Default wgpu setup plus BC texture compression where the adapter has it
/// (the 3D map stores its textures block-compressed).
fn wgpu_options(no_vsync: bool) -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::{wgpu, WgpuConfiguration, WgpuSetup};
    let mut o = WgpuConfiguration::default();
    if no_vsync {
        o.surface.present_mode = wgpu::PresentMode::AutoNoVsync;
    }
    if let WgpuSetup::CreateNew(c) = &mut o.wgpu_setup {
        let base = c.device_descriptor.clone();
        c.device_descriptor = std::sync::Arc::new(move |adapter| {
            let mut d = base(adapter);
            d.required_features |= adapter.features()
                & (wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::DEPTH32FLOAT_STENCIL8);
            // smaller device-memory blocks: the 3D map frees and allocates
            // many buffers and textures of mixed sizes while it streams
            d.memory_hints = wgpu::MemoryHints::MemoryUsage;
            d
        });
    }
    o
}

/// Window / taskbar icon (the exe icon itself is embedded by build.rs).
fn app_icon() -> egui::IconData {
    let png = include_bytes!("../../../assets/logo/sixng_icon_256.png");
    match image::load_from_memory(png) {
        Ok(img) => {
            let img = img.to_rgba8();
            let (w, h) = img.dimensions();
            egui::IconData {
                rgba: img.into_raw(),
                width: w,
                height: h,
            }
        }
        Err(_) => egui::IconData::default(),
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Library,
    Jobs,
    Status,
    Settings,
    Browser,
    Model,
    Map2D,
    Map3D,
    Textures,
    Audio,
}

#[derive(Default)]
struct Task {
    label: String,
    done: u64,
    total: u64,
    running: bool,
    message: Option<String>,
}

enum Preview {
    None,
    Text(String),
    Json(String),
    Textures {
        list: Vec<sixng_core::texture::TexInfo>,
        res: Arc<Resource>,
        json: String,
    },
}

struct App {
    game: String,
    work: String,
    out: String,
    archives: Vec<ArchiveInfo>,
    sel_archive: Option<usize>,
    index: Option<Arc<ArchiveIndex>>,
    filter_text: String,
    filter_kind: String,
    filter_rtype: String,
    filtered: Vec<usize>,
    filter_dirty: bool,
    selected: BTreeSet<usize>,
    focus: Option<usize>,
    preview: Preview,
    tex_sel: usize,
    tex_handle: Option<(String, egui::TextureHandle)>,
    tex_pending: Arc<Mutex<Option<(String, Result<(u32, u32, Vec<u8>), String>)>>>,
    task: Arc<Mutex<Task>>,
    loaded_index: Arc<Mutex<Option<ArchiveIndex>>>,
    tex_png: bool,
    tex_dds: bool,
    /// Body shape for ped clothing exports (`gcweightbelly=0.5,...`).
    body_shape: String,
    /// Face variant for ped head exports.
    head_skin: usize,
    tab: Tab,
    model: model_view::ModelView,
    cli: Cli,
    cli_started: bool,
    map2d: map2d::Map2D,
    map3d: map3d::Map3D,
    textures: texture_view::TextureView,
    /// Job id of a `texture-users` job (the Textures tab reloads its index when it ends).
    texture_users_job: Option<u32>,
    audio: audio_view::AudioView,
    shot: Option<(PathBuf, u32, bool)>,
    map3d_start: Option<[f32; 3]>,
    bake: bakejob::BakeJob,
    settings_msg: String,
    library: library::Library,
    jobs: jobs::Jobs,
    setup: pages::Setup,
    status: pages::Status,
    palette: pages::Palette,
    thumb_budget_mb: u64,
    setup_done: bool,
}

impl App {
    fn new(cli: Cli) -> Self {
        let game = default_game();
        let work = default_work();
        let _ = std::fs::create_dir_all(&work);
        // shader / parameter / meta field names (<work>/names/names.json)
        sixng_core::names::set_work_dir(Path::new(&work));
        if !sixng_core::names::dict_path(Path::new(&work)).exists()
            && Path::new(game.trim()).is_dir()
        {
            // built once in the background (needs the common.rpf index; ~2 s)
            let (g, w) = (game.trim().to_string(), work.clone());
            std::thread::spawn(move || {
                let w = Path::new(&w);
                let r = sixng_core::names::build(Path::new(&g), &w.join("index"), w);
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(w.join("gui.log"))
                    .and_then(|mut f| {
                        use std::io::Write;
                        writeln!(
                            f,
                            "[names] {}",
                            match r {
                                Ok(v) => v.to_string(),
                                Err(e) => format!("not built: {e:#}"),
                            }
                        )
                    });
            });
        }
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::path::Path::new(&work).join("gui.log"))
            .and_then(|mut f| {
                use std::io::Write;
                writeln!(
                    f,
                    "[start] work = {work} (SIXNG_WORK {}, settings {:?}, file {:?}: {})",
                    std::env::var("SIXNG_WORK").is_ok(),
                    load_settings().work,
                    settings_path(),
                    settings_path()
                        .map(|p| match std::fs::read(&p) {
                            Ok(b) => match serde_json::from_slice::<AppSettings>(&b) {
                                Ok(_) => "ok".to_string(),
                                Err(e) => format!("parse error {e}"),
                            },
                            Err(e) => format!("read error {e}"),
                        })
                        .unwrap_or_default()
                )
            });
        let st = load_settings();
        let thumb_budget_mb = st.thumb_budget_mb.unwrap_or(512);
        let out = st
            .out
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Path::new(&work).join("out").display().to_string());
        // the wizard opens on first start or when the game folder is gone (not for automation runs)
        let need_game = game.trim().is_empty() || !Path::new(game.trim()).is_dir();
        let setup = pages::Setup {
            open: need_game
                || (!st.setup_done && cli.args.is_empty())
                || cli.get("tab") == Some("setup"),
            ..Default::default()
        };
        let mut a = App {
            library: library::Library::new(&game, &work, thumb_budget_mb),
            jobs: jobs::Jobs::new(&game, &work),
            setup,
            status: pages::Status::default(),
            palette: pages::Palette::default(),
            thumb_budget_mb,
            setup_done: st.setup_done,
            model: model_view::ModelView::new(&game, &work),
            map2d: map2d::Map2D::new(&work),
            map3d: map3d::Map3D::new(&game, &work),
            textures: texture_view::TextureView::new(&game, &work),
            texture_users_job: None,
            audio: audio_view::AudioView::new(&game, &work),
            shot: None,
            map3d_start: None,
            bake: bakejob::BakeJob::default(),
            settings_msg: String::new(),
            tab: Tab::Library,
            cli,
            cli_started: false,
            game,
            work,
            out,
            archives: vec![],
            sel_archive: None,
            index: None,
            filter_text: String::new(),
            filter_kind: String::new(),
            filter_rtype: String::new(),
            filtered: vec![],
            filter_dirty: true,
            selected: BTreeSet::new(),
            focus: None,
            preview: Preview::None,
            tex_sel: 0,
            tex_handle: None,
            tex_pending: Arc::new(Mutex::new(None)),
            task: Arc::new(Mutex::new(Task::default())),
            loaded_index: Arc::new(Mutex::new(None)),
            tex_png: true,
            tex_dds: false,
            body_shape: String::new(),
            head_skin: 0,
        };
        a.map3d.load_settings();
        a.refresh();
        a
    }

    /// Bar above the map tabs: bake / re-bake the map, progress, result.
    fn bake_bar(&mut self, ui: &mut egui::Ui) {
        let baked = Path::new(&self.work).join("map").join("meta.json").exists();
        let (req, opt) = bakejob::BakeJob::missing_inputs(&self.work);
        let ctx = ui.ctx().clone();
        egui::Panel::top("bake_bar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if self.bake.running() {
                    ui.spinner();
                    ui.label(format!("Baking map ({}:{:02}) · {}", self.bake.elapsed() / 60, self.bake.elapsed() % 60, self.bake.last_line));
                    if ui.button("Cancel").clicked() {
                        self.bake.cancel();
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(500));
                    return;
                }
                let label = if baked { "🔄 Re-bake map" } else { "🍞 Bake map" };
                let can = req.is_empty();
                let hover = if can {
                    format!(
                        "Build the map files in {}/map from the exported world data (about 2 minutes, a few GB of RAM). Runs in a separate process; the map tabs reload when it is done.{}",
                        self.work,
                        if opt.is_empty() { String::new() } else { format!("

Optional layers not exported yet (the map works without them): {}.", opt.join(", ")) }
                    )
                } else {
                    format!("Export the world data first: Status page → Export world data, or Archives → World data → {}.", req.join(", "))
                };
                if ui.add_enabled(can, egui::Button::new(label)).on_hover_text(&hover).on_disabled_hover_text(&hover).clicked() {
                    if let Err(e) = self.bake.start(&self.game, &self.work) {
                        self.bake.result = Some(e);
                    }
                }
                if !can {
                    ui.colored_label(egui::Color32::from_rgb(230, 180, 80), format!("Needs world data first: Status page → Export world data, or Archives → World data → {}", req.join(", ")));
                } else if !baked {
                    ui.label("No baked map yet.");
                } else if !opt.is_empty() {
                    ui.weak(format!("missing optional layers: {}", opt.join(", "))).on_hover_text("Export them from Archives → World data (or Jobs), then re-bake to add them.");
                }
                if let Some(r) = &self.bake.result {
                    ui.separator();
                    ui.label(r);
                }
            });
        });
    }

    fn ws(&self) -> Workspace {
        Workspace::new(&self.game, &self.work)
    }

    fn refresh(&mut self) {
        self.archives = self.ws().archives();
    }

    fn busy(&self) -> bool {
        self.task.lock().unwrap().running
    }

    fn start_task<F>(&self, label: &str, ctx: &egui::Context, f: F)
    where
        F: FnOnce(Progress) -> Result<String, String> + Send + 'static,
    {
        {
            let mut t = self.task.lock().unwrap();
            *t = Task {
                label: label.into(),
                running: true,
                ..Default::default()
            };
        }
        let task = self.task.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let t2 = task.clone();
            let c2 = ctx.clone();
            let pr: Progress = Arc::new(move |stage, d, tot| {
                let mut t = t2.lock().unwrap();
                t.done = d;
                t.total = tot;
                t.message = Some(stage.to_string());
                c2.request_repaint();
            });
            let r = f(pr);
            let mut t = task.lock().unwrap();
            t.running = false;
            t.message = Some(match r {
                Ok(m) => m,
                Err(e) => format!("error: {e}"),
            });
            ctx.request_repaint();
        });
    }

    fn open_archive(&mut self, i: usize, ctx: &egui::Context) {
        self.sel_archive = Some(i);
        self.index = None;
        self.selected.clear();
        self.focus = None;
        self.preview = Preview::None;
        let a = self.archives[i].clone();
        let ws = self.ws();
        let slot = self.loaded_index.clone();
        if ws.is_indexed(&a) {
            self.start_task(&format!("Loading index {}", a.id), ctx, move |_| {
                let ix = ws.load_index(&a).map_err(|e| e.to_string())?;
                let n = ix.entries.len();
                *slot.lock().unwrap() = Some(ix);
                Ok(format!("{} entries", n))
            });
        }
    }

    fn index_archive(&mut self, i: usize, ctx: &egui::Context) {
        let a = self.archives[i].clone();
        let ws = self.ws();
        let slot = self.loaded_index.clone();
        self.sel_archive = Some(i);
        self.start_task(&format!("Indexing {}", a.id), ctx, move |pr| {
            let ix = ws
                .ensure_index(&a, false, true, pr)
                .map_err(|e| e.to_string())?;
            let msg = format!(
                "{}: {} entries, {:.1}% coverage",
                a.id,
                ix.entries.len(),
                ix.stats.coverage * 100.0
            );
            *slot.lock().unwrap() = Some(ix);
            Ok(msg)
        });
    }

    fn apply_filter(&mut self) {
        self.filtered.clear();
        let Some(ix) = &self.index else { return };
        let re = regex::RegexBuilder::new(&self.filter_text)
            .case_insensitive(true)
            .build()
            .ok();
        for (i, e) in ix.entries.iter().enumerate() {
            if !self.filter_kind.is_empty() && e.kind.name() != self.filter_kind {
                continue;
            }
            if !self.filter_rtype.is_empty()
                && e.rtype.as_deref() != Some(self.filter_rtype.as_str())
            {
                continue;
            }
            if !self.filter_text.is_empty() {
                let hay = format!("{} {}", e.name.as_deref().unwrap_or(""), e.id);
                let ok = match &re {
                    Some(r) => r.is_match(&hay),
                    None => hay
                        .to_lowercase()
                        .contains(&self.filter_text.to_lowercase()),
                };
                if !ok {
                    continue;
                }
            }
            self.filtered.push(i);
        }
        self.filter_dirty = false;
    }

    fn focus_entry(&mut self, i: usize) {
        self.focus = Some(i);
        self.tex_sel = 0;
        self.tex_handle = None;
        let (Some(ix), Some(ai)) = (&self.index, self.sel_archive) else {
            return;
        };
        let a = &self.archives[ai];
        let e = &ix.entries[i];
        self.preview = match (e.kind, e.rtype.as_deref()) {
            (Kind::RageObj | Kind::Resource, Some("txd")) => match Resource::load(&a.path, e) {
                Ok(r) => {
                    let list = sixng_core::texture::find_textures(&r);
                    let json = serde_json::to_string_pretty(&list).unwrap_or_default();
                    Preview::Textures {
                        list,
                        res: Arc::new(r),
                        json,
                    }
                }
                Err(err) => Preview::Text(format!("error: {err}")),
            },
            (Kind::RageObj | Kind::Resource, _) => match Resource::load(&a.path, e) {
                Ok(r) => {
                    let info = sixng_core::drawable::describe(&r);
                    if !info.embedded_textures.is_empty() {
                        let list = info.embedded_textures.clone();
                        let json = serde_json::to_string_pretty(&info).unwrap_or_default();
                        Preview::Textures {
                            list,
                            res: Arc::new(r),
                            json,
                        }
                    } else {
                        Preview::Json(serde_json::to_string_pretty(&info).unwrap_or_default())
                    }
                }
                Err(err) => Preview::Text(format!("error: {err}")),
            },
            _ => match sixng_core::scan::read_entry(&a.path, e) {
                Ok(d) => {
                    let n = d.len().min(256 * 1024);
                    let printable = d[..n.min(4096)]
                        .iter()
                        .filter(|&&c| c >= 0x20 || c == b'\n' || c == b'\r' || c == b'\t')
                        .count();
                    if printable * 10 > n.min(4096) * 9 {
                        Preview::Text(String::from_utf8_lossy(&d[..n]).to_string())
                    } else {
                        let mut s = String::new();
                        for (row, chunk) in d[..n.min(8192)].chunks(16).enumerate() {
                            s += &format!("{:06x}  {}\n", row * 16, sixng_core::rpf9::hex(chunk));
                        }
                        Preview::Text(s)
                    }
                }
                Err(err) => Preview::Text(format!("error: {err}")),
            },
        };
    }

    fn request_texture(&mut self, ctx: &egui::Context) {
        let Preview::Textures { list, res, .. } = &self.preview else {
            return;
        };
        let Some(t) = list.get(self.tex_sel).cloned() else {
            return;
        };
        let res = res.clone();
        let slot = self.tex_pending.clone();
        let key = format!("{}#{}", t.struct_off, t.name);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = (|| -> anyhow::Result<(u32, u32, Vec<u8>)> {
                let (fmt, lin) = sixng_core::texture::mip0_linear(&res, &t, false)?;
                let rgba = sixng_core::texture::to_rgba(fmt, &lin, t.width, t.height)?;
                Ok((t.width, t.height, rgba))
            })();
            *slot.lock().unwrap() = Some((key, r.map_err(|e| e.to_string())));
            ctx.request_repaint();
        });
    }

    fn extract(&mut self, ctx: &egui::Context, all_filtered: bool) {
        let (Some(ix), Some(ai)) = (self.index.clone(), self.sel_archive) else {
            return;
        };
        let a = self.archives[ai].clone();
        let idxs: Vec<usize> = if all_filtered {
            self.filtered.clone()
        } else {
            self.selected.iter().copied().collect()
        };
        let body_shape = match sixng_core::bodyshape::parse_spec(&self.body_shape) {
            Ok(v) => v,
            Err(e) => {
                self.settings_msg = e;
                return;
            }
        };
        let opts = ExportOptions {
            out: PathBuf::from(&self.out),
            png: self.tex_png,
            dds: self.tex_dds,
            raw: false,
            force: false,
            skip_embedded_textures: false,
            body_shape,
            head_skin: self.head_skin,
            paint: None,
            tex_remap: Default::default(), ped_tints: Default::default(),
        };
        self.start_task(
            &format!("Extracting {} entries", idxs.len()),
            ctx,
            move |pr| {
                let sel: Vec<&Entry> = idxs.iter().map(|&i| &ix.entries[i]).collect();
                let items = export::export_many(&a, &sel, &opts, pr).map_err(|e| e.to_string())?;
                let ok = items.iter().filter(|i| i.status == "ok").count();
                let sk = items.iter().filter(|i| i.status == "skipped").count();
                let er = items.iter().filter(|i| i.status == "error").count();
                Ok(format!(
                    "extracted {ok}, skipped {sk}, errors {er} -> {}",
                    opts.out.display()
                ))
            },
        );
    }
}

impl App {
    fn logic_inner(&mut self, ctx: &egui::Context) {
        self.jobs.poll();
        for (id, ok) in self.jobs.take_finished() {
            if ok && self.texture_users_job == Some(id) {
                self.textures.reload_users();
            }
            if ok {
                self.refresh();
                self.status = pages::Status::default();
            }
        }
        self.setup_tick();
        if self.bake.running() {
            if let Some(ok) = self.bake.poll(&self.work) {
                if ok {
                    self.map2d.reload();
                    self.map3d.reset_data();
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        if self.jobs.active_count() > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    fn ui_inner(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&eframe::egui_wgpu::RenderState>,
        #[cfg(all(target_os = "macos", feature = "metal4-poc"))] mut metal4_renderer: Option<
            &mut render3d_metal4::Renderer3D4,
        >,
    ) {
        let ctx = ui.ctx().clone();
        if self.setup.open {
            if !self.cli_started && self.cli.get("tab") == Some("setup") {
                self.cli_started = true;
                if let Some(p) = self.cli.get("screenshot") {
                    self.shot = Some((PathBuf::from(p), 0, self.cli.has("exit")));
                }
            }
            self.setup_ui(ui);
            self.window_shot(&ctx);
            return;
        }
        if !self.cli_started {
            self.cli_started = true;
            self.apply_cli(&ctx);
        }
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Library, "🏠 Library");
                ui.selectable_value(&mut self.tab, Tab::Model, "🧊 Model viewer");
                ui.selectable_value(&mut self.tab, Tab::Map2D, "🗺 Map 2D");
                ui.selectable_value(&mut self.tab, Tab::Map3D, "🌆 Map 3D");
                ui.selectable_value(&mut self.tab, Tab::Textures, "🖼 Textures");
                ui.selectable_value(&mut self.tab, Tab::Audio, "🔊 Audio");
                ui.separator();
                let n = self.jobs.active_count();
                let jl = if n > 0 {
                    format!("⚙ Jobs ({n})")
                } else {
                    "⚙ Jobs".to_string()
                };
                ui.selectable_value(&mut self.tab, Tab::Jobs, jl);
                ui.selectable_value(&mut self.tab, Tab::Status, "📊 Status");
                ui.selectable_value(&mut self.tab, Tab::Settings, "🔧 Settings");
                ui.selectable_value(&mut self.tab, Tab::Browser, "📦 Archives")
                    .on_hover_text("Advanced: browse raw archive entries");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
                    if let Some(renderer) = metal4_renderer.as_deref_mut() {
                        let mut mode = renderer.temporal_upscale_mode();
                        egui::ComboBox::from_id_salt("metalfx-temporal-mode")
                            .selected_text(match mode {
                                render3d_metal4::TemporalUpscaleMode::Off => "MetalFX Off",
                                render3d_metal4::TemporalUpscaleMode::Scale150 => "MetalFX 1.5x",
                                render3d_metal4::TemporalUpscaleMode::Scale200 => "MetalFX 2x",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut mode, render3d_metal4::TemporalUpscaleMode::Off, "Off");
                                ui.selectable_value(&mut mode, render3d_metal4::TemporalUpscaleMode::Scale150, "1.5x");
                                ui.selectable_value(&mut mode, render3d_metal4::TemporalUpscaleMode::Scale200, "2x");
                            });
                        renderer.set_temporal_upscale_mode(mode);
                    }
                    if ui.button("🔍 Search  Ctrl+K").clicked() {
                        self.palette.open = true;
                    }
                    if self.bake.running() {
                        ui.spinner();
                        ui.weak("baking map");
                    }
                });
            });
        });
        self.palette_ui(&ctx);
        self.window_shot(&ctx);
        match self.tab {
            Tab::Library => {
                let out = self.out.clone();
                let mut act = None;
                egui::CentralPanel::default().show(ui, |ui| {
                    act = self.library.ui(ui, render_state, &mut self.model, &out)
                });
                match act {
                    Some(library::Action::OpenModel(id)) => {
                        self.model.open(&id, 0, &ctx);
                        self.tab = Tab::Model;
                    }
                    Some(library::Action::ShowMap) => self.tab = Tab::Map2D,
                    Some(library::Action::ShowTextures) => self.tab = Tab::Textures,
                    Some(library::Action::ShowAudio) => self.tab = Tab::Audio,
                    Some(library::Action::Job(label, args)) => {
                        self.jobs.enqueue(&label, args);
                        self.tab = Tab::Jobs;
                    }
                    Some(library::Action::OpenJobs) => self.tab = Tab::Jobs,
                    None => {}
                }
                return;
            }
            Tab::Jobs => {
                egui::CentralPanel::default().show(ui, |ui| self.jobs.ui(ui));
                return;
            }
            Tab::Status => {
                egui::CentralPanel::default().show(ui, |ui| self.status_ui(ui));
                return;
            }
            Tab::Settings => {
                egui::CentralPanel::default().show(ui, |ui| self.settings_ui(ui));
                return;
            }
            _ => {}
        }
        if self.tab == Tab::Model {
            egui::CentralPanel::default().show(ui, |ui| self.model.ui(ui, render_state));
            if let Some(h) = self.model.view_texture.take() {
                self.textures.show_hash(h);
                self.tab = Tab::Textures;
            }
            return;
        }
        if self.tab == Tab::Textures {
            egui::CentralPanel::default().show(ui, |ui| self.textures.ui(ui));
            if let Some(id) = self.textures.open_model.take() {
                self.model.open(&id, 0, &ctx);
                self.tab = Tab::Model;
            }
            if std::mem::take(&mut self.textures.want_users_job) {
                self.texture_users_job = Some(self.jobs.enqueue("Texture users", vec!["texture-users".into()]));
                self.tab = Tab::Jobs;
            }
            return;
        }
        if self.tab == Tab::Audio {
            egui::CentralPanel::default().show(ui, |ui| self.audio.ui(ui));
            return;
        }
        if self.tab == Tab::Map2D || self.tab == Tab::Map3D {
            self.bake_bar(ui);
        }
        if self.tab == Tab::Map3D {
            self.map2d.ensure_loaded(&ctx);
            if self.map3d.data.is_none() {
                if let Some(d) = &self.map2d.data {
                    self.map3d.data = Some(d.clone());
                    if let Some([x, y, h]) = self.map3d_start.take() {
                        self.map3d.place(x, y, h);
                    }
                }
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    self.map3d.ui(
                        ui,
                        render_state,
                        #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
                        metal4_renderer.as_deref_mut(),
                    )
                });
            if let Some(m) = self.map3d.open_model.take() {
                self.open_from_map(&m, &ctx);
            }

            return;
        }
        if self.tab == Tab::Map2D {
            egui::CentralPanel::default().show(ui, |ui| self.map2d.ui(ui));
            if let Some(m) = self.map2d.open_model.take() {
                self.open_from_map(&m, &ctx);
            }
            return;
        }
        if let Some(ix) = self.loaded_index.lock().unwrap().take() {
            self.index = Some(Arc::new(ix));
            self.filter_dirty = true;
        }
        if self.filter_dirty {
            self.apply_filter();
        }
        let pending = self.tex_pending.lock().unwrap().take();
        if let Some((key, r)) = pending {
            match r {
                Ok((w, h, rgba)) => {
                    let img =
                        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    self.tex_handle = Some((
                        key.clone(),
                        ctx.load_texture(key, img, egui::TextureOptions::LINEAR),
                    ));
                }
                Err(e) => self.preview_error(e),
            }
        }

        let mut world_req: Option<&'static str> = None;
        egui::Panel::top("top").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Game:");
                ui.add(egui::TextEdit::singleline(&mut self.game).desired_width(220.0));
                if ui.button("Browse…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.game = p.display().to_string();
                        self.refresh();
                    }
                }
                ui.label("Work:");
                ui.add(egui::TextEdit::singleline(&mut self.work).desired_width(200.0)).on_hover_text("Folder for indexes, the map bake and the model cache");
                if ui.button("Browse…").on_hover_text("Pick the work folder").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.work = p.display().to_string();
                    }
                }
                if ui.button("💾 Save").on_hover_text("Remember these game and work folders (used from the next start, wherever the app is launched from)").clicked() {
                    let st = self.settings();
                    self.settings_msg = match save_settings(&st) {
                        Ok(()) => "Saved. Restart the app to use the new folders everywhere.".into(),
                        Err(e) => format!("Could not save settings: {e}"),
                    };
                }
                if !self.settings_msg.is_empty() {
                    ui.label(egui::RichText::new(&self.settings_msg).small());
                }
                if ui.button("Refresh").clicked() {
                    self.refresh();
                }
                ui.separator();
                ui.label("Output:");
                ui.add(egui::TextEdit::singleline(&mut self.out).desired_width(200.0));
                if ui.button("…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.out = p.display().to_string();
                    }
                }
                ui.checkbox(&mut self.tex_png, "PNG");
                ui.checkbox(&mut self.tex_dds, "DDS");
                ui.label("Body shape:").on_hover_text("Ped clothing: pose the shared ped skeleton with the game's body-shape rig, e.g. gcweightbelly=0.5,gcmusclechest=1 (gcweight* -1..1, gcmuscle* 0..1, gcbutt, gcbreasts; allweight=v, allmuscle=v). Empty = neutral. Needs packs.rpf and anim.rpf indexed.");
                ui.add(egui::TextEdit::singleline(&mut self.body_shape).hint_text("neutral").desired_width(160.0));
                ui.label("Head skin:").on_hover_text("Ped heads: face variant (0 = first). A head gets the face the game links to it, or one of the faces of the same sex, body type and ethnicity. The model viewer has arrows to switch through them.");
                ui.add(egui::DragValue::new(&mut self.head_skin).range(0..=999));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("World data (whole map → Output):");
                for (id, label, help) in sixng_core::world::STEPS {
                    if ui.add_enabled(!self.busy(), egui::Button::new(*label)).on_hover_text(*help).clicked() {
                        world_req = Some(*id);
                    }
                }
            });
        });
        if let Some(step) = world_req {
            let ws = self.ws();
            let out = PathBuf::from(&self.out);
            self.start_task(&format!("World data: {step}"), &ctx, move |pr| {
                sixng_core::world::run_step(&ws, step, &out, pr).map_err(|e| e.to_string())
            });
        }

        egui::Panel::bottom("status").show(ui, |ui| {
            let t = self.task.lock().unwrap();
            ui.horizontal(|ui| {
                if t.running {
                    ui.spinner();
                    ui.label(&t.label);
                    if t.total > 0 {
                        ui.add(
                            egui::ProgressBar::new(t.done as f32 / t.total as f32)
                                .desired_width(300.0)
                                .show_percentage(),
                        );
                    }
                } else if !t.label.is_empty() {
                    ui.label(format!(
                        "{} — {}",
                        t.label,
                        t.message.clone().unwrap_or_default()
                    ));
                } else {
                    ui.label("Ready");
                }
            });
        });

        let mut open_req = None;
        let mut index_req = None;
        egui::Panel::left("archives")
            .resizable(true)
            .default_size(300.0)
            .max_size(380.0)
            .show(ui, |ui| {
                ui.heading("Archives");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let ws = self.ws();
                    for (i, a) in self.archives.iter().enumerate() {
                        let indexed = ws.is_indexed(a);
                        ui.horizontal(|ui| {
                            let sel = self.sel_archive == Some(i);
                            let label = format!("{} ({:.1} GB)", a.id, a.size as f64 / 1e9);
                            let txt = if indexed {
                                label.clone()
                            } else {
                                format!("○ {label}")
                            };
                            let r = ui.selectable_label(sel, txt).on_hover_text(&a.id);
                            if r.clicked() {
                                open_req = Some(i);
                            }
                            if !indexed && ui.small_button("Index").clicked() {
                                index_req = Some(i);
                            }
                        });
                    }
                });
            });
        if !self.busy() {
            if let Some(i) = index_req {
                self.index_archive(i, &ctx);
            } else if let Some(i) = open_req {
                self.open_archive(i, &ctx);
            }
        }

        let mut focus_req = None;
        let mut tex_req = false;
        egui::Panel::right("preview")
            .resizable(true)
            .default_size(540.0)
            .min_size(420.0)
            .show(ui, |ui| {
                ui.heading("Preview");
                match &self.preview {
                    Preview::None => {
                        ui.label("Select an entry.");
                    }
                    Preview::Text(s) | Preview::Json(s) => {
                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(s.as_str()).monospace())
                                    .extend(),
                            );
                        });
                    }
                    Preview::Textures { list, json, .. } => {
                        let names: Vec<String> = list
                            .iter()
                            .map(|t| format!("{} {}x{} {}", t.name, t.width, t.height, t.format))
                            .collect();
                        let cur = names.get(self.tex_sel).cloned().unwrap_or_default();
                        egui::ComboBox::from_label("texture")
                            .selected_text(cur)
                            .width(380.0)
                            .show_ui(ui, |ui| {
                                for (i, n) in names.iter().enumerate() {
                                    if ui.selectable_value(&mut self.tex_sel, i, n).clicked() {
                                        tex_req = true;
                                    }
                                }
                            });
                        if self.tex_handle.is_none() {
                            tex_req = true;
                        }
                        if let Some((_, h)) = &self.tex_handle {
                            let size = h.size_vec2();
                            let scale = (500.0 / size.x.max(size.y)).min(1.0);
                            ui.image((h.id(), size * scale));
                        } else {
                            ui.spinner();
                        }
                        ui.collapsing("metadata", |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(json.as_str()).monospace(),
                                        )
                                        .extend(),
                                    );
                                });
                        });
                    }
                }
            });
        if tex_req && matches!(self.preview, Preview::Textures { .. }) {
            self.tex_handle = None;
            if self.tex_pending.lock().unwrap().is_none() {
                self.request_texture(&ctx);
            }
        }

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(ix) = self.index.clone() else {
                ui.label("Pick an indexed archive on the left, or press Index (first scan of levels.rpf takes ~3 min).");
                return;
            };
            ui.horizontal(|ui| {
                ui.label("Search:");
                if ui.add(egui::TextEdit::singleline(&mut self.filter_text).hint_text("regex on name / id").desired_width(220.0)).changed() {
                    self.filter_dirty = true;
                }
                egui::ComboBox::from_label("kind").selected_text(if self.filter_kind.is_empty() { "any" } else { &self.filter_kind }).show_ui(ui, |ui| {
                    if ui.selectable_value(&mut self.filter_kind, String::new(), "any").clicked() {
                        self.filter_dirty = true;
                    }
                    for k in Kind::all() {
                        if ui.selectable_value(&mut self.filter_kind, k.name(), k.name()).clicked() {
                            self.filter_dirty = true;
                        }
                    }
                });
                egui::ComboBox::from_label("type").selected_text(if self.filter_rtype.is_empty() { "any" } else { &self.filter_rtype }).show_ui(ui, |ui| {
                    for t in ["", "txd", "dr", "ft", "dd", "td"] {
                        if ui.selectable_value(&mut self.filter_rtype, t.to_string(), if t.is_empty() { "any" } else { t }).clicked() {
                            self.filter_dirty = true;
                        }
                    }
                });
                ui.label(format!("{} / {} entries, {} selected", self.filtered.len(), ix.entries.len(), self.selected.len()));
            });
            ui.horizontal(|ui| {
                let busy = self.busy();
                if ui.add_enabled(!busy && !self.selected.is_empty(), egui::Button::new("Extract selected")).clicked() {
                    self.extract(&ctx, false);
                }
                if ui.add_enabled(!busy && !self.filtered.is_empty(), egui::Button::new(format!("Extract all {} filtered", self.filtered.len()))).clicked() {
                    self.extract(&ctx, true);
                }
                if ui.button("Clear selection").clicked() {
                    self.selected.clear();
                }
                if let (Some(f), Some(ai)) = (self.focus, self.sel_archive) {
                    let e = &ix.entries[f];
                    if matches!(e.rtype.as_deref(), Some("dr") | Some("ft") | None) && e.kind == Kind::RageObj && ui.button("🧊 Open in model viewer").clicked() {
                        let _ = ai;
                        let id = e.id.clone();
                        self.tab = Tab::Model;
                        self.model.open(&id, 0, &ctx);
                    }
                }
                if !ix.names_done {
                    ui.label("(names not recovered yet: re-index to get names)");
                }
            });
            ui.separator();
            let row_h = 18.0;
            egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, row_h, self.filtered.len(), |ui, range| {
                for r in range {
                    let i = self.filtered[r];
                    let e = &ix.entries[i];
                    ui.horizontal(|ui| {
                        let mut checked = self.selected.contains(&i);
                        if ui.checkbox(&mut checked, "").changed() {
                            if checked {
                                self.selected.insert(i);
                            } else {
                                self.selected.remove(&i);
                            }
                        }
                        let text = format!(
                            "{:>10x}  {:<8} {:<4} {:>10}  {}",
                            e.off,
                            e.kind.name(),
                            e.rtype.as_deref().unwrap_or(""),
                            human(e.usize),
                            e.name.as_deref().unwrap_or("")
                        );
                        if ui.selectable_label(self.focus == Some(i), egui::RichText::new(text).monospace()).clicked() {
                            focus_req = Some(i);
                        }
                    });
                }
            });
        });
        if let Some(i) = focus_req {
            self.focus_entry(i);
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.logic_inner(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.ui_inner(
            ui,
            frame.wgpu_render_state(),
            #[cfg(all(target_os = "macos", feature = "metal4-poc"))]
            None,
        );
    }
}

#[cfg(all(target_os = "macos", feature = "metal4-poc"))]
impl metal4_app::UiApp for App {
    fn logic(&mut self, ctx: &egui::Context) {
        self.logic_inner(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, renderer: &mut render3d_metal4::Renderer3D4) {
        self.ui_inner(ui, None, Some(renderer));
    }
}

impl App {
    /// Full-window screenshot for --tab map2d automation: waits for the map
    /// data and tiles, then saves and optionally exits.
    fn window_shot(&mut self, ctx: &egui::Context) {
        let Some((path, frames, exit)) = self.shot.clone() else {
            return;
        };
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            let _ = render3d::save_png(&path, img.size[0] as u32, img.size[1] as u32, &rgba);
            self.shot = None;
            if exit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }
        let ready = frames > 40
            && !self.bake.running()
            && match self.tab {
                Tab::Model => self.model.idle(),
                Tab::Textures => self.textures.idle(),
                Tab::Audio => self.audio.idle(),
                _ if self.setup.open => true,
                Tab::Library => self.library.idle() && self.model.picker.loaded(),
                Tab::Status => self.status.sizes.lock().unwrap().is_some(),
                Tab::Jobs | Tab::Settings => true,
                _ => self.map2d.data.is_some(),
            };
        if ready && frames < 100_000 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.shot = Some((path, 100_000, exit));
        } else if frames < 100_000 {
            self.shot = Some((path, frames + 1, exit));
        }
        ctx.request_repaint();
    }

    /// A model picked on the map: its Library item page when the model is
    /// listed there, else straight to the viewer.
    fn open_from_map(&mut self, m: &str, ctx: &egui::Context) {
        let ml = m.to_ascii_lowercase();
        let hit = self.model.picker.items().and_then(|items| {
            items
                .iter()
                .find(|it| it.id == m || it.lname == ml)
                .map(|it| it.id.clone())
        });
        match hit {
            Some(id) => {
                self.library.show_item(&id);
                self.tab = Tab::Library;
            }
            None => {
                self.tab = Tab::Model;
                self.model.open(m, 0, ctx);
            }
        }
    }

    fn apply_cli(&mut self, ctx: &egui::Context) {
        let c = self.cli.clone();
        let page = match c.get("tab") {
            Some("library") => Some(Tab::Library),
            Some("jobs") => Some(Tab::Jobs),
            Some("status") => Some(Tab::Status),
            Some("settings") => Some(Tab::Settings),
            Some("archives") => Some(Tab::Browser),
            Some("audio") => Some(Tab::Audio),
            _ => None,
        };
        if let Some(t) = page {
            self.tab = t;
            if let Some(b) = c
                .get("bank")
                .and_then(|h| u64::from_str_radix(h.trim_start_matches("0x"), 16).ok())
            {
                let play = c
                    .get("play")
                    .and_then(|h| u32::from_str_radix(h.trim_start_matches("0x"), 16).ok());
                self.audio.select_bank(b, play);
            }
            if let Some(id) = c.get("item") {
                self.library.show_item(id);
            }
            if c.has("palette") {
                self.palette.open = true;
                self.palette_query(c.get("palette").unwrap_or(""));
            }
            if let Some(p) = c.get("screenshot") {
                self.shot = Some((PathBuf::from(p), 0, c.has("exit")));
            }
            return;
        }
        if c.get("tab") == Some("textures") || c.get("texture").is_some() {
            self.tab = Tab::Textures;
            if let Some(h) = c
                .get("texture")
                .and_then(|h| u64::from_str_radix(h.trim_start_matches("0x"), 16).ok())
            {
                self.textures.show_hash(h);
            }
        }
        if c.get("tab") == Some("map3d") {
            self.tab = Tab::Map3D;
            let m3 = &mut self.map3d;
            let (mut x, mut y) = (-640.0, -60.0);
            if let Some(v) = c.get("center") {
                let v: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                if v.len() == 2 {
                    (x, y) = (v[0], v[1]);
                }
            }
            self.map3d_start = Some([x, y, c.f("height").unwrap_or(120.0)]);
            if let Some(v) = c.f("yaw") {
                m3.cam.yaw = v.to_radians();
            }
            if let Some(v) = c.f("pitch") {
                m3.cam.pitch = v.to_radians();
            }
            if let Some(v) = c.get("pick") {
                // "x,y;x,y" as fractions of the 3D view
                m3.auto.picks = v
                    .split(';')
                    .filter_map(|p| p.split_once(','))
                    .filter_map(|(a, b)| Some([a.trim().parse().ok()?, b.trim().parse().ok()?]))
                    .collect();
            }
            if let Some(v) = c.get("quality") {
                if let Some(q) = map3d::QUALITY.iter().position(|x| x.0 == v) {
                    m3.set_quality(q);
                }
            }
            if let Some(v) = c.f("detail") {
                m3.detail_level = v;
                m3.detail = map3d::Detail::from_level(v);
            }
            if let Some(v) = c.u("max-tex") {
                m3.detail.max_tex = v as u32;
            }
            if c.has("no-textures") {
                m3.detail.textures = false;
            }
            for (key, arr) in [("kinds", &mut m3.kinds), ("lods", &mut m3.lods)] {
                if let Some(v) = c.get(key) {
                    *arr = [false; 6];
                    for k in v.split(',').filter_map(|x| x.parse::<usize>().ok()) {
                        if k < 6 {
                            arr[k] = true;
                        }
                    }
                }
            }
            if let Some(v) = c.f("ram-cap") {
                m3.set_ram_cap(Some(v), false);
            }
            m3.decal_bias = !c.has("no-decal-bias");
            m3.show_proxies = c.has("show-proxies");
            if c.has("no-hd-fallback") {
                m3.detail.hd_fallback = false;
            }
            if let Some(h) = c.f("tod") {
                m3.tod = h;
                m3.look.set_time_of_day(h);
            }
            if let Some(v) = c.f("fog") {
                m3.look.fog = v;
            }
            if let Some(v) = c.f("exposure") {
                m3.look.exposure = v;
            }
            m3.show_roads = c.has("roads");
            m3.show_lights = c.has("lights");
            m3.show_terrain = !c.has("no-terrain");
            if c.has("no-windows") {
                m3.show_minimap = false;
                m3.show_stats = false;
            }
            if let Some(v) = c.get("fly") {
                // "x,y,height,seconds[;x,y,height,seconds...]"
                for leg in v.split(';') {
                    let v: Vec<f32> = leg
                        .split(',')
                        .filter_map(|x| x.trim().parse().ok())
                        .collect();
                    if v.len() == 4 {
                        m3.auto.fly.push(([v[0], v[1], v[2]], v[3]));
                    }
                }
            }
            if let Some(p) = c.get("screenshot") {
                m3.auto.screenshot = Some(PathBuf::from(p));
                m3.auto.exit = c.has("exit");
            }
            return;
        }
        if c.get("tab") == Some("map2d") {
            self.tab = Tab::Map2D;
            let mp = &mut self.map2d;
            if let Some(v) = c.get("center") {
                let v: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                if v.len() == 2 {
                    mp.cx = v[0];
                    mp.cy = v[1];
                }
            }
            if let Some(v) = c.f("mpp") {
                mp.mpp = v;
            }
            if let Some(v) = c.u("base") {
                mp.base = v as usize;
            }
            if let Some(v) = c.get("overlays") {
                for o in mp.overlays.iter_mut() {
                    o.0 = false;
                }
                for k in v.split(',').filter_map(|x| x.parse::<usize>().ok()) {
                    if k < mp.overlays.len() {
                        mp.overlays[k].0 = true;
                    }
                }
            }
            mp.show_roads = !c.has("no-roads");
            mp.show_lights = c.has("lights");
            mp.light_int = c.has("interior-lights");
            mp.show_interiors = c.has("interiors");
            if let Some(h) = c.f("hour") {
                mp.hour = h;
            }
            if let Some(v) = c.u("road-colour") {
                mp.road_colour = v as usize;
            }
            if let Some(q) = c.get("search") {
                mp.search = q.to_string();
                mp.pending_search = true;
            }
            if c.has("start-bake") {
                // automation: same as the Bake map button; --screenshot waits for it
                if let Err(e) = self.bake.start(&self.game, &self.work) {
                    self.bake.result = Some(e);
                }
            }
            if let Some(p) = c.get("screenshot") {
                self.shot = Some((PathBuf::from(p), 0, c.has("exit")));
            }
            return;
        }
        let m = &mut self.model;
        if let Some(v) = c.get("hair") {
            // grooms to show: hex offsets in packs.rpf
            m.hair.selected = v
                .split(',')
                .filter_map(|h| u64::from_str_radix(h.trim().trim_start_matches("0x"), 16).ok())
                .collect();
            if let Some(w) = c.f("hair-width") {
                m.hair.width_mm = w;
            }
        }
        if let Some(h) = c.f("tod") {
            m.tod = h;
            m.look.set_time_of_day(h);
        }
        if let Some(v) = c.u("mode") {
            m.look.mode = v.min(7);
        }
        if let Some(v) = c.u("slot") {
            m.look.slot = v.min(3);
        }
        if let Some(v) = c.u("rma") {
            m.look.rma_order = v.min(3);
        }
        if let Some(v) = c.u("finish") {
            m.look.finish = v.min(3);
        }
        if let Some(v) = c.f("pearl") {
            m.look.pearl_amount = v;
        }
        if let Some(v) = c.f("exposure") {
            m.look.exposure = v;
        }
        if let Some(v) = c.f("wet") {
            m.look.wet = v;
        }
        if let Some(p) = c.get("paint") {
            let v: Vec<f32> = p.split(',').filter_map(|x| x.parse().ok()).collect();
            if v.len() == 3 {
                m.look.paint = [v[0], v[1], v[2]];
            }
        }
        m.look.wire = c.has("wire");
        m.look.tint = !c.has("no-tint");
        m.hair.show = !c.has("no-fur");
        m.auto.find_hair = c.has("find-hair");
        m.show_skeleton = c.has("skeleton");
        m.show_normals = c.has("normals");
        m.decal_bias = !c.has("no-decal-bias");
        m.wheel_spin = c.f("spin").unwrap_or(0.0);
        m.door_open = c.f("door").unwrap_or(0.0);
        if let Some(v) = c.f("yaw") {
            m.cam.yaw = v;
        }
        if let Some(v) = c.f("pitch") {
            m.cam.pitch = v;
        }
        m.zoom_after_load = c.f("zoom");
        if let Some(q) = c.get("model") {
            self.tab = Tab::Model;
            let lod = c.u("lod").unwrap_or(0) as usize;
            m.open(q, lod, ctx);
            m.auto.then = c.get("then").map(str::to_string);
        }
        if c.has("no-picker") {
            m.show_picker = false;
        }
        if let Some(s) = c.get("picker-search") {
            m.picker.search = s.to_string();
        }
        if let Some(s) = c.get("picker-cat") {
            m.picker.set_cat(s);
        }
        if let Some(p) = c.get("window-screenshot") {
            // whole window (picker and panels), not just the 3D view
            self.shot = Some((PathBuf::from(p), 0, c.has("exit")));
        } else if let Some(p) = c.get("screenshot") {
            m.auto.screenshot = Some(PathBuf::from(p));
            m.auto.exit = c.has("exit");
        }
    }

    fn preview_error(&mut self, e: String) {
        if let Preview::Textures { json, .. } = &mut self.preview {
            *json = format!("texture decode error: {e}\n\n{json}");
        }
    }
}

/// Name of the platform file manager, for button labels.
pub const FILE_MANAGER: &str = if cfg!(windows) {
    "Explorer"
} else if cfg!(target_os = "macos") {
    "Finder"
} else {
    "file manager"
};

/// Open a file or folder with the platform's default handler.
pub fn open_path(p: &std::path::Path) {
    let cmd = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(p).spawn();
}

/// Show a file selected in the file manager (Linux: open its folder).
pub fn reveal(p: &std::path::Path) {
    let _ = if cfg!(windows) {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", p.display()))
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg("-R").arg(p).spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(p.parent().unwrap_or(p))
            .spawn()
    };
}

/// The SixNG banner (logo) as a texture, loaded once.
fn banner(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("sixng_banner");
    if let Some(h) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return h;
    }
    let png = include_bytes!("../../../assets/logo/sixng_banner_640.png");
    let ci = match image::load_from_memory(png) {
        Ok(img) => {
            let img = img.to_rgba8();
            let (w, h) = img.dimensions();
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw())
        }
        Err(_) => egui::ColorImage::new([1, 1], vec![egui::Color32::TRANSPARENT]),
    };
    let h = ctx.load_texture("sixng_banner", ci, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, h.clone()));
    h
}

fn human(n: u64) -> String {
    if n >= 10 << 30 {
        format!("{:.1} GB", n as f64 / (1u64 << 30) as f64)
    } else if n >= 1 << 20 {
        format!("{:.1} MB", n as f64 / (1u64 << 20) as f64)
    } else if n >= 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}

#[allow(dead_code)]
fn unused(_: Filter) {}
