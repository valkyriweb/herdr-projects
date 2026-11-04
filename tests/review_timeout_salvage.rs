//! What a timed-out review keeps, and what it never stands for: its whole folder survives the retry (screenshots too),
//! its text is capped and its credentials hidden, it is never kept as a passing review, and a retry on the commit a
//! shorter check passed on does not run that check again.
use crate::common;
use common::*;
use serde_json::{json, Value};
use sw::json::*;

fn verdict(findings: Value) -> Value { json!({ "verdict": { "meets": true, "text": "Looked at it.", "findings": findings } }) }

#[test]
fn the_whole_folder_survives_the_retry_and_its_text_is_capped_with_credentials_hidden() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 0, "review.timeout": "1s" }), ..Default::default() });
    let png = "\u{89}PNG\r\n\u{1a}\n\u{0}\u{0}\u{0}\rIHDR";
    w.script(json!({ "worker": [{ "files": { "f.txt": "v1\n" }, "commit": "v1", "note": "Did v1." }],
        "reviewer": [{ "status": "working", "evidence": { "shot.png": png, "big.log": "x".repeat(200_000), "notes.md": "F1 major\napi_key=sk-abcdefghijklmnopqrstuvwx\n" },
            "screen": "curl -H 'Authorization: Bearer abcdef0123456789abcdef'\nghp_abcdefghijklmnopqrstuvwxyz0123\nF2 minor" },
            verdict(json!([]))] }));
    let id = w.add("Slow review with a screenshot");
    delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "failed", "{}", t.at("failure"));
    let rv = t.at("review");
    let partial = rv.s("partial");
    // Capped: one file's start, never the whole of it, and all of it together within its bound.
    assert!(partial.len() < 30_000, "{}", partial.len());
    assert!(partial.contains("[… cut, 200000 bytes in all]"), "{partial}");
    // A screenshot is named, and kept as it was beside the review folder.
    assert!(partial.contains("## shot.png\n[not text,"), "{partial}");
    let folder = std::path::PathBuf::from(rv.s("folder"));
    assert!(folder.join("shot.png").is_file(), "{}", folder.display());
    // Credentials on the screen or in a file are hidden before the board keeps them.
    for secret in ["abcdef0123456789abcdef", "ghp_abcdefghijklmnopqrstuvwxyz0123", "sk-abcdefghijklmnopqrstuvwx"] { assert!(!partial.contains(secret), "{secret} in {partial}"); }
    assert!(partial.contains("Authorization: Bearer [hidden]") && partial.contains("F2 minor") && partial.contains("F1 major"), "{partial}");
    assert!(!sw::store::reviews::review_history(&t)[0].s("partial").contains("abcdef0123456789abcdef"));
    // The retry's reviewer starts in an empty folder; the timed-out one's screenshot is still there.
    sw::actions::finish_line::retry(&id).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval", "{} {}", t.s("msg"), t.at("failure"));
    assert!(folder.join("shot.png").is_file(), "the retry deleted the timed-out reviewer's screenshot");
    assert_eq!(sw::store::reviews::review_history(&t)[0].s("folder"), folder.display().to_string());
}

#[test]
fn a_timed_out_review_is_never_a_result_that_stands_for_the_work() {
    let mut t = Obj::new();
    t.put("stage", "approval");
    t.put("review", json!({ "error": "review timed out after 1s (review.timeout)", "timeout": { "stage": "review", "reason": "timeout" }, "partial": "F1", "head": "abc" }));
    // Shown and read as the task's review, but never kept as one that passed, and its words say it failed.
    assert!(sw::store::reviews::current_review(&t).is_some());
    assert!(sw::store::reviews::standing_review(&t).is_none());
    assert!(!sw::gate::passes(t.at("review")));
    assert!(sw::status_words::review_failed(&t));
    t.put("review", json!({ "pass": true, "findings": [] }));
    assert!(sw::store::reviews::standing_review(&t).is_some());
}

const CI: &str = ".github/workflows/ci.yml";
const FLOW: &str = "name: CI\non:\n  pull_request:\njobs:\n  build:\n    runs-on: x\n";

#[test]
fn a_retry_on_the_commit_the_shorter_check_passed_on_does_not_run_it_again() {
    let w = world_with(Opts { repo: true, origin: true, settings: json!({ "review.timeout": "1s" }), repo_settings: Some(json!({ "check": {
        "command": "echo full >> \"$HOME/checks.log\"",
        "again": "echo \"again $HERDR_PROJECTS_CHECKED\" >> \"$HOME/checks.log\"" } })), ..Default::default() });
    let finding = json!({ "severity": "critical", "subject": "It does not do it", "where": "f.txt", "proof": "ran it" });
    w.script(json!({ "worker": [
            { "files": { "f.txt": "1\n", CI: FLOW }, "commit": "v1", "note": "Did v1." },
            { "files": { "f.txt": "2\n" }, "commit": "v2", "note": "Did v2." }],
        "reviewer": [verdict(json!([finding])), { "status": "working", "screen": "F9 minor so far" }, verdict(json!([]))] }));
    let id = w.add("Fix it, slowly reviewed");
    delegate(&id);
    let mut t = settle(&id);
    for _ in 0..3 { if t.s("stage") != "failed" { t = settle(&id); } }
    assert_eq!(t.s("stage"), "failed", "{} {}", t.s("msg"), t.at("failure"));
    assert!(sw::store::reviews::timed_out(t.at("review")), "{}", t.at("review"));
    let ran = || read(&w.base.join("home/checks.log")).lines().map(String::from).collect::<Vec<_>>();
    let before = ran();
    assert_eq!(before.len(), 2, "{before:?}");
    assert!(before[1].starts_with("again "), "{before:?}");
    sw::actions::finish_line::retry(&id).unwrap();
    let mut t = settle(&id);
    for _ in 0..3 { if t.s("stage") != "approval" { t = settle(&id); } }
    assert_eq!(t.s("stage"), "approval", "{} {}", t.s("msg"), t.at("failure"));
    assert_eq!(ran(), before, "the shorter check ran again on the commit it passed on");
}
