use crate::openai::remove_completion_markers;

pub(super) fn clean_hunyuan(text: &str) -> String {
    clean_shared(text)
}

pub(super) fn clean_openai_compatible(text: &str) -> String {
    let text = clean_shared(text);
    for label in ["translation:", "translated text:"] {
        if text.len() >= label.len() && text[..label.len()].eq_ignore_ascii_case(label) {
            return text[label.len()..].trim().to_owned();
        }
    }
    text
}

fn clean_shared(text: &str) -> String {
    let text = remove_completion_markers(text);
    strip_current_input_artifacts(text.trim())
}

fn strip_current_input_artifacts(text: &str) -> String {
    let mut output = Vec::new();
    for line in text.lines() {
        let normalized = line
            .trim()
            .trim_matches(|character: char| character == '-' || character.is_whitespace())
            .to_ascii_lowercase();
        if normalized == "end current input" {
            break;
        }
        if normalized == "begin current input" || normalized == "current input:" {
            continue;
        }
        output.push(line);
    }
    output.join("\n").trim().to_owned()
}

#[must_use]
pub fn is_probable_translation_context_leak(
    source_text: &str,
    translated_text: &str,
    prompt_context: Option<&str>,
) -> bool {
    let output = translated_text.trim();
    if output.is_empty() {
        return false;
    }

    let folded = output.to_ascii_lowercase();
    const CONTEXT_MARKERS: [&str; 8] = [
        "# translation context",
        "## language order",
        "## terminology",
        "## recent bilingual history",
        "begin reference context",
        "end reference context",
        "begin current input",
        "end current input",
    ];
    if CONTEXT_MARKERS.iter().any(|marker| folded.contains(marker)) {
        return true;
    }

    let glossary_rows = output
        .lines()
        .filter(|line| {
            let cells = line
                .split(',')
                .map(str::trim)
                .filter(|cell| !cell.is_empty())
                .count();
            cells >= 2 && line.chars().count() <= 240
        })
        .take(3)
        .count();
    if glossary_rows >= 3 {
        return true;
    }

    if let Some(context) = prompt_context
        .map(str::trim)
        .filter(|context| !context.is_empty())
    {
        let copied_lines = output
            .lines()
            .map(str::trim)
            .filter(|line| {
                line.chars().count() >= 4 && context.lines().any(|row| row.trim() == *line)
            })
            .take(3)
            .count();
        if copied_lines >= 3 {
            return true;
        }

        let source_chars = source_text.trim().chars().count().max(1);
        let output_chars = output.chars().count();
        if output_chars > (source_chars.saturating_mul(10) + 64).max(192) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_cleanup_only_strips_generic_translation_labels() {
        assert_eq!(
            clean_openai_compatible("Translation: bonjour <|im_end|>"),
            "bonjour"
        );
        assert_eq!(
            clean_hunyuan("Translation: bonjour <|im_end|>"),
            "Translation: bonjour"
        );
    }

    #[test]
    fn detects_copied_glossary_instead_of_translation() {
        let leaked = "Baptiste,Baptiste\nBastion,Bastion\nBrigitte,Brigitte\nMercy,Mercy";
        assert!(is_probable_translation_context_leak(
            "卢西奥。",
            leaked,
            Some("## Terminology\nBaptiste,Baptiste\nBastion,Bastion")
        ));
        assert!(is_probable_translation_context_leak(
            "hello",
            "## Terminology\nhello,你好",
            None
        ));
        assert!(is_probable_translation_context_leak(
            "你玩莱因哈特吗？",
            "Do you play Reinhardt?\nEND CURRENT INPUT",
            None
        ));
    }

    #[test]
    fn accepts_concise_translation_with_a_terminology_match() {
        assert!(!is_probable_translation_context_leak(
            "I love Mercy.",
            "我喜欢天使。",
            Some("## Terminology\n天使,Mercy")
        ));
    }
}
