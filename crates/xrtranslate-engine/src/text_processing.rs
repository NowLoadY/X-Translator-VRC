//! Text normalization shared by the native ASR and translation pipeline.
//!
//! The functions in this module mirror and extend the ASR and translation
//! processing paths. In particular, translation segmentation retains the original
//! source segment for display while stripping filler edges and validating content
//! across all supported languages (including Cyrillic, Latin script with accents,
//! CJK, Kana, Hangul, etc.) based on the active translation task.

/// Maximum number of Unicode scalar values held before a comma may split an
/// otherwise unfinished translation segment.
pub const TRANSLATION_SOFT_SEGMENT_LIMIT: usize = 72;

const HARD_TRANSLATION_BOUNDARIES: &[char] =
    &['。', '！', '？', '；', '：', '.', '!', '?', ';', ':'];
const SOFT_TRANSLATION_BOUNDARIES: &[char] = &['，', '、', ','];
const FILLER_WORDS_DEFAULT: &[char] = &['嗯', '啊', '呃', '额', '哦', '噢', '唉', '哎'];
const FILLER_PUNCTUATION: &[char] = &[
    '，', '。', '！', '？', '；', '：', '、', ',', '.', '!', '?', ';', ':', '~', '…', ' ',
];
const STUTTER_CHARACTERS: &[char] = &[
    '我', '你', '他', '她', '它', '这', '那', '对', '是', '有', '没', '好', '啊', '嗯', '哦', '呃',
    '就',
];

/// One source span prepared for translation.
///
/// [`translation_text`](Self::translation_text) has leading and trailing
/// filler words removed.  [`source_text`](Self::source_text) retains the
/// trimmed ASR text that should be shown to listeners.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranslationSegmentPair {
    /// Cleaned text supplied to translation and TTS.
    pub translation_text: String,
    /// Original, trimmed text supplied to the frontend.
    pub source_text: String,
}

/// Splits text at sentence endings and uses a comma only after 72 characters.
///
/// This reproduces the Python `split_translation_segments` behavior, including
/// its final `should_emit_segment(segment, 1)` filter.  Therefore an
/// unterminated tail is deliberately not emitted until it gains an accepted
/// terminal boundary.
pub fn split_translation_segments(text: &str) -> Vec<String> {
    split_translation_segments_internal(text, false)
}

fn split_translation_segments_internal(text: &str, emit_unterminated: bool) -> Vec<String> {
    let value = text.trim();
    if value.is_empty() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let mut buffer = Vec::new();
    for character in value.chars() {
        buffer.push(character);
        if HARD_TRANSLATION_BOUNDARIES.contains(&character) {
            push_translation_segment(&mut segments, &buffer, emit_unterminated);
            buffer.clear();
            continue;
        }

        while buffer.len() >= TRANSLATION_SOFT_SEGMENT_LIMIT {
            let soft_break = buffer
                .iter()
                .rposition(|character| SOFT_TRANSLATION_BOUNDARIES.contains(character));
            let cutoff = soft_break.map_or(TRANSLATION_SOFT_SEGMENT_LIMIT, |index| index + 1);
            push_translation_segment(&mut segments, &buffer[..cutoff], emit_unterminated);
            buffer = trim_start_chars(&buffer[cutoff..]).to_vec();
        }
    }

    push_translation_segment(&mut segments, &buffer, emit_unterminated);
    segments
}

/// Collapses only obvious adjacent ASR repetitions across all alphabetic and CJK scripts.
///
/// The transformation is repeated until stable: adjacent words (case-insensitively across
/// alphabetic scripts), punctuation-separated CJK phrases, repeated short CJK phrases,
/// and a set of repeated CJK stutter characters are collapsed. Non-adjacent repeated
/// words are retained.
pub fn remove_asr_stutters(text: &str) -> String {
    let mut value = text.trim().to_owned();
    if value.is_empty() {
        return value;
    }

    loop {
        let previous = value.clone();
        value = collapse_matches(&previous, repeated_word_end);
        value = collapse_matches(&value, repeated_cjk_phrase_end);
        value = collapse_matches(&value, repeated_cjk_short_phrase_end);
        value = collapse_matches(&value, repeated_cjk_stutter_character_end);
        if value == previous {
            return value.trim().to_owned();
        }
    }
}

/// Removes audio-overlap text repeated at the boundary of two forced ASR
/// chunks. Matching ignores case, whitespace, and punctuation while the
/// returned suffix preserves the current transcript's original spelling.
///
/// A match must contain at least two content tokens, or one token of four or
/// more characters. This avoids deleting intentional short repetitions such
/// as "yes, yes" while still handling a split through a distinctive word.
pub fn remove_transcript_overlap(previous: &str, current: &str) -> String {
    let previous_tokens = overlap_tokens(previous);
    let current_tokens = overlap_tokens(current);
    let maximum = previous_tokens.len().min(current_tokens.len()).min(24);
    let matched = (1..=maximum).rev().find(|&count| {
        let left = &previous_tokens[previous_tokens.len() - count..];
        let right = &current_tokens[..count];
        left.iter()
            .zip(right)
            .all(|(left, right)| left.normalized == right.normalized)
            && (count >= 2
                || left
                    .first()
                    .is_some_and(|token| token.normalized.chars().count() >= 4))
    });
    let Some(matched) = matched else {
        return current.trim().to_owned();
    };
    let cutoff = current_tokens[matched - 1].end_byte;
    current[cutoff..]
        .trim_start_matches(|character: char| {
            character.is_whitespace() || (!character.is_alphanumeric() && character != '\'')
        })
        .trim()
        .to_owned()
}

/// Removes filler words and filler punctuation from the two edges for a given source language.
pub fn strip_filler_edges_for_lang(text: &str, source_lang: &str) -> String {
    let fillers = filler_words_for_lang(source_lang);
    let mut value = text.trim().to_owned();
    let mut previous = None;
    while !value.is_empty() && previous.as_deref() != Some(value.as_str()) {
        previous = Some(value.clone());
        let characters: Vec<char> = value.chars().collect();
        let prefix_end = filler_prefix_end_custom(&characters, &fillers);
        let without_prefix = characters[prefix_end..].iter().collect::<String>();
        let characters: Vec<char> = without_prefix.trim().chars().collect();
        let suffix_start = filler_suffix_start_custom(&characters, &fillers);
        value = characters[..suffix_start]
            .iter()
            .collect::<String>()
            .trim()
            .to_owned();
    }
    value
}

/// Removes default filler words and filler punctuation from the two edges.
pub fn strip_filler_edges(text: &str) -> String {
    strip_filler_edges_for_lang(text, "auto")
}

/// Returns whether text becomes empty after [`strip_filler_edges`].
pub fn is_filler_segment(text: &str) -> bool {
    strip_filler_edges(text).is_empty()
}

/// Produces translation text and display text for every emittable segment given a source language.
pub fn translation_segment_pairs_for_text_with_lang(
    text: &str,
    source_lang: &str,
) -> Vec<TranslationSegmentPair> {
    split_translation_segments(text)
        .into_iter()
        .filter_map(|source_text| translation_pair_with_lang(source_text.trim(), source_lang))
        .collect()
}

/// Produces translation segment pairs for a completed ASR chunk given a source language.
pub fn translation_segment_pairs_for_final_text_with_lang(
    text: &str,
    source_lang: &str,
) -> Vec<TranslationSegmentPair> {
    split_translation_segments_internal(text, true)
        .into_iter()
        .filter_map(|source_text| translation_pair_with_lang(&source_text, source_lang))
        .collect()
}

/// Produces cleaned source strings supplied to the translation model for a given source language.
pub fn translation_segments_for_text_with_lang(text: &str, source_lang: &str) -> Vec<String> {
    translation_segment_pairs_for_text_with_lang(text, source_lang)
        .into_iter()
        .map(|pair| pair.translation_text)
        .collect()
}

/// Produces translation text and display text for every emittable segment.
pub fn translation_segment_pairs_for_text(text: &str) -> Vec<TranslationSegmentPair> {
    translation_segment_pairs_for_text_with_lang(text, "auto")
}

/// Produces translation segments for a completed ASR chunk.
pub fn translation_segment_pairs_for_final_text(text: &str) -> Vec<TranslationSegmentPair> {
    translation_segment_pairs_for_final_text_with_lang(text, "auto")
}

/// Produces only the cleaned source strings supplied to the translation model.
pub fn translation_segments_for_text(text: &str) -> Vec<String> {
    translation_segments_for_text_with_lang(text, "auto")
}

fn push_if_emittable(segments: &mut Vec<String>, characters: &[char]) {
    let segment = characters.iter().collect::<String>();
    let trimmed = segment.trim();
    if !trimmed.is_empty()
        && trimmed.chars().last().is_some_and(|character| {
            matches!(
                character,
                '。' | '，' | ',' | '.' | '!' | '！' | '?' | '？' | ';' | '；' | ':'
            )
        })
    {
        segments.push(trimmed.to_owned());
    }
}

fn push_translation_segment(
    segments: &mut Vec<String>,
    characters: &[char],
    emit_unterminated: bool,
) {
    if emit_unterminated {
        let segment = characters.iter().collect::<String>();
        let segment = segment.trim();
        if !segment.is_empty() {
            segments.push(segment.to_owned());
        }
    } else {
        push_if_emittable(segments, characters);
    }
}

fn translation_pair_with_lang(source_text: &str, source_lang: &str) -> Option<TranslationSegmentPair> {
    let source_text = source_text.trim();
    let translation_text = strip_filler_edges_for_lang(source_text, source_lang);
    (content_token_count(&translation_text) > 0).then(|| TranslationSegmentPair {
        translation_text,
        source_text: source_text.to_owned(),
    })
}

fn filler_words_for_lang(lang: &str) -> &'static [char] {
    let norm = lang.trim().to_lowercase();
    let main_lang = norm.split(['-', '_']).next().unwrap_or("auto");
    match main_lang {
        "zh" | "auto" => FILLER_WORDS_DEFAULT,
        _ => &[],
    }
}

#[derive(Debug)]
struct OverlapToken {
    normalized: String,
    end_byte: usize,
}

fn overlap_tokens(text: &str) -> Vec<OverlapToken> {
    let characters = text.char_indices().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let (start_byte, character) = characters[index];
        if is_content_cjk_or_kana(character) || is_hangul(character) {
            tokens.push(OverlapToken {
                normalized: character.to_lowercase().collect(),
                end_byte: start_byte + character.len_utf8(),
            });
            index += 1;
            continue;
        }
        if character.is_alphanumeric() {
            let mut end = index + 1;
            while end < characters.len()
                && !is_content_cjk_or_kana(characters[end].1)
                && !is_hangul(characters[end].1)
                && (characters[end].1.is_alphanumeric() || characters[end].1 == '\'')
            {
                end += 1;
            }
            let end_byte = characters.get(end).map_or(text.len(), |(byte, _)| *byte);
            tokens.push(OverlapToken {
                normalized: text[start_byte..end_byte].to_lowercase(),
                end_byte,
            });
            index = end;
            continue;
        }
        index += 1;
    }
    tokens
}

fn is_hangul(character: char) -> bool {
    matches!(character as u32, 0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF)
}

fn trim_start_chars(characters: &[char]) -> &[char] {
    let first_non_whitespace = characters
        .iter()
        .position(|character| !character.is_whitespace())
        .unwrap_or(characters.len());
    &characters[first_non_whitespace..]
}

fn collapse_matches(
    text: &str,
    mut find_match: impl FnMut(&[char], usize) -> Option<(usize, usize)>,
) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut collapsed = Vec::with_capacity(characters.len());
    let mut index = 0;
    while index < characters.len() {
        if let Some((unit_end, match_end)) = find_match(&characters, index) {
            collapsed.extend_from_slice(&characters[index..unit_end]);
            index = match_end;
        } else {
            collapsed.push(characters[index]);
            index += 1;
        }
    }
    collapsed.into_iter().collect()
}

fn repeated_word_end(characters: &[char], index: usize) -> Option<(usize, usize)> {
    if (index > 0 && is_word_re_character(characters[index - 1]))
        || !characters
            .get(index)
            .is_some_and(|character| is_word_re_character(*character))
    {
        return None;
    }
    let unit_end = word_unit_end(characters, index);
    let after_separator = word_separator_end(characters, unit_end)?;
    let unit = &characters[index..unit_end];
    let second_end = after_separator.checked_add(unit.len())?;
    if second_end > characters.len()
        || !characters[after_separator..second_end]
            .iter()
            .zip(unit)
            .all(|(right, left)| word_case_equal(*left, *right))
        || characters
            .get(second_end)
            .is_some_and(|character| is_word_re_character(*character))
    {
        return None;
    }
    Some((unit_end, second_end))
}

fn repeated_cjk_phrase_end(characters: &[char], index: usize) -> Option<(usize, usize)> {
    if !characters
        .get(index)
        .is_some_and(|character| is_han(*character))
    {
        return None;
    }
    let mut unit_end = index;
    while unit_end < characters.len() && unit_end - index < 8 && is_han(characters[unit_end]) {
        unit_end += 1;
    }
    let after_separator = separator_end(characters, unit_end, true)?;
    let unit_len = unit_end - index;
    let second_end = after_separator.checked_add(unit_len)?;
    (second_end <= characters.len()
        && characters[after_separator..second_end] == characters[index..unit_end])
        .then_some((unit_end, second_end))
}

fn repeated_cjk_short_phrase_end(characters: &[char], index: usize) -> Option<(usize, usize)> {
    if !characters
        .get(index)
        .is_some_and(|character| is_han(*character))
    {
        return None;
    }
    for length in (2..=6).rev() {
        let unit_end = index + length;
        let second_end = unit_end + length;
        if second_end <= characters.len()
            && characters[index..unit_end]
                .iter()
                .all(|character| is_han(*character))
            && characters[unit_end..second_end] == characters[index..unit_end]
        {
            return Some((unit_end, second_end));
        }
    }
    None
}

fn repeated_cjk_stutter_character_end(characters: &[char], index: usize) -> Option<(usize, usize)> {
    let character = *characters.get(index)?;
    if !STUTTER_CHARACTERS.contains(&character)
        || characters.get(index + 1).copied() != Some(character)
    {
        return None;
    }
    let mut end = index + 2;
    while characters.get(end).copied() == Some(character) {
        end += 1;
    }
    Some((index + 1, end))
}

fn word_unit_end(characters: &[char], index: usize) -> usize {
    let mut end = index;
    while characters
        .get(end)
        .is_some_and(|character| is_word_re_character(*character))
    {
        end += 1;
    }
    if characters.get(end) == Some(&'\'')
        && characters
            .get(end + 1)
            .is_some_and(|character| is_word_re_character(*character))
    {
        end += 1;
        while characters
            .get(end)
            .is_some_and(|character| is_word_re_character(*character))
        {
            end += 1;
        }
    }
    end
}

fn separator_end(characters: &[char], start: usize, punctuation_required: bool) -> Option<usize> {
    let mut end = start;
    while characters
        .get(end)
        .is_some_and(|character| character.is_whitespace())
    {
        end += 1;
    }
    let punctuation_start = end;
    while characters
        .get(end)
        .is_some_and(|character| is_stutter_separator_punctuation(*character))
    {
        end += 1;
    }
    if end != punctuation_start {
        while characters
            .get(end)
            .is_some_and(|character| character.is_whitespace())
        {
            end += 1;
        }
        return Some(end);
    }
    if punctuation_required {
        return None;
    }
    (end > start).then_some(end)
}

fn word_separator_end(characters: &[char], start: usize) -> Option<usize> {
    if let Some(end) = separator_end(characters, start, true) {
        return Some(end);
    }

    let mut end = start;
    while characters
        .get(end)
        .is_some_and(|character| character.is_whitespace())
    {
        end += 1;
    }
    (end > start).then_some(end)
}

fn is_stutter_separator_punctuation(character: char) -> bool {
    matches!(
        character,
        ',' | '，' | '、' | '.' | '!' | '！' | '?' | '？' | ';' | '；' | ':' | '：'
    )
}

fn is_word_re_character(character: char) -> bool {
    character.is_alphabetic() && !is_content_cjk_or_kana(character)
}

fn word_case_equal(left: char, right: char) -> bool {
    left.to_lowercase().collect::<String>() == right.to_lowercase().collect::<String>()
}

fn is_han(character: char) -> bool {
    matches!(character as u32, 0x4E00..=0x9FFF)
}

fn filler_prefix_end_custom(characters: &[char], fillers: &[char]) -> usize {
    let mut index = 0;
    let mut found_filler = false;
    loop {
        while characters
            .get(index)
            .is_some_and(|character| FILLER_PUNCTUATION.contains(character))
        {
            index += 1;
        }
        if characters
            .get(index)
            .is_some_and(|character| fillers.contains(character))
        {
            found_filler = true;
            index += 1;
        } else {
            return found_filler.then_some(index).unwrap_or(0);
        }
    }
}

fn filler_suffix_start_custom(characters: &[char], fillers: &[char]) -> usize {
    let mut index = characters.len();
    let mut found_filler = false;
    loop {
        while index > 0 && FILLER_PUNCTUATION.contains(&characters[index - 1]) {
            index -= 1;
        }
        if index > 0 && fillers.contains(&characters[index - 1]) {
            found_filler = true;
            index -= 1;
        } else {
            return found_filler.then_some(index).unwrap_or(characters.len());
        }
    }
}

fn content_token_count(text: &str) -> usize {
    let characters: Vec<char> = text.chars().collect();
    let mut count = 0;
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if is_content_cjk_or_kana(character) {
            count += 1;
            index += 1;
        } else if is_word_re_character(character) {
            count += 1;
            index = word_unit_end(&characters, index);
        } else if character.is_numeric() || character.is_ascii_digit() {
            count += 1;
            while characters
                .get(index)
                .is_some_and(|next| next.is_numeric() || next.is_ascii_digit())
            {
                index += 1;
            }
        } else {
            index += 1;
        }
    }
    count
}

fn is_content_cjk_or_kana(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0x3040..=0x30FF | 0x31F0..=0x31FF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_adjacent_english_and_chinese_stutters_until_stable() {
        assert_eq!(remove_asr_stutters(" yes, YES "), "yes");
        assert_eq!(remove_asr_stutters("oh!oh!oh!"), "oh!");
        assert_eq!(remove_asr_stutters("two two devices"), "two devices");
        assert_eq!(remove_asr_stutters("对，对，对"), "对");
        assert_eq!(remove_asr_stutters("两个两个设备"), "两个设备");
        assert_eq!(remove_asr_stutters("对你好，对你好"), "对你好");
        assert_eq!(remove_asr_stutters("嗯嗯嗯"), "嗯");
        assert_eq!(remove_asr_stutters("yes, no, yes"), "yes, no, yes");
    }

    #[test]
    fn collapses_adjacent_russian_stutters() {
        assert_eq!(remove_asr_stutters(" да, ДА "), "да");
        assert_eq!(remove_asr_stutters("привет, привет"), "привет");
    }

    #[test]
    fn removes_multilingual_overlap_without_deleting_short_repetition() {
        assert_eq!(
            remove_transcript_overlap(
                "We need to cross the central street.",
                "the central street, then turn left."
            ),
            "then turn left."
        );
        assert_eq!(
            remove_transcript_overlap("今天我们去公园", "去公园然后吃饭"),
            "然后吃饭"
        );
        assert_eq!(
            remove_transcript_overlap("yes", "yes, yes we can"),
            "yes, yes we can"
        );
        assert_eq!(
            remove_transcript_overlap("configuration", "configuration is ready"),
            "is ready"
        );
    }

    #[test]
    fn completed_asr_text_keeps_an_unpunctuated_tail_for_translation() {
        let pairs = translation_segment_pairs_for_final_text("First sentence. unfinished tail");
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].source_text, "First sentence.");
        assert_eq!(pairs[1].source_text, "unfinished tail");
        assert_eq!(
            translation_segment_pairs_for_final_text("continuous speech without punctuation")[0]
                .source_text,
            "continuous speech without punctuation"
        );
        let long = format!("{}.", "a".repeat(80));
        let rebuilt = translation_segment_pairs_for_final_text(&long)
            .into_iter()
            .map(|pair| pair.source_text)
            .collect::<String>();
        assert_eq!(rebuilt, long);
    }

    #[test]
    fn russian_text_produces_valid_translation_segment_pairs() {
        let pairs = translation_segment_pairs_for_final_text_with_lang(" сюкаплеет ", "ru");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].source_text, "сюкаплеет");
        assert_eq!(pairs[0].translation_text, "сюкаплеет");

        let multi_pairs = translation_segment_pairs_for_final_text_with_lang("Сюжет. Закончено.", "ru");
        assert_eq!(multi_pairs.len(), 2);
        assert_eq!(multi_pairs[0].source_text, "Сюжет.");
        assert_eq!(multi_pairs[1].source_text, "Закончено.");
    }

    #[test]
    fn strips_only_edge_fillers_and_keeps_punctuation_without_fillers() {
        assert_eq!(strip_filler_edges(" 嗯，啊，今天很好。哦！ "), "今天很好");
        assert_eq!(strip_filler_edges("嗯，啊！"), "");
        assert_eq!(strip_filler_edges("..."), "...");
        assert!(is_filler_segment("嗯，啊！"));
        assert!(!is_filler_segment("..."));
        assert!(!is_filler_segment("啊，实际内容。哦"));
    }

    #[test]
    fn preserves_sentence_pairs_but_excludes_filler_only_segments() {
        let pairs = translation_segment_pairs_for_text("嗯，啊，Hello，world。哦！");
        assert_eq!(
            pairs,
            vec![TranslationSegmentPair {
                translation_text: "Hello，world。".into(),
                source_text: "嗯，啊，Hello，world。".into(),
            }]
        );
        assert_eq!(translation_segments_for_text("嗯！"), Vec::<String>::new());
    }

    #[test]
    fn keeps_short_comma_clauses_together_but_uses_comma_after_soft_limit() {
        assert_eq!(
            split_translation_segments("你好，世界。再见！"),
            vec!["你好，世界。", "再见！"]
        );

        let long_sentence = format!("{}，{}。", "a".repeat(30), "b".repeat(50));
        assert_eq!(
            split_translation_segments(&long_sentence),
            vec![
                format!("{}，", "a".repeat(30)),
                format!("{}。", "b".repeat(50))
            ]
        );
    }

    #[test]
    fn matches_python_terminal_boundary_filtering() {
        assert_eq!(
            split_translation_segments("unterminated tail"),
            Vec::<String>::new()
        );
        assert_eq!(
            split_translation_segments("full-width colon："),
            Vec::<String>::new()
        );
        assert_eq!(
            split_translation_segments("ascii colon:"),
            vec!["ascii colon:"]
        );
    }
}

