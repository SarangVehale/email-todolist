// Mail is text somebody else wrote, and the window builds a lot of HTML from it. This feeds every field a message can supply a payload that would run
// script or break out of an attribute, draws every place it can be seen (the list, an opened row with its draft, an invitation, the other tabs,
// Settings), and checks three things: nothing ran, nothing was made from the payload (an element of its own), and the text is on the page as text.
import assert from "node:assert/strict";
import { suite, openSettings } from "./harness.mjs";

/** Runs in the page: puts hostile text in every field of three tasks (one payload style each) and in the mailbox and rule text, then draws the list. */
function plant() {
  window.__xss = [];
  const styles = [
    id => `<img id="pwn_${id}" src="x" onerror="window.__xss.push('${id}')">`,
    id => `"><img id="pwn_${id}" src=x onerror="window.__xss.push('${id}')">`,
    id => `'><svg id="pwn_${id}" onload="window.__xss.push('${id}')"></svg><b id="pwn_b_${id}">`,
  ];
  styles.forEach((style, k) => {
    const p = f => style(`${f}${k}`);
    const t = M.mk(70001 + k, p("sender"), `${p("addr")}@x.example`, p("subject"), 1 + k, 4, p("category"), p("action"), false, false, p("excerpt"), p("why"), { known: false, reply: false, bulk: false });
    Object.assign(t, {
      note: p("note"), reply_to: p("replyto"), unsubscribe: `javascript:window.__xss.push('unsub${k}')`, needs_reply: true, due: "2099-01-01",
      attachments: [p("attachment")], attachment_count: 1, remind: p("remind"), thread: p("thread"),
      draft: { body: p("draft"), source: p("source"), tone: "friendly" },
      event: { summary: p("evsummary"), start: "20991120T103000Z", end: "20991120T110000Z", tzid: "", location: p("evlocation"), zone_name: p("evzone"), all_day: false, cancelled: false },
    });
    M.tasks.unshift(t);
  });
  M.accounts[0] = { ...M.accounts[0], user: p0("mbuser"), host: "imap.example.com" };
  M.rules.push({ id: "hostile", field: "subject", text: p0("ruletext"), priority: 2, category: null, file: false });
  function p0(id) { return styles[1](id) }
  lastSnap = ""; return load();
}

/** Runs in the page: what has happened since `plant`. */
const outcome = () => ({
  ran: window.__xss.slice(),
  built: [...document.querySelectorAll('[id^="pwn_"]')].map(e => e.id),
  attributeBroken: [...document.querySelectorAll("[onerror], [onload]")].map(e => e.tagName.toLowerCase() + "#" + e.id),
  javascriptLinks: [...document.querySelectorAll("a[href], [src], [action], [formaction]")].filter(e => /^\s*javascript:/i.test(e.getAttribute("href") || e.getAttribute("src") || e.getAttribute("action") || e.getAttribute("formaction") || "")).length,
  text: document.body.textContent,
});

const clean = async (page, where) => {
  const o = await page.run(outcome);
  assert.deepEqual(o.ran, [], `${where}: script ran`);
  assert.deepEqual(o.built, [], `${where}: elements were made from mail text`);
  assert.deepEqual(o.attributeBroken, [], `${where}: an event attribute was made from mail text`);
  assert.equal(o.javascriptLinks, 0, `${where}: a javascript: link was made from mail text`);
  return o;
};

suite("hostile text in a message", it => {
  it("the list and an opened row show every field as text, and nothing runs", async page => {
    await page.run(plant);
    await page.settle(400);
    const list = await clean(page, "the list");
    assert.ok(list.text.includes('<img id="pwn_subject0"'), "the subject is on the page as text, not dropped");
    // open each planted row: its detail, its draft, its note, its attachments, its invitation and its reminder
    for (let k = 0; k < 3; k++) {
      await page.run(uid => { const r = document.querySelector(`.row[data-uid="${uid}"]`); r?.click() }, 70001 + k);
      await page.settle(250);
      await clean(page, `the opened row ${k}`);
      await page.key("Escape"); await page.settle(100);
    }
    const opened = await page.run(() => { document.querySelector('.row[data-uid="70001"]')?.click(); return document.body.textContent });
    assert.ok(opened.includes('<img id="pwn_note0"') || opened.includes("pwn_note0"), "the note is shown as text when the row is open");
  });

  it("every tab, Settings (mailboxes and rules) and the dialogs stay clean", async page => {
    await page.run(plant);
    await page.settle(300);
    for (const tab of ["starred", "snoozed", "done", "log"]) {
      await page.run(name => { [...document.querySelectorAll("#bar nav button")].find(b => b.textContent.trim().toLowerCase().startsWith(name))?.click() }, tab);
      await page.settle(250);
      await clean(page, `the ${tab} tab`);
    }
    await page.run(() => { [...document.querySelectorAll("#bar nav button")].find(b => b.textContent.trim().toLowerCase().startsWith("to-do"))?.click() });
    await openSettings(page); await page.settle(300);
    const s = await clean(page, "Settings");
    assert.ok(s.text.includes('<img id="pwn_mbuser"') || s.text.includes("pwn_mbuser"), "the mailbox's address is shown as text in Settings");
    await page.key("Escape"); await page.key("Escape"); await page.settle(150);
    await page.key("?"); await page.settle(250);
    await clean(page, "the shortcut card");
  });

  // A check that cannot fail proves nothing: the detectors must notice markup that was built from text.
  it("the checks notice an element made from text and a script that ran", async page => {
    await page.run(() => {
      window.__xss = [];
      const d = document.createElement("div"); d.innerHTML = '<img id="pwn_probe" src="x" onerror="window.__xss.push(\'probe\')"><a href="javascript:void(0)">x</a>'; document.body.appendChild(d);
    });
    await page.settle(300);
    const o = await page.run(outcome);
    assert.ok(o.built.includes("pwn_probe"), "an element with the payload's id is reported");
    assert.ok(o.ran.includes("probe"), "a script that ran is reported");
    assert.ok(o.attributeBroken.length >= 1 && o.javascriptLinks >= 1, "an event attribute and a javascript: link are reported");
  });
});
