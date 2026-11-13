"use strict";
/* Reply drafts, sorting and filtering, snooze and sender preferences. Drafts live inside the to-do list:
   each task that needs a reply gets a draft editor in its expanded row. Nothing here ever sends mail. */

const INS = [["Thanks", "just acknowledge"], ["Confirm", "confirm"], ["Decline", "decline politely"], ["Need time", "ask for more time"]];
const REDO = [["Shorter", "make it shorter"], ["Friendlier", "make it friendlier"], ["More formal", "make it more formal"]];
const SORT_LABELS = { priority: "Priority", newest: "Newest", oldest: "Oldest", due: "Due date", sender: "Sender" };
const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
const SOURCE_LABEL = { ai: "Written by AI", template: "Quick reply", user: "Edited by you" };

const isoToday = () => { const d = new Date(); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`; };
const dueLabel = iso => { const [, m, d] = iso.split("-"); return `${+d} ${MONTHS[+m - 1]}`; };
const isLocal = t => t.uid >= 2000000000;
const editing = () => document.activeElement?.closest?.("#view textarea, #view input");

/* small tags in the row's meta line */
function tagsHTML(t) {
  const out = [];
  if (t._thread) out.push(`<span class="tg">${t._thread.length} messages</span>`);
  if (t._max > t.priority) out.push(`<span class="tg" title="An earlier message in this conversation ranks higher">Earlier P${t._max}</span>`);
  if (t.gone && t.status !== "done") out.push('<span class="tg">Left inbox</span>');
  if (t.closed_by === "replied") out.push(`<span class="tg">${t.kind === "followup" ? "They replied" : "Replied"}</span>`);
  if (t.kind === "followup") out.push('<span class="tg" title="You wrote and nobody has answered. It closes itself when a reply arrives.">Waiting</span>');
  if (t.learned) out.push(`<span class="tg" title="Adjusted from your corrections of this sender. Sender → Normal starts over.">Learned ${t.learned > 0 ? "+" : "−"}${Math.abs(t.learned)}</span>`);
  if (t.draft) out.push(`<span class="tg">Draft${t.draft.saved ? " · saved" : ""}</span>`);
  else if (t.needs_reply) out.push('<span class="tg">↩ Reply</span>');
  if (t.due) out.push(`<span class="tg ${t.due <= isoToday() ? "r" : ""}">Due ${dueLabel(t.due)}</span>`);
  if (t.attachment_count) out.push(`<span class="tg">Files ${t.attachment_count}</span>`);
  if (isLocal(t) && t.kind !== "followup") out.push('<span class="tg">Mine</span>');
  if (t.pref === "important") out.push('<span class="tg">★ Sender</span>');
  if (t.pref === "mute") out.push('<span class="tg">Muted</span>');
  return out.join("");
}

/* Where an unsubscribe link goes. Never throws: one odd header must not stop the whole list from drawing. */
function unsubWhere(u) {
  try { return /^mailto:/i.test(u) ? "by email" : new URL(u).hostname || "link" } catch { return "link" }
}

/* snooze presets, and a date picker for "wake me on this day" */
function snoozeHTML(t) {
  const tomorrow = new Date(Date.now() + 864e5), min = `${tomorrow.getFullYear()}-${String(tomorrow.getMonth() + 1).padStart(2, "0")}-${String(tomorrow.getDate()).padStart(2, "0")}`;
  return `<div class="drow"><span class="lbl">Snooze</span><button class="mini" data-act="snooze" data-preset="3h">3 hours</button><button class="mini" data-act="snooze" data-preset="tomorrow">Tomorrow</button><button class="mini" data-act="snooze" data-preset="nextweek">Next week</button><label class="lbl" for="sd${t.uid}">or until</label><input id="sd${t.uid}" class="mini dateinp" type="date" min="${min}" data-snooze-date="${t.uid}" aria-label="Snooze until this day">${isSnoozed(t) ? '<button class="mini" data-act="snooze" data-preset="clear">Bring back</button>' : ""}</div>`;
}

/* messages read on demand (kept in memory only, per task id) */
const bodies = {};

/* attachments, the unsubscribe link, due date, title, the message text and a free-text note */
function extraHTML(t) {
  const files = t.attachment_count ? `<div class="spec"><div><span>Files</span><b>${t.attachments.map(esc).join(" · ")}${t.attachment_count > t.attachments.length ? ` · and ${t.attachment_count - t.attachments.length} more` : ""}</b></div></div>` : "";
  const unsub = t.unsubscribe ? `<div class="drow"><span class="lbl">Newsletter</span><button class="mini" data-act="copy-unsub">Copy unsubscribe link</button><span class="lbl">${esc(unsubWhere(t.unsubscribe))}</span></div>` : "";
  const own = find(t.uid) || t; // a conversation row shows the earliest due date; the field edits this message's own
  const due = `<div class="drow"><label class="lbl" for="du${t.uid}">Due</label><input id="du${t.uid}" class="mini dateinp" type="date" data-due="${t.uid}" value="${esc(own.due || "")}" aria-label="Due date">${own.due ? '<button class="mini" data-act="due-clear">Clear</button>' : ""}</div>`;
  const title = isLocal(t) ? `<div class="drow"><label class="lbl" for="ti${t.uid}">Title</label><input id="ti${t.uid}" class="dtext titleinp" type="text" maxlength="200" data-title="${t.uid}" value="${esc(t.subject)}" aria-label="Task title"></div>` : "";
  const gmail = S.gmail && t.message_id ? '<button class="mini" data-act="copy-gmail">Copy Gmail link</button>' : "";
  const read = !isLocal(t) && !isHub
    ? `<div class="drow"><button class="mini" data-act="read-body" aria-expanded="${bodies[t.uid] !== undefined}">${bodies[t.uid] !== undefined ? "Hide message" : "Read message"}</button>${gmail}</div>${bodies[t.uid] !== undefined ? `<pre class="bodytext" tabindex="0" aria-label="Message text">${esc(bodies[t.uid] || "No text.")}</pre>` : ""}`
    : "";
  const note = `<label class="lbl nl" for="n${t.uid}">Note</label><textarea id="n${t.uid}" class="dtext note" data-note="${t.uid}" rows="2" maxlength="2000" placeholder="Anything to remember about this" aria-label="Note">${esc(t.note || "")}</textarea>`;
  return files + unsub + title + due + read + note;
}

/* the controls inside an expanded row */
function replyHTML(t) {
  if (isLocal(t)) return extraHTML(t) + snoozeHTML(t);
  return extraHTML(t) + replyBlock(t);
}
function replyBlock(t) {
  const d = t.draft, ctl = (act, label, extra = "") => `<button class="mini ${extra}" data-act="${act}">${label}</button>`;
  const chips = (list, act) => list.map(([l, ins]) => `<button class="mini" data-act="${act}" data-ins="${esc(ins)}">${l}</button>`).join("");
  const hasLink = d && /(https?:\/\/|www\.)/i.test(d.body);
  const head = `<div class="rhd"><span>Reply to ${esc(t.reply_to || t.addr)}</span><em>${d ? esc(SOURCE_LABEL[d.source] || "") + (d.saved ? " · in your Drafts folder" : "") + (hasLink ? " · contains a link" : "") : ""}</em></div>`;
  const warn = t.reply_warn ? `<div class="warn">This reply would go to <b>${esc(t.reply_to || t.addr)}</b>, which is not the sender’s own address, or the message failed its sender check. Make sure that is what you want.</div>` : "";
  const desktop = !isHub; // writing a draft spends AI quota and saving writes to your mailbox: desktop only
  const body = d
    ? `<textarea class="dtext" data-draft="${t.uid}" rows="7" spellcheck="true" aria-label="Reply draft">${esc(d.body)}</textarea>
       <div class="drow">${ctl("draft-copy", "Copy")}${desktop ? ctl("draft-save", d.saved ? "Save again" : "Save to Drafts") : ""}${ctl("draft-discard", "Discard")}</div>
       ${desktop ? `<div class="drow"><span class="lbl">Rewrite</span>${chips(REDO, "draft-new")}${chips(INS, "draft-new")}</div>` : ""}
       <div class="note">Nothing is ever sent. Review the text, then copy it${desktop ? " or save it to your mail app’s Drafts" : ""}.</div>`
    : desktop
      ? `<div class="drow"><button class="mini pri" data-act="draft-new" data-ins="" ${t.drafting ? "disabled" : ""}>${t.drafting ? "Writing…" : "Draft a reply"}</button>${chips(INS, "draft-new")}</div>`
      : `<div class="note">Open the desktop app to write a draft for this message.</div>`;
  const snooze = snoozeHTML(t);
  const pref = t.pref || "normal", pb = (lvl, l) => `<button class="mini ${pref === lvl ? "on" : ""}" data-act="pref" data-level="${lvl}">${l}</button>`;
  return `<div class="reply">${head}${warn}${body}</div>${snooze}<div class="drow"><span class="lbl">Sender</span>${pb("normal", "Normal")}${pb("important", "Always important")}${pb("mute", "Mute")}</div>`;
}

/* chips, search and sort above the list */
function filterHTML() {
  const base = baseList(), cats = {};
  base.forEach(t => cats[t.category] = (cats[t.category] || 0) + 1);
  const filtered = flt.cat !== "all" || flt.only || flt.q.trim();
  const nReply = base.filter(t => t.needs_reply || t.draft).length, nDraft = base.filter(t => t.draft).length;
  const chip = (key, label, n, on) => `<button class="chip ${on ? "on" : ""}" data-flt="${esc(key)}">${label}<i>${n}</i></button>`;
  return `<div class="fbar"><div class="chips">${chip("all", "All", base.length, flt.cat === "all" && !flt.only)}${nReply ? chip("only:reply", "Reply", nReply, flt.only === "reply") : ""}${nDraft ? chip("only:drafts", "Drafts", nDraft, flt.only === "drafts") : ""}${Object.keys(cats).sort().map(c => chip("cat:" + c, esc(c), cats[c], flt.cat === c)).join("")}</div>
    <div class="fctl">${filtered && listFor().length > 1 ? `<button class="mini" data-bulk="1">Done all ${listFor().length}</button>` : ""}<input id="q" type="search" placeholder="Search   /" value="${esc(flt.q)}" aria-label="Search tasks" autocomplete="off"><select id="sort" aria-label="Sort by">${Object.entries(SORT_LABELS).map(([k, l]) => `<option value="${k}" ${flt.sort === k ? "selected" : ""}>${l}</option>`).join("")}</select></div></div>`;
}

/* ───────── actions ───────── */
async function genDraft(t, ins, force) {
  t.drafting = true; render(); busy++;
  try {
    const r = await rpc("draft_generate", { uid: t.uid, instruction: ins, force: !!force });
    t.draft = { body: r.body, source: r.source, saved: false, updated: Date.now() / 1000 };
    if (r.source === "template" && S.settings.ai && S.settings.ai.mode !== "rules") toast("The AI didn’t answer, so a quick reply was used");
  } catch (e) { toast("Couldn’t write a draft: " + e) }
  finally { busy--; t.drafting = false; render() }
}

let draftTimer;
function saveDraftText(uid, body, now) {
  const t = find(uid); if (t?.draft) { t.draft.body = body; t.draft.source = "user" }
  clearTimeout(draftTimer);
  draftTimer = setTimeout(() => call("draft_update", { uid, body }), now ? 0 : 600);
}

function setDue(t, due) {
  const uid = t.uid, old = t.due || ""; if (old === due) return;
  t.due = due; undoStack.push(() => { const x = find(uid); if (x) x.due = old; call("set_due", { uid, due: old }) });
  call("set_due", { uid, due }); showUndo(due ? "Due date set" : "Due date cleared"); render();
}

let noteTimer;
function saveNote(uid, note, now) {
  const t = find(uid); if (t) t.note = note;
  clearTimeout(noteTimer);
  noteTimer = setTimeout(() => call("set_note", { uid, note }), now ? 0 : 600);
}

async function copyText(text) {
  try { await navigator.clipboard.writeText(text); toast("Copied") }
  catch { toast("Couldn’t copy. Select the text and copy it by hand") }
}

/* Returns true when the click was one of ours. */
function handleExtra(e, t, act) {
  const el = e.target.closest("[data-act]");
  switch (act) {
    case "draft-new": {
      const edited = t.draft?.source === "user";
      if (edited && !twice(el, "Replace your edits? Click again")) return true;
      genDraft(t, el.dataset.ins || "", edited); return true;
    }
    case "copy-unsub": copyText(t.unsubscribe || ""); return true;
    case "copy-gmail": copyText("https://mail.google.com/mail/u/0/#search/" + encodeURIComponent("rfc822msgid:" + String(t.message_id).replace(/^<|>$/g, ""))); return true;
    case "due-clear": setDue(t, ""); return true;
    case "read-body":
      if (bodies[t.uid] !== undefined) { delete bodies[t.uid]; render(); return true }
      (async () => { try { bodies[t.uid] = await rpc("read_body", { uid: t.uid }) } catch (err) { toast(String(err)) } render() })();
      return true;
    case "draft-copy": copyText(t.draft?.body || ""); return true;
    case "draft-discard":
      if (t.draft?.source === "user" && !twice(el, "Discard your edits? Click again")) return true;
      t.draft = null; call("draft_discard", { uid: t.uid }); render(); return true;
    case "draft-save":
      if (t.reply_warn && !twice(el, "Address differs. Click again to save")) return true;
      (async () => { busy++; try { await rpc("draft_to_mailbox", { uid: t.uid }); toast("Saving to your Drafts folder…"); if (t.draft) t.draft.saved = true } catch (err) { toast(String(err)) } finally { busy--; render() } })();
      return true;
    case "snooze": {
      const p = el.dataset.preset, ids = members(t).map(x => x.uid), until = p === "clear" ? 0 : (p === "3h" ? Date.now() / 1000 + 10800 : Date.now() / 1000 + 86400);
      ids.forEach(u => { const x = find(u); if (x) x.snooze_until = until; call("set_snooze", { uid: u, preset: p }) });
      undoStack.push(() => ids.forEach(u => { const x = find(u); if (x) x.snooze_until = 0; call("set_snooze", { uid: u, preset: "clear" }) }));
      if (p !== "clear") { showUndo("Snoozed"); openUid = null } render(); return true;
    }
    case "pref": {
      const lvl = el.dataset.level; t.pref = lvl === "normal" ? "" : lvl;
      S.tasks.filter(x => x.addr === t.addr && x.status !== "done").forEach(x => { x.pref = t.pref; if (lvl === "important") x.priority = Math.max(x.priority, 4); if (lvl === "mute") { x.priority = 1; x.urgent = false } });
      call("set_pref", { uid: t.uid, level: lvl }); toast(lvl === "important" ? "Mail from this sender will rank high" : lvl === "mute" ? "This sender is muted" : "Back to normal"); render(); return true;
    }
  }
  return false;
}

/* filters, search and sort */
document.addEventListener("click", e => {
  const c = e.target.closest("[data-flt]"); if (!c) return;
  const [k, v] = c.dataset.flt.split(":");
  if (k === "all") { flt.cat = "all"; flt.only = "" }
  else if (k === "cat") { flt.cat = flt.cat === v ? "all" : v }
  else if (k === "only") { flt.only = flt.only === v ? "" : v }
  render();
});
document.addEventListener("input", e => {
  if (e.target.id === "q") { flt.q = e.target.value; render() }
  else if (e.target.dataset?.draft) saveDraftText(+e.target.dataset.draft, e.target.value, false);
  else if (e.target.dataset?.note) saveNote(+e.target.dataset.note, e.target.value, false);
});
document.addEventListener("change", e => {
  const x = e.target;
  if (x.id === "sort") { flt.sort = x.value; render() }
  else if (x.dataset?.due) { const t = find(+x.dataset.due), id = x.id; if (t) { setDue(t, x.value); document.getElementById(id)?.focus() } }
  else if (x.dataset?.title) {
    const t = find(+x.dataset.title), title = x.value.trim(); if (!t || !title || title === t.subject) { if (t) x.value = t.subject; return }
    const old = t.subject; t.subject = t.action = title;
    undoStack.push(() => { const y = find(t.uid); if (y) y.subject = y.action = old; call("set_title", { uid: t.uid, title: old }) });
    call("set_title", { uid: t.uid, title }); showUndo("Title changed"); const id = x.id; render(); document.getElementById(id)?.focus();
  } else if (x.dataset?.snoozeDate) {
    const t = find(+x.dataset.snoozeDate); if (!t || !x.value) return;
    const ids = members(t).map(y => y.uid);
    (async () => {
      busy++; try { for (const u of ids) await rpc("set_snooze", { uid: u, date: x.value }); const until = new Date(x.value + "T09:00:00").getTime() / 1000; ids.forEach(u => { const y = find(u); if (y) y.snooze_until = until }); undoStack.push(() => ids.forEach(u => { const y = find(u); if (y) y.snooze_until = 0; call("set_snooze", { uid: u, preset: "clear" }) })); showUndo("Snoozed"); openUid = null }
      catch (err) { toast(String(err).replace(/^Error: /, "")) } finally { busy--; render() }
    })();
  }
});
document.addEventListener("focusout", e => {
  if (e.target.dataset?.draft) saveDraftText(+e.target.dataset.draft, e.target.value, true);
  if (e.target.dataset?.note) saveNote(+e.target.dataset.note, e.target.value, true);
  if (pendingRender && !editing()) { pendingRender = false; render() }
});
document.addEventListener("keydown", e => { if (e.target.id === "q" && e.key === "Escape") { flt.q = ""; e.target.blur(); render() } });

/* mark all folded automated mail as done (one undo restores all) */
document.addEventListener("click", e => {
  const b = e.target.closest("[data-autodone]"); if (!b) return;
  const l = parts().auto.flatMap(members); if (!l.length || !twice(b, `Click again: ${l.length} done`)) return;
  const ids = l.map(t => t.uid);
  l.forEach(t => { t.status = "done"; call("set_done", { uid: t.uid, done: true }) });
  undoStack.push(() => ids.forEach(u => { const t = find(u); if (t) { t.status = "open"; call("set_done", { uid: u, done: false }) } }));
  showUndo(`${ids.length} done`); render();
});

/* mark everything that left the inbox as done (one undo restores all) */
document.addEventListener("click", e => {
  const b = e.target.closest("[data-gonedone]"); if (!b) return;
  const l = parts().gone.flatMap(members); if (!l.length || !twice(b, `Click again: ${l.length} done`)) return;
  const ids = l.map(t => t.uid);
  l.forEach(t => { t.status = "done"; call("set_done", { uid: t.uid, done: true }) });
  undoStack.push(() => ids.forEach(u => { const t = find(u); if (t) { t.status = "open"; call("set_done", { uid: u, done: false }) } }));
  showUndo(`${ids.length} done`); render();
});

/* mark everything currently shown as done (one undo restores all) */
document.addEventListener("click", e => {
  const b = e.target.closest("[data-bulk]"); if (!b) return;
  const rows = listFor(); if (rows.length < 2 || !twice(b, `Click again: ${rows.length} done`)) return;
  const l = rows.flatMap(members), ids = l.map(t => t.uid);
  l.forEach(t => { t.status = "done"; call("set_done", { uid: t.uid, done: true }) });
  undoStack.push(() => ids.forEach(u => { const t = find(u); if (t) { t.status = "open"; call("set_done", { uid: u, done: false }) } }));
  showUndo(`${ids.length} done`); render();
});

/* ───────── add a task of your own, and the shortcuts card ───────── */
const vl = () => $("#veil");
let veilFrom = null;
const FOCUSABLE = 'a[href],button:not([disabled]),input:not([disabled]),select,textarea,[tabindex]:not([tabindex="-1"])';
function dialogOpened(label) {
  veilFrom = document.activeElement;
  const p = vl().querySelector(".panel"); if (p) { p.setAttribute("role", "dialog"); p.setAttribute("aria-modal", "true"); p.setAttribute("aria-label", label) }
  setTimeout(() => (vl().querySelector("[autofocus]") || vl().querySelector(FOCUSABLE))?.focus(), 30);
}
document.addEventListener("keydown", e => {
  if (e.key !== "Tab" || !vl().classList.contains("show")) return;
  const f = [...vl().querySelectorAll(FOCUSABLE)].filter(x => x.offsetParent !== null); if (!f.length) return;
  const first = f[0], last = f[f.length - 1];
  if (!vl().contains(document.activeElement)) { e.preventDefault(); first.focus() }
  else if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus() }
  else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus() }
});
function openAdd() {
  vl().dataset.kind = "add"; vl().classList.add("show");
  vl().innerHTML = `<div class="panel"><h2>Add a task</h2><p>For things that did not come by email. It lives in your list only.</p>
    <label class="f" for="at-title">What needs doing</label><input id="at-title" type="text" maxlength="200" autocomplete="off" placeholder="Renew passport">
    <div class="row2" style="grid-template-columns:1fr 1fr"><div><label class="f" for="at-pri">Priority</label><select id="at-pri">${[1, 2, 3, 4, 5].map(n => `<option value="${n}" ${n === 3 ? "selected" : ""}>${n}${n === 5 ? " (highest)" : n === 1 ? " (lowest)" : ""}</option>`).join("")}</select></div>
    <div><label class="f" for="at-due">Due (optional)</label><input id="at-due" type="date"></div></div>
    <div class="foot"><span class="sp"></span><button class="btn" data-vc="1">Cancel</button><button class="btn pri" data-add="1">Add</button></div></div>`;
  dialogOpened("Add a task");
  setTimeout(() => $("#at-title")?.focus(), 40);
}
async function submitAdd() {
  const title = $("#at-title").value.trim(); if (!title) { toast("Give the task a title"); return }
  try {
    const uid = await rpc("add_task", { title, priority: +$("#at-pri").value, due: $("#at-due").value || "" });
    view = "todo"; flt.cat = "all"; flt.only = ""; flt.q = ""; cur = uid; closeVeil(); toast("Task added");
  } catch (err) { toast(String(err).replace(/^Error: /, "")) }
}
const HELP = [["J K", "Move down / up"], ["Enter", "Open or close a task (while selecting, picks the row)"], ["X", "Mark done"], ["1–5", "Set priority"], ["S", "Star"], ["R", "Reply draft"], ["H", "Snooze until tomorrow"], ["V", "Select several, then X, 1–5, S or H"], ["A", "Add a task"], ["/", "Search"], ["Z", "Undo"], [",", "Settings"], ["?", "This card"]];
function openHelp() {
  vl().dataset.kind = "help"; vl().classList.add("show");
  vl().innerHTML = `<div class="panel"><h2>Keyboard</h2><div class="keylist">${HELP.map(([k, l]) => `<div><span class="cap">${k}</span><b>${l}</b></div>`).join("")}</div>
    <div class="foot"><span class="sp"></span><button class="btn pri" data-vc="1">Close</button></div></div>`;
  dialogOpened("Keyboard shortcuts");
}
document.addEventListener("click", e => {
  if (vl().dataset.kind === "add" && e.target.closest("[data-add]")) submitAdd();
  else if ((vl().dataset.kind === "add" || vl().dataset.kind === "help") && e.target === vl()) closeVeil();
  else if (e.target.closest("[data-act=addtask]")) openAdd();
});
document.addEventListener("keydown", e => {
  const k = vl().dataset.kind;
  if (vl().classList.contains("show") && e.key === "Escape" && k === "settings") { closeVeil(); return }
  if (vl().classList.contains("show") && (k === "add" || k === "help")) {
    if (e.key === "Escape") closeVeil();
    else if (k === "add" && e.key === "Enter" && e.target.id === "at-title") { e.preventDefault(); submitAdd() }
    return;
  }
  if (e.metaKey || e.ctrlKey || e.altKey || vl().classList.contains("show") || /INPUT|SELECT|TEXTAREA/.test(e.target.tagName)) return;
  // Caps Lock or Shift must not silently turn these off
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (key === "a") { e.preventDefault(); openAdd() } else if (key === "?") { e.preventDefault(); openHelp() }
});

/* ───────── select many, act once ─────────
   Acts on the same messages a single row would (members()), so mail that arrived after the screen was drawn is
   never closed unseen. The selection only ever holds rows that are visible now (pruneSel). One undo step per batch. */
const selRows = () => listFor().filter(t => sel.has(t.uid));
function pruneSel() {
  const ids = new Set(listFor().map(t => t.uid));
  [...sel].forEach(u => { if (!ids.has(u)) sel.delete(u) });
  if (selAnchor !== null && !ids.has(selAnchor)) selAnchor = null;
}
function enterSelect(withCurrent) {
  selMode = true; openUid = null; sel.clear(); selAnchor = null;
  if (withCurrent && cur != null) { sel.add(cur); selAnchor = cur }
  render();
}
function exitSelect() { selMode = false; sel.clear(); selAnchor = null; if (view !== "log") render() }
function selectClick(e, t) {
  const l = listFor().map(x => x.uid);
  if (e.shiftKey && selAnchor !== null && l.includes(selAnchor)) {
    const a = l.indexOf(selAnchor), b = l.indexOf(t.uid);
    for (let i = Math.min(a, b); i <= Math.max(a, b); i++) sel.add(l[i]);
  } else { sel.has(t.uid) ? sel.delete(t.uid) : sel.add(t.uid); selAnchor = t.uid }
  render();
}
function selbarHTML() {
  const n = selRows().length, b = (k, l, x = "") => `<button class="mini ${x}" data-sel="${k}">${l}</button>`;
  const acts = view === "done" ? b("done", "Reopen")
    : b("done", "Done") + b("star", "Star") + '<span class="lbl">Priority</span>' + [1, 2, 3, 4, 5].map(p => b("p" + p, p)).join("") + (view === "snoozed" ? b("unsnooze", "Bring back") : b("snooze:tomorrow", "Tomorrow") + b("snooze:nextweek", "Next week")) + b("mute", "Mute senders");
  return `<div class="selbar" role="toolbar" aria-label="Actions for the selected tasks"><b aria-live="polite">${n ? n + " selected" : "Pick tasks"}</b>${n ? acts : ""}<span class="sp"></span>${b("all", "All " + listFor().length)}${n ? b("none", "None") : ""}${b("exit", "Exit")}</div>`;
}
function bulk(k, el) {
  // the bar offers Reopen only in Done and no snoozing in Snoozed; the keys must obey the same limits (a snooze cannot be
  // put back as it was, and a done task is not re-ranked)
  if ((view === "done" && k !== "done") || (view === "snoozed" && k.startsWith("snooze:"))) return;
  const rows = selRows(); if (!rows.length) { toast("Select something first"); return }
  const ms = [...new Map(rows.flatMap(members).map(x => [x.uid, x])).values()], ids = ms.map(x => x.uid);
  if (!ms.length) return;
  if (k === "done") {
    const v = view !== "done", was = ms.map(x => [x.uid, x.status]);
    ms.forEach(x => { x.status = v ? "done" : "open"; call("set_done", { uid: x.uid, done: v }) });
    undoStack.push(() => was.forEach(([u, s]) => { const x = find(u); if (x) { x.status = s; call("set_done", { uid: u, done: s === "done" }) } }));
    showUndo(`${ids.length} ${v ? "done" : "reopened"}`);
  } else if (k === "star") {
    const v = !ms.every(x => x.star), was = ms.map(x => [x.uid, !!x.star]);
    ms.forEach(x => { x.star = v; call("set_star", { uid: x.uid, star: v }) });
    undoStack.push(() => was.forEach(([u, s]) => { const x = find(u); if (x) { x.star = s; call("set_star", { uid: u, star: s }) } }));
    showUndo(`${ids.length} ${v ? "starred" : "unstarred"}`);
  } else if (/^p[1-5]$/.test(k)) {
    // like pressing 1-5 on a row: the newest message of each conversation only. Every change is a correction the ranker
    // learns from, and one click on a long thread must not count as one correction per message
    const own = rows.map(r => find(r.uid)).filter(Boolean), n = +k[1], was = own.map(x => [x.uid, x.priority, !!x.star]);
    own.forEach(x => { if (x.priority !== n) { x.priority = n; if (n >= 4) x.star = true; call("set_priority", { uid: x.uid, priority: n }) } });
    undoStack.push(() => was.forEach(([u, p, s]) => { const x = find(u); if (!x) return; x.priority = p; call("set_priority", { uid: u, priority: p }); if (!s && n >= 4) { x.star = false; call("set_star", { uid: u, star: false }) } }));
    showUndo(`${own.length} set to priority ${n}`);
  } else if (k.startsWith("snooze:")) {
    const p = k.slice(7), until = Date.now() / 1000 + (p === "tomorrow" ? 86400 : 604800);
    ms.forEach(x => { x.snooze_until = until; call("set_snooze", { uid: x.uid, preset: p }) });
    undoStack.push(() => ids.forEach(u => { const x = find(u); if (x) { x.snooze_until = 0; call("set_snooze", { uid: u, preset: "clear" }) } }));
    showUndo(`${ids.length} snoozed`);
  } else if (k === "unsnooze") {
    ms.forEach(x => { x.snooze_until = 0; call("set_snooze", { uid: x.uid, preset: "clear" }) });
    toast(`${ids.length} brought back. Snooze them again from each task if you change your mind`);
  } else if (k === "mute") {
    const addrs = [...new Set(ms.map(x => x.addr).filter(Boolean))];
    if (!addrs.length) { toast("These are your own tasks. There is no sender to mute"); return }
    if (el && !twice(el, `Click again: mute ${addrs.length} sender${addrs.length > 1 ? "s" : ""}`)) return;
    addrs.forEach(a => {
      const rep = ms.find(x => x.addr === a);
      S.tasks.filter(x => x.addr === a && x.status !== "done").forEach(x => { x.pref = "mute"; x.priority = 1; x.urgent = false });
      call("set_pref", { uid: rep.uid, level: "mute" });
    });
    toast(`${addrs.length} sender${addrs.length > 1 ? "s" : ""} muted. Open a task and choose Normal to undo`);
  } else return;
  sel.clear(); selAnchor = null; render();
}
/* keys while selecting; returns true when the key was ours */
function selKey(e, k, t) {
  if (k === "Escape") { exitSelect(); return true }
  if (k === " " || k === "Enter") { if (t) { e.preventDefault(); sel.has(t.uid) ? sel.delete(t.uid) : sel.add(t.uid); selAnchor = t.uid; render() } return true }
  if (e.shiftKey && (k === "j" || k === "k" || k === "ArrowDown" || k === "ArrowUp")) {
    e.preventDefault(); if (t) sel.add(t.uid);
    move(k === "j" || k === "ArrowDown" ? 1 : -1); if (cur != null) { sel.add(cur); render() }
    return true;
  }
  if (k === "x" || k === "e") { bulk("done"); return true }
  if (/^[1-5]$/.test(k)) { bulk("p" + k); return true }
  if (k === "s") { bulk("star"); return true }
  if (k === "h") { bulk("snooze:tomorrow"); return true }
  return k === "r"; // a reply draft makes no sense for many rows; every other key behaves as usual
}
document.addEventListener("click", e => {
  const b = e.target.closest("[data-sel]"); if (!b) return;
  const k = b.dataset.sel;
  if (k === "exit") return exitSelect();
  if (k === "all") { listFor().forEach(t => sel.add(t.uid)); return render() }
  if (k === "none") { sel.clear(); selAnchor = null; return render() }
  bulk(k, b);
});
