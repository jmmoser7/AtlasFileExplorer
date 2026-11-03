param([string]$RhinoSystem = 'C:\Program Files\Rhino 8\System', [string]$OutputDirectory, [switch]$SkipIfUnavailable)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$rhino = Join-Path $RhinoSystem 'RhinoCommon.dll'
if (!(Test-Path -LiteralPath $rhino)) {
    # Compile reference only: the user's licensed Rhino installation supplies
    # all managed/native runtime binaries through HeadlessRhino's resolver.
    $version = '8.18.25100.11001'
    $hash = '65b997c1dc357a5e8d1a1ef42e420b817e947899deb8551d7cf24f5b29c5cc62'
    $cache = Join-Path $repo "target/headless-rhino-reference/$version"
    $archive = Join-Path $cache "rhinocommon.$version.nupkg"
    New-Item -ItemType Directory -Force -Path $cache | Out-Null
    try {
        if (!(Test-Path -LiteralPath $archive -PathType Leaf)) {
            Invoke-WebRequest -Uri "https://api.nuget.org/v3-flatcontainer/rhinocommon/$version/rhinocommon.$version.nupkg" -OutFile $archive
        }
    } catch {
        if ($SkipIfUnavailable) { Write-Host "Optional headless Rhino compile reference unavailable: $_"; return }
        throw
    }
    # Integrity failures always stop, including optional development builds.
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $hash) {
        throw "RhinoCommon compile-reference checksum mismatch: $archive. Remove the cached archive and retry."
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $entry = $zip.GetEntry('lib/net48/RhinoCommon.dll')
        if ($null -eq $entry) { throw 'Pinned RhinoCommon package is missing its net48 compile reference.' }
        $rhino = Join-Path $cache 'RhinoCommon.dll'
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $rhino, $true)
    } finally { $zip.Dispose() }
}
$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (!(Test-Path -LiteralPath $compiler)) {
    if ($SkipIfUnavailable) { Write-Host 'Optional headless Rhino helper unavailable: .NET Framework compiler missing.'; return }
    throw '.NET Framework x64 C# compiler unavailable'
}
$output = if ($OutputDirectory) { [IO.Path]::GetFullPath($OutputDirectory) } else { Join-Path $repo 'target\headless-rhino' }
New-Item -ItemType Directory -Force -Path $output | Out-Null
& $compiler /nologo /target:exe /platform:x64 /optimize+ "/reference:$rhino" /reference:System.Web.Extensions.dll "/out:$output\HeadlessRhino.exe" (Join-Path $PSScriptRoot 'HeadlessRhino.cs')
if ($LASTEXITCODE -ne 0) { throw "Headless Rhino build failed ($LASTEXITCODE)" }
Write-Output (Join-Path $output 'HeadlessRhino.exe')
