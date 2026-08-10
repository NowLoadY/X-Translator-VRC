//! Explicit native model installation and verification entrypoint.
//!
//! This tool is intentionally separate from `xrtranslate-backend`: serving
//! audio must never surprise a user by consuming gigabytes of network traffic
//! or mutating the active model directory.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use xrtranslate_assets::{
    ModelAssetId, ModelAssetsConfig, NativeModelInstaller, ResolvedModelAssets,
};
use xrtranslate_config::AppConfig;

#[derive(Debug, Parser)]
#[command(
    name = "xrtranslate-installer",
    version,
    about = "Install or verify native XRTranslate GGUF models"
)]
struct Arguments {
    /// Path to the compatibility config.json file.
    #[arg(long, default_value = "config.json")]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Download one immutable model package, verify it, and atomically enable it.
    Install { package: Package },
    /// Read and hash installed packages without changing any active files.
    Verify { package: Option<Package> },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Package {
    Qwen3AsrGguf,
    HyMt2,
}

impl From<Package> for ModelAssetId {
    fn from(value: Package) -> Self {
        match value {
            Package::Qwen3AsrGguf => Self::Qwen3AsrGguf,
            Package::HyMt2 => Self::HunyuanMtGguf,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Arguments::parse();
    let config = AppConfig::from_path(&args.config)?;
    let project_root = args
        .config
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let assets = ModelAssetsConfig {
        models_directory: config.model_manager.models_directory,
        qwen3_asr_gguf_directory: config.model_manager.qwen3_asr_gguf_directory,
        hunyuan_mt_gguf_directory: config.model_manager.hunyuan_mt_gguf_directory,
    }
    .resolve(project_root);

    match args.command {
        Command::Install { package } => install(assets, package.into()).await?,
        Command::Verify { package } => verify(&assets, package.map(Into::into))?,
    }
    Ok(())
}

async fn install(
    assets: ResolvedModelAssets,
    package: ModelAssetId,
) -> Result<(), Box<dyn std::error::Error>> {
    let installer = NativeModelInstaller::new(assets)?;
    let mut last_file = "";
    let mut last_reported = 0_u64;
    let installed = installer
        .install(package, |progress| {
            if progress.relative_path != last_file {
                last_file = progress.relative_path;
                last_reported = 0;
            }
            const REPORT_INTERVAL: u64 = 64 * 1024 * 1024;
            if progress.downloaded_bytes == progress.total_bytes
                || progress.downloaded_bytes >= last_reported.saturating_add(REPORT_INTERVAL)
            {
                eprintln!(
                    "{}: {}/{} MiB",
                    progress.relative_path,
                    progress.downloaded_bytes / (1024 * 1024),
                    progress.total_bytes / (1024 * 1024)
                );
                last_reported = progress.downloaded_bytes;
            }
        })
        .await?;
    println!(
        "Installed and verified {} at {}",
        package,
        installed.display()
    );
    Ok(())
}

fn verify(
    assets: &ResolvedModelAssets,
    package: Option<ModelAssetId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let preflight = assets.verify_integrity();
    let diagnostics = preflight
        .diagnostics()
        .iter()
        .filter(|diagnostic| package.is_none_or(|id| diagnostic.asset_id == id))
        .collect::<Vec<_>>();
    if diagnostics.is_empty() {
        println!("Installed model files match the native manifest.");
        return Ok(());
    }
    let message = diagnostics
        .iter()
        .map(|diagnostic| format!("- {diagnostic}"))
        .collect::<Vec<_>>()
        .join("\n");
    Err(message.into())
}
