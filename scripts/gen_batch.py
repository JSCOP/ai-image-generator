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
  "image_model": "gpt-image-2.5",
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
  python3 scripts/gen_batch.py presets/extraction-rpg.json --image-model grok-imagine-image-2.0 --dry-run
"""
from __future__ import annotations

import argparse
import json
import os
import random
import re
import subprocess
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, replace
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gallery import topic_dirs
from gen_image import DEFAULT_IMAGE_MODEL


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
        image_model=str(data.get("image_model") or os.environ.get("CLIPROXY_IMAGE_MODEL") or DEFAULT_IMAGE_MODEL),
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


def run_gen_image(gen_script: Path, prompt: str, output_path: Path,
                  preset: Preset, topic: str, workspace: Path, timeout: int):
    return subprocess.run([
        sys.executable, str(gen_script), prompt, "-o", str(output_path),
        "--topic", topic, "--topic-root", str(workspace),
        "--model", preset.model, "--image-model", preset.image_model,
        "--size", preset.size, "--quality", preset.quality, "--action", "generate",
    ], text=True, encoding="utf-8", capture_output=True, timeout=timeout)


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
    _ = parser.add_argument("--image-model", default=None, help="Override the preset image model")
    _ = parser.add_argument("--topic-root", default=str(Path.cwd()), help="Workspace or ImageGallery directory")
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
    if args.image_model:
        preset = replace(preset, image_model=args.image_model)
    topic = (args.topic or preset.topic).strip() or preset.topic
    topic_slug = safe_slug(topic)
    workspace = Path(args.topic_root).expanduser().resolve()
    output_dir, metadata_dir = topic_dirs(workspace, topic_slug)
    gen_script = Path(args.gen_script)
    if not gen_script.is_file() or args.count < 1:
        print("Invalid generator or count", file=sys.stderr)
        return 2
    plans = []
    for offset in range(args.count):
        index = args.start_index + offset
        rng = random.Random(f"{args.seed}:{topic_slug}:{index}")
        category = weighted_category(preset.categories, rng)
        plans.append({"id": f"{topic_slug}_{index:06d}",
                      "prompt": build_prompt(preset, category, rng),
                      "category": category.name, "image_model": preset.image_model,
                      "output": str(output_dir / f"{topic_slug}_{index:06d}.png")})
    if args.dry_run:
        print(json.dumps({"ok": True, "dry_run": True, "outputs": [], "failures": [],
                          "output_dir": str(output_dir), "metadata_dir": str(metadata_dir),
                          "planned_outputs": plans}, ensure_ascii=False))
        return 0
    outputs, failures = [], []
    lock = threading.Lock()
    stop = threading.Event()

    def process(plan):
        if stop.is_set():
            return
        path = Path(plan["output"])
        if args.resume and path.is_file():
            with lock:
                outputs.append(str(path))
            return
        try:
            proc = run_gen_image(gen_script, plan["prompt"], path, preset, topic, workspace, args.per_image_timeout)
            error = (proc.stderr or proc.stdout)[-500:] if proc.returncode else ""
            if not error and not path.is_file():
                error = "Generator returned without an output image"
        except subprocess.TimeoutExpired:
            error = "Generation timed out"
        with lock:
            if error:
                failures.append({"id": plan["id"], "reason": error})
                if len(failures) >= args.max_failures:
                    stop.set()
            else:
                outputs.append(str(path))

    with ThreadPoolExecutor(max_workers=max(1, args.concurrency)) as pool:
        list(pool.map(process, plans))
    print(json.dumps({"ok": not failures and len(outputs) == len(plans), "mode": "batch",
                      "output_dir": str(output_dir), "metadata_dir": str(metadata_dir),
                      "outputs": sorted(outputs), "failures": failures}, ensure_ascii=False))
    return 0 if not failures and len(outputs) == len(plans) else 1


if __name__ == "__main__":
    raise SystemExit(main())
