use crate::{graphics::Graphics, App};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::windows::{IconExtWindows, WindowAttributesExtWindows},
    window::{Window, WindowId, WindowLevel},
};

pub fn run() -> Result<(), String> {
    // Before anything reads settings or presets: adopt files written by
    // versions from before the NeuralLayer rename.
    let migration = crate::migrate::legacy_names(&crate::runtime_folder());
    let event_loop = EventLoop::<()>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    let ctx = egui::Context::default();
    let proxy = event_loop.create_proxy();
    ctx.set_request_repaint_callback(move |_| {
        let _ = proxy.send_event(());
    });
    let mut runner = Runner {
        ctx,
        window: None,
        state: None,
        app: None,
        pending: Vec::new(),
        next_frame: Instant::now(),
        error: None,
        input_router: None,
        pointer_known: false,
        cadence_probe: crate::cadence::Probe::from_args(),
        migration: Some(migration),
    };
    event_loop.run_app(&mut runner).map_err(|e| e.to_string())?;
    // Drop capture and graphics before their native window.
    if let Some(app) = &mut runner.app {
        app.flush();
        crate::native::taskbar_menu_recovery(app.hwnd, 0, false);
    }
    runner.input_router.take();
    runner.app.take();
    if let Some(error) = runner.error {
        Err(error)
    } else {
        Ok(())
    }
}
struct Runner {
    ctx: egui::Context,
    window: Option<Arc<Window>>,
    state: Option<egui_winit::State>,
    app: Option<App>,
    pending: Vec<egui::Event>,
    next_frame: Instant,
    error: Option<String>,
    input_router: Option<crate::native::InputRouter>,
    pointer_known: bool,
    cadence_probe: Option<crate::cadence::Probe>,
    migration: Option<Result<usize, String>>,
}
impl ApplicationHandler<()> for Runner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let small_icon = winit::window::Icon::from_resource(1, Some((16, 16).into()))
            .expect("Embedded small application icon");
        let taskbar_icon = winit::window::Icon::from_resource(1, Some((32, 32).into()))
            .expect("Embedded taskbar application icon");
        let attributes = Window::default_attributes()
            .with_title("NeuralLayer")
            .with_visible(false)
            .with_inner_size(winit::dpi::LogicalSize::new(
                crate::menu::DEFAULT_MENU_SIZE.x + 2. * crate::menu::IDLE_MENU_MARGIN,
                crate::menu::DEFAULT_MENU_SIZE.y + 2. * crate::menu::IDLE_MENU_MARGIN,
            ))
            .with_min_inner_size(winit::dpi::LogicalSize::new(1.0, 1.0))
            .with_decorations(false)
            .with_transparent(true)
            .with_no_redirection_bitmap(true)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_window_icon(Some(small_icon))
            .with_taskbar_icon(Some(taskbar_icon));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.error = Some(error.to_string());
                event_loop.exit();
                return;
            }
        };
        let hwnd = match window.window_handle().unwrap().as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get(),
            _ => unreachable!(),
        };
        let taskbar_error = crate::native::taskbar_identity(hwnd).err();
        match crate::native::InputRouter::new(hwnd) {
            Ok(router) => self.input_router = Some(router),
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
                return;
            }
        }
        let graphics = match Graphics::new(hwnd) {
            Ok(graphics) => graphics,
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
                return;
            }
        };
        self.state = Some(egui_winit::State::new(
            self.ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(16384),
        ));
        self.app = Some(App::new(&self.ctx, hwnd, graphics));
        match self.migration.take() {
            Some(Ok(n)) if n > 0 => self
                .app
                .as_mut()
                .unwrap()
                .notify("Settings and presets from the previous version were adopted."),
            Some(Err(error)) => self.app.as_mut().unwrap().error = Some(error),
            _ => {}
        }
        if let Some(error) = taskbar_error {
            self.app.as_mut().unwrap().error = Some(error);
        }
        window.set_visible(true);
        self.window = Some(window);
    }
    fn user_event(&mut self, _: &ActiveEventLoop, _: ()) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(window) = &self.window else {
            return;
        };
        let Some(state) = &mut self.state else {
            return;
        };
        // Winit can suppress a CursorMoved when re-entering at exactly the
        // previous pixel. Egui discarded that position on CursorLeft; restore
        // it before the first button event so the click/drag is not discarded.
        let entering = match &event {
            WindowEvent::CursorEntered { device_id } => Some(*device_id),
            WindowEvent::MouseInput { device_id, .. }
            | WindowEvent::MouseWheel { device_id, .. }
                if !self.pointer_known =>
            {
                Some(*device_id)
            }
            _ => None,
        };
        if let Some(device_id) = entering {
            if let Some((x, y)) = self
                .app
                .as_ref()
                .and_then(|app| crate::native::cursor_client(app.hwnd))
            {
                let _ = state.on_window_event(
                    window,
                    &WindowEvent::CursorMoved {
                        device_id,
                        position: winit::dpi::PhysicalPosition::new(x as f64, y as f64),
                    },
                );
                self.pointer_known = true;
            }
        }
        match &event {
            WindowEvent::CursorLeft { .. } => self.pointer_known = false,
            WindowEvent::CursorMoved { .. } => self.pointer_known = true,
            _ => {}
        }
        let response = state.on_window_event(window, &event);
        if response.repaint {
            window.request_redraw();
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now < self.next_frame {
                    return;
                }
                let limit = self
                    .app
                    .as_ref()
                    .map(|app| app.frame_limit)
                    .unwrap_or(60)
                    .clamp(15, 120);
                self.next_frame = now + Duration::from_secs_f64(1.0 / limit as f64);
                let size = window.inner_size();
                if size.width == 0 || size.height == 0 || window.is_minimized() == Some(true) {
                    return;
                }
                let mut input = state.take_egui_input(window);
                input.events.append(&mut self.pending);
                let app = self.app.as_mut().unwrap();
                if let Some(probe) = &mut self.cadence_probe {
                    if probe.tick(app) {
                        event_loop.exit();
                        return;
                    }
                }
                let output = self.ctx.run(input, |ctx| app.update(ctx));
                if let Some(router) = &self.input_router {
                    router.update(
                        &self.ctx,
                        !app.paused && app.controls_visible(),
                        app.live_panel || app.menu_visible,
                    );
                }
                if output.viewport_output.values().any(|viewport| {
                    viewport
                        .commands
                        .iter()
                        .any(|command| matches!(command, egui::ViewportCommand::Close))
                }) {
                    event_loop.exit();
                    return;
                }
                state.handle_platform_output(window, output.platform_output);
                let meshes = self.ctx.tessellate(output.shapes, output.pixels_per_point);
                let mut options = app.options;
                options.paused = app.paused as u32;
                options.idle_resize =
                    (app.target.is_none() && app.menu_resizing && !app.live_panel) as u32;
                if let Err(error) = app.graphics.render(
                    [size.width, size.height],
                    output.pixels_per_point,
                    &meshes,
                    &output.textures_delta,
                    options,
                ) {
                    app.error = Some(error);
                    app.menu_visible = true;
                } else {
                    app.frame_rates.draws += 1;
                    if app.frame_rates.due() {
                        app.frame_rates.sample(
                            app.graphics.status().frames,
                            app.graphics.nr_status().evaluations,
                        );
                    }
                }
                for viewport in output.viewport_output.into_values() {
                    for command in viewport.commands {
                        match command {
                            egui::ViewportCommand::Close => event_loop.exit(),
                            egui::ViewportCommand::StartDrag => {
                                let _ = window.drag_window();
                            }
                            egui::ViewportCommand::InnerSize(s) => {
                                let _ = window
                                    .request_inner_size(winit::dpi::LogicalSize::new(s.x, s.y));
                            }
                            egui::ViewportCommand::MousePassthrough(pass) => {
                                crate::native::capture_input_mode(
                                    app.hwnd,
                                    app.target.as_ref(),
                                    pass,
                                );
                            }
                            egui::ViewportCommand::Screenshot => {
                                match app.graphics.screenshot([size.width, size.height]) {
                                    Ok(image) => self.pending.push(egui::Event::Screenshot {
                                        viewport_id: egui::ViewportId::ROOT,
                                        image: Arc::new(image),
                                    }),
                                    Err(error) => app.error = Some(error),
                                }
                                window.request_redraw();
                            }
                            _ => {}
                        }
                    }
                }
                // Without a source, the native hit-test rectangle should hug
                // the menu instead of leaving an invisible video-sized canvas.
                // Compare physical sizes to avoid repeated requests at mixed DPI.
                if app.target.is_none() && !app.live_panel {
                    let logical =
                        app.menu_size + egui::Vec2::splat(2. * crate::menu::IDLE_MENU_MARGIN);
                    let scale = self.ctx.pixels_per_point();
                    let mut desired = winit::dpi::PhysicalSize::new(
                        (logical.x * scale).round().max(1.) as u32,
                        (logical.y * scale).round().max(1.) as u32,
                    );
                    if let Some(monitor) = window.current_monitor() {
                        desired.width = desired.width.min(monitor.size().width);
                        desired.height = desired.height.min(monitor.size().height);
                    }
                    if window.inner_size() != desired {
                        let _ = window.request_inner_size(desired);
                    }
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if Instant::now() >= self.next_frame {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }
}
