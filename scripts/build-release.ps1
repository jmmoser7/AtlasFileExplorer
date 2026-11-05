# Build the two desktop applications and refresh their Desktop shortcuts.
[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
. (Join-Path $PSScriptRoot "_msvc-env.ps1")

cargo build --release -p slate -p native-file-atlas -p atlas-core --bin slate --bin native-file-atlas --bin slate-video-worker
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

& (Join-Path $root 'tools/headless-rhino/build.ps1') -OutputDirectory (Join-Path $root 'target/release') -SkipIfUnavailable
& (Join-Path $PSScriptRoot "install-shortcuts.ps1") -Configuration Release
