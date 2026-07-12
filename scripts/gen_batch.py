#!/usr/bin/env python3
"""Generic batch image generator driven by a JSON preset.

This is the reusable CLI. To create a new dataset you write a small JSON
preset describing categories and prompt templates; you do not write a new
Python script. See `presets/extraction-rpg.json` for an example.

Preset shape (all fields optional unless noted):

{
  "topic": "extraction-rpg",                # output folder name (required)
  "size": "1920x1080",                      # exact output size
  "quality": "high",                        # low|medium|high
  "model": "gpt-5.5",                       # CLIProxyAPI main model
  "image_model": "gpt-image-2",
  "global_style": "concept art, ...",       # appended to every prompt
  "negative": "no real brand logos, ...",   # appended as constraints line
  "categories": [                           # required, length >= 1
    {
      "name": "barbarian",                  # folder name
      "weight": 3,                          # relative sampling weight (default 1)
      "templates": [                        # one chosen randomly per image
        "Barbarian wielding a great-axe ...",
        "Barbarian mid-charge ..."
      ]
    },
    ...
  ]
}

Per image the script picks a category by weight, picks one template at
random, then concatenates: template + ". " + global_style + " Constraints: "
+ negative.

Usage:

  python3 scripts/gen_batch.py presets/extraction-rpg.json --count 100 --concurrency 8
  python3 scripts/gen_batch.py presets/extraction-rpg.json --count 5 --dry-run
  python3 scripts/gen_batch.py presets/extraction-rpg.json --count 100 --resume
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import random
import re
import subprocess
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_GEN_SCRIPT = str(Path(__file__).resolve().with_name("gen_image.py"))


@dataclass(frozen=True)
class Category:
    name: str
    weight: int
    templates: list[str]


@dataclass(frozen=True)
class Preset:
    topic: str
    size: str
    quality: str
    model: str
    image_model: str
    global_style: str
    negative: str
    categories: list[Category]


def safe_slug(value: str) -> str:
    raw = re.sub(r"[^\w\s.-]+", "_", value.strip(), flags=re.UNICODE)
    raw = re.sub(r"\s+", "_", raw, flags=re.UNICODE).strip("._- ")
    return raw[:80] or "preset"


def load_preset(path: Path) -> Preset:
    data = json.loads(path.read_text(encoding="utf-8"))
    raw_cats = data.get("categories") or []
    if not raw_cats:
        raise SystemExit(f"Preset {path} has no categories.")
    categories = [
        Category(
            name=str(c["name"]),
            weight=max(1, int(c.get("weight", 1))),
            templates=[str(t) for t in c.get("templates") or []],
        )
        for c in raw_cats
    ]
    for cat in categories:
        if not cat.templates:
            raise SystemExit(f"Category '{cat.name}' has no templates.")
    return Preset(
        topic=str(data.get("topic") or path.stem),
        size=str(data.get("size", "1920x1080")),
        quality=str(data.get("quality", "high")),
        model=str(data.get("model", os.environ.get("CLIPROXY_MAIN_MODEL", "gpt-5.5"))),
        image_model=str(data.get("image_model", "gpt-image-2")),
        global_style=str(data.get("global_style", "")),
        negative=str(data.get("negative", "")),
        categories=categories,
    )


def weighted_category(categories: list[Category], rng: random.Random) -> Category:
    total = sum(c.weight for c in categories)
    pick = rng.randrange(total)
    cursor = 0
    for cat in categories:
        cursor += cat.weight
        if pick < cursor:
            return cat
    return categories[-1]


def build_prompt(preset: Preset, cat: Category, rng: random.Random) -> str:
    base = rng.choice(cat.templates)
    parts = [base.strip()]
    if preset.global_style.strip():
        parts.append(preset.global_style.strip())
    if preset.negative.strip():
        parts.append("Constraints: " + preset.negative.strip())
    return " ".join(parts)


def prompt_hash(prompt: str) -> str:
    return hashlib.sha256(prompt.encode("utf-8")).hexdigest()[:16]


def append_jsonl(path: Path, row: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as f:
        _ = f.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")


def run_gen_image(
    gen_script: Path,
    prompt: str,
    output_path: Path,
    events_path: Path,
    preset: Preset,
    topic: str,
    timeout: int,
) -> subprocess.CompletedProcess[str]:
    cmd = [
        sys.executable,
        str(gen_script),
        prompt,
        "-o", str(output_path),
        "--topic", topic,
        "--model", preset.model,
        "--image-model", preset.image_model,
        "--size", preset.size,
        "--quality", preset.quality,
        "--action", "generate",
        "--events", str(events_path),
    ]
    return subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a dataset of images from a JSON preset using CLIProxyAPI."
    )
    _ = parser.add_argument("preset", type=Path, help="Path to preset JSON file")
    _ = parser.add_argument("--count", type=int, default=10, help="Number of images")
    _ = parser.add_argument("--start-index", type=int, default=1)
    _ = parser.add_argument("--concurrency", type=int, default=1, help="Parallel workers")
    _ = parser.add_argument("--seed", type=int, default=42)
    _ = parser.add_argument("--topic", default=None, help="Override preset topic (output folder)")
    _ = parser.add_argument("--topic-root", default=str(ROOT), help="Parent folder for output/ and runs/ dirs")
    _ = parser.add_argument("--resume", action="store_true", help="Skip files that already exist")
    _ = parser.add_argument("--dry-run", action="store_true", help="Plan only, no API calls")
    _ = parser.add_argument("--max-failures", type=int, default=20)
    _ = parser.add_argument("--per-image-timeout", type=int, default=600, help="Subprocess timeout seconds")
    _ = parser.add_argument("--gen-script", default=DEFAULT_GEN_SCRIPT)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.preset.is_file():
        _ = sys.stderr.write(f"Preset not found: {args.preset}\n")
        return 2
    preset = load_preset(args.preset)
    topic = (args.topic or preset.topic).strip() or preset.topic
    topic_slug = safe_slug(topic)
    root = Path(args.topic_root).expanduser()
    if not root.is_absolute():
        root = (Path.cwd() / root).resolve()
    out_dir = root / "output" / topic_slug
    run_dir = root / "runs" / topic_slug / dt.datetime.now().strftime("%Y%m%d_%H%M%S")
    events_dir = run_dir / "events"
    metadata_path = run_dir / "metadata.jsonl"
    failures_path = run_dir / "failures.jsonl"
    prompts_path = run_dir / "prompts.jsonl"
    gen_script = Path(args.gen_script)
    if not gen_script.is_file():
        _ = sys.stderr.write(f"gen_image.py not found: {gen_script}\n")
        return 2

    out_dir.mkdir(parents=True, exist_ok=True)
    events_dir.mkdir(parents=True, exist_ok=True)

    print(f"Preset: {args.preset}")
    print(f"Topic: {topic} ({topic_slug})")
    print(f"Output: {out_dir}")
    print(f"Run: {run_dir}")
    print(f"Count: {args.count}  Concurrency: {args.concurrency}  Quality: {preset.quality}")
    print(f"Categories: {[(c.name, c.weight) for c in preset.categories]}")

    write_lock = threading.Lock()
    counters = {"completed": 0, "failures": 0}
    stop_event = threading.Event()

    def process(offset: int) -> None:
        if stop_event.is_set():
            return
        index = args.start_index + offset
        rng = random.Random(f"{args.seed}:{topic_slug}:{index}")
        cat = weighted_category(preset.categories, rng)
        prompt = build_prompt(preset, cat, rng)
        image_id = f"{topic_slug}_{index:06d}"
        output_path = out_dir / safe_slug(cat.name) / f"{image_id}.png"
        events_path = events_dir / f"{image_id}.sse"

        plan = {
            "id": image_id,
            "index": index,
            "category": cat.name,
            "topic": topic,
            "topic_slug": topic_slug,
            "prompt_hash": prompt_hash(prompt),
            "prompt": prompt,
            "output_path": str(output_path),
            "size": preset.size,
            "quality": preset.quality,
            "model": preset.model,
            "image_model": preset.image_model,
        }
        with write_lock:
            append_jsonl(prompts_path, plan)

        if args.resume and output_path.is_file():
            print(f"[skip] {image_id} ({cat.name})")
            return
        if args.dry_run:
            print(f"[plan] {image_id} ({cat.name})")
            return

        print(f"[generate] {image_id} ({cat.name})")
        output_path.parent.mkdir(parents=True, exist_ok=True)
        started = dt.datetime.now(dt.timezone.utc).isoformat()
        try:
            proc = run_gen_image(gen_script, prompt, output_path, events_path, preset, topic, args.per_image_timeout)
        except subprocess.TimeoutExpired:
            with write_lock:
                counters["failures"] += 1
                append_jsonl(failures_path, {**plan, "status": "timeout", "started_at": started})
                stop_now = counters["failures"] >= args.max_failures
            print(f"[failed] {image_id}: timeout", file=sys.stderr)
            if stop_now:
                stop_event.set()
            return
        finished = dt.datetime.now(dt.timezone.utc).isoformat()

        if proc.returncode == 0 and output_path.is_file():
            with write_lock:
                counters["completed"] += 1
                append_jsonl(metadata_path, {
                    **plan, "status": "ok",
                    "started_at": started, "finished_at": finished,
                    "stdout": proc.stdout.strip(),
                })
            print(f"[ok] {image_id} -> {output_path}")
        else:
            with write_lock:
                counters["failures"] += 1
                append_jsonl(failures_path, {
                    **plan, "status": "failed",
                    "started_at": started, "finished_at": finished,
                    "returncode": proc.returncode,
                    "stdout": proc.stdout[-2000:],
                    "stderr": proc.stderr[-2000:],
                })
                stop_now = counters["failures"] >= args.max_failures
            print(f"[failed] {image_id}: {proc.stderr[-500:]}", file=sys.stderr)
            if stop_now:
                stop_event.set()

    if args.concurrency <= 1:
        for offset in range(args.count):
            if stop_event.is_set():
                break
            process(offset)
    else:
        with ThreadPoolExecutor(max_workers=args.concurrency) as pool:
            futs = [pool.submit(process, offset) for offset in range(args.count)]
            for fut in as_completed(futs):
                exc = fut.exception()
                if exc is not None:
                    print(f"[worker-error] {exc}", file=sys.stderr)

    print(f"\nCompleted: {counters['completed']}, failures: {counters['failures']}")
    print(f"Run dir: {run_dir}")
    return 0 if counters["failures"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
