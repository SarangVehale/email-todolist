#!/usr/bin/env python3
"""One version for the whole project.

  python scripts/bump_version.py --check            all version fields agree, CHANGELOG.md has an entry for it, and the
                                                    version and release history documents exist
  python scripts/bump_version.py --check 0.2.0      ...and they all equal 0.2.0 (the release workflow passes the tag), the
                                                    changelog heading is dated (not "Unreleased") and docs/RELEASE_HISTORY.md
                                                    has a row for 0.2.0
  python scripts/bump_version.py 0.2.0              set every version field to 0.2.0
  python scripts/bump_version.py 0.2.0 --dry-run    show what would change

Fields: app/package.json, app/src-tauri/tauri.conf.json, app/src-tauri/Cargo.toml, engine/Cargo.toml and the matching
entries in both Cargo.lock files. After bumping, add a CHANGELOG.md heading, run the tests and commit.
"""
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SEMVER = re.compile(r"^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$")

PKG_JSON = ROOT / "app" / "package.json"
TAURI_CONF = ROOT / "app" / "src-tauri" / "tauri.conf.json"
CARGO_TOMLS = {"app/src-tauri/Cargo.toml": ROOT / "app" / "src-tauri" / "Cargo.toml", "engine/Cargo.toml": ROOT / "engine" / "Cargo.toml"}
LOCKS = {
    "app/src-tauri/Cargo.lock": (ROOT / "app" / "src-tauri" / "Cargo.lock", ["app", "engine"]),
    "engine/Cargo.lock": (ROOT / "engine" / "Cargo.lock", ["engine"]),
}
CHANGELOG = ROOT / "CHANGELOG.md"
VERSION_HISTORY = ROOT / "docs" / "VERSION_HISTORY.md"
RELEASE_HISTORY = ROOT / "docs" / "RELEASE_HISTORY.md"


def read(p):
    # newline="" keeps CRLF/LF exactly as they are on disk, so a bump never rewrites line endings
    with open(p, encoding="utf-8", newline="") as f:
        return f.read()


def toml_version(text):
    m = re.search(r'(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"', text)
    return m.group(1) if m else None


def lock_version(text, name):
    m = re.search(r'\[\[package\]\]\r?\nname = "%s"\r?\nversion = "([^"]+)"' % re.escape(name), text)
    return m.group(1) if m else None


def current():
    found = {
        "app/package.json": json.loads(read(PKG_JSON)).get("version"),
        "app/src-tauri/tauri.conf.json": json.loads(read(TAURI_CONF)).get("version"),
    }
    for label, p in CARGO_TOMLS.items():
        found[label] = toml_version(read(p))
    for label, (p, names) in LOCKS.items():
        if p.exists():
            for n in names:
                found[f"{label} [{n}]"] = lock_version(read(p), n)
    return found


def check(expected=None):
    found = current()
    versions = set(found.values())
    problems = []
    if None in versions:
        problems += [f"could not read a version from {k}" for k, v in found.items() if v is None]
    if len(versions - {None}) > 1:
        problems.append("versions disagree:\n" + "\n".join(f"  {k}: {v}" for k, v in found.items()))
    version = next(iter(versions - {None}), None)
    if expected and version != expected:
        problems.append(f"the tag says {expected} but the project says {version}")
    if version and not SEMVER.match(version):
        problems.append(f"{version!r} is not a semantic version")
    if version:
        if not CHANGELOG.exists() or not re.search(r"(?m)^## \[?%s\]?" % re.escape(version), read(CHANGELOG)):
            problems.append(f"CHANGELOG.md has no '## [{version}]' heading")
    for doc in (VERSION_HISTORY, RELEASE_HISTORY):
        if not doc.exists():
            problems.append(f"{doc.relative_to(ROOT).as_posix()} is missing")
    if version and expected and CHANGELOG.exists():
        heading = re.search(r"(?m)^## \[?%s\]?.*$" % re.escape(version), read(CHANGELOG))
        if heading and "unreleased" in heading.group(0).lower():
            problems.append(f"the CHANGELOG.md heading for {version} still says Unreleased: give it the release date")
        elif heading and not re.search(r"\d{4}-\d{2}-\d{2}", heading.group(0)):
            problems.append(f"the CHANGELOG.md heading for {version} has no date: write it as '## [{version}] - YYYY-MM-DD'")
    if version and expected and RELEASE_HISTORY.exists():
        if not re.search(r"(?m)^\|\s*%s\s*\|" % re.escape(version), read(RELEASE_HISTORY)):
            problems.append(f"docs/RELEASE_HISTORY.md has no row for {version} (add it when preparing the release)")
    if problems:
        print("\n".join("FAIL " + p for p in problems))
        return 1
    print(f"OK: every version field is {version}, CHANGELOG.md has an entry for it, and the version and release history exist")
    return 0


def bump(new, dry):
    if not SEMVER.match(new):
        print(f"FAIL {new!r} is not a semantic version like 1.2.3 or 1.2.3-beta.1")
        return 1
    edits = []

    def edit(path, fn):
        old = read(path)
        out = fn(old)
        if out != old:
            edits.append(path)
            if not dry:
                with open(path, "w", encoding="utf-8", newline="") as f:
                    f.write(out)

    edit(PKG_JSON, lambda t: re.sub(r'("version"\s*:\s*")[^"]+(")', r"\g<1>%s\g<2>" % new, t, count=1))
    edit(TAURI_CONF, lambda t: re.sub(r'("version"\s*:\s*")[^"]+(")', r"\g<1>%s\g<2>" % new, t, count=1))
    for p in CARGO_TOMLS.values():
        edit(p, lambda t: re.sub(r'(?ms)(^\[package\].*?^version\s*=\s*")[^"]+(")', r"\g<1>%s\g<2>" % new, t, count=1))
    for p, names in LOCKS.values():
        if p.exists():
            def fix(t, names=names):
                for n in names:
                    t = re.sub(r'(\[\[package\]\]\r?\nname = "%s"\r?\nversion = ")[^"]+(")' % re.escape(n), r"\g<1>%s\g<2>" % new, t, count=1)
                return t
            edit(p, fix)
    print(("Would change" if dry else "Changed") + f" {len(edits)} file(s):")
    for p in edits:
        print("  " + str(p.relative_to(ROOT)))
    if not dry:
        print(f"\nNext: add a '## [{new}]' section to CHANGELOG.md, run the tests, commit, then tag v{new}.")
    return 0


def main(argv):
    args = [a for a in argv if not a.startswith("--")]
    if "--check" in argv:
        return check(args[0] if args else None)
    if len(args) != 1:
        print(__doc__)
        return 2
    return bump(args[0], "--dry-run" in argv)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
