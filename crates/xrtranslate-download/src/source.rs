//! Central download-source routing for immutable artifacts.
//!
//! Feature installers select an official URL and whether the user requested a
//! mirror. Host-specific mirror URL rules stay here so model and runtime
//! installers never grow their own URL rewriting policy.

use std::borrow::Cow;

/// User-selected route for an immutable artifact download.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DownloadSource {
    #[default]
    Official,
    Mirror,
}

impl DownloadSource {
    #[must_use]
    pub const fn from_mirror_enabled(enabled: bool) -> Self {
        if enabled {
            Self::Mirror
        } else {
            Self::Official
        }
    }

    pub(crate) fn resolve<'a>(self, official_url: &'a str) -> Cow<'a, str> {
        if self == Self::Official {
            return Cow::Borrowed(official_url);
        }
        if let Some(path) = official_url.strip_prefix("https://huggingface.co/") {
            return Cow::Owned(format!("https://hf-mirror.com/{path}"));
        }
        if official_url.starts_with("https://github.com/") {
            return Cow::Owned(format!("https://ghfast.top/{official_url}"));
        }
        Cow::Borrowed(official_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_source_preserves_the_manifest_url() {
        let url = "https://github.com/org/repo/releases/download/v1/file.zip";
        assert_eq!(DownloadSource::Official.resolve(url), url);
    }

    #[test]
    fn mirror_source_routes_supported_hosts_centrally() {
        assert_eq!(
            DownloadSource::Mirror
                .resolve("https://huggingface.co/org/model/resolve/revision/model.gguf"),
            "https://hf-mirror.com/org/model/resolve/revision/model.gguf"
        );
        assert_eq!(
            DownloadSource::Mirror
                .resolve("https://github.com/org/repo/releases/download/v1/file.zip"),
            "https://ghfast.top/https://github.com/org/repo/releases/download/v1/file.zip"
        );
    }

    #[test]
    fn mirror_source_leaves_unknown_https_hosts_unchanged() {
        let url = "https://downloads.example.com/file.zip";
        assert_eq!(DownloadSource::Mirror.resolve(url), url);
    }
}
