use xr_corpus_protocol::RecordTranslationRequest;

/// Joins successful translation segments into one logical dialogue turn.
///
/// Segmentation is an inference/subtitle concern and must not consume history
/// capacity. Space-delimited languages need a separator between independently
/// trimmed segments, while Chinese and Japanese punctuation already carries
/// the visual boundary.
pub(crate) fn join_segments<'a>(
    language: &str,
    segments: impl IntoIterator<Item = &'a str>,
) -> String {
    let mut joined = String::new();
    let separate = !matches!(base_language(language), "zh" | "ja");
    for segment in segments {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        if separate && !joined.is_empty() {
            joined.push(' ');
        }
        joined.push_str(segment);
    }
    joined
}

fn base_language(language: &str) -> &str {
    language.trim().split(['-', '_']).next().unwrap_or_default()
}

pub(crate) struct LogicalTurnRecord<'a> {
    pub(crate) context_id: u64,
    pub(crate) turn_id: String,
    pub(crate) speaker_id: String,
    pub(crate) source_language: &'a str,
    pub(crate) target_language: &'a str,
    pub(crate) completed_pairs: &'a [(String, String)],
}

impl LogicalTurnRecord<'_> {
    /// Collapses successful subtitle segments into the single history update
    /// expected by XR Corpus. Failed segments are absent from `completed_pairs`,
    /// so source and translation remain aligned.
    pub(crate) fn into_request(self) -> Option<RecordTranslationRequest> {
        if self.completed_pairs.is_empty() {
            return None;
        }
        Some(RecordTranslationRequest {
            context_id: self.context_id,
            turn_id: Some(self.turn_id),
            speaker_id: self.speaker_id,
            source_language: self.source_language.to_owned(),
            target_language: self.target_language.to_owned(),
            source_text: join_segments(
                self.source_language,
                self.completed_pairs
                    .iter()
                    .map(|(source, _)| source.as_str()),
            ),
            translated_text: join_segments(
                self.target_language,
                self.completed_pairs
                    .iter()
                    .map(|(_, translated)| translated.as_str()),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speak_segments_form_one_language_appropriate_turn() {
        assert_eq!(
            join_segments("en", ["Good morning.", "How are you?"]),
            "Good morning. How are you?"
        );
        assert_eq!(
            join_segments("zh-CN", ["早上好。", "你好吗？"]),
            "早上好。你好吗？"
        );
        assert_eq!(
            join_segments("ja", ["おはよう。", "元気ですか？"]),
            "おはよう。元気ですか？"
        );
    }

    #[test]
    fn completed_segments_create_one_logical_history_request() {
        let pairs = vec![
            ("Good morning.".into(), "早上好。".into()),
            ("How are you?".into(), "你好吗？".into()),
        ];
        let request = LogicalTurnRecord {
            context_id: 7,
            turn_id: "speak-3".into(),
            speaker_id: "speaker-01".into(),
            source_language: "en",
            target_language: "zh",
            completed_pairs: &pairs,
        }
        .into_request()
        .unwrap();

        assert_eq!(request.turn_id.as_deref(), Some("speak-3"));
        assert_eq!(request.source_text, "Good morning. How are you?");
        assert_eq!(request.translated_text, "早上好。你好吗？");
    }
}
