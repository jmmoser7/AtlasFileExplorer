# Installs Slate's optional local object-highlighting model for this user.
# Inference is offline after this one-time setup. No source images are uploaded.
$ErrorActionPreference = 'Stop'
$segmentRoot = Join-Path $env:LOCALAPPDATA 'NativeFileAtlas\segmentation'
$segmentPython = Join-Path $segmentRoot 'venv\Scripts\python.exe'
if (-not (Test-Path -LiteralPath $segmentPython)) {
    python -m venv (Join-Path $segmentRoot 'venv')
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the segmentation environment.' }
}
& $segmentPython -m pip install --disable-pip-version-check torch torchvision --index-url https://download.pytorch.org/whl/cpu
if ($LASTEXITCODE -ne 0) { throw 'Could not install the local inference runtime.' }
& $segmentPython -m pip install --disable-pip-version-check wheel pillow opencv-python-headless
if ($LASTEXITCODE -ne 0) { throw 'Could not install the segmentation adapter.' }
$env:SAM2_BUILD_CUDA = '0'
& $segmentPython -m pip install --disable-pip-version-check --no-build-isolation 'https://github.com/facebookresearch/sam2/archive/2b90b9f5ceec907a1c18123530e92e794ad901a4.zip'
if ($LASTEXITCODE -ne 0) { throw 'Could not install SAM 2.' }
$env:ATLAS_SEGMENT_MODEL = Join-Path $segmentRoot 'model'
@'
import os
from pathlib import Path
from urllib.request import urlretrieve
target = Path(os.environ['ATLAS_SEGMENT_MODEL'])
target.mkdir(parents=True, exist_ok=True)
checkpoint = target / 'sam2.1_hiera_tiny.pt'
if not checkpoint.is_file():
    temporary = checkpoint.with_suffix('.part')
    urlretrieve('https://dl.fbaipublicfiles.com/segment_anything_2/092824/sam2.1_hiera_tiny.pt', temporary)
    temporary.replace(checkpoint)
print('Object highlighting is ready. Images stay on this machine.')
'@ | & $segmentPython -
if ($LASTEXITCODE -ne 0) { throw 'Could not download the object-highlighting model.' }
