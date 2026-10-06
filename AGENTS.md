# Project instructions

- For installation on another PC, dependency setup, skill registration/repair, or updates, read [INSTALL.md](INSTALL.md). Use the actual checkout path and its `.venv` interpreter; the `E:` drive and existing user names in historical examples are optional.
- The skill source is `skills/ai-image-generator/`. Register directory links with `scripts/setup.py --agent <agent>` or `scripts/sync_skills.py`; edit the repository source so a pull updates linked agents.
- For generation, read `skills/ai-image-generator/SKILL.md` and use `tools/ai_image.py`. Set `topic_root` to the user's destination project. Verify outputs and exact requested model IDs before reporting success.
- Local install checks use `scripts/setup.py --check` and `python -m unittest discover -s tests -v`. These checks use dry-runs/loopback fixtures; real generation requires a reachable CLIProxyAPI server and quota.
- Preserve `.venv`, local configuration, reference images, and `ImageGallery` during updates. Pull with `--ff-only` after checking local changes. Keep credentials in environment variables or Studio session memory; never commit them.

## Automation providers

Use structured shell/CLI operations for files, configuration, and processes; browser tools for browser content; desktop automation when those surfaces cannot express the task. Use only installed desktop providers. If the selected provider is unavailable, report its exact error without installing or starting another provider. For authenticated account/session browser work use Browser Use; for program/website debugging or testing use Playwright MCP.
