//! Default native audio pipeline: Silero endpointing, Qwen3-ASR, and Hy-MT2.
//!
//! Every `NativePipeline` is owned by a single WebSocket session.  In
//! particular, the Silero ONNX recurrent state must never be shared between
//! microphone streams.  Model servers are shared out-of-process through their
//! local llama.cpp HTTP endpoints.

use std::{
    mem,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use xrtranslate_assets::{ModelAssetsConfig, ResolvedModelAssets};
use xrtranslate_config::AppConfig;
use xrtranslate_engine::{
    TranslationSegmentPair, remove_asr_stutters, remove_transcript_overlap,
    translation_segment_pairs_for_final_text_with_lang,
};
use xrtranslate_inference::{
    Qwen3AsrAdapter, Qwen3AsrOptions, ReqwestClient, TranslationAdapter, TranslationOptions,
    TranslationProvider,
};
use xrtranslate_speaker::{OnlineSpeakerDiarizer, TrackerConfig};
use xrtranslate_vad::{
    EndpointConfig, EndpointDetector, EndpointEvent, FRAME_BYTES, FRAME_SAMPLES, SAMPLE_RATE_HZ,
    SileroVad, Utterance,
};

const SILERO_VAD_MODEL: &str = "models/silero-vad/src/silero_vad/data/silero_vad.onnx";
/// Largest binary WebSocket message accepted from a microphone client.
///
/// At 16 kHz mono PCM16 this is eight seconds.  Longer audio must arrive in
/// multiple WebSocket messages so a client cannot allocate an unbounded VAD,
/// WAV, and base64 working set in one request.
pub(crate) const MAX_INPUT_PCM_BYTES: usize = 256 * 1024;

/// The normalized ASR result and every emittable translation segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecognizedOutput {
    pub(crate) source_text: String,
    pub(crate) segments: Vec<TranslationSegmentPair>,
    pub(crate) asr_elapsed: Duration,
}

/// VAD output paired with its absolute position inside the current audio epoch.
#[derive(Debug)]
pub(crate) struct TimedUtterance {
    pub(crate) utterance: Utterance,
    pub(crate) source_start_ms: f64,
    pub(crate) source_end_ms: f64,
}

impl RecognizedOutput {
    /// Removes text produced from the duplicated audio at a hard VAD boundary.
    /// Returns false when the new result contained only duplicated context.
    pub(crate) fn remove_overlap_with(&mut self, previous: &str, source_language: &str) -> bool {
        self.source_text = remove_transcript_overlap(previous, &self.source_text);
        self.segments = translation_segment_pairs_for_final_text_with_lang(&self.source_text, source_language);
        !self.source_text.is_empty()
    }
}

/// A single Hunyuan translation emitted after a recognized source segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranslationOutput {
    pub(crate) source_text: String,
    pub(crate) translated_text: String,
    pub(crate) mt_elapsed: Duration,
}

/// Per-session state for the fully native default GGUF route.
pub(crate) struct NativePipeline {
    vad: SileroVad,
    endpoint: EndpointDetector,
    pending_pcm: Vec<u8>,
    processed_samples: u64,
    inference: NativeInference,
}

/// Cloneable, stateless network side of the native pipeline.
///
/// A session actor keeps [`NativePipeline`] on its audio/VAD owner task while
/// moving this value to a bounded inference worker.  That prevents slow local
/// model HTTP calls from blocking WebSocket control and microphone intake.
#[derive(Clone)]
pub(crate) struct NativeInference {
    asr: Qwen3AsrAdapter<ReqwestClient>,
    translation: TranslationAdapter<ReqwestClient>,
    speaker: Option<SpeakerInferenceConfig>,
}

#[derive(Clone)]
struct SpeakerInferenceConfig {
    model_path: PathBuf,
    tracker: TrackerConfig,
    min_utterance_ms: u32,
    intra_threads: usize,
}

impl NativePipeline {
    /// Validates the selected native GGUF route and opens a stateful Silero model.
    ///
    /// The model servers themselves are deliberately not contacted here.  The
    /// first request reports a precise endpoint error while permitting the
    /// backend launcher to bring llama.cpp up concurrently with the client.
    pub(crate) fn new(config: &AppConfig, project_root: &Path) -> Result<Self, String> {
        let default_route = config.default_gguf().map_err(|error| error.to_string())?;
        resolved_model_assets(config, project_root)
            .check()
            .into_result()
            .map_err(|error| error.to_string())?;

        let vad_path = project_root.join(SILERO_VAD_MODEL);
        let vad = SileroVad::from_file(&vad_path)
            .map_err(|error| format!("cannot load Silero VAD {}: {error}", vad_path.display()))?;
        let threshold = config.asr.vad_threshold as f32;
        let endpoint = EndpointDetector::new(EndpointConfig {
            speech_threshold: threshold,
            silence_frames_to_finalize: frames_for_ms(config.asr.vad_silence_ms),
            adaptive_silence_after_frames: frames_for_ms(config.asr.vad_adaptive_after_ms),
            adaptive_silence_frames_to_finalize: frames_for_ms(config.asr.vad_adaptive_silence_ms),
            // Retain the legacy ~320 ms capture pre-roll.
            pre_roll_frames: 10,
            max_active_frames: frames_for_ms(config.asr.vad_max_utterance_ms),
            max_active_overlap_frames: frames_for_ms(config.asr.vad_overlap_ms),
        })
        .map_err(|error| error.to_string())?;
        let http =
            ReqwestClient::with_default_direct_timeout().map_err(|error| error.to_string())?;
        let asr = Qwen3AsrAdapter::new(http.clone(), default_route.asr_url, "qwen3-asr")
            .map_err(|error| error.to_string())?;
        let translation = TranslationAdapter::new(
            http,
            default_route.translation_url,
            "hy-mt2",
            TranslationProvider::Hunyuan,
        )
        .map_err(|error| error.to_string())?;
        let speaker = if config.speaker.enabled {
            let model_path = if config.speaker.model_path.is_absolute() {
                config.speaker.model_path.clone()
            } else {
                project_root.join(&config.speaker.model_path)
            };
            if !model_path.is_file() {
                return Err(format!(
                    "speaker recognition is enabled but the ERes2NetV2 ONNX model is missing: {}",
                    model_path.display()
                ));
            }
            let tracker = TrackerConfig {
                similarity_threshold: config.speaker.similarity_threshold as f32,
                same_speaker_hysteresis: config.speaker.same_speaker_hysteresis as f32,
                max_speakers: config.speaker.max_speakers,
            }
            .validate()
            .map_err(|error| error.to_string())?;
            if config.speaker.min_utterance_ms == 0 {
                return Err("speaker.min_utterance_ms must be greater than zero".into());
            }
            if !(1..=64).contains(&config.speaker.intra_threads) {
                return Err("speaker.intra_threads must be within 1..=64".into());
            }
            Some(SpeakerInferenceConfig {
                model_path,
                tracker,
                min_utterance_ms: config.speaker.min_utterance_ms,
                intra_threads: config.speaker.intra_threads,
            })
        } else {
            None
        };

        Ok(Self {
            vad,
            endpoint,
            pending_pcm: Vec::new(),
            processed_samples: 0,
            inference: NativeInference {
                asr,
                translation,
                speaker,
            },
        })
    }

    /// Accepts arbitrary-sized mono PCM16LE transport chunks at 16 kHz.
    ///
    /// The network protocol does not prescribe a binary frame size.  This
    /// method therefore retains incomplete samples until a complete 512-sample
    /// Silero frame can be evaluated, rather than rejecting valid clients.
    pub(crate) fn push_pcm(&mut self, pcm: &[u8]) -> Result<Vec<TimedUtterance>, String> {
        validate_input_chunk_size(pcm.len())?;
        if !pcm.len().is_multiple_of(2) {
            return Err("PCM16LE audio must contain whole 16-bit samples".into());
        }
        self.pending_pcm.extend_from_slice(pcm);

        // Move all completed frames out once.  `split_off` once per 1 KiB
        // frame copied the entire remaining tail and turned a large valid
        // transport message into quadratic work.
        let complete_bytes = self.pending_pcm.len() / FRAME_BYTES * FRAME_BYTES;
        let completed = self.pending_pcm.drain(..complete_bytes).collect::<Vec<_>>();
        let mut utterances = Vec::new();
        for frame in completed.chunks_exact(FRAME_BYTES) {
            let probability = self
                .vad
                .infer_pcm16le(frame)
                .map_err(|error| error.to_string())?;
            let event = self
                .endpoint
                .push_pcm16le(frame, probability)
                .map_err(|error| error.to_string())?;
            self.processed_samples = self.processed_samples.saturating_add(FRAME_SAMPLES as u64);
            if let EndpointEvent::Finalized(utterance) = event {
                utterances.push(self.with_timeline(utterance, 0));
            }
        }
        Ok(utterances)
    }

    /// Completes the current utterance when a client ends a logical turn.
    pub(crate) fn flush(&mut self) -> Result<Option<TimedUtterance>, String> {
        let mut trailing_padding_samples = 0;
        let mut finalized = None;
        if !self.pending_pcm.is_empty() {
            let mut frame = mem::take(&mut self.pending_pcm);
            let received_samples = frame.len() / 2;
            trailing_padding_samples = FRAME_SAMPLES.saturating_sub(received_samples);
            frame.resize(FRAME_BYTES, 0);
            let probability = self
                .vad
                .infer_pcm16le(&frame)
                .map_err(|error| error.to_string())?;
            let event = self
                .endpoint
                .push_pcm16le(&frame, probability)
                .map_err(|error| error.to_string())?;
            if let EndpointEvent::Finalized(utterance) = event {
                finalized = Some(utterance);
            }
            self.processed_samples = self
                .processed_samples
                .saturating_add(received_samples as u64);
        }
        Ok(finalized
            .or_else(|| self.endpoint.flush())
            .map(|utterance| self.with_timeline(utterance, trailing_padding_samples)))
    }

    /// Drops buffered audio and recurrent VAD state after a route change.
    pub(crate) fn reset(&mut self) {
        self.vad.reset();
        self.endpoint.reset();
        self.pending_pcm.clear();
        self.processed_samples = 0;
    }

    /// Makes a request-capable handle for the session's bounded worker.
    pub(crate) fn inference(&self) -> NativeInference {
        self.inference.clone()
    }

    fn with_timeline(
        &self,
        utterance: Utterance,
        trailing_padding_samples: usize,
    ) -> TimedUtterance {
        let real_samples = utterance
            .samples
            .len()
            .saturating_sub(trailing_padding_samples) as u64;
        let end_samples = self.processed_samples;
        let start_samples = end_samples.saturating_sub(real_samples);
        TimedUtterance {
            utterance,
            source_start_ms: samples_to_ms(start_samples),
            source_end_ms: samples_to_ms(end_samples),
        }
    }
}

impl NativeInference {
    /// Opens one stateless embedding model and generation-local tracker on the
    /// bounded inference worker. Disabled configurations allocate nothing.
    pub(crate) fn speaker_diarizer(&self) -> Result<Option<OnlineSpeakerDiarizer>, String> {
        self.speaker
            .as_ref()
            .map(|config| {
                OnlineSpeakerDiarizer::from_file(
                    &config.model_path,
                    config.intra_threads,
                    config.tracker,
                )
                .map_err(|error| {
                    format!(
                        "cannot load speaker embedding model {}: {error}",
                        config.model_path.display()
                    )
                })
            })
            .transpose()
    }

    pub(crate) fn speaker_min_utterance_ms(&self) -> Option<u32> {
        self.speaker.as_ref().map(|config| config.min_utterance_ms)
    }

    /// Runs ASR for a complete utterance and applies the Python-compatible
    /// stutter removal and sentence/filler segmentation before any MT call.
    pub(crate) async fn transcribe(
        &self,
        utterance: Utterance,
        source_language: &str,
        target_language: &str,
    ) -> Result<Option<RecognizedOutput>, String> {
        let pcm = utterance_to_pcm16le(utterance);
        let asr_started = Instant::now();
        let transcript = self
            .asr
            .transcribe_pcm16(
                &pcm,
                Qwen3AsrOptions {
                    language: asr_language(source_language),
                    prompt_context: Some(asr_instruction(source_language, target_language)),
                    max_tokens: 128,
                },
            )
            .await
            .map_err(|error| format!("ASR request failed: {error}"))?;
        let asr_elapsed = asr_started.elapsed();
        let source_text = remove_asr_stutters(&transcript.text);
        if source_text.is_empty() {
            return Ok(None);
        }
        Ok(Some(RecognizedOutput {
            segments: translation_segment_pairs_for_final_text_with_lang(&source_text, source_language),
            source_text,
            asr_elapsed,
        }))
    }

    /// Translates one already-normalized, user-visible source segment.
    pub(crate) async fn translate_segment(
        &self,
        segment: &TranslationSegmentPair,
        source_language: &str,
        target_language: &str,
    ) -> Result<TranslationOutput, String> {
        let route = translation_route(&segment.translation_text, source_language, target_language);
        let mt_started = Instant::now();
        let translated = self
            .translation
            .translate(
                &segment.translation_text,
                TranslationOptions::new(route.source, route.target),
            )
            .await
            .map_err(|error| format!("translation request failed: {error}"))?;

        Ok(TranslationOutput {
            source_text: segment.source_text.clone(),
            translated_text: translated.text,
            mt_elapsed: mt_started.elapsed(),
        })
    }
}

fn samples_to_ms(samples: u64) -> f64 {
    samples as f64 * 1_000.0 / f64::from(SAMPLE_RATE_HZ)
}

fn frames_for_ms(milliseconds: u32) -> usize {
    let samples = u64::from(milliseconds) * u64::from(SAMPLE_RATE_HZ);
    let frame_samples = FRAME_SAMPLES as u64 * 1_000;
    usize::try_from(samples.div_ceil(frame_samples))
        .unwrap_or(usize::MAX)
        .max(1)
}

/// Resolves every native model path once from the shared compatibility config.
/// The desktop preflight and backend launcher use the same fields, so an
/// installed content-addressed package cannot be accepted by one component and
/// ignored by the other.
pub(crate) fn resolved_model_assets(
    config: &AppConfig,
    project_root: &Path,
) -> ResolvedModelAssets {
    ModelAssetsConfig {
        models_directory: config.model_manager.models_directory.clone(),
        qwen3_asr_gguf_directory: config.model_manager.qwen3_asr_gguf_directory.clone(),
        hunyuan_mt_gguf_directory: config.model_manager.hunyuan_mt_gguf_directory.clone(),
    }
    .resolve(project_root)
}

/// The final source and target names for a translation request.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TranslationRoute {
    source: String,
    target: String,
}

fn utterance_to_pcm16le(utterance: Utterance) -> Vec<u8> {
    utterance
        .samples
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect()
}

fn asr_language(source_language: &str) -> Option<String> {
    let source = normalized_code(source_language);
    (source != "auto").then(|| language_name(&source).to_owned())
}

/// Builds the same stateless, route-aware anti-hallucination instruction used
/// by the legacy Qwen3 one-shot path.  It intentionally never includes prior
/// transcript text: each VAD utterance must be recognized independently.
fn asr_instruction(source_language: &str, target_language: &str) -> String {
    let source = normalized_code(source_language);
    if source != "auto" {
        let language = language_name(&source);
        return format!(
            "Transcribe only the audible {language} speech in its original writing system. \
             Return the raw transcript only; never translate, paraphrase, complete, \
             or repeat text from another utterance."
        );
    }

    let pair = target_language
        .split(',')
        .map(normalized_code)
        .filter(|code| !code.is_empty())
        .collect::<Vec<_>>();
    let languages = pair
        .iter()
        .filter(|code| language_name(code) != *code)
        .take(2)
        .map(|code| language_name(code))
        .collect::<Vec<_>>();
    if languages.len() >= 2 {
        return format!(
            "This is bilingual ASR. The current utterance may be {} or {}. \
             Identify the spoken language from the audio and transcribe it in its original \
             writing system. Do not translate between the two languages, infer missing words, \
             or repeat text from earlier utterances. Return only the audible transcript.",
            languages[0], languages[1]
        );
    }

    "Transcribe only speech audible in this audio. Preserve its original writing system. \
     Do not translate, paraphrase, infer missing words, or repeat earlier text."
        .into()
}

fn translation_route(
    transcript: &str,
    source_language: &str,
    target_language: &str,
) -> TranslationRoute {
    let source = normalized_code(source_language);
    let targets = target_language
        .split(',')
        .map(normalized_code)
        .filter(|code| !code.is_empty())
        .collect::<Vec<_>>();

    if source != "auto" {
        return TranslationRoute {
            source: language_name(&source).to_owned(),
            target: language_list(&targets),
        };
    }

    if targets.len() == 2 {
        if let Some(detected) = detect_language(transcript, &targets) {
            let target = targets
                .iter()
                .find(|candidate| candidate.as_str() != detected)
                .expect("two distinct configured route languages");
            return TranslationRoute {
                source: language_name(detected).to_owned(),
                target: language_name(target).to_owned(),
            };
        }
        return TranslationRoute {
            source: "auto".into(),
            target: language_list(&targets),
        };
    }

    TranslationRoute {
        source: "auto".into(),
        target: language_list(&targets),
    }
}

fn normalized_code(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn language_list(codes: &[String]) -> String {
    let names = codes
        .iter()
        .map(|code| language_name(code).to_owned())
        .collect::<Vec<_>>();
    match names.as_slice() {
        [] => "the configured target language".into(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        _ => names.join(", "),
    }
}

fn language_name(code: &str) -> &str {
    match code {
        "af" => "Afrikaans",
        "zh" => "Chinese",
        "en" => "English",
        "fr" => "French",
        "pt" => "Portuguese",
        "es" => "Spanish",
        "ja" => "Japanese",
        "ru" => "Russian",
        "ko" => "Korean",
        "th" => "Thai",
        "it" => "Italian",
        "de" => "German",
        "vi" => "Vietnamese",
        "id" => "Indonesian",
        "pl" => "Polish",
        "cs" => "Czech",
        "nl" => "Dutch",
        "auto" => "automatically detected language",
        _ => code,
    }
}

fn detect_language<'a>(text: &str, candidates: &'a [String]) -> Option<&'a str> {
    let contains = |code: &str| candidates.iter().any(|candidate| candidate == code);
    if contains("ja")
        && text
            .chars()
            .any(|character| ('\u{3040}'..='\u{31ff}').contains(&character))
    {
        return Some("ja");
    }
    if contains("zh")
        && text
            .chars()
            .any(|character| ('\u{3400}'..='\u{9fff}').contains(&character))
    {
        return Some("zh");
    }
    if contains("ru")
        && text
            .chars()
            .any(|character| ('\u{0400}'..='\u{04ff}').contains(&character))
    {
        return Some("ru");
    }
    if contains("ko")
        && text
            .chars()
            .any(|character| ('\u{ac00}'..='\u{d7af}').contains(&character) || ('\u{1100}'..='\u{11ff}').contains(&character))
    {
        return Some("ko");
    }
    if contains("en")
        && text
            .chars()
            .any(|character| character.is_ascii_alphabetic())
    {
        return Some("en");
    }
    None
}

/// Ensures the wire stream is compatible with the initial no-resample path.
pub(crate) fn validate_input_sample_rate(sample_rate: u32) -> Result<(), String> {
    if sample_rate == SAMPLE_RATE_HZ {
        Ok(())
    } else {
        Err(format!(
            "the native Qwen3 route requires {SAMPLE_RATE_HZ} Hz mono PCM; received {sample_rate} Hz"
        ))
    }
}

/// Rejects an oversized WebSocket audio message before it reaches the VAD
/// working buffers.  The transport remains free to send arbitrary frame sizes
/// below this limit.
pub(crate) fn validate_input_chunk_size(bytes: usize) -> Result<(), String> {
    if bytes <= MAX_INPUT_PCM_BYTES {
        Ok(())
    } else {
        Err(format!(
            "PCM WebSocket message is {bytes} bytes; the native backend limit is {MAX_INPUT_PCM_BYTES} bytes"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_INPUT_PCM_BYTES, asr_instruction, asr_language, frames_for_ms, translation_route,
        validate_input_chunk_size, validate_input_sample_rate,
    };

    #[test]
    fn automatic_bilingual_route_follows_the_recognized_script() {
        assert_eq!(
            translation_route("你好", "auto", "zh,en"),
            super::TranslationRoute {
                source: "Chinese".into(),
                target: "English".into(),
            }
        );
        assert_eq!(
            translation_route("hello", "auto", "zh,en"),
            super::TranslationRoute {
                source: "English".into(),
                target: "Chinese".into(),
            }
        );
    }

    #[test]
    fn explicit_language_is_used_for_asr_and_translation() {
        assert_eq!(asr_language("ja"), Some("Japanese".into()));
        assert_eq!(
            translation_route("こんにちは", "ja", "en"),
            super::TranslationRoute {
                source: "Japanese".into(),
                target: "English".into(),
            }
        );
    }

    #[test]
    fn asr_instruction_is_pair_aware_without_prior_transcript_context() {
        let prompt = asr_instruction("auto", "zh,en");
        assert!(prompt.contains("Chinese or English"));
        assert!(prompt.contains("Do not translate"));
        assert!(asr_instruction("ja", "en").contains("audible Japanese"));
    }

    #[test]
    fn initial_native_route_rejects_non_16khz_input() {
        assert!(validate_input_sample_rate(16_000).is_ok());
        assert!(validate_input_sample_rate(48_000).is_err());
    }

    #[test]
    fn input_audio_message_has_a_firm_memory_limit() {
        assert!(validate_input_chunk_size(MAX_INPUT_PCM_BYTES).is_ok());
        assert!(validate_input_chunk_size(MAX_INPUT_PCM_BYTES + 1).is_err());
    }

    #[test]
    fn endpoint_durations_round_up_to_complete_silero_frames() {
        assert_eq!(frames_for_ms(128), 4);
        assert_eq!(frames_for_ms(4_000), 125);
        assert_eq!(frames_for_ms(8_000), 250);
        assert_eq!(frames_for_ms(1), 1);
    }
}
