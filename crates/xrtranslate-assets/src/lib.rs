//! Local model manifests and preflight checks for the native backend.
//!
//! This crate deliberately declares **where** the default GGUF assets belong
//! and checks whether they can be used. It does not download models or contact
//! Hugging Face. A higher-level downloader may use its integrity verifier and
//! atomic staging promotion primitive, but owns network, retry, and UI policy.

#![forbid(unsafe_code)]

use std::{
    error::Error,
    fmt, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use reqwest::{StatusCode, header::RANGE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

/// Stable identifier for a model package required by the initial native route.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelAssetId {
    /// Qwen3-ASR GGUF plus its multimodal projection.
    Qwen3AsrGguf,
    /// Hunyuan MT2 GGUF used by the local translation server.
    HunyuanMtGguf,
}

impl ModelAssetId {
    /// Stable, machine-readable identifier used in diagnostics and packaging.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Qwen3AsrGguf => "qwen3-asr-gguf",
            Self::HunyuanMtGguf => "hy-mt2",
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

impl ModelSource {
    /// Builds a pinned Hugging Face resolve URL for a manifest file.
    ///
    /// The repository, revision, and file names come only from the compiled
    /// manifest.  An installer must never replace the revision with `main` or
    /// another mutable branch name.
    #[must_use]
    pub fn hugging_face_resolve_url(&self, relative_path: &str) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repository, self.revision, relative_path
        )
    }
}

/// Native backend capability provided by a model asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelCapability {
    Asr,
    Translation,
}

/// A file that must exist within a [`ModelAssetManifest::relative_directory`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequiredModelFile {
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
///
/// This crate contains no download client; a higher-level installer resolves
/// immutable source URLs from this declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelSource {
    /// Source repository expected to contain this asset.
    pub repository: &'static str,
    /// Immutable Hugging Face revision from which every declared file came.
    pub revision: &'static str,
    /// Exact source-file patterns used by an installer, if it has one.
    pub include_patterns: &'static [&'static str],
}

/// Static description of one locally-installed model package.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelAssetManifest {
    pub id: ModelAssetId,
    pub label: &'static str,
    pub capability: ModelCapability,
    pub provider: &'static str,
    /// Directory relative to the models root.
    pub relative_directory: &'static str,
    pub required_files: &'static [RequiredModelFile],
    pub source: ModelSource,
}

const QWEN3_ASR_REQUIRED_FILES: &[RequiredModelFile] = &[
    RequiredModelFile {
        relative_path: "Qwen3-ASR-1.7B.Q4_K_M.gguf",
        purpose: "Qwen3-ASR quantized GGUF model",
        bytes: 1_282_435_552,
        sha256: "3893b8926065bbff3da7586d21d8711a9b4fa4fa8f12cd0cefad58e31b2660b6",
    },
    RequiredModelFile {
        relative_path: "Qwen3-ASR-1.7B.mmproj-f16.gguf",
        purpose: "Qwen3-ASR multimodal projection GGUF",
        bytes: 641_774_112,
        sha256: "5bc361e19bfdf3617c85247f9b706f7186ce0d156d9ed3c5d8bca8900b8fc3b7",
    },
];

const HUNYUAN_MT_REQUIRED_FILES: &[RequiredModelFile] = &[RequiredModelFile {
    relative_path: "Hy-MT2-1.8B-Q4_K_M.gguf",
    purpose: "Hy-MT2 quantized GGUF model",
    bytes: 1_133_080_448,
    sha256: "dc5f44fcf1fa496ee7ad725982c0c8c553a4de00259b53af84c4b89fb0c06699",
}];

/// Default local Qwen3-ASR GGUF package.
pub const QWEN3_ASR_GGUF: ModelAssetManifest = ModelAssetManifest {
    id: ModelAssetId::Qwen3AsrGguf,
    label: "Speech Recognition Model",
    capability: ModelCapability::Asr,
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
    provider: "hunyuan",
    relative_directory: "HY-MT2",
    required_files: HUNYUAN_MT_REQUIRED_FILES,
    source: ModelSource {
        repository: "tencent/Hy-MT2-1.8B-GGUF",
        revision: "1cd5208700acedef4ef93019b6cfc148b8522d45",
        include_patterns: &["Hy-MT2-1.8B-Q4_K_M.gguf"],
    },
};

/// All GGUF packages required by the first Python-free route.
pub const DEFAULT_GGUF_MANIFEST: &[ModelAssetManifest] = &[QWEN3_ASR_GGUF, HUNYUAN_MT_GGUF];

/// Returns the static manifest for `id`.
#[must_use]
pub const fn manifest_for(id: ModelAssetId) -> &'static ModelAssetManifest {
    match id {
        ModelAssetId::Qwen3AsrGguf => &QWEN3_ASR_GGUF,
        ModelAssetId::HunyuanMtGguf => &HUNYUAN_MT_GGUF,
    }
}

/// Optional path overrides read from the model-manager configuration.
///
/// Every relative value is interpreted relative to the project root, never to
/// the current working directory. In an unchanged `config.json` this struct is
/// simply its default and paths resolve beneath `<project-root>/models`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelAssetsConfig {
    /// Override the default `<project-root>/models` directory.
    #[serde(alias = "models_root", alias = "model_root")]
    pub models_directory: Option<PathBuf>,
    /// Override the Qwen3 package directory, while retaining its fixed
    /// required filenames. It may be an absolute path.
    pub qwen3_asr_gguf_directory: Option<PathBuf>,
    /// Override the Hy-MT2 package directory, while retaining its fixed
    /// required filename. It may be an absolute path.
    pub hunyuan_mt_gguf_directory: Option<PathBuf>,
}

impl ModelAssetsConfig {
    /// Resolves configured paths with a stable project-root base.
    #[must_use]
    pub fn resolve(&self, project_root: impl AsRef<Path>) -> ResolvedModelAssets {
        let project_root = project_root.as_ref().to_path_buf();
        let models_directory = resolve_from_project_root(
            &project_root,
            self.models_directory
                .as_deref()
                .unwrap_or_else(|| Path::new("models")),
        );
        let qwen3_directory = self
            .qwen3_asr_gguf_directory
            .as_deref()
            .map(|path| resolve_from_project_root(&project_root, path))
            .unwrap_or_else(|| models_directory.join(QWEN3_ASR_GGUF.relative_directory));
        let hunyuan_directory = self
            .hunyuan_mt_gguf_directory
            .as_deref()
            .map(|path| resolve_from_project_root(&project_root, path))
            .unwrap_or_else(|| models_directory.join(HUNYUAN_MT_GGUF.relative_directory));

        ResolvedModelAssets {
            project_root,
            models_directory,
            qwen3_asr: ResolvedModelAsset::new(&QWEN3_ASR_GGUF, qwen3_directory),
            hunyuan_mt: ResolvedModelAsset::new(&HUNYUAN_MT_GGUF, hunyuan_directory),
        }
    }
}

/// Resolves a path relative to `project_root`, preserving absolute paths.
#[must_use]
pub fn resolve_from_project_root(project_root: &Path, configured_path: &Path) -> PathBuf {
    if configured_path.is_absolute() {
        configured_path.to_path_buf()
    } else {
        project_root.join(configured_path)
    }
}

/// The two concrete model packages and their resolved local paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedModelAssets {
    pub project_root: PathBuf,
    pub models_directory: PathBuf,
    pub qwen3_asr: ResolvedModelAsset,
    pub hunyuan_mt: ResolvedModelAsset,
}

impl ResolvedModelAssets {
    /// Builds the default project layout without any configuration overrides.
    #[must_use]
    pub fn for_project_root(project_root: impl AsRef<Path>) -> Self {
        ModelAssetsConfig::default().resolve(project_root)
    }

    /// Checks every declared file without modifying its contents.
    #[must_use]
    pub fn check(&self) -> ModelAssetsPreflight {
        let diagnostics = self
            .iter()
            .into_iter()
            .flat_map(ResolvedModelAsset::check)
            .collect();
        ModelAssetsPreflight { diagnostics }
    }

    /// Performs the expensive cryptographic verification explicitly requested
    /// by an installer, update flow, or user-facing “Verify models” command.
    ///
    /// Normal backend startup deliberately calls [`Self::check`] instead so it
    /// does not re-read more than 3 GB of model files on every launch.
    #[must_use]
    pub fn verify_integrity(&self) -> ModelAssetsPreflight {
        let diagnostics = self
            .iter()
            .into_iter()
            .flat_map(ResolvedModelAsset::verify_integrity)
            .collect();
        ModelAssetsPreflight { diagnostics }
    }

    /// Returns the deterministic paths needed by the two default
    /// `llama-server` specifications. Call [`Self::check`] before spawning.
    #[must_use]
    pub fn llama_cpp_paths(&self) -> DefaultLlamaCppPaths {
        DefaultLlamaCppPaths {
            qwen3_asr_model: self.qwen3_asr.required_file_path(0),
            qwen3_asr_mmproj: self.qwen3_asr.required_file_path(1),
            hunyuan_mt_model: self.hunyuan_mt.required_file_path(0),
        }
    }

    /// Returns an asset by its configuration key.  Callers use this to turn
    /// the active provider configuration into an installer catalogue.
    #[must_use]
    pub fn asset(&self, id: ModelAssetId) -> &ResolvedModelAsset {
        match id {
            ModelAssetId::Qwen3AsrGguf => &self.qwen3_asr,
            ModelAssetId::HunyuanMtGguf => &self.hunyuan_mt,
        }
    }

    /// Atomically enables one fully downloaded package from a staging
    /// directory.  The source must be created on the same filesystem as the
    /// final package directory; the method intentionally never overwrites an
    /// existing installation.
    ///
    /// Downloaders are expected to write to a unique child of the models root,
    /// verify it, and only then call this method.  A failed or interrupted
    /// download therefore cannot leave a partially updated active package.
    pub fn install_from_staging(
        &self,
        id: ModelAssetId,
        staging_directory: impl AsRef<Path>,
    ) -> Result<PathBuf, AtomicInstallError> {
        let target = match id {
            ModelAssetId::Qwen3AsrGguf => &self.qwen3_asr,
            ModelAssetId::HunyuanMtGguf => &self.hunyuan_mt,
        };
        install_verified_directory(target, staging_directory.as_ref())
    }

    fn iter(&self) -> [&ResolvedModelAsset; 2] {
        [&self.qwen3_asr, &self.hunyuan_mt]
    }
}

/// Failure while atomically promoting a verified model package.
#[derive(Debug)]
pub enum AtomicInstallError {
    StagingInvalid {
        directory: PathBuf,
        source: ModelAssetsPreflightError,
    },
    DestinationExists(PathBuf),
    CreateParent {
        path: PathBuf,
        source: io::Error,
    },
    Rename {
        staging: PathBuf,
        destination: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for AtomicInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StagingInvalid { directory, source } => {
                write!(
                    formatter,
                    "staged model package at {} is invalid: {source}",
                    directory.display()
                )
            }
            Self::DestinationExists(path) => write!(
                formatter,
                "refusing to overwrite existing model package at {}",
                path.display()
            ),
            Self::CreateParent { path, source } => {
                write!(
                    formatter,
                    "cannot create model parent {}: {source}",
                    path.display()
                )
            }
            Self::Rename {
                staging,
                destination,
                source,
            } => write!(
                formatter,
                "cannot atomically activate staged package {} at {}: {source}",
                staging.display(),
                destination.display()
            ),
        }
    }
}

impl Error for AtomicInstallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StagingInvalid { source, .. } => Some(source),
            Self::CreateParent { source, .. } | Self::Rename { source, .. } => Some(source),
            Self::DestinationExists(_) => None,
        }
    }
}

fn install_verified_directory(
    target: &ResolvedModelAsset,
    staging_directory: &Path,
) -> Result<PathBuf, AtomicInstallError> {
    let staged = ResolvedModelAsset::new(target.manifest, staging_directory.to_path_buf());
    ModelAssetsPreflight {
        diagnostics: staged.verify_integrity(),
    }
    .into_result()
    .map_err(|source| AtomicInstallError::StagingInvalid {
        directory: staging_directory.to_path_buf(),
        source,
    })?;

    let destination = target.directory.clone();
    if destination.exists() {
        return Err(AtomicInstallError::DestinationExists(destination));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| AtomicInstallError::CreateParent {
            path: destination.clone(),
            source: io::Error::other("model package destination has no parent"),
        })?;
    fs::create_dir_all(parent).map_err(|source| AtomicInstallError::CreateParent {
        path: parent.to_path_buf(),
        source,
    })?;
    fs::rename(staging_directory, &destination).map_err(|source| AtomicInstallError::Rename {
        staging: staging_directory.to_path_buf(),
        destination: destination.clone(),
        source,
    })?;
    Ok(destination)
}

/// Progress reported while a single immutable model file is transferred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadProgress {
    pub asset_id: ModelAssetId,
    pub relative_path: &'static str,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

/// Download and installation failures that preserve the staging directory for
/// a safe retry.  The installer never deletes an active model package.
#[derive(Debug)]
pub enum ModelDownloadError {
    HttpClient(reqwest::Error),
    StagingDirectory {
        path: PathBuf,
        source: io::Error,
    },
    Locked(PathBuf),
    Lock {
        path: PathBuf,
        source: io::Error,
    },
    HttpStatus {
        url: String,
        status: StatusCode,
    },
    Transfer {
        url: String,
        source: reqwest::Error,
    },
    FileIo {
        path: PathBuf,
        source: io::Error,
    },
    TooLarge {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    Integrity {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    AtomicInstall(AtomicInstallError),
}

impl fmt::Display for ModelDownloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HttpClient(error) => {
                write!(formatter, "cannot create model download client: {error}")
            }
            Self::StagingDirectory { path, source } => {
                write!(
                    formatter,
                    "cannot create model staging directory {}: {source}",
                    path.display()
                )
            }
            Self::Locked(path) => write!(
                formatter,
                "another model installer already owns the package lock at {}",
                path.display()
            ),
            Self::Lock { path, source } => {
                write!(
                    formatter,
                    "cannot acquire package lock {}: {source}",
                    path.display()
                )
            }
            Self::HttpStatus { url, status } => {
                write!(formatter, "model download {url} returned HTTP {status}")
            }
            Self::Transfer { url, source } => {
                write!(formatter, "model download {url} failed: {source}")
            }
            Self::FileIo { path, source } => {
                write!(
                    formatter,
                    "cannot write staged model file {}: {source}",
                    path.display()
                )
            }
            Self::TooLarge {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "staged model file {} is {actual} bytes; expected at most {expected} bytes",
                path.display()
            ),
            Self::Integrity {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "staged model file {} has SHA-256 {actual}; expected {expected}",
                path.display()
            ),
            Self::AtomicInstall(error) => error.fmt(formatter),
        }
    }
}

impl Error for ModelDownloadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::HttpClient(error) | Self::Transfer { source: error, .. } => Some(error),
            Self::StagingDirectory { source, .. }
            | Self::Lock { source, .. }
            | Self::FileIo { source, .. } => Some(source),
            Self::AtomicInstall(error) => Some(error),
            Self::Locked(_)
            | Self::HttpStatus { .. }
            | Self::TooLarge { .. }
            | Self::Integrity { .. } => None,
        }
    }
}

/// Native downloader for the compiled, immutable GGUF manifest.
///
/// Files are resumed through HTTP Range requests in a deterministic staging
/// directory, verified against byte length and SHA-256, then promoted through
/// [`ResolvedModelAssets::install_from_staging`].  It does not perform hidden
/// downloads at backend startup; a desktop UI or installer command must call
/// it explicitly.
#[derive(Clone, Debug)]
pub struct NativeModelInstaller {
    assets: ResolvedModelAssets,
    client: reqwest::Client,
}

impl NativeModelInstaller {
    pub fn new(assets: ResolvedModelAssets) -> Result<Self, ModelDownloadError> {
        let client = reqwest::Client::builder()
            .user_agent(concat!(
                "xrtranslate-assets/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(ModelDownloadError::HttpClient)?;
        Ok(Self { assets, client })
    }

    /// Downloads exactly one default package.  `on_progress` is called for
    /// resumed bytes too, so a UI can render a stable progress bar.
    pub async fn install(
        &self,
        id: ModelAssetId,
        mut on_progress: impl FnMut(DownloadProgress),
    ) -> Result<PathBuf, ModelDownloadError> {
        let target = self.asset(id);
        if target.directory().exists() {
            if target.verify_integrity().is_empty() {
                return Ok(target.directory().to_path_buf());
            }
            return Err(ModelDownloadError::AtomicInstall(
                AtomicInstallError::DestinationExists(target.directory().to_path_buf()),
            ));
        }
        let staging = self.staging_directory(target.manifest());
        fs::create_dir_all(&staging).map_err(|source| ModelDownloadError::StagingDirectory {
            path: staging.clone(),
            source,
        })?;
        let _lock =
            InstallLock::acquire(staging.parent().expect("staging directory has parent"), id)?;

        for file in target.manifest().required_files {
            self.download_file(
                id,
                target.manifest().source,
                *file,
                &staging,
                &mut on_progress,
            )
            .await?;
        }
        self.assets
            .install_from_staging(id, staging)
            .map_err(ModelDownloadError::AtomicInstall)
    }

    fn asset(&self, id: ModelAssetId) -> &ResolvedModelAsset {
        match id {
            ModelAssetId::Qwen3AsrGguf => &self.assets.qwen3_asr,
            ModelAssetId::HunyuanMtGguf => &self.assets.hunyuan_mt,
        }
    }

    fn staging_directory(&self, manifest: &ModelAssetManifest) -> PathBuf {
        self.assets
            .models_directory
            .join(".xrtranslate-staging")
            .join(format!(
                "{}-{}",
                manifest.id.as_str(),
                manifest.source.revision
            ))
    }

    async fn download_file(
        &self,
        id: ModelAssetId,
        source: ModelSource,
        file: RequiredModelFile,
        staging: &Path,
        on_progress: &mut impl FnMut(DownloadProgress),
    ) -> Result<(), ModelDownloadError> {
        let complete = staging.join(file.relative_path);
        if complete.is_file() {
            let actual_size = fs::metadata(&complete)
                .map_err(|source| ModelDownloadError::FileIo {
                    path: complete.clone(),
                    source,
                })?
                .len();
            if actual_size != file.bytes {
                return Err(ModelDownloadError::TooLarge {
                    path: complete,
                    expected: file.bytes,
                    actual: actual_size,
                });
            }
            let actual = sha256_file(&complete).map_err(|source| ModelDownloadError::FileIo {
                path: complete.clone(),
                source,
            })?;
            if !actual.eq_ignore_ascii_case(file.sha256) {
                return Err(ModelDownloadError::Integrity {
                    path: complete,
                    expected: file.sha256.to_owned(),
                    actual,
                });
            }
            on_progress(DownloadProgress {
                asset_id: id,
                relative_path: file.relative_path,
                downloaded_bytes: file.bytes,
                total_bytes: file.bytes,
            });
            return Ok(());
        }

        let partial = staging.join(format!("{}.part", file.relative_path));
        let existing = fs::metadata(&partial)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if existing > file.bytes {
            return Err(ModelDownloadError::TooLarge {
                path: partial,
                expected: file.bytes,
                actual: existing,
            });
        }
        if existing == file.bytes {
            let actual = sha256_file(&partial).map_err(|source| ModelDownloadError::FileIo {
                path: partial.clone(),
                source,
            })?;
            if !actual.eq_ignore_ascii_case(file.sha256) {
                return Err(ModelDownloadError::Integrity {
                    path: partial,
                    expected: file.sha256.to_owned(),
                    actual,
                });
            }
            tokio::fs::rename(&partial, &complete)
                .await
                .map_err(|source| ModelDownloadError::FileIo {
                    path: complete,
                    source,
                })?;
            on_progress(DownloadProgress {
                asset_id: id,
                relative_path: file.relative_path,
                downloaded_bytes: file.bytes,
                total_bytes: file.bytes,
            });
            return Ok(());
        }
        if let Some(parent) = partial.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|source| {
                ModelDownloadError::StagingDirectory {
                    path: parent.to_path_buf(),
                    source,
                }
            })?;
        }
        let url = source.hugging_face_resolve_url(file.relative_path);
        let mut request = self.client.get(&url);
        if existing > 0 {
            request = request.header(RANGE, format!("bytes={existing}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|source| ModelDownloadError::Transfer {
                url: url.clone(),
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(ModelDownloadError::HttpStatus { url, status });
        }
        let append = existing > 0 && status == StatusCode::PARTIAL_CONTENT;
        let mut downloaded = if append { existing } else { 0 };
        let mut output = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&partial)
            .await
            .map_err(|source| ModelDownloadError::FileIo {
                path: partial.clone(),
                source,
            })?;
        on_progress(DownloadProgress {
            asset_id: id,
            relative_path: file.relative_path,
            downloaded_bytes: downloaded,
            total_bytes: file.bytes,
        });
        let mut response = response;
        while let Some(chunk) =
            response
                .chunk()
                .await
                .map_err(|source| ModelDownloadError::Transfer {
                    url: url.clone(),
                    source,
                })?
        {
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            if downloaded > file.bytes {
                return Err(ModelDownloadError::TooLarge {
                    path: partial,
                    expected: file.bytes,
                    actual: downloaded,
                });
            }
            output
                .write_all(&chunk)
                .await
                .map_err(|source| ModelDownloadError::FileIo {
                    path: partial.clone(),
                    source,
                })?;
            on_progress(DownloadProgress {
                asset_id: id,
                relative_path: file.relative_path,
                downloaded_bytes: downloaded,
                total_bytes: file.bytes,
            });
        }
        output
            .flush()
            .await
            .map_err(|source| ModelDownloadError::FileIo {
                path: partial.clone(),
                source,
            })?;
        output
            .sync_all()
            .await
            .map_err(|source| ModelDownloadError::FileIo {
                path: partial.clone(),
                source,
            })?;
        if downloaded != file.bytes {
            return Err(ModelDownloadError::TooLarge {
                path: partial,
                expected: file.bytes,
                actual: downloaded,
            });
        }
        tokio::fs::rename(&partial, &complete)
            .await
            .map_err(|source| ModelDownloadError::FileIo {
                path: complete,
                source,
            })?;
        Ok(())
    }
}

struct InstallLock {
    path: PathBuf,
}

impl InstallLock {
    fn acquire(staging_parent: &Path, id: ModelAssetId) -> Result<Self, ModelDownloadError> {
        let path = staging_parent.join(format!("{}.lock", id.as_str()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write;
                writeln!(
                    file,
                    "pid={} created_unix={}",
                    std::process::id(),
                    unix_seconds()
                )
                .map_err(|source| ModelDownloadError::Lock {
                    path: path.clone(),
                    source,
                })?;
                Ok(Self { path })
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Err(ModelDownloadError::Locked(path))
            }
            Err(source) => Err(ModelDownloadError::Lock { path, source }),
        }
    }
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Concrete paths consumed by the default Qwen3-ASR and Hy-MT2 servers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefaultLlamaCppPaths {
    pub qwen3_asr_model: PathBuf,
    pub qwen3_asr_mmproj: PathBuf,
    pub hunyuan_mt_model: PathBuf,
}

/// A resolved static manifest and the directory where it should be installed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedModelAsset {
    manifest: &'static ModelAssetManifest,
    directory: PathBuf,
}

impl ResolvedModelAsset {
    fn new(manifest: &'static ModelAssetManifest, directory: PathBuf) -> Self {
        Self {
            manifest,
            directory,
        }
    }

    #[must_use]
    pub const fn manifest(&self) -> &'static ModelAssetManifest {
        self.manifest
    }

    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Resolves a declared required file by its manifest index.
    ///
    /// This is public primarily for integration with `llama-server` startup
    /// specifications. Callers should use the named fields in
    /// [`ResolvedModelAssets::llama_cpp_paths`] for the default route.
    #[must_use]
    pub fn required_file_path(&self, index: usize) -> PathBuf {
        let required_file = self.manifest.required_files.get(index).unwrap_or_else(|| {
            panic!(
                "model asset {} has no required file at index {index}",
                self.manifest.id
            )
        });
        self.directory.join(required_file.relative_path)
    }

    fn check(&self) -> Vec<ModelAssetDiagnostic> {
        self.manifest
            .required_files
            .iter()
            .map(|required_file| self.check_file(*required_file))
            .filter_map(Result::err)
            .collect()
    }

    fn verify_integrity(&self) -> Vec<ModelAssetDiagnostic> {
        self.manifest
            .required_files
            .iter()
            .map(|required_file| self.verify_file(*required_file))
            .filter_map(Result::err)
            .collect()
    }

    fn check_file(&self, required_file: RequiredModelFile) -> Result<(), ModelAssetDiagnostic> {
        let path = self.directory.join(required_file.relative_path);
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(self.diagnostic(required_file, path, ModelAssetProblem::Missing));
            }
            Err(error) => {
                return Err(self.diagnostic(
                    required_file,
                    path,
                    ModelAssetProblem::MetadataUnavailable {
                        kind: error.kind(),
                        message: error.to_string(),
                    },
                ));
            }
        };

        if !metadata.is_file() {
            return Err(self.diagnostic(required_file, path, ModelAssetProblem::NotAFile));
        }

        if let Err(error) = fs::File::open(&path) {
            return Err(self.diagnostic(
                required_file,
                path,
                ModelAssetProblem::Unreadable {
                    kind: error.kind(),
                    message: error.to_string(),
                },
            ));
        }

        Ok(())
    }

    fn verify_file(&self, required_file: RequiredModelFile) -> Result<(), ModelAssetDiagnostic> {
        let path = self.directory.join(required_file.relative_path);
        let metadata = fs::metadata(&path).map_err(|error| {
            let problem = if error.kind() == io::ErrorKind::NotFound {
                ModelAssetProblem::Missing
            } else {
                ModelAssetProblem::MetadataUnavailable {
                    kind: error.kind(),
                    message: error.to_string(),
                }
            };
            self.diagnostic(required_file, path.clone(), problem)
        })?;
        if !metadata.is_file() {
            return Err(self.diagnostic(required_file, path, ModelAssetProblem::NotAFile));
        }
        if metadata.len() != required_file.bytes {
            return Err(self.diagnostic(
                required_file,
                path,
                ModelAssetProblem::SizeMismatch {
                    expected: required_file.bytes,
                    actual: metadata.len(),
                },
            ));
        }
        let actual = sha256_file(&path).map_err(|error| {
            self.diagnostic(
                required_file,
                path.clone(),
                ModelAssetProblem::Unreadable {
                    kind: error.kind(),
                    message: error.to_string(),
                },
            )
        })?;
        if !actual.eq_ignore_ascii_case(required_file.sha256) {
            return Err(self.diagnostic(
                required_file,
                path,
                ModelAssetProblem::HashMismatch {
                    expected: required_file.sha256.to_owned(),
                    actual,
                },
            ));
        }
        Ok(())
    }

    fn diagnostic(
        &self,
        required_file: RequiredModelFile,
        path: PathBuf,
        problem: ModelAssetProblem,
    ) -> ModelAssetDiagnostic {
        ModelAssetDiagnostic {
            asset_id: self.manifest.id,
            asset_label: self.manifest.label,
            required_file,
            path,
            problem,
        }
    }
}

/// Result of checking all declared assets before launching llama.cpp.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModelAssetsPreflight {
    diagnostics: Vec<ModelAssetDiagnostic>,
}

impl ModelAssetsPreflight {
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.diagnostics.is_empty()
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[ModelAssetDiagnostic] {
        &self.diagnostics
    }

    /// Turns a failed preflight into an error suitable for the backend's
    /// startup path while retaining every actionable problem.
    pub fn into_result(self) -> Result<(), ModelAssetsPreflightError> {
        if self.is_ready() {
            Ok(())
        } else {
            Err(ModelAssetsPreflightError {
                diagnostics: self.diagnostics,
            })
        }
    }
}

/// An individual problem found while validating a required asset file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelAssetDiagnostic {
    pub asset_id: ModelAssetId,
    pub asset_label: &'static str,
    pub required_file: RequiredModelFile,
    pub path: PathBuf,
    pub problem: ModelAssetProblem,
}

impl fmt::Display for ModelAssetDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} ({}) requires {} at {}: {}",
            self.asset_label,
            self.asset_id,
            self.required_file.purpose,
            self.path.display(),
            self.problem
        )
    }
}

/// Why an expected model file cannot be used.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelAssetProblem {
    Missing,
    NotAFile,
    MetadataUnavailable {
        kind: io::ErrorKind,
        message: String,
    },
    Unreadable {
        kind: io::ErrorKind,
        message: String,
    },
    SizeMismatch {
        expected: u64,
        actual: u64,
    },
    HashMismatch {
        expected: String,
        actual: String,
    },
}

impl fmt::Display for ModelAssetProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => formatter.write_str("file is missing"),
            Self::NotAFile => formatter.write_str("path exists but is not a regular file"),
            Self::MetadataUnavailable { message, .. } => {
                write!(formatter, "could not inspect path ({message})")
            }
            Self::Unreadable { message, .. } => {
                write!(formatter, "file cannot be opened ({message})")
            }
            Self::SizeMismatch { expected, actual } => {
                write!(
                    formatter,
                    "file size is {actual} bytes; expected {expected} bytes"
                )
            }
            Self::HashMismatch { expected, actual } => {
                write!(formatter, "SHA-256 is {actual}; expected {expected}")
            }
        }
    }
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    // Windows' default thread stack is commonly 1 MiB, so this transfer
    // buffer must live on the heap rather than making `verify` overflow before
    // it can hash the first model byte.
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Failed [`ModelAssetsPreflight`] with all missing or unusable files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelAssetsPreflightError {
    diagnostics: Vec<ModelAssetDiagnostic>,
}

impl ModelAssetsPreflightError {
    #[must_use]
    pub fn diagnostics(&self) -> &[ModelAssetDiagnostic] {
        &self.diagnostics
    }
}

impl fmt::Display for ModelAssetsPreflightError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("default GGUF assets are not ready:")?;
        for diagnostic in &self.diagnostics {
            write!(formatter, "\n- {diagnostic}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ModelAssetsPreflightError {}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    static NEXT_TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    fn temporary_project_root() -> PathBuf {
        let unique = format!(
            "xrtranslate-assets-test-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock must be after the Unix epoch")
                .as_nanos(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed),
        );
        std::env::temp_dir().join(unique)
    }

    #[test]
    fn static_manifest_declares_the_default_gguf_route() {
        assert_eq!(DEFAULT_GGUF_MANIFEST.len(), 2);
        assert_eq!(QWEN3_ASR_GGUF.required_files.len(), 2);
        assert_eq!(HUNYUAN_MT_GGUF.required_files.len(), 1);
        assert_eq!(
            manifest_for(ModelAssetId::Qwen3AsrGguf).provider,
            "qwen3-gguf"
        );
        assert_eq!(
            manifest_for(ModelAssetId::HunyuanMtGguf).source.repository,
            "tencent/Hy-MT2-1.8B-GGUF"
        );
        assert_eq!(
            QWEN3_ASR_GGUF
                .source
                .hugging_face_resolve_url("Qwen3-ASR-1.7B.Q4_K_M.gguf"),
            "https://huggingface.co/mradermacher/Qwen3-ASR-1.7B-GGUF/resolve/cc946c78d3804752f7ba1bc42720c0f7aaf3d1ad/Qwen3-ASR-1.7B.Q4_K_M.gguf"
        );
    }

    #[test]
    fn defaults_are_resolved_from_the_project_root() {
        let root = Path::new("release-root");
        let paths = ResolvedModelAssets::for_project_root(root).llama_cpp_paths();

        assert_eq!(
            paths.qwen3_asr_model,
            root.join("models")
                .join("Qwen3-ASR-1.7B-GGUF")
                .join("Qwen3-ASR-1.7B.Q4_K_M.gguf")
        );
        assert_eq!(
            paths.qwen3_asr_mmproj,
            root.join("models")
                .join("Qwen3-ASR-1.7B-GGUF")
                .join("Qwen3-ASR-1.7B.mmproj-f16.gguf")
        );
        assert_eq!(
            paths.hunyuan_mt_model,
            root.join("models")
                .join("HY-MT2")
                .join("Hy-MT2-1.8B-Q4_K_M.gguf")
        );
    }

    #[test]
    fn configuration_overrides_are_still_relative_to_project_root() {
        let config = ModelAssetsConfig {
            models_directory: Some(PathBuf::from("installed-models")),
            qwen3_asr_gguf_directory: Some(PathBuf::from("custom/qwen")),
            hunyuan_mt_gguf_directory: None,
        };
        let assets = config.resolve("release-root");

        assert_eq!(
            assets.models_directory,
            PathBuf::from("release-root/installed-models")
        );
        assert_eq!(
            assets.qwen3_asr.directory(),
            Path::new("release-root/custom/qwen")
        );
        assert_eq!(
            assets.hunyuan_mt.directory(),
            Path::new("release-root/installed-models/HY-MT2")
        );
    }

    #[test]
    fn preflight_reports_each_missing_file_with_its_expected_path() {
        let root = temporary_project_root();
        let assets = ResolvedModelAssets::for_project_root(&root);
        let preflight = assets.check();

        assert!(!preflight.is_ready());
        assert_eq!(preflight.diagnostics().len(), 3);
        assert!(preflight.diagnostics().iter().any(|diagnostic| {
            diagnostic.asset_id == ModelAssetId::Qwen3AsrGguf
                && diagnostic.problem == ModelAssetProblem::Missing
                && diagnostic.path.ends_with("Qwen3-ASR-1.7B.Q4_K_M.gguf")
        }));
        assert!(
            preflight
                .into_result()
                .unwrap_err()
                .to_string()
                .contains("default GGUF assets are not ready")
        );
    }

    #[test]
    fn preflight_accepts_files_and_rejects_a_directory_in_their_place() {
        let root = temporary_project_root();
        let assets = ResolvedModelAssets::for_project_root(&root);
        for asset in assets.iter() {
            fs::create_dir_all(asset.directory()).unwrap();
            for index in 0..asset.manifest().required_files.len() {
                fs::write(asset.required_file_path(index), b"fixture").unwrap();
            }
        }
        assert!(assets.check().is_ready());

        let mmproj = assets.qwen3_asr.required_file_path(1);
        fs::remove_file(&mmproj).unwrap();
        fs::create_dir(&mmproj).unwrap();
        let preflight = assets.check();

        assert_eq!(preflight.diagnostics().len(), 1);
        assert_eq!(
            preflight.diagnostics()[0].problem,
            ModelAssetProblem::NotAFile
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_integrity_verification_accepts_matching_files_and_reports_hash_tampering() {
        let root = temporary_project_root();
        let directory = root.join("staging");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("fixture.gguf");
        fs::write(&path, b"fixture").unwrap();
        let digest = Box::leak(sha256_file(&path).unwrap().into_boxed_str());
        let files = Box::leak(Box::new([RequiredModelFile {
            relative_path: "fixture.gguf",
            purpose: "test fixture",
            bytes: 7,
            sha256: digest,
        }]));
        let manifest = Box::leak(Box::new(ModelAssetManifest {
            id: ModelAssetId::Qwen3AsrGguf,
            label: "fixture",
            capability: ModelCapability::Asr,
            provider: "fixture",
            relative_directory: "fixture",
            required_files: files,
            source: ModelSource {
                repository: "fixture/repository",
                revision: "0000000000000000000000000000000000000000",
                include_patterns: &["fixture.gguf"],
            },
        }));
        let asset = ResolvedModelAsset::new(manifest, directory);
        assert!(asset.verify_integrity().is_empty());

        fs::write(asset.required_file_path(0), b"changed").unwrap();
        let problems = asset.verify_integrity();
        assert!(matches!(
            problems.as_slice(),
            [ModelAssetDiagnostic {
                problem: ModelAssetProblem::HashMismatch { .. },
                ..
            }]
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verified_staging_directory_is_promoted_without_overwriting_an_install() {
        let root = temporary_project_root();
        let staging = root.join("models").join(".staging-fixture");
        fs::create_dir_all(&staging).unwrap();
        let staged_file = staging.join("fixture.gguf");
        fs::write(&staged_file, b"fixture").unwrap();
        let digest = Box::leak(sha256_file(&staged_file).unwrap().into_boxed_str());
        let files = Box::leak(Box::new([RequiredModelFile {
            relative_path: "fixture.gguf",
            purpose: "test fixture",
            bytes: 7,
            sha256: digest,
        }]));
        let manifest = Box::leak(Box::new(ModelAssetManifest {
            id: ModelAssetId::HunyuanMtGguf,
            label: "fixture",
            capability: ModelCapability::Translation,
            provider: "fixture",
            relative_directory: "fixture",
            required_files: files,
            source: ModelSource {
                repository: "fixture/repository",
                revision: "0000000000000000000000000000000000000000",
                include_patterns: &["fixture.gguf"],
            },
        }));
        let target = ResolvedModelAsset::new(manifest, root.join("models").join("fixture"));

        let installed = install_verified_directory(&target, &staging).unwrap();
        assert_eq!(installed, target.directory());
        assert!(installed.join("fixture.gguf").is_file());
        assert!(!staging.exists());
        let second_staging = root.join("models").join("second-staging");
        fs::create_dir_all(&second_staging).unwrap();
        fs::write(second_staging.join("fixture.gguf"), b"fixture").unwrap();
        assert!(matches!(
            install_verified_directory(&target, &second_staging),
            Err(AtomicInstallError::DestinationExists(_))
        ));

        fs::remove_dir_all(root).unwrap();
    }
}
