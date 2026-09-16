[CmdletBinding()]
param([Parameter(Mandatory)][string]$AppDirectory, [Parameter(Mandatory)][string]$ReleaseDirectory,
    [Parameter(Mandatory)][string]$Channel, [Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'
foreach ($file in @('slate.exe', 'native-file-atlas.exe', 'pdfium.dll', 'runtime/node.exe', 'runtime/node_modules/npm/bin/npm-cli.js',
    'runtime/LICENSE', 'cursor-sidecar/index.mjs', 'cursor-sidecar/package-lock.json', 'PDFium-LICENSE', 'install-shortcuts.ps1')) {
    if (-not (Test-Path -LiteralPath (Join-Path $AppDirectory $file) -PathType Leaf)) { throw "Package is missing $file" }
}
$distribution = Get-Content (Join-Path $AppDirectory 'atlas-release.json') -Raw | ConvertFrom-Json
if ($distribution.channel -ne $Channel -or $distribution.version -ne $Version) { throw 'Release metadata mismatch.' }
$feed = Get-Content (Join-Path $ReleaseDirectory "releases.$Channel.json") -Raw | ConvertFrom-Json
$full = @($feed.Assets | Where-Object { $_.Type -eq 'Full' -and $_.Version -eq $Version })
if ($full.Count -ne 1) { throw 'Expected exactly one full update in the feed.' }
$package = Join-Path $ReleaseDirectory $full[0].FileName
if ((Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash -ne $full[0].SHA256) { throw 'Release feed checksum mismatch.' }
if ((Get-Item -LiteralPath $package).Length -ne $full[0].Size) { throw 'Release feed size mismatch.' }
if (@(Get-ChildItem -LiteralPath $ReleaseDirectory -Filter '*Setup.exe').Count -ne 1) { throw 'Installer is missing.' }
Write-Output 'Verified suite contents, channel/version, setup executable and update package checksum.'
