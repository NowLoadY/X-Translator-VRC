use super::format_time_ms;
use crate::plugins::player::{controller::VideoPlayerController, i18n::tr};
use crate::ui::components;
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Stroke};

pub(super) fn render_subtitles_card(
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

            let is_manually_scrolling = controller.last_manual_scroll.map_or(false, |instant| {
                instant.elapsed() < std::time::Duration::from_secs(5)
            });
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
                                egui::RichText::new(tr(
                                    language,
                                    "No subtitle at current timestamp",
                                ))
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
                if auto_scroll_active
                    && active_idx.is_some()
                    && active_idx != controller.last_auto_scrolled_idx
                {
                    if let Some(idx) = active_idx {
                        let center_offset = (viewport_height - row_height) * 0.5;
                        let target_offset = ((idx as f32 * row_height) - center_offset).max(0.0);
                        scroll_area = scroll_area.vertical_scroll_offset(target_offset);
                        controller.last_auto_scrolled_idx = Some(idx);
                    }
                }

                let scroll_output =
                    scroll_area.show_rows(ui, row_height, total_rows, |ui, row_range| {
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
                                                    format_time_ms(
                                                        cue.end_ms.max(cue.start_ms + 2000)
                                                    )
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
                            || i.raw
                                .events
                                .iter()
                                .any(|e| matches!(e, egui::Event::MouseWheel { .. }))
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
