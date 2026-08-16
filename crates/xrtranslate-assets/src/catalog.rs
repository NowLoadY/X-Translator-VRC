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
}

impl ModelAssetId {
    /// Stable, machine-readable identifier used in diagnostics and packaging.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Qwen3AsrGguf => "qwen3-asr-gguf",
            Self::HunyuanMtGguf => "hy-mt2",
            Self::HunyuanMt7bGguf => "hy-mt2-big",
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
}

impl ModelSource {
    /// Builds a pinned Hugging Face resolve URL for a manifest file.
    #[must_use]
    pub fn hugging_face_resolve_url(&self, relative_path: &str) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repository, self.revision, relative_path
        )
    }
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
    },
};

/// All GGUF packages required by the first Python-free route.
pub const DEFAULT_GGUF_MANIFEST: &[ModelAssetManifest] =
    &[QWEN3_ASR_GGUF, HUNYUAN_MT_GGUF, HUNYUAN_MT_7B_GGUF];

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
