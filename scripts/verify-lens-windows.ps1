# Repeatable Slate Lens visual verification on Windows.
# Requires a release build: cargo build --release -p slate
# Usage: .\scripts\verify-lens-windows.ps1 [-OutDir <path>]

param(
    [string]$OutDir = ""
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$release = Join-Path $repo "target\release"
$slate = Join-Path $release "slate.exe"
$smoke = Join-Path $repo ".lens-smoke.slate"

if (-not (Test-Path $slate)) {
    throw "Missing $slate — run: cargo build --release -p slate"
}
if (-not (Test-Path $smoke)) {
    throw "Missing $smoke — create or restore the Lens smoke workbook."
}
if (-not (Test-Path (Join-Path $release "pdfium.dll"))) {
    Copy-Item (Join-Path $repo "pdfium.dll") $release -Force -ErrorAction SilentlyContinue
}

if ([string]::IsNullOrWhiteSpace($OutDir)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $OutDir = Join-Path $release "lens-verification-$stamp"
}
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null

function Invoke-LensShot {
    param(
        [string]$Name,
        [string]$Focus = "",
        [uint64]$Delay = 240
    )
    $path = Join-Path $OutDir $Name
    $env:SLATE_SHOT = "$path;$Delay"
    if ($Focus) {
        $env:SLATE_LENS_FOCUS = $Focus
    } else {
        Remove-Item Env:SLATE_LENS_FOCUS -ErrorAction SilentlyContinue
    }
    $proc = Start-Process -FilePath $slate -ArgumentList "`"$smoke`"" -WorkingDirectory $release -PassThru -Wait
    if ($proc.ExitCode -ne 0) {
        throw "Slate exited $($proc.ExitCode) while capturing $Name"
    }
    if (-not (Test-Path $path)) {
        throw "Screenshot missing: $path"
    }
    Write-Host "OK $Name"
}

Write-Host "Capturing Lens verification shots to $OutDir"
Invoke-LensShot -Name "01-ready.png" -Delay 240
Invoke-LensShot -Name "03-package-focused.png" -Focus "slate" -Delay 240
Invoke-LensShot -Name "06-file-weighted-focused.png" -Focus "menubar.rs" -Delay 240
Invoke-LensShot -Name "07-no-direct-dependencies.png" -Focus "lib.rs" -Delay 240
Write-Host "Done — $($OutDir)"
