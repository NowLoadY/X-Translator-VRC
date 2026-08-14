use crate::CaptureSource;
use crate::i18n::UiLanguage;

/// Identifies the distinct owner or source controlling the active translation session.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TranslationSessionOwner {
    #[default]
    None,
    MainLive {
        capture_source: CaptureSource,
    },
    Meeting {
        meeting_id: String,
        is_imported: bool,
    },
    VideoPlayer {
        task_id: String,
    },
}

impl TranslationSessionOwner {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn is_active(&self) -> bool {
        !self.is_none()
    }

    pub fn is_main_live(&self) -> bool {
        matches!(self, Self::MainLive { .. })
    }

    pub fn is_meeting(&self) -> bool {
        matches!(self, Self::Meeting { .. })
    }

    pub fn is_video_player(&self) -> bool {
        matches!(self, Self::VideoPlayer { .. })
    }

    pub fn meeting_id(&self) -> Option<&str> {
        match self {
            Self::Meeting { meeting_id, .. } => Some(meeting_id.as_str()),
            _ => None,
        }
    }

    pub fn video_task_id(&self) -> Option<&str> {
        match self {
            Self::VideoPlayer { task_id } => Some(task_id.as_str()),
            _ => None,
        }
    }

    pub fn display_name(&self, language: UiLanguage) -> &'static str {
        match self {
            Self::None => match language {
                UiLanguage::Chinese => "空闲",
                UiLanguage::Japanese => "アイドル",
                UiLanguage::Korean => "대기 중",
                UiLanguage::Russian => "Свободно",
                UiLanguage::English => "Idle",
            },
            Self::MainLive { .. } => match language {
                UiLanguage::Chinese => "实时翻译",
                UiLanguage::Japanese => "リアルタイム翻訳",
                UiLanguage::Korean => "실시간 번역",
                UiLanguage::Russian => "Прямой перевод",
                UiLanguage::English => "Live Translation",
            },
            Self::Meeting { is_imported, .. } => {
                if *is_imported {
                    match language {
                        UiLanguage::Chinese => "会议录音导入",
                        UiLanguage::Japanese => "会議録音インポート",
                        UiLanguage::Korean => "회의 녹음 가져오기",
                        UiLanguage::Russian => "Импорт записи собрания",
                        UiLanguage::English => "Meeting Audio Import",
                    }
                } else {
                    match language {
                        UiLanguage::Chinese => "会议纪要录制",
                        UiLanguage::Japanese => "会議議事録",
                        UiLanguage::Korean => "회의록",
                        UiLanguage::Russian => "Протокол собрания",
                        UiLanguage::English => "Meeting Notes",
                    }
                }
            }
            Self::VideoPlayer { .. } => match language {
                UiLanguage::Chinese => "视频播放器",
                UiLanguage::Japanese => "動画プレーヤー",
                UiLanguage::Korean => "비디오 플레이어",
                UiLanguage::Russian => "Видеоплеер",
                UiLanguage::English => "Media Player",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_owner_predicates() {
        let none = TranslationSessionOwner::None;
        assert!(none.is_none());
        assert!(!none.is_active());

        let main = TranslationSessionOwner::MainLive {
            capture_source: CaptureSource::Microphone,
        };
        assert!(main.is_main_live());
        assert!(main.is_active());
        assert_eq!(main.display_name(UiLanguage::Chinese), "实时翻译");

        let meeting = TranslationSessionOwner::Meeting {
            meeting_id: "m123".into(),
            is_imported: false,
        };
        assert!(meeting.is_meeting());
        assert_eq!(meeting.meeting_id(), Some("m123"));
        assert_eq!(meeting.display_name(UiLanguage::English), "Meeting Notes");

        let video = TranslationSessionOwner::VideoPlayer {
            task_id: "v456".into(),
        };
        assert!(video.is_video_player());
        assert_eq!(video.video_task_id(), Some("v456"));
        assert_eq!(video.display_name(UiLanguage::Chinese), "视频播放器");
    }
}
