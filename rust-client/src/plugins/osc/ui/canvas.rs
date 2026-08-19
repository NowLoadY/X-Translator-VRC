use crate::ui::components::card;
use eframe::egui;

pub fn render_canvas(
    plugin: &mut super::super::OscPlugin,
    ui: &mut egui::Ui,
    language: crate::i18n::UiLanguage,
) {
    card(ui, |ui| {
        ui.set_min_height(140.0);

        let preview = plugin.manager().chatbox_preview();
        let is_empty = preview.text.trim().is_empty();
        let char_count = preview.text.chars().count();
        let limit = plugin.draft().max_text_length;

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
                Some(crate::i18n::tr(language, "Live").to_owned())
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

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 90.0),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                ui.add_space(6.0);

                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(15, 23, 42))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(51, 65, 85)))
                    .corner_radius(egui::CornerRadius::same(16))
                    .inner_margin(egui::Margin::symmetric(20, 14))
                    .show(ui, |ui| {
                        ui.set_max_width(380.0);
                        if is_empty {
                            ui.label(
                                egui::RichText::new(crate::i18n::tr(language, "Empty"))
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

        ui.add_space(6.0);
    });
}

pub fn render_bottom_input_bar(
    plugin: &mut super::super::OscPlugin,
    ui: &mut egui::Ui,
    language: crate::i18n::UiLanguage,
) {
    let is_enabled = plugin.draft().enabled;
    let mut submit = false;
    let has_text = !plugin.draft_input().trim().is_empty();

    card(ui, |ui| {
        let mut request_focus_back = false;
        let mut text_edit_response = None;

        ui.horizontal(|ui| {
            let send_btn_width = 76.0;
            let spacing = 8.0;
            let input_frame_width = (ui.available_width() - send_btn_width - spacing).max(120.0);
            let input_content_width = (input_frame_width - 28.0).max(72.0);

            egui::Frame::new()
                .fill(egui::Color32::from_rgb(248, 250, 252))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(226, 232, 240),
                ))
                .corner_radius(egui::CornerRadius::same(14))
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.set_width(input_content_width);
                    ui.horizontal(|ui| {
                        let text_frame = egui::Frame::new()
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE)
                            .corner_radius(egui::CornerRadius::ZERO)
                            .inner_margin(egui::Margin::ZERO);

                        let clear_width = if has_text { 32.0 } else { 0.0 };
                        let edit_width = (input_content_width - clear_width - 8.0).max(50.0);

                        let edit_resp = ui.add_enabled(
                            is_enabled,
                            egui::TextEdit::singleline(plugin.draft_input_mut())
                                .id_salt("osc_bottom_chatbox_input")
                                .hint_text(crate::i18n::tr(
                                    language,
                                    "Type a message to Chatbox (Press Enter to send)...",
                                ))
                                .frame(text_frame)
                                .margin(egui::Margin::ZERO)
                                .desired_width(edit_width),
                        );

                        if edit_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if has_text {
                                submit = true;
                            }
                            request_focus_back = true;
                        }

                        if has_text {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let clear_btn = egui::Frame::new()
                                        .fill(egui::Color32::from_rgb(226, 232, 240))
                                        .corner_radius(egui::CornerRadius::same(10))
                                        .inner_margin(egui::Margin::symmetric(6, 2))
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new("×")
                                                    .color(egui::Color32::from_rgb(100, 116, 139))
                                                    .size(12.0)
                                                    .strong(),
                                            )
                                        })
                                        .response
                                        .interact(egui::Sense::click());
                                    if clear_btn.clicked() {
                                        plugin.draft_input_mut().clear();
                                        request_focus_back = true;
                                    }
                                },
                            );
                        }

                        text_edit_response = Some(edit_resp);
                    });
                });

            ui.add_space(spacing);

            let send_btn = crate::ui::components::primary_button_enabled(
                ui,
                crate::i18n::tr(language, "Send"),
                is_enabled && has_text,
            );
            if send_btn.clicked() {
                submit = true;
                request_focus_back = true;
            }
        });

        if let Some(resp) = text_edit_response {
            if request_focus_back {
                resp.request_focus();
            }
        }

        if submit && is_enabled && has_text {
            let text = plugin.draft_input().trim().to_string();
            plugin.send_manual_message(&text);
            plugin.draft_input_mut().clear();
            ui.ctx().request_repaint();
        }
    });
}
