# Build the two desktop applications and refresh their Desktop shortcuts.
[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
. (Join-Path $PSScriptRoot "_msvc-env.ps1")

cargo build --release -p slate -p native-file-atlas
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

& (Join-Path $PSScriptRoot "install-shortcuts.ps1") -Configuration Release
