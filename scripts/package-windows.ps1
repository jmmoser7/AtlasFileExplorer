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
    cargo build --locked --release -p slate -p native-file-atlas -p atlas-core --bin slate --bin native-file-atlas --bin slate-video-worker
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
}
$metadata = cargo metadata --locked --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Dependency metadata failed.' }
$binaryDirectory = Join-Path $metadata.target_directory 'release'
# Even SkipBuild packages must include the helper; compile it from the local
# SDK or the pinned compile-only reference. Neither source is redistributed.
& (Join-Path $repo 'tools/headless-rhino/build.ps1') -OutputDirectory $binaryDirectory
# CI staging belongs outside Cargo's cached target tree; otherwise each release
# adds another complete Node/runtime/installer copy to the next build's cache.
$scratch = if ($env:RUNNER_TEMP) { Join-Path $env:RUNNER_TEMP 'atlas-distribution' } else { Join-Path $repo 'target/distribution' }
$build = Join-Path $scratch ("{0}-{1}-{2}" -f $Channel, $Version, [Guid]::NewGuid().ToString('N'))
$stage = Join-Path $build 'app'
$output = Join-Path $build 'Releases'
$downloads = Join-Path $scratch 'downloads'
New-Item -ItemType Directory -Force -Path $stage, $output, $downloads | Out-Null

function Get-VerifiedArchive([string]$Url, [string]$Hash, [string]$Name) {
    $path = Join-Path $downloads $Name
    if (-not (Test-Path -LiteralPath $path)) { Invoke-WebRequest -Uri $Url -OutFile $path }
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $Hash) { throw "Checksum mismatch: $Name. Remove the cached archive and retry." }
    return $path
}

foreach ($exe in @('slate.exe', 'native-file-atlas.exe', 'slate-video-worker.exe')) {
    Copy-Item -LiteralPath (Join-Path $binaryDirectory $exe) -Destination $stage
}
# Our compiled bridge only. Rhino's commercial SDK/native DLLs
# remain in the recipient's installed Rhino directory and are never packaged.
$rhinoHelper=Join-Path $binaryDirectory 'HeadlessRhino.exe'
Copy-Item -LiteralPath $rhinoHelper -Destination $stage
Copy-Item -LiteralPath (Join-Path $repo 'tools/headless-rhino/README.md') -Destination (Join-Path $stage 'HeadlessRhino-README.md')
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination $stage
Copy-Item -LiteralPath (Join-Path $repo 'apps/slate/assets/fonts/DEJAVU-LICENSE.txt') -Destination $stage
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

# Slate loads this matched Mesa WGL pair from its private runtime directory.
# Leave the system OpenGL installation and the suite's root DLL search alone.
$mesaBin = Get-VerifiedArchive 'https://github.com/pal1000/mesa-dist-win/releases/download/25.2.8/mesa3d-25.2.8-release-msvc.7z' 'e47cfe0be35f0c40c3d488c0e965d10656154ed55cfdfe4f4d79d4395672dc02' 'mesa3d-25.2.8-release-msvc.7z'
$mesaSource = Get-VerifiedArchive 'https://archive.mesa3d.org/mesa-25.2.8.tar.xz' '097842f3e49d996868b38688db87b006f7d4541e93ce86d2f341d8b3e7be7c93' 'mesa-25.2.8.tar.xz'
$llvmLicense = Get-VerifiedArchive 'https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-21.1.7/llvm/LICENSE.TXT' '8d85c1057d742e597985c7d4e6320b015a9139385cff4cbae06ffc0ebe89afee' 'llvm-21.1.7-LICENSE.txt'
$mesaRuntime = New-Item -ItemType Directory -Path (Join-Path $stage 'runtime/software-gl')
& tar.exe -xf $mesaBin -C $build 'x64/opengl32.dll' 'x64/libgallium_wgl.dll'
if ($LASTEXITCODE -ne 0) { throw 'Mesa Windows renderer extraction failed.' }
& tar.exe -xf $mesaSource -C $build 'mesa-25.2.8/licenses' 'mesa-25.2.8/docs/license.rst' 'mesa-25.2.8/src/util/xxhash.h'
if ($LASTEXITCODE -ne 0) { throw 'Mesa source license extraction failed.' }
foreach ($dll in @('opengl32.dll', 'libgallium_wgl.dll')) {
    Copy-Item -LiteralPath (Join-Path $build "x64/$dll") -Destination $mesaRuntime.FullName
}
Copy-Item -LiteralPath (Join-Path $build 'mesa-25.2.8/licenses') -Destination $mesaRuntime.FullName -Recurse
Copy-Item -LiteralPath (Join-Path $build 'mesa-25.2.8/docs/license.rst') -Destination (Join-Path $mesaRuntime.FullName 'Mesa-LICENSE.rst')
Copy-Item -LiteralPath (Join-Path $build 'mesa-25.2.8/src/util/xxhash.h') -Destination (Join-Path $mesaRuntime.FullName 'licenses/Mesa-xxhash-NOTICE.txt')
Copy-Item -LiteralPath $llvmLicense -Destination (Join-Path $mesaRuntime.FullName 'LLVM-LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'distribution/mesa-graphics-notice.txt') -Destination (Join-Path $mesaRuntime.FullName 'NOTICE.txt')
# The protected GPU backend uses the matching shader compiler and validator.
$dxc = Get-VerifiedArchive 'https://github.com/microsoft/DirectXShaderCompiler/releases/download/v1.8.2505.1/dxc_2025_07_14.zip' '9ad895a6b039e3a8f8c22a1009f866800b840a74b50db9218d13319e215ea8a4' 'dxc_2025_07_14.zip'
$shaderRuntime = New-Item -ItemType Directory -Path (Join-Path $build 'dxc')
$shaderNotices = @('LICENSE-LLVM.txt', 'LICENSE-MIT.txt', 'LICENSE-MS.txt', 'ReleaseNotes.md')
& tar.exe -xf $dxc -C $shaderRuntime.FullName 'bin/x64/dxcompiler.dll' 'bin/x64/dxil.dll' @shaderNotices
if ($LASTEXITCODE -ne 0) { throw 'DirectX shader runtime extraction failed.' }
foreach ($dll in @('dxcompiler.dll', 'dxil.dll')) {
    Copy-Item -LiteralPath (Join-Path $shaderRuntime.FullName "bin/x64/$dll") -Destination $mesaRuntime.FullName
}
$dxcLicenses = New-Item -ItemType Directory -Path (Join-Path $mesaRuntime.FullName 'licenses/DXC')
foreach ($notice in $shaderNotices) {
    Copy-Item -LiteralPath (Join-Path $shaderRuntime.FullName $notice) -Destination $dxcLicenses.FullName
}
$mesaNotices = @(
    @{ Name='zlib-1.3.1-LICENSE'; Url='https://raw.githubusercontent.com/madler/zlib/v1.3.1/LICENSE'; Sha='845efc77857d485d91fb3e0b884aaa929368c717ae8186b66fe1ed2495753243' },
    @{ Name='zstd-1.5.7-LICENSE'; Url='https://raw.githubusercontent.com/facebook/zstd/v1.5.7/LICENSE'; Sha='7055266497633c9025b777c78eb7235af13922117480ed5c674677adc381c9d8' },
    @{ Name='DirectX-Headers-1.618.2-LICENSE'; Url='https://raw.githubusercontent.com/microsoft/DirectX-Headers/v1.618.2/LICENSE'; Sha='903df5512f7d02609fed0c780a9b704f5a3eeb6e4d84ebe42a29845c81899a3c' },
    @{ Name='libva-2.22.0-COPYING'; Url='https://raw.githubusercontent.com/intel/libva/2.22.0/COPYING'; Sha='c86a782ee845b52472dae9b9d79fb915d333628ac0efe49cdce63644814931de' },
    @{ Name='SPIRV-Tools-sdk-1.4.328.1-LICENSE'; Url='https://raw.githubusercontent.com/KhronosGroup/SPIRV-Tools/vulkan-sdk-1.4.328.1/LICENSE'; Sha='cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30' },
    @{ Name='SPIRV-Headers-sdk-1.4.328.1-LICENSE'; Url='https://raw.githubusercontent.com/KhronosGroup/SPIRV-Headers/vulkan-sdk-1.4.328.1/LICENSE'; Sha='ea43b1de38a6f90c488800d66dec1ed671e68cda530266bc96951fb5b6307613' },
    @{ Name='LLVM-xxhash-NOTICE'; Url='https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-21.1.7/llvm/lib/Support/xxhash.cpp'; Sha='b47e89a65e40f34c7e336a58f1902c958b7bd90b3370bd497c8cb788eb40c2d4' },
    @{ Name='Mesa-Windows-build-LICENSE'; Url='https://raw.githubusercontent.com/pal1000/mesa-dist-win/25.2.8/LICENSE'; Sha='9cd0121dc070f0ac65c1fc266bee74bdbbc22cea4210ae72ae0c1f076448d4cc' }
)
foreach ($notice in $mesaNotices) {
    $file = Get-VerifiedArchive $notice.Url $notice.Sha "$($notice.Name).txt"
    Copy-Item -LiteralPath $file -Destination (Join-Path $mesaRuntime.FullName 'licenses')
}

$sidecar = New-Item -ItemType Directory -Path (Join-Path $stage 'cursor-sidecar')
foreach ($file in @('index.mjs', 'artifacts.mjs', 'artifact-generation-v1.md', 'board-link.mjs', 'board-link-v1.md', 'attachments.mjs', 'models.mjs', 'activity.mjs', 'policy.mjs', 'install.mjs', 'supervise.mjs', 'package.json', 'package-lock.json', 'README.md', 'SETUP.md')) {
    Copy-Item -LiteralPath (Join-Path $repo "docs/agent/cursor-sidecar/$file") -Destination $sidecar.FullName
}

# Carry license texts for compiled Rust dependencies as well as the lockfile.
$licenses = New-Item -ItemType Directory -Path (Join-Path $stage 'Rust-licenses')
$metadata.packages | Select-Object name, version, license, repository | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $licenses 'index.json') -Encoding utf8
foreach ($package in $metadata.packages) {
    $dir = Split-Path -Parent $package.manifest_path
    # Embedded fonts and vendored libraries keep notices below the crate root.
    $texts = Get-ChildItem -LiteralPath $dir -File -Recurse | Where-Object {
        $_.Name -match '(^|[-_.])(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT|AUTHORS|OFL|UFL)($|[-_.])' -and
        $_.Extension -notin @('.rs', '.toml', '.json', '.lock', '.exe', '.dll', '.png', '.jpg', '.ttf', '.otf', '.woff', '.woff2')
    }
    if ($texts) {
        $dest = New-Item -ItemType Directory -Force -Path (Join-Path $licenses "$($package.name)-$($package.version)")
        foreach ($text in $texts) {
            $relative = $text.FullName.Substring($dir.Length).TrimStart([char[]]'\/')
            $destination = Join-Path $dest.FullName $relative
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
            Copy-Item -LiteralPath $text.FullName -Destination $destination
        }
    }
}
Copy-Item -LiteralPath (Join-Path $repo 'Cargo.lock') -Destination $licenses.FullName
# meshopt embeds the native license in a C++ header and omits the binding's
# root license files from its published crate archive.
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'distribution/meshopt-notices.txt') -Destination $licenses.FullName

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
