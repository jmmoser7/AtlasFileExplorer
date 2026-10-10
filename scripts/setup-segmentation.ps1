# Installs Slate's optional, pinned CPU model for this user. No images are uploaded.
param([switch]$VerifyOnly)
$ErrorActionPreference = 'Stop'
$segmentRoot = Join-Path $env:LOCALAPPDATA 'NativeFileAtlas\segmentation'
$segmentPython = Join-Path $segmentRoot 'venv\Scripts\python.exe'
if (-not (Test-Path -LiteralPath $segmentPython)) {
    if ($VerifyOnly) { throw 'Segmentation is not installed.' }
    python -m venv (Join-Path $segmentRoot 'venv')
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the segmentation environment.' }
}
& $segmentPython -c 'import sys,struct; assert (3,11) <= sys.version_info[:2] <= (3,13) and struct.calcsize("P")==8, "Use 64-bit Python 3.11-3.13 (3.13 is the validated runtime)."'
if ($LASTEXITCODE -ne 0) { throw 'Unsupported Python runtime.' }
if (-not $VerifyOnly) {
    & $segmentPython -m pip install --disable-pip-version-check --require-hashes -r (Join-Path $PSScriptRoot 'segmentation-bootstrap.lock')
    if ($LASTEXITCODE -ne 0) { throw 'Could not install the pinned build tools.' }
    $segmentCudaBefore = $env:SAM2_BUILD_CUDA
    try {
        $env:SAM2_BUILD_CUDA = '0'
        & $segmentPython -m pip install --disable-pip-version-check --require-hashes --no-build-isolation -r (Join-Path $PSScriptRoot 'segmentation-requirements.lock')
        if ($LASTEXITCODE -ne 0) { throw 'Could not install the pinned inference runtime.' }
    } finally { $env:SAM2_BUILD_CUDA = $segmentCudaBefore }
    & $segmentPython -m pip check
    if ($LASTEXITCODE -ne 0) { throw 'Segmentation dependencies are inconsistent.' }
}
$segmentArgs = @((Join-Path $PSScriptRoot 'setup_segmentation.py'), $segmentRoot)
if ($VerifyOnly) { $segmentArgs += '--verify-only' }
& $segmentPython @segmentArgs
if ($LASTEXITCODE -ne 0) { throw 'Segmentation verification failed.' }
