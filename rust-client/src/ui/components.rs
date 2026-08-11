use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Stroke, Ui, Vec2};

pub fn card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(Color32::WHITE)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(18))
        .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
        .show(ui, add_contents)
        .inner
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
            .corner_radius(CornerRadius::same(8));

    let response = ui.add_enabled(enabled, button);
    ui.memory_mut(|m| m.data.insert_temp(id, response.hovered()));
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
        .corner_radius(CornerRadius::same(8));

    let response = ui.add_enabled(enabled, button);
    ui.memory_mut(|m| m.data.insert_temp(id, response.hovered()));
    response
}

/// Shared language selector used by settings and first-run onboarding.
/// Returns whether the selected language changed.
pub fn language_selector(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    language: &mut crate::i18n::UiLanguage,
) -> bool {
    let previous = *language;
    egui::ComboBox::from_id_salt(id)
        .selected_text(language.display_name())
        .width(104.0)
        .show_ui(ui, |ui| {
            for candidate in crate::i18n::UiLanguage::ALL {
                ui.selectable_value(language, candidate, candidate.display_name());
            }
        });
    *language != previous
}

pub fn danger_button(ui: &mut Ui, text: &str) -> egui::Response {
    danger_button_enabled(ui, text, true)
}

pub fn danger_button_enabled(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    let button = egui::Button::new(egui::RichText::new(text).color(Color32::WHITE).strong())
        .fill(if enabled {
            Color32::from_rgb(220, 38, 38)
        } else {
            Color32::from_rgb(254, 202, 202)
        })
        .min_size(Vec2::new(90.0, 32.0))
        .corner_radius(CornerRadius::same(8));
    ui.add_enabled(enabled, button)
}

/// Adds a widget controlled by feature availability.
pub fn feature_widget<W: egui::Widget>(
    ui: &mut Ui,
    feature: crate::feature_access::Feature,
    language: crate::i18n::UiLanguage,
    widget: W,
) -> egui::Response {
    let access = crate::feature_access::access(feature);
    decorate_unavailable(ui.add_enabled(access.available, widget), access, language)
}

/// Adds a feature-aware checkbox.
pub fn feature_checkbox(
    ui: &mut Ui,
    feature: crate::feature_access::Feature,
    language: crate::i18n::UiLanguage,
    checked: &mut bool,
    text: &str,
) -> egui::Response {
    feature_widget(ui, feature, language, egui::Checkbox::new(checked, text))
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

    if animated_button(ui, browse_label).clicked() {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(filter_name, extensions)
            .pick_file()
        {
            *value = path.display().to_string();
            changed = true;
        }
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
            Color32::from_rgb(100, 116, 139),
            "",
        )
    };

    Frame::new()
        .fill(bg_color)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("{dot}{status}"))
                    .color(fg_color)
                    .size(12.0)
                    .strong(),
            );
        });
}

#[allow(dead_code)]
pub fn muted_label(ui: &mut Ui, muted: bool, language: crate::i18n::UiLanguage) {
    let (text, color) = if muted {
        (
            crate::i18n::tr(language, "VRChat Muted"),
            Color32::from_rgb(217, 119, 6),
        )
    } else {
        (
            crate::i18n::tr(language, "VRChat Active"),
            crate::ui::theme::text_weak(),
        )
    };
    ui.label(egui::RichText::new(text).color(color));
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
        .corner_radius(CornerRadius::same(10))
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
                            Color32::from_rgb(226, 232, 240),
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
                        .corner_radius(CornerRadius::same(9))
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
    label: &str,
    suffix: &str,
) -> egui::Response {
    ui.horizontal(|ui| {
        if !label.is_empty() {
            ui.allocate_ui_with_layout(
                Vec2::new(110.0, 20.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(
                        egui::RichText::new(label)
                            .color(crate::ui::theme::text_strong())
                            .strong(),
                    );
                },
            );
        }

        let slider = egui::Slider::new(value, range)
            .show_value(false)
            .step_by(0.5)
            .trailing_fill(true);

        let response = ui.add(slider);

        let value_text = format!("{:.1}{}", *value, suffix);
        ui.add_space(4.0);
        Frame::new()
            .fill(Color32::from_rgb(241, 245, 249))
            .corner_radius(CornerRadius::same(6))
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

        response
    })
    .inner
}
