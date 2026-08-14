use crate::ui::components::{self, section};
use eframe::egui;

pub fn render(
    plugin: &mut super::super::OscPlugin,
    ui: &mut egui::Ui,
    language: crate::i18n::UiLanguage,
) -> Vec<super::OscUiAction> {
    let mut actions = Vec::new();
    section(ui, crate::i18n::tr(language, "OSC Network"), |ui| {
        components::feature_ui(
            ui,
            crate::feature_access::Feature::OscChatbox,
            language,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::i18n::tr(language, "Status:"));
                    ui.label(
                        egui::RichText::new(plugin.manager().listener_status())
                            .color(crate::ui::theme::text_weak())
                            .size(12.0),
                    );
                });

                ui.add_space(10.0);

                ui.horizontal_wrapped(|ui| {
                    ui.label("IP:");
                    ui.add(
                        egui::TextEdit::singleline(&mut plugin.draft_mut().ip).desired_width(110.0),
                    );

                    ui.add_space(16.0);
                    ui.label(crate::i18n::tr(language, "Send Port:"));
                    ui.add(
                        egui::DragValue::new(&mut plugin.draft_mut().send_port).range(1..=u16::MAX),
                    );

                    ui.add_space(16.0);
                    ui.label(crate::i18n::tr(language, "Listen Port:"));
                    ui.add(
                        egui::DragValue::new(&mut plugin.draft_mut().listen_port)
                            .range(1..=u16::MAX),
                    );
                });

                ui.add_space(10.0);

                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::i18n::tr(language, "Limit:"));
                    ui.add(
                        egui::DragValue::new(&mut plugin.draft_mut().max_text_length)
                            .range(1..=10_000),
                    );
                });

                ui.add_space(10.0);

                components::modern_slider_f64(
                    ui,
                    &mut plugin.draft_mut().history_ttl_seconds,
                    10.0..=20.0,
                    15.0,
                    crate::i18n::tr(language, "History TTL:"),
                    "s",
                );

                ui.add_space(12.0);

                if components::animated_button(ui, crate::i18n::tr(language, "Apply")).clicked() {
                    actions.push(super::OscUiAction::SettingsApplied(plugin.apply_draft()));
                    actions.push(super::OscUiAction::SaveSettings);
                }
            },
        );
    });
    actions
}
