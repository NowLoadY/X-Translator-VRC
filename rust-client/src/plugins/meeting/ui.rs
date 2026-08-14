use super::{
    MeetingAction, MeetingAudioSource, MeetingInputRequest, MeetingPlugin, MeetingReprocessRequest,
    MeetingStartRequest, MeetingUiSnapshot,
    controller::{
        MeetingController, MeetingPane, MeetingRoute, can_continue, meeting_status_label,
    },
    i18n::tr,
};
use crate::ui::components;
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Stroke};
use rust_client::{MarkerKind, Meeting, MeetingSourceKind, MeetingStatus, Segment, SegmentMarker};

#[derive(Clone)]
enum UiAction {
    None,
    NewLive,
    NewImport,
    CreateAndStart,
    Open(String),
    Back,
    Continue(String),
    Pause,
    End,
    NewTopic,
    AddMarker(String, MarkerKind),
    SaveMarker(SegmentMarker),
    QuickNote,
    SaveMinutes,
    JumpToEvidence(String),
    Export,
    ExportMeeting(String),
    Reprocess,
    AskDelete(String),
    Delete(String),
    CancelDelete,
    RenameSpeaker(String, String),
    MergeSpeaker(String, String),
}

pub fn render(
    plugin: &mut MeetingPlugin,
    snapshot: &MeetingUiSnapshot,
    ui: &mut egui::Ui,
) -> MeetingAction {
    let action = match plugin.controller.route {
        MeetingRoute::Library => render_library(&mut plugin.controller, snapshot.language, ui),
        MeetingRoute::Create => render_setup(&mut plugin.controller, snapshot, ui),
        MeetingRoute::Detail => render_detail(&mut plugin.controller, snapshot.language, ui),
    };
    apply_action(&mut plugin.controller, action, snapshot)
}

fn page_header(
    ui: &mut egui::Ui,
    title: &str,
    language: crate::i18n::UiLanguage,
    right: impl FnOnce(&mut egui::Ui),
) {
    let title_text = tr(language, title);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title_text)
                .size(22.0)
                .color(crate::ui::theme::text_strong())
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
    ui.add_space(14.0);
}

fn render_library(
    controller: &mut MeetingController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) -> UiAction {
    let mut action = UiAction::None;
    page_header(ui, "Meeting notes", language, |ui| {
        if components::primary_button(ui, tr(language, "New meeting")).clicked() {
            action = UiAction::NewLive;
        }
        if components::animated_button(ui, tr(language, "Import audio")).clicked() {
            action = UiAction::NewImport;
        }
    });

    if let Some(error) = &controller.error {
        components::danger_alert(ui, error);
        ui.add_space(10.0);
    }

    components::search_bar(ui, &mut controller.search, tr(language, "Search meetings"));

    ui.add_space(12.0);

    if controller.meetings.is_empty() {
        components::card(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new(tr(language, "No meetings yet"))
                        .size(16.0)
                        .strong()
                        .color(crate::ui::theme::text_strong()),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(tr(
                        language,
                        "Create a live record or import an audio file. Records are stored locally.",
                    ))
                    .size(12.0)
                    .color(crate::ui::theme::text_weak()),
                );
                ui.add_space(16.0);
            });
        });
        return action;
    }

    let query = controller.search.to_lowercase();
    egui::ScrollArea::vertical()
        .id_salt("meeting_library_scroll")
        .show(ui, |ui| {
            for meeting in controller
                .meetings
                .iter()
                .filter(|meeting| query.is_empty() || meeting.name.to_lowercase().contains(&query))
            {
                ui.push_id(("meeting-card", &meeting.id), |ui| {
                    components::card(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let (badge_bg, badge_fg, badge_text) = match meeting.source_kind {
                                    MeetingSourceKind::LiveCapture => (
                                        Color32::from_rgb(236, 253, 245),
                                        Color32::from_rgb(5, 150, 105),
                                        "LIVE",
                                    ),
                                    MeetingSourceKind::ImportedAudio => (
                                        Color32::from_rgb(238, 242, 255),
                                        Color32::from_rgb(79, 70, 229),
                                        "FILE",
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
                                        egui::RichText::new(&meeting.name)
                                            .size(15.5)
                                            .strong()
                                            .color(crate::ui::theme::text_strong()),
                                    );
                                    ui.add_space(2.0);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} · {} → {} · {}",
                                            source_label(meeting, language),
                                            meeting_language_label(&meeting.source_language, language),
                                            meeting_language_label(&meeting.target_language, language),
                                            format_timestamp(meeting.last_activity_at_ms)
                                        ))
                                        .color(crate::ui::theme::text_weak())
                                        .size(11.5),
                                    );
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let status_text = meeting_status_label(meeting.status);
                                        components::status_badge(
                                            ui,
                                            status_text,
                                            meeting.status == MeetingStatus::Live,
                                            meeting.status == MeetingStatus::Failed,
                                        );
                                    },
                                );
                            });

                            ui.add_space(10.0);

                            ui.horizontal_wrapped(|ui| {
                                if components::primary_button(ui, tr(language, "Open")).clicked() {
                                    action = UiAction::Open(meeting.id.clone());
                                }
                                if can_continue(meeting)
                                    && components::animated_button(
                                        ui,
                                        tr(language, "Continue recording"),
                                    )
                                    .clicked()
                                {
                                    action = UiAction::Continue(meeting.id.clone());
                                }
                                if components::animated_button(
                                    ui,
                                    tr(language, "Export Markdown"),
                                )
                                .clicked()
                                {
                                    action = UiAction::ExportMeeting(meeting.id.clone());
                                }
                                let active = controller.is_recording(&meeting.id);
                                let delete = ui
                                    .add_enabled_ui(!active, |ui| {
                                        components::danger_button(ui, tr(language, "Delete"))
                                    })
                                    .inner;
                                if active {
                                    delete.on_hover_text(tr(
                                        language,
                                        "Finish the active meeting before deleting it",
                                    ));
                                } else if delete.clicked() {
                                    action = UiAction::AskDelete(meeting.id.clone());
                                }
                            });

                            if controller.pending_delete.as_deref() == Some(&meeting.id) {
                                ui.add_space(8.0);
                                Frame::new()
                                    .fill(Color32::from_rgb(254, 242, 242))
                                    .stroke(Stroke::new(1.0, Color32::from_rgb(254, 202, 202)))
                                    .corner_radius(CornerRadius::same(10))
                                    .inner_margin(Margin::same(12))
                                    .show(ui, |ui| {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                egui::RichText::new(tr(
                                                    language,
                                                    "Delete this meeting and all of its local records?",
                                                ))
                                                .color(Color32::from_rgb(185, 28, 28))
                                                .strong()
                                                .size(12.5),
                                            );
                                            ui.add_space(8.0);
                                            ui.horizontal(|ui| {
                                                if components::danger_button(
                                                    ui,
                                                    tr(language, "Delete permanently"),
                                                )
                                                .clicked()
                                                {
                                                    action = UiAction::Delete(meeting.id.clone());
                                                }
                                                if components::animated_button(
                                                    ui,
                                                    tr(language, "Cancel"),
                                                )
                                                .clicked()
                                                {
                                                    action = UiAction::CancelDelete;
                                                }
                                            });
                                        });
                                    });
                            }
                        });
                    });
                    ui.add_space(10.0);
                });
            }
        });
    action
}

fn render_setup(
    controller: &mut MeetingController,
    snapshot: &MeetingUiSnapshot,
    ui: &mut egui::Ui,
) -> UiAction {
    let language = snapshot.language;
    let mut action = UiAction::None;
    page_header(
        ui,
        if controller.draft.import_audio {
            "Import audio"
        } else {
            "New live meeting"
        },
        language,
        |ui| {
            if components::animated_button(ui, tr(language, "Back")).clicked() {
                action = UiAction::Back;
            }
        },
    );

    let validation_error = draft_validation_error(controller, snapshot);

    components::card(ui, |ui| {
        ui.vertical(|ui| {
            components::section_heading(ui, tr(language, "Meeting Details"));

            ui.label(
                egui::RichText::new(tr(language, "Name"))
                    .strong()
                    .color(crate::ui::theme::text_strong()),
            );
            ui.add_space(4.0);
            components::input_field(
                ui,
                &mut controller.draft.name,
                tr(language, "Meeting name (e.g. Weekly Sync)"),
            );

            ui.add_space(14.0);

            if controller.draft.import_audio {
                ui.label(
                    egui::RichText::new(tr(language, "Audio File"))
                        .strong()
                        .color(crate::ui::theme::text_strong()),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.allocate_ui(
                        egui::vec2((ui.available_width() - 110.0).max(200.0), 0.0),
                        |ui| {
                            components::input_field(
                                ui,
                                &mut controller.draft.import_path,
                                tr(language, "Choose audio file path"),
                            );
                        },
                    );
                    ui.add_space(6.0);
                    if components::animated_button(ui, tr(language, "Choose file")).clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Audio", &["wav", "mp3", "flac", "m4a", "aac", "ogg"])
                            .pick_file()
                    {
                        controller.draft.import_path = path.display().to_string();
                        if controller.draft.name == "New meeting"
                            && let Some(stem) = path.file_stem().and_then(|value| value.to_str())
                        {
                            controller.draft.name = stem.to_owned();
                        }
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(tr(
                        language,
                        "This meeting references the source file. Moving or deleting it prevents reprocessing.",
                    ))
                    .color(crate::ui::theme::text_weak())
                    .size(11.5),
                );
            } else {
                ui.label(
                    egui::RichText::new(tr(language, "Audio source"))
                        .strong()
                        .color(crate::ui::theme::text_strong()),
                );
                ui.add_space(4.0);

                let capture_options = [
                    (
                        MeetingAudioSource::Microphone,
                        capture_label(MeetingAudioSource::Microphone, language).to_string(),
                    ),
                    (
                        MeetingAudioSource::SystemAudio,
                        capture_label(MeetingAudioSource::SystemAudio, language).to_string(),
                    ),
                    (
                        MeetingAudioSource::Both,
                        capture_label(MeetingAudioSource::Both, language).to_string(),
                    ),
                ];
                components::searchable_combobox(
                    ui,
                    "meeting_capture_source",
                    capture_label(controller.draft.capture_source, language),
                    &mut controller.draft.capture_source,
                    &capture_options,
                );

                ui.add_space(8.0);
                ui.checkbox(
                    &mut controller.draft.save_recording,
                    tr(language, "Save audio for reprocessing"),
                );
            }

            ui.add_space(14.0);

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
                        "meeting_source_language",
                        meeting_language_label(&controller.draft.source_language, language),
                        &mut controller.draft.source_language,
                        &source_options,
                    ) && controller.draft.source_language != "auto"
                        && controller.draft.target_language == controller.draft.source_language
                    {
                        controller.draft.target_language =
                            if controller.draft.source_language == "zh" {
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
                        "meeting_setup",
                        &controller.draft.source_language,
                        &mut controller.draft.target_language,
                        language,
                        |code, lang| meeting_language_label(code, lang),
                    );
                });
            });

            ui.add_space(18.0);
            ui.separator();
            ui.add_space(14.0);

            if let Some(error) = validation_error.as_deref() {
                components::danger_alert(ui, error);
                ui.add_space(12.0);
            }

            if components::primary_button_enabled(
                ui,
                tr(
                    language,
                    if controller.draft.import_audio {
                        "Create and process"
                    } else {
                        "Start meeting"
                    },
                ),
                validation_error.is_none(),
            )
            .clicked()
            {
                action = UiAction::CreateAndStart;
            }
        });
    });

    action
}

fn render_detail(
    controller: &mut MeetingController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) -> UiAction {
    let Some(bundle) = controller.bundle.as_ref() else {
        controller.route = MeetingRoute::Library;
        return UiAction::None;
    };
    let meeting = bundle.meeting.clone();
    let mut action = UiAction::None;
    page_header(ui, &meeting.name, language, |ui| {
        if meeting.status == MeetingStatus::Live && controller.is_recording(&meeting.id) {
            if components::danger_button(ui, tr(language, "End meeting")).clicked() {
                action = UiAction::End;
            }
            if components::animated_button(ui, tr(language, "Pause")).clicked() {
                action = UiAction::Pause;
            }
        } else if can_continue(&meeting)
            && components::primary_button(ui, tr(language, "Continue recording")).clicked()
        {
            action = UiAction::Continue(meeting.id.clone());
        }
        if components::animated_button(ui, tr(language, "Meetings")).clicked() {
            action = UiAction::Back;
        }
    });

    ui.horizontal(|ui| {
        components::status_badge(
            ui,
            meeting_status_label(meeting.status),
            meeting.status == MeetingStatus::Live,
            meeting.status == MeetingStatus::Failed,
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(format!(
                "{} → {}",
                meeting_language_label(&meeting.source_language, language),
                meeting_language_label(&meeting.target_language, language)
            ))
            .color(crate::ui::theme::text_weak())
            .size(12.0),
        );
        if meeting.can_reprocess {
            ui.add_space(8.0);
            Frame::new()
                .fill(Color32::from_rgb(240, 253, 244))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(tr(language, "Audio available for reprocessing"))
                            .color(Color32::from_rgb(22, 101, 52))
                            .size(11.0),
                    );
                });
        }
    });

    ui.add_space(10.0);

    // Segmented Tab Switcher Bar
    Frame::new()
        .fill(Color32::from_rgb(241, 245, 249))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let tabs = [
                    (MeetingPane::Timeline, "Timeline"),
                    (MeetingPane::Minutes, "Minutes"),
                    (MeetingPane::Transcript, "Full transcript"),
                ];
                for (pane, key) in tabs {
                    let selected = controller.pane == pane;
                    let bg = if selected {
                        Color32::WHITE
                    } else {
                        Color32::TRANSPARENT
                    };
                    let stroke = if selected {
                        Stroke::new(1.0, Color32::from_rgb(226, 232, 240))
                    } else {
                        Stroke::NONE
                    };
                    let fg = if selected {
                        Color32::from_rgb(37, 99, 235)
                    } else {
                        crate::ui::theme::text_weak()
                    };

                    let mut text = egui::RichText::new(tr(language, key))
                        .color(fg)
                        .size(12.5);
                    if selected {
                        text = text.strong();
                    }

                    Frame::new()
                        .fill(bg)
                        .stroke(stroke)
                        .corner_radius(CornerRadius::same(7))
                        .inner_margin(Margin::symmetric(14, 6))
                        .show(ui, |ui| {
                            let resp = ui.selectable_label(false, text);
                            if resp.clicked() {
                                controller.pane = pane;
                            }
                        });
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if components::animated_button(ui, tr(language, "Export Markdown")).clicked() {
                        action = UiAction::Export;
                    }
                    if meeting.can_reprocess
                        && components::animated_button(ui, tr(language, "Reprocess audio"))
                            .clicked()
                    {
                        action = UiAction::Reprocess;
                    }
                });
            });
        });

    ui.add_space(12.0);

    match controller.pane {
        MeetingPane::Timeline => render_timeline(controller, language, &mut action, ui),
        MeetingPane::Minutes => render_minutes(controller, language, &mut action, ui),
        MeetingPane::Transcript => render_transcript(controller, language, ui),
    }
    action
}

fn render_timeline(
    controller: &mut MeetingController,
    language: crate::i18n::UiLanguage,
    action: &mut UiAction,
    ui: &mut egui::Ui,
) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut controller.search)
                .hint_text(tr(language, "Search this meeting"))
                .desired_width(240.0),
        );
        ui.add_space(10.0);
        ui.add(
            egui::TextEdit::singleline(&mut controller.new_topic_title)
                .hint_text(tr(language, "New topic title"))
                .desired_width(200.0),
        );
        if components::animated_button(ui, tr(language, "New topic")).clicked() {
            *action = UiAction::NewTopic;
        }
    });

    ui.add_space(8.0);

    let speakers = controller
        .bundle
        .as_ref()
        .map(|bundle| bundle.speakers.clone())
        .unwrap_or_default();

    if !speakers.is_empty() {
        egui::CollapsingHeader::new(tr(language, "Manage speakers")).show(ui, |ui| {
            for speaker in &speakers {
                ui.push_id(("speaker-editor", &speaker.id), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let name = controller
                            .speaker_name_drafts
                            .entry(speaker.id.clone())
                            .or_insert_with(|| speaker.name.clone());
                        ui.add(egui::TextEdit::singleline(name).desired_width(180.0));
                        if components::animated_button(ui, tr(language, "Rename")).clicked() {
                            *action = UiAction::RenameSpeaker(speaker.id.clone(), name.clone());
                        }
                        if speakers.len() > 1 {
                            let target = controller
                                .speaker_merge_targets
                                .entry(speaker.id.clone())
                                .or_insert_with(|| {
                                    speakers
                                        .iter()
                                        .find(|other| other.id != speaker.id)
                                        .map(|other| other.id.clone())
                                        .unwrap_or_default()
                                });

                            let merge_options: Vec<_> = speakers
                                .iter()
                                .filter(|other| other.id != speaker.id)
                                .map(|other| (other.id.clone(), other.name.clone()))
                                .collect();

                            let current_target_name = speakers
                                .iter()
                                .find(|other| other.id == *target)
                                .map(|other| other.name.as_str())
                                .unwrap_or_else(|| tr(language, "Merge into…"));

                            components::searchable_combobox(
                                ui,
                                ("merge-target", &speaker.id),
                                current_target_name,
                                target,
                                &merge_options,
                            );

                            if components::danger_button(ui, tr(language, "Merge")).clicked()
                                && !target.is_empty()
                            {
                                *action =
                                    UiAction::MergeSpeaker(speaker.id.clone(), target.clone());
                            }
                        }
                    });
                });
            }
            ui.label(
                egui::RichText::new(tr(
                    language,
                    "Automatic speaker labels are provisional. Renaming confirms an identity; merging redirects all linked voice clusters.",
                ))
                .size(11.0)
                .color(crate::ui::theme::text_weak()),
            );
        });
        ui.add_space(8.0);
    }

    let Some(bundle) = controller.bundle.as_mut() else {
        return;
    };
    let evidence_target = controller.evidence_target.clone();
    let mut evidence_reached = false;
    let query = controller.search.to_lowercase();
    egui::ScrollArea::vertical()
        .id_salt("meeting_timeline_scroll")
        .stick_to_bottom(query.is_empty())
        .show(ui, |ui| {
            for topic in &bundle.topics {
                let topic_segments = bundle
                    .segments
                    .iter()
                    .filter(|segment| segment.topic_id == topic.id)
                    .collect::<Vec<_>>();
                let visible = query.is_empty()
                    || topic
                        .title
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
                    || topic_segments
                        .iter()
                        .any(|segment| segment_matches(segment, &query));
                if !visible {
                    continue;
                }
                ui.push_id(("topic", &topic.id), |ui| {
                    components::section(
                        ui,
                        topic.title.as_deref().unwrap_or("Untitled topic"),
                        |ui| {
                            if topic_segments.is_empty() {
                                ui.label(
                                    egui::RichText::new("No conversation in this topic yet")
                                        .italics()
                                        .color(crate::ui::theme::text_weak()),
                                );
                            }
                            for segment in topic_segments {
                                if !query.is_empty() && !segment_matches(segment, &query) {
                                    continue;
                                }
                                render_segment(
                                    segment,
                                    &bundle.speakers,
                                    &bundle.markers,
                                    evidence_target.as_deref(),
                                    &mut evidence_reached,
                                    language,
                                    action,
                                    ui,
                                );
                                ui.add_space(6.0);
                            }
                        },
                    );
                    ui.add_space(9.0);
                });
            }
        });
    if evidence_reached {
        controller.evidence_target = None;
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut controller.quick_note)
                .hint_text(tr(language, "Quick note linked to the latest message"))
                .desired_width(f32::INFINITY),
        );
        if components::primary_button(ui, tr(language, "Add note")).clicked() {
            *action = UiAction::QuickNote;
        }
    });
}

fn render_segment(
    segment: &Segment,
    speakers: &[rust_client::Speaker],
    markers: &[SegmentMarker],
    evidence_target: Option<&str>,
    evidence_reached: &mut bool,
    language: crate::i18n::UiLanguage,
    action: &mut UiAction,
    ui: &mut egui::Ui,
) {
    let frame = Frame::new()
        .fill(Color32::from_rgb(250, 252, 255))
        .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let speaker = segment
                        .canonical_speaker_id
                        .as_deref()
                        .and_then(|id| speakers.iter().find(|speaker| speaker.id == id));

                    let speaker_name = speaker
                        .map(|s| s.name.as_str())
                        .unwrap_or_else(|| tr(language, "Unknown speaker"));

                    components::speaker_badge(ui, speaker_name);

                    if segment.speaker_token.is_some() && speaker.is_none() {
                        ui.label(
                            egui::RichText::new(tr(language, "automatic cluster"))
                                .size(11.0)
                                .color(crate::ui::theme::text_weak()),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format_duration(segment.start_ms))
                                .size(11.0)
                                .color(crate::ui::theme::text_weak())
                                .monospace(),
                        );
                    });
                });

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(&segment.original_text)
                        .size(13.0)
                        .color(Color32::from_rgb(71, 85, 105)),
                );

                if let Some(translated) = &segment.translated_text {
                    ui.add_space(4.0);
                    Frame::new()
                        .fill(Color32::from_rgb(241, 245, 249))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(8, 5))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(translated)
                                    .strong()
                                    .size(13.5)
                                    .color(crate::ui::theme::text_strong()),
                            );
                        });
                }

                if !segment.is_final {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(tr(language, "Updating…"))
                            .italics()
                            .size(11.5)
                            .color(Color32::from_rgb(37, 99, 235)),
                    );
                }

                ui.add_space(6.0);

                // Quick Marker Pill Buttons
                ui.horizontal_wrapped(|ui| {
                    if tag_button(
                        ui,
                        tr(language, "Key decision"),
                        Color32::from_rgb(254, 243, 199),
                        Color32::from_rgb(180, 83, 9),
                    )
                    .clicked()
                    {
                        *action = UiAction::AddMarker(segment.id.clone(), MarkerKind::KeyDecision);
                    }
                    if tag_button(
                        ui,
                        tr(language, "Action item"),
                        Color32::from_rgb(209, 250, 229),
                        Color32::from_rgb(4, 120, 87),
                    )
                    .clicked()
                    {
                        *action = UiAction::AddMarker(segment.id.clone(), MarkerKind::ActionItem);
                    }
                    if tag_button(
                        ui,
                        tr(language, "Note"),
                        Color32::from_rgb(224, 231, 255),
                        Color32::from_rgb(67, 56, 202),
                    )
                    .clicked()
                    {
                        *action = UiAction::AddMarker(segment.id.clone(), MarkerKind::Note);
                    }
                });

                let attached_markers: Vec<_> = markers
                    .iter()
                    .filter(|marker| marker.segment_id == segment.id)
                    .collect();

                if !attached_markers.is_empty() {
                    ui.add_space(4.0);
                    for marker in attached_markers {
                        Frame::new()
                            .fill(Color32::from_rgb(255, 255, 255))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(marker_label(marker.kind, language))
                                            .size(11.5)
                                            .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(&marker.text)
                                            .size(12.0)
                                            .color(crate::ui::theme::text_strong()),
                                    );
                                });
                            });
                    }
                }
            });
        });

    if evidence_target == Some(segment.id.as_str()) {
        ui.scroll_to_rect(frame.response.rect, Some(egui::Align::Center));
        *evidence_reached = true;
    }
}

fn tag_button(ui: &mut egui::Ui, text: &str, bg: Color32, fg: Color32) -> egui::Response {
    Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).color(fg).size(11.0).strong())
        })
        .response
        .interact(egui::Sense::click())
}

fn render_minutes(
    controller: &mut MeetingController,
    language: crate::i18n::UiLanguage,
    action: &mut UiAction,
    ui: &mut egui::Ui,
) {
    ui.label(
        egui::RichText::new(tr(
            language,
            "Editable Markdown minutes. Nothing is generated automatically.",
        ))
        .color(crate::ui::theme::text_weak())
        .size(12.0),
    );
    ui.add_space(6.0);

    let minutes_response = ui.add(
        egui::TextEdit::multiline(&mut controller.minutes_draft)
            .desired_rows(18)
            .desired_width(f32::INFINITY),
    );
    if minutes_response.changed() {
        controller.minutes_dirty = true;
    }
    ui.add_space(8.0);

    if components::primary_button(ui, tr(language, "Save minutes")).clicked() {
        *action = UiAction::SaveMinutes;
    }
    ui.add_space(14.0);

    if let Some(bundle) = controller.bundle.as_mut() {
        components::section(ui, tr(language, "User markers"), |ui| {
            for marker in &mut bundle.markers {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(marker_label(marker.kind, language))
                            .strong()
                            .size(12.0),
                    );
                    let response =
                        ui.add(egui::TextEdit::singleline(&mut marker.text).desired_width(420.0));
                    if response.lost_focus() && response.changed() {
                        *action = UiAction::SaveMarker(marker.clone());
                    }
                    let timestamp = bundle
                        .segments
                        .iter()
                        .find(|segment| segment.id == marker.segment_id)
                        .map(|segment| format_duration(segment.start_ms))
                        .unwrap_or_else(|| "Evidence".into());
                    if ui.small_button(timestamp).clicked() {
                        *action = UiAction::JumpToEvidence(marker.segment_id.clone());
                    }
                });
            }
        });
    }
}

fn render_transcript(
    controller: &mut MeetingController,
    _language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) {
    if let Some(bundle) = controller.bundle.as_ref() {
        egui::ScrollArea::vertical()
            .id_salt("meeting_full_transcript")
            .show(ui, |ui| {
                for segment in &bundle.segments {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format_duration(segment.start_ms))
                                .monospace()
                                .size(11.0)
                                .color(crate::ui::theme::text_weak()),
                        );
                        ui.label(
                            egui::RichText::new(&segment.original_text)
                                .size(12.5)
                                .color(crate::ui::theme::text_normal()),
                        );
                        if let Some(text) = &segment.translated_text {
                            ui.label(
                                egui::RichText::new(text)
                                    .strong()
                                    .size(13.0)
                                    .color(crate::ui::theme::text_strong()),
                            );
                        }
                    });
                    ui.separator();
                }
            });
    }
}

fn apply_action(
    controller: &mut MeetingController,
    action: UiAction,
    snapshot: &MeetingUiSnapshot,
) -> MeetingAction {
    match action {
        UiAction::None => {}
        UiAction::NewLive => {
            controller.reset_draft(false, snapshot);
            controller.route = MeetingRoute::Create;
        }
        UiAction::NewImport => {
            controller.reset_draft(true, snapshot);
            controller.route = MeetingRoute::Create;
        }
        UiAction::Back => {
            controller.refresh_library();
            controller.route = MeetingRoute::Library;
        }
        UiAction::Open(id) => controller.open_meeting(&id),
        UiAction::CreateAndStart => {
            if let Some(error) = draft_validation_error(controller, snapshot) {
                controller.error = Some(error);
                return MeetingAction::None;
            }
            return MeetingAction::CreateAndStart(start_request(controller));
        }
        UiAction::Continue(id) => {
            if controller
                .active_meeting_id()
                .is_some_and(|active_id| active_id != id)
            {
                controller.error =
                    Some("Finish the active meeting before continuing a different meeting".into());
                return MeetingAction::None;
            }
            return MeetingAction::Continue(id);
        }
        UiAction::Pause => return MeetingAction::Pause,
        UiAction::End => return MeetingAction::End,
        UiAction::NewTopic => controller.create_topic(),
        UiAction::AddMarker(id, kind) => controller.add_marker(&id, kind),
        UiAction::SaveMarker(marker) => controller.save_marker(&marker),
        UiAction::QuickNote => controller.add_quick_note(),
        UiAction::SaveMinutes => controller.save_minutes(),
        UiAction::JumpToEvidence(segment_id) => {
            controller.pane = MeetingPane::Timeline;
            controller.evidence_target = Some(segment_id);
        }
        UiAction::AskDelete(id) => controller.pending_delete = Some(id),
        UiAction::Delete(id) => controller.delete(&id),
        UiAction::CancelDelete => controller.pending_delete = None,
        UiAction::Export => {
            if let Some(meeting_id) = controller
                .bundle
                .as_ref()
                .map(|bundle| bundle.meeting.id.clone())
            {
                return MeetingAction::Export(meeting_id);
            }
        }
        UiAction::ExportMeeting(meeting_id) => return MeetingAction::Export(meeting_id),
        UiAction::Reprocess => {
            let meeting_id = controller
                .bundle
                .as_ref()
                .map(|bundle| bundle.meeting.id.clone());
            let path = controller.reprocessable_audio_path();
            if let (Some(meeting_id), Some(path)) = (meeting_id, path) {
                if controller.active_meeting_id().is_some() {
                    controller.error = Some(
                        "Wait for the active meeting to finish before reprocessing audio".into(),
                    );
                    return MeetingAction::None;
                }
                if !path.is_file() {
                    controller.error =
                        Some("The retained audio file is no longer available".into());
                    return MeetingAction::None;
                }
                return MeetingAction::Reprocess(MeetingReprocessRequest {
                    meeting_id,
                    audio_path: path,
                    topic_title: "Reprocessed audio".into(),
                });
            } else {
                controller.error = Some("The retained audio file is no longer available".into());
            }
        }
        UiAction::RenameSpeaker(id, name) => controller.rename_speaker(&id, &name),
        UiAction::MergeSpeaker(source, target) => controller.merge_speakers(&source, &target),
    }
    MeetingAction::None
}

fn start_request(controller: &MeetingController) -> MeetingStartRequest {
    let draft = &controller.draft;
    let input = if draft.import_audio {
        MeetingInputRequest::ImportedAudio {
            path: draft.import_path.trim().into(),
        }
    } else {
        MeetingInputRequest::Live {
            source: draft.capture_source,
            save_recording: draft.save_recording,
        }
    };
    MeetingStartRequest {
        name: draft.name.trim().into(),
        source_language: draft.source_language.clone(),
        target_language: draft.target_language.clone(),
        input,
    }
}

fn draft_validation_error(
    controller: &MeetingController,
    snapshot: &MeetingUiSnapshot,
) -> Option<String> {
    if !controller.storage_available() {
        return Some("Meeting storage is unavailable; recording is disabled".into());
    }
    if controller.draft.name.trim().is_empty() {
        return Some("Enter a meeting name".into());
    }
    if controller.active_meeting_id().is_some() {
        return Some("Finish the active meeting before starting another one".into());
    }
    if snapshot.host_session_busy {
        return Some("Stop the current translation session before starting a meeting".into());
    }
    if controller.draft.source_language != "auto"
        && controller.draft.source_language == controller.draft.target_language
    {
        return Some("Spoken language and translation language cannot be the same".into());
    }
    if controller.draft.import_audio {
        let value = controller.draft.import_path.trim();
        if value.is_empty() {
            return Some("Choose an audio file".into());
        }
        if !std::path::Path::new(value).is_file() {
            return Some("The selected audio file is not available".into());
        }
    }
    None
}

fn segment_matches(segment: &Segment, query: &str) -> bool {
    segment.original_text.to_lowercase().contains(query)
        || segment
            .translated_text
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(query)
}

fn marker_label(kind: MarkerKind, language: crate::i18n::UiLanguage) -> &'static str {
    match kind {
        MarkerKind::KeyDecision => tr(language, "Key decision"),
        MarkerKind::ActionItem => tr(language, "Action item"),
        MarkerKind::Note => tr(language, "Note"),
    }
}

fn source_label(meeting: &Meeting, language: crate::i18n::UiLanguage) -> &'static str {
    match meeting.source_kind {
        MeetingSourceKind::LiveCapture => tr(language, "Live"),
        MeetingSourceKind::ImportedAudio => tr(language, "Imported audio"),
    }
}

fn meeting_language_label(code: &str, language: crate::i18n::UiLanguage) -> String {
    if code == "auto" {
        return tr(language, "Auto (bidirectional)").to_string();
    }
    if code.contains(',') {
        let parts: Vec<_> = code
            .split(',')
            .map(|part| single_language_label(part.trim(), language))
            .collect();
        return parts.join(" + ");
    }
    single_language_label(code, language)
}

fn single_language_label(code: &str, language: crate::i18n::UiLanguage) -> String {
    let english_name = crate::LANGUAGE_OPTIONS
        .iter()
        .find_map(|(value, label)| (*value == code).then_some(*label));
    if let Some(name) = english_name {
        tr(language, name).to_string()
    } else {
        code.to_string()
    }
}

fn capture_label(source: MeetingAudioSource, language: crate::i18n::UiLanguage) -> &'static str {
    match source {
        MeetingAudioSource::Microphone => tr(language, "Microphone"),
        MeetingAudioSource::SystemAudio => tr(language, "System audio"),
        MeetingAudioSource::Both => tr(language, "Microphone + system"),
    }
}

fn format_duration(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn format_timestamp(ms: i64) -> String {
    if ms <= 0 {
        return "-".to_string();
    }
    let secs = ms / 1000;
    let days = (secs / 86400) as i64;
    let daytime = (secs % 86400) as i64;
    let hours = daytime / 3600;
    let minutes = (daytime % 3600) / 60;

    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {hours:02}:{minutes:02}")
}
