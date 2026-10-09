"""Storage checks use a loopback fixture, never a real image provider."""
import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "tools"))
from gallery import topic_dirs
from organize_gallery import apply_moves, plan_moves


class GalleryTests(unittest.TestCase):
    def test_model_discovery_lists_live_image_ids_and_creates_no_files(self):
        ids = ["gpt-image-2.5", "gpt-image-2.5-flare", "gpt-image-2.5-sunburst",
               "gemini-3.1-flash-image", "grok-imagine-image-2.0", "gemini-3.1-pro",
               "gpt-image-2.5", "grok-imagine-video"]
        status = 200
        authorizations = []

        class Provider(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                self.server.test_case.assertEqual(self.path, "/v1/models")
                authorizations.append(self.headers.get("Authorization"))
                body = json.dumps({"data": [{"id": model} for model in ids]}).encode()
                self.send_response(status)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Provider)
        server.test_case = self
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        env = {**os.environ, "CLIPROXY_BASE_URL": f"http://127.0.0.1:{server.server_port}/v1",
               "CLIPROXY_API_KEY": "models-fixture-secret", "PYTHONDONTWRITEBYTECODE": "1",
               "PYTHONIOENCODING": "utf-8"}
        with tempfile.TemporaryDirectory() as tmp:
            def run():
                process = subprocess.run([sys.executable, "-B", str(ROOT / "tools/ai_image.py"), "--list-models"],
                                         cwd=tmp, env=env, capture_output=True, text=True,
                                         encoding="utf-8", timeout=20)
                self.assertNotIn(env["CLIPROXY_API_KEY"], process.stdout + process.stderr)
                self.assertEqual(list(Path(tmp).iterdir()), [])
                return process, json.loads(process.stdout)

            process, result = run()
            self.assertEqual(process.returncode, 0, result)
            self.assertEqual(result["available_models"], ids[:5])
            supports_references = {model["id"]: model["reference_images"] for model in result["models"]}
            self.assertTrue(supports_references["gpt-image-2.5-sunburst"])
            self.assertFalse(supports_references["grok-imagine-image-2.0"])
            ids[:] = ["gemini-3.1-pro"]
            process, result = run()
            self.assertNotEqual(process.returncode, 0)
            self.assertFalse(result["ok"])
            self.assertEqual(result["available_models"], [])
            status = 401
            process, result = run()
            self.assertNotEqual(process.returncode, 0)
            self.assertFalse(result["ok"])
            self.assertIn("401", result["error"])
            self.assertEqual(authorizations, ["Bearer models-fixture-secret"] * 3)

    def test_generation_modes_record_prompts_and_keep_exports_image_only(self):
        data = io.BytesIO()
        Image.new("RGB", (8, 8), "orange").save(data, "PNG")
        encoded = base64.b64encode(data.getvalue()).decode()
        calls = []

        class Provider(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                calls.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
                response = {"data": [{"b64_json": encoded}]}
                if calls[-1].get("model") == "gpt-image-2.5":
                    response["model"] = "gpt-image-2.5-sunburst"
                body = json.dumps(response).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Provider)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        env = {**os.environ, "CLIPROXY_BASE_URL": f"http://127.0.0.1:{server.server_port}/v1",
               "CLIPROXY_API_KEY": "gallery-fixture-secret", "PYTHONDONTWRITEBYTECODE": "1"}
        env.pop("CLIPROXY_IMAGE_MODEL", None)
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp)

            def run(spec):
                process = subprocess.run([sys.executable, "-B", str(ROOT / "tools/ai_image.py")],
                                         input=json.dumps(spec), cwd=workspace, env=env,
                                         capture_output=True, text=True, encoding="utf-8", timeout=20)
                return process, json.loads(process.stdout)

            spec = {"prompt": "귀여운 고양이", "topic": "cute-cat", "size": "8x8"}
            _, planned = run({**spec, "dry_run": True})
            self.assertTrue(planned["ok"])
            self.assertFalse((workspace / "ImageGallery").exists())
            self.assertEqual(calls, [])
            _, result = run(spec)
            self.assertTrue(result["ok"], result)
            self.assertEqual(calls[-1]["model"], "gpt-image-2.5")
            image = workspace / "ImageGallery/output/cute-cat/cute-cat.png"
            record = workspace / "ImageGallery/metadata/cute-cat/cute-cat.png.json"
            self.assertEqual(result["outputs"], [str(image)])
            self.assertEqual(json.loads(record.read_text(encoding="utf-8"))["prompt"], spec["prompt"])
            self.assertEqual(json.loads(record.read_text(encoding="utf-8"))["image_model"], "gpt-image-2.5")
            original = image.read_bytes()
            process, result = run(spec)
            self.assertNotEqual(process.returncode, 0)
            self.assertFalse(result["ok"])
            self.assertEqual(image.read_bytes(), original)
            self.assertEqual(len(calls), 1)
            _, result = run({**spec, "resume": True})
            self.assertTrue(result["ok"])
            self.assertEqual(len(calls), 1)
            _, result = run({"mode": "jobs", "topic": "poses", "size": "8x8",
                             "topic_root": str(workspace / "ImageGallery"),
                             "jobs": [{"id": "sleep", "prompt": "sleeping kitten"},
                                      {"id": "run", "prompt": "running kitten", "image_model": "gpt-image-2.5-flare"}]})
            self.assertTrue(result["ok"], result)
            self.assertEqual({call["prompt"]: call["model"] for call in calls[-2:]},
                             {"sleeping kitten": "gpt-image-2.5", "running kitten": "gpt-image-2.5-flare"})
            preset = workspace / "preset.json"
            preset.write_text(json.dumps({"topic": "batch", "size": "8x8", "categories": [
                {"name": "cat", "templates": ["fluffy cat"]}]}), encoding="utf-8")
            _, result = run({"mode": "batch", "preset": str(preset), "count": 2})
            self.assertTrue(result["ok"], result)
            self.assertEqual([call["model"] for call in calls[-2:]], ["gpt-image-2.5"] * 2)
            self.assertEqual(len(result["outputs"]), 2)
            output, metadata = topic_dirs(workspace, "batch")
            self.assertEqual(len(list(output.glob("*.png"))), 2)
            self.assertEqual(len(list(metadata.glob("*.json"))), 2)
            for path in (workspace / "ImageGallery/output").rglob("*"):
                if path.is_file():
                    self.assertEqual(path.suffix, ".png")
            for path in (workspace / "ImageGallery/metadata").rglob("*.json"):
                self.assertNotIn(env["CLIPROXY_API_KEY"], path.read_text(encoding="utf-8"))
            self.assertFalse((workspace / "output").exists())
            self.assertFalse((workspace / "runs").exists())

            project = workspace / "projects/minecraft-map"
            _, result = run({**spec, "topic_root": str(project), "image_model": "gpt-image-2.5-sunburst"})
            self.assertTrue(result["ok"], result)
            self.assertEqual(calls[-1]["model"], "gpt-image-2.5-sunburst")
            project_image = project / "ImageGallery/output/cute-cat/cute-cat.png"
            self.assertEqual(result["outputs"], [str(project_image)])
            model_record = json.loads((project / "ImageGallery/metadata/cute-cat/cute-cat.png.json").read_text(encoding="utf-8"))
            self.assertEqual(model_record["image_model"], "gpt-image-2.5-sunburst")
            self.assertNotIn("response_model", model_record)
            _, result = run({**spec, "topic": "alias", "topic_root": str(project), "image_model": "gpt-image-2.5"})
            self.assertTrue(result["ok"], result)
            model_record = json.loads((project / "ImageGallery/metadata/alias/alias.png.json").read_text(encoding="utf-8"))
            self.assertEqual(model_record["image_model"], "gpt-image-2.5")
            self.assertEqual(model_record["response_model"], "gpt-image-2.5-sunburst")

            from mcp_service import ImageService
            from mcp_models import GenerationSpec
            direct_root = workspace / "direct-cli"
            for selected in (None, "grok-imagine-image-2.0"):
                command = [sys.executable, str(ROOT / "scripts/gen_image.py"), "direct model selection",
                           "--topic-root", str(direct_root), "--topic", selected or "default", "--size", "8x8"]
                if selected:
                    command.extend(["--image-model", selected])
                process = subprocess.run(command, env=env, capture_output=True, text=True, encoding="utf-8", timeout=20)
                self.assertEqual(process.returncode, 0, process.stderr)
                self.assertEqual(calls[-1]["model"], selected or "gpt-image-2.5")
            service = ImageService(root=workspace)
            service.base_url = env["CLIPROXY_BASE_URL"]
            service.key = env["CLIPROXY_API_KEY"]
            try:
                plan = service.plan_generation(GenerationSpec(prompt="MCP kitten", topic="mcp-cat", size="8x8"))
                self.assertEqual(plan["spec"]["jobs"][0]["image_model"], "gpt-image-2.5")
                selected_plan = service.plan_generation(GenerationSpec(prompt="MCP explicit model", topic="mcp-chosen",
                                                                        image_model="gpt-image-2.5-sunburst"))
                self.assertEqual(selected_plan["spec"]["jobs"][0]["image_model"], "gpt-image-2.5-sunburst")
                self.assertFalse(Path(plan["output_dir"]).exists())
                job = service.start_generation(plan["plan_id"])
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    job = service.get_job(job["job_id"])
                    if job["status"] in {"succeeded", "failed"}:
                        break
                    time.sleep(0.05)
                self.assertEqual(job["status"], "succeeded", job)
                self.assertFalse((service.state / "specs").exists())
                self.assertEqual(len(service.list_images()["images"]), 6)
                resized = service.resize_image(job["outputs"][0], 4, 4)
                self.assertTrue(Path(resized["path"]).is_file())
                self.assertTrue(str(service.state).startswith(str(workspace / "ImageGallery/metadata")))
                self.assertFalse(any(p.suffix != ".png" for p in (workspace / "ImageGallery/output").rglob("*") if p.is_file()))
            finally:
                service.close()

    def test_migration_preserves_collisions_updates_records_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp)
            old = workspace / "ImageGallery/cat/output/cat/image.png"
            old.parent.mkdir(parents=True)
            old.write_bytes(b"old-image")
            existing = workspace / "ImageGallery/output/cat/image.png"
            existing.parent.mkdir(parents=True)
            existing.write_bytes(b"new-image")
            record = workspace / "ImageGallery/cat/spec.json"
            record.write_text(json.dumps({"reference_images": [str(old)], "output_dir": str(old.parent)}), encoding="utf-8")
            moves = plan_moves(workspace)
            self.assertEqual(len(moves), 2)
            report = apply_moves(workspace, moves)
            self.assertEqual(report["moved"], 2)
            renamed = existing.with_name("image__legacy_1.png")
            self.assertEqual(renamed.read_bytes(), b"old-image")
            self.assertEqual(existing.read_bytes(), b"new-image")
            saved = json.loads((workspace / "ImageGallery/metadata/cat/spec.json").read_text(encoding="utf-8"))
            self.assertEqual(saved["reference_images"], [str(renamed)])
            self.assertEqual(saved["output_dir"], str(existing.parent))
            self.assertEqual(plan_moves(workspace), [])
            self.assertTrue(Path(report["record"]).is_file())


if __name__ == "__main__":
    unittest.main()
