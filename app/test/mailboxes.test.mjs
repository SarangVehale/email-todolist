// Several mailboxes: the "Add a mailbox" dialog, the mailbox list in Settings, and the tags and chips on the list.
import assert from "node:assert/strict";
import { suite, wrapMock, unwrapMock, release, openSettings } from "./harness.mjs";

const kindAndTitle = page => page.run(() => ({ kind: veil.dataset.kind, shown: veil.classList.contains("show"), h2: veil.querySelector("h2")?.textContent || "", box: MB }));
const addDialog = async page => { await openSettings(page); await page.click("[data-s=addacct]"); await page.waitFor(() => document.activeElement.id === "m-user") }; // the dialog puts the keyboard in the address box a moment after it opens
const fillAndTest = async (page, user, pw) => {
  await page.fill("#m-user", user); await page.fill("#m-pw", pw);
  await page.click("#m-test"); await page.waitFor(() => document.querySelector("#mtestres .ok")); // the demo backend takes about a second
};

suite("the Add a mailbox dialog", it => {
  it("Cancel while the connection is being tested goes back to Settings and nothing is drawn when the answer comes", async page => {
    await addDialog(page);
    await page.run(wrapMock, [{ cmd: "test_account", hold: true, result: { ok: true, sent: "Sent" } }]);
    await page.fill("#m-user", "new@example.com"); await page.fill("#m-pw", "secret");
    await page.click("#m-test");
    assert.equal(await page.run(() => MB.testing), true, "the test is out");
    await page.click("[data-m=cancel]");
    await release(page); // the answer arrives
    const s = await kindAndTitle(page);
    assert.equal(s.kind, "settings"); assert.equal(s.h2, "Settings"); assert.equal(s.box, null);
  });

  it("a failed add keeps the dialog and says why; after a fix the same dialog adds the mailbox", async page => {
    await addDialog(page);
    await page.run(wrapMock, [{ cmd: "add_account", fail: "The server refused that password" }]);
    await fillAndTest(page, "new@example.com", "secret");
    await page.click("#m-add");
    await page.waitFor(() => document.querySelector("#mtestres .bad"));
    assert.equal(await page.run(() => document.querySelector("#mtestres .bad").textContent), "The server refused that password");
    const failed = await page.run(() => ({ add: document.querySelector("#m-add").disabled, busy: MB.busy, kind: veil.dataset.kind }));
    assert.deepEqual(failed, { add: true, busy: false, kind: "mailbox" }, "Add is off until it is tested again, and it is not stuck busy");
    await page.run(unwrapMock);
    await fillAndTest(page, "new@example.com", "second");
    await page.run(() => { undoStack.push(() => {}); bodies[7] = "text read before" }); // what was read or can be undone must not follow a mailbox number
    await page.click("#m-add");
    await page.waitFor(() => veil.dataset.kind === "settings" && document.querySelectorAll(".mbx").length === 3);
    assert.ok(await page.run(() => S.accounts.some(a => a.user === "new@example.com")));
    assert.deepEqual(await page.run(() => [undoStack.length, Object.keys(bodies).length]), [0, 0], "adding a mailbox forgets what was read and what could be undone");
  });

  it("Cancel while the mailbox is being added goes back to Settings, and the refusal that comes later draws nothing", async page => {
    await addDialog(page);
    await page.run(wrapMock, [{ cmd: "add_account", hold: true, fail: "The server refused that password" }]);
    await fillAndTest(page, "new@example.com", "secret");
    await page.click("#m-add");
    assert.equal(await page.run(() => MB.busy), true, "the add is out");
    await page.click("[data-m=cancel]");
    await release(page); // the refusal arrives
    const s = await kindAndTitle(page);
    assert.equal(s.kind, "settings"); assert.equal(s.h2, "Settings"); assert.equal(s.box, null);
  });

  it("a redraw keeps the keyboard in the box it was in", async page => {
    await addDialog(page);
    await page.run(() => document.querySelector("#m-provider").focus());
    await page.run(() => { const s = document.querySelector("#m-provider"); s.value = "other"; s.dispatchEvent(new Event("change", { bubbles: true })) });
    assert.equal(await page.run(() => document.activeElement.id), "m-provider");
    assert.ok(await page.run(() => !!document.querySelector("#m-host")), "the server box appeared");
  });

  it("Escape goes back to Settings, and Escape again closes it", async page => {
    await addDialog(page);
    await page.key("Escape");
    const back = await kindAndTitle(page);
    assert.equal(back.kind, "settings"); assert.equal(back.h2, "Settings");
    await page.key("Escape");
    const s = await kindAndTitle(page);
    assert.equal(s.shown, false);
  });

  it("adding a mailbox that is already there is refused with the engine's message and nothing changes", async page => {
    await addDialog(page);
    await fillAndTest(page, "you@example.com", "secret"); // already in the app: the demo backend says so, as the engine does
    await page.click("#m-add");
    await page.waitFor(() => document.querySelector("#mtestres .bad"));
    assert.match(await page.run(() => document.querySelector("#mtestres .bad").textContent), /already in the app/);
    assert.equal(await page.run(() => S.accounts.length), 2);
  });

  // "Other IMAP server" shows a Port box. An empty one reads as 0, which the engine refuses with a plain sentence instead of
  // trying to connect; the dialog shows that sentence as it is and keeps Add off.
  it("an empty Port box says the port isn't valid and a real port connects", async page => {
    await addDialog(page);
    await page.run(() => { const s = document.querySelector("#m-provider"); s.value = "other"; s.dispatchEvent(new Event("change", { bubbles: true })) });
    await page.waitFor(() => document.querySelector("#m-port"));
    await page.fill("#m-host", "mail.example.org"); await page.fill("#m-user", "new@example.org"); await page.fill("#m-pw", "secret");
    await page.fill("#m-port", "");
    await page.click("#m-test");
    await page.waitFor(() => document.querySelector("#mtestres .bad"));
    assert.equal(await page.run(() => document.querySelector("#mtestres .bad").textContent), "That mail server port isn't valid");
    assert.equal(await page.run(() => document.querySelector("#m-add").disabled), true, "Add stays off until a test passes");
    await page.fill("#m-port", "993");
    await page.click("#m-test"); await page.waitFor(() => document.querySelector("#mtestres .ok"));
    assert.equal(await page.run(() => document.querySelector("#m-add").disabled), false, "a real port passes and Add turns on");
  });

  it("a server name with a space in it is refused by the test, in the engine's words", async page => {
    await addDialog(page);
    await page.run(() => { const s = document.querySelector("#m-provider"); s.value = "other"; s.dispatchEvent(new Event("change", { bubbles: true })) });
    await page.waitFor(() => document.querySelector("#m-host"));
    await page.fill("#m-host", "mail example.org"); await page.fill("#m-user", "new@example.org"); await page.fill("#m-pw", "secret");
    await page.click("#m-test");
    await page.waitFor(() => document.querySelector("#mtestres .bad"));
    assert.equal(await page.run(() => document.querySelector("#mtestres .bad").textContent), "That mail server name isn't valid");
  });
});

suite("the mailboxes in Settings and on the list", it => {
  // a conversation can reach two of your mailboxes: it stays two tasks, and the cursor stays in the mailbox it was in
  // (the first mailbox's copy is the older, so it comes first in the list: equal priorities are listed oldest first)
  const twoCopies = () => {
    const mk = (uid, mb, hrs) => { const t = M.mk(uid, "Pat Lee", "pat@x.example", "Lunch", hrs, 5, "work", "Reply about lunch", false, false, "x", "r", { known: false, reply: false, bulk: false }); t.mailbox = mb; t.thread = "<lunch@x.example>"; return t };
    M.tasks.push(mk(50001, 0, 5), mk(400050001, 1, 2)); lastSnap = ""; return load();
  };

  it("the same message that reached two mailboxes is two rows, one in each", async page => {
    await page.run(twoCopies);
    const rows = await page.run(() => [...document.querySelectorAll("#view .row")].map(r => +r.dataset.uid).filter(u => u === 50001 || u === 400050001));
    assert.deepEqual(rows.sort((a, b) => a - b), [50001, 400050001]);
  });

  it("when a newer message arrives in one mailbox's copy of a conversation the cursor stays on that copy, not the other mailbox's", async page => {
    await page.run(twoCopies);
    await page.run(() => { cur = 400050001; render() });
    await page.run(() => { const t = M.mk(400050002, "Pat Lee", "pat@x.example", "Re: Lunch", 1, 5, "work", "Reply about lunch", false, false, "x", "r", { known: false, reply: false, bulk: false }); t.mailbox = 1; t.thread = "<lunch@x.example>"; M.tasks.push(t); lastSnap = ""; return load() });
    assert.equal(await page.run(() => cur), 400050002);
  });

  it("Remove asks twice, and then the mailbox and its tasks are gone and Settings lists one mailbox less", async page => {
    await openSettings(page);
    await page.run(() => { undoStack.push(() => {}); bodies[400000015] = "text read before" });
    await page.click("[data-s=rmacct][data-id='1']");
    assert.match(await page.run(() => document.querySelector("[data-s=rmacct][data-id='1']").textContent), /^Click again/);
    assert.equal(await page.run(() => M.accounts.length), 2, "one click removed nothing");
    await page.click("[data-s=rmacct][data-id='1']");
    await page.waitFor(() => M.accounts.length === 1 && document.querySelectorAll(".mbx").length === 1);
    assert.equal(await page.run(() => document.querySelectorAll("[data-s=rmacct]").length), 0, "the last mailbox cannot be removed here");
    await page.waitFor(() => !S.tasks.some(t => t.mailbox === 1));
    assert.deepEqual(await page.run(() => [undoStack.length, Object.keys(bodies).length]), [0, 0], "removing a mailbox forgets what was read and what could be undone");
  });

  it("signing out forgets what was read and what could be undone", async page => {
    await openSettings(page);
    await page.run(() => { undoStack.push(() => {}); bodies[400000015] = "text read before" });
    await page.click("[data-s=signout]"); await page.click("[data-s=signout]");
    await page.waitFor(() => undoStack.length === 0); // then the setup wizard opens for the empty app
    assert.equal(await page.run(() => Object.keys(bodies).length), 0);
  });

  it("removing the mailbox the list is filtered to clears the filter instead of showing an empty list", async page => {
    await page.click("[data-flt='acct:1']");
    assert.equal(await page.run(() => flt.acct), "1");
    await openSettings(page);
    await page.click("[data-s=rmacct][data-id='1']"); await page.click("[data-s=rmacct][data-id='1']");
    await page.waitFor(() => M.accounts.length === 1);
    await page.click("[data-vc]");
    await page.waitFor(() => flt.acct === "");
    assert.ok(await page.run(() => document.querySelectorAll("#view .row").length > 0), "the list is not empty");
  });

  it("with two mailboxes every task of a mailbox has its tag and a chip filters to it; with one there are neither", async page => {
    const info = await page.run(() => ({
      chips: [...document.querySelectorAll("[data-flt^='acct:']")].map(c => c.dataset.flt),
      tagged: [...document.querySelectorAll("#view .row .tg")].map(t => t.title),
      own: [...document.querySelectorAll("#view .row")].filter(r => find(+r.dataset.uid).mailbox == null).map(r => [...r.querySelectorAll(".tg")].filter(g => S.accounts.some(a => a.user === g.title)).length),
    }));
    assert.deepEqual(info.chips, ["acct:0", "acct:1"]);
    assert.ok(info.tagged.includes("you@example.com") && info.tagged.includes("sam@work.example"));
    assert.ok(info.own.length > 0 && info.own.every(n => n === 0), "a task you added belongs to no mailbox and has no mailbox tag");
    await page.click("[data-flt='acct:1']");
    const only = await page.run(() => [...document.querySelectorAll("#view .row")].map(r => Math.floor(+r.dataset.uid / 400000000)));
    assert.ok(only.length > 0 && only.every(n => n === 1), "only the second mailbox's rows are listed");
    await page.click("[data-flt='acct:1']"); // the chip again: back to everything
    assert.ok(await page.run(() => document.querySelectorAll("#view .row").length > 3));
    // down to one mailbox: no chips, no tags
    await openSettings(page);
    await page.click("[data-s=rmacct][data-id='1']"); await page.click("[data-s=rmacct][data-id='1']");
    await page.waitFor(() => M.accounts.length === 1);
    await page.click("[data-vc]");
    await page.waitFor(() => document.querySelectorAll("[data-flt^='acct:']").length === 0);
    assert.equal(await page.run(() => [...document.querySelectorAll("#view .row .tg")].filter(g => g.title.includes("@")).length), 0);
  });
});
