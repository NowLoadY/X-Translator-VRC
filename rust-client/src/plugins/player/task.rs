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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioChannelItem {
    pub index: usize,
    pub id: String,
    pub name: String,
    pub is_left: bool,
    pub is_right: bool,
    pub is_center: bool,
    pub playback: bool,
    pub recognition: bool,
}

impl AudioChannelItem {
    pub fn default_for_count(count: usize) -> Vec<AudioChannelItem> {
        match count {
            1 => vec![
                AudioChannelItem {
                    index: 0,
                    id: "mono".to_string(),
                    name: "Mono (单声道)".to_string(),
                    is_left: true,
                    is_right: true,
                    is_center: true,
                    playback: true,
                    recognition: true,
                }
            ],
            2 => vec![
                AudioChannelItem {
                    index: 0,
                    id: "fl".to_string(),
                    name: "FL (左声道 / Front Left)".to_string(),
                    is_left: true,
                    is_right: false,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 1,
                    id: "fr".to_string(),
                    name: "FR (右声道 / Front Right)".to_string(),
                    is_left: false,
                    is_right: true,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
            ],
            3 => vec![
                AudioChannelItem {
                    index: 0,
                    id: "fl".to_string(),
                    name: "FL (左声道 / Front Left)".to_string(),
                    is_left: true,
                    is_right: false,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 1,
                    id: "fr".to_string(),
                    name: "FR (右声道 / Front Right)".to_string(),
                    is_left: false,
                    is_right: true,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 2,
                    id: "lfe".to_string(),
                    name: "LFE (低音炮 / Subwoofer)".to_string(),
                    is_left: false,
                    is_right: false,
                    is_center: true,
                    playback: true,
                    recognition: false,
                },
            ],
            4 => vec![
                AudioChannelItem {
                    index: 0,
                    id: "fl".to_string(),
                    name: "FL (前左 / Front Left)".to_string(),
                    is_left: true,
                    is_right: false,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 1,
                    id: "fr".to_string(),
                    name: "FR (前右 / Front Right)".to_string(),
                    is_left: false,
                    is_right: true,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 2,
                    id: "bl".to_string(),
                    name: "BL (后左 / Back Left)".to_string(),
                    is_left: true,
                    is_right: false,
                    is_center: false,
                    playback: true,
                    recognition: false,
                },
                AudioChannelItem {
                    index: 3,
                    id: "br".to_string(),
                    name: "BR (后右 / Back Right)".to_string(),
                    is_left: false,
                    is_right: true,
                    is_center: false,
                    playback: true,
                    recognition: false,
                },
            ],
            5 => vec![
                AudioChannelItem {
                    index: 0,
                    id: "fl".to_string(),
                    name: "FL (前左 / Front Left)".to_string(),
                    is_left: true,
                    is_right: false,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 1,
                    id: "fr".to_string(),
                    name: "FR (前右 / Front Right)".to_string(),
                    is_left: false,
                    is_right: true,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 2,
                    id: "fc".to_string(),
                    name: "FC (中置对白 / Center Dialogue)".to_string(),
                    is_left: false,
                    is_right: false,
                    is_center: true,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 3,
                    id: "sl".to_string(),
                    name: "SL (左环绕 / Surround Left)".to_string(),
                    is_left: true,
                    is_right: false,
                    is_center: false,
                    playback: true,
                    recognition: false,
                },
                AudioChannelItem {
                    index: 4,
                    id: "sr".to_string(),
                    name: "SR (右环绕 / Surround Right)".to_string(),
                    is_left: false,
                    is_right: true,
                    is_center: false,
                    playback: true,
                    recognition: false,
                },
            ],
            6 => vec![
                AudioChannelItem {
                    index: 0,
                    id: "fl".to_string(),
                    name: "FL (前左 / Front Left)".to_string(),
                    is_left: true,
                    is_right: false,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 1,
                    id: "fr".to_string(),
                    name: "FR (前右 / Front Right)".to_string(),
                    is_left: false,
                    is_right: true,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 2,
                    id: "fc".to_string(),
                    name: "FC (中置对白 / Center Dialogue)".to_string(),
                    is_left: false,
                    is_right: false,
                    is_center: true,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 3,
                    id: "lfe".to_string(),
                    name: "LFE (低音炮 / Subwoofer)".to_string(),
                    is_left: false,
                    is_right: false,
                    is_center: true,
                    playback: true,
                    recognition: false,
                },
                AudioChannelItem {
                    index: 4,
                    id: "sl".to_string(),
                    name: "SL (左环绕 / Surround Left)".to_string(),
                    is_left: true,
                    is_right: false,
                    is_center: false,
                    playback: true,
                    recognition: false,
                },
                AudioChannelItem {
                    index: 5,
                    id: "sr".to_string(),
                    name: "SR (右环绕 / Surround Right)".to_string(),
                    is_left: false,
                    is_right: true,
                    is_center: false,
                    playback: true,
                    recognition: false,
                },
            ],
            8 => vec![
                AudioChannelItem {
                    index: 0,
                    id: "fl".to_string(),
                    name: "FL (前左 / Front Left)".to_string(),
                    is_left: true,
                    is_right: false,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 1,
                    id: "fr".to_string(),
                    name: "FR (前右 / Front Right)".to_string(),
                    is_left: false,
                    is_right: true,
                    is_center: false,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 2,
                    id: "fc".to_string(),
                    name: "FC (中置对白 / Center Dialogue)".to_string(),
                    is_left: false,
                    is_right: false,
                    is_center: true,
                    playback: true,
                    recognition: true,
                },
                AudioChannelItem {
                    index: 3,
                    id: "lfe".to_string(),
                    name: "LFE (低音炮 / Subwoofer)".to_string(),
                    is_left: false,
                    is_right: false,
                    is_center: true,
                    playback: true,
                    recognition: false,
                },
                AudioChannelItem {
                    index: 4,
                    id: "bl".to_string(),
                    name: "BL (后左 / Back Left)".to_string(),
                    is_left: true,
                    is_right: false,
                    is_center: false,
                    playback: true,
                    recognition: false,
                },
                AudioChannelItem {
                    index: 5,
                    id: "br".to_string(),
                    name: "BR (后右 / Back Right)".to_string(),
                    is_left: false,
                    is_right: true,
                    is_center: false,
                    playback: true,
                    recognition: false,
                },
                AudioChannelItem {
                    index: 6,
                    id: "sl".to_string(),
                    name: "SL (左侧环绕 / Side Left)".to_string(),
                    is_left: true,
                    is_right: false,
                    is_center: false,
                    playback: true,
                    recognition: false,
                },
                AudioChannelItem {
                    index: 7,
                    id: "sr".to_string(),
                    name: "SR (右侧环绕 / Side Right)".to_string(),
                    is_left: false,
                    is_right: true,
                    is_center: false,
                    playback: true,
                    recognition: false,
                },
            ],
            other => (0..other)
                .map(|i| AudioChannelItem {
                    index: i,
                    id: format!("c{}", i),
                    name: format!("CH {} (声道 {})", i + 1, i + 1),
                    is_left: i % 2 == 0,
                    is_right: i % 2 != 0,
                    is_center: false,
                    playback: true,
                    recognition: true,
                })
                .collect(),
        }
    }
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
    #[serde(default)]
    pub audio_channels: Vec<AudioChannelItem>,
    #[serde(default)]
    pub is_task_running: bool,
    pub created_at_sec: u64,
    pub last_played_sec: u64,
    pub duration_ms: i64,
    pub subtitles: SubtitleTimeline,
}

fn default_video_recognition() -> RecognitionSettings {
    RecognitionSettings {
        background_noise: 0.6,
        pause_tolerance: 1.0,
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
            audio_channels: AudioChannelItem::default_for_count(2),
            is_task_running: false,
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
