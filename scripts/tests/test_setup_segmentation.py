"""Offline tests for model verification; no weights or torch imports required."""
import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "setup_segmentation", Path(__file__).resolve().parents[1] / "setup_segmentation.py")
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)


class CheckpointTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "model.pt"
        self.expected = b"reviewed checkpoint bytes"
        self.digest = patch.object(setup, "MODEL_SHA256", hashlib.sha256(self.expected).hexdigest())
        self.digest.start()
        self.addCleanup(self.digest.stop)

    def test_existing_valid_checkpoint_needs_no_network(self):
        self.path.write_bytes(self.expected)
        with patch.object(setup, "urlopen") as download:
            setup.ensure_model(self.path, verify_only=True)
            download.assert_not_called()

    def test_existing_corruption_stops_before_network(self):
        self.path.write_bytes(b"corrupt")
        with patch.object(setup, "urlopen") as download:
            with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                setup.ensure_model(self.path)
            download.assert_not_called()

    def test_verified_download_is_promoted_atomically(self):
        with patch.object(setup, "urlopen", return_value=io.BytesIO(self.expected)):
            setup.ensure_model(self.path)
        self.assertEqual(self.path.read_bytes(), self.expected)
        self.assertFalse(self.path.with_suffix(".part").exists())

    def test_bad_download_is_never_installed(self):
        with patch.object(setup, "urlopen", return_value=io.BytesIO(b"wrong")):
            with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                setup.ensure_model(self.path)
        self.assertFalse(self.path.exists())
        self.assertFalse(self.path.with_suffix(".part").exists())

    def test_verify_only_never_downloads_a_missing_model(self):
        with patch.object(setup, "urlopen") as download:
            with self.assertRaisesRegex(RuntimeError, "missing"):
                setup.ensure_model(self.path, verify_only=True)
            download.assert_not_called()


if __name__ == "__main__":
    unittest.main()
