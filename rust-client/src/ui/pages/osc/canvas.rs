use crate::ui::components::card;
use eframe::egui;

pub fn render_canvas(app: &crate::XRTranslateApp, ui: &mut egui::Ui) {
    let canvas_height = (ui.available_height() - 10.0).max(220.0);

    card(ui, |ui| {
        ui.set_min_height(canvas_height - 32.0);

        let preview = app.osc_manager.chatbox_preview();
        let is_empty = preview.text.trim().is_empty();
        let char_count = preview.text.chars().count();
        let limit = app.osc_draft.max_text_length;

        // Character Gauge at Top-Right
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            let text_color = if char_count > limit {
                egui::Color32::from_rgb(220, 38, 38)
            } else {
                crate::ui::theme::text_weak()
            };
            let lifecycle = if preview.typing {
                crate::i18n::tr(app.ui_language, "Live typing").to_owned()
            } else if let Some(remaining) = preview.expires_in {
                format!(
                    "{} {:.1}s",
                    crate::i18n::tr(app.ui_language, "Expires in"),
                    remaining.as_secs_f64()
                )
            } else {
                crate::i18n::tr(app.ui_language, "Idle").to_owned()
            };
            ui.label(
                egui::RichText::new(format!(
                    "{}/{} {} · {}",
                    char_count,
                    limit,
                    crate::i18n::tr(app.ui_language, "chars"),
                    lifecycle,
                ))
                .color(text_color)
                .size(12.0)
                .strong(),
            );
        });

        ui.add_space(8.0);

        // Center VRChat Chatbox Speech Bubble (Content-driven width up to 380px max!)
        let container_height = (canvas_height - 60.0).max(120.0);

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), container_height),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                ui.add_space(20.0);

                // Dynamic content-sized dark speech bubble (Bounded max width like VRChat)
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(24, 26, 38))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(52, 56, 75)))
                    .corner_radius(egui::CornerRadius::same(12))
                    .inner_margin(egui::Margin::symmetric(20, 14))
                    .show(ui, |ui| {
                        ui.set_max_width(380.0);
                        if is_empty {
                            ui.label(
                                egui::RichText::new(crate::i18n::tr(
                                    app.ui_language,
                                    "(Chatbox Cleared / Empty)",
                                ))
                                .family(egui::FontFamily::Monospace)
                                .color(egui::Color32::from_rgb(120, 128, 150))
                                .size(13.0)
                                .italics(),
                            );
                        } else {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&preview.text)
                                        .family(egui::FontFamily::Monospace)
                                        .color(egui::Color32::from_rgb(240, 244, 255))
                                        .size(13.5),
                                )
                                .wrap(),
                            );
                        }
                    });
            },
        );
    });
}
