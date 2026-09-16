# Keep this outside current/: Velopack replaces current/ during installation.
# The suite holds shared Windows file handles for its entire process lifetime.
# Only install when ALL those handles have closed voluntarily. Never kill an app.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$InstallRoot, [Parameter(Mandatory)][string]$RequestPath)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath($InstallRoot)
$requestFile = [IO.Path]::GetFullPath($RequestPath)
$coordinator = $null
$running = $null
try {
    if ([IO.Path]::GetDirectoryName($requestFile) -ne $root) { throw 'Invalid request path.' }
    $coordinator = [IO.File]::Open((Join-Path $root 'atlas-install.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
    $request = Get-Content -LiteralPath $requestFile -Raw | ConvertFrom-Json
    if ($request.package -notmatch '^[^/\\:]+-full\.nupkg$' -or $request.sha256 -notmatch '^[a-fA-F0-9]{64}$') { throw 'Invalid package metadata.' }
    if ($request.restart -notin @('slate.exe', 'native-file-atlas.exe')) { throw 'Invalid restart application.' }
    $package = Join-Path (Join-Path $root 'packages') $request.package
    # Reverify even cached downloads (including SDK downloads skipped as existing).
    $stream = [IO.File]::OpenRead($package)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '') }
    finally { $stream.Dispose(); $sha.Dispose() }
    if ((Get-Item -LiteralPath $package).Length -ne $request.size -or $hash -ne $request.sha256) {
        Remove-Item -LiteralPath $package
        throw 'Update checksum failed. Download the update again.'
    }
    [IO.File]::WriteAllText([IO.Path]::ChangeExtension($requestFile, 'ready'), 'ready')
    $deadline = [DateTime]::UtcNow.AddMinutes(30)
    while ($null -eq $running -and [DateTime]::UtcNow -lt $deadline) {
        try { $running = [IO.File]::Open((Join-Path $root 'atlas-running.lock'), 'OpenOrCreate', 'ReadWrite', 'None') }
        catch [IO.IOException] { Start-Sleep -Milliseconds 500 }
    }
    if ($null -eq $running) { throw 'Update cancelled: close all Slate and File Atlas windows, then try again.' }
    # Framework-owned installation/rollback. No app restart until the exclusive
    # handle is released, and no Update.exe force-close while a workbook is open.
    & (Join-Path $root 'Update.exe') apply --package $package --norestart --root $root --packageDir (Join-Path $root 'packages')
    if ($LASTEXITCODE -ne 0) { throw "Installer failed with exit code $LASTEXITCODE." }
    $running.Dispose()
    $running = $null
    $errorFile = Join-Path $root 'atlas-update-error.txt'
    if (Test-Path -LiteralPath $errorFile) { Remove-Item -LiteralPath $errorFile }
    Start-Process -FilePath (Join-Path (Join-Path $root 'current') $request.restart) -WorkingDirectory $root
} catch {
    [IO.File]::WriteAllText((Join-Path $root 'atlas-update-error.txt'), $_.ToString())
    exit 1
} finally {
    if ($null -ne $running) { $running.Dispose() }
    if ($null -ne $coordinator) { $coordinator.Dispose() }
}
