"""Parallel generation and transient-failure retry; loopback fixtures only, no provider quota."""
import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import io
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
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


def provider_fixture(statuses: list[int]):
    """Serve /v1/images/generations, replying with each queued status, then 200."""
    source = io.BytesIO()
    Image.new("RGB", (64, 64), "#2b6cb0").save(source, format="PNG")
    image_payload = base64.b64encode(source.getvalue()).decode("ascii")
    queue = list(statuses)
    lock = threading.Lock()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            self.rfile.read(int(self.headers["Content-Length"]))
            with lock:
                status = queue.pop(0) if queue else 200
            body = {"data": [{"b64_json": image_payload}]} if status == 200 else {"error": {"message": f"fixture {status}"}}
            encoded = json.dumps(body).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


class TransientExitCodeTests(unittest.TestCase):
    def run_gen_image(self, status: int, tmp: str) -> subprocess.CompletedProcess:
        server = provider_fixture([status])
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        return subprocess.run(
            [sys.executable, str(ROOT / "scripts" / "gen_image.py"), "diagnostic",
             "--topic", "exit-code", "--topic-root", tmp, "-o", "x.png", "--size", "64x64",
             "--image-model", "gpt-image-2.5", "--base-url", f"http://127.0.0.1:{server.server_port}/v1",
             "--api-key", "fixture-key", "--timeout", "20"],
            capture_output=True, text=True, encoding="utf-8", cwd=str(ROOT), timeout=60,
        )

    def test_rate_limit_and_server_errors_use_transient_exit_code(self):
        for status in (429, 503):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as tmp:
                proc = self.run_gen_image(status, tmp)
                self.assertEqual(proc.returncode, 3, proc.stderr)

    def test_client_errors_are_not_marked_transient(self):
        for status in (400, 402):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as tmp:
                proc = self.run_gen_image(status, tmp)
                self.assertEqual(proc.returncode, 1, proc.stderr)

    def test_unreachable_proxy_is_transient(self):
        with tempfile.TemporaryDirectory() as tmp:
            proc = subprocess.run(
                [sys.executable, str(ROOT / "scripts" / "gen_image.py"), "diagnostic",
                 "--topic", "exit-code", "--topic-root", tmp, "-o", "x.png", "--size", "64x64",
                 "--image-model", "gpt-image-2.5", "--base-url", "http://127.0.0.1:1/v1",
                 "--api-key", "fixture-key", "--timeout", "5"],
                capture_output=True, text=True, encoding="utf-8", cwd=str(ROOT), timeout=60,
            )
        self.assertEqual(proc.returncode, 3, proc.stderr)


class ParallelRunnerTests(unittest.TestCase):
    def setUp(self):
        self.ai_image = load_module(f"ai_image_parallel_{self.id()}", ROOT / "tools" / "ai_image.py")
        self.ai_image.RETRY_BACKOFF_SEC = 0
        self.original_run_child = self.ai_image._run_child
        self.addCleanup(setattr, self.ai_image, "_run_child", self.original_run_child)

    def test_default_concurrency_runs_thirty_images_at_once(self):
        lock = threading.Lock()
        in_flight = {"now": 0, "max": 0}

        def fake(cmd, timeout):
            with lock:
                in_flight["now"] += 1
                in_flight["max"] = max(in_flight["max"], in_flight["now"])
            time.sleep(0.3)
            with lock:
                in_flight["now"] -= 1
            return 0, "ok"

        self.ai_image._run_child = fake
        with tempfile.TemporaryDirectory() as tmp:
            result = self.ai_image.run_single({"prompt": "p", "topic": "par", "topic_root": tmp, "count": self.ai_image.DEFAULT_CONCURRENCY + 2})
        self.assertTrue(result["ok"], result)
        self.assertEqual(result["concurrency"], self.ai_image.DEFAULT_CONCURRENCY)
        self.assertEqual(in_flight["max"], self.ai_image.DEFAULT_CONCURRENCY)

    def test_concurrency_is_clamped_to_maximum(self):
        self.ai_image._run_child = lambda cmd, timeout: (0, "ok")
        with tempfile.TemporaryDirectory() as tmp:
            result = self.ai_image.run_jobs({
                "topic": "clamp", "topic_root": tmp, "concurrency": 500,
                "jobs": [{"id": f"j{i}", "prompt": "p"} for i in range(40)],
            })
        self.assertTrue(result["ok"], result)
        self.assertEqual(result["concurrency"], self.ai_image.MAX_CONCURRENCY)

    def test_transient_failures_are_retried_then_succeed(self):
        calls = []

        def fake(cmd, timeout):
            calls.append(cmd[cmd.index("-o") + 1])
            return (3, "CLIProxyAPI request failed: HTTP 429") if len(calls) <= 2 else (0, "ok")

        self.ai_image._run_child = fake
        with tempfile.TemporaryDirectory() as tmp:
            result = self.ai_image.run_single({"prompt": "p", "topic": "retry", "topic_root": tmp,
                                               "count": 1, "retries": 2})
        self.assertTrue(result["ok"], result)
        self.assertEqual(len(calls), 3)
        self.assertEqual(result["retried"], 2)

    def test_timeouts_are_retried_but_permanent_errors_are_not(self):
        attempts = {"timeout": 0, "permanent": 0}

        def fake(cmd, timeout):
            name = cmd[cmd.index("-o") + 1]
            key = "timeout" if name.startswith("t") else "permanent"
            attempts[key] += 1
            return (124, "timed out") if key == "timeout" else (1, "HTTP 400 bad prompt")

        self.ai_image._run_child = fake
        with tempfile.TemporaryDirectory() as tmp:
            result = self.ai_image.run_jobs({
                "topic": "mixed", "topic_root": tmp, "retries": 2,
                "jobs": [{"id": "t1", "prompt": "p"}, {"id": "p1", "prompt": "p"}],
            })
        self.assertFalse(result["ok"])
        self.assertEqual(attempts, {"timeout": 3, "permanent": 1})
        self.assertEqual(len(result["failures"]), 2)

    def test_retries_zero_disables_retry(self):
        calls = []
        self.ai_image._run_child = lambda cmd, timeout: (calls.append(1) or (3, "HTTP 503"))
        with tempfile.TemporaryDirectory() as tmp:
            result = self.ai_image.run_single({"prompt": "p", "topic": "noretry", "topic_root": tmp, "retries": 0})
        self.assertFalse(result["ok"])
        self.assertEqual(len(calls), 1)

    def test_end_to_end_parallel_generation_recovers_from_rate_limit(self):
        server = provider_fixture([429, 429])
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        env = {**os.environ, "CLIPROXY_BASE_URL": f"http://127.0.0.1:{server.server_port}/v1",
               "CLIPROXY_API_KEY": "fixture-key", "PYTHONUTF8": "1", "AI_IMAGE_RETRY_BACKOFF_SEC": "0"}
        with tempfile.TemporaryDirectory() as tmp:
            spec = {"prompt": "diagnostic", "topic": "e2e-par", "topic_root": tmp, "count": 6,
                    "size": "64x64", "image_model": "gpt-image-2.5"}
            proc = subprocess.run(
                [sys.executable, str(ROOT / "tools" / "ai_image.py"), "--json", json.dumps(spec)],
                capture_output=True, text=True, encoding="utf-8", cwd=str(ROOT), env=env, timeout=120,
            )
            result = json.loads(proc.stdout.strip().splitlines()[-1])
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertTrue(result["ok"], result)
            self.assertEqual(len(result["outputs"]), 6)
            self.assertEqual(result["retried"], 2)
            for path in result["outputs"]:
                with Image.open(path) as im:
                    self.assertEqual(im.size, (64, 64))


class McpConcurrencyTests(unittest.TestCase):
    def test_mcp_spec_defaults_to_parallel_generation(self):
        sys.path.insert(0, str(ROOT / "tools"))
        try:
            from mcp_models import GenerationSpec
        finally:
            sys.path.remove(str(ROOT / "tools"))
        self.assertEqual(GenerationSpec(prompt="p").concurrency, 30)
        self.assertEqual(GenerationSpec(prompt="p", concurrency=32).concurrency, 32)
        with self.assertRaises(ValueError):
            GenerationSpec(prompt="p", concurrency=33)


if __name__ == "__main__":
    unittest.main()
