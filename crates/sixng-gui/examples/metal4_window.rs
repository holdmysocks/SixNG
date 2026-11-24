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
#[path = "../src/frame_presentation_mtl4.rs"]
mod frame_presentation_mtl4;
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
#[derive(Default)]
struct DemoUi;

#[cfg(target_os = "macos")]
impl metal4_app::UiApp for DemoUi {
    fn logic(&mut self, _ctx: &eframe::egui::Context) {}

    fn ui(&mut self, ui: &mut eframe::egui::Ui, renderer: &mut render3d_metal4::Renderer3D4) {
        eframe::egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Metal 4 egui painter");
            ui.label("The window, UI, and presentation are rendered through Metal 4.");
            let size = ui.available_size().ceil();
            let pixels = [size.x as u32, size.y as u32];
            let seconds = ui.input(|input| input.time);
            let mut camera = render3d::Camera::default();
            camera.fly = true;
            camera.eye = [seconds.sin() as f32, 0.0, 3.0].into();
            camera.pitch = 0.0;
            camera.yaw = 0.0;
            let look = render3d::Look::default();
            match renderer.render(pixels, &camera, &look, &[], &[], &[]) {
                Ok(texture) => {
                    let (rect, _) = ui.allocate_exact_size(size, eframe::egui::Sense::hover());
                    ui.painter().image(
                        texture,
                        rect,
                        eframe::egui::Rect::from_min_max(
                            eframe::egui::pos2(0.0, 0.0),
                            eframe::egui::pos2(1.0, 1.0),
                        ),
                        eframe::egui::Color32::WHITE,
                    );
                }
                Err(error) => {
                    ui.colored_label(eframe::egui::Color32::RED, format!("{error:#}"));
                }
            }
        });
    }

    fn configure_renderer(
        &mut self,
        renderer: &mut render3d_metal4::Renderer3D4,
    ) -> anyhow::Result<()> {
        renderer.set_frame_generation_mode(
            render3d_metal4::FrameGenerationMode::Interpolate120,
        );
        Ok(())
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
