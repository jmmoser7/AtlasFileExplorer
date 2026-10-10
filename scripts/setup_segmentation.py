"""Verify the pinned environment and checkpoint before declaring setup ready."""
import argparse
import hashlib
import importlib.metadata as metadata
import json
from pathlib import Path
import re
from urllib.request import urlopen

MODEL_URL = "https://dl.fbaipublicfiles.com/segment_anything_2/092824/sam2.1_hiera_tiny.pt"
# Meta's Git LFS pointer: facebook/sam2.1-hiera-tiny, sam2.1_hiera_tiny.pt.
MODEL_SHA256 = "7402e0d864fa82708a20fbd15bc84245c2f26dff0eb43a4b5b93452deb34be69"


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
    args = parser.parse_args()
    verify_packages()
    ensure_model(args.root / "model" / "sam2.1_hiera_tiny.pt", args.verify_only)
    print("Verified local object highlighting. Images stay on this machine.")


if __name__ == "__main__":
    main()
