#[cfg(target_os = "macos")]
#[path = "../src/bc.rs"]
mod bc;
#[cfg(target_os = "macos")]
#[path = "../src/cells.rs"]
mod cells;
#[cfg(target_os = "macos")]
#[path = "../src/glb.rs"]
mod glb;
#[cfg(target_os = "macos")]
#[path = "../src/mapbake.rs"]
mod mapbake;
#[cfg(target_os = "macos")]
#[path = "../src/metal4_app.rs"]
mod metal4_app;
#[cfg(target_os = "macos")]
#[path = "../src/metal4_egui.rs"]
mod metal4_egui;
#[cfg(target_os = "macos")]
#[path = "../src/render3d.rs"]
mod render3d;
#[cfg(target_os = "macos")]
#[path = "../src/render3d_metal4.rs"]
mod render3d_metal4;

#[cfg(target_os = "macos")]
struct DemoUi;

#[cfg(target_os = "macos")]
impl metal4_app::UiApp for DemoUi {
    fn logic(&mut self, _ctx: &eframe::egui::Context) {}

    fn ui(&mut self, ui: &mut eframe::egui::Ui, _renderer: &mut render3d_metal4::Renderer3D4) {
        eframe::egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Metal 4 egui painter");
            ui.label("The window, UI, and presentation are rendered through Metal 4.");
        });
    }
}

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    if !metal4_app::is_available()? {
        anyhow::bail!("Metal 4 requires macOS 26 or later and a Metal 4-capable GPU");
    }
    metal4_app::run(DemoUi)
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The Metal 4 window example only runs on macOS.");
}
