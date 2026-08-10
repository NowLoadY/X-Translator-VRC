use crate::ui::components::{self, danger_button, primary_button, section, status_badge};
use crate::{
    CaptureSource, LANGUAGE_OPTIONS, language_label, route_label,
};
use eframe::egui;

pub fn render(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    // Header
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(crate::i18n::tr(app.ui_language, "Translation"))
                .size(22.0)
                .color(crate::ui::theme::text_strong())
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (is_active, is_error) = if app.connection_status.contains("error") {
                (false, true)
            } else if app.is_translating {
                (true, false)
            } else {
                (false, false)
            };
            let status = crate::i18n::tr_dynamic(app.ui_language, &app.connection_status);
            status_badge(ui, status.as_ref(), is_active, is_error);

            ui.add_space(10.0);
            let btn_text = if app.floating_subtitles_enabled {
                crate::i18n::tr(app.ui_language, "💬 Hide Subtitle Window")
            } else {
                crate::i18n::tr(app.ui_language, "🖥️ Desktop Subtitle Window")
            };
            if components::animated_button(ui, btn_text).clicked() {
                app.floating_subtitles_enabled = !app.floating_subtitles_enabled;
                app.save_settings();
            }
        });
    });

    if let Some(error) = &app.last_error {
        ui.add_space(8.0);
        components::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("!")
                        .color(egui::Color32::from_rgb(220, 38, 38))
                        .strong(),
                );
                ui.label(egui::RichText::new(error).color(egui::Color32::from_rgb(220, 38, 38)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if components::animated_button(
                        ui,
                        crate::i18n::tr(app.ui_language, "View Detailed Log"),
                    )
                    .clicked()
                    {
                        let log = app.backend_manager.get_latest_log();
                        app.modal_dialog = crate::ui::modal::ModalDialog::error(
                            crate::i18n::tr(app.ui_language, "Detailed Error Traceback"),
                            error,
                            Some(&log),
                        );
                    }
                });
            });
        });
    }

    ui.add_space(14.0);

    // Section 1: Voice Route
    section(ui, crate::i18n::tr(app.ui_language, "Voice Route"), |ui| {
        let previous_source = app.source_lang.clone();
        let previous_target = app.target_lang.clone();

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr(app.ui_language, "Input:"))
                    .color(crate::ui::theme::text_strong())
                    .strong(),
            );
            egui::ComboBox::from_id_salt("source_language")
                .selected_text(language_label(app.ui_language, &app.source_lang))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.source_lang,
                        "auto".into(),
                        crate::i18n::tr(app.ui_language, "Auto (bidirectional)"),
                    );
                    for (code, label) in LANGUAGE_OPTIONS {
                        ui.selectable_value(
                            &mut app.source_lang,
                            (*code).into(),
                            crate::i18n::tr(app.ui_language, label),
                        );
                    }
                });

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("↔")
                    .color(crate::ui::theme::text_weak())
                    .strong(),
            );
            ui.add_space(8.0);

            if app.source_lang == "auto" {
                ui.label(
                    egui::RichText::new(crate::i18n::tr(app.ui_language, "Pair:"))
                        .color(crate::ui::theme::text_strong())
                        .strong(),
                );
                let (mut a, mut b) = match app.target_lang.split_once(',') {
                    Some((x, y)) => (x.to_string(), y.to_string()),
                    None => ("zh".to_string(), "en".to_string()),
                };

                egui::ComboBox::from_id_salt("target_language_a")
                    .selected_text(language_label(app.ui_language, &a))
                    .show_ui(ui, |ui| {
                        for (code, label) in LANGUAGE_OPTIONS {
                            ui.selectable_value(
                                &mut a,
                                (*code).into(),
                                crate::i18n::tr(app.ui_language, label),
                            );
                        }
                    });

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("↔")
                        .color(crate::ui::theme::text_weak())
                        .strong(),
                );
                ui.add_space(4.0);

                egui::ComboBox::from_id_salt("target_language_b")
                    .selected_text(language_label(app.ui_language, &b))
                    .show_ui(ui, |ui| {
                        for (code, label) in LANGUAGE_OPTIONS {
                            ui.selectable_value(
                                &mut b,
                                (*code).into(),
                                crate::i18n::tr(app.ui_language, label),
                            );
                        }
                    });

                let new_target = format!("{a},{b}");
                if new_target != app.target_lang {
                    app.target_lang = new_target;
                }
            } else {
                ui.label(
                    egui::RichText::new(crate::i18n::tr(app.ui_language, "Target:"))
                        .color(crate::ui::theme::text_strong())
                        .strong(),
                );
                egui::ComboBox::from_id_salt("target_language")
                    .selected_text(route_label(
                        app.ui_language,
                        &app.source_lang,
                        &app.target_lang,
                    ))
                    .show_ui(ui, |ui| {
                        for (code, label) in LANGUAGE_OPTIONS {
                            ui.selectable_value(
                                &mut app.target_lang,
                                (*code).into(),
                                crate::i18n::tr(app.ui_language, label),
                            );
                        }
                    });
            }
        });

        if app.source_lang != previous_source || app.target_lang != previous_target {
            app.apply_language_route();
        }
    });

    ui.add_space(10.0);

    // Section 2: Audio Input & Action Bar
    section(ui, crate::i18n::tr(app.ui_language, "Audio Input"), |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr(app.ui_language, "Source:"))
                    .color(crate::ui::theme::text_strong())
                    .strong(),
            );
            let previous_source = app.capture_source;
            egui::ComboBox::from_id_salt("capture_source")
                .selected_text(match app.capture_source {
                    CaptureSource::Microphone => crate::i18n::tr(app.ui_language, "Microphone"),
                    CaptureSource::SystemAudio => {
                        crate::i18n::tr(app.ui_language, "System Audio (WASAPI)")
                    }
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.capture_source,
                        CaptureSource::Microphone,
                        crate::i18n::tr(app.ui_language, "Microphone"),
                    );
                    ui.selectable_value(
                        &mut app.capture_source,
                        CaptureSource::SystemAudio,
                        crate::i18n::tr(app.ui_language, "System Audio (WASAPI)"),
                    );
                });
            if app.capture_source != previous_source {
                app.switch_capture_source(previous_source);
            }

            ui.add_space(6.0);
            if components::animated_button(ui, crate::i18n::tr(app.ui_language, "Refresh"))
                .clicked()
            {
                app.devices = app.audio_system.available_devices();
                app.loopback_devices = app.audio_system.available_loopback_devices();
                app.refresh_selected_input_config();
            }
        });

        ui.add_space(8.0);
        render_capture_device_selector(app, ui);

        ui.add_space(12.0);

        // Integrated Frosted Action Bar
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(248, 250, 252))
            .corner_radius(egui::CornerRadius::same(10))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(226, 232, 240)))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if app.is_translating {
                        if danger_button(ui, crate::i18n::tr(app.ui_language, "Stop Translation")).clicked()
                        {
                            app.stop();
                        }
                    } else {
                        if primary_button(ui, crate::i18n::tr(app.ui_language, "Start Translation"))
                            .clicked()
                        {
                            app.start(Some(ui.ctx().clone()));
                        }
                    }

                    if let Some(config) = &app.selected_input_config {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} Hz, {} ch ({})",
                                    config.sample_rate,
                                    config.channels,
                                    config.sample_format
                                ))
                                .color(crate::ui::theme::text_weak())
                                .size(11.5),
                            );
                        });
                    }
                });
            });
    });

    ui.add_space(10.0);

    // Section 3: Dual History Panels (Recognition & Translation)
    ui.columns(2, |columns| {
        // Recognition Panel
        section(
            &mut columns[0],
            &format!(
                "{} ({})",
                crate::i18n::tr(app.ui_language, "Recognition History"),
                app.recognition_history.len()
            ),
            |ui| {
                let history_height = (ui.available_height() - 10.0).max(180.0);
                ui.set_min_height(history_height);

                egui::ScrollArea::vertical()
                    .id_salt("recognition_history_scroll")
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if app.recognition_history.is_empty() && app.partial_text.is_empty() {
                            ui.label(
                                egui::RichText::new(crate::i18n::tr(
                                    app.ui_language,
                                    "No speech recognized yet...",
                                ))
                                .color(crate::ui::theme::text_weak())
                                .italics(),
                            );
                        } else {
                            let total = app.recognition_history.len();
                            for (i, entry) in app.recognition_history.iter().enumerate() {
                                let is_last = i == total - 1 && app.partial_text.is_empty();
                                ui.horizontal_wrapped(|ui| {
                                    if let Some(speaker) =
                                        crate::compact_speaker_label(&entry.speaker_id)
                                    {
                                        ui.label(
                                            egui::RichText::new(speaker)
                                                .color(egui::Color32::from_rgb(37, 99, 235))
                                                .size(11.5)
                                                .strong(),
                                        );
                                    }
                                    if entry.source_end_ms > entry.source_start_ms {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}–{}",
                                                crate::format_timeline_ms(entry.source_start_ms),
                                                crate::format_timeline_ms(entry.source_end_ms),
                                            ))
                                            .color(crate::ui::theme::text_weak())
                                            .size(10.5),
                                        );
                                    }
                                });
                                let resp = ui.label(
                                    egui::RichText::new(&entry.text)
                                        .color(crate::ui::theme::text_normal())
                                        .size(13.0),
                                );
                                if is_last {
                                    resp.scroll_to_me(Some(egui::Align::BOTTOM));
                                }
                                ui.add_space(4.0);
                            }
                            if !app.partial_text.is_empty() {
                                let resp = ui.label(
                                    egui::RichText::new(&app.partial_text)
                                        .color(egui::Color32::from_rgb(37, 99, 235))
                                        .size(13.0)
                                        .italics(),
                                );
                                resp.scroll_to_me(Some(egui::Align::BOTTOM));
                            }
                        }
                    });
            },
        );

        // Translation Panel
        section(
            &mut columns[1],
            &format!(
                "{} ({})",
                crate::i18n::tr(app.ui_language, "Translation History"),
                app.translations.len()
            ),
            |ui| {
                let history_height = (ui.available_height() - 10.0).max(180.0);
                ui.set_min_height(history_height);

                egui::ScrollArea::vertical()
                    .id_salt("translation_history_scroll")
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if app.translations.is_empty() {
                            ui.label(
                                egui::RichText::new(crate::i18n::tr(
                                    app.ui_language,
                                    "No translations emitted yet...",
                                ))
                                .color(crate::ui::theme::text_weak())
                                .italics(),
                            );
                        } else {
                            let total = app.translations.len();
                            for (i, entry) in app.translations.iter().enumerate() {
                                let is_last = i == total - 1;
                                let group_resp = egui::Frame::new()
                                    .fill(egui::Color32::from_rgb(248, 250, 252))
                                    .corner_radius(egui::CornerRadius::same(8))
                                    .inner_margin(egui::Margin::symmetric(10, 8))
                                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(226, 232, 240)))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal_wrapped(|ui| {
                                            if let Some(speaker) =
                                                crate::compact_speaker_label(&entry.speaker_id)
                                            {
                                                ui.label(
                                                    egui::RichText::new(speaker)
                                                        .color(egui::Color32::from_rgb(37, 99, 235))
                                                        .size(11.5)
                                                        .strong(),
                                                );
                                            }
                                            if entry.source_end_ms > entry.source_start_ms {
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "{}–{}",
                                                        crate::format_timeline_ms(
                                                            entry.source_start_ms,
                                                        ),
                                                        crate::format_timeline_ms(
                                                            entry.source_end_ms,
                                                        ),
                                                    ))
                                                    .color(crate::ui::theme::text_weak())
                                                    .size(10.5),
                                                );
                                            }
                                        });
                                        ui.label(
                                            egui::RichText::new(&entry.source)
                                                .color(crate::ui::theme::text_weak())
                                                .size(11.5),
                                        );
                                        ui.add_space(2.0);
                                        ui.label(
                                            egui::RichText::new(&entry.translated)
                                                .color(crate::ui::theme::text_strong())
                                                .size(13.0)
                                                .strong(),
                                        );
                                    })
                                    .response;

                                if is_last {
                                    group_resp.scroll_to_me(Some(egui::Align::BOTTOM));
                                }
                                ui.add_space(4.0);
                            }
                        }
                    });
            },
        );
    });
}

fn render_capture_device_selector(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    let level =
        f32::from_bits(app.input_level.load(std::sync::atomic::Ordering::Relaxed)).clamp(0.0, 1.0);
    let decibels = 20.0 * level.max(0.000_001).log10();
    let raw_fraction = ((decibels + 60.0) / 60.0).clamp(0.0, 1.0);
    let animated_fraction = crate::ui::animation::AnimationSystem::smooth_audio_level(
        ui.ctx(),
        "input_level_meter",
        raw_fraction,
    );

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(crate::i18n::tr(app.ui_language, "Device:"))
                .color(crate::ui::theme::text_strong())
                .strong(),
        );
        match app.capture_source {
            CaptureSource::Microphone => {
                let previous_device = app.selected_device_id.clone();
                let current_name = app
                    .devices
                    .iter()
                    .find(|device| device.id == app.selected_device_id)
                    .map(|device| device.name.as_str())
                    .unwrap_or(crate::i18n::tr(app.ui_language, "Default microphone"));

                egui::ComboBox::from_id_salt("mic_device_selector")
                    .selected_text(current_name)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut app.selected_device_id,
                            String::new(),
                            crate::i18n::tr(app.ui_language, "Default microphone"),
                        );
                        for device in &app.devices {
                            ui.selectable_value(
                                &mut app.selected_device_id,
                                device.id.clone(),
                                &device.name,
                            );
                        }
                    });

                if app.selected_device_id != previous_device {
                    app.switch_capture_device(previous_device);
                }
            }
            CaptureSource::SystemAudio => {
                let previous_device = app.selected_loopback_device_id.clone();
                let current_name = app
                    .loopback_devices
                    .iter()
                    .find(|device| device.id == app.selected_loopback_device_id)
                    .map(|device| device.name.as_str())
                    .unwrap_or(crate::i18n::tr(
                        app.ui_language,
                        "Default render output (loopback)",
                    ));

                egui::ComboBox::from_id_salt("loopback_device_selector")
                    .selected_text(current_name)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut app.selected_loopback_device_id,
                            String::new(),
                            crate::i18n::tr(app.ui_language, "Default render output (loopback)"),
                        );
                        for device in &app.loopback_devices {
                            ui.selectable_value(
                                &mut app.selected_loopback_device_id,
                                device.id.clone(),
                                &device.name,
                            );
                        }
                    });

                if app.selected_loopback_device_id != previous_device {
                    app.switch_capture_device(previous_device);
                }
            }
        }

        ui.add_space(8.0);
        // Integrated subtle audio meter bar right next to the device selector
        if app.is_translating {
            let bar_width = 80.0;
            let bar_height = 6.0;
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(bar_width, bar_height),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(3),
                egui::Color32::from_rgb(241, 245, 249),
            );
            if animated_fraction > 0.01 {
                let active_rect = egui::Rect::from_min_size(
                    rect.min,
                    egui::vec2(rect.width() * animated_fraction, rect.height()),
                );
                ui.painter().rect_filled(
                    active_rect,
                    egui::CornerRadius::same(3),
                    egui::Color32::from_rgb(37, 99, 235),
                );
            }
        }
    });
}
