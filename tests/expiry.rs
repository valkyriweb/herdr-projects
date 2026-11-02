//! Expired worker instructions and status notes are shown, and never sent to a new worker.
use crate::common;
use common::*;
use serde_json::{json, Value};
use sw::json::*;

const EXPIRED: &str = "2026-10-19T16:32Z";
const NOW: &str = "2026-10-19T16:33:00Z";

fn at(value: &str) -> i64 { chrono::DateTime::parse_from_rfc3339(value).unwrap().timestamp_millis() }
fn args(words: &[&str]) -> sw::cli::flags::Args { sw::cli::flags::parse(&words.iter().map(|s| s.to_string()).collect::<Vec<_>>()) }
fn command(words: &[&str]) -> (Value, String) { sw::cli::tasks::command(words[0], &args(words)).unwrap() }
fn expire_task_instructions(id: &str, words: &str) -> (Value, String) {
    command(&["set", "worker.instructions", words, "--task", id, "--expires", EXPIRED])
}

#[test]
fn set_instruction_expiry_blocks_delegate_and_explains_the_wait_in_list_and_details() {
    let w = world();
    let id = w.add("Old worker slot");
    sw::clock::set(Some(at(NOW)));
    let (saved, text) = expire_task_instructions(&id, "Use the local test slot.");
    assert_eq!(saved.s("expires"), EXPIRED);
    assert!(text.contains(&format!("Expires: {EXPIRED}")), "{text}");
    let (shown, show_text) = command(&["show", &id]);
    let row = shown.arr("settings").iter().find(|r| r.s("name") == "worker.instructions").unwrap();
    assert_eq!(row.arr("expires").first().map(sw::json::text).as_deref(), Some(EXPIRED));
    assert!(show_text.contains(&format!("expires {EXPIRED}")), "{show_text}");
    let unstarted = command(&["list"]).0;
    let row = unstarted.as_array().unwrap().iter().find(|t| t.s("id") == id).unwrap();
    assert!(!row.s("msg").contains("renew or clear"), "an unstarted task is not already waiting: {row}");

    let (delegated, message) = command(&["delegate", &id]);
    let reason = format!("instructions expired {EXPIRED}");
    assert_eq!((delegated.s("stage"), delegated.s("state")), ("asks", "blocked"));
    assert!(message.contains(&format!("{reason}: renew or clear")), "{message}");
    assert!(starts(&w).is_empty(), "expired instructions started a worker: {:?}", starts(&w));

    let (listed, _) = command(&["list"]);
    let row = listed.as_array().unwrap().iter().find(|t| t.s("id") == id).unwrap();
    assert_eq!(row.s("msg"), format!("{reason}: renew or clear"));
    let (detail, _) = command(&["details", &id]);
    assert_eq!(detail.s("stale"), reason);

    command(&["set", "worker.instructions", "Use a fresh test slot.", "--task", &id, "--expires", "2026-10-19T17:00Z"]);
    let (renewed, message) = command(&["delegate", &id]);
    assert_ne!(renewed.s("stage"), "asks");
    assert!(!renewed.b("staleCause"));
    assert!(!message.contains("waiting:"), "{message}");
    w.script(json!({ "worker": [{ "note": "Used the renewed instructions." }] }));
    settle(&id);
    assert_eq!(starts(&w).len(), 1);
}

#[test]
fn add_with_delegate_reports_when_inherited_instructions_are_expired() {
    let w = world();
    sw::clock::set(Some(at(NOW)));
    command(&["set", "worker.instructions", "Use the old shared slot.", "--expires", EXPIRED]);
    std::env::set_current_dir(&w.root).unwrap();
    let (_, message) = command(&["add", "Expired inherited brief", "--delegate"]);
    assert!(message.contains(&format!("waiting: instructions expired {EXPIRED}: renew or clear")), "{message}");
    assert!(starts(&w).is_empty());
}

#[test]
fn delegate_can_set_the_task_instructions_and_expiry_together() {
    let w = world();
    let id = w.add("Expired delegate brief");
    sw::clock::set(Some(at(NOW)));
    let (delegated, message) = command(&["delegate", &id, "--worker.instructions", "Use the local test slot.", "--expires", EXPIRED]);
    let saved = task(&id);
    assert_eq!(saved.at("settings").s("worker.instructions"), "Use the local test slot.");
    assert_eq!(saved.at("settingsExpires").s("worker.instructions"), EXPIRED);
    assert_eq!(delegated.s("stage"), "asks");
    assert!(message.contains(&format!("instructions expired {EXPIRED}: renew or clear")), "{message}");
    assert!(starts(&w).is_empty());
}

#[test]
fn delegate_expiry_applies_to_inherited_instructions_and_keeps_dependencies_when_parked() {
    let w = world();
    let id = w.add("Expired inherited brief");
    let dependency = w.add("Waited-on task");
    sw::clock::set(Some(at(NOW)));
    command(&["set", "worker.instructions", "Use the inherited shared brief."]);

    let (parked, message) = command(&["delegate", &id, "--after", &dependency, "--expires", EXPIRED]);
    assert_eq!(parked.s("stage"), "asks");
    assert!(message.contains(&format!("instructions expired {EXPIRED}: renew or clear")), "{message}");
    assert!(task(&id).o("settings").map_or(true, |s| !s.contains_key("worker.instructions")));
    assert_eq!(task(&id).arr("after").len(), 1);

    command(&["delegate", &id, "--expires", "2026-10-19T17:00Z"]);
    let renewed = task(&id);
    assert_eq!(renewed.s("stage"), "after");
    assert_eq!(renewed.arr("after").len(), 1);
    assert_eq!(sw::store::settings::lookup::setting_text("worker.instructions", Some(&renewed)), "Use the inherited shared brief.");
    assert_eq!(renewed.at("settingsExpires").s("worker.instructions"), "2026-10-19T17:00Z");
    assert!(starts(&w).is_empty());

    sw::clock::set(Some(at("2026-10-19T17:00:00Z")));
    assert_eq!(sw::expiry::reason(&renewed).as_deref(), Some("instructions expired 2026-10-19T17:00Z"));
}

#[test]
fn an_expired_absolute_utc_window_in_instructions_blocks_delegate_without_guessing_clock_times() {
    let w = world();
    let id = w.add("Expired window");
    sw::clock::set(Some(at(NOW)));
    let t = task(&id);
    sw::store::settings::change::set_value("worker.instructions", &json!("Reserved 2026-10-19T16:27Z to 2026-10-19T16:32Z"), &sw::store::settings::lookup::Scope::task(&t)).unwrap();
    let (delegated, message) = command(&["delegate", &id]);
    assert_eq!(delegated.s("stage"), "asks");
    assert!(message.contains(&format!("instructions expired {EXPIRED}: renew or clear")), "{message}");
    assert!(starts(&w).is_empty());

    let id = w.add("Clock times are unclear");
    let t = task(&id);
    sw::store::settings::change::set_value("worker.instructions", &json!("Reserved 16:27-16:32Z"), &sw::store::settings::lookup::Scope::task(&t)).unwrap();
    assert!(sw::expiry::reason(&task(&id)).is_none());

    let id = w.add("A future explicit expiry wins");
    let t = task(&id);
    sw::store::settings::change::set_value_expiring(
        "worker.instructions",
        &json!("Reserved 2026-10-19T16:27Z to 2026-10-19T16:32Z"),
        &sw::store::settings::lookup::Scope::task(&t),
        Some(&json!("2026-10-19T17:00Z")),
    ).unwrap();
    assert!(sw::expiry::reason(&task(&id)).is_none());
}

#[test]
fn an_expired_status_note_waits_and_shows_its_reason_in_list_and_details() {
    let w = world();
    let id = w.add("Expired task note");
    sw::clock::set(Some(at(NOW)));
    command(&["report", &id, "--state", "blocked", "--kind", "question", "--msg", "The local test slot is reserved.", "--expires", EXPIRED]);
    command(&["report", &id, "--state", "blocked"]);
    assert!(!command(&["details", &id]).0.b("noteExpires"));
    command(&["report", &id, "--state", "blocked", "--msg", "The test slot moved."]);
    assert!(!command(&["details", &id]).0.b("noteExpires"));
    command(&["report", &id, "--state", "blocked", "--msg", "The local test slot is reserved.", "--expires", EXPIRED]);
    let (delegated, message) = command(&["delegate", &id]);
    let reason = format!("task note expired {EXPIRED}");
    assert_eq!((delegated.s("stage"), delegated.s("state")), ("asks", "blocked"));
    assert!(message.contains(&format!("{reason}: renew or clear")), "{message}");
    assert!(starts(&w).is_empty());
    let (listed, _) = command(&["list"]);
    let row = listed.as_array().unwrap().iter().find(|t| t.s("id") == id).unwrap();
    assert_eq!(row.s("msg"), format!("{reason}: renew or clear"));
    let (detail, _) = command(&["details", &id]);
    assert_eq!(detail.s("stale"), reason);
    assert_eq!(detail.s("noteExpires"), EXPIRED);

    command(&["report", &id, "--state", "blocked", "--msg", "The old note no longer applies.", "--expires", "none"]);
    let renewed = command(&["details", &id]).0;
    assert!(!renewed.b("noteExpires"));
    assert!(!renewed.b("stale"));
}

#[test]
fn a_measured_load_gate_is_reported_stale_and_blocks_delegate() {
    let w = world();
    let id = w.add("Load-gated status note");
    command(&["report", &id, "--state", "blocked", "--kind", "question", "--msg", "Local starts wait for high load.", "--stale-if", "load < 1000000000"]);
    let reason = sw::expiry::reason(&task(&id)).unwrap();
    assert!(reason.starts_with("stale: load now ") && reason.ends_with(" < 1000000000"), "{reason}");
    let (detail, _) = command(&["details", &id]);
    assert_eq!(detail.s("stale"), reason);
    assert!(detail.s("noteGate").contains("load now "), "{}", detail.s("noteGate"));
    let (delegated, message) = command(&["delegate", &id]);
    assert_eq!(delegated.s("stage"), "asks");
    assert!(message.contains(&format!("{reason}: renew or clear")), "{message}");
    assert!(starts(&w).is_empty());

    command(&["report", &id, "--state", "blocked", "--msg", "The load is now safe.", "--stale-if", "none"]);
    assert!(sw::expiry::reason(&task(&id)).is_none());
    assert!(!command(&["details", &id]).0.b("noteGate"));
}

#[test]
fn remote_delegation_carries_instruction_expiry_and_reports_the_wait() {
    let w = world();
    let id = w.add("Remote expired instructions");
    sw::clock::set(Some(at(NOW)));
    command(&["set", "worker.instructions", "Use the old local slot.", "--expires", EXPIRED]);
    let mine = sw::store::settings::places::yours();
    let repo = id.split('/').next().unwrap();
    let settings = sw::store::settings::lookup::to_send(repo, &mine);
    let expiries = sw::store::settings::lookup::to_send_expiries(repo, &mine);
    assert_eq!(expiries.at("worker.instructions").as_array().and_then(|a| a.first()).map(sw::json::text).as_deref(), Some(EXPIRED));

    let answer = sw::remote::door("do", &json!({ "action": "delegate", "id": id, "from": "sender", "settings": settings, "settingsExpires": expiries })).unwrap();
    assert!(answer.s("msg").contains(&format!("instructions expired {EXPIRED}: renew or clear")), "{}", answer.s("msg"));
    assert_eq!(task(&id).at("carried").at("settingsExpires").at("worker.instructions").as_array().and_then(|a| a.first()).map(sw::json::text).as_deref(), Some(EXPIRED));
    assert_eq!(task(&id).s("stage"), "asks");
    assert!(starts(&w).is_empty());
}

#[test]
fn retry_and_restart_park_tasks_with_expired_instructions_without_starting_or_prompting() {
    let w = world_with(Opts { repo: true, settings: json!({ "worker.agent": "pi" }), ..Default::default() });
    sw::clock::set(Some(at(NOW)));
    w.script(json!({ "worker": [{ "ends": "pi-unknown-provider", "hook": true, "exit": true }] }));
    let retry_id = w.add("Retry with expired instructions");
    delegate(&retry_id);
    let failed = settle(&retry_id);
    assert_eq!(failed.s("stage"), "failed");
    expire_task_instructions(&retry_id, "Use the old provider.");
    let prompt_count = prompts(&w, failed.at("link").s("pane")).len();
    let start_count = starts(&w).len();
    let (retried, message) = command(&["retry", &retry_id]);
    assert_eq!(retried.s("stage"), "asks");
    assert!(message.contains(&format!("instructions expired {EXPIRED}: renew or clear")), "{message}");
    assert_eq!(starts(&w).len(), start_count);
    assert_eq!(prompts(&w, failed.at("link").s("pane")).len(), prompt_count);

    w.script(json!({ "worker": [{ "ends": "pi-unknown-provider", "hook": true, "exit": true }] }));
    let restart_id = w.add("Restart with expired instructions");
    delegate(&restart_id);
    let stopped = settle(&restart_id);
    assert_eq!(stopped.s("stage"), "failed");
    expire_task_instructions(&restart_id, "Use the old provider.");
    let starts_before_restart = starts(&w).len();
    sw::delegate::jobs::restart_job(&restart_id);
    assert_eq!(task(&restart_id).s("stage"), "asks");
    assert_eq!(starts(&w).len(), starts_before_restart);
}
