// The Backup section of Settings and the restore dialog.
import assert from "node:assert/strict";
import { suite, wrapMock, unwrapMock, release, openSettings, chooseFile, backupText } from "./harness.mjs";

const FILE = backupText({
  rules: [{ field: "sender", text: "boss@corp.example", priority: 5 }],
  own_tasks: [{ title: "Renew the passport", priority: 3 }],
  mailboxes: [{ user: "new@box.example", host: "imap.box.example", port: 993 }],
  settings: { ai: { mode: "rules" } },
});
const state = page => page.run(() => ({ kind: veil.dataset.kind, h2: veil.querySelector("h2")?.textContent || "", rs: RS && { busy: RS.busy, withOptions: RS.withOptions, hasPreview: !!RS.preview, error: RS.error } }));
const pick = async (page, text = FILE, pad = 0) => { await openSettings(page); await page.run(chooseFile, "backup.json", text, pad) };
const pickAndWait = async page => { await pick(page); await page.waitFor(() => veil.querySelector("h2")?.textContent === "Restore from a backup" && RS && RS.preview && document.activeElement?.dataset?.q === "cancel") };
const calls = (page, cmd) => page.run(c => window.__calls.filter(x => x.cmd === c).map(x => x.a), cmd);

suite("the Backup section", it => {
  it("Save a backup file says where the file is and that nothing was left out", async page => {
    await openSettings(page);
    await page.click("[data-s=backup]");
    await page.waitFor(() => /^Saved /.test(document.querySelector("#bkres").textContent));
    const text = await page.run(() => document.querySelector("#bkres").textContent);
    assert.match(text, /^Saved C:\/Users\/you\/Downloads\/inbox-agent-backup-/);
    assert.doesNotMatch(text, /left out/);
  });

  it("when the file holds only so many of something, the message says what was left out", async page => {
    await openSettings(page);
    await page.run(wrapMock, [{ cmd: "backup_export", result: { path: "C:/x/backup.json", counts: { left_out: { corrections: 12, mail_state: 3 } } } }]);
    await page.click("[data-s=backup]");
    await page.waitFor(() => /left out/.test(document.querySelector("#bkres").textContent));
    const text = await page.run(() => document.querySelector("#bkres").textContent);
    assert.match(text, /left out: 12 corrections, 3 mail state\./);
    assert.match(text, /at most 5,000 sender choices, 2,000 corrections/);
  });

  it("a backup that fails to save says why and does not claim it saved", async page => {
    await openSettings(page);
    await page.run(wrapMock, [{ cmd: "backup_export", fail: "The Downloads folder cannot be written" }]);
    await page.click("[data-s=backup]");
    await page.waitFor(() => document.querySelector("#bkres .bad"));
    assert.equal(await page.run(() => document.querySelector("#bkres").textContent), "The Downloads folder cannot be written");
  });
});

suite("the restore dialog", it => {
  it("names what Restore will do before it does anything, and puts the keyboard on Cancel so Enter restores nothing", async page => {
    await page.run(wrapMock, []);
    await pickAndWait(page);
    const d = await page.run(() => ({ text: veil.textContent, go: veil.querySelector("[data-q=go]").disabled, boxes: veil.textContent.includes("Mailboxes to add") }));
    assert.match(d.text, /1 rule will be added/); assert.match(d.text, /1 task of yours will be added/); assert.match(d.text, /boss@corp\.example/);
    assert.equal(d.boxes, false, "mailboxes are listed only when the options box is ticked");
    assert.equal(d.go, false);
    await page.waitFor(() => document.activeElement?.dataset?.q === "cancel");
    await page.key("Enter"); // by reflex
    assert.equal((await state(page)).kind, "settings", "Enter pressed Cancel");
    assert.equal((await calls(page, "backup_import")).every(a => a.apply === false), true, "nothing was applied by looking");
    assert.equal(await page.run(() => M.rules.length), 1, "and nothing changed");
  });

  it("a preview that arrives late for an unticked box is not shown for the box as it is now", async page => {
    await page.run(wrapMock, [{ cmd: "backup_import", args: { settings: true }, hold: true }]);
    await pickAndWait(page);
    await page.click("[data-q=options]"); // ticked: its preview is held
    assert.equal((await state(page)).rs.withOptions, true);
    await page.click("[data-q=options]"); // unticked again before it comes
    await release(page); // the held answer for the ticked box arrives now
    const d = await page.run(() => ({ on: veil.querySelector("[data-q=options]").checked, text: veil.textContent, go: veil.querySelector("[data-q=go]").disabled }));
    assert.equal(d.on, false);
    assert.doesNotMatch(d.text, /Mailboxes to add/, "the answer for the ticked box was thrown away");
    assert.equal(d.go, false, "Restore works on the preview that is shown");
  });

  it("a preview that fails late for an unticked box does not put its error on the dialog as it is now", async page => {
    await page.run(wrapMock, [{ cmd: "backup_import", args: { settings: true }, hold: true, fail: "The options could not be read" }]);
    await pickAndWait(page);
    await page.click("[data-q=options]"); // ticked: its preview is held, and will fail
    await page.click("[data-q=options]"); // unticked again before the failure comes
    await release(page);
    const d = await page.run(() => ({ error: !!veil.querySelector("p.bad"), go: veil.querySelector("[data-q=go]").disabled, text: veil.textContent }));
    assert.equal(d.error, false, "the error belongs to a box that is no longer ticked");
    assert.equal(d.go, false); assert.match(d.text, /1 rule will be added/);
  });

  it("ticking the box adds the mailboxes and options to the preview, and the box keeps the keyboard", async page => {
    await page.run(wrapMock, []);
    await pickAndWait(page);
    await page.click("[data-q=options]");
    await page.waitFor(() => veil.textContent.includes("Mailboxes to add"));
    const d = await page.run(() => ({ list: veil.textContent.includes("new@box.example"), focus: document.activeElement?.dataset?.q }));
    assert.equal(d.list, true); assert.equal(d.focus, "options");
  });

  it("with the box ticked the preview lists each mailbox with its server, warns about it, and says a paid ranking choice is not restored", async page => {
    await page.run(wrapMock, []);
    await pick(page, backupText({ mailboxes: [{ user: "new@box.example", host: "imap.box.example", port: 993 }], settings: { ai: { mode: "anthropic" } } }));
    await page.waitFor(() => veil.querySelector("[data-q=options]") && RS && RS.preview);
    await page.click("[data-q=options]");
    await page.waitFor(() => veil.textContent.includes("Mailboxes to add"));
    const text = await page.run(() => veil.textContent);
    assert.match(text, /new@box\.example \(imap\.box\.example:993\)/);
    assert.match(text, /restore only a file you made yourself/);
    assert.match(text, /ranking with Claude and your own key\. That is not restored/);
  });

  it("while Restore is working Cancel and Escape do nothing; when it is done the dialog closes, says what came back, and the rule editor shows the restored rule", async page => {
    await page.run(wrapMock, [{ cmd: "backup_import", args: { apply: true }, hold: true }]);
    await pickAndWait(page);
    await page.click("[data-q=go]");
    const d = await page.run(() => ({ cancel: veil.querySelector("[data-q=cancel]").disabled, go: veil.querySelector("[data-q=go]").textContent, box: veil.querySelector("[data-q=options]").disabled }));
    assert.deepEqual(d, { cancel: true, go: "Restoring…", box: true });
    await page.key("Escape");
    assert.equal((await state(page)).kind, "restore", "Escape did not close it");
    await release(page); // the engine is done
    await page.waitFor(() => veil.dataset.kind === "settings" && veil.querySelector("h2").textContent === "Settings");
    assert.match(await page.run(() => document.querySelector("#toasts").textContent), /Restored: 1 rule, 1 task/);
    const rules = await page.run(() => [...document.querySelectorAll("#rules input[data-r=text]")].map(i => i.value));
    assert.deepEqual(rules.sort(), ["boss@corp.example", "digest.example"], "the editor reads the rules again");
    await page.click("[data-r=save]"); await page.settle(100);
    assert.equal(await page.run(() => M.rules.length), 2, "the next Save does not drop the restored rule");
    assert.equal((await state(page)).rs, null);
  });

  it("a restore that fails shows why, is not stuck busy, and leaves Cancel working", async page => {
    await page.run(wrapMock, [{ cmd: "backup_import", args: { apply: true }, fail: "The database is busy" }]);
    await pickAndWait(page);
    await page.click("[data-q=go]");
    await page.waitFor(() => veil.querySelector("p.bad"));
    const d = await page.run(() => ({ kind: veil.dataset.kind, cancel: veil.querySelector("[data-q=cancel]").disabled, error: veil.querySelector("p.bad").textContent, busy: RS.busy, go: veil.querySelector("[data-q=go]").disabled }));
    assert.deepEqual(d, { kind: "restore", cancel: false, error: "The database is busy", busy: false, go: true }, "the error stays until the file is chosen again");
    await page.click("[data-q=cancel]");
    assert.equal((await state(page)).kind, "settings");
  });

  it("rule changes that are not saved stop a restore from starting", async page => {
    await openSettings(page);
    await page.click("[data-r=add]"); // an unsaved rule
    await page.run(chooseFile, "backup.json", FILE);
    await page.waitFor(() => /not saved/.test(document.querySelector("#bkres")?.textContent || ""));
    const d = await state(page);
    assert.equal(d.kind, "settings"); assert.equal(d.rs, null);
  });

  it("Escape while the file is still being read leaves nothing to draw when the answer comes", async page => {
    await page.run(wrapMock, [{ cmd: "backup_import", hold: true }]);
    await pick(page);
    await page.waitFor(() => veil.dataset.kind === "restore" && window.__held.length === 1);
    await page.key("Escape");
    assert.equal((await state(page)).kind, "settings");
    await release(page); // the answer to the preview arrives
    const d = await state(page);
    assert.equal(d.kind, "settings"); assert.equal(d.h2, "Settings"); assert.equal(d.rs, null);
  });

  it("a file that is not one of this app's backups says so and cannot be restored", async page => {
    await pick(page, "this is not json");
    await page.waitFor(() => veil.querySelector("p.bad"));
    const d = await page.run(() => ({ error: veil.querySelector("p.bad").textContent, go: veil.querySelector("[data-q=go]").disabled }));
    assert.match(d.error, /not a backup made by Inbox Agent/); assert.equal(d.go, true);
  });

  it("a backup from a newer version of the app says to update", async page => {
    await pick(page, backupText({ version: 9 }));
    await page.waitFor(() => veil.querySelector("p.bad"));
    assert.match(await page.run(() => veil.querySelector("p.bad").textContent), /newer version of Inbox Agent/);
  });

  it("a file over 8 MB is refused before it is read", async page => {
    await pick(page, FILE, 8 * 1024 * 1024);
    await page.waitFor(() => /too large/.test(document.querySelector("#bkres")?.textContent || ""));
    const d = await state(page);
    assert.equal(d.kind, "settings"); assert.equal(d.rs, null);
  });

  it("a backup with nothing new in it says so and offers no Restore", async page => {
    await pick(page, backupText({}));
    await page.waitFor(() => veil.textContent.includes("Nothing in this file is new"));
    assert.equal(await page.run(() => veil.querySelector("[data-q=go]").disabled), true);
  });
});
