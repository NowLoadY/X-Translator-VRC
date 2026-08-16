use serde_json::{Value, json};

use crate::InferenceError;

use super::{
    TranslationProfile,
    output::clean_hunyuan,
    prompt_context::{REFERENCE_CONTEXT_INSTRUCTION, normalized_optional, required_text},
};
use crate::translation::TranslationOptions;

// Official Hy-MT2 1.8B/7B generation profile. llama.cpp names the
// Transformers `repetition_penalty` field `repeat_penalty`.
const TEMPERATURE: f64 = 0.7;
const TOP_P: f64 = 0.6;
const TOP_K: u64 = 20;
const REPEAT_PENALTY: f64 = 1.05;

pub(super) static PROFILE: TranslationProfile = TranslationProfile {
    temperature: TEMPERATURE,
    build_messages,
    apply_sampling,
    clean_output: clean_hunyuan,
};

/// Produces the direct Hy-MT2 prompt used by the legacy default route.
pub fn build_hunyuan_prompt(
    text: &str,
    source_language: &str,
    target_language: &str,
    prompt_context: Option<&str>,
) -> Result<String, InferenceError> {
    let text = required_text(text, "source_text")?;
    let source_language = required_text(source_language, "source_language")?;
    let target_language = required_text(target_language, "target_language")?;
    let prompt = if source_language == "auto" {
        format!(
            "Translate the following text into the other language among {target_language}. Output only the translation; do not add explanations."
        )
    } else {
        format!(
            "Translate the following {source_language} text into natural {target_language}. Output only the translation; do not add explanations."
        )
    };
    if let Some(context) = normalized_optional(prompt_context) {
        return Ok(format!(
            "{prompt}\n\n{REFERENCE_CONTEXT_INSTRUCTION}\n\n--- BEGIN REFERENCE CONTEXT ---\n{context}\n--- END REFERENCE CONTEXT ---\n\nCurrent input:\n{text}"
        ));
    }
    Ok(format!("{prompt}\n\n{text}"))
}

fn build_messages(
    source_text: &str,
    options: &TranslationOptions,
) -> Result<Value, InferenceError> {
    let text = required_text(source_text, "source_text")?;
    let source_language = required_text(&options.source_language, "source_language")?;
    let target_language = required_text(&options.target_language, "target_language")?;
    let context = normalized_optional(options.prompt_context.as_deref());
    Ok(json!([{
        "role": "user",
        "content": build_hunyuan_prompt(text, source_language, target_language, context)?,
    }]))
}

fn apply_sampling(payload: &mut Value, options: &TranslationOptions) {
    payload["top_p"] = Value::from(TOP_P);
    payload["top_k"] = Value::from(TOP_K);
    payload["repeat_penalty"] = Value::from(REPEAT_PENALTY);
    // Transformers applies repetition_penalty to the complete token history.
    // Some llama.cpp OpenAI-compatible schemas reject the documented -1
    // shorthand, so send the concrete per-slot context window instead.
    payload["repeat_last_n"] = Value::from(options.context_window_tokens.max(1));
    // llama.cpp otherwise adds a 0.05 min-p filter that is absent from the
    // model author's recommended generation profile.
    payload["min_p"] = Value::from(0.0);
}
