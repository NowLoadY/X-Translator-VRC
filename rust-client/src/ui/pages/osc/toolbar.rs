use crate::osc::{BannerConfig, BannerContentType, OscFormatMode};
use crate::ui::components::{self, card};
use eframe::egui;

pub fn render_toolbar(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    let mut changed = false;

    card(ui, |ui| {
        ui.vertical(|ui| {
            // Row 1: OSC Enable Switch on Left, Clear Action on Right
            ui.horizontal(|ui| {
                if ui
                    .checkbox(
                        &mut app.osc_draft.enabled,
                        crate::i18n::tr(app.ui_language, "Enable OSC"),
                    )
                    .changed()
                {
                    changed = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if components::animated_button(
                        ui,
                        crate::i18n::tr(app.ui_language, "Clear Chatbox"),
                    )
                    .clicked()
                    {
                        app.translations.clear();
                        app.recognition_history.clear();
                        app.partial_text.clear();
                        app.pending_final_asr = None;
                        app.osc_manager.clear_chatbox();
                    }
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // Row 2: Text Format Selector & Speaker Number Toggle
            ui.horizontal_wrapped(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(100.0, 20.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.label(
                            egui::RichText::new(crate::i18n::tr(app.ui_language, "Text Format:"))
                                .strong(),
                        );
                    },
                );

                let format_resp = egui::ComboBox::from_id_salt("osc_format_mode")
                    .selected_text(app.osc_draft.format_mode.label(app.ui_language))
                    .show_ui(ui, |ui| {
                        let r1 = ui.selectable_value(
                            &mut app.osc_draft.format_mode,
                            OscFormatMode::BilingualSourceFirst,
                            OscFormatMode::BilingualSourceFirst.label(app.ui_language),
                        );
                        let r2 = ui.selectable_value(
                            &mut app.osc_draft.format_mode,
                            OscFormatMode::BilingualTargetFirst,
                            OscFormatMode::BilingualTargetFirst.label(app.ui_language),
                        );
                        let r3 = ui.selectable_value(
                            &mut app.osc_draft.format_mode,
                            OscFormatMode::Inline,
                            OscFormatMode::Inline.label(app.ui_language),
                        );
                        let r4 = ui.selectable_value(
                            &mut app.osc_draft.format_mode,
                            OscFormatMode::TargetOnly,
                            OscFormatMode::TargetOnly.label(app.ui_language),
                        );
                        r1.changed() || r2.changed() || r3.changed() || r4.changed()
                    });
                if format_resp.inner.unwrap_or(false) {
                    changed = true;
                }

                ui.add_space(16.0);
                if ui
                    .checkbox(
                        &mut app.osc_draft.show_speaker_number,
                        crate::i18n::tr(app.ui_language, "Speaker Number"),
                    )
                    .changed()
                {
                    changed = true;
                }
            });

            ui.add_space(8.0);

            // Row 3: Header Content Selector
            if render_banner_selector(
                ui,
                crate::i18n::tr(app.ui_language, "Header Prefix:"),
                "header_type_combo",
                &mut app.osc_draft.header_config,
                app.ui_language,
            ) {
                changed = true;
            }

            ui.add_space(8.0);

            // Row 4: Footer Content Selector
            if render_banner_selector(
                ui,
                crate::i18n::tr(app.ui_language, "Footer Suffix:"),
                "footer_type_combo",
                &mut app.osc_draft.footer_config,
                app.ui_language,
            ) {
                changed = true;
            }

            ui.add_space(8.0);

            // Row 5: Message TTL Slider (10s ~ 20s)
            if components::modern_slider_f64(
                ui,
                &mut app.osc_draft.history_ttl_seconds,
                10.0..=20.0,
                crate::i18n::tr(app.ui_language, "Message TTL:"),
                "s",
            )
            .changed()
            {
                changed = true;
            }
        });
    });

    if changed {
        let _ = app.osc_manager.update_settings(app.osc_draft.clone());
        app.save_settings();
    }
}

fn render_banner_selector(
    ui: &mut egui::Ui,
    label: &str,
    combo_id: &str,
    banner: &mut BannerConfig,
    language: crate::i18n::UiLanguage,
) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(100.0, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(egui::RichText::new(label).strong());
            },
        );

        let combo_resp = egui::ComboBox::from_id_salt(combo_id)
            .selected_text(banner.content_type.label(language))
            .show_ui(ui, |ui| {
                let r1 = ui.selectable_value(
                    &mut banner.content_type,
                    BannerContentType::None,
                    BannerContentType::None.label(language),
                );
                let r2 = ui.selectable_value(
                    &mut banner.content_type,
                    BannerContentType::CustomText,
                    BannerContentType::CustomText.label(language),
                );
                let r3 = ui.selectable_value(
                    &mut banner.content_type,
                    BannerContentType::SystemTime,
                    BannerContentType::SystemTime.label(language),
                );
                let r4 = ui.selectable_value(
                    &mut banner.content_type,
                    BannerContentType::CpuStatus,
                    BannerContentType::CpuStatus.label(language),
                );
                let r5 = ui.selectable_value(
                    &mut banner.content_type,
                    BannerContentType::GpuStatus,
                    BannerContentType::GpuStatus.label(language),
                );
                r1.changed() || r2.changed() || r3.changed() || r4.changed() || r5.changed()
            });

        if combo_resp.inner.unwrap_or(false) {
            changed = true;
        }

        ui.add_space(8.0);

        match banner.content_type {
            BannerContentType::None => {
                ui.label(
                    egui::RichText::new(crate::i18n::tr(language, "(Off)"))
                        .color(crate::ui::theme::text_weak())
                        .size(12.0),
                );
            }
            BannerContentType::CustomText => {
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut banner.custom_text)
                            .hint_text("e.g. [AFK] or [CN/JP]")
                            .desired_width(150.0),
                    )
                    .changed()
                {
                    changed = true;
                }
            }
            BannerContentType::SystemTime => {
                ui.label(
                    egui::RichText::new(crate::i18n::tr(language, "Auto-synced with system clock"))
                        .color(crate::ui::theme::text_weak())
                        .size(12.0),
                );
            }
            BannerContentType::CpuStatus | BannerContentType::GpuStatus => {
                if ui
                    .checkbox(
                        &mut banner.show_device_name,
                        crate::i18n::tr(language, "Full Name"),
                    )
                    .changed()
                {
                    changed = true;
                }
            }
        }
    });

    changed
}
