//! Fullscreen onboarding wizard and initial setup steps.
//!
//! Provides the step-by-step setup flow for model provider configuration,
//! optional TTS voice cloning, and centralized resource download / runtime installation.

use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke};
use xrtranslate_assets::{ModelCapability, ModelLevel};

use crate::{
    i18n,
    model_install::{
        NativeModelPackage, NativeModelTaskState, configured_model_packages,
        model_level_packages_for_provider, model_package_for_provider_config_key, set_model_level,
    },
    ui::{components, theme},
};

const STEPS: [&'static str; 4] = [
    "Welcome",
    "Configure models",
    "Optional TTS",
    "Download",
];

pub fn render_onboarding_fullscreen(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    let total_pages = STEPS.len();
    if app.onboarding_page >= total_pages {
        app.onboarding_page = total_pages - 1;
    }
    let requirement = crate::onboarding::evaluate_step_requirement(
        app.onboarding_page,
        &app.project_root(),
        &app.service_config,
        &app.backend_manager,
        &app.model_task_manager,
        &app.runtime_installer,
    );

    let viewport_focused = ui.input(|input| input.viewport().focused.unwrap_or(true));

    egui::Panel::bottom("onboarding_bottom_nav")
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(theme::content_backdrop(viewport_focused))
                .inner_margin(Margin::symmetric(36, 14))
                .stroke(Stroke::NONE),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if app.onboarding_page > 0
                    && components::animated_button(ui, i18n::tr(app.ui_language, "Back")).clicked()
                {
                    app.onboarding_page -= 1;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if app.onboarding_page + 1 == total_pages {
                        if components::primary_button_enabled(
                            ui,
                            i18n::tr(app.ui_language, "Open Translation"),
                            requirement.is_none(),
                        )
                        .clicked()
                        {
                            app.finish_onboarding();
                        }
                    } else if components::primary_button_enabled(
                        ui,
                        i18n::tr(app.ui_language, "Continue"),
                        requirement.is_none(),
                    )
                    .clicked()
                    {
                        app.onboarding_page += 1;
                    }
                    if let Some(hint) = requirement {
                        ui.label(
                            RichText::new(i18n::tr(app.ui_language, hint))
                                .size(12.0)
                                .color(theme::text_weak()),
                        );
                    }
                });
            });
        });

    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(theme::content_backdrop(viewport_focused))
                .inner_margin(Margin::symmetric(36, 20))
                .stroke(Stroke::NONE),
        )
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("XRTranslate")
                            .size(14.0)
                            .color(theme::primary())
                            .strong(),
                    );
                    ui.add_space(6.0);
                    let github_icon = egui::Image::new(egui::include_image!(
                        "../../../resources/icons/github.svg"
                    ))
                    .fit_to_exact_size(egui::vec2(20.0, 20.0))
                    .tint(theme::text_weak());

                    let github_btn = ui
                        .add(egui::Button::image(github_icon).frame(false))
                        .on_hover_text("GitHub: NowLoadY/XRTranslate")
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if github_btn.clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(
                            "https://github.com/NowLoadY/XRTranslate",
                        ));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{}/{}",
                                i18n::tr(
                                    app.ui_language,
                                    match app.onboarding_page {
                                        0 => "Step 1",
                                        1 => "Step 2",
                                        2 => "Step 3",
                                        _ => "Step 4",
                                    }
                                ),
                                total_pages
                            ))
                            .size(13.0)
                            .color(theme::text_weak()),
                        );
                        ui.add_space(8.0);
                        let mut language = app.ui_language;
                        if components::language_selector(
                            ui,
                            "onboarding_ui_language",
                            &mut language,
                        ) {
                            app.set_ui_language(language);
                        }
                        ui.add_space(8.0);
                        let mut proxy = app.download_proxy_url.clone();
                        let proxy_response = components::singleline_input(
                            ui,
                            &mut proxy,
                            i18n::tr(app.ui_language, "Proxy, e.g. http://127.0.0.1:7890"),
                            220.0,
                            false,
                        );
                        if proxy_response.lost_focus() || proxy_response.changed() {
                            app.set_download_proxy_url(proxy);
                        }
                    });
                });
                ui.add_space(8.0);
                ui.label(
                    RichText::new(i18n::tr(
                        app.ui_language,
                        "A calm start, one step at a time",
                    ))
                    .size(22.0)
                    .color(theme::text_strong())
                    .strong(),
                );
                ui.add_space(20.0);

                render_onboarding_steps(
                    ui,
                    app.ui_language,
                    &STEPS,
                    app.onboarding_page,
                );

                ui.add_space(28.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt("onboarding_content_scroll")
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let cur = app.onboarding_page;
                        crate::ui::animation::AnimationSystem::render_page_flip_transition(
                            ui,
                            cur,
                            |ui| match cur {
                                0 => render_onboarding_welcome(app.ui_language, ui),
                                1 => render_onboarding_models(app, ui),
                                2 => render_onboarding_tts(app, ui),
                                _ => render_onboarding_download(app, ui),
                            },
                        );
                    });
            });
        });
}

fn render_onboarding_steps(
    ui: &mut egui::Ui,
    language: i18n::UiLanguage,
    steps: &[&'static str],
    current: usize,
) {
    ui.horizontal(|ui| {
        for (i, title) in steps.iter().enumerate() {
            let active = i == current;
            let visited = i < current;
            let fill = if active {
                theme::primary()
            } else if visited {
                Color32::from_rgb(219, 234, 254)
            } else {
                Color32::from_rgb(241, 245, 249)
            };
            let text_color = if active {
                Color32::WHITE
            } else if visited {
                theme::primary_dark()
            } else {
                theme::text_weak()
            };
            let stroke = if active {
                Stroke::NONE
            } else if visited {
                Stroke::new(1.0, Color32::from_rgb(147, 197, 253))
            } else {
                Stroke::new(1.0, theme::border())
            };
            Frame::new()
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    let label = i18n::tr(language, *title);
                    ui.label(
                        RichText::new(format!("{} {}", i + 1, label))
                            .size(12.5)
                            .color(text_color)
                            .strong(),
                    );
                });

            if i + 1 < steps.len() {
                ui.add_space(4.0);
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
    title: &'static str,
    description: &'static str,
    stroke_color: Color32,
    language: crate::i18n::UiLanguage,
) {
    Frame::new()
        .fill(theme::surface_control())
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(22, 20))
        .stroke(Stroke::new(1.5, stroke_color))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(108.0);

            ui.label(
                RichText::new(crate::i18n::tr(language, title))
                    .size(16.0)
                    .color(theme::text_strong())
                    .strong(),
            );

            ui.add_space(8.0);
            ui.label(
                RichText::new(crate::i18n::tr(language, description))
                    .size(13.0)
                    .color(theme::text_weak())
                    .line_height(Some(19.0)),
            );
        });
}

fn render_onboarding_welcome(language: crate::i18n::UiLanguage, ui: &mut egui::Ui) {
    onboarding_title(
        ui,
        language,
        "Welcome to XRTranslate",
        Some("Your modular, real-time speech recognition, translation, and VR immersion platform."),
    );
    ui.columns(3, |columns| {
        onboarding_feature_card(
            &mut columns[0],
            "Audio Input",
            "Microphone & desktop audio capture with AI noise suppression and VAD detection.",
            Color32::from_rgb(59, 130, 246),
            language,
        );
        onboarding_feature_card(
            &mut columns[1],
            "Recognition & Translation",
            "High-accuracy real-time speech translation powered by local models or cloud APIs.",
            Color32::from_rgb(16, 185, 129),
            language,
        );
        onboarding_feature_card(
            &mut columns[2],
            "Plugins & Integrations",
            "VRChat OSC sync, desktop floating subtitles, and meeting minutes recording.",
            Color32::from_rgb(245, 158, 11),
            language,
        );
    });
}

// ---------------------------------------------------------------------------
// Step 2: Configure Models (Pure configuration without download buttons)
// ---------------------------------------------------------------------------

fn render_onboarding_models(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    let language = app.ui_language;
    onboarding_title(
        ui,
        language,
        "Configure model providers",
        Some("Select local models or cloud APIs for speech recognition and translation. Required model packages will be downloaded in the final step."),
    );
    let project_root = app.project_root();
    let packages = match configured_model_packages(&project_root) {
        Ok(packages) => packages,
        Err(error) => {
            app.last_error = Some(error);
            Vec::new()
        }
    };
    let mut level_change = None;
    let mut provider_change = None;
    let mut remote_fields = None;
    let capabilities = [
        (
            "asr",
            ModelCapability::Asr,
            "Speech Recognition Model",
        ),
        (
            "translation",
            ModelCapability::Translation,
            "Translation Model",
        ),
    ];
    ui.columns(capabilities.len(), |columns| {
        for (index, (category, capability, title)) in capabilities.iter().enumerate() {
            let package = packages
                .iter()
                .find(|package| package.capability == *capability);
            let provider = app.service_config.onboarding_provider_state(category);
            let levels = package.map_or_else(Vec::new, |package| {
                model_level_packages_for_provider(package.provider, package.capability)
            });
            let result = onboarding_model_config_card(
                &mut columns[index],
                language,
                category,
                title,
                provider,
                package.map(|package| package.level),
                &levels,
                if index % 2 == 0 {
                    Color32::from_rgb(59, 130, 246)
                } else {
                    Color32::from_rgb(16, 185, 129)
                },
            );
            if let Some(level) = result.selected_level {
                level_change = Some((*capability, level));
            }
            if let Some(provider) = result.selected_provider {
                provider_change = Some((*category, provider));
            }
            if let Some(fields) = result.remote_fields {
                remote_fields = Some((*category, fields));
            }
        }
    });
    if let Some((category, provider)) = provider_change {
        app.service_config
            .select_onboarding_provider(category, &provider);
        let state = app.service_config.onboarding_provider_state(category);
        if state
            .as_ref()
            .is_some_and(|state| !state.remote || !state.api_key.trim().is_empty())
        {
            if let Err(error) = app.service_config.save_onboarding_configuration() {
                app.last_error = Some(error);
            }
        }
    }
    if let Some((category, fields)) = remote_fields {
        app.service_config
            .set_onboarding_remote_fields(category, fields.model, fields.api_key);
        if fields.commit {
            if let Err(error) = app.service_config.save_onboarding_configuration() {
                app.last_error = Some(error);
            }
        }
    }
    if let Some(message) = app.service_config.onboarding_message() {
        ui.add_space(10.0);
        ui.label(
            RichText::new(message)
                .size(12.0)
                .color(Color32::from_rgb(220, 38, 38)),
        );
    }
    if let Some((capability, level)) = level_change {
        match set_model_level(&project_root, capability, level) {
            Ok(()) => {
                app.model_task_manager.invalidate_discovery();
                app.backend_manager.shutdown();
                app.last_error = None;
            }
            Err(error) => app.last_error = Some(error),
        }
    }
}

#[derive(Default)]
struct ModelConfigCardResult {
    selected_level: Option<ModelLevel>,
    selected_provider: Option<String>,
    remote_fields: Option<RemoteProviderFields>,
}

struct RemoteProviderFields {
    model: String,
    api_key: String,
    commit: bool,
}

fn onboarding_model_config_card(
    ui: &mut egui::Ui,
    language: i18n::UiLanguage,
    category: &'static str,
    title: &'static str,
    mut provider: Option<crate::service_config::OnboardingProviderState>,
    selected_level: Option<ModelLevel>,
    levels: &[NativeModelPackage],
    stroke_color: Color32,
) -> ModelConfigCardResult {
    let mut result = ModelConfigCardResult::default();
    Frame::new()
        .fill(theme::surface_control())
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(18))
        .stroke(Stroke::new(1.5, stroke_color))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(i18n::tr(language, title))
                    .size(16.0)
                    .color(theme::text_strong())
                    .strong(),
            );
            ui.add_space(10.0);
            let Some(provider) = provider.as_mut() else {
                ui.label(i18n::tr(language, "No providers configured"));
                return;
            };

            ui.horizontal(|ui| {
                ui.label(i18n::tr(language, "Mode"));
                let mut remote = provider.remote;
                egui::ComboBox::from_id_salt((category, "provider_mode"))
                    .selected_text(i18n::tr(
                        language,
                        if remote { "Online API" } else { "Local model" },
                    ))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut remote,
                            false,
                            i18n::tr(language, "Local model"),
                        );
                        ui.selectable_value(
                            &mut remote,
                            true,
                            i18n::tr(language, "Online API"),
                        );
                    });
                if remote != provider.remote
                    && let Some((name, _)) = provider
                        .choices
                        .iter()
                        .find(|(_, is_remote)| *is_remote == remote)
                {
                    result.selected_provider = Some(name.clone());
                }

                if !provider.remote {
                    let Some(mut level) = selected_level else {
                        return;
                    };
                    ui.add_space(12.0);
                    ui.label(i18n::tr(language, "Level"));
                    egui::ComboBox::from_id_salt((category, "model_level"))
                        .selected_text(i18n::tr(language, level.as_str()))
                        .show_ui(ui, |ui| {
                            for package in levels {
                                ui.selectable_value(
                                    &mut level,
                                    package.level,
                                    i18n::tr(language, package.level.as_str()),
                                );
                            }
                        });
                    if Some(level) != selected_level {
                        result.selected_level = Some(level);
                    }
                } else {
                    ui.add_space(12.0);
                    ui.label(i18n::tr(language, "Provider:"));
                    egui::ComboBox::from_id_salt((category, "online_provider"))
                        .selected_text(&provider.selected)
                        .show_ui(ui, |ui| {
                            for (name, is_remote) in &provider.choices {
                                if *is_remote
                                    && ui
                                        .selectable_label(provider.selected == *name, name)
                                        .clicked()
                                {
                                    result.selected_provider = Some(name.clone());
                                }
                            }
                        });
                }
            });

            if provider.remote {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(i18n::tr(language, "Model"));
                    let model_response = components::singleline_input(
                        ui,
                        &mut provider.model,
                        i18n::tr(language, "Model"),
                        (ui.available_width() - 60.0).max(160.0),
                        false,
                    );
                    if model_response.changed() || model_response.lost_focus() {
                        result.remote_fields = Some(RemoteProviderFields {
                            model: provider.model.clone(),
                            api_key: provider.api_key.clone(),
                            commit: model_response.lost_focus(),
                        });
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(i18n::tr(language, "API key"));
                    let key_response = components::singleline_input(
                        ui,
                        &mut provider.api_key,
                        i18n::tr(language, "API key"),
                        (ui.available_width() - 70.0).max(160.0),
                        true,
                    );
                    let commit = key_response.lost_focus()
                        || ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if key_response.changed() || commit {
                        result.remote_fields = Some(RemoteProviderFields {
                            model: provider.model.clone(),
                            api_key: provider.api_key.clone(),
                            commit,
                        });
                    }
                });
            }
        });
    result
}

// ---------------------------------------------------------------------------
// Step 3: Optional TTS (Pure configuration without download button)
// ---------------------------------------------------------------------------

fn render_onboarding_tts(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    let language = app.ui_language;
    onboarding_title(
        ui,
        language,
        "Optional text-to-speech",
        Some("Choose Skip to keep translated text only, or select a voice-cloning provider. The model will be downloaded in the final step."),
    );
    let provider = app.service_config.onboarding_provider_state("tts");
    let mut selected_provider = None;

    Frame::new()
        .fill(theme::surface_control())
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(18))
        .stroke(Stroke::new(1.5, Color32::from_rgb(244, 63, 94)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(i18n::tr(language, "Voice cloning & speech synthesis"))
                    .size(16.0)
                    .color(theme::text_strong())
                    .strong(),
            );
            ui.add_space(10.0);
            let Some(provider) = provider else {
                ui.label(i18n::tr(language, "No providers configured"));
                return;
            };
            ui.horizontal(|ui| {
                ui.label(i18n::tr(language, "Provider:"));
                egui::ComboBox::from_id_salt(("tts", "provider"))
                    .selected_text(if provider.selected == "none" {
                        i18n::tr(language, "Skip")
                    } else {
                        "Audio8 (ONNX FP16)"
                    })
                    .show_ui(ui, |ui| {
                        for (name, _) in &provider.choices {
                            let label = if name == "none" {
                                i18n::tr(language, "Skip")
                            } else {
                                "Audio8 (ONNX FP16)"
                            };
                            if ui.selectable_label(provider.selected == *name, label).clicked() {
                                selected_provider = Some(name.clone());
                            }
                        }
                    });
            });
            ui.add_space(10.0);
            if provider.selected == "none" {
                ui.label(
                    RichText::new(i18n::tr(
                        language,
                        "TTS disabled. Translated subtitles will be displayed on screen without voice playback.",
                    ))
                    .size(12.5)
                    .color(theme::text_weak()),
                );
            } else {
                ui.label(
                    RichText::new(i18n::tr(
                        language,
                        "Audio8 ONNX model provides local voice cloning and real-time speech playback.",
                    ))
                    .size(12.5)
                    .color(theme::text_weak()),
                );
            }
        });

    if let Some(selected) = selected_provider {
        app.service_config
            .select_onboarding_provider("tts", &selected);
        if let Err(error) = app.service_config.save_onboarding_configuration() {
            app.last_error = Some(error);
        }
    }
}

// ---------------------------------------------------------------------------
// Step 4: Centralized Download (Dynamic model cards & runtime acceleration)
// ---------------------------------------------------------------------------

struct DownloadItem {
    id: xrtranslate_assets::ModelAssetId,
    category_title: &'static str,
    detail: String,
    download_bytes: Option<u64>,
    installed: bool,
    stroke_color: Color32,
}

fn render_onboarding_download(app: &mut crate::XRTranslateApp, ui: &mut egui::Ui) {
    let language = app.ui_language;
    let project_root = app.project_root();

    if app.model_task_manager.needs_discovery()
        && let Err(error) = app
            .model_task_manager
            .discover_existing(project_root.clone())
    {
        app.last_error = Some(error);
    }

    let requirements = app.service_config.runtime_requirements();
    if !app.runtime_installer.is_busy()
        && !app.runtime_installer.plan_matches(requirements)
        && let Err(error) = app.runtime_installer.prepare_for(
            project_root.clone(),
            requirements,
        )
    {
        app.last_error = Some(error);
    }

    onboarding_title(
        ui,
        language,
        "Download required resources",
        Some("Download the configured model packages and native inference runtime for your system."),
    );

    let busy = app.model_task_manager.is_busy();
    let retry = matches!(
        app.model_task_manager.state(),
        NativeModelTaskState::Failed(_)
    );
    let mut install = None;

    // 1. Model packages (ASR, MT, TTS)
    let packages = match configured_model_packages(&project_root) {
        Ok(packages) => packages,
        Err(error) => {
            app.last_error = Some(error);
            Vec::new()
        }
    };

    let tts_provider = app.service_config.onboarding_provider_state("tts");
    let selected_audio8 = tts_provider
        .as_ref()
        .is_some_and(|state| state.selected == "audio8");
    let tts_package = selected_audio8
        .then(|| {
            model_package_for_provider_config_key(
                &project_root,
                "audio8",
                ModelCapability::Tts,
                "audio8-tts-onnx-fp16",
            )
            .ok()
        })
        .flatten();

    let mut download_items: Vec<DownloadItem> = Vec::new();

    // ASR package (if local)
    if let Some(asr_package) = packages.iter().find(|p| p.capability == ModelCapability::Asr) {
        let installed = app.model_task_manager.is_model_present(asr_package.id);
        download_items.push(DownloadItem {
            id: asr_package.id,
            category_title: "Speech Recognition Model",
            detail: format!("{} · {}", asr_package.label, i18n::tr(language, asr_package.level.as_str())),
            download_bytes: (!installed).then_some(asr_package.download_bytes),
            installed,
            stroke_color: Color32::from_rgb(59, 130, 246),
        });
    }

    // Translation package (if local)
    if let Some(mt_package) = packages.iter().find(|p| p.capability == ModelCapability::Translation) {
        let installed = app.model_task_manager.is_model_present(mt_package.id);
        download_items.push(DownloadItem {
            id: mt_package.id,
            category_title: "Translation Model",
            detail: format!("{} · {}", mt_package.label, i18n::tr(language, mt_package.level.as_str())),
            download_bytes: (!installed).then_some(mt_package.download_bytes),
            installed,
            stroke_color: Color32::from_rgb(16, 185, 129),
        });
    }

    // TTS package (if Audio8)
    if let Some(tts_pkg) = &tts_package {
        let installed = app.model_task_manager.is_model_present(tts_pkg.id) || app.model_task_manager.is_model_ready(tts_pkg.id);
        download_items.push(DownloadItem {
            id: tts_pkg.id,
            category_title: "Voice Cloning & TTS Model",
            detail: "Audio8 (ONNX FP16)".to_string(),
            download_bytes: (!installed).then_some(tts_pkg.download_bytes),
            installed,
            stroke_color: Color32::from_rgb(244, 63, 94),
        });
    }

    if !download_items.is_empty() {
        ui.label(
            RichText::new(i18n::tr(language, "Model Packages"))
                .size(15.0)
                .color(theme::text_strong())
                .strong(),
        );
        ui.add_space(8.0);

        for item in &download_items {
            let action = if item.installed {
                "Installed"
            } else if retry {
                "Retry"
            } else {
                "Download"
            };
            let clicked = render_download_card(
                ui,
                language,
                item,
                action,
                !busy && !item.installed,
            );
            if clicked {
                install = Some(item.id);
            }
            ui.add_space(8.0);
        }
    } else {
        Frame::new()
            .fill(theme::surface_control())
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(16))
            .stroke(Stroke::new(1.5, Color32::from_rgb(16, 185, 129)))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(i18n::tr(
                        language,
                        "All services use cloud APIs. No local models or inference runtimes are required.",
                    ))
                    .size(13.5)
                    .color(Color32::from_rgb(4, 120, 87))
                    .strong(),
                );
            });
    }

    if let Some(asset_id) = install
        && let Err(error) = app
            .model_task_manager
            .install(project_root.clone(), asset_id)
    {
        app.last_error = Some(error);
    }

    render_model_task_state(ui, language, app.model_task_manager.state());

    // 2. Inference Runtime & Hardware Acceleration (if local models are configured)
    let requires_runtime = requirements.llama_cpp || requirements.onnx_tts;
    if requires_runtime {
        ui.add_space(16.0);
        ui.label(
            RichText::new(i18n::tr(language, "Inference Runtime & Hardware Acceleration"))
                .size(15.0)
                .color(theme::text_strong())
                .strong(),
        );
        ui.add_space(8.0);

        render_runtime_installation_section(app, ui, language, &project_root);
    }
}

fn render_download_card(
    ui: &mut egui::Ui,
    language: i18n::UiLanguage,
    item: &DownloadItem,
    action: &'static str,
    enabled: bool,
) -> bool {
    let mut clicked = false;
    Frame::new()
        .fill(theme::surface_control())
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(18, 14))
        .stroke(Stroke::new(1.5, item.stroke_color))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(i18n::tr(language, item.category_title))
                            .size(15.0)
                            .color(theme::text_strong())
                            .strong(),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(&item.detail)
                            .size(13.0)
                            .color(theme::text_weak()),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if item.installed {
                        Frame::new()
                            .fill(Color32::from_rgb(16, 185, 129))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(18, 7))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(i18n::tr(language, "Installed"))
                                        .color(Color32::WHITE)
                                        .size(13.0)
                                        .strong(),
                                );
                            });
                    } else {
                        let action_label = item.download_bytes.map_or_else(
                            || i18n::tr(language, action).to_owned(),
                            |bytes| {
                                format!(
                                    "{} · {}",
                                    i18n::tr(language, action),
                                    components::format_file_size(bytes),
                                )
                            },
                        );
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new(
                                    RichText::new(action_label).color(Color32::WHITE).strong(),
                                )
                                .fill(Color32::from_rgb(37, 99, 235))
                                .min_size(egui::Vec2::new(110.0, 32.0))
                                .corner_radius(CornerRadius::same(10)),
                            )
                            .clicked()
                        {
                            clicked = true;
                        }
                    }
                });
            });
        });
    clicked
}

fn render_runtime_installation_section(
    app: &mut crate::XRTranslateApp,
    ui: &mut egui::Ui,
    language: i18n::UiLanguage,
    project_root: &std::path::Path,
) {
    let state = app.runtime_installer.state().clone();
    let download_size = app.runtime_installer.download_size_bytes();
    let backend_name = app
        .runtime_installer
        .backend_label()
        .unwrap_or("CPU")
        .to_owned();
    let cuda_version = app.runtime_installer.cuda_version_label();
    let backend = cuda_version.map_or(backend_name.clone(), |version| {
        format!("{backend_name} {version}")
    });

    let default_runtime = app.backend_manager.runtime_layout().runtime_root().to_path_buf();
    let is_custom = !app.backend_manager.runtime_directory.trim().is_empty();
    let mut runtime_dir = app.backend_manager.runtime_directory.clone();

    // Option A: Automatic Setup
    Frame::new()
        .fill(theme::surface_control())
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(
            1.5,
            if !is_custom {
                Color32::from_rgb(16, 185, 129)
            } else {
                Color32::from_rgb(110, 231, 183)
            },
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(i18n::tr(language, "Option A: Automatic Setup (Recommended)"))
                    .size(14.5)
                    .color(Color32::from_rgb(4, 120, 87))
                    .strong(),
            );
            ui.add_space(8.0);
            let ready = download_size == Some(0) && !app.runtime_installer.is_busy();
            if ready {
                Frame::new()
                    .fill(Color32::from_rgb(16, 185, 129))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(18, 7))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("{} · {}", i18n::tr(language, "Installed"), backend))
                                .color(Color32::WHITE)
                                .size(13.0)
                                .strong(),
                        );
                    });
            } else {
                let button_text = download_size.map_or_else(
                    || i18n::tr(language, "Preparing download…").to_owned(),
                    |bytes| {
                        format!(
                            "{} {} · {}",
                            i18n::tr(language, "Download"),
                            backend,
                            components::format_file_size(bytes)
                        )
                    },
                );
                let button = components::primary_button_enabled(
                    ui,
                    &button_text,
                    download_size.is_some() && !app.runtime_installer.is_busy(),
                );
                if button.clicked() {
                    app.backend_manager.runtime_directory.clear();
                    if let Err(error) = app.backend_manager.save_runtime_directory() {
                        app.last_error = Some(error);
                    }
                    if let Err(error) = app.runtime_installer.install_recommended(project_root.to_path_buf()) {
                        app.last_error = Some(error);
                    }
                }
            }

            if let Some(path) = components::render_runtime_task_state(
                ui,
                language,
                &state,
                "Extracting native runtime...",
                "The native runtime is installed and ready.",
            ) {
                app.backend_manager.runtime_directory = path.to_string_lossy().to_string();
                if let Err(error) = app.backend_manager.save_runtime_directory() {
                    app.last_error = Some(error);
                }
            }
        });

    ui.add_space(12.0);

    // Option B: Custom Runtime Directory
    Frame::new()
        .fill(theme::surface_control())
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(
            1.5,
            if is_custom {
                theme::primary()
            } else {
                theme::border()
            },
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(i18n::tr(language, "Option B: Choose Existing Runtime Directory"))
                    .size(14.5)
                    .color(theme::text_strong())
                    .strong(),
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(i18n::tr(language, "Runtime Directory:"));
                let response = components::singleline_input(
                    ui,
                    &mut runtime_dir,
                    i18n::tr(language, "Path to runtime folder"),
                    (ui.available_width() - 80.0).max(200.0),
                    false,
                );
                if response.changed() || response.lost_focus() {
                    app.backend_manager.runtime_directory = runtime_dir.clone();
                    if let Err(error) = app.backend_manager.save_runtime_directory() {
                        app.last_error = Some(error);
                    }
                    let requirements = app.service_config.runtime_requirements();
                    let _ = app.runtime_installer.prepare_for(project_root.to_path_buf(), requirements);
                }
                if components::animated_button(ui, i18n::tr(language, "Browse...")).clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    app.backend_manager.runtime_directory = path.to_string_lossy().to_string();
                    if let Err(error) = app.backend_manager.save_runtime_directory() {
                        app.last_error = Some(error);
                    }
                    let requirements = app.service_config.runtime_requirements();
                    let _ = app.runtime_installer.prepare_for(project_root.to_path_buf(), requirements);
                }
            });

            if !is_custom {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        i18n::tr(language, "Default:"),
                        default_runtime.display()
                    ))
                    .size(12.0)
                    .color(theme::text_weak()),
                );
            }
        });
}

fn render_model_task_state(
    ui: &mut egui::Ui,
    language: i18n::UiLanguage,
    state: &NativeModelTaskState,
) {
    match state {
        NativeModelTaskState::Idle => {}
        NativeModelTaskState::Discovering => {
            ui.add_space(6.0);
            ui.label(
                RichText::new(i18n::tr(language, "Scanning local models..."))
                    .size(12.0)
                    .color(theme::text_weak()),
            );
        }
        NativeModelTaskState::Detected { .. } => {
            ui.add_space(6.0);
            ui.label(
                RichText::new(i18n::tr(language, "Model packages detected."))
                    .size(12.0)
                    .color(Color32::from_rgb(5, 150, 105)),
            );
        }
        NativeModelTaskState::Installing {
            downloaded_bytes,
            total_bytes,
            ..
        } => {
            ui.add_space(6.0);
            if *total_bytes > 0 {
                ui.add(
                    egui::ProgressBar::new(
                        (*downloaded_bytes as f64 / *total_bytes as f64).clamp(0.0, 1.0) as f32,
                    )
                    .text(format!(
                        "{} / {}",
                        components::format_file_size(*downloaded_bytes),
                        components::format_file_size(*total_bytes),
                    )),
                );
            } else {
                ui.label(
                    RichText::new(format!(
                        "{}...",
                        components::format_file_size(*downloaded_bytes)
                    ))
                    .size(12.0)
                    .color(theme::text_weak()),
                );
            }
        }
        NativeModelTaskState::Installed { .. } => {
            ui.add_space(6.0);
            ui.label(
                RichText::new(i18n::tr(language, "A model package is ready."))
                    .size(12.0)
                    .color(Color32::from_rgb(5, 150, 105)),
            );
        }
        NativeModelTaskState::Failed(error) => {
            ui.add_space(6.0);
            ui.label(
                RichText::new(error)
                    .size(12.0)
                    .color(Color32::from_rgb(220, 38, 38)),
            );
        }
    }
}
