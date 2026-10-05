//! Menu pages other than Neural (see `optiscaler`).
use crate::graphics::Uniform;
use crate::hotkeys::{Action, Hotkey};
use crate::native::{self, Area, Target};
use crate::theme::{self, p, section, slider, slider_fmt, Format, Icon, Kind};
use crate::{App, SourceInfo};
use egui::{Align, Layout, Rect, RichText, Sense, Stroke, Ui, Vec2};

impl App {
    // ----------------------------------------------------------------- source --

    pub(crate) fn source_page(&mut self, ui: &mut Ui) {
        if ui.available_width() > 700. {
            let side = 260.;
            let list = ui.available_width() - side - 16.;
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(list);
                    self.window_list(ui);
                });
                ui.add_space(8.);
                ui.vertical(|ui| {
                    ui.set_width(side);
                    self.capture_settings(ui);
                });
            });
        } else {
            self.window_list(ui);
            self.capture_settings(ui);
        }
    }

    fn window_list(&mut self, ui: &mut Ui) {
        let c = p();
        ui.horizontal(|ui| {
            let width = ui.available_width() - 42.;
            egui::Frame::none()
                .fill(c.inset)
                .stroke(Stroke::new(1., c.line))
                .rounding(8.)
                .inner_margin(egui::Margin::symmetric(10., 4.))
                .show(ui, |ui| {
                    ui.set_width(width - 22.);
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(16.), Sense::hover());
                        theme::paint_icon(ui.painter(), r, Icon::Search, c.muted);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text("Search windows or processes")
                                .frame(false)
                                .desired_width(f32::INFINITY),
                        );
                    });
                });
            if theme::icon_button(ui, Icon::Refresh, "Refresh the window list").clicked() {
                self.refresh_windows();
            }
        });
        let query = self.search.to_lowercase();
        let windows: Vec<Target> = self.windows.clone();
        for window in &windows {
            self.ensure_source_info(ui.ctx(), window);
        }
        let matches = |w: &Target, info: Option<&SourceInfo>| {
            query.is_empty()
                || w.title.to_lowercase().contains(&query)
                || info.is_some_and(|i| i.executable.to_lowercase().contains(&query))
        };
        let visible: Vec<&Target> = windows
            .iter()
            .filter(|w| matches(w, self.sources.get(&w.hwnd)))
            .collect();
        let recent_rank = |w: &Target| {
            self.sources.get(&w.hwnd).and_then(|info| {
                self.settings
                    .recent
                    .iter()
                    .position(|e| e.eq_ignore_ascii_case(&info.executable))
            })
        };
        let mut recent: Vec<&Target> = visible
            .iter()
            .copied()
            .filter(|w| recent_rank(w).is_some())
            .collect();
        recent.sort_by_key(|w| recent_rank(w));
        recent.truncate(3);
        theme::hint(
            ui,
            format!(
                "{} windows · selecting one fits the canvas over it once",
                visible.len()
            ),
        );
        let mut chosen = None;
        if !recent.is_empty() {
            theme::overline(ui, "Recent");
            for window in &recent {
                if self.window_row(ui, window, true) {
                    chosen = Some((*window).clone());
                }
            }
            ui.add_space(4.);
            theme::overline(ui, "All windows");
        }
        if visible.is_empty() {
            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label("No matching windows");
                theme::hint(ui, "Open an app or game, then refresh.");
            });
        }
        for window in &visible {
            if recent.iter().any(|r| r.hwnd == window.hwnd) {
                continue;
            }
            if self.window_row(ui, window, false) {
                chosen = Some((*window).clone());
            }
        }
        if let Some(window) = chosen {
            self.fit(window);
        }
    }

    fn ensure_source_info(&mut self, ctx: &egui::Context, window: &Target) {
        if self.sources.contains_key(&window.hwnd) {
            return;
        }
        let icon = native::window_icon(window.hwnd, 32).map(|rgba| {
            ctx.load_texture(
                format!("window-icon-{}", window.hwnd),
                egui::ColorImage::from_rgba_premultiplied([32, 32], &rgba),
                egui::TextureOptions::LINEAR,
            )
        });
        self.sources.insert(
            window.hwnd,
            SourceInfo {
                executable: native::executable(window.pid).unwrap_or_default(),
                icon,
            },
        );
    }

    /// One selectable window; returns true when clicked.
    fn window_row(&self, ui: &mut Ui, window: &Target, recent: bool) -> bool {
        let c = p();
        let selected = self
            .target
            .as_ref()
            .is_some_and(|w| w.hwnd == window.hwnd && w.pid == window.pid);
        let info = self.sources.get(&window.hwnd);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 54.), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, &window.title)
        });
        let fill = if selected {
            c.nav_active
        } else if response.hovered() {
            c.raised
        } else {
            c.panel
        };
        let stroke = if selected || response.has_focus() {
            c.accent
        } else {
            c.line_soft
        };
        ui.painter().rect(rect, 9., fill, Stroke::new(1., stroke));
        let icon = Rect::from_min_size(
            egui::pos2(rect.left() + 12., rect.center().y - 14.),
            Vec2::splat(28.),
        );
        match info.and_then(|i| i.icon.as_ref()) {
            Some(texture) => {
                ui.painter().image(
                    texture.id(),
                    icon,
                    Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    egui::Color32::WHITE,
                );
            }
            None => {
                ui.painter().rect_filled(icon, 7., c.selected);
                let letter = window
                    .title
                    .chars()
                    .find(|c| c.is_alphanumeric())
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
                ui.painter().text(
                    icon.center(),
                    egui::Align2::CENTER_CENTER,
                    letter,
                    egui::FontId::proportional(14.),
                    c.ink,
                );
            }
        }
        let text_left = icon.right() + 12.;
        let badge = if selected {
            Some(("LIVE", c.good))
        } else if recent {
            Some(("RECENT", c.accent))
        } else {
            None
        };
        let badge_width = if badge.is_some() { 64. } else { 0. };
        let text_width = (rect.right() - text_left - 12. - badge_width).max(20.);
        let title = theme::single_line(ui, &window.title, 14., c.ink, text_width);
        let title_row = title.size();
        ui.painter()
            .galley(egui::pos2(text_left, rect.top() + 9.), title, c.ink);
        let size = native::bounds(window, Area::Window)
            .map(|b| format!(" · {}×{}", b.width, b.height))
            .unwrap_or_default();
        let executable = info
            .map(|i| i.executable.as_str())
            .filter(|e| !e.is_empty())
            .unwrap_or("PID");
        let detail = format!("{executable}{size} · PID {}", window.pid);
        let mut job = egui::text::LayoutJob::single_section(
            detail,
            egui::TextFormat::simple(egui::FontId::monospace(11.), c.muted),
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(text_width);
        let detail = ui.fonts(|f| f.layout_job(job));
        ui.painter().galley(
            egui::pos2(text_left, rect.top() + 13. + title_row.y),
            detail,
            c.muted,
        );
        if let Some((text, color)) = badge {
            let galley =
                ui.painter()
                    .layout_no_wrap(text.into(), egui::FontId::proportional(10.5), color);
            let chip = Rect::from_min_size(
                egui::pos2(
                    rect.right() - 12. - galley.size().x - 12.,
                    rect.center().y - 10.,
                ),
                Vec2::new(galley.size().x + 12., 20.),
            );
            ui.painter().rect_stroke(chip, 5., Stroke::new(1., color));
            ui.painter()
                .galley(chip.center() - galley.size() / 2., galley, color);
        }
        response.on_hover_text(&window.title).clicked()
    }

    fn capture_settings(&mut self, ui: &mut Ui) {
        section(ui, "Capture area", |ui| {
            let width = ui.available_width();
            let mut area = self.area;
            theme::segmented(
                ui,
                "capture_area",
                &mut area,
                &[(Area::Content, "Content"), (Area::Window, "Whole window")],
                width,
            );
            self.area = area;
            theme::hint(
                ui,
                if self.area == Area::Content {
                    "Skips the title bar and borders. Applies on the next fit."
                } else {
                    "Includes the title bar and borders. Applies on the next fit."
                },
            );
            ui.horizontal_wrapped(|ui| {
                let running = self.target.is_some();
                if ui
                    .add_enabled(running, theme::button("Fit again", Kind::Secondary))
                    .on_hover_text(
                        "Match the canvas to the source window's current position and size.",
                    )
                    .clicked()
                {
                    if let Some(target) = self.target.clone() {
                        self.fit(target);
                    }
                }
                if ui
                    .add_enabled(running, theme::button("Restart capture", Kind::Secondary))
                    .on_hover_text(
                        "Restart the source stream without moving the canvas or changing effects.",
                    )
                    .clicked()
                {
                    self.restart_capture();
                }
            });
        });
        section(ui, "How it works", |ui| {
            for (n, step) in [
                "Pick a window; the canvas snaps over it once.".to_string(),
                format!(
                    "Turn on neural rendering with {}.",
                    self.key(Action::Neural)
                ),
                format!(
                    "Hide the controls with {}; clicks pass through to your game.",
                    self.key(Action::Menu)
                ),
            ]
            .iter()
            .enumerate()
            {
                ui.horizontal_top(|ui| {
                    ui.label(
                        RichText::new(format!("{}.", n + 1))
                            .color(p().accent)
                            .strong(),
                    );
                    theme::hint(ui, step.clone());
                });
            }
        });
    }

    // ------------------------------------------------------------------- look --

    /// Compact look picker for the command bar's quick settings.
    pub(crate) fn preset_choice(&mut self, ui: &mut Ui, id: &str, width: f32) {
        let current = self
            .looks
            .active()
            .map_or("Off".to_string(), |l| l.name.clone());
        let mut choice = self.looks.selected.clone();
        egui::ComboBox::from_id_salt(id)
            .width((width - 8.).max(80.))
            .selected_text(current)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut choice, None, "Off");
                for look in &self.looks.list {
                    ui.add_enabled_ui(look.available, |ui| {
                        ui.selectable_value(&mut choice, Some(look.id.clone()), &look.name);
                    });
                }
            });
        if choice != self.looks.selected {
            self.select_look(choice);
        }
    }

    pub(crate) fn look_page(&mut self, ui: &mut Ui) {
        theme::page_title(
            ui,
            "Look",
            "ReShade presets applied before neural rendering. One look runs at a time.",
        );
        section(ui, "Looks", |ui| {
            let c = p();
            let mut choose: Option<Option<String>> = None;
            if self.look_row(ui, None, "Off", "No preset", true) {
                choose = Some(None);
            }
            let rows: Vec<(String, String, String, bool)> = self
                .looks
                .list
                .iter()
                .map(|l| {
                    let detail = if l.available {
                        format!(
                            "{} effect{}",
                            l.effect_count(),
                            if l.effect_count() == 1 { "" } else { "s" }
                        )
                    } else {
                        "Some shader files are missing · re-import it".into()
                    };
                    (l.id.clone(), l.name.clone(), detail, l.available)
                })
                .collect();
            let mut remove = None;
            for (id, name, detail, available) in rows {
                ui.horizontal(|ui| {
                    let confirming = self.confirm_remove.as_deref() == Some(id.as_str());
                    let button_width = if confirming { 150. } else { 40. };
                    ui.allocate_ui_with_layout(
                        Vec2::new((ui.available_width() - button_width).max(80.), 54.),
                        Layout::top_down(Align::Min),
                        |ui| {
                            if self.look_row(ui, Some(&id), &name, &detail, available) {
                                choose = Some(Some(id.clone()));
                            }
                        },
                    );
                    if confirming {
                        if ui.add(theme::button("Remove", Kind::Danger)).clicked() {
                            remove = Some(id.clone());
                        }
                        if ui.add(theme::button("Keep", Kind::Ghost)).clicked() {
                            self.confirm_remove = None;
                        }
                    } else if theme::icon_button(ui, Icon::Close, &format!("Remove {name}"))
                        .clicked()
                    {
                        self.confirm_remove = Some(id.clone());
                    }
                });
            }
            if let Some(id) = remove {
                self.confirm_remove = None;
                match self.looks.remove(&id, &self.runtime_folder) {
                    Ok(()) => self.notify("Look removed."),
                    Err(error) => self.error = Some(error),
                }
            }
            if let Some(choice) = choose {
                if choice != self.looks.selected {
                    self.select_look(choice);
                }
            }
            if let Some(error) = &self.looks.error {
                ui.colored_label(c.stop, error);
            } else if self.looks.selected.is_some() && !self.looks.ready {
                theme::hint(ui, "Loading the look…");
            }
            ui.add_space(4.);
            if ui
                .add(theme::button("Import preset…", Kind::Primary))
                .on_hover_text("Choose a ReShade preset .ini or the .zip you downloaded")
                .clicked()
            {
                self.import_look();
            }
            theme::hint(
                ui,
                "Pick the preset's .ini (keep its reshade-shaders folder next to it) or the .zip you downloaded. Only the effects the preset uses are copied, with their include files and textures. Effects that need the game's depth buffer can't work on captured video.",
            );
            if self.looks.list.is_empty() {
                theme::hint(ui, "No looks imported yet.");
            }
        });
        section(ui, "Image adjustments", |ui| self.image_settings(ui));
        section(ui, "ReShade effects", |ui| self.effect_controls(ui));
    }

    /// One selectable look; returns true when clicked.
    fn look_row(
        &self,
        ui: &mut Ui,
        id: Option<&str>,
        name: &str,
        detail: &str,
        available: bool,
    ) -> bool {
        let c = p();
        let selected = self.looks.selected.as_deref() == id;
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 50.),
            if available {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, available, selected, name)
        });
        let fill = if selected {
            c.nav_active
        } else if response.hovered() && available {
            c.raised
        } else {
            c.panel
        };
        let stroke = if selected || response.has_focus() {
            c.accent
        } else {
            c.line_soft
        };
        ui.painter().rect(rect, 9., fill, Stroke::new(1., stroke));
        let dot = egui::pos2(rect.left() + 20., rect.center().y);
        ui.painter().circle_stroke(
            dot,
            7.,
            Stroke::new(1.5, if selected { c.accent } else { c.muted }),
        );
        if selected {
            ui.painter().circle_filled(dot, 3.5, c.accent);
        }
        let left = rect.left() + 38.;
        let width = (rect.right() - left - 12.).max(20.);
        let title = theme::single_line(
            ui,
            name,
            14.,
            if available { c.ink } else { c.muted },
            width,
        );
        let title_height = title.size().y;
        ui.painter()
            .galley(egui::pos2(left, rect.top() + 8.), title, c.ink);
        let sub = theme::single_line(
            ui,
            detail,
            12.,
            if available { c.muted } else { c.warn },
            width,
        );
        ui.painter().galley(
            egui::pos2(left, rect.top() + 10. + title_height),
            sub,
            c.muted,
        );
        response.clicked()
    }

    fn import_look(&mut self) {
        let path = match crate::runtime_import::pick_preset(self.hwnd) {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        match crate::looks::import(&path, &self.runtime_folder) {
            Ok(result) => {
                let mut text = format!("Imported {}", result.names.join(", "));
                if !result.missing.is_empty() {
                    text += &format!(" · skipped missing {}", result.missing.join(", "));
                }
                self.error = None;
                self.notify(text);
                self.reload_looks(result.ids.first().map(String::as_str));
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn image_settings(&mut self, ui: &mut Ui) {
        let mut enabled = self.options.effect != 0;
        if theme::toggle(
            ui,
            &mut enabled,
            "Built-in sharpen, saturation and contrast",
        )
        .changed()
        {
            self.options.effect = enabled as u32;
        }
        if enabled {
            slider(ui, "Sharpness", &mut self.options.sharpness, 0.0..=2.0, 0.3);
            slider(
                ui,
                "Saturation",
                &mut self.options.saturation,
                0.0..=2.0,
                1.,
            );
            slider(ui, "Contrast", &mut self.options.contrast, 0.5..=1.5, 1.);
            let mut comparison = self.options.comparison != 0;
            theme::toggle(ui, &mut comparison, "Compare original / adjusted");
            self.options.comparison = comparison as u32;
            if comparison {
                slider_fmt(
                    ui,
                    "Divider",
                    &mut self.options.split,
                    0.0..=1.0,
                    0.5,
                    Format::Percent,
                );
            }
        }
    }

    fn effect_controls(&mut self, ui: &mut Ui) {
        if self.graphics.status().reshade_loaded == 0 {
            theme::hint(ui, "ReShade loads with neural rendering or on request.");
            if ui
                .add(theme::button("Load ReShade effects", Kind::Primary))
                .clicked()
            {
                self.load_effects(false);
            }
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(theme::button("Save preset", Kind::Secondary))
                .clicked()
            {
                match self.graphics.save_preset() {
                    Ok(()) => self.notify("ReShade settings saved in this app's preset."),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add(theme::button("Reload saved preset", Kind::Secondary))
                .clicked()
            {
                if let Err(error) = self
                    .graphics
                    .preset(&self.runtime_folder, self.neural_requested)
                {
                    self.error = Some(error);
                }
            }
            if ui
                .add(theme::button("Native ReShade panel", Kind::Ghost))
                .on_hover_text("Open ReShade's own overlay for advanced shader controls.")
                .clicked()
            {
                match self.graphics.native_overlay(true) {
                    Ok(()) => self.live_panel = true,
                    Err(error) => self.error = Some(error),
                }
            }
        });
        let techniques: Vec<_> = self
            .graphics
            .techniques()
            .into_iter()
            .filter(|t| crate::menu::user_effect(&t.effect))
            .collect();
        if !crate::menu::user_effect(&self.selected_effect) {
            self.selected_effect.clear();
        }
        if techniques.is_empty() {
            theme::hint(ui, "No additional color effects loaded.");
            if !self.neural_requested
                && ui
                    .add(theme::button("Load color effects", Kind::Secondary))
                    .clicked()
            {
                self.load_effects(false);
            }
        }
        egui::ScrollArea::vertical()
            .id_salt("technique_list")
            .max_height(200.0)
            .show(ui, |ui| {
                for technique in techniques {
                    ui.horizontal(|ui| {
                        let mut enabled = technique.enabled;
                        if ui.checkbox(&mut enabled, "").changed() {
                            if let Err(error) = self.graphics.set_technique(
                                &technique.effect,
                                &technique.name,
                                enabled,
                            ) {
                                self.error = Some(error);
                            } else if enabled {
                                self.options.reshade = 1;
                            }
                        }
                        if ui
                            .selectable_label(
                                self.selected_effect == technique.effect,
                                &technique.name,
                            )
                            .on_hover_text(format!(
                                "{} · click to edit its settings",
                                technique.effect
                            ))
                            .clicked()
                        {
                            self.selected_effect = technique.effect.clone();
                        }
                    });
                }
            });
        if self.selected_effect.is_empty() {
            return;
        }
        ui.separator();
        ui.horizontal(|ui| {
            theme::heading(ui, &self.selected_effect);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::icon_button(ui, Icon::Close, "Close shader settings").clicked() {
                    self.selected_effect.clear();
                }
            });
        });
        if self.selected_effect.is_empty() {
            return;
        }
        let uniforms = self.graphics.uniforms(&self.selected_effect);
        if uniforms.is_empty() {
            theme::hint(
                ui,
                "This shader has no editable settings, or is still loading.",
            );
        }
        egui::ScrollArea::vertical()
            .id_salt("uniform_list")
            .max_height(300.0)
            .show(ui, |ui| {
                let mut category = String::new();
                for mut uniform in uniforms {
                    if !uniform.category.is_empty() && uniform.category != category {
                        ui.add_space(4.);
                        theme::overline(ui, &uniform.category);
                        category = uniform.category.clone();
                    }
                    let id = uniform.name.clone();
                    let edit = ui.push_id(id, |ui| uniform_editor(ui, &mut uniform)).inner;
                    let result = match edit {
                        UniformEdit::None => Ok(()),
                        UniformEdit::Changed => self.graphics.set_uniform(
                            &self.selected_effect,
                            &uniform.name,
                            &uniform.values,
                            false,
                        ),
                        UniformEdit::Reset => self.graphics.set_uniform(
                            &self.selected_effect,
                            &uniform.name,
                            &uniform.values,
                            true,
                        ),
                    };
                    if let Err(error) = result {
                        self.error = Some(error);
                    }
                }
            });
    }

    // ---------------------------------------------------------------- compare --

    pub(crate) fn compare_page(&mut self, ui: &mut Ui) {
        theme::page_title(
            ui,
            "Compare",
            "Show the original capture next to the neural result. Comparison never changes processing.",
        );
        section(ui, "Mode", |ui| {
            let width = ui.available_width();
            let mut mode = self.nr_options.compare;
            if theme::segmented(
                ui,
                "compare_page_mode",
                &mut mode,
                &[(0, "Off"), (2, "Wipe"), (1, "Side by side")],
                width,
            )
            .changed
            {
                self.nr_options.compare = mode;
                if mode != 0 {
                    self.last_compare_mode = mode;
                }
            }
            ui.add_enabled_ui(self.nr_options.compare != 0, |ui| {
                if self.nr_options.compare == 1 {
                    slider_fmt(
                        ui,
                        "Zoom",
                        &mut self.nr_options.zoom,
                        1.0..=2.,
                        1.,
                        Format::Times,
                    );
                } else {
                    slider_fmt(
                        ui,
                        "Divider position",
                        &mut self.nr_options.split,
                        0.0..=1.,
                        0.5,
                        Format::Percent,
                    );
                }
                let mut swap = self.nr_options.swap != 0;
                if theme::toggle(ui, &mut swap, "Swap original and neural sides").changed() {
                    self.nr_options.swap = swap as u32;
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(theme::button(
                    format!("Open compare controls · {}", self.key(Action::Compare)),
                    Kind::Primary,
                ))
                .clicked()
            {
                self.toggle_compare();
            }
        });
        theme::hint(
            ui,
            "Drag the divider directly on the image; everything else stays click-through.",
        );
        if !self.neural_enabled() {
            theme::hint(
                ui,
                "Neural rendering is off, so both sides currently match.",
            );
        }
    }

    // --------------------------------------------------------------- settings --

    pub(crate) fn settings_page(&mut self, ui: &mut Ui) {
        theme::page_title(
            ui,
            "Settings",
            "Appearance, canvas placement, window behavior and shortcuts.",
        );
        section(ui, "Appearance", |ui| {
            let width = ui.available_width();
            let mut mode = theme::mode();
            if theme::segmented(
                ui,
                "theme_mode",
                &mut mode,
                &[
                    (theme::Mode::Dark, "Dark"),
                    (theme::Mode::Light, "Pearl light"),
                ],
                width,
            )
            .changed
            {
                theme::apply(ui.ctx(), mode);
            }
            theme::hint(ui, "Menu colors never affect the captured image.");
        });
        section(ui, "Canvas", |ui| {
            theme::toggle(ui, &mut self.outline, "Show the canvas border");
            if let Some(b) = native::surface_bounds(self.hwnd) {
                theme::value_chip(
                    ui,
                    &format!("{} × {} px at {}, {}", b.width, b.height, b.x, b.y),
                    false,
                );
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.target.is_some(),
                        theme::button("Fit again", Kind::Secondary),
                    )
                    .clicked()
                {
                    if let Some(target) = self.target.clone() {
                        self.fit(target);
                    }
                }
                if ui
                    .add(theme::button("Room for menu", Kind::Secondary))
                    .on_hover_text(
                        "Enlarge this canvas to 1000 × 780 logical pixels at its current position.",
                    )
                    .clicked()
                {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                            1000.0, 780.0,
                        )));
                    self.notify("Canvas enlarged. Use Fit again to match your selected window.");
                }
            });
        });
        section(ui, "Window", |ui| {
            let mut on_top = self.always_on_top;
            if theme::toggle(ui, &mut on_top, "Keep the canvas above other windows").changed() {
                self.set_always_on_top(on_top);
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(theme::button(
                        "Reset menu size and position",
                        Kind::Secondary,
                    ))
                    .clicked()
                {
                    self.menu_position = None;
                    self.menu_size = crate::menu::DEFAULT_MENU_SIZE;
                }
                if ui
                    .add_enabled(
                        self.target.is_some(),
                        theme::button("Use command bar", Kind::Secondary),
                    )
                    .on_hover_text("Collapse the menu into a compact strip over the canvas.")
                    .clicked()
                {
                    self.show_bar();
                }
            });
        });
        section(ui, "Shortcuts", |ui| self.shortcut_settings(ui));
    }

    fn shortcut_settings(&mut self, ui: &mut Ui) {
        let c = p();
        theme::hint(
            ui,
            format!(
                "{} hides and brings back whichever controls you were using: the full menu or the command bar.",
                self.key(Action::Menu)
            ),
        );
        ui.add_space(6.);
        self.record_shortcut(ui);
        let status = self.runtime.status();
        egui::Grid::new("shortcuts")
            .num_columns(2)
            .spacing([16., 8.])
            .min_col_width(120.)
            .show(ui, |ui| {
                for action in Action::ALL {
                    let registered = match action {
                        Action::Neural => status.neural_hotkey,
                        Action::Menu => status.ui_hotkey,
                        Action::Compare => status.compare_hotkey,
                        Action::Pause => status.pause_hotkey,
                    };
                    ui.label(RichText::new(action.describe()).color(c.ink));
                    ui.horizontal(|ui| {
                        let recording = self.rebinding == Some(action);
                        let text = if recording {
                            "Press a key…".to_string()
                        } else {
                            self.key(action)
                        };
                        let button = ui
                            .add(
                                egui::Button::new(
                                    RichText::new(text)
                                        .monospace()
                                        .color(if recording { c.on_accent } else { c.ink }),
                                )
                                .fill(if recording { c.accent } else { c.raised })
                                .stroke(Stroke::new(1., if recording { c.accent } else { c.line }))
                                .min_size(Vec2::new(150., 30.)),
                            )
                            .on_hover_text(if recording {
                                "Press the new shortcut, with Ctrl, Alt or Shift if you like. Esc cancels."
                            } else {
                                "Click, then press a new shortcut"
                            });
                        if button.clicked() {
                            button.surrender_focus();
                            if recording {
                                self.finish_rebinding();
                            } else {
                                self.rebinding = Some(action);
                                self.hotkey_message = None;
                                // Let the current keys reach the menu while recording.
                                self.runtime.set_hotkeys(None);
                            }
                        }
                        if self.rebinding.is_none() && !registered {
                            ui.label(
                                RichText::new("in use by another app")
                                    .size(12.)
                                    .color(c.warn),
                            );
                        }
                    });
                    ui.end_row();
                }
            });
        if let Some(message) = &self.hotkey_message {
            ui.label(RichText::new(message).size(13.).color(c.warn));
        }
        ui.horizontal_wrapped(|ui| {
            let defaults = crate::hotkeys::Hotkeys::default();
            if ui
                .add_enabled(
                    self.hotkeys != defaults,
                    theme::button("Restore default shortcuts", Kind::Secondary),
                )
                .clicked()
            {
                self.hotkeys = defaults;
                self.hotkey_message = None;
                self.finish_rebinding();
            }
        });
        if !status.tray {
            theme::hint(ui, "The tray icon is unavailable.");
        }
    }

    /// Turn the next key press into the shortcut being recorded.
    fn record_shortcut(&mut self, ui: &mut Ui) {
        let Some(action) = self.rebinding else {
            return;
        };
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        let Some((key, modifiers)) = pressed else {
            return;
        };
        if key == egui::Key::Escape && !modifiers.any() {
            self.finish_rebinding();
            return;
        }
        self.hotkey_message = match Hotkey::from_egui(key, modifiers) {
            None => Some(format!(
                "{} can't be used as a global shortcut.",
                key.name()
            )),
            Some(hotkey) if hotkey.needs_modifier() && !hotkey.has_modifier() => Some(format!(
                "Add Ctrl, Alt or Shift: {} alone would stop that key working in other apps.",
                hotkey.label()
            )),
            Some(hotkey) => match self.hotkeys.conflict(action, hotkey) {
                Some(other) => Some(format!(
                    "{} is already used for “{}”.",
                    hotkey.label(),
                    other.describe()
                )),
                None => {
                    self.hotkeys.set(action, hotkey);
                    self.finish_rebinding();
                    self.notify(format!("{} · {}", action.describe(), hotkey.label()));
                    None
                }
            },
        };
    }

    /// Stop recording and register the current shortcuts again.
    pub(crate) fn finish_rebinding(&mut self) {
        self.rebinding = None;
        self.runtime.set_hotkeys(Some(self.hotkeys));
    }

    // ------------------------------------------------------------ diagnostics --

    pub(crate) fn diagnostics_page(&mut self, ui: &mut Ui) {
        theme::page_title(
            ui,
            "Diagnostics",
            "Live rates, neural runtime state and recovery.",
        );
        section(ui, "Live rates", |ui| {
            match self.frame_rates.rates() {
                Some([capture, neural, redraw]) => {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 28.;
                        theme::stat(ui, &format!("{capture:.0} fps"), "CAPTURE");
                        theme::stat(ui, &format!("{neural:.0}/s"), "NEURAL EVALUATIONS");
                        theme::stat(ui, &format!("{redraw:.0} fps"), "REDRAW");
                    });
                }
                None => {
                    theme::hint(
                        ui,
                        if self.target.is_some() {
                            "Measuring…"
                        } else {
                            "Start capturing a window to measure rates."
                        },
                    );
                }
            }
            theme::hint(ui, "Measured over the last second. Redraw includes reused images; these are not monitor presentation FPS.");
        });
        section(ui, "Neural runtime", |ui| {
            let s = self.graphics.nr_status();
            let message = s.message();
            if !message.is_empty() {
                ui.add(egui::Label::new(RichText::new(message).monospace().size(12.)).wrap());
            }
            egui::Grid::new("nr_status")
                .num_columns(2)
                .spacing([16., 6.])
                .show(ui, |ui| {
                    let rows = [
                        (
                            "Model",
                            if s.width > 0 {
                                format!("{} × {}", s.width, s.height)
                            } else {
                                "not built".into()
                            },
                        ),
                        (
                            "State",
                            if s.pending != 0 {
                                "applying settings".into()
                            } else if s.active != 0 {
                                "active".into()
                            } else {
                                "inactive".into()
                            },
                        ),
                        ("Successful frames", s.evaluations.to_string()),
                        ("Model builds", s.builds.to_string()),
                        ("Motion dispatches", s.motion_frames.to_string()),
                    ];
                    for (label, value) in rows {
                        ui.label(RichText::new(label).color(p().muted));
                        ui.label(RichText::new(value).monospace());
                        ui.end_row();
                    }
                });
            if ui
                .add(theme::button("Rebuild model / retry", Kind::Secondary))
                .on_hover_text("Recreate the neural feature, e.g. after a driver error.")
                .clicked()
            {
                self.graphics.nr_reset();
                self.notify("Neural model will be rebuilt.");
            }
        });
        section(ui, "Motion guide overrides", |ui| {
            slider(
                ui,
                "Motion scale X",
                &mut self.nr_options.mv_x,
                -4.0..=4.,
                1.,
            );
            slider(
                ui,
                "Motion scale Y",
                &mut self.nr_options.mv_y,
                -4.0..=4.,
                1.,
            );
        });
        section(ui, "Reset", |ui| {
            theme::hint(ui, "Restores style, model and image settings to their defaults. Keeps neural on/off, motion backend, passes and comparison.");
            if self.confirm_reset {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add(theme::button("Reset neural settings", Kind::Danger))
                        .clicked()
                    {
                        self.nr_options.reset_tuning();
                        self.confirm_reset = false;
                        self.notify("Neural settings reset to defaults.");
                    }
                    if ui.add(theme::button("Cancel", Kind::Ghost)).clicked() {
                        self.confirm_reset = false;
                    }
                });
            } else if ui
                .add(theme::button("Reset neural settings…", Kind::Secondary))
                .clicked()
            {
                self.confirm_reset = true;
            }
        });
        section(ui, "About", |ui| {
            egui::Grid::new("about")
                .num_columns(2)
                .spacing([16., 6.])
                .show(ui, |ui| {
                    ui.label(RichText::new("Build").color(p().muted));
                    ui.label(RichText::new(env!("OVERLAY_BUILD_LABEL")).monospace());
                    ui.end_row();
                    ui.label(RichText::new("Folder").color(p().muted));
                    ui.add(
                        egui::Label::new(
                            RichText::new(self.runtime_folder.display().to_string())
                                .monospace()
                                .size(12.),
                        )
                        .wrap(),
                    );
                    ui.end_row();
                });
        });
    }
}

enum UniformEdit {
    None,
    Changed,
    Reset,
}

/// Editor for one ReShade uniform: checkbox, choice list, sliders or drag values.
fn uniform_editor(ui: &mut Ui, uniform: &mut Uniform) -> UniformEdit {
    let label = if uniform.label.trim().is_empty() {
        uniform.name.clone()
    } else {
        uniform.label.clone()
    };
    let mut changed = false;
    ui.label(&label).on_hover_text(&uniform.tooltip);
    let reset = ui
        .horizontal_wrapped(|ui| {
            if uniform.kind == 0 {
                let mut value = uniform.values[0] != 0.0;
                if ui.checkbox(&mut value, "Enabled").changed() {
                    uniform.values[0] = value as u8 as f32;
                    changed = true;
                }
            } else if uniform.items.len() > 1 && uniform.components == 1 {
                let mut selected = uniform.values[0].max(0.0) as usize;
                egui::ComboBox::from_id_salt("choice")
                    .selected_text(uniform.items.get(selected).cloned().unwrap_or_default())
                    .show_ui(ui, |ui| {
                        for (index, item) in uniform.items.iter().enumerate() {
                            changed |= ui.selectable_value(&mut selected, index, item).changed();
                        }
                    });
                uniform.values[0] = selected as f32;
            } else {
                let float = uniform.kind == 3;
                for component in 0..uniform.components {
                    let (lo, hi) = (uniform.minimum[component], uniform.maximum[component]);
                    let value = &mut uniform.values[component];
                    changed |= if uniform.bounded && hi > lo {
                        let mut slider = egui::Slider::new(value, lo..=hi);
                        if !float {
                            slider = slider.integer();
                        }
                        ui.add(slider).changed()
                    } else {
                        ui.add(egui::DragValue::new(value).speed(if float { 0.01 } else { 1.0 }))
                            .changed()
                    };
                }
            }
            theme::reset_button(ui, &label).clicked()
        })
        .inner;
    ui.add_space(4.);
    if reset {
        UniformEdit::Reset
    } else if changed {
        UniformEdit::Changed
    } else {
        UniformEdit::None
    }
}
