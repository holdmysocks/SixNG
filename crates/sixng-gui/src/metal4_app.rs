pub trait UiApp {
    fn logic(&mut self, ctx: &eframe::egui::Context);
    fn ui(&mut self, ui: &mut eframe::egui::Ui, renderer: &mut crate::render3d_metal4::Renderer3D4);
    fn configure_renderer(
        &mut self,
        _renderer: &mut crate::render3d_metal4::Renderer3D4,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::UiApp;
    use anyhow::{bail, Context};
    use eframe::egui::{self, ViewportCommand, ViewportId};
    use metal_rust::foundation::{OperatingSystemVersion, ProcessInfo};
    use metal_rust::metal::{Device, GPUFamily, PixelFormat, ResidencySet};
    use metal_rust::quartz_core::Layer;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use winit::application::ApplicationHandler;
    use winit::dpi::PhysicalSize;
    use winit::event::{StartCause, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
    use winit::raw_window_handle::HasWindowHandle;
    use winit::window::{Window, WindowId};

    fn is_macos_26_or_later(version: &OperatingSystemVersion) -> bool {
        (
            version.major_version,
            version.minor_version,
            version.patch_version,
        ) >= (26, 0, 0)
    }

    fn os_supports_metal4() -> bool {
        is_macos_26_or_later(&ProcessInfo::current().operating_system_version())
    }

    pub fn is_available() -> anyhow::Result<bool> {
        if !os_supports_metal4() {
            return Ok(false);
        }
        let Some(device) = Device::system_default() else {
            return Ok(false);
        };
        Ok(device.supports_family(GPUFamily::GPUFamilyMetal4)?)
    }

    struct Renderer {
        device: Device,
        layer: Layer,
        queue: metal_rust::metal4::CommandQueue,
        painter: super::super::metal4_egui::Painter,
        scene: crate::render3d_metal4::Renderer3D4,
        residency: ResidencySet,
        size: PhysicalSize<u32>,
        has_presented: bool,
        frame_presentation: crate::frame_presentation_mtl4::FramePresentationMtl4,
    }

    impl Renderer {
        fn new(window: &Window) -> anyhow::Result<Self> {
            if !os_supports_metal4() {
                bail!("Metal 4 requires macOS 26 or later");
            }
            let device = Device::system_default().context("no Metal device")?;
            if !device.supports_family(GPUFamily::GPUFamilyMetal4)? {
                bail!("{} does not support the Metal 4 GPU family", device.name());
            }

            let layer = Layer::with_device(&device)?;
            layer.set_pixel_format(PixelFormat::BGRA8_UNORM_SRGB);
            layer.set_framebuffer_only(true);
            layer.set_maximum_drawable_count(3)?;
            layer.set_display_sync_enabled(true);
            let handle = window.window_handle()?.as_raw();
            layer.attach_to_window(handle)?;

            let size = window.inner_size();
            layer.set_drawable_size(size.width as f64, size.height as f64)?;

            let queue = device.new_mtl4_command_queue()?;
            let residency = layer.residency_set()?;
            queue.add_residency_set(&residency)?;
            let painter = super::super::metal4_egui::Painter::new(&device)?;
            let scene = crate::render3d_metal4::Renderer3D4::new(&device, &queue)?;
            let refresh_rate = window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz());
            let mut frame_presentation =
                crate::frame_presentation_mtl4::FramePresentationMtl4::new(refresh_rate);
            frame_presentation.resize(&device, size)?;
            frame_presentation.start()?;

            Ok(Self {
                device,
                layer,
                queue,
                painter,
                scene,
                residency,
                size,
                has_presented: false,
                frame_presentation,
            })
        }

        fn resize(&mut self, size: PhysicalSize<u32>) -> anyhow::Result<()> {
            self.size = size;
            if size.width > 0 && size.height > 0 {
                self.layer
                    .set_drawable_size(size.width as f64, size.height as f64)?;
                self.frame_presentation.resize(&self.device, size)?;
                self.frame_presentation.reset();
            }
            Ok(())
        }

        fn set_refresh_rate_millihertz(&mut self, rate: Option<u32>) {
            self.frame_presentation.set_refresh_rate_millihertz(rate);
            if rate.is_none_or(|rate| rate < 119_500)
                && self.scene.frame_generation_mode()
                    == crate::render3d_metal4::FrameGenerationMode::Interpolate120
            {
                eprintln!("[frame-generation] disabled: active display is not 120 Hz");
                self.scene
                    .set_frame_generation_mode(crate::render3d_metal4::FrameGenerationMode::Off);
            }
        }

        fn render(
            &mut self,
            ctx: &egui::Context,
            mut output: egui::FullOutput,
        ) -> anyhow::Result<()> {
            if self.scene.frame_generation_mode()
                == crate::render3d_metal4::FrameGenerationMode::Interpolate120
                && !self.frame_presentation.supports_frame_generation()
            {
                eprintln!("[frame-generation] disabled: active display is not 120 Hz");
                self.scene
                    .set_frame_generation_mode(crate::render3d_metal4::FrameGenerationMode::Off);
            }
            let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
            if self.scene.frame_generation_mode()
                == crate::render3d_metal4::FrameGenerationMode::Interpolate120
            {
                let (texture, presented_interpolated) = if self.frame_presentation.phase()
                    == crate::frame_presentation_mtl4::FramePhase::Real
                {
                    (self.scene.real_output_texture(), false)
                } else {
                    match self.scene.interpolated_output_texture() {
                        Some(texture) => (Some(texture), true),
                        None => (self.scene.real_output_texture(), false),
                    }
                };
                if let Some(texture) = texture {
                    self.painter.register_external_texture(
                        egui::TextureId::User(crate::render3d_metal4::MAP_TEXTURE_ID),
                        &texture,
                    )?;
                }
                self.painter.render(
                    &self.layer,
                    &self.queue,
                    &self.residency,
                    [self.size.width, self.size.height],
                    output.pixels_per_point,
                    &primitives,
                    &mut output.textures_delta,
                )?;
                self.frame_presentation.advance(presented_interpolated);
            } else {
                if let Some(texture) = self.scene.output_texture() {
                    self.painter.register_external_texture(
                        egui::TextureId::User(crate::render3d_metal4::MAP_TEXTURE_ID),
                        &texture,
                    )?;
                }
                self.painter.render(
                    &self.layer,
                    &self.queue,
                    &self.residency,
                    [self.size.width, self.size.height],
                    output.pixels_per_point,
                    &primitives,
                    &mut output.textures_delta,
                )?;
            }
            if !self.has_presented {
                println!(
                    "Presented an MTL4 egui frame on {} ({}x{})",
                    self.device.name(),
                    self.size.width,
                    self.size.height
                );
                self.has_presented = true;
            }
            Ok(())
        }
    }

    enum UserEvent {
        Repaint(Duration),
    }

    struct HostApp<T> {
        window: Option<Arc<Window>>,
        window_id: Option<WindowId>,
        renderer: Option<Renderer>,
        egui_ctx: egui::Context,
        egui_state: Option<egui_winit::State>,
        sixng: T,
        proxy: Option<EventLoopProxy<UserEvent>>,
        repaint_at: Option<Instant>,
        failure: Option<anyhow::Error>,
    }

    impl<T: UiApp> HostApp<T> {
        fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
            self.failure = Some(error);
            event_loop.exit();
        }

        fn repaint(&self) {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }

        fn schedule_repaint(&mut self, delay: Duration) {
            let deadline = Instant::now() + delay;
            self.repaint_at = Some(
                self.repaint_at
                    .map_or(deadline, |current| current.min(deadline)),
            );
        }

        fn draw(&mut self, event_loop: &ActiveEventLoop) {
            let Some(window) = self.window.as_ref() else {
                return;
            };
            let Some(state) = self.egui_state.as_mut() else {
                return;
            };
            let input = state.take_egui_input(window);
            let Some(renderer) = self.renderer.as_mut() else {
                return;
            };
            renderer.scene.set_frame_generation_real_slot(
                renderer.frame_presentation.should_render_real_frame(),
            );
            let mut output = self.egui_ctx.run_ui(input, |ui| {
                self.sixng.logic(ui.ctx());
                self.sixng.ui(ui, &mut renderer.scene);
            });

            let root_commands = output
                .viewport_output
                .get(&ViewportId::ROOT)
                .map(|root| root.commands.as_slice())
                .unwrap_or_default();
            if root_commands.contains(&ViewportCommand::Close) {
                output.drop_without_applying_deltas();
                event_loop.exit();
                return;
            }
            if root_commands
                .iter()
                .any(|command| matches!(command, ViewportCommand::Screenshot(_)))
            {
                output.drop_without_applying_deltas();
                self.fail(
                    event_loop,
                    anyhow::anyhow!("Metal 4 screenshot capture has not been implemented yet"),
                );
                return;
            }

            let platform_output = std::mem::take(&mut output.platform_output);
            state.handle_platform_output(window, platform_output);
            if let Err(error) = renderer.render(&self.egui_ctx, output) {
                self.fail(event_loop, error);
                return;
            }
            if renderer.scene.frame_generation_mode()
                == crate::render3d_metal4::FrameGenerationMode::Interpolate120
            {
                let Some(interval) = renderer.frame_presentation.pacing_interval() else {
                    self.fail(
                        event_loop,
                        anyhow::anyhow!("frame generation requires a 120 Hz display"),
                    );
                    return;
                };
                self.repaint_at = Some(renderer.frame_presentation.schedule(interval));
                let (real, interpolated, fallback) = renderer.frame_presentation.counts();
                if (real + interpolated + fallback).is_multiple_of(120) {
                    eprintln!(
                        "[frame-generation] presented real={real} interpolated={interpolated} fallback={fallback}"
                    );
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    self.repaint_at.expect("120fps repaint deadline was set"),
                ));
            }
        }
    }

    impl<T: UiApp> ApplicationHandler<UserEvent> for HostApp<T> {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let attributes = Window::default_attributes()
                .with_title("SixNG - Metal 4")
                .with_inner_size(PhysicalSize::new(960, 640));
            let window = match event_loop.create_window(attributes) {
                Ok(window) => Arc::new(window),
                Err(error) => {
                    self.fail(event_loop, error.into());
                    return;
                }
            };
            let egui_ctx = egui::Context::default();
            let egui_state = egui_winit::State::new(
                egui_ctx.clone(),
                ViewportId::ROOT,
                window.as_ref(),
                Some(window.scale_factor() as f32),
                window.theme(),
                None,
            );
            let Some(proxy) = self.proxy.clone() else {
                self.fail(
                    event_loop,
                    anyhow::anyhow!("missing winit event-loop proxy"),
                );
                return;
            };
            egui_ctx.set_request_repaint_callback(move |info| {
                let _ = proxy.send_event(UserEvent::Repaint(info.delay));
            });
            let mut renderer = match Renderer::new(&window) {
                Ok(renderer) => renderer,
                Err(error) => {
                    self.fail(event_loop, error);
                    return;
                }
            };
            if let Err(error) = self.sixng.configure_renderer(&mut renderer.scene) {
                self.fail(
                    event_loop,
                    error.context("failed to configure the Metal 4 renderer"),
                );
                return;
            }
            self.window_id = Some(window.id());
            self.window = Some(window.clone());
            self.renderer = Some(renderer);
            self.egui_state = Some(egui_state);
            self.egui_ctx = egui_ctx;
            window.request_redraw();
        }

        fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
            if matches!(cause, StartCause::ResumeTimeReached { .. }) {
                self.repaint_at = None;
                self.repaint();
            }
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            window_id: WindowId,
            event: WindowEvent,
        ) {
            if self.window_id != Some(window_id) {
                return;
            }
            let repaint = self
                .egui_state
                .as_mut()
                .zip(self.window.as_ref())
                .is_some_and(|(state, window)| state.on_window_event(window, &event).repaint);
            if repaint {
                self.repaint();
            }
            match event {
                WindowEvent::CloseRequested | WindowEvent::Destroyed => event_loop.exit(),
                WindowEvent::Resized(size) => {
                    if let Some(renderer) = self.renderer.as_mut() {
                        if let Err(error) = renderer.resize(size) {
                            self.fail(event_loop, error);
                            return;
                        }
                        renderer.set_refresh_rate_millihertz(
                            self.window
                                .as_ref()
                                .and_then(|window| window.current_monitor())
                                .and_then(|monitor| monitor.refresh_rate_millihertz()),
                        );
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
                WindowEvent::RedrawRequested => {
                    self.draw(event_loop);
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    if let Some(window) = self.window.as_ref() {
                        let size = window.inner_size();
                        if let Some(renderer) = self.renderer.as_mut() {
                            if let Err(error) = renderer.resize(size) {
                                self.fail(event_loop, error);
                                return;
                            }
                            renderer.set_refresh_rate_millihertz(
                                window
                                    .current_monitor()
                                    .and_then(|monitor| monitor.refresh_rate_millihertz()),
                            );
                        }
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
                _ => {}
            }
        }

        fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
            match event {
                UserEvent::Repaint(delay) if delay.is_zero() => self.repaint(),
                UserEvent::Repaint(delay) => {
                    self.schedule_repaint(delay);
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        self.repaint_at.expect("repaint deadline was set"),
                    ));
                }
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            event_loop.set_control_flow(match self.repaint_at {
                Some(deadline) => ControlFlow::WaitUntil(deadline),
                None => ControlFlow::Wait,
            });
        }
    }

    pub fn run<T: UiApp + 'static>(sixng: T) -> anyhow::Result<()> {
        if !is_available()? {
            bail!("Metal 4 requires macOS 26 or later and a Metal 4-capable GPU");
        }
        let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
        let mut app = HostApp {
            window: None,
            window_id: None,
            renderer: None,
            egui_ctx: egui::Context::default(),
            egui_state: None,
            sixng,
            proxy: Some(event_loop.create_proxy()),
            repaint_at: None,
            failure: None,
        };
        event_loop.run_app(&mut app)?;
        match app.failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{is_macos_26_or_later, OperatingSystemVersion};

        #[test]
        fn metal4_os_gate_starts_at_macos_26() {
            let version = |major_version, minor_version, patch_version| OperatingSystemVersion {
                major_version,
                minor_version,
                patch_version,
            };
            assert!(!is_macos_26_or_later(&version(25, 9, 0)));
            assert!(is_macos_26_or_later(&version(26, 0, 0)));
            assert!(is_macos_26_or_later(&version(27, 0, 0)));
        }
    }
}

#[cfg(target_os = "macos")]
pub fn is_available() -> anyhow::Result<bool> {
    macos::is_available()
}

#[cfg(target_os = "macos")]
pub fn run<T: UiApp + 'static>(sixng: T) -> anyhow::Result<()> {
    macos::run(sixng)
}

#[cfg(not(target_os = "macos"))]
pub fn run<T: UiApp + 'static>(_sixng: T) -> anyhow::Result<()> {
    anyhow::bail!("the Metal 4 window example only runs on macOS")
}
