# Build + launch Slate and File Atlas (dev). Used by `bacon both`.
# Stays running until both apps exit or bacon kills this process.
# See docs/dev-loop.md.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
. (Join-Path $PSScriptRoot "_msvc-env.ps1")

$debugDir = Join-Path $root "target\debug"

# Only the binaries this script launched. Matching on process name alone also
# killed a release build running alongside it — e.g. a `--features ui-tuner`
# instance kept open for token work, which is exactly what you do not want
# vanishing on every rebuild.
function Stop-DevApps {
    Get-Process -Name slate, native-file-atlas -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and $_.Path.StartsWith($debugDir, 'OrdinalIgnoreCase') } |
        Stop-Process -Force -ErrorAction SilentlyContinue
}

Stop-DevApps

cargo build -p slate -p native-file-atlas
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$slate = Join-Path $root "target\debug\slate.exe"
$atlas = Join-Path $root "target\debug\native-file-atlas.exe"
if (-not (Test-Path $slate) -or -not (Test-Path $atlas)) {
    Write-Error "Expected debug binaries missing under target\debug\"
    exit 1
}

$procs = @(
    (Start-Process -FilePath $slate -WorkingDirectory $root -PassThru),
    (Start-Process -FilePath $atlas -WorkingDirectory $root -PassThru)
)
Write-Host "Launched Slate (pid $($procs[0].Id)) and File Atlas (pid $($procs[1].Id))."

try {
    while ($true) {
        $alive = @($procs | Where-Object { -not $_.HasExited })
        if ($alive.Count -eq 0) {
            break
        }
        Start-Sleep -Seconds 1
    }
} finally {
    Stop-DevApps
}
