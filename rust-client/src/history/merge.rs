use super::model::*;

const STREAM_TEXT_LIMIT: usize = 4_096;

pub(crate) fn collect_recognition_window(
    pending: &mut Vec<PendingRecognitionWindow>,
    stream_id: u64,
    continuous: bool,
    segment_index: u32,
    segment_count: u32,
    entry: RecognitionHistoryEntry,
) -> Option<RecognitionHistoryEntry> {
    let turn_id = entry.turn_id.clone();
    let index = pending
        .iter()
        .position(|window| window.stream_id == stream_id && window.turn_id == turn_id)
        .unwrap_or_else(|| {
            if pending.len() >= 32 {
                pending.remove(0);
            }
            pending.push(PendingRecognitionWindow {
                stream_id,
                continuous,
                turn_id,
                segment_count: segment_count.max(1),
                segments: Vec::new(),
            });
            pending.len() - 1
        });
    let window = &mut pending[index];
    window.segment_count = window.segment_count.max(segment_count.max(1));
    if let Some((_, existing)) = window
        .segments
        .iter_mut()
        .find(|(index, _)| *index == segment_index)
    {
        *existing = entry;
    } else {
        window.segments.push((segment_index, entry));
    }
    if window.segments.len() < window.segment_count as usize {
        return None;
    }

    let mut window = pending.remove(index);
    window.segments.sort_by_key(|(index, _)| *index);
    let (_, first) = window.segments.first()?.clone();
    let mut combined = RecognitionHistoryEntry {
        stream_id: window.continuous.then_some(window.stream_id),
        live: window.continuous,
        text: String::new(),
        source_start_ms: first.source_start_ms,
        source_end_ms: first.source_end_ms,
        activation_matches: Vec::new(),
        context_matches: Vec::new(),
        revision: None,
        ..first
    };
    for (_, segment) in window.segments {
        let position = crate::streaming::append_segment(&mut combined.text, &segment.text);
        crate::streaming::append_term_matches(
            &mut combined.activation_matches,
            &segment.activation_matches,
            position,
        );
        crate::streaming::append_term_matches(
            &mut combined.context_matches,
            &segment.context_matches,
            position,
        );
        if !segment.speaker_id.is_empty() {
            combined.speaker_id = segment.speaker_id;
        }
        combined.source_start_ms = combined.source_start_ms.min(segment.source_start_ms);
        combined.source_end_ms = combined.source_end_ms.max(segment.source_end_ms);
    }
    Some(combined)
}

pub(crate) fn merge_stream_recognition(
    history: &mut Vec<RecognitionHistoryEntry>,
    stream_id: u64,
    mut fragment: RecognitionHistoryEntry,
) {
    retain_recognition_tail(&mut fragment);
    let Some(current) = history
        .iter_mut()
        .rfind(|entry| entry.stream_id == Some(stream_id) && entry.live)
    else {
        initialize_recognition_revision(&mut fragment);
        history.push(fragment);
        return;
    };

    let stable = current
        .revision
        .as_ref()
        .map(crate::streaming::RevisableText::stable_text)
        .filter(|text| !text.is_empty())
        .unwrap_or(&current.text);
    if crate::streaming::should_roll_caption(
        stable,
        current.source_start_ms,
        fragment.source_end_ms,
    ) {
        let handoff = current.revision.as_ref().map_or_else(
            || {
                crate::streaming::handoff_text(
                    &current.text,
                    &fragment.text,
                    fragment.overlap_ratio,
                )
            },
            |revision| revision.handoff(&fragment.text, fragment.overlap_ratio),
        );
        if !handoff.text.trim().is_empty() {
            current.live = false;
            fragment.text = handoff.text;
            fragment.activation_matches =
                trimmed_term_matches(&fragment.activation_matches, handoff.source_start);
            fragment.context_matches =
                trimmed_term_matches(&fragment.context_matches, handoff.source_start);
            initialize_recognition_revision(&mut fragment);
            history.push(fragment);
            return;
        }
    }

    if fragment.revisable {
        let update = current
            .revision
            .get_or_insert_with(|| crate::streaming::RevisableText::new(&current.text))
            .update(&fragment.text, fragment.overlap_ratio);
        merge_revision_matches(
            &mut current.activation_matches,
            &fragment.activation_matches,
            update.hypothesis_start,
        );
        merge_revision_matches(
            &mut current.context_matches,
            &fragment.context_matches,
            update.hypothesis_start,
        );
        current.text = update.text;
    } else {
        let position = crate::streaming::append_text(&mut current.text, &fragment.text);
        crate::streaming::append_term_matches(
            &mut current.activation_matches,
            &fragment.activation_matches,
            position,
        );
        crate::streaming::append_term_matches(
            &mut current.context_matches,
            &fragment.context_matches,
            position,
        );
    }
    if !fragment.speaker_id.is_empty() {
        current.speaker_id = fragment.speaker_id;
    }
    current.source_start_ms = current.source_start_ms.min(fragment.source_start_ms);
    current.source_end_ms = current.source_end_ms.max(fragment.source_end_ms);
    retain_recognition_tail(current);
}

fn initialize_recognition_revision(entry: &mut RecognitionHistoryEntry) {
    if entry.revisable {
        entry.revision = Some(crate::streaming::RevisableText::new(&entry.text));
    }
}

pub(crate) fn merge_stream_translation(
    history: &mut Vec<TranslationHistoryEntry>,
    stream_id: u64,
    mut fragment: TranslationHistoryEntry,
) -> StreamMerge {
    crate::streaming::retain_tail(&mut fragment.source, None, STREAM_TEXT_LIMIT);
    crate::streaming::retain_tail(
        &mut fragment.translated,
        Some(&mut fragment.term_matches),
        STREAM_TEXT_LIMIT,
    );
    let Some(current) = history
        .iter_mut()
        .rfind(|entry| entry.stream_id == Some(stream_id) && entry.live)
    else {
        initialize_revision(&mut fragment);
        history.push(fragment.clone());
        return StreamMerge {
            entry: fragment,
            rolled_over: false,
            changed: true,
        };
    };

    let stable_source = current
        .source_revision
        .as_ref()
        .map(crate::streaming::RevisableText::stable_text)
        .filter(|text| !text.is_empty())
        .unwrap_or(&current.source);
    if crate::streaming::should_roll_caption(
        stable_source,
        current.source_start_ms,
        fragment.source_end_ms,
    ) {
        let source = current.source_revision.as_ref().map_or_else(
            || {
                crate::streaming::handoff_text(
                    &current.source,
                    &fragment.source,
                    fragment.overlap_ratio,
                )
            },
            |revision| revision.handoff(&fragment.source, fragment.overlap_ratio),
        );
        if !source.text.trim().is_empty() {
            let translated = current.translated_revision.as_ref().map_or_else(
                || {
                    crate::streaming::handoff_text(
                        &current.translated,
                        &fragment.translated,
                        fragment.overlap_ratio,
                    )
                },
                |revision| revision.handoff(&fragment.translated, fragment.overlap_ratio),
            );
            current.live = false;
            fragment.source = source.text;
            fragment.translated = translated.text;
            fragment.term_matches =
                trimmed_term_matches(&fragment.term_matches, translated.source_start);
            initialize_revision(&mut fragment);
            history.push(fragment.clone());
            return StreamMerge {
                entry: fragment,
                rolled_over: true,
                changed: true,
            };
        }
    }

    let (source_changed, translated_changed) = if fragment.revisable {
        let old_source = current.source.clone();
        let old_translated = current.translated.clone();
        let source = current
            .source_revision
            .get_or_insert_with(|| crate::streaming::RevisableText::new(&current.source))
            .update(&fragment.source, fragment.overlap_ratio);
        let translated = current
            .translated_revision
            .get_or_insert_with(|| crate::streaming::RevisableText::new(&current.translated))
            .update(&fragment.translated, fragment.overlap_ratio);
        current.source = source.text;
        current.translated = translated.text;
        merge_revision_matches(
            &mut current.term_matches,
            &fragment.term_matches,
            translated.hypothesis_start,
        );
        (
            current.source != old_source,
            current.translated != old_translated,
        )
    } else {
        let source_changed =
            crate::streaming::append_text(&mut current.source, &fragment.source).is_some();
        let translated_offset =
            crate::streaming::append_text(&mut current.translated, &fragment.translated);
        let translated_changed = translated_offset.is_some();
        crate::streaming::append_term_matches(
            &mut current.term_matches,
            &fragment.term_matches,
            translated_offset,
        );
        (source_changed, translated_changed)
    };
    crate::streaming::retain_tail(&mut current.source, None, STREAM_TEXT_LIMIT);
    crate::streaming::retain_tail(
        &mut current.translated,
        Some(&mut current.term_matches),
        STREAM_TEXT_LIMIT,
    );
    if !fragment.speaker_id.is_empty() {
        current.speaker_id = fragment.speaker_id;
    }
    current.source_start_ms = current.source_start_ms.min(fragment.source_start_ms);
    current.source_end_ms = current.source_end_ms.max(fragment.source_end_ms);
    StreamMerge {
        entry: current.clone(),
        rolled_over: false,
        changed: source_changed || translated_changed,
    }
}

fn initialize_revision(entry: &mut TranslationHistoryEntry) {
    if entry.revisable {
        entry.source_revision = Some(crate::streaming::RevisableText::new(&entry.source));
        entry.translated_revision = Some(crate::streaming::RevisableText::new(&entry.translated));
    }
}

fn shifted_term_matches(
    matches: &[xrtranslate_protocol::CorpusTermMatch],
    offset: usize,
) -> Vec<xrtranslate_protocol::CorpusTermMatch> {
    let Ok(offset) = u32::try_from(offset) else {
        return Vec::new();
    };
    matches
        .iter()
        .cloned()
        .filter_map(|mut term| {
            term.start_byte = term.start_byte.checked_add(offset)?;
            term.end_byte = term.end_byte.checked_add(offset)?;
            Some(term)
        })
        .collect()
}

fn trimmed_term_matches(
    matches: &[xrtranslate_protocol::CorpusTermMatch],
    source_start: usize,
) -> Vec<xrtranslate_protocol::CorpusTermMatch> {
    let Ok(source_start) = u32::try_from(source_start) else {
        return Vec::new();
    };
    matches
        .iter()
        .cloned()
        .filter_map(|mut term| {
            if term.start_byte < source_start {
                return None;
            }
            term.start_byte = term.start_byte.checked_sub(source_start)?;
            term.end_byte = term.end_byte.checked_sub(source_start)?;
            Some(term)
        })
        .collect()
}

fn merge_revision_matches(
    current: &mut Vec<xrtranslate_protocol::CorpusTermMatch>,
    incoming: &[xrtranslate_protocol::CorpusTermMatch],
    hypothesis_start: usize,
) {
    let Ok(stable_end) = u32::try_from(hypothesis_start) else {
        current.clear();
        return;
    };
    current.retain(|term| term.end_byte <= stable_end);
    current.extend(shifted_term_matches(incoming, hypothesis_start));
}

fn retain_recognition_tail(entry: &mut RecognitionHistoryEntry) {
    let original_len = entry.text.len();
    crate::streaming::retain_tail(
        &mut entry.text,
        Some(&mut entry.activation_matches),
        STREAM_TEXT_LIMIT,
    );
    let removed = original_len.saturating_sub(entry.text.len());
    if removed > 0 {
        entry.context_matches = trimmed_term_matches(&entry.context_matches, removed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_settings::CaptureSource;

    fn fragment(stream_id: u64, source: &str, translated: &str) -> TranslationHistoryEntry {
        TranslationHistoryEntry {
            turn_id: String::new(),
            stream_id: Some(stream_id),
            audio_source: CaptureSource::Microphone,
            live: true,
            source: source.into(),
            translated: translated.into(),
            speaker_id: String::new(),
            source_start_ms: 0.0,
            source_end_ms: 1.0,
            term_matches: Vec::new(),
            revisable: false,
            overlap_ratio: 0.0,
            source_revision: None,
            translated_revision: None,
        }
    }

    fn snapshot(stream_id: u64, source: &str, translated: &str) -> TranslationHistoryEntry {
        TranslationHistoryEntry {
            revisable: true,
            overlap_ratio: 0.34,
            ..fragment(stream_id, source, translated)
        }
    }

    fn recognition_snapshot(stream_id: u64, turn_id: &str, text: &str) -> RecognitionHistoryEntry {
        RecognitionHistoryEntry {
            stream_id: Some(stream_id),
            live: true,
            text: text.into(),
            turn_id: turn_id.into(),
            speaker_id: String::new(),
            source_start_ms: 0.0,
            source_end_ms: 1_000.0,
            activation_matches: Vec::new(),
            context_matches: Vec::new(),
            revisable: true,
            overlap_ratio: 0.34,
            revision: None,
        }
    }

    #[test]
    fn streaming_translation_updates_each_audio_source_in_place() {
        let mut history = Vec::new();
        merge_stream_translation(&mut history, 1, fragment(1, "Hello", "你好"));
        merge_stream_translation(&mut history, 2, fragment(2, "Music", "音乐"));
        let microphone =
            merge_stream_translation(&mut history, 1, fragment(1, "world", "你好世界"));

        assert_eq!(history.len(), 2);
        assert_eq!(microphone.entry.source, "Hello world");
        assert_eq!(microphone.entry.translated, "你好世界");
        assert_eq!(history[1].source, "Music");
    }

    #[test]
    fn streaming_translation_rolls_a_finished_caption_into_history() {
        let mut history = Vec::new();
        let mut first = fragment(
            1,
            "This is a complete first sentence with enough stable words to roll cleanly.",
            "这是完整的第一句。",
        );
        first.source_end_ms = 4_000.0;
        merge_stream_translation(&mut history, 1, first);
        let mut next = fragment(1, "Next", "下一句");
        next.source_start_ms = 4_000.0;
        next.source_end_ms = 5_000.0;
        let update = merge_stream_translation(&mut history, 1, next);

        assert!(update.rolled_over);
        assert_eq!(history.len(), 2);
        assert!(!history[0].live);
        assert!(history[1].live);
    }

    #[test]
    fn revisable_windows_replace_the_unstable_tail() {
        let mut history = Vec::new();
        merge_stream_translation(
            &mut history,
            1,
            snapshot(1, "we walk across the central street", "我们走过中央大街"),
        );
        let update = merge_stream_translation(
            &mut history,
            1,
            snapshot(1, "the central station and turn left", "中央车站然后左转"),
        );

        assert_eq!(
            update.entry.source,
            "we walk across the central station and turn left"
        );
        assert_eq!(update.entry.translated, "我们走过中央车站然后左转");
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn provisional_window_punctuation_does_not_roll_a_live_caption() {
        let mut history = Vec::new();
        let mut first = snapshot(1, "A short provisional sentence.", "一个临时短句。");
        first.source_end_ms = 2_000.0;
        merge_stream_translation(&mut history, 1, first);

        let mut next = snapshot(1, "provisional sentence continues here", "临时短句仍在继续");
        next.source_start_ms = 1_000.0;
        next.source_end_ms = 5_000.0;
        let update = merge_stream_translation(&mut history, 1, next);

        assert!(!update.rolled_over);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn caption_rollover_consumes_the_shared_window_prefix() {
        let mut history = Vec::new();
        let mut first = snapshot(
            1,
            "I frown, but you use it to end the sentence with a period.",
            "I frown, but you use it to end the sentence with a period.",
        );
        first.source_end_ms = 4_000.0;
        merge_stream_translation(&mut history, 1, first);

        let mut next = snapshot(
            1,
            "use it to end the sentence with a period. I can only say I admit it.",
            "use it to end the sentence with a period. I can only say I admit it.",
        );
        next.source_start_ms = 2_500.0;
        next.source_end_ms = 5_500.0;
        let update = merge_stream_translation(&mut history, 1, next);

        assert!(update.rolled_over);
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].source, "I can only say I admit it.");
        assert_eq!(history[1].translated, "I can only say I admit it.");
    }

    #[test]
    fn recognition_window_combines_all_segments_before_display() {
        let mut pending = Vec::<PendingRecognitionWindow>::new();
        let second = recognition_snapshot(7, "turn-1", "我的台词全念一遍。");
        let first = recognition_snapshot(7, "turn-1", "准备好的台词。");

        assert!(collect_recognition_window(&mut pending, 7, true, 2, 2, second).is_none());
        let combined = collect_recognition_window(&mut pending, 7, true, 1, 2, first).unwrap();

        assert_eq!(combined.text, "准备好的台词。我的台词全念一遍。");
        assert!(pending.is_empty());
    }

    #[test]
    fn recognition_history_revises_the_shared_audio_window_in_place() {
        let mut history = Vec::new();
        merge_stream_recognition(
            &mut history,
            7,
            recognition_snapshot(7, "turn-1", "你停在了这条我们熟悉的街。"),
        );
        let mut next = recognition_snapshot(7, "turn-2", "熟悉的街。然后继续向前走。");
        next.source_start_ms = 1_000.0;
        next.source_end_ms = 3_000.0;
        merge_stream_recognition(&mut history, 7, next);

        assert_eq!(history.len(), 1);
        assert_eq!(
            history[0].text,
            "你停在了这条我们熟悉的街。然后继续向前走。"
        );
    }
}
