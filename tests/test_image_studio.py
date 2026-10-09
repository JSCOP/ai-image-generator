"""Observable local-server boundaries; no provider requests or paid generations."""
import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import http.client
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]


class ImageStudioBoundaryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temp.cleanup)
        cls.root = Path(cls.temp.name)
        for directory in ("tools", "scripts"):
            shutil.copytree(ROOT / directory, cls.root / directory,
                            ignore=shutil.ignore_patterns("__pycache__"))
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            cls.port = listener.getsockname()[1]
        cls.origin = f"http://127.0.0.1:{cls.port}"
        # A dead loopback endpoint ensures these tests cannot spend provider quota.
        environment = {**os.environ, "CLIPROXY_BASE_URL": "http://127.0.0.1:1/v1",
                       "CLIPROXY_API_KEY": "boundary-test-key", "PYTHONUTF8": "1"}
        cls.process = subprocess.Popen(
            [sys.executable, str(cls.root / "tools/image_studio.py"), "--port", str(cls.port)],
            cwd=cls.root, env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        cls.addClassCleanup(cls.stop_server)
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if cls.process.poll() is not None:
                raise RuntimeError(f"Studio exited during startup: {cls.process.returncode}")
            try:
                status, _, body = cls.request("GET", "/api/config")
                if status == 200:
                    cls.token = json.loads(body)["csrf_token"]
                    return
            except OSError:
                pass
            time.sleep(0.05)
        raise RuntimeError("Studio did not become ready within 15 seconds")

    @classmethod
    def stop_server(cls):
        if cls.process.poll() is None:
            cls.process.terminate()
        cls.process.wait(timeout=10)

    @classmethod
    def request(cls, method, path, payload=None, headers=None):
        request_headers = dict(headers or {})
        body = None if payload is None else json.dumps(payload).encode("utf-8")
        if body is not None:
            request_headers.setdefault("Content-Type", "application/json")
        connection = http.client.HTTPConnection("127.0.0.1", cls.port, timeout=5)
        try:
            connection.request(method, path, body, request_headers)
            response = connection.getresponse()
            return response.status, dict(response.getheaders()), response.read()
        finally:
            connection.close()

    def authorized(self):
        return {"Origin": self.origin, "X-Studio-Token": self.token}

    def valid_request(self):
        return {"prompt": "A green cube", "positive": "", "negative": "",
                "image_model": "grok-imagine-image-2.0", "size": "1920x1080",
                "quality": "high", "references": []}

    def test_foreign_origins_and_missing_token_cannot_generate(self):
        _, _, before = self.request("GET", "/api/jobs")
        for headers in ({"Origin": "https://untrusted.example", "X-Studio-Token": self.token},
                        {"Origin": "http://127.0.0.1:9", "X-Studio-Token": self.token},
                        {"Origin": self.origin}):
            with self.subTest(headers=list(headers)):
                status, _, _ = self.request("POST", "/api/jobs", self.valid_request(), headers)
                self.assertEqual(status, 403)
        status, _, body = self.request("GET", "/api/jobs")
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body)["jobs"], json.loads(before)["jobs"])

    def test_rebound_host_cannot_read_bootstrap_or_files(self):
        for path in ("/api/config", "/"):
            with self.subTest(path=path):
                status, _, _ = self.request("GET", path, headers={"Host": "attacker.example"})
                self.assertIn(status, (400, 403, 421))

    def test_reference_and_model_validation_prevents_provider_calls(self):
        _, _, before = self.request("GET", "/api/jobs")
        cases = [
            {"image_model": "unknown-image-model"},
            {"size": "999999x999999"},
            {"prompt": "", "positive": "", "negative": "only exclusions"},
            {"references": [{"name": "ref.png", "data_url": "data:image/png;base64,bm90LWFuLWltYWdl"}]},
            {"image_model": "gpt-image-2", "references": [
                {"name": "ref.png", "data_url": "data:image/png;base64,bm90LWFuLWltYWdl"}]},
        ]
        for patch in cases:
            with self.subTest(patch=patch):
                status, _, _ = self.request("POST", "/api/jobs", {**self.valid_request(), **patch}, self.authorized())
                self.assertEqual(status, 400)
        _, _, body = self.request("GET", "/api/jobs")
        self.assertEqual(json.loads(body)["jobs"], json.loads(before)["jobs"])

    def test_gpt_image_2_5_models_are_listed_and_use_edit_for_references(self):
        status, _, body = self.request("GET", "/api/config")
        self.assertEqual(status, 200)
        models = {m["id"]: m for m in json.loads(body)["models"]}
        for model_id in ("gpt-image-2.5", "gpt-image-2.5-flare", "gpt-image-2.5-sunburst"):
            with self.subTest(model=model_id):
                self.assertIn(model_id, models)
                self.assertTrue(models[model_id]["reference_images"])

    def test_gpt_edit_models_cover_every_gpt_image_catalog_entry(self):
        sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
        try:
            import image_studio
        finally:
            sys.path.pop(0)
        gpt_ids = {m["id"] for m in image_studio.MODELS if m["id"].startswith("gpt-image-")}
        self.assertTrue({"gpt-image-2.5", "gpt-image-2.5-flare", "gpt-image-2.5-sunburst"} <= gpt_ids)
        self.assertEqual(image_studio.GPT_EDIT_MODELS, gpt_ids)

    def test_static_routes_never_expose_source_or_settings(self):
        for path in ("/scripts/gen_image.py", "/config/image-studio.local.json",
                     "/files/../../scripts/gen_image.py", "/files/%2e%2e/%2e%2e/scripts/gen_image.py"):
            with self.subTest(path=path):
                status, _, _ = self.request("GET", path)
                self.assertIn(status, (400, 403, 404))

    def test_session_connection_generates_downloadable_image_without_saving_key(self):
        source = io.BytesIO()
        Image.new("RGB", (64, 64), "#23875b").save(source, format="PNG")
        image_payload = base64.b64encode(source.getvalue()).decode("ascii")
        session_key = "session-only-fixture-key"
        reject = threading.Event()
        requested_models = []

        class ProviderFixture(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def reply(self, payload, status=200):
                encoded = json.dumps(payload).encode("utf-8")
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(encoded)))
                self.end_headers()
                self.wfile.write(encoded)

            def do_GET(self):
                if self.path != "/v1/models" or self.headers.get("Authorization") != "Bearer " + session_key:
                    self.send_error(401)
                    return
                self.reply({"data": [{"id": "grok-imagine-image-2.0"}, {"id": "gpt-image-2.5"}]})

            def do_POST(self):
                if self.path != "/v1/images/generations" or self.headers.get("Authorization") != "Bearer " + session_key:
                    self.send_error(401)
                    return
                requested_models.append(json.loads(self.rfile.read(int(self.headers["Content-Length"])))['model'])
                if reject.is_set():
                    self.reply({"error": {"message": f"Insufficient credits: {session_key} data:image/png;base64,privatebytes"}}, 402)
                    return
                self.reply({"data": [{"b64_json": image_payload}]})

        fixture = ThreadingHTTPServer(("127.0.0.1", 0), ProviderFixture)
        worker = threading.Thread(target=fixture.serve_forever, daemon=True)
        worker.start()
        self.addCleanup(fixture.server_close)
        self.addCleanup(fixture.shutdown)
        base_url = f"http://127.0.0.1:{fixture.server_port}/v1"
        status, _, body = self.request("POST", "/api/connection",
                                      {"base_url": base_url, "api_key": session_key}, self.authorized())
        self.assertEqual(status, 200)
        self.assertTrue(json.loads(body)["ok"], body)
        _, _, config = self.request("GET", "/api/config")
        self.assertEqual(json.loads(config)["defaults"]["image_model"], "gpt-image-2.5")
        default_request = self.valid_request()
        default_request.pop("image_model")
        status, _, body = self.request("POST", "/api/jobs", default_request, self.authorized())
        self.assertEqual(status, 202, body)
        job = json.loads(body)
        deadline = time.monotonic() + 20
        while job["status"] in ("queued", "running") and time.monotonic() < deadline:
            time.sleep(0.05)
            _, _, body = self.request("GET", "/api/jobs/" + job["id"])
            job = json.loads(body)
        self.assertEqual(job["status"], "succeeded", job)
        self.assertEqual(requested_models[-1], "gpt-image-2.5")
        self.assertEqual(job["image_model"], "gpt-image-2.5")
        status, _, image = self.request("GET", job["images"][0]["url"])
        self.assertEqual(status, 200)
        with Image.open(io.BytesIO(image)) as saved:
            self.assertEqual(saved.size, (1920, 1080))
            self.assertEqual(saved.format, "PNG")
        for path in (self.root / "config/image-studio.local.json",
                     self.root / "ImageGallery/metadata" / job["id"] / "job.json"):
            self.assertNotIn(session_key, path.read_text(encoding="utf-8"))
        reject.set()
        status, _, body = self.request("POST", "/api/jobs", self.valid_request(), self.authorized())
        self.assertEqual(status, 202)
        failed = json.loads(body)
        deadline = time.monotonic() + 20
        while failed["status"] in ("queued", "running") and time.monotonic() < deadline:
            time.sleep(0.05)
            _, _, body = self.request("GET", "/api/jobs/" + failed["id"])
            failed = json.loads(body)
        self.assertEqual(failed["status"], "failed", failed)
        self.assertEqual(requested_models[-1], "grok-imagine-image-2.0")
        self.assertIn("Insufficient credits", failed["error"])
        saved_error = (self.root / "ImageGallery/metadata" / failed["id"] / "job.json").read_text(encoding="utf-8")
        self.assertNotIn(session_key, saved_error)
        self.assertNotIn("privatebytes", saved_error)


if __name__ == "__main__":
    unittest.main()
