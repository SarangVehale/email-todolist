# Security

This document records the security review done before the first release, what was fixed, what is
tested, and what remains. It is a snapshot, not a certification: it was a code-level review plus
automated checks, not a third-party penetration test.

## Trust boundaries

| Boundary | Who controls it | What flows across |
| --- | --- | --- |
| Received email (every header and body) | **Attackers** | SQLite, the UI, IMAP search commands, model prompts, desktop and ntfy alerts |
| Model output | Untrusted | Priority, category, action text, "urgent" flag |
| Phone hub (HTTP on the LAN) | Anyone on the network | Token-protected commands, static UI files |
| Settings (hosts, URLs, topic, ports) | The user, via the desktop window only | URLs, headers and IMAP connections |
| Desktop window (Tauri IPC) | Local UI only, strict CSP | The single `rpc` command |
| Keychain, `settings.json`, `agent.db` | The local user | Mail password, API key (keychain only), access code, task text |

## Findings and fixes (verified with a failing test first, unless noted)

| # | Severity | Finding | Fix | Regression test |
| --- | --- | --- | --- | --- |
| 1 | High | **IMAP command injection**: a crafted `Message-ID` using RFC 2047 encoding decoded to real CR/LF and reached the write-back `SEARCH` command | Message-ID is kept only if it is a plain `<visible-ASCII>` token (<= 200 bytes, no quotes or backslashes); otherwise dropped | `message_id_with_encoded_crlf_...`, `message_id_accepts_only_a_plain_angle_addr` |
| 2 | High | **Mass modification**: IMAP `HEADER` search is a substring match, so a short or forged Message-ID could match many messages and archive, label or star them all | Every hit's real header is re-read; only exact matches (max 3) are acted on; >50 hits or no exact match means no action | `substring_search_hits_are_filtered_to_exact_message_ids` |
| 3 | Medium | **Hub denial of service**: the HTTP library had no timeouts, so one stalled client could freeze the hub | Replaced with a small purpose-built server: 5 s I/O timeout, 16 KB header and 64 KB body caps, 32 concurrent connections, one request per connection | `stalled_clients_cannot_block_other_clients`, `oversized_and_malformed_requests_are_refused` |
| 4 | Medium | **Hub died after any settings save** (port still held by the previous thread) | Rebinds with retry; stop flag honoured | `hub_survives_repeated_settings_saves` |
| 5 | Medium | **Spoofed sender** gained "known person" trust and a high-priority alert | A failed DKIM, SPF or DMARC result in `Authentication-Results` cancels sender and reply trust. Model prompts defang `<` and `>` in email text so it cannot close the untrusted wrapper | `failed_dkim_or_spf_cancels_known_sender_trust`, `email_text_cannot_close_the_untrusted_wrapper...` |
| 6 | Medium | **Crash on hostile MIME**: deeply nested multipart overflowed the stack and aborted the whole process (reproduced) | Refuse messages with an absurd number of MIME containers, parse on a 64 MB-stack thread, cap text recursion, skip messages over 5 MB | `deeply_nested_multipart_does_not_crash_the_process` |
| 7 | Low | **Access code handling**: code stayed in the address bar, no brute-force limit, no clickjacking or Referer protection, public state exposed account and alert topic, no rotation | Code stripped from the URL after first load; 10 failures per minute locks an address out; `X-Frame-Options`, `Referrer-Policy`, `nosniff`, CSP and `no-store` on every response; the hub sees only `onboarded` and `writeback`; code rotates on disable and sign-out, plus a "Rotate access code" button | `guessing_tokens_gets_an_address_locked_out`, `responses_carry_hardening_headers`, `rotating_the_token_kills_the_old_one`, `public_state_leaks_neither...` |
| 8 | Low | **Data at rest**: no retention; a changed `UIDVALIDITY` could lose mail | Finished tasks pruned after 90 days, log after 30; when the server's `UIDVALIDITY` changes the mailbox starts over (the stored tasks, drafts and queued changes go, because the old numbers now point at different messages) | `prune_drops_old_done_tasks...`, `changed_uidvalidity_starts_the_mailbox_over...` |
| 9 | Info | CSP allowed inline scripts; settings fields were not validated; model calls followed redirects | UI split into files, `script-src 'self'` in the app and the hub; settings validated before every save (host, ports, ntfy topic and server, model names, URLs); model and alert HTTP calls never follow redirects | `settings_reject_values_that_could_inject_into_urls_or_commands`, `defaults_are_valid` |
| 10 | High | **Stored XSS via model output**: the model's `category` reached an HTML attribute (the filter chips) unescaped, so a crafted email could have tried to inject markup | The engine keeps only the eight known categories (anything else becomes `other`); the UI also escapes the attribute | `unknown_or_hostile_categories_become_other` |
| 11 | Info | **Reply drafts** are new attacker-adjacent output: header injection when saving to the Drafts folder, prompt injection through the email being replied to, runaway AI cost | Draft messages are built by one function that rejects invalid addresses, strips control characters from every header, encodes non-ASCII subjects, base64-encodes the body, and keeps only clean Message-IDs in threading headers. Email text, sender and the user's own instruction are defanged in the prompt. Auto-drafting has a daily cap, a minimum priority, needs an AI model, and discards template fallbacks. **No code in the app can send mail**: a draft only ever reaches the Drafts folder, by an explicit click, and only when mailbox write-back is on | `draft_message_headers_cannot_be_injected`, `draft_message_round_trips_...`, `the_draft_prompt_defangs_...`, `task_commands_work_through_the_core` |
| 12 | Info | **Phone hub command surface** grew (snooze, sender preference, draft editing) | One explicit allow-list (`commands::PUBLIC`); account, settings, secrets, export and token commands stay desktop-only, checked by a test that walks both lists | `the_phone_may_use_task_commands_but_not_settings_or_secrets` |

### Second review (reply drafts, snooze, preferences, scheduler)

An independent read-only reviewer examined the draft features after they were built. It found no critical or high
issues and confirmed that no code in the app can send mail and that the saved draft message cannot be header-injected.
Every finding was checked against the code first; these were confirmed and fixed, each with a regression test in
`engine/tests/review_fixes.rs`:

| # | Severity | Finding | Fix |
| --- | --- | --- | --- |
| 13 | Medium | The draft's recipient is attacker-chosen: a spoofed message with a `Reply-To` pointing elsewhere produced a draft addressed there, with no warning | Authentication status is stored. A draft shows a red warning, and "Save to Drafts" needs a second click, when the Reply-To domain differs from the sender's or the message failed DKIM/SPF/DMARC |
| 14 | Medium | A stolen phone access code could trigger paid AI calls, overwrite drafts and write to the user's Drafts folder | `draft_generate`, `draft_to_mailbox` and export are desktop-only. The phone can still read, edit, copy and discard drafts |
| 15 | Medium | "Always important" sender preference lifted spoofed messages to P4 and raised an alert | Preferences are skipped when the message failed its sender check; muted senders are never offered a reply |
| 16 | Low | The automatic-draft limit could be overshot by concurrent drafting, and discarding or editing a draft refunded the budget | One draft is written at a time, and the daily count lives in its own counter that edits cannot reduce |
| 17 | Low | Rewrite silently replaced the user's own edits | Confirmation is required and the engine refuses without an explicit `force` |
| 18 | Low | Drafts could contain zero-width or bidirectional-override characters, or links, that disguise text | Those characters are stripped; a draft that contains a link says so |
| 19 | Low | Sign-out left the log, corrections and sender preferences behind | Sign-out wipes everything the app stored |
| 20 | Low | Repeated clicks appended the same draft repeatedly; a login name that is not an address failed silently | One pending save per draft; a clear message when the login is not an email address |
| 21 | Low | The Markdown export copied hostile subjects and sender names verbatim (fake list items and links) | Control characters are dropped and Markdown characters escaped |
| 22 | Low | Pruning trusted the sender's `Date`, so a far-future date kept a message body on disk indefinitely | Pruning uses the time the task was completed |

Not changed on purpose: quiet hours drop non-urgent alerts instead of deferring them (the daily digest and the
snooze notification are not held back either), and after a settings change the old and new scheduler threads can
overlap for one 20-second tick.

### Robustness gaps closed during the edge-case pass

| Gap | Fix |
| --- | --- |
| HTML-only mail (most modern mail) had no readable text, so ranking and drafts were blind | HTML is rendered to text: script, style, title and comments removed, block tags become line breaks, entities decoded; malformed markup degrades to text and never errors |
| Model output wrapped in prose, fences, or with a second object, or a priority written as `4.0` or `"4"`, was rejected | The first JSON object that has a priority is used; numbers and flags are read leniently; everything else still falls back to the rules |
| A message with no usable Message-ID silently produced no mailbox change | The user is told why in the log; no guess is made |
| Snooze times inside a daylight-saving gap or overlap | The earliest valid local time is used |
| The `Authorization` scheme name was case-sensitive | `bearer` in any case is accepted (RFC 7235); other schemes are refused |
| Microsoft accounts looked supported | Microsoft turned off password sign-in for IMAP, so the entry now says it is not supported yet |

## Checked and found sound

Exact-match routes only (no filesystem access, path traversal tested); bound SQL parameters
everywhere; constant-time token comparison; 32-character access code from a CSPRNG; hub commands
limited to a short allow-list (twelve today, plus reading the list, none of which can reach the account, settings, secrets or the AI); secrets in the OS keychain only; the API key is
sent only to a fixed Anthropic HTTPS URL; TLS to the mail server with hostname verification;
ntfy title restricted to ASCII; all UI sinks escape `& < > " '`.

## Dependency scan

`cargo audit` (RustSec, 1,471 advisories): the one vulnerability (bundled SQLite, RUSTSEC-2026-0459)
was fixed by upgrading `rusqlite`. One warning remains: `lexical-core 0.7.6` (RUSTSEC-2023-0086,
soundness), pulled in by the IMAP parser. **Accepted** because it parses only TLS responses from the
user's own mail provider and the only newer IMAP release is an alpha with a different API. Revisit
when `imap` 3.x is stable. The IMAP parser also triggers a Rust future-incompatibility warning.

CI also scans the app shell (`app/src-tauri/Cargo.lock`: Tauri, the webview and plugins) and the npm tree. Today that
scan shows no vulnerabilities and five notices about unmaintained or unsound crates (`proc-macro-error`,
`zbus_names`, `zvariant`, `zvariant_derive`, and `glib` RUSTSEC-2024-0429) that only the Linux GTK/D-Bus build pulls
in; the Windows build does not use them. They wait on upstream Tauri releases.

## Residual risks and limits

- **The phone hub is plain HTTP.** On a shared Wi-Fi the access code can be sniffed. Use it on a
  trusted home network or over a private VPN (for example Tailscale). Rotating the code limits exposure.
- **Local data is not encrypted at rest.** `agent.db` holds subjects, 400-character excerpts, up to 3,000 characters of message text (kept only to write replies) and any reply drafts;
  `settings.json` holds the access code. They live in the user's profile; use full-disk encryption.
- **Email content goes to the chosen model.** Rules-only mode sends nothing. Ollama stays on the
  computer while its address is this computer's (the default; Settings accepts any http or https address, so pointing it at
  another machine sends the text there). An Anthropic key sends email text to Anthropic.
- **Authentication-Results is only as good as the receiving server.** If a provider adds none, trust
  falls back to the previous behaviour.
- **AI-written drafts can be wrong or persuasive.** They are always shown for review, never sent, and clearly labelled by source (AI, quick reply or edited).
- **Mail server behaviour is tested against a mock, not a real server.** `engine/tests/imap_flows.rs` runs the real sync and
  write-back code over the IMAP protocol against a scripted server (read-only fetches, exact-message targeting, label and flag
  changes, draft append, move/copy fallback, retries). Real servers can differ in details.
- **Network limits:** every mail connection has a 15-second connect timeout and 60-second read/write timeouts.
- **Not done:** a third-party penetration test, fuzzing, a test against a real mail server with
  write-back enabled, reproducible builds.
- **Releases are unsigned.** Windows SmartScreen and macOS Gatekeeper will warn until you add code
  signing certificates (see `.github/workflows/release.yml`).
- **CI actions are pinned to commit SHAs;** Dependabot proposes updates.

## Review 3 (independent Opus review, 2026-11-13): findings and fixes

Each fix has a regression test (`engine/tests/phase1.rs`, `features2.rs`, `imap_flows.rs`).

| Finding | Fix |
| --- | --- |
| A malformed `List-Unsubscribe` link (`https://[`) made the UI throw and blank the whole list | The engine accepts only links a browser can parse (real host, valid port, no user info); the UI never throws |
| `panic = "abort"` defeated the parser's panic guard; the same message was re-fetched after every restart | Unwinding restored; each message is marked before it is handled, a panic skips only that message, and a message that crashes the app twice is skipped |
| Quadratic HTML stripping (a 1 MB comment flood did not finish in 10 minutes) | One pass, linear time; HTML past 256 KB is ignored |
| A damaged database silently stopped the watcher | Startup integrity check; a damaged file is set aside as `agent.db.corrupt-<time>` and rebuilt; a schema version refuses data from a newer app; a `.bak` copy is kept before an upgrade |
| "Sign out" left mail text in free pages and the write-ahead log, and kept the API key | `secure_delete`, checkpoint and `VACUUM` on wipe; every keychain entry is deleted |
| Any keychain failure showed "no password" | A locked or missing keychain is reported as such |
| Mail data lived in Roaming AppData | Now in local AppData (`%LOCALAPPDATA%`), which does not sync to domain profile servers |
| The hub answered any network peer | It answers only loopback, private, link-local, shared-address (Tailscale) and IPv6 local peers |
| Panics left no trace in a build without a console | `crash.log` beside the database (panic text and location only, 100 KB cap); no telemetry |
| The app shell's dependencies were not scanned | CI audits both lockfiles |

A Sonnet re-review of these fixes found more, all fixed with tests:

| Finding | Fix |
| --- | --- |
| A sync waiting on the network could write mail back into a database that sign-out had just wiped | The sync checks the stop flag before every message and before every write |
| An ordinary database error (busy, disk full) was counted as a crash, and the third retry skipped the message for good | Only a real crash counts; an ordinary error clears the marker and the message is retried |
| A busy or locked database was treated as corrupt and renamed away; one failure stopped all background work | Only real corruption (a failed integrity check, "not a database") moves a file; a busy database is an error and the app keeps running |
| Backups, set-aside databases and `crash.log` survived "Sign out and reset" | All of them are deleted on sign-out |
| A forged `Date` year (such as 262143) panicked inside date handling | Dates outside 2000-2100 are treated as unknown |
| A panic outside the per-message guard ended the sync thread silently | A panic in any connection attempt becomes an ordinary error and goes through the retry path |
| Unsubscribe links to `localhost`, IP addresses or single-label hosts were accepted; links over 500 characters were refused | Public-looking hosts only; links up to 2,000 characters |
| CSS leaked into the text when the 256 KB cap cut a style block in half | An unfinished block at the cut is dropped |
| The integrity check ran on every settings save | Once per launch |

### Phase 2 design review (Opus, 2026-11-13, before any code)

The design was reviewed before it was built. Changes it forced:

| Risk found | Decision |
| --- | --- |
| Words like "URGENT action required" would give a phisher priority 5, the urgent flag, a quiet-hours bypass and a push alert | Rules never set the urgent flag; priority 5 needs a known sender, a question, a near due date and a passed sender check; message words count for nothing |
| Closing tasks whose message "left the inbox" would hide mail that was archived to snooze it or moved by a filter | Such tasks are only marked and shown in a fold; they stay open |
| An auto-closed task would close again after the user reopened it, and reopening would queue an unarchive that duplicates the mail | A `keep_open` flag; no mailbox change is ever queued by the automatic path |
| A server error, a reply with no UIDs, or a changed UIDVALIDITY could read as "everything is gone" | All of these mean unknown and change nothing; a batch vanishing while the inbox has mail is not believed |
| A forwarded message looks like a reply (it carries In-Reply-To) | Forwards do not count |
| Learning keyed on the display name, inflated by undo or by one keypress on a conversation, lets a sender be talked up | Keyed on the address; undo removes the correction; one correction per action; forged mail neither teaches nor learns; a shift never sets urgent; Normal resets |
| Thread keys that are empty or taken from a truncated list group unrelated mail or whole sets of tasks | Separate stored root; no key means `uid:<id>`; tasks you added never group |

The stored message text is now readable in the app through a desktop-only command (`read_body`). A phone with the access
code still gets the 400-character excerpt only.

A Sonnet review of the finished Phase 2 code found more, all fixed with tests:

| Finding | Fix |
| --- | --- |
| Replying to the newest message of a conversation closed the older, unanswered ones (References lists them all) | Only In-Reply-To marks a message as answered |
| Forwards written as WG:, TR:, RV: or "Fwd :" counted as replies | Forward prefixes in nine languages, spaces tolerated |
| Done, star and snooze on a conversation row reached messages hidden by a search or the Starred tab | The row remembers the messages it was grouped from, in the list actually shown |
| A conversation row hid facts about older messages (urgent, due, draft, needs a reply) and could fold or mark "left your inbox" a conversation that still had live mail | The row shows what is true of any message; folds need every message to qualify |
| Clicking a priority dot on a conversation taught the opposite direction | The dots show the newest message's own priority |
| Nudging one mail up twice, or walking it 3, 5, 4, 3, counted as several corrections | One correction per task (first priority to latest), removed when they match again |
| A correction could match a different sender's mail after the server renumbered | Matched by task and address; renumbering keeps what was learned but clears the task link; a new account forgets it all |
| Any "due today" in a message lifted a promotion above real mail | A date lifts a row only for someone who writes to you and is waiting on an answer, never bulk mail |
| A reopened task could be closed by the app again after a stray close and undo | `keep_open` is only ever set, never cleared |
| A forged message naming a conversation in References woke it from snooze | Mail that failed its sender check cannot wake a snooze |
| Waking a conversation sent one notification per message | One per conversation |
| The IMAP library clears the read time limit after every IDLE, so a silent server could hang the sync forever | The 60-second limit is restored after each IDLE |
| The extra Sent and inbox checks ran on every inbox change | At most every 20 seconds after a change, and every two minutes when idle |
| Keyboard: Enter on a row's buttons opened the row instead, and editing a date by keyboard dropped focus | Enter and Space belong to a focused button; focus returns to the field after a change |

Known and accepted: a conversation whose replies carry only In-Reply-To (no References) can split into more than one row;
tasks older than the newest 500 are not checked for replies; mail you sent to yourself comes back as a task (some people use
that as a reminder); a mail returned to the inbox after being archived through write-back arrives under a new UID and can
appear as a second task.

### Phase 3 design review (Opus, 2026-11-13, before any code)

| Risk found | Decision |
| --- | --- |
| Google sign-in would send a bearer token for the whole mailbox to whatever host the account setting names; `is_gmail` was a suffix match (`evilgmail.com` counted); a settings patch could change the account and skip the data reset | `is_gmail` is now an exact or dot-boundary match; a settings patch cannot touch the account (only `save_account`, which resets the old data, can); Google sign-in itself is deferred with a written list of what it needs |
| Diagnostics redaction leaked: subjects containing the closing quote, backticks in panic text, errors with no quotes, login names with no `@`, home folders with spaces | Redaction rewritten (first to last quote, backticks, error details cut, paths to end of line); crash lines keep only time, thread and file:line; tested with those exact strings |
| The update check trusted `html_url` after a prefix check, counted successes (so a rate limit retried all day) and could be pointed anywhere by a taken-over repository | The page address is built from the configured repository and a strictly checked tag; one attempt a day is recorded; off until a repository is set |

A Sonnet review of the finished Phase 3 code found more, all fixed with tests:

| Finding | Fix |
| --- | --- |
| The account guard checked only for an `account` key, so a patch that was a list (serde reads one in field order) rewrote every setting, host and user included, skipping the data reset; `onboarded` could also be set | A patch must be an object, and the account and setup flag must come out of it unchanged |
| Addresses were only partly removed: `jane.o'connor@…` left "jane.o'", and VERP bounce addresses leaked the recipient's name | Any piece of text holding an "@" is replaced whole |
| An error line was kept whole if a safe phrase appeared anywhere in it, so "…bad header Jane Doe you replied" survived | The text after an error colon is kept only when it starts with a safe phrase |
| The Drafts folder name and a custom mail-server host identify the user, and both were in the report | Folder and path-like names become `<name>`; only well-known provider hosts are named, anything else is "custom server" |
| The day's update check was spent before the network was tried, so a laptop with no Wi-Fi at login never checked again that day | The day counts only once the server answers; a transport failure is retried in an hour, one check at a time |
| The stored tag was rebuilt as "v" + version, so a tag without a "v" produced a dead link | The real tag is stored, and a tag may carry at most one leading "v" |
| A pre-release was never told about its own final release | A final release counts as newer than its pre-releases |
| Tests that could not fail: the update gates (short-circuited by the empty repository) and the end-to-end diagnostics check (which passed with no log at all) | Both strengthened; the gates are now tested directly |

Known and accepted: an account change or a server renumbering clears the stored "checked today" marker, so one extra
check can happen that day; a renamed repository stops notices silently (GitHub answers with a redirect, which the app
does not follow); the check is on by default and the wizard does not ask about it (PRIVACY.md describes it, and it does
nothing until the project has a repository).

## Review 4 (two-model council plus a documentation drift pass, 2026-11-13)

Two models were asked the same question independently — "should this be published as 0.1.0 to strangers?" — without
being told each other's answers or mine. One said **do not ship**, the other **ship with caveats**. Both were right
about different things, and every finding below was reproduced before it was changed.

### Found by the seat that said "do not ship"

| Finding | Fix |
| --- | --- |
| **The phone hub served a working fake sign-in form to everyone on the network.** Static files need no access code (the page has to load before it can authenticate), and the shipped bundle included the demo backend. A visitor with no code therefore got the setup wizard, an "App password" field, and the answer "Connected · Sent folder: [Gmail]/Sent Mail" to anything typed. The legitimate user whose phone lost its code saw the same thing: a real password typed into a form that discards it, followed by a fabricated inbox. Reproduced in a browser end to end | The demo backend is no longer part of the hub bundle, and a page with no access code is now a dead end that says to scan the code again. It contains no password field. Development uses `app/ui/demo.html`, which the hub refuses to serve |
| **There was no way to re-enter a password.** `save_account` was reachable only from the first-run wizard; Settings offered only "Sign out and reset", which deletes the task list, notes, drafts and your own tasks. Providers cancel app passwords whenever the account password changes, so this is a first-week event | Settings has a "New app password" field that checks the password and saves it. The mailbox is unchanged, so nothing is deleted |
| A test named `changed_uidvalidity_resets_the_cursor_so_new_mail_is_not_lost` asserted that every task was deleted | Renamed to `changed_uidvalidity_starts_the_mailbox_over_because_the_uids_now_mean_other_mail`, with a comment explaining that the mail is still on the server |

### Found by the seat that said "ship with caveats"

| Finding | Fix |
| --- | --- |
| **A mailbox whose Sent folder could not be found loaded no mail at all.** The lookup ran before the first sync and its failure restarted the whole connection every 30 seconds. Only five English and Gmail names are guessed, so a server that sets no `\Sent` flag and calls the folder "Gesendet" or "Éléments envoyés" left the user with an error and an empty list | The Sent folder is optional. Mail loads either way; only reply detection and "someone you write to" are weaker, and the user is told why |
| **A refused password was retried every 30 seconds for ever.** Providers lock accounts for that | A refused sign-in is now its own kind of error: it waits five minutes and the status says to enter the password again |
| **Mail could disappear from the list.** A message the server answered for but sent no content for was skipped, while the cursor still advanced past it, so it never appeared | The cursor stops short of such a message and tries again; after a few tries it gives up and says so in the log |

### Accepted, not changed

Unsigned builds, no automatic updates until the project has a repository, and the security contact in this file are all
still open and are listed above. The deeper point both seats made stands: **nothing here has ever spoken to a real mail
server.** The mock in `engine/tests/imap_flows.rs` proves the client does what its author expected, not what Gmail or
Dovecot actually does. A live run remains the single most valuable thing left to do.

Still open from this review: a signed update channel, code signing and notarization, a live-mailbox test, macOS and
Linux builds run on real machines, a `Host` header check on the hub, two watchers briefly overlapping after a
settings save (the stop flag now keeps them from writing, but the old one can still sit in IDLE for up to a minute),
quitting the app twice in the middle of one slow message (which counts like a crash and skips that message),
the hub dropping phones on unusual public-address networks without a log line, manual-task ids sharing the IMAP UID range, and re-matching tasks by Message-ID after a UIDVALIDITY
change.

## Review 5 (Phase 4: select many, rules, follow-ups; council, drift pass and an Opus review loop, 2026-11-13)

Two model seats (Opus and Sonnet) answered "should this ship?" blind; both said **ship with caveats**, and they agreed on the
defects. Two fresh-eyes passes then compared every document and in-app promise with the code (29 problems). An Opus reviewer
then went through the final change in two rounds and approved it. Full ledger: `docs/2026-11-13_PHASE4_REVIEW.md`.

| Finding | Fix |
| --- | --- |
| **A `sender` rule matched by substring and on the display name.** A stranger could send from `dana@studio.example.evil.example`, or from any address with the display name "Your Accountant", with valid SPF and DKIM, and climb to priority 5 with an alert. The forged-sender guard only catches `dkim=fail`-style forgeries, not a real message from the attacker's own domain | A `sender` rule is the exact address and a display name is never matched. A rule may raise a priority only for mail that passed its sender check, and a `subject` rule only for someone you write to (anyone can choose a subject). The same guards apply on arrival and in "Apply to my list" |
| **Your own nudge and out-of-office replies counted as replies to a follow-up.** On Gmail, All Mail holds your own mail, so reminding someone closed the reminder with a false "They replied", or stopped it ever being made | The reply search leaves out your own addresses and anything marked `Auto-Submitted`; a reply the app has already read is recognised locally, even after it was filed away |
| **On servers without All Mail, a reply you archived (including by pressing X) produced a false "nobody has replied" task** | The `\Archive` folder is searched too, and replies already read by the app are checked first |
| **Open follow-ups beyond the tenth were never rechecked and every open one cost a search on every pass** | Open follow-ups now close from mail the app has already read, with no search; at most 20 are open at once |
| **The follow-up check ignored the stop flag**, so it could write a task naming a recipient into a database that "Sign out and reset" had just wiped | The check stops before each step and each write |
| **The "kept 60 days" promise for follow-up records was false: nothing deleted them**; the same gap meant the 90-day and 30-day clean-up ran only at start-up | The routine clean-up removes them, and the scheduler runs it once a day |
| **The reason line a rule left on a task repeated the rule's text, and the phone sees task reasons** | The reason says "a rule of yours" without the text |
| Two threads adding a task at the same moment could pick the same id, and a follow-up's task and record were written separately | The id is chosen inside the `INSERT`, and the follow-up writes are one transaction |
| Bulk priority changed every message of a conversation, so one click recorded one ranker correction per message | It changes the newest message of each selection, like pressing 1-5 on a row |
| The select keys skipped the limits the bar enforces (snoozing already-snoozed rows lost their original times on undo) | Keys and buttons go through the same check |

Accepted, with the reasons in `docs/designs/phase4.md`: a reply moved to another folder before the app read it, an
autoresponder that does not mark itself, a substring `FROM` match, a reopened follow-up not counting toward the cap, and a
list reply not closing an open follow-up.

## Review 6 (Phase 5: reminders, search inside messages, calendar invitations; council, drift passes and an Opus review loop, 2026-11-13)

An Opus seat and a Sonnet seat answered "should this ship?" blind: **do not ship** and **ship with caveats**. Two fresh-eyes
passes (17 and 13 problems) compared documents and promises with the code. An Opus reviewer then went through the fixed
tree in two rounds and approved it. Full ledger: `docs/2026-11-13_PHASE5_REVIEW.md`.

| Finding | Fix |
| --- | --- |
| **Words of deleted messages stayed readable in the database file.** A delete in a full-text index only writes a tombstone; the removed words stay in the index segments until a merge, which for an old index may never happen. Pruned, replaced and edited text, and a mailbox switch, all left words behind, although PRIVACY.md said the index goes with the task list | The index is set to overwrite deleted entries (`secure-delete`), written once, not on every open; a test reads the database and journal files after each way a task can disappear |
| **A repeating task with only a due date could not be finished before that date** (rent paid on the 28th for the 1st stayed put) | The due date is the floor for the next occurrence, like the reminder is |
| **Invitation times in a Windows zone name (what Outlook writes) were dropped**, so the app showed and saved the wrong hour for meetings across zones, with no hint | About sixty Windows names are understood; any other zone is said to be unknown in the card |
| **UTC invitations took the UTC day** for the tag and the due date (23:30Z on the 14th is the 15th in India) | The day is the day where you are |
| **An invitation's `DURATION`, an end time in another zone, and recurrence were lost silently**, so a saved file could have the wrong length (3 hours instead of 8) or hide that it repeats | A duration gives the end; an end in another zone is dropped rather than misread; a repeating invitation is marked and only its first time is kept |
| **U+2028, U+2029 and bidirectional or zero-width marks passed the cleaning** of calendar text, so a summary could end a line for a program that splits on U+2028 or reorder what a reader sees | They are removed on reading and again when the file is built; the builder does not rely on the parser |
| A reminder could not be re-saved or given a repeat once its time had passed; one edited at the instant it fired could be lost | The time the task already has is accepted and not announced again; the announcement marks only the time that was selected |
| App-made follow-up tasks accepted reminders; "Done all N" ignored the repeat handling and could leave an Undo bar that reversed an unrelated action; finishing a repeating task twice quickly skipped a period | Refused, routed, and guarded |

Accepted, in `docs/designs/phase5.md`: a repeating invitation is saved as its first occurrence only, and a zoned event is
written without a `VTIMEZONE` block (Google and Apple accept that, some other calendars may not).

## Review 7 (Phase 6: several mailboxes; council, drift passes and an Opus review loop, 2026-11-13)

An Opus seat and a Sonnet seat reviewed blind and both said **ship with caveats**. A Sonnet pass compared the design and pseudocode
with the code (12 disagreements, three of them code bugs) and a Haiku pass compared the promises (5, one a code bug). An Opus
reviewer then went through the fixed tree in three rounds (six findings, then one, then none) and approved it. Full ledger:
`docs/2026-11-13_PHASE6_REVIEW.md`.

| Finding | Fix |
| --- | --- |
| **The phone received every mailbox's login.** The sync logs "connected as <login>" and the phone's state carried the log unfiltered, so the phone hub, which must never see addresses, saw one per mailbox (one before this change) | The log the phone gets has the login removed from those lines |
| **A stop while the last message of a batch was being ranked moved the cursor past it**, so the message was skipped for good after any restart (this predates the change) and a removed mailbox's cursor could be written back, so a new mailbox with the same number skipped its whole inbox | A stopped sync no longer touches the cursor or reports a status; a new mailbox starts from a clean number |
| **The engine's tests used the real OS keychain when cargo was started from the top folder**, so a test run could overwrite or delete the passwords of the person running it (it happened once during this work: two dummy entries, removed by hand) | The engine recognises its own cargo test runs and uses an in-memory keychain wherever they are started; a test pins it |
| **Data crossed mailboxes**: a follow-up record of an answered message went to mailbox 0; a correction unlinked by a renumbering sat in mailbox 0's id range and was deleted with it; a snooze was woken by the same message arriving in another mailbox; the digest and wake notices merged conversations across mailboxes; a follow-up open for 60 days lost its mailbox and survived the mailbox's removal (also for databases from the previous release) | Each now belongs to its own mailbox and goes only with it |
| **`save_account` reset a mailbox before stopping the syncs; `remove_account` and `sign_out` left every thread stopped if they failed halfway** | The syncs are stopped first and started again whatever happens (by construction; a race cannot be forced reliably in a test) |
| The duplicate-mailbox check missed `imap.gmail.com.` and `googlemail.com`; after the first mailbox was removed only four could be added; the first place needed no address | The server is compared in a normal form; the first place is taken again by the next mailbox; every mailbox needs a server and an address |
| UI: read message text and undo closures could follow a reused mailbox number; Cancel during Test or Add threw; focus was dropped | Caches are cleared when mailboxes change; the dialog ignores late answers and keeps focus |

Accepted, documented: removing a mailbox leaves the short activity-log lines that name subjects until the log's 30 days are up; the
phone is told a task's id, which encodes a mailbox number but no name or address; Gmail's per-address link form
(`/mail/u/<address>/`) is untested against a live account; nothing was run against a live mailbox, macOS or Linux.

## Review 8 (Phase 7: backup and restore; council, drift passes and an Opus review loop, 2026-11-14)

An Opus seat and a Sonnet seat reviewed blind and both said **ship with caveats**. A Sonnet pass compared the design, pseudocode and
history documents with the code (14 disagreements) and a Haiku pass compared the promises (6). An Opus reviewer then went through the
fixed tree in three rounds (a blocker and two other findings, then one regression, then one race) and approved it. Full ledger:
`docs/2026-11-14_PHASE7_REVIEW.md`. A backup file is untrusted input when it is restored, and a private document when it is written,
and most of the findings are about one of those two things.

| Finding | Fix |
| --- | --- |
| **A restored file could switch the ranking to the paid cloud service.** With the options box ticked and an API key saved, mail details would go to Anthropic and the key would be billed | The cloud ranking is never taken from a file; the preview says so. A local model or rules only can come back |
| **A restored mailbox could sign in with a password left in the keychain under its number** (by an earlier install, or a mailbox removed in a hurry), sending it to whatever server the file names | A new mailbox's number is cleared of data and keychain password before it is used; the preview lists each new mailbox with its server so it is seen before an app password is pasted |
| **The backup could hold text written about your mail**: reply drafts from the AI or from a template, which paraphrase the message | Only drafts you edited yourself are saved; corrections (which hold a sender and subject) are the one place mail details appear, and PRIVACY.md says so |
| **Restoring could change what you have**: old corrections counted as the newest; a restored correction was filed under mailbox 0 and lost when it was removed; a snooze could be unlimited; "restoring twice changes nothing" was false in four cases | Corrections go by date and belong to no mailbox; a snooze is limited like one you type; every case of restoring twice is a test; the exceptions that remain (a later snooze, ticked options, a sender choice's effect on that sender's open mail) are named in the dialog and the documents |
| **A restore of the biggest file took about 45 seconds and locked the database**, and one that started while a sync was saving mail failed with "database is locked" | Indexes on the columns it looks up; the transaction takes the write lock first and waits for it |
| A restore was possible in a window that showed the wrong preview, with unsaved rule edits, or during an apply that could not be cancelled; the rule editor kept stale rules and the next "Save rules" removed what was restored | The preview always belongs to what the box says; unsaved rules block a restore; the editor reads the rules again; Cancel is off while applying |
| The release check said "dated" but only rejected the word "Unreleased", and the version and release histories had no check | A tag now needs a dated changelog heading and a row in `docs/RELEASE_HISTORY.md`; the check has its own tests, run by CI |

Accepted, documented: the backup is not encrypted; a restored mailbox's server comes from the file (shown in the preview, with a warning
to restore only your own file); nothing was run against a live mailbox, macOS or Linux.

## Review 9 (Phase 8: a big mailbox stays fast; council, drift passes and an Opus review loop, 2026-11-14)

A performance change with no new input and no new command, reviewed all the same. An Opus seat said **do not ship as it was** and a Sonnet
seat **ship with caveats**; a Sonnet pass found 10 disagreements between the documents and the code and a Haiku pass 6; an Opus reviewer
then went through the fixed tree in two rounds and approved it. Full ledger: `docs/2026-11-14_PHASE8_REVIEW.md`. No security finding;
the one that mattered was about the user's data.

| Finding | Fix |
| --- | --- |
| **Undo of a bulk finish silently reopened only some of the tasks.** The window now holds only the newest 200 finished tasks, and the undo closures asked the engine to reopen a task only if the window still held it; after the next poll a "Done all automated" of 300 came back as 198, and the rest were finished tasks that appeared nowhere | The undo asks the engine to reopen every task it finished; the Done tab has "Show older" (the engine takes `state { done }`, at most 20,000) so older finished tasks are reachable |
| The keyboard could be taken over by a row that had focus (`j` stuck, `x` finished the wrong task); a click in an open fold drew the whole main list; "Show more" under a fold grew the list above it | Focus is released when the cursor moves; each part of the list has its own count |
| The documents promised a search of finished tasks that does not exist, and counts that were not true on the Done tab | Reworded; the promises now say what the code does |
| The new scale check could have deleted a real app data folder | It refuses to run if the folder exists; it only removes a folder it created |

## Review 10 (Phase 9: tests for the window; council, drift passes and an Opus review loop, 2026-11-14)

No new input, no new command and no change in what the app does (one line in the restore dialog was reworded to say what it already did).
The new thing is the test harness in `app/test`, which opens a local web server and a browser's debugging port while tests run, so that
was what the reviewers looked at. A two-seat council (Opus: do not ship until two things were fixed; Sonnet: ship with caveats), a Sonnet
pass and a Haiku pass over the documents, and an Opus reviewer in two rounds (round 2: approve) are recorded in
`docs/2026-11-14_PHASE9_REVIEW.md`.

| Finding | Fix |
| --- | --- |
| What the harness opens: the server serves only files under `app/ui` and listens on 127.0.0.1 only (a `..` or absolute path gets a 404, a malformed `%` in an address got an unhandled exception and now gets a 400); the browser's debugging port listens on 127.0.0.1 and exists only while tests run, so a local program could drive an empty-profile browser during a run | Accepted: same trust as any local development server; the browser starts with an empty temporary profile |
| What the harness ends: it ends processes. The first version ended the one process it started (not the browser, on Windows, and `taskkill` was not found from some shells), a later one searched for processes by the run's own temporary profile folder without a time limit, and a number the browser had reported could be reused by another process before it was killed | It asks the browser for its own process number and ends that process, and its helpers, at the moment it reports it; it calls the system tools by full path; the search by profile folder (the run's own random folder name, never anything else) is used only if the browser does not stop answering, with a time limit; folders are deleted by a process that only ever deletes folders it created |
| A start that failed part-way left a server listening and a browser running, so a test run never ended and the browser stayed | A failed start ends what it began; every wait has a time limit |
| No dependency was added (a request to install a DOM library was refused by the permission system and not retried), nothing is downloaded, nothing can send mail, and `app/test` is outside what the installer bundles | |

## Review 11 (Phase 10: documentation as code; three-seat council, drift passes and an Opus review loop, 2026-11-14)

A documentation phase with one engine fix. Opus, Sonnet and Fable seats (the owner asked for Fable) each said **ship with caveats**; a Sonnet
pass over the technical documents, and two Haiku passes over what the guides, FAQ and runbooks promise, checked about 1,300 claims
against the code; an Opus reviewer approved in the second round. Full ledger: `docs/2026-11-14_PHASE10_REVIEW.md`.

| Finding | Fix |
| --- | --- |
| Statements about security and privacy that the code does not support: the phone hub's command allow-list is twelve commands, not eleven; Ollama stays on the computer only while its address is this computer's (Settings accepts any http or https address); "never deletes a message" (on a server that cannot move one, the app copies it to Archive and removes the original); a backup module comment said it holds no message text (a correction keeps the sender and subject); a hub time limit described as on the whole request when it is on each read | The documents and comments now say what the code does |
| The Settings "Test" for ranking showed a green "Works" when no model was chosen or no API key was saved, so a person could believe mail was being ranked by a model when it was ranked by rules | It answers "No model is chosen" or "No API key is saved"; test first, each guard put back once |
| The documentation checker itself: what it can and cannot catch is written in the review record (it checks structure, the command list, limits and counts, not whether a sentence is true) | Hardened against the weaknesses the reviewers found; 38 tests of its own |
| A new runbook for a leaked password, key, access code or topic | Revoke at the source first, then give the app the new one; every button named is real |

## Review 12 (Phase 11: gaps the documentation and the first CI run found; three-seat council, two drift passes and an Opus review loop, 2026-11-14)

Opus, Sonnet and Fable seats each read the change blind: no seat found a bypass or a regression for a real user (Opus: ship; Sonnet and
Fable: ship with fixes, all test gaps and one wrong sentence in the command reference). A Sonnet pass over the technical documents and a
Haiku pass over the guides checked about 210 claims; an Opus reviewer approved in the first round. Full ledger:
`docs/2026-11-14_PHASE11_REVIEW.md`.

| Finding | Fix |
| --- | --- |
| A task number, a priority and a mail server port were cut to 32, 8 and 16 bits, so a request for task 2^32 + n acted on task n and port 66529 meant 993 | `try_from`: a number that does not fit names no task, no priority and no port; ports outside 1 to 65535 are refused |
| A settings request could set the phone hub's access code to any string (such as "a") and the ranking mode to any word | The access code can only be made by the app (switching the hub on, or Rotate access code); the mode must be rules, ollama or anthropic (a saved file with another mode reads as rules) |
| "Test connection", "Detect", saving a mailbox and finishing setup took whatever they were given: an empty password was saved, setup could finish with no mailbox | They check the server name, address, password and web address first; each guard was put back once (23 mutants) and a test failed |
| Two writes took the database's write lock late, so a sync committing in between made them fail with "database is locked" (seen on Linux) | The write lock is taken at once (`BEGIN IMMEDIATE`) |

Not closed at the time: one run of the window's tests failed on Linux in CI and the next passed on the same code, cause not found (a failure
now puts the last lines of the log in an annotation). Two more failures followed, and the annotation showed the cause: a browser that was slow
to start on a busy runner (fixed in Phase 12).

## Reporting a vulnerability

Please report privately to the maintainer rather than opening a public issue. Add a contact address
here before publishing.
