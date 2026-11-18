"""Offline tests for model verification; no weights or torch imports required."""
import hashlib
import importlib.util
import io
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import venv

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


class PythonRepairTests(unittest.TestCase):
    def test_wrong_python_version_preserves_the_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cfg = root / "venv" / "pyvenv.cfg"
            cfg.parent.mkdir()
            original = f"home = {root / 'uninstalled'}\nversion = 0.1.2\n".encode()
            cfg.write_bytes(original)
            with self.assertRaisesRegex(RuntimeError, "64-bit Python 0.1"):
                setup.repair_python(root)
            self.assertEqual(cfg.read_bytes(), original)

    @unittest.skipUnless(sys.platform == "win32", "Windows venv launcher")
    def test_missing_base_reuses_existing_packages_and_checkpoint(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            environment = root / "venv"
            venv.EnvBuilder(with_pip=False).create(environment)
            cfg = environment / "pyvenv.cfg"
            content = cfg.read_text()
            cfg.write_text("\n".join(
                f"home = {root / 'uninstalled'}" if line.startswith("home = ") else line
                for line in content.splitlines()) + "\n")
            launcher = environment / "Scripts/python.exe"
            self.assertNotEqual(subprocess.run([str(launcher), "--version"], capture_output=True).returncode, 0)
            package = environment / "Lib/site-packages/retained_probe.py"
            package.write_text("value = 73\n")
            checkpoint = root / "model/checkpoint.pt"
            checkpoint.parent.mkdir()
            checkpoint.write_bytes(b"retained model")
            setup.repair_python(root)
            subprocess.run([str(launcher), "-I", "-c", "import retained_probe; assert retained_probe.value == 73"],
                           check=True)
            self.assertEqual(checkpoint.read_bytes(), b"retained model")

if __name__ == "__main__":
    unittest.main()
