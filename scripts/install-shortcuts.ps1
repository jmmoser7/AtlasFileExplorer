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

function Get-LauncherScan {
    # Do not recurse Start Menu Programs: vendor installers nest paths past MAX_PATH.
    # Named Desktop/Start shortcuts are already written above. Scan pins and the
    # desktop/start roots for extra copies that still launch these exes.
    $scans = @(
        @{ Path = $desktop; Recurse = $false },
        @{ Path = $programs; Recurse = $false },
        @{ Path = [Environment]::GetFolderPath("CommonDesktopDirectory"); Recurse = $false },
        @{ Path = [Environment]::GetFolderPath("CommonPrograms"); Recurse = $false },
        @{ Path = (Join-Path $env:APPDATA "Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"); Recurse = $true },
        @{ Path = (Join-Path $env:APPDATA "Microsoft\Internet Explorer\Quick Launch\User Pinned\StartMenu"); Recurse = $true }
    )
    $scans | Where-Object { $_.Path -and (Test-Path -LiteralPath $_.Path) }
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
    foreach ($scan in Get-LauncherScan) {
        $items = Get-ChildItem -LiteralPath $scan.Path -Filter *.lnk -File -Recurse:$scan.Recurse -ErrorAction SilentlyContinue
        foreach ($item in $items) {
            try {
                $existing = $shell.CreateShortcut($item.FullName)
            } catch {
                continue
            }
            $targetName = [System.IO.Path]::GetFileName($existing.TargetPath)
            if ($targetName -and ($targetName -ieq $fileName)) {
                Update-Shortcut -Path $item.FullName -Executable $Executable -Description $Description -IconLocation $icon
            }
        }
    }
}

Update-AppLaunchers -Name "Slate" -Executable (Join-Path $binDir "slate.exe") -Description "Slate board workspace"
Update-AppLaunchers -Name "File Atlas" -Executable (Join-Path $binDir "native-file-atlas.exe") -Description "File Atlas explorer"
