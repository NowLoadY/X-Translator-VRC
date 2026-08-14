use super::{
    backend::PlaybackStatus,
    controller::{VideoPlayerController, VideoPlayerRoute},
    i18n::tr,
    PlayerTranslationRequest, VideoPlayerAction, VideoPlayerPlugin, VideoPlayerUiSnapshot,
    VideoSubtitleMode,
};
use crate::ui::components;
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Stroke};
use std::path::PathBuf;

pub fn render(
    plugin: &mut VideoPlayerPlugin,
    snapshot: &VideoPlayerUiSnapshot,
    ui: &mut egui::Ui,
) -> VideoPlayerAction {
    plugin.controller.tick();
    let language = snapshot.language;

    match plugin.controller.route {
        VideoPlayerRoute::Library => render_library(&mut plugin.controller, language, ui),
        VideoPlayerRoute::Create => render_create(&mut plugin.controller, language, ui),
        VideoPlayerRoute::Player => render_player(&mut plugin.controller, language, ui),
    }
}

fn render_library(
    controller: &mut VideoPlayerController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) -> VideoPlayerAction {
    let mut action = VideoPlayerAction::None;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(tr(language, "Video Tasks"))
                .size(22.0)
                .color(crate::ui::theme::text_strong())
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::primary_button(ui, tr(language, "New Video")).clicked() {
                controller.open_create();
            }
        });
    });

    ui.add_space(14.0);

    if let Some(error) = &controller.error {
        components::danger_alert(ui, error);
        ui.add_space(10.0);
    }

    components::search_bar(ui, &mut controller.search_query, tr(language, "Search videos..."));
    ui.add_space(12.0);

    let search_lower = controller.search_query.trim().to_lowercase();
    let filtered_tasks: Vec<_> = controller
        .store
        .tasks
        .iter()
        .filter(|task| {
            if search_lower.is_empty() {
                true
            } else {
                task.title.to_lowercase().contains(&search_lower)
            }
        })
        .cloned()
        .collect();

    if filtered_tasks.is_empty() {
        components::card(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label(
                    egui::RichText::new(tr(language, "No video tasks yet"))
                        .size(17.0)
                        .strong()
                        .color(crate::ui::theme::text_strong()),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(tr(
                        language,
                        "Create a new video playback and translation task to get started.",
                    ))
                    .size(13.0)
                    .color(crate::ui::theme::text_weak()),
                );
                ui.add_space(18.0);
                if components::primary_button(ui, tr(language, "New Video")).clicked() {
                    controller.open_create();
                }
                ui.add_space(24.0);
            });
        });
        return action;
    }

    let mut task_to_play = None;
    let mut task_to_delete = None;
    let mut srt_to_export = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for task in &filtered_tasks {
                components::card(ui, |ui| {
                    ui.vertical(|ui| {
                        // 1. Header Row (Badge + Title + Metadata)
                        ui.horizontal(|ui| {
                            let (badge_bg, badge_fg, badge_text) = match &task.source {
                                super::backend::MediaSource::LocalFile(_) => (
                                    Color32::from_rgb(238, 242, 255),
                                    Color32::from_rgb(79, 70, 229),
                                    "VIDEO FILE",
                                ),
                                super::backend::MediaSource::NetworkStream(_) => (
                                    Color32::from_rgb(236, 253, 245),
                                    Color32::from_rgb(5, 150, 105),
                                    "STREAM",
                                ),
                            };

                            Frame::new()
                                .fill(badge_bg)
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(Margin::symmetric(6, 3))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(badge_text)
                                            .color(badge_fg)
                                            .strong()
                                            .size(11.0),
                                    );
                                });

                            ui.add_space(8.0);

                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(&task.title)
                                        .size(16.0)
                                        .strong()
                                        .color(crate::ui::theme::text_strong()),
                                );
                                ui.add_space(3.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} • {} {} • {} → {} • {}",
                                        format_time_ms(task.duration_ms),
                                        task.subtitles.count(),
                                        tr(language, "Subtitles Count"),
                                        task.source_language.to_uppercase(),
                                        task.target_language.to_uppercase(),
                                        format_timestamp_date(task.created_at_sec)
                                    ))
                                    .size(12.0)
                                    .color(crate::ui::theme::text_weak()),
                                );
                            });
                        });

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(10.0);

                        // 2. Action Buttons Row (Spacious, well-aligned)
                        ui.horizontal_wrapped(|ui| {
                            if components::primary_button(ui, tr(language, "Play")).clicked() {
                                task_to_play = Some(task.clone());
                            }

                            ui.add_space(6.0);

                            if task.subtitles.count() > 0 {
                                if components::animated_button(ui, tr(language, "Export Subtitles")).clicked() {
                                    srt_to_export = Some(task.subtitles.export_srt());
                                }
                                ui.add_space(6.0);
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if components::danger_button(ui, tr(language, "Delete")).clicked() {
                                    task_to_delete = Some(task.id.clone());
                                }
                            });
                        });
                    });
                });
                ui.add_space(10.0);
            }
        });

    if let Some(task) = task_to_play {
        if let Ok(_) = controller.play_task(&task.id) {
            if task.subtitle_mode == VideoSubtitleMode::RealtimeTranslation {
                match &task.source {
                    super::backend::MediaSource::LocalFile(path) => {
                        action = VideoPlayerAction::StartTranslation(
                            PlayerTranslationRequest::ImportMediaFile {
                                path: path.clone(),
                                source_language: task.source_language,
                                target_language: task.target_language,
                                recognition: task.recognition.clone(),
                            },
                        );
                    }
                    super::backend::MediaSource::NetworkStream(_) => {
                        action = VideoPlayerAction::StartTranslation(
                            PlayerTranslationRequest::LiveStream {
                                source_language: task.source_language,
                                target_language: task.target_language,
                                recognition: task.recognition.clone(),
                            },
                        );
                    }
                }
            } else {
                action = VideoPlayerAction::StopTranslation;
            }
        }
    }

    if let Some(id) = task_to_delete {
        let was_active = controller.active_task_id.as_deref() == Some(&id);
        controller.delete_task(&id);
        if was_active {
            action = VideoPlayerAction::StopTranslation;
        }
    }

    if let Some(srt) = srt_to_export {
        if let Some(save_path) = rfd::FileDialog::new()
            .set_file_name("subtitles.srt")
            .add_filter("Subtitles", &["srt"])
            .save_file()
        {
            let _ = std::fs::write(save_path, srt);
        }
    }

    action
}

fn render_create(
    controller: &mut VideoPlayerController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) -> VideoPlayerAction {
    let mut action = VideoPlayerAction::None;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(tr(language, "New Video Task"))
                .size(22.0)
                .color(crate::ui::theme::text_strong())
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::animated_button(ui, tr(language, "Back to Library")).clicked() {
                controller.open_library();
            }
        });
    });

    ui.add_space(14.0);

    if let Some(error) = &controller.error {
        components::danger_alert(ui, error);
        ui.add_space(10.0);
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            components::card(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(tr(language, "Video Source"))
                            .size(15.0)
                            .strong()
                            .color(crate::ui::theme::text_strong()),
                    );
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        let text_width = (ui.available_width() - 95.0).max(100.0);
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut controller.draft_source)
                                .hint_text(tr(language, "Enter stream URL or choose local file..."))
                                .desired_width(text_width),
                        );

                        if response.changed() && controller.draft_title.trim().is_empty() {
                            let text = controller.draft_source.trim();
                            if !text.starts_with("http://")
                                && !text.starts_with("https://")
                                && !text.starts_with("rtsp://")
                                && !text.starts_with("rtmp://")
                            {
                                if let Some(filename) = std::path::Path::new(text).file_name() {
                                    controller.draft_title = filename.to_string_lossy().to_string();
                                }
                            }
                        }

                        if components::primary_button(ui, tr(language, "Browse...")).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Video Files", &["mp4", "mkv", "webm", "avi", "mov", "flv", "ts", "m4v"])
                                .add_filter("Audio Files", &["mp3", "wav", "flac", "aac", "ogg", "m4a"])
                                .pick_file()
                            {
                                let file_title = path
                                    .file_name()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                if controller.draft_title.trim().is_empty() {
                                    controller.draft_title = file_title;
                                }
                                controller.draft_source = path.to_string_lossy().to_string();
                            }
                        }
                    });

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(16.0);

                    ui.label(
                        egui::RichText::new(tr(language, "Task Title (Optional)"))
                            .size(15.0)
                            .strong()
                            .color(crate::ui::theme::text_strong()),
                    );
                    ui.add_space(8.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut controller.draft_title)
                            .hint_text(tr(language, "Leave empty to use file name"))
                            .desired_width(ui.available_width()),
                    );

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(16.0);

                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(tr(language, "Spoken language"))
                                    .strong()
                                    .color(crate::ui::theme::text_strong()),
                            );
                            ui.add_space(4.0);

                            let mut source_options = vec![(
                                "auto".to_string(),
                                tr(language, "Auto (bidirectional)").to_string(),
                            )];
                            for (code, label) in crate::LANGUAGE_OPTIONS {
                                source_options.push((
                                    (*code).to_string(),
                                    tr(language, label).to_string(),
                                ));
                            }

                            if components::searchable_combobox(
                                ui,
                                "video_source_lang",
                                crate::language_label(language, &controller.draft_source_lang),
                                &mut controller.draft_source_lang,
                                &source_options,
                            ) && controller.draft_source_lang != "auto"
                                && controller.draft_target_lang == controller.draft_source_lang
                            {
                                controller.draft_target_lang = if controller.draft_source_lang == "zh" {
                                    "en".to_string()
                                } else {
                                    "zh".to_string()
                                };
                            }
                        });

                        ui.add_space(24.0);

                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(tr(language, "Translation language"))
                                    .strong()
                                    .color(crate::ui::theme::text_strong()),
                            );
                            ui.add_space(4.0);

                            components::target_language_pair_selector(
                                ui,
                                "video_create",
                                &controller.draft_source_lang,
                                &mut controller.draft_target_lang,
                                language,
                                |code, lang| crate::language_label(lang, code).to_string(),
                            );
                        });
                    });

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(16.0);

                    ui.label(
                        egui::RichText::new(tr(language, "Subtitle Mode"))
                            .size(15.0)
                            .strong()
                            .color(crate::ui::theme::text_strong()),
                    );
                    ui.add_space(8.0);

                    let mut mode_idx = match controller.draft_subtitle_mode {
                        VideoSubtitleMode::RealtimeTranslation => 0,
                        VideoSubtitleMode::ImportedSrt(_) => 1,
                        VideoSubtitleMode::None => 2,
                    };

                    ui.horizontal(|ui| {
                        if ui.radio_value(&mut mode_idx, 0, tr(language, "Realtime Audio Translation")).clicked() {
                            controller.draft_subtitle_mode = VideoSubtitleMode::RealtimeTranslation;
                        }
                        ui.add_space(12.0);
                        if ui.radio_value(&mut mode_idx, 1, tr(language, "Import Existing SRT Subtitles")).clicked() {
                            controller.draft_subtitle_mode = VideoSubtitleMode::ImportedSrt(PathBuf::new());
                        }
                        ui.add_space(12.0);
                        if ui.radio_value(&mut mode_idx, 2, tr(language, "No Subtitles")).clicked() {
                            controller.draft_subtitle_mode = VideoSubtitleMode::None;
                        }
                    });

                    if let VideoSubtitleMode::ImportedSrt(path) = &mut controller.draft_subtitle_mode {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let path_str = path.to_string_lossy().to_string();
                            ui.add(
                                egui::TextEdit::singleline(&mut path_str.as_str())
                                    .desired_width((ui.available_width() - 110.0).max(100.0))
                                    .interactive(false),
                            );
                            if components::primary_button(ui, tr(language, "Choose SRT...")).clicked() {
                                if let Some(srt_file) = rfd::FileDialog::new()
                                    .add_filter("SRT Subtitles", &["srt"])
                                    .pick_file()
                                {
                                    *path = srt_file;
                                }
                            }
                        });
                    }

                    if controller.draft_subtitle_mode == VideoSubtitleMode::RealtimeTranslation {
                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(16.0);

                        ui.label(
                            egui::RichText::new(tr(language, "Recognition Settings"))
                                .size(15.0)
                                .strong()
                                .color(crate::ui::theme::text_strong()),
                        );
                        ui.add_space(8.0);

                        let recognize_when = tr(language, "Recognize when:");
                        let speak = tr(language, "Speak");
                        let always = tr(language, "Always");
                        let vad_sensitivity = tr(language, "VAD Sensitivity");
                        let pause_tolerance = tr(language, "Pause tolerance");

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(recognize_when)
                                    .strong()
                                    .color(crate::ui::theme::text_strong()),
                            );
                            egui::ComboBox::from_id_salt("video_draft_recognition_timing")
                                .selected_text(if controller.draft_recognition.continuous_recognition {
                                    always
                                } else {
                                    speak
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut controller.draft_recognition.continuous_recognition, false, speak);
                                    ui.selectable_value(&mut controller.draft_recognition.continuous_recognition, true, always);
                                });
                        });

                        ui.add_space(8.0);

                        components::modern_slider_f32(
                            ui,
                            &mut controller.draft_recognition.background_noise,
                            0.05..=0.8,
                            0.15,
                            vad_sensitivity,
                            &[],
                        );

                        if !controller.draft_recognition.continuous_recognition {
                            ui.add_space(8.0);
                            components::modern_slider_f32(
                                ui,
                                &mut controller.draft_recognition.pause_tolerance,
                                0.0..=1.0,
                                0.5,
                                pause_tolerance,
                                &[],
                            );
                        }
                    }

                    ui.add_space(20.0);

                    ui.horizontal(|ui| {
                        if components::primary_button(ui, tr(language, "Start Playback")).clicked() {
                            match controller.start_draft_task() {
                                Ok(task_id) => {
                                    if let Some(task) = controller.store.get(&task_id) {
                                        if task.subtitle_mode == VideoSubtitleMode::RealtimeTranslation {
                                            match &task.source {
                                                super::backend::MediaSource::LocalFile(path) => {
                                                    action = VideoPlayerAction::StartTranslation(
                                                        PlayerTranslationRequest::ImportMediaFile {
                                                            path: path.clone(),
                                                            source_language: task.source_language.clone(),
                                                            target_language: task.target_language.clone(),
                                                            recognition: task.recognition.clone(),
                                                        },
                                                    );
                                                }
                                                super::backend::MediaSource::NetworkStream(_) => {
                                                    action = VideoPlayerAction::StartTranslation(
                                                        PlayerTranslationRequest::LiveStream {
                                                            source_language: task.source_language.clone(),
                                                            target_language: task.target_language.clone(),
                                                            recognition: task.recognition.clone(),
                                                        },
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    controller.error = Some(e);
                                }
                            }
                        }

                        ui.add_space(8.0);
                        if components::animated_button(ui, tr(language, "Back to Library")).clicked() {
                            controller.open_library();
                            action = VideoPlayerAction::StopTranslation;
                        }
                    });
                });
            });
            ui.add_space(16.0);
        });

    action
}

fn render_player(
    controller: &mut VideoPlayerController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) -> VideoPlayerAction {
    let mut action = VideoPlayerAction::None;

    if !controller.fullscreen_mode {
        ui.horizontal(|ui| {
            if components::animated_button(ui, tr(language, "Back to Library")).clicked() {
                controller.open_library();
                action = VideoPlayerAction::StopTranslation;
            }

            ui.add_space(8.0);
            let title = if let Some(src) = &controller.current_source {
                src.display_title()
            } else {
                tr(language, "Video Player").to_string()
            };

            let title_available = (ui.available_width() - 240.0).max(100.0);
            ui.add_sized(
                [title_available, 28.0],
                egui::Label::new(
                    egui::RichText::new(title)
                        .size(17.0)
                        .color(crate::ui::theme::text_strong())
                        .strong(),
                )
                .truncate(),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if components::animated_button(ui, tr(language, "New Video")).clicked() {
                    controller.open_create();
                    action = VideoPlayerAction::StopTranslation;
                }

                if controller.subtitles.count() > 0
                    && components::animated_button(ui, tr(language, "Export SRT")).clicked()
                    && let Some(save_path) = rfd::FileDialog::new()
                        .set_file_name("subtitles.srt")
                        .add_filter("Subtitles", &["srt"])
                        .save_file()
                {
                    let _ = std::fs::write(save_path, controller.subtitles.export_srt());
                }
            });
        });

        ui.add_space(10.0);
    }

    if let Some(error) = &controller.error {
        components::danger_alert(ui, error);
        ui.add_space(10.0);
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            render_viewport_card(controller, language, ui);
            render_subtitles_card(controller, language, ui);
            ui.add_space(16.0);
        });

    action
}

fn render_viewport_card(
    controller: &mut VideoPlayerController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) {
    let viewport_height = if controller.fullscreen_mode {
        (ui.available_height() - 20.0).max(400.0)
    } else {
        380.0
    };

    components::card(ui, |ui| {
        ui.vertical(|ui| {
            // 1. Video Canvas Frame
            let (video_rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), viewport_height - 68.0),
                egui::Sense::hover(),
            );

            let is_mouse_over = response.hovered()
                || ui.input(|i| i.pointer.latest_pos().map_or(false, |pos| video_rect.contains(pos)));
            if is_mouse_over {
                controller.note_mouse_motion();
            }

            let is_playing = controller.current_source.is_some()
                && (controller.get_status() == PlaybackStatus::Playing
                    || controller.get_status() == PlaybackStatus::Paused);

            if is_playing {
                if controller.native_host.is_none() {
                    match crate::plugins::player::backend::window::NativeVideoHost::new() {
                        Ok(host) => {
                            if let Some(backend) = &mut controller.backend {
                                backend.attach_native_host(host.hwnd.0 as *mut std::ffi::c_void);
                            }
                            controller.native_host = Some(host);
                        }
                        Err(e) => {
                            log::error!("Failed to initialize NativeVideoHost: {}", e);
                        }
                    }
                }

                if let Some(host) = &controller.native_host {
                    let pixels_per_point = ui.ctx().pixels_per_point();
                    let physical_min = video_rect.min.to_vec2() * pixels_per_point;
                    let physical_size = video_rect.size() * pixels_per_point;
                    host.set_rect(
                        physical_min.x.round() as i32,
                        physical_min.y.round() as i32,
                        physical_size.x.round() as i32,
                        physical_size.y.round() as i32,
                    );
                    host.show();
                }
            } else {
                if let Some(host) = &controller.native_host {
                    host.hide();
                }
                ui.painter().rect_filled(
                    video_rect,
                    CornerRadius::same(12),
                    Color32::from_rgb(15, 23, 42),
                );
                ui.painter().text(
                    video_rect.center() - egui::vec2(0.0, 10.0),
                    egui::Align2::CENTER_CENTER,
                    tr(language, "No video loaded"),
                    egui::FontId::proportional(17.0),
                    Color32::from_rgb(226, 232, 240),
                );
                ui.painter().text(
                    video_rect.center() + egui::vec2(0.0, 15.0),
                    egui::Align2::CENTER_CENTER,
                    tr(
                        language,
                        "Select a local video file or enter a network stream URL to start playback.",
                    ),
                    egui::FontId::proportional(12.0),
                    Color32::from_rgb(148, 163, 184),
                );
            }

            ui.add_space(8.0);

            // 2. Light Theme Control Bar
            Frame::new()
                .fill(Color32::from_rgb(245, 248, 252))
                .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let play_text = if controller.get_status() == PlaybackStatus::Playing {
                            tr(language, "Pause")
                        } else {
                            tr(language, "Play")
                        };
                        if components::primary_button_enabled(
                            ui,
                            play_text,
                            controller.current_source.is_some(),
                        )
                        .clicked()
                        {
                            controller.toggle_play();
                        }

                        ui.add_space(8.0);

                        ui.label(
                            egui::RichText::new(format!(
                                "{} / {}",
                                format_time_ms(controller.get_time_ms()),
                                format_time_ms(controller.get_duration_ms())
                            ))
                            .monospace()
                            .size(11.5)
                            .strong()
                            .color(crate::ui::theme::text_strong()),
                        );

                        ui.add_space(8.0);

                        let mut current_sec = (controller.get_time_ms() / 1000) as f32;
                        let max_sec = (controller.get_duration_ms().max(1) / 1000) as f32;
                        let seek_slider_width = (ui.available_width() - 340.0).max(60.0);
                        let slider = egui::Slider::new(&mut current_sec, 0.0..=max_sec).show_value(false);
                        if ui.add_sized([seek_slider_width, 18.0], slider).changed() {
                            if let Some(backend) = &mut controller.backend {
                                backend.seek((current_sec * 1000.0) as i64);
                            }
                        }

                        ui.add_space(8.0);

                        let mut vol = controller.volume;
                        let vol_slider = egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false);
                        if ui.add_sized([45.0, 16.0], vol_slider).changed() {
                            controller.set_volume(vol);
                        }

                        ui.add_space(4.0);

                        if components::animated_button(
                            ui,
                            if controller.muted {
                                tr(language, "Unmute")
                            } else {
                                tr(language, "Mute")
                            },
                        )
                        .clicked()
                        {
                            controller.toggle_mute();
                        }

                        ui.add_space(4.0);

                        if components::animated_button(
                            ui,
                            if controller.show_subtitles {
                                tr(language, "Hide Subtitles")
                            } else {
                                tr(language, "Show Subtitles")
                            },
                        )
                        .clicked()
                        {
                            controller.show_subtitles = !controller.show_subtitles;
                        }

                        ui.add_space(4.0);

                        if components::animated_button(
                            ui,
                            if controller.fullscreen_mode {
                                tr(language, "Exit Fullscreen")
                            } else {
                                tr(language, "Fullscreen")
                            },
                        )
                        .clicked()
                        {
                            controller.toggle_fullscreen();
                        }
                    });
                });

            if is_playing {
                let diag = controller.get_diagnostics();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "HWDEC: {} | Codec: {} | {}x{} | {:.1} FPS | Dropped: {}",
                            if diag.hwdec_current.is_empty() { "auto" } else { &diag.hwdec_current },
                            if diag.video_codec.is_empty() { "-" } else { &diag.video_codec },
                            diag.width,
                            diag.height,
                            diag.fps,
                            diag.dropped_frames,
                        ))
                        .size(11.0)
                        .color(crate::ui::theme::text_weak())
                        .monospace(),
                    );
                });
            }
        });
    });
}

fn render_subtitles_card(
    controller: &mut VideoPlayerController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) {
    if controller.fullscreen_mode {
        return;
    }

    ui.add_space(10.0);

    components::card(ui, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr(language, "Live Subtitles & Translation"))
                        .size(16.0)
                        .strong()
                        .color(crate::ui::theme::text_strong()),
                );
                ui.add_space(8.0);
                components::speaker_badge(
                    ui,
                    &format!("{} {}", controller.subtitles.count(), tr(language, "Subtitles Count")),
                );
            });

            ui.add_space(10.0);

            let current_time_ms = controller.get_time_ms();
            let active_cue = controller.subtitles.active_cue_at(current_time_ms);

            // Highlighted Active Subtitle Banner (Full Card Width)
            Frame::new()
                .fill(Color32::from_rgb(239, 246, 255))
                .stroke(Stroke::new(1.0, Color32::from_rgb(191, 219, 254)))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if let Some(cue) = active_cue {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                if let Some(speaker) = &cue.speaker_name {
                                    components::speaker_badge(ui, speaker);
                                    ui.add_space(6.0);
                                }
                                ui.label(
                                    egui::RichText::new(format!(
                                        "[{} - {}]",
                                        format_time_ms(cue.start_ms),
                                        format_time_ms(cue.end_ms.max(cue.start_ms + 2000))
                                    ))
                                    .size(11.5)
                                    .color(Color32::from_rgb(59, 130, 246)),
                                );
                            });

                            ui.add_space(4.0);

                            ui.label(
                                egui::RichText::new(&cue.original_text)
                                    .size(14.0)
                                    .color(crate::ui::theme::text_weak()),
                            );

                            if let Some(trans) = &cue.translated_text {
                                ui.add_space(2.0);
                                ui.label(
                                    egui::RichText::new(trans)
                                        .size(18.0)
                                        .strong()
                                        .color(Color32::from_rgb(30, 58, 138)),
                                );
                            }
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(tr(language, "No subtitle at current timestamp"))
                                    .size(14.0)
                                    .color(crate::ui::theme::text_weak()),
                            );
                        });
                    }
                });

            // Scrollable Timeline History (Virtualized with show_rows for extreme performance)
            let cues = controller.subtitles.cues();
            if !cues.is_empty() {
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(tr(language, "Subtitle Timeline"))
                        .size(13.0)
                        .strong()
                        .color(crate::ui::theme::text_strong()),
                );
                ui.add_space(6.0);

                let mut seek_to_ms = None;
                let total_rows = cues.len();
                let row_height = 46.0;

                egui::ScrollArea::vertical()
                    .id_salt("player_subtitles_timeline_scroll")
                    .max_height(200.0)
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, total_rows, |ui, row_range| {
                        for idx in row_range {
                            let cue = &cues[idx];
                            let is_current = current_time_ms >= cue.start_ms
                                && current_time_ms <= cue.end_ms.max(cue.start_ms + 2000);

                            let bg_color = if is_current {
                                Color32::from_rgb(239, 246, 255)
                            } else {
                                Color32::from_rgb(248, 250, 252)
                            };

                            let resp = Frame::new()
                                .fill(bg_color)
                                .stroke(if is_current {
                                    Stroke::new(1.0, Color32::from_rgb(147, 197, 253))
                                } else {
                                    Stroke::new(1.0, Color32::from_rgb(241, 245, 249))
                                })
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format_time_ms(cue.start_ms))
                                                .monospace()
                                                .size(11.5)
                                                .color(Color32::from_rgb(37, 99, 235))
                                                .strong(),
                                        );
                                        ui.add_space(6.0);

                                        if let Some(speaker) = &cue.speaker_name {
                                            components::speaker_badge(ui, speaker);
                                            ui.add_space(6.0);
                                        }

                                        ui.vertical(|ui| {
                                            ui.label(
                                                egui::RichText::new(&cue.original_text)
                                                    .size(12.5)
                                                    .color(crate::ui::theme::text_weak()),
                                            );
                                            if let Some(trans) = &cue.translated_text {
                                                ui.label(
                                                    egui::RichText::new(trans)
                                                        .size(13.5)
                                                        .strong()
                                                        .color(crate::ui::theme::text_strong()),
                                                );
                                            }
                                        });
                                    });
                                })
                                .response
                                .interact(egui::Sense::click());

                            if resp.clicked() {
                                seek_to_ms = Some(cue.start_ms);
                            }
                            ui.add_space(4.0);
                        }
                    });

                if let Some(ms) = seek_to_ms {
                    if let Some(backend) = &mut controller.backend {
                        backend.seek(ms);
                    }
                }
            }
        });
    });
}

fn format_time_ms(ms: i64) -> String {
    let secs = ms.max(0) / 1000;
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

fn format_timestamp_date(timestamp_sec: u64) -> String {
    let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_sec);
    if let Ok(duration) = std::time::SystemTime::now().duration_since(dt) {
        let mins = duration.as_secs() / 60;
        if mins < 1 {
            return "Just now".into();
        } else if mins < 60 {
            return format!("{}m ago", mins);
        }
        let hours = mins / 60;
        if hours < 24 {
            return format!("{}h ago", hours);
        }
        let days = hours / 24;
        return format!("{}d ago", days);
    }
    "Recently".into()
}
