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

    pub fn clear(&mut self) {
        self.cues.clear();
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

        // 1. Check exact ID match (turn_id or stream_id based)
        if let Some(existing) = self.cues.iter_mut().find(|c| c.id == cue.id) {
            if *existing == cue {
                return false;
            }
            *existing = cue;
            return true;
        }

        // 2. Check semantic & temporal duplication:
        // If identical text within 3500ms OR overlapping time window within 1000ms
        let cue_orig = cue.original_text.trim();
        if let Some(existing) = self.cues.iter_mut().find(|c| {
            let time_diff = (c.start_ms - cue.start_ms).abs();
            (time_diff <= 3500 && c.original_text.trim() == cue_orig) || time_diff <= 1000
        }) {
            if *existing == cue {
                return false;
            }
            *existing = cue;
            return true;
        }

        // 3. New distinct subtitle
        self.cues.push(cue);
        self.cues.sort_by_key(|c| c.start_ms);
        true
    }

    pub fn active_cue_at(&self, current_ms: i64) -> Option<&SubtitleCue> {
        if !self.enabled {
            return None;
        }
        // Subtitle lead-in time: display subtitles slightly ahead of speech onset
        // (~250ms advance) to match natural human reading rhythm and visual perception.
        const SUBTITLE_LEAD_IN_MS: i64 = 250;
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
            let orig = cue.original_text.trim();
            if !orig.is_empty() {
                out.push_str(orig);
                out.push('\n');
            }
            if let Some(trans) = &cue.translated_text {
                let trans_trim = trans.trim();
                if trans_trim != orig && !trans_trim.is_empty() {
                    out.push_str(trans_trim);
                    out.push('\n');
                }
            }
            out.push('\n');
        }
        out
    }

    pub fn export_lrc(&self, title: Option<&str>) -> String {
        let mut out = String::new();
        if let Some(t) = title {
            let clean = t.trim();
            if !clean.is_empty() {
                out.push_str(&format!("[ti:{}]\n", clean));
            }
        }
        for cue in self.cues() {
            let time_tag = format_timestamp_lrc(cue.start_ms);
            let orig = cue.original_text.trim();
            if !orig.is_empty() {
                out.push_str(&time_tag);
                out.push_str(orig);
                out.push('\n');
            }
            if let Some(trans) = &cue.translated_text {
                let trans_trim = trans.trim();
                if trans_trim != orig && !trans_trim.is_empty() {
                    out.push_str(&time_tag);
                    out.push_str(trans_trim);
                    out.push('\n');
                }
            }
        }
        out
    }
}

fn format_timestamp_lrc(ms: i64) -> String {
    let ms_max = ms.max(0);
    let mins = ms_max / 60000;
    let secs = (ms_max % 60000) / 1000;
    let hundredths = (ms_max % 1000) / 10;
    format!("[{:02}:{:02}.{:02}]", mins, secs, hundredths)
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
        assert!(!srt.contains("[Speaker]"));
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
        assert_eq!(
            timeline.cues()[0].translated_text.as_deref(),
            Some("将愿望寄托在爱。")
        );

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

    #[test]
    fn test_lrc_export() {
        let mut timeline = SubtitleTimeline::new();
        timeline.add_cue(SubtitleCue {
            id: "1".into(),
            start_ms: 13504,
            end_ms: 16480,
            speaker_name: None,
            original_text: "Now I'm seventeen.".into(),
            translated_text: Some("我现在十七岁了。".into()),
        });
        timeline.add_cue(SubtitleCue {
            id: "2".into(),
            start_ms: 16544,
            end_ms: 18544,
            speaker_name: None,
            original_text: "My sky.".into(),
            translated_text: Some("我的天空。".into()),
        });

        let lrc = timeline.export_lrc(Some("17 - 椎名林檎"));
        assert!(lrc.contains("[ti:17 - 椎名林檎]"));
        assert!(lrc.contains("[00:13.50]Now I'm seventeen."));
        assert!(lrc.contains("[00:13.50]我现在十七岁了。"));
        assert!(lrc.contains("[00:16.54]My sky."));
        assert!(lrc.contains("[00:16.54]我的天空。"));
    }

    #[test]
    fn test_subtitles_deduplication_on_repeated_events() {
        let mut timeline = SubtitleTimeline::new();
        // Turn 1 first interim
        timeline.add_cue(SubtitleCue {
            id: "turn_1".into(),
            start_ms: 109000,
            end_ms: 111000,
            speaker_name: Some("speaker-04".into()),
            original_text: "You uh talked".into(),
            translated_text: Some("你刚才讨论了".into()),
        });
        assert_eq!(timeline.count(), 1);

        // Turn 1 second interim with slightly refined start timestamp
        timeline.add_cue(SubtitleCue {
            id: "turn_1".into(),
            start_ms: 109200,
            end_ms: 112000,
            speaker_name: Some("speaker-04".into()),
            original_text: "You uh talked to Louis about Sunday.".into(),
            translated_text: Some("你刚才和路易斯讨论了周日的事。".into()),
        });
        assert_eq!(timeline.count(), 1);
        assert_eq!(
            timeline.cues()[0].original_text,
            "You uh talked to Louis about Sunday."
        );

        // Semantic duplicate with different transient ID but same text and timestamp within 3.5s
        timeline.add_cue(SubtitleCue {
            id: "cue_109200".into(),
            start_ms: 109200,
            end_ms: 112000,
            speaker_name: Some("speaker-04".into()),
            original_text: "You uh talked to Louis about Sunday.".into(),
            translated_text: Some("你刚才和路易斯讨论了周日的事。".into()),
        });
        assert_eq!(timeline.count(), 1);
    }
}
