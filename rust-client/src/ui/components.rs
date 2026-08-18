use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Stroke, Ui, Vec2};

pub fn card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(Color32::WHITE)
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::same(18))
        .stroke(Stroke::new(1.0, Color32::from_rgb(235, 240, 248)))
        .shadow(egui::Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: Color32::from_rgba_unmultiplied(15, 23, 42, 10),
        })
        .show(ui, add_contents)
        .inner
}

pub fn action_card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(Color32::from_rgb(245, 248, 252))
        .corner_radius(CornerRadius::same(18))
        .stroke(Stroke::NONE)
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, add_contents)
        .inner
}

pub fn history_entry_card<R>(
    ui: &mut Ui,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::Response {
    Frame::new()
        .fill(Color32::from_rgb(245, 248, 252))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(12, 9))
        .stroke(Stroke::NONE)
        .show(ui, add_contents)
        .response
}

pub fn dark_container_frame<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(Color32::from_rgb(15, 23, 42))
        .stroke(Stroke::new(1.0, Color32::from_rgb(51, 65, 85)))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(12))
        .show(ui, add_contents)
        .inner
}

pub fn speaker_badge(ui: &mut Ui, speaker: &str) {
    Frame::new()
        .fill(Color32::from_rgb(239, 246, 255))
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(speaker)
                    .color(Color32::from_rgb(37, 99, 235))
                    .size(11.5)
                    .strong(),
            );
        });
}

pub fn swap_capsule_button(ui: &mut Ui, enabled: bool) -> egui::Response {
    let id = ui.make_persistent_id("lang_swap_capsule");
    let is_hovered = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("hover_state"))
            .unwrap_or(false)
    });
    let is_active = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("active_state"))
            .unwrap_or(false)
    });

    let hover_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("hover"),
        is_hovered && enabled,
        0.15,
    );
    let active_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("active"),
        is_active && enabled,
        0.08,
    );

    let current_time = ui.ctx().input(|i| i.time);
    let click_time = ui.memory(|m| m.data.get_temp::<f64>(id.with("click_time")).unwrap_or(0.0));
    let elapsed = (current_time - click_time) as f32;
    let is_animating_click = elapsed >= 0.0 && elapsed < 0.28;
    let click_factor = if is_animating_click {
        ui.ctx().request_repaint();
        let t = (elapsed / 0.28).clamp(0.0, 1.0);
        (1.0 - crate::ui::animation::AnimationSystem::ease_out_cubic(t)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let rest_fill = Color32::WHITE;
    let hover_fill = Color32::from_rgb(245, 248, 253);
    let active_fill = Color32::from_rgb(235, 242, 254);

    let fill = if enabled {
        let base =
            crate::ui::animation::AnimationSystem::lerp_color(rest_fill, hover_fill, hover_factor);
        let base =
            crate::ui::animation::AnimationSystem::lerp_color(base, active_fill, active_factor);
        crate::ui::animation::AnimationSystem::lerp_color(
            base,
            Color32::from_rgb(230, 240, 255),
            click_factor,
        )
    } else {
        Color32::from_rgb(245, 248, 252)
    };

    let text_color = if enabled {
        let base = crate::ui::animation::AnimationSystem::lerp_color(
            Color32::from_rgb(59, 130, 246),
            Color32::from_rgb(37, 99, 235),
            hover_factor,
        );
        crate::ui::animation::AnimationSystem::lerp_color(
            base,
            Color32::from_rgb(29, 78, 216),
            active_factor,
        )
    } else {
        Color32::from_rgb(148, 163, 184)
    };

    let rest_stroke = Stroke::new(1.0, Color32::from_rgb(226, 232, 240));
    let hover_stroke = Stroke::new(1.0, Color32::from_rgb(203, 213, 225));
    let active_stroke = Stroke::new(1.0, Color32::from_rgb(148, 163, 184));
    let stroke_color = if enabled {
        let base = crate::ui::animation::AnimationSystem::lerp_color(
            rest_stroke.color,
            hover_stroke.color,
            hover_factor,
        );
        crate::ui::animation::AnimationSystem::lerp_color(base, active_stroke.color, active_factor)
    } else {
        rest_stroke.color
    };

    let rest_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 4,
        spread: 0,
        color: Color32::from_black_alpha(10),
    };
    let hover_shadow = egui::Shadow {
        offset: [0, 3],
        blur: 6,
        spread: 0,
        color: Color32::from_black_alpha(15),
    };
    let active_shadow = egui::Shadow {
        offset: [0, 1],
        blur: 2,
        spread: 0,
        color: Color32::from_black_alpha(6),
    };

    let shadow = if enabled {
        let base = crate::ui::animation::AnimationSystem::lerp_shadow(
            rest_shadow,
            hover_shadow,
            hover_factor,
        );
        crate::ui::animation::AnimationSystem::lerp_shadow(base, active_shadow, active_factor)
    } else {
        egui::Shadow::NONE
    };

    let resp = Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke_color))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(9, 4))
        .shadow(shadow)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("↔")
                    .color(text_color)
                    .size(13.0)
                    .strong(),
            )
        })
        .response
        .interact(egui::Sense::click());

    if resp.clicked() {
        ui.memory_mut(|m| {
            m.data.insert_temp(id.with("click_time"), current_time);
        });
        ui.ctx().request_repaint();
    }

    ui.memory_mut(|m| {
        m.data.insert_temp(id.with("hover_state"), resp.hovered());
        m.data
            .insert_temp(id.with("active_state"), resp.is_pointer_button_down_on());
    });
    if resp.hovered() && enabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn segmented_audio_meter(
    ui: &mut Ui,
    raw_fraction: f32,
    active: bool,
    visible: bool,
    updating: bool,
) {
    if visible {
        if updating {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }
        let animated_fraction = raw_fraction.clamp(0.0, 1.0);

        let seg_count = 6;
        let seg_w = 8.5;
        let seg_h = 8.5;
        let gap = 3.5;
        let total_w = (seg_count as f32) * seg_w + (seg_count - 1) as f32 * gap;

        let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, seg_h), egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let step = 1.0 / seg_count as f32;

            for i in 0..seg_count {
                let seg_threshold = (i as f32) * step;
                let is_filled = animated_fraction > seg_threshold;

                let min_x = rect.min.x + (i as f32) * (seg_w + gap);
                let seg_rect = egui::Rect::from_min_size(
                    egui::pos2(min_x, rect.min.y),
                    Vec2::new(seg_w, seg_h),
                );

                let radius = CornerRadius::same(4);

                let color = if is_filled {
                    if active {
                        Color32::from_rgb(16, 185, 129)
                    } else {
                        Color32::from_rgb(59, 130, 246)
                    }
                } else {
                    Color32::from_rgb(226, 232, 240)
                };

                painter.rect_filled(seg_rect, radius, color);
            }
        }
    }
}

pub fn sparse_dot_background(ui: &mut Ui) {
    let rect = ui.max_rect();
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let spacing = 26.0;
        let radius = 1.0;
        let color = Color32::from_rgba_unmultiplied(148, 163, 184, 38);

        let start_x = (rect.min.x / spacing).floor() * spacing + (spacing / 2.0);
        let start_y = (rect.min.y / spacing).floor() * spacing + (spacing / 2.0);

        let mut y = start_y;
        while y < rect.max.y {
            let mut x = start_x;
            while x < rect.max.x {
                if rect.contains(egui::pos2(x, y)) {
                    painter.circle_filled(egui::pos2(x, y), radius, color);
                }
                x += spacing;
            }
            y += spacing;
        }
    }
}

pub fn wavy_divider_black_shadow(ui: &mut Ui) {
    let available_width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(available_width, 8.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let y_center = rect.center().y;
        let amplitude = 2.0;
        let wavelength = 12.0;
        let stroke_width = 1.5;

        let points_shadow: Vec<egui::Pos2> = (0..=(rect.width() as i32))
            .step_by(2)
            .map(|x| {
                let x_pos = rect.min.x + x as f32;
                let phase = (x as f32 / wavelength) * std::f32::consts::TAU;
                let y_pos = y_center + phase.sin() * amplitude + 0.8;
                egui::pos2(x_pos, y_pos)
            })
            .collect();

        let points_main: Vec<egui::Pos2> = (0..=(rect.width() as i32))
            .step_by(2)
            .map(|x| {
                let x_pos = rect.min.x + x as f32;
                let phase = (x as f32 / wavelength) * std::f32::consts::TAU;
                let y_pos = y_center + phase.sin() * amplitude;
                egui::pos2(x_pos, y_pos)
            })
            .collect();

        painter.add(egui::Shape::line(
            points_shadow,
            Stroke::new(stroke_width, Color32::from_black_alpha(35)),
        ));

        painter.add(egui::Shape::line(
            points_main,
            Stroke::new(stroke_width, Color32::from_rgb(15, 23, 42)),
        ));
    }
}

pub fn wavy_divider(ui: &mut Ui, color: Color32) {
    let available_width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(available_width, 8.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let y_center = rect.center().y;
        let amplitude = 2.0;
        let wavelength = 12.0;
        let points: Vec<egui::Pos2> = (0..=(rect.width() as i32))
            .step_by(2)
            .map(|x| {
                let x_pos = rect.min.x + x as f32;
                let phase = (x as f32 / wavelength) * std::f32::consts::TAU;
                let y_pos = y_center + phase.sin() * amplitude;
                egui::pos2(x_pos, y_pos)
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(1.5, color)));
    }
}

pub fn section_heading(ui: &mut Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .size(15.0)
            .color(crate::ui::theme::text_strong())
            .strong(),
    );
    ui.add_space(8.0);
}

pub fn section<R>(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    ui.push_id(title, |ui| {
        card(ui, |ui| {
            section_heading(ui, title);
            add_contents(ui)
        })
    })
    .inner
}

pub fn animated_button(ui: &mut Ui, text: &str) -> egui::Response {
    animated_button_enabled(ui, text, true)
}

/// Formats a byte count for display.
pub fn format_file_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn animated_button_enabled(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    let id = ui.make_persistent_id(text);
    let is_hovered = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("hover_state"))
            .unwrap_or(false)
    });
    let is_active = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("active_state"))
            .unwrap_or(false)
    });

    let hover_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("anim_hover"),
        is_hovered && enabled,
        0.15,
    );
    let active_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("anim_active"),
        is_active && enabled,
        0.08,
    );

    let current_time = ui.ctx().input(|i| i.time);
    let click_time = ui.memory(|m| m.data.get_temp::<f64>(id.with("click_time")).unwrap_or(0.0));
    let elapsed = (current_time - click_time) as f32;
    let is_animating_click = elapsed >= 0.0 && elapsed < 0.25;
    let click_factor = if is_animating_click {
        ui.ctx().request_repaint();
        let t = (elapsed / 0.25).clamp(0.0, 1.0);
        (1.0 - crate::ui::animation::AnimationSystem::ease_out_cubic(t)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let rest_fill = Color32::WHITE;
    let hover_fill = Color32::from_rgb(245, 248, 253);
    let active_fill = Color32::from_rgb(235, 241, 250);

    let fill = if enabled {
        let base =
            crate::ui::animation::AnimationSystem::lerp_color(rest_fill, hover_fill, hover_factor);
        let base =
            crate::ui::animation::AnimationSystem::lerp_color(base, active_fill, active_factor);
        crate::ui::animation::AnimationSystem::lerp_color(
            base,
            Color32::from_rgb(230, 240, 255),
            click_factor,
        )
    } else {
        Color32::from_rgb(241, 245, 249)
    };

    let text_color = if enabled {
        crate::ui::theme::text_strong()
    } else {
        crate::ui::theme::text_weak()
    };

    let rest_stroke = Stroke::new(1.0, Color32::from_rgb(226, 232, 240));
    let hover_stroke = Stroke::new(1.0, Color32::from_rgb(203, 213, 225));
    let active_stroke = Stroke::new(1.0, Color32::from_rgb(148, 163, 184));
    let click_stroke = Stroke::new(1.0, Color32::from_rgb(147, 197, 253));

    let stroke = if enabled {
        let base = crate::ui::animation::AnimationSystem::lerp_color(
            rest_stroke.color,
            hover_stroke.color,
            hover_factor,
        );
        let base = crate::ui::animation::AnimationSystem::lerp_color(
            base,
            active_stroke.color,
            active_factor,
        );
        let color = crate::ui::animation::AnimationSystem::lerp_color(
            base,
            click_stroke.color,
            click_factor,
        );
        Stroke::new(1.0, color)
    } else {
        Stroke::new(1.0, Color32::from_rgb(226, 232, 240))
    };

    let rest_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 5,
        spread: 0,
        color: Color32::from_black_alpha(12),
    };
    let hover_shadow = egui::Shadow {
        offset: [0, 3],
        blur: 7,
        spread: 0,
        color: Color32::from_black_alpha(16),
    };
    let active_shadow = egui::Shadow {
        offset: [0, 1],
        blur: 2,
        spread: 0,
        color: Color32::from_black_alpha(6),
    };

    let shadow = if enabled {
        let base = crate::ui::animation::AnimationSystem::lerp_shadow(
            rest_shadow,
            hover_shadow,
            hover_factor,
        );
        crate::ui::animation::AnimationSystem::lerp_shadow(base, active_shadow, active_factor)
    } else {
        egui::Shadow::NONE
    };

    let resp = ui
        .add_enabled_ui(enabled, |ui| {
            Frame::new()
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(14, 7))
                .shadow(shadow)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .color(text_color)
                            .size(13.0)
                            .strong(),
                    );
                })
                .response
                .interact(egui::Sense::click())
        })
        .inner;

    if resp.clicked() {
        ui.memory_mut(|m| {
            m.data.insert_temp(id.with("click_time"), current_time);
        });
        ui.ctx().request_repaint();
    }

    ui.memory_mut(|m| {
        m.data.insert_temp(id.with("hover_state"), resp.hovered());
        m.data
            .insert_temp(id.with("active_state"), resp.is_pointer_button_down_on());
    });
    if resp.hovered() && enabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn primary_button(ui: &mut Ui, text: &str) -> egui::Response {
    primary_button_enabled(ui, text, true)
}

pub fn primary_button_enabled(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    let id = ui.make_persistent_id(text);
    let is_hovered = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("hover_state"))
            .unwrap_or(false)
    });
    let is_active = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("active_state"))
            .unwrap_or(false)
    });

    let hover_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("anim_hover"),
        is_hovered && enabled,
        0.15,
    );
    let active_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("anim_active"),
        is_active && enabled,
        0.08,
    );

    let rest_fill = Color32::from_rgb(59, 130, 246);
    let hover_fill = Color32::from_rgb(37, 99, 235);
    let active_fill = Color32::from_rgb(29, 78, 216);

    let fill = if enabled {
        let base =
            crate::ui::animation::AnimationSystem::lerp_color(rest_fill, hover_fill, hover_factor);
        crate::ui::animation::AnimationSystem::lerp_color(base, active_fill, active_factor)
    } else {
        Color32::from_rgb(219, 234, 254)
    };

    let text_color = if enabled {
        Color32::WHITE
    } else {
        Color32::from_rgb(147, 197, 253)
    };

    let rest_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 4,
        spread: 0,
        color: Color32::from_black_alpha(15),
    };
    let hover_shadow = egui::Shadow {
        offset: [0, 3],
        blur: 6,
        spread: 0,
        color: Color32::from_black_alpha(20),
    };
    let active_shadow = egui::Shadow {
        offset: [0, 1],
        blur: 2,
        spread: 0,
        color: Color32::from_black_alpha(8),
    };

    let shadow = if enabled {
        let base = crate::ui::animation::AnimationSystem::lerp_shadow(
            rest_shadow,
            hover_shadow,
            hover_factor,
        );
        crate::ui::animation::AnimationSystem::lerp_shadow(base, active_shadow, active_factor)
    } else {
        egui::Shadow::NONE
    };

    let resp = ui
        .add_enabled_ui(enabled, |ui| {
            Frame::new()
                .fill(fill)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::symmetric(18, 8))
                .shadow(shadow)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .color(text_color)
                            .size(13.5)
                            .strong(),
                    );
                })
                .response
                .interact(egui::Sense::click())
        })
        .inner;

    ui.memory_mut(|m| {
        m.data.insert_temp(id.with("hover_state"), resp.hovered());
        m.data
            .insert_temp(id.with("active_state"), resp.is_pointer_button_down_on());
    });
    if resp.hovered() && enabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn searchable_combobox<T: PartialEq + Clone>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    selected_text: impl Into<String>,
    selected: &mut T,
    options: &[(T, String)],
) -> bool {
    let mut changed = false;
    let search_id = ui.make_persistent_id(&id).with("combo_search");

    egui::ComboBox::from_id_salt(&id)
        .selected_text(selected_text.into())
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            let is_more_than_3 = options.len() > 3;

            let mut search_query = if is_more_than_3 {
                ui.memory(|m| m.data.get_temp::<String>(search_id).unwrap_or_default())
            } else {
                String::new()
            };

            if is_more_than_3 {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    let te = egui::TextEdit::singleline(&mut search_query)
                        .hint_text("Search...")
                        .desired_width(130.0)
                        .margin(Margin::symmetric(6, 4));
                    ui.add(te);
                    ui.add_space(2.0);
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(2.0);

                ui.memory_mut(|m| m.data.insert_temp(search_id, search_query.clone()));
            }

            let query_lower = search_query.trim().to_lowercase();
            let mut match_count = 0;

            for (val, label) in options {
                if !query_lower.is_empty() && !label.to_lowercase().contains(&query_lower) {
                    continue;
                }
                match_count += 1;
                if ui.selectable_value(selected, val.clone(), label).clicked() {
                    changed = true;
                    if is_more_than_3 {
                        ui.memory_mut(|m| m.data.insert_temp(search_id, String::new()));
                    }
                }
            }

            if is_more_than_3 && match_count == 0 {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("No matching items")
                        .size(12.0)
                        .color(crate::ui::theme::text_weak()),
                );
                ui.add_space(4.0);
            }
        });

    changed
}

pub fn search_bar(ui: &mut Ui, query: &mut String, hint: &str) -> bool {
    let mut changed = false;
    let has_query = !query.is_empty();
    Frame::new()
        .fill(Color32::from_rgb(248, 250, 252))
        .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let text_frame = Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::ZERO)
                    .inner_margin(Margin::ZERO);
                let response = ui.add(
                    egui::TextEdit::singleline(query)
                        .hint_text(hint)
                        .frame(text_frame)
                        .margin(Margin::symmetric(0, 0))
                        .desired_width(ui.available_width() - if has_query { 24.0 } else { 0.0 }),
                );
                if response.changed() {
                    changed = true;
                }
                if !query.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let clear_btn = Frame::new()
                            .fill(Color32::from_rgb(226, 232, 240))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("×")
                                        .color(Color32::from_rgb(100, 116, 139))
                                        .size(12.0)
                                        .strong(),
                                )
                            })
                            .response
                            .interact(egui::Sense::click());
                        if clear_btn.clicked() {
                            query.clear();
                            changed = true;
                        }
                    });
                }
            });
        });
    changed
}

pub fn input_field(ui: &mut Ui, text: &mut String, hint: &str) -> egui::Response {
    let mut resp = None;
    Frame::new()
        .fill(Color32::from_rgb(248, 250, 252))
        .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            let text_frame = Frame::new()
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::ZERO)
                .inner_margin(Margin::ZERO);
            let r = ui.add(
                egui::TextEdit::singleline(text)
                    .hint_text(hint)
                    .frame(text_frame)
                    .margin(Margin::ZERO)
                    .desired_width(ui.available_width()),
            );
            resp = Some(r);
        });
    resp.unwrap()
}

pub fn danger_alert(ui: &mut Ui, text: &str) {
    Frame::new()
        .fill(Color32::from_rgb(254, 242, 242))
        .stroke(Stroke::new(1.0, Color32::from_rgb(254, 202, 202)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("!")
                        .strong()
                        .color(Color32::from_rgb(220, 38, 38)),
                );
                ui.label(
                    egui::RichText::new(text)
                        .color(Color32::from_rgb(185, 28, 28))
                        .size(12.5),
                );
            });
        });
}

pub fn target_language_pair_selector(
    ui: &mut Ui,
    id_prefix: &str,
    source_language: &str,
    target_language: &mut String,
    language: crate::i18n::UiLanguage,
    label_fn: impl Fn(&str, crate::i18n::UiLanguage) -> String,
) -> bool {
    let mut changed = false;
    if source_language == "auto" {
        let (mut a, mut b) = match target_language.split_once(',') {
            Some((x, y)) => (x.to_string(), y.to_string()),
            None => ("zh".to_string(), "en".to_string()),
        };

        let options_a: Vec<_> = crate::LANGUAGE_OPTIONS
            .iter()
            .filter(|(code, _)| !crate::languages_conflict(code, &b))
            .map(|(code, label)| {
                (
                    (*code).to_string(),
                    crate::i18n::tr(language, label).to_string(),
                )
            })
            .collect();

        let options_b: Vec<_> = crate::LANGUAGE_OPTIONS
            .iter()
            .filter(|(code, _)| !crate::languages_conflict(code, &a))
            .map(|(code, label)| {
                (
                    (*code).to_string(),
                    crate::i18n::tr(language, label).to_string(),
                )
            })
            .collect();

        ui.horizontal(|ui| {
            if searchable_combobox(
                ui,
                format!("{id_prefix}_target_a"),
                label_fn(&a, language),
                &mut a,
                &options_a,
            ) {
                changed = true;
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("↔")
                    .color(crate::ui::theme::text_weak())
                    .strong(),
            );
            ui.add_space(4.0);
            if searchable_combobox(
                ui,
                format!("{id_prefix}_target_b"),
                label_fn(&b, language),
                &mut b,
                &options_b,
            ) {
                changed = true;
            }
        });

        let new_target = format!("{a},{b}");
        if new_target != *target_language {
            *target_language = new_target;
            changed = true;
        }
    } else {
        if target_language.contains(',') {
            if let Some((first, _)) = target_language.split_once(',') {
                *target_language = first.to_string();
                changed = true;
            }
        }
        if crate::languages_conflict(target_language, source_language) {
            let fallback = if crate::languages_conflict(source_language, "zh") {
                "en"
            } else {
                "zh"
            };
            *target_language = fallback.to_string();
            changed = true;
        }

        let mut target_options = Vec::new();
        for (code, label) in crate::LANGUAGE_OPTIONS {
            if !crate::languages_conflict(code, source_language) {
                target_options.push((
                    (*code).to_string(),
                    crate::i18n::tr(language, label).to_string(),
                ));
            }
        }

        if searchable_combobox(
            ui,
            format!("{id_prefix}_target"),
            label_fn(target_language, language),
            target_language,
            &target_options,
        ) {
            changed = true;
        }
    }
    changed
}

pub fn language_selector(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    language: &mut crate::i18n::UiLanguage,
) -> bool {
    let options: Vec<_> = crate::i18n::UiLanguage::ALL
        .into_iter()
        .map(|lang| (lang, lang.display_name().to_string()))
        .collect();
    let current_text = language.display_name().to_string();
    searchable_combobox(ui, id, current_text, language, &options)
}

pub fn danger_button(ui: &mut Ui, text: &str) -> egui::Response {
    danger_button_enabled(ui, text, true)
}

pub fn danger_button_enabled(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    let id = ui.make_persistent_id(text);
    let is_hovered = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("hover_state"))
            .unwrap_or(false)
    });
    let is_active = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("active_state"))
            .unwrap_or(false)
    });

    let hover_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("anim_hover"),
        is_hovered && enabled,
        0.15,
    );
    let active_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("anim_active"),
        is_active && enabled,
        0.08,
    );

    let rest_fill = Color32::from_rgb(239, 68, 68);
    let hover_fill = Color32::from_rgb(220, 38, 38);
    let active_fill = Color32::from_rgb(185, 28, 28);

    let fill = if enabled {
        let base =
            crate::ui::animation::AnimationSystem::lerp_color(rest_fill, hover_fill, hover_factor);
        crate::ui::animation::AnimationSystem::lerp_color(base, active_fill, active_factor)
    } else {
        Color32::from_rgb(254, 202, 202)
    };

    let text_color = Color32::WHITE;

    let rest_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 4,
        spread: 0,
        color: Color32::from_black_alpha(15),
    };
    let hover_shadow = egui::Shadow {
        offset: [0, 3],
        blur: 6,
        spread: 0,
        color: Color32::from_black_alpha(20),
    };
    let active_shadow = egui::Shadow {
        offset: [0, 1],
        blur: 2,
        spread: 0,
        color: Color32::from_black_alpha(8),
    };

    let shadow = if enabled {
        let base = crate::ui::animation::AnimationSystem::lerp_shadow(
            rest_shadow,
            hover_shadow,
            hover_factor,
        );
        crate::ui::animation::AnimationSystem::lerp_shadow(base, active_shadow, active_factor)
    } else {
        egui::Shadow::NONE
    };

    let resp = ui
        .add_enabled_ui(enabled, |ui| {
            Frame::new()
                .fill(fill)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::symmetric(18, 8))
                .shadow(shadow)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .color(text_color)
                            .size(13.5)
                            .strong(),
                    );
                })
                .response
                .interact(egui::Sense::click())
        })
        .inner;

    ui.memory_mut(|m| {
        m.data.insert_temp(id.with("hover_state"), resp.hovered());
        m.data
            .insert_temp(id.with("active_state"), resp.is_pointer_button_down_on());
    });
    if resp.hovered() && enabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn pill_toggle(ui: &mut Ui, checked: &mut bool) -> egui::Response {
    let id = ui.next_auto_id();
    let is_hovered = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("hover_state"))
            .unwrap_or(false)
    });
    let is_active = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("active_state"))
            .unwrap_or(false)
    });

    let hover_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("hover"),
        is_hovered,
        0.15,
    );
    let active_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("active"),
        is_active,
        0.08,
    );
    let switch_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("switch"),
        *checked,
        0.18,
    );

    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(36.0, 20.0), egui::Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        let track_checked_base = Color32::from_rgb(59, 130, 246);
        let track_checked_hover = Color32::from_rgb(37, 99, 235);
        let track_checked_active = Color32::from_rgb(29, 78, 216);

        let track_unchecked_base = Color32::from_rgb(226, 232, 240);
        let track_unchecked_hover = Color32::from_rgb(203, 213, 225);
        let track_unchecked_active = Color32::from_rgb(186, 198, 214);

        let track_checked = crate::ui::animation::AnimationSystem::lerp_color(
            track_checked_base,
            track_checked_hover,
            hover_factor,
        );
        let track_checked = crate::ui::animation::AnimationSystem::lerp_color(
            track_checked,
            track_checked_active,
            active_factor,
        );

        let track_unchecked = crate::ui::animation::AnimationSystem::lerp_color(
            track_unchecked_base,
            track_unchecked_hover,
            hover_factor,
        );
        let track_unchecked = crate::ui::animation::AnimationSystem::lerp_color(
            track_unchecked,
            track_unchecked_active,
            active_factor,
        );

        let track_fill = crate::ui::animation::AnimationSystem::lerp_color(
            track_unchecked,
            track_checked,
            switch_factor,
        );

        painter.rect_filled(rect, CornerRadius::same(10), track_fill);

        let knob_radius = 7.5;
        let min_x = rect.min.x + 10.0;
        let max_x = rect.max.x - 10.0;
        let current_x = min_x + (max_x - min_x) * switch_factor;

        let knob_center = egui::pos2(current_x, rect.center().y);
        painter.circle_filled(knob_center, knob_radius, Color32::WHITE);
    }

    ui.memory_mut(|m| {
        m.data
            .insert_temp(id.with("hover_state"), response.hovered());
        m.data.insert_temp(
            id.with("active_state"),
            response.is_pointer_button_down_on(),
        );
    });
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    response
}

pub fn toggle_with_label(ui: &mut Ui, checked: &mut bool, label: &str) -> egui::Response {
    ui.horizontal(|ui| {
        let mut resp = pill_toggle(ui, checked);
        ui.add_space(4.0);
        let text_resp = ui.label(
            egui::RichText::new(label)
                .color(crate::ui::theme::text_strong())
                .size(13.0),
        );
        if text_resp.clicked() {
            *checked = !*checked;
            resp.mark_changed();
        }
        resp
    })
    .inner
}

pub fn feature_checkbox(
    ui: &mut Ui,
    feature: crate::feature_access::Feature,
    language: crate::i18n::UiLanguage,
    checked: &mut bool,
    text: &str,
) -> egui::Response {
    let access = crate::feature_access::access(feature);
    let mut response = ui
        .add_enabled_ui(access.available, |ui| toggle_with_label(ui, checked, text))
        .inner;
    response = decorate_unavailable(response, access, language);
    response
}

pub fn feature_ui<R>(
    ui: &mut Ui,
    feature: crate::feature_access::Feature,
    language: crate::i18n::UiLanguage,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let access = crate::feature_access::access(feature);
    let mut response = ui.add_enabled_ui(access.available, add_contents);
    response.response = decorate_unavailable(response.response, access, language);
    response
}

fn decorate_unavailable(
    response: egui::Response,
    access: crate::feature_access::FeatureAccess,
    language: crate::i18n::UiLanguage,
) -> egui::Response {
    match access.unavailable_reason {
        Some(reason) if !access.available => {
            response.on_disabled_hover_text(crate::i18n::tr(language, reason))
        }
        _ => response,
    }
}

pub fn file_path_input(
    ui: &mut Ui,
    value: &mut String,
    hint: &str,
    browse_label: &str,
    filter_name: &str,
    extensions: &[&str],
    input_width: f32,
) -> bool {
    let mut changed = ui
        .add(
            egui::TextEdit::singleline(value)
                .hint_text(hint)
                .desired_width(input_width.min(420.0))
                .margin(egui::vec2(10.0, 8.0)),
        )
        .changed();

    if animated_button(ui, browse_label).clicked()
        && let Some(path) = rfd::FileDialog::new()
            .add_filter(filter_name, extensions)
            .pick_file()
    {
        *value = path.display().to_string();
        changed = true;
    }
    changed
}

pub fn status_badge(ui: &mut Ui, status: &str, is_active: bool, is_error: bool) {
    let (bg_color, fg_color, dot) = if is_error {
        (
            Color32::from_rgb(254, 242, 242),
            Color32::from_rgb(220, 38, 38),
            "● ",
        )
    } else if is_active {
        (
            Color32::from_rgb(236, 253, 245),
            Color32::from_rgb(5, 150, 105),
            "● ",
        )
    } else {
        (
            Color32::from_rgb(239, 246, 255),
            Color32::from_rgb(37, 99, 235),
            "",
        )
    };

    Frame::new()
        .fill(bg_color)
        .corner_radius(CornerRadius::same(14))
        .stroke(Stroke::NONE)
        .inner_margin(Margin::symmetric(12, 5))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("{dot}{status}"))
                    .color(fg_color)
                    .size(12.0)
                    .strong(),
            );
        });
}

pub struct SubNavItem<T: Copy + PartialEq> {
    pub id: T,
    pub icon: &'static str,
    pub label: &'static str,
}

pub fn sub_sidebar<T: Copy + PartialEq>(
    ui: &mut Ui,
    selected: &mut T,
    items: &[SubNavItem<T>],
    language: crate::i18n::UiLanguage,
) {
    let width = 175.0;
    let card_id = ui.make_persistent_id("sub_sidebar_layout_measurement");

    let header_h = ui.memory(|m| m.data.get_temp::<f32>(card_id).unwrap_or(32.0));

    let card_height = ui.available_height().max(240.0);
    let inner_height = card_height - 24.0;
    let count = items.len();

    let gap = 8.0;
    let total_gaps = count.saturating_sub(1) as f32 * gap;
    let space_for_buttons = (inner_height - header_h - total_gaps).max(0.0);

    let item_height = if count > 0 {
        (space_for_buttons / count as f32).max(38.0)
    } else {
        38.0
    };

    Frame::new()
        .fill(Color32::from_rgb(245, 248, 252))
        .corner_radius(CornerRadius::same(18))
        .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
        .inner_margin(Margin::symmetric(10, 12))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.set_min_height(card_height);
            ui.vertical(|ui| {
                let start_y = ui.cursor().top();

                ui.label(
                    egui::RichText::new(crate::i18n::tr(language, "NAVIGATE"))
                        .size(10.5)
                        .color(crate::ui::theme::text_weak())
                        .strong(),
                );
                ui.add_space(8.0);

                let header_end_y = ui.cursor().top();
                ui.memory_mut(|m| {
                    m.data
                        .insert_temp(card_id, (header_end_y - start_y).max(20.0))
                });

                for (idx, item) in items.iter().enumerate() {
                    if idx > 0 {
                        ui.add_space(gap);
                    }

                    let is_selected = *selected == item.id;
                    let id = ui.make_persistent_id(item.label);

                    let is_hovered = ui.memory(|m| {
                        m.data
                            .get_temp::<bool>(id.with("hover_state"))
                            .unwrap_or(false)
                    });
                    let is_active = ui.memory(|m| {
                        m.data
                            .get_temp::<bool>(id.with("active_state"))
                            .unwrap_or(false)
                    });

                    let select_factor = crate::ui::animation::AnimationSystem::animate_bool(
                        ui.ctx(),
                        id.with("select"),
                        is_selected,
                        0.20,
                    );

                    let hover_factor = crate::ui::animation::AnimationSystem::animate_bool(
                        ui.ctx(),
                        id.with("hover"),
                        is_hovered && !is_selected,
                        0.15,
                    );

                    let active_factor = crate::ui::animation::AnimationSystem::animate_bool(
                        ui.ctx(),
                        id.with("active"),
                        is_active && !is_selected,
                        0.08,
                    );

                    let bg_fill = if select_factor > 0.0 {
                        crate::ui::animation::AnimationSystem::lerp_color(
                            Color32::TRANSPARENT,
                            Color32::from_rgb(239, 246, 255),
                            select_factor,
                        )
                    } else if active_factor > 0.0 {
                        crate::ui::animation::AnimationSystem::lerp_color(
                            Color32::TRANSPARENT,
                            Color32::from_rgb(229, 239, 255),
                            active_factor,
                        )
                    } else if hover_factor > 0.0 {
                        crate::ui::animation::AnimationSystem::lerp_color(
                            Color32::TRANSPARENT,
                            Color32::from_rgb(238, 244, 253),
                            hover_factor,
                        )
                    } else {
                        Color32::TRANSPARENT
                    };

                    let text_color = crate::ui::animation::AnimationSystem::lerp_color(
                        crate::ui::theme::text_normal(),
                        Color32::from_rgb(37, 99, 235),
                        select_factor,
                    );

                    let text = if item.icon.is_empty() {
                        item.label.to_string()
                    } else {
                        format!("{} {}", item.icon, item.label)
                    };

                    let button_h = item_height.clamp(38.0, 52.0);
                    let v_padding = ((button_h - 18.0) / 2.0).max(8.0);

                    let resp = Frame::new()
                        .fill(bg_fill)
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(Margin::symmetric(14, v_padding as i8))
                        .stroke(Stroke::NONE)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                if select_factor > 0.1 {
                                    let (bar_rect, _) = ui.allocate_exact_size(
                                        Vec2::new(3.0, 14.0),
                                        egui::Sense::hover(),
                                    );
                                    let bar_color = Color32::from_rgba_premultiplied(
                                        37,
                                        99,
                                        235,
                                        (255.0 * select_factor) as u8,
                                    );
                                    ui.painter().rect_filled(
                                        bar_rect,
                                        CornerRadius::same(2),
                                        bar_color,
                                    );
                                    ui.add_space(4.0);
                                }
                                let mut rt =
                                    egui::RichText::new(&text).size(13.5).color(text_color);
                                if is_selected {
                                    rt = rt.strong();
                                }
                                ui.label(rt)
                            })
                        })
                        .response
                        .interact(egui::Sense::click());

                    ui.memory_mut(|m| {
                        m.data.insert_temp(id.with("hover_state"), resp.hovered());
                        m.data
                            .insert_temp(id.with("active_state"), resp.is_pointer_button_down_on());
                    });

                    if resp.clicked() {
                        *selected = item.id;
                    }
                    if resp.hovered() && !is_selected {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
            });
        });
}

pub fn modern_slider_f64(
    ui: &mut Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    default: f64,
    label: &str,
    suffix: &str,
) -> egui::Response {
    ui.horizontal(|ui| {
        let available = ui.available_width();
        let is_narrow = available < 280.0;
        let label_w = if is_narrow { 85.0 } else { 110.0 };

        if !label.is_empty() {
            ui.allocate_ui_with_layout(
                Vec2::new(label_w, 20.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(
                        egui::RichText::new(label)
                            .color(crate::ui::theme::text_strong())
                            .size(if is_narrow { 12.0 } else { 13.0 })
                            .strong(),
                    );
                },
            );
        }

        let slider = egui::Slider::new(value, range)
            .show_value(false)
            .step_by(0.5)
            .trailing_fill(true);

        let slider_w = (ui.available_width() - 82.0).max(50.0);
        let response = ui.add_sized(Vec2::new(slider_w, 20.0), slider);

        let value_text = format!("{:.1}{}", *value, suffix);
        ui.add_space(4.0);
        Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::symmetric(10, 4))
            .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
            .shadow(egui::Shadow {
                offset: [0, 2],
                blur: 4,
                spread: 0,
                color: Color32::from_black_alpha(15),
            })
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(value_text)
                        .size(12.5)
                        .color(crate::ui::theme::text_strong())
                        .strong(),
                );
            });

        let mut reset = reset_button(ui, label);
        if reset.clicked() && *value != default {
            *value = default;
            reset.mark_changed();
        }

        response | reset
    })
    .inner
}

pub fn modern_slider_f32(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
    label: &str,
    states: &[&str],
) -> egui::Response {
    ui.horizontal(|ui| {
        let available = ui.available_width();
        let is_narrow = available < 280.0;
        let label_w = if is_narrow { 85.0 } else { 110.0 };

        ui.allocate_ui_with_layout(
            Vec2::new(label_w, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(label)
                        .color(crate::ui::theme::text_strong())
                        .size(if is_narrow { 12.0 } else { 13.0 })
                        .strong(),
                );
            },
        );
        let slider_w = (ui.available_width() - 82.0).max(50.0);
        let response = ui.add_sized(
            Vec2::new(slider_w, 20.0),
            egui::Slider::new(value, range.clone())
                .show_value(false)
                .step_by(0.01)
                .trailing_fill(true),
        );
        ui.add_space(4.0);
        Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::symmetric(10, 4))
            .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
            .shadow(egui::Shadow {
                offset: [0, 2],
                blur: 4,
                spread: 0,
                color: Color32::from_black_alpha(15),
            })
            .show(ui, |ui| {
                let label = slider_state_label(*value, &range, states)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{value:.2}"));
                ui.label(
                    egui::RichText::new(label)
                        .size(12.5)
                        .color(crate::ui::theme::text_strong())
                        .strong(),
                );
            });
        let mut reset = reset_button(ui, label);
        if reset.clicked() && *value != default {
            *value = default;
            reset.mark_changed();
        }
        response | reset
    })
    .inner
}

fn slider_state_label<'a>(
    value: f32,
    range: &std::ops::RangeInclusive<f32>,
    states: &'a [&str],
) -> Option<&'a str> {
    if states.is_empty() {
        return None;
    }
    let span = *range.end() - *range.start();
    let fraction = if span > 0.0 {
        (value - *range.start()) / span
    } else {
        0.0
    };
    let index = (fraction.clamp(0.0, 1.0) * (states.len() - 1) as f32).round() as usize;
    states.get(index).copied()
}

pub fn reset_button(ui: &mut Ui, id_salt: &str) -> egui::Response {
    let id = ui.make_persistent_id("slider_reset_btn").with(id_salt);
    let is_hovered = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("hover_state"))
            .unwrap_or(false)
    });
    let is_active = ui.memory(|m| {
        m.data
            .get_temp::<bool>(id.with("active_state"))
            .unwrap_or(false)
    });

    let hover_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("hover"),
        is_hovered,
        0.15,
    );
    let active_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("active"),
        is_active,
        0.08,
    );

    let current_time = ui.ctx().input(|i| i.time);
    let spin_start = ui.memory(|m| m.data.get_temp::<f64>(id.with("spin_start")).unwrap_or(0.0));
    let elapsed = (current_time - spin_start) as f32;
    let spin_duration = 0.40;
    let is_spinning = elapsed >= 0.0 && elapsed < spin_duration;

    let (rotation_angle, spin_accent_factor) = if is_spinning {
        ui.ctx().request_repaint();
        let t = (elapsed / spin_duration).clamp(0.0, 1.0);
        let progress = crate::ui::animation::AnimationSystem::ease_out_cubic(t);
        let angle = -std::f32::consts::TAU * progress;
        let accent = (1.0 - progress).clamp(0.0, 1.0);
        (angle, accent)
    } else {
        (0.0, 0.0)
    };

    let rest_fill = Color32::WHITE;
    let hover_fill = Color32::from_rgb(248, 250, 252);
    let active_fill = Color32::from_rgb(235, 241, 250);
    let spin_fill = Color32::from_rgb(239, 246, 255);

    let base_fill =
        crate::ui::animation::AnimationSystem::lerp_color(rest_fill, hover_fill, hover_factor);
    let fill =
        crate::ui::animation::AnimationSystem::lerp_color(base_fill, active_fill, active_factor);
    let fill =
        crate::ui::animation::AnimationSystem::lerp_color(fill, spin_fill, spin_accent_factor);

    let rest_stroke = Color32::from_rgb(226, 232, 240);
    let hover_stroke = Color32::from_rgb(203, 213, 225);
    let active_stroke = Color32::from_rgb(148, 163, 184);
    let spin_stroke = Color32::from_rgb(147, 197, 253);

    let stroke_color =
        crate::ui::animation::AnimationSystem::lerp_color(rest_stroke, hover_stroke, hover_factor);
    let stroke_color = crate::ui::animation::AnimationSystem::lerp_color(
        stroke_color,
        active_stroke,
        active_factor,
    );
    let stroke_color = crate::ui::animation::AnimationSystem::lerp_color(
        stroke_color,
        spin_stroke,
        spin_accent_factor,
    );

    let rest_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 4,
        spread: 0,
        color: Color32::from_black_alpha(10),
    };
    let hover_shadow = egui::Shadow {
        offset: [0, 3],
        blur: 6,
        spread: 0,
        color: Color32::from_black_alpha(15),
    };
    let active_shadow = egui::Shadow {
        offset: [0, 1],
        blur: 2,
        spread: 0,
        color: Color32::from_black_alpha(6),
    };

    let shadow = {
        let s = crate::ui::animation::AnimationSystem::lerp_shadow(
            rest_shadow,
            hover_shadow,
            hover_factor,
        );
        crate::ui::animation::AnimationSystem::lerp_shadow(s, active_shadow, active_factor)
    };

    let icon_rest_tint = crate::ui::theme::text_strong();
    let icon_spin_tint = Color32::from_rgb(37, 99, 235);
    let icon_tint = crate::ui::animation::AnimationSystem::lerp_color(
        icon_rest_tint,
        icon_spin_tint,
        spin_accent_factor,
    );

    let mut image = egui::Image::new(egui::include_image!("../../resources/icons/reset.svg"))
        .fit_to_exact_size(Vec2::splat(13.0))
        .tint(icon_tint);

    if rotation_angle.abs() > 0.0001 {
        image = image.rotate(rotation_angle, Vec2::splat(0.5));
    }

    let resp = Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(6))
        .stroke(Stroke::new(1.0, stroke_color))
        .shadow(shadow)
        .show(ui, |ui| {
            ui.add(image);
        })
        .response
        .interact(egui::Sense::click());

    if resp.clicked() {
        ui.memory_mut(|m| {
            m.data.insert_temp(id.with("spin_start"), current_time);
        });
        ui.ctx().request_repaint();
    }

    ui.memory_mut(|m| {
        m.data.insert_temp(id.with("hover_state"), resp.hovered());
        m.data
            .insert_temp(id.with("active_state"), resp.is_pointer_button_down_on());
    });
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
