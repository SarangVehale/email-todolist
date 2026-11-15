"""The packaged app, checked as a user meets it (Windows, needs a release build).

    python scripts/packaged_check/check_app.py

Starts the built app (app/src-tauri/target/release/app.exe) with a data folder of its own making and seeded settings, then drives
it two ways: through the phone hub's HTTP interface (what a phone or a hostile neighbour on the Wi-Fi can do) and through the real
window's own IPC with the browser protocol on port 9222 (desktop_checks.mjs: the commands the hub must refuse). The app runs with its
keychain in memory (INBOX_AGENT_KEYCHAIN=memory), so the setup checks' fake password never reaches the real credential store and a real
mailbox password is neither read nor overwritten.

It REFUSES TO RUN if the app already has a data folder (%LOCALAPPDATA%\\app.inboxagent.desktop): it deletes that folder afterwards,
and that may be your real mail. Close the app, move that folder away, run this, and move it back."""
import json, os, secrets, sqlite3, string, subprocess, sys, tempfile, time, urllib.request, urllib.error, shutil

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
EXE = os.path.join(ROOT, "app", "src-tauri", "target", "release", "app.exe")
DATA = os.path.join(os.environ["LOCALAPPDATA"], "app.inboxagent.desktop")
PORT = 18787
# the hub listens on the network while the check runs: a code made for this run, not one that is public in the repository
TOKEN = "".join(secrets.choice(string.ascii_letters + string.digits) for _ in range(32))
BASE = f"http://127.0.0.1:{PORT}"
results = []


def check(name, cond, detail=""):
    results.append((name, bool(cond)))
    print(("PASS " if cond else "FAIL ") + name + (f"  [{detail}]" if detail and not cond else ""))


def req(path, body=None, token=TOKEN, method=None):
    data = json.dumps(body).encode() if body is not None else None
    r = urllib.request.Request(BASE + path, data=data, method=method or ("POST" if data else "GET"))
    if token:
        r.add_header("Authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(r, timeout=10) as resp:
            return resp.status, dict(resp.headers), resp.read()
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), e.read()


def rpc(cmd, args=None, token=TOKEN):
    st, _, b = req("/api/rpc", {"cmd": cmd, "args": args or {}}, token)
    return st, json.loads(b or b"{}")


if os.path.exists(DATA):
    sys.exit(f"{DATA} exists: this check would delete it. Close the app and move that folder away first.")
if not os.path.exists(EXE):
    sys.exit(f"{EXE} is missing: build the app first (cd app && npm run tauri build).")
os.makedirs(DATA)
json.dump({"onboarded": False, "account": {"host": "imap.example.com", "user": "me@example.com", "port": 993}, "hub": {"enabled": True, "port": PORT, "token": TOKEN}}, open(os.path.join(DATA, "settings.json"), "w"))
env = dict(os.environ, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222", INBOX_AGENT_KEYCHAIN="memory")
proc = subprocess.Popen([EXE, "--minimized"], env=env)
try:
    up = False
    for _ in range(60):
        try:
            if req("/manifest.json", token=None)[0] == 200:
                up = True
                break
        except Exception:
            time.sleep(0.5)
    check("packaged app starts and its hub answers", up)
    if not up:
        sys.exit(1)

    st, hdr, body = req("/", token=None)
    check("hub serves the UI with hardening headers", st == 200 and "frame-ancestors 'none'" in hdr.get("Content-Security-Policy", "") and hdr.get("X-Frame-Options") == "DENY")
    check("CSP forbids inline scripts", "script-src 'self'" in hdr.get("Content-Security-Policy", "") and "script-src 'self' 'unsafe-inline'" not in hdr.get("Content-Security-Policy", ""))
    for f in ["style.css", "main.js", "drafts.js", "setup.js", "boot.js", "manifest.json", "icon.svg",
              "fonts/bodoni-moda-latin-wght-normal.woff2", "fonts/ibm-plex-mono-latin-400-normal.woff2", "fonts/space-grotesk-latin-wght-normal.woff2"]:
        check("serves /" + f, req("/" + f, token=None)[0] == 200)
    # the demo backend answers "Connected" to any password: serving it would be a credential form on the network
    for f in ["mock.js", "demo.html"]:
        check("refuses to serve /" + f + " (demo backend)", req("/" + f, token=None)[0] == 404)
    page = req("/index.html", token=None)[2].decode("utf-8", "replace")
    check("the served page does not load the demo backend", "mock.js" not in page)

    st, j = rpc("state")
    check("state works with the right token", st == 200 and j.get("ok") and j["data"]["tasks"] == [])
    check("bad token is refused", rpc("state", token="wrong")[0] == 401)
    st, j = rpc("state")
    check("hub state hides settings details", set(j["data"]["settings"].keys()) == {"onboarded", "writeback"})
    check("hub state never carries the rules", "rules" not in j["data"], list(j["data"].keys()))
    check("hub state never carries the mailboxes", "accounts" not in j["data"] and "more_accounts" not in json.dumps(j["data"]), list(j["data"].keys()))

    # seed tasks into the real database the engine created
    db = sqlite3.connect(os.path.join(DATA, "agent.db"))
    cols = [r[1] for r in db.execute("PRAGMA table_info(tasks)")]
    uv = db.execute("PRAGMA user_version").fetchone()[0]
    check("the database is at format 2 with its search index", uv == 2 and db.execute("SELECT 1 FROM sqlite_master WHERE name='tasks_fts'").fetchone() is not None, uv)
    check("the follow-up table knows its mailbox", "acct" in [r[1] for r in db.execute("PRAGMA table_info(followups)")])
    check("database has the new columns", all(c in cols for c in ["body", "needs_reply", "due", "snooze_until", "reply_to", "refs"]), cols)
    now = time.time()
    rows = [
        (1, "<a1@x>", "Priya Nair", "priya@acme.com", "Q4 contract?", now - 7200, 5, "work", "Sign the contract", 1, "Can you sign today?", "reply", '{"known":true,"reply":true,"bulk":false}',
         "Hi Priya,\nCan you sign the Q4 contract today?\nThanks", 1, "2026-11-13", "priya@acme.com", "<r0@x>"),
        (2, "<a2@x>", "Shop", "deals@shop.com", "50% off sale", now - 3600, 1, "promo", "", 0, "buy now", "bulk", '{"known":false,"reply":false,"bulk":true}', "buy now", 0, "", "deals@shop.com", ""),
        (3, "<a3@x>", "Shop", "deals@shop.com", "More deals", now - 1800, 1, "promo", "", 0, "buy more", "bulk", '{"known":false,"reply":false,"bulk":true}', "buy more", 0, "", "deals@shop.com", ""),
    ]
    for r in rows:
        db.execute("INSERT INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,excerpt,why,signals,body,needs_reply,due,reply_to,refs) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", r)
    db.execute("UPDATE tasks SET event=? WHERE uid=3", (json.dumps({"summary": "Shop demo: open day", "start": "20991120T093000Z", "end": "20991120T100000Z", "tzid": "", "location": "Main street", "all_day": False, "cancelled": True}),))
    db.commit()
    db.close()

    st, j = rpc("state")
    t1 = next(t for t in j["data"]["tasks"] if t["uid"] == 1)
    check("tasks carry reply, due and draft fields", t1["needs_reply"] is True and t1["due"] == "2026-11-13" and t1["draft"] is None and t1["reply_to"] == "priya@acme.com")

    check("tasks carry the kind field (follow-ups)", t1.get("kind") == "", t1.get("kind"))
    check("tasks carry their mailbox number", t1.get("mailbox") == 0, t1.get("mailbox"))
    check("tasks carry reminder, repeat and event fields", t1.get("remind") == "" and t1.get("repeat") == "" and t1.get("event") is None, {k: t1.get(k) for k in ("remind", "repeat", "event")})
    check("tasks carry thread, closed_by, gone and learned fields", all(k in t1 for k in ("thread", "closed_by", "gone", "learned")) and t1["thread"] != "" and t1["gone"] is False, {k: t1.get(k) for k in ("thread", "closed_by", "gone", "learned")})
    st, j = rpc("set_due", {"uid": 1, "due": "2026-12-24"})
    check("a due date can be edited from the phone", st == 200 and j.get("data") is True, j)
    st, j = rpc("set_due", {"uid": 1, "due": "2026-02-30"})
    check("an impossible due date is refused", st == 200 and j.get("data") is False, j)
    st, j = rpc("set_title", {"uid": 1, "title": "renamed mail"})
    check("a mail task cannot be renamed", st == 200 and j.get("data") is False, j)
    st, j = rpc("set_snooze", {"uid": 1, "date": "2020-01-01"})
    check("snoozing until a past date is refused", st == 400, j)
    st, j = rpc("add_task", {"title": "Renew passport", "priority": 3, "due": ""})
    uid_mine = j.get("data")
    check("a task of your own can be added", st == 200 and isinstance(uid_mine, int) and uid_mine >= 2000000000, j)
    st, j = rpc("set_title", {"uid": uid_mine, "title": "Renew passport online"})
    check("and renamed", st == 200 and j.get("data") is True, j)
    future = time.strftime("%Y-%m-%d 09:00", time.localtime(time.time() + 3 * 86400))
    past = time.strftime("%Y-%m-%d 09:00", time.localtime(time.time() - 3 * 86400))
    st, j = rpc("set_reminder", {"uid": uid_mine, "at": future, "repeat": "weekly"})
    check("a reminder with a repeat can be set from the phone on your own task", st == 200 and j.get("data") is True, j)
    st, j = rpc("state")
    mine = next(t for t in j["data"]["tasks"] if t["uid"] == uid_mine)
    check("and it comes back in the task", mine["remind"] == future and mine["repeat"] == "weekly", mine)
    st, j = rpc("set_reminder", {"uid": uid_mine, "at": past})
    check("a reminder in the past is refused with the reason", st == 400 and "has not passed" in j.get("error", ""), j)
    st, j = rpc("set_reminder", {"uid": 1, "at": future})
    check("a reminder on a mail task is refused", st == 400 and "added yourself" in j.get("error", ""), j)
    st, j = rpc("set_done", {"uid": uid_mine, "done": True})
    st, j2 = rpc("state")
    mine = next(t for t in j2["data"]["tasks"] if t["uid"] == uid_mine)
    check("finishing a repeating task keeps it open and moves it a week on", mine["status"] == "open" and mine["remind"] > future, mine)
    st, j = rpc("draft_generate", {"uid": uid_mine, "instruction": ""})
    check("the hub refuses drafts for a task of your own as well", st == 400, j)

    st, j = rpc("draft_generate", {"uid": 1, "instruction": "confirm"})
    check("hub refuses draft_generate (spends AI quota: desktop-only)", st == 400 and "not available" in j.get("error", ""), j)
    st, j = rpc("draft_update", {"uid": 1, "body": "My own reply."})
    check("editing a draft marks it as the user's", st == 200)
    st, j = rpc("state")
    d = next(t for t in j["data"]["tasks"] if t["uid"] == 1)["draft"]
    check("draft is stored with the task", d and d["body"] == "My own reply." and d["source"] == "user")
    st, j = rpc("draft_to_mailbox", {"uid": 1})
    check("hub refuses draft_to_mailbox (writes to the mailbox: desktop-only)", st == 400 and "not available" in j.get("error", ""), j)
    st, j = rpc("state")
    t1b = next(t for t in j["data"]["tasks"] if t["uid"] == 1)
    check("reply warning flag reaches the client", "reply_warn" in t1b and t1b["reply_warn"] is False, t1b.get("reply_warn"))
    st, j = rpc("set_snooze", {"uid": 2, "preset": "tomorrow"})
    check("snooze works", st == 200 and j["data"] is True)
    st, j = rpc("state")
    check("snoozed task has a future wake time", next(t for t in j["data"]["tasks"] if t["uid"] == 2)["snooze_until"] > time.time())
    st, j = rpc("set_pref", {"uid": 3, "level": "important"})
    st, j = rpc("state")
    check("sender preference lifts mail from that sender", next(t for t in j["data"]["tasks"] if t["uid"] == 3)["priority"] >= 4)
    for cmd in ["save_settings", "set_secret", "sign_out", "export_markdown", "rotate_hub_token", "hub_info", "save_account", "read_body", "diagnostics", "rules_save", "rules_apply", "search", "save_event", "add_account", "remove_account", "backup_export", "backup_import"]:
        st, j = rpc(cmd, {"patch": {}})
        check(f"hub refuses desktop-only command {cmd}", st == 400 and "not available" in j.get("error", ""))
    # hostile input through the hub
    st, j = rpc("set_priority", {"uid": 1, "priority": 99})
    check("out-of-range priority is rejected", st == 200 and j["data"] is False)
    st, j = rpc("draft_update", {"uid": 1, "body": "x" * 9000})
    check("oversized draft is rejected", st == 400)
    st, _, _ = req("/api/rpc", {"cmd": "state"}, token=None)
    check("no token means 401", st == 401)

    # desktop-only commands, through the real window's IPC
    node = subprocess.run(["node", os.path.join(HERE, "desktop_checks.mjs")], capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=60)
    for line in node.stdout.strip().splitlines():
        results.append((line[5:], line.startswith("PASS")))
        print(line)
    db = sqlite3.connect(os.path.join(DATA, "agent.db"))
    n = db.execute("SELECT COUNT(*) FROM outbox WHERE op='draft' AND uid=1").fetchone()[0]
    db.close()
    check("exactly one draft save is queued after three clicks", n == 1, n)
    time.sleep(1)
    check("app process is still running", proc.poll() is None)
finally:
    proc.terminate()
    try:
        proc.wait(5)
    except Exception:
        proc.kill()
    time.sleep(1)
    # the window's browser engine keeps files open for a few seconds after the app ends: try for a while
    for _ in range(30):
        shutil.rmtree(DATA, ignore_errors=True)
        if not os.path.exists(DATA):
            break
        time.sleep(1)
    if os.path.exists(DATA):
        print(f"note: {DATA} could not be removed yet (the browser engine still holds it); remove it before the next run")

failed = [n for n, ok in results if not ok]
print(f"\n{len(results) - len(failed)}/{len(results)} checks passed")
sys.exit(1 if failed else 0)
