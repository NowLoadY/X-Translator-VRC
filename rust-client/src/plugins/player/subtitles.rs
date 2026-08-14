use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleCue {
    pub id: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub speaker_name: Option<String>,
    pub original_text: String,
    pub translated_text: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SubtitleTimeline {
    cues: Vec<SubtitleCue>,
    pub enabled: bool,
}

impl SubtitleTimeline {
    pub fn new() -> Self {
        Self {
            cues: Vec::new(),
            enabled: true,
        }
    }

    pub fn add_cue(&mut self, cue: SubtitleCue) -> bool {
        if cue.original_text.trim().is_empty()
            && cue
                .translated_text
                .as_ref()
                .map_or(true, |t| t.trim().is_empty())
        {
            return false;
        }

        if let Some(existing) = self.cues.iter_mut().find(|c| c.id == cue.id) {
            if *existing == cue {
                return false;
            }
            *existing = cue;
            true
        } else if let Some(existing) = self
            .cues
            .iter_mut()
            .find(|c| (c.start_ms - cue.start_ms).abs() <= 600)
        {
            if *existing == cue {
                return false;
            }
            *existing = cue;
            true
        } else {
            self.cues.push(cue);
            self.cues.sort_by_key(|c| c.start_ms);
            true
        }
    }

    pub fn active_cue_at(&self, current_ms: i64) -> Option<&SubtitleCue> {
        if !self.enabled {
            return None;
        }
        // Subtitle lead-in time: display subtitles slightly ahead of speech onset
        // (~250ms advance) to match natural human reading rhythm and visual perception.
        const SUBTITLE_LEAD_IN_MS: i64 = 150;
        let query_ms = current_ms + SUBTITLE_LEAD_IN_MS;
        self.cues.iter().find(|cue| {
            let effective_end = if cue.end_ms <= cue.start_ms {
                cue.start_ms + 3000
            } else {
                cue.end_ms.max(cue.start_ms + 2000)
            };
            query_ms >= cue.start_ms && current_ms <= effective_end
        })
    }

    pub fn count(&self) -> usize {
        self.cues.len()
    }

    pub fn cues(&self) -> &[SubtitleCue] {
        &self.cues
    }

    pub fn export_srt(&self) -> String {
        let mut out = String::new();
        for (idx, cue) in self.cues().iter().enumerate() {
            let start = format_timestamp_srt(cue.start_ms);
            let end = format_timestamp_srt(cue.end_ms.max(cue.start_ms + 2000));
            out.push_str(&format!("{}\n{} --> {}\n", idx + 1, start, end));
            if let Some(speaker) = &cue.speaker_name {
                out.push_str(&format!("[{}] ", speaker));
            }
            out.push_str(&cue.original_text);
            out.push('\n');
            if let Some(trans) = &cue.translated_text {
                if trans != &cue.original_text && !trans.trim().is_empty() {
                    out.push_str(trans);
                    out.push('\n');
                }
            }
            out.push('\n');
        }
        out
    }
}

fn format_timestamp_srt(ms: i64) -> String {
    let ms_max = ms.max(0);
    let hours = ms_max / 3600000;
    let mins = (ms_max % 3600000) / 60000;
    let secs = (ms_max % 60000) / 1000;
    let millis = ms_max % 1000;
    format!("{:02}:{:02}:{:02},{:03}", hours, mins, secs, millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_active_cues_and_srt_export() {
        let mut timeline = SubtitleTimeline::new();
        timeline.add_cue(SubtitleCue {
            id: "1".into(),
            start_ms: 1000,
            end_ms: 3000,
            speaker_name: Some("Speaker".into()),
            original_text: "Hello world".into(),
            translated_text: Some("你好世界".into()),
        });
        assert_eq!(timeline.count(), 1);
        assert_eq!(timeline.cues().len(), 1);
        // With 250ms lead-in, cue starting at 1000ms is visible from 750ms through 3000ms
        assert!(timeline.active_cue_at(740).is_none());
        assert!(timeline.active_cue_at(750).is_some());
        assert!(timeline.active_cue_at(2000).is_some());
        assert!(timeline.active_cue_at(3000).is_some());
        assert!(timeline.active_cue_at(3001).is_none());
        let srt = timeline.export_srt();
        assert!(srt.contains("Hello world"));
        assert!(srt.contains("你好世界"));
    }

    #[test]
    fn test_streaming_cue_in_place_revision() {
        let mut timeline = SubtitleTimeline::new();
        timeline.add_cue(SubtitleCue {
            id: "stream_1_56000".into(),
            start_ms: 56000,
            end_ms: 57200,
            speaker_name: None,
            original_text: "に願いを。".into(),
            translated_text: Some("将愿望寄托在缝隙中。".into()),
        });
        assert_eq!(timeline.count(), 1);

        // Streaming update arrives for the same sentence
        timeline.add_cue(SubtitleCue {
            id: "stream_1_56000".into(),
            start_ms: 56000,
            end_ms: 58100,
            speaker_name: None,
            original_text: "に願い愛を。".into(),
            translated_text: Some("将愿望寄托在爱。".into()),
        });
        assert_eq!(timeline.count(), 1);
        assert_eq!(timeline.cues()[0].original_text, "に願い愛を。");
        assert_eq!(timeline.cues()[0].translated_text.as_deref(), Some("将愿望寄托在爱。"));

        // Final streaming revision arrives
        timeline.add_cue(SubtitleCue {
            id: "stream_1_56000".into(),
            start_ms: 56000,
            end_ms: 59000,
            speaker_name: None,
            original_text: "に願い愛一つ。".into(),
            translated_text: Some("将愿望寄托在爱中。".into()),
        });
        assert_eq!(timeline.count(), 1);
        assert_eq!(timeline.cues()[0].original_text, "に願い愛一つ。");
    }
}
