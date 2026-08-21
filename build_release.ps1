[CmdletBinding()]
param(
    # A new directory for the staged release. Relative paths use the project root.
    [string]$Output,
    # Add the verified Qwen3-ASR and Hy-MT2 GGUF packages for an offline release.
    [switch]$IncludeModels,
    # Verified ONNX Runtime 1.28 core DLL. GPU providers are downloaded later.
    [string]$OnnxRuntimeCpu,
    # Validate all release inputs without writing a release directory.
    [switch]$ValidateOnly
)

$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$workspaceManifest = Join-Path $projectRoot 'Cargo.toml'
$configPath = Join-Path $projectRoot 'config.json'
$vadModel = Join-Path $projectRoot 'models\silero-vad\src\silero_vad\data\silero_vad.onnx'
$speakerModel = Join-Path $projectRoot 'models\3D-Speaker-ERes2NetV2\speaker_embedding.onnx'
$denoiseModel = Join-Path $projectRoot 'models\gtcrn\gtcrn_simple.onnx'
$corporaDirectory = Join-Path $projectRoot 'XR-Corpus\corpora'
$cargoPath = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'

if ([string]::IsNullOrWhiteSpace($OnnxRuntimeCpu)) {
    throw 'Pass -OnnxRuntimeCpu <onnxruntime.dll>. Expected ORT 1.28.0 CUDA13 archive core: 16,277,856 bytes, SHA-256 2462fe2d64ce063babefda3d9b1998380ffa74e99acf5d24d520ee67daa9e0f1. This compact CPU-capable core is the only inference runtime bundled in the release.'
}
$OnnxRuntimeCpu = [System.IO.Path]::GetFullPath($OnnxRuntimeCpu)
if (-not (Test-Path -LiteralPath $OnnxRuntimeCpu -PathType Leaf)) {
    throw "ONNX Runtime CPU core was not found: $OnnxRuntimeCpu"
}
$onnxRuntimePackageRoot = Split-Path (Split-Path $OnnxRuntimeCpu -Parent) -Parent
$onnxRuntimeLicense = Join-Path $onnxRuntimePackageRoot 'LICENSE'
$onnxRuntimeNotices = Join-Path $onnxRuntimePackageRoot 'ThirdPartyNotices.txt'
foreach ($path in @($onnxRuntimeLicense, $onnxRuntimeNotices)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "The ONNX Runtime core must come from an extracted official package with LICENSE and ThirdPartyNotices.txt: missing $path"
    }
}

if (-not (Test-Path -LiteralPath $workspaceManifest)) {
    throw "Rust workspace manifest was not found: $workspaceManifest"
}
if (-not (Test-Path -LiteralPath $configPath)) {
    throw "Release configuration was not found: $configPath"
}
if (-not (Test-Path -LiteralPath $vadModel)) {
    throw "Silero VAD model was not found: $vadModel"
}
if (-not (Test-Path -LiteralPath $speakerModel)) {
    throw "ERes2NetV2 speaker ONNX model was not found: $speakerModel"
}
if (-not (Test-Path -LiteralPath $denoiseModel)) {
    throw "GTCRN denoise ONNX model was not found: $denoiseModel"
}
if (-not (Test-Path -LiteralPath $corporaDirectory)) {
    throw "Versioned Markdown corpora were not found: $corporaDirectory"
}

if (Test-Path -LiteralPath $cargoPath) {
    $cargo = $cargoPath
} elseif (Get-Command cargo -ErrorAction SilentlyContinue) {
    $cargo = 'cargo'
} else {
    throw 'Cargo was not found. Install Rust with rustup, then restart PowerShell.'
}

if ([string]::IsNullOrWhiteSpace($Output)) {
    $version = "0.1.0"
    if (Test-Path -LiteralPath $workspaceManifest) {
        $manifestContent = Get-Content -LiteralPath $workspaceManifest -Raw
        if ($manifestContent -match 'version\s*=\s*"([^"]+)"') {
            $version = $matches[1]
        }
    }
    $Output = Join-Path $projectRoot "dist\XRTranslate-v$version-win-x64"
} elseif (-not [System.IO.Path]::IsPathRooted($Output)) {
    $Output = Join-Path $projectRoot $Output
}
$Output = [System.IO.Path]::GetFullPath($Output)

if (Test-Path -LiteralPath $Output) {
    throw "Release output already exists. Choose a new -Output path: $Output"
}

$buildArguments = @(
    'build', '--manifest-path', $workspaceManifest, '--release',
    '--package', 'rust-client',
    '--package', 'xrtranslate-backend',
    '--package', 'xrtranslate-installer',
    '--package', 'xrtranslate-updater',
    '--package', 'xrtranslate-packager',
    '--features', 'rust-client/mpv,xrtranslate-backend/managed-ort'
)

Write-Host 'Building native release binaries...'
& $cargo @buildArguments
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

& $cargo build --manifest-path (Join-Path $projectRoot 'XR-Corpus\Cargo.toml') --target-dir (Join-Path $projectRoot 'target') --release --package xr-corpus-server
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$packageArguments = @(
    'run', '--manifest-path', $workspaceManifest, '--release', '--package', 'xrtranslate-packager', '--',
    '--rust-client-bin', (Join-Path $projectRoot 'target\release\rust-client.exe'),
    '--backend-bin', (Join-Path $projectRoot 'target\release\xrtranslate-backend.exe'),
    '--corpus-bin', (Join-Path $projectRoot 'target\release\xr-corpus-server.exe'),
    '--installer-bin', (Join-Path $projectRoot 'target\release\xrtranslate-installer.exe'),
    '--updater-bin', (Join-Path $projectRoot 'target\release\xrtranslate-updater.exe'),
    '--config', $configPath,
    '--resources-dir', (Join-Path $projectRoot 'rust-client\resources'),
    '--corpora-dir', $corporaDirectory,
    '--vad-model', $vadModel,
    '--speaker-model', $speakerModel,
    '--denoise-model', $denoiseModel,
    '--onnx-runtime-cpu', $OnnxRuntimeCpu,
    '--onnx-runtime-license', $onnxRuntimeLicense,
    '--onnx-runtime-notices', $onnxRuntimeNotices,
    '--output', $Output
)
if ($IncludeModels) {
    $packageArguments += '--include-models'
}
if ($ValidateOnly) {
    $packageArguments += '--check'
}

Write-Host 'Preparing the native release package...'
& $cargo @packageArguments
if ($LASTEXITCODE -ne 0 -or $ValidateOnly) {
    exit $LASTEXITCODE
}

Write-Host "Release directory is ready: $Output"
