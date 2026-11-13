# Phase 3: getting ready to ship

* **Date:** 2026-11-13
* **Status:** update check and diagnostics built; Google sign-in deferred. An independent review (Opus) found that the
  Google part is the riskiest and cannot be tested against Google from here, and listed what it needs; that list is kept
  below so it can be done as its own pass.

## Problem

Phases 1 and 2 made the engine safe and the list useful. Four things still stand between the app and real users:

1. **Gmail needs an app password.** That works today but is a barrier: it needs 2-step verification and a trip into Google
   account settings. Google sign-in would be easier, but a research pass found the real constraint: reading Gmail over IMAP
   needs the restricted scope `https://mail.google.com/`, and a public app using it must pass a paid yearly security
   assessment. The owner has no money for paid services, so a shared Google client ID is out. What is free and allowed:
   each user (or the owner for personal use) creates their own Google Cloud client ID and pastes it in.
2. **There is no way to learn that a new version exists.** Users would never hear about a security fix.
3. **Support is blind.** When something fails on a user's machine there is nothing safe to paste into a bug report.
4. **Releases are unsigned and there is no repository yet.**

## Constraints that do not change

No paid service of any kind. The app never sends mail. Data stays on the computer. App passwords stay the default for
everyone. Nothing here may add a server of ours.

## Design

### 1. Update check (built; off until a repository is set)

`UPDATE_REPO` in `engine/src/updates.rs` is empty today. While it is empty there is no network call and no setting shown. When
it names a repository (`owner/name`), the scheduler makes one attempt a day (an attempt, not a success, recorded in the
database, so a rate limit or a deleted repository is not retried all day and a settings save does not restart the clock):
`GET https://api.github.com/repos/<repo>/releases/latest` over HTTPS with no redirects, a 5-second timeout, a 256 KB cap
and a User-Agent of `InboxAgent/<version>`. Only the response's `tag_name` is used, and only when it is a plain version
(digits and dots, an optional short alphanumeric pre-release suffix; no slashes, spaces, `?`, `#` or `+`). The release
page address is built from the configured repository and the checked tag; the response's own `html_url` is never used
(a repository that is renamed or taken over cannot redirect users). The app shows "Version X is available" with a button
that copies the link, and "Not now". It never downloads or runs anything. Settings has a switch; phones are never shown the
notice. Review changes: the first draft accepted `html_url` after a prefix check (which let `owner/name-evil` through) and
counted successes, not attempts.

### 2. Diagnostics (built)

A desktop-only command and a "Copy diagnostics" button. The text has the version, system, status, counts, option names and
the mail server's host name, plus the last 30 activity lines and the last 20 crash-log lines, both redacted. Redaction (a
review found the first version leaked): everything from the first opening quote to the **last** closing quote (curly, straight
and backtick, because Rust panic messages quote strings in backticks) becomes an ellipsis; anything around an `@` becomes
`<address>`; from the first path under a home folder to the end of the line becomes `<path>` (folder names can contain
spaces); the text after an error colon, inside "connection lost (...)" and the login name in "connected as" are removed
unless they are one of a few fixed phrases; the line is cut to 200 characters. Crash lines keep only the time, thread and
file:line; the panic text is dropped.

### 3. Deferred: Google sign-in with your own client ID

The review's list of what a safe version needs, kept for the next pass:

* Send the token only to `imap.gmail.com:993`, ignoring the account host setting; `is_gmail` must be an exact or dot-boundary
  match (done now); settings patches must not touch the account (done now).
* Branch on an `auth` field when loading the account (today "no password" would be reported for a Google account); delete
  the right keychain entries on sign-out and when switching between password and Google.
* One process-wide token provider behind a mutex that refreshes one request at a time and remembers the time of an
  `invalid_grant`, so the watcher (30 s), write-back (15 s) and settings restarts cannot hammer Google.
* Revoke with the token in a POST body, never in a URL that an error message could echo into the activity log (the hub
  can read the log); log only error codes from a fixed list.
* A loopback listener that keeps accepting until the right state arrives (browsers open idle spare connections), with
  per-connection 5-second and 8 KB limits, 404 for anything else, a fixed page, `127.0.0.1` and never `localhost`, one flow
  at a time.
* XOAUTH2: keep the server's error challenge, drop the cached token and retry once on a rejected login.
* Sign-in realities: a Desktop client always has a secret and Google requires it; a Web client ID fails with
  `redirect_uri_mismatch`; a project in Testing needs the user added as a test user and loses refresh tokens after 7 days;
  users can untick the mail permission (check the returned scope and refresh token); `login_hint` for the right account.
* Open the browser from Rust after checking the address starts with `https://accounts.google.com/`.

## Out of scope

A shared Google client ID, Microsoft sign-in, automatic download and install of updates (needs signing keys and a repository),
telemetry of any kind.

## Test plan

Each item has a failing test first. OAuth runs against a local mock token server and the mock IMAP server (extended with
AUTHENTICATE). The update check runs against a local mock HTTP server with the base URL injectable. Diagnostics is a unit
test over a populated store, checking that no address, subject or secret appears in the output.
