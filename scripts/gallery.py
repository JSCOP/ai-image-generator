"""Shared image-only output and per-image request records."""
from __future__ import annotations

import json
import os
import uuid
from pathlib import Path


def gallery_root(workspace: str | Path) -> Path:
    root = Path(workspace).expanduser().resolve()
    # Accept an existing gallery (including old gallery/topic roots) as well.
    for parent in (root, *root.parents):
        if parent.name.casefold() == "imagegallery":
            return parent
    return root / "ImageGallery"


def topic_dirs(workspace: str | Path, topic: str) -> tuple[Path, Path]:
    root = gallery_root(workspace)
    return root / "output" / topic, root / "metadata" / topic


def metadata_path(output: Path, workspace: str | Path, topic: str) -> Path:
    images, records = topic_dirs(workspace, topic)
    output = output.resolve()
    relative = output.relative_to(images) if output.is_relative_to(images) else Path(output.name)
    return records / (str(relative) + ".json")


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + "." + uuid.uuid4().hex + ".tmp")
    try:
        temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)
