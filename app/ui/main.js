"use strict";
/* ───────── backend adapter: Tauri window, phone hub, or demo mock ───────── */
const TA = window.__TAURI__;
const qs = new URLSearchParams(location.search);
if (qs.get("token")) {
  localStorage.setItem("tok", qs.get("token"));
  qs.delete("token"); // keep the access code out of the address bar, history and Referer
  history.replaceState(null, "", location.pathname + (qs.toString() ? "?" + qs : ""));
}
const TOKEN = localStorage.getItem("tok") || "";
const MODE = TA ? "tauri" : (TOKEN && qs.get("mock") == null ? "hub" : "mock");
const isHub = MODE === "hub";
async function rpc(cmd, args = {}) {
  if (MODE === "tauri") return TA.core.invoke("rpc", { cmd, args });
  if (MODE === "hub") {
    const r = await fetch("/api/rpc", { method: "POST", headers: { Authorization: "Bearer " + TOKEN, "Content-Type": "application/json" }, body: JSON.stringify({ cmd, args }) });
    const j = await r.json();
    if (!j.ok) throw j.error || "request failed";
    return j.data;
  }
  return mock(cmd, args);
}

/* ───────── state ───────── */
let S = { settings: { onboarded: true, ai: {}, alerts: {}, hub: {}, account: {} }, tasks: [], log: [], status: "starting", pending: 0 };
let view = "todo", cur = null, openUid = null, undoStack = [], lastSnap = "", busy = 0, pendingRender = false;
let flt = { cat: "all", only: "", q: "", sort: "priority" }; // category / reply / draft filters, search text, sort order
const $ = s => document.querySelector(s);
const esc = s => String(s ?? "").replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
/* Nothing-style dot-matrix numerals: 5x7 glyphs drawn as dots (lit = on, unlit = faint) */
const DM = { 0: "01110 10001 10011 10101 11001 10001 01110", 1: "00100 01100 00100 00100 00100 00100 01110", 2: "01110 10001 00001 00010 00100 01000 11111", 3: "11110 00001 00001 01110 00001 00001 11110", 4: "00010 00110 01010 10010 11111 00010 00010", 5: "11111 10000 11110 00001 00001 10001 01110", 6: "00110 01000 10000 11110 10001 10001 01110", 7: "11111 00001 00010 00100 01000 01000 01000", 8: "01110 10001 10001 01110 10001 10001 01110", 9: "01110 10001 10001 01111 00001 00010 01100" };
const dm = (n, cls = "") => {
  const s = String(Math.min(Math.max(n, 0), 99)).padStart(2, "0");
  const dots = [...s].map((d, i) => DM[d].split(" ").map((row, y) => [...row].map((b, x) => `<circle cx="${i * 6 + x + .5}" cy="${y + .5}" r=".36" class="${b === "1" ? "on" : "off"}"/>`).join("")).join("")).join("");
  return `<svg class="dm ${cls}" viewBox="0 0 ${s.length * 6 - 1} 7" role="img" aria-label="${s}">${dots}</svg>`;
};
const ago = t => { const h = (Date.now() / 1000 - t) / 3600; return h < 1 ? Math.max(1, Math.round(h * 60)) + "m" : h < 24 ? Math.round(h) + "h" : Math.round(h / 24) + "d" };
const label = t => t.action || t.subject;
const CHECK = '<svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2.5 6.5l2.5 2.5 4.5-5.5"/></svg>';
function toast(msg) { const e = document.createElement("div"); e.className = "toast"; e.textContent = msg; $("#toasts").prepend(e); setTimeout(() => e.remove(), 4000) }

/* ───────── list model ───────── */
const SECT = [["Now", "now"], ["Today", ""], ["This week", ""], ["Later", ""]];
const sect = p => p >= 5 ? 0 : p === 4 ? 1 : p === 3 ? 2 : 3;
const isSnoozed = t => t.snooze_until > Date.now() / 1000;
const SORTS = {
  priority: (a, b) => b.priority - a.priority || (b.star - a.star) || a.received - b.received,
  newest: (a, b) => b.received - a.received,
  oldest: (a, b) => a.received - b.received,
  due: (a, b) => (a.due || "9999").localeCompare(b.due || "9999") || b.priority - a.priority,
  sender: (a, b) => a.sender.localeCompare(b.sender),
};
/* the tasks that belong to the current tab, before filters */
function baseList() {
  if (view === "done") return S.tasks.filter(t => t.status === "done");
  const open = S.tasks.filter(t => t.status !== "done");
  if (view === "snoozed") return open.filter(isSnoozed);
  const active = open.filter(t => !isSnoozed(t));
  return view === "star" ? active.filter(t => t.star) : active;
}
function listFor() {
  let l = baseList();
  if (view === "todo" || view === "star") {
    if (flt.cat !== "all") l = l.filter(t => t.category === flt.cat);
    if (flt.only === "reply") l = l.filter(t => t.needs_reply || t.draft);
    if (flt.only === "drafts") l = l.filter(t => t.draft);
    const q = flt.q.trim().toLowerCase();
    if (q) l = l.filter(t => [t.sender, t.addr, t.subject, t.action, t.excerpt].join(" ").toLowerCase().includes(q));
  }
  return [...l].sort(view === "done" ? SORTS.newest : SORTS[flt.sort] || SORTS.priority);
}

/* ───────── render ───────── */
function rowHTML(t) {
  const sg = t.signals || {}, why = [sg.known && "Someone you write to", sg.reply && "Replying to you", sg.bulk && "Automated or bulk"].filter(Boolean).join(" · ") || "Unknown sender";
  return `<div class="row ${t.uid === cur ? "cur" : ""} ${t.uid === openUid ? "open" : ""} ${t.status === "done" ? "isdone" : ""}" data-uid="${t.uid}">
    <button class="chk" data-act="done" aria-label="${t.status === "done" ? "Reopen" : "Mark done"}"><b>X</b>${CHECK}</button>
    <div class="mid" data-act="open"><div class="t">${esc(label(t))}</div><div class="s">${t.urgent ? "<b>Urgent</b> · " : ""}${tagsHTML(t)}${esc(t.sender)} · ${esc(t.subject)} · ${ago(t.received)}</div></div>
    <button class="star ${t.star ? "on" : ""}" data-act="star" aria-label="Star">${t.star ? "★" : "☆"}</button>
    <div class="pri ${t.priority >= 4 ? "hi" : ""}" role="group" aria-label="Priority">${[1, 2, 3, 4, 5].map(n => `<button data-p="${n}" class="${t.priority >= n ? "lit" : ""}" aria-label="Priority ${n}"></button>`).join("")}</div>
    <div class="more"><p>${esc(t.excerpt) || "No text."}</p><div class="spec">${isLocal(t) ? "" : `<div><span>From</span><b>${esc(t.sender)} · ${esc(t.addr)}</b></div><div><span>Signals</span><b>${esc(why)}</b></div><div><span>Why</span><b>${esc(t.why)}</b></div><div><span>Mailbox</span><b>${S.settings.writeback ? "Done files it away" : "Write-back off · changes stay here"}</b></div>`}</div>${replyHTML(t)}</div>
  </div>`;
}
function render() {
  const open = S.tasks.filter(t => t.status !== "done" && !isSnoozed(t)), snoozed = S.tasks.filter(t => t.status !== "done" && isSnoozed(t)), st = S.status || "";
  const ok = st === "idle" || st === "syncing", err = st.startsWith("error") || st === "no password";
  const tabs = [["todo", "To-do", open.length], ["star", "Starred", open.filter(t => t.star).length], ["snoozed", "Snoozed", snoozed.length], ["done", "Done", S.tasks.filter(t => t.status === "done").length], ["log", "Log", ""]];
  $("#bar").innerHTML = `<div class="brand"><span class="mark">要</span><span class="dot ${ok ? "" : err ? "err" : "off"}" title="${esc(st)}"></span></div>
    <nav>${tabs.map(([k, n, c]) => `<button class="${view === k ? "on" : ""}" data-view="${k}">${n}${c !== "" && c !== 0 ? `<i>${c}</i>` : ""}</button>`).join("")}</nav><span class="sp"></span><button class="gear" data-act="addtask">Add task</button>${isHub ? "" : '<button class="gear" data-act="settings">Settings</button>'}`;
  const names = { todo: "To<em>-do</em>", star: "Starred", snoozed: "Snoozed", done: "Done", log: "Log" };
  const d = new Date(), kick = `${{ todo: "Inbox", star: "Marked", snoozed: "Resting", done: "Filed", log: "Record" }[view]} · ${d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })}`;
  let h = `<div class="kick">${kick}</div><h1>${names[view]}</h1>`;
  if (view === "log") {
    h += `<p class="sub2">Everything the agent does, in order.</p><div class="log">${S.log.map(l => `<div><b>${new Date(l.at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</b>${esc(l.msg)}</div>`).join("") || "<div>Nothing yet.</div>"}</div>`;
  } else {
    const l = listFor();
    if (!l.some(t => t.uid === cur)) cur = l[0]?.uid ?? null;
    const hi = l.filter(t => t.priority >= 4).length, ur = l.filter(t => t.urgent).length, dr = l.filter(t => t.draft).length;
    h += l.length ? `<div class="stats"><div class="stat">${dm(l.length)}<em>${view === "done" ? "done" : view === "snoozed" ? "resting" : "open"}</em></div>${view === "done" || view === "snoozed" ? "" : `<div class="stat">${dm(hi, hi ? "r" : "")}<em>high</em></div><div class="stat">${dm(ur, ur ? "r" : "")}<em>urgent</em></div><div class="stat">${dm(dr)}<em>drafts</em></div>`}${S.pending ? `<div class="stat">${dm(S.pending)}<em>syncing</em></div>` : ""}</div>` : "";
    if ((view === "todo" || view === "star") && (baseList().length || flt.q)) h += filterHTML();
    let last = -1;
    for (const t of l) {
      const s = view === "todo" && flt.sort === "priority" ? sect(t.priority) : -2;
      if (s >= 0 && s !== last) h += `<div class="sec ${SECT[s][1]}"><span>0${s + 1} // ${SECT[s][0]}</span><i>${l.filter(x => sect(x.priority) === s).length}</i></div>`;
      last = s; h += rowHTML(t);
    }
    if (!l.length) h += `<div class="empty">${view === "done" ? "Nothing done yet" : view === "snoozed" ? "Nothing resting" : flt.q || flt.cat !== "all" || flt.only ? "No matches" : ok ? "All clear" : "Waiting for mail"}<small>${view === "todo" && ok ? "The agent is still watching your inbox." : ""}</small></div>`;
  }
  const wasSearching = document.activeElement?.id === "q", caret = wasSearching ? document.activeElement.selectionStart : 0;
  $("#view").innerHTML = h;
  if (wasSearching) { const q = $("#q"); q.focus(); q.setSelectionRange(caret, caret) }
  const msg = err ? `<span class="e">${esc(st === "no password" ? (isHub ? "Open the desktop app to sign in again" : "Sign in again in Settings") : st)}</span>` : ok ? "Watching your inbox" : st === "connecting" ? "Connecting…" : st === "setup" ? "Setup needed" : esc(st);
  const rl = view === "log" ? [] : listFor().slice(0, 80), ch = Math.max(5, Math.min(18, Math.floor((innerHeight - 40) / Math.max(rl.length, 1))));
  $("#rail").style.setProperty("--ch", ch + "px");
  $("#rail").innerHTML = rl.map(t => `<div class="rc ${t.uid === cur ? "cur" : ""}" data-rail="${t.uid}" title="${esc(label(t))}">${ch >= 10 ? [5, 4, 3, 2, 1].map(n => `<i class="${t.priority >= n ? "h" : ""}"></i>`).join("") : `<i class="${t.priority >= 4 ? "h" : ""}"></i>`}</div>`).join("");
  $("#foot").innerHTML = `<span>${msg}</span><span class="sp"></span><span class="keys"><span class="cap">J</span><span class="cap">K</span>Move<span class="cap">X</span>Done<span class="cap">1–5</span>Priority<span class="cap">S</span>Star<span class="cap">R</span>Reply<span class="cap">H</span>Snooze<span class="cap">/</span>Search<span class="cap">A</span>Add<span class="cap">Z</span>Undo<span class="cap">?</span>Keys</span>`;
}

/* ───────── actions (optimistic; the engine confirms on the next poll) ───────── */
const find = uid => S.tasks.find(t => t.uid === uid);
/* two-step confirmation without native dialogs (they are unreliable in the desktop webview) */
const armed = new WeakMap();
function twice(btn, label) {
  if (armed.has(btn)) { clearTimeout(armed.get(btn)); armed.delete(btn); return true }
  const old = btn.textContent; btn.textContent = label;
  armed.set(btn, setTimeout(() => { armed.delete(btn); btn.textContent = old }, 4000));
  return false;
}
async function call(cmd, args) { busy++; try { await rpc(cmd, args) } catch (e) { toast("Couldn't save: " + e) } finally { busy-- } }
function showUndo(msg) { $("#undoMsg").textContent = msg; $("#undo").style.display = "flex"; clearTimeout(window.ut); window.ut = setTimeout(() => $("#undo").style.display = "none", 5000) }
function setDone(t) {
  const el = document.querySelector(`.row[data-uid="${t.uid}"]`), v = t.status !== "done";
  const go = () => { t.status = v ? "done" : "open"; undoStack.push(() => { t.status = v ? "open" : "done"; call("set_done", { uid: t.uid, done: !v }) }); call("set_done", { uid: t.uid, done: v }); showUndo(v ? "Done" : "Reopened"); render() };
  if (el && v && view !== "done") { el.classList.add("gone"); setTimeout(go, 280) } else go();
}
function setPri(t, n) { if (t.priority === n) return; const o = t.priority, s = t.star; undoStack.push(() => { t.priority = o; t.star = s; call("set_priority", { uid: t.uid, priority: o }); if (!s && n >= 4) call("set_star", { uid: t.uid, star: false }) }); t.priority = n; if (n >= 4) t.star = true; call("set_priority", { uid: t.uid, priority: n }); render() }
function toggleStar(t) { const v = !t.star; t.star = v; undoStack.push(() => { t.star = !v; call("set_star", { uid: t.uid, star: !v }) }); call("set_star", { uid: t.uid, star: v }); render() }
function move(d) { const l = listFor(); let i = l.findIndex(t => t.uid === cur); i = Math.max(0, Math.min(l.length - 1, i + d)); cur = l[i]?.uid ?? null; render(); document.querySelector(".row.cur")?.scrollIntoView({ block: "nearest" }) }

document.addEventListener("click", e => {
  if ($("#veil").classList.contains("show")) return;
  const rc = e.target.closest("[data-rail]"); if (rc) { cur = +rc.dataset.rail; render(); document.querySelector(".row.cur")?.scrollIntoView({ block: "center", behavior: "smooth" }); return }
  const v = e.target.closest("[data-view]"); if (v) { view = v.dataset.view; render(); return }
  const a = e.target.closest("[data-act]")?.dataset.act;
  if (a === "settings") return openSettings();
  if (a === "addtask") return openAdd();
  const row = e.target.closest(".row"); if (!row) return;
  const t = find(+row.dataset.uid); if (!t) return; cur = t.uid;
  const pb = e.target.closest("[data-p]");
  if (e.target.closest(".more") && !pb && a && handleExtra(e, t, a)) return;
  if (e.target.closest("textarea")) return;
  if (pb) setPri(t, +pb.dataset.p);
  else if (a === "done") setDone(t);
  else if (a === "star") toggleStar(t);
  else { openUid = openUid === t.uid ? null : t.uid; render() }
});
document.addEventListener("keydown", e => {
  if (e.metaKey || e.ctrlKey || e.altKey || $("#veil").classList.contains("show") || /INPUT|SELECT|TEXTAREA/.test(e.target.tagName)) return;
  const t = find(cur), k = e.key;
  if (k === "j" || k === "ArrowDown") { e.preventDefault(); move(1) } else if (k === "k" || k === "ArrowUp") { e.preventDefault(); move(-1) }
  else if (t && (k === "x" || k === "e")) setDone(t);
  else if (t && /^[1-5]$/.test(k)) setPri(t, +k);
  else if (t && k === "s") toggleStar(t);
  else if (t && (k === "Enter" || k === " ")) { e.preventDefault(); openUid = openUid === t.uid ? null : t.uid; render() }
  else if (t && k === "r" && !isLocal(t)) { e.preventDefault(); openUid = t.uid; render(); if (!t.draft && !isHub) genDraft(t, ""); else document.querySelector(".row.open textarea")?.focus() }
  else if (t && k === "h") { handleExtra({ target: { closest: () => ({ dataset: { preset: "tomorrow" } }) } }, t, "snooze") }
  else if (k === "/") { e.preventDefault(); const q = $("#q"); if (q) q.focus() }
  else if (k === "z") { undoStack.pop()?.(); render() }
  else if (k === "," && !isHub) openSettings();
});
$("#undoBtn").onclick = () => { undoStack.pop()?.(); $("#undo").style.display = "none"; render() };

/* ───────── polling ───────── */
async function load() {
  if (busy) return;
  try {
    const d = await rpc("state"), snap = JSON.stringify([d.tasks, d.log, d.status, d.pending, d.settings]);
    if (snap === lastSnap) return; lastSnap = snap; S = d;
    const v = $("#veil");
    if (!d.settings.onboarded && !isHub) { if (!v.classList.contains("show")) openWizard() }
    if (!v.classList.contains("show")) { if (editing()) pendingRender = true; else render() }
  } catch (e) { S.status = "error: " + e; if (!$("#veil").classList.contains("show") && !editing()) render() }
}

