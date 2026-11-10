# Exercise the production verifier with disposable payload/feed fixtures only.
# These are not executable installers or evidence of native runtime behavior.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$verifier = Join-Path $repo 'scripts/verify-windows-package.ps1'
$fixtureRoot = Join-Path $repo ('target/package-verifier-' + [Guid]::NewGuid().ToString('N'))
$stage = Join-Path $fixtureRoot 'app'
$release = Join-Path $fixtureRoot 'Releases'
$checks = 0

function Write-Fixture([string]$Path, [string]$Content = 'fixture') {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
    [IO.File]::WriteAllText($Path, $Content)
}
function Verify([string]$Channel) {
    & $verifier -AppDirectory $stage -ReleaseDirectory $release -Channel $Channel -Version '0.0.1' | Out-Null
}
function Expect-Rejection([string]$Channel, [string]$Expected) {
    $message = $null
    try { Verify $Channel } catch { $message = $_.Exception.Message }
    if (-not $message -or -not $message.Contains($Expected)) {
        throw "Expected '$Expected' for $Channel, got '$message'."
    }
}

try {
    foreach ($file in @('slate.exe', 'native-file-atlas.exe', 'slate-video-worker.exe',
        'HeadlessRhino.exe', 'HeadlessRhino-README.md', 'pdfium.dll', 'PDFium-LICENSE',
        'DEJAVU-LICENSE.txt', 'install-shortcuts.ps1', 'runtime/node.exe', 'runtime/LICENSE',
        'runtime/node_modules/npm/bin/npm-cli.js', 'cursor-sidecar/index.mjs',
        'cursor-sidecar/artifacts.mjs', 'cursor-sidecar/artifact-generation-v1.md',
        'cursor-sidecar/board-link.mjs', 'cursor-sidecar/board-link-v1.md',
        'cursor-sidecar/attachments.mjs', 'cursor-sidecar/models.mjs',
        'cursor-sidecar/activity.mjs', 'cursor-sidecar/policy.mjs',
        'cursor-sidecar/install.mjs', 'cursor-sidecar/supervise.mjs',
        'cursor-sidecar/package-lock.json')) {
        Write-Fixture (Join-Path $stage $file)
    }
    foreach ($notice in @('OFL.txt', 'UFL.txt', 'emoji-icon-font-mit-license.txt')) {
        Write-Fixture (Join-Path $stage "Rust-licenses/epaint_default_fonts-fixture/fonts/$notice")
    }
    $package = Join-Path $release 'fixture.nupkg'
    Write-Fixture $package 'fixture package bytes'
    Write-Fixture (Join-Path $release 'FixtureSetup.exe')
    foreach ($channel in @('stable', 'preview')) {
        Write-Fixture (Join-Path $stage 'atlas-release.json') (@{channel=$channel;version='0.0.1'} | ConvertTo-Json)
        $asset = @{Type='Full';Version='0.0.1';FileName='fixture.nupkg';
            SHA256=(Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash;
            Size=(Get-Item -LiteralPath $package).Length}
        Write-Fixture (Join-Path $release "releases.$channel.json") (@{Assets=@($asset)} | ConvertTo-Json -Depth 4)
        Verify $channel
        $checks++
        foreach ($required in @('HeadlessRhino.exe', 'HeadlessRhino-README.md', 'slate-video-worker.exe')) {
            $path = Join-Path $stage $required
            Remove-Item -LiteralPath $path
            Expect-Rejection $channel "Package is missing $required"
            $checks++
            Write-Fixture $path
        }
        foreach ($binary in @('RhinoCommon.dll', 'Rhino.exe', 'rHcOmMoN_c.DlL', 'RhinoLibrary.dll')) {
            $path = Join-Path $stage "runtime/nested/$binary"
            Write-Fixture $path
            Expect-Rejection $channel 'Commercial Rhino installation binary must not be redistributed'
            $checks++
            Remove-Item -LiteralPath $path
        }
    }
    Write-Output "PASS: $checks production package-verifier checks; no native executables launched."
} finally {
    # Delete only this generated fixture tree within the repository's target.
    $resolvedRoot = [IO.Path]::GetFullPath($fixtureRoot)
    $targetPrefix = [IO.Path]::GetFullPath((Join-Path $repo 'target')) + [IO.Path]::DirectorySeparatorChar
    if (-not $resolvedRoot.StartsWith($targetPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Fixture cleanup escaped the intended target directory.'
    }
    if (Test-Path -LiteralPath $resolvedRoot) { Remove-Item -LiteralPath $resolvedRoot -Recurse -Force }
}
