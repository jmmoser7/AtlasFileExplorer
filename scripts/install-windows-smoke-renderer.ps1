$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or
    $env:RUNNER_OS -ne 'Windows' -or $env:RUNNER_ARCH -ne 'X64' -or !$env:RUNNER_TEMP) {
    throw 'The software renderer is for disposable x64 GitHub-hosted Windows runners.'
}

# Mesa's llvmpipe documentation describes MSOGL as the fallback ICD for Windows
# without hardware OpenGL. Register it on the runner without replacing opengl32.dll.
$registry = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\OpenGLDrivers'
$key = Join-Path $registry 'MSOGL'
$driverName = 'atlas-smoke-mesa.dll'
$driver = Join-Path ([Environment]::SystemDirectory) $driverName
if ((Test-Path -LiteralPath $key) -or (Test-Path -LiteralPath $driver)) {
    throw 'The runner already has a registered fallback OpenGL driver.'
}

$name = 'mesa3d-25.2.8-release-msvc.7z'
$url = "https://github.com/pal1000/mesa-dist-win/releases/download/25.2.8/$name"
$expected = 'e47cfe0be35f0c40c3d488c0e965d10656154ed55cfdfe4f4d79d4395672dc02'
$stage = Join-Path $env:RUNNER_TEMP "slate-smoke-renderer-$([guid]::NewGuid().ToString('N'))"
New-Item -Path $stage -ItemType Directory | Out-Null
$archive = Join-Path $stage $name
Invoke-WebRequest -Uri $url -OutFile $archive -TimeoutSec 90
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected) {
    throw 'The Mesa software renderer download did not match the pinned SHA-256.'
}
& tar.exe -xf $archive -C $stage x64/libgallium_wgl.dll
if ($LASTEXITCODE -ne 0) {
    throw "Could not extract the Mesa software renderer (tar exit $LASTEXITCODE)."
}

Copy-Item -LiteralPath (Join-Path $stage 'x64/libgallium_wgl.dll') -Destination $driver
New-Item -Path $key -Force | Out-Null
New-ItemProperty -LiteralPath $key -Name DLL -PropertyType String -Value $driverName | Out-Null
New-ItemProperty -LiteralPath $key -Name DriverVersion -PropertyType DWord -Value 1 | Out-Null
New-ItemProperty -LiteralPath $key -Name Flags -PropertyType DWord -Value 1 | Out-Null
New-ItemProperty -LiteralPath $key -Name Version -PropertyType DWord -Value 2 | Out-Null
