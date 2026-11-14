#!/usr/bin/env python3
"""Checks that the documentation is in step with the repository. Run from anywhere:

    python scripts/check_docs.py            check everything, exit 1 and list the problems if any
    python scripts/check_docs.py --root DIR check another tree (the tests use this)

What it checks (the rules are in docs/standards/docs-style-guide.md):
  * every relative link and anchor in every Markdown file points at something that exists;
  * every fenced code block has a language, and every Mermaid block starts with a known diagram type and is balanced;
  * the documents that must carry the header line (Status, Audience, Last reviewed) do, and were reviewed in the last 180 days;
  * docs/README.md (the map) leads to every document in docs/;
  * docs/api/rpc.md has a section for every engine command in the code and none for a command that does not exist, and every
    command the window calls is one of them;
  * ADR files are numbered without gaps, their titles match their numbers, and docs/adr/README.md lists each one;
  * every public Rust item of the engine has a doc comment (fields and variants are left to the reference documents);
  * no filler words ("simply", "obviously", "please note") in the documents this phase wrote.
"""
import argparse
import datetime
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SKIP_DIRS = {".git", "node_modules", "target", "gen", ".claude", "__pycache__", "dist"}
MAX_AGE_DAYS = 180
MERMAID_TYPES = ("flowchart", "graph", "sequenceDiagram", "stateDiagram-v2", "stateDiagram", "erDiagram", "classDiagram", "gantt", "timeline",
                 "pie", "journey", "gitGraph", "mindmap", "quadrantChart", "C4Context", "C4Container", "requirementDiagram", "sankey-beta", "xychart-beta")
# documents that must start with the header line; older records (design documents before Phase 10, review records) are exempt
HEADER_DIRS = ("docs/architecture", "docs/adr", "docs/api", "docs/reference", "docs/guides", "docs/runbooks", "docs/product", "docs/testing",
               "docs/standards", "docs/templates")
HEADER_FILES = ("docs/README.md", "docs/troubleshooting.md", "docs/faq.md", "docs/designs/phase10.md")
FILLER = re.compile(r"\b(simply|obviously|please note)\b", re.I)
HEADER = re.compile(r"^> \*\*Status:\*\* (?P<status>[^·]+?) · .*\*\*Last reviewed:\*\* (?P<date>\d{4}-\d{2}-\d{2}|YYYY-MM-DD)\s*$")
STATUSES = {"current", "draft", "superseded", "proposed", "accepted", "rejected", "deprecated"}


def rel(path, root):
    return os.path.relpath(path, root).replace(os.sep, "/")


def md_files(root):
    out = []
    for base, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        out += [os.path.join(base, f) for f in files if f.endswith(".md")]
    return sorted(out)


def read(path):
    with open(path, encoding="utf-8-sig") as fh:
        return fh.read()


def blocks(text):
    """Yields (kind, start_line, lines) for each fenced block: kind is the info string; and the text outside blocks as lines."""
    inside, info, start, buf = False, "", 0, []
    for n, line in enumerate(text.splitlines(), 1):
        if re.match(r"^\s*(```|~~~)", line):
            if not inside:
                inside, info, start, buf = True, line.strip().lstrip("`~").strip(), n, []
            else:
                yield info, start, buf
                inside = False
            continue
        if inside:
            buf.append(line)
    if inside:
        yield info, start, buf


def prose(text):
    """The text with fenced blocks and inline code blanked out, line numbers kept."""
    out, inside = [], False
    for line in text.splitlines():
        if re.match(r"^\s*(```|~~~)", line):
            inside = not inside
            out.append("")
        elif inside:
            out.append("")
        else:
            out.append(re.sub(r"`[^`]*`", "``", line))
    return out


def slugs(text):
    """GitHub-style anchors of the headings of a document (the text of inline code stays in the heading)."""
    seen, out, inside = {}, set(), False
    for line in text.splitlines():
        if re.match(r"^\s*(```|~~~)", line):
            inside = not inside
            continue
        m = None if inside else re.match(r"^(#{1,6})\s+(.*?)\s*#*\s*$", line)
        if not m:
            continue
        h = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", m.group(2))
        h = re.sub(r"[`*]", "", h).lower()
        s = re.sub(r"[^\w\- ]", "", h).replace(" ", "-")
        n = seen.get(s, 0)
        seen[s] = n + 1
        out.add(s if n == 0 else f"{s}-{n}")
    return out


# ───────── the checks: each returns a list of problems ─────────
def check_links(root, files):
    errors = []
    cache = {}
    link = re.compile(r"!?\[(?:[^\[\]]|\[[^\]]*\])*\]\(\s*(<[^>]*>|[^)\s]+)(?:\s+(?:\"[^\"]*\"|'[^']*'|\([^)]*\)))?\s*\)")  # the text may hold an image: [![x](a.png)](b.md)
    definition = re.compile(r"^\s{0,3}\[(?!\^)[^\]]+\]:\s*(<[^>]*>|\S+)")  # a reference definition, not a footnote ([^1]: text)
    for f in files:
        if rel(f, root).startswith("docs/templates/"):
            continue  # placeholders such as NNNN-title.md
        text = read(f)
        for n, line in enumerate(prose(text), 1):
            found = [m.group(1) for m in link.finditer(line)]
            d = definition.match(line)
            if d:
                found.append(d.group(1))
            for target in found:
                target = target.strip("<>")
                if re.match(r"^(https?:|mailto:|tel:)", target):
                    continue
                path, _, anchor = target.partition("#")
                if path.startswith("/"):
                    dest = os.path.normpath(os.path.join(root, path.lstrip("/")))  # GitHub reads /docs/x.md from the repository root
                else:
                    dest = f if path == "" else os.path.normpath(os.path.join(os.path.dirname(f), path))
                if path and not os.path.exists(dest):
                    errors.append(f"{rel(f, root)}:{n}: broken link to {target} (to point at a line use file.rs#L120, which GitHub resolves)")
                    continue
                if anchor and dest.endswith(".md") and os.path.isfile(dest):
                    if dest not in cache:
                        cache[dest] = slugs(read(dest))
                    if anchor.lower() not in cache[dest]:
                        errors.append(f"{rel(f, root)}:{n}: the link {target} points at a heading that does not exist")
    return errors


def check_fences_and_mermaid(root, files):
    errors = []
    for f in files:
        text = read(f)
        open_line = None
        for n, line in enumerate(text.splitlines(), 1):  # a fence with no language
            if re.match(r"^\s*(```|~~~)", line):
                if open_line is None:
                    open_line = n
                    if line.strip().lstrip("`~").strip() == "":
                        errors.append(f"{rel(f, root)}:{n}: a code block with no language (tag it, for example ```text)")
                else:
                    open_line = None
        for info, start, lines in blocks(text):
            if info.split()[0:1] != ["mermaid"]:
                continue
            body = [l for l in lines if l.strip() and not l.strip().startswith("%%")]
            if body and body[0].strip() == "---":  # front matter
                end = next((i for i, l in enumerate(body[1:], 1) if l.strip() == "---"), 0)
                body = body[end + 1:]
            where = f"{rel(f, root)}:{start}"
            if not body:
                errors.append(f"{where}: an empty Mermaid block")
                continue
            kind = body[0].strip().split()[0]
            if kind not in MERMAID_TYPES:
                errors.append(f"{where}: a Mermaid block must start with a diagram type (flowchart, sequenceDiagram, ...), not {kind!r}")
                continue
            plain = [re.sub(r'"[^"]*"', '""', l) for l in body]
            if kind == "erDiagram":
                plain = [re.sub(r"[|o}{]{1,2}--[|o}{]{1,2}", " -- ", l) for l in plain]  # cardinality marks such as ||--o{ are not braces
            joined = "\n".join(plain)
            for o, c in ("[]", "()", "{}"):
                if joined.count(o) != joined.count(c):
                    errors.append(f"{where}: unbalanced {o}{c} in a Mermaid block (quote labels that hold punctuation)")
            if sum(1 for l in lines if l.count('"') % 2) :
                errors.append(f"{where}: a Mermaid line with an odd number of quotes")
            if kind in ("flowchart", "graph"):
                for l in plain:
                    for lab in re.finditer(r"\b[A-Za-z0-9_]+\[([^\]\"\[({/\\>][^\]\"]*)\]", l):
                        if "(" in lab.group(1) or ")" in lab.group(1):
                            errors.append(f"{where}: quote a label that holds parentheses: [\"{lab.group(1)}\"]")
            ends = sum(1 for l in plain if l.strip() == "end")
            if kind in ("flowchart", "graph"):
                opens = sum(1 for l in plain if re.match(r"^\s*subgraph\b", l))
                if opens != ends:
                    errors.append(f"{where}: {opens} subgraph(s) but {ends} end(s)")
                for l in plain:
                    if re.match(r"^\s*(end|graph)\s*(\[|\(|-->|---|$)", l) and l.strip() != "end":
                        errors.append(f"{where}: do not use the word end or graph as a node id: {l.strip()!r}")
            if kind == "sequenceDiagram":
                opens = sum(1 for l in plain if re.match(r"^\s*(alt|opt|loop|par|rect|critical|break)\b", l))
                if opens != ends:
                    errors.append(f"{where}: {opens} alt/opt/loop block(s) but {ends} end(s)")
                for l in plain:
                    if ";" in l and not l.strip().startswith("%%"):
                        errors.append(f"{where}: a semicolon inside a sequenceDiagram line breaks Mermaid: {l.strip()!r}")
    return errors


def header_required(relpath):
    if relpath in HEADER_FILES:
        return True
    if relpath.startswith("docs/designs/phase"):
        m = re.match(r"docs/designs/phase(\d+)\.md$", relpath)
        return bool(m and int(m.group(1)) >= 10)
    return any(relpath.startswith(d + "/") for d in HEADER_DIRS)


def check_headers(root, files, today=None):
    today = today or datetime.date.today()
    errors = []
    for f in files:
        r = rel(f, root)
        if not header_required(r):
            continue
        lines = read(f).splitlines()
        head = next((l for l in lines[:8] if l.startswith("> **Status:**")), None)
        if head is None:
            errors.append(f"{r}: no header line (> **Status:** ... · **Audience:** ... · **Last reviewed:** YYYY-MM-DD)")
            continue
        m = HEADER.match(head)
        if not m:
            errors.append(f"{r}: the header line is not in the form > **Status:** x · **Audience:** y · **Last reviewed:** YYYY-MM-DD")
            continue
        if r.startswith("docs/templates/"):
            continue  # a template holds placeholders, such as "proposed | accepted" and YYYY-MM-DD
        if m.group("status").split()[0] not in STATUSES:
            errors.append(f"{r}: unknown status {m.group('status')!r} (it starts with one of {', '.join(sorted(STATUSES))})")
        d = m.group("date")
        if d != "YYYY-MM-DD":
            try:
                age = (today - datetime.date.fromisoformat(d)).days
            except ValueError:
                errors.append(f"{r}: {d} is not a date")
                continue
            if age > MAX_AGE_DAYS:
                errors.append(f"{r}: last reviewed {d}, {age} days ago (the limit is {MAX_AGE_DAYS}): check it against the code and update the date")
            if age < 0:
                errors.append(f"{r}: last reviewed {d} is in the future")
    return errors


def check_map(root, files):
    """docs/README.md leads to every document under docs/, directly or through the README.md of the document's folder."""
    errors = []
    mapf = os.path.join(root, "docs", "README.md")
    if not os.path.isfile(mapf):
        return ["docs/README.md (the documentation map) does not exist"]
    link = re.compile(r"\]\(([^)\s#]+)")

    def targets(path):
        out = set()
        for line in prose(read(path)):
            for m in link.finditer(line):
                t = m.group(1)
                if not re.match(r"^(https?:|mailto:)", t):
                    out.add(os.path.normpath(os.path.join(os.path.dirname(path), t)))
        return out

    reached = targets(mapf)
    for folder_index in [p for p in list(reached) if os.path.basename(p) == "README.md" and os.path.isfile(p)]:
        reached |= targets(folder_index)
    for f in files:
        r = rel(f, root)
        if r.startswith("docs/") and f != mapf and os.path.normpath(f) not in reached:
            errors.append(f"{r}: not reachable from docs/README.md (add it to the map or to its folder's README.md)")
    return errors


def engine_commands(root):
    names = set()
    cmds = os.path.join(root, "engine", "src", "commands.rs")
    core = os.path.join(root, "engine", "src", "core.rs")
    if os.path.isfile(cmds):
        text = read(cmds)
        for const in ("PUBLIC", "DESKTOP_ONLY"):
            m = re.search(r"const %s: &\[&str\] = &\[(.*?)\];" % const, text, re.S)
            if m:
                names |= set(re.findall(r'"([a-z_]+)"', m.group(1)))
    if os.path.isfile(core):
        for body in rpc_bodies(read(core)):
            for line in body:
                m = re.match(r'^\s+("[a-z_]+"(?: \| "[a-z_]+")*) =>', line)
                if m:
                    names |= set(re.findall(r'"([a-z_]+)"', m.group(1)))
    return names


def rpc_bodies(text):
    """The lines of each function called rpc or rpc_public (the engine's command dispatchers), found by matching braces."""
    lines, out = text.splitlines(), []
    for i, line in enumerate(lines):
        if re.match(r"^\s*(pub )?fn (rpc|rpc_public)\b", line):
            depth, started, body = 0, False, []
            for l in lines[i:]:
                plain = re.sub(r"'(?:[^'\\]|\\.)'", "", re.sub(r'"(?:[^"\\]|\\.)*"', '""', l))
                depth += plain.count("{") - plain.count("}")
                started = started or "{" in plain
                body.append(l)
                if started and depth <= 0:
                    break
            out.append(body)
    return out


def window_calls(root):
    names = set()
    ui = os.path.join(root, "app", "ui")
    if os.path.isdir(ui):
        for f in sorted(os.listdir(ui)):
            if f.endswith(".js") and f != "mock.js":
                names |= set(re.findall(r'\brpc\("([a-z_]+)"', read(os.path.join(ui, f))))
                names |= set(re.findall(r'\bcall\("([a-z_]+)"', read(os.path.join(ui, f))))
    return names


def check_rpc(root):
    ref = os.path.join(root, "docs", "api", "rpc.md")
    code = engine_commands(root)
    if not code:
        return []  # no engine in this tree (a test tree without one)
    if not os.path.isfile(ref):
        return ["docs/api/rpc.md (the command reference) does not exist"]
    documented = set(re.findall(r"^###\s+`([a-z_]+)`", read(ref), re.M))
    errors = [f"docs/api/rpc.md: the command {c!r} is in the engine but has no section (### `{c}`)" for c in sorted(code - documented)]
    errors += [f"docs/api/rpc.md: documents {c!r}, which is not a command of the engine" for c in sorted(documented - code)]
    errors += [f"app/ui calls {c!r}, which is not a command of the engine" for c in sorted(window_calls(root) - code)]
    return errors


def check_adrs(root):
    folder = os.path.join(root, "docs", "adr")
    if not os.path.isdir(folder):
        return []
    errors, numbers = [], []
    index = read(os.path.join(folder, "README.md")) if os.path.isfile(os.path.join(folder, "README.md")) else ""
    for f in sorted(os.listdir(folder)):
        m = re.match(r"^(\d{4})-[a-z0-9-]+\.md$", f)
        if not m:
            continue
        n = int(m.group(1))
        numbers.append(n)
        text = read(os.path.join(folder, f))
        first = text.splitlines()[0] if text else ""
        if not re.match(r"^# ADR-%s: \S" % m.group(1), first):
            errors.append(f"docs/adr/{f}: the title must be '# ADR-{m.group(1)}: ...' (found {first!r})")
        if f not in index:
            errors.append(f"docs/adr/{f}: not listed in docs/adr/README.md")
    if numbers != list(range(1, len(numbers) + 1)):
        errors.append(f"docs/adr: the ADR numbers must run 0001, 0002, ... without gaps or repeats (found {numbers})")
    return errors


def check_filler(root, files):
    errors = []
    for f in files:
        r = rel(f, root)
        if not (header_required(r) and not r.startswith("docs/templates/")):
            continue
        for n, line in enumerate(prose(read(f)), 1):
            m = FILLER.search(re.sub(r'"[^"]*"', '""', line))
            if m:
                errors.append(f"{r}:{n}: avoid the filler word {m.group(1)!r}")
    return errors


def check_rust_docs(root):
    """Every public fn, struct, enum, trait, const, static and type of the engine has a doc comment (fields and variants excepted)."""
    errors = []
    src = os.path.join(root, "engine", "src")
    if not os.path.isdir(src):
        return errors
    item = re.compile(r"^\s*pub (?:(?:const|async|unsafe|extern\s+\"[^\"]*\")\s+)*(fn|struct|enum|trait|const|static|type)\s+([A-Za-z_][A-Za-z0-9_]*)")
    for base, _, files in os.walk(src):
        for name in sorted(files):
            if not name.endswith(".rs"):
                continue
            path = os.path.join(base, name)
            lines = read(path).splitlines()
            for i, line in enumerate(lines):
                m = item.match(line)
                if not m:
                    continue
                j = i - 1
                while j >= 0 and lines[j].strip() != "":
                    t = lines[j].strip()
                    if t.startswith("#["):
                        j -= 1
                    elif t.endswith("]") and not t.startswith("///"):  # the end of an attribute that spans lines: go up to its first line
                        k = j
                        while k >= 0 and not lines[k].strip().startswith("#[") and j - k < 40:
                            k -= 1
                        if k < 0 or not lines[k].strip().startswith("#["):
                            break
                        j = k - 1
                    else:
                        break
                if j < 0 or not re.match(r"^\s*///(?!/)\s*\S", lines[j]):
                    errors.append(f"{rel(path, root)}:{i + 1}: public {m.group(1)} {m.group(2)} has no doc comment (///)")
    return errors


def check_hub(root):
    """The numbers and routes in engine/src/hub.rs appear in docs/api/hub-http.md."""
    hub = os.path.join(root, "engine", "src", "hub.rs")
    doc = os.path.join(root, "docs", "api", "hub-http.md")
    if not os.path.isfile(hub):
        return []
    if not os.path.isfile(doc):
        return ["docs/api/hub-http.md (the phone hub's interface) does not exist"]
    text, errors = read(hub), []
    words = {m.replace(",", "") for m in re.findall(r"\d[\d,]*", read(doc))}
    for name, expr in re.findall(r"^const ([A-Z_]+): (?:usize|u32|u64|Duration) = (.+?);", text, re.M):
        nums = re.findall(r"\d+", expr)
        if not nums or name == "CSP":
            continue
        value = 1
        for n in nums:
            value *= int(n)
        if "from_secs" in expr:
            value = int(nums[0])
        accepted = {str(value), str(value // 1024) if value % 1024 == 0 else ""}
        if not accepted & words:
            errors.append(f"docs/api/hub-http.md: the limit {name} = {value} in engine/src/hub.rs is not in the document")
    files = re.search(r"const FILES:.*?\n\];", text, re.S)
    for path in re.findall(r'^\s+"(/[^"]*)",\s*$', files.group(0) if files else "", re.M):
        if path not in read(doc):
            errors.append(f"docs/api/hub-http.md: the route {path} served by engine/src/hub.rs is not in the document")
    return errors


def check_counts(root):
    """The README says how many tests there are; docs/TESTCASES.md (generated) says how many there really are."""
    cat, readme = os.path.join(root, "docs", "TESTCASES.md"), os.path.join(root, "README.md")
    if not (os.path.isfile(cat) and os.path.isfile(readme)):
        return []
    c, r = read(cat), read(readme)
    errors = []
    for pat, label, want in ((r"\*\*(\d+) automated tests\*\*", "engine", r"engine \((\d+) tests"), (r"\*\*(\d+) window tests\*\*", "window", r"the window \((\d+) tests")):
        real, said = re.search(pat, c), re.search(want, r)
        if real and (not said or said.group(1) != real.group(1)):
            errors.append(f"README.md: says {said.group(1) if said else 'no number of'} {label} tests, docs/TESTCASES.md counts {real.group(1)}")
    return errors


def stale_warnings(root, today=None):
    """Documents that will fail the review-date rule within 30 days (printed, not counted as problems)."""
    today = today or datetime.date.today()
    out = []
    for f in md_files(root):
        r = rel(f, root)
        if not header_required(r) or r.startswith("docs/templates/"):
            continue
        m = next((HEADER.match(l) for l in read(f).splitlines()[:8] if HEADER.match(l)), None)
        if m and m.group("date") != "YYYY-MM-DD":
            try:
                age = (today - datetime.date.fromisoformat(m.group("date"))).days
            except ValueError:
                continue
            if MAX_AGE_DAYS - 30 < age <= MAX_AGE_DAYS:
                out.append(f"{r}: last reviewed {age} days ago; it fails the check in {MAX_AGE_DAYS - age} days")
    return out


def run(root=ROOT, today=None):
    files = md_files(root)
    errors = []
    errors += check_links(root, files)
    errors += check_fences_and_mermaid(root, files)
    errors += check_headers(root, files, today)
    errors += check_map(root, files)
    errors += check_rpc(root)
    errors += check_adrs(root)
    errors += check_filler(root, files)
    errors += check_rust_docs(root)
    errors += check_hub(root)
    errors += check_counts(root)
    return errors


def main(argv=None):
    ap = argparse.ArgumentParser(description="Check the documentation against the repository.")
    ap.add_argument("--root", default=ROOT)
    args = ap.parse_args(argv)
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")  # a problem text can hold an arrow or a dash; a Windows pipe is not UTF-8 by default
    errors = run(args.root)
    for w in stale_warnings(args.root):
        print("warning: " + w)
    if errors:
        print(f"{len(errors)} documentation problem(s):")
        for e in errors:
            print("  " + e)
        return 1
    print(f"OK: {len(md_files(args.root))} Markdown files checked (links, diagrams, headers, the map, the command reference, ADRs, Rust doc comments)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
