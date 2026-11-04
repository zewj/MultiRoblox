//! Colours, fonts and egui style for the dark MultiRoblox look.

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    TextStyle, Visuals,
};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(13, 15, 21);
pub const SURFACE: Color32 = Color32::from_rgb(17, 20, 28);
pub const CARD: Color32 = Color32::from_rgb(23, 27, 37);
pub const CARD_HOVER: Color32 = Color32::from_rgb(29, 34, 48);
pub const INPUT: Color32 = Color32::from_rgb(12, 14, 20);
pub const BORDER: Color32 = Color32::from_rgb(38, 44, 60);
pub const BORDER_HI: Color32 = Color32::from_rgb(58, 66, 88);
pub const TEXT: Color32 = Color32::from_rgb(231, 233, 240);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(142, 150, 170);
pub const TEXT_DIM: Color32 = Color32::from_rgb(93, 101, 122);
pub const ACCENT: Color32 = Color32::from_rgb(124, 92, 255);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(143, 116, 255);
pub const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(31, 23, 64, 255);
pub const OK: Color32 = Color32::from_rgb(61, 220, 151);
pub const WARN: Color32 = Color32::from_rgb(255, 176, 32);
pub const DANGER: Color32 = Color32::from_rgb(255, 92, 116);
pub const WHITE: Color32 = Color32::from_rgb(255, 255, 255);

pub const RADIUS: u8 = 8;
pub const CARD_RADIUS: u8 = 12;

pub fn semibold() -> FontFamily {
    FontFamily::Name("InterSemiBold".into())
}

pub fn medium() -> FontFamily {
    FontFamily::Name("InterMedium".into())
}

pub fn apply(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "InterRegular".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "InterMedium".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Medium.ttf"
        ))),
    );
    fonts.font_data.insert(
        "InterSemiBold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-SemiBold.ttf"
        ))),
    );
    let fallback: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let with = |primary: &str| {
        let mut v = vec![primary.to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    fonts
        .families
        .insert(FontFamily::Proportional, with("InterRegular"));
    fonts.families.insert(medium(), with("InterMedium"));
    fonts.families.insert(semibold(), with("InterSemiBold"));
    ctx.set_fonts(fonts);

    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = CARD;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(CARD_RADIUS);
    visuals.menu_corner_radius = CornerRadius::same(RADIUS);
    visuals.extreme_bg_color = INPUT;
    visuals.text_edit_bg_color = Some(INPUT);
    visuals.faint_bg_color = CARD_HOVER;
    visuals.code_bg_color = INPUT;
    visuals.hyperlink_color = ACCENT_HOVER;
    visuals.warn_fg_color = WARN;
    visuals.error_fg_color = DANGER;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT_HOVER);
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.striped = false;

    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = CARD;
    w.noninteractive.weak_bg_fill = CARD;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = CornerRadius::same(RADIUS);

    w.inactive.bg_fill = Color32::from_rgb(31, 36, 50);
    w.inactive.weak_bg_fill = Color32::from_rgb(31, 36, 50);
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = CornerRadius::same(RADIUS);
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = Color32::from_rgb(40, 46, 64);
    w.hovered.weak_bg_fill = Color32::from_rgb(40, 46, 64);
    w.hovered.bg_stroke = Stroke::new(1.0, BORDER_HI);
    w.hovered.fg_stroke = Stroke::new(1.0, WHITE);
    w.hovered.corner_radius = CornerRadius::same(RADIUS);
    w.hovered.expansion = 0.0;

    w.active.bg_fill = Color32::from_rgb(48, 54, 76);
    w.active.weak_bg_fill = Color32::from_rgb(48, 54, 76);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, WHITE);
    w.active.corner_radius = CornerRadius::same(RADIUS);
    w.active.expansion = 0.0;

    w.open.bg_fill = Color32::from_rgb(31, 36, 50);
    w.open.weak_bg_fill = Color32::from_rgb(31, 36, 50);
    w.open.bg_stroke = Stroke::new(1.0, BORDER_HI);
    w.open.fg_stroke = Stroke::new(1.0, TEXT);
    w.open.corner_radius = CornerRadius::same(RADIUS);

    ctx.set_visuals(visuals);

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.spacing.interact_size = egui::vec2(40.0, 30.0);
        style.spacing.window_margin = Margin::same(16);
        style.spacing.menu_margin = Margin::same(8);
        style.spacing.text_edit_width = 280.0;
        style.spacing.scroll.floating = false;
        style.spacing.scroll.bar_width = 6.0;
        style.spacing.scroll.bar_inner_margin = 6.0;
        style.spacing.scroll.bar_outer_margin = 0.0;
        style.spacing.scroll.handle_min_length = 24.0;
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, medium())),
            (TextStyle::Heading, FontId::new(22.0, semibold())),
            (
                TextStyle::Monospace,
                FontId::new(12.5, FontFamily::Monospace),
            ),
        ]
        .into();
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    });
}
