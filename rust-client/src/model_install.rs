//! Background native-model installation for the desktop client.
//!
//! The installer intentionally owns its own worker thread and Tokio runtime:
//! model downloads and SHA-256 verification can take minutes and must never
//! run in eframe's UI thread.

use crossbeam_channel::{Receiver, TryRecvError, unbounded};
use std::{path::PathBuf, thread};
use xrtranslate_assets::{
    DownloadProgress, ModelAssetId, ModelAssetsConfig, ModelCapability, NativeModelInstaller,
    ResolvedModelAssets,
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
    #[allow(dead_code)]
    pub capability: ModelCapability,
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
    Ok(ModelAssetsConfig {
        models_directory: config.model_manager.models_directory,
        qwen3_asr_gguf_directory: config.model_manager.qwen3_asr_gguf_directory,
        hunyuan_mt_gguf_directory: config.model_manager.hunyuan_mt_gguf_directory,
    }
    .resolve(project_root))
}

pub fn configured_model_packages(
    project_root: &std::path::Path,
) -> Result<Vec<NativeModelPackage>, String> {
    let config = load_config(project_root)?;
    let assets = ModelAssetsConfig {
        models_directory: config.model_manager.models_directory.clone(),
        qwen3_asr_gguf_directory: config.model_manager.qwen3_asr_gguf_directory.clone(),
        hunyuan_mt_gguf_directory: config.model_manager.hunyuan_mt_gguf_directory.clone(),
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
            Ok(NativeModelPackage {
                id,
                label: manifest.label,
                capability: manifest.capability,
            })
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
    let assets = ModelAssetsConfig {
        models_directory: config.model_manager.models_directory,
        qwen3_asr_gguf_directory: config.model_manager.qwen3_asr_gguf_directory,
        hunyuan_mt_gguf_directory: config.model_manager.hunyuan_mt_gguf_directory,
    }
    .resolve(project_root);
    let id = ModelAssetId::from_config_key(key)
        .ok_or_else(|| format!("Unknown model_asset in provider configuration: {key}"))?;
    let manifest = assets.asset(id).manifest();
    Ok(NativeModelPackage {
        id,
        label: manifest.label,
        capability: manifest.capability,
    })
}

fn load_config(project_root: &std::path::Path) -> Result<AppConfig, String> {
    let config_path = project_root.join("config.json");
    AppConfig::from_path(&config_path)
        .map_err(|error| format!("Cannot read {}: {error}", config_path.display()))
}
