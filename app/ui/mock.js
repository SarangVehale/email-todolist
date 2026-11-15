"use strict";
/* ───────── demo backend (design preview; also the fallback when no engine is present) ───────── */
const M = (() => {
  const H = 3600, now = () => Date.now() / 1000;
  const mk = (uid, sender, addr, subject, hrs, p, category, action, urgent, star, excerpt, why, sig) => ({ uid, message_id: "<" + uid + ">", sender, addr, subject, received: now() - hrs * H, priority: p, category, action, urgent, status: "open", star, excerpt, why, signals: sig, needs_reply: false, due: "", snooze_until: 0, reply_to: addr, reply_warn: false, draft: null, pref: "", attachments: [], attachment_count: 0, unsubscribe: "", note: "", thread: "uid:" + uid, closed_by: "", gone: false, learned: 0, kind: "", remind: "", repeat: "", event: null });
  return {
    mk, // used below to add the conversation demo data
    settings: { onboarded: new URLSearchParams(location.search).get("demo") !== null, account: { id: 0, host: "imap.gmail.com", user: "you@example.com", port: 993 }, more_accounts: [{ id: 1, host: "imap.work.example", user: "sam@work.example", port: 993 }], ai: { mode: "rules", ollama_model: "" }, alerts: { desktop: true, ntfy_topic: "" }, hub: { enabled: false, port: 8787, token: "" }, writeback: false },
    tasks: [
      mk(3, "Priya Nair", "priya@acme.example", "Re: Q4 contract — need your sign-off today", 2, 5, "work", "Review and sign the Q4 contract", true, true, "Legal cleared the last changes. Can you sign by 5pm so we can countersign before the deadline?", "Reply to your own mail from a known colleague; deadline today.", { known: true, reply: true, bulk: false }),
      mk(7, "Riverside Clinic", "appointments@clinic.example", "Appointment confirmation request", 5, 4, "personal", "Confirm Thursday 10:30 appointment", false, false, "Please confirm your slot on Thursday at 10:30 or we will release it.", "Known sender; the slot is released if unanswered.", { known: true, reply: false, bulk: false }),
      mk(12, "Ravi (Dad)", "ravi.k@family.example", "Flight details for Diwali", 26, 4, "personal", "Pick a flight for Diwali and reply", false, true, "Sending three options. Tell me which works so I can book tonight.", "Family, direct question.", { known: true, reply: false, bulk: false }),
      mk(15, "City Bank", "alerts@bank.example", "Statement ready · card ending 4412", 30, 3, "finance", "Pay card statement by 20 Nov", false, false, "October statement available. Total due $184.20, due 20 Nov.", "Automated, but contains a due date.", { known: false, reply: false, bulk: true }),
      mk(18, "CodeHost", "notifications@codehost.example", "[mail-client] PR #12 review requested", 8, 3, "work", "Review PR #12", false, false, "You were requested to review: Add triage overrides table.", "Review request addressed to you.", { known: false, reply: false, bulk: true }),
      mk(21, "Weekly Digest", "no-reply@digest.example", "Your weekly reading list", 40, 1, "reading", "Skim weekly reading list", false, false, "Top stories picked for you this week…", "Newsletter; List-Unsubscribe present.", { known: false, reply: false, bulk: true }),
      mk(22, "FoodApp", "offers@foodapp.example", "50% off your next order", 3, 1, "promo", "", false, false, "Treat yourself tonight with code FEAST50.", "Promotion, no action.", { known: false, reply: false, bulk: true }),
    ],
    log: [{ at: now() - 60, msg: "connected as you@example.com; Sent folder “[Gmail]/Sent Mail”" }, { at: now() - 50, msg: "triaged “Re: Q4 contract” → P5, work, urgent" }],
    status: "idle", pending: 0, gmail: true, update_feature: true, update: { version: "0.2.0", url: "https://github.com/example/inbox-agent/releases/tag/v0.2.0" },
  };
})();
/* sample reply, due date and category so the design can be previewed */
Object.assign(M.tasks[0], { needs_reply: true, due: new Date(Date.now() + 864e5).toISOString().slice(0, 10), draft: { body: "Hi Priya,\n\nThanks for flagging this. Legal's changes look good to me, so I'll sign it this afternoon and send it back before 5pm.\n\nBest,\nSam", source: "ai", saved: false, updated: Date.now() / 1000 } });
Object.assign(M.tasks[1], { needs_reply: true });
// a conversation: an older message from the same thread as the contract mail, a message that left the inbox,
// an automatically closed task and a learned shift
M.tasks.push(M.mk(2, "Priya Nair", "priya@acme.example", "Q4 contract draft attached", 20, 3, "work", "Review the Q4 contract draft", false, false, "Draft attached for review.", "Reply to your own mail from a known colleague.", { known: true, reply: false, bulk: false }));
Object.assign(M.tasks[M.tasks.length - 1], { thread: "<q4-contract@acme.example>" });
Object.assign(M.tasks[0], { thread: "<q4-contract@acme.example>" });
Object.assign(M.tasks[4], { gone: true });
M.tasks.push(M.mk(30, "Dana Ortiz", "dana@studio.example", "Re: invoice 2291", 12, 3, "finance", "Invoice 2291", false, false, "Thanks, paid today.", "Reply to your mail.", { known: true, reply: true, bulk: false }));
Object.assign(M.tasks[M.tasks.length - 1], { status: "done", closed_by: "replied" });
Object.assign(M.tasks[2], { learned: 1, why: "Rule-based: Someone you write to. Adjusted from your corrections." });
Object.assign(M.tasks[3], { attachments: ["statement-october.pdf"], attachment_count: 1 });
Object.assign(M.tasks[5], { unsubscribe: "https://digest.example/unsubscribe?u=1" });
Object.assign(M.tasks[2], { needs_reply: true, due: "2026-11-11", reply_to: "ravi@other-domain.example", reply_warn: true });
M.settings.followups = { days: 3 };
M.rules = [{ id: "demo0001", field: "domain", text: "digest.example", priority: 1, category: null, file: false }];
M.tasks.push(Object.assign(M.mk(2000000100, "You", "", "Follow up: Quote for the kitchen", 90, 3, "personal", "Follow up: Quote for the kitchen", false, false, "You wrote to ana@builder.example on 9 Nov about “Quote for the kitchen”. Nobody has replied yet.", "Nobody has replied to something you sent", {}), { message_id: "", reply_to: "", kind: "followup", thread: "uid:2000000100" }));
// a repeating reminder of your own, and an invitation with an event, for the preview
M.tasks.push(Object.assign(M.mk(2000000200, "You", "", "Put the bins out", 30, 3, "personal", "Put the bins out", false, false, "", "Added by you", {}), { message_id: "", reply_to: "", thread: "uid:2000000200", remind: "2026-11-17 07:00", repeat: "weekly", due: "2026-11-17" }));
Object.assign(M.tasks.find(t => t.uid === 7), { event: { summary: "Appointment at Riverside Clinic", start: "20991120T103000Z", end: "20991120T110000Z", tzid: "", location: "Riverside Clinic, 2nd floor", all_day: false, cancelled: false } });
const BODIES = { 12: "Sending three options. The boiler flight is the cheapest one, tell me which works.", 15: "Statement attached. Your direct debit for the boiler service plan is due." };
for (const x of M.tasks) x.mailbox = x.uid >= 2000000000 ? (x.kind === "followup" ? 0 : null) : 0;
for (const u of [15, 18]) { const x = M.tasks.find(y => y.uid === u); x.uid += 400000000; x.mailbox = 1; if (BODIES[u]) { BODIES[u + 400000000] = BODIES[u]; delete BODIES[u] } }
// A mail server port that is given must be a whole number from 1 to 65535 (the engine refuses anything else; none means 993).
const portBad = a => a.port !== undefined && a.port !== null && !(Number.isInteger(a.port) && a.port >= 1 && a.port <= 65535);
const syncAccounts = () => { M.accounts = [M.settings.account, ...M.settings.more_accounts].filter(m => m.host).map(m => ({ id: m.id, user: m.user, host: m.host, gmail: /(^|\.)gmail\.com$/.test(m.host), status: "idle" })) };
syncAccounts();
M.settings.drafts = { auto: false, tone: "friendly", signature: "Best,\nSam", min_priority: 3, daily_cap: 25 };
Object.assign(M.settings.alerts, { quiet_start: "", quiet_end: "", digest_time: "", categories: [] });
const TEMPLATES = { thanks: "Thanks for your message. I've received it and will get back to you shortly.", confirm: "Thanks, that works for me. Consider it confirmed.", decline: "Thank you for thinking of me. Unfortunately I can't make this work, so I'll have to decline.", later: "Thanks for reaching out. I need a little more time and will reply properly by [date]." };
function mock(cmd, a) {
  const log = m => M.log.unshift({ at: Date.now() / 1000, msg: m });
  const t = () => M.tasks.find(x => x.uid === a.uid);
  switch (cmd) {
    case "state": {
      // like the engine: every task that is not finished, and the newest 200 finished ones with their total
      const done = M.tasks.filter(x => x.status === "done").sort((x, y) => (y.done_at || y.received) - (x.done_at || x.received)), shown = Math.min(20000, Math.max(200, Number.isFinite(+(a && a.done)) ? +a.done : 200));
      return Promise.resolve(JSON.parse(JSON.stringify({ ...M, tasks: [...M.tasks.filter(x => x.status !== "done"), ...done.slice(0, shown)], done_total: done.length, has_anthropic_key: !!M.hasKey, rules: M.rules })));
    }
    case "set_done": if (t().repeat && a.done && t().uid >= 2000000000) { const nx = (t().remind || t().due || "2026-11-17").slice(0, 10); const d = new Date(nx + "T12:00:00"); d.setDate(d.getDate() + (t().repeat === "daily" ? 1 : t().repeat === "weekly" ? 7 : 30)); const iso = d.toISOString().slice(0, 10); if (t().remind) t().remind = iso + t().remind.slice(10); if (t().due) t().due = iso; return Promise.resolve(true) } t().status = a.done ? "done" : "open"; t().done_at = a.done ? Date.now() / 1000 : 0; log((a.done ? "archived “" : "reopened “") + (t().action || t().subject) + "”"); return Promise.resolve(true);
    case "set_snooze": if (a.date) { if (!/^\d{4}-\d{2}-\d{2}$/.test(a.date) || new Date(a.date + "T09:00:00") <= new Date()) return Promise.reject("Pick a day from today (only before 09:00) to a year ahead"); t().snooze_until = new Date(a.date + "T09:00:00").getTime() / 1000; return Promise.resolve(true) } { const p = a.preset; if (!["3h", "tomorrow", "nextweek", "clear"].includes(p)) return Promise.reject("unknown snooze option"); t().snooze_until = p === "clear" ? 0 : Date.now() / 1000 + (p === "3h" ? 10800 : p === "tomorrow" ? 86400 : 604800); log((p === "clear" ? "unsnoozed “" : "snoozed “") + (t().action || t().subject) + "”"); return Promise.resolve(true) }
    case "set_pref": { const x = t(); M.tasks.filter(y => y.addr === x.addr && y.status !== "done").forEach(y => { y.pref = a.level === "normal" ? "" : a.level; if (a.level === "important") y.priority = Math.max(y.priority, 4); if (a.level === "mute") { y.priority = 1; y.urgent = false } }); return Promise.resolve(true) }
    case "draft_generate": if (t().draft && t().draft.source === "user" && !a.force) return Promise.reject("This draft has your edits. Confirm to replace it."); return new Promise(res => setTimeout(() => {
      const ins = (a.instruction || "").toLowerCase(), key = ins.includes("declin") ? "decline" : ins.includes("confirm") ? "confirm" : ins.includes("time") ? "later" : "thanks";
      const name = t().sender.split(" ")[0], sig = M.settings.drafts.signature, ai = M.settings.ai.mode !== "rules";
      const body = `Hi ${name},\n\n${ai ? "Thanks for this. " : ""}${TEMPLATES[key]}${sig ? "\n\n" + sig : ""}`;
      t().draft = { body, source: ai ? "ai" : "template", saved: false, updated: Date.now() / 1000 }; log("wrote reply draft for “" + (t().action || t().subject) + "”");
      res({ body, source: t().draft.source });
    }, 700));
    case "draft_update": if (t().draft) { t().draft.body = a.body; t().draft.source = "user" } return Promise.resolve(true);
    case "set_due": if (!t() || (a.due && !/^\d{4}-\d{2}-\d{2}$/.test(a.due))) return Promise.resolve(false); t().due = a.due || ""; return Promise.resolve(true);
    case "set_title": if (!t() || t().uid < 2000000000 || !String(a.title || "").trim()) return Promise.resolve(false); t().subject = t().action = String(a.title).trim(); return Promise.resolve(true);
    case "read_body": return Promise.resolve(t() ? (t().excerpt || "") + "\n\n(This is demo text. The real app shows the stored message text, up to 3,000 characters.)" : "");
    case "diagnostics": return Promise.resolve("Inbox Agent diagnostics\nVersion: demo\nSystem: demo\nStatus: idle\nTasks: 9 open, 1 done, 1 left the inbox\nOptions: ranking rules · write-back off · phone hub off · quiet hours off\nMail server: imap.gmail.com\n\nRecent activity (newest first, private text removed):\n  triaged “…” → P5, work, urgent\n\nCrash log (last lines, private text removed):\n  none\n");
    case "set_note": if (!t() || String(a.note).length > 2000) return Promise.resolve(false); t().note = a.note; return Promise.resolve(true);
    case "add_task": {
      const title = String(a.title || "").trim(); if (!title || title.length > 200) return Promise.reject("Give the task a title (up to 200 characters)");
      const uid = Math.max(2000000000, ...M.tasks.map(x => x.uid)) + (M.tasks.some(x => x.uid >= 2000000000) ? 1 : 0);
      M.tasks.push({ uid, message_id: "", sender: "You", addr: "", subject: title, received: Date.now() / 1000, priority: a.priority || 3, category: "personal", action: title, urgent: false, status: "open", star: false, excerpt: "", why: "Added by you", signals: {}, needs_reply: false, due: a.due || "", snooze_until: 0, reply_to: "", reply_warn: false, draft: null, pref: "", attachments: [], attachment_count: 0, unsubscribe: "", note: "", thread: "uid:" + uid, closed_by: "", gone: false, learned: 0, kind: "", remind: "", repeat: "", event: null });
      return Promise.resolve(uid);
    }
    case "draft_discard": t().draft = null; return Promise.resolve(true);
    case "draft_to_mailbox": if (!M.settings.writeback) return Promise.reject("Turn on “Let Inbox Agent file mail” in Settings to save drafts to your mailbox"); if (t().draft) t().draft.saved = true; return Promise.resolve(true);
    case "backup_export": return Promise.resolve({ path: "C:/Users/you/Downloads/inbox-agent-backup-20261114-091500.json", counts: { rules: M.rules.length, sender_choices: 0, corrections: 1, own_tasks: M.tasks.filter(x => x.uid >= 2000000000 && x.kind !== "followup").length, mail_state: 3, mailboxes: M.accounts.length, left_out: {} } });
    case "backup_import": {
      let b; try { b = JSON.parse(String(a.json || "")) } catch { return Promise.reject("This is not a backup made by Inbox Agent: it could not be read") }
      if (!b || b.format !== "inbox-agent-backup") return Promise.reject("This is not a backup made by Inbox Agent");
      if (!(b.version >= 1)) return Promise.reject("This is not a backup made by Inbox Agent: it has no version");
      if (b.version > 1) return Promise.reject("This backup was made by a newer version of Inbox Agent. Update the app, then restore it");
      if (!M.settings.onboarded) return Promise.reject("Finish setting up your first mailbox before restoring a backup");
      const newRules = (b.rules || []).filter(r => !M.rules.some(x => x.field === r.field && String(x.text).toLowerCase() === String(r.text).toLowerCase())).slice(0, 50 - M.rules.length);
      const newOwn = (b.own_tasks || []).filter(t => !M.tasks.some(x => x.uid >= 2000000000 && x.subject === t.title && (x.due || "") === (t.due || "")));
      const newBoxes = a.settings ? (b.mailboxes || []).filter(m => !M.accounts.some(x => x.user.toLowerCase() === String(m.user).toLowerCase() && x.host.toLowerCase() === String(m.host).toLowerCase())) : [];
      const rep = { applied: !!a.apply, rules_added: newRules.length, rules_skipped: (b.rules || []).length - newRules.length, sender_choices_added: (b.sender_choices || []).length, sender_choices_skipped: 0, corrections_added: (b.corrections || []).length, corrections_skipped: 0,
        own_tasks_added: newOwn.length, own_tasks_skipped: (b.own_tasks || []).length - newOwn.length, reminders_dropped: 0, mail_applied: 0, mail_unchanged: 0, mail_parked: (b.mail_state || []).length, mail_skipped_no_mailbox: 0, mail_skipped_full: 0,
        options_changed: a.settings && b.settings ? Object.keys(b.settings).map(k => k + ".*") : [], mailboxes_added: newBoxes.length, mailboxes_skipped: (b.mailboxes || []).length - newBoxes.length,
        options_skipped: a.settings && b.settings && b.settings.ai && b.settings.ai.mode === "anthropic" ? ["ai.mode"] : [],
        mailboxes_new: newBoxes.map(m => `${m.user} (${m.host}:${m.port || 993})`),
        rules_new: newRules.slice(0, 20).map(r => `If the ${r.field === "subject" ? "subject contains" : r.field === "domain" ? "domain is" : "address is"} ${r.text}: ${r.file ? "file the mail away (mark it done)" : "priority " + r.priority}`) };
      if (a.apply) {
        newRules.forEach(r => M.rules.push({ id: Math.random().toString(36).slice(2, 10), field: r.field, text: r.text, priority: r.priority ?? null, category: r.category ?? null, file: !!r.file }));
        newOwn.forEach((t, i) => M.tasks.push(Object.assign(M.mk(2000000300 + M.tasks.length + i, "You", "", t.title, 1, t.priority || 3, "personal", t.title, false, !!t.star, "", "Added by you", {}), { message_id: "", reply_to: "", thread: "uid:r" + i + t.title, mailbox: null, due: t.due || "", note: t.note || "" })));
        newBoxes.forEach(m => { const used = new Set(M.accounts.map(x => x.id)); let id = 0; while (used.has(id)) id++; if (id < 5) { if (id === 0) M.settings.account = { id, host: m.host, port: m.port || 993, user: m.user }; else M.settings.more_accounts.push({ id, host: m.host, port: m.port || 993, user: m.user }); syncAccounts() } });
      }
      return Promise.resolve(rep);
    }
    case "export_markdown": return Promise.resolve("# To-do\n" + M.tasks.filter(x => x.status === "open" && !(x.snooze_until > Date.now() / 1000)).map(x => `\n- [ ] **P${x.priority}** ${x.action || x.subject} — ${x.sender} (${x.category})`).join("") + "\n");
    case "set_star": t().star = a.star; return Promise.resolve(true);
    case "set_priority": log(`P${t().priority}→P${a.priority} saved as a correction`); t().priority = a.priority; return Promise.resolve(true);
    case "test_account": {
      // the same checks, in the same order, as the engine: nothing connects until they pass
      if (portBad(a)) return Promise.reject("That mail server port isn't valid");
      if (!a.host || !a.user) return Promise.reject("Enter the mail server and the email address");
      if (!/^[A-Za-z0-9.-]{1,253}$/.test(a.host)) return Promise.reject("That mail server name isn't valid");
      if (/[\u0000-\u001f\u007f-\u009f]/.test(a.user) || a.user.length > 320) return Promise.reject("That email address isn't valid");
      if (!String(a.password || "").replace(/ /g, "")) return Promise.reject("Enter the app password");
      return new Promise((res, rej) => setTimeout(() => /@/.test(a.user) ? res({ ok: true, sent: "[Gmail]/Sent Mail" }) : rej("login failed: enter an address and app password"), 900));
    }
    case "save_account": {
      if (!String(a.password || "").replace(/ /g, "")) return Promise.reject("Enter the app password for this mailbox");
      if (portBad(a)) return Promise.reject("That mail server port isn't valid");
      if (!a.host || !a.user) return Promise.reject("Enter the mail server and the email address");
      const id = a.id || 0;
      if (id === 0) M.settings.account = { id: 0, host: a.host, port: a.port, user: a.user };
      else { const m = M.settings.more_accounts.find(x => x.id === id); if (!m) return Promise.reject("There is no such mailbox"); Object.assign(m, { host: a.host, port: a.port, user: a.user }) }
      syncAccounts(); return Promise.resolve(true);
    }
    case "add_account": {
      if (!M.settings.onboarded || !M.accounts.length) return Promise.reject("Set up your first mailbox before adding another");
      if (!String(a.password || "").replace(/ /g, "")) return Promise.reject("Enter the app password for this mailbox");
      if (M.accounts.length >= 5) return Promise.reject("You can read at most five mailboxes at once");
      if (portBad(a)) return Promise.reject("That mail server port isn't valid");
      if (!/@/.test(a.user || "") || !a.host) return Promise.reject("Enter the mail server and the email address of this mailbox");
      if (M.accounts.some(m => m.host.toLowerCase() === String(a.host).toLowerCase() && m.user.toLowerCase() === String(a.user).toLowerCase())) return Promise.reject("That mailbox is already in the app");
      const used = new Set(M.accounts.map(m => m.id)); let id = 0; while (used.has(id)) id++;
      if (id === 0) M.settings.account = { id, host: a.host, port: a.port || 993, user: a.user }; else M.settings.more_accounts.push({ id, host: a.host, port: a.port || 993, user: a.user }); syncAccounts(); return Promise.resolve(id);
    }
    case "remove_account": {
      const id = a.id, had = M.accounts.some(m => m.id === id); if (!had) return Promise.reject("There is no such mailbox");
      M.tasks = M.tasks.filter(x => x.mailbox !== id);
      if (id === 0) M.settings.account = { id: 0, host: "", user: "", port: 993 }; else M.settings.more_accounts = M.settings.more_accounts.filter(m => m.id !== id);
      syncAccounts(); if (!M.accounts.length) M.settings.onboarded = false; return Promise.resolve(true);
    }
    case "save_settings": { const mg = (x, y) => { for (const k in y) { if (y[k] && typeof y[k] === "object") mg(x[k] ??= {}, y[k]); else x[k] = y[k] } }; mg(M.settings, a.patch); if (M.settings.hub.enabled && !M.settings.hub.token) M.settings.hub.token = "demo"; return Promise.resolve(M.settings) }
    case "set_reminder": {
      const x = t(); if (!x || x.uid < 2000000000) return Promise.reject(x ? "Reminders are for tasks you added yourself. To see a message later, snooze it" : "no such task");
      const at = a.at || "", rep = a.repeat || "";
      if (!["", "daily", "weekly", "monthly"].includes(rep)) return Promise.reject("A task can repeat daily, weekly or monthly");
      if (at && !/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/.test(at)) return Promise.reject("Pick a date and a time");
      if (at && at < nowLocal()) return Promise.reject("Pick a time that has not passed");
      if (!at && rep && !x.due) return Promise.reject("A repeat needs a reminder time or a due date");
      x.remind = at; x.repeat = at || rep ? rep : ""; return Promise.resolve(true);
    }
    case "search": { const q = String(a.q || "").toLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean); return Promise.resolve(q.length ? M.tasks.filter(x => { const hay = [x.sender, x.addr, x.subject, x.excerpt, x.note, BODIES[x.uid] || ""].join(" ").toLowerCase(); return q.every(w => hay.includes(w)) }).map(x => x.uid) : []); }
    case "save_event": { const x = t(); if (!x) return Promise.reject("no such task"); if (!x.event) return Promise.reject("This message has no event to save"); if (x.event.cancelled) return Promise.reject("This event was cancelled, so there is nothing to add to a calendar"); return Promise.resolve("C:\\Users\\you\\Downloads\\invite-appointment-at-riverside-clinic-20991120.ics"); }
    case "rules_save": {
      const rs = a.rules;
      if (!Array.isArray(rs) || rs.length > 50) return Promise.reject("At most 50 rules are allowed");
      const ids = new Set();
      for (const r of rs) {
        if (!r.id || ids.has(r.id)) return Promise.reject("Every rule needs its own short id");
        ids.add(r.id);
        if (!["sender", "domain", "subject"].includes(r.field)) return Promise.reject("A rule looks at the sender, the domain or the subject");
        if (!String(r.text || "").trim() || String(r.text).length > 100) return Promise.reject("A rule needs text of 1 to 100 characters");
        if (!r.priority && !r.category && !r.file) return Promise.reject("A rule has to do something: set a priority, set a category, or file the mail");
        if (r.file && r.priority) return Promise.reject("Filing mail and setting its priority don't go together");
      }
      M.rules = JSON.parse(JSON.stringify(rs)); return Promise.resolve(true);
    }
    case "rules_apply": {
      const hit = x => x.status === "open" && x.uid < 2000000000 && !x.pref && M.rules.find(r => { const t = r.text.toLowerCase(), ad = (x.addr || "").toLowerCase(); return r.field === "domain" ? ad.endsWith("@" + t) || ad.endsWith("." + t) : r.field === "subject" ? x.subject.toLowerCase().includes(t) : ad === t });
      let matched = 0, changed = 0, filed = 0;
      M.tasks.forEach(x => { const r = hit(x); if (!r) return; matched++; const np = r.file ? Math.min(x.priority, 2) : (r.priority || x.priority), nc = r.category || x.category; if (np === x.priority && nc === x.category && !r.file) return; changed++; if (r.file) filed++; if (!a.dry) { x.priority = np; x.category = nc; if (r.file) x.status = "done" } });
      return Promise.resolve({ matched, changed, filed });
    }
    case "set_secret": if (a.name === "anthropic-key" && a.value) M.hasKey = true; return Promise.resolve(true);
    case "detect_ollama": return Promise.resolve(["llama3.2:3b", "qwen2.5:7b"]);
    case "test_ai": { const ai = M.settings.ai || {}; if (ai.mode === "ollama" && !String(ai.ollama_model || "").trim()) return Promise.reject("No model is chosen"); if (ai.mode === "anthropic" && !M.hasKey) return Promise.reject("No API key is saved"); } return new Promise(r => setTimeout(() => r({ priority: 5, category: "work", action: "Sign the contract", why: "deadline today" }), 700));
    case "rotate_hub_token": return Promise.resolve(true);
    case "finish_setup": if (!M.settings.account.host || !M.settings.account.user) return Promise.reject("Add your first mailbox before finishing setup"); M.settings.onboarded = true; return Promise.resolve(true);
    case "sign_out": M.settings.onboarded = false; M.settings.account = { id: 0, host: "", user: "", port: 993 }; M.settings.more_accounts = []; syncAccounts(); return Promise.resolve(true);
    case "hub_info": { let c = ""; for (let y = 0; y < 21; y++) for (let x = 0; x < 21; x++) if ((x * 7 + y * 13 + x * y) % 3 === 0) c += `<rect x="${x}" y="${y}" width="1" height="1"/>`; return Promise.resolve({ url: "http://192.168.1.20:8787/?token=demo", enabled: true, svg: `<svg viewBox="0 0 21 21" shape-rendering="crispEdges"><rect width="21" height="21" fill="#fff"/>${c}</svg>` }) }
  }
  return Promise.reject("unknown command");
}

