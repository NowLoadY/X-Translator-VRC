pub mod backend;
pub mod controller;
pub mod i18n;
pub mod installer;
pub mod subtitles;
pub mod task;
pub mod ui;

use std::path::PathBuf;
use crate::i18n::UiLanguage;
use controller::VideoPlayerController;
pub use task::VideoSubtitleMode;

#[derive(Clone, Debug)]
pub struct VideoPlayerUiSnapshot {
    pub language: UiLanguage,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerTranslationRequest {
    ImportMediaFile {
        path: PathBuf,
        source_language: String,
        target_language: String,
        recognition: crate::client_settings::RecognitionSettings,
        audio_channels: Vec<task::AudioChannelItem>,
    },
    LiveStream {
        source_language: String,
        target_language: String,
        recognition: crate::client_settings::RecognitionSettings,
        audio_channels: Vec<task::AudioChannelItem>,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum VideoPlayerAction {
    #[default]
    None,
    StartTranslation(PlayerTranslationRequest),
    StopTranslation,
}

pub struct VideoPlayerPlugin {
    pub controller: VideoPlayerController,
}

impl Default for VideoPlayerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoPlayerPlugin {
    pub fn new() -> Self {
        Self {
            controller: VideoPlayerController::default(),
        }
    }

    pub fn render_page(&mut self, snapshot: &VideoPlayerUiSnapshot, ui: &mut eframe::egui::Ui) -> VideoPlayerAction {
        ui::render(self, snapshot, ui)
    }

    pub fn on_visibility_changed(&mut self, is_visible: bool) {
        if !is_visible || self.controller.route != controller::VideoPlayerRoute::Player {
            self.controller.fullscreen_mode = false;
            if let Some(host) = &self.controller.native_host {
                host.hide();
            }
        }
    }

    pub fn on_translation_segment(
        &mut self,
        id: String,
        start_ms: i64,
        end_ms: i64,
        speaker: Option<String>,
        source: String,
        translated: Option<String>,
    ) {
        self.controller.ingest_live_caption(
            id,
            start_ms,
            end_ms,
            speaker,
            source,
            translated,
        );
    }
}
