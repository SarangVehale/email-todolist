# Phase 12 review record, 2026-11-15

Run 10, which is not on the original roadmap: the owner asked for "direct Google login", for several accounts, and for an app that is "stupidly simple to use:
my grandma should get it up and running". Design: `docs/designs/phase12.md` and [ADR-0015](adr/0015-sign-in-with-google-and-microsoft-needs-the-owners-client-id.md).
Three gates, each by models other than the one that wrote the work. Every finding was checked against the code before anything changed.

## What was built

* **Several accounts** already existed (Phase 6, up to five mailboxes). Nothing new was needed; the owner was told.
* **Simple setup.** Welcome, the email address (the provider is read from the domain), a button that opens the page where the provider makes the app password
  (the new engine command `open_page`, an `https` and host allow-list that the phone hub cannot call), a paste box and a **Paste** button, one **Connect** that
  tests, saves and finishes, and a last screen. Errors are sentences (`friendly()`), the server's words behind "Details for a helper". Outlook, Hotmail and
  unknown providers get an honest screen and a plain server form.
* **The simple view.** Cards with a big Done and Later, plain headings, an Undo that stays, large text and light colours per device, a simple Settings page
  with "More options", and `state.checked_at` for "Last read at".
* **Sign in with Google (loopback and PKCE) and Microsoft (device code)** in `engine/src/oauth.rs`, XOAUTH2 in `watcher.rs`, the refresh token in the keychain,
  the setting `oauth`, four commands. **Off until the owner puts a client ID in `oauth.rs`** ([the runbook](runbooks/register-sign-in.md)).

First commit `3434999` (503 engine tests, 113 window tests); the fixes below in the commit after it (512 engine tests, 121 window tests).
Mutation checks: every new guard was put back to its old behaviour once: 6 on the page allow-list and the hub, 3 on `checked_at`, 18 on the sign-in code, 15 on
the first-run screens, 17 on the simple view and 14 on the review fixes. Survivors were all explained: a second-`@` check that the domain pattern already makes
(removed), a filter-bar guard that the stylesheet also provides, and four guards that each had a second defence (the host pin and the removal-versus-refresh lock;
removed together they fail the test); three weak tests were strengthened (the password was not trimmed; a client-ID guard; a "last read" test that could not fail).

## Gate 1: council (three seats, blind to each other)

| Seat | Verdict | Findings |
| --- | --- | --- |
| Opus | ship with fixes | no blocker. The check of a new sign-in retried with the old saved token (hiding the server's refusal; when signing in again it approved the old grant); Cancel stayed on screen while the mailbox was being saved; the status line gave password advice to a signed-in mailbox; the sign-in was used up before the mailbox was checked; a cancelled Microsoft start left a stale code; "Last read at" went stale on a quiet mailbox |
| Sonnet | ship with fixes | no blocker. "Last read at" stale (and not in the window's snapshot); two tests assert the client-ID constants are empty, so the runbook's step 1 turned the suite red; the same retry flaw; the password advice; a stale poll loop could cancel a newer sign-in; setup reset a person's ranking and filing choices; the Add dialog still called Outlook unsupported; "Later" had no way back; a Microsoft refresh token may exceed Windows' keychain limit (unverified) |
| Fable | ship with fixes | no blocker. A token went to whatever host the settings file named; a signed-in mailbox had no way back to a code; "Open that page" was dead outside setup; wrong words for a refused account sign-in; the sign-in used up too early; a settings change during the server check was lost; a refresh could write a token back after removal; a Microsoft start without a generation check; the simple view depended on pressing "Open my list"; the welcome card overclaimed; every IMAP failure counted as a refusal; tests that cannot fail; the design overclaimed (first close, "Open this email") |

All of it that is not listed under "Not changed" below was true and is fixed.

## Gate 2: drift passes

| Pass | Result |
| --- | --- |
| Technical documents (Sonnet) | about 40 drifts: test and manual-case counts, the command reference (what `save_account`, `remove_account`, `finish_setup`, `sign_out`, `save_settings`, `state` and `open_page` do now), the settings reference (a table row outside its table; six references to wizard steps that no longer exist), the security model (the host claim, the outbound calls, missing rows), the design (a first-close sentence that was never built, "Show everything", a Tauri permission that never changed, a "hostile redirect" test that does not exist), an error placed under Errors that is a side effect, review dates |
| Promises in the guides (Haiku) | 14 reported, 14 real: a "Detect" button and a "Claude subscription" sentence that the window no longer has; wording that fits only the dense view (the status line, Undo's timing, the Add task button, "press x or the circle"); Settings paths that omit "More options"; the autostart switch that exists; "four sentences" for five; the password message on the server screen |

## Gate 3: sol-loop with Opus as the single reviewer

Round 1 (a ledger of the council's and the drift passes' findings was given up front): **CHANGES REQUESTED**, no blocker or major, six minor and three nits.

| Id | Severity | Outcome |
| --- | --- | --- |
| R1-1 | minor | **fixed**: a Microsoft refresh in flight while the person signed in again wrote the old grant's rotated token over the new one; it now writes back only if the keychain still holds the token it used (a test with a slow token server) |
| R1-2, R1-3 | minor | **fixed**: the old Google grant was revoked before the new one was saved (a failed save left the mailbox with no working credential) and the 5-second revoke sat between the settings re-read and the save (a change made meanwhile was lost); the revoke is now after the save, the keychain is put back when the settings cannot be saved, and the IMAP password is deleted only after the save (a test makes the settings file unwritable) |
| R1-4 | minor | **fixed**: the new access token was cached before the server accepted it (and under a number the mailbox might not get); it is cached only after the save |
| R1-5, R1-9 | minor, nit | **fixed**: the mailbox is planned before the sign-in is taken, and the taken sign-in must be the one that was planned (else it is cancelled at Google) |
| R1-6, R1-7 | minor, nit | **fixed**: the `oauth_finish` section of the command reference now says what is spent and what is not, and that revoking is Google only |
| R1-8 | nit | **fixed**: a Cancel pressed while the "done" answer was on its way no longer leads to a saved mailbox (a window test sends "done" after Cancel) |
| (test comment) | nit | **fixed**: a comment said the old grant "would pass"; it would show in the server's log |

Round 2 verifies these (below).

A mutant that revokes the old grant "even if nothing was saved" survived: the revoke is after the early return on a failed save, so the mutation changes nothing (equivalent); the
ordering is held by the test that makes the save fail and checks that the old grant was not revoked.

## A fault the CI found while this was being finished

CI run 43 failed once on macOS in the first-run test of the Enter key. It did not fail in six runs on this machine, and it failed once in two full runs, with the window's own state in the message: the
keyboard was on the **Open page** button, not in the code box. The cause is real and is not the test's: when a dialog opens, a 30-millisecond timer moves the keyboard to its first control; on a slow computer
that timer fired after the person had already reached a later screen and pulled the keyboard away. It now does nothing if the keyboard is already inside the dialog (a window test fires the timer late
and checks, and fails on the old code).

## Not changed, on purpose

* **The status line and the mailbox rows show the engine's text** (`error: …`) in the simple view, so a lost connection prints a system error. The design said the window maps
  these to sentences; it does so in the setup screens only. The design now says so. Mapping the status line is worth doing later, with its own tests.
* **A Microsoft refresh token may be longer than Windows' keychain accepts** (about 1,280 characters). Unverified; the save now says "Couldn't save the sign-in in your system
  keychain" and manual case MT-154 checks it with a real account.
* **The production guard on the test endpoints** (`oauth::use_test_endpoints` does nothing outside this crate's tests) cannot be tested from a test of this crate, which is
  where it is honoured. It is two lines; the test that was named for it was renamed to what it checks.
* **The server's XOAUTH2 error challenge** is read and answered but its text is not shown: the server's refusal comes back as the error, which says the same.

## What CI showed

| Run | Commit | Result |
| --- | --- | --- |
| 42 | `3434999` | Ubuntu and the audit green. Windows and macOS failed in two window tests that read the page's background colour one frame after a switch of the light theme (the browser had not yet shown it); the engine tests passed everywhere. The tests now wait for the colour. |
