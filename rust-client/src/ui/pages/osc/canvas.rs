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

        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            let (bg_color, text_color) = if char_count > limit {
                (
                    egui::Color32::from_rgb(254, 242, 242),
                    egui::Color32::from_rgb(220, 38, 38),
                )
            } else {
                (
                    egui::Color32::from_rgb(239, 246, 255),
                    crate::ui::theme::primary_dark(),
                )
            };
            let lifecycle = if preview.typing {
                Some(crate::i18n::tr(app.ui_language, "Live").to_owned())
            } else {
                preview
                    .next_message_expires_in
                    .map(|remaining| format!("{:.1}s", remaining.as_secs_f64()))
            };
            let status = lifecycle.map_or_else(
                || format!("{char_count}/{limit}"),
                |lifecycle| format!("{char_count}/{limit} · {lifecycle}"),
            );

            egui::Frame::new()
                .fill(bg_color)
                .stroke(egui::Stroke::NONE)
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(10, 4))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(status)
                            .color(text_color)
                            .size(11.5)
                            .strong(),
                    );
                });
        });

        ui.add_space(8.0);

        let container_height = (canvas_height - 60.0).max(120.0);

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), container_height),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                ui.add_space(20.0);

                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(15, 23, 42))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)))
                    .corner_radius(egui::CornerRadius::same(16))
                    .inner_margin(egui::Margin::symmetric(20, 14))
                    .show(ui, |ui| {
                        ui.set_max_width(380.0);
                        if is_empty {
                            ui.label(
                                egui::RichText::new(crate::i18n::tr(app.ui_language, "Empty"))
                                    .family(egui::FontFamily::Monospace)
                                    .color(egui::Color32::from_rgb(100, 116, 139))
                                    .size(13.0)
                                    .italics(),
                            );
                        } else {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&preview.text)
                                        .family(egui::FontFamily::Monospace)
                                        .color(egui::Color32::from_rgb(241, 245, 249))
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
