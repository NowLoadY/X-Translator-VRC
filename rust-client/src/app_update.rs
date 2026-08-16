//! Application update discovery, staging, and handoff to the updater process.
//!
//! The desktop UI owns only the state machine. Network, archive validation, and
//! staging run on a worker thread; replacing the running application is handed
//! to `xrtranslate-updater` so Windows can swap the executable after exit.

use crossbeam_channel::{Receiver, TryRecvError, unbounded};
use reqwest::{
    StatusCode,
    header::{ACCEPT, ACCEPT_ENCODING, CONTENT_RANGE, HeaderValue, RANGE},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};
use tokio::io::AsyncWriteExt;

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/NowLoadY/XRTranslate/releases/latest";
const LATEST_RELEASE_PAGE: &str = "https://github.com/NowLoadY/XRTranslate/releases/latest";
const RELEASE_DOWNLOAD_BASE: &str = "https://github.com/NowLoadY/XRTranslate/releases/download/";
const USER_AGENT: &str = concat!("XRTranslate updater/", env!("CARGO_PKG_VERSION"));
const MAX_DOWNLOAD_ATTEMPTS: u32 = 4;
const GITHUB_API_VERSION: &str = "2022-11-28";

#[derive(Clone, Debug)]
pub struct AppUpdateInfo {
    pub version: String,
    pub asset_name: String,
    pub size: u64,
}

#[derive(Clone, Debug, Default)]
pub enum AppUpdateState {
    #[default]
    Idle,
    Checking,
    Current,
    Available(AppUpdateInfo),
    Downloading {
        info: AppUpdateInfo,
        downloaded: u64,
        total: u64,
    },
    Ready(AppUpdateInfo),
    Installing,
    Failed(String),
}

impl AppUpdateState {
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading { .. } | Self::Installing
        )
    }
}

#[derive(Clone, Debug)]
pub struct PreparedUpdate {
    source: PathBuf,
    updater_entrypoint: String,
    info: AppUpdateInfo,
}

#[derive(Debug)]
pub struct AppUpdateInstall {
    pub updater: PathBuf,
    pub source: PathBuf,
    pub target: PathBuf,
}

#[derive(Debug)]
enum Event {
    Checked(Result<Option<ReleaseAsset>, String>),
    Downloading { downloaded: u64, total: u64 },
    Prepared(Result<PreparedUpdate, String>),
}

#[derive(Default)]
pub struct AppUpdateManager {
    state: AppUpdateState,
    events: Option<Receiver<Event>>,
    available: Option<ReleaseAsset>,
    prepared: Option<PreparedUpdate>,
    proxy_url: Option<String>,
}

impl AppUpdateManager {
    pub fn set_proxy_url(&mut self, proxy_url: &str) {
        self.proxy_url = (!proxy_url.trim().is_empty()).then(|| proxy_url.trim().to_owned());
    }
    #[must_use]
    pub fn state(&self) -> &AppUpdateState {
        &self.state
    }

    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.state.is_busy()
    }

    pub fn check(&mut self) -> Result<(), String> {
        if self.is_busy() {
            return Ok(());
        }
        let (sender, receiver) = unbounded();
        let proxy_url = self.proxy_url.clone();
        thread::Builder::new()
            .name("app-update-checker".into())
            .spawn(move || {
                let result = run_async(|| check_latest_release(proxy_url.as_deref()));
                let _ = sender.send(Event::Checked(result));
            })
            .map_err(|error| format!("Cannot start update checker: {error}"))?;
        self.state = AppUpdateState::Checking;
        self.events = Some(receiver);
        Ok(())
    }

    pub fn download(&mut self, project_root: PathBuf) -> Result<(), String> {
        if self.is_busy() {
            return Ok(());
        }
        let asset = self
            .available
            .clone()
            .ok_or("Check for updates before downloading.")?;
        let info = asset.info();
        let (sender, receiver) = unbounded();
        let proxy_url = self.proxy_url.clone();
        thread::Builder::new()
            .name("app-update-downloader".into())
            .spawn(move || {
                let progress = sender.clone();
                let result = run_async(|| {
                    download_and_stage(project_root, asset, progress, proxy_url.as_deref())
                });
                let _ = sender.send(Event::Prepared(result));
            })
            .map_err(|error| format!("Cannot start update download: {error}"))?;
        self.state = AppUpdateState::Downloading {
            downloaded: 0,
            total: info.size,
            info,
        };
        self.events = Some(receiver);
        Ok(())
    }

    pub fn begin_install(&mut self) -> Result<AppUpdateInstall, String> {
        if self.is_busy() {
            return Err("An update task is already running.".into());
        }
        let prepared = self
            .prepared
            .clone()
            .ok_or("Download the update before installing.")?;
        let updater = prepared.source.join(&prepared.updater_entrypoint);
        if !updater.is_file() {
            return Err("The update installer is missing from the downloaded package.".into());
        }
        self.state = AppUpdateState::Installing;
        Ok(AppUpdateInstall {
            updater,
            source: prepared.source,
            target: PathBuf::from("."),
        })
    }

    pub fn poll(&mut self) {
        let Some(events) = &self.events else {
            return;
        };
        let mut finished = false;
        loop {
            match events.try_recv() {
                Ok(Event::Checked(result)) => {
                    match result {
                        Ok(Some(asset)) => {
                            self.state = AppUpdateState::Available(asset.info());
                            self.available = Some(asset);
                            self.prepared = None;
                        }
                        Ok(None) => {
                            self.state = AppUpdateState::Current;
                            self.available = None;
                            self.prepared = None;
                        }
                        Err(error) => self.state = AppUpdateState::Failed(error),
                    }
                    finished = true;
                    break;
                }
                Ok(Event::Downloading { downloaded, total }) => {
                    if let AppUpdateState::Downloading { info, .. } = &self.state {
                        self.state = AppUpdateState::Downloading {
                            info: info.clone(),
                            downloaded,
                            total,
                        };
                    }
                }
                Ok(Event::Prepared(result)) => {
                    match result {
                        Ok(prepared) => {
                            self.state = AppUpdateState::Ready(prepared.info.clone());
                            self.prepared = Some(prepared);
                        }
                        Err(error) => self.state = AppUpdateState::Failed(error),
                    }
                    finished = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.state =
                        AppUpdateState::Failed("The update worker stopped unexpectedly.".into());
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
    version: String,
    name: String,
    download_url: String,
    size: u64,
    sha256: Option<String>,
}

impl ReleaseAsset {
    fn info(&self) -> AppUpdateInfo {
        AppUpdateInfo {
            version: self.version.clone(),
            asset_name: self.name.clone(),
            size: self.size,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

async fn check_latest_release(proxy_url: Option<&str>) -> Result<Option<ReleaseAsset>, String> {
    if !cfg!(any(target_os = "windows", target_os = "linux")) {
        return Err("Updates are available for Windows and Linux builds only.".into());
    }
    let client = http_client(proxy_url)?;
    let (latest_tag, latest_version) = match discover_latest_version(&client).await {
        Ok(release) => release,
        Err(page_error) => {
            let release = fetch_latest_release(&client).await.map_err(|api_error| {
                format!(
                    "Cannot check for updates: GitHub release page failed ({page_error}); \
                     GitHub API failed ({api_error})."
                )
            })?;
            return release_asset_if_newer(release);
        }
    };
    if !version_is_newer(&latest_version, crate::version::APP_VERSION) {
        return Ok(None);
    }

    match fetch_latest_release(&client).await {
        Ok(release) => release_asset_if_newer(release),
        Err(api_error) => fallback_release_asset(&client, &latest_tag, &latest_version)
            .await
            .map(Some)
            .map_err(|fallback_error| {
                format!(
                    "GitHub API is unavailable ({api_error}); direct release lookup also failed \
                     ({fallback_error})."
                )
            }),
    }
}

async fn discover_latest_version(client: &reqwest::Client) -> Result<(String, String), String> {
    let response = client
        .get(LATEST_RELEASE_PAGE)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let tag = release_tag_from_url(response.url())
        .ok_or_else(|| format!("unexpected redirect target {}", response.url()))?;
    let version = normalize_version(&tag)?;
    Ok((tag, version))
}

async fn fetch_latest_release(client: &reqwest::Client) -> Result<GitHubRelease, String> {
    let response = client
        .get(LATEST_RELEASE_URL)
        .header(ACCEPT, "application/vnd.github+json")
        .header(
            "X-GitHub-Api-Version",
            HeaderValue::from_static(GITHUB_API_VERSION),
        )
        .send()
        .await
        .map_err(|error| error.to_string())?;
    response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<GitHubRelease>()
        .await
        .map_err(|error| error.to_string())
}

fn release_asset_if_newer(release: GitHubRelease) -> Result<Option<ReleaseAsset>, String> {
    let latest_version = normalize_version(&release.tag_name)?;
    if !version_is_newer(&latest_version, crate::version::APP_VERSION) {
        return Ok(None);
    }
    let asset = select_release_asset(&release.assets)
        .ok_or_else(|| format!("No update package is available for {}.", platform_label()))?;
    Ok(Some(ReleaseAsset {
        version: latest_version,
        name: asset.name.clone(),
        download_url: asset.browser_download_url.clone(),
        size: asset.size,
        sha256: asset.digest.as_deref().and_then(parse_sha256_digest),
    }))
}

async fn fallback_release_asset(
    client: &reqwest::Client,
    tag: &str,
    version: &str,
) -> Result<ReleaseAsset, String> {
    let name = standard_release_asset_name(version);
    let mut download_url = reqwest::Url::parse(RELEASE_DOWNLOAD_BASE)
        .map_err(|error| format!("invalid release URL: {error}"))?;
    download_url
        .path_segments_mut()
        .map_err(|_| "invalid release URL".to_string())?
        .push(tag)
        .push(&name);
    let response = client
        .head(download_url.clone())
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let size = response
        .content_length()
        .filter(|size| *size > 0)
        .ok_or("release server did not report the package size")?;
    Ok(ReleaseAsset {
        version: version.to_owned(),
        name,
        download_url: download_url.into(),
        size,
        sha256: None,
    })
}

fn release_tag_from_url(url: &reqwest::Url) -> Option<String> {
    let segments = url.path_segments()?.collect::<Vec<_>>();
    let tag = segments
        .windows(2)
        .find_map(|pair| (pair[0] == "tag").then_some(pair[1]))?;
    (!tag.is_empty()).then(|| tag.to_owned())
}

fn standard_release_asset_name(version: &str) -> String {
    let platform = if cfg!(target_os = "windows") {
        "win-x64"
    } else {
        "linux-x64"
    };
    format!("XRTranslate-v{version}-{platform}.zip")
}

async fn download_and_stage(
    project_root: PathBuf,
    asset: ReleaseAsset,
    sender: crossbeam_channel::Sender<Event>,
    proxy_url: Option<&str>,
) -> Result<PreparedUpdate, String> {
    if !asset.download_url.starts_with("https://") {
        return Err("The update download URL is not secure.".into());
    }
    if asset.size == 0 {
        return Err("The update package is empty.".into());
    }
    let updates_root = project_root.join("runtime").join("updates");
    let download_dir = updates_root.join("downloads");
    let staging_root = updates_root.join(format!("v{}-staging", safe_path_segment(&asset.version)));
    let payload = staging_root.join("payload");
    fs::create_dir_all(&download_dir)
        .map_err(|error| format!("Cannot create update download folder: {error}"))?;
    reset_directory(&payload)?;

    let archive = download_dir.join(&asset.name);
    let partial = download_dir.join(format!("{}.part", asset.name));
    let client = http_client(proxy_url)?;
    download_asset(&client, &asset, &partial, &archive, |downloaded, total| {
        let _ = sender.send(Event::Downloading { downloaded, total });
    })
    .await?;

    verify_downloaded_asset(&archive, &asset)?;
    extract_zip(&archive, &payload)?;
    let source = release_source_directory(&payload)?;
    let updater_entrypoint = validate_staged_release(&source)?;
    Ok(PreparedUpdate {
        source,
        updater_entrypoint,
        info: asset.info(),
    })
}

async fn download_asset(
    client: &reqwest::Client,
    asset: &ReleaseAsset,
    partial: &Path,
    complete: &Path,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    if complete.is_file() && file_size(complete)? == asset.size {
        on_progress(asset.size, asset.size);
        return Ok(());
    }
    if complete.exists() {
        let _ = fs::remove_file(complete);
    }
    if file_size(partial).unwrap_or(0) > asset.size {
        let _ = fs::remove_file(partial);
    }
    for attempt in 1..=MAX_DOWNLOAD_ATTEMPTS {
        match transfer_once(client, asset, partial, &mut on_progress).await {
            Ok(()) => {
                tokio::fs::rename(partial, complete)
                    .await
                    .map_err(|error| format!("Cannot save update package: {error}"))?;
                return Ok(());
            }
            Err(_error) if attempt < MAX_DOWNLOAD_ATTEMPTS => {
                tokio::time::sleep(Duration::from_secs(1 << (attempt - 1).min(4))).await;
            }
            Err(error) => return Err(error.message),
        }
    }
    unreachable!("download loop always returns")
}

struct TransferError {
    message: String,
}

async fn transfer_once(
    client: &reqwest::Client,
    asset: &ReleaseAsset,
    partial: &Path,
    on_progress: &mut impl FnMut(u64, u64),
) -> Result<(), TransferError> {
    let existing = file_size(partial).unwrap_or(0).min(asset.size);
    let mut request = client
        .get(&asset.download_url)
        .header(ACCEPT_ENCODING, "identity");
    if existing > 0 {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            return Err(TransferError {
                message: format!("Cannot download update: {error}"),
            });
        }
    };
    if !response.status().is_success() {
        return Err(TransferError {
            message: format!("Update download returned HTTP {}.", response.status()),
        });
    }

    let append = if existing > 0 && response.status() == StatusCode::PARTIAL_CONTENT {
        validate_content_range(&response, existing, asset.size)
            .map_err(|message| TransferError { message })?;
        true
    } else {
        existing == 0
    };
    let mut downloaded = if append { existing } else { 0 };
    if let Some(parent) = partial.parent()
        && let Err(error) = tokio::fs::create_dir_all(parent).await
    {
        return Err(TransferError {
            message: format!("Cannot create update download folder: {error}"),
        });
    }
    let mut output = match tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(partial)
        .await
    {
        Ok(output) => output,
        Err(error) => {
            return Err(TransferError {
                message: format!("Cannot write update package: {error}"),
            });
        }
    };
    on_progress(downloaded, asset.size);
    let mut response = response;
    loop {
        let chunk = match response.chunk().await {
            Ok(chunk) => chunk,
            Err(error) => {
                let _ = output.flush().await;
                return Err(TransferError {
                    message: format!("Update download was interrupted: {error}"),
                });
            }
        };
        let Some(chunk) = chunk else { break };
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > asset.size {
            let _ = output.flush().await;
            let _ = fs::remove_file(partial);
            return Err(TransferError {
                message: "The update package is larger than expected.".into(),
            });
        }
        if let Err(error) = output.write_all(&chunk).await {
            return Err(TransferError {
                message: format!("Cannot write update package: {error}"),
            });
        }
        on_progress(downloaded, asset.size);
    }
    if let Err(error) = output.flush().await {
        return Err(TransferError {
            message: format!("Cannot finish update package: {error}"),
        });
    }
    if let Err(error) = output.sync_all().await {
        return Err(TransferError {
            message: format!("Cannot finish update package: {error}"),
        });
    }
    if downloaded != asset.size {
        return Err(TransferError {
            message: format!(
                "Update download stopped at {} of {} bytes.",
                downloaded, asset.size
            ),
        });
    }
    Ok(())
}

fn validate_content_range(
    response: &reqwest::Response,
    expected_start: u64,
    expected_total: u64,
) -> Result<(), String> {
    let value = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let parsed = value
        .strip_prefix("bytes ")
        .and_then(|value| value.split_once('/'))
        .and_then(|(range, total)| {
            let (start, _) = range.split_once('-')?;
            Some((start.parse::<u64>().ok()?, total.parse::<u64>().ok()?))
        });
    if parsed == Some((expected_start, expected_total)) {
        Ok(())
    } else {
        Err("The update server returned an invalid resume response.".into())
    }
}

fn verify_downloaded_asset(path: &Path, asset: &ReleaseAsset) -> Result<(), String> {
    let actual_size = file_size(path)?;
    if actual_size != asset.size {
        return Err(format!(
            "The update package has {actual_size} bytes; expected {}.",
            asset.size
        ));
    }
    if let Some(expected) = &asset.sha256 {
        let actual = sha256_file(path)
            .map_err(|error| format!("Cannot verify update package {}: {error}", path.display()))?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err("The update package did not pass verification.".into());
        }
    }
    Ok(())
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(archive)
        .map_err(|error| format!("Cannot open update package {}: {error}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|error| format!("The update package is not a valid archive: {error}"))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| format!("Cannot read update package: {error}"))?;
        let Some(name) = entry.enclosed_name() else {
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
        io::copy(&mut entry, &mut file)
            .map_err(|error| format!("Cannot extract {}: {error}", output.display()))?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output, fs::Permissions::from_mode(mode)).map_err(|error| {
                format!("Cannot set permissions on {}: {error}", output.display())
            })?;
        }
    }
    Ok(())
}

fn release_source_directory(payload: &Path) -> Result<PathBuf, String> {
    if payload.join("release-manifest.json").is_file() {
        return Ok(payload.to_path_buf());
    }
    let release_roots = fs::read_dir(payload)
        .map_err(|error| format!("Cannot inspect update package: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("release-manifest.json").is_file())
        .collect::<Vec<_>>();
    match release_roots.as_slice() {
        [root] => Ok(root.clone()),
        _ => Err("The update package does not contain a valid XRTranslate release.".into()),
    }
}

fn validate_staged_release(source: &Path) -> Result<String, String> {
    let manifest_path = source.join("release-manifest.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).map_err(|error| {
            format!(
                "Cannot read release manifest {}: {error}",
                manifest_path.display()
            )
        })?)
        .map_err(|error| format!("Invalid release manifest: {error}"))?;
    if manifest["python"].as_bool() != Some(false) {
        return Err("The selected release package is not supported by this client.".into());
    }
    let Some(client) = manifest
        .pointer("/entrypoints/client")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return Err("The update package is missing the application entrypoint.".into());
    };
    let Some(updater) = manifest
        .pointer("/entrypoints/updater")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return Err("The update package is missing the update helper.".into());
    };
    if !source.join(client).is_file() || !source.join(updater).is_file() {
        return Err("The update package is incomplete.".into());
    }
    Ok(updater.to_owned())
}

pub fn spawn_updater(install: AppUpdateInstall) -> Result<(), String> {
    let mut command = Command::new(&install.updater);
    command
        .arg("--source")
        .arg(&install.source)
        .arg("--target")
        .arg(&install.target)
        .arg("--current-pid")
        .arg(std::process::id().to_string())
        .arg("--restart")
        .current_dir(&install.target);
    crate::child_process::hide_console(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Cannot start update installer: {error}"))
}

fn select_release_asset(assets: &[GitHubAsset]) -> Option<&GitHubAsset> {
    assets
        .iter()
        .filter(|asset| {
            let name = asset.name.to_ascii_lowercase();
            name.ends_with(".zip") && name_matches_platform(&name)
        })
        .max_by_key(|asset| platform_asset_score(&asset.name.to_ascii_lowercase()))
}

fn name_matches_platform(name: &str) -> bool {
    let arch_ok = ["x64", "x86_64", "amd64"]
        .iter()
        .any(|token| name.contains(token));
    if cfg!(target_os = "windows") {
        arch_ok && ["win", "windows"].iter().any(|token| name.contains(token))
    } else if cfg!(target_os = "linux") {
        arch_ok && name.contains("linux")
    } else {
        false
    }
}

fn platform_asset_score(name: &str) -> u8 {
    let mut score = 0;
    if name.contains("x64") {
        score += 2;
    }
    if cfg!(target_os = "windows") && name.contains("win-x64") {
        score += 3;
    }
    if cfg!(target_os = "linux") && name.contains("linux-x64") {
        score += 3;
    }
    score
}

fn platform_label() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows x64"
    } else if cfg!(target_os = "linux") {
        "Linux x64"
    } else {
        "this platform"
    }
}

fn normalize_version(tag: &str) -> Result<String, String> {
    let version = tag.trim().trim_start_matches(['v', 'V']);
    if version.is_empty() {
        Err("The latest release has no version number.".into())
    } else {
        Ok(version.to_owned())
    }
}

fn version_is_newer(latest: &str, current: &str) -> bool {
    let latest = version_parts(latest);
    let current = version_parts(current);
    latest > current
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .split(['.', '-', '+'])
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn parse_sha256_digest(value: &str) -> Option<String> {
    let digest = value.strip_prefix("sha256:").unwrap_or(value).trim();
    (digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| digest.to_ascii_lowercase())
}

fn http_client(proxy_url: Option<&str>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(45));
    if let Some(proxy_url) = proxy_url.filter(|url| !url.trim().is_empty()) {
        builder = builder.proxy(
            reqwest::Proxy::all(proxy_url)
                .map_err(|error| format!("Cannot configure download proxy: {error}"))?,
        );
    }
    builder
        .build()
        .map_err(|error| format!("Cannot create update client: {error}"))
}

fn run_async<F, Fut, T>(task: F) -> Result<T, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Cannot initialize update worker: {error}"))?
        .block_on(task())
}

fn reset_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(|error| format!("Cannot reset {}: {error}", path.display()))?;
    }
    fs::create_dir_all(path).map_err(|error| format!("Cannot create {}: {error}", path.display()))
}

fn file_size(path: &Path) -> Result<u64, String> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|error| format!("Cannot inspect {}: {error}", path.display()))
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
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

fn safe_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_current_platform_zip_assets() {
        let assets = vec![
            GitHubAsset {
                name: "XRTranslate-v1.2.0-linux-x64.zip".into(),
                browser_download_url: "https://example.invalid/linux.zip".into(),
                size: 1,
                digest: None,
            },
            GitHubAsset {
                name: "XRTranslate-v1.2.0-win-x64.zip".into(),
                browser_download_url: "https://example.invalid/win.zip".into(),
                size: 1,
                digest: None,
            },
        ];
        let selected = select_release_asset(&assets).unwrap();
        if cfg!(target_os = "windows") {
            assert!(selected.name.contains("win-x64"));
        } else if cfg!(target_os = "linux") {
            assert!(selected.name.contains("linux-x64"));
        }
    }

    #[test]
    fn compares_release_versions_numerically() {
        assert!(version_is_newer("0.10.0", "0.2.9"));
        assert!(!version_is_newer("0.2.0", "0.2.0"));
        assert!(!version_is_newer("0.1.9", "0.2.0"));
    }

    #[test]
    fn parses_github_sha256_digest() {
        let digest = "a".repeat(64);
        assert_eq!(
            parse_sha256_digest(&format!("sha256:{digest}")).as_deref(),
            Some(digest.as_str())
        );
        assert_eq!(parse_sha256_digest("sha256:not-a-digest"), None);
    }

    #[test]
    fn extracts_release_tag_from_latest_redirect() {
        let url = reqwest::Url::parse(
            "https://github.com/NowLoadY/XRTranslate/releases/tag/v0.2.5",
        )
        .unwrap();
        assert_eq!(release_tag_from_url(&url).as_deref(), Some("v0.2.5"));

        let unexpected =
            reqwest::Url::parse("https://github.com/NowLoadY/XRTranslate/releases").unwrap();
        assert_eq!(release_tag_from_url(&unexpected), None);
    }

    #[test]
    fn fallback_asset_name_matches_release_packaging() {
        let name = standard_release_asset_name("0.2.5");
        if cfg!(target_os = "windows") {
            assert_eq!(name, "XRTranslate-v0.2.5-win-x64.zip");
        } else {
            assert_eq!(name, "XRTranslate-v0.2.5-linux-x64.zip");
        }
    }
}
