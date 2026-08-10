use crate::ui::components::{self, SubNavItem, section, sub_sidebar};
use eframe::egui;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SettingsSection {
    GeneralAppearance,
    ServiceProviders,
    OscNetwork,
    AudioIntegration,
    BackendServer,
}

impl Default for SettingsSection {
    fn default() -> Self {
        Self::GeneralAppearance
    }
}

pub fn render(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new(crate::i18n::tr(app.ui_language, "Settings"))
            .size(22.0)
            .color(crate::ui::theme::text_strong())
            .strong(),
    );
    ui.add_space(12.0);

    let nav_items = [
        SubNavItem {
            id: SettingsSection::GeneralAppearance,
            icon: "",
            label: crate::i18n::tr(app.ui_language, "General & Appearance"),
        },
        SubNavItem {
            id: SettingsSection::ServiceProviders,
            icon: "",
            label: crate::i18n::tr(app.ui_language, "Service Providers"),
        },
        SubNavItem {
            id: SettingsSection::AudioIntegration,
            icon: "",
            label: crate::i18n::tr(app.ui_language, "Audio & Integration"),
        },
        SubNavItem {
            id: SettingsSection::OscNetwork,
            icon: "",
            label: crate::i18n::tr(app.ui_language, "VRChat OSC"),
        },
        SubNavItem {
            id: SettingsSection::BackendServer,
            icon: "",
            label: crate::i18n::tr(app.ui_language, "Local Service"),
        },
    ];

    ui.horizontal_top(|ui| {
        // 1. Modular Reusable Secondary Left Sub-Sidebar (Section Navigation)
        sub_sidebar(ui, &mut app.settings_section, &nav_items, app.ui_language);

        ui.add_space(12.0);

        // 2. Section Content View Area (Vertical Column)
        ui.vertical(|ui| {
            ui.set_min_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_salt("settings_scroll_area")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let cur = app.settings_section;

                    crate::ui::animation::AnimationSystem::render_animated_page(
                        ui,
                        cur,
                        |ui| match cur {
                            SettingsSection::GeneralAppearance => {
                                render_general_appearance_section(app, ui);
                            }
                            SettingsSection::ServiceProviders => {
                                let project_root = app.project_root();
                                app.service_config.render(
                                    ui,
                                    &mut app.backend_manager,
                                    &mut app.model_task_manager,
                                    &project_root,
                                    app.ui_language,
                                );
                            }
                            SettingsSection::OscNetwork => {
                                render_osc_network_section(app, ui);
                            }
                            SettingsSection::AudioIntegration => {
                                render_audio_integration_section(app, ui);
                            }
                            SettingsSection::BackendServer => {
                                render_server_section(app, ui);
                            }
                        },
                    );
                });
        });
    });
}

fn render_general_appearance_section(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    section(
        ui,
        crate::i18n::tr(app.ui_language, "Application Language"),
        |ui| {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr(app.ui_language, "Application Language"));
                if components::language_selector(ui, "settings_ui_language", &mut app.ui_language) {
                    app.set_ui_language(app.ui_language);
                }
            });
        },
    );
    ui.add_space(14.0);

    section(
        ui,
        crate::i18n::tr(app.ui_language, "About & Open Source"),
        |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(crate::i18n::tr(app.ui_language, "App Version:"))
                        .color(crate::ui::theme::text_strong())
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(crate::version::version_display_string())
                        .color(crate::ui::theme::text_normal()),
                );
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(crate::i18n::tr(app.ui_language, "GitHub Repository:"))
                        .color(crate::ui::theme::text_strong())
                        .strong(),
                );
                ui.add_space(4.0);
                ui.hyperlink_to(
                    "https://github.com/NowLoadY/XRTranslate",
                    "https://github.com/NowLoadY/XRTranslate",
                );
            });
        },
    );
}

fn render_server_section(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    section(
        ui,
        crate::i18n::tr(app.ui_language, "Local Service & Models"),
        |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr(
                    app.ui_language,
                    "XRTranslate starts the local service when you begin translating. Leave this empty to use the service included with the app.",
                ))
                .color(crate::ui::theme::text_weak())
                .size(12.0),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr(app.ui_language, "llama-server path:"));
                let path_changed = components::file_path_input(
                    ui,
                    &mut app.backend_manager.llama_server_path,
                    crate::i18n::tr(app.ui_language, "Choose llama-server.exe"),
                    crate::i18n::tr(app.ui_language, "Browse…"),
                    "llama-server",
                    &["exe"],
                    (ui.available_width() - 170.0).max(160.0),
                );
                if path_changed && app.backend_manager.llama_server_path_is_valid() {
                    match app.backend_manager.save_llama_server_path() {
                        Ok(()) => app.last_error = None,
                        Err(error) => app.last_error = Some(error),
                    }
                }
            });
        },
    );

    ui.add_space(14.0);

    section(
        ui,
        crate::i18n::tr(app.ui_language, "Translation Server Endpoint"),
        |ui| {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr(app.ui_language, "Server URL:"));
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut app.server_url)
                            .desired_width((ui.available_width() - 100.0).clamp(240.0, 360.0)),
                    )
                    .changed()
                {
                    app.save_settings();
                }
            });
        },
    );
}

fn render_osc_network_section(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    section(
        ui,
        crate::i18n::tr(app.ui_language, "VRChat OSC Network & Rules"),
        |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::tr(app.ui_language, "Listener Status:"));
                let status = app.osc_manager.listener_status();
                ui.label(
                    egui::RichText::new(status)
                        .color(crate::ui::theme::text_weak())
                        .size(12.0),
                );
            });

            ui.add_space(10.0);

            ui.horizontal_wrapped(|ui| {
                ui.label("IP:");
                ui.add(egui::TextEdit::singleline(&mut app.osc_draft.ip).desired_width(110.0));

                ui.add_space(16.0);
                ui.label(crate::i18n::tr(app.ui_language, "Send Port:"));
                ui.add(egui::DragValue::new(&mut app.osc_draft.send_port).range(1..=u16::MAX));

                ui.add_space(16.0);
                ui.label(crate::i18n::tr(app.ui_language, "Listen Port:"));
                ui.add(egui::DragValue::new(&mut app.osc_draft.listen_port).range(1..=u16::MAX));
            });

            ui.add_space(10.0);

            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::tr(app.ui_language, "Character Limit:"));
                ui.add(egui::DragValue::new(&mut app.osc_draft.max_text_length).range(1..=10_000));
            });

            ui.add_space(10.0);

            components::modern_slider_f64(
                ui,
                &mut app.osc_draft.history_ttl_seconds,
                10.0..=20.0,
                crate::i18n::tr(app.ui_language, "History TTL:"),
                "s",
            );

            ui.add_space(12.0);

            if components::animated_button(
                ui,
                crate::i18n::tr(app.ui_language, "Apply & Restart Listener"),
            )
            .clicked()
            {
                match app.osc_manager.update_settings(app.osc_draft.clone()) {
                    Ok(()) => app.last_error = None,
                    Err(error) => app.last_error = Some(error),
                }
                app.save_settings();
            }
        },
    );
}

fn render_audio_integration_section(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    section(
        ui,
        crate::i18n::tr(app.ui_language, "Audio & Integration"),
        |ui| {
            let mut tts_enabled = app.tts_enabled;
            if ui
                .checkbox(
                    &mut tts_enabled,
                    crate::i18n::tr(app.ui_language, "Enable TTS Audio Playback"),
                )
                .changed()
            {
                app.set_tts_enabled(tts_enabled);
            }

            ui.add_space(8.0);

            let mut gate_enabled = app.mute_self_pauses_translation.load(Ordering::Acquire);
            if ui
                .checkbox(
                    &mut gate_enabled,
                    crate::i18n::tr(
                        app.ui_language,
                        "Pause translation when VRChat microphone is muted (/MuteSelf)",
                    ),
                )
                .changed()
            {
                app.mute_self_pauses_translation
                    .store(gate_enabled, Ordering::Release);
                app.save_settings();
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);

            let mut floating_enabled = app.floating_subtitles_enabled;
            if ui
                .checkbox(
                    &mut floating_enabled,
                    crate::i18n::tr(app.ui_language, "Enable Desktop Floating Subtitle Window"),
                )
                .changed()
            {
                app.floating_subtitles_enabled = floating_enabled;
                app.save_settings();
            }

            if app.floating_subtitles_enabled {
                ui.add_space(8.0);
                components::modern_slider_f64(
                    ui,
                    &mut app.floating_subtitles_font_size,
                    10.0..=24.0,
                    crate::i18n::tr(app.ui_language, "Font Size:"),
                    "px",
                );
            }
        },
    );
}
