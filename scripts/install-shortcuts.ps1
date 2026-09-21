# Point Desktop, Start menu, and existing pins at the locally built applications.
# Pins are not added, removed, or reordered. A shortcut that already launches
# slate.exe or native-file-atlas.exe is retargeted at this build, including a
# taskbar or Start pin that was still opening an older copy.
[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$binDir = Join-Path $root ("target\" + $Configuration.ToLowerInvariant())
$desktop = [Environment]::GetFolderPath("DesktopDirectory")
$programs = [Environment]::GetFolderPath("Programs")
$shell = New-Object -ComObject WScript.Shell

function Update-Shortcut {
    param(
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)] [string]$Executable,
        [Parameter(Mandatory)] [string]$Description,
        [Parameter(Mandatory)] [string]$IconLocation
    )

    $shortcut = $shell.CreateShortcut($Path)
    $current = $shortcut.TargetPath -eq $Executable -and
        $shortcut.WorkingDirectory -eq $root -and
        $shortcut.Description -eq $Description -and
        $shortcut.IconLocation -eq $IconLocation
    if ($current) {
        return
    }

    $shortcut.TargetPath = $Executable
    $shortcut.WorkingDirectory = $root
    $shortcut.Description = $Description
    $shortcut.IconLocation = $IconLocation
    $shortcut.Save()
    Write-Host "Updated $Path -> $Executable"
}

function Get-LauncherRoots {
    $candidates = @(
        $desktop,
        $programs,
        [Environment]::GetFolderPath("CommonDesktopDirectory"),
        [Environment]::GetFolderPath("CommonPrograms"),
        (Join-Path $env:APPDATA "Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"),
        (Join-Path $env:APPDATA "Microsoft\Internet Explorer\Quick Launch\User Pinned\StartMenu")
    )
    $candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -Unique
}

function Update-AppLaunchers {
    param(
        [Parameter(Mandatory)] [string]$Name,
        [Parameter(Mandatory)] [string]$Executable,
        [Parameter(Mandatory)] [string]$Description
    )

    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "Missing $Executable. Build the $Configuration profile before installing shortcuts."
    }

    $icon = "$Executable,0"
    foreach ($folder in @($desktop, $programs)) {
        Update-Shortcut -Path (Join-Path $folder "$Name.lnk") -Executable $Executable -Description $Description -IconLocation $icon
    }

    $fileName = [System.IO.Path]::GetFileName($Executable)
    foreach ($folder in Get-LauncherRoots) {
        Get-ChildItem -LiteralPath $folder -Filter *.lnk -Recurse -ErrorAction SilentlyContinue | ForEach-Object {
            $existing = $shell.CreateShortcut($_.FullName)
            $targetName = [System.IO.Path]::GetFileName($existing.TargetPath)
            if ($targetName -and ($targetName -ieq $fileName)) {
                Update-Shortcut -Path $_.FullName -Executable $Executable -Description $Description -IconLocation $icon
            }
        }
    }
}

Update-AppLaunchers -Name "Slate" -Executable (Join-Path $binDir "slate.exe") -Description "Slate board workspace"
Update-AppLaunchers -Name "File Atlas" -Executable (Join-Path $binDir "native-file-atlas.exe") -Description "File Atlas explorer"
