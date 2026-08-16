use super::super::{dark_pill_button, format_time_ms};
use crate::plugins::player::{
    backend::PlaybackStatus, controller::VideoPlayerController, i18n::tr,
};
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Stroke};

pub(super) fn render_audio_card(
    controller: &mut VideoPlayerController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) {
    let is_playing =
        controller.current_source.is_some() && controller.get_status() == PlaybackStatus::Playing;

    Frame::new()
        .fill(Color32::from_rgb(15, 23, 42))
        .stroke(Stroke::new(1.0, Color32::from_rgb(51, 65, 85)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal(|ui| {
                // Audio Icon / Visualizer Symbol
                Frame::new()
                    .fill(Color32::from_rgb(30, 41, 59))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        let symbol = if is_playing { "🎵" } else { "🎧" };
                        ui.label(egui::RichText::new(symbol).size(26.0));
                    });

                ui.add_space(12.0);

                ui.vertical(|ui| {
                    let title = controller
                        .current_source
                        .as_ref()
                        .map(|s| s.display_title())
                        .unwrap_or_else(|| tr(language, "Audio Playback").to_string());

                    ui.label(
                        egui::RichText::new(title)
                            .size(16.0)
                            .strong()
                            .color(Color32::from_rgb(241, 245, 249)),
                    );
                    ui.add_space(3.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "{} / {}",
                            format_time_ms(controller.get_time_ms()),
                            format_time_ms(controller.get_duration_ms())
                        ))
                        .monospace()
                        .size(12.0)
                        .color(Color32::from_rgb(148, 163, 184)),
                    );
                });
            });

            ui.add_space(14.0);

            // Controls & Seek Bar
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                // ── Play / Pause ──
                let play_icon = if is_playing { "⏸" } else { "⏵" };
                let play_enabled = controller.current_source.is_some();
                if dark_pill_button(ui, play_icon, play_enabled, true) {
                    controller.toggle_play();
                }

                // ── Seek slider ──
                let mut current_sec = (controller.get_time_ms() / 1000) as f32;
                let max_sec = (controller.get_duration_ms().max(1) / 1000) as f32;
                let right_w = 160.0;
                let slider_w = (ui.available_width() - right_w).max(40.0);
                let slider = egui::Slider::new(&mut current_sec, 0.0..=max_sec).show_value(false);
                if ui.add_sized([slider_w, 16.0], slider).changed() {
                    if let Some(backend) = &mut controller.backend {
                        backend.seek((current_sec * 1000.0) as i64);
                    }
                }

                // ── Volume ──
                let mut vol = controller.volume;
                let vol_slider = egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false);
                if ui.add_sized([40.0, 16.0], vol_slider).changed() {
                    controller.set_volume(vol);
                }

                // ── Mute ──
                let mute_icon = if controller.muted { "🔇" } else { "🔊" };
                if dark_pill_button(ui, mute_icon, true, false) {
                    controller.toggle_mute();
                }
            });
        });
}

pub(super) fn render_viewport_card(
    controller: &mut VideoPlayerController,
    language: crate::i18n::UiLanguage,
    ui: &mut egui::Ui,
) {
    let total_player_height = if controller.fullscreen_mode {
        ui.available_height()
    } else {
        400.0
    };

    let has_video_source = controller.current_source.is_some() && !controller.is_audio_only_task();
    let is_playing = controller.get_status() == PlaybackStatus::Playing;
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
            .request_repaint_after(std::time::Duration::from_millis(
                remaining_ms.max(20).min(100),
            ));
    }

    // Hide mouse cursor in fullscreen when controls have faded out
    if controller.fullscreen_mode && is_playing && !is_paused && controls_alpha < 0.05 {
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
    }

    // Layout constants for the compact floating pill
    let bar_height = 40.0;
    let bar_margin_h = 16.0;
    let bar_margin_bottom = if controller.fullscreen_mode {
        16.0
    } else {
        6.0
    };
    let hwnd_shrink = (bar_height + bar_margin_bottom + 2.0) * controls_alpha;

    let (corner_radius, stroke_style) = if controller.fullscreen_mode {
        (CornerRadius::ZERO, Stroke::NONE)
    } else {
        (
            CornerRadius::same(12),
            Stroke::new(1.0, Color32::from_rgb(30, 41, 59)),
        )
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

            if has_video_source {
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

    if has_video_source && !controller.fullscreen_mode {
        let diag = controller.get_diagnostics();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "HWDEC: {} | Codec: {} | {}x{} | {:.1} FPS | Dropped: {}",
                    if diag.hwdec_current.is_empty() {
                        "auto"
                    } else {
                        &diag.hwdec_current
                    },
                    if diag.video_codec.is_empty() {
                        "-"
                    } else {
                        &diag.video_codec
                    },
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
