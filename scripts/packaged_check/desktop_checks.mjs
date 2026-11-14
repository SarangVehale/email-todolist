// Desktop-only commands, exercised through the real Tauri window's IPC (the phone hub must not reach these).
let pages = [];
for (let i = 0; i < 40 && !pages.length; i++) {
  try { pages = (await (await fetch("http://127.0.0.1:9222/json")).json()).filter(p => p.type === "page"); } catch { /* not up yet */ }
  if (!pages.length) await new Promise(r => setTimeout(r, 500));
}
if (!pages.length) { console.log("FAIL desktop window not reachable"); process.exit(1); }
const ws = new WebSocket(pages[0].webSocketDebuggerUrl);
await new Promise(r => (ws.onopen = r));
let id = 0; const pending = new Map();
ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id) } };
const send = (method, params = {}) => new Promise(r => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })) });
const call = async (cmd, args) => {
  const expr = `(async () => { try { return { ok: await window.__TAURI__.core.invoke("rpc", { cmd: ${JSON.stringify(cmd)}, args: ${JSON.stringify(args)} }) } } catch (e) { return { err: String(e) } } })()`;
  return (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result.result.value;
};
let bad = 0;
const check = (name, cond, detail) => { console.log((cond ? "PASS " : "FAIL ") + name + (cond ? "" : "  [" + JSON.stringify(detail) + "]")); if (!cond) bad++; };

let r = await call("draft_generate", { uid: 1, instruction: "confirm" });
check("desktop: draft_generate over the user's edit is refused without force", r.err && r.err.includes("your edits"), r);
r = await call("draft_generate", { uid: 1, instruction: "confirm", force: true });
check("desktop: draft_generate with force writes a template reply", r.ok && r.ok.source === "template" && r.ok.body.includes("confirmed"), r);
r = await call("draft_to_mailbox", { uid: 1 });
check("desktop: saving to the mailbox is refused while write-back is off", r.err && r.err.includes("Let Inbox Agent file mail"), r);
r = await call("save_settings", { patch: { writeback: true } });
check("desktop: write-back and a valid login can be saved", r.ok && r.ok.writeback === true, r);
for (let i = 0; i < 3; i++) r = await call("draft_to_mailbox", { uid: 1 });
check("desktop: draft_to_mailbox accepted (repeated clicks are harmless)", r.ok === true, r);
r = await call("export_markdown", {});
check("desktop: export works and escapes markup", typeof r.ok === "string" && r.ok.startsWith("# To-do") && !r.ok.includes("[click]("), r);
r = await call("draft_to_mailbox", { uid: 3 });
check("desktop: no draft means nothing to save", r.err && /no draft/i.test(r.err), r);
// Phase 4: rules and follow-ups, through the real window
const good = [{ id: "r1", field: "domain", text: "shop.com", priority: 1, category: null, file: false }];
r = await call("rules_save", { rules: good });
check("desktop: rules can be saved", r.ok === true, r);
r = await call("rules_save", { rules: [{ id: "r2", field: "body", text: "x", priority: 5, category: null, file: false }] });
check("desktop: an invalid rule is refused and says why", r.err && /sender, the domain or the subject/.test(r.err), r);
r = await call("rules_apply", { dry: true });
check("desktop: a dry run skips a sender that has a saved preference (the shop was marked important)", r.ok && r.ok.matched === 0 && r.ok.changed === 0 && r.ok.filed === 0, r);
r = await call("state", {});
check("desktop: state carries the saved rules and the follow-up setting", r.ok && r.ok.rules.length === 1 && r.ok.rules[0].text === "shop.com" && r.ok.settings.followups.days === 0, r.ok && { rules: r.ok.rules, f: r.ok.settings.followups });
r = await call("save_settings", { patch: { followups: { days: 4 } } });
check("desktop: an unoffered follow-up wait is refused", r.err && /2, 3, 5 or 7/.test(r.err), r);
r = await call("save_settings", { patch: { followups: { days: 3 } } });
check("desktop: a follow-up wait of 3 days is saved", r.ok && r.ok.followups.days === 3, r);
// Phase 5: search inside message text, and saving an invitation's event
r = await call("search", { q: "thanks" });
check("desktop: search finds a word that is only in the stored message text", r.ok && r.ok.includes(1), r);
r = await call("search", { q: 'OR "x" * body:thanks NEAR(a b)' });
check("desktop: what is typed is never search syntax (it finds nothing, and nothing breaks)", Array.isArray(r.ok) && !r.err, r);
r = await call("search", { q: "" });
check("desktop: an empty search finds nothing", Array.isArray(r.ok) && r.ok.length === 0, r);
r = await call("save_event", { uid: 1 });
check("desktop: a message without an event cannot be saved", r.err && /no event/.test(r.err), r);
r = await call("save_event", { uid: 3 });
check("desktop: a cancelled invitation cannot be saved (nothing is written)", r.err && /cancelled/.test(r.err), r);
// Phase 6: several mailboxes, through the real window
r = await call("state", {});
check("desktop: state lists the mailboxes (one, the seeded first) with how each is doing", r.ok && Array.isArray(r.ok.accounts) && r.ok.accounts.length === 1 && r.ok.accounts[0].id === 0 && r.ok.accounts[0].user === "me@example.com" && typeof r.ok.accounts[0].status === "string" && r.ok.settings.more_accounts.length === 0, r.ok && r.ok.accounts);
check("desktop: no mailbox entry carries a password", r.ok && r.ok.accounts.every(a => !("password" in a)) && !JSON.stringify(r.ok.settings).toLowerCase().includes("password"), r.ok && r.ok.settings);
r = await call("add_account", { host: "imap.example.org", port: 993, user: "second@example.org", password: "pw" });
check("desktop: a mailbox cannot be added before setup is finished", r.err && /first mailbox/.test(r.err), r);
r = await call("remove_account", { id: 3 });
check("desktop: removing a mailbox that does not exist is refused", r.err && /no such mailbox/i.test(r.err), r);
r = await call("save_account", { id: 2, host: "imap.example.org", port: 993, user: "second@example.org", password: "pw" });
check("desktop: a new password for a mailbox that does not exist is refused", r.err && /no such mailbox/i.test(r.err), r);
r = await call("save_settings", { patch: { more_accounts: [{ id: 1, host: "evil.example", port: 993, user: "x@y.example" }] } });
check("desktop: mailboxes cannot be added through settings (their data and password are not looked after there)", r.err && /save_account/.test(r.err), r);
r = await call("state", {});
check("desktop: and none was added", r.ok && r.ok.accounts.length === 1 && r.ok.settings.more_accounts.length === 0, r.ok && r.ok.accounts);

// Phase 7: backup and restore, through the real window
import fs from "node:fs";
import os from "node:os";
const sleep = ms => new Promise(r => setTimeout(r, ms));
const evalJs = async expr => (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result.result.value;
r = await call("backup_import", { json: JSON.stringify({ format: "inbox-agent-backup", version: 1 }), apply: false, settings: false });
check("desktop: a restore is refused until setup is finished", r.err && /Finish setting up/.test(r.err), r);
r = await call("finish_setup", {});
check("desktop: setup is refused while the first mailbox has no password", r.err && /first mailbox/.test(r.err), r);
r = await call("save_account", { host: "imap.example.com", port: 993, user: "me@example.com", password: "app-password-for-the-test" });
check("desktop: the first mailbox is saved with its password", r.ok === true, r);
r = await call("finish_setup", {});
check("desktop: setup can be finished (a restore needs it)", r.ok === true, r);
r = await call("backup_import", { json: "hello", apply: false, settings: false });
check("desktop: a file that is not a backup is refused with a plain message", r.err && /not a backup/.test(r.err), r);
r = await call("backup_import", { json: JSON.stringify({ format: "inbox-agent-backup", version: 2 }), apply: true, settings: true });
check("desktop: a backup from a newer version is refused", r.err && /newer version/.test(r.err), r);
const wanted = { format: "inbox-agent-backup", version: 1,
  rules: [{ id: "z", field: "domain", text: "backup-test.example", priority: 2 }],
  sender_choices: [{ addr: "someone@backup-test.example", level: "mute" }],
  own_tasks: [{ title: "Restored in the packaged app", priority: 4, due: "2027-03-01", note: "hello" }],
  settings: { ai: { mode: "anthropic" }, hub: { enabled: true, token: "evil" }, writeback: true } };
const before = (await call("state", {})).ok;
r = await call("backup_import", { json: JSON.stringify(wanted), apply: false, settings: true });
check("desktop: a preview says what would be added and what is not restored", r.ok && r.ok.applied === false && r.ok.rules_added === 1 && r.ok.own_tasks_added === 1 && r.ok.sender_choices_added === 1, r);
const mid = (await call("state", {})).ok;
check("desktop: a preview writes nothing", mid.rules.length === before.rules.length && mid.tasks.length === before.tasks.length, { rules: mid.rules.length, tasks: mid.tasks.length });
r = await call("backup_import", { json: JSON.stringify(wanted), apply: true, settings: true });
check("desktop: a restore adds the rule, the task and the sender choice", r.ok && r.ok.applied === true && r.ok.rules_added === 1 && r.ok.own_tasks_added === 1, r);
const after = (await call("state", {})).ok;
check("desktop: the restored data is in the list", after.rules.some(x => x.text === "backup-test.example") && after.tasks.some(t => t.subject === "Restored in the packaged app" && t.note === "hello"), after.rules);
check("desktop: a file cannot turn on the phone hub, write-back or the paid ranking", after.settings.hub.enabled === before.settings.hub.enabled && after.settings.writeback === before.settings.writeback && after.settings.ai.mode !== "anthropic", { hub: after.settings.hub.enabled, wb: after.settings.writeback, ai: after.settings.ai.mode });
r = await call("backup_import", { json: JSON.stringify(wanted), apply: true, settings: true });
check("desktop: restoring the same file again adds nothing", r.ok && r.ok.rules_added === 0 && r.ok.own_tasks_added === 0 && r.ok.sender_choices_added === 0, r);
// the file itself: saved to Downloads, readable, with no secret in it; then removed again
r = await call("backup_export", {});
let exported = null;
try { exported = r.ok && fs.readFileSync(r.ok.path, "utf8"); } catch (e) { exported = null; }
check("desktop: a backup file is written and says what it holds", exported && /inbox-agent-backup-\d{8}-\d{6}\.json$/.test(r.ok.path) && JSON.parse(exported).format === "inbox-agent-backup" && exported.includes("backup-test.example") && exported.includes("Restored in the packaged app"), r);
check("desktop: the backup holds no password, phone code or notification topic", exported && !/password|\"token\"|ntfy_topic|ntfy_server|ollama_url/i.test(exported), exported && exported.slice(0, 300));
if (r.ok && r.ok.path) { try { fs.unlinkSync(r.ok.path); } catch (e) { /* left in Downloads */ } }
// the window: choose a file in the real file input and read the preview
const tmp = os.tmpdir() + "\\ia-backup-ui-test.json";
fs.writeFileSync(tmp, JSON.stringify({ format: "inbox-agent-backup", version: 1, rules: [{ id: "u", field: "domain", text: "ui-test.example", priority: 3 }, { id: "v", field: "subject", text: "ui invoice", priority: 4 }] }));
await evalJs("closeVeil(); openSettings(); 1"); await sleep(600);
const dom = await send("DOM.getDocument", { depth: 1 });
const q = await send("DOM.querySelector", { nodeId: dom.result.root.nodeId, selector: "#bk-file" });
check("desktop: Settings has a Backup section with a file chooser", q.result && q.result.nodeId > 0, q);
await send("DOM.setFileInputFiles", { nodeId: q.result.nodeId, files: [tmp] }); await sleep(900);
const panel = await evalJs("document.querySelector('#veil .panel') ? document.querySelector('#veil .panel').innerText : ''");
check("desktop: choosing a file shows a preview that names the new rules", /Restore from a backup/.test(panel) && /2 rules will be added/.test(panel) && /ui-test\.example/.test(panel), panel.slice(0, 300));
await evalJs("document.querySelector('[data-q=go]').click(); 1"); await sleep(1200);
const rulesNow = (await call("state", {})).ok.rules.map(x => x.text);
check("desktop: Restore in the window adds the rules and goes back to Settings", rulesNow.includes("ui-test.example") && rulesNow.includes("ui invoice") && (await evalJs("veil.dataset.kind")) === "settings", rulesNow);
const rows = await evalJs("document.querySelectorAll('#veil .rule').length");
check("desktop: the rule editor shows the restored rules", rows >= 3, rows);
try { fs.unlinkSync(tmp); } catch (e) { /* temp */ }
await evalJs("closeVeil(); 1");
ws.close();
process.exit(bad ? 1 : 0);
