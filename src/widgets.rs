//! Reusable bits of chrome: cards, pills, toggles, stat tiles and button styles.

use crate::theme;
use egui::{
    Align, Color32, CornerRadius, Frame, Layout, Margin, Response, RichText, Sense, Stroke, Ui,
    Widget,
};

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(theme::CARD)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(CornerRadius::same(theme::CARD_RADIUS))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn title(text: &str) -> RichText {
    RichText::new(text)
        .size(16.0)
        .family(theme::semibold())
        .color(theme::TEXT)
}

pub fn muted(text: &str) -> RichText {
    RichText::new(text).size(13.0).color(theme::TEXT_MUTED)
}

pub fn small_dim(text: &str) -> RichText {
    RichText::new(text).size(12.0).color(theme::TEXT_DIM)
}

pub fn section(ui: &mut Ui, heading: &str, sub: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(title(heading));
        if !sub.is_empty() {
            ui.label(muted(sub));
        }
    });
}

/// Small coloured dot followed by text, drawn in a rounded capsule.
/// Painted by hand so it keeps its size and order in any parent layout.
pub fn pill(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = egui::FontId::new(12.5, theme::medium());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let (pad_x, pad_y, dot, gap) = (10.0, 5.0, 8.0, 7.0);
    let size = egui::vec2(
        pad_x * 2.0 + dot + gap + galley.size().x,
        pad_y * 2.0 + galley.size().y.max(dot),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let radius = rect.height() / 2.0;
        let painter = ui.painter();
        painter.rect_filled(rect, radius, color.gamma_multiply(0.14));
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, color.gamma_multiply(0.45)),
            egui::StrokeKind::Inside,
        );
        let dot_center = egui::pos2(rect.left() + pad_x + dot / 2.0, rect.center().y);
        painter.circle_filled(dot_center, dot / 2.0, color);
        let text_pos = egui::pos2(
            rect.left() + pad_x + dot + gap,
            rect.center().y - galley.size().y / 2.0,
        );
        painter.galley(text_pos, galley, color);
    }
    response
}

pub fn stat_tile(ui: &mut Ui, label: &str, value: &str, sub: &str, color: Color32) {
    card(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.label(
            RichText::new(label.to_uppercase())
                .size(11.0)
                .family(theme::medium())
                .color(theme::TEXT_DIM),
        );
        ui.label(
            RichText::new(value)
                .size(24.0)
                .family(theme::semibold())
                .color(color),
        );
        ui.label(small_dim(sub));
    });
}

/// iOS-style toggle switch.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let size = egui::vec2(44.0, 24.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(response.id, *on);
        let off = Color32::from_rgb(44, 50, 68);
        let bg = off.lerp_to_gamma(theme::ACCENT, t);
        let radius = rect.height() / 2.0;
        ui.painter().rect_filled(rect, radius, bg);
        if response.hovered() {
            ui.painter().rect_stroke(
                rect,
                radius,
                Stroke::new(1.0, theme::BORDER_HI),
                egui::StrokeKind::Inside,
            );
        }
        let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        ui.painter()
            .circle_filled(egui::pos2(x, rect.center().y), radius - 4.0, theme::WHITE);
    }
    response
}

pub enum ButtonKind {
    Primary,
    Ghost,
    Danger,
}

pub fn button(ui: &mut Ui, text: &str, kind: ButtonKind) -> Response {
    let (fill, stroke, fg) = match kind {
        ButtonKind::Primary => (theme::ACCENT, Stroke::NONE, theme::WHITE),
        ButtonKind::Ghost => (
            Color32::TRANSPARENT,
            Stroke::new(1.0, theme::BORDER_HI),
            theme::TEXT,
        ),
        ButtonKind::Danger => (
            Color32::TRANSPARENT,
            Stroke::new(1.0, theme::DANGER.gamma_multiply(0.55)),
            theme::DANGER,
        ),
    };
    let label = RichText::new(text)
        .family(theme::medium())
        .size(13.5)
        .color(fg);
    let mut b = egui::Button::new(label)
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .min_size(egui::vec2(0.0, 34.0));
    if matches!(kind, ButtonKind::Primary) {
        b = b.frame(true);
    }
    let hover_fill = match kind {
        ButtonKind::Primary => theme::ACCENT_HOVER,
        ButtonKind::Ghost => Color32::from_rgb(31, 36, 50),
        ButtonKind::Danger => theme::DANGER.gamma_multiply(0.16),
    };
    ui.scope(|ui| {
        let v = &mut ui.style_mut().visuals.widgets;
        v.hovered.weak_bg_fill = hover_fill;
        v.hovered.bg_fill = hover_fill;
        v.active.weak_bg_fill = hover_fill;
        v.active.bg_fill = hover_fill;
        v.inactive.weak_bg_fill = fill;
        v.inactive.bg_fill = fill;
        v.hovered.bg_stroke = stroke;
        v.active.bg_stroke = stroke;
        b.ui(ui)
    })
    .inner
}

pub fn small_button(ui: &mut Ui, text: &str, kind: ButtonKind) -> Response {
    let (fill, stroke, fg) = match kind {
        ButtonKind::Primary => (theme::ACCENT, Stroke::NONE, theme::WHITE),
        ButtonKind::Ghost => (
            Color32::TRANSPARENT,
            Stroke::new(1.0, theme::BORDER_HI),
            theme::TEXT,
        ),
        ButtonKind::Danger => (
            Color32::TRANSPARENT,
            Stroke::new(1.0, theme::DANGER.gamma_multiply(0.55)),
            theme::DANGER,
        ),
    };
    let hover_fill = match kind {
        ButtonKind::Primary => theme::ACCENT_HOVER,
        ButtonKind::Ghost => Color32::from_rgb(31, 36, 50),
        ButtonKind::Danger => theme::DANGER.gamma_multiply(0.16),
    };
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(10.0, 4.0);
        let v = &mut ui.style_mut().visuals.widgets;
        v.hovered.weak_bg_fill = hover_fill;
        v.hovered.bg_fill = hover_fill;
        v.active.weak_bg_fill = hover_fill;
        v.active.bg_fill = hover_fill;
        v.hovered.bg_stroke = stroke;
        v.active.bg_stroke = stroke;
        egui::Button::new(
            RichText::new(text)
                .size(12.5)
                .family(theme::medium())
                .color(fg),
        )
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(6))
        .min_size(egui::vec2(0.0, 26.0))
        .ui(ui)
    })
    .inner
}

/// A settings row: title + description on the left, a toggle on the right.
pub fn setting_row(ui: &mut Ui, heading: &str, description: &str, value: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_min_height(44.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(
                RichText::new(heading)
                    .size(14.0)
                    .family(theme::medium())
                    .color(theme::TEXT),
            );
            ui.label(muted(description));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed = toggle(ui, value).changed();
        });
    });
    changed
}

pub fn divider(ui: &mut Ui) {
    let rect = ui.available_rect_before_wrap();
    let y = rect.top() + 4.0;
    ui.painter()
        .hline(rect.x_range(), y, Stroke::new(1.0, theme::BORDER));
    ui.add_space(9.0);
}
