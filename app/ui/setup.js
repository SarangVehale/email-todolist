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
function closeVeil() { veil.classList.remove("show"); veil.innerHTML = ""; veil.dataset.kind = ""; lastSnap = ""; render(); load() }
const rand = n => Array.from(crypto.getRandomValues(new Uint8Array(n)), b => "abcdefghjkmnpqrstuvwxyz23456789"[b % 31]).join("");
const panel = inner => `<div class="panel">${inner}</div>`;

function openWizard() { veil.dataset.kind = "wizard"; veil.classList.add("show"); drawWizard() }
function drawWizard() {
  const n = W.step; let h = `<div class="step">Step ${n + 1} of 5</div>`, back = n > 0 ? `<button class="btn" data-w="back">Back</button>` : "", next = `<button class="btn pri" data-w="next">Continue</button>`;
  if (n === 0) h += `<h2>Welcome.</h2><p>Inbox Agent reads your mail and turns it into one list, ranked by how much each message needs you. You get alerted only when something is important.</p>
      <div class="help"><b>Private by design.</b> Your mail is read on this computer. Your password is kept in your system keychain, never in a file. Nothing in your mailbox is changed unless you switch that on.</div>`;
  if (n === 1) {
    const p = PROVIDERS[W.provider];
    h += `<h2>Connect your mail</h2><label class="f">Provider</label><select data-f="provider">${Object.entries(PROVIDERS).map(([k, v]) => `<option value="${k}" ${W.provider === k ? "selected" : ""}>${v.name}</option>`).join("")}</select>
      <div class="help"><b>Getting an app password</b><ol>${p.help.map(x => `<li>${esc(x)}</li>`).join("")}</ol></div>
      ${W.provider === "other" ? `<div class="row2"><div><label class="f">IMAP server</label><input type="text" data-f="host" value="${esc(W.host)}" placeholder="imap.example.com"></div><div><label class="f">Port</label><input type="number" data-f="port" value="${W.port}"></div></div>` : ""}
      <label class="f">Email address</label><input type="text" data-f="user" value="${esc(W.user)}" autocomplete="off" placeholder="you@example.com">
      <label class="f">App password</label><input type="password" data-f="pw" value="${esc(W.pw)}" autocomplete="off">
      <div class="foot"><button class="btn sm" data-w="test" ${W.testing ? "disabled" : ""}>${W.testing ? "Testing…" : "Test connection"}</button><span id="testres">${W.tested ? (W.tested.ok ? `<span class="ok">Connected · Sent folder: ${esc(W.tested.sent)}</span>` : `<span class="bad">${esc(W.tested.error)}</span>`) : ""}</span></div>`;
    next = `<button class="btn pri" data-w="next" ${W.tested?.ok ? "" : "disabled"}>Continue</button>`;
  }
  if (n === 2) {
    h += `<h2>How should mail be ranked?</h2><p>You can change this any time in Settings.</p>
      <div class="opt ${W.ai === "rules" ? "sel" : ""}" data-ai="rules"><div class="r"></div><div><b>Rules only</b><span>Instant and fully private. Uses who you write to, replies to your mail and bulk-mail headers.</span></div></div>
      <div class="opt ${W.ai === "ollama" ? "sel" : ""}" data-ai="ollama"><div class="r"></div><div><b>Local AI (Ollama)</b><span>Runs a model on your computer. Free and private. Needs Ollama installed with a model pulled.</span></div></div>
      <div class="opt ${W.ai === "anthropic" ? "sel" : ""}" data-ai="anthropic"><div class="r"></div><div><b>Claude with your own API key</b><span>Best quality. Email text is sent to Anthropic and billed to your API account.</span></div></div>`;
    if (W.ai === "ollama") h += `<label class="f">Model</label>${W.ollamaModels.length ? `<select data-f="model">${W.ollamaModels.map(m => `<option ${W.model === m ? "selected" : ""}>${esc(m)}</option>`).join("")}</select>` : `<div class="help">Ollama wasn’t found on this computer. Install it from ollama.com, run <b>ollama pull llama3.2</b>, then press Detect.</div>`}<div class="foot"><button class="btn sm" data-w="detect">Detect</button></div>`;
    if (W.ai === "anthropic") h += `<label class="f">API key</label><input type="password" data-f="key" value="${esc(W.key)}" autocomplete="off" placeholder="sk-ant-…"><p>Kept in your system keychain. Signing in with a Claude subscription isn’t offered: Anthropic doesn’t permit it in third-party apps.</p>`;
    if (W.ai !== "rules") h += `<div class="foot"><button class="btn sm" data-w="aitest">Test</button><span>${W.aiTest ? (W.aiTest.ok ? `<span class="ok">Works · P${W.aiTest.priority} · ${esc(W.aiTest.action)}</span>` : `<span class="bad">${esc(W.aiTest.error)} (rules will be used)</span>`) : ""}</span></div>`;
  }
  if (n === 3) {
    h += `<h2>Alerts</h2><label class="sw"><input type="checkbox" data-c="desktop" ${W.desktop ? "checked" : ""}><div><b>Desktop notifications</b><span>Only for priority 4–5 and urgent mail.</span></div></label>
      <label class="sw"><input type="checkbox" data-c="autostart" ${W.autostart ? "checked" : ""}><div><b>Start with this computer</b><span>Keeps watching your inbox from the tray.</span></div></label>
      <label class="f">Phone alerts with ntfy (optional)</label><div class="row2" style="grid-template-columns:1fr auto"><input type="text" data-f="topic" value="${esc(W.topic)}" placeholder="a private topic name"><button class="btn sm" data-w="topic">Generate</button></div>
      <p>Install the free ntfy app on your phone and subscribe to the same topic. Treat the topic like a password.</p>`;
  }
  if (n === 4) {
    h += `<h2>Almost done.</h2><label class="sw"><input type="checkbox" data-c="writeback" ${W.writeback ? "checked" : ""}><div><b>Let Inbox Agent file mail for me</b><span>Marking a task done archives the message. Stars and priorities become flags and labels. Leave off to keep your mailbox untouched.</span></div></label>
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
function openSettings() { veil.dataset.kind = "settings"; veil.classList.add("show"); drawSettings() }
async function drawSettings() {
  const s = S.settings, a = s.alerts || {}, ai = s.ai || {}, dr = s.drafts || { auto: false, tone: "friendly", signature: "", min_priority: 3, daily_cap: 25 };
  let hub = null; if (s.hub?.enabled) hub = await rpc("hub_info").catch(() => null);
  veil.innerHTML = panel(`<h2>Settings</h2>
    <div class="sect">Mail</div><p>${esc(s.account?.user)} · ${esc(s.account?.host)}</p><div class="foot" style="margin-top:12px"><button class="btn sm" data-s="signout">Sign out and reset</button></div>
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
    <div class="sect">Mailbox</div>
    <label class="sw"><input type="checkbox" data-s="writeback" ${s.writeback ? "checked" : ""}><div><b>Let Inbox Agent file mail</b><span>Done archives, star flags, priorities label (Gmail). ${S.pending ? S.pending + " change(s) waiting." : ""}</span></div></label>
    <div class="sect">Export</div><div class="foot" style="margin-top:8px"><button class="btn sm" data-s="export">Copy to-do list as Markdown</button></div>
    <div class="sect">Phone and tablet</div>
    <label class="sw"><input type="checkbox" data-s="hub" ${s.hub?.enabled ? "checked" : ""}><div><b>Phone hub</b><span>Serves this app on your network. Scan the code on a phone on the same Wi-Fi, then Add to Home Screen. Plain HTTP: use a trusted network or a private VPN.</span></div></label>
    ${hub ? `<div class="qr">${hub.svg}</div><p style="word-break:break-all;font-size:11px;margin-top:10px">${esc(hub.url)}</p><div class="foot" style="margin-top:10px"><button class="btn sm" data-s="rotate">Rotate access code</button></div>` : ""}
    <div class="foot"><span class="sp"></span><button class="btn pri" data-vc="1">Close</button></div>`);
}
async function settingsChange(t) {
  const k = t.dataset.s; if (!k) return;
  const patch = { aimode: { ai: { mode: t.value } }, ollama_model: { ai: { ollama_model: t.value } }, desktop: { alerts: { desktop: t.checked } }, topic: { alerts: { ntfy_topic: t.value.trim() } }, writeback: { writeback: t.checked }, hub: { hub: { enabled: t.checked } },
    digest_time: { alerts: { digest_time: t.value } }, draft_auto: { drafts: { auto: t.checked } }, draft_tone: { drafts: { tone: t.value } },
    draft_sig: { drafts: { signature: t.value } }, draft_min: { drafts: { min_priority: +t.value } }, draft_cap: { drafts: { daily_cap: +t.value || 0 } } }[k];
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
  const k = e.target.closest("[data-s]")?.dataset.s;
  if (k === "aitest") { const r = $("#aitres"); r.textContent = "Testing…"; try { const x = await rpc("test_ai"); r.innerHTML = `<span class="ok">Works · P${x.priority} · ${esc(x.action)}</span>` } catch (err) { r.innerHTML = `<span class="bad">${esc(String(err))}</span>` } }
  if (k === "alertcat") {
    const c = e.target.closest("[data-cat]").dataset.cat, cur = S.settings.alerts?.categories || [];
    const all = cur.length ? cur : [...ALERT_CATS];
    let next = all.includes(c) ? all.filter(x => x !== c) : [...all, c];
    if (next.length === ALERT_CATS.length || !next.length) next = [];
    try { S.settings = await rpc("save_settings", { patch: { alerts: { categories: next } } }); drawSettings() } catch (err) { toast("Couldn't save: " + err) }
  }
  if (k === "export") { try { await navigator.clipboard.writeText(await rpc("export_markdown")); toast("Copied as Markdown") } catch (err) { toast("Couldn’t copy: " + err) } }
  if (k === "rotate") { await rpc("rotate_hub_token"); toast("New access code. Scan the code again on your phone."); drawSettings() }
  if (k === "signout" && twice(e.target.closest("[data-s]"), "Click again: removes password and local list")) { await rpc("sign_out"); W = { ...W, step: 0, tested: null, pw: "" }; closeVeil() }
}

