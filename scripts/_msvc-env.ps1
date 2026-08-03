# Shared MSVC/SDK env for this machine when VS2022's libs are incomplete.
# Dot-source from other scripts. No-op when paths are missing or LIB is set.
$msvc2019 = "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Tools\MSVC\14.29.30133"
$winsdk = "C:\Program Files (x86)\Windows Kits\10"
if ((Test-Path "$msvc2019\lib\x64\msvcrt.lib") -and -not $env:LIB) {
    $env:INCLUDE = "$msvc2019\include;$winsdk\include\10.0.19041.0\ucrt;$winsdk\include\10.0.19041.0\shared;$winsdk\include\10.0.19041.0\um"
    $env:LIB = "$msvc2019\lib\x64;$winsdk\lib\10.0.19041.0\ucrt\x64;$winsdk\lib\10.0.19041.0\um\x64"
}
