#!/usr/bin/env python3
"""Link every agent skill directory to this repository's `skills/` tree.

The repository is the single source of truth. Agent skill folders such as
`~/.omp/agent/skills/<name>` are directory links back into
`<repo>/skills/<name>`, so `git pull` alone updates every agent on the machine.

Usage:
    python scripts/sync_skills.py            # link, backing up real directories
    python scripts/sync_skills.py --check    # report only, exit 1 when out of sync
    python scripts/sync_skills.py --json     # machine-readable report
"""
from __future__ import annotations

import argparse
import json
import os
import stat
import subprocess
import sys
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKILLS_DIR = ROOT / "skills"

MOUNT_POINT_TAG = getattr(stat, "IO_REPARSE_TAG_MOUNT_POINT", None)
SYMLINK_TAG = getattr(stat, "IO_REPARSE_TAG_SYMLINK", None)


def default_skill_roots(home: Path | None = None) -> list[Path]:
    base = home or Path.home()
    return [
        base / ".omp" / "agent" / "skills",
        base / ".claude" / "skills",
        base / ".codex" / "skills",
        base / ".agents" / "skills",
    ]


def link_state(path: Path) -> tuple[str, Path | None]:
    """Classify `path` as missing, link, dir, or file, plus the link target."""
    try:
        info = os.lstat(path)
    except OSError:
        return "missing", None
    tag = getattr(info, "st_reparse_tag", 0)
    is_link = os.path.islink(path) or tag in {t for t in (MOUNT_POINT_TAG, SYMLINK_TAG) if t}
    if is_link:
        try:
            return "link", Path(os.path.realpath(path))
        except OSError:
            return "link", None
    if stat.S_ISDIR(info.st_mode):
        return "dir", None
    return "file", None


def make_link(link: Path, target: Path) -> None:
    if os.name == "nt":
        # Junctions need no elevation and no developer mode.
        proc = subprocess.run(
            ["cmd", "/c", "mklink", "/J", str(link), str(target)],
            capture_output=True,
            text=True,
        )
        if proc.returncode != 0:
            raise OSError(f"mklink failed for {link}: {proc.stdout.strip()} {proc.stderr.strip()}")
        return
    os.symlink(target, link, target_is_directory=True)


def sync(
    skills_dir: Path,
    skill_roots: list[Path],
    apply: bool = True,
) -> list[dict[str, str]]:
    """Point every existing agent skill root at `skills_dir`.

    Only roots that already exist are touched; a missing agent directory means
    that agent is not installed on this machine.
    """
    sources = sorted(p for p in skills_dir.iterdir() if p.is_dir()) if skills_dir.is_dir() else []
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    results: list[dict[str, str]] = []

    for root in skill_roots:
        if not root.is_dir():
            results.append({"root": str(root), "skill": "", "action": "root-absent", "detail": ""})
            continue
        for source in sources:
            link = root / source.name
            state, target = link_state(link)
            if state == "link" and target == source.resolve():
                results.append({"root": str(root), "skill": source.name, "action": "ok", "detail": str(target)})
                continue
            if not apply:
                results.append({"root": str(root), "skill": source.name, "action": "stale", "detail": state})
                continue

            detail = ""
            if state == "link":
                link.unlink()
                detail = f"relinked from {target}"
            elif state in {"dir", "file"}:
                backup = root / f"{source.name}.bak-{stamp}"
                link.rename(backup)
                detail = f"backup {backup.name}"
            make_link(link, source.resolve())
            results.append({"root": str(root), "skill": source.name, "action": "linked", "detail": detail})

    return results


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    _ = ap.add_argument("--check", action="store_true", help="report only; exit 1 when a link is missing or stale")
    _ = ap.add_argument("--json", dest="as_json", action="store_true", help="print a JSON report")
    args = ap.parse_args()

    results = sync(SKILLS_DIR, default_skill_roots(), apply=not args.check)
    stale = [r for r in results if r["action"] == "stale"]

    if args.as_json:
        print(json.dumps({"repo": str(ROOT), "results": results}, ensure_ascii=False))
    else:
        print(f"repo: {ROOT}")
        for r in results:
            skill = r["skill"] or "-"
            print(f"  {r['action']:<12} {skill:<24} {r['root']} {r['detail']}")

    return 1 if stale else 0


if __name__ == "__main__":
    raise SystemExit(main())
