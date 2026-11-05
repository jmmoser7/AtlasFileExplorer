# Fast File Atlas edit loop: save → incremental build → relaunch (dev profile).
# See docs/dev-loop.md. Does not alter app runtime behavior.
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

. (Join-Path $PSScriptRoot "_msvc-env.ps1")

if (Get-Command bacon -ErrorAction SilentlyContinue) {
    bacon atlas @args
    exit $LASTEXITCODE
}

if (Get-Command cargo-watch -ErrorAction SilentlyContinue) {
    cargo watch -s "powershell -NoProfile -File scripts/run-both-dev.ps1 -App native-file-atlas" @args
    exit $LASTEXITCODE
}

Write-Host @"
Install a watcher once, then re-run this script:

  cargo install --locked bacon
  .\scripts\dev-atlas.ps1

Or without a watcher:

  .\scripts\run-both-dev.ps1 -App native-file-atlas
"@
exit 1
