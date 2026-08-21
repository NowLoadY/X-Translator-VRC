use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable identifier for a model package required by the initial native route.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelAssetId {
    /// Qwen3-ASR GGUF plus its multimodal projection.
    Qwen3AsrGguf,
    /// Hunyuan MT2 GGUF used by the local translation server.
    HunyuanMtGguf,
    HunyuanMt7bGguf,
    /// Audio8 multilingual TTS ONNX FP16 package, including voice registration.
    Audio8TtsOnnxFp16,
}

impl ModelAssetId {
    /// Stable, machine-readable identifier used in diagnostics and packaging.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Qwen3AsrGguf => "qwen3-asr-gguf",
            Self::HunyuanMtGguf => "hy-mt2",
            Self::HunyuanMt7bGguf => "hy-mt2-big",
            Self::Audio8TtsOnnxFp16 => "audio8-tts-onnx-fp16",
        }
    }

    /// Resolves a stable `model_asset` key stored in a provider object.
    #[must_use]
    pub fn from_config_key(value: &str) -> Option<Self> {
        DEFAULT_GGUF_MANIFEST
            .iter()
            .find(|manifest| manifest.id.as_str() == value)
            .map(|manifest| manifest.id)
    }
}

impl fmt::Display for ModelAssetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Native backend capability provided by a model asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelCapability {
    Asr,
    Translation,
    Tts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelLevel {
    Normal,
    Big,
    Ultra,
}

/// Runtime role of a file inside a model package. Server factories query this
/// role instead of relying on manifest array position.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ModelFileRole {
    Weights,
    MultimodalProjection,
    RuntimeManifest,
    Tokenizer,
    CodecDecoder,
    CodecEncoder,
    FastArGraph,
    CodecDecoderData,
    CodecEncoderData,
    RegistrationManifest,
    SlowArGraph,
}

impl ModelLevel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Big => "big",
            Self::Ultra => "ultra",
        }
    }
}

/// A file that must exist within a [`ModelAssetManifest::relative_directory`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequiredModelFile {
    pub role: ModelFileRole,
    /// File path relative to the asset directory. This is intentionally not a
    /// glob: runtime startup must use a deterministic artifact.
    pub relative_path: &'static str,
    /// Human-readable purpose shown in preflight diagnostics.
    pub purpose: &'static str,
    /// Exact byte length recorded in the versioned source manifest.
    pub bytes: u64,
    /// Lowercase SHA-256 digest of the complete file.
    pub sha256: &'static str,
}

/// Repository metadata retained for installers and release packaging.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelSource {
    /// Source repository expected to contain this asset.
    pub repository: &'static str,
    /// Immutable Hugging Face revision from which every declared file came.
    pub revision: &'static str,
    /// Exact source-file patterns used by an installer, if it has one.
    pub include_patterns: &'static [&'static str],
    /// Per-file source overrides for packages assembled from compatible,
    /// independently versioned exports.
    pub file_overrides: &'static [ModelFileSource],
}

impl ModelSource {
    /// Builds a pinned Hugging Face resolve URL for a manifest file.
    #[must_use]
    pub fn hugging_face_resolve_url(&self, relative_path: &str) -> String {
        if let Some(source) = self
            .file_overrides
            .iter()
            .find(|source| source.relative_path == relative_path)
        {
            return format!(
                "https://huggingface.co/{}/resolve/{}/{}",
                source.repository, source.revision, source.remote_path
            );
        }
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repository, self.revision, relative_path
        )
    }
}

/// Immutable source of one file that differs from the package's primary
/// repository. Download and verification still use the shared installer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelFileSource {
    pub relative_path: &'static str,
    pub repository: &'static str,
    pub revision: &'static str,
    pub remote_path: &'static str,
}

/// Static description of one locally-installed model package.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelAssetManifest {
    pub id: ModelAssetId,
    pub label: &'static str,
    pub capability: ModelCapability,
    pub level: ModelLevel,
    pub provider: &'static str,
    /// Directory relative to the models root.
    pub relative_directory: &'static str,
    pub required_files: &'static [RequiredModelFile],
    pub source: ModelSource,
}

const QWEN3_ASR_REQUIRED_FILES: &[RequiredModelFile] = &[
    RequiredModelFile {
        role: ModelFileRole::Weights,
        relative_path: "Qwen3-ASR-1.7B.Q4_K_M.gguf",
        purpose: "Qwen3-ASR quantized GGUF model",
        bytes: 1_282_435_552,
        sha256: "3893b8926065bbff3da7586d21d8711a9b4fa4fa8f12cd0cefad58e31b2660b6",
    },
    RequiredModelFile {
        role: ModelFileRole::MultimodalProjection,
        relative_path: "Qwen3-ASR-1.7B.mmproj-f16.gguf",
        purpose: "Qwen3-ASR multimodal projection GGUF",
        bytes: 641_774_112,
        sha256: "5bc361e19bfdf3617c85247f9b706f7186ce0d156d9ed3c5d8bca8900b8fc3b7",
    },
];

const HUNYUAN_MT_REQUIRED_FILES: &[RequiredModelFile] = &[RequiredModelFile {
    role: ModelFileRole::Weights,
    relative_path: "Hy-MT2-1.8B-Q4_K_M.gguf",
    purpose: "Hy-MT2 quantized GGUF model",
    bytes: 1_133_080_448,
    sha256: "dc5f44fcf1fa496ee7ad725982c0c8c553a4de00259b53af84c4b89fb0c06699",
}];

const HUNYUAN_MT_7B_REQUIRED_FILES: &[RequiredModelFile] = &[RequiredModelFile {
    role: ModelFileRole::Weights,
    relative_path: "Hy-MT2-7B-Q4_K_M.gguf",
    purpose: "Hy-MT2 7B quantized GGUF model",
    bytes: 4_624_648_896,
    sha256: "9f96256500f3fc1ab4d64336b58f52a949a95ad7516b0c229476eef782f9f77b",
}];

const AUDIO8_TTS_REQUIRED_FILES: &[RequiredModelFile] = &[
    RequiredModelFile {
        role: ModelFileRole::SlowArGraph,
        relative_path: "slow_ar_fp16.onnx",
        purpose: "Audio8 slow autoregressive FP16 ONNX graph",
        bytes: 1_348_765_672,
        sha256: "7b58e12eddca63b45d52a833d6f697b02d5431a8538b6cd8f1b115ecd9bded82",
    },
    RequiredModelFile {
        role: ModelFileRole::FastArGraph,
        relative_path: "fast_ar_fp16.onnx",
        purpose: "Audio8 fast autoregressive FP16 ONNX graph",
        bytes: 134_041_582,
        sha256: "33dd894dc73dad4c3f74fc1a3505b88a2489684a441052bc94fc6700ea106ccd",
    },
    RequiredModelFile {
        role: ModelFileRole::CodecDecoder,
        relative_path: "codec_decoder_fp16.onnx",
        purpose: "Audio8 FP16 codec decoder graph",
        bytes: 594_319,
        sha256: "6e379be31db6c1b0c111e0e3d2aeb10717ee96b197462b926de411e75a1fd019",
    },
    RequiredModelFile {
        role: ModelFileRole::CodecDecoderData,
        relative_path: "codec_decoder_fp16.onnx.data",
        purpose: "Audio8 FP16 codec decoder weights",
        bytes: 260_741_440,
        sha256: "18838f686aa7c1528fb69ec11e1ab404fdc4dc823d13219abfd4b327988527c0",
    },
    RequiredModelFile {
        role: ModelFileRole::CodecEncoder,
        relative_path: "registration/codec_encoder_fp16.onnx",
        purpose: "Audio8 voice registration codec encoder",
        bytes: 940_787,
        sha256: "e856d7999442cdc8f1f2ed0d2c055532cf359f0dd6d9a44fd4b98584c5d5dfa5",
    },
    RequiredModelFile {
        role: ModelFileRole::CodecEncoderData,
        relative_path: "registration/codec_encoder_fp16.onnx.data",
        purpose: "Audio8 voice registration codec encoder weights",
        bytes: 414_425_088,
        sha256: "19c740fcc4d45aa2546e9ab86e31c6200955c4b0a139758296fbf1064bf009cd",
    },
    RequiredModelFile {
        role: ModelFileRole::RuntimeManifest,
        relative_path: "runtime_manifest.json",
        purpose: "Audio8 runtime manifest",
        bytes: 1_080,
        sha256: "6473ae7d0106a2e369e442c72a71d2d46d8fbd3fe18c80d80b1b46e4aa241930",
    },
    RequiredModelFile {
        role: ModelFileRole::RegistrationManifest,
        relative_path: "registration/registration_manifest.json",
        purpose: "Audio8 voice registration manifest",
        bytes: 165,
        sha256: "36ef9d2f435f0f7b5ab66dc78a44411a24c0ab9e3a2c63738babe575747a584f",
    },
    RequiredModelFile {
        role: ModelFileRole::Tokenizer,
        relative_path: "tokenizer/tokenizer.json",
        purpose: "Audio8 tokenizer",
        bytes: 12_217_872,
        sha256: "f24e08099d45a8adf3f52f5f0b03276e433bb9d689bb15fcbcc48ce58744588b",
    },
];

/// Default local Qwen3-ASR GGUF package.
pub const QWEN3_ASR_GGUF: ModelAssetManifest = ModelAssetManifest {
    id: ModelAssetId::Qwen3AsrGguf,
    label: "Speech Recognition Model",
    capability: ModelCapability::Asr,
    level: ModelLevel::Normal,
    provider: "qwen3-gguf",
    relative_directory: "Qwen3-ASR-1.7B-GGUF",
    required_files: QWEN3_ASR_REQUIRED_FILES,
    source: ModelSource {
        repository: "mradermacher/Qwen3-ASR-1.7B-GGUF",
        revision: "cc946c78d3804752f7ba1bc42720c0f7aaf3d1ad",
        include_patterns: &["*Q4_K_M.gguf", "*mmproj-f16.gguf"],
        file_overrides: &[],
    },
};

/// Default local Hy-MT2 GGUF package.
pub const HUNYUAN_MT_GGUF: ModelAssetManifest = ModelAssetManifest {
    id: ModelAssetId::HunyuanMtGguf,
    label: "Translation Model",
    capability: ModelCapability::Translation,
    level: ModelLevel::Normal,
    provider: "hunyuan",
    relative_directory: "HY-MT2",
    required_files: HUNYUAN_MT_REQUIRED_FILES,
    source: ModelSource {
        repository: "tencent/Hy-MT2-1.8B-GGUF",
        revision: "1cd5208700acedef4ef93019b6cfc148b8522d45",
        include_patterns: &["Hy-MT2-1.8B-Q4_K_M.gguf"],
        file_overrides: &[],
    },
};

/// Larger local Hy-MT2 GGUF package.
pub const HUNYUAN_MT_7B_GGUF: ModelAssetManifest = ModelAssetManifest {
    id: ModelAssetId::HunyuanMt7bGguf,
    label: "Translation Model",
    capability: ModelCapability::Translation,
    level: ModelLevel::Big,
    provider: "hunyuan",
    relative_directory: "Hy-MT2-7B-GGUF",
    required_files: HUNYUAN_MT_7B_REQUIRED_FILES,
    source: ModelSource {
        repository: "tencent/Hy-MT2-7B-GGUF",
        revision: "707464294cf5b2a5a69982855020858ed58cf1d1",
        include_patterns: &["Hy-MT2-7B-Q4_K_M.gguf"],
        file_overrides: &[],
    },
};

const AUDIO8_FP16_SOURCE_OVERRIDES: &[ModelFileSource] = &[
    ModelFileSource {
        relative_path: "slow_ar_fp16.onnx",
        repository: "OpenVoiceOS/phoonnx-audio8-tts",
        revision: "6e4de996325cebb25df81efd6b0adc08792cd21f",
        remote_path: "slow_ar_fp16.onnx",
    },
    ModelFileSource {
        relative_path: "fast_ar_fp16.onnx",
        repository: "OpenVoiceOS/phoonnx-audio8-tts",
        revision: "6e4de996325cebb25df81efd6b0adc08792cd21f",
        remote_path: "fast_ar_fp16.onnx",
    },
];

/// High-quality Audio8 zero-shot voice-cloning package for its ONNX provider.
pub const AUDIO8_TTS_ONNX_FP16: ModelAssetManifest = ModelAssetManifest {
    id: ModelAssetId::Audio8TtsOnnxFp16,
    label: "Audio8 TTS (ONNX FP16)",
    capability: ModelCapability::Tts,
    level: ModelLevel::Normal,
    provider: "audio8",
    relative_directory: "Audio8-TTS-Preview-0.6B-ONNX-FP16",
    required_files: AUDIO8_TTS_REQUIRED_FILES,
    source: ModelSource {
        repository: "Audio8/Audio8-TTS-Preview-0.6B-ONNX-INT4",
        revision: "818569c6b832118ad68d61bbd873abe250fcd68a",
        include_patterns: &["*.onnx", "*.onnx.data", "*.json"],
        file_overrides: AUDIO8_FP16_SOURCE_OVERRIDES,
    },
};

/// All GGUF packages required by the first Python-free route.
pub const DEFAULT_GGUF_MANIFEST: &[ModelAssetManifest] = &[
    QWEN3_ASR_GGUF,
    HUNYUAN_MT_GGUF,
    HUNYUAN_MT_7B_GGUF,
    AUDIO8_TTS_ONNX_FP16,
];

pub fn manifests_for_capability(
    capability: ModelCapability,
) -> impl Iterator<Item = &'static ModelAssetManifest> {
    DEFAULT_GGUF_MANIFEST
        .iter()
        .filter(move |manifest| manifest.capability == capability)
}

/// Returns the static manifest for `id`.
#[must_use]
pub fn manifest_for(id: ModelAssetId) -> &'static ModelAssetManifest {
    DEFAULT_GGUF_MANIFEST
        .iter()
        .find(|manifest| manifest.id == id)
        .expect("every model asset id must have a catalog manifest")
}
