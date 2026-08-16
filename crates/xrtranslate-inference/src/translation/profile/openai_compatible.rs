use serde_json::{Value, json};

use crate::InferenceError;

use super::{
    TranslationProfile,
    output::clean_openai_compatible,
    prompt_context::{REFERENCE_CONTEXT_INSTRUCTION, normalized_optional, required_text},
};
use crate::translation::TranslationOptions;

pub(super) static PROFILE: TranslationProfile = TranslationProfile {
    temperature: 0.7,
    build_messages,
    apply_sampling: |_, _| {},
    clean_output: clean_openai_compatible,
};

fn build_messages(
    source_text: &str,
    options: &TranslationOptions,
) -> Result<Value, InferenceError> {
    let text = required_text(source_text, "source_text")?;
    let source_language = required_text(&options.source_language, "source_language")?;
    let target_language = required_text(&options.target_language, "target_language")?;
    let context = normalized_optional(options.prompt_context.as_deref());

    let mut system = if source_language == "auto" {
        format!(
            "You are a real-time speech translator. The input language is one of the following: {target_language}. Translate it into the OTHER language from that list. Output only the translation."
        )
    } else {
        format!(
            "You are a real-time speech translator. If input is already {target_language}, output it unchanged. Otherwise translate it into natural, fluent {target_language}. Output only the translation."
        )
    };
    if let Some(context) = context {
        system.push_str("\n\n");
        system.push_str(REFERENCE_CONTEXT_INSTRUCTION);
        system.push('\n');
        system.push_str(context);
    }
    let user_content = if source_language == "auto" {
        format!("Current input:\n{text}")
    } else {
        format!("Source language: {source_language}\nCurrent input:\n{text}")
    };
    Ok(json!([
        {"role": "system", "content": system},
        {"role": "user", "content": user_content},
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_keep_context_out_of_source_text() {
        let options = TranslationOptions {
            source_language: "English".into(),
            target_language: "Chinese".into(),
            prompt_context: Some("Earlier context".into()),
            context_window_tokens: 2_048,
            max_tokens: 32,
        };
        let messages = build_messages("Good morning", &options).unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert!(
            messages[0]["content"]
                .as_str()
                .unwrap()
                .contains("Earlier context")
        );
        assert_eq!(
            messages[1]["content"],
            "Source language: English\nCurrent input:\nGood morning"
        );
    }
}
