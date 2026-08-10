use serde_json::{Value, json};

use crate::{
    AsyncHttpClient, InferenceError, OpenAiCompatibleClient,
    openai::{non_streaming_chat_payload, remove_completion_markers},
};

/// Prompt style selected for a translation endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationProvider {
    /// Hy-MT2's direct, single-user-message instruction format.
    Hunyuan,
    /// Generic OpenAI-compatible instruction/messages format (including Groq).
    OpenAiCompatible,
}

/// Options that accompany a single source segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationOptions {
    pub source_language: String,
    pub target_language: String,
    /// Previously translated context. Hunyuan and generic adapters include it
    /// as context but always clearly delimit the current source segment.
    pub prompt_context: Option<String>,
    pub max_tokens: u32,
}

impl TranslationOptions {
    pub fn new(source_language: impl Into<String>, target_language: impl Into<String>) -> Self {
        Self {
            source_language: source_language.into(),
            target_language: target_language.into(),
            prompt_context: None,
            max_tokens: 256,
        }
    }
}

/// A translated segment after model-output cleanup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationResult {
    pub text: String,
}

/// Reusable MT adapter for Hy-MT2 GGUF and remote OpenAI-compatible services.
#[derive(Debug, Clone)]
pub struct TranslationAdapter<C> {
    chat: OpenAiCompatibleClient<C>,
    model: String,
    provider: TranslationProvider,
}

impl<C> TranslationAdapter<C> {
    pub fn new(
        http: C,
        endpoint: impl Into<String>,
        model: impl Into<String>,
        provider: TranslationProvider,
    ) -> Result<Self, InferenceError> {
        let model = model.into().trim().to_owned();
        if model.is_empty() {
            return Err(InferenceError::InvalidConfiguration {
                field: "model",
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            chat: OpenAiCompatibleClient::new(http, endpoint)?,
            model,
            provider,
        })
    }

    /// Creates a translation adapter for an OpenAI-compatible endpoint that
    /// requires an `Authorization: Bearer …` header (for example Groq).
    pub fn with_bearer_token(
        http: C,
        endpoint: impl Into<String>,
        model: impl Into<String>,
        provider: TranslationProvider,
        token: impl Into<String>,
    ) -> Result<Self, InferenceError> {
        let model = model.into().trim().to_owned();
        if model.is_empty() {
            return Err(InferenceError::InvalidConfiguration {
                field: "model",
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            chat: OpenAiCompatibleClient::with_bearer_token(http, endpoint, token)?,
            model,
            provider,
        })
    }

    pub fn provider(&self) -> TranslationProvider {
        self.provider
    }
}

impl<C: AsyncHttpClient> TranslationAdapter<C> {
    pub async fn translate(
        &self,
        source_text: &str,
        options: TranslationOptions,
    ) -> Result<TranslationResult, InferenceError> {
        let messages = build_translation_messages(self.provider, source_text, &options)?;
        let mut payload = non_streaming_chat_payload(
            &self.model,
            messages,
            if self.provider == TranslationProvider::Hunyuan {
                0.0
            } else {
                0.7
            },
            options.max_tokens,
        );
        if self.provider == TranslationProvider::Hunyuan {
            // Keep the legacy Hy-MT2 sampling contract.  Changing this during
            // a runtime migration changes model output, not just transport.
            payload["top_p"] = Value::from(0.6);
        }
        let completion = self.chat.chat_completion(payload).await?;
        let text = clean_translation_output(&completion.text, self.provider);
        if text.is_empty() {
            return Err(InferenceError::EmptyOutput {
                operation: "translation",
            });
        }
        Ok(TranslationResult { text })
    }
}

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
        format!("Translate the following text into the other language among {target_language}. Output only the translation; do not add explanations.")
    } else {
        format!("Translate the following {source_language} text into natural {target_language}. Output only the translation; do not add explanations.")
    };
    // The Python Hunyuan provider deliberately did not inject prompt context:
    // the model is trained for a direct one-shot translation instruction.
    // Preserve the argument for the generic adapter API, but do not silently
    // change the default model prompt.
    let _ = prompt_context;
    Ok(format!("{prompt}\n\n{text}"))
}

/// Builds OpenAI chat messages without performing network I/O.
pub fn build_translation_messages(
    provider: TranslationProvider,
    source_text: &str,
    options: &TranslationOptions,
) -> Result<Value, InferenceError> {
    let text = required_text(source_text, "source_text")?;
    let source_language = required_text(&options.source_language, "source_language")?;
    let target_language = required_text(&options.target_language, "target_language")?;
    let context = normalized_optional(options.prompt_context.as_deref());
    match provider {
        TranslationProvider::Hunyuan => Ok(json!([{
            "role": "user",
            "content": build_hunyuan_prompt(text, source_language, target_language, context)?,
        }])),
        TranslationProvider::OpenAiCompatible => {
            let mut system = if source_language == "auto" {
                format!("You are a real-time speech translator. The input language is one of the following: {target_language}. Translate it into the OTHER language from that list. Output only the translation.")
            } else {
                format!("You are a real-time speech translator. If input is already {target_language}, output it unchanged. Otherwise translate it into natural, fluent {target_language}. Output only the translation.")
            };
            if let Some(context) = context {
                system.push_str("\n\nContext (reference only; do not translate it):\n");
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
    }
}

fn required_text<'a>(value: &'a str, field: &'static str) -> Result<&'a str, InferenceError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(InferenceError::InvalidConfiguration {
            field,
            message: "must not be empty".into(),
        });
    }
    Ok(value)
}

fn normalized_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn clean_translation_output(text: &str, provider: TranslationProvider) -> String {
    let text = remove_completion_markers(text);
    let text = text.trim();
    if provider == TranslationProvider::Hunyuan {
        return text.to_owned();
    }
    for label in ["translation:", "translated text:"] {
        if text.len() >= label.len() && text[..label.len()].eq_ignore_ascii_case(label) {
            return text[label.len()..].trim().to_owned();
        }
    }
    text.to_owned()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{HttpRequest, HttpResponse, TransportError};

    #[derive(Default)]
    struct RecordingHttpClient {
        requests: Mutex<Vec<HttpRequest>>,
        responses: Mutex<Vec<Result<HttpResponse, TransportError>>>,
    }

    impl RecordingHttpClient {
        fn respond_with(&self, response: HttpResponse) {
            self.responses.lock().unwrap().push(Ok(response));
        }
    }

    impl AsyncHttpClient for RecordingHttpClient {
        async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
            self.requests.lock().unwrap().push(request);
            self.responses.lock().unwrap().remove(0)
        }
    }

    #[tokio::test]
    async fn hunyuan_request_uses_direct_prompt_and_cleans_response() {
        let http = RecordingHttpClient::default();
        http.respond_with(HttpResponse {
            status: 200,
            body: r#"{"choices":[{"message":{"content":"你好 <|im_end|>"}}]}"#.into(),
        });
        let adapter = TranslationAdapter::new(
            http,
            "http://127.0.0.1:8002/v1/chat/completions",
            "hy-mt2",
            TranslationProvider::Hunyuan,
        )
        .unwrap();
        let result = adapter
            .translate(
                "hello",
                TranslationOptions {
                    source_language: "English".into(),
                    target_language: "Chinese".into(),
                    prompt_context: Some("A previous sentence.".into()),
                    max_tokens: 256,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.text, "你好");

        let http = adapter.chat.into_inner();
        let request = http.requests.lock().unwrap().pop().unwrap();
        assert_eq!(request.url, "http://127.0.0.1:8002/v1/chat/completions");
        assert_eq!(request.body["model"], "hy-mt2");
        assert_eq!(request.body["temperature"], 0.0);
        assert_eq!(request.body["top_p"], 0.6);
        assert_eq!(request.body["messages"].as_array().unwrap().len(), 1);
        let prompt = request.body["messages"][0]["content"].as_str().unwrap();
        assert!(prompt.contains("following English text into natural Chinese"));
        assert!(!prompt.contains("Context (reference only"));
        assert!(prompt.ends_with("\n\nhello"));
    }

    #[tokio::test]
    async fn generic_adapter_sends_bearer_auth_outside_json_payload() {
        let http = RecordingHttpClient::default();
        http.respond_with(HttpResponse {
            status: 200,
            body: r#"{"choices":[{"message":{"content":"bonjour"}}]}"#.into(),
        });
        let adapter = TranslationAdapter::with_bearer_token(
            http,
            "https://example.test/openai/v1/chat/completions",
            "remote-model",
            TranslationProvider::OpenAiCompatible,
            "test-token",
        )
        .unwrap();
        adapter
            .translate("hello", TranslationOptions::new("English", "French"))
            .await
            .unwrap();

        let http = adapter.chat.into_inner();
        let request = http.requests.lock().unwrap().pop().unwrap();
        assert!(
            request
                .headers
                .contains(&("authorization".into(), "Bearer test-token".into()))
        );
        assert_eq!(request.body["model"], "remote-model");
        assert!(request.body.get("authorization").is_none());
    }

    #[test]
    fn generic_messages_keep_context_out_of_source_text() {
        let options = TranslationOptions {
            source_language: "English".into(),
            target_language: "Chinese".into(),
            prompt_context: Some("Earlier context".into()),
            max_tokens: 32,
        };
        let messages = build_translation_messages(
            TranslationProvider::OpenAiCompatible,
            "Good morning",
            &options,
        )
        .unwrap();
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
