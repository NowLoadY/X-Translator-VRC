use crate::ui::components::{self, SubNavItem, section, sub_sidebar};
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub enum SettingsSection {
    #[default]
    GeneralAppearance,
    ServiceProviders,
    OscNetwork,
    BackendServer,
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
            label: crate::i18n::tr(app.ui_language, "General"),
        },
        SubNavItem {
            id: SettingsSection::ServiceProviders,
            icon: "",
            label: crate::i18n::tr(app.ui_language, "Service Providers"),
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
        sub_sidebar(ui, &mut app.settings_section, &nav_items, app.ui_language);

        ui.add_space(12.0);

        ui.vertical(|ui| {
            ui.set_min_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_salt("settings_scroll_area")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let cur = app.settings_section;

                    crate::ui::animation::AnimationSystem::render_animated_page(ui, cur, |ui| {
                        match cur {
                            SettingsSection::GeneralAppearance => {
                                render_general_appearance_section(app, ui);
                            }
                            SettingsSection::ServiceProviders => {
                                let project_root = app.project_root();
                                let apply = app.service_config.render(
                                    ui,
                                    &mut app.backend_manager,
                                    &mut app.model_task_manager,
                                    &project_root,
                                    app.ui_language,
                                );
                                if apply {
                                    app.apply_service_configuration(Some(ui.ctx().clone()));
                                }
                            }
                            SettingsSection::OscNetwork => {
                                render_osc_network_section(app, ui);
                            }
                            SettingsSection::BackendServer => {
                                render_server_section(app, ui);
                            }
                        }
                    });
                });
        });
    });
}

fn render_general_appearance_section(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    section(ui, crate::i18n::tr(app.ui_language, "Language"), |ui| {
        if components::language_selector(ui, "settings_ui_language", &mut app.ui_language) {
            app.set_ui_language(app.ui_language);
        }
    });
    ui.add_space(14.0);

    section(ui, crate::i18n::tr(app.ui_language, "Downloads"), |ui| {
        ui.label(
            egui::RichText::new(crate::i18n::tr(app.ui_language, "Download proxy"))
                .color(crate::ui::theme::text_strong())
                .strong(),
        );
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.download_proxy_url)
                .hint_text(crate::i18n::tr(
                    app.ui_language,
                    "Optional, e.g. http://127.0.0.1:7890",
                ))
                .desired_width(300.0),
        );
        if response.lost_focus() && response.changed() {
            app.set_download_proxy_url(app.download_proxy_url.clone());
        }
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(crate::i18n::tr(
                app.ui_language,
                "Used only for updates and downloads. Leave empty when your VPN uses global mode.",
            ))
            .size(12.0)
            .color(crate::ui::theme::text_weak()),
        );
    });
    ui.add_space(14.0);

    section(ui, crate::i18n::tr(app.ui_language, "About"), |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr(app.ui_language, "Version:"))
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
                egui::RichText::new("GitHub:")
                    .color(crate::ui::theme::text_strong())
                    .strong(),
            );
            ui.add_space(4.0);
            ui.hyperlink_to(
                "https://github.com/NowLoadY/XRTranslate",
                "https://github.com/NowLoadY/XRTranslate",
            );
        });
        ui.add_space(14.0);
        render_update_controls(app, ui);
    });
}

fn render_update_controls(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    use crate::app_update::AppUpdateState;

    let state = app.app_update_state().clone();
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Update")
                .color(crate::ui::theme::text_strong())
                .strong(),
        );
        ui.add_space(4.0);
        let status = match &state {
            AppUpdateState::Idle => "Ready",
            AppUpdateState::Checking => "Checking for updates...",
            AppUpdateState::Current => "You're up to date",
            AppUpdateState::Available(_) => "Update available",
            AppUpdateState::Downloading { .. } => "Downloading",
            AppUpdateState::Ready(_) => "Ready to install",
            AppUpdateState::Installing => "Installing...",
            AppUpdateState::Failed(_) => "Check failed",
        };
        ui.label(egui::RichText::new(status).color(crate::ui::theme::text_weak()));
        match &state {
            AppUpdateState::Available(info) | AppUpdateState::Ready(info) => {
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(format!("v{}", info.version))
                            .color(crate::ui::theme::text_strong())
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(info.asset_name.as_str())
                            .size(12.0)
                            .color(crate::ui::theme::text_weak()),
                    );
                });
            }
            AppUpdateState::Downloading {
                downloaded, total, ..
            } => {
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{} / {}",
                        components::format_file_size(*downloaded),
                        components::format_file_size(*total)
                    ))
                    .color(crate::ui::theme::text_strong()),
                );
            }
            _ => {}
        }
    });

    if let AppUpdateState::Failed(error) = &state {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(error)
                .size(12.0)
                .color(egui::Color32::from_rgb(220, 38, 38)),
        );
    }

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let busy = app.app_update_manager.is_busy();
        let primary_label = match &state {
            AppUpdateState::Ready(_) => "Install and Restart",
            AppUpdateState::Available(_) => "Download Update",
            AppUpdateState::Checking => "Checking...",
            AppUpdateState::Downloading { .. } => "Downloading...",
            AppUpdateState::Installing => "Installing...",
            AppUpdateState::Current => "Check for Updates",
            AppUpdateState::Failed(_) => "Try Again",
            AppUpdateState::Idle => "Check for Updates",
        };

        if components::primary_button_enabled(ui, primary_label, !busy).clicked() {
            match &state {
                AppUpdateState::Ready(_) => app.install_update_and_restart(),
                AppUpdateState::Available(_) => app.download_update(),
                AppUpdateState::Current | AppUpdateState::Idle | AppUpdateState::Failed(_) => {
                    app.check_for_updates()
                }
                AppUpdateState::Checking
                | AppUpdateState::Downloading { .. }
                | AppUpdateState::Installing => {}
            }
        }
    });
}

fn render_server_section(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    section(ui, crate::i18n::tr(app.ui_language, "Backend"), |ui| {
        ui.horizontal(|ui| {
            ui.label("llama-server:");
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
    });

    ui.add_space(14.0);

    section(ui, crate::i18n::tr(app.ui_language, "Server"), |ui| {
        ui.horizontal(|ui| {
            ui.label("URL:");
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
    });
}

fn render_osc_network_section(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    section(ui, crate::i18n::tr(app.ui_language, "OSC Network"), |ui| {
        components::feature_ui(
            ui,
            crate::feature_access::Feature::OscChatbox,
            app.ui_language,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::i18n::tr(app.ui_language, "Status:"));
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
                    ui.add(
                        egui::DragValue::new(&mut app.osc_draft.listen_port).range(1..=u16::MAX),
                    );
                });

                ui.add_space(10.0);

                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::i18n::tr(app.ui_language, "Limit:"));
                    ui.add(
                        egui::DragValue::new(&mut app.osc_draft.max_text_length).range(1..=10_000),
                    );
                });

                ui.add_space(10.0);

                components::modern_slider_f64(
                    ui,
                    &mut app.osc_draft.history_ttl_seconds,
                    10.0..=20.0,
                    15.0,
                    crate::i18n::tr(app.ui_language, "History TTL:"),
                    "s",
                );

                ui.add_space(12.0);

                if components::animated_button(ui, crate::i18n::tr(app.ui_language, "Apply"))
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
    });
}
