"""Verify the pinned environment and checkpoint before declaring setup ready."""
import argparse
import hashlib
import importlib.metadata as metadata
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tempfile
from urllib.request import urlopen

MODEL_URL = "https://dl.fbaipublicfiles.com/segment_anything_2/092824/sam2.1_hiera_tiny.pt"
# Meta's Git LFS pointer: facebook/sam2.1-hiera-tiny, sam2.1_hiera_tiny.pt.
MODEL_SHA256 = "7402e0d864fa82708a20fbd15bc84245c2f26dff0eb43a4b5b93452deb34be69"


def repair_python(root):
    """Reconnect a Windows venv to a compatible replacement for its missing base Python."""
    venv = root / "venv"
    cfg = venv / "pyvenv.cfg"
    original = cfg.read_bytes()
    text = original.decode("utf-8")
    fields = dict(line.split(" = ", 1) for line in text.splitlines() if " = " in line)
    previous_home = Path(fields["home"])
    if (previous_home / "python.exe").exists():
        raise RuntimeError("The base Python is present; the launcher failed for another reason.")
    version = tuple(int(part) for part in fields["version"].split(".")[:2])
    if version != sys.version_info[:2] or struct.calcsize("P") != 8:
        raise RuntimeError(f"Select a 64-bit Python {version[0]}.{version[1]} using -Python to keep installed packages.")
    base = Path(sys._base_executable)
    if not base.is_file() or base.name.lower() != "python.exe":
        raise RuntimeError("Select an installed Windows Python interpreter using -Python.")
    replacements = {"home": str(base.parent), "executable": str(base), "version": sys.version.split()[0]}
    lines = []
    for line in text.splitlines(keepends=True):
        name, sep, _ = line.partition(" = ")
        if sep and name in replacements:
            ending = line[len(line.rstrip("\r\n")):]
            line = f"{name} = {replacements[name]}{ending}"
        lines.append(line)
    candidate = "".join(lines).encode("utf-8")
    # Check the existing launcher with the corrected config before touching the working directory.
    with tempfile.TemporaryDirectory(dir=venv.parent) as probe:
        trial = Path(probe)
        (trial / "Scripts").mkdir()
        launcher = trial / "Scripts/python.exe"
        launcher.write_bytes((venv / "Scripts/python.exe").read_bytes())
        (trial / "pyvenv.cfg").write_bytes(candidate)
        subprocess.run([str(launcher), "-I", "-S", "-c", "import sys"], check=True)
        if cfg.read_bytes() != original:
            raise RuntimeError("The Python configuration changed during repair. Rerun setup.")
        os.replace(trial / "pyvenv.cfg", cfg)


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_model(checkpoint, verify_only=False):
    if checkpoint.is_file():
        if sha256(checkpoint) != MODEL_SHA256:
            raise RuntimeError(f"Model checksum mismatch: {checkpoint}. Remove it and rerun setup.")
        return
    if verify_only:
        raise RuntimeError("Segmentation checkpoint is missing; run setup first.")
    checkpoint.parent.mkdir(parents=True, exist_ok=True)
    temporary = checkpoint.with_suffix(".part")
    try:
        with urlopen(MODEL_URL, timeout=60) as source, temporary.open("wb") as target:
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                target.write(chunk)
        if sha256(temporary) != MODEL_SHA256:
            raise RuntimeError("Downloaded model checksum mismatch; installation stopped.")
        temporary.replace(checkpoint)
    finally:
        temporary.unlink(missing_ok=True)


def verify_packages():
    lock = Path(__file__).with_name("segmentation-requirements.lock").read_text()
    for name, expected in re.findall(r"^([\w-]+)==([^\s]+)", lock, re.MULTILINE):
        actual = metadata.version(name)
        if actual != expected:
            raise RuntimeError(f"{name}: expected {expected}, found {actual}; rerun setup.")
    source = re.search(r"SAM-2 @ (\S+) \\\n\s+--hash=sha256:([a-f0-9]+)", lock)
    if source is None:
        raise RuntimeError("SAM source pin is missing from the lockfile.")
    installed = json.loads(metadata.distribution("SAM-2").read_text("direct_url.json") or "{}")
    hashes = installed.get("archive_info", {}).get("hashes", {})
    if installed.get("url") != source[1] or hashes.get("sha256") != source[2]:
        raise RuntimeError("Installed SAM source differs from the reviewed archive; rerun setup.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--verify-only", action="store_true")
    parser.add_argument("--repair-python", action="store_true")
    args = parser.parse_args()
    if args.repair_python:
        repair_python(args.root)
        return
    verify_packages()
    ensure_model(args.root / "model" / "sam2.1_hiera_tiny.pt", args.verify_only)
    print("Verified local object highlighting. Images stay on this machine.")


if __name__ == "__main__":
    main()
