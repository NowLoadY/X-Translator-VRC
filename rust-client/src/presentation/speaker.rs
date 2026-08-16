/// Converts a diarization token into the compact label used by host and plugin
/// presentation surfaces.
pub(crate) fn compact_speaker_label(speaker_id: &str) -> Option<String> {
    let value = speaker_id.trim();
    if value.is_empty() {
        return None;
    }
    let suffix = value.strip_prefix("speaker-").unwrap_or(value);
    if suffix.eq_ignore_ascii_case("unknown") {
        return Some("S?".into());
    }
    let sequence = suffix.trim_start_matches('0');
    Some(format!(
        "S{}",
        if sequence.is_empty() { "0" } else { sequence }
    ))
}

#[cfg(test)]
mod tests {
    use super::compact_speaker_label;

    #[test]
    fn labels_are_stable_and_human_readable() {
        assert_eq!(compact_speaker_label("speaker-01").as_deref(), Some("S1"));
        assert_eq!(compact_speaker_label("speaker-12").as_deref(), Some("S12"));
        assert_eq!(
            compact_speaker_label("speaker-unknown").as_deref(),
            Some("S?")
        );
        assert_eq!(compact_speaker_label(""), None);
    }
}
