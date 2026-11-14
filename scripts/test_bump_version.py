#!/usr/bin/env python3
"""Tests for the release check in bump_version.py: python scripts/test_bump_version.py

The check is what keeps the changelog and the release history honest at the moment of a release, so it is tested on its own,
with throw-away copies of the documents."""
import contextlib
import importlib.util
import io
import pathlib
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("bump_version", HERE / "bump_version.py")
bump = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bump)

DATED = "# Changelog\n\n## [0.1.0] - 2026-12-01\n\nFirst build.\n"
UNRELEASED = "# Changelog\n\n## [0.1.0] - Unreleased\n\nFirst build.\n"
NO_DATE = "# Changelog\n\n## [0.1.0]\n\nFirst build.\n"
TABLE = "| Version | Tag |\n| --- | --- |\n"
TEMPLATE = "```\n| <version> | v<version> | in preparation |\n```\n"


class ReleaseCheck(unittest.TestCase):
    def run_check(self, changelog, release_history, expected, version="0.1.0", make_history=True):
        with tempfile.TemporaryDirectory() as d:
            d = pathlib.Path(d)
            (d / "CHANGELOG.md").write_text(changelog, encoding="utf-8")
            (d / "VERSION_HISTORY.md").write_text("# Version history\n", encoding="utf-8")
            if make_history:
                (d / "RELEASE_HISTORY.md").write_text(release_history, encoding="utf-8")
            saved = (bump.CHANGELOG, bump.VERSION_HISTORY, bump.RELEASE_HISTORY, bump.current, bump.ROOT)
            bump.CHANGELOG, bump.VERSION_HISTORY, bump.RELEASE_HISTORY = d / "CHANGELOG.md", d / "VERSION_HISTORY.md", d / "RELEASE_HISTORY.md"
            bump.current = lambda: {"app/package.json": version, "engine/Cargo.toml": version}
            bump.ROOT = d
            out = io.StringIO()
            try:
                with contextlib.redirect_stdout(out):
                    code = bump.check(expected)
            finally:
                bump.CHANGELOG, bump.VERSION_HISTORY, bump.RELEASE_HISTORY, bump.current, bump.ROOT = saved
            return code, out.getvalue()

    def test_an_ordinary_check_does_not_need_a_release_row_or_a_date(self):
        code, out = self.run_check(UNRELEASED, TABLE + TEMPLATE, None)
        self.assertEqual(code, 0, out)

    def test_a_release_with_a_dated_heading_and_a_row_passes(self):
        code, out = self.run_check(DATED, TABLE + "| 0.1.0 | v0.1.0 | in preparation |\n", "0.1.0")
        self.assertEqual(code, 0, out)

    def test_a_release_with_an_unreleased_heading_is_refused(self):
        code, out = self.run_check(UNRELEASED, TABLE + "| 0.1.0 | v0.1.0 | in preparation |\n", "0.1.0")
        self.assertEqual(code, 1)
        self.assertIn("Unreleased", out)

    def test_a_release_with_an_undated_heading_is_refused(self):
        code, out = self.run_check(NO_DATE, TABLE + "| 0.1.0 | v0.1.0 | in preparation |\n", "0.1.0")
        self.assertEqual(code, 1)
        self.assertIn("no date", out)

    def test_a_release_without_a_row_in_the_release_history_is_refused(self):
        code, out = self.run_check(DATED, TABLE, "0.1.0")
        self.assertEqual(code, 1)
        self.assertIn("no row", out)

    def test_the_template_row_is_not_a_row(self):
        code, out = self.run_check(DATED, TABLE + TEMPLATE, "0.1.0")
        self.assertEqual(code, 1, out)

    def test_a_row_for_another_version_is_not_a_row_for_this_one(self):
        code, out = self.run_check(DATED, TABLE + "| 0.0.9 | v0.0.9 | published |\n", "0.1.0")
        self.assertEqual(code, 1, out)

    def test_a_tag_that_does_not_match_the_project_is_refused(self):
        code, out = self.run_check(DATED, TABLE + "| 0.2.0 | v0.2.0 | x |\n", "0.2.0")
        self.assertEqual(code, 1)
        self.assertIn("the tag says 0.2.0", out)

    def test_a_missing_release_history_is_refused_even_for_an_ordinary_check(self):
        code, out = self.run_check(DATED, "", None, make_history=False)
        self.assertEqual(code, 1)
        self.assertIn("RELEASE_HISTORY.md is missing", out)


if __name__ == "__main__":
    sys.exit(unittest.main())
