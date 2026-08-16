pub mod backend;
pub mod controller;
pub mod i18n;
pub mod installer;
pub mod subtitles;
pub mod task;
pub mod ui;

use crate::i18n::UiLanguage;
use crate::session_coordinator::{
    PluginSessionBinding, PluginSessionOwner, SessionOutputPolicy, TranslationSessionPlugin,
};
use controller::VideoPlayerController;
use std::path::PathBuf;
#[allow(unused_imports)]
pub use task::MediaType;
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

    pub fn render_page(
        &mut self,
        snapshot: &VideoPlayerUiSnapshot,
        ui: &mut eframe::egui::Ui,
    ) -> VideoPlayerAction {
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
        metadata: subtitles::SubtitleMetadata,
    ) {
        self.controller
            .ingest_live_caption(id, start_ms, end_ms, speaker, source, translated, metadata);
    }
}

impl TranslationSessionPlugin for VideoPlayerPlugin {
    fn translation_session_binding(&self) -> Option<PluginSessionBinding> {
        let task_id = self.controller.active_task_id.clone()?;
        let is_file = matches!(
            &self.controller.current_source,
            Some(backend::MediaSource::LocalFile(_))
        );
        Some(PluginSessionBinding {
            owner: PluginSessionOwner::new(
                super::PluginId::VIDEO_PLAYER.as_str(),
                task_id,
                "Media Player",
                "Open Media Player",
                "Media Player owns the active translation session",
            ),
            output_policy: SessionOutputPolicy::Host,
            host_tts: !is_file,
            external_audio_gate: !is_file,
            finish_when_audio_ends: is_file,
        })
    }
}

#[cfg(test)]
mod session_binding_tests {
    use super::*;

    #[test]
    fn local_files_disable_live_only_session_capabilities() {
        let mut plugin = VideoPlayerPlugin::new();
        plugin.controller.active_task_id = Some("file-task".into());
        plugin.controller.current_source =
            Some(backend::MediaSource::LocalFile("movie.mp4".into()));

        let binding = plugin.translation_session_binding().unwrap();
        assert!(binding.publish_to_host_outputs());
        assert!(!binding.host_tts);
        assert!(!binding.external_audio_gate);
        assert!(binding.finish_when_audio_ends);
    }

    #[test]
    fn network_streams_inherit_host_live_capabilities() {
        let mut plugin = VideoPlayerPlugin::new();
        plugin.controller.active_task_id = Some("stream-task".into());
        plugin.controller.current_source = Some(backend::MediaSource::NetworkStream(
            "https://example.test/live".into(),
        ));

        let binding = plugin.translation_session_binding().unwrap();
        assert!(binding.host_tts);
        assert!(binding.external_audio_gate);
        assert!(!binding.finish_when_audio_ends);
    }
}
