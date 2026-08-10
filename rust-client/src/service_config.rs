use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
enum JsonFieldKind {
    String,
    Bool,
    Number,
    Json,
}

struct ConfigField {
    name: String,
    value: String,
    kind: JsonFieldKind,
}

struct ProviderCard {
    name: String,
    fields: Vec<ConfigField>,
}

struct ServiceCategory {
    key: &'static str,
    title: &'static str,
    selected_provider: String,
    providers: Vec<ProviderCard>,
    show_all: bool,
}

/// Editable view of the ASR and translation provider portions of `config.json`.
/// The original JSON document is retained so unrelated project settings are preserved.
pub struct ServiceConfigEditor {
    path: PathBuf,
    document: Value,
    categories: Vec<ServiceCategory>,
    dirty: bool,
    message: Option<String>,
}

impl ServiceConfigEditor {
    pub fn load() -> Self {
        let path = project_config_path();
        let mut editor = Self {
            path,
            document: Value::Object(Map::new()),
            categories: Vec::new(),
            dirty: false,
            message: None,
        };
        if let Err(error) = editor.reload() {
            editor.message = Some(error);
        }
        editor
    }

    pub fn reload(&mut self) -> Result<(), String> {
        let contents = std::fs::read_to_string(&self.path)
            .map_err(|error| format!("Cannot read {}: {error}", self.path.display()))?;
        self.document = serde_json::from_str(&contents)
            .map_err(|error| format!("Invalid config.json: {error}"))?;
        self.categories = [
            ("asr", "ASR / Speech Recognition"),
            ("translation", "Translation"),
        ]
        .into_iter()
        .map(|(key, title)| Self::make_category(&self.document, key, title))
        .collect();
        self.dirty = false;
        self.message = Some("Reloaded config.json".into());
        Ok(())
    }

    fn make_category(document: &Value, key: &'static str, title: &'static str) -> ServiceCategory {
        let section = document.get(key).and_then(Value::as_object);
        let mut providers: Vec<ProviderCard> = section
            .and_then(|section| section.get("providers"))
            .and_then(Value::as_object)
            .map(|providers| {
                providers
                    .iter()
                    .map(|(name, config)| ProviderCard {
                        name: name.clone(),
                        fields: config
                            .as_object()
                            .map(|config| {
                                config
                                    .iter()
                                    .map(|(name, value)| ConfigField {
                                        name: name.clone(),
                                        value: display_value(value),
                                        kind: field_kind(value),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        providers.sort_by(|a: &ProviderCard, b: &ProviderCard| a.name.cmp(&b.name));

        let selected_provider = section
            .and_then(|section| section.get("provider"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| providers.first().map(|provider| provider.name.clone()))
            .unwrap_or_default();

        ServiceCategory {
            key,
            title,
            selected_provider,
            providers,
            show_all: false,
        }
    }

    pub fn render(
        &mut self,
        ui: &mut eframe::egui::Ui,
        backend: &mut crate::backend::BackendManager,
        model_tasks: &mut crate::model_install::NativeModelTaskManager,
        project_root: &std::path::Path,
        language: crate::i18n::UiLanguage,
    ) {
        use crate::ui::components::{self, section};
        use eframe::egui;

        ui.label(
            egui::RichText::new(crate::i18n::tr(language, "Service Providers"))
                .size(22.0)
                .color(crate::ui::theme::text_strong())
                .strong(),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(crate::i18n::tr(language, "Choose active providers and configure parameters for config.json. Changes take effect after saving and restarting the backend."))
                .color(crate::ui::theme::text_weak())
                .size(12.0),
        );
        ui.add_space(14.0);

        if model_tasks.needs_discovery() {
            if let Err(error) = model_tasks.discover_existing(project_root.to_path_buf()) {
                self.message = Some(error);
            }
        }

        for cat_idx in 0..self.categories.len() {
            let category_title = crate::i18n::tr(language, self.categories[cat_idx].title);
            let category_key = self.categories[cat_idx].key;

            section(ui, category_title, |ui| {
                // Row 1: Active Provider selector & View All toggle
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(crate::i18n::tr(language, "Active Provider:")).strong(),
                    );
                    let previous = self.categories[cat_idx].selected_provider.clone();
                    let selected_label = if self.categories[cat_idx].selected_provider.is_empty() {
                        crate::i18n::tr(language, "No providers configured")
                    } else {
                        &self.categories[cat_idx].selected_provider
                    };

                    let provider_names: Vec<String> = self.categories[cat_idx]
                        .providers
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();

                    let combo_resp = egui::ComboBox::from_id_salt((category_key, "provider_combo"))
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            for name in &provider_names {
                                ui.selectable_value(
                                    &mut self.categories[cat_idx].selected_provider,
                                    name.clone(),
                                    name,
                                );
                            }
                        });

                    if self.categories[cat_idx].selected_provider != previous {
                        self.dirty = true;
                    }

                    if combo_resp.response.changed() {
                        self.dirty = true;
                    }

                    ui.add_space(16.0);
                    ui.checkbox(
                        &mut self.categories[cat_idx].show_all,
                        crate::i18n::tr(language, "Show All Providers"),
                    );
                });

                ui.add_space(12.0);

                if self.categories[cat_idx].providers.is_empty() {
                    ui.label(
                        egui::RichText::new(crate::i18n::tr(
                            language,
                            "No providers found in config.json.",
                        ))
                        .color(crate::ui::theme::text_weak()),
                    );
                    return;
                }

                let show_all = self.categories[cat_idx].show_all;
                let active_name = self.categories[cat_idx].selected_provider.clone();

                if show_all {
                    // Render Grid for ALL Providers
                    for provider_idx in 0..self.categories[cat_idx].providers.len() {
                        let provider_name = self.categories[cat_idx].providers[provider_idx]
                            .name
                            .clone();
                        let is_active = provider_name == active_name;
                        let model_asset =
                            provider_model_asset(&self.categories[cat_idx].providers[provider_idx]);

                        ui.push_id(&provider_name, |ui| {
                            egui::Frame::new()
                                .fill(if is_active {
                                    egui::Color32::from_rgb(240, 246, 255)
                                } else {
                                    egui::Color32::from_gray(250)
                                })
                                .corner_radius(egui::CornerRadius::same(8))
                                .inner_margin(egui::Margin::same(12))
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    if is_active {
                                        egui::Color32::from_rgb(59, 130, 246)
                                    } else {
                                        egui::Color32::from_gray(225)
                                    },
                                ))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(&provider_name)
                                                .strong()
                                                .size(14.0)
                                                .color(crate::ui::theme::text_strong()),
                                        );
                                        if is_active {
                                            ui.label(
                                                egui::RichText::new(crate::i18n::tr(
                                                    language, "(Active)",
                                                ))
                                                .color(egui::Color32::from_rgb(37, 99, 235))
                                                .size(12.0)
                                                .strong(),
                                            );
                                        }
                                        if let Some(message) = render_provider_model_action(
                                            ui,
                                            backend,
                                            model_tasks,
                                            project_root,
                                            language,
                                            category_key,
                                            &provider_name,
                                            model_asset.as_deref(),
                                        ) {
                                            self.message = Some(message);
                                        }
                                    });

                                    ui.add_space(8.0);

                                    let fields_len = self.categories[cat_idx].providers
                                        [provider_idx]
                                        .fields
                                        .len();
                                    if fields_len == 0 {
                                        ui.label(
                                            egui::RichText::new(crate::i18n::tr(
                                                language,
                                                "No configurable parameters",
                                            ))
                                            .color(crate::ui::theme::text_weak())
                                            .size(12.0),
                                        );
                                    } else {
                                        egui::Grid::new((category_key, &provider_name, "all_grid"))
                                            .num_columns(2)
                                            .spacing([16.0, 8.0])
                                            .min_col_width(130.0)
                                            .show(ui, |ui| {
                                                for field in &mut self.categories[cat_idx].providers
                                                    [provider_idx]
                                                    .fields
                                                {
                                                    ui.label(
                                                        egui::RichText::new(&field.name)
                                                            .color(crate::ui::theme::text_normal()),
                                                    );
                                                    let edit_w =
                                                        (ui.available_width() - 20.0).max(200.0);
                                                    if render_field_input(ui, field, edit_w) {
                                                        self.dirty = true;
                                                    }
                                                    ui.end_row();
                                                }
                                            });
                                    }
                                });
                        });
                        ui.add_space(10.0);
                    }
                } else {
                    // Render Form Grid for the ACTIVE Provider Only
                    let active_idx = self.categories[cat_idx]
                        .providers
                        .iter()
                        .position(|p| p.name == active_name);

                    if let Some(idx) = active_idx {
                        let provider_name = self.categories[cat_idx].providers[idx].name.clone();
                        let model_asset =
                            provider_model_asset(&self.categories[cat_idx].providers[idx]);

                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Parameters for Active Provider ('{}'):",
                                    provider_name
                                ))
                                .size(13.5)
                                .color(crate::ui::theme::text_strong())
                                .strong(),
                            );
                            ui.add_space(8.0);
                            if let Some(message) = render_provider_model_action(
                                ui,
                                backend,
                                model_tasks,
                                project_root,
                                language,
                                category_key,
                                &provider_name,
                                model_asset.as_deref(),
                            ) {
                                self.message = Some(message);
                            }
                        });
                        ui.add_space(10.0);

                        let fields_len = self.categories[cat_idx].providers[idx].fields.len();
                        if fields_len == 0 {
                            ui.label(
                                egui::RichText::new(crate::i18n::tr(
                                    language,
                                    "No configurable parameters for this provider.",
                                ))
                                .color(crate::ui::theme::text_weak()),
                            );
                        } else {
                            egui::Grid::new((category_key, &provider_name, "active_grid"))
                                .num_columns(2)
                                .spacing([20.0, 10.0])
                                .min_col_width(140.0)
                                .show(ui, |ui| {
                                    for field in &mut self.categories[cat_idx].providers[idx].fields
                                    {
                                        ui.label(
                                            egui::RichText::new(&field.name)
                                                .strong()
                                                .color(crate::ui::theme::text_strong()),
                                        );
                                        let edit_w = (ui.available_width() - 20.0).clamp(240.0, 360.0);
                                        if render_field_input(ui, field, edit_w) {
                                            self.dirty = true;
                                        }
                                        ui.end_row();
                                    }
                                });
                        }
                    }
                }
            });
            ui.add_space(12.0);
        }

        // Action Toolbar
        ui.horizontal(|ui| {
            let save_label = if self.dirty {
                crate::i18n::tr(language, "Save Service Config *")
            } else {
                crate::i18n::tr(language, "Save Service Config")
            };
            let save = components::primary_button(ui, save_label);
            if save.clicked() {
                match self.save() {
                    Ok(()) => self.message = Some(format!("Saved {}", self.path.display())),
                    Err(error) => self.message = Some(error),
                }
            }
            if components::animated_button(ui, crate::i18n::tr(language, "Reload")).clicked() {
                if let Err(error) = self.reload() {
                    self.message = Some(error);
                }
            }
            if self.dirty {
                ui.label(
                    egui::RichText::new(crate::i18n::tr(language, "(Unsaved changes)"))
                        .color(egui::Color32::from_rgb(217, 119, 6))
                        .strong(),
                );
            }
        });
        if let Some(message) = &self.message {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(message)
                    .color(crate::ui::theme::text_weak())
                    .size(12.0),
            );
        }
    }

    fn save(&mut self) -> Result<(), String> {
        let root = self
            .document
            .as_object_mut()
            .ok_or("config.json root must be an object")?;
        for category in &self.categories {
            let section = root
                .get_mut(category.key)
                .and_then(Value::as_object_mut)
                .ok_or_else(|| format!("Missing {} section", category.key))?;
            section.insert(
                "provider".into(),
                Value::String(category.selected_provider.clone()),
            );
            let providers = section
                .get_mut("providers")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| format!("Missing {}.providers section", category.key))?;
            for provider in &category.providers {
                let config = providers
                    .get_mut(&provider.name)
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| format!("Missing provider {}", provider.name))?;
                for field in &provider.fields {
                    config.insert(field.name.clone(), parse_value(&field.value, field.kind)?);
                }
            }
        }
        let formatted = serde_json::to_string_pretty(&self.document)
            .map_err(|error| format!("Cannot serialize config.json: {error}"))?;
        std::fs::write(&self.path, format!("{formatted}\n"))
            .map_err(|error| format!("Cannot save {}: {error}", self.path.display()))?;
        self.dirty = false;
        Ok(())
    }
}

fn provider_model_asset(provider: &ProviderCard) -> Option<String> {
    provider
        .fields
        .iter()
        .find(|field| field.name == "model_asset")
        .map(|field| field.value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Renders the same model lifecycle control inside every provider card that
/// declares a `model_asset`. The provider configuration, rather than a model
/// name in the UI, decides which package is offered.
fn render_provider_model_action(
    ui: &mut eframe::egui::Ui,
    backend: &mut crate::backend::BackendManager,
    model_tasks: &mut crate::model_install::NativeModelTaskManager,
    project_root: &std::path::Path,
    language: crate::i18n::UiLanguage,
    category_key: &str,
    provider_name: &str,
    model_asset: Option<&str>,
) -> Option<String> {
    use crate::model_install::{NativeModelTaskState, model_package_for_config_key};
    use eframe::egui;

    let Some(model_asset) = model_asset else {
        return crate::ui::components::animated_button(
            ui,
            crate::i18n::tr(language, "Check model files"),
        )
        .clicked()
        .then(
            || match backend.check_model_files(category_key, provider_name) {
                Ok(message) => message,
                Err(error) => error,
            },
        );
    };

    let package = match model_package_for_config_key(project_root, model_asset) {
        Ok(package) => package,
        Err(error) => return Some(error),
    };

    let ready = model_tasks.is_model_ready(package.id);
    let present = model_tasks.is_model_present(package.id);
    let busy = model_tasks.is_busy();
    let action_label = if present { "Verify" } else { "Download" };
    let clicked = ui
        .add_enabled(
            !busy,
            egui::Button::new(crate::i18n::tr(language, action_label)),
        )
        .on_hover_text(package.label)
        .clicked();
    if clicked {
        return model_tasks
            .install(project_root.to_path_buf(), package.id)
            .err();
    }

    let status = match model_tasks.state() {
        NativeModelTaskState::Discovering => Some("Looking for existing model packages..."),
        NativeModelTaskState::Detected { .. } if ready => Some("Model package verified."),
        NativeModelTaskState::Detected { .. } if present => {
            Some("Model files found. Verify before use.")
        }
        NativeModelTaskState::Installing {
            asset_id,
            relative_path,
            ..
        } if *asset_id == package.id => {
            if let Some(path) = relative_path {
                ui.label(
                    egui::RichText::new(path)
                        .size(11.0)
                        .color(crate::ui::theme::text_weak()),
                );
            }
            Some("Preparing native model installation...")
        }
        NativeModelTaskState::Installed {
            asset_id,
            directory,
        } if *asset_id == package.id => {
            ui.label(
                egui::RichText::new(directory.display().to_string())
                    .size(11.0)
                    .color(crate::ui::theme::text_weak()),
            );
            Some("Model package verified.")
        }
        NativeModelTaskState::Failed(error) => return Some(error.clone()),
        _ => None,
    };
    if let Some(status) = status {
        ui.label(
            egui::RichText::new(crate::i18n::tr(language, status))
                .size(11.0)
                .color(if ready {
                    egui::Color32::from_rgb(5, 150, 105)
                } else {
                    crate::ui::theme::text_weak()
                }),
        );
    }
    None
}

fn render_field_input(ui: &mut eframe::egui::Ui, field: &mut ConfigField, width: f32) -> bool {
    use eframe::egui;

    match field.kind {
        JsonFieldKind::Bool => {
            let mut val = field.value.trim().parse::<bool>().unwrap_or(false);
            let label = if val { "true" } else { "false" };
            if ui.checkbox(&mut val, label).changed() {
                field.value = val.to_string();
                true
            } else {
                false
            }
        }
        _ => {
            if field.name == "device" {
                let mut changed = false;
                let current = field.value.clone();
                egui::ComboBox::from_id_salt(&field.name)
                    .selected_text(&current)
                    .show_ui(ui, |ui| {
                        for opt in ["cuda", "cpu", "mps", "auto"] {
                            if ui
                                .selectable_value(&mut field.value, opt.to_string(), opt)
                                .changed()
                            {
                                changed = true;
                            }
                        }
                    });
                changed
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut field.value)
                        .desired_width(width.min(360.0))
                        .hint_text(value_hint(field.kind)),
                )
                .changed()
            }
        }
    }
}

fn project_config_path() -> PathBuf {
    for start in [std::env::current_dir().ok(), std::env::current_exe().ok()] {
        let Some(start) = start else {
            continue;
        };
        let directory = if start.is_dir() {
            start
        } else {
            start.parent().map(PathBuf::from).unwrap_or(start)
        };
        for ancestor in directory.ancestors() {
            let candidate = ancestor.join("config.json");
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from("config.json")
}

fn field_kind(value: &Value) -> JsonFieldKind {
    match value {
        Value::String(_) => JsonFieldKind::String,
        Value::Bool(_) => JsonFieldKind::Bool,
        Value::Number(_) => JsonFieldKind::Number,
        _ => JsonFieldKind::Json,
    }
}

fn display_value(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn value_hint(kind: JsonFieldKind) -> &'static str {
    match kind {
        JsonFieldKind::String => "Text",
        JsonFieldKind::Bool => "true / false",
        JsonFieldKind::Number => "Number",
        JsonFieldKind::Json => "JSON value",
    }
}

fn parse_value(value: &str, kind: JsonFieldKind) -> Result<Value, String> {
    match kind {
        JsonFieldKind::String => Ok(Value::String(value.into())),
        JsonFieldKind::Bool => value
            .trim()
            .parse::<bool>()
            .map(Value::Bool)
            .map_err(|_| format!("{value:?} must be true or false")),
        JsonFieldKind::Number => serde_json::from_str::<Value>(value.trim())
            .ok()
            .filter(Value::is_number)
            .ok_or_else(|| format!("{value:?} must be a JSON number")),
        JsonFieldKind::Json => serde_json::from_str(value.trim())
            .map_err(|error| format!("Invalid JSON value {value:?}: {error}")),
    }
}
