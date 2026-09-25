# Reclaim disk from a Cargo target directory without creating work for the
# managed-machine auditor (docs/windows-builds.md).
#
# Removes only:
#   1. <profile>\incremental      compiler reuse cache; holds no executables.
#   2. Older copies of this workspace's own artifacts in <profile>\deps. Those
#      are rebuilt after every source change anyway; the newest copy of each
#      is kept.
# Third-party artifacts, build-script outputs, and proc-macro DLLs are never
# touched: rebuilding them creates executables that need fresh review.
#
#   .\scripts\prune-target.ps1 -DryRun
#   .\scripts\prune-target.ps1 -Target ..\Slate-wt-feature\target
param(
    [string]$Target = (Join-Path $PSScriptRoot '..\target'),
    [string[]]$Profiles = @('debug', 'release'),
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$Target = (Resolve-Path -LiteralPath $Target).Path

function Get-TreeBytes([string]$Path) {
    $sum = (Get-ChildItem -LiteralPath $Path -Recurse -File -Force -ErrorAction SilentlyContinue |
        Measure-Object Length -Sum).Sum
    if ($sum) { [int64]$sum } else { [int64]0 }
}

$repo = Split-Path -Parent $PSScriptRoot
$meta = cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $repo 'Cargo.toml') | ConvertFrom-Json
$members = $meta.packages | ForEach-Object { $_.name -replace '-', '_' }
# A workspace target whose name a dependency also uses is ambiguous in deps\; leave it.
$thirdParty = Select-String -LiteralPath (Join-Path $repo 'Cargo.lock') -Pattern '^name = "(.+)"' |
    ForEach-Object { $_.Matches[0].Groups[1].Value -replace '-', '_' } |
    Where-Object { $members -notcontains $_ }
$own = New-Object 'System.Collections.Generic.HashSet[string]'
foreach ($pkg in $meta.packages) {
    foreach ($t in $pkg.targets) {
        $name = $t.name -replace '-', '_'
        if ($name -eq 'build_script_build' -or $thirdParty -contains $name) { continue }
        [void]$own.Add($name)
        [void]$own.Add("lib$name")
    }
}

$freed = [int64]0
foreach ($profile in $Profiles) {
    $root = Join-Path $Target $profile
    if (-not (Test-Path -LiteralPath $root)) { continue }

    $incremental = Join-Path $root 'incremental'
    if (Test-Path -LiteralPath $incremental) {
        foreach ($dir in Get-ChildItem -LiteralPath $incremental -Directory -Force) {
            $bytes = Get-TreeBytes $dir.FullName
            $freed += $bytes
            if (-not $DryRun) { Remove-Item -LiteralPath $dir.FullName -Recurse -Force -ErrorAction SilentlyContinue }
        }
    }

    $deps = Join-Path $root 'deps'
    if (-not (Test-Path -LiteralPath $deps)) { continue }
    $ownFiles = Get-ChildItem -LiteralPath $deps -File -Force | Where-Object {
        $_.Name -match '^(?<stem>.+)-(?<hash>[0-9a-f]{16})\.' -and $own.Contains($Matches.stem)
    }
    foreach ($group in $ownFiles | Group-Object { ($_.Name -replace '-[0-9a-f]{16}\..*$', '') }) {
        # One build writes several files per hash (.exe, .pdb, .d, .rlib, ...); keep the newest hash whole.
        $newest = $group.Group | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        $keep = ([regex]::Match($newest.Name, '-([0-9a-f]{16})\.')).Groups[1].Value
        foreach ($file in $group.Group) {
            if ($file.Name -like "*-$keep.*") { continue }
            $freed += $file.Length
            if (-not $DryRun) { Remove-Item -LiteralPath $file.FullName -Force -ErrorAction SilentlyContinue }
        }
    }
}

$verb = if ($DryRun) { 'Would free' } else { 'Freed' }
'{0} {1:N1} GB from {2}' -f $verb, ($freed / 1GB), $Target
