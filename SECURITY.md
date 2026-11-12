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
| 8 | Low | **Data at rest**: no retention; a changed `UIDVALIDITY` could lose mail | Finished tasks pruned after 90 days, log after 30; cursor resets when the server's `UIDVALIDITY` changes | `prune_drops_old_done_tasks...`, `changed_uidvalidity_resets_the_cursor...` |
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
limited to four, and desktop-only commands blocked; secrets in the OS keychain only; the API key is
sent only to a fixed Anthropic HTTPS URL; TLS to the mail server with hostname verification;
ntfy title restricted to ASCII; all UI sinks escape `& < > " '`.

## Dependency scan

`cargo audit` (RustSec, 1,471 advisories): the one vulnerability (bundled SQLite, RUSTSEC-2026-0459)
was fixed by upgrading `rusqlite`. One warning remains: `lexical-core 0.7.6` (RUSTSEC-2023-0086,
soundness), pulled in by the IMAP parser. **Accepted** because it parses only TLS responses from the
user's own mail provider and the only newer IMAP release is an alpha with a different API. Revisit
when `imap` 3.x is stable. The IMAP parser also triggers a Rust future-incompatibility warning.

## Residual risks and limits

- **The phone hub is plain HTTP.** On a shared Wi-Fi the access code can be sniffed. Use it on a
  trusted home network or over a private VPN (for example Tailscale). Rotating the code limits exposure.
- **Local data is not encrypted at rest.** `agent.db` holds subjects, 400-character excerpts, up to 3,000 characters of message text (kept only to write replies) and any reply drafts;
  `settings.json` holds the access code. They live in the user's profile; use full-disk encryption.
- **Email content goes to the chosen model.** Rules-only mode sends nothing. Ollama stays on the
  computer. An Anthropic key sends email text to Anthropic.
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
- **CI actions use version tags, not commit SHAs.** Pin them to SHAs before the first public release.

## Reporting a vulnerability

Please report privately to the maintainer rather than opening a public issue. Add a contact address
here before publishing.
