import contextlib
import base64
import importlib.util
import io
import json
import sys
import tempfile
import time
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


class GeneratorObservabilityTests(unittest.TestCase):
    def test_run_jobs_creates_output_dir_and_emits_progress_to_stderr(self):
        ai_image = load_module("ai_image_under_test", ROOT / "tools" / "ai_image.py")
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            fake_script = tmp_path / "fake_gen_image.py"
            fake_script.write_text(
                "import sys\n"
                "print('fake image complete')\n"
                "raise SystemExit(0)\n",
                encoding="utf-8",
            )
            ai_image.GEN_IMAGE = fake_script

            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                result = ai_image.run_jobs(
                    {
                        "topic": "observability-test",
                        "topic_root": str(tmp_path),
                        "jobs": [{"id": "01_quick", "prompt": "quick"}],
                        "concurrency": 1,
                    }
                )

            self.assertTrue(result["ok"])
            self.assertTrue(Path(result["output_dir"]).is_dir())
            events = [json.loads(line) for line in stderr.getvalue().splitlines() if line.strip()]
            self.assertEqual([event["event"] for event in events], ["job_started", "job_finished"])
            self.assertEqual(events[0]["id"], "01_quick")
            self.assertEqual(events[1]["id"], "01_quick")

    def test_run_jobs_applies_hard_per_job_timeout(self):
        ai_image = load_module("ai_image_timeout_under_test", ROOT / "tools" / "ai_image.py")
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            fake_script = tmp_path / "fake_slow_gen_image.py"
            fake_script.write_text(
                "import time\n"
                "time.sleep(1.0)\n"
                "raise SystemExit(0)\n",
                encoding="utf-8",
            )
            ai_image.GEN_IMAGE = fake_script

            stderr = io.StringIO()
            started = time.time()
            with contextlib.redirect_stderr(stderr):
                result = ai_image.run_jobs(
                    {
                        "topic": "timeout-test",
                        "topic_root": str(tmp_path),
                        "jobs": [{"id": "01_slow", "prompt": "slow"}],
                        "concurrency": 1,
                        "job_timeout_sec": 0.1,
                    }
                )
            elapsed = time.time() - started

            self.assertLess(elapsed, 0.8)
            self.assertFalse(result["ok"])
            self.assertIn("timed out", result["failures"][0]["reason"])

    def test_gen_image_call_cliproxy_enforces_total_timeout(self):
        gen_image = load_module("gen_image_under_test", ROOT / "scripts" / "gen_image.py")

        def slow_urlopen(*args, **kwargs):
            time.sleep(1.0)
            raise AssertionError("urlopen returned after total timeout should have fired")

        original_urlopen = gen_image.urllib.request.urlopen
        gen_image.urllib.request.urlopen = slow_urlopen
        try:
            args = gen_image.GenerateImageOptions(
                prompt="diagnostic",
                output="out.png",
                model="test-main",
                image_model="test-image",
                size="1024x1024",
                quality="low",
                action="generate",
                events=None,
                reference_image=[],
                base_url="http://127.0.0.1:9/v1",
                api_key=None,
                timeout=0.1,
                output_format="png",
                topic="test",
                topic_slug="test",
                topic_dir=Path("output/test"),
            )
            started = time.time()
            with self.assertRaisesRegex(RuntimeError, "Timed out waiting for CLIProxyAPI"):
                gen_image.call_cliproxy(args)
            self.assertLess(time.time() - started, 0.8)
        finally:
            gen_image.urllib.request.urlopen = original_urlopen


    def test_gemini_backend_builds_native_multimodal_request(self):
        gen_image = load_module("gen_image_gemini_under_test", ROOT / "scripts" / "gen_image.py")
        with tempfile.TemporaryDirectory() as tmp:
            reference = Path(tmp) / "reference.png"
            reference.write_bytes(b"\x89PNG\r\n\x1a\nreference")
            args = gen_image.GenerateImageOptions(
                prompt="diagnostic",
                output="out.png",
                model="unused-main",
                image_model="gemini-3.1-flash-image",
                size="1920x1088",
                quality="high",
                action="generate",
                events=None,
                reference_image=[str(reference)],
                base_url="http://127.0.0.1:8317/v1",
                api_key="test-key",
                timeout=1,
                output_format="png",
                topic="test",
                topic_slug="test",
                topic_dir=Path("output/test"),
            )
            payload = gen_image.build_gemini_payload(args)

        self.assertEqual(gen_image.image_backend(args.image_model), "gemini")
        config = payload["generationConfig"]
        self.assertEqual(config["responseModalities"], ["IMAGE", "TEXT"])
        self.assertEqual(config["imageConfig"], {"aspectRatio": "16:9", "imageSize": "2K"})
        self.assertEqual(payload["contents"][0]["parts"][1]["inlineData"]["mimeType"], "image/png")
        self.assertEqual(gen_image._gemini_image_size("512x288"), "512")
        self.assertEqual(gen_image._gemini_image_size("1024x576"), "1K")
        self.assertEqual(gen_image._gemini_image_size("1920x1088"), "2K")
        self.assertEqual(gen_image._gemini_image_size("3840x2160"), "4K")

    def test_native_image_extracts_gemini_and_openai_shapes(self):
        gen_image = load_module("gen_image_extract_under_test", ROOT / "scripts" / "gen_image.py")
        encoded = base64.b64encode(b"image-bytes").decode("ascii")
        gemini_body = json.dumps(
            {"candidates": [{"content": {"parts": [{"inlineData": {"mimeType": "image/jpeg", "data": encoded}}]}}]}
        ).encode()
        openai_body = json.dumps({"data": [{"b64_json": encoded}]}).encode()

        self.assertEqual(gen_image.extract_native_image(gemini_body, "gemini"), (b"image-bytes", "image/jpeg"))
        self.assertEqual(gen_image.extract_native_image(openai_body, "openai-images"), (b"image-bytes", "image/png"))

    def test_provider_padding_and_exact_output_resize(self):
        gen_image = load_module("gen_image_resize_under_test", ROOT / "scripts" / "gen_image.py")
        from PIL import Image

        source = io.BytesIO()
        Image.new("RGB", (2752, 1536), "white").save(source, format="JPEG")
        normalized = gen_image.normalize_image_bytes(
            source.getvalue(),
            "image/jpeg",
            "png",
            "1920x1080",
        )

        self.assertEqual(gen_image.provider_request_size("1920x1080"), "1920x1088")
        with Image.open(io.BytesIO(normalized)) as output:
            self.assertEqual(output.size, (1920, 1080))
            self.assertEqual(output.format, "PNG")

    def test_ai_entrypoint_passes_selected_image_model(self):
        ai_image = load_module("ai_image_model_under_test", ROOT / "tools" / "ai_image.py")
        commands = []
        original_run_child = ai_image._run_child
        ai_image._run_child = lambda cmd, timeout: (commands.append(cmd) or (0, "ok"))
        try:
            with tempfile.TemporaryDirectory() as tmp:
                result = ai_image.run_single(
                    {
                        "prompt": "diagnostic",
                        "topic": "provider-test",
                        "topic_root": tmp,
                        "image_model": "gemini-3.1-flash-image",
                        "count": 1,
                    }
                )
        finally:
            ai_image._run_child = original_run_child

        self.assertTrue(result["ok"])
        image_model_index = commands[0].index("--image-model")
        self.assertEqual(commands[0][image_model_index + 1], "gemini-3.1-flash-image")

    def test_ai_entrypoint_passes_edit_action_for_single(self):
        ai_image = load_module("ai_image_action_under_test", ROOT / "tools" / "ai_image.py")
        commands = []
        original_run_child = ai_image._run_child
        ai_image._run_child = lambda cmd, timeout: (commands.append(cmd) or (0, "ok"))
        try:
            with tempfile.TemporaryDirectory() as tmp:
                result = ai_image.run_single(
                    {
                        "prompt": "preserve source geometry",
                        "topic": "edit-action-test",
                        "topic_root": tmp,
                        "action": "edit",
                    }
                )
        finally:
            ai_image._run_child = original_run_child

        self.assertTrue(result["ok"])
        action_index = commands[0].index("--action")
        self.assertEqual(commands[0][action_index + 1], "edit")

    def test_ai_entrypoint_passes_per_job_action(self):
        ai_image = load_module("ai_image_job_action_under_test", ROOT / "tools" / "ai_image.py")
        commands = []
        original_run_child = ai_image._run_child
        ai_image._run_child = lambda cmd, timeout: (commands.append(cmd) or (0, "ok"))
        try:
            with tempfile.TemporaryDirectory() as tmp:
                result = ai_image.run_jobs(
                    {
                        "topic": "job-edit-action-test",
                        "topic_root": tmp,
                        "action": "generate",
                        "jobs": [{"id": "01_edit", "prompt": "edit", "action": "edit"}],
                        "concurrency": 1,
                    }
                )
        finally:
            ai_image._run_child = original_run_child

        self.assertTrue(result["ok"])
        action_index = commands[0].index("--action")
        self.assertEqual(commands[0][action_index + 1], "edit")
    def test_run_single_resume_skips_existing_outputs(self):
        ai_image = load_module("ai_image_resume_under_test", ROOT / "tools" / "ai_image.py")
        commands = []
        original_run_child = ai_image._run_child
        ai_image._run_child = lambda cmd, timeout: (commands.append(cmd) or (0, "ok"))
        try:
            with tempfile.TemporaryDirectory() as tmp:
                output_dir = Path(tmp) / "output" / "resume-test"
                output_dir.mkdir(parents=True)
                existing = output_dir / "resume-test_001.png"
                existing.write_bytes(b"existing")
                result = ai_image.run_single(
                    {
                        "prompt": "diagnostic",
                        "topic": "resume-test",
                        "topic_root": tmp,
                        "count": 2,
                        "resume": True,
                        "concurrency": 1,
                    }
                )
        finally:
            ai_image._run_child = original_run_child

        self.assertTrue(result["ok"])
        self.assertEqual(len(result["outputs"]), 2)
        self.assertEqual(len(commands), 1)
        self.assertTrue(result["outputs"][0].endswith("resume-test_001.png"))


    def test_ai_entrypoint_rejects_invalid_action(self):
        ai_image = load_module("ai_image_invalid_action_under_test", ROOT / "tools" / "ai_image.py")
        result = ai_image.run_single({"prompt": "diagnostic", "action": "preserve"})

        self.assertFalse(result["ok"])
        self.assertIn("invalid action", result["error"])

if __name__ == "__main__":
    unittest.main()
