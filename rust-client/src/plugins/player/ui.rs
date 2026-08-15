use super::{
    backend::PlaybackStatus,
    controller::{VideoPlayerController, VideoPlayerRoute},
    i18n::tr,
    PlayerTranslationRequest, VideoPlayerAction, VideoPlayerPlugin, VideoPlayerUiSnapshot,
};
use crate::ui::components;
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Stroke};

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

fn render_runtime_install_banner(
    controller: &mut VideoPlayerController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) {
    if controller.backend.is_some() {
        return;
    }

    components::card(ui, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🎬").size(22.0));
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(tr(language, "Video Player Runtime Missing"))
                            .size(16.0)
                            .strong()
                            .color(crate::ui::theme::text_strong()),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(tr(
                            language,
                            "Video playback and multi-track audio extraction require the mpv runtime library (mpv-2.dll). You can click the button below to download and configure it directly from the GitHub repository.",
                        ))
                        .size(12.5)
                        .color(crate::ui::theme::text_weak()),
                    );
                });
            });

            ui.add_space(10.0);

            let state = controller.mpv_installer.state().clone();
            match state {
                super::installer::MpvInstallState::Idle => {
                    if components::primary_button(
                        ui,
                        tr(language, "Download Player Runtime (46.8 MB)"),
                    )
                    .clicked()
                    {
                        let _ = controller.mpv_installer.start_download();
                    }
                }
                super::installer::MpvInstallState::Downloading { downloaded, total } => {
                    ui.horizontal(|ui| {
                        let ratio = if total > 0 {
                            (downloaded as f32 / total as f32).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        let progress_text = format!(
                            "{} / {} ({:.1}%)",
                            components::format_file_size(downloaded),
                            components::format_file_size(total),
                            ratio * 100.0
                        );
                        ui.add(
                            egui::ProgressBar::new(ratio)
                                .text(progress_text)
                                .animate(true),
                        );
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(tr(language, "Downloading runtime..."))
                            .size(12.0)
                            .color(crate::ui::theme::text_weak()),
                    );
                }
                super::installer::MpvInstallState::Extracting => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new(tr(
                                language,
                                "Extracting and installing player runtime...",
                            ))
                            .size(13.0)
                            .color(crate::ui::theme::primary()),
                        );
                    });
                }
                super::installer::MpvInstallState::Failed(ref err) => {
                    ui.label(
                        egui::RichText::new(format!("{}: {err}", tr(language, "Download failed")))
                            .size(12.5)
                            .color(crate::ui::theme::danger()),
                    );
                    ui.add_space(6.0);
                    if components::primary_button(ui, tr(language, "Retry Download")).clicked() {
                        let _ = controller.mpv_installer.start_download();
                    }
                }
                super::installer::MpvInstallState::Ready => {
                    ui.label(
                        egui::RichText::new(tr(language, "Player runtime installed successfully!"))
                            .size(13.0)
                            .color(crate::ui::theme::success()),
                    );
                }
            }
        });
    });
    ui.add_space(14.0);
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

    render_runtime_install_banner(controller, language, ui);

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
        if controller.backend.is_none() && !controller.try_init_backend() {
            controller.error = Some(
                tr(
                    language,
                    "Please download and install the player runtime first",
                )
                .into(),
            );
        } else if let Ok(_) = controller.play_task(&task.id) {
            action = VideoPlayerAction::None;
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

    render_runtime_install_banner(controller, language, ui);

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
                        ui.add(
                            egui::TextEdit::singleline(&mut controller.draft_source)
                                .hint_text(tr(language, "Enter stream URL or choose local file..."))
                                .desired_width(text_width),
                        );

                        if components::primary_button(ui, tr(language, "Browse...")).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Video Files", &["mp4", "mkv", "webm", "avi", "mov", "flv", "ts", "m4v"])
                                .add_filter("Audio Files", &["mp3", "wav", "flac", "aac", "ogg", "m4a"])
                                .pick_file()
                            {
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

                    ui.add_space(20.0);

                    ui.horizontal(|ui| {
                        if components::primary_button(ui, tr(language, "Create & Play")).clicked() {
                            if controller.backend.is_none() && !controller.try_init_backend() {
                                controller.error = Some(
                                    tr(
                                        language,
                                        "Please download and install the player runtime first",
                                    )
                                    .into(),
                                );
                            } else {
                                match controller.start_draft_task() {
                                    Ok(_) => {
                                        controller.error = None;
                                        action = VideoPlayerAction::None;
                                    }
                                    Err(e) => {
                                        controller.error = Some(e);
                                    }
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

    // Handle ESC key to exit fullscreen
    if controller.fullscreen_mode && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        controller.fullscreen_mode = false;
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
    }

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

    if controller.fullscreen_mode {
        render_viewport_card(controller, language, ui);
    } else {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                render_viewport_card(controller, language, ui);
                let task_action = render_task_control_card(controller, language, ui);
                if task_action != VideoPlayerAction::None {
                    action = task_action;
                }
                render_subtitles_card(controller, language, ui);
                ui.add_space(16.0);
            });
    }

    action
}

fn render_task_control_card(
    controller: &mut VideoPlayerController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) -> VideoPlayerAction {
    let mut action = VideoPlayerAction::None;

    if controller.fullscreen_mode {
        return action;
    }

    let Some(active_id) = controller.active_task_id.clone() else {
        return action;
    };

    let Some(task) = controller.store.get_mut(&active_id) else {
        return action;
    };

    let mut routing_changed = false;
    let mut task_settings_changed = false;
    let mut do_start = false;
    let mut do_pause = false;
    let mut do_restart = false;

    ui.add_space(10.0);

    components::card(ui, |ui| {
        ui.vertical(|ui| {
            // Header: Task Status and Start / Pause / Restart Buttons
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr(language, "Task Configuration"))
                        .size(16.0)
                        .strong()
                        .color(crate::ui::theme::text_strong()),
                );

                ui.add_space(8.0);

                if task.is_task_running {
                    Frame::new()
                        .fill(Color32::from_rgb(220, 252, 231))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(8, 2))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("● {}", tr(language, "Running")))
                                    .size(11.5)
                                    .color(Color32::from_rgb(22, 101, 52))
                                    .strong(),
                            );
                        });
                } else if task.subtitles.count() > 0 {
                    Frame::new()
                        .fill(Color32::from_rgb(238, 242, 255))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(8, 2))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("✓ {}", tr(language, "Completed")))
                                    .size(11.5)
                                    .color(Color32::from_rgb(79, 70, 229))
                                    .strong(),
                            );
                        });
                } else {
                    Frame::new()
                        .fill(Color32::from_rgb(241, 245, 249))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(8, 2))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("○ {}", tr(language, "Idle / Ready")))
                                    .size(11.5)
                                    .color(crate::ui::theme::text_weak()),
                            );
                        });
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if components::animated_button(ui, tr(language, "Clear & Restart")).clicked() {
                        do_restart = true;
                    }

                    ui.add_space(6.0);

                    if task.is_task_running {
                        if components::animated_button(ui, tr(language, "Pause Task")).clicked() {
                            do_pause = true;
                        }
                    } else {
                        if components::primary_button(ui, tr(language, "Start Task")).clicked() {
                            do_start = true;
                        }
                    }
                });
            });

            ui.add_space(10.0);

            // Processing Progress Section (Audio Extraction & Recognition Progress Bars)
            let is_extracting = controller.is_extracting;
            let extract_frac = controller
                .extraction_progress
                .unwrap_or(if task.subtitles.count() > 0 || (task.is_task_running && !is_extracting) {
                    1.0
                } else {
                    0.0
                })
                .clamp(0.0, 1.0);
            let total_dur_ms = task.duration_ms;
            let last_cue_end_ms = task.subtitles.cues().last().map(|c| c.end_ms).unwrap_or(0);
            let recog_pos_ms = controller
                .recognize_position
                .map(|p| p.as_millis() as i64)
                .unwrap_or(last_cue_end_ms);
            let recog_frac = if total_dur_ms > 0 {
                (recog_pos_ms as f32 / total_dur_ms as f32).clamp(0.0, 1.0)
            } else {
                controller
                    .recognition_progress
                    .unwrap_or(if task.subtitles.count() > 0 && !task.is_task_running {
                        1.0
                    } else {
                        0.0
                    })
                    .clamp(0.0, 1.0)
            };

            Frame::new()
                .fill(Color32::from_rgb(248, 250, 252))
                .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("📊 {}", tr(language, "Processing Progress")))
                                .size(13.5)
                                .strong()
                                .color(crate::ui::theme::text_strong()),
                        );
                        if task.subtitles.count() > 0 {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                Frame::new()
                                    .fill(Color32::from_rgb(238, 242, 255))
                                    .corner_radius(CornerRadius::same(4))
                                    .inner_margin(Margin::symmetric(6, 2))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "✓ {} {}",
                                                task.subtitles.count(),
                                                tr(language, "cues generated")
                                            ))
                                            .size(11.0)
                                            .color(Color32::from_rgb(79, 70, 229))
                                            .strong(),
                                        );
                                    });
                            });
                        }
                    });

                    ui.add_space(8.0);

                    // 1. Audio Extraction Progress Bar
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("🎵 {}", tr(language, "Audio Extraction")))
                                .size(12.0)
                                .color(crate::ui::theme::text_strong()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let status_text = if is_extracting {
                                let pos_str = controller
                                    .extract_position
                                    .map(|p| format_time_ms(p.as_millis() as i64))
                                    .unwrap_or_else(|| "00:00".into());
                                let dur_str = controller
                                    .extract_duration
                                    .or_else(|| {
                                        if total_dur_ms > 0 {
                                            Some(std::time::Duration::from_millis(
                                                total_dur_ms as u64,
                                            ))
                                        } else {
                                            None
                                        }
                                    })
                                    .map(|d| format_time_ms(d.as_millis() as i64))
                                    .unwrap_or_else(|| "--:--".into());
                                format!(
                                    "{} / {} · {:.0}%",
                                    pos_str,
                                    dur_str,
                                    extract_frac * 100.0
                                )
                            } else if extract_frac >= 1.0 {
                                format!("{} (100%)", tr(language, "Extraction Completed"))
                            } else {
                                tr(language, "Ready").to_string()
                            };
                            ui.label(
                                egui::RichText::new(status_text).size(11.5).color(
                                    if is_extracting {
                                        Color32::from_rgb(37, 99, 235)
                                    } else {
                                        crate::ui::theme::text_weak()
                                    },
                                ),
                            );
                        });
                    });
                    ui.add_space(3.0);
                    ui.add(
                        egui::ProgressBar::new(extract_frac)
                            .desired_height(6.0)
                            .corner_radius(CornerRadius::same(3))
                            .animate(is_extracting),
                    );

                    ui.add_space(10.0);

                    // 2. Speech Recognition Progress Bar
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "🎙️ {}",
                                tr(language, "Speech Recognition & Subtitles")
                            ))
                            .size(12.0)
                            .color(crate::ui::theme::text_strong()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let status_text = if task.is_task_running && !is_extracting {
                                format!(
                                    "{} / {} · {:.0}%",
                                    format_time_ms(recog_pos_ms),
                                    format_time_ms(total_dur_ms),
                                    recog_frac * 100.0
                                )
                            } else if recog_frac >= 1.0
                                || (task.subtitles.count() > 0 && !task.is_task_running)
                            {
                                format!(
                                    "{} ({} / {})",
                                    tr(language, "Recognition Completed"),
                                    format_time_ms(recog_pos_ms),
                                    format_time_ms(total_dur_ms)
                                )
                            } else {
                                tr(language, "Ready").to_string()
                            };
                            ui.label(
                                egui::RichText::new(status_text).size(11.5).color(
                                    if task.is_task_running && !is_extracting {
                                        Color32::from_rgb(16, 185, 129)
                                    } else {
                                        crate::ui::theme::text_weak()
                                    },
                                ),
                            );
                        });
                    });
                    ui.add_space(3.0);
                    ui.add(
                        egui::ProgressBar::new(recog_frac)
                            .desired_height(6.0)
                            .corner_radius(CornerRadius::same(3))
                            .animate(task.is_task_running && !is_extracting),
                    );
                });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            let config_summary = format!(
                "{} → {} | VAD: {:.2} | Pause: {:.2}s",
                crate::language_label(language, &task.source_language),
                crate::language_label(language, &task.target_language),
                task.recognition.background_noise,
                task.recognition.pause_tolerance
            );

            egui::CollapsingHeader::new(
                egui::RichText::new(format!(
                    "⚙ {} ({})",
                    tr(language, "Recognition Settings"),
                    config_summary
                ))
                .size(13.0)
                .color(crate::ui::theme::text_weak()),
            )
            .id_salt("video_player_recognition_settings")
            .default_open(!task.is_task_running)
            .show(ui, |ui| {
                ui.add_space(6.0);

                // Spoken & Translation Languages, VAD
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(tr(language, "Spoken language"))
                                .size(12.5)
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
                            "video_player_source_lang",
                            crate::language_label(language, &task.source_language),
                            &mut task.source_language,
                            &source_options,
                        ) {
                            task_settings_changed = true;
                            if task.source_language != "auto" && task.target_language == task.source_language {
                                task.target_language = if task.source_language == "zh" {
                                    "en".to_string()
                                } else {
                                    "zh".to_string()
                                };
                            }
                        }
                    });

                    ui.add_space(20.0);

                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(tr(language, "Translation language"))
                                .size(12.5)
                                .strong()
                                .color(crate::ui::theme::text_strong()),
                        );
                        ui.add_space(4.0);

                        if components::target_language_pair_selector(
                            ui,
                            "video_player_target_lang",
                            &task.source_language,
                            &mut task.target_language,
                            language,
                            |code, lang| crate::language_label(lang, code).to_string(),
                        ) {
                            task_settings_changed = true;
                        }
                    });

                    ui.add_space(20.0);

                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(tr(language, "VAD Sensitivity"))
                                .size(12.5)
                                .strong()
                                .color(crate::ui::theme::text_strong()),
                        );
                        ui.add_space(4.0);
                        let mut noise = task.recognition.background_noise;
                        if ui.add(egui::Slider::new(&mut noise, 0.05..=0.8).show_value(true)).changed() {
                            task.recognition.background_noise = noise;
                            task_settings_changed = true;
                        }
                    });

                    ui.add_space(20.0);

                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(tr(language, "Pause tolerance"))
                                .size(12.5)
                                .strong()
                                .color(crate::ui::theme::text_strong()),
                        );
                        ui.add_space(4.0);
                        let mut pause = task.recognition.pause_tolerance;
                        if ui.add(egui::Slider::new(&mut pause, 0.0..=1.0).show_value(true)).changed() {
                            task.recognition.pause_tolerance = pause;
                            task_settings_changed = true;
                        }
                    });
                });

                ui.add_space(14.0);

                let layout_info = controller
                    .backend
                    .as_ref()
                    .and_then(|b| b.get_audio_layout())
                    .unwrap_or_else(|| {
                        if task.audio_channels.len() == 2 {
                            "stereo".to_string()
                        } else if task.audio_channels.len() == 6 {
                            "5.1".to_string()
                        } else {
                            format!("{} ch", task.audio_channels.len())
                        }
                    });

                // Channel Routing Matrix Collapsible Panel
                egui::CollapsingHeader::new(
                    egui::RichText::new(format!(
                        "🔊 {} ({}: {})",
                        tr(language, "Channel Routing & Separation"),
                        tr(language, "Audio Layout"),
                        layout_info
                    ))
                    .size(14.0)
                    .strong()
                    .color(crate::ui::theme::text_strong()),
                )
                .id_salt("video_player_channel_routing")
                .default_open(true)
                .show(ui, |ui| {
                    ui.add_space(6.0);

                    // Quick Presets
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Presets:")
                                .size(12.0)
                                .color(crate::ui::theme::text_weak()),
                        );
                        ui.add_space(4.0);

                        if ui.button(egui::RichText::new(tr(language, "Enable All")).size(11.5)).clicked() {
                            for ch in &mut task.audio_channels {
                                ch.playback = true;
                                ch.recognition = true;
                            }
                            routing_changed = true;
                        }

                        if ui.button(egui::RichText::new(tr(language, "Dialogue Only")).size(11.5)).clicked() {
                            let has_fc = task.audio_channels.iter().any(|c| c.id == "fc");
                            for ch in &mut task.audio_channels {
                                ch.playback = true;
                                if has_fc {
                                    ch.recognition = ch.id == "fc";
                                } else {
                                    ch.recognition = true;
                                }
                            }
                            routing_changed = true;
                        }

                        if ui.button(egui::RichText::new(tr(language, "Stereo Default")).size(11.5)).clicked() {
                            for ch in &mut task.audio_channels {
                                ch.playback = true;
                                if ch.id == "lfe" || ch.id == "sl" || ch.id == "sr" || ch.id == "bl" || ch.id == "br" {
                                    ch.recognition = false;
                                } else {
                                    ch.recognition = true;
                                }
                            }
                            routing_changed = true;
                        }
                    });

                    ui.add_space(8.0);

                    // Double-Column Checkbox Table Frame
                    Frame::new()
                        .fill(Color32::from_rgb(248, 250, 252))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            egui::Grid::new("player_channel_routing_grid")
                                .num_columns(3)
                                .spacing([24.0, 8.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    // Table Header
                                    ui.label(
                                        egui::RichText::new(tr(language, "Channel"))
                                            .size(12.5)
                                            .strong()
                                            .color(crate::ui::theme::text_strong()),
                                    );
                                    ui.label(
                                        egui::RichText::new(tr(language, "Playback (Hear Audio)"))
                                            .size(12.5)
                                            .strong()
                                            .color(Color32::from_rgb(37, 99, 235)),
                                    );
                                    ui.label(
                                        egui::RichText::new(tr(language, "Recognition (Send to ASR)"))
                                            .size(12.5)
                                            .strong()
                                            .color(Color32::from_rgb(16, 185, 129)),
                                    );
                                    ui.end_row();

                                    for ch in &mut task.audio_channels {
                                        if ch.id == "fc" {
                                            ui.label(
                                                egui::RichText::new(&ch.name)
                                                    .strong()
                                                    .color(Color32::from_rgb(37, 99, 235)),
                                            );
                                        } else {
                                            ui.label(&ch.name);
                                        }

                                        if ui.checkbox(&mut ch.playback, "").changed() {
                                            routing_changed = true;
                                        }
                                        if ui.checkbox(&mut ch.recognition, "").changed() {
                                            task_settings_changed = true;
                                        }
                                        ui.end_row();
                                    }
                                });
                        });
                });
            });
        });
    });

    if do_restart {
        if let Some(task) = controller.store.get(&active_id) {
            let source = task.source.clone();
            let source_language = task.source_language.clone();
            let target_language = task.target_language.clone();
            let recognition = task.recognition.clone();
            let audio_channels = task.audio_channels.clone();
            controller.clear_and_restart_task();
            match &source {
                super::backend::MediaSource::LocalFile(path) => {
                    action = VideoPlayerAction::StartTranslation(
                        PlayerTranslationRequest::ImportMediaFile {
                            path: path.clone(),
                            source_language,
                            target_language,
                            recognition,
                            audio_channels,
                        },
                    );
                }
                super::backend::MediaSource::NetworkStream(_) => {
                    action = VideoPlayerAction::StartTranslation(
                        PlayerTranslationRequest::LiveStream {
                            source_language,
                            target_language,
                            recognition,
                            audio_channels,
                        },
                    );
                }
            }
        }
    } else if do_pause {
        controller.pause_task();
        action = VideoPlayerAction::StopTranslation;
    } else if do_start {
        if let Some(task) = controller.store.get(&active_id) {
            let source = task.source.clone();
            let source_language = task.source_language.clone();
            let target_language = task.target_language.clone();
            let recognition = task.recognition.clone();
            let audio_channels = task.audio_channels.clone();
            controller.start_task();
            match &source {
                super::backend::MediaSource::LocalFile(path) => {
                    action = VideoPlayerAction::StartTranslation(
                        PlayerTranslationRequest::ImportMediaFile {
                            path: path.clone(),
                            source_language,
                            target_language,
                            recognition,
                            audio_channels,
                        },
                    );
                }
                super::backend::MediaSource::NetworkStream(_) => {
                    action = VideoPlayerAction::StartTranslation(
                        PlayerTranslationRequest::LiveStream {
                            source_language,
                            target_language,
                            recognition,
                            audio_channels,
                        },
                    );
                }
            }
        }
    } else if routing_changed {
        controller.apply_channel_routing();
    } else if task_settings_changed {
        let _ = controller.store.save_to_dir(&controller.storage_dir);
    }

    action
}

fn render_viewport_card(
    controller: &mut VideoPlayerController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) {
    let total_player_height = if controller.fullscreen_mode {
        ui.available_height()
    } else {
        400.0
    };

    let is_playing = controller.current_source.is_some()
        && (controller.get_status() == PlaybackStatus::Playing
            || controller.get_status() == PlaybackStatus::Paused);

    let is_paused = controller.get_status() == PlaybackStatus::Paused;
    let recent_hover = controller
        .last_hover_instant
        .map_or(true, |inst| inst.elapsed().as_secs_f32() < 2.5);

    let show_controls = !is_playing || is_paused || recent_hover;

    // Smooth animated opacity for the floating overlay
    let controls_alpha = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        egui::Id::new("player_controls_overlay_alpha"),
        show_controls,
        0.22,
    );

    // Schedule repaint when timer is active so fade-out triggers automatically without user input
    if is_playing && !is_paused && recent_hover {
        let elapsed = controller
            .last_hover_instant
            .map_or(0.0, |inst| inst.elapsed().as_secs_f32());
        let remaining_ms = ((2.5 - elapsed).max(0.0) * 1000.0) as u64;
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(remaining_ms.max(20).min(100)));
    }

    // Hide mouse cursor in fullscreen when controls have faded out
    if controller.fullscreen_mode && is_playing && !is_paused && controls_alpha < 0.05 {
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
    }

    // Allow Escape key to exit fullscreen
    if controller.fullscreen_mode && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        controller.fullscreen_mode = false;
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
    }

    // Layout constants for the compact floating pill
    let bar_height = 40.0;
    let bar_margin_h = 16.0;
    let bar_margin_bottom = if controller.fullscreen_mode { 16.0 } else { 6.0 };
    let hwnd_shrink = (bar_height + bar_margin_bottom + 2.0) * controls_alpha;

    let (corner_radius, stroke_style) = if controller.fullscreen_mode {
        (CornerRadius::ZERO, Stroke::NONE)
    } else {
        (CornerRadius::same(12), Stroke::new(1.0, Color32::from_rgb(30, 41, 59)))
    };

    // Dark Cinema / Video Player Outer Container
    let outer_resp = Frame::new()
        .fill(Color32::from_rgb(10, 15, 26))
        .stroke(stroke_style)
        .corner_radius(corner_radius)
        .inner_margin(Margin::same(0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(total_player_height);

            // Video area takes the full height
            let (video_rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), total_player_height),
                egui::Sense::click_and_drag(),
            );

            let bar_rect = egui::Rect::from_min_max(
                egui::pos2(
                    video_rect.min.x + bar_margin_h,
                    video_rect.max.y - bar_height - bar_margin_bottom,
                ),
                egui::pos2(
                    video_rect.max.x - bar_margin_h,
                    video_rect.max.y - bar_margin_bottom,
                ),
            );

            let pointer_delta = ui.input(|i| i.pointer.delta());
            let is_mouse_moving = pointer_delta.length_sq() > 0.25;
            let is_interacting = ui.input(|i| {
                i.pointer.any_click()
                    || i.pointer.any_down()
                    || i.pointer.any_released()
                    || i.smooth_scroll_delta != egui::Vec2::ZERO
            });
            let is_controls_hovered = ui.input(|i| {
                i.pointer
                    .latest_pos()
                    .map_or(false, |pos| bar_rect.expand(6.0).contains(pos))
            });

            // Only refresh the idle timer when mouse is moving, clicking, scrolling, or hovering the control bar
            if is_mouse_moving || is_interacting || (is_controls_hovered && show_controls) {
                controller.note_mouse_motion();
            }

            if response.double_clicked() && controller.current_source.is_some() {
                controller.toggle_fullscreen();
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Fullscreen(controller.fullscreen_mode));
            }

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
                    let ppp = ui.ctx().pixels_per_point();
                    let physical_min = video_rect.min.to_vec2() * ppp;
                    let physical_size = video_rect.size() * ppp;
                    // Shrink native HWND by the control bar area so controls are not occluded
                    let shrink_px = hwnd_shrink * ppp;
                    host.set_rect(
                        physical_min.x.round() as i32,
                        physical_min.y.round() as i32,
                        physical_size.x.round() as i32,
                        (physical_size.y - shrink_px).round().max(0.0) as i32,
                    );
                    host.show();
                }
            } else {
                if let Some(host) = &controller.native_host {
                    host.hide();
                }
                ui.painter().rect_filled(
                    video_rect,
                    corner_radius,
                    Color32::from_rgb(10, 15, 26),
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

            // ── Floating Pill Control Bar ─────────────────────────────────────
            if controls_alpha > 0.001 {
                let alpha = (controls_alpha * 255.0) as u8;

                // Dark glass pill background
                let pill_bg = Color32::from_rgba_unmultiplied(
                    12, 18, 32,
                    (alpha as f32 * 0.92) as u8,
                );
                let pill_border = Color32::from_rgba_unmultiplied(
                    55, 70, 95,
                    (alpha as f32 * 0.6) as u8,
                );
                ui.painter().rect_filled(bar_rect, CornerRadius::same(20), pill_bg);
                ui.painter().rect_stroke(bar_rect, CornerRadius::same(20), Stroke::new(1.0, pill_border), egui::StrokeKind::Outside);

                // Child UI inside the pill
                let mut child_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(bar_rect.shrink2(egui::vec2(10.0, 0.0)))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                child_ui.set_opacity(controls_alpha);

                child_ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;

                    // ── Play / Pause ──
                    let play_icon = if controller.get_status() == PlaybackStatus::Playing {
                        "⏸"
                    } else {
                        "⏵"
                    };
                    let play_enabled = controller.current_source.is_some();
                    if dark_pill_button(ui, play_icon, play_enabled, true) {
                        controller.toggle_play();
                    }

                    // ── Time ──
                    ui.label(
                        egui::RichText::new(format!(
                            "{} / {}",
                            format_time_ms(controller.get_time_ms()),
                            format_time_ms(controller.get_duration_ms())
                        ))
                        .monospace()
                        .size(11.0)
                        .color(Color32::from_rgba_unmultiplied(200, 210, 225, alpha)),
                    );

                    // ── Seek slider ──
                    let mut current_sec = (controller.get_time_ms() / 1000) as f32;
                    let max_sec = (controller.get_duration_ms().max(1) / 1000) as f32;
                    let right_w = 180.0; // space for buttons to the right
                    let slider_w = (ui.available_width() - right_w).max(30.0);
                    let slider = egui::Slider::new(&mut current_sec, 0.0..=max_sec).show_value(false);
                    if ui.add_sized([slider_w, 14.0], slider).changed() {
                        if let Some(backend) = &mut controller.backend {
                            backend.seek((current_sec * 1000.0) as i64);
                        }
                    }

                    // ── Volume ──
                    let mut vol = controller.volume;
                    let vol_slider = egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false);
                    if ui.add_sized([36.0, 14.0], vol_slider).changed() {
                        controller.set_volume(vol);
                    }

                    // ── Mute ──
                    let mute_icon = if controller.muted { "🔇" } else { "🔊" };
                    if dark_pill_button(ui, mute_icon, true, false) {
                        controller.toggle_mute();
                    }

                    // ── Subtitles ──
                    let sub_icon = if controller.show_subtitles { "💬" } else { "💬" };
                    if dark_pill_button(ui, sub_icon, true, false) {
                        controller.show_subtitles = !controller.show_subtitles;
                    }

                    // ── Fullscreen ──
                    let fs_icon = if controller.fullscreen_mode { "⛶" } else { "⛶" };
                    if dark_pill_button(ui, fs_icon, true, false) {
                        controller.toggle_fullscreen();
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(controller.fullscreen_mode));
                    }
                });
            }
        });

    let _ = outer_resp;

    if is_playing && !controller.fullscreen_mode {
        let diag = controller.get_diagnostics();
        ui.add_space(4.0);
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
            let current_time_ms = controller.get_time_ms();
            let now = std::time::Instant::now();

            let cues = controller.subtitles.cues();
            let cues_count = cues.len();

            let active_idx = if cues_count > 0 {
                let query_time = current_time_ms + 250;
                let idx = cues.partition_point(|cue| cue.start_ms <= query_time);
                if idx > 0 {
                    let candidate_idx = idx - 1;
                    let cue = &cues[candidate_idx];
                    let effective_end = if cue.end_ms <= cue.start_ms {
                        cue.start_ms + 3000
                    } else {
                        cue.end_ms.max(cue.start_ms + 2000)
                    };
                    if current_time_ms <= effective_end {
                        Some(candidate_idx)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let is_manually_scrolling = controller
                .last_manual_scroll
                .map_or(false, |instant| instant.elapsed() < std::time::Duration::from_secs(5));
            let auto_scroll_active = !is_manually_scrolling;

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
                    &format!("{} {}", cues_count, tr(language, "Subtitles Count")),
                );

                if is_manually_scrolling {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(
                                egui::RichText::new("⤓ Auto-scroll")
                                    .size(12.0)
                                    .color(Color32::from_rgb(37, 99, 235)),
                            )
                            .clicked()
                        {
                            controller.last_manual_scroll = None;
                            controller.last_auto_scrolled_idx = None;
                        }
                    });
                }
            });

            ui.add_space(10.0);

            if cues.is_empty() {
                Frame::new()
                    .fill(Color32::from_rgb(248, 250, 252))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(241, 245, 249)))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(20))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                egui::RichText::new(tr(language, "No subtitle at current timestamp"))
                                    .size(13.5)
                                    .color(crate::ui::theme::text_weak()),
                            );
                        });
                    });
            } else {
                let mut seek_to_ms = None;
                let row_height = 88.0;
                let total_rows = cues.len();

                let mut scroll_area = egui::ScrollArea::vertical()
                    .id_salt("player_subtitles_timeline_scroll")
                    .min_scrolled_height(360.0)
                    .max_height(500.0)
                    .auto_shrink([false, false]);

                let viewport_height = controller
                    .timeline_viewport_height
                    .unwrap_or(450.0)
                    .clamp(360.0, 500.0);

                // Programmatic auto-scroll: ONLY when transitioning to a NEW cue
                if auto_scroll_active && active_idx.is_some() && active_idx != controller.last_auto_scrolled_idx {
                    if let Some(idx) = active_idx {
                        let center_offset = (viewport_height - row_height) * 0.5;
                        let target_offset = ((idx as f32 * row_height) - center_offset).max(0.0);
                        scroll_area = scroll_area.vertical_scroll_offset(target_offset);
                        controller.last_auto_scrolled_idx = Some(idx);
                    }
                }

                let scroll_output = scroll_area.show_rows(ui, row_height, total_rows, |ui, row_range| {
                    for idx in row_range {
                        let cue = &cues[idx];
                        let is_current = Some(idx) == active_idx;

                        let bg_color = if is_current {
                            Color32::from_rgb(239, 246, 255)
                        } else {
                            Color32::from_rgb(248, 250, 252)
                        };

                        let stroke = if is_current {
                            Stroke::new(1.5, Color32::from_rgb(96, 165, 250))
                        } else {
                            Stroke::new(1.0, Color32::from_rgb(241, 245, 249))
                        };

                        let resp = Frame::new()
                            .fill(bg_color)
                            .stroke(stroke)
                            .corner_radius(CornerRadius::same(if is_current { 10 } else { 8 }))
                            .inner_margin(Margin::symmetric(14, 8))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.set_height(row_height - 8.0);
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "[{} - {}]",
                                                format_time_ms(cue.start_ms),
                                                format_time_ms(cue.end_ms.max(cue.start_ms + 2000))
                                            ))
                                            .size(11.5)
                                            .monospace()
                                            .color(if is_current {
                                                Color32::from_rgb(37, 99, 235)
                                            } else {
                                                Color32::from_rgb(59, 130, 246)
                                            })
                                            .strong(),
                                        );

                                        if let Some(speaker) = &cue.speaker_name {
                                             ui.add_space(6.0);
                                             components::speaker_badge(ui, speaker);
                                        }
                                    });

                                    ui.add_space(2.0);

                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&cue.original_text)
                                                .size(12.5)
                                                .color(if is_current {
                                                    crate::ui::theme::text_strong()
                                                } else {
                                                    crate::ui::theme::text_weak()
                                                }),
                                        )
                                        .truncate(),
                                    );

                                    if let Some(trans) = &cue.translated_text {
                                        ui.add_space(1.0);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(trans)
                                                    .size(14.0)
                                                    .strong()
                                                    .color(if is_current {
                                                        Color32::from_rgb(30, 58, 138)
                                                    } else {
                                                        Color32::from_rgb(30, 64, 175)
                                                    }),
                                            )
                                            .truncate(),
                                        );
                                    }
                                });
                            })
                            .response
                            .interact(egui::Sense::click());

                        if is_current && auto_scroll_active {
                            resp.scroll_to_me(Some(egui::Align::Center));
                        }

                        if resp.clicked() {
                            seek_to_ms = Some(cue.start_ms);
                            controller.last_manual_scroll = None;
                            controller.last_auto_scrolled_idx = None;
                        }

                        ui.add_space(8.0);
                    }
                });

                controller.timeline_viewport_height = Some(scroll_output.inner_rect.height());

                let is_hovered = ui.rect_contains_pointer(scroll_output.inner_rect)
                    || ui.rect_contains_pointer(scroll_output.inner_rect.expand(16.0));
                let wheel_scrolled = is_hovered
                    && ui.input(|i| {
                        i.smooth_scroll_delta.y.abs() > 0.05
                            || i.smooth_scroll_delta.x.abs() > 0.05
                            || i.raw.events.iter().any(|e| matches!(e, egui::Event::MouseWheel { .. }))
                    });
                let is_dragged = is_hovered && ui.input(|i| i.pointer.is_decidedly_dragging());

                if wheel_scrolled || is_dragged {
                    controller.last_manual_scroll = Some(now);
                    controller.last_auto_scrolled_idx = active_idx;
                }

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
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;
    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, mins, s)
    } else {
        format!("{:02}:{:02}", mins, s)
    }
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

/// Compact dark-themed button for the floating video control bar.
/// Returns `true` if clicked.
fn dark_pill_button(ui: &mut egui::Ui, icon: &str, enabled: bool, accent: bool) -> bool {
    let desired = egui::vec2(32.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() && enabled;
        let pressed = response.is_pointer_button_down_on() && enabled;

        let bg = if pressed {
            Color32::from_rgba_unmultiplied(80, 100, 140, 180)
        } else if hovered {
            Color32::from_rgba_unmultiplied(55, 70, 100, 160)
        } else if accent {
            Color32::from_rgba_unmultiplied(37, 99, 235, 200)
        } else {
            Color32::from_rgba_unmultiplied(35, 45, 65, 140)
        };

        let text_color = if !enabled {
            Color32::from_rgb(90, 100, 115)
        } else if pressed {
            Color32::WHITE
        } else if hovered {
            Color32::from_rgb(220, 230, 245)
        } else {
            Color32::from_rgb(190, 200, 215)
        };

        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), bg);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(14.0),
            text_color,
        );
    }

    response.clicked() && enabled
}
