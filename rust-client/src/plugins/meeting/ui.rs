use super::{
    MeetingAction, MeetingAudioSource, MeetingInputRequest, MeetingPlugin, MeetingReprocessRequest,
    MeetingStartRequest, MeetingUiSnapshot,
    controller::{
        MeetingController, MeetingPane, MeetingRoute, can_continue, meeting_status_label,
    },
};
use crate::ui::components;
use eframe::egui;
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
        MeetingRoute::Library => render_library(&mut plugin.controller, ui),
        MeetingRoute::Create => render_setup(&mut plugin.controller, snapshot, ui),
        MeetingRoute::Detail => render_detail(&mut plugin.controller, ui),
    };
    apply_action(&mut plugin.controller, action, snapshot)
}

fn page_header(ui: &mut egui::Ui, title: &str, right: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(22.0)
                .color(crate::ui::theme::text_strong())
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
    ui.add_space(14.0);
}

fn render_library(controller: &mut MeetingController, ui: &mut egui::Ui) -> UiAction {
    let mut action = UiAction::None;
    page_header(ui, "Meeting records", |ui| {
        if components::primary_button(ui, "New meeting").clicked() {
            action = UiAction::NewLive;
        }
        if components::animated_button(ui, "Import audio").clicked() {
            action = UiAction::NewImport;
        }
    });
    if let Some(error) = &controller.error {
        ui.colored_label(egui::Color32::from_rgb(220, 38, 38), error);
        ui.add_space(8.0);
    }
    ui.add(egui::TextEdit::singleline(&mut controller.search).hint_text("Search meetings"));
    ui.add_space(10.0);
    if controller.meetings.is_empty() {
        components::card(ui, |ui| {
            ui.heading("No meetings yet");
            ui.label("Create a live record or import an audio file. Records are stored locally.");
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
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(&meeting.name).size(15.0).strong());
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} · {} → {} · {}",
                                        source_label(meeting),
                                        meeting.source_language,
                                        meeting.target_language,
                                        format_timestamp(meeting.last_activity_at_ms)
                                    ))
                                    .color(crate::ui::theme::text_weak())
                                    .size(11.5),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let status = meeting_status_label(meeting.status);
                                    components::status_badge(
                                        ui,
                                        status,
                                        meeting.status == MeetingStatus::Live,
                                        meeting.status == MeetingStatus::Failed,
                                    );
                                },
                            );
                        });
                        ui.add_space(8.0);
                        ui.horizontal_wrapped(|ui| {
                            if components::animated_button(ui, "Open").clicked() {
                                action = UiAction::Open(meeting.id.clone());
                            }
                            if can_continue(meeting)
                                && components::primary_button(ui, "Continue recording").clicked()
                            {
                                action = UiAction::Continue(meeting.id.clone());
                            }
                            if components::animated_button(ui, "Export Markdown").clicked() {
                                action = UiAction::ExportMeeting(meeting.id.clone());
                            }
                            let active = controller.is_recording(&meeting.id);
                            let delete = ui
                                .add_enabled_ui(!active, |ui| {
                                    components::danger_button(ui, "Delete")
                                })
                                .inner;
                            if active {
                                delete
                                    .on_hover_text("Finish the active meeting before deleting it");
                            } else if delete.clicked() {
                                action = UiAction::AskDelete(meeting.id.clone());
                            }
                        });
                        if controller.pending_delete.as_deref() == Some(&meeting.id) {
                            ui.separator();
                            ui.label("Delete this meeting and all of its local records?");
                            ui.horizontal(|ui| {
                                if components::danger_button(ui, "Delete permanently").clicked() {
                                    action = UiAction::Delete(meeting.id.clone());
                                }
                                if components::animated_button(ui, "Cancel").clicked() {
                                    action = UiAction::CancelDelete;
                                }
                            });
                        }
                    });
                    ui.add_space(8.0);
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
    let mut action = UiAction::None;
    page_header(
        ui,
        if controller.draft.import_audio {
            "Import audio"
        } else {
            "New live meeting"
        },
        |ui| {
            if components::animated_button(ui, "Back").clicked() {
                action = UiAction::Back;
            }
        },
    );
    components::section(ui, "Meeting", |ui| {
        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut controller.draft.name).desired_width(f32::INFINITY));
        ui.add_space(8.0);
        if controller.draft.import_audio {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut controller.draft.import_path)
                        .hint_text("Audio file")
                        .desired_width(420.0),
                );
                if components::animated_button(ui, "Choose file").clicked()
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
            ui.label(
                egui::RichText::new(
                    "This meeting references the source file. Moving or deleting it prevents reprocessing.",
                )
                    .color(crate::ui::theme::text_weak())
                    .size(11.5),
            );
        } else {
            ui.label("Audio source");
            egui::ComboBox::from_id_salt("meeting_capture_source")
                .selected_text(capture_label(controller.draft.capture_source))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut controller.draft.capture_source,
                        MeetingAudioSource::Microphone,
                        "Microphone",
                    );
                    ui.selectable_value(
                        &mut controller.draft.capture_source,
                        MeetingAudioSource::SystemAudio,
                        "System audio",
                    );
                    ui.selectable_value(
                        &mut controller.draft.capture_source,
                        MeetingAudioSource::Both,
                        "Microphone + system",
                    );
                });
            ui.checkbox(
                &mut controller.draft.save_recording,
                "Save audio for reprocessing",
            );
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Spoken language");
                egui::ComboBox::from_id_salt("meeting_source_language")
                    .selected_text(meeting_language_label(&controller.draft.source_language))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut controller.draft.source_language,
                            "auto".to_owned(),
                            "Auto (bidirectional)",
                        );
                        for (code, label) in MEETING_LANGUAGES {
                            ui.selectable_value(
                                &mut controller.draft.source_language,
                                (*code).to_owned(),
                                *label,
                            );
                        }
                    });
            });
            ui.add_space(18.0);
            ui.vertical(|ui| {
                ui.label("Translation language");
                egui::ComboBox::from_id_salt("meeting_target_language")
                    .selected_text(meeting_language_label(&controller.draft.target_language))
                    .show_ui(ui, |ui| {
                        for (code, label) in MEETING_LANGUAGES {
                            ui.selectable_value(
                                &mut controller.draft.target_language,
                                (*code).to_owned(),
                                *label,
                            );
                        }
                    });
            });
        });
    });
    ui.add_space(12.0);
    let validation_error = draft_validation_error(controller, snapshot);
    if let Some(error) = validation_error.as_deref() {
        ui.colored_label(egui::Color32::from_rgb(185, 28, 28), error);
        ui.add_space(6.0);
    }
    if components::primary_button_enabled(
        ui,
        if controller.draft.import_audio {
            "Create and process"
        } else {
            "Start meeting"
        },
        validation_error.is_none(),
    )
    .clicked()
    {
        action = UiAction::CreateAndStart;
    }
    action
}

fn render_detail(controller: &mut MeetingController, ui: &mut egui::Ui) -> UiAction {
    let Some(bundle) = controller.bundle.as_ref() else {
        controller.route = MeetingRoute::Library;
        return UiAction::None;
    };
    let meeting = bundle.meeting.clone();
    let mut action = UiAction::None;
    page_header(ui, &meeting.name, |ui| {
        if meeting.status == MeetingStatus::Live && controller.is_recording(&meeting.id) {
            if components::danger_button(ui, "End meeting").clicked() {
                action = UiAction::End;
            }
            if components::animated_button(ui, "Pause").clicked() {
                action = UiAction::Pause;
            }
        } else if can_continue(&meeting)
            && components::primary_button(ui, "Continue recording").clicked()
        {
            action = UiAction::Continue(meeting.id.clone());
        }
        if components::animated_button(ui, "Meetings").clicked() {
            action = UiAction::Back;
        }
    });
    ui.horizontal_wrapped(|ui| {
        components::status_badge(
            ui,
            meeting_status_label(meeting.status),
            meeting.status == MeetingStatus::Live,
            meeting.status == MeetingStatus::Failed,
        );
        ui.label(
            egui::RichText::new(format!(
                "{} → {}",
                meeting.source_language, meeting.target_language
            ))
            .color(crate::ui::theme::text_weak()),
        );
        if meeting.can_reprocess {
            ui.label("Audio available for reprocessing");
        }
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.selectable_value(&mut controller.pane, MeetingPane::Timeline, "Timeline");
        ui.selectable_value(&mut controller.pane, MeetingPane::Minutes, "Minutes");
        ui.selectable_value(
            &mut controller.pane,
            MeetingPane::Transcript,
            "Full transcript",
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if components::animated_button(ui, "Export Markdown").clicked() {
                action = UiAction::Export;
            }
            if meeting.can_reprocess && components::animated_button(ui, "Reprocess audio").clicked()
            {
                action = UiAction::Reprocess;
            }
        });
    });
    ui.separator();
    ui.add_space(8.0);
    match controller.pane {
        MeetingPane::Timeline => render_timeline(controller, &mut action, ui),
        MeetingPane::Minutes => render_minutes(controller, &mut action, ui),
        MeetingPane::Transcript => render_transcript(controller, ui),
    }
    action
}

fn render_timeline(controller: &mut MeetingController, action: &mut UiAction, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut controller.search)
                .hint_text("Search this meeting")
                .desired_width(260.0),
        );
        ui.add(
            egui::TextEdit::singleline(&mut controller.new_topic_title)
                .hint_text("New topic title")
                .desired_width(200.0),
        );
        if components::animated_button(ui, "New topic").clicked() {
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
        egui::CollapsingHeader::new("Manage speakers").show(ui, |ui| {
            for speaker in &speakers {
                ui.push_id(("speaker-editor", &speaker.id), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let name = controller
                            .speaker_name_drafts
                            .entry(speaker.id.clone())
                            .or_insert_with(|| speaker.name.clone());
                        ui.add(egui::TextEdit::singleline(name).desired_width(180.0));
                        if components::animated_button(ui, "Rename").clicked() {
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
                            egui::ComboBox::from_id_salt(("merge-target", &speaker.id))
                                .selected_text(
                                    speakers
                                        .iter()
                                        .find(|other| other.id == *target)
                                        .map(|other| other.name.as_str())
                                        .unwrap_or("Merge into…"),
                                )
                                .show_ui(ui, |ui| {
                                    for other in
                                        speakers.iter().filter(|other| other.id != speaker.id)
                                    {
                                        ui.selectable_value(target, other.id.clone(), &other.name);
                                    }
                                });
                            if components::danger_button(ui, "Merge").clicked()
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
                egui::RichText::new("Automatic speaker labels are provisional. Renaming confirms an identity; merging redirects all linked voice clusters.")
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
                .hint_text("Quick note linked to the latest message")
                .desired_width(f32::INFINITY),
        );
        if components::primary_button(ui, "Add note").clicked() {
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
    action: &mut UiAction,
    ui: &mut egui::Ui,
) {
    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgb(248, 250, 252))
        .stroke(egui::Stroke::new(
            1.0,
            egui::Color32::from_rgb(226, 232, 240),
        ))
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let speaker = segment
                    .canonical_speaker_id
                    .as_deref()
                    .and_then(|id| speakers.iter().find(|speaker| speaker.id == id));
                ui.label(
                    egui::RichText::new(
                        speaker
                            .map(|speaker| speaker.name.as_str())
                            .unwrap_or("Unknown speaker"),
                    )
                    .color(egui::Color32::from_rgb(37, 99, 235))
                    .strong(),
                );
                if segment.speaker_token.is_some() && speaker.is_none() {
                    ui.label(
                        egui::RichText::new("automatic cluster")
                            .size(10.5)
                            .color(crate::ui::theme::text_weak()),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format_duration(segment.start_ms))
                            .size(11.0)
                            .color(crate::ui::theme::text_weak()),
                    );
                });
            });
            ui.label(&segment.original_text);
            if let Some(translated) = &segment.translated_text {
                ui.label(
                    egui::RichText::new(translated)
                        .strong()
                        .color(crate::ui::theme::text_strong()),
                );
            }
            if !segment.is_final {
                ui.label(
                    egui::RichText::new("Updating…")
                        .italics()
                        .color(egui::Color32::from_rgb(37, 99, 235)),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("Key decision").clicked() {
                    *action = UiAction::AddMarker(segment.id.clone(), MarkerKind::KeyDecision);
                }
                if ui.small_button("Action item").clicked() {
                    *action = UiAction::AddMarker(segment.id.clone(), MarkerKind::ActionItem);
                }
                if ui.small_button("Note").clicked() {
                    *action = UiAction::AddMarker(segment.id.clone(), MarkerKind::Note);
                }
            });
            for marker in markers
                .iter()
                .filter(|marker| marker.segment_id == segment.id)
            {
                ui.label(format!("{}: {}", marker_label(marker.kind), marker.text));
            }
        });
    if evidence_target == Some(segment.id.as_str()) {
        ui.scroll_to_rect(frame.response.rect, Some(egui::Align::Center));
        *evidence_reached = true;
    }
}

fn render_minutes(controller: &mut MeetingController, action: &mut UiAction, ui: &mut egui::Ui) {
    ui.label("Editable Markdown minutes. Nothing is generated automatically.");
    let minutes_response = ui.add(
        egui::TextEdit::multiline(&mut controller.minutes_draft)
            .desired_rows(18)
            .desired_width(f32::INFINITY),
    );
    if minutes_response.changed() {
        controller.minutes_dirty = true;
    }
    if components::primary_button(ui, "Save minutes").clicked() {
        *action = UiAction::SaveMinutes;
    }
    ui.add_space(12.0);
    if let Some(bundle) = controller.bundle.as_mut() {
        components::section(ui, "User markers", |ui| {
            for marker in &mut bundle.markers {
                ui.horizontal(|ui| {
                    ui.label(marker_label(marker.kind));
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

fn render_transcript(controller: &mut MeetingController, ui: &mut egui::Ui) {
    if let Some(bundle) = controller.bundle.as_ref() {
        egui::ScrollArea::vertical()
            .id_salt("meeting_full_transcript")
            .show(ui, |ui| {
                for segment in &bundle.segments {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format_duration(segment.start_ms))
                                .monospace()
                                .color(crate::ui::theme::text_weak()),
                        );
                        ui.label(&segment.original_text);
                        if let Some(text) = &segment.translated_text {
                            ui.label(egui::RichText::new(text).strong());
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
fn marker_label(kind: MarkerKind) -> &'static str {
    match kind {
        MarkerKind::KeyDecision => "Key decision",
        MarkerKind::ActionItem => "Action item",
        MarkerKind::Note => "Note",
    }
}
fn source_label(meeting: &Meeting) -> &'static str {
    match meeting.source_kind {
        MeetingSourceKind::LiveCapture => "Live",
        MeetingSourceKind::ImportedAudio => "Imported audio",
    }
}

const MEETING_LANGUAGES: &[(&str, &str)] = &[
    ("zh", "Chinese"),
    ("en", "English"),
    ("fr", "French"),
    ("pt", "Portuguese"),
    ("es", "Spanish"),
    ("ja", "Japanese"),
    ("ru", "Russian"),
    ("ko", "Korean"),
    ("th", "Thai"),
    ("it", "Italian"),
    ("de", "German"),
    ("vi", "Vietnamese"),
    ("id", "Indonesian"),
    ("pl", "Polish"),
    ("cs", "Czech"),
    ("nl", "Dutch"),
];

fn meeting_language_label(code: &str) -> &'static str {
    if code == "auto" {
        return "Auto (bidirectional)";
    }
    MEETING_LANGUAGES
        .iter()
        .find_map(|(value, label)| (*value == code).then_some(*label))
        .unwrap_or("Unknown language")
}
fn capture_label(source: MeetingAudioSource) -> &'static str {
    match source {
        MeetingAudioSource::Microphone => "Microphone",
        MeetingAudioSource::SystemAudio => "System audio",
        MeetingAudioSource::Both => "Microphone + system audio",
    }
}
fn format_duration(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
fn format_timestamp(ms: i64) -> String {
    format!("{}", ms / 1000)
}
