//! Live controls for the OptiScaler neural module hosted in our capture renderer.
use crate::hotkeys::Action;
use crate::theme::{self, p, section, slider, slider_fmt, Format, Kind};
use crate::App;
use egui::{Align, Layout, RichText, Ui, Vec2};

pub fn runtime() -> &'static str {
    "runtime-optiscaler"
}

// Mirrors `SpatpitNrOptions` in native/optiscaler.h, which asserts the same size.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub enabled: u32,
    pub preset: u32,
    pub style: u32,
    pub auto_mask: u32,
    pub model_scale: f32,
    pub intensity: f32,
    pub structure: f32,
    pub tone: f32,
    pub skin: f32,
    pub blend: f32,
    pub colour: f32,
    pub max_ratio: f32,
    pub transfer: u32,
    pub compare: u32,
    pub swap: u32,
    pub debug: u32,
    pub split: f32,
    pub zoom: f32,
    pub mv_x: f32,
    pub mv_y: f32,
    pub motion_backend: u32,
    pub motion_protection: u32,
    pub passes: u32,
    pub lighting_stability: u32,
    /// Optional own settings for the 2nd and 3rd passes.
    pub pass_tuning: [PassTuning; 2],
}

// Mirrors `SpatpitNrPassTuning` in native/optiscaler.h.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PassTuning {
    /// 0: follow the main pass settings.
    pub custom: u32,
    pub style: u32,
    pub model_scale: f32,
    pub intensity: f32,
    pub blend: f32,
}
impl Default for PassTuning {
    fn default() -> Self {
        Self {
            custom: 0,
            style: 0,
            model_scale: 0.5,
            intensity: 1.,
            blend: 1.,
        }
    }
}
impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: 1,
            preset: 0,
            style: 0,
            auto_mask: 0,
            model_scale: 0.5,
            intensity: 1.,
            structure: 1.,
            tone: 1.,
            skin: -1.,
            blend: 1.,
            colour: 1.,
            max_ratio: 4.,
            transfer: 1,
            compare: 0,
            swap: 0,
            debug: 0,
            split: 0.5,
            zoom: 1.,
            mv_x: 1.,
            mv_y: 1.,
            // SpatpitNeuralFx is the only motion backend in this release.
            motion_backend: 1,
            // Unreliable-motion protection is not exposed in the menu.
            motion_protection: 0,
            passes: 1,
            lighting_stability: 1,
            pass_tuning: [PassTuning::default(); 2],
        }
    }
}
const _: () = assert!(std::mem::size_of::<PassTuning>() == 20);
const _: () = assert!(std::mem::size_of::<Options>() == 136);

/// The NR styles. Balanced is SpatpitNeuralFx's own tuning.
pub const STYLES: [(u32, &str, &str); 4] = [
    (0, "Default", "The runtime's own look, with the strongest detail."),
    (1, "Natural", "Softer contrast that stays closer to the source."),
    (2, "Cinematic", "Deeper tone and filmic highlights."),
    (
        3,
        "Balanced",
        "Between Default and Natural, with adaptive detail protection. Lighting stabilization applies in 1×.",
    ),
];

impl Options {
    /// Restore model and image tuning while keeping activation, motion
    /// backend, pass count and comparison choices.
    pub fn reset_tuning(&mut self) {
        *self = Self {
            enabled: self.enabled,
            motion_backend: self.motion_backend,
            passes: self.passes,
            compare: self.compare,
            swap: self.swap,
            split: self.split,
            zoom: self.zoom,
            ..Self::default()
        };
    }
}

// Mirrors `SpatpitNrStatus` in native/optiscaler.h.
#[repr(C)]
pub struct Status {
    pub evaluations: u64,
    pub builds: u64,
    pub width: u32,
    pub height: u32,
    pub pending: u32,
    pub active: u32,
    pub message: [u8; 384],
    pub motion_frames: u64,
}
const _: () = assert!(std::mem::size_of::<Status>() == 424);
impl Default for Status {
    fn default() -> Self {
        Self {
            evaluations: 0,
            builds: 0,
            width: 0,
            height: 0,
            pending: 0,
            active: 0,
            message: [0; 384],
            motion_frames: 0,
        }
    }
}
impl Status {
    pub fn message(&self) -> String {
        String::from_utf8_lossy(self.message.split(|c| *c == 0).next().unwrap_or_default())
            .into_owned()
    }
}

impl App {
    pub(crate) fn neural_enabled(&self) -> bool {
        self.options.reshade != 0 && self.options.neural != 0 && self.nr_options.enabled != 0
    }

    pub(crate) fn toggle_neural(&mut self) {
        if self.neural_enabled() {
            self.nr_options.enabled = 0;
        } else {
            if self.options.reshade == 0 || self.options.neural == 0 {
                let tab = self.tab;
                self.load_effects(true);
                self.tab = tab;
                if self.error.is_some() {
                    self.show_menu();
                    return;
                }
            }
            self.nr_options.enabled = 1;
        }
        let key = self.key(Action::Neural);
        self.notify(if self.nr_options.enabled != 0 {
            format!("Neural rendering enabled · {key} to bypass")
        } else {
            format!("Neural rendering bypassed · {key} to enable")
        });
    }

    /// Choose 1–3 neural passes, enabling neural rendering if needed.
    pub(crate) fn set_passes(&mut self, passes: u32) {
        self.nr_options.passes = passes.clamp(1, 3);
        if !self.neural_enabled() {
            self.toggle_neural();
        }
    }

    pub(crate) fn toggle_compare(&mut self) {
        if self.compare_visible && !self.paused {
            self.close_compare();
        } else {
            let _ = self.graphics.native_overlay(false);
            self.live_panel = false;
            self.compare_from_bar = self.compare_from_bar || self.bar_visible;
            self.compare_visible = true;
            self.menu_visible = false;
            self.bar_visible = false;
            self.paused = false;
            if self.nr_options.compare == 0 {
                self.nr_options.compare = self.last_compare_mode;
            }
            crate::native::restore_surface(self.hwnd);
        }
    }

    pub(crate) fn close_compare(&mut self) {
        self.compare_visible = false;
        if std::mem::take(&mut self.compare_from_bar) {
            self.bar_visible = true;
        }
        if self.nr_options.compare != 0 {
            self.last_compare_mode = self.nr_options.compare;
            self.nr_options.compare = 0;
        }
    }

    pub(crate) fn neural_page(&mut self, ui: &mut Ui) {
        let width = ui.available_width();
        if width > 540. {
            ui.columns(2, |cols| {
                self.neural_enable_control(&mut cols[0]);
                self.output_limit_control(&mut cols[1]);
            });
        } else {
            self.neural_enable_control(ui);
            self.output_limit_control(ui);
        }

        section(ui, "Style", |ui| self.style_cards(ui));
        if ui.available_width() > 620. {
            ui.columns(2, |cols| {
                section(&mut cols[0], "Workload", |ui| self.workload_controls(ui));
                section(&mut cols[1], "Image balance", |ui| {
                    self.balance_controls(ui)
                });
            });
        } else {
            section(ui, "Workload", |ui| self.workload_controls(ui));
            section(ui, "Image balance", |ui| self.balance_controls(ui));
        }
        if self.nr_options.passes > 1 {
            section(ui, "Extra passes", |ui| self.extra_pass_controls(ui));
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(theme::button(
                    format!("Compare · {}", self.key(Action::Compare)),
                    Kind::Secondary,
                ))
                .clicked()
            {
                self.toggle_compare();
            }
        });
        theme::hint(
            ui,
            "Uses estimated motion and flat depth. Source-game frame generation, Reflex, engine exposure and separate UI correction are unavailable to screen capture.",
        );
    }

    fn neural_enable_control(&mut self, ui: &mut Ui) {
        if !self
            .runtime_folder
            .join(runtime())
            .join("nvngx_dlssnr.dll")
            .is_file()
        {
            // The NVIDIA runtime is not part of this release; users add their own.
            let response = ui
                .vertical(|ui| {
                    ui.label("Neural rendering needs your NVIDIA runtime file.");
                    let button = ui.add(theme::button("Import neural runtime…", Kind::Primary));
                    theme::hint(
                        ui,
                        "Select nvngx_dlssnr.dll from a source you trust. It is not included in this release.",
                    );
                    button
                })
                .inner;
            self.rects.neural_toggle = response.rect;
            if response.clicked() {
                match crate::runtime_import::pick_and_import(self.hwnd, &self.runtime_folder) {
                    Ok(true) => {
                        self.error = None;
                        self.notify("Runtime imported. Turn on neural rendering to continue.");
                    }
                    Ok(false) => {}
                    Err(error) => self.error = Some(error),
                }
            }
            return;
        }
        let mut enabled = self.neural_enabled();
        let response = ui
            .vertical(|ui| {
                let toggle = theme::toggle(ui, &mut enabled, "Enable neural rendering");
                let s = self.graphics.nr_status();
                let status = if self.options.reshade == 0 || self.options.neural == 0 {
                    "Turn on to load the neural runtime.".to_string()
                } else if self.nr_options.enabled == 0 {
                    format!("Bypassed · {} to resume.", self.key(Action::Neural))
                } else if self.target.is_none() {
                    "Ready · choose a window on Source.".into()
                } else if s.pending != 0 {
                    "Applying model settings…".into()
                } else if s.active != 0 && s.width > 0 {
                    format!("Active · model {} × {}", s.width, s.height)
                } else {
                    s.message()
                };
                theme::hint(ui, status);
                toggle
            })
            .inner;
        self.rects.neural_toggle = response.rect;
        if response.changed() {
            self.toggle_neural();
        }
    }

    pub(crate) fn output_limit_control(&mut self, ui: &mut Ui) {
        let width = ui.available_width().min(320.);
        let group = ui.vertical(|ui| {
            ui.label(RichText::new("Output FPS limit").color(p().ink))
                .on_hover_text("Maximum processing rate for the overlay; does not increase the source game's frame rate.");
            let labels: Vec<(u32, String)> = crate::settings::FRAME_LIMITS
                .iter()
                .map(|v| (*v, v.to_string()))
                .collect();
            let options: Vec<(u32, &str)> = labels.iter().map(|(v, s)| (*v, s.as_str())).collect();
            theme::segmented(ui, "frame_limit", &mut self.frame_limit, &options, width);
            if self.target.is_some() {
                theme::hint(ui, self.frame_rates.label().replace('\n', " · "))
                    .on_hover_text("Measured over the last second. Capture counts received frames; redraw includes reused images. Neural counts model evaluations (each extra pass adds one). These are not monitor presentation FPS or generated frames.");
            }
        });
        self.rects.output_limit = group.response.rect;
    }

    fn style_cards(&mut self, ui: &mut Ui) {
        let c = p();
        let columns = if ui.available_width() > 520. { 4 } else { 2 };
        let gap = 8.;
        let w = (ui.available_width() - gap * (columns as f32 - 1.)) / columns as f32;
        egui::Grid::new("style_cards")
            .num_columns(columns)
            .spacing(Vec2::splat(gap))
            .show(ui, |ui| {
                for (n, (style, name, about)) in STYLES.iter().enumerate() {
                    let selected = self.nr_options.style == *style;
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(w, 80.), egui::Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::RadioButton,
                            true,
                            selected,
                            *name,
                        )
                    });
                    let fill = if selected { c.nav_active } else { c.raised };
                    let stroke = if selected || response.has_focus() {
                        c.accent
                    } else if response.hovered() {
                        c.muted
                    } else {
                        c.line
                    };
                    ui.painter()
                        .rect(rect, 9., fill, egui::Stroke::new(1., stroke));
                    let inner = rect.shrink(10.);
                    ui.painter().text(
                        inner.left_top(),
                        egui::Align2::LEFT_TOP,
                        *name,
                        egui::FontId::proportional(14.5),
                        c.ink,
                    );
                    let summary = about.split(". ").next().unwrap_or(about).to_string();
                    let galley = ui.painter().layout(
                        summary,
                        egui::FontId::proportional(12.),
                        c.muted,
                        inner.width(),
                    );
                    ui.painter()
                        .galley(inner.left_top() + Vec2::new(0., 22.), galley, c.muted);
                    let response = response.on_hover_text(*about);
                    if response.clicked() {
                        self.nr_options.style = *style;
                    }
                    if (n + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
    }

    /// Compact style choice for the command bar.
    pub(crate) fn style_combo(&mut self, ui: &mut Ui, id: &str, width: f32) {
        let current = STYLES
            .iter()
            .find(|s| s.0 == self.nr_options.style)
            .map_or("Default", |s| s.1);
        egui::ComboBox::from_id_salt(id)
            .width(width)
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (style, name, about) in STYLES {
                    ui.selectable_value(&mut self.nr_options.style, style, name)
                        .on_hover_text(about);
                }
            });
    }

    pub(crate) fn style_segmented(&mut self, ui: &mut Ui, id: &str, width: f32) {
        let options: Vec<(u32, &str)> = STYLES
            .iter()
            .map(|s| (s.0, if s.0 == 2 { "Cinema" } else { s.1 }))
            .collect();
        let mut style = self.nr_options.style;
        if theme::segmented(ui, id, &mut style, &options, width).changed {
            self.nr_options.style = style;
        }
    }

    /// The main pass's model resolution slider and resulting model size.
    pub(crate) fn model_scale_control(&mut self, ui: &mut Ui) {
        model_scale_slider(ui, &mut self.nr_options.model_scale);
        let s = self.graphics.nr_status();
        let size = if s.width > 0 && s.pending == 0 && s.active != 0 {
            Some((s.width, s.height))
        } else {
            self.model_size(self.nr_options.model_scale)
        };
        theme::hint(ui, model_size_text(size));
    }

    /// The model size the renderer will use for `scale`, once a source is fitted.
    fn model_size(&self, scale: f32) -> Option<(u32, u32)> {
        self.fitted.map(|b| {
            (
                ((b.width as f32 * scale) as u32 & !7).max(32),
                ((b.height as f32 * scale) as u32 & !7).max(32),
            )
        })
    }

    /// Own settings for the 2nd and 3rd passes, e.g. a lower model resolution.
    fn extra_pass_controls(&mut self, ui: &mut Ui) {
        theme::hint(
            ui,
            "Each extra pass can follow the main settings or use its own, for example a lower model resolution to save GPU time.",
        );
        let count = (self.nr_options.passes as usize - 1).min(2);
        if count == 2 && ui.available_width() > 620. {
            ui.columns(2, |cols| {
                for (i, col) in cols.iter_mut().enumerate() {
                    self.pass_tuning_controls(col, i);
                }
            });
        } else {
            for i in 0..count {
                self.pass_tuning_controls(ui, i);
                ui.add_space(6.);
            }
        }
    }

    fn pass_tuning_controls(&mut self, ui: &mut Ui, index: usize) {
        let main = self.nr_options;
        let size = self.model_size(main.pass_tuning[index].model_scale);
        ui.push_id(("pass_tuning", index), |ui| {
            let t = &mut self.nr_options.pass_tuning[index];
            theme::overline(ui, &format!("Pass {}", index + 2));
            let mut custom = t.custom != 0;
            if theme::toggle(ui, &mut custom, "Own settings").changed() {
                if custom {
                    // Start from the main pass so turning this on changes nothing yet.
                    *t = PassTuning {
                        custom: 1,
                        style: main.style,
                        model_scale: main.model_scale,
                        intensity: main.intensity,
                        blend: main.blend,
                    };
                } else {
                    t.custom = 0;
                }
            }
            if t.custom == 0 {
                theme::hint(ui, "Uses the main pass settings.");
                return;
            }
            model_scale_slider(ui, &mut t.model_scale);
            theme::hint(ui, model_size_text(size));
            let options: Vec<(u32, &str)> = STYLES
                .iter()
                .map(|s| (s.0, if s.0 == 2 { "Cinema" } else { s.1 }))
                .collect();
            let width = ui.available_width();
            theme::segmented(ui, "style", &mut t.style, &options, width);
            slider(ui, "Intensity", &mut t.intensity, 0.0..=2., 1.);
            slider_fmt(
                ui,
                "Neural blend",
                &mut t.blend,
                0.0..=1.,
                1.,
                Format::Percent,
            );
        });
    }

    fn workload_controls(&mut self, ui: &mut Ui) {
        let width = ui.available_width();
        self.model_scale_control(ui);
        ui.add_space(4.);
        ui.horizontal(|ui| {
            ui.label("Passes");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let c = p();
                ui.spacing_mut().item_spacing.x = 3.;
                for n in (1..=3).rev() {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(14., 6.), egui::Sense::hover());
                    ui.painter().rect_filled(
                        r,
                        2.,
                        if n <= self.nr_options.passes {
                            c.warn
                        } else {
                            c.line
                        },
                    );
                }
                ui.label(RichText::new("GPU cost ").size(12.).color(c.muted));
            });
        });
        let mut passes = self.nr_options.passes;
        if theme::segmented(
            ui,
            "passes",
            &mut passes,
            &[(1, "Normal 1×"), (2, "2×"), (3, "3×")],
            width,
        )
        .changed
        {
            self.set_passes(passes);
        }
        theme::hint(
            ui,
            "Each extra pass reprocesses the previous result: more GPU time and memory, and artifacts can grow.",
        );
        ui.add_space(4.);
        ui.label("Enlargement");
        let mut transfer = self.nr_options.transfer;
        if theme::segmented(
            ui,
            "transfer",
            &mut transfer,
            &[(0, "Classic"), (1, "Matched residual")],
            width,
        )
        .changed
        {
            self.nr_options.transfer = transfer;
        }
    }

    fn balance_controls(&mut self, ui: &mut Ui) {
        const PRESETS: [&str; 4] = ["Default", "Preset 1", "Preset 2", "Preset 3"];
        let o = &mut self.nr_options;
        slider(ui, "Overall intensity", &mut o.intensity, 0.0..=2., 1.);
        slider_fmt(
            ui,
            "Neural blend",
            &mut o.blend,
            0.0..=1.,
            1.,
            Format::Percent,
        );
        slider_fmt(
            ui,
            "Colour transfer",
            &mut o.colour,
            0.0..=1.,
            1.,
            Format::Percent,
        );
        egui::CollapsingHeader::new("Advanced")
            .id_salt("neural_advanced")
            .show(ui, |ui| {
                ui.label("NR preset");
                egui::ComboBox::from_id_salt("nr_preset")
                    .width(ui.available_width().min(240.))
                    .selected_text(PRESETS[(o.preset as usize).min(3)])
                    .show_ui(ui, |ui| {
                        for (i, name) in PRESETS.iter().enumerate() {
                            ui.selectable_value(&mut o.preset, i as u32, *name);
                        }
                    });
                ui.add_space(4.);
                slider(ui, "Structure", &mut o.structure, 0.0..=2., 1.);
                slider(ui, "Local tone", &mut o.tone, 0.0..=2., 1.);
                slider(
                    ui,
                    "Character / skin structure",
                    &mut o.skin,
                    -1.0..=2.,
                    -1.,
                );
                theme::hint(ui, "−1 follows the model's structure setting.");
                let mut mask = o.auto_mask != 0;
                if theme::toggle(ui, &mut mask, "Automatic character mask").changed() {
                    o.auto_mask = mask as u32;
                }
                ui.add_space(4.);
                slider_fmt(
                    ui,
                    "Maximum brightening",
                    &mut o.max_ratio,
                    1.0..=16.,
                    4.,
                    Format::Times,
                );
            });
    }

    /// Write the neural options now. Edits are also saved automatically once
    /// they settle; see `App::sync`.
    pub(crate) fn save_nr_options(&mut self) {
        // Named, portable configuration: never write raw FFI struct bytes.
        let o = self.nr_options;
        let values = [
            ("enabled", o.enabled as f32),
            ("preset", o.preset as f32),
            ("style", o.style as f32),
            ("auto_mask", o.auto_mask as f32),
            ("model_scale", o.model_scale),
            ("intensity", o.intensity),
            ("structure", o.structure),
            ("tone", o.tone),
            ("skin", o.skin),
            ("blend", o.blend),
            ("colour", o.colour),
            ("max_ratio", o.max_ratio),
            ("transfer", o.transfer as f32),
            ("compare", o.compare as f32),
            ("swap", o.swap as f32),
            ("debug", o.debug as f32),
            ("split", o.split),
            ("zoom", o.zoom),
            ("mv_x", o.mv_x),
            ("mv_y", o.mv_y),
            ("motion_backend", o.motion_backend as f32),
            ("motion_protection", o.motion_protection as f32),
            ("passes", o.passes as f32),
        ];
        let mut content = values
            .iter()
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect::<String>();
        for (i, t) in o.pass_tuning.iter().enumerate() {
            let n = i + 2;
            content += &format!(
                "pass{n}_custom={}\npass{n}_style={}\npass{n}_model_scale={}\npass{n}_intensity={}\npass{n}_blend={}\n",
                t.custom, t.style, t.model_scale, t.intensity, t.blend
            );
        }
        match std::fs::write(self.runtime_folder.join("SpatpitOptiScaler.ini"), content) {
            Ok(()) => {
                self.nr_saved = o;
                self.nr_save.take();
            }
            Err(e) => self.error = Some(format!("Could not save neural settings: {e}")),
        }
    }
}
/// Model resolution in whole percent steps; the renderer rounds the model
/// size to multiples of 8 pixels anyway.
fn model_scale_slider(ui: &mut Ui, scale: &mut f32) {
    slider_fmt(
        ui,
        "Model resolution",
        scale,
        0.25..=1.,
        0.5,
        Format::Percent,
    );
    *scale = (*scale * 100.).round() / 100.;
}

fn model_size_text(size: Option<(u32, u32)>) -> String {
    match size {
        Some((w, h)) => format!("Model {w} × {h} · the canvas stays full size"),
        None => "Lower values evaluate fewer pixels; the canvas stays full size.".into(),
    }
}

pub fn load_options() -> Options {
    let mut o = Options::default();
    let text = std::fs::read_to_string(crate::runtime_folder().join("SpatpitOptiScaler.ini"))
        .unwrap_or_default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let Ok(v) = v.parse::<f32>() else { continue };
        if !v.is_finite() {
            continue;
        }
        match k {
            "enabled" => o.enabled = (v != 0.) as u32,
            "preset" => o.preset = (v as u32).min(3),
            "style" => o.style = (v as u32).min(3),
            "auto_mask" => o.auto_mask = (v != 0.) as u32,
            "model_scale" => o.model_scale = v.clamp(0.25, 1.),
            "intensity" => o.intensity = v.clamp(0., 2.),
            "structure" => o.structure = v.clamp(0., 2.),
            "tone" => o.tone = v.clamp(0., 2.),
            "skin" => o.skin = v.clamp(-1., 2.),
            "blend" => o.blend = v.clamp(0., 1.),
            "colour" => o.colour = v.clamp(0., 1.),
            "max_ratio" => o.max_ratio = v.clamp(1., 16.),
            "transfer" => o.transfer = (v as u32).min(1),
            "compare" => o.compare = (v as u32).min(2),
            "swap" => o.swap = (v != 0.) as u32,
            "debug" => o.debug = (v as u32).min(6),
            "split" => o.split = v.clamp(0., 1.),
            "zoom" => o.zoom = v.clamp(1., 2.),
            "mv_x" => o.mv_x = v.clamp(-4., 4.),
            "mv_y" => o.mv_y = v.clamp(-4., 4.),
            // This release always uses SpatpitNeuralFx; ignore older choices.
            "motion_backend" => {}
            // Protection is not exposed in the menu; ignore old saved values.
            "motion_protection" => {}
            "passes" => o.passes = (v as u32).clamp(1, 3),
            // Lighting correction is automatic. Ignore the old checkbox value;
            // the native replay harness retains a diagnostic bypass for A/B tests.
            "lighting_stability" => {}
            key => {
                let pass = match key.get(..6) {
                    Some("pass2_") => 0,
                    Some("pass3_") => 1,
                    _ => continue,
                };
                let t = &mut o.pass_tuning[pass];
                match &key[6..] {
                    "custom" => t.custom = (v != 0.) as u32,
                    "style" => t.style = (v as u32).min(3),
                    "model_scale" => t.model_scale = v.clamp(0.25, 1.),
                    "intensity" => t.intensity = v.clamp(0., 2.),
                    "blend" => t.blend = v.clamp(0., 1.),
                    _ => {}
                }
            }
        }
    }
    o
}
