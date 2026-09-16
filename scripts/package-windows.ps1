# Reproducible, allowlisted distribution. Never package target/release wholesale.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+(-preview\.\d+)?$')][string]$Version,
    [ValidateSet('stable', 'preview')][string]$Channel = 'stable',
    [switch]$SkipBuild,
    [string]$Vpk = 'vpk',
    [string]$VpkDll
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo
. (Join-Path $PSScriptRoot '_msvc-env.ps1')
$env:ATLAS_RELEASE_VERSION = $Version
$env:ATLAS_RELEASE_CHANNEL = $Channel
if (-not $SkipBuild) {
    cargo build --locked --release -p slate -p native-file-atlas
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
}
$build = Join-Path $repo ("target/distribution/{0}-{1}-{2}" -f $Channel, $Version, [Guid]::NewGuid().ToString('N'))
$stage = Join-Path $build 'app'
$output = Join-Path $build 'Releases'
$downloads = Join-Path $repo 'target/distribution/downloads'
New-Item -ItemType Directory -Force -Path $stage, $output, $downloads | Out-Null

function Get-VerifiedArchive([string]$Url, [string]$Hash, [string]$Name) {
    $path = Join-Path $downloads $Name
    if (-not (Test-Path -LiteralPath $path)) { Invoke-WebRequest -Uri $Url -OutFile $path }
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $Hash) { throw "Checksum mismatch: $Name. Remove the cached archive and retry." }
    return $path
}

foreach ($exe in @('slate.exe', 'native-file-atlas.exe')) {
    Copy-Item -LiteralPath (Join-Path $repo "target/release/$exe") -Destination $stage
}
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination $stage
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'distribution/install-shortcuts.ps1') -Destination $stage
Copy-Item -LiteralPath (Join-Path $repo 'docs/distribution.md') -Destination (Join-Path $stage 'Distribution.md')
# Windows PowerShell 5's UTF-8 writer adds a BOM, which serde_json rejects.
[IO.File]::WriteAllText((Join-Path $stage 'atlas-release.json'), (@{ channel = $Channel; version = $Version } | ConvertTo-Json))

# Same PDFium version as vendor/pdfium.dll, with its complete license bundle.
$pdfArchive = Get-VerifiedArchive 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/7920/pdfium-win-x64.tgz' 'bf25149815b34b00042f48a886653d469c817529dd9cccabb4b509b6465a9526' 'pdfium-7920-win-x64.tgz'
$pdf = Join-Path $build 'pdfium'
New-Item -ItemType Directory -Path $pdf | Out-Null
tar -xzf $pdfArchive -C $pdf
if ($LASTEXITCODE -ne 0) { throw 'PDFium extraction failed.' }
Copy-Item -LiteralPath (Join-Path $pdf 'bin/pdfium.dll') -Destination $stage
Copy-Item -LiteralPath (Join-Path $pdf 'LICENSE') -Destination (Join-Path $stage 'PDFium-LICENSE')
Copy-Item -LiteralPath (Join-Path $pdf 'licenses') -Destination (Join-Path $stage 'PDFium-licenses') -Recurse

# Node/npm are private to the suite. Cursor's optional SDK is installed by the
# existing first-use flow, after the user has configured that integration.
$nodeArchive = Get-VerifiedArchive 'https://nodejs.org/dist/v24.21.0/node-v24.21.0-win-x64.zip' '158f7685b44de51f6c0df1d153526cbcd3e1bc739a8dfc607721cef75de9e541' 'node-v24.21.0-win-x64.zip'
Expand-Archive -LiteralPath $nodeArchive -DestinationPath $build
Copy-Item -LiteralPath (Join-Path $build 'node-v24.21.0-win-x64') -Destination (Join-Path $stage 'runtime') -Recurse
$sidecar = New-Item -ItemType Directory -Path (Join-Path $stage 'cursor-sidecar')
foreach ($file in @('index.mjs', 'package.json', 'package-lock.json', 'README.md', 'SETUP.md')) {
    Copy-Item -LiteralPath (Join-Path $repo "docs/agent/cursor-sidecar/$file") -Destination $sidecar.FullName
}

# Carry license texts for compiled Rust dependencies as well as the lockfile.
$licenses = New-Item -ItemType Directory -Path (Join-Path $stage 'Rust-licenses')
$metadata = cargo metadata --locked --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Dependency metadata failed.' }
$metadata.packages | Select-Object name, version, license, repository | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $licenses 'index.json') -Encoding utf8
foreach ($package in $metadata.packages) {
    $dir = Split-Path -Parent $package.manifest_path
    $texts = Get-ChildItem -LiteralPath $dir -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE)' }
    if ($texts) {
        $dest = New-Item -ItemType Directory -Force -Path (Join-Path $licenses "$($package.name)-$($package.version)")
        $texts | Copy-Item -Destination $dest.FullName
    }
}
Copy-Item -LiteralPath (Join-Path $repo 'Cargo.lock') -Destination $licenses.FullName

$id = if ($Channel -eq 'preview') { 'AtlasSuitePreview' } else { 'AtlasSuite' }
$title = if ($Channel -eq 'preview') { 'Slate Preview' } else { 'Slate' }
$packArgs = @('pack', '--packId', $id, '--packTitle', $title, '--packVersion', $Version,
    '--packDir', $stage, '--mainExe', 'slate.exe', '--outputDir', $output,
    '--channel', $Channel, '--runtime', 'win-x64', '--delta', 'None',
    '--icon', (Join-Path $repo 'apps/slate/assets/slate.ico'),
    '--framework', 'webview2,vcredist143-x64', '--shortcuts', 'Desktop,StartMenuRoot')
if ($env:WINDOWS_SIGN_CERTIFICATE) {
    $sign = '/fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /f "{0}" /p "{1}"' -f $env:WINDOWS_SIGN_CERTIFICATE, $env:WINDOWS_SIGN_PASSWORD
    $packArgs += @('--signParams', $sign)
}
if ($VpkDll) { dotnet $VpkDll @packArgs } else { & $Vpk @packArgs }
if ($LASTEXITCODE -ne 0) { throw 'Installer packaging failed.' }
& (Join-Path $PSScriptRoot 'verify-windows-package.ps1') -AppDirectory $stage -ReleaseDirectory $output -Channel $Channel -Version $Version
if ($env:GITHUB_OUTPUT) { "release_dir=$output" >> $env:GITHUB_OUTPUT }
Write-Output "Windows release: $output"
