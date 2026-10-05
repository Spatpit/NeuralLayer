//! Menu palette, style and shared controls. These colors never affect captured pixels.
use egui::{Color32, FontId, Rect, Response, RichText, Rounding, Sense, Stroke, Ui, Vec2};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Dark,
    Light,
}
impl Mode {
    pub fn id(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }
}

pub struct Palette {
    /// Menu body.
    pub bg: Color32,
    /// Header, navigation rail and footer.
    pub chrome: Color32,
    /// Cards and floating panels.
    pub panel: Color32,
    /// Buttons resting on a panel.
    pub raised: Color32,
    /// Wells: segmented controls, text fields, value chips.
    pub inset: Color32,
    pub line: Color32,
    pub line_soft: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    /// A selected segment or a hovered row.
    pub selected: Color32,
    pub nav_active: Color32,
    pub good: Color32,
    pub good_fill: Color32,
    pub good_line: Color32,
    pub warn: Color32,
    pub stop: Color32,
    pub stop_fill: Color32,
    pub stop_line: Color32,
    pub shadow: Color32,
}

const DARK: Palette = Palette {
    bg: Color32::from_rgb(14, 17, 23),
    chrome: Color32::from_rgb(11, 14, 19),
    panel: Color32::from_rgb(18, 22, 29),
    raised: Color32::from_rgb(26, 31, 40),
    inset: Color32::from_rgb(11, 14, 19),
    line: Color32::from_rgb(42, 49, 64),
    line_soft: Color32::from_rgb(35, 42, 54),
    ink: Color32::from_rgb(232, 236, 242),
    muted: Color32::from_rgb(163, 173, 187),
    accent: Color32::from_rgb(138, 180, 255),
    on_accent: Color32::from_rgb(11, 14, 19),
    selected: Color32::from_rgb(39, 50, 71),
    nav_active: Color32::from_rgb(31, 39, 53),
    good: Color32::from_rgb(95, 211, 154),
    good_fill: Color32::from_rgb(18, 37, 28),
    good_line: Color32::from_rgb(35, 74, 53),
    warn: Color32::from_rgb(242, 179, 107),
    stop: Color32::from_rgb(255, 160, 160),
    stop_fill: Color32::from_rgb(42, 21, 25),
    stop_line: Color32::from_rgb(90, 42, 51),
    shadow: Color32::from_black_alpha(110),
};

// Pearl: the original light identity.
const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(243, 244, 246),
    chrome: Color32::WHITE,
    panel: Color32::WHITE,
    raised: Color32::WHITE,
    inset: Color32::from_rgb(230, 234, 240),
    line: Color32::from_rgb(203, 208, 216),
    line_soft: Color32::from_rgb(221, 225, 231),
    ink: Color32::from_rgb(32, 40, 51),
    muted: Color32::from_rgb(89, 101, 116),
    accent: Color32::from_rgb(66, 95, 134),
    on_accent: Color32::WHITE,
    selected: Color32::WHITE,
    nav_active: Color32::from_rgb(228, 236, 246),
    good: Color32::from_rgb(40, 116, 81),
    good_fill: Color32::from_rgb(238, 245, 241),
    good_line: Color32::from_rgb(200, 224, 211),
    warn: Color32::from_rgb(161, 92, 18),
    stop: Color32::from_rgb(161, 48, 66),
    stop_fill: Color32::from_rgb(255, 245, 245),
    stop_line: Color32::from_rgb(217, 162, 172),
    shadow: Color32::from_black_alpha(40),
};

static MODE: AtomicU8 = AtomicU8::new(0);

pub fn mode() -> Mode {
    if MODE.load(Ordering::Relaxed) == 1 {
        Mode::Light
    } else {
        Mode::Dark
    }
}

/// The active palette. The menu is drawn on one thread; the mode only changes
/// through [`apply`].
pub fn p() -> &'static Palette {
    match mode() {
        Mode::Dark => &DARK,
        Mode::Light => &LIGHT,
    }
}

pub fn apply(ctx: &egui::Context, mode: Mode) {
    MODE.store(mode as u8, Ordering::Relaxed);
    let c = p();
    ctx.set_theme(match mode {
        Mode::Dark => egui::Theme::Dark,
        Mode::Light => egui::Theme::Light,
    });
    let mut style = (*ctx.style()).clone();
    let mut v = match mode {
        Mode::Dark => egui::Visuals::dark(),
        Mode::Light => egui::Visuals::light(),
    };
    v.override_text_color = Some(c.ink);
    v.panel_fill = c.bg;
    v.window_fill = c.panel;
    v.faint_bg_color = c.inset;
    v.extreme_bg_color = c.inset;
    v.code_bg_color = c.inset;
    v.window_stroke = Stroke::new(1., c.line);
    v.window_shadow = egui::Shadow {
        offset: Vec2::new(0., 8.),
        blur: 24.,
        spread: 0.,
        color: c.shadow,
    };
    v.popup_shadow = v.window_shadow;
    v.window_rounding = Rounding::same(10.);
    v.menu_rounding = Rounding::same(8.);
    v.hyperlink_color = c.accent;
    v.selection.bg_fill = c.selected;
    v.selection.stroke = Stroke::new(1., c.accent);
    v.widgets.noninteractive.bg_fill = c.inset;
    v.widgets.noninteractive.weak_bg_fill = c.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1., c.line_soft);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1., c.muted);
    v.widgets.inactive.bg_fill = c.raised;
    v.widgets.inactive.weak_bg_fill = c.raised;
    v.widgets.inactive.fg_stroke = Stroke::new(1., c.ink);
    v.widgets.inactive.bg_stroke = Stroke::new(1., c.line);
    v.widgets.hovered.bg_fill = c.selected;
    v.widgets.hovered.weak_bg_fill = c.selected;
    v.widgets.hovered.bg_stroke = Stroke::new(1., c.accent);
    v.widgets.hovered.fg_stroke = Stroke::new(1., c.ink);
    v.widgets.active = v.widgets.hovered;
    v.widgets.open = v.widgets.hovered;
    for widget in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        widget.rounding = Rounding::same(7.0);
        widget.expansion = 0.;
    }
    style.visuals = v;
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.interact_size = Vec2::new(36.0, 30.0);
    style.spacing.slider_rail_height = 6.0;
    style.spacing.combo_width = 180.0;
    for (text, size) in [
        (egui::TextStyle::Body, 15.0),
        (egui::TextStyle::Button, 14.5),
        (egui::TextStyle::Small, 12.5),
        (egui::TextStyle::Heading, 20.0),
        (egui::TextStyle::Monospace, 13.0),
    ] {
        let family = if text == egui::TextStyle::Monospace {
            egui::FontFamily::Monospace
        } else {
            egui::FontFamily::Proportional
        };
        style.text_styles.insert(text, FontId::new(size, family));
    }
    ctx.set_style(style);
}

// ---------------------------------------------------------------- surfaces --

/// A card on the menu body.
pub fn card() -> egui::Frame {
    egui::Frame::none()
        .fill(p().panel)
        .stroke(Stroke::new(1.0, p().line_soft))
        .rounding(10.0)
        .inner_margin(14.0)
}

/// A floating panel over captured pixels (quick bar, compare controls).
pub fn floating() -> egui::Frame {
    egui::Frame::none()
        .fill(p().panel)
        .stroke(Stroke::new(1.0, p().line))
        .rounding(12.0)
        .inner_margin(8.0)
        .shadow(egui::Shadow {
            offset: Vec2::new(0., 8.),
            blur: 24.,
            spread: 0.,
            color: p().shadow,
        })
}

/// A titled card that fills the available width.
pub fn section<R>(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui) -> R) -> R {
    let width = ui.available_width();
    card()
        .show(ui, |ui| {
            ui.set_width((width - 30.).max(1.));
            heading(ui, title);
            ui.add_space(2.);
            content(ui)
        })
        .inner
}

pub fn heading(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(14.).strong().color(p().ink));
}

pub fn page_title(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(20.).strong().color(p().ink));
    if !subtitle.is_empty() {
        hint(ui, subtitle);
    }
}

pub fn hint(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(egui::Label::new(RichText::new(text.into()).size(12.5).color(p().muted)).wrap())
}

pub fn overline(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(11.)
            .strong()
            .color(p().muted),
    );
}

/// One line of text, truncated with an ellipsis to fit `width`.
pub fn single_line(
    ui: &Ui,
    text: &str,
    size: f32,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat::simple(FontId::proportional(size), color),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    ui.fonts(|f| f.layout_job(job))
}

/// A keyboard shortcut chip.
pub fn kbd(ui: &mut Ui, key: &str) -> Response {
    let font = FontId::monospace(11.5);
    let galley = ui.painter().layout_no_wrap(key.into(), font, p().ink);
    let size = galley.size() + Vec2::new(10., 6.);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect(rect, 4., p().raised, Stroke::new(1., p().line));
    painter.line_segment(
        [
            rect.left_bottom() + Vec2::new(3., 0.),
            rect.right_bottom() - Vec2::new(3., 0.),
        ],
        Stroke::new(1.5, p().line),
    );
    painter.galley(rect.center() - galley.size() / 2., galley, p().ink);
    response
}

/// A compact numeric readout with a caption, for live rates.
pub fn stat(ui: &mut Ui, value: &str, caption: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.;
        ui.label(RichText::new(value).monospace().size(14.).color(p().ink));
        ui.label(RichText::new(caption).size(9.5).strong().color(p().muted));
    });
}

/// "● Rendering · source" status pill.
pub fn status_pill(ui: &mut Ui, running: bool, title: &str, detail: &str) -> Response {
    let c = p();
    let (fill, line, dot) = if running {
        (c.good_fill, c.good_line, c.good)
    } else {
        (c.raised, c.line, c.muted)
    };
    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1., line))
        .rounding(16.)
        .inner_margin(egui::Margin::symmetric(12., 6.))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 7.;
                let (r, _) = ui.allocate_exact_size(Vec2::new(8., 16.), Sense::hover());
                ui.painter().circle_filled(r.center(), 4., dot);
                ui.label(RichText::new(title).strong().size(13.5).color(if running {
                    c.good
                } else {
                    c.muted
                }));
                if !detail.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("· {detail}"))
                                .size(13.)
                                .color(c.muted),
                        )
                        .truncate(),
                    )
                    .on_hover_text(detail);
                }
            });
        })
        .response
}

// ---------------------------------------------------------------- buttons --

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Secondary,
    Danger,
    Ghost,
}

pub fn button(text: impl Into<String>, kind: Kind) -> egui::Button<'static> {
    let c = p();
    let text = text.into();
    let (fill, stroke, color) = match kind {
        Kind::Primary => (c.accent, c.accent, c.on_accent),
        Kind::Secondary => (c.raised, c.line, c.ink),
        Kind::Danger => (c.stop_fill, c.stop_line, c.stop),
        Kind::Ghost => (Color32::TRANSPARENT, Color32::TRANSPARENT, c.ink),
    };
    egui::Button::new(RichText::new(text).color(color).strong())
        .fill(fill)
        .stroke(Stroke::new(1., stroke))
        .min_size(Vec2::new(0., 34.))
}

/// A square button drawing one of our vector icons; always labelled for accessibility.
pub fn icon_button(ui: &mut Ui, icon: Icon, label: &str) -> Response {
    icon_button_colored(ui, icon, label, p().muted)
}

pub fn icon_button_colored(ui: &mut Ui, icon: Icon, label: &str, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(34.), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let c = p();
    let active = response.hovered() || response.has_focus();
    if active {
        ui.painter()
            .rect(rect, 8., c.raised, Stroke::new(1., c.line));
    }
    let color = if !ui.is_enabled() {
        c.line
    } else if active && color == c.muted {
        c.ink
    } else {
        color
    };
    paint_icon(ui.painter(), rect.shrink(8.), icon, color);
    response.on_hover_text(label)
}

// --------------------------------------------------------------- controls --

pub fn toggle(ui: &mut Ui, value: &mut bool, label: &str) -> Response {
    ui.horizontal_wrapped(|ui| {
        let (rect, mut response) = ui.allocate_exact_size(Vec2::new(38., 22.), Sense::click());
        let text = ui.add(egui::Label::new(label).sense(Sense::click()));
        if response.clicked() || text.clicked() {
            *value = !*value;
            response.mark_changed();
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
        });
        paint_switch(
            ui,
            response.id,
            rect,
            *value,
            response.has_focus() || text.hovered(),
        );
        response | text
    })
    .inner
}

pub fn paint_switch(ui: &Ui, id: egui::Id, rect: Rect, on: bool, focus: bool) {
    let c = p();
    let t = ui.ctx().animate_bool_with_time(id.with("switch"), on, 0.12);
    let fill = if !ui.is_enabled() {
        c.line_soft
    } else if on {
        c.accent
    } else {
        c.line
    };
    ui.painter().rect_filled(rect, rect.height() / 2., fill);
    let radius = rect.height() / 2. - 3.;
    let x = egui::lerp(
        (rect.left() + radius + 3.)..=(rect.right() - radius - 3.),
        t,
    );
    let knob = if on { c.on_accent } else { c.ink };
    ui.painter()
        .circle_filled(egui::pos2(x, rect.center().y), radius, knob);
    if focus {
        ui.painter().rect_stroke(
            rect.expand(2.),
            rect.height() / 2. + 2.,
            Stroke::new(1., c.accent),
        );
    }
}

pub struct Segmented {
    pub changed: bool,
}

/// Equal-width segmented choice that fills `width`.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    value: &mut T,
    options: &[(T, &str)],
    width: f32,
) -> Segmented {
    segmented_with(ui, id, value, options, width, |_| true)
}

pub fn segmented_with<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    value: &mut T,
    options: &[(T, &str)],
    width: f32,
    enabled: impl Fn(T) -> bool,
) -> Segmented {
    let c = p();
    let id = ui.make_persistent_id(id);
    let width = width.max(options.len() as f32 * 36.);
    let (outer, response) = ui.allocate_exact_size(Vec2::new(width, 34.), Sense::hover());
    ui.painter()
        .rect(outer, 8., c.inset, Stroke::new(1., c.line_soft));
    let inner = outer.shrink(3.);
    let step = inner.width() / options.len().max(1) as f32;
    let mut changed = false;
    let _ = response;
    for (index, (option, label)) in options.iter().enumerate() {
        let rect = Rect::from_min_size(
            inner.min + Vec2::new(step * index as f32, 0.),
            Vec2::new(step, inner.height()),
        )
        .shrink2(Vec2::new(1., 0.));
        let available = ui.is_enabled() && enabled(*option);
        let sense = if available {
            Sense::click()
        } else {
            Sense::hover()
        };
        let item = ui.interact(rect, id.with(index), sense);
        let selected = *value == *option;
        item.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, available, selected, *label)
        });
        if item.clicked() && !selected {
            *value = *option;
            changed = true;
        }
        let selected = *value == *option;
        if selected {
            ui.painter().rect_filled(rect, 6., c.selected);
            if crate::theme::mode() == Mode::Light {
                ui.painter()
                    .rect_stroke(rect, 6., Stroke::new(1., c.line_soft));
            }
        } else if item.hovered() && available {
            ui.painter().rect_filled(rect, 6., c.raised);
        }
        if item.has_focus() {
            ui.painter()
                .rect_stroke(rect, 6., Stroke::new(1., c.accent));
        }
        let color = if !available {
            c.line
        } else if selected {
            c.ink
        } else {
            c.muted
        };
        let font = FontId::proportional(if step < 64. { 12.5 } else { 13.5 });
        let galley = ui.painter().layout(
            (*label).to_owned(),
            font,
            color,
            (rect.width() - 6.).max(10.),
        );
        ui.painter()
            .galley(rect.center() - galley.size() / 2., galley, color);
    }
    Segmented { changed }
}

#[derive(Clone, Copy)]
pub enum Format {
    Decimal(usize),
    Percent,
    Times,
}
impl Format {
    pub fn show(self, value: f32) -> String {
        match self {
            Self::Decimal(d) => format!("{value:.d$}"),
            Self::Percent => format!("{:.0}%", value * 100.),
            Self::Times => format!("{value:.1}×"),
        }
    }
}

/// Labelled slider with a value chip and a reset button that appears once the
/// value differs from its default. Returns the slider's own response.
pub fn slider(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
) -> Response {
    slider_fmt(ui, label, value, range, default, Format::Decimal(2))
}

pub fn slider_fmt(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
    format: Format,
) -> Response {
    let c = p();
    ui.push_id(label, |ui| {
        let modified = (*value - default).abs() > 1e-4;
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(14.).color(c.ink));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                // Keep the row height stable whether or not reset is shown.
                if ui.add_visible(modified, ResetButton(label)).clicked() {
                    *value = default;
                }
                value_chip(ui, &format.show(*value), modified);
            });
        });
        let width = ui.available_width().max(60.);
        let response = bare_slider(ui, value, range, width).on_hover_text(format!(
            "{label}: {} (default {})",
            format.show(*value),
            format.show(default)
        ));
        ui.add_space(6.);
        response
    })
    .inner
}

/// A slider without label or value, in the menu's accent style.
pub fn bare_slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    width: f32,
) -> Response {
    let c = p();
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = width;
        let visuals = ui.visuals_mut();
        visuals.selection.bg_fill = c.accent;
        visuals.widgets.inactive.bg_fill = c.line;
        for widget in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
        ] {
            widget.fg_stroke = Stroke::new(1.5, c.accent);
        }
        visuals.widgets.hovered.bg_fill = c.ink;
        visuals.widgets.active.bg_fill = c.ink;
        ui.add(
            egui::Slider::new(value, range)
                .show_value(false)
                .trailing_fill(true),
        )
    })
    .inner
}

pub fn value_chip(ui: &mut Ui, text: &str, highlight: bool) -> Response {
    let c = p();
    egui::Frame::none()
        .fill(c.inset)
        .stroke(Stroke::new(
            1.,
            if highlight { c.accent } else { c.line_soft },
        ))
        .rounding(6.)
        .inner_margin(egui::Margin::symmetric(7., 2.))
        .show(ui, |ui| {
            ui.label(RichText::new(text).monospace().size(12.5).color(c.ink));
        })
        .response
}

struct ResetButton<'a>(&'a str);
impl egui::Widget for ResetButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        reset_button(ui, self.0)
    }
}

pub fn reset_button(ui: &mut Ui, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(26.), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("Reset {label}"),
        )
    });
    let c = p();
    let active = response.hovered() || response.has_focus();
    if active {
        ui.painter().rect(
            rect,
            6.,
            c.raised,
            Stroke::new(
                1.,
                if response.has_focus() {
                    c.accent
                } else {
                    c.line
                },
            ),
        );
    }
    paint_icon(
        ui.painter(),
        rect.shrink(6.),
        Icon::Reset,
        if active { c.ink } else { c.accent },
    );
    response.on_hover_text(format!("Reset {label} to its default"))
}

// ------------------------------------------------------------------ icons --

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Source,
    Neural,
    Look,
    Compare,
    Display,
    Diagnostics,
    Pin,
    Stop,
    Minus,
    Close,
    Expand,
    Collapse,
    Pause,
    Search,
    Refresh,
    Swap,
    Reset,
    Tune,
}

/// Stroke icons drawn on a 24-unit grid, independent of font glyph coverage.
pub fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 24.;
    let o = rect.center() - Vec2::splat(12. * s);
    let pt = |x: f32, y: f32| o + Vec2::new(x * s, y * s);
    let stroke = Stroke::new(1.7 * s.max(0.75), color);
    let line = |pts: &[(f32, f32)]| {
        painter.add(egui::Shape::line(
            pts.iter().map(|&(x, y)| pt(x, y)).collect(),
            stroke,
        ));
    };
    let rect_at = |x: f32, y: f32, w: f32, h: f32, r: f32| {
        painter.rect_stroke(
            Rect::from_min_max(pt(x, y), pt(x + w, y + h)),
            r * s,
            stroke,
        );
    };
    match icon {
        Icon::Source => {
            rect_at(3., 4., 18., 12., 2.);
            line(&[(8., 20.), (16., 20.)]);
            line(&[(12., 16.), (12., 20.)]);
        }
        Icon::Neural => {
            line(&[
                (12., 3.),
                (13.8, 7.6),
                (18., 9.),
                (13.8, 10.4),
                (12., 15.),
                (10.2, 10.4),
                (6., 9.),
                (10.2, 7.6),
                (12., 3.),
            ]);
            line(&[
                (18., 15.),
                (18.8, 17.),
                (20.8, 17.8),
                (18.8, 18.6),
                (18., 20.6),
                (17.2, 18.6),
                (15.2, 17.8),
                (17.2, 17.),
                (18., 15.),
            ]);
        }
        Icon::Look => {
            painter.circle_stroke(pt(12., 12.), 9. * s, stroke);
            for (x, y) in [(8., 10.), (12., 7.5), (16., 10.)] {
                painter.circle_filled(pt(x, y), 1.4 * s, color);
            }
            line(&[(9., 16.5), (12., 17.), (15., 16.)]);
        }
        Icon::Compare => {
            rect_at(3., 4., 18., 16., 2.);
            line(&[(12., 4.), (12., 20.)]);
        }
        Icon::Display => {
            line(&[(4., 6.), (14., 6.)]);
            line(&[(18., 6.), (20., 6.)]);
            line(&[(4., 12.), (8., 12.)]);
            line(&[(12., 12.), (20., 12.)]);
            line(&[(4., 18.), (16., 18.)]);
            for (x, y) in [(16., 6.), (10., 12.), (18., 18.)] {
                painter.circle_stroke(pt(x, y), 2. * s, stroke);
            }
        }
        Icon::Diagnostics => line(&[
            (3., 12.),
            (7., 12.),
            (10., 4.),
            (14., 20.),
            (17., 12.),
            (21., 12.),
        ]),
        Icon::Pin => {
            line(&[
                (9., 4.),
                (15., 4.),
                (14., 10.),
                (17., 13.),
                (7., 13.),
                (10., 10.),
                (9., 4.),
            ]);
            line(&[(12., 13.), (12., 20.)]);
        }
        Icon::Stop => {
            painter.rect_filled(Rect::from_min_max(pt(6., 6.), pt(18., 18.)), 2. * s, color);
        }
        Icon::Minus => line(&[(5., 12.), (19., 12.)]),
        Icon::Close => {
            line(&[(6., 6.), (18., 18.)]);
            line(&[(18., 6.), (6., 18.)]);
        }
        Icon::Expand => {
            rect_at(3., 4., 18., 16., 2.);
            line(&[(9., 4.), (9., 20.)]);
        }
        Icon::Collapse => {
            rect_at(3., 4., 18., 16., 2.);
            line(&[(3., 9.), (21., 9.)]);
        }
        Icon::Pause => {
            rect_at(6., 5., 4., 14., 1.);
            rect_at(14., 5., 4., 14., 1.);
        }
        Icon::Search => {
            painter.circle_stroke(pt(11., 11.), 7. * s, stroke);
            line(&[(16.5, 16.5), (20., 20.)]);
        }
        Icon::Refresh => {
            let points: Vec<_> = (0..=20)
                .map(|i| {
                    let a = 0.3 + i as f32 / 20. * 5.2;
                    pt(12. + 8. * a.cos(), 12. + 8. * a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
            line(&[(20., 4.), (20., 10.), (14., 10.)]);
        }
        Icon::Swap => {
            line(&[(4., 8.), (19., 8.), (15., 4.)]);
            line(&[(20., 16.), (5., 16.), (9., 20.)]);
        }
        Icon::Reset => {
            let points: Vec<_> = (0..=24)
                .map(|i| {
                    let a =
                        -std::f32::consts::PI * 0.8 + i as f32 / 24. * std::f32::consts::PI * 1.65;
                    pt(12. + 8. * a.cos(), 12. + 8. * a.sin())
                })
                .collect();
            let tip = points[0];
            painter.add(egui::Shape::line(points, stroke));
            painter.line_segment([tip, tip + Vec2::new(0., -5. * s)], stroke);
            painter.line_segment([tip, tip + Vec2::new(5. * s, 0.)], stroke);
        }
        Icon::Tune => {
            line(&[(6., 4.), (6., 20.)]);
            line(&[(12., 4.), (12., 20.)]);
            line(&[(18., 4.), (18., 20.)]);
            for (x, y) in [(6., 15.), (12., 8.), (18., 13.)] {
                painter.circle_filled(pt(x, y), 2.4 * s, color);
            }
        }
    }
}
