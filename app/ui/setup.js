"use strict";
/* ───────── dialogs: first-run wizard & settings ───────── */
const PROVIDERS = {
  gmail: { name: "Gmail", host: "imap.gmail.com", help: ["Turn on 2-Step Verification in your Google account.", "In Security, open App passwords and create one called Inbox Agent.", "Paste the 16 characters below. IMAP must be on in Gmail settings."] },
  icloud: { name: "iCloud Mail", host: "imap.mail.me.com", help: ["Sign in at account.apple.com.", "Create an app-specific password for Inbox Agent and paste it below."] },
  yahoo: { name: "Yahoo", host: "imap.mail.yahoo.com", help: ["In Account security, generate an app password and paste it below."] },
  fastmail: { name: "Fastmail", host: "imap.fastmail.com", help: ["Settings, Privacy & Security, App passwords. Choose IMAP access."] },
  outlook: { name: "Outlook / Microsoft 365 (not supported yet)", host: "outlook.office365.com", help: ["Microsoft has turned off password sign-in for IMAP, so a password or app password will be refused. Sign-in with Microsoft is not supported in this version.", "Gmail, iCloud, Yahoo, Fastmail and most other IMAP providers work with an app password."] },
  other: { name: "Other IMAP server", host: "", help: ["Enter your server and an app password if your provider offers one."] },
};
let W = { step: 0, provider: "gmail", host: "imap.gmail.com", port: 993, user: "", pw: "", tested: null, testing: false, ai: "rules", ollamaModels: [], model: "", key: "", aiTest: null, desktop: true, topic: "", autostart: true, writeback: false, hub: false };
const veil = $("#veil");
function closeVeil() {
  const back = typeof veilFrom !== "undefined" ? veilFrom : null;
  veil.classList.remove("show"); veil.innerHTML = ""; veil.dataset.kind = ""; lastSnap = ""; render(); load();
  setTimeout(() => { const t = back && back.isConnected ? back : document.querySelector(".row.cur .mid, #bar nav button.on"); t?.focus?.() }, 40);
}
const rand = n => Array.from(crypto.getRandomValues(new Uint8Array(n)), b => "abcdefghjkmnpqrstuvwxyz23456789"[b % 31]).join("");
const panel = inner => `<div class="panel" role="dialog" aria-modal="true" aria-label="Inbox Agent">${inner}</div>`;

function openWizard() { veil.dataset.kind = "wizard"; veil.classList.add("show"); drawWizard(); dialogOpened("Set up Inbox Agent") }
function drawWizard() {
  const n = W.step; let h = `<div class="step">Step ${n + 1} of 5</div>`, back = n > 0 ? `<button class="btn" data-w="back">Back</button>` : "", next = `<button class="btn pri" data-w="next">Continue</button>`;
  if (n === 0) h += `<h2>Welcome.</h2><p>Inbox Agent reads your mail and turns it into one list, ranked by how much each message needs you. You get alerted only when something is important.</p>
      <div class="help"><b>Private by design.</b> Your mail is read on this computer. Your password is kept in your system keychain, never in a file. Nothing in your mailbox is changed unless you switch that on.</div>`;
  if (n === 1) {
    const p = PROVIDERS[W.provider];
    h += `<h2>Connect your mail</h2><label class="f" for="w-provider">Provider</label><select id="w-provider" data-f="provider">${Object.entries(PROVIDERS).map(([k, v]) => `<option value="${k}" ${W.provider === k ? "selected" : ""}>${v.name}</option>`).join("")}</select>
      <div class="help"><b>Getting an app password</b><ol>${p.help.map(x => `<li>${esc(x)}</li>`).join("")}</ol></div>
      ${W.provider === "other" ? `<div class="row2"><div><label class="f" for="w-host">IMAP server</label><input id="w-host" type="text" data-f="host" value="${esc(W.host)}" placeholder="imap.example.com"></div><div><label class="f" for="w-port">Port</label><input id="w-port" type="number" data-f="port" value="${W.port}"></div></div>` : ""}
      <label class="f" for="w-user">Email address</label><input id="w-user" type="text" data-f="user" value="${esc(W.user)}" autocomplete="off" placeholder="you@example.com">
      <label class="f" for="w-pw">App password</label><input id="w-pw" type="password" data-f="pw" value="${esc(W.pw)}" autocomplete="off">
      <div class="foot"><button class="btn sm" data-w="test" ${W.testing ? "disabled" : ""}>${W.testing ? "Testing…" : "Test connection"}</button><span id="testres">${W.tested ? (W.tested.ok ? `<span class="ok">Connected · Sent folder: ${esc(W.tested.sent)}</span>` : `<span class="bad">${esc(W.tested.error)}</span>`) : ""}</span></div>`;
    next = `<button class="btn pri" data-w="next" ${W.tested?.ok ? "" : "disabled"}>Continue</button>`;
  }
  if (n === 2) {
    h += `<h2>How should mail be ranked?</h2><p>You can change this any time in Settings.</p><div role="radiogroup" aria-label="How mail is ranked">
      <button type="button" class="opt ${W.ai === "rules" ? "sel" : ""}" data-ai="rules" role="radio" aria-checked="${W.ai === "rules"}"><div class="r"></div><div><b>Rules only</b><span>Instant and fully private. Uses who you write to, replies to your mail and bulk-mail headers.</span></div></button>
      <button type="button" class="opt ${W.ai === "ollama" ? "sel" : ""}" data-ai="ollama" role="radio" aria-checked="${W.ai === "ollama"}"><div class="r"></div><div><b>Local AI (Ollama)</b><span>Runs a model on your computer. Free and private. Needs Ollama installed with a model pulled.</span></div></button>
      <button type="button" class="opt ${W.ai === "anthropic" ? "sel" : ""}" data-ai="anthropic" role="radio" aria-checked="${W.ai === "anthropic"}"><div class="r"></div><div><b>Claude with your own API key</b><span>Best quality. Email text is sent to Anthropic and billed to your API account.</span></div></button>`;
    h += "</div>";
    if (W.ai === "ollama") h += `<label class="f">Model</label>${W.ollamaModels.length ? `<select data-f="model">${W.ollamaModels.map(m => `<option ${W.model === m ? "selected" : ""}>${esc(m)}</option>`).join("")}</select>` : `<div class="help">Ollama wasn’t found on this computer. Install it from ollama.com, run <b>ollama pull llama3.2</b>, then press Detect.</div>`}<div class="foot"><button class="btn sm" data-w="detect">Detect</button></div>`;
    if (W.ai === "anthropic") h += `<label class="f" for="w-key">API key</label><input id="w-key" type="password" data-f="key" value="${esc(W.key)}" autocomplete="off" placeholder="sk-ant-…"><p>Kept in your system keychain. Signing in with a Claude subscription isn’t offered: Anthropic doesn’t permit it in third-party apps.</p>`;
    if (W.ai !== "rules") h += `<div class="foot"><button class="btn sm" data-w="aitest">Test</button><span>${W.aiTest ? (W.aiTest.ok ? `<span class="ok">Works · P${W.aiTest.priority} · ${esc(W.aiTest.action)}</span>` : `<span class="bad">${esc(W.aiTest.error)} (rules will be used)</span>`) : ""}</span></div>`;
  }
  if (n === 3) {
    h += `<h2>Alerts</h2><label class="sw"><input type="checkbox" data-c="desktop" ${W.desktop ? "checked" : ""}><div><b>Desktop notifications</b><span>Priority 4–5 and urgent mail, and your reminders, the daily digest and snooze notices.</span></div></label>
      <label class="sw"><input type="checkbox" data-c="autostart" ${W.autostart ? "checked" : ""}><div><b>Start with this computer</b><span>Keeps watching your inbox from the tray.</span></div></label>
      <label class="f" for="w-topic">Phone alerts with ntfy (optional)</label><div class="row2" style="grid-template-columns:1fr auto"><input id="w-topic" type="text" data-f="topic" value="${esc(W.topic)}" placeholder="a private topic name"><button class="btn sm" data-w="topic">Generate</button></div>
      <p>Install the free ntfy app on your phone and subscribe to the same topic. Treat the topic like a password.</p>`;
  }
  if (n === 4) {
    h += `<h2>Almost done.</h2><label class="sw"><input type="checkbox" data-c="writeback" ${W.writeback ? "checked" : ""}><div><b>Let Inbox Agent file mail for me</b><span>Marking a task done archives the message. Stars become flags, and on Gmail priorities become labels. Left off, your mailbox is untouched, but the changes you make are remembered and applied if you switch this on later.</span></div></label>
      <label class="sw"><input type="checkbox" data-c="hub" ${W.hub ? "checked" : ""}><div><b>Use on phone or tablet</b><span>Shows a QR code in Settings. Works on your home Wi-Fi (not encrypted) or over a private VPN such as Tailscale.</span></div></label>`;
    next = `<button class="btn pri" data-w="finish">Finish</button>`;
  }
  veil.innerHTML = panel(h + `<div class="foot">${back}<span class="sp"></span>${next}</div>`);
}
veil.addEventListener("input", e => {
  const f = e.target.dataset.f; if (!f || veil.dataset.kind !== "wizard") return;
  W[f] = e.target.type === "number" ? +e.target.value : e.target.value;
  if (["user", "pw", "host", "port"].includes(f)) { W.tested = null; const nx = veil.querySelector("[data-w=next]"); if (nx) nx.disabled = true; const r = $("#testres"); if (r) r.innerHTML = "" }
});
veil.addEventListener("change", e => {
  const t = e.target;
  if (veil.dataset.kind === "wizard") {
    if (t.dataset.f === "provider") { W.provider = t.value; W.host = PROVIDERS[t.value].host; W.port = 993; W.tested = null; drawWizard() }
    if (t.dataset.c) W[t.dataset.c] = t.checked;
    if (t.dataset.f === "model") W.model = t.value;
  } else if (veil.dataset.kind === "settings") settingsChange(t);
});
veil.addEventListener("click", async e => {
  if (e.target.closest("[data-vc]") || e.target === veil && veil.dataset.kind === "settings") return closeVeil();
  if (veil.dataset.kind === "settings") return settingsClick(e);
  if (veil.dataset.kind === "mailbox") return mailboxClick(e);
  if (veil.dataset.kind === "restore") return restoreClick(e);
  const ai = e.target.closest("[data-ai]"); if (ai) { W.ai = ai.dataset.ai; W.aiTest = null; if (W.ai === "ollama") { W.ollamaModels = await rpc("detect_ollama").catch(() => []); W.model = W.ollamaModels[0] || "" } drawWizard(); return }
  const w = e.target.closest("[data-w]")?.dataset.w; if (!w) return;
  if (w === "back") { W.step--; drawWizard() }
  if (w === "next") { W.step++; drawWizard() }
  if (w === "topic") { W.topic = "inbox-" + rand(16); drawWizard() }
  if (w === "detect") { W.ollamaModels = await rpc("detect_ollama").catch(() => []); W.model = W.ollamaModels[0] || ""; drawWizard() }
  if (w === "test") {
    W.testing = true; W.tested = null; drawWizard();
    try { const r = await rpc("test_account", { host: W.host, port: W.port, user: W.user, password: W.pw }); W.tested = { ok: true, sent: r.sent } }
    catch (err) { W.tested = { ok: false, error: String(err).replace(/^Error: /, "") } }
    W.testing = false; drawWizard();
  }
  if (w === "aitest") {
    try {
      if (W.ai === "anthropic" && W.key) await rpc("set_secret", { name: "anthropic-key", value: W.key });
      await rpc("save_settings", { patch: { ai: { mode: W.ai, ollama_model: W.model } } });
      W.aiTest = { ok: true, ...(await rpc("test_ai")) };
    } catch (err) { W.aiTest = { ok: false, error: String(err) } }
    drawWizard();
  }
  if (w === "finish") {
    try {
      await rpc("save_account", { host: W.host, port: W.port, user: W.user, password: W.pw });
      if (W.ai === "anthropic" && W.key) await rpc("set_secret", { name: "anthropic-key", value: W.key });
      await rpc("save_settings", { patch: { ai: { mode: W.ai, ollama_model: W.model }, alerts: { desktop: W.desktop, ntfy_topic: W.topic }, writeback: W.writeback, hub: { enabled: W.hub } } });
      if (TA) await TA.core.invoke(W.autostart ? "plugin:autostart|enable" : "plugin:autostart|disable").catch(() => {});
      await rpc("finish_setup"); W.pw = ""; W.key = ""; closeVeil();
    } catch (err) { toast("Setup failed: " + err) }
  }
});

const ALERT_CATS = ["work", "personal", "finance", "travel", "notification", "promo", "reading", "other"];
function openSettings() { rulesDraft = null; rulesDirty = false; veil.dataset.kind = "settings"; veil.classList.add("show"); drawSettings().then(() => dialogOpened("Settings")) }

/* ───────── your rules (the edits live in rulesDraft until Save) ───────── */
let rulesDraft = null, rulesDirty = false;
const FIELDS = [["sender", "Address is"], ["domain", "Domain is"], ["subject", "Subject contains"]];
function rulesHTML() {
  if (!rulesDraft) rulesDraft = JSON.parse(JSON.stringify(S.rules || []));
  if (!rulesDraft.length) return "<p>No rules yet.</p>";
  const out = r => r.file ? "file" : r.priority ? String(r.priority) : "";
  return rulesDraft.map((r, i) => `<div class="rule" data-i="${i}"><div class="rrow"><select data-r="field" aria-label="Look at">${FIELDS.map(([v, l]) => `<option value="${v}" ${r.field === v ? "selected" : ""}>${l}</option>`).join("")}</select><input type="text" data-r="text" maxlength="100" value="${esc(r.text)}" placeholder="text to look for" aria-label="Text to look for" autocomplete="off"></div>
    <div class="rrow"><span class="lbl">then</span><select data-r="outcome" aria-label="Priority">${[["", "Keep priority"], ["1", "Priority 1"], ["2", "Priority 2"], ["3", "Priority 3"], ["4", "Priority 4"], ["5", "Priority 5"], ["file", "File away (mark done)"]].map(([v, l]) => `<option value="${v}" ${out(r) === v ? "selected" : ""}>${l}</option>`).join("")}</select>
    <select data-r="category" aria-label="Category"><option value="">Keep category</option>${ALERT_CATS.map(c => `<option ${r.category === c ? "selected" : ""}>${c}</option>`).join("")}</select></div>
    <div class="rrow"><button class="mini" data-r="up" ${i === 0 ? "disabled" : ""} aria-label="Move rule up">Up</button><button class="mini" data-r="down" ${i === rulesDraft.length - 1 ? "disabled" : ""} aria-label="Move rule down">Down</button><button class="mini" data-r="del">Remove</button></div></div>`).join("");
}
function rulesNote(html) { const r = $("#rulesres"); if (r) r.innerHTML = html }
function ruleEdit(el) {
  const row = el.closest(".rule"); if (!row || !rulesDraft) return;
  const r = rulesDraft[+row.dataset.i]; if (!r) return;
  const v = el.value, k = el.dataset.r;
  if (k === "field") r.field = v;
  else if (k === "text") r.text = v;
  else if (k === "outcome") { r.file = v === "file"; r.priority = v && v !== "file" ? +v : null }
  else if (k === "category") r.category = v || null;
  rulesDirty = true; rulesNote("Not saved yet");
}
async function ruleClick(b) {
  const k = b.dataset.r, row = b.closest(".rule"), i = row ? +row.dataset.i : -1;
  const redraw = () => { const box = $("#rules"); if (box) box.innerHTML = rulesHTML() };
  if (k === "add") {
    if (rulesDraft.length >= 50) { rulesNote('<span class="bad">At most 50 rules</span>'); return }
    rulesDraft.push({ id: rand(8), field: "sender", text: "", priority: null, category: null, file: false }); rulesDirty = true; redraw(); rulesNote("Not saved yet"); return;
  }
  if (k === "up" && i > 0) { [rulesDraft[i - 1], rulesDraft[i]] = [rulesDraft[i], rulesDraft[i - 1]]; rulesDirty = true; redraw(); rulesNote("Not saved yet"); return }
  if (k === "down" && i >= 0 && i < rulesDraft.length - 1) { [rulesDraft[i + 1], rulesDraft[i]] = [rulesDraft[i], rulesDraft[i + 1]]; rulesDirty = true; redraw(); rulesNote("Not saved yet"); return }
  if (k === "del" && i >= 0) { rulesDraft.splice(i, 1); rulesDirty = true; redraw(); rulesNote("Not saved yet"); return }
  if (k === "save") {
    try { await rpc("rules_save", { rules: rulesDraft }); S.rules = JSON.parse(JSON.stringify(rulesDraft)); rulesDirty = false; rulesNote('<span class="ok">Saved. New mail follows these rules.</span>') }
    catch (err) { rulesNote(`<span class="bad">${esc(String(err).replace(/^Error: /, ""))}</span>`) }
    return;
  }
  if (k === "apply") {
    if (rulesDirty) { rulesNote('<span class="bad">Save the rules first</span>'); return }
    try {
      const d = await rpc("rules_apply", { dry: true });
      if (!d.changed) { rulesNote("No open task would change."); return }
      if (!twice(b, `Click again: change ${d.changed}${d.filed ? `, file ${d.filed}` : ""}`)) { rulesNote(`${d.changed} of ${d.matched} matching tasks would change${d.filed ? `; ${d.filed} would be marked done` : ""}.`); return }
      const r = await rpc("rules_apply", { dry: false });
      b.textContent = "Apply to my list"; // twice() leaves its "click again" wording on the button
      lastSnap = ""; await load();
      rulesNote(`<span class="ok">Changed ${r.changed}${r.filed ? `, filed ${r.filed}` : ""}.</span>`);
    } catch (err) { rulesNote(`<span class="bad">${esc(String(err).replace(/^Error: /, ""))}</span>`) }
  }
}
veil.addEventListener("input", e => { if (veil.dataset.kind === "settings" && e.target.dataset?.r === "text") ruleEdit(e.target) });
/* ───────── several mailboxes ───────── */
function mailStatus(st) {
  st = String(st || "");
  if (st === "idle") return '<span class="ok">Connected</span>';
  if (st === "syncing" || st === "connecting") return "Reading mail…";
  if (st === "no password") return '<span class="bad">Needs a new app password</span>';
  if (st.startsWith("error")) return `<span class="bad">${esc(st.replace(/^error:\s*/, "").slice(0, 90))}</span>`;
  return esc(st);
}
function mailboxesHTML(s) {
  const mbs = (S.accounts && S.accounts.length) ? S.accounts : [{ id: 0, user: s.account?.user, host: s.account?.host, status: S.status }];
  return mbs.map(m => `<div class="mbx" data-id="${m.id}"><p><b>${esc(m.user)}</b> · ${esc(m.host)} · ${mailStatus(m.status)}</p>
    <label class="f" for="s-pw${m.id}">New app password</label><input id="s-pw${m.id}" type="password" data-s="newpw" data-id="${m.id}" autocomplete="off" placeholder="paste a new app password">
    <div class="foot" style="margin-top:8px"><button class="btn sm" data-s="savepw" data-id="${m.id}">Save password</button><span data-pwres="${m.id}"></span><span class="sp"></span>${mbs.length > 1 ? `<button class="btn sm" data-s="rmacct" data-id="${m.id}">Remove</button>` : ""}</div></div>`).join("");
}
let MB = null;
function openAddMailbox() {
  MB = { provider: "gmail", host: PROVIDERS.gmail.host, port: 993, user: "", pw: "", tested: null, testing: false, busy: false };
  veil.dataset.kind = "mailbox"; drawMailbox();
  const p = veil.querySelector(".panel"); if (p) p.setAttribute("aria-label", "Add a mailbox");
  setTimeout(() => veil.querySelector("#m-user")?.focus(), 30);
}
function mailboxBack() { MB = null; veil.dataset.kind = "settings"; drawSettings().then(() => veil.querySelector("[data-s=addacct]")?.focus()) }
function drawMailbox() {
  const p = PROVIDERS[MB.provider], n = (S.accounts || []).length, focused = veil.contains(document.activeElement) ? document.activeElement.id : "";
  veil.innerHTML = panel(`<h2>Add a mailbox</h2><p>Its mail joins the same list. A small tag on each task says which mailbox it came to. ${n} of 5 in use.</p>
    <label class="f" for="m-provider">Provider</label><select id="m-provider" data-f="provider">${Object.entries(PROVIDERS).map(([k, v]) => `<option value="${k}" ${MB.provider === k ? "selected" : ""}>${v.name}</option>`).join("")}</select>
    <div class="help"><b>Getting an app password</b><ol>${p.help.map(x => `<li>${esc(x)}</li>`).join("")}</ol></div>
    ${MB.provider === "other" ? `<div class="row2"><div><label class="f" for="m-host">IMAP server</label><input id="m-host" type="text" data-f="host" value="${esc(MB.host)}" placeholder="imap.example.com"></div><div><label class="f" for="m-port">Port</label><input id="m-port" type="number" data-f="port" value="${MB.port}"></div></div>` : ""}
    <label class="f" for="m-user">Email address</label><input id="m-user" type="text" data-f="user" value="${esc(MB.user)}" autocomplete="off" placeholder="you@example.com">
    <label class="f" for="m-pw">App password</label><input id="m-pw" type="password" data-f="pw" value="${esc(MB.pw)}" autocomplete="off">
    <div class="foot"><button class="btn sm" id="m-test" data-m="test" ${MB.testing ? "disabled" : ""}>${MB.testing ? "Testing…" : "Test connection"}</button><span id="mtestres">${MB.tested ? (MB.tested.ok ? `<span class="ok">Connected · Sent folder: ${esc(MB.tested.sent)}</span>` : `<span class="bad">${esc(MB.tested.error)}</span>`) : ""}</span></div>
    <div class="foot"><span class="sp"></span><button class="btn" data-m="cancel">Cancel</button><button class="btn pri" id="m-add" data-m="add" ${MB.tested?.ok && !MB.busy ? "" : "disabled"}>Add mailbox</button></div>`);
  veil.querySelector(".panel")?.setAttribute("aria-label", "Add a mailbox");
  if (focused) veil.querySelector("#" + focused)?.focus(); // a redraw must not drop the keyboard onto the page
}
async function mailboxClick(e) {
  const m = e.target.closest("[data-m]")?.dataset.m, mb = MB; if (!m || !mb) return;
  if (m === "cancel") return mailboxBack();
  const here = () => MB === mb; // Cancel or Escape while a request is out leaves nothing to draw on when it comes back
  if (m === "test") {
    mb.testing = true; mb.tested = null; drawMailbox();
    try { const r = await rpc("test_account", { host: mb.host, port: mb.port, user: mb.user, password: mb.pw }); mb.tested = { ok: true, sent: r.sent } }
    catch (err) { mb.tested = { ok: false, error: String(err).replace(/^Error: /, "") } }
    mb.testing = false; if (here()) drawMailbox();
  }
  if (m === "add") {
    if (mb.busy) return; mb.busy = true; drawMailbox();
    try { await rpc("add_account", { host: mb.host, port: mb.port, user: mb.user, password: mb.pw }) }
    catch (err) { mb.busy = false; mb.tested = { ok: false, error: String(err).replace(/^Error: /, "") }; if (here()) drawMailbox(); return }
    clearReadCaches(); // a mailbox number may be a removed mailbox's again
    lastSnap = ""; await load(); toast("Mailbox added. Its mail is being read.");
    if (here()) { MB = null; veil.dataset.kind = "settings"; drawSettings().then(() => veil.querySelector("[data-s=addacct]")?.focus()) }
  }
}
/* task ids are reused when a mailbox number is: what was read or can be undone for a removed mailbox must not follow the number */
function clearReadCaches() { for (const k of Object.keys(bodies)) delete bodies[k]; undoStack.length = 0; const u = $("#undo"); if (u) u.style.display = "none" }
/* ───────── restore from a backup file ───────── */
let RS = null;
const MAX_BACKUP = 8 * 1024 * 1024;
async function pickBackup(input) {
  const file = input.files && input.files[0]; if (!file) return;
  const out = $("#bkres");
  if (rulesDirty) { out.innerHTML = '<span class="bad">You have rule changes that are not saved. Save the rules first, then restore.</span>'; return }
  if (file.size > MAX_BACKUP) { out.innerHTML = '<span class="bad">That file is too large to be one of this app\'s backups (the limit is 8 MB)</span>'; return }
  let text; try { text = await file.text() } catch (err) { out.innerHTML = '<span class="bad">That file could not be read</span>'; return }
  RS = { name: file.name, json: text, withOptions: false, preview: null, error: "", busy: false };
  const rs = RS;
  veil.dataset.kind = "restore"; await previewRestore();
  if (RS !== rs) return; // closed while the file was being read
  drawRestore();
  setTimeout(() => veil.querySelector("[data-q=cancel]")?.focus(), 30); // Cancel, so that Enter pressed by reflex restores nothing
}
async function previewRestore() {
  const rs = RS, asked = rs.withOptions; rs.error = ""; rs.preview = null;
  // an answer is kept only if it is for what the box says now: the preview shown is the preview of what Restore will do
  try { const p = await rpc("backup_import", { json: rs.json, apply: false, settings: asked }); if (RS === rs && rs.withOptions === asked) rs.preview = p }
  catch (err) { if (RS === rs && rs.withOptions === asked) rs.error = String(err).replace(/^Error: /, "") }
}
function previewLines(p) {
  const n = (k, one, many) => p[k] ? `<li>${p[k]} ${p[k] === 1 ? one : many}</li>` : "";
  const l = [
    n("rules_added", "rule will be added", "rules will be added"),
    n("sender_choices_added", "sender choice will be added", "sender choices will be added"),
    n("corrections_added", "correction will be added", "corrections will be added"),
    n("own_tasks_added", "task of yours will be added", "tasks of yours will be added"),
    n("mail_applied", "message already here will get what you did with it", "messages already here will get what you did with them"),
    n("mail_parked", "message is kept for when it is read", "messages are kept for when they are read"),
    p.reminders_dropped ? `<li>${p.reminders_dropped} reminder that has already passed is left out</li>` : "",
    p.mailboxes_added ? `<li>${p.mailboxes_added} mailbox will be added, with no password: each asks for its app password in Settings</li>` : "",
    p.options_changed && p.options_changed.length ? `<li>${p.options_changed.length} option${p.options_changed.length === 1 ? "" : "s"} will change</li>` : "",
  ].join("");
  const skipped = p.mail_skipped_no_mailbox ? `<p>${p.mail_skipped_no_mailbox} ${p.mail_skipped_no_mailbox === 1 ? "entry is" : "entries are"} about a mailbox that is not set up here. Add that mailbox, then restore again.</p>` : "";
  const same = (p.rules_skipped || 0) + (p.own_tasks_skipped || 0) + (p.sender_choices_skipped || 0) + (p.corrections_skipped || 0) + (p.mail_unchanged || 0);
  const already = same ? `<p>What you already have is kept as it is; ${same} item${same === 1 ? " was" : "s were"} already here.</p>` : "";
  const rules = (p.rules_new || []).length ? `<p>New rules:</p><ul class="plist">${p.rules_new.map(x => `<li>${esc(x)}</li>`).join("")}</ul>` : "";
  const boxes = (p.mailboxes_new || []).length ? `<p><b>Mailboxes to add</b> (each asks for its app password, so check the server: restore only a file you made yourself):</p><ul class="plist">${p.mailboxes_new.map(x => `<li>${esc(x)}</li>`).join("")}</ul>` : "";
  const cloud = (p.options_skipped || []).includes("ai.mode") ? "<p>The file asks for ranking with Claude and your own key. That is not restored: choose it in Settings if you want it.</p>" : "";
  return { l, extra: rules + boxes + cloud + skipped + already };
}
function drawRestore() {
  const rs = RS, p = rs.preview;
  let body;
  if (rs.error) body = `<p class="bad">${esc(rs.error)}</p>`;
  else if (!p) body = "<p>Reading the file…</p>";
  else { const { l, extra } = previewLines(p); body = (l ? `<p>Restoring this file will:</p><ul class="plist">${l}</ul>` : "<p>Nothing in this file is new to this computer.</p>") + extra }
  const can = !!p && !rs.error && !rs.busy && !!previewLines(p).l;
  veil.innerHTML = panel(`<h2>Restore from a backup</h2><p>${esc(rs.name)}</p>${body}
    <label class="sw"><input type="checkbox" data-q="options" ${rs.withOptions ? "checked" : ""} ${rs.busy ? "disabled" : ""}><div><b>Also restore options and mailboxes</b><span>Ranking mode (rules or a local model, never the paid cloud choice), alerts, reply-draft options, follow-up days and the list of mailboxes. They replace your current values. Never a password, the phone code or write-back.</span></div></label>
    <p>Restoring adds what is missing and never removes anything you have. A later snooze, and the options if you tick the box, replace yours; choosing a sender important or muted changes that sender's open mail, as choosing it yourself does.</p>
    <div class="foot"><span class="sp"></span><button class="btn" data-q="cancel" ${rs.busy ? "disabled" : ""}>Cancel</button><button class="btn pri" data-q="go" ${can ? "" : "disabled"}>${rs.busy ? "Restoring…" : "Restore"}</button></div>`);
  veil.querySelector(".panel")?.setAttribute("aria-label", "Restore from a backup");
}
function restoreBack() { if (RS && RS.busy) return; /* a restore in progress cannot be cancelled */ RS = null; veil.dataset.kind = "settings"; drawSettings().then(() => veil.querySelector("[data-s=restore]")?.focus()) }
async function restoreClick(e) {
  const q = e.target.closest("[data-q]")?.dataset.q, rs = RS; if (!rs || !q) return;
  if (q === "cancel") return restoreBack();
  if (q === "go" && !rs.busy && rs.preview) {
    rs.busy = true; drawRestore();
    try {
      const r = await rpc("backup_import", { json: rs.json, apply: true, settings: rs.withOptions });
      lastSnap = ""; await load();
      const said = [r.rules_added && `${r.rules_added} rule${r.rules_added === 1 ? "" : "s"}`, r.own_tasks_added && `${r.own_tasks_added} task${r.own_tasks_added === 1 ? "" : "s"}`, (r.mail_applied + r.mail_parked) && `${r.mail_applied + r.mail_parked} thing${r.mail_applied + r.mail_parked === 1 ? "" : "s"} you did with mail`].filter(Boolean).join(", ");
      toast("Restored" + (said ? ": " + said : ""));
      rulesDraft = null; rulesDirty = false; // the editor reads the rules again, so the next Save cannot drop the restored ones
      rs.busy = false; // done: the dialog may close now
      if (RS === rs) restoreBack();
    } catch (err) { rs.busy = false; rs.error = String(err).replace(/^Error: /, ""); if (RS === rs) drawRestore() }
  }
}
veil.addEventListener("change", async e => {
  if (veil.dataset.kind !== "restore" || !RS || e.target.dataset.q !== "options") return;
  const rs = RS; if (rs.busy) return; rs.withOptions = e.target.checked; rs.preview = null; drawRestore(); await previewRestore(); if (RS === rs) { drawRestore(); veil.querySelector("[data-q=options]")?.focus() }
});
veil.addEventListener("input", e => {
  const f = e.target.dataset?.f; if (!f || veil.dataset.kind !== "mailbox" || !MB) return;
  MB[f] = e.target.type === "number" ? +e.target.value : e.target.value;
  if (["user", "pw", "host", "port"].includes(f)) { MB.tested = null; const ad = veil.querySelector("[data-m=add]"); if (ad) ad.disabled = true; const r = $("#mtestres"); if (r) r.innerHTML = "" }
});
veil.addEventListener("change", e => {
  if (veil.dataset.kind === "mailbox" && MB && e.target.dataset.f === "provider") { MB.provider = e.target.value; MB.host = PROVIDERS[e.target.value].host; MB.port = 993; MB.tested = null; drawMailbox() }
});
async function drawSettings() {
  const s = S.settings, a = s.alerts || {}, ai = s.ai || {}, dr = s.drafts || { auto: false, tone: "friendly", signature: "", min_priority: 3, daily_cap: 25 };
  let hub = null; if (s.hub?.enabled) hub = await rpc("hub_info").catch(() => null);
  veil.innerHTML = panel(`<h2>Settings</h2>
    <div class="sect">Mail</div>
    ${mailboxesHTML(s)}
    <p>Providers cancel app passwords whenever you change your account password. Pasting a new one here keeps your list, notes and tasks.</p>
    <div class="foot" style="margin-top:12px"><button class="btn sm" data-s="addacct" ${(S.accounts || []).length >= 5 ? "disabled" : ""}>Add another mailbox</button><span class="sp"></span><button class="btn sm" data-s="signout">Sign out and reset</button></div>
    <div class="sect">Ranking</div>
    <select data-s="aimode"><option value="rules" ${ai.mode === "rules" ? "selected" : ""}>Rules only</option><option value="ollama" ${ai.mode === "ollama" ? "selected" : ""}>Local AI (Ollama)</option><option value="anthropic" ${ai.mode === "anthropic" ? "selected" : ""}>Claude (own API key)</option></select>
    ${ai.mode === "ollama" ? `<label class="f">Ollama model</label><input type="text" data-s="ollama_model" value="${esc(ai.ollama_model)}" placeholder="llama3.2">` : ""}
    ${ai.mode === "anthropic" ? `<label class="f">API key ${S.has_anthropic_key ? "(saved — enter a new one to replace)" : ""}</label><input type="password" data-s="key" placeholder="sk-ant-…" autocomplete="off">` : ""}
    <div class="foot" style="margin-top:14px"><button class="btn sm" data-s="aitest">Test</button><span id="aitres"></span></div>
    <div class="sect">Alerts</div>
    <label class="sw"><input type="checkbox" data-s="desktop" ${a.desktop ? "checked" : ""}><div><b>Desktop notifications</b></div></label>
    <label class="f">ntfy topic</label><input type="text" data-s="topic" value="${esc(a.ntfy_topic)}" placeholder="optional">
    <div class="sect">Alert schedule</div>
    <div class="row2" style="grid-template-columns:1fr 1fr"><div><label class="f">Quiet from</label><input type="time" data-s="quiet_start" value="${esc(a.quiet_start)}"></div><div><label class="f">Quiet until</label><input type="time" data-s="quiet_end" value="${esc(a.quiet_end)}"></div></div>
    <p>Urgent mail still gets through during quiet hours.</p>
    <label class="f">Alert me about</label><div class="chips">${ALERT_CATS.map(c => `<button class="chip ${!(a.categories || []).length || a.categories.includes(c) ? "on" : ""}" data-s="alertcat" data-cat="${c}">${c}</button>`).join("")}</div>
    <p>Urgent mail always alerts. Choose none to hear about every category.</p>
    <label class="f">Daily digest at</label><input type="time" data-s="digest_time" value="${esc(a.digest_time)}">
    <div class="sect">Reply drafts</div>
    <label class="sw"><input type="checkbox" data-s="draft_auto" ${dr.auto ? "checked" : ""}><div><b>Write drafts automatically</b><span>For mail that needs a reply. Needs an AI model (Ollama or your own key). Drafts are never sent; they appear inside the task.</span></div></label>
    <label class="f">Tone</label><select data-s="draft_tone">${["friendly", "concise", "formal"].map(x => `<option ${dr.tone === x ? "selected" : ""}>${x}</option>`).join("")}</select>
    <label class="f">Signature</label><textarea class="dtext" style="min-height:80px" rows="3" data-s="draft_sig" placeholder="Best,&#10;Your name">${esc(dr.signature)}</textarea>
    <div class="row2" style="grid-template-columns:1fr 1fr"><div><label class="f">Only for priority</label><select data-s="draft_min">${[1, 2, 3, 4, 5].map(n => `<option value="${n}" ${dr.min_priority === n ? "selected" : ""}>${n} and up</option>`).join("")}</select></div><div><label class="f">Daily limit</label><input type="number" min="0" max="200" data-s="draft_cap" value="${dr.daily_cap}"></div></div>
    <div class="sect">Follow-ups</div>
    <label class="f" for="s-follow">Remind me when nobody has answered</label>
    <select id="s-follow" data-s="follow_days">${[[0, "Off"], [2, "After 2 days"], [3, "After 3 days"], [5, "After 5 days"], [7, "After 7 days"]].map(([v, l]) => `<option value="${v}" ${(s.followups?.days || 0) === v ? "selected" : ""}>${l}</option>`).join("")}</select>
    <p>Looks at conversations you started in the last two weeks. If nobody has replied, a “Waiting” task appears in your list, and it closes itself when a reply arrives. The app only asks your mail server whether a reply exists, in your Inbox and in All Mail or Archive when there is one. Your own messages and automatic out-of-office replies do not count. It never sends anything.</p>
    <div class="sect">Your rules</div>
    <p>“If the address, domain or subject matches, then set the priority or category, or file the mail.” The first rule that matches is used. A sender you marked “always important” or “mute” skips the rules. A rule on the subject can only raise mail from people you write to, and no rule can raise mail that failed its sender check.</p>
    <p>Rules also run on mail read when you first connect a mailbox. With “Let Inbox Agent file mail” on, a rule that files mail archives it in your mailbox, and priority changes become labels there.</p>
    <div id="rules">${rulesHTML()}</div>
    <div class="foot" style="margin-top:12px"><button class="btn sm" data-r="add">Add a rule</button><button class="btn sm" data-r="save">Save rules</button><button class="btn sm" data-r="apply">Apply to my list</button><span id="rulesres"></span></div>
    <div class="sect">Mailbox</div>
    <label class="sw"><input type="checkbox" data-s="writeback" ${s.writeback ? "checked" : ""}><div><b>Let Inbox Agent file mail</b><span>Done archives, star flags, priorities label (Gmail). ${S.pending ? S.pending + " change(s) waiting." : ""}</span></div></label>
    ${S.update_feature ? `<div class="sect">Updates</div><label class="sw"><input type="checkbox" data-s="update_check" ${s.updates?.check !== false ? "checked" : ""}><div><b>Check for a newer version</b><span>Once a day the app asks GitHub for the latest release. Nothing is downloaded or installed; you only see a notice and a link. GitHub sees your network address, like any web request.</span></div></label>` : ""}
    <div class="sect">Export and support</div><div class="foot" style="margin-top:8px"><button class="btn sm" data-s="export">Copy to-do list as Markdown</button><button class="btn sm" data-s="diag">Copy diagnostics</button></div><p>Diagnostics has the version, counts, option names and recent activity with names, subjects and addresses removed. Read it before you paste it into a bug report.</p>
    <div class="sect">Backup</div>
    <p>Saves your rules, sender choices, corrections, tasks, notes and what you did with mail into one file, so a new computer or a fresh start does not lose them. It holds no password, no phone code and no message bodies (a correction keeps the sender and subject of the mail you corrected). It is not encrypted: keep it like any private document.</p>
    <div class="foot" style="margin-top:8px"><button class="btn sm" data-s="backup">Save a backup file</button><button class="btn sm" data-s="restore">Restore from a backup file</button></div>
    <p id="bkres" style="word-break:break-all"></p><input type="file" id="bk-file" accept=".json,application/json" hidden aria-label="Backup file">
    <div class="sect">Phone and tablet</div>
    <label class="sw"><input type="checkbox" data-s="hub" ${s.hub?.enabled ? "checked" : ""}><div><b>Phone hub</b><span>Serves this app on your network. Scan the code on a phone on the same Wi-Fi, then Add to Home Screen. Plain HTTP: use a trusted network or a private VPN.</span></div></label>
    ${hub ? `<div class="qr">${hub.svg}</div><p style="word-break:break-all;font-size:11px;margin-top:10px">${esc(hub.url)}</p><div class="foot" style="margin-top:10px"><button class="btn sm" data-s="rotate">Rotate access code</button></div>` : ""}
    <div class="foot"><span class="sp"></span><button class="btn pri" data-vc="1">Close</button></div>`);
}
async function settingsChange(t) {
  if (t.id === "bk-file") return pickBackup(t);
  if (t.dataset.r) return ruleEdit(t);
  const k = t.dataset.s; if (!k) return;
  const patch = { aimode: { ai: { mode: t.value } }, ollama_model: { ai: { ollama_model: t.value } }, desktop: { alerts: { desktop: t.checked } }, topic: { alerts: { ntfy_topic: t.value.trim() } }, writeback: { writeback: t.checked }, hub: { hub: { enabled: t.checked } },
    digest_time: { alerts: { digest_time: t.value } }, draft_auto: { drafts: { auto: t.checked } }, draft_tone: { drafts: { tone: t.value } },
    draft_sig: { drafts: { signature: t.value } }, update_check: { updates: { check: t.checked } }, draft_min: { drafts: { min_priority: +t.value } }, follow_days: { followups: { days: +t.value } }, draft_cap: { drafts: { daily_cap: +t.value || 0 } } }[k];
  if (k === "quiet_start" || k === "quiet_end") {
    const st = veil.querySelector("[data-s=quiet_start]").value, en = veil.querySelector("[data-s=quiet_end]").value;
    if (!!st !== !!en) { toast("Set both a start and an end for quiet hours"); return }
    try { S.settings = await rpc("save_settings", { patch: { alerts: { quiet_start: st, quiet_end: en } } }) } catch (e) { toast("Couldn't save: " + e) }
    return;
  }
  if (k === "key") { if (t.value) { await rpc("set_secret", { name: "anthropic-key", value: t.value }); S.has_anthropic_key = true; toast("Key saved to keychain") } return }
  if (!patch) return;
  try { S.settings = await rpc("save_settings", { patch }); drawSettings() } catch (e) { toast("Couldn't save: " + e) }
}
async function settingsClick(e) {
  const rb = e.target.closest("button[data-r]"); if (rb) return ruleClick(rb);
  const k = e.target.closest("[data-s]")?.dataset.s;
  if (k === "aitest") { const r = $("#aitres"); r.textContent = "Testing…"; try { const x = await rpc("test_ai"); r.innerHTML = `<span class="ok">Works · P${x.priority} · ${esc(x.action)}</span>` } catch (err) { r.innerHTML = `<span class="bad">${esc(String(err))}</span>` } }
  if (k === "alertcat") {
    const c = e.target.closest("[data-cat]").dataset.cat, cur = S.settings.alerts?.categories || [];
    const all = cur.length ? cur : [...ALERT_CATS];
    let next = all.includes(c) ? all.filter(x => x !== c) : [...all, c];
    if (next.length === ALERT_CATS.length || !next.length) next = [];
    try { S.settings = await rpc("save_settings", { patch: { alerts: { categories: next } } }); drawSettings() } catch (err) { toast("Couldn't save: " + err) }
  }
  if (k === "savepw") {
    const id = +e.target.closest("[data-s]").dataset.id || 0;
    const box = veil.querySelector(`input[data-s=newpw][data-id="${id}"]`), pw = box?.value.trim(), out = veil.querySelector(`[data-pwres="${id}"]`);
    if (!pw) { out.innerHTML = '<span class="bad">Paste the new password first</span>'; return }
    out.textContent = "Checking…";
    const acct = (id === 0 ? S.settings.account : (S.settings.more_accounts || []).find(m => m.id === id)) || {};
    try {
      await rpc("test_account", { host: acct.host, port: acct.port, user: acct.user, password: pw });
      await rpc("save_account", { id, host: acct.host, port: acct.port, user: acct.user, password: pw });
      box.value = ""; out.innerHTML = '<span class="ok">Saved. Your list is untouched.</span>';
    } catch (err) { out.innerHTML = `<span class="bad">${esc(String(err).replace(/^Error: /, ""))}</span>` }
  }
  if (k === "addacct") { openAddMailbox(); return }
  if (k === "rmacct") {
    const btn = e.target.closest("[data-s]"), id = +btn.dataset.id;
    if (!twice(btn, "Click again: removes this mailbox, its tasks and its saved password from the app")) return;
    try { await rpc("remove_account", { id }); clearReadCaches(); lastSnap = ""; await load(); drawSettings() } catch (err) { toast("Couldn't remove: " + String(err).replace(/^Error: /, "")) }
  }
  if (k === "backup") {
    const out = $("#bkres"); out.textContent = "Saving…";
    try {
      const r = await rpc("backup_export"), left = Object.entries(r.counts.left_out || {}).map(([k, n]) => `${n} ${k.replace(/_/g, " ")}`);
      out.textContent = "Saved " + r.path + (left.length ? `. A backup file holds at most 5,000 sender choices, 2,000 corrections, 2,000 tasks of yours and 5,000 entries about mail; left out: ${left.join(", ")}.` : ""); toast("Backup saved");
    } catch (err) { out.innerHTML = `<span class="bad">${esc(String(err).replace(/^Error: /, ""))}</span>` }
    return;
  }
  if (k === "restore") { const f = $("#bk-file"); f.value = ""; f.click(); return }
  if (k === "diag") { try { await navigator.clipboard.writeText(await rpc("diagnostics")); toast("Diagnostics copied. Read it, then paste it into your bug report.") } catch (err) { toast("Couldn’t copy: " + err) } }
  if (k === "export") { try { await navigator.clipboard.writeText(await rpc("export_markdown")); toast("Copied as Markdown") } catch (err) { toast("Couldn’t copy: " + err) } }
  if (k === "rotate") { await rpc("rotate_hub_token"); toast("New access code. Scan the code again on your phone."); drawSettings() }
  if (k === "signout" && twice(e.target.closest("[data-s]"), "Click again: removes every saved password and key, and the local list")) { await rpc("sign_out"); clearReadCaches(); W = { ...W, step: 0, tested: null, pw: "" }; closeVeil() }
}

