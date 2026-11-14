//! Phase 8: a big mailbox stays fast (design: docs/designs/phase8.md).
use engine::core::Core;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

fn fresh(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ia-p8-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A computer with `open` open tasks and `done` finished ones. Finished task k was finished at time k (so the newest are the
/// highest numbers); one of the oldest has a word nobody else has.
fn busy(name: &str, open: usize, done: usize) -> (Arc<Core>, PathBuf) {
    let dir = fresh(name);
    let c = Core::new(dir.clone(), Arc::new(|_, _| {}));
    c.store().unwrap(); // makes the database
    let db = rusqlite::Connection::open(dir.join("agent.db")).unwrap();
    db.execute_batch(&format!(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<{total})
         INSERT INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,status,excerpt,why,signals,body,needs_reply,due,reply_to,refs,auth_fail,thread,done_at)
         SELECT i, '<t'||i||'@x>', 'S', 's@x.example', CASE WHEN i={oldest} THEN 'zebra777 the oldest finished' ELSE 'subject '||i END,
                1000+i, 3, 'work', '', 0, CASE WHEN i<={open} THEN 'open' ELSE 'done' END, '', '', '{{}}', '', 0, '', 's@x.example', '', 0,
                '<t'||i||'@x>', CASE WHEN i<={open} THEN 0 ELSE i END FROM n;",
        total = open + done,
        oldest = open + 1,
    ))
    .unwrap();
    (c, dir)
}

fn uids(v: &Value, status: &str) -> Vec<u64> {
    v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["status"] == status)
        .map(|t| t["uid"].as_u64().unwrap())
        .collect()
}

#[test]
fn the_window_gets_every_open_task_and_only_the_newest_finished_ones() {
    let (c, dir) = busy("window", 3, 250);
    let st = c.rpc("state", &json!({})).unwrap();
    assert_eq!(uids(&st, "open").len(), 3);
    let done = uids(&st, "done");
    assert_eq!(done.len(), 200, "the newest 200 finished tasks");
    assert!(
        done.iter().all(|u| *u >= 54),
        "finished tasks 54 to 253 are the newest 200: {done:?}"
    );
    assert_eq!(
        st["done_total"], 250,
        "and the window is told how many there are"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_open_task_arrives_however_many_there_are() {
    let (c, dir) = busy("open", 600, 5);
    let st = c.rpc("state", &json!({})).unwrap();
    assert_eq!(uids(&st, "open").len(), 600);
    assert_eq!(st["done_total"], 5);
    assert_eq!(
        uids(&st, "done").len(),
        5,
        "fewer than 200 finished: all of them"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_phone_is_told_the_same() {
    let (c, dir) = busy("phone", 2, 300);
    let st = c.rpc_public("state", &json!({})).unwrap();
    assert_eq!(uids(&st, "done").len(), 200);
    assert_eq!(st["done_total"], 300);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_finished_task_beyond_the_newest_200_is_still_kept_and_found_and_the_total_follows_a_finish() {
    let (c, dir) = busy("kept", 2, 250);
    let st = c.store().unwrap();
    let oldest = 3; // the oldest finished task
    assert!(st.get(oldest).is_some(), "it is still in the database");
    let found = c.rpc("search", &json!({"q": "zebra777"})).unwrap();
    assert!(
        found.as_array().unwrap().iter().any(|u| u == oldest),
        "and in the search: {found}"
    );
    let before = c.rpc("state", &json!({})).unwrap();
    assert!(
        !uids(&before, "done").contains(&(oldest as u64)),
        "but not among the 200 shown"
    );
    // finishing one more raises the total; reopening lowers it
    assert!(st.set_done(1, true).unwrap());
    assert_eq!(c.rpc("state", &json!({})).unwrap()["done_total"], 251);
    assert!(st.set_done(1, false).unwrap());
    assert_eq!(c.rpc("state", &json!({})).unwrap()["done_total"], 250);
    // a task you finish now is among the newest, so it can be undone from the window
    st.set_done(2, true).unwrap();
    assert!(uids(&c.rpc("state", &json!({})).unwrap(), "done").contains(&2u64));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_window_can_ask_for_more_finished_tasks_up_to_a_limit() {
    let (c, dir) = busy("more-done", 2, 1200);
    let n = |v: Value| uids(&v, "done").len();
    assert_eq!(
        n(c.rpc("state", &json!({})).unwrap()),
        200,
        "by default the newest 200"
    );
    assert_eq!(n(c.rpc("state", &json!({"done": 600})).unwrap()), 600);
    assert_eq!(
        n(c.rpc("state", &json!({"done": 50})).unwrap()),
        200,
        "never fewer than the default"
    );
    assert_eq!(
        n(c.rpc("state", &json!({"done": "lots"})).unwrap()),
        200,
        "something that is not a number is ignored"
    );
    assert_eq!(
        n(c.rpc("state", &json!({"done": 99999999})).unwrap()),
        1200,
        "all there are, when there are fewer than the limit"
    );
    // the phone can ask too, and gets the same
    assert_eq!(
        n(c.rpc_public("state", &json!({"done": 600})).unwrap()),
        600
    );
    let st = c.rpc("state", &json!({"done": 600})).unwrap();
    assert_eq!(
        st["done_total"], 1200,
        "and is still told how many there are"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn what_the_window_may_ask_for_is_bounded() {
    let (c, dir) = busy("bounded", 1, 30);
    // 20,000 finished tasks is the most one answer holds, whatever is asked
    assert_eq!(engine::core::DONE_MAX, 20_000);
    assert_eq!(
        c.rpc("state", &json!({"done": u64::MAX})).unwrap()["done_total"],
        30
    );
    let _ = std::fs::remove_dir_all(&dir);
}
