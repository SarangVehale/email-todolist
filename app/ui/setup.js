"use strict";
/* ───────── dialogs: first-run wizard & settings ───────── */
/** The mail providers the app can guide a person through. `match` says which email domains are theirs. `page` is where their mail code is made: the address
 *  must be on the engine's list of pages it opens (an engine test reads every `page:` written in this file). `steps` are what to do there, in plain words. */
const PROVIDERS = {
  gmail: { name: "Gmail", maker: "Google", host: "imap.gmail.com", match: d => d === "gmail.com" || d === "googlemail.com", page: "https://myaccount.google.com/apppasswords",
    steps: ["Press the button above. Google's page opens in your web browser. Sign in if it asks you to.", "Type Inbox Agent as the name and press Create.", "Google shows a code of 16 letters. Copy it, then come back to this window."],
    tip: "If Google says it can't make a code, “2-Step Verification” has to be turned on first. Search for it on that page, turn it on, then press the button again." },
  icloud: { name: "iCloud Mail", maker: "Apple", host: "imap.mail.me.com", match: d => ["icloud.com", "me.com", "mac.com"].includes(d), page: "https://account.apple.com/account/manage",
    steps: ["Press the button above and sign in with your Apple ID.", "Open “Sign-In and Security”, then “App-Specific Passwords”, and press the + button.", "Type Inbox Agent as the name. Copy the code Apple shows, then come back to this window."] },
  yahoo: { name: "Yahoo Mail", maker: "Yahoo", host: "imap.mail.yahoo.com", match: d => ["ymail.com", "rocketmail.com"].includes(d) || (/^yahoo\.[a-z]{2,3}(\.[a-z]{2})?$/.test(d) && d !== "yahoo.co.jp"), page: "https://login.yahoo.com/account/security",
    steps: ["Press the button above and sign in to Yahoo.", "Look for “Generate app password” (or “App passwords”) on that page. Type Inbox Agent as the name and press Generate.", "Copy the code Yahoo shows, then come back to this window."] },
  fastmail: { name: "Fastmail", maker: "Fastmail", host: "imap.fastmail.com", match: d => ["fastmail.com", "fastmail.fm", "messagingengine.com"].includes(d), page: "https://app.fastmail.com/settings/security/integrations",
    steps: ["Press the button above and sign in to Fastmail.", "Press “New app password”, type Inbox Agent as the name, and choose “Mail (IMAP/SMTP)”.", "Copy the code Fastmail shows, then come back to this window."] },
  outlook: { name: "Outlook, Hotmail or Live", maker: "Microsoft", host: "outlook.office365.com", match: d => /^(outlook|hotmail|live)\.[a-z]{2,3}(\.[a-z]{2})?$/.test(d) || d === "msn.com", page: null,
    steps: ["Microsoft has turned off password sign-in for IMAP, so a password or app password will be refused. Sign-in with Microsoft is not supported in this version.", "Gmail, iCloud, Yahoo, Fastmail and most other IMAP providers work with an app password."] },
  other: { name: "Other IMAP server", maker: "", host: "", match: null, page: null, steps: ["Enter your server and an app password if your provider offers one."] },
};
/** What the provider is called in the menu of the Add a mailbox dialog: Outlook says it is not supported unless this build can sign in with Microsoft. */
const providerLabel = k => k === "outlook" && !(S.sign_in && S.sign_in.microsoft) ? PROVIDERS[k].name + " (not supported yet)" : PROVIDERS[k].name;
/** Which provider an email address belongs to: a key of PROVIDERS, or "other" when it is not one the app knows. */
function providerFor(address) {
  const d = domainOf(address);
  for (const [k, p] of Object.entries(PROVIDERS)) if (p.match && p.match(d)) return k;
  return "other";
}
/** The part of an address after its last @, in lower case. */
const domainOf = a => String(a).slice(String(a).lastIndexOf("@") + 1).trim().toLowerCase();
/** Does this look like name@example.com? Only a check on typing: the mail server is the judge of whether the mailbox exists. */
function validAddress(a) {
  a = String(a).trim();
  if (a.length > 320 || /[\s<>(),;:"\\]/.test(a)) return false;
  const i = a.indexOf("@");
  return i > 0 && /^[^@.][^@]*\.[^@.]{2,}$/.test(a.slice(i + 1)); // the part after the first @ may hold no @ at all
}
/** What went wrong, as a sentence a person can act on. `raw` is the engine's own text: precise, and what the log and a helper need, so it stays behind
 *  "Details". An engine message that is already a plain sentence (it starts with a capital and has no colon, bracket or system error number) is shown as it is. */
function friendly(raw, providerKey, host, how) {
  raw = String(raw).replace(/^Error: /, "");
  const l = raw.toLowerCase(), known = providerKey && providerKey !== "other" && PROVIDERS[providerKey], name = known ? known.name : "your mail server", maker = known ? known.maker : "your provider";
  const say = s => ({ say: s, detail: raw });
  if (/^[A-Z][^:[\]]*$/.test(raw) && !/os error/i.test(raw)) return { say: raw, detail: "" };
  if (/web login required|log in via your web browser/.test(l)) return say(`${name} wants you to sign in on its website first. Open ${name} in your web browser, sign in there, then try again here.`);
  if (/imap access is disabled|imap is disabled|imap.*not enabled|imap.*turned off/.test(l)) return say(`${name} says other mail programs are switched off for this account. Look for "IMAP" in ${name}'s mail settings, turn it on, then try again.`);
  if (/too many|rate limit|try again later|temporarily|throttl/.test(l)) return say(`${name} asks you to wait a little. Try again in a few minutes.`);
  if (how === "account" && /sign-in refused|authenticationfailed|invalid credentials|login failed|authenticate failed/.test(l)) return say(`${maker} did not let Inbox Agent in with that account. Make sure you signed in with the address you typed, then try again.`);
  if (/sign-in refused|authenticationfailed|invalid credentials|login failed|authenticate failed|application-specific password|app password/.test(l))
    return say(known ? `That code didn't work. Make a new one with the button above and paste it here. Your usual ${maker} password won't work.` : "The server didn't accept that sign-in. Check the email address and the password. Many providers need an app password instead of your usual one.");
  if (/could not find the server|no such host|failed to lookup|name or service not known|nodename nor servname|os error 11001|os error 11004/.test(l))
    return say(known ? `Couldn't reach ${name}. Check that this computer is online, then try again.` : `Couldn't find a mail server called "${host || "that"}". Check how it is spelled.`);
  if (/timed out|timeout|did not properly respond|os error 10060|os error 110\b/.test(l)) return say(`${name} took too long to answer. Check that this computer is online, then try again.`);
  if (/actively refused|connection refused|os error 10061|os error 111\b/.test(l)) return say(known ? `Couldn't connect to ${name}. Check that this computer is online, then try again.` : `Couldn't connect to "${host || "that server"}". Check the server name and the port.`);
  if (/secure connection failed|certificate|tls|ssl|handshake/.test(l)) return say(`A safe connection to ${name} couldn't be made. This can happen on a work or school network, or when the date or time on this computer is wrong. Check the clock, or try another network.`);
  if (/network is unreachable|no route|os error 10051|os error 10065|os error 101\b|os error 113\b/.test(l)) return say("This computer doesn't seem to be online. Check your internet, then try again.");
  return say("Something went wrong while connecting. Wait a minute and try again.");
}
/** A person's answer as markup: a sentence, and the engine's own words behind "Details for a helper". */
const sayHTML = r => !r ? "" : r.ok ? `<p class="ok">${esc(r.ok)}</p>${r.device ? `<p class="devcode">Your code: <b>${esc(r.device.code)}</b></p><p>Type it on Microsoft's page: <b>${esc(String(r.device.url).replace(/^https:\/\//, ""))}</b> <button class="btn sm" data-w="opendevice" data-m="opendevice" data-s="opendevice" data-url="${esc(r.device.url)}">Open that page</button></p>` : ""}${r.cancel ? '<button class="btn sm" data-w="cancelsignin" data-m="cancelsignin" data-s="cancelsignin">Cancel</button>' : ""}` : `<p class="bad">${esc(r.say)}</p>${r.detail ? `<details class="why"><summary>Details for a helper</summary><code>${esc(r.detail)}</code></details>` : ""}`;
/** What setup makes sure of: the mailbox is not changed (no filing) and the phone hub is off. Ranking and alerts are left as they are: a new install already has rules and desktop alerts, and a person who set them up again after removing a mailbox keeps their choices. */
const SAFE_START = { writeback: false, hub: { enabled: false } };
/** Which account sign-in this provider of the first-run screens has and this build offers: "google", "microsoft" or "". */
const signInOffer = k => { const p = k === "gmail" ? "google" : k === "outlook" ? "microsoft" : ""; return p && S.sign_in && S.sign_in[p] ? p : "" };
const PROVIDER_NAME = { google: "Google", microsoft: "Microsoft" };
/** The state of the first-run screens. The default is the screen a new install starts on. */
const freshWizard = () => ({ flow: 0, step: "welcome", provider: "other", host: "", hostFor: "", port: 993, user: "", pw: "", show: false, busy: false, result: null, autostart: true });
let W = freshWizard();
const veil = $("#veil");
function closeVeil() {
  const back = typeof veilFrom !== "undefined" ? veilFrom : null;
  veil.classList.remove("show"); veil.innerHTML = ""; veil.dataset.kind = ""; lastSnap = ""; render(); load();
  setTimeout(() => { const t = back && back.isConnected ? back : document.querySelector(".row.cur .mid, #bar nav button.on"); t?.focus?.() }, 40);
}
const rand = n => Array.from(crypto.getRandomValues(new Uint8Array(n)), b => "abcdefghjkmnpqrstuvwxyz23456789"[b % 31]).join("");
const panel = inner => `<div class="panel" role="dialog" aria-modal="true" aria-label="Inbox Agent">${inner}</div>`;
/** What a passed connection test says. A server with no Sent folder we can find (`sent` is null) is still a mailbox, and the test says what is lost.
 *  The engine's own words are not used here: it answers `sent: null`, not a sentence. */
const testedOk = t => `<span class="ok">Connected · ${t.sent ? "Sent folder: " + esc(t.sent) : "no Sent folder found, so replies you send will not close tasks and there are no follow-up reminders"}</span>`;
/** Draws a dialog's panel under its name (a redraw, such as the next wizard step, must keep it: a screen reader announces the name). */
function showPanel(name, inner, cls = "") { veil.innerHTML = panel(inner); const p = veil.querySelector(".panel"); p.setAttribute("aria-label", name); if (cls) p.classList.add(cls); labelControls(veil) }
/** A control is read out by its label, not by the text drawn near it: a label with no `for` names the control after it (the next sibling, or the first
 *  control inside it). Without this the fields and menus of Settings that are drawn as label-then-control are just "edit" and "combo box" to a screen reader. */
function labelControls(root) {
  let n = 0;
  for (const l of root.querySelectorAll("label:not([for])")) {
    if (l.querySelector("input, select, textarea")) continue; // a label that wraps its control names it already
    const next = l.nextElementSibling, c = next?.matches("input, select, textarea") ? next : next?.querySelector("input, select, textarea");
    if (!c) continue;
    if (!c.id) c.id = "lc" + (++n);
    l.htmlFor = c.id;
  }
}

function openWizard() { veil.dataset.kind = "wizard"; veil.classList.add("show"); drawWizard(); dialogOpened("Set up Inbox Agent") }
/** The first-run screens: welcome, the email address, the provider's page and a place to paste its code, and a last screen. Each is one thing to do.
 *  A provider the app does not know gets the plain server form instead; Outlook, Hotmail and Live get an honest sentence. */
const WIZARD_STEP = { welcome: 1, address: 2, guide: 3, other: 3, outlook: 3 }, WIZARD_BACK = { address: "welcome", guide: "address", other: "address", outlook: "address" };
const pasteKeys = () => /Mac|iPhone|iPad/.test(navigator.platform || "") ? "Command and V" : "Ctrl and V";
function drawWizard() {
  const n = W.step, p = PROVIDERS[W.provider] || PROVIDERS.other, no = WIZARD_STEP[n], b = W.busy ? "disabled" : "";
  const back = `<button class="btn" data-w="back" ${b}>Back</button>`, connect = `<button class="btn pri big" data-w="connect" ${b}>Connect</button>`;
  let h = no ? `<div class="step">Step ${no} of 3</div>` : "", foot = "";
  if (n === "welcome") {
    h += `<h2>Welcome.</h2><p>Inbox Agent reads your email and makes one short list of what needs you.</p><p>It never sends email, and it never changes your mailbox unless you ask it to.</p>
      <div class="help"><b>Private.</b> Your email is read on this computer only. Your password is kept in your computer's password safe, never in a file.</div>`;
    foot = `<span class="sp"></span><button class="btn pri big" data-w="next" data-focus>Get started</button>`;
  } else if (n === "address") {
    h += `<h2>What is your email address?</h2><p>Type the address of the mailbox you want Inbox Agent to read.</p>
      <label class="f" for="w-user">Email address</label><input id="w-user" type="text" data-f="user" data-focus value="${esc(W.user)}" inputmode="email" autocapitalize="off" spellcheck="false" autocomplete="off" placeholder="name@example.com">
      <p id="w-hint" role="status">${esc(addressHint(W.user))}</p>`;
    foot = `${back}<span class="sp"></span><button class="btn pri big" data-w="next" ${validAddress(W.user) ? "" : "disabled"}>Continue</button>`;
  } else if (n === "guide") {
    const prov = signInOffer(W.provider);
    h += `<h2>${prov ? "Connect your Gmail" : "Make a code for Inbox Agent"}</h2>${prov ? `<button class="btn pri big" data-w="signin" data-provider="${prov}" data-focus>Sign in with ${PROVIDER_NAME[prov]}</button><p>Your web browser opens a page where you sign in. There is nothing to copy.</p><div class="help"><b>Or use a code.</b> That works for everyone.</div>` : ""}<p>${esc(p.maker)} makes a special 16-letter code just for this app. It is safer than using your own password, and you can cancel it any time.</p>
      <button class="btn ${prov ? "" : "pri"} big" data-w="open" ${prov ? "" : "data-focus"}>Open ${esc(p.maker)}'s page</button>
      <ol class="steps">${p.steps.map(x => `<li>${esc(x)}</li>`).join("")}</ol>${p.tip ? `<div class="help">${esc(p.tip)}</div>` : ""}
      <label class="f" for="w-pw">The code</label>
      <div class="pastebox"><input id="w-pw" type="${W.show ? "text" : "password"}" data-f="pw" value="${esc(W.pw)}" autocapitalize="off" spellcheck="false" autocomplete="off" placeholder="abcd efgh ijkl mnop"><button class="btn" data-w="paste">Paste</button></div>
      <label class="chk2"><input type="checkbox" data-c="show" ${W.show ? "checked" : ""}> Show the code</label>
      <div id="w-result" role="status" aria-live="polite">${sayHTML(W.result)}</div>`;
    foot = `${back}<span class="sp"></span>${connect}`;
  } else if (n === "other") {
    h += `<h2>Your mail server</h2><p>${W.provider === "outlook" ? "You chose to set this up by hand." : `Inbox Agent doesn't know ${esc(domainOf(W.user))} yet, so it needs a few details.`} Your email provider's help pages list them under "IMAP settings".</p>
      <div class="row2"><div><label class="f" for="w-host">Mail server</label><input id="w-host" type="text" data-f="host" value="${esc(W.host)}" placeholder="imap.example.com" autocapitalize="off" spellcheck="false"></div><div><label class="f" for="w-port">Port</label><input id="w-port" type="number" data-f="port" value="${W.port}"></div></div>
      <label class="f" for="w-pw">App password</label>
      <div class="pastebox"><input id="w-pw" type="${W.show ? "text" : "password"}" data-f="pw" data-focus value="${esc(W.pw)}" autocapitalize="off" spellcheck="false" autocomplete="off"><button class="btn" data-w="paste">Paste</button></div>
      <label class="chk2"><input type="checkbox" data-c="show" ${W.show ? "checked" : ""}> Show the password</label>
      <p>Use an app password if your provider offers one. Your usual password may be refused.</p>
      <div id="w-result" role="status" aria-live="polite">${sayHTML(W.result)}</div>`;
    foot = `${back}<span class="sp"></span>${connect}`;
  } else if (n === "outlook" && signInOffer("outlook")) {
    h += `<h2>Outlook, Hotmail and Live</h2><p>Sign in with your Microsoft account. A page opens, and Microsoft shows you a short code to type there.</p>
      <button class="btn pri big" data-w="signin" data-provider="microsoft" data-focus>Sign in with Microsoft</button>
      <div id="w-result" role="status" aria-live="polite">${sayHTML(W.result)}</div>`;
    foot = `${back}<span class="sp"></span><button class="btn sm" data-w="manual">I know what I'm doing: set it up by hand</button>`;
  } else if (n === "outlook") {
    h += `<h2>Outlook, Hotmail and Live</h2><p>Microsoft does not let an app like this one sign in to these addresses with a code, so Inbox Agent cannot read this mailbox yet.</p>
      <p>Gmail, iCloud, Yahoo, Fastmail and most other providers work. If you have another address, use that one.</p>`;
    foot = `${back}<span class="sp"></span><button class="btn sm" data-w="manual">I know what I'm doing: set it up by hand</button>`;
  } else if (n === "ready") {
    h += `<h2>You're connected.</h2><p>Inbox Agent is reading your email now. The first time can take a few minutes. Your list fills in by itself.</p>
      <div class="help"><b>It keeps watching.</b> If you close this window, Inbox Agent keeps running. Its icon near the clock brings it back.</div>
      <label class="sw"><input type="checkbox" data-c="autostart" ${W.autostart ? "checked" : ""}><div><b>Open Inbox Agent when I turn on this computer</b><span>So new important mail never waits for you to start it.</span></div></label>`;
    foot = `<span class="sp"></span><button class="btn pri big" data-w="finish" data-focus>Open my list</button>`;
  }
  showPanel("Set up Inbox Agent", h + `<div class="foot">${foot}</div>`, "wz");
  veil.querySelector("[data-focus]")?.focus();
}
/** The line under the address box: what the app makes of it, or what is missing. Nothing while the person is still typing the name. */
function addressHint(a) {
  a = String(a).trim();
  if (validAddress(a)) { const k = providerFor(a); return k === "other" ? "I don't know that provider yet, but it may still work." : `That looks like a ${PROVIDERS[k].name.replace(/ \(.*/, "")} address.` }
  return a.includes("@") ? "An address looks like name@example.com" : "";
}
/** Shows an answer under the code box without redrawing the screen, so the keyboard stays where it was and a screen reader reads it out. */
function wizardSay(r) { W.result = r; const el = $("#w-result"); if (el) { el.innerHTML = sayHTML(r); el.scrollIntoView?.({ block: "center" }) } }
/** Tests what was typed, saves the mailbox with the safe defaults and moves to the last screen. One button does all of it: nobody should have to find "Test". */
async function wizardConnect() {
  if (W.busy) return;
  const acct = { host: W.host.trim(), port: W.port, user: W.user.trim(), password: W.pw.trim() }, name = PROVIDERS[W.provider]?.name || "your mail server";
  if (!acct.password.trim()) return wizardSay({ say: W.provider === "other" ? "Type the password first." : "Paste the code first." });
  W.busy = true; for (const x of veil.querySelectorAll(".foot .btn")) x.disabled = true;
  wizardSay({ ok: `Connecting to ${name.replace(/ \(.*/, "")}. This can take up to half a minute.` });
  try {
    await rpc("test_account", acct);
    await rpc("save_account", acct);
    await rpc("save_settings", { patch: SAFE_START });
    await rpc("finish_setup");
    await setupDone();
    W.pw = ""; W.busy = false; W.result = null; W.step = "ready"; drawWizard();
  } catch (err) {
    W.busy = false; for (const x of veil.querySelectorAll(".foot .btn")) x.disabled = false;
    wizardSay(friendly(String(err), W.provider, acct.host));
    veil.querySelector("#w-pw")?.focus();
  }
}
/** The first mailbox is connected: setup is over. A new install gets the simple view now (not only when the last screen is closed: the app may be closed before that),
 *  and starts with the computer unless that is unticked. */
async function setupDone() {
  if (lookGet("easy") === null) { lookSet("easy", "1"); applyLook(); render() }
  if (TA) await TA.core.invoke(W.autostart ? "plugin:autostart|enable" : "plugin:autostart|disable").catch(() => {});
}
async function openDevicePage(btn) { try { await rpc("open_page", { url: btn.closest("[data-url]").dataset.url }) } catch { toast("Couldn't open your web browser. Type the address into it.") } }
/** SIGNIN_POLL: how often the window asks whether the sign-in has finished (a test makes it short). */
let SIGNIN_POLL = 1500, signInSeq = 0;
/** Signs in with a Google or Microsoft account, start to finish: starts it, shows what the person has to do (`say`), waits for the answer, and saves the mailbox
 *  with `finishArgs`. `alive()` goes false when the person has left or cancelled: the sign-in is then called off. Resolves to the mailbox number, or throws the sentence. */
async function signInFlow(provider, user, finishArgs, say, alive) {
  const mine = ++signInSeq;
  const r = await rpc("oauth_start", { provider, user });
  say(r.kind === "microsoft" ? { ok: "Microsoft's page is open in your web browser. Type this code there, then sign in.", device: { code: r.code, url: r.url } } : { ok: `${PROVIDER_NAME[provider]}'s page is open in your web browser. Sign in there, then come back here.` });
  for (;;) {
    await new Promise(res => setTimeout(res, SIGNIN_POLL));
    if (!alive()) { if (signInSeq === mine) rpc("oauth_cancel").catch(() => {}); throw ""; } // (a newer sign-in is not to be cancelled by this one giving up)
    const p = await rpc("oauth_poll");
    if (p.state === "done") break;
    if (p.state === "failed") throw new Error(p.error);
    if (p.state === "idle") throw new Error("The sign-in was cancelled.");
  }
  if (!alive()) { if (signInSeq === mine) rpc("oauth_cancel").catch(() => {}); throw ""; } // cancelled while the answer was on its way: nothing is saved
  say({ ok: "Signed in. Connecting to your mailbox…", final: true }); // too late to cancel: the mailbox is being saved
  return rpc("oauth_finish", finishArgs);
}
/** The first-run screens: sign in with an account, then finish setup the same way Connect does. */
async function wizardSignIn(provider) {
  if (W.busy) return;
  const my = ++W.flow;
  W.busy = true; for (const x of veil.querySelectorAll(".foot .btn, [data-w=signin], [data-w=open], [data-w=paste]")) x.disabled = true;
  const unlock = () => { W.busy = false; for (const x of veil.querySelectorAll("[data-w]")) x.disabled = false };
  try {
    await signInFlow(provider, W.user.trim(), {}, r => { if (W.flow === my) wizardSay({ ...r, cancel: !r.final }) }, () => W.flow === my && veil.dataset.kind === "wizard");
    if (W.flow !== my) return;
    await rpc("save_settings", { patch: SAFE_START });
    await rpc("finish_setup");
    await setupDone();
    W.busy = false; W.pw = ""; W.result = null; W.step = "ready"; drawWizard();
  } catch (err) {
    if (W.flow !== my) return; // cancelled: the screen has already been told
    unlock();
    wizardSay(friendly(String(err), provider === "google" ? "gmail" : "outlook", "", "account"));
  }
}
veil.addEventListener("input", e => {
  const f = e.target.dataset.f; if (!f || veil.dataset.kind !== "wizard") return;
  W[f] = e.target.type === "number" ? +e.target.value : e.target.value;
  if (f === "user") { const h = $("#w-hint"); if (h) h.textContent = addressHint(W.user); const nx = veil.querySelector("[data-w=next]"); if (nx) nx.disabled = !validAddress(W.user) }
  if (["user", "pw", "host", "port"].includes(f)) { W.result = null; const r = $("#w-result"); if (r) r.innerHTML = "" }
});
veil.addEventListener("keydown", e => {
  if (veil.dataset.kind !== "wizard" || e.key !== "Enter" || e.target.tagName !== "INPUT" || e.target.type === "checkbox") return;
  e.preventDefault(); veil.querySelector(".foot .btn.pri:not([disabled])")?.click();
});
veil.addEventListener("change", e => {
  const t = e.target;
  if (veil.dataset.kind === "wizard") {
    if (t.dataset.c === "show") { W.show = t.checked; drawWizard(); veil.querySelector("[data-c=show]")?.focus() }
    else if (t.dataset.c) { W[t.dataset.c] = t.checked; if (t.dataset.c === "autostart" && TA) TA.core.invoke(t.checked ? "plugin:autostart|enable" : "plugin:autostart|disable").catch(() => {}) }
  } else if (veil.dataset.kind === "settings") settingsChange(t);
});
veil.addEventListener("click", async e => {
  if (e.target.closest("[data-vc]") || e.target === veil && veil.dataset.kind === "settings") return closeVeil();
  if (veil.dataset.kind === "settings") return settingsClick(e);
  if (veil.dataset.kind === "mailbox") return mailboxClick(e);
  if (veil.dataset.kind === "restore") return restoreClick(e);
  const w = e.target.closest("[data-w]")?.dataset.w;
  if (w === "cancelsignin") { W.flow++; W.busy = false; rpc("oauth_cancel").catch(() => {}); for (const x of veil.querySelectorAll("[data-w]")) x.disabled = false; return wizardSay({ ok: "Cancelled. Nothing was connected." }) }
  if (w === "opendevice") return openDevicePage(e.target.closest("[data-w]"));
  if (!w || W.busy) return;
  if (w === "signin") return wizardSignIn(e.target.closest("[data-provider]").dataset.provider);
  if (w === "back") { W.step = WIZARD_BACK[W.step] || "welcome"; W.result = null; drawWizard() }
  if (w === "next" && W.step === "welcome") { W.step = "address"; drawWizard() }
  else if (w === "next" && W.step === "address" && validAddress(W.user)) {
    const k = providerFor(W.user), d = domainOf(W.user);
    W.user = W.user.trim(); W.provider = k; W.port = 993; W.result = null;
    if (W.hostFor !== d) { W.hostFor = d; W.pw = ""; W.host = PROVIDERS[k].host || "imap." + d }
    W.step = k === "outlook" ? "outlook" : k === "other" ? "other" : "guide"; drawWizard();
  }
  if (w === "manual") { W.provider = "other"; W.host = W.host || "imap." + domainOf(W.user); W.step = "other"; drawWizard() }
  if (w === "open") {
    const p = PROVIDERS[W.provider];
    try { await rpc("open_page", { url: p.page }); wizardSay({ ok: `${p.maker}'s page should be open in your web browser. Switch to it, make the code, then come back here.` }) }
    catch (err) { wizardSay({ say: `Couldn't open your web browser. Open it yourself and go to ${p.page.replace(/^https:\/\//, "")}`, detail: String(err).replace(/^Error: /, "") }) }
  }
  if (w === "paste") {
    let t = ""; try { t = ((await navigator.clipboard.readText()) || "").trim() } catch { /* the browser may refuse; the box still takes a pasted code */ }
    if (!t) return wizardSay({ say: `Couldn't read what you copied. Click in the box and press ${pasteKeys()} together.` });
    W.pw = t; const box = $("#w-pw"); if (box) box.value = t;
    wizardConnect();
  }
  if (w === "connect") wizardConnect();
  if (w === "finish") closeVeil();
});

const ALERT_CATS = ["work", "personal", "finance", "travel", "notification", "promo", "reading", "other"];
let moreSettings = false, resignFlow = 0;
function openSettings() { moreSettings = false; rulesDraft = null; rulesDirty = false; veil.dataset.kind = "settings"; veil.classList.add("show"); drawSettings().then(() => dialogOpened("Settings")) }

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
function mailStatus(st, auth) {
  st = String(st || "");
  if (st === "idle") return '<span class="ok">Connected</span>';
  if (st === "syncing" || st === "connecting") return "Reading mail…";
  if (st === "no password") return `<span class="bad">${auth ? "Needs you to sign in again" : easy() ? "Needs a new code. Paste it below." : "Needs a new app password"}</span>`;
  if (st.startsWith("error")) return `<span class="bad">${esc(st.replace(/^error:\s*/, "").slice(0, 90))}</span>`;
  return esc(st);
}
function mailboxesHTML(s) {
  const mbs = (S.accounts && S.accounts.length) ? S.accounts : [{ id: 0, user: s.account?.user, host: s.account?.host, status: S.status }];
  return mbs.map(m => `<div class="mbx" data-id="${m.id}"><p><b>${esc(m.user)}</b> · ${esc(m.host)} · ${mailStatus(m.status, m.auth)}</p>
    ${m.auth ? `<p>Signed in with ${PROVIDER_NAME[m.auth] || "an account"}. No password is kept.</p>` : `<label class="f" for="s-pw${m.id}">${easy() ? "Paste a new code if the old one stopped working" : "New app password"}</label><input id="s-pw${m.id}" type="password" data-s="newpw" data-id="${m.id}" autocomplete="off" placeholder="${easy() ? "paste the code here" : "paste a new app password"}">`}
    <div class="foot" style="margin-top:8px">${m.auth ? `<button class="btn sm" data-s="resign" data-id="${m.id}" data-provider="${esc(m.auth)}">Sign in again</button>` : `<button class="btn sm" data-s="savepw" data-id="${m.id}">${easy() ? "Save code" : "Save password"}</button>`}<span data-pwres="${m.id}"></span><span class="sp"></span>${mbs.length > 1 ? `<button class="btn sm" data-s="rmacct" data-id="${m.id}">Remove</button>` : ""}</div>
    ${m.auth ? `<details class="why"><summary>Use a code instead of signing in</summary><label class="f" for="s-pw${m.id}">${easy() ? "Paste the code" : "App password"}</label><input id="s-pw${m.id}" type="password" data-s="newpw" data-id="${m.id}" autocomplete="off" placeholder="${easy() ? "paste the code here" : "paste an app password"}"><div class="foot" style="margin-top:8px"><button class="btn sm" data-s="savepw" data-id="${m.id}">${easy() ? "Save code" : "Save password"}</button></div></details>` : ""}</div>`).join("");
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
  showPanel("Add a mailbox", `<h2>Add a mailbox</h2><p>Its mail joins the same list. A small tag on each task says which mailbox it came to. ${n} of 5 in use.</p>
    <label class="f" for="m-provider">Provider</label><select id="m-provider" data-f="provider">${Object.entries(PROVIDERS).map(([k]) => `<option value="${k}" ${MB.provider === k ? "selected" : ""}>${providerLabel(k)}</option>`).join("")}</select>
    ${signInOffer(MB.provider) ? `<button class="btn pri" data-m="signin" data-provider="${signInOffer(MB.provider)}" ${MB.busy ? "disabled" : ""}>Sign in with ${PROVIDER_NAME[signInOffer(MB.provider)]}</button><div id="msignin" role="status" aria-live="polite"></div><p>Or use a code, as below.</p>` : ""}
    <div class="help"><b>Making a code</b>${p.page ? ` <button class="mini" data-m="open">Open ${esc(p.maker)}'s page</button>` : ""}<ol>${(MB.provider === "outlook" && signInOffer("outlook") ? ["Press the button above. Your web browser opens Microsoft's page, and this window shows a code to type there."] : p.steps).map(x => `<li>${esc(x)}</li>`).join("")}</ol></div>
    ${MB.provider === "other" ? `<div class="row2"><div><label class="f" for="m-host">IMAP server</label><input id="m-host" type="text" data-f="host" value="${esc(MB.host)}" placeholder="imap.example.com"></div><div><label class="f" for="m-port">Port</label><input id="m-port" type="number" data-f="port" value="${MB.port}"></div></div>` : ""}
    <label class="f" for="m-user">Email address</label><input id="m-user" type="text" data-f="user" value="${esc(MB.user)}" autocomplete="off" placeholder="you@example.com">
    <label class="f" for="m-pw">App password</label><input id="m-pw" type="password" data-f="pw" value="${esc(MB.pw)}" autocomplete="off">
    <div class="foot"><button class="btn sm" id="m-test" data-m="test" ${MB.testing ? "disabled" : ""}>${MB.testing ? "Testing…" : "Test connection"}</button><span id="mtestres">${MB.tested ? (MB.tested.ok ? testedOk(MB.tested) : `<span class="bad">${esc(MB.tested.error)}</span>`) : ""}</span></div>
    ${MB.tested && !MB.tested.ok && MB.tested.detail ? `<details class="why"><summary>Details for a helper</summary><code>${esc(MB.tested.detail)}</code></details>` : ""}
    <div class="foot"><span class="sp"></span><button class="btn" data-m="cancel">Cancel</button><button class="btn pri" id="m-add" data-m="add" ${MB.tested?.ok && !MB.busy ? "" : "disabled"}>Add mailbox</button></div>`);
  if (focused) veil.querySelector("#" + focused)?.focus(); // a redraw must not drop the keyboard onto the page
}
/** The Add a mailbox dialog: sign in with an account, then add the mailbox. */
async function mailboxSignIn(mb, prov) {
  if (mb.busy) return;
  const out = r => { const o = $("#msignin"); if (o) o.innerHTML = sayHTML(r) };
  if (!validAddress(mb.user)) return out({ say: "Type your email address first.", detail: "" });
  const my = mb.flow = (mb.flow || 0) + 1, live = () => MB === mb && mb.flow === my;
  mb.busy = true; drawMailbox();
  try {
    await signInFlow(prov, mb.user.trim(), { add: true }, r => { if (live()) out({ ...r, cancel: !r.final }) }, live);
    clearReadCaches(); lastSnap = ""; await load(); toast("Mailbox added. Its mail is being read.");
    if (live()) { MB = null; veil.dataset.kind = "settings"; drawSettings().then(() => veil.querySelector("[data-s=addacct]")?.focus()) }
  } catch (err) {
    if (!live()) return;
    mb.busy = false; drawMailbox(); out(friendly(String(err), prov === "google" ? "gmail" : "outlook", "", "account"));
  }
}
async function mailboxClick(e) {
  const m = e.target.closest("[data-m]")?.dataset.m, mb = MB; if (!m || !mb) return;
  if (m === "cancel") return mailboxBack();
  if (m === "opendevice") return openDevicePage(e.target.closest("[data-m]"));
  if (m === "cancelsignin") { mb.flow = (mb.flow || 0) + 1; mb.busy = false; rpc("oauth_cancel").catch(() => {}); const o = $("#msignin"); if (o) o.innerHTML = '<p class="ok">Cancelled. Nothing was added.</p>'; return drawMailbox() }
  if (m === "signin") return mailboxSignIn(mb, e.target.closest("[data-provider]").dataset.provider);
  if (m === "open") { const p = PROVIDERS[mb.provider]; try { await rpc("open_page", { url: p.page }) } catch (err) { toast("Couldn't open your web browser. Go to " + p.page.replace(/^https:\/\//, "")) } return }
  const here = () => MB === mb; // Cancel or Escape while a request is out leaves nothing to draw on when it comes back
  if (m === "test") {
    mb.testing = true; mb.tested = null; drawMailbox();
    try { const r = await rpc("test_account", { host: mb.host, port: mb.port, user: mb.user, password: mb.pw }); mb.tested = { ok: true, sent: r.sent } }
    catch (err) { const r = friendly(String(err), mb.provider, mb.host); mb.tested = { ok: false, error: r.say, detail: r.detail } }
    mb.testing = false; if (here()) drawMailbox();
  }
  if (m === "add") {
    if (mb.busy) return; mb.busy = true; drawMailbox();
    try { await rpc("add_account", { host: mb.host, port: mb.port, user: mb.user, password: mb.pw }) }
    catch (err) { mb.busy = false; const r = friendly(String(err), mb.provider, mb.host); mb.tested = { ok: false, error: r.say, detail: r.detail }; if (here()) drawMailbox(); return }
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
  showPanel("Restore from a backup", `<h2>Restore from a backup</h2><p>${esc(rs.name)}</p>${body}
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
/** "How it looks": the text size, the simple view and the light colours. They belong to this device, so a phone and a laptop can differ. */
function lookHTML() {
  const size = textSize();
  return `<div class="sect">How it looks</div><div class="f" id="sz-l">Text size</div>
    <div class="szrow" role="group" aria-labelledby="sz-l">${[["normal", "Normal"], ["large", "Large"], ["huge", "Huge"]].map(([k, l]) => `<button class="szb" data-s="size" data-size="${k}" aria-pressed="${size === k}">${l}</button>`).join("")}</div>
    <label class="sw"><input type="checkbox" data-s="easy" ${easy() ? "checked" : ""}><div><b>Simple view</b><span>Big buttons and plain words. Turn it off to see everything the app can do.</span></div></label>
    <label class="sw"><input type="checkbox" data-s="light" ${themeOf() === "light" ? "checked" : ""}><div><b>Light colours</b><span>Dark text on a light page. Off is light text on black.</span></div></label>
    <p>These choices are for this device only.</p>`;
}
/** Settings in the simple view: the look, the mailboxes, alerts and starting with the computer; everything else is under "More options". */
async function basicSettingsHTML(s, a) {
  const auto = TA ? await TA.core.invoke("plugin:autostart|is_enabled").catch(() => null) : null;
  return `<h2>Settings</h2>${lookHTML()}
    <div class="sect">Your mailboxes</div>${mailboxesHTML(s)}
    <div class="foot" style="margin-top:12px"><button class="btn sm" data-s="addacct" ${(S.accounts || []).length >= 5 ? "disabled" : ""}>Add another mailbox</button></div>
    <div class="sect">Alerts</div>
    <label class="sw"><input type="checkbox" data-s="desktop" ${a.desktop ? "checked" : ""}><div><b>Tell me when something important arrives</b><span>A small message appears near the clock.</span></div></label>
    ${typeof auto === "boolean" ? `<label class="sw"><input type="checkbox" data-s="autostart" ${auto ? "checked" : ""}><div><b>Open Inbox Agent when I turn on this computer</b></div></label>` : ""}
    <div class="foot" style="margin-top:22px"><button class="btn sm" data-s="moresettings">More options</button><span class="sp"></span><button class="btn pri" data-vc="1">Close</button></div>`;
}
async function drawSettings() {
  const s = S.settings, a = s.alerts || {}, ai = s.ai || {}, dr = s.drafts || { auto: false, tone: "friendly", signature: "", min_priority: 3, daily_cap: 25 };
  if (easy() && !moreSettings) { showPanel("Settings", await basicSettingsHTML(s, a)); return }
  let hub = null; if (s.hub?.enabled) hub = await rpc("hub_info").catch(() => null);
  showPanel("Settings", `<h2>Settings</h2>${easy() ? '<div class="foot" style="margin:0 0 6px"><button class="btn sm" data-s="moresettings">Back to simple settings</button></div>' : ""}${lookHTML()}
    <div class="sect">Mail</div>
    ${mailboxesHTML(s)}
    <p>Providers cancel app passwords whenever you change your account password. Pasting a new one here keeps your list, notes and tasks.</p>
    <div class="foot" style="margin-top:12px"><button class="btn sm" data-s="addacct" ${(S.accounts || []).length >= 5 ? "disabled" : ""}>Add another mailbox</button><span class="sp"></span><button class="btn sm" data-s="signout">Sign out and reset</button></div>
    <div class="sect">Ranking</div>
    <select data-s="aimode" aria-label="Ranking mode"><option value="rules" ${ai.mode === "rules" ? "selected" : ""}>Rules only</option><option value="ollama" ${ai.mode === "ollama" ? "selected" : ""}>Local AI (Ollama)</option><option value="anthropic" ${ai.mode === "anthropic" ? "selected" : ""}>Claude (own API key)</option></select>
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
  if (k === "easy") { lookSet("easy", t.checked ? "1" : "0"); applyLook(); render(); drawSettings(); return }
  if (k === "light") { lookSet("theme", t.checked ? "light" : "dark"); applyLook(); drawSettings(); return }
  if (k === "autostart") { if (TA) await TA.core.invoke(t.checked ? "plugin:autostart|enable" : "plugin:autostart|disable").catch(() => { toast("Couldn't change that"); drawSettings() }); return }
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
  if (k === "size") { lookSet("size", e.target.closest("[data-size]").dataset.size); applyLook(); drawSettings(); return }
  if (k === "moresettings") { moreSettings = !moreSettings; drawSettings(); return }
  if (k === "opendevice") return openDevicePage(e.target.closest("[data-s]"));
  if (k === "cancelsignin") { resignFlow++; rpc("oauth_cancel").catch(() => {}); const o = veil.querySelector("[data-pwres]:not(:empty)"); if (o) o.innerHTML = "Cancelled."; for (const b of veil.querySelectorAll("[data-s=resign]")) b.disabled = false; return }
  if (k === "resign") {
    const btn = e.target.closest("[data-s]"), id = +btn.dataset.id, prov = btn.dataset.provider, acct = (S.accounts || []).find(m => m.id === id), out = veil.querySelector(`[data-pwres="${id}"]`), my = ++resignFlow;
    if (!acct || !out) return;
    btn.disabled = true;
    try {
      await signInFlow(prov, acct.user, { id }, r => { if (resignFlow === my) out.innerHTML = sayHTML({ ...r, cancel: !r.final }) }, () => resignFlow === my && veil.dataset.kind === "settings");
      if (resignFlow === my) { out.innerHTML = '<span class="ok">Signed in. Your list is untouched.</span>'; lastSnap = ""; load() }
    } catch (err) { if (err !== "" && resignFlow === my) out.innerHTML = sayHTML(friendly(String(err), prov === "google" ? "gmail" : "outlook", "", "account")) }
    if (resignFlow === my) btn.disabled = false;
    return;
  }
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
      if ((S.accounts || []).find(m => m.id === id)?.auth) { // it signed in with an account until now: the row is a password row from here on
        lastSnap = ""; await load(); await drawSettings();
        const again = veil.querySelector(`[data-pwres="${id}"]`); if (again) again.innerHTML = '<span class="ok">Saved. It uses this code now. Your list is untouched.</span>';
      }
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
  if (k === "signout" && twice(e.target.closest("[data-s]"), "Click again: removes every saved password and key, and the local list")) { await rpc("sign_out"); clearReadCaches(); W = freshWizard(); closeVeil() }
}

