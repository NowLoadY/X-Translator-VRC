//! Native llama.cpp runtime discovery and installation.
//!
//! The configured `model_manager.llama_cpp.downloads` list is the contract:
//! we select CUDA when the installed NVIDIA driver reports a compatible
//! runtime and adequate compute capability, and otherwise select the portable
//! CPU build.

use crossbeam_channel::{Receiver, TryRecvError, unbounded};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
};
use xrtranslate_config::{
    AppConfig, LlamaCppArchiveFormat, LlamaCppAssetKind, LlamaCppRuntimeConfig, RuntimeLayout,
};
use xrtranslate_download::{DownloadClient, DownloadSpec};

const MIN_CUDA_COMPUTE_CAPABILITY: (u16, u16) = (6, 0);
const TURING_COMPUTE_CAPABILITY: (u16, u16) = (7, 5);
const BLACKWELL_MINIMUM_CUDA: (u16, u16) = (12, 8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeBackend {
    Cpu,
    Cuda,
}

#[derive(Clone, Debug)]
struct RuntimeSelection {
    assets: Vec<ReleaseAsset>,
    backend: RuntimeBackend,
    executable: String,
    required_files: Vec<String>,
    required_file_prefixes: Vec<String>,
}

impl RuntimeSelection {
    fn total_bytes(&self) -> u64 {
        self.assets.iter().map(|asset| asset.size).sum()
    }
}

impl RuntimeBackend {
    const fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Cuda => "CUDA",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NvidiaCuda {
    gpu: String,
    compute_capability: (u16, u16),
    driver_cuda: String,
}

#[derive(Clone, Debug)]
pub enum RuntimeInstallState {
    Idle,
    Detecting,
    Ready,
    Downloading {
        asset: String,
        downloaded: u64,
        total: u64,
    },
    Extracting,
    Installed(PathBuf),
    Failed(String),
}

impl RuntimeInstallState {
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        matches!(
            self,
            Self::Detecting | Self::Downloading { .. } | Self::Extracting
        )
    }
}

#[derive(Debug)]
enum Event {
    Prepared(Result<RuntimeSelection, String>),
    Downloading {
        asset: String,
        downloaded: u64,
        total: u64,
    },
    Extracting,
    Finished(Result<PathBuf, String>),
}

/// One background worker for the optional automatic llama.cpp installer.
pub struct RuntimeInstaller {
    state: RuntimeInstallState,
    events: Option<Receiver<Event>>,
    selection: Option<RuntimeSelection>,
    proxy_url: Option<String>,
}

impl Default for RuntimeInstaller {
    fn default() -> Self {
        Self {
            state: RuntimeInstallState::Idle,
            events: None,
            selection: None,
            proxy_url: None,
        }
    }
}

impl RuntimeInstaller {
    pub fn set_proxy_url(&mut self, proxy_url: &str) {
        self.proxy_url = (!proxy_url.trim().is_empty()).then(|| proxy_url.trim().to_owned());
    }
    #[must_use]
    pub fn state(&self) -> &RuntimeInstallState {
        &self.state
    }

    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.state.is_busy()
    }

    #[must_use]
    pub fn download_size_bytes(&self) -> Option<u64> {
        self.selection.as_ref().map(RuntimeSelection::total_bytes)
    }

    #[must_use]
    pub fn backend_label(&self) -> Option<&'static str> {
        self.selection
            .as_ref()
            .map(|selection| selection.backend.label())
    }

    pub fn prepare_recommended(&mut self, project_root: PathBuf) -> Result<(), String> {
        if !matches!(self.state, RuntimeInstallState::Idle) {
            return Ok(());
        }
        let (sender, receiver) = unbounded();
        thread::Builder::new()
            .name("llama-cpp-planner".into())
            .spawn(move || {
                let result = configured_release_assets(&project_root)
                    .and_then(|assets| select_assets(&assets));
                let _ = sender.send(Event::Prepared(result));
            })
            .map_err(|error| format!("Cannot start llama.cpp planner: {error}"))?;
        self.state = RuntimeInstallState::Detecting;
        self.events = Some(receiver);
        Ok(())
    }

    pub fn install_recommended(&mut self, project_root: PathBuf) -> Result<(), String> {
        if self.is_busy() {
            return Err("A llama.cpp installation is already running.".into());
        }
        let selection = self.selection.clone().ok_or_else(|| {
            "The llama.cpp download plan is not ready. Wait for hardware detection to finish."
                .to_owned()
        })?;
        let (sender, receiver) = unbounded();
        let proxy_url = self.proxy_url.clone();
        thread::Builder::new()
            .name("llama-cpp-installer".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| format!("Cannot create download runtime: {error}"))
                    .and_then(|runtime| {
                        runtime.block_on(install(
                            project_root,
                            selection,
                            sender.clone(),
                            proxy_url.as_deref(),
                        ))
                    });
                let _ = sender.send(Event::Finished(result));
            })
            .map_err(|error| format!("Cannot start llama.cpp installer: {error}"))?;
        self.state = RuntimeInstallState::Downloading {
            asset: String::new(),
            downloaded: 0,
            total: self.download_size_bytes().unwrap_or(0),
        };
        self.events = Some(receiver);
        Ok(())
    }

    pub fn poll(&mut self) {
        let Some(events) = &self.events else {
            return;
        };
        let mut finished = false;
        loop {
            match events.try_recv() {
                Ok(Event::Prepared(result)) => {
                    match result {
                        Ok(selection) => {
                            self.state = RuntimeInstallState::Ready;
                            self.selection = Some(selection);
                        }
                        Err(error) => self.state = RuntimeInstallState::Failed(error),
                    }
                    finished = true;
                    break;
                }
                Ok(Event::Downloading {
                    asset,
                    downloaded,
                    total,
                }) => {
                    self.state = RuntimeInstallState::Downloading {
                        asset,
                        downloaded,
                        total,
                    };
                }
                Ok(Event::Extracting) => self.state = RuntimeInstallState::Extracting,
                Ok(Event::Finished(result)) => {
                    self.state = match result {
                        Ok(path) => RuntimeInstallState::Installed(path),
                        Err(error) => RuntimeInstallState::Failed(error),
                    };
                    finished = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.state = RuntimeInstallState::Failed(
                        "The llama.cpp installer stopped unexpectedly.".into(),
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
}

#[derive(Clone, Debug)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    sha256: String,
    archive_format: LlamaCppArchiveFormat,
    kind: LlamaCppAssetKind,
    target: String,
    cuda_version: Option<String>,
    executable: String,
    required_files: Vec<String>,
    required_file_prefixes: Vec<String>,
}

async fn install(
    project_root: PathBuf,
    selection: RuntimeSelection,
    sender: crossbeam_channel::Sender<Event>,
    proxy_url: Option<&str>,
) -> Result<PathBuf, String> {
    let executable_name = selection.executable.clone();
    let layout = RuntimeLayout::for_project_root(&project_root);
    let target = layout.llama_cpp_directory();
    let executable = target.join(&executable_name);
    if executable.is_file() {
        validate_runtime_files(
            &target,
            &selection.executable,
            &selection.required_files,
            &selection.required_file_prefixes,
        )?;
        return crate::backend::BackendManager::persist_llama_server_path(
            &project_root,
            &executable,
        );
    }
    if target.exists() {
        return Err(format!(
            "{} already exists but does not contain {}. Choose a manual path or remove that incomplete runtime folder.",
            target.display(),
            executable_name
        ));
    }

    let client = DownloadClient::with_proxy("XRTranslate runtime installer", proxy_url)
        .map_err(|error| error.to_string())?;
    let release = load_runtime_config(&project_root)?.release;
    let runtime_root = project_root.join("runtime");
    let staging = runtime_root.join(format!(".llama.cpp-{release}-staging"));
    prune_obsolete_runtime_staging(&runtime_root, &staging)?;
    let downloads = staging.join("downloads");
    let payload = staging.join("payload");
    fs::create_dir_all(&downloads)
        .map_err(|error| format!("Cannot create runtime staging folder: {error}"))?;
    let total = selection.total_bytes();
    let mut completed = 0_u64;
    for asset in &selection.assets {
        let archive = downloads.join(&asset.name);
        client
            .download_to(
                DownloadSpec::verified(
                    &asset.name,
                    &asset.browser_download_url,
                    asset.size,
                    &asset.sha256,
                ),
                &archive,
                |progress| {
                    let _ = sender.send(Event::Downloading {
                        asset: asset.name.clone(),
                        downloaded: completed.saturating_add(progress.downloaded_bytes),
                        total,
                    });
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        completed = completed.saturating_add(asset.size);
    }
    let _ = sender.send(Event::Extracting);
    if payload.exists() {
        fs::remove_dir_all(&payload)
            .map_err(|error| format!("Cannot reset runtime extraction folder: {error}"))?;
    }
    fs::create_dir_all(&payload)
        .map_err(|error| format!("Cannot create runtime extraction folder: {error}"))?;
    for asset in &selection.assets {
        extract_archive(&downloads.join(&asset.name), &payload, asset.archive_format)?;
    }
    let staged_executable = payload.join(&executable_name);
    if !staged_executable.is_file() {
        return Err(format!(
            "The selected llama.cpp release did not contain {}.",
            executable_name
        ));
    }
    make_executable(&staged_executable)?;
    validate_runtime_files(
        &payload,
        &selection.executable,
        &selection.required_files,
        &selection.required_file_prefixes,
    )?;
    fs::rename(&payload, &target)
        .map_err(|error| format!("Cannot activate llama.cpp runtime: {error}"))?;
    let _ = fs::remove_dir_all(&staging);
    crate::backend::BackendManager::persist_llama_server_path(&project_root, &executable)
}

fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|error| format!("Cannot inspect {}: {error}", path.display()))?
            .permissions();
        permissions.set_mode(permissions.mode() | 0o111);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("Cannot mark {} executable: {error}", path.display()))?;
    }
    Ok(())
}

fn prune_obsolete_runtime_staging(runtime_root: &Path, current: &Path) -> Result<(), String> {
    let entries = match fs::read_dir(runtime_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Cannot inspect runtime staging folders: {error}")),
    };
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Cannot inspect runtime staging entry: {error}"))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path != current
            && entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
            && name.starts_with(".llama.cpp-")
            && name.ends_with("-staging")
        {
            fs::remove_dir_all(&path).map_err(|error| {
                format!(
                    "Cannot remove obsolete runtime staging {}: {error}",
                    path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn extract_archive(
    archive: &Path,
    destination: &Path,
    format: LlamaCppArchiveFormat,
) -> Result<(), String> {
    match format {
        LlamaCppArchiveFormat::Zip => extract_zip(archive, destination),
        LlamaCppArchiveFormat::TarGz => extract_tar_gz(archive, destination),
    }
}

fn safe_archive_path(destination: &Path, name: &Path) -> Result<PathBuf, String> {
    use std::path::Component;
    if name.is_absolute()
        || name.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        })
    {
        return Err(format!(
            "archive entry escapes extraction directory: {}",
            name.display()
        ));
    }
    Ok(destination.join(name))
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(archive)
        .map_err(|error| format!("Cannot open {}: {error}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|error| format!("Invalid archive {}: {error}", archive.display()))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| format!("Cannot read archive entry: {error}"))?;
        let name = entry.enclosed_name().ok_or_else(|| {
            format!(
                "archive entry escapes extraction directory: {}",
                entry.name()
            )
        })?;
        let output = safe_archive_path(destination, &name)?;
        if entry.is_dir() {
            fs::create_dir_all(&output)
                .map_err(|error| format!("Cannot create {}: {error}", output.display()))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
        }
        let mut file = fs::File::create(&output)
            .map_err(|error| format!("Cannot create {}: {error}", output.display()))?;
        std::io::copy(&mut entry, &mut file)
            .map_err(|error| format!("Cannot extract {}: {error}", output.display()))?;
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(archive)
        .map_err(|error| format!("Cannot open {}: {error}", archive.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive
        .entries()
        .map_err(|error| format!("Invalid tar.gz archive: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("Cannot read tar entry: {error}"))?;
        let name = entry
            .path()
            .map_err(|error| format!("Cannot read tar entry path: {error}"))?
            .into_owned();
        let output = safe_archive_path(destination, &name)?;
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&output)
                .map_err(|error| format!("Cannot create {}: {error}", output.display()))?;
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err(format!("unsupported tar entry type: {}", name.display()));
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
        }
        let mut file = fs::File::create(&output)
            .map_err(|error| format!("Cannot create {}: {error}", output.display()))?;
        std::io::copy(&mut entry, &mut file)
            .map_err(|error| format!("Cannot extract {}: {error}", output.display()))?;
        #[cfg(unix)]
        if let Ok(mode) = entry.header().mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output, fs::Permissions::from_mode(mode)).map_err(|error| {
                format!(
                    "Cannot restore permissions for {}: {error}",
                    output.display()
                )
            })?;
        }
    }
    Ok(())
}

/// Returns the configured manual-download page. The UI deliberately reads
/// this from `config.json` too, so the page and automatic assets stay aligned.
pub(crate) fn configured_release_page(project_root: &Path) -> Result<String, String> {
    let config = load_runtime_config(project_root)?;
    let page = config.release_page.trim();
    if page.is_empty() {
        return Err("model_manager.llama_cpp.release_page is empty in config.json.".into());
    }
    if !page.starts_with("https://") {
        return Err("model_manager.llama_cpp.release_page must be an HTTPS URL.".into());
    }
    Ok(page.into())
}

fn configured_release_assets(project_root: &Path) -> Result<Vec<ReleaseAsset>, String> {
    let config = load_runtime_config(project_root)?;
    release_assets_from_config(&config)
}

fn load_runtime_config(project_root: &Path) -> Result<LlamaCppRuntimeConfig, String> {
    let path = project_root.join("config.json");
    AppConfig::from_path(&path)
        .map(|config| config.model_manager.llama_cpp)
        .map_err(|error| {
            format!(
                "Cannot read llama.cpp download configuration from {}: {error}",
                path.display()
            )
        })
}

fn release_assets_from_config(config: &LlamaCppRuntimeConfig) -> Result<Vec<ReleaseAsset>, String> {
    if config.release.trim().is_empty() {
        return Err("model_manager.llama_cpp.release is empty in config.json.".into());
    }
    if config.downloads.is_empty() {
        return Err("model_manager.llama_cpp.downloads is empty in config.json.".into());
    }

    let mut names = HashSet::new();
    config
        .downloads
        .iter()
        .map(|download| {
            let name = download.name.trim();
            let url = download.url.trim();
            if name.is_empty()
                || (download.archive_format == LlamaCppArchiveFormat::Zip
                    && !name.ends_with(".zip"))
                || (download.archive_format == LlamaCppArchiveFormat::TarGz
                    && !name.ends_with(".tar.gz"))
            {
                return Err(format!(
                    "model_manager.llama_cpp.downloads contains an archive name incompatible with its declared format: {:?}.",
                    download.name
                ));
            }
            if !names.insert(name.to_owned()) {
                return Err(format!(
                    "model_manager.llama_cpp.downloads contains duplicate archive {name:?}."
                ));
            }
            if !url.starts_with("https://") {
                return Err(format!(
                    "model_manager.llama_cpp.downloads[{name}] must use an HTTPS URL."
                ));
            }
            if download.bytes == 0 {
                return Err(format!(
                    "model_manager.llama_cpp.downloads[{name}].bytes must be greater than zero."
                ));
            }
            let sha256 = download.sha256.trim();
            if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(format!(
                    "model_manager.llama_cpp.downloads[{name}].sha256 must be a 64-character hexadecimal digest."
                ));
            }
            let target = if download.target.trim().is_empty() {
                legacy_target_from_name(name)
            } else {
                download.target.trim().to_owned()
            };
            let (kind, cuda_version, executable, required_files, required_file_prefixes) =
                normalize_runtime_metadata(download, name, &target)?;
            Ok(ReleaseAsset {
                name: name.into(),
                browser_download_url: url.into(),
                size: download.bytes,
                sha256: sha256.to_ascii_lowercase(),
                archive_format: download.archive_format,
                kind,
                target,
                cuda_version,
                executable,
                required_files,
                required_file_prefixes,
            })
        })
        .collect()
}

fn select_assets(assets: &[ReleaseAsset]) -> Result<RuntimeSelection, String> {
    // Linux currently ships a verified portable CPU archive. Keep NVIDIA
    // probing tied to the Windows CUDA capability until Linux CUDA/Vulkan
    // packages are declared with their own runtime metadata.
    let nvidia = if cfg!(target_os = "windows") {
        supported_nvidia_cuda()?
    } else {
        None
    };
    select_assets_for_hardware(assets, nvidia.as_ref())
}

fn select_assets_for_hardware(
    assets: &[ReleaseAsset],
    nvidia: Option<&NvidiaCuda>,
) -> Result<RuntimeSelection, String> {
    let target = current_runtime_target();
    let assets: Vec<_> = assets
        .iter()
        .filter(|asset| asset.target == target)
        .cloned()
        .collect();
    if assets.is_empty() {
        return Err(format!(
            "no llama.cpp runtime assets are configured for {target}"
        ));
    }
    if let Some(nvidia) = nvidia {
        let supported = parse_version(&nvidia.driver_cuda).ok_or_else(|| {
            format!(
                "NVIDIA GPU {} reported an invalid CUDA version {:?}.",
                nvidia.gpu, nvidia.driver_cuda
            )
        })?;
        let minimum = minimum_cuda_for_compute_capability(nvidia.compute_capability);
        let runtime = best_cuda_asset(
            &assets,
            supported,
            minimum,
            nvidia.compute_capability,
        )
        .ok_or_else(|| {
            format!(
                "NVIDIA GPU {} (compute capability {}) requires CUDA {} or newer, and the driver supports up to CUDA {}, but the configured llama.cpp download list has no compatible CUDA package for {}. Update the driver, update config.json, or install llama.cpp manually.",
                nvidia.gpu,
                format_version(nvidia.compute_capability),
                format_version(minimum),
                nvidia.driver_cuda,
                target
            )
        })?;
        let cuda_version = runtime
            .cuda_version
            .as_deref()
            .ok_or_else(|| "selected CUDA asset has no CUDA version".to_owned())?;
        let cudart = assets
            .iter()
            .find(|asset| asset.kind == LlamaCppAssetKind::CudaRuntime && asset.cuda_version.as_deref() == Some(cuda_version))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "The configured llama.cpp download list is missing the CUDA runtime package for version {cuda_version}; refusing to create an incomplete GPU installation."
                )
            })?;
        let executable = runtime.executable.clone();
        let required_files = runtime
            .required_files
            .iter()
            .chain(cudart.required_files.iter())
            .cloned()
            .collect();
        let required_file_prefixes = runtime
            .required_file_prefixes
            .iter()
            .chain(cudart.required_file_prefixes.iter())
            .cloned()
            .collect();
        return Ok(RuntimeSelection {
            assets: vec![runtime, cudart],
            backend: RuntimeBackend::Cuda,
            executable,
            required_files,
            required_file_prefixes,
        });
    }

    let runtime = assets
        .iter()
        .find(|asset| asset.kind == LlamaCppAssetKind::ServerCpu)
        .cloned()
        .ok_or_else(|| {
            format!("the configured llama.cpp download list has no CPU package for {target}")
        })?;
    let executable = runtime.executable.clone();
    let required_files = runtime.required_files.clone();
    let required_file_prefixes = runtime.required_file_prefixes.clone();
    Ok(RuntimeSelection {
        assets: vec![runtime],
        backend: RuntimeBackend::Cpu,
        executable,
        required_files,
        required_file_prefixes,
    })
}

/// Converts persisted runtime metadata into the installer representation.
/// The filename checks here are intentionally limited to legacy entries that
/// predate the declarative fields; new entries never use vendor filenames.
fn normalize_runtime_metadata(
    download: &xrtranslate_config::LlamaCppDownload,
    name: &str,
    target: &str,
) -> Result<
    (
        LlamaCppAssetKind,
        Option<String>,
        String,
        Vec<String>,
        Vec<String>,
    ),
    String,
> {
    let legacy = download.target.trim().is_empty();
    let legacy_cuda = legacy && name.contains("-cuda-");
    let legacy_cudart = legacy && name.contains("cudart");
    let kind = if legacy_cudart {
        LlamaCppAssetKind::CudaRuntime
    } else if legacy_cuda {
        LlamaCppAssetKind::ServerCuda
    } else {
        download.kind
    };
    let cuda_version = download.cuda_version.clone().or_else(|| {
        legacy_cuda
            .then_some(name)
            .and_then(|name| name.split("-cuda-").nth(1))
            .and_then(|version| version.split('-').next())
            .map(str::to_owned)
    });
    let executable = if download.executable.trim().is_empty() {
        if !legacy && kind != LlamaCppAssetKind::CudaRuntime {
            return Err(format!(
                "model_manager.llama_cpp.downloads[{name}].executable must be declared for new-format server assets."
            ));
        } else if target.starts_with("windows-") {
            "llama-server.exe".into()
        } else {
            "llama-server".into()
        }
    } else {
        download.executable.trim().to_owned()
    };
    let migrate_windows_requirements =
        download.required_files.is_empty() && target.starts_with("windows-");
    let required_files = if migrate_windows_requirements {
        match kind {
            LlamaCppAssetKind::ServerCpu => vec!["ggml.dll".into()],
            LlamaCppAssetKind::ServerCuda => vec!["ggml.dll".into(), "ggml-cuda.dll".into()],
            LlamaCppAssetKind::CudaRuntime => Vec::new(),
        }
    } else {
        download.required_files.clone()
    };
    let required_file_prefixes = if download.required_file_prefixes.is_empty()
        && target.starts_with("windows-")
        && kind == LlamaCppAssetKind::CudaRuntime
    {
        vec!["cudart64_".into(), "cublas64_".into(), "cublasLt64_".into()]
    } else {
        download.required_file_prefixes.clone()
    };
    Ok((
        kind,
        cuda_version,
        executable,
        required_files,
        required_file_prefixes,
    ))
}

fn best_cuda_asset(
    assets: &[ReleaseAsset],
    supported: (u16, u16),
    minimum: (u16, u16),
    compute_capability: (u16, u16),
) -> Option<ReleaseAsset> {
    assets
        .iter()
        .filter_map(|asset| {
            if asset.kind != LlamaCppAssetKind::ServerCuda {
                return None;
            }
            let version = asset.cuda_version.as_deref()?;
            let version = parse_version(version)?;
            (version >= minimum
                && version <= supported
                && cuda_supports_compute_capability(version, compute_capability))
            .then_some((version, asset.clone()))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, asset)| asset)
}

fn current_runtime_target() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn legacy_target_from_name(name: &str) -> String {
    if name.contains("-win-") {
        "windows-x86_64".into()
    } else {
        current_runtime_target()
    }
}

fn parse_version(value: &str) -> Option<(u16, u16)> {
    let mut parts = value.split('.');
    let version = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    parts.next().is_none().then_some(version)
}

fn cuda_supports_compute_capability(
    cuda_version: (u16, u16),
    compute_capability: (u16, u16),
) -> bool {
    cuda_version.0 < 13 || compute_capability >= TURING_COMPUTE_CAPABILITY
}

fn format_version(version: (u16, u16)) -> String {
    format!("{}.{}", version.0, version.1)
}

fn minimum_cuda_for_compute_capability(capability: (u16, u16)) -> (u16, u16) {
    if capability.0 >= 10 {
        BLACKWELL_MINIMUM_CUDA
    } else {
        (0, 0)
    }
}

fn supported_nvidia_cuda() -> Result<Option<NvidiaCuda>, String> {
    let Some((program, query)) = run_nvidia_smi(&[
        "--query-gpu=name,compute_cap",
        "--format=csv,noheader,nounits",
    ])?
    else {
        return Ok(None);
    };
    if !query.status.success() {
        return Err(command_failure("Cannot query NVIDIA GPUs", &query));
    }
    let mut gpus = parse_nvidia_gpu_rows(&String::from_utf8_lossy(&query.stdout))?;
    gpus.retain(|gpu| gpu.compute_capability >= MIN_CUDA_COMPUTE_CAPABILITY);
    let Some(mut selected) = gpus.into_iter().max_by_key(|gpu| gpu.compute_capability) else {
        return Ok(None);
    };

    let version_output = crate::child_process::hide_console(&mut Command::new(&program))
        .output()
        .map_err(|error| format!("Cannot run {}: {error}", program.display()))?;
    if !version_output.status.success() {
        return Err(command_failure(
            "Cannot query the NVIDIA driver CUDA version",
            &version_output,
        ));
    }
    let version_text = String::from_utf8_lossy(&version_output.stdout);
    selected.driver_cuda = cuda_version_from_nvidia_smi(&version_text).ok_or_else(|| {
        "nvidia-smi did not report a parseable CUDA Version or CUDA UMD Version; refusing to silently install the CPU runtime on an NVIDIA system.".to_owned()
    })?;
    Ok(Some(selected))
}

fn parse_nvidia_gpu_rows(output: &str) -> Result<Vec<NvidiaCuda>, String> {
    let mut gpus = Vec::new();
    let mut invalid = Vec::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let Some((gpu, capability)) = line.rsplit_once(',') else {
            invalid.push(line.to_owned());
            continue;
        };
        let Some(compute_capability) = parse_version(capability.trim()) else {
            invalid.push(line.to_owned());
            continue;
        };
        gpus.push(NvidiaCuda {
            gpu: gpu.trim().to_owned(),
            compute_capability,
            driver_cuda: String::new(),
        });
    }
    if gpus.is_empty() {
        let detail = if invalid.is_empty() {
            "nvidia-smi returned no GPU rows".to_owned()
        } else {
            format!("unparseable rows: {}", invalid.join(" | "))
        };
        Err(format!("Cannot identify an NVIDIA GPU ({detail})."))
    } else {
        Ok(gpus)
    }
}

fn run_nvidia_smi(args: &[&str]) -> Result<Option<(PathBuf, std::process::Output)>, String> {
    let mut candidates = Vec::new();
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        candidates.push(PathBuf::from(system_root).join("System32/nvidia-smi.exe"));
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates
            .push(PathBuf::from(program_files).join("NVIDIA Corporation/NVSMI/nvidia-smi.exe"));
    }
    candidates.push(PathBuf::from("nvidia-smi"));

    for program in candidates {
        match crate::child_process::hide_console(&mut Command::new(&program))
            .args(args)
            .output()
        {
            Ok(output) => return Ok(Some((program, output))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!("Cannot run {}: {error}", program.display()));
            }
        }
    }

    if windows_reports_nvidia_adapter() {
        Err("Windows reports an NVIDIA display adapter, but nvidia-smi could not be found. Reinstall or update the NVIDIA driver; the installer will not silently substitute a CPU runtime.".into())
    } else {
        Ok(None)
    }
}

fn windows_reports_nvidia_adapter() -> bool {
    if !cfg!(target_os = "windows") {
        return false;
    }
    crate::child_process::hide_console(&mut Command::new("reg"))
        .args([
            "query",
            "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}",
            "/s",
            "/v",
            "DriverDesc",
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout)
                .to_ascii_lowercase()
                .contains("nvidia")
        })
}

fn command_failure(context: &str, output: &std::process::Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if detail.is_empty() {
        format!("{context} (exit status {})", output.status)
    } else {
        format!("{context}: {detail}")
    }
}

fn cuda_version_from_nvidia_smi(version_text: &str) -> Option<String> {
    let version = ["CUDA Version: ", "CUDA UMD Version: "]
        .into_iter()
        .find_map(|marker| {
            let start = version_text.find(marker)? + marker.len();
            Some(
                version_text[start..]
                    .split_whitespace()
                    .next()?
                    .trim_end_matches('|'),
            )
        })?;
    parse_version(version)?;
    Some(version.to_owned())
}

fn validate_runtime_files(
    directory: &Path,
    executable: &str,
    required_files: &[String],
    required_file_prefixes: &[String],
) -> Result<(), String> {
    let executable_path = directory.join(executable);
    if !executable_path.is_file() {
        return Err(format!(
            "runtime executable is missing: {}",
            executable_path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if executable_path
            .metadata()
            .map_err(|error| error.to_string())?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            return Err(format!(
                "runtime executable is not executable: {}",
                executable_path.display()
            ));
        }
    }
    for required in required_files {
        if !directory.join(required).is_file() {
            return Err(format!("runtime is missing required file: {required}"));
        }
    }
    for prefix in required_file_prefixes {
        if !directory_contains_file_prefix(directory, prefix)? {
            return Err(format!(
                "runtime is missing a required file with prefix: {prefix}"
            ));
        }
    }
    Ok(())
}

fn directory_contains_file_prefix(directory: &Path, prefix: &str) -> Result<bool, String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("Cannot inspect {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "Cannot inspect an entry in {}: {error}",
                directory.display()
            )
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(prefix) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.into(),
            browser_download_url: "https://example.invalid/file.zip".into(),
            size: 1,
            sha256: "0".repeat(64),
            archive_format: LlamaCppArchiveFormat::Zip,
            kind: if name.contains("cudart") {
                LlamaCppAssetKind::CudaRuntime
            } else if name.contains("cuda") {
                LlamaCppAssetKind::ServerCuda
            } else {
                LlamaCppAssetKind::ServerCpu
            },
            target: current_runtime_target(),
            cuda_version: name
                .contains("cuda-12.4")
                .then(|| "12.4".into())
                .or_else(|| name.contains("cuda-13.3").then(|| "13.3".into())),
            executable: "llama-server.exe".into(),
            required_files: vec!["ggml.dll".into()],
            required_file_prefixes: Vec::new(),
        }
    }

    #[test]
    fn automatic_installer_uses_the_configured_download_urls() {
        let config = AppConfig::from_json_str(include_str!("../../config.json")).unwrap();
        let assets = release_assets_from_config(&config.model_manager.llama_cpp).unwrap();
        assert_eq!(assets.len(), 6);
        assert_eq!(config.model_manager.llama_cpp.release, "b10333");
        assert!(
            !config
                .model_manager
                .llama_cpp
                .release_page
                .ends_with("/latest")
        );
        for asset in assets {
            assert!(!asset.browser_download_url.contains("api.github.com"));
            assert!(!asset.browser_download_url.contains("/latest"));
            assert!(asset.browser_download_url.ends_with(&asset.name));
            assert!(asset.size > 0);
            assert_eq!(asset.sha256.len(), 64);
        }
        let linux = release_assets_from_config(&config.model_manager.llama_cpp)
            .unwrap()
            .into_iter()
            .find(|asset| asset.target == "linux-x86_64")
            .expect("verified Linux x86_64 runtime asset");
        assert_eq!(linux.archive_format, LlamaCppArchiveFormat::TarGz);
        assert_eq!(linux.executable, "llama-b10333/llama-server");
        assert_eq!(
            linux.sha256,
            "936ce04d98abe2a977e9dd2ff92659bb96947e136acee8f2bc3e21d8eaebbf23"
        );
    }

    #[test]
    fn configured_downloads_reject_duplicate_names_and_non_https_urls() {
        let config = LlamaCppRuntimeConfig {
            release: "test".into(),
            release_page: "https://example.invalid/releases/test".into(),
            downloads: vec![
                xrtranslate_config::LlamaCppDownload {
                    name: "llama-test-bin-win-cpu-x64.zip".into(),
                    url: "https://example.invalid/one.zip".into(),
                    bytes: 1,
                    sha256: "0".repeat(64),
                    ..Default::default()
                },
                xrtranslate_config::LlamaCppDownload {
                    name: "llama-test-bin-win-cpu-x64.zip".into(),
                    url: "http://example.invalid/two.zip".into(),
                    bytes: 1,
                    sha256: "0".repeat(64),
                    ..Default::default()
                },
            ],
        };
        let error = release_assets_from_config(&config).unwrap_err();
        assert!(error.contains("duplicate archive"));
    }

    #[test]
    fn selects_complete_cuda_runtime_for_blackwell() {
        let assets = vec![
            asset("llama-b1-bin-win-cpu-x64.zip"),
            asset("llama-b1-bin-win-cuda-12.4-x64.zip"),
            asset("cudart-llama-bin-win-cuda-12.4-x64.zip"),
            asset("llama-b1-bin-win-cuda-13.3-x64.zip"),
            asset("cudart-llama-bin-win-cuda-13.3-x64.zip"),
        ];
        let nvidia = NvidiaCuda {
            gpu: "NVIDIA GeForce RTX 5080".into(),
            compute_capability: (12, 0),
            driver_cuda: "13.3".into(),
        };
        let selected = select_assets_for_hardware(&assets, Some(&nvidia)).unwrap();
        assert_eq!(selected.backend, RuntimeBackend::Cuda);
        assert_eq!(
            selected
                .assets
                .iter()
                .map(|asset| asset.name.as_str())
                .collect::<Vec<_>>(),
            [
                "llama-b1-bin-win-cuda-13.3-x64.zip",
                "cudart-llama-bin-win-cuda-13.3-x64.zip"
            ]
        );
    }

    #[test]
    fn pre_turing_gpu_never_selects_cuda_13() {
        let assets = vec![
            asset("llama-b1-bin-win-cuda-12.4-x64.zip"),
            asset("cudart-llama-bin-win-cuda-12.4-x64.zip"),
            asset("llama-b1-bin-win-cuda-13.3-x64.zip"),
            asset("cudart-llama-bin-win-cuda-13.3-x64.zip"),
        ];
        for (gpu, compute_capability) in [
            ("NVIDIA GeForce GTX 1080", (6, 1)),
            ("NVIDIA TITAN V", (7, 0)),
        ] {
            let selected = select_assets_for_hardware(
                &assets,
                Some(&NvidiaCuda {
                    gpu: gpu.into(),
                    compute_capability,
                    driver_cuda: "13.3".into(),
                }),
            )
            .unwrap();
            assert_eq!(
                selected.assets[0].name,
                "llama-b1-bin-win-cuda-12.4-x64.zip"
            );
        }
    }

    #[test]
    fn turing_and_newer_can_select_cuda_13() {
        assert!(!cuda_supports_compute_capability((13, 3), (7, 0)));
        assert!(cuda_supports_compute_capability((13, 3), (7, 5)));
        assert!(cuda_supports_compute_capability((13, 3), (8, 9)));
    }

    #[test]
    fn blackwell_never_falls_back_to_incompatible_cuda_or_cpu() {
        let assets = vec![
            asset("llama-b1-bin-win-cpu-x64.zip"),
            asset("llama-b1-bin-win-cuda-12.4-x64.zip"),
            asset("cudart-llama-bin-win-cuda-12.4-x64.zip"),
            asset("llama-b1-bin-win-cuda-13.3-x64.zip"),
            asset("cudart-llama-bin-win-cuda-13.3-x64.zip"),
        ];
        let nvidia = NvidiaCuda {
            gpu: "NVIDIA GeForce RTX 5080".into(),
            compute_capability: (12, 0),
            driver_cuda: "12.8".into(),
        };
        let error = select_assets_for_hardware(&assets, Some(&nvidia))
            .expect_err("incompatible release must fail");
        assert!(error.contains("requires CUDA 12.8 or newer"));
        assert!(error.contains("driver supports up to CUDA 12.8"));
    }

    #[test]
    fn missing_cudart_is_an_error_instead_of_an_incomplete_install() {
        let assets = vec![asset("llama-b1-bin-win-cuda-13.3-x64.zip")];
        let nvidia = NvidiaCuda {
            gpu: "NVIDIA GeForce RTX 5080".into(),
            compute_capability: (12, 0),
            driver_cuda: "13.3".into(),
        };
        let error = select_assets_for_hardware(&assets, Some(&nvidia))
            .expect_err("missing cudart must fail");
        assert!(error.contains("CUDA runtime package for version 13.3"));
    }

    #[test]
    fn parses_all_nvidia_gpus_instead_of_only_the_first() {
        let gpus = parse_nvidia_gpu_rows(
            "Unavailable virtual adapter, N/A\nNVIDIA GeForce GTX 580, 2.0\nNVIDIA GeForce RTX 5080, 12.0\n",
        )
        .unwrap();
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[1].gpu, "NVIDIA GeForce RTX 5080");
        assert_eq!(gpus[1].compute_capability, (12, 0));
    }

    #[test]
    fn runtime_assets_use_declared_cuda_versions() {
        let assets = release_assets_from_config(&LlamaCppRuntimeConfig {
            release: "test".into(),
            downloads: vec![xrtranslate_config::LlamaCppDownload {
                name: "server.zip".into(),
                url: "https://example.invalid/server.zip".into(),
                archive_format: LlamaCppArchiveFormat::Zip,
                bytes: 1,
                sha256: "0".repeat(64),
                kind: LlamaCppAssetKind::ServerCuda,
                target: current_runtime_target(),
                cuda_version: Some("13.3".into()),
                executable: "llama-server".into(),
                required_files: vec!["libggml.so".into()],
                required_file_prefixes: Vec::new(),
            }],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(assets[0].cuda_version.as_deref(), Some("13.3"));
    }

    #[test]
    fn declared_tar_gz_format_is_preserved_without_filename_inference() {
        let assets = release_assets_from_config(&LlamaCppRuntimeConfig {
            release: "test".into(),
            downloads: vec![xrtranslate_config::LlamaCppDownload {
                name: "server.tar.gz".into(),
                url: "https://example.invalid/server.tar.gz".into(),
                bytes: 1,
                sha256: "0".repeat(64),
                archive_format: LlamaCppArchiveFormat::TarGz,
                target: current_runtime_target(),
                kind: LlamaCppAssetKind::ServerCpu,
                executable: "bin/llama-server".into(),
                required_files: vec!["lib/libggml.so".into()],
                ..Default::default()
            }],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(assets[0].archive_format, LlamaCppArchiveFormat::TarGz);
        assert_eq!(assets[0].kind, LlamaCppAssetKind::ServerCpu);
    }

    #[test]
    fn archive_paths_reject_parent_and_absolute_entries() {
        let root = Path::new("runtime/staging");
        assert!(safe_archive_path(root, Path::new("../escape")).is_err());
        assert!(safe_archive_path(root, Path::new("/absolute")).is_err());
        assert_eq!(
            safe_archive_path(root, Path::new("bin/server")).unwrap(),
            root.join("bin/server")
        );
    }

    #[test]
    fn reads_current_nvidia_smi_cuda_umd_output() {
        let output = "| NVIDIA-SMI 610.47  CUDA UMD Version: 13.3 |";
        assert_eq!(
            cuda_version_from_nvidia_smi(output).as_deref(),
            Some("13.3")
        );
    }
}
