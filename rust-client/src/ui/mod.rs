pub mod animation;
pub mod components;
pub mod modal;
pub mod pages;
pub mod theme;

use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default, Serialize, Deserialize)]
pub enum Page {
    #[default]
    Translation,
    Osc,
    Settings,
}

pub struct NavigationState {
    pub collapsed: bool,
    pub page: Page,
}

impl Default for NavigationState {
    fn default() -> Self {
        Self {
            collapsed: false,
            page: Page::Translation,
        }
    }
}

pub fn render_onboarding_fullscreen(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    const STEPS: [&str; 3] = ["Welcome", "Get llama.cpp", "Install models"];

    let total_pages = STEPS.len();
    if app.onboarding_page >= total_pages {
        app.onboarding_page = 0;
    }
    let requirement = onboarding_requirement(app);
    let can_advance = requirement.is_none();

    egui::Panel::bottom("onboarding_bottom_nav")
        .frame(
            Frame::new()
                .fill(Color32::from_rgb(244, 248, 255))
                .inner_margin(Margin::symmetric(36, 14))
                .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240))),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if app.onboarding_page > 0
                    && components::animated_button(ui, crate::i18n::tr(app.ui_language, "Back"))
                        .clicked()
                {
                    app.onboarding_page -= 1;
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if app.onboarding_page + 1 == total_pages {
                        if components::primary_button_enabled(
                            ui,
                            crate::i18n::tr(app.ui_language, "Open Translation"),
                            can_advance,
                        )
                        .clicked()
                        {
                            app.finish_onboarding();
                        }
                    } else if components::primary_button_enabled(
                        ui,
                        crate::i18n::tr(app.ui_language, "Continue"),
                        can_advance,
                    )
                    .clicked()
                    {
                        app.onboarding_page += 1;
                    }

                    if let Some(requirement) = requirement {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(crate::i18n::tr(app.ui_language, requirement))
                                .size(11.0)
                                .color(theme::text_weak()),
                        );
                    }
                });
            });
        });

    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(Color32::from_rgb(244, 248, 255))
                .inner_margin(Margin::symmetric(36, 20)),
        )
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("XRTranslate")
                                .size(14.0)
                                .color(Color32::from_rgb(37, 99, 235))
                                .strong(),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(crate::i18n::tr(
                                app.ui_language,
                                "A calm start, one step at a time",
                            ))
                            .size(22.0)
                            .color(theme::text_strong())
                            .strong(),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let language_changed = components::language_selector(
                            ui,
                            "onboarding_ui_language",
                            &mut app.ui_language,
                        );
                        if language_changed {
                            app.set_ui_language(app.ui_language);
                        }
                        ui.add_space(10.0);
                        components::status_badge(
                            ui,
                            &format!(
                                "{} {}/{}",
                                crate::i18n::tr(app.ui_language, "Step"),
                                app.onboarding_page + 1,
                                total_pages
                            ),
                            true,
                            false,
                        );
                    });
                });

                ui.add_space(14.0);

                render_onboarding_steps(ui, app.ui_language, app.onboarding_page, &STEPS);

                ui.add_space(14.0);

                Frame::new()
                    .fill(Color32::WHITE)
                    .corner_radius(CornerRadius::same(20))
                    .inner_margin(Margin::symmetric(28, 24))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(219, 230, 246)))
                    .show(ui, |ui| {
                        ui.set_height(ui.available_height());
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical()
                            .id_salt("onboarding_content_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| match app.onboarding_page {
                                0 => render_onboarding_welcome(app.ui_language, ui),
                                1 => render_onboarding_runtime(app, ui),
                                _ => render_onboarding_models(app, ui),
                            });
                    });
            });
        });
}

/// Returns the current page's unmet prerequisite. The footer uses this one
/// source of truth to avoid letting users enter a partly configured session.
fn onboarding_requirement(app: &crate::XRTranslateApp) -> Option<&'static str> {
    match app.onboarding_page {
        1 => {
            if app.backend_manager.llama_server_path_is_valid() {
                None
            } else {
                Some("Choose an existing llama-server.exe to continue.")
            }
        }
        2 => {
            if app.model_task_manager.is_busy() {
                return Some("Wait for the current model task to finish.");
            }
            let packages =
                match crate::model_install::configured_model_packages(&app.project_root()) {
                    Ok(packages) => packages,
                    Err(_) => return Some("Install every required model package to continue."),
                };
            if !packages.is_empty()
                && packages
                    .iter()
                    .all(|package| app.model_task_manager.is_model_present(package.id))
            {
                None
            } else {
                Some("Install every required model package to continue.")
            }
        }
        _ => None,
    }
}

fn render_onboarding_steps(
    ui: &mut egui::Ui,
    language: crate::i18n::UiLanguage,
    active_step: usize,
    steps: &[&'static str],
) {
    ui.horizontal(|ui| {
        for (index, step) in steps.iter().enumerate() {
            let active = index == active_step;
            let complete = index < active_step;
            let (fill, text) = if active {
                (Color32::from_rgb(59, 130, 246), Color32::WHITE)
            } else if complete {
                (
                    Color32::from_rgb(239, 246, 255),
                    Color32::from_rgb(37, 99, 235),
                )
            } else {
                (
                    Color32::from_rgb(245, 248, 252),
                    Color32::from_rgb(148, 163, 184),
                )
            };

            let check_mark = if complete { "✓ " } else { "" };

            Frame::new()
                .fill(fill)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(14, 7))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}{}{}",
                            check_mark,
                            if !complete {
                                format!("{}  ", index + 1)
                            } else {
                                "".to_string()
                            },
                            crate::i18n::tr(language, step)
                        ))
                        .size(12.5)
                        .color(text)
                        .strong(),
                    );
                });

            if index + 1 < steps.len() {
                ui.add_space(2.0);
                ui.label(
                    RichText::new("›")
                        .size(14.0)
                        .color(Color32::from_rgb(203, 218, 235)),
                );
                ui.add_space(2.0);
            }
        }
    });
}

fn onboarding_title(
    ui: &mut egui::Ui,
    language: crate::i18n::UiLanguage,
    title: &'static str,
    body: Option<&'static str>,
) {
    ui.label(
        RichText::new(crate::i18n::tr(language, title))
            .size(28.0)
            .color(theme::text_strong())
            .strong(),
    );
    if let Some(subtitle) = body {
        ui.add_space(6.0);
        ui.label(
            RichText::new(crate::i18n::tr(language, subtitle))
                .size(14.0)
                .color(theme::text_weak()),
        );
    }
    ui.add_space(20.0);
}

fn onboarding_feature_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &'static str,
    tint: Color32,
    language: crate::i18n::UiLanguage,
) {
    Frame::new()
        .fill(tint)
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(20))
        .stroke(Stroke::new(1.0, Color32::from_black_alpha(12)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(82.0);
            ui.label(
                RichText::new(icon)
                    .size(22.0)
                    .strong()
                    .color(Color32::from_rgb(37, 99, 235)),
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new(crate::i18n::tr(language, title))
                    .size(14.5)
                    .color(theme::text_strong())
                    .strong(),
            );
        });
}

fn render_onboarding_welcome(language: crate::i18n::UiLanguage, ui: &mut egui::Ui) {
    onboarding_title(ui, language, "Welcome to XRTranslate", None);
    ui.columns(3, |columns| {
        onboarding_feature_card(
            &mut columns[0],
            "01",
            "Audio Input",
            Color32::from_rgb(239, 246, 255),
            language,
        );
        onboarding_feature_card(
            &mut columns[1],
            "02",
            "Recognition & Translation",
            Color32::from_rgb(240, 253, 250),
            language,
        );
        onboarding_feature_card(
            &mut columns[2],
            "03",
            "VRChat OSC",
            Color32::from_rgb(255, 247, 237),
            language,
        );
    });
}

fn render_onboarding_runtime(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    use crate::runtime_install::RuntimeInstallState;

    let language = app.ui_language;
    let runtime_is_available = app.backend_manager.llama_server_path_is_valid();
    if !runtime_is_available
        && matches!(app.runtime_installer.state(), RuntimeInstallState::Idle)
        && let Err(error) = app
            .runtime_installer
            .prepare_recommended(app.project_root())
    {
        app.last_error = Some(error);
    }
    onboarding_title(ui, language, "Download llama.cpp", None);

    // Option A: Automatic Setup
    Frame::new()
        .fill(Color32::from_rgb(240, 253, 250))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(18))
        .stroke(Stroke::new(1.0, Color32::from_rgb(167, 243, 208)))
        .show(ui, |ui| {
            ui.label(
                RichText::new(crate::i18n::tr(
                    language,
                    "Option A: Automatic Setup (Recommended)",
                ))
                .size(14.5)
                .color(Color32::from_rgb(4, 120, 87))
                .strong(),
            );
            ui.add_space(12.0);
            let download_size = app.runtime_installer.download_size_bytes();
            let backend = app.runtime_installer.backend_label().unwrap_or_default();
            let action = if matches!(
                app.runtime_installer.state(),
                RuntimeInstallState::Failed(_)
            ) {
                crate::i18n::tr(language, "Retry")
            } else {
                crate::i18n::tr(language, "Download")
            };
            let button_label = if runtime_is_available {
                crate::i18n::tr(language, "Installed").to_owned()
            } else {
                download_size.map_or_else(
                    || crate::i18n::tr(language, "Preparing download…").to_owned(),
                    |bytes| {
                        format!(
                            "{action} {backend} · {}",
                            components::format_file_size(bytes)
                        )
                    },
                )
            };
            if components::primary_button_enabled(
                ui,
                &button_label,
                !runtime_is_available
                    && download_size.is_some()
                    && !app.runtime_installer.is_busy(),
            )
            .clicked()
                && let Err(error) = app
                    .runtime_installer
                    .install_recommended(app.project_root())
            {
                app.last_error = Some(error);
            }
            if runtime_is_available {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(
                        language,
                        "llama.cpp is installed and ready.",
                    ))
                    .size(12.0)
                    .color(Color32::from_rgb(5, 150, 105)),
                );
            } else {
                match app.runtime_installer.state() {
                    RuntimeInstallState::Idle | RuntimeInstallState::Ready => {}
                    RuntimeInstallState::Detecting => {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(crate::i18n::tr(
                                language,
                                "Detecting the recommended runtime...",
                            ))
                            .size(12.0)
                            .color(theme::text_weak()),
                        );
                    }
                    RuntimeInstallState::Downloading {
                        asset,
                        downloaded,
                        total,
                    } => {
                        ui.add_space(6.0);
                        ui.label(RichText::new(asset).size(11.0).color(theme::text_weak()));
                        if *total > 0 {
                            ui.add(
                                egui::ProgressBar::new(
                                    (*downloaded as f64 / *total as f64).clamp(0.0, 1.0) as f32,
                                )
                                .text(format!(
                                    "{} / {}",
                                    components::format_file_size(*downloaded),
                                    components::format_file_size(*total),
                                )),
                            );
                        }
                    }
                    RuntimeInstallState::Extracting => {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(crate::i18n::tr(language, "Extracting llama.cpp..."))
                                .size(12.0)
                                .color(theme::text_weak()),
                        );
                    }
                    RuntimeInstallState::Installed(path) => {
                        ui.add_space(6.0);
                        let installed_path = path.display().to_string();
                        if app.backend_manager.llama_server_path != installed_path {
                            app.backend_manager.adopt_installed_llama_server_path(path);
                        }
                        ui.label(
                            RichText::new(crate::i18n::tr(
                                language,
                                "llama.cpp is installed and ready.",
                            ))
                            .size(12.0)
                            .color(Color32::from_rgb(5, 150, 105)),
                        );
                    }
                    RuntimeInstallState::Failed(error) => {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(error)
                                .size(12.0)
                                .color(Color32::from_rgb(220, 38, 38)),
                        );
                    }
                }
            }
        });

    ui.add_space(14.0);

    // Option B: Manual Setup Card
    Frame::new()
        .fill(Color32::from_rgb(248, 250, 252))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(18))
        .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
        .show(ui, |ui| {
            ui.label(
                RichText::new(crate::i18n::tr(language, "Option B: Install Manually"))
                    .size(14.5)
                    .color(theme::text_strong())
                    .strong(),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(crate::i18n::tr(
                    language,
                    "If automatic download fails or you prefer using an existing llama.cpp build:",
                ))
                .size(12.0)
                .color(theme::text_weak()),
            );

            ui.add_space(12.0);

            // Step 1: Download
            ui.label(
                RichText::new(crate::i18n::tr(
                    language,
                    "1. Download the right package manually",
                ))
                .size(13.0)
                .color(theme::text_strong())
                .strong(),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(crate::i18n::tr(
                    language,
                    "NVIDIA graphics: download the matching llama-...-bin-win-cuda-...-x64.zip and cudart-llama-bin-win-cuda-...-x64.zip from the same release and CUDA version.",
                ))
                .size(12.0)
                .color(theme::text_normal()),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(crate::i18n::tr(
                    language,
                    "CPU only: download llama-...-bin-win-cpu-x64.zip.",
                ))
                .size(12.0)
                .color(theme::text_normal()),
            );
            ui.add_space(8.0);
            match crate::runtime_install::configured_release_page(&app.project_root()) {
                Ok(release_page) => {
                    ui.hyperlink_to(
                        crate::i18n::tr(language, "Open llama.cpp downloads"),
                        release_page,
                    );
                }
                Err(error) => {
                    ui.label(RichText::new(error).size(11.0).color(Color32::from_rgb(220, 38, 38)));
                }
            }

            ui.add_space(12.0);

            // Step 2: Extract & keep together
            ui.label(
                RichText::new(crate::i18n::tr(language, "2. Keep the runtime together"))
                    .size(13.0)
                    .color(theme::text_strong())
                    .strong(),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(crate::i18n::tr(
                    language,
                    "Extract every file into one folder, for example D:\\llama.cpp. With NVIDIA, llama-server.exe and cudart64_*.dll must stay in that same folder.",
                ))
                .size(12.0)
                .color(theme::text_normal()),
            );
        });

    ui.add_space(14.0);

    // Selected Path Executable Card
    Frame::new()
        .fill(Color32::WHITE)
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(18))
        .stroke(Stroke::new(1.0, Color32::from_rgb(219, 230, 246)))
        .show(ui, |ui| {
            ui.label(
                RichText::new(crate::i18n::tr(language, "Select llama-server.exe"))
                    .size(14.0)
                    .color(theme::text_strong())
                    .strong(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let path_changed = components::file_path_input(
                    ui,
                    &mut app.backend_manager.llama_server_path,
                    crate::i18n::tr(language, "Choose llama-server.exe"),
                    crate::i18n::tr(language, "Browse…"),
                    "llama-server",
                    &["exe"],
                    (ui.available_width() - 94.0).max(180.0),
                );
                if path_changed && app.backend_manager.llama_server_path_is_valid() {
                    match app.backend_manager.save_llama_server_path() {
                        Ok(()) => app.last_error = None,
                        Err(error) => app.last_error = Some(error),
                    }
                }
            });
            if let Some(error) = &app.last_error {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(error)
                        .size(12.0)
                        .color(Color32::from_rgb(220, 38, 38)),
                );
            }
        });
}

fn render_onboarding_models(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    use crate::model_install::{
        NativeModelTaskState, configured_model_packages, model_level_packages, set_model_level,
    };

    let language = app.ui_language;
    onboarding_title(ui, language, "Install your model packages", None);
    let project_root = app.project_root();
    if app.model_task_manager.needs_discovery()
        && let Err(error) = app
            .model_task_manager
            .discover_existing(project_root.clone())
    {
        app.last_error = Some(error);
    }
    let busy = app.model_task_manager.is_busy();
    let packages = match configured_model_packages(&project_root) {
        Ok(packages) => packages,
        Err(error) => {
            app.last_error = Some(error);
            Vec::new()
        }
    };
    let mut install = None;
    let mut level_change = None;
    let retry = matches!(
        app.model_task_manager.state(),
        NativeModelTaskState::Failed(_)
    );
    ui.columns(packages.len().max(1), |columns| {
        for (index, package) in packages.iter().enumerate() {
            let installed = app.model_task_manager.is_model_present(package.id);
            let (download_clicked, selected_level) = onboarding_model_card(
                &mut columns[index],
                language,
                OnboardingModelCard {
                    title: package.label,
                    selected_level: package.level,
                    levels: &model_level_packages(package.capability),
                    action: if installed {
                        "Installed"
                    } else if retry {
                        "Retry"
                    } else {
                        "Download"
                    },
                    enabled: !busy && !installed,
                    download_bytes: (!installed).then_some(package.download_bytes),
                    tint: if index % 2 == 0 {
                        Color32::from_rgb(239, 246, 255)
                    } else {
                        Color32::from_rgb(240, 253, 250)
                    },
                },
            );
            if download_clicked {
                install = Some(package.id);
            }
            if let Some(level) = selected_level {
                level_change = Some((package.capability, level));
            }
        }
    });
    if let Some((capability, level)) = level_change {
        install = None;
        match set_model_level(&project_root, capability, level) {
            Ok(()) => {
                app.model_task_manager.invalidate_discovery();
                app.backend_manager.shutdown();
                app.last_error = None;
            }
            Err(error) => app.last_error = Some(error),
        }
    }
    if let Some(asset_id) = install
        && let Err(error) = app
            .model_task_manager
            .install(project_root.clone(), asset_id)
    {
        app.last_error = Some(error);
    }
    ui.add_space(16.0);
    if components::animated_button(ui, crate::i18n::tr(language, "Verify models")).clicked()
        && let Err(error) = app.model_task_manager.verify(project_root)
    {
        app.last_error = Some(error);
    }
    ui.add_space(10.0);
    match app.model_task_manager.state() {
        NativeModelTaskState::Idle => ui.label(
            RichText::new(crate::i18n::tr(
                language,
                "Install both packages, then verify them here.",
            ))
            .size(12.0)
            .color(theme::text_weak()),
        ),
        NativeModelTaskState::Discovering => ui.label(
            RichText::new(crate::i18n::tr(
                language,
                "Looking for existing model packages...",
            ))
            .size(12.0)
            .color(theme::text_weak()),
        ),
        NativeModelTaskState::Detected { present, .. } => {
            let all_installed = !packages.is_empty()
                && packages.iter().all(|package| present.contains(&package.id));
            let message = if all_installed {
                "Your model packages are installed."
            } else if !present.is_empty() {
                "One model package is installed. Install the remaining package."
            } else {
                "Choose a model package to install."
            };
            ui.label(
                RichText::new(crate::i18n::tr(language, message))
                    .size(12.0)
                    .color(if !present.is_empty() {
                        Color32::from_rgb(5, 150, 105)
                    } else {
                        theme::text_weak()
                    }),
            )
        }
        NativeModelTaskState::Installing {
            downloaded_bytes,
            total_bytes,
            ..
        } => {
            let fraction = if *total_bytes == 0 {
                0.0
            } else {
                (*downloaded_bytes as f64 / *total_bytes as f64).clamp(0.0, 1.0) as f32
            };
            ui.add(egui::ProgressBar::new(fraction).text(format!(
                "{} / {}",
                components::format_file_size(*downloaded_bytes),
                components::format_file_size(*total_bytes),
            )))
        }
        NativeModelTaskState::Verifying => ui.label(
            RichText::new(crate::i18n::tr(language, "Checking your model files…"))
                .size(12.0)
                .color(theme::text_weak()),
        ),
        NativeModelTaskState::Installed { .. } => ui.label(
            RichText::new(crate::i18n::tr(language, "A model package is ready."))
                .size(12.0)
                .color(Color32::from_rgb(5, 150, 105)),
        ),
        NativeModelTaskState::Verified => ui.label(
            RichText::new(crate::i18n::tr(language, "Your model packages are ready."))
                .size(12.0)
                .color(Color32::from_rgb(5, 150, 105)),
        ),
        NativeModelTaskState::Failed(error) => ui.label(
            RichText::new(error)
                .size(12.0)
                .color(Color32::from_rgb(220, 38, 38)),
        ),
    };
}

struct OnboardingModelCard<'a> {
    title: &'static str,
    selected_level: xrtranslate_assets::ModelLevel,
    levels: &'a [crate::model_install::NativeModelPackage],
    action: &'static str,
    enabled: bool,
    download_bytes: Option<u64>,
    tint: Color32,
}

fn onboarding_model_card(
    ui: &mut egui::Ui,
    language: crate::i18n::UiLanguage,
    card: OnboardingModelCard<'_>,
) -> (bool, Option<xrtranslate_assets::ModelLevel>) {
    Frame::new()
        .fill(card.tint)
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(18))
        .stroke(Stroke::new(1.0, Color32::from_black_alpha(10)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(crate::i18n::tr(language, card.title))
                    .size(16.0)
                    .color(theme::text_strong())
                    .strong(),
            );
            ui.add_space(10.0);
            let mut level = card.selected_level;
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr(language, "Level"));
                egui::ComboBox::from_id_salt((card.title, "model_level"))
                    .selected_text(crate::i18n::tr(language, level.as_str()))
                    .show_ui(ui, |ui| {
                        for package in card.levels {
                            ui.selectable_value(
                                &mut level,
                                package.level,
                                crate::i18n::tr(language, package.level.as_str()),
                            );
                        }
                    });
            });
            ui.add_space(14.0);
            let action_label = card.download_bytes.map_or_else(
                || crate::i18n::tr(language, card.action).to_owned(),
                |bytes| {
                    format!(
                        "{} · {}",
                        crate::i18n::tr(language, card.action),
                        components::format_file_size(bytes),
                    )
                },
            );
            let clicked = ui
                .add_enabled(
                    card.enabled,
                    egui::Button::new(RichText::new(action_label).color(Color32::WHITE).strong())
                        .fill(Color32::from_rgb(37, 99, 235))
                        .min_size(egui::Vec2::new(100.0, 32.0))
                        .corner_radius(CornerRadius::same(10)),
                )
                .clicked();
            (clicked, (level != card.selected_level).then_some(level))
        })
        .inner
}

pub fn render_sidebar(
    ui: &mut egui::Ui,
    navigation: &mut NavigationState,
    modal_dialog: &mut modal::ModalDialog,
    first_run: &mut bool,
    onboarding_page: &mut usize,
    language: crate::i18n::UiLanguage,
    expand_factor: f32,
) {
    use egui::include_image;

    let icon_tr = include_image!("../../resources/icons/translation.svg");
    let icon_osc = include_image!("../../resources/icons/osc.svg");
    let icon_settings = include_image!("../../resources/icons/settings.svg");
    let icon_guide = include_image!("../../resources/icons/guide.svg");
    let icon_expand = include_image!("../../resources/icons/chevron-right.svg");
    let icon_collapse = include_image!("../../resources/icons/chevron-left.svg");

    ui.vertical(|ui| {
        ui.add_space(4.0);

        // Brand Header & Expand/Collapse Toggle
        ui.horizontal(|ui| {
            if expand_factor > 0.15 {
                let text_opacity = ((expand_factor - 0.15) / 0.85).clamp(0.0, 1.0);
                ui.scope(|ui| {
                    ui.set_opacity(text_opacity);
                    ui.label(
                        RichText::new("XRTranslate")
                            .size(16.0)
                            .color(theme::text_strong())
                            .strong(),
                    );
                });
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (toggle_icon, tooltip) = if navigation.collapsed {
                    (icon_expand, "Expand sidebar")
                } else {
                    (icon_collapse, "Collapse sidebar")
                };

                let toggle_img = egui::Image::new(toggle_icon)
                    .fit_to_exact_size(egui::vec2(14.0, 14.0))
                    .tint(theme::text_strong());

                let toggle_btn = ui
                    .add(
                        egui::Button::image(toggle_img)
                            .min_size(egui::vec2(28.0, 28.0))
                            .corner_radius(CornerRadius::same(14)),
                    )
                    .on_hover_text(crate::i18n::tr(language, tooltip));

                if toggle_btn.clicked() {
                    navigation.collapsed = !navigation.collapsed;
                }
            });
        });

        ui.add_space(16.0);

        nav_item_animated(
            ui,
            navigation,
            Page::Translation,
            icon_tr,
            crate::i18n::tr(language, "Translation"),
            expand_factor,
        );
        ui.add_space(4.0);
        nav_item_animated(
            ui,
            navigation,
            Page::Osc,
            icon_osc,
            crate::i18n::tr(language, "VRChat OSC"),
            expand_factor,
        );
        ui.add_space(4.0);
        nav_item_animated(
            ui,
            navigation,
            Page::Settings,
            icon_settings,
            crate::i18n::tr(language, "Settings"),
            expand_factor,
        );

        ui.add_space(20.0);
        components::wavy_divider_black_shadow(ui);
        ui.add_space(12.0);

        guide_button_animated(
            ui,
            modal_dialog,
            language,
            icon_guide.clone(),
            expand_factor,
        );
        ui.add_space(4.0);
        if sidebar_text_button(
            ui,
            "sidebar_welcome_btn",
            "Welcome Page",
            icon_guide,
            language,
            expand_factor,
        ) {
            *onboarding_page = 0;
            *first_run = true;
        }
    });
}

fn sidebar_text_button(
    ui: &mut egui::Ui,
    id_source: &str,
    label: &'static str,
    icon: egui::ImageSource<'static>,
    language: crate::i18n::UiLanguage,
    expand_factor: f32,
) -> bool {
    let id = ui.make_persistent_id(id_source);
    let hovered = ui.memory(|memory| memory.data.get_temp::<bool>(id.with("hover_state")).unwrap_or(false));
    let active = ui.memory(|memory| memory.data.get_temp::<bool>(id.with("active_state")).unwrap_or(false));

    let hover = animation::AnimationSystem::animate_bool(ui.ctx(), id.with("hover"), hovered, 0.15);
    let active_factor = animation::AnimationSystem::animate_bool(ui.ctx(), id.with("active"), active, 0.08);

    let bg_fill = animation::AnimationSystem::lerp_color(
        animation::AnimationSystem::lerp_color(
            Color32::TRANSPARENT,
            Color32::from_rgb(238, 244, 253),
            hover,
        ),
        Color32::from_rgb(229, 239, 255),
        active_factor,
    );

    let response = Frame::new()
        .fill(bg_fill)
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::symmetric(
            (12.0 * expand_factor + 8.0 * (1.0 - expand_factor)).round() as i8,
            8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if expand_factor < 0.2 {
                    ui.add_space(((ui.available_width() - 16.0) / 2.0).max(0.0));
                }
                ui.add(
                    egui::Image::new(icon)
                        .fit_to_exact_size(egui::vec2(16.0, 16.0))
                        .tint(theme::text_strong()),
                );
                if expand_factor > 0.1 {
                    ui.add_space(10.0 * ((expand_factor - 0.1) / 0.9).clamp(0.0, 1.0));
                    ui.label(
                        RichText::new(crate::i18n::tr(language, label))
                            .color(theme::text_strong())
                            .size(13.0),
                    );
                }
            });
        })
        .response
        .interact(egui::Sense::click());
    if expand_factor < 0.3 {
        response
            .clone()
            .on_hover_text(crate::i18n::tr(language, label));
    }
    ui.memory_mut(|memory| {
        memory.data.insert_temp(id.with("hover_state"), response.hovered());
        memory.data.insert_temp(id.with("active_state"), response.is_pointer_button_down_on());
    });
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked()
}

fn open_guide_modal(modal_dialog: &mut modal::ModalDialog, language: crate::i18n::UiLanguage) {
    *modal_dialog = modal::ModalDialog::carousel(vec![
        modal::ModalPage::new(
            crate::i18n::tr(language, "Translation"),
            crate::i18n::tr(language, "Select audio and start."),
        ),
        modal::ModalPage::new(
            crate::i18n::tr(language, "VRChat OSC"),
            crate::i18n::tr(language, "Configure chatbox output."),
        ),
        modal::ModalPage::new(
            crate::i18n::tr(language, "Settings"),
            crate::i18n::tr(language, "Install llama.cpp and models."),
        ),
    ]);
}

fn nav_item_animated(
    ui: &mut egui::Ui,
    navigation: &mut NavigationState,
    page: Page,
    icon: egui::ImageSource<'static>,
    label: &str,
    expand_factor: f32,
) {
    let is_selected = navigation.page == page;
    let id = ui.make_persistent_id(label);

    let is_hovered = ui.memory(|m| m.data.get_temp::<bool>(id.with("hover_state")).unwrap_or(false));
    let is_active = ui.memory(|m| m.data.get_temp::<bool>(id.with("active_state")).unwrap_or(false));

    let hover_factor = animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("hover"),
        is_hovered && !is_selected,
        0.15,
    );
    let active_factor = animation::AnimationSystem::animate_bool(
        ui.ctx(),
        id.with("active"),
        is_active && !is_selected,
        0.08,
    );
    let select_factor =
        animation::AnimationSystem::animate_bool(ui.ctx(), id.with("select"), is_selected, 0.20);

    let base_hover = animation::AnimationSystem::lerp_color(
        Color32::TRANSPARENT,
        Color32::from_rgb(238, 244, 253),
        hover_factor,
    );
    let base_active = animation::AnimationSystem::lerp_color(
        base_hover,
        Color32::from_rgb(229, 239, 255),
        active_factor,
    );
    let bg_fill = animation::AnimationSystem::lerp_color(
        base_active,
        Color32::from_rgb(239, 246, 255),
        select_factor,
    );

    let text_color = animation::AnimationSystem::lerp_color(
        theme::text_normal(),
        Color32::from_rgb(37, 99, 235),
        select_factor,
    );

    let inner_padding_x = (12.0 * expand_factor + 8.0 * (1.0 - expand_factor)).round();

    let frame_response = Frame::new()
        .fill(bg_fill)
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::symmetric(inner_padding_x as i8, 9))
        .stroke(Stroke::NONE)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if expand_factor < 0.2 {
                    let indent = ((ui.available_width() - 16.0) / 2.0).max(0.0);
                    ui.add_space(indent);
                }

                if select_factor > 0.05 && expand_factor > 0.2 {
                    let (bar_rect, _) =
                        ui.allocate_exact_size(egui::vec2(3.0, 14.0), egui::Sense::hover());
                    let bar_color = Color32::from_rgba_premultiplied(
                        37,
                        99,
                        235,
                        (255.0 * select_factor) as u8,
                    );
                    ui.painter()
                        .rect_filled(bar_rect, CornerRadius::same(2), bar_color);
                    ui.add_space(3.0);
                }

                ui.add(
                    egui::Image::new(icon)
                        .fit_to_exact_size(egui::vec2(16.0, 16.0))
                        .tint(text_color),
                );

                if expand_factor > 0.1 {
                    let text_opacity = ((expand_factor - 0.1) / 0.9).clamp(0.0, 1.0);
                    ui.add_space(10.0 * text_opacity);
                    ui.scope(|ui| {
                        ui.set_opacity(text_opacity);
                        let mut rt = RichText::new(label).color(text_color).size(13.5);
                        if is_selected {
                            rt = rt.strong();
                        }
                        ui.label(rt);
                    });
                }
            });
        });

    let response = frame_response.response.interact(egui::Sense::click());

    if expand_factor < 0.3 {
        response.clone().on_hover_text(label);
    }

    ui.memory_mut(|m| {
        m.data.insert_temp(id.with("hover_state"), response.hovered());
        m.data.insert_temp(id.with("active_state"), response.is_pointer_button_down_on());
    });

    if response.clicked() {
        navigation.page = page;
    }

    if response.hovered() && !is_selected {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn guide_button_animated(
    ui: &mut egui::Ui,
    modal_dialog: &mut modal::ModalDialog,
    language: crate::i18n::UiLanguage,
    icon: egui::ImageSource<'static>,
    expand_factor: f32,
) {
    let guide_id = ui.make_persistent_id("sidebar_guide_btn");
    let is_hovered = ui.memory(|m| m.data.get_temp::<bool>(guide_id.with("hover_state")).unwrap_or(false));
    let is_active = ui.memory(|m| m.data.get_temp::<bool>(guide_id.with("active_state")).unwrap_or(false));

    let hover_factor = animation::AnimationSystem::animate_bool(
        ui.ctx(),
        guide_id.with("hover"),
        is_hovered,
        0.15,
    );
    let active_factor = animation::AnimationSystem::animate_bool(
        ui.ctx(),
        guide_id.with("active"),
        is_active,
        0.08,
    );

    let base_hover = animation::AnimationSystem::lerp_color(
        Color32::TRANSPARENT,
        Color32::from_rgb(238, 244, 253),
        hover_factor,
    );
    let bg_fill = animation::AnimationSystem::lerp_color(
        base_hover,
        Color32::from_rgb(229, 239, 255),
        active_factor,
    );

    let inner_padding_x = (12.0 * expand_factor + 8.0 * (1.0 - expand_factor)).round();

    let frame_response = Frame::new()
        .fill(bg_fill)
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::symmetric(inner_padding_x as i8, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if expand_factor < 0.2 {
                    let indent = ((ui.available_width() - 16.0) / 2.0).max(0.0);
                    ui.add_space(indent);
                }

                let guide_img = egui::Image::new(icon)
                    .fit_to_exact_size(egui::vec2(16.0, 16.0))
                    .tint(theme::text_strong());
                ui.add(guide_img);

                if expand_factor > 0.1 {
                    let text_opacity = ((expand_factor - 0.1) / 0.9).clamp(0.0, 1.0);
                    ui.add_space(10.0 * text_opacity);
                    ui.scope(|ui| {
                        ui.set_opacity(text_opacity);
                        ui.label(
                            RichText::new(crate::i18n::tr(language, "User Guide"))
                                .color(theme::text_strong())
                                .size(13.0),
                        );
                    });
                }
            });
        });

    let response = frame_response.response.interact(egui::Sense::click());

    if expand_factor < 0.3 {
        response
            .clone()
            .on_hover_text(crate::i18n::tr(language, "User Guide"));
    }

    ui.memory_mut(|m| {
        m.data.insert_temp(guide_id.with("hover_state"), response.hovered());
        m.data.insert_temp(guide_id.with("active_state"), response.is_pointer_button_down_on());
    });

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if response.clicked() {
        open_guide_modal(modal_dialog, language);
    }
}
