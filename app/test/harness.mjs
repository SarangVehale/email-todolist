// Runs the real window (app/ui) in a real browser and drives it, with nothing but Node's own modules.
//
// It serves app/ui from a small server on a free port, starts a Chromium-based browser that is already on the machine (headless, with an
// empty profile in a temporary folder), and talks to it over the browser's debugging protocol with Node's built-in WebSocket and fetch.
// Each test gets a fresh page of demo.html?demo=1, which runs mock.js, the window's own demo backend: a test can add thousands of tasks,
// make a request slow or make it fail by wrapping window.mock.
//
//   INBOX_AGENT_BROWSER=<path to a Chromium-based browser>   use this one
//   INBOX_AGENT_UI_TESTS=require                              a missing browser is a failure (otherwise the tests are skipped)
import http from "node:http";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import assert from "node:assert/strict";
import { describe, it as nodeIt, before, after } from "node:test";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const UI = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "ui");
const TYPES = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8", ".svg": "image/svg+xml", ".json": "application/json", ".woff2": "font/woff2" };
const sleep = ms => new Promise(r => setTimeout(r, ms));
/** A promise that fails instead of waiting for ever: a test that hangs must fail, not stop the whole run. */
const within = (promise, ms, what) => { let t; return Promise.race([promise, new Promise((_, bad) => { t = setTimeout(() => bad(new Error("timed out: " + what)), ms) })]).finally(() => clearTimeout(t)) };

export const required = process.env.INBOX_AGENT_UI_TESTS === "require";

/** The path of a Chromium-based browser on this machine, or null. */
export function findBrowser() {
  const given = process.env.INBOX_AGENT_BROWSER;
  if (given) return fs.existsSync(given) ? given : null;
  const list = [];
  if (process.platform === "win32") {
    for (const base of [process.env["ProgramFiles(x86)"], process.env.ProgramFiles, process.env.LOCALAPPDATA]) {
      if (!base) continue;
      list.push(path.join(base, "Microsoft", "Edge", "Application", "msedge.exe"), path.join(base, "Google", "Chrome", "Application", "chrome.exe"));
    }
  } else if (process.platform === "darwin") {
    list.push("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge", "/Applications/Chromium.app/Contents/MacOS/Chromium");
  } else {
    for (const name of ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "microsoft-edge"]) {
      const r = spawnSync("which", [name], { encoding: "utf8" });
      if (r.status === 0 && r.stdout.trim()) list.push(r.stdout.trim());
    }
  }
  return list.find(p => fs.existsSync(p)) || null;
}

function serveUi() {
  const server = http.createServer((req, res) => {
    let name;
    try { name = decodeURIComponent(new URL(req.url, "http://x").pathname).replace(/^\/+/, "") || "index.html" } catch { res.writeHead(400); res.end("bad request"); return }
    const file = path.resolve(UI, name);
    if (!file.startsWith(UI + path.sep) || !fs.existsSync(file) || !fs.statSync(file).isFile()) { res.writeHead(404); res.end("not found"); return }
    res.writeHead(200, { "Content-Type": TYPES[path.extname(file)] || "application/octet-stream", "Cache-Control": "no-store" });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise(resolve => server.listen(0, "127.0.0.1", () => resolve(server)));
}

/** One tab of the browser. */
export class Page {
  constructor(ws, id, harness) {
    this.ws = ws; this.id = id; this.harness = harness; this.next = 0; this.pending = new Map(); this.waiters = []; this.errors = [];
    ws.onmessage = e => {
      const m = JSON.parse(e.data);
      if (m.id && this.pending.has(m.id)) { const { ok, bad } = this.pending.get(m.id); this.pending.delete(m.id); m.error ? bad(new Error(m.error.message)) : ok(m.result) }
      else if (m.method) {
        if (m.method === "Runtime.exceptionThrown") this.errors.push(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text);
        this.waiters = this.waiters.filter(w => !(w.method === m.method && (w.ok(m.params), true)));
      }
    };
  }
  send(method, params = {}) { return within(new Promise((ok, bad) => { const id = ++this.next; this.pending.set(id, { ok, bad }); this.ws.send(JSON.stringify({ id, method, params })) }), 60000, "the browser's answer to " + method) }
  once(method) { return new Promise(ok => this.waiters.push({ method, ok })) }

  /** Opens a path of the served window and waits until it has drawn. */
  async goto(p = "/demo.html?demo=1") {
    await this.send("Page.enable"); await this.send("Runtime.enable");
    const loaded = this.once("Page.loadEventFired");
    await this.send("Page.navigate", { url: this.harness.url + p });
    await within(loaded, 30000, "the page to load");
    await this.waitFor(() => typeof S !== "undefined" && S.tasks && S.tasks.length > 0 && document.querySelector("#view .row"));
  }
  /** Runs a function in the page (it must not use anything outside itself) and returns what it returns. */
  async run(fn, ...args) {
    const r = await this.send("Runtime.evaluate", { expression: `(${fn.toString()})(...${JSON.stringify(args)})`, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error("in the page: " + (r.exceptionDetails.exception?.description || r.exceptionDetails.text));
    return r.result.value;
  }
  /** Polls a function in the page until it returns something truthy. */
  async waitFor(fn, ...args) {
    const until = Date.now() + 15000;
    for (;;) {
      let v; try { v = await this.run(fn, ...args) } catch { v = false }
      if (v) return v;
      if (Date.now() > until) throw new Error("timed out waiting for: " + fn.toString().slice(0, 160));
      await sleep(25);
    }
  }
  /** Lets the window finish what it is doing (the demo backend answers in a moment). */
  async settle(ms = 60) { await sleep(ms) }

  /** A real key press, to whatever has the focus. */
  async key(name, { shift = false } = {}) {
    const special = { Escape: [27, "Escape"], Enter: [13, "Enter"], Tab: [9, "Tab"], ArrowDown: [40, "ArrowDown"], ArrowUp: [38, "ArrowUp"] }[name];
    const punctuation = { ",": [188, "Comma"], "/": [191, "Slash"] }[name];
    const base = special ? { key: name, code: special[1], windowsVirtualKeyCode: special[0], modifiers: shift ? 8 : 0 }
      : { key: name, text: name, code: punctuation ? punctuation[1] : /^[a-z]$/i.test(name) ? "Key" + name.toUpperCase() : /^\d$/.test(name) ? "Digit" + name : "", windowsVirtualKeyCode: punctuation ? punctuation[0] : name.toUpperCase().charCodeAt(0), modifiers: shift ? 8 : 0 };
    // Enter carries a carriage return as its text: that is what makes the page see a keypress, which is what presses a focused button
    await this.send("Input.dispatchKeyEvent", { type: special && name !== "Enter" ? "rawKeyDown" : "keyDown", ...base, ...(name === "Enter" ? { text: "\r" } : {}) });
    await this.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
  }
  async keys(name, n) { for (let i = 0; i < n; i++) await this.key(name) }
  /** A real mouse click on the middle of the element that matches (it is scrolled into view first). The click waits until the element stays
   *  where it is and is what is under the pointer there: the window moves its own focus a moment after a dialog opens, which scrolls the
   *  dialog, and a click aimed before that would land on something else. */
  async click(selector, { shift = false, nth = 0 } = {}) {
    const look = () => this.run((sel, nth) => {
      const el = document.querySelectorAll(sel)[nth]; if (!el) return null;
      el.scrollIntoView({ block: "center" });
      const r = el.getBoundingClientRect(), x = r.left + r.width / 2, y = r.top + r.height / 2, hit = document.elementFromPoint(x, y);
      return { x, y, onIt: !!hit && (el === hit || el.contains(hit)) };
    }, selector, nth);
    let box = null, seen = false;
    for (let i = 0; i < 60 && !box; i++) {
      const a = await look(); if (!a) { await sleep(50); continue }
      seen = true;
      await sleep(40);
      const b = await look();
      if (b && a.x === b.x && a.y === b.y && b.onIt) box = b; else await sleep(50);
    }
    if (!box) throw new Error(seen ? "it is covered by something else, or keeps moving: " + selector : "nothing matches " + selector);
    const mods = shift ? 8 : 0;
    await this.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: box.x, y: box.y });
    await this.send("Input.dispatchMouseEvent", { type: "mousePressed", x: box.x, y: box.y, button: "left", clickCount: 1, modifiers: mods });
    await this.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: box.x, y: box.y, button: "left", clickCount: 1, modifiers: mods });
    await this.settle(30);
  }
  /** Types into an input: focuses it, selects what is there, and inserts the text as typing would. */
  async fill(selector, text) {
    await this.run(sel => { const el = document.querySelector(sel); el.focus(); el.select?.() }, selector);
    await this.send("Input.insertText", { text });
  }
  async close() { try { this.ws.close() } catch { /* already closed */ } await fetch(`${this.harness.browserUrl}/json/close/${this.id}`, { signal: AbortSignal.timeout(10000) }).catch(() => {}) }
}

/** The profile folders of browsers that are running. At exit (also after Ctrl-C) those that have not been deleted are handed to a cleaner;
 *  the browser of a run that crashed is not ended by that, and its folder is cleared by a later run once it is an hour old. */
const live = new Set();
/** Windows tools by full path: a Node started from some shells has a PATH that does not find them (taskkill was "not found"). */
const SYSTEM32 = path.join(process.env.SystemRoot || process.env.windir || "C:\\Windows", "System32");
const TASKKILL = path.join(SYSTEM32, "taskkill.exe"), POWERSHELL = path.join(SYSTEM32, "WindowsPowerShell", "v1.0", "powershell.exe");
/** Ends every process whose command line holds this folder (a browser started with it as its profile), and nothing else. */
function killByProfile(profile) {
  const limit = { stdio: "ignore", timeout: 15000 }; // a stuck helper must not hold the run for ever
  if (process.platform === "win32") {
    const folder = profile.replace(/'/g, "''");
    spawnSync(POWERSHELL, ["-NoProfile", "-NonInteractive", "-Command", `Get-CimInstance Win32_Process | Where-Object { $_.ProcessId -ne $PID -and $_.CommandLine -like '*${folder}*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`], limit);
  } else spawnSync("pkill", ["-9", "-f", profile], limit);
}
// A helper process of Edge can hold a file of the profile for 20 seconds or more after the browser has been ended (seen on Windows). A folder
// that could not be deleted at once is handed, when this process ends, to a small process of its own that goes on trying for two minutes, so
// that the run does not wait for it.
process.on("exit", () => {
  const left = [...live].filter(p => fs.existsSync(p));
  if (!left.length) return;
  const script = "const fs=require('fs');const left=JSON.parse(process.argv[1]);let n=0;const t=setInterval(()=>{for(const p of left.slice()){try{fs.rmSync(p,{recursive:true,force:true});left.splice(left.indexOf(p),1)}catch(e){}}if(!left.length||++n>120)clearInterval(t)},1000)";
  try { spawn(process.execPath, ["-e", script, JSON.stringify(left)], { detached: true, stdio: "ignore", windowsHide: true, cwd: os.tmpdir() }).unref() } catch { /* the next run clears it once it is an hour old */ }
});
process.on("SIGINT", () => process.exit(130));

/** The server and the browser for one test file. */
export class Harness {
  static sweep() {
    for (const name of fs.readdirSync(os.tmpdir())) {
      if (!name.startsWith("inbox-agent-ui-")) continue;
      const dir = path.join(os.tmpdir(), name);
      try { if (Date.now() - fs.statSync(dir).mtimeMs > 3600e3) fs.rmSync(dir, { recursive: true, force: true }) } catch { /* in use */ }
    } // only those more than an hour old: a run that is going on now has newer ones
  }
  static async start() {
    if (typeof WebSocket === "undefined") {
      if (required) throw new Error("these tests need Node 22 or later (the built-in WebSocket)");
      return null;
    }
    Harness.sweep();
    const exe = findBrowser();
    if (!exe) {
      if (required) throw new Error("no Chromium-based browser found, and INBOX_AGENT_UI_TESTS=require (set INBOX_AGENT_BROWSER to its path)");
      return null;
    }
    const h = new Harness(); h.exe = exe;
    try {
    h.server = await serveUi(); h.url = `http://127.0.0.1:${h.server.address().port}`; h.server.unref();
    h.profile = fs.mkdtempSync(path.join(os.tmpdir(), "inbox-agent-ui-")); live.add(h.profile);
    const args = ["--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check", "--disable-extensions", "--disable-background-networking", `--user-data-dir=${h.profile}`, "--remote-debugging-port=0", "--window-size=1200,900"];
    if (process.platform === "linux") args.push("--no-sandbox"); // hosted CI runners start it as a user without the kernel features the sandbox needs
    h.proc = spawn(exe, [...args, "about:blank"], { stdio: "ignore" });
    h.proc.on("error", e => { h.spawnError = e }); h.proc.unref();
    const portFile = path.join(h.profile, "DevToolsActivePort");
    let port = 0; // the file can be there but locked, or there but not yet written: read it until it holds a number
    for (let i = 0; i < 300 && !port; i++) {
      if (h.spawnError) throw new Error("the browser could not be started: " + h.spawnError.message);
      try { const first = fs.readFileSync(portFile, "utf8").split("\n")[0].trim(); if (/^\d+$/.test(first)) port = +first } catch { /* not there yet, or in use */ }
      if (!port) await sleep(50);
    }
    if (!port) throw new Error("the browser did not start");
    h.browserUrl = `http://127.0.0.1:${port}`;
    return h;
    } catch (e) { await h.stop(); throw e }
  }
  /** A fresh tab. */
  async page() {
    const t = await (await fetch(`${this.browserUrl}/json/new?about:blank`, { method: "PUT", signal: AbortSignal.timeout(15000) })).json();
    const ws = new WebSocket(t.webSocketDebuggerUrl);
    await within(new Promise((ok, bad) => { ws.onopen = ok; ws.onerror = () => bad(new Error("could not connect to the page")) }), 15000, "the connection to the page");
    return new Page(ws, t.id, this);
  }
  async stop() {
    try { this.server?.close() } catch { /* closed */ }
    // The process we started is not always the browser: Edge on Windows hands over to another process and exits, and the browser then
    // goes on running for 20 seconds or more after it has been asked to quit, holding its profile folder. So the browser is asked for its
    // own process number, asked to quit, and that process and its helpers are ended.
    const answers = async () => { try { await fetch(`${this.browserUrl}/json/version`, { signal: AbortSignal.timeout(2000) }); return true } catch { return false } };
    let pid = 0;
    if (this.browserUrl) {
      try {
        const info = await (await fetch(`${this.browserUrl}/json/version`, { signal: AbortSignal.timeout(5000) })).json();
        const ws = new WebSocket(info.webSocketDebuggerUrl);
        await new Promise(done => {
          ws.onopen = () => ws.send(JSON.stringify({ id: 1, method: "SystemInfo.getProcessInfo" }));
          ws.onmessage = e => {
            const m = JSON.parse(e.data);
            if (m.id === 1) {
              pid = ((m.result && m.result.processInfo) || []).find(x => x.type === "browser")?.id || 0;
              ws.send(JSON.stringify({ id: 2, method: "Browser.close" }));
              // On Windows the browser goes on running for 20 seconds or more after it has been asked to quit, so it and its helpers are ended
              // now, while the number is certainly still the browser's (a number can be given to another process once it has gone).
              if (pid && process.platform === "win32") spawnSync(TASKKILL, ["/F", "/T", "/PID", String(pid)], { stdio: "ignore", timeout: 15000 }); // (a filter by image name would make taskkill ask WMI, which takes 10 seconds on a busy machine)
            }
          };
          ws.onclose = done; ws.onerror = done; setTimeout(done, 3000);
        });
      } catch { /* it is already gone */ }
      for (let i = 0; i < 30 && await answers(); i++) await sleep(100); // wait until it has stopped answering
    }
    if (this.proc && this.proc.exitCode === null) this.proc.kill("SIGKILL"); // (the process that was started: Node holds its handle, so this cannot reach another)
    // If the browser did not go (it still answers), whatever is left is found by the profile folder it was started with, which is unique to
    // this run, and ended (looking for processes is slow on a busy machine, so it is not done when it is not needed, and it has a time limit).
    if (this.browserUrl && await answers()) killByProfile(this.profile);
    // The folder is deleted if that works within a few seconds. If a helper process still holds a file it stays on the list, and the process
    // that ends this run deletes it (see above) without anything waiting for it.
    for (let i = 0; i < 15; i++) { try { fs.rmSync(this.profile, { recursive: true, force: true }); live.delete(this.profile); break } catch { await sleep(200) } }
  }
}

/** Puts a lot of tasks into the demo backend and loads them (runs in the page). */
export function addTasks({ open = 0, auto = 0, done = 0, gone = 0, base = 50000 } = {}) {
  const cats = ["work", "personal", "finance", "promo", "reading", "other"];
  const make = (n, start, status, bulk, prefix, hrs) => {
    for (let i = 0; i < n; i++) {
      const t = M.mk(start + i, prefix + (i % 200), prefix.toLowerCase() + (i % 200) + "@mail.example", prefix + " subject " + i, hrs + i / 60, 1 + (i % 2), cats[i % 6], "Do " + prefix + " " + i, false, false, "An excerpt of " + prefix + " " + i, "Rule-based", { known: false, reply: false, bulk });
      t.mailbox = 0; t.thread = prefix + i;
      if (status === "done") { t.status = "done"; t.done_at = 1000 + i }
      M.tasks.push(t);
    }
  };
  make(open, base, "open", false, "Main", 1); make(auto, base + 10000, "open", true, "Auto", 1); make(done, base + 20000, "done", false, "Fin", 1);
  const first = M.tasks.length; make(gone, base + 30000, "open", false, "Gone", 1); M.tasks.slice(first).forEach(t => { t.gone = true }); // mail that left the inbox
  lastSnap = ""; return load().then(() => S.tasks.length);
}

/** Makes the demo backend slow, fail or answer something else for chosen commands (runs in the page). A rule is
 *  { cmd, args?: {name: value} (all must match), hold?: true (answers only when the test calls release(page)), delay?: ms, fail?: "message",
 *  result?: anything }; the first that matches is used. `hold` makes a "slow answer" exact: nothing depends on how fast the machine is.
 *  The commands asked of the backend are kept in window.__calls, and unwrapMock() puts the real backend back. */
export function wrapMock(rules) {
  window.__realMock = window.__realMock || window.mock; window.__calls = []; window.__held = [];
  window.mock = (cmd, a) => {
    window.__calls.push({ cmd, a });
    const r = rules.find(x => x.cmd === cmd && Object.entries(x.args || {}).every(([k, v]) => a && a[k] === v));
    if (!r) return window.__realMock(cmd, a);
    const go = () => r.fail ? Promise.reject(r.fail) : r.result !== undefined ? Promise.resolve(r.result) : window.__realMock(cmd, a);
    if (r.hold) return new Promise(res => { window.__held.push(() => res(go())) });
    return r.delay ? new Promise(res => setTimeout(() => res(go()), r.delay)) : go();
  };
}
export function unwrapMock() { if (window.__realMock) window.mock = window.__realMock }
/** Waits until the backend is holding a call, lets the oldest one go, and gives the window a moment to act on the answer. */
export async function release(page) {
  await page.waitFor(() => window.__held && window.__held.length > 0);
  await page.run(() => window.__held.shift()());
  await page.settle(150);
}

/** Opens Settings with the comma key and waits until it has drawn. */
export async function openSettings(page) {
  await page.key(",");
  await page.waitFor(() => document.querySelector("#veil.show [data-s=addacct]"));
  await page.waitFor(() => veil.contains(document.activeElement)); // the window moves the keyboard into the dialog a moment after it opens
}
/** Chooses a backup file as the file chooser would, then lets the window read it (runs in the page). `pad` adds that many spaces. */
export function chooseFile(name, text, pad = 0) {
  const dt = new DataTransfer(); dt.items.add(new File([text + " ".repeat(pad)], name, { type: "application/json" }));
  const i = document.querySelector("#bk-file"); i.files = dt.files; i.dispatchEvent(new Event("change", { bubbles: true }));
}
/** The text of a backup file the demo backend accepts. */
export const backupText = (extra = {}) => JSON.stringify({ format: "inbox-agent-backup", version: 1, ...extra });

/** The browser of this test file: started by the first group that needs it, ended after the last test of the file (at the top level of the
 *  module, so it is registered once per file). One browser for each file, not each group: ending one takes seconds on Windows. */
let shared = null;
const sharedHarness = async () => (shared ||= await Harness.start());
after(async () => { await shared?.stop(); shared = null }, { timeout: 120000 });

/** A group of tests that share the file's browser. `define` gets `it(title, async page => {...}, { path?, allowErrors? })`; each test has a fresh page,
 *  and fails if the page threw an exception that nothing caught. */
export function suite(name, define) {
  const given = process.env.INBOX_AGENT_BROWSER;
  const skip = typeof WebSocket === "undefined" && !required ? "needs Node 22 or later (the built-in WebSocket)" : !findBrowser() && !required ? (given ? `INBOX_AGENT_BROWSER is set to ${given}, which is not there` : "no Chromium-based browser found (set INBOX_AGENT_BROWSER)") : false;
  describe(name, { skip }, () => {
    let h;
    before(async () => { h = await sharedHarness() }, { timeout: 60000 });
    define((title, fn, opts = {}) => nodeIt(title, { timeout: 120000 }, async () => {
      const page = await h.page();
      try {
        await page.goto(opts.path);
        await fn(page);
        if (!opts.allowErrors) assert.deepEqual(page.errors, [], "the page threw an exception nothing caught");
      } finally { await page.close() }
    }));
  });
}
