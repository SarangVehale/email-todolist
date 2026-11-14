"""Lists the licence of every Rust crate in the app, with the licence texts, in THIRD_PARTY_LICENSES.txt, and refuses a licence nobody has reviewed.

    python scripts/gen_licenses.py            # writes THIRD_PARTY_LICENSES.txt
    python scripts/gen_licenses.py --check    # fails if the file is out of date or a crate has a licence that is not on the reviewed list

MIT, Apache-2.0 and the like ask that their text and copyright line travel with a program that contains the code, so the installer carries the
file this writes (`bundle.resources` in app/src-tauri/tauri.conf.json). It reads the packages `cargo metadata` names for the Tauri app (the
engine and everything it uses, for every platform, so it is a little more than any one installer holds) and, for each, the licence files in the
crate's own source folder, which needs the crates downloaded (`cargo fetch --locked` in app/src-tauri). The result does not depend on the
machine it was made on.

A licence is accepted when its SPDX expression can be satisfied with licences on ALLOWED: for `A OR B` one of them, for `A AND B` both.
`MIT OR Apache-2.0 OR LGPL-2.1-or-later` is fine (we take MIT); a crate that is only GPL, AGPL or LGPL is not. A new licence is added to
ALLOWED only after reading it, with the reason in the comment next to it."""
import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TAURI = os.path.join(ROOT, "app", "src-tauri")
OUT = os.path.join(ROOT, "THIRD_PARTY_LICENSES.txt")

ALLOWED = {
    "MIT", "MIT-0", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "0BSD", "Unlicense", "CC0-1.0", "BSL-1.0",
    "Unicode-3.0",        # the Unicode data tables of the icu crates: permissive, attribution
    "CDLA-Permissive-2.0",  # webpki-roots: the list of trusted certificate authorities, as data
    "MPL-2.0",            # file-level copyleft: cssparser, cssparser-macros, selectors (Servo), dtoa-short and option-ext, used unmodified through Tauri; their source is on crates.io
}


def _tokens(expr):
    return re.findall(r"\(|\)|[^\s()]+", expr.replace("/", " OR "))


def acceptable(expr):
    """True when the SPDX expression can be met using only licences in ALLOWED."""
    toks = _tokens(expr)
    pos = 0

    def parse_or():
        nonlocal pos
        ok = parse_and()
        while pos < len(toks) and toks[pos].upper() == "OR":
            pos += 1
            right = parse_and()
            ok = ok or right
        return ok

    def parse_and():
        nonlocal pos
        ok = parse_atom()
        while pos < len(toks) and toks[pos].upper() == "AND":
            pos += 1
            right = parse_atom()
            ok = ok and right
        return ok

    def parse_atom():
        nonlocal pos
        if pos >= len(toks):
            raise ValueError("an operator with nothing after it")
        t = toks[pos]
        pos += 1
        if t == "(":
            ok = parse_or()
            if pos >= len(toks) or toks[pos] != ")":
                return False  # an unclosed bracket is not a licence expression
            pos += 1
            return ok
        ident = t
        if pos < len(toks) and toks[pos].upper() == "WITH":  # `Apache-2.0 WITH LLVM-exception`: the exception only widens the licence
            pos += 2
        return ident in ALLOWED

    try:
        return bool(toks) and parse_or() and pos == len(toks)
    except ValueError:
        return False


LICENCE_FILE = re.compile(r"^(LICEN[SC]E|COPYING|UNLICENSE|NOTICE)([-_. ].*)?$", re.IGNORECASE)


def packages():
    meta = json.loads(subprocess.run(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=TAURI, capture_output=True, text=True,
                                     encoding="utf-8", check=True).stdout)
    return sorted((p for p in meta["packages"] if p["source"] is not None), key=lambda p: (p["name"], p["version"]))


def texts_of(pkg):
    folder = os.path.dirname(pkg["manifest_path"])
    out = []
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if os.path.isfile(path) and LICENCE_FILE.match(name):
            body = open(path, encoding="utf-8", errors="replace", newline="").read().replace("\r\n", "\n").replace("\r", "\n")
            out.append((name, "\n".join(line.rstrip() for line in body.strip("\n").split("\n"))))
    return out


def build():
    pkgs = packages()
    bad = [f"{p['name']} {p['version']}: {p.get('license') or '(no licence field)'}" for p in pkgs if not acceptable(p.get("license") or "")]
    if bad:
        raise SystemExit("These crates have a licence that is not on the reviewed list (read it, then add it to ALLOWED in this script with the reason):\n  "
                         + "\n  ".join(bad))
    groups, unshipped = {}, []
    for p in pkgs:
        files = texts_of(p)
        if not files:
            unshipped.append(p)
        for _, text in files:
            groups.setdefault(hashlib.sha256(text.encode("utf-8")).hexdigest(), (text, []))[1].append(f"{p['name']} {p['version']}")
    lines = [
        "THIRD-PARTY LICENCES",
        "",
        "Inbox Agent contains the Rust crates below. Each is under the licence shown, and the texts that travel with it follow.",
        "The program itself is under the MIT licence (LICENSE). The bundled fonts are under the SIL Open Font License; see THIRD_PARTY_NOTICES.md.",
        "Made by scripts/gen_licenses.py from the crates' own source folders; do not edit by hand.",
        "",
        f"{len(pkgs)} crates.",
        "",
        "=" * 78,
        "CRATES AND THEIR LICENCES",
        "=" * 78,
        "",
    ]
    for p in pkgs:
        lines.append(f"{p['name']} {p['version']}: {p['license']}" + (f"  ({p['repository']})" if p.get("repository") else ""))
    if unshipped:
        lines += ["", "The package of these crates holds no licence file; the licence above is what the crate declares:"]
        lines += [f"  {p['name']} {p['version']}" for p in unshipped]
    for text, names in sorted(groups.values(), key=lambda g: (g[1][0], g[0])):
        lines += ["", "=" * 78, "THE FOLLOWING CRATES COME WITH THIS TEXT:", "  " + ", ".join(names), "=" * 78, "", text]
    return "\n".join(lines) + "\n"


def main():
    text = build()
    if "--check" in sys.argv:
        have = open(OUT, encoding="utf-8", newline="").read().replace("\r\n", "\n") if os.path.exists(OUT) else ""
        if have != text:
            raise SystemExit("THIRD_PARTY_LICENSES.txt is out of date: run python scripts/gen_licenses.py and commit it")
        print(f"OK: THIRD_PARTY_LICENSES.txt is current and every licence is on the reviewed list ({text.count(chr(10))} lines)")
        return
    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)
    print(f"wrote THIRD_PARTY_LICENSES.txt ({len(text) // 1024} KB)")


if __name__ == "__main__":
    main()
