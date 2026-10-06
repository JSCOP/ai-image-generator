"""Verify fresh-PC skill installation and safe relinking without touching user skills."""
import contextlib
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import sync_skills


class SkillInstallTests(unittest.TestCase):
    def test_explicit_fresh_root_check_is_read_only_and_install_tracks_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            source = base / "repo" / "skills" / "ai-image-generator"
            source.mkdir(parents=True)
            (source / "SKILL.md").write_text("first", encoding="utf-8")
            root = sync_skills.agent_skill_root("codex", base / "new-user")
            report = sync_skills.sync(source.parent, [root], apply=False, create_missing=True)
            self.assertEqual(report[0]["action"], "stale")
            self.assertFalse(root.exists())
            report = sync_skills.sync(source.parent, [root], create_missing=True)
            self.assertEqual(report[0]["action"], "linked")
            (source / "SKILL.md").write_text("pulled update", encoding="utf-8")
            self.assertEqual((root / source.name / "SKILL.md").read_text(encoding="utf-8"), "pulled update")
            self.assertEqual(sync_skills.sync(source.parent, [root], apply=False)[0]["action"], "ok")

    def test_relink_preserves_old_checkout(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            old = base / "old" / "skills" / "demo"
            new = base / "new" / "skills" / "demo"
            root = base / "agent" / "skills"
            for path in (old, new, root):
                path.mkdir(parents=True)
            (old / "SKILL.md").write_text("old checkout", encoding="utf-8")
            (new / "SKILL.md").write_text("new checkout", encoding="utf-8")
            sync_skills.make_link(root / "demo", old)
            sync_skills.sync(new.parent, [root])
            self.assertEqual((old / "SKILL.md").read_text(encoding="utf-8"), "old checkout")
            self.assertEqual((root / "demo" / "SKILL.md").read_text(encoding="utf-8"), "new checkout")

    def test_check_does_not_succeed_when_no_agents_exist(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            source = base / "skills" / "demo"
            source.mkdir(parents=True)
            with patch.object(sync_skills, "SKILLS_DIR", source.parent), \
                 patch.object(sync_skills, "default_skill_roots", return_value=[base / "missing"]), \
                 patch.object(sys, "argv", ["sync_skills.py", "--check"]), \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(sync_skills.main(), 1)


if __name__ == "__main__":
    unittest.main()
