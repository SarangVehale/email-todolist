//! Phase 10: what writing the documentation found in the code (design: docs/designs/phase10.md).
use engine::core::Core;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

fn fresh(name: &str) -> (Arc<Core>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("ia-p10-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    (Core::new(dir.clone(), Arc::new(|_, _| {})), dir)
}

/// The Test button of the ranking settings used to say "Works" for a model that was never chosen: with no model name
/// (or no key) the engine quietly ranks with rules, and rules never fail. It must say so instead.
#[test]
fn test_ai_says_so_when_no_model_or_key_is_chosen_instead_of_passing_on_rules() {
    let (c, dir) = fresh("test-ai");
    let save = |ai: serde_json::Value| {
        c.rpc("save_settings", &json!({"patch": {"ai": ai}}))
            .unwrap()
    };

    save(json!({"mode": "rules"}));
    let ok = c.rpc("test_ai", &json!({})).unwrap();
    assert!(
        ok["priority"].as_u64().is_some(),
        "rules only: the test works and shows what it ranked"
    );

    save(json!({"mode": "ollama", "ollama_model": ""}));
    let e = c.rpc("test_ai", &json!({})).unwrap_err().to_string();
    assert!(e.contains("No model is chosen"), "{e}");

    save(json!({"mode": "anthropic"}));
    let e = c.rpc("test_ai", &json!({})).unwrap_err().to_string();
    assert!(e.contains("No API key is saved"), "{e}");

    let _ = std::fs::remove_dir_all(dir);
}

/// The phone guide and the user stories promise that switching the hub off and on again gives a new access code
/// (and that saving other settings while it is on keeps the one you have scanned).
#[test]
fn switching_the_hub_off_and_on_gives_a_new_access_code() {
    let (c, dir) = fresh("hub-code");
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
        .max(20000);
    let patch = |hub: serde_json::Value| {
        c.rpc("save_settings", &json!({"patch": {"hub": hub}}))
            .unwrap()
    };

    patch(json!({"enabled": true, "port": port}));
    let first = c.settings().hub.token;
    assert_eq!(
        first.len(),
        32,
        "a code is made when the hub is switched on"
    );

    patch(json!({"port": port}));
    assert_eq!(c.settings().hub.token, first, "saving again keeps the code");

    patch(json!({"enabled": false}));
    assert_eq!(c.settings().hub.token, "", "switched off: no code is kept");

    patch(json!({"enabled": true, "port": port}));
    let second = c.settings().hub.token;
    assert_eq!(second.len(), 32);
    assert_ne!(second, first, "switched on again: a new code");

    patch(json!({"enabled": false}));
    let _ = std::fs::remove_dir_all(dir);
}
