use crate::audio::InputDevice;
use crate::i18n::UiLanguage;
use crate::osc::OscSettings;
use crate::ui::Page;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureSource {
    Microphone,
    SystemAudio,
}

impl Default for CaptureSource {
    fn default() -> Self {
        Self::Microphone
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ClientSettings {
    #[serde(default)]
    pub capture_source: CaptureSource,
    #[serde(default)]
    pub selected_device_id: String,
    #[serde(default)]
    pub selected_loopback_device_id: String,
    #[serde(default = "default_source_lang")]
    pub source_lang: String,
    #[serde(default = "default_target_lang")]
    pub target_lang: String,
    #[serde(default)]
    pub tts_enabled: bool,
    #[serde(default)]
    pub mute_self_pauses_translation: bool,
    #[serde(default)]
    pub ui_language: UiLanguage,
    #[serde(default = "default_true")]
    pub first_run: bool,
    #[serde(default = "default_server_url")]
    pub server_url: String,
    #[serde(default = "OscSettings::from_project_config")]
    pub osc_settings: OscSettings,
    #[serde(default)]
    pub active_page: Page,
    #[serde(default)]
    pub sidebar_collapsed: bool,
    #[serde(default)]
    pub floating_subtitles_enabled: bool,
    #[serde(default = "default_floating_max_count")]
    pub floating_subtitles_max_count: usize,
    #[serde(default = "default_floating_font_size")]
    pub floating_subtitles_font_size: f64,
}

fn default_source_lang() -> String {
    "auto".into()
}

fn default_target_lang() -> String {
    "zh,en".into()
}

const fn default_true() -> bool {
    true
}

fn default_server_url() -> String {
    "ws://127.0.0.1:7654/ws".into()
}

fn default_floating_max_count() -> usize {
    5
}

fn default_floating_font_size() -> f64 {
    14.0
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            capture_source: CaptureSource::Microphone,
            selected_device_id: String::new(),
            selected_loopback_device_id: String::new(),
            source_lang: default_source_lang(),
            target_lang: default_target_lang(),
            tts_enabled: false,
            mute_self_pauses_translation: false,
            ui_language: UiLanguage::default(),
            first_run: true,
            server_url: default_server_url(),
            osc_settings: OscSettings::from_project_config(),
            active_page: Page::default(),
            sidebar_collapsed: false,
            floating_subtitles_enabled: false,
            floating_subtitles_max_count: default_floating_max_count(),
            floating_subtitles_font_size: default_floating_font_size(),
        }
    }
}

impl ClientSettings {
    pub fn load(project_root: &Path) -> Self {
        let settings_path = project_root.join("runtime").join("rust-client-settings.json");
        if let Ok(contents) = std::fs::read_to_string(&settings_path) {
            if let Ok(settings) = serde_json::from_str::<ClientSettings>(&contents) {
                return settings;
            }
        }

        // Migration from legacy app_state.json if available
        let app_state_path = project_root.join("runtime").join("app_state.json");
        let mut settings = Self::default();
        if let Ok(contents) = std::fs::read_to_string(&app_state_path) {
            #[derive(Deserialize)]
            struct LegacyAppState {
                first_run: Option<bool>,
                ui_language: Option<UiLanguage>,
            }
            if let Ok(legacy) = serde_json::from_str::<LegacyAppState>(&contents) {
                if let Some(first_run) = legacy.first_run {
                    settings.first_run = first_run;
                }
                if let Some(ui_language) = legacy.ui_language {
                    settings.ui_language = ui_language;
                }
            }
        }
        settings
    }

    pub fn sanitize_devices(
        &mut self,
        available_mics: &[InputDevice],
        available_loopbacks: &[InputDevice],
    ) {
        self.osc_settings.history_ttl_seconds =
            self.osc_settings.history_ttl_seconds.clamp(10.0, 20.0);
        self.floating_subtitles_max_count = self.floating_subtitles_max_count.clamp(1, 10);
        self.floating_subtitles_font_size = self.floating_subtitles_font_size.clamp(10.0, 24.0);

        if !self.selected_device_id.is_empty()
            && !available_mics.iter().any(|d| d.id == self.selected_device_id)
        {
            log::warn!(
                "Saved microphone ID '{}' is no longer available. Falling back to default.",
                self.selected_device_id
            );
            self.selected_device_id.clear();
        }

        if !self.selected_loopback_device_id.is_empty()
            && !available_loopbacks
                .iter()
                .any(|d| d.id == self.selected_loopback_device_id)
        {
            log::warn!(
                "Saved loopback device ID '{}' is no longer available. Falling back to default.",
                self.selected_loopback_device_id
            );
            self.selected_loopback_device_id.clear();
        }
    }

    pub fn save(&self, project_root: &Path) -> Result<(), String> {
        let directory = project_root.join("runtime");
        let _ = std::fs::create_dir_all(&directory);
        let path = directory.join("rust-client-settings.json");
        let contents = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, format!("{contents}\n")).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_settings_load_save_and_sanitize() {
        let root = std::env::temp_dir().join("xrtranslate_test_settings");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::create_dir_all(&root);

        let mut settings = ClientSettings::default();
        settings.capture_source = CaptureSource::SystemAudio;
        settings.selected_device_id = "mic-1".into();
        settings.selected_loopback_device_id = "loopback-1".into();
        settings.tts_enabled = true;
        settings.source_lang = "en".into();
        settings.sidebar_collapsed = true;
        settings.active_page = Page::Osc;
        settings.osc_settings.show_speaker_number = true;

        settings.save(&root).unwrap();

        let mut loaded = ClientSettings::load(&root);
        assert_eq!(loaded.capture_source, CaptureSource::SystemAudio);
        assert_eq!(loaded.selected_device_id, "mic-1");
        assert_eq!(loaded.selected_loopback_device_id, "loopback-1");
        assert_eq!(loaded.tts_enabled, true);
        assert_eq!(loaded.source_lang, "en");
        assert_eq!(loaded.sidebar_collapsed, true);
        assert_eq!(loaded.active_page, Page::Osc);
        assert_eq!(loaded.osc_settings.show_speaker_number, true);

        // Test sanitization with missing device
        let available_mics = vec![InputDevice {
            id: "mic-2".into(),
            name: "Other Mic".into(),
        }];
        let available_loopbacks = vec![InputDevice {
            id: "loopback-1".into(),
            name: "Loopback 1".into(),
        }];

        loaded.sanitize_devices(&available_mics, &available_loopbacks);
        assert_eq!(loaded.selected_device_id, ""); // Reset due to mic-1 missing
        assert_eq!(loaded.selected_loopback_device_id, "loopback-1"); // Kept

        let _ = std::fs::remove_dir_all(&root);
    }
}
