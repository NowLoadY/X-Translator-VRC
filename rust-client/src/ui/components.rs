use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Stroke, Ui, Vec2};

pub fn card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(Color32::WHITE)
        .corner_radius(CornerRadius::same(18))
        .inner_margin(Margin::same(18))
        .stroke(Stroke::new(1.0, Color32::from_rgb(232, 237, 246)))
        .shadow(egui::Shadow {
            offset: [0, 3],
            blur: 10,
            spread: 0,
            color: Color32::from_black_alpha(7),
        })
        .show(ui, add_contents)
        .inner
}

/// Paints a subtle, sparse dot-matrix background pattern across the available panel area.
pub fn sparse_dot_background(ui: &mut Ui) {
    let rect = ui.max_rect();
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let spacing = 22.0;
        let radius = 1.0;
        let color = Color32::from_rgba_unmultiplied(148, 163, 184, 60);

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

/// Renders a pure black sine wave line with a subtle drop shadow.
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

/// Renders a smooth sine wave decorative line.
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
    let is_hovered = ui.memory(|m| m.data.get_temp::<bool>(id).unwrap_or(false));

    let hover_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("anim"),
        is_hovered && enabled,
        0.15,
    );
    let fill = if enabled {
        crate::ui::animation::AnimationSystem::lerp_color(
            Color32::from_rgb(241, 245, 249),
            Color32::from_rgb(226, 232, 240),
            hover_factor,
        )
    } else {
        Color32::from_rgb(241, 245, 249)
    };

    let button =
        egui::Button::new(egui::RichText::new(text).color(crate::ui::theme::text_normal()))
            .fill(fill)
            .min_size(Vec2::new(80.0, 30.0))
            .corner_radius(CornerRadius::same(14));

    let response = ui.add_enabled(enabled, button);

    ui.memory_mut(|m| m.data.insert_temp(id, response.hovered()));
    if response.hovered() && enabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn primary_button(ui: &mut Ui, text: &str) -> egui::Response {
    primary_button_enabled(ui, text, true)
}

pub fn primary_button_enabled(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    let id = ui.make_persistent_id(text);
    let is_hovered = ui.memory(|m| m.data.get_temp::<bool>(id).unwrap_or(false));

    let hover_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("anim"),
        is_hovered && enabled,
        0.15,
    );
    let fill = if enabled {
        crate::ui::animation::AnimationSystem::lerp_color(
            Color32::from_rgb(37, 99, 235),
            Color32::from_rgb(29, 78, 216),
            hover_factor,
        )
    } else {
        Color32::from_rgb(191, 219, 254)
    };

    let button = egui::Button::new(egui::RichText::new(text).color(Color32::WHITE).strong())
        .fill(fill)
        .min_size(Vec2::new(100.0, 32.0))
        .corner_radius(CornerRadius::same(14));

    let response = ui.add_enabled(enabled, button);

    ui.memory_mut(|m| m.data.insert_temp(id, response.hovered()));
    if response.hovered() && enabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Renders a smart combobox that automatically includes an integrated search input box when options > 3.
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
                        .hint_text("🔍 Search...")
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

/// Shared language selector used by settings and first-run onboarding.
/// Returns whether the selected language changed.
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
    let is_hovered = ui.memory(|m| m.data.get_temp::<bool>(id).unwrap_or(false));
    let hover_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("anim"),
        is_hovered && enabled,
        0.15,
    );
    let fill = if enabled {
        crate::ui::animation::AnimationSystem::lerp_color(
            Color32::from_rgb(239, 68, 68),
            Color32::from_rgb(220, 38, 38),
            hover_factor,
        )
    } else {
        Color32::from_rgb(254, 202, 202)
    };
    let button = egui::Button::new(egui::RichText::new(text).color(Color32::WHITE).strong())
        .fill(fill)
        .min_size(Vec2::new(90.0, 32.0))
        .corner_radius(CornerRadius::same(14));

    let response = ui.add_enabled(enabled, button);

    ui.memory_mut(|m| m.data.insert_temp(id, response.hovered()));
    if response.hovered() && enabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Renders a cute pill-like toggle switch.
pub fn pill_toggle(ui: &mut Ui, checked: &mut bool) -> egui::Response {
    let id = ui
        .make_persistent_id("pill_toggle")
        .with(checked as *const bool);
    let is_hovered = ui.memory(|m| m.data.get_temp::<bool>(id).unwrap_or(false));
    let hover_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("hover"),
        is_hovered,
        0.15,
    );
    let switch_factor = crate::ui::animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("switch"),
        *checked,
        0.18,
    );

    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(34.0, 18.0), egui::Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        let track_fill = if *checked {
            crate::ui::animation::AnimationSystem::lerp_color(
                Color32::from_rgb(37, 99, 235),
                Color32::from_rgb(29, 78, 216),
                hover_factor,
            )
        } else {
            crate::ui::animation::AnimationSystem::lerp_color(
                Color32::from_rgb(226, 232, 240),
                Color32::from_rgb(203, 213, 225),
                hover_factor,
            )
        };

        painter.rect_filled(rect, CornerRadius::same(9), track_fill);

        let knob_radius = 7.0;
        let min_x = rect.min.x + 9.0;
        let max_x = rect.max.x - 9.0;
        let current_x = min_x + (max_x - min_x) * switch_factor;

        let knob_center = egui::pos2(current_x, rect.center().y);
        painter.circle_filled(knob_center, knob_radius, Color32::WHITE);
    }

    ui.memory_mut(|m| m.data.insert_temp(id, response.hovered()));
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

/// Adds a feature-aware checkbox using the cute pill toggle.
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

/// Adds controls governed by one feature.
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

/// Rounded file-picker input used wherever a local executable or model path
/// is configured. The caller owns all labels, so fixed copy remains in i18n.
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
            Color32::from_rgb(254, 226, 226),
            Color32::from_rgb(185, 28, 28),
            "● ",
        )
    } else if is_active {
        (
            Color32::from_rgb(220, 252, 231),
            Color32::from_rgb(21, 128, 61),
            "● ",
        )
    } else {
        (
            Color32::from_rgb(241, 245, 249),
            Color32::from_rgb(37, 99, 235),
            "",
        )
    };

    Frame::new()
        .fill(bg_color)
        .corner_radius(CornerRadius::same(14))
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

/// Generic item representation for the secondary inner sidebar navigation.
pub struct SubNavItem<T: Copy + PartialEq> {
    pub id: T,
    pub icon: &'static str,
    pub label: &'static str,
}

/// Modular, reusable secondary left sub-sidebar component for sub-page section navigation.
pub fn sub_sidebar<T: Copy + PartialEq>(
    ui: &mut Ui,
    selected: &mut T,
    items: &[SubNavItem<T>],
    language: crate::i18n::UiLanguage,
) {
    let width = 175.0;
    let card_id = ui.make_persistent_id("sub_sidebar_layout_measurement");

    // Read actual measured header height from memory (defaults to 32.0 on frame 1)
    let header_h = ui.memory(|m| m.data.get_temp::<f32>(card_id).unwrap_or(32.0));

    let card_height = ui.available_height().max(240.0);
    let inner_height = card_height - 24.0; // Frame top & bottom inner_margin (12 + 12)
    let count = items.len();

    let gap = 8.0;
    let total_gaps = count.saturating_sub(1) as f32 * gap;
    let space_for_buttons = (inner_height - header_h - total_gaps).max(0.0);

    // Dynamic button height scaling with available space (retaining 38.0px minimum constraint)
    let item_height = if count > 0 {
        (space_for_buttons / count as f32).max(38.0)
    } else {
        38.0
    };

    Frame::new()
        .fill(Color32::from_rgb(248, 250, 252))
        .corner_radius(CornerRadius::same(16))
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

                    let is_hovered = ui.memory(|m| m.data.get_temp::<bool>(id).unwrap_or(false));

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

                    let bg_fill = if select_factor > 0.0 {
                        crate::ui::animation::AnimationSystem::lerp_color(
                            Color32::TRANSPARENT,
                            Color32::WHITE,
                            select_factor,
                        )
                    } else if hover_factor > 0.0 {
                        crate::ui::animation::AnimationSystem::lerp_color(
                            Color32::TRANSPARENT,
                            Color32::from_rgb(241, 245, 249),
                            hover_factor,
                        )
                    } else {
                        Color32::TRANSPARENT
                    };

                    let text_color = crate::ui::animation::AnimationSystem::lerp_color(
                        crate::ui::theme::text_normal(),
                        Color32::from_rgb(37, 99, 235), // Accent Blue
                        select_factor,
                    );

                    let stroke = Stroke::new(
                        1.0,
                        crate::ui::animation::AnimationSystem::lerp_color(
                            Color32::TRANSPARENT,
                            Color32::from_rgb(219, 234, 254),
                            select_factor,
                        ),
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
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::symmetric(14, v_padding as i8))
                        .stroke(stroke)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
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

                    ui.memory_mut(|m| m.data.insert_temp(id, resp.hovered()));

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

/// A modern, styled slider component with smooth track styling, active fill, rounded handle, and formatted value display.
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
            .fill(Color32::from_rgb(241, 245, 249))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(8, 3))
            .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(value_text)
                        .size(12.5)
                        .color(crate::ui::theme::primary())
                        .strong(),
                );
            });

        let mut reset = reset_button(ui);
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
            .fill(Color32::from_rgb(241, 245, 249))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(8, 3))
            .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
            .show(ui, |ui| {
                let label = slider_state_label(*value, &range, states)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{value:.2}"));
                ui.label(
                    egui::RichText::new(label)
                        .size(12.5)
                        .color(crate::ui::theme::primary())
                        .strong(),
                );
            });
        let mut reset = reset_button(ui);
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

fn reset_button(ui: &mut Ui) -> egui::Response {
    let image = egui::Image::new(egui::include_image!("../../resources/icons/reset.svg"))
        .fit_to_exact_size(Vec2::splat(14.0))
        .tint(crate::ui::theme::text_normal());
    ui.add(
        egui::Button::image(image)
            .min_size(Vec2::splat(28.0))
            .corner_radius(CornerRadius::same(10)),
    )
}
