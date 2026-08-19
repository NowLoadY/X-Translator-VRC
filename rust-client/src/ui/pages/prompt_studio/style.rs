use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};

pub(super) const GRAPH_ACCENT: Color32 = Color32::from_gray(72);
pub(super) const BAR_FILL: Color32 = Color32::from_rgba_unmultiplied_const(250, 250, 249, 164);
pub(super) const BAR_BORDER: Color32 = Color32::from_gray(194);
pub(super) const INK: Color32 = Color32::from_gray(68);
pub(super) const MUTED: Color32 = Color32::from_gray(112);
pub(super) const CANVAS_FILL: Color32 = Color32::from_rgba_unmultiplied_const(242, 243, 242, 104);
pub(super) const CANVAS_BORDER: Color32 = Color32::from_gray(188);
pub(super) const GRID: Color32 = Color32::from_gray(218);
pub(super) const NODE_TEXT: Color32 = Color32::from_gray(55);
pub(super) const NODE_MUTED: Color32 = Color32::from_gray(105);
pub(super) const NODE_BORDER: Color32 = Color32::from_gray(155);

pub(super) fn apply(ui: &mut egui::Ui) {
    let style = ui.style_mut();
    style.spacing.item_spacing = Vec2::new(7.0, 6.0);
    style.spacing.button_padding = Vec2::new(9.0, 5.0);
    let visuals = &mut style.visuals;
    for widgets in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widgets.corner_radius = CornerRadius::same(1);
        widgets.expansion = 0.0;
    }
    visuals.selection.bg_fill = Color32::from_gray(218);
    visuals.selection.stroke = Stroke::new(1.0, GRAPH_ACCENT);
    visuals.menu_corner_radius = CornerRadius::same(1);
    visuals.popup_shadow = egui::Shadow::NONE;
}

pub(super) fn command_button(ui: &mut egui::Ui, text: &str, filled: bool) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .font(egui::FontId::monospace(9.5))
                .color(if filled { Color32::WHITE } else { INK }),
        )
        .fill(if filled {
            Color32::from_gray(76)
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::new(1.0, BAR_BORDER))
        .corner_radius(CornerRadius::same(1))
        .min_size(Vec2::new(62.0, 25.0)),
    )
}

pub(super) fn provider_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(label)
                .font(egui::FontId::monospace(9.5))
                .color(if selected { Color32::WHITE } else { MUTED }),
        )
        .fill(if selected {
            Color32::from_gray(82)
        } else {
            Color32::TRANSPARENT
        })
        .stroke(if selected {
            Stroke::NONE
        } else {
            Stroke::new(1.0, BAR_BORDER)
        })
        .corner_radius(CornerRadius::same(1))
        .min_size(Vec2::new(70.0, 24.0)),
    )
}
