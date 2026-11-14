// The long list: how many rows are drawn, what "Show more" does, and the cursor.
import assert from "node:assert/strict";
import { suite, addTasks } from "./harness.mjs";

const rows = page => page.run(() => document.querySelectorAll("#view .row").length);

suite("the list of a big mailbox", it => {
  it("draws 150 rows at first, with Show more and Show all under them", async page => {
    await page.run(addTasks, { open: 3000 });
    assert.equal(await rows(page), 150);
    const buttons = await page.run(() => [...document.querySelectorAll("[data-act=more],[data-act=moreall]")].map(b => b.textContent));
    assert.match(buttons[0], /^Show 150 more$/);
    assert.match(buttons[1], /^Show all \d+ more$/);
  });

  it("Show 150 more adds 150, Show all draws everything", async page => {
    await page.run(addTasks, { open: 600 });
    await page.click("[data-act=more]");
    assert.equal(await rows(page), 300);
    await page.click("[data-act=moreall]");
    assert.equal(await page.run(() => document.querySelectorAll("#view .row").length === listFor().length), true, "every row of the list is drawn");
    assert.ok(await rows(page) >= 600);
    assert.equal(await page.run(() => document.querySelectorAll("[data-act=more]").length), 0);
  });

  it("holding j past the last drawn row draws more and keeps the cursor on a drawn row", async page => {
    await page.run(addTasks, { open: 600 });
    await page.keys("j", 200);
    const state = await page.run(() => ({ rows: document.querySelectorAll("#view .row").length, cursorRows: document.querySelectorAll(".row.cur").length, onScreen: (() => { const r = document.querySelector(".row.cur").getBoundingClientRect(); return r.bottom > 0 && r.top < innerHeight })() }));
    assert.equal(state.cursorRows, 1);
    assert.ok(state.rows > 150, `more rows were drawn (${state.rows})`);
    assert.ok(state.onScreen, "the cursor row is on screen");
  });

  it("a cursor key moves the cursor by changing classes, without drawing the page again", async page => {
    await page.run(addTasks, { open: 600 });
    await page.run(() => { window.__row = document.querySelectorAll("#view .row")[3]; window.__view = document.getElementById("view") });
    const before = await page.run(() => document.querySelector(".row.cur").dataset.uid);
    await page.key("j");
    const after = await page.run(() => ({ uid: document.querySelector(".row.cur").dataset.uid, sameRow: document.querySelectorAll("#view .row")[3] === window.__row, count: document.querySelectorAll(".row.cur").length }));
    assert.notEqual(after.uid, before);
    assert.equal(after.count, 1);
    assert.ok(after.sameRow, "the rows are the same elements: nothing was redrawn");
  });

  it("another tab, another sort order or a search starts the list again at 150 rows", async page => {
    await page.run(addTasks, { open: 600 });
    await page.click("[data-act=moreall]");
    assert.ok(await rows(page) > 150);
    await page.click("[data-view=done]");
    await page.click("[data-view=todo]");
    assert.equal(await rows(page), 150, "another tab");
    await page.click("[data-act=moreall]");
    assert.ok(await rows(page) > 150);
    await page.run(() => { const s = document.querySelector("#sort"); s.value = "newest"; s.dispatchEvent(new Event("change", { bubbles: true })) });
    assert.equal(await rows(page), 150, "another sort order");
    await page.click("[data-act=moreall]");
    assert.ok(await rows(page) > 150);
    await page.fill("#q", "subject"); // matches the 600 added tasks: still a long list
    await page.settle(150); // the search has drawn
    assert.equal(await rows(page), 150, "a search");
  });

  it("only the open row has its detail drawn", async page => {
    await page.run(addTasks, { open: 100 });
    await page.click("#view .row .mid", { nth: 4 });
    const open = await page.run(() => ({ open: document.querySelectorAll(".row.open").length, withDetail: [...document.querySelectorAll("#view .row .more")].filter(m => m.innerHTML.length > 0).length, excerpt: !!document.querySelector(".row.open .more p") }));
    assert.deepEqual(open, { open: 1, withDetail: 1, excerpt: true });
    await page.click(".row.open .mid");
    assert.equal(await page.run(() => [...document.querySelectorAll("#view .row .more")].filter(m => m.innerHTML.length > 0).length), 0);
  });
});
