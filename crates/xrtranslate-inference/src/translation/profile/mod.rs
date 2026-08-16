mod hunyuan;
mod openai_compatible;
mod output;
mod prompt_context;

use serde_json::Value;

use crate::InferenceError;

use super::{TranslationOptions, TranslationProvider};

pub use hunyuan::build_hunyuan_prompt;
pub use output::is_probable_translation_context_leak;

pub(super) struct TranslationProfile {
    temperature: f64,
    build_messages: fn(&str, &TranslationOptions) -> Result<Value, InferenceError>,
    apply_sampling: fn(&mut Value, &TranslationOptions),
    clean_output: fn(&str) -> String,
}

impl TranslationProfile {
    pub(super) fn temperature(&self) -> f64 {
        self.temperature
    }

    pub(super) fn build_messages(
        &self,
        source_text: &str,
        options: &TranslationOptions,
    ) -> Result<Value, InferenceError> {
        (self.build_messages)(source_text, options)
    }

    pub(super) fn apply_sampling(&self, payload: &mut Value, options: &TranslationOptions) {
        (self.apply_sampling)(payload, options);
    }

    pub(super) fn clean_output(&self, text: &str) -> String {
        (self.clean_output)(text)
    }
}

pub(super) fn registered(provider: TranslationProvider) -> &'static TranslationProfile {
    match provider {
        TranslationProvider::Hunyuan => &hunyuan::PROFILE,
        TranslationProvider::OpenAiCompatible => &openai_compatible::PROFILE,
    }
}

/// Builds OpenAI chat messages without performing network I/O.
pub fn build_translation_messages(
    provider: TranslationProvider,
    source_text: &str,
    options: &TranslationOptions,
) -> Result<Value, InferenceError> {
    registered(provider).build_messages(source_text, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_translation_input() {
        let options = TranslationOptions::new("English", "Chinese");
        let error =
            build_translation_messages(TranslationProvider::Hunyuan, "  ", &options).unwrap_err();
        assert!(matches!(
            error,
            InferenceError::InvalidConfiguration {
                field: "source_text",
                ..
            }
        ));
    }
}
