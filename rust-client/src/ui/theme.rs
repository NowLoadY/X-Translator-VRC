use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Visuals};

pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::light();

    visuals.window_fill = Color32::from_rgba_unmultiplied(248, 250, 252, 196);
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.faint_bg_color = Color32::from_rgb(238, 244, 253);
    visuals.extreme_bg_color = Color32::WHITE;

    let border_stroke = Stroke::new(1.0, Color32::from_rgb(226, 232, 240));

    visuals.widgets.noninteractive.bg_fill = Color32::WHITE;
    visuals.widgets.noninteractive.weak_bg_fill = Color32::WHITE;
    visuals.widgets.noninteractive.bg_stroke = border_stroke;
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(16);
    visuals.widgets.noninteractive.expansion = 0.0;

    visuals.widgets.inactive.bg_fill = Color32::WHITE;
    visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    visuals.widgets.inactive.bg_stroke = border_stroke;
    visuals.widgets.inactive.corner_radius = CornerRadius::same(16);
    visuals.widgets.inactive.expansion = 0.0;

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(248, 250, 252);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(248, 250, 252);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(203, 213, 225));
    visuals.widgets.hovered.corner_radius = CornerRadius::same(16);
    visuals.widgets.hovered.expansion = 0.0;

    visuals.widgets.active.bg_fill = Color32::from_rgb(241, 245, 249);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(241, 245, 249);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(148, 163, 184));
    visuals.widgets.active.corner_radius = CornerRadius::same(16);
    visuals.widgets.active.expansion = 0.0;

    visuals.widgets.open.bg_fill = Color32::WHITE;
    visuals.widgets.open.weak_bg_fill = Color32::WHITE;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, Color32::from_rgb(203, 213, 225));
    visuals.widgets.open.corner_radius = CornerRadius::same(16);
    visuals.widgets.open.expansion = 0.0;

    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_normal());
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text_strong());
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, text_strong());
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, text_strong());
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, text_strong());

    visuals.selection.bg_fill = Color32::from_rgb(239, 246, 255);
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(59, 130, 246));

    visuals.slider_trailing_fill = true;
    visuals.menu_corner_radius = CornerRadius::same(16);
    visuals.popup_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(12),
    };
    visuals.window_shadow = egui::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(12),
    };

    ctx.set_visuals(visuals.clone());
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Light);

    ctx.all_styles_mut(move |style| {
        style.visuals = visuals.clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.window_margin = Margin::same(16);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
    });
}

pub fn text_strong() -> Color32 {
    Color32::from_rgb(30, 41, 59)
}

pub fn text_normal() -> Color32 {
    Color32::from_rgb(71, 85, 105)
}

pub fn text_weak() -> Color32 {
    Color32::from_rgb(148, 163, 184)
}

pub fn primary() -> Color32 {
    Color32::from_rgb(59, 130, 246)
}

pub fn primary_dark() -> Color32 {
    Color32::from_rgb(37, 99, 235)
}

pub fn success() -> Color32 {
    Color32::from_rgb(22, 163, 74)
}

pub fn danger() -> Color32 {
    Color32::from_rgb(220, 38, 38)
}
