#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod cadence;
mod cli;
mod graphics;
mod hotkeys;
mod looks;
mod menu;
mod migrate;
mod native;
mod optiscaler;
mod pages;
mod runner;
mod runtime_import;
mod settings;
mod smoke;
mod theme;

use egui::Vec2;
use hotkeys::{Action, Hotkeys};
use native::{Area, Bounds, Event, Runtime, Target};
use settings::{Debounce, Settings};
use std::collections::HashMap;
use std::time::{Duration, Instant};

fn main() {
    cli::run();
    let outcome = runner::run();
    if let Err(error) = outcome {
        eprintln!("{error}");
        if std::env::args().any(|a| a == "--self-test") {
            let _ = std::fs::write("artifacts/startup-error.txt", &error);
        } else {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    std::ptr::null_mut(),
                    native::wide(&error).as_ptr(),
                    native::wide("NeuralLayer").as_ptr(),
                    0x10,
                );
            }
        }
        std::process::exit(1);
    }
}

/// Pages of the full menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Source,
    Neural,
    Look,
    Compare,
    Settings,
    Diagnostics,
}
impl Page {
    pub const ALL: [Self; 6] = [
        Self::Source,
        Self::Neural,
        Self::Look,
        Self::Compare,
        Self::Settings,
        Self::Diagnostics,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Neural => "Neural",
            Self::Look => "Look",
            Self::Compare => "Compare",
            Self::Settings => "Settings",
            Self::Diagnostics => "Diagnostics",
        }
    }
    pub fn icon(self) -> theme::Icon {
        match self {
            Self::Source => theme::Icon::Source,
            Self::Neural => theme::Icon::Neural,
            Self::Look => theme::Icon::Look,
            Self::Compare => theme::Icon::Compare,
            Self::Settings => theme::Icon::Display,
            Self::Diagnostics => theme::Icon::Diagnostics,
        }
    }
}

/// Screen rectangles of controls, recorded each frame for the live self-tests.
pub struct UiRects {
    pub neural_toggle: egui::Rect,
    pub output_limit: egui::Rect,
    pub compare_slider: egui::Rect,
    pub menu_header: egui::Rect,
    pub stop_button: egui::Rect,
    pub menu_resize: egui::Rect,
}
impl Default for UiRects {
    fn default() -> Self {
        Self {
            neural_toggle: egui::Rect::NOTHING,
            output_limit: egui::Rect::NOTHING,
            compare_slider: egui::Rect::NOTHING,
            menu_header: egui::Rect::NOTHING,
            stop_button: egui::Rect::NOTHING,
            menu_resize: egui::Rect::NOTHING,
        }
    }
}

/// Cached details for a window in the source list.
pub struct SourceInfo {
    pub executable: String,
    pub icon: Option<egui::TextureHandle>,
}

struct App {
    runtime: Runtime,
    graphics: graphics::Graphics,
    options: graphics::Options,
    runtime_folder: std::path::PathBuf,
    /// Saved preferences; compared each frame with the live values.
    settings: Settings,
    settings_save: Debounce,
    /// False in diagnostic runs: they use defaults and never overwrite the user's preferences.
    persist: bool,
    frame_limit: u32,
    frame_rates: cadence::Meter,
    tab: Page,
    nr_options: optiscaler::Options,
    /// What the renderer last received, and what was last written to disk.
    nr_configured: optiscaler::Options,
    nr_saved: optiscaler::Options,
    nr_save: Debounce,
    live_panel: bool,
    selected_effect: String,
    looks: looks::Looks,
    /// The look whose Remove button was clicked once and awaits confirmation.
    confirm_remove: Option<String>,
    neural_requested: bool,
    hwnd: isize,
    windows: Vec<Target>,
    sources: HashMap<isize, SourceInfo>,
    target: Option<Target>,
    fitted: Option<Bounds>,
    idle_bounds: Option<Bounds>,
    always_on_top: bool,
    area: Area,
    outline: bool,
    menu_visible: bool,
    compare_visible: bool,
    /// The compact command bar shown over the canvas instead of the full menu.
    bar_visible: bool,
    quick_settings: bool,
    compare_from_bar: bool,
    last_compare_mode: u32,
    rects: UiRects,
    menu_size: Vec2,
    menu_resize_origin: Option<(egui::Pos2, Vec2)>,
    menu_resizing: bool,
    menu_position: Option<egui::Pos2>,
    footer_height: f32,
    confirm_reset: bool,
    hotkeys: Hotkeys,
    /// The shortcut being recorded on Settings → Shortcuts; global shortcuts
    /// are suspended meanwhile.
    rebinding: Option<Action>,
    hotkey_message: Option<String>,
    /// The controls last hidden were the command bar, so the show/hide
    /// shortcut brings the bar back instead of the full menu.
    prefer_bar: bool,
    logo: egui::TextureHandle,
    paused: bool,
    applied_presentation: Option<(bool, bool)>,
    search: String,
    notice: String,
    notice_at: Instant,
    error: Option<String>,
    refreshed: Instant,
    started: Instant,
    screenshot_path: Option<String>,
    screenshot_requested: bool,
    test: Option<smoke::Scenario>,
}
impl App {
    fn new(ctx: &egui::Context, hwnd: isize, mut graphics: graphics::Graphics) -> Self {
        let args: Vec<_> = std::env::args().collect();
        let diagnostic = args
            .iter()
            .any(|a| a == "--self-test" || a == "--cadence-app" || a == "--screenshot");
        let folder = runtime_folder();
        let settings = if diagnostic {
            Settings::default()
        } else {
            Settings::load(&folder)
        };
        // `--theme` and `--page` help produce documentation screenshots.
        let flag = |name: &str| {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .map(String::as_str)
        };
        theme::apply(
            ctx,
            flag("--theme")
                .and_then(theme::Mode::from_id)
                .unwrap_or(settings.theme),
        );
        let menu_size = flag("--menu-size")
            .and_then(|v| v.split_once('x'))
            .and_then(|(w, h)| Some(Vec2::new(w.parse().ok()?, h.parse().ok()?)))
            .map(|s| s.clamp(Vec2::new(340., 300.), Vec2::new(1180., 920.)));
        let page = flag("--page").and_then(|name| {
            Page::ALL
                .into_iter()
                .find(|p| p.label().eq_ignore_ascii_case(name))
        });
        let screenshot_path = args
            .iter()
            .position(|a| a == "--screenshot")
            .and_then(|i| args.get(i + 1))
            .cloned();
        let mut nr_options = optiscaler::load_options();
        if args.iter().any(|a| a == "--test-custom") {
            nr_options.motion_backend = 1;
        }
        graphics.nr_configure(&nr_options);
        let logo_pixels = image::load_from_memory(include_bytes!("../assets/app.png"))
            .expect("Embedded logo")
            .into_rgba8();
        let logo = ctx.load_texture(
            "app-logo",
            egui::ColorImage::from_rgba_unmultiplied(
                [logo_pixels.width() as usize, logo_pixels.height() as usize],
                logo_pixels.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        let runtime = folder.join(optiscaler::runtime());
        // Older ReShade.ini files only searched the top shader folder.
        let search_error = looks::ensure_search_paths(&runtime).err();
        let looks = looks::Looks::load(&folder);
        let mut options = graphics::Options {
            effect: settings.adjustments as u32,
            sharpness: settings.sharpness,
            saturation: settings.saturation,
            contrast: settings.contrast,
            ..Default::default()
        };
        if looks.selected.is_some() {
            options.reshade = 1;
        }
        let mut error = None;
        if !settings.always_on_top {
            if let Err(e) = native::set_topmost(hwnd, false) {
                error = Some(e);
            }
        }
        let show = flag("--show").map(str::to_owned);
        let mut app = Self {
            logo,
            menu_position: None,
            rects: UiRects::default(),
            tab: if args.iter().any(|a| a == "--neural-page") {
                Page::Neural
            } else {
                page.unwrap_or(Page::Source)
            },
            nr_configured: nr_options,
            nr_saved: nr_options,
            nr_save: Debounce::default(),
            nr_options,
            live_panel: false,
            runtime: Runtime::start(hwnd, ctx.clone(), settings.hotkeys),
            hotkeys: settings.hotkeys,
            rebinding: None,
            hotkey_message: None,
            prefer_bar: settings.prefer_bar,
            graphics,
            options,
            looks,
            confirm_remove: None,
            runtime_folder: folder,
            persist: !diagnostic,
            frame_limit: settings.frame_limit,
            always_on_top: settings.always_on_top,
            area: settings.area,
            outline: settings.outline,
            menu_size: menu_size.unwrap_or(Vec2::from(settings.menu_size)),
            settings,
            settings_save: Debounce::default(),
            frame_rates: cadence::Meter::default(),
            selected_effect: String::new(),
            neural_requested: false,
            hwnd,
            windows: native::windows(),
            sources: HashMap::new(),
            target: None,
            fitted: None,
            idle_bounds: None,
            menu_visible: true,
            compare_visible: false,
            bar_visible: false,
            quick_settings: false,
            compare_from_bar: false,
            last_compare_mode: if nr_options.compare == 0 {
                2
            } else {
                nr_options.compare
            },
            menu_resize_origin: None,
            menu_resizing: false,
            footer_height: 52.,
            confirm_reset: false,
            paused: false,
            applied_presentation: None,
            search: String::new(),
            notice: "Choose a window to place this canvas.".into(),
            notice_at: Instant::now(),
            error,
            refreshed: Instant::now(),
            started: Instant::now(),
            screenshot_path,
            screenshot_requested: false,
            test: args
                .iter()
                .any(|a| a == "--self-test")
                .then(smoke::Scenario::new),
        };
        if let Some(error) = search_error {
            app.error = Some(error);
        }
        match show.as_deref() {
            Some("bar") => {
                app.menu_visible = false;
                app.bar_visible = true;
                app.quick_settings = args.iter().any(|a| a == "--quick-settings");
            }
            Some("compare") => {
                app.menu_visible = false;
                app.compare_visible = true;
                app.nr_options.compare = 2;
            }
            _ => {}
        }
        app
    }

    /// Re-read the imported looks, optionally activating one.
    fn reload_looks(&mut self, select: Option<&str>) {
        let selected = self.looks.selected.clone();
        self.looks = looks::Looks::load(&self.runtime_folder);
        let next = select.map(String::from).or(selected);
        if next != self.looks.selected {
            self.select_look(next);
        }
    }

    fn select_look(&mut self, id: Option<String>) {
        match self.looks.set(id.as_deref(), &self.runtime_folder) {
            Ok(()) if id.is_some() => self.options.reshade = 1,
            Ok(()) => {}
            Err(error) => self.error = Some(error),
        }
    }

    /// Show a transient status message in the menu footer.
    fn notify(&mut self, text: impl Into<String>) {
        self.notice = text.into();
        self.notice_at = Instant::now();
    }

    fn fit(&mut self, target: Target) {
        let previous_bounds = native::surface_bounds(self.hwnd);
        if native::bounds(&target, self.area).is_none() {
            self.error = Some("That source is minimized, closed, or unavailable.".into());
            return;
        }
        if let Err(error) = self
            .graphics
            .capture(target.hwnd, self.area == Area::Content)
        {
            self.error = Some(error);
            return;
        }
        match native::fit_once(self.hwnd, &target, self.area) {
            Ok(rect) => {
                if self.target.is_none() {
                    self.idle_bounds = previous_bounds;
                }
                if let Some(executable) = native::executable(target.pid) {
                    self.settings_remember(&executable);
                }
                self.frame_rates.reset();
                self.fitted = Some(rect);
                self.target = Some(target);
                self.error = None;
                self.notify("Live capture started. The canvas stays put until you fit it again.");
            }
            Err(error) => {
                self.graphics.stop();
                self.error = Some(error);
            }
        }
    }

    fn settings_remember(&mut self, executable: &str) {
        if self.persist {
            self.settings.remember(executable);
            self.settings_save.touch();
        }
    }

    fn restart_capture(&mut self) {
        if let Some(target) = self.target.as_ref() {
            match self
                .graphics
                .capture(target.hwnd, self.area == Area::Content)
            {
                Ok(()) => {
                    self.error = None;
                    self.notify("Capture restarted.");
                }
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn set_always_on_top(&mut self, enabled: bool) {
        match native::set_topmost(self.hwnd, enabled) {
            Ok(()) => {
                self.always_on_top = enabled;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn stop_rendering(&mut self) {
        if self.live_panel {
            let _ = self.graphics.native_overlay(false);
            self.live_panel = false;
        }
        self.graphics.stop();
        self.target = None;
        self.frame_rates.reset();
        self.fitted = None;
        self.menu_position = None;
        self.tab = Page::Source;
        self.error = None;
        // Reapply input even if menu visibility was unchanged: there is no source now.
        self.applied_presentation = None;
        self.show_menu();
        if let Some(bounds) = self.idle_bounds.take() {
            if let Err(error) = native::place_surface(self.hwnd, bounds) {
                self.error = Some(error);
            }
        }
        self.notify("Rendering stopped. Image settings are kept; choose a window to start again.");
    }

    fn show_menu(&mut self) {
        self.compare_visible = false;
        self.bar_visible = false;
        self.menu_visible = true;
        self.paused = false;
        native::restore_surface(self.hwnd);
    }

    /// Collapse the full menu into the command bar over the canvas.
    fn show_bar(&mut self) {
        if self.compare_visible {
            self.close_compare();
        }
        self.menu_visible = false;
        self.bar_visible = true;
        self.paused = false;
    }

    /// Whether the show/hide shortcut should use the command bar now.
    fn prefers_bar(&self) -> bool {
        self.prefer_bar && self.target.is_some()
    }

    /// Bring back the user's preferred controls: the command bar or the menu.
    fn restore_controls(&mut self) {
        if self.prefers_bar() {
            self.show_bar();
        } else {
            self.show_menu();
        }
    }

    /// The current label of a shortcut, e.g. "F8".
    fn key(&self, action: Action) -> String {
        self.hotkeys.label(action)
    }

    fn hide_controls(&mut self) {
        if self.can_hide() {
            // Remember which controls were in use so the shortcut brings
            // back the same ones.
            if self.menu_visible {
                self.prefer_bar = false;
            } else if self.bar_visible {
                self.prefer_bar = true;
            }
            self.menu_visible = false;
            self.bar_visible = false;
        }
    }

    fn load_effects(&mut self, neural: bool) {
        let result = self
            .graphics
            .load_reshade(&self.runtime_folder)
            .and_then(|_| self.graphics.preset(&self.runtime_folder, neural));
        match result {
            Ok(()) => {
                self.options.effect = 0;
                self.options.reshade = 1;
                self.neural_requested = neural;
                self.options.neural = neural as u32;
                if neural {
                    self.tab = Page::Neural;
                }
                self.error = None;
                let text = if neural {
                    format!(
                        "Neural rendering ready · {} to bypass",
                        self.key(Action::Neural)
                    )
                } else {
                    "ReShade effects ready".into()
                };
                self.notify(text);
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn can_hide(&self) -> bool {
        let status = self.runtime.status();
        status.ui_hotkey || status.tray
    }

    fn handle_event(&mut self, event: Event, ctx: &egui::Context) {
        if self.live_panel {
            let _ = self.graphics.native_overlay(false);
            self.live_panel = false;
        }
        match event {
            Event::ToggleMenu => {
                // Toggle whichever controls were last in use: the full menu
                // or the command bar.
                if self.paused || !(self.menu_visible || self.bar_visible) {
                    self.restore_controls();
                } else {
                    self.hide_controls();
                }
            }
            Event::RestoreControls => self.restore_controls(),
            Event::ShowMenu => self.show_menu(),
            Event::ToggleNeural => self.toggle_neural(),
            Event::ToggleCompare => self.toggle_compare(),
            Event::TogglePaused => {
                if self.can_hide() {
                    self.paused = !self.paused;
                    if !self.paused {
                        native::restore_surface(self.hwnd);
                    }
                }
            }
            Event::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    fn controls_visible(&self) -> bool {
        self.menu_visible || self.compare_visible || self.bar_visible
    }

    fn presentation(&mut self, ctx: &egui::Context) {
        if self.always_on_top && !self.paused {
            if let Some(target) = &self.target {
                if let Err(error) = native::keep_above_source(self.hwnd, target) {
                    self.error = Some(error);
                }
            }
        }
        if !native::taskbar_menu_recovery(
            self.hwnd,
            self.runtime.status().receiver,
            !self.menu_visible || self.paused,
        ) {
            self.error =
                Some("Could not enable taskbar menu recovery. Use the tray icon or F8.".into());
        }
        let state = (self.controls_visible(), self.paused);
        if self.applied_presentation != Some(state) {
            // Hiding the menu changes input behavior on the same rendering window.
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(
                !state.0 || self.paused,
            ));
            self.applied_presentation = Some(state);
            ctx.request_repaint();
        }
    }

    /// Send changed neural options to the renderer and persist all
    /// preferences once edits settle.
    fn sync(&mut self, ctx: &egui::Context) {
        if self.nr_options != self.nr_configured {
            self.graphics.nr_configure(&self.nr_options);
            self.nr_configured = self.nr_options;
        }
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        if self.nr_options != self.nr_saved {
            if !self.nr_save.pending() {
                self.nr_save.touch();
            }
            if self.nr_save.due(pointer_down) {
                self.save_nr_options();
            }
        }
        if self.persist {
            let current = self.current_settings();
            if current != self.settings {
                self.settings = current;
                self.settings_save.touch();
            }
            if self.settings_save.due(pointer_down) {
                self.write_settings();
            }
        }
    }

    fn current_settings(&self) -> Settings {
        Settings {
            theme: theme::mode(),
            frame_limit: self.frame_limit,
            always_on_top: self.always_on_top,
            area: self.area,
            outline: self.outline,
            menu_size: [self.menu_size.x, self.menu_size.y],
            adjustments: self.options.effect != 0,
            sharpness: self.options.sharpness,
            saturation: self.options.saturation,
            contrast: self.options.contrast,
            recent: self.settings.recent.clone(),
            hotkeys: self.hotkeys,
            prefer_bar: self.prefer_bar,
        }
    }

    fn write_settings(&mut self) {
        let path = settings::path(&self.runtime_folder);
        if let Err(e) = std::fs::write(&path, self.settings.serialize()) {
            self.error = Some(format!("Could not save preferences: {e}"));
        }
    }

    /// Write anything still waiting for its debounce, e.g. on exit.
    fn flush(&mut self) {
        if self.nr_options != self.nr_saved {
            self.save_nr_options();
        }
        if self.persist {
            let current = self.current_settings();
            if current != self.settings || self.settings_save.take() {
                self.settings = current;
                self.write_settings();
            }
        }
    }
}

impl App {
    fn update(&mut self, ctx: &egui::Context) {
        self.menu_resizing = false;
        if let Err(error) = self.looks.tick(&mut self.graphics) {
            self.looks.error = Some(error);
        }
        self.live_panel = self.graphics.native_overlay_active();
        if self.target.is_some() && self.graphics.status().capture_active == 0 {
            let reason = self.graphics.error();
            self.stop_rendering();
            self.windows = native::windows();
            self.refreshed = Instant::now();
            self.notify(if reason.is_empty() {
                "Capture ended. Select another window.".into()
            } else {
                reason
            });
        }
        while let Ok(event) = self.runtime.events.try_recv() {
            self.handle_event(event, ctx);
        }
        // Abandon an unfinished shortcut recording once its page is gone.
        if self.rebinding.is_some()
            && (!self.menu_visible || self.paused || self.tab != Page::Settings)
        {
            self.finish_rebinding();
        }
        if self.menu_visible && !self.paused && self.refreshed.elapsed() > Duration::from_secs(3) {
            self.refresh_windows();
        }
        ctx.request_repaint_after(Duration::from_millis(if self.menu_visible {
            250
        } else if self.bar_visible {
            500
        } else {
            1000
        }));
        if let Some(mut test) = self.test.take() {
            test.tick(self, ctx);
            self.test = Some(test);
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        if !self.paused {
            if self.outline {
                ctx.layer_painter(egui::LayerId::background()).rect_stroke(
                    ctx.screen_rect().shrink(1.0),
                    0.0,
                    egui::Stroke::new(2.0, theme::p().accent),
                );
            }
            if self.menu_visible {
                if self.live_panel {
                    self.live_panel_toolbar(ctx);
                } else {
                    self.menu(ctx);
                }
            } else if self.bar_visible && !self.live_panel {
                self.command_bar(ctx);
            }
            if self.compare_visible {
                self.compare_overlay(ctx);
            }
        }
        self.sync(ctx);
        self.presentation(ctx);
        if let Some(path) = &self.screenshot_path {
            if !self.screenshot_requested && self.started.elapsed() > Duration::from_secs(2) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                self.screenshot_requested = true;
            }
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    save_image(path, &image);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn refresh_windows(&mut self) {
        self.windows = native::windows();
        self.refreshed = Instant::now();
        let alive: Vec<isize> = self.windows.iter().map(|w| w.hwnd).collect();
        self.sources.retain(|hwnd, _| alive.contains(hwnd));
    }
}

fn save_image(path: &str, image: &egui::ColorImage) {
    let pixels: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|pixel| pixel.to_array())
        .collect();
    image::save_buffer(
        path,
        &pixels,
        image.size[0] as u32,
        image.size[1] as u32,
        image::ColorType::Rgba8,
    )
    .expect("Save rendered surface");
}

fn runtime_folder() -> std::path::PathBuf {
    let executable = std::env::current_exe().expect("Executable path");
    let folder = executable
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    #[cfg(debug_assertions)]
    if !folder.join(optiscaler::runtime()).exists() {
        return std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    }
    folder
}
