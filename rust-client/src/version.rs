//! Dedicated application version and build metadata repository.
//!
//! This module serves as the central source of truth for versioning, release metadata,
//! build targets, and future diagnostic/record information.

/// Application semantic version (inherited from Cargo.toml at compile time).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Official application display name.
pub const APP_NAME: &str = "XRTranslate";

/// Build profile (Debug or Release).
pub const APP_BUILD_PROFILE: &str = if cfg!(debug_assertions) {
    "Debug"
} else {
    "Release"
};

/// Returns formatted display version string, e.g. "v0.1.0 (Release)".
pub fn version_display_string() -> String {
    format!("v{} ({})", APP_VERSION, APP_BUILD_PROFILE)
}

/// Returns full metadata summary for logging and diagnostics.
pub fn full_metadata_summary() -> String {
    format!("{} v{} [{}]", APP_NAME, APP_VERSION, APP_BUILD_PROFILE)
}
