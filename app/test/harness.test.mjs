// The harness's own start-up, with a fake browser (a few lines of Node) in place of Chrome: a browser that is slow or dies at first is started
// again, and one that never comes up is reported. This is the failure that cancelled every test of a file in CI runs 10 and 22 and in a release
// rehearsal: the browser had not opened its debugging port within a fixed 15 seconds.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import { Harness } from "./harness.mjs";

// The fake browser: finds --user-data-dir=<folder> in its arguments and, unless FAKE_BROWSER_SLOW_FIRST_START says its first start is to fail,
// writes <folder>/DevToolsActivePort as a browser does when it is ready.
const script = path.join(os.tmpdir(), `fake-browser-${process.pid}.mjs`);
fs.writeFileSync(script, `
import fs from "node:fs"; import path from "node:path";
const dir = process.argv.find(a => a.startsWith("--user-data-dir="))?.slice("--user-data-dir=".length);
const mark = process.env.FAKE_BROWSER_MARK;
if (mark && !fs.existsSync(mark)) { fs.writeFileSync(mark, "started once"); process.exit(1) } // the first start fails
if (dir) fs.writeFileSync(path.join(dir, "DevToolsActivePort"), "9\\n/devtools/browser/fake\\n");
setTimeout(() => process.exit(0), 200);
`);

describe("the harness starting a browser", { skip: typeof WebSocket === "undefined" ? "needs Node 22 or later (the built-in WebSocket)" : false }, () => {
  it("starts it again when the first start produced nothing", async () => {
    const mark = path.join(os.tmpdir(), `fake-browser-mark-${process.pid}`);
    fs.rmSync(mark, { force: true });
    process.env.FAKE_BROWSER_MARK = mark;
    try {
      const h = await Harness.start({ exe: process.execPath, lead: [script], waitMs: 3000, attempts: 2 });
      assert.ok(h && h.browserUrl === "http://127.0.0.1:9", "the second start was used");
      assert.ok(fs.existsSync(mark), "there was a first start");
      await h.stop();
    } finally { delete process.env.FAKE_BROWSER_MARK; fs.rmSync(mark, { force: true }) }
  });

  it("without a second try the same browser is reported as not started", async () => {
    const mark = path.join(os.tmpdir(), `fake-browser-mark2-${process.pid}`);
    fs.rmSync(mark, { force: true });
    process.env.FAKE_BROWSER_MARK = mark;
    try {
      await assert.rejects(Harness.start({ exe: process.execPath, lead: [script], waitMs: 600, attempts: 1 }), /did not start within/);
    } finally { delete process.env.FAKE_BROWSER_MARK; fs.rmSync(mark, { force: true }) }
  });

  it("a browser that never comes up is reported after both tries, having waited the whole time for each", async () => {
    const nothing = path.join(os.tmpdir(), `fake-browser-nothing-${process.pid}.mjs`);
    fs.writeFileSync(nothing, "setTimeout(() => process.exit(0), 100);\n");
    const began = Date.now();
    try {
      await assert.rejects(Harness.start({ exe: process.execPath, lead: [nothing], waitMs: 500, attempts: 2 }), /did not start within 1 second$/);
      assert.ok(Date.now() - began >= 1000, "two waits of half a second each, so it was tried twice");
    } finally { fs.rmSync(nothing, { force: true }) }
  });
});

process.on("exit", () => fs.rmSync(script, { force: true }));
