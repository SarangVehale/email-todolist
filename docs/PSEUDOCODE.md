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
    start scheduler thread                    # always (snooze wake-ups, daily digest, the update check)
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
        sent_folder = folder flagged \Sent  (else common names; none found: log it and carry on with an empty index)
        sent_index  = recipients and Message-IDs of the last 500 sent messages, and the conversations you
                      started among them (subject, first recipient, date)     # read-only
        process_new()
        maintain()                            # section 5b
        loop until stop:
            EXAMINE INBOX (read-only); IDLE for up to 60 s
            if the mailbox changed: process_new(), then maintain() at most every 20 s
            otherwise, every 120 s: maintain()
        on any error: status = "error"; wait 30 s; reconnect   (300 s, with a plain message, when the server refused the sign-in)
```
Edge cases: network drops, server hangs — timeouts; `a_failing_server…`, IMAP flow tests.

## 3. Fetching new mail (`watcher.rs: process_new`)

```
prune old data (done tasks past 90 days, log past 30, corrections past 365)

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
            triage  = apply sender preference: "important" is not applied to mail that failed its sender check, "mute" always is
            triage  = no sender preference: apply the first matching user rule (section 5c)
            triage  = rules only, no preference, no rule set the priority, sender check not failed: shift priority by what the last three
                      corrections of this ADDRESS taught (>= 2, same direction, 1-2 steps); undo removes a correction
            a new message in a snoozed conversation wakes it
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
        thread = first id of the raw References header, else In-Reply-To, else own Message-ID  ("" if none usable)
        attachments = names of parts marked attachment (or non-inline parts with a file name): count them all,
                      keep five names, path and control characters removed, <= 60 chars; inline images are skipped
        unsubscribe = first https link in List-Unsubscribe, else the first mailto link, and only with a real
                      public host (or a plausible mailbox); anything else (http, javascript, data, spaces, an IP
                      or a single-label host, > 2000 chars) -> ""
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
              priority 5 = needs_reply and sender check not failed and due is today, tomorrow or past   # never "urgent"
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

## 5b. Closing tasks that are finished elsewhere (`watcher.rs: reconcile_open`, `refresh_sent_index`)

```
after the first sync, after a mailbox change (at most every 20 s), and every 2 minutes while idle:                    # failures are logged, never fatal
    top up the Sent index with new Sent messages (read again if the Sent UIDVALIDITY changed)
        replied_to = every id named in In-Reply-To (not References) of a sent message, except forwards (Fwd, FW, WG, TR, RV, ENC...)
    for each open mail task (not user-added), newest 500:
        if replied_to has its Message-ID and the user has not reopened it: close it, closed_by = "replied"
                                                                          # no outbox item, mailbox untouched
    EXAMINE INBOX; if its UIDVALIDITY differs from the stored one: stop (process_new handles renumbering)
    for the remaining tasks in chunks of 200:
        FETCH uids RFC822.SIZE; on any error: stop                  # unknown means change nothing
        if the reply names no UIDs, or >= 5 vanished while the inbox is not empty: stop
        mark `gone` for tasks whose UID is missing; clear `gone` for tasks that are back   # they stay open
```
Edge cases: server errors and odd replies, renumbered folders, forwards, reopened tasks, tasks the user added —
`a_task_closes_itself_when_you_replied…`, `forwarding_a_message_is_not_replying_to_it`, `a_server_error_means_unknown…`,
`a_large_batch_vanishing…`, `a_reopened_task_stays_open…`.

## 5c. Your own rules (`rules.rs`, `store.rs: add, apply_rules`)

```
rule = { id, field: sender | domain | subject, text, priority?, category?, file }
valid: <= 50 rules, unique short ids, text 1-100 characters with no control characters, a known field, priority 1-5,
       a known category, at least one outcome, and `file` never together with a priority
match: sender  -> the ADDRESS is exactly the text (case-insensitive); a display name or a longer address never matches
       domain  -> the address is that domain or a sub-domain of it; a leading "@" is ignored ("notacme.com" is not "acme.com")
       subject -> the subject contains the text          # plain text only: no regular expressions
may_raise: not for mail that failed its sender check; a `subject` rule only when the sender is known or replying to you

on arrival (Store::add), after the model or rules ranker and the sender preference:
    if the sender has a preference: skip rules entirely
    r = first rule that matches (the order is the order in Settings)
    if r.priority: apply it when it lowers or keeps the priority, or when may_raise; below 4 it also clears urgent
    if r.category: set it
    if r.file:     priority = min(priority, 2); urgent = false; needs_reply = false; after storing, mark the task done
                   (the same call as pressing X, so with write-back on the message is archived)
    the reason says "A rule of yours ..." without the rule's text (the phone sees reasons)
    the learned shift is skipped when a rule set the priority

apply to the list (rules_apply, dry run counts only, dry defaults to true):
    for each open mail task with no sender preference and no correction of its own on record:
        r = first match; same guards as on arrival; compute the new priority / category / filed state;
        a rule with a priority of 3 or less also takes urgent off, even when the number already matches
        count it; change it unless dry; a changed priority resets "learned" and queues the Gmail label
        nothing is recorded as a correction
```
Rules run on every message read, including a new mailbox's first sync; with write-back off, archive and label changes wait
in the outbox like any other. Edge cases: look-alike addresses and domains, a display name, empty text, the forged-sender
guard, a stranger's subject, a muted sender, urgent mail ranked low by a rule, filed mail never alerting or drafting,
hand-corrected tasks left alone, a refused rule set changing nothing, the reason never repeating the rule —
`phase4.rs` (rules tests).

## 5d. Follow-ups (`followup.rs`, `signals.rs: note_started`, `store.rs`)

```
off by default (settings.followups.days = 0); 2, 3, 5 or 7 turns it on
SentIndex.started: every sent message with a clean Message-ID, no In-Reply-To, a subject that is not a forward, at least
                   one valid recipient (first ten To and Cc) and a Date; newest 200 kept; subject cleaned and cut at 150
SentIndex.from_addrs: up to 8 addresses your sent mail came from
own = from_addrs + the login name when it is an address

check(days, stop):                               # inside maintain(), after the Sent index is topped up
    if days == 0 or stop: return                 # no commands at all, not even for follow-ups already open
    for each open follow-up (not reopened by the user):
        if a stored mail replies to it (thread or References has its id; not its own copy, not from `own`, not bulk):
            close it, closed_by = "replied", remember "answered"        # no search, no server round trip
    room = 20 - open follow-ups
    candidates = started messages sent between 14 days and `days` days ago, not yet in the `followups` table,
                 with a recipient that is not in `own` (the first such one is the subject of the task)
    for each candidate, newest first, at most 10 per pass; stop if `stop` is set before each step and before each write:
        answered = a stored mail already replies to it
                   or, asking the server (folders: INBOX, \All, \Archive when present):
                      EXAMINE; UID SEARCH NOT FROM "<own>"... NOT HEADER Auto-Submitted "auto-"
                                          OR HEADER In-Reply-To "<id>" HEADER References "<id>"     # id is a clean <token>
        any error: stop the pass, remember nothing for this message         # unknown never makes a task
        answered:       remember "answered"            (never searched again)
        not answered:   if room > 0: remember "waiting"; make a task of your own "Follow up: <subject>", priority 3,
                        kind "followup" (the task and its record are written together); room -= 1
prune follow-up records older than 60 days at start and once a day; sign-out and an account change delete them with their tasks
```
Edge cases: server cannot search, replies archived to All Mail or Archive, a reply read and filed by the app, your own
nudge, an out-of-office notice, a task you closed, a task you reopened, messages to yourself (also with a login that is not
an address), replies and forwards you sent, a Date in the future, ids that could break a command, more than ten candidates,
more than twenty open, sign-out in the middle of a check, the feature switched off while follow-ups are open —
`phase4.rs` and the `followups` module of `imap_flows.rs`.

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
Reopening a task the app closed by itself never queues an "unarchive" (the mailbox was never changed).
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
    once a day: the clean-up of old data (done tasks past 90 days, log past 30, corrections past 365, follow-up records past 60)
    if digest time is set, now >= that time, and no digest has been sent today:
        record today; if anything is open: notify "N open · H high · D due [· U urgent] · R draft(s) ready"
        N counts conversations, not messages; snoozed tasks and ones that left the inbox are left out; the urgent
        figure shows only when it is above zero

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
              alert categories only from the eight known names (empty = all; urgent mail always alerts);
              follow-up wait 0 (off), 2, 3, 5 or 7 days
    hub disabled -> clear its access code;  hub enabled with no code -> make a new random one
    write atomically (temp file then rename)
```
Edge cases: corrupt or older settings files fall back safely — `corrupt_or_old_settings_files…`,
`bad_settings_patches_are_errors_not_crashes`.

## 11. Update check and diagnostics (`updates.rs`, `diag.rs`)

```
every scheduler tick:
    if UPDATE_REPO is empty, the user turned the check off, the server already answered today, or a failed try
       is still inside its hour: do nothing
    on another thread: GET https://api.github.com/repos/<repo>/releases/latest     # 5 s, no redirects, <= 256 KB
        tag = tag_name, only if it is a plain version (digits, dots, optional short alphanumeric suffix)
        the server answered: store the tag if it is newer than the running version, else clear it; record today
        no answer at all (no network yet after a reboot): try again in an hour, not tomorrow
state():  show the stored tag only while it is still newer than the running version; the page address is rebuilt
          from the repository and the tag; phones never get it

diagnostics():                                                                      # desktop only
    counts + option names + the server's host name, but only for well-known providers (else "custom server")
    activity lines: quoted text -> ellipsis, around "@" -> <address>, home paths -> <path>, error details removed
    crash lines: keep time, thread and file:line only
```
Edge cases: hostile tags and page addresses, oversized or silent servers, redirects, a rate limit, quotes inside subjects,
backticks in panic messages, folder names with spaces — `tags_that_are_not_plain_versions_are_never_used`,
`the_page_address_is_built_from_the_checked_tag…`, `redaction_handles_quotes_inside_text…`.
