#!/usr/bin/env python3
"""Generate the T2IN core airport event-hazard top-down CCTV dataset.

This script is intentionally a small orchestration wrapper around
scripts/gen_image.py. It keeps the approved 10 spaces x 10 event categories x
10 images matrix deterministic, supports a pilot-first workflow, and verifies
folder counts without changing T2IN contracts or dashboard code.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import json
import shutil
import subprocess
import sys
import threading
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from gallery import topic_dirs
from gen_image import DEFAULT_IMAGE_MODEL
from typing import Literal

try:
    from PIL import Image, ImageDraw, ImageFont
except Exception:  # pragma: no cover - contact sheet is best-effort.
    Image = None
    ImageDraw = None
    ImageFont = None


ROOT = Path(__file__).resolve().parent.parent
GEN_IMAGE = ROOT / "scripts" / "gen_image.py"
RUN_TOPIC = "t2in-event-hazards-core10-20260511"
PILOT_TOPIC = f"{RUN_TOPIC}-pilot"
COMPLETE_TOPIC = f"{RUN_TOPIC}-complete"
RUN_DIR = topic_dirs(ROOT, RUN_TOPIC)[1]
PILOT_OUT = topic_dirs(ROOT, PILOT_TOPIC)[0]
COMPLETE_OUT = topic_dirs(ROOT, COMPLETE_TOPIC)[0]

SIZE = "1920x1088"
QUALITY = "high"
MODEL = "gpt-5.5"
IMAGE_MODEL = DEFAULT_IMAGE_MODEL


@dataclass(frozen=True)
class Space:
    id: str
    ko: str
    description: str


@dataclass(frozen=True)
class Event:
    id: str
    event_type: str
    ko: str
    template: str
    prompt: str


@dataclass(frozen=True)
class Job:
    id: str
    space_id: str
    space_label: str
    event_id: str
    event_type: str
    event_label: str
    template: str
    variant: int
    prompt: str
    pilot_output: str
    complete_output: str


SPACES: list[Space] = [
    Space(
        "check_in_counters",
        "체크인카운터",
        "airline check-in counter row with self-service kiosks, baggage scales, queue lanes, stanchions, staff counters, and rolling luggage",
    ),
    Space(
        "duty_free",
        "면세점",
        "duty-free shopping zone with open display islands, glass storefronts, wide circulation paths, planters, and generic blank sign panels",
    ),
    Space(
        "retail_concourse",
        "상가",
        "airport retail concourse with small shop entrances, glass partitions, central walkway, display tables, benches, and scattered shoppers with carry-ons",
    ),
    Space(
        "departure_hall",
        "출국장",
        "large departures hall with polished tile floor, open walking areas, seating rows, queue barriers, digital signboards as blank top surfaces, and luggage carts",
    ),
    Space(
        "security_checkpoint",
        "보안검색대",
        "airport security screening area with parallel lanes, tray return belts, queue grids, inspection tables, walk-through scanners seen only from above, and staff points",
    ),
    Space(
        "airport_entrance",
        "공항입구",
        "terminal entrance lobby with sliding glass door lines, information desk, wayfinding pillars, cart return area, waiting travelers, and broad polished floor",
    ),
    Space(
        "gate_waiting_area",
        "탑승구_대기공간",
        "gate waiting lounge with rows of connected seats, charging tables, boarding desk, blank digital panels, glass wall light, and luggage beside seats",
    ),
    Space(
        "boarding_gate",
        "탑승구",
        "boarding gate podiums with queue rails, rectangular gate threshold, compact boarding lanes, nearby seating, staff counters, and travelers with carry-ons",
    ),
    Space(
        "immigration_area",
        "출입국심사",
        "immigration control hall with booth rows, glass partitions, parallel queue lanes, document check counters, compact service booths, and carry-on luggage",
    ),
    Space(
        "baggage_drop",
        "수하물위탁",
        "self-service baggage drop area with automated machines, conveyor mouths, scales, queue ropes, suitcase clusters, staff assistance points, and luggage carts",
    ),
]


EVENTS: list[Event] = [
    Event(
        "density_threshold",
        "density_threshold",
        "인구밀집",
        "crowd_density",
        "dense but orderly crowd congestion exceeding a safe density threshold, compressed queues and blocked walking lanes, staff managing passenger flow, no panic",
    ),
    Event(
        "loitering",
        "loitering",
        "배회",
        "loitering",
        "one or two people lingering without clear purpose near a pillar, kiosk, barrier, or service edge while normal passenger flow passes around them; suspicious stationary posture but no confrontation",
    ),
    Event(
        "person_fall",
        "person_fall",
        "쓰러짐/실신",
        "fall_duration",
        "one traveler lying on the floor beside luggage, nearby passengers and staff forming a clear assistance buffer, non-graphic with no visible injury or blood",
    ),
    Event(
        "fight",
        "fight",
        "폭행/싸움",
        "fight",
        "two people in a non-graphic physical confrontation with raised arms or pushing posture, bystanders keeping distance, staff approaching, no weapons, no blood, no severe injury",
    ),
    Event(
        "fire_smoke",
        "fire_smoke",
        "화재/방화",
        "fire_smoke",
        "small contained fire or visible smoke near a bin, fixture, kiosk, or service equipment, orange glow and light smoke visible from overhead, travelers keeping a safe distance",
    ),
    Event(
        "abandoned_baggage",
        "abandoned_baggage",
        "유기/방치",
        "abandoned_object",
        "unattended object incident; variants 1-7 show isolated abandoned luggage or carry-on baggage with people keeping distance, variants 8-10 show a separated child or missing-child situation with airport staff approaching calmly",
    ),
    Event(
        "perimeter_breach",
        "perimeter_breach",
        "침입",
        "perimeter_breach",
        "unauthorized entry into a restricted lane, staff-only doorway, service corridor, or closed barrier area; person crossing the boundary while staff respond calmly",
    ),
    Event(
        "suspicious_object",
        "suspicious_object",
        "의심물체",
        "suspicious_object",
        "unusual unidentified package, box, backpack, or device-like object placed in a walkway or near service equipment, with a visible empty buffer and cautious bystanders",
    ),
    Event(
        "abnormal_speed",
        "abnormal_speed",
        "비정상 이동",
        "abnormal_speed",
        "one person running, dashing, or moving against the normal passenger flow with slight overhead motion blur and startled spacing around them, no collision or injury",
    ),
    Event(
        "custom_event_blackout_power_outage",
        "custom_event",
        "정전",
        "blackout_power_outage",
        "localized power outage with darkened zone, emergency floor lights, battery-backed blank sign panels, staff guiding passengers with safe visibility, CCTV-like low-light overhead view",
    ),
]


VARIANTS = [
    "wide overview with moderate crowd and clear floor-plan geometry",
    "sparse scene with isolated tracking targets and large clean floor areas",
    "busy but orderly scene with several partial occlusions from luggage carts and queue rails",
    "queue-heavy scene with compressed lanes and clear event focal point",
    "cart-heavy scene with rolling suitcases and nested luggage carts near the event",
    "staff-response scene with uniformed staff approaching while passengers keep distance",
    "open crossing-path scene with pedestrians moving around the event area",
    "seating-edge scene near benches or waiting chairs with suitcases beside people",
    "kiosk or service-counter edge scene with machines and blank display panels",
    "occlusion-challenge scene where the event remains readable despite furniture, rails, and luggage",
]


PROMPT_PREFIX = """Create a photorealistic synthetic safety-incident dataset frame for T2IN airport top-down CCTV simulation.
Camera and geometry are mandatory: exactly 90-degree vertical overhead view, orthographic-like ceiling CCTV, no tilt, no 3/4 perspective, no horizon, no visible ceiling, no wall elevation, no balcony viewpoint.
Show only floor-plan geometry and top surfaces of counters, kiosks, seats, belts, glass railings, luggage carts, suitcases, planters, queue rails, and people.
People must appear as direct-overhead heads, shoulders, arms, coats, backpacks, and luggage shapes, suitable for multi-object tracking and VLM event recognition.
Keep polished airport floors, realistic indoor lighting, readable crowd flow, and a clear event focal point."""


PROMPT_SUFFIX = """Hard constraints: generic fictional airport only; no real brand logos; no readable airline or airport names; no readable Korean or English text; no watermark; no UI overlay; no labels; no captions; no map overlay; no fisheye; no oblique angle.
Safety constraints: non-graphic safety-training image; no blood; no gore; no severe injury; no weapons; no panic stampede; no close-up faces; no sexualized content.
Output one coherent natural overhead airport CCTV photograph."""


def abandoned_variant_prompt(variant: int) -> str:
    if variant <= 7:
        return (
            "abandoned luggage subcase: one unattended suitcase, backpack, or carry-on is isolated in the space, "
            "with a visible empty buffer around it and passengers avoiding the object"
        )
    return (
        "separated-child subcase: one small child-like traveler figure is separated from nearby adult groups, "
        "airport staff are calmly approaching, no distress close-up, no harm, safety-training context"
    )


def build_prompt(space: Space, event: Event, variant: int) -> str:
    variant_text = VARIANTS[variant - 1]
    event_detail = abandoned_variant_prompt(variant) if event.id == "abandoned_baggage" else event.prompt
    return "\n".join(
        [
            PROMPT_PREFIX,
            f"Space category: {space.id} / {space.ko}. Scene: {space.description}.",
            f"Event category: {event.id} / {event.ko}. Contract mapping: event_type={event.event_type}, template={event.template}.",
            f"Event depiction: {event_detail}.",
            f"Variation {variant:02d}: {variant_text}.",
            "Make the event visually unambiguous from overhead while preserving realistic passenger flow and airport-scale object sizes.",
            PROMPT_SUFFIX,
        ]
    )


def all_jobs() -> list[Job]:
    jobs: list[Job] = []
    for space in SPACES:
        for event in EVENTS:
            for variant in range(1, 11):
                job_id = f"{space.id}__{event.id}__{variant:02d}"
                rel = Path(space.id) / event.id / f"{job_id}.png"
                jobs.append(
                    Job(
                        id=job_id,
                        space_id=space.id,
                        space_label=space.ko,
                        event_id=event.id,
                        event_type=event.event_type,
                        event_label=event.ko,
                        template=event.template,
                        variant=variant,
                        prompt=build_prompt(space, event, variant),
                        pilot_output=str((PILOT_OUT / rel).resolve()),
                        complete_output=str((COMPLETE_OUT / rel).resolve()),
                    )
                )
    return jobs


def selected_jobs(scope: Literal["pilot", "remaining", "complete"]) -> list[Job]:
    jobs = all_jobs()
    if scope == "pilot":
        return [j for j in jobs if j.variant == 1]
    if scope == "remaining":
        return jobs
    return jobs


def ensure_run_dir() -> None:
    RUN_DIR.mkdir(parents=True, exist_ok=True)


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")


def plan() -> None:
    ensure_run_dir()
    jobs = all_jobs()
    write_jsonl(RUN_DIR / "jobs_all.jsonl", [asdict(j) for j in jobs])
    write_jsonl(RUN_DIR / "jobs_pilot.jsonl", [asdict(j) for j in jobs if j.variant == 1])
    write_jsonl(RUN_DIR / "jobs_remaining.jsonl", [asdict(j) for j in jobs])

    summary = {
        "run_topic": RUN_TOPIC,
        "pilot_topic": PILOT_TOPIC,
        "complete_topic": COMPLETE_TOPIC,
        "size": SIZE,
        "quality": QUALITY,
        "model": MODEL,
        "image_model": IMAGE_MODEL,
        "spaces": [asdict(s) for s in SPACES],
        "events": [asdict(e) for e in EVENTS],
        "counts": {
            "spaces": len(SPACES),
            "events": len(EVENTS),
            "variants_per_space_event": 10,
            "pilot_images": 100,
            "complete_images": 1000,
            "remaining_api_generations_after_pilot": 900,
        },
        "outputs": {
            "pilot": str(PILOT_OUT.resolve()),
            "complete": str(COMPLETE_OUT.resolve()),
            "run_dir": str(RUN_DIR.resolve()),
        },
    }
    write_json(RUN_DIR / "summary.json", summary)
    print(json.dumps(summary["counts"], ensure_ascii=False, sort_keys=True))


def output_for_scope(job: Job, scope: str) -> Path:
    if scope == "pilot":
        return Path(job.pilot_output)
    return Path(job.complete_output)


def copy_pilot_to_complete(job: Job, overwrite: bool) -> dict:
    src = Path(job.pilot_output)
    dst = Path(job.complete_output)
    if dst.is_file() and not overwrite:
        return {"id": job.id, "status": "skipped_existing", "output": str(dst)}
    if not src.is_file():
        return {"id": job.id, "status": "missing_pilot", "output": str(dst), "error": f"pilot image missing: {src}"}
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    return {"id": job.id, "status": "copied_from_pilot", "output": str(dst), "source": str(src)}


def run_generation_job(job: Job, scope: str, overwrite: bool) -> dict:
    output_path = output_for_scope(job, scope)
    if output_path.is_file() and not overwrite:
        return {"id": job.id, "status": "skipped_existing", "output": str(output_path)}

    output_path.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        sys.executable,
        str(GEN_IMAGE),
        job.prompt,
        "--topic",
        PILOT_TOPIC if scope == "pilot" else COMPLETE_TOPIC,
        "--topic-root",
        str(ROOT),
        "-o",
        str(output_path),
        "--size",
        SIZE,
        "--quality",
        QUALITY,
        "--model",
        MODEL,
        "--image-model",
        IMAGE_MODEL,
    ]

    started = time.time()
    proc = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", cwd=str(ROOT))
    elapsed = round(time.time() - started, 2)
    result = {
        "id": job.id,
        "space_id": job.space_id,
        "event_id": job.event_id,
        "variant": job.variant,
        "status": "ok" if proc.returncode == 0 and output_path.is_file() else "failed",
        "returncode": proc.returncode,
        "elapsed_sec": elapsed,
        "output": str(output_path),
        "stdout_tail": "\n".join((proc.stdout or "").splitlines()[-3:]),
        "stderr_tail": "\n".join((proc.stderr or "").splitlines()[-8:]),
    }
    return result


def generate(scope: Literal["pilot", "remaining", "complete"], concurrency: int, overwrite: bool, max_failures: int) -> int:
    ensure_run_dir()
    plan()

    jobs = selected_jobs(scope)
    if scope == "pilot":
        runnable = jobs
    elif scope == "remaining":
        runnable = [j for j in jobs if j.variant >= 2]
    else:
        runnable = jobs

    stamp = dt.datetime.now().strftime("%Y%m%d_%H%M%S")
    result_path = RUN_DIR / f"generate_{scope}_{stamp}.jsonl"
    lock = threading.Lock()
    failures = 0
    completed = 0
    skipped = 0
    copied = 0

    with result_path.open("w", encoding="utf-8") as f:
        if scope == "remaining":
            for job in jobs:
                if job.variant != 1:
                    continue
                row = copy_pilot_to_complete(job, overwrite)
                f.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")
                if row["status"] == "copied_from_pilot":
                    copied += 1
                elif row["status"] == "skipped_existing":
                    skipped += 1
                else:
                    failures += 1

        def submit(job: Job) -> dict:
            return run_generation_job(job, scope, overwrite)

        with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, concurrency)) as pool:
            futures = [pool.submit(submit, job) for job in runnable]
            for future in concurrent.futures.as_completed(futures):
                row = future.result()
                with lock:
                    f.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")
                    f.flush()
                if row["status"] == "ok":
                    completed += 1
                elif row["status"] == "skipped_existing":
                    skipped += 1
                else:
                    failures += 1
                print(
                    f"[{completed + skipped + failures}/{len(runnable) + (100 if scope == 'remaining' else 0)}] "
                    f"{row['status']} {row['id']}"
                )
                if failures >= max_failures:
                    print(f"Reached max failures ({max_failures}); remaining queued jobs may still finish.")
                    break

    summary = {
        "scope": scope,
        "result_path": str(result_path.resolve()),
        "generated": completed,
        "copied": copied,
        "skipped": skipped,
        "failures": failures,
    }
    print(json.dumps(summary, ensure_ascii=False, sort_keys=True))
    return 0 if failures == 0 else 2


def expected_count(scope: str) -> int:
    return 100 if scope == "pilot" else 1000


def verify(scope: Literal["pilot", "complete"], contact_sheet: bool) -> int:
    ensure_run_dir()
    root = PILOT_OUT if scope == "pilot" else COMPLETE_OUT
    expected_per_pair = 1 if scope == "pilot" else 10
    rows: list[dict] = []
    failures: list[str] = []
    total = 0

    for space in SPACES:
        for event in EVENTS:
            folder = root / space.id / event.id
            files = sorted(folder.glob("*.png")) if folder.is_dir() else []
            total += len(files)
            row = {
                "space_id": space.id,
                "event_id": event.id,
                "count": len(files),
                "expected": expected_per_pair,
                "folder": str(folder),
            }
            rows.append(row)
            if len(files) != expected_per_pair:
                failures.append(f"{space.id}/{event.id}: expected {expected_per_pair}, found {len(files)}")

            if Image is not None:
                for path in files:
                    try:
                        with Image.open(path) as img:
                            if img.size != (1920, 1088):
                                failures.append(f"{path}: expected 1920x1088, found {img.size}")
                    except Exception as exc:
                        failures.append(f"{path}: cannot open image ({exc})")

    if total != expected_count(scope):
        failures.append(f"total: expected {expected_count(scope)}, found {total}")

    stamp = dt.datetime.now().strftime("%Y%m%d_%H%M%S")
    report = {
        "scope": scope,
        "root": str(root.resolve()),
        "total": total,
        "expected_total": expected_count(scope),
        "ok": not failures,
        "failures": failures,
        "matrix": rows,
    }
    report_path = RUN_DIR / f"verify_{scope}_{stamp}.json"
    write_json(report_path, report)

    if contact_sheet:
        make_contact_sheet(scope, root)

    print(json.dumps({"ok": not failures, "total": total, "report": str(report_path.resolve())}, ensure_ascii=False))
    if failures:
        for failure in failures[:25]:
            print(f"FAIL {failure}", file=sys.stderr)
    return 0 if not failures else 2


def make_contact_sheet(scope: str, root: Path) -> None:
    if Image is None or ImageDraw is None:
        print("PIL not available; skipping contact sheet.")
        return

    files = sorted(root.glob("*/*/*.png"))
    if not files:
        print("No images for contact sheet.")
        return

    max_images = 100 if scope == "pilot" else 250
    sampled = files if len(files) <= max_images else files[:: max(1, len(files) // max_images)][:max_images]
    thumb_w, thumb_h = 192, 109
    label_h = 34
    cols = 10 if scope == "pilot" else 25
    rows = (len(sampled) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * thumb_w, rows * (thumb_h + label_h)), "white")
    draw = ImageDraw.Draw(sheet)
    font = ImageFont.load_default()

    for idx, path in enumerate(sampled):
        col = idx % cols
        row = idx // cols
        x = col * thumb_w
        y = row * (thumb_h + label_h)
        try:
            with Image.open(path) as img:
                img = img.convert("RGB")
                img.thumbnail((thumb_w, thumb_h))
                ox = x + (thumb_w - img.width) // 2
                sheet.paste(img, (ox, y))
        except Exception:
            draw.rectangle((x, y, x + thumb_w - 1, y + thumb_h - 1), outline="red")
        label = "/".join(path.relative_to(root).parts[:2])
        draw.text((x + 2, y + thumb_h + 2), label[:31], fill="black", font=font)

    out = root / f"_contact_sheet_{scope}.jpg"
    sheet.save(out, "JPEG", quality=85)
    print(f"Contact sheet: {out}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="T2IN event hazard top-down CCTV dataset runner")
    parser.add_argument("--image-model", default=DEFAULT_IMAGE_MODEL, help="Image model (default: gpt-image-2.5)")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("plan", help="Write deterministic manifests and summary only")

    gen = sub.add_parser("generate", help="Generate pilot, remaining, or complete dataset")
    gen.add_argument("--scope", choices=("pilot", "remaining", "complete"), required=True)
    gen.add_argument("--concurrency", type=int, default=3)
    gen.add_argument("--overwrite", action="store_true", help="Overwrite existing images")
    gen.add_argument("--max-failures", type=int, default=20)

    ver = sub.add_parser("verify", help="Verify image counts and dimensions")
    ver.add_argument("--scope", choices=("pilot", "complete"), required=True)
    ver.add_argument("--contact-sheet", action="store_true")

    return parser.parse_args()


def main() -> int:
    global IMAGE_MODEL
    args = parse_args()
    IMAGE_MODEL = args.image_model
    if args.command == "plan":
        plan()
        return 0
    if args.command == "generate":
        return generate(args.scope, args.concurrency, args.overwrite, args.max_failures)
    if args.command == "verify":
        return verify(args.scope, args.contact_sheet)
    raise AssertionError(args.command)


if __name__ == "__main__":
    raise SystemExit(main())
