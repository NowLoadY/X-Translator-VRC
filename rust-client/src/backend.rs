use serde_json::Value;
use std::{
    net::{TcpStream, ToSocketAddrs},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};
use xrtranslate_assets::{ModelAssetId, ModelAssetsConfig};
use xrtranslate_config::AppConfig;

pub enum BackendStart {
    Ready,
    Starting,
}

pub enum BackendStatus {
    Ready,
    Starting,
    Failed(String),
}

/// Owns only the backend process tree started by this native client.
pub struct BackendManager {
    project_root: PathBuf,
    pub llama_server_path: String,
    child: Option<Child>,
    #[cfg(windows)]
    job: Option<KillOnCloseJob>,
}

impl BackendManager {
    pub fn load() -> Self {
        let project_root = project_root();
        let configured_path = load_llama_server_path(&project_root);
        let llama_server_path = preferred_llama_server_path(&project_root, &configured_path);
        let manager = Self {
            project_root,
            llama_server_path,
            child: None,
            #[cfg(windows)]
            job: None,
        };
        if manager.llama_server_path != configured_path
            && !manager.llama_server_path.trim().is_empty()
        {
            if let Err(error) =
                Self::write_llama_server_path(&manager.project_root, &manager.llama_server_path)
            {
                log::warn!("Cannot persist recovered llama-server path: {error}");
            }
        }
        manager
    }

    pub fn project_root(&self) -> PathBuf {
        self.project_root.clone()
    }

    pub fn llama_server_path_is_valid(&self) -> bool {
        let value = self.llama_server_path.trim();
        !value.is_empty() && absolute_from_project_root(&self.project_root, value.into()).is_file()
    }

    /// Stores the local llama.cpp executable where the Rust backend already
    /// expects it: `model_manager.llama_server_path` in `config.json`.
    pub fn save_llama_server_path(&mut self) -> Result<(), String> {
        let requested = self.llama_server_path.trim();
        if requested.is_empty() {
            return Err("llama-server path is empty".into());
        }
        let path = absolute_from_project_root(&self.project_root, PathBuf::from(requested));
        let persisted = Self::persist_llama_server_path(&self.project_root, &path)?;
        self.llama_server_path = persisted.display().to_string();
        Ok(())
    }

    /// Adopts an executable installed by the automatic runtime worker. The
    /// worker has already persisted it, so this only synchronizes live UI
    /// state with the durable configuration.
    pub fn adopt_installed_llama_server_path(&mut self, path: &std::path::Path) {
        self.llama_server_path = absolute_from_project_root(&self.project_root, path.into())
            .display()
            .to_string();
    }

    pub(crate) fn persist_llama_server_path(
        project_root: &std::path::Path,
        path: &std::path::Path,
    ) -> Result<PathBuf, String> {
        let path = absolute_from_project_root(project_root, path.into());
        if !path.is_file() {
            return Err(format!(
                "llama-server executable does not exist: {}",
                path.display()
            ));
        }
        let value = path.display().to_string();
        Self::write_llama_server_path(project_root, &value)?;
        Ok(path)
    }

    fn write_llama_server_path(project_root: &std::path::Path, value: &str) -> Result<(), String> {
        let config_path = project_root.join("config.json");
        let contents = std::fs::read_to_string(&config_path)
            .map_err(|error| format!("Cannot read {}: {error}", config_path.display()))?;
        let mut document: Value = serde_json::from_str(&contents)
            .map_err(|error| format!("Invalid config.json: {error}"))?;
        let root = document
            .as_object_mut()
            .ok_or("config.json root must be an object")?;
        let model_manager = root
            .entry("model_manager")
            .or_insert_with(|| Value::Object(serde_json::Map::new()))
            .as_object_mut()
            .ok_or("config.json model_manager must be an object")?;
        model_manager.insert(
            "llama_server_path".into(),
            Value::String(value.trim().into()),
        );
        let formatted = serde_json::to_string_pretty(&document)
            .map_err(|error| format!("Cannot serialize config.json: {error}"))?;
        std::fs::write(&config_path, format!("{formatted}\n"))
            .map_err(|error| format!("Cannot save {}: {error}", config_path.display()))
    }

    pub fn prepare(&mut self, server_url: &str) -> Result<BackendStart, String> {
        if server_reachable(server_url) {
            return Ok(BackendStart::Ready);
        }
        if !is_local_server(server_url) {
            return Err(format!(
                "Backend at {server_url} is unavailable. Automatic startup is only available for localhost."
            ));
        }
        if self.child.is_some() {
            return Ok(BackendStart::Starting);
        }
        self.start()?;
        Ok(BackendStart::Starting)
    }

    pub fn status(&mut self, server_url: &str) -> BackendStatus {
        if server_reachable(server_url) {
            return BackendStatus::Ready;
        }
        let Some(child) = &mut self.child else {
            return BackendStatus::Failed("Backend process is no longer running".into());
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                self.child = None;
                #[cfg(windows)]
                {
                    self.job = None;
                }
                let log = self.get_latest_log();
                let detail = if log.trim().is_empty() {
                    format!("Backend launcher exited before it became ready ({status})")
                } else {
                    format!(
                        "Backend launcher exited before it became ready ({status})\n\nLog Traceback:\n{}",
                        log.trim()
                    )
                };
                BackendStatus::Failed(detail)
            }
            Ok(None) => BackendStatus::Starting,
            Err(error) => BackendStatus::Failed(format!("Cannot inspect backend process: {error}")),
        }
    }

    pub fn get_latest_log(&self) -> String {
        let path = self
            .project_root
            .join("runtime")
            .join("logs")
            .join("backend_startup.log");
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    pub fn shutdown(&mut self) {
        #[cfg(windows)]
        if let Some(job) = self.job.take() {
            job.terminate();
            self.child = None;
        }

        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Checks the declared model package from the shared, version-pinned Rust
    /// manifest without invoking an external downloader.
    pub fn check_model_files(&self, category: &str, provider: &str) -> Result<String, String> {
        let id = match (category, provider) {
            ("asr", "qwen3-gguf") => ModelAssetId::Qwen3AsrGguf,
            ("translation", "hunyuan") | ("translation", "hy-mt2") => ModelAssetId::HunyuanMtGguf,
            _ => {
                return Err(format!(
                    "The native backend currently supports local model checks for qwen3-gguf ASR and hunyuan translation, not {category}:{provider}."
                ));
            }
        };
        let config_path = self.project_root.join("config.json");
        let config = AppConfig::from_path(&config_path)
            .map_err(|error| format!("Cannot read {}: {error}", config_path.display()))?;
        let assets = ModelAssetsConfig {
            models_directory: config.model_manager.models_directory,
            qwen3_asr_gguf_directory: config.model_manager.qwen3_asr_gguf_directory,
            hunyuan_mt_gguf_directory: config.model_manager.hunyuan_mt_gguf_directory,
        }
        .resolve(&self.project_root);
        let preflight = assets.check();
        let diagnostics = preflight
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.asset_id == id)
            .collect::<Vec<_>>();
        if diagnostics.is_empty() {
            Ok("Required native model files are ready. Use the native Verify command before a release to validate SHA-256.".into())
        } else {
            Err(format!(
                "Missing native model files:\n{}",
                diagnostics
                    .iter()
                    .map(|diagnostic| format!("- {diagnostic}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
        }
    }

    fn start(&mut self) -> Result<(), String> {
        // Revalidate and persist immediately before the backend reads
        // config.json. This also retries a recovery write that may have failed
        // transiently during application startup.
        self.save_llama_server_path()?;
        let child = self
            .native_backend_command_with_log("backend_startup.log")?
            .arg("--config")
            .arg(self.project_root.join("config.json"))
            .arg("--manage-llama-servers")
            .spawn()
            .map_err(|error| format!("Cannot start backend: {error}"))?;

        #[cfg(windows)]
        {
            let job = KillOnCloseJob::new()?;
            if let Err(error) = job.assign(&child) {
                let mut child = child;
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            self.job = Some(job);
            self.child = Some(child);
        }
        #[cfg(not(windows))]
        {
            self.child = Some(child);
        }
        Ok(())
    }

    fn native_backend_command_with_log(&self, log_filename: &str) -> Result<Command, String> {
        let executable = self.resolve_native_backend_executable()?;
        let mut command = Command::new(executable);
        command.current_dir(&self.project_root).stdin(Stdio::null());

        let log_dir = self.project_root.join("runtime").join("logs");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_file = log_dir.join(log_filename);

        let print_to_console = std::env::var_os("XRTRANSLATE_BACKEND_CONSOLE_LOG").is_some();
        if print_to_console {
            command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        } else if let Ok(file) = std::fs::File::create(&log_file) {
            if let Ok(stderr_file) = file.try_clone() {
                command.stdout(file).stderr(stderr_file);
            } else {
                command.stdout(Stdio::null()).stderr(Stdio::null());
            }
        } else {
            command.stdout(Stdio::null()).stderr(Stdio::null());
        }

        // The desktop client must never allocate a Windows Terminal/conhost
        // window when it starts the managed backend. Debug output can still
        // be inherited when a console already exists, but this flag prevents
        // Windows from creating a new one for the child process.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        Ok(command)
    }

    fn resolve_native_backend_executable(&self) -> Result<PathBuf, String> {
        let executable = if cfg!(windows) {
            "xrtranslate-backend.exe"
        } else {
            "xrtranslate-backend"
        };
        let debug_binary = self
            .project_root
            .join("target")
            .join("debug")
            .join(executable);
        let release_binary = self
            .project_root
            .join("target")
            .join("release")
            .join(executable);
        let packaged_binary = self.project_root.join("bin").join(executable);
        let candidates = if cfg!(debug_assertions) {
            [
                self.project_root.join(executable),
                packaged_binary.clone(),
                self.project_root.join("backend").join(executable),
                debug_binary,
                release_binary,
            ]
        } else {
            [
                self.project_root.join(executable),
                packaged_binary,
                self.project_root.join("backend").join(executable),
                release_binary,
                debug_binary,
            ]
        };
        candidates.iter().find(|path| path.is_file()).cloned().ok_or_else(|| {
            format!(
                "Native backend executable was not found. Build xrtranslate-backend or use the packaged application. Looked for:\n{}",
                candidates
                    .iter()
                    .map(|path| format!("- {}", path.display()))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
    }
}

impl Drop for BackendManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn project_root() -> PathBuf {
    for start in [std::env::current_dir().ok(), std::env::current_exe().ok()] {
        let Some(start) = start else {
            continue;
        };
        let directory = if start.is_dir() {
            start
        } else {
            start.parent().map(PathBuf::from).unwrap_or(start)
        };
        for ancestor in directory.ancestors() {
            if ancestor.join("config.json").exists()
                && (ancestor.join("Cargo.toml").exists()
                    || ancestor.join("xrtranslate-backend.exe").exists()
                    || ancestor
                        .join("bin")
                        .join("xrtranslate-backend.exe")
                        .exists()
                    || ancestor.join("xrtranslate-backend").exists()
                    || ancestor
                        .join("bin")
                        .join("xrtranslate-backend")
                        .exists())
            {
                return ancestor.into();
            }
        }
    }
    PathBuf::from(".")
}

fn absolute_from_project_root(project_root: &std::path::Path, path: PathBuf) -> PathBuf {
    let candidate = if path.is_absolute() {
        path
    } else {
        project_root.join(path)
    };
    std::path::absolute(&candidate).unwrap_or(candidate)
}

fn preferred_llama_server_path(project_root: &std::path::Path, configured: &str) -> String {
    let configured = configured.trim();
    if !configured.is_empty() {
        let candidate = absolute_from_project_root(project_root, configured.into());
        if candidate.is_file() {
            return candidate.display().to_string();
        }
    }

    let installed = absolute_from_project_root(
        project_root,
        PathBuf::from("runtime")
            .join("llama.cpp")
            .join("llama-server.exe"),
    );
    if installed.is_file() {
        installed.display().to_string()
    } else {
        configured.to_owned()
    }
}

fn load_llama_server_path(project_root: &std::path::Path) -> String {
    AppConfig::from_path(project_root.join("config.json"))
        .map(|config| config.model_manager.llama_server_path)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "xrtranslate-backend-{label}-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn create_server(root: &std::path::Path) -> PathBuf {
        let server = root
            .join("runtime")
            .join("llama.cpp")
            .join("llama-server.exe");
        std::fs::create_dir_all(server.parent().unwrap()).unwrap();
        std::fs::write(&server, b"test").unwrap();
        server
    }

    #[test]
    fn relative_configured_runtime_is_resolved_from_project_root() {
        let root = temp_root("relative");
        let server = create_server(&root);
        let selected = preferred_llama_server_path(&root, "runtime/llama.cpp/llama-server.exe");
        assert_eq!(PathBuf::from(selected), server);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn standard_runtime_is_recovered_when_config_is_empty_or_stale() {
        let root = temp_root("recover");
        let server = create_server(&root);
        assert_eq!(
            PathBuf::from(preferred_llama_server_path(&root, "")),
            server
        );
        assert_eq!(
            PathBuf::from(preferred_llama_server_path(
                &root,
                "C:/missing/llama-server.exe"
            )),
            server
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn installed_runtime_is_written_to_config_as_an_absolute_valid_path() {
        let root = temp_root("persist");
        let server = create_server(&root);
        std::fs::write(&root.join("config.json"), b"{\"model_manager\":{}}").unwrap();

        let persisted = BackendManager::persist_llama_server_path(&root, &server).unwrap();
        let config: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("config.json")).unwrap())
                .unwrap();

        assert!(persisted.is_absolute());
        assert_eq!(
            config["model_manager"]["llama_server_path"],
            persisted.display().to_string()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn is_local_server(server_url: &str) -> bool {
    let address = server_address(server_url).unwrap_or_default();
    matches!(
        address
            .split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "127.0.0.1" | "localhost" | "::1"
    )
}

fn server_reachable(server_url: &str) -> bool {
    let Some(address) = server_address(server_url) else {
        return false;
    };
    let Ok(addresses) = address.to_socket_addrs() else {
        return false;
    };
    addresses
        .into_iter()
        .any(|address| TcpStream::connect_timeout(&address, Duration::from_millis(120)).is_ok())
}

fn server_address(server_url: &str) -> Option<&str> {
    let without_scheme = server_url
        .strip_prefix("ws://")
        .or_else(|| server_url.strip_prefix("wss://"))?;
    let address = without_scheme.split('/').next()?.trim();
    (!address.is_empty()).then_some(address)
}

#[cfg(windows)]
struct KillOnCloseJob {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl KillOnCloseJob {
    fn new() -> Result<Self, String> {
        use std::mem::size_of;
        use windows_sys::Win32::{
            Foundation::{GetLastError, INVALID_HANDLE_VALUE},
            System::JobObjects::{
                CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
        };

        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(format!("Cannot create backend process job: {}", unsafe {
                GetLastError()
            }));
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
            return Err(format!(
                "Cannot configure backend process job: {}",
                unsafe { GetLastError() }
            ));
        }
        Ok(Self { handle })
    }

    fn assign(&self, child: &Child) -> Result<(), String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::GetLastError, System::JobObjects::AssignProcessToJobObject,
        };
        let assigned = unsafe { AssignProcessToJobObject(self.handle, child.as_raw_handle() as _) };
        if assigned == 0 {
            return Err(format!("Cannot manage backend process tree: {}", unsafe {
                GetLastError()
            }));
        }
        Ok(())
    }

    fn terminate(&self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.handle, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}
