//! Native configuration reader for the project-root `config.json`.
//!
//! The typed fields cover the settings needed by the native backend. The full
//! parsed document remains available through [`AppConfig::raw`], so optional
//! provider settings can evolve without forcing this crate to model each one.

#![forbid(unsafe_code)]

use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
pub use xr_corpus_core::CorpusConfig as PromptContextConfig;

/// A map of provider-specific settings retained without imposing a model
/// schema on optional providers.
pub type ProviderConfigs = Map<String, Value>;

/// The parsed native-backend configuration and the complete original JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub audio: AudioConfig,
    pub asr: AsrConfig,
    pub denoise: DenoiseConfig,
    pub speaker: SpeakerConfig,
    pub prompt_context: PromptContextConfig,
    pub integrations: IntegrationsConfig,
    pub storage: StorageConfig,
    pub translation: TranslationConfig,
    pub tts: TtsConfig,
    pub model_manager: ModelManagerConfig,
    /// The unmodified parsed document, including sections unknown to this
    /// crate and frontend preferences.
    pub raw: Value,
    /// The source file when this configuration was loaded from disk.
    pub source_path: Option<PathBuf>,
}

impl AppConfig {
    /// Reads and validates JSON syntax from `path`.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref().to_path_buf();
        let contents = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        let mut config = Self::from_json_str(&contents)?;
        config.source_path = Some(path);
        Ok(config)
    }

    /// Parses a `config.json` document without associating it with a path.
    pub fn from_json_str(contents: &str) -> Result<Self, ConfigError> {
        let raw: Value = serde_json::from_str(contents).map_err(ConfigError::InvalidJson)?;
        Self::from_value(raw)
    }

    /// Builds a typed configuration while retaining `raw` exactly as parsed.
    pub fn from_value(raw: Value) -> Result<Self, ConfigError> {
        let typed: TypedConfig =
            serde_json::from_value(raw.clone()).map_err(ConfigError::InvalidStructure)?;
        Ok(Self {
            server: typed.server,
            audio: typed.audio,
            asr: typed.asr,
            denoise: typed.denoise,
            speaker: typed.speaker,
            prompt_context: typed.prompt_context,
            integrations: typed.integrations,
            storage: typed.storage,
            translation: typed.translation,
            tts: typed.tts,
            model_manager: typed.model_manager,
            raw,
            source_path: None,
        })
    }

    /// Resolves the common configuration contract for the selected local ASR
    /// and translation providers without knowing their concrete model family.
    /// Provider factories in the backend decide which implementations they
    /// support; this configuration crate only validates shared local-runtime
    /// fields.
    pub fn native_model_route(&self) -> Result<NativeModelRouteConfig, DefaultGgufValidationError> {
        let mut issues = Vec::new();
        if self.tts.provider.trim() != "none" {
            issues.push(format!(
                "tts.provider must be \"none\" for the default native route until that TTS provider is migrated (found {:?})",
                self.tts.provider
            ));
        }
        let llama_server_path = required_non_empty(
            &self.model_manager.llama_server_path,
            "model_manager.llama_server_path",
            &mut issues,
        );
        let asr = active_native_provider(
            &self.asr.provider,
            &self.asr.providers,
            "asr",
            LocalModelRuntimeConfig {
                context_window_tokens: 2_048,
                max_tokens: 128,
                parallel_slots: 1,
            },
            &mut issues,
        );
        let translation = active_native_provider(
            &self.translation.provider,
            &self.translation.providers,
            "translation",
            LocalModelRuntimeConfig {
                context_window_tokens: 2_048,
                max_tokens: 256,
                parallel_slots: 2,
            },
            &mut issues,
        );

        if issues.is_empty() {
            Ok(NativeModelRouteConfig {
                llama_server_path: PathBuf::from(llama_server_path.expect("checked above")),
                asr: asr.expect("checked above"),
                translation: translation.expect("checked above"),
            })
        } else {
            Err(DefaultGgufValidationError { issues })
        }
    }

    /// Validates the first native, Python-free GGUF route and returns the
    /// values necessary to launch its two `llama-server` children.
    ///
    /// This validates configuration only; it intentionally does not check
    /// whether the executable, model files, or HTTP endpoints exist. Those
    /// environment checks belong to the process supervisor.
    pub fn default_gguf(&self) -> Result<DefaultGgufConfig, DefaultGgufValidationError> {
        let mut issues = Vec::new();

        if self.asr.provider.trim() != "qwen3-gguf" {
            issues.push(format!(
                "asr.provider must be \"qwen3-gguf\" for the default GGUF route (found {:?})",
                self.asr.provider
            ));
        }
        if self.translation.provider.trim() != "hunyuan" {
            issues.push(format!(
                "translation.provider must be \"hunyuan\" for the default GGUF route (found {:?})",
                self.translation.provider
            ));
        }
        if self.tts.provider.trim() != "none" {
            issues.push(format!(
                "tts.provider must be \"none\" for the default native route until that TTS provider is migrated (found {:?})",
                self.tts.provider
            ));
        }

        let llama_server_path = required_non_empty(
            &self.model_manager.llama_server_path,
            "model_manager.llama_server_path",
            &mut issues,
        );
        let hunyuan_gguf_repo = required_non_empty(
            &self.model_manager.hunyuan_gguf_repo,
            "model_manager.hunyuan_gguf_repo",
            &mut issues,
        );
        let asr_url = required_provider_url(
            &self.asr.providers,
            "qwen3-gguf",
            "asr.providers.qwen3-gguf.url",
            &mut issues,
        );
        let translation_url = required_provider_url(
            &self.translation.providers,
            "hunyuan",
            "translation.providers.hunyuan.url",
            &mut issues,
        );
        let asr_runtime = provider_runtime_config(
            &self.asr.providers,
            "qwen3-gguf",
            "asr.providers.qwen3-gguf",
            LocalModelRuntimeConfig {
                context_window_tokens: 2_048,
                max_tokens: 128,
                parallel_slots: 1,
            },
            &mut issues,
        );
        let translation_runtime = provider_runtime_config(
            &self.translation.providers,
            "hunyuan",
            "translation.providers.hunyuan",
            LocalModelRuntimeConfig {
                context_window_tokens: 2_048,
                max_tokens: 256,
                parallel_slots: 2,
            },
            &mut issues,
        );

        if issues.is_empty() {
            Ok(DefaultGgufConfig {
                llama_server_path: PathBuf::from(llama_server_path.expect("checked above")),
                hunyuan_gguf_repo: hunyuan_gguf_repo.expect("checked above"),
                asr_url: asr_url.expect("checked above"),
                translation_url: translation_url.expect("checked above"),
                asr_runtime,
                translation_runtime,
            })
        } else {
            Err(DefaultGgufValidationError { issues })
        }
    }

    /// Convenience form of [`Self::default_gguf`] for callers that only need
    /// validation before their own startup logic.
    pub fn validate_default_gguf(&self) -> Result<(), DefaultGgufValidationError> {
        self.default_gguf().map(|_| ())
    }

    /// Returns the ordered native model-asset keys used by the currently
    /// selected ASR and translation provider objects.  The UI and installer
    /// use these keys to build their model catalogue; they never hard-code a
    /// model name or provider pair.
    #[must_use]
    pub fn active_native_model_assets(&self) -> Vec<String> {
        let mut keys = Vec::new();
        for (provider, providers) in [
            (&self.asr.provider, &self.asr.providers),
            (&self.translation.provider, &self.translation.providers),
        ] {
            let Some(model_asset) = providers
                .get(provider.trim())
                .and_then(Value::as_object)
                .and_then(|model| model.get("model_asset"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|key| !key.is_empty())
            else {
                continue;
            };
            if !keys.iter().any(|existing| existing == model_asset) {
                keys.push(model_asset.to_owned());
            }
        }
        keys
    }
}

fn required_non_empty(value: &str, path: &str, issues: &mut Vec<String>) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        issues.push(format!("{path} must be a non-empty string"));
        None
    } else {
        Some(value.to_owned())
    }
}

fn required_provider_url(
    providers: &ProviderConfigs,
    provider: &str,
    path: &str,
    issues: &mut Vec<String>,
) -> Option<String> {
    let Some(provider_config) = providers.get(provider) else {
        issues.push(format!(
            "{path} is missing because provider {provider:?} is not configured"
        ));
        return None;
    };
    let Some(provider_config) = provider_config.as_object() else {
        issues.push(format!("{path} must be configured inside a JSON object"));
        return None;
    };
    let Some(url) = provider_config.get("url").and_then(Value::as_str) else {
        issues.push(format!("{path} must be a non-empty HTTP URL"));
        return None;
    };
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        issues.push(format!("{path} must start with http:// or https://"));
        return None;
    }
    Some(url.to_owned())
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct TypedConfig {
    #[serde(default)]
    server: ServerConfig,
    #[serde(default)]
    audio: AudioConfig,
    #[serde(default)]
    asr: AsrConfig,
    #[serde(default)]
    denoise: DenoiseConfig,
    #[serde(default)]
    speaker: SpeakerConfig,
    #[serde(default)]
    prompt_context: PromptContextConfig,
    #[serde(default)]
    integrations: IntegrationsConfig,
    #[serde(default)]
    storage: StorageConfig,
    #[serde(default)]
    translation: TranslationConfig,
    #[serde(default)]
    tts: TtsConfig,
    #[serde(default)]
    model_manager: ModelManagerConfig,
}

/// HTTP/WebSocket listener settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_server_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_server_port(),
        }
    }
}

/// Microphone and TTS PCM settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default = "default_pre_buffer_frames")]
    pub pre_buffer_frames: usize,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_tts_sample_rate")]
    pub tts_sample_rate: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            pre_buffer_frames: default_pre_buffer_frames(),
            sample_rate: default_sample_rate(),
            tts_sample_rate: default_tts_sample_rate(),
        }
    }
}

/// ASR selection and untyped provider options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AsrConfig {
    #[serde(default = "default_asr_provider")]
    pub provider: String,
    #[serde(default)]
    pub providers: ProviderConfigs,
    #[serde(default = "default_vad_threshold")]
    pub vad_threshold: f64,
    /// Ordinary silence required to close a short utterance.
    #[serde(default = "default_vad_silence_ms")]
    pub vad_silence_ms: u32,
    /// Duration after which a shorter micro-pause may close the utterance.
    #[serde(default = "default_vad_adaptive_after_ms")]
    pub vad_adaptive_after_ms: u32,
    /// Micro-pause accepted after `vad_adaptive_after_ms`.
    #[serde(default = "default_vad_adaptive_silence_ms")]
    pub vad_adaptive_silence_ms: u32,
    /// Hard limit for speech with no usable pause.
    #[serde(default = "default_vad_max_utterance_ms")]
    pub vad_max_utterance_ms: u32,
    /// Audio copied across a hard boundary to protect split phonemes.
    #[serde(default = "default_vad_overlap_ms")]
    pub vad_overlap_ms: u32,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            provider: default_asr_provider(),
            providers: ProviderConfigs::new(),
            vad_threshold: default_vad_threshold(),
            vad_silence_ms: default_vad_silence_ms(),
            vad_adaptive_after_ms: default_vad_adaptive_after_ms(),
            vad_adaptive_silence_ms: default_vad_adaptive_silence_ms(),
            vad_max_utterance_ms: default_vad_max_utterance_ms(),
            vad_overlap_ms: default_vad_overlap_ms(),
        }
    }
}

impl AsrConfig {
    pub fn provider_config(&self, provider: &str) -> Option<&Value> {
        self.providers.get(provider)
    }
}

/// Native GTCRN-Light v3 speech enhancement and background noise suppression settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DenoiseConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_denoise_model_path")]
    pub model_path: PathBuf,
    #[serde(default = "default_denoise_intra_threads")]
    pub intra_threads: usize,
}

impl Default for DenoiseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model_path: default_denoise_model_path(),
            intra_threads: default_denoise_intra_threads(),
        }
    }
}

/// Native speaker-embedding and online-clustering settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerConfig {
    /// Speaker recognition is opt-in because the exported ONNX model is a
    /// separately licensed/downloaded artifact rather than part of the repo.
    #[serde(default)]
    pub enabled: bool,
    /// ERes2NetV2 ONNX file exported with 3D-Speaker's official exporter.
    #[serde(default = "default_speaker_model_path")]
    pub model_path: PathBuf,
    /// Cosine threshold above which an embedding joins an existing centroid.
    #[serde(default = "default_speaker_similarity_threshold")]
    pub similarity_threshold: f64,
    /// Lower threshold applied only to the immediately previous speaker.
    #[serde(default = "default_same_speaker_hysteresis")]
    pub same_speaker_hysteresis: f64,
    /// Required cosine advantage before changing a plausible previous speaker.
    #[serde(default = "default_speaker_switch_margin")]
    pub speaker_switch_margin: f64,
    /// Strict upper bound for per-session centroid memory.
    #[serde(default = "default_max_speakers")]
    pub max_speakers: usize,
    /// Very short speech is not reliable enough to create a voiceprint.
    #[serde(default = "default_speaker_min_utterance_ms")]
    pub min_utterance_ms: u32,
    /// ONNX Runtime CPU threads reserved for speaker embedding inference.
    #[serde(default = "default_speaker_intra_threads")]
    pub intra_threads: usize,
}

impl Default for SpeakerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_path: default_speaker_model_path(),
            similarity_threshold: default_speaker_similarity_threshold(),
            same_speaker_hysteresis: default_same_speaker_hysteresis(),
            speaker_switch_margin: default_speaker_switch_margin(),
            max_speakers: default_max_speakers(),
            min_utterance_ms: default_speaker_min_utterance_ms(),
            intra_threads: default_speaker_intra_threads(),
        }
    }
}

fn provider_runtime_config(
    providers: &ProviderConfigs,
    provider: &str,
    path: &str,
    defaults: LocalModelRuntimeConfig,
    issues: &mut Vec<String>,
) -> LocalModelRuntimeConfig {
    let object = providers.get(provider).and_then(Value::as_object);
    let mut value = |field: &str, default: u32, minimum: u32, maximum: u32| {
        let Some(raw) = object.and_then(|provider| provider.get(field)) else {
            return default;
        };
        let Some(raw) = raw.as_u64().and_then(|value| u32::try_from(value).ok()) else {
            issues.push(format!("{path}.{field} must be an integer"));
            return default;
        };
        if !(minimum..=maximum).contains(&raw) {
            issues.push(format!(
                "{path}.{field} must be within {minimum}..={maximum}"
            ));
            return default;
        }
        raw
    };
    let runtime = LocalModelRuntimeConfig {
        context_window_tokens: value(
            "context_window_tokens",
            defaults.context_window_tokens,
            256,
            32_768,
        ),
        max_tokens: value("max_tokens", defaults.max_tokens, 16, 4_096),
        parallel_slots: value("parallel_slots", u32::from(defaults.parallel_slots), 1, 16) as u16,
    };
    if runtime.max_tokens.saturating_add(128) > runtime.context_window_tokens {
        issues.push(format!(
            "{path}.context_window_tokens must leave at least 128 input tokens beyond max_tokens"
        ));
    }
    runtime
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_log_directory")]
    pub log_dir: PathBuf,
    #[serde(default = "default_log_max_bytes")]
    pub log_max_bytes: u64,
    #[serde(default = "default_log_retained_files")]
    pub log_retained_files: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            log_dir: default_log_directory(),
            log_max_bytes: default_log_max_bytes(),
            log_retained_files: default_log_retained_files(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IntegrationsConfig {
    #[serde(default)]
    pub vrcx: VrcxIntegrationConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VrcxIntegrationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_vrcx_snapshot_ttl_seconds")]
    pub snapshot_ttl_seconds: u64,
    #[serde(default = "default_vrcx_max_players")]
    pub max_players: usize,
    #[serde(default = "default_vrcx_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default)]
    pub database_path: Option<PathBuf>,
}

impl Default for VrcxIntegrationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            snapshot_ttl_seconds: default_vrcx_snapshot_ttl_seconds(),
            max_players: default_vrcx_max_players(),
            poll_interval_ms: default_vrcx_poll_interval_ms(),
            database_path: None,
        }
    }
}

/// Translation selection and untyped provider options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranslationConfig {
    #[serde(default = "default_translation_provider")]
    pub provider: String,
    #[serde(default)]
    pub providers: ProviderConfigs,
    #[serde(default = "default_source_lang")]
    pub source_lang: String,
    #[serde(default = "default_target_lang")]
    pub target_lang: String,
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            provider: default_translation_provider(),
            providers: ProviderConfigs::new(),
            source_lang: default_source_lang(),
            target_lang: default_target_lang(),
        }
    }
}

impl TranslationConfig {
    pub fn provider_config(&self, provider: &str) -> Option<&Value> {
        self.providers.get(provider)
    }
}

/// TTS selection and optional future-provider options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TtsConfig {
    #[serde(default = "default_tts_provider")]
    pub provider: String,
    #[serde(default)]
    pub providers: ProviderConfigs,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            provider: default_tts_provider(),
            providers: ProviderConfigs::new(),
        }
    }
}

impl TtsConfig {
    pub fn provider_config(&self, provider: &str) -> Option<&Value> {
        self.providers.get(provider)
    }
}

/// Paths and repositories used to manage llama.cpp-backed GGUF models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelManagerConfig {
    #[serde(default = "default_hunyuan_gguf_repo")]
    pub hunyuan_gguf_repo: String,
    #[serde(default = "default_llama_server_path")]
    pub llama_server_path: String,
    /// Release files used by the desktop client's optional llama.cpp installer.
    /// Keeping the URLs in `config.json` makes a release update a configuration
    /// change instead of a client-code change.
    #[serde(default)]
    pub llama_cpp: LlamaCppRuntimeConfig,
    /// Optional models root, resolved relative to `config.json` by the native
    /// backend and desktop client.  Keeping this here prevents each frontend
    /// from inventing a different model search path.
    #[serde(default, alias = "models_root", alias = "model_root")]
    pub models_directory: Option<PathBuf>,
    /// Optional package-directory overrides for a versioned native install.
    #[serde(default)]
    pub qwen3_asr_gguf_directory: Option<PathBuf>,
    #[serde(default)]
    pub hunyuan_mt_gguf_directory: Option<PathBuf>,
}

/// A fixed llama.cpp release and its downloadable Windows archives.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlamaCppRuntimeConfig {
    /// Human-readable release identifier used in installer diagnostics.
    #[serde(default)]
    pub release: String,
    /// Page shown by the desktop client's manual-install link.
    #[serde(default)]
    pub release_page: String,
    /// Exact archive names and URLs available to the automatic installer.
    #[serde(default)]
    pub downloads: Vec<LlamaCppDownload>,
}

/// One llama.cpp archive available from the configured release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlamaCppDownload {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub sha256: String,
}

impl Default for ModelManagerConfig {
    fn default() -> Self {
        Self {
            hunyuan_gguf_repo: default_hunyuan_gguf_repo(),
            llama_server_path: default_llama_server_path(),
            llama_cpp: LlamaCppRuntimeConfig::default(),
            models_directory: None,
            qwen3_asr_gguf_directory: None,
            hunyuan_mt_gguf_directory: None,
        }
    }
}

/// Configuration needed by the default native GGUF supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultGgufConfig {
    pub llama_server_path: PathBuf,
    pub hunyuan_gguf_repo: String,
    pub asr_url: String,
    pub translation_url: String,
    pub asr_runtime: LocalModelRuntimeConfig,
    pub translation_runtime: LocalModelRuntimeConfig,
}

/// Provider-neutral local model route consumed by backend provider factories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeModelRouteConfig {
    pub llama_server_path: PathBuf,
    pub asr: NativeProviderConfig,
    pub translation: NativeProviderConfig,
}

/// Common settings shared by every native model provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeProviderConfig {
    pub provider: String,
    pub url: String,
    /// Stable local package key. Older configurations may omit this and let
    /// the backend provider profile choose its compatibility default.
    pub model_asset: Option<String>,
    pub runtime: LocalModelRuntimeConfig,
    pub supports_prompt_context: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalModelRuntimeConfig {
    pub context_window_tokens: u32,
    pub max_tokens: u32,
    pub parallel_slots: u16,
}

/// JSON parse/read failures for [`AppConfig`].
#[derive(Debug)]
pub enum ConfigError {
    Read { path: PathBuf, source: io::Error },
    InvalidJson(serde_json::Error),
    InvalidStructure(serde_json::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "cannot read configuration {}: {source}",
                    path.display()
                )
            }
            Self::InvalidJson(source) => {
                write!(formatter, "config.json is not valid JSON: {source}")
            }
            Self::InvalidStructure(source) => {
                write!(
                    formatter,
                    "config.json has an invalid setting type: {source}"
                )
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::InvalidJson(source) | Self::InvalidStructure(source) => Some(source),
        }
    }
}

/// A collection of actionable problems in the native default GGUF route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultGgufValidationError {
    issues: Vec<String>,
}

impl DefaultGgufValidationError {
    pub fn issues(&self) -> &[String] {
        &self.issues
    }
}

impl fmt::Display for DefaultGgufValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("default GGUF configuration is not runnable:")?;
        for issue in &self.issues {
            write!(formatter, "\n- {issue}")?;
        }
        Ok(())
    }
}

impl Error for DefaultGgufValidationError {}

fn default_host() -> String {
    "0.0.0.0".into()
}
const fn default_server_port() -> u16 {
    7654
}
const fn default_pre_buffer_frames() -> usize {
    20
}
const fn default_sample_rate() -> u32 {
    16_000
}
const fn default_tts_sample_rate() -> u32 {
    48_000
}
fn default_asr_provider() -> String {
    "qwen3-gguf".into()
}
const fn default_vad_threshold() -> f64 {
    0.6
}
const fn default_vad_silence_ms() -> u32 {
    320
}
const fn default_vad_adaptive_after_ms() -> u32 {
    4_000
}
const fn default_vad_adaptive_silence_ms() -> u32 {
    128
}
const fn default_vad_max_utterance_ms() -> u32 {
    8_000
}
const fn default_vad_overlap_ms() -> u32 {
    256
}
fn default_denoise_model_path() -> PathBuf {
    PathBuf::from("models/gtcrn/gtcrn_simple.onnx")
}
const fn default_denoise_intra_threads() -> usize {
    1
}
fn default_speaker_model_path() -> PathBuf {
    PathBuf::from("models/3D-Speaker-ERes2NetV2/speaker_embedding.onnx")
}
const fn default_true() -> bool {
    true
}
const fn default_vrcx_snapshot_ttl_seconds() -> u64 {
    60
}
const fn default_vrcx_max_players() -> usize {
    80
}
const fn default_vrcx_poll_interval_ms() -> u64 {
    2_000
}
fn default_log_directory() -> PathBuf {
    PathBuf::from("runtime/logs")
}
const fn default_log_max_bytes() -> u64 {
    2 * 1024 * 1024
}
const fn default_log_retained_files() -> usize {
    2
}
const fn default_speaker_similarity_threshold() -> f64 {
    0.56
}
const fn default_same_speaker_hysteresis() -> f64 {
    0.14
}

fn active_native_provider(
    selected_provider: &str,
    providers: &ProviderConfigs,
    section: &str,
    defaults: LocalModelRuntimeConfig,
    issues: &mut Vec<String>,
) -> Option<NativeProviderConfig> {
    let provider = selected_provider.trim();
    if provider.is_empty() {
        issues.push(format!("{section}.provider must be a non-empty string"));
        return None;
    }
    let path = format!("{section}.providers.{provider}");
    let url = required_provider_url(providers, provider, &format!("{path}.url"), issues);
    let object = providers.get(provider).and_then(Value::as_object);
    let model_asset = object
        .and_then(|provider| provider.get("model_asset"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let runtime = provider_runtime_config(providers, provider, &path, defaults, issues);
    let supports_prompt_context = object
        .and_then(|provider| provider.get("supports_prompt_context"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match url {
        Some(url) => Some(NativeProviderConfig {
            provider: provider.to_owned(),
            url,
            model_asset,
            runtime,
            supports_prompt_context,
        }),
        None => None,
    }
}
const fn default_speaker_switch_margin() -> f64 {
    0.04
}
const fn default_max_speakers() -> usize {
    8
}
const fn default_speaker_min_utterance_ms() -> u32 {
    750
}
const fn default_speaker_intra_threads() -> usize {
    2
}
fn default_translation_provider() -> String {
    "hunyuan".into()
}
fn default_source_lang() -> String {
    "auto".into()
}
fn default_target_lang() -> String {
    "zh,en".into()
}
fn default_tts_provider() -> String {
    "none".into()
}
fn default_hunyuan_gguf_repo() -> String {
    "tencent/Hy-MT2-1.8B-GGUF".into()
}
fn default_llama_server_path() -> String {
    "D:/app_install_path/AI/llama.cpp/llama-server.exe".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_config_is_read_with_optional_sections_preserved() {
        let config = AppConfig::from_json_str(include_str!("../../../config.json")).unwrap();

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 7654);
        assert_eq!(config.audio.sample_rate, 16_000);
        assert_eq!(config.audio.tts_sample_rate, 48_000);
        assert_eq!(config.asr.provider, "qwen3-gguf");
        assert_eq!(config.asr.vad_silence_ms, 320);
        assert_eq!(config.asr.vad_adaptive_after_ms, 4_000);
        assert_eq!(config.asr.vad_adaptive_silence_ms, 128);
        assert_eq!(config.asr.vad_max_utterance_ms, 8_000);
        assert_eq!(config.asr.vad_overlap_ms, 256);
        assert!(config.speaker.enabled);
        assert_eq!(config.speaker.max_speakers, 8);
        assert_eq!(config.speaker.min_utterance_ms, 750);
        assert_eq!(config.speaker.same_speaker_hysteresis, 0.12);
        assert_eq!(config.speaker.speaker_switch_margin, 0.04);
        assert!(config.prompt_context.enabled);
        assert_eq!(config.prompt_context.max_entries, 6);
        assert_eq!(config.prompt_context.asr_max_chars, 800);
        assert_eq!(config.prompt_context.asr_history_entries, 1);
        assert_eq!(config.prompt_context.translation_history_entries, 6);
        assert_eq!(config.prompt_context.translation_max_chars, 1200);
        assert_eq!(
            config.prompt_context.corpora_directory,
            PathBuf::from("XR-Corpus/corpora/v1")
        );
        assert_eq!(config.storage.log_dir, PathBuf::from("runtime/logs"));
        assert_eq!(config.storage.log_max_bytes, 2 * 1024 * 1024);
        assert_eq!(config.storage.log_retained_files, 2);
        assert_eq!(config.translation.provider, "hunyuan");
        assert_eq!(config.tts.provider, "none");
        assert_eq!(config.model_manager.llama_cpp.release, "b10333");
        assert_eq!(config.model_manager.llama_cpp.downloads.len(), 5);
        assert_eq!(
            config.model_manager.llama_cpp.downloads[0].name,
            "llama-b10333-bin-win-cpu-x64.zip"
        );
        assert_eq!(
            config
                .raw
                .pointer("/osc/listen_port")
                .and_then(Value::as_u64),
            Some(9001)
        );
    }

    #[test]
    fn legacy_speaker_config_gets_the_safe_switch_margin() {
        let speaker: SpeakerConfig = serde_json::from_str(
            r#"{"enabled":true,"similarity_threshold":0.56,"same_speaker_hysteresis":0.16}"#,
        )
        .unwrap();

        assert_eq!(speaker.speaker_switch_margin, 0.04);
    }

    #[test]
    fn root_config_passes_default_gguf_validation() {
        let config = AppConfig::from_json_str(include_str!("../../../config.json")).unwrap();
        let gguf = config.default_gguf().unwrap();

        assert_eq!(
            gguf.llama_server_path,
            PathBuf::from("D:/app_install_path/AI/llama.cpp/llama-server.exe")
        );
        assert_eq!(gguf.hunyuan_gguf_repo, "tencent/Hy-MT2-1.8B-GGUF");
        assert_eq!(gguf.asr_url, "http://127.0.0.1:8001/v1/chat/completions");
        assert_eq!(
            gguf.translation_url,
            "http://127.0.0.1:8002/v1/chat/completions"
        );
        assert_eq!(gguf.asr_runtime.context_window_tokens, 4_800);
        assert_eq!(gguf.asr_runtime.max_tokens, 128);
        assert_eq!(gguf.asr_runtime.parallel_slots, 1);
        assert_eq!(gguf.translation_runtime.context_window_tokens, 2_048);
        assert_eq!(gguf.translation_runtime.max_tokens, 256);
        assert_eq!(gguf.translation_runtime.parallel_slots, 2);
    }

    #[test]
    fn native_model_route_uses_the_selected_provider_contract() {
        let config = AppConfig::from_json_str(include_str!("../../../config.json")).unwrap();
        let route = config.native_model_route().unwrap();

        assert_eq!(route.asr.provider, "qwen3-gguf");
        assert_eq!(route.asr.model_asset.as_deref(), Some("qwen3-asr-gguf"));
        assert_eq!(route.asr.runtime.context_window_tokens, 4_800);
        assert_eq!(route.translation.provider, "hunyuan");
        assert_eq!(route.translation.model_asset.as_deref(), Some("hy-mt2"));
        assert_eq!(route.translation.runtime.parallel_slots, 2);
    }

    #[test]
    fn native_model_route_does_not_assume_a_provider_family() {
        let mut document: Value =
            serde_json::from_str(include_str!("../../../config.json")).unwrap();
        document["translation"]["provider"] = Value::from("future-local-provider");
        document["translation"]["providers"]["future-local-provider"] = serde_json::json!({
            "url": "http://127.0.0.1:8010/v1/chat/completions",
            "model_asset": "future-translation-model",
            "context_window_tokens": 4096,
            "max_tokens": 512,
            "parallel_slots": 3
        });

        let route = AppConfig::from_value(document)
            .unwrap()
            .native_model_route()
            .unwrap();

        assert_eq!(route.translation.provider, "future-local-provider");
        assert_eq!(
            route.translation.model_asset.as_deref(),
            Some("future-translation-model")
        );
        assert_eq!(route.translation.runtime.parallel_slots, 3);
    }

    #[test]
    fn native_model_route_accepts_legacy_providers_without_an_asset_key() {
        let mut document: Value =
            serde_json::from_str(include_str!("../../../config.json")).unwrap();
        document["asr"]["providers"]["qwen3-gguf"]
            .as_object_mut()
            .unwrap()
            .remove("model_asset");
        document["translation"]["providers"]["hunyuan"]
            .as_object_mut()
            .unwrap()
            .remove("model_asset");

        let route = AppConfig::from_value(document)
            .unwrap()
            .native_model_route()
            .unwrap();

        assert_eq!(route.asr.model_asset, None);
        assert_eq!(route.translation.model_asset, None);
    }

    #[test]
    fn model_runtime_rejects_output_that_leaves_no_input_budget() {
        let mut document: Value =
            serde_json::from_str(include_str!("../../../config.json")).unwrap();
        document["translation"]["providers"]["hunyuan"]["context_window_tokens"] = Value::from(256);
        document["translation"]["providers"]["hunyuan"]["max_tokens"] = Value::from(256);
        let config = AppConfig::from_value(document).unwrap();
        assert!(
            config
                .default_gguf()
                .unwrap_err()
                .to_string()
                .contains("must leave at least 128 input tokens")
        );
    }

    #[test]
    fn gguf_validation_reports_all_actionable_fields() {
        let config = AppConfig::from_json_str(
            r#"{
                "asr": {"provider": "sensevoice"},
                "translation": {"provider": "groq"},
                "tts": {"provider": "index"},
                "model_manager": {"llama_server_path": "", "hunyuan_gguf_repo": ""}
            }"#,
        )
        .unwrap();

        let error = config.default_gguf().unwrap_err();
        let message = error.to_string();
        assert!(message.contains("asr.provider must be \"qwen3-gguf\""));
        assert!(message.contains("translation.provider must be \"hunyuan\""));
        assert!(message.contains("tts.provider must be \"none\""));
        assert!(message.contains("model_manager.llama_server_path must be a non-empty string"));
        assert!(message.contains("asr.providers.qwen3-gguf.url is missing"));
        assert!(message.contains("translation.providers.hunyuan.url is missing"));
    }
}
