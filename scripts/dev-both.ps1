# Fast edit loop for BOTH apps: save → build Slate + File Atlas → relaunch.
# See docs/dev-loop.md. Does not alter app runtime behavior.
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
. (Join-Path $PSScriptRoot "_msvc-env.ps1")

if (Get-Command bacon -ErrorAction SilentlyContinue) {
    bacon both @args
    exit $LASTEXITCODE
}

Write-Host @"
Install bacon once, then re-run:

  cargo install --locked bacon
  .\scripts\dev-both.ps1

One-shot without a watcher:

  .\scripts\run-both-dev.ps1
"@
exit 1
