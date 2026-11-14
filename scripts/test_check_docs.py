#!/usr/bin/env python3
"""Tests for scripts/check_docs.py: each rule must fail when it is broken and stay quiet when it is not.
Run: python scripts/test_check_docs.py"""
import datetime
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import check_docs as cd  # noqa: E402

TODAY = datetime.date(2026, 11, 14)
HEAD = "> **Status:** current · **Audience:** tests · **Last reviewed:** 2026-11-01\n"
GOOD_MAP = "# Map\n\n" + HEAD + "\nSee [a](guides/a.md).\n"


def tree(files):
    """Writes {relative path: text} into a new temporary folder and returns it."""
    root = tempfile.mkdtemp(prefix="check-docs-test-")
    for path, text in files.items():
        full = os.path.join(root, *path.split("/"))
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w", encoding="utf-8", newline="") as fh:
            fh.write(text)
    return root


def doc(body, head=HEAD):
    return "# Title\n\n" + head + "\n" + body + "\n"


def problems(files, today=TODAY):
    return cd.run(tree(files), today)


class CheckDocs(unittest.TestCase):
    def test_a_clean_tree_passes(self):
        self.assertEqual(problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("Text.")}), [])

    def test_a_broken_link_fails(self):
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("See [x](missing.md).")})
        self.assertTrue(any("broken link to missing.md" in e for e in out), out)

    def test_a_broken_anchor_fails_and_a_good_one_passes(self):
        files = {"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("## Real heading\n\nSee [x](#nothing-here) and [y](#real-heading).")}
        out = problems(files)
        self.assertEqual(len([e for e in out if "heading that does not exist" in e]), 1, out)

    def test_a_link_to_a_source_line_needs_the_form_github_resolves(self):
        files = {"docs/README.md": GOOD_MAP, "engine/src/x.rs": "// x\n", "docs/guides/a.md": doc("See [x](../../engine/src/x.rs#L12).")}
        self.assertEqual(problems(files), [])
        files["docs/guides/a.md"] = doc("See [x](../../engine/src/x.rs:12).")
        self.assertTrue(any("broken link" in e for e in problems(files)))

    def test_a_heading_that_holds_a_link_or_code_has_the_anchor_github_makes(self):
        files = {"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("## [The link](x.md) and `set_done`\n\nSee [here](#the-link-and-set_done).")}
        self.assertEqual([e for e in problems(files) if "heading" in e], [])

    def test_an_impossible_date_is_reported_not_a_crash(self):
        bad = "> **Status:** current · **Audience:** tests · **Last reviewed:** 2026-13-45\n"
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("Text.", bad)})
        self.assertTrue(any("is not a date" in e for e in out), out)

    def test_a_review_date_close_to_the_limit_gives_a_warning_only(self):
        near = "> **Status:** current · **Audience:** tests · **Last reviewed:** 2026-06-01\n"
        root = tree({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("Text.", near)})
        self.assertEqual(cd.run(root, TODAY), [])
        self.assertTrue(any("docs/guides/a.md" in w and "fails the check in" in w for w in cd.stale_warnings(root, TODAY)))

    def test_mermaid_unquoted_parentheses_in_a_flowchart_label_fail(self):
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("```mermaid\nflowchart TD\n    A[Window (Tauri)] --> B\n```")})
        self.assertTrue(any("quote a label" in e for e in out), out)
        ok = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc('```mermaid\nflowchart TD\n    A["Window (Tauri)"] --> B[("Database")]\n```')})
        self.assertEqual(ok, [])

    def test_a_doc_comment_above_a_multi_line_attribute_counts(self):
        src = "/// Documented.\n#[derive(\n    Debug,\n    Clone,\n)]\npub struct A;\n"
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("x"), "engine/src/x.rs": src})
        self.assertEqual([e for e in out if "no doc comment" in e], [])

    def test_the_hub_numbers_and_routes_must_be_in_the_hub_document(self):
        hub = 'const MAX_BODY: usize = 64 * 1024;\nconst MAX_CONNECTIONS: usize = 32;\nconst IO_TIMEOUT: Duration = Duration::from_secs(5);\nconst FILES: &[(&str, &str, &[u8])] = &[\n    (\n        "/main.js",\n        "text/javascript",\n        b"x",\n    ),\n];\n'
        base = {"docs/README.md": GOOD_MAP + "[h](api/hub-http.md)\n", "docs/guides/a.md": doc("x"), "engine/src/hub.rs": hub}
        good = problems({**base, "docs/api/hub-http.md": doc("Limits: body 64 KB, 32 connections, 5 s. Files: `/main.js`.")})
        self.assertEqual([e for e in good if "hub" in e], [])
        bad = problems({**base, "docs/api/hub-http.md": doc("Limits: 5 s. Files: none.")})
        self.assertTrue(any("MAX_CONNECTIONS" in e for e in bad), bad)
        self.assertTrue(any("MAX_BODY" in e for e in bad), bad)
        self.assertTrue(any("/main.js" in e for e in bad), bad)

    def test_the_readme_must_say_how_many_tests_the_catalogue_counts(self):
        cat = "# Automated test catalogue\n**460 automated tests** across 20 files.\n**49 window tests** across 4 files.\n"
        readme = "Built and tested: engine (459 tests, including x), the window (49 tests in a real browser), UI.\n"
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("x"), "docs/TESTCASES.md": cat, "README.md": readme})
        self.assertEqual([e for e in out if "says" in e], ["README.md: says 459 engine tests, docs/TESTCASES.md counts 460"])

    def test_a_link_inside_a_code_block_is_not_checked(self):
        files = {"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("```text\n[x](missing.md)\n```")}
        self.assertEqual(problems(files), [])

    def test_a_code_block_without_a_language_fails(self):
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("```\nplain\n```")})
        self.assertTrue(any("no language" in e for e in out), out)

    def test_mermaid_must_start_with_a_diagram_type(self):
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("```mermaid\nA --> B\n```")})
        self.assertTrue(any("must start with a diagram type" in e for e in out), out)

    def test_mermaid_unbalanced_brackets_fail(self):
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("```mermaid\nflowchart TD\n    A[open --> B\n```")})
        self.assertTrue(any("unbalanced" in e for e in out), out)

    def test_mermaid_brackets_inside_quotes_are_fine(self):
        body = '```mermaid\nflowchart TD\n    A["IMAP: INBOX (all)"] --> B\n```'
        self.assertEqual(problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc(body)}), [])

    def test_mermaid_er_cardinality_marks_are_not_braces(self):
        body = "```mermaid\nerDiagram\n    TASK ||--o{ DRAFT : has\n    TASK {\n        int id\n    }\n```"
        self.assertEqual(problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc(body)}), [])

    def test_mermaid_subgraph_without_end_fails(self):
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("```mermaid\nflowchart TD\n    subgraph One\n    A --> B\n```")})
        self.assertTrue(any("subgraph" in e for e in out), out)

    def test_mermaid_semicolon_in_a_sequence_diagram_fails(self):
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("```mermaid\nsequenceDiagram\n    A->>B: one; two\n```")})
        self.assertTrue(any("semicolon" in e for e in out), out)

    def test_a_missing_header_fails(self):
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": "# Title\n\nNo header here.\n"})
        self.assertTrue(any("no header line" in e for e in out), out)

    def test_a_stale_review_date_fails(self):
        old = "> **Status:** current · **Audience:** tests · **Last reviewed:** 2026-01-01\n"
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("Text.", old)})
        self.assertTrue(any("days ago" in e for e in out), out)

    def test_an_unknown_status_fails(self):
        bad = "> **Status:** maybe · **Audience:** tests · **Last reviewed:** 2026-11-01\n"
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("Text.", bad)})
        self.assertTrue(any("unknown status" in e for e in out), out)

    def test_a_template_may_hold_placeholders(self):
        tpl = "> **Status:** proposed | accepted · **Audience:** x · **Last reviewed:** YYYY-MM-DD\n"
        files = {"docs/README.md": GOOD_MAP + "[t](templates/README.md)\n", "docs/guides/a.md": doc("Text."),
                 "docs/templates/README.md": doc("[a](adr.md)"), "docs/templates/adr.md": doc("See [x](NNNN-title.md).", tpl)}
        self.assertEqual(problems(files), [])

    def test_a_document_missing_from_the_map_fails(self):
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("Text."), "docs/guides/b.md": doc("Text.")})
        self.assertTrue(any("docs/guides/b.md: not reachable" in e for e in out), out)

    def test_a_document_reached_through_a_folder_index_passes(self):
        files = {"docs/README.md": GOOD_MAP + "[i](runbooks/README.md)\n", "docs/guides/a.md": doc("Text."),
                 "docs/runbooks/README.md": doc("[r](r.md)"), "docs/runbooks/r.md": doc("Text.")}
        self.assertEqual(problems(files), [])

    def _engine(self):
        return {"engine/src/commands.rs": '/// Commands a phone may use.\npub const PUBLIC: &[&str] = &[\n    "set_done",\n];\nconst DESKTOP_ONLY: &[&str] = &[\n    "search",\n];\n',
                "engine/src/core.rs": '/// Runs a command.\npub fn rpc(&self, cmd: &str) {\n    match cmd {\n        "state" => 1,\n        "sign_out" => 2,\n    }\n}\nfn other(s: &str) {\n    match s {\n        "idle" => 0,\n    }\n}\n',
                "app/ui/main.js": 'rpc("state"); call("set_done", {});'}

    def _rpc(self, names):
        return doc("\n".join(f"### `{n}`\n\nText." for n in names))

    def test_every_command_needs_a_section(self):
        files = {**self._engine(), "docs/README.md": GOOD_MAP + "[a](api/rpc.md)\n", "docs/guides/a.md": doc("x"),
                 "docs/api/rpc.md": self._rpc(["state", "set_done", "search"])}
        out = problems(files)
        self.assertTrue(any("'sign_out' is in the engine but has no section" in e for e in out), out)

    def test_a_section_for_a_command_that_does_not_exist_fails(self):
        files = {**self._engine(), "docs/README.md": GOOD_MAP + "[a](api/rpc.md)\n", "docs/guides/a.md": doc("x"),
                 "docs/api/rpc.md": self._rpc(["state", "set_done", "search", "sign_out", "ghost"])}
        out = problems(files)
        self.assertTrue(any("'ghost'" in e and "not a command" in e for e in out), out)

    def test_the_window_calling_an_unknown_command_fails(self):
        files = {**self._engine(), "app/ui/main.js": 'rpc("state"); rpc("nope");', "docs/README.md": GOOD_MAP + "[a](api/rpc.md)\n",
                 "docs/guides/a.md": doc("x"), "docs/api/rpc.md": self._rpc(["state", "set_done", "search", "sign_out"])}
        out = problems(files)
        self.assertTrue(any("app/ui calls 'nope'" in e for e in out), out)

    def test_all_commands_documented_passes(self):
        files = {**self._engine(), "docs/README.md": GOOD_MAP + "[a](api/rpc.md)\n", "docs/guides/a.md": doc("x"),
                 "docs/api/rpc.md": self._rpc(["state", "set_done", "search", "sign_out"])}
        self.assertEqual(problems(files), [])

    def test_adr_numbers_must_have_no_gap(self):
        index = "| [1](0001-a.md) | [3](0003-c.md) |\n"
        files = {"docs/README.md": GOOD_MAP + "[adr](adr/README.md)\n", "docs/guides/a.md": doc("x"), "docs/adr/README.md": doc(index),
                 "docs/adr/0001-a.md": "# ADR-0001: A\n\n" + HEAD, "docs/adr/0003-c.md": "# ADR-0003: C\n\n" + HEAD}
        out = problems(files)
        self.assertTrue(any("without gaps" in e for e in out), out)

    def test_adr_title_must_match_its_number_and_the_index_must_list_it(self):
        files = {"docs/README.md": GOOD_MAP + "[adr](adr/README.md)\n", "docs/guides/a.md": doc("x"), "docs/adr/README.md": doc("nothing"),
                 "docs/adr/0001-a.md": "# ADR-0002: A\n\n" + HEAD}
        out = problems(files)
        self.assertTrue(any("title must be" in e for e in out), out)
        self.assertTrue(any("not listed in docs/adr/README.md" in e for e in out), out)

    def test_filler_words_fail_but_quoted_ones_do_not(self):
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("You can simply press the key.")})
        self.assertTrue(any("filler word" in e for e in out), out)
        self.assertEqual(problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc('Avoid the word "simply".')}), [])

    def test_a_public_rust_item_needs_a_doc_comment(self):
        src = "/// Documented.\n#[derive(Debug)]\npub struct A {\n    pub field: u8,\n}\n\npub fn bare() {}\n\n/// Fine.\npub fn ok() {}\n"
        out = problems({"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("x"), "engine/src/x.rs": src})
        self.assertEqual([e for e in out if "no doc comment" in e], ["engine/src/x.rs:7: public fn bare has no doc comment (///)"])


class CheckDocsMore(unittest.TestCase):
    def test_a_file_with_a_byte_order_mark_is_read_correctly(self):
        bom = chr(0xFEFF)
        files = {"docs/README.md": GOOD_MAP + "[adr](adr/README.md)\n", "docs/guides/a.md": doc("x"), "docs/adr/README.md": doc("[0001](0001-a.md)"),
                 "docs/adr/0001-a.md": bom + "# ADR-0001: A\n\n" + HEAD}
        self.assertEqual(problems(files), [])

    def test_a_heading_that_starts_with_an_emoji_has_the_anchor_github_makes(self):
        files = {"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("## 🚀 Launch\n\nSee [x](#-launch).")}
        self.assertEqual(problems(files), [])
        files["docs/guides/a.md"] = doc("## 🚀 Launch\n\nSee [x](#launch).")
        self.assertTrue(any("heading that does not exist" in e for e in problems(files)))

    def test_link_forms_with_titles_angle_brackets_references_and_root_paths(self):
        base = {"docs/README.md": GOOD_MAP}
        ok = doc("A [t](<../guides/a.md#title>) and [u](a.md 'a title') and [v](/docs/guides/a.md) and [w](a.md (a title)).\n\n[ref]: a.md\n")
        self.assertEqual(problems({**base, "docs/guides/a.md": ok}), [])
        for bad in ("[t](missing.md 'a title')", "[t](</docs/missing.md>)", "[t](/docs/missing.md)", "See [r][ref].\n\n[ref]: missing.md"):
            self.assertTrue(any("broken link" in e for e in problems({**base, "docs/guides/a.md": doc(bad)})), bad)

    def test_a_workflow_value_with_a_colon_and_a_space_must_be_quoted(self):
        base = {"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("x")}
        bad = "jobs:\n  t:\n    steps:\n      - name: Scan (the shell: Tauri, webview)\n        run: echo hi\n"
        out = problems({**base, ".github/workflows/ci.yml": bad})
        self.assertTrue(any("ci.yml:4" in e and "colon and a space" in e for e in out), out)
        good = ('jobs:\n  t:\n    steps:\n      - name: "Scan (the shell: Tauri, webview)"\n        run: |\n          echo "a: b"\n          x: y z: w\n'
                "      - uses: actions/checkout@abc # v4: pinned\n        env:\n          RUSTDOCFLAGS: -D warnings\n")
        self.assertEqual([e for e in problems({**base, ".github/workflows/ci.yml": good}) if "ci.yml" in e], [])

    def test_an_image_inside_a_link_and_a_footnote(self):
        base = {"docs/README.md": GOOD_MAP}
        ok = doc("A [![alt](a.md)](a.md) and a footnote[^1].\n\n[^1]: See the guide for more.\n")
        self.assertEqual(problems({**base, "docs/guides/a.md": ok}), [])
        bad = problems({**base, "docs/guides/a.md": doc("A [![alt](a.md)](nope.md).")})
        self.assertTrue(any("broken link to nope.md" in e for e in bad), bad)

    def test_doc_comment_forms(self):
        files = {"docs/README.md": GOOD_MAP, "docs/guides/a.md": doc("x")}
        for src, missing in (("//// not a doc comment\npub fn a() {}\n", True), ("///\npub fn a() {}\n", True),
                             ("/// Fine.\npub async unsafe fn a() {}\n", False), ("pub async unsafe fn a() {}\n", True),
                             ("/// Fine.\npub unsafe extern \"C\" fn a() {}\n", False), ("pub unsafe extern \"C\" fn a() {}\n", True)):
            out = [e for e in problems({**files, "engine/src/x.rs": src}) if "no doc comment" in e]
            self.assertEqual(bool(out), missing, src)


if __name__ == "__main__":
    unittest.main(verbosity=1)
