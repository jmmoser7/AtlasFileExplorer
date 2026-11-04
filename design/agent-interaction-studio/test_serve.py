"""The launcher exposes one fixed action and rejects browser cross-origin launches."""
import importlib.util
import json
from pathlib import Path
import threading
import unittest
from unittest.mock import Mock, patch
from urllib.error import HTTPError
from urllib.request import Request, urlopen

spec = importlib.util.spec_from_file_location("studio_server", Path(__file__).with_name("serve.py"))
server_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(server_module)


class LaunchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = server_module.Server(("127.0.0.1", 0), server_module.Handler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.base = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def request(self, path, origin=None, body=None, host=None):
        headers = {}
        if origin is not None:
            headers["Origin"] = origin
        if host is not None:
            headers["Host"] = host
        request = Request(self.base + path, data=body, headers=headers)
        try:
            with urlopen(request) as response:
                return response.status, json.load(response)
        except HTTPError as error:
            return error.code, json.load(error)

    def test_fixed_action_and_launch_deduplication(self):
        process = Mock(pid=1234)
        process.poll.return_value = None
        status = {"available": True, "modifiedUtc": "known"}
        server_module.native_process = None
        with patch.object(server_module, "native_status", return_value=status), patch.object(server_module.subprocess, "Popen", return_value=process) as launch:
            self.assertEqual(self.request("/api/native-studio")[0], 200)
            launch.assert_not_called()
            self.assertEqual(self.request("/api/native-studio/launch", self.base, b"{}")[0], 200)
            self.assertEqual(self.request("/api/native-studio/launch", self.base, b"{}")[1]["message"], "Rust Studio is already open.")
            launch.assert_called_once()
            self.assertEqual(launch.call_args.args[0], [str(server_module.NATIVE_STUDIO), "--large"])
            self.assertEqual(launch.call_args.kwargs["cwd"], str(server_module.WORKSPACE))

    def test_origin_host_payload_and_path_are_restricted(self):
        with patch.object(server_module.subprocess, "Popen") as launch:
            for origin in (None, "https://example.com", self.base + "/wrong"):
                self.assertEqual(self.request("/api/native-studio/launch", origin, b"{}")[0], 403)
            self.assertEqual(self.request("/api/native-studio/launch", self.base, b"{}", "example.com")[0], 403)
            self.assertEqual(self.request("/api/native-studio/launch?exe=other", self.base, b"{}")[0], 404)
            self.assertEqual(self.request("/api/native-studio/launch", self.base, b'{"exe":"other"}')[0], 400)
            launch.assert_not_called()

    def test_missing_binary_and_launch_errors_are_visible(self):
        server_module.native_process = None
        with patch.object(server_module, "native_status", return_value={"available": False, "message": "Missing"}):
            self.assertEqual(self.request("/api/native-studio/launch", self.base, b"{}"), (404, {"available": False, "message": "Missing"}))
        with patch.object(server_module, "native_status", return_value={"available": True}), patch.object(server_module.subprocess, "Popen", side_effect=OSError):
            self.assertEqual(self.request("/api/native-studio/launch", self.base, b"{}")[0], 500)


if __name__ == "__main__":
    unittest.main()
