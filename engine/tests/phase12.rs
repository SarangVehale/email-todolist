//! Phase 12: simple enough for a grandmother (design: docs/designs/phase12.md).
use engine::core::Core;
use engine::webpage;
use serde_json::json;
use std::sync::Arc;

fn fresh(name: &str) -> (Arc<Core>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ia-p12-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    (Core::new(dir.clone(), Arc::new(|_, _| {})), dir)
}

/// The window opens the provider's page where a mail password is made, and a message in its webmail. Only `https`, only a named host, nothing hidden
/// in the address: a bug or a hostile mail must not be able to send a person anywhere else.
#[test]
fn only_a_named_https_page_can_be_opened() {
    for ok in [
        "https://myaccount.google.com/apppasswords",
        "https://login.yahoo.com/account/security",
        "https://account.apple.com/account/manage",
        "https://app.fastmail.com/settings/security/integrations",
        "https://mail.google.com/mail/u/0/#search/rfc822msgid%3Aabc%40x",
        "https://MYACCOUNT.GOOGLE.COM",
        "https://outlook.live.com/mail/0/",
    ] {
        assert!(webpage::allowed(ok), "{ok} is a page we open");
    }
    for bad in [
        "http://myaccount.google.com/apppasswords",
        "https://evil.example/myaccount.google.com",
        "https://myaccount.google.com.evil.example/",
        "https://evil.example@myaccount.google.com/",
        "https://myaccount.google.com@evil.example/",
        "https://myaccount.google.com:8443/",
        "https://myaccount.google.com\\@evil.example/",
        "https://my%61ccount.google.com/",
        "https://google.com/",
        "https://sub.myaccount.google.com/",
        "javascript:alert(1)",
        "file:///C:/Windows/System32/calc.exe",
        "ms-msdt:/id PCWDiagnostic",
        "https://myaccount.google.com/\nHost: evil",
        "https://myaccount.google.com/ x",
        "",
        "https://",
        "myaccount.google.com",
    ] {
        assert!(!webpage::allowed(bad), "{bad:?} is not a page we open");
    }
    let long = format!("https://myaccount.google.com/{}", "a".repeat(2100));
    assert!(
        !webpage::allowed(&long),
        "an address over 2048 characters is refused"
    );
}

#[test]
fn the_command_opens_nothing_in_a_test_and_remembers_what_it_would_have_opened() {
    let (c, dir) = fresh("open");
    assert_eq!(
        c.rpc(
            "open_page",
            &json!({"url": "https://myaccount.google.com/apppasswords"})
        )
        .unwrap(),
        json!(true)
    );
    assert!(webpage::opened().contains(&"https://myaccount.google.com/apppasswords".to_string()));
    let e = c
        .rpc("open_page", &json!({"url": "https://evil.example/"}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("can't be opened"), "{e}");
    assert!(
        !webpage::opened().iter().any(|u| u.contains("evil")),
        "a refused address is not remembered as opened"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// A phone, or anyone holding its access code, must not be able to make the computer open a browser.
#[test]
fn the_phone_hub_cannot_open_a_page() {
    let (c, dir) = fresh("hub");
    let url = "https://myaccount.google.com/only-the-hub-test-asks-for-this";
    assert!(c.rpc_public("open_page", &json!({"url": url})).is_err());
    assert!(
        !webpage::opened().iter().any(|u| u == url),
        "the hub's refusal opened nothing"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// The window names the pages it opens in `app/ui/setup.js` (`page: "https://…"`). One that the engine does not allow would be a button that
/// does nothing, so every one written there is checked against the engine's own rule, and there has to be one for each provider that has a code to make.
#[test]
fn every_page_the_window_asks_to_open_is_one_the_engine_allows() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/ui/setup.js");
    let js = std::fs::read_to_string(&path).expect("app/ui/setup.js is in the repository");
    let pages: Vec<&str> = js
        .lines()
        .flat_map(|l| l.split("page: \"").skip(1))
        .filter_map(|rest| rest.split('"').next())
        .filter(|u| u.starts_with("https://"))
        .collect();
    assert!(
        pages.len() >= 4,
        "Gmail, iCloud, Yahoo and Fastmail each have a page: {pages:?}"
    );
    for u in &pages {
        assert!(
            webpage::allowed(u),
            "{u} is written in setup.js but the engine would refuse it"
        );
    }
}
