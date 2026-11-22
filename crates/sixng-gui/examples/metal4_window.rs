#[path = "../src/bc.rs"]
mod bc;
#[path = "../src/cells.rs"]
mod cells;
#[path = "../src/glb.rs"]
mod glb;
#[path = "../src/mapbake.rs"]
mod mapbake;
#[path = "../src/metal4_app.rs"]
mod metal4_app;
#[path = "../src/metal4_egui.rs"]
mod metal4_egui;
#[path = "../src/render3d.rs"]
mod render3d;
#[path = "../src/render3d_metal4.rs"]
mod render3d_metal4;

struct DemoUi;

impl metal4_app::UiApp for DemoUi {
    fn logic(&mut self, _ctx: &eframe::egui::Context) {}

    fn ui(&mut self, ui: &mut eframe::egui::Ui, _renderer: &mut render3d_metal4::Renderer3D4) {
        eframe::egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Metal 4 egui painter");
            ui.label("The window, UI, and presentation are rendered through Metal 4.");
        });
    }
}

fn main() -> anyhow::Result<()> {
    metal4_app::run(DemoUi)
}
