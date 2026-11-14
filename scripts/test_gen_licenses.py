#!/usr/bin/env python3
"""Tests for the licence rule in gen_licenses.py: python scripts/test_gen_licenses.py

The rule is what keeps a copyleft crate from slipping into an installer unnoticed, so it is tested on its own, on licence expressions only
(no cargo, no crates needed)."""
import importlib.util
import pathlib
import unittest

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("gen_licenses", HERE / "gen_licenses.py")
gen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gen)


class Acceptable(unittest.TestCase):
    def test_the_usual_permissive_licences_pass(self):
        for e in ["MIT", "Apache-2.0", "MIT OR Apache-2.0", "Apache-2.0 OR MIT", "BSD-3-Clause", "ISC", "Zlib", "0BSD", "Unlicense", "CC0-1.0",
                  "Unicode-3.0", "BSL-1.0", "MPL-2.0", "CDLA-Permissive-2.0"]:
            self.assertTrue(gen.acceptable(e), e)

    def test_the_old_slash_spelling_means_or(self):
        for e in ["MIT/Apache-2.0", "Apache-2.0/MIT", "Apache-2.0 / MIT", "Unlicense/MIT"]:
            self.assertTrue(gen.acceptable(e), e)

    def test_an_exception_does_not_make_a_licence_unacceptable(self):
        self.assertTrue(gen.acceptable("Apache-2.0 WITH LLVM-exception"))
        self.assertTrue(gen.acceptable("Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT"))

    def test_a_choice_with_a_permissive_side_passes(self):
        self.assertTrue(gen.acceptable("MIT OR Apache-2.0 OR LGPL-2.1-or-later"))
        self.assertTrue(gen.acceptable("GPL-3.0-only OR MIT"))

    def test_and_needs_both_sides(self):
        self.assertTrue(gen.acceptable("(Apache-2.0 OR MIT) AND BSD-3-Clause"))
        self.assertTrue(gen.acceptable("Apache-2.0 AND ISC"))
        self.assertFalse(gen.acceptable("MIT AND GPL-2.0-only"))
        self.assertFalse(gen.acceptable("(MIT OR Apache-2.0) AND AGPL-3.0-only"))

    def test_copyleft_and_unknown_licences_fail(self):
        for e in ["GPL-3.0-only", "GPL-2.0-or-later", "LGPL-2.1-or-later", "AGPL-3.0", "SSPL-1.0", "Proprietary", "CC-BY-NC-4.0", "EUPL-1.2"]:
            self.assertFalse(gen.acceptable(e), e)

    def test_a_missing_or_broken_expression_fails(self):
        for e in ["", "   ", "(MIT", "MIT OR", "OR MIT", "MIT Apache-2.0", "MIT)"]:
            self.assertFalse(gen.acceptable(e), repr(e))


if __name__ == "__main__":
    unittest.main()
