// Sign in with Google and with Microsoft in the window (design: docs/designs/phase12.md). The buttons exist only when the build has a client ID for them
// (the demo backend has one when the address says ?signin=google,microsoft), they replace "copy a code" for people who want that, and everything
// that can go wrong is a sentence.
import assert from "node:assert/strict";
import { suite, wrapMock, unwrapMock, release, openSettings } from "./harness.mjs";

const WITH = "/demo.html?demo=1&signin=google,microsoft";
/** A new install (the demo signed out), the first screens, and a short wait between asks. */
const newInstall = async page => {
  await page.run(() => { localStorage.clear(); applyLook(); SIGNIN_POLL = 30 });
  await page.run(() => rpc("sign_out"));
  await page.waitFor(() => veil.dataset.kind === "wizard" && veil.querySelector("h2")?.textContent === "Welcome.");
  await page.waitFor(() => S.sign_in); // the window has been told what this build offers
};
const toScreen = async (page, address) => {
  await newInstall(page);
  await page.click("[data-w=next]"); await page.waitFor(() => W.step === "address");
  await page.fill("#w-user", address); await page.click("[data-w=next]");
  await page.waitFor(() => ["guide", "other", "outlook"].includes(W.step));
};
const calls = page => page.run(() => window.__calls.map(c => c.cmd).filter(c => c !== "state"));

suite("sign in with an account: the first-run screens", it => {
  it("is offered only when the build has a client ID, and the code stays for everyone", async page => {
    await toScreen(page, "me@gmail.com");
    assert.equal(await page.run(() => document.querySelector("h2").textContent), "Connect your Gmail");
    assert.match(await page.run(() => document.querySelector("[data-w=signin]").textContent), /Sign in with Google/);
    assert.match(await page.run(() => veil.textContent), /Or use a code\. That works for everyone\./);
    assert.ok(await page.run(() => !!document.querySelector("[data-w=open], #w-pw")), "the code way is still there");
    assert.equal(await page.run(() => document.querySelector("[data-w=open]").classList.contains("pri")), false, "the button for the account is the main one");
  }, { path: WITH });

  it("without a client ID there is no button, and the screen is the code screen of before", async page => {
    await toScreen(page, "me@gmail.com");
    assert.equal(await page.run(() => !!document.querySelector("[data-w=signin]")), false);
    assert.equal(await page.run(() => document.querySelector("h2").textContent), "Make a code for Inbox Agent");
  });

  it("Google: the page opens, the window waits, and setup finishes as Connect finishes it, with no password and no test of one", async page => {
    await toScreen(page, "me@gmail.com");
    await page.run(wrapMock, [{ cmd: "oauth_poll", hold: true }]); // the answer comes when the test says, so the waiting can be looked at
    await page.click("[data-w=signin]");
    await page.waitFor(() => /Google's page is open/.test(document.querySelector("#w-result").textContent));
    assert.equal(await page.run(() => W.busy), true);
    assert.ok(await page.run(() => !!document.querySelector("[data-w=cancelsignin]")), "it can be cancelled");
    assert.equal(await page.run(() => document.querySelector("[data-w=back]").disabled), true);
    await release(page); await release(page); // still waiting, then done
    await page.waitFor(() => W.step === "ready");
    assert.ok((await page.run(() => window.__opened)).some(u => u.startsWith("https://accounts.google.com/")), "Google's page was opened");
    const asked = (await calls(page)).filter((c, i, a) => c !== "oauth_poll" || a[i - 1] !== "oauth_poll");
    assert.deepEqual(asked, ["oauth_start", "oauth_poll", "oauth_finish", "save_settings", "finish_setup"], "no password, no test of one");
    assert.deepEqual(await page.run(() => window.__calls.find(c => c.cmd === "oauth_start").a), { provider: "google", user: "me@gmail.com" });
    assert.deepEqual(await page.run(() => [M.settings.oauth[0], M.settings.account.host, M.settings.account.user]), ["google", "imap.gmail.com", "me@gmail.com"]);
    assert.equal(await page.run(() => document.querySelector("h2").textContent), "You're connected.");
  }, { path: WITH });

  it("Cancel stops the wait, tells the engine, and leaves nothing connected", async page => {
    await toScreen(page, "me@gmail.com");
    await page.run(wrapMock, [{ cmd: "oauth_poll", hold: true }]);
    await page.click("[data-w=signin]");
    await page.waitFor(() => document.querySelector("[data-w=cancelsignin]"));
    await page.click("[data-w=cancelsignin]");
    assert.match(await page.run(() => document.querySelector("#w-result").textContent), /Cancelled\. Nothing was connected\./);
    assert.deepEqual(await page.run(() => [W.busy, document.querySelector("[data-w=signin]").disabled, document.querySelector("[data-w=back]").disabled]), [false, false, false], "the screen works again");
    assert.ok((await calls(page)).includes("oauth_cancel"));
    await page.run(() => { window.__held.forEach(f => f()) }); // the answer that was being waited for arrives late
    await page.settle(250);
    assert.equal(await page.run(() => W.step), "guide", "a late answer does not carry on");
    assert.ok(!(await calls(page)).includes("oauth_finish") && !(await calls(page)).includes("finish_setup"));
  }, { path: WITH });

  it("a sign-in that fails, or that the mail server refuses, is a sentence", async page => {
    await toScreen(page, "me@gmail.com");
    await page.run(wrapMock, [{ cmd: "oauth_poll", result: { state: "failed", error: "You chose not to allow Inbox Agent, so nothing was connected." } }]);
    await page.click("[data-w=signin]");
    await page.waitFor(() => document.querySelector("#w-result .bad"));
    assert.equal(await page.run(() => document.querySelector("#w-result .bad").textContent), "You chose not to allow Inbox Agent, so nothing was connected.");
    assert.equal(await page.run(() => W.busy), false);
    await page.run(wrapMock, [{ cmd: "oauth_finish", fail: "sign-in refused: [AUTHENTICATIONFAILED] Invalid credentials (Failure)" }]);
    await page.click("[data-w=signin]");
    await page.waitFor(() => /did not let Inbox Agent in/.test(document.querySelector("#w-result").textContent));
    assert.match(await page.run(() => document.querySelector("#w-result .bad").textContent), /^Google did not let Inbox Agent in with that account\. Make sure you signed in with the address you typed, then try again\.$/);
    assert.equal(await page.run(() => document.querySelector("#w-result code").textContent), "sign-in refused: [AUTHENTICATIONFAILED] Invalid credentials (Failure)", "the server's words are behind Details");
    assert.equal(await page.run(() => W.step), "guide");
  }, { path: WITH });

  it("Microsoft: Outlook and Hotmail addresses sign in with a code that is typed on Microsoft's page", async page => {
    await toScreen(page, "me@hotmail.com");
    assert.equal(await page.run(() => W.step), "outlook");
    assert.match(await page.run(() => document.querySelector("[data-w=signin]").textContent), /Sign in with Microsoft/);
    assert.equal(await page.run(() => /cannot read this mailbox yet/.test(veil.textContent)), false, "no apology when there is a way in");
    await page.run(wrapMock, [{ cmd: "oauth_poll", hold: true }]);
    await page.click("[data-w=signin]");
    await page.waitFor(() => document.querySelector(".devcode"));
    assert.equal(await page.run(() => document.querySelector(".devcode b").textContent), "ABCD-EFGH");
    assert.match(await page.run(() => document.querySelector("#w-result").textContent), /www\.microsoft\.com\/link/);
    await page.click("[data-w=opendevice]");
    await page.waitFor(() => (window.__opened || []).includes("https://www.microsoft.com/link"));
    await page.run(() => { window.__held.forEach(f => f()) }); await page.run(unwrapMock);
    await page.waitFor(() => W.step === "ready");
    assert.deepEqual(await page.run(() => [M.settings.oauth[0], M.settings.account.host]), ["microsoft", "outlook.office365.com"]);
  }, { path: WITH });
});

suite("sign in with an account: mailboxes added later", it => {
  const addDialog = async page => { await openSettings(page); await page.click("[data-s=addacct]"); await page.waitFor(() => document.activeElement.id === "m-user") };

  it("the Add a mailbox dialog offers it for Gmail, adds the mailbox, and Settings says how it signs in", async page => {
    await page.run(() => { SIGNIN_POLL = 30 });
    await addDialog(page);
    assert.match(await page.run(() => document.querySelector("[data-m=signin]").textContent), /Sign in with Google/);
    await page.click("[data-m=signin]");
    await page.waitFor(() => /Type your email address first/.test(document.querySelector("#msignin").textContent));
    await page.fill("#m-user", "new@gmail.com");
    await page.run(wrapMock, []);
    await page.click("[data-m=signin]");
    await page.waitFor(() => M.accounts.length === 3);
    assert.deepEqual(await page.run(() => window.__calls.find(c => c.cmd === "oauth_finish").a), { add: true });
    await page.waitFor(() => veil.dataset.kind === "settings");
    await page.waitFor(() => [...document.querySelectorAll(".mbx")].length === 3);
    const row = await page.run(() => { const r = [...document.querySelectorAll(".mbx")].find(x => x.textContent.includes("new@gmail.com")); return { text: r.textContent, pw: !!r.querySelector("input[type=password]"), again: !!r.querySelector("[data-s=resign]") } });
    assert.match(row.text, /Signed in with Google\. No password is kept\./);
    assert.deepEqual([row.pw, row.again], [false, true]);
    assert.equal(await page.run(() => document.querySelectorAll(".mbx input[type=password]").length), 2, "the other two still take a password");
  }, { path: WITH });

  it("Sign in again signs the same mailbox in again and says the list is untouched", async page => {
    await page.run(() => { SIGNIN_POLL = 30; M.settings.oauth = { 1: "google" }; syncAccounts() });
    await page.run(() => { lastSnap = ""; return load() });
    await openSettings(page);
    await page.waitFor(() => document.querySelector("[data-s=resign]"));
    await page.run(wrapMock, []);
    await page.click("[data-s=resign]");
    await page.waitFor(() => /Signed in\. Your list is untouched\./.test(document.querySelector("[data-pwres='1']").textContent));
    assert.deepEqual(await page.run(() => [window.__calls.find(c => c.cmd === "oauth_start").a, window.__calls.find(c => c.cmd === "oauth_finish").a]), [{ provider: "google", user: "sam@work.example" }, { id: 1 }]);
  }, { path: WITH });

  it("a mailbox on another provider's server is not offered it, and neither is anyone without a client ID", async page => {
    await addDialog(page);
    await page.run(() => { const s = document.querySelector("#m-provider"); s.value = "yahoo"; s.dispatchEvent(new Event("change", { bubbles: true })) });
    await page.waitFor(() => document.querySelector("#m-provider").value === "yahoo");
    assert.equal(await page.run(() => !!document.querySelector("[data-m=signin]")), false);
  }, { path: WITH });

  it("without a client ID the dialog is what it was", async page => {
    await addDialog(page);
    assert.equal(await page.run(() => !!document.querySelector("[data-m=signin], #msignin")), false);
  });
});
