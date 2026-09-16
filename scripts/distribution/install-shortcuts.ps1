# Velopack owns the Slate shortcut. This hook adds the second app in the suite.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$InstallRoot, [int]$Remove = 0)
$ErrorActionPreference = 'Stop'
$current = Join-Path $InstallRoot 'current'
$distribution = Get-Content -LiteralPath (Join-Path $current 'atlas-release.json') -Raw | ConvertFrom-Json
$name = if ($distribution.channel -eq 'preview') { 'File Atlas Preview.lnk' } else { 'File Atlas.lnk' }
$link = Join-Path ([Environment]::GetFolderPath('Programs')) $name
if ($Remove -eq 1) {
    if (Test-Path -LiteralPath $link) { Remove-Item -LiteralPath $link }
} else {
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($link)
    $shortcut.TargetPath = Join-Path $current 'native-file-atlas.exe'
    $shortcut.WorkingDirectory = $InstallRoot
    $shortcut.IconLocation = "$current\native-file-atlas.exe,0"
    $shortcut.Description = 'File Atlas — visual file organizer'
    $shortcut.Save()
}
