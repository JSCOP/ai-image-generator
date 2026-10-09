#!/usr/bin/env python3
"""One-shot helper: generate exactly one image per art-direction candidate.

Reads presets/art-direction-candidates.json, then for each category calls
scripts/gen_image.py with the first template + global_style + negative.
Output: ImageGallery/output/art-direction-candidates/<category>.png
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from gallery import topic_dirs
from gen_image import DEFAULT_IMAGE_MODEL

ROOT = Path(__file__).resolve().parent.parent
PRESET = ROOT / "presets" / "art-direction-candidates.json"
GEN = ROOT / "scripts" / "gen_image.py"
OUT_ROOT = topic_dirs(ROOT, "art-direction-candidates")[0]
QUALITY = os.environ.get("QUALITY", "high")
CONCURRENCY = int(os.environ.get("CONCURRENCY", "6"))


def build_prompt(tpl: str, style: str, negative: str) -> str:
    parts = [tpl.strip()]
    if style.strip():
        parts.append(style.strip())
    if negative.strip():
        parts.append("Constraints: " + negative.strip())
    return " ".join(parts)


def run_one(name: str, prompt: str, size: str, model: str, image_model: str) -> tuple[str, int, str]:
    out_path = OUT_ROOT / f"{name}.png"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        sys.executable, str(GEN), prompt,
        "-o", str(out_path),
        "--topic", "art-direction-candidates",
        "--topic-root", str(ROOT),
        "--model", model,
        "--image-model", image_model,
        "--size", size,
        "--quality", QUALITY,
        "--action", "generate",
    ]
    print(f"[start] {name}", flush=True)
    proc = subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=900)
    return name, proc.returncode, (proc.stderr[-400:] if proc.returncode != 0 else str(out_path))


def main() -> int:
    data = json.loads(PRESET.read_text(encoding="utf-8"))
    style = data.get("global_style", "")
    negative = data.get("negative", "")
    size = data.get("size", "1920x1088")
    model = data.get("model", "gpt-5.5")
    image_model = data.get("image_model") or os.environ.get("CLIPROXY_IMAGE_MODEL") or DEFAULT_IMAGE_MODEL
    cats = data["categories"]

    print(f"Generating {len(cats)} candidates @ quality={QUALITY}, concurrency={CONCURRENCY}")
    tasks = [(c["name"], build_prompt(c["templates"][0], style, negative)) for c in cats]

    with ThreadPoolExecutor(max_workers=CONCURRENCY) as pool:
        futs = [pool.submit(run_one, n, p, size, model, image_model) for n, p in tasks]
        for fut in as_completed(futs):
            name, rc, info = fut.result()
            if rc == 0:
                print(f"[ok] {name} -> {info}", flush=True)
            else:
                print(f"[FAIL] {name} ({rc}): {info}", flush=True, file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
