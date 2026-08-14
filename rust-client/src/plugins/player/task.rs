use super::{backend::MediaSource, subtitles::SubtitleTimeline};
use crate::client_settings::RecognitionSettings;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VideoSubtitleMode {
    #[default]
    RealtimeTranslation,
    ImportedSrt(PathBuf),
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoTask {
    pub id: String,
    pub title: String,
    pub source: MediaSource,
    pub source_language: String,
    pub target_language: String,
    pub subtitle_mode: VideoSubtitleMode,
    #[serde(default = "default_video_recognition")]
    pub recognition: RecognitionSettings,
    pub created_at_sec: u64,
    pub last_played_sec: u64,
    pub duration_ms: i64,
    pub subtitles: SubtitleTimeline,
}

fn default_video_recognition() -> RecognitionSettings {
    RecognitionSettings {
        background_noise: 0.15,
        pause_tolerance: 0.5,
        continuous_recognition: false,
    }
}

impl VideoTask {
    pub fn new(
        title: String,
        source: MediaSource,
        source_lang: String,
        target_lang: String,
        subtitle_mode: VideoSubtitleMode,
        recognition: RecognitionSettings,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            source,
            source_language: source_lang,
            target_language: target_lang,
            subtitle_mode,
            recognition,
            created_at_sec: now,
            last_played_sec: now,
            duration_ms: 0,
            subtitles: SubtitleTimeline::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VideoTaskStore {
    pub tasks: Vec<VideoTask>,
}

impl VideoTaskStore {
    pub fn load_from_dir(dir: &Path) -> Self {
        let file_path = dir.join("video_tasks.json");
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            if let Ok(store) = serde_json::from_str::<Self>(&content) {
                return store;
            }
        }
        Self::default()
    }

    pub fn save_to_dir(&self, dir: &Path) -> std::io::Result<()> {
        let _ = std::fs::create_dir_all(dir);
        let file_path = dir.join("video_tasks.json");
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(file_path, content)
    }

    pub fn add_or_update(&mut self, task: VideoTask) {
        if let Some(pos) = self.tasks.iter().position(|t| t.id == task.id) {
            self.tasks[pos] = task;
        } else {
            self.tasks.insert(0, task);
        }
    }

    pub fn delete(&mut self, id: &str) {
        self.tasks.retain(|t| t.id != id);
    }

    pub fn get(&self, id: &str) -> Option<&VideoTask> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut VideoTask> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }
}
