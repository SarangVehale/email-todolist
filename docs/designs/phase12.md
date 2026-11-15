# Phase 12: simple enough for a grandmother

> **Status:** accepted (Run 10, 2026-11-15): all four steps are built and the gates and the release rehearsal are done ([the review record](../2026-11-15_PHASE12_REVIEW.md)); the test with a real grandmother is the owner's · **Audience:** maintainers · **Last reviewed:** 2026-11-15

The owner's goal: "The entire app should be stupidly simple to use. My grandma should get it up and running." Three council seats
(Opus, Sonnet, Fable) read the app as an 80-year-old on a Windows laptop with a Gmail, Outlook, Yahoo or iCloud address would meet it, blind to
each other. They agreed on almost everything; this is what they said and what is built.

## What the seats agreed on

| Where she gives up today | Where it is | The fix |
| --- | --- | --- |
| The installer says "Windows protected your PC" | The build is unsigned | A guide in words (`docs/guides/install-and-the-windows-warning.md`: pictures are the owner's to take on a real machine); free signing (SignPath Foundation) is an application only the owner can make |
| A form with a provider menu and "App password", help that says "2-Step Verification" and "IMAP" with no link | `app/ui/setup.js` step 2 | Type the email address; the provider is read from it; one screen per provider with a button that opens the right page and three plain steps |
| "Test connection" must be found and pressed, and failures print `sign-in refused: …` | `setup.js` | The test runs by itself when the code is pasted; errors are sentences with one next step; the raw text is behind "Details" |
| Outlook and Hotmail: "(not supported yet)" | `setup.js` | Microsoft sign-in (free for personal accounts), shown when the owner has set a client ID; until then a friendly full sentence before she has spent any effort |
| Steps 3 to 5 ask about Ollama, a Claude key, ntfy, a phone hub | `setup.js` | Gone from the first-run screens; setup makes sure that filing and the phone hub are off, and leaves ranking and alerts as they are (rules and desktop alerts in a new install) |
| A dense black page of 11-pixel uppercase labels, five dots, a rail and a footer of key caps | `style.css`, `main.js` | Easy mode: large text, light theme, plain headings, a big **Done** and **Later** on each card; the dense look is one switch away ("Simple view" in Settings) |
| Closing the window seems to quit the app | `app/src-tauri/src/lib.rs` | A sentence on the last setup screen: it keeps watching, the icon near the clock opens it (a sentence on the first real close is not built: the shell would have to show it) |

## Decisions

* **Easy mode is the default for a new install; an existing install keeps what it has.** The choice is per device (`localStorage`: `ia.easy`,
  `ia.size`, `ia.theme`), so a phone and a laptop can differ, and the "Simple view" switch in Settings turns the dense look back on.
  Nothing is removed from the engine: every feature stays; Easy mode hides it.
* **Sign-in.** Gmail has two paths: the guided app-password trip (works for everyone, free) and **Sign in with Google** (one click, no code),
  which exists only when a Google client ID is set in the build. Google's `https://mail.google.com/` scope is *restricted*: an unverified app
  is limited to 100 users, shows "Google hasn't verified this app" (she clicks Advanced, then Continue) and, while the project is in
  Testing, loses refresh tokens after 7 days; removing all that needs Google's verification and a yearly security assessment that costs
  money, which the owner has ruled out. So: Sign in with Google is for family and friends (up to 100), with the project set to **In
  production, unverified**, never Testing; the app password path stays the default and the honest answer for anyone else. **Sign in with
  Microsoft** (Outlook, Hotmail, Live) needs a free Entra app registration (a public client, no secret, scopes `IMAP.AccessAsUser.All` and
  `offline_access`) and uses the device-code flow: the app shows a short code and opens the page. Both buttons are hidden until the owner
  puts a client ID in `engine/src/oauth.rs` (`GOOGLE_CLIENT_ID`, `MICROSOFT_CLIENT_ID`), and the documents say how.
* **Several mailboxes already exist** (up to five, Settings, "Add another mailbox"). They get the same simple flow, and each mailbox says
  how it signs in (a password or an account).
* **No tokens in a log, a backup or a URL an error could echo.** The refresh token lives in the system keychain beside the app passwords;
  the access token only in memory; the loopback listener and every other point of the Phase 3 checklist (`designs/phase3.md`, section 3) stay.
* **Plain words.** App password becomes "the 16-letter code", snooze "Later", mute "Stop showing mail from them"; the engine's error text is
  mapped to sentences in the window's setup screens (the engine keeps its precise messages: they are the log's and the tests'; the status line and the mailbox rows of the simple view show a sentence too: `plainStatus` in `setup.js`; the dense view keeps the engine's text).

## What is built, in order

1. **Easy setup (built).** Welcome (what it does and does not do, in three sentences); the email address; the provider guide (a button that opens the
   provider's page, three steps, a large paste box and a **Paste** button that takes the clipboard); one **Connect** that tests, saves and finishes; a
   last screen with **Open my list**. Filing and the phone hub are made sure to be off (ranking and alerts are left as they are), and a sentence says what starting with Windows does and that the app
   keeps running when the window is closed. *Tests:* the whole flow in the window suite; the provider is read from the address (a table test); errors map to
   sentences (a table test); the pages the window opens are checked against the engine's list (an engine test).
   *What differs from the first plan:* there is no separate "Test" step and no auto-test on typing (Connect, or Paste, is the test); the Add a mailbox
   dialog in Settings keeps its single form and gains the same button and sentences, not the three screens (a second mailbox is rare); the one-time
   sentence about the window closing is on the last setup screen, not on the first real close (the shell would have to show it: that is left for a
   later change).
2. **Easy look (built; the guide is `guides/the-simple-view.md`).** Text size (Normal, Large, Huge; Large by default), a light theme (default in Easy mode), plain section names, a big **Done**
   and **Later** on every card, the dense page's dots, rail, counters and key hints hidden, an **Undo** that stays until the next action, a
   first-run card, "Open in Gmail" for a Gmail message (other providers have no address the app can build), a "last read at" line, and a
   Settings page of five rows with "More options" under it. *Tests:* the accessibility checks (contrast, names, targets) run on five combinations of
   view, theme and size and on every first-run screen; the hostile-text test runs in the simple view too.
3. **Sign in with Google and with Microsoft (built, and off until the owner registers the app: [ADR-0015](../adr/0015-sign-in-with-google-and-microsoft-needs-the-owners-client-id.md), [the runbook](../runbooks/register-sign-in.md)).** XOAUTH2 in the IMAP client, a token store with one refresher, the loopback (Google) and
   device-code (Microsoft) flows, a mailbox that says how it signs in, sign-out that removes the right keychain entries. *Tests:* a scripted
   token server and a scripted IMAP server that accepts XOAUTH2 (success, a rejected token that is refreshed once, a revoked grant, a flow
   with the wrong state, a stalled or oversized callback), and the Phase 3 checklist (one item, keeping the server's error challenge, is not done: the server's refusal text comes back as the error, which is what the challenge says).
4. **Gates and a rehearsal of the release workflow (done).** A three-seat council, two drift passes and three rounds of the Opus loop; the workflow rehearsed on `v0.1.0-rc.6`, which passed. (The `open` crate is a new dependency of the engine; the shell's permissions are unchanged.)

## Out of scope

A shared Google client ID for strangers (it needs the paid assessment), free code signing (an application the owner makes), automatic
updates (needs signing keys and a repository setting), reply-in-app (the app never sends), a read-aloud mode, a "helper" mode that locks Settings,
and localisation. They are listed because the seats raised them; the first is the one to do next.

## Test plan

Each step is test-first and each new guard is put back to its old behaviour once, as in every phase. The three seats' last recommendation, a test
with a real grandmother (unaided from the download to the list; three days of use; reading the list at the default size), cannot be done by this
repository: it is the owner's, and the record says it has not happened.
