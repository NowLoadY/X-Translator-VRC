use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Visuals};

pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::light();

    // Slate palette background colors
    visuals.window_fill = Color32::from_rgb(248, 250, 252);
    visuals.panel_fill = Color32::from_rgb(248, 250, 252);
    visuals.faint_bg_color = Color32::from_rgb(241, 245, 249);
    visuals.extreme_bg_color = Color32::WHITE; // Text inputs, etc.

    // Widget styling (Buttons, Comboboxes, etc.) - Pill-like rounded corners (14px)
    let border_stroke = Stroke::new(1.0, Color32::from_rgb(226, 232, 240));

    visuals.widgets.noninteractive.bg_fill = Color32::WHITE;
    visuals.widgets.noninteractive.bg_stroke = border_stroke;
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(14);
    visuals.widgets.noninteractive.expansion = 0.0;

    visuals.widgets.inactive.bg_fill = Color32::WHITE;
    visuals.widgets.inactive.bg_stroke = border_stroke;
    visuals.widgets.inactive.corner_radius = CornerRadius::same(14);
    visuals.widgets.inactive.expansion = 0.0;

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(241, 245, 249);
    visuals.widgets.hovered.bg_stroke = border_stroke;
    visuals.widgets.hovered.corner_radius = CornerRadius::same(14);
    visuals.widgets.hovered.expansion = 0.0;

    visuals.widgets.active.bg_fill = Color32::from_rgb(226, 232, 240);
    visuals.widgets.active.bg_stroke = border_stroke;
    visuals.widgets.active.corner_radius = CornerRadius::same(14);
    visuals.widgets.active.expansion = 0.0;

    // Text hierarchy
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_normal());
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text_normal());
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, text_strong());
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, text_strong());

    // Selection color (e.g. text selection, active toggle, combobox selection)
    visuals.selection.bg_fill = Color32::from_rgb(219, 234, 254); // Light blue tint
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(37, 99, 235)); // Accent Blue

    // Force active visuals and theme preference to Light
    ctx.set_visuals(visuals.clone());
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Light);

    ctx.all_styles_mut(move |style| {
        style.visuals = visuals.clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.window_margin = Margin::same(14);
        style.spacing.button_padding = egui::vec2(13.0, 8.0);
    });
}

pub fn text_strong() -> Color32 {
    Color32::from_rgb(15, 23, 42) // Slate 900
}

pub fn text_normal() -> Color32 {
    Color32::from_rgb(51, 65, 85) // Slate 700
}

pub fn text_weak() -> Color32 {
    Color32::from_rgb(100, 116, 139) // Slate 500
}

pub fn primary() -> Color32 {
    Color32::from_rgb(37, 99, 235) // Accent Blue
}
