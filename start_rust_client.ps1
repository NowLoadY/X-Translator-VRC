[CmdletBinding()]
param(
    [switch]$Release
)

$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$manifestPath = Join-Path $projectRoot 'rust-client\Cargo.toml'
$workspaceManifestPath = Join-Path $projectRoot 'Cargo.toml'
$cargoPath = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'

foreach ($path in @($manifestPath, $workspaceManifestPath)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Rust workspace manifest was not found: $path"
    }
}

if (Test-Path -LiteralPath $cargoPath) {
    $cargo = $cargoPath
} elseif (Get-Command cargo -ErrorAction SilentlyContinue) {
    $cargo = 'cargo'
} else {
    throw 'Cargo was not found. Install Rust with rustup, then restart PowerShell.'
}

# Keep the client and its child backend attached to this console while using
# the development launcher.  This makes capture and VAD diagnostics visible
# during reproduction without changing packaged-app behaviour.
if ([string]::IsNullOrWhiteSpace($env:RUST_LOG)) {
    $env:RUST_LOG = 'info'
}
$env:XRTRANSLATE_BACKEND_CONSOLE_LOG = '1'

$buildArguments = @('build', '--manifest-path', $workspaceManifestPath, '--package', 'xrtranslate-backend')
if ($Release) {
    $buildArguments += '--release'
}

Write-Host 'Preparing the local translation service...'
& $cargo @buildArguments
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$runArguments = @('run', '--manifest-path', $manifestPath)
if ($Release) {
    $runArguments += '--release'
}

Write-Host 'Starting XRTranslate...'
Write-Host "Logging enabled (RUST_LOG=$env:RUST_LOG). Press Ctrl+C to stop."
& $cargo @runArguments
exit $LASTEXITCODE
