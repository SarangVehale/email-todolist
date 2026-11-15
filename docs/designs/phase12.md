# Phase 12: simple enough for a grandmother

> **Status:** draft (Run 10, 2026-11-15) · **Audience:** maintainers · **Last reviewed:** 2026-11-15

The owner's goal: "The entire app should be stupidly simple to use. My grandma should get it up and running." Three council seats
(Opus, Sonnet, Fable) read the app as an 80-year-old on a Windows laptop with a Gmail, Outlook, Yahoo or iCloud address would meet it, blind to
each other. They agreed on almost everything; this is what they said and what is built.

## What the seats agreed on

| Where she gives up today | Where it is | The fix |
| --- | --- | --- |
| The installer says "Windows protected your PC" | The build is unsigned | A picture guide next to the download and in the welcome screen now; free signing (SignPath Foundation) is an application only the owner can make |
| A form with a provider menu and "App password", help that says "2-Step Verification" and "IMAP" with no link | `app/ui/setup.js` step 2 | Type the email address; the provider is read from it; one screen per provider with a button that opens the right page and three plain steps |
| "Test connection" must be found and pressed, and failures print `sign-in refused: …` | `setup.js` | The test runs by itself when the code is pasted; errors are sentences with one next step; the raw text is behind "Details" |
| Outlook and Hotmail: "(not supported yet)" | `setup.js` | Microsoft sign-in (free for personal accounts), shown when the owner has set a client ID; until then a friendly full sentence before she has spent any effort |
| Steps 3 to 5 ask about Ollama, a Claude key, ntfy, a phone hub | `setup.js` | Gone from the wizard; safe defaults (rules only, desktop alerts on, filing off, phone off) |
| A dense black page of 11-pixel uppercase labels, five dots, a rail and a footer of key caps | `style.css`, `main.js` | Easy mode: large text, light theme, plain headings, a big **Done** and **Later** on each card; the dense look stays as "Show everything" |
| Closing the window seems to quit the app | `app/src-tauri/src/lib.rs` | A sentence the first time: it keeps watching, the icon near the clock opens it |

## Decisions

* **Easy mode is the default for a new install; an existing install keeps what it has.** The choice is per device (`localStorage`: `ia.easy`,
  `ia.size`, `ia.theme`), so a phone and a laptop can differ, and "Show everything" in Settings turns the dense look back on.
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
  mapped to sentences in the window (the engine keeps its precise messages: they are the log's and the tests').

## What is built, in order

1. **Easy setup.** Welcome (what it does and does not do, in three sentences); the email address; the provider guide (a button that opens the
   provider's page, three steps, "I have the code" and a large paste box that takes the clipboard); the test runs by itself; **Open my list**.
   Alerts on, filing off, phone off, rules only, and a sentence about starting with Windows. *Tests:* the whole flow in the window suite;
   the provider is read from the address (a table test); errors map to sentences (a table test).
2. **Easy look.** Text size (Normal, Large, Huge; Large by default), a light theme (default in Easy mode), plain section names, a big **Done**
   and **Later** on every card, the dense page's dots, rail, counters and key hints hidden, an **Undo** that stays until the next action, a
   first-run card, "Open this email" in the mail's own website, a sentence on the first close of the window, a "last checked" line, and a
   Settings page of five rows with "More options" under it. *Tests:* the accessibility checks (contrast, names, targets) run in both
   themes and every size; the hostile-text test runs in Easy mode too.
3. **Sign in with Google and with Microsoft.** XOAUTH2 in the IMAP client, a token store with one refresher, the loopback (Google) and
   device-code (Microsoft) flows, a mailbox that says how it signs in, sign-out that removes the right keychain entries. *Tests:* a scripted
   token server and a scripted IMAP server that accepts XOAUTH2 (success, a rejected token that is refreshed once, a revoked grant, a flow
   with the wrong state, a hostile redirect), and the Phase 3 checklist item by item.
4. **Gates and a rehearsal of the release workflow** (the Tauri opener is a new dependency and a new permission).

## Out of scope

A shared Google client ID for strangers (it needs the paid assessment), free code signing (an application the owner makes), automatic
updates (needs signing keys and a repository setting), reply-in-app (the app never sends), a read-aloud mode, a "helper" mode that locks Settings,
and localisation. They are listed because the seats raised them; the first is the one to do next.

## Test plan

Each step is test-first and each new guard is put back to its old behaviour once, as in every phase. The three seats' last recommendation, a test
with a real grandmother (unaided from the download to the list; three days of use; reading the list at the default size), cannot be done by this
repository: it is the owner's, and the record says it has not happened.
