"""Inspect or reorganize legacy generated assets without overwriting files."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from gallery import gallery_root, write_json

IMAGES = {".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".tif", ".tiff", ".svg", ".ico", ".icns", ".avif"}
TEXT = {".json", ".jsonl", ".py", ".ps1", ".mjs", ".js", ".md", ".txt", ".csv", ".html", ".xml"}


def plan_moves(workspace: Path) -> list[tuple[Path, Path]]:
    workspace = workspace.resolve()
    gallery = gallery_root(workspace)
    sources = []
    if workspace != gallery:
        for name in ("output", "runs", "refs"):
            directory = workspace / name
            if directory.is_dir():
                sources.extend((p, name) for p in directory.rglob("*") if p.is_file())
        debug = workspace / "debug_events.json"
        if debug.is_file():
            sources.append((debug, "debug"))
    if gallery.is_dir():
        for child in gallery.iterdir():
            if child.name in {"metadata"}:
                continue
            sources.extend((p, "gallery") for p in ([child] if child.is_file() else child.rglob("*")) if p.is_file())
    topics = {p.name for p in (workspace / "output").iterdir() if p.is_dir()} if (workspace / "output").is_dir() else set()
    topics |= {p.name for p in gallery.iterdir() if p.is_dir() and p.name not in {"output", "metadata"}} if gallery.is_dir() else set()
    moves, reserved = [], set()
    for source, kind in sorted(sources):
        if source.is_symlink() or not source.resolve().is_relative_to(workspace):
            raise ValueError(f"Refusing linked/outside source: {source}")
        if kind == "debug":
            topic, parts = "_legacy", [source.name]
        elif kind == "refs":
            topic, parts = "_references", list(source.relative_to(workspace / "refs").parts)
        else:
            base = gallery if kind == "gallery" else workspace / kind
            parts = list(source.relative_to(base).parts)
            if kind == "gallery" and parts[0] == "output":
                parts.pop(0)
            if parts[0] == "studio" and len(parts) >= 3:
                parts.pop(0)
            topic = parts.pop(0) if len(parts) > 1 else "_legacy"
            if topic == "mcp":
                topic = "_mcp"
            if len(parts) == 1 and kind == "runs":
                topic = next((t for t in sorted(topics, key=len, reverse=True) if parts[0].startswith(t)), topic)
            while len(parts) > 1 and parts[0] in {"output", topic}:
                parts.pop(0)
        is_image = source.suffix.lower() in IMAGES and kind not in {"refs", "runs", "debug"} and not any(p.lower() in {"refs", "references", "ui-selected-images"} for p in parts[:-1])
        destination = gallery / ("output" if is_image else "metadata") / topic / Path(*parts)
        if source == destination:
            continue
        original = destination
        number = 1
        while destination.exists() or destination in reserved:
            destination = original.with_name(f"{original.stem}__legacy_{number}{original.suffix}")
            number += 1
        if not destination.resolve().is_relative_to(gallery):
            raise ValueError(f"Destination outside gallery: {destination}")
        reserved.add(destination)
        moves.append((source, destination))
    return moves


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def rewrite_paths(workspace: Path, moves: list[tuple[Path, Path]]) -> int:
    gallery = gallery_root(workspace)
    replacements = {}
    directories = {}
    for source, destination in moves:
        parent, target = source.parent, destination.parent
        while parent != workspace and parent != gallery and parent.is_relative_to(workspace):
            previous = directories.get(parent)
            if previous is None or target.is_relative_to(gallery / "output"):
                directories[parent] = target
            parent, target = parent.parent, target.parent
    pairs = list(directories.items()) + [(workspace / "output", gallery / "output"),
             (workspace / "runs", gallery / "metadata"),
             (workspace / "refs", gallery / "metadata" / "_references"),
             (workspace / "output" / "studio", gallery / "output")] + moves
    for source, destination in pairs:
        for old, new in ((str(source), str(destination)), (source.as_posix(), destination.as_posix()),
                         (str(source).replace("\\", "\\\\"), str(destination).replace("\\", "\\\\"))):
            replacements[old.casefold()] = new
    pattern = re.compile("|".join(re.escape(p) for p in sorted(replacements, key=len, reverse=True)), re.IGNORECASE)
    rewritten = 0
    for _, destination in moves:
        if destination.suffix.lower() not in TEXT:
            continue
        try:
            text = destination.read_text(encoding="utf-8-sig")
        except UnicodeError:
            continue
        updated = pattern.sub(lambda match: replacements[match[0].casefold()], text)
        if updated != text:
            destination.write_text(updated, encoding="utf-8")
            rewritten += 1
    return rewritten


def apply_moves(workspace: Path, moves: list[tuple[Path, Path]]) -> dict:
    workspace = workspace.resolve()
    gallery = gallery_root(workspace)
    # Validate the entire plan before changing any files.
    for source, destination in moves:
        if not source.resolve().is_relative_to(workspace) or not destination.resolve().is_relative_to(gallery):
            raise ValueError("Move escapes the selected workspace")
        if not source.is_file() or destination.exists():
            raise ValueError(f"Plan changed; inspect again: {source}")
    record = gallery / "metadata" / "_maintenance" / f"organize-{datetime.now():%Y%m%d-%H%M%S-%f}.json"
    report = {"workspace": str(workspace), "moves": [{"from": str(a), "to": str(b)} for a, b in moves]}
    if not moves:
        return {"moved": 0, "rewritten": 0}
    write_json(record, report)
    hashes = {}
    for source, destination in moves:
        before = digest(source)
        destination.parent.mkdir(parents=True, exist_ok=True)
        source.rename(destination)
        if digest(destination) != before:
            raise RuntimeError(f"Content changed during move: {destination}")
        hashes[str(destination)] = before
    rewritten = rewrite_paths(workspace, moves)
    for _, destination in moves:
        if destination.suffix.lower() in TEXT:
            hashes[str(destination)] = digest(destination)
    # Only empty directories inside explicitly selected asset roots are removed.
    roots = [gallery] + ([workspace / n for n in ("output", "runs", "refs")] if workspace != gallery else [])
    for root in roots:
        if not root.is_dir():
            continue
        for path in sorted([p for p in root.rglob("*") if p.is_dir()] + [root], key=lambda p: len(p.parts), reverse=True):
            if not path.resolve().is_relative_to(workspace):
                raise ValueError("Directory outside workspace")
            try:
                path.rmdir()
            except OSError:
                pass
    report.update(verified_sha256=hashes, rewritten=rewritten)
    write_json(record, report)
    return {"moved": len(moves), "rewritten": rewritten, "record": str(record)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", required=True, type=Path)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    workspace = args.workspace.resolve()
    if not workspace.is_dir():
        parser.error("Workspace does not exist")
    moves = plan_moves(workspace)
    summary = {"workspace": str(workspace), "planned": len(moves),
               "destinations": dict(Counter(p.relative_to(gallery_root(workspace)).parts[0] for _, p in moves))}
    if args.apply:
        summary.update(apply_moves(workspace, moves))
    print(json.dumps(summary, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
