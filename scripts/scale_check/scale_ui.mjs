// Times the real window's load() and render() with the seeded database, against the targets of docs/designs/phase8.md.
let pages = [];
for (let i = 0; i < 40 && !pages.length; i++) {
  try { pages = (await (await fetch("http://127.0.0.1:9222/json")).json()).filter(p => p.type === "page"); } catch { /* not up yet */ }
  if (!pages.length) await new Promise(r => setTimeout(r, 500));
}
if (!pages.length) { console.log("FAIL window not reachable"); process.exit(1); }
const ws = new WebSocket(pages[0].webSocketDebuggerUrl);
await new Promise(r => (ws.onopen = r));
let id = 0; const pending = new Map();
ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id) } };
const send = (method, params = {}) => new Promise(r => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })) });
const ev = async expr => { const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true }); return r.result.exceptionDetails ? "ERR " + JSON.stringify(r.result.exceptionDetails.exception?.description || r.result.exceptionDetails.text) : r.result.result.value; };
let bad = 0;
const check = (name, cond, detail) => { console.log((cond ? "PASS " : "FAIL ") + name + "  [" + detail + "]"); if (!cond) bad++; };
await new Promise(r => setTimeout(r, 1500));
await ev("(() => { try { closeVeil() } catch (e) {} return 1 })()");
const load = await ev("(async () => { const t0 = performance.now(); lastSnap = ''; await load(); return Math.round(performance.now() - t0) })()");
check("the window loads and draws in under 500 ms", load < 500, load + " ms");
const tasks = await ev("S.tasks.length"), doneTotal = await ev("S.done_total");
check("the window holds the open tasks and only the newest 200 finished", tasks === 3200 && doneTotal === 5000, tasks + " tasks, done_total " + doneTotal);
const rows = await ev("document.querySelectorAll('.row').length");
check("the list draws 150 rows to start with", rows === 150, rows + " rows");
const more = await ev("[...document.querySelectorAll('[data-act=more],[data-act=moreall]')].map(b => b.textContent).join(' | ')");
check("with 'Show more' and 'Show all' under them", /Show 150 more/.test(more) && /Show all/.test(more), more);
const nodes = await ev("document.getElementsByTagName('*').length");
check("the page is under 25,000 elements", nodes < 25000, nodes + " elements");
const render = await ev("(() => { const t0 = performance.now(); render(); return Math.round(performance.now() - t0) })()");
check("drawing the list again takes under 150 ms", render < 150, render + " ms");
const keyj = await ev("(() => { const t0 = performance.now(); document.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true })); document.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true })); return Math.round((performance.now() - t0) / 2) })()");
check("a key press takes under 80 ms", keyj < 80, keyj + " ms");
const done = await ev("(() => { view = 'done'; const t0 = performance.now(); render(); const ms = Math.round(performance.now() - t0); const r = document.querySelectorAll('.row').length; const note = document.querySelector('.sub2')?.textContent || ''; view = 'todo'; render(); return JSON.stringify({ ms, r, note }) })()");
const d = JSON.parse(done);
check("the Done tab draws in under 300 ms and says it shows the 200 most recently finished of 5000", d.ms < 300 && /200 most recently finished of 5000/.test(d.note), done);
const walk = await ev("(async () => { const key = k => document.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true })); for (let i = 0; i < 400; i++) key('j'); await new Promise(r => setTimeout(r, 50)); return JSON.stringify({ rows: document.querySelectorAll('.row').length, cursorRows: document.querySelectorAll('.row.cur').length }) })()");
const w = JSON.parse(walk);
check("400 presses of j reach row 400 and draw more rows on the way", w.rows >= 400 && w.cursorRows === 1, walk);
const search = await ev("(() => { const t0 = performance.now(); flt.q = 'number 777'; render(); const ms = Math.round(performance.now() - t0); const r = document.querySelectorAll('.row').length; flt.q = ''; render(); return JSON.stringify({ ms, r }) })()");
check("a search draws quickly and finds the task", JSON.parse(search).ms < 200 && JSON.parse(search).r >= 1, search);
ws.close(); process.exit(bad ? 1 : 0);
