//! Native llama.cpp runtime discovery and installation for Windows releases.
//!
//! The configured `model_manager.llama_cpp.downloads` list is the contract:
//! we select CUDA when the installed NVIDIA driver reports a compatible
//! runtime and adequate compute capability, and otherwise select the portable
//! CPU build.

use crossbeam_channel::{Receiver, TryRecvError, unbounded};
use futures::StreamExt;
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    thread,
};
use xrtranslate_config::{AppConfig, LlamaCppRuntimeConfig};

const MIN_CUDA_COMPUTE_CAPABILITY: (u16, u16) = (6, 0);
const BLACKWELL_MINIMUM_CUDA: (u16, u16) = (12, 8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeBackend {
    Cpu,
    Cuda,
}

struct RuntimeSelection {
    assets: Vec<ReleaseAsset>,
    backend: RuntimeBackend,
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
}

impl Default for RuntimeInstaller {
    fn default() -> Self {
        Self {
            state: RuntimeInstallState::Idle,
            events: None,
        }
    }
}

impl RuntimeInstaller {
    #[must_use]
    pub fn state(&self) -> &RuntimeInstallState {
        &self.state
    }

    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.state.is_busy()
    }

    pub fn install_recommended(&mut self, project_root: PathBuf) -> Result<(), String> {
        if self.is_busy() {
            return Err("A llama.cpp installation is already running.".into());
        }
        let (sender, receiver) = unbounded();
        thread::Builder::new()
            .name("llama-cpp-installer".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| format!("Cannot create download runtime: {error}"))
                    .and_then(|runtime| runtime.block_on(install(project_root, sender.clone())));
                let _ = sender.send(Event::Finished(result));
            })
            .map_err(|error| format!("Cannot start llama.cpp installer: {error}"))?;
        self.state = RuntimeInstallState::Detecting;
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
}

async fn install(
    project_root: PathBuf,
    sender: crossbeam_channel::Sender<Event>,
) -> Result<PathBuf, String> {
    if !cfg!(target_os = "windows") || std::env::consts::ARCH != "x86_64" {
        return Err("Automatic llama.cpp installation currently supports Windows x64 only.".into());
    }
    let target = project_root.join("runtime").join("llama.cpp");
    let executable = target.join("llama-server.exe");
    if executable.is_file() {
        return crate::backend::BackendManager::persist_llama_server_path(
            &project_root,
            &executable,
        );
    }
    if target.exists() {
        return Err(format!(
            "{} already exists but does not contain llama-server.exe. Choose a manual path or remove that incomplete runtime folder.",
            target.display()
        ));
    }

    let client = reqwest::Client::builder()
        .user_agent("XRTranslate runtime installer")
        .build()
        .map_err(|error| format!("Cannot create download client: {error}"))?;
    let assets = configured_release_assets(&project_root)?;
    let selection = select_assets(&assets)?;
    let staging = project_root.join("runtime").join(".llama.cpp-installing");
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|error| format!("Cannot clear old runtime staging folder: {error}"))?;
    }
    fs::create_dir_all(&staging)
        .map_err(|error| format!("Cannot create runtime staging folder: {error}"))?;
    for asset in selection.assets {
        let archive = staging.join(&asset.name);
        download(&client, &asset, &archive, &sender).await?;
        let _ = sender.send(Event::Extracting);
        extract_zip(&archive, &staging)?;
        fs::remove_file(&archive)
            .map_err(|error| format!("Cannot remove downloaded archive: {error}"))?;
    }
    let staged_executable = staging.join("llama-server.exe");
    if !staged_executable.is_file() {
        return Err("The selected llama.cpp release did not contain llama-server.exe.".into());
    }
    validate_runtime_files(&staging, selection.backend)?;
    fs::rename(&staging, &target)
        .map_err(|error| format!("Cannot activate llama.cpp runtime: {error}"))?;
    crate::backend::BackendManager::persist_llama_server_path(&project_root, &executable)
}

async fn download(
    client: &reqwest::Client,
    asset: &ReleaseAsset,
    output: &Path,
    sender: &crossbeam_channel::Sender<Event>,
) -> Result<(), String> {
    let response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(|error| format!("Cannot download {}: {error}", asset.name))?
        .error_for_status()
        .map_err(|error| format!("Download failed for {}: {error}", asset.name))?;
    let total = response.content_length().unwrap_or(asset.size);
    let mut file = fs::File::create(output)
        .map_err(|error| format!("Cannot create {}: {error}", output.display()))?;
    let mut downloaded = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| format!("Download interrupted for {}: {error}", asset.name))?;
        file.write_all(&chunk)
            .map_err(|error| format!("Cannot write {}: {error}", output.display()))?;
        downloaded += chunk.len() as u64;
        let _ = sender.send(Event::Downloading {
            asset: asset.name.clone(),
            downloaded,
            total,
        });
    }
    Ok(())
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
        let Some(name) = entry.enclosed_name().map(PathBuf::from) else {
            continue;
        };
        let output = destination.join(name);
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
            if name.is_empty() || !name.ends_with(".zip") {
                return Err(format!(
                    "model_manager.llama_cpp.downloads contains an invalid archive name {:?}.",
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
            Ok(ReleaseAsset {
                name: name.into(),
                browser_download_url: url.into(),
                // The response normally supplies Content-Length. A zero
                // fallback keeps the progress bar indeterminate if a proxy
                // strips that header.
                size: 0,
            })
        })
        .collect()
}

fn select_assets(assets: &[ReleaseAsset]) -> Result<RuntimeSelection, String> {
    select_assets_for_hardware(assets, supported_nvidia_cuda()?.as_ref())
}

fn select_assets_for_hardware(
    assets: &[ReleaseAsset],
    nvidia: Option<&NvidiaCuda>,
) -> Result<RuntimeSelection, String> {
    if let Some(nvidia) = nvidia {
        let supported = parse_version(&nvidia.driver_cuda).ok_or_else(|| {
            format!(
                "NVIDIA GPU {} reported an invalid CUDA version {:?}.",
                nvidia.gpu, nvidia.driver_cuda
            )
        })?;
        let minimum = minimum_cuda_for_compute_capability(nvidia.compute_capability);
        let runtime = best_cuda_asset(assets, supported, minimum).ok_or_else(|| {
            format!(
                "NVIDIA GPU {} (compute capability {}) requires CUDA {} or newer, and the driver supports up to CUDA {}, but the configured llama.cpp download list has no compatible Windows x64 CUDA package. Update the NVIDIA driver, update config.json, or install llama.cpp manually.",
                nvidia.gpu,
                format_version(nvidia.compute_capability),
                format_version(minimum),
                nvidia.driver_cuda
            )
        })?;
        let suffix = cuda_suffix(&runtime.name).expect("selected CUDA asset has suffix");
        let cudart_name = format!("cudart-llama-bin-win-cuda-{suffix}-x64.zip");
        let cudart = assets
            .iter()
            .find(|asset| asset.name == cudart_name)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "The configured llama.cpp download list is missing the required CUDA runtime package {cudart_name}; refusing to create an incomplete GPU installation."
                )
            })?;
        return Ok(RuntimeSelection {
            assets: vec![runtime, cudart],
            backend: RuntimeBackend::Cuda,
        });
    }

    let runtime = assets
        .iter()
        .find(|asset| asset.name.contains("-bin-win-cpu-x64.zip"))
        .cloned()
        .ok_or_else(|| {
            String::from("The configured llama.cpp download list has no Windows x64 CPU package.")
        })?;
    Ok(RuntimeSelection {
        assets: vec![runtime],
        backend: RuntimeBackend::Cpu,
    })
}

fn best_cuda_asset(
    assets: &[ReleaseAsset],
    supported: (u16, u16),
    minimum: (u16, u16),
) -> Option<ReleaseAsset> {
    assets
        .iter()
        .filter_map(|asset| {
            if !asset.name.starts_with("llama-") {
                return None;
            }
            let version = cuda_suffix(&asset.name)?;
            let version = parse_version(version)?;
            (version >= minimum && version <= supported).then_some((version, asset.clone()))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, asset)| asset)
}

fn cuda_suffix(name: &str) -> Option<&str> {
    let prefix = "-bin-win-cuda-";
    let suffix = "-x64.zip";
    let start = name.find(prefix)? + prefix.len();
    name.strip_suffix(suffix)?.get(start..)
}

fn parse_version(value: &str) -> Option<(u16, u16)> {
    let mut parts = value.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
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

    let version_output = Command::new(&program)
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
        match Command::new(&program).args(args).output() {
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
    Command::new("reg")
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

fn validate_runtime_files(directory: &Path, backend: RuntimeBackend) -> Result<(), String> {
    if !directory.join("ggml.dll").is_file() {
        return Err("The selected llama.cpp release is missing ggml.dll.".into());
    }
    if backend == RuntimeBackend::Cpu {
        return Ok(());
    }
    for (exact, prefix) in [
        (Some("ggml-cuda.dll"), None),
        (None, Some("cudart64_")),
        (None, Some("cublas64_")),
        (None, Some("cublasLt64_")),
    ] {
        let present = if let Some(exact) = exact {
            directory.join(exact).is_file()
        } else {
            directory_contains_dll_prefix(directory, prefix.expect("prefix checked"))?
        };
        if !present {
            let missing = exact
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{}*.dll", prefix.expect("prefix checked")));
            return Err(format!(
                "The CUDA llama.cpp installation is incomplete: missing {missing}"
            ));
        }
    }
    Ok(())
}

fn directory_contains_dll_prefix(directory: &Path, prefix: &str) -> Result<bool, String> {
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
        if name.starts_with(prefix) && name.ends_with(".dll") {
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
        }
    }

    #[test]
    fn automatic_installer_uses_the_configured_download_urls() {
        let config = AppConfig::from_json_str(include_str!("../../config.json")).unwrap();
        let assets = release_assets_from_config(&config.model_manager.llama_cpp).unwrap();
        assert_eq!(assets.len(), 5);
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
        }
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
                },
                xrtranslate_config::LlamaCppDownload {
                    name: "llama-test-bin-win-cpu-x64.zip".into(),
                    url: "http://example.invalid/two.zip".into(),
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
            .err()
            .expect("incompatible release must fail");
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
            .err()
            .expect("missing cudart must fail");
        assert!(error.contains("cudart-llama-bin-win-cuda-13.3-x64.zip"));
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
    fn parses_release_cuda_suffix() {
        assert_eq!(
            cuda_suffix("llama-b10330-bin-win-cuda-13.3-x64.zip"),
            Some("13.3")
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
