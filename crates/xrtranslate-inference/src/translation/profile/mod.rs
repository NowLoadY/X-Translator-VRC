mod hunyuan;
mod openai_compatible;
mod output;

use serde_json::{Value, json};
use xrtranslate_prompt::{PromptExecutionTrace, PromptMessageRole, PromptProviderTarget};

use crate::InferenceError;

use super::{TranslationOptions, TranslationProvider};

pub use output::is_probable_translation_context_leak;

pub(super) struct TranslationProfile {
    target: PromptProviderTarget,
    temperature: f64,
    apply_sampling: fn(&mut Value, &TranslationOptions),
    clean_output: fn(&str) -> String,
}

impl TranslationProfile {
    pub(super) fn temperature(&self) -> f64 {
        self.temperature
    }

    pub(super) fn build_prompt(
        &self,
        source_text: &str,
        options: &TranslationOptions,
    ) -> Result<RenderedTranslationPrompt, InferenceError> {
        render_prompt(self.target, source_text, options)
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

pub fn build_translation_messages(
    provider: TranslationProvider,
    source_text: &str,
    options: &TranslationOptions,
) -> Result<Value, InferenceError> {
    registered(provider)
        .build_prompt(source_text, options)
        .map(|prompt| prompt.messages)
}

pub(super) struct RenderedTranslationPrompt {
    pub(super) messages: Value,
    pub(super) trace: PromptExecutionTrace,
}

fn render_prompt(
    target: PromptProviderTarget,
    source_text: &str,
    options: &TranslationOptions,
) -> Result<RenderedTranslationPrompt, InferenceError> {
    let execution = options
        .prompt_graph
        .render_with_trace(
            target,
            source_text,
            &options.source_language,
            &options.target_language,
            &options.prompt_context,
        )
        .map_err(|error| InferenceError::InvalidConfiguration {
            field: "prompt_graph",
            message: error.to_string(),
        })?;
    Ok(RenderedTranslationPrompt {
        messages: Value::Array(
            execution
                .render
                .messages
                .into_iter()
                .map(|message| {
                    json!({
                        "role": match message.role {
                            PromptMessageRole::System => "system",
                            PromptMessageRole::User => "user",
                        },
                        "content": message.content,
                    })
                })
                .collect(),
        ),
        trace: execution.trace,
    })
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
                field: "prompt_graph",
                ..
            }
        ));
    }

    #[test]
    fn provider_profiles_only_select_graph_request_messages() {
        let options = TranslationOptions::new("English", "Chinese");
        let openai = build_translation_messages(
            TranslationProvider::OpenAiCompatible,
            "Good morning",
            &options,
        )
        .unwrap();
        let hunyuan =
            build_translation_messages(TranslationProvider::Hunyuan, "Good morning", &options)
                .unwrap();
        assert_eq!(openai.as_array().unwrap().len(), 2);
        assert_eq!(hunyuan.as_array().unwrap().len(), 1);
    }
}
