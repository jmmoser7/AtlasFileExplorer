# Fast Slate edit loop: save → incremental build → relaunch (dev profile).
# See docs/dev-loop.md. Does not alter app runtime behavior.
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

. (Join-Path $PSScriptRoot "_msvc-env.ps1")

if (Get-Command bacon -ErrorAction SilentlyContinue) {
    bacon slate @args
    exit $LASTEXITCODE
}

if (Get-Command cargo-watch -ErrorAction SilentlyContinue) {
    cargo watch -x "run -p slate" @args
    exit $LASTEXITCODE
}

Write-Host @"
Install a watcher once, then re-run this script:

  cargo install --locked bacon
  .\scripts\dev-slate.ps1

Or without a watcher:

  cargo run -p slate
"@
exit 1
