"""AI-friendly image generation entry point. JSON in -> JSON out.

Designed so an LLM can call one command, parse one JSON line, and know
whether each requested image was produced and where.

Usage:
  # JSON via stdin
  echo '{"prompt":"...","topic":"x"}' | python tools/ai_image.py

  # JSON via file
  python tools/ai_image.py --spec spec.json

  # JSON inline
  python tools/ai_image.py --json '{"prompt":"...","topic":"x"}'

  # Self-describe
  python tools/ai_image.py --schema
  python tools/ai_image.py --example

Spec keys (all in one flat object):
  mode              "single" (default) | "jobs" | "batch"
  prompt            string  (required when mode=single)
  topic             string  (default "ai-image-request")
  count             int     (default 1)
  size              "WIDTHxHEIGHT", positive integers (default "1920x1080")
  quality           "low" | "medium" | "high"  (default "high")
  model             main Responses model for gpt-image backends (default from environment)
  image_model       image provider model (default gpt-image-2)
  reference_images  [string] paths (default [])
  action            "auto" | "generate" | "edit" (default "generate"; GPT reference edits should use "edit")
  topic_root        project output root (default project root)
  preset            path to preset JSON  (required when mode=batch)
  jobs              [object]  (required when mode=jobs); each item:
                      { id, prompt, reference_images?, size?, quality?, action? }
                      output -> output/<topic_slug>/<id>.png
  prompt_prefix     string prepended to every job's prompt (mode=jobs)
  prompt_suffix     string appended to every job's prompt (mode=jobs)
  concurrency       int (default 4, max useful ~8; applies to single count, jobs, and batch)
  resume            bool (default false)
  dry_run           bool (default false)
  job_timeout_sec   per-image wall-clock timeout seconds (default CLIPROXY_TIMEOUT or 600; <=0 disables)
  seed              int  (optional)

Output (one JSON line on stdout):
  {
    "ok":          bool,
    "mode":        "single" | "batch",
    "outputs":     [absolute paths of produced PNGs],
    "failures":    [{"index":N, "reason":"..."}],
    "topic_dir":   absolute path (single: topic folder, batch: output topic folder),
    "output_dir":  absolute output directory,
    "elapsed_sec": float,
    "error":       "..."   // only when ok=false at top level
  }

Exit codes: 0=ok, 2=spec ran but had failures, 1=fatal (bad spec / invocation).
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import subprocess
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GEN_IMAGE = ROOT / "scripts" / "gen_image.py"
GEN_BATCH = ROOT / "scripts" / "gen_batch.py"
DEFAULT_TOPIC = "image-request"


SCHEMA = {
    "mode": {"type": "string", "enum": ["single", "jobs", "batch"], "default": "single"},
    "prompt": {"type": "string", "required_when": "mode=single"},
    "topic": {"type": "string", "default": "ai-image-request"},
    "count": {"type": "integer", "default": 1, "min": 1},
    "size": {"type": "string", "format": "WIDTHxHEIGHT (positive integers)", "default": "1920x1080"},
    "quality": {"type": "string", "enum": ["low", "medium", "high"], "default": "high"},
    "model": {"type": "string", "description": "main Responses model used by gpt-image backends"},
    "image_model": {"type": "string", "default": "gpt-image-2", "description": "gpt-image-*, gemini-*-image, or grok-imagine-*"},
    "action": {"type": "string", "enum": ["auto", "generate", "edit"], "default": "generate", "description": "image tool action; use edit for GPT reference-image restyling"},
    "reference_images": {"type": "array<string>", "description": "image paths", "default": []},
    "topic_root": {"type": "string", "description": "project output root", "default": str(ROOT)},
    "preset": {"type": "string", "required_when": "mode=batch", "description": "path to preset JSON"},
    "jobs": {
        "type": "array<object>",
        "required_when": "mode=jobs",
        "description": "per-image specs; each item: {id, prompt, reference_images?, size?, quality?, action?}",
    },
    "prompt_prefix": {"type": "string", "description": "shared preamble prepended to each job's prompt (mode=jobs)"},
    "prompt_suffix": {"type": "string", "description": "shared suffix appended to each job's prompt (mode=jobs)"},
    "concurrency": {"type": "integer", "default": 4},
    "resume": {"type": "boolean", "default": False},
    "dry_run": {"type": "boolean", "default": False},
    "seed": {"type": "integer", "optional": True},
    "job_timeout_sec": {"type": "number", "default": 600, "description": "per-image wall-clock timeout seconds; <=0 disables"},
}

EXAMPLES = {
    "single_minimal": {
        "prompt": "고양이가 우주를 유영하는 시네마틱 사진",
        "topic": "cat-in-space",
    },
    "single_full": {
        "mode": "single",
        "prompt": "wizard casting fireball in a torch-lit dungeon",
        "topic": "wizard-test",
        "count": 3,
        "size": "1920x1088",
        "quality": "high",
        "action": "edit",
        "reference_images": ["E:/ai-image-generator/output/extraction-rpg-dad/ui_combat_hud/extraction-rpg-dad_000001.png"],
    },
    "batch_resume": {
        "mode": "batch",
        "preset": "presets/extraction-rpg.json",
        "count": 50,
        "concurrency": 8,
        "resume": True,
    },
    "jobs_with_per_image_refs": {
        "mode": "jobs",
        "topic": "airport-topview-eagle-batch10",
        "size": "1920x1088",
        "quality": "high",
        "concurrency": 4,
        "prompt_prefix": "Strict vertical orthographic top-down view of the EXACT physical airport space shown in the reference photo.",
        "prompt_suffix": "Constraints: no invented columns, no extra walls, no unrelated furniture.",
        "jobs": [
            {
                "id": "01_checkin_hall",
                "prompt": "wide departure check-in hall ...",
                "reference_images": ["D:/Eagle/CityAI.library/images/.../photo.jpg"],
            },
            {
                "id": "02_info_island",
                "prompt": "information desk island with circular help counter ...",
                "reference_images": ["D:/Eagle/CityAI.library/images/.../photo.jpg"],
            },
        ],
    },
}


def _safe_topic_slug(value: str) -> str:
    raw = value.strip()
    raw = re.sub(r"[^\w\s.-]+", "_", raw, flags=re.UNICODE)
    raw = re.sub(r"\s+", "_", raw, flags=re.UNICODE).strip("._- ")
    return (raw[:80] or DEFAULT_TOPIC)


def _resolve_under_root(value: str | Path | None, default: Path = ROOT) -> Path:
    path = Path(value) if value else default
    if path.is_absolute():
        return path
    return (ROOT / path).resolve()


def _emit(obj: dict, exit_code: int) -> int:
    json.dump(obj, sys.stdout, ensure_ascii=False)
    sys.stdout.write("\n")
    sys.stdout.flush()
    return exit_code


_PROGRESS_LOCK = threading.Lock()


def _emit_progress(event: dict) -> None:
    """Write machine-readable progress to stderr without corrupting stdout JSON."""
    event.setdefault("ts", round(time.time(), 3))
    with _PROGRESS_LOCK:
        json.dump(event, sys.stderr, ensure_ascii=False)
        sys.stderr.write("\n")
        sys.stderr.flush()


def _resolve_job_timeout(spec: dict) -> float | None:
    raw = spec.get("job_timeout_sec", spec.get("timeout_sec", os.environ.get("CLIPROXY_TIMEOUT", "600")))
    if raw is None or raw == "":
        return None
    timeout = float(raw)
    return timeout if timeout > 0 else None


def _validate_action(action: str) -> str | None:
    if action not in ("auto", "generate", "edit"):
        return f"invalid action {action!r}; expected auto, generate, or edit"
    return None


def _run_child(cmd: list[str], timeout_sec: float | None) -> tuple[int, str]:
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            encoding="utf-8",
            cwd=str(ROOT),
            timeout=timeout_sec,
        )
    except subprocess.TimeoutExpired as exc:
        tail = "\n".join(p for p in (exc.stderr, exc.stdout) if isinstance(p, str))
        last = tail.strip().splitlines()[-1] if tail.strip() else ""
        suffix = f": {last[:220]}" if last else ""
        return 124, f"timed out after {timeout_sec:g}s{suffix}"
    tail = (proc.stderr or proc.stdout or "").strip().splitlines()
    last = tail[-1] if tail else ""
    return proc.returncode, last


def _validate_size(size: str) -> str | None:
    try:
        w_str, h_str = size.lower().split("x")
        w, h = int(w_str), int(h_str)
    except Exception:
        return f"invalid size {size!r}; expected WIDTHxHEIGHT"
    if w <= 0 or h <= 0:
        return f"size {size}: width and height must be positive"
    return None


def run_single(spec: dict) -> dict:
    prompt = (spec.get("prompt") or "").strip()
    if not prompt:
        return {"ok": False, "error": "spec.prompt is required when mode=single"}

    topic = str(spec.get("topic") or "ai-image-request")
    topic_slug = _safe_topic_slug(topic)
    count = max(1, int(spec.get("count", 1)))
    size = spec.get("size", "1920x1080")
    quality = spec.get("quality", "high")
    model = spec.get("model")
    image_model = spec.get("image_model")
    action = str(spec.get("action") or "generate")
    concurrency = max(1, int(spec.get("concurrency", 4)))
    resume = bool(spec.get("resume"))
    refs = [_resolve_under_root(r) for r in list(spec.get("reference_images") or [])]
    topic_root = _resolve_under_root(spec.get("topic_root"))
    output_dir = topic_root / "output" / topic_slug
    try:
        timeout_sec = _resolve_job_timeout(spec)
    except (TypeError, ValueError):
        return {"ok": False, "error": "job_timeout_sec must be a number"}

    err = _validate_size(size)
    if err:
        return {"ok": False, "error": err}
    if quality not in ("low", "medium", "high"):
        return {"ok": False, "error": f"invalid quality {quality!r}"}
    err = _validate_action(action)
    if err:
        return {"ok": False, "error": err}
    for r in refs:
        if not r.is_file():
            return {"ok": False, "error": f"reference image not found: {r}"}
    output_dir.mkdir(parents=True, exist_ok=True)

    started = time.time()
    outputs: list[str] = []
    failures: list[dict] = []

    def _one(i: int) -> tuple[int, int, str, str]:
        # Use topic_slug (not raw topic) so filenames cannot contain path separators.
        # Previously this used raw topic, which on Windows caused the file to land
        # outside output/<topic_slug>/ when topic contained "/".
        name = f"{topic_slug}.png" if count == 1 else f"{topic_slug}_{i:03d}.png"
        out_path = str((output_dir / name).resolve())
        if resume and Path(out_path).is_file():
            _emit_progress({"event": "image_skipped", "mode": "single", "index": i, "output": out_path})
            return i, 0, out_path, ""
        _emit_progress({"event": "image_started", "mode": "single", "index": i, "output": out_path})
        image_started = time.time()
        cmd = [
            sys.executable, str(GEN_IMAGE), prompt,
            "--topic", topic, "--topic-root", str(topic_root),
            "-o", name, "--size", size, "--quality", quality,
        ]
        cmd += ["--action", action]
        if timeout_sec is not None:
            cmd += ["--timeout", f"{timeout_sec:g}"]
        if model:
            cmd += ["--model", str(model)]
        if image_model:
            cmd += ["--image-model", str(image_model)]
        for r in refs:
            cmd += ["--reference-image", str(r)]
        rc, last = _run_child(cmd, timeout_sec)
        event = "image_finished" if rc == 0 else "image_failed"
        _emit_progress({"event": event, "mode": "single", "index": i, "output": out_path, "elapsed_sec": round(time.time() - image_started, 2), "reason": last[:300] if rc else ""})
        return i, rc, out_path, last

    workers = min(concurrency, count)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        futs = [pool.submit(_one, i) for i in range(1, count + 1)]
        for f in as_completed(futs):
            i, rc, path, last = f.result()
            if rc == 0:
                outputs.append(path)
            else:
                failures.append({"index": i, "reason": last[:300] or f"rc={rc}"})

    return {
        "ok": len(failures) == 0,
        "mode": "single",
        "outputs": sorted(outputs),
        "failures": sorted(failures, key=lambda x: x["index"]),
        "topic_dir": str(output_dir.resolve()),
        "output_dir": str(output_dir.resolve()),
        "elapsed_sec": round(time.time() - started, 2),
    }


def run_jobs(spec: dict) -> dict:
    """Run a list of independent image jobs in parallel.

    Each item in spec.jobs is a per-image spec:
      {
        "id": "01_xxx",                # required: unique id (used as filename)
        "prompt": "scene description",  # required (combined with prompt_prefix/suffix)
        "reference_images": [...],      # optional; falls back to spec.reference_images
        "size": "...", "quality": "...", "action": "edit"  # optional; fall back to spec values
      }

    Spec-level keys:
      topic              parent topic; output goes under output/<topic_slug>/<id>.png
      prompt_prefix      string prepended to every job's prompt (shared preamble)
      prompt_suffix      string appended to every job's prompt
      reference_images   default refs if a job omits its own
      size, quality      defaults (size 1920x1080, quality high)
      concurrency        parallel workers (default 4)
      job_timeout_sec    per-image wall-clock timeout; <=0 disables
    """
    parent_topic = str(spec.get("topic") or "ai-image-jobs")
    parent_slug = _safe_topic_slug(parent_topic)
    default_size = spec.get("size", "1920x1080")
    default_quality = spec.get("quality", "high")
    default_refs = list(spec.get("reference_images") or [])
    default_model = spec.get("model")
    default_image_model = spec.get("image_model")
    default_action = str(spec.get("action") or "generate")
    dry_run = bool(spec.get("dry_run"))
    resume = bool(spec.get("resume"))
    prefix = (spec.get("prompt_prefix") or "").strip()
    suffix = (spec.get("prompt_suffix") or "").strip()
    topic_root = _resolve_under_root(spec.get("topic_root"))
    output_dir = topic_root / "output" / parent_slug
    concurrency = max(1, int(spec.get("concurrency", 4)))
    try:
        timeout_sec = _resolve_job_timeout(spec)
    except (TypeError, ValueError):
        return {"ok": False, "error": "job_timeout_sec must be a number"}
    jobs = spec.get("jobs") or []
    if not isinstance(jobs, list) or not jobs:
        return {"ok": False, "error": "spec.jobs must be a non-empty array"}

    for idx, j in enumerate(jobs):
        if not isinstance(j, dict):
            return {"ok": False, "error": f"jobs[{idx}] must be an object"}
        body = (j.get("prompt") or "").strip()
        if not body and not (prefix or suffix):
            return {"ok": False, "error": f"jobs[{idx}].prompt is required"}
    if not dry_run:
        output_dir.mkdir(parents=True, exist_ok=True)

    started = time.time()
    outputs: list[dict] = []
    failures: list[dict] = []

    def _one(idx: int, job: dict) -> tuple[int, dict]:
        job_id = str(job.get("id") or f"job_{idx + 1:03d}")
        job_slug = _safe_topic_slug(job_id)
        size = job.get("size") or default_size
        quality = job.get("quality") or default_quality
        refs_raw = job.get("reference_images")
        refs_paths = list(refs_raw) if refs_raw is not None else default_refs
        model = job.get("model") or default_model
        image_model = job.get("image_model") or default_image_model
        action = str(job.get("action") or default_action)
        refs = [_resolve_under_root(r) for r in refs_paths]
        body = (job.get("prompt") or "").strip()
        prompt_parts = [p for p in (prefix, body, suffix) if p]
        prompt = "\n".join(prompt_parts)

        err = _validate_size(size)
        if err:
            return idx, {"ok": False, "id": job_id, "error": err}
        if quality not in ("low", "medium", "high"):
            return idx, {"ok": False, "id": job_id, "error": f"invalid quality {quality!r}"}
        err = _validate_action(action)
        if err:
            return idx, {"ok": False, "id": job_id, "error": err}
        for r in refs:
            if not r.is_file():
                return idx, {"ok": False, "id": job_id, "error": f"reference image not found: {r}"}

        name = f"{job_slug}.png"
        out_path = str((output_dir / name).resolve())
        if dry_run:
            return idx, {"ok": True, "id": job_id, "output": out_path, "dry_run": True}
        if resume and Path(out_path).is_file():
            _emit_progress({"event": "job_skipped", "mode": "jobs", "id": job_id, "index": idx, "output": out_path})
            return idx, {"ok": True, "id": job_id, "output": out_path, "skipped": True}

        _emit_progress({"event": "job_started", "mode": "jobs", "id": job_id, "index": idx, "output": out_path})
        job_started = time.time()
        cmd = [
            sys.executable, str(GEN_IMAGE), prompt,
            "--topic", parent_topic, "--topic-root", str(topic_root),
            "-o", name, "--size", size, "--quality", quality,
        ]
        cmd += ["--action", action]
        if timeout_sec is not None:
            cmd += ["--timeout", f"{timeout_sec:g}"]
        for r in refs:
            cmd += ["--reference-image", str(r)]
        if model:
            cmd += ["--model", str(model)]
        if image_model:
            cmd += ["--image-model", str(image_model)]
        rc, last = _run_child(cmd, timeout_sec)
        if rc == 0:
            _emit_progress({"event": "job_finished", "mode": "jobs", "id": job_id, "index": idx, "output": out_path, "elapsed_sec": round(time.time() - job_started, 2)})
            return idx, {"ok": True, "id": job_id, "output": out_path}
        error = last[:300] or f"rc={rc}"
        _emit_progress({"event": "job_failed", "mode": "jobs", "id": job_id, "index": idx, "output": out_path, "elapsed_sec": round(time.time() - job_started, 2), "reason": error})
        return idx, {"ok": False, "id": job_id, "error": error}

    with ThreadPoolExecutor(max_workers=concurrency) as pool:
        futs = [pool.submit(_one, i, j) for i, j in enumerate(jobs)]
        for f in as_completed(futs):
            i, r = f.result()
            if r.get("ok"):
                outputs.append({"id": r["id"], "output": r["output"]})
            else:
                failures.append({"id": r.get("id"), "index": i, "reason": r.get("error", "unknown")})

    return {
        "ok": len(failures) == 0,
        "mode": "jobs",
        "dry_run": dry_run,
        "topic_dir": str(output_dir.resolve()),
        "output_dir": str(output_dir.resolve()),
        "outputs": [] if dry_run else sorted(outputs, key=lambda x: x["id"]),
        "planned_outputs": sorted(outputs, key=lambda x: x["id"]) if dry_run else [],
        "failures": sorted(failures, key=lambda x: (x.get("id") or "")),
        "elapsed_sec": round(time.time() - started, 2),
    }


def run_batch(spec: dict) -> dict:
    preset = spec.get("preset")
    if not preset:
        return {"ok": False, "error": "spec.preset is required when mode=batch"}
    preset_path = Path(preset)
    if not preset_path.is_absolute():
        preset_path = (ROOT / preset_path).resolve()
    if not preset_path.is_file():
        return {"ok": False, "error": f"preset not found: {preset_path}"}

    count = int(spec.get("count", 10))
    conc = int(spec.get("concurrency", 4))

    topic_root = _resolve_under_root(spec.get("topic_root"))

    cmd = [sys.executable, str(GEN_BATCH), str(preset_path),
           "--count", str(count), "--concurrency", str(conc),
           "--topic-root", str(topic_root)]
    if spec.get("resume"): cmd.append("--resume")
    if spec.get("dry_run"): cmd.append("--dry-run")
    if spec.get("topic"): cmd += ["--topic", str(spec["topic"])]
    if spec.get("seed") is not None: cmd += ["--seed", str(spec["seed"])]

    started = time.time()
    proc = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", cwd=str(ROOT))

    topic = spec.get("topic")
    if not topic:
        try:
            topic = json.loads(preset_path.read_text(encoding="utf-8")).get("topic")
        except Exception:
            topic = None
    topic_slug = _safe_topic_slug(topic) if topic else None
    output_dir = (topic_root / "output" / topic_slug).resolve() if topic_slug else None
    run_root = (topic_root / "runs" / topic_slug).resolve() if topic_slug else None

    return {
        "ok": proc.returncode == 0,
        "mode": "batch",
        "topic_dir": str(output_dir) if output_dir else None,
        "output_dir": str(output_dir) if output_dir else None,
        "run_root": str(run_root) if run_root else None,
        "stdout_tail": (proc.stdout or "").splitlines()[-20:],
        "stderr_tail": (proc.stderr or "").splitlines()[-20:],
        "elapsed_sec": round(time.time() - started, 2),
    }


def main() -> int:
    ap = argparse.ArgumentParser(
        description="AI-friendly image generator. JSON in, JSON out.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--spec", help="path to JSON spec file")
    g.add_argument("--json", dest="inline_json", help="inline JSON spec string")
    g.add_argument("--schema", action="store_true", help="print spec schema and exit")
    g.add_argument("--example", action="store_true", help="print example specs and exit")
    args = ap.parse_args()

    if args.schema:
        print(json.dumps(SCHEMA, indent=2, ensure_ascii=False))
        return 0
    if args.example:
        print(json.dumps(EXAMPLES, indent=2, ensure_ascii=False))
        return 0

    try:
        if args.spec:
            spec = json.loads(Path(args.spec).read_text(encoding="utf-8"))
        elif args.inline_json:
            spec = json.loads(args.inline_json)
        else:
            raw = sys.stdin.read().strip()
            if not raw:
                return _emit({"ok": False, "error": "no spec on stdin and neither --spec nor --json given"}, 1)
            spec = json.loads(raw)
    except json.JSONDecodeError as e:
        return _emit({"ok": False, "error": f"invalid JSON spec: {e}"}, 1)
    except OSError as e:
        return _emit({"ok": False, "error": f"cannot read spec: {e}"}, 1)

    if not isinstance(spec, dict):
        return _emit({"ok": False, "error": "spec must be a JSON object"}, 1)

    mode = spec.get("mode", "single")
    if mode == "single":
        result = run_single(spec)
    elif mode == "batch":
        result = run_batch(spec)
    elif mode == "jobs":
        result = run_jobs(spec)
    else:
        return _emit({"ok": False, "error": f"unknown mode {mode!r}; use 'single', 'jobs' or 'batch'"}, 1)

    return _emit(result, 0 if result.get("ok") else 2)


if __name__ == "__main__":
    sys.exit(main())
