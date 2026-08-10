//! Native inference adapters for the Python-free XRTranslate backend.
//!
//! The crate deliberately speaks the OpenAI-compatible HTTP contract exposed
//! by `llama-server` instead of linking llama.cpp into the desktop process.
//! Its transport is a trait so session code can use a real [`ReqwestClient`]
//! while tests exercise the exact JSON contract without a running model.

#![forbid(unsafe_code)]

mod error;
mod http;
mod openai;
mod qwen3;
mod translation;
mod wav;

pub use error::{InferenceError, TransportError};
pub use http::{AsyncHttpClient, HttpRequest, HttpResponse, ReqwestClient};
pub use openai::{ChatCompletion, OpenAiCompatibleClient};
pub use qwen3::{AsrTranscript, Qwen3AsrAdapter, Qwen3AsrOptions};
pub use translation::{
    TranslationAdapter, TranslationOptions, TranslationProvider, TranslationResult,
    build_hunyuan_prompt, build_translation_messages,
};
pub use wav::{PCM16_MONO_16KHZ_FORMAT, pcm16_mono_16khz_to_wav};
