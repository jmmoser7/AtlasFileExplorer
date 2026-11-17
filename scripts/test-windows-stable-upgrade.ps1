# Test the public stable install and the exact candidate assets on a disposable runner.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ReleaseDirectory,
    [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version
)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or !$env:RUNNER_TEMP) {
    throw 'This test installs stable Slate and File Atlas shortcuts. Run it on a disposable Windows CI runner.'
}
$repo = Split-Path -Parent $PSScriptRoot
$release = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
$fixture = Join-Path $repo 'target/debug/examples/update_smoke.exe'
$test = Join-Path $env:RUNNER_TEMP ("atlas-stable-smoke-" + [Guid]::NewGuid().ToString('N'))
$previousRoot = Join-Path $test 'upgraded'
$cleanRoot = Join-Path $test 'clean'
New-Item -Path $test -ItemType Directory | Out-Null

function Assert-Artifact([string]$Path, [long]$Size, [string]$Sha) {
    if ($Sha -notmatch '^[a-fA-F0-9]{64}$' -or
        (Get-Item -LiteralPath $Path).Length -ne $Size -or
        (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $Sha) {
        throw "Unexpected size or SHA-256 for $Path."
    }
}

function Install-Suite([string]$Setup, [string]$Root) {
    $log = Join-Path $test ("setup-" + (Split-Path -Leaf $Root) + ".log")
    $setupProcess = Start-Process -FilePath $Setup -ArgumentList @(
        '--silent', '--installto', ('"' + $Root + '"'), '--log', ('"' + $log + '"')
    ) -WindowStyle Hidden -PassThru
    if (!$setupProcess.WaitForExit(120000) -or $setupProcess.ExitCode -ne 0) {
        if (!$setupProcess.HasExited) { $setupProcess.Kill() }
        Get-Content -LiteralPath $log -Tail 50 -ErrorAction SilentlyContinue
        throw "Suite setup failed for $Root."
    }
}

function Assert-Installed([string]$Root, [string]$ExpectedVersion) {
    $config = Get-Content -LiteralPath (Join-Path $Root 'current/atlas-release.json') -Raw | ConvertFrom-Json
    if ($config.channel -ne 'stable' -or $config.version -ne $ExpectedVersion) {
        throw "Expected stable $ExpectedVersion, found $($config.channel) $($config.version) in $Root."
    }
    foreach ($name in @('Update.exe', 'current/sq.version', 'current/slate.exe', 'current/native-file-atlas.exe')) {
        if (!(Test-Path -LiteralPath (Join-Path $Root $name) -PathType Leaf)) {
            throw "Installed suite is missing $name."
        }
    }
}

function Wait-ForFile([string]$Path) {
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    while (!(Test-Path -LiteralPath $Path) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if (!(Test-Path -LiteralPath $Path)) { throw "Timed out waiting for $Path." }
}

function Assert-AppBytes([string]$Root, $ExpectedHashes) {
    foreach ($name in $ExpectedHashes.Keys) {
        $path = Join-Path $Root "current/$name"
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $ExpectedHashes[$name]) {
            throw "$name in $Root differs from the generated update package."
        }
    }
}

$repository = 'https://github.com/jmmoser7/AtlasFileExplorer'
$latest = Invoke-RestMethod -Uri 'https://api.github.com/repos/jmmoser7/AtlasFileExplorer/releases/latest' -TimeoutSec 30
if ($latest.draft -or $latest.prerelease -or $latest.tag_name -notmatch '^v(\d+\.\d+\.\d+)$') {
    throw 'The published stable baseline could not be identified.'
}
$previous = $Matches[1]
if ([version]$previous -ge [version]$Version) { throw 'Expected a stable upgrade from the published version.' }
$setupName = 'AtlasSuite-stable-Setup.exe'
$baseline = @($latest.assets | Where-Object name -eq $setupName)
$baselineUrl = "$repository/releases/download/v$previous/$setupName"
if ($baseline.Count -ne 1 -or $baseline[0].browser_download_url -ne $baselineUrl) {
    throw 'The published stable release is missing its versioned installer.'
}
$baselineSetup = Join-Path $test 'previous-Setup.exe'
Invoke-WebRequest -Uri $baselineUrl -OutFile $baselineSetup -TimeoutSec 180
if ($baseline[0].digest -notmatch '^sha256:([a-fA-F0-9]{64})$') {
    throw 'GitHub did not provide the stable installer digest.'
}
Assert-Artifact $baselineSetup $baseline[0].size $Matches[1]

$feed = Get-Content -LiteralPath (Join-Path $release 'releases.stable.json') -Raw | ConvertFrom-Json
$packageName = "AtlasSuite-$Version-stable-full.nupkg"
$full = @($feed.Assets | Where-Object {
    $_.PackageId -eq 'AtlasSuite' -and $_.Version -eq $Version -and
    $_.Type -eq 'Full' -and $_.FileName -eq $packageName
})
if ($full.Count -ne 1) { throw 'Expected exactly one matching candidate full update.' }
$asset = $full[0]
$package = Join-Path $release $packageName
Assert-Artifact $package $asset.Size $asset.SHA256
$hashes = @{}
$zip = [IO.Compression.ZipFile]::OpenRead($package)
try {
    foreach ($name in @('slate.exe', 'native-file-atlas.exe', 'slate-video-worker.exe')) {
        $entry = $zip.GetEntry("lib/app/$name")
        if (!$entry) { throw "The candidate update archive does not contain $name." }
        $stream = $entry.Open()
        try { $hashes[$name] = (Get-FileHash -InputStream $stream -Algorithm SHA256).Hash }
        finally { $stream.Dispose() }
    }
} finally { $zip.Dispose() }

Install-Suite $baselineSetup $previousRoot
Assert-Installed $previousRoot $previous
Copy-Item -LiteralPath $fixture -Destination (Join-Path $previousRoot 'current/update_smoke.exe')
$probe = Join-Path $previousRoot 'current/update_smoke.exe'
$workbook = Join-Path $test 'preserved-workbook.slate'
Copy-Item -LiteralPath (Join-Path $repo 'docs/audit/2026-09-15-evidence/staging-accepted.slate') -Destination $workbook
$workbookHash = (Get-FileHash -LiteralPath $workbook -Algorithm SHA256).Hash

$holders = @()
$downloader = $null
$coordinator = $null
try {
    foreach ($number in 1, 2) {
        $holders += Start-Process -FilePath $probe -ArgumentList @(
            ('"' + (Join-Path $test "stop-$number") + '"'),
            ('"' + (Join-Path $test "running-$number") + '"')
        ) -WindowStyle Hidden -PassThru
        Wait-ForFile (Join-Path $test "running-$number")
    }
    $downloader = Start-Process -FilePath $probe -ArgumentList @(
        '--stage-stable-upgrade', ('"' + $release + '"'), $previous, $Version
    ) -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $test 'download.log')
    if (!$downloader.WaitForExit(60000) -or $downloader.ExitCode -ne 0) {
        Get-Content -LiteralPath (Join-Path $test 'download.log') -ErrorAction SilentlyContinue
        throw 'Velopack did not discover and stage the candidate update.'
    }
    Assert-Artifact (Join-Path $previousRoot "packages/$packageName") $asset.Size $asset.SHA256

    $request = Join-Path $previousRoot 'request.json'
    @{ package = $packageName; sha256 = $asset.SHA256; size = $asset.Size; restart = 'slate.exe' } |
        ConvertTo-Json | Set-Content -LiteralPath $request -Encoding utf8
    $helper = Join-Path $repo 'crates/atlas-update/src/install-update.ps1'
    $coordinator = Start-Process -FilePath powershell.exe -ArgumentList @(
        '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', ('"' + $helper + '"'),
        '-InstallRoot', ('"' + $previousRoot + '"'), '-RequestPath', ('"' + $request + '"')
    ) -WindowStyle Hidden -PassThru
    Wait-ForFile (Join-Path $previousRoot 'request.ready')
    Assert-Installed $previousRoot $previous
    'stop' | Set-Content -LiteralPath (Join-Path $test 'stop-1')
    if (!$holders[0].WaitForExit(10000) -or $holders[0].ExitCode -ne 0) { throw 'The first guard holder did not exit.' }
    Start-Sleep -Milliseconds 750
    Assert-Installed $previousRoot $previous
    if ($holders[1].HasExited) { throw 'The second guard holder did not survive the first exit.' }
    'stop' | Set-Content -LiteralPath (Join-Path $test 'stop-2')
    if (!$holders[1].WaitForExit(10000) -or $holders[1].ExitCode -ne 0) { throw 'The second guard holder did not exit.' }
    if (!$coordinator.WaitForExit(90000) -or $coordinator.ExitCode -ne 0) {
        Get-Content -LiteralPath (Join-Path $previousRoot 'atlas-update-error.txt') -ErrorAction SilentlyContinue
        throw 'The production coordinator could not apply the candidate package.'
    }
    Assert-Installed $previousRoot $Version
    Assert-AppBytes $previousRoot $hashes
    if ((Get-FileHash -LiteralPath $workbook -Algorithm SHA256).Hash -ne $workbookHash) {
        throw 'Upgrade modified the workbook.'
    }

    $slatePath = Join-Path $previousRoot 'current/slate.exe'
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $restarted = @(Get-Process -Name slate -ErrorAction SilentlyContinue | Where-Object Path -eq $slatePath)
        if ($restarted.Count -gt 0) { break }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($restarted.Count -eq 0) { throw 'The coordinator did not restart the updated Slate process.' }
    Start-Sleep -Seconds 2
    if (!(Get-Process -Name slate -ErrorAction SilentlyContinue | Where-Object Path -eq $slatePath)) {
        throw 'The restarted Slate process exited during startup.'
    }

    Install-Suite (Join-Path $release $setupName) $cleanRoot
    Assert-Installed $cleanRoot $Version
    Assert-AppBytes $cleanRoot $hashes
    Write-Output "PASS: published stable $previous installer, candidate $Version clean install, coordinated upgrade, restart and workbook preservation."
} finally {
    foreach ($process in ($holders + @($downloader, $coordinator))) {
        if ($null -ne $process -and !$process.HasExited) { $process.Kill() }
    }
    # Only the two runner-owned installations can have matching app processes.
    $testPrefix = [IO.Path]::GetFullPath($test) + [IO.Path]::DirectorySeparatorChar
    Get-Process -Name slate,native-file-atlas -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and $_.Path.StartsWith($testPrefix, [StringComparison]::OrdinalIgnoreCase) } |
        ForEach-Object { if (!$_.HasExited) { $_.Kill() } }
}
