use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;

use crate::{
    AsyncHttpClient, InferenceError, OpenAiCompatibleClient,
    openai::{non_streaming_chat_payload, remove_completion_markers},
    pcm16_mono_16khz_to_wav,
};

/// Options for one Qwen3-ASR completion request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qwen3AsrOptions {
    /// Language name understood by Qwen3-ASR (for example `English` or
    /// `Chinese`). An empty value asks the model to infer the language.
    pub language: Option<String>,
    /// Optional prompt context from the current conversation.
    pub prompt_context: Option<String>,
    /// Maximum generated transcript tokens.
    pub max_tokens: u32,
}

impl Default for Qwen3AsrOptions {
    fn default() -> Self {
        Self {
            language: None,
            prompt_context: None,
            max_tokens: 128,
        }
    }
}

/// A completed Qwen3-ASR transcription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrTranscript {
    pub text: String,
}

/// Qwen3-ASR GGUF adapter backed by llama.cpp's OpenAI-compatible endpoint.
#[derive(Debug, Clone)]
pub struct Qwen3AsrAdapter<C> {
    chat: OpenAiCompatibleClient<C>,
    model: String,
}

impl<C> Qwen3AsrAdapter<C> {
    pub fn new(
        http: C,
        endpoint: impl Into<String>,
        model: impl Into<String>,
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
        })
    }

    pub fn endpoint(&self) -> &str {
        self.chat.endpoint()
    }
}

impl<C: AsyncHttpClient> Qwen3AsrAdapter<C> {
    /// Sends a complete VAD-delimited PCM16/16kHz turn to Qwen3-ASR.
    ///
    /// The raw PCM is always converted to WAV before base64 encoding. The
    /// resulting `input_audio` content part follows the OpenAI multimodal
    /// chat-completions shape accepted by current llama-server builds.
    pub async fn transcribe_pcm16(
        &self,
        pcm: &[u8],
        options: Qwen3AsrOptions,
    ) -> Result<AsrTranscript, InferenceError> {
        let wav = pcm16_mono_16khz_to_wav(pcm)?;
        let encoded_wav = STANDARD.encode(wav);

        let mut messages = Vec::new();
        if let Some(context) = normalized_optional(&options.prompt_context) {
            messages.push(json!({"role": "system", "content": context}));
        }

        let mut content = vec![json!({
            "type": "input_audio",
            "input_audio": {"data": encoded_wav, "format": "wav"}
        })];
        // The legacy Qwen3 llama.cpp client supplied an explicit user
        // instruction only when the route pinned the input language.  In an
        // automatic route its pair-aware system prompt is the instruction;
        // retaining that distinction avoids biasing language detection.
        if let Some(language) = normalized_optional(&options.language) {
            content.push(json!({
                "type": "text",
                "text": format!(
                    "Transcribe this {language} speech. Return only the spoken transcript; do not translate or explain it."
                )
            }));
        }
        messages.push(json!({
            "role": "user",
            "content": content
        }));

        let payload = non_streaming_chat_payload(
            &self.model,
            serde_json::Value::Array(messages),
            0.0,
            options.max_tokens,
        );
        let completion = self.chat.chat_completion(payload).await?;
        let text = clean_asr_text(&completion.text);
        Ok(AsrTranscript { text })
    }
}

fn normalized_optional(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn clean_asr_text(text: &str) -> String {
    let text = text
        .rsplit_once("<asr_text>")
        .map_or(text, |(_, transcript)| transcript);
    remove_completion_markers(text)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use base64::{Engine as _, engine::general_purpose::STANDARD};

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
    async fn qwen3_request_wraps_pcm_in_wav_and_uses_input_audio() {
        let http = RecordingHttpClient::default();
        http.respond_with(HttpResponse {
            status: 200,
            body: r#"{"choices":[{"message":{"content":"language English<asr_text>Hello <|im_end|>"}}]}"#.into(),
        });
        let adapter = Qwen3AsrAdapter::new(
            http,
            "http://127.0.0.1:8001/v1/chat/completions",
            "qwen3-asr",
        )
        .unwrap();

        let result = adapter
            .transcribe_pcm16(
                &[1, 0, 2, 0],
                Qwen3AsrOptions {
                    language: Some("English".into()),
                    prompt_context: Some("Names: Codex".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(result.text, "Hello");
        let http = adapter.chat.into_inner();
        let request = http.requests.lock().unwrap().pop().unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "http://127.0.0.1:8001/v1/chat/completions");
        assert_eq!(request.body["model"], "qwen3-asr");
        assert_eq!(request.body["temperature"], 0.0);
        assert_eq!(request.body["stream"], false);
        assert_eq!(request.body["messages"][0]["role"], "system");
        assert_eq!(
            request.body["messages"][1]["content"][0]["type"],
            "input_audio"
        );
        assert_eq!(
            request.body["messages"][1]["content"][0]["input_audio"]["format"],
            "wav"
        );
        let encoded = request.body["messages"][1]["content"][0]["input_audio"]["data"]
            .as_str()
            .unwrap();
        let wav = STANDARD.decode(encoded).unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[44..], &[1, 0, 2, 0]);
        assert!(
            request.body["messages"][1]["content"][1]["text"]
                .as_str()
                .unwrap()
                .contains("English")
        );
    }

    #[tokio::test]
    async fn qwen3_http_failure_is_structured() {
        let http = RecordingHttpClient::default();
        http.respond_with(HttpResponse {
            status: 503,
            body: "temporarily unavailable".into(),
        });
        let adapter = Qwen3AsrAdapter::new(
            http,
            "http://127.0.0.1:8001/v1/chat/completions",
            "qwen3-asr",
        )
        .unwrap();
        let error = adapter
            .transcribe_pcm16(&[0, 0], Qwen3AsrOptions::default())
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            InferenceError::HttpStatus { status: 503, .. }
        ));
        assert!(error.to_string().contains("temporarily unavailable"));
    }

    #[test]
    fn qwen3_marker_only_output_becomes_an_empty_transcript() {
        // A silent audio smoke test on llama.cpp returns this prefix without
        // transcript content. It must not become an OSC subtitle.
        assert_eq!(clean_asr_text("language None<asr_text>"), "");
    }
}
