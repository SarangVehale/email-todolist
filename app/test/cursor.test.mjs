// The cursor and the parts of the list: a row with the keyboard focus, folds, undo and select mode.
import assert from "node:assert/strict";
import { suite, addTasks } from "./harness.mjs";

const drawn = (page, from, to) => page.run((a, b) => [...document.querySelectorAll("#view .row")].filter(r => +r.dataset.uid >= a && +r.dataset.uid < b).length, from, to);

suite("the cursor", it => {
  it("a row that has the keyboard focus does not take the keys back: j keeps moving and x finishes the highlighted row", async page => {
    await page.run(addTasks, { open: 40 });
    await page.run(() => document.querySelectorAll("#view .row .mid")[3].focus()); // what Tab does
    await page.keys("j", 3);
    const state = await page.run(() => {
      const rows = [...document.querySelectorAll("#view .row")], cur = document.querySelector(".row.cur");
      return { index: rows.indexOf(cur), uid: +cur.dataset.uid, focusInARow: !!document.activeElement.closest?.(".row") };
    });
    assert.equal(state.index, 6, "three presses from row 3 reach row 6 (the focused row must not pull the cursor back)");
    assert.equal(state.focusInARow, false);
    await page.key("x");
    await page.waitFor(uid => find(uid).status === "done", state.uid); // after the finishing animation
    assert.equal(await page.run(uid => find(uid).status, state.uid), "done", "x finished the highlighted row");
  });

  it("k on the first row and j on the last row bring the cursor back into view", async page => {
    await page.run(addTasks, { open: 40 });
    const visible = () => page.run(() => { const r = document.querySelector(".row.cur").getBoundingClientRect(); return r.top >= -2 && r.bottom <= innerHeight + 2 });
    await page.run(() => { document.querySelector("main")?.scrollTo?.(0, 99999); window.scrollTo(0, 99999) });
    assert.equal(await visible(), false, "the first row is out of sight before the key");
    await page.key("k"); // the cursor is on the first row
    assert.ok(await visible(), "k at the top");
    await page.run(() => { cur = lastList[lastList.length - 1].uid; render(); window.scrollTo(0, 0) });
    assert.equal(await visible(), false, "the last row is out of sight before the key");
    await page.key("j");
    assert.ok(await visible(), "j at the bottom");
  });

  it("the rail marks the row the cursor is on", async page => {
    await page.run(addTasks, { open: 40 });
    await page.keys("j", 3);
    const marks = await page.run(() => ({ rail: document.querySelector("#rail .rc.cur")?.dataset.rail, row: document.querySelector(".row.cur")?.dataset.uid, marked: document.querySelectorAll("#rail .rc.cur").length }));
    assert.ok(marks.row, "there is a cursor row"); assert.equal(marks.rail, marks.row); assert.equal(marks.marked, 1);
  });
});

suite("the parts of the list (automated mail)", it => {
  it("Show 150 more under an open fold draws more of the fold and none of the list above it", async page => {
    await page.run(addTasks, { open: 405, auto: 600 });
    await page.click("[data-act=autofold]");
    const before = { main: await drawn(page, 50000, 50405), auto: await drawn(page, 60000, 60600) };
    assert.equal(before.auto, 150);
    await page.click("[data-act=more][data-part=auto]");
    const after = { main: await drawn(page, 50000, 50405), auto: await drawn(page, 60000, 60600) };
    assert.equal(after.auto, 300);
    assert.equal(after.main, before.main, "the list above the fold stays as it was");
  });

  it("opening a row inside a fold does not draw the whole list above it or throw the page away from the row", async page => {
    await page.run(addTasks, { open: 405, auto: 600 });
    await page.click("[data-act=autofold]");
    const mainBefore = await drawn(page, 50000, 50405);
    await page.click("#view .row[data-uid^='60'] .mid", { nth: 40 });
    assert.equal(await drawn(page, 50000, 50405), mainBefore, "no more of the main list was drawn");
    const onScreen = await page.run(() => { const r = document.querySelector(".row.open").getBoundingClientRect(); return r.top >= 0 && r.top < innerHeight });
    assert.ok(onScreen, "the opened row is still on screen");
  });

  it("a cursor far inside a fold raises only the fold's count", async page => {
    await page.run(addTasks, { open: 405, auto: 600 });
    await page.click("[data-act=autofold]");
    const mainBefore = await drawn(page, 50000, 50405);
    await page.run(() => { cur = parts().auto[300].uid; render() });
    assert.ok(await drawn(page, 60000, 60600) > 300);
    assert.equal(await drawn(page, 50000, 50405), mainBefore);
  });
});

suite("finishing, undoing and selecting", it => {
  it("x finishes the highlighted task and z brings it back", async page => {
    await page.run(addTasks, { open: 30 });
    await page.key("j");
    const uid = await page.run(() => +document.querySelector(".row.cur").dataset.uid);
    await page.key("x");
    await page.waitFor(u => find(u).status === "done", uid); // after the finishing animation
    await page.key("z");
    await page.waitFor(u => find(u).status === "open", uid);
  });

  it("undo of 'Done all automated' reopens every task, even those the window no longer holds after a poll", async page => {
    await page.run(addTasks, { open: 20, auto: 300, done: 100 });
    await page.click("[data-autodone]"); await page.click("[data-autodone]"); // the first click asks "Click again"
    await page.settle(250);
    assert.equal(await page.run(() => M.tasks.filter(t => t.uid >= 60000 && t.uid < 60300 && t.status === "done").length), 300);
    // the next poll: the window now holds only the newest 200 finished tasks
    await page.run(() => { lastSnap = ""; return load() });
    assert.ok(await page.run(() => S.tasks.filter(t => t.uid >= 60000 && t.uid < 60300).length) < 300, "the poll dropped some of them from the window");
    await page.key("z"); await page.settle(500);
    assert.equal(await page.run(() => M.tasks.filter(t => t.uid >= 60000 && t.uid < 60300 && t.status === "open").length), 300);
  });

  it("undo of 'Done all' for mail that left your inbox reopens every task too", async page => {
    await page.run(addTasks, { open: 20, gone: 300, done: 100 });
    await page.click("[data-gonedone]"); await page.click("[data-gonedone]");
    await page.settle(250);
    assert.equal(await page.run(() => M.tasks.filter(t => t.uid >= 80000 && t.uid < 80300 && t.status === "done").length), 300);
    await page.run(() => { lastSnap = ""; return load() });
    assert.ok(await page.run(() => S.tasks.filter(t => t.uid >= 80000 && t.uid < 80300).length) < 300, "the poll dropped some of them from the window");
    await page.key("z"); await page.settle(500);
    assert.equal(await page.run(() => M.tasks.filter(t => t.uid >= 80000 && t.uid < 80300 && t.status === "open").length), 300);
  });

  it("select mode: All selects the whole list and Done finishes all of it, and one Undo takes them back even after a poll", async page => {
    await page.run(addTasks, { open: 400 });
    await page.click("[data-act=selectmode]");
    await page.click("[data-sel=all]");
    const n = await page.run(() => ({ chosen: sel.size, list: listFor().length, drawn: document.querySelectorAll("#view .row").length }));
    assert.equal(n.chosen, n.list);
    assert.ok(n.drawn < n.list, "more rows are selected than are drawn");
    await page.click("[data-sel=done]"); await page.settle(300);
    assert.equal(await page.run(() => M.tasks.filter(t => t.uid >= 50000 && t.uid < 50400 && t.status === "done").length), 400);
    await page.run(() => { lastSnap = ""; return load() }); // the next poll: the window holds only the newest 200 finished tasks
    assert.ok(await page.run(() => S.tasks.filter(t => t.uid >= 50000 && t.uid < 50400).length) < 400, "the poll dropped some of them from the window");
    await page.key("z"); await page.settle(500);
    assert.equal(await page.run(() => M.tasks.filter(t => t.uid >= 50000 && t.uid < 50400 && t.status === "open").length), 400);
  });

  it("undo of 'Done all N' for a filtered list reopens every task, even those the window no longer holds after a poll", async page => {
    await page.run(addTasks, { open: 1500 }); // 250 of them in each of six categories
    await page.click("[data-flt='cat:work']");
    await page.click("[data-bulk]"); await page.click("[data-bulk]"); // the first click asks "Click again"
    await page.settle(300);
    const done = () => page.run(() => M.tasks.filter(t => t.category === "work" && t.uid >= 50000 && t.uid < 51500 && t.status === "done").length);
    assert.ok(await done() >= 250);
    const all = await done();
    await page.run(() => { lastSnap = ""; return load() });
    assert.ok(await page.run(() => S.tasks.filter(t => t.category === "work" && t.uid >= 50000 && t.uid < 51500).length) < all, "the poll dropped some of them from the window");
    await page.key("z"); await page.settle(700);
    assert.equal(await page.run(() => M.tasks.filter(t => t.category === "work" && t.uid >= 50000 && t.uid < 51500 && t.status === "open").length), all);
  });

  it("shift-click selects the drawn rows between two clicks and none that are not drawn", async page => {
    await page.run(addTasks, { open: 405, auto: 600 });
    await page.click("[data-act=autofold]");
    await page.click("[data-act=selectmode]");
    await page.click("#view .row .chk", { nth: 9 });
    await page.click("#view .row[data-uid^='60'] .chk", { nth: 5, shift: true });
    const r = await page.run(() => ({ chosen: sel.size, allDrawn: [...sel].every(u => document.querySelector(`#view .row[data-uid="${u}"]`)), drawn: document.querySelectorAll("#view .row").length }));
    assert.ok(r.allDrawn, "every selected row is drawn");
    assert.ok(r.chosen > 100 && r.chosen < r.drawn);
  });

  it("the Done tab shows the newest finished tasks, says how many there are, and can show older ones", async page => {
    await page.run(addTasks, { open: 5, done: 1000 });
    const total = await page.run(() => M.tasks.filter(t => t.status === "done").length); // the demo has finished one of its own
    await page.click("[data-view=done]");
    const first = await page.run(() => ({ note: document.querySelector(".sub2")?.textContent, label: document.querySelector("[data-view=done]").innerText.replace(/\s+/g, " "), loaded: S.tasks.filter(t => t.status === "done").length, older: document.querySelector("[data-act=olderdone]")?.textContent }));
    assert.match(first.note, new RegExp("The 200 most recently finished of " + total + " finished tasks"));
    assert.match(first.label, new RegExp(String(total)));
    assert.equal(first.loaded, 200);
    assert.equal(first.older, "Show 500 older");
    const newest = await page.run(() => ({ newest: S.tasks.some(t => t.uid === 70999), oldest: S.tasks.some(t => t.uid === 70000) }));
    assert.deepEqual(newest, { newest: true, oldest: false }, "the 200 shown are the newest finished, not the oldest");
    await page.click("[data-act=olderdone]"); await page.settle(250);
    assert.equal(await page.run(() => S.tasks.filter(t => t.status === "done").length), 700);
    await page.click("[data-view=todo]"); await page.run(() => { lastSnap = ""; return load() });
    await page.click("[data-view=done]");
    assert.equal(await page.run(() => S.tasks.filter(t => t.status === "done").length), 200, "leaving the tab goes back to 200");
    await page.click("[data-act=olderdone]"); await page.settle(250);
    assert.equal(await page.run(() => S.tasks.filter(t => t.status === "done").length), 700, "and the button adds 500, not 1,000");
  });

  it("Show older goes away when the window holds every finished task the engine can send", async page => {
    await page.run(addTasks, { open: 5, done: 20100 });
    await page.click("[data-view=done]");
    assert.ok(await page.run(() => !!document.querySelector("[data-act=olderdone]")));
    await page.run(() => { doneWanted = 19800; lastSnap = ""; return load() });
    await page.click("[data-act=olderdone]");
    await page.waitFor(() => S.tasks.filter(t => t.status === "done").length >= 20000);
    assert.equal(await page.run(() => document.querySelectorAll("[data-act=olderdone]").length), 0, "nothing more can be asked for");
    assert.match(await page.run(() => document.querySelector(".sub2").textContent), /The 20000 most recently finished of 2010\d finished tasks/);
  });

  it("the Done count moves when a task is finished or reopened here, without waiting for the next poll", async page => {
    await page.run(addTasks, { open: 5, done: 300 });
    await page.run(() => { cur = 50002; render() }); // a single task (the demo's first row is a conversation of two)
    const before = await page.run(() => doneTotal());
    await page.key("x");
    await page.waitFor(() => find(50002).status === "done");
    assert.equal(await page.run(() => doneTotal()), before + 1, "the count moved as soon as the task was finished here, not at the next poll");
    await page.key("z");
    await page.waitFor(() => find(50002).status === "open");
    assert.equal(await page.run(() => doneTotal()), before);
  });
});
