"""Local Studio preview and the fixed standalone Rust Studio launcher."""
from datetime import datetime, timezone
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import json
import subprocess
import sys
import threading
from urllib.parse import urlsplit

STUDIO = Path(__file__).resolve().parent
WORKSPACE = STUDIO.parent.parent
NATIVE_STUDIO = WORKSPACE / "target/debug/examples/frame_canvas.exe"
VIDEO_STUDIO = WORKSPACE / "target/debug/examples/video_canvas.exe"
VIDEO_WORKER = WORKSPACE / "target/debug/slate-video-worker.exe"
VIDEO_STATUS_PATH = "/api/native-studio/video"
VIDEO_LAUNCH_PATH = VIDEO_STATUS_PATH + "/launch"
LAUNCH_PATH = "/api/native-studio/launch"
STATUS_PATH = "/api/native-studio"
launch_lock = threading.Lock()
native_process = None
video_process = None


def native_status():
    try:
        stat = NATIVE_STUDIO.stat()
    except FileNotFoundError:
        return {"available": False, "message": "Rust Studio has not been built in this workspace."}
    return {
        "available": True,
        "artifact": "target/debug/examples/frame_canvas.exe",
        "modifiedUtc": datetime.fromtimestamp(stat.st_mtime, timezone.utc).isoformat(),
    }


def video_status():
    if not VIDEO_STUDIO.is_file() or not VIDEO_WORKER.is_file():
        return {"available": False, "message": "Video Studio and its production decoder worker must both be built in this workspace."}
    return {
        "available": True,
        "artifact": "target/debug/examples/video_canvas.exe",
        "modifiedUtc": datetime.fromtimestamp(VIDEO_STUDIO.stat().st_mtime, timezone.utc).isoformat(),
    }


class Handler(SimpleHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(STUDIO), **kwargs)

    def log_message(self, *_args):
        pass

    def end_headers(self):
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()

    def json_response(self, status, body):
        data = json.dumps(body).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == STATUS_PATH:
            self.json_response(200, native_status())
        elif self.path == VIDEO_STATUS_PATH:
            self.json_response(200, video_status())
        else:
            super().do_GET()

    def do_POST(self):
        global native_process, video_process
        self.close_connection = True
        video = self.path == VIDEO_LAUNCH_PATH
        if self.path not in (LAUNCH_PATH, VIDEO_LAUNCH_PATH):
            self.json_response(404, {"message": "Unknown Studio action."})
            return
        # A browser must explicitly originate the launch from this loopback server.
        port = self.server.server_port
        try:
            origin = urlsplit(self.headers.get("Origin", ""))
            host = urlsplit("http://" + self.headers.get("Host", ""))
            local_origin = (
                origin.scheme == "http"
                and origin.hostname in ("localhost", "127.0.0.1")
                and origin.port == port
                and host.hostname == origin.hostname
                and host.port == port
                and not host.username
                and not host.password
                and not host.path
                and not origin.username
                and not origin.password
                and not origin.path
                and not origin.query
                and not origin.fragment
            )
        except ValueError:
            local_origin = False
        if not local_origin:
            self.json_response(403, {"message": "Launch Rust Studio from the local Studio dashboard."})
            return
        if self.headers.get("Content-Length") != "2":
            self.json_response(400, {"message": "This action accepts no launch arguments."})
            return
        if self.rfile.read(2) != b"{}":
            self.json_response(400, {"message": "This action accepts no launch arguments."})
            return
        with launch_lock:
            status = video_status() if video else native_status()
            if not status["available"]:
                self.json_response(404, status)
                return
            process = video_process if video else native_process
            if process is not None and process.poll() is None:
                self.json_response(200, {**status, "message": "Rust Studio is already open.", "pid": process.pid})
                return
            try:
                process = subprocess.Popen(
                    [str(VIDEO_STUDIO)] if video else [str(NATIVE_STUDIO), "--large"],
                    cwd=str(WORKSPACE),
                    stdin=subprocess.DEVNULL,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
            except OSError:
                self.json_response(500, {"message": "Rust Studio could not be opened. Check the local executable."})
                return
            if video:
                video_process = process
            else:
                native_process = process
            if process.poll() is not None:
                self.json_response(500, {"message": "Rust Studio exited during launch."})
                return
            self.json_response(200, {**status, "message": "Rust Studio launch requested. Look for its native window.", "pid": process.pid})


class Server(ThreadingHTTPServer):
    request_queue_size = 128
    daemon_threads = True


if __name__ == "__main__":
    Server(("127.0.0.1", int(sys.argv[1]) if len(sys.argv) > 1 else 8765), Handler).serve_forever()
