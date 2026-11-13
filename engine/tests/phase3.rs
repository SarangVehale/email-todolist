//! Phase 3: diagnostics a user can paste into a bug report, and the update check.
use engine::diag::{redact, report, ReportInput};

fn input<'a>(log: &'a [String], crash: &'a [String]) -> ReportInput<'a> {
    ReportInput {
        version: "0.1.0",
        os: "windows x86_64",
        status: "idle",
        open: 12,
        done: 40,
        gone: 3,
        ai_mode: "rules",
        writeback: false,
        hub: true,
        quiet_hours: true,
        google: false,
        host: "imap.gmail.com",
        log,
        crash,
    }
}

// ── redaction ──
#[test]
fn addresses_quotes_and_user_paths_never_survive_redaction() {
    for (raw, banned) in [
        (
            "connected as sarang@example.com; Sent folder “[Gmail]/Sent Mail”",
            vec!["sarang@example.com", "Sent Mail"],
        ),
        (
            "triaged “Invoice for John Smith” → P3, finance",
            vec!["John Smith", "Invoice"],
        ),
        (
            "sender boss@company.co.uk set to important",
            vec!["boss@company.co.uk"],
        ),
        (
            "connection lost (login failed: user sarang@example.com not allowed); retrying in 30s",
            vec!["sarang@example.com"],
        ),
        (
            "could not open C:\\Users\\saran\\AppData\\Local\\x.db",
            vec!["saran"],
        ),
        (
            "could not open /home/sarang/.local/share/x.db",
            vec!["sarang"],
        ),
        ("could not open /Users/sarang/Library/x.db", vec!["sarang"]),
        (
            "added task \"Call mum about the surgery\"",
            vec!["surgery", "mum"],
        ),
        ("closed “Re: lawyer letter”: you replied", vec!["lawyer"]),
        (
            "mailbox update “archive” failed: no such message <abc123@mail.example.com>",
            vec!["abc123@mail.example.com"],
        ),
    ] {
        let out = redact(raw);
        for b in banned {
            assert!(!out.contains(b), "{b:?} survived in {out:?} (from {raw:?})");
        }
    }
}

#[test]
fn redaction_keeps_the_shape_of_the_message_so_it_is_still_useful() {
    // the reason inside the brackets could be a server's text about the user, so it goes; the shape stays
    let out = redact("connection lost (timed out); retrying in 30s");
    assert_eq!(out, "connection lost (…); retrying in 30s");
    assert!(redact("skipped message 42: processing it crashed").contains("42"));
    let c = redact("connected as a@b.com; Sent folder “X”");
    assert!(
        c.starts_with("connected;") && !c.contains("a@b.com") && c.contains("Sent folder"),
        "{c}"
    );
}

#[test]
fn redaction_is_bounded_and_survives_hostile_text() {
    let long = "x".repeat(10_000);
    assert!(redact(&long).chars().count() <= 200);
    for nasty in [
        "",
        "“",
        "”“",
        "\"",
        "@",
        "a@",
        "@b",
        "\u{0}\u{1b}[31m",
        "“unterminated",
        "C:\\Users\\",
    ] {
        let _ = redact(nasty);
    }
    assert!(
        !redact("line\nwith\nbreaks").contains('\n'),
        "one log entry stays one line"
    );
    assert!(!redact("ctrl\u{7}char").contains('\u{7}'));
}

// ── the report ──
#[test]
fn the_report_has_the_facts_a_bug_report_needs_and_nothing_private() {
    let log = vec![
        "connected as sarang@example.com; Sent folder “[Gmail]/Sent Mail”".to_string(),
        "triaged “Invoice for John Smith” → P3, finance".to_string(),
    ];
    let crash = vec![
        "2026-11-13 10:00:00 [watcher] src/mail.rs:10 index out of bounds for sarang@example.com"
            .to_string(),
    ];
    let r = report(&input(&log, &crash));
    for must in [
        "0.1.0",
        "windows x86_64",
        "idle",
        "12 open",
        "40 done",
        "3 left the inbox",
        "rules",
        "imap.gmail.com",
    ] {
        assert!(r.contains(must), "{must} missing from:\n{r}");
    }
    for banned in ["sarang@example.com", "John Smith", "Invoice", "Sent Mail"] {
        assert!(!r.contains(banned), "{banned} leaked into:\n{r}");
    }
    assert!(
        r.contains("src/mail.rs:10"),
        "the crash location is the useful part"
    );
}

#[test]
fn the_report_redacts_a_status_that_carries_an_error_with_user_data() {
    let mut i = input(&[], &[]);
    i.status = "error: login failed for sarang@example.com";
    assert!(!report(&i).contains("sarang@example.com"));
}

#[test]
fn an_empty_report_still_reads_well() {
    let r = report(&input(&[], &[]));
    assert!(r.contains("Recent activity") && r.contains("none"));
}

// ── through the command layer ──
#[test]
fn diagnostics_is_desktop_only_and_has_nothing_private_end_to_end() {
    let dir = std::env::temp_dir().join(format!("inbox-p3-diag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let c = engine::core::Core::new(dir.clone(), std::sync::Arc::new(|_, _| {}));
    {
        let st = c.store().unwrap();
        let e = engine::mail::parse(1, b"From: Pat <pat@private.example>\r\nSubject: My secret surgery\r\nMessage-ID: <x@y>\r\n\r\nbody text\r\n").unwrap();
        let s = engine::signals::Signals::default();
        use engine::triage::Triager;
        st.add(
            &e,
            &engine::triage::RulesTriager.triage(&e, &s).unwrap(),
            &s,
        )
        .unwrap();
        st.log("connected as pat@private.example; Sent folder “Secret”")
            .unwrap();
    }
    std::fs::write(
        dir.join("crash.log"),
        "2026-11-13 10:00:00 [watcher] src/x.rs:1 boom for pat@private.example\n",
    )
    .unwrap();
    let r = c.rpc("diagnostics", &serde_json::json!({})).unwrap();
    let text = r.as_str().unwrap();
    assert!(
        text.contains("1 open") && text.contains("src/x.rs:1"),
        "{text}"
    );
    // the banned-word checks below mean nothing unless the lines really are in the report
    assert!(
        text.contains("triaged") && text.contains("connected"),
        "the activity lines are there:\n{text}"
    );
    // ... and were redacted in place, not dropped: the subject became an ellipsis and the crash line kept its location
    assert!(text.contains('…'), "redaction ran:\n{text}");
    assert!(
        text.contains("(message removed)"),
        "the panic text went, the location stayed:\n{text}"
    );
    for banned in [
        "pat@private.example",
        "secret surgery",
        "Secret",
        "body text",
    ] {
        assert!(!text.contains(banned), "{banned} leaked:\n{text}");
    }
    assert!(
        c.rpc_public("diagnostics", &serde_json::json!({})).is_err(),
        "a phone cannot ask for it"
    );
    let _ = std::fs::remove_dir_all(dir);
}

// ── update check ──
use engine::updates::{check, newer, Update};
use std::io::{Read, Write};
use std::net::TcpListener;

/// Serves one canned HTTP response per connection, forever, and returns the base address.
fn mock_server(response: Vec<u8>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let r = response.clone();
            std::thread::spawn(move || {
                let mut s = s;
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let _ = s.write_all(&r);
            });
        }
    });
    format!("http://{addr}")
}

fn json_ok(body: &str) -> Vec<u8> {
    format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes()
}

const REPO: &str = "someone/inbox-agent";

#[test]
fn version_comparison_follows_the_numbers_not_the_text() {
    for (cur, tag, expect) in [
        ("0.1.0", "v0.1.1", true),
        ("0.1.0", "0.2.0", true),
        ("0.9.0", "v0.10.0", true),
        ("1.0.0", "v0.99.99", false),
        ("0.1.0", "v0.1.0", false),
        ("0.1.0", "0.1.0+build5", false),
        ("0.1.0", "v0.2.0-beta.1", false),
        ("0.2.0-beta.1", "v0.2.0-beta.2", false), // only x.y.z is compared: same core, not newer
        ("0.1.0-beta.1", "v0.2.0-beta.1", true),
        ("0.1.0", "latest", false),
        ("0.1.0", "", false),
        ("0.1.0", "v1", false),
        ("0.1.0", "1.2.3.4", false),
        ("0.1.0", "v1.2.x", false),
        ("0.1.0", "99999999999.0.0", false),
        ("0.1.0", "-1.0.0", false),
        ("garbage", "v9.9.9", false),
    ] {
        assert_eq!(newer(cur, tag), expect, "{cur} vs {tag}");
    }
}

#[test]
fn a_newer_release_is_reported_with_its_github_page() {
    let base = mock_server(json_ok(&format!(
        r#"{{"tag_name":"v0.2.0","html_url":"https://github.com/{REPO}/releases/tag/v0.2.0","draft":false}}"#
    )));
    let u = check(&base, REPO, "0.1.0").unwrap();
    assert_eq!(
        u,
        Some(Update {
            tag: "v0.2.0".into(),
            version: "0.2.0".into(),
            url: format!("https://github.com/{REPO}/releases/tag/v0.2.0")
        })
    );
}

#[test]
fn up_to_date_drafts_and_prereleases_say_nothing() {
    let same = mock_server(json_ok(
        r#"{"tag_name":"v0.1.0","html_url":"https://github.com/someone/inbox-agent/releases/tag/v0.1.0"}"#,
    ));
    assert_eq!(check(&same, REPO, "0.1.0").unwrap(), None);
    let draft = mock_server(json_ok(r#"{"tag_name":"v9.0.0","draft":true}"#));
    assert_eq!(check(&draft, REPO, "0.1.0").unwrap(), None);
    let pre = mock_server(json_ok(r#"{"tag_name":"v9.0.0-rc.1"}"#));
    assert_eq!(check(&pre, REPO, "0.1.0").unwrap(), None);
}

#[test]
fn the_page_address_is_built_from_the_checked_tag_never_taken_from_the_response() {
    for evil in [
        "https://evil.example/releases/tag/v0.2.0",
        "javascript:alert(1)",
        "https://github.com/other/repo/releases/tag/v0.2.0",
        "https://github.com/someone/inbox-agent-evil/releases/tag/v0.2.0",
        "https://github.com.evil.example/someone/inbox-agent/releases/x",
        "",
    ] {
        let body = serde_json::json!({"tag_name": "v0.2.0", "html_url": evil}).to_string();
        let base = mock_server(json_ok(&body));
        let u = check(&base, REPO, "0.1.0")
            .unwrap()
            .expect("a newer release is still reported");
        assert_eq!(
            u.url,
            format!("https://github.com/{REPO}/releases/tag/v0.2.0"),
            "{evil:?}"
        );
    }
}

#[test]
fn tags_that_are_not_plain_versions_are_never_used() {
    for tag in [
        "v0.2.0/../../evil",
        "v0.2.0 ",
        "v0.2.0\r\nHost: evil",
        "v0.2.0?x=1",
        "v0.2.0#frag",
        "v0.2.0-rc/1",
        "v0.2.0-",
        "v0.2.0-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "v0.2.0+build",
        "0.2",
        "v0.2.0.1",
        "<script>",
    ] {
        assert_eq!(
            engine::updates::release(REPO, "0.1.0", tag),
            None,
            "{tag:?}"
        );
    }
    assert!(engine::updates::release(REPO, "0.1.0", "v0.2.0").is_some());
    assert!(engine::updates::release(REPO, "0.1.0-beta", "v0.2.0-rc.1").is_some());
}

#[test]
fn what_the_daily_check_stored_is_shown_only_while_it_is_still_newer() {
    use engine::updates::pending;
    assert!(pending(Some("v0.2.0"), REPO, "0.1.0").is_some());
    assert!(
        pending(Some("v0.2.0"), REPO, "0.2.0").is_none(),
        "the user updated"
    );
    assert!(pending(Some("v0.2.0"), REPO, "0.3.0").is_none());
    assert!(pending(Some(""), REPO, "0.1.0").is_none());
    assert!(pending(None, REPO, "0.1.0").is_none());
    assert!(
        pending(Some("../../x"), REPO, "0.1.0").is_none(),
        "a tampered stored value is not trusted"
    );
    assert!(pending(Some("v0.2.0"), "not a repo", "0.1.0").is_none());
}

#[test]
fn failures_are_errors_to_ignore_never_panics_or_false_alarms() {
    let responses: Vec<(&str, Vec<u8>)> = vec![
        ("404", b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()),
        ("403 rate limited", b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()),
        ("500", b"HTTP/1.1 500 Oops\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()),
        ("redirect", b"HTTP/1.1 302 Found\r\nLocation: http://evil.example/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()),
        ("not json", json_ok("<html>not json</html>")),
        ("empty", json_ok("")),
        ("wrong shape", json_ok("[1,2,3]")),
        ("tag is a number", json_ok(r#"{"tag_name": 5}"#)),
        ("garbage", b"\x00\x01\x02 not http at all".to_vec()),
    ];
    for (name, r) in responses {
        let base = mock_server(r);
        let res = check(&base, REPO, "0.1.0");
        assert!(
            !matches!(res, Ok(Some(_))),
            "{name} must not report an update"
        );
    }
}

#[test]
fn an_oversized_response_is_cut_off_not_swallowed() {
    let big = format!(r#"{{"tag_name":"v0.2.0","pad":"{}"}}"#, "x".repeat(600_000));
    let base = mock_server(json_ok(&big));
    assert!(
        check(&base, REPO, "0.1.0").is_err(),
        "a truncated body is not valid JSON"
    );
}

#[test]
fn a_server_that_never_answers_times_out() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::sleep(std::time::Duration::from_secs(30));
            drop(s);
        }
    });
    let t = std::time::Instant::now();
    assert!(check(&format!("http://{addr}"), REPO, "0.1.0").is_err());
    assert!(
        t.elapsed() < std::time::Duration::from_secs(10),
        "{:?}",
        t.elapsed()
    );
}

#[test]
fn only_real_repository_names_are_ever_requested() {
    // a mock that fails the test if anything connects
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    l.set_nonblocking(true).unwrap();
    for bad in [
        "",
        "owner",
        "owner/",
        "/name",
        "a/b/c",
        "a b/c",
        "../x",
        "a/..",
        ".hidden/x",
        "a/b?x=1",
        "a/b#f",
        "ow ner/name",
        "a/b\r\nHost: evil",
    ] {
        assert!(
            check(&format!("http://{addr}"), bad, "0.1.0").is_err(),
            "{bad:?}"
        );
    }
    assert!(
        l.accept().is_err(),
        "no connection may be made for a bad repository name"
    );
}

#[test]
fn the_check_is_off_until_a_repository_is_set() {
    assert_eq!(
        engine::updates::UPDATE_REPO,
        "",
        "flip this when the project has a GitHub repository (see RELEASING.md)"
    );
}

// ── cases a design review found that the first redaction missed ──
#[test]
fn redaction_handles_quotes_inside_text_backticks_bare_errors_and_spaced_paths() {
    for (raw, banned) in [
        // a subject that itself contains the closing quote character
        (
            "triaged “He said “hello” to Jane Doe” → P3, personal",
            vec!["Jane", "hello", "He said"],
        ),
        // Rust's own panic text quotes the offending string in backticks
        (
            "byte index 5 is not a char boundary; it is inside `é` of `Salary review for Jane Doe`",
            vec!["Jane", "Salary", "é"],
        ),
        // errors with no quotes around the user's data
        (
            "could not parse message 7: invalid header from Jane Doe about the surgery",
            vec!["Jane", "surgery"],
        ),
        (
            "could not check for finished mail this time: NO [AUTHENTICATIONFAILED] Invalid credentials for jane",
            vec!["jane", "AUTHENTICATIONFAILED"],
        ),
        (
            "mailbox update “archive” failed: NO Label Salary-2026 not found",
            vec!["Salary"],
        ),
        (
            "connection lost (login failed: user jane.doe not allowed); retrying in 30s",
            vec!["jane", "not allowed"],
        ),
        // a login name that is not an address
        (
            "connected as jdoe; Sent folder “Sent Items”",
            vec!["jdoe", "Sent Items"],
        ),
        // a home folder with a space in it
        (
            "could not open C:\\Users\\Jane Doe\\AppData\\Local\\inbox\\agent.db",
            vec!["Jane", "Doe"],
        ),
        ("could not open /home/jane doe/data", vec!["jane", "doe"]),
    ] {
        let out = redact(raw);
        for b in banned {
            assert!(
                !out.contains(b),
                "{b:?} survived in {out:?} (from {raw:?})"
            );
        }
    }
    // the fixed phrases that carry no user data stay readable
    assert!(redact("skipped message 42: processing it crashed").contains("processing it crashed"));
    assert!(redact("skipped message 9: larger than 5 MB").contains("larger than 5 MB"));
    assert!(redact("connection lost (timed out); retrying in 30s").contains("retrying in 30s"));
}

#[test]
fn a_crash_line_keeps_its_time_thread_and_location_but_never_the_panic_text() {
    let line = "2026-11-13 10:00:00 [watcher] src/mail.rs:10 byte index 3 is not a char boundary in `Divorce papers for Jane Doe`";
    let out = engine::diag::redact_crash(line);
    assert!(
        out.contains("2026-11-13") && out.contains("[watcher]") && out.contains("src/mail.rs:10"),
        "{out}"
    );
    for banned in ["Divorce", "Jane", "boundary"] {
        assert!(!out.contains(banned), "{banned} in {out}");
    }
    assert!(engine::diag::redact_crash("").len() < 10);
    assert!(engine::diag::redact_crash("garbage").len() < 40);
}

// ── settings and state around the update check and the account ──
#[test]
fn only_gmails_own_hosts_count_as_gmail() {
    use engine::settings::is_gmail;
    for yes in [
        "imap.gmail.com",
        "IMAP.GMAIL.COM",
        "gmail.com",
        "imap.googlemail.com",
        "imap.gmail.com.",
    ] {
        assert!(is_gmail(yes), "{yes}");
    }
    for no in [
        "evilgmail.com",
        "imap.gmail.com.evil.example",
        "notgoogle.com",
        "gmail.com.evil.io",
        "xgooglemail.com",
        "",
        "mail.example.com",
    ] {
        assert!(!is_gmail(no), "{no}");
    }
}

fn settings_core(name: &str) -> (std::sync::Arc<engine::core::Core>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("inbox-p3-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    (
        engine::core::Core::new(dir.clone(), std::sync::Arc::new(|_, _| {})),
        dir,
    )
}

#[test]
fn a_settings_patch_cannot_change_the_account() {
    let (c, dir) = settings_core("acct");
    let err = c
        .rpc(
            "save_settings",
            &serde_json::json!({"patch": {"account": {"host": "evil.example"}}}),
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("save_account"), "{err}");
    assert!(
        c.rpc(
            "save_settings",
            &serde_json::json!({"patch": {"writeback": true}})
        )
        .is_ok(),
        "ordinary patches still work"
    );
    assert_eq!(c.settings().account.host, "");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_update_check_is_a_setting_and_stays_hidden_without_a_repository() {
    let (c, dir) = settings_core("upd");
    let st = c.rpc("state", &serde_json::json!({})).unwrap();
    assert_eq!(
        st["update_feature"],
        serde_json::json!(false),
        "no repository: the feature does not exist"
    );
    assert!(st["update"].is_null());
    assert!(
        c.settings().updates.check,
        "on by default (and a no-op until a repository is named)"
    );
    c.rpc(
        "save_settings",
        &serde_json::json!({"patch": {"updates": {"check": false}}}),
    )
    .unwrap();
    assert!(!c.settings().updates.check);
    // UPDATE_REPO is empty, so state() short-circuits before the gates; test the gates themselves
    use engine::updates::{pending, release};
    let repo = "someone/inbox-agent";
    assert!(
        release(repo, "0.1.0", "v0.2.0").is_some(),
        "with a repository, a newer tag is an update"
    );
    assert!(pending(Some("v0.2.0"), repo, "0.1.0").is_some());
    assert!(
        pending(Some("v0.2.0"), "", "0.1.0").is_none(),
        "no repository, no notice"
    );
    let public = c.rpc_public("state", &serde_json::json!({})).unwrap();
    assert!(
        public["update"].is_null(),
        "a phone is never shown update notices"
    );
    let _ = std::fs::remove_dir_all(dir);
}

// ── findings from the independent review of Phase 3 ──
#[test]
fn a_settings_patch_in_any_shape_cannot_change_the_account_or_onboarding() {
    let (c, dir) = settings_core("acct2");
    // seed an account the way save_account would
    let mut s = c.settings();
    s.account = engine::settings::AccountCfg {
        host: "imap.example.com".into(),
        port: 993,
        user: "me@example.com".into(),
    };
    s.onboarded = true;
    s.save(&c.dir).unwrap();

    // a list is read by serde in field order: onboarded, account, ... so it rewrites everything
    let as_list = serde_json::json!({"patch": [true, {"host": "evil.example", "port": 993, "user": "x@y.z"}]});
    assert!(
        c.rpc("save_settings", &as_list).is_err(),
        "a patch that is not an object must be refused"
    );
    let nested = serde_json::json!({"patch": {"account": {"host": "evil.example"}}});
    assert!(c.rpc("save_settings", &nested).is_err());
    let onboard = serde_json::json!({"patch": {"onboarded": false}});
    assert!(
        c.rpc("save_settings", &onboard).is_err(),
        "setup state is not a setting"
    );
    for bad in [
        serde_json::json!("string"),
        serde_json::json!(5),
        serde_json::json!(null),
        serde_json::json!(true),
    ] {
        assert!(
            c.rpc("save_settings", &serde_json::json!({"patch": bad}))
                .is_err(),
            "{bad}"
        );
    }
    let s = c.settings();
    assert_eq!(
        (
            s.account.host.as_str(),
            s.account.user.as_str(),
            s.onboarded
        ),
        ("imap.example.com", "me@example.com", true)
    );
    // ordinary settings still work and keep the account
    c.rpc(
        "save_settings",
        &serde_json::json!({"patch": {"writeback": true}}),
    )
    .unwrap();
    assert!(c.settings().writeback && c.settings().account.host == "imap.example.com");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn every_shape_of_address_is_removed_including_the_odd_ones() {
    for (raw, banned) in [
        (
            "sender jane.o'connor@example.com set to important",
            vec!["jane", "connor"],
        ),
        (
            "sender bounce-123=jane=example.com@list.org set to mute",
            vec!["jane", "bounce"],
        ),
        (
            "sender jane&co@example.com set to normal",
            vec!["jane", "co@"],
        ),
        (
            "sender first/last@example.com set to mute",
            vec!["first", "last"],
        ),
        ("sender !weird!@example.com set to mute", vec!["weird"]),
        ("sender jane{doe}@example.com set to mute", vec!["doe"]),
        (
            "sender \"Jane Doe\" <jane@example.com> set to mute",
            vec!["jane@", "Jane Doe"],
        ),
        (
            "sender jane@[192.168.1.5] set to mute",
            vec!["192.168", "jane"],
        ),
        (
            "sender jane@example.com, bob@other.net set to mute",
            vec!["jane", "bob", "other.net"],
        ),
    ] {
        let out = redact(raw);
        for b in banned {
            assert!(!out.contains(b), "{b:?} survived in {out:?} (from {raw:?})");
        }
        assert!(out.contains("set to"), "the useful shape stays: {out:?}");
    }
}

#[test]
fn safe_phrases_are_matched_exactly_not_merely_contained() {
    // a server reply or a sender name that happens to contain a safe phrase must not keep the whole line
    let out = redact("could not parse message 7: bad header Jane Doe you replied");
    assert!(!out.contains("Jane"), "{out}");
    let out = redact("could not check for finished mail this time: NO [SERVERBUG] processing it crashed for jane");
    assert!(!out.contains("jane"), "{out}");
    // the real lines still read well
    assert!(redact("skipped message 42: processing it crashed").contains("processing it crashed"));
    assert!(redact("skipped message 9: larger than 5 MB").contains("larger than 5 MB"));
}

#[test]
fn folder_names_and_custom_server_names_are_not_in_the_report() {
    let log = vec!["saved a reply draft to Drafts/Jane-Private for “x”".to_string()];
    let r = report(&ReportInput {
        host: "mail.janes-law-firm.example",
        log: &log,
        ..input(&[], &[])
    });
    assert!(
        !r.contains("Jane-Private"),
        "a folder name can be a person:\n{r}"
    );
    assert!(
        !r.contains("janes-law-firm"),
        "a private server name identifies the user:\n{r}"
    );
    assert!(r.contains("custom server"));
    // a well-known provider is useful and tells nobody anything
    let known = report(&ReportInput {
        host: "imap.gmail.com",
        ..input(&[], &[])
    });
    assert!(known.contains("imap.gmail.com"));
}
