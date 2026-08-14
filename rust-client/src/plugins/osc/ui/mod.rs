pub mod canvas;
mod settings;
pub mod toolbar;

use crate::ui::components::card;
use eframe::egui;

#[derive(Clone, Copy)]
pub struct OscPageContext<'a> {
    pub language: crate::i18n::UiLanguage,
    pub last_error: Option<&'a str>,
    pub mute_gate_enabled: bool,
}

#[derive(Debug)]
pub enum OscUiAction {
    ClearHostHistory,
    SetMuteGateEnabled(bool),
    SetSpeakerRecognitionEnabled(bool),
    SaveSettings,
    SettingsApplied(Result<(), String>),
}

pub fn render(
    plugin: &mut super::OscPlugin,
    ui: &mut egui::Ui,
    context: OscPageContext<'_>,
) -> Vec<OscUiAction> {
    let mut actions = Vec::new();
    ui.label(
        egui::RichText::new(crate::i18n::tr(context.language, "VRChat OSC Studio"))
            .size(22.0)
            .color(crate::ui::theme::text_strong())
            .strong(),
    );

    ui.add_space(12.0);

    if let Some(error) = context.last_error {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("⚠").color(egui::Color32::from_rgb(220, 38, 38)));
                ui.label(egui::RichText::new(error).color(egui::Color32::from_rgb(220, 38, 38)));
            });
        });
        ui.add_space(10.0);
    }

    toolbar::render_toolbar(
        plugin,
        ui,
        context.language,
        context.mute_gate_enabled,
        &mut actions,
    );

    ui.add_space(12.0);

    canvas::render_canvas(plugin, ui, context.language);
    actions
}

pub fn render_settings(
    plugin: &mut super::OscPlugin,
    ui: &mut egui::Ui,
    language: crate::i18n::UiLanguage,
) -> Vec<OscUiAction> {
    settings::render(plugin, ui, language)
}
