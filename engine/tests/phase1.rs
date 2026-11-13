//! Phase 1 hardening: hostile HTML cannot stall the watcher, the crash log is bounded, and the database,
//! secrets and sign-out behave as promised.
use engine::mail::html_to_text;
use std::time::{Duration, Instant};

// ── HTML stripping stays fast on hostile input ──
fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed())
}

#[test]
fn a_megabyte_of_empty_comments_is_stripped_quickly() {
    // regression: the old stripper re-lowercased the whole string for every block (quadratic)
    let html = "<!---->".repeat(150_000); // about 1 MB
    let (out, took) = timed(|| html_to_text(&html));
    // the 256 KB cap can cut the last comment in half, leaving a few characters of it
    assert!(out.trim().len() < 8, "{:?}", out.trim());
    assert!(took < Duration::from_secs(2), "took {took:?}");
}

#[test]
fn many_script_and_style_blocks_are_stripped_quickly() {
    let html = "<script>x</script><style>y</style>z".repeat(40_000);
    let (out, took) = timed(|| html_to_text(&html));
    assert!(out.contains('z'));
    assert!(took < Duration::from_secs(2), "took {took:?}");
}

#[test]
fn unclosed_blocks_and_stray_angle_brackets_do_not_stall() {
    let html = "<script>".repeat(100_000) + &"<a ".repeat(100_000) + &"<!-- ".repeat(100_000);
    let (_, took) = timed(|| html_to_text(&html));
    assert!(took < Duration::from_secs(2), "took {took:?}");
}

#[test]
fn only_the_start_of_a_huge_html_body_is_read() {
    let html = format!("<p>hello</p>{}<p>THE-END</p>", "<b>x</b>".repeat(100_000));
    let out = html_to_text(&html);
    assert!(out.contains("hello"));
    assert!(
        !out.contains("THE-END"),
        "text beyond the 256 KB cap is ignored"
    );
}

#[test]
fn stripping_still_handles_ordinary_mail() {
    let out = html_to_text(
        "<html><head><title>T</title><style>p{}</style></head><body><!-- hi --><p>One</p><script>bad()</script><p>Two &amp; three</p></body></html>",
    );
    assert!(out.contains("One") && out.contains("Two & three"));
    assert!(!out.contains("bad()"));
    assert!(!out.contains(" hi "));
    assert!(!out.trim_start().starts_with('T'), "the title is dropped");
    assert!(!out.contains("p{}"));
}

#[test]
fn comments_hide_scripts_inside_them_and_scripts_hide_comment_markers() {
    let out = html_to_text("a<!-- <script> -->b<script>/* <!-- */</script>c");
    assert_eq!(out.split_whitespace().collect::<String>(), "abc");
}

// ── crash log ──
#[test]
fn the_crash_log_records_and_stays_bounded() {
    let dir = std::env::temp_dir().join(format!("inbox-diag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    engine::diag::record(
        &dir,
        "watcher",
        "src/x.rs:1",
        "boom\nwith\tcontrol\u{7}chars",
    );
    let text = std::fs::read_to_string(dir.join("crash.log")).unwrap();
    assert!(text.contains("watcher") && text.contains("boom with control chars"));
    assert_eq!(
        text.lines().count(),
        1,
        "newlines in a message cannot forge extra lines"
    );
    for _ in 0..600 {
        engine::diag::record(&dir, "t", "loc", &"x".repeat(1000));
    }
    let size = std::fs::metadata(dir.join("crash.log")).unwrap().len();
    assert!(
        size < 200 * 1024,
        "log restarts once it passes 100 KB, got {size}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── database health, schema version and a wipe that really wipes ──
use engine::store::{Store, SCHEMA_VERSION};
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("inbox-p1-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn add_task(st: &Store, uid: u32, subject: &str, body: &str) {
    use engine::triage::{RulesTriager, Triager};
    let raw =
        format!("From: a@b.com\r\nSubject: {subject}\r\nMessage-ID: <m{uid}@x>\r\n\r\n{body}\r\n");
    let e = engine::mail::parse(uid, raw.as_bytes()).unwrap();
    let s = engine::signals::Signals::default();
    st.add(&e, &RulesTriager.triage(&e, &s).unwrap(), &s)
        .unwrap();
}

#[test]
fn a_new_database_records_its_schema_version() {
    let d = scratch("ver");
    let p = d.join("agent.db");
    drop(Store::open(&p).unwrap());
    let v: i32 = rusqlite::Connection::open(&p)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, SCHEMA_VERSION);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn data_from_a_newer_version_is_refused_and_left_untouched() {
    let d = scratch("newer");
    let p = d.join("agent.db");
    drop(Store::open(&p).unwrap());
    rusqlite::Connection::open(&p)
        .unwrap()
        .execute_batch("PRAGMA user_version=99")
        .unwrap();
    let before = std::fs::read(&p).unwrap();
    let err = Store::open_recovering(&p)
        .err()
        .expect("must refuse")
        .to_string();
    assert!(err.contains("newer version"), "{err}");
    assert_eq!(
        std::fs::read(&p).unwrap(),
        before,
        "never quarantined or modified"
    );
    assert!(std::fs::read_dir(&d).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("corrupt")));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_damaged_database_is_set_aside_and_rebuilt() {
    let d = scratch("corrupt");
    let p = d.join("agent.db");
    std::fs::write(
        &p,
        b"this is not a sqlite file, it is just text ".repeat(300),
    )
    .unwrap();
    let (st, moved) = Store::open_recovering(&p).unwrap();
    let moved = moved.expect("the damaged file is reported");
    assert!(moved.exists() && moved.to_string_lossy().contains("corrupt"));
    add_task(&st, 1, "works again", "hello");
    assert!(st.get(1).is_some());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_healthy_database_is_not_touched() {
    let d = scratch("healthy");
    let p = d.join("agent.db");
    drop(Store::open(&p).unwrap());
    let (_, moved) = Store::open_recovering(&p).unwrap();
    assert!(moved.is_none());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn upgrading_an_older_database_keeps_a_backup() {
    let d = scratch("backup");
    let p = d.join("agent.db");
    drop(Store::open(&p).unwrap());
    rusqlite::Connection::open(&p)
        .unwrap()
        .execute_batch("PRAGMA user_version=0")
        .unwrap();
    drop(Store::open(&p).unwrap());
    assert!(
        d.join("agent.db.bak").exists(),
        "a copy is kept before migrating"
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn sign_out_leaves_no_mail_text_in_the_database_files() {
    let d = scratch("wipe");
    let p = d.join("agent.db");
    let st = Store::open(&p).unwrap();
    add_task(
        &st,
        1,
        "ZEBRA-SUBJECT-4471",
        "ZEBRA-BODY-9921 private words",
    );
    st.set_note(1, "ZEBRA-NOTE-1357").unwrap();
    st.wipe_all().unwrap();
    for f in ["agent.db", "agent.db-wal"] {
        if let Ok(bytes) = std::fs::read(d.join(f)) {
            let text = String::from_utf8_lossy(&bytes);
            for needle in ["ZEBRA-SUBJECT", "ZEBRA-BODY", "ZEBRA-NOTE"] {
                assert!(!text.contains(needle), "{needle} still in {f}");
            }
        }
    }
    drop(st);
    let _ = std::fs::remove_dir_all(d);
}

// ── keychain failures are not "no password" ──
#[test]
fn keychain_errors_are_told_apart_from_a_missing_entry() {
    use engine::secrets::classify;
    assert!(matches!(classify(Err(keyring::Error::NoEntry)), Ok(None)));
    assert!(matches!(classify(Ok(String::new())), Ok(None)));
    assert_eq!(classify(Ok("pw".into())).unwrap().as_deref(), Some("pw"));
    let locked = keyring::Error::NoStorageAccess("locked".into());
    assert!(
        classify(Err(locked)).is_err(),
        "an unavailable keychain must not look like no password"
    );
}

// ── the phone hub only answers local networks ──
#[test]
fn the_hub_refuses_peers_outside_private_and_local_networks() {
    use engine::hub::local_peer;
    use std::net::IpAddr;
    let yes = [
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.1.20",
        "169.254.3.4",
        "100.64.0.1",
        "100.127.255.255",
        "::1",
        "fd12:3456::1",
        "fe80::1",
        "::ffff:192.168.1.5",
    ];
    let no = [
        "8.8.8.8",
        "172.32.0.1",
        "172.15.0.1",
        "100.63.255.255",
        "100.128.0.1",
        "192.169.0.1",
        "1.1.1.1",
        "2001:db8::1",
        "2606:4700::1",
        "::ffff:8.8.8.8",
        "0.0.0.0",
    ];
    for ip in yes {
        assert!(
            local_peer(ip.parse::<IpAddr>().unwrap()),
            "{ip} should be allowed"
        );
    }
    for ip in no {
        assert!(
            !local_peer(ip.parse::<IpAddr>().unwrap()),
            "{ip} should be refused"
        );
    }
}

// ── fixes from the independent review of Phase 1 ──
#[test]
fn a_locked_database_is_an_error_not_corruption_and_stays_where_it_is() {
    let d = scratch("locked");
    let p = d.join("agent.db");
    drop(Store::open(&p).unwrap());
    let holder = rusqlite::Connection::open(&p).unwrap();
    // in the default journal mode an exclusive lock blocks readers too (in WAL mode readers are never blocked)
    holder
        .query_row("PRAGMA journal_mode=DELETE", [], |_| Ok(()))
        .unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let res = Store::open_recovering(&p);
    assert!(res.is_err(), "a locked database cannot be opened");
    assert!(p.exists(), "the healthy file is not renamed");
    assert!(
        std::fs::read_dir(&d).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("corrupt")),
        "nothing was set aside"
    );
    holder.execute_batch("ROLLBACK").unwrap();
    drop(holder);
    assert!(
        Store::open_recovering(&p).unwrap().1.is_none(),
        "it opens normally once the lock is gone"
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn sign_out_removes_backups_set_aside_databases_and_the_crash_log() {
    let d = scratch("leftovers");
    for f in [
        "agent.db.bak",
        "agent.db.corrupt-123",
        "agent.db.corrupt-123-wal",
        "crash.log",
    ] {
        std::fs::write(d.join(f), "ZEBRA mail text").unwrap();
    }
    std::fs::write(d.join("settings.json"), "{}").unwrap();
    std::fs::write(d.join("agent.db"), "live").unwrap();
    engine::store::remove_leftover_files(&d);
    let mut left: Vec<String> = std::fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    left.sort();
    assert_eq!(
        left,
        vec!["agent.db", "settings.json"],
        "the live database and settings are not touched here"
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn css_cut_off_by_the_size_cap_does_not_leak_into_the_text() {
    let html = format!("<p>hello</p><style>{}", "a{color:red}".repeat(60_000));
    let out = html_to_text(&html);
    assert!(out.contains("hello"));
    assert!(
        !out.contains("color:red"),
        "an unfinished block at the cut is dropped"
    );
    // a short document with an unfinished block keeps its text, as before
    let short = html_to_text("<p>hi</p><style>x");
    assert!(short.contains("hi") && short.contains('x'));
}

#[test]
fn forged_dates_cannot_crash_date_handling() {
    use engine::dates::{extract_due, received_date};
    for secs in [8.0e12, -1.0e12, f64::MAX, f64::NAN, f64::INFINITY, 0.0] {
        let d = received_date(secs);
        let _ = extract_due("due tomorrow, by friday, eod", d);
    }
    let e = engine::mail::parse(1, b"From: a@b.com\r\nSubject: x\r\nDate: Mon, 1 Jan 262143 12:00:00 +1400\r\n\r\nplease reply by tomorrow\r\n").unwrap();
    assert_eq!(
        e.received, 0.0,
        "an absurd Date header is treated as unknown"
    );
}
