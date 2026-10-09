#!/usr/bin/env python3
"""Install this checkout's virtualenv and optional agent skills/MCP dependencies.

Run with a working Python 3.11+: py -3 scripts/setup.py --agent codex
Use --check to verify without installing, generating images, or contacting a provider.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys
import venv

from sync_skills import agent_skill_root, sync

ROOT = Path(__file__).resolve().parents[1]


def run(command: list[str], *, capture: bool = False) -> subprocess.CompletedProcess:
    return subprocess.run(command, cwd=ROOT, check=True, text=True,
                          encoding="utf-8", capture_output=capture)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--agent", action="append", choices=["codex", "claude", "omp"],
                        help="install/check linked skills for this agent; repeatable")
    parser.add_argument("--mcp", action="store_true", help="include optional MCP server dependencies")
    parser.add_argument("--check", action="store_true", help="verify only; no installation or provider calls")
    args = parser.parse_args()
    if sys.version_info < (3, 11):
        print("Python 3.11+ is required. On Windows try: py -3 scripts/setup.py", file=sys.stderr)
        return 1
    venv_dir = ROOT / ".venv"
    python = venv_dir / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
    try:
        if not python.is_file():
            if args.check:
                raise RuntimeError("Missing .venv; run setup without --check first.")
            if venv_dir.exists():
                raise RuntimeError("Existing .venv is incomplete; it was preserved. Rename it before reinstalling.")
            venv.EnvBuilder(with_pip=True).create(venv_dir)
        run([str(python), "-c", "import sys; assert sys.version_info >= (3,11), 'Python 3.11+ required'"], capture=True)
        if not args.check:
            requirements = "requirements-mcp.txt" if args.mcp else "requirements.txt"
            run([str(python), "-m", "pip", "install", "-r", str(ROOT / requirements)])
        run([str(python), "-c", "import PIL; assert int(PIL.__version__.split('.')[0]) == 12, 'Pillow 12 required'"], capture=True)
        run([str(python), "-m", "pip", "check"], capture=True)
        if args.mcp:
            run([str(python), str(ROOT / "tools/mcp_server.py"), "--help"], capture=True)
        roots = list(dict.fromkeys(agent_skill_root(agent) for agent in (args.agent or [])))
        if roots:
            report = sync(ROOT / "skills", roots, apply=not args.check, create_missing=True)
            for item in report:
                print(f"{item['action']}: {item['root']} {item['skill']} {item['detail']}")
            if any(item["action"] not in {"ok", "linked"} for item in report) or not report:
                raise RuntimeError("Skill links are missing/stale; run setup without --check to repair them.")
        run([str(python), str(ROOT / "tools/ai_image.py"), "--schema"], capture=True)
        spec = {"mode": "single", "prompt": "installation verification", "topic": "install-check",
                "topic_root": str(ROOT), "count": 1, "size": "1920x1080", "quality": "high",
                "dry_run": True}
        result = run([str(python), str(ROOT / "tools/ai_image.py"), "--json", json.dumps(spec)], capture=True)
        if not json.loads(result.stdout.strip().splitlines()[-1]).get("ok"):
            raise RuntimeError("CLI dry-run verification failed.")
        print(f"Verified checkout: {ROOT}")
        print(f"Python: {python}")
        print("Local installation verified. Provider connectivity/quota still needs --list-models and a real generation.")
        return 0
    except subprocess.CalledProcessError as exc:
        print(f"Command failed (exit {exc.returncode}): {exc.cmd}", file=sys.stderr)
        if exc.stderr:
            print(exc.stderr.strip(), file=sys.stderr)
        if exc.stdout:
            print(exc.stdout.strip(), file=sys.stderr)
        return 1
    except (OSError, RuntimeError, ValueError) as exc:
        print(f"Setup failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
