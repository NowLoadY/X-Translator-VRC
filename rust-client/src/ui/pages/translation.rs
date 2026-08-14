use crate::ui::components::{self, danger_button, section, status_badge};
use crate::{CaptureSource, LANGUAGE_OPTIONS, language_label};
use eframe::egui;

pub fn render(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(crate::i18n::tr(app.ui_language, "Translation"))
                .size(22.0)
                .color(crate::ui::theme::text_strong())
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (is_active, is_error) = if app.connection_status.to_lowercase().contains("error") {
                (false, true)
            } else {
                (true, false)
            };
            let status = crate::i18n::tr_dynamic(app.ui_language, &app.connection_status);
            status_badge(ui, status.as_ref(), is_active, is_error);
        });
    });

    if let Some(error) = &app.last_error {
        let error_summary = error.lines().next().unwrap_or(error);
        ui.add_space(8.0);
        components::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("!")
                        .color(egui::Color32::from_rgb(220, 38, 38))
                        .strong(),
                );
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
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(error_summary)
                            .color(egui::Color32::from_rgb(220, 38, 38)),
                    )
                    .truncate(),
                );
            });
        });
    }

    ui.add_space(14.0);

    section(ui, crate::i18n::tr(app.ui_language, "Voice Route"), |ui| {
        let previous_source = app.source_lang.clone();
        let previous_target = app.target_lang.clone();

        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr(app.ui_language, "Input:"))
                    .color(crate::ui::theme::text_strong())
                    .strong(),
            );
            let mut source_options = vec![(
                "auto".to_string(),
                crate::i18n::tr(app.ui_language, "Auto (bidirectional)").to_string(),
            )];
            for (code, label) in LANGUAGE_OPTIONS {
                source_options.push((
                    (*code).to_string(),
                    crate::i18n::tr(app.ui_language, label).to_string(),
                ));
            }
            if components::searchable_combobox(
                ui,
                "source_language",
                language_label(app.ui_language, &app.source_lang),
                &mut app.source_lang,
                &source_options,
            ) && app.source_lang != "auto"
                && app.target_lang == app.source_lang
            {
                app.target_lang = if app.source_lang == "zh" {
                    "en".to_string()
                } else {
                    "zh".to_string()
                };
            }

            if app.source_lang == "auto" {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(crate::i18n::tr(app.ui_language, "Pair:"))
                        .color(crate::ui::theme::text_strong())
                        .strong(),
                );
                components::target_language_pair_selector(
                    ui,
                    "translation_page",
                    &app.source_lang,
                    &mut app.target_lang,
                    app.ui_language,
                    |code, lang| language_label(lang, code).to_string(),
                );
            } else {
                ui.add_space(4.0);
                if components::swap_capsule_button(ui, true).clicked() {
                    let temp = app.source_lang.clone();
                    app.source_lang = app.target_lang.clone();
                    app.target_lang = temp;
                    app.apply_language_route();
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(crate::i18n::tr(app.ui_language, "Target:"))
                        .color(crate::ui::theme::text_strong())
                        .strong(),
                );
                components::target_language_pair_selector(
                    ui,
                    "translation_page",
                    &app.source_lang,
                    &mut app.target_lang,
                    app.ui_language,
                    |code, lang| language_label(lang, code).to_string(),
                );
            }
        });

        if app.source_lang != previous_source || app.target_lang != previous_target {
            app.apply_language_route();
        }
    });

    ui.add_space(10.0);

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
                    CaptureSource::Both => crate::i18n::tr(app.ui_language, "Both"),
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
                    ui.selectable_value(
                        &mut app.capture_source,
                        CaptureSource::Both,
                        crate::i18n::tr(app.ui_language, "Both"),
                    );
                });
            if app.capture_source != previous_source {
                app.switch_capture_source(previous_source);
            }

            ui.add_space(6.0);
            if components::animated_button_enabled(
                ui,
                crate::i18n::tr(app.ui_language, "Refresh"),
                app.device_refresh_rx.is_none(),
            )
            .clicked()
            {
                app.request_audio_device_refresh(ui.ctx().clone());
            }
        });

        ui.add_space(8.0);
        render_capture_device_selector(app, ui);

        ui.add_space(10.0);
        if app.capture_source == CaptureSource::Both {
            let avail_w = ui.available_width();
            if avail_w < 620.0 {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(248, 250, 252))
                    .corner_radius(egui::CornerRadius::same(14))
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(226, 232, 240),
                    ))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        render_input_adaptation(app, ui, CaptureSource::Microphone);
                    });

                ui.add_space(8.0);

                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(248, 250, 252))
                    .corner_radius(egui::CornerRadius::same(14))
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(226, 232, 240),
                    ))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        render_input_adaptation(app, ui, CaptureSource::SystemAudio);
                    });
            } else {
                ui.columns(2, |columns| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(248, 250, 252))
                        .corner_radius(egui::CornerRadius::same(14))
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(226, 232, 240),
                        ))
                        .inner_margin(egui::Margin::same(12))
                        .show(&mut columns[0], |ui| {
                            render_input_adaptation(app, ui, CaptureSource::Microphone);
                        });

                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(248, 250, 252))
                        .corner_radius(egui::CornerRadius::same(14))
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(226, 232, 240),
                        ))
                        .inner_margin(egui::Margin::same(12))
                        .show(&mut columns[1], |ui| {
                            render_input_adaptation(app, ui, CaptureSource::SystemAudio);
                        });
                });
            }
        } else {
            render_input_adaptation(app, ui, app.capture_source);
        }

        ui.add_space(12.0);

        components::action_card(ui, |ui| {
            ui.horizontal(|ui| {
                if app.is_translating {
                    match &app.session_owner {
                        crate::session_coordinator::TranslationSessionOwner::Meeting { .. } => {
                            if components::primary_button(
                                ui,
                                crate::i18n::tr(app.ui_language, "Open meeting controls"),
                            )
                            .clicked()
                            {
                                app.open_meeting_plugin();
                            }
                            ui.label(
                                egui::RichText::new(crate::i18n::tr(
                                    app.ui_language,
                                    "A meeting owns the active audio session",
                                ))
                                .color(crate::ui::theme::text_weak())
                                .size(11.5),
                            );
                        }
                        crate::session_coordinator::TranslationSessionOwner::VideoPlayer { .. } => {
                            if components::primary_button(
                                ui,
                                crate::i18n::tr(app.ui_language, "Open Media Player"),
                            )
                            .clicked()
                            {
                                app.open_video_player_plugin();
                            }
                            ui.label(
                                egui::RichText::new(crate::i18n::tr(
                                    app.ui_language,
                                    "Media Player owns the active translation session",
                                ))
                                .color(crate::ui::theme::text_weak())
                                .size(11.5),
                            );
                        }
                        _ => {
                            if danger_button(
                                ui,
                                crate::i18n::tr(app.ui_language, "Stop Translation"),
                            )
                            .clicked()
                            {
                                app.stop();
                            }
                        }
                    }
                } else {
                    if components::primary_button_enabled(
                        ui,
                        crate::i18n::tr(app.ui_language, "Start Translation"),
                        app.backend_start_deadline.is_none(),
                    )
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
                                config.sample_rate, config.channels, config.sample_format
                            ))
                            .color(crate::ui::theme::text_weak())
                            .size(11.5),
                        );
                    });
                }
            });

            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let mut tts_enabled = app.tts_enabled;
                if components::feature_checkbox(
                    ui,
                    crate::feature_access::Feature::TtsPlayback,
                    app.ui_language,
                    &mut tts_enabled,
                    crate::i18n::tr(app.ui_language, "TTS"),
                )
                .changed()
                {
                    app.set_tts_enabled(tts_enabled);
                }

                ui.add_space(12.0);
                let mut floating_enabled = app.floating_subtitles_enabled;
                if components::feature_checkbox(
                    ui,
                    crate::feature_access::Feature::FloatingSubtitles,
                    app.ui_language,
                    &mut floating_enabled,
                    crate::i18n::tr(app.ui_language, "Floating subtitles"),
                )
                .changed()
                {
                    app.set_floating_subtitles_enabled(floating_enabled);
                }
            });
        });
    });

    ui.add_space(10.0);

    ui.columns(2, |columns| {
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
                                egui::RichText::new(crate::i18n::tr(app.ui_language, "No speech"))
                                    .color(crate::ui::theme::text_weak())
                                    .italics(),
                            );
                        } else {
                            let total = app.recognition_history.len();
                            for (i, entry) in app.recognition_history.iter().enumerate() {
                                let is_last = i == total - 1 && app.partial_text.is_empty();
                                let resp = components::history_entry_card(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    if let Some(speaker) =
                                        crate::compact_speaker_label(&entry.speaker_id)
                                    {
                                        ui.horizontal(|ui| {
                                            components::speaker_badge(ui, &speaker);
                                        });
                                        ui.add_space(2.0);
                                    }
                                    render_text_with_term_matches(
                                        ui,
                                        &entry.text,
                                        &entry.activation_matches,
                                        &entry.context_matches,
                                        crate::ui::theme::text_normal(),
                                        false,
                                    );
                                });
                                if is_last {
                                    resp.scroll_to_me(Some(egui::Align::BOTTOM));
                                }
                                ui.add_space(4.0);
                            }
                            if !app.partial_text.is_empty() {
                                let resp = ui
                                    .horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("• • •")
                                                .color(egui::Color32::from_rgb(59, 130, 246))
                                                .size(11.0)
                                                .strong(),
                                        );
                                        ui.add_space(2.0);
                                        ui.label(
                                            egui::RichText::new(&app.partial_text)
                                                .color(egui::Color32::from_rgb(37, 99, 235))
                                                .size(13.0)
                                                .italics(),
                                        )
                                    })
                                    .response;
                                resp.scroll_to_me(Some(egui::Align::BOTTOM));
                            }
                        }
                    });
            },
        );

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
                                    "No translations",
                                ))
                                .color(crate::ui::theme::text_weak())
                                .italics(),
                            );
                        } else {
                            let total = app.translations.len();
                            for (i, entry) in app.translations.iter().enumerate() {
                                let is_last = i == total - 1;
                                let group_resp = components::history_entry_card(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    if let Some(speaker) =
                                        crate::compact_speaker_label(&entry.speaker_id)
                                    {
                                        ui.horizontal(|ui| {
                                            components::speaker_badge(ui, &speaker);
                                        });
                                        ui.add_space(2.0);
                                    }
                                    if !entry.source.is_empty() {
                                        ui.label(
                                            egui::RichText::new(&entry.source)
                                                .color(crate::ui::theme::text_weak())
                                                .size(11.5),
                                        );
                                        ui.add_space(2.0);
                                    }
                                    render_text_with_term_matches(
                                        ui,
                                        &entry.translated,
                                        &entry.term_matches,
                                        &[],
                                        crate::ui::theme::text_strong(),
                                        true,
                                    );
                                });

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

fn render_text_with_term_matches(
    ui: &mut egui::Ui,
    text: &str,
    primary_matches: &[xrtranslate_protocol::CorpusTermMatch],
    secondary_matches: &[xrtranslate_protocol::CorpusTermMatch],
    base_color: egui::Color32,
    strong: bool,
) -> egui::Response {
    let mut matches = secondary_matches
        .iter()
        .map(|term_match| (term_match, false))
        .chain(primary_matches.iter().map(|term_match| (term_match, true)))
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        left.0
            .start_byte
            .cmp(&right.0.start_byte)
            // Prefer activations when spans coincide.
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.0.end_byte.cmp(&left.0.end_byte))
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mut cursor = 0usize;
        for (term_match, primary) in matches {
            let (Ok(start), Ok(end)) = (
                usize::try_from(term_match.start_byte),
                usize::try_from(term_match.end_byte),
            ) else {
                continue;
            };
            if start < cursor
                || end <= start
                || end > text.len()
                || !text.is_char_boundary(start)
                || !text.is_char_boundary(end)
                || text.get(start..end) != Some(term_match.text.as_str())
            {
                continue;
            }
            if cursor < start {
                let mut text = egui::RichText::new(&text[cursor..start])
                    .color(base_color)
                    .size(13.0);
                if strong {
                    text = text.strong();
                }
                ui.label(text);
            }
            let tooltip = term_match
                .sources
                .iter()
                .map(|source| {
                    format!(
                        "{}\n{} / {}\n{}",
                        source.title, source.domain, source.subdomain, source.corpus_id
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            let mut highlighted = egui::RichText::new(&text[start..end])
                .color(if primary {
                    egui::Color32::from_rgb(37, 99, 235)
                } else {
                    egui::Color32::from_rgb(96, 165, 250)
                })
                .size(13.0);
            if primary {
                highlighted = highlighted.strong();
            }
            ui.label(highlighted).on_hover_text(tooltip);
            cursor = end;
        }
        if cursor < text.len() {
            let mut trailing = egui::RichText::new(&text[cursor..])
                .color(base_color)
                .size(13.0);
            if strong {
                trailing = trailing.strong();
            }
            ui.label(trailing);
        }
    })
    .response
}

fn render_input_adaptation(
    app: &mut crate::XRTranslateApp,
    ui: &mut egui::Ui,
    source: CaptureSource,
) {
    if app.capture_source == CaptureSource::Both {
        let title = match source {
            CaptureSource::Microphone => crate::i18n::tr(app.ui_language, "Microphone").to_string(),
            CaptureSource::SystemAudio => {
                crate::i18n::tr(app.ui_language, "System Audio (WASAPI)").to_string()
            }
            CaptureSource::Both => unreachable!(),
        };
        ui.label(
            egui::RichText::new(title)
                .color(crate::ui::theme::text_strong())
                .size(13.5)
                .strong(),
        );
        ui.add_space(4.0);
    }
    let language = app.ui_language;
    let recognize_when = crate::i18n::tr(language, "Recognize when:");
    let speak = crate::i18n::tr(language, "Speak");
    let always = crate::i18n::tr(language, "Always");
    let vad_sensitivity = crate::i18n::tr(language, "VAD Sensitivity");
    let pause_tolerance = crate::i18n::tr(language, "Pause tolerance");
    let changed = {
        let recognition = app.recognition_settings_mut(source);
        let timing_changed = ui
            .horizontal(|ui| {
                ui.label(egui::RichText::new(recognize_when).strong());
                let previous = recognition.continuous_recognition;
                egui::ComboBox::from_id_salt(("recognition_timing", source))
                    .selected_text(if recognition.continuous_recognition {
                        always
                    } else {
                        speak
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut recognition.continuous_recognition, false, speak);
                        ui.selectable_value(&mut recognition.continuous_recognition, true, always);
                    });
                recognition.continuous_recognition != previous
            })
            .inner;
        let background_response = components::modern_slider_f32(
            ui,
            &mut recognition.background_noise,
            0.05..=0.8,
            0.2,
            vad_sensitivity,
            &[],
        );
        let background_changed = background_response.drag_stopped()
            || (background_response.changed() && !background_response.dragged());
        let pause_changed = if recognition.continuous_recognition {
            false
        } else {
            let response = components::modern_slider_f32(
                ui,
                &mut recognition.pause_tolerance,
                0.0..=1.0,
                0.0,
                pause_tolerance,
                &[],
            );
            response.drag_stopped() || (response.changed() && !response.dragged())
        };
        timing_changed || background_changed || pause_changed
    };
    if changed {
        app.set_audio_adaptation(source);
    }
}

fn render_audio_level(
    ui: &mut egui::Ui,
    level: &std::sync::Arc<std::sync::atomic::AtomicU32>,
    vad_active: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    visible: bool,
    updating: bool,
) {
    let level = f32::from_bits(level.load(std::sync::atomic::Ordering::Relaxed)).clamp(0.0, 1.0);
    let decibels = 20.0 * level.max(0.000_001).log10();
    let raw_fraction = ((decibels + 60.0) / 60.0).clamp(0.0, 1.0);
    let active = vad_active.load(std::sync::atomic::Ordering::Relaxed);

    components::segmented_audio_meter(ui, raw_fraction, active, visible, updating);
}

fn render_capture_device_selector(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
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

                let mut mic_options = vec![(
                    String::new(),
                    crate::i18n::tr(app.ui_language, "Default microphone").to_string(),
                )];
                for device in &app.devices {
                    mic_options.push((device.id.clone(), device.name.clone()));
                }

                if components::searchable_combobox(
                    ui,
                    "mic_device_selector",
                    current_name,
                    &mut app.selected_device_id,
                    &mic_options,
                ) {
                    app.switch_capture_device(CaptureSource::Microphone, previous_device);
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

                let mut loopback_options = vec![(
                    String::new(),
                    crate::i18n::tr(app.ui_language, "Default render output (loopback)")
                        .to_string(),
                )];
                for device in &app.loopback_devices {
                    loopback_options.push((device.id.clone(), device.name.clone()));
                }

                if components::searchable_combobox(
                    ui,
                    "loopback_device_selector",
                    current_name,
                    &mut app.selected_loopback_device_id,
                    &loopback_options,
                ) {
                    app.switch_capture_device(CaptureSource::SystemAudio, previous_device);
                }
            }
            CaptureSource::Both => {
                let previous_device = app.selected_device_id.clone();
                let current_name = app
                    .devices
                    .iter()
                    .find(|device| device.id == app.selected_device_id)
                    .map(|device| device.name.as_str())
                    .unwrap_or(crate::i18n::tr(app.ui_language, "Default microphone"));
                let mut mic_options = vec![(
                    String::new(),
                    crate::i18n::tr(app.ui_language, "Default microphone").to_string(),
                )];
                for device in &app.devices {
                    mic_options.push((device.id.clone(), device.name.clone()));
                }

                if components::searchable_combobox(
                    ui,
                    "both_mic_device_selector",
                    current_name,
                    &mut app.selected_device_id,
                    &mic_options,
                ) {
                    app.switch_capture_device(CaptureSource::Microphone, previous_device);
                }
            }
        }
        ui.add_space(8.0);
        let (level, vad_active) = match app.capture_source {
            CaptureSource::Microphone | CaptureSource::Both => {
                (&app.input_level, &app.microphone_vad_active)
            }
            CaptureSource::SystemAudio => (&app.loopback_level, &app.loopback_vad_active),
        };
        render_audio_level(ui, level, vad_active, true, app.is_translating);
    });

    if app.capture_source == CaptureSource::Both {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr(app.ui_language, "System Audio (WASAPI)"))
                    .color(crate::ui::theme::text_strong())
                    .strong(),
            );
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
            let mut loopback_options = vec![(
                String::new(),
                crate::i18n::tr(app.ui_language, "Default render output (loopback)").to_string(),
            )];
            for device in &app.loopback_devices {
                loopback_options.push((device.id.clone(), device.name.clone()));
            }

            if components::searchable_combobox(
                ui,
                "both_loopback_device_selector",
                current_name,
                &mut app.selected_loopback_device_id,
                &loopback_options,
            ) {
                app.switch_capture_device(CaptureSource::SystemAudio, previous_device);
            }
            ui.add_space(8.0);
            render_audio_level(
                ui,
                &app.loopback_level,
                &app.loopback_vad_active,
                true,
                app.is_translating,
            );
        });
    }
}
