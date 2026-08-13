use crate::osc::{BannerConfig, BannerContentType, OscFormatMode, OscMessageSeparator};
use crate::ui::components::{self, card};
use eframe::egui;
use std::sync::atomic::Ordering;

pub fn render_toolbar(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    let mut changed = false;

    card(ui, |ui| {
        components::feature_ui(
            ui,
            crate::feature_access::Feature::OscChatbox,
            app.ui_language,
            |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if components::feature_checkbox(
                            ui,
                            crate::feature_access::Feature::OscChatbox,
                            app.ui_language,
                            &mut app.osc_draft.enabled,
                            "OSC",
                        )
                        .changed()
                        {
                            changed = true;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if components::animated_button(
                                ui,
                                crate::i18n::tr(app.ui_language, "Clear"),
                            )
                            .clicked()
                            {
                                app.clear_history();
                            }
                        });
                    });

                    ui.add_space(8.0);
                    let mut mute_gate_enabled =
                        app.mute_self_pauses_translation.load(Ordering::Acquire);
                    if components::feature_checkbox(
                        ui,
                        crate::feature_access::Feature::MuteSync,
                        app.ui_language,
                        &mut mute_gate_enabled,
                        crate::i18n::tr(app.ui_language, "Pause while muted"),
                    )
                    .changed()
                    {
                        app.set_mute_self_pauses_translation(mute_gate_enabled);
                    }

                    ui.add_space(8.0);
                    components::wavy_divider(ui, egui::Color32::from_rgb(226, 232, 240));
                    ui.add_space(8.0);

                    ui.horizontal_wrapped(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(100.0, 20.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new(crate::i18n::tr(
                                        app.ui_language,
                                        "Format:",
                                    ))
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
                        let mut speaker_number_enabled = app.osc_draft.show_speaker_number;
                        if components::feature_checkbox(
                            ui,
                            crate::feature_access::Feature::SpeakerNumbers,
                            app.ui_language,
                            &mut speaker_number_enabled,
                            crate::i18n::tr(app.ui_language, "Speaker numbers"),
                        )
                        .changed()
                        {
                            app.set_osc_speaker_number_enabled(speaker_number_enabled);
                        }
                    });

                    ui.add_space(8.0);

                    let target_only = app.osc_draft.format_mode == OscFormatMode::TargetOnly;
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(100.0, 20.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new(crate::i18n::tr(
                                        app.ui_language,
                                        if target_only {
                                            "Between messages:"
                                        } else {
                                            "Message layout:"
                                        },
                                    ))
                                    .strong(),
                                );
                            },
                        );
                        let response = egui::ComboBox::from_id_salt("osc_message_separator")
                            .selected_text(
                                app.osc_draft
                                    .message_separator
                                    .layout_label(app.ui_language, target_only),
                            )
                            .show_ui(ui, |ui| {
                                let mut selection_changed = false;
                                for value in
                                    [OscMessageSeparator::NewLine, OscMessageSeparator::Space]
                                {
                                    selection_changed |= ui
                                        .selectable_value(
                                            &mut app.osc_draft.message_separator,
                                            value,
                                            value.layout_label(app.ui_language, target_only),
                                        )
                                        .changed();
                                }
                                selection_changed
                            });
                        if response.inner.unwrap_or(false) {
                            changed = true;
                        }
                    });
                    ui.add_space(8.0);

                    if render_banner_selector(
                        ui,
                        crate::i18n::tr(app.ui_language, "Header:"),
                        "header_type_combo",
                        &mut app.osc_draft.header_config,
                        app.ui_language,
                    ) {
                        changed = true;
                    }

                    ui.add_space(8.0);

                    if render_banner_selector(
                        ui,
                        crate::i18n::tr(app.ui_language, "Footer:"),
                        "footer_type_combo",
                        &mut app.osc_draft.footer_config,
                        app.ui_language,
                    ) {
                        changed = true;
                    }

                    ui.add_space(8.0);

                    if components::modern_slider_f64(
                        ui,
                        &mut app.osc_draft.history_ttl_seconds,
                        10.0..=20.0,
                        15.0,
                        "TTL:",
                        "s",
                    )
                    .changed()
                    {
                        changed = true;
                    }
                })
            },
        );
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
            BannerContentType::None => {}
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
            BannerContentType::SystemTime => {}
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
