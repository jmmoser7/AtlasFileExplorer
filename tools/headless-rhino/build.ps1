param([string]$RhinoSystem = 'C:\Program Files\Rhino 8\System', [string]$OutputDirectory, [switch]$SkipIfUnavailable)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$version = '8.18.25100.11001'

function Get-CompileReferences([string]$Package, [string]$Hash, [string]$InstalledDirectory, [string[]]$Names) {
    $installed = $Names | ForEach-Object { Join-Path $InstalledDirectory $_ }
    if (@($installed | Where-Object { !(Test-Path -LiteralPath $_ -PathType Leaf) }).Count -eq 0) {
        return $installed
    }

    # The licensed local installation supplies all runtime binaries. These
    # packages supply compile-only metadata on machines without Rhino.
    $cache = Join-Path $repo "target/headless-rhino-reference/$version"
    $archive = Join-Path $cache "$Package.$version.nupkg"
    New-Item -ItemType Directory -Force -Path $cache | Out-Null
    try {
        if (!(Test-Path -LiteralPath $archive -PathType Leaf)) {
            Invoke-WebRequest -Uri "https://api.nuget.org/v3-flatcontainer/$Package/$version/$Package.$version.nupkg" -OutFile $archive
        }
    } catch {
        if ($SkipIfUnavailable) { Write-Host "Optional headless Rhino compile reference unavailable: $_"; return }
        throw
    }
    # Integrity failures always stop, including optional development builds.
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $Hash) {
        throw "$Package compile-reference checksum mismatch: $archive. Remove the cached archive and retry."
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        foreach ($name in $Names) {
            $entry = $zip.GetEntry("lib/net48/$name")
            if ($null -eq $entry) { throw "Pinned $Package package is missing its net48 compile reference for $name." }
            $reference = Join-Path $cache $name
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $reference, $true)
            Write-Output $reference
        }
    } finally { $zip.Dispose() }
}

$rhino = Get-CompileReferences 'rhinocommon' '65b997c1dc357a5e8d1a1ef42e420b817e947899deb8551d7cf24f5b29c5cc62' $RhinoSystem @('RhinoCommon.dll')
if (!$rhino) { return }
$grasshopperDirectory = Join-Path (Split-Path $RhinoSystem -Parent) 'Plug-ins\Grasshopper'
$grasshopper = Get-CompileReferences 'grasshopper' '982AB9FD5643BABAE6B04148B3D0306448CBB67F47BE0A2B7438DA96D8825C6B' $grasshopperDirectory @('Grasshopper.dll', 'GH_IO.dll')

$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (!(Test-Path -LiteralPath $compiler)) {
    if ($SkipIfUnavailable) { Write-Host 'Optional headless Rhino helper unavailable: .NET Framework compiler missing.'; return }
    throw '.NET Framework x64 C# compiler unavailable'
}
$output = if ($OutputDirectory) { [IO.Path]::GetFullPath($OutputDirectory) } else { Join-Path $repo 'target\headless-rhino' }
New-Item -ItemType Directory -Force -Path $output | Out-Null
$references = @($rhino | ForEach-Object { "/reference:$_" })
$sources = @(Join-Path $PSScriptRoot 'HeadlessRhino.cs')
$options = @()
if ($grasshopper) {
    $references += @($grasshopper | ForEach-Object { "/reference:$_" })
    $sources += @('GrasshopperHost.cs', 'GrasshopperPorts.cs', 'GrasshopperStartup.cs', 'GrasshopperPackageCandidates.cs' | ForEach-Object { Join-Path $PSScriptRoot $_ })
    $options += '/define:SLATE_GRASSHOPPER', '/reference:System.Windows.Forms.dll', '/reference:System.IO.Compression.dll'
}
& $compiler /nologo /target:exe /platform:x64 /optimize+ $references $options /reference:System.Web.Extensions.dll /reference:System.Drawing.dll "/out:$output\HeadlessRhino.exe" $sources
if ($LASTEXITCODE -ne 0) { throw "Headless Rhino build failed ($LASTEXITCODE)" }
Write-Output (Join-Path $output 'HeadlessRhino.exe')
