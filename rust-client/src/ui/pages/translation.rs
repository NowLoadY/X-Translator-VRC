use crate::ui::components::{self, danger_button, section, status_badge};
use crate::{CaptureSource, LANGUAGE_OPTIONS, language_label};
use eframe::egui;
use std::hash::{Hash, Hasher};

const HISTORY_ROW_HEIGHT: f32 = 88.0;
const HISTORY_ROW_GAP: f32 = 8.0;

fn history_content_height(row_count: usize) -> f32 {
    if row_count == 0 {
        return 0.0;
    }
    row_count as f32 * HISTORY_ROW_HEIGHT + row_count.saturating_sub(1) as f32 * HISTORY_ROW_GAP
}

fn history_bottom_scroll_offset(row_count: usize, viewport_height: f32) -> f32 {
    (history_content_height(row_count) - viewport_height).max(0.0)
}

fn visible_history_range(viewport: egui::Rect, row_count: usize) -> std::ops::Range<usize> {
    let row_extent = HISTORY_ROW_HEIGHT + HISTORY_ROW_GAP;
    let first = ((viewport.min.y / row_extent).floor() as isize - 1).max(0) as usize;
    let end = ((viewport.max.y / row_extent).ceil() as isize + 1).max(0) as usize;
    first.min(row_count)..end.min(row_count)
}

fn show_virtual_history_rows(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    row_count: usize,
    scroll_to_end: bool,
    mut render_row: impl FnMut(&mut egui::Ui, usize),
) {
    let viewport_height_id = ui.make_persistent_id((id_salt, "viewport_height"));
    let viewport_height = ui
        .memory(|memory| memory.data.get_temp::<f32>(viewport_height_id))
        .unwrap_or_else(|| ui.available_height())
        .max(0.0);
    let mut scroll_area = egui::ScrollArea::vertical()
        .id_salt(id_salt)
        .animated(false)
        .auto_shrink([false, false]);
    if scroll_to_end {
        scroll_area = scroll_area
            .vertical_scroll_offset(history_bottom_scroll_offset(row_count, viewport_height));
    }

    let output = scroll_area.show_viewport(ui, |ui, viewport| {
        let content_top = ui.max_rect().top();
        let content_left = ui.max_rect().left();
        let content_width = ui.available_width();
        let row_extent = HISTORY_ROW_HEIGHT + HISTORY_ROW_GAP;
        ui.set_height(history_content_height(row_count));

        for index in visible_history_range(viewport, row_count) {
            let row_top = content_top + index as f32 * row_extent;
            let row_rect = egui::Rect::from_min_size(
                egui::pos2(content_left, row_top),
                egui::vec2(content_width, HISTORY_ROW_HEIGHT),
            );
            ui.scope_builder(
                egui::UiBuilder::new()
                    .id_salt((id_salt, index))
                    .max_rect(row_rect),
                |ui| {
                    render_row(ui, index);
                },
            );
        }
    });
    ui.memory_mut(|memory| {
        memory
            .data
            .insert_temp(viewport_height_id, output.inner_rect.height());
    });
}

fn recognition_history_fingerprint(
    entries: &[crate::history::RecognitionHistoryEntry],
    partial_text: &str,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    entries.len().hash(&mut hasher);
    if let Some(first) = entries.first() {
        first.stream_id.hash(&mut hasher);
        first.turn_id.hash(&mut hasher);
        first.text.hash(&mut hasher);
    }
    if let Some(last) = entries.last() {
        last.stream_id.hash(&mut hasher);
        last.turn_id.hash(&mut hasher);
        last.speaker_id.hash(&mut hasher);
        last.text.hash(&mut hasher);
    }
    partial_text.hash(&mut hasher);
    hasher.finish()
}

fn translation_history_fingerprint(entries: &[crate::history::TranslationHistoryEntry]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    entries.len().hash(&mut hasher);
    if let Some(first) = entries.first() {
        first.stream_id.hash(&mut hasher);
        first.turn_id.hash(&mut hasher);
        first.segment_index.hash(&mut hasher);
        first.source.hash(&mut hasher);
        first.translated.hash(&mut hasher);
    }
    if let Some(last) = entries.last() {
        last.stream_id.hash(&mut hasher);
        last.turn_id.hash(&mut hasher);
        last.segment_index.hash(&mut hasher);
        last.speaker_id.hash(&mut hasher);
        last.source.hash(&mut hasher);
        last.translated.hash(&mut hasher);
    }
    hasher.finish()
}

fn fixed_height_history_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    components::history_entry_card(ui, |ui| {
        // Keep every virtual row's layout height identical to the height used
        // by the scroll-position math. Text stays on a single-line preview,
        // just like the VideoPlayer subtitle rows.
        ui.set_width(ui.available_width());
        ui.set_height(HISTORY_ROW_HEIGHT - 18.0);
        add_contents(ui);
    });
}

#[cfg(test)]
mod virtual_history_tests {
    use super::*;

    #[test]
    fn content_height_has_no_trailing_gap() {
        assert_eq!(history_content_height(0), 0.0);
        assert_eq!(history_content_height(1), HISTORY_ROW_HEIGHT);
        assert_eq!(
            history_content_height(3),
            3.0 * HISTORY_ROW_HEIGHT + 2.0 * HISTORY_ROW_GAP
        );
    }

    #[test]
    fn last_row_ends_at_the_virtual_content_bottom() {
        let row_count = 100;
        let last_row_top = (row_count - 1) as f32 * (HISTORY_ROW_HEIGHT + HISTORY_ROW_GAP);
        assert_eq!(
            last_row_top + HISTORY_ROW_HEIGHT,
            history_content_height(row_count)
        );
        assert_eq!(
            history_bottom_scroll_offset(row_count, 180.0) + 180.0,
            history_content_height(row_count)
        );
        assert_eq!(history_bottom_scroll_offset(1, 180.0), 0.0);
    }

    #[test]
    fn visible_range_is_bounded_near_the_end() {
        let row_count = 100;
        let content_height = history_content_height(row_count);
        let viewport = egui::Rect::from_min_max(
            egui::pos2(0.0, content_height - 180.0),
            egui::pos2(500.0, content_height),
        );
        let range = visible_history_range(viewport, row_count);

        assert!(range.start < range.end);
        assert_eq!(range.end, row_count);
        assert!(range.len() <= 5);
    }
}

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
                    CaptureSource::SystemAudio => crate::i18n::tr(app.ui_language, "System Audio"),
                    CaptureSource::Both => crate::i18n::tr(app.ui_language, "Both"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.capture_source,
                        CaptureSource::Microphone,
                        crate::i18n::tr(app.ui_language, "Microphone"),
                    );
                    ui.add_enabled_ui(!app.loopback_devices.is_empty(), |ui| {
                        ui.selectable_value(
                            &mut app.capture_source,
                            CaptureSource::SystemAudio,
                            crate::i18n::tr(app.ui_language, "System Audio"),
                        );
                        ui.selectable_value(
                            &mut app.capture_source,
                            CaptureSource::Both,
                            crate::i18n::tr(app.ui_language, "Both"),
                        );
                    });
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
        if app.capture_source == CaptureSource::Both && !app.loopback_devices.is_empty() {
            let avail_w = ui.available_width();
            if avail_w < 620.0 {
                egui::Frame::new()
                    .fill(egui::Color32::TRANSPARENT)
                    .corner_radius(egui::CornerRadius::same(8))
                    .stroke(egui::Stroke::new(1.0, crate::ui::theme::border()))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        render_input_adaptation(app, ui, CaptureSource::Microphone);
                    });

                ui.add_space(8.0);

                egui::Frame::new()
                    .fill(egui::Color32::TRANSPARENT)
                    .corner_radius(egui::CornerRadius::same(8))
                    .stroke(egui::Stroke::new(1.0, crate::ui::theme::border()))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        render_input_adaptation(app, ui, CaptureSource::SystemAudio);
                    });
            } else {
                ui.columns(2, |columns| {
                    egui::Frame::new()
                        .fill(egui::Color32::TRANSPARENT)
                        .corner_radius(egui::CornerRadius::same(8))
                        .stroke(egui::Stroke::new(1.0, crate::ui::theme::border()))
                        .inner_margin(egui::Margin::same(12))
                        .show(&mut columns[0], |ui| {
                            render_input_adaptation(app, ui, CaptureSource::Microphone);
                        });

                    egui::Frame::new()
                        .fill(egui::Color32::TRANSPARENT)
                        .corner_radius(egui::CornerRadius::same(8))
                        .stroke(egui::Stroke::new(1.0, crate::ui::theme::border()))
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
                        crate::session_coordinator::TranslationSessionOwner::Plugin(owner) => {
                            let plugin_id = crate::plugins::PluginId::parse(owner.plugin_id());
                            let open_label = owner.open_label(app.ui_language);
                            let active_message = owner.active_message(app.ui_language);
                            let open_clicked = components::primary_button(ui, open_label).clicked();
                            ui.label(
                                egui::RichText::new(active_message)
                                    .color(crate::ui::theme::text_weak())
                                    .size(11.5),
                            );
                            if open_clicked && let Some(plugin_id) = plugin_id {
                                app.open_plugin(plugin_id);
                            }
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
                    if components::animated_button_enabled(
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
                let tts_configured = app.service_config.tts_is_configured();
                let mut tts_enabled = app.tts_enabled;
                let tts_response = ui.add_enabled_ui(tts_configured, |ui| {
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
                });
                if !tts_configured {
                    tts_response.response.on_disabled_hover_text(crate::i18n::tr(
                        app.ui_language,
                        "Configure a TTS provider in Settings to enable TTS playback.",
                    ));
                }

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(crate::i18n::tr(app.ui_language, "TTS output:"))
                        .color(crate::ui::theme::text_normal()),
                );
                let current_output = app
                    .tts_output_devices
                    .iter()
                    .find(|device| device.id == app.selected_tts_output_device_id)
                    .map(|device| device.name.clone())
                    .unwrap_or_else(|| crate::i18n::tr(app.ui_language, "Default speaker").into());
                let mut output_options = vec![(
                    String::new(),
                    crate::i18n::tr(app.ui_language, "Default speaker").to_owned(),
                )];
                output_options.extend(
                    app.tts_output_devices
                        .iter()
                        .map(|device| (device.id.clone(), device.name.clone())),
                );
                let output_selector = ui.add_enabled_ui(tts_configured && !app.is_translating, |ui| {
                    if components::searchable_combobox(
                        ui,
                        "tts_output_device_selector",
                        &current_output,
                        &mut app.selected_tts_output_device_id,
                        &output_options,
                    ) {
                        app.audio_system.clear_tts_playback();
                        app.save_settings();
                    }
                });
                if app.is_translating {
                    output_selector.response.on_hover_text(crate::i18n::tr(
                        app.ui_language,
                        "Stop translation to change the TTS output device.",
                    ));
                }

                for source in app.capture_source.routes() {
                    ui.add_space(8.0);
                    let status = app.voice_clone_state(*source).cloned();
                    let busy = status.as_ref().is_some_and(|status| {
                        matches!(
                            status.state,
                            xrtranslate_protocol::VoiceClonePhase::Collecting
                                | xrtranslate_protocol::VoiceClonePhase::Registering
                        )
                    });
                    let label = match status.as_ref().map(|status| status.state) {
                        Some(xrtranslate_protocol::VoiceClonePhase::Collecting) => status
                            .as_ref()
                            .map(|status| {
                                format!(
                                    "{} {:.1}/{:.1}s",
                                    crate::i18n::tr(app.ui_language, "Collecting voice…"),
                                    status.collected_seconds,
                                    status.required_seconds
                                )
                            })
                            .unwrap(),
                        Some(xrtranslate_protocol::VoiceClonePhase::Registering) => {
                            crate::i18n::tr(app.ui_language, "Creating voice…").into()
                        }
                        _ => match source {
                            CaptureSource::Microphone => {
                                crate::i18n::tr(app.ui_language, "Clone microphone voice").into()
                            }
                            CaptureSource::SystemAudio => {
                                crate::i18n::tr(app.ui_language, "Clone system voice").into()
                            }
                            CaptureSource::Both => unreachable!(),
                        },
                    };
                    let response = components::animated_button_enabled(
                        ui,
                        &label,
                        app.is_translating && !busy && tts_configured,
                    );
                    let clicked = response.clicked();
                    if let Some(message) =
                        status.as_ref().and_then(|status| status.message.as_deref())
                    {
                        response.on_hover_text(message);
                    } else if !tts_configured {
                        response.on_disabled_hover_text(crate::i18n::tr(
                            app.ui_language,
                            "Configure a TTS provider in Settings to enable voice cloning.",
                        ));
                    }
                    if clicked {
                        app.begin_voice_clone(*source);
                    }
                    if status.as_ref().is_some_and(|status| {
                        status.state == xrtranslate_protocol::VoiceClonePhase::Ready
                    }) {
                        ui.label(
                            egui::RichText::new("✓").color(egui::Color32::from_rgb(5, 150, 105)),
                        );
                    } else if let Some(message) = status.as_ref().and_then(|status| {
                        (status.state == xrtranslate_protocol::VoiceClonePhase::Failed)
                            .then_some(status.message.as_deref())
                            .flatten()
                    }) {
                        ui.label(
                            egui::RichText::new(crate::i18n::tr(
                                app.ui_language,
                                "Voice cloning failed",
                            ))
                            .color(egui::Color32::from_rgb(220, 38, 38)),
                        )
                        .on_hover_text(message);
                    }
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
                let scroll_state_id = ui.make_persistent_id("recognition_history_scroll_state");
                let previous_fingerprint = ui.memory(|memory| {
                    memory
                        .data
                        .get_temp::<u64>(scroll_state_id)
                        .unwrap_or_default()
                });
                let current_fingerprint =
                    recognition_history_fingerprint(&app.recognition_history, &app.partial_text);
                let has_partial = !app.partial_text.is_empty();
                let row_count = app.recognition_history.len() + usize::from(has_partial);
                let should_scroll = row_count > 0 && current_fingerprint != previous_fingerprint;

                egui::Frame::new()
                    // Paint a stable, low-alpha layer across the whole viewport. Without a
                    // content shape in the empty area, a transparent WGPU surface can expose
                    // an older compositor tile while the live history is changing size.
                    .fill(crate::ui::theme::history_viewport())
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(4, 4))
                    .show(ui, |ui| {
                        ui.set_min_height((history_height - 8.0).max(0.0));
                        if row_count == 0 {
                            ui.label(
                                egui::RichText::new(crate::i18n::tr(app.ui_language, "No speech"))
                                    .color(crate::ui::theme::text_weak())
                                    .italics(),
                            );
                        } else {
                            show_virtual_history_rows(
                                ui,
                                "recognition_history_scroll",
                                row_count,
                                should_scroll,
                                |ui, index| {
                                    if let Some(entry) = app.recognition_history.get(index) {
                                        fixed_height_history_card(ui, |ui| {
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
                                    } else {
                                        fixed_height_history_card(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("• • •")
                                                        .color(crate::ui::theme::primary())
                                                        .size(11.0)
                                                        .strong(),
                                                );
                                                ui.add_space(2.0);
                                                ui.label(
                                                    egui::RichText::new(&app.partial_text)
                                                        .color(crate::ui::theme::primary_dark())
                                                        .size(13.0)
                                                        .italics(),
                                                )
                                            });
                                        });
                                    }
                                },
                            );
                        }
                    });
                ui.memory_mut(|memory| {
                    memory
                        .data
                        .insert_temp(scroll_state_id, current_fingerprint);
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
                let scroll_state_id = ui.make_persistent_id("translation_history_scroll_state");
                let previous_fingerprint = ui
                    .memory(|memory| memory.data.get_temp::<u64>(scroll_state_id))
                    .unwrap_or_default();
                let current_fingerprint = translation_history_fingerprint(&app.translations);
                let row_count = app.translations.len();
                let should_scroll = row_count > 0 && current_fingerprint != previous_fingerprint;

                egui::Frame::new()
                    .fill(crate::ui::theme::history_viewport())
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(4, 4))
                    .show(ui, |ui| {
                        ui.set_min_height((history_height - 8.0).max(0.0));
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
                            show_virtual_history_rows(
                                ui,
                                "translation_history_scroll",
                                row_count,
                                should_scroll,
                                |ui, index| {
                                    let entry = &app.translations[index];
                                    fixed_height_history_card(ui, |ui| {
                                        if let Some(speaker) =
                                            crate::compact_speaker_label(&entry.speaker_id)
                                        {
                                            ui.horizontal(|ui| {
                                                components::speaker_badge(ui, &speaker);
                                            });
                                            ui.add_space(2.0);
                                        }
                                        if !entry.source.is_empty() {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&entry.source)
                                                        .color(crate::ui::theme::text_weak())
                                                        .size(11.5),
                                                )
                                                .truncate(),
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
                                },
                            );
                        }
                    });
                ui.memory_mut(|memory| {
                    memory
                        .data
                        .insert_temp(scroll_state_id, current_fingerprint);
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
    // Virtual history rows have a fixed height, so the preview must stay on
    // one line. The row clip handles horizontal overflow while preserving the
    // individual terminology hover targets.
    ui.horizontal(|ui| {
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
                    crate::ui::theme::primary_dark()
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
                crate::i18n::tr(app.ui_language, "System Audio").to_string()
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
                ui.label(
                    egui::RichText::new(recognize_when)
                        .color(crate::ui::theme::text_strong())
                        .strong(),
                );
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
            0.30,
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
                0.10,
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
    id_source: &'static str,
    level: &std::sync::Arc<std::sync::atomic::AtomicU32>,
    vad_active: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    visible: bool,
    updating: bool,
) {
    let level = f32::from_bits(level.load(std::sync::atomic::Ordering::Relaxed)).clamp(0.0, 1.0);
    let decibels = 20.0 * level.max(0.000_001).log10();
    let raw_fraction = ((decibels + 60.0) / 60.0).clamp(0.0, 1.0);
    let active = vad_active.load(std::sync::atomic::Ordering::Relaxed);

    components::segmented_audio_meter(ui, id_source, raw_fraction, active, visible, updating);
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
                if app.loopback_devices.is_empty() {
                    ui.label(crate::i18n::tr(
                        app.ui_language,
                        "System audio capture is unavailable on this host",
                    ));
                    return;
                }
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
        let (id_source, level, vad_active) = match app.capture_source {
            CaptureSource::Microphone | CaptureSource::Both => {
                ("microphone", &app.input_level, &app.microphone_vad_active)
            }
            CaptureSource::SystemAudio => (
                "system_audio",
                &app.loopback_level,
                &app.loopback_vad_active,
            ),
        };
        render_audio_level(ui, id_source, level, vad_active, true, app.is_translating);
    });

    if app.capture_source == CaptureSource::Both && !app.loopback_devices.is_empty() {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr(app.ui_language, "System Audio"))
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
                "system_audio",
                &app.loopback_level,
                &app.loopback_vad_active,
                true,
                app.is_translating,
            );
        });
    }
}
