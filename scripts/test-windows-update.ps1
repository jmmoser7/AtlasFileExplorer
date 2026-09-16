# Real install/upgrade in a disposable CI installation. Fixture processes use
# the production startup guard. No user's actual installation is touched.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$scratch = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { Join-Path $repo 'target' }
$test = Join-Path $scratch ("update-smoke-" + [Guid]::NewGuid().ToString('N'))
$stage = Join-Path $test 'stage'
$install = Join-Path $test 'installed'
$first = Join-Path $test 'first'
$second = Join-Path $test 'second'
New-Item -ItemType Directory -Force -Path $stage, $first, $second | Out-Null
cargo build --locked -p atlas-update --example update_smoke
if ($LASTEXITCODE -ne 0) { throw 'Smoke fixture build failed.' }
foreach ($exe in @('slate.exe', 'native-file-atlas.exe')) {
    Copy-Item (Join-Path $repo 'target/debug/examples/update_smoke.exe') (Join-Path $stage $exe)
}
'{"channel":"stable"}' | Set-Content (Join-Path $stage 'atlas-release.json')
'param($InstallRoot, $Remove)' | Set-Content (Join-Path $stage 'install-shortcuts.ps1')
foreach ($version in @('0.0.1', '0.0.2')) {
    $output = if ($version -eq '0.0.1') { $first } else { $second }
    $version | Set-Content (Join-Path $stage 'smoke-version.txt')
    vpk pack --packId AtlasUpdateSmoke --packVersion $version --packDir $stage --mainExe slate.exe --outputDir $output --channel stable --delta None --shortcuts None
    if ($LASTEXITCODE -ne 0) { throw "Smoke packaging $version failed." }
}
$setup = Get-ChildItem -LiteralPath $first -Filter '*Setup.exe' | Select-Object -First 1
$setupProcess = Start-Process -FilePath $setup.FullName -ArgumentList @('--silent', '--installto', ('"' + $install + '"')) -WindowStyle Hidden -PassThru
if (-not $setupProcess.WaitForExit(60000) -or $setupProcess.ExitCode -ne 0) { throw 'Smoke installation failed.' }
if (-not (Test-Path (Join-Path $install 'current/slate.exe'))) { throw 'Missing installed app.' }
'keep this workbook' | Set-Content (Join-Path $install 'saved-workbook.slate')
$asset = (Get-Content (Join-Path $second 'releases.stable.json') -Raw | ConvertFrom-Json).Assets | Where-Object Type -eq Full
Copy-Item (Join-Path $second $asset.FileName) (Join-Path $install 'packages')
$request = Join-Path $install 'request.json'
@{ package = $asset.FileName; sha256 = $asset.SHA256; size = $asset.Size; restart = 'slate.exe' } | ConvertTo-Json | Set-Content $request
$helper = Join-Path $install 'atlas-install-update.ps1'
Copy-Item (Join-Path $repo 'crates/atlas-update/src/install-update.ps1') $helper
function Wait-ForFile([string]$Path) {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while (-not (Test-Path -LiteralPath $Path) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    if (-not (Test-Path -LiteralPath $Path)) { throw "Timed out waiting for $Path" }
}
function Assert-Version([string]$Expected) {
    $actual = (Get-Content (Join-Path $install 'current/smoke-version.txt') -Raw).Trim()
    if ($actual -ne $Expected) { throw "Expected $Expected, found $actual" }
}
$one = $null
$two = $null
$coordinator = $null
try {
    $one = Start-Process -FilePath (Join-Path $install 'current/slate.exe') -ArgumentList @(('"' + $test + '/stop-one"'), ('"' + $test + '/ready-one"')) -WindowStyle Hidden -PassThru
    $two = Start-Process -FilePath (Join-Path $install 'current/native-file-atlas.exe') -ArgumentList @(('"' + $test + '/stop-two"'), ('"' + $test + '/ready-two"')) -WindowStyle Hidden -PassThru
    Wait-ForFile (Join-Path $test 'ready-one')
    Wait-ForFile (Join-Path $test 'ready-two')
    # Setup may launch the first version; only the upgrade's restart counts.
    $restartMarker = Join-Path $install 'restarted-version.txt'
    if (Test-Path -LiteralPath $restartMarker) { Remove-Item -LiteralPath $restartMarker }
    $coordinator = Start-Process powershell.exe -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', ('"' + $helper + '"'), '-InstallRoot', ('"' + $install + '"'), '-RequestPath', ('"' + $request + '"')) -WindowStyle Hidden -PassThru
    Wait-ForFile (Join-Path $install 'request.ready')
    Assert-Version '0.0.1'
    'stop' | Set-Content (Join-Path $test 'stop-one')
    if (-not $one.WaitForExit(10000)) { throw 'First app failed to close normally.' }
    Start-Sleep -Milliseconds 800
    Assert-Version '0.0.1'
    if ($two.HasExited) { throw 'Updater terminated the other app.' }
    'stop' | Set-Content (Join-Path $test 'stop-two')
    if (-not $two.WaitForExit(10000)) { throw 'Second app failed to close normally.' }
    if (-not $coordinator.WaitForExit(60000) -or $coordinator.ExitCode -ne 0) {
        Get-Content (Join-Path $install 'atlas-update-error.txt') -ErrorAction SilentlyContinue
        throw 'Coordinated update failed.'
    }
    Wait-ForFile (Join-Path $install 'restarted-version.txt')
    Assert-Version '0.0.2'
    if ((Get-Content (Join-Path $install 'restarted-version.txt') -Raw).Trim() -ne '0.0.2') { throw 'Updated app did not restart.' }
    if ((Get-Content (Join-Path $install 'saved-workbook.slate') -Raw).Trim() -ne 'keep this workbook') { throw 'Update touched user data.' }
    Write-Output 'PASS: install, two-process wait, real upgrade, restart and saved data.'
} finally {
    # Only isolated fixture processes created above are eligible for cleanup.
    foreach ($process in @($one, $two, $coordinator)) {
        if ($null -ne $process -and -not $process.HasExited) { $process.Kill() }
    }
}
