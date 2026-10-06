#!/usr/bin/env python3
"""Link every agent skill directory to this repository's `skills/` tree.

The repository is the single source of truth. Agent skill folders such as
`~/.omp/agent/skills/<name>` are directory links back into
`<repo>/skills/<name>`, so `git pull` alone updates every agent on the machine.

Usage:
    python scripts/sync_skills.py            # link, backing up real directories
    python scripts/sync_skills.py --agent codex  # create a fresh user's skill root
    python scripts/sync_skills.py --check    # report only, exit 1 when out of sync
    python scripts/sync_skills.py --json     # machine-readable report
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import stat
import subprocess
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


def agent_skill_root(agent: str, home: Path | None = None) -> Path:
    base = home or Path.home()
    return {
        "codex": base / ".agents" / "skills",
        "claude": base / ".claude" / "skills",
        "omp": base / ".omp" / "agent" / "skills",
    }[agent]


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


def default_backup_dir() -> Path:
    return Path.home() / ".ai-image-generator" / "skill-backups"


def sync(
    skills_dir: Path,
    skill_roots: list[Path],
    apply: bool = True,
    backup_dir: Path | None = None,
    create_missing: bool = False,
) -> list[dict[str, str]]:
    """Point every existing agent skill root at `skills_dir`.

    Existing roots are touched by default; explicitly selected agents can create
    their roots with create_missing. Displaced directories move
    outside the skill root, because agents scan every subdirectory of a skill
    root and a leftover backup would register as a second copy of the skill.
    """
    sources = sorted(p for p in skills_dir.iterdir() if p.is_dir() and (p / "SKILL.md").is_file()) if skills_dir.is_dir() else []
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S-%f")
    backups = backup_dir or default_backup_dir()
    backups = backups.expanduser().resolve()
    if any(backups.is_relative_to(root.resolve()) for root in skill_roots):
        raise ValueError("Keep skill backups outside every selected agent skill root")
    results: list[dict[str, str]] = []

    for root in skill_roots:
        if not root.is_dir():
            if create_missing and apply:
                root.mkdir(parents=True, exist_ok=True)
            else:
                results.append({"root": str(root), "skill": "", "action": "stale" if create_missing else "root-absent", "detail": "root missing"})
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
                if os.name == "nt" and os.lstat(link).st_reparse_tag == MOUNT_POINT_TAG:
                    link.rmdir()  # Remove only the junction, preserving its target.
                else:
                    link.unlink()
                detail = f"relinked from {target}"
            elif state in {"dir", "file"}:
                if source.resolve().is_relative_to(link.resolve()):
                    raise ValueError(f"Refusing to move the source checkout: {link}")
                slug = re.sub(r"[^A-Za-z0-9]+", "-", str(root)).strip("-")
                backup = backups / f"{slug}-{source.name}-{stamp}"
                backup.parent.mkdir(parents=True, exist_ok=True)
                # Both absolute paths are within the selected root/backup directory.
                # shutil.move also preserves old skills when drives differ.
                if not link.absolute().is_relative_to(root.absolute()) or not backup.resolve().is_relative_to(backups):
                    raise ValueError("Skill backup paths escaped the selected directories")
                shutil.move(str(link), str(backup))
                detail = f"backup {backup}"
            make_link(link, source.resolve())
            results.append({"root": str(root), "skill": source.name, "action": "linked", "detail": detail})

    return results


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    _ = ap.add_argument("--check", action="store_true", help="report only; exit 1 when a link is missing or stale")
    _ = ap.add_argument("--json", dest="as_json", action="store_true", help="print a JSON report")
    ap.add_argument("--agent", action="append", choices=["codex", "claude", "omp"], help="select an agent and create its skill root; repeatable")
    ap.add_argument("--root", action="append", type=Path, help="explicit custom skill root; repeatable (creates it unless --check)")
    args = ap.parse_args()

    roots = [agent_skill_root(agent) for agent in (args.agent or [])]
    roots.extend(path.expanduser().resolve() for path in (args.root or []))
    explicit = bool(roots)
    roots = list(dict.fromkeys(roots)) if explicit else default_skill_roots()
    results = sync(SKILLS_DIR, roots, apply=not args.check, create_missing=explicit)
    stale = [r for r in results if r["action"] == "stale"]

    if args.as_json:
        print(json.dumps({"repo": str(ROOT), "results": results}, ensure_ascii=False))
    else:
        print(f"repo: {ROOT}")
        for r in results:
            skill = r["skill"] or "-"
            print(f"  {r['action']:<12} {skill:<24} {r['root']} {r['detail']}")

    usable = any(r["action"] in {"ok", "linked"} for r in results)
    return 1 if stale or not usable else 0


if __name__ == "__main__":
    raise SystemExit(main())
