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
  if (t.draft) out.push(`<span class="tg">Draft${t.draft.saved ? " · saved" : ""}</span>`);
  else if (t.needs_reply) out.push('<span class="tg">↩ Reply</span>');
  if (t.due) out.push(`<span class="tg ${t.due <= isoToday() ? "r" : ""}">Due ${dueLabel(t.due)}</span>`);
  if (t.attachment_count) out.push(`<span class="tg">Files ${t.attachment_count}</span>`);
  if (isLocal(t)) out.push('<span class="tg">Mine</span>');
  if (t.pref === "important") out.push('<span class="tg">★ Sender</span>');
  if (t.pref === "mute") out.push('<span class="tg">Muted</span>');
  return out.join("");
}

/* Where an unsubscribe link goes. Never throws: one odd header must not stop the whole list from drawing. */
function unsubWhere(u) {
  try { return /^mailto:/i.test(u) ? "by email" : new URL(u).hostname || "link" } catch { return "link" }
}

/* attachments, the unsubscribe link and a free-text note */
function extraHTML(t) {
  const files = t.attachment_count ? `<div class="spec"><div><span>Files</span><b>${t.attachments.map(esc).join(" · ")}${t.attachment_count > t.attachments.length ? ` · and ${t.attachment_count - t.attachments.length} more` : ""}</b></div></div>` : "";
  const unsub = t.unsubscribe ? `<div class="drow"><span class="lbl">Newsletter</span><button class="mini" data-act="copy-unsub">Copy unsubscribe link</button><span class="lbl">${esc(unsubWhere(t.unsubscribe))}</span></div>` : "";
  const note = `<label class="lbl nl" for="n${t.uid}">Note</label><textarea id="n${t.uid}" class="dtext note" data-note="${t.uid}" rows="2" maxlength="2000" placeholder="Anything to remember about this" aria-label="Note">${esc(t.note || "")}</textarea>`;
  return files + unsub + note;
}

/* the controls inside an expanded row */
function replyHTML(t) {
  if (isLocal(t)) return extraHTML(t) + `<div class="drow"><span class="lbl">Snooze</span><button class="mini" data-act="snooze" data-preset="3h">3 hours</button><button class="mini" data-act="snooze" data-preset="tomorrow">Tomorrow</button><button class="mini" data-act="snooze" data-preset="nextweek">Next week</button>${isSnoozed(t) ? '<button class="mini" data-act="snooze" data-preset="clear">Bring back</button>' : ""}</div>`;
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
  const snooze = `<div class="drow"><span class="lbl">Snooze</span><button class="mini" data-act="snooze" data-preset="3h">3 hours</button><button class="mini" data-act="snooze" data-preset="tomorrow">Tomorrow</button><button class="mini" data-act="snooze" data-preset="nextweek">Next week</button>${isSnoozed(t) ? '<button class="mini" data-act="snooze" data-preset="clear">Bring back</button>' : ""}</div>`;
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
    case "draft-copy": copyText(t.draft?.body || ""); return true;
    case "draft-discard": t.draft = null; call("draft_discard", { uid: t.uid }); render(); return true;
    case "draft-save":
      if (t.reply_warn && !twice(el, "Address differs. Click again to save")) return true;
      (async () => { busy++; try { await rpc("draft_to_mailbox", { uid: t.uid }); toast("Saving to your Drafts folder…"); if (t.draft) t.draft.saved = true } catch (err) { toast(String(err)) } finally { busy--; render() } })();
      return true;
    case "snooze": {
      const p = el.dataset.preset;
      call("set_snooze", { uid: t.uid, preset: p }); t.snooze_until = p === "clear" ? 0 : (p === "3h" ? Date.now() / 1000 + 10800 : Date.now() / 1000 + 86400);
      undoStack.push(() => { t.snooze_until = 0; call("set_snooze", { uid: t.uid, preset: "clear" }) });
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
document.addEventListener("change", e => { if (e.target.id === "sort") { flt.sort = e.target.value; render() } });
document.addEventListener("focusout", e => {
  if (e.target.dataset?.draft) saveDraftText(+e.target.dataset.draft, e.target.value, true);
  if (e.target.dataset?.note) saveNote(+e.target.dataset.note, e.target.value, true);
  if (pendingRender && !editing()) { pendingRender = false; render() }
});
document.addEventListener("keydown", e => { if (e.target.id === "q" && e.key === "Escape") { flt.q = ""; e.target.blur(); render() } });

/* mark everything currently shown as done (one undo restores all) */
document.addEventListener("click", e => {
  const b = e.target.closest("[data-bulk]"); if (!b) return;
  const l = listFor(); if (l.length < 2 || !twice(b, `Click again: ${l.length} done`)) return;
  const ids = l.map(t => t.uid);
  l.forEach(t => { t.status = "done"; call("set_done", { uid: t.uid, done: true }) });
  undoStack.push(() => ids.forEach(u => { const t = find(u); if (t) { t.status = "open"; call("set_done", { uid: u, done: false }) } }));
  showUndo(`${ids.length} done`); render();
});

/* ───────── add a task of your own, and the shortcuts card ───────── */
const vl = () => $("#veil");
function openAdd() {
  vl().dataset.kind = "add"; vl().classList.add("show");
  vl().innerHTML = `<div class="panel"><h2>Add a task</h2><p>For things that did not come by email. It lives in your list only.</p>
    <label class="f" for="at-title">What needs doing</label><input id="at-title" type="text" maxlength="200" autocomplete="off" placeholder="Renew passport">
    <div class="row2" style="grid-template-columns:1fr 1fr"><div><label class="f" for="at-pri">Priority</label><select id="at-pri">${[1, 2, 3, 4, 5].map(n => `<option value="${n}" ${n === 3 ? "selected" : ""}>${n}${n === 5 ? " (highest)" : n === 1 ? " (lowest)" : ""}</option>`).join("")}</select></div>
    <div><label class="f" for="at-due">Due (optional)</label><input id="at-due" type="date"></div></div>
    <div class="foot"><span class="sp"></span><button class="btn" data-vc="1">Cancel</button><button class="btn pri" data-add="1">Add</button></div></div>`;
  setTimeout(() => $("#at-title")?.focus(), 30);
}
async function submitAdd() {
  const title = $("#at-title").value.trim(); if (!title) { toast("Give the task a title"); return }
  try {
    const uid = await rpc("add_task", { title, priority: +$("#at-pri").value, due: $("#at-due").value || "" });
    view = "todo"; flt.cat = "all"; flt.only = ""; flt.q = ""; cur = uid; closeVeil(); toast("Task added");
  } catch (err) { toast(String(err).replace(/^Error: /, "")) }
}
const HELP = [["J K", "Move down / up"], ["Enter", "Open or close a task"], ["X", "Mark done"], ["1–5", "Set priority"], ["S", "Star"], ["R", "Reply draft"], ["H", "Snooze until tomorrow"], ["A", "Add a task"], ["/", "Search"], ["Z", "Undo"], [",", "Settings"], ["?", "This card"]];
function openHelp() {
  vl().dataset.kind = "help"; vl().classList.add("show");
  vl().innerHTML = `<div class="panel"><h2>Keyboard</h2><div class="keylist">${HELP.map(([k, l]) => `<div><span class="cap">${k}</span><b>${l}</b></div>`).join("")}</div>
    <div class="foot"><span class="sp"></span><button class="btn pri" data-vc="1">Close</button></div></div>`;
}
document.addEventListener("click", e => {
  if (vl().dataset.kind === "add" && e.target.closest("[data-add]")) submitAdd();
  else if ((vl().dataset.kind === "add" || vl().dataset.kind === "help") && e.target === vl()) closeVeil();
  else if (e.target.closest("[data-act=addtask]")) openAdd();
});
document.addEventListener("keydown", e => {
  const k = vl().dataset.kind;
  if (vl().classList.contains("show") && (k === "add" || k === "help")) {
    if (e.key === "Escape") closeVeil();
    else if (k === "add" && e.key === "Enter" && e.target.id === "at-title") { e.preventDefault(); submitAdd() }
    return;
  }
  if (e.metaKey || e.ctrlKey || e.altKey || vl().classList.contains("show") || /INPUT|SELECT|TEXTAREA/.test(e.target.tagName)) return;
  if (e.key === "a") { e.preventDefault(); openAdd() } else if (e.key === "?") { e.preventDefault(); openHelp() }
});
