"""Generation, edit and model-check requests must not follow redirects (the key would travel). Loopback only."""
import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import io
import json
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


gen_image = load_module("gen_image", ROOT / "scripts" / "gen_image.py")
sys.path.insert(0, str(ROOT / "tools"))
import image_core  # noqa: E402


def png_bytes() -> bytes:
    buffer = io.BytesIO()
    Image.new("RGB", (16, 16), "#2b6cb0").save(buffer, format="PNG")
    return buffer.getvalue()


class RedirectSecurityTests(unittest.TestCase):
    def setUp(self):
        self.destination_auth: list[str] = []
        image = base64.b64encode(png_bytes()).decode("ascii")
        received = self.destination_auth

        class Destination(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def _reply(self):
                length = int(self.headers.get("Content-Length") or 0)
                if length:
                    self.rfile.read(length)
                received.append(self.headers.get("Authorization") or "")
                body = json.dumps({"data": [{"id": "gpt-image-2.5", "b64_json": image}]}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            do_GET = _reply
            do_POST = _reply

        self.destination = ThreadingHTTPServer(("127.0.0.1", 0), Destination)
        target = f"http://localhost:{self.destination.server_address[1]}"

        class Source(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def _redirect(self):
                length = int(self.headers.get("Content-Length") or 0)
                if length:
                    self.rfile.read(length)
                self.send_response(302)
                self.send_header("Location", target + self.path)
                self.send_header("Content-Length", "0")
                self.end_headers()

            do_GET = _redirect
            do_POST = _redirect

        self.source = ThreadingHTTPServer(("127.0.0.1", 0), Source)
        for server in (self.destination, self.source):
            threading.Thread(target=server.serve_forever, daemon=True).start()
        self.base_url = f"http://127.0.0.1:{self.source.server_address[1]}/v1"
        self.tmp = tempfile.TemporaryDirectory()

    def tearDown(self):
        for server in (self.destination, self.source):
            server.shutdown()
            server.server_close()
        self.tmp.cleanup()

    def options(self, image_model: str, references: list[str] | None = None, action: str = "generate"):
        return gen_image.GenerateImageOptions(
            prompt="p", output=str(Path(self.tmp.name) / "out.png"), model="gpt-5.5",
            image_model=image_model, size="64x64", quality="low", action=action, events=None,
            reference_image=references or [], base_url=self.base_url, api_key="fake-token",
            timeout=5.0, output_format="png", topic="t", topic_slug="t", topic_dir=Path(self.tmp.name),
        )

    def test_generation_does_not_follow_redirects(self):
        _body, status, _url = gen_image.call_native_image_backend(self.options("gpt-image-2.5"))
        self.assertEqual(status, 302)
        self.assertEqual(self.destination_auth, [])

    def test_gemini_generation_does_not_follow_redirects(self):
        _body, status, _url = gen_image.call_native_image_backend(self.options("gemini-3.1-flash-image"))
        self.assertEqual(status, 302)
        self.assertEqual(self.destination_auth, [])

    def test_edit_does_not_follow_redirects(self):
        reference = Path(self.tmp.name) / "ref.png"
        reference.write_bytes(png_bytes())
        _body, status, _url = gen_image.call_native_image_backend(
            self.options("gpt-image-2.5", [str(reference)], action="edit"))
        self.assertEqual(status, 302)
        self.assertEqual(self.destination_auth, [])

    def test_model_check_does_not_follow_redirects(self):
        result = image_core.check_connection(self.base_url, "fake-token")
        self.assertFalse(result["ok"])
        self.assertEqual(self.destination_auth, [])

    def test_redirect_status_is_not_transient(self):
        proc = subprocess.run(
            [sys.executable, str(ROOT / "scripts" / "gen_image.py"), "p", "--topic", "t",
             "--topic-root", self.tmp.name, "--size", "64x64", "--base-url", self.base_url,
             "--api-key", "fake-token", "--image-model", "gpt-image-2.5"],
            capture_output=True, text=True, encoding="utf-8", timeout=30,
        )
        self.assertEqual(proc.returncode, 1, proc.stderr)
        self.assertIn("redirect", proc.stderr.lower())
        self.assertNotIn("fake-token", proc.stderr + proc.stdout)
        self.assertEqual(self.destination_auth, [])


if __name__ == "__main__":
    unittest.main()
