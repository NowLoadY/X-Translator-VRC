#![forbid(unsafe_code)]

use reqwest::{
    StatusCode,
    header::{ACCEPT_ENCODING, CONTENT_RANGE, RANGE},
};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fmt, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct DownloadSpec<'a> {
    pub label: &'a str,
    pub url: &'a str,
    pub bytes: u64,
    pub sha256: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct DownloadPolicy {
    pub max_attempts: u32,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub retry_delay: Duration,
}

impl Default for DownloadPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            connect_timeout: Duration::from_secs(15),
            read_timeout: Duration::from_secs(45),
            retry_delay: Duration::from_secs(1),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadClient {
    client: reqwest::Client,
    policy: DownloadPolicy,
}

impl DownloadClient {
    pub fn new(user_agent: &str) -> Result<Self, DownloadError> {
        Self::with_policy(user_agent, DownloadPolicy::default())
    }

    pub fn with_policy(user_agent: &str, policy: DownloadPolicy) -> Result<Self, DownloadError> {
        if policy.max_attempts == 0 {
            return Err(DownloadError::InvalidSpec(
                "download max_attempts must be greater than zero".into(),
            ));
        }
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .connect_timeout(policy.connect_timeout)
            .read_timeout(policy.read_timeout)
            .build()
            .map_err(|error| DownloadError::Client(error.to_string()))?;
        Ok(Self { client, policy })
    }

    pub async fn download(
        &self,
        spec: DownloadSpec<'_>,
        partial: &Path,
        complete: &Path,
        mut on_progress: impl FnMut(DownloadProgress),
    ) -> Result<(), DownloadError> {
        validate_spec(spec)?;
        if complete.is_file() {
            match verify_file(complete, spec) {
                Ok(()) => {
                    on_progress(DownloadProgress {
                        downloaded_bytes: spec.bytes,
                        total_bytes: spec.bytes,
                    });
                    return Ok(());
                }
                Err(error) => {
                    let _ = fs::remove_file(complete);
                    return Err(error);
                }
            }
        }
        if let Some(parent) = partial.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|source| DownloadError::FileIo {
                    path: parent.to_path_buf(),
                    source,
                })?;
        }

        for attempt in 1..=self.policy.max_attempts {
            match self.transfer_once(spec, partial, &mut on_progress).await {
                Ok(()) => {
                    if let Err(error) = verify_file(partial, spec) {
                        let _ = fs::remove_file(partial);
                        return Err(error);
                    }
                    tokio::fs::rename(partial, complete)
                        .await
                        .map_err(|source| DownloadError::FileIo {
                            path: complete.to_path_buf(),
                            source,
                        })?;
                    return Ok(());
                }
                Err(error) if error.is_retryable() && attempt < self.policy.max_attempts => {
                    let multiplier = 1_u32 << (attempt - 1).min(4);
                    tokio::time::sleep(self.policy.retry_delay.saturating_mul(multiplier)).await;
                }
                Err(error) => return Err(error.with_attempts(attempt)),
            }
        }
        unreachable!("a non-zero retry policy always returns from the loop")
    }

    async fn transfer_once(
        &self,
        spec: DownloadSpec<'_>,
        partial: &Path,
        on_progress: &mut impl FnMut(DownloadProgress),
    ) -> Result<(), DownloadError> {
        let existing = fs::metadata(partial)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if existing > spec.bytes {
            let _ = fs::remove_file(partial);
            return Err(DownloadError::Size {
                path: partial.to_path_buf(),
                expected: spec.bytes,
                actual: existing,
            });
        }
        if existing == spec.bytes {
            return Ok(());
        }

        let mut request = self
            .client
            .get(spec.url)
            .header(ACCEPT_ENCODING, "identity");
        if existing > 0 {
            request = request.header(RANGE, format!("bytes={existing}-"));
        }
        let mut response = request
            .send()
            .await
            .map_err(|error| DownloadError::Transfer {
                label: spec.label.to_owned(),
                message: error.to_string(),
                attempts: 0,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(DownloadError::HttpStatus {
                label: spec.label.to_owned(),
                status,
                attempts: 0,
            });
        }

        let append = if existing > 0 && status == StatusCode::PARTIAL_CONTENT {
            validate_content_range(&response, existing, spec.bytes, spec.label)?;
            true
        } else if existing > 0 && status == StatusCode::OK {
            false
        } else if existing == 0 && status == StatusCode::PARTIAL_CONTENT {
            validate_content_range(&response, 0, spec.bytes, spec.label)?;
            false
        } else {
            existing == 0
        };
        let mut downloaded = if append { existing } else { 0 };
        let expected_response_bytes = spec.bytes.saturating_sub(downloaded);
        if let Some(actual) = response.content_length()
            && actual != expected_response_bytes
        {
            return Err(DownloadError::RemoteSize {
                label: spec.label.to_owned(),
                expected: expected_response_bytes,
                actual,
            });
        }

        let mut output = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(partial)
            .await
            .map_err(|source| DownloadError::FileIo {
                path: partial.to_path_buf(),
                source,
            })?;
        on_progress(DownloadProgress {
            downloaded_bytes: downloaded,
            total_bytes: spec.bytes,
        });

        loop {
            let chunk = match response.chunk().await {
                Ok(chunk) => chunk,
                Err(error) => {
                    let _ = output.flush().await;
                    let _ = output.sync_all().await;
                    return Err(DownloadError::Transfer {
                        label: spec.label.to_owned(),
                        message: error.to_string(),
                        attempts: 0,
                    });
                }
            };
            let Some(chunk) = chunk else { break };
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            if downloaded > spec.bytes {
                let _ = output.flush().await;
                drop(output);
                let _ = fs::remove_file(partial);
                return Err(DownloadError::Size {
                    path: partial.to_path_buf(),
                    expected: spec.bytes,
                    actual: downloaded,
                });
            }
            output
                .write_all(&chunk)
                .await
                .map_err(|source| DownloadError::FileIo {
                    path: partial.to_path_buf(),
                    source,
                })?;
            on_progress(DownloadProgress {
                downloaded_bytes: downloaded,
                total_bytes: spec.bytes,
            });
        }
        output
            .flush()
            .await
            .map_err(|source| DownloadError::FileIo {
                path: partial.to_path_buf(),
                source,
            })?;
        output
            .sync_all()
            .await
            .map_err(|source| DownloadError::FileIo {
                path: partial.to_path_buf(),
                source,
            })?;
        if downloaded != spec.bytes {
            return Err(DownloadError::Incomplete {
                label: spec.label.to_owned(),
                expected: spec.bytes,
                actual: downloaded,
                attempts: 0,
            });
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum DownloadError {
    Client(String),
    InvalidSpec(String),
    FileIo {
        path: PathBuf,
        source: io::Error,
    },
    HttpStatus {
        label: String,
        status: StatusCode,
        attempts: u32,
    },
    Transfer {
        label: String,
        message: String,
        attempts: u32,
    },
    Incomplete {
        label: String,
        expected: u64,
        actual: u64,
        attempts: u32,
    },
    RemoteSize {
        label: String,
        expected: u64,
        actual: u64,
    },
    Range {
        label: String,
        expected_start: u64,
        value: String,
    },
    Size {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    Integrity {
        path: PathBuf,
        expected: String,
        actual: String,
    },
}

impl DownloadError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Transfer { .. } | Self::Incomplete { .. } => true,
            Self::HttpStatus { status, .. } => matches!(
                *status,
                StatusCode::REQUEST_TIMEOUT
                    | StatusCode::TOO_MANY_REQUESTS
                    | StatusCode::INTERNAL_SERVER_ERROR
                    | StatusCode::BAD_GATEWAY
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::GATEWAY_TIMEOUT
            ),
            _ => false,
        }
    }

    fn with_attempts(mut self, attempts: u32) -> Self {
        match &mut self {
            Self::HttpStatus {
                attempts: value, ..
            }
            | Self::Transfer {
                attempts: value, ..
            }
            | Self::Incomplete {
                attempts: value, ..
            } => *value = attempts,
            _ => {}
        }
        self
    }
}

impl fmt::Display for DownloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(message) => write!(formatter, "cannot create download client: {message}"),
            Self::InvalidSpec(message) => formatter.write_str(message),
            Self::FileIo { path, source } => {
                write!(formatter, "cannot write {}: {source}", path.display())
            }
            Self::HttpStatus {
                label,
                status,
                attempts,
            } => write!(
                formatter,
                "download {label} returned HTTP {status} after {attempts} attempt(s); retry to resume"
            ),
            Self::Transfer {
                label,
                message,
                attempts,
            } => write!(
                formatter,
                "download {label} was interrupted after {attempts} attempt(s): {message}; retry to resume"
            ),
            Self::Incomplete {
                label,
                expected,
                actual,
                attempts,
            } => write!(
                formatter,
                "download {label} stopped at {actual}/{expected} bytes after {attempts} attempt(s); retry to resume"
            ),
            Self::RemoteSize {
                label,
                expected,
                actual,
            } => write!(
                formatter,
                "download {label} reported {actual} bytes; the manifest expects {expected}"
            ),
            Self::Range {
                label,
                expected_start,
                value,
            } => write!(
                formatter,
                "download {label} returned invalid Content-Range {value:?}; expected byte {expected_start}"
            ),
            Self::Size {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "downloaded file {} is {actual} bytes; expected {expected}",
                path.display()
            ),
            Self::Integrity {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "downloaded file {} has SHA-256 {actual}; expected {expected}",
                path.display()
            ),
        }
    }
}

impl Error for DownloadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::FileIo { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn validate_spec(spec: DownloadSpec<'_>) -> Result<(), DownloadError> {
    if !spec.url.starts_with("https://") {
        return Err(DownloadError::InvalidSpec(format!(
            "download {} must use HTTPS",
            spec.label
        )));
    }
    if spec.bytes == 0 {
        return Err(DownloadError::InvalidSpec(format!(
            "download {} has no configured byte length",
            spec.label
        )));
    }
    if spec.sha256.len() != 64 || !spec.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DownloadError::InvalidSpec(format!(
            "download {} has an invalid SHA-256",
            spec.label
        )));
    }
    Ok(())
}

fn validate_content_range(
    response: &reqwest::Response,
    expected_start: u64,
    expected_total: u64,
    label: &str,
) -> Result<(), DownloadError> {
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
    if parsed != Some((expected_start, expected_total)) {
        return Err(DownloadError::Range {
            label: label.to_owned(),
            expected_start,
            value: value.to_owned(),
        });
    }
    Ok(())
}

fn verify_file(path: &Path, spec: DownloadSpec<'_>) -> Result<(), DownloadError> {
    let actual_size = fs::metadata(path)
        .map_err(|source| DownloadError::FileIo {
            path: path.to_path_buf(),
            source,
        })?
        .len();
    if actual_size != spec.bytes {
        return Err(DownloadError::Size {
            path: path.to_path_buf(),
            expected: spec.bytes,
            actual: actual_size,
        });
    }
    let actual = sha256_file(path).map_err(|source| DownloadError::FileIo {
        path: path.to_path_buf(),
        source,
    })?;
    if !actual.eq_ignore_ascii_case(spec.sha256) {
        return Err(DownloadError::Integrity {
            path: path.to_path_buf(),
            expected: spec.sha256.to_owned(),
            actual,
        });
    }
    Ok(())
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
