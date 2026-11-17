# Velopack owns the main Slate shortcut. This hook adds File Atlas and
# the explicit graphics compatibility entry.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$InstallRoot, [int]$Remove = 0)
$ErrorActionPreference = 'Stop'
$current = Join-Path $InstallRoot 'current'
$distribution = Get-Content -LiteralPath (Join-Path $current 'atlas-release.json') -Raw | ConvertFrom-Json
$suffix = if ($distribution.channel -eq 'preview') { ' Preview' } else { '' }
$programs = [Environment]::GetFolderPath('Programs')
$entries = @(
    @{ Name = "File Atlas$suffix.lnk"; Exe = 'native-file-atlas.exe'; Arguments = ''; Description = 'File Atlas — visual file organizer' },
    @{ Name = "Slate$suffix (Software graphics).lnk"; Exe = 'slate.exe'; Arguments = '--software-graphics'; Description = 'Slate with the software OpenGL renderer' }
)
if ($Remove -ne 1) { $shell = New-Object -ComObject WScript.Shell }
foreach ($entry in $entries) {
    $link = Join-Path $programs $entry.Name
    if ($Remove -eq 1) {
        if (Test-Path -LiteralPath $link) { Remove-Item -LiteralPath $link }
        continue
    }
    $shortcut = $shell.CreateShortcut($link)
    $shortcut.TargetPath = Join-Path $current $entry.Exe
    $shortcut.WorkingDirectory = $InstallRoot
    $shortcut.Arguments = $entry.Arguments
    $shortcut.IconLocation = "$current\$($entry.Exe),0"
    $shortcut.Description = $entry.Description
    $shortcut.Save()
}
