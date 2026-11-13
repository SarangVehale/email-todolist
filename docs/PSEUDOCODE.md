# Pseudocode

How the engine works, in plain steps. File names point at the real code. Each block ends with the **edge cases** it
handles and the test that pins them (see [TESTCASES.md](TESTCASES.md)).

## 1. Start-up (`core.rs: Core::start`)

```
start():
    stop every thread from the previous start; make a fresh stop flag
    settings = load settings.json            # corrupt or missing -> defaults
    open the database: integrity check; a damaged file is set aside as agent.db.corrupt-<time> and rebuilt;
                       data from a newer app version is refused and left alone
    start scheduler thread                    # always (snooze wake-ups, daily digest)
    if hub enabled:      start hub thread
    account = settings + password from OS keychain
    if no account or not onboarded: status = "setup" / "no password"; return
    start watcher thread                      # section 2
    start write-back thread                   # section 6
```
Edge cases: restarting after every settings save must not leave two hubs fighting for the port (hub retries its bind for
6 s) — `hub_survives_repeated_settings_saves`.

## 2. Watching the mailbox (`watcher.rs: run`)

```
run():
    loop until stop:
        connect over TLS (15 s connect, 60 s read/write limits); login
        sent_folder = folder flagged \Sent  (else common names)
        sent_index  = recipients and Message-IDs of the last 500 sent messages     # read-only
        process_new()
        loop until stop:
            EXAMINE INBOX (read-only); IDLE for up to 60 s
            if the mailbox changed: process_new()
        on any error: status = "error"; wait 30 s; reconnect
```
Edge cases: network drops, server hangs — timeouts; `a_failing_server…`, IMAP flow tests.

## 3. Fetching new mail (`watcher.rs: process_new`)

```
process_new():
    mailbox = EXAMINE INBOX
    if server's UIDVALIDITY differs from the stored one:        # UIDs now mean different mails
        forget tasks, drafts, queued actions, cursor; log it
    uids = (cursor == 0) ? latest N of ALL : UID cursor+1:* filtered to > cursor
    for each chunk of 20 uids:
        sizes = FETCH RFC822.SIZE;  skip (and log) anything over 5 MB
        for each wanted message: FETCH BODY.PEEK[]          # PEEK: never marks mail read
            mark the message as in flight; if it has been started twice without finishing (it crashed the app): skip it
            inside a panic guard (a bug on one message skips only that message):
            email = parse(raw)           # section 4; failure -> log, continue
            signals = compute(email, sent_index)
            triage  = AI or rules, falling back to rules on any error
            triage  = apply sender preference (important / mute) unless the sender check failed
            store task; callback(alert, label, auto-draft)
            clear the in-flight mark
        cursor = max(cursor, highest uid in chunk)           # skipped messages still advance it
```
Edge cases: empty inbox, UID gaps, renumbered mailbox, oversized and hostile mail —
`an_empty_inbox_is_fine`, `uids_with_gaps…`, `a_renumbered_mailbox…`, `oversized_and_hostile…`.

## 4. Parsing one message (`mail.rs: parse`)

```
parse(raw):
    if more than 200 "multipart/" markers: refuse                 # stack-depth attack
    on a 64 MB-stack thread:                                      # a panic cannot abort the app
        headers: From, Subject, Date, Reply-To, References, List-*, Authentication-Results
        message_id = Message-ID only if it is a plain <visible-ASCII> token <= 200 bytes, else ""
        sender_ok  = Authentication-Results has no dkim/spf/dmarc "=fail"
        text = first text/plain part
        if text is empty: text = first text/html part rendered to text   # only the first 256 KB; one linear pass; blocks -> newlines, tags dropped,
                                                                         # script/style/title/comments removed,
                                                                         # entities decoded
        body = text with quoted ">" lines removed, blank runs collapsed, capped at 3000 chars
        excerpt = first 400 chars on one line
        reply_to = first valid Reply-To address, else the sender
        attachments = names of parts marked attachment (or non-inline parts with a file name): count them all,
                      keep five names, path and control characters removed, <= 60 chars; inline images are skipped
        unsubscribe = first https link in List-Unsubscribe, else the first mailto link; anything else
                      (http, javascript, data, spaces, > 500 chars) -> ""
```
Edge cases: HTML-only mail, other charsets, base64 and quoted-printable, encoded-word headers, missing From/Subject/
Date, NUL bytes, attachment-only mail, 20 000-character subjects, forged Message-IDs —
`html_only_mail_becomes_readable_text`, `other_character_sets…`, `missing_or_strange_headers…`.

## 5. Ranking (`signals.rs`, `triage.rs`, `ai.rs`)

```
signals: known    = sender in my Sent recipients   AND sender_ok
         reply    = In-Reply-To matches a Message-ID I sent  AND sender_ok
         bulk     = List-Unsubscribe | List-Id | Precedence bulk | Auto-Submitted | noreply address

rules score:  reply or (known and not bulk) -> 4;  bulk -> 1 (2 if known);  otherwise 3
              category from subject words (finance/travel/promo/work), else notification/personal/other
              needs_reply = not bulk and (reply or known) and the text contains "?"
              due = first date in the subject or first 1500 body characters that follows a cue word
                    (due, by, before, until, deadline, on ...): ISO, "20 Nov [2026]", "Nov 20th", weekday names
                    (the same day counts), today/eod, tomorrow; no cue -> no date; impossible dates (30 Feb) ignored;
                    a year-less date more than 45 days past means next year. Relative words use the day the mail arrived.

AI (optional, local Ollama or own API key):
    prompt = signals + recent corrections + the email with < > swapped so it cannot close its wrapper
    reply  = first JSON object that has a priority   # tolerant: prose, fences, 4.0, "4", "yes", extra objects
    category must be one of eight known names (else "other"); due must be a real YYYY-MM-DD
    any failure -> rules;  if the model left due empty, the rules' date extraction fills it
```
Edge cases: spoofed sender, prompt injection, hostile category strings, malformed model output —
`failed_dkim_or_spf…`, `email_text_cannot_close…`, `unknown_or_hostile_categories…`, `model_replies_in_many_shapes…`.

## 6. Changing the real mailbox (`writeback.rs`) — only when the user turned it on

```
user action (done / star / priority / save draft) -> update local task AND queue an outbox item
    (tasks you added yourself, uid >= 2,000,000,000, and notes never queue anything: there is no message to change)

drain():                                           # in order; stops at the first failure
    for item in outbox:
        if the task has no usable Message-ID: log why; drop the item      # never guess
        hits = SEARCH HEADER Message-ID "<id>"      # a SUBSTRING match on the server
        if more than 50 hits: do nothing
        keep only hits whose own Message-ID equals the id exactly; if none or more than 3: do nothing
        Gmail:   done -> remove \Inbox, add "AI/Done";  priority -> swap "AI/P#";  star -> \Flagged
        others:  done -> MOVE to the Archive folder (COPY + \Deleted + EXPUNGE if MOVE is refused)
        draft:   build the message (section 7); APPEND to the Drafts folder flagged \Draft
        on error: retry later; after 5 tries drop the item and log it
```
Edge cases: decoy IDs that merely contain the real one, duplicate copies, missing folders, refused commands —
`gmail_actions_touch_exactly…`, `duplicate_copies…`, `a_missing_archive_folder…`, `a_lookup_with_no_exact_match…`.

## 7. Reply drafts (`commands.rs`, `drafts.rs`) — nothing is ever sent

```
draft_generate(task):                              # desktop only
    if the task is one you added yourself: refuse (nothing to reply to)
    if the existing draft has the user's edits and no explicit "replace": refuse
    model = configured AI;  text = model.draft(email) or, on error/empty, a quick-reply template
    clean: no code fences, no leading "Subject:", no control, zero-width or bidi characters, <= 8000 chars
    store the draft with its source (ai | template | user)

auto_draft(new task):                              # one at a time
    only if enabled, needs a reply, priority >= minimum, an AI is configured, no draft yet
    take one slot from today's budget (counter that edits/discards cannot refund); else stop

build_message(draft):                              # for "Save to Drafts"
    from and to must be single valid addresses (no spaces, quotes, commas, control chars)
    subject: control characters stripped, <= 200 chars, "Re: " added once, RFC 2047-encoded if non-ASCII
    In-Reply-To / References: only clean Message-IDs
    body: base64; headers can never contain a line break
```
Edge cases: header injection, extreme subjects, empty bodies, odd sender names, repeated clicks —
`draft_message_headers_cannot_be_injected`, `draft_subjects_and_bodies_at_the_extremes…`, `saving_a_draft_is_queued_once…`.

## 8. Phone hub (`hub.rs`)

```
accept loop (non-blocking, honours the stop flag):
    if the peer is not on a local network (loopback, private, link-local, 100.64/10, IPv6 local): drop it
    if 32 connections are already open: drop the new one
    spawn a thread:
        5 s read and write timeouts
        read the request head (<= 16 KB) -> else 431;  malformed request line or header -> 400
        Content-Length > 64 KB -> 413;  not a number -> 400;  short body -> 408/400
        GET  exact path in the embedded file table -> file (never the filesystem)  else 404
        POST /api/rpc:
            address locked out (10 failures in 60 s)?            -> 429
            bearer token compared in constant time, scheme case-insensitive -> else 401 and count a failure
            command must be in the phone allow-list (no settings, secrets, export, draft generate or save)
        every response: nosniff, X-Frame-Options DENY, no-referrer, CSP without inline scripts, no-store
```
Edge cases: stalled clients, floods, hostile headers, odd methods — `stalled_clients_cannot_block…`,
`the_hub_recovers_after_a_flood…`, `broken_content_lengths…`, `the_bearer_scheme_is_case_insensitive…`.

## 9. Scheduler (`jobs.rs`)

```
every 20 s:
    for tasks whose snooze time has passed: clear the snooze, notify once ("Back from snooze")
    if digest time is set, now >= that time, and no digest has been sent today:
        record today; if anything is open: notify "N open · H high · U urgent · D drafts ready"

snooze presets (local time):  3h = now + 3 hours;  tomorrow = tomorrow 09:00;
                              next week = the next Monday after today, 09:00;  clear = bring back
quiet hours: window may wrap past midnight; start is inside, end is outside; urgent mail ignores it
```
Edge cases: month/year/leap-day boundaries, Sunday and Monday starts, midnight windows, daylight-saving gaps (the
earliest valid local time is used) — `snooze_presets_across_month_year_and_leap_boundaries`,
`next_week_always_lands_on_a_monday…`, `digest_and_quiet_hours_at_the_boundaries`.

## 10. Settings (`settings.rs`)

```
save_settings(patch):
    merged = current + patch (unknown fields ignored; wrong types -> error)
    validate: server host = letters digits . - only; ports 1..65535, hub port >= 1024;
              ntfy topic = letters digits _ - only; ntfy and Ollama URLs must be http(s) without whitespace;
              times "HH:MM"; quiet hours need both ends; draft tone, signature length, limits in range;
              alert categories only from the eight known names (empty = all; urgent mail always alerts)
    hub disabled -> clear its access code;  hub enabled with no code -> make a new random one
    write atomically (temp file then rename)
```
Edge cases: corrupt or older settings files fall back safely — `corrupt_or_old_settings_files…`,
`bad_settings_patches_are_errors_not_crashes`.
