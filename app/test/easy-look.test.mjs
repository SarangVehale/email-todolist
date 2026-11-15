// The easy view (design: docs/designs/phase12.md): big Done and Later on each message, plain headings, an Undo that stays, a card that says how it works,
// text size and light colours per device, and Settings with five things in it. The dense view must stay as it was for an install already in use.
import assert from "node:assert/strict";
import { suite, openSettings } from "./harness.mjs";

/** The look is kept in the browser's own storage, which pages of one test file share: every test starts from nothing. */
const start = async (page, { easy = false, size, theme } = {}) => {
  await page.run((e, s, t) => {
    localStorage.clear();
    if (e) localStorage.setItem("ia.easy", "1");
    if (s) localStorage.setItem("ia.size", s);
    if (t) localStorage.setItem("ia.theme", t);
    applyLook(); lastSnap = ""; render();
  }, easy, size, theme);
  await page.settle(100);
};
const look = page => page.run(() => ({ easy: document.documentElement.classList.contains("easy"), light: document.documentElement.classList.contains("light"), zoom: document.documentElement.style.zoom, view }));
const visible = (page, sel) => page.run(s => [...document.querySelectorAll(s)].some(e => getComputedStyle(e).display !== "none" && e.getBoundingClientRect().width > 0), sel);

suite("the dense view stays as it was", it => {
  it("an install in use has no easy view, no zoom and the dark colours until its owner turns them on", async page => {
    await start(page);
    assert.deepEqual(await look(page), { easy: false, light: false, zoom: "1", view: "todo" });
    assert.equal(await visible(page, ".row .chk"), true, "the circle to finish a task is there");
    assert.equal(await visible(page, "#rail"), true);
    assert.equal(await page.run(() => document.querySelector("#bar nav").textContent.replace(/\d+/g, "").trim()), "To-doStarredSnoozedDoneLog");
    assert.equal(await page.run(() => !!document.querySelector(".welcomecard, .ea, .big")), false);
  });

  it("Settings offers the look to everyone, and turning the simple view on changes the page at once", async page => {
    await start(page);
    await openSettings(page);
    assert.equal(await page.run(() => document.querySelector("[data-s=easy]").checked), false);
    assert.deepEqual(await page.run(() => [...document.querySelectorAll("[data-s=size]")].map(b => [b.textContent, b.getAttribute("aria-pressed")])), [["Normal", "true"], ["Large", "false"], ["Huge", "false"]]);
    await page.click("[data-s=easy]");
    await page.waitFor(() => document.documentElement.classList.contains("easy"));
    assert.deepEqual(await look(page), { easy: true, light: true, zoom: "1.2", view: "todo" }, "the simple view brings large text and light colours");
    assert.equal(await page.run(() => localStorage.getItem("ia.easy")), "1");
    await page.waitFor(() => getComputedStyle(document.body).backgroundColor === "rgb(255, 255, 255)"); // the browser may take a frame to show it
    assert.equal(await page.run(() => getComputedStyle(document.body).backgroundColor), "rgb(255, 255, 255)", "the page is light");
  });
});

suite("the easy list", it => {
  it("says it in plain words: one heading, a sentence for the count, headings a person understands, and nothing she did not ask for", async page => {
    await start(page, { easy: true });
    const w = await page.run(() => ({
      h1: document.querySelector("h1").textContent, tabs: [...document.querySelectorAll("#bar nav button")].map(b => b.textContent.replace(/\d+/g, "").trim()),
      secs: [...document.querySelectorAll(".sec span")].map(s => s.textContent), lede: document.querySelector(".sub2.ez")?.textContent,
      kick: document.querySelector(".kick").textContent,
    }));
    assert.equal(w.h1, "Your list");
    assert.deepEqual(w.tabs, ["Your list", "Later", "Done"], "no Starred and no Log");
    assert.ok(w.secs.length >= 2 && w.secs.every(s => ["Needs you now", "Needs you today", "Can wait", "Not important"].includes(s)), w.secs.join("|"));
    assert.match(w.lede, /^(\d+ messages? needs? you\.|Nothing is urgent\. The rest can wait\.)$/);
    assert.match(w.kick, /^[A-Z][a-z]+day\b/); assert.doesNotMatch(w.kick, /Inbox/, "the date in full, with no 'Inbox' beside it");
    for (const sel of [".row .chk", ".row .star", ".row .pri", "#rail", "#foot .keys", ".stats", ".fbar", "[data-act=selectmode]"]) assert.equal(await visible(page, sel), false, sel + " is hidden");
    for (const word of ["P5", "// ", "To-do"]) assert.equal(await page.run(x => document.querySelector("#view").textContent.includes(x), word), false, "no " + JSON.stringify(word) + " on the page");
  });

  it("every message is a card with a big Done and a big Later, each named by what it is for", async page => {
    await start(page, { easy: true });
    const cards = await page.run(() => [...document.querySelectorAll("#view .row")].map(r => ({ t: r.querySelector(".t").textContent, s: r.querySelector(".s").textContent, b: [...r.querySelectorAll(".ea button")].map(x => [x.textContent, x.getAttribute("aria-label"), Math.round(x.getBoundingClientRect().height)]) })));
    assert.ok(cards.length > 3);
    for (const c of cards) {
      assert.deepEqual(c.b.map(x => x[0]), ["Done", "Later"]);
      assert.equal(c.b[0][1], "Done: " + c.t); assert.equal(c.b[1][1], "Later: " + c.t);
      assert.ok(c.b.every(x => x[2] >= 44), "the buttons are at least 44 pixels high: " + JSON.stringify(c.b));
      assert.match(c.s, / ago$| yesterday$/, "when, in words: " + c.s);
    }
  });

  it("Done hides the message, and Undo stays until the next thing is done", async page => {
    await start(page, { easy: true });
    const first = await page.run(() => document.querySelector("#view .row").dataset.uid);
    await page.click(`.row[data-uid="${first}"] [data-act=done]`);
    await page.waitFor(u => !document.querySelector(`.row[data-uid="${u}"]`), first);
    assert.match(await page.run(() => document.querySelector("#undoMsg").textContent), /^Done/);
    assert.equal(await page.run(() => getComputedStyle(document.querySelector("#undo")).display), "flex");
    await page.settle(5600); // the dense view hides it after 5 seconds
    assert.equal(await page.run(() => getComputedStyle(document.querySelector("#undo")).display), "flex", "still there for a person who was not quick");
    await page.click("#undoBtn");
    await page.waitFor(u => document.querySelector(`.row[data-uid="${u}"]`), first);
  });

  it("Later puts the message away until tomorrow; it is in the Later tab and Undo brings it back", async page => {
    await start(page, { easy: true });
    const first = await page.run(() => +document.querySelector("#view .row").dataset.uid);
    await page.click(`.row[data-uid="${first}"] [data-act=later]`);
    await page.waitFor(u => !document.querySelector(`.row[data-uid="${u}"]`), first);
    assert.equal(await page.run(() => document.querySelector("#undoMsg").textContent), "Put away until tomorrow");
    assert.ok(await page.run(u => find(u).snooze_until > Date.now() / 1000 + 12 * 3600, first), "snoozed to tomorrow, not for a few hours");
    await page.click("[data-view=snoozed]");
    await page.waitFor(u => document.querySelector(`.row[data-uid="${u}"]`), first);
    assert.equal(await page.run(() => document.querySelector("h1").textContent), "Put away until later");
    assert.deepEqual(await page.run(u => [...document.querySelectorAll(`.row[data-uid="${u}"] .ea button`)].map(b => b.textContent), first), ["Done", "Bring back now"], "a message that is put away can be done or brought back");
    await page.click("#undoBtn");
    await page.waitFor(u => !find(u).snooze_until, first);
  });

  it("a message that was put away can be brought back at once, and says so", async page => {
    await start(page, { easy: true });
    const first = await page.run(() => +document.querySelector("#view .row").dataset.uid);
    await page.click(`.row[data-uid="${first}"] [data-act=later]`);
    await page.waitFor(u => !document.querySelector(`.row[data-uid="${u}"]`), first);
    await page.click("[data-view=snoozed]");
    await page.waitFor(u => document.querySelector(`.row[data-uid="${u}"] [data-act=unlater]`), first);
    assert.equal(await page.run(u => document.querySelector(`.row[data-uid="${u}"] [data-act=unlater]`).getAttribute("aria-label"), first).then(x => x.startsWith("Bring back now: ")), true);
    await page.click(`.row[data-uid="${first}"] [data-act=unlater]`);
    await page.waitFor(u => !find(u).snooze_until, first);
    await page.click("[data-view=todo]");
    await page.waitFor(u => document.querySelector(`.row[data-uid="${u}"]`), first);
  });

  it("the time of the last read follows the engine, so a quiet mailbox does not look stuck", async page => {
    await start(page, { easy: true });
    await page.run(() => load()); // the window has now seen the engine's answer once: a later change has to be noticed against that
    const before = await page.run(() => document.querySelector("#foot span").textContent);
    await page.run(() => { M.checkedAt += 3 * 3600 + 60 * 7 });
    await page.run(() => load());
    await page.waitFor(b => document.querySelector("#foot span").textContent !== b, before);
    assert.match(await page.run(() => document.querySelector("#foot span").textContent), /^Watching your mailbox\. Last read at /);
  });

  it("the Done tab says Put back and does", async page => {
    await start(page, { easy: true });
    await page.click("[data-view=done]");
    await page.waitFor(() => document.querySelector("#view .row"));
    const name = await page.run(() => document.querySelector("#view .row .ea button").textContent);
    assert.equal(name, "Put back");
    const uid = await page.run(() => +document.querySelector("#view .row").dataset.uid);
    await page.click(`.row[data-uid="${uid}"] [data-act=done]`);
    await page.waitFor(u => find(u).status !== "done", uid);
  });

  it("a card at the top says how it works until it is dismissed once, and stays dismissed", async page => {
    await start(page, { easy: true });
    const card = await page.run(() => document.querySelector(".welcomecard")?.textContent || "");
    assert.match(card, /never sends anything/); assert.match(card, /Done.*hides a message/); assert.match(card, /Later.*until tomorrow/); assert.match(card, /Undo/);
    await page.click("[data-act=welcomed]");
    assert.equal(await page.run(() => !!document.querySelector(".welcomecard")), false);
    await page.run(() => { lastSnap = ""; render() });
    assert.equal(await page.run(() => !!document.querySelector(".welcomecard")), false, "still gone after the list is drawn again");
    assert.equal(await page.run(() => localStorage.getItem("ia.welcomed")), "1");
  });

  it("an opened message shows who, why, the start of it, and buttons to read it all and to open it in Gmail", async page => {
    await start(page, { easy: true });
    const uid = await page.run(() => { const t = S.tasks.find(x => x.status !== "done" && x.uid < 2000000000); t.message_id = "<abc.123@mail.example>"; render(); return t.uid });
    await page.click(`.row[data-uid="${uid}"] .mid`);
    await page.waitFor(u => document.querySelector(`.row[data-uid="${u}"] .more .spec`), uid);
    const more = await page.run(u => document.querySelector(`.row[data-uid="${u}"] .more`).textContent, uid);
    assert.match(more, /From/); assert.match(more, /Why it is here/);
    for (const word of ["Signals", "Priority", "Sender", "Always important", "Mute", "Rewrite", "Draft a reply", "Snooze"]) assert.equal(more.includes(word), false, "no " + word);
    assert.match(more, /Read message/); assert.match(more, /Open in Gmail/);
    await page.click(`.row[data-uid="${uid}"] [data-act=open-gmail]`);
    await page.waitFor(() => (window.__opened || []).length === 1);
    const opened = await page.run(() => window.__opened);
    assert.equal(opened.length, 1);
    assert.match(opened[0], /^https:\/\/mail\.google\.com\/mail\/u\/[^/]+\/#search\/rfc822msgid%3Aabc\.123%40mail\.example$/, "the signed-in account, then a search for the message's own id");
  });

  it("the status line says when the mailbox was last read, in words and without key caps", async page => {
    await start(page, { easy: true });
    assert.match(await page.run(() => document.querySelector("#foot span").textContent), /^Watching your mailbox\. Last read at \d{1,2}:\d{2}( ?[AP]M)?\.$/);
    await page.run(() => { S.checked_at = 0; render() });
    assert.equal(await page.run(() => document.querySelector("#foot span").textContent), "Watching your mailbox.");
  });

  it("while the first mail is being read it says that the first time takes a few minutes", async page => {
    await start(page, { easy: true });
    await page.run(() => { S.tasks = []; S.status = "syncing"; render() });
    assert.match(await page.run(() => document.querySelector(".empty").textContent), /Reading your latest mail… ?The first time it can take a few minutes\. Your list fills in by itself\./);
  });

  it("Settings in the simple view has the look, the mailboxes, alerts and More options, and nothing about ranking models or the phone", async page => {
    await start(page, { easy: true });
    await openSettings(page);
    const heads = await page.run(() => [...veil.querySelectorAll(".sect")].map(s => s.textContent));
    assert.deepEqual(heads, ["How it looks", "Your mailboxes", "Alerts"]);
    assert.equal(await page.run(() => !!veil.querySelector("[data-s=aimode], [data-s=hub], [data-s=writeback], [data-s=signout], [data-r=add], [data-s=backup]")), false, "the rest is under More options");
    assert.match(await page.run(() => veil.querySelector(".mbx label").textContent), /Paste a new code if the old one stopped working/);
    assert.match(await page.run(() => veil.querySelector("[data-s=savepw]").textContent), /Save code/);
    await page.click("[data-s=moresettings]");
    await page.waitFor(() => veil.querySelector("[data-s=aimode]"));
    assert.equal(await page.run(() => veil.querySelector("[data-s=moresettings]").textContent), "Back to simple settings");
    assert.ok(await page.run(() => !!veil.querySelector("[data-s=signout]")), "everything is there");
    await page.click("[data-s=moresettings]");
    await page.waitFor(() => !veil.querySelector("[data-s=aimode]"));
  });

  it("text size: Normal, Large and Huge, kept for this device, and the simple view starts at Large", async page => {
    await start(page, { easy: true });
    assert.equal((await look(page)).zoom, "1.2");
    await openSettings(page);
    assert.deepEqual(await page.run(() => [...document.querySelectorAll("[data-s=size]")].map(b => b.getAttribute("aria-pressed"))), ["false", "true", "false"], "Large is the one pressed");
    await page.click("[data-size=huge]");
    await page.waitFor(() => document.documentElement.style.zoom === "1.45");
    assert.equal(await page.run(() => localStorage.getItem("ia.size")), "huge");
    await page.click("[data-size=normal]");
    await page.waitFor(() => document.documentElement.style.zoom === "1");
    await page.run(() => { applyLook() }); // a reload reads the choice again
    assert.equal((await look(page)).zoom, "1");
  });

  it("light colours are a switch of their own, in either view", async page => {
    await start(page, { easy: true });
    await openSettings(page);
    assert.equal(await page.run(() => document.querySelector("[data-s=light]").checked), true);
    await page.click("[data-s=light]");
    await page.waitFor(() => !document.documentElement.classList.contains("light"));
    await page.waitFor(() => getComputedStyle(document.body).backgroundColor === "rgb(0, 0, 0)"); // the browser may take a frame to show it
    assert.equal(await page.run(() => getComputedStyle(document.body).backgroundColor), "rgb(0, 0, 0)", "light text on black again");
    assert.equal(await page.run(() => localStorage.getItem("ia.theme")), "dark");
  });

  it("turning the simple view off shows everything again, and on a tab the simple view lacks it goes back to the list", async page => {
    await start(page, { easy: true });
    await page.run(() => { view = "log" });
    await page.run(() => applyLook());
    assert.equal((await look(page)).view, "todo", "the easy view has no Log tab");
    await openSettings(page);
    await page.click("[data-s=easy]");
    await page.waitFor(() => !document.documentElement.classList.contains("easy"));
    assert.equal(await visible(page, ".row .chk"), true);
    assert.deepEqual(await page.run(() => [...veil.querySelectorAll(".sect")].map(s => s.textContent).slice(0, 2)), ["How it looks", "Mail"], "Settings is the whole page again");
  });
});
