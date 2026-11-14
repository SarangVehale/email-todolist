//! Phase 11: closing the gaps that writing the documentation found (design: docs/designs/phase11.md).
use engine::core::Core;
use engine::secrets;
use engine::settings::Settings;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

fn fresh(name: &str) -> (Arc<Core>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("ia-p11-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    (Core::new(dir.clone(), Arc::new(|_, _| {})), dir)
}

fn err(c: &Arc<Core>, cmd: &str, args: Value) -> String {
    c.rpc(cmd, &args).unwrap_err().to_string()
}

fn task(c: &Arc<Core>, uid: u64) -> Value {
    let state = c.rpc("state", &json!({})).unwrap();
    state["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["uid"] == json!(uid))
        .cloned()
        .expect("the task is in the list")
}

/// A number that does not fit the type the engine keeps it in used to be cut to fit: task 2^32 + n acted on task n, priority
/// 261 was read as 5, and port 66529 was read as 993. Each must be refused instead.
#[test]
fn a_number_too_big_for_its_type_is_refused_not_wrapped() {
    let (c, dir) = fresh("wrap");
    let uid = c
        .rpc("add_task", &json!({"title": "Call the bank"}))
        .unwrap()
        .as_u64()
        .unwrap();
    assert_eq!(task(&c, uid)["priority"], json!(3));
    let wrapped = uid + (1u64 << 32);

    for (cmd, args) in [
        ("set_priority", json!({"uid": wrapped, "priority": 5})),
        ("set_done", json!({"uid": wrapped, "done": true})),
        ("set_star", json!({"uid": wrapped, "star": true})),
        ("set_priority", json!({"uid": uid, "priority": 261})),
    ] {
        assert_eq!(c.rpc(cmd, &args).unwrap(), json!(false), "{cmd} {args}");
    }
    let t = task(&c, uid);
    assert_eq!(
        (
            t["priority"].clone(),
            t["status"].clone(),
            t["star"].clone()
        ),
        (json!(3), json!("open"), json!(false)),
        "nothing changed"
    );
    assert_eq!(
        c.rpc("set_priority", &json!({"uid": uid, "priority": 5}))
            .unwrap(),
        json!(true),
        "a real change still works"
    );

    let account = |port: u64| json!({"host": "127.0.0.1", "port": port, "user": "a@b.example", "password": "p"});
    for cmd in ["test_account", "save_account"] {
        assert!(
            err(&c, cmd, account(66529)).contains("port"),
            "{cmd}: 66529 is not 993"
        );
        assert!(err(&c, cmd, account(0)).contains("port"), "{cmd}: port 0");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// What a request may give as the port: none or null is 993; a whole number from 1 to 65535 is a port; anything else, even a
/// number in quotes, is refused. (A bad server name is the second thing `test_account` checks, so it tells a port that passed.)
#[test]
fn a_port_is_a_whole_number_or_nothing() {
    let (c, dir) = fresh("port");
    let base = json!({"host": "a b", "user": "a@b.example", "password": "p"});
    let with = |port: Value| {
        let mut b = base.clone();
        if !port.is_null() {
            b["port"] = port;
        }
        err(&c, "test_account", b)
    };
    for fine in [json!(null), json!(1), json!(993), json!(65535)] {
        assert!(
            with(fine.clone()).contains("server name"),
            "{fine} passes the port check"
        );
    }
    let mut null_port = base.clone();
    null_port["port"] = json!(null);
    assert!(
        err(&c, "test_account", null_port).contains("server name"),
        "a null port is 993"
    );
    for bad in [
        json!(0),
        json!(65536),
        json!(66529),
        json!(-1),
        json!(993.5),
        json!("993"),
        json!(true),
    ] {
        assert!(with(bad.clone()).contains("port"), "{bad} is not a port");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// The snooze error told people to pick "from tomorrow" although today is accepted while 09:00 is still ahead.
#[test]
fn the_snooze_date_error_says_what_is_accepted() {
    let (c, dir) = fresh("snooze");
    let uid = c
        .rpc("add_task", &json!({"title": "Later"}))
        .unwrap()
        .as_u64()
        .unwrap();
    let e = err(&c, "set_snooze", json!({"uid": uid, "date": "2000-01-01"}));
    assert!(e.contains("from today") && e.contains("09:00"), "{e}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_ranking_mode_that_does_not_exist_is_refused() {
    let (c, dir) = fresh("mode");
    let e = err(
        &c,
        "save_settings",
        json!({"patch": {"ai": {"mode": "bogus"}}}),
    );
    assert!(e.contains("Ranking mode"), "{e}");
    for ok in ["rules", "ollama", "anthropic"] {
        c.rpc("save_settings", &json!({"patch": {"ai": {"mode": ok}}}))
            .unwrap();
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// The phone hub's access code is made by the app. A settings change used to be able to set it to any string, such as "a".
#[test]
fn a_settings_change_cannot_set_the_hub_access_code() {
    let (c, dir) = fresh("token");
    let e = err(
        &c,
        "save_settings",
        json!({"patch": {"hub": {"enabled": true, "token": "abc"}}}),
    );
    assert!(e.contains("access code"), "{e}");
    assert_eq!(c.settings().hub.token, "", "nothing was saved");

    // with the hub on there is a code, and only the app changes it
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
        .max(20000);
    let save = |hub: Value| c.rpc("save_settings", &json!({"patch": {"hub": hub}}));
    save(json!({"enabled": true, "port": port})).unwrap();
    let code = c.settings().hub.token;
    assert!(!code.is_empty(), "switching the hub on made a code");
    save(json!({"token": code})).expect("the code that is already there is not a change");
    for bad in [
        json!({"token": "abc"}),
        json!({"token": ""}),
        // the whole hub section as a list, which serde reads field by field: still a change of the code
        json!([true, port, "abc"]),
    ] {
        let e = save(bad.clone()).unwrap_err().to_string();
        assert!(e.contains("access code"), "{bad}: {e}");
        assert_eq!(c.settings().hub.token, code, "{bad}: the code is as it was");
    }
    save(json!({"enabled": false})).unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

/// A saved settings file with a ranking mode that is none of the three (hand-edited) reads as "rules": otherwise the check on
/// the mode would refuse every later save, including a password change that has nothing to do with it.
#[test]
fn a_saved_ranking_mode_that_does_not_exist_reads_as_rules() {
    let (c, dir) = fresh("loadmode");
    let mut s = Settings::default();
    s.ai.mode = "bogus".into();
    s.save(&dir).unwrap();
    assert_eq!(c.settings().ai.mode, "rules");
    c.rpc(
        "save_settings",
        &json!({"patch": {"updates": {"check": false}}}),
    )
    .expect("an unrelated save is not refused for the mode");
    assert_eq!(c.settings().ai.mode, "rules");
    let _ = std::fs::remove_dir_all(dir);
}

/// "Test connection" and "Detect" took whatever they were given: a host with a space, an empty password, a web address that is
/// not http(s).
#[test]
fn test_account_and_detect_ollama_check_what_they_are_given() {
    let (c, dir) = fresh("inputs");
    let base = json!({"host": "127.0.0.1", "port": 993, "user": "a@b.example", "password": "p"});
    let with = |k: &str, v: &str| {
        let mut b = base.clone();
        b[k] = json!(v);
        b
    };
    assert!(err(&c, "test_account", with("host", "a b")).contains("server name"));
    assert!(err(&c, "test_account", with("host", "")).contains("Enter the mail server"));
    assert!(err(&c, "test_account", with("user", "")).contains("Enter the mail server"));
    assert!(err(&c, "test_account", with("password", "  ")).contains("app password"));
    // an address saving would refuse: a control character, or too long to be one
    assert!(err(&c, "test_account", with("user", "a\u{7}@b.example")).contains("email address"));
    assert!(err(&c, "test_account", with("user", &"a".repeat(400))).contains("email address"));
    assert!(err(&c, "detect_ollama", json!({"url": "ftp://example.test"})).contains("http(s)"));
    assert_eq!(
        c.rpc("detect_ollama", &json!({"url": "http://127.0.0.1:1"}))
            .unwrap(),
        json!([]),
        "a valid address with nothing there is an empty list"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// save_account stored an empty password (which then reads as "needs a new app password"); finish_setup finished setup with no
/// mailbox at all.
#[test]
fn a_mailbox_needs_a_password_and_setup_needs_a_mailbox() {
    let (c, dir) = fresh("setup");
    let acct = |password: &str| json!({"host": "127.0.0.1", "port": 993, "user": "a@b.example", "password": password});
    assert!(err(&c, "finish_setup", json!({})).contains("first mailbox"));
    assert!(err(&c, "save_account", acct("   ")).contains("app password"));
    let mut no_host = acct("p");
    no_host["host"] = json!("");
    assert!(
        err(&c, "save_account", no_host).contains("Enter the mail server"),
        "an empty server is not a mailbox either"
    );
    assert!(
        c.settings().account.host.is_empty()
            && secrets::lookup(&secrets::imap_key(0)).unwrap().is_none(),
        "the refused mailboxes saved nothing: no server and no password"
    );
    c.rpc("save_account", &acct("p")).unwrap();

    // a server and an address, but the password is gone from the keychain, or the address is empty: still no mailbox
    let key = secrets::imap_key(0);
    secrets::delete(&key);
    assert!(
        err(&c, "finish_setup", json!({})).contains("first mailbox"),
        "no password, no mailbox"
    );
    secrets::set(&key, "p").unwrap();
    let mut s = Settings::load(&dir);
    let user = std::mem::take(&mut s.account.user);
    s.save(&dir).unwrap();
    assert!(
        err(&c, "finish_setup", json!({})).contains("first mailbox"),
        "no address, no mailbox"
    );
    s.account.user = user;
    s.save(&dir).unwrap();
    assert!(!c.settings().onboarded, "nothing finished setup");

    c.rpc("finish_setup", &json!({})).unwrap();
    assert!(c.settings().onboarded);

    // adding another mailbox takes a port the same way
    for port in [66529u64, 0] {
        let mut more = acct("p");
        more["port"] = json!(port);
        assert!(
            err(&c, "add_account", more).contains("port"),
            "add_account port {port}"
        );
    }
    let _ = c.rpc("sign_out", &json!({})); // stops the background threads and wipes the folder
    let _ = std::fs::remove_dir_all(dir);
}

/// The diagnostics report had an "Google sign-in" option from a feature that was never built.
#[test]
fn the_diagnostics_report_does_not_mention_google_sign_in() {
    let (c, dir) = fresh("diag");
    let report = c.rpc("diagnostics", &json!({})).unwrap();
    let report = report.as_str().unwrap();
    assert!(report.contains("Options: ranking"), "{report}");
    assert!(!report.contains("Google"), "{report}");
    let _ = std::fs::remove_dir_all(dir);
}
