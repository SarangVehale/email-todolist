# User stories

> **Status:** current · **Audience:** the owner, maintainers and testers · **Last reviewed:** 2026-11-14

What each kind of person wants from Inbox Agent, with acceptance criteria and the tests that check them. The stories follow
[the user-story template](../templates/user-story.md); the requirements they serve are in [the product requirements](prd.md), and the
personas (Mira, Tomas, Dana) are described there.

## How to read a story

* **Status** says whether the story is built, partly built or not built, as in the requirements.
* **Verified by** names real tests. Engine tests are in `engine/tests/` and are named by file and test name. Window tests are
  named by their exact title in `app/test/*.test.mjs` and marked "(window)". Manual cases (`MT-nn`) are in
  [the manual test plan](../TESTCASES.manual.md) and need a real mailbox or machine: none has a recorded result yet.
* A story with no automated test says so. A manual case is a procedure, not a pass: see [the test plan](../testing/test-plan.md).

## Contents

| Epic | Stories |
| --- | --- |
| [Onboarding](#epic-onboarding) | US-01 connect a mailbox, US-02 read without changing, US-36 update notice, US-37 install |
| [Triage and ranking](#epic-triage-and-ranking) | US-03 one ranked list, US-04 choose the ranking, US-05 not fooled by words, US-06 a list that tidies itself, US-07 learn from corrections, US-08 important or mute, US-09 your own rules, US-10 search, US-11 alerts |
| [Acting on tasks](#epic-acting-on-tasks) | US-12 keyboard, US-13 select several, US-14 undo a bulk finish, US-15 reply drafts, US-16 file mail, US-17 invitations |
| [Your own tasks and reminders](#epic-your-own-tasks-and-reminders) | US-18 own tasks, US-19 reminders and repeats, US-20 follow-ups |
| [Several mailboxes](#epic-several-mailboxes) | US-21 add, US-22 remove, US-23 one fails, US-24 changes go to the right mailbox |
| [Backup and restore](#epic-backup-and-restore) | US-25 save, US-26 restore, US-27 a file cannot switch on risks |
| [The phone](#epic-the-phone) | US-28 the same list, US-29 safe access |
| [Privacy and safety](#epic-privacy-and-safety) | US-30 never sends, US-31 keychain, US-32 sign out, US-33 hostile mail, US-34 diagnostics |
| [Performance](#epic-performance) | US-35 a big mailbox |
| [Not built yet](#not-built-yet) | US-38 Google sign-in, US-39 automatic updates, US-40 Outlook |

## Epic: onboarding

### US-01: Connect a mailbox with an app password

> **Epic:** onboarding · **Priority:** must · **Status:** built · **Requirements:** FR-01

**As** Mira, **I want** to connect my mailbox in a wizard with an app password, **so that** I do not need to know what IMAP is.

* Given the wizard, when I choose Gmail, iCloud, Yahoo, Fastmail or another IMAP server and press "Test connection", then it
  reports the connection and the Sent folder it found.
* Given a wrong password, then a clear error shows and Continue stays disabled.
* Given an app password pasted with spaces, then the spaces are removed.
* Given a refused password, then the app waits five minutes before trying again, and tells it apart from a server that is down.
* Given an empty server, address or password, a server name with a space in it, or a port that is not a whole number from 1 to 65535
  (66529 is not read as 993), then "Test connection" says what is wrong and does not connect.
* Given no first mailbox with its password, then Finish refuses: it saves the mailbox first, and that says what is missing ("Enter
  the app password for this mailbox", "Enter the mail server and the email address"); the engine's own last check, `finish_setup`
  ("Add your first mailbox before finishing setup"), holds if it is called alone. An empty password is never saved.

**Verified by:** engine `imap_flows.rs`: `a_wrong_password_is_told_apart_from_a_server_that_is_simply_down`,
`special_use_folders_are_found_by_their_flags`, `a_mailbox_whose_sent_folder_cannot_be_found_still_loads_all_the_mail`; `phase11.rs`:
`test_account_and_detect_ollama_check_what_they_are_given`, `a_mailbox_needs_a_password_and_setup_needs_a_mailbox`,
`a_number_too_big_for_its_type_is_refused_not_wrapped`. Manual:
MT-01, MT-02, MT-03, MT-04, MT-07, MT-08. The wizard has no window test.

### US-02: Read my mail without changing it

> **Epic:** onboarding · **Priority:** must · **Status:** built · **Requirements:** FR-02

**As** Tomas, **I want** the app to read my mailbox without marking or moving anything, **so that** trying it costs me nothing.

* Given a first start, then the latest 50 messages become tasks and none changes from unread to read; later reads take only new mail.
* Given an oversized or hostile message, then it is skipped and the sync goes on.
* Given a mailbox that the server renumbered, then the sync starts over instead of losing mail.

**Verified by:** engine `imap_flows.rs`: `first_sync_backfills_the_latest_messages_then_only_new_ones_and_never_modifies_the_mailbox`,
`oversized_and_hostile_messages_are_skipped_without_stopping_the_sync`,
`a_renumbered_mailbox_restarts_the_sync_instead_of_losing_mail`. Manual: MT-10, MT-12, MT-16.

### US-36: Be told when a newer version exists

> **Epic:** onboarding · **Priority:** should · **Status:** partly (built, off until a repository is named) · **Requirements:** FR-24

**As** Mira, **I want** a notice when a newer version is out, **so that** I do not run an old build.

* Given no repository is set, then nothing is asked and nothing is sent.
* Given a newer release, then the notice shows its GitHub page, built from a checked tag and never taken from the answer.
* Given a pre-release or a draft, then nothing is shown. The app downloads nothing.

**Verified by:** engine `phase3.rs`: `the_check_is_off_until_a_repository_is_set`, `a_newer_release_is_reported_with_its_github_page`,
`up_to_date_drafts_and_prereleases_say_nothing`, `the_page_address_is_built_from_the_checked_tag_never_taken_from_the_response`.
Not verified against GitHub: the build names no repository (`UPDATE_REPO` is unset) and no release exists to ask.

### US-37: Install it with one installer

> **Epic:** onboarding · **Priority:** must · **Status:** partly (Windows built and run on the author's machine, not yet on a clean one; macOS and Linux never built) · **Requirements:** FR-25

**As** Dana, **I want** one installer that needs no administrator prompt and removes cleanly, **so that** I can try it and leave.

* Given the Windows installer, then it installs for the current user, adds a Start Menu entry, keeps running in the tray when the
  window closes, and focuses the running window on a second launch.
* Given an uninstall, then the program, shortcut and registry entry go and the data folder stays (the documents say so).

**Verified by:** no automated test. Manual: MT-60, MT-61, MT-62, MT-63, MT-64, MT-65. The checks of the version fields
(`python scripts/bump_version.py --check`) are in CI on all three systems; the macOS and Linux installers have never been built.

## Epic: triage and ranking

### US-03: See one list ranked by how much each message needs me

> **Epic:** triage and ranking · **Priority:** must · **Status:** built · **Requirements:** FR-03, FR-26

**As** Mira, **I want** every message to get a priority, a category, a due date and a "needs a reply" mark, **so that** the top of the list is what I should do first.

* Given a reply to something I sent, then it ranks high; a newsletter or no-reply sender ranks low; a stranger ranks in the middle.
* Given a due date stated after a cue word ("due", "by", "before"), then it becomes the due date; a date with no cue is ignored.
* Given an HTML-only message, then the task has readable text.
* Each task shows its attachment names (never the files) and a copy-only unsubscribe link.

**Verified by:** engine `engine.rs`: `reply_to_my_mail_is_high_priority`, `bulk_mail_is_low_priority`, `stranger_gets_middle_score`;
`features.rs`: `rules_sort_by_category_and_detect_questions`; `features2.rs`: `cued_dates_are_found_in_common_forms`,
`dates_without_a_cue_are_ignored`, `attachments_are_counted_and_inline_images_are_not`,
`unsubscribe_prefers_https_then_mailto_and_rejects_the_rest`; `edge_cases.rs`: `html_only_mail_becomes_readable_text`. Manual: MT-13,
MT-14, MT-15.

### US-04: Choose how mail is ranked

> **Epic:** triage and ranking · **Priority:** must · **Status:** built · **Requirements:** FR-04

**As** Tomas, **I want** to choose rules only, a local model or my own API key, **so that** I decide whether any mail text leaves my computer.

* Given rules only (the default), then nothing is sent to any model.
* Given a model that fails or answers nonsense, then the rules rank the message and no mail is lost.
* Given mail text that tries to close the prompt's wrapper, then it cannot.
* Given a ranking mode other than rules, ollama or anthropic, then the change is refused and nothing is saved.

**Verified by:** engine `engine.rs`: `failing_ai_falls_back_to_rules`; `edge_cases.rs`:
`model_replies_in_many_shapes_are_understood_or_refused_safely`; `security_more.rs`:
`email_text_cannot_close_the_untrusted_wrapper_in_the_model_prompt`; `phase11.rs`: `a_ranking_mode_that_does_not_exist_is_refused`. Manual: MT-31. A real model has not been run in a test.

### US-05: Not be fooled by what a message says about itself

> **Epic:** triage and ranking · **Priority:** must · **Status:** built · **Requirements:** FR-05

**As** Mira, **I want** words like "URGENT" to count for nothing, **so that** a phisher cannot put a message at the top.

* Given a stranger writing "urgent action required", then the message gets no priority 5 and no urgent flag.
* Given mail that failed its sender check, then "known sender" trust is withdrawn.
* Given a look-alike address or a display name, then it cannot climb in the ranking, and a rule on a look-alike does not raise it.

**Verified by:** engine `phase2.rs`: `a_phisher_writing_urgent_words_gets_nothing_from_them`; `security.rs`:
`failed_dkim_or_spf_cancels_known_sender_trust`; `phase4.rs`: `a_stranger_cannot_climb_with_a_look_alike_address_or_a_display_name`.
Manual: MT-18, MT-81.

### US-06: Have the list tidy itself

> **Epic:** triage and ranking · **Priority:** must · **Status:** built · **Requirements:** FR-06

**As** Mira, **I want** a task to close when I reply, and a conversation to be one row, **so that** I do not clean the list by hand.

* Given I replied from another device, then the task closes ("you replied") and nothing is written to the mailbox. A forward is not a reply.
* Given several messages in one conversation, then there is one row with the highest priority.
* Given mail that left the inbox, then it is marked and folded away but stays open, and comes back if it returns.
* Given a new message in a snoozed conversation, then it wakes.

**Verified by:** engine `imap_flows.rs`: `a_task_closes_itself_when_you_replied_and_nothing_is_queued_or_changed`,
`forwarding_a_message_is_not_replying_to_it`, `mail_that_left_the_inbox_is_marked_but_stays_open_and_comes_back_if_it_returns`;
`phase2.rs`: `a_reply_chain_shares_one_thread_key_rooted_at_the_first_message`, `a_new_message_in_a_snoozed_thread_wakes_it`. Window:
`Show 150 more under an open fold draws more of the fold and none of the list above it` (window). Manual: MT-104.

### US-07: Have the ranking learn from my corrections

> **Epic:** triage and ranking · **Priority:** should · **Status:** built · **Requirements:** FR-07

**As** Mira, **I want** the app to learn when I move a sender's mail up or down, **so that** later mail from them lands where I want it.

* Given two corrections in the same direction, then later mail from that address shifts the same way, never by more than two steps.
* Given an undo, then the correction is removed, not balanced by an opposite one.
* Given mail that failed its sender check, then it neither teaches nor learns, and learning never makes anything urgent.
* Given a priority outside 1 to 5, or a task number too big for 32 bits, then nothing changes and nothing is learned (261 is not read as 5).

**Verified by:** engine `phase2.rs`: `two_corrections_in_the_same_direction_shift_later_mail_from_that_address`,
`a_shift_is_never_larger_than_two_steps`, `undo_removes_the_correction_instead_of_adding_an_opposite_one`,
`mail_that_failed_its_sender_check_neither_teaches_nor_learns`, `learning_never_makes_anything_urgent`; `phase11.rs`:
`a_number_too_big_for_its_type_is_refused_not_wrapped`.

### US-08: Mark a sender always important, or mute it

> **Epic:** triage and ranking · **Priority:** should · **Status:** built · **Requirements:** FR-07

**As** Mira, **I want** to say "always important" or "mute" for a sender, **so that** I decide once.

* Given a sender marked important or muted, then new and existing mail from them is ranked that way, and the choice beats what the app learned.
* Given a spoofed message from an important sender, then the choice does not rescue it.
* Given a muted sender, then their mail is priority 1 and is not marked as needing a reply, so no automatic draft is made.

**Verified by:** engine `features.rs`: `sender_preferences_override_the_model_for_new_and_existing_mail`; `phase2.rs`:
`a_sender_preference_wins_over_learning`; `review_fixes.rs`: `an_important_sender_preference_does_not_rescue_a_spoofed_message`,
`a_muted_sender_is_never_asked_for_a_reply`. Manual: MT-45, MT-71.

### US-09: Write my own rules

> **Epic:** triage and ranking · **Priority:** should · **Status:** built · **Requirements:** FR-08

**As** Dana, **I want** "if the address, domain or subject matches, set the priority or category, or file it", **so that** the app handles my regular mail my way.

* Given a domain rule, then it matches that domain and its subdomains but not a look-alike. The first matching rule is used.
* Given "Apply to my list", then a count is shown first, a dry run changes nothing, and what I already decided is left alone.
* Given a rule that files mail, then nothing it files can raise an alert or start a draft.

**Verified by:** engine `phase4.rs`: `a_domain_rule_matches_that_domain_and_its_subdomains_but_not_look_alikes`,
`the_first_matching_rule_is_the_one_used`, `applying_changes_open_mail_and_a_dry_run_changes_nothing`,
`applying_leaves_alone_what_the_user_already_decided`, `a_filing_rule_leaves_nothing_that_could_raise_an_alert_or_start_a_draft`.
Manual: MT-72, MT-73, MT-74, MT-81.

### US-10: Search inside messages

> **Epic:** triage and ranking · **Priority:** should · **Status:** built · **Requirements:** FR-16

**As** Mira, **I want** to find a word that is in the body of a message, **so that** I can find mail I only half remember.

* Given a word only in the first 3,000 characters of a stored message, then the message is found and tagged "In text" (desktop only).
* Given what I type, then quotes, `OR` and `*` are plain words, and a query is cut to 8 words and 100 characters.
* Given the phone, then it cannot search inside message text.
* Given a search, then the list starts again at 150 rows.

**Verified by:** engine `phase5.rs`: `search_finds_words_anywhere_in_a_message_and_wants_all_of_them`,
`what_you_type_is_never_search_syntax`, `a_query_is_limited_to_a_hundred_characters_and_eight_words`,
`search_is_a_desktop_command_the_phone_cannot_use`. Window:
`another tab, another sort order or a search starts the list again at 150 rows` (window). Manual: MT-87, MT-88.

### US-11: Be alerted only when something matters

> **Epic:** triage and ranking · **Priority:** must · **Status:** built · **Requirements:** FR-09

**As** Mira, **I want** alerts for important mail only, with quiet hours and a daily digest, **so that** the app does not become more noise.

* Given quiet hours, then normal mail does not alert and urgent mail does.
* Given a digest time, then one notice is sent each day after it, even if the app was closed at that time.
* Given the first read of a new mailbox, then nothing alerts for the backlog.
* Given a choice of categories, then only those raise alerts; an unknown category is refused.

**Verified by:** engine `features.rs`: `quiet_hours_handle_windows_that_wrap_midnight`, `the_digest_is_sent_once_a_day_after_its_time`;
`edge_cases.rs`: `digest_and_quiet_hours_at_the_boundaries`; `features2.rs`: `alert_categories_are_validated`. Manual: MT-11, MT-12,
MT-48, MT-49.

## Epic: acting on tasks

### US-12: Do everything from the keyboard

> **Epic:** acting on tasks · **Priority:** must · **Status:** built (screen-reader support partly) · **Requirements:** FR-21, NFR-08, NFR-11

**As** Mira, **I want** keys for every action on the list, **so that** my hands stay on the keyboard.

* Given the list, then `j` and `k` (or the arrows) move; `x` or `e` finishes; `1` to `5` sets the priority; `s` stars; `h` snoozes;
  `z` undoes; `?` shows the keys.
* Given a field or a dialog has the focus, then these keys do nothing; Escape closes a dialog.
* Given a row that has the keyboard focus, then `j` keeps moving and `x` finishes the highlighted row.

**Verified by:** engine `features.rs`: `task_commands_work_through_the_core`. Window:
`a row that has the keyboard focus does not take the keys back: j keeps moving and x finishes the highlighted row` (window),
`x finishes the highlighted task and z brings it back` (window),
`k on the first row and j on the last row bring the cursor back into view` (window),
`Escape goes back to Settings, and Escape again closes it` (window). Manual: MT-47, MT-136. No screen-reader test has been run, and
the Tab trap in dialogs has no test.

### US-13: Select several tasks and act on all of them

> **Epic:** acting on tasks · **Priority:** should · **Status:** built · **Requirements:** FR-15

**As** Mira, **I want** to select a range of rows and finish, star, snooze or mute them at once, **so that** a clean-up takes seconds.

* Given select mode (`v`), then Shift with `j` selects the rows between, and "All" selects the whole list.
* Given a shift-click, then it selects the drawn rows between two clicks and none that are not drawn.
* Given "Mute senders", then it asks twice.

**Verified by:** window: `select mode: All selects the whole list and Done finishes all of it, and one Undo takes them back even after a poll` (window),
`shift-click selects the drawn rows between two clicks and none that are not drawn` (window). Manual: MT-70, MT-71.

### US-14: Undo a bulk finish completely

> **Epic:** acting on tasks · **Priority:** must · **Status:** built · **Requirements:** FR-15

**As** Mira, **I want** one Undo to bring back every task a bulk action finished, **so that** a slip is never costly.

* Given "Done all automated", "Done all" for mail that left the inbox, or "Done all N" for a filtered list, then one Undo reopens every
  task it finished, even those the window no longer holds after a poll.
* Given a task finished one at a time, then Undo reopens it.

**Verified by:** window: `undo of 'Done all automated' reopens every task, even those the window no longer holds after a poll` (window),
`undo of 'Done all' for mail that left your inbox reopens every task too` (window),
`undo of 'Done all N' for a filtered list reopens every task, even those the window no longer holds after a poll` (window). Engine
`phase8.rs`: `a_finished_task_beyond_the_newest_200_is_still_kept_and_found_and_the_total_follows_a_finish`. Manual: MT-46, MT-133, MT-134.

### US-15: Get a reply draft, and never have it sent

> **Epic:** acting on tasks · **Priority:** must · **Status:** built · **Requirements:** FR-10

**As** Mira, **I want** a draft inside a task that needs a reply, **so that** I start from text and finish in my own mail program.

* Given a task that needs a reply, then `r` makes a template draft with the sender's first name and my signature, or an AI draft labelled as such.
* Given a draft I edited, then "Rewrite" asks before it replaces my edits.
* Given "Let Inbox Agent file mail" is on, then "Save to Drafts" puts one copy in the Drafts folder, threaded and flagged as a draft,
  and nothing is sent. A different Reply-To shows a warning first.
* Given a draft, then its headers cannot be injected, and at most the daily limit of AI drafts is written.

**Verified by:** engine `features.rs`: `without_a_model_the_draft_is_a_template_with_name_and_signature`,
`draft_message_headers_cannot_be_injected`; `imap_flows.rs`: `a_reply_draft_is_appended_to_drafts_flagged_and_threaded_and_nothing_is_sent`;
`review_fixes.rs`: `rewriting_never_silently_replaces_the_users_own_edits`, `a_reply_address_that_differs_from_the_sender_is_flagged`,
`saving_a_draft_is_queued_once_and_needs_a_usable_from_address`, `the_daily_ai_draft_budget_is_real_and_cannot_be_refunded`. Manual:
MT-30 to MT-38.

### US-16: Let the app file mail, only when I say so

> **Epic:** acting on tasks · **Priority:** should · **Status:** built · **Requirements:** FR-11

**As** Dana, **I want** done, star and priority to be applied in my mailbox when I turn that on, **so that** my mail program matches my list.

* Given the setting is off (the default), then nothing in the mailbox is touched and my changes wait.
* Given Gmail, then done removes the Inbox label and adds `AI/Done`, and priority sets one `AI/P#` label. Given another provider, then
  done moves the message to the Archive folder.
* Given a message that cannot be found by its exact Message-ID, then nothing changes.
* Given a server that keeps failing, then the change is retried a few times, then dropped and logged.

**Verified by:** engine `imap_flows.rs`: `gmail_actions_touch_exactly_the_right_message_and_nothing_else`,
`a_lookup_with_no_exact_match_changes_nothing`, `other_providers_archive_by_moving_and_fall_back_to_copy_and_delete_without_move`,
`a_failing_server_is_retried_a_few_times_then_the_item_is_dropped_and_logged`; `core.rs`: `actions_update_state_and_queue_mailbox_changes`;
`security.rs`: `message_id_with_encoded_crlf_cannot_carry_imap_commands`. Manual: MT-20 to MT-29, none run against a real server.

### US-17: Turn an invitation into a calendar file

> **Epic:** acting on tasks · **Priority:** could · **Status:** built · **Requirements:** FR-17

**As** Dana, **I want** an invitation's event on its task and a file I can open in my calendar, **so that** I add it myself.

* Given an invitation, then the task shows its event in my time zone (UTC times converted; Outlook zone names understood).
* Given "Save .ics", then a new file is written to Downloads, never over an existing one, built by the app and not copied from the message.
* Given a cancelled event, then it says so and offers no save.

**Verified by:** engine `phase5.rs`: `a_utc_invitation_is_read`, `a_cancelled_event_is_marked_and_a_reply_is_ignored`,
`saving_writes_a_new_file_each_time_and_never_overwrites_or_leaves_the_folder`,
`a_hostile_summary_or_place_cannot_add_properties_to_the_file`, `saving_an_event_is_a_desktop_command_and_refuses_what_it_should`.
Manual: MT-91 to MT-96, which need a real calendar and have not been run.

## Epic: your own tasks and reminders

### US-18: Keep my own tasks and notes in the same list

> **Epic:** your own tasks and reminders · **Priority:** should · **Status:** built · **Requirements:** FR-12

**As** Dana, **I want** to add a task that never came by email, with a note and a due date, **so that** one list holds everything.

* Given `a`, then a task with a title of up to 200 characters and a priority from 1 to 5 is added; it never queues a change to a mailbox.
* Given a note, then it is cleaned and kept to 2,000 characters; a due date can be set, changed and cleared on any task.
* Given "Reset" of a mailbox, then my own tasks stay; given "Sign out and reset", they go.

**Verified by:** engine `features2.rs`: `own_tasks_validate_input`, `own_tasks_never_queue_mailbox_changes`,
`notes_are_saved_cleaned_and_bounded`, `own_tasks_survive_a_mailbox_reset_but_not_sign_out`; `phase2.rs`:
`a_due_date_can_be_set_changed_and_cleared_on_any_task`, `only_your_own_tasks_can_be_renamed`.

### US-19: Be reminded, and repeat a task

> **Epic:** your own tasks and reminders · **Priority:** should · **Status:** built · **Requirements:** FR-13

**As** Dana, **I want** a reminder on my own task and a repeat (daily, weekly, monthly), **so that** rent and reviews do not slip.

* Given a reminder time, then it is announced once, and again only if I change it; one that came due while the app was closed is announced at start.
* Given a repeating task, then finishing it moves it to the next time and keeps it open (there is no undo for that).
* Given a monthly task on the 31st, then it keeps its day across short months and leap years.
* Given a mail task, then it takes no reminder (snooze is what mail has).

**Verified by:** engine `phase5.rs`: `a_reminder_fires_once_and_again_only_when_it_is_changed`,
`a_reminder_that_came_due_while_the_app_was_closed_fires_when_it_starts`,
`completing_a_repeating_task_moves_it_to_the_next_time_and_keeps_it_open`,
`a_monthly_task_set_for_the_31st_does_not_drift_and_handles_leap_years`,
`a_reminder_needs_a_real_time_that_has_not_passed_and_a_task_of_your_own`. Manual: MT-82 to MT-86, MT-97, MT-98 (a real notification at
a real minute is not covered by a test).

### US-20: Be reminded of mail nobody answered

> **Epic:** your own tasks and reminders · **Priority:** could · **Status:** built · **Requirements:** FR-14

**As** Mira, **I want** a "Waiting" task when a conversation I started goes unanswered, **so that** I follow up in time.

* Given follow-ups are on (off by default; 2, 3, 5 or 7 days), then an unanswered conversation becomes a task and nothing is written to the mailbox.
* Given a reply, then the task closes itself. My own nudge and an out-of-office reply are not answers.
* Given a server that cannot search, then no task is made falsely. Given the setting off, then no command is sent at all.

**Verified by:** engine `imap_flows.rs`: `an_unanswered_conversation_you_started_becomes_a_task_and_nothing_is_written`,
`an_out_of_office_reply_is_not_a_reply`, `your_own_nudge_in_all_mail_is_not_a_reply`, `off_means_no_commands_at_all`,
`a_server_that_cannot_search_is_unknown_never_a_false_followup`; `phase4.rs`: `followup_settings_accept_only_the_offered_waits`. Manual:
MT-75 to MT-80.

## Epic: several mailboxes

### US-21: Read up to five mailboxes as one list

> **Epic:** several mailboxes · **Priority:** should · **Status:** built · **Requirements:** FR-18

**As** Dana, **I want** my work and personal mailboxes in one list, **so that** I look in one place.

* Given a second mailbox, then its mail merges into the list, each task has a small tag with its name, and there is a filter chip for each. With one mailbox there are neither.
* Given the same address and server twice, or a sixth mailbox, then it is refused with a plain message and nothing changes.
* Given two servers that use the same message numbers, then they make two tasks.

**Verified by:** engine `phase6.rs`: `up_to_four_more_mailboxes_each_with_its_own_number`, `adding_a_mailbox_is_refused_when_it_should_be`,
`two_mailboxes_with_the_same_imap_uid_make_two_tasks`, `the_same_server_written_another_way_is_still_the_same_mailbox`. Window:
`adding a mailbox that is already there is refused with the engine's message and nothing changes` (window),
`with two mailboxes every task of a mailbox has its tag and a chip filters to it; with one there are neither` (window),
`the same message that reached two mailboxes is two rows, one in each` (window). Manual: MT-99, MT-100, MT-102, MT-107.

### US-22: Remove a mailbox and everything of it

> **Epic:** several mailboxes · **Priority:** should · **Status:** built · **Requirements:** FR-18

**As** Dana, **I want** to remove a mailbox and have its tasks, drafts and password go, **so that** nothing of it lingers.

* Given "Remove" twice to confirm, then the mailbox's tasks, drafts, waiting changes, follow-up records and keychain entry are deleted
  and the mailbox itself is untouched. The other mailboxes and my own tasks stay.
* Given the list was filtered to that mailbox, then the filter clears instead of showing an empty list.
* Given the first mailbox is removed, then the others keep working; given the last, the app asks for setup again.

**Verified by:** engine `phase6.rs`: `removing_a_mailbox_removes_everything_of_it_and_nothing_else`,
`removing_the_first_mailbox_keeps_the_others_and_removing_the_last_asks_for_setup_again`. Window:
`Remove asks twice, and then the mailbox and its tasks are gone and Settings lists one mailbox less` (window),
`removing the mailbox the list is filtered to clears the filter instead of showing an empty list` (window). Manual: MT-106, MT-108.

### US-23: Have one failing mailbox not stop the others

> **Epic:** several mailboxes · **Priority:** should · **Status:** built · **Requirements:** FR-18

**As** Dana, **I want** a mailbox with a revoked password to say so, **so that** the others keep arriving.

* Given a mailbox with no or a refused password, then Settings says what is wrong with it and the others sync.
* Given a new password pasted for it, then it resumes without a restart and without losing its list.
* The status shown is the worst of the mailboxes.

**Verified by:** engine `phase6.rs`: `a_mailbox_with_no_password_says_so_and_does_not_stop_the_other_from_starting`,
`the_status_shown_is_the_worst_of_the_mailboxes`,
`another_mailbox_can_get_a_new_password_without_losing_its_list_and_a_new_address_resets_only_it`. Manual: MT-105.

### US-24: Have a change reach only the mailbox it belongs to

> **Epic:** several mailboxes · **Priority:** must · **Status:** built · **Requirements:** FR-18

**As** Dana, **I want** archiving a task to touch only its own mailbox, **so that** a same-named message elsewhere is safe.

* Given write-back and a task of the second mailbox marked done, then it is archived there only.
* Given a reply sent from one mailbox, then it closes that mailbox's task and not the other's.

**Verified by:** engine `imap_flows.rs`: `write_back_of_one_mailbox_reaches_only_its_own_server_and_leaves_the_others_queue`,
`a_reply_you_sent_from_one_mailbox_closes_that_mailboxs_task_and_not_the_other`; `phase6.rs`:
`queued_mailbox_changes_go_to_the_mailbox_they_belong_to`. Manual: MT-103, MT-104.

## Epic: backup and restore

### US-25: Save a backup of what I did

> **Epic:** backup and restore · **Priority:** should · **Status:** built · **Requirements:** FR-19

**As** Tomas, **I want** one file with my rules, choices, own tasks and what I did with mail, **so that** a new computer is not a fresh start.

* Given "Save a backup file", then a new `inbox-agent-backup-<date>-<time>.json` is written to Downloads, never over another, and its path is shown.
* The file holds no password, no API key, no access code, no notification topic and no message text.
* Given more entries than a file can hold, then the message says how many were left out.

**Verified by:** engine `phase7.rs`: `a_backup_holds_what_you_did_and_nothing_secret`, `the_file_is_new_never_over_another_and_named_by_time`,
`a_backup_says_what_it_had_to_leave_out`, `only_reply_drafts_you_edited_go_in_a_backup`. Window:
`Save a backup file says where the file is and that nothing was left out` (window),
`when the file holds only so many of something, the message says what was left out` (window),
`a backup that fails to save says why and does not claim it saved` (window). Manual: MT-112, MT-113, MT-127.

### US-26: Restore from a backup, with a preview, and lose nothing

> **Epic:** backup and restore · **Priority:** should · **Status:** built · **Requirements:** FR-19

**As** Tomas, **I want** to see what a restore will do before it does it, and for it to only add, **so that** I cannot make things worse.

* Given a file, then a preview lists what would be added and writes nothing; "Restore" is a second step, and Enter pressed by reflex restores nothing.
* Given a restore, then nothing is removed or overwritten, the same file restored twice changes nothing, and nothing is queued for the mailbox.
* Given one bad value in the file, then the whole restore is refused and nothing is half done.
* Given a file that is not a backup, is too large, or is from a newer version, then a plain message says so.

**Verified by:** engine `phase7.rs`: `a_backup_restores_into_a_fresh_install`,
`restoring_never_removes_or_overwrites_and_a_second_time_changes_nothing`, `a_preview_writes_nothing_and_tells_what_the_restore_then_does`,
`one_bad_value_refuses_the_whole_restore_and_nothing_is_half_done`, `a_file_that_is_not_ours_or_is_too_new_is_refused`,
`state_of_mail_never_goes_backwards_and_nothing_is_queued_for_the_mailbox`. Window:
`names what Restore will do before it does anything, and puts the keyboard on Cancel so Enter restores nothing` (window),
`while Restore is working Cancel and Escape do nothing; when it is done the dialog closes, says what came back, and the rule editor shows the restored rule` (window),
`rule changes that are not saved stop a restore from starting` (window),
`a file that is not one of this app's backups says so and cannot be restored` (window),
`a backup from a newer version of the app says to update` (window), `a file over 8 MB is refused before it is read` (window),
`a backup with nothing new in it says so and offers no Restore` (window). Manual: MT-114 to MT-117, MT-119 to MT-121, MT-123.

### US-27: Never let a file switch on something risky

> **Epic:** backup and restore · **Priority:** must · **Status:** built · **Requirements:** FR-19

**As** Tomas, **I want** a restored file never to turn on the phone hub, write-back or the paid ranking, **so that** a file someone gave me cannot widen what the app does.

* Given a file that asks for the hub, write-back or a notification address, then it is ignored; a file asking for ranking with Claude is
  not restored, and the preview says so.
* Given "Also restore options and mailboxes", then each mailbox is listed with its server first, and it comes back without a password.
* The options in a backup are an allow-list.

**Verified by:** engine `phase7.rs`: `a_file_cannot_turn_on_the_hub_write_back_or_a_network_destination`,
`a_file_cannot_switch_ranking_to_the_cloud_service_and_the_preview_says_so`, `what_is_in_the_options_of_a_backup_is_exactly_the_allow_list`,
`a_restored_mailbox_never_signs_in_with_a_password_left_over_from_another_one`. Window:
`with the box ticked the preview lists each mailbox with its server, warns about it, and says a paid ranking choice is not restored` (window).
Manual: MT-118, MT-122, MT-124 to MT-126.

## Epic: the phone

### US-28: Use the same list on my phone or tablet

> **Epic:** the phone · **Priority:** should · **Status:** built · **Requirements:** FR-20

**As** Dana, **I want** to scan a QR code and see my list on my tablet, **so that** I can finish tasks from the sofa.

* Given the hub on and the same Wi-Fi, then scanning the QR code opens the list and the access code leaves the address bar.
* Given the phone, then it sees the same tasks and no mailbox names or addresses, and it is offered no "write" or "save to Drafts" buttons.
* Given a change made on the phone, then the desktop shows it within a few seconds.

**Verified by:** engine `phase8.rs`: `the_phone_is_told_the_same`; `hub.rs`: `public_state_leaks_neither_account_nor_alert_secrets`,
`every_ui_file_is_served_and_nothing_else`; `phase6.rs`: `the_phone_never_sees_the_login_of_a_mailbox_not_even_in_the_log`;
`features.rs`: `the_phone_may_use_task_commands_but_not_settings_or_secrets`. Manual: MT-50, MT-51, MT-52, MT-109. The phone's layout has no test.

### US-29: Keep the phone's access safe

> **Epic:** the phone · **Priority:** must · **Status:** built (plain HTTP is a documented residual risk) · **Requirements:** FR-20, NFR-07

**As** Tomas, **I want** a stolen or guessed access code to do little harm, **so that** the hub is safe on my home network.

* Given 10 wrong access codes, then that address is refused for a minute.
* Given "Rotate access code", then the old code stops working at once. Turning the hub off and on issues a new one.
* Given a device outside private networks, then the hub refuses it.
* Given the phone, then it cannot spend AI quota, write to the mailbox, change settings, or make or restore a backup.
* Given a settings change that sets the access code to a value of its own, then it is refused: only the app makes the code.

**Verified by:** engine `hub.rs`: `guessing_tokens_gets_an_address_locked_out`, `rotating_the_token_kills_the_old_one`,
`oversized_and_malformed_requests_are_refused`, `the_demo_backend_is_never_served_to_the_network`; `phase1.rs`:
`the_hub_refuses_peers_outside_private_and_local_networks`; `review_fixes.rs`:
`a_stolen_phone_code_cannot_spend_ai_quota_or_write_to_the_mailbox`; `phase7.rs`: `the_phone_cannot_make_or_restore_a_backup`;
`phase11.rs`: `a_settings_change_cannot_set_the_hub_access_code`. Manual: MT-53, MT-54, MT-55.

## Epic: privacy and safety

### US-30: Be sure the app never sends mail and keeps my data here

> **Epic:** privacy and safety · **Priority:** must · **Status:** built · **Requirements:** NFR-01, NFR-03

**As** Tomas, **I want** the app to have no way to send mail and no server of its own, **so that** I can trust it with my mailbox.

* Given a reply draft, then saving it appends it to the Drafts folder and nothing is sent; the code has no SMTP.
* Given reading, then the mailbox is not modified.
* Given no repository named, then the update check sends nothing.

**Verified by:** engine `imap_flows.rs`: `a_reply_draft_is_appended_to_drafts_flagged_and_threaded_and_nothing_is_sent`,
`first_sync_backfills_the_latest_messages_then_only_new_ones_and_never_modifies_the_mailbox`; `phase3.rs`:
`the_check_is_off_until_a_repository_is_set`. That no SMTP code exists is checked by reading `engine/Cargo.toml` and searching the
source, not by a test. Manual: MT-33.

### US-31: Keep passwords in the operating system's keychain

> **Epic:** privacy and safety · **Priority:** must · **Status:** built · **Requirements:** NFR-02

**As** Tomas, **I want** app passwords and API keys in the keychain, never in a file, **so that** copying a folder copies no secret.

* Given a mailbox, then its password has its own keychain entry; a keychain that cannot be read is not reported as a missing password.
* Given a new password saved, then the list, notes and own tasks stay.
* Given any test run, then the real keychain is never touched.

**Verified by:** engine `phase1.rs`: `keychain_errors_are_told_apart_from_a_missing_entry`; `phase6.rs`:
`every_mailbox_has_its_own_keychain_entry_and_sign_out_forgets_them_all`, `the_keychain_is_never_the_real_one_when_the_tests_run`;
`phase3.rs`: `a_new_password_can_be_saved_without_wiping_anything`; `phase7.rs`: `a_backup_holds_what_you_did_and_nothing_secret`. Manual:
MT-106 (the keychain entry is gone).

### US-32: Sign out and leave nothing behind

> **Epic:** privacy and safety · **Priority:** must · **Status:** built · **Requirements:** FR-22

**As** Tomas, **I want** "Sign out and reset" to remove my data, **so that** I can hand the computer on.

* Given the command, then every mailbox password, the task list, drafts, log, corrections, rules, follow-up records, the database
  backup and the crash log are removed, and no message text remains in the database files or the search index.
* Files I saved to Downloads (a backup, a calendar file) stay: they are mine.
* Given the window, then what it had read and what could be undone is forgotten.

**Verified by:** engine `review_fixes.rs`: `signing_out_wipes_everything_the_app_stored`; `phase1.rs`:
`sign_out_leaves_no_mail_text_in_the_database_files`, `sign_out_removes_backups_set_aside_databases_and_the_crash_log`; `phase5.rs`:
`signing_out_leaves_no_message_text_in_the_index`; `phase6.rs`: `signing_out_removes_every_mailbox_and_every_password`. Window:
`signing out forgets what was read and what could be undone` (window). Manual: MT-39, MT-89.

### US-33: Survive hostile mail and a damaged database

> **Epic:** privacy and safety · **Priority:** must · **Status:** built · **Requirements:** NFR-06

**As** Mira, **I want** a strange message or a broken file never to take the app down or lose my list, **so that** I can depend on it.

* Given a Message-ID with encoded line breaks or deeply nested parts, then no IMAP command is injected and the process does not crash.
* Given a message that panics the parser, then it is skipped and the rest of the sync continues.
* Given a damaged database, then it is set aside and rebuilt; given one from a newer version, then it is refused and left untouched; an
  upgrade keeps a copy first.

**Verified by:** engine `security.rs`: `message_id_with_encoded_crlf_cannot_carry_imap_commands`,
`deeply_nested_multipart_does_not_crash_the_process`; `imap_flows.rs`:
`a_message_that_panics_is_skipped_and_the_rest_of_the_sync_continues`; `phase1.rs`: `a_damaged_database_is_set_aside_and_rebuilt`,
`data_from_a_newer_version_is_refused_and_left_untouched`, `upgrading_an_older_database_keeps_a_backup`; `edge_cases.rs`:
`hostile_and_unicode_text_is_stored_verbatim_and_safely`. Manual: MT-16, MT-17.

### US-34: Copy diagnostics without private information

> **Epic:** privacy and safety · **Priority:** should · **Status:** built · **Requirements:** FR-23

**As** Mira, **I want** a report I can paste into a bug report with my names and subjects removed, **so that** I can ask for help safely.

* Given "Copy diagnostics", then the report has counts, option names and recent activity, and no address, subject, folder name or user path.
* Given a status with an error that carries user data, then it is redacted. The command is desktop only.

**Verified by:** engine `phase3.rs`: `the_report_has_the_facts_a_bug_report_needs_and_nothing_private`,
`addresses_quotes_and_user_paths_never_survive_redaction`, `diagnostics_is_desktop_only_and_has_nothing_private_end_to_end`,
`folder_names_and_custom_server_names_are_not_in_the_report`; `phase11.rs`: `the_diagnostics_report_does_not_mention_google_sign_in`.

## Epic: performance

### US-35: Keep the app fast with thousands of tasks

> **Epic:** performance · **Priority:** must · **Status:** partly (one target missed by a hair) · **Requirements:** NFR-05

**As** Mira, **I want** the list to stay quick with a big mailbox, **so that** a heavy inbox does not make the app sluggish.

* Given thousands of tasks, then the window is sent every open task and only the newest 200 finished ones, and says how many there are in all.
* Given a long list, then 150 rows are drawn at a time with "Show 150 more" and "Show all N more"; the cursor draws more as it reaches them.
* Given a key press, then the cursor moves by changing classes, not by drawing the page; only the open row has its detail drawn.
* Given the Done tab, then it says how many of how many it shows and can show older ones.
* The targets and what was measured are in [the product requirements](prd.md); the poll answer is 127 ms and 2.6 MB against an aim of
  120 ms and 2.5 MB.

**Verified by:** engine `phase8.rs`: `the_window_gets_every_open_task_and_only_the_newest_finished_ones`,
`every_open_task_arrives_however_many_there_are`, `the_window_can_ask_for_more_finished_tasks_up_to_a_limit`,
`what_the_window_may_ask_for_is_bounded`. Window: `draws 150 rows at first, with Show more and Show all under them` (window),
`a cursor key moves the cursor by changing classes, without drawing the page again` (window),
`only the open row has its detail drawn` (window),
`the Done tab shows the newest finished tasks, says how many there are, and can show older ones` (window). Packaged app:
`python scripts/scale_check/scale.py 3000 5000`. Manual: MT-128 to MT-135.

## Not built yet

These have no tests. Each is a decision or work the owner holds (see [the open items](prd.md#open-items-the-owner-holds)).

### US-38: Sign in with Google instead of an app password

> **Epic:** onboarding · **Priority:** could · **Status:** not built (deferred) · **Requirements:** FR-27

**As** Mira, **I want** to sign in with my Google account, **so that** I do not have to create an app password.

* Not designed to acceptance level. [Phase 3](../designs/phase3.md) lists what it needs: a client ID each person creates, an owner
  decision on the paid assessment a shared client would need, and a design review before any code.

**Verified by:** nothing yet.

### US-39: Update the app automatically

> **Epic:** onboarding · **Priority:** could · **Status:** not built · **Requirements:** FR-28

**As** Mira, **I want** new versions to install themselves, **so that** I stay current.

* Needs a repository, an updater key pair and a `latest.json` on each release ([RELEASING.md](../../RELEASING.md), "What is not set up yet").

**Verified by:** nothing yet.

### US-40: Use an Outlook or Microsoft 365 account

> **Epic:** onboarding · **Priority:** could · **Status:** not built, not planned · **Requirements:** FR-29

**As** a person with a Microsoft account, **I want** to connect it, **so that** I can use the app.

* Microsoft turned off password sign-in for IMAP, so the wizard says the account is not supported. A different sign-in design would
  be needed first.

**Verified by:** manual MT-06 checks the wizard's message; nothing checks a connection, because there is none.
