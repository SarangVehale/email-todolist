// What the window is to a screen reader and to a pointer: every control has a name, every dialog is labelled, text is readable on the black
// background, and nothing that is clicked is smaller than WCAG 2.2's 24 by 24 pixel minimum (2.5.8). These are checks a machine can make;
// they are not a screen-reader test and do not cover focus order or what is announced.
import assert from "node:assert/strict";
import { suite, openSettings } from "./harness.mjs";

/** Runs in the page: what is wrong with what is on screen now. */
const problems = () => {
  const lum = ([r, g, b]) => { const f = v => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4) }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b) };
  const parse = c => { const m = c.match(/rgba?\(([^)]+)\)/); if (!m) return null; const p = m[1].split(/[ ,\/]+/).filter(Boolean).map(Number); return { rgb: p.slice(0, 3), a: p.length > 3 ? p[3] : 1 } };
  const over = (top, bottom) => top.a >= 1 ? top.rgb : top.rgb.map((v, i) => v * top.a + bottom[i] * (1 - top.a));
  const bgOf = el => { const chain = []; for (let e = el; e; e = e.parentElement) { const c = parse(getComputedStyle(e).backgroundColor); if (c && c.a > 0) chain.push(c); if (c && c.a >= 1) break } let base = [0, 0, 0]; for (const c of chain.reverse()) base = over(c, base); return base };
  const ratio = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05) };
  const shown = el => { const cs = getComputedStyle(el), r = el.getBoundingClientRect(); return cs.display !== "none" && cs.visibility !== "hidden" && +cs.opacity !== 0 && r.width > 0 && r.height > 0 };
  const label = el => {
    const al = el.getAttribute("aria-label"); if (al && al.trim()) return al;
    const lb = el.getAttribute("aria-labelledby"); if (lb) return lb.split(" ").map(i => document.getElementById(i)?.textContent || "").join(" ").trim();
    if (el.matches("input, select, textarea")) return [...(el.labels || [])].map(x => x.textContent.trim()).join(" ").trim() || el.getAttribute("title") || "";
    return el.textContent.trim() || el.getAttribute("title") || el.alt || "";
  };
  const out = { unnamed: [], small: [], lowContrast: [], unlabelledDialogs: [], lowest: 21 };
  const controls = "button, a[href], input:not([type=hidden]), select, textarea, [role=button], [role=checkbox]";
  for (const el of document.querySelectorAll(controls)) {
    if (!shown(el)) continue;
    const id = el.tagName.toLowerCase() + (el.id ? "#" + el.id : "") + (el.className ? "." + String(el.className).trim().split(/\s+/).join(".") : "");
    if (!label(el)) out.unnamed.push(id + " " + el.outerHTML.slice(0, 80));
    const r = el.getBoundingClientRect();
    if ((r.width < 23.5 || r.height < 23.5) && !el.matches("input[type=checkbox], input[type=radio], input[type=file]")) out.small.push(id + " " + Math.round(r.width) + "x" + Math.round(r.height));
  }
  for (const el of document.querySelectorAll("body *")) {
    if (![...el.childNodes].some(n => n.nodeType === 3 && n.textContent.trim()) || !shown(el)) continue;
    const cs = getComputedStyle(el), fg = parse(cs.color); if (!fg) continue;
    const bg = bgOf(el), rt = ratio(over(fg, bg), bg), px = parseFloat(cs.fontSize);
    const need = px >= 24 || (px >= 18.66 && +cs.fontWeight >= 700) ? 3 : 4.5;
    out.lowest = Math.min(out.lowest, rt);
    if (rt < need) out.lowContrast.push(el.tagName.toLowerCase() + (el.className ? "." + String(el.className).trim().split(/\s+/).join(".") : "") + " " + rt.toFixed(2) + " (needs " + need + ") " + JSON.stringify(el.textContent.trim().slice(0, 24)));
  }
  for (const p of document.querySelectorAll(".panel")) {
    if (p.getAttribute("role") !== "dialog" || p.getAttribute("aria-modal") !== "true" || !(p.getAttribute("aria-label") || "").trim() || p.getAttribute("aria-label") === "Inbox Agent") out.unlabelledDialogs.push(p.outerHTML.slice(0, 100));
  }
  out.lowest = +out.lowest.toFixed(2);
  return out;
};

const clean = (found, what) => {
  assert.deepEqual(found.unnamed, [], what + ": controls a screen reader cannot name");
  assert.deepEqual(found.small, [], what + ": click targets under 24 by 24 pixels");
  assert.deepEqual(found.lowContrast, [], what + ": text below the contrast WCAG asks for");
  assert.deepEqual(found.unlabelledDialogs, [], what + ": dialogs without a role, modal flag and name of their own");
};

suite("what a screen reader and a pointer meet", it => {
  it("the list", async page => {
    await page.settle(400);
    clean(await page.run(problems), "the list");
  });

  it("Settings: every field and menu has the label drawn above it", async page => {
    await openSettings(page);
    clean(await page.run(problems), "Settings");
    assert.equal(await page.run(() => document.querySelector("[data-s=topic]").labels[0]?.textContent), "ntfy topic", "the label is tied to its box, not just drawn near it");
    assert.equal(await page.run(() => document.querySelector("[data-s=aimode]").getAttribute("aria-label")), "Ranking mode");
  });

  it("the wizard, every screen", async page => {
    await page.run(() => { W.user = "me@gmail.com"; W.step = "welcome"; openWizard() });
    const screens = [["welcome", "other"], ["address", "other"], ["guide", "gmail"], ["guide", "gmail", "refused"], ["guide", "gmail", "shown"], ["other", "other"], ["outlook", "outlook"], ["ready", "gmail"]];
    for (const [step, provider, extra] of screens) {
      await page.run((st, pr, ex) => {
        W.step = st; W.provider = pr; W.host = "imap.example.org"; W.show = ex === "shown"; W.pw = ex ? "abcd efgh ijkl mnop" : "";
        W.result = ex === "refused" ? friendly("sign-in refused: [AUTHENTICATIONFAILED] Invalid credentials (Failure)", pr, "") : null; drawWizard();
        if (ex === "refused") veil.querySelector("details.why").open = true;
      }, step, provider, extra);
      await page.settle(100);
      clean(await page.run(problems), "wizard screen " + step + (extra ? " (" + extra + ")" : ""));
    }
  });

  it("the easy view and the light colours pass the same checks at every text size", async page => {
    const looks = [["easy", "light", "large"], ["easy", "light", "huge"], ["easy", "dark", "large"], ["dense", "light", "normal"], ["dense", "dark", "huge"]];
    for (const [mode, theme, size] of looks) {
      const where = `${mode} view, ${theme} colours, ${size} text`;
      await page.run((m, t, s) => { localStorage.clear(); if (m === "easy") localStorage.setItem("ia.easy", "1"); localStorage.setItem("ia.theme", t); localStorage.setItem("ia.size", s); applyLook(); lastSnap = ""; render() }, mode, theme, size);
      await page.settle(250);
      clean(await page.run(problems), where + ": the list");
      await page.run(() => { cur = lastList[0].uid; openUid = cur; render() }); await page.settle(300);
      clean(await page.run(problems), where + ": an opened message");
      await page.run(() => { openUid = null; render() });
      await openSettings(page);
      clean(await page.run(problems), where + ": Settings");
      if (mode === "easy") { await page.click("[data-s=moresettings]"); await page.settle(200); clean(await page.run(problems), where + ": Settings with More options") }
      await page.key("Escape"); await page.key("Escape"); await page.settle(150);
      await page.run(() => { W.user = "me@gmail.com"; W.provider = "gmail"; W.show = false; W.pw = ""; W.result = friendly("sign-in refused: x", "gmail", ""); openWizard() });
      for (const step of ["welcome", "guide", "ready"]) {
        await page.run(st => { W.step = st; drawWizard() }, step); await page.settle(120);
        clean(await page.run(problems), where + ": the " + step + " screen");
      }
      await page.run(() => closeVeil());
    }
    await page.run(() => localStorage.clear());
  });

  it("the Add a mailbox dialog, the add-task dialog and the shortcut card", async page => {
    await openSettings(page); await page.click("[data-s=addacct]"); await page.waitFor(() => document.querySelector("#m-user"));
    await page.run(() => { const s = document.querySelector("#m-provider"); s.value = "other"; s.dispatchEvent(new Event("change", { bubbles: true })) });
    await page.waitFor(() => document.querySelector("#m-port"));
    clean(await page.run(problems), "Add a mailbox (other server)");
    await page.key("Escape"); await page.key("Escape"); await page.settle(200);
    await page.key("a"); await page.waitFor(() => document.querySelector("#at-title"));
    clean(await page.run(problems), "Add a task");
    await page.key("Escape"); await page.settle(200);
    await page.key("?"); await page.settle(300);
    clean(await page.run(problems), "the shortcut card");
  });

  it("an opened row with its reply drafts and the other tabs", async page => {
    await page.settle(300);
    await page.key("Enter"); await page.settle(500); // opens the row the cursor is on
    clean(await page.run(problems), "an opened row");
    await page.key("Escape"); await page.settle(150);
    for (const tab of ["Starred", "Snoozed", "Done", "Log"]) {
      await page.run(name => { [...document.querySelectorAll("#bar nav button")].find(b => b.textContent.trim().toLowerCase().startsWith(name))?.click() }, tab.toLowerCase());
      await page.settle(300);
      clean(await page.run(problems), "the " + tab + " tab");
    }
  });

  it("the Restore from a backup dialog", async page => {
    await openSettings(page);
    await page.run(() => { const i = document.querySelector("#bk-file"); const dt = new DataTransfer(); dt.items.add(new File([JSON.stringify({ format: "inbox-agent-backup", version: 1 })], "b.json", { type: "application/json" })); i.files = dt.files; i.dispatchEvent(new Event("change", { bubbles: true })) });
    await page.waitFor(() => veil.dataset.kind === "restore" || document.querySelector("#veil .panel h2")?.textContent.startsWith("Restore"));
    clean(await page.run(problems), "the restore dialog");
  });

  // A check that cannot fail proves nothing: the same function must notice a bad control, a small target and grey text.
  it("the checks themselves notice a nameless control, a small target and text that is too faint", async page => {
    await page.run(() => {
      document.body.insertAdjacentHTML("beforeend", '<input id="zz-nameless" type="text" style="position:fixed;top:5px;left:5px">' +
        '<button id="zz-tiny" aria-label="tiny" style="position:fixed;top:40px;left:5px;width:10px;height:10px;padding:0"></button>' +
        '<p id="zz-faint" style="position:fixed;top:70px;left:5px;color:#333;background:#000;font-size:14px">faint words</p>');
    });
    const found = await page.run(problems);
    assert.ok(found.unnamed.some(x => x.includes("zz-nameless")), "a control with no label is reported");
    assert.ok(found.small.some(x => x.includes("zz-tiny")), "a 10 pixel button is reported");
    assert.ok(found.lowContrast.some(x => x.includes("faint words")), "grey on black is reported");
  });
});
