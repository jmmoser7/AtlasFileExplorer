# Create or refresh Desktop and Start menu shortcuts for the locally built applications.
# Taskbar pinning stays a user-controlled Windows action; pin either shortcut once.
[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$binDir = Join-Path $root ("target\\" + $Configuration.ToLowerInvariant())
$desktop = [Environment]::GetFolderPath("DesktopDirectory")
$programs = [Environment]::GetFolderPath("Programs")
$shell = New-Object -ComObject WScript.Shell

function Set-AppShortcut {
    param(
        [Parameter(Mandatory)] [string]$Name,
        [Parameter(Mandatory)] [string]$Executable,
        [Parameter(Mandatory)] [string]$Description,
        [Parameter(Mandatory)] [string]$Icon
    )

    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "Missing $Executable. Build the $Configuration profile before installing shortcuts."
    }

    foreach ($folder in @($desktop, $programs)) {
        $path = Join-Path $folder "$Name.lnk"
        $shortcut = $shell.CreateShortcut($path)
        $needsUpdate = $shortcut.TargetPath -ne $Executable -or
            $shortcut.WorkingDirectory -ne $root -or
            $shortcut.Description -ne $Description -or
            $shortcut.IconLocation -ne "$Icon,0"

        if ($needsUpdate) {
            $shortcut.TargetPath = $Executable
            $shortcut.WorkingDirectory = $root
            $shortcut.Description = $Description
            $shortcut.IconLocation = "$Icon,0"
            $shortcut.Save()
            Write-Host "Updated $path"
        } else {
            Write-Host "$Name shortcut is already current."
        }
    }
}

Set-AppShortcut -Name "Slate" -Executable (Join-Path $binDir "slate.exe") -Description "Slate board workspace" -Icon (Join-Path $root "apps/slate/assets/slate.ico")
Set-AppShortcut -Name "File Atlas" -Executable (Join-Path $binDir "native-file-atlas.exe") -Description "File Atlas explorer" -Icon (Join-Path $root "apps/file-atlas/assets/file-atlas.ico")
