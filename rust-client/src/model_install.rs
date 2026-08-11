//! Background native-model installation for the desktop client.
//!
//! The installer intentionally owns its own worker thread and Tokio runtime:
//! model downloads and SHA-256 verification can take minutes and must never
//! run in eframe's UI thread.

use crossbeam_channel::{Receiver, TryRecvError, unbounded};
use std::{path::PathBuf, thread};
use xrtranslate_assets::{
    DownloadProgress, ModelAssetId, ModelAssetsConfig, ModelCapability, ModelLevel,
    NativeModelInstaller, ResolvedModelAssets, manifest_for, manifests_for_capability,
};
use xrtranslate_config::AppConfig;

#[derive(Clone, Debug)]
pub enum NativeModelTaskState {
    Idle,
    Discovering,
    Detected {
        /// Packages whose expected files are already present. They still need
        /// SHA-256 verification before the backend may use them.
        present: Vec<ModelAssetId>,
        ready: Vec<ModelAssetId>,
    },
    Installing {
        asset_id: ModelAssetId,
        relative_path: Option<String>,
        downloaded_bytes: u64,
        total_bytes: u64,
    },
    Verifying,
    Installed {
        asset_id: ModelAssetId,
        directory: PathBuf,
    },
    Verified,
    Failed(String),
}

/// A model package exposed by the provider objects selected in `config.json`.
#[derive(Clone, Debug)]
pub struct NativeModelPackage {
    pub id: ModelAssetId,
    pub label: &'static str,
    pub download_bytes: u64,
    pub capability: ModelCapability,
    pub level: ModelLevel,
}

impl NativeModelTaskState {
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        matches!(
            self,
            Self::Discovering | Self::Installing { .. } | Self::Verifying
        )
    }
}

#[derive(Clone, Copy, Debug)]
enum NativeModelTask {
    Discover,
    Install(ModelAssetId),
    Verify,
}

#[derive(Debug)]
enum NativeModelTaskEvent {
    Progress(DownloadProgress),
    Finished(NativeModelTaskResult),
}

#[derive(Debug)]
enum NativeModelTaskResult {
    Detected {
        present: Vec<ModelAssetId>,
        ready: Vec<ModelAssetId>,
    },
    Installed {
        asset_id: ModelAssetId,
        directory: PathBuf,
    },
    Verified,
    Failed(String),
}

/// Coordinates at most one native model task. Results are polled by the UI,
/// while all filesystem checks, hashing, and network transfer stays on its
/// worker.
pub struct NativeModelTaskManager {
    state: NativeModelTaskState,
    events: Option<Receiver<NativeModelTaskEvent>>,
}

impl Default for NativeModelTaskManager {
    fn default() -> Self {
        Self {
            state: NativeModelTaskState::Idle,
            events: None,
        }
    }
}

impl NativeModelTaskManager {
    #[must_use]
    pub fn state(&self) -> &NativeModelTaskState {
        &self.state
    }

    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.state.is_busy()
    }

    pub fn install(&mut self, project_root: PathBuf, asset_id: ModelAssetId) -> Result<(), String> {
        self.start(project_root, NativeModelTask::Install(asset_id))
    }

    pub fn verify(&mut self, project_root: PathBuf) -> Result<(), String> {
        self.start(project_root, NativeModelTask::Verify)
    }

    /// Starts a one-time, background presence scan for the configured model
    /// packages. It never downloads or hashes; explicit verification remains
    /// available as a separate action.
    pub fn discover_existing(&mut self, project_root: PathBuf) -> Result<(), String> {
        self.start(project_root, NativeModelTask::Discover)
    }

    pub fn invalidate_discovery(&mut self) {
        if !self.is_busy() {
            self.state = NativeModelTaskState::Idle;
            self.events = None;
        }
    }

    #[must_use]
    pub fn needs_discovery(&self) -> bool {
        matches!(
            self.state,
            NativeModelTaskState::Idle | NativeModelTaskState::Installed { .. }
        )
    }

    #[must_use]
    pub fn is_model_ready(&self, asset_id: ModelAssetId) -> bool {
        match (&self.state, asset_id) {
            (NativeModelTaskState::Detected { ready, .. }, requested) => ready.contains(&requested),
            (
                NativeModelTaskState::Installed {
                    asset_id: installed,
                    ..
                },
                requested,
            ) => *installed == requested,
            (NativeModelTaskState::Verified, _) => true,
            _ => false,
        }
    }

    /// Returns true when all expected files for this package are present.
    /// This inexpensive preflight deliberately does not hash on the UI thread;
    /// callers should offer verification instead of another download.
    #[must_use]
    pub fn is_model_present(&self, asset_id: ModelAssetId) -> bool {
        match (&self.state, asset_id) {
            (NativeModelTaskState::Detected { present, .. }, requested) => {
                present.contains(&requested)
            }
            (
                NativeModelTaskState::Installed {
                    asset_id: installed,
                    ..
                },
                requested,
            ) => *installed == requested,
            (NativeModelTaskState::Verified, _) => true,
            _ => false,
        }
    }

    /// Applies completed worker events. Call this once every UI frame.
    pub fn poll(&mut self) {
        let Some(events) = &self.events else {
            return;
        };

        let mut finished = false;
        loop {
            match events.try_recv() {
                Ok(NativeModelTaskEvent::Progress(progress)) => {
                    self.state = NativeModelTaskState::Installing {
                        asset_id: progress.asset_id,
                        relative_path: Some(progress.relative_path.to_owned()),
                        downloaded_bytes: progress.downloaded_bytes,
                        total_bytes: progress.total_bytes,
                    };
                }
                Ok(NativeModelTaskEvent::Finished(result)) => {
                    self.state = match result {
                        NativeModelTaskResult::Detected { present, ready } => {
                            NativeModelTaskState::Detected { present, ready }
                        }
                        NativeModelTaskResult::Installed {
                            asset_id,
                            directory,
                        } => NativeModelTaskState::Installed {
                            asset_id,
                            directory,
                        },
                        NativeModelTaskResult::Verified => NativeModelTaskState::Verified,
                        NativeModelTaskResult::Failed(error) => NativeModelTaskState::Failed(error),
                    };
                    finished = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.state = NativeModelTaskState::Failed(
                        "The native model worker stopped before reporting a result.".into(),
                    );
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.events = None;
        }
    }

    fn start(&mut self, project_root: PathBuf, task: NativeModelTask) -> Result<(), String> {
        if self.is_busy() {
            return Err("A native model task is already running.".into());
        }

        let (event_tx, event_rx) = unbounded();
        thread::Builder::new()
            .name("native-model-installer".into())
            .spawn(move || run_task(project_root, task, event_tx))
            .map_err(|error| format!("Cannot start native model worker: {error}"))?;
        self.state = match task {
            NativeModelTask::Discover => NativeModelTaskState::Discovering,
            NativeModelTask::Install(asset_id) => NativeModelTaskState::Installing {
                asset_id,
                relative_path: None,
                downloaded_bytes: 0,
                total_bytes: 0,
            },
            NativeModelTask::Verify => NativeModelTaskState::Verifying,
        };
        self.events = Some(event_rx);
        Ok(())
    }
}

fn run_task(
    project_root: PathBuf,
    task: NativeModelTask,
    event_tx: crossbeam_channel::Sender<NativeModelTaskEvent>,
) {
    let result = match task {
        NativeModelTask::Discover => discover_models(project_root),
        NativeModelTask::Install(asset_id) => install_model(project_root, asset_id, &event_tx),
        NativeModelTask::Verify => verify_models(project_root),
    };
    let _ = event_tx.send(NativeModelTaskEvent::Finished(result));
}

fn discover_models(project_root: PathBuf) -> NativeModelTaskResult {
    match configured_model_packages(&project_root).and_then(|packages| {
        let assets = load_assets(&project_root)?;
        let presence = assets.check();
        let present = packages
            .iter()
            .filter(|package| {
                !presence
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.asset_id == package.id)
            })
            .map(|package| package.id)
            .collect::<Vec<_>>();
        Ok((present, Vec::new()))
    }) {
        Ok((present, ready)) => NativeModelTaskResult::Detected { present, ready },
        Err(error) => NativeModelTaskResult::Failed(error),
    }
}

fn install_model(
    project_root: PathBuf,
    asset_id: ModelAssetId,
    event_tx: &crossbeam_channel::Sender<NativeModelTaskEvent>,
) -> NativeModelTaskResult {
    let result = (|| -> Result<PathBuf, String> {
        let assets = load_assets(&project_root)?;
        let installer = NativeModelInstaller::new(assets).map_err(|error| error.to_string())?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("Cannot initialize model installer runtime: {error}"))?;
        let progress_tx = event_tx.clone();
        runtime
            .block_on(installer.install(asset_id, move |progress| {
                let _ = progress_tx.send(NativeModelTaskEvent::Progress(progress));
            }))
            .map_err(|error| error.to_string())
    })();

    match result {
        Ok(directory) => NativeModelTaskResult::Installed {
            asset_id,
            directory,
        },
        Err(error) => NativeModelTaskResult::Failed(error),
    }
}

fn verify_models(project_root: PathBuf) -> NativeModelTaskResult {
    match load_assets(&project_root) {
        Ok(assets) => {
            let preflight = assets.verify_integrity();
            if preflight.is_ready() {
                NativeModelTaskResult::Verified
            } else {
                NativeModelTaskResult::Failed(
                    preflight
                        .diagnostics()
                        .iter()
                        .map(|diagnostic| format!("- {diagnostic}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            }
        }
        Err(error) => NativeModelTaskResult::Failed(error),
    }
}

fn load_assets(project_root: &std::path::Path) -> Result<ResolvedModelAssets, String> {
    let config = load_config(project_root)?;
    let (asr_asset, translation_asset) = selected_asset_ids(&config)?;
    Ok(ModelAssetsConfig {
        models_directory: config.model_manager.models_directory,
        qwen3_asr_gguf_directory: config.model_manager.qwen3_asr_gguf_directory,
        hunyuan_mt_gguf_directory: config.model_manager.hunyuan_mt_gguf_directory,
        qwen3_asr_asset: Some(asr_asset),
        hunyuan_mt_asset: Some(translation_asset),
    }
    .resolve(project_root))
}

pub fn configured_model_packages(
    project_root: &std::path::Path,
) -> Result<Vec<NativeModelPackage>, String> {
    let config = load_config(project_root)?;
    let (asr_asset, translation_asset) = selected_asset_ids(&config)?;
    let assets = ModelAssetsConfig {
        models_directory: config.model_manager.models_directory.clone(),
        qwen3_asr_gguf_directory: config.model_manager.qwen3_asr_gguf_directory.clone(),
        hunyuan_mt_gguf_directory: config.model_manager.hunyuan_mt_gguf_directory.clone(),
        qwen3_asr_asset: Some(asr_asset),
        hunyuan_mt_asset: Some(translation_asset),
    }
    .resolve(project_root);
    config
        .active_native_model_assets()
        .into_iter()
        .map(|key| {
            let id = ModelAssetId::from_config_key(&key).ok_or_else(|| {
                format!("Unknown model_asset in the active provider configuration: {key}")
            })?;
            let manifest = assets.asset(id).manifest();
            Ok(package_from_manifest(manifest))
        })
        .collect()
}

/// Resolves one provider's declared `model_asset` without assuming that the
/// provider is currently selected. This lets the service-provider UI render
/// the same card action for active and previewed providers.
pub fn model_package_for_config_key(
    project_root: &std::path::Path,
    key: &str,
) -> Result<NativeModelPackage, String> {
    let config = load_config(project_root)?;
    let (asr_asset, translation_asset) = selected_asset_ids(&config)?;
    let assets = ModelAssetsConfig {
        models_directory: config.model_manager.models_directory,
        qwen3_asr_gguf_directory: config.model_manager.qwen3_asr_gguf_directory,
        hunyuan_mt_gguf_directory: config.model_manager.hunyuan_mt_gguf_directory,
        qwen3_asr_asset: Some(asr_asset),
        hunyuan_mt_asset: Some(translation_asset),
    }
    .resolve(project_root);
    let id = ModelAssetId::from_config_key(key)
        .ok_or_else(|| format!("Unknown model_asset in provider configuration: {key}"))?;
    let manifest = assets.asset(id).manifest();
    Ok(package_from_manifest(manifest))
}

fn package_from_manifest(manifest: &xrtranslate_assets::ModelAssetManifest) -> NativeModelPackage {
    NativeModelPackage {
        id: manifest.id,
        label: manifest.label,
        capability: manifest.capability,
        level: manifest.level,
        download_bytes: manifest.required_files.iter().map(|file| file.bytes).sum(),
    }
}

pub fn model_level_packages(capability: ModelCapability) -> Vec<NativeModelPackage> {
    manifests_for_capability(capability)
        .map(package_from_manifest)
        .collect()
}

pub fn set_model_level(
    project_root: &std::path::Path,
    capability: ModelCapability,
    level: ModelLevel,
) -> Result<(), String> {
    let manifest = manifests_for_capability(capability)
        .find(|manifest| manifest.level == level)
        .ok_or_else(|| format!("The selected model level is not available for {capability:?}."))?;
    let path = project_root.join("config.json");
    let contents = std::fs::read_to_string(&path)
        .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let mut document: serde_json::Value =
        serde_json::from_str(&contents).map_err(|error| format!("Invalid config.json: {error}"))?;
    let section_name = match capability {
        ModelCapability::Asr => "asr",
        ModelCapability::Translation => "translation",
    };
    let section = document
        .get_mut(section_name)
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| format!("Missing {section_name} configuration."))?;
    let provider = section
        .get("provider")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("Missing {section_name}.provider."))?
        .to_owned();
    let provider_config = section
        .get_mut("providers")
        .and_then(serde_json::Value::as_object_mut)
        .and_then(|providers| providers.get_mut(&provider))
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| format!("Missing {section_name}.providers.{provider}."))?;
    provider_config.insert(
        "model_asset".into(),
        serde_json::Value::String(manifest.id.as_str().into()),
    );
    let formatted = serde_json::to_string_pretty(&document)
        .map_err(|error| format!("Cannot serialize config.json: {error}"))?;
    xrtranslate_config::AppConfig::from_value(document)
        .map_err(|error| format!("Invalid configuration: {error}"))?;
    std::fs::write(&path, format!("{formatted}\n"))
        .map_err(|error| format!("Cannot save {}: {error}", path.display()))
}

fn selected_asset_ids(config: &AppConfig) -> Result<(ModelAssetId, ModelAssetId), String> {
    let mut asr = None;
    let mut translation = None;
    for key in config.active_native_model_assets() {
        let id = ModelAssetId::from_config_key(&key).ok_or_else(|| {
            format!("Unknown model_asset in active provider configuration: {key}")
        })?;
        match manifest_for(id).capability {
            ModelCapability::Asr => asr = Some(id),
            ModelCapability::Translation => translation = Some(id),
        }
    }
    Ok((
        asr.ok_or("The active ASR provider has no model package.")?,
        translation.ok_or("The active translation provider has no model package.")?,
    ))
}

fn load_config(project_root: &std::path::Path) -> Result<AppConfig, String> {
    let config_path = project_root.join("config.json");
    AppConfig::from_path(&config_path)
        .map_err(|error| format!("Cannot read {}: {error}", config_path.display()))
}
