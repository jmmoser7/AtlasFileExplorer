param([string]$RhinoSystem = 'C:\Program Files\Rhino 8\System', [string]$OutputDirectory, [switch]$SkipIfUnavailable)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$rhino = Join-Path $RhinoSystem 'RhinoCommon.dll'
if (!(Test-Path -LiteralPath $rhino)) {
    if ($SkipIfUnavailable) { Write-Host 'Optional headless Rhino helper unavailable: install Rhino 8 SDK to enable this capability.'; return }
    throw "Rhino 8 SDK unavailable: $rhino"
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
