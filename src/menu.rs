//! The menu shell, the command bar and the on-canvas comparison controls.
use crate::hotkeys::Action;
use crate::theme::{self, p, Icon, Kind};
use crate::{App, Page};
use egui::{Align, Layout, Rect, RichText, Rounding, Sense, Stroke, Ui, Vec2};
use std::time::Duration;

pub(crate) const DEFAULT_MENU_SIZE: Vec2 = Vec2::new(760., 720.);
pub(crate) const IDLE_MENU_MARGIN: f32 = 4.;
const NAV_WIDTH: f32 = 172.;
/// Below this width the navigation rail becomes a row of tabs.
const RAIL_MIN_WIDTH: f32 = 600.;

pub(crate) fn user_effect(effect: &str) -> bool {
    if crate::looks::owned(effect) {
        return false;
    }
    let name = effect
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(effect)
        .to_ascii_lowercase();
    !matches!(
        name.as_str(),
        "dlss5_feed.fx"
            | "lumenite_kernel.fx"
            | "lumenite_quantmotion.fx"
            | "vort_motion.fx"
            | "lumenite_lsao.fx"
            | "lumenite_quantao.fx"
            | "lumenite_rtao.fx"
            | "lumenite_sssr.fx"
            | "lumenite_traa.fx"
    )
}

impl App {
    pub(crate) fn menu(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let idle = self.target.is_none();
        let margin = if idle || screen.width() < 420. || screen.height() < 300. {
            IDLE_MENU_MARGIN
        } else {
            18.
        };
        let size = Vec2::new(
            self.menu_size.x.min((screen.width() - margin * 2.).max(1.)),
            self.menu_size
                .y
                .min((screen.height() - margin * 2.).max(1.)),
        );
        let position = if idle {
            screen.min + Vec2::splat(IDLE_MENU_MARGIN)
        } else {
            self.menu_position
                .unwrap_or(screen.min + Vec2::splat(margin))
        };
        // Allow moving the panel aside, while keeping its header
        // reachable. Clamp again every frame to recover after resize/DPI changes.
        let position = egui::pos2(
            position
                .x
                .clamp(screen.left(), (screen.right() - 180.).max(screen.left())),
            position
                .y
                .clamp(screen.top(), (screen.bottom() - 70.).max(screen.top())),
        );
        self.menu_position = Some(position);
        let c = p();
        let response = egui::Window::new("Overlay menu")
            .id(egui::Id::new("embedded_menu"))
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .constrain(false)
            .min_size(Vec2::ZERO)
            .fixed_rect(Rect::from_min_size(position, size.max(Vec2::splat(1.))))
            .frame(
                egui::Frame::none()
                    .fill(c.bg)
                    .stroke(Stroke::new(1., c.line))
                    .rounding(12.)
                    .inner_margin(0.),
            )
            .show(ctx, |ui| {
                // One outer scroll keeps everything reachable on very small canvases.
                egui::ScrollArea::vertical()
                    .id_salt("menu_shell")
                    .max_height(size.y.max(1.))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(size.x.max(100.));
                        ui.spacing_mut().item_spacing.y = 0.;
                        self.header(ui, ctx, position);
                        self.session_strip(ui);
                        let footer = self.footer_height;
                        let body = (position.y + size.y - ui.cursor().top() - footer).max(90.);
                        self.body(ui, body);
                        let footer = self.footer(ui);
                        self.footer_height = footer;
                    });
            });
        if let Some(response) = response {
            self.resize_grip(ctx, response.response.rect);
        }
    }

    fn header(&mut self, ui: &mut Ui, ctx: &egui::Context, position: egui::Pos2) {
        let c = p();
        let width = ui.available_width();
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, 50.), Sense::hover());
        ui.painter().rect_filled(
            row,
            Rounding {
                nw: 12.,
                ne: 12.,
                sw: 0.,
                se: 0.,
            },
            c.chrome,
        );
        let idle = self.target.is_none();
        let buttons = if idle { 2. } else { 3. };
        let buttons_width = buttons * 34. + (buttons - 1.) * 2.;
        let grip_rect = Rect::from_min_max(
            row.min,
            egui::pos2(
                (row.right() - buttons_width - 12.).max(row.left() + 1.),
                row.bottom(),
            ),
        );
        let grip = ui
            .interact(grip_rect, ui.id().with("header_drag"), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::Grab)
            .on_hover_text(if idle {
                "Drag to move the app"
            } else {
                "Drag to move this menu"
            });
        self.rects.menu_header = grip.rect;
        if idle {
            if grip.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
        } else if grip.dragged_by(egui::PointerButton::Primary) {
            self.menu_position = Some(position + ctx.input(|i| i.pointer.delta()));
        }
        let painter = ui.painter();
        let logo = Rect::from_min_size(
            egui::pos2(row.left() + 16., row.center().y - 12.),
            Vec2::splat(24.),
        );
        painter.image(
            self.logo.id(),
            logo,
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            egui::Color32::WHITE,
        );
        let room = grip_rect.width() - 56.;
        let galley =
            painter.layout_no_wrap("NeuralLayer".into(), egui::FontId::proportional(16.), c.ink);
        let title_width = galley.size().x;
        if room > 60. {
            painter.galley(
                egui::pos2(logo.right() + 10., row.center().y - galley.size().y / 2.),
                galley,
                c.ink,
            );
        }
        let build = env!("OVERLAY_BUILD_LABEL");
        let chip = painter.layout_no_wrap(build.into(), egui::FontId::monospace(11.), c.muted);
        if room > title_width + chip.size().x + 40. {
            let at = egui::pos2(
                logo.right() + 10. + title_width + 10.,
                row.center().y - chip.size().y / 2.,
            );
            painter.rect_stroke(
                Rect::from_min_size(at - Vec2::new(6., 3.), chip.size() + Vec2::new(12., 6.)),
                5.,
                Stroke::new(1., c.line),
            );
            painter.galley(at, chip, c.muted);
        }
        let mut x = row.right() - 12. - 34.;
        let mut slot = || {
            let rect = Rect::from_min_size(egui::pos2(x, row.center().y - 17.), Vec2::splat(34.));
            x -= 36.;
            rect
        };
        if ui
            .put(slot(), |ui: &mut Ui| {
                theme::icon_button(ui, Icon::Close, "Quit NeuralLayer")
            })
            .clicked()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let can_hide = self.can_hide();
        let hide_label = format!("Hide controls · {}", self.key(Action::Menu));
        if ui
            .put(slot(), |ui: &mut Ui| {
                ui.add_enabled_ui(can_hide, |ui| {
                    theme::icon_button(ui, Icon::Minus, &hide_label)
                })
                .inner
            })
            .clicked()
        {
            self.hide_controls();
        }
        if !idle
            && ui
                .put(slot(), |ui: &mut Ui| {
                    theme::icon_button(
                        ui,
                        Icon::Collapse,
                        "Collapse to the command bar (the show/hide shortcut will use it too)",
                    )
                })
                .clicked()
        {
            self.prefer_bar = true;
            self.show_bar();
        }
    }

    fn session_strip(&mut self, ui: &mut Ui) {
        let c = p();
        egui::Frame::none()
            .fill(c.chrome)
            .inner_margin(egui::Margin {
                left: 16.,
                right: 16.,
                top: 2.,
                bottom: 12.,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = Vec2::new(8., 8.);
                let running = self.target.is_some();
                let title = self
                    .target
                    .as_ref()
                    .map_or("choose a window to start".to_string(), |t| t.title.clone());
                let narrow = ui.available_width() < 520.;
                let controls_width = 140. + 8. + 96.;
                let pill = |ui: &mut Ui, width: f32| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(width.max(80.), 34.),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_max_width(width.max(80.));
                            theme::status_pill(
                                ui,
                                running,
                                if running { "Rendering" } else { "Idle" },
                                &title,
                            );
                        },
                    );
                };
                if narrow {
                    pill(ui, ui.available_width());
                    ui.horizontal(|ui| {
                        self.topmost_control(ui);
                        self.stop_control(ui, 96.);
                    });
                } else {
                    ui.horizontal(|ui| {
                        pill(ui, ui.available_width() - controls_width - 16.);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            self.stop_control(ui, 96.);
                            self.topmost_control(ui);
                        });
                    });
                }
            });
        let rect = ui.min_rect();
        ui.painter().hline(
            rect.x_range(),
            ui.cursor().top(),
            Stroke::new(1., c.line_soft),
        );
    }

    fn topmost_control(&mut self, ui: &mut Ui) {
        let c = p();
        let on = self.always_on_top;
        let response = ui
            .add(
                // Same label either way; the highlight shows whether it is on.
                egui::Button::new(RichText::new("Always on top").color(if on {
                    c.ink
                } else {
                    c.muted
                }))
                .fill(if on { c.nav_active } else { c.raised })
                .stroke(Stroke::new(1., if on { c.accent } else { c.line }))
                .min_size(Vec2::new(140., 34.)),
            )
            .on_hover_text(if on {
                "On · keep the canvas above other windows. Click to turn off."
            } else {
                "Off · allow other windows above the canvas. Click to turn on."
            });
        if response.clicked() {
            self.set_always_on_top(!self.always_on_top);
        }
    }

    fn stop_control(&mut self, ui: &mut Ui, width: f32) {
        let running = self.target.is_some();
        let stop = ui
            .add_enabled(
                running,
                theme::button(
                    "Stop",
                    if running {
                        Kind::Danger
                    } else {
                        Kind::Secondary
                    },
                )
                .min_size(Vec2::new(width, 34.)),
            )
            .on_hover_text(
                "Stop rendering, clear the source, and restore the app's position and size.",
            );
        self.rects.stop_button = stop.rect;
        if stop.clicked() {
            self.stop_rendering();
        }
    }

    fn body(&mut self, ui: &mut Ui, mut height: f32) {
        let width = ui.available_width();
        let rail = width >= RAIL_MIN_WIDTH;
        if !rail {
            height -= egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12., 10.))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing = Vec2::new(4., 4.);
                    ui.horizontal_wrapped(|ui| {
                        for page in Page::ALL {
                            self.nav_button(ui, page, false);
                        }
                    });
                })
                .response
                .rect
                .height();
        }
        let height = height.max(70.);
        ui.allocate_ui_with_layout(
            Vec2::new(width, height),
            Layout::left_to_right(Align::Min),
            |ui| {
                ui.set_height(height);
                if rail {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(NAV_WIDTH, height), Sense::hover());
                    ui.painter().rect_filled(rect, 0., p().chrome);
                    ui.painter().vline(
                        rect.right(),
                        rect.y_range(),
                        Stroke::new(1., p().line_soft),
                    );
                    let mut nav = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect.shrink2(Vec2::new(10., 12.)))
                            .layout(Layout::top_down(Align::Min)),
                    );
                    nav.spacing_mut().item_spacing.y = 3.;
                    for page in Page::ALL {
                        self.nav_button(&mut nav, page, true);
                    }
                    self.shortcut_legend(&mut nav, rect);
                }
                let content_width = ui.available_width();
                ui.vertical(|ui| {
                    ui.set_width(content_width);
                    egui::ScrollArea::vertical()
                        .id_salt(("page", self.tab))
                        .max_height(height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Frame::none()
                                .inner_margin(egui::Margin::symmetric(18., 16.))
                                .show(ui, |ui| {
                                    ui.set_width((content_width - 36.).max(60.));
                                    ui.spacing_mut().item_spacing = Vec2::new(8., 10.);
                                    match self.tab {
                                        Page::Source => self.source_page(ui),
                                        Page::Neural => self.neural_page(ui),
                                        Page::Look => self.look_page(ui),
                                        Page::Compare => self.compare_page(ui),
                                        Page::Settings => self.settings_page(ui),
                                        Page::Diagnostics => self.diagnostics_page(ui),
                                    }
                                });
                        });
                });
            },
        );
    }

    fn nav_button(&mut self, ui: &mut Ui, page: Page, rail: bool) {
        let c = p();
        let active = self.tab == page;
        let size = if rail {
            Vec2::new(ui.available_width(), 38.)
        } else {
            let text = ui.painter().layout_no_wrap(
                page.label().into(),
                egui::FontId::proportional(13.5),
                c.ink,
            );
            Vec2::new(text.size().x + 40., 32.)
        };
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                true,
                active,
                page.label(),
            )
        });
        if active {
            ui.painter().rect_filled(rect, 8., c.nav_active);
        } else if response.hovered() {
            ui.painter().rect_filled(rect, 8., c.raised);
        }
        if response.has_focus() {
            ui.painter()
                .rect_stroke(rect, 8., Stroke::new(1., c.accent));
        }
        let color = if active { c.ink } else { c.muted };
        let icon = Rect::from_min_size(
            egui::pos2(rect.left() + 10., rect.center().y - 8.),
            Vec2::splat(16.),
        );
        theme::paint_icon(
            ui.painter(),
            icon,
            page.icon(),
            if active { c.accent } else { color },
        );
        ui.painter().text(
            egui::pos2(icon.right() + 10., rect.center().y),
            egui::Align2::LEFT_CENTER,
            page.label(),
            egui::FontId::proportional(if rail { 14.5 } else { 13.5 }),
            color,
        );
        // Mark pages whose feature is active.
        let live = match page {
            Page::Neural => self.neural_enabled(),
            Page::Look => self.looks.selected.is_some(),
            Page::Compare => self.nr_options.compare != 0,
            _ => false,
        };
        if live && rail {
            ui.painter().circle_filled(
                egui::pos2(rect.right() - 12., rect.center().y),
                3.5,
                c.accent,
            );
        }
        if response.clicked() {
            self.tab = page;
        }
    }

    fn shortcut_legend(&self, ui: &mut Ui, rail: Rect) {
        let status = self.runtime.status();
        let rows: Vec<(&str, String, bool)> = Action::ALL
            .into_iter()
            .map(|action| {
                let ok = match action {
                    Action::Neural => status.neural_hotkey,
                    Action::Menu => status.ui_hotkey,
                    Action::Compare => status.compare_hotkey,
                    Action::Pause => status.pause_hotkey,
                };
                (action.short(), self.key(action), ok)
            })
            .collect();
        // Each row is one interact height; plus card margins and spacing.
        let height = rows.len() as f32 * 34. + 24.;
        if rail.height() < 6. * 41. + height + 40. {
            return;
        }
        let rect = Rect::from_min_size(
            egui::pos2(rail.left() + 10., rail.bottom() - height - 12.),
            Vec2::new(rail.width() - 20., height),
        );
        let mut legend = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Min)),
        );
        theme::card().inner_margin(10.).show(&mut legend, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.;
            for (label, key, ok) in rows {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(12.5).color(if ok {
                        p().muted
                    } else {
                        p().line
                    }));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::kbd(ui, &key);
                    });
                })
                .response
                .on_hover_text(if ok {
                    "Global shortcut · change it in Settings"
                } else {
                    "Unavailable: another app registered this key · change it in Settings"
                });
            }
        });
    }

    /// Errors, the latest notice and quick actions. Returns its height.
    fn footer(&mut self, ui: &mut Ui) -> f32 {
        let c = p();
        let top = ui.cursor().top();
        ui.painter()
            .hline(ui.max_rect().x_range(), top, Stroke::new(1., c.line_soft));
        let frame = egui::Frame::none()
            .fill(c.chrome)
            .rounding(Rounding {
                nw: 0.,
                ne: 0.,
                sw: 12.,
                se: 12.,
            })
            .inner_margin(egui::Margin::symmetric(16., 10.))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = Vec2::new(8., 6.);
                let mut dismiss = false;
                if let Some(error) = &self.error {
                    ui.horizontal(|ui| {
                        if theme::icon_button_colored(ui, Icon::Close, "Dismiss", c.stop).clicked()
                        {
                            dismiss = true;
                        }
                        ui.add(
                            egui::Label::new(RichText::new(error).color(c.stop).size(13.)).wrap(),
                        );
                    });
                }
                if dismiss {
                    self.error = None;
                }
                if let Some(error) = &self.runtime.status().error {
                    ui.label(RichText::new(error).color(c.warn).size(13.));
                }
                ui.horizontal_wrapped(|ui| {
                    let fresh = self.notice_at.elapsed() < Duration::from_secs(8);
                    let text = if fresh {
                        self.notice.clone()
                    } else if self.target.is_some() {
                        format!(
                            "Click outside the menu to reach your game · {} hides the controls",
                            self.key(Action::Menu)
                        )
                    } else {
                        "Pick a window on Source · drag the header to move the app".into()
                    };
                    ui.label(RichText::new(text).size(12.5).color(if fresh {
                        c.accent
                    } else {
                        c.muted
                    }));
                });
            });
        frame.response.rect.height()
    }

    fn resize_grip(&mut self, ctx: &egui::Context, menu: Rect) {
        let corner = menu.right_bottom() - Vec2::splat(21.);
        egui::Area::new(egui::Id::new("menu_resize_grip"))
            .order(egui::Order::Foreground)
            .fixed_pos(corner)
            .movable(false)
            .show(ctx, |ui| {
                let (rect, grip) = ui.allocate_exact_size(Vec2::splat(18.), Sense::drag());
                self.rects.menu_resize = rect;
                for offset in [0., 5., 10.] {
                    ui.painter().line_segment(
                        [
                            rect.right_top() + Vec2::new(0., offset),
                            rect.left_bottom() + Vec2::new(offset, 0.),
                        ],
                        Stroke::new(1.5, p().muted),
                    );
                }
                let grip = grip
                    .on_hover_cursor(egui::CursorIcon::ResizeNwSe)
                    .on_hover_text("Drag to resize the menu");
                if grip.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(start) = ctx.input(|i| i.pointer.press_origin()) {
                        self.menu_resize_origin = Some((start, self.menu_size));
                    }
                }
                if grip.dragged_by(egui::PointerButton::Primary) {
                    self.menu_resizing = true;
                    if let (Some((start, initial)), Some(pointer)) = (
                        self.menu_resize_origin,
                        ctx.input(|i| i.pointer.interact_pos()),
                    ) {
                        self.menu_size = (initial + (pointer - start))
                            .clamp(Vec2::new(340., 300.), Vec2::new(1180., 920.));
                    }
                } else if !ctx.input(|i| i.pointer.primary_down()) {
                    self.menu_resize_origin = None;
                }
            });
    }

    // ------------------------------------------------------------ command bar --

    /// A compact strip of the most-used controls over the canvas.
    pub(crate) fn command_bar(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let compact = screen.width() < 900.;
        let bar = egui::Area::new(egui::Id::new("command_bar"))
            .anchor(egui::Align2::CENTER_TOP, [0., 14.])
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                theme::floating().show(ui, |ui| {
                    ui.set_max_width((screen.width() - 32.).max(200.));
                    ui.spacing_mut().item_spacing = Vec2::new(6., 6.);
                    ui.horizontal_wrapped(|ui| self.command_bar_contents(ui, compact));
                })
            })
            .response;
        if self.quick_settings {
            egui::Area::new(egui::Id::new("quick_settings"))
                .fixed_pos(egui::pos2(
                    (bar.rect.center().x - 170.).max(screen.left() + 8.),
                    bar.rect.bottom() + 8.,
                ))
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    theme::floating().inner_margin(14.).show(ui, |ui| {
                        ui.set_width(320.);
                        ui.spacing_mut().item_spacing = Vec2::new(8., 8.);
                        self.quick_settings_panel(ui);
                    });
                });
        }
    }

    fn command_bar_contents(&mut self, ui: &mut Ui, compact: bool) {
        let c = p();
        let running = self.target.is_some();
        let (dot, _) = ui.allocate_exact_size(Vec2::new(10., 34.), Sense::hover());
        ui.painter()
            .circle_filled(dot.center(), 4., if running { c.good } else { c.muted });
        if !compact {
            let title = self
                .target
                .as_ref()
                .map_or("No source", |t| t.title.as_str())
                .to_string();
            ui.allocate_ui_with_layout(
                Vec2::new(150., 34.),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_max_width(150.);
                    ui.add(egui::Label::new(RichText::new(&title).size(13.).strong()).truncate())
                        .on_hover_text(&title);
                },
            );
        }
        separator(ui);
        let mut enabled = self.neural_enabled();
        let neural_key = self.key(Action::Neural);
        let neural = ui
            .horizontal(|ui| {
                let response = theme::toggle(ui, &mut enabled, "Neural");
                theme::kbd(ui, &neural_key);
                response
            })
            .inner;
        if neural.changed() {
            self.toggle_neural();
        }
        let mut passes = self.nr_options.passes;
        if theme::segmented(
            ui,
            "bar_passes",
            &mut passes,
            &[(1, "1×"), (2, "2×"), (3, "3×")],
            116.,
        )
        .changed
        {
            self.set_passes(passes);
        }
        self.style_combo(ui, "bar_style", 120.);
        if ui
            .add(theme::button("Compare", Kind::Ghost))
            .on_hover_text(format!(
                "Compare original and neural output · {}",
                self.key(Action::Compare)
            ))
            .clicked()
        {
            self.compare_from_bar = true;
            self.toggle_compare();
        }
        if !compact {
            separator(ui);
            let rates = self.frame_rates.rates();
            let show = |v: Option<f64>| v.map_or("–".to_string(), |v| format!("{v:.0}"));
            theme::stat(ui, &show(rates.map(|r| r[0])), "CAPTURE");
            theme::stat(ui, &format!("{}/s", show(rates.map(|r| r[1]))), "NEURAL");
            theme::stat(ui, &show(rates.map(|r| r[2])), "OUTPUT");
            separator(ui);
        }
        let on_top = self.always_on_top;
        if theme::icon_button_colored(
            ui,
            Icon::Pin,
            if on_top {
                "Always on top · on (click to turn off)"
            } else {
                "Always on top · off (click to turn on)"
            },
            if on_top { c.accent } else { c.muted },
        )
        .clicked()
        {
            self.set_always_on_top(!on_top);
        }
        let tune = theme::icon_button_colored(
            ui,
            Icon::Tune,
            "Quick settings",
            if self.quick_settings {
                c.accent
            } else {
                c.muted
            },
        );
        if tune.clicked() {
            self.quick_settings = !self.quick_settings;
        }
        if theme::icon_button(ui, Icon::Expand, "Full menu").clicked() {
            self.show_menu();
        }
        let can_hide = self.can_hide();
        let hide_label = format!(
            "Hide the command bar · {} brings it back",
            self.key(Action::Menu)
        );
        if ui
            .add_enabled_ui(can_hide, |ui| {
                theme::icon_button(ui, Icon::Minus, &hide_label)
            })
            .inner
            .clicked()
        {
            self.hide_controls();
        }
        if theme::icon_button_colored(ui, Icon::Stop, "Stop rendering", c.stop).clicked() {
            self.stop_rendering();
        }
    }

    fn quick_settings_panel(&mut self, ui: &mut Ui) {
        let width = ui.available_width();
        ui.horizontal(|ui| {
            theme::heading(ui, "Quick settings");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.link("All neural settings").clicked() {
                    self.tab = Page::Neural;
                    self.show_menu();
                }
            });
        });
        theme::overline(ui, "Style");
        self.style_segmented(ui, "quick_style", width);
        self.model_scale_control(ui);
        theme::slider(
            ui,
            "Intensity",
            &mut self.nr_options.intensity,
            0.0..=2.,
            1.,
        );
        theme::slider_fmt(
            ui,
            "Neural blend",
            &mut self.nr_options.blend,
            0.0..=1.,
            1.,
            theme::Format::Percent,
        );
        theme::overline(ui, "Look");
        self.preset_choice(ui, "quick_look", width);
    }

    // -------------------------------------------------------------- compare --

    /// Comparison controls over the canvas: a draggable divider on the image
    /// and a compact control strip. Everything else passes through to the source.
    pub(crate) fn compare_overlay(&mut self, ctx: &egui::Context) {
        let c = p();
        let screen = ctx.screen_rect();
        let wipe = self.nr_options.compare == 2;
        let swap = self.nr_options.swap != 0;
        // Side labels; not interactive so they never block the source.
        for (left, text) in [(true, "ORIGINAL"), (false, "NEURAL")] {
            let left = left != swap;
            if self.nr_options.compare == 0 {
                break;
            }
            let anchor = if left {
                (egui::Align2::LEFT_TOP, [16., 16.])
            } else {
                (egui::Align2::RIGHT_TOP, [-16., 16.])
            };
            egui::Area::new(egui::Id::new(("compare_label", text)))
                .anchor(anchor.0, anchor.1)
                .interactable(false)
                .order(egui::Order::Background)
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(c.chrome.gamma_multiply(0.85))
                        .stroke(Stroke::new(1., c.line))
                        .rounding(7.)
                        .inner_margin(egui::Margin::symmetric(10., 5.))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(text)
                                    .size(11.5)
                                    .strong()
                                    .color(if text == "NEURAL" { c.accent } else { c.ink }),
                            );
                        });
                });
        }
        if wipe && screen.width() > 240. && screen.height() > 200. {
            let x = screen.left() + screen.width() * self.nr_options.split;
            let center = egui::pos2(x, screen.top() + screen.height() * 0.62);
            egui::Area::new(egui::Id::new("compare_divider"))
                .fixed_pos(center - Vec2::splat(18.))
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.), Sense::drag());
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Slider,
                            true,
                            "Comparison divider",
                        )
                    });
                    let painter = ui.painter();
                    painter.circle_filled(rect.center(), 18., c.ink);
                    painter.circle_stroke(rect.center(), 18., Stroke::new(1., c.line));
                    for direction in [-1f32, 1.] {
                        let tip = rect.center() + Vec2::new(direction * 9., 0.);
                        let base = rect.center() + Vec2::new(direction * 3., 0.);
                        painter.add(egui::Shape::line(
                            vec![base + Vec2::new(0., -5.), tip, base + Vec2::new(0., 5.)],
                            Stroke::new(2., c.bg),
                        ));
                    }
                    let response = response
                        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                        .on_hover_text("Drag to move the divider");
                    if response.dragged() {
                        if let Some(pointer) = ctx.input(|i| i.pointer.interact_pos()) {
                            self.nr_options.split =
                                ((pointer.x - screen.left()) / screen.width()).clamp(0., 1.);
                        }
                    }
                });
        }
        egui::Area::new(egui::Id::new("compare_controls"))
            .anchor(egui::Align2::LEFT_BOTTOM, [18., -18.])
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                theme::floating().show(ui, |ui| {
                    // Keep the canvas's bottom-right corner free for the source.
                    ui.set_max_width((screen.width() - 18. - 80.).clamp(200., 760.));
                    ui.spacing_mut().item_spacing = Vec2::new(8., 6.);
                    ui.horizontal_wrapped(|ui| self.compare_controls(ui));
                    if !self.neural_enabled() {
                        theme::hint(ui, "Neural rendering is off · both sides match.");
                    }
                    if self.target.is_none() {
                        theme::hint(ui, "Select a source in the full menu first.");
                    }
                });
            });
    }

    fn compare_controls(&mut self, ui: &mut Ui) {
        let mut mode = self.nr_options.compare;
        if theme::segmented(
            ui,
            "compare_mode",
            &mut mode,
            &[(2, "Wipe"), (1, "Side by side")],
            190.,
        )
        .changed
        {
            self.nr_options.compare = mode;
        }
        if ui
            .add(theme::button("Swap", Kind::Secondary))
            .on_hover_text("Swap original and neural sides")
            .clicked()
        {
            self.nr_options.swap = u32::from(self.nr_options.swap == 0);
        }
        ui.allocate_ui_with_layout(
            Vec2::new(250. + 40., 34.),
            Layout::left_to_right(Align::Center),
            |ui| {
                let (value, range, label) = if self.nr_options.compare == 1 {
                    (&mut self.nr_options.zoom, 1.0..=2.0, "Zoom")
                } else {
                    (&mut self.nr_options.split, 0.0..=1.0, "Split")
                };
                ui.label(RichText::new(label).size(12.5).color(p().muted));
                let response = theme::bare_slider(ui, value, range, 200.).on_hover_text(label);
                self.rects.compare_slider = response.rect;
            },
        );
        let mut enabled = self.neural_enabled();
        let neural_key = self.key(Action::Neural);
        if ui
            .horizontal(|ui| {
                let r = theme::toggle(ui, &mut enabled, "Neural");
                theme::kbd(ui, &neural_key);
                r
            })
            .inner
            .changed()
        {
            self.toggle_neural();
        }
        if ui
            .add(theme::button("Menu", Kind::Ghost))
            .on_hover_text("Full menu")
            .clicked()
        {
            self.show_menu();
        }
        if ui
            .add(theme::button("Close", Kind::Ghost))
            .on_hover_text(format!("Close comparison · {}", self.key(Action::Compare)))
            .clicked()
        {
            self.close_compare();
        }
    }

    pub(crate) fn live_panel_toolbar(&mut self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("native_panel_return"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-12., -12.])
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                theme::floating().inner_margin(12.).show(ui, |ui| {
                    if ui
                        .add(theme::button("Back to overlay menu", Kind::Primary))
                        .clicked()
                    {
                        let _ = self.graphics.native_overlay(false);
                        self.live_panel = false;
                    }
                    theme::hint(ui, "ReShade · live shader controls");
                });
            });
    }
}

fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(9., 26.), Sense::hover());
    ui.painter()
        .vline(rect.center().x, rect.y_range(), Stroke::new(1., p().line));
}
