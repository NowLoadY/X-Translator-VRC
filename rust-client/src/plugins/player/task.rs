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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MediaType {
    #[default]
    Video,
    AudioOnly,
}

pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "flac", "aac", "ogg", "m4a", "opus", "wma", "ape", "alac",
];

pub fn detect_media_type(path: &Path) -> MediaType {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .filter(|ext| AUDIO_EXTENSIONS.contains(&ext.as_str()))
        .map_or(MediaType::Video, |_| MediaType::AudioOnly)
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
            1 => vec![AudioChannelItem {
                index: 0,
                id: "mono".to_string(),
                name: "Mono (单声道)".to_string(),
                is_left: true,
                is_right: true,
                is_center: true,
                playback: true,
                recognition: true,
            }],
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
    #[serde(default)]
    pub media_type: MediaType,
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
    #[allow(dead_code)]
    pub fn new(
        title: String,
        source: MediaSource,
        source_lang: String,
        target_lang: String,
        subtitle_mode: VideoSubtitleMode,
        recognition: RecognitionSettings,
    ) -> Self {
        let media_type = match &source {
            MediaSource::LocalFile(p) => detect_media_type(p),
            MediaSource::NetworkStream(_) => MediaType::Video,
        };
        Self::new_with_media_type(
            title,
            source,
            media_type,
            source_lang,
            target_lang,
            subtitle_mode,
            recognition,
        )
    }

    pub fn new_with_media_type(
        title: String,
        source: MediaSource,
        media_type: MediaType,
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
            media_type,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_media_type() {
        assert_eq!(
            detect_media_type(Path::new("song.mp3")),
            MediaType::AudioOnly
        );
        assert_eq!(
            detect_media_type(Path::new("audio.wav")),
            MediaType::AudioOnly
        );
        assert_eq!(
            detect_media_type(Path::new("track.flac")),
            MediaType::AudioOnly
        );
        assert_eq!(
            detect_media_type(Path::new("voice.m4a")),
            MediaType::AudioOnly
        );
        assert_eq!(
            detect_media_type(Path::new("podcast.ogg")),
            MediaType::AudioOnly
        );
        assert_eq!(
            detect_media_type(Path::new("record.opus")),
            MediaType::AudioOnly
        );
        assert_eq!(
            detect_media_type(Path::new("music.aac")),
            MediaType::AudioOnly
        );
        assert_eq!(detect_media_type(Path::new("video.mp4")), MediaType::Video);
        assert_eq!(detect_media_type(Path::new("movie.mkv")), MediaType::Video);
        assert_eq!(detect_media_type(Path::new("clip.webm")), MediaType::Video);
    }

    #[test]
    fn test_video_task_backwards_compatible_deserialization() {
        let json_without_media_type = r#"{
            "id": "test-123",
            "title": "Old Task",
            "source": {"LocalFile": "test.mp4"},
            "source_language": "ja",
            "target_language": "zh",
            "subtitle_mode": "RealtimeTranslation",
            "created_at_sec": 1000,
            "last_played_sec": 1000,
            "duration_ms": 5000,
            "subtitles": {"cues": [], "enabled": true}
        }"#;

        let task: VideoTask = serde_json::from_str(json_without_media_type).unwrap();
        assert_eq!(task.media_type, MediaType::Video);
    }
}
