use serde::{Deserialize, Serialize};

/// Facts selected for one translation segment. The values are deliberately
/// neutral so callers can obtain them from XR Corpus or another context
/// provider without coupling this composer to that service.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranslationPromptContext {
    pub language_order: Vec<String>,
    pub terminology_rows: Vec<String>,
    pub recent_turns: Vec<PromptTurn>,
    pub previous_revision: Option<PromptTurn>,
    pub surrounding_source: Option<SurroundingSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTurn {
    pub turn_id: Option<String>,
    pub speaker_id: String,
    pub source_language: String,
    pub target_language: String,
    pub source_text: String,
    pub translated_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurroundingSource {
    pub speaker_id: String,
    pub source_language: String,
    pub before: String,
    pub after: String,
}

/// A user-selectable reference-context block. The current input is not a
/// block: provider profiles append it through their non-editable input field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranslationPromptBlock {
    LanguageOrder,
    Terminology,
    RecentTurns { limit: Option<usize> },
    PreviousRevision,
    SurroundingSource,
    CustomText { text: String },
}

/// Ordered prompt composition owned by the shared translation layer.
///
/// The default is intentionally equivalent to the built-in context policy.
/// A future settings editor can persist this value without changing the
/// provider profiles or the XR Corpus protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationPromptTemplate {
    pub blocks: Vec<TranslationPromptBlock>,
}

impl Default for TranslationPromptTemplate {
    fn default() -> Self {
        Self {
            blocks: vec![
                TranslationPromptBlock::LanguageOrder,
                TranslationPromptBlock::Terminology,
                TranslationPromptBlock::RecentTurns { limit: None },
                TranslationPromptBlock::PreviousRevision,
                TranslationPromptBlock::SurroundingSource,
            ],
        }
    }
}

impl TranslationPromptTemplate {
    pub fn compose(&self, context: &TranslationPromptContext) -> Option<String> {
        let mut sections = Vec::new();
        for block in &self.blocks {
            if let Some(section) = render_block(block, context) {
                sections.push(section);
            }
        }
        (!sections.is_empty())
            .then(|| format!("# Translation Context\n\n{}", sections.join("\n\n")))
    }
}

fn render_block(
    block: &TranslationPromptBlock,
    context: &TranslationPromptContext,
) -> Option<String> {
    match block {
        TranslationPromptBlock::LanguageOrder => {
            let languages = context
                .language_order
                .iter()
                .map(|language| language.trim())
                .filter(|language| !language.is_empty())
                .collect::<Vec<_>>();
            (!languages.is_empty()).then(|| format!("## Language Order\n\n{}", languages.join(",")))
        }
        TranslationPromptBlock::Terminology => {
            let rows = context
                .terminology_rows
                .iter()
                .map(|row| row.trim())
                .filter(|row| !row.is_empty())
                .collect::<Vec<_>>();
            (!rows.is_empty()).then(|| format!("## Terminology\n\n{}", rows.join("\n")))
        }
        TranslationPromptBlock::RecentTurns { limit } => {
            let start = limit
                .map(|limit| context.recent_turns.len().saturating_sub(limit))
                .unwrap_or_default();
            let turns = context
                .recent_turns
                .iter()
                .skip(start)
                .map(render_turn)
                .collect::<Vec<_>>();
            (!turns.is_empty())
                .then(|| format!("## Recent Bilingual History\n\n{}", turns.join("\n\n")))
        }
        TranslationPromptBlock::PreviousRevision => {
            context.previous_revision.as_ref().map(|turn| {
                format!(
                    "## Previous Revision of Current Speech\n\n{}",
                    render_turn(turn)
                )
            })
        }
        TranslationPromptBlock::SurroundingSource => {
            let source = context.surrounding_source.as_ref()?;
            let mut lines = Vec::new();
            append_source_line(&mut lines, source, "Before current input", &source.before);
            append_source_line(&mut lines, source, "After current input", &source.after);
            (!lines.is_empty()).then(|| {
                format!(
                    "## Current Utterance Context (context only; do not translate)\n\n{}",
                    lines.join("\n")
                )
            })
        }
        TranslationPromptBlock::CustomText { text } => {
            let text = text.trim();
            (!text.is_empty()).then(|| format!("## Custom Reference Text\n\n{text}"))
        }
    }
}

fn render_turn(turn: &PromptTurn) -> String {
    let speaker = if turn.speaker_id.trim().is_empty() {
        String::new()
    } else {
        format!("{} ", turn.speaker_id.trim())
    };
    format!(
        "{speaker}{}: {}\n{speaker}{}: {}",
        turn.source_language.trim(),
        turn.source_text.trim(),
        turn.target_language.trim(),
        turn.translated_text.trim()
    )
}

fn append_source_line(
    lines: &mut Vec<String>,
    source: &SurroundingSource,
    label: &str,
    text: &str,
) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let speaker = if source.speaker_id.trim().is_empty() {
        String::new()
    } else {
        format!("{} ", source.speaker_id.trim())
    };
    lines.push(format!(
        "{label}: {speaker}{} / {text}",
        source.source_language.trim()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> TranslationPromptContext {
        TranslationPromptContext {
            language_order: vec!["en".into(), "zh".into()],
            terminology_rows: vec!["天使,Mercy".into()],
            recent_turns: vec![PromptTurn {
                turn_id: Some("previous".into()),
                speaker_id: "speaker-01".into(),
                source_language: "en".into(),
                target_language: "zh".into(),
                source_text: "We changed the plan.".into(),
                translated_text: "我们改计划了。".into(),
            }],
            previous_revision: Some(PromptTurn {
                turn_id: Some("current".into()),
                speaker_id: "speaker-01".into(),
                source_language: "en".into(),
                target_language: "zh".into(),
                source_text: "The current window.".into(),
                translated_text: "当前窗口。".into(),
            }),
            surrounding_source: Some(SurroundingSource {
                speaker_id: "speaker-01".into(),
                source_language: "en".into(),
                before: "Before it.".into(),
                after: "After it.".into(),
            }),
        }
    }

    #[test]
    fn default_template_composes_all_selected_context_blocks() {
        let prompt = TranslationPromptTemplate::default()
            .compose(&context())
            .unwrap();
        assert!(prompt.contains("## Language Order\n\nen,zh"));
        assert!(prompt.contains("## Terminology\n\n天使,Mercy"));
        assert!(prompt.contains("## Recent Bilingual History"));
        assert!(prompt.contains("## Previous Revision of Current Speech"));
        assert!(prompt.contains("Before current input: speaker-01 en / Before it."));
        assert!(prompt.contains("After current input: speaker-01 en / After it."));
    }

    #[test]
    fn template_can_select_recent_turn_count_and_custom_text() {
        let template = TranslationPromptTemplate {
            blocks: vec![
                TranslationPromptBlock::RecentTurns { limit: Some(1) },
                TranslationPromptBlock::CustomText {
                    text: "Keep the tone casual.".into(),
                },
            ],
        };
        let prompt = template.compose(&context()).unwrap();
        assert!(prompt.contains("We changed the plan."));
        assert!(prompt.contains("Keep the tone casual."));
        assert!(!prompt.contains("## Language Order"));
        assert!(!prompt.contains("## Previous Revision"));
    }

    #[test]
    fn empty_template_does_not_create_an_empty_reference_context() {
        let template = TranslationPromptTemplate { blocks: Vec::new() };
        assert_eq!(template.compose(&TranslationPromptContext::default()), None);
    }
}
