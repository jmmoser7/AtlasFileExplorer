# Build + launch Slate and File Atlas (dev). Used by `bacon both`.
# Stays running until both apps exit or bacon kills this process.
# See docs/dev-loop.md.
[CmdletBinding()]
param(
    [ValidateSet('both', 'slate', 'native-file-atlas')][string]$App = 'both',
    [switch]$Release,
    [switch]$UiTuner,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$AppArguments
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
. (Join-Path $PSScriptRoot "_msvc-env.ps1")

# Bacon and cargo-watch use this same owner for single-app relaunches. Cargo
# does not build a dependency's binary target when it builds the application.
if ($App -ne 'both') {
    $helperArgs = @('build', '-p', 'atlas-core', '--bin', 'slate-video-worker', '--color', 'always')
    $runArgs = @('run', '-p', $App, '--color', 'always')
    if ($Release) { $helperArgs += '--release'; $runArgs += '--release' }
    if ($UiTuner) { $runArgs += @('--features', 'ui-tuner') }
    cargo @helperArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo @runArgs -- @AppArguments
    exit $LASTEXITCODE
}
if ($Release -or $UiTuner -or $AppArguments) {
    throw 'Release, UiTuner and AppArguments apply to a single -App only.'
}

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

cargo build -p slate -p native-file-atlas -p atlas-core --bin slate --bin native-file-atlas --bin slate-video-worker
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

& (Join-Path $PSScriptRoot "install-shortcuts.ps1") -Configuration Debug

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
