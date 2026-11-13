"use strict";
/* ───────── demo backend (design preview; also the fallback when no engine is present) ───────── */
const M = (() => {
  const H = 3600, now = () => Date.now() / 1000;
  const mk = (uid, sender, addr, subject, hrs, p, category, action, urgent, star, excerpt, why, sig) => ({ uid, message_id: "<" + uid + ">", sender, addr, subject, received: now() - hrs * H, priority: p, category, action, urgent, status: "open", star, excerpt, why, signals: sig, needs_reply: false, due: "", snooze_until: 0, reply_to: addr, reply_warn: false, draft: null, pref: "", attachments: [], attachment_count: 0, unsubscribe: "", note: "", thread: "uid:" + uid, closed_by: "", gone: false, learned: 0 });
  return {
    mk, // used below to add the conversation demo data
    settings: { onboarded: qs.get("demo") !== null, account: { host: "imap.gmail.com", user: "you@example.com", port: 993 }, ai: { mode: "rules", ollama_model: "" }, alerts: { desktop: true, ntfy_topic: "" }, hub: { enabled: false, port: 8787, token: "" }, writeback: false },
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
    status: "idle", pending: 0, gmail: true,
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
M.settings.drafts = { auto: false, tone: "friendly", signature: "Best,\nSam", min_priority: 3, daily_cap: 25 };
Object.assign(M.settings.alerts, { quiet_start: "", quiet_end: "", digest_time: "", categories: [] });
const TEMPLATES = { thanks: "Thanks for your message. I've received it and will get back to you shortly.", confirm: "Thanks, that works for me. Consider it confirmed.", decline: "Thank you for thinking of me. Unfortunately I can't make this work, so I'll have to decline.", later: "Thanks for reaching out. I need a little more time and will reply properly by [date]." };
function mock(cmd, a) {
  const log = m => M.log.unshift({ at: Date.now() / 1000, msg: m });
  const t = () => M.tasks.find(x => x.uid === a.uid);
  switch (cmd) {
    case "state": return Promise.resolve(JSON.parse(JSON.stringify({ ...M, has_anthropic_key: false })));
    case "set_done": t().status = a.done ? "done" : "open"; log((a.done ? "archived “" : "reopened “") + (t().action || t().subject) + "”"); return Promise.resolve(true);
    case "set_snooze": if (a.date) { if (!/^\d{4}-\d{2}-\d{2}$/.test(a.date) || new Date(a.date + "T09:00:00") <= new Date()) return Promise.reject("Pick a day from tomorrow to a year ahead"); t().snooze_until = new Date(a.date + "T09:00:00").getTime() / 1000; return Promise.resolve(true) } { const p = a.preset; if (!["3h", "tomorrow", "nextweek", "clear"].includes(p)) return Promise.reject("unknown snooze option"); t().snooze_until = p === "clear" ? 0 : Date.now() / 1000 + (p === "3h" ? 10800 : p === "tomorrow" ? 86400 : 604800); log((p === "clear" ? "unsnoozed “" : "snoozed “") + (t().action || t().subject) + "”"); return Promise.resolve(true) }
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
    case "set_note": if (!t() || String(a.note).length > 2000) return Promise.resolve(false); t().note = a.note; return Promise.resolve(true);
    case "add_task": {
      const title = String(a.title || "").trim(); if (!title || title.length > 200) return Promise.reject("Give the task a title (up to 200 characters)");
      const uid = Math.max(2000000000, ...M.tasks.map(x => x.uid)) + (M.tasks.some(x => x.uid >= 2000000000) ? 1 : 0);
      M.tasks.push({ uid, message_id: "", sender: "You", addr: "", subject: title, received: Date.now() / 1000, priority: a.priority || 3, category: "personal", action: title, urgent: false, status: "open", star: false, excerpt: "", why: "Added by you", signals: {}, needs_reply: false, due: a.due || "", snooze_until: 0, reply_to: "", reply_warn: false, draft: null, pref: "", attachments: [], attachment_count: 0, unsubscribe: "", note: "", thread: "uid:" + uid, closed_by: "", gone: false, learned: 0 });
      return Promise.resolve(uid);
    }
    case "draft_discard": t().draft = null; return Promise.resolve(true);
    case "draft_to_mailbox": if (!M.settings.writeback) return Promise.reject("Turn on “Let Inbox Agent file mail” in Settings to save drafts to your mailbox"); if (t().draft) t().draft.saved = true; return Promise.resolve(true);
    case "export_markdown": return Promise.resolve("# To-do\n" + M.tasks.filter(x => x.status === "open" && !(x.snooze_until > Date.now() / 1000)).map(x => `\n- [ ] **P${x.priority}** ${x.action || x.subject} — ${x.sender} (${x.category})`).join("") + "\n");
    case "set_star": t().star = a.star; return Promise.resolve(true);
    case "set_priority": log(`P${t().priority}→P${a.priority} saved as a correction`); t().priority = a.priority; return Promise.resolve(true);
    case "test_account": return new Promise((res, rej) => setTimeout(() => /@/.test(a.user) && a.password ? res({ ok: true, sent: "[Gmail]/Sent Mail" }) : rej("login failed: enter an address and app password"), 900));
    case "save_account": M.settings.account = { host: a.host, port: a.port, user: a.user }; return Promise.resolve(true);
    case "save_settings": { const mg = (x, y) => { for (const k in y) { if (y[k] && typeof y[k] === "object") mg(x[k] ??= {}, y[k]); else x[k] = y[k] } }; mg(M.settings, a.patch); if (M.settings.hub.enabled && !M.settings.hub.token) M.settings.hub.token = "demo"; return Promise.resolve(M.settings) }
    case "set_secret": return Promise.resolve(true);
    case "detect_ollama": return Promise.resolve(["llama3.2:3b", "qwen2.5:7b"]);
    case "test_ai": return new Promise(r => setTimeout(() => r({ priority: 5, category: "work", action: "Sign the contract", why: "deadline today" }), 700));
    case "rotate_hub_token": return Promise.resolve(true);
    case "finish_setup": M.settings.onboarded = true; return Promise.resolve(true);
    case "sign_out": M.settings.onboarded = false; return Promise.resolve(true);
    case "hub_info": { let c = ""; for (let y = 0; y < 21; y++) for (let x = 0; x < 21; x++) if ((x * 7 + y * 13 + x * y) % 3 === 0) c += `<rect x="${x}" y="${y}" width="1" height="1"/>`; return Promise.resolve({ url: "http://192.168.1.20:8787/?token=demo", enabled: true, svg: `<svg viewBox="0 0 21 21" shape-rendering="crispEdges"><rect width="21" height="21" fill="#fff"/>${c}</svg>` }) }
  }
  return Promise.reject("unknown command");
}

