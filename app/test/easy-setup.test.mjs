// The first-run screens (design: docs/designs/phase12.md): the email address, the provider read from it, one screen to make a code, one button to connect,
// sentences instead of server text, and nothing asked that a grandmother could not answer.
import assert from "node:assert/strict";
import { suite, wrapMock, unwrapMock, release, openSettings } from "./harness.mjs";

/** Signs the demo out, which is what a new install looks like, and waits for the first screen. */
const newInstall = async (page, keepLook = false) => {
  if (!keepLook) await page.run(() => { localStorage.clear(); applyLook() }); // the look is kept per device, and pages of one test file share it
  await page.run(() => rpc("sign_out"));
  await page.waitFor(() => veil.dataset.kind === "wizard" && veil.querySelector("h2")?.textContent === "Welcome.");
};
const here = page => page.run(() => ({ step: W.step, h2: veil.querySelector("h2")?.textContent, label: veil.querySelector(".panel")?.getAttribute("aria-label"), small: veil.querySelector(".step")?.textContent || "" }));
/** Welcome, then the address typed, then Continue. */
const toGuide = async (page, address, keepLook = false) => {
  await newInstall(page, keepLook);
  await page.click("[data-w=next]"); await page.waitFor(() => W.step === "address");
  await page.fill("#w-user", address); await page.click("[data-w=next]");
  await page.waitFor(() => ["guide", "other", "outlook"].includes(W.step));
};
/** The commands asked of the backend, without the window's own background refresh. */
const calls = page => page.run(() => window.__calls.map(c => c.cmd).filter(c => c !== "state"));

suite("which provider an address belongs to", it => {
  it("is read from the part after the @, and a lookalike is not a match", async page => {
    const table = [
      ["me@gmail.com", "gmail"], ["ME@GMAIL.COM", "gmail"], ["me@googlemail.com", "gmail"],
      ["me@outlook.com", "outlook"], ["me@hotmail.co.uk", "outlook"], ["me@live.com", "outlook"], ["me@msn.com", "outlook"], ["me@outlook.fr", "outlook"],
      ["me@yahoo.com", "yahoo"], ["me@yahoo.co.uk", "yahoo"], ["me@ymail.com", "yahoo"], ["me@rocketmail.com", "yahoo"],
      ["me@icloud.com", "icloud"], ["me@me.com", "icloud"], ["me@mac.com", "icloud"], ["me@fastmail.com", "fastmail"], ["me@fastmail.fm", "fastmail"],
      ["me@yahoo.co.jp", "other"], ["me@example.org", "other"], ["me@gmail.com.evil.example", "other"], ["me@notgmail.com", "other"], ["gmail.com@evil.example", "other"],
      ["me@mail.gmail.com", "other"], ["me@hotmail.evil.example", "other"], ["", "other"],
    ];
    const got = await page.run(rows => rows.map(([a]) => providerFor(a)), table);
    assert.deepEqual(got, table.map(r => r[1]), "each address maps to the provider in the same row");
  });

  it("an address has to look like one before Continue is on", async page => {
    const good = ["me@gmail.com", "a.b+c@example.co.uk", " me@example.org "], bad = ["", "me", "me@", "@gmail.com", "me@gmail", "me@.com", "me@@gmail.com", "m e@gmail.com", "me@gm ail.com", "me<x>@gmail.com", "me@gmail.c", "a@b.com,c@d.com", "x".repeat(400) + "@gmail.com"];
    assert.deepEqual(await page.run(a => a.filter(x => !validAddress(x)), good), [], "these are accepted");
    assert.deepEqual(await page.run(a => a.filter(x => validAddress(x)), bad), [], "these are refused");
  });
});

suite("what went wrong, in a sentence", it => {
  it("the engine's text becomes a sentence with one next step, and the engine's own words stay behind Details", async page => {
    const rows = [
      ["sign-in refused: [AUTHENTICATIONFAILED] Invalid credentials (Failure)", "gmail", "imap.gmail.com", /That code didn't work\. Make a new one/, true],
      ["sign-in refused: [ALERT] Application-specific password required: https://support.google.com/accounts/answer/185833 (Failure)", "gmail", "", /That code didn't work/, true],
      ["sign-in refused: LOGIN failed", "other", "mail.example.org", /didn't accept that sign-in/, true],
      ["sign-in refused: [ALERT] Web login required: https://support.google.com/mail/accounts/answer/78754 (Failure)", "gmail", "", /sign in on its website first/, true],
      ["sign-in refused: IMAP access is disabled for your domain", "gmail", "", /switched off for this account/, true],
      ["could not find the server imap.exmaple.org", "other", "imap.exmaple.org", /Couldn't find a mail server called "imap.exmaple.org"/, true],
      ["No such host is known. (os error 11001)", "yahoo", "imap.mail.yahoo.com", /Couldn't reach Yahoo Mail\. Check that this computer is online/, true],
      ["A connection attempt failed because the connected party did not properly respond after a period of time (os error 10060)", "icloud", "", /iCloud Mail took too long to answer/, true],
      ["No connection could be made because the target machine actively refused it. (os error 10061)", "other", "mail.example.org", /Couldn't connect to "mail.example.org"\. Check the server name and the port/, true],
      ["secure connection failed: the certificate is not valid", "fastmail", "", /safe connection to Fastmail couldn't be made.*date or time/, true],
      ["A socket operation was attempted to an unreachable network. (os error 10051)", "gmail", "", /doesn't seem to be online/, true],
      ["too many login attempts, try again later", "gmail", "", /asks you to wait a little/, true],
      ["something nobody has seen before", "gmail", "", /Something went wrong while connecting\. Wait a minute/, true],
      // the engine's own plain sentences are shown as they are, with nothing behind Details
      ["That mail server port isn't valid", "gmail", "", /^That mail server port isn't valid$/, false],
      ["That mailbox is already in the app", "gmail", "", /^That mailbox is already in the app$/, false],
      ["Error: That email address isn't valid", "other", "", /^That email address isn't valid$/, false],
    ];
    const got = await page.run(rs => rs.map(([raw, p, h]) => friendly(raw, p, h)), rows);
    rows.forEach(([raw, , , say, detail], i) => {
      assert.match(got[i].say, say, raw);
      assert.equal(!!got[i].detail, detail, "details for: " + raw);
      if (detail) assert.equal(got[i].detail, raw, "the engine's words are kept whole");
    });
  });

  it("a sentence is never the raw text of a system error", async page => {
    const raws = ["sign-in refused: x (os error 10061)", "No such host is known. (os error 11001)", "connection refused (os error 111)", "[AUTHENTICATIONFAILED] x", "parser panicked"];
    const says = await page.run(rs => rs.map(r => friendly(r, "gmail", "").say), raws);
    for (const s of says) assert.doesNotMatch(s, /os error|\[|sign-in refused|panicked/, s);
  });
});

suite("the first-run screens", it => {
  it("welcome, the address, a code from the provider's page, one Connect, and the list", async page => {
    await newInstall(page);
    let s = await here(page);
    assert.equal(s.label, "Set up Inbox Agent"); assert.match(s.small, /Step 1 of 3/);
    assert.equal(await page.run(() => document.activeElement?.dataset.w), "next", "the keyboard is on the one button");
    await page.run(wrapMock, []); // records the commands
    await page.click("[data-w=next]"); await page.waitFor(() => W.step === "address");
    s = await here(page); assert.equal(s.h2, "What is your email address?"); assert.match(s.small, /Step 2 of 3/);
    assert.equal(await page.run(() => document.querySelector("[data-w=next]").disabled), true, "Continue waits for an address");
    await page.fill("#w-user", "me@gmail.com");
    assert.equal(await page.run(() => document.querySelector("#w-hint").textContent), "That looks like a Gmail address.");
    await page.click("[data-w=next]"); await page.waitFor(() => W.step === "guide");
    s = await here(page); assert.equal(s.h2, "Make a code for Inbox Agent"); assert.match(s.small, /Step 3 of 3/);
    assert.equal(await page.run(() => document.querySelectorAll(".wz .steps li").length), 3, "three steps");
    assert.match(await page.run(() => document.querySelector("[data-w=open]").textContent), /Open Google's page/);
    await page.click("[data-w=open]");
    await page.waitFor(() => (window.__opened || []).length === 1);
    assert.deepEqual(await page.run(() => window.__opened), ["https://myaccount.google.com/apppasswords"], "the page where the code is made");
    assert.match(await page.run(() => document.querySelector("#w-result").textContent), /Google's page should be open/);
    await page.fill("#w-pw", "abcd efgh ijkl mnop");
    await page.click("[data-w=connect]");
    await page.waitFor(() => W.step === "ready");
    s = await here(page); assert.equal(s.h2, "You're connected.");
    assert.deepEqual((await calls(page)).filter(c => c !== "open_page"), ["test_account", "save_account", "save_settings", "finish_setup"], "tested, then saved, then finished: no second button");
    const patch = await page.run(() => window.__calls.find(c => c.cmd === "save_settings").a.patch);
    assert.deepEqual(patch, { writeback: false, hub: { enabled: false } }, "setup makes sure of two things only: the mailbox is not filed in and the phone hub is off (ranking and alerts are left as they are)");
    assert.deepEqual(await page.run(() => [localStorage.getItem("ia.easy"), document.documentElement.classList.contains("easy")]), ["1", true], "the simple view is settled when the mailbox is connected, not only when the last screen is closed");
    assert.equal(await page.run(() => W.pw), "", "the code is not kept in the window once it is saved");
    await page.click("[data-w=finish]");
    await page.waitFor(() => !veil.classList.contains("show"));
    assert.deepEqual(await page.run(() => [S.settings.onboarded, S.settings.account.user]), [true, "me@gmail.com"]);
    assert.deepEqual(await page.run(() => [localStorage.getItem("ia.easy"), document.documentElement.classList.contains("easy"), document.documentElement.classList.contains("light"), document.documentElement.style.zoom]), ["1", true, true, "1.2"], "a new install starts with the easy view: large text, light colours");
  });

  it("finishing setup leaves a choice already made on this device as it was", async page => {
    await page.run(() => { localStorage.clear(); localStorage.setItem("ia.easy", "0"); applyLook() });
    await toGuide(page, "me@gmail.com", true);
    await page.fill("#w-pw", "abcd efgh ijkl mnop"); await page.click("[data-w=connect]"); await page.waitFor(() => W.step === "ready");
    await page.click("[data-w=finish]"); await page.waitFor(() => !veil.classList.contains("show"));
    assert.deepEqual(await page.run(() => [localStorage.getItem("ia.easy"), document.documentElement.classList.contains("easy")]), ["0", false]);
    await page.run(() => localStorage.clear());
  });

  it("Enter does what the big button does, on the address and on the code", async page => {
    await newInstall(page);
    await page.key("Enter"); await page.waitFor(() => W.step === "address"); // on Welcome the keyboard is on Get started
    await page.fill("#w-user", "me@yahoo.com"); await page.key("Enter");
    await page.waitFor(() => W.step === "guide");
    assert.match(await page.run(() => document.querySelector("[data-w=open]").textContent), /Open Yahoo's page/);
    await page.fill("#w-pw", "abcdefghijklmnop"); await page.key("Enter");
    await page.waitFor(() => W.step === "ready");
  });

  it("a refused code says so in a sentence, keeps what was typed, saves nothing, and works after a new code", async page => {
    await toGuide(page, "me@gmail.com");
    const raw = "sign-in refused: [AUTHENTICATIONFAILED] Invalid credentials (Failure)";
    await page.run(wrapMock, [{ cmd: "test_account", fail: raw }]);
    await page.fill("#w-pw", "wrong wrong wrong w");
    await page.click("[data-w=connect]");
    await page.waitFor(() => document.querySelector("#w-result .bad"));
    assert.match(await page.run(() => document.querySelector("#w-result .bad").textContent), /^That code didn't work\. Make a new one with the button above and paste it here\./);
    assert.equal(await page.run(() => document.querySelector("#w-result code").textContent), raw, "the engine's words are behind Details");
    assert.equal(await page.run(() => document.querySelector("#w-result details").open), false, "closed until it is opened");
    assert.deepEqual(await page.run(() => [W.step, W.busy, document.querySelector("#w-pw").value, document.activeElement.id]), ["guide", false, "wrong wrong wrong w", "w-pw"], "still here, the code is still there, the keyboard is in the box");
    assert.deepEqual(await calls(page), ["test_account"], "nothing was saved");
    assert.equal(await page.run(() => document.querySelector("[data-w=connect]").disabled), false, "Connect works again");
    await page.run(unwrapMock);
    await page.fill("#w-pw", "abcd efgh ijkl mnop");
    assert.equal(await page.run(() => document.querySelector("#w-result").textContent), "", "typing clears the old answer");
    await page.click("[data-w=connect]"); await page.waitFor(() => W.step === "ready");
  });

  it("while it connects, nothing can be pressed twice and Back is off", async page => {
    await toGuide(page, "me@gmail.com");
    await page.run(wrapMock, [{ cmd: "test_account", hold: true }]);
    await page.fill("#w-pw", "abcd efgh ijkl mnop");
    await page.click("[data-w=connect]");
    const busy = await page.run(() => ({ busy: W.busy, connect: document.querySelector("[data-w=connect]").disabled, back: document.querySelector("[data-w=back]").disabled, say: document.querySelector("#w-result").textContent }));
    assert.deepEqual([busy.busy, busy.connect, busy.back], [true, true, true]);
    assert.match(busy.say, /Connecting to Gmail\. This can take up to half a minute\./);
    await page.run(() => { veil.querySelector("[data-w=connect]").disabled = false; veil.querySelector("[data-w=connect]").click(); veil.querySelector("#w-pw").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })) });
    await release(page); await page.waitFor(() => W.step === "ready");
    assert.equal((await calls(page)).filter(c => c === "test_account").length, 1, "a second press while it was busy asked nothing");
  });

  it("Paste fills the code from what was copied and connects; if the copy cannot be read it says what to press", async page => {
    await toGuide(page, "me@gmail.com");
    await page.run(wrapMock, []);
    await page.run(() => { navigator.clipboard.readText = () => Promise.reject(new Error("denied")) });
    await page.click("[data-w=paste]");
    await page.waitFor(() => /Couldn't read what you copied/.test(document.querySelector("#w-result").textContent));
    assert.match(await page.run(() => document.querySelector("#w-result").textContent), /press (Ctrl|Command) and V together/);
    assert.equal(await page.run(() => W.step), "guide");
    await page.run(() => { navigator.clipboard.readText = () => Promise.resolve("  abcd efgh ijkl mnop \n") });
    await page.click("[data-w=paste]");
    await page.waitFor(() => W.step === "ready");
    assert.equal(await page.run(() => M.settings.account.user), "me@gmail.com", "the pasted code was used to save the mailbox");
    assert.deepEqual(await page.run(() => window.__calls.filter(c => c.cmd === "test_account" || c.cmd === "save_account").map(c => c.a.password)), ["abcd efgh ijkl mnop", "abcd efgh ijkl mnop"], "what was copied is passed on without the blanks and the line end around it");
  });

  it("the code is hidden unless asked, and showing it keeps what was typed", async page => {
    await toGuide(page, "me@gmail.com");
    await page.fill("#w-pw", "abcd efgh ijkl mnop");
    assert.equal(await page.run(() => document.querySelector("#w-pw").type), "password");
    await page.click("[data-c=show]");
    assert.deepEqual(await page.run(() => [document.querySelector("#w-pw").type, document.querySelector("#w-pw").value, document.activeElement.dataset.c]), ["text", "abcd efgh ijkl mnop", "show"]);
    await page.click("[data-c=show]");
    assert.equal(await page.run(() => document.querySelector("#w-pw").type), "password");
  });

  it("an Outlook, Hotmail or Live address gets an honest screen with no code box; by hand is still possible", async page => {
    await toGuide(page, "me@hotmail.com");
    assert.deepEqual(await page.run(() => [W.step, !!document.querySelector("#w-pw"), document.querySelector("h2").textContent]), ["outlook", false, "Outlook, Hotmail and Live"]);
    assert.match(await page.run(() => veil.textContent), /cannot read this mailbox yet/);
    await page.click("[data-w=manual]"); await page.waitFor(() => W.step === "other");
    assert.deepEqual(await page.run(() => [document.querySelector("#w-host").value, document.querySelector("#w-port").value]), ["outlook.office365.com", "993"]);
  });

  it("an address the app does not know gets the plain server form with the server guessed, and typed text is shown as text", async page => {
    await toGuide(page, "me@example.org");
    assert.equal(await page.run(() => W.step), "other");
    assert.deepEqual(await page.run(() => [document.querySelector("#w-host").value, document.querySelector("#w-port").value]), ["imap.example.org", "993"]);
    assert.match(await page.run(() => veil.querySelector(".panel p:nth-of-type(1)").textContent), /doesn't know example\.org yet/);
    await page.click("[data-w=back]"); await page.waitFor(() => W.step === "address");
    await page.fill("#w-user", `me@a'b&"c.org`);
    assert.equal(await page.run(() => document.querySelector("[data-w=next]").disabled), true, "a quote in the domain is not an address");
    // "&lt" with no semicolon is read as "<" by an HTML parser: drawn without escaping it would turn into a character that is not in the address
    await page.fill("#w-user", "me@a'b&ltc.org"); await page.click("[data-w=next]"); await page.waitFor(() => W.step === "other");
    assert.match(await page.run(() => veil.querySelector(".panel p:nth-of-type(1)").textContent), /doesn't know a'b&ltc\.org yet/);
    assert.equal(await page.run(() => !!veil.querySelector(".panel img, .panel script")), false);
  });

  it("an unknown server is connected with the server and port that were typed", async page => {
    await toGuide(page, "me@example.org");
    await page.run(wrapMock, []);
    await page.fill("#w-host", "mail.example.org"); await page.fill("#w-port", "1993"); await page.fill("#w-pw", "  secret  ");
    await page.click("[data-w=connect]"); await page.waitFor(() => W.step === "ready");
    assert.deepEqual(await page.run(() => { const c = window.__calls.find(x => x.cmd === "save_account").a; return [c.host, c.port, c.user, c.password] }), ["mail.example.org", 1993, "me@example.org", "secret"], "typed with blanks around it, passed on without them");
  });

  it("Back goes one screen back and keeps what was typed; a new address starts the code again", async page => {
    await toGuide(page, "me@gmail.com");
    await page.fill("#w-pw", "abcd efgh ijkl mnop");
    await page.click("[data-w=back]"); await page.waitFor(() => W.step === "address");
    assert.equal(await page.run(() => document.querySelector("#w-user").value), "me@gmail.com");
    await page.click("[data-w=next]"); await page.waitFor(() => W.step === "guide");
    assert.equal(await page.run(() => document.querySelector("#w-pw").value), "abcd efgh ijkl mnop", "the same address keeps its code");
    await page.click("[data-w=back]"); await page.waitFor(() => W.step === "address");
    await page.fill("#w-user", "me@yahoo.com"); await page.click("[data-w=next]"); await page.waitFor(() => W.step === "guide");
    assert.equal(await page.run(() => document.querySelector("#w-pw").value), "", "a different address does not inherit another provider's code");
    await page.click("[data-w=back]"); await page.click("[data-w=back]"); await page.waitFor(() => W.step === "welcome");
    assert.equal(await page.run(() => !!document.querySelector("[data-w=back]")), false, "there is nothing before Welcome");
  });

  it("every page it can open is one the app opens, and anything else is refused", async page => {
    const pages = await page.run(() => Object.values(PROVIDERS).map(p => p.page).filter(Boolean));
    assert.ok(pages.length >= 4, "Gmail, iCloud, Yahoo and Fastmail each have a page");
    for (const url of pages) assert.equal(await page.run(u => rpc("open_page", { url: u }).then(() => "opened", e => String(e)), url), "opened", url);
    const before = await page.run(() => window.__opened.length);
    for (const url of ["https://evil.example/", "http://myaccount.google.com/", "https://myaccount.google.com.evil.example/", "javascript:alert(1)", "https://myaccount.google.com@evil.example/", ""]) {
      assert.match(await page.run(u => rpc("open_page", { url: u }).then(() => "opened", e => String(e)), url), /can't be opened from here/, url);
    }
    assert.equal(await page.run(() => window.__opened.length), before, "a refused address opens nothing");
  });

  it("if the browser cannot be opened it says the address to type", async page => {
    await toGuide(page, "me@gmail.com");
    await page.run(wrapMock, [{ cmd: "open_page", fail: "Couldn't open your browser: no default browser" }]);
    await page.click("[data-w=open]");
    await page.waitFor(() => document.querySelector("#w-result .bad"));
    assert.match(await page.run(() => document.querySelector("#w-result .bad").textContent), /Open it yourself and go to myaccount\.google\.com\/apppasswords/);
  });

  it("the last screen turns starting with the computer off when asked, and says the app keeps running", async page => {
    await toGuide(page, "me@gmail.com");
    await page.fill("#w-pw", "abcd efgh ijkl mnop"); await page.click("[data-w=connect]"); await page.waitFor(() => W.step === "ready");
    assert.match(await page.run(() => veil.textContent), /keeps running\. Its icon near the clock brings it back/);
    assert.equal(await page.run(() => document.querySelector("[data-c=autostart]").checked), true, "on by default, and the sentence is on the screen");
    await page.click("[data-c=autostart]");
    assert.equal(await page.run(() => W.autostart), false);
  });
});

suite("the Add a mailbox dialog in Settings uses the same sentences and the same button", it => {
  const addDialog = async page => { await openSettings(page); await page.click("[data-s=addacct]"); await page.waitFor(() => document.activeElement.id === "m-user") };

  it("a refused password is a sentence, with the server's words behind Details", async page => {
    await addDialog(page);
    const raw = "sign-in refused: [AUTHENTICATIONFAILED] Invalid credentials (Failure)";
    await page.run(wrapMock, [{ cmd: "test_account", fail: raw }]);
    await page.fill("#m-user", "new@example.com"); await page.fill("#m-pw", "wrong");
    await page.click("#m-test");
    await page.waitFor(() => document.querySelector("#mtestres .bad"));
    assert.match(await page.run(() => document.querySelector("#mtestres .bad").textContent), /^That code didn't work\./);
    assert.equal(await page.run(() => document.querySelector("details.why code").textContent), raw);
    assert.equal(await page.run(() => document.querySelector("#m-add").disabled), true, "nothing can be added after a refusal");
  });

  it("a refusal when the mailbox is added is a sentence too, and nothing is added", async page => {
    await addDialog(page);
    const raw = "No such host is known. (os error 11001)";
    await page.fill("#m-user", "new@example.com"); await page.fill("#m-pw", "secret");
    await page.click("#m-test"); await page.waitFor(() => document.querySelector("#mtestres .ok"));
    await page.run(wrapMock, [{ cmd: "add_account", fail: raw }]);
    await page.click("#m-add");
    await page.waitFor(() => document.querySelector("#mtestres .bad"));
    assert.match(await page.run(() => document.querySelector("#mtestres .bad").textContent), /^Couldn't reach Gmail\. Check that this computer is online/);
    assert.equal(await page.run(() => document.querySelector("details.why code").textContent), raw);
  });

  it("the provider's steps have a button that opens the page where the code is made", async page => {
    await addDialog(page);
    assert.match(await page.run(() => document.querySelector("[data-m=open]").textContent), /Open Google's page/);
    await page.click("[data-m=open]");
    await page.waitFor(() => (window.__opened || []).length === 1);
    assert.deepEqual(await page.run(() => window.__opened), ["https://myaccount.google.com/apppasswords"]);
    await page.run(() => { const s = document.querySelector("#m-provider"); s.value = "other"; s.dispatchEvent(new Event("change", { bubbles: true })) });
    await page.waitFor(() => document.querySelector("#m-port"));
    assert.equal(await page.run(() => !!document.querySelector("[data-m=open]")), false, "a server the app does not know has no page to open");
  });
});
