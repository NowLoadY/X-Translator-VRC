pub mod canvas;
pub mod toolbar;

use crate::ui::components::card;
use eframe::egui;

pub fn render(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new(crate::i18n::tr(app.ui_language, "VRChat OSC Studio"))
            .size(22.0)
            .color(crate::ui::theme::text_strong())
            .strong(),
    );

    ui.add_space(12.0);

    if let Some(error) = &app.last_error {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("⚠").color(egui::Color32::from_rgb(220, 38, 38)));
                ui.label(egui::RichText::new(error).color(egui::Color32::from_rgb(220, 38, 38)));
            });
        });
        ui.add_space(10.0);
    }

    toolbar::render_toolbar(app, ui);

    ui.add_space(12.0);

    canvas::render_canvas(app, ui);
}
