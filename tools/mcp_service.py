"""Local image operations and durable generation jobs behind the MCP server.

Generation always goes through ai_image.py; planning never invokes a provider.
One process owns a state directory. Different clients may use --state-dir.
"""
from __future__ import annotations

import copy
import io
import json
import os
import random
import re
import signal
import subprocess
import sys
import threading
import time
import uuid
from pathlib import Path
from typing import Any

from PIL import Image, ImageDraw, ImageOps

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "tools"))
import gen_batch
import ai_image
import image_studio as studio
from gallery import gallery_root, topic_dirs, metadata_path, write_json
from mcp_models import CropRegion, GenerationSpec, ImageOptions, PresetSpec

TERMINAL = {"succeeded", "failed", "cancelled", "interrupted"}


class FileLock:
    """Nonblocking cross-process lock, released by the OS on process death."""
    def __init__(self, path: Path):
        path.parent.mkdir(parents=True, exist_ok=True)
        self.file = path.open("a+b")
        self.file.seek(0, 2)
        if self.file.tell() == 0:
            self.file.write(b"0")
            self.file.flush()
        self.file.seek(0)
        try:
            if os.name == "nt":
                import msvcrt
                msvcrt.locking(self.file.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(self.file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            self.file.close()
            raise ValueError("Directory is in use by another MCP server/job") from None

    def close(self):
        if not self.file.closed:
            self.file.close()


class ImageService:
    def __init__(self, root: Path = ROOT, state_dir: Path | None = None,
                 allowed_roots: list[Path] | None = None):
        self.root = root.resolve()
        self.allowed_roots = [self.root] + [p.resolve() for p in allowed_roots or []]
        self.state = (state_dir or gallery_root(self.root) / "metadata" / "_mcp").resolve()
        self._owner = FileLock(self.state / ".owner.lock")
        self._lock = threading.RLock()
        self._processes: dict[str, subprocess.Popen] = {}
        self._threads: dict[str, threading.Thread] = {}
        self._closed = False
        self.key = studio.resolve_api_key()
        self.base_url = studio.load_base_url()
        for path in (self.state / "jobs").glob("*.json"):
            rec = self._read_json(path)
            if rec["status"] not in TERMINAL:
                rec.update(status="interrupted", error="Server stopped; use resume_job", updated_at=time.time())
                self._write(path, rec)

    def scrub(self, value: Any) -> Any:
        if isinstance(value, str):
            if self.key:
                value = value.replace(self.key, "[REDACTED]")
            return re.sub(r"(?i)Bearer\s+[^\s\"']+", "Bearer [REDACTED]", value)
        if isinstance(value, dict):
            return {k: self.scrub(v) for k, v in value.items()}
        if isinstance(value, list):
            return [self.scrub(v) for v in value]
        return value

    def path(self, value: str | Path, *, exists: bool = False) -> Path:
        path = Path(value)
        path = (path if path.is_absolute() else self.root / path).resolve()
        if not any(path.is_relative_to(root) for root in self.allowed_roots):
            raise ValueError("Path outside allowed roots; configure --allow-root on the server")
        if exists and not path.is_file():
            raise ValueError(f"File not found: {path}")
        return path

    def _read_json(self, path: Path) -> dict:
        if path.stat().st_size > 16 * 1024 * 1024:
            raise ValueError("JSON file exceeds 16 MiB")
        return json.loads(path.read_text(encoding="utf-8-sig"))

    def _write(self, path: Path, value: Any):
        write_json(path, self.scrub(value))

    def _record_path(self, kind: str, record_id: str) -> Path:
        if not re.fullmatch(r"[0-9a-f]{32}", record_id):
            raise ValueError("Invalid record ID")
        path = self.state / kind / f"{record_id}.json"
        if not path.is_file():
            raise ValueError(f"Unknown {kind} ID: {record_id}")
        return path

    def _image(self, value: str) -> Image.Image:
        path = self.path(value, exists=True)
        if path.suffix.lower() not in studio.IMAGE_SUFFIXES:
            raise ValueError("Only PNG/JPEG/WebP images are supported")
        if path.stat().st_size > 64 * 1024 * 1024:
            raise ValueError("Image exceeds 64 MiB")
        with Image.open(path) as source:
            if source.width > 8192 or source.height > 8192 or source.width * source.height > 50_000_000:
                raise ValueError("Image exceeds 8192px per side or 50 million pixels")
            source.load()
            return ImageOps.exif_transpose(source).copy()

    def image_info(self, path: str) -> dict:
        resolved = self.path(path, exists=True)
        with self._image(path) as im:
            return {"path": str(resolved), "width": im.width, "height": im.height,
                    "mode": im.mode, "bytes": resolved.stat().st_size}

    def preview(self, path: str, max_side: int = 1536) -> bytes:
        if not 64 <= max_side <= 2048:
            raise ValueError("max_side must be 64..2048")
        with self._image(path) as im:
            im.thumbnail((max_side, max_side), Image.Resampling.LANCZOS)
            data = io.BytesIO()
            im.save(data, format="PNG")
            return data.getvalue()

    def list_images(self, directory: str = "ImageGallery/output", offset: int = 0, limit: int = 50) -> dict:
        if offset < 0 or not 1 <= limit <= 200:
            raise ValueError("offset >= 0 and limit 1..200 required")
        directory_path = self.path(directory)
        if not directory_path.is_dir():
            raise ValueError("Image directory does not exist")
        files = sorted(p for p in directory_path.rglob("*") if p.is_file() and p.suffix.lower() in studio.IMAGE_SUFFIXES
                       and any(p.resolve().is_relative_to(root) for root in self.allowed_roots))
        return {"total": len(files), "offset": offset,
                "images": [{"path": str(p.resolve()), "bytes": p.stat().st_size} for p in files[offset:offset + limit]]}

    def list_presets(self) -> list[dict]:
        return [{"name": p.stem, "path": str(p)} for p in sorted((self.root / "presets").glob("*.json"))]

    def studio_catalog(self) -> list[dict]:
        return copy.deepcopy(studio.MODELS)

    def available_models(self) -> list[str]:
        result = self.check_connection()
        return result.get("available_models", []) if result.get("ok") else []

    def check_connection(self) -> dict:
        return self.scrub(studio.check_connection(self.base_url, self.key))

    def get_preset(self, path: str) -> dict:
        return self._read_json(self.path(path, exists=True))

    def save_preset(self, name: str, preset: PresetSpec) -> dict:
        slug = ai_image._safe_topic_slug(name)
        path = self.path(self.root / "presets" / f"{slug}.json")
        if any(not t.strip() for c in preset.categories for t in c.templates):
            raise ValueError("Preset templates cannot be blank")
        # Exclusive creation: existing presets are never overwritten.
        path.parent.mkdir(parents=True, exist_ok=True)
        data = preset.model_dump(exclude_none=True)
        with path.open("x", encoding="utf-8") as out:
            json.dump(self.scrub(data), out, ensure_ascii=False, indent=2)
        return {"path": str(path), "preset": data}

    def _options(self, options: dict) -> dict:
        options = ImageOptions.model_validate(options).model_dump(exclude_none=True)
        width, height = map(int, options["size"].split("x"))
        if max(width, height) > 8192 or width * height > 50_000_000:
            raise ValueError("Output exceeds 8192px per side or 50 million pixels")
        refs = []
        for ref in options["reference_images"]:
            path = self.path(ref, exists=True)
            if path.stat().st_size > studio.LIMITS["reference_bytes"]:
                raise ValueError("Reference image exceeds 10 MiB")
            self.image_info(str(path))
            refs.append(str(path))
        options["reference_images"] = refs
        if refs and options["image_model"].startswith("grok-"):
            raise ValueError("Grok does not support reference images; use GPT Image or Gemini")
        if options["action"] == "edit" and not refs:
            raise ValueError("edit requires at least one reference image")
        return options

    def plan_generation(self, spec: GenerationSpec) -> dict:
        plan_id = uuid.uuid4().hex
        options = {k: getattr(spec, k) for k in ImageOptions.model_fields}
        topic = spec.topic or f"mcp-{plan_id[:12]}"
        topic_root = self.path(spec.topic_root or self.root)
        jobs = []
        if spec.mode == "batch":
            preset_path = self.path(spec.preset, exists=True)
            # Validate shape before the legacy loader, which uses SystemExit for bad input.
            preset_data = self.get_preset(str(preset_path))
            preset = PresetSpec.model_validate(preset_data)
            for k in ImageOptions.model_fields:
                if k not in spec.model_fields_set:
                    options[k] = getattr(preset, k)
            source = gen_batch.load_preset(preset_path)
            rng = random.Random(spec.seed)
            for i in range(spec.count):
                cat = gen_batch.weighted_category(source.categories, rng)
                jobs.append({"id": f"{i + 1:06d}_{ai_image._safe_topic_slug(cat.name)[:60]}",
                             "prompt": gen_batch.build_prompt(source, cat, rng)})
        elif spec.mode == "jobs":
            jobs = [j.model_dump(exclude_none=True) for j in spec.jobs]
        else:
            jobs = [{"id": f"image_{i + 1:03d}", "prompt": spec.prompt} for i in range(spec.count)]
        output_dir, metadata_dir = topic_dirs(topic_root, ai_image._safe_topic_slug(topic))
        output_dir = self.path(output_dir)
        self.path(metadata_dir)
        normalized, seen = [], set()
        for job in jobs:
            job_id = ai_image._safe_topic_slug(job["id"])
            if job_id.casefold() in seen:
                raise ValueError("Job IDs collide after filename normalization")
            if job_id.split(".")[0].upper() in {"CON", "PRN", "AUX", "NUL", *[f"COM{i}" for i in range(1, 10)], *[f"LPT{i}" for i in range(1, 10)]}:
                raise ValueError("Job ID is a reserved Windows filename")
            seen.add(job_id.casefold())
            opts = self._options({**options, **{k: v for k, v in job.items() if k in ImageOptions.model_fields}})
            prompt = studio.compose_prompt("\n".join(p.strip() for p in (spec.prompt_prefix, job["prompt"], spec.prompt_suffix) if p.strip()), spec.positive, spec.negative)
            if not prompt.strip():
                raise ValueError("Prompt cannot be blank")
            normalized.append({"id": job_id, "prompt": prompt, **opts})
        plan = {"plan_id": plan_id, "created_at": time.time(), "source_mode": spec.mode,
                "count": len(normalized), "output_dir": str(output_dir),
                "outputs": [str(self.path(output_dir / f"{j['id']}.png")) for j in normalized],
                "spec": {"mode": "jobs", "topic": ai_image._safe_topic_slug(topic), "topic_root": str(topic_root),
                         "concurrency": spec.concurrency, "job_timeout_sec": spec.job_timeout_sec, "jobs": normalized}}
        self._write(self.state / "plans" / f"{plan_id}.json", plan)
        return self.scrub(plan)

    def get_plan(self, plan_id: str) -> dict:
        return self._read_json(self._record_path("plans", plan_id))

    def get_job(self, job_id: str) -> dict:
        with self._lock:
            return self._read_json(self._record_path("jobs", job_id))

    def list_jobs(self, limit: int = 50) -> list[dict]:
        if not 1 <= limit <= 200:
            raise ValueError("limit must be 1..200")
        with self._lock:
            paths = sorted((self.state / "jobs").glob("*.json"), key=lambda p: p.stat().st_mtime, reverse=True)
            return [self._read_json(p) for p in paths[:limit]]

    def _update(self, job_id: str, **fields):
        with self._lock:
            rec = self.get_job(job_id)
            rec.update(fields, updated_at=time.time())
            self._write(self._record_path("jobs", job_id), rec)

    def _valid_output(self, path: str, size: str) -> bool:
        try:
            info = self.image_info(path)
            return f"{info['width']}x{info['height']}" == size
        except (ValueError, OSError):
            return False

    def start_generation(self, plan_id: str, *, resume: bool = False) -> dict:
        with self._lock:
            if self._closed:
                raise ValueError("Server is stopping")
            if any(t.is_alive() for t in self._threads.values()):
                raise ValueError("Another generation is active; poll or cancel it first")
            plan = self.get_plan(plan_id)
            output_dir = self.path(plan["output_dir"])
            topic_lock = FileLock(self.path(topic_dirs(plan["spec"]["topic_root"], plan["spec"]["topic"])[1]) / ".mcp.lock")
            try:
                # Revalidate references and output boundaries at execution time.
                for job, output in zip(plan["spec"]["jobs"], plan["outputs"], strict=True):
                    self._options({k: v for k, v in job.items() if k in ImageOptions.model_fields})
                    p = self.path(output)
                    if p.exists() and (not resume or not self._valid_output(output, job["size"])):
                        raise ValueError("Output already exists (or is invalid). Use resume_job for valid partial results, or a new topic")
                job_id = uuid.uuid4().hex
                rec = {"job_id": job_id, "plan_id": plan_id, "status": "queued", "created_at": time.time(),
                       "updated_at": time.time(), "count": plan["count"], "completed": 0,
                       "outputs": [], "events": [], "resume": resume, "error": None, "result": None}
                self._write(self.state / "jobs" / f"{job_id}.json", rec)
                thread = threading.Thread(target=self._run, args=(job_id, plan, resume, topic_lock), daemon=True)
                self._threads[job_id] = thread
                thread.start()
                return rec
            except BaseException:
                topic_lock.close()
                raise

    @staticmethod
    def _kill(proc: subprocess.Popen):
        if proc.poll() is not None:
            return
        if os.name == "nt":
            subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], capture_output=True,
                           creationflags=subprocess.CREATE_NO_WINDOW, timeout=15)
        else:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    def _run(self, job_id: str, plan: dict, resume: bool, topic_lock: FileLock):
        proc = None
        try:
            spec = copy.deepcopy(plan["spec"])
            spec["resume"] = resume
            env = {**os.environ, "PYTHONIOENCODING": "utf-8", "CLIPROXY_BASE_URL": self.base_url}
            if self.key:
                env["CLIPROXY_API_KEY"] = self.key
            with self._lock:
                if self.get_job(job_id)["status"] == "cancelling":
                    self._update(job_id, status="cancelled")
                    return
                proc = subprocess.Popen([sys.executable, str(ROOT / "tools" / "ai_image.py")],
                                        cwd=ROOT, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        text=True, encoding="utf-8", errors="replace",
                                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
                                        start_new_session=os.name != "nt")
                self._processes[job_id] = proc
                self._update(job_id, status="running")
            proc.stdin.write(json.dumps(spec, ensure_ascii=False))
            proc.stdin.close()
            stdout: list[str] = []
            def read_stdout():
                # Bounded protocol output; continue draining to avoid a pipe deadlock.
                total = 0
                while chunk := proc.stdout.read(65536):
                    total += len(chunk)
                    if total <= 4 * 1024 * 1024:
                        stdout.append(chunk)
            reader = threading.Thread(target=read_stdout, daemon=True)
            reader.start()
            for line in proc.stderr:
                try:
                    event = json.loads(line)
                    if not isinstance(event, dict):
                        continue
                except ValueError:
                    continue
                with self._lock:
                    rec = self.get_job(job_id)
                    events = (rec["events"] + [self.scrub(event)])[-30:]
                    completed = rec["completed"] + int(event.get("event") in {"job_finished", "job_skipped", "job_failed"})
                    self._update(job_id, events=events, completed=completed)
            proc.wait()
            reader.join()
            result = json.loads("".join(stdout)) if stdout else {"ok": False, "error": "Generator returned no JSON"}
            outputs = [p for j, p in zip(spec["jobs"], plan["outputs"], strict=True) if self._valid_output(p, j["size"])]
            with self._lock:
                cancelled = self.get_job(job_id)["status"] == "cancelling"
                success = proc.returncode == 0 and result.get("ok") and not result.get("failures") and len(outputs) == plan["count"]
                status = "cancelled" if cancelled else "succeeded" if success else "failed"
                error = None if success else result.get("error") or ("Generation cancelled" if cancelled else "Generation failed or output verification failed; inspect result/events")
                self._update(job_id, status=status, outputs=outputs, result=result, error=error)
        except Exception as exc:
            with self._lock:
                cancelled = self.get_job(job_id)["status"] == "cancelling"
                outputs = [p for j, p in zip(plan["spec"]["jobs"], plan["outputs"], strict=True) if self._valid_output(p, j["size"])]
                self._update(job_id, status="cancelled" if cancelled else "failed", error=self.scrub(str(exc)), outputs=outputs)
        finally:
            if proc:
                self._kill(proc)
                proc.wait()
                proc.stdout.close()
                proc.stderr.close()
            with self._lock:
                self._processes.pop(job_id, None)
            topic_lock.close()

    def cancel_job(self, job_id: str) -> dict:
        with self._lock:
            rec = self.get_job(job_id)
            if rec["status"] in TERMINAL:
                return rec
            self._update(job_id, status="cancelling")
            proc = self._processes.get(job_id)
        if proc:
            self._kill(proc)
        return self.get_job(job_id)

    def resume_job(self, job_id: str) -> dict:
        rec = self.get_job(job_id)
        if rec["status"] not in TERMINAL:
            raise ValueError("Wait until the job is terminal before resuming")
        return self.start_generation(rec["plan_id"], resume=True)

    def studio_history(self, limit: int = 50) -> list[dict]:
        if not 1 <= limit <= 200:
            raise ValueError("limit must be 1..200")
        paths = sorted((gallery_root(self.root) / "metadata").glob("*/job.json"), key=lambda p: p.stat().st_mtime, reverse=True)
        return [self.scrub(self._read_json(self.path(p, exists=True))) for p in paths[:limit]]

    def _save_image(self, im: Image.Image, path: Path) -> dict:
        path = self.path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as output:
            im.save(output, format="PNG")
        return self.image_info(str(path))

    def crop_image(self, path: str, regions: list[CropRegion], upscale: int = 1) -> dict:
        if not 1 <= len(regions) <= 100 or not 1 <= upscale <= 4:
            raise ValueError("Use 1..100 regions and upscale 1..4")
        topic = "crop-" + uuid.uuid4().hex[:12]
        directory, records = topic_dirs(self.root, topic)
        directory = self.path(directory)
        outputs = []
        with self._image(path) as im:
            names = [ai_image._safe_topic_slug(r.name).casefold() for r in regions]
            if len(set(names)) != len(names):
                raise ValueError("Region names must be unique after normalization")
            for region in regions:
                if region.x + region.width > im.width or region.y + region.height > im.height:
                    raise ValueError("Crop region extends outside the image")
                if max(region.width, region.height) * upscale > 8192 or region.width * region.height * upscale ** 2 > 50_000_000:
                    raise ValueError("Upscaled crop exceeds image limits")
            for index, region in enumerate(regions):
                cropped = im.crop((region.x, region.y, region.x + region.width, region.y + region.height))
                if upscale > 1:
                    cropped = cropped.resize((cropped.width * upscale, cropped.height * upscale), Image.Resampling.LANCZOS)
                outputs.append({"region": region.model_dump(), **self._save_image(cropped, directory / f"{index + 1:03d}_{ai_image._safe_topic_slug(region.name)}.png")})
                cropped.close()
        manifest = self.path(records / "crops.json")
        self._write(manifest, {"source": str(self.path(path)), "upscale": upscale, "outputs": outputs})
        return {"manifest": str(manifest), "outputs": outputs}

    def resize_image(self, path: str, width: int, height: int, fit: str = "contain") -> dict:
        if min(width, height) < 1 or max(width, height) > 8192 or width * height > 50_000_000:
            raise ValueError("Invalid output dimensions")
        with self._image(path) as source:
            if fit == "contain":
                result = ImageOps.pad(source.convert("RGBA"), (width, height), method=Image.Resampling.LANCZOS, color=(0, 0, 0, 0))
            elif fit == "cover":
                result = ImageOps.fit(source, (width, height), method=Image.Resampling.LANCZOS)
            elif fit == "stretch":
                result = source.resize((width, height), Image.Resampling.LANCZOS)
            else:
                raise ValueError("fit must be contain, cover, or stretch")
            try:
                topic = "resize-" + uuid.uuid4().hex[:12]
                output = topic_dirs(self.root, topic)[0] / "image.png"
                info = self._save_image(result, output)
                self._write(metadata_path(output, self.root, topic), {"source": str(self.path(path)), "fit": fit, **info})
                return info
            finally:
                result.close()

    def contact_sheet(self, paths: list[str], columns: int = 4, tile_size: int = 256) -> dict:
        if not 1 <= len(paths) <= 100 or not 1 <= columns <= 10 or not 64 <= tile_size <= 512:
            raise ValueError("Use 1..100 images, 1..10 columns, and tile_size 64..512")
        columns = min(columns, len(paths))
        rows = (len(paths) + columns - 1) // columns
        width, height = columns * tile_size, rows * (tile_size + 28)
        if max(width, height) > 8192 or width * height > 50_000_000:
            raise ValueError("Contact sheet exceeds image limits; increase columns or reduce tile_size")
        with Image.new("RGB", (width, height), "#20242b") as sheet:
            draw = ImageDraw.Draw(sheet)
            for i, path in enumerate(paths):
                with self._image(path) as im:
                    tile = ImageOps.contain(im.convert("RGB"), (tile_size, tile_size))
                    x, y = (i % columns) * tile_size, (i // columns) * (tile_size + 28)
                    sheet.paste(tile, (x + (tile_size - tile.width) // 2, y + (tile_size - tile.height) // 2))
                    draw.text((x + 8, y + tile_size + 5), str(i + 1), fill="white")
            topic = "contact-sheet-" + uuid.uuid4().hex[:12]
            output = topic_dirs(self.root, topic)[0] / "contact-sheet.png"
            info = self._save_image(sheet, output)
        self._write(metadata_path(output, self.root, topic), {"sources": paths, **info})
        return {**info, "images": [{"index": i + 1, "path": str(self.path(p))} for i, p in enumerate(paths)]}

    def close(self):
        with self._lock:
            self._closed = True
            active = [job_id for job_id, thread in self._threads.items() if thread.is_alive()]
        try:
            for job_id in active:
                self.cancel_job(job_id)
            for job_id in active:
                self._threads[job_id].join(timeout=20)
        finally:
            self._owner.close()
