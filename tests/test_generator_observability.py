import contextlib
import base64
import importlib.util
import io
import json
import os
import shutil
import subprocess
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

    def test_grok_image_models_use_native_images_endpoint(self):
        gen_image = load_module("gen_image_grok_under_test", ROOT / "scripts" / "gen_image.py")
        models = (
            "grok-imagine-image-2.0",
            "grok-imagine-image",
            "grok-imagine-image-quality",
        )
        calls = []

        def options(image_model: str, references=None):
            return gen_image.GenerateImageOptions(
                prompt="a centered green triangle",
                output="out.png",
                model="unused-main",
                image_model=image_model,
                size="1024x1024",
                quality="low",
                action="generate",
                events=None,
                reference_image=list(references or []),
                base_url="http://127.0.0.1:8317/v1",
                api_key="test-key",
                timeout=1,
                output_format="png",
                topic="test",
                topic_slug="test",
                topic_dir=Path("output/test"),
            )

        original_call = gen_image._call_json_blocking
        gen_image._call_json_blocking = lambda url, payload, args: (
            calls.append((url, payload)) or (b'{"data": []}', 200)
        )
        try:
            for model in models:
                args = options(model)
                body, status, url = gen_image.call_native_image_backend(args)
                self.assertEqual(gen_image.image_backend(model), "openai-images")
                self.assertEqual(status, 200)
                self.assertEqual(url, "http://127.0.0.1:8317/v1/images/generations")
                self.assertEqual(json.loads(body), {"data": []})
                self.assertEqual(calls[-1][1]["model"], model)
                self.assertEqual(calls[-1][1]["prompt"], "a centered green triangle")
                self.assertEqual(calls[-1][1]["n"], 1)

            with self.assertRaisesRegex(RuntimeError, "reference-image editing is not yet supported"):
                gen_image.call_native_image_backend(options(models[0], ["reference.png"]))
        finally:
            gen_image._call_json_blocking = original_call

        self.assertEqual(len(calls), len(models))

    def test_provider_padding_and_exact_output_resize(self):
        gen_image = load_module("gen_image_resize_under_test", ROOT / "scripts" / "gen_image.py")
        from PIL import Image

        source = io.BytesIO()
        Image.new("RGB", (2752, 1536), "white").save(source, format="JPEG")
        normalized = gen_image.normalize_image_bytes(
            source.getvalue(),
            "png",
            "1920x1080",
        )

        self.assertEqual(gen_image.provider_request_size("1920x1080"), "1920x1088")
        with Image.open(io.BytesIO(normalized)) as output:
            self.assertEqual(output.size, (1920, 1080))
            self.assertEqual(output.format, "PNG")

    def test_exact_size_jpeg_payload_is_reencoded_to_requested_png(self):
        gen_image = load_module("gen_image_format_under_test", ROOT / "scripts" / "gen_image.py")
        from PIL import Image

        source = io.BytesIO()
        Image.new("RGB", (1024, 1024), "white").save(source, format="JPEG")
        normalized = gen_image.normalize_image_bytes(source.getvalue(), "png", "1024x1024")

        self.assertTrue(normalized.startswith(b"\x89PNG\r\n\x1a\n"))
        with Image.open(io.BytesIO(normalized)) as output:
            self.assertEqual(output.size, (1024, 1024))
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
    def test_ai_schema_lists_supported_grok_image_models(self):
        ai_image = load_module("ai_image_schema_under_test", ROOT / "tools" / "ai_image.py")
        self.assertEqual(
            ai_image.SCHEMA["image_model"]["examples"],
            [
                "gpt-image-2",
                "gemini-3.1-flash-image",
                "grok-imagine-image-2.0",
                "grok-imagine-image-quality",
                "grok-imagine-image",
            ],
        )
        self.assertEqual(
            ai_image.EXAMPLES["single_grok"]["image_model"],
            "grok-imagine-image-2.0",
        )
        self.assertEqual(
            ai_image.SCHEMA["image_model"]["default"],
            "grok-imagine-image-2.0",
        )
        gen_image = load_module("gen_image_default_under_test", ROOT / "scripts" / "gen_image.py")
        self.assertEqual(gen_image.DEFAULT_IMAGE_MODEL, "grok-imagine-image-2.0")


    def test_ai_batch_passes_image_model_override(self):
        ai_image = load_module("ai_image_batch_model_under_test", ROOT / "tools" / "ai_image.py")
        commands = []
        original_run = ai_image.subprocess.run
        ai_image.subprocess.run = lambda cmd, **kwargs: (
            commands.append(cmd) or ai_image.subprocess.CompletedProcess(cmd, 0, "", "")
        )
        try:
            with tempfile.TemporaryDirectory() as tmp:
                preset = Path(tmp) / "preset.json"
                preset.write_text(
                    json.dumps(
                        {
                            "topic": "batch-provider-test",
                            "categories": [{"name": "test", "templates": ["diagnostic"]}],
                        }
                    ),
                    encoding="utf-8",
                )
                result = ai_image.run_batch(
                    {
                        "preset": str(preset),
                        "topic_root": tmp,
                        "image_model": "grok-imagine-image-2.0",
                        "dry_run": True,
                    }
                )
        finally:
            ai_image.subprocess.run = original_run

        self.assertTrue(result["ok"])
        image_model_index = commands[0].index("--image-model")
        self.assertEqual(commands[0][image_model_index + 1], "grok-imagine-image-2.0")

    def test_gen_batch_cli_overrides_preset_image_model(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            preset = tmp_path / "preset.json"
            preset.write_text(
                json.dumps(
                    {
                        "topic": "batch-override-test",
                        "image_model": "gpt-image-2",
                        "categories": [{"name": "test", "templates": ["diagnostic"]}],
                    }
                ),
                encoding="utf-8",
            )
            proc = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts" / "gen_batch.py"),
                    str(preset),
                    "--count",
                    "1",
                    "--dry-run",
                    "--topic-root",
                    tmp,
                    "--image-model",
                    "grok-imagine-image-2.0",
                ],
                capture_output=True,
                text=True,
                encoding="utf-8",
                cwd=str(ROOT),
            )

            self.assertEqual(proc.returncode, 0, proc.stderr)
            prompt_files = list((tmp_path / "runs" / "batch-override-test").glob("*/prompts.jsonl"))
            self.assertEqual(len(prompt_files), 1)
            prompt = json.loads(prompt_files[0].read_text(encoding="utf-8").strip())
            self.assertEqual(prompt["image_model"], "grok-imagine-image-2.0")

    def test_image_menu_passes_selected_grok_model_to_batch_dry_run(self):
        marker = time.time_ns()
        topic = f"grok-menu-test-{marker}"
        preset = ROOT / "presets" / f"000-{topic}.json"
        run_dir = ROOT / "runs" / topic
        output_dir = ROOT / "output" / topic
        preset.write_text(
            json.dumps(
                {
                    "topic": topic,
                    "image_model": "gpt-image-2",
                    "categories": [{"name": "test", "templates": ["diagnostic"]}],
                }
            ),
            encoding="utf-8",
        )
        env = os.environ.copy()
        env["CLIPROXY_API_KEY"] = "test-key"
        env["CLIPROXY_BASE_URL"] = "http://127.0.0.1:8317/v1"
        try:
            proc = subprocess.run(
                ["pwsh", "-NoProfile", "-File", str(ROOT / "tools" / "Image-Menu.ps1")],
                input="2\n1\n1\n1\nn\ny\n3\nq\n",
                capture_output=True,
                text=True,
                encoding="utf-8",
                env=env,
                cwd=str(ROOT),
                timeout=30,
            )

            self.assertEqual(proc.returncode, 0, proc.stderr)
            prompt_files = list(run_dir.glob("*/prompts.jsonl"))
            self.assertEqual(len(prompt_files), 1)
            prompt = json.loads(prompt_files[0].read_text(encoding="utf-8").strip())
            self.assertEqual(prompt["image_model"], "grok-imagine-image-2.0")
        finally:
            preset.unlink(missing_ok=True)
            shutil.rmtree(run_dir, ignore_errors=True)
            shutil.rmtree(output_dir, ignore_errors=True)

    def test_ai_entrypoint_accepts_utf8_bom_spec_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            preset = tmp_path / "preset.json"
            preset.write_text(
                json.dumps(
                    {
                        "topic": "bom-spec-test",
                        "categories": [{"name": "test", "templates": ["diagnostic"]}],
                    }
                ),
                encoding="utf-8",
            )
            spec = tmp_path / "spec.json"
            spec.write_text(
                json.dumps(
                    {
                        "mode": "batch",
                        "preset": str(preset),
                        "topic_root": tmp,
                        "count": 1,
                        "dry_run": True,
                    }
                ),
                encoding="utf-8-sig",
            )
            proc = subprocess.run(
                [sys.executable, str(ROOT / "tools" / "ai_image.py"), "--spec", str(spec)],
                capture_output=True,
                text=True,
                encoding="utf-8",
                cwd=str(ROOT),
                timeout=60,
            )

            self.assertEqual(proc.returncode, 0, proc.stderr)
            result = json.loads(proc.stdout.strip().splitlines()[-1])
            self.assertTrue(result["ok"], result)

    def test_sync_skills_links_backs_up_and_is_idempotent(self):
        sync_skills = load_module("sync_skills_under_test", ROOT / "scripts" / "sync_skills.py")
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            skills_dir = tmp_path / "repo" / "skills"
            (skills_dir / "demo-skill").mkdir(parents=True)
            (skills_dir / "demo-skill" / "SKILL.md").write_text("canonical", encoding="utf-8")

            linked_root = tmp_path / "agent-a" / "skills"
            linked_root.mkdir(parents=True)
            stale = linked_root / "demo-skill"
            stale.mkdir()
            (stale / "SKILL.md").write_text("stale copy", encoding="utf-8")
            absent_root = tmp_path / "agent-b" / "skills"
            backup_dir = tmp_path / "backups"

            roots = [linked_root, absent_root]
            first = sync_skills.sync(skills_dir, roots, apply=True, backup_dir=backup_dir)

            self.assertEqual(
                [r["action"] for r in first if r["root"] == str(linked_root)], ["linked"]
            )
            self.assertEqual(
                [r["action"] for r in first if r["root"] == str(absent_root)], ["root-absent"]
            )
            self.assertEqual((stale / "SKILL.md").read_text(encoding="utf-8"), "canonical")
            self.assertEqual([p.name for p in linked_root.iterdir()], ["demo-skill"])
            backups = list(backup_dir.iterdir())
            self.assertEqual(len(backups), 1)
            self.assertEqual((backups[0] / "SKILL.md").read_text(encoding="utf-8"), "stale copy")

            second = sync_skills.sync(skills_dir, roots, apply=False, backup_dir=backup_dir)
            self.assertEqual(
                [r["action"] for r in second if r["root"] == str(linked_root)], ["ok"]
            )

            (skills_dir / "demo-skill" / "SKILL.md").write_text("updated", encoding="utf-8")
            self.assertEqual((stale / "SKILL.md").read_text(encoding="utf-8"), "updated")

if __name__ == "__main__":
    unittest.main()
