"""Scale check (Windows, needs a release build): the packaged app with a heavy database.

    python scripts/scale_check/scale.py 3000 5000      # open tasks, finished tasks

Starts the built app (app/src-tauri/target/release/app.exe) with a fresh data folder, seeds that many tasks into its database,
then checks the targets of docs/designs/phase8.md: how long the engine takes to answer the window's poll and how big the answer is,
and how long the window takes to draw and to react (it is driven with the browser protocol on port 9222).

It REFUSES TO RUN if the app already has a data folder (%LOCALAPPDATA%\\app.inboxagent.desktop): it would have to delete it afterwards,
and that may be your real mail. Close the app, move that folder away, run this, and move it back. The app runs with its keychain in memory
(INBOX_AGENT_KEYCHAIN=memory), so a real mailbox password is never read."""
import json, os, secrets, shutil, sqlite3, string, subprocess, sys, time, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
EXE = os.path.join(ROOT, "app", "src-tauri", "target", "release", "app.exe")
DATA = os.path.join(os.environ["LOCALAPPDATA"], "app.inboxagent.desktop")
# the hub listens on the network while this runs: a code made for this run, not one that is public in the repository
PORT, TOKEN = 18788, "".join(secrets.choice(string.ascii_letters + string.digits) for _ in range(32))
OPEN, DONE = int(sys.argv[1]), int(sys.argv[2])
HERE = os.path.dirname(os.path.abspath(__file__))


def rpc(cmd, args=None):
    r = urllib.request.Request(f"http://127.0.0.1:{PORT}/api/rpc", data=json.dumps({"cmd": cmd, "args": args or {}}).encode(), method="POST")
    r.add_header("Authorization", "Bearer " + TOKEN)
    t = time.time()
    with urllib.request.urlopen(r, timeout=120) as resp:
        body = resp.read()
    return time.time() - t, len(body)


if os.path.exists(DATA):
    sys.exit(f"{DATA} exists: this check would delete it. Close the app and move that folder away first.")
if not os.path.exists(EXE):
    sys.exit(f"{EXE} is missing: build the app first (cd app && npx tauri build).")
os.makedirs(DATA)
json.dump({"onboarded": True, "account": {"id": 0, "host": "imap.example.com", "user": "me@example.com", "port": 993}, "hub": {"enabled": True, "port": PORT, "token": TOKEN}}, open(os.path.join(DATA, "settings.json"), "w"))
# the seeded settings name a first mailbox, so the app looks for its password: with the keychain in memory it finds none, instead of reading a real one
env = dict(os.environ, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222", INBOX_AGENT_KEYCHAIN="memory")
proc = subprocess.Popen([EXE, "--minimized"], env=env)
try:
    for _ in range(60):
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{PORT}/manifest.json", timeout=2)
            break
        except Exception:
            time.sleep(0.5)
    db = sqlite3.connect(os.path.join(DATA, "agent.db"), timeout=30)
    t = time.time()
    total = OPEN + DONE
    db.execute(f"""WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<{total})
      INSERT INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,status,excerpt,why,signals,body,needs_reply,due,reply_to,refs,auth_fail,thread,done_at)
      SELECT i, '<s'||i||'@x>', 'Sender '||(i%400), 'sender'||(i%400)||'@mail.example', 'Subject line number '||i||' about something fairly ordinary', {time.time()}-i*60, 1+(i%5),
             CASE i%6 WHEN 0 THEN 'work' WHEN 1 THEN 'personal' WHEN 2 THEN 'finance' WHEN 3 THEN 'promo' WHEN 4 THEN 'reading' ELSE 'other' END,
             'Do the thing for message '||i, 0, CASE WHEN i>{OPEN} THEN 'done' ELSE 'open' END, 'An excerpt of the message '||i||' that is a few hundred characters long, so that the size is realistic. '||'x', 'Rule-based', '{{"known":false,"reply":false,"bulk":{"true" if True else "false"}}}',
             'The body of message '||i||' '||'lorem ipsum dolor sit amet ', 0, '', 'sender'||(i%400)||'@mail.example', '', 0, '<s'||i||'@x>', CASE WHEN i>{OPEN} THEN {time.time()} ELSE 0 END FROM n""")
    db.commit(); db.close()
    print(f"seeded {OPEN} open and {DONE} done in {time.time()-t:.1f}s")
    secs = 0; size = 0
    for k in range(3):
        secs, size = rpc("state")
        print(f"state: {secs*1000:.0f} ms, {size/1e6:.1f} MB")
    print(("PASS" if secs < 0.25 and size < 3e6 else "FAIL") + f" the poll answer is under 250 ms and 3 MB (the aim is 120 ms and 2.5 MB)  [{secs*1000:.0f} ms, {size/1e6:.1f} MB]")
    node = subprocess.run(["node", os.path.join(HERE, "scale_ui.mjs")], capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=300)
    print(node.stdout.strip()); print(node.stderr.strip()[-500:])
finally:
    proc.terminate()
    try:
        proc.wait(5)
    except Exception:
        proc.kill()
    time.sleep(1)
    shutil.rmtree(DATA, ignore_errors=True)
